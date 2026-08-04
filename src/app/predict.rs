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

use super::{ACCENT, App, CHANGED, FALSE_COL, FLAG_ON, card, parse_hex};
use crate::i18n::{self, Lang};

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
    /// Texte brut saisi : nécessaire pour repérer une confusion décimal/hexa,
    /// que la valeur analysée seule ne permettrait plus de distinguer.
    pub(crate) input: String,
    /// Valeur du registre AVANT le pas, pour expliquer la transition.
    pub(crate) before: u64,
    /// `None` tant que le pas n'a pas été fait.
    pub(crate) got: Option<u64>,
}

impl Prediction {
    pub(crate) fn verdict(&self) -> Option<Verdict> {
        self.got.map(|g| if g == self.expected { Verdict::Right } else { Verdict::Wrong })
    }
}

/// Erreur reconnue dans une prédiction fausse.
///
/// Une prédiction ratée est le moment le plus instructif de l'exercice : encore
/// faut-il dire à l'élève *en quoi* il s'est trompé. Un écart chiffré ne
/// l'apprend pas ; un motif nommé, si.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Mistake {
    /// Le champ attend de l'hexadécimal, la valeur a été écrite en décimal.
    DecimalWrittenAsHex { typed: String, hex: u64 },
    /// C'est la valeur d'AVANT l'instruction qui a été annoncée.
    GaveValueBefore,
    /// Le registre n'a pas bougé, contrairement à ce qui était annoncé.
    RegisterDidNotChange,
    /// Complément à deux : l'opposé a été annoncé (ou lu comme non signé).
    TwosComplement { signed: i64 },
    /// Écriture 32 bits : les 32 bits hauts ont été remis à zéro.
    ZeroExtended32,
    /// Facteur 8 : confusion entre un nombre d'octets et un nombre de mots.
    WordSizeFactor,
    /// Écart de un — souvent une borne de boucle.
    OffByOne { diff: i64 },
    /// La valeur annoncée existe, mais dans un autre registre.
    ValueIsInRegister(&'static str),
    /// Aucun motif reconnu : on explique au moins la transition.
    Unrecognised,
}

impl Mistake {
    /// Analyse une prédiction fausse.
    ///
    /// `others` = (nom, valeur) des autres registres APRÈS le pas, pour repérer
    /// une valeur annoncée qui a atterri ailleurs. L'ordre des tests va du motif
    /// le plus spécifique au plus général.
    pub(crate) fn detect(
        typed: &str,
        expected: u64,
        got: u64,
        before: u64,
        others: &[(&'static str, u64)],
    ) -> Mistake {
        // Décimal saisi dans un champ hexa : « 60 » vaut 0x60 = 96, alors que
        // l'élève pensait au 60 décimal, soit 0x3C. Piège le plus fréquent.
        if let Ok(as_decimal) = typed.trim().trim_start_matches("0x").parse::<u64>()
            && !typed.trim().starts_with("0x")
            && as_decimal == got
            && expected != got
        {
            return Mistake::DecimalWrittenAsHex { typed: typed.trim().to_string(), hex: got };
        }
        if expected == before && got != before {
            return Mistake::GaveValueBefore;
        }
        if got == before {
            return Mistake::RegisterDidNotChange;
        }
        if expected != 0 && got == expected.wrapping_neg() {
            return Mistake::TwosComplement { signed: got as i64 };
        }
        if expected > u32::MAX as u64 && got == expected & 0xFFFF_FFFF {
            return Mistake::ZeroExtended32;
        }
        if expected != 0 && (got == expected.wrapping_mul(8) || expected == got.wrapping_mul(8)) {
            return Mistake::WordSizeFactor;
        }
        let diff = got.wrapping_sub(expected) as i64;
        if diff == 1 || diff == -1 {
            return Mistake::OffByOne { diff };
        }
        if let Some((name, _)) = others.iter().find(|(_, v)| *v == expected) {
            return Mistake::ValueIsInRegister(name);
        }
        Mistake::Unrecognised
    }

    /// Titre court de l'erreur.
    pub(crate) fn title(&self, lang: Lang) -> String {
        let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        match self {
            Mistake::DecimalWrittenAsHex { .. } => {
                t("Décimal écrit dans un champ hexadécimal", "Decimal typed into a hex field", "Decimal escrito en un campo hexadecimal").into()
            }
            Mistake::GaveValueBefore => t("C'est la valeur d'avant", "That was the value before", "Ese era el valor anterior").into(),
            Mistake::RegisterDidNotChange => t("Ce registre n'a pas bougé", "This register did not change", "Este registro no cambió").into(),
            Mistake::TwosComplement { .. } => t("Question de signe", "A matter of sign", "Cuestión de signo").into(),
            Mistake::ZeroExtended32 => t("Écriture 32 bits", "32-bit write", "Escritura de 32 bits").into(),
            Mistake::WordSizeFactor => t("Octets ou mots de 8 ?", "Bytes or 8-byte words?", "¿Bytes o palabras de 8?").into(),
            Mistake::OffByOne { .. } => t("À une unité près", "Off by one", "Por una unidad").into(),
            Mistake::ValueIsInRegister(_) => t("Bonne valeur, mauvais registre", "Right value, wrong register", "Valor correcto, registro equivocado").into(),
            Mistake::Unrecognised => t("Voyons ce qui s'est passé", "Let's see what happened", "Veamos qué pasó").into(),
        }
    }

    /// Explication détaillée, avec les valeurs en jeu.
    pub(crate) fn explanation(&self, lang: Lang, reg: &str, expected: u64, got: u64, before: u64) -> String {
        let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        match self {
            Mistake::DecimalWrittenAsHex { typed, hex } => format!(
                "{} {typed} {} 0x{typed} = {} {}. {} {hex} {} 0x{hex:X}.",
                t("Tu as tapé", "You typed", "Escribiste"),
                t(", que le champ a lu comme", ", which the field read as", ", que el campo leyó como"),
                expected,
                t("en décimal", "in decimal", "en decimal"),
                t("La bonne réponse était bien", "The right answer was indeed", "La respuesta correcta era"),
                t("en décimal, mais il s'écrit", "in decimal, but it is written", "en decimal, pero se escribe"),
            ),
            Mistake::GaveValueBefore => format!(
                "{reg} {} 0x{before:X} {} 0x{got:X}. {}",
                t("valait", "held", "valía"),
                t("avant l'instruction, et vaut maintenant", "before the instruction, and now holds", "antes de la instrucción, y ahora vale"),
                t(
                    "Tu as annoncé l'état d'avant : l'instruction a bien écrit dans ce registre.",
                    "You gave the previous state: the instruction did write to this register.",
                    "Diste el estado anterior: la instrucción sí escribió en este registro.",
                ),
            ),
            Mistake::RegisterDidNotChange => format!(
                "{reg} {} 0x{before:X}. {}",
                t("est resté à", "stayed at", "se quedó en"),
                t(
                    "Cette instruction ne le touche pas — regarde quel registre elle prend pour destination.",
                    "This instruction does not touch it — look at which register it writes to.",
                    "Esta instrucción no lo toca — mira en qué registro escribe.",
                ),
            ),
            Mistake::TwosComplement { signed } => format!(
                "0x{got:X} {} {signed} {}. {}",
                t("vaut", "is", "vale"),
                t("en complément à deux", "in two's complement", "en complemento a dos"),
                t(
                    "Un nombre négatif est stocké comme son opposé binaire : tous les bits hauts sont à 1.",
                    "A negative number is stored as its binary opposite: all the high bits are 1.",
                    "Un número negativo se guarda como su opuesto binario: todos los bits altos están a 1.",
                ),
            ),
            Mistake::ZeroExtended32 => format!(
                "{} 0x{expected:X}, {} 0x{got:X}. {}",
                t("Tu attendais", "You expected", "Esperabas"),
                t("le registre contient", "the register holds", "el registro contiene"),
                t(
                    "Écrire dans la moitié 32 bits (eax, ebx…) remet automatiquement à zéro les 32 bits hauts du registre 64 bits.",
                    "Writing to the 32-bit half (eax, ebx…) automatically zeroes the upper 32 bits of the 64-bit register.",
                    "Escribir en la mitad de 32 bits (eax, ebx…) pone automáticamente a cero los 32 bits altos.",
                ),
            ),
            Mistake::WordSizeFactor => format!(
                "{} 8 {} 0x{expected:X} {} 0x{got:X}. {}",
                t("Il y a un facteur", "There is a factor of", "Hay un factor"),
                t("entre", "between", "entre"),
                t("et", "and", "y"),
                t(
                    "Sur la pile, une case fait 8 octets : compter les cases et compter les octets ne donne pas le même nombre.",
                    "On the stack a slot is 8 bytes: counting slots and counting bytes do not give the same number.",
                    "En la pila una casilla ocupa 8 bytes: contar casillas y contar bytes no da lo mismo.",
                ),
            ),
            Mistake::OffByOne { diff } => format!(
                "{} {}. {}",
                t("Il manque exactement", "You are off by exactly", "Falta exactamente"),
                diff,
                t(
                    "Vérifie l'ordre des opérations : le décrément a-t-il lieu avant ou après la lecture ?",
                    "Check the order of operations: does the decrement happen before or after the read?",
                    "Comprueba el orden: ¿el decremento ocurre antes o después de la lectura?",
                ),
            ),
            Mistake::ValueIsInRegister(other) => format!(
                "0x{expected:X} {} {other}, {} {reg} {} 0x{got:X}. {}",
                t("se trouve bien dans", "is indeed in", "está en"),
                t("mais", "but", "pero"),
                t("contient", "holds", "contiene"),
                t(
                    "Relis la destination de l'instruction : c'est l'opérande de gauche.",
                    "Re-read the instruction's destination: it is the left-hand operand.",
                    "Relee el destino de la instrucción: es el operando de la izquierda.",
                ),
            ),
            Mistake::Unrecognised => format!(
                "{reg} : 0x{before:X} → 0x{got:X}. {}",
                t(
                    "Compare avec ce que fait l'instruction, décrit juste au-dessus.",
                    "Compare with what the instruction does, described just above.",
                    "Compara con lo que hace la instrucción, descrito arriba.",
                ),
            ),
        }
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
    /// Fenêtre flottante « Prédiction ».
    ///
    /// Fenêtre plutôt que colonne de la bande CPU : le panneau a besoin de
    /// hauteur (énoncé, saisie, verdict) et l'ajouter à la bande écrasait les
    /// autres colonnes. Flottant, il se place où l'élève veut, se déplace
    /// pendant qu'il lit son code, et ne dispute sa largeur à personne.
    pub(crate) fn predict_window(&mut self, ctx: &egui::Context) {
        if !self.pedagogy_predict {
            return;
        }
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);

        // Score dans le titre : reste visible même fenêtre repliée.
        let title = match self.pred_score.percent() {
            Some(pct) => format!(
                "🎯 {} — {}/{} ({pct} %)",
                tr("Prédiction", "Prediction", "Predicción"),
                self.pred_score.right,
                self.pred_score.total
            ),
            None => format!("🎯 {}", tr("Prédiction", "Prediction", "Predicción")),
        };

        let mut open = true;
        egui::Window::new(title)
            // Id explicite : le titre contient le score et change donc en cours
            // de partie. Sans cela, egui perdrait la position à chaque point.
            .id(egui::Id::new("predict_window"))
            .open(&mut open)
            .collapsible(true)
            .resizable(true)
            .default_width(320.0)
            .default_height(260.0)
            .default_pos(ctx.content_rect().center() + egui::vec2(180.0, -60.0))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("predict_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.predict_body(ui));
            });

        // Fermer la fenêtre décoche l'entrée du menu Affichage : un seul état.
        if !open {
            self.pedagogy_predict = false;
            self.save_settings();
        }
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
                .fill(col.linear_multiply(0.06))
                .stroke(egui::Stroke::new(1.2_f32, col))
                .corner_radius(egui::CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(8, 6))
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
                        ui.label(
                            RichText::new(format!("0x{:X}", p.expected))
                                .monospace()
                                .color(ui.visuals().strong_text_color()),
                        );
                        ui.end_row();
                        ui.label(RichText::new(tr("Réalité", "Actual", "Real")).small().color(hdr));
                        ui.label(
                            RichText::new(format!("0x{:X}", p.got.unwrap_or(0)))
                                .monospace()
                                .strong()
                                .color(col),
                        );
                        ui.end_row();
                        if v == Verdict::Wrong {
                            let got = p.got.unwrap_or(0);
                            let delta = got.wrapping_sub(p.expected) as i64;
                            ui.label(RichText::new(tr("Écart", "Off by", "Diferencia")).small().color(hdr));
                            ui.label(RichText::new(format!("{delta:+}")).monospace().color(CHANGED));
                            ui.end_row();
                        }
                    });
                });

            // --- Pourquoi c'est faux ---
            // Un écart chiffré n'apprend rien. On nomme l'erreur, on rappelle ce
            // que fait l'instruction, et on donne quoi regarder.
            if v == Verdict::Wrong {
                let got = p.got.unwrap_or(0);
                let others: Vec<(&'static str, u64)> = self
                    .snap()
                    .map(|s| {
                        s.regs
                            .named()
                            .iter()
                            .filter(|(n, _)| *n != p.reg)
                            .map(|(n, v)| (*n, *v))
                            .collect()
                    })
                    .unwrap_or_default();
                let mistake = Mistake::detect(&p.input, p.expected, got, p.before, &others);

                ui.add_space(6.0);
                egui::Frame::default()
                    // Fond très discret : c'est le TEXTE qui doit ressortir, pas
                    // le cadre. À 0.10 d'accent, le texte par défaut passait
                    // sous le seuil de lisibilité.
                    .fill(ACCENT.linear_multiply(0.045))
                    .stroke(egui::Stroke::new(1.2_f32, ACCENT.linear_multiply(0.7)))
                    .corner_radius(egui::CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new(format!("💡 {}", mistake.title(lang)))
                                .strong()
                                .color(ACCENT),
                        );
                        ui.add_space(3.0);
                        ui.label(
                            RichText::new(mistake.explanation(lang, p.reg, p.expected, got, p.before))
                                .size(12.5)
                                .color(ui.visuals().strong_text_color()),
                        );
                    });

                // Ce que fait réellement l'instruction, dans les mots du panneau
                // INSTRUCTION : l'élève n'a pas à changer de panneau pour l'avoir.
                if let Some(insn) = self.disasm.iter().find(|i| {
                    format!("{} {}", i.mnemonic, i.operands) == p.insn
                }) {
                    let flags = self
                        .snap()
                        .map(|s| crate::debugger::Flags::from_eflags(s.regs.eflags))
                        .unwrap_or_default();
                    let e = crate::explain::explain(&insn.mnemonic, &insn.operands, flags, lang);
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!("{} {}", tr("Rappel —", "Reminder —", "Recordatorio —"), e.title))
                            .small()
                            .strong()
                            .color(hdr),
                    );
                    ui.label(
                        RichText::new(&e.description)
                            .size(12.0)
                            .color(ui.visuals().text_color()),
                    );
                }
            }
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
                    .id(egui::Id::new("kb_pred_input"))
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
                            input: self.pred_input.clone(),
                            before: self.reg_value(self.pred_reg).unwrap_or(0),
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
            input: "3C".into(),
            before: 0,
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
            input: "3C".into(),
            before: app.reg_value("RAX").unwrap_or(0),
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
            input: "DEAD".into(),
            before: app.reg_value("RBX").unwrap_or(0),
            got: None,
        });
        app.step();
        let p = app.prediction.as_ref().unwrap();
        assert_eq!(p.got, Some(8), "RBX vaut 8");
        assert_eq!(p.verdict(), Some(Verdict::Wrong));
        assert_eq!(app.pred_score.right, 1, "le juste ne bouge pas");
        assert_eq!(app.pred_score.total, 2, "le total augmente");
        assert_eq!(app.pred_score.percent(), Some(50));

        // La fenêtre flottante se rend sans paniquer, verdict affiché.
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.predict_window(ctx));
        assert!(app.pedagogy_predict, "la fenêtre reste ouverte");
    }

    /// Une prédiction déjà résolue ne doit pas être recomptée à chaque pas.
    #[test]
    fn resolution_is_idempotent() {
        let mut app = App::new();
        app.prediction = Some(Prediction {
            reg: "RAX", expected: 1, insn: String::new(), step: 0,
            input: "1".into(), before: 0, got: Some(2),
        });
        app.pred_score = Score { right: 0, total: 1 };
        app.resolve_prediction();
        app.resolve_prediction();
        assert_eq!(app.pred_score.total, 1, "pas de double comptage");
    }

    /// Fermer la fenêtre doit décocher l'entrée du menu Affichage, et la
    /// désactiver depuis le menu doit la faire disparaître : un seul état.
    #[test]
    fn window_visibility_follows_the_view_menu_flag() {
        let mut app = App::new();
        let ctx = egui::Context::default();

        // Désactivée : la fenêtre ne se rend pas du tout.
        app.pedagogy_predict = false;
        let out = ctx.run(Default::default(), |ctx| app.predict_window(ctx));
        assert!(
            !out.shapes.iter().any(|s| s.clip_rect.width() > 0.0 && !matches!(s.shape, egui::Shape::Noop)),
            "aucune forme ne doit être peinte quand l'option est désactivée"
        );
        assert!(!app.pedagogy_predict);

        // Activée : elle se rend et l'état reste vrai.
        app.pedagogy_predict = true;
        let _ = ctx.run(Default::default(), |ctx| app.predict_window(ctx));
        assert!(app.pedagogy_predict, "la fenêtre ouverte garde l'option active");
    }

    /// Le titre porte le score, donc il change en cours de partie. L'Id de la
    /// fenêtre doit rester stable, sinon egui oublie sa position à chaque point.
    #[test]
    fn window_id_is_stable_across_score_changes() {
        let mut app = App::new();
        app.pedagogy_predict = true;
        let ctx = egui::Context::default();

        let pos_of = |ctx: &egui::Context| {
            ctx.memory(|m| m.area_rect(egui::Id::new("predict_window")).map(|r| r.min))
        };

        let _ = ctx.run(Default::default(), |ctx| app.predict_window(ctx));
        let first = pos_of(&ctx);
        assert!(first.is_some(), "la fenêtre doit être enregistrée sous son Id explicite");

        // Le score change → le titre change.
        app.pred_score = Score { right: 3, total: 4 };
        let _ = ctx.run(Default::default(), |ctx| app.predict_window(ctx));
        assert_eq!(pos_of(&ctx), first, "la position doit survivre au changement de titre");
    }

    /// Le piège que le champ hexadécimal tend lui-même : l'élève pense « 60 »
    /// en décimal, le champ lit 0x60 = 96. C'est l'erreur la plus fréquente et
    /// elle doit être nommée, pas présentée comme un écart de 36.
    #[test]
    fn decimal_typed_into_a_hex_field_is_recognised() {
        // « mov rax, 60 » → RAX = 0x3C. L'élève tape « 60 », lu 0x60.
        let m = Mistake::detect("60", 0x60, 0x3C, 0, &[]);
        assert_eq!(m, Mistake::DecimalWrittenAsHex { typed: "60".into(), hex: 0x3C });
        let txt = m.explanation(Lang::Fr, "RAX", 0x60, 0x3C, 0);
        assert!(txt.contains("60"), "doit citer ce qui a été tapé : {txt}");
        assert!(txt.contains("3C"), "et la bonne écriture hexa : {txt}");

        // Mais « 0x60 » est une saisie hexa assumée : ce n'est plus ce piège.
        let m = Mistake::detect("0x60", 0x60, 0x3C, 0, &[]);
        assert_ne!(m, Mistake::DecimalWrittenAsHex { typed: "0x60".into(), hex: 0x3C });
    }

    #[test]
    fn giving_the_previous_value_is_recognised() {
        // RAX valait 5, l'instruction le met à 60 ; l'élève annonce 5.
        let m = Mistake::detect("5", 5, 60, 5, &[]);
        assert_eq!(m, Mistake::GaveValueBefore);
        assert!(m.explanation(Lang::Fr, "RAX", 5, 60, 5).contains("avant"));
    }

    #[test]
    fn an_untouched_register_is_recognised() {
        // RBX vaut 7 avant et après ; l'élève annonçait 9.
        let m = Mistake::detect("9", 9, 7, 7, &[]);
        assert_eq!(m, Mistake::RegisterDidNotChange);
    }

    #[test]
    fn twos_complement_is_recognised() {
        // L'élève annonce 5, le registre contient -5.
        let neg5 = 5u64.wrapping_neg();
        let m = Mistake::detect("5", 5, neg5, 0, &[]);
        assert_eq!(m, Mistake::TwosComplement { signed: -5 });
        assert!(m.explanation(Lang::Fr, "RAX", 5, neg5, 0).contains("-5"));
    }

    /// `mov eax, …` remet à zéro les 32 bits hauts : surprise classique.
    #[test]
    fn zero_extension_of_a_32_bit_write_is_recognised() {
        let m = Mistake::detect("1122334455", 0x1122_3344_5566_7788, 0x5566_7788, 0, &[]);
        assert_eq!(m, Mistake::ZeroExtended32);
        assert!(m.explanation(Lang::Fr, "RAX", 0x1122_3344_5566_7788, 0x5566_7788, 0).contains("32"));
    }

    #[test]
    fn stack_word_factor_is_recognised() {
        // L'élève compte 2 cases de pile, le registre a bougé de 16 octets.
        let m = Mistake::detect("2", 2, 16, 0, &[]);
        assert_eq!(m, Mistake::WordSizeFactor);
        assert!(m.explanation(Lang::Fr, "RSP", 2, 16, 0).contains("8"));
    }

    #[test]
    fn off_by_one_is_recognised() {
        let m = Mistake::detect("9", 9, 10, 0, &[]);
        assert_eq!(m, Mistake::OffByOne { diff: 1 });
        let m = Mistake::detect("b", 11, 10, 0, &[]);
        assert_eq!(m, Mistake::OffByOne { diff: -1 });
    }

    /// Bonne valeur, mauvaise destination : on nomme le registre qui la porte.
    #[test]
    fn value_landing_in_another_register_is_recognised() {
        let others = [("RBX", 0x2A_u64), ("RCX", 0)];
        let m = Mistake::detect("2a", 0x2A, 0x99, 0x99, &others);
        // RegisterDidNotChange est plus spécifique et l'emporte ici…
        assert_eq!(m, Mistake::RegisterDidNotChange);
        // …mais si le registre a bien changé, c'est le bon registre qu'on cite.
        let m = Mistake::detect("2a", 0x2A, 0x99, 0x11, &others);
        assert_eq!(m, Mistake::ValueIsInRegister("RBX"));
        assert!(m.explanation(Lang::Fr, "RAX", 0x2A, 0x99, 0x11).contains("RBX"));
    }

    /// Sans motif reconnu, on explique au moins la transition — jamais de
    /// message vide.
    #[test]
    fn unrecognised_still_explains_the_transition() {
        let m = Mistake::detect("dead", 0xDEAD, 0x1234, 0x7777, &[]);
        assert_eq!(m, Mistake::Unrecognised);
        let txt = m.explanation(Lang::Fr, "RAX", 0xDEAD, 0x1234, 0x7777);
        assert!(txt.contains("7777") && txt.contains("1234"), "avant → après : {txt}");
    }

    /// Chaque motif doit être titré et expliqué dans les trois langues.
    #[test]
    fn every_mistake_is_explained_in_every_language() {
        let all = [
            Mistake::DecimalWrittenAsHex { typed: "60".into(), hex: 0x3C },
            Mistake::GaveValueBefore,
            Mistake::RegisterDidNotChange,
            Mistake::TwosComplement { signed: -5 },
            Mistake::ZeroExtended32,
            Mistake::WordSizeFactor,
            Mistake::OffByOne { diff: 1 },
            Mistake::ValueIsInRegister("RBX"),
            Mistake::Unrecognised,
        ];
        for m in &all {
            for lang in [Lang::Fr, Lang::En, Lang::Es] {
                assert!(!m.title(lang).is_empty(), "{m:?} sans titre en {lang:?}");
                let e = m.explanation(lang, "RAX", 5, 9, 1);
                assert!(e.len() > 20, "{m:?} : explication trop courte en {lang:?} : {e}");
            }
        }
    }
}
