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
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("asm_studio")
}

/// Peuple `~/.local/share/asm_studio/examples/` avec les programmes de
/// démonstration et les exercices auto-corrigés.
///
/// Chaque fichier ABSENT est écrit, à chaque lancement — et non plus « tout ou
/// rien au premier lancement ». Ainsi un exemple ou un exercice ajouté dans une
/// nouvelle version apparaît aussi chez les utilisateurs installés de longue
/// date, au lieu de rester invisible parce que le dossier existait déjà. Un
/// fichier présent n'est jamais réécrit : le travail de l'élève est préservé.
pub(super) fn setup_examples() {
    let dir = data_dir().join("examples");
    if std::fs::create_dir_all(&dir).is_err() {
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
        // Exercices auto-corrigés : squelettes à compléter (voir src/exercise.rs).
        ("ex_code_sortie.asm",  include_str!("../../examples_seed/ex_code_sortie.asm")),
        ("ex_somme.asm",        include_str!("../../examples_seed/ex_somme.asm")),
        ("ex_maximum.asm",      include_str!("../../examples_seed/ex_maximum.asm")),
        ("ex_puissance.asm",    include_str!("../../examples_seed/ex_puissance.asm")),
    ];
    for (name, content) in files {
        let path = dir.join(name);
        if !path.exists() {
            let _ = std::fs::write(path, content);
        }
    }
}

pub(super) fn settings_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("asm_studio").join("settings.conf"))
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

    #[test]
    fn list_entries_finds_asm_example() {
        let (_dirs, files) = list_entries(&abs_dir_of(Path::new("examples/test.asm")));
        assert!(
            files.iter().any(|f| f.file_name().unwrap() == "test.asm"),
            "test.asm doit apparaître dans l'explorateur"
        );
    }
}
