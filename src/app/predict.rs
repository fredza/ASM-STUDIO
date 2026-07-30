//! Prédire avant de révéler : l'élève annonce le résultat, puis l'observe.
//!
//! Regarder un pas-à-pas est passif — on suit les couleurs sans rien anticiper.
//! Ici, avant chaque `Step`, l'élève écrit ce que vaudra un registre. L'app
//! compare et compte les points.
//!
//! L'architecture s'y prête : le record-and-replay a déjà calculé l'avenir.
//! Après le pas, `history[view_index]` contient la vérité — il n'y a donc rien
//! à simuler, seulement à comparer.

use eframe::egui::{self, RichText};

use super::{App, CHANGED, FALSE_COL, FLAG_ON, card, panel_header, parse_hex};
use crate::i18n;

/// Résultat d'une prédiction résolue.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Verdict {
    /// Valeur annoncée = valeur obtenue.
    Right,
    /// Valeur annoncée ≠ valeur obtenue.
    Wrong,
}

/// Une prédiction en attente ou résolue.
#[derive(Clone)]
pub(crate) struct Prediction {
    /// Registre visé, ex. « RAX ».
    pub(crate) reg: &'static str,
    /// Valeur annoncée par l'élève.
    pub(crate) expected: u64,
    /// Instruction sur laquelle porte la prédiction (pour l'affichage).
    pub(crate) insn: String,
    /// Étape à laquelle la prédiction a été posée.
    pub(crate) step: usize,
    /// `None` tant que le pas n'a pas été fait.
    pub(crate) got: Option<u64>,
}

impl Prediction {
    pub(crate) fn verdict(&self) -> Option<Verdict> {
        self.got.map(|g| if g == self.expected { Verdict::Right } else { Verdict::Wrong })
    }
}

/// Compteur de réussite, affiché en permanence.
#[derive(Clone, Copy, Default)]
pub(crate) struct Score {
    pub(crate) right: u32,
    pub(crate) total: u32,
}

impl Score {
    /// Pourcentage de réussite, ou `None` si aucune prédiction encore.
    pub(crate) fn percent(&self) -> Option<u32> {
        (self.total > 0).then(|| self.right * 100 / self.total)
    }
}

/// Registres proposés à la prédiction : les généraux, sans RIP ni EFLAGS
/// (prédire RIP est un exercice différent, et EFLAGS n'est pas lisible à la main).
pub(crate) const PREDICTABLE: [&str; 16] = [
    "RAX", "RBX", "RCX", "RDX", "RSI", "RDI", "RBP", "RSP",
    "R8", "R9", "R10", "R11", "R12", "R13", "R14", "R15",
];

impl App {
    /// Valeur courante du registre visé par la prédiction.
    fn reg_value(&self, name: &str) -> Option<u64> {
        let snap = self.snap()?;
        snap.regs.named().iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
    }

    /// Résout la prédiction en attente après un `Step`, et met le score à jour.
    /// Appelé depuis `step()` : à ce moment `snap()` reflète déjà l'après.
    pub(crate) fn resolve_prediction(&mut self) {
        let Some(p) = self.prediction.as_mut() else { return };
        if p.got.is_some() {
            return; // déjà résolue
        }
        let reg = p.reg;
        let Some(actual) = self.reg_value(reg) else { return };
        // Réemprunt : reg_value a relâché l'emprunt mutable.
        let Some(p) = self.prediction.as_mut() else { return };
        p.got = Some(actual);
        let right = p.expected == actual;
        self.pred_score.total += 1;
        if right {
            self.pred_score.right += 1;
        }
    }

    /// Panneau « Prédiction » : saisie avant le pas, verdict après.
    pub(crate) fn predict_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let hdr = self.c_header();

        panel_header(ui, |ui| {
            super::header_title(ui, hdr, None, tr("PRÉDICTION", "PREDICTION", "PREDICCIÓN"));
            // Score à droite de l'en-tête : visible en permanence.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(pct) = self.pred_score.percent() {
                    let col = match pct {
                        p if p >= 75 => FLAG_ON,
                        p if p >= 50 => CHANGED,
                        _ => FALSE_COL,
                    };
                    ui.label(
                        RichText::new(format!(
                            "{}/{}  ({pct} %)",
                            self.pred_score.right, self.pred_score.total
                        ))
                        .monospace()
                        .strong()
                        .color(col),
                    );
                }
            });
        });

        // Le corps défile : sans cela son contenu pousserait la bande CPU
        // à grandir quand la colonne PRÉDICTION est affichée.
        egui::ScrollArea::vertical()
            .id_salt("predict_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| self.predict_body(ui));
    }

    /// Corps du panneau (saisie ou verdict), rendu dans une zone défilante.
    fn predict_body(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let hdr = self.c_header();

        if !self.can_step() {
            ui.weak(tr(
                "Lance un programme et place-toi à la dernière étape pour prédire.",
                "Run a program and go to the last step to make a prediction.",
                "Ejecuta un programa y ve al último paso para predecir.",
            ));
            return;
        }

        // Instruction sur le point de s'exécuter : c'est elle qu'on prédit.
        let Some(rip) = self.view_rip() else { return };
        let next_insn = self
            .disasm
            .iter()
            .find(|i| i.address == rip)
            .map(|i| format!("{} {}", i.mnemonic, i.operands));

        // --- Prédiction résolue : on montre le verdict ---
        if let Some(p) = self.prediction.clone()
            && let Some(v) = p.verdict()
        {
            let (icon, col, msg) = match v {
                Verdict::Right => (
                    "✔",
                    FLAG_ON,
                    tr("Bien vu !", "Well spotted!", "¡Bien visto!"),
                ),
                Verdict::Wrong => (
                    "✘",
                    FALSE_COL,
                    tr("Pas tout à fait.", "Not quite.", "No exactamente."),
                ),
            };
            egui::Frame::default()
                .fill(col.linear_multiply(0.14))
                .stroke(egui::Stroke::new(1.0_f32, col))
                .rounding(egui::Rounding::same(5.0))
                .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icon).size(16.0).strong().color(col));
                        ui.label(RichText::new(msg).strong().color(col));
                    });
                    ui.add_space(3.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!(
                                "{} {} · {} · {}",
                                tr("étape", "step", "paso"),
                                p.step,
                                p.insn,
                                p.reg
                            ))
                            .monospace()
                            .small()
                            .weak(),
                        )
                        .wrap(),
                    );
                    egui::Grid::new("pred_verdict").num_columns(2).spacing([10.0, 2.0]).show(ui, |ui| {
                        ui.label(RichText::new(tr("Tu as dit", "You said", "Dijiste")).small().color(hdr));
                        ui.label(RichText::new(format!("0x{:X}", p.expected)).monospace());
                        ui.end_row();
                        ui.label(RichText::new(tr("Réalité", "Actual", "Real")).small().color(hdr));
                        ui.label(
                            RichText::new(format!("0x{:X}", p.got.unwrap_or(0)))
                                .monospace()
                                .strong()
                                .color(col),
                        );
                        ui.end_row();
                        // L'écart aide à comprendre l'erreur (souvent un facteur 8,
                        // une confusion signé/non signé, ou un décalage de 1).
                        if v == Verdict::Wrong {
                            let got = p.got.unwrap_or(0);
                            let delta = got.wrapping_sub(p.expected) as i64;
                            ui.label(RichText::new(tr("Écart", "Off by", "Diferencia")).small().color(hdr));
                            ui.label(RichText::new(format!("{delta:+}")).monospace().color(CHANGED));
                            ui.end_row();
                        }
                    });
                });
            ui.add_space(6.0);
            if ui.button(tr("Prédire le pas suivant", "Predict next step", "Predecir el siguiente paso")).clicked() {
                self.prediction = None;
                self.pred_input.clear();
            }
            return;
        }

        // --- Prédiction en attente de saisie ---
        card(ui, |ui| {
            ui.label(
                RichText::new(tr(
                    "Avant d'exécuter cette instruction, que vaudra le registre ?",
                    "Before running this instruction, what will the register hold?",
                    "Antes de ejecutar esta instrucción, ¿qué valdrá el registro?",
                ))
                .size(12.5),
            );
            if let Some(ins) = &next_insn {
                ui.add_space(3.0);
                ui.label(RichText::new(ins).monospace().strong().color(self.c_mnemonic()));
            }
        });
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("pred_reg")
                .selected_text(RichText::new(self.pred_reg).monospace())
                .width(76.0)
                .show_ui(ui, |ui| {
                    for r in PREDICTABLE {
                        ui.selectable_value(&mut self.pred_reg, r, RichText::new(r).monospace());
                    }
                });
            ui.label("=");
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.pred_input)
                    .desired_width(120.0)
                    .font(egui::TextStyle::Monospace)
                    .hint_text(tr("hexa", "hex", "hexa")),
            );
            let submit = ui.button(tr("Valider", "Submit", "Validar")).clicked()
                || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if submit {
                match parse_hex(&self.pred_input) {
                    Some(v) => {
                        self.prediction = Some(Prediction {
                            reg: self.pred_reg,
                            expected: v,
                            insn: next_insn.clone().unwrap_or_default(),
                            step: self.view_index,
                            got: None,
                        });
                        // Le pas exécute l'instruction et résout la prédiction.
                        self.step();
                    }
                    None => {
                        self.status = tr(
                            "Valeur hexa invalide (ex. 3C ou 0x3C)",
                            "Invalid hex value (e.g. 3C or 0x3C)",
                            "Valor hexadecimal inválido (ej. 3C o 0x3C)",
                        )
                        .to_string()
                    }
                }
            }
        });
        // Valeur actuelle du registre choisi : point de départ du raisonnement.
        if let Some(cur) = self.reg_value(self.pred_reg) {
            ui.add_space(2.0);
            ui.label(
                RichText::new(format!(
                    "{} {} = 0x{cur:X}",
                    tr("actuellement", "currently", "actualmente"),
                    self.pred_reg
                ))
                .monospace()
                .small()
                .color(self.c_bytes()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_percentage() {
        let mut s = Score::default();
        assert_eq!(s.percent(), None, "aucune prédiction ⇒ pas de score");
        s.total = 4;
        s.right = 3;
        assert_eq!(s.percent(), Some(75));
        s.right = 0;
        assert_eq!(s.percent(), Some(0));
    }

    #[test]
    fn verdict_compares_exactly() {
        let mut p = Prediction {
            reg: "RAX",
            expected: 0x3C,
            insn: "mov rax, 60".into(),
            step: 0,
            got: None,
        };
        assert_eq!(p.verdict(), None, "non résolue tant que got est None");
        p.got = Some(0x3C);
        assert_eq!(p.verdict(), Some(Verdict::Right));
        p.got = Some(0x3D);
        assert_eq!(p.verdict(), Some(Verdict::Wrong), "un bit d'écart suffit");
    }

    /// La liste proposée ne doit pas contenir RIP ni EFLAGS.
    #[test]
    fn predictable_excludes_rip_and_eflags() {
        assert!(!PREDICTABLE.contains(&"RIP"));
        assert!(!PREDICTABLE.contains(&"EFLAGS"));
        assert_eq!(PREDICTABLE.len(), 16, "les 16 registres généraux");
    }

    /// Bout en bout : une prédiction juste et une fausse sur un vrai programme.
    /// Vérifie que la résolution lit bien l'état d'APRÈS le pas.
    #[test]
    fn prediction_resolves_against_real_execution() {
        use std::path::PathBuf;
        let mut app = App::new();
        app.src_path = PathBuf::from("build/pred-test.asm");
        app.out_dir = PathBuf::from("build/pred");
        app.source = "section .text\n global _start\n_start:\n mov rax, 60\n \
                       mov rbx, 8\n mov rax,60\n xor rdi,rdi\n syscall\n"
            .to_string();
        app.pedagogy_predict = true;
        app.launch();
        assert!(app.dbg.is_some(), "programme lancé");

        // « mov rax, 60 » : RAX vaudra 0x3C. Prédiction juste.
        app.prediction = Some(Prediction {
            reg: "RAX",
            expected: 0x3C,
            insn: "mov rax, 60".into(),
            step: app.view_index,
            got: None,
        });
        app.step();
        let p = app.prediction.as_ref().expect("prédiction conservée");
        assert_eq!(p.got, Some(0x3C), "doit lire l'état d'après le pas");
        assert_eq!(p.verdict(), Some(Verdict::Right));
        assert_eq!(app.pred_score.right, 1);
        assert_eq!(app.pred_score.total, 1);

        // « mov rbx, 8 » : on annonce n'importe quoi. Prédiction fausse.
        app.prediction = Some(Prediction {
            reg: "RBX",
            expected: 0xDEAD,
            insn: "mov rbx, 8".into(),
            step: app.view_index,
            got: None,
        });
        app.step();
        let p = app.prediction.as_ref().unwrap();
        assert_eq!(p.got, Some(8), "RBX vaut 8");
        assert_eq!(p.verdict(), Some(Verdict::Wrong));
        assert_eq!(app.pred_score.right, 1, "le juste ne bouge pas");
        assert_eq!(app.pred_score.total, 2, "le total augmente");
        assert_eq!(app.pred_score.percent(), Some(50));

        // Le panneau se rend sans paniquer, verdict affiché.
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.predict_ui(ui));
        });
    }

    /// Une prédiction déjà résolue ne doit pas être recomptée à chaque pas.
    #[test]
    fn resolution_is_idempotent() {
        let mut app = App::new();
        app.prediction = Some(Prediction {
            reg: "RAX", expected: 1, insn: String::new(), step: 0, got: Some(2),
        });
        app.pred_score = Score { right: 0, total: 1 };
        app.resolve_prediction();
        app.resolve_prediction();
        assert_eq!(app.pred_score.total, 1, "pas de double comptage");
    }
}
