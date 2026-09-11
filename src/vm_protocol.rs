//! Format d'échange entre l'hôte macOS ([`crate::vm_debugger`]) et l'agent qui
//! tourne dans la VM Linux (`bin/asmstudio_agent`). Compilé des deux côtés :
//! ni l'un ni l'autre ne réinterprète les octets de l'autre à la main.
//!
//! Chaque variante de [`Request`] correspond à *une action utilisateur*, pas
//! à une instruction machine : [`Request::RunUntil`] fait tourner toute la
//! boucle de pas côté agent (ptrace local à la VM, donc rapide) et ne renvoie
//! qu'une seule réponse — sans quoi « Continuer » ferait des dizaines de
//! milliers d'aller-retours réseau pour un seul clic.
//!
//! Trame : un message JSON par ligne (NDJSON) sur le flux TCP. Le choix n'est
//! pas la vitesse — un format binaire irait plus vite — mais la facilité de
//! déboguer une connexion qui, ici, ne voit jamais plus de quelques appels par
//! seconde : on peut lire une session capturée avec `nc` sans outil dédié.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------
// Types de données, miroir de ceux de `debugger` (voir sa justification
// dans ce module et dans src/vm_debugger.rs : dupliquer la *forme* plutôt
// que de modifier `debugger.rs`, qui doit rester intact).
// ---------------------------------------------------------------------

/// Nombre de mots de pile capturés dans chaque snapshot — même valeur que
/// [`crate::debugger::STACK_WINDOW`], avec laquelle elle doit rester en phase.
pub const STACK_WINDOW: usize = 16;

/// Port de l'agent, redirigé tel quel par QEMU (`hostfwd`) — une seule VM à
/// la fois, pas besoin de négocier. Connu de `vm_session` (qui démarre QEMU
/// avec ce port), `vm_debugger` (qui s'y connecte pour chaque appel, sans
/// passer par `VmSession` — une connexion TCP ratée dit déjà tout ce qu'il y
/// a à savoir sur une VM pas prête) et de l'agent lui-même.
pub const AGENT_PORT: u16 = 7878;

#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Registers {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub eflags: u64,
}

impl Registers {
    /// Liste ordonnée (nom, valeur) pour l'affichage — même ordre que
    /// `debugger::Registers::named`, dont dépendent `breakpoint::Condition`
    /// et l'UI des registres.
    pub fn named(&self) -> [(&'static str, u64); 18] {
        [
            ("RAX", self.rax),
            ("RBX", self.rbx),
            ("RCX", self.rcx),
            ("RDX", self.rdx),
            ("RSI", self.rsi),
            ("RDI", self.rdi),
            ("RBP", self.rbp),
            ("RSP", self.rsp),
            ("R8", self.r8),
            ("R9", self.r9),
            ("R10", self.r10),
            ("R11", self.r11),
            ("R12", self.r12),
            ("R13", self.r13),
            ("R14", self.r14),
            ("R15", self.r15),
            ("RIP", self.rip),
            ("EFLAGS", self.eflags),
        ]
    }
}

#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Flags {
    pub cf: bool,
    pub pf: bool,
    pub af: bool,
    pub zf: bool,
    pub sf: bool,
    pub of: bool,
}

impl Flags {
    pub fn from_eflags(e: u64) -> Self {
        Flags {
            cf: e & (1 << 0) != 0,
            pf: e & (1 << 2) != 0,
            af: e & (1 << 4) != 0,
            zf: e & (1 << 6) != 0,
            sf: e & (1 << 7) != 0,
            of: e & (1 << 11) != 0,
        }
    }

    pub fn named(&self) -> [(&'static str, bool); 6] {
        [
            ("ZF", self.zf),
            ("CF", self.cf),
            ("OF", self.of),
            ("SF", self.sf),
            ("PF", self.pf),
            ("AF", self.af),
        ]
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RunState {
    Stopped,
    Running,
    Exited(i32),
    Signaled,
    Faulted(Fault),
}

/// Signal représenté par son nom POSIX plutôt que par `nix::sys::signal::Signal` :
/// évite de faire dépendre le format d'échange d'un type qui ne vise pas
/// spécialement la sérialisation, pour une poignée de valeurs connues.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Fault {
    pub signal: SignalName,
    pub addr: Option<u64>,
    pub rip: u64,
}

impl Fault {
    /// Même nom, même méthode que `debugger::Fault::signal_name` : le code
    /// qui affiche ou classe une faute (voir `diagnostic.rs`) s'écrit une
    /// seule fois pour les deux représentations.
    pub fn signal_name(&self) -> &'static str {
        self.signal.label()
    }
}

/// Noms plutôt que valeurs numériques du signal — voir le commentaire sur
/// [`Fault`]. Casse POSIX délibérément conservée (`SIGSEGV`, pas `Segv`) :
/// c'est ce qui permet à `Signal::SIGSEGV` de continuer à s'écrire pareil
/// dans le code et les tests partagés avec le chemin natif (`nix`), une fois
/// ce type aliasé à la place de `nix::sys::signal::Signal` sur macOS.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalName {
    SIGSEGV,
    SIGFPE,
    SIGILL,
    SIGBUS,
    Other,
}

impl SignalName {
    pub fn label(self) -> &'static str {
        match self {
            SignalName::SIGSEGV => "SIGSEGV",
            SignalName::SIGFPE => "SIGFPE",
            SignalName::SIGILL => "SIGILL",
            SignalName::SIGBUS => "SIGBUS",
            SignalName::Other => "signal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegionKind {
    Code,
    Data,
    Rodata,
    Heap,
    Stack,
}

impl RegionKind {
    pub fn label(self) -> &'static str {
        match self {
            RegionKind::Code => ".text",
            RegionKind::Data => ".data/.bss",
            RegionKind::Rodata => ".rodata",
            RegionKind::Heap => "[heap]",
            RegionKind::Stack => "[stack]",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemRegion {
    pub start: u64,
    pub end: u64,
    pub kind: RegionKind,
    pub perms: String,
}

impl MemRegion {
    pub fn contains(&self, addr: u64) -> bool {
        (self.start..self.end).contains(&addr)
    }
    pub fn size(&self) -> u64 {
        self.end - self.start
    }
}

/// Registres SSE/x87 — voir `debugger::FpRegisters` pour la justification du
/// format brut. Toujours envoyé en entier (pas de partage `Arc` possible à
/// travers le fil) : ce n'est pertinent que pour un programme qui utilise
/// vraiment les XMM, rare parmi les exercices.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct FpRegisters {
    pub xmm: [u128; 16],
    pub st: [[u8; 10]; 8],
    pub fcw: u16,
    pub fsw: u16,
    pub ftw: u8,
    pub mxcsr: u32,
}

impl FpRegisters {
    pub fn top(&self) -> usize {
        ((self.fsw >> 11) & 0b111) as usize
    }
    pub fn st_reg(&self, i: usize) -> (usize, [u8; 10], bool) {
        let phys = (self.top() + i) % 8;
        (phys, self.st[phys], self.ftw & (1 << phys) != 0)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub regs: Registers,
    pub stack: [u64; STACK_WINDOW],
    /// Contrairement à `debugger::Snapshot::fp`, pas de partage `Arc` entre
    /// snapshots voisins : chaque message porte sa propre valeur, l'aliasing
    /// mémoire d'origine ne traverse pas le fil de toute façon.
    pub fp: Option<FpRegisters>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Watchpoint {
    pub addr: u64,
    pub len: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WatchHit {
    pub addr: u64,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
    pub step: usize,
}

impl WatchHit {
    pub fn values(&self) -> (u64, u64) {
        let lire = |v: &[u8]| {
            let mut buf = [0u8; 8];
            let n = v.len().min(8);
            buf[..n].copy_from_slice(&v[..n]);
            u64::from_le_bytes(buf)
        };
        (lire(&self.before), lire(&self.after))
    }
}

/// Miroir de `debugger::DbgError`, plus les pannes propres au transport
/// (perdu/délai dépassé) qui n'ont pas d'équivalent local.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DbgError {
    BadPath,
    PtraceDenied,
    NoStart,
    ExitedBeforeStart(i32),
    UnexpectedInitialState(String),
    HistoryFull(usize),
    NoStdin,
    NotStopped,
    UnknownRegister(String),
    System { what: String, err: String },
    /// La VM n'a pas répondu dans le délai imparti — plantée, ou l'agent est
    /// occupé sur une boucle trop longue.
    Timeout,
    /// La connexion à l'agent a été perdue (VM tuée, redémarrée, réseau coupé).
    ConnectionLost(String),
}

impl DbgError {
    /// Miroir de `debugger::DbgError::message` — mêmes messages pour les
    /// variantes communes, plus celles propres au transport.
    pub fn message(&self, lang: crate::i18n::Lang) -> String {
        use crate::i18n;
        let tr = |fr, en, es| i18n::tr3(lang, fr, en, es);
        match self {
            DbgError::BadPath => tr(
                "chemin du binaire illisible (non-UTF8)",
                "unreadable binary path (not UTF-8)",
                "ruta del binario ilegible (no UTF-8)",
            )
            .to_string(),
            DbgError::PtraceDenied => tr(
                "le débogage est interdit par le système (ptrace refusé)",
                "debugging is denied by the system (ptrace refused)",
                "el sistema prohíbe la depuración (ptrace denegado)",
            )
            .to_string(),
            DbgError::NoStart => tr(
                "le programme n'a pas démarré (execve a échoué)",
                "the program did not start (execve failed)",
                "el programa no arrancó (execve falló)",
            )
            .to_string(),
            DbgError::ExitedBeforeStart(code) => format!(
                "{} ({} {code})",
                tr(
                    "le programme s'est terminé avant le débogage",
                    "the program exited before debugging began",
                    "el programa terminó antes de la depuración",
                ),
                tr("code", "code", "código"),
            ),
            DbgError::UnexpectedInitialState(s) => {
                format!("{} : {s}", tr("état initial inattendu", "unexpected initial state", "estado inicial inesperado"))
            }
            DbgError::HistoryFull(max) => format!(
                "{} ({max} {}) : {}",
                tr("historique plein", "history full", "historial lleno"),
                tr("étapes", "steps", "pasos"),
                tr(
                    "le programme boucle-t-il ? Relancez-le.",
                    "is the program looping? Restart it.",
                    "¿el programa hace un bucle? Reinícielo.",
                ),
            ),
            DbgError::NoStdin => tr(
                "entrée standard non redirigée",
                "standard input not redirected",
                "entrada estándar no redirigida",
            )
            .to_string(),
            DbgError::NotStopped => tr(
                "le processus n'est pas arrêté",
                "the process is not stopped",
                "el proceso no está detenido",
            )
            .to_string(),
            DbgError::UnknownRegister(name) => {
                format!("{} : {name}", tr("registre inconnu", "unknown register", "registro desconocido"))
            }
            DbgError::System { what, err } => format!("{what}: {err}"),
            DbgError::Timeout => tr(
                "la VM n'a pas répondu à temps",
                "the VM did not respond in time",
                "la VM no respondió a tiempo",
            )
            .to_string(),
            DbgError::ConnectionLost(detail) => format!(
                "{} : {detail}",
                tr("connexion à la VM perdue", "connection to the VM lost", "conexión con la VM perdida")
            ),
        }
    }
}

pub type DbgResult<T> = Result<T, DbgError>;

// ---------------------------------------------------------------------
// Condition de point d'arrêt sérialisable : forme la plus simple qui
// permette à l'agent d'évaluer `stop(regs)` sans reconstituer un
// `breakpoint::Condition` complet (l'agent réutilise déjà `breakpoint`
// directement — voir bin/asmstudio_agent.rs — cette variante n'est que le
// texte brut à parser côté agent, pour ne pas exiger que
// `breakpoint::Condition` soit sérialisable en plus d'être réutilisé tel quel).
// ---------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct StopSpec {
    pub addr: u64,
    /// Texte de la condition tel que tapé par l'élève (`breakpoint::parse`
    /// l'interprète côté agent) ; `None` = point d'arrêt nu.
    pub condition_text: Option<String>,
}

// ---------------------------------------------------------------------
// Requêtes / réponses RPC.
// ---------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub enum Request {
    /// Sonde de vie : l'agent répond aussitôt, sans toucher à une session.
    Ping,
    /// Lie les objets fournis (`ld`, exécuté dans l'invité) et prépare le
    /// binaire résultant pour un futur `Launch`. `ld_args` reprend
    /// `assemble::LinkOptions::ld_args()` tel quel.
    Link { objects: Vec<LinkObject>, ld_args: Vec<String> },
    /// Démarre (ou redémarre) le binaire lié par le dernier `Link` réussi.
    Launch,
    Step,
    Poll,
    RunUntil { budget: usize, stops: Vec<StopSpec> },
    ReadMem { addr: u64, len: usize },
    WriteMem { addr: u64, bytes: Vec<u8> },
    SetRegister { name: String, value: u64 },
    TakeOutput,
    WriteStdin { text: String },
    Watch { addr: u64, len: usize },
    Unwatch { addr: u64 },
    Watchpoints,
    TakeWatchHit,
    MemRegions,
    HeapRange,
    Fault,
    LoadBias,
    /// Termine le processus tracé (le lien reste valable pour un futur `Launch`).
    Stop,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct LinkObject {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Response {
    Pong,
    Link(DbgResult<LinkResult>),
    Launch(DbgResult<LaunchResult>),
    /// `Step`/`Poll` : l'état atteint, et le nouveau snapshot s'il y en a un
    /// (aucun tant que `state == Running`, un appel bloquant est en cours).
    Stepped(DbgResult<StepResult>),
    RunUntil(DbgResult<RunUntilResult>),
    Mem(DbgResult<Vec<u8>>),
    Unit(DbgResult<()>),
    Output(String),
    Watchpoints(Vec<Watchpoint>),
    WatchHit(Option<WatchHit>),
    MemRegions(Vec<MemRegion>),
    HeapRange(Option<(u64, u64)>),
    Fault(Option<Fault>),
    LoadBias(u64),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct LinkResult {
    pub log: String,
    /// Octets du binaire lié : l'hôte les écrit dans son propre `out_dir`
    /// pour que désassemblage/`src_map`/`binfmt::inspect` continuent de
    /// fonctionner sans changement — l'agent, lui, garde sa propre copie
    /// pour l'exécuter.
    pub binary: Vec<u8>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct LaunchResult {
    pub load_bias: u64,
    pub initial: Snapshot,
    /// PID du tracé *dans l'invité* — jamais directement actionnable depuis
    /// l'hôte (aucun processus local ne porte ce numéro), gardé uniquement
    /// pour l'affichage informatif que fait déjà l'UI.
    pub pid: i32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub state: RunState,
    /// Nouveau snapshot le cas échéant (absent si l'état reste `Running`).
    pub snapshot: Option<Snapshot>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RunUntilResult {
    pub steps: usize,
    pub state: RunState,
    /// Un snapshot par instruction exécutée pendant cet appel — nécessaire
    /// pour reconstituer `history` côté hôte à l'identique de ce que ferait
    /// `Debugger::run_until` en local (la pile d'appels/syscalls affichée par
    /// l'UI se lit en comparant deux snapshots consécutifs).
    pub snapshots: Vec<Snapshot>,
}

// ---------------------------------------------------------------------
// Trame : une ligne JSON par message.
// ---------------------------------------------------------------------

use std::io::{self, BufRead, Write};

pub fn write_message<T: Serialize>(w: &mut impl Write, msg: &T) -> io::Result<()> {
    let line = serde_json::to_string(msg).map_err(io::Error::other)?;
    w.write_all(line.as_bytes())?;
    w.write_all(b"\n")?;
    w.flush()
}

pub fn read_message<T: for<'de> Deserialize<'de>>(r: &mut impl BufRead) -> io::Result<T> {
    let mut line = String::new();
    let n = r.read_line(&mut line)?;
    if n == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "connexion fermée par l'agent"));
    }
    serde_json::from_str(line.trim_end()).map_err(io::Error::other)
}
