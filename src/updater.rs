//! Mise à jour automatique : vérifie GitHub Releases, télécharge et remplace le binaire.
//!
//! La vérification tourne dans un thread de fond pour ne pas bloquer l'UI.
//! Le résultat est récupéré dans `update()` via un canal `mpsc`.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

use serde::Deserialize;

/// Dépôt GitHub source des releases.
const GITHUB_REPO: &str = "fredza/asm-studio";

/// URL de l'API GitHub Releases.
fn api_url() -> String {
    format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest")
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
        Self { state: UpdateState::UpToDate, rx: None }
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
                download_and_install(&url, &tx)
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
        if matches!(self.state, UpdateState::Done | UpdateState::Error(_) | UpdateState::UpToDate | UpdateState::Available(_)) {
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
    let current = env!("CARGO_PKG_VERSION");

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

    // Cherche le binaire Linux x86-64 dans les assets.
    let asset = body.assets.iter().find(|a| {
        let n = a.name.to_lowercase();
        n.contains("linux") && n.contains("x86_64") && !n.ends_with(".sha256")
    });

    match asset {
        Some(a) => UpdateState::Available(ReleaseInfo {
            tag: body.tag_name,
            notes: body.body.unwrap_or_default(),
            download_url: a.browser_download_url.clone(),
        }),
        None => UpdateState::Error(format!(
            "Release {} trouvée mais aucun binaire Linux x86-64 dans les assets.",
            body.tag_name
        )),
    }
}

/// Télécharge le binaire, remplace l'exécutable courant, envoie la progression.
fn download_and_install(url: &str, tx: &Sender<UpdateState>) -> UpdateState {
    // Téléchargement dans un fichier temporaire.
    let tmp = match tempfile_path() {
        Some(p) => p,
        None => return UpdateState::Error("Impossible de créer un fichier temporaire.".into()),
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
    let mut file = match std::fs::File::create(&tmp) {
        Ok(f) => f,
        Err(e) => return UpdateState::Error(format!("Écriture: {e}")),
    };

    let mut downloaded: u64 = 0;
    let mut buf = [0u8; 8192];
    loop {
        use std::io::Read;
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => return UpdateState::Error(format!("Lecture: {e}")),
        };
        use std::io::Write;
        if file.write_all(&buf[..n]).is_err() {
            return UpdateState::Error("Erreur d'écriture disque.".into());
        }
        downloaded += n as u64;
        if let Some(total) = total {
            let _ = tx.send(UpdateState::Downloading(downloaded as f32 / total as f32));
        }
    }
    drop(file);

    // Rendre le binaire exécutable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
    }

    // Remplacer l'exécutable courant de façon atomique.
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return UpdateState::Error(format!("current_exe: {e}")),
    };
    // Sur Linux on peut remplacer un binaire en cours d'exécution grâce au rename(2).
    if let Err(e) = std::fs::rename(&tmp, &exe) {
        // rename peut échouer entre partitions → copie de secours.
        if let Err(e2) = std::fs::copy(&tmp, &exe) {
            return UpdateState::Error(format!("Remplacement: {e} / copie: {e2}"));
        }
        let _ = std::fs::remove_file(&tmp);
    }

    UpdateState::Done
}

// ---------- Utilitaires ----------

/// Compare deux chaînes de version "X.Y.Z".
/// Renvoie `true` si `remote > local`.
fn is_newer(remote: &str, local: &str) -> bool {
    let parse = |s: &str| -> (u32, u32, u32) {
        let mut it = s.split('.').filter_map(|p| p.parse().ok());
        (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
    };
    parse(remote) > parse(local)
}

fn tempfile_path() -> Option<PathBuf> {
    let mut p = std::env::temp_dir();
    p.push(format!("asm_studio_update_{}", std::process::id()));
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("0.4.0", "0.3.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("0.2.9", "0.3.0"));
    }
}
