use eframe::egui::{self, Color32, RichText};

use crate::i18n;
use crate::debugger::RunState;

use super::{
    App, ACCENT, FLAG_ON, FLAG_OFF, FALSE_COL, CHANGED,
    accent_button, bordered_button, icon_button, icon_btn_widget,
};

impl App {
    // ---------- Raccourcis ----------

    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::Key;
        // Ignore les raccourcis d'action quand l'éditeur a le focus (sauf Ctrl+*).
        let (step, run, stop, build, save, open, new, first, prev, next, last) = ctx.input(|i| {
            let c = i.modifiers.ctrl;
            (
                i.key_pressed(Key::F10) || i.key_pressed(Key::F8),
                i.key_pressed(Key::F5),
                i.key_pressed(Key::Escape) || (i.modifiers.shift && i.key_pressed(Key::F5)),
                c && i.key_pressed(Key::B),
                c && i.key_pressed(Key::S),
                c && i.key_pressed(Key::O),
                c && i.key_pressed(Key::N),
                i.key_pressed(Key::Home),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                i.key_pressed(Key::End),
            )
        });
        if save {
            self.save_source();
        }
        if open {
            self.open_browser();
        }
        if new {
            self.new_file();
        }
        if build {
            self.build();
        }
        if run {
            self.launch();
        }
        if stop {
            self.stop();
        }
        if step {
            self.step();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
            self.show_shortcuts = true;
        }
        // Affichage : Ctrl+1..4 bascule chaque panneau.
        let (t_expl, t_instr, t_cpu, t_bottom) = ctx.input(|i| {
            let c = i.modifiers.ctrl;
            (
                c && i.key_pressed(Key::Num1),
                c && i.key_pressed(Key::Num2),
                c && i.key_pressed(Key::Num3),
                c && i.key_pressed(Key::Num4),
            )
        });
        if t_expl {
            self.show_explorer = !self.show_explorer;
        }
        if t_instr {
            self.show_instruction = !self.show_instruction;
        }
        if t_cpu {
            self.show_cpu_band = !self.show_cpu_band;
        }
        if t_bottom {
            self.show_bottom_band = !self.show_bottom_band;
        }
        if t_expl || t_instr || t_cpu || t_bottom {
            self.save_settings();
        }
        // Timeline seulement si l'éditeur n'a pas le focus (évite le conflit ←/→).
        let editing = ctx.memory(|m| m.focused().is_some());
        if self.dbg.is_some() && !editing {
            if first {
                self.set_view(0);
            }
            if prev {
                self.set_view(self.view_index as i64 - 1);
            }
            if next {
                self.set_view(self.view_index as i64 + 1);
            }
            if last {
                self.set_view(i64::MAX);
            }
        }
    }

    /// Applique le thème choisi (Système / Sombre / Clair) + le style moderne.
    pub(super) fn apply_theme(&mut self, ctx: &egui::Context) {
        use egui::{FontId, Rounding, Theme, ThemePreference, TextStyle, vec2};
        let dark = match self.theme_pref {
            ThemePreference::Dark => true,
            ThemePreference::Light => false,
            ThemePreference::System => {
                ctx.input(|i| i.raw.system_theme) != Some(Theme::Light)
            }
        };
        self.dark = dark; // pour la palette de texte sensible au thème
        let mut style = (*ctx.style()).clone();
        let mut v = if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        v.window_rounding = Rounding::same(8.0);
        v.menu_rounding = Rounding::same(6.0);
        v.selection.bg_fill = ACCENT.linear_multiply(0.45);
        v.hyperlink_color = ACCENT;
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
            &mut v.widgets.noninteractive,
        ] {
            w.rounding = Rounding::same(5.0);
        }
        if dark {
            v.panel_fill = Color32::from_rgb(0x1E, 0x1E, 0x22);
            v.window_fill = Color32::from_rgb(0x25, 0x25, 0x2B);
            v.extreme_bg_color = Color32::from_rgb(0x17, 0x17, 0x1B);
            v.faint_bg_color = Color32::from_rgb(0x28, 0x28, 0x30);
        } else {
            // Thème clair : texte par défaut nettement sombre pour le contraste,
            // et fonds légèrement teintés pour délimiter les panneaux.
            v.override_text_color = Some(Color32::from_rgb(0x1C, 0x20, 0x28));
            v.panel_fill = Color32::from_rgb(0xF4, 0xF5, 0xF8);
            v.window_fill = Color32::from_rgb(0xFB, 0xFB, 0xFD);
            v.extreme_bg_color = Color32::from_rgb(0xFF, 0xFF, 0xFF);
            v.faint_bg_color = Color32::from_rgb(0xEA, 0xEC, 0xF1);
            v.hyperlink_color = Color32::from_rgb(0x1B, 0x5E, 0xA8);
        }
        style.visuals = v;
        style.spacing.item_spacing = vec2(8.0, 6.0);
        style.spacing.button_padding = vec2(9.0, 4.0);
        // Barres de défilement « solides » (réservent leur largeur) plutôt que
        // flottantes : elles ne se dessinent plus par-dessus le contenu.
        style.spacing.scroll = egui::style::ScrollStyle::solid();
        style.text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
        style.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
        style.text_styles.insert(TextStyle::Monospace, FontId::monospace(13.0));
        style.text_styles.insert(TextStyle::Heading, FontId::proportional(18.0));
        style.text_styles.insert(TextStyle::Small, FontId::proportional(11.0));
        ctx.set_style(style);
    }

    // ---------- Menu ----------

    pub(super) fn menu_bar(&mut self, ctx: &egui::Context) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        egui::TopBottomPanel::top("menubar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button(tr("Fichier", "File", "Archivo"), |ui| {
                    if ui.button(tr("Nouveau            Ctrl+N", "New                Ctrl+N", "Nuevo               Ctrl+N")).clicked() {
                        self.new_file();
                        ui.close_menu();
                    }
                    if ui.button(tr("Ouvrir…            Ctrl+O", "Open…              Ctrl+O", "Abrir…              Ctrl+O")).clicked() {
                        self.open_browser();
                        ui.close_menu();
                    }
                    if ui.button(tr("Enregistrer        Ctrl+S", "Save               Ctrl+S", "Guardar            Ctrl+S")).clicked() {
                        self.save_source();
                        ui.close_menu();
                    }
                    if ui.button(tr("Enregistrer sous…", "Save As…", "Guardar como…")).clicked() {
                        self.open_saveas();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(tr("Quitter", "Quit", "Salir")).clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Build", |ui| {
                    if ui.button(tr("Assembler + Lier   Ctrl+B", "Assemble + Link    Ctrl+B", "Ensamblar + Enlazar   Ctrl+B")).clicked() {
                        self.build();
                        ui.close_menu();
                    }
                    if ui.button(tr("Exécuter (Lancer)  F5", "Run                F5", "Ejecutar           F5")).clicked() {
                        self.launch();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Debug", |ui| {
                    if ui.button(tr("Lancer / Restart   F5", "Run / Restart      F5", "Ejecutar / Reiniciar  F5")).clicked() {
                        self.launch();
                        ui.close_menu();
                    }
                    if ui.button(tr("Pas à pas          F10", "Step               F10", "Paso a paso         F10")).clicked() {
                        self.step();
                        ui.close_menu();
                    }
                    if ui.button(tr("Stop               Échap", "Stop               Esc", "Detener             Esc")).clicked() {
                        self.stop();
                        ui.close_menu();
                    }
                });
                ui.menu_button(tr("Affichage", "View", "Vista"), |ui| {
                    ui.label(RichText::new(tr("Panneaux", "Panels", "Paneles")).small().weak());
                    let mut changed = false;
                    changed |= ui.checkbox(&mut self.show_explorer, tr("Explorateur          Ctrl+1", "Explorer             Ctrl+1", "Explorador          Ctrl+1")).changed();
                    changed |= ui.checkbox(&mut self.show_instruction, tr("Instruction          Ctrl+2", "Instruction          Ctrl+2", "Instrucción          Ctrl+2")).changed();
                    changed |= ui.checkbox(&mut self.show_cpu_band, tr("Bande CPU (registres…)  Ctrl+3", "CPU band (registers…)   Ctrl+3", "Banda CPU (registros…)  Ctrl+3")).changed();
                    changed |= ui.checkbox(&mut self.show_bottom_band, tr("Bande basse (mémoire…)  Ctrl+4", "Bottom band (memory…)   Ctrl+4", "Banda inferior (memoria…)  Ctrl+4")).changed();
                    ui.separator();
                    if ui.button(tr("Tout afficher", "Show all", "Mostrar todo")).clicked() {
                        self.show_explorer = true;
                        self.show_instruction = true;
                        self.show_cpu_band = true;
                        self.show_bottom_band = true;
                        changed = true;
                        ui.close_menu();
                    }
                    if changed {
                        self.save_settings();
                    }
                });
                ui.menu_button(tr("Aide", "Help", "Ayuda"), |ui| {
                    if ui.button(tr("Réglages…", "Settings…", "Configuración…")).clicked() {
                        self.show_settings = true;
                        ui.close_menu();
                    }
                    if ui.button(tr("Raccourcis clavier…", "Keyboard shortcuts…", "Accesos directos…")).clicked() {
                        self.show_shortcuts = true;
                        ui.close_menu();
                    }
                    if ui.button(tr("Calculatrice…", "Calculator…", "Calculadora…")).clicked() {
                        self.show_calculator = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(tr("Vérifier les mises à jour…", "Check for updates…", "Buscar actualizaciones…")).clicked() {
                        self.updater.check();
                        ui.close_menu();
                    }
                    #[cfg(debug_assertions)]
                    {
                        if ui.button(
                            egui::RichText::new("🧪 Simuler une mise à jour")
                                .color(egui::Color32::from_rgb(180, 140, 60))
                                .italics(),
                        ).clicked() {
                            self.updater.simulate();
                            ui.close_menu();
                        }
                    }
                    ui.separator();
                    if ui.button(tr("À propos ASM Studio…", "About ASM Studio…", "Acerca de ASM Studio…")).clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                });
            });
        });
    }

    // ---------- Barre d'outils ----------

    pub(super) fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                let lang = self.lang;
                let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
                let running = self.dbg.as_ref().is_some_and(|d| d.is_alive());
                let can_step = self.can_step();
                // Handles clonés (Arc bon marché) => pas d'emprunt de self dans la barre.
                let ic = |f: fn(&super::Icons) -> &egui::TextureHandle| self.icons.as_ref().map(|i| f(i).clone());
                let (ic_run, ic_debug, ic_build) = (ic(|i| &i.run), ic(|i| &i.debug), ic(|i| &i.assembler));
                let (ic_pause, ic_stop) = (ic(|i| &i.pause), ic(|i| &i.stop));
                let (ic_restart, ic_attach) = (ic(|i| &i.restart), ic(|i| &i.attach));

                // Run : accent quand inactif, grisé quand un programme tourne.
                if self
                    .tip(accent_button(ui, ic_run.as_ref(), "Run", !running), tr("Lancer (F5)", "Run (F5)", "Ejecutar (F5)"))
                    .clicked()
                {
                    self.launch();
                }
                // Pause : non implémenté (step-by-step uniquement), toujours grisé.
                ui.add_enabled(false, icon_btn_widget(ic_pause.as_ref(), "Pause"));
                // Next : exécute l'instruction suivante (accent quand disponible).
                if self
                    .tip(accent_button(ui, ic_debug.as_ref(), "Next", can_step), tr("Instruction suivante (F10)", "Next instruction (F10)", "Instrucción siguiente (F10)"))
                    .clicked()
                {
                    self.step();
                }
                // Stop.
                if self.tip(bordered_button(ui, ic_stop.as_ref(), "Stop", running), tr("Arrêter (Échap)", "Stop (Esc)", "Detener (Esc)")).clicked() {
                    self.stop();
                }
                // Restart = relancer depuis le début.
                if self
                    .tip(icon_button(ui, ic_restart.as_ref(), "Restart"), tr("Relancer (F5)", "Restart (F5)", "Reiniciar (F5)"))
                    .clicked()
                {
                    self.launch();
                }
                ui.separator();
                if self
                    .tip(icon_button(ui, ic_build.as_ref(), "Build"), tr("Assembler + Lier (Ctrl+B)", "Assemble + Link (Ctrl+B)", "Ensamblar + Enlazar (Ctrl+B)"))
                    .clicked()
                {
                    self.build();
                }
                // Attach : non implémenté.
                ui.add_enabled(false, icon_btn_widget(ic_attach.as_ref(), "Attach"));
                // (Réglages : accessible via le menu Aide — pas de doublon ici.)
            });
            ui.add_space(3.0);
        });
    }

    // ---------- Barre d'état ----------

    pub(super) fn status_bar(&mut self, ctx: &egui::Context) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let mut kill_requested = false;
        egui::TopBottomPanel::bottom("statusbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                match &self.dbg {
                    Some(d) if d.is_alive() => {
                        ui.colored_label(FLAG_ON, "● Running");
                        ui.separator();
                        // Clic droit sur le PID → menu contextuel Kill.
                        ui.label(format!("PID {}", d.pid()))
                            .on_hover_text(tr(
                                "Clic droit pour Kill · Esc pour Stop",
                                "Right-click to Kill · Esc to Stop",
                                "Clic derecho para Matar · Esc para Detener",
                            ))
                            .context_menu(|ui| {
                                if ui.button(tr("🗙 Kill (SIGKILL)", "🗙 Kill (SIGKILL)", "🗙 Matar (SIGKILL)")).clicked() {
                                    kill_requested = true;
                                    ui.close_menu();
                                }
                            });
                    }
                    Some(d) => match d.state {
                        RunState::Exited(0) => {
                            ui.colored_label(
                                FLAG_ON,
                                RichText::new(format!("✔ {} 0", tr("Exit", "Exit", "Salir"))).strong(),
                            );
                        }
                        RunState::Exited(c) => {
                            ui.colored_label(
                                FALSE_COL,
                                RichText::new(format!("✘ {} {c}", tr("Exit", "Exit", "Salir"))).strong(),
                            );
                        }
                        RunState::Signaled => {
                            ui.colored_label(FALSE_COL, RichText::new(tr("✘ Signal", "✘ Signal", "✘ Señal")).strong());
                        }
                        RunState::Faulted(f) => {
                            ui.colored_label(
                                FALSE_COL,
                                RichText::new(format!("✘ {}", f.signal_name())).strong(),
                            );
                        }
                        RunState::Stopped => {
                            ui.colored_label(FLAG_OFF, format!("○ {}", tr("Arrêté", "Stopped", "Detenido")));
                        }
                    },
                    None => {
                        ui.colored_label(FLAG_OFF, format!("○ {}", tr("Prêt", "Ready", "Listo")));
                    }
                }
                ui.separator();
                ui.label(RichText::new("Arch : x86_64").color(self.c_header()));
                ui.separator();
                ui.label(RichText::new("Mode : 64-bit").color(self.c_header()));
                if let Some(s) = self.snap() {
                    ui.separator();
                    ui.label(format!("{} : 0x{:X}", tr("Arrêté à", "Stopped at", "Detenido en"), s.regs.rip));
                    if let Some(next) = self.next_addr() {
                        ui.separator();
                        ui.colored_label(CHANGED, format!("{} : 0x{next:X}", tr("Suivant", "Next", "Siguiente")));
                    }
                }
                // Dernier message d'action (Enregistré, Build OK, erreurs…).
                if !self.status.is_empty() {
                    ui.separator();
                    ui.label(RichText::new(&self.status).color(self.c_header()));
                }
                // À droite : position curseur, encodage, syntaxe.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("NASM").color(ACCENT).strong());
                    ui.separator();
                    ui.label(RichText::new("UTF-8").color(self.c_header()));
                    ui.separator();
                    ui.label(
                        RichText::new(format!("Ln {}, Col {}", self.editor_ln, self.editor_col))
                            .color(self.c_header()),
                    );
                });
            });
        });
        if kill_requested {
            self.stop();
        }
    }
}
