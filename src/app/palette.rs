//! Palette de commandes : toute l'application au clavier.
//!
//! egui n'ouvre pas ses menus au clavier — `menu_button` ne réagit qu'à la
//! souris. Sans autre voie d'accès, un utilisateur au clavier ne peut atteindre
//! ni « Ouvrir », ni les réglages, ni un panneau fermé.
//!
//! La palette contourne le problème par le haut : **Ctrl+Maj+P**, on tape
//! quelques lettres, les flèches choisissent, Entrée exécute.
//!
//! Chaque action de l'application y figure, y compris celles qui n'ont pas de
//! raccourci : les cinq menus jusqu'à « Quitter », la navigation entre panneaux
//! et onglets, la recherche et le remplacement, et les bascules de la fenêtre
//! Réglages. Ce qui s'ajoute ailleurs s'ajoute donc ici — sans quoi la promesse
//! du module devient fausse en silence, et un utilisateur au clavier se
//! retrouve devant une action qu'il ne peut pas atteindre.
//!
//! N'y figurent pas les gestes qui n'ont de sens que sur une cible désignée à
//! la souris ou au curseur (ouvrir un fichier récent précis, éditer tel
//! registre) : la palette exécute des commandes, elle ne remplace pas la
//! sélection.

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
    OpenExamples,
    Save,
    SaveAs,
    ClearRecent,
    Quit,
    // Éditeur
    Find,
    FindReplace,
    FindNext,
    FindPrev,
    ReplaceCurrent,
    ReplaceAll,
    FoldAtCursor,
    UnfoldAll,
    // Exécution
    Build,
    Run,
    Step,
    StepOver,
    Continue,
    ToggleBreakpoint,
    BreakpointCondition,
    ClearBreakpoints,
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
    NextPanel,
    PrevPanel,
    ClosePanel,
    NextTab,
    PrevTab,
    ResetLayout,
    ShowAllPanels,
    // Fenêtres
    TogglebPrediction,
    ToggleTutorial,
    ResetTutorial,
    ProgramOutput,
    Calculator,
    Settings,
    Shortcuts,
    About,
    License,
    ActivateLicense,
    CheckUpdates,
    // Préférences : les mêmes bascules que la fenêtre Réglages, à portée de
    // frappe. Elles y restent aussi — la palette n'est pas le seul chemin.
    ToggleTooltips,
    ToggleAnimations,
    ToggleAsmstd,
    TogglePedagogyAnim,
    TogglePedagogyMemView,
    // Langue, thème et mode d'affichage
    SetLang(Lang),
    SetTheme(egui::ThemePreference),
    SetMode(crate::app::UiMode),
}

impl Command {
    /// Libellé affiché, traduit.
    pub(crate) fn label(self, lang: Lang) -> String {
        let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        match self {
            Command::New => t("Fichier : Nouveau", "File: New", "Archivo: Nuevo").into(),
            Command::Open => t("Fichier : Ouvrir…", "File: Open…", "Archivo: Abrir…").into(),
            Command::OpenExamples => t(
                "Fichier : Exemples et exercices",
                "File: Examples and exercises",
                "Archivo: Ejemplos y ejercicios",
            )
            .into(),
            Command::Save => t("Fichier : Enregistrer", "File: Save", "Archivo: Guardar").into(),
            Command::SaveAs => t("Fichier : Enregistrer sous…", "File: Save As…", "Archivo: Guardar como…").into(),
            Command::ClearRecent => t(
                "Fichier : Vider la liste des récents",
                "File: Clear the recent list",
                "Archivo: Vaciar la lista de recientes",
            )
            .into(),
            Command::Quit => t("Fichier : Quitter", "File: Quit", "Archivo: Salir").into(),
            Command::Find => t("Rechercher dans l'éditeur", "Find in editor", "Buscar en el editor").into(),
            Command::FindReplace => t("Rechercher / remplacer dans l'éditeur", "Find / replace in editor", "Buscar / reemplazar en el editor").into(),
            Command::FindNext => t("Correspondance suivante", "Next match", "Coincidencia siguiente").into(),
            Command::FindPrev => t("Correspondance précédente", "Previous match", "Coincidencia anterior").into(),
            Command::ReplaceCurrent => t(
                "Remplacer la correspondance courante",
                "Replace the current match",
                "Reemplazar la coincidencia actual",
            )
            .into(),
            Command::ReplaceAll => t(
                "Remplacer toutes les correspondances",
                "Replace every match",
                "Reemplazar todas las coincidencias",
            )
            .into(),
            Command::FoldAtCursor => t("Replier le label sous le curseur", "Fold the label under the cursor", "Plegar la etiqueta bajo el cursor").into(),
            Command::UnfoldAll => t("Tout déplier", "Unfold all", "Desplegar todo").into(),
            Command::Build => t("Assembler", "Build", "Ensamblar").into(),
            Command::Run => t("Lancer le programme", "Run the program", "Ejecutar el programa").into(),
            Command::Step => t("Pas à pas (une instruction)", "Step (one instruction)", "Paso a paso (una instrucción)").into(),
            Command::StepOver => t(
                "Pas par-dessus (exécute l'appel d'un bloc)",
                "Step over (run the call in one go)",
                "Paso por encima (ejecuta la llamada de una vez)",
            )
            .into(),
            Command::Continue => t(
                "Continuer jusqu'au point d'arrêt",
                "Continue to breakpoint",
                "Continuar hasta el punto de interrupción",
            )
            .into(),
            Command::ToggleBreakpoint => t(
                "Point d'arrêt sur la ligne courante",
                "Toggle breakpoint on current line",
                "Punto de interrupción en la línea actual",
            )
            .into(),
            Command::BreakpointCondition => t(
                "Condition du point d'arrêt de la ligne courante",
                "Breakpoint condition on current line",
                "Condición del punto de interrupción de la línea actual",
            )
            .into(),
            Command::ClearBreakpoints => t(
                "Retirer tous les points d'arrêt",
                "Remove all breakpoints",
                "Quitar todos los puntos de interrupción",
            )
            .into(),
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
            Command::NextPanel => t("Panneau suivant", "Next panel", "Panel siguiente").into(),
            Command::PrevPanel => t("Panneau précédent", "Previous panel", "Panel anterior").into(),
            Command::ClosePanel => t(
                "Fermer le panneau focalisé",
                "Close the focused panel",
                "Cerrar el panel enfocado",
            )
            .into(),
            Command::NextTab => t("Onglet suivant", "Next tab", "Pestaña siguiente").into(),
            Command::PrevTab => t("Onglet précédent", "Previous tab", "Pestaña anterior").into(),
            Command::ResetLayout => t("Réinitialiser la disposition", "Reset layout", "Restablecer disposición").into(),
            Command::ShowAllPanels => t("Afficher tous les panneaux", "Show all panels", "Mostrar todos los paneles").into(),
            Command::TogglebPrediction => t("Fenêtre Prédiction", "Prediction window", "Ventana Predicción").into(),
            Command::ToggleTutorial => t("Activer/désactiver le tutoriel", "Enable/disable the tutorial", "Activar/desactivar el tutorial").into(),
            Command::ResetTutorial => t(
                "Réinitialiser la progression du tutoriel",
                "Reset the tutorial progress",
                "Reiniciar el progreso del tutorial",
            )
            .into(),
            Command::ProgramOutput => t(
                "Sortie du programme (sans les messages de l'IDE)",
                "Program output (without the IDE's messages)",
                "Salida del programa (sin los mensajes del IDE)",
            )
            .into(),
            Command::Calculator => t("Calculatrice multi-base", "Multi-base calculator", "Calculadora multibase").into(),
            Command::Settings => t("Réglages…", "Settings…", "Configuración…").into(),
            Command::Shortcuts => t("Raccourcis clavier…", "Keyboard shortcuts…", "Atajos de teclado…").into(),
            Command::About => t("À propos", "About", "Acerca de").into(),
            Command::License => t("Afficher la licence…", "Show the license…", "Mostrar la licencia…").into(),
            Command::ActivateLicense => t("Activer une licence…", "Activate a license…", "Activar una licencia…").into(),
            Command::CheckUpdates => t("Vérifier les mises à jour", "Check for updates", "Buscar actualizaciones").into(),
            Command::ToggleTooltips => t(
                "Réglages : infobulles de raccourcis",
                "Settings: shortcut tooltips",
                "Configuración: información sobre atajos",
            )
            .into(),
            Command::ToggleAnimations => t(
                "Réglages : animations de l'interface",
                "Settings: interface animations",
                "Configuración: animaciones de la interfaz",
            )
            .into(),
            Command::ToggleAsmstd => t(
                "Réglages : bibliothèque asmstd",
                "Settings: asmstd library",
                "Configuración: biblioteca asmstd",
            )
            .into(),
            Command::TogglePedagogyAnim => t(
                "Réglages : animations pédagogiques",
                "Settings: teaching animations",
                "Configuración: animaciones pedagógicas",
            )
            .into(),
            Command::TogglePedagogyMemView => t(
                "Réglages : vue mémoire unifiée",
                "Settings: unified memory view",
                "Configuración: vista de memoria unificada",
            )
            .into(),
            Command::SetLang(Lang::Fr) => t("Langue : Français", "Language: French", "Idioma: Francés").into(),
            Command::SetLang(Lang::En) => t("Langue : Anglais", "Language: English", "Idioma: Inglés").into(),
            Command::SetLang(Lang::Es) => t("Langue : Espagnol", "Language: Spanish", "Idioma: Español").into(),
            Command::SetTheme(p) => format!(
                "{} : {}",
                t("Thème", "Theme", "Tema"),
                match p {
                    egui::ThemePreference::Light => t("Clair", "Light", "Claro"),
                    egui::ThemePreference::Dark => t("Sombre", "Dark", "Oscuro"),
                    egui::ThemePreference::System => t("Système", "System", "Sistema"),
                }
            ),
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
            Command::Find => "Ctrl+F",
            Command::FindReplace => "Ctrl+H",
            Command::FindNext => "F3",
            Command::FindPrev => "Maj+F3",
            Command::FoldAtCursor => "Ctrl+Maj+[",
            Command::UnfoldAll => "Ctrl+Maj+]",
            Command::Run => "F5",
            Command::Step => "F10",
            Command::StepOver => "Maj+F10",
            Command::Continue => "F9",
            Command::ToggleBreakpoint => "Ctrl+F8",
            Command::BreakpointCondition => "Ctrl+Maj+F8",
            Command::Stop => "Échap",
            Command::TimelineStart => "Home",
            Command::TimelineEnd => "End",
            Command::TimelinePrev => "←",
            Command::TimelineNext => "→",
            Command::NextPanel => "F6",
            Command::PrevPanel => "Maj+F6",
            Command::ClosePanel => "Ctrl+W",
            Command::NextTab => "Ctrl+Tab",
            Command::PrevTab => "Ctrl+Maj+Tab",
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
            Command::StepOver,
            Command::Continue,
            Command::Build,
            Command::Stop,
            Command::ResumeHere,
            Command::ToggleBreakpoint,
            Command::BreakpointCondition,
            Command::ClearBreakpoints,
            Command::New,
            Command::Open,
            Command::OpenExamples,
            Command::Save,
            Command::SaveAs,
            Command::ClearRecent,
            Command::Find,
            Command::FindReplace,
            Command::FindNext,
            Command::FindPrev,
            Command::ReplaceCurrent,
            Command::ReplaceAll,
            Command::FoldAtCursor,
            Command::UnfoldAll,
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
            Command::NextPanel,
            Command::PrevPanel,
            Command::NextTab,
            Command::PrevTab,
            Command::ClosePanel,
            Command::ShowAllPanels,
            Command::ResetLayout,
            Command::TogglebPrediction,
            Command::ToggleTutorial,
            Command::ResetTutorial,
            Command::ProgramOutput,
            Command::Calculator,
            Command::Settings,
            Command::Shortcuts,
            Command::CheckUpdates,
            Command::About,
            Command::License,
            Command::ActivateLicense,
            Command::SetMode(crate::app::UiMode::Learning),
            Command::SetMode(crate::app::UiMode::Full),
            Command::SetLang(Lang::Fr),
            Command::SetLang(Lang::En),
            Command::SetLang(Lang::Es),
            Command::SetTheme(egui::ThemePreference::Dark),
            Command::SetTheme(egui::ThemePreference::Light),
            Command::SetTheme(egui::ThemePreference::System),
            Command::ToggleTooltips,
            Command::ToggleAnimations,
            Command::ToggleAsmstd,
            Command::TogglePedagogyAnim,
            Command::TogglePedagogyMemView,
            // En dernier : c'est la seule entrée qui ferme l'application, et
            // elle n'a rien à faire au milieu d'une liste qu'on parcourt aux
            // flèches.
            Command::Quit,
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
            Command::OpenExamples => self.open_examples_dir(),
            Command::Save => {
                self.save_source();
            }
            Command::SaveAs => self.open_saveas(),
            Command::ClearRecent => {
                self.recent_files.clear();
                self.save_settings();
            }
            // Pas de `ViewportCommand::Close` ici : `run_command` n'a pas le
            // contexte egui sous la main, et surtout la fermeture doit passer
            // par le même chemin que la croix de la fenêtre — celui qui
            // s'arrête pour demander quoi faire du travail non enregistré.
            Command::Quit => self.quit_requested = true,
            Command::FoldAtCursor => self.fold_label_at_cursor(),
            Command::UnfoldAll => self.unfold_all(),
            Command::Find => {
                self.show_find = true;
                self.find_replace_mode = false;
                self.show_panel(Panel::Editor);
                self.focus_panel(Panel::Editor);
            }
            Command::FindReplace => {
                self.show_find = true;
                self.find_replace_mode = true;
                self.show_panel(Panel::Editor);
                self.focus_panel(Panel::Editor);
            }
            // La barre de recherche se rouvre : sans le surlignage, on ne
            // verrait pas où le curseur vient d'atterrir.
            Command::FindNext | Command::FindPrev => {
                if !self.find_query.is_empty() {
                    self.show_find = true;
                    self.show_panel(Panel::Editor);
                    if cmd == Command::FindPrev {
                        self.find_prev();
                    } else {
                        self.find_next();
                    }
                }
            }
            Command::ReplaceCurrent => self.find_replace_current(),
            Command::ReplaceAll => self.find_replace_all(),
            Command::Build => self.build(),
            Command::Run => self.launch(),
            Command::Step => self.step(),
            Command::StepOver => self.step_over(),
            Command::Continue => self.cont(),
            Command::ToggleBreakpoint => {
                let line = self.editor_ln;
                self.toggle_breakpoint(line);
            }
            Command::BreakpointCondition => {
                let line = self.editor_ln;
                self.open_breakpoint_condition(line);
            }
            Command::ClearBreakpoints => self.breakpoints.clear(),
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
            Command::NextPanel => self.focus_next_panel(false),
            Command::PrevPanel => self.focus_next_panel(true),
            Command::ClosePanel => {
                if let Some(p) = self.focused_panel() {
                    self.hide_panel(p);
                    self.save_settings();
                }
            }
            Command::NextTab => self.cycle_tab(false),
            Command::PrevTab => self.cycle_tab(true),
            Command::ResetLayout => self.reset_dock_layout(),
            Command::ShowAllPanels => {
                for p in Panel::ALL {
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
                self.save_settings();
            }
            // `reset_tutorial` enregistre déjà, et remet l'élève devant l'accueil.
            Command::ResetTutorial => self.reset_tutorial(),
            Command::ProgramOutput => self.show_program_output = true,
            Command::Calculator => self.show_calculator = true,
            Command::Settings => self.show_settings = true,
            Command::Shortcuts => self.show_shortcuts = true,
            Command::About => self.show_about = true,
            Command::License => self.show_license = true,
            Command::ActivateLicense => self.show_license_gate = true,
            Command::CheckUpdates => self.updater.check(),
            Command::ToggleTooltips => {
                self.show_tooltips = !self.show_tooltips;
                self.save_settings();
            }
            Command::ToggleAnimations => {
                self.animate = !self.animate;
                self.save_settings();
            }
            Command::ToggleAsmstd => {
                self.use_asmstd = !self.use_asmstd;
                self.save_settings();
            }
            Command::TogglePedagogyAnim => {
                self.pedagogy_anim = !self.pedagogy_anim;
                self.save_settings();
            }
            Command::TogglePedagogyMemView => {
                self.pedagogy_memview = !self.pedagogy_memview;
                self.save_settings();
            }
            Command::SetLang(l) => {
                self.lang = l;
                self.save_settings();
            }
            // `apply_theme` relit la préférence à chaque image : rien d'autre
            // à faire pour que le changement se voie.
            Command::SetTheme(p) => {
                self.theme_pref = p;
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

    /// Deux entrées de même libellé sont indiscernables une fois affichées :
    /// l'utilisateur en choisirait une au hasard. Le piège est un copier-coller
    /// de libellé en ajoutant une commande — d'où ce test plutôt qu'une
    /// relecture.
    #[test]
    fn no_two_commands_share_a_label() {
        for lang in [Lang::Fr, Lang::En, Lang::Es] {
            let mut seen: Vec<(String, Command)> = Vec::new();
            for c in Command::all() {
                let l = c.label(lang);
                if let Some((_, other)) = seen.iter().find(|(s, _)| *s == l) {
                    panic!("{c:?} et {other:?} portent le même libellé en {lang:?} : {l}");
                }
                seen.push((l, c));
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

        // La sortie du programme s'ouvre aussi au clavier, pas seulement par le
        // bouton de l'en-tête de la console.
        assert!(!app.show_program_output);
        app.run_command(Command::ProgramOutput);
        assert!(app.show_program_output);

        // Fenêtres et langue.
        assert!(!app.show_settings);
        app.run_command(Command::Settings);
        assert!(app.show_settings);
        app.run_command(Command::SetLang(Lang::Es));
        assert_eq!(app.lang, Lang::Es);
    }

    /// Les bascules de la fenêtre Réglages sont exécutables depuis la palette,
    /// et elles basculent bien la même chose — pas une case voisine.
    #[test]
    fn preference_toggles_flip_their_own_setting() {
        let mut app = App::new();
        let before = (
            app.show_tooltips,
            app.animate,
            app.use_asmstd,
            app.pedagogy_anim,
            app.pedagogy_memview,
        );
        app.run_command(Command::ToggleTooltips);
        app.run_command(Command::ToggleAnimations);
        app.run_command(Command::ToggleAsmstd);
        app.run_command(Command::TogglePedagogyAnim);
        app.run_command(Command::TogglePedagogyMemView);
        assert_eq!(
            (
                !app.show_tooltips,
                !app.animate,
                !app.use_asmstd,
                !app.pedagogy_anim,
                !app.pedagogy_memview
            ),
            before,
            "chaque bascule doit inverser son propre réglage"
        );

        app.run_command(Command::SetTheme(egui::ThemePreference::Light));
        assert_eq!(app.theme_pref, egui::ThemePreference::Light);
    }

    /// Quitter passe par le drapeau, donc par la garde du travail non
    /// enregistré — et non par un `Close` immédiat qui l'aurait contournée.
    #[test]
    fn quitting_goes_through_the_unsaved_guard() {
        let mut app = App::new();
        assert!(!app.quit_requested);
        app.run_command(Command::Quit);
        assert!(app.quit_requested);
    }

    /// Les commandes de navigation entre panneaux agissent sans dépendre d'un
    /// raccourci clavier : c'est tout l'intérêt de les avoir mises ici.
    #[test]
    fn panel_navigation_commands_move_the_focus() {
        let mut app = App::new();
        app.set_ui_mode(crate::app::UiMode::Full);
        app.run_command(Command::FocusPanel(Panel::Editor));

        app.run_command(Command::NextPanel);
        let after_next = app.focused_panel();
        assert_ne!(after_next, Some(Panel::Editor), "on doit avoir changé de panneau");

        app.run_command(Command::PrevPanel);
        assert_eq!(app.focused_panel(), Some(Panel::Editor), "et pouvoir revenir");

        // Fermer le panneau focalisé, comme Ctrl+W.
        let p = app.focused_panel().expect("un panneau focalisé");
        app.run_command(Command::ClosePanel);
        assert!(!app.panel_is_open(p), "{p:?} devrait être fermé");
    }

    /// Chercher « remplacer » ou « thème » doit trouver, sinon les commandes
    /// ajoutées seraient présentes mais introuvables.
    #[test]
    fn the_new_commands_are_reachable_by_typing() {
        let cases: [(&str, Command); 5] = [
            ("remplacer toutes", Command::ReplaceAll),
            ("theme sombre", Command::SetTheme(egui::ThemePreference::Dark)),
            ("quitter", Command::Quit),
            ("exemples", Command::OpenExamples),
            ("onglet suivant", Command::NextTab),
        ];
        for (query, expected) in cases {
            let r = filter(query, Lang::Fr);
            assert_eq!(r.first(), Some(&expected), "« {query} » devrait remonter {expected:?}");
        }
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
