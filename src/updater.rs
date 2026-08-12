//! Mise à jour automatique : vérifie GitHub Releases, télécharge et remplace le binaire.
//!
//! La vérification tourne dans un thread de fond pour ne pas bloquer l'UI.
//! Le résultat est récupéré dans `update()` via un canal `mpsc`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;

/// Dépôt GitHub source des releases.
///
/// Orthographié exactement comme le dépôt. L'API GitHub est insensible à la
/// casse et acceptait donc `fredza/asm-studio`, mais une constante qui ne
/// ressemble pas à sa cible n'invite pas à être vérifiée — et rien ne garantit
/// cette tolérance aux redirections près (un dépôt renommé répond en 301, que
/// `ureq` ne suit pas forcément avec la même méthode).
const GITHUB_REPO: &str = "fredza/ASM-STUDIO";

/// Clé publique réservée aux mises à jour. Elle est distincte de celle des
/// licences : une compromission de l'un des processus ne donne pas
/// automatiquement le contrôle de l'autre.
///
/// La clé privée correspondante reste hors de ce dépôt ; chaque release publie
/// une signature Base64 dans un asset `<binaire>.sig`.
const UPDATE_PUBLIC_KEY: [u8; 32] = [
    0xDF, 0xDA, 0xC3, 0x0E, 0x0B, 0xB2, 0xFB, 0x98, 0x8F, 0x58, 0x13, 0xE6, 0x30, 0xDD, 0x39, 0xC9,
    0x27, 0x44, 0x91, 0x7C, 0x75, 0x04, 0x7C, 0xD7, 0x0C, 0x44, 0x4D, 0xF0, 0xAC, 0x6D, 0x58, 0x39,
];

/// URL de l'API GitHub Releases.
fn api_url() -> String {
    format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest")
}

/// Page du dépôt, telle qu'affichée dans la fenêtre « À propos ».
pub fn repo_url() -> String {
    format!("https://github.com/{GITHUB_REPO}")
}

/// Le même dépôt sans le protocole : plus court à lire dans l'UI.
pub fn repo_label() -> String {
    format!("github.com/{GITHUB_REPO}")
}

// ---------- Types publics ----------

#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    /// Tag de la version, ex. "v0.4.0".
    pub tag: String,
    /// Notes de publication (body Markdown).
    pub notes: String,
    /// URL de téléchargement du binaire Linux x86-64.
    pub download_url: String,
    /// URL de la signature Ed25519 Base64 du binaire exact.
    pub signature_url: String,
}

#[derive(Debug, Clone)]
pub enum UpdateState {
    /// Vérification en cours (thread actif).
    Checking,
    /// Version actuelle déjà à jour.
    UpToDate,
    /// Nouvelle version disponible.
    Available(ReleaseInfo),
    /// Téléchargement en cours (0.0 … 1.0).
    Downloading(f32),
    /// Mise à jour appliquée — redémarrage nécessaire.
    Done,
    /// Erreur non bloquante.
    Error(String),
}

// ---------- Structures JSON de l'API GitHub ----------

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    body: Option<String>,
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

// ---------- Gestion de l'état dans l'UI ----------

/// Gère le cycle de vie de la vérification et du téléchargement.
/// À stocker dans `App` et appelé depuis `update()` chaque frame.
pub struct Updater {
    pub state: UpdateState,
    rx: Option<Receiver<UpdateState>>,
}

impl Updater {
    pub fn new() -> Self {
        Self {
            state: UpdateState::UpToDate,
            rx: None,
        }
    }

    /// Lance la vérification en arrière-plan. Sans effet si déjà en cours.
    pub fn check(&mut self) {
        if matches!(self.state, UpdateState::Checking) {
            return;
        }
        self.state = UpdateState::Checking;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        std::thread::spawn(move || {
            let result = check_latest();
            let _ = tx.send(result);
        });
    }

    /// Injecte directement un état « mise à jour disponible » sans réseau.
    /// Pratique pour tester l'UI sans GitHub. Réservé aux builds de debug, où
    /// se trouve l'entrée de menu « Simuler une mise à jour ».
    #[cfg(debug_assertions)]
    pub fn simulate(&mut self) {
        self.state = UpdateState::Available(ReleaseInfo {
            tag: "v99.0.0".to_string(),
            notes: "## Simulation de mise à jour\n\
                    - Test de la détection de nouvelle version\n\
                    - Test de la barre de progression\n\
                    - Test du remplacement de binaire (mode dry-run)\n\n\
                    *Aucun fichier ne sera modifié.*"
                .to_string(),
            download_url: "simulate://fake-download".to_string(),
            signature_url: "simulate://fake-signature".to_string(),
        });
    }

    /// Lance le téléchargement + remplacement en arrière-plan.
    /// Si l'URL commence par `simulate://`, fait un dry-run avec fausse progression.
    pub fn install(&mut self, info: ReleaseInfo) {
        self.state = UpdateState::Downloading(0.0);
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let url = info.download_url.clone();
        std::thread::spawn(move || {
            let result = if url.starts_with("simulate://") {
                simulate_download(&tx)
            } else {
                download_and_install(&url, &info.signature_url, &tx)
            };
            let _ = tx.send(result);
        });
    }

    /// À appeler chaque frame pour consommer les messages du thread de fond.
    pub fn poll(&mut self) {
        let Some(rx) = &self.rx else { return };
        while let Ok(s) = rx.try_recv() {
            self.state = s;
        }
        if matches!(
            self.state,
            UpdateState::Done
                | UpdateState::Error(_)
                | UpdateState::UpToDate
                | UpdateState::Available(_)
        ) {
            self.rx = None;
        }
    }
}

// ---------- Logique réseau (thread de fond) ----------

/// Simule un téléchargement de 3 secondes avec progression, sans toucher aucun fichier.
fn simulate_download(tx: &Sender<UpdateState>) -> UpdateState {
    let steps = 30u32;
    for i in 1..=steps {
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = tx.send(UpdateState::Downloading(i as f32 / steps as f32));
    }
    UpdateState::Done
}

/// Interroge l'API GitHub et renvoie l'état correspondant.
fn check_latest() -> UpdateState {
    let current = crate::version::SEMVER;

    let body: GhRelease = match ureq::get(&api_url())
        .header("User-Agent", "asm-studio-updater")
        .call()
    {
        Ok(resp) => match resp.into_body().read_json() {
            Ok(r) => r,
            Err(e) => return UpdateState::Error(format!("JSON: {e}")),
        },
        Err(e) => return UpdateState::Error(format!("Réseau: {e}")),
    };

    let remote_ver = body.tag_name.trim_start_matches('v');
    if !is_newer(remote_ver, current) {
        return UpdateState::UpToDate;
    }

    // L'asset de mise à jour est le binaire brut, dont le nom se termine par
    // `-linux-x86_64`. L'archive de distribution porte le même préfixe mais se
    // termine en `.tar.gz` : la télécharger puis la renommer à la place de
    // l'exécutable rendrait l'installation inutilisable.
    let asset = linux_update_asset(&body.assets);

    match asset {
        Some(a) => {
            let signature_name = format!("{}.sig", a.name);
            let Some(signature) = body
                .assets
                .iter()
                .find(|other| other.name == signature_name)
            else {
                return UpdateState::Error(format!(
                    "Release {} trouvée, mais la signature {} est absente.",
                    body.tag_name, signature_name
                ));
            };
            UpdateState::Available(ReleaseInfo {
                tag: body.tag_name,
                notes: body.body.unwrap_or_default(),
                download_url: a.browser_download_url.clone(),
                signature_url: signature.browser_download_url.clone(),
            })
        }
        None => UpdateState::Error(format!(
            "Release {} trouvée mais aucun binaire Linux x86-64 dans les assets.",
            body.tag_name
        )),
    }
}

/// Binaire Linux x86-64 directement remplaçable par l'updater.
///
/// Le suffixe sans extension est volontaire : l'archive destinée aux
/// installations manuelles est `…-linux-x86_64.tar.gz`, donc ne peut jamais
/// être prise pour un exécutable, quel que soit l'ordre des assets GitHub.
fn linux_update_asset(assets: &[GhAsset]) -> Option<&GhAsset> {
    assets.iter().find(|a| {
        a.name
            .to_ascii_lowercase()
            .ends_with("-linux-x86_64")
    })
}

/// Télécharge le binaire, remplace l'exécutable courant, envoie la progression.
fn download_and_install(url: &str, signature_url: &str, tx: &Sender<UpdateState>) -> UpdateState {
    // Le temporaire doit vivre à côté de l'exécutable : le renommage final est
    // alors atomique et ne peut jamais laisser le programme installé tronqué.
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return UpdateState::Error(format!("current_exe: {e}")),
    };

    let signature = match download_signature(signature_url) {
        Ok(signature) => signature,
        Err(e) => return UpdateState::Error(e),
    };

    let resp = match ureq::get(url)
        .header("User-Agent", "asm-studio-updater")
        .call()
    {
        Ok(r) => r,
        Err(e) => return UpdateState::Error(format!("Téléchargement: {e}")),
    };

    // Taille totale (optionnelle, pour la barre de progression).
    let total: Option<u64> = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok());

    let mut reader = resp.into_body().into_reader();
    let (tmp, mut file) = match new_tempfile_beside(&exe) {
        Ok(pair) => pair,
        Err(e) => return UpdateState::Error(e),
    };

    let mut downloaded: u64 = 0;
    let mut buf = [0u8; 8192];
    loop {
        use std::io::Read;
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return UpdateState::Error(format!("Lecture: {e}"));
            }
        };
        use std::io::Write;
        if file.write_all(&buf[..n]).is_err() {
            let _ = std::fs::remove_file(&tmp);
            return UpdateState::Error("Erreur d'écriture disque.".into());
        }
        downloaded += n as u64;
        if let Some(total) = total.filter(|n| *n > 0) {
            let _ = tx.send(UpdateState::Downloading(downloaded as f32 / total as f32));
        }
    }
    if let Err(e) = file.sync_all() {
        let _ = std::fs::remove_file(&tmp);
        return UpdateState::Error(format!("Synchronisation disque: {e}"));
    }
    drop(file);

    if let Err(e) = verify_file_signature(&tmp, &signature) {
        let _ = std::fs::remove_file(&tmp);
        return UpdateState::Error(e);
    }

    // Rendre le binaire exécutable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
    }

    // Le temporaire et la cible partagent leur système de fichiers : sur Linux
    // rename(2) remplace aussi un binaire en cours d'exécution sans fenêtre où
    // l'installation serait absente ou partiellement écrite.
    if let Err(e) = std::fs::rename(&tmp, &exe) {
        let _ = std::fs::remove_file(&tmp);
        return UpdateState::Error(format!("Remplacement atomique: {e}"));
    }

    UpdateState::Done
}

/// Télécharge et décode une signature Ed25519 encodée en Base64.
fn download_signature(url: &str) -> Result<Signature, String> {
    use std::io::Read;

    let response = ureq::get(url)
        .header("User-Agent", "asm-studio-updater")
        .call()
        .map_err(|e| format!("Signature de mise à jour: {e}"))?;
    let mut encoded = String::new();
    response
        .into_body()
        .into_reader()
        .read_to_string(&mut encoded)
        .map_err(|e| format!("Lecture de la signature: {e}"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|_| "Signature de mise à jour mal encodée".to_string())?;
    let raw: [u8; 64] = bytes
        .try_into()
        .map_err(|_| "Signature de mise à jour de longueur incorrecte".to_string())?;
    Ok(Signature::from_bytes(&raw))
}

/// Vérifie le binaire complet avant de le rendre exécutable ou de remplacer
/// l'installation courante.
fn verify_file_signature(path: &Path, signature: &Signature) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Lecture du binaire téléchargé: {e}"))?;
    verify_signature(&bytes, signature, &UPDATE_PUBLIC_KEY)
}

fn verify_signature(
    bytes: &[u8],
    signature: &Signature,
    public_key: &[u8; 32],
) -> Result<(), String> {
    let key = VerifyingKey::from_bytes(public_key)
        .map_err(|_| "Clé publique de mise à jour invalide".to_string())?;
    key.verify(bytes, signature)
        .map_err(|_| "Signature de mise à jour invalide : installation annulée".to_string())
}

// ---------- Utilitaires ----------

/// Compare deux versions semver, préversions comprises (voir
/// [`crate::version::is_newer`], où vit toute la sémantique des numéros).
fn is_newer(remote: &str, local: &str) -> bool {
    crate::version::is_newer(remote, local)
}

/// Crée sans suivre de lien un fichier temporaire unique dans le dossier de
/// l'exécutable. `create_new` est la propriété de sécurité importante : même
/// si un autre processus devine le nom, il ne peut pas nous faire écraser un
/// fichier ou suivre un lien symbolique.
fn new_tempfile_beside(exe: &Path) -> Result<(PathBuf, std::fs::File), String> {
    use std::fs::OpenOptions;

    let dir = exe
        .parent()
        .ok_or_else(|| "exécutable sans dossier parent".to_string())?;
    for attempt in 0..128u32 {
        let name = format!(
            ".asm-studio-update-{}-{}-{attempt}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        );
        let path = dir.join(name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Création du fichier temporaire: {e}")),
        }
    }
    Err("Impossible de réserver un fichier temporaire unique.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn version_comparison() {
        assert!(is_newer("0.4.0", "0.3.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("0.2.9", "0.3.0"));
        // Une bêta n'est pas une mise à jour de sa propre finale.
        assert!(!is_newer("0.4.7-beta.1", "0.4.7"));
    }

    #[test]
    fn raw_linux_binary_is_preferred_to_distribution_archive() {
        let asset = |name: &str| GhAsset {
            name: name.to_string(),
            browser_download_url: format!("https://example.invalid/{name}"),
        };
        let assets = vec![
            asset("asm-studio-0.4.8-beta.8-linux-x86_64.tar.gz"),
            asset("asm-studio-0.4.8-beta.8-linux-x86_64"),
            asset("asm-studio-0.4.8-beta.8-linux-x86_64.sig"),
        ];
        assert_eq!(
            linux_update_asset(&assets).map(|a| a.name.as_str()),
            Some("asm-studio-0.4.8-beta.8-linux-x86_64")
        );
    }

    #[test]
    fn update_temporary_file_is_created_next_to_the_executable() {
        let dir = PathBuf::from("build/updater-temp-test");
        std::fs::create_dir_all(&dir).expect("dossier de test");
        let exe = dir.join("asm_studio");
        let (first, first_file) = new_tempfile_beside(&exe).expect("premier temporaire");
        let (second, second_file) = new_tempfile_beside(&exe).expect("second temporaire");

        assert_eq!(first.parent(), Some(dir.as_path()));
        assert_eq!(second.parent(), Some(dir.as_path()));
        assert_ne!(
            first, second,
            "deux réservations ne se partagent jamais un nom"
        );

        drop(first_file);
        drop(second_file);
        std::fs::remove_file(first).expect("suppression premier temporaire");
        std::fs::remove_file(second).expect("suppression second temporaire");
    }

    #[test]
    fn only_the_matching_ed25519_signature_is_accepted() {
        let signing_key = SigningKey::from_bytes(&[11; 32]);
        let bytes = b"binaire de mise a jour";
        let signature = signing_key.sign(bytes);
        let public_key = signing_key.verifying_key().to_bytes();

        assert!(verify_signature(bytes, &signature, &public_key).is_ok());
        assert!(verify_signature(b"binaire modifie", &signature, &public_key).is_err());
    }
}
