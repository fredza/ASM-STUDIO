//! La version du logiciel, en un seul endroit.
//!
//! Trois nombres qui ne veulent pas dire la même chose cohabitaient : la
//! version du paquet (`Cargo.toml`), le numéro de bêta réécrit à la main dans
//! le bandeau — resté sur « BÊTA 2 » toute une version — et le hash git du
//! build. Une information affichée à trois endroits finit toujours par se
//! contredire.
//!
//! Tout est donc dérivé d'une seule source. `Cargo.toml` porte la version
//! semver complète, préversion comprise :
//!
//! ```text
//!     0.4.7-beta.4
//!     │ │ │  └── 4ᵉ bêta de cette version
//!     │ │ └───── PATCH : corrections
//!     │ └─────── MINEUR : ajouts compatibles
//!     └───────── MAJEUR : ruptures
//! ```
//!
//! Le numéro de build s'y ajoute en métadonnées (`+build.127`), et il est
//! incrémenté par `build.rs` à chaque compilation. Semver le dit explicitement :
//! les métadonnées de build ne comptent pas dans la comparaison de versions —
//! deux binaires `0.4.7-beta.4+build.126` et `+build.127` sont la même version
//! du logiciel, compilée deux fois. C'est exactement ce qu'on veut : le numéro
//! identifie un binaire, la version identifie une livraison.

/// Version du paquet, telle qu'écrite dans `Cargo.toml` (`0.4.7-beta.4`).
pub const SEMVER: &str = env!("CARGO_PKG_VERSION");

/// Numéro de build, incrémenté à chaque compilation par `build.rs`.
pub const BUILD: &str = env!("BUILD_NUMBER");

/// Hash court du commit compilé, ou « inconnu » hors dépôt git.
pub const COMMIT: &str = env!("GIT_HASH");

/// Date de compilation (`AAAA-MM-JJ`).
pub const DATE: &str = env!("BUILD_DATE");

/// Numéro de bêta, s'il y en a un (`0.4.7-beta.4` → `Some("4")`).
///
/// Une version finale n'en a pas, et le bandeau ne prétend alors plus rien.
pub fn beta() -> Option<&'static str> {
    SEMVER
        .split_once("-beta.")
        .map(|(_, rest)| rest.split('+').next().unwrap_or(rest))
}

/// Version complète, métadonnées de build comprises :
/// `0.4.7-beta.4+build.127`. C'est ce qui s'affiche dans « À propos » et ce que
/// l'utilisateur transmet quand il signale quelque chose.
pub fn full() -> String {
    format!("{SEMVER}+build.{BUILD}")
}

/// `remote` est-il strictement plus récent que `local` ?
///
/// Comparaison semver, y compris les préversions — et c'est tout l'enjeu : une
/// bêta précède la version finale du même numéro (`0.4.7-beta.4` < `0.4.7`),
/// alors qu'une lecture naïve la ferait passer pour un `0.4.4` et proposerait
/// une mise à jour vers une version antérieure. Les métadonnées de build sont
/// ignorées, comme la norme le prescrit : `+build.127` ne rend pas plus récent.
pub fn is_newer(remote: &str, local: &str) -> bool {
    order(remote) > order(local)
}

/// Clé de comparaison d'une version : les trois nombres, puis la préversion.
///
/// La préversion est rendue par un couple `(rang, numéro)` où le rang vaut 3
/// pour une version finale : elle l'emporte donc sur toutes ses préversions,
/// qui se classent entre elles dans l'ordre alpha < bêta < rc.
fn order(v: &str) -> (u32, u32, u32, u8, u32) {
    let v = v.trim().trim_start_matches('v');
    let v = v.split('+').next().unwrap_or(v); // métadonnées de build : ignorées
    let (core, pre) = match v.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (v, None),
    };
    let mut it = core.split('.').map(|n| n.trim().parse().unwrap_or(0));
    let (major, minor, patch) = (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    );
    let (rank, number) = match pre {
        None => (3, 0),
        Some(pre) => {
            let (name, num) = pre.split_once('.').unwrap_or((pre, "0"));
            let rank = match name {
                "alpha" => 0,
                "beta" => 1,
                "rc" => 2,
                // Étiquette inconnue : traitée comme une préversion très en
                // amont, plutôt que comme une finale qu'elle n'est pas.
                _ => 0,
            };
            (rank, num.parse().unwrap_or(0))
        }
    };
    (major, minor, patch, rank, number)
}

/// Étiquette courte pour le bandeau de préversion : « BÊTA 4 », ou rien du tout
/// sur une version finale.
pub fn beta_label(lang: crate::i18n::Lang) -> Option<String> {
    let n = beta()?;
    Some(match lang {
        crate::i18n::Lang::Fr => format!("VERSION BÊTA {n}"),
        crate::i18n::Lang::En => format!("BETA {n} VERSION"),
        crate::i18n::Lang::Es => format!("VERSIÓN BETA {n}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;

    /// La version du paquet doit rester analysable : c'est elle qui alimente le
    /// bandeau, « À propos », la vérification de licence et la mise à jour.
    #[test]
    fn the_package_version_parses_as_semver() {
        let (major, minor, patch, ..) = order(SEMVER);
        assert_eq!(
            format!("{major}.{minor}.{patch}"),
            SEMVER.split(['-', '+']).next().unwrap(),
            "les trois nombres doivent se relire tels qu'écrits"
        );
        // Un suffixe, s'il existe, est une préversion reconnue.
        if let Some(suffix) = SEMVER.split_once('-').map(|(_, s)| s) {
            assert!(
                suffix.starts_with("beta.") || suffix.starts_with("rc."),
                "préversion inattendue : {suffix}"
            );
        }
    }

    /// Le numéro de bêta vient de la version, et de nulle part ailleurs — c'est
    /// précisément le doublon qui avait laissé « BÊTA 2 » à l'écran pendant que
    /// `Cargo.toml` en annonçait une autre.
    #[test]
    fn the_beta_number_follows_the_package_version() {
        match SEMVER.split_once("-beta.") {
            Some((_, n)) => {
                let n = n.split('+').next().unwrap();
                assert_eq!(beta(), Some(n));
                for lang in [Lang::Fr, Lang::En, Lang::Es] {
                    let label = beta_label(lang).expect("une bêta s'annonce");
                    assert!(label.contains(n), "« {label} » doit porter le numéro {n}");
                }
            }
            None => {
                assert_eq!(beta(), None, "une version finale n'annonce pas de bêta");
                assert!(beta_label(Lang::Fr).is_none());
            }
        }
    }

    /// Une bêta précède la finale du même numéro, et les préversions se
    /// classent entre elles. Sans cela, `0.4.7-beta.4` se lisait « 0.4.4 » et
    /// l'IDE proposait de « mettre à jour » vers une version plus ancienne.
    #[test]
    fn prereleases_compare_before_their_final_version() {
        assert!(is_newer("0.4.7", "0.4.7-beta.4"), "la finale l'emporte sur sa bêta");
        assert!(!is_newer("0.4.7-beta.4", "0.4.7"));
        assert!(is_newer("0.4.7-beta.5", "0.4.7-beta.4"));
        assert!(is_newer("0.4.8-beta.8", "0.4.7"));
        assert!(!is_newer("0.4.7-beta.4", "0.4.7-beta.4"), "égalité : rien à faire");
        assert!(is_newer("0.4.7-rc.1", "0.4.7-beta.9"), "rc après bêta");
        assert!(is_newer("0.5.0-beta.1", "0.4.7"), "le numéro passe avant le rang");
        // Métadonnées de build : sans effet, la norme est explicite.
        assert!(!is_newer("0.4.7+build.999", "0.4.7+build.1"));
        // Et le « v » des tags GitHub ne doit pas fausser la lecture.
        assert!(is_newer("v0.5.0", "0.4.7"));
    }

    /// La version affichée porte le numéro de build, et le numéro de build est
    /// un nombre : c'est lui qui distingue deux compilations d'une même version.
    #[test]
    fn the_full_version_carries_the_build_number() {
        let full = full();
        let (version, meta) = full.split_once('+').expect("métadonnées de build attendues");
        assert_eq!(version, SEMVER);
        let n = meta.strip_prefix("build.").expect("préfixé par « build. »");
        assert!(n.parse::<u64>().is_ok(), "numéro de build non numérique : {n}");
    }
}
