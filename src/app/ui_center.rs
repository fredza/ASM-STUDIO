use eframe::egui::{self, RichText};

use crate::debugger::Flags;
use crate::explain;
use crate::i18n;
use crate::syntax;
use crate::syscall;

use super::{
    App, ACCENT, ACTION, CHANGED, FLAG_ON, FLAG_OFF, FALSE_COL, GUTTER,
    badge, card,
};

impl App {
    // ---------- Centre : onglets Éditeur / Désassemblage ----------

    /// Onglet « Éditeur » : nom de fichier, repère RIP, puis la zone de texte.
    ///
    /// Le sélecteur d'onglets a disparu — c'est la barre d'onglets de la zone
    /// d'ancrage qui joue ce rôle désormais. Restent les deux informations que
    /// l'élève lit sans quitter son code : quel fichier, et où en est le CPU.
    pub(super) fn editor_tab_ui(&mut self, ui: &mut egui::Ui) {
        let hdr = self.c_header();
        ui.horizontal(|ui| {
            let name = self.src_path.file_name().unwrap_or_default().to_string_lossy();
            let mark = if self.dirty { " ●" } else { "" };
            ui.label(RichText::new(format!("{name}{mark}")).color(hdr));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.rip_banner(ui);
            });
        });
        ui.separator();
        self.editor_ui(ui);
    }

    /// Repère « ▶ RIP : 0x… mnémonique opérandes », si un programme tourne.
    pub(super) fn rip_banner(&self, ui: &mut egui::Ui) {
        let Some(s) = self.snap() else { return };
        let Some(insn) = self.disasm.iter().find(|i| i.address == s.regs.rip) else { return };
        ui.horizontal(|ui| {
            ui.label(RichText::new("▶").color(ACTION));
            ui.label(
                RichText::new(format!("RIP : 0x{:X}", s.regs.rip))
                    .monospace()
                    .color(self.c_addr()),
            );
            ui.label(
                RichText::new(format!("{} {}", insn.mnemonic, insn.operands))
                    .monospace()
                    .color(self.c_mnemonic()),
            );
        });
    }

    /// Déplace la sélection du désassemblage d'une instruction (clavier).
    /// Sans sélection, part de l'instruction courante (RIP).
    pub(super) fn move_disasm_selection(&mut self, down: bool) {
        if self.disasm.is_empty() {
            return;
        }
        let cur = self
            .selected
            .or_else(|| self.view_rip())
            .and_then(|a| self.disasm.iter().position(|i| i.address == a));
        let next = match (cur, down) {
            (None, _) => 0,
            (Some(i), true) => (i + 1).min(self.disasm.len() - 1),
            (Some(i), false) => i.saturating_sub(1),
        };
        self.selected = Some(self.disasm[next].address);
        self.scroll_to_sel = Some(super::dock::Panel::Disasm);
    }

    pub(super) fn editor_ui(&mut self, ui: &mut egui::Ui) {
        // Ligne source courante (RIP) à surligner pendant le débogage.
        let hl = self.current_source_line();
        let dark = self.dark;

        // Coloration syntaxique NASM (retour à la ligne désactivé => aligné aux numéros).
        let mut layouter = |ui: &egui::Ui, text: &str, _wrap: f32| {
            ui.fonts(|f| f.layout_job(syntax::highlight(text, dark, hl)))
        };

        // Gouttière : numéros de ligne (▶ + accent sur la ligne courante).
        let line_count = self.source.matches('\n').count() + 1;
        let width = line_count.to_string().len();
        let gfont = egui::FontId::monospace(syntax::FONT_SIZE);
        let mut gutter_job = egui::text::LayoutJob::default();
        for i in 1..=line_count {
            if i > 1 {
                gutter_job.append("\n", 0.0, egui::TextFormat::default());
            }
            let is_cur = hl == Some(i - 1);
            let (marker, color) = if is_cur { ("▶", ACCENT) } else { (" ", GUTTER) };
            gutter_job.append(
                &format!("{marker}{i:>width$}"),
                0.0,
                egui::TextFormat {
                    font_id: gfont.clone(),
                    color,
                    ..Default::default()
                },
            );
        }

        // Largeur du contenu = ligne la plus longue (pour le scroll horizontal).
        let char_w = ui.fonts(|f| f.glyph_width(&gfont, 'M'));
        let max_cols = self.source.lines().map(|l| l.chars().count()).max().unwrap_or(0);
        let content_w = (max_cols as f32 + 2.0) * char_w;

        ui.horizontal_top(|ui| {
            // Gouttière : défilement vertical synchronisé, sans barre ni scroll direct.
            egui::ScrollArea::vertical()
                .id_salt("gutter_scroll")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .enable_scrolling(false)
                .auto_shrink([true, false])
                .vertical_scroll_offset(self.editor_scroll_y)
                .show(ui, |ui| {
                    let galley = ui.fonts(|f| f.layout_job(gutter_job));
                    ui.add(egui::Label::new(galley).selectable(false));
                });
            ui.separator();
            // Éditeur : défilement vertical + horizontal ; la gouttière reste fixe.
            let out = egui::ScrollArea::both()
                .id_salt("editor_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let out = egui::TextEdit::multiline(&mut self.source)
                        .id(super::editor_id())
                        .frame(false)
                        .code_editor()
                        .desired_width(content_w.max(ui.available_width()))
                        .desired_rows(28)
                        .lock_focus(true)
                        .layouter(&mut layouter)
                        .show(ui);
                    if out.response.changed() {
                        self.dirty = true;
                    }
                    // Position du curseur (Ln/Col) pour la barre d'état.
                    if let Some(range) = out.cursor_range {
                        let p = range.primary.pcursor;
                        self.editor_ln = p.paragraph + 1;
                        self.editor_col = p.offset + 1;
                    }
                });
            // Synchronise la gouttière sur le défilement vertical de l'éditeur.
            self.editor_scroll_y = out.state.offset.y;
        });
    }

    pub(super) fn disasm_ui(&mut self, ui: &mut egui::Ui) {
        if self.disasm.is_empty() {
            ui.label(i18n::tr3(self.lang,
                "Cliquez sur « Lancer » pour assembler, lier et exécuter votre programme.",
                "Click \"Run\" to assemble, link and execute your program.",
                "Haga clic en «Ejecutar» para ensamblar, enlazar y ejecutar su programa.",
            ));
            return;
        }
        let rip = self.view_rip();
        let scroll_here = self.take_scroll_request(super::dock::Panel::Disasm);
        let mut clicked: Option<u64> = None;
        egui::ScrollArea::vertical().id_salt("disasm_scroll").show(ui, |ui| {
            for insn in &self.disasm {
                let is_current = Some(insn.address) == rip;
                let is_selected = Some(insn.address) == self.selected;
                // Forme de fond réservée AVANT le contenu => dessinée derrière le
                // texte (sinon le rectangle masquerait l'instruction).
                let bg = ui.painter().add(egui::Shape::Noop);
                let inner = ui.horizontal(|ui| {
                    if is_current {
                        ui.label(RichText::new("➤").color(CHANGED));
                    } else {
                        ui.label("    ");
                    }
                    ui.label(RichText::new(format!("0x{:08X}", insn.address)).monospace().color(self.c_addr()));
                    ui.label(RichText::new(format!("{:<20}", insn.bytes_hex())).monospace().color(self.c_bytes()));
                    ui.label(RichText::new(format!("{:<7}", insn.mnemonic)).monospace().color(self.c_mnemonic()));
                    ui.label(RichText::new(&insn.operands).monospace());
                });
                let row = inner.response.interact(egui::Sense::click());
                if row.clicked() {
                    clicked = Some(insn.address);
                }
                // La sélection déplacée au clavier doit rester visible.
                if is_selected && scroll_here {
                    row.scroll_to_me(Some(egui::Align::Center));
                }
                let fill = if is_current {
                    Some(self.c_rip_row())
                } else if is_selected {
                    Some(self.c_sel_row())
                } else if row.hovered() {
                    Some(self.c_sel_row().linear_multiply(0.5))
                } else {
                    None
                };
                if let Some(color) = fill {
                    let rect = row.rect.expand2(egui::vec2(0.0, 2.0));
                    ui.painter().set(bg, egui::Shape::rect_filled(rect, 3.0, color));
                }
            }
        });
        if let Some(addr) = clicked {
            self.selected = if self.selected == Some(addr) { None } else { Some(addr) };
        }
    }

    // ---------- Panneau INSTRUCTION ----------

    pub(super) fn instruction_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);

        // FLAGS n'est plus épinglé ici : c'est un panneau ancrable à part
        // entière, et le garder aussi en bas de INSTRUCTION affichait les mêmes
        // six drapeaux deux fois à l'écran.
        let target = self.selected.or_else(|| self.view_rip());
        let Some(addr) = target else {
            ui.label(tr(
                "Lancez le programme, puis cliquez une instruction.",
                "Run the program, then click an instruction.",
                "Ejecute el programa y luego haga clic en una instrucción.",
            ));
            return;
        };
        let Some(insn) = self.disasm.iter().find(|i| i.address == addr).cloned() else {
            ui.label("—");
            return;
        };
        let flags = self.snap().map(|s| Flags::from_eflags(s.regs.eflags)).unwrap_or_default();
        let e = explain::explain(&insn.mnemonic, &insn.operands, flags, self.lang);
        let mnem_col = self.c_mnemonic();

        // Ligne 1 : nom de l'instruction + bouton Microscope (aligné à droite).
        ui.horizontal(|ui| {
            ui.label(RichText::new(&e.title).size(16.0).strong().color(mnem_col));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(tr("🔬 Microscope", "🔬 Microscope", "🔬 Microscopio"))
                    .on_hover_text(tr("Tout voir sur cette seule instruction", "See everything about this one instruction", "Ver todo sobre esta instrucción"))
                    .clicked()
                {
                    self.microscope = Some(addr);
                }
            });
        });
        // Ligne 2 : catégorie + repère (instruction courante / sélection) à droite.
        ui.horizontal(|ui| {
            ui.label(RichText::new(e.category).italics().weak().size(12.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let tag = if self.selected.is_some() {
                    tr("(sélection)", "(selection)", "(selección)")
                } else {
                    tr("(instruction courante)", "(current instruction)", "(instrucción actual)")
                };
                ui.label(RichText::new(tag).small().weak());
            });
        });

        // Pastille syscall : numéro RAX + nom, sur une ligne compacte.
        if insn.mnemonic == "syscall"
            && let Some(snap) = self.snap()
        {
            let rax = snap.regs.rax;
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                badge(ui, &format!("#{rax}"), self.c_bytes());
                ui.label(
                    RichText::new(syscall::name(rax))
                        .monospace()
                        .strong()
                        .color(self.c_mnemonic()),
                );
            });
        }

        // Étape de cadre d'appel : replace l'instruction dans la mécanique du
        // prologue/épilogue, que le panneau CALL STACK ne raconte pas.
        if let Some(phase) = crate::abi::frame_phase(&insn.mnemonic, &insn.operands) {
            ui.add_space(6.0);
            egui::Frame::default()
                .fill(ACTION.linear_multiply(0.12))
                .stroke(egui::Stroke::new(1.0_f32, ACTION.linear_multiply(0.6)))
                .rounding(egui::Rounding::same(5.0))
                .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(tr("🧱 Cadre d'appel", "🧱 Call frame", "🧱 Marco de llamada"))
                            .small()
                            .strong()
                            .color(ACTION),
                    );
                    ui.add_space(2.0);
                    ui.label(RichText::new(phase.explain(self.lang)).size(12.5));
                });
        }

        // Registres d'arguments d'un appel de fonction : sur un « call », c'est
        // là que l'ordre System V prend son sens.
        if insn.mnemonic == "call"
            && let Some(snap) = self.snap()
        {
            let hdr2 = self.c_header();
            let regs = snap.regs.clone();
            ui.add_space(6.0);
            card(ui, |ui| {
                ui.label(
                    RichText::new(tr("Arguments (ABI System V)", "Arguments (System V ABI)", "Argumentos (ABI System V)"))
                        .small()
                        .strong()
                        .color(hdr2),
                );
                egui::Grid::new("abi_call_args").num_columns(3).spacing([8.0, 2.0]).show(ui, |ui| {
                    for (i, rn) in crate::abi::ARG_REGS.iter().enumerate() {
                        let v = regs.named().iter().find(|(n, _)| n == rn).map(|(_, v)| *v).unwrap_or(0);
                        ui.label(RichText::new(format!("arg{}", i + 1)).small().weak());
                        ui.label(RichText::new(*rn).monospace().small().color(hdr2));
                        ui.label(RichText::new(format!("0x{v:X}")).monospace().small());
                        ui.end_row();
                    }
                });
                ui.label(
                    RichText::new(tr(
                        "La valeur de retour reviendra dans RAX. RBX, R12–R15 seront intacts ; \
                         les autres peuvent être écrasés.",
                        "The return value will come back in RAX. RBX, R12–R15 will be intact; \
                         the others may be clobbered.",
                        "El valor de retorno volverá en RAX. RBX, R12–R15 quedarán intactos; \
                         los demás pueden ser sobrescritos.",
                    ))
                    .small()
                    .weak(),
                );
            });
        }

        // Registres d'arguments pour un appel système : donne le sens des
        // valeurs plutôt que de laisser l'élève deviner l'ordre.
        if insn.mnemonic == "syscall"
            && let Some(snap) = self.snap()
        {
            let hdr2 = self.c_header();
            let regs = snap.regs.clone();
            ui.add_space(6.0);
            card(ui, |ui| {
                ui.label(
                    RichText::new(tr("Arguments (ABI syscall)", "Arguments (syscall ABI)", "Argumentos (ABI syscall)"))
                        .small()
                        .strong()
                        .color(hdr2),
                );
                egui::Grid::new("abi_syscall_args").num_columns(3).spacing([8.0, 2.0]).show(ui, |ui| {
                    for (i, rn) in crate::abi::SYSCALL_ARG_REGS.iter().enumerate() {
                        let v = regs.named().iter().find(|(n, _)| n == rn).map(|(_, v)| *v).unwrap_or(0);
                        ui.label(RichText::new(format!("arg{}", i + 1)).small().weak());
                        ui.label(RichText::new(*rn).monospace().small().color(hdr2));
                        ui.label(RichText::new(format!("0x{v:X}")).monospace().small());
                        ui.end_row();
                    }
                });
                ui.label(
                    RichText::new(tr(
                        "syscall écrase RCX et R11 : le 4e argument passe par R10.",
                        "syscall clobbers RCX and R11: the 4th argument goes through R10.",
                        "syscall sobrescribe RCX y R11: el 4.º argumento pasa por R10.",
                    ))
                    .small()
                    .weak(),
                );
            });
        }

        // Description pédagogique + lien vers la référence officielle.
        ui.add_space(8.0);
        card(ui, |ui| {
            ui.label(RichText::new(&e.description).size(13.0));
        });
        ui.add_space(6.0);
        ui.hyperlink_to(
            format!("📖 {} {} ↗", tr("Référence Intel de", "Intel reference for", "Referencia Intel de"), insn.mnemonic.to_uppercase()),
            explain::doc_url(&insn.mnemonic),
        )
        .on_hover_text(tr(
            "Ouvre la page de l'instruction (manuel Intel SDM, felixcloutier.com)",
            "Opens the instruction page (Intel SDM manual, felixcloutier.com)",
            "Abre la página de la instrucción (manual Intel SDM, felixcloutier.com)",
        ));

        if let Some(cond) = &e.condition {
            ui.add_space(4.0);
            ui.label(RichText::new(tr("Condition", "Condition", "Condición")).strong());
            ui.label(RichText::new(cond).monospace());
            // Effet : où mène le saut si la condition est vraie.
            if !insn.operands.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(tr("Effet", "Effect", "Efecto")).strong());
                ui.label(
                    RichText::new(format!(
                        "{} {}.",
                        tr("Si la condition est vraie, RIP =", "If the condition is true, RIP =", "Si la condición es verdadera, RIP ="),
                        insn.operands
                    ))
                    .monospace(),
                );
            }
            ui.add_space(4.0);
            let hdr2 = self.c_header();
            card(ui, |ui| {
                    ui.label(RichText::new(tr("État actuel", "Current state", "Estado actual")).small().strong().color(hdr2));
                    ui.horizontal(|ui| {
                        for (name, val) in &e.relevant_flags {
                            let c = if *val { FLAG_ON } else { FLAG_OFF };
                            ui.label(
                                RichText::new(format!("{name} = {}", *val as u8))
                                    .monospace()
                                    .color(c),
                            );
                        }
                    });
                    if let Some(taken) = e.taken {
                        ui.add_space(4.0);
                        let (txt, col) = if taken {
                            (tr("✔ Condition vraie — le saut sera pris.", "✔ Condition true — the jump will be taken.", "✔ Condición verdadera — el salto se tomará."), FLAG_ON)
                        } else {
                            (tr("✘ Condition fausse — pas de saut.", "✘ Condition false — no jump.", "✘ Condición falsa — sin salto."), FALSE_COL)
                        };
                        let fill = if taken {
                            FLAG_ON.linear_multiply(0.12)
                        } else {
                            FALSE_COL.linear_multiply(0.12)
                        };
                        egui::Frame::default()
                            .fill(fill)
                            .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                            .rounding(egui::Rounding::same(4.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new(txt).color(col).strong());
                            });
                    }
                });
        }
        if !e.affects_flags.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(tr("Flags positionnés", "Flags set", "Flags activos")).strong());
            ui.label(RichText::new(e.affects_flags.join("  ")).monospace().color(CHANGED));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::dock::Panel;
    use crate::disasm::Insn;

    fn insn(addr: u64) -> Insn {
        Insn {
            address: addr,
            bytes: vec![0x90],
            mnemonic: "nop".into(),
            operands: String::new(),
        }
    }

    /// Ctrl+Tab fait défiler les onglets DU NŒUD qui a le focus. Sans nœud
    /// focalisé, l'appel doit rester sans effet plutôt que de paniquer.
    #[test]
    fn tab_cycle_is_safe_without_focus() {
        let mut app = App::new();
        app.set_ui_mode(crate::app::UiMode::Full);
        app.cycle_tab(false);
        app.cycle_tab(true);
        // La disposition reste intacte.
        assert!(app.panel_is_open(Panel::Editor));
        assert!(app.panel_is_open(Panel::Disasm));
    }

    /// Après focalisation d'un panneau, le cycle change bien l'onglet actif du
    /// nœud — Éditeur / Désassemblage / Vue mémoire partagent le même nœud.
    #[test]
    fn tab_cycle_moves_within_the_focused_node() {
        let mut app = App::new();
        // L'éditeur ne partage un nœud avec le désassemblage qu'en mode complet.
        app.set_ui_mode(crate::app::UiMode::Full);
        app.focus_panel(Panel::Editor);
        assert_eq!(app.focused_panel(), Some(Panel::Editor));

        app.cycle_tab(false);
        let after = app.focused_panel();
        assert_ne!(after, Some(Panel::Editor), "le cycle doit changer d'onglet");
        assert!(
            matches!(after, Some(Panel::Disasm) | Some(Panel::MemMap)),
            "on reste dans le nœud du centre, obtenu : {after:?}"
        );

        // Un tour complet ramène à l'éditeur.
        app.cycle_tab(false);
        app.cycle_tab(false);
        assert_eq!(app.focused_panel(), Some(Panel::Editor));
    }

    /// Les flèches parcourent le désassemblage et s'arrêtent aux bornes.
    #[test]
    fn disasm_selection_moves_and_clamps() {
        let mut app = App::new();
        app.disasm = vec![insn(0x10), insn(0x20), insn(0x30)];

        app.move_disasm_selection(true);
        assert_eq!(app.selected, Some(0x10));
        app.move_disasm_selection(true);
        assert_eq!(app.selected, Some(0x20));
        app.move_disasm_selection(true);
        assert_eq!(app.selected, Some(0x30));
        app.move_disasm_selection(true);
        assert_eq!(app.selected, Some(0x30), "borne basse respectée");

        app.move_disasm_selection(false);
        assert_eq!(app.selected, Some(0x20));
        app.move_disasm_selection(false);
        assert_eq!(app.selected, Some(0x10));
        app.move_disasm_selection(false);
        assert_eq!(app.selected, Some(0x10), "borne haute respectée");
    }

    #[test]
    fn disasm_selection_is_safe_when_empty() {
        let mut app = App::new();
        app.disasm.clear();
        app.move_disasm_selection(true);
        assert_eq!(app.selected, None);
    }
}
