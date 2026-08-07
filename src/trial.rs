//! Délai avant inscription gratuite, en complément de la licence signée
//! (`crate::license`) : dès le premier lancement, les fonctions réservées
//! (désassemblage, registres/flags, timeline) restent utilisables pendant
//! [`TRIAL_DAYS`] jours sans licence, avant de retomber sur le verrouillage
//! habituel si aucune inscription (licence gratuite) n'a eu lieu entre-temps.
//! Pas un « essai » au sens commercial — il n'y a rien à acheter derrière,
//! juste un délai avant de devoir s'inscrire.
//!
//! Comme le reste du système de licence, c'est déclaratif plutôt qu'un verrou
//! technique absolu : le dépôt est public, quiconque lit ce fichier sait
//! comment le contourner en recompilant. Le but est de décourager les deux
//! contournements les plus faciles à faire *sans même toucher au code* :
//!
//! 1. **Supprimer le marqueur** pour relancer un essai complet. Contré par
//!    [`crate::app::paths::trial_marker_paths`] : trois copies redondantes
//!    sur des répertoires XDG distincts. `reconcile` recompose l'état à
//!    partir de celles qui survivent et réécrit celles qui manquent — il
//!    faut désormais supprimer les trois à la fois, pas une seule.
//! 2. **Reculer l'horloge système** une fois l'essai expiré, pour que
//!    `now - start` redevienne petit. Contré par une haute marque
//!    (`last_seen`) persistée à chaque lancement : l'heure effective utilisée
//!    pour le calcul est `max(horloge actuelle, dernière heure déjà vue)`,
//!    donc jamais inférieure à ce qui a déjà été observé.
//!
//! Combiner les deux (reculer l'horloge *et* supprimer les trois copies)
//! reste possible — c'est un contournement délibéré, pas un effacement ou un
//! réglage d'horloge distrait, ce qui correspond à l'objectif énoncé ici et
//! dans `crate::license`.
//!
//! [`trusted_now`] (le point 2 ci-dessus, isolé) est aussi réutilisée par
//! `crate::license::verify` pour la date d'expiration optionnelle d'une
//! licence signée : même risque de recul d'horloge, même parade.

use std::sync::OnceLock;

use crate::app::paths::trial_marker_paths;

pub(crate) const TRIAL_DAYS: u64 = 14;

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Jours restants avant inscription obligatoire (peut être négatif une fois
/// le délai dépassé). Compte en jours pleins écoulés depuis `start`, pas en
/// dates civiles : pas besoin de fuseau horaire ni de calendrier pour un
/// simple compte à rebours.
fn days_remaining(start: u64, now: u64) -> i64 {
    let elapsed_days = now.saturating_sub(start) / 86400;
    TRIAL_DAYS as i64 - elapsed_days as i64
}

/// Format d'un marqueur sur disque : `premier_lancement:dernier_vu` (secondes
/// Unix). Un entier seul (ancien format, une seule valeur) reste accepté :
/// il vaut alors pour les deux champs.
fn parse_marker(content: &str) -> Option<(u64, u64)> {
    let content = content.trim();
    match content.split_once(':') {
        Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
        None => {
            let v: u64 = content.parse().ok()?;
            Some((v, v))
        }
    }
}

fn format_marker(first_seen: u64, last_seen: u64) -> String {
    format!("{first_seen}:{last_seen}")
}

/// Recompose `(first_seen, last_seen)` à partir du contenu (déjà lu, une
/// entrée par copie potentiellement absente/illisible) des marqueurs
/// redondants, plus l'heure actuelle.
///
/// - `first_seen` = le plus ancien trouvé (résiste à la suppression d'une
///   copie : tant qu'une survit avec la vraie date, elle l'emporte).
/// - `last_seen` = le plus récent trouvé, jamais inférieur à `now` (résiste
///   à un recul d'horloge : la marque haute ne redescend jamais).
/// - Si aucune copie n'est exploitable, c'est un état neuf : les deux valent
///   `now`.
fn reconcile_values(now: u64, contents: &[Option<String>]) -> (u64, u64) {
    let found: Vec<(u64, u64)> = contents
        .iter()
        .filter_map(|c| c.as_deref())
        .filter_map(parse_marker)
        .collect();

    if found.is_empty() {
        return (now, now);
    }
    let first_seen = found.iter().map(|(f, _)| *f).min().unwrap();
    let last_seen = found.iter().map(|(_, l)| *l).max().unwrap().max(now);
    (first_seen, last_seen)
}

/// Lit les copies redondantes sur disque, recompose l'état via
/// [`reconcile_values`], puis réécrit chaque copie (manquante, illisible ou
/// juste désynchronisée) avec le résultat — auto-réparation.
fn reconcile_from_disk(now: u64) -> (u64, u64) {
    let paths = trial_marker_paths();
    let contents: Vec<Option<String>> =
        paths.iter().map(|p| std::fs::read_to_string(p).ok()).collect();
    let (first_seen, last_seen) = reconcile_values(now, &contents);

    let serialized = format_marker(first_seen, last_seen);
    for p in &paths {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, &serialized);
    }
    (first_seen, last_seen)
}

/// `(first_seen, last_seen)` figé pour la durée du process : la
/// réconciliation (lecture + auto-réparation des trois copies) n'a besoin de
/// tourner qu'une fois par lancement, pas à chaque frame — `is_unlocked()`
/// est appelée en continu depuis `dock.rs`.
static TRIAL_ANCHOR: OnceLock<(u64, u64)> = OnceLock::new();

/// Jours restants avant inscription obligatoire. `TRIAL_DAYS` (jamais
/// persistant) en test — comme `license::load`, qui renvoie toujours
/// `Missing` en `cfg!(test)` : les tests ne doivent dépendre ni écrire dans
/// une vraie installation. Ici la valeur neutre choisie est « délai déjà
/// passé » (voir `is_active`), pour que les tests existants sur le
/// verrouillage restent valides sans modification.
pub(crate) fn days_left() -> i64 {
    if cfg!(test) {
        return 0;
    }
    let now = now_unix();
    let &(first_seen, last_seen) = TRIAL_ANCHOR.get_or_init(|| reconcile_from_disk(now));
    // Heure effective jamais inférieure à la plus haute déjà observée
    // (persistée à l'initialisation ci-dessus) : un recul d'horloge en
    // cours de session ne fait pas reculer le compte à rebours.
    let effective_now = now.max(last_seen);
    days_remaining(first_seen, effective_now)
}

/// Encore avant la date limite d'inscription ? `false` en test (voir `days_left`).
pub(crate) fn is_active() -> bool {
    days_left() > 0
}

/// Heure « de confiance » : jamais inférieure à la plus haute horloge déjà
/// observée sur cette machine (même mécanisme anti-recul d'horloge que
/// [`days_left`], voir la doc de module). Réutilisée par
/// `crate::license::verify` pour vérifier la date d'expiration d'une
/// licence : sans ça, reculer l'horloge système après expiration
/// suffirait à faire réapparaître une licence expirée, exactement comme
/// pour le délai d'essai. `now_unix()` seul en test (voir `days_left`).
pub(crate) fn trusted_now() -> u64 {
    let now = now_unix();
    if cfg!(test) {
        return now;
    }
    let &(_, last_seen) = TRIAL_ANCHOR.get_or_init(|| reconcile_from_disk(now));
    now.max(last_seen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_trial_remaining_on_day_zero() {
        assert_eq!(days_remaining(1_000, 1_000), TRIAL_DAYS as i64);
    }

    #[test]
    fn counts_down_by_whole_days_not_partial_ones() {
        let start = 1_000_000;
        assert_eq!(days_remaining(start, start + 86_399), TRIAL_DAYS as i64, "23h59 : encore le jour 0");
        assert_eq!(days_remaining(start, start + 86_400), TRIAL_DAYS as i64 - 1);
        assert_eq!(days_remaining(start, start + 86_400 * 2), TRIAL_DAYS as i64 - 2);
    }

    #[test]
    fn goes_negative_once_expired() {
        let start = 0;
        let after = 86_400 * (TRIAL_DAYS + 5);
        assert!(days_remaining(start, after) < 0);
    }

    #[test]
    fn a_clock_that_moved_backwards_never_grants_extra_days() {
        // `saturating_sub` : si `now` < `start` (horloge remise en arrière,
        // ou marqueur corrompu), on ne doit jamais dépasser le délai complet.
        assert_eq!(days_remaining(1_000_000, 0), TRIAL_DAYS as i64);
    }

    #[test]
    fn fresh_install_has_no_readable_marker_anywhere() {
        let now = 500_000;
        assert_eq!(reconcile_values(now, &[None, None, None]), (now, now));
    }

    #[test]
    fn deleting_one_of_three_copies_does_not_reset_the_trial() {
        // Deux copies survivent avec la vraie date de départ ancienne ;
        // la troisième a été supprimée (`None`). Le vrai départ doit
        // l'emporter, pas « aucune copie trouvée = essai neuf ».
        let real_start = 1_000_000;
        let now = real_start + 86_400 * 20; // 20 jours plus tard, essai expiré
        let contents = [
            Some(format_marker(real_start, now - 100)),
            None, // supprimée par l'utilisateur
            Some(format_marker(real_start, now - 100)),
        ];
        let (first_seen, _) = reconcile_values(now, &contents);
        assert_eq!(first_seen, real_start);
    }

    #[test]
    fn a_forged_copy_claiming_a_fresh_start_does_not_win_over_survivors() {
        // Une copie trafiquée prétend être installée « maintenant » pour
        // repartir sur un essai complet ; les deux copies authentiques
        // gardent la vraie date ancienne. Le minimum (le plus ancien)
        // l'emporte : la date trafiquée est ignorée.
        let real_start = 1_000_000;
        let now = real_start + 86_400 * 20;
        let contents = [
            Some(format_marker(real_start, now - 100)),
            Some(format_marker(now, now)), // forgée
            Some(format_marker(real_start, now - 100)),
        ];
        let (first_seen, _) = reconcile_values(now, &contents);
        assert_eq!(first_seen, real_start);
    }

    #[test]
    fn rolling_back_the_clock_does_not_lower_the_high_water_mark() {
        // `last_seen` déjà vu loin dans le "futur" (par rapport à l'horloge
        // reculée) doit continuer à dominer, même si `now` a reculé.
        let real_start = 1_000_000;
        let previously_seen = real_start + 86_400 * 20;
        let rolled_back_now = real_start + 100; // horloge reculée après coup
        let contents = [
            Some(format_marker(real_start, previously_seen)),
            Some(format_marker(real_start, previously_seen)),
            Some(format_marker(real_start, previously_seen)),
        ];
        let (_, last_seen) = reconcile_values(rolled_back_now, &contents);
        assert_eq!(last_seen, previously_seen);
    }

    #[test]
    fn deleting_all_copies_and_rolling_back_the_clock_does_start_a_fresh_trial() {
        // Cas non contré, documenté : supprimer les TROIS copies à la fois
        // reste un contournement possible, tout comme recompiler depuis les
        // sources. Le but n'est que de rendre le geste délibéré, pas
        // accidentel ni trivial en une commande.
        let now = 42;
        assert_eq!(reconcile_values(now, &[None, None, None]), (now, now));
    }

    #[test]
    fn legacy_single_integer_marker_is_still_readable() {
        // Ancien format (avant l'ajout de `last_seen`) : un entier seul.
        assert_eq!(parse_marker("1234567"), Some((1_234_567, 1_234_567)));
    }

    #[test]
    fn current_format_round_trips() {
        assert_eq!(parse_marker(&format_marker(10, 20)), Some((10, 20)));
    }

    #[test]
    fn garbage_marker_is_treated_as_absent() {
        assert_eq!(parse_marker("n'importe quoi"), None);
        assert_eq!(reconcile_values(999, &[Some("n'importe quoi".to_string())]), (999, 999));
    }
}
