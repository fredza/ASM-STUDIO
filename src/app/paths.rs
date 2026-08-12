//! Emplacements sur disque et parcours de l'arborescence.
//!
//! ASM Studio suit la spécification XDG : les réglages vont dans
//! `~/.config/asm_studio/`, les exemples et les artefacts d'assemblage dans
//! `~/.local/share/asm_studio/`. Ces dossiers sont toujours inscriptibles, quel
//! que soit l'endroit d'où l'exécutable est lancé — c'est pourquoi on n'utilise
//! pas le répertoire de l'exécutable (`target/debug/examples/`, créé par Cargo,
//! provoquait un faux positif à la création des exemples).

use std::path::{Path, PathBuf};

/// Nom d'affichage d'un chemin (dernier segment).
pub(super) fn file_name(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

/// Entrées d'un dossier : (sous-dossiers, tous les fichiers), triés, en masquant
/// les entrées cachées (préfixe `.`). Pour l'explorateur en arbre.
pub(super) fn list_entries(dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if file_name(&p).starts_with('.') {
                continue;
            }
            if p.is_dir() {
                dirs.push(p);
            } else {
                files.push(p);
            }
        }
    }
    dirs.sort();
    files.sort();
    (dirs, files)
}

/// True si le fichier est une source assembleur (`.asm`/`.s`).
pub(super) fn is_asm(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "asm" || e == "s")
}

/// Répertoire de données utilisateur XDG : `~/.local/share/asm_studio/`.
/// Cohérent avec les settings dans `~/.config/asm_studio/`, toujours accessible
/// en écriture quelle que soit la position de l'exécutable.
pub(super) fn data_dir() -> PathBuf {
    // La spec XDG Base Directory impose de traiter une variable *définie mais
    // vide* comme absente : sans le `.filter`, une variable exportée vide
    // (fréquent selon la session de bureau) donne un chemin relatif au
    // répertoire courant du process au lieu du vrai chemin absolu.
    std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("asm_studio")
}

/// Peuple `~/.local/share/asm_studio/examples/` avec les programmes de
/// démonstration et les exercices auto-corrigés.
///
/// Chaque fichier ABSENT est écrit — et non plus « tout ou rien au premier
/// lancement ». Ainsi un exemple ou un exercice ajouté dans une nouvelle
/// version apparaît aussi chez les utilisateurs installés de longue date, au
/// lieu de rester invisible parce que le dossier existait déjà. Un fichier
/// présent n'est jamais réécrit : le travail de l'élève est préservé.
///
/// Ce parcours ne se refait pas à chaque démarrage. Un témoin déposé dans le
/// dossier porte la version qui l'a semé ; tant qu'elle correspond, il n'y a
/// rien de nouveau à écrire et on s'arrête sur un seul `read` au lieu de
/// soixante `stat`. Le témoin porte la version *du paquet* et une révision du
/// catalogue, et non le numéro de build : recompiler ne change pas le
/// catalogue, livrer si.
pub(super) fn setup_examples() {
    // Incrémenté quand on ajoute un exemple sans forcément livrer une nouvelle
    // version semver. Les installations déjà semées reçoivent ainsi les
    // nouveaux fichiers, sans jamais réécrire le travail existant.
    const CATALOGUE_REVISION: &str = "2";
    let dir = data_dir().join("examples");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let stamp = dir.join(".semis");
    let catalogue_stamp = format!("{}:{CATALOGUE_REVISION}", crate::version::SEMVER);
    if !needs_seeding(std::fs::read_to_string(&stamp).ok().as_deref(), &catalogue_stamp) {
        return;
    }

    let files: &[(&str, &str)] = &[
        ("asmstd.inc",          include_str!("../../examples/asmstd.inc")),
        ("hello_world.asm",     include_str!("../../examples_seed/hello_world.asm")),
        ("exit_code.asm",       include_str!("../../examples_seed/exit_code.asm")),
        ("arithmetic.asm",      include_str!("../../examples_seed/arithmetic.asm")),
        ("boucle.asm",          include_str!("../../examples_seed/boucle.asm")),
        ("conditionnels.asm",   include_str!("../../examples_seed/conditionnels.asm")),
        ("fibonacci.asm",       include_str!("../../examples_seed/fibonacci.asm")),
        ("factorielle.asm",     include_str!("../../examples_seed/factorielle.asm")),
        ("longueur_chaine.asm", include_str!("../../examples_seed/longueur_chaine.asm")),
        ("pile_demo.asm",       include_str!("../../examples_seed/pile_demo.asm")),
        ("lire_ecrire.asm",     include_str!("../../examples_seed/lire_ecrire.asm")),
        // Les quatre fondamentaux Windows gardent leurs jumeaux ELF ci-dessus :
        // ouvrir l'un d'eux active automatiquement la cible PE64.
        ("win_hello_world.asm", include_str!("../../examples_seed/win_hello_world.asm")),
        ("win_arithmetic.asm",  include_str!("../../examples_seed/win_arithmetic.asm")),
        ("win_boucle.asm",      include_str!("../../examples_seed/win_boucle.asm")),
        ("win_lire_ecrire.asm", include_str!("../../examples_seed/win_lire_ecrire.asm")),
        // Exercices auto-corrigés : squelettes à compléter (voir src/exercise.rs).
        ("ex_code_sortie.asm",  include_str!("../../examples_seed/ex_code_sortie.asm")),
        ("ex_somme.asm",        include_str!("../../examples_seed/ex_somme.asm")),
        ("ex_maximum.asm",      include_str!("../../examples_seed/ex_maximum.asm")),
        ("ex_puissance.asm",    include_str!("../../examples_seed/ex_puissance.asm")),
        ("ex_factorielle.asm",  include_str!("../../examples_seed/ex_factorielle.asm")),
        ("ex_fibonacci.asm",    include_str!("../../examples_seed/ex_fibonacci.asm")),
        ("ex_longueur.asm",     include_str!("../../examples_seed/ex_longueur.asm")),
        ("ex_bits.asm",         include_str!("../../examples_seed/ex_bits.asm")),
        ("ex_tableau.asm",      include_str!("../../examples_seed/ex_tableau.asm")),
        ("ex_moyenne.asm",      include_str!("../../examples_seed/ex_moyenne.asm")),
        // Les exercices du livret « L'Assembleur x86-64 pour débutants », dans
        // l'ordre de ses chapitres : le nom porte le numéro du cours, pour que
        // l'élève qui lit le PDF retrouve l'exercice sans chercher. Ceux du
        // chapitre 9 dont le sujet est déjà couvert (somme d'un tableau) ne sont
        // pas dupliqués : ex_tableau.asm les traite.
        ("ex_c1_bases.asm",              include_str!("../../examples_seed/ex_c1_bases.asm")),
        ("ex_c2_1_code_retour.asm",      include_str!("../../examples_seed/ex_c2_1_code_retour.asm")),
        ("ex_c2_2_mon_message.asm",      include_str!("../../examples_seed/ex_c2_2_mon_message.asm")),
        ("ex_c3_1_tailles.asm",          include_str!("../../examples_seed/ex_c3_1_tailles.asm")),
        ("ex_c3_2_copier_registres.asm", include_str!("../../examples_seed/ex_c3_2_copier_registres.asm")),
        ("ex_c4_1_calculette.asm",       include_str!("../../examples_seed/ex_c4_1_calculette.asm")),
        ("ex_c4_2_division_signee.asm",  include_str!("../../examples_seed/ex_c4_2_division_signee.asm")),
        ("ex_c5_1_echange.asm",          include_str!("../../examples_seed/ex_c5_1_echange.asm")),
        ("ex_c5_2_trois_valeurs.asm",    include_str!("../../examples_seed/ex_c5_2_trois_valeurs.asm")),
        ("ex_c6_1_plus_petit.asm",       include_str!("../../examples_seed/ex_c6_1_plus_petit.asm")),
        ("ex_c6_2_trois_nombres.asm",    include_str!("../../examples_seed/ex_c6_2_trois_nombres.asm")),
        ("ex_c6_3_pair_impair.asm",      include_str!("../../examples_seed/ex_c6_3_pair_impair.asm")),
        ("ex_c7_1_compte_rebours.asm",   include_str!("../../examples_seed/ex_c7_1_compte_rebours.asm")),
        ("ex_c7_2_etoiles.asm",          include_str!("../../examples_seed/ex_c7_2_etoiles.asm")),
        ("ex_c7_3_multiplication.asm",   include_str!("../../examples_seed/ex_c7_3_multiplication.asm")),
        ("ex_c8_1_soustraire.asm",       include_str!("../../examples_seed/ex_c8_1_soustraire.asm")),
        ("ex_c8_2_somme_jusqua.asm",     include_str!("../../examples_seed/ex_c8_2_somme_jusqua.asm")),
        ("ex_c8_3_trois_appels.asm",     include_str!("../../examples_seed/ex_c8_3_trois_appels.asm")),
        ("ex_c9_1_tableau_min.asm",      include_str!("../../examples_seed/ex_c9_1_tableau_min.asm")),
        ("ex_c9_3_compter_pairs.asm",    include_str!("../../examples_seed/ex_c9_3_compter_pairs.asm")),
        ("ex_c10_1_triple.asm",          include_str!("../../examples_seed/ex_c10_1_triple.asm")),
        ("ex_c10_2_somme_saisie.asm",    include_str!("../../examples_seed/ex_c10_2_somme_saisie.asm")),
        ("ex_c11_2_tri_decroissant.asm", include_str!("../../examples_seed/ex_c11_2_tri_decroissant.asm")),
        ("ex_c11_3_fizzbuzz.asm",        include_str!("../../examples_seed/ex_c11_3_fizzbuzz.asm")),
        ("ex_c11_4_palindrome.asm",      include_str!("../../examples_seed/ex_c11_4_palindrome.asm")),
        ("ex_c11_5_premiers.asm",        include_str!("../../examples_seed/ex_c11_5_premiers.asm")),
    ];
    for (name, content) in files {
        let path = dir.join(name);
        if !path.exists() {
            let _ = std::fs::write(path, content);
        }
    }
    // Témoin écrit en dernier : si l'écriture des fichiers a échoué en cours de
    // route (disque plein, dossier en lecture seule), le prochain lancement
    // retentera plutôt que de considérer le semis fait.
    let _ = std::fs::write(&stamp, catalogue_stamp);
}

/// Le catalogue d'exemples doit-il être parcouru ?
///
/// `stamp` est le contenu du témoin déposé au dernier semis, s'il existe.
/// Séparé du disque pour être vérifiable : c'est la décision qui compte, pas
/// l'écriture.
fn needs_seeding(stamp: Option<&str>, version: &str) -> bool {
    stamp.is_none_or(|s| s.trim() != version)
}

pub(super) fn settings_path() -> Option<PathBuf> {
    // Voir le commentaire de `data_dir` : une variable XDG vide doit être
    // traitée comme absente, sous peine de chemin relatif au répertoire
    // courant du process au lieu du vrai chemin absolu.
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("asm_studio").join("settings.conf"))
}

/// Chemin de la licence collée par l'utilisateur, à côté de `settings.conf`.
/// `pub(crate)` : lu depuis `crate::license`, hors du module `app`.
pub(crate) fn license_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("asm_studio").join("license.txt"))
}

/// Répertoire de cache XDG : `~/.cache/asm_studio/`.
fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("asm_studio")
}

/// Répertoire d'état XDG : `~/.local/state/asm_studio/`.
fn state_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("asm_studio")
}

/// Marqueurs de premier lancement (période avant inscription gratuite),
/// en **trois copies redondantes** sur des répertoires XDG distincts, sous
/// des noms neutres différents. Volontairement discrets : ni à côté de
/// `settings.conf`/`license.txt` (déjà connus comme emplacements à
/// chercher), ni tous au même endroit sous le même nom.
///
/// Ce n'est toujours pas un verrou absolu : le dépôt est public, quiconque
/// lit `crate::trial` sait où ils sont et comment ils fonctionnent. Mais
/// `crate::trial::reconcile` recompose l'état à partir de celles qui
/// survivent et réécrit celles qui manquent : supprimer une seule copie (le
/// réflexe le plus évident, `rm ~/.local/share/asm_studio/.cache_id`) ne
/// suffit plus à obtenir un nouvel essai — il faut désormais trouver et
/// supprimer les trois en même temps, un geste délibéré plutôt qu'un
/// effacement distrait.
/// `pub(crate)` : lu depuis `crate::trial`, hors du module `app`.
pub(crate) fn trial_marker_paths() -> [PathBuf; 3] {
    [
        data_dir().join(".cache_id"),
        cache_dir().join(".sess_meta"),
        state_dir().join(".ck"),
    ]
}

/// Répertoire contenant `asmstd.inc` dans les données utilisateur.
pub(super) fn asmstd_dir() -> Option<PathBuf> {
    let dir = data_dir().join("examples");
    dir.join("asmstd.inc").exists().then_some(dir)
}

/// Répertoire absolu contenant `path` (remonte à `current_dir` si besoin).
pub(super) fn abs_dir_of(path: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    abs.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn abs_dir_is_absolute_and_navigable() {
        // À partir d'un chemin relatif, on obtient un dossier absolu dont on
        // peut remonter le parent (ce qui faisait échouer le navigateur avant).
        let dir = abs_dir_of(Path::new("examples/test.asm"));
        assert!(dir.is_absolute(), "le dossier doit être absolu");
        assert!(dir.ends_with("examples"));
        assert!(dir.parent().is_some(), "on doit pouvoir remonter (..)");
    }

    /// Le semis ne se refait que lorsqu'il a quelque chose à faire : au premier
    /// lancement, et après une mise à jour qui peut avoir ajouté des exemples.
    /// Entre les deux, démarrer ne doit rien coûter.
    #[test]
    fn seeding_happens_once_per_version() {
        assert!(needs_seeding(None, "0.4.7"), "premier lancement : il faut semer");
        assert!(!needs_seeding(Some("0.4.7"), "0.4.7"), "déjà semé : rien à faire");
        assert!(needs_seeding(Some("0.4.6"), "0.4.7"), "mise à jour : de nouveaux exemples peut-être");
        // Le fichier est écrit sans retour à la ligne, mais un éditeur en
        // ajoute un : ne pas ressemer pour un « \n ».
        assert!(!needs_seeding(Some("0.4.7\n"), "0.4.7"));
        assert!(needs_seeding(Some(""), "0.4.7"), "témoin vide : on ressème");
        assert!(
            needs_seeding(Some("0.4.7:1"), "0.4.7:2"),
            "une révision de catalogue doit déposer les nouveaux exemples"
        );
    }

    #[test]
    fn list_entries_finds_asm_example() {
        let (_dirs, files) = list_entries(&abs_dir_of(Path::new("examples/test.asm")));
        assert!(
            files.iter().any(|f| f.file_name().unwrap() == "test.asm"),
            "test.asm doit apparaître dans l'explorateur"
        );
    }
}
