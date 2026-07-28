use eframe::egui::{self, RichText};

use crate::debugger::Flags;
use crate::explain;
use crate::i18n;
use crate::syntax;
use crate::syscall;

use super::{
    App, ACCENT, ACTION, CHANGED, FLAG_ON, FLAG_OFF, FALSE_COL, GUTTER,
    badge, card, panel_header, icon_tab, icon_img,
};

impl App {
    // ---------- Centre : onglets Éditeur / Désassemblage ----------

    pub(super) fn center_ui(&mut self, ui: &mut egui::Ui) {
        let hdr = self.c_header();
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str| i18n::tr(lang, fr, en);
        let (edit_ic, disasm_ic) = match &self.icons {
            Some(i) => (Some(i.editor.clone()), Some(i.assembler.clone())),
            None => (None, None),
        };
        panel_header(ui, |ui| {
            if icon_tab(ui, edit_ic.as_ref(), tr("Éditeur", "Editor"), self.tab == super::Tab::Editor).clicked() {
                self.tab = super::Tab::Editor;
            }
            if icon_tab(ui, disasm_ic.as_ref(), tr("Désassemblage", "Disassembly"), self.tab == super::Tab::Disasm).clicked() {
                self.tab = super::Tab::Disasm;
            }
            ui.separator();
            let name = self.src_path.file_name().unwrap_or_default().to_string_lossy();
            let mark = if self.dirty { " ●" } else { "" };
            ui.label(RichText::new(format!("{name}{mark}")).color(hdr));
        });
        // Bandeau RIP (façon mockup) : « RIP : 0x… mnémonique opérandes ».
        if let Some(s) = self.snap()
            && let Some(insn) = self.disasm.iter().find(|i| i.address == s.regs.rip)
        {
            ui.horizontal(|ui| {
                ui.label(RichText::new("▶").color(ACTION));
                ui.label(RichText::new(format!("RIP : 0x{:X}", s.regs.rip)).monospace().color(self.c_addr()));
                ui.label(
                    RichText::new(format!("{} {}", insn.mnemonic, insn.operands))
                        .monospace()
                        .color(self.c_mnemonic()),
                );
            });
        }
        ui.add_space(2.0);
        match self.tab {
            super::Tab::Editor => self.editor_ui(ui),
            super::Tab::Disasm => self.disasm_ui(ui),
        }
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
            ui.label(i18n::tr(self.lang,
                "Cliquez sur « Lancer » pour assembler, lier et exécuter votre programme.",
                "Click \"Run\" to assemble, link and execute your program.",
            ));
            return;
        }
        let rip = self.view_rip();
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
        let bulb_ic = self.icons.as_ref().map(|i| i.instruction.clone());
        let hdr = self.c_header();
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str| i18n::tr(lang, fr, en);
        panel_header(ui, |ui| {
            super::header_title(ui, hdr, None, "INSTRUCTION");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                icon_img(ui, bulb_ic.as_ref(), 16.0);
            });
        });

        // FLAGS épinglé au bas du panneau INSTRUCTION (le cadre par défaut du
        // panneau dessine le trait de séparation avec le contenu au-dessus).
        egui::TopBottomPanel::bottom("instr_flags")
            .resizable(false)
            .show_inside(ui, |ui| self.flags_ui(ui));

        let target = self.selected.or_else(|| self.view_rip());
        let Some(addr) = target else {
            ui.label(tr(
                "Lancez le programme, puis cliquez une instruction.",
                "Run the program, then click an instruction.",
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
                    .button(tr("🔬 Microscope", "🔬 Microscope"))
                    .on_hover_text(tr("Tout voir sur cette seule instruction", "See everything about this one instruction"))
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
                    tr("(sélection)", "(selection)")
                } else {
                    tr("(instruction courante)", "(current instruction)")
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

        // Description pédagogique + lien vers la référence officielle.
        ui.add_space(8.0);
        card(ui, |ui| {
            ui.label(RichText::new(&e.description).size(13.0));
        });
        ui.add_space(6.0);
        ui.hyperlink_to(
            format!("📖 {} {} ↗", tr("Référence Intel de", "Intel reference for"), insn.mnemonic.to_uppercase()),
            explain::doc_url(&insn.mnemonic),
        )
        .on_hover_text(tr(
            "Ouvre la page de l'instruction (manuel Intel SDM, felixcloutier.com)",
            "Opens the instruction page (Intel SDM manual, felixcloutier.com)",
        ));

        if let Some(cond) = &e.condition {
            ui.add_space(4.0);
            ui.label(RichText::new(tr("Condition", "Condition")).strong());
            ui.label(RichText::new(cond).monospace());
            // Effet : où mène le saut si la condition est vraie.
            if !insn.operands.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(tr("Effet", "Effect")).strong());
                ui.label(
                    RichText::new(format!(
                        "{} {}.",
                        tr("Si la condition est vraie, RIP =", "If the condition is true, RIP ="),
                        insn.operands
                    ))
                    .monospace(),
                );
            }
            ui.add_space(4.0);
            let hdr2 = self.c_header();
            card(ui, |ui| {
                    ui.label(RichText::new(tr("État actuel", "Current state")).small().strong().color(hdr2));
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
                            (tr("✔ Condition vraie — le saut sera pris.", "✔ Condition true — the jump will be taken."), FLAG_ON)
                        } else {
                            (tr("✘ Condition fausse — pas de saut.", "✘ Condition false — no jump."), FALSE_COL)
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
            ui.label(RichText::new(tr("Flags positionnés", "Flags set")).strong());
            ui.label(RichText::new(e.affects_flags.join("  ")).monospace().color(CHANGED));
        }
    }
}
