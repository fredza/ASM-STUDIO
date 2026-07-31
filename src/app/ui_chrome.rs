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

        // Ctrl+Maj+P ouvre la palette. C'est la porte d'entrée du clavier :
        // egui n'ouvre pas ses menus au clavier, la palette les remplace.
        if ctx.input(|i| i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(Key::P)) {
            self.open_palette();
            return;
        }
        // Palette ouverte : elle capte tout, sinon Échap arrêterait le
        // programme et les flèches piloteraient la timeline en arrière-plan.
        if self.palette_open {
            return;
        }
        // Ignore les raccourcis d'action quand l'éditeur a le focus (sauf Ctrl+*).
        let (step, run, stop, build, save, open, new, first, prev, next, last) = ctx.input(|i| {
            let c = i.modifiers.ctrl;
            (
                i.key_pressed(Key::F10) || i.key_pressed(Key::F8),
                i.key_pressed(Key::F5),
                i.modifiers.shift && i.key_pressed(Key::F5),
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
        // Échap : d'abord sortir du champ de saisie, sinon arrêter le programme.
        // Sans cette priorité, un utilisateur au clavier reste piégé dans l'éditeur.
        let esc = ctx.input(|i| i.key_pressed(Key::Escape));
        let focused = ctx.memory(|m| m.focused().is_some());
        if esc && focused {
            ctx.memory_mut(|m| m.surrender_focus(m.focused().unwrap()));
        } else if stop || esc {
            self.stop();
        }

        // --- Navigation clavier ---
        // F6 / Maj+F6 : panneau suivant / précédent de la disposition.
        // Ctrl+F6 : retour direct à l'éditeur, où que l'on soit.
        let (f6, f6_back, f6_editor) = ctx.input(|i| {
            let p = i.key_pressed(Key::F6);
            (p && !i.modifiers.shift && !i.modifiers.ctrl, p && i.modifiers.shift, p && i.modifiers.ctrl)
        });
        if f6_editor {
            self.focus_panel(super::dock::Panel::Editor);
            ctx.memory_mut(|m| m.request_focus(super::editor_id()));
        } else if f6 || f6_back {
            self.focus_next_panel(f6_back);
            if self.focused_panel() == Some(super::dock::Panel::Editor) {
                ctx.memory_mut(|m| m.request_focus(super::editor_id()));
            }
        }
        // Quitter l'éditeur au clavier : relâcher son focus, sinon il continue
        // d'avaler les touches destinées au panneau nouvellement visé.
        if std::mem::take(&mut self.ctx_surrender_focus)
            && let Some(id) = ctx.memory(|m| m.focused())
        {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
        // Ctrl+Tab / Ctrl+Maj+Tab : fait défiler les onglets du centre.
        let (tab_next, tab_prev) = ctx.input(|i| {
            let t = i.modifiers.ctrl && i.key_pressed(Key::Tab);
            (t && !i.modifiers.shift, t && i.modifiers.shift)
        });
        if tab_next || tab_prev {
            self.cycle_tab(tab_prev);
        }
        if step {
            self.step();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
            self.show_shortcuts = true;
        }
        // Affichage : Ctrl+1..5 bascule un panneau de la disposition.
        use super::dock::Panel;
        let quick: [(egui::Key, Panel); 4] = [
            (Key::Num1, Panel::Explorer),
            (Key::Num2, Panel::Instruction),
            (Key::Num3, Panel::Registers),
            (Key::Num4, Panel::Memory),
        ];
        let mut toggled = false;
        for (key, panel) in quick {
            if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(key)) {
                self.toggle_panel(panel);
                toggled = true;
            }
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(Key::Num5)) {
            self.pedagogy_predict = !self.pedagogy_predict;
            toggled = true;
        }
        if toggled {
            self.save_settings();
        }

        // Échap : d'abord sortir du champ de saisie, sinon arrêter le programme.
        // Sans cette priorité, un utilisateur au clavier reste piégé dans l'éditeur.
        let esc = ctx.input(|i| i.key_pressed(Key::Escape));
        let focused = ctx.memory(|m| m.focused().is_some());
        if esc && focused {
            ctx.memory_mut(|m| m.surrender_focus(m.focused().unwrap()));
        } else if stop || esc {
            self.stop();
        }

        // --- Navigation clavier ---
        // F6 : ramène le focus dans l'éditeur (point d'entrée du clavier).
        if ctx.input(|i| i.key_pressed(Key::F6)) {
            self.focus_panel(super::dock::Panel::Editor);
            ctx.memory_mut(|m| m.request_focus(super::editor_id()));
        }
        // Ctrl+Tab / Ctrl+Maj+Tab : fait défiler les onglets du centre.
        let (tab_next, tab_prev) = ctx.input(|i| {
            let t = i.modifiers.ctrl && i.key_pressed(Key::Tab);
            (t && !i.modifiers.shift, t && i.modifiers.shift)
        });
        if tab_next || tab_prev {
            self.cycle_tab(tab_prev);
        }
        if step {
            self.step();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
            self.show_shortcuts = true;
        }
        // Affichage : Ctrl+1..5 bascule chaque panneau.
        // Timeline seulement si l'éditeur n'a pas le focus (évite le conflit ←/→).
        let editing = ctx.memory(|m| m.focused().is_some());

        // ↑/↓ parcourent le désassemblage quand son onglet est actif ; Entrée
        // ouvre le microscope sur l'instruction retenue.
        // Le panneau focalisé décide de ce que font les flèches : chaque liste
        // se parcourt au clavier, Entrée valide.
        if !editing {
            let (up, down, enter) = ctx.input(|i| {
                (
                    i.key_pressed(Key::ArrowUp),
                    i.key_pressed(Key::ArrowDown),
                    i.key_pressed(Key::Enter),
                )
            });
            match self.focused_panel() {
                Some(super::dock::Panel::Disasm) if !self.disasm.is_empty() => {
                    if up || down {
                        self.move_disasm_selection(down);
                    }
                    if enter && let Some(a) = self.selected {
                        self.microscope = Some(a);
                    }
                }
                Some(super::dock::Panel::Explorer) => {
                    if up || down {
                        self.move_explorer_selection(down);
                    }
                    if enter && let Some(f) = self.explorer_selected.clone() {
                        self.open_file(f);
                    }
                }
                Some(super::dock::Panel::Registers) => {
                    if up || down {
                        self.move_reg_selection(down);
                    }
                    // ←/→ traversent la ligne ; la timeline garde ces touches
                    // pour les autres panneaux.
                    let (left, right) = ctx.input(|i| {
                        (i.key_pressed(Key::ArrowLeft), i.key_pressed(Key::ArrowRight))
                    });
                    if left || right {
                        self.move_reg_selection_sideways(right);
                    }
                    if enter {
                        self.edit_selected_register();
                    }
                }
                _ => {}
            }
        }

        // Ctrl+W ferme le panneau focalisé — pendant clavier du bouton ✕ de
        // l'onglet, qui n'était atteignable qu'à la souris.
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(Key::W))
            && let Some(p) = self.focused_panel()
        {
            self.hide_panel(p);
            self.save_settings();
        }

        // La timeline garde ←/→ sauf si le panneau focalisé les utilise.
        let arrows_taken = self.focused_panel() == Some(super::dock::Panel::Registers);
        if self.dbg.is_some() && !editing && !arrows_taken {
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
                    ui.label(
                        RichText::new(tr(
                            "Glissez un onglet pour le déplacer, l'empiler ou le détacher.",
                            "Drag a tab to move, stack or detach it.",
                            "Arrastre una pestaña para moverla, apilarla o desacoplarla.",
                        ))
                        .small()
                        .weak(),
                    );
                    ui.add_space(3.0);
                    // Une case par panneau : cochée = présent quelque part dans
                    // la disposition (ancré ou en fenêtre).
                    let mut toggle: Option<super::dock::Panel> = None;
                    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                        for p in super::dock::Panel::ALL {
                            let mut open = self.panel_is_open(p);
                            if ui.checkbox(&mut open, p.title(lang)).changed() {
                                toggle = Some(p);
                            }
                        }
                    });
                    if let Some(p) = toggle {
                        self.toggle_panel(p);
                        self.save_settings();
                    }

                    ui.separator();
                    ui.label(RichText::new(tr("Fenêtres", "Windows", "Ventanas")).small().weak());
                    if ui
                        .checkbox(&mut self.pedagogy_predict, tr("Prédiction              Ctrl+5", "Prediction              Ctrl+5", "Predicción              Ctrl+5"))
                        .changed()
                    {
                        self.save_settings();
                    }

                    ui.separator();
                    if ui.button(tr("Tout afficher", "Show all", "Mostrar todo")).clicked() {
                        for p in super::dock::Panel::ALL {
                            if !self.panel_is_open(p) {
                                self.show_panel(p);
                            }
                        }
                        self.save_settings();
                        ui.close_menu();
                    }
                    if ui
                        .button(tr("Réinitialiser la disposition", "Reset layout", "Restablecer disposición"))
                        .on_hover_text(tr(
                            "Remet les panneaux à leur place d'origine.",
                            "Puts every panel back in its original place.",
                            "Devuelve cada panel a su lugar original.",
                        ))
                        .clicked()
                    {
                        self.reset_dock_layout();
                        ui.close_menu();
                    }
                });
                ui.menu_button(tr("Aide", "Help", "Ayuda"), |ui| {
                    if ui
                        .button(tr("Palette de commandes…  Ctrl+Maj+P", "Command palette…  Ctrl+Shift+P", "Paleta de comandos…  Ctrl+May+P"))
                        .on_hover_text(tr(
                            "Toutes les actions de l'application, au clavier.",
                            "Every action in the application, from the keyboard.",
                            "Todas las acciones de la aplicación, desde el teclado.",
                        ))
                        .clicked()
                    {
                        self.open_palette();
                        ui.close_menu();
                    }
                    ui.separator();
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
                // À droite : position curseur, encodage, syntaxe, et surtout le
                // panneau qui a le focus clavier — le repère qui manquait pour
                // savoir où l'on se trouve après un F6.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("NASM").color(ACCENT).strong());
                    ui.separator();
                    match &self.focused_panel_name {
                        Some(name) => {
                            ui.label(
                                RichText::new(format!("⌨ {name}"))
                                    .color(ACCENT)
                                    .strong(),
                            )
                            .on_hover_text(tr(
                                "Panneau focalisé — F6 pour le suivant, Maj+F6 pour le précédent",
                                "Focused panel — F6 for the next one, Shift+F6 for the previous",
                                "Panel enfocado — F6 para el siguiente, Mayús+F6 para el anterior",
                            ));
                        }
                        None => {
                            ui.label(RichText::new(tr("⌨ F6", "⌨ F6", "⌨ F6")).weak())
                                .on_hover_text(tr(
                                    "Aucun panneau focalisé — appuyez sur F6",
                                    "No focused panel — press F6",
                                    "Ningún panel enfocado — pulse F6",
                                ));
                        }
                    }
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
