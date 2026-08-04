//! Internationalisation minimale : trois langues (français par défaut, anglais, espagnol),
//! choisies dans les Réglages. Les chaînes sont fournies en ligne aux points
//! d'usage via `tr3(lang, fr, en, es)` — pas de table de clés à maintenir à part.

/// Langue de l'interface.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Fr,
    En,
    Es,
}

impl Lang {
    /// Clé de persistance dans le fichier de réglages.
    pub fn key(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
            Lang::Es => "es",
        }
    }

    /// Reconstruit depuis la clé persistée (français par défaut).
    pub fn from_key(s: &str) -> Lang {
        match s {
            "en" => Lang::En,
            "es" => Lang::Es,
            _ => Lang::Fr,
        }
    }
}

/// Renvoie la variante correspondant à `lang` (2 langues — espagnol → anglais en fallback).
pub fn tr(lang: Lang, fr: &'static str, en: &'static str) -> &'static str {
    match lang {
        Lang::Fr => fr,
        Lang::En | Lang::Es => en,
    }
}

/// Renvoie la variante correspondant à `lang` (3 langues complètes).
pub fn tr3(lang: Lang, fr: &'static str, en: &'static str, es: &'static str) -> &'static str {
    match lang {
        Lang::Fr => fr,
        Lang::En => en,
        Lang::Es => es,
    }
}
