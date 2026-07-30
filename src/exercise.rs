//! Exercices auto-corrigés : des attentes déclarées dans le source NASM.
//!
//! Les exemples livrés sont des démonstrations — l'élève les exécute et regarde.
//! Rien ne lui dit s'il a *juste*. Un exercice ajoute des attentes vérifiables,
//! écrites en commentaires du fichier `.asm` lui-même : pas de fichier annexe à
//! synchroniser, et l'énoncé voyage avec le code.
//!
//! ```text
//! ;@titre Calculer 5!
//! ;@enonce Laisse le résultat dans RAX avant de quitter.
//! ;@attendu rax == 120
//! ;@attendu exit == 0
//! ```
//!
//! Les directives sont des commentaires NASM ordinaires : un fichier d'exercice
//! s'assemble et s'exécute normalement, y compris hors de l'application.

use crate::debugger::Registers;
use crate::i18n::{self, Lang};

/// Ce qu'une attente porte sur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    /// Un registre général, par son nom majuscule (« RAX »).
    Register(&'static str),
    /// Le code de sortie du programme.
    ExitCode,
}

/// Comparaison demandée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Op {
    fn parse(s: &str) -> Option<Op> {
        Some(match s {
            "==" | "=" => Op::Eq,
            "!=" | "<>" => Op::Ne,
            "<" => Op::Lt,
            "<=" => Op::Le,
            ">" => Op::Gt,
            ">=" => Op::Ge,
            _ => return None,
        })
    }

    fn symbol(self) -> &'static str {
        match self {
            Op::Eq => "==",
            Op::Ne => "!=",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
        }
    }

    /// Applique la comparaison en arithmétique signée : c'est ce qu'attend un
    /// élève qui écrit `rax >= -1`.
    fn holds(self, got: i64, want: i64) -> bool {
        match self {
            Op::Eq => got == want,
            Op::Ne => got != want,
            Op::Lt => got < want,
            Op::Le => got <= want,
            Op::Gt => got > want,
            Op::Ge => got >= want,
        }
    }
}

/// Une attente à vérifier à la fin du programme.
#[derive(Debug, Clone, PartialEq)]
pub struct Expectation {
    pub subject: Subject,
    pub op: Op,
    pub want: i64,
    /// Ligne du fichier où l'attente est déclarée (pour signaler une faute de
    /// frappe dans l'énoncé).
    pub line: usize,
}

impl Expectation {
    /// Forme lisible, ex. « RAX == 120 ».
    pub fn label(&self) -> String {
        let subj = match self.subject {
            Subject::Register(r) => r.to_string(),
            Subject::ExitCode => "exit".to_string(),
        };
        format!("{subj} {} {}", self.op.symbol(), self.want)
    }
}

/// L'énoncé complet extrait d'un fichier source.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Exercise {
    pub title: Option<String>,
    pub statement: Option<String>,
    pub expectations: Vec<Expectation>,
    /// Directives mal formées, signalées à l'auteur de l'exercice.
    pub errors: Vec<String>,
}

impl Exercise {
    /// Vrai si le fichier déclare au moins une attente vérifiable.
    pub fn is_exercise(&self) -> bool {
        !self.expectations.is_empty()
    }
}

/// Résultat de la vérification d'une attente.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub expectation: Expectation,
    /// Valeur observée, ou `None` si elle n'est pas disponible (ex. code de
    /// sortie alors que le programme n'a pas encore quitté).
    pub got: Option<i64>,
}

impl Check {
    pub fn passed(&self) -> bool {
        self.got
            .is_some_and(|g| self.expectation.op.holds(g, self.expectation.want))
    }
}

/// Noms de registres reconnus dans les directives.
const REGS: [&str; 16] = [
    "RAX", "RBX", "RCX", "RDX", "RSI", "RDI", "RBP", "RSP",
    "R8", "R9", "R10", "R11", "R12", "R13", "R14", "R15",
];

/// Extrait l'énoncé et les attentes d'un source NASM.
///
/// Reconnaît, en français comme en anglais, `;@titre`/`;@title`,
/// `;@enonce`/`;@statement`, `;@attendu`/`;@expect`. Les lignes qui ne sont pas
/// des directives sont ignorées : un fichier ordinaire produit un [`Exercise`]
/// vide dont [`Exercise::is_exercise`] est faux.
pub fn parse(source: &str) -> Exercise {
    let mut ex = Exercise::default();

    for (i, raw) in source.lines().enumerate() {
        let line = i + 1;
        let t = raw.trim();
        // Une directive est un commentaire NASM commençant par @.
        let Some(rest) = t.strip_prefix(";@").or_else(|| t.strip_prefix("; @")) else {
            continue;
        };
        let (key, value) = match rest.split_once(char::is_whitespace) {
            Some((k, v)) => (k.trim().to_lowercase(), v.trim()),
            None => (rest.trim().to_lowercase(), ""),
        };

        match key.as_str() {
            "titre" | "title" => ex.title = Some(value.to_string()),
            // Un énoncé sur plusieurs lignes s'accumule.
            "enonce" | "énoncé" | "statement" => match &mut ex.statement {
                Some(s) => {
                    s.push(' ');
                    s.push_str(value);
                }
                None => ex.statement = Some(value.to_string()),
            },
            "attendu" | "expect" | "assert" => match parse_expectation(value, line) {
                Ok(e) => ex.expectations.push(e),
                Err(msg) => ex.errors.push(format!("{line}: {msg}")),
            },
            _ => ex.errors.push(format!("{line}: directive inconnue « {key} »")),
        }
    }
    ex
}

/// Analyse « rax == 120 » / « exit != 0 » / « rbx >= -1 ».
fn parse_expectation(s: &str, line: usize) -> Result<Expectation, String> {
    let mut it = s.split_whitespace();
    let subj_raw = it.next().ok_or("attente vide")?;
    let op_raw = it.next().ok_or_else(|| format!("comparateur manquant après « {subj_raw} »"))?;
    let val_raw = it.next().ok_or_else(|| format!("valeur manquante après « {op_raw} »"))?;
    if it.next().is_some() {
        return Err(format!("texte en trop après « {val_raw} »"));
    }

    let up = subj_raw.to_uppercase();
    let subject = if up == "EXIT" || up == "SORTIE" {
        Subject::ExitCode
    } else {
        // On renvoie un &'static str en retrouvant le nom dans la table.
        let reg = REGS
            .iter()
            .find(|r| **r == up)
            .ok_or_else(|| format!("sujet inconnu « {subj_raw} » (registre ou « exit »)"))?;
        Subject::Register(reg)
    };

    let op = Op::parse(op_raw).ok_or_else(|| format!("comparateur inconnu « {op_raw} »"))?;
    let want = parse_value(val_raw)
        .ok_or_else(|| format!("valeur illisible « {val_raw} » (décimal, 0x… ou 0b…)"))?;

    Ok(Expectation { subject, op, want, line })
}

/// Lit une valeur décimale signée, hexadécimale (`0x`) ou binaire (`0b`),
/// ainsi qu'un caractère entre apostrophes (`'A'`) — pratique pour les exercices
/// sur les chaînes.
fn parse_value(s: &str) -> Option<i64> {
    let s = s.trim();
    // Caractère : 'A' → 65.
    if s.len() == 3 && s.starts_with('\'') && s.ends_with('\'') {
        return s.chars().nth(1).map(|c| c as i64);
    }
    let (neg, body) = match s.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let v = if let Some(h) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        i64::from_str_radix(h, 16).ok()?
    } else if let Some(b) = body.strip_prefix("0b").or_else(|| body.strip_prefix("0B")) {
        i64::from_str_radix(b, 2).ok()?
    } else {
        body.parse::<i64>().ok()?
    };
    Some(if neg { -v } else { v })
}

/// Vérifie toutes les attentes contre l'état final observé.
///
/// * `regs` — registres au dernier instant connu ;
/// * `exit_code` — code de sortie, `None` si le programme n'a pas quitté
///   normalement (encore en cours, tué, ou planté).
pub fn check(ex: &Exercise, regs: &Registers, exit_code: Option<i32>) -> Vec<Check> {
    ex.expectations
        .iter()
        .map(|e| {
            let got = match e.subject {
                Subject::Register(name) => regs
                    .named()
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, v)| *v as i64),
                Subject::ExitCode => exit_code.map(|c| c as i64),
            };
            Check { expectation: e.clone(), got }
        })
        .collect()
}

/// Bilan : (réussies, total).
pub fn tally(checks: &[Check]) -> (usize, usize) {
    (checks.iter().filter(|c| c.passed()).count(), checks.len())
}

/// Message de synthèse traduit.
pub fn summary(checks: &[Check], lang: Lang) -> String {
    let (ok, total) = tally(checks);
    if total == 0 {
        return i18n::tr3(lang, "Aucune attente déclarée.", "No expectation declared.", "Ninguna expectativa declarada.")
            .to_string();
    }
    if ok == total {
        format!(
            "{} {ok}/{total}",
            i18n::tr3(lang, "✔ Exercice réussi —", "✔ Exercise passed —", "✔ Ejercicio superado —")
        )
    } else {
        format!(
            "{} {ok}/{total}",
            i18n::tr3(lang, "✘ Pas encore —", "✘ Not yet —", "✘ Todavía no —")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regs_with(rax: u64, rbx: u64) -> Registers {
        Registers { rax, rbx, ..Default::default() }
    }

    #[test]
    fn plain_file_is_not_an_exercise() {
        let ex = parse("section .text\n global _start\n_start:\n mov rax, 60\n ; commentaire\n");
        assert!(!ex.is_exercise());
        assert!(ex.errors.is_empty(), "un commentaire ordinaire n'est pas une erreur");
        assert_eq!(ex.title, None);
    }

    #[test]
    fn parses_title_statement_and_expectations() {
        let src = "\
;@titre Factorielle
;@enonce Calcule 5! et
;@enonce laisse le résultat dans RAX.
;@attendu rax == 120
;@attendu exit == 0
section .text
";
        let ex = parse(src);
        assert!(ex.is_exercise());
        assert_eq!(ex.title.as_deref(), Some("Factorielle"));
        assert_eq!(
            ex.statement.as_deref(),
            Some("Calcule 5! et laisse le résultat dans RAX."),
            "les lignes d'énoncé s'accumulent"
        );
        assert_eq!(ex.expectations.len(), 2);
        assert_eq!(ex.expectations[0].subject, Subject::Register("RAX"));
        assert_eq!(ex.expectations[0].want, 120);
        assert_eq!(ex.expectations[1].subject, Subject::ExitCode);
        assert!(ex.errors.is_empty(), "erreurs : {:?}", ex.errors);
    }

    #[test]
    fn accepts_english_directives_and_alternate_spellings() {
        let ex = parse(";@title T\n;@statement S\n;@expect rax == 1\n;@assert rbx != 0\n; @expect rcx >= 0\n");
        assert_eq!(ex.title.as_deref(), Some("T"));
        assert_eq!(ex.expectations.len(), 3, "erreurs : {:?}", ex.errors);
    }

    #[test]
    fn parses_all_value_forms() {
        let ex = parse(
            ";@attendu rax == 0x1F\n;@attendu rbx == 0b1010\n;@attendu rcx == -1\n;@attendu rdx == 'A'\n",
        );
        assert!(ex.errors.is_empty(), "erreurs : {:?}", ex.errors);
        let w: Vec<i64> = ex.expectations.iter().map(|e| e.want).collect();
        assert_eq!(w, vec![31, 10, -1, 65]);
    }

    #[test]
    fn all_comparators_parse() {
        let ex = parse(
            ";@attendu rax == 1\n;@attendu rax != 1\n;@attendu rax < 1\n\
             ;@attendu rax <= 1\n;@attendu rax > 1\n;@attendu rax >= 1\n",
        );
        assert_eq!(ex.expectations.len(), 6, "erreurs : {:?}", ex.errors);
        let ops: Vec<Op> = ex.expectations.iter().map(|e| e.op).collect();
        assert_eq!(ops, vec![Op::Eq, Op::Ne, Op::Lt, Op::Le, Op::Gt, Op::Ge]);
    }

    /// Une directive fautive doit être signalée avec sa ligne, pas ignorée en
    /// silence : sinon l'auteur croit son exercice vérifié alors qu'il ne l'est pas.
    #[test]
    fn malformed_directives_are_reported_with_line() {
        let ex = parse(";@attendu rax\n;@attendu rax ~~ 3\n;@attendu zzz == 1\n;@attendu rax == abc\n;@bidule x\n");
        assert!(ex.expectations.is_empty());
        assert_eq!(ex.errors.len(), 5);
        assert!(ex.errors[0].starts_with('1'), "{:?}", ex.errors[0]);
        assert!(ex.errors[1].contains("comparateur"), "{:?}", ex.errors[1]);
        assert!(ex.errors[2].contains("zzz"), "{:?}", ex.errors[2]);
        assert!(ex.errors[3].contains("abc"), "{:?}", ex.errors[3]);
        assert!(ex.errors[4].contains("inconnue"), "{:?}", ex.errors[4]);
    }

    #[test]
    fn check_passes_and_fails_correctly() {
        let ex = parse(";@attendu rax == 120\n;@attendu rbx == 7\n;@attendu exit == 0\n");
        let checks = check(&ex, &regs_with(120, 99), Some(0));
        assert!(checks[0].passed(), "RAX = 120 attendu 120");
        assert!(!checks[1].passed(), "RBX = 99 attendu 7");
        assert!(checks[2].passed(), "exit 0");
        assert_eq!(tally(&checks), (2, 3));
    }

    /// Sans code de sortie (programme encore en cours ou planté), l'attente sur
    /// `exit` échoue au lieu de passer par défaut.
    #[test]
    fn missing_exit_code_fails_rather_than_passes() {
        let ex = parse(";@attendu exit == 0\n");
        let checks = check(&ex, &regs_with(0, 0), None);
        assert_eq!(checks[0].got, None);
        assert!(!checks[0].passed(), "une attente non observable ne doit pas passer");
    }

    /// Les comparaisons sont signées : `rax >= -1` avec RAX = 0xFFFF…FF (=-1)
    /// doit passer, alors qu'en non signé ce serait un très grand nombre.
    #[test]
    fn comparisons_are_signed() {
        let ex = parse(";@attendu rax >= -1\n;@attendu rax == -1\n");
        let checks = check(&ex, &regs_with(u64::MAX, 0), None);
        assert!(checks[0].passed(), "-1 >= -1");
        assert!(checks[1].passed(), "0xFFFF… vaut -1 en signé");
    }

    #[test]
    fn summary_is_translated_and_reflects_tally() {
        let ex = parse(";@attendu rax == 1\n");
        let pass = check(&ex, &regs_with(1, 0), None);
        let fail = check(&ex, &regs_with(2, 0), None);
        for lang in [Lang::Fr, Lang::En, Lang::Es] {
            assert!(summary(&pass, lang).contains("1/1"));
            assert!(summary(&pass, lang).contains('✔'), "réussi en {lang:?}");
            assert!(summary(&fail, lang).contains('✘'), "échoué en {lang:?}");
            assert!(!summary(&[], lang).is_empty());
        }
    }

    #[test]
    fn expectation_label_is_readable() {
        let ex = parse(";@attendu rax == 120\n;@attendu exit != 3\n");
        assert_eq!(ex.expectations[0].label(), "RAX == 120");
        assert_eq!(ex.expectations[1].label(), "exit != 3");
    }
}
