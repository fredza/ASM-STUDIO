//! Panneau TUTORIEL : le parcours guidé, relié aux outils de l'IDE.
//!
//! Charger une leçon ne se contente pas d'afficher son texte : le programme de
//! départ entre dans l'éditeur, les panneaux dont la leçon parle s'ouvrent, et
//! ses attentes arment le panneau Exercice. L'élève lit et manipule au même
//! endroit.

use eframe::egui::{self, RichText};

use super::dock::Panel;
use super::{accent, App, flag_on, card};
use crate::i18n;
use crate::tutorial::{self, Level, Lesson};

impl App {
    /// Remet le parcours à zéro, comme au tout premier lancement : oublie les
    /// leçons terminées, rouvre le sommaire, rallume le panneau Tutoriel et fait
    /// réapparaître le bandeau d'accueil. Le tout est aussitôt persisté.
    pub(super) fn reset_tutorial(&mut self) {
        self.tutorial_progress = crate::tutorial::Progress::default();
        self.tutorial_current = None;
        // Repartir de zéro, c'est retrouver l'expérience du nouveau venu.
        self.welcome_dismissed = false;
        self.tutorial_enabled = true;
        self.focus_panel(super::dock::Panel::Exercise);
        self.status = i18n::tr3(
            self.lang,
            "Progression du tutoriel réinitialisée.",
            "Tutorial progress reset.",
            "Progreso del tutorial reiniciado.",
        )
        .to_string();
        self.save_settings();
    }

    /// Ouvre le tutoriel : l'allume s'il était éteint, l'amène au premier plan,
    /// et revient au sommaire si aucune leçon n'est en cours.
    ///
    /// C'est le seul chemin d'accès, appelé par le bandeau d'accueil, le menu
    /// Aide et la palette. Il en manquait un : une fois le bandeau écarté, plus
    /// rien dans l'interface ne menait au parcours — le panneau qui l'héberge
    /// s'appelait « Exercices », et le mot « tutoriel » n'apparaissait nulle part.
    pub(super) fn open_tutorial(&mut self) {
        self.tutorial_enabled = true;
        self.show_panel(super::dock::Panel::Exercise);
        self.focus_panel(super::dock::Panel::Exercise);
        self.status = i18n::tr3(
            self.lang,
            "Tutoriel ouvert.",
            "Tutorial opened.",
            "Tutorial abierto.",
        )
        .to_string();
        self.save_settings();
    }

    /// Revient au sommaire du parcours, en quittant la leçon ouverte.
    pub(super) fn show_tutorial_toc(&mut self) {
        self.tutorial_current = None;
        self.open_tutorial();
    }

    /// Fait réapparaître le bandeau d'accueil, écarté d'un clic un peu vite.
    pub(super) fn show_welcome_again(&mut self) {
        self.welcome_dismissed = false;
        // Le bandeau ne se montre qu'en mode Apprentissage : le redemander
        // depuis le mode complet ne doit pas rester sans effet visible.
        self.set_ui_mode(super::UiMode::Learning);
        self.save_settings();
    }

    /// Titre d'un exercice semé, lu dans sa directive `;@titre`.
    ///
    /// Le nom de fichier ne dit rien à l'élève ; le titre, si. On lit le fichier
    /// installé plutôt que la copie embarquée : c'est celui qu'on va ouvrir, et
    /// l'élève a pu le renommer dans son propre énoncé.
    pub(super) fn exercise_title(file_name: &str) -> Option<String> {
        let path = super::data_dir().join("examples").join(file_name);
        let src = std::fs::read_to_string(path).ok()?;
        crate::exercise::parse(&src).title
    }

    /// Ouvre un exercice du dossier des exemples, par son nom de fichier.
    pub(super) fn open_exercise_file(&mut self, file_name: &str) {
        let path = super::data_dir().join("examples").join(file_name);
        if !path.exists() {
            self.log(&format!(
                "{} {}",
                i18n::tr3(
                    self.lang,
                    "Exercice introuvable :",
                    "Exercise not found:",
                    "Ejercicio no encontrado:"
                ),
                path.display()
            ));
            return;
        }
        self.open_file(path);
    }

    /// Charge une leçon, après s'être assuré que le travail en cours ne part
    /// pas avec (voir [`super::unsaved`]) : le programme de départ d'une leçon
    /// écrase l'éditeur, et l'exercice à moitié écrit qui s'y trouvait avec.
    pub(super) fn load_lesson(&mut self, lesson: &Lesson) {
        self.guarded(super::unsaved::PendingAction::LoadLesson(lesson.id.to_string()));
    }

    /// Charge une leçon : programme de départ, panneaux, et mise au point.
    pub(super) fn load_lesson_now(&mut self, lesson: &Lesson) {
        self.tutorial_current = Some(lesson.id.to_string());
        // Une leçon du parcours Windows suppose la cible qui va avec : sinon
        // `nasm -f elf64` refuse son « extern ExitProcess », et l'élève lit une
        // erreur qui ne parle pas de ce qu'il est en train d'apprendre.
        if lesson.level.needs_pe() {
            self.pe_enabled = true;
        }
        self.set_target(lesson.target());

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
            // Le programme de départ n'est pas encore du travail : c'est ce que
            // l'élève va en faire qui le deviendra.
            self.mark_saved();
            self.dbg = None;
            self.disasm.clear();
            self.binary = None;
            // Arme les attentes portées par le programme lui-même.
            self.reload_exercise();
        }

        // Ouvre ce dont la leçon parle : elle met sous les yeux ce qu'elle explique.
        for key in &lesson.panels {
            if let Some(p) = Panel::from_key(key)
                && !self.panel_is_open(p) {
                    self.show_panel(p);
                }
        }
        self.save_settings();
    }

    /// Sommaire du parcours. L'aiguillage vers une leçon ouverte se fait dans
    /// `exercise_ui`, qui héberge désormais les deux.
    pub(super) fn tutorial_toc_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let hdr = self.c_header();

        let mut to_load: Option<Lesson> = None;

        ui.add_space(2.0);
        self.progress_bar_ui(ui, None);
        ui.add_space(6.0);
        // Reprendre où l'on s'est arrêté, sans avoir à retrouver la ligne dans
        // le sommaire : c'est le geste le plus fréquent à l'ouverture.
        if let Some(l) = self.tutorial_progress.next_lesson() {
            let (done, _) = self.tutorial_progress.overall(self.pe_enabled);
            let label = if done == 0 {
                tr("▶ Commencer le parcours", "▶ Start the path", "▶ Empezar el recorrido")
            } else {
                tr("▶ Reprendre", "▶ Resume", "▶ Continuar")
            };
            if ui
                .add(egui::Button::new(RichText::new(label).strong()).fill(super::action()))
                .on_hover_text(format!("« {} »", l.title.get(lang)))
                .clicked()
            {
                to_load = Some(l);
            }
            ui.add_space(6.0);
        }
        ui.label(
            RichText::new(tr(
                "Chaque leçon charge son programme, ouvre les panneaux qu'elle explique, et propose ses exercices auto-corrigés (✎).",
                "Each lesson loads its program, opens the panels it explains, and offers its self-checked exercises (✎).",
                "Cada lección carga su programa, abre los paneles que explica y propone sus ejercicios autocorregidos (✎).",
            ))
            .size(12.0)
            .weak(),
        );
        ui.add_space(6.0);

        egui::ScrollArea::vertical()
            .id_salt("tutorial_toc")
            // Le sommaire partage le panneau avec les attentes : il ne doit pas
            // occuper toute la hauteur.
            .max_height(260.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for level in Level::ALL {
                    // Le parcours Windows suit son réglage : décocher
                    // « assemblage Windows » retire aussi ses leçons, plutôt
                    // que de proposer des exercices qu'on ne peut pas faire.
                    if level.needs_pe() && !self.pe_enabled {
                        continue;
                    }
                    let (done, total) = self.tutorial_progress.tally(level);
                    let complete = total > 0 && done == total;
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(level.title(lang))
                                .strong()
                                .color(if complete { flag_on() } else { accent() }),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            super::badge(
                                ui,
                                &format!("{done}/{total}"),
                                if complete { flag_on() } else { hdr },
                            );
                        });
                    });
                    ui.add_space(2.0);

                    for lesson in tutorial::lessons_of(level) {
                        let done = self.tutorial_progress.is_done(lesson.id);
                        // Une leçon sans étape n'est pas encore écrite : on la
                        // montre pour que le parcours soit lisible, mais grisée.
                        let planned = lesson.steps.is_empty();
                        // Une leçon s'ouvre quand celle d'avant est validée.
                        // Elle reste visible : voir la suite du chemin fait
                        // partie de ce qui donne envie de le prendre.
                        let locked = !self.tutorial_progress.is_unlocked(lesson.id, self.pe_enabled);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(if done {
                                    "✔"
                                } else if locked {
                                    "🔒"
                                } else {
                                    "○"
                                })
                                .color(if done { flag_on() } else { self.c_bytes() }),
                            );
                            let mut txt = RichText::new(lesson.title.get(lang)).size(12.5);
                            if planned || locked {
                                txt = txt.weak();
                            }
                            let resp = ui.add(egui::Label::new(txt).sense(egui::Sense::click()));
                            if resp.hovered() && !locked {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if locked {
                                resp.on_hover_text(tr(
                                    "Validez la leçon précédente pour ouvrir celle-ci.",
                                    "Validate the previous lesson to open this one.",
                                    "Valide la lección anterior para abrir esta.",
                                ));
                            } else if resp.clicked() {
                                to_load = Some(lesson.clone());
                            }
                            if planned {
                                ui.label(
                                    RichText::new(tr("(à venir)", "(planned)", "(por venir)"))
                                        .small()
                                        .weak(),
                                );
                            }
                            // Les exercices d'application se comptent dès le
                            // sommaire : le parcours annonce ce qu'il y a à
                            // faire, pas seulement ce qu'il y a à lire.
                            let n = tutorial::practice_for(lesson.id).len();
                            if n > 0 {
                                ui.label(
                                    RichText::new(format!("· {n} ✎"))
                                        .small()
                                        .weak(),
                                )
                                .on_hover_text(tr(
                                    "Exercices auto-corrigés sur cette notion, dans la leçon.",
                                    "Self-checked exercises on this topic, inside the lesson.",
                                    "Ejercicios autocorregidos sobre este tema, dentro de la lección.",
                                ));
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

    /// Indicateur de progression du parcours : barre remplie, compte des leçons
    /// terminées, et rang de la leçon en cours quand il y en a une.
    ///
    /// Sans lui, le parcours n'avait pas de longueur visible : on savait qu'on
    /// lisait une leçon, pas s'il en restait deux ou vingt. La barre est le seul
    /// endroit qui répond à « où j'en suis ? » d'un coup d'œil.
    fn progress_bar_ui(&self, ui: &mut egui::Ui, current: Option<&Lesson>) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let (done, total) = self.tutorial_progress.overall(self.pe_enabled);
        if total == 0 {
            return;
        }
        let frac = done as f32 / total as f32;
        let rank = current.and_then(|l| crate::tutorial::position(l.id, self.pe_enabled));

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(tr("Progression", "Progress", "Progreso"))
                    .small()
                    .strong()
                    .color(self.c_header()),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Le rang de la leçon ouverte à droite : « leçon 7 / 29 » situe
                // là où le compte des leçons terminées ne dit rien.
                if let Some((i, n)) = rank {
                    ui.label(
                        RichText::new(match lang {
                            i18n::Lang::Fr => format!("leçon {} / {n}", i + 1),
                            i18n::Lang::En => format!("lesson {} / {n}", i + 1),
                            i18n::Lang::Es => format!("lección {} / {n}", i + 1),
                        })
                        .small()
                        .weak(),
                    );
                }
            });
        });
        ui.add(
            egui::ProgressBar::new(frac)
                .desired_height(10.0)
                .fill(if done == total { flag_on() } else { accent() })
                .text(
                    RichText::new(match lang {
                        i18n::Lang::Fr => format!("{done} / {total} terminées"),
                        i18n::Lang::En => format!("{done} / {total} done"),
                        i18n::Lang::Es => format!("{done} / {total} terminadas"),
                    })
                    .small(),
                ),
        )
        .on_hover_text(tr(
            "Leçons marquées terminées sur l'ensemble du parcours.",
            "Lessons marked as done over the whole path.",
            "Lecciones marcadas como terminadas en todo el recorrido.",
        ));
    }

    /// La leçon ouverte peut-elle être validée ?
    ///
    /// Une leçon qui porte un programme se valide en le faisant marcher : ses
    /// attentes doivent toutes passer. Une leçon qui n'en porte pas — un texte à
    /// lire — se valide en la lisant, il n'y a rien à vérifier.
    pub(super) fn lesson_is_solved(&self, lesson: &Lesson) -> bool {
        if !lesson.has_starter() {
            return true;
        }
        !self.checks.is_empty() && self.checks.iter().all(|c| c.passed())
    }

    /// Combien d'indices ont déjà été demandés pour cette leçon.
    ///
    /// Le compte est rangé avec l'identifiant de la leçon qui l'a demandé :
    /// interrogé pour une autre, il repart de zéro. C'est ce qui évite d'avoir
    /// à remettre un compteur à zéro dans chacun des chemins qui ouvrent une
    /// leçon — le sommaire, « Reprendre », les flèches, la validation.
    pub(super) fn hints_shown(&self, id: &str) -> usize {
        match &self.tutorial_hints {
            Some((lesson, n)) if lesson == id => *n,
            _ => 0,
        }
    }

    /// Délivre un indice de plus pour cette leçon.
    pub(super) fn reveal_hint(&mut self, id: &str) {
        let n = self.hints_shown(id) + 1;
        self.tutorial_hints = Some((id.to_string(), n));
    }

    /// Contenu d'une leçon ouverte.
    pub(super) fn lesson_ui(&mut self, ui: &mut egui::Ui, lesson: &Lesson) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let hdr = self.c_header();
        let mut back = false;
        let mut reload = false;
        let mut toggle_done = false;
        let mut go_to: Option<Lesson> = None;
        let mut open_exercise: Option<String> = None;
        let mut reveal = false;
        let done = self.tutorial_progress.is_done(lesson.id);
        let (prev, next) = crate::tutorial::neighbours(lesson.id, self.pe_enabled);
        // Le compte d'indices ne vaut que pour la leçon qui l'a demandé : ouvrir
        // une autre leçon repart de zéro, sans qu'aucun des chemins d'ouverture
        // n'ait à s'en souvenir.
        let shown = self.hints_shown(lesson.id);

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
        self.progress_bar_ui(ui, Some(lesson));
        ui.add_space(6.0);
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

                    // L'objectif dit ce qu'on saura faire ; « à quoi ça sert »
                    // dit pourquoi quelqu'un a eu besoin de l'inventer. Sans
                    // cette phrase, une leçon sur les flags est une liste de
                    // lettres à retenir plutôt qu'une réponse à une question.
                    if let Some(why) = &lesson.why {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(tr("À quoi ça sert", "What it is for", "Para qué sirve"))
                                .small()
                                .strong()
                                .color(hdr),
                        );
                        ui.add_space(2.0);
                        ui.add(egui::Label::new(RichText::new(why.get(lang)).size(12.5)).wrap());
                    }

                    // Le prérequis n'est pas écrit dans la leçon : il est lu sur
                    // le parcours. Une leçon déplacée emporte son prérequis avec
                    // elle, là où un texte saisi à la main aurait menti.
                    if let Some(p) = &prev {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format!(
                                "{} « {} »",
                                tr("Vient après :", "Comes after:", "Viene después de:"),
                                p.title.get(lang)
                            ))
                            .size(11.5)
                            .weak(),
                        )
                        .on_hover_text(tr(
                            "Cette leçon suppose la précédente acquise.",
                            "This lesson assumes the previous one is understood.",
                            "Esta lección supone adquirida la anterior.",
                        ));
                    }
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
                                .color(accent()),
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

                // Les indices. Le parcours ne s'ouvre qu'en validant : rester
                // bloqué n'est donc plus une gêne, c'est un cul-de-sac. Ils sont
                // délivrés un par un, du plus vague au plus précis, et le
                // dernier dicte la solution — mieux vaut une leçon finie avec
                // l'indice ultime qu'un élève qui referme l'IDE.
                if !lesson.hints.is_empty() {
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(tr("Coup de pouce", "A nudge", "Un empujón"))
                            .small()
                            .strong()
                            .color(hdr),
                    );
                    ui.add_space(2.0);
                    for (i, hint) in lesson.hints.iter().take(shown).enumerate() {
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new("💡").size(12.0));
                            ui.add(
                                egui::Label::new(
                                    RichText::new(hint.get(lang)).size(12.5).color(
                                        // Le dernier indice donne la solution :
                                        // il se distingue des autres, pour que
                                        // le choix de le lire reste un choix.
                                        if i + 1 == lesson.hints.len() {
                                            accent()
                                        } else {
                                            ui.visuals().text_color()
                                        },
                                    ),
                                )
                                .wrap(),
                            );
                        });
                        ui.add_space(4.0);
                    }
                    if shown < lesson.hints.len() {
                        let last = shown + 1 == lesson.hints.len();
                        let label = if shown == 0 {
                            tr("💡 Un indice ?", "💡 A hint?", "💡 ¿Una pista?")
                        } else if last {
                            tr("💡 Montrer la solution", "💡 Show the solution", "💡 Mostrar la solución")
                        } else {
                            tr("💡 Un autre indice", "💡 Another hint", "💡 Otra pista")
                        };
                        if ui
                            .button(RichText::new(label).size(12.0))
                            .on_hover_text(if last {
                                tr(
                                    "Le dernier indice dit quoi écrire. À n'ouvrir qu'après avoir cherché.",
                                    "The last hint says what to write. Open it only after trying.",
                                    "La última pista dice qué escribir. Ábrala solo tras intentarlo.",
                                )
                            } else {
                                tr(
                                    "Un indice de plus, un peu plus précis que le précédent.",
                                    "One more hint, a little more precise than the last.",
                                    "Una pista más, algo más precisa que la anterior.",
                                )
                            })
                            .clicked()
                        {
                            reveal = true;
                        }
                    }
                }

                // Ce qui reste quand le programme est refermé. Les étapes se
                // lisent dans le mouvement du pas à pas ; ceci se relit avant de
                // passer à la suite, et c'est la seule partie de la leçon qui
                // tienne sans l'IDE sous les yeux.
                if !lesson.takeaway.is_empty() {
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(tr("À retenir", "Takeaway", "Para recordar"))
                            .small()
                            .strong()
                            .color(hdr),
                    );
                    ui.add_space(2.0);
                    card(ui, |ui| {
                        for point in &lesson.takeaway {
                            ui.horizontal_top(|ui| {
                                ui.label(RichText::new("•").strong().color(accent()));
                                ui.add(
                                    egui::Label::new(RichText::new(point.get(lang)).size(12.5))
                                        .wrap(),
                                );
                            });
                            ui.add_space(3.0);
                        }
                    });
                }

                // Les exercices d'application de la leçon. C'est ce qui manquait
                // pour que le parcours et le dossier d'exercices soient la même
                // chose : une leçon finie mène quelque part.
                let practice = tutorial::practice_for(lesson.id);
                if !practice.is_empty() {
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(tr("S'entraîner", "Practice", "Practicar"))
                            .small()
                            .strong()
                            .color(hdr),
                    );
                    ui.label(
                        RichText::new(tr(
                            "Des exercices auto-corrigés sur cette notion. Chacun s'ouvre dans l'éditeur avec son énoncé et ses attentes.",
                            "Self-checked exercises on this topic. Each opens in the editor with its statement and expectations.",
                            "Ejercicios autocorregidos sobre este tema. Cada uno se abre en el editor con su enunciado y sus expectativas.",
                        ))
                        .size(11.5)
                        .weak(),
                    );
                    ui.add_space(4.0);
                    for file in practice {
                        // Le titre de l'exercice plutôt que son nom de fichier :
                        // « Le plus petit des deux » dit ce qu'il y a à faire,
                        // « ex_c6_1_plus_petit.asm » non.
                        let label = super::App::exercise_title(file)
                            .unwrap_or_else(|| (*file).to_string());
                        if ui
                            .button(RichText::new(format!("▸ {label}")).size(12.0))
                            .on_hover_text(*file)
                            .clicked()
                        {
                            open_exercise = Some((*file).to_string());
                        }
                    }
                }
            });

        // Les attentes de la leçon, juste sous ses étapes : l'élève voit d'un
        // seul coup d'œil ce qu'on lui demande et où il en est.
        if lesson.has_starter() {
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);
            self.checks_ui(ui);
        }

        ui.separator();
        ui.add_space(4.0);
        // La barre de fin de leçon : reculer, valider, avancer. Le parcours se
        // menait jusqu'ici au seul bouton « marquer comme terminée », et rien ne
        // disait comment passer à la suite sans repasser par le sommaire.
        let solved = self.lesson_is_solved(lesson);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    prev.is_some(),
                    egui::Button::new(RichText::new(tr("←", "←", "←")).small()),
                )
                .on_hover_text(match &prev {
                    Some(p) => format!(
                        "{} « {} »",
                        tr("Leçon précédente :", "Previous lesson:", "Lección anterior:"),
                        p.title.get(lang)
                    ),
                    None => tr(
                        "C'est la première leçon du parcours.",
                        "This is the first lesson of the path.",
                        "Es la primera lección del recorrido.",
                    )
                    .to_string(),
                })
                .clicked()
            {
                go_to = prev.clone();
            }

            // Valider n'est pas cocher une case : tant que le programme de la
            // leçon ne satisfait pas ses attentes, le bouton reste inerte et dit
            // pourquoi. C'est aussi lui qui ouvre la suite — « Suivante → » ne
            // mène nulle part tant que celle-ci n'est pas validée.
            let label = if done {
                tr("✔ Terminée — annuler", "✔ Done — undo", "✔ Terminada — deshacer")
            } else {
                tr("✔ Valider la leçon", "✔ Validate the lesson", "✔ Validar la lección")
            };
            let btn = egui::Button::new(RichText::new(label).strong());
            let btn = if solved && !done { btn.fill(super::action()) } else { btn };
            let resp = ui.add_enabled(done || solved, btn).on_hover_text(if done {
                tr(
                    "Retirer cette leçon des leçons terminées.",
                    "Remove this lesson from the completed ones.",
                    "Quitar esta lección de las terminadas.",
                )
            } else {
                tr(
                    "Marque la leçon terminée et ouvre la suivante.",
                    "Marks the lesson as done and opens the next one.",
                    "Marca la lección como terminada y abre la siguiente.",
                )
            });
            if resp.clicked() {
                toggle_done = true;
            }
            if !done && !solved {
                // Le bouton grisé n'explique rien à lui seul : dire à côté ce
                // qui manque vaut mieux qu'un survol que personne ne tente.
                ui.label(
                    RichText::new(tr(
                        "— lancez le programme (F5) : toutes les attentes doivent passer.",
                        "— run the program (F5): every expectation must pass.",
                        "— ejecute el programa (F5): todas las expectativas deben pasar.",
                    ))
                    .small()
                    .weak(),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // La suite s'ouvre en validant celle-ci, pas en la contournant :
                // les leçons se tiennent l'une l'autre, et sauter la
                // comparaison pour lire le saut conditionnel ne fait pas gagner
                // de temps, il fait lire une leçon qui ne veut plus rien dire.
                let next_open = next
                    .as_ref()
                    .is_some_and(|n| self.tutorial_progress.is_unlocked(n.id, self.pe_enabled));
                if ui
                    .add_enabled(
                        next_open,
                        egui::Button::new(
                            RichText::new(tr("Suivante →", "Next →", "Siguiente →")).small(),
                        ),
                    )
                    .on_hover_text(match &next {
                        Some(n) if next_open => format!(
                            "{} « {} »",
                            tr("Leçon suivante :", "Next lesson:", "Lección siguiente:"),
                            n.title.get(lang)
                        ),
                        Some(_) => tr(
                            "Validez cette leçon pour ouvrir la suivante.",
                            "Validate this lesson to open the next one.",
                            "Valide esta lección para abrir la siguiente.",
                        )
                        .to_string(),
                        None => tr(
                            "C'est la dernière leçon du parcours.",
                            "This is the last lesson of the path.",
                            "Es la última lección del recorrido.",
                        )
                        .to_string(),
                    })
                    .clicked()
                {
                    go_to = next.clone();
                }
            });
        });
        ui.add_space(2.0);

        if reveal {
            self.reveal_hint(lesson.id);
        }
        if let Some(file) = open_exercise {
            self.open_exercise_file(&file);
        }
        if reload {
            self.load_lesson(lesson);
        }
        if toggle_done {
            if done {
                self.tutorial_progress.mark_undone(lesson.id);
            } else {
                self.tutorial_progress.mark_done(lesson.id);
                // Enchaîner : la leçon suivante s'ouvre d'elle-même. Celle qui
                // SUIT dans le parcours, pas la première non terminée : après
                // avoir repris une leçon sautée, on veut continuer là où on est,
                // pas être renvoyé au trou qu'on vient de boucher.
                match next.clone().or_else(|| self.tutorial_progress.next_lesson()) {
                    Some(n) => go_to = Some(n),
                    None => self.tutorial_current = None,
                }
            }
            self.save_settings();
        }
        // Changer de leçon passe par le garde-fou du travail non enregistré,
        // comme le sommaire : le programme de la leçon d'arrivée écrase
        // l'éditeur.
        if let Some(l) = go_to {
            self.tutorial_current = Some(l.id.to_string());
            if l.has_starter() {
                self.load_lesson(&l);
            }
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

    /// « Réinitialiser » fait repartir de zéro comme au premier lancement :
    /// progression oubliée, sommaire rouvert, panneau Tutoriel et bandeau
    /// d'accueil de retour.
    #[test]
    fn resetting_the_tutorial_restores_the_newcomer_state() {
        let mut app = App::new();
        app.tutorial_progress.mark_done("registres");
        app.tutorial_progress.mark_done("boucles");
        app.tutorial_current = Some("boucles".to_string());
        app.welcome_dismissed = true;
        app.tutorial_enabled = false;

        app.reset_tutorial();

        assert!(!app.tutorial_progress.is_done("registres"), "leçon oubliée");
        assert_eq!(
            app.tutorial_progress.tally(tutorial::Level::Beginner).0,
            0,
            "aucune leçon débutant ne reste marquée terminée"
        );
        assert!(app.tutorial_current.is_none(), "retour au sommaire");
        assert!(!app.welcome_dismissed, "le bandeau d'accueil réapparaît");
        assert!(app.tutorial_enabled, "le tutoriel est rallumé");
        assert!(app.panel_is_open(Panel::Exercise), "et le panneau Exercices est ouvert");
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

    /// La progression se compte sur le parcours réellement proposé : allumer
    /// l'assemblage Windows ajoute ses leçons au total, l'éteindre les retire.
    /// Un dénominateur qui compterait des leçons invisibles serait un mensonge.
    #[test]
    fn the_progress_indicator_counts_the_path_actually_offered() {
        let mut app = App::new();
        app.pe_enabled = false;
        let (done, total) = app.tutorial_progress.overall(false);
        assert_eq!(done, 0, "rien de fait au départ");
        let (_, with_pe) = app.tutorial_progress.overall(true);
        assert!(with_pe > total, "le parcours Windows allonge le total");

        app.tutorial_progress.mark_done("registres");
        assert_eq!(app.tutorial_progress.overall(false).0, 1);

        // Et le rang d'une leçon suit le même parcours.
        let (rank, n) = tutorial::position("registres", false).expect("leçon du parcours");
        assert!(rank < n && n == total);
        assert!(
            tutorial::position("win_premier", false).is_none(),
            "une leçon Windows n'a pas de rang quand son parcours est éteint"
        );
        assert!(tutorial::position("win_premier", true).is_some());
    }

    /// « Précédente » et « Suivante » encadrent la leçon dans l'ordre du
    /// parcours, et s'arrêtent à ses deux bouts.
    #[test]
    fn the_neighbours_frame_the_lesson_and_stop_at_both_ends() {
        let path = tutorial::path(true);
        let first = &path[0];
        let last = path.last().expect("parcours non vide");

        let (prev, next) = tutorial::neighbours(first.id, true);
        assert!(prev.is_none(), "rien avant la première leçon");
        assert_eq!(next.expect("une suite").id, path[1].id);

        let (prev, next) = tutorial::neighbours(last.id, true);
        assert!(next.is_none(), "rien après la dernière");
        assert_eq!(prev.expect("une précédente").id, path[path.len() - 2].id);

        // Un identifiant qui n'est pas du parcours n'a pas de voisines.
        let (prev, next) = tutorial::neighbours("lecon_inexistante", true);
        assert!(prev.is_none() && next.is_none());
    }

    /// Le bouton « Valider » n'est pas une case à cocher : tant que le programme
    /// de la leçon ne satisfait pas ses attentes, il reste inerte — et il
    /// s'ouvre dès qu'elles passent toutes.
    #[test]
    fn a_lesson_can_only_be_validated_once_its_expectations_pass() {
        use std::path::PathBuf;
        let mut app = App::new();
        let lesson = tutorial::find("premier_programme").expect("leçon présente");
        app.load_lesson(&lesson);
        app.out_dir = PathBuf::from("build/tuto-valider");
        app.src_path = PathBuf::from("build/tuto-valider/premier.asm");

        assert!(
            !app.lesson_is_solved(&lesson),
            "sans exécution, aucune attente n'est vérifiée : rien à valider"
        );

        app.launch();
        for _ in 0..10 {
            app.step();
        }
        assert!(!app.lesson_is_solved(&lesson), "le programme de départ échoue");

        app.source = app.source.replace("xor rdi, rdi", "mov rdi, 7");
        app.launch();
        for _ in 0..10 {
            app.step();
        }
        assert!(app.lesson_is_solved(&lesson), "corrigée, la leçon se valide : {:?}", app.checks);

        // Une leçon sans programme — un texte à lire — se valide sans condition :
        // il n'y a rien à faire tourner.
        let planned = tutorial::find("shellcode").expect("leçon présente");
        if !planned.has_starter() {
            assert!(app.lesson_is_solved(&planned));
        }
    }

    /// Valider enchaîne sur la leçon SUIVANTE du parcours, pas sur la première
    /// non terminée : après avoir repris une leçon sautée, on continue où l'on
    /// est plutôt que d'être renvoyé au trou qu'on vient de boucher.
    #[test]
    fn validating_a_skipped_lesson_moves_forward_not_backward() {
        let path = tutorial::path(true);
        // On saute la première, on termine la deuxième : la suivante attendue
        // est la troisième, alors que `next_lesson()` renverrait la première.
        let mut app = App::new();
        app.tutorial_progress.mark_done(path[1].id);
        let (_, next) = tutorial::neighbours(path[1].id, true);
        assert_eq!(next.expect("une suite").id, path[2].id);
        assert_eq!(
            app.tutorial_progress.next_lesson().expect("une leçon").id,
            path[0].id,
            "la reprise, elle, renvoie bien au premier trou"
        );
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

    /// Le panneau Exercices se rend dans tous ses états — sommaire, leçon
    /// ouverte, leçon annoncée, identifiant périmé, et tutoriel désactivé.
    #[test]
    fn tutorial_panel_renders_in_both_states() {
        let mut app = App::new();
        app.tutorial_enabled = true;
        let ctx = egui::Context::default();

        app.tutorial_current = None;
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.exercise_ui(ui));
        });

        app.tutorial_current = Some("pile".into());
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.exercise_ui(ui));
        });

        // Une leçon annoncée mais non rédigée doit se rendre sans paniquer.
        app.tutorial_current = Some("shellcode".into());
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.exercise_ui(ui));
        });

        // Un identifiant disparu retombe sur le sommaire au lieu de planter.
        app.tutorial_current = Some("lecon_inexistante".into());
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.exercise_ui(ui));
        });
    }

    /// Bout en bout : activer le tutoriel, charger une leçon, la RÉSOUDRE, et
    /// voir le panneau Exercice la valider. C'est la promesse du module.
    #[test]
    fn a_lesson_can_be_solved_and_is_validated() {
        use std::path::PathBuf;
        let mut app = App::new();
        app.tutorial_enabled = true;
        assert!(app.panel_is_open(Panel::Exercise));

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

    /// Chaque leçon des niveaux écrits doit ÉCHOUER telle quelle et PASSER une
    /// fois son TODO appliqué. Sans les deux moitiés, l'exercice est un leurre :
    /// un programme qui passe déjà n'apprend rien, un programme qu'aucune
    /// correction ne sauve décourage.
    ///
    /// La correction est ici celle que le commentaire du TODO dicte, mot pour
    /// mot : le test vérifie donc aussi que la consigne écrite est la bonne.
    #[test]
    fn every_written_lesson_fails_then_passes() {
        use std::path::PathBuf;

        // (leçon, texte du TODO à remplacer, correction attendue)
        let fixes: &[(&str, &str, &str)] = &[
            // --- Débutant ---
            // Le niveau d'entrée mérite la même garantie que les autres : c'est
            // celui où un exercice qui se valide tout seul décourage le plus,
            // faute d'assez d'assembleur pour comprendre qu'il n'a rien fait.
            ("premier_programme", "xor rdi, rdi", "mov rdi, 7"),
            ("registres", "; TODO : atteindre 100 dans RBX", "add rbx, 50"),
            ("tailles", "; TODO : écrire 0xFF dans l'octet bas", "mov al, 0xFF"),
            ("memoire", "mov rbx, valeur     ; TODO", "mov rbx, [valeur]"),
            ("flags", "; TODO : « sete bl »", "sete bl"),
            ("pile", "; TODO : deux « pop » bien ordonnés", "pop rbx\n    pop rcx"),
            ("pile", ";        Dernier entré", ""),
            ("sauts", "; TODO : sauter à .rsi_gagne", "jg .rsi_gagne"),
            ("boucles", "; TODO : ajouter RCX à RBX", "add rbx, rcx"),
            // --- Intermédiaire ---
            ("mul_div", "; TODO : multiplier RAX par R8", "imul rax, r8"),
            ("fonctions", "; TODO : multiplier RAX par RDI", "imul rax, rdi"),
            ("system_v", "; TODO : « push rbx » ici", "push rbx"),
            ("system_v", "; TODO : … et « pop rbx »", "pop rbx"),
            ("syscalls", "; TODO : RDX doit porter", ""),
            ("syscalls", "xor rdx, rdx", "mov rdx, msg_len"),
            ("tas", "mov rdi, r12        ; TODO", "lea rdi, [r12 + 4096]"),
            ("tableaux", "; TODO : ajouter l'élément courant", "add rbx, [tab + rcx*8]"),
            ("structures", "; TODO : ajouter le champ y", "add rbx, [rsi + pt_y]"),
            ("chaines", "; TODO : s'arrêter quand AL vaut 0", "test al, al\n    jz .fin"),
            // --- Avancé ---
            ("elf", "; TODO : retrancher l'adresse de _start", "sub rbx, _start"),
            ("linking", "; TODO : retrancher __bss_start", "sub rbx, __bss_start"),
            ("plt_got", "call [rsi]                  ; TODO", "call [rsi + 8]"),
            ("relocations", "lea rcx, [rel _start]   ; TODO", "lea rcx, [rel valeur]"),
            ("simd", "; TODO : additionner les deux vecteurs", "paddd xmm0, xmm1"),
            ("optimisation", "mov rbx, rax            ; TODO", "lea rbx, [rax + rax*4]"),
            ("optimisation", "add rbx, 0              ; TODO", "add rbx, rbx"),
            // --- Expert ---
            ("reverse", "; TODO : déchiffrer l'octet", "xor al, cle"),
            (
                "desassemblage",
                "db 0x48, 0xc7, 0xc3, 0x00, 0x00, 0x00, 0x00",
                "db 0x48, 0xc7, 0xc3, 0x2a, 0x00, 0x00, 0x00",
            ),
            ("syscalls_avances", "; TODO : fds+4 est l'extrémité", "mov edi, [rel fds]"),
            ("shellcode", "; TODO : déposer \"Hi\" sur la pile", "mov rax, 0x6948\n    push rax"),
            ("exploitation", "mov [rbp + 0], rax", "mov [rbp + 8], rax"),
            ("performance", "add rax, 0", "shl rax, 3"),
        ];

        let written = tutorial::lessons_of(tutorial::Level::Beginner)
            .into_iter()
            .chain(tutorial::lessons_of(tutorial::Level::Intermediate))
            .chain(tutorial::lessons_of(tutorial::Level::Advanced))
            .chain(tutorial::lessons_of(tutorial::Level::Expert));

        for lesson in written {
            // « installation » n'a pas de programme : c'est le premier contact,
            // avant tout code, et il n'y a rien à faire échouer.
            if !lesson.has_starter() {
                continue;
            }
            let mut app = App::new();
            app.load_lesson(&lesson);
            let dir = PathBuf::from(format!("build/tuto-inter/{}", lesson.id));
            std::fs::create_dir_all(&dir).expect("dossier de travail");
            app.out_dir = dir.clone();
            app.src_path = dir.join(format!("{}.asm", lesson.id));

            assert!(app.has_exercise(), "{} : attentes non armées", lesson.id);

            // Tel quel : au moins une attente doit tomber. Qu'il se termine ou
            // non importe peu ici — un programme de départ a le droit d'être
            // cassé au point de boucler.
            let _ = run_to_completion(&mut app);
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

            let terminated = run_to_completion(&mut app);
            assert!(
                terminated,
                "{} : le programme corrigé ne se termine pas dans la borne — boucle sans fin ?",
                lesson.id
            );
            assert!(
                app.checks.iter().all(|c| c.passed()),
                "{} : la correction dictée par le TODO ne suffit pas : {:?}",
                lesson.id,
                app.checks
            );
        }
    }

    /// Une solution qui donne la BONNE valeur par un moyen PROSCRIT doit être
    /// recalée. C'est ce que ferment les contraintes de texte, là où l'attente
    /// de valeur seule laissait passer la triche.
    #[test]
    fn a_forbidden_form_is_rejected_even_when_the_value_is_right() {
        use std::path::PathBuf;

        // (leçon, motif remplacé, triche produisant la bonne valeur autrement)
        let cheats: &[(&str, &str, &str)] = &[
            // ×10 en une multiplication : RBX vaut 70, mais « imul » est interdit.
            ("optimisation", "mov rbx, rax            ; TODO", "imul rbx, rax, 10"),
            ("optimisation", "add rbx, 0              ; TODO", "nop"),
            // ×8 en multiplication plutôt que par décalage.
            ("performance", "add rax, 0", "imul rax, rax, 8"),
            // Adresse absolue : l'écart reste nul, mais « rel » est requis.
            ("relocations", "lea rcx, [rel _start]   ; TODO", "mov rcx, valeur"),
        ];

        for lesson in tutorial::lessons_of(tutorial::Level::Advanced)
            .into_iter()
            .chain(tutorial::lessons_of(tutorial::Level::Expert))
        {
            if !cheats.iter().any(|(id, ..)| *id == lesson.id) {
                continue;
            }
            let mut app = App::new();
            app.load_lesson(&lesson);
            let dir = PathBuf::from(format!("build/tuto-cheat/{}", lesson.id));
            std::fs::create_dir_all(&dir).expect("dossier de travail");
            app.out_dir = dir.clone();
            app.src_path = dir.join(format!("{}.asm", lesson.id));

            for (id, pat, cheat) in cheats {
                if *id != lesson.id {
                    continue;
                }
                let line = app
                    .source
                    .lines()
                    .find(|l| l.contains(pat))
                    .unwrap_or_else(|| panic!("{} : motif « {pat} » introuvable", lesson.id))
                    .to_string();
                let indent = " ".repeat(line.len() - line.trim_start().len());
                app.source = app.source.replace(&line, &format!("{indent}{cheat}"));
            }

            let terminated = run_to_completion(&mut app);
            assert!(terminated, "{} : la triche ne se termine pas", lesson.id);

            // La valeur, elle, est bonne : la triche « marche » — mais une
            // contrainte de texte doit malgré tout la recaler.
            let value_ok = app.checks.iter().all(|c| match &c.requirement {
                crate::exercise::Requirement::Value(_) => c.passed(),
                crate::exercise::Requirement::Text(_) => true,
            });
            assert!(value_ok, "{} : la triche devrait donner la bonne valeur : {:?}", lesson.id, app.checks);
            assert!(
                !app.checks.iter().all(|c| c.passed()),
                "{} : une forme proscrite a été acceptée",
                lesson.id
            );
        }
    }

    /// Assemble, lance, et avance jusqu'à la fin du programme — ou jusqu'à une
    /// borne franche. Rend `true` s'il s'est terminé (sortie, signal ou faute),
    /// `false` s'il tournait encore au bout de la borne, ce qui trahit une
    /// boucle sans fin dans le programme de la leçon.
    ///
    /// `can_step` devient faux dès que le programme n'est plus vivant : passé ce
    /// point, la boucle s'arrête au lieu de dérouler des pas vides.
    ///
    /// Réserve : un programme qui se BLOQUE dans un appel système — une lecture
    /// sur une entrée vide, par exemple — suspend le pas ptrace lui-même, et
    /// aucune borne côté test ne peut l'interrompre. Les leçons livrées n'en
    /// contiennent pas.
    #[must_use]
    fn run_to_completion(app: &mut App) -> bool {
        app.launch();
        for _ in 0..2000 {
            if !app.can_step() {
                return true; // plus rien à exécuter : terminé proprement
            }
            app.step();
        }
        !app.can_step()
    }

    /// Le tutoriel doit rester atteignable après que le bandeau d'accueil a été
    /// écarté — c'est le trou qu'on vient de boucher : le panneau s'appelait
    /// « Exercices », le mot « tutoriel » n'apparaissait nulle part, et écarter
    /// le bandeau fermait la seule porte.
    #[test]
    fn the_tutorial_is_reachable_after_dismissing_the_welcome_banner() {
        use crate::app::palette::Command;

        let mut app = App::new();
        // L'élève écarte le bandeau et éteint le tutoriel : le pire des cas.
        app.welcome_dismissed = true;
        app.tutorial_enabled = false;
        app.tutorial_current = Some("boucles".to_string());
        app.hide_panel(Panel::Exercise);

        app.run_command(Command::OpenTutorial);
        assert!(app.tutorial_enabled, "le tutoriel se rallume tout seul");
        assert!(app.panel_is_open(Panel::Exercise), "son panneau s'ouvre");
        assert_eq!(app.tutorial_current, None, "et on revient au sommaire");

        // Le bandeau, lui aussi, doit pouvoir revenir.
        app.run_command(Command::ShowWelcome);
        assert!(!app.welcome_dismissed, "le bandeau réapparaît");
        assert_eq!(app.mode, crate::app::UiMode::Learning, "et dans le mode où il s'affiche");

        // Enfin, le panneau annonce ce qu'il contient : sans le mot, personne ne
        // devine que le parcours est là.
        for lang in [i18n::Lang::Fr, i18n::Lang::En, i18n::Lang::Es] {
            let title = Panel::Exercise.title(lang).to_lowercase();
            assert!(title.contains("tutorial") || title.contains("tutoriel"), "titre : {title}");
        }
    }

    /// Le parcours Windows, de bout en bout : la leçon pose sa cible, son
    /// programme échoue tel quel, et passe une fois la consigne appliquée.
    ///
    /// La correction est celle que le TODO dicte, mot pour mot — le test
    /// vérifie donc aussi que la consigne écrite est la bonne. La vérification
    /// finale passe par le code de sortie, seul observable sans débogueur ;
    /// sans Wine, on s'arrête à ce qui reste vérifiable : l'assemblage et les
    /// contraintes de texte.
    #[test]
    fn every_windows_lesson_fails_then_passes() {
        use std::path::PathBuf;

        // (leçon, texte à remplacer, correction attendue)
        let fixes: &[(&str, &str, &str)] = &[
            ("win_premier", "mov     ecx, 0", "mov     ecx, 7"),
            ("win_appel", "mov     edi, 42", "mov     ecx, 42"),
            ("win_pile", "sub     rsp, 0", "sub rsp, 40"),
            ("win_imports", "lea     rcx, [autre]", "lea     rcx, [mot]"),
            ("win_format", "mov     rcx, [treize]", "mov     rcx, [douze]"),
        ];
        let wine = crate::winerun::available();

        for lesson in tutorial::lessons_of(tutorial::Level::Windows) {
            let mut app = App::new();
            app.load_lesson(&lesson);
            assert_eq!(
                app.target,
                crate::assemble::Target::Windows,
                "{} : la leçon doit poser sa cible",
                lesson.id
            );
            let dir = PathBuf::from(format!("build/tuto-win/{}", lesson.id));
            std::fs::create_dir_all(&dir).expect("dossier de travail");
            app.out_dir = dir.clone();
            app.src_path = dir.join(format!("{}.asm", lesson.id));
            assert!(app.has_exercise(), "{} : attentes non armées", lesson.id);

            // Tel quel : le programme s'assemble, mais ne satisfait pas l'énoncé.
            app.build();
            assert!(app.binary.is_some(), "{} : le départ doit s'assembler ({})", lesson.id, app.console);
            if wine {
                let failing = run_windows_to_completion(&mut app);
                assert!(
                    !failing.is_empty() && !failing.iter().all(|c| c.passed()),
                    "{} : le programme de départ passe déjà, il n'y a rien à faire",
                    lesson.id
                );
            }

            // L'élève applique la consigne.
            let mut fixed = false;
            for (id, from, to) in fixes {
                if *id == lesson.id && app.source.contains(from) {
                    app.source = app.source.replace(from, to);
                    fixed = true;
                }
            }
            assert!(fixed, "{} : aucune correction connue pour cette leçon", lesson.id);

            app.build();
            assert!(
                app.binary.is_some(),
                "{} : la correction doit s'assembler ({})",
                lesson.id,
                app.console
            );
            if wine {
                let checks = run_windows_to_completion(&mut app);
                assert!(
                    !checks.is_empty() && checks.iter().all(|c| c.passed()),
                    "{} : après correction, tout doit passer : {:?}",
                    lesson.id,
                    checks
                );
            }
        }
    }

    /// Lance le programme Windows courant et sonde jusqu'à sa fin, comme le
    /// ferait la boucle de frame. Rend les contrôles tels qu'ils ont été jugés.
    fn run_windows_to_completion(app: &mut App) -> Vec<crate::exercise::Check> {
        let ctx = egui::Context::default();
        app.launch();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while app.wine.is_some() {
            app.poll_wine(&ctx);
            assert!(std::time::Instant::now() < deadline, "le programme ne finit pas");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        app.checks.clone()
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
