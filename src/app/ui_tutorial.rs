//! Panneau TUTORIEL : le parcours guidé, relié aux outils de l'IDE.
//!
//! Charger une leçon ne se contente pas d'afficher son texte : le programme de
//! départ entre dans l'éditeur, les panneaux dont la leçon parle s'ouvrent, et
//! ses attentes arment le panneau Exercice. L'élève lit et manipule au même
//! endroit.

use eframe::egui::{self, RichText};

use super::dock::Panel;
use super::{ACCENT, App, FLAG_ON, card};
use crate::i18n;
use crate::tutorial::{self, Level, Lesson};

impl App {
    /// Charge une leçon : programme de départ, panneaux, et mise au point.
    pub(super) fn load_lesson(&mut self, lesson: &Lesson) {
        self.tutorial_current = Some(lesson.id.to_string());

        if let Some(src) = lesson.starter {
            self.source = src.to_string();
            // Le fichier prend le nom de la leçon : l'élève retrouve son travail
            // dans l'explorateur, et peut l'enregistrer sans écraser un exemple.
            self.src_path = super::data_dir()
                .join("tutoriel")
                .join(format!("{}.asm", lesson.id));
            if let Some(dir) = self.src_path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            self.dirty = true;
            self.dbg = None;
            self.disasm.clear();
            self.binary = None;
            // Arme les attentes portées par le programme lui-même.
            self.reload_exercise();
        }

        // Ouvre ce dont la leçon parle : elle met sous les yeux ce qu'elle explique.
        for key in &lesson.panels {
            if let Some(p) = Panel::from_key(key) {
                if !self.panel_is_open(p) {
                    self.show_panel(p);
                }
            }
        }
        self.save_settings();
    }

    pub(super) fn tutorial_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let hdr = self.c_header();

        // Leçon ouverte : on affiche son contenu, avec un retour au sommaire.
        let current = self
            .tutorial_current
            .clone()
            .and_then(|id| tutorial::find(&id));

        if let Some(lesson) = current {
            self.lesson_ui(ui, &lesson);
            return;
        }

        // --- Sommaire ---
        ui.add_space(2.0);
        ui.label(
            RichText::new(tr(
                "Un parcours en quatre niveaux. Chaque leçon charge son programme et ouvre les panneaux qu'elle explique.",
                "A four-level path. Each lesson loads its program and opens the panels it explains.",
                "Un recorrido de cuatro niveles. Cada lección carga su programa y abre los paneles que explica.",
            ))
            .size(12.0)
            .weak(),
        );
        ui.add_space(6.0);

        let mut to_load: Option<Lesson> = None;
        egui::ScrollArea::vertical()
            .id_salt("tutorial_toc")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for level in Level::ALL {
                    let (done, total) = self.tutorial_progress.tally(level);
                    let complete = total > 0 && done == total;
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(level.title(lang))
                                .strong()
                                .color(if complete { FLAG_ON } else { ACCENT }),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            super::badge(
                                ui,
                                &format!("{done}/{total}"),
                                if complete { FLAG_ON } else { hdr },
                            );
                        });
                    });
                    ui.add_space(2.0);

                    for lesson in tutorial::lessons_of(level) {
                        let done = self.tutorial_progress.is_done(lesson.id);
                        // Une leçon sans étape n'est pas encore écrite : on la
                        // montre pour que le parcours soit lisible, mais grisée.
                        let planned = lesson.steps.is_empty();
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(if done { "✔" } else { "○" })
                                    .color(if done { FLAG_ON } else { self.c_bytes() }),
                            );
                            let mut txt = RichText::new(lesson.title.get(lang)).size(12.5);
                            if planned {
                                txt = txt.weak();
                            }
                            let resp = ui.add(egui::Label::new(txt).sense(egui::Sense::click()));
                            if resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if resp.clicked() {
                                to_load = Some(lesson.clone());
                            }
                            if planned {
                                ui.label(
                                    RichText::new(tr("(à venir)", "(planned)", "(por venir)"))
                                        .small()
                                        .weak(),
                                );
                            }
                        });
                    }
                    ui.add_space(8.0);
                }
            });

        if let Some(l) = to_load {
            // Une leçon non écrite s'ouvre quand même : son objectif se lit.
            self.tutorial_current = Some(l.id.to_string());
            if l.has_starter() {
                self.load_lesson(&l);
            }
        }
    }

    /// Contenu d'une leçon ouverte.
    fn lesson_ui(&mut self, ui: &mut egui::Ui, lesson: &Lesson) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let hdr = self.c_header();
        let mut back = false;
        let mut reload = false;
        let mut toggle_done = false;
        let done = self.tutorial_progress.is_done(lesson.id);

        ui.horizontal(|ui| {
            if ui
                .button(RichText::new(tr("← Sommaire", "← Contents", "← Índice")).small())
                .clicked()
            {
                back = true;
            }
            ui.label(
                RichText::new(lesson.level.title(lang))
                    .small()
                    .weak(),
            );
        });
        ui.add_space(4.0);
        ui.label(RichText::new(lesson.title.get(lang)).size(15.0).strong().color(self.c_mnemonic()));
        ui.add_space(4.0);

        egui::ScrollArea::vertical()
            .id_salt("tutorial_lesson")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                card(ui, |ui| {
                    ui.label(
                        RichText::new(tr("Objectif", "Goal", "Objetivo"))
                            .small()
                            .strong()
                            .color(hdr),
                    );
                    ui.add_space(2.0);
                    ui.label(RichText::new(lesson.goal.get(lang)).size(12.5));
                });
                ui.add_space(8.0);

                if lesson.steps.is_empty() {
                    ui.label(
                        RichText::new(tr(
                            "Cette leçon est annoncée mais pas encore rédigée. Son objectif figure ci-dessus pour situer la suite du parcours.",
                            "This lesson is planned but not written yet. Its goal appears above so you can see where the path leads.",
                            "Esta lección está prevista pero aún no redactada. Su objetivo aparece arriba.",
                        ))
                        .size(12.0)
                        .weak(),
                    );
                    return;
                }

                for (i, step) in lesson.steps.iter().enumerate() {
                    ui.horizontal_top(|ui| {
                        ui.label(
                            RichText::new(format!("{}.", i + 1))
                                .monospace()
                                .strong()
                                .color(ACCENT),
                        );
                        ui.add(egui::Label::new(RichText::new(step.get(lang)).size(12.5)).wrap());
                    });
                    ui.add_space(6.0);
                }

                if lesson.has_starter() {
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(tr(
                            "Le programme de la leçon est dans l'éditeur, avec ses attentes. Appuyez sur F5 : le panneau Exercice dira si c'est bon.",
                            "The lesson's program is in the editor, with its expectations. Press F5: the Exercise panel will tell you if it is right.",
                            "El programa de la lección está en el editor, con sus expectativas. Pulse F5: el panel Ejercicio dirá si está bien.",
                        ))
                        .size(12.0)
                        .weak(),
                    );
                    ui.add_space(6.0);
                    if ui
                        .button(tr("↻ Recharger le programme", "↻ Reload the program", "↻ Recargar el programa"))
                        .on_hover_text(tr(
                            "Remet le programme de départ — vos modifications seront perdues.",
                            "Restores the starting program — your changes will be lost.",
                            "Restaura el programa inicial — sus cambios se perderán.",
                        ))
                        .clicked()
                    {
                        reload = true;
                    }
                }
            });

        ui.separator();
        ui.horizontal(|ui| {
            let label = if done {
                tr("✔ Terminée — annuler", "✔ Done — undo", "✔ Terminada — deshacer")
            } else {
                tr("Marquer comme terminée", "Mark as done", "Marcar como terminada")
            };
            if ui.button(label).clicked() {
                toggle_done = true;
            }
        });

        if reload {
            self.load_lesson(lesson);
        }
        if toggle_done {
            if done {
                self.tutorial_progress.mark_undone(lesson.id);
            } else {
                self.tutorial_progress.mark_done(lesson.id);
                // Enchaîner : la leçon suivante s'ouvre d'elle-même.
                if let Some(next) = self.tutorial_progress.next_lesson() {
                    self.tutorial_current = Some(next.id.to_string());
                    if next.has_starter() {
                        self.load_lesson(&next);
                    }
                } else {
                    self.tutorial_current = None;
                }
            }
            self.save_settings();
        }
        if back {
            self.tutorial_current = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Charger une leçon doit VRAIMENT piloter l'IDE : programme dans
    /// l'éditeur, attentes armées, panneaux ouverts.
    #[test]
    fn loading_a_lesson_drives_the_ide() {
        let mut app = App::new();
        let lesson = tutorial::find("registres").expect("leçon présente");

        app.load_lesson(&lesson);

        assert!(app.source.contains("mov rbx, rax"), "le programme entre dans l'éditeur");
        assert!(app.has_exercise(), "les attentes de la leçon sont armées");
        assert_eq!(app.exercise.expectations.len(), 2, "rbx == 100 et exit == 0");
        for key in &lesson.panels {
            let p = Panel::from_key(key).expect("clé de panneau valide");
            assert!(app.panel_is_open(p), "{key} doit être ouvert par la leçon");
        }
        assert_eq!(app.tutorial_current.as_deref(), Some("registres"));
    }

    /// Toutes les clés de panneau du catalogue doivent exister : une faute de
    /// frappe laisserait une leçon incapable d'ouvrir ce dont elle parle.
    #[test]
    fn every_lesson_panel_key_is_valid() {
        for l in tutorial::catalogue() {
            for key in &l.panels {
                assert!(
                    Panel::from_key(key).is_some(),
                    "leçon « {} » : clé de panneau inconnue « {key} »",
                    l.id
                );
            }
        }
    }

    /// Terminer une leçon enchaîne sur la suivante.
    #[test]
    fn finishing_a_lesson_opens_the_next_one() {
        let mut app = App::new();
        app.tutorial_progress.mark_done("installation");
        app.tutorial_current = Some("premier_programme".into());
        app.tutorial_progress.mark_done("premier_programme");

        let next = app.tutorial_progress.next_lesson().expect("il en reste");
        assert_eq!(next.id, "registres", "l'ordre du parcours est respecté");
    }

    /// Le panneau se rend dans les deux états — sommaire et leçon ouverte.
    #[test]
    fn tutorial_panel_renders_in_both_states() {
        let mut app = App::new();
        let ctx = egui::Context::default();

        app.tutorial_current = None;
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.tutorial_ui(ui));
        });

        app.tutorial_current = Some("pile".into());
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.tutorial_ui(ui));
        });

        // Une leçon annoncée mais non rédigée doit se rendre sans paniquer.
        app.tutorial_current = Some("shellcode".into());
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.tutorial_ui(ui));
        });

        // Un identifiant disparu retombe sur le sommaire au lieu de planter.
        app.tutorial_current = Some("lecon_inexistante".into());
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.tutorial_ui(ui));
        });
    }

    /// Bout en bout : activer le tutoriel, charger une leçon, la RÉSOUDRE, et
    /// voir le panneau Exercice la valider. C'est la promesse du module.
    #[test]
    fn a_lesson_can_be_solved_and_is_validated() {
        use std::path::PathBuf;
        let mut app = App::new();
        app.tutorial_enabled = true;
        app.sync_tutorial_panel();
        assert!(app.panel_is_open(Panel::Tutorial));

        let lesson = tutorial::find("premier_programme").expect("leçon présente");
        app.load_lesson(&lesson);
        app.out_dir = PathBuf::from("build/tuto-e2e");
        app.src_path = PathBuf::from("build/tuto-e2e/premier.asm");

        // Tel quel, l'exercice ÉCHOUE : le programme rend 0, on attend 7.
        app.launch();
        for _ in 0..10 {
            app.step();
        }
        assert!(!app.checks.is_empty(), "les attentes doivent être vérifiées");
        assert!(
            !app.checks.iter().all(|c| c.passed()),
            "le programme de départ ne doit PAS déjà passer, sinon il n'y a rien à faire"
        );

        // L'élève applique la consigne.
        app.source = app.source.replace("xor rdi, rdi", "mov rdi, 7");
        app.launch();
        for _ in 0..10 {
            app.step();
        }
        assert!(
            app.checks.iter().all(|c| c.passed()),
            "après correction, tout doit passer : {:?}",
            app.checks
        );

        // Et la leçon peut être marquée terminée, ce qui se conserve.
        app.tutorial_progress.mark_done(lesson.id);
        let saved = app.tutorial_progress.to_string();
        let back = tutorial::Progress::parse(&saved);
        assert!(back.is_done("premier_programme"));
    }

    /// Chaque leçon intermédiaire doit ÉCHOUER telle quelle et PASSER une fois
    /// son TODO appliqué. Sans les deux moitiés, l'exercice est un leurre : un
    /// programme qui passe déjà n'apprend rien, un programme qu'aucune
    /// correction ne sauve décourage.
    ///
    /// La correction est ici celle que le commentaire du TODO dicte, mot pour
    /// mot : le test vérifie donc aussi que la consigne écrite est la bonne.
    #[test]
    fn every_intermediate_lesson_fails_then_passes() {
        use std::path::PathBuf;

        // (leçon, texte du TODO à remplacer, correction attendue)
        let fixes: &[(&str, &str, &str)] = &[
            ("fonctions", "; TODO : multiplier RAX par RDI", "imul rax, rdi"),
            ("system_v", "; TODO : « push rbx » ici", "push rbx"),
            ("system_v", "; TODO : … et « pop rbx »", "pop rbx"),
            ("syscalls", "; TODO : RDX doit porter", ""),
            ("syscalls", "xor rdx, rdx", "mov rdx, msg_len"),
            ("tas", "mov rdi, r12        ; TODO", "lea rdi, [r12 + 4096]"),
            ("tableaux", "; TODO : ajouter l'élément courant", "add rbx, [tab + rcx*8]"),
            ("structures", "; TODO : ajouter le champ y", "add rbx, [rsi + pt_y]"),
            ("chaines", "; TODO : s'arrêter quand AL vaut 0", "test al, al\n    jz .fin"),
        ];

        for lesson in tutorial::lessons_of(tutorial::Level::Intermediate) {
            let mut app = App::new();
            app.load_lesson(&lesson);
            let dir = PathBuf::from(format!("build/tuto-inter/{}", lesson.id));
            std::fs::create_dir_all(&dir).expect("dossier de travail");
            app.out_dir = dir.clone();
            app.src_path = dir.join(format!("{}.asm", lesson.id));

            assert!(app.has_exercise(), "{} : attentes non armées", lesson.id);

            // Tel quel : au moins une attente doit tomber.
            run_to_completion(&mut app);
            assert!(
                !app.checks.is_empty() && !app.checks.iter().all(|c| c.passed()),
                "{} : le programme de départ passe déjà, il n'y a rien à faire",
                lesson.id
            );

            // On applique la consigne, à la lettre.
            let mut applied = 0;
            for (id, todo, fix) in fixes {
                if *id != lesson.id {
                    continue;
                }
                let line = app
                    .source
                    .lines()
                    .find(|l| l.contains(todo))
                    .unwrap_or_else(|| panic!("{} : TODO « {todo} » introuvable", lesson.id))
                    .to_string();
                let indent = " ".repeat(line.len() - line.trim_start().len());
                app.source = app.source.replace(&line, &format!("{indent}{fix}"));
                applied += 1;
            }
            assert!(applied > 0, "{} : aucune correction connue", lesson.id);

            run_to_completion(&mut app);
            assert!(
                app.checks.iter().all(|c| c.passed()),
                "{} : la correction dictée par le TODO ne suffit pas : {:?}",
                lesson.id,
                app.checks
            );
        }
    }

    /// Assemble, lance, et avance jusqu'à la fin du programme. La borne évite
    /// qu'une boucle folle bloque la suite des tests.
    fn run_to_completion(app: &mut App) {
        app.launch();
        for _ in 0..400 {
            app.step();
        }
    }

    /// Le programme d'une leçon est écrit dans un dossier à part, pour ne pas
    /// écraser les exemples livrés.
    #[test]
    fn lesson_files_do_not_overwrite_the_examples() {
        let mut app = App::new();
        let lesson = tutorial::find("boucles").expect("leçon présente");
        app.load_lesson(&lesson);
        let path = app.src_path.to_string_lossy().to_string();
        assert!(path.contains("tutoriel"), "chemin attendu dans tutoriel/ : {path}");
        assert!(path.ends_with("boucles.asm"), "{path}");
        assert!(!path.contains("/examples/"), "ne doit pas viser les exemples : {path}");
    }
}
