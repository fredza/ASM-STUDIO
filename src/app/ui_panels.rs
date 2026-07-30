use eframe::egui::{self, Color32, RichText};

use crate::debugger::Flags;
use crate::i18n;

use super::{
    App, ACTION, CHANGED, FLAG_ON, FLAG_OFF, FALSE_COL, FLASH_BRIGHT,
    PUSH_COL, POP_COL, WIRE_COLORS,
    changed_color, changed_color2, lerp_color,
    panel_header, header, header_icon, header_title, icon_tab,
    hex_dump_rows, parse_hex, parse_hex_bytes,
    dir_tree,
};

/// Couleur du fil du n-ième registre (palette cyclique).
fn wire_col(i: usize) -> Color32 {
    WIRE_COLORS[i % WIRE_COLORS.len()]
}

/// Taille mémoire lisible par un humain, dans les unités de la langue courante
/// (fr : o/Kio/Mio — en/es : B/KiB/MiB).
fn human_size(bytes: u64, lang: crate::i18n::Lang) -> String {
    const K: u64 = 1024;
    let fr = matches!(lang, crate::i18n::Lang::Fr);
    let (b1, k, m, g) = if fr {
        ("o", "Kio", "Mio", "Gio")
    } else {
        ("B", "KiB", "MiB", "GiB")
    };
    match bytes {
        n if n < K => format!("{n} {b1}"),
        n if n < K * K => format!("{} {k}", n / K),
        n if n < K * K * K => format!("{} {m}", n / (K * K)),
        n => format!("{} {g}", n / (K * K * K)),
    }
}

/// Bande de 64 cellules montrant quels bits ont basculé entre `pval` et `val` :
/// cellule vive = bit modifié (clignote), cellule mate = bit inchangé.
/// Les bits à 1 sont pleins, ceux à 0 sont en creux — l'élève voit le motif binaire.
fn bit_diff_strip(ui: &mut egui::Ui, val: u64, pval: u64, blink: f32) {
    const CELL: f32 = 2.6;
    const GAP: f32 = 0.6;
    const H: f32 = 9.0;
    let diff = val ^ pval;
    let w = 64.0 * CELL + 63.0 * GAP;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, H), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let off = ui.visuals().extreme_bg_color;
    let on = ui.visuals().weak_text_color().gamma_multiply(0.55);
    let hot = lerp_color(CHANGED, FLASH_BRIGHT, blink);
    // Bit 63 à gauche, bit 0 à droite (ordre de lecture d'un nombre binaire).
    for i in 0..64u32 {
        let bit = 63 - i;
        let set = (val >> bit) & 1 == 1;
        let flipped = (diff >> bit) & 1 == 1;
        let x = rect.left() + i as f32 * (CELL + GAP);
        let cell = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(CELL, H));
        let col = match (flipped, set) {
            (true, true) => hot,
            (true, false) => hot.gamma_multiply(0.4),
            (false, true) => on,
            (false, false) => off,
        };
        // Un bit modifié occupe toute la hauteur ; un bit stable reste discret.
        let r = if flipped { cell } else { cell.shrink2(egui::vec2(0.0, 2.0)) };
        painter.rect_filled(r, 0.8, col);
    }
    // Séparateurs tous les 8 bits (frontières d'octet) pour aider à compter.
    for b in 1..8 {
        let x = rect.left() + (b as f32 * 8.0) * (CELL + GAP) - GAP * 0.5;
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(0.6_f32, ui.visuals().weak_text_color().gamma_multiply(0.35)),
        );
    }
    if resp.hovered() {
        resp.on_hover_text(format!(
            "{} bit(s) modifié(s)\n0b{val:064b}",
            diff.count_ones()
        ));
    }
}

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
            ui.label(RichText::new(format!("{} {}/{last}", tr("Instruction", "Instruction", "Instrucción"), self.view_index)).strong());
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
                        ui.label(RichText::new(name).monospace().strong().color(hdr));
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
                        });
                });
                ui.add_space(1.0);
            }
        });
    }

    // ---------- Vue mémoire unifiée (mode pédagogique) ----------

    /// Schéma peint : colonne de registres à gauche, carte des régions mémoire à
    /// droite, reliées par des courbes de Bézier colorées. Survoler un registre
    /// isole son fil et met en évidence l'octet visé ; les registres qui viennent
    /// de changer clignotent avec leur fil.
    pub(super) fn memory_map_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);

        if !self.can_read_memory() {
            let msg = match self.dbg.as_ref().map(|d| d.is_alive()) {
                Some(false) => tr(
                    "Programme terminé — relancez pour voir la carte mémoire.",
                    "Program finished — relaunch to see the memory map.",
                    "Programa terminado — reinícielo para ver el mapa de memoria.",
                ),
                _ => tr(
                    "Lancez un programme et avancez pas à pas : chaque registre qui contient une \
                     adresse sera relié à la région mémoire qu'il désigne.",
                    "Run a program and step through it: every register holding an address will be \
                     wired to the memory region it points at.",
                    "Ejecute un programa y avance paso a paso: cada registro que contenga una \
                     dirección se conectará a la región de memoria que señala.",
                ),
            };
            ui.weak(msg);
            return;
        }

        let Some(rows) = self.reg_rows() else { return };

        // --- Collecte : régions mappées + octets visés par chaque registre ---
        let regions = self.dbg.as_ref().unwrap().mem_regions();
        if regions.is_empty() {
            ui.weak(tr(
                "Aucune région mémoire lisible (/proc/<pid>/maps vide).",
                "No readable memory region (/proc/<pid>/maps empty).",
                "Ninguna región de memoria legible (/proc/<pid>/maps vacío).",
            ));
            return;
        }

        struct RegWire {
            name: &'static str,
            val: u64,
            changed: bool,
            /// Index dans `regions` de la région pointée, si le pointeur est valide.
            region: Option<usize>,
            /// 16 octets lus à l'adresse pointée.
            bytes: Option<Vec<u8>>,
        }
        let wires: Vec<RegWire> = {
            let dbg = self.dbg.as_ref().unwrap();
            rows.iter()
                .map(|(name, val, pval)| {
                    let region = regions.iter().position(|r| r.contains(*val));
                    let bytes = region.and_then(|_| dbg.read_mem(*val, 16).ok());
                    RegWire { name, val: *val, changed: val != pval, region, bytes }
                })
                .collect()
        };

        let (hdr, addr_c, bytes_c) = (self.c_header(), self.c_addr(), self.c_bytes());
        let blink = self.blink_intensity(ui);
        let weak_col = self.c_bytes().gamma_multiply(0.7);

        // Légende du code couleur des régions.
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("Régions :", "Regions:", "Regiones:")).small().color(hdr));
            for kind in [
                crate::debugger::RegionKind::Code,
                crate::debugger::RegionKind::Rodata,
                crate::debugger::RegionKind::Data,
                crate::debugger::RegionKind::Heap,
                crate::debugger::RegionKind::Stack,
            ] {
                if regions.iter().any(|r| r.kind == kind) {
                    super::badge(ui, kind.label(), super::region_color(kind));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(tr(
                        "survolez un registre pour isoler son fil",
                        "hover a register to isolate its wire",
                        "pase el cursor sobre un registro para aislar su hilo",
                    ))
                    .small()
                    .italics()
                    .color(weak_col),
                );
            });
        });
        ui.add_space(4.0);

        // --- Canevas peint ---
        egui::ScrollArea::vertical()
            .id_salt("memmap_canvas")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                const ROW_H: f32 = 22.0;
                const ROW_GAP: f32 = 4.0;
                const LANE_GAP: f32 = 6.0;
                let reg_w = 172.0_f32;
                let gap_w = 74.0_f32; // couloir des courbes
                let avail = ui.available_width();
                let lane_w = (avail - reg_w - gap_w - 8.0).max(150.0);

                let reg_h = wires.len() as f32 * (ROW_H + ROW_GAP);
                // Hauteur d'une voie de région : proportionnelle au nombre de
                // registres qui la visent, avec un minimum lisible.
                let lane_hits: Vec<usize> = (0..regions.len())
                    .map(|i| wires.iter().filter(|w| w.region == Some(i)).count())
                    .collect();
                let lane_h: Vec<f32> = lane_hits
                    .iter()
                    .map(|&n| if n > 0 { 54.0 + 16.0 * (n.min(3) as f32) } else { 30.0 })
                    .collect();
                let lanes_total: f32 = lane_h.iter().sum::<f32>() + lane_h.len() as f32 * LANE_GAP;

                let canvas_h = reg_h.max(lanes_total) + 8.0;
                let (canvas, _resp) = ui.allocate_exact_size(
                    egui::vec2(avail, canvas_h),
                    egui::Sense::hover(),
                );
                let painter = ui.painter_at(canvas);
                let pointer = ui.ctx().pointer_latest_pos();

                // Rectangles des registres (colonne de gauche), centrés verticalement.
                let reg_x0 = canvas.left() + 2.0;
                let reg_y0 = canvas.top() + ((canvas_h - reg_h) * 0.5).max(0.0);
                let reg_rects: Vec<egui::Rect> = (0..wires.len())
                    .map(|i| {
                        let y = reg_y0 + i as f32 * (ROW_H + ROW_GAP);
                        egui::Rect::from_min_size(egui::pos2(reg_x0, y), egui::vec2(reg_w, ROW_H))
                    })
                    .collect();

                // Rectangles des voies mémoire (colonne de droite).
                let lane_x0 = canvas.right() - lane_w - 2.0;
                let lane_y0 = canvas.top() + ((canvas_h - lanes_total) * 0.5).max(0.0);
                let mut lane_rects: Vec<egui::Rect> = Vec::with_capacity(regions.len());
                let mut y = lane_y0;
                for h in &lane_h {
                    lane_rects.push(egui::Rect::from_min_size(
                        egui::pos2(lane_x0, y),
                        egui::vec2(lane_w, *h),
                    ));
                    y += h + LANE_GAP;
                }

                // Registre survolé (priorité au fil isolé).
                let hovered: Option<usize> = pointer.and_then(|p| {
                    reg_rects.iter().position(|r| r.expand2(egui::vec2(0.0, 2.0)).contains(p))
                });

                // ---- 1. Voies mémoire (peintes d'abord : arrière-plan) ----
                for (i, region) in regions.iter().enumerate() {
                    let rect = lane_rects[i];
                    let col = super::region_color(region.kind);
                    let targeted = lane_hits[i] > 0;
                    // Un fil isolé estompe les voies non visées par ce registre.
                    let dim = match hovered {
                        Some(h) => wires[h].region != Some(i),
                        None => !targeted,
                    };
                    let a = if dim { 0.10 } else { 0.26 };
                    painter.rect_filled(rect, 5.0, col.linear_multiply(a));
                    painter.rect_stroke(
                        rect,
                        5.0,
                        egui::Stroke::new(if dim { 0.7_f32 } else { 1.4 }, col.gamma_multiply(if dim { 0.5 } else { 1.0 })),
                    );
                    // Bande d'accent à gauche de la voie.
                    painter.rect_filled(
                        egui::Rect::from_min_size(rect.left_top(), egui::vec2(4.0, rect.height())),
                        3.0,
                        col.gamma_multiply(if dim { 0.4 } else { 1.0 }),
                    );

                    let txt_c = if dim { weak_col } else { col };
                    // Titre : nom de la région + taille.
                    painter.text(
                        rect.left_top() + egui::vec2(10.0, 4.0),
                        egui::Align2::LEFT_TOP,
                        format!("{}  ({})", region.kind.label(), human_size(region.size(), lang)),
                        egui::FontId::proportional(12.0),
                        txt_c,
                    );
                    // Bornes d'adresses + permissions.
                    painter.text(
                        rect.left_top() + egui::vec2(10.0, 19.0),
                        egui::Align2::LEFT_TOP,
                        format!("0x{:X} – 0x{:X}   {}", region.start, region.end, region.perms),
                        egui::FontId::monospace(9.5),
                        if dim { weak_col } else { addr_c },
                    );

                    // Mini-carte : position relative des pointeurs dans la région.
                    let track = egui::Rect::from_min_size(
                        rect.left_bottom() + egui::vec2(10.0, -14.0),
                        egui::vec2(rect.width() - 20.0, 6.0),
                    );
                    painter.rect_filled(track, 3.0, ui.visuals().extreme_bg_color.gamma_multiply(0.8));
                    for (wi, w) in wires.iter().enumerate() {
                        if w.region != Some(i) {
                            continue;
                        }
                        let faded = hovered.is_some_and(|h| h != wi);
                        let frac = (w.val - region.start) as f64 / region.size().max(1) as f64;
                        let x = track.left() + track.width() * frac as f32;
                        let wc = wire_col(wi).gamma_multiply(if faded { 0.35 } else { 1.0 });
                        // Curseur de position dans la région.
                        painter.rect_filled(
                            egui::Rect::from_center_size(
                                egui::pos2(x, track.center().y),
                                egui::vec2(3.0, 12.0),
                            ),
                            1.5,
                            wc,
                        );
                    }

                    // Contenu visé, quand un seul fil (ou le fil survolé) cible la voie.
                    let show: Option<usize> = match hovered {
                        Some(h) if wires[h].region == Some(i) => Some(h),
                        None if lane_hits[i] == 1 => wires.iter().position(|w| w.region == Some(i)),
                        _ => None,
                    };
                    if let Some(wi) = show
                        && let Some(bytes) = &wires[wi].bytes
                    {
                        let hex: String = bytes.iter().take(8).map(|b| format!("{b:02X} ")).collect();
                        let ascii: String = bytes
                            .iter()
                            .take(8)
                            .map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '·' })
                            .collect();
                        painter.text(
                            rect.left_top() + egui::vec2(10.0, 33.0),
                            egui::Align2::LEFT_TOP,
                            format!("[{}] {} │{ascii}│", wires[wi].name, hex.trim_end()),
                            egui::FontId::monospace(10.0),
                            wire_col(wi),
                        );
                    }
                }

                // ---- 2. Boîtes de registres ----
                for (i, w) in wires.iter().enumerate() {
                    let rect = reg_rects[i];
                    let is_hov = hovered == Some(i);
                    let linked = w.region.is_some();
                    let faded = hovered.is_some_and(|h| h != i);

                    let wc = wire_col(i);
                    // Fond : clignotant si la valeur vient de changer.
                    let fill = if w.changed && blink > 0.0 {
                        lerp_color(CHANGED.linear_multiply(0.20), FLASH_BRIGHT.linear_multiply(0.5), blink)
                    } else if is_hov {
                        wc.linear_multiply(0.20)
                    } else if linked {
                        ui.visuals().faint_bg_color
                    } else {
                        Color32::TRANSPARENT
                    };
                    painter.rect_filled(rect, 4.0, fill);
                    let stroke_c = if w.changed && blink > 0.0 {
                        lerp_color(CHANGED, FLASH_BRIGHT, blink)
                    } else if linked {
                        wc.gamma_multiply(if faded { 0.35 } else { 1.0 })
                    } else {
                        weak_col.gamma_multiply(0.5)
                    };
                    let sw = if w.changed && blink > 0.0 { 1.0 + 1.6 * blink } else if is_hov { 1.6 } else { 1.0 };
                    painter.rect_stroke(rect, 4.0, egui::Stroke::new(sw, stroke_c));

                    let name_c = if w.changed {
                        lerp_color(CHANGED, FLASH_BRIGHT, blink)
                    } else if faded {
                        weak_col
                    } else {
                        hdr
                    };
                    painter.text(
                        rect.left_center() + egui::vec2(7.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        w.name,
                        egui::FontId::monospace(11.5),
                        name_c,
                    );
                    painter.text(
                        rect.right_center() + egui::vec2(-7.0, 0.0),
                        egui::Align2::RIGHT_CENTER,
                        format!("0x{:012X}", w.val),
                        egui::FontId::monospace(10.5),
                        if faded { weak_col } else if linked { addr_c } else { bytes_c },
                    );
                }

                // ---- 3. Fils de Bézier registre → voie mémoire ----
                // Peints après, pour passer au-dessus des boîtes et des voies.
                for (i, w) in wires.iter().enumerate() {
                    let Some(ri) = w.region else { continue };
                    let faded = hovered.is_some_and(|h| h != i);
                    let emphasised = hovered == Some(i) || (w.changed && blink > 0.0);

                    let from = reg_rects[i].right_center() + egui::vec2(1.0, 0.0);
                    // Point d'arrivée : hauteur proportionnelle à l'offset dans la région,
                    // pour que le fil désigne réellement l'endroit visé.
                    let lane = lane_rects[ri];
                    let region = &regions[ri];
                    let frac = ((w.val - region.start) as f64 / region.size().max(1) as f64) as f32;
                    let to_y = lane.top() + 6.0 + (lane.height() - 12.0) * frac.clamp(0.0, 1.0);
                    let to = egui::pos2(lane.left() - 1.0, to_y);

                    let mut col = wire_col(i);
                    if faded {
                        col = col.gamma_multiply(0.22);
                    } else if w.changed && blink > 0.0 {
                        col = lerp_color(col, FLASH_BRIGHT, blink * 0.8);
                    }
                    let width = if emphasised { 2.4_f32 } else if faded { 0.9 } else { 1.5 };

                    // Courbe cubique : tangentes horizontales aux deux extrémités.
                    let dx = ((to.x - from.x) * 0.55).max(24.0);
                    let curve = egui::epaint::CubicBezierShape::from_points_stroke(
                        [from, from + egui::vec2(dx, 0.0), to - egui::vec2(dx, 0.0), to],
                        false,
                        Color32::TRANSPARENT,
                        egui::Stroke::new(width, col),
                    );
                    painter.add(curve);
                    // Pastille de départ + tête de flèche à l'arrivée.
                    painter.circle_filled(from, if emphasised { 3.4 } else { 2.4 }, col);
                    let tip = 5.0_f32;
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            to,
                            to + egui::vec2(-tip, -tip * 0.6),
                            to + egui::vec2(-tip, tip * 0.6),
                        ],
                        col,
                        egui::Stroke::NONE,
                    ));
                }

                // Registres sans pointeur valide : note explicative sous le canevas.
                let unlinked = wires.iter().filter(|w| w.region.is_none()).count();
                if unlinked > 0 {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(format!(
                            "{unlinked} {}",
                            tr(
                                "registre(s) ne contiennent pas d'adresse mappée (valeur numérique, 0, ou zone non lisible).",
                                "register(s) hold no mapped address (plain number, 0, or unreadable area).",
                                "registro(s) no contienen una dirección mapeada (valor numérico, 0 o zona no legible).",
                            )
                        ))
                        .small()
                        .color(weak_col),
                    );
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
