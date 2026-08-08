//! Conditions attachées aux points d'arrêt.
//!
//! Un point d'arrêt nu suffit tant que la ligne visée est exécutée une fois.
//! Dans une boucle, il ne suffit plus : s'arrêter au tour 4 000 demandait
//! jusqu'ici quatre mille « Continuer ». La condition répond à la vraie
//! question de l'élève — « arrête-toi *quand* RCX vaut 0 » — et c'est aussi
//! l'occasion de lui faire écrire ce qu'il croit être vrai.
//!
//! La grammaire tient en une ligne, volontairement :
//!
//! ```text
//! condition := opérande comparateur opérande
//! opérande  := registre | drapeau | littéral
//! registre  := RAX…R15, RIP, EFLAGS, et leurs moitiés basses EAX…R15D
//! drapeau   := ZF, CF, OF, SF, PF, AF   (valent 0 ou 1)
//! littéral  := 42 | -1 | 0x2A | 0b1010
//! comparateur := == != < <= > >=
//! ```
//!
//! Pas de conjonction, pas de parenthèses, pas de déréférencement mémoire :
//! une condition qui demanderait un manuel n'aurait plus rien de pédagogique,
//! et l'immense majorité tient dans cette forme.
//!
//! Les registres se comparent en non signé, sauf si un littéral négatif
//! apparaît — auquel cas tout passe en signé. `RAX == -1` et
//! `RAX == 0xFFFFFFFFFFFFFFFF` reconnaissent donc la même valeur. Corollaire à
//! connaître : `RAX < 0` n'est jamais vrai (aucun négatif en vue, donc lecture
//! non signée) ; « ce registre est-il négatif ? » s'écrit `SF == 1`, ce qui est
//! de toute façon la bonne leçon.

use crate::debugger::{Flags, Registers};
use crate::i18n::{self, Lang};

/// Comparateur d'une condition.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Op {
    fn apply(self, a: i128, b: i128) -> bool {
        match self {
            Op::Eq => a == b,
            Op::Ne => a != b,
            Op::Lt => a < b,
            Op::Le => a <= b,
            Op::Gt => a > b,
            Op::Ge => a >= b,
        }
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
}

/// Base dans laquelle un littéral a été écrit. Conservée pour le réafficher
/// tel quel : un masque saisi en `0xFF00` relu « 65280 » donnerait
/// l'impression que la condition a été comprise de travers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Radix {
    Dec,
    Hex,
    Bin,
}

/// Un côté de la comparaison.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Operand {
    /// Registre ou drapeau, par son nom en majuscules.
    Name(String),
    /// Littéral, en i128 pour tenir aussi bien `0xFFFFFFFFFFFFFFFF` que `-1`.
    Imm(i128, Radix),
}

/// Une condition analysée, prête à être évaluée à chaque passage.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Condition {
    left: Operand,
    op: Op,
    right: Operand,
}

impl std::fmt::Display for Condition {
    /// Réécrit la condition sous forme normalisée : c'est ce qui s'affiche
    /// dans l'infobulle de la pastille, et ça confirme à l'élève ce qui a été
    /// compris de ce qu'il a tapé.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let show = |o: &Operand| match o {
            Operand::Name(n) => n.clone(),
            Operand::Imm(v, _) if *v < 0 => v.to_string(),
            Operand::Imm(v, Radix::Hex) => format!("0x{v:X}"),
            Operand::Imm(v, Radix::Bin) => format!("0b{v:b}"),
            Operand::Imm(v, Radix::Dec) => v.to_string(),
        };
        write!(f, "{} {} {}", show(&self.left), self.op.symbol(), show(&self.right))
    }
}

impl Condition {
    /// Vrai si la condition tient dans cet état du CPU.
    ///
    /// Un nom que l'on ne sait pas résoudre renvoie `false` : l'exécution
    /// continue au lieu de s'arrêter sans raison compréhensible. Le parseur
    /// ayant déjà refusé les noms inconnus, le cas ne devrait pas se produire.
    pub fn eval(&self, regs: &Registers, flags: &Flags) -> bool {
        // Un littéral négatif dit assez que l'élève raisonne en signé : les
        // registres sont alors lus comme des entiers signés, sans quoi
        // `RAX == -1` serait faux face à 0xFFFFFFFFFFFFFFFF, ce qui est
        // exactement ce que cette valeur veut dire.
        let signed = matches!(self.left, Operand::Imm(v, _) if v < 0)
            || matches!(self.right, Operand::Imm(v, _) if v < 0);
        let (Some(a), Some(b)) = (
            resolve(&self.left, regs, flags, signed),
            resolve(&self.right, regs, flags, signed),
        ) else {
            return false;
        };
        self.op.apply(a, b)
    }
}

/// Valeur d'un opérande dans l'état courant.
fn resolve(operand: &Operand, regs: &Registers, flags: &Flags, signed: bool) -> Option<i128> {
    match operand {
        Operand::Imm(v, _) => Some(*v),
        Operand::Name(name) => {
            if let Some((_, v)) = flags.named().iter().find(|(n, _)| *n == name) {
                return Some(*v as i128);
            }
            let (full, mask32) = match name.strip_suffix('D') {
                // R8D…R15D : moitié basse des registres numérotés.
                Some(base) if base.starts_with('R') => (base.to_string(), true),
                _ => match name.strip_prefix('E') {
                    // EAX…EDI : même chose pour les registres historiques.
                    // « EFLAGS » n'entre pas ici, il est dans la liste telle quelle.
                    Some(rest) if name != "EFLAGS" => (format!("R{rest}"), true),
                    _ => (name.clone(), false),
                },
            };
            let (_, value) = regs.named().into_iter().find(|(n, _)| *n == full)?;
            Some(match (mask32, signed) {
                (true, true) => value as u32 as i32 as i128,
                (true, false) => (value & 0xFFFF_FFFF) as i128,
                (false, true) => value as i64 as i128,
                (false, false) => value as i128,
            })
        }
    }
}

/// Tous les noms acceptés à gauche ou à droite d'un comparateur, pour le
/// message d'erreur et l'aide de saisie.
pub fn known_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = Registers::default().named().iter().map(|(n, _)| *n).collect();
    names.extend(Flags::default().named().iter().map(|(n, _)| *n));
    names
}

/// Analyse une condition écrite par l'élève.
///
/// `Ok(None)` pour une saisie vide : c'est la façon de retirer la condition
/// sans retirer le point d'arrêt, et ce n'est pas une erreur.
pub fn parse(text: &str, lang: Lang) -> Result<Option<Condition>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    // Les comparateurs à deux caractères d'abord : « <= » commence par « < »,
    // et le chercher dans le désordre couperait la condition au mauvais endroit.
    const OPS: [(&str, Op); 7] = [
        ("==", Op::Eq),
        ("!=", Op::Ne),
        ("<=", Op::Le),
        (">=", Op::Ge),
        ("<", Op::Lt),
        (">", Op::Gt),
        // Un seul « = » est l'erreur de frappe la plus courante quand on vient
        // d'écrire du NASM toute la journée : on l'accepte comme égalité
        // plutôt que de renvoyer l'élève à sa syntaxe.
        ("=", Op::Eq),
    ];
    let Some((sym, op)) = OPS.iter().find(|(sym, _)| text.contains(sym)) else {
        return Err(i18n::tr3(
            lang,
            "Il manque une comparaison : ==, !=, <, <=, > ou >=. Exemple : RCX == 0",
            "A comparison is missing: ==, !=, <, <=, > or >=. Example: RCX == 0",
            "Falta una comparación: ==, !=, <, <=, > o >=. Ejemplo: RCX == 0",
        )
        .to_string());
    };
    let (l, r) = text.split_once(sym).expect("le séparateur vient d'être trouvé");
    Ok(Some(Condition {
        left: parse_operand(l, lang)?,
        op: *op,
        right: parse_operand(r, lang)?,
    }))
}

fn parse_operand(text: &str, lang: Lang) -> Result<Operand, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(i18n::tr3(
            lang,
            "Il manque une valeur d'un côté de la comparaison.",
            "A value is missing on one side of the comparison.",
            "Falta un valor en un lado de la comparación.",
        )
        .to_string());
    }
    let upper = text.to_ascii_uppercase();
    if known_names().contains(&upper.as_str()) || is_half_register(&upper) {
        return Ok(Operand::Name(upper));
    }
    match parse_number(&upper) {
        Some((v, radix)) => Ok(Operand::Imm(v, radix)),
        None => Err(match lang {
            Lang::Fr => format!(
                "« {text} » n'est ni un registre, ni un drapeau, ni un nombre. \
                 Registres : RAX…R15, RIP ; drapeaux : ZF, CF, OF, SF, PF, AF ; \
                 nombres : 42, -1, 0x2A, 0b1010."
            ),
            Lang::En => format!(
                "“{text}” is neither a register, a flag, nor a number. \
                 Registers: RAX…R15, RIP; flags: ZF, CF, OF, SF, PF, AF; \
                 numbers: 42, -1, 0x2A, 0b1010."
            ),
            Lang::Es => format!(
                "«{text}» no es un registro, ni un flag, ni un número. \
                 Registros: RAX…R15, RIP; flags: ZF, CF, OF, SF, PF, AF; \
                 números: 42, -1, 0x2A, 0b1010."
            ),
        }),
    }
}

/// Moitié basse d'un registre 64 bits : EAX…EDI, R8D…R15D.
fn is_half_register(name: &str) -> bool {
    let full = match name.strip_suffix('D') {
        Some(base) if base.starts_with('R') => base.to_string(),
        _ => match name.strip_prefix('E') {
            Some(rest) if name != "EFLAGS" => format!("R{rest}"),
            _ => return false,
        },
    };
    Registers::default().named().iter().any(|(n, _)| *n == full)
}

/// Littéral décimal, hexadécimal (`0x`) ou binaire (`0b`), signe compris,
/// rendu avec la base dans laquelle il a été écrit.
/// Le souligné est toléré : `0b1010_0110` se lit bien mieux.
fn parse_number(text: &str) -> Option<(i128, Radix)> {
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest.trim_start()),
        None => (1, text.strip_prefix('+').unwrap_or(text).trim_start()),
    };
    let digits = digits.replace('_', "");
    let (magnitude, radix) = match digits.strip_prefix("0X") {
        Some(hex) => (i128::from_str_radix(hex, 16).ok()?, Radix::Hex),
        None => match digits.strip_prefix("0B") {
            Some(bin) => (i128::from_str_radix(bin, 2).ok()?, Radix::Bin),
            None => (digits.parse::<i128>().ok()?, Radix::Dec),
        },
    };
    Some((sign * magnitude, radix))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regs() -> Registers {
        Registers { rax: 42, rcx: 0, rbx: u64::MAX, rip: 0x401000, ..Default::default() }
    }

    fn flags() -> Flags {
        Flags { zf: true, ..Default::default() }
    }

    fn ok(text: &str) -> Condition {
        parse(text, Lang::Fr).expect("condition valide").expect("condition non vide")
    }

    #[test]
    fn an_empty_condition_is_not_an_error() {
        assert_eq!(parse("   ", Lang::Fr), Ok(None), "vide = pas de condition");
    }

    #[test]
    fn registers_compare_against_numbers_in_every_base() {
        for text in ["RAX == 42", "RAX == 0x2A", "RAX == 0b101010", "rax==42"] {
            assert!(ok(text).eval(&regs(), &flags()), "{text}");
        }
        assert!(!ok("RAX == 43").eval(&regs(), &flags()));
    }

    #[test]
    fn every_comparator_works() {
        let cases = [
            ("RAX == 42", true),
            ("RAX != 42", false),
            ("RAX < 43", true),
            ("RAX <= 42", true),
            ("RAX > 41", true),
            ("RAX >= 43", false),
        ];
        for (text, expected) in cases {
            assert_eq!(ok(text).eval(&regs(), &flags()), expected, "{text}");
        }
    }

    /// Le « = » de NASM est accepté comme égalité : c'est l'erreur de frappe
    /// la plus prévisible, et la refuser n'apprendrait rien.
    #[test]
    fn a_single_equals_is_read_as_equality() {
        assert!(ok("RAX = 42").eval(&regs(), &flags()));
    }

    #[test]
    fn flags_are_usable_and_worth_zero_or_one() {
        assert!(ok("ZF == 1").eval(&regs(), &flags()));
        assert!(ok("CF == 0").eval(&regs(), &flags()));
        assert!(!ok("ZF == 0").eval(&regs(), &flags()));
    }

    #[test]
    fn two_registers_can_be_compared() {
        assert!(ok("RAX > RCX").eval(&regs(), &flags()));
        assert!(!ok("RAX == RCX").eval(&regs(), &flags()));
    }

    /// 0xFFFFFFFFFFFFFFFF vaut -1 pour qui raisonne en signé : les deux
    /// écritures doivent reconnaître la même valeur.
    #[test]
    fn a_negative_literal_switches_the_comparison_to_signed() {
        assert!(ok("RBX == -1").eval(&regs(), &flags()), "RBX = 0xFFFFFFFFFFFFFFFF");
        assert!(ok("RBX == 0xFFFFFFFFFFFFFFFF").eval(&regs(), &flags()));
        // Comparée à un négatif, elle vaut bien -1, donc plus que -2…
        assert!(ok("RBX > -2").eval(&regs(), &flags()));
        // … alors que sans négatif en vue, elle reste la plus grande valeur
        // non signée possible. C'est la règle, et le seul piège de la syntaxe :
        // `RBX < 0` n'est jamais vrai, il faut `RBX == -1` ou `SF == 1`.
        assert!(ok("RBX > 1000").eval(&regs(), &flags()));
        assert!(!ok("RBX < 0").eval(&regs(), &flags()));
    }

    #[test]
    fn half_registers_only_see_the_low_32_bits() {
        let r = Registers { rax: 0x1234_5678_9ABC_DEF0, r8: 0xFFFF_FFFF_0000_0001, ..Default::default() };
        assert!(ok("EAX == 0x9ABCDEF0").eval(&r, &flags()));
        assert!(!ok("EAX == 0x123456789ABCDEF0").eval(&r, &flags()));
        assert!(ok("R8D == 1").eval(&r, &flags()));
    }

    /// EFLAGS ne doit pas être confondu avec la moitié basse d'un « RFLAGS »
    /// qui n'existe pas.
    #[test]
    fn eflags_stays_a_register_of_its_own() {
        let r = Registers { eflags: 0x246, ..Default::default() };
        assert!(ok("EFLAGS == 0x246").eval(&r, &flags()));
    }

    #[test]
    fn underscores_in_numbers_are_tolerated() {
        let r = Registers { rax: 0b1010_0110, ..Default::default() };
        assert!(ok("RAX == 0b1010_0110").eval(&r, &flags()));
    }

    #[test]
    fn a_missing_comparator_is_explained() {
        let err = parse("RAX 42", Lang::Fr).unwrap_err();
        assert!(err.contains("=="), "le message doit montrer la syntaxe : {err}");
    }

    #[test]
    fn an_unknown_name_is_explained_and_lists_what_is_accepted() {
        let err = parse("RXX == 1", Lang::Fr).unwrap_err();
        assert!(err.contains("RXX"), "le message doit citer ce qui n'a pas été compris");
        assert!(err.contains("ZF"), "et rappeler ce qui est accepté : {err}");
    }

    #[test]
    fn a_half_written_condition_is_refused() {
        assert!(parse("RAX ==", Lang::Fr).is_err());
        assert!(parse("== 3", Lang::Fr).is_err());
    }

    /// Les trois langues répondent, et jamais avec le texte d'une autre.
    #[test]
    fn errors_speak_the_interface_language() {
        let fr = parse("RXX == 1", Lang::Fr).unwrap_err();
        let en = parse("RXX == 1", Lang::En).unwrap_err();
        let es = parse("RXX == 1", Lang::Es).unwrap_err();
        assert!(fr.contains("drapeau") && en.contains("flag") && es.contains("flag"));
        assert_ne!(fr, en);
        assert_ne!(en, es);
    }

    /// La forme normalisée est ce que l'infobulle montrera : les noms sont
    /// remis en majuscules et la comparaison espacée, mais la base d'écriture
    /// du nombre est celle de l'élève — un masque saisi en hexadécimal relu en
    /// décimal donnerait l'impression d'avoir été mal compris.
    #[test]
    fn a_condition_reads_back_normalized_without_changing_base() {
        assert_eq!(ok("rax=42").to_string(), "RAX == 42");
        assert_eq!(ok("RCX <= 0x1_0000").to_string(), "RCX <= 0x10000");
        assert_eq!(ok("rdx == 0b1010").to_string(), "RDX == 0b1010");
        assert_eq!(ok("RAX != -1").to_string(), "RAX != -1");
        assert_eq!(ok("ZF != CF").to_string(), "ZF != CF");
    }
}
