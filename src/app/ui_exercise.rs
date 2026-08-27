//! Module EXERCICES : énoncé et attentes vérifiées.
//!
//! Il est indépendant du parcours guidé. L'onglet s'ouvre seulement lorsqu'un
//! exercice concret est chargé ; le tutoriel garde sa propre grande boîte.

use eframe::egui::{self, RichText};

use super::{App, action, false_col, flag_on, card};
use crate::exercise;
use crate::i18n;

impl App {
    /// Vrai si le fichier courant est un exercice — conditionne l'affichage.
    pub(super) fn has_exercise(&self) -> bool {
        self.exercise.is_exercise()
    }

    /// Module EXERCICES, appelé lorsqu'un fichier auto-corrigé est ouvert.
    pub(super) fn exercise_ui(&mut self, ui: &mut egui::Ui) {
        self.checks_ui(ui);
    }

    /// Attentes du fichier courant et leur verdict.
    pub(super) fn checks_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let hdr = self.c_header();

        // Le panneau est un onglet permanent : quand le fichier courant n'est
        // pas un exercice, il explique comment en écrire un plutôt que de
        // rester vide.
        if !self.has_exercise() {
            ui.add_space(4.0);
            ui.label(
                RichText::new(tr(
                    "Ce fichier ne déclare aucune attente.",
                    "This file declares no expectation.",
                    "Este archivo no declara ninguna expectativa.",
                ))
                .strong()
                .color(hdr),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(tr(
                    "Ajoute des directives en commentaire pour qu'ASM Studio corrige ton \
                     programme automatiquement :",
                    "Add comment directives so ASM Studio checks your program automatically:",
                    "Añade directivas en comentarios para que ASM Studio corrija tu programa:",
                ))
                .small(),
            );
            ui.add_space(4.0);
            card(ui, |ui| {
                ui.label(
                    RichText::new(
                        ";@titre Somme de 1 à 10\n\
                         ;@enonce Laisse 55 dans RBX.\n\
                         ;@attendu rbx == 55\n\
                         ;@attendu exit == 0",
                    )
                    .monospace()
                    .small(),
                );
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(tr(
                    "Les attentes portent sur « exit » ou sur un registre que le programme \
                     n'écrase pas — RAX vaut 60 au moment du sys_exit.",
                    "Expectations target \"exit\" or a register the program does not clobber — \
                     RAX holds 60 at the sys_exit.",
                    "Las expectativas apuntan a «exit» o a un registro que el programa no \
                     sobrescribe — RAX vale 60 en el sys_exit.",
                ))
                .small()
                .weak(),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(tr(
                    "« ;@interdit imul » et « ;@requis rel » contraignent la MANIÈRE : \
                     elles portent sur le texte du code, hors commentaires.",
                    "\";@forbid imul\" and \";@require rel\" constrain the WAY: they target \
                     the code text, comments aside.",
                    "«;@forbid imul» y «;@require rel» limitan el CÓMO: apuntan al texto \
                     del código, sin contar comentarios.",
                ))
                .small()
                .weak(),
            );
            return;
        }

        let mut open_lesson: Option<crate::tutorial::Lesson> = None;
        let (done, total) = exercise::tally(&self.checks);
        let verified = !self.checks.is_empty();
        let all_ok = verified && done == total;

        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if verified {
                    let col = if all_ok { flag_on() } else { false_col() };
                    super::badge(ui, &format!("{done}/{total}"), col);
                } else {
                    super::badge(
                        ui,
                        &format!("{}", self.exercise.requirement_count()),
                        hdr,
                    );
                }
            });
        });

        egui::ScrollArea::vertical()
            .id_salt("exercise_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Titre de l'exercice.
                if let Some(t) = &self.exercise.title {
                    ui.label(RichText::new(t).size(14.0).strong().color(self.c_mnemonic()));
                    ui.add_space(3.0);
                }
                // Énoncé.
                if let Some(s) = &self.exercise.statement {
                    card(ui, |ui| {
                        ui.label(RichText::new(s).size(12.5));
                    });
                    ui.add_space(6.0);
                }

                // La leçon dont relève cet exercice. C'est la réciproque du
                // « S'entraîner » des leçons : l'élève qui ouvre un exercice
                // par l'explorateur, sans savoir par quel bout le prendre,
                // retrouve d'un clic ce qui l'explique.
                if let Some(lesson) = self.lesson_of_current_file() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(tr("Leçon :", "Lesson:", "Lección:"))
                                .small()
                                .weak(),
                        );
                        if ui
                            .link(RichText::new(lesson.title.get(lang)).small())
                            .on_hover_text(tr(
                                "Ouvre la leçon qui explique cette notion",
                                "Open the lesson that explains this topic",
                                "Abre la lección que explica este tema",
                            ))
                            .clicked()
                        {
                            open_lesson = Some(lesson.clone());
                        }
                    });
                    ui.add_space(6.0);
                }

                // Bandeau de réussite, quand tout passe : la récompense.
                if all_ok {
                    egui::Frame::default()
                        .fill(flag_on().linear_multiply(0.16))
                        .stroke(egui::Stroke::new(1.0_f32, flag_on()))
                        .corner_radius(egui::CornerRadius::same(5))
                        .inner_margin(egui::Margin::symmetric(8, 6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(tr("✔ Exercice réussi !", "✔ Exercise passed!", "✔ ¡Ejercicio superado!"))
                                    .strong()
                                    .color(flag_on()),
                            );
                        });
                    ui.add_space(6.0);
                }

                ui.label(
                    RichText::new(tr("Attentes", "Expectations", "Expectativas"))
                        .small()
                        .strong()
                        .color(hdr),
                );
                ui.add_space(2.0);

                // Liste des attentes. Avant exécution, on montre juste ce qui est
                // demandé ; après, le verdict et la valeur obtenue.
                if verified {
                    for c in &self.checks {
                        let ok = c.passed();
                        let (icon, col) = if ok { ("✔", flag_on()) } else { ("✘", false_col()) };
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(icon).strong().color(col));
                            ui.label(RichText::new(c.label()).monospace().color(col));
                            if !ok {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let got = match c.got {
                                            Some(g) => format!("= {g}"),
                                            None => tr("non observé", "not observed", "no observado").to_string(),
                                        };
                                        ui.label(RichText::new(got).monospace().small().color(false_col()));
                                    },
                                );
                            }
                        });
                    }
                } else {
                    let pending = self
                        .exercise
                        .expectations
                        .iter()
                        .map(|e| e.label())
                        .chain(self.exercise.text_rules.iter().map(|r| r.label()));
                    for label in pending {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("○").color(self.c_bytes()));
                            ui.label(RichText::new(label).monospace().color(self.c_bytes()));
                        });
                    }
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(tr(
                            "Exécute le programme jusqu'au bout pour vérifier.",
                            "Run the program to completion to check.",
                            "Ejecuta el programa hasta el final para verificar.",
                        ))
                        .small()
                        .weak(),
                    );
                }

                // Directives mal formées : c'est l'auteur de l'exercice qu'on
                // prévient, pas l'élève — mais mieux vaut le voir que l'ignorer.
                if !self.exercise.errors.is_empty() {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(tr("⚠ Énoncé", "⚠ Statement", "⚠ Enunciado"))
                            .small()
                            .strong()
                            .color(action()),
                    );
                    for e in &self.exercise.errors {
                        ui.label(RichText::new(e).small().monospace().color(action()));
                    }
                }
            });

        if let Some(lesson) = open_lesson {
            self.enter_learning();
            self.tutorial_current = Some(lesson.id.to_string());
            self.save_settings();
        }
    }

    /// Leçon dont relève le fichier actuellement ouvert, s'il s'agit d'un
    /// exercice rattaché au parcours.
    fn lesson_of_current_file(&self) -> Option<crate::tutorial::Lesson> {
        let name = self.src_path.file_name()?.to_str()?;
        crate::tutorial::lesson_of_exercise(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Bout en bout : un exercice réussi et un exercice raté, avec rendu headless.
    #[test]
    fn exercise_is_verified_at_program_exit() {
        let mut app = App::new();
        app.src_path = PathBuf::from("build/ex-test.asm");
        app.out_dir = PathBuf::from("build/ex");
        // 5! = 120 laissé dans RBX, sortie 0.
        app.source = "\
;@titre Factorielle de 5
;@enonce Laisse 120 dans RBX.
;@attendu rbx == 120
;@attendu exit == 0
section .text
    global _start
_start:
    mov rbx, 120
    mov rax, 60
    xor rdi, rdi
    syscall
"
        .to_string();

        app.launch();
        assert!(app.has_exercise(), "le fichier doit être reconnu comme exercice");
        assert_eq!(app.exercise.title.as_deref(), Some("Factorielle de 5"));
        assert!(app.checks.is_empty(), "rien n'est vérifié avant exécution");

        for _ in 0..12 {
            app.step();
        }
        assert_eq!(app.checks.len(), 2, "les deux attentes sont vérifiées");
        assert!(app.checks.iter().all(|c| c.passed()), "checks : {:?}", app.checks);
        assert!(app.status.contains("2/2"), "statut = {}", app.status);

        // Rendu headless du panneau.
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.exercise_ui(ui));
        });
    }

    #[test]
    fn failed_expectation_is_reported() {
        let mut app = App::new();
        app.src_path = PathBuf::from("build/ex-fail.asm");
        app.out_dir = PathBuf::from("build/exf");
        app.source = "\
;@attendu rbx == 120
section .text
    global _start
_start:
    mov rbx, 7
    mov rax, 60
    xor rdi, rdi
    syscall
"
        .to_string();

        app.launch();
        for _ in 0..12 {
            app.step();
        }
        assert_eq!(app.checks.len(), 1);
        assert!(!app.checks[0].passed(), "RBX = 7 ≠ 120");
        assert_eq!(app.checks[0].got, Some(7), "la valeur obtenue est montrée");
        assert!(app.status.contains("0/1"), "statut = {}", app.status);
    }


    /// La fusion : un seul panneau porte le parcours ET la vérification.
    /// Il n'existe plus de panneau Tutoriel séparé.
    #[test]
    fn one_panel_hosts_both_the_path_and_the_checks() {
        use crate::app::dock::Panel;

        // Aucun panneau ne s'appelle « tutorial ».
        assert!(
            Panel::from_key("tutorial").is_none(),
            "le panneau Tutoriel a fusionné dans Exercices"
        );
        assert!(Panel::from_key("exercise").is_some());
        for lang in [crate::i18n::Lang::Fr, crate::i18n::Lang::En, crate::i18n::Lang::Es] {
            let t = Panel::Exercise.title(lang);
            assert!(!t.is_empty(), "titre manquant en {lang:?}");
        }
        // Le titre nomme les deux, et au pluriel : le panneau héberge le
        // parcours ET les attentes du fichier courant. Ne mentionner que les
        // exercices avait une conséquence concrète — un élève qui avait écarté
        // le bandeau d'accueil ne retrouvait plus le tutoriel.
        assert_eq!(Panel::Exercise.title(crate::i18n::Lang::Fr), "Exercices");

        let mut app = App::new();
        let ctx = egui::Context::default();

        // Tutoriel actif, aucune leçon ouverte : sommaire + attentes.
        app.enter_learning();
        app.tutorial_current = None;
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.exercise_ui(ui));
        });

        // Leçon ouverte : son contenu ET ses attentes, au même endroit.
        let lesson = crate::tutorial::find("registres").expect("leçon présente");
        app.load_lesson(&lesson);
        assert!(app.has_exercise(), "la leçon arme ses attentes");
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.exercise_ui(ui));
        });

        // Mode complet : le parcours quitte l'écran avec le mode auquel il
        // appartient — mais le panneau sait revenir, réduit à la vérification,
        // dès qu'un fichier déclare des attentes.
        app.set_ui_mode(crate::app::UiMode::Full);
        assert!(!app.panel_is_open(Panel::Exercise), "le parcours suit son mode");

        app.reload_exercise();
        assert!(
            app.panel_is_open(Panel::Exercise),
            "un exercice ouvert en mode complet rouvre son panneau"
        );
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.exercise_ui(ui));
        });
    }

    /// Le panneau fusionné est un onglet ordinaire — l'exception SETTING_DRIVEN
    /// a disparu avec lui. Il est là au premier lancement, qui se fait en mode
    /// Apprentissage ; c'est le mode complet, et lui seul, qui s'en passe.
    #[test]
    fn the_merged_panel_is_an_ordinary_one() {
        use crate::app::dock::Panel;
        let app = App::new();
        assert!(!app.panel_is_open(Panel::Exercise), "absent au démarrage");
        // Le compte est figé pour qu'un panneau ajouté sans être inscrit dans
        // `ALL` — donc absent du menu Panneaux et de la palette — se voie.
        assert_eq!(Panel::ALL.len(), 16, "seize panneaux, ni plus ni moins");
    }

    /// Un fichier ordinaire ne déclenche ni panneau ni vérification.
    #[test]
    fn plain_program_is_not_an_exercise() {
        let mut app = App::new();
        app.src_path = PathBuf::from("build/ex-plain.asm");
        app.out_dir = PathBuf::from("build/exp");
        app.source = "section .text\n global _start\n_start:\n mov rax,60\n xor rdi,rdi\n syscall\n"
            .to_string();
        app.launch();
        assert!(!app.has_exercise());
        for _ in 0..8 {
            app.step();
        }
        assert!(app.checks.is_empty(), "aucune vérification sur un fichier ordinaire");
    }
}
