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

#[cfg(test)]
mod tests {
    use super::*;
    
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
