//! Garde-fou du travail non enregistré.
//!
//! Quatre gestes remplacent le contenu de l'éditeur sans que l'élève l'ait
//! demandé explicitement : Nouveau, Ouvrir (dialogue ou explorateur), charger
//! une leçon, et quitter. Aucun ne consultait l'état « modifié » : un exercice
//! à moitié écrit disparaissait sans un mot, ce qui est la pire chose qu'un
//! IDE d'apprentissage puisse faire à quelqu'un qui débute.
//!
//! Le motif retenu est celui d'une action différée : le geste est décrit par
//! [`PendingAction`], mis de côté, puis rejoué à l'identique une fois la
//! question tranchée. Les appelants ne changent pas de forme — ils continuent
//! d'appeler `new_file()`, `open_file(path)`, `load_lesson(&lesson)` — c'est
//! l'implémentation de ces trois entrées qui passe désormais par ici.

use std::path::PathBuf;

use eframe::egui::{self, RichText};

use crate::i18n;

use super::{App, dialog_window, false_col};

/// Un geste réclamé par l'élève, qui écraserait le tampon d'édition.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum PendingAction {
    NewFile,
    OpenFile(PathBuf),
    /// Leçon du parcours guidé, retenue par son identifiant : le catalogue la
    /// rendra à l'identique, et on n'a pas à en garder une copie ici.
    LoadLesson(String),
    /// Fermeture de l'application. N'arrive jamais par [`App::guarded`] mais
    /// par l'interception de l'événement de fermeture, qui a besoin d'annuler
    /// celui-ci avant de pouvoir poser la question.
    Quit,
}

impl App {
    /// Passage obligé des actions qui perdent le contenu de l'éditeur.
    ///
    /// Renvoie `true` si l'action a été exécutée sur-le-champ (rien à perdre),
    /// `false` si elle attend la réponse de l'élève. Une action déjà en attente
    /// n'est pas remplacée : la question posée porte sur celle-là, et la
    /// réponse doit valoir pour ce qui a été affiché.
    pub(super) fn guarded(&mut self, action: PendingAction) -> bool {
        if !self.dirty() {
            self.perform_pending(action);
            return true;
        }
        if self.unsaved_prompt.is_none() {
            self.unsaved_prompt = Some(action);
        }
        false
    }

    /// Rejoue le geste mis de côté.
    fn perform_pending(&mut self, action: PendingAction) {
        match action {
            PendingAction::NewFile => self.new_file_now(),
            PendingAction::OpenFile(path) => self.open_file_now(path),
            // Une leçon retirée du catalogue entre-temps (mise à jour) ne doit
            // pas faire paniquer l'application : on ne charge rien.
            PendingAction::LoadLesson(id) => {
                if let Some(lesson) = crate::tutorial::find(&id) {
                    self.load_lesson_now(&lesson);
                }
            }
            // `discard_confirmed` : le `Close` qu'on réémet repasse par
            // `check_close_request`, qui reposerait la question à l'infini
            // puisque le tampon est resté modifié.
            PendingAction::Quit => {
                self.discard_confirmed = true;
                self.quit_requested = true;
            }
        }
    }

    /// Ce que l'action fera, pour que la boîte dise à quoi on consent.
    fn pending_label(&self, action: &PendingAction) -> String {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        match action {
            PendingAction::NewFile => {
                tr("Créer un nouveau fichier", "Create a new file", "Crear un archivo nuevo")
                    .to_string()
            }
            PendingAction::OpenFile(path) => format!(
                "{} {}",
                tr("Ouvrir", "Open", "Abrir"),
                path.file_name().unwrap_or(path.as_os_str()).to_string_lossy()
            ),
            PendingAction::LoadLesson(id) => match crate::tutorial::find(id) {
                Some(lesson) => format!(
                    "{} « {} »",
                    tr("Charger la leçon", "Load the lesson", "Cargar la lección"),
                    lesson.title.get(lang)
                ),
                None => tr("Charger la leçon", "Load the lesson", "Cargar la lección").to_string(),
            },
            PendingAction::Quit => {
                tr("Quitter ASM Studio", "Quit ASM Studio", "Salir de ASM Studio").to_string()
            }
        }
    }

    /// Boîte « Modifications non enregistrées » : enregistrer, abandonner, ou
    /// renoncer à l'action. Sans effet tant qu'aucune action n'attend.
    pub(super) fn unsaved_window(&mut self, ctx: &egui::Context) {
        let Some(action) = self.unsaved_prompt.clone() else { return };
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);

        let name = self
            .src_path
            .file_name()
            .unwrap_or(self.src_path.as_os_str())
            .to_string_lossy()
            .into_owned();
        let what = self.pending_label(&action);
        // Ce qui est en jeu, chiffré : « 12 lignes » parle plus qu'un « des
        // modifications » indistinct, et aide à trancher sans hésiter.
        let changed = changed_line_count(&self.saved_source, &self.source);

        // Trois issues, dont une irréversible : on ne referme pas la boîte à la
        // croix (elle vaudrait « Annuler », mais autant l'exiger explicitement)
        // — `open` reste donc à `true` et seuls les boutons décident.
        let mut choice: Option<Choice> = None;
        dialog_window(ctx, tr(
            "Modifications non enregistrées",
            "Unsaved changes",
            "Cambios sin guardar",
        ))
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_width(400.0);
            ui.add_space(4.0);
            ui.label(RichText::new(name).strong().monospace());
            ui.add_space(2.0);
            ui.label(match lang {
                i18n::Lang::Fr => format!("{changed} ligne(s) modifiée(s) depuis le dernier enregistrement."),
                i18n::Lang::En => format!("{changed} line(s) changed since the last save."),
                i18n::Lang::Es => format!("{changed} línea(s) modificada(s) desde el último guardado."),
            });
            ui.add_space(8.0);
            ui.label(RichText::new(format!("→ {what}")).weak());
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let spacing = 8.0;
                let btn_w = (ui.available_width() - 2.0 * spacing) / 3.0;
                if ui
                    .add_sized(
                        [btn_w, 28.0],
                        egui::Button::new(
                            RichText::new(tr("Enregistrer", "Save", "Guardar")).strong(),
                        )
                        .fill(super::action()),
                    )
                    .clicked()
                {
                    choice = Some(Choice::Save);
                }
                if ui
                    .add_sized(
                        [btn_w, 28.0],
                        egui::Button::new(tr("Abandonner", "Discard", "Descartar"))
                            .stroke(egui::Stroke::new(1.0_f32, false_col())),
                    )
                    .clicked()
                {
                    choice = Some(Choice::Discard);
                }
                if ui
                    .add_sized([btn_w, 28.0], egui::Button::new(tr("Annuler", "Cancel", "Cancelar")))
                    .clicked()
                {
                    choice = Some(Choice::Cancel);
                }
            });
            ui.add_space(4.0);
        });

        match choice {
            // Un échec d'écriture (droits, disque plein) est déjà journalisé par
            // `save_source` : on garde la boîte ouverte plutôt que d'exécuter
            // l'action, sinon l'échec emporterait le travail qu'il fallait sauver.
            Some(Choice::Save) => {
                if self.save_source() {
                    self.unsaved_prompt = None;
                    self.perform_pending(action);
                }
            }
            Some(Choice::Discard) => {
                self.unsaved_prompt = None;
                self.perform_pending(action);
            }
            Some(Choice::Cancel) => self.unsaved_prompt = None,
            None => {}
        }
    }
}

/// Issue choisie dans la boîte.
enum Choice {
    Save,
    Discard,
    Cancel,
}

/// Nombre de lignes qui diffèrent entre deux versions du source.
///
/// Comparaison ligne à ligne, sans algorithme de diff : ce n'est pas un outil
/// de fusion, juste un ordre de grandeur affiché à côté du nom du fichier. Une
/// ligne insérée au début compte donc toutes les suivantes comme décalées, ce
/// qui reste une réponse honnête à « qu'est-ce que je perds ? ».
fn changed_line_count(before: &str, after: &str) -> usize {
    let mut old = before.lines();
    let mut new = after.lines();
    let mut n = 0;
    loop {
        match (old.next(), new.next()) {
            (None, None) => return n,
            (a, b) => {
                if a != b {
                    n += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une application dont le tampon est modifié par rapport au disque.
    fn dirty_app() -> App {
        let mut app = App::new();
        app.saved_source = "mov rax, 1\n".to_string();
        app.source = "mov rax, 2\n".to_string();
        app
    }

    #[test]
    fn a_clean_buffer_lets_the_action_through_immediately() {
        let mut app = App::new();
        assert!(!app.dirty());
        assert!(app.guarded(PendingAction::NewFile), "rien à perdre : exécution directe");
        assert!(app.unsaved_prompt.is_none(), "aucune question à poser");
    }

    #[test]
    fn a_dirty_buffer_holds_the_action_back() {
        let mut app = dirty_app();
        let before = app.source.clone();
        assert!(!app.guarded(PendingAction::NewFile), "l'action doit attendre");
        assert_eq!(app.unsaved_prompt, Some(PendingAction::NewFile));
        assert_eq!(app.source, before, "le travail est intact tant qu'on n'a pas répondu");
    }

    /// La question porte sur l'action affichée : un second geste ne doit pas
    /// s'y substituer en douce (l'élève répondrait à côté).
    #[test]
    fn a_second_action_does_not_replace_the_one_being_asked_about() {
        let mut app = dirty_app();
        app.guarded(PendingAction::NewFile);
        app.guarded(PendingAction::OpenFile(PathBuf::from("/tmp/autre.asm")));
        assert_eq!(app.unsaved_prompt, Some(PendingAction::NewFile));
    }

    /// Les points d'entrée réels passent bien par la garde — c'est ce qui
    /// compte : ce sont eux que les menus, l'explorateur et la palette appellent.
    #[test]
    fn the_public_entry_points_are_all_guarded() {
        let mut app = dirty_app();
        let work = app.source.clone();

        app.new_file();
        assert_eq!(app.source, work, "Nouveau ne doit pas écraser le travail");
        app.unsaved_prompt = None;

        app.open_file(PathBuf::from("/tmp/asm-studio-inexistant.asm"));
        assert_eq!(app.source, work, "Ouvrir non plus");
        app.unsaved_prompt = None;

        if let Some(lesson) = crate::tutorial::catalogue().into_iter().find(|l| l.has_starter()) {
            app.load_lesson(&lesson);
            assert_eq!(app.source, work, "charger une leçon non plus");
            assert!(app.unsaved_prompt.is_some());
        }
    }

    /// « Abandonner » exécute le geste ; le tampon est bien remplacé.
    ///
    /// L'assemblage Windows est coupé ici pour que le geste aille jusqu'au
    /// bout : sinon le nouveau fichier s'arrête sur la question du format, qui
    /// vient APRÈS ce garde-fou et se teste ailleurs (`new_file_tests`).
    #[test]
    fn discarding_runs_the_action() {
        let mut app = dirty_app();
        app.pe_enabled = false;
        app.guarded(PendingAction::NewFile);
        app.unsaved_prompt = None;
        app.perform_pending(PendingAction::NewFile);
        assert!(app.source.contains("_start"), "le squelette a pris la place");
        assert!(!app.dirty(), "un squelette vierge n'est pas du travail à sauver");
    }

    /// L'ordre des deux questions : d'abord le sort du travail en cours,
    /// ENSUITE seulement le format du nouveau fichier. Les poser dans l'autre
    /// sens ferait choisir un format pour un fichier qu'on renonce à créer.
    #[test]
    fn the_format_question_comes_after_the_unsaved_one() {
        let mut app = dirty_app();
        app.pe_enabled = true;
        let work = app.source.clone();

        assert!(!app.guarded(PendingAction::NewFile), "le garde-fou passe en premier");
        assert!(!app.new_file_prompt, "le format ne se demande pas encore");
        assert_eq!(app.source, work);

        // L'élève abandonne son travail : c'est là que le format se demande.
        app.unsaved_prompt = None;
        app.perform_pending(PendingAction::NewFile);
        assert!(app.new_file_prompt, "puis vient la question du format");
        assert_eq!(app.source, work, "et rien n'est écrasé avant la réponse");
    }

    /// Quitter passe par un drapeau : c'est `update` qui tient le viewport.
    #[test]
    fn quitting_asks_for_a_close_and_disarms_the_guard() {
        let mut app = dirty_app();
        app.perform_pending(PendingAction::Quit);
        assert!(app.quit_requested, "la fermeture doit être réémise");
        assert!(app.discard_confirmed, "sans quoi la question se reposerait en boucle");
    }

    /// Une leçon disparue du catalogue ne doit rien casser.
    #[test]
    fn an_unknown_lesson_id_is_ignored() {
        let mut app = dirty_app();
        let before = app.source.clone();
        app.perform_pending(PendingAction::LoadLesson("leçon-inexistante".to_string()));
        assert_eq!(app.source, before);
    }

    /// La boîte se rend sans paniquer, pour chaque forme d'action, et reste
    /// ouverte tant qu'aucun bouton n'est cliqué.
    #[test]
    fn the_window_renders_for_every_action() {
        for action in [
            PendingAction::NewFile,
            PendingAction::OpenFile(PathBuf::from("/tmp/x.asm")),
            PendingAction::LoadLesson("bases-mov".to_string()),
            PendingAction::Quit,
        ] {
            let mut app = dirty_app();
            app.unsaved_prompt = Some(action.clone());
            let ctx = egui::Context::default();
            let _ = ctx.run(Default::default(), |ctx| app.unsaved_window(ctx));
            assert_eq!(app.unsaved_prompt, Some(action), "aucun clic ⇒ rien ne bouge");
        }
    }

    /// Fermée, elle ne peint rien.
    #[test]
    fn a_closed_window_paints_nothing() {
        let mut app = App::new();
        app.unsaved_prompt = None;
        let ctx = egui::Context::default();
        let out = ctx.run(Default::default(), |ctx| app.unsaved_window(ctx));
        assert!(out.shapes.is_empty());
    }

    #[test]
    fn changed_lines_are_counted() {
        assert_eq!(changed_line_count("a\nb\n", "a\nb\n"), 0);
        assert_eq!(changed_line_count("a\nb\n", "a\nB\n"), 1);
        assert_eq!(changed_line_count("a\n", "a\nb\nc\n"), 2, "lignes ajoutées comptées");
        assert_eq!(changed_line_count("a\nb\nc\n", "a\n"), 2, "lignes retirées aussi");
    }

    /// Le cœur de l'affaire : éditer sans enregistrer marque le fichier, et
    /// revenir au texte enregistré efface le marqueur.
    #[test]
    fn dirtiness_follows_the_content_both_ways() {
        let mut app = App::new();
        let original = app.source.clone();
        app.source.push_str("\n    nop\n");
        assert!(app.dirty());
        app.source = original;
        assert!(!app.dirty(), "revenir en arrière ⇒ plus rien à enregistrer");
    }
}
