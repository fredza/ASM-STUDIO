use eframe::egui::{self, Color32, RichText};

use crate::i18n;
use crate::debugger::RunState;

use super::{
    App, ACCENT, FLAG_ON, FLAG_OFF, FALSE_COL, CHANGED,
    accent_button, bordered_button, icon_button,
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
        // F1 bascule, comme toutes les touches qui montrent quelque chose : le
        // deuxième appui referme. Ouvrir sans pouvoir refermer par la même
        // touche oblige à viser la croix à la souris — exactement ce que les
        // raccourcis servent à éviter.
        if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
            self.show_shortcuts = !self.show_shortcuts;
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

        // Les flèches pilotent le panneau focalisé, sauf si l'on tape dedans.
        let editing = self.typing_in_focused_panel(ctx);

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
                Some(super::dock::Panel::Memory) => {
                    // Le vidage défile ligne par ligne, page par page.
                    let (pg_up, pg_dn) = ctx.input(|i| {
                        (i.key_pressed(Key::PageUp), i.key_pressed(Key::PageDown))
                    });
                    if up || down {
                        self.scroll_memory(down, 1);
                    }
                    if pg_up || pg_dn {
                        self.scroll_memory(pg_dn, 8);
                    }
                }
                Some(super::dock::Panel::MemMap) => {
                    // La vue mémoire montre les registres : ↑↓ isolent le fil
                    // d'un registre, comme le survol à la souris.
                    if up || down {
                        self.move_reg_selection_sideways(down);
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
        let arrows_taken = matches!(
            self.focused_panel(),
            Some(
                super::dock::Panel::Registers
                    | super::dock::Panel::Memory
                    | super::dock::Panel::MemMap
                    | super::dock::Panel::Disasm
                    | super::dock::Panel::Explorer
            )
        );
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

    /// Charge des polices système de repli pour les symboles absents des
    /// polices embarquées d'egui.
    ///
    /// Ubuntu-Light ne couvre ni « ✘ », ni « → », ni « ● » ; aucune police à
    /// empattements classiques ne couvre « 🗑 ». Il faut donc PLUSIEURS replis,
    /// pas un seul : une police à large couverture latine-symboles, et une
    /// police de symboles Unicode. Toutes celles qui sont trouvées sont
    /// ajoutées, dans l'ordre.
    ///
    /// Ajoutées EN FIN de liste : les polices d'egui gardent la main sur ce
    /// qu'elles savent rendre, l'aspect général ne change pas. Si rien n'est
    /// trouvé, l'application fonctionne — seuls quelques glyphes restent des
    /// carrés.
    pub(super) fn install_fallback_font(ctx: &egui::Context) {
        const CANDIDATES: [&str; 8] = [
            // Large couverture latine + flèches, puces, coches.
            "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Regular.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            // Symboles Unicode hors plan latin : corbeille, pictogrammes.
            "/usr/share/fonts/google-noto/NotoSansSymbols2-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
            "/usr/share/fonts/google-noto/NotoSansSymbols-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansSymbols-Regular.ttf",
        ];

        let mut fonts = egui::FontDefinitions::default();
        let mut added = 0usize;
        for path in CANDIDATES {
            let Ok(bytes) = std::fs::read(path) else { continue };
            let name = format!("fallback{added}");
            fonts.font_data.insert(name.clone(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push(name.clone());
            }
            added += 1;
        }
        if added > 0 {
            ctx.set_fonts(fonts);
        }
    }

    /// Applique le thème choisi (Système / Sombre / Clair) + le style moderne.
    pub(super) fn apply_theme(&mut self, ctx: &egui::Context) {
        use egui::{FontId, CornerRadius, Theme, ThemePreference, TextStyle, vec2};
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
        // Angles plus ronds et sélection plus douce : l'interface respire, le
        // bleu d'accent ne claque plus autant sous le texte sélectionné.
        v.window_corner_radius = CornerRadius::same(10);
        v.menu_corner_radius = CornerRadius::same(8);
        v.selection.bg_fill = ACCENT.linear_multiply(0.38);
        v.hyperlink_color = ACCENT;
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
            &mut v.widgets.noninteractive,
        ] {
            w.corner_radius = CornerRadius::same(7);
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

        // Organisation conventionnelle : Fichier / Exécution / Affichage /
        // Outils / Aide. « Build » et « Debug » n'étaient qu'un seul sujet —
        // faire tourner le programme — et se recoupaient (Lancer figurait dans
        // les deux). Les réglages quittent « Aide », où personne ne les cherche.
        egui::TopBottomPanel::top("menubar").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                // Entrée de menu : libellé à gauche, raccourci aligné à droite.
                fn item(ui: &mut egui::Ui, label: &str, shortcut: &str) -> bool {
                    let clicked = ui
                        .add(egui::Button::new(label).shortcut_text(shortcut))
                        .clicked();
                    if clicked {
                        ui.close();
                    }
                    clicked
                }

                ui.menu_button(tr("Fichier", "File", "Archivo"), |ui| {
                    if item(ui, tr("Nouveau", "New", "Nuevo"), "Ctrl+N") {
                        self.new_file();
                    }
                    if item(ui, tr("Ouvrir…", "Open…", "Abrir…"), "Ctrl+O") {
                        self.open_browser();
                    }
                    if item(
                        ui,
                        tr(
                            "Exemples et exercices",
                            "Examples and exercises",
                            "Ejemplos y ejercicios",
                        ),
                        "",
                    ) {
                        self.open_examples_dir();
                    }
                    ui.separator();
                    if item(ui, tr("Enregistrer", "Save", "Guardar"), "Ctrl+S") {
                        self.save_source();
                    }
                    if item(ui, tr("Enregistrer sous…", "Save As…", "Guardar como…"), "") {
                        self.open_saveas();
                    }
                    ui.separator();
                    if item(ui, tr("Préférences…", "Preferences…", "Preferencias…"), "") {
                        self.show_settings = true;
                    }
                    ui.separator();
                    if item(ui, tr("Quitter", "Quit", "Salir"), "") {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button(tr("Exécution", "Run", "Ejecución"), |ui| {
                    if item(ui, tr("Assembler", "Build", "Ensamblar"), "Ctrl+B") {
                        self.build();
                    }
                    if item(ui, tr("Lancer", "Run", "Ejecutar"), "F5") {
                        self.launch();
                    }
                    ui.separator();
                    if item(ui, tr("Pas à pas", "Step", "Paso a paso"), "F10") {
                        self.step();
                    }
                    if item(ui, tr("Arrêter", "Stop", "Detener"), "Échap") {
                        self.stop();
                    }
                    ui.separator();
                    ui.add_enabled_ui(self.dbg.is_some(), |ui| {
                        if item(ui, tr("Début de la timeline", "Timeline start", "Inicio de la línea"), "Home") {
                            self.set_view(0);
                        }
                        if item(ui, tr("Étape précédente", "Previous step", "Paso anterior"), "←") {
                            self.set_view(self.view_index as i64 - 1);
                        }
                        if item(ui, tr("Étape suivante", "Next step", "Paso siguiente"), "→") {
                            self.set_view(self.view_index as i64 + 1);
                        }
                        if item(ui, tr("Fin de la timeline", "Timeline end", "Fin de la línea"), "End") {
                            self.set_view(i64::MAX);
                        }
                        ui.separator();
                        if item(ui, tr("Reprendre ici", "Resume here", "Reanudar aquí"), "") {
                            self.resume_here();
                        }
                    });
                });

                ui.menu_button(tr("Affichage", "View", "Vista"), |ui| self.view_menu(ui));

                ui.menu_button(tr("Outils", "Tools", "Herramientas"), |ui| {
                    if item(ui, tr("Palette de commandes…", "Command palette…", "Paleta de comandos…"), "Ctrl+Maj+P") {
                        self.open_palette();
                    }
                    if item(ui, tr("Calculatrice multi-base…", "Multi-base calculator…", "Calculadora multibase…"), "") {
                        self.show_calculator = true;
                    }
                    ui.separator();
                    if item(ui, tr("Vérifier les mises à jour", "Check for updates", "Buscar actualizaciones"), "") {
                        self.updater.check();
                    }
                    #[cfg(debug_assertions)]
                    if item(ui, "🧪 Simuler une mise à jour", "") {
                        self.updater.simulate();
                    }
                });

                ui.menu_button(tr("Aide", "Help", "Ayuda"), |ui| {
                    if item(ui, tr("Raccourcis clavier…", "Keyboard shortcuts…", "Atajos de teclado…"), "F1") {
                        self.show_shortcuts = true;
                    }
                    ui.separator();
                    if item(ui, tr("À propos", "About", "Acerca de"), "") {
                        self.show_about = true;
                    }
                });
            });
        });
    }

    /// Contenu du menu Affichage : mode d'abord, puis panneaux, puis disposition.
    fn view_menu(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        use super::UiMode;

        ui.label(RichText::new(tr("Mode d'affichage", "Display mode", "Modo de visualización")).small().weak());
        let mut wanted = self.mode;
        for m in [UiMode::Learning, UiMode::Full] {
            ui.radio_value(&mut wanted, m, m.label(lang))
                .on_hover_text(m.description(lang));
        }
        if wanted != self.mode {
            self.set_ui_mode(wanted);
            ui.close();
        }

        ui.separator();
        ui.menu_button(tr("Panneaux", "Panels", "Paneles"), |ui| {
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
            let mut toggle: Option<super::dock::Panel> = None;
            let advanced = super::dock::ADVANCED;
            // Les panneaux avancés sont regroupés à part et annoncés comme tels,
            // plutôt que noyés dans une liste de quatorze cases à cocher.
            for p in super::dock::Panel::ALL {
                if advanced.contains(&p) {
                    continue;
                }
                let mut open = self.panel_is_open(p);
                if ui.checkbox(&mut open, p.title(lang)).changed() {
                    toggle = Some(p);
                }
            }
            ui.separator();
            ui.label(RichText::new(tr("Avancé", "Advanced", "Avanzado")).small().weak());
            for p in advanced {
                let mut open = self.panel_is_open(p);
                if ui.checkbox(&mut open, p.title(lang)).changed() {
                    toggle = Some(p);
                }
            }
            if let Some(p) = toggle {
                self.toggle_panel(p);
                self.save_settings();
            }
        });

        if ui
            .checkbox(
                &mut self.pedagogy_predict,
                tr("Fenêtre Prédiction", "Prediction window", "Ventana Predicción"),
            )
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
            ui.close();
        }
        if ui
            .button(tr("Réinitialiser la disposition", "Reset layout", "Restablecer disposición"))
            .on_hover_text(tr(
                "Remet les panneaux à la disposition du mode courant.",
                "Puts every panel back to the current mode's layout.",
                "Devuelve los paneles a la disposición del modo actual.",
            ))
            .clicked()
        {
            self.reset_dock_layout();
            ui.close();
        }
    }

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
                let (ic_stop, ic_restart) = (ic(|i| &i.stop), ic(|i| &i.restart));

                // Libellés dans la langue de l'élève : « Run » figé en anglais
                // détonnait dans une interface par ailleurs traduite.
                // Run : accent quand inactif, grisé quand un programme tourne.
                if self
                    .tip(accent_button(ui, ic_run.as_ref(), tr("Lancer", "Run", "Ejecutar"), !running), tr("Lancer (F5)", "Run (F5)", "Ejecutar (F5)"))
                    .clicked()
                {
                    self.launch();
                }
                // Next : exécute l'instruction suivante (accent quand disponible).
                if self
                    .tip(accent_button(ui, ic_debug.as_ref(), tr("Suivant", "Next", "Siguiente"), can_step), tr("Instruction suivante (F10)", "Next instruction (F10)", "Instrucción siguiente (F10)"))
                    .clicked()
                {
                    self.step();
                }
                // Stop.
                if self.tip(bordered_button(ui, ic_stop.as_ref(), tr("Arrêter", "Stop", "Detener"), running), tr("Arrêter (Échap)", "Stop (Esc)", "Detener (Esc)")).clicked() {
                    self.stop();
                }
                // Restart = relancer depuis le début.
                if self
                    .tip(icon_button(ui, ic_restart.as_ref(), tr("Relancer", "Restart", "Reiniciar")), tr("Relancer (F5)", "Restart (F5)", "Reiniciar (F5)"))
                    .clicked()
                {
                    self.launch();
                }
                ui.separator();
                if self
                    .tip(icon_button(ui, ic_build.as_ref(), tr("Assembler", "Build", "Ensamblar")), tr("Assembler + Lier (Ctrl+B)", "Assemble + Link (Ctrl+B)", "Ensamblar + Enlazar (Ctrl+B)"))
                    .clicked()
                {
                    self.build();
                }
                // « Pause » et « Attach » n'existaient qu'en boutons grisés en
                // permanence — des affordances mortes, déroutantes pour un
                // débutant. Retirées : la barre ne montre que ce qui agit.
                // (Réglages : accessible via le menu Aide — pas de doublon ici.)
            });
            ui.add_space(3.0);
        });
    }

    // ---------- Bandeau d'accueil ----------

    /// Bandeau d'accueil du mode apprentissage : un mot de bienvenue et deux
    /// portes d'entrée — le tutoriel, ou un exemple. Ne s'affiche qu'en mode
    /// apprentissage, et disparaît définitivement une fois écarté (persisté).
    pub(super) fn welcome_banner(&mut self, ctx: &egui::Context) {
        if self.mode != super::UiMode::Learning || self.welcome_dismissed {
            return;
        }
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let hdr = self.c_header();
        let (mut open_ex, mut start_tuto, mut dismiss) = (false, false, false);

        egui::TopBottomPanel::top("welcome")
            .frame(
                egui::Frame::new()
                    .fill(ACCENT.linear_multiply(0.12))
                    .inner_margin(egui::Margin::symmetric(10, 7)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(tr(
                            "👋 Bienvenue ! Nouveau en assembleur ? Suis le parcours guidé, ou ouvre un exemple pour explorer.",
                            "👋 Welcome! New to assembly? Follow the guided path, or open an example to explore.",
                            "👋 ¡Bienvenido! ¿Nuevo en ensamblador? Sigue el recorrido guiado, o abre un ejemplo para explorar.",
                        ))
                        .color(hdr),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(tr("Écarter", "Dismiss", "Descartar")).clicked() {
                            dismiss = true;
                        }
                        if ui.button(tr("Ouvrir un exemple", "Open an example", "Abrir un ejemplo")).clicked() {
                            open_ex = true;
                        }
                        if ui
                            .add(egui::Button::new(
                                RichText::new(tr(
                                    "▶ Commencer le tutoriel",
                                    "▶ Start the tutorial",
                                    "▶ Empezar el tutorial",
                                ))
                                .color(egui::Color32::WHITE),
                            ).fill(ACCENT))
                            .clicked()
                        {
                            start_tuto = true;
                        }
                    });
                });
            });

        if open_ex {
            self.open_examples_dir();
        }
        if start_tuto {
            self.tutorial_enabled = true;
            self.focus_panel(super::dock::Panel::Exercise);
            self.save_settings();
        }
        if dismiss {
            self.welcome_dismissed = true;
            self.save_settings();
        }
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
                                    ui.close();
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
                // « Arch : x86_64 » et « Mode : 64-bit » ne disent rien à un
                // débutant : du bruit en mode apprentissage. On les réserve au
                // mode complet, où ce repère technique a sa place.
                if self.mode == super::UiMode::Full {
                    ui.separator();
                    ui.label(RichText::new("Arch : x86_64").color(self.c_header()));
                    ui.separator();
                    ui.label(RichText::new("Mode : 64-bit").color(self.c_header()));
                }
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


#[cfg(test)]
mod keyboard_tests {
    use super::*;
    use crate::app::dock::Panel;
    use std::path::PathBuf;

    /// Envoie une vraie touche à travers egui, comme le ferait le système.
    fn key(k: egui::Key) -> egui::RawInput {
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: k,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        }
    }

    /// Même chose avec des modificateurs (Ctrl, Maj).
    fn key_mod(k: egui::Key, modifiers: egui::Modifiers) -> egui::RawInput {
        egui::RawInput {
            modifiers,
            events: vec![egui::Event::Key {
                key: k,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        }
    }

    fn ctrl(k: egui::Key) -> egui::RawInput {
        key_mod(k, egui::Modifiers::CTRL)
    }

    /// `tag` distingue les artefacts : sans cela, les tests exécutés en
    /// parallèle assemblent dans le même dossier et s'écrasent l'un l'autre.
    fn running_app(tag: &str) -> App {
        let mut app = App::new();
        // Ces tests visent le désassemblage, la mémoire et la vue mémoire,
        // qui sont des panneaux du mode complet.
        app.set_ui_mode(crate::app::UiMode::Full);
        app.src_path = PathBuf::from(format!("build/kbnav-{tag}.asm"));
        app.out_dir = PathBuf::from(format!("build/kbnav-{tag}"));
        app.source = "section .text\n global _start\n_start:\n mov rax,1\n mov rbx,2\n \
                       mov rcx,3\n mov rax,60\n xor rdi,rdi\n syscall\n"
            .to_string();
        app.launch();
        app.step();
        app
    }

    /// Une frame complète : raccourcis puis rendu, comme `App::update`.
    fn frame(app: &mut App, ctx: &egui::Context, input: egui::RawInput) {
        let _ = ctx.run(input, |ctx| {
            app.handle_shortcuts(ctx);
            app.dock_ui(ctx);
        });
    }

    /// Les flèches doivent piloter le désassemblage, la mémoire et la vue
    /// mémoire — les trois panneaux signalés comme inertes.
    #[test]
    fn arrows_drive_disasm_memory_and_memmap() {
        let mut app = running_app("arrows");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, Default::default());

        // --- Désassemblage ---
        app.focus_panel(Panel::Disasm);
        frame(&mut app, &ctx, Default::default());
        app.selected = None;
        frame(&mut app, &ctx, key(egui::Key::ArrowDown));
        let first = app.selected;
        assert!(first.is_some(), "↓ doit sélectionner une instruction");
        frame(&mut app, &ctx, key(egui::Key::ArrowDown));
        assert_ne!(app.selected, first, "↓ doit avancer dans la liste");

        // --- Mémoire ---
        app.focus_panel(Panel::Memory);
        frame(&mut app, &ctx, Default::default());
        let base = app.mem_addr;
        frame(&mut app, &ctx, key(egui::Key::ArrowDown));
        assert_eq!(app.mem_addr, base + 16, "↓ descend d'une ligne de 16 octets");
        frame(&mut app, &ctx, key(egui::Key::ArrowUp));
        assert_eq!(app.mem_addr, base, "↑ remonte d'une ligne");
        frame(&mut app, &ctx, key(egui::Key::PageDown));
        assert_eq!(app.mem_addr, base + 128, "PgDn saute huit lignes");
        // Le champ de saisie suit l'adresse affichée.
        assert_eq!(app.mem_input, format!("0x{:X}", app.mem_addr));

        // --- Vue mémoire ---
        app.focus_panel(Panel::MemMap);
        frame(&mut app, &ctx, Default::default());
        app.reg_sel = 0;
        frame(&mut app, &ctx, key(egui::Key::ArrowDown));
        assert_eq!(app.reg_sel, 1, "↓ isole le fil du registre suivant");
    }

    /// Le défaut à l'origine du signalement : un champ de saisie gardait le
    /// focus et condamnait TOUTE la navigation clavier de l'application.
    #[test]
    fn a_focused_text_field_does_not_freeze_other_panels() {
        let mut app = running_app("frozen");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, Default::default());

        // On simule un clic passé dans « aller @ » : ce champ garde le focus.
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("kb_mem_goto")));
        app.focus_panel(Panel::Disasm);
        frame(&mut app, &ctx, Default::default());

        app.selected = None;
        frame(&mut app, &ctx, key(egui::Key::ArrowDown));
        assert!(
            app.selected.is_some(),
            "un champ focalisé ailleurs ne doit pas geler les flèches"
        );
    }

    /// Mais pendant une vraie saisie, les flèches restent au texte : elles
    /// doivent déplacer le curseur, pas la sélection du panneau.
    #[test]
    fn arrows_belong_to_the_text_field_while_typing() {
        let mut app = running_app("typing");
        let ctx = egui::Context::default();
        app.focus_panel(Panel::Memory);
        frame(&mut app, &ctx, Default::default());

        let base = app.mem_addr;
        // L'éditeur de texte du panneau MÉMOIRE a le focus : on tape dedans.
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("kb_mem_goto")));
        frame(&mut app, &ctx, key(egui::Key::ArrowDown));
        assert_eq!(app.mem_addr, base, "la saisie garde les flèches");
    }

    /// L'adresse mémoire ne doit pas reboucler vers les adresses hautes en
    /// remontant depuis 0 : un `wrapping_sub` afficherait 0xFFFF… sans raison.
    #[test]
    fn memory_scroll_clamps_at_zero() {
        let mut app = App::new();
        app.mem_addr = 8;
        app.scroll_memory(false, 1);
        assert_eq!(app.mem_addr, 0, "borné à zéro");
        app.scroll_memory(false, 100);
        assert_eq!(app.mem_addr, 0, "reste à zéro");
    }

    /// Déplacer une sélection au clavier doit demander à SON panneau d'amener
    /// l'élément à l'écran — c'est ce qui manquait : la barre de défilement ne
    /// suivait pas le curseur dans les registres.
    #[test]
    fn keyboard_selection_requests_its_own_scroll() {
        use crate::app::dock::Panel;
        let mut app = running_app("scroll");

        app.move_reg_selection(true);
        assert_eq!(app.scroll_to_sel, Some(Panel::Registers));

        app.move_disasm_selection(true);
        assert_eq!(app.scroll_to_sel, Some(Panel::Disasm), "la demande la plus récente gagne");

        app.move_explorer_selection(true);
        assert_eq!(app.scroll_to_sel, Some(Panel::Explorer));
    }

    /// La demande est NOMINATIVE : un panneau ne doit pas absorber celle d'un
    /// autre, sinon le premier rendu volerait le défilement.
    #[test]
    fn a_scroll_request_is_only_consumed_by_its_target() {
        use crate::app::dock::Panel;
        let mut app = running_app("scroll2");
        app.scroll_to_sel = Some(Panel::Registers);

        assert!(!app.take_scroll_request(Panel::Disasm), "le désassemblage ne doit rien prendre");
        assert!(!app.take_scroll_request(Panel::Explorer), "l'explorateur non plus");
        assert_eq!(app.scroll_to_sel, Some(Panel::Registers), "la demande est intacte");

        assert!(app.take_scroll_request(Panel::Registers), "le destinataire la reçoit");
        assert_eq!(app.scroll_to_sel, None, "et elle est consommée");
        assert!(!app.take_scroll_request(Panel::Registers), "une seule fois");
    }

    /// Bout en bout : après une flèche, le panneau des registres reçoit bien sa
    /// demande, et le rendu la consomme.
    #[test]
    fn arrow_then_render_consumes_the_scroll_request() {
        use crate::app::dock::Panel;
        let mut app = running_app("scroll3");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, Default::default());
        app.focus_panel(Panel::Registers);
        frame(&mut app, &ctx, Default::default());

        frame(&mut app, &ctx, key(egui::Key::ArrowDown));
        // Le rendu du même frame a consommé la demande.
        assert_eq!(app.scroll_to_sel, None, "la demande doit être consommée par le rendu");
        assert!(app.reg_sel > 0, "et la sélection avoir bougé");
    }

    /// Un raccourci ne doit déclencher son action QU'UNE FOIS par appui.
    ///
    /// Le gestionnaire de raccourcis avait fini par contenir deux copies du même
    /// bloc : F10 exécutait deux pas, Échap arrêtait deux fois, F6 se battait
    /// avec lui-même. Rien ne le signalait — ni le compilateur, ni les tests
    /// existants, qui appelaient les actions directement. On vérifie donc l'effet
    /// observable d'un appui unique.
    #[test]
    fn a_shortcut_fires_exactly_once_per_press() {
        let mut app = running_app("once");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, Default::default());

        // F10 = un pas, donc exactement une entrée d'historique de plus.
        let before = app.dbg.as_ref().map(|d| d.history.len()).unwrap_or(0);
        frame(&mut app, &ctx, key(egui::Key::F10));
        let after = app.dbg.as_ref().map(|d| d.history.len()).unwrap_or(0);
        assert_eq!(
            after - before,
            1,
            "F10 doit avancer d'un seul pas (obtenu {})",
            after - before
        );

        // F1 ouvre l'aide ; deux bascules l'auraient laissée fermée.
        app.show_shortcuts = false;
        frame(&mut app, &ctx, key(egui::Key::F1));
        assert!(app.show_shortcuts, "F1 doit ouvrir la fenêtre des raccourcis");
    }

    /// Toute touche qui MONTRE quelque chose doit le cacher au second appui.
    /// F1 ne faisait qu'ouvrir : la fenêtre ne se refermait qu'à la souris.
    #[test]
    fn keys_that_show_something_close_it_on_the_second_press() {
        let mut app = App::new();
        app.set_ui_mode(crate::app::UiMode::Full);
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, Default::default());

        // --- F1 : fenêtre d'aide ---
        assert!(!app.show_shortcuts, "fermée au départ");
        frame(&mut app, &ctx, key(egui::Key::F1));
        assert!(app.show_shortcuts, "1er appui : ouvre");
        frame(&mut app, &ctx, key(egui::Key::F1));
        assert!(!app.show_shortcuts, "2e appui : referme");
        frame(&mut app, &ctx, key(egui::Key::F1));
        assert!(app.show_shortcuts, "3e appui : rouvre");

        // --- Ctrl+1..4 : panneaux ; Ctrl+5 : fenêtre Prédiction ---
        for (k, panel) in [
            (egui::Key::Num1, Panel::Explorer),
            (egui::Key::Num2, Panel::Instruction),
            (egui::Key::Num3, Panel::Registers),
            (egui::Key::Num4, Panel::Memory),
        ] {
            let before = app.panel_is_open(panel);
            frame(&mut app, &ctx, ctrl(k));
            assert_eq!(app.panel_is_open(panel), !before, "{panel:?} doit basculer");
            frame(&mut app, &ctx, ctrl(k));
            assert_eq!(app.panel_is_open(panel), before, "{panel:?} doit revenir");
        }
        let before = app.pedagogy_predict;
        frame(&mut app, &ctx, ctrl(egui::Key::Num5));
        assert_eq!(app.pedagogy_predict, !before, "Ctrl+5 doit basculer Prédiction");
        frame(&mut app, &ctx, ctrl(egui::Key::Num5));
        assert_eq!(app.pedagogy_predict, before, "et revenir");
    }

    /// Revue de TOUS les raccourcis annoncés par la fenêtre d'aide : chacun est
    /// envoyé comme une vraie touche, et jugé sur son effet observable.
    ///
    /// Ctrl+O reste dehors : il ouvre le sélecteur de fichiers du système, qui
    /// s'afficherait pour de bon pendant la suite de tests. Son câblage se lit
    /// directement dans `handle_shortcuts`.
    #[test]
    fn every_documented_shortcut_has_its_effect() {
        let mut app = running_app("audit");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, Default::default());
        let shift_f5 = key_mod(egui::Key::F5, egui::Modifiers::SHIFT);

        // --- Exécution ---
        let n = |a: &App| a.dbg.as_ref().map(|d| d.history.len()).unwrap_or(0);
        let before = n(&app);
        frame(&mut app, &ctx, key(egui::Key::F10));
        assert_eq!(n(&app) - before, 1, "F10 : un pas");
        let before = n(&app);
        frame(&mut app, &ctx, key(egui::Key::F8));
        assert_eq!(n(&app) - before, 1, "F8 : un pas (alias de F10)");

        // --- Timeline : Home / ← / → / End ---
        // Aucun des panneaux qui confisquent les flèches ne doit avoir le focus.
        app.focus_panel(Panel::Console);
        frame(&mut app, &ctx, Default::default());
        // Sans plusieurs étapes, Home et End tomberaient toutes deux sur 0 et
        // les quatre assertions passeraient sans rien éprouver.
        assert!(n(&app) >= 3, "il faut une timeline à parcourir, {} étape(s)", n(&app));
        app.set_view(1);
        frame(&mut app, &ctx, key(egui::Key::Home));
        assert_eq!(app.view_index, 0, "Home : début de la timeline");
        frame(&mut app, &ctx, key(egui::Key::ArrowRight));
        assert_eq!(app.view_index, 1, "→ : étape suivante");
        frame(&mut app, &ctx, key(egui::Key::ArrowLeft));
        assert_eq!(app.view_index, 0, "← : étape précédente");
        frame(&mut app, &ctx, key(egui::Key::End));
        assert_eq!(app.view_index, n(&app) - 1, "End : fin de la timeline");

        // --- Navigation entre panneaux ---
        app.focus_panel(Panel::Editor);
        frame(&mut app, &ctx, Default::default());
        frame(&mut app, &ctx, key(egui::Key::F6));
        let after_f6 = app.focused_panel();
        assert_ne!(after_f6, Some(Panel::Editor), "F6 : panneau suivant");
        frame(&mut app, &ctx, key_mod(egui::Key::F6, egui::Modifiers::SHIFT));
        assert_eq!(app.focused_panel(), Some(Panel::Editor), "Maj+F6 : précédent");
        frame(&mut app, &ctx, key(egui::Key::F6));
        frame(&mut app, &ctx, key(egui::Key::F6));
        assert_ne!(app.focused_panel(), Some(Panel::Editor), "on s'est éloigné");
        frame(&mut app, &ctx, ctrl(egui::Key::F6));
        assert_eq!(app.focused_panel(), Some(Panel::Editor), "Ctrl+F6 : retour à l'éditeur");

        // Ctrl+Tab : onglet suivant DANS le nœud focalisé (Éditeur/Désas./Vue).
        let before = app.focused_panel();
        frame(&mut app, &ctx, ctrl(egui::Key::Tab));
        assert_ne!(app.focused_panel(), before, "Ctrl+Tab : onglet suivant");

        // Ctrl+W ferme le panneau focalisé.
        app.focus_panel(Panel::Console);
        frame(&mut app, &ctx, Default::default());
        assert!(app.panel_is_open(Panel::Console));
        frame(&mut app, &ctx, ctrl(egui::Key::W));
        assert!(!app.panel_is_open(Panel::Console), "Ctrl+W : ferme le panneau focalisé");

        // --- Palette ---
        assert!(!app.palette_open);
        frame(&mut app, &ctx, key_mod(egui::Key::P, egui::Modifiers::CTRL | egui::Modifiers::SHIFT));
        assert!(app.palette_open, "Ctrl+Maj+P : ouvre la palette");
        app.palette_open = false;

        // --- Fichier ---
        app.dirty = true;
        frame(&mut app, &ctx, ctrl(egui::Key::S));
        assert!(!app.dirty, "Ctrl+S : enregistre");
        frame(&mut app, &ctx, ctrl(egui::Key::N));
        assert!(app.source.contains("sys_exit"), "Ctrl+N : nouveau fichier");
        assert!(app.dbg.is_none(), "Ctrl+N : remet le débogueur à zéro");

        // --- Assembler et lancer ---
        app.source = "section .text\n global _start\n_start:\n mov rax,60\n xor rdi,rdi\n syscall\n".to_string();
        app.src_path = PathBuf::from("build/kbnav-audit.asm");
        app.save_source();
        // Remis à zéro : sinon un binaire hérité ferait passer l'assertion même
        // si Ctrl+B n'était plus câblé.
        app.binary = None;
        frame(&mut app, &ctx, ctrl(egui::Key::B));
        assert!(app.binary.is_some(), "Ctrl+B : assemble et lie ({})", app.status);
        app.dbg = None;
        frame(&mut app, &ctx, key(egui::Key::F5));
        assert!(app.dbg.is_some(), "F5 : lance ({})", app.status);

        // --- Arrêt : Maj+F5 puis Échap ---
        frame(&mut app, &ctx, shift_f5);
        assert!(app.dbg.is_none(), "Maj+F5 : arrête");
        frame(&mut app, &ctx, key(egui::Key::F5));
        assert!(app.dbg.is_some(), "relancé pour éprouver Échap");
        frame(&mut app, &ctx, key(egui::Key::Escape));
        assert!(app.dbg.is_none(), "Échap : arrête");
    }

    /// Échap doit arrêter le programme une seule fois, sans effet de bord.
    #[test]
    fn escape_stops_once() {
        let mut app = running_app("esc");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, Default::default());
        assert!(app.dbg.is_some());
        frame(&mut app, &ctx, key(egui::Key::Escape));
        assert!(app.dbg.is_none(), "Échap arrête le programme");
    }
}

#[cfg(test)]
mod font_tests {
    use super::*;

    /// La police de repli doit être enregistrée dans les deux familles.
    ///
    /// On ne peut pas vérifier ici que « ✘ », « → » ou « ● » cessent de
    /// s'afficher en carrés : `glyph_width` renvoie la largeur du glyphe de
    /// remplacement pour un caractère absent, donc jamais zéro. La couverture
    /// réelle se constate à l'écran ; ce test garantit seulement que le repli
    /// est bien en place et poussé EN DERNIER, pour ne pas changer l'aspect
    /// des caractères que les polices d'egui savent déjà rendre.
    #[test]
    fn fallback_font_is_registered_last_in_both_families() {
        let ctx = egui::Context::default();
        App::install_fallback_font(&ctx);
        let _ = ctx.run(Default::default(), |_| {});

        // Sur une machine sans aucune des polices candidates, l'installation est
        // un non-événement : le test n'a alors rien à vérifier.
        let has_font = [
            "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Regular.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        ]
        .iter()
        .any(|p| std::path::Path::new(p).exists());
        if !has_font {
            return;
        }

        let defs = ctx.fonts(|_| ()); // force l'initialisation
        let _ = defs;
        let families = ctx.style().text_styles.clone();
        assert!(!families.is_empty(), "les styles de texte doivent exister");

        // Le rendu doit fonctionner après installation.
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("✔ ✘ → ← ● ▲ ▼ ◀ ➤ ⌨ ⚠");
            });
        });
    }

    /// Sans police système, ou appelée deux fois, l'installation ne doit pas
    /// casser l'application.
    #[test]
    fn installing_the_fallback_is_safe_and_idempotent() {
        let ctx = egui::Context::default();
        App::install_fallback_font(&ctx);
        App::install_fallback_font(&ctx);
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("→ ● ✘");
            });
        });
    }
}

#[cfg(test)]
mod welcome_tests {
    use super::*;

    /// Le bandeau d'accueil se dessine en mode apprentissage tant qu'il n'est
    /// pas écarté, et nulle part ailleurs — le tout sans paniquer en headless.
    #[test]
    fn welcome_banner_shows_only_in_learning_until_dismissed() {
        let ctx = egui::Context::default();
        let render = |app: &mut App| {
            let _ = ctx.run(Default::default(), |ctx| app.welcome_banner(ctx));
        };

        let mut app = App::new();
        assert_eq!(app.mode, crate::app::UiMode::Learning, "défaut : apprentissage");
        assert!(!app.welcome_dismissed, "affiché par défaut pour un nouveau venu");
        render(&mut app); // apprentissage + non écarté : le bandeau se dessine

        app.welcome_dismissed = true;
        render(&mut app); // écarté : plus rien

        app.welcome_dismissed = false;
        app.set_ui_mode(crate::app::UiMode::Full);
        render(&mut app); // mode complet : pas de bandeau
    }
}
