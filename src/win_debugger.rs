//! Pas-à-pas sur un exécutable PE64, via Wine et le protocole GDB Remote
//! Serial Protocol (RSP).
//!
//! [`crate::winerun`] fait déjà tourner un `.exe` sous Wine, mais sans le
//! déboguer : le débogueur `ptrace` de [`crate::debugger`] suit les adresses
//! du binaire qu'il a lui-même lancé, et celles d'un programme Windows
//! derrière le chargeur de Wine n'ont plus rien à voir avec celles du
//! listing. Ce module résout ce problème autrement : `winedbg --gdb` sait
//! s'arrêter avant la première instruction d'un `.exe` et parler un protocole
//! standard sur un port TCP local. Ici, on parle ce protocole nous-mêmes —
//! sans dépendre d'un vrai binaire `gdb` installé sur la machine — pour lire
//! les mêmes registres et la même mémoire que le débogueur Linux, avec les
//! mêmes types ([`Registers`], [`FpRegisters`]).
//!
//! Deux écarts avec `winedbg` amont, découverts à l'usage et contournés ici :
//!
//! * un cache `debuginfod` local abîmé fait planter `winedbg` sur une
//!   assertion interne (`dlls/dbghelp/dwarf.c`) — on coupe donc
//!   `DEBUGINFOD_URLS` pour l'enfant, qui n'en a de toute façon aucun besoin ;
//! * les paquets `Z0`/`z0` (points d'arrêt logiciels du protocole) ne sont
//!   pas implémentés par ce `winedbg` : une réponse vide au lieu de `OK`. Les
//!   points d'arrêt sont donc posés à la main ([`WinDebugger::set_breakpoint`])
//!   en écrivant l'octet `0xCC` (`INT3`) directement en mémoire, exactement ce
//!   qu'un vrai débogueur ferait pour son propre compte.
//!
//! Volontairement absent de cette première version : la sortie du programme.
//! `winedbg --gdb` redirige le `stdout` du débogué vers `/dev/null` (son
//! `stderr`, lui, reste branché — mais ce sont les diagnostics de Wine, pas
//! ceux du programme), et le protocole RSP ne relaie aucun paquet `O` pour la
//! compenser — vérifié : aucun n'apparaît, même en continuant jusqu'à la fin
//! du programme. Rendre la sortie visible demandera d'intercepter les appels
//! à `WriteFile`/`WriteConsoleA` par point d'arrêt (leur adresse d'IAT est
//! déjà connue de [`crate::pe_link`], qui a construit le `.exe`) plutôt que de
//! compter sur un canal que Wine ne fournit pas ici. Ce module ne le fait pas
//! encore : ce qu'il ne peut pas montrer, il ne le simule pas.
//!
//! Branché à l'interface par `app::win_debug_ops` et sa fenêtre dédiée
//! (`app::ui_win_debug`) : pas à pas, continuer, arrêter — un premier
//! panneau volontairement minimal, qui ne sollicite pas encore toute l'API
//! ci-dessous (`fp_regs`, `write_mem`, marqués `#[allow(dead_code)]` là où
//! ils sont définis plutôt que par un `allow` de module, maintenant que le
//! reste sert vraiment).
//!
//! Toutes les méthodes ci-dessous sont **bloquantes** : le protocole RSP est
//! un échange requête/réponse, et [`WinDebugger::cont`] ne rend la main que
//! lorsque le débogué s'arrête à nouveau. `app::win_debug_ops` ne les appelle
//! donc jamais depuis le fil de l'interface : il confie le `WinDebugger` à un
//! thread dédié pour la durée de la session. Ce module lui fournit de quoi
//! couper court à une attente qui s'éternise ([`WinDbgInterrupt`]), la seule
//! chose qu'un thread extérieur puisse faire sur une connexion synchrone.
//!
//! Seule exception, et elle a coûté cher : [`WinDebugger::available`] est
//! appelée par la barre d'outils, donc depuis le fil de l'interface, à chaque
//! frame. Elle lançait deux processus Wine à chaque fois — voir
//! [`AVAILABILITY_TTL`] pour ce que ça donnait, et pourquoi sa réponse est
//! désormais mémorisée.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use object::Object;

use crate::debugger::{Flags, FpRegisters, Registers};
use crate::i18n::{self, Lang};

/// Ce qui peut empêcher le débogueur Windows de faire ce qu'on lui demande.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WinDbgError {
    /// Wine n'est pas installé, ou `winedbg` est introuvable sur le `PATH`.
    WineMissing,
    /// Chemin du binaire non représentable en UTF-8.
    BadPath,
    /// `winedbg` n'a pas ouvert son port à temps : rien n'a démarré.
    NoStart,
    /// La connexion TCP locale au port RSP a échoué.
    ConnectFailed(String),
    /// Réponse du protocole absente, tronquée, ou inattendue à ce moment.
    Protocol(String),
    /// Le processus n'est pas arrêté : ni pas, ni lecture/écriture possibles.
    NotStopped,
    /// Nom de registre inconnu.
    UnknownRegister(String),
    /// La session a été coupée volontairement pendant l'attente (« Arrêter »
    /// alors qu'une commande n'avait pas encore répondu) — voir
    /// [`WinDbgInterrupt`].
    Interrupted,
    /// `winedbg` n'a rien répondu dans le temps imparti. Le cas courant n'est
    /// pas une panne mais une attente : le programme débogué est arrêté sur
    /// quelque chose que le protocole ne sait pas nous montrer — une boîte de
    /// dialogue modale, une saisie clavier — et personne n'y a répondu.
    Timeout,
}

impl WinDbgError {
    pub fn message(&self, lang: Lang) -> String {
        let tr = |fr, en, es| i18n::tr3(lang, fr, en, es);
        match self {
            WinDbgError::WineMissing => tr(
                "Wine n'est pas installé (nécessaire pour déboguer un .exe)",
                "Wine is not installed (required to debug a .exe)",
                "Wine no está instalado (necesario para depurar un .exe)",
            )
            .to_string(),
            WinDbgError::BadPath => tr(
                "chemin du binaire illisible (non-UTF8)",
                "unreadable binary path (not UTF-8)",
                "ruta del binario ilegible (no UTF-8)",
            )
            .to_string(),
            WinDbgError::NoStart => tr(
                "winedbg n'a pas démarré à temps",
                "winedbg did not start in time",
                "winedbg no arrancó a tiempo",
            )
            .to_string(),
            WinDbgError::ConnectFailed(e) => format!(
                "{}: {e}",
                tr(
                    "connexion au débogueur Wine impossible",
                    "could not connect to the Wine debugger",
                    "no se pudo conectar con el depurador de Wine",
                )
            ),
            WinDbgError::Protocol(e) => format!(
                "{}: {e}",
                tr(
                    "réponse inattendue de winedbg",
                    "unexpected reply from winedbg",
                    "respuesta inesperada de winedbg",
                )
            ),
            WinDbgError::NotStopped => tr(
                "le processus n'est pas arrêté",
                "the process is not stopped",
                "el proceso no está detenido",
            )
            .to_string(),
            WinDbgError::UnknownRegister(name) => {
                format!("{} : {name}", tr("registre inconnu", "unknown register", "registro desconocido"))
            }
            WinDbgError::Interrupted => tr(
                "session de débogage interrompue",
                "debugging session interrupted",
                "sesión de depuración interrumpida",
            )
            .to_string(),
            WinDbgError::Timeout => tr(
                "le programme n'a pas repris la main. S'il affichait une boîte de dialogue, \
                 il fallait y répondre : winedbg ne peut pas le faire à votre place, et il \
                 n'attend pas indéfiniment. La session est refermée — relancez « Démarrer ».",
                "the program never came back. If it was showing a dialog box, it had to be \
                 answered: winedbg cannot do it for you, and it does not wait forever. The \
                 session is closed — hit “Start” again.",
                "el programa no devolvió el control. Si mostraba un cuadro de diálogo, había \
                 que responderlo: winedbg no puede hacerlo por usted, y no espera \
                 indefinidamente. La sesión se cerró — pulse «Iniciar» de nuevo.",
            )
            .to_string(),
        }
    }
}

pub type WinDbgResult<T> = std::result::Result<T, WinDbgError>;

/// De quoi couper court à une attente en cours, depuis un autre thread que
/// celui qui possède le [`WinDebugger`].
///
/// Le besoin vient d'un cas très concret : le débogué appelle `MessageBoxA`,
/// Windows affiche une boîte modale et n'en sort qu'au clic sur « OK ». Le
/// `cont()` parti là-dedans attend sa réponse RSP jusqu'à [`CONT_TIMEOUT`] —
/// deux minutes, exprès, pour laisser à l'élève le temps de répondre à la
/// boîte. Sans cette poignée, ce seraient deux minutes de session qui ne
/// répond plus à « Arrêter ». Et rien ne peut être *envoyé* sur cette
/// connexion pour l'abréger : ce `winedbg` n'implémente pas l'interruption
/// asynchrone (`\x03`) du protocole.
///
/// La seule coupure fiable est donc de tuer `winedbg` lui-même : le serveur
/// disparu, la connexion TCP se ferme, et la lecture bloquée côté client
/// échoue immédiatement — le thread propriétaire redevient libre de relâcher
/// sa session. C'est ce que fait [`Self::interrupt`], et c'est aussi ce qui
/// évite de laisser derrière soi un `winedbg` et un programme débogué qui
/// continueraient de tourner, invisibles.
#[derive(Clone, Default)]
pub struct WinDbgInterrupt {
    /// PID du `winedbg` de la session, tant qu'il n'a pas été moissonné.
    ///
    /// Sous mutex plutôt qu'en atomique : le `Drop` du [`WinDebugger`] doit
    /// pouvoir le désarmer *avant* son `wait()`, sans qu'un `interrupt()`
    /// concurrent puisse viser entre-temps un PID que l'OS aurait déjà
    /// recyclé pour un tout autre processus.
    pid: Arc<Mutex<Option<u32>>>,
    /// Posé par [`Self::interrupt`], lu par les attentes qui ne sont pas des
    /// lectures socket — au lancement, la boucle qui guette l'ouverture du
    /// port ne verrait sinon rien du tout et tournerait jusqu'à son délai.
    cancelled: Arc<AtomicBool>,
}

impl WinDbgInterrupt {
    pub fn new() -> Self {
        Self::default()
    }

    /// Verrou pris sans se soucier de l'empoisonnement : un thread qui aurait
    /// paniqué en tenant ce mutex n'y laisse qu'un `Option<u32>`, toujours
    /// cohérent — refuser d'y toucher ne ferait que rendre l'arrêt impossible.
    fn slot(&self) -> std::sync::MutexGuard<'_, Option<u32>> {
        self.pid.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn arm(&self, pid: u32) {
        *self.slot() = Some(pid);
    }

    fn disarm(&self) {
        *self.slot() = None;
    }

    /// Coupe la session : `winedbg` est tué, toute lecture bloquée sur sa
    /// connexion échoue dans la foulée. Rend vrai si un processus a bien été
    /// visé (faux si la session n'était pas encore lancée, ou déjà finie).
    pub fn interrupt(&self) -> bool {
        self.cancelled.store(true, Ordering::SeqCst);
        let slot = self.slot();
        let Some(pid) = *slot else { return false };
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGKILL,
        )
        .is_ok()
    }

    /// La session a-t-elle été coupée volontairement ? Ce qui permet à
    /// l'appelant de ne pas rapporter comme une panne l'échec de lecture
    /// qu'il vient lui-même de provoquer.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

fn io_err(what: &'static str) -> impl Fn(std::io::Error) -> WinDbgError {
    move |e| WinDbgError::Protocol(format!("{what}: {e}"))
}

/// État de vie du processus tracé, côté RSP.
///
/// Pas d'équivalent à `RunState::Running` de [`crate::debugger`] : les
/// commandes RSP sont synchrones (on attend la réponse), et ce module ne
/// relaie pas encore l'entrée standard — un programme qui bloque dessus
/// bloque donc l'appel en cours, borné par [`REQUEST_TIMEOUT`] ou abrégé par
/// [`WinDbgInterrupt`]. « En cours » se lit donc du côté de l'appelant (le
/// thread de session de `app::win_debug_ops`), pas ici.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WinRunState {
    Stopped,
    Exited(i32),
    Signaled,
}

/// Un point d'arrêt posé à la main : l'octet original, pour le restaurer.
struct Bp {
    addr: u64,
    orig: u8,
    /// Faux juste après avoir été franchi (le temps de rejouer l'instruction
    /// d'origine) — sans quoi [`WinDebugger::cont`] retriggerait sans avancer.
    armed: bool,
}

/// Délai maximal d'attente d'une réponse RSP. Une lecture de registres ou de
/// mémoire répond en microsecondes : ce plafond ne sert qu'à ne jamais rester
/// bloqué pour toujours sur une connexion devenue muette.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Délai accordé au seul paquet qui peut légitimement mettre longtemps :
/// `vCont;c`, qui ne répond que lorsque le programme s'arrête à nouveau.
///
/// Vingt secondes suffisaient tant qu'on ne pensait qu'aux boucles de calcul.
/// Mais un programme Windows qui appelle `MessageBoxA` attend un humain :
/// l'élève lit la boîte, cherche le bouton, clique. Vérifié à l'usage — il y
/// passe couramment plus de vingt secondes, et la session mourait alors sous
/// ses yeux sur un « Resource temporarily unavailable (os error 11) » qui ne
/// veut rien dire pour lui. Deux minutes lui laissent le temps de répondre,
/// et restent un plafond : rien ne peut attendre indéfiniment ici.
const CONT_TIMEOUT: Duration = Duration::from_secs(120);

/// Délai laissé à `winedbg` pour ouvrir son port avant d'abandonner.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(8);

/// Une connexion RSP : trame les paquets `$...#xx`, accuse réception de ceux
/// reçus, et rend leur charge utile débarrassée de l'enveloppe.
struct RspConn {
    sock: TcpStream,
    buf: Vec<u8>,
}

impl RspConn {
    fn connect(port: u16, deadline: Instant, interrupt: &WinDbgInterrupt) -> WinDbgResult<Self> {
        loop {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(sock) => {
                    sock.set_read_timeout(Some(REQUEST_TIMEOUT))
                        .map_err(io_err("réglage du délai"))?;
                    sock.set_nodelay(true).ok();
                    return Ok(RspConn { sock, buf: Vec::new() });
                }
                Err(e) => {
                    // Seul endroit du lancement où tuer `winedbg` ne se voit
                    // pas : il n'y a pas encore de socket dont la fermeture
                    // nous réveillerait. On regarde donc le drapeau, sans quoi
                    // « Arrêter » attendrait les huit secondes de démarrage.
                    if interrupt.is_cancelled() {
                        return Err(WinDbgError::Interrupted);
                    }
                    if Instant::now() >= deadline {
                        return Err(WinDbgError::ConnectFailed(e.to_string()));
                    }
                    std::thread::sleep(Duration::from_millis(30));
                }
            }
        }
    }

    fn send(&mut self, payload: &str) -> WinDbgResult<()> {
        let data = payload.as_bytes();
        let sum: u32 = data.iter().map(|&b| u32::from(b)).sum();
        let mut pkt = Vec::with_capacity(data.len() + 4);
        pkt.push(b'$');
        pkt.extend_from_slice(data);
        pkt.push(b'#');
        pkt.extend_from_slice(format!("{:02x}", sum & 0xff).as_bytes());
        self.sock.write_all(&pkt).map_err(io_err("écriture RSP"))
    }

    /// Détache le premier paquet complet du tampon, s'il y en a un — en
    /// jetant au passage les octets d'accusé de réception (`+`/`-`) qui le
    /// précèdent.
    fn take_packet(&mut self) -> Option<Vec<u8>> {
        let start = self.buf.iter().position(|&b| b != b'+' && b != b'-')?;
        if self.buf[start] != b'$' {
            // Paquet mal formé (ne devrait pas arriver avec ce serveur) :
            // on jette l'octet fautif plutôt que de bloquer indéfiniment.
            self.buf.drain(..=start);
            return None;
        }
        let hash = self.buf[start..].iter().position(|&b| b == b'#')? + start;
        if self.buf.len() < hash + 3 {
            return None; // somme de contrôle pas encore arrivée en entier
        }
        let payload = self.buf[start + 1..hash].to_vec();
        self.buf.drain(..hash + 3);
        Some(payload)
    }

    fn recv(&mut self) -> WinDbgResult<Vec<u8>> {
        loop {
            if let Some(pkt) = self.take_packet() {
                // Accusé de réception : ce serveur reste en mode acquitté
                // (il n'annonce `QStartNoAckMode` qu'en capacité, pas en
                // défaut), donc chaque paquet reçu attend le sien.
                self.sock.write_all(b"+").map_err(io_err("accusé RSP"))?;
                return Ok(pkt);
            }
            let mut tmp = [0u8; 4096];
            let n = loop {
                // Un signal reçu pendant le `read` (observé sous test, quand
                // plusieurs sessions Wine tournent en parallèle) ne veut rien
                // dire de la connexion elle-même : on retente, comme le ferait
                // n'importe quel appel bloquant robuste à `EINTR`.
                match self.sock.read(&mut tmp) {
                    Ok(n) => break n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    // Le délai de lecture qui expire arrive en `WouldBlock`
                    // (Linux) ou `TimedOut` (autres) : ce n'est pas une panne
                    // du protocole mais une attente qu'on abrège, et le dire
                    // en ces termes est tout ce qui sépare un message utile
                    // d'un « os error 11 » incompréhensible.
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        return Err(WinDbgError::Timeout);
                    }
                    Err(e) => return Err(io_err("lecture RSP")(e)),
                }
            };
            if n == 0 {
                return Err(WinDbgError::Protocol(
                    "connexion RSP fermée par winedbg".to_string(),
                ));
            }
            self.buf.extend_from_slice(&tmp[..n]);
        }
    }

    fn request(&mut self, payload: &str) -> WinDbgResult<Vec<u8>> {
        self.send(payload)?;
        self.recv()
    }

    /// Comme [`Self::request`], mais en accordant [`CONT_TIMEOUT`] à la
    /// réponse. Réservé à la reprise d'exécution : elle seule peut attendre
    /// un geste humain (voir [`CONT_TIMEOUT`]).
    fn request_running(&mut self, payload: &str) -> WinDbgResult<Vec<u8>> {
        self.sock
            .set_read_timeout(Some(CONT_TIMEOUT))
            .map_err(io_err("réglage du délai"))?;
        let out = self.request(payload);
        // Remis quoi qu'il arrive : les lectures suivantes (registres,
        // mémoire) n'ont aucune raison d'attendre deux minutes, et une erreur
        // ici ne doit pas laisser la connexion avec un plafond de travers.
        let _ = self.sock.set_read_timeout(Some(REQUEST_TIMEOUT));
        out
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(s: &[u8]) -> WinDbgResult<Vec<u8>> {
    let text = std::str::from_utf8(s)
        .map_err(|_| WinDbgError::Protocol("octets hexadécimaux invalides".to_string()))?;
    if text.len() % 2 != 0 {
        return Err(WinDbgError::Protocol("longueur hexadécimale impaire".to_string()));
    }
    (0..text.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&text[i..i + 2], 16)
                .map_err(|_| WinDbgError::Protocol(format!("octet invalide: {}", &text[i..i + 2])))
        })
        .collect()
}

/// Longueur du paquet `g` que ce `winedbg` rend pour une cible x86-64 :
/// 17 registres généraux (rax…r15, rip) de 8 octets, eflags sur 4, six
/// sélecteurs de segment sur 2 chacun, huit registres x87 de 10 octets, les
/// mots de contrôle FPU, seize XMM de 16 octets et enfin mxcsr sur 4 —
/// vérifiée en comparant RIP au point d'entrée connu du lieur, puis en
/// confirmant que la disposition retenue épuise exactement les 511 octets
/// rendus, sans reste ni débordement.
const G_PACKET_MIN_LEN: usize = 140; // au moins les registres généraux + eflags
const G_PACKET_FULL_LEN: usize = 511; // avec x87/XMM/mxcsr

/// Index RSP (ordre du paquet `g`) des registres 64 bits, dans l'ordre de
/// [`Registers::named`] — pour `P<index>=<valeur>`.
fn reg_index(name: &str) -> Option<u32> {
    let i = match name {
        "RAX" => 0,
        "RBX" => 1,
        "RCX" => 2,
        "RDX" => 3,
        "RSI" => 4,
        "RDI" => 5,
        "RBP" => 6,
        "RSP" => 7,
        "R8" => 8,
        "R9" => 9,
        "R10" => 10,
        "R11" => 11,
        "R12" => 12,
        "R13" => 13,
        "R14" => 14,
        "R15" => 15,
        "RIP" => 16,
        _ => return None,
    };
    Some(i)
}

fn registers_from_g(bytes: &[u8]) -> WinDbgResult<(Registers, Option<FpRegisters>)> {
    if bytes.len() < G_PACKET_MIN_LEN {
        return Err(WinDbgError::Protocol(format!(
            "paquet g trop court ({} octets)",
            bytes.len()
        )));
    }
    let u64_at = |off: usize| u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());
    let regs = Registers {
        rax: u64_at(0),
        rbx: u64_at(8),
        rcx: u64_at(16),
        rdx: u64_at(24),
        rsi: u64_at(32),
        rdi: u64_at(40),
        rbp: u64_at(48),
        rsp: u64_at(56),
        r8: u64_at(64),
        r9: u64_at(72),
        r10: u64_at(80),
        r11: u64_at(88),
        r12: u64_at(96),
        r13: u64_at(104),
        r14: u64_at(112),
        r15: u64_at(120),
        rip: u64_at(128),
        eflags: u32::from_le_bytes(bytes[136..140].try_into().unwrap()) as u64,
    };
    let fp = (bytes.len() >= G_PACKET_FULL_LEN).then(|| {
        let mut xmm = [0u128; 16];
        for (i, slot) in xmm.iter_mut().enumerate() {
            let off = 251 + i * 16;
            *slot = u128::from_le_bytes(bytes[off..off + 16].try_into().unwrap());
        }
        let mut st = [[0u8; 10]; 8];
        for (i, slot) in st.iter_mut().enumerate() {
            let off = 152 + i * 10;
            slot.copy_from_slice(&bytes[off..off + 10]);
        }
        FpRegisters {
            xmm,
            st,
            fcw: u16::from_le_bytes(bytes[232..234].try_into().unwrap()),
            fsw: u16::from_le_bytes(bytes[234..236].try_into().unwrap()),
            ftw: bytes[236],
            mxcsr: u32::from_le_bytes(bytes[507..511].try_into().unwrap()),
        }
    });
    Ok((regs, fp))
}

/// Combien de temps la réponse de [`WinDebugger::available`] reste valable.
///
/// Cette recherche n'est pas un test de `PATH` : elle lance deux vrais
/// processus Wine (`wine --version`, `winedbg --help`), soit ~140 ms à chaque
/// appel. L'interface l'appelait depuis la barre d'outils, donc à **chaque
/// frame** dès que la cible était Windows — et le pas-à-pas Windows redemande
/// une frame toutes les 30 ms tant qu'une commande est en vol. De quoi saturer
/// un cœur, figer l'IDE, et faire défiler dans `ps` un flot de `start.exe
/// /exec winedbg` éphémères aux PID toujours nouveaux, à côté de celui de la
/// session : c'est exactement le « winedbg qui se relance en boucle » signalé
/// après un clic sur une boîte de dialogue.
///
/// Le résultat est donc mémorisé quelques secondes. Assez pour qu'une frame ne
/// coûte plus rien ; assez peu pour tenir la promesse d'origine — installer
/// Wine pendant que l'IDE tourne suffit à s'en servir, sans le redémarrer.
const AVAILABILITY_TTL: Duration = Duration::from_secs(5);

/// Dernière réponse de [`WinDebugger::available`], avec sa date.
static AVAILABILITY: Mutex<Option<(bool, Instant)>> = Mutex::new(None);

/// Un débogueur PE64 sous Wine, piloté via `winedbg --gdb` et RSP.
pub struct WinDebugger {
    /// Le processus `winedbg` lui-même (pas le débogué, que Wine gère à part
    /// dans `wineserver` — le tuer met fin au débogué par les mêmes règles
    /// Windows qu'un vrai débogueur qui se détache brutalement).
    child: Child,
    conn: RspConn,
    pub state: WinRunState,
    regs: Registers,
    fp: Option<FpRegisters>,
    breakpoints: Vec<Bp>,
    /// Armée sur le PID de `child` : c'est par elle qu'un autre thread peut
    /// abréger une attente (voir [`WinDbgInterrupt`]). Conservée ici pour la
    /// désarmer au `Drop`, avant de moissonner le processus.
    interrupt: WinDbgInterrupt,
}

impl WinDebugger {
    /// Wine et `winedbg` sont-ils utilisables ?
    ///
    /// Appelable depuis la boucle de frames : la réponse est mémorisée
    /// [`AVAILABILITY_TTL`] (voir la note de cette constante — sans elle, cet
    /// appel lançait deux processus Wine par frame).
    pub fn available() -> bool {
        let mut slot = AVAILABILITY.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((found, when)) = *slot
            && when.elapsed() < AVAILABILITY_TTL
        {
            return found;
        }
        // La sonde garde le verrou : deux threads qui se posent la question en
        // même temps lancent alors un seul Wine, pas deux.
        let found = Self::probe();
        *slot = Some((found, Instant::now()));
        found
    }

    /// La recherche elle-même, sans mémorisation.
    fn probe() -> bool {
        crate::winerun::available()
            && Command::new("winedbg")
                .arg("--help")
                .env("DEBUGINFOD_URLS", "")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .stdin(Stdio::null())
                .status()
                .is_ok()
    }

    /// Lance `exe` sous `winedbg --gdb` et s'arrête avant sa première
    /// instruction.
    ///
    /// Bloque le thread appelant le temps que Wine démarre (jusqu'à
    /// [`STARTUP_TIMEOUT`]) : à n'appeler que depuis un thread dédié.
    ///
    /// L'application passe par [`Self::launch_interruptible`], pour garder de
    /// quoi couper la session ; cette forme courte reste la porte d'entrée
    /// naturelle du module — c'est elle que les tests ci-dessous utilisent,
    /// et elle documente le cas où l'annulation n'est pas un souci.
    #[allow(dead_code)]
    pub fn launch(exe: &Path) -> WinDbgResult<Self> {
        Self::launch_interruptible(exe, &WinDbgInterrupt::new())
    }

    /// Comme [`Self::launch`], mais en armant `interrupt` dès que `winedbg`
    /// existe : à partir de là, un autre thread peut couper la session — y
    /// compris pendant le démarrage, qui n'est pas instantané.
    pub fn launch_interruptible(exe: &Path, interrupt: &WinDbgInterrupt) -> WinDbgResult<Self> {
        if !Self::available() {
            return Err(WinDbgError::WineMissing);
        }
        // Sans rapport avec notre propre session `--gdb` (déjà attachée dès
        // le départ, l'exception nous revient à nous, pas au gestionnaire
        // automatique de Wine) : filet de sécurité pour le cas où `winedbg`
        // meurt en cours de route et laisse le débogué sans personne
        // attaché. Voir la doc de [`crate::winerun::suppress_crash_dialog`].
        crate::winerun::suppress_crash_dialog();
        // `winedbg` refuse un chemin relatif (« Couldn't start process »,
        // vérifié) : il le cherche tel quel plutôt que depuis le répertoire
        // courant du processus.
        let absolute = std::fs::canonicalize(exe)
            .map_err(|_| WinDbgError::BadPath)?;
        let path = absolute.to_str().ok_or(WinDbgError::BadPath)?;
        let entry_va = read_entry_va(&absolute)?;

        // Port éphémère : on demande à l'OS d'en choisir un libre, puis on
        // relâche l'écoute aussitôt pour que `winedbg` s'y attache à son
        // tour. Fenêtre de course minime, acceptable pour un poste de travail
        // mono-utilisateur — la même classe de compromis que les fichiers
        // temporaires du reste de l'IDE.
        let port = {
            let probe = std::net::TcpListener::bind("127.0.0.1:0")
                .map_err(|e| WinDbgError::ConnectFailed(e.to_string()))?;
            probe.local_addr().map_err(|e| WinDbgError::ConnectFailed(e.to_string()))?.port()
        };

        let child = Command::new("winedbg")
            .args(["--gdb", "--port", &port.to_string(), "--no-start", path])
            // Cache `debuginfod` local abîmé -> assertion interne de winedbg
            // (voir la doc de module) ; ce chemin n'en a de toute façon pas
            // besoin, seuls les symboles du débogué compteraient, et ce
            // module n'en lit aucun.
            .env("DEBUGINFOD_URLS", "")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| WinDbgError::NoStart)?;
        // Armée avant toute attente : ce qui suit peut durer plusieurs
        // secondes, et « Arrêter » doit déjà mordre pendant ce temps-là.
        interrupt.arm(child.id());

        let deadline = Instant::now() + STARTUP_TIMEOUT;
        let conn = match RspConn::connect(port, deadline, interrupt) {
            Ok(conn) => conn,
            Err(e) => {
                // Pas encore de `WinDebugger` pour porter le `Drop` qui
                // nettoie : sans ces deux lignes, un `winedbg` qui n'a pas
                // ouvert son port resterait en zombie derrière l'IDE.
                let mut child = child;
                let _ = child.kill();
                let _ = child.wait();
                interrupt.disarm();
                return Err(e);
            }
        };

        // Construit dès la connexion établie, avant même la poignée de main :
        // tout échec en dessous passe désormais par `Drop`, qui tue `winedbg`
        // et le moissonne, au lieu de l'abandonner en arrière-plan.
        let mut dbg = WinDebugger {
            child,
            conn,
            state: WinRunState::Stopped,
            regs: Registers::default(),
            fp: None,
            breakpoints: Vec::new(),
            interrupt: interrupt.clone(),
        };

        // Capacités : sans `xmlRegisters=i386`, qui fait planter cette
        // version de winedbg (assertion dans gdbproxy.c — vérifié).
        dbg.conn.request("qSupported:multiprocess+;swbreak+;hwbreak+")?;

        // Confirme l'arrêt initial avant de lire les registres : un « W »/« X »
        // ici voudrait dire que le programme s'est terminé avant même le
        // premier arrêt (chemin introuvable, sous-système incompatible…).
        let stop = dbg.conn.request("?")?;
        match stop.first() {
            Some(b'T') | Some(b'S') => {}
            Some(b'W') | Some(b'X') => return Err(WinDbgError::NoStart),
            _ => {
                return Err(WinDbgError::Protocol(format!(
                    "arrêt initial inattendu: {}",
                    String::from_utf8_lossy(&stop)
                )));
            }
        }
        dbg.refresh_regs()?;

        // Le premier arrêt tombe dans le chargeur de Wine (`ntdll`), avant
        // que le programme du laboratoire n'existe vraiment — vérifié :
        // c'est `process_breakpoint()` dans `ntdll.dll`, pas la première
        // instruction de `main`. On file donc jusqu'au vrai point d'entrée
        // avant de rendre la main, pour tenir la même promesse que le
        // débogueur Linux : « s'arrête juste avant sa première instruction ».
        dbg.set_breakpoint(entry_va)?;
        dbg.cont()?;
        dbg.clear_breakpoint(entry_va)?;
        if dbg.state != WinRunState::Stopped {
            return Err(WinDbgError::NoStart);
        }
        Ok(dbg)
    }

    /// Interprète un paquet d'arrêt (`T`/`S`/`W`/`X`) et met à jour l'état.
    /// Rend vrai si le processus est toujours vivant (donc s'il faut relire
    /// les registres).
    fn apply_stop(&mut self, pkt: &[u8]) -> WinDbgResult<bool> {
        match pkt.first() {
            Some(b'T') | Some(b'S') => {
                self.state = WinRunState::Stopped;
                Ok(true)
            }
            Some(b'W') => {
                let code = parse_hex_u32(&pkt[1..]).unwrap_or(0);
                self.state = WinRunState::Exited(code as i32);
                Ok(false)
            }
            Some(b'X') => {
                self.state = WinRunState::Signaled;
                Ok(false)
            }
            _ => Err(WinDbgError::Protocol(format!(
                "paquet d'arrêt inattendu: {}",
                String::from_utf8_lossy(pkt)
            ))),
        }
    }

    fn refresh_regs(&mut self) -> WinDbgResult<()> {
        let g = from_hex(&self.conn.request("g")?)?;
        let (regs, fp) = registers_from_g(&g)?;
        self.regs = regs;
        self.fp = fp;
        Ok(())
    }

    /// Une seule instruction, sans se soucier des points d'arrêt (utilisé en
    /// interne par [`Self::cont`] pour franchir celui sur lequel on est
    /// arrêté avant de le réarmer).
    fn raw_step(&mut self) -> WinDbgResult<()> {
        let reply = self.conn.request("vCont;s")?;
        if self.apply_stop(&reply)? {
            self.refresh_regs()?;
        }
        Ok(())
    }

    /// Une instruction machine. Sans effet si le programme n'est pas arrêté.
    pub fn step(&mut self) -> WinDbgResult<()> {
        if self.state != WinRunState::Stopped {
            return Ok(());
        }
        self.raw_step()
    }

    /// Reprend l'exécution jusqu'au prochain point d'arrêt posé ou la fin du
    /// programme.
    pub fn cont(&mut self) -> WinDbgResult<()> {
        if self.state != WinRunState::Stopped {
            return Ok(());
        }
        // Sur un point d'arrêt qu'on vient de franchir (désarmé) : rejouer
        // l'instruction d'origine avant de le réarmer, sinon il retriggerait
        // sans que le programme n'ait avancé.
        if let Some(pos) = self
            .breakpoints
            .iter()
            .position(|b| !b.armed && b.addr == self.regs.rip)
        {
            self.raw_step()?;
            if self.state != WinRunState::Stopped {
                return Ok(()); // terminé pendant ce pas
            }
            let addr = self.breakpoints[pos].addr;
            self.poke(addr, 0xCC)?;
            self.breakpoints[pos].armed = true;
        }

        let reply = self.conn.request_running("vCont;c")?;
        if !self.apply_stop(&reply)? {
            return Ok(()); // terminé
        }
        self.refresh_regs()?;

        // `INT3` avance RIP d'un octet après le déclenchement : la ramener à
        // l'adresse du point d'arrêt et désarmer celui-ci pour que
        // l'instruction reste lisible et réexécutable.
        let hit = self.regs.rip.wrapping_sub(1);
        if let Some(pos) = self.breakpoints.iter().position(|b| b.armed && b.addr == hit) {
            let orig = self.breakpoints[pos].orig;
            self.poke(hit, orig)?;
            self.breakpoints[pos].armed = false;
            self.set_register("RIP", hit)?;
        }
        Ok(())
    }

    /// Écrit un unique octet en mémoire, sans passer par la vérification
    /// d'état publique de [`Self::write_mem`] (déjà garantie arrêtée ici).
    fn poke(&mut self, addr: u64, byte: u8) -> WinDbgResult<()> {
        let reply = self.conn.request(&format!("M{addr:x},1:{:02x}", byte))?;
        if reply != b"OK" {
            return Err(WinDbgError::Protocol(format!(
                "écriture mémoire refusée à 0x{addr:x}"
            )));
        }
        Ok(())
    }

    /// Pose un point d'arrêt logiciel à `addr` (adresse d'une frontière
    /// d'instruction réelle — celle-ci n'est pas vérifiée ici, c'est à
    /// l'appelant de la garantir, par exemple via le désassemblage).
    pub fn set_breakpoint(&mut self, addr: u64) -> WinDbgResult<()> {
        if self.state != WinRunState::Stopped {
            return Err(WinDbgError::NotStopped);
        }
        if self.breakpoints.iter().any(|b| b.addr == addr) {
            return Ok(());
        }
        let orig = self.read_mem(addr, 1)?[0];
        self.poke(addr, 0xCC)?;
        self.breakpoints.push(Bp { addr, orig, armed: true });
        Ok(())
    }

    /// Retire le point d'arrêt à `addr`, si présent. Restaure l'octet
    /// d'origine seulement s'il était encore armé (sinon l'instruction est
    /// déjà intacte).
    pub fn clear_breakpoint(&mut self, addr: u64) -> WinDbgResult<()> {
        let Some(pos) = self.breakpoints.iter().position(|b| b.addr == addr) else {
            return Ok(());
        };
        let bp = self.breakpoints.remove(pos);
        if bp.armed {
            self.poke(addr, bp.orig)?;
        }
        Ok(())
    }

    pub fn breakpoints(&self) -> impl Iterator<Item = u64> + '_ {
        self.breakpoints.iter().map(|b| b.addr)
    }

    /// Registres généraux courants.
    pub fn regs(&self) -> &Registers {
        &self.regs
    }

    /// Drapeaux décodés depuis EFLAGS.
    pub fn flags(&self) -> Flags {
        Flags::from_eflags(self.regs.eflags)
    }

    /// Registres SSE/x87, si le serveur les a rendus (paquet `g` complet).
    ///
    /// Pas encore lu : la fenêtre de pas-à-pas Windows (`app::ui_win_debug`)
    /// n'affiche pour l'instant que les registres généraux et les drapeaux.
    #[allow(dead_code)]
    pub fn fp_regs(&self) -> Option<&FpRegisters> {
        self.fp.as_ref()
    }

    /// Modifie un registre général (processus arrêté requis).
    pub fn set_register(&mut self, name: &str, value: u64) -> WinDbgResult<()> {
        if self.state != WinRunState::Stopped {
            return Err(WinDbgError::NotStopped);
        }
        if name == "EFLAGS" {
            let reply = self
                .conn
                .request(&format!("P11={}", to_hex(&(value as u32).to_le_bytes())))?;
            if reply != b"OK" {
                return Err(WinDbgError::Protocol("écriture EFLAGS refusée".to_string()));
            }
            self.regs.eflags = value;
            return Ok(());
        }
        let idx = reg_index(name).ok_or_else(|| WinDbgError::UnknownRegister(name.to_string()))?;
        let reply = self
            .conn
            .request(&format!("P{idx:x}={}", to_hex(&value.to_le_bytes())))?;
        if reply != b"OK" {
            return Err(WinDbgError::Protocol(format!("écriture de {name} refusée")));
        }
        self.refresh_regs()
    }

    /// Lit `len` octets à `addr` dans l'espace d'adressage du débogué.
    pub fn read_mem(&mut self, addr: u64, len: usize) -> WinDbgResult<Vec<u8>> {
        let reply = self.conn.request(&format!("m{addr:x},{len:x}"))?;
        if reply.first() == Some(&b'E') {
            return Err(WinDbgError::Protocol(format!(
                "lecture refusée à 0x{addr:x}: {}",
                String::from_utf8_lossy(&reply)
            )));
        }
        from_hex(&reply)
    }

    /// Écrit `bytes` à `addr` (processus arrêté requis).
    ///
    /// Pas encore appelée : la fenêtre de pas-à-pas Windows n'a pas de
    /// panneau mémoire éditable pour l'instant, seulement des registres en
    /// lecture seule.
    #[allow(dead_code)]
    pub fn write_mem(&mut self, addr: u64, bytes: &[u8]) -> WinDbgResult<()> {
        if self.state != WinRunState::Stopped {
            return Err(WinDbgError::NotStopped);
        }
        let reply = self
            .conn
            .request(&format!("M{addr:x},{:x}:{}", bytes.len(), to_hex(bytes)))?;
        if reply != b"OK" {
            return Err(WinDbgError::Protocol(format!("écriture refusée à 0x{addr:x}")));
        }
        Ok(())
    }

    /// Le programme est encore arrêté, prêt pour un nouveau pas.
    pub fn is_alive(&self) -> bool {
        self.state == WinRunState::Stopped
    }
}

impl Drop for WinDebugger {
    fn drop(&mut self) {
        // `k` (kill) est best-effort : si la connexion est déjà partie, il
        // n'y a de toute façon plus rien à prévenir côté serveur. Tuer notre
        // propre `winedbg` termine ensuite le débogué par les mêmes règles
        // Windows qu'un détachement brutal (`DEBUG_PROCESS` sans
        // `DebugSetProcessKillOnExit(FALSE)`).
        let _ = self.conn.send("k");
        // Désarmée AVANT le `wait()` : une fois le processus moissonné, son
        // PID redevient attribuable, et un `interrupt()` retardataire tuerait
        // un innocent. Le mutex de la poignée rend l'ordre observable.
        self.interrupt.disarm();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Décode un entier hexadécimal ASCII (le code de sortie d'un paquet `W`).
fn parse_hex_u32(digits: &[u8]) -> Option<u32> {
    let text = std::str::from_utf8(digits).ok()?;
    u32::from_str_radix(text.trim(), 16).ok()
}

/// Adresse virtuelle du point d'entrée déclaré dans l'en-tête PE de `exe`.
///
/// Relu depuis le fichier plutôt que transmis par l'appelant : n'importe quel
/// `.exe` PE64 porte cette information dans son en-tête, et la relire
/// affranchit ce module de connaître [`crate::pe_link`] ou la manière dont le
/// binaire a été produit.
fn read_entry_va(exe: &Path) -> WinDbgResult<u64> {
    let data = std::fs::read(exe).map_err(|_| WinDbgError::BadPath)?;
    let file = object::File::parse(&*data).map_err(|_| WinDbgError::BadPath)?;
    Ok(file.entry())
}

/// Verrou partagé par les tests qui font tourner une vraie session Wine.
///
/// `wineserver` et le serveur X sont communs à tout le processus de test.
/// Plusieurs sessions de pas-à-pas en parallèle se tolèrent (verrou en
/// lecture) ; celle qui pilote une vraie fenêtre à l'écran, elle, ne supporte
/// aucun voisin — vérifié : la boîte de dialogue qu'elle attend se referme
/// toute seule dès qu'une autre session Wine s'agite à côté, et le test perd
/// alors ce qu'il prétendait vérifier. Elle prend donc le verrou en écriture
/// et tourne seule.
#[cfg(test)]
static WINE_TESTS: std::sync::RwLock<()> = std::sync::RwLock::new(());

/// À prendre par tout test qui lance une vraie session Wine sans avoir besoin
/// de l'écran pour lui seul (voir [`WINE_TESTS`]).
#[cfg(test)]
pub(crate) fn shared_wine_test() -> std::sync::RwLockReadGuard<'static, ()> {
    WINE_TESTS.read().unwrap_or_else(|e| e.into_inner())
}

/// À prendre par le seul test qui pilote une vraie fenêtre à l'écran.
#[cfg(test)]
pub(crate) fn exclusive_wine_test() -> std::sync::RwLockWriteGuard<'static, ()> {
    WINE_TESTS.write().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble::{self, Target};
    use std::path::PathBuf;

    /// La barre d'outils demande « Wine est-il là ? » à chaque frame, et le
    /// pas-à-pas Windows redemande une frame toutes les 30 ms tant qu'une
    /// commande tourne. Chaque réponse non mémorisée coûtait deux lancements
    /// de Wine (~140 ms) : l'IDE y passait tout son temps, et `ps` se
    /// remplissait de `winedbg` éphémères — le « winedbg en boucle » signalé.
    ///
    /// Deux cents appels d'affilée doivent donc coûter à peu près rien. Sans
    /// mémorisation, ce même compte prendrait une demi-minute.
    #[test]
    fn asking_whether_wine_is_available_is_free_after_the_first_time() {
        // Le premier appel paie la sonde, où qu'on en soit dans le TTL.
        let first = WinDebugger::available();
        let t = Instant::now();
        for _ in 0..200 {
            assert_eq!(WinDebugger::available(), first, "réponse instable");
        }
        let elapsed = t.elapsed();
        assert!(
            elapsed < Duration::from_millis(500),
            "200 appels ont pris {elapsed:?} : la réponse n'est pas mémorisée, \
             et l'interface repaie un lancement de Wine à chaque frame"
        );
    }

    fn build_exe(name: &str, source: &str) -> PathBuf {
        let dir = PathBuf::from("build").join(name);
        std::fs::create_dir_all(&dir).expect("dossier");
        let asm = dir.join(format!("{name}.asm"));
        std::fs::write(&asm, source).expect("source");
        assemble::assemble_for(&asm, &dir, &[], Target::Windows, crate::i18n::Lang::Fr)
            .expect("assemblage PE")
            .binary
    }

    const HELLO: &str = r#"
        bits 64
        default rel
        section .data
            msg    db "Bonjour", 13, 10
            msglen equ $ - msg
        section .bss
            ecrits resq 1
        section .text
            global main
            extern GetStdHandle
            extern WriteFile
            extern ExitProcess
        main:
            sub     rsp, 40
            mov     ecx, -11
            call    GetStdHandle
            mov     rcx, rax
            lea     rdx, [msg]
            mov     r8d, msglen
            lea     r9, [ecrits]
            mov     qword [rsp + 32], 0
            call    WriteFile
            mov     ecx, 7
            call    ExitProcess
        "#;

    /// Vérification ponctuelle demandée par l'utilisateur après une boîte
    /// « WineDbg n'a pas pu s'y attacher » vue pendant une session de test
    /// manuel : l'exemple livré `win_hello_world.asm`, pas-à-pas jusqu'au
    /// bout, ne doit ni planter ni laisser winedbg dans un état étrange.
    #[test]
    fn shipped_win_hello_world_steps_to_a_clean_exit() {
        if !WinDebugger::available() {
            eprintln!("wine absent : non vérifié");
            return;
        }
        let _wine = shared_wine_test();
        let source = std::fs::read_to_string("examples_seed/win_hello_world.asm")
            .expect("l'exemple est livré avec l'IDE");
        let exe = build_exe("windbg-shipped-hello", &source);
        let mut dbg = WinDebugger::launch(&exe).expect("lancement");
        // Un « Suivant » entre dans kernel32/ntdll (pas de step-over côté
        // Windows) : quelques pas dans le programme lui-même, comme un élève
        // le ferait, puis Continuer jusqu'à la sortie plutôt que de
        // dénombrer des instructions internes à Wine.
        for _ in 0..3 {
            if !dbg.is_alive() {
                break;
            }
            dbg.step().expect("pas");
        }
        dbg.cont().expect("continuer jusqu'à la fin");
        assert!(!dbg.is_alive());
        assert_eq!(dbg.state, WinRunState::Exited(0), "code de sortie inattendu");
    }

    /// Le lancement s'arrête avant la première instruction, et les registres
    /// lus décrivent un thread réel (RSP non nul).
    #[test]
    fn launch_stops_before_the_first_instruction() {
        if !WinDebugger::available() {
            eprintln!("wine absent : lancement non vérifié");
            return;
        }
        let _wine = shared_wine_test();
        let exe = build_exe("windbg-launch", HELLO);
        let dbg = WinDebugger::launch(&exe).expect("lancement");
        assert!(dbg.is_alive());
        assert_ne!(dbg.regs().rsp, 0, "une pile doit déjà exister");
    }

    /// Chaque pas avance RIP d'au moins une instruction, jamais en arrière.
    #[test]
    fn stepping_moves_rip_forward() {
        if !WinDebugger::available() {
            eprintln!("wine absent : pas-à-pas non vérifié");
            return;
        }
        let _wine = shared_wine_test();
        let exe = build_exe("windbg-step", HELLO);
        let mut dbg = WinDebugger::launch(&exe).expect("lancement");
        let mut last = dbg.regs().rip;
        for _ in 0..5 {
            dbg.step().expect("pas");
            if !dbg.is_alive() {
                break;
            }
            assert_ne!(dbg.regs().rip, last, "RIP doit bouger à chaque pas");
            last = dbg.regs().rip;
        }
    }

    /// Un point d'arrêt posé sur l'entrée arrête `cont()` exactement là, avec
    /// des registres cohérents (la pile pointe toujours quelque part de
    /// plausible), et le laisse réarmable pour un futur passage.
    #[test]
    fn a_breakpoint_stops_execution_at_the_right_address() {
        if !WinDebugger::available() {
            eprintln!("wine absent : point d'arrêt non vérifié");
            return;
        }
        let _wine = shared_wine_test();
        let exe = build_exe("windbg-bp", HELLO);
        let mut dbg = WinDebugger::launch(&exe).expect("lancement");
        let entry = dbg.regs().rip;

        // Un point d'arrêt un peu plus loin, sur une frontière d'instruction
        // réelle : après `sub rsp,40` (4 octets, confirmé par lecture directe
        // — 48 83 EC 28) vient `mov ecx,-11` (5 octets).
        let target = entry + 4;
        dbg.set_breakpoint(target).expect("pose du point d'arrêt");
        dbg.cont().expect("continuer jusqu'au point d'arrêt");
        assert!(dbg.is_alive(), "le programme doit être arrêté, pas terminé");
        assert_eq!(dbg.regs().rip, target, "arrêt exactement sur le point d'arrêt");

        // Un pas franchit l'instruction d'origine (restaurée), sans retomber
        // aussitôt sur le point d'arrêt qu'on vient de quitter.
        dbg.step().expect("pas après le point d'arrêt");
        assert!(dbg.is_alive());
        assert_ne!(dbg.regs().rip, target);
    }

    /// Le programme va jusqu'au bout et rend son code de sortie ; le
    /// débogueur redevient inutilisable pour un pas ultérieur (silencieux,
    /// pas une erreur).
    #[test]
    fn the_program_runs_to_completion_and_reports_its_exit_code() {
        if !WinDebugger::available() {
            eprintln!("wine absent : fin de programme non vérifiée");
            return;
        }
        let _wine = shared_wine_test();
        let exe = build_exe("windbg-exit", HELLO);
        let mut dbg = WinDebugger::launch(&exe).expect("lancement");
        dbg.cont().expect("continuer jusqu'à la fin");
        assert!(!dbg.is_alive());
        assert_eq!(dbg.state, WinRunState::Exited(7));
    }

    /// Lire puis réécrire la mémoire à l'entrée retrouve les mêmes octets
    /// qu'assemblés — la fenêtre de plausibilité la plus directe qui soit.
    #[test]
    fn memory_written_is_memory_read_back() {
        if !WinDebugger::available() {
            eprintln!("wine absent : mémoire non vérifiée");
            return;
        }
        let _wine = shared_wine_test();
        let exe = build_exe("windbg-mem", HELLO);
        let mut dbg = WinDebugger::launch(&exe).expect("lancement");
        let entry = dbg.regs().rip;
        let original = dbg.read_mem(entry, 4).expect("lecture");
        assert_eq!(original, vec![0x48, 0x83, 0xEC, 0x28], "sub rsp,40");

        dbg.write_mem(entry, &[0x90, 0x90, 0x90, 0x90]).expect("écriture");
        let patched = dbg.read_mem(entry, 4).expect("relecture");
        assert_eq!(patched, vec![0x90, 0x90, 0x90, 0x90]);
    }

    /// Un programme qui ne rendra jamais la main tout seul — la version
    /// automatisable du `MessageBoxA` qui attend un clic.
    const BOUCLE_SANS_FIN: &str = r#"
        bits 64
        default rel
        section .text
            global main
            extern ExitProcess
        main:
            xor     ecx, ecx
        boucle:
            inc     ecx
            jmp     boucle
            call    ExitProcess
        "#;

    /// La promesse de [`WinDbgInterrupt`] : un `cont()` parti dans quelque
    /// chose qui ne s'arrête jamais est bel et bien débloqué depuis un autre
    /// thread, en une poignée de secondes plutôt qu'au bout des vingt du
    /// délai RSP — et il échoue, plutôt que de rendre un état inventé.
    #[test]
    fn interrupting_unblocks_a_command_that_never_returns() {
        if !WinDebugger::available() {
            eprintln!("wine absent : interruption non vérifiée");
            return;
        }
        let _wine = shared_wine_test();
        let exe = build_exe("windbg-interrupt", BOUCLE_SANS_FIN);
        let interrupt = WinDbgInterrupt::new();
        let mut dbg = WinDebugger::launch_interruptible(&exe, &interrupt).expect("lancement");

        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let outcome = dbg.cont();
            let _ = tx.send(outcome.is_err());
        });

        // D'abord la preuve que la commande est bien coincée : sans quoi la
        // suite passerait pour une bonne raison sans rien démontrer.
        assert!(
            rx.recv_timeout(Duration::from_millis(400)).is_err(),
            "cont() ne peut pas revenir seul d'une boucle sans fin"
        );

        let t = Instant::now();
        assert!(interrupt.interrupt(), "winedbg devait être visé");
        let failed = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("l'interruption doit débloquer cont()");
        assert!(failed, "la lecture coupée doit échouer, pas rendre un faux arrêt");
        assert!(
            t.elapsed() < Duration::from_secs(5),
            "déblocage en {:?}, bien avant les 20 s du délai RSP",
            t.elapsed()
        );
        worker.join().expect("le thread de commande doit se terminer");
    }

    /// Modifier un registre général se voit à la prochaine lecture.
    #[test]
    fn setting_a_register_is_visible_afterwards() {
        if !WinDebugger::available() {
            eprintln!("wine absent : écriture de registre non vérifiée");
            return;
        }
        let _wine = shared_wine_test();
        let exe = build_exe("windbg-setreg", HELLO);
        let mut dbg = WinDebugger::launch(&exe).expect("lancement");
        dbg.set_register("RAX", 0x1234_5678_9abc_def0).expect("écriture");
        assert_eq!(dbg.regs().rax, 0x1234_5678_9abc_def0);
    }
}
