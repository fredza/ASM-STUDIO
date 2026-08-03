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

/// Analyse une valeur dans la base donnée (2, 8, 10 ou 16).
/// Base 10 : signé (`i64`), supporte le signe `-`. Autres bases : bit-pattern `u64` casté.
/// Renvoie `None` si vide ou hors plage.
pub(super) fn calc_parse(s: &str, base: u32) -> Option<i64> {
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
pub(super) fn calc_format(v: i64, base: u32) -> String {
    match base {
        16 => format!("0x{:X}", v as u64),
        8 => format!("0o{:o}", v as u64),
        2 => format!("0b{:b}", v as u64),
        _ => format!("{v}"),
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
