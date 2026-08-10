//! Thèmes de couleurs : un catalogue, pas deux constantes.
//!
//! Jusqu'ici l'application connaissait « sombre » et « clair », l'un et l'autre
//! écrits en dur — une vingtaine de `const Color32` dans `app/mod.rs` pour
//! l'interface, deux `Palette` dans `syntax.rs` pour le code. Ajouter un thème
//! demandait de retoucher les deux, et le thème clair traînait des couleurs
//! pensées pour le fond sombre (la pulsation « CPU vivant » y était invisible).
//!
//! Ici, un thème est **une donnée** : une entrée de [`THEMES`]. Il porte tout ce
//! qui change d'un thème à l'autre — surfaces, textes, accents fonctionnels,
//! coloration syntaxique — et rien d'autre. En ajouter un consiste à écrire une
//! fonction qui renvoie un [`Theme`] et à l'ajouter à la liste : ni interface ni
//! réglage ni palette de commandes à toucher, tous parcourent [`THEMES`].
//!
//! Le thème courant est un **index atomique** dans ce catalogue. Les couleurs
//! sont lues à chaque image, depuis le code de rendu, sans avoir à faire
//! descendre une référence dans chaque fonction d'affichage — et sans verrou sur
//! un chemin parcouru des milliers de fois par image.

use eframe::egui::Color32;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Couleurs de l'interface (tout sauf la coloration du code).
///
/// Les noms disent l'**usage**, pas la teinte : `error` reste `error` même dans
/// un thème où il tire sur le rose. C'est ce qui permet d'écrire un thème sans
/// relire le code qui s'en sert.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    // --- Surfaces, du fond vers le premier plan ---
    /// Fond des panneaux (`Visuals::panel_fill`).
    pub bg: Color32,
    /// Fond des fenêtres flottantes et des menus.
    pub window: Color32,
    /// Fond le plus contrasté : éditeur, champs de saisie (`extreme_bg_color`).
    pub extreme: Color32,
    /// Fond discret : cartes, lignes alternées (`faint_bg_color`).
    pub faint: Color32,
    /// Fond des widgets au repos, puis au survol et à l'appui.
    pub surface: Color32,
    pub surface_hover: Color32,
    pub surface_active: Color32,
    /// Trait des bordures et séparateurs.
    pub border: Color32,

    // --- Textes ---
    /// Texte courant.
    pub text: Color32,
    /// Texte mis en avant (`strong()`).
    pub text_strong: Color32,
    /// Titres de section et libellés secondaires.
    pub header: Color32,

    // --- Accents fonctionnels ---
    /// Accent principal : liens, sélection, repère de la ligne courante.
    pub accent: Color32,
    /// Accent d'action : boutons Lancer / Pas à pas.
    pub action: Color32,
    /// Valeur qui vient de changer.
    pub changed: Color32,
    /// Pic de la pulsation « CPU vivant » (fondu vers [`Palette::changed`]).
    /// S'éclaircit sur un thème sombre, s'assombrit sur un thème clair — sans
    /// quoi la pulsation ne se voit pas.
    pub flash: Color32,
    /// Drapeau à 1, empilement, réussite.
    pub ok: Color32,
    /// Drapeau à 0, valeur éteinte.
    pub off: Color32,
    /// Erreur, condition fausse, point d'arrêt.
    pub error: Color32,
    /// Dépilement, avertissement.
    pub warn: Color32,

    // --- Listes de code (désassemblage, vidage mémoire) ---
    pub addr: Color32,
    pub bytes: Color32,
    pub mnemonic: Color32,
    /// Numéros de ligne de l'éditeur.
    pub gutter: Color32,
    /// Fond de la ligne où pointe RIP.
    pub rip_row: Color32,
    /// Fond d'une ligne sélectionnée ou survolée.
    pub sel_row: Color32,
}

/// Coloration syntaxique NASM (voir [`crate::syntax`]).
#[derive(Debug, Clone, Copy)]
pub struct Syntax {
    pub comment: Color32,
    pub mnemonic: Color32,
    pub register: Color32,
    pub number: Color32,
    pub directive: Color32,
    pub label: Color32,
    pub string: Color32,
    pub text: Color32,
    /// Fond de la ligne courante (RIP) pendant le débogage.
    pub line_bg: Color32,
    /// Fond de la ligne où se trouve le curseur d'édition.
    pub cursor_line_bg: Color32,
    /// Fond des correspondances de recherche, et de la correspondance active.
    pub match_bg: Color32,
    pub match_current_bg: Color32,
    /// Fond de la paire de brackets sous le curseur.
    pub bracket_bg: Color32,
}

/// Un thème complet, désigné par son [`Theme::id`] dans le fichier de réglages.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Identifiant stable, écrit dans `settings.conf`. Ne change jamais.
    pub id: &'static str,
    /// Nom affiché — les thèmes portent des noms propres, on ne les traduit pas.
    pub name: &'static str,
    /// Thème sombre ? Sert aux quelques rendus qui doivent choisir un contraste
    /// (et à résoudre la préférence « Système »).
    pub dark: bool,
    pub ui: Palette,
    pub syntax: Syntax,
}

/// Mélange `a` vers `b` (`t` ∈ [0,1]). Sert à dériver les fonds teintés d'un
/// thème (ligne courante, correspondance de recherche) de sa couleur de base,
/// plutôt que de les écrire à la main pour chaque variante.
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

// ---------------------------------------------------------------------------
// Thèmes maison : ceux d'avant le catalogue, repris à l'identique pour que
// personne ne voie son interface changer en installant cette version.
// ---------------------------------------------------------------------------

fn dark_theme() -> Theme {
    let bg = rgb(0x1E1E22);
    Theme {
        id: "dark",
        name: "Sombre",
        dark: true,
        ui: Palette {
            bg,
            window: rgb(0x25252B),
            extreme: rgb(0x17171B),
            faint: rgb(0x282830),
            surface: rgb(0x2B2B33),
            surface_hover: rgb(0x35353F),
            surface_active: rgb(0x3E3E4A),
            border: rgb(0x3A3A44),
            text: rgb(0xC8CBD2),
            text_strong: rgb(0xF0F1F4),
            header: rgb(0x8A9BB4),
            accent: rgb(0x4C8BF5),
            action: rgb(0xE88A2E),
            changed: rgb(0xF5A623),
            flash: rgb(0xFFF29A),
            ok: rgb(0x5FBF69),
            off: rgb(0x777780),
            error: rgb(0xD95B5B),
            warn: rgb(0xE08A3C),
            addr: rgb(0x7F9CD1),
            bytes: rgb(0x808088),
            mnemonic: rgb(0x6EB4E8),
            gutter: rgb(0x606670),
            rip_row: rgb(0x3A331E),
            sel_row: rgb(0x2E2E38),
        },
        syntax: Syntax {
            // Tons VSCode « Dark+ ».
            comment: rgb(0x6A9955),
            mnemonic: rgb(0x569CD6),
            register: rgb(0x9CDCFE),
            number: rgb(0xB5CEA8),
            directive: rgb(0xC586C0),
            label: rgb(0xDCDCAA),
            string: rgb(0xCE9178),
            text: rgb(0xD4D4D4),
            line_bg: rgb(0x3A331E),
            cursor_line_bg: rgb(0x232329),
            match_bg: rgb(0x515C6A),
            match_current_bg: rgb(0xA87A1E),
            bracket_bg: rgb(0x51510A),
        },
    }
}

fn light_theme() -> Theme {
    Theme {
        id: "light",
        name: "Clair",
        dark: false,
        ui: Palette {
            bg: rgb(0xF4F5F8),
            window: rgb(0xFBFBFD),
            extreme: rgb(0xFFFFFF),
            faint: rgb(0xEAECF1),
            surface: rgb(0xE6E8EE),
            surface_hover: rgb(0xDADDE6),
            surface_active: rgb(0xCBD0DC),
            border: rgb(0xC3C8D4),
            text: rgb(0x1C2028),
            text_strong: rgb(0x000000),
            header: rgb(0x3B4A63),
            accent: rgb(0x1B5EA8),
            action: rgb(0xB45F0A),
            changed: rgb(0xA96A00),
            // Sur fond blanc, un pic clair ne se voit pas : la pulsation
            // s'assombrit au lieu de s'éclaircir.
            flash: rgb(0x5A3400),
            ok: rgb(0x2E8B3A),
            off: rgb(0x8A8A93),
            error: rgb(0xB32424),
            warn: rgb(0xA85A12),
            addr: rgb(0x2A5386),
            bytes: rgb(0x606470),
            mnemonic: rgb(0x1B5EA8),
            gutter: rgb(0x9098A6),
            rip_row: rgb(0xFFEEB0),
            sel_row: rgb(0xD5E2F4),
        },
        syntax: Syntax {
            // Tons VSCode « Light+ ».
            comment: rgb(0x008000),
            mnemonic: rgb(0x0451A5),
            register: rgb(0x0F68A0),
            number: rgb(0x0A6E48),
            directive: rgb(0xAF00DB),
            label: rgb(0x795E26),
            string: rgb(0xA31515),
            text: rgb(0x1C2028),
            line_bg: rgb(0xFFF3C4),
            cursor_line_bg: rgb(0xF2F4FA),
            match_bg: rgb(0xCFE0EA),
            match_current_bg: rgb(0xFFD77A),
            bracket_bg: rgb(0xDDE6A8),
        },
    }
}

// ---------------------------------------------------------------------------
// Catppuccin — https://catppuccin.com (licence MIT)
//
// Quatre déclinaisons d'une même palette, de la plus sombre (Mocha) à la seule
// claire (Latte). Chacune nomme ses vingt-six teintes de la même façon : c'est
// ce qui permet de les décrire par UNE fonction, et non par quatre thèmes
// recopiés. Les rôles suivent le guide de style du projet : mauve pour les
// mots-clés (donc les mnémoniques), rouge pour les variables intrinsèques
// (donc les registres), bleu pour les fonctions (donc les labels), vert pour
// les chaînes, pêche pour les nombres.
// ---------------------------------------------------------------------------

/// Les teintes d'une déclinaison Catppuccin, dans l'ordre du site officiel.
struct Catppuccin {
    id: &'static str,
    name: &'static str,
    dark: bool,
    // Accents.
    pink: Color32,
    mauve: Color32,
    red: Color32,
    peach: Color32,
    yellow: Color32,
    green: Color32,
    teal: Color32,
    sapphire: Color32,
    blue: Color32,
    // Neutres, du plus clair au plus sombre (l'inverse pour Latte).
    text: Color32,
    subtext0: Color32,
    overlay1: Color32,
    overlay0: Color32,
    surface2: Color32,
    surface1: Color32,
    surface0: Color32,
    base: Color32,
    mantle: Color32,
    crust: Color32,
}

impl Catppuccin {
    fn into_theme(self) -> Theme {
        // Vers quoi « éclaircir » : le blanc sur un thème sombre, le noir sur un
        // thème clair. Une seule notion — « plus de contraste avec le fond » —
        // qui évite de dupliquer chaque dérivation.
        let fore = if self.dark { Color32::WHITE } else { Color32::BLACK };
        Theme {
            id: self.id,
            name: self.name,
            dark: self.dark,
            ui: Palette {
                bg: self.base,
                window: self.mantle,
                // L'éditeur est la surface la plus enfoncée : c'est ce qui lui
                // donne sa profondeur face aux panneaux qui l'entourent.
                extreme: self.crust,
                faint: self.surface0,
                surface: self.surface0,
                surface_hover: self.surface1,
                surface_active: self.surface2,
                border: self.surface1,
                text: self.text,
                text_strong: mix(self.text, fore, 0.35),
                header: self.subtext0,
                accent: self.mauve,
                action: self.peach,
                changed: self.yellow,
                flash: mix(self.yellow, fore, 0.55),
                ok: self.green,
                off: self.overlay0,
                error: self.red,
                warn: self.peach,
                addr: self.sapphire,
                bytes: self.overlay1,
                mnemonic: self.blue,
                gutter: self.overlay0,
                rip_row: mix(self.base, self.yellow, 0.20),
                sel_row: self.surface0,
            },
            syntax: Syntax {
                comment: self.overlay1,
                mnemonic: self.mauve,
                register: self.red,
                number: self.peach,
                directive: self.pink,
                label: self.blue,
                string: self.green,
                text: self.text,
                // Les fonds se dérivent de celui de l'ÉDITEUR (`crust`), pas du
                // fond des panneaux : c'est par-dessus lui qu'ils se peignent.
                line_bg: mix(self.crust, self.yellow, 0.22),
                cursor_line_bg: mix(self.crust, self.surface2, 0.35),
                match_bg: mix(self.crust, self.sapphire, 0.30),
                match_current_bg: mix(self.crust, self.peach, 0.45),
                bracket_bg: mix(self.crust, self.teal, 0.35),
            },
        }
    }
}

fn mocha() -> Theme {
    Catppuccin {
        id: "catppuccin-mocha",
        name: "Catppuccin Mocha",
        dark: true,
        pink: rgb(0xF5C2E7),
        mauve: rgb(0xCBA6F7),
        red: rgb(0xF38BA8),
        peach: rgb(0xFAB387),
        yellow: rgb(0xF9E2AF),
        green: rgb(0xA6E3A1),
        teal: rgb(0x94E2D5),
        sapphire: rgb(0x74C7EC),
        blue: rgb(0x89B4FA),
        text: rgb(0xCDD6F4),
        subtext0: rgb(0xA6ADC8),
        overlay1: rgb(0x7F849C),
        overlay0: rgb(0x6C7086),
        surface2: rgb(0x585B70),
        surface1: rgb(0x45475A),
        surface0: rgb(0x313244),
        base: rgb(0x1E1E2E),
        mantle: rgb(0x181825),
        crust: rgb(0x11111B),
    }
    .into_theme()
}

fn macchiato() -> Theme {
    Catppuccin {
        id: "catppuccin-macchiato",
        name: "Catppuccin Macchiato",
        dark: true,
        pink: rgb(0xF5BDE6),
        mauve: rgb(0xC6A0F6),
        red: rgb(0xED8796),
        peach: rgb(0xF5A97F),
        yellow: rgb(0xEED49F),
        green: rgb(0xA6DA95),
        teal: rgb(0x8BD5CA),
        sapphire: rgb(0x7DC4E4),
        blue: rgb(0x8AADF4),
        text: rgb(0xCAD3F5),
        subtext0: rgb(0xA5ADCB),
        overlay1: rgb(0x8087A2),
        overlay0: rgb(0x6E738D),
        surface2: rgb(0x5B6078),
        surface1: rgb(0x494D64),
        surface0: rgb(0x363A4F),
        base: rgb(0x24273A),
        mantle: rgb(0x1E2030),
        crust: rgb(0x181926),
    }
    .into_theme()
}

fn frappe() -> Theme {
    Catppuccin {
        id: "catppuccin-frappe",
        name: "Catppuccin Frappé",
        dark: true,
        pink: rgb(0xF4B8E4),
        mauve: rgb(0xCA9EE6),
        red: rgb(0xE78284),
        peach: rgb(0xEF9F76),
        yellow: rgb(0xE5C890),
        green: rgb(0xA6D189),
        teal: rgb(0x81C8BE),
        sapphire: rgb(0x85C1DC),
        blue: rgb(0x8CAAEE),
        text: rgb(0xC6D0F5),
        subtext0: rgb(0xA5ADCE),
        overlay1: rgb(0x838BA7),
        overlay0: rgb(0x737994),
        surface2: rgb(0x626880),
        surface1: rgb(0x51576D),
        surface0: rgb(0x414559),
        base: rgb(0x303446),
        mantle: rgb(0x292C3C),
        crust: rgb(0x232634),
    }
    .into_theme()
}

fn latte() -> Theme {
    Catppuccin {
        id: "catppuccin-latte",
        name: "Catppuccin Latte",
        dark: false,
        pink: rgb(0xEA76CB),
        mauve: rgb(0x8839EF),
        red: rgb(0xD20F39),
        peach: rgb(0xFE640B),
        yellow: rgb(0xDF8E1D),
        green: rgb(0x40A02B),
        teal: rgb(0x179299),
        sapphire: rgb(0x209FB5),
        blue: rgb(0x1E66F5),
        text: rgb(0x4C4F69),
        subtext0: rgb(0x6C6F85),
        overlay1: rgb(0x8C8FA1),
        overlay0: rgb(0x9CA0B0),
        surface2: rgb(0xACB0BE),
        surface1: rgb(0xBCC0CC),
        surface0: rgb(0xCCD0DA),
        base: rgb(0xEFF1F5),
        mantle: rgb(0xE6E9EF),
        crust: rgb(0xDCE0E8),
    }
    .into_theme()
}

// ---------------------------------------------------------------------------
// Catalogue et thème courant
// ---------------------------------------------------------------------------

/// Tous les thèmes disponibles, dans l'ordre où ils sont proposés.
///
/// **Pour en ajouter un** : écrire une fonction qui renvoie un [`Theme`] et
/// l'ajouter ici. Réglages, palette de commandes et persistance suivent tout
/// seuls — aucun des trois ne connaît la liste autrement que par ce tableau.
pub static THEMES: LazyLock<Vec<Theme>> = LazyLock::new(|| {
    vec![dark_theme(), light_theme(), latte(), frappe(), macchiato(), mocha()]
});

/// Index du thème « sombre » et du thème « clair » retenus quand la préférence
/// suit le système.
const SYSTEM_DARK: &str = "dark";
const SYSTEM_LIGHT: &str = "light";

/// Le thème demandé : soit un thème nommé, soit « celui du système ».
///
/// Séparé de [`Theme`] parce que « Système » n'est pas un thème : c'est une
/// règle de choix, qui dépend de l'OS et peut changer pendant la session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Suit le réglage clair/sombre du bureau.
    System,
    /// Un thème du catalogue, par son `id`.
    Named(&'static str),
}

impl Default for Choice {
    fn default() -> Self {
        Choice::Named("dark")
    }
}

impl Choice {
    /// Valeur écrite dans `settings.conf`.
    pub fn key(self) -> &'static str {
        match self {
            Choice::System => "system",
            Choice::Named(id) => id,
        }
    }

    /// Relit une valeur du fichier de réglages. Un identifiant inconnu (thème
    /// retiré, fichier bricolé) retombe sur le thème sombre plutôt que de
    /// laisser l'application sans couleurs.
    pub fn from_key(k: &str) -> Choice {
        if k == "system" {
            return Choice::System;
        }
        match by_id(k) {
            Some(t) => Choice::Named(t.id),
            None => Choice::default(),
        }
    }

    /// Le thème effectivement appliqué. `system_dark` vient de l'OS.
    pub fn resolve(self, system_dark: bool) -> &'static Theme {
        let id = match self {
            Choice::System if system_dark => SYSTEM_DARK,
            Choice::System => SYSTEM_LIGHT,
            Choice::Named(id) => id,
        };
        by_id(id).unwrap_or(&THEMES[0])
    }

    /// Libellé affiché dans les réglages et la palette de commandes.
    pub fn label(self, lang: crate::i18n::Lang) -> String {
        match self {
            Choice::System => {
                crate::i18n::tr3(lang, "Système (suit l'OS)", "System (follow OS)", "Sistema (sigue el SO)")
                    .to_string()
            }
            Choice::Named(id) => by_id(id).map_or_else(|| id.to_string(), |t| t.name.to_string()),
        }
    }
}

/// Le thème d'identifiant `id`, s'il existe.
pub fn by_id(id: &str) -> Option<&'static Theme> {
    THEMES.iter().find(|t| t.id == id)
}

/// Index du thème courant dans [`THEMES`].
///
/// Un entier atomique plutôt qu'un verrou : la lecture est sur le chemin du
/// rendu, appelée des milliers de fois par image (chaque libellé coloré), et
/// l'écriture n'a lieu qu'au changement de thème.
static CURRENT: AtomicUsize = AtomicUsize::new(0);

/// Le thème en vigueur. C'est par ici que passent toutes les couleurs affichées.
pub fn current() -> &'static Theme {
    let i = CURRENT.load(Ordering::Relaxed);
    THEMES.get(i).unwrap_or(&THEMES[0])
}

/// Fixe le thème en vigueur. Sans effet si l'identifiant est inconnu — mieux
/// vaut garder le thème précédent qu'afficher une interface incolore.
pub fn set_current(theme: &Theme) {
    if let Some(i) = THEMES.iter().position(|t| t.id == theme.id) {
        CURRENT.store(i, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deux thèmes de même identifiant seraient indiscernables dans le fichier
    /// de réglages : celui qu'on relit ne serait pas forcément celui qu'on a
    /// choisi. Le piège classique est le copier-coller d'une déclinaison.
    #[test]
    fn every_theme_has_a_unique_id_and_name() {
        for (i, a) in THEMES.iter().enumerate() {
            assert!(!a.id.is_empty() && !a.name.is_empty(), "{a:?} incomplet");
            for b in THEMES.iter().skip(i + 1) {
                assert_ne!(a.id, b.id, "identifiant en double : {}", a.id);
                assert_ne!(a.name, b.name, "nom en double : {}", a.name);
            }
        }
    }

    /// « Système » doit trouver de quoi se résoudre dans les deux sens, sinon
    /// la préférence retomberait silencieusement sur le premier thème venu.
    #[test]
    fn system_resolves_to_a_dark_and_a_light_theme() {
        let d = Choice::System.resolve(true);
        let l = Choice::System.resolve(false);
        assert!(d.dark, "{} devrait être sombre", d.id);
        assert!(!l.dark, "{} devrait être clair", l.id);
    }

    #[test]
    fn a_theme_survives_the_round_trip_through_the_settings_file() {
        for t in THEMES.iter() {
            let c = Choice::Named(t.id);
            assert_eq!(Choice::from_key(c.key()), c, "aller-retour cassé pour {}", t.id);
        }
        assert_eq!(Choice::from_key("system"), Choice::System);
    }

    /// Un fichier de réglages écrit par une version qui connaissait un thème
    /// depuis retiré ne doit pas laisser l'application sans couleurs.
    #[test]
    fn an_unknown_id_falls_back_instead_of_failing() {
        assert_eq!(Choice::from_key("theme-qui-nexiste-pas"), Choice::default());
        assert_eq!(Choice::Named("inconnu").resolve(true).id, THEMES[0].id);
    }

    /// La pulsation « CPU vivant » va de `flash` vers `changed`. Sur un thème
    /// clair, un pic PLUS CLAIR que le fond est invisible : c'est le défaut que
    /// traînait l'ancien thème clair, et qu'un thème ajouté referait volontiers.
    #[test]
    fn the_flash_peak_contrasts_with_the_background_of_every_theme() {
        let lum = |c: Color32| 0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32;
        for t in THEMES.iter() {
            let (flash, bg) = (lum(t.ui.flash), lum(t.ui.bg));
            if t.dark {
                assert!(flash > bg + 40.0, "{} : pic trop terne ({flash} vs fond {bg})", t.id);
            } else {
                assert!(flash < bg - 40.0, "{} : pic trop clair ({flash} vs fond {bg})", t.id);
            }
        }
    }

    /// Le texte doit se lire sur le fond de l'éditeur — c'est la seule chose
    /// qu'un thème ne peut pas se permettre de rater.
    #[test]
    fn code_text_contrasts_with_the_editor_background() {
        let lum = |c: Color32| 0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32;
        for t in THEMES.iter() {
            let d = (lum(t.syntax.text) - lum(t.ui.extreme)).abs();
            assert!(d > 80.0, "{} : texte et fond trop proches ({d})", t.id);
        }
    }

    #[test]
    fn setting_the_current_theme_changes_what_is_read_back() {
        let before = current().id;
        for t in THEMES.iter() {
            set_current(t);
            assert_eq!(current().id, t.id);
        }
        set_current(by_id(before).unwrap());
    }
}
