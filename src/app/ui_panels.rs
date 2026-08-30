use eframe::egui::{self, Color32, RichText};

use crate::debugger::Flags;
use crate::i18n;
use crate::simd::{self, XmmView};

use super::{
    App, accent, action, changed_col, flag_on, flag_off, false_col, flash_bright, gutter_col,
    push_col, pop_col,
    changed_color, changed_color2, lerp_color,
    panel_header, icon_img, icon_tab,
    hex_dump_rows, parse_hex, parse_hex_bytes,
    dialog_window,
    explorer_entries, explorer_row, EXPLORER_ROW_H, ExplorerAction, ExplorerRename,
    ExplorerRowColors, ExplorerRowLabels, ExplorerRowMarks,
};
use super::pedagogy::bit_diff_strip;

/// Bouton « voir la sortie seule » de l'en-tête de console. Nommé plutôt
/// qu'écrit sur place : un test vérifie que ce caractère précis a bien un
/// glyphe, faute de quoi il s'afficherait en carré vide (le sort de `❯`, déjà
/// absent des polices par défaut).
const CONSOLE_OUTPUT_ICON: &str = "🖵";

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
                            btn = btn.fill(action());
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
                    .color(changed_col()),
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
                            Err(e) => {
                                let msg = e.message(lang);
                                self.log(&msg);
                            }
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
        let see_output = i18n::tr3(
            self.lang,
            "Voir la sortie du programme seule, comme au terminal",
            "See the program's output alone, as in a terminal",
            "Ver solo la salida del programa, como en un terminal",
        );
        panel_header(ui, |ui| {
            icon_img(ui, console_ic.as_ref(), 15.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Petite corbeille rouge plutôt qu'un mot : l'action est
                // universelle, et le libellé passe en infobulle.
                let btn = egui::Button::new(RichText::new("🗑").size(15.0).color(false_col()))
                    .frame(false);
                let resp = ui.add(btn).on_hover_text(clear);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    self.console.clear();
                }
                // Ouvre la sortie du programme démêlée du journal de l'IDE.
                // Au même endroit que la corbeille : les deux actions portent
                // sur ce qu'affiche la console, pas sur le programme.
                let btn = egui::Button::new(RichText::new(CONSOLE_OUTPUT_ICON).size(15.0).color(accent())).frame(false);
                let resp = ui.add(btn).on_hover_text(see_output);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    self.show_program_output = true;
                }
            });
        });
        // Champ de saisie en bas : le programme tracé lit ce qu'on y tape.
        // Réservé aux moments où il tourne — hors exécution, il n'y a personne
        // au bout du tuyau.
        // Un programme lancé sous Wine lit lui aussi son entrée standard : il
        // n'a pas de débogueur derrière, mais il a bien quelqu'un au bout du tuyau.
        let can_input = self.dbg.as_ref().is_some_and(|d| d.is_alive() && d.has_stdin())
            || self.wine.as_ref().is_some_and(|w| w.is_running());
        if can_input {
            let waiting = self.dbg.as_ref().is_some_and(|d| d.is_waiting());
            // Le focus se prend au passage en attente, et une seule fois : le
            // reprendre à chaque frame retiendrait l'élève dans ce champ tant
            // que le programme n'a pas sa saisie, sans pouvoir retourner dans
            // l'éditeur ni ouvrir la palette.
            let claim_focus = waiting && !self.stdin_focus_claimed;
            self.stdin_focus_claimed = waiting;
            egui::TopBottomPanel::bottom("console_stdin")
                .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(2, 4)))
                .show_inside(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Le chevron passe à l'orange quand le programme est
                        // effectivement suspendu sur un `read` : c'est le seul
                        // signe visible que c'est à l'élève de jouer.
                        ui.label(
                            RichText::new("❯")
                                .monospace()
                                .color(if waiting { action() } else { gutter_col() }),
                        );
                        let hint = i18n::tr3(
                            self.lang,
                            "Entrée envoyée au programme…",
                            "Input sent to the program…",
                            "Entrada enviada al programa…",
                        );
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.stdin_input)
                                .id(super::stdin_id())
                                .desired_width(f32::INFINITY)
                                .font(egui::TextStyle::Monospace)
                                .hint_text(hint),
                        );
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.send_stdin();
                            resp.request_focus();
                        }
                        // Le programme vient de se suspendre sur son `read` :
                        // l'élève n'a qu'à taper.
                        if claim_focus {
                            resp.request_focus();
                        }
                    });
                });
        }
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
        } else if self.dbg.as_ref().is_some_and(|d| d.is_waiting()) {
            // Suspendu dans un appel système : ptrace n'a pas la main, et le
            // dire vaut mieux que de laisser croire à un panneau en panne.
            tr("édition suspendue (le programme attend une entrée)", "editing paused (the program is waiting for input)", "edición en pausa (el programa espera una entrada)")
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
                            .color(if kb_sel { super::accent() } else { hdr });
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
                                egui::Stroke::new(1.0_f32, super::accent()),
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
                                    lerp_color(changed_col().linear_multiply(0.22), flash_bright().linear_multiply(0.55), blink)
                                } else {
                                    changed_color(flash).linear_multiply(0.22)
                                }
                            } else {
                                ui.visuals().faint_bg_color
                            };
                            let t = RichText::new(format!("0x{val:016X}")).monospace();
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    // Bordure clignotante autour du chip modifié ;
                                    // au repos, un liseré discret comme les cartes de
                                    // flags, pour que registres et flags forment une
                                    // même famille visuelle.
                                    let stroke = if changed && blink > 0.0 {
                                        egui::Stroke::new(1.0 + 1.4 * blink, lerp_color(changed_col(), flash_bright(), blink))
                                    } else if changed {
                                        egui::Stroke::new(1.0_f32, changed_color(flash))
                                    } else {
                                        egui::Stroke::new(1.0_f32, hdr.linear_multiply(0.22))
                                    };
                                    if editable {
                                        let chip = egui::Button::new(t)
                                            .fill(bg)
                                            .stroke(stroke)
                                            .corner_radius(egui::CornerRadius::same(7))
                                            .min_size(egui::vec2(0.0, 22.0));
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
                                            .corner_radius(egui::CornerRadius::same(7))
                                            .inner_margin(egui::Margin::symmetric(8, 4))
                                            .show(ui, |ui| {
                                                ui.label(t);
                                            });
                                    }
                                    // Flèche directionnelle + delta chiffré, clignotants.
                                    if changed && ped {
                                        let up = val > pval;
                                        let base = if up { push_col() } else { pop_col() };
                                        let col = lerp_color(base, flash_bright(), blink);
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
                Err(e) => {
                    let msg = e.message(lang);
                    self.log(&msg);
                }
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

        // La bande CPU du dock peut être très basse. L'ancien plafond de trois
        // cartes imposait deux lignes, dont la seconde était coupée sans barre
        // de défilement : c'était le « glitch » visible dans Flags. Six cartes
        // compactes tiennent sur une ligne quand la largeur le permet, et le
        // ScrollArea conserve l'accès aux lignes restantes dans un panneau étroit.
        let cols = ((ui.available_width() / 130.0).floor() as usize).clamp(1, 6);
        egui::ScrollArea::vertical()
            .id_salt("flags_cards_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(3.0);
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
                                (flag_on().linear_multiply(0.16), flag_on().linear_multiply(0.55), flag_on())
                            } else {
                                (c[j].visuals().faint_bg_color, flag_off().linear_multiply(0.30), flag_off())
                            };
                            egui::Frame::new()
                                .fill(fill)
                                .stroke(egui::Stroke::new(1.0_f32, stroke_col))
                                .corner_radius(egui::CornerRadius::same(7))
                                .inner_margin(egui::Margin::symmetric(7, 5))
                                .show(&mut c[j], |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(
                                                RichText::new(*name)
                                                    .monospace()
                                                    .strong()
                                                    .size(14.0)
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
                                                        .size(18.0)
                                                        .color(accent),
                                                );
                                            },
                                        );
                                    });
                                });
                        }
                    });
                    ui.add_space(5.0);
                }
            });
    }

    // ---------- Explorateur de fichiers (panneau de gauche) ----------

    /// L'arbre tel qu'il s'affiche : les dossiers dépliés, aplatis dans l'ordre
    /// des lignes.
    ///
    /// Le clavier et la souris parcourent désormais la MÊME liste. C'est ce qui
    /// manquait : ↑/↓ ne voyaient que la racine pendant que la souris atteignait
    /// toute la profondeur dépliée, et la sélection semblait sauter au hasard.
    fn explorer_rows(&self) -> Vec<std::path::PathBuf> {
        explorer_entries(&self.explorer_expanded, &self.explorer_dir)
            .into_iter()
            .map(|entry| entry.path)
            .collect()
    }

    /// Déplie un dossier replié, replie un dossier déplié.
    pub(super) fn toggle_explorer_expanded(&mut self, path: &std::path::Path) {
        if !self.explorer_expanded.remove(path) {
            self.explorer_expanded.insert(path.to_path_buf());
        }
    }

    /// Déplace la sélection clavier d'une ligne dans l'arbre affiché.
    pub(super) fn move_explorer_selection(&mut self, down: bool) {
        let entries = self.explorer_rows();
        if entries.is_empty() {
            return;
        }
        let cur = self
            .explorer_selected
            .as_ref()
            .or(Some(&self.src_path))
            .and_then(|p| entries.iter().position(|entry| entry == p));
        let next = match (cur, down) {
            (None, _) => 0,
            (Some(i), true) => (i + 1).min(entries.len() - 1),
            (Some(i), false) => i.saturating_sub(1),
        };
        self.explorer_selected = Some(entries[next].clone());
        self.scroll_to_sel = Some(super::dock::Panel::Explorer);
    }

    /// ←/→ dans l'arbre : replier ou déplier le dossier retenu, et à défaut
    /// remonter au dossier parent ou descendre sur son premier enfant.
    ///
    /// C'est le geste attendu d'un arbre, et le seul moyen de déplier sans
    /// viser le chevron à la souris.
    pub(super) fn slide_explorer_selection(&mut self, right: bool) {
        let Some(sel) = self.explorer_selected.clone() else {
            self.move_explorer_selection(true);
            return;
        };
        let is_dir = sel.is_dir();
        let open = self.explorer_expanded.contains(&sel);
        if right {
            if is_dir && !open {
                self.explorer_expanded.insert(sel);
            } else if is_dir {
                // Déjà déplié : la flèche descend sur le premier enfant, qui
                // suit immédiatement son dossier dans la liste aplatie.
                let rows = self.explorer_rows();
                if let Some(i) = rows.iter().position(|p| *p == sel)
                    && let Some(next) = rows.get(i + 1)
                    && next.starts_with(&sel)
                {
                    self.explorer_selected = Some(next.clone());
                }
            }
        } else if is_dir && open {
            self.explorer_expanded.remove(&sel);
        } else if let Some(parent) = sel.parent()
            && parent != self.explorer_dir
            && parent.starts_with(&self.explorer_dir)
        {
            self.explorer_selected = Some(parent.to_path_buf());
        }
        self.scroll_to_sel = Some(super::dock::Panel::Explorer);
    }

    pub(super) fn explorer_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);

        // --- En-tête : la racine à gauche, les actions à droite ---
        //
        // Dans ce sens, un nom de dossier long s'élide au lieu de repousser les
        // boutons hors du panneau — un explorateur est souvent la colonne la
        // plus étroite de la fenêtre.
        let mut go_up = false;
        let mut new_file = false;
        let mut new_folder = false;
        let mut collapse_all = false;
        let root_label = self
            .explorer_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.explorer_dir.display().to_string());
        let root_path = self.explorer_dir.display().to_string();
        panel_header(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                collapse_all = self
                    .tip(ui.small_button("⊟"), tr("Tout replier", "Collapse all", "Contraer todo"))
                    .clicked();
                new_folder = self
                    .tip(
                        ui.small_button("🗀"),
                        tr("Nouveau dossier", "New folder", "Nueva carpeta"),
                    )
                    .clicked();
                new_file = self
                    .tip(
                        ui.small_button("✚"),
                        tr(
                            "Nouveau fichier dans ce dossier — le format (ELF ou PE) est demandé",
                            "New file in this folder — the format (ELF or PE) is asked for",
                            "Archivo nuevo en esta carpeta — se pregunta el formato (ELF o PE)",
                        ),
                    )
                    .clicked();
                go_up = self
                    .tip(
                        ui.small_button("⬆"),
                        tr("Dossier parent comme racine", "Parent folder as root", "Carpeta padre como raíz"),
                    )
                    .clicked();
                ui.add_space(2.0);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    self.tip(
                        ui.add(
                            egui::Label::new(
                                RichText::new(&root_label).strong().color(self.c_header()),
                            )
                            .truncate(),
                        ),
                        &root_path,
                    );
                });
            });
        });
        if go_up && let Some(p) = self.explorer_dir.parent() {
            self.explorer_dir = p.to_path_buf();
            self.explorer_selected = None;
        }
        if collapse_all {
            self.explorer_expanded.clear();
        }
        if new_file {
            let dir = self.explorer_dir.clone();
            self.new_file_in(dir);
        }
        if new_folder {
            self.begin_explorer_new_folder(self.explorer_dir.clone());
        }

        // --- L'arbre ---
        let theme = crate::theme::current();
        let colors = ExplorerRowColors {
            asm: self.c_mnemonic(),
            other: self.c_bytes(),
            text: theme.ui.text,
            // Un dossier ambre, un fichier assembleur de la couleur des
            // mnémoniques, le reste éteint : le type se lit à l'icône.
            folder: theme.ui.action,
            sel_bg: self.c_sel_row(),
            hover_bg: ui.visuals().widgets.hovered.weak_bg_fill.gamma_multiply(0.5),
            sel_fg: theme.ui.text_strong,
            accent: theme.ui.accent,
            dim: theme.ui.off,
        };
        let labels = ExplorerRowLabels {
            open: tr("Ouvrir", "Open", "Abrir"),
            expand: tr("Déplier / replier", "Expand / collapse", "Desplegar / contraer"),
            set_root: tr(
                "Ouvrir ce dossier comme racine",
                "Open this folder as root",
                "Abrir esta carpeta como raíz",
            ),
            new_file: tr("Nouveau fichier ici", "New file here", "Archivo nuevo aquí"),
            new_folder: tr("Nouveau dossier ici", "New folder here", "Nueva carpeta aquí"),
            rename: tr("Renommer  (F2)", "Rename  (F2)", "Renombrar  (F2)"),
            copy_path: tr("Copier le chemin", "Copy path", "Copiar la ruta"),
            delete: tr("Supprimer…  (Suppr)", "Delete…  (Del)", "Eliminar…  (Supr)"),
        };
        // Le repère suit la sélection clavier quand il y en a une, sinon le
        // fichier ouvert : sans cela, les flèches déplaceraient un curseur
        // invisible.
        let selected = self.explorer_selected.clone().unwrap_or_else(|| self.src_path.clone());
        let open_in_editor = self.src_path.clone();
        let path_tip = self.show_tooltips;
        let scroll_here = self.take_scroll_request(super::dock::Panel::Explorer);
        let renaming = self.explorer_renaming.clone();
        // `take` : l'aplatissement a besoin des dossiers dépliés pendant que les
        // lignes empruntent `self` en écriture (le champ de renommage). L'état
        // est rendu intact juste après.
        let expanded = std::mem::take(&mut self.explorer_expanded);
        let entries = explorer_entries(&expanded, &self.explorer_dir);
        let mut explorer_action = None;
        let mut renaming_is_visible = false;
        // Les lignes se touchent : `show_rows` espace les rangées de
        // `item_spacing.y`, et les huit points de blanc du reste de l'interface
        // transformeraient l'arbre en liste flottante — bandes de survol
        // détachées, traits de retrait pointillés.
        ui.spacing_mut().item_spacing.y = 0.0;
        // Arbre virtualisé : au défilement, `show_rows` ne rend que la tranche
        // visible au lieu de reconstruire l'intégralité de l'arborescence.
        egui::ScrollArea::vertical()
            .id_salt("explorer_scroll")
            .auto_shrink([false, false])
            .show_rows(ui, EXPLORER_ROW_H, entries.len(), |ui, rows| {
                for index in rows {
                    let entry = &entries[index];
                    let renaming_this = renaming.as_ref() == Some(&entry.path);
                    renaming_is_visible |= renaming_this;
                    let marks = ExplorerRowMarks {
                        selected: entry.path == selected,
                        open_in_editor: entry.path == open_in_editor,
                        scroll_to: scroll_here && entry.path == selected,
                        path_tip,
                    };
                    let rename = renaming_this.then_some(ExplorerRename {
                        input: &mut self.explorer_rename_input,
                        focus: &mut self.explorer_rename_focus,
                    });
                    if let Some(action) = explorer_row(ui, entry, &marks, rename, &labels, &colors)
                    {
                        explorer_action = Some(action);
                    }
                }
            });
        self.explorer_expanded = expanded;
        if entries.is_empty() {
            ui.weak(tr("Ce dossier est vide.", "This folder is empty.", "Esta carpeta está vacía."));
        }
        // La ligne en cours de renommage a quitté l'arbre (dossier replié,
        // racine changée, défilement) : on valide, comme un clic ailleurs, au
        // lieu de laisser une saisie invisible bloquée.
        if renaming.is_some() && !renaming_is_visible && self.explorer_renaming == renaming {
            self.finish_explorer_rename();
        }
        match explorer_action {
            Some(ExplorerAction::Open(path)) => {
                self.explorer_selected = Some(path.clone());
                self.open_file(path);
            }
            Some(ExplorerAction::Select(path)) => self.explorer_selected = Some(path),
            Some(ExplorerAction::Toggle(path)) => {
                self.explorer_selected = Some(path.clone());
                self.toggle_explorer_expanded(&path);
            }
            Some(ExplorerAction::Navigate(path)) => {
                self.explorer_dir = path;
                self.explorer_selected = None;
            }
            Some(ExplorerAction::BeginRename(path)) => self.begin_explorer_rename(path),
            Some(ExplorerAction::CommitRename) => self.finish_explorer_rename(),
            Some(ExplorerAction::CancelRename) => self.cancel_explorer_rename(),
            Some(ExplorerAction::Delete(path)) => self.explorer_delete = Some(path),
            Some(ExplorerAction::NewFolderIn(dir)) => self.begin_explorer_new_folder(dir),
            Some(ExplorerAction::NewFileIn(dir)) => self.new_file_in(dir),
            Some(ExplorerAction::CopyPath(path)) => {
                let path = path.display().to_string();
                ui.ctx().copy_text(path.clone());
                self.status = format!("{} {path}", tr("Chemin copié :", "Path copied:", "Ruta copiada:"));
            }
            None => {}
        }
        self.explorer_dialogs(ui.ctx());
    }

    /// Les opérations avec nom ou suppression ont une confirmation explicite,
    /// mais le renommage reste directement dans la ligne de l'arbre.
    fn explorer_dialogs(&mut self, ctx: &egui::Context) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        if self.explorer_new_folder {
            let parent = self
                .explorer_new_folder_parent
                .clone()
                .unwrap_or_else(|| self.explorer_dir.clone());
            // Le nom est jugé AVANT d'être peint : la boîte peut alors dire
            // pourquoi elle refuse, au lieu de laisser cliquer « Créer » pour
            // n'afficher l'échec qu'ensuite, dans la console.
            let name = self.explorer_new_folder_input.trim().to_string();
            let problem = if name.is_empty() {
                None
            } else if !Self::valid_explorer_name(&name) {
                Some(tr(
                    "Un nom simple, sans / ni \\.",
                    "One simple name, without / or \\.",
                    "Un nombre simple, sin / ni \\.",
                ))
            } else if parent.join(&name).exists() {
                Some(tr(
                    "Ce nom est déjà pris dans ce dossier.",
                    "That name is already taken in this folder.",
                    "Ese nombre ya existe en esta carpeta.",
                ))
            } else {
                None
            };
            let can_create = !name.is_empty() && problem.is_none();
            let mut open = true;
            let mut create = false;
            let mut cancel = false;
            dialog_window(ctx, tr("Nouveau dossier", "New folder", "Nueva carpeta"))
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.set_width(380.0);
                    ui.label(tr("Dans", "In", "En"));
                    ui.label(RichText::new(parent.display().to_string()).monospace().weak());
                    ui.add_space(8.0);
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.explorer_new_folder_input)
                            .id(super::explorer_new_folder_id())
                            .desired_width(f32::INFINITY)
                            .hint_text(tr("Nom du dossier", "Folder name", "Nombre de la carpeta")),
                    );
                    // Le focus une seule fois, à l'ouverture : le redemander à
                    // chaque image empêcherait le champ de le perdre, donc
                    // Entrée de se voir, et le volerait à tout le reste.
                    if std::mem::take(&mut self.explorer_new_folder_focus) {
                        response.request_focus();
                    }
                    ui.add_space(4.0);
                    match problem {
                        Some(msg) => {
                            ui.label(RichText::new(msg).color(false_col()));
                        }
                        None => {
                            ui.label(RichText::new(" ").small());
                        }
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(can_create, egui::Button::new(tr("Créer", "Create", "Crear")))
                            .clicked()
                            || (can_create
                                && response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        {
                            create = true;
                        }
                        if ui.button(tr("Annuler", "Cancel", "Cancelar")).clicked() {
                            cancel = true;
                        }
                    });
                });
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                cancel = true;
            }
            if create {
                self.create_explorer_folder();
            }
            if !open || cancel {
                self.explorer_new_folder = false;
                self.explorer_new_folder_parent = None;
            }
        }
        if let Some(path) = self.explorer_delete.clone() {
            let mut confirm = false;
            let mut cancel = false;
            dialog_window(ctx, tr("Supprimer ?", "Delete?", "¿Eliminar?"))
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(if path.is_dir() {
                        tr("Le dossier et tout son contenu seront supprimés.", "The folder and all of its contents will be deleted.", "La carpeta y todo su contenido se eliminarán.")
                    } else {
                        tr("Ce fichier sera supprimé.", "This file will be deleted.", "Este archivo se eliminará.")
                    });
                    ui.label(RichText::new(path.display().to_string()).monospace().weak());
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new(tr("Supprimer", "Delete", "Eliminar")).color(false_col())).clicked() {
                            confirm = true;
                        }
                        if ui.button(tr("Annuler", "Cancel", "Cancelar")).clicked() {
                            cancel = true;
                        }
                    });
                });
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                cancel = true;
            }
            if confirm {
                self.explorer_delete = None;
                self.delete_explorer_entry(path);
            } else if cancel {
                self.explorer_delete = None;
            }
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
                line(ui, RichText::new(format!("#{depth}  0x{rip:08X}  {}", tr("(courant)", "(current)", "(actual)"))).monospace().color(changed_col()));
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
                        Some(r) if r < 0 => false_col(),
                        Some(_) => flag_on(),
                        None => self.c_bytes(),
                    };
                    // Le journal reste compact — nom, numéro, arguments bruts —
                    // mais l'effet de l'appel est à un survol de là : relire
                    // « write(fd=1, …) » trois écrans plus bas ne dit toujours
                    // pas ce qui s'est passé.
                    let hover = {
                        let d = crate::syscall::describe(&s.regs, self.lang);
                        match &d.note {
                            Some(n) => format!("{}\n\n⚠ {n}", d.summary),
                            None => d.summary,
                        }
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
                        )
                        .on_hover_text(hover.as_str());
                        ui.label(
                            RichText::new(format!("#{}", s.number))
                                .monospace()
                                .small()
                                .color(self.c_bytes()),
                        )
                        .on_hover_text(hover.as_str());
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
                        )
                        .on_hover_text(hover.as_str());
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
                ui.label(RichText::new("⬇ PUSH").strong().color(changed_color2(flash, push_col())));
            } else if rsp > prsp {
                ui.label(RichText::new("⬆ POP").strong().color(changed_color2(flash, pop_col())));
            } else {
                ui.label("");
            }
        }

        let prev_stack = self.prev_snap().map(|p| p.stack).unwrap_or_default();
        let blink = self.blink_intensity(ui);
        let ped = self.pedagogy_anim;
        // Décalage horizontal du glissement : la case fraîchement empilée arrive
        // depuis la droite (PUSH) ou repart vers la droite (POP).
        let slide = self.blink_progress(ui).map(|p| (1.0 - p) * 26.0).unwrap_or(0.0);
        let pushed = self.prev_snap().is_some_and(|p| rsp < p.regs.rsp);
        let addr_c = self.c_addr();
        let hdr = self.c_header();
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
                    push_col()
                } else if addr == rbp {
                    action()
                } else {
                    self.c_bytes().gamma_multiply(0.5)
                };
                // Case modifiée : fond clignotant.
                let fill = if changed && blink > 0.0 {
                    lerp_color(changed_col().linear_multiply(0.18), flash_bright().linear_multiply(0.45), blink)
                } else if changed {
                    changed_color(flash).linear_multiply(0.18)
                } else if addr == rsp || addr == rbp {
                    ui.visuals().faint_bg_color
                } else {
                    Color32::TRANSPARENT
                };
                // Le sommet de pile glisse pendant l'animation (effet empilement).
                let dx = if ped && is_top && pushed { slide } else { 0.0 };
                // Chaque case est une carte : coin arrondi, liseré teinté par le
                // rôle (sommet / cadre / corps) — même famille visuelle que les
                // registres et les flags.
                let stroke_col = if changed {
                    changed_color(flash)
                } else if addr == rsp || addr == rbp {
                    bar_col
                } else {
                    hdr.linear_multiply(0.18)
                };
                ui.horizontal(|ui| {
                    ui.add_space(dx);
                    egui::Frame::new()
                        .fill(fill)
                        .stroke(egui::Stroke::new(1.0_f32, stroke_col.linear_multiply(0.6)))
                        .corner_radius(egui::CornerRadius::same(7))
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                // Barre de rôle, à gauche dans la carte.
                                let (bar, _) = ui.allocate_exact_size(egui::vec2(4.0, 18.0), egui::Sense::hover());
                                ui.painter().rect_filled(bar, 2.0, bar_col);
                                ui.add_space(3.0);
                                ui.label(RichText::new(format!("0x{addr:012X}")).monospace().small().color(addr_c));
                                let mut vt = RichText::new(format!("0x{val:016X}")).monospace();
                                if changed {
                                    vt = vt.color(if blink > 0.0 {
                                        lerp_color(changed_col(), flash_bright(), blink)
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
                                // Marqueurs de rôle alignés à droite de la carte.
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    // Rôle de la case dans le cadre d'appel : ce qui
                                    // transforme une colonne d'adresses en structure.
                                    if let Some(kind) = crate::abi::classify_slot(addr, rbp) {
                                        let (txt, col) = match kind {
                                            crate::abi::SlotKind::ReturnAddress => (kind.label(lang), false_col()),
                                            crate::abi::SlotKind::SavedFramePointer => (kind.label(lang), action()),
                                            _ => (kind.label(lang), self.c_bytes()),
                                        };
                                        ui.label(RichText::new(txt).small().italics().color(col));
                                    }
                                    if !marker.is_empty() {
                                        ui.label(
                                            RichText::new(marker)
                                                .monospace()
                                                .small()
                                                .strong()
                                                .color(if addr == rsp { push_col() } else { action() }),
                                        );
                                    }
                                });
                            });
                        });
                });
                ui.add_space(3.0);
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

    // ---------- Format du binaire ----------

    /// Panneau FORMAT : ce qu'est devenu le source une fois assemblé.
    ///
    /// Même présentation pour un ELF et pour un PE — c'est le propos : les deux
    /// formats répondent aux mêmes questions (par où l'exécution commence, quel
    /// morceau est du code, lequel est modifiable, ce qui vient d'ailleurs), et
    /// l'élève qui a compris l'un a compris l'autre aux trois quarts.
    pub(super) fn format_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let hdr = self.c_header();
        let addr_c = self.c_addr();
        let Some(info) = self.format_info.clone() else {
            ui.weak(tr(
                "Assemblez un programme (F5) pour examiner le binaire produit.",
                "Assemble a program (F5) to inspect the binary produced.",
                "Ensamble un programa (F5) para examinar el binario producido.",
            ));
            return;
        };

        egui::ScrollArea::vertical().id_salt("format_scroll").auto_shrink([false, false]).show(ui, |ui| {
            super::card(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&info.format).monospace().strong().size(16.0).color(super::accent()));
                    ui.label(RichText::new(format!("· {} · {}", info.arch, info.kind)).weak());
                });
                ui.add_space(2.0);
                egui::Grid::new("format_head").num_columns(2).spacing([14.0, 3.0]).show(ui, |ui| {
                    ui.label(RichText::new(tr("Point d'entrée", "Entry point", "Punto de entrada")).small().weak());
                    ui.label(RichText::new(format!("0x{:X}", info.entry)).monospace().color(addr_c))
                        .on_hover_text(tr(
                            "La première instruction exécutée : le système y place RIP après avoir chargé l'image.",
                            "The first instruction executed: the system puts RIP there once the image is loaded.",
                            "La primera instrucción ejecutada: el sistema coloca RIP allí tras cargar la imagen.",
                        ));
                    ui.end_row();
                    if info.image_base != 0 {
                        ui.label(RichText::new(tr("Base d'image", "Image base", "Base de imagen")).small().weak());
                        ui.label(RichText::new(format!("0x{:X}", info.image_base)).monospace().color(addr_c));
                        ui.end_row();
                    }
                    ui.label(RichText::new(tr("Taille du fichier", "File size", "Tamaño del archivo")).small().weak());
                    ui.label(RichText::new(format!("{} {}", info.file_size, tr("octets", "bytes", "bytes"))).monospace());
                    ui.end_row();
                });
                // Ce panneau répond aux questions qu'ASM Studio sait poser d'un
                // binaire ; Desdec pose les autres. Le bouton est ici plutôt
                // qu'à la barre d'outils parce que c'est ici qu'on regarde le
                // fichier produit, et que la question vient en le regardant.
                ui.add_space(4.0);
                if ui
                    .button(tr("Ouvrir dans Desdec ↗", "Open in Desdec ↗", "Abrir en Desdec ↗"))
                    .on_hover_text(tr(
                        "Réassemble, puis ouvre le binaire dans Desdec : entropie, chaînes, fonctions, désassemblage complet. Desdec s'installe à part.",
                        "Re-assembles, then opens the binary in Desdec: entropy, strings, functions, full disassembly. Desdec is installed separately.",
                        "Reensambla y abre el binario en Desdec: entropía, cadenas, funciones, desensamblado completo. Desdec se instala aparte.",
                    ))
                    .clicked()
                {
                    self.send_to_desdec();
                }
            });

            ui.add_space(6.0);
            ui.label(RichText::new(tr("Sections", "Sections", "Secciones")).small().strong().color(hdr));
            egui::Grid::new("format_sections").num_columns(5).striped(true).spacing([12.0, 3.0]).show(ui, |ui| {
                for c in [
                    tr("nom", "name", "nombre"),
                    tr("adresse", "address", "dirección"),
                    tr("en mémoire", "in memory", "en memoria"),
                    tr("dans le fichier", "in the file", "en el archivo"),
                    tr("droits", "perms", "permisos"),
                ] {
                    ui.label(RichText::new(c).small().weak());
                }
                ui.end_row();
                for s in &info.sections {
                    ui.label(RichText::new(&s.name).monospace().strong()).on_hover_text(&s.role);
                    ui.label(RichText::new(if s.address == 0 { "—".to_string() } else { format!("0x{:X}", s.address) }).monospace().color(addr_c));
                    ui.label(RichText::new(format!("{}", s.size)).monospace());
                    // Zéro octet sur le disque et de la place en mémoire : c'est
                    // .bss, et c'est la ligne qui fait poser la question.
                    let file = if s.file_size == 0 && s.size > 0 {
                        RichText::new("0").monospace().color(super::accent())
                    } else {
                        RichText::new(format!("{}", s.file_size)).monospace()
                    };
                    ui.label(file);
                    ui.label(RichText::new(&s.perms).monospace().color(if s.perms.contains('x') { super::flag_on() } else { ui.visuals().text_color() }));
                    ui.end_row();
                }
            });

            if !info.imports.is_empty() {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(tr("Fonctions importées", "Imported functions", "Funciones importadas"))
                        .small()
                        .strong()
                        .color(hdr),
                );
                let mut lib = String::new();
                for imp in &info.imports {
                    if imp.library != lib {
                        lib = imp.library.clone();
                        ui.label(RichText::new(&lib).monospace().weak());
                    }
                    ui.label(RichText::new(format!("    {}", imp.name)).monospace());
                }
            }

            if !info.symbols.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Symboles globaux", "Global symbols", "Símbolos globales")).small().strong().color(hdr));
                for (name, addr) in info.symbols.iter().take(64) {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("0x{addr:X}")).monospace().color(addr_c));
                        ui.label(RichText::new(name).monospace());
                    });
                }
            }

            for note in &info.notes {
                ui.add_space(6.0);
                super::card(ui, |ui| {
                    ui.label(RichText::new(note).small());
                });
            }
        });
    }

    // ---------- SSE / x87 ----------

    /// Panneau SSE / FPU : les seize registres XMM et la pile x87.
    ///
    /// Le tutoriel enseigne `movdqa xmm0, [rel a]` et `paddd xmm0, xmm1` depuis
    /// toujours ; jusqu'ici, l'élève exécutait ces instructions sans pouvoir
    /// regarder le seul endroit où le résultat se trouvait. Le panneau montre
    /// chaque registre dans la lecture qu'en fait l'instruction (deux `double`,
    /// quatre entiers, seize octets…), avec la même pulsation « CPU vivant » que
    /// les registres généraux pour signaler ce qui vient de changer.
    pub(super) fn simd_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let Some(snap) = self.snap() else {
            ui.weak(tr("Aucun programme lancé.", "No program running.", "Ningún programa en ejecución."));
            return;
        };
        let Some(fp) = snap.fp.clone() else {
            ui.weak(tr(
                "Registres flottants indisponibles pour ce processus.",
                "Floating-point registers unavailable for this process.",
                "Registros de coma flotante no disponibles para este proceso.",
            ));
            return;
        };
        // L'état précédent sert uniquement à repérer ce qui a bougé.
        let prev = self.prev_snap().and_then(|s| s.fp.clone());
        let flash = self.flash_progress(ui);
        let hdr = self.c_header();
        let view = self.xmm_view;

        panel_header(ui, |ui| {
            ui.label(RichText::new(tr("Vue", "View", "Vista")).small().weak());
            egui::ComboBox::from_id_salt("xmm_view")
                .selected_text(view.label())
                .width(78.0)
                .show_ui(ui, |ui| {
                    for v in XmmView::ALL {
                        ui.selectable_value(&mut self.xmm_view, v, v.label())
                            .on_hover_text(v.hint(lang));
                    }
                });
            ui.checkbox(
                &mut self.simd_hide_zero,
                RichText::new(tr("masquer les nuls", "hide zeroed", "ocultar los nulos")).small(),
            )
            .on_hover_text(tr(
                "Un programme n'utilise presque jamais les seize registres : masquer ceux qui valent zéro laisse voir ceux qui travaillent.",
                "A program almost never uses all sixteen registers: hiding the zeroed ones leaves only those doing work.",
                "Un programa casi nunca usa los dieciséis registros: ocultar los que valen cero deja ver los que trabajan.",
            ));
        });
        let view = self.xmm_view; // relu : la combo a pu changer la vue à l'instant

        egui::ScrollArea::vertical().id_salt("simd_scroll").auto_shrink([false, false]).show(ui, |ui| {
            let mut shown = 0usize;
            for i in 0..16 {
                let v = fp.xmm[i];
                let changed = prev.as_ref().is_some_and(|p| p.xmm[i] != v);
                if self.simd_hide_zero && simd::is_zero(v) && !changed {
                    continue;
                }
                shown += 1;
                let name_col = if changed { changed_color(flash) } else { hdr };
                ui.horizontal_top(|ui| {
                    ui.add_sized(
                        [46.0, 18.0],
                        egui::Label::new(RichText::new(format!("XMM{i}")).monospace().strong().color(name_col)),
                    )
                    .on_hover_text(format!("{:032X}", v));
                    // Les cases séparées par « │ » : l'élève voit du premier coup
                    // combien de valeurs le registre porte dans cette lecture.
                    let cells = simd::lanes(v, view);
                    ui.label(
                        RichText::new(cells.join(" │ "))
                            .monospace()
                            .color(if changed { changed_color(flash) } else { ui.visuals().text_color() }),
                    );
                });
                shown = shown.max(1);
            }
            if shown == 0 {
                ui.weak(tr(
                    "Les seize registres XMM sont à zéro — ce programme n'a pas encore touché au calcul vectoriel.",
                    "All sixteen XMM registers are zero — this program has not touched vector maths yet.",
                    "Los dieciséis registros XMM están a cero — este programa aún no ha tocado el cálculo vectorial.",
                ));
            }

            // Pile x87 : montrée seulement si elle contient quelque chose. Un
            // programme x86-64 moderne ne s'en sert plus, et huit lignes vides
            // n'apprendraient rien à celui qui n'en aura jamais besoin.
            let occupied = (0..8).filter(|i| fp.st_reg(*i).2).count();
            if occupied > 0 {
                ui.add_space(8.0);
                ui.label(RichText::new(tr("Pile x87", "x87 stack", "Pila x87")).small().strong().color(hdr));
                for i in 0..8 {
                    let (phys, raw, used) = fp.st_reg(i);
                    if !used {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [46.0, 18.0],
                            egui::Label::new(RichText::new(format!("ST({i})")).monospace().strong().color(hdr)),
                        )
                        .on_hover_text(format!(
                            "{} R{phys} — TOP = {}",
                            tr("registre physique", "physical register", "registro físico"),
                            fp.top()
                        ));
                        ui.label(RichText::new(simd::fmt_f64(simd::st_to_f64(raw))).monospace());
                    });
                }
            }

            // MXCSR : arrondi et exceptions levées. Les exceptions sont
            // collantes — le dire évite de croire que la dernière instruction
            // vient de diviser par zéro.
            ui.add_space(8.0);
            let flags = simd::exception_flags((fp.mxcsr & 0x3F) as u16, lang);
            let raised: Vec<&str> = flags.iter().filter(|f| f.set).map(|f| f.name).collect();
            ui.label(
                RichText::new(format!(
                    "MXCSR 0x{:04X} — {} : {}",
                    fp.mxcsr,
                    tr("arrondi", "rounding", "redondeo"),
                    simd::rounding_mode(((fp.mxcsr >> 13) & 0b11) as u8, lang)
                ))
                .monospace()
                .small()
                .weak(),
            );
            if raised.is_empty() {
                ui.label(
                    RichText::new(tr("Aucune exception flottante levée.", "No floating-point exception raised.", "Ninguna excepción de coma flotante levantada."))
                        .small()
                        .weak(),
                );
            } else {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(tr("Exceptions :", "Exceptions:", "Excepciones:")).small().weak());
                    for f in flags.iter().filter(|f| f.set) {
                        ui.label(RichText::new(f.name).monospace().small().color(flag_on()))
                            .on_hover_text(format!(
                                "{}\n{}",
                                f.meaning,
                                tr(
                                    "Drapeau collant : levé depuis le début du programme, pas forcément par la dernière instruction.",
                                    "Sticky flag: raised at some point since the program started, not necessarily by the last instruction.",
                                    "Bandera pegajosa: levantada en algún momento desde el inicio del programa, no necesariamente por la última instrucción."
                                )
                            ));
                    }
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Un bouton dont le glyphe manque s'affiche en carré vide, et l'élève ne
    /// peut pas deviner ce qu'il ouvre. Le contrôle porte sur les polices
    /// *par défaut* d'egui, sans les polices système ajoutées par
    /// `install_fallback_font` : elles, on ne les a pas sur toutes les machines.
    #[test]
    fn the_console_output_button_has_a_real_glyph() {
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |_| {});
        let has = ctx.fonts_mut(|f| {
            f.has_glyphs(&egui::FontId::proportional(15.0), CONSOLE_OUTPUT_ICON)
        });
        assert!(has, "{CONSOLE_OUTPUT_ICON} n'a pas de glyphe : il s'afficherait en tofu");
    }

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

    // ---------- Explorateur ----------

    /// Un dossier de travail vide, propre à chaque test.
    fn explorer_sandbox(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("asmstudio-explorateur-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dossier de test");
        dir
    }

    /// Rend l'explorateur une image, en lui remettant les événements donnés.
    fn render_explorer(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) {
        let input = egui::RawInput { events, ..Default::default() };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.set_max_width(240.0);
                app.explorer_ui(ui);
            });
        });
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// LE défaut signalé : on tapait le nouveau nom, on appuyait sur Entrée, et
    /// rien ne se passait.
    ///
    /// La cause était un `request_focus()` redemandé à CHAQUE image : le champ
    /// ne perdait donc jamais le focus, `lost_focus()` restait faux, et la
    /// validation n'arrivait jamais. Le test tient les deux bouts — le focus ne
    /// se demande qu'une fois, et Entrée renomme pour de bon sur le disque.
    #[test]
    fn enter_validates_a_rename_in_the_tree() {
        let dir = explorer_sandbox("entree");
        let before = dir.join("avant.asm");
        std::fs::write(&before, "; rien\n").unwrap();

        let mut app = App::new();
        app.explorer_dir = dir.clone();
        app.src_path = before.clone();
        app.begin_explorer_rename(before.clone());
        app.explorer_rename_input = "apres.asm".to_string();

        let ctx = egui::Context::default();
        render_explorer(&ctx, &mut app, Vec::new());
        assert!(
            !app.explorer_rename_focus,
            "la demande de focus est à usage unique, sinon Entrée ne se voit jamais"
        );
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(super::super::explorer_rename_id()),
            "le champ de renommage doit avoir pris le focus"
        );

        render_explorer(&ctx, &mut app, vec![key(egui::Key::Enter)]);
        assert!(dir.join("apres.asm").is_file(), "Entrée doit renommer le fichier");
        assert!(!before.exists(), "l'ancien nom ne doit plus exister");
        assert_eq!(app.explorer_renaming, None, "la saisie se referme après validation");
        assert_eq!(app.src_path, dir.join("apres.asm"), "le fichier ouvert suit son nom");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Échap abandonne : le fichier garde son nom.
    #[test]
    fn escape_abandons_a_rename() {
        let dir = explorer_sandbox("echap");
        let file = dir.join("garde-moi.asm");
        std::fs::write(&file, "; rien\n").unwrap();

        let mut app = App::new();
        app.explorer_dir = dir.clone();
        app.begin_explorer_rename(file.clone());
        app.explorer_rename_input = "autre.asm".to_string();

        let ctx = egui::Context::default();
        render_explorer(&ctx, &mut app, Vec::new());
        render_explorer(&ctx, &mut app, vec![key(egui::Key::Escape)]);

        assert!(file.is_file(), "Échap ne renomme rien");
        assert!(!dir.join("autre.asm").exists());
        assert_eq!(app.explorer_renaming, None, "la saisie se referme quand même");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// L'autre moitié du même défaut : tant qu'un renommage était ouvert, le
    /// champ reprenait le focus à chaque image — donc à tout autre champ que
    /// l'on venait de cliquer. C'est ce qui rendait la souris « erratique »
    /// partout ailleurs dans l'application.
    #[test]
    fn a_rename_never_steals_the_focus_back() {
        let dir = explorer_sandbox("focus");
        let file = dir.join("un.asm");
        std::fs::write(&file, "; rien\n").unwrap();

        let mut app = App::new();
        app.explorer_dir = dir.clone();
        app.begin_explorer_rename(file.clone());

        let ctx = egui::Context::default();
        render_explorer(&ctx, &mut app, Vec::new());

        // L'utilisateur clique ailleurs : un autre champ prend le focus.
        let elsewhere = egui::Id::new("un_autre_champ");
        ctx.memory_mut(|m| m.request_focus(elsewhere));
        let mut other = String::new();
        let input = egui::RawInput::default();
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                app.explorer_ui(ui);
                ui.add(egui::TextEdit::singleline(&mut other).id(elsewhere));
            });
        });

        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(elsewhere),
            "l'explorateur ne doit pas reprendre le focus au champ que l'on vient de cliquer"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Pendant la frappe du nouveau nom, les flèches, F2 et Suppr doivent
    /// appartenir au champ et non à l'arbre : le champ est donc déclaré dans
    /// [`text_inputs`], avec le panneau auquel il appartient.
    #[test]
    fn typing_a_new_name_is_typing_not_navigating() {
        let mut app = App::new();
        app.focus_panel(super::super::dock::Panel::Explorer);
        let ctx = egui::Context::default();
        ctx.memory_mut(|m| m.request_focus(super::super::explorer_rename_id()));
        assert!(
            app.typing_in_focused_panel(&ctx),
            "le champ de renommage doit compter comme une saisie"
        );
    }

    /// Le clavier doit parcourir l'arbre TEL QU'IL EST AFFICHÉ : un dossier
    /// déplié se traverse, ses fichiers compris. Avant, ↑/↓ ne voyaient que la
    /// racine et sautaient par-dessus tout ce que la souris avait déplié.
    #[test]
    fn arrows_walk_the_expanded_tree() {
        let dir = explorer_sandbox("arbre");
        let sub = dir.join("atelier");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("dedans.asm"), "").unwrap();
        std::fs::write(dir.join("zzz.asm"), "").unwrap();

        let mut app = App::new();
        app.explorer_dir = dir.clone();
        app.explorer_selected = Some(sub.clone());

        // Replié : après le dossier vient le fichier de la racine.
        app.move_explorer_selection(true);
        assert_eq!(app.explorer_selected.as_deref(), Some(dir.join("zzz.asm").as_path()));

        // Déplié : c'est l'enfant qui vient juste après.
        app.explorer_selected = Some(sub.clone());
        app.slide_explorer_selection(true);
        assert!(app.explorer_expanded.contains(&sub), "→ déplie le dossier retenu");
        app.move_explorer_selection(true);
        assert_eq!(app.explorer_selected.as_deref(), Some(sub.join("dedans.asm").as_path()));

        // ← depuis un enfant remonte à son dossier, puis le replie.
        app.slide_explorer_selection(false);
        assert_eq!(app.explorer_selected.as_deref(), Some(sub.as_path()));
        app.slide_explorer_selection(false);
        assert!(!app.explorer_expanded.contains(&sub), "← replie le dossier retenu");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Renommer un dossier déplié ne doit pas le replier : l'état est indexé
    /// par chemin, et le chemin vient de changer.
    #[test]
    fn renaming_a_folder_keeps_it_expanded() {
        let dir = explorer_sandbox("deplie");
        let sub = dir.join("avant");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("x.asm"), "").unwrap();

        let mut app = App::new();
        app.explorer_dir = dir.clone();
        app.explorer_expanded.insert(sub.clone());
        app.begin_explorer_rename(sub.clone());
        app.explorer_rename_input = "apres".to_string();
        app.finish_explorer_rename();

        assert!(dir.join("apres").is_dir(), "le dossier doit être renommé");
        assert!(
            app.explorer_expanded.contains(&dir.join("apres")),
            "et rester déplié sous son nouveau nom"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Un nom refusé garde la ligne en édition ET lui rend le focus : sans
    /// cela, la ligne restait éditable mais plus rien n'écoutait la frappe.
    #[test]
    fn a_refused_name_keeps_the_field_alive() {
        let dir = explorer_sandbox("refus");
        let file = dir.join("un.asm");
        std::fs::write(&file, "").unwrap();

        let mut app = App::new();
        app.explorer_dir = dir.clone();
        app.begin_explorer_rename(file.clone());
        app.explorer_rename_input = "sous/dossier.asm".to_string();
        app.explorer_rename_focus = false;
        app.finish_explorer_rename();

        assert_eq!(app.explorer_renaming, Some(file.clone()), "la saisie reste ouverte");
        assert!(app.explorer_rename_focus, "et reprend le focus");
        assert!(file.is_file(), "rien n'a bougé sur le disque");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Un dossier créé depuis le clic droit d'un sous-dossier naît CHEZ LUI, et
    /// le déplie pour qu'on le voie apparaître.
    #[test]
    fn a_new_folder_lands_in_the_folder_that_asked_for_it() {
        let dir = explorer_sandbox("creation");
        let sub = dir.join("atelier");
        std::fs::create_dir(&sub).unwrap();

        let mut app = App::new();
        app.explorer_dir = dir.clone();
        app.begin_explorer_new_folder(sub.clone());
        app.explorer_new_folder_input = "exercices".to_string();
        app.create_explorer_folder();

        assert!(sub.join("exercices").is_dir(), "le dossier naît dans celui visé");
        assert!(app.explorer_expanded.contains(&sub), "son parent se déplie");
        assert!(!app.explorer_new_folder, "la boîte se referme");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Les quatre boutons de la barre de l'explorateur doivent avoir un glyphe
    /// dans les polices par défaut d'egui : sans cela, ils s'affichent en carré
    /// vide et plus personne ne devine ce qu'ils font.
    #[test]
    fn the_explorer_toolbar_buttons_have_real_glyphs() {
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |_| {});
        for icon in ["⬆", "✚", "🗀", "⊟"] {
            let has =
                ctx.fonts_mut(|f| f.has_glyphs(&egui::FontId::proportional(14.0), icon));
            assert!(has, "{icon} n'a pas de glyphe : il s'afficherait en tofu");
        }
    }

    /// L'arbre se rend dans une colonne étroite, à plusieurs niveaux de
    /// profondeur et avec un thème quelconque, sans rien faire paniquer.
    #[test]
    fn the_explorer_renders_deep_and_narrow_in_every_theme() {
        let dir = explorer_sandbox("rendu");
        let deep = dir.join("un").join("deux").join("trois");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("un-nom-de-fichier-vraiment-tres-long.asm"), "").unwrap();
        std::fs::write(dir.join("racine.o"), "").unwrap();

        let mut app = App::new();
        app.explorer_dir = dir.clone();
        app.explorer_expanded.insert(dir.join("un"));
        app.explorer_expanded.insert(dir.join("un").join("deux"));
        app.explorer_expanded.insert(deep.clone());

        let _guard = crate::theme::lock_for_test();
        let ctx = egui::Context::default();
        for theme in crate::theme::THEMES.iter() {
            crate::theme::set_current(theme);
            render_explorer(&ctx, &mut app, Vec::new());
        }
        crate::theme::set_current(&crate::theme::THEMES[0]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Peint l'explorateur avec `selected` en surbrillance et renvoie
    /// (zone du panneau, bande de sélection).
    fn explorer_selection_band(app: &mut App, selected: &std::path::Path) -> (egui::Rect, egui::Rect) {
        // La couleur cherchée vient du thème, global au processus.
        let _theme = crate::theme::lock_for_test();
        app.explorer_selected = Some(selected.to_path_buf());
        let sel = app.c_sel_row();
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(260.0, 500.0))),
            ..Default::default()
        };
        let mut avail = egui::Rect::ZERO;
        let out = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                avail = ui.max_rect();
                app.explorer_ui(ui);
            });
        });
        let band = out
            .shapes
            .iter()
            .find_map(|cs| match &cs.shape {
                egui::Shape::Rect(r) if r.fill == sel => Some(r.rect),
                _ => None,
            })
            .expect("la ligne sélectionnée doit porter sa bande");
        (avail, band)
    }

    /// La bande de sélection couvre TOUTE la largeur du panneau, et les lignes
    /// se touchent : `show_rows` espace les rangées de `item_spacing.y`, et les
    /// huit points de blanc du reste de l'interface auraient transformé l'arbre
    /// en liste flottante.
    #[test]
    fn explorer_rows_are_full_width_and_contiguous() {
        let dir = explorer_sandbox("bandes");
        std::fs::write(dir.join("a.asm"), "").unwrap();
        std::fs::write(dir.join("b.asm"), "").unwrap();

        let mut app = App::new();
        app.explorer_dir = dir.clone();

        let (avail, first) = explorer_selection_band(&mut app, &dir.join("a.asm"));
        assert!(
            (first.left() - avail.left()).abs() < 1.0 && (first.right() - avail.right()).abs() < 1.0,
            "bande large de {} sur {} disponibles",
            first.width(),
            avail.width()
        );
        assert!(
            (first.height() - EXPLORER_ROW_H).abs() < 0.5,
            "hauteur de bande {} au lieu de {EXPLORER_ROW_H}",
            first.height()
        );

        let (_, second) = explorer_selection_band(&mut app, &dir.join("b.asm"));
        assert!(
            (second.top() - first.top() - EXPLORER_ROW_H).abs() < 0.5,
            "les lignes doivent se toucher : pas de {} pour une hauteur de {EXPLORER_ROW_H}",
            second.top() - first.top()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// L'explorateur se parcourt au clavier et s'arrête aux bornes, comme le
    /// désassemblage — sinon le panneau resterait pilotable à la souris seule.
    #[test]
    fn explorer_selection_moves_and_clamps() {
        let mut app = App::new();
        // Dossier d'exemples réel : il contient plusieurs entrées.
        app.explorer_dir = super::super::abs_dir_of(std::path::Path::new("examples/test.asm"));
        let (dirs, files) = super::super::list_entries(&app.explorer_dir);
        let entries: Vec<_> = dirs.into_iter().chain(files).collect();
        assert!(entries.len() >= 2, "il faut au moins deux entrées pour tester");

        app.explorer_selected = None;
        app.move_explorer_selection(true);
        let first = app.explorer_selected.clone();
        assert!(first.is_some(), "une première sélection doit apparaître");

        // On descend jusqu'au bout, puis une fois de trop.
        for _ in 0..entries.len() + 3 {
            app.move_explorer_selection(true);
        }
        assert_eq!(
            app.explorer_selected.as_ref(),
            entries.last(),
            "la descente doit s'arrêter à la dernière entrée"
        );

        // Et on remonte jusqu'au premier, puis une fois de trop.
        for _ in 0..entries.len() + 3 {
            app.move_explorer_selection(false);
        }
        assert_eq!(
            app.explorer_selected.as_ref(),
            entries.first(),
            "la remontée doit s'arrêter à la première entrée"
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

    /// Le panneau SSE/FPU se rend pour de vrai, avant et après lancement, et
    /// dans chacune de ses vues. Une grille mal fermée ou un index de case hors
    /// bornes ne se voit qu'au rendu — pas en compilant.
    #[test]
    fn the_simd_panel_renders_in_every_view() {
        let ctx = egui::Context::default();
        let mut app = App::new();

        // Sans programme lancé : le panneau doit le dire, pas paniquer.
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.simd_ui(ui));
        });

        app.src_path = PathBuf::from("build/simd-ui/simd.asm");
        app.out_dir = PathBuf::from("build/simd-ui");
        app.source = std::fs::read_to_string("examples/simd.asm").expect("exemple SIMD");
        std::fs::create_dir_all("build/simd-ui").expect("dossier");
        app.launch();
        for _ in 0..12 {
            app.step();
        }
        assert!(
            app.snap().and_then(|s| s.fp.clone()).is_some(),
            "le snapshot doit porter les registres flottants"
        );
        for view in crate::simd::XmmView::ALL {
            app.xmm_view = view;
            for hide in [true, false] {
                app.simd_hide_zero = hide;
                let _ = ctx.run(Default::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| app.simd_ui(ui));
                });
            }
        }
    }

    /// Le panneau FORMAT se rend pour les deux formats — c'est justement son
    /// propos de les montrer côte à côte.
    #[test]
    fn the_format_panel_renders_for_elf_and_pe() {
        let ctx = egui::Context::default();
        let mut app = App::new();

        // Avant tout assemblage : une invite, pas un panneau vide.
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.format_ui(ui));
        });

        std::fs::create_dir_all("build/fmt-ui").expect("dossier");
        app.out_dir = PathBuf::from("build/fmt-ui");
        for (target, source) in [
            (
                crate::assemble::Target::Linux,
                "section .text\n global _start\n_start:\n mov rax,60\n xor rdi,rdi\n syscall\n".to_string(),
            ),
            (
                crate::assemble::Target::Windows,
                std::fs::read_to_string("examples/hello-windows.asm").expect("exemple Windows"),
            ),
        ] {
            app.target = target;
            app.src_path = PathBuf::from("build/fmt-ui/prog.asm");
            app.source = source;
            app.build();
            let info = app.format_info.as_ref().expect("le binaire doit être décrit");
            assert!(!info.sections.is_empty(), "{target:?} : sections attendues");
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.format_ui(ui));
            });
        }
    }
}
