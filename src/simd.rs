//! Lecture des registres vectoriels et flottants : XMM, pile x87, MXCSR.
//!
//! Un registre XMM ne porte aucun type. Les mêmes seize octets sont deux
//! `double` pour `addpd`, quatre `float` pour `addps`, quatre entiers pour
//! `paddd` et seize octets pour `pshufb` — c'est l'instruction qui décide, pas
//! le registre. Montrer un XMM en hexadécimal seul revient donc à ne rien
//! montrer : l'élève qui vient d'écrire `addsd xmm0, xmm1` veut lire `3.5`, pas
//! `400C000000000000`. Ce module fournit les lectures possibles ; l'interface
//! laisse choisir laquelle, en gardant l'hexadécimal comme filet.
//!
//! Il ne touche jamais au processus tracé : il ne transforme que des octets,
//! ce qui le rend testable sans `ptrace`.

use crate::i18n::{self, Lang};

/// Les façons de lire les seize octets d'un registre XMM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmmView {
    /// 2 × `double` (`addsd`, `addpd`, `cvtsi2sd`…).
    F64,
    /// 4 × `float` (`addss`, `addps`, `cvtsi2ss`…).
    F32,
    /// 2 × entier 64 bits signé.
    I64,
    /// 4 × entier 32 bits signé (`paddd`, `pmulld`…).
    I32,
    /// 8 × entier 16 bits signé (`paddw`…).
    I16,
    /// 16 × octet non signé (`paddb`, `pshufb`, chaînes SSE 4.2…).
    U8,
    /// Les 128 bits bruts, sans interprétation.
    Hex,
}

impl XmmView {
    /// Toutes les vues, dans l'ordre du sélecteur.
    pub const ALL: [XmmView; 7] = [
        XmmView::F64,
        XmmView::F32,
        XmmView::I64,
        XmmView::I32,
        XmmView::I16,
        XmmView::U8,
        XmmView::Hex,
    ];

    /// Étiquette courte du sélecteur — volontairement dans la notation des
    /// suffixes d'instructions (`pd`, `ps`, `dq`…), que l'élève retrouve
    /// ensuite dans son code.
    pub fn label(self) -> &'static str {
        match self {
            XmmView::F64 => "2 × f64",
            XmmView::F32 => "4 × f32",
            XmmView::I64 => "2 × i64",
            XmmView::I32 => "4 × i32",
            XmmView::I16 => "8 × i16",
            XmmView::U8 => "16 × u8",
            XmmView::Hex => "hex",
        }
    }

    /// Ce que la vue signifie, et les instructions qui la produisent.
    pub fn hint(self, lang: Lang) -> &'static str {
        let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        match self {
            XmmView::F64 => t(
                "Deux flottants double précision — ce que lisent addsd, addpd, mulsd, cvtsi2sd.",
                "Two double-precision floats — what addsd, addpd, mulsd, cvtsi2sd read.",
                "Dos flotantes de doble precisión — lo que leen addsd, addpd, mulsd, cvtsi2sd.",
            ),
            XmmView::F32 => t(
                "Quatre flottants simple précision — ce que lisent addss, addps, mulps.",
                "Four single-precision floats — what addss, addps, mulps read.",
                "Cuatro flotantes de precisión simple — lo que leen addss, addps, mulps.",
            ),
            XmmView::I64 => t(
                "Deux entiers 64 bits signés — paddq, movq.",
                "Two signed 64-bit integers — paddq, movq.",
                "Dos enteros de 64 bits con signo — paddq, movq.",
            ),
            XmmView::I32 => t(
                "Quatre entiers 32 bits signés — paddd, pmulld, la vue SIMD la plus courante.",
                "Four signed 32-bit integers — paddd, pmulld, the most common SIMD view.",
                "Cuatro enteros de 32 bits con signo — paddd, pmulld, la vista SIMD más común.",
            ),
            XmmView::I16 => t(
                "Huit entiers 16 bits signés — paddw, pmullw.",
                "Eight signed 16-bit integers — paddw, pmullw.",
                "Ocho enteros de 16 bits con signo — paddw, pmullw.",
            ),
            XmmView::U8 => t(
                "Seize octets — paddb, pshufb, et les instructions de chaînes.",
                "Sixteen bytes — paddb, pshufb, and the string instructions.",
                "Dieciséis bytes — paddb, pshufb y las instrucciones de cadenas.",
            ),
            XmmView::Hex => t(
                "Les 128 bits bruts, sans interprétation.",
                "The raw 128 bits, uninterpreted.",
                "Los 128 bits en bruto, sin interpretación.",
            ),
        }
    }
}

/// Découpe un registre XMM en cases, de la plus basse à la plus haute.
///
/// L'ordre suit celui de la mémoire (petit-boutiste) : la première case rendue
/// est celle des bits de poids faible, c'est-à-dire l'élément que touchent les
/// formes scalaires (`addsd`, `addss`) et celui que `movq rax, xmm0` redescend.
pub fn lanes(value: u128, view: XmmView) -> Vec<String> {
    let b = value.to_le_bytes();
    let chunk = |n: usize, i: usize| -> u64 {
        let mut w = 0u64;
        for k in 0..n {
            w |= (b[i * n + k] as u64) << (8 * k);
        }
        w
    };
    match view {
        XmmView::F64 => (0..2).map(|i| fmt_f64(f64::from_bits(chunk(8, i)))).collect(),
        XmmView::F32 => (0..4)
            .map(|i| fmt_f64(f32::from_bits(chunk(4, i) as u32) as f64))
            .collect(),
        XmmView::I64 => (0..2).map(|i| (chunk(8, i) as i64).to_string()).collect(),
        XmmView::I32 => (0..4).map(|i| (chunk(4, i) as u32 as i32).to_string()).collect(),
        XmmView::I16 => (0..8).map(|i| (chunk(2, i) as u16 as i16).to_string()).collect(),
        XmmView::U8 => b.iter().map(|x| format!("{x:02X}")).collect(),
        XmmView::Hex => vec![format!("{value:032X}")],
    }
}

/// Affiche un flottant sans le bruit de `{:?}` : les valeurs rondes restent
/// rondes (`3` et non `3.0000000000000004`), les autres gardent assez de
/// chiffres pour qu'une erreur d'arrondi reste visible, et les cas spéciaux
/// (NaN, ±∞, zéro négatif) se nomment au lieu de s'écrire en chiffres.
pub fn fmt_f64(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v > 0.0 { "+∞" } else { "-∞" }.to_string();
    }
    if v == 0.0 {
        // Le zéro négatif existe et se distingue : un résultat qui bascule de
        // +0 à -0 raconte quelque chose sur le calcul qui l'a produit.
        return if v.is_sign_negative() { "-0" } else { "0" }.to_string();
    }
    let a = v.abs();
    if !(1e-4..1e16).contains(&a) {
        format!("{v:e}")
    } else {
        let s = format!("{v:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Convertit un registre x87 (80 bits : signe, exposant 15 bits, mantisse
/// explicite 64 bits) vers le `f64` le plus proche, pour l'afficher.
///
/// Le format n'a pas de bit implicite, contrairement à `f64` : le bit de poids
/// fort de la mantisse est stocké, et c'est lui qui distingue un nombre normal
/// d'une valeur dénormale ou invalide. La conversion perd donc de la précision
/// — 64 bits de mantisse contre 52 — mais elle rend une valeur lisible, ce que
/// dix octets bruts ne font pas.
pub fn st_to_f64(raw: [u8; 10]) -> f64 {
    let mantissa = u64::from_le_bytes(raw[0..8].try_into().expect("8 octets"));
    let se = u16::from_le_bytes(raw[8..10].try_into().expect("2 octets"));
    let sign = if se & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = (se & 0x7FFF) as i32;
    match exp {
        0 if mantissa == 0 => sign * 0.0,
        // Dénormaux : exposant plancher, pas de bit implicite à ajouter.
        0 => sign * (mantissa as f64) * 2f64.powi(-16382 - 63),
        0x7FFF if mantissa << 1 == 0 => sign * f64::INFINITY,
        0x7FFF => f64::NAN,
        _ => sign * (mantissa as f64) * 2f64.powi(exp - 16383 - 63),
    }
}

/// Mode d'arrondi codé sur deux bits, commun à MXCSR (bits 13-14) et au mot de
/// contrôle x87 (bits 10-11).
pub fn rounding_mode(bits: u8, lang: Lang) -> &'static str {
    let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
    match bits & 0b11 {
        0 => t("au plus proche (pair)", "to nearest (even)", "al más cercano (par)"),
        1 => t("vers -∞", "toward -∞", "hacia -∞"),
        2 => t("vers +∞", "toward +∞", "hacia +∞"),
        _ => t("vers zéro (troncature)", "toward zero (truncate)", "hacia cero (truncamiento)"),
    }
}

/// Drapeau d'exception flottante : sigle, état, et ce qu'il signifie.
pub struct FpFlag {
    pub name: &'static str,
    pub set: bool,
    /// Explication en clair, dans la langue de l'interface.
    pub meaning: &'static str,
}

/// Les six exceptions levées, lues dans MXCSR (bits 0-5) ou dans le mot d'état
/// x87 (mêmes six bits, même ordre).
///
/// Un drapeau levé est *collant* : il reste à 1 jusqu'à ce qu'on l'efface. Il
/// dit qu'une exception s'est produite depuis le début du programme, pas que la
/// dernière instruction l'a produite — nuance qui explique bien des surprises.
pub fn exception_flags(bits: u16, lang: Lang) -> Vec<FpFlag> {
    let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
    let table: [(&'static str, &'static str); 6] = [
        ("IE", t("opération invalide (0/0, ∞-∞, racine d'un négatif)", "invalid operation (0/0, ∞-∞, sqrt of a negative)", "operación inválida (0/0, ∞-∞, raíz de un negativo)")),
        ("DE", t("opérande dénormal", "denormal operand", "operando desnormalizado")),
        ("ZE", t("division par zéro", "division by zero", "división por cero")),
        ("OE", t("dépassement de capacité (résultat trop grand)", "overflow (result too large)", "desbordamiento (resultado demasiado grande)")),
        ("UE", t("soupassement (résultat trop petit)", "underflow (result too small)", "subdesbordamiento (resultado demasiado pequeño)")),
        ("PE", t("précision : le résultat exact n'est pas représentable", "precision: the exact result is not representable", "precisión: el resultado exacto no es representable")),
    ];
    table
        .into_iter()
        .enumerate()
        .map(|(i, (name, meaning))| FpFlag {
            name,
            set: bits & (1 << i) != 0,
            meaning,
        })
        .collect()
}

/// Vrai si le registre XMM ne contient que des zéros — l'immense majorité des
/// cas, que l'interface propose de masquer pour ne montrer que ce qui travaille.
pub fn is_zero(v: u128) -> bool {
    v == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La case basse est celle que touchent les instructions scalaires : c'est
    /// elle qui doit sortir en premier, sinon `addsd xmm0, xmm1` semblerait
    /// écrire dans la mauvaise moitié du registre.
    #[test]
    fn low_lane_comes_first() {
        // 3.5 en double dans les 64 bits bas, 0 dans les hauts.
        let v = 3.5f64.to_bits() as u128;
        assert_eq!(lanes(v, XmmView::F64), vec!["3.5", "0"]);
    }

    /// Les quatre entiers de la leçon SIMD (`paddd`) se relisent dans l'ordre.
    #[test]
    fn four_int32_lanes_read_in_order() {
        let mut v: u128 = 0;
        for (i, x) in [1i32, 2, 3, -4].iter().enumerate() {
            v |= ((*x as u32) as u128) << (32 * i);
        }
        assert_eq!(lanes(v, XmmView::I32), vec!["1", "2", "3", "-4"]);
    }

    /// Un octet de chaque, dans l'ordre mémoire.
    #[test]
    fn sixteen_bytes_follow_memory_order() {
        let v = u128::from_le_bytes([0xDE, 0xAD, 0xBE, 0xEF, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let l = lanes(v, XmmView::U8);
        assert_eq!(&l[..4], ["DE", "AD", "BE", "EF"]);
        assert_eq!(lanes(v, XmmView::Hex)[0], "000000000000000000000000EFBEADDE");
    }

    /// Les valeurs rondes s'affichent rondes, les cas spéciaux se nomment.
    #[test]
    fn float_formatting_stays_readable() {
        assert_eq!(fmt_f64(3.0), "3");
        assert_eq!(fmt_f64(0.5), "0.5");
        assert_eq!(fmt_f64(-0.0), "-0");
        assert_eq!(fmt_f64(f64::NAN), "NaN");
        assert_eq!(fmt_f64(f64::NEG_INFINITY), "-∞");
        assert_eq!(fmt_f64(1.0 / 3.0), "0.333333");
    }

    /// Le format 80 bits se relit : mantisse explicite, biais 16383.
    #[test]
    fn x87_long_double_converts() {
        // 1.0 : mantisse = 1 << 63, exposant = 16383.
        let mut raw = [0u8; 10];
        raw[0..8].copy_from_slice(&(1u64 << 63).to_le_bytes());
        raw[8..10].copy_from_slice(&16383u16.to_le_bytes());
        assert_eq!(st_to_f64(raw), 1.0);

        // -2.5 : mantisse = 1.25 × 2^63, exposant = 16384, signe posé.
        let mut raw = [0u8; 10];
        raw[0..8].copy_from_slice(&((1u64 << 63) | (1u64 << 61)).to_le_bytes());
        raw[8..10].copy_from_slice(&(0x8000u16 | 16384).to_le_bytes());
        assert_eq!(st_to_f64(raw), -2.5);

        // Registre vide (que des zéros) : 0, et non un NaN inventé.
        assert_eq!(st_to_f64([0u8; 10]), 0.0);
    }

    /// Les six exceptions sortent dans l'ordre du manuel, et chacune s'explique
    /// dans les trois langues — un sigle seul n'apprend rien.
    #[test]
    fn exception_flags_are_named_and_explained() {
        for lang in [Lang::Fr, Lang::En, Lang::Es] {
            let f = exception_flags(0b000101, lang);
            assert_eq!(f.len(), 6);
            assert_eq!(f[0].name, "IE");
            assert!(f[0].set, "IE levé");
            assert!(!f[1].set, "DE non levé");
            assert!(f[2].set, "ZE levé");
            assert!(f.iter().all(|x| !x.meaning.is_empty()), "chaque drapeau s'explique");
        }
    }

    /// Les quatre modes d'arrondi ont un libellé dans chaque langue.
    #[test]
    fn rounding_modes_translate() {
        for lang in [Lang::Fr, Lang::En, Lang::Es] {
            let all: Vec<&str> = (0..4).map(|b| rounding_mode(b, lang)).collect();
            assert_eq!(all.len(), 4);
            assert!(all.iter().all(|s| !s.is_empty()));
        }
    }
}
