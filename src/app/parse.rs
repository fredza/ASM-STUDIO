//! Analyse et formatage des saisies numériques.
//!
//! Sert au laboratoire mémoire (adresses et octets hexadécimaux) et à la
//! calculatrice multi-base. En base 10 les valeurs sont signées (`i64`) ; dans
//! les autres bases on manipule le motif de bits (`u64` casté), ce qui permet
//! d'afficher un nombre négatif sous sa forme hexadécimale réelle.

pub(super) fn parse_hex(s: &str) -> Option<u64> {
    let s = s.trim();
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u64::from_str_radix(s, 16).ok()
}

/// Analyse une suite d'octets hexadécimaux (« 48 65 6C » ou « 48656C »).
pub(super) fn parse_hex_bytes(s: &str) -> Option<Vec<u8>> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty() || !cleaned.len().is_multiple_of(2) {
        return None;
    }
    (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).ok())
        .collect()
}

/// Base fictive : la saisie n'est plus un nombre mais du texte, chaque
/// caractère valant son code ASCII (« Hi » = 0x4869).
///
/// 256 ne peut pas entrer en collision avec une vraie base : `from_str_radix`
/// s'arrête à 36, et `char::is_digit` panique au-delà — d'où les branches
/// ASCII placées AVANT tout appel à ces deux fonctions.
pub(crate) const CALC_BASE_ASCII: u32 = 256;

/// Décode une saisie ASCII en octets, en interprétant les échappements
/// `\0`, `\t`, `\n`, `\r`, `\\` et `\xNN`.
///
/// Sans échappements on ne pourrait ni saisir un octet nul ni un octet non
/// imprimable — or ce sont précisément ceux qui comptent en assembleur (le
/// zéro terminal d'une chaîne, le `\n` d'un `write`).
///
/// Une séquence incomplète (`\x4`, antislash final) ne produit rien : elle est
/// en cours de frappe. Un caractère hors ASCII n'a pas de code sur un octet,
/// il est ignoré.
pub(super) fn calc_ascii_bytes(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            if c.is_ascii() {
                out.push(c as u8);
            }
            continue;
        }
        match chars.next() {
            Some('0') => out.push(0),
            Some('t') => out.push(b'\t'),
            Some('n') => out.push(b'\n'),
            Some('r') => out.push(b'\r'),
            Some('\\') => out.push(b'\\'),
            Some('x') | Some('X') => {
                let hi = chars.peek().copied().filter(char::is_ascii_hexdigit);
                let Some(hi) = hi else { continue };
                chars.next();
                let lo = chars.peek().copied().filter(char::is_ascii_hexdigit);
                let Some(lo) = lo else { continue };
                chars.next();
                let byte = (hi.to_digit(16).unwrap() * 16 + lo.to_digit(16).unwrap()) as u8;
                out.push(byte);
            }
            // `\q` : l'antislash ne veut rien dire ici, on garde la lettre.
            Some(other) if other.is_ascii() => out.push(other as u8),
            _ => {}
        }
    }
    out
}

/// Rend les octets significatifs de `v` sous forme de texte ASCII, les
/// caractères non imprimables étant échappés — donc relisible par
/// [`calc_ascii_bytes`].
pub(super) fn calc_ascii_text(v: i64) -> String {
    calc_bytes_of(v, calc_width_bytes(v))
        .into_iter()
        .map(|b| match b {
            0 => "\\0".to_string(),
            b'\t' => "\\t".to_string(),
            b'\n' => "\\n".to_string(),
            b'\r' => "\\r".to_string(),
            b'\\' => "\\\\".to_string(),
            0x20..=0x7E => (b as char).to_string(),
            _ => format!("\\x{b:02X}"),
        })
        .collect()
}

/// Analyse une valeur dans la base donnée (2, 8, 10, 16 ou ASCII).
/// Base 10 : signé (`i64`), supporte le signe `-`. Autres bases : bit-pattern `u64` casté.
/// Renvoie `None` si vide ou hors plage.
pub(super) fn calc_parse(s: &str, base: u32) -> Option<i64> {
    if base == CALC_BASE_ASCII {
        // Pas de `trim` ici : l'espace est un caractère (0x20) comme un autre.
        let bytes = calc_ascii_bytes(s);
        if bytes.is_empty() || bytes.len() > 8 {
            return None;
        }
        return Some(bytes.iter().fold(0u64, |acc, &b| (acc << 8) | b as u64) as i64);
    }
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if base == 10 {
        return s.parse::<i64>().ok();
    }
    u64::from_str_radix(s, base).ok().map(|v| v as i64)
}

/// Formate `v` dans la base donnée, avec préfixe (`0x`/`0o`/`0b`) sauf en base 10.
/// Hex/Oct/Bin : affiche le motif de bits en non-signé. Dec : affiche signé.
/// ASCII : le texte entre apostrophes.
pub(super) fn calc_format(v: i64, base: u32) -> String {
    match base {
        16 => format!("0x{:X}", v as u64),
        8 => format!("0o{:o}", v as u64),
        2 => format!("0b{:b}", v as u64),
        CALC_BASE_ASCII => format!("'{}'", calc_ascii_text(v)),
        _ => format!("{v}"),
    }
}

/// Même valeur, sans les décorations (préfixe `0x`, apostrophes) : ce qui peut
/// être remis tel quel dans un champ de saisie.
pub(super) fn calc_format_bare(v: i64, base: u32) -> String {
    if base == CALC_BASE_ASCII {
        return calc_ascii_text(v);
    }
    calc_format(v, base)
        .trim_start_matches("0x")
        .trim_start_matches("0b")
        .trim_start_matches("0o")
        .to_string()
}

/// Nettoie une saisie pour la base donnée : ne garde que ce qui a un sens, et
/// borne l'ASCII à 8 octets — la largeur d'un registre.
pub(super) fn calc_sanitize(s: &mut String, base: u32) {
    match base {
        CALC_BASE_ASCII => {
            s.retain(|c| c.is_ascii() && !c.is_ascii_control());
            // Retirer un caractère peut casser un `\xNN` en séquence
            // incomplète, qui ne compte plus : la boucle converge quand même.
            while calc_ascii_bytes(s).len() > 8 {
                s.pop();
            }
        }
        10 => {
            let neg = s.starts_with('-');
            s.retain(|c| c.is_ascii_digit());
            if neg {
                s.insert(0, '-');
            }
        }
        _ => s.retain(|c| c.is_digit(base)),
    }
}

// ======================================================================
//  Calculatrice : opérations et vue bit à bit
// ======================================================================

/// Opération de la calculatrice.
///
/// Les opérations bit à bit viennent en premier : c'est ce qu'on manipule en
/// assembleur, et c'est ce que l'arithmétique décimale cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CalcOp {
    And,
    Or,
    Xor,
    Shl,
    Shr,
    /// Décalage arithmétique : recopie le bit de signe.
    Sar,
    Add,
    Sub,
    Mul,
    /// Division entière ; division par zéro neutralisée.
    Div,
    /// Reste ; modulo par zéro neutralisé.
    Rem,
}

impl CalcOp {
    /// Toutes les opérations, bit à bit d'abord.
    pub(crate) const ALL: [CalcOp; 11] = [
        CalcOp::And,
        CalcOp::Or,
        CalcOp::Xor,
        CalcOp::Shl,
        CalcOp::Shr,
        CalcOp::Sar,
        CalcOp::Add,
        CalcOp::Sub,
        CalcOp::Mul,
        CalcOp::Div,
        CalcOp::Rem,
    ];

    /// Symbole affiché sur le bouton.
    pub(crate) fn symbol(self) -> &'static str {
        match self {
            CalcOp::And => "AND",
            CalcOp::Or => "OR",
            CalcOp::Xor => "XOR",
            CalcOp::Shl => "<<",
            CalcOp::Shr => ">>",
            CalcOp::Sar => ">>>",
            CalcOp::Add => "+",
            CalcOp::Sub => "−",
            CalcOp::Mul => "×",
            CalcOp::Div => "÷",
            CalcOp::Rem => "mod",
        }
    }

    /// Vrai pour les opérations qui travaillent sur les bits.
    pub(crate) fn is_bitwise(self) -> bool {
        matches!(
            self,
            CalcOp::And | CalcOp::Or | CalcOp::Xor | CalcOp::Shl | CalcOp::Shr | CalcOp::Sar
        )
    }

    /// Instruction x86-64 correspondante, pour faire le lien avec le code.
    pub(crate) fn mnemonic(self) -> &'static str {
        match self {
            CalcOp::And => "and",
            CalcOp::Or => "or",
            CalcOp::Xor => "xor",
            CalcOp::Shl => "shl",
            CalcOp::Shr => "shr",
            CalcOp::Sar => "sar",
            CalcOp::Add => "add",
            CalcOp::Sub => "sub",
            CalcOp::Mul => "imul",
            CalcOp::Div => "idiv",
            CalcOp::Rem => "idiv",
        }
    }

    /// Applique l'opération.
    ///
    /// Les décalages sont bornés à 63 : au-delà, le processeur x86 ne prend que
    /// les 6 bits de poids faible du compteur, et Rust paniquerait. Diviser par
    /// zéro renvoie `None` plutôt que d'interrompre.
    pub(crate) fn apply(self, a: i64, b: i64) -> Option<i64> {
        let shift = (b as u64 & 63) as u32;
        Some(match self {
            CalcOp::And => a & b,
            CalcOp::Or => a | b,
            CalcOp::Xor => a ^ b,
            CalcOp::Shl => ((a as u64) << shift) as i64,
            CalcOp::Shr => ((a as u64) >> shift) as i64,
            CalcOp::Sar => a >> shift,
            CalcOp::Add => a.wrapping_add(b),
            CalcOp::Sub => a.wrapping_sub(b),
            CalcOp::Mul => a.wrapping_mul(b),
            CalcOp::Div => {
                if b == 0 {
                    return None;
                }
                a.wrapping_div(b)
            }
            CalcOp::Rem => {
                if b == 0 {
                    return None;
                }
                a.wrapping_rem(b)
            }
        })
    }
}

/// Nombre d'octets nécessaires pour représenter la valeur, arrondi à 1, 2, 4
/// ou 8 — comme les tailles d'opérande du processeur.
///
/// Afficher 64 bits pour la valeur 5 noierait l'information ; en montrer 8 pour
/// une adresse la tronquerait. On suit donc la taille naturelle.
pub(super) fn calc_width_bytes(v: i64) -> usize {
    let u = v as u64;
    if u <= 0xFF {
        1
    } else if u <= 0xFFFF {
        2
    } else if u <= 0xFFFF_FFFF {
        4
    } else {
        8
    }
}

/// Découpe une valeur en octets, du poids FORT au poids faible, sur `bytes`
/// octets. C'est l'ordre de lecture d'un nombre, pas celui de la mémoire.
pub(super) fn calc_bytes_of(v: i64, bytes: usize) -> Vec<u8> {
    let u = v as u64;
    (0..bytes).rev().map(|i| ((u >> (i * 8)) & 0xFF) as u8).collect()
}

/// Les huit bits d'un octet, du poids fort au poids faible.
pub(super) fn calc_bits_of(byte: u8) -> [bool; 8] {
    let mut out = [false; 8];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = (byte >> (7 - i)) & 1 == 1;
    }
    out
}

/// Bascule le bit de rang `bit` (0 = poids faible) d'une valeur.
pub(super) fn calc_toggle_bit(v: i64, bit: u32) -> i64 {
    ((v as u64) ^ (1u64 << bit)) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    

    // ---------- Calculatrice ----------

    /// Les opérations bit à bit sont la raison d'être de cette calculatrice :
    /// elles doivent donner exactement ce que donnerait le processeur.
    #[test]
    fn bitwise_operations_match_the_cpu() {
        assert_eq!(CalcOp::And.apply(0b1100, 0b1010), Some(0b1000));
        assert_eq!(CalcOp::Or.apply(0b1100, 0b1010), Some(0b1110));
        assert_eq!(CalcOp::Xor.apply(0b1100, 0b1010), Some(0b0110));
        assert_eq!(CalcOp::Shl.apply(1, 8), Some(256));
        assert_eq!(CalcOp::Shr.apply(256, 8), Some(1));
    }

    /// shr et sar diffèrent sur les négatifs — c'est tout leur intérêt.
    #[test]
    fn logical_and_arithmetic_shifts_differ_on_negatives() {
        let neg = -8i64;
        assert_eq!(CalcOp::Sar.apply(neg, 1), Some(-4), "sar préserve le signe");
        let logical = CalcOp::Shr.apply(neg, 1).unwrap();
        assert!(logical > 0, "shr insère des zéros : -8 devient un grand positif");
        assert_eq!(logical as u64, (neg as u64) >> 1);
    }

    /// Un décalage de 64 ou plus ne doit pas paniquer : le processeur ne garde
    /// que les 6 bits de poids faible du compteur.
    #[test]
    fn shifts_are_masked_like_the_hardware() {
        assert_eq!(CalcOp::Shl.apply(1, 64), Some(1), "64 & 63 = 0");
        assert_eq!(CalcOp::Shl.apply(1, 65), Some(2), "65 & 63 = 1");
        assert_eq!(CalcOp::Shl.apply(1, -1), Some(CalcOp::Shl.apply(1, 63).unwrap()));
    }

    /// Division par zéro : on renvoie None au lieu de laisser paniquer.
    #[test]
    fn division_by_zero_is_neutralised() {
        assert_eq!(CalcOp::Div.apply(10, 0), None);
        assert_eq!(CalcOp::Rem.apply(10, 0), None);
        assert_eq!(CalcOp::Div.apply(17, 5), Some(3));
        assert_eq!(CalcOp::Rem.apply(17, 5), Some(2));
        // Débordement du cas limite : -MIN / -1 déborde en Rust.
        assert_eq!(CalcOp::Div.apply(i64::MIN, -1), Some(i64::MIN), "sans panique");
    }

    #[test]
    fn every_operation_is_labelled_and_mapped_to_an_instruction() {
        for op in CalcOp::ALL {
            assert!(!op.symbol().is_empty(), "{op:?} sans symbole");
            assert!(!op.mnemonic().is_empty(), "{op:?} sans instruction");
        }
        let bitwise = CalcOp::ALL.iter().filter(|o| o.is_bitwise()).count();
        assert_eq!(bitwise, 6, "six opérations bit à bit");
    }

    /// La largeur suit la taille naturelle de la valeur : montrer 64 bits pour
    /// « 5 » noierait l'information.
    #[test]
    fn width_follows_the_natural_operand_size() {
        assert_eq!(calc_width_bytes(0), 1);
        assert_eq!(calc_width_bytes(255), 1);
        assert_eq!(calc_width_bytes(256), 2);
        assert_eq!(calc_width_bytes(0xFFFF), 2);
        assert_eq!(calc_width_bytes(0x1_0000), 4);
        assert_eq!(calc_width_bytes(0xFFFF_FFFF), 4);
        assert_eq!(calc_width_bytes(0x1_0000_0000), 8);
        assert_eq!(calc_width_bytes(-1), 8, "un négatif occupe tous les bits");
    }

    /// Les octets sortent du poids fort au poids faible : c'est l'ordre de
    /// LECTURE d'un nombre, pas celui de la mémoire.
    #[test]
    fn bytes_come_out_most_significant_first() {
        assert_eq!(calc_bytes_of(0x1234, 2), vec![0x12, 0x34]);
        assert_eq!(calc_bytes_of(0xDEADBEEF, 4), vec![0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(calc_bytes_of(0xFF, 1), vec![0xFF]);
        assert_eq!(calc_bytes_of(0, 8).len(), 8);
    }

    #[test]
    fn bits_come_out_most_significant_first() {
        assert_eq!(
            calc_bits_of(0b1010_0001),
            [true, false, true, false, false, false, false, true]
        );
        assert_eq!(calc_bits_of(0), [false; 8]);
        assert_eq!(calc_bits_of(0xFF), [true; 8]);
    }

    /// Cliquer un bit doit le basculer, et deux clics revenir au départ.
    #[test]
    fn toggling_a_bit_is_its_own_inverse() {
        assert_eq!(calc_toggle_bit(0, 0), 1);
        assert_eq!(calc_toggle_bit(0, 7), 128);
        assert_eq!(calc_toggle_bit(0b1111, 1), 0b1101);
        let v = 0x1234i64;
        assert_eq!(calc_toggle_bit(calc_toggle_bit(v, 5), 5), v);
        // Le bit de signe se bascule aussi, sans déborder.
        assert_eq!(calc_toggle_bit(0, 63), i64::MIN);
    }

    #[test]
    fn calc_parse_reads_each_base() {
        assert_eq!(calc_parse("101", 2), Some(0b101));
        assert_eq!(calc_parse("777", 8), Some(0o777));
        assert_eq!(calc_parse("42", 10), Some(42));
        assert_eq!(calc_parse("dead", 16), Some(0xDEAD));
        assert_eq!(calc_parse("  ff  ", 16), Some(0xFF), "espaces tolérés");
        assert_eq!(calc_parse("", 10), None, "vide → None");
        assert_eq!(calc_parse("-42", 10), Some(-42), "décimal négatif supporté");
    }

    /// La base ASCII lit du texte : chaque caractère vaut son code, le premier
    /// caractère occupant les poids forts — l'ordre de lecture, pas celui de
    /// la mémoire.
    #[test]
    fn ascii_reads_text_as_its_codes() {
        assert_eq!(calc_parse("A", CALC_BASE_ASCII), Some(0x41));
        assert_eq!(calc_parse("Hi", CALC_BASE_ASCII), Some(0x4869));
        assert_eq!(calc_parse("", CALC_BASE_ASCII), None, "vide → None");
        // L'espace est un caractère (0x20), pas du remplissage à ignorer.
        assert_eq!(calc_parse(" ", CALC_BASE_ASCII), Some(0x20));
        assert_eq!(calc_parse("A B", CALC_BASE_ASCII), Some(0x41_20_42));
        // Huit octets tiennent dans un registre, neuf non.
        assert_eq!(calc_parse("12345678", CALC_BASE_ASCII), Some(0x3132333435363738));
        assert_eq!(calc_parse("123456789", CALC_BASE_ASCII), None);
    }

    /// Sans échappements, ni l'octet nul ni le saut de ligne ne seraient
    /// saisissables — or ce sont ceux qui comptent en assembleur.
    #[test]
    fn ascii_understands_escapes() {
        assert_eq!(calc_parse("\\0", CALC_BASE_ASCII), Some(0));
        assert_eq!(calc_parse("\\n", CALC_BASE_ASCII), Some(0x0A));
        assert_eq!(calc_parse("\\t", CALC_BASE_ASCII), Some(0x09));
        assert_eq!(calc_parse("\\r", CALC_BASE_ASCII), Some(0x0D));
        assert_eq!(calc_parse("\\\\", CALC_BASE_ASCII), Some(0x5C));
        assert_eq!(calc_parse("\\xFF", CALC_BASE_ASCII), Some(0xFF));
        assert_eq!(calc_parse("Hi\\n", CALC_BASE_ASCII), Some(0x48690A));
        // Séquences en cours de frappe : elles ne produisent rien.
        assert_eq!(calc_parse("\\x", CALC_BASE_ASCII), None);
        assert_eq!(calc_parse("\\x4", CALC_BASE_ASCII), None);
        assert_eq!(calc_parse("\\", CALC_BASE_ASCII), None);
    }

    /// Ce que la calculatrice affiche doit pouvoir être relu tel quel.
    #[test]
    fn ascii_formatting_roundtrips() {
        for text in ["A", "Hi!", " ", "\\0", "\\n", "\\\\", "\\xFF", "ab\\0"] {
            let v = calc_parse(text, CALC_BASE_ASCII).unwrap();
            let shown = calc_format_bare(v, CALC_BASE_ASCII);
            assert_eq!(calc_parse(&shown, CALC_BASE_ASCII), Some(v), "aller-retour de {text}");
        }
        assert_eq!(calc_format(0x41, CALC_BASE_ASCII), "'A'");
        assert_eq!(calc_format(0x4869, CALC_BASE_ASCII), "'Hi'");
        assert_eq!(calc_format(0, CALC_BASE_ASCII), "'\\0'");
        assert_eq!(calc_format(0x7F, CALC_BASE_ASCII), "'\\x7F'", "non imprimable échappé");
    }

    /// Le filtre de saisie : chaque base ne laisse passer que ce qu'elle sait
    /// lire, et l'ASCII s'arrête à la largeur d'un registre.
    #[test]
    fn sanitize_keeps_only_what_the_base_can_read() {
        let mut s = "12zz34".to_string();
        calc_sanitize(&mut s, 10);
        assert_eq!(s, "1234");
        let mut s = "-4x2".to_string();
        calc_sanitize(&mut s, 10);
        assert_eq!(s, "-42", "le signe survit au filtre");
        let mut s = "dezzad".to_string();
        calc_sanitize(&mut s, 16);
        assert_eq!(s, "dead");
        let mut s = "1o0l1".to_string();
        calc_sanitize(&mut s, 2);
        assert_eq!(s, "101");

        let mut s = "Hé là !".to_string();
        calc_sanitize(&mut s, CALC_BASE_ASCII);
        assert_eq!(s, "H l !", "un caractère hors ASCII n'a pas de code sur un octet");
        let mut s = "123456789abc".to_string();
        calc_sanitize(&mut s, CALC_BASE_ASCII);
        assert_eq!(calc_ascii_bytes(&s).len(), 8, "borné à 8 octets");
        // Tronquer au milieu d'un `\xNN` ne doit pas boucler sans fin.
        let mut s = "AAAAAAA\\xFF".to_string();
        calc_sanitize(&mut s, CALC_BASE_ASCII);
        assert!(calc_ascii_bytes(&s).len() <= 8);
    }

    /// Les opérations gardent leur sens sur du texte : mettre le bit 5 à zéro
    /// passe une minuscule en majuscule, c'est l'exercice classique.
    #[test]
    fn ascii_feeds_the_usual_bit_tricks() {
        let a = calc_parse("a", CALC_BASE_ASCII).unwrap();
        let mask = calc_parse("\\xDF", CALC_BASE_ASCII).unwrap();
        let r = CalcOp::And.apply(a, mask).unwrap();
        assert_eq!(calc_format(r, CALC_BASE_ASCII), "'A'");
    }

    #[test]
    fn calc_format_roundtrips_with_prefix() {
        assert_eq!(calc_format(255, 10), "255");
        assert_eq!(calc_format(255, 16), "0xFF");
        assert_eq!(calc_format(255, 8), "0o377");
        assert_eq!(calc_format(255, 2), "0b11111111");
        // Aller-retour parse ∘ format (sans le préfixe, retiré par le filtre UI).
        let v = 0xCAFE;
        assert_eq!(calc_parse("CAFE", 16), Some(v));
        assert_eq!(calc_format(v, 16), "0xCAFE");
    }

    /// Le clignotement pédagogique doit vraiment osciller (pas un simple fondu)
    #[test]
    fn parse_hex_bytes_accepts_spaced_and_contiguous() {
        assert_eq!(parse_hex_bytes("48 65 6C"), Some(vec![0x48, 0x65, 0x6C]));
        assert_eq!(parse_hex_bytes("48656C"), Some(vec![0x48, 0x65, 0x6C]));
        assert_eq!(parse_hex_bytes("4"), None, "longueur impaire invalide");
        assert_eq!(parse_hex_bytes("zz"), None, "non-hexa invalide");
    }
}
