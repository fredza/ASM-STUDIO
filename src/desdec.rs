//! Passage de la main à Desdec, l'explorateur de binaires.
//!
//! Le trajet inverse existe déjà : Desdec exporte une fonction en NASM et lance
//! ASM Studio sur le fichier écrit — c'est pour lui que [`crate::app::App`]
//! ouvre le chemin reçu en argument. Ce module referme la boucle. L'élève
//! assemble ici, puis regarde là-bas ce que l'assembleur a réellement produit :
//! sections et entropie, chaînes, table d'import, désassemblage complet.
//!
//! Rien n'est envoyé d'autre que le binaire qu'ASM Studio vient lui-même de
//! produire, et Desdec n'exécute jamais le fichier qu'il ouvre.
//!
//! Desdec s'installe à part. Son absence n'est donc pas une erreur mais un cas
//! ordinaire, et il est cherché à chaque envoi plutôt qu'une fois pour toutes :
//! l'installer pendant que l'IDE tourne doit suffire à s'en servir, comme pour
//! Wine (voir [`crate::winerun::available`]).

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use crate::i18n::{self, Lang};

/// Nom de l'exécutable, tel que l'installeur de Desdec le dépose.
const EXE: &str = "desdec";

/// Les processus Desdec lancés d'ici et pas encore terminés.
///
/// Un enfant dont personne ne lit le code de sortie reste dans la table des
/// processus jusqu'à la mort de son parent — et ASM Studio, lui, tourne des
/// heures. On garde donc les poignées pour les relever au prochain envoi :
/// l'utilisateur qui ouvre dix binaires dans la journée ne laisse pas dix
/// zombies derrière lui.
static LAUNCHED: Mutex<Vec<Child>> = Mutex::new(Vec::new());

/// Chemin de l'exécutable Desdec, s'il est installé.
///
/// Le `PATH` d'abord, puis les préfixes où les installeurs le déposent : une
/// application lancée depuis le menu du bureau hérite parfois d'un `PATH`
/// minimal, où `~/.local/bin` ne figure pas — alors même que les deux outils y
/// sont installés côte à côte.
pub fn locate() -> Option<PathBuf> {
    locate_in(std::env::var_os("PATH").as_deref(), std::env::var_os("HOME").as_deref())
}

/// Le cœur de [`locate`], sans l'environnement du processus.
///
/// Séparé pour que la recherche se teste sans écrire dans `PATH` : les tests
/// s'exécutent en parallèle, et le reste de la suite lance `nasm` et `ld` — un
/// `PATH` modifié le temps d'un test les priverait de leurs outils.
fn locate_in(path: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf> {
    let from_path: Vec<PathBuf> = path.map(|p| std::env::split_paths(p).collect()).unwrap_or_default();
    let fallbacks = [
        home.map(|home| PathBuf::from(home).join(".local/bin")),
        Some(PathBuf::from("/usr/local/bin")),
        Some(PathBuf::from("/usr/bin")),
    ];
    from_path
        .into_iter()
        .chain(fallbacks.into_iter().flatten())
        .map(|dir| dir.join(EXE))
        .find(|candidate| is_executable(candidate))
}

/// Un fichier exécutable — et non un dossier nommé `desdec`, ni un fichier sans
/// le bit d'exécution qu'on ne pourrait de toute façon pas lancer.
fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Ouvre `binary` dans Desdec, sans attendre : la fenêtre d'ASM Studio doit
/// rester vivante pendant que l'autre analyse le fichier.
///
/// Les trois flux standard sont coupés. Desdec est une application graphique,
/// et ce qu'il écrirait sur la sortie du terminal qui a lancé ASM Studio ne
/// serait lu par personne — au mieux, cela brouillerait la console de l'élève.
///
/// Un envoi réussi ne garantit pas que la fenêtre s'ouvrira : le processus est
/// lancé, la suite lui appartient. C'est ce que dit la valeur de retour, et
/// rien de plus.
pub fn send(binary: &Path, lang: Lang) -> Result<(), String> {
    let Some(exe) = locate() else {
        return Err(i18n::tr3(
            lang,
            "Desdec est introuvable. C'est un explorateur de binaires qui s'installe à part : une fois son exécutable « desdec » dans le PATH (ou dans ~/.local/bin), cette commande lui passera le fichier assemblé.",
            "Desdec was not found. It is a binary explorer installed separately: once its \"desdec\" executable is on the PATH (or in ~/.local/bin), this command will hand it the assembled file.",
            "No se encontró Desdec. Es un explorador de binarios que se instala aparte: cuando su ejecutable «desdec» esté en el PATH (o en ~/.local/bin), esta orden le pasará el archivo ensamblado.",
        )
        .to_string());
    };
    // Le binaire doit exister avant qu'on ne lance quoi que ce soit : Desdec
    // s'ouvrirait sur son écran vide, et l'élève croirait l'envoi réussi.
    if !binary.is_file() {
        return Err(format!(
            "{} : {}",
            i18n::tr3(
                lang,
                "Aucun binaire à envoyer — assemblez d'abord le programme",
                "No binary to send — assemble the program first",
                "No hay binario que enviar — ensamble primero el programa",
            ),
            binary.display()
        ));
    }
    reap();
    let child = Command::new(&exe)
        .arg(binary)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            format!(
                "{} {} : {e}",
                i18n::tr3(lang, "Impossible de lancer", "Cannot start", "No se puede iniciar"),
                exe.display()
            )
        })?;
    if let Ok(mut launched) = LAUNCHED.lock() {
        launched.push(child);
    }
    Ok(())
}

/// Relève les Desdec déjà refermés. Les vivants restent en liste.
fn reap() {
    let Ok(mut launched) = LAUNCHED.lock() else { return };
    launched.retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Écrit un faux `desdec` exécutable dans `dir` et renvoie son chemin.
    fn fake_desdec(dir: &Path) -> PathBuf {
        let path = dir.join(EXE);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").expect("écriture du faux desdec");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("droits");
        }
        path
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("asm_studio_desdec_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dossier temporaire");
        dir
    }

    #[test]
    fn an_executable_on_the_path_is_found() {
        let dir = temp_dir("path");
        let expected = fake_desdec(&dir);
        assert_eq!(
            locate_in(Some(dir.as_os_str()), None).as_deref(),
            Some(expected.as_path()),
            "le desdec du PATH doit être trouvé"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_it_on_the_path_the_usual_install_prefix_is_tried() {
        let home = temp_dir("home");
        let bin = home.join(".local/bin");
        std::fs::create_dir_all(&bin).expect("faux ~/.local/bin");
        let expected = fake_desdec(&bin);
        let empty = temp_dir("vide");
        assert_eq!(
            locate_in(Some(empty.as_os_str()), Some(home.as_os_str())).as_deref(),
            Some(expected.as_path()),
            "~/.local/bin est le second endroit à regarder"
        );
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn a_directory_named_desdec_is_not_an_executable() {
        let dir = temp_dir("dossier");
        std::fs::create_dir_all(dir.join(EXE)).expect("faux dossier");
        assert!(!is_executable(&dir.join(EXE)), "un dossier n'est pas un exécutable");
        assert!(!is_executable(&dir.join("absent")), "un fichier absent non plus");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sending_a_binary_that_does_not_exist_says_so_rather_than_launching() {
        let missing = std::env::temp_dir().join("asm_studio_binaire_absent");
        let _ = std::fs::remove_file(&missing);
        let err = send(&missing, Lang::Fr).expect_err("un binaire absent ne s'envoie pas");
        assert!(
            err.contains("assemblez") || err.contains("Desdec est introuvable"),
            "le message doit nommer la cause : {err}"
        );
    }
}
