//! Cycle de vie de la VM Linux qui héberge `ld`+ptrace sur macOS (voir
//! `src/vm_debugger.rs` et le plan de portage). Modelé sur
//! [`crate::winerun::WineRun`] : démarrage, sonde de santé, `Drop` qui ne
//! laisse jamais le processus survivre à l'IDE.
//!
//! Démarrer une VM prend de l'ordre de la dizaine de secondes (mesuré en
//! spike, sur une image déjà démarrée plusieurs fois — une image fraîchement
//! installée peut prendre nettement plus longtemps avant que l'agent
//! réponde vraiment, voir `wait_for_agent`) : ça ne doit jamais geler
//! l'image egui. Le sondage de disponibilité tourne donc sur un thread
//! dédié, et l'UI ne fait que lire un état partagé à chaque frame
//! (`status()`), sur le même principe que `WinDebugger::available()`.

use std::io::BufReader;
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::vm_protocol::{self, AGENT_PORT, Request, Response};

/// Délai au-delà duquel un boot qui ne répond toujours pas est considéré en
/// échec plutôt que simplement lent. Le noyau démarre en une douzaine de
/// secondes, mais l'agent lui-même (l'aller-retour Ping → Pong qui compte
/// vraiment, voir `probe_once`) peut prendre nettement plus longtemps sur
/// une image fraîchement installée — mesuré jusqu'à ~35 s — d'où une marge
/// large plutôt que les ~12 s du seul boot noyau.
const BOOT_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Clone)]
pub enum VmStatus {
    NotStarted,
    Booting,
    Ready,
    Failed(String),
}

/// Emplacement de l'image construite par `install/build-vm-image.sh` — voir
/// ce script pour la construction. Sous le dossier de support applicatif de
/// l'utilisateur, jamais dans le bundle (l'image se reconstruit, un binaire
/// signé/notarisé non).
pub fn image_path() -> PathBuf {
    let base = dirs_support_dir();
    base.join("ASM Studio").join("vm").join("asmstudio-vm.qcow2")
}

fn dirs_support_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library/Application Support")
}

/// Vrai si `qemu-system-x86_64` est sur le `PATH`. Sondé à la demande plutôt
/// que mis en cache : comme pour `winerun::available`, installer QEMU
/// pendant que l'IDE tourne doit suffire à en profiter sans redémarrage.
pub fn qemu_available() -> bool {
    Command::new("qemu-system-x86_64")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

pub struct VmSession {
    status: Arc<Mutex<VmStatus>>,
    child: Option<Child>,
}

impl VmSession {
    /// Démarre la VM en tâche de fond. Ne bloque pas : le boot (une
    /// douzaine de secondes pour le noyau, potentiellement bien plus avant
    /// que l'agent réponde vraiment sur une image neuve — voir
    /// `BOOT_TIMEOUT`) se déroule sur un thread séparé qui met `status` à
    /// jour ; l'appelant poll cet état à chaque frame (voir
    /// `App::poll_vm_boot`).
    pub fn start() -> Self {
        let status = Arc::new(Mutex::new(VmStatus::NotStarted));
        if !qemu_available() {
            *status.lock().unwrap() = VmStatus::Failed(
                "QEMU introuvable (qemu-system-x86_64). Installez-le : brew install qemu".into(),
            );
            return VmSession { status, child: None };
        }
        let image = image_path();
        if !image.is_file() {
            *status.lock().unwrap() = VmStatus::Failed(format!(
                "image VM absente ({}) — lancez install/build-vm-image.sh",
                image.display()
            ));
            return VmSession { status, child: None };
        }

        let child = Command::new("qemu-system-x86_64")
            .args(["-M", "q35", "-m", "1024", "-smp", "2"])
            .args(["-display", "none"])
            .arg("-drive")
            .arg(format!("file={},if=virtio,format=qcow2", image.display()))
            .arg("-netdev")
            .arg(format!("user,id=n0,hostfwd=tcp:127.0.0.1:{AGENT_PORT}-:{AGENT_PORT}"))
            .args(["-device", "virtio-net-pci,netdev=n0"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        let child = match child {
            Ok(c) => c,
            Err(e) => {
                *status.lock().unwrap() = VmStatus::Failed(format!("qemu-system-x86_64 n'a pas démarré : {e}"));
                return VmSession { status, child: None };
            }
        };

        *status.lock().unwrap() = VmStatus::Booting;
        let probe_status = Arc::clone(&status);
        std::thread::spawn(move || wait_for_agent(probe_status));

        VmSession { status, child: Some(child) }
    }

    pub fn status(&self) -> VmStatus {
        self.status.lock().unwrap().clone()
    }

    /// Adresse de l'agent, seulement une fois la VM prête. N'est utile qu'à
    /// l'affichage — [`crate::vm_debugger`] et `crate::assemble` se
    /// connectent directement au port bien connu ([`AGENT_PORT`]) sans passer
    /// par `VmSession` : une connexion refusée dit déjà tout ce qu'il y a à
    /// savoir sur une VM pas prête, pas besoin de vérifier ce statut d'abord.
    pub fn agent_addr(&self) -> Option<SocketAddr> {
        match self.status() {
            VmStatus::Ready => Some(SocketAddr::from(([127, 0, 0, 1], AGENT_PORT))),
            _ => None,
        }
    }
}

impl Drop for VmSession {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn wait_for_agent(status: Arc<Mutex<VmStatus>>) {
    let deadline = Instant::now() + BOOT_TIMEOUT;
    let addr = SocketAddr::from(([127, 0, 0, 1], AGENT_PORT));
    while Instant::now() < deadline {
        if probe_once(&addr) {
            *status.lock().unwrap() = VmStatus::Ready;
            return;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    *status.lock().unwrap() =
        VmStatus::Failed("la VM n'a pas répondu à temps (démarrage trop long ou image cassée)".into());
}

/// Un simple `connect()` réussit dès que QEMU démarre : son réseau
/// utilisateur (SLIRP) accepte la connexion côté hôte avant même que
/// l'invité ait fini de démarrer, ce qui rendait `Ready` prématuré —
/// mesuré : jusqu'à une trentaine de secondes d'écart entre un `connect()`
/// qui réussit et l'agent qui répond vraiment sur une image fraîchement
/// installée. Un aller-retour complet du protocole (`Ping` → `Pong`) est le
/// seul signal fiable que l'agent tourne, pas seulement que QEMU existe.
fn probe_once(addr: &SocketAddr) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(addr, Duration::from_millis(500)) else {
        return false;
    };
    if stream.set_read_timeout(Some(Duration::from_millis(800))).is_err()
        || stream.set_write_timeout(Some(Duration::from_millis(500))).is_err()
    {
        return false;
    }
    if vm_protocol::write_message(&mut stream, &Request::Ping).is_err() {
        return false;
    }
    let mut reader = BufReader::new(stream);
    matches!(vm_protocol::read_message::<Response>(&mut reader), Ok(Response::Pong))
}
