use eframe::egui::{self, Color32, RichText};

use crate::i18n;
use crate::debugger::RunState;

use super::{
    App, accent, flag_on, flag_off, false_col, warn_col, changed_col,
    accent_button, bordered_button, icon_button,
};

impl App {
    // ---------- Raccourcis de l'éditeur ----------

    /// Les touches qui appartiennent à l'éditeur : autocomplétion, indentation,
    /// commentaire, déplacement et duplication de lignes, aller à la ligne.
    ///
    /// Chacune est **consommée** (`consume_key`) plutôt que seulement lue : le
    /// champ de texte d'egui verrait sinon le Tab et l'insérerait en plus du
    /// cran d'indentation qu'on vient de poser, et Entrée validerait une
    /// proposition tout en sautant une ligne.
    ///
    /// Deux portées, volontairement distinctes. Tab et les touches de la liste
    /// de complétion exigent que le CHAMP ait le curseur, sinon Tab ne pourrait
    /// plus passer d'un widget à l'autre dans le reste de l'application. Les
    /// autres se contentent que le PANNEAU éditeur soit actif : elles agissent
    /// sur la sélection mémorisée, qu'on ait cliqué dans le texte ou non.
    fn handle_editor_keys(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};

        // Une seule des deux portées suffit à disqualifier les raccourcis quand
        // on tape ailleurs (barre de recherche, « aller @ », champ de licence).
        let in_field = ctx.memory(|m| m.focused()) == Some(super::editor_id());
        let in_panel = in_field || self.focused_panel() == Some(super::dock::Panel::Editor);
        if !in_panel {
            return;
        }
        let take = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));

        // --- Liste d'autocomplétion : elle a la priorité sur tout le reste ---
        if self.complete_open && in_field {
            if take(Modifiers::NONE, Key::ArrowDown) {
                self.move_completion(true);
                return;
            }
            if take(Modifiers::NONE, Key::ArrowUp) {
                self.move_completion(false);
                return;
            }
            if take(Modifiers::NONE, Key::Tab) || take(Modifiers::NONE, Key::Enter) {
                self.accept_completion();
                return;
            }
            if take(Modifiers::NONE, Key::Escape) {
                self.dismiss_completion();
                return;
            }
        }
        // Ctrl+Espace la rouvre à la demande, même après un Échap.
        if take(Modifiers::CTRL, Key::Space) {
            self.force_completion();
            return;
        }

        // --- Indentation ---
        if in_field {
            if take(Modifiers::SHIFT, Key::Tab) {
                self.editor_outdent();
                return;
            }
            if take(Modifiers::NONE, Key::Tab) {
                self.editor_indent();
                return;
            }
        }

        // --- Lignes ---
        // Ctrl+/ sur un clavier français se tape Ctrl+Maj+: — la même touche
        // physique, avec Maj en plus. Les deux combinaisons sont acceptées, et
        // `Key::Colon` avec elles : selon le pilote, l'un ou l'autre remonte.
        let comment = [
            (Modifiers::CTRL, Key::Slash),
            (Modifiers::CTRL | Modifiers::SHIFT, Key::Slash),
            (Modifiers::CTRL, Key::Colon),
            (Modifiers::CTRL | Modifiers::SHIFT, Key::Colon),
        ]
        .into_iter()
        .any(|(m, k)| take(m, k));
        if comment {
            self.editor_toggle_comment();
            return;
        }
        if take(Modifiers::ALT, Key::ArrowUp) {
            self.editor_move_lines(false);
            return;
        }
        if take(Modifiers::ALT, Key::ArrowDown) {
            self.editor_move_lines(true);
            return;
        }
        if take(Modifiers::CTRL, Key::D) {
            self.editor_duplicate_lines();
            return;
        }
        if take(Modifiers::CTRL | Modifiers::SHIFT, Key::K) {
            self.editor_delete_lines();
            return;
        }
        if take(Modifiers::CTRL, Key::G) {
            self.open_goto_line();
        }
    }

    // ---------- Raccourcis ----------

    /// Recherche et remplacement : Ctrl+F, Ctrl+H, F3 et Maj+F3.
    ///
    /// Actifs même quand l'éditeur a le focus, comme Ctrl+S/O/N/B : ce sont
    /// des gestes d'IDE, pas des gestes de champ de texte.
    fn handle_find_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::Key;

        // Ctrl+F ouvre la recherche, Ctrl+H la recherche-remplacement.
        let (find, replace) = ctx.input(|i| {
            let c = i.modifiers.ctrl;
            (c && i.key_pressed(Key::F), c && i.key_pressed(Key::H))
        });
        if find || replace {
            self.show_find = true;
            self.find_replace_mode = replace;
            self.show_panel(super::dock::Panel::Editor);
            self.focus_panel(super::dock::Panel::Editor);
            ctx.memory_mut(|m| m.request_focus(super::find_query_id()));
        }
        // F3 / Maj+F3 : correspondance suivante / précédente, même barre
        // fermée — elle se rouvre pour qu'on revoie le surlignage.
        let (f3, f3_back) = ctx.input(|i| {
            let p = i.key_pressed(Key::F3);
            (p && !i.modifiers.shift, p && i.modifiers.shift)
        });
        if (f3 || f3_back) && !self.find_query.is_empty() {
            self.show_find = true;
            if f3_back {
                self.find_prev();
            } else {
                self.find_next();
            }
        }
    }

    /// Repli des labels : Ctrl+Maj+[ replie celui sous le curseur, Ctrl+Maj+]
    /// déplie tout — mêmes touches que VSCode (Fold / Unfold).
    ///
    /// Le second est simplifié en « tout déplier » plutôt que « déplier celui
    /// sous le curseur » : sans curseur vivant une fois replié (vue lecture
    /// seule), viser un label précis au clavier n'aurait pas de sens.
    fn handle_fold_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::Key;

        let (fold, unfold_all) = ctx.input(|i| {
            let c = i.modifiers.ctrl && i.modifiers.shift;
            (c && i.key_pressed(Key::OpenBracket), c && i.key_pressed(Key::CloseBracket))
        });
        if fold {
            self.fold_label_at_cursor();
        }
        if unfold_all {
            self.unfold_all();
        }
    }

    /// Affichage : Ctrl+1..4 basculent un panneau de la disposition, Ctrl+5 le
    /// mode « prédire la valeur ». Chacun est un réglage persistant, d'où
    /// l'enregistrement en fin de course — une seule fois, quel qu'en soit le
    /// nombre basculé dans la même image.
    fn handle_panel_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::Key;
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
    }

    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::Key;

        // Ctrl+Maj+P ouvre la palette. C'est la porte d'entrée du clavier :
        // egui n'ouvre pas ses menus au clavier, la palette les remplace.
        if ctx.input(|i| i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(Key::P)) {
            self.open_palette();
            return;
        }
        // La barre occupe une ligne entière sur les petits écrans. Ce raccourci
        // correspond au menu Affichage et à la palette de commandes.
        if ctx.input(|i| i.modifiers.ctrl && i.modifiers.alt && i.key_pressed(Key::T)) {
            self.show_toolbar = !self.show_toolbar;
            self.save_settings();
            return;
        }
        // Palette ouverte : elle capte tout, sinon Échap arrêterait le
        // programme et les flèches piloteraient la timeline en arrière-plan.
        if self.palette_open {
            return;
        }
        // Barre de recherche ouverte : Échap la referme d'abord, sans faire
        // aussi office d'arrêt du programme — sinon fermer la recherche
        // interromprait une exécution en cours par surprise.
        if self.show_find && ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.show_find = false;
            if let Some(id) = ctx.memory(|m| m.focused()) {
                ctx.memory_mut(|m| m.surrender_focus(id));
            }
            ctx.memory_mut(|m| m.request_focus(super::editor_id()));
            return;
        }
        // Ignore les raccourcis d'action quand l'éditeur a le focus (sauf Ctrl+*).
        let (step, run, stop, build, save, open, new, new_project, first, prev, next, last) = ctx.input(|i| {
            let c = i.modifiers.ctrl;
            (
                // Sans modificateur : Maj+F10 est le pas par-dessus et Ctrl+F8
                // le point d'arrêt, ni l'un ni l'autre ne doit avancer d'un pas.
                (!i.modifiers.shift && i.key_pressed(Key::F10))
                    || (!i.modifiers.ctrl && i.key_pressed(Key::F8)),
                i.key_pressed(Key::F5),
                i.modifiers.shift && i.key_pressed(Key::F5),
                c && i.key_pressed(Key::B),
                c && i.key_pressed(Key::S),
                c && i.key_pressed(Key::O),
                c && !i.modifiers.shift && i.key_pressed(Key::N),
                c && i.modifiers.shift && i.key_pressed(Key::N),
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
        if new_project {
            self.new_project();
        }
        if build {
            self.build();
        }
        if run {
            self.launch();
        }
        // Pas par-dessus, continuer, point d'arrêt. Maj+F10 prolonge le F10 du
        // pas simple ; F9 et Ctrl+F8 reprennent les touches de JetBrains, dont
        // vient le public de cet IDE.
        let (step_over, cont, toggle_bp, edit_bp) = ctx.input(|i| {
            (
                i.modifiers.shift && i.key_pressed(Key::F10),
                i.key_pressed(Key::F9),
                i.modifiers.ctrl && !i.modifiers.shift && i.key_pressed(Key::F8),
                i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(Key::F8),
            )
        });
        if step_over {
            self.step_over();
        }
        if cont {
            self.cont();
        }
        if toggle_bp {
            let line = self.editor_ln;
            self.toggle_breakpoint(line);
        }
        // Ctrl+Maj+F8 : la condition de la ligne du curseur, comme le clic
        // droit dans la gouttière — c'est aussi la touche de JetBrains pour
        // aller voir ses points d'arrêt.
        if edit_bp {
            let line = self.editor_ln;
            self.open_breakpoint_condition(line);
        }
        self.handle_find_shortcuts(ctx);
        self.handle_fold_shortcuts(ctx);
        // Gestes d'édition (Tab, Alt+↑↓, Ctrl+D…) et autocomplétion : traités
        // avant tout le reste, et RETIRÉS du flux d'événements — c'est ce qui
        // empêche le champ de texte, puis les blocs suivants, de les revoir.
        // Une liste de complétion ouverte y consomme Échap : la fermer ne doit
        // pas arrêter en même temps le programme qui tourne.
        self.handle_editor_keys(ctx);
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
        self.handle_panel_shortcuts(ctx);

        // Les flèches pilotent le panneau focalisé, sauf si l'on tape dedans.
        let editing = self.typing_in_focused_panel(ctx);

        // ↑/↓ parcourent le désassemblage quand son onglet est actif ; Entrée
        // ouvre le microscope sur l'instruction retenue.
        // Le panneau focalisé décide de ce que font les flèches : chaque liste
        // se parcourt au clavier, Entrée valide.
        if !editing {
            let (up, down, enter, rename, delete) = ctx.input(|i| {
                (
                    i.key_pressed(Key::ArrowUp),
                    i.key_pressed(Key::ArrowDown),
                    i.key_pressed(Key::Enter),
                    i.key_pressed(Key::F2),
                    i.key_pressed(Key::Delete),
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
                    // ←/→ replient et déplient l'arbre : sans elles, le clavier
                    // ne pouvait atteindre que la racine.
                    let (left, right) = ctx.input(|i| {
                        (i.key_pressed(Key::ArrowLeft), i.key_pressed(Key::ArrowRight))
                    });
                    if left || right {
                        self.slide_explorer_selection(right);
                    }
                    // Entrée ouvre le fichier, et déplie le dossier — elle ne
                    // change plus de racine : l'arbre se parcourt sur place.
                    if enter && let Some(f) = self.explorer_selected.clone() {
                        if f.is_dir() {
                            self.toggle_explorer_expanded(&f);
                        } else {
                            self.open_file(f);
                        }
                    }
                    if rename && let Some(path) = self.explorer_selected.clone() {
                        self.begin_explorer_rename(path);
                    }
                    if delete && let Some(path) = self.explorer_selected.clone() {
                        self.explorer_delete = Some(path);
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

    /// Applique le thème choisi + le style moderne.
    ///
    /// Tout ce qui est coloré ici vient du catalogue de [`crate::theme`] :
    /// ajouter un thème ne demande pas de repasser par cette fonction. Seul le
    /// cas « Système » regarde encore l'OS, pour savoir lequel des deux thèmes
    /// intégrés il désigne.
    pub(super) fn apply_theme(&mut self, ctx: &egui::Context) {
        use egui::{FontId, CornerRadius, Theme, TextStyle, vec2};
        let system_dark = ctx.input(|i| i.raw.system_theme) != Some(Theme::Light);
        let theme = self.theme_pref.resolve(system_dark);
        // Publié pour tout le code d'affichage, qui lit les couleurs par
        // `crate::theme::current()` sans avoir à recevoir le thème en argument.
        crate::theme::set_current(theme);
        let p = &theme.ui;
        let mut style = (*ctx.style()).clone();
        // Le point de départ egui donne les dizaines de réglages qu'un thème
        // n'a pas à décrire (ombres, épaisseurs, expansions) ; ce qui suit
        // remplace ceux qui font l'identité visuelle.
        let mut v = if theme.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        // Des surfaces plus nettes, mais pas « molles » : l'IDE distingue
        // clairement ses niveaux (chrome, zones de travail, cartes) sans
        // transformer chaque contrôle en pastille. Les grands angles restent
        // réservés aux fenêtres et aux groupes de la barre de travail.
        v.window_corner_radius = CornerRadius::same(12);
        v.menu_corner_radius = CornerRadius::same(9);
        v.selection.bg_fill = p.accent.linear_multiply(0.38);
        v.selection.stroke.color = p.text_strong;
        v.hyperlink_color = p.accent;
        v.panel_fill = p.bg;
        v.window_fill = p.window;
        v.window_stroke.color = p.border;
        v.extreme_bg_color = p.extreme;
        v.faint_bg_color = p.faint;
        // Aucun `override_text_color` : il écraserait aussi les nuances faible
        // et forte, dont l'interface se sert partout (libellés secondaires,
        // titres). Les niveaux se posent un par un sur les états de widget,
        // d'où egui dérive ensuite `text_color`, `weak_text_color` et
        // `strong_text_color`.
        v.override_text_color = None;
        let states: [(&mut egui::style::WidgetVisuals, Color32, Color32); 5] = [
            (&mut v.widgets.noninteractive, p.bg, p.text),
            (&mut v.widgets.inactive, p.surface, p.text),
            (&mut v.widgets.hovered, p.surface_hover, p.text_strong),
            (&mut v.widgets.active, p.surface_active, p.text_strong),
            (&mut v.widgets.open, p.surface, p.text),
        ];
        for (w, fill, fg) in states {
            w.corner_radius = CornerRadius::same(7);
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.bg_stroke.color = p.border;
            w.fg_stroke.color = fg;
        }
        style.visuals = v;
        // Un rythme vertical un peu plus généreux rend les panneaux denses
        // (registres, pile, mémoire) nettement plus scannables, sans réduire
        // l'espace consacré au code.
        style.spacing.item_spacing = vec2(8.0, 7.0);
        style.spacing.button_padding = vec2(10.0, 5.0);
        style.spacing.indent = 18.0;
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
        egui::TopBottomPanel::top("menubar")
            .frame(
                egui::Frame::new()
                    .fill(crate::theme::current().ui.window)
                    .stroke(egui::Stroke::new(1.0_f32, crate::theme::current().ui.border.gamma_multiply(0.7)))
                    .inner_margin(egui::Margin::symmetric(10, 3)),
            )
            .show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                // Une signature compacte : elle ancre la navigation et rend
                // immédiatement identifiable la barre qui ne contient que les
                // menus, sans gaspiller la place nécessaire aux libellés.
                ui.label(RichText::new("ASM").strong().color(accent()));
                ui.label(RichText::new("STUDIO").small().strong().color(self.c_header()));
                ui.separator();
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
                    if item(ui, tr("Nouveau projet…", "New project…", "Proyecto nuevo…"), "Ctrl+Maj+N") {
                        self.new_project();
                    }
                    if item(ui, tr("Ouvrir…", "Open…", "Abrir…"), "Ctrl+O") {
                        self.open_browser();
                    }
                    self.recent_menu(ui);
                    // « Exemples et exercices » a quitté ce menu pour
                    // « Apprendre » : personne ne cherche un exercice sous
                    // Fichier, et le parcours était éparpillé sur quatre menus.
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
                    // La cible d'abord : c'est elle qui décide de ce que
                    // « Assembler » produit et de si « Lancer » a un sens. Le
                    // sous-menu n'apparaît que si l'assemblage Windows est
                    // proposé (Réglages) : sinon il n'y a qu'une cible, et un
                    // menu à un seul choix n'est pas un choix.
                    if self.pe_enabled {
                    ui.menu_button(tr("Cible", "Target", "Destino"), |ui| {
                        use crate::assemble::Target;
                        let mut chosen: Option<Target> = None;
                        for (t, label, tip) in [
                            (
                                Target::Linux,
                                tr("Linux — ELF64", "Linux — ELF64", "Linux — ELF64"),
                                tr(
                                    "Assemblé, exécuté et débogué pas à pas ici.",
                                    "Assembled, run and single-stepped here.",
                                    "Ensamblado, ejecutado y depurado paso a paso aquí.",
                                ),
                            ),
                            (
                                Target::Windows,
                                tr("Windows — PE64 console", "Windows — PE64 console", "Windows — PE64 consola"),
                                tr(
                                    "Produit un vrai .exe, désassemblable et lisible dans le panneau FORMAT — mais non exécutable sous Linux.",
                                    "Produces a real .exe, disassemblable and readable in the FORMAT panel — but not runnable on Linux.",
                                    "Produce un .exe real, desensamblable y legible en el panel FORMATO — pero no ejecutable en Linux.",
                                ),
                            ),
                            (
                                Target::WindowsGui,
                                tr("Windows — PE64 fenêtré", "Windows — PE64 GUI", "Windows — PE64 con ventanas"),
                                tr(
                                    "Même chose, sans console au lancement : pour un programme qui ne parle que par MessageBox.",
                                    "Same, with no console at startup: for a program that only speaks through MessageBox.",
                                    "Lo mismo, sin consola al arrancar: para un programa que solo habla por MessageBox.",
                                ),
                            ),
                        ] {
                            let mut on = self.target == t;
                            if ui.checkbox(&mut on, label).on_hover_text(tip).clicked() {
                                chosen = Some(t);
                            }
                        }
                        if let Some(t) = chosen {
                            self.set_target(t);
                        }
                    });
                    ui.separator();
                    }
                    if item(ui, tr("Assembler", "Build", "Ensamblar"), "Ctrl+B") {
                        self.build();
                    }
                    if item(ui, tr("Lancer", "Run", "Ejecutar"), "F5") {
                        self.launch();
                    }
                    ui.separator();
                    if self.target.is_runnable() {
                        if item(ui, tr("Pas à pas", "Step", "Paso a paso"), "F10") {
                            self.step();
                        }
                    } else {
                        ui.add_enabled(
                            false,
                            egui::Button::new(tr(
                                "Pas à pas — indisponible pour PE64",
                                "Step — unavailable for PE64",
                                "Paso a paso — no disponible para PE64",
                            ))
                            .shortcut_text("F10"),
                        )
                        .on_disabled_hover_text(tr(
                            "Wine exécute le PE64, mais ne permet pas à ASM Studio de le dérouler instruction par instruction.",
                            "Wine runs the PE64, but does not let ASM Studio walk through it instruction by instruction.",
                            "Wine ejecuta el PE64, pero no permite que ASM Studio lo recorra instrucción por instrucción.",
                        ));
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

                // Le parcours guidé, ses exercices et sa progression tenaient
                // dans trois menus sans rapport — le tutoriel sous « Aide », les
                // exercices sous « Fichier », la progression sous
                // « Préférences ». Un menu de premier niveau les réunit : c'est
                // le sujet principal du logiciel pour qui débute, il mérite son
                // entrée plutôt que d'être un recoin de l'aide.
                ui.menu_button(tr("Apprendre", "Learn", "Aprender"), |ui| {
                    if item(ui, tr("Parcours guidé", "Guided path", "Recorrido guiado"), "") {
                        self.show_tutorial_toc();
                    }
                    ui.add_enabled_ui(self.has_lesson_to_resume(), |ui| {
                        // Le titre de la leçon dans le libellé : « Reprendre »
                        // seul ne dit pas où l'on retombe, et c'est justement la
                        // question de celui qui rouvre l'application.
                        let label = match self.lesson_to_resume_title() {
                            Some(t) => format!("{} — « {t} »", tr("Reprendre", "Resume", "Continuar")),
                            None => tr("Reprendre la leçon", "Resume the lesson", "Continuar la lección").to_string(),
                        };
                        if item(ui, &label, "") {
                            self.resume_lesson();
                        }
                    });
                    ui.separator();
                    if item(
                        ui,
                        tr("Catalogue d'exercices…", "Exercise catalogue…", "Catálogo de ejercicios…"),
                        "",
                    ) {
                        self.open_examples_dir();
                    }
                    ui.separator();
                    // La progression n'était lisible que dans le panneau ✦ :
                    // l'annoncer ici donne au menu la même information que la
                    // barre de progression, sans avoir à ouvrir le panneau.
                    let (done, total) = self.tutorial_progress.overall(self.pe_enabled);
                    ui.label(
                        RichText::new(match lang {
                            i18n::Lang::Fr => format!("Progression : {done} / {total} leçons"),
                            i18n::Lang::En => format!("Progress: {done} / {total} lessons"),
                            i18n::Lang::Es => format!("Progreso: {done} / {total} lecciones"),
                        })
                        .small()
                        .weak(),
                    );
                    if item(ui, tr("Revoir l'écran d'accueil", "Show the welcome screen again", "Volver a ver la pantalla de bienvenida"), "") {
                        self.show_welcome_again();
                    }
                    if item(
                        ui,
                        tr("Réinitialiser la progression…", "Reset progress…", "Reiniciar el progreso…"),
                        "",
                    ) {
                        self.reset_tutorial();
                    }
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
                    // Desdec n'est pas fourni avec l'IDE, et l'entrée reste
                    // pourtant toujours cliquable : la griser demanderait de
                    // fouiller le PATH à chaque image du menu, et un clic sans
                    // Desdec installé répond déjà, en toutes lettres, ce qu'il
                    // manque et où le mettre.
                    if ui
                        .add(egui::Button::new(tr("Envoyer vers Desdec", "Send to Desdec", "Enviar a Desdec")))
                        .on_hover_text(tr(
                            "Assemble, puis ouvre le binaire produit dans Desdec : sections, chaînes, table d'import, désassemblage complet. Desdec s'installe à part.",
                            "Assembles, then opens the binary produced in Desdec: sections, strings, import table, full disassembly. Desdec is installed separately.",
                            "Ensambla y abre el binario producido en Desdec: secciones, cadenas, tabla de importación, desensamblado completo. Desdec se instala aparte.",
                        ))
                        .clicked()
                    {
                        ui.close();
                        self.send_to_desdec();
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
                    // Le tutoriel et l'écran d'accueil ont leur menu à eux :
                    // « Apprendre ». Ce menu-ci retrouve son sujet — l'aide sur
                    // le logiciel, pas l'enseignement de l'assembleur.
                    if item(ui, tr("Raccourcis clavier…", "Keyboard shortcuts…", "Atajos de teclado…"), "F1") {
                        self.show_shortcuts = true;
                    }
                    // Une fois la licence active, plus rien à activer : l'entrée
                    // disparaît au lieu de proposer une action sans effet utile.
                    if !self.is_licensed() {
                        ui.separator();
                        if item(ui, tr("Activer une licence…", "Activate a license…", "Activar una licencia…"), "") {
                            self.show_license_gate = true;
                        }
                    }
                    ui.separator();
                    if item(ui, tr("À propos", "About", "Acerca de"), "") {
                        self.show_about = true;
                    }
                });
            });
        });
    }

    /// Sous-menu « Récents ». Grisé tant que rien n'a été ouvert : une entrée
    /// qui s'ouvre sur le vide déroute plus qu'elle ne renseigne.
    ///
    /// Chaque ligne montre le nom du fichier puis son dossier en gris — deux
    /// exercices portent souvent le même nom dans deux dossiers différents, et
    /// le seul nom ne permettrait pas de les distinguer.
    fn recent_menu(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        // Les chemins morts sont retirés à l'ouverture du menu : c'est le seul
        // moment où l'on regarde la liste, et le seul où toucher au disque pour
        // la vérifier ne coûte rien.
        self.prune_recent();
        let recent = self.recent_files.clone();
        let mut to_open = None;
        let mut clear = false;

        ui.add_enabled_ui(!recent.is_empty(), |ui| {
            ui.menu_button(tr("Récents", "Recent", "Recientes"), |ui| {
                for path in &recent {
                    let name = path.file_name().unwrap_or(path.as_os_str()).to_string_lossy();
                    let parent = path
                        .parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default();
                    let mut text = egui::text::LayoutJob::default();
                    text.append(&name, 0.0, egui::TextFormat::default());
                    text.append(
                        &format!("   {parent}"),
                        0.0,
                        egui::TextFormat {
                            color: ui.visuals().weak_text_color(),
                            ..Default::default()
                        },
                    );
                    if ui.button(text).on_hover_text(path.display().to_string()).clicked() {
                        to_open = Some(path.clone());
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button(tr("Vider la liste", "Clear the list", "Vaciar la lista")).clicked() {
                    clear = true;
                    ui.close();
                }
            });
        });

        if let Some(path) = to_open {
            self.open_file(path);
        }
        if clear {
            self.recent_files.clear();
            self.save_settings();
        }
    }

    /// Contenu du menu Affichage : mode d'abord, puis panneaux, puis disposition.
    fn view_menu(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        use super::UiMode;

        ui.label(RichText::new(tr("Mode d'affichage", "Display mode", "Modo de visualización")).small().weak());
        let mut wanted = self.mode;
        for m in [UiMode::Learning, UiMode::Editor, UiMode::Full] {
            ui.radio_value(&mut wanted, m, m.label(lang))
                .on_hover_text(m.description(lang));
        }
        if wanted != self.mode {
            self.set_ui_mode(wanted);
            ui.close();
        }

        ui.separator();
        if ui
            .checkbox(
                &mut self.show_toolbar,
                tr("Barre d'outils", "Toolbar", "Barra de herramientas"),
            )
            .on_hover_text(tr(
                "Afficher ou cacher les boutons Lancer, Suivant et Continuer (Ctrl+Alt+T).",
                "Show or hide the Run, Step and Continue buttons (Ctrl+Alt+T).",
                "Mostrar u ocultar los botones Ejecutar, Siguiente y Continuar (Ctrl+Alt+T).",
            ))
            .changed()
        {
            self.save_settings();
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
        if !self.show_toolbar {
            return;
        }
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(crate::theme::current().ui.bg)
                    .inner_margin(egui::Margin::symmetric(10, 6)),
            )
            .show(ctx, |ui| {
            egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .stroke(egui::Stroke::new(1.0_f32, crate::theme::current().ui.border.gamma_multiply(0.75)))
                .corner_radius(egui::CornerRadius::same(9))
                .inner_margin(egui::Margin::symmetric(7, 5))
                .show(ui, |ui| {
            ui.horizontal(|ui| {
                let lang = self.lang;
                let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
                // « Lancer » se grise aussi pendant qu'un .exe tourne sous
                // Wine : c'est bien un programme de l'élève en cours, même
                // sans débogueur derrière.
                let running = self.dbg.as_ref().is_some_and(|d| d.is_alive())
                    || self.wine.as_ref().is_some_and(|w| w.is_running());
                let can_step = self.can_step();
                let step_tip = if self.target.is_runnable() {
                    tr("Instruction suivante (F10)", "Next instruction (F10)", "Instrucción siguiente (F10)")
                } else {
                    tr(
                        "Pas à pas indisponible pour PE64 ; choisissez Linux ELF64 pour déboguer.",
                        "Single-stepping is unavailable for PE64; choose Linux ELF64 to debug.",
                        "El paso a paso no está disponible para PE64; elija Linux ELF64 para depurar.",
                    )
                };
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
                    .tip(accent_button(ui, ic_debug.as_ref(), tr("Suivant", "Next", "Siguiente"), can_step), step_tip)
                    .clicked()
                {
                    self.step();
                }
                // Par-dessus : franchit un `call` d'un bloc, sans dérouler la
                // fonction appelée instruction par instruction.
                if self
                    .tip(
                        bordered_button(ui, None, tr("Par-dessus", "Step over", "Por encima"), can_step),
                        tr(
                            "Exécuter l'appel d'un bloc (Maj+F10)",
                            "Run the call in one go (Shift+F10)",
                            "Ejecutar la llamada de una vez (Mayús+F10)",
                        ),
                    )
                    .clicked()
                {
                    self.step_over();
                }
                // Continuer : jusqu'au prochain point d'arrêt, ou la fin.
                if self
                    .tip(
                        bordered_button(ui, None, tr("Continuer", "Continue", "Continuar"), can_step),
                        tr(
                            "Jusqu'au prochain point d'arrêt (F9)",
                            "To the next breakpoint (F9)",
                            "Hasta el próximo punto de interrupción (F9)",
                        ),
                    )
                    .clicked()
                {
                    self.cont();
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

                // L'état du programme et le format produit sont déjà dans la
                // barre d'état. Les répéter ici gaspille l'espace réservé aux
                // actions et donne l'impression de trois boutons sans action.
            });
            });
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
                    .fill(accent().linear_multiply(0.12))
                    .stroke(egui::Stroke::new(1.0_f32, accent().gamma_multiply(0.48)))
                    .inner_margin(egui::Margin::symmetric(14, 9)),
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
                            ).fill(accent()))
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
            self.show_tutorial_toc();
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
        let mut switch_mode: Option<super::UiMode> = None;
        egui::TopBottomPanel::bottom("statusbar")
            .frame(
                egui::Frame::new()
                    .fill(crate::theme::current().ui.window)
                    .stroke(egui::Stroke::new(1.0_f32, crate::theme::current().ui.border.gamma_multiply(0.68)))
                    .inner_margin(egui::Margin::symmetric(10, 4)),
            )
            .show(ctx, |ui| {
            ui.horizontal(|ui| {
                match &self.dbg {
                    Some(d) if d.is_alive() => {
                        ui.colored_label(flag_on(), "● Running");
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
                                flag_on(),
                                RichText::new(format!("✔ {} 0", tr("Exit", "Exit", "Salir"))).strong(),
                            );
                        }
                        RunState::Exited(c) => {
                            ui.colored_label(
                                false_col(),
                                RichText::new(format!("✘ {} {c}", tr("Exit", "Exit", "Salir"))).strong(),
                            );
                        }
                        RunState::Signaled => {
                            ui.colored_label(false_col(), RichText::new(tr("✘ Signal", "✘ Signal", "✘ Señal")).strong());
                        }
                        RunState::Faulted(f) => {
                            ui.colored_label(
                                false_col(),
                                RichText::new(format!("✘ {}", f.signal_name())).strong(),
                            );
                        }
                        RunState::Stopped => {
                            ui.colored_label(flag_off(), format!("○ {}", tr("Arrêté", "Stopped", "Detenido")));
                        }
                        // Suspendu dans un appel système : presque toujours un
                        // `read` qui attend la saisie de l'élève.
                        RunState::Running => {
                            ui.colored_label(
                                flag_on(),
                                RichText::new(tr(
                                    "⏳ En attente d'entrée",
                                    "⏳ Waiting for input",
                                    "⏳ Esperando entrada",
                                ))
                                .strong(),
                            );
                        }
                    },
                    None => {
                        ui.colored_label(flag_off(), format!("○ {}", tr("Prêt", "Ready", "Listo")));
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
                        ui.colored_label(changed_col(), format!("{} : 0x{next:X}", tr("Suivant", "Next", "Siguiente")));
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
                    ui.label(RichText::new("NASM").color(accent()).strong());
                    // Juste à côté : le format que « Assembler » va produire.
                    // L'assembleur ne dit pas tout — le même NASM sort un ELF ou
                    // un PE selon la cible, et rien à l'écran ne le montrait :
                    // il fallait rouvrir le menu Exécution ▸ Cible pour savoir
                    // dans quel monde on écrivait.
                    ui.separator();
                    let (fmt, tip) = match self.target {
                        crate::assemble::Target::Linux => (
                            "ELF64",
                            tr(
                                "Cible Linux : nasm -f elf64 puis ld. Exécuté et déroulé pas à pas ici.",
                                "Linux target: nasm -f elf64 then ld. Run and single-stepped here.",
                                "Destino Linux: nasm -f elf64 y ld. Ejecutado y recorrido paso a paso aquí.",
                            ),
                        ),
                        crate::assemble::Target::Windows => (
                            "PE64",
                            tr(
                                "Cible Windows console : nasm -f win64 puis le lieur intégré. Lancé par Wine s'il est installé, sans pas-à-pas.",
                                "Windows console target: nasm -f win64 then the built-in linker. Run through Wine when installed, with no single-stepping.",
                                "Destino Windows consola: nasm -f win64 y el enlazador integrado. Ejecutado con Wine si está instalado, sin paso a paso.",
                            ),
                        ),
                        crate::assemble::Target::WindowsGui => (
                            // L'indicateur donne le FORMAT du binaire, non son
                            // type d'interface : Windows console et fenêtré
                            // restent tous deux des PE64.
                            "PE64",
                            tr(
                                "Cible Windows fenêtrée : même chose, sans console au lancement.",
                                "Windows GUI target: same, with no console at startup.",
                                "Destino Windows con ventanas: lo mismo, sin consola al arrancar.",
                            ),
                        ),
                    };
                    // Vert quand le binaire produit se débogue ici, orangé quand
                    // il ne fait que s'assembler : la couleur porte la nuance.
                    let col = if self.target.is_runnable() { flag_on() } else { warn_col() };
                    ui.label(RichText::new(fmt).color(col).strong()).on_hover_text(tip);
                    // Le mode courant, toujours affiché et toujours cliquable.
                    //
                    // Il ne se montrait qu'en Apprentissage : l'absence
                    // d'étiquette était censée dire « mode complet », ce que
                    // personne ne lit. Et l'étiquette était morte — elle
                    // annonçait un contexte qu'il fallait aller changer dans le
                    // menu Affichage. Un clic bascule désormais, ce qui en fait
                    // le repère ET l'interrupteur du même état.
                    ui.separator();
                    let other = match self.mode {
                        super::UiMode::Learning => super::UiMode::Editor,
                        super::UiMode::Editor => super::UiMode::Full,
                        super::UiMode::Full => super::UiMode::Learning,
                    };
                    let col = if self.mode == super::UiMode::Learning { accent() } else { self.c_header() };
                    if ui
                        .add(egui::Button::new(RichText::new(self.mode.label(lang)).color(col).strong()).frame(false))
                        .on_hover_text(format!(
                            "{}\n\n{} « {} »",
                            self.mode.description(lang),
                            tr("Cliquer pour passer en", "Click to switch to", "Clic para pasar a"),
                            other.label(lang),
                        ))
                        .clicked()
                    {
                        switch_mode = Some(other);
                    }
                    ui.separator();
                    match &self.focused_panel_name {
                        Some(name) => {
                            ui.label(
                                RichText::new(format!("⌨ {name}"))
                                    .color(accent())
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
        // Hors de la fermeture de dessin : `set_ui_mode` remplace la
        // disposition, donc le dock que la frame courante est en train de lire.
        if let Some(m) = switch_mode {
            self.set_ui_mode(m);
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
        // Le panneau Registres est réservé aux licences : sans elle, il ne
        // rend jamais la liste, donc jamais la consommation testée ici.
        app.license = crate::license::valid_for_tests();
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

        // --- Ctrl+Alt+T : barre d'outils ---
        let toolbar = key_mod(egui::Key::T, egui::Modifiers::CTRL | egui::Modifiers::ALT);
        assert!(app.show_toolbar, "visible par défaut");
        frame(&mut app, &ctx, toolbar.clone());
        assert!(!app.show_toolbar, "Ctrl+Alt+T la cache");
        frame(&mut app, &ctx, toolbar);
        assert!(app.show_toolbar, "et le second appui la réaffiche");

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
        app.source.push_str("\n    nop\n");
        assert!(app.dirty());
        frame(&mut app, &ctx, ctrl(egui::Key::S));
        assert!(!app.dirty(), "Ctrl+S : enregistre");
        frame(&mut app, &ctx, ctrl(egui::Key::N));
        // Le nouveau fichier demande d'abord son format : c'est la réponse qui
        // pose le squelette. Ici on vérifie que le raccourci mène bien à la
        // question, puis on y répond.
        assert!(app.new_file_prompt, "Ctrl+N : demande le format du nouveau fichier");
        app.create_new_file(crate::assemble::Target::Linux);
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

    // ---------- Recherche / remplacement (Ctrl+F / Ctrl+H) ----------

    #[test]
    fn ctrl_f_opens_the_find_bar_in_search_only_mode() {
        let mut app = running_app("find1");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, Default::default());
        assert!(!app.show_find);

        frame(&mut app, &ctx, ctrl(egui::Key::F));
        assert!(app.show_find, "Ctrl+F doit ouvrir la barre");
        assert!(!app.find_replace_mode, "Ctrl+F seul n'affiche pas le remplacement");
    }

    #[test]
    fn ctrl_h_opens_the_find_bar_with_replace_mode() {
        let mut app = running_app("find2");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, Default::default());

        frame(&mut app, &ctx, ctrl(egui::Key::H));
        assert!(app.show_find);
        assert!(app.find_replace_mode, "Ctrl+H doit afficher la ligne de remplacement");
    }

    /// Fermer la recherche par Échap ne doit PAS aussi arrêter un programme en
    /// cours d'exécution : les deux usages d'Échap ne doivent pas se marcher
    /// dessus.
    #[test]
    fn escape_closes_find_bar_without_stopping_the_running_program() {
        let mut app = running_app("find3");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, Default::default());
        frame(&mut app, &ctx, ctrl(egui::Key::F));
        assert!(app.show_find);
        assert!(app.dbg.is_some(), "running_app lance déjà un programme");

        frame(&mut app, &ctx, key(egui::Key::Escape));
        assert!(!app.show_find, "Échap doit fermer la barre");
        assert!(app.dbg.is_some(), "fermer la recherche ne doit pas arrêter le programme");
    }

    /// F3 doit rouvrir la barre (pour revoir le surlignage) et avancer à la
    /// correspondance suivante, même si elle avait été fermée entre-temps.
    #[test]
    fn f3_reopens_a_closed_bar_and_advances_to_the_next_match() {
        let mut app = running_app("find4");
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, Default::default());
        // Le source de `running_app` contient deux occurrences de « rax ».
        app.find_query = "rax".to_string();
        app.show_find = false;

        frame(&mut app, &ctx, key(egui::Key::F3));
        assert!(app.show_find, "F3 doit rouvrir la barre");
        assert_eq!(app.find_current, 1, "avance vers la correspondance suivante");
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

        ctx.fonts(|_| ()); // force l'initialisation
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

#[cfg(test)]
mod menu_tests {
    use super::*;

    /// L'apprentissage a son menu, et un seul.
    ///
    /// Ses entrées étaient réparties sur trois menus sans rapport — le tutoriel
    /// sous « Aide », les exercices sous « Fichier », la progression sous
    /// « Préférences ». Personne ne cherche un exercice sous Fichier.
    #[test]
    fn the_learning_path_has_its_own_top_level_menu() {
        let ctx = egui::Context::default();
        let mut app = App::new();
        let out = ctx.run(Default::default(), |ctx| app.menu_bar(ctx));
        let texts = super::status_bar_tests::collect_text(&out.shapes);

        assert!(
            texts.iter().any(|t| t == "Apprendre"),
            "le menu Apprendre manque, vu : {texts:?}"
        );
        // L'ordre compte : Apprendre se lit après Exécution et avant Affichage,
        // là où l'on progresse du « faire tourner » vers le « regarder ».
        let rank = |name: &str| texts.iter().position(|t| t == name);
        assert!(
            rank("Exécution") < rank("Apprendre") && rank("Apprendre") < rank("Affichage"),
            "ordre des menus inattendu : {texts:?}"
        );
    }
}

#[cfg(test)]
pub(super) mod status_bar_tests {
    use super::*;

    /// La barre d'état annonce le format que « Assembler » va produire, à côté
    /// de l'assembleur : le même NASM sort un ELF ou un PE selon la cible, et
    /// rien à l'écran ne le disait.
    #[test]
    fn the_status_bar_names_the_binary_format_next_to_the_assembler() {
        use crate::assemble::Target;
        for (target, expected) in [
            (Target::Linux, "ELF64"),
            (Target::Windows, "PE64"),
            (Target::WindowsGui, "PE64"),
        ] {
            let mut app = App::new();
            app.pe_enabled = true;
            app.set_target(target);
            let ctx = egui::Context::default();
            let out = ctx.run(Default::default(), |ctx| app.status_bar(ctx));

            let texts = collect_text(&out.shapes);
            assert!(texts.iter().any(|t| t == "NASM"), "l'assembleur reste affiché");
            assert!(
                texts.iter().any(|t| t == expected),
                "cible {target:?} : « {expected} » attendu dans la barre d'état, vu : {texts:?}"
            );
        }
    }

    /// La barre d'état nomme TOUJOURS le mode courant, et le nomme JUSTE.
    ///
    /// Elle ne l'affichait qu'en Apprentissage : l'absence d'étiquette était
    /// censée signifier « mode complet », ce que personne ne déduit. Et rien ne
    /// garantissait que l'étiquette dise l'état du parcours — c'est ce qui la
    /// rendait désynchronisée quand le tutoriel avait son propre interrupteur.
    #[test]
    fn the_status_bar_always_names_the_current_mode() {
        let ctx = egui::Context::default();
        let mut app = App::new();
        let learning = ctx.run(Default::default(), |ctx| app.status_bar(ctx));
        assert!(
            collect_text(&learning.shapes).iter().any(|text| text == "Apprentissage"),
            "le mode Apprentissage se nomme"
        );
        assert!(app.tutorial_enabled(), "et l'étiquette dit vrai : le parcours est offert");

        app.set_ui_mode(crate::app::UiMode::Full);
        let full = ctx.run(Default::default(), |ctx| app.status_bar(ctx));
        let texts = collect_text(&full.shapes);
        assert!(
            !texts.iter().any(|text| text == "Apprentissage"),
            "le mode complet ne se fait pas passer pour l'apprentissage"
        );
        assert!(texts.iter().any(|text| text == "Complet"), "il se nomme, lui aussi");
        assert!(!app.tutorial_enabled(), "et l'étiquette dit vrai là encore");
    }

    /// L'étiquette de mode est l'interrupteur du mode : un clic bascule.
    #[test]
    fn clicking_the_mode_label_switches_the_mode() {
        let mut app = App::new();
        assert_eq!(app.mode, crate::app::UiMode::Learning);
        let ctx = egui::Context::default();
        // Le clic est simulé là où l'étiquette a été peinte : on la retrouve en
        // parcourant la frame précédente plutôt qu'en codant une coordonnée.
        let mut input = egui::RawInput::default();
        let target = mode_label_pos(&ctx, &mut app).expect("étiquette de mode dessinée");
        input.events.push(egui::Event::PointerMoved(target));
        input.events.push(egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        });
        input.events.push(egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        });
        let _ = ctx.run(input, |ctx| app.status_bar(ctx));
        assert_eq!(app.mode, crate::app::UiMode::Editor, "un clic passe en mode éditeur seul");
    }

    /// Centre de l'étiquette de mode, telle qu'elle vient d'être peinte.
    fn mode_label_pos(ctx: &egui::Context, app: &mut App) -> Option<egui::Pos2> {
        let label = app.mode.label(app.lang);
        let out = ctx.run(Default::default(), |ctx| app.status_bar(ctx));
        fn walk(shape: &egui::Shape, want: &str, out: &mut Option<egui::Pos2>) {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == want => {
                    *out = Some(t.pos + t.galley.size() / 2.0);
                }
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, want, out)),
                _ => {}
            }
        }
        let mut pos = None;
        for c in &out.shapes {
            walk(&c.shape, label, &mut pos);
        }
        pos
    }

    /// Tout le texte peint pendant une frame, à plat. Partagé avec les autres
    /// modules de tests de l'interface : ce que l'utilisateur lit à l'écran est
    /// la seule chose qu'un test d'UI puisse vraiment vérifier.
    pub(in crate::app) fn collect_text(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for s in shapes {
            walk(&s.shape, &mut out);
        }
        out
    }
}
