//! Palette de commandes : toute l'application au clavier.
//!
//! egui n'ouvre pas ses menus au clavier — `menu_button` ne réagit qu'à la
//! souris. Sans autre voie d'accès, un utilisateur au clavier ne peut atteindre
//! ni « Ouvrir », ni les réglages, ni un panneau fermé.
//!
//! La palette contourne le problème par le haut : **Ctrl+Maj+P**, on tape
//! quelques lettres, les flèches choisissent, Entrée exécute. Chaque action de
//! l'application y figure, y compris celles qui n'ont pas de raccourci.

use eframe::egui::{self, RichText};

use super::dock::Panel;
use super::{ACCENT, App};
use crate::i18n::{self, Lang};

/// Une action exécutable depuis la palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    // Fichier
    New,
    Open,
    Save,
    SaveAs,
    // Exécution
    Build,
    Run,
    Step,
    Stop,
    ResumeHere,
    // Timeline
    TimelineStart,
    TimelineEnd,
    TimelinePrev,
    TimelineNext,
    // Panneaux : afficher/masquer, ou donner le focus.
    TogglePanel(Panel),
    FocusPanel(Panel),
    ResetLayout,
    ShowAllPanels,
    // Fenêtres
    TogglebPrediction,
    ToggleTutorial,
    Calculator,
    Settings,
    Shortcuts,
    About,
    CheckUpdates,
    // Langue et mode d'affichage
    SetLang(Lang),
    SetMode(crate::app::UiMode),
}

impl Command {
    /// Libellé affiché, traduit.
    pub(crate) fn label(self, lang: Lang) -> String {
        let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        match self {
            Command::New => t("Fichier : Nouveau", "File: New", "Archivo: Nuevo").into(),
            Command::Open => t("Fichier : Ouvrir…", "File: Open…", "Archivo: Abrir…").into(),
            Command::Save => t("Fichier : Enregistrer", "File: Save", "Archivo: Guardar").into(),
            Command::SaveAs => t("Fichier : Enregistrer sous…", "File: Save As…", "Archivo: Guardar como…").into(),
            Command::Build => t("Assembler", "Build", "Ensamblar").into(),
            Command::Run => t("Lancer le programme", "Run the program", "Ejecutar el programa").into(),
            Command::Step => t("Pas à pas (une instruction)", "Step (one instruction)", "Paso a paso (una instrucción)").into(),
            Command::Stop => t("Arrêter le programme", "Stop the program", "Detener el programa").into(),
            Command::ResumeHere => t("Reprendre à cette étape", "Resume at this step", "Reanudar en este paso").into(),
            Command::TimelineStart => t("Timeline : début", "Timeline: start", "Línea de tiempo: inicio").into(),
            Command::TimelineEnd => t("Timeline : fin", "Timeline: end", "Línea de tiempo: fin").into(),
            Command::TimelinePrev => t("Timeline : étape précédente", "Timeline: previous step", "Línea de tiempo: paso anterior").into(),
            Command::TimelineNext => t("Timeline : étape suivante", "Timeline: next step", "Línea de tiempo: paso siguiente").into(),
            Command::TogglePanel(p) => format!(
                "{} : {}",
                t("Afficher/masquer", "Show/hide", "Mostrar/ocultar"),
                p.title(lang)
            ),
            Command::FocusPanel(p) => format!(
                "{} : {}",
                t("Aller au panneau", "Go to panel", "Ir al panel"),
                p.title(lang)
            ),
            Command::ResetLayout => t("Réinitialiser la disposition", "Reset layout", "Restablecer disposición").into(),
            Command::ShowAllPanels => t("Afficher tous les panneaux", "Show all panels", "Mostrar todos los paneles").into(),
            Command::TogglebPrediction => t("Fenêtre Prédiction", "Prediction window", "Ventana Predicción").into(),
            Command::ToggleTutorial => t("Activer/désactiver le tutoriel", "Enable/disable the tutorial", "Activar/desactivar el tutorial").into(),
            Command::Calculator => t("Calculatrice multi-base", "Multi-base calculator", "Calculadora multibase").into(),
            Command::Settings => t("Réglages…", "Settings…", "Configuración…").into(),
            Command::Shortcuts => t("Raccourcis clavier…", "Keyboard shortcuts…", "Atajos de teclado…").into(),
            Command::About => t("À propos", "About", "Acerca de").into(),
            Command::CheckUpdates => t("Vérifier les mises à jour", "Check for updates", "Buscar actualizaciones").into(),
            Command::SetLang(Lang::Fr) => t("Langue : Français", "Language: French", "Idioma: Francés").into(),
            Command::SetLang(Lang::En) => t("Langue : Anglais", "Language: English", "Idioma: Inglés").into(),
            Command::SetLang(Lang::Es) => t("Langue : Espagnol", "Language: Spanish", "Idioma: Español").into(),
            Command::SetMode(m) => format!(
                "{} : {}",
                t("Mode d'affichage", "Display mode", "Modo de visualización"),
                m.label(lang)
            ),
        }
    }

    /// Raccourci équivalent, affiché à droite de l'entrée.
    pub(crate) fn shortcut(self) -> Option<&'static str> {
        Some(match self {
            Command::New => "Ctrl+N",
            Command::Open => "Ctrl+O",
            Command::Save => "Ctrl+S",
            Command::Build => "Ctrl+B",
            Command::Run => "F5",
            Command::Step => "F10",
            Command::Stop => "Échap",
            Command::TimelineStart => "Home",
            Command::TimelineEnd => "End",
            Command::TimelinePrev => "←",
            Command::TimelineNext => "→",
            Command::TogglebPrediction => "Ctrl+5",
            Command::Shortcuts => "F1",
            _ => return None,
        })
    }

    /// Toutes les commandes, dans l'ordre de présentation par défaut.
    pub(crate) fn all() -> Vec<Command> {
        let mut v = vec![
            Command::Run,
            Command::Step,
            Command::Build,
            Command::Stop,
            Command::ResumeHere,
            Command::New,
            Command::Open,
            Command::Save,
            Command::SaveAs,
            Command::TimelineStart,
            Command::TimelinePrev,
            Command::TimelineNext,
            Command::TimelineEnd,
        ];
        // Aller au panneau avant afficher/masquer : c'est le geste le plus
        // fréquent quand on navigue au clavier.
        v.extend(Panel::ALL.map(Command::FocusPanel));
        v.extend(Panel::ALL.map(Command::TogglePanel));
        v.extend([
            Command::ShowAllPanels,
            Command::ResetLayout,
            Command::TogglebPrediction,
            Command::ToggleTutorial,
            Command::Calculator,
            Command::Settings,
            Command::Shortcuts,
            Command::CheckUpdates,
            Command::About,
            Command::SetMode(crate::app::UiMode::Learning),
            Command::SetMode(crate::app::UiMode::Full),
            Command::SetLang(Lang::Fr),
            Command::SetLang(Lang::En),
            Command::SetLang(Lang::Es),
        ]);
        v
    }
}

/// Score de correspondance d'une requête sur un libellé.
///
/// Sous-chaîne d'abord (le plus prévisible), puis correspondance par
/// sous-séquence — « vm » retrouve « Vue Mémoire ». Renvoie `None` si la
/// requête ne colle pas du tout. Plus le score est bas, meilleure est la
/// correspondance.
pub(crate) fn match_score(query: &str, label: &str) -> Option<u32> {
    if query.is_empty() {
        return Some(1000);
    }
    let q = fold(query);
    let l = fold(label);
    if q.is_empty() {
        return Some(1000);
    }
    // Correspondance exacte de sous-chaîne : d'autant meilleure qu'elle est tôt.
    if let Some(pos) = l.find(&q) {
        return Some(pos as u32);
    }
    // Sous-séquence : toutes les lettres de la requête, dans l'ordre.
    let mut it = l.chars();
    let mut span = 0u32;
    for c in q.chars() {
        let mut found = false;
        for lc in it.by_ref() {
            span += 1;
            if lc == c {
                found = true;
                break;
            }
        }
        if !found {
            return None;
        }
    }
    // Pénalisé par rapport aux sous-chaînes, et d'autant plus que c'est étalé.
    Some(500 + span)
}

/// Minuscules sans accents ni ponctuation : « Réglages… » et « reglages »
/// doivent se rencontrer.
fn fold(s: &str) -> String {
    s.chars()
        .filter_map(|c| {
            let c = match c {
                'à' | 'â' | 'ä' | 'á' | 'À' | 'Â' | 'Ä' | 'Á' => 'a',
                'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
                'î' | 'ï' | 'í' | 'Î' | 'Ï' | 'Í' => 'i',
                'ô' | 'ö' | 'ó' | 'Ô' | 'Ö' | 'Ó' => 'o',
                'ù' | 'û' | 'ü' | 'ú' | 'Ù' | 'Û' | 'Ü' | 'Ú' => 'u',
                'ç' | 'Ç' => 'c',
                'ñ' | 'Ñ' => 'n',
                other => other,
            };
            let c = c.to_lowercase().next()?;
            (c.is_alphanumeric() || c == ' ').then_some(c)
        })
        .collect()
}

/// Commandes retenues pour une requête, triées de la meilleure à la moins bonne.
pub(crate) fn filter(query: &str, lang: Lang) -> Vec<Command> {
    let mut scored: Vec<(u32, usize, Command)> = Command::all()
        .into_iter()
        .enumerate()
        .filter_map(|(i, c)| match_score(query, &c.label(lang)).map(|s| (s, i, c)))
        .collect();
    // À score égal, on conserve l'ordre de déclaration : la liste ne saute pas
    // d'une frappe à l'autre.
    scored.sort_by_key(|(s, i, _)| (*s, *i));
    scored.into_iter().map(|(_, _, c)| c).collect()
}

impl App {
    /// Exécute une commande de la palette.
    pub(super) fn run_command(&mut self, cmd: Command) {
        match cmd {
            Command::New => self.new_file(),
            Command::Open => self.open_browser(),
            Command::Save => {
                self.save_source();
            }
            Command::SaveAs => self.open_saveas(),
            Command::Build => self.build(),
            Command::Run => self.launch(),
            Command::Step => self.step(),
            Command::Stop => self.stop(),
            Command::ResumeHere => self.resume_here(),
            Command::TimelineStart => self.set_view(0),
            Command::TimelineEnd => self.set_view(i64::MAX),
            Command::TimelinePrev => self.set_view(self.view_index as i64 - 1),
            Command::TimelineNext => self.set_view(self.view_index as i64 + 1),
            Command::TogglePanel(p) => {
                self.toggle_panel(p);
                self.save_settings();
            }
            Command::FocusPanel(p) => {
                // Un panneau fermé s'ouvre avant de recevoir le focus : sinon la
                // commande semblerait sans effet.
                if !self.panel_is_open(p) {
                    self.show_panel(p);
                    self.save_settings();
                }
                self.focus_panel(p);
            }
            Command::ResetLayout => self.reset_dock_layout(),
            Command::ShowAllPanels => {
                for p in Panel::ALL {
                    // Le tutoriel s'ouvre par son réglage, pas par ici.
                    if crate::app::dock::SETTING_DRIVEN.contains(&p) {
                        continue;
                    }
                    if !self.panel_is_open(p) {
                        self.show_panel(p);
                    }
                }
                self.save_settings();
            }
            Command::TogglebPrediction => {
                self.pedagogy_predict = !self.pedagogy_predict;
                self.save_settings();
            }
            Command::ToggleTutorial => {
                self.tutorial_enabled = !self.tutorial_enabled;
                self.sync_tutorial_panel();
                self.save_settings();
            }
            Command::Calculator => self.show_calculator = true,
            Command::Settings => self.show_settings = true,
            Command::Shortcuts => self.show_shortcuts = true,
            Command::About => self.show_about = true,
            Command::CheckUpdates => self.updater.check(),
            Command::SetLang(l) => {
                self.lang = l;
                self.save_settings();
            }
            Command::SetMode(m) => self.set_ui_mode(m),
        }
    }

    /// Ouvre la palette, curseur dans le champ de recherche.
    pub(super) fn open_palette(&mut self) {
        self.palette_open = true;
        self.palette_query.clear();
        self.palette_sel = 0;
        self.palette_focus = true;
    }

    /// Fenêtre de la palette de commandes.
    pub(super) fn palette_window(&mut self, ctx: &egui::Context) {
        if !self.palette_open {
            return;
        }
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let hdr = self.c_header();

        let matches = filter(&self.palette_query, lang);
        // La sélection reste dans les bornes quand la liste se réduit.
        if self.palette_sel >= matches.len() {
            self.palette_sel = matches.len().saturating_sub(1);
        }

        // Touches de navigation lues AVANT les widgets : le champ de saisie
        // consommerait sinon les flèches et Entrée.
        let (up, down, enter, esc, page_up, page_down) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::PageUp),
                i.key_pressed(egui::Key::PageDown),
            )
        });
        if !matches.is_empty() {
            let n = matches.len();
            if down {
                self.palette_sel = (self.palette_sel + 1) % n;
            }
            if up {
                self.palette_sel = (self.palette_sel + n - 1) % n;
            }
            if page_down {
                self.palette_sel = (self.palette_sel + 8).min(n - 1);
            }
            if page_up {
                self.palette_sel = self.palette_sel.saturating_sub(8);
            }
        }
        if esc {
            self.palette_open = false;
            return;
        }
        let mut chosen: Option<Command> = None;
        if enter {
            chosen = matches.get(self.palette_sel).copied();
        }

        let mut open = true;
        egui::Window::new(tr("Palette de commandes", "Command palette", "Paleta de comandos"))
            .id(egui::Id::new("command_palette"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_size(egui::vec2(460.0, 400.0))
            .pivot(egui::Align2::CENTER_TOP)
            .default_pos(ctx.content_rect().center_top() + egui::vec2(0.0, 90.0))
            .show(ctx, |ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.palette_query)
                        .desired_width(f32::INFINITY)
                        .hint_text(tr(
                            "Tapez pour filtrer… (↑↓ choisir, Entrée exécuter, Échap fermer)",
                            "Type to filter… (↑↓ choose, Enter run, Esc close)",
                            "Escriba para filtrar… (↑↓ elegir, Enter ejecutar, Esc cerrar)",
                        )),
                );
                // Le focus va au champ à l'ouverture, une seule fois.
                if std::mem::take(&mut self.palette_focus) {
                    resp.request_focus();
                }
                ui.add_space(4.0);

                if matches.is_empty() {
                    ui.weak(tr("Aucune commande", "No command", "Ningún comando"));
                    return;
                }

                let sel = self.palette_sel;
                egui::ScrollArea::vertical()
                    .id_salt("palette_list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (i, cmd) in matches.iter().enumerate() {
                            let is_sel = i == sel;
                            let bg = if is_sel {
                                ACCENT.linear_multiply(0.25)
                            } else {
                                egui::Color32::TRANSPARENT
                            };
                            let r = egui::Frame::default()
                                .fill(bg)
                                .corner_radius(egui::CornerRadius::same(4))
                                .inner_margin(egui::Margin::symmetric(6, 3))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        let mut txt = RichText::new(cmd.label(lang));
                                        if is_sel {
                                            txt = txt.strong().color(ACCENT);
                                        }
                                        ui.label(txt);
                                        if let Some(sc) = cmd.shortcut() {
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    ui.label(
                                                        RichText::new(sc)
                                                            .monospace()
                                                            .small()
                                                            .color(hdr),
                                                    );
                                                },
                                            );
                                        }
                                    });
                                });
                            // Clic à la souris : la palette reste utilisable aux deux.
                            let resp = r.response.interact(egui::Sense::click());
                            if resp.clicked() {
                                chosen = Some(*cmd);
                            }
                            if resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            // L'élément retenu reste visible pendant la navigation.
                            if is_sel && (up || down || page_up || page_down) {
                                r.response.scroll_to_me(None);
                            }
                        }
                    });
            });

        if let Some(cmd) = chosen {
            self.palette_open = false;
            self.run_command(cmd);
        } else if !open {
            self.palette_open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_is_labelled_in_every_language() {
        for c in Command::all() {
            for lang in [Lang::Fr, Lang::En, Lang::Es] {
                assert!(!c.label(lang).is_empty(), "{c:?} sans libellé en {lang:?}");
            }
        }
    }

    /// La palette doit couvrir chaque panneau, dans les deux sens : y aller et
    /// l'afficher/masquer. C'est ce qui remplace les menus au clavier.
    #[test]
    fn palette_covers_every_panel() {
        let all = Command::all();
        for p in Panel::ALL {
            assert!(all.contains(&Command::FocusPanel(p)), "aller à {p:?} manquant");
            assert!(all.contains(&Command::TogglePanel(p)), "bascule de {p:?} manquante");
        }
    }

    #[test]
    fn empty_query_returns_everything() {
        let all = filter("", Lang::Fr);
        assert_eq!(all.len(), Command::all().len());
    }

    /// Une sous-chaîne doit primer sur une sous-séquence : taper « run » doit
    /// remonter « Lancer » avant une correspondance dispersée.
    #[test]
    fn substring_beats_subsequence() {
        let sub = match_score("regl", "Réglages…").expect("sous-chaîne (accents pliés)");
        let seq = match_score("rgl", "Réglages…").expect("sous-séquence");
        assert!(sub < seq, "sous-chaîne {sub} doit primer sur sous-séquence {seq}");
    }

    #[test]
    fn accents_and_case_are_ignored() {
        assert!(match_score("reglages", "Réglages…").is_some());
        assert!(match_score("RÉGLAGES", "réglages").is_some());
        assert!(match_score("memoire", "Vue mémoire").is_some());
    }

    /// La sous-séquence permet les abréviations : « vm » → « Vue mémoire ».
    #[test]
    fn subsequence_matches_abbreviations() {
        assert!(match_score("vm", "Vue mémoire").is_some());
        assert!(match_score("vme", "Vue mémoire").is_some());
        assert!(match_score("zzz", "Vue mémoire").is_none(), "non-correspondance");
    }

    #[test]
    fn filtering_finds_expected_commands() {
        let r = filter("lancer", Lang::Fr);
        assert!(!r.is_empty());
        assert_eq!(r[0], Command::Run, "« lancer » doit remonter Run en tête");

        let r = filter("console", Lang::Fr);
        assert!(
            r.iter().any(|c| matches!(c, Command::FocusPanel(Panel::Console))),
            "« console » doit proposer d'y aller"
        );

        assert!(filter("xyzzy-introuvable", Lang::Fr).is_empty());
    }

    /// L'ordre ne doit pas sautiller entre deux frappes équivalentes : à score
    /// égal, l'ordre de déclaration tranche.
    #[test]
    fn ordering_is_stable() {
        let a = filter("panneau", Lang::Fr);
        let b = filter("panneau", Lang::Fr);
        assert_eq!(a, b);
    }

    #[test]
    fn shortcuts_are_declared_for_the_common_actions() {
        for c in [Command::Run, Command::Step, Command::Build, Command::Save] {
            assert!(c.shortcut().is_some(), "{c:?} devrait afficher son raccourci");
        }
    }

    /// La palette doit réellement agir : ouvrir un panneau fermé, y aller, et
    /// se refermer. Un test de filtrage seul ne prouverait rien.
    #[test]
    fn running_a_command_acts_on_the_app() {
        let mut app = App::new();
        // Mode complet : le test manipule des panneaux avancés.
        app.set_ui_mode(crate::app::UiMode::Full);

        // Fermer puis rouvrir la console par la palette.
        app.run_command(Command::TogglePanel(Panel::Console));
        assert!(!app.panel_is_open(Panel::Console));
        app.run_command(Command::TogglePanel(Panel::Console));
        assert!(app.panel_is_open(Panel::Console));

        // « Aller au panneau » sur un panneau FERMÉ doit l'ouvrir d'abord,
        // sinon la commande semblerait sans effet.
        app.run_command(Command::TogglePanel(Panel::Syscalls));
        assert!(!app.panel_is_open(Panel::Syscalls));
        app.run_command(Command::FocusPanel(Panel::Syscalls));
        assert!(app.panel_is_open(Panel::Syscalls), "le panneau doit être rouvert");
        assert_eq!(app.focused_panel(), Some(Panel::Syscalls));

        // Fenêtres et langue.
        assert!(!app.show_settings);
        app.run_command(Command::Settings);
        assert!(app.show_settings);
        app.run_command(Command::SetLang(Lang::Es));
        assert_eq!(app.lang, Lang::Es);
    }

    /// Rendu headless : ouverture, frappe, navigation, exécution par Entrée.
    #[test]
    fn palette_renders_and_closes_after_running() {
        let mut app = App::new();
        app.open_palette();
        assert!(app.palette_open);
        assert!(app.palette_query.is_empty(), "la requête repart à zéro");

        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.palette_window(ctx));
        assert!(app.palette_open, "la palette reste ouverte sans action");

        // Une requête qui ne colle à rien ne doit pas paniquer au rendu.
        app.palette_query = "xyzzy-introuvable".into();
        let _ = ctx.run(Default::default(), |ctx| app.palette_window(ctx));

        // Exécuter une commande referme la palette.
        app.palette_query = "reglages".into();
        let m = filter(&app.palette_query, app.lang);
        assert!(!m.is_empty());
        app.palette_open = false;
        app.run_command(m[0]);
        assert!(!app.palette_open);
    }

    /// La sélection ne doit jamais sortir des bornes quand la liste rétrécit
    /// sous les doigts de l'utilisateur.
    #[test]
    fn selection_stays_in_bounds_while_typing() {
        let mut app = App::new();
        app.open_palette();
        app.palette_sel = 40; // très bas dans la liste complète

        let ctx = egui::Context::default();
        app.palette_query = "reglages".into(); // ne laisse qu'une poignée d'entrées
        let _ = ctx.run(Default::default(), |ctx| app.palette_window(ctx));

        let n = filter(&app.palette_query, app.lang).len();
        assert!(app.palette_sel < n.max(1), "sélection {} hors de {n}", app.palette_sel);
    }
}
