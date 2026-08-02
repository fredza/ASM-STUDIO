use eframe::egui::{self, Color32, RichText};

use crate::debugger::Flags;
use crate::i18n;

use super::{
    App, ACTION, CHANGED, FLAG_ON, FLAG_OFF, FALSE_COL, FLASH_BRIGHT,
    PUSH_COL, POP_COL,
    changed_color, changed_color2, lerp_color,
    panel_header, icon_img, icon_tab,
    hex_dump_rows, parse_hex, parse_hex_bytes,
    dir_tree,
};
use super::pedagogy::bit_diff_strip;


impl App {
    // ---------- Bande basse ----------

    /// Timeline en colonne (bande basse), style mockup.
    pub(super) fn timeline_col_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let Some(last) = self.dbg.as_ref().map(|d| d.history.len() - 1) else {
            ui.weak(tr("— lancez un programme", "— run a program", "— inicia un programa"));
            return;
        };
        // Pastilles numérotées (façon mockup) si peu d'étapes ; sinon slider.
        let mut goto: Option<usize> = None;
        if last <= 60 {
            egui::ScrollArea::horizontal().id_salt("timeline_dots").max_height(30.0).show(ui, |ui| {
                ui.horizontal(|ui| {
                    for i in 0..=last {
                        let cur = i == self.view_index;
                        let txt = RichText::new(format!("{i}")).monospace().size(11.0).color(
                            if cur { Color32::WHITE } else { self.c_header() },
                        );
                        let mut btn = egui::Button::new(txt).min_size(egui::vec2(22.0, 22.0)).corner_radius(egui::CornerRadius::same(11));
                        if cur {
                            btn = btn.fill(ACTION);
                        }
                        if ui.add(btn).clicked() {
                            goto = Some(i);
                        }
                    }
                });
            });
        } else {
            let mut idx = self.view_index;
            ui.spacing_mut().slider_width = (ui.available_width() - 16.0).max(80.0);
            if ui.add(egui::Slider::new(&mut idx, 0..=last).show_value(false)).changed() {
                goto = Some(idx);
            }
        }
        if let Some(i) = goto {
            self.set_view(i as i64);
        }

        // Étape courante : « Instruction N/last : mnémonique ».
        if let Some(s) = self.snap()
            && let Some(insn) = self.disasm.iter().find(|i| i.address == s.regs.rip)
        {
            // Libellé d'étape : il doit rester lisible même en colonne étroite,
            // d'où le repli sur plusieurs lignes plutôt qu'une troncature.
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "{} {}/{last}",
                        tr("Étape", "Step", "Paso"),
                        self.view_index
                    ))
                    .strong()
                    .color(CHANGED),
                )
                .wrap(),
            );
            ui.label(
                RichText::new(format!("{} {}", insn.mnemonic, insn.operands))
                    .monospace()
                    .color(self.c_mnemonic()),
            );
        }

        // Contrôles de lecture (⏮ ⏪ ▶ ⏩ ⏭).
        ui.horizontal(|ui| {
            if self.tip(ui.button("⏮"), tr("Début (Home)", "Start (Home)", "Inicio (Home)")).clicked() {
                self.set_view(0);
            }
            if self.tip(ui.button("⏪"), tr("Précédent (←)", "Previous (←)", "Anterior (←)")).clicked() {
                self.set_view(self.view_index as i64 - 1);
            }
            if self.tip(ui.button("▶"), tr("Suivant (→)", "Next (→)", "Siguiente (→)")).clicked() {
                self.set_view(self.view_index as i64 + 1);
            }
            if self.tip(ui.button("⏩"), tr("Suivant (→)", "Next (→)", "Siguiente (→)")).clicked() {
                self.set_view(self.view_index as i64 + 1);
            }
            if self.tip(ui.button("⏭"), tr("Fin (End)", "End (End)", "Fin (Fin)")).clicked() {
                self.set_view(i64::MAX);
            }
        });
        if !self.is_head_view()
            && self.tip(ui.button(tr("⟳ Reprendre ici", "⟳ Resume here", "⟳ Reanudar aquí")), tr("Ré-exécute jusqu'à cette étape", "Re-run up to this step", "Volver a ejecutar hasta este paso")).clicked()
        {
            self.resume_here();
        }
    }

    /// Déplace la fenêtre du vidage mémoire d'une ligne (16 octets).
    ///
    /// `rows` permet un saut de page. L'adresse est bornée à 0 par le bas : un
    /// décalage négatif reboucherait vers les adresses hautes, ce qui n'a aucun
    /// sens pour l'élève.
    pub(super) fn scroll_memory(&mut self, down: bool, rows: u64) {
        let delta = rows.saturating_mul(16);
        self.mem_addr = if down {
            self.mem_addr.saturating_add(delta)
        } else {
            self.mem_addr.saturating_sub(delta)
        };
        self.mem_input = format!("0x{:X}", self.mem_addr);
    }

    pub(super) fn memory_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        // Régions utiles pour le sélecteur (calculées avant l'UI, sans emprunt).
        let regions: Vec<(&str, u64)> = match self.dbg.as_ref().filter(|d| d.is_alive()) {
            Some(d) => {
                let mut v = vec![(tr("Pile (RSP)", "Stack (RSP)", "Pila (RSP)"), d.regs().rsp), (tr("Base (RBP)", "Base (RBP)", "Base (RBP)"), d.regs().rbp)];
                if let Some((h0, _)) = d.heap_range() {
                    v.push((tr("Tas (heap)", "Heap", "Montículo (heap)"), h0));
                }
                v
            }
            None => Vec::new(),
        };
        let mut pick: Option<u64> = None;
        let mem_ic = self.icons.as_ref().map(|i| i.memory.clone());
        panel_header(ui, |ui| {
            icon_img(ui, mem_ic.as_ref(), 15.0);
            // Sélecteur de région (façon mockup).
            egui::ComboBox::from_id_salt("mem_region")
                .selected_text(RichText::new(format!("0x{:012X}..", self.mem_addr)).monospace())
                .show_ui(ui, |ui| {
                    for (label, addr) in &regions {
                        if ui.selectable_label(false, format!("{label}  ·  0x{addr:X}")).clicked() {
                            pick = Some(*addr);
                        }
                    }
                    if regions.is_empty() {
                        ui.weak(tr("(lancez un programme)", "(run a program)", "(inicia un programa)"));
                    }
                });
            ui.label(tr("aller @", "go to @", "ir a @"));
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.mem_input)
                    .id(egui::Id::new("kb_mem_goto"))
                    .desired_width(130.0)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("0x402000"),
            );
            let can_mem = self.can_read_memory();
            let go = ui.add_enabled(can_mem, egui::Button::new(tr("Aller", "Go", "Ir"))).clicked()
                || (can_mem && resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if go {
                match parse_hex(&self.mem_input) {
                    Some(a) => {
                        self.mem_addr = a;
                        self.status = format!("{} 0x{a:X}", tr("Mémoire @", "Memory @", "Memoria @"));
                    }
                    None => self.status = tr("Adresse hexa invalide", "Invalid hex address", "Dirección hexadecimal inválida").to_string(),
                }
            }
        });
        if let Some(a) = pick {
            self.mem_addr = a;
            self.mem_input = format!("0x{a:X}");
        }
        if !self.can_read_memory() {
            let msg = match self.dbg.as_ref().map(|d| d.is_alive()) {
                Some(false) => tr(
                    "Programme terminé — relancez pour explorer la mémoire.",
                    "Program finished — relaunch to explore memory.",
                    "Programa terminado — reinícielo para explorar la memoria.",
                ),
                Some(true) => tr(
                    "Revenez à la dernière étape de la timeline pour lire la mémoire.",
                    "Go back to the last timeline step to read memory.",
                    "Vuelva al último paso de la línea de tiempo para leer la memoria.",
                ),
                None => tr("Lancez un programme pour explorer la mémoire.", "Run a program to explore memory.", "Inicie un programa para explorar la memoria."),
            };
            ui.weak(msg);
            return;
        }

        // Laboratoire mémoire : écrire des octets à l'adresse de base affichée.
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("✎ écrire @ base :", "✎ write @ base:", "✎ escribir @ base:")).small());
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.mem_poke)
                    .id(egui::Id::new("kb_mem_poke"))
                    .desired_width(150.0)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("48 65 6C…"),
            );
            let write = ui.button(tr("Écrire", "Write", "Escribir")).clicked()
                || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if write {
                match parse_hex_bytes(&self.mem_poke) {
                    Some(bytes) if !bytes.is_empty() => {
                        let addr = self.mem_addr;
                        match self.dbg.as_mut().unwrap().write_mem(addr, &bytes) {
                            Ok(_) => {
                                self.status = format!("{} {} 0x{addr:X}", bytes.len(), tr("octet(s) écrit(s) @", "byte(s) written @", "byte(s) escrito(s) @"));
                                self.mem_poke.clear();
                            }
                            Err(e) => self.log(&e),
                        }
                    }
                    _ => self.status = tr("Octets hexa invalides (ex. « 48 65 6C »)", "Invalid hex bytes (e.g. \"48 65 6C\")", "Bytes hexadecimales inválidos (ej. «48 65 6C»)").to_string(),
                }
            }
        });
        ui.separator();

        // Explication du petit-boutisme, repliée : elle éclaire le vidage juste
        // en dessous sans encombrer ceux qui n'en ont pas besoin.
        self.endianness_ui(ui);
        ui.add_space(2.0);

        let (addr_c, bytes_c) = (self.c_addr(), self.c_bytes());
        let dbg = self.dbg.as_ref().unwrap();
        egui::ScrollArea::both()
            .id_salt("mem_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| hex_dump_rows(ui, addr_c, bytes_c, dbg, self.mem_addr, 8));
    }

    pub(super) fn console_ui(&mut self, ui: &mut egui::Ui) {
        let console_ic = self.icons.as_ref().map(|i| i.console.clone());
        let clear = i18n::tr3(self.lang, "Effacer la console", "Clear the console", "Borrar la consola");
        panel_header(ui, |ui| {
            icon_img(ui, console_ic.as_ref(), 15.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Petite corbeille rouge plutôt qu'un mot : l'action est
                // universelle, et le libellé passe en infobulle.
                let btn = egui::Button::new(RichText::new("🗑").size(15.0).color(FALSE_COL))
                    .frame(false);
                let resp = ui.add(btn).on_hover_text(clear);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    self.console.clear();
                }
            });
        });
        egui::ScrollArea::vertical()
            .id_salt("console_scroll")
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(RichText::new(&self.console).monospace())
                        .selectable(true)
                        .wrap(),
                );
            });
    }

    // ---------- Registres + Flags ----------

    /// Registres montrés en mode apprentissage : les huit généraux hérités du
    /// 8086 plus RIP. R8–R15 et EFLAGS brut viennent plus tard — EFLAGS est de
    /// toute façon décodé par le panneau FLAGS.
    const LEARNING_REGS: [&'static str; 9] = [
        "RAX", "RBX", "RCX", "RDX", "RSI", "RDI", "RBP", "RSP", "RIP",
    ];

    /// (nom, valeur, valeur précédente) des registres à afficher, filtrés selon
    /// le mode. Source unique : la navigation clavier et la vue mémoire s'y
    /// réfèrent aussi, donc les index restent cohérents.
    pub(super) fn reg_rows(&self) -> Option<Vec<(&'static str, u64, u64)>> {
        let snap = self.snap()?;
        let prev = self.prev_snap()?;
        let learning = self.mode == super::UiMode::Learning;
        Some(
            snap.regs
                .named()
                .iter()
                .zip(prev.regs.named())
                .filter(|((n, _), _)| !learning || Self::LEARNING_REGS.contains(n))
                .map(|((n, v), (_, p))| (*n, *v, p))
                .collect(),
        )
    }

    /// Déplace la sélection clavier dans le panneau des registres.
    pub(super) fn move_reg_selection(&mut self, down: bool) {
        let n = self.reg_rows().map_or(0, |r| r.len());
        if n == 0 {
            return;
        }
        // ↑/↓ sautent une ligne entière (autant de registres qu'il y a de
        // colonnes affichées), ce qui suit ce que l'œil voit plutôt que l'ordre
        // de déclaration.
        let step = self.reg_cols.max(1);
        self.reg_sel = if down {
            (self.reg_sel + step).min(n - 1)
        } else {
            self.reg_sel.saturating_sub(step)
        };
        self.scroll_to_sel = Some(super::dock::Panel::Registers);
    }

    /// Déplace la sélection d'un registre (←/→, dans la ligne).
    pub(super) fn move_reg_selection_sideways(&mut self, right: bool) {
        let n = self.reg_rows().map_or(0, |r| r.len());
        if n == 0 {
            return;
        }
        self.reg_sel = if right {
            (self.reg_sel + 1).min(n - 1)
        } else {
            self.reg_sel.saturating_sub(1)
        };
        self.scroll_to_sel = Some(super::dock::Panel::Registers);
    }

    /// Ouvre l'édition du registre retenu (Entrée dans le panneau REGISTERS).
    pub(super) fn edit_selected_register(&mut self) {
        if !self.can_step() {
            return;
        }
        let Some(rows) = self.reg_rows() else { return };
        let Some(&(name, val, _)) = rows.get(self.reg_sel) else { return };
        self.edit_reg = Some(name);
        self.edit_buf = format!("{val:X}");
        self.edit_focus = true;
    }

    pub(super) fn registers_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let Some(rows) = self.reg_rows() else {
            ui.label(tr("Aucun programme lancé.", "No program running.", "Ningún programa en ejecución."));
            return;
        };
        // Édition possible seulement quand le processus est vivant et en pause à
        // la dernière étape (ptrace ne peut pas écrire dans un process terminé).
        let editable = self.can_step();
        let hint = if editable {
            tr("clic sur une valeur pour l'éditer", "click a value to edit it", "clic en un valor para editarlo")
        } else if self.dbg.as_ref().is_some_and(|d| !d.is_alive()) {
            tr("édition indisponible (programme terminé — relancez)", "editing unavailable (program finished — relaunch)", "edición no disponible (programa terminado — reinicie)")
        } else {
            tr("édition à la dernière étape (revenez en fin de timeline)", "editing only at the last step (go to the end of the timeline)", "edición solo en el último paso (vaya al final de la línea de tiempo)")
        };
        ui.label(RichText::new(hint).small().weak());
        let flash = self.flash_progress(ui); // pulsation « CPU vivant »
        let scroll_here = self.take_scroll_request(super::dock::Panel::Registers);
        let blink = self.blink_intensity(ui); // clignotement pédagogique (0 si désactivé)
        let ped = self.pedagogy_anim;
        let mut commit: Option<(&'static str, u64)> = None;
        let mut stop_edit = false;

        let hdr = self.c_header();
        // Nombre de colonnes selon la largeur : jusqu'à trois registres par ligne
        // quand le panneau est large (on remplit l'espace au lieu de le laisser
        // vide à droite), moins s'il est étroit pour qu'une valeur 64 bits ne
        // déborde jamais. Chaque registre occupe deux cellules : le nom, la valeur.
        let cols = ((ui.available_width() / 205.0).floor() as usize).clamp(1, 3);
        self.reg_cols = cols;
        egui::ScrollArea::vertical()
            .id_salt("regs_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("regs_grid").num_columns(cols * 2).spacing([22.0, 8.0]).show(ui, |ui| {
                    for (i, (name, val, pval)) in rows.iter().enumerate() {
                        let (name, val, pval) = (*name, *val, *pval);
                        // Registre retenu au clavier : surligné, pour que les
                        // flèches ne déplacent pas un curseur invisible.
                        let kb_sel = i == self.reg_sel;
                        // Rôle ABI en infobulle : « survit-il à un call ? » est la
                        // question que se pose l'élève dont une valeur disparaît.
                        let r = crate::abi::role(name);
                        let role_tip = format!(
                            "{name} — {}\n{}",
                            r.label(lang),
                            if r.survives_call() {
                                tr("✔ conservé à travers un call", "✔ preserved across a call", "✔ conservado a través de un call")
                            } else {
                                tr("✘ peut être écrasé par un call", "✘ may be clobbered by a call", "✘ puede ser sobrescrito por un call")
                            }
                        );
                        let name_txt = RichText::new(name)
                            .monospace()
                            .strong()
                            .color(if kb_sel { super::ACCENT } else { hdr });
                        let name_resp = ui.label(name_txt).on_hover_text(role_tip);
                        // Le registre retenu au clavier est amené à l'écran :
                        // sans cela, les flèches le poussaient hors du cadre
                        // visible et la barre de défilement ne suivait pas.
                        if kb_sel && scroll_here {
                            name_resp.scroll_to_me(Some(egui::Align::Center));
                        }
                        if kb_sel {
                            // Liseré autour du nom : discret mais sans ambiguïté.
                            ui.painter().rect_stroke(
                                name_resp.rect.expand(2.0),
                                3.0,
                                egui::Stroke::new(1.0_f32, super::ACCENT),
                                egui::StrokeKind::Middle,
                            );
                        }
                        if self.edit_reg == Some(name) {
                            // Édition : champ hexa + ✓ (valider) / ✗ (annuler).
                            let focus_now = std::mem::take(&mut self.edit_focus);
                            let buf = &mut self.edit_buf;
                            let mut committed: Option<u64> = None;
                            let mut ended = false;
                            ui.horizontal(|ui| {
                                let resp = ui.add(
                                    egui::TextEdit::singleline(buf)
                                        .id(egui::Id::new("kb_reg_edit"))
                                        .desired_width(96.0)
                                        .font(egui::TextStyle::Monospace)
                                        .hint_text("hex"),
                                );
                                if focus_now {
                                    resp.request_focus();
                                }
                                let enter =
                                    resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                if ui.small_button("✓").clicked() || enter {
                                    committed = parse_hex(buf);
                                    ended = true;
                                }
                                if ui.small_button("✗").clicked() {
                                    ended = true;
                                }
                            });
                            if let Some(v) = committed {
                                commit = Some((name, v));
                            }
                            if ended {
                                stop_edit = true;
                            }
                        } else {
                            // Valeur en « chip » arrondi ; fond orangé si la valeur
                            // a changé (pulse via l'animation « CPU vivant »).
                            let changed = val != pval;
                            // Mode pédagogique : le fond clignote (3 pulses) au lieu
                            // d'un simple fondu, pour attirer l'œil sur le changement.
                            let bg = if changed {
                                if blink > 0.0 {
                                    lerp_color(CHANGED.linear_multiply(0.22), FLASH_BRIGHT.linear_multiply(0.55), blink)
                                } else {
                                    changed_color(flash).linear_multiply(0.22)
                                }
                            } else {
                                ui.visuals().faint_bg_color
                            };
                            let t = RichText::new(format!("0x{val:016X}")).monospace();
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    // Bordure clignotante autour du chip modifié.
                                    let stroke = if changed && blink > 0.0 {
                                        egui::Stroke::new(1.0 + 1.4 * blink, lerp_color(CHANGED, FLASH_BRIGHT, blink))
                                    } else if changed {
                                        egui::Stroke::new(1.0_f32, changed_color(flash))
                                    } else {
                                        egui::Stroke::NONE
                                    };
                                    if editable {
                                        let chip = egui::Button::new(t)
                                            .fill(bg)
                                            .stroke(stroke)
                                            .corner_radius(egui::CornerRadius::same(4));
                                        let resp = ui.add(chip).on_hover_text(tr("Cliquer pour modifier", "Click to edit", "Clic para editar"));
                                        if resp.hovered() {
                                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }
                                        if resp.clicked() {
                                            self.edit_reg = Some(name);
                                            self.edit_buf = format!("{val:X}");
                                            self.edit_focus = true;
                                        }
                                    } else {
                                        egui::Frame::new()
                                            .fill(bg)
                                            .stroke(stroke)
                                            .corner_radius(egui::CornerRadius::same(4))
                                            .inner_margin(egui::Margin::symmetric(8, 3))
                                            .show(ui, |ui| {
                                                ui.label(t);
                                            });
                                    }
                                    // Flèche directionnelle + delta chiffré, clignotants.
                                    if changed && ped {
                                        let up = val > pval;
                                        let base = if up { PUSH_COL } else { POP_COL };
                                        let col = lerp_color(base, FLASH_BRIGHT, blink);
                                        ui.label(
                                            RichText::new(if up { "▲" } else { "▼" })
                                                .strong()
                                                .color(col)
                                                .size(13.0 + 3.0 * blink),
                                        );
                                        // Delta signé (décimal si petit, sinon hexa).
                                        let d = val.wrapping_sub(pval) as i64;
                                        let txt = if d.unsigned_abs() < 1_000_000 {
                                            format!("{d:+}")
                                        } else if up {
                                            format!("+0x{:X}", val - pval)
                                        } else {
                                            format!("-0x{:X}", pval - val)
                                        };
                                        ui.label(RichText::new(txt).monospace().small().color(col));
                                    }
                                });
                                // Bande de bits : montre exactement quels bits ont basculé.
                                if changed && ped {
                                    bit_diff_strip(ui, val, pval, blink, lang);
                                    // Ancienne valeur en fantôme, pour la comparaison.
                                    ui.label(
                                        RichText::new(format!("← 0x{pval:016X}"))
                                            .monospace()
                                            .small()
                                            .color(self.c_bytes().gamma_multiply(0.8)),
                                    );
                                }
                            });
                        }
                        if (i + 1) % cols == 0 {
                            ui.end_row();
                        }
                    }
                });
            });

        // Applique l'édition après le rendu (évite l'emprunt simultané de dbg).
        if let Some((name, v)) = commit {
            self.edit_reg = None;
            match self.dbg.as_mut().unwrap().set_register(name, v) {
                Ok(_) => self.status = format!("{name} = 0x{v:X}"),
                Err(e) => self.log(&e),
            }
        } else if stop_edit {
            self.edit_reg = None;
        }
    }

    pub(super) fn flags_ui(&self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let (Some(snap), Some(prev)) = (self.snap(), self.prev_snap()) else {
            ui.weak(tr("Aucun programme lancé.", "No program running.", "Ningún programa en ejecución."));
            return;
        };
        let flash = self.flash_progress(ui);
        let flags = Flags::from_eflags(snap.regs.eflags);
        let prevf = Flags::from_eflags(prev.regs.eflags);

        // Nom complet de chaque drapeau : le sigle seul ne dit rien à un débutant.
        let full = |name: &str| match name {
            "ZF" => tr("Zéro", "Zero", "Cero"),
            "CF" => tr("Retenue", "Carry", "Acarreo"),
            "OF" => tr("Débordement", "Overflow", "Desbordamiento"),
            "SF" => tr("Signe", "Sign", "Signo"),
            "PF" => tr("Parité", "Parity", "Paridad"),
            "AF" => tr("Retenue aux.", "Aux. carry", "Acarreo aux."),
            _ => "",
        };

        let items: Vec<(&'static str, bool, bool)> = flags
            .named()
            .iter()
            .zip(prevf.named())
            .map(|((n, v), (_, p))| (*n, *v, p))
            .collect();

        // Des cartes qui occupent la largeur au lieu de six sigles perdus dans un
        // coin : jusqu'à trois par ligne quand la place le permet, deux sinon.
        let cols = ((ui.available_width() / 150.0).floor() as usize).clamp(1, 3);
        ui.add_space(4.0);
        for chunk in items.chunks(cols) {
            ui.columns(cols, |c| {
                for (j, (name, val, pval)) in chunk.iter().enumerate() {
                    let changed = *val != *pval;
                    // Vert quand actif, gris quand inactif, orange quand il vient
                    // de basculer — l'œil repère le drapeau qui a changé.
                    let (fill, stroke_col, accent) = if changed {
                        let cc = changed_color(flash);
                        (cc.linear_multiply(0.20), cc, cc)
                    } else if *val {
                        (FLAG_ON.linear_multiply(0.16), FLAG_ON.linear_multiply(0.55), FLAG_ON)
                    } else {
                        (c[j].visuals().faint_bg_color, FLAG_OFF.linear_multiply(0.30), FLAG_OFF)
                    };
                    egui::Frame::new()
                        .fill(fill)
                        .stroke(egui::Stroke::new(1.0_f32, stroke_col))
                        .corner_radius(egui::CornerRadius::same(8))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(&mut c[j], |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(*name)
                                            .monospace()
                                            .strong()
                                            .size(16.0)
                                            .color(accent),
                                    );
                                    ui.label(RichText::new(full(name)).small().weak());
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(if *val { "1" } else { "0" })
                                                .monospace()
                                                .strong()
                                                .size(22.0)
                                                .color(accent),
                                        );
                                    },
                                );
                            });
                        });
                }
            });
            ui.add_space(8.0);
        }
    }

    // ---------- Explorateur de fichiers (panneau de gauche) ----------

    /// Déplace la sélection clavier de l'explorateur d'un fichier.
    ///
    /// Ne parcourt que les fichiers du dossier racine : les sous-dossiers se
    /// déplient à la souris, et un parcours récursif au clavier serait plus
    /// déroutant qu'utile.
    pub(super) fn move_explorer_selection(&mut self, down: bool) {
        let (_, files) = super::list_entries(&self.explorer_dir);
        if files.is_empty() {
            return;
        }
        let cur = self
            .explorer_selected
            .as_ref()
            .or(Some(&self.src_path))
            .and_then(|p| files.iter().position(|f| f == p));
        let next = match (cur, down) {
            (None, _) => 0,
            (Some(i), true) => (i + 1).min(files.len() - 1),
            (Some(i), false) => i.saturating_sub(1),
        };
        self.explorer_selected = Some(files[next].clone());
        self.scroll_to_sel = Some(super::dock::Panel::Explorer);
    }

    pub(super) fn explorer_ui(&mut self, ui: &mut egui::Ui) {
        let up_tip = i18n::tr3(self.lang, "Dossier parent comme racine", "Parent folder as root", "Carpeta padre como raíz");

        // Barre : nom du dossier racine + remonter d'un cran.
        let mut go_up = false;
        ui.horizontal(|ui| {
            if self
                .tip(ui.small_button("⬆"), up_tip)
                .clicked()
            {
                go_up = true;
            }
            let root = self
                .explorer_dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.explorer_dir.display().to_string());
            ui.label(RichText::new(root).strong().color(self.c_header()))
                .on_hover_text(self.explorer_dir.display().to_string());
        });
        if go_up && let Some(p) = self.explorer_dir.parent() {
            self.explorer_dir = p.to_path_buf();
        }
        ui.separator();

        // Arbre de fichiers (dossiers repliables + fichiers cliquables).
        let asm_col = self.c_mnemonic();
        let other_col = self.c_bytes();
        // Le repère suit la sélection clavier quand il y en a une, sinon le
        // fichier ouvert : sans cela, les flèches déplaceraient un curseur invisible.
        let cur = self.explorer_selected.clone().unwrap_or_else(|| self.src_path.clone());
        let scroll_here = self.take_scroll_request(super::dock::Panel::Explorer);
        let mut to_open = None;
        egui::ScrollArea::both().id_salt("explorer_scroll").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().indent = 14.0;
            let root = self.explorer_dir.clone();
            dir_tree(ui, &root, &cur, scroll_here, asm_col, other_col, &mut to_open);
        });
        if let Some(f) = to_open {
            self.open_file(f);
        }
    }

    // ---------- Call stack ----------

    pub(super) fn callstack_ui(&self, ui: &mut egui::Ui) {
        if self.dbg.is_none() {
            ui.weak("—");
            return;
        }
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(self.lang, fr, en, es);
        // Défilement sur les deux axes, comme SYSCALLS : une frame s'écrit
        // « #2  0x004000B5  (courant) » et dépasse vite une colonne étroite.
        // `.extend()` empêche le repli du texte, sans quoi la barre horizontale
        // n'aurait rien à faire défiler.
        egui::ScrollArea::both().id_salt("callstack_scroll").auto_shrink([false, false]).show(ui, |ui| {
            let line = |ui: &mut egui::Ui, txt: RichText| {
                ui.add(egui::Label::new(txt).extend());
            };
            // Frame courante en haut (RIP), puis les retours empilés.
            let mut depth = self.call_stack.len();
            if let Some(rip) = self.view_rip() {
                line(ui, RichText::new(format!("#{depth}  0x{rip:08X}  {}", tr("(courant)", "(current)", "(actual)"))).monospace().color(CHANGED));
            }
            for addr in self.call_stack.iter().rev() {
                depth = depth.saturating_sub(1);
                line(ui, RichText::new(format!("#{depth}  0x{addr:08X}")).monospace().color(self.c_addr()));
            }
            if self.call_stack.is_empty() {
                line(ui, RichText::new(tr("(aucun appel en cours)", "(no active call)", "(ninguna llamada activa)")).weak());
            }
        });
    }

    // ---------- Syscalls ----------

    pub(super) fn syscalls_ui(&self, ui: &mut egui::Ui) {
        // Défilement sur les DEUX axes : les arguments d'un appel système sont
        // souvent plus larges que la colonne (chemins, tampons, tailles). On les
        // laisse déborder et on donne une barre horizontale, plutôt que de les
        // tronquer — un argument coupé n'apprend rien.
        egui::ScrollArea::both()
            .id_salt("syscalls_scroll")
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.syscalls.is_empty() {
                    ui.weak(i18n::tr3(self.lang, "(aucun appel système)", "(no system call)", "(ninguna llamada al sistema)"));
                }
                for s in &self.syscalls {
                    // Couleur encode le résultat : vert=ok, rouge=erreur, gris=pending.
                    let col = match s.ret {
                        Some(r) if r < 0 => FALSE_COL,
                        Some(_) => FLAG_ON,
                        None => self.c_bytes(),
                    };
                    // Ligne 1 : nom  #num  = ret. Le retour est aligné à gauche
                    // à la suite du numéro : dans une zone qui défile
                    // horizontalement, un alignement à droite se collerait au
                    // bord du viewport et glisserait au défilement.
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&s.name)
                                .monospace()
                                .strong()
                                .color(col),
                        );
                        ui.label(
                            RichText::new(format!("#{}", s.number))
                                .monospace()
                                .small()
                                .color(self.c_bytes()),
                        );
                        if let Some(r) = s.ret {
                            ui.label(
                                RichText::new(format!("= {r}"))
                                    .monospace()
                                    .small()
                                    .color(col),
                            );
                        }
                    });
                    // Ligne 2 : arguments complets, en une seule ligne que la
                    // barre horizontale permet de parcourir.
                    if !s.args.is_empty() {
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!("  ({})", s.args))
                                    .monospace()
                                    .small()
                                    .weak(),
                            )
                            .extend(),
                        );
                    }
                    ui.add_space(2.0);
                }
            });
    }

    // ---------- Pile / Tas ----------

    pub(super) fn stack_ui(&mut self, ui: &mut egui::Ui) {
        // Handles clonés (Arc) => la closure peut muter self.stack_tab.
        let (stack_ic, heap_ic) = match &self.icons {
            Some(i) => (Some(i.stack.clone()), Some(i.heap.clone())),
            None => (None, None),
        };
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        panel_header(ui, |ui| {
            if icon_tab(ui, stack_ic.as_ref(), tr("Pile", "Stack", "Pila"), self.stack_tab == super::StackTab::Stack).clicked() {
                self.stack_tab = super::StackTab::Stack;
            }
            if icon_tab(ui, heap_ic.as_ref(), tr("Tas", "Heap", "Montículo"), self.stack_tab == super::StackTab::Heap).clicked() {
                self.stack_tab = super::StackTab::Heap;
            }
        });
        match self.stack_tab {
            super::StackTab::Stack => self.stack_view(ui),
            super::StackTab::Heap => self.heap_view(ui),
        }
    }

    pub(super) fn stack_view(&self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let Some(snap) = self.snap() else {
            ui.label("—");
            return;
        };
        let flash = self.flash_progress(ui);
        let (rsp, rbp) = (snap.regs.rsp, snap.regs.rbp);

        // Badge PUSH / POP d'après la variation de RSP par rapport à l'étape précédente.
        if let Some(prev) = self.prev_snap() {
            let prsp = prev.regs.rsp;
            if rsp < prsp {
                ui.label(RichText::new("⬇ PUSH").strong().color(changed_color2(flash, PUSH_COL)));
            } else if rsp > prsp {
                ui.label(RichText::new("⬆ POP").strong().color(changed_color2(flash, POP_COL)));
            } else {
                ui.label("");
            }
        }

        let prev_stack = self.prev_snap().map(|p| p.stack.clone()).unwrap_or_default();
        let blink = self.blink_intensity(ui);
        let ped = self.pedagogy_anim;
        // Décalage horizontal du glissement : la case fraîchement empilée arrive
        // depuis la droite (PUSH) ou repart vers la droite (POP).
        let slide = self.blink_progress(ui).map(|p| (1.0 - p) * 26.0).unwrap_or(0.0);
        let pushed = self.prev_snap().is_some_and(|p| rsp < p.regs.rsp);
        let addr_c = self.c_addr();
        egui::ScrollArea::vertical()
            .id_salt("stack_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
            for (i, val) in snap.stack.iter().enumerate() {
                let addr = rsp.wrapping_add((i as u64) * 8);
                let changed = prev_stack.get(i) != Some(val);
                let is_top = i == 0;
                // Barre de gauche : couleur = rôle de la case (sommet / cadre / corps).
                let bar_col = if addr == rsp {
                    PUSH_COL
                } else if addr == rbp {
                    ACTION
                } else {
                    self.c_bytes().gamma_multiply(0.5)
                };
                // Case modifiée : fond clignotant.
                let fill = if changed && blink > 0.0 {
                    lerp_color(CHANGED.linear_multiply(0.18), FLASH_BRIGHT.linear_multiply(0.45), blink)
                } else if changed {
                    changed_color(flash).linear_multiply(0.18)
                } else if addr == rsp || addr == rbp {
                    ui.visuals().faint_bg_color
                } else {
                    Color32::TRANSPARENT
                };
                // Le sommet de pile glisse pendant l'animation (effet empilement).
                let dx = if ped && is_top && pushed { slide } else { 0.0 };
                ui.horizontal(|ui| {
                    ui.add_space(dx);
                    // Barre verticale colorée (repère visuel du rôle de la case).
                    let (bar, _) = ui.allocate_exact_size(egui::vec2(4.0, 17.0), egui::Sense::hover());
                    ui.painter().rect_filled(bar.shrink2(egui::vec2(0.5, 1.0)), 2.0, bar_col);
                    egui::Frame::new()
                        .fill(fill)
                        .corner_radius(egui::CornerRadius::same(3))
                        .inner_margin(egui::Margin::symmetric(4, 1))
                        .show(ui, |ui| {
                            ui.label(RichText::new(format!("0x{addr:012X}")).monospace().small().color(addr_c));
                            let mut vt = RichText::new(format!("0x{val:016X}")).monospace();
                            if changed {
                                vt = vt.color(if blink > 0.0 {
                                    lerp_color(CHANGED, FLASH_BRIGHT, blink)
                                } else {
                                    changed_color(flash)
                                });
                            }
                            ui.label(vt);
                            let marker = if addr == rsp && addr == rbp {
                                "◀ RSP,RBP"
                            } else if addr == rsp {
                                "◀ RSP"
                            } else if addr == rbp {
                                "◀ RBP"
                            } else {
                                ""
                            };
                            if !marker.is_empty() {
                                ui.label(
                                    RichText::new(marker)
                                        .monospace()
                                        .small()
                                        .strong()
                                        .color(if addr == rsp { PUSH_COL } else { ACTION }),
                                );
                            }
                            // Rôle de la case dans le cadre d'appel : c'est ce qui
                            // transforme une colonne d'adresses en structure lisible.
                            if let Some(kind) = crate::abi::classify_slot(addr, rbp) {
                                let (txt, col) = match kind {
                                    crate::abi::SlotKind::ReturnAddress => {
                                        (kind.label(lang), FALSE_COL)
                                    }
                                    crate::abi::SlotKind::SavedFramePointer => {
                                        (kind.label(lang), ACTION)
                                    }
                                    _ => (kind.label(lang), self.c_bytes()),
                                };
                                ui.label(RichText::new(txt).small().italics().color(col));
                            }
                        });
                });
                ui.add_space(1.0);
            }
        });
    }

    /// Vue du tas (segment `[heap]` de /proc/<pid>/maps), en hexadécimal.
    pub(super) fn heap_view(&self, ui: &mut egui::Ui) {
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(self.lang, fr, en, es);
        if !self.can_read_memory() {
            let msg = match self.dbg.as_ref().map(|d| d.is_alive()) {
                Some(false) => tr(
                    "Programme terminé — relancez pour explorer le tas.",
                    "Program finished — relaunch to explore the heap.",
                    "Programa terminado — reinícielo para explorar el montículo.",
                ),
                Some(true) => tr(
                    "Revenez à la dernière étape de la timeline pour lire le tas.",
                    "Go back to the last timeline step to read the heap.",
                    "Vuelva al último paso de la línea de tiempo para leer el montículo.",
                ),
                None => tr("Lancez un programme pour explorer le tas.", "Run a program to explore the heap.", "Inicie un programa para explorar el montículo."),
            };
            ui.weak(msg);
            return;
        }
        let (hdr, addr_c, bytes_c) = (self.c_header(), self.c_addr(), self.c_bytes());
        let dbg = self.dbg.as_ref().unwrap();
        let Some((start, end)) = dbg.heap_range() else {
            ui.weak(tr(
                "Aucun tas pour ce programme : le segment [heap] n'apparaît qu'après un appel \
                 brk/mmap (allocation dynamique). Un programme n'utilisant que .data/.bss ou la \
                 pile n'a pas de tas.",
                "No heap for this program: the [heap] segment only appears after a brk/mmap call \
                 (dynamic allocation). A program using only .data/.bss or the stack has no heap.",
                "Sin montículo para este programa: el segmento [heap] solo aparece tras una llamada \
                 brk/mmap (asignación dinámica). Un programa que solo usa .data/.bss o la pila no tiene montículo.",
            ));
            return;
        };
        let size = end - start;
        ui.label(
            RichText::new(format!("[heap] 0x{start:X} – 0x{end:X}  ({size} {})", tr("octets", "bytes", "bytes")))
                .monospace()
                .color(hdr),
        );
        ui.add_space(2.0);
        let rows = size.div_ceil(16).min(16);
        egui::ScrollArea::both().id_salt("heap_scroll").auto_shrink([false, false]).show(ui, |ui| {
            hex_dump_rows(ui, addr_c, bytes_c, dbg, start, rows);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Le panneau SYSCALLS doit se rendre sans paniquer avec des arguments
    /// longs, et la trace doit bien contenir les appels — c'est ce contenu
    /// large qui justifie la barre de défilement horizontale.
    #[test]
    fn syscalls_panel_renders_with_long_arguments() {
        let mut app = App::new();
        app.src_path = PathBuf::from("build/sysc-test.asm");
        app.out_dir = PathBuf::from("build/sysc");
        // write(1, msg, 60) produit une ligne d'arguments large.
        app.source = "\
section .data
    msg db 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 10
section .text
    global _start
_start:
    mov rax, 1
    mov rdi, 1
    mov rsi, msg
    mov rdx, 60
    syscall
    mov rax, 60
    xor rdi, rdi
    syscall
"
        .to_string();

        app.launch();
        assert!(app.dbg.is_some(), "programme lancé");
        for _ in 0..20 {
            app.step();
        }
        assert!(!app.syscalls.is_empty(), "des appels système doivent être journalisés");
        let write = &app.syscalls[0];
        assert_eq!(write.number, 1, "premier appel = write");
        assert!(
            !write.args.is_empty(),
            "les arguments doivent être conservés en entier, pas tronqués"
        );

        // Rendu headless dans une colonne étroite : c'est le cas où la barre
        // horizontale sert, et rien ne doit paniquer.
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.set_max_width(180.0);
                app.syscalls_ui(ui);
            });
        });
    }

    /// CALL STACK doit lui aussi défiler horizontalement : une frame s'écrit
    /// « #2  0x004000B5  (courant) » et dépasse une colonne étroite. On vérifie
    /// que la pile d'appels est bien peuplée et que le rendu tient dans 150 px.
    #[test]
    fn callstack_panel_renders_in_a_narrow_column() {
        let mut app = App::new();
        app.src_path = PathBuf::from("build/cs-test.asm");
        app.out_dir = PathBuf::from("build/cs");
        // Deux niveaux d'appel imbriqués : la pile a de la profondeur à montrer.
        app.source = "\
section .text
    global _start
_start:
    call niveau1
    mov rax, 60
    xor rdi, rdi
    syscall
niveau1:
    call niveau2
    ret
niveau2:
    mov rax, 1
    ret
"
        .to_string();

        app.launch();
        assert!(app.dbg.is_some(), "programme lancé");
        // Avance jusqu'à être au plus profond des appels.
        let mut deepest = 0;
        for _ in 0..12 {
            app.step();
            deepest = deepest.max(app.call_stack.len());
        }
        assert!(deepest >= 2, "deux appels imbriqués attendus, vu {deepest}");

        // Rendu headless des DEUX panneaux dans une colonne étroite.
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.set_max_width(150.0);
                app.callstack_ui(ui);
                app.syscalls_ui(ui);
            });
        });
    }

    /// L'explorateur se parcourt au clavier et s'arrête aux bornes, comme le
    /// désassemblage — sinon le panneau resterait pilotable à la souris seule.
    #[test]
    fn explorer_selection_moves_and_clamps() {
        let mut app = App::new();
        // Dossier d'exemples réel : il contient plusieurs .asm.
        app.explorer_dir = super::super::abs_dir_of(std::path::Path::new("examples/test.asm"));
        let (_, files) = super::super::list_entries(&app.explorer_dir);
        assert!(files.len() >= 2, "il faut au moins deux fichiers pour tester");

        app.explorer_selected = None;
        app.move_explorer_selection(true);
        let first = app.explorer_selected.clone();
        assert!(first.is_some(), "une première sélection doit apparaître");

        // On descend jusqu'au bout, puis une fois de trop.
        for _ in 0..files.len() + 3 {
            app.move_explorer_selection(true);
        }
        assert_eq!(
            app.explorer_selected.as_ref(),
            files.last(),
            "la descente doit s'arrêter au dernier fichier"
        );

        // Et on remonte jusqu'au premier, puis une fois de trop.
        for _ in 0..files.len() + 3 {
            app.move_explorer_selection(false);
        }
        assert_eq!(
            app.explorer_selected.as_ref(),
            files.first(),
            "la remontée doit s'arrêter au premier fichier"
        );
    }

    /// Un dossier sans fichier ne doit pas paniquer.
    #[test]
    fn explorer_selection_is_safe_when_empty() {
        let mut app = App::new();
        app.explorer_dir = std::env::temp_dir().join("asmstudio-vide-inexistant");
        app.explorer_selected = None;
        app.move_explorer_selection(true);
        assert_eq!(app.explorer_selected, None);
    }

    /// Les registres se parcourent au clavier : ↑/↓ changent de ligne (deux
    /// registres par ligne), ←/→ traversent la ligne, et Entrée ouvre l'édition.
    #[test]
    fn register_selection_navigates_and_clamps() {
        use std::path::PathBuf;
        let mut app = App::new();
        app.src_path = PathBuf::from("build/reg-kb.asm");
        app.out_dir = PathBuf::from("build/regkb");
        app.source = "section .text\n global _start\n_start:\n mov rax,5\n \
                       mov rax,60\n xor rdi,rdi\n syscall\n"
            .to_string();
        app.launch();
        app.step();

        let n = app.snap().unwrap().regs.named().len();
        assert!(n >= 16, "les registres doivent être disponibles");

        app.reg_sel = 0;
        app.move_reg_selection(true);
        assert_eq!(app.reg_sel, 2, "↓ saute une ligne entière (2 par ligne)");
        app.move_reg_selection(false);
        assert_eq!(app.reg_sel, 0);
        app.move_reg_selection(false);
        assert_eq!(app.reg_sel, 0, "borne haute respectée");

        app.move_reg_selection_sideways(true);
        assert_eq!(app.reg_sel, 1, "→ avance d'un registre");
        app.move_reg_selection_sideways(false);
        assert_eq!(app.reg_sel, 0);
        app.move_reg_selection_sideways(false);
        assert_eq!(app.reg_sel, 0, "borne haute respectée");

        // Descente jusqu'au bout : jamais hors bornes.
        for _ in 0..n + 5 {
            app.move_reg_selection(true);
        }
        assert!(app.reg_sel < n, "sélection {} hors de {n}", app.reg_sel);

        // Entrée ouvre l'édition du registre retenu.
        app.reg_sel = 0;
        app.edit_selected_register();
        let named = app.snap().unwrap().regs.named();
        assert_eq!(app.edit_reg, Some(named[0].0), "édition du registre retenu");
        assert!(!app.edit_buf.is_empty(), "le tampon reçoit la valeur courante");
    }

    /// Sans programme lancé, la navigation dans les registres ne doit rien faire
    /// plutôt que paniquer.
    #[test]
    fn register_navigation_is_safe_without_a_program() {
        let mut app = App::new();
        app.move_reg_selection(true);
        app.move_reg_selection_sideways(true);
        app.edit_selected_register();
        assert_eq!(app.reg_sel, 0);
        assert_eq!(app.edit_reg, None, "rien à éditer sans processus");
    }

    /// Le mode apprentissage masque R8–R15 et EFLAGS : neuf registres au lieu
    /// de dix-huit. La navigation clavier doit suivre la même liste, sinon les
    /// flèches désigneraient des registres invisibles.
    #[test]
    fn learning_mode_shows_fewer_registers() {
        use std::path::PathBuf;
        let mut app = App::new();
        app.src_path = PathBuf::from("build/regmode.asm");
        app.out_dir = PathBuf::from("build/regmode");
        app.source = "section .text\n global _start\n_start:\n mov rax,5\n \
                       mov rax,60\n xor rdi,rdi\n syscall\n"
            .to_string();
        app.launch();
        app.step();

        let learning = app.reg_rows().expect("registres disponibles");
        assert_eq!(learning.len(), 9, "les huit généraux + RIP");
        for n in ["R8", "R15", "EFLAGS"] {
            assert!(!learning.iter().any(|(name, _, _)| *name == n), "{n} ne doit pas apparaître");
        }
        for n in ["RAX", "RSP", "RIP"] {
            assert!(learning.iter().any(|(name, _, _)| *name == n), "{n} doit apparaître");
        }

        app.set_ui_mode(super::super::UiMode::Full);
        let full = app.reg_rows().expect("registres disponibles");
        assert!(full.len() > learning.len(), "le mode complet en montre davantage");
        assert!(full.iter().any(|(n, _, _)| *n == "R15"));

        // La navigation clavier est bornée par la liste VISIBLE.
        app.set_ui_mode(super::super::UiMode::Learning);
        app.reg_sel = 0;
        for _ in 0..30 {
            app.move_reg_selection_sideways(true);
        }
        assert!(app.reg_sel < 9, "sélection {} hors des 9 registres visibles", app.reg_sel);

        // Et Entrée édite bien le registre visible retenu, pas un homonyme d'index.
        app.reg_sel = 8;
        app.edit_selected_register();
        assert_eq!(app.edit_reg, Some("RIP"), "le 9e registre visible est RIP");
    }
}
