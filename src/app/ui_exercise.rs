//! Panneau EXERCICE : énoncé et attentes vérifiées.
//!
//! Onglet ancrable comme les autres. Il s'ouvre tout seul quand le fichier
//! chargé déclare des attentes (`;@attendu`), et explique comment en écrire
//! quand ce n'est pas le cas.

use eframe::egui::{self, RichText};

use super::{App, ACTION, FALSE_COL, FLAG_ON, card};
use crate::exercise;
use crate::i18n;

impl App {
    /// Vrai si le fichier courant est un exercice — conditionne l'affichage.
    pub(super) fn has_exercise(&self) -> bool {
        self.exercise.is_exercise()
    }

    pub(super) fn exercise_ui(&mut self, ui: &mut egui::Ui) {
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

        let (done, total) = exercise::tally(&self.checks);
        let verified = !self.checks.is_empty();
        let all_ok = verified && done == total;

        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if verified {
                    let col = if all_ok { FLAG_ON } else { FALSE_COL };
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

                // Bandeau de réussite, quand tout passe : la récompense.
                if all_ok {
                    egui::Frame::default()
                        .fill(FLAG_ON.linear_multiply(0.16))
                        .stroke(egui::Stroke::new(1.0_f32, FLAG_ON))
                        .rounding(egui::Rounding::same(5.0))
                        .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(tr("✔ Exercice réussi !", "✔ Exercise passed!", "✔ ¡Ejercicio superado!"))
                                    .strong()
                                    .color(FLAG_ON),
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
                        let (icon, col) = if ok { ("✔", FLAG_ON) } else { ("✘", FALSE_COL) };
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
                                        ui.label(RichText::new(got).monospace().small().color(FALSE_COL));
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
                            .color(ACTION),
                    );
                    for e in &self.exercise.errors {
                        ui.label(RichText::new(e).small().monospace().color(ACTION));
                    }
                }
            });
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
