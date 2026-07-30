use eframe::egui::{self, Color32, RichText};

use crate::debugger::Flags;
use crate::i18n;

use super::{
    App, ACTION, CHANGED, FLAG_ON, FLAG_OFF, FALSE_COL, FLASH_BRIGHT,
    PUSH_COL, POP_COL,
    changed_color, changed_color2, lerp_color,
    panel_header, header, header_icon, header_title, icon_tab,
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
        header_icon(ui, self.c_header(), self.icons.as_ref().map(|i| &i.timeline), "TIMELINE");
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
                        let mut btn = egui::Button::new(txt).min_size(egui::vec2(22.0, 22.0)).rounding(egui::Rounding::same(11.0));
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
        let hdr = self.c_header();
        panel_header(ui, |ui| {
            header_title(ui, hdr, mem_ic.as_ref(), "MEMORY");
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
        let hdr = self.c_header();
        let clear = i18n::tr3(self.lang, "effacer", "clear", "borrar");
        panel_header(ui, |ui| {
            header_title(ui, hdr, console_ic.as_ref(), "CONSOLE");
            if ui.small_button(clear).clicked() {
                self.console.clear();
            }
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

    /// (nom, valeur, valeur précédente) des registres à afficher.
    pub(super) fn reg_rows(&self) -> Option<Vec<(&'static str, u64, u64)>> {
        let snap = self.snap()?;
        let prev = self.prev_snap()?;
        Some(
            snap.regs
                .named()
                .iter()
                .zip(prev.regs.named())
                .map(|((n, v), (_, p))| (*n, *v, p))
                .collect(),
        )
    }

    pub(super) fn registers_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        header_icon(ui, self.c_header(), self.icons.as_ref().map(|i| &i.registers), "REGISTERS");
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
        let blink = self.blink_intensity(ui); // clignotement pédagogique (0 si désactivé)
        let ped = self.pedagogy_anim;
        let mut commit: Option<(&'static str, u64)> = None;
        let mut stop_edit = false;

        let hdr = self.c_header();
        egui::ScrollArea::vertical()
            .id_salt("regs_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Deux registres par ligne (grille à 4 colonnes) : tout tient sans
                // ascenseur et la largeur du panneau est mieux exploitée.
                egui::Grid::new("regs_grid").num_columns(4).spacing([20.0, 6.0]).show(ui, |ui| {
                    for (i, (name, val, pval)) in rows.iter().enumerate() {
                        let (name, val, pval) = (*name, *val, *pval);
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
                        ui.label(RichText::new(name).monospace().strong().color(hdr))
                            .on_hover_text(role_tip);
                        if self.edit_reg == Some(name) {
                            // Édition : champ hexa + ✓ (valider) / ✗ (annuler).
                            let focus_now = std::mem::take(&mut self.edit_focus);
                            let buf = &mut self.edit_buf;
                            let mut committed: Option<u64> = None;
                            let mut ended = false;
                            ui.horizontal(|ui| {
                                let resp = ui.add(
                                    egui::TextEdit::singleline(buf)
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
                                            .rounding(egui::Rounding::same(4.0));
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
                                        egui::Frame::none()
                                            .fill(bg)
                                            .stroke(stroke)
                                            .rounding(egui::Rounding::same(4.0))
                                            .inner_margin(egui::Margin::symmetric(8.0, 3.0))
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
                                    bit_diff_strip(ui, val, pval, blink);
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
                        if i % 2 == 1 {
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
        header(ui, self.c_header(), "FLAGS");
        let (Some(snap), Some(prev)) = (self.snap(), self.prev_snap()) else {
            ui.weak("—");
            return;
        };
        let flash = self.flash_progress(ui);
        let flags = Flags::from_eflags(snap.regs.eflags);
        let prevf = Flags::from_eflags(prev.regs.eflags);
        // Disposition horizontale compacte (grille 3×2) : tient sans ascenseur
        // dans le panneau étroit. Chaque cellule = « NOM ● valeur ».
        const PER_ROW: usize = 3;
        egui::Grid::new("flags_grid").num_columns(PER_ROW).spacing([16.0, 6.0]).show(ui, |ui| {
            for (i, ((name, val), (_, pval))) in
                flags.named().iter().zip(prevf.named()).enumerate()
            {
                let changed = *val != pval;
                let dot = if changed {
                    changed_color(flash)
                } else if *val {
                    FLAG_ON
                } else {
                    FLAG_OFF
                };
                ui.horizontal(|ui| {
                    let mut nm = RichText::new(*name).monospace().strong();
                    if changed {
                        nm = nm.color(changed_color(flash));
                    }
                    ui.label(nm);
                    ui.label(RichText::new("●").color(dot).size(10.0));
                    ui.label(
                        RichText::new(if *val { "1" } else { "0" })
                            .monospace()
                            .strong()
                            .color(if *val { FLAG_ON } else { FLAG_OFF }),
                    );
                });
                if (i + 1) % PER_ROW == 0 {
                    ui.end_row();
                }
            }
        });
    }

    // ---------- Explorateur de fichiers (panneau de gauche) ----------

    pub(super) fn explorer_ui(&mut self, ui: &mut egui::Ui) {
        let up_tip = i18n::tr3(self.lang, "Dossier parent comme racine", "Parent folder as root", "Carpeta padre como raíz");
        header_icon(ui, self.c_header(), self.icons.as_ref().map(|i| &i.explorer), "EXPLORER");

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
        let cur = self.src_path.clone();
        let mut to_open = None;
        egui::ScrollArea::both().id_salt("explorer_scroll").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().indent = 14.0;
            let root = self.explorer_dir.clone();
            dir_tree(ui, &root, &cur, asm_col, other_col, &mut to_open);
        });
        if let Some(f) = to_open {
            self.open_file(f);
        }
    }

    // ---------- Call stack ----------

    pub(super) fn callstack_ui(&self, ui: &mut egui::Ui) {
        header_icon(ui, self.c_header(), self.icons.as_ref().map(|i| &i.callstack), "CALL STACK");
        if self.dbg.is_none() {
            ui.weak("—");
            return;
        }
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(self.lang, fr, en, es);
        egui::ScrollArea::vertical().id_salt("callstack_scroll").auto_shrink([false, false]).show(ui, |ui| {
            // Frame courante en haut (RIP), puis les retours empilés.
            let mut depth = self.call_stack.len();
            if let Some(rip) = self.view_rip() {
                ui.label(RichText::new(format!("#{depth}  0x{rip:08X}  {}", tr("(courant)", "(current)", "(actual)"))).monospace().color(CHANGED));
            }
            for addr in self.call_stack.iter().rev() {
                depth = depth.saturating_sub(1);
                ui.label(RichText::new(format!("#{depth}  0x{addr:08X}")).monospace().color(self.c_addr()));
            }
            if self.call_stack.is_empty() {
                ui.weak(tr("(aucun appel en cours)", "(no active call)", "(ninguna llamada activa)"));
            }
        });
    }

    // ---------- Syscalls ----------

    pub(super) fn syscalls_ui(&self, ui: &mut egui::Ui) {
        header_icon(ui, self.c_header(), self.icons.as_ref().map(|i| &i.syscalls), "SYSCALLS");
        egui::ScrollArea::vertical()
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
                    // Ligne 1 : nom  #num  ——  = ret (aligné à droite).
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
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    RichText::new(format!("= {r}"))
                                        .monospace()
                                        .small()
                                        .color(col),
                                );
                            });
                        }
                    });
                    // Ligne 2 : arguments tronqués (évite le débordement horizontal).
                    if !s.args.is_empty() {
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!("  ({})", s.args))
                                    .monospace()
                                    .small()
                                    .weak(),
                            )
                            .truncate(),
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
                    egui::Frame::none()
                        .fill(fill)
                        .rounding(egui::Rounding::same(3.0))
                        .inner_margin(egui::Margin::symmetric(4.0, 1.0))
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
