//! Internationalisation minimale : deux langues (français par défaut, anglais),
//! choisies dans les Réglages. Les chaînes sont fournies en ligne aux points
//! d'usage via `tr(lang, fr, en)` — pas de table de clés à maintenir à part.

/// Langue de l'interface.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Fr,
    En,
}

impl Lang {
    /// Clé de persistance dans le fichier de réglages.
    pub fn key(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
        }
    }

    /// Reconstruit depuis la clé persistée (français par défaut).
    pub fn from_key(s: &str) -> Lang {
        match s {
            "en" => Lang::En,
            _ => Lang::Fr,
        }
    }
}

/// Renvoie la variante correspondant à `lang`.
pub fn tr(lang: Lang, fr: &'static str, en: &'static str) -> &'static str {
    match lang {
        Lang::Fr => fr,
        Lang::En => en,
    }
}
