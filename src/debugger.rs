//! Couche d'exécution pas-à-pas via `ptrace` (Linux/x86-64).
//!
//! On `fork()` un enfant qui fait `PTRACE_TRACEME` puis `execve` le binaire.
//! Le parent (le thread qui a forké — ici le thread principal de l'UI) pilote
//! l'enfant avec `PTRACE_SINGLESTEP` et lit les registres via `PTRACE_GETREGS`.
//!
//! Les trois descripteurs standard de l'enfant sont redirigés vers des tuyaux
//! avant l'`execve` : ce que le programme écrit arrive dans la console de
//! l'IDE au lieu du terminal parent (invisible depuis un lanceur de bureau),
//! et l'élève peut lui envoyer de l'entrée. Corollaire indispensable : un
//! `read` sans donnée disponible bloquerait le `waitpid` — donc l'interface
//! entière, mono-thread — le temps du syscall. C'est pourquoi l'attente se
//! fait en `WNOHANG` avec un budget de temps court ([`RunState::Running`]).

use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nix::sys::ptrace;
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::{ForkResult, Pid, execv, fork};

use crate::i18n::{self, Lang};

/// Ce qui peut empêcher le débogueur de faire ce qu'on lui demande.
///
/// Une erreur décrite plutôt qu'une phrase déjà rédigée : ces messages
/// finissent dans la console de l'élève, qui a pu mettre l'interface en
/// anglais ou en espagnol. Ce module n'a pas à connaître cette langue — elle
/// peut même changer entre l'instant de la faute et celui où elle s'affiche.
/// Il dit ce qui s'est passé ; [`Self::message`] le met en mots, au moment de
/// l'écrire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbgError {
    /// Chemin du binaire non représentable en UTF-8 (ou contenant un zéro).
    BadPath,
    /// `ptrace` refusé par le système : durcissement Yama, conteneur sans
    /// `CAP_SYS_PTRACE`, politique de sécurité.
    PtraceDenied,
    /// `execve` a échoué : rien n'a démarré.
    NoStart,
    /// Le programme s'est terminé de lui-même avant le premier arrêt.
    ExitedBeforeStart(i32),
    /// Premier arrêt dans un état qu'on ne sait pas interpréter.
    UnexpectedInitialState(String),
    /// Plafond de l'historique atteint (voir [`MAX_HISTORY`]).
    HistoryFull(usize),
    /// L'entrée standard du programme n'a pas pu être redirigée : il n'y a
    /// nulle part où écrire ce que l'élève tape.
    NoStdin,
    /// Le processus n'est pas arrêté : ni lecture ni écriture possible.
    NotStopped,
    /// Nom de registre inconnu (saisie du laboratoire mémoire).
    UnknownRegister(String),
    /// Un appel système a échoué. `what` dit ce qu'on tentait — `fork`,
    /// `getregs`, `write @0x...` — et `err` porte le texte de l'OS, qu'on rend
    /// tel quel : il n'a pas de traduction à offrir, et c'est lui qui permet de
    /// comprendre ce qui s'est passé.
    System { what: String, err: String },
}

impl DbgError {
    /// Une erreur système, avec ce qu'on tentait de faire.
    fn sys(what: impl Into<String>, err: impl std::fmt::Display) -> DbgError {
        DbgError::System { what: what.into(), err: err.to_string() }
    }

    /// Le message tel qu'il sera montré, dans la langue de l'interface.
    pub fn message(&self, lang: Lang) -> String {
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
        }
    }
}

/// Le `Result` de ce module. Alias plutôt que `Result<T, DbgError>` répété
/// trente-six fois — c'est le nombre exact de signatures concernées.
pub type DbgResult<T> = std::result::Result<T, DbgError>;

/// Sous-ensemble des registres généraux + RIP + EFLAGS que l'on suit.
#[derive(Clone, Default, PartialEq)]
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
    fn from_raw(r: &libc::user_regs_struct) -> Self {
        Registers {
            rax: r.rax,
            rbx: r.rbx,
            rcx: r.rcx,
            rdx: r.rdx,
            rsi: r.rsi,
            rdi: r.rdi,
            rbp: r.rbp,
            rsp: r.rsp,
            r8: r.r8,
            r9: r.r9,
            r10: r.r10,
            r11: r.r11,
            r12: r.r12,
            r13: r.r13,
            r14: r.r14,
            r15: r.r15,
            rip: r.rip,
            eflags: r.eflags,
        }
    }

    /// Liste ordonnée (nom, valeur) pour l'affichage.
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

/// Flags décodés depuis EFLAGS (les 6 essentiels pour l'apprentissage).
#[derive(Clone, Copy, Default, PartialEq)]
pub struct Flags {
    pub cf: bool, // Carry
    pub pf: bool, // Parity
    pub af: bool, // Auxiliary carry
    pub zf: bool, // Zero
    pub sf: bool, // Sign
    pub of: bool, // Overflow
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

    /// (nom, valeur) dans l'ordre d'affichage de la maquette.
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

/// État de vie du processus tracé.
#[derive(Clone, Copy, PartialEq)]
pub enum RunState {
    /// Arrêté sur une instruction, prêt à stepper.
    Stopped,
    /// Le pas est lancé mais l'arrêt n'est pas encore survenu : l'instruction
    /// en cours est un appel système qui prend son temps (typiquement un
    /// `read` qui attend une saisie). L'UI reste vivante et rappelle
    /// [`Debugger::poll`] à chaque frame jusqu'au retour à `Stopped`.
    Running,
    /// Terminé (code de sortie).
    Exited(i32),
    /// Tué par un signal.
    Signaled,
    /// Arrêté par une faute matérielle (SIGSEGV, SIGFPE, SIGILL, SIGBUS).
    ///
    /// L'exécution ne peut pas reprendre : réinjecter le signal tuerait le
    /// processus, le supprimer ferait refauter la même instruction en boucle
    /// (c'est exactement ce que faisait l'ancien code — RIP restait figé et
    /// l'élève ne voyait rien du tout).
    Faulted(Fault),
}

/// Une faute matérielle capturée au moment où elle survient, avec le contexte
/// nécessaire pour l'expliquer à l'élève.
#[derive(Clone, Copy, PartialEq)]
pub struct Fault {
    /// Signal reçu (`SIGSEGV`, `SIGFPE`, `SIGILL`, `SIGBUS`).
    pub signal: nix::sys::signal::Signal,
    /// Adresse qui a provoqué la faute (`siginfo_t::si_addr`).
    /// `None` si le noyau ne l'a pas renseignée.
    pub addr: Option<u64>,
    /// RIP de l'instruction fautive.
    pub rip: u64,
}

impl Fault {
    /// Nom court du signal, tel qu'affiché à l'élève.
    pub fn signal_name(&self) -> &'static str {
        use nix::sys::signal::Signal::*;
        match self.signal {
            SIGSEGV => "SIGSEGV",
            SIGFPE => "SIGFPE",
            SIGILL => "SIGILL",
            SIGBUS => "SIGBUS",
            _ => "signal",
        }
    }
}

/// Nombre de mots de pile capturés dans chaque snapshot (à partir de RSP).
pub const STACK_WINDOW: usize = 16;

/// Nature d'une région mémoire, pour le code couleur de la vue mémoire unifiée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    /// Segment exécutable (`.text`).
    Code,
    /// Segment inscriptible (`.data` / `.bss`).
    Data,
    /// Segment en lecture seule (`.rodata`).
    Rodata,
    /// Tas (`[heap]`), croît vers les adresses hautes.
    Heap,
    /// Pile (`[stack]`), croît vers les adresses basses.
    Stack,
}

impl RegionKind {
    /// Libellé court affiché sur le schéma.
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

/// Une région mappée dans l'espace d'adressage du processus.
#[derive(Debug, Clone)]
pub struct MemRegion {
    pub start: u64,
    pub end: u64,
    pub kind: RegionKind,
    /// Permissions brutes de `/proc/<pid>/maps`, ex. « rw-p ».
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

/// Unité de calcul flottant et vectorielle : les seize registres XMM (SSE) et
/// la pile x87, telles que `PTRACE_GETFPREGS` les rend (format `FXSAVE`).
///
/// Les octets sont conservés bruts. Un XMM n'a pas de type : c'est
/// l'instruction qui décide s'il porte deux `double`, quatre `float` ou seize
/// octets, et c'est l'interprétation — donc l'affichage — qui tranche
/// (voir [`crate::simd`]). Convertir ici figerait ce choix trop tôt.
#[derive(Clone, PartialEq)]
pub struct FpRegisters {
    /// XMM0 à XMM15, 128 bits chacun, en ordre naturel (index = numéro).
    pub xmm: [u128; 16],
    /// Registres physiques de la pile x87, 80 bits chacun. Attention :
    /// l'index est le registre *physique*, pas `ST(i)` — la correspondance
    /// passe par TOP ([`Self::top`]).
    pub st: [[u8; 10]; 8],
    /// Mot de contrôle x87 (précision, arrondi, masques d'exception).
    pub fcw: u16,
    /// Mot d'état x87 : contient TOP (bits 11-13) et les exceptions levées.
    pub fsw: u16,
    /// Tag word *abrégé* du format FXSAVE : un bit par registre physique,
    /// à 1 quand il est occupé (le format FSAVE en utilisait deux).
    pub ftw: u8,
    /// Mot de contrôle/état SSE : arrondi, exceptions, DAZ/FTZ.
    pub mxcsr: u32,
}

impl FpRegisters {
    fn from_raw(r: &libc::user_fpregs_struct) -> Self {
        let mut xmm = [0u128; 16];
        for (i, slot) in xmm.iter_mut().enumerate() {
            // Chaque registre occupe quatre `u32` consécutifs, poids faible en tête.
            let mut bytes = [0u8; 16];
            for (j, w) in r.xmm_space[i * 4..i * 4 + 4].iter().enumerate() {
                bytes[j * 4..j * 4 + 4].copy_from_slice(&w.to_le_bytes());
            }
            *slot = u128::from_le_bytes(bytes);
        }
        let mut st = [[0u8; 10]; 8];
        for (i, slot) in st.iter_mut().enumerate() {
            // Un registre x87 occupe 80 bits utiles dans un créneau de 128.
            let mut bytes = [0u8; 16];
            for (j, w) in r.st_space[i * 4..i * 4 + 4].iter().enumerate() {
                bytes[j * 4..j * 4 + 4].copy_from_slice(&w.to_le_bytes());
            }
            slot.copy_from_slice(&bytes[..10]);
        }
        FpRegisters {
            xmm,
            st,
            fcw: r.cwd,
            fsw: r.swd,
            ftw: r.ftw as u8,
            mxcsr: r.mxcsr,
        }
    }

    /// Sommet de la pile x87 : `ST(0)` est le registre physique `top()`.
    pub fn top(&self) -> usize {
        ((self.fsw >> 11) & 0b111) as usize
    }

    /// Contenu de `ST(i)` (0 = sommet), avec le registre physique qui le porte
    /// et son occupation d'après le tag word.
    pub fn st_reg(&self, i: usize) -> (usize, [u8; 10], bool) {
        let phys = (self.top() + i) % 8;
        (phys, self.st[phys], self.ftw & (1 << phys) != 0)
    }
}

/// État complet du CPU à une étape donnée, conservé pour la timeline (M5).
///
/// La fenêtre de pile est un tableau inline et non un `Vec` : un snapshot est
/// enregistré à *chaque* instruction, et une allocation par pas coûtait plus
/// cher que la lecture mémoire elle-même.
#[derive(Clone)]
pub struct Snapshot {
    pub regs: Registers,
    /// Fenêtre de pile (`STACK_WINDOW` mots de 64 bits) à partir de RSP.
    pub stack: [u64; STACK_WINDOW],
    /// Registres SSE/x87 du moment — partagés avec les snapshots voisins tant
    /// qu'ils ne changent pas.
    ///
    /// Le bloc pèse 400 octets, contre 272 pour tout le reste du snapshot : le
    /// copier à chaque instruction triplerait la timeline pour rien, puisque la
    /// quasi-totalité des programmes ne touche jamais à un XMM. On le lit donc
    /// à chaque pas (il faut bien savoir s'il a changé) mais on ne le *garde*
    /// qu'une fois : tant que les octets sont identiques, tous les snapshots
    /// pointent le même `Arc`. Un programme sans flottants paie huit octets par
    /// pas ; un programme vectoriel paie le prix plein, et c'est justement
    /// celui qui veut voir la timeline de ses XMM.
    pub fp: Option<Arc<FpRegisters>>,
}

/// Plafond de l'historique. Au-delà, [`Debugger::step`] refuse d'avancer
/// plutôt que de laisser une boucle infinie remplir la RAM : un snapshot pèse
/// 280 octets, ce plafond borne donc la timeline à ~140 Mo — plus les blocs
/// flottants effectivement distincts, un par changement de XMM. Il ne peut pas
/// être contourné en oubliant les vieux snapshots — la timeline et
/// `resume_here` indexent l'historique par numéro d'étape absolu.
pub const MAX_HISTORY: usize = 500_000;

/// Budget d'attente d'un pas avant de rendre la main à l'interface. Une
/// instruction ordinaire s'arrête en quelques microsecondes ; seul un appel
/// système bloquant épuise ce budget, et c'est précisément le cas où l'UI doit
/// continuer à tourner.
const STEP_BUDGET: Duration = Duration::from_millis(20);

/// Durée pendant laquelle un appel système est guetté en attente active avant
/// de céder le CPU. Couvre largement un `write` sur un tuyau, qui est l'appel
/// que les programmes de l'élève passent en boucle.
const SPIN_BUDGET: Duration = Duration::from_micros(300);

/// Tuyaux d'E/S du processus tracé, côté parent.
struct Io {
    /// Extrémité de lecture du tuyau branché sur stdout+stderr de l'enfant
    /// (non bloquante : on la vide sans jamais attendre).
    out: OwnedFd,
    /// Extrémité d'écriture du tuyau branché sur stdin de l'enfant
    /// (non bloquante, pour la raison dite sur `outgoing`).
    inp: OwnedFd,
    /// Octets lus mais pas encore remis à l'appelant. Conservés en brut : une
    /// lecture peut couper un caractère multi-octets en deux, la queue
    /// incomplète attend le reste plutôt que de sortir en « � ».
    pending: Vec<u8>,
    /// Saisie pas encore avalée par le programme. Un tuyau ne tient que 64 Kio,
    /// et le programme n'en lit que ce qu'il veut : au-delà, écrire bloquerait
    /// jusqu'à ce qu'il consomme — c'est-à-dire peut-être jamais, l'IDE figé
    /// avec lui. On dépose donc ce qui passe et on garde le reste pour les
    /// frames suivantes.
    outgoing: Vec<u8>,
}

pub struct Debugger {
    child: Pid,
    pub state: RunState,
    /// Historique des états, un par instruction exécutée (index 0 = état initial).
    /// Permet le scrubbing de la timeline sans ré-exécuter (record-and-replay).
    pub history: Vec<Snapshot>,
    /// `/proc/<pid>/mem` ouvert une fois pour toutes : chaque snapshot lit la
    /// fenêtre de pile, et rouvrir le fichier à chaque lecture coûtait un
    /// `open`+`close` par mot de 64 bits.
    mem: File,
    /// Tuyaux stdin/stdout/stderr, ou `None` si la redirection a échoué (le
    /// programme écrit alors dans le terminal parent, comme avant).
    io: Option<Io>,
}

impl Debugger {
    /// Lance le binaire et s'arrête juste avant sa première instruction.
    pub fn launch(binary: &Path) -> DbgResult<Self> {
        let path = binary
            .to_str()
            .ok_or(DbgError::BadPath)?;
        let cpath = CString::new(path).map_err(|_| DbgError::BadPath)?;

        // Tuyaux créés AVANT le fork pour que l'enfant en hérite. `O_CLOEXEC`
        // sur les quatre extrémités : seules les copies posées par `dup2` (qui
        // efface le drapeau) survivent à l'`execve`, les originales se ferment
        // toutes seules. Sans quoi l'enfant garderait l'extrémité d'écriture
        // de son propre stdout et l'EOF n'arriverait jamais.
        let pipes = Pipes::new().ok();
        let (child_in, child_out) = match &pipes {
            Some(p) => (p.in_r.as_raw_fd(), p.out_w.as_raw_fd()),
            None => (-1, -1),
        };

        match unsafe { fork() }.map_err(|e| DbgError::sys("fork", e))? {
            ForkResult::Child => {
                // Dans l'enfant : uniquement des appels async-signal-safe avant
                // execve — `dup2` et `_exit` le sont.
                if child_out >= 0 {
                    unsafe {
                        libc::dup2(child_in, 0);
                        libc::dup2(child_out, 1);
                        libc::dup2(child_out, 2);
                    }
                }
                // Sans PTRACE_TRACEME, execve réussit mais le parent ne reçoit
                // jamais le SIGTRAP attendu. Ne pas présenter ce refus de
                // permission comme un échec mystérieux d'execve.
                if ptrace::traceme().is_err() {
                    unsafe { libc::_exit(126) };
                }
                let _ = execv(&cpath, std::slice::from_ref(&cpath));
                // execve a échoué : on sort sans dérouler la pile du parent.
                unsafe { libc::_exit(127) };
            }
            ForkResult::Parent { child } => {
                // Premier arrêt attendu : SIGTRAP juste après execve, avant la
                // 1re instruction. Si l'enfant est déjà mort, execve a échoué.
                match waitpid(child, None).map_err(|e| DbgError::sys("waitpid initial", e))? {
                    WaitStatus::Stopped(_, _) => {}
                    WaitStatus::Exited(_, 126) => {
                        return Err(DbgError::PtraceDenied);
                    }
                    WaitStatus::Exited(_, 127) => {
                        return Err(DbgError::NoStart);
                    }
                    WaitStatus::Exited(_, code) => {
                        return Err(DbgError::ExitedBeforeStart(code));
                    }
                    other => {
                        return Err(DbgError::UnexpectedInitialState(format!("{other:?}")));
                    }
                }
                // Le parent lâche les extrémités qui appartiennent à l'enfant :
                // tant qu'il tient `out_w`, lire `out` ne verrait jamais l'EOF.
                let io = pipes.map(Pipes::into_parent_side);
                let mem = open_mem(child)?;
                let regs = read_regs(child)?;
                let snap = snapshot_of(&mem, &regs, read_fpregs(child), None);
                Ok(Debugger {
                    child,
                    state: RunState::Stopped,
                    history: vec![snap],
                    mem,
                    io,
                })
            }
        }
    }

    /// Lance l'exécution d'une instruction machine. Rend la main dès l'arrêt
    /// obtenu (cas ordinaire) ou au bout de [`STEP_BUDGET`] en laissant l'état
    /// à [`RunState::Running`] — à charge pour l'appelant de rappeler
    /// [`Self::poll`] à chaque frame tant que c'est le cas.
    pub fn step(&mut self) -> DbgResult<()> {
        if self.state != RunState::Stopped {
            return Ok(());
        }
        if self.history.len() > MAX_HISTORY {
            return Err(DbgError::HistoryFull(MAX_HISTORY));
        }
        // Seul un appel système peut suspendre l'exécution durablement. Une
        // instruction ordinaire s'arrête en quelques microsecondes : l'attendre
        // en bloquant coûte un `waitpid`, alors que le sondage lui ferait payer
        // un tour de boucle et une sieste — deux ordres de grandeur de plus,
        // sur le chemin le plus chaud du débogueur.
        let may_block = self.at_syscall();
        ptrace::step(self.child, None).map_err(|e| DbgError::sys("singlestep", e))?;
        self.state = RunState::Running;
        if may_block {
            return self.poll();
        }
        let status = waitpid(self.child, None).map_err(|e| DbgError::sys("waitpid", e))?;
        self.finish_step(status)?;
        self.drain_output();
        Ok(())
    }

    /// Vrai si l'instruction sur le point de s'exécuter est un `syscall`
    /// (opcode `0F 05`) — la seule qui puisse rendre la main au noyau pour un
    /// temps indéterminé.
    fn at_syscall(&self) -> bool {
        let rip = self.head().regs.rip;
        matches!(read_mem_at(&self.mem, rip, 2).as_deref(), Ok([0x0F, 0x05]))
    }

    /// Récupère l'arrêt en attente si le pas lancé par [`Self::step`] s'est
    /// achevé. Sans effet dans tout autre état. Ne bloque jamais plus de
    /// [`STEP_BUDGET`].
    pub fn poll(&mut self) -> DbgResult<()> {
        if self.state != RunState::Running {
            return Ok(());
        }
        let start = Instant::now();
        let deadline = start + STEP_BUDGET;
        // Le programme est justement suspendu dans un `read` : c'est le moment
        // où il consomme ce qui attendait faute de place dans le tuyau.
        self.flush_input()?;
        loop {
            let status = waitpid(self.child, Some(WaitPidFlag::WNOHANG))
                .map_err(|e| DbgError::sys("waitpid", e))?;
            if status != WaitStatus::StillAlive {
                self.finish_step(status)?;
                // Le programme a pu écrire juste avant de rendre la main (ou de
                // mourir) : on vide le tuyau avant que l'appelant ne l'affiche.
                self.drain_output();
                return Ok(());
            }
            self.drain_output();
            let now = Instant::now();
            if now >= deadline {
                return Ok(()); // reste `Running`, l'UI reprend la main
            }
            // La plupart des appels système (`write`, `brk`…) reviennent en
            // quelques microsecondes : on les guette sans céder le CPU. Passé
            // ce court délai, celui-ci attend visiblement quelque chose
            // d'extérieur, et il vaut mieux dormir que tourner à vide.
            if now < start + SPIN_BUDGET {
                std::hint::spin_loop();
            } else {
                std::thread::sleep(Duration::from_micros(200));
            }
        }
    }

    /// Enregistre l'arrêt qui vient de survenir.
    fn finish_step(&mut self, status: WaitStatus) -> DbgResult<()> {
        match status {
            WaitStatus::Exited(_, code) => {
                self.state = RunState::Exited(code);
            }
            WaitStatus::Signaled(_, _, _) => {
                self.state = RunState::Signaled;
            }
            // Arrêt sur livraison de signal. SIGTRAP = fin normale du
            // single-step ; tout le reste est une faute matérielle qu'il faut
            // capturer AVANT de retenter, sinon l'instruction refaute sans fin.
            WaitStatus::Stopped(_, sig) if is_fault(sig) => {
                let regs = read_regs(self.child)?;
                // Le snapshot de l'instant de la faute est conservé : l'élève
                // doit pouvoir inspecter les registres qui l'ont causée.
                let snap = self.capture(&regs);
                self.history.push(snap);
                self.state = RunState::Faulted(Fault {
                    signal: sig,
                    addr: fault_addr(self.child),
                    rip: regs.rip,
                });
            }
            _ => {
                let regs = read_regs(self.child)?;
                let snap = self.capture(&regs);
                self.history.push(snap);
                self.state = RunState::Stopped;
            }
        }
        Ok(())
    }

    /// Vide le tuyau de sortie dans le tampon interne. Non bloquant.
    fn drain_output(&mut self) {
        let Some(io) = self.io.as_mut() else { return };
        let mut buf = [0u8; 8192];
        loop {
            let n = unsafe { libc::read(io.out.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
            match n {
                // 0 = EOF (le programme est mort), < 0 = EAGAIN sur un tuyau
                // vide dans le cas normal : dans les deux cas il n'y a plus
                // rien à lire maintenant.
                n if n <= 0 => return,
                n => io.pending.extend_from_slice(&buf[..n as usize]),
            }
        }
    }

    /// Retire et rend ce que le programme a écrit depuis le dernier appel.
    /// La queue d'un caractère multi-octets coupé en deux reste en attente.
    pub fn take_output(&mut self) -> String {
        self.drain_output();
        // Appelé à chaque frame par l'interface : c'est le battement qui écoule
        // aussi la saisie restée en attente.
        let _ = self.flush_input();
        let Some(io) = self.io.as_mut() else {
            return String::new();
        };
        let valid = match std::str::from_utf8(&io.pending) {
            Ok(_) => io.pending.len(),
            Err(e) => e.valid_up_to(),
        };
        let rest = io.pending.split_off(valid);
        let out = String::from_utf8_lossy(&io.pending).into_owned();
        io.pending = rest;
        out
    }

    /// Envoie une ligne sur l'entrée standard du programme. C'est ce qui
    /// débloque un `read` en attente (état [`RunState::Running`]).
    pub fn write_stdin(&mut self, line: &str) -> DbgResult<()> {
        let Some(io) = self.io.as_mut() else {
            return Err(DbgError::NoStdin);
        };
        io.outgoing.extend_from_slice(line.as_bytes());
        self.flush_input()
    }

    /// Pousse vers le programme ce qui reste de saisie en attente, sans jamais
    /// attendre. Ce qui ne passe pas — tuyau plein, programme qui ne lit pas
    /// encore — reste en file pour la frame suivante.
    fn flush_input(&mut self) -> DbgResult<()> {
        let Some(io) = self.io.as_mut() else {
            return Ok(());
        };
        let mut sent = 0;
        while sent < io.outgoing.len() {
            let rest = &io.outgoing[sent..];
            let n = unsafe { libc::write(io.inp.as_raw_fd(), rest.as_ptr().cast(), rest.len()) };
            if n < 0 {
                let err = std::io::Error::last_os_error();
                match err.kind() {
                    // Interrompu par un signal : rien n'est perdu, on réessaie.
                    std::io::ErrorKind::Interrupted => continue,
                    // Tuyau plein : le programme n'a pas encore lu. On garde
                    // le reste et on rendra la main.
                    std::io::ErrorKind::WouldBlock => break,
                    // Le programme a fermé son entrée ou s'est terminé sans
                    // tout lire — le cas de `read-stdin.asm`, qui prend
                    // 64 octets et sort. Un shell n'en dit rien non plus : on
                    // jette ce qui reste, sans en faire une erreur du pas.
                    std::io::ErrorKind::BrokenPipe => {
                        io.outgoing.clear();
                        return Ok(());
                    }
                    _ => {
                        io.outgoing.drain(..sent);
                        return Err(DbgError::sys(
                            i18n::tr3(
                                Lang::Fr,
                                "écriture sur l'entrée standard",
                                "writing to standard input",
                                "escritura en la entrada estándar",
                            ),
                            err,
                        ));
                    }
                }
            }
            sent += n as usize;
        }
        io.outgoing.drain(..sent);
        Ok(())
    }

    /// Vrai si le programme peut recevoir de l'entrée (tuyau branché).
    pub fn has_stdin(&self) -> bool {
        self.io.is_some()
    }

    /// Enchaîne les pas jusqu'à ce que `stop` accepte l'état atteint, que le
    /// programme se termine, ou que `budget` instructions soient écoulées.
    /// Renvoie le nombre d'instructions exécutées.
    ///
    /// `stop` reçoit tous les registres et non le seul RIP : c'est ce qui
    /// permet aux points d'arrêt conditionnels de trancher (« RCX == 0 »)
    /// sans que le débogueur ait à connaître leur grammaire.
    ///
    /// Le single-step est conservé plutôt qu'un vrai `int3` : chaque étape doit
    /// entrer dans l'historique, sans quoi la timeline aurait des trous entre
    /// deux points d'arrêt. Le budget rend la main à l'interface — une boucle
    /// infinie ne fige donc rien, et l'appelant peut relancer.
    pub fn run_until(
        &mut self,
        budget: usize,
        stop: impl Fn(&Registers) -> bool,
    ) -> DbgResult<usize> {
        let mut done = 0;
        while done < budget {
            self.step()?;
            // Appel système bloquant : on rend la main, l'appelant reprendra
            // quand `poll` aura débloqué le pas.
            if self.is_waiting() {
                return Ok(done);
            }
            done += 1;
            if !self.is_ready() {
                return Ok(done); // terminé, tué ou en faute
            }
            if stop(self.regs()) {
                return Ok(done);
            }
        }
        Ok(done)
    }

    /// Faute matérielle en cours, si l'exécution s'est arrêtée dessus.
    pub fn fault(&self) -> Option<Fault> {
        match self.state {
            RunState::Faulted(f) => Some(f),
            _ => None,
        }
    }

    /// Snapshot de tête (état courant).
    pub fn head(&self) -> &Snapshot {
        self.history.last().expect("history non vide")
    }

    /// Registres courants (tête de l'historique).
    pub fn regs(&self) -> &Registers {
        &self.head().regs
    }

    /// Nombre d'instructions exécutées (index max de la timeline).
    pub fn steps(&self) -> usize {
        self.history.len() - 1
    }

    /// Le programme tourne encore (arrêté sur une instruction, ou en train
    /// d'exécuter un appel système qui prend son temps).
    pub fn is_alive(&self) -> bool {
        matches!(self.state, RunState::Stopped | RunState::Running)
    }

    /// Prêt à recevoir un nouveau pas (par opposition à [`RunState::Running`],
    /// où le pas précédent n'est pas encore terminé).
    pub fn is_ready(&self) -> bool {
        self.state == RunState::Stopped
    }

    /// Le programme est suspendu dans un appel système (le plus souvent un
    /// `read` qui attend une saisie).
    pub fn is_waiting(&self) -> bool {
        self.state == RunState::Running
    }

    /// PID du processus tracé.
    pub fn pid(&self) -> i32 {
        self.child.as_raw()
    }

    /// Lit `len` octets à l'adresse `addr` dans l'espace mémoire du tracé (état
    /// courant vivant), via `/proc/<pid>/mem`.
    pub fn read_mem(&self, addr: u64, len: usize) -> DbgResult<Vec<u8>> {
        read_mem_at(&self.mem, addr, len)
    }

    /// Modifie un registre (processus arrêté requis), puis met à jour le
    /// snapshot courant. Utilisé par le « laboratoire mémoire ».
    pub fn set_register(&mut self, name: &str, value: u64) -> DbgResult<()> {
        if self.state != RunState::Stopped {
            return Err(DbgError::NotStopped);
        }
        let mut r = ptrace::getregs(self.child).map_err(|e| DbgError::sys("getregs", e))?;
        match name {
            "RAX" => r.rax = value,
            "RBX" => r.rbx = value,
            "RCX" => r.rcx = value,
            "RDX" => r.rdx = value,
            "RSI" => r.rsi = value,
            "RDI" => r.rdi = value,
            "RBP" => r.rbp = value,
            "RSP" => r.rsp = value,
            "R8" => r.r8 = value,
            "R9" => r.r9 = value,
            "R10" => r.r10 = value,
            "R11" => r.r11 = value,
            "R12" => r.r12 = value,
            "R13" => r.r13 = value,
            "R14" => r.r14 = value,
            "R15" => r.r15 = value,
            "RIP" => r.rip = value,
            "EFLAGS" => r.eflags = value,
            _ => return Err(DbgError::UnknownRegister(name.to_string())),
        }
        ptrace::setregs(self.child, r).map_err(|e| DbgError::sys("setregs", e))?;
        self.refresh_head()
    }

    /// Écrit des octets en mémoire (processus arrêté), puis met à jour le snapshot.
    pub fn write_mem(&mut self, addr: u64, bytes: &[u8]) -> DbgResult<()> {
        if self.state != RunState::Stopped {
            return Err(DbgError::NotStopped);
        }
        write_mem_at(&self.mem, addr, bytes)?;
        self.refresh_head()
    }

    /// Capture l'état courant du processus tracé, en réutilisant le bloc
    /// flottant du snapshot précédent s'il n'a pas changé.
    fn capture(&self, regs: &Registers) -> Snapshot {
        snapshot_of(
            &self.mem,
            regs,
            read_fpregs(self.child),
            self.history.last(),
        )
    }

    /// Recharge le snapshot de tête depuis le processus (après une édition).
    fn refresh_head(&mut self) -> DbgResult<()> {
        let regs = read_regs(self.child)?;
        // Le snapshot remplacé est encore en queue : c'est bien celui d'avant
        // qui doit servir de référence pour le partage du bloc flottant.
        let snap = snapshot_of(
            &self.mem,
            &regs,
            read_fpregs(self.child),
            self.history
                .len()
                .checked_sub(2)
                .and_then(|i| self.history.get(i)),
        );
        if let Some(h) = self.history.last_mut() {
            *h = snap;
        }
        Ok(())
    }

    /// Toutes les régions mappées du processus (`/proc/<pid>/maps`), pour la vue
    /// mémoire unifiée. Les régions sans intérêt pédagogique (bibliothèques
    /// partagées, vvar/vdso) sont écartées afin de garder un schéma lisible.
    pub fn mem_regions(&self) -> Vec<MemRegion> {
        let Ok(maps) = std::fs::read_to_string(format!("/proc/{}/maps", self.child.as_raw()))
        else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for line in maps.lines() {
            let mut it = line.split_whitespace();
            let Some(range) = it.next() else { continue };
            let Some(perms) = it.next() else { continue };
            let Some((s, e)) = range.split_once('-') else {
                continue;
            };
            let (Ok(start), Ok(end)) = (u64::from_str_radix(s, 16), u64::from_str_radix(e, 16))
            else {
                continue;
            };
            // 6e champ = chemin ou pseudo-nom entre crochets (absent pour l'anonyme).
            let path = line.split_whitespace().nth(5).unwrap_or("");
            let kind = match path {
                "[stack]" => RegionKind::Stack,
                "[heap]" => RegionKind::Heap,
                "[vvar]" | "[vdso]" | "[vsyscall]" => continue,
                p if p.contains(".so") => continue,
                _ => {
                    // Segments de l'exécutable : x → code, w → données, sinon lecture seule.
                    if perms.contains('x') {
                        RegionKind::Code
                    } else if perms.contains('w') {
                        RegionKind::Data
                    } else {
                        RegionKind::Rodata
                    }
                }
            };
            out.push(MemRegion {
                start,
                end,
                kind,
                perms: perms.to_string(),
            });
        }
        out.sort_by_key(|r| r.start);
        out
    }

    /// Bornes (début, fin) du segment `[heap]` d'après `/proc/<pid>/maps`,
    /// ou `None` si le programme n'a pas encore de tas.
    pub fn heap_range(&self) -> Option<(u64, u64)> {
        let maps = std::fs::read_to_string(format!("/proc/{}/maps", self.child.as_raw())).ok()?;
        for line in maps.lines() {
            if line.trim_end().ends_with("[heap]") {
                let (start, end) = line.split_whitespace().next()?.split_once('-')?;
                return Some((
                    u64::from_str_radix(start, 16).ok()?,
                    u64::from_str_radix(end, 16).ok()?,
                ));
            }
        }
        None
    }
}

impl Drop for Debugger {
    fn drop(&mut self) {
        // Évite les zombies si on relance ou on ferme pendant l'exécution.
        // ptrace::kill() est déprécié sur Linux moderne ; on envoie SIGKILL directement.
        // `Running` compte aussi : un programme suspendu dans un `read` est
        // bien vivant, et c'est justement celui qu'on abandonne le plus souvent.
        if self.is_alive() {
            let _ = nix::sys::signal::kill(self.child, nix::sys::signal::Signal::SIGKILL);
            let _ = waitpid(self.child, None);
        }
    }
}

/// Les quatre extrémités des deux tuyaux, entre leur création et le partage
/// parent/enfant qui suit le `fork`.
struct Pipes {
    /// Côté enfant : son stdin.
    in_r: OwnedFd,
    /// Côté parent : de quoi alimenter ce stdin.
    in_w: OwnedFd,
    /// Côté parent : de quoi lire ce que l'enfant écrit.
    out_r: OwnedFd,
    /// Côté enfant : ses stdout et stderr.
    out_w: OwnedFd,
}

impl Pipes {
    fn new() -> Result<Self, std::io::Error> {
        let (in_r, in_w) = new_pipe()?;
        let (out_r, out_w) = new_pipe()?;
        // Lecture non bloquante : la console se vide à chaque frame, elle ne
        // doit jamais attendre après un programme qui n'écrit rien. Écriture
        // non bloquante pour la raison symétrique : ni l'une ni l'autre ne doit
        // pouvoir suspendre le thread qui peint l'interface.
        set_nonblocking(&out_r)?;
        set_nonblocking(&in_w)?;
        Ok(Pipes {
            in_r,
            in_w,
            out_r,
            out_w,
        })
    }

    /// Ne garde que les extrémités du parent ; les deux autres se ferment ici,
    /// ce qui est la condition pour que l'EOF remonte de part et d'autre.
    fn into_parent_side(self) -> Io {
        Io {
            out: self.out_r,
            inp: self.in_w,
            pending: Vec::new(),
            outgoing: Vec::new(),
        }
    }
}

/// Crée un tuyau dont les deux extrémités se ferment à l'`execve`.
fn new_pipe() -> Result<(OwnedFd, OwnedFd), std::io::Error> {
    let mut fds = [0 as libc::c_int; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

fn set_nonblocking(fd: &OwnedFd) -> Result<(), std::io::Error> {
    let raw = fd.as_raw_fd();
    let flags = unsafe { libc::fcntl(raw, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(raw, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Ouvre `/proc/<pid>/mem` en lecture-écriture, une fois pour la vie du tracé.
fn open_mem(pid: Pid) -> DbgResult<File> {
    let path = format!("/proc/{}/mem", pid.as_raw());
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|e| DbgError::sys(format!("open {path}"), e))
}

/// Vrai si ce signal traduit une faute matérielle (et non la fin normale d'un
/// single-step, signalée par `SIGTRAP`).
fn is_fault(sig: nix::sys::signal::Signal) -> bool {
    use nix::sys::signal::Signal::*;
    matches!(sig, SIGSEGV | SIGFPE | SIGILL | SIGBUS)
}

/// Adresse fautive (`siginfo_t::si_addr`) via `PTRACE_GETSIGINFO`.
///
/// C'est la seule source fiable : pour un déréférencement, RIP donne
/// l'instruction, mais seul `si_addr` donne l'adresse *visée*.
fn fault_addr(pid: Pid) -> Option<u64> {
    let info = ptrace::getsiginfo(pid).ok()?;
    // si_addr vit dans une union ; sur Linux/x86-64 il occupe le premier champ
    // du variant _sigfault, aligné après si_signo/si_errno/si_code.
    let addr = unsafe { info.si_addr() } as u64;
    Some(addr)
}

fn read_regs(pid: Pid) -> DbgResult<Registers> {
    let raw = ptrace::getregs(pid).map_err(|e| DbgError::sys("getregs", e))?;
    Ok(Registers::from_raw(&raw))
}

/// Lit les registres SSE/x87. Rend `None` plutôt qu'une erreur : un processus
/// qui vient de mourir n'a plus de registres flottants, et ce n'est pas une
/// raison pour perdre le snapshot des registres généraux.
fn read_fpregs(pid: Pid) -> Option<FpRegisters> {
    ptrace::getregset::<ptrace::regset::NT_PRFPREG>(pid)
        .ok()
        .map(|raw| FpRegisters::from_raw(&raw))
}

/// Lit `len` octets à `addr` via un `/proc/<pid>/mem` déjà ouvert.
fn read_mem_at(mem: &File, addr: u64, len: usize) -> DbgResult<Vec<u8>> {
    use std::os::unix::fs::FileExt;

    let mut buf = vec![0u8; len];
    mem.read_exact_at(&mut buf, addr)
        .map_err(|e| DbgError::sys(format!("read @0x{addr:X}"), e))?;
    Ok(buf)
}

/// Écrit `bytes` à `addr` via un `/proc/<pid>/mem` déjà ouvert.
fn write_mem_at(mem: &File, addr: u64, bytes: &[u8]) -> DbgResult<()> {
    use std::os::unix::fs::FileExt;

    mem.write_all_at(bytes, addr)
        .map_err(|e| DbgError::sys(format!("write @0x{addr:X}"), e))?;
    Ok(())
}

/// Capture un snapshot (registres + fenêtre de pile) de l'état courant.
///
/// La fenêtre de pile part en une seule lecture : elle est contiguë, et c'est
/// le chemin le plus chaud du débogueur (une fois par instruction exécutée).
/// Si elle déborde de la région mappée — RSP à moins de 128 octets du sommet —
/// la lecture groupée échoue en bloc : on retombe alors mot par mot pour
/// garder ce qui est lisible, plutôt que d'afficher une pile vide.
/// `previous` est le snapshot qui précède, s'il existe : quand les registres
/// flottants n'ont pas bougé — le cas de presque toutes les instructions — son
/// bloc est repris tel quel au lieu d'en allouer un identique.
fn snapshot_of(
    mem: &File,
    regs: &Registers,
    fp: Option<FpRegisters>,
    previous: Option<&Snapshot>,
) -> Snapshot {
    let fp = match (fp, previous.and_then(|p| p.fp.as_ref())) {
        (Some(new), Some(old)) if *old.as_ref() == new => Some(Arc::clone(old)),
        (Some(new), _) => Some(Arc::new(new)),
        (None, _) => None,
    };
    let mut stack = [0u64; STACK_WINDOW];
    match read_mem_at(mem, regs.rsp, STACK_WINDOW * 8) {
        Ok(bytes) => {
            for (word, chunk) in stack.iter_mut().zip(bytes.chunks_exact(8)) {
                *word = u64::from_le_bytes(chunk.try_into().expect("chunks_exact(8)"));
            }
        }
        Err(_) => {
            for (i, word) in stack.iter_mut().enumerate() {
                let addr = regs.rsp.wrapping_add((i as u64) * 8);
                *word = read_mem_at(mem, addr, 8)
                    .map(|b| u64::from_le_bytes(b.try_into().expect("8 octets")))
                    .unwrap_or(0);
            }
        }
    }
    Snapshot {
        regs: regs.clone(),
        stack,
        fp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble;
    use std::path::Path;

    /// Les trois langues répondent, et jamais avec le texte d'une autre : ces
    /// messages partent dans la console de l'élève, qui a pu mettre l'IDE en
    /// anglais ou en espagnol.
    #[test]
    fn every_error_speaks_the_interface_language() {
        let cases = [
            DbgError::BadPath,
            DbgError::PtraceDenied,
            DbgError::NoStart,
            DbgError::ExitedBeforeStart(3),
            DbgError::UnexpectedInitialState("Continued".to_string()),
            DbgError::HistoryFull(MAX_HISTORY),
            DbgError::NoStdin,
            DbgError::NotStopped,
            DbgError::UnknownRegister("RXX".to_string()),
        ];
        for e in cases {
            let (fr, en, es) = (e.message(Lang::Fr), e.message(Lang::En), e.message(Lang::Es));
            assert!(!fr.is_empty(), "{e:?} : message vide");
            assert_ne!(fr, en, "{e:?} : le français et l'anglais disent la même chose");
            assert_ne!(en, es, "{e:?} : l'anglais et l'espagnol disent la même chose");
        }
        // L'erreur système est le seul cas sans traduction : le texte vient de
        // l'OS, et c'est lui qui explique ce qui s'est passé.
        let sys = DbgError::sys("getregs", "No such process");
        assert_eq!(sys.message(Lang::Fr), sys.message(Lang::En));
        assert!(sys.message(Lang::Es).contains("No such process"));
    }

    /// Assemble l'exemple, le lance sous ptrace, le fait avancer jusqu'à la fin.
    /// Vérifie qu'après `cmp rax, rbx` (5 vs 8) les flags sont cohérents :
    /// résultat négatif => ZF=0, SF=1, CF=1 (emprunt), OF=0.
    #[test]
    fn step_through_example_sets_flags() {
        // Dossier dédié : évite toute collision avec les autres tests parallèles.
        let out = assemble::assemble_with_includes(
            Path::new("examples/test.asm"),
            Path::new("build/test-dbg"),
            &[],
        )
        .expect("assemblage");
        let mut dbg = Debugger::launch(&out.binary).expect("launch");

        // mov rax,5 ; push rax ; mov rbx,8 ; cmp rax,rbx  => 4 steps jusqu'au cmp inclus.
        for _ in 0..4 {
            dbg.step().expect("step");
            assert!(dbg.is_alive(), "le programme ne devrait pas être terminé");
        }
        let f = Flags::from_eflags(dbg.regs().eflags);
        assert!(!f.zf, "ZF doit être 0 (5 != 8)");
        assert!(f.sf, "SF doit être 1 (5 - 8 < 0)");
        assert!(f.cf, "CF doit être 1 (emprunt sur 5 - 8)");
        assert!(!f.of, "OF doit être 0");

        // Fait tourner jusqu'à la sortie.
        for _ in 0..1000 {
            if !dbg.is_alive() {
                break;
            }
            dbg.step().expect("step");
        }
        assert!(matches!(dbg.state, RunState::Exited(0)), "exit 0 attendu");
    }

    /// M5 : l'historique enregistre un snapshot par step, et la fenêtre de pile
    /// capture bien la valeur empilée par `push rax`.
    #[test]
    fn history_records_snapshots_and_stack() {
        let out = assemble::assemble_with_includes(
            Path::new("examples/test.asm"),
            Path::new("build/test-hist"),
            &[],
        )
        .expect("assemblage");
        let mut dbg = Debugger::launch(&out.binary).expect("launch");

        assert_eq!(dbg.history.len(), 1, "état initial = 1 snapshot");
        assert_eq!(dbg.steps(), 0);

        // mov rax,5 ; push rax  => après 2 steps, le sommet de pile vaut 5.
        dbg.step().expect("step");
        dbg.step().expect("step");
        assert_eq!(dbg.history.len(), 3, "3 snapshots (initial + 2 steps)");
        assert_eq!(dbg.head().stack[0], 5, "push rax a empilé 5");

        // Le snapshot de tête reflète bien les registres courants.
        assert_eq!(dbg.head().regs.rip, dbg.regs().rip);
    }

    /// Après un brk qui agrandit le tas, le segment [heap] doit être détecté.
    #[test]
    fn heap_range_detected_after_brk() {
        let out = assemble::assemble_with_includes(
            Path::new("examples/heap.asm"),
            Path::new("build/test-heap"),
            &[],
        )
        .expect("assemblage");
        let mut dbg = Debugger::launch(&out.binary).expect("launch");

        assert!(dbg.heap_range().is_none(), "pas de tas au démarrage");

        // 7 instructions jusqu'au second brk inclus (mov/xor/syscall/mov/add/mov/syscall).
        for _ in 0..7 {
            dbg.step().expect("step");
        }
        assert!(dbg.is_alive(), "le programme doit encore tourner");
        let (start, end) = dbg.heap_range().expect("le tas doit exister après brk");
        assert!(end > start, "le tas doit avoir une taille non nulle");
    }

    /// La vue mémoire unifiée a besoin des régions classées : au minimum du code
    /// exécutable et une pile, et RIP/RSP doivent tomber dans les bonnes régions.
    #[test]
    fn mem_regions_classify_code_and_stack() {
        let out = assemble::assemble_with_includes(
            Path::new("examples/test.asm"),
            Path::new("build/test-regions"),
            &[],
        )
        .expect("assemblage");
        let dbg = Debugger::launch(&out.binary).expect("launch");
        let regions = dbg.mem_regions();

        assert!(!regions.is_empty(), "au moins une région mappée");
        assert!(
            regions.iter().any(|r| r.kind == RegionKind::Code),
            "le segment exécutable doit être détecté"
        );
        assert!(
            regions.iter().any(|r| r.kind == RegionKind::Stack),
            "la pile doit être détectée"
        );
        // Les régions sont triées et non vides.
        assert!(
            regions.windows(2).all(|w| w[0].start <= w[1].start),
            "régions triées"
        );
        assert!(regions.iter().all(|r| r.size() > 0), "taille non nulle");

        // RIP pointe dans du code, RSP dans la pile : c'est ce que le schéma relie.
        let regs = dbg.regs();
        let rip_region = regions.iter().find(|r| r.contains(regs.rip));
        assert_eq!(
            rip_region.map(|r| r.kind),
            Some(RegionKind::Code),
            "RIP doit tomber dans le segment exécutable"
        );
        let rsp_region = regions.iter().find(|r| r.contains(regs.rsp));
        assert_eq!(
            rsp_region.map(|r| r.kind),
            Some(RegionKind::Stack),
            "RSP doit tomber dans la pile"
        );
    }

    /// Ce que le programme écrit doit arriver dans l'IDE, et non dans le
    /// terminal parent — invisible quand l'application est lancée depuis un
    /// lanceur de bureau.
    #[test]
    fn program_output_is_captured() {
        let out = assemble::assemble_with_includes(
            Path::new("examples/test.asm"),
            Path::new("build/test-stdout"),
            &[],
        )
        .expect("assemblage");
        let mut dbg = Debugger::launch(&out.binary).expect("launch");

        for _ in 0..100 {
            if !dbg.is_alive() {
                break;
            }
            dbg.step().expect("step");
        }
        // test.asm écrit les 10 premiers octets de son message.
        assert_eq!(dbg.take_output(), "Bonjour de");
        // Une seconde prise ne redonne pas les mêmes octets.
        assert_eq!(dbg.take_output(), "");
    }

    /// Un `read` sans donnée disponible laisse le pas en suspens au lieu de
    /// bloquer l'appelant : c'est ce qui gelait l'interface entière.
    #[test]
    fn a_blocking_read_suspends_the_step_instead_of_hanging() {
        let out = assemble::assemble_with_includes(
            Path::new("examples/read-stdin.asm"),
            Path::new("build/test-stdin"),
            &[],
        )
        .expect("assemblage");
        let mut dbg = Debugger::launch(&out.binary).expect("launch");

        // mov rax,0 ; mov rdi,0 ; mov rsi,buf ; mov rdx,64 => 4 pas avant le syscall.
        for _ in 0..4 {
            dbg.step().expect("step");
            assert!(dbg.is_ready(), "aucune de ces instructions ne bloque");
        }
        // Le pas qui exécute `read` rend la main sans avoir abouti.
        let started = Instant::now();
        dbg.step().expect("step");
        assert!(dbg.is_waiting(), "le pas doit rester en attente d'entrée");
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "step() a bloqué {:?} au lieu de rendre la main",
            started.elapsed()
        );
        // Sonder ne change rien tant que personne n'écrit.
        dbg.poll().expect("poll");
        assert!(dbg.is_waiting(), "toujours en attente sans saisie");

        // La saisie débloque le syscall.
        dbg.write_stdin("salut\n").expect("write_stdin");
        for _ in 0..50 {
            dbg.poll().expect("poll");
            if !dbg.is_waiting() {
                break;
            }
        }
        assert!(dbg.is_ready(), "le read doit avoir abouti");
        assert_eq!(dbg.regs().rax, 6, "read a lu « salut\\n », soit 6 octets");

        // Et le programme réécrit ce qu'il a lu, qui nous revient par la sortie.
        for _ in 0..100 {
            if !dbg.is_alive() {
                break;
            }
            dbg.step().expect("step");
        }
        assert_eq!(dbg.take_output(), "salut\n");
    }

    /// Une saisie plus grosse que le tuyau ne doit pas suspendre l'appelant :
    /// le programme ne lit que 64 octets, le reste attendrait indéfiniment
    /// qu'il en veuille davantage — et l'interface avec.
    #[test]
    fn a_huge_input_does_not_block_the_caller() {
        let out = assemble::assemble_with_includes(
            Path::new("examples/read-stdin.asm"),
            Path::new("build/test-bigstdin"),
            &[],
        )
        .expect("assemblage");
        let mut dbg = Debugger::launch(&out.binary).expect("launch");

        // Bien au-delà des 64 Kio que retient un tuyau sous Linux.
        let huge = "x".repeat(256 * 1024);
        let started = Instant::now();
        dbg.write_stdin(&huge).expect("write_stdin");
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "write_stdin a bloqué {:?} au lieu de mettre en file",
            started.elapsed()
        );

        // Le programme lit ses 64 octets et repart : ce qui compte est qu'il
        // ait bien reçu le début de la saisie, pas qu'il ait tout avalé.
        for _ in 0..200 {
            if !dbg.is_alive() {
                break;
            }
            dbg.step().expect("step");
            dbg.poll().expect("poll");
        }
        assert!(
            dbg.take_output().starts_with("xxx"),
            "le programme doit avoir reçu et réécrit le début de la saisie"
        );
    }

    /// `run_until` enchaîne les pas jusqu'à l'adresse visée, en gardant un
    /// snapshot par instruction — sans quoi la timeline aurait des trous.
    #[test]
    fn run_until_stops_at_the_requested_address_without_skipping_history() {
        let out = assemble::assemble_with_includes(
            Path::new("examples/test.asm"),
            Path::new("build/test-runto"),
            &[],
        )
        .expect("assemblage");
        let mut dbg = Debugger::launch(&out.binary).expect("launch");

        // Cible : l'adresse atteinte après 3 pas, retrouvée en rejouant.
        let mut probe = Debugger::launch(&out.binary).expect("launch");
        for _ in 0..3 {
            probe.step().expect("step");
        }
        let target = probe.regs().rip;
        drop(probe);

        let done = dbg
            .run_until(1000, |regs| regs.rip == target)
            .expect("run_until");
        assert_eq!(done, 3, "trois instructions exécutées");
        assert_eq!(dbg.regs().rip, target, "arrêt sur l'adresse visée");
        assert_eq!(dbg.history.len(), 4, "un snapshot par pas, aucun trou");
    }

    /// Le budget borne l'exécution : une boucle infinie rend la main plutôt
    /// que de figer l'appelant.
    #[test]
    fn run_until_gives_up_when_the_budget_runs_out() {
        let out = assemble::assemble_with_includes(
            Path::new("examples/test.asm"),
            Path::new("build/test-budget"),
            &[],
        )
        .expect("assemblage");
        let mut dbg = Debugger::launch(&out.binary).expect("launch");

        // Condition d'arrêt jamais remplie : seul le budget peut interrompre.
        let done = dbg.run_until(3, |_| false).expect("run_until");
        assert_eq!(done, 3, "exactement le budget");
        assert!(
            dbg.is_ready(),
            "le programme est toujours là, prêt à continuer"
        );
    }

    /// Laboratoire mémoire : éditer un registre et écrire en mémoire.
    #[test]
    fn edit_register_and_memory() {
        let out = assemble::assemble_with_includes(
            Path::new("examples/test.asm"),
            Path::new("build/test-lab"),
            &[],
        )
        .expect("assemblage");
        let mut dbg = Debugger::launch(&out.binary).expect("launch");

        dbg.set_register("RAX", 0xDEAD_BEEF).expect("set RAX");
        assert_eq!(dbg.regs().rax, 0xDEAD_BEEF, "RAX doit refléter l'édition");

        let rsp = dbg.regs().rsp;
        dbg.write_mem(rsp, &[0x11, 0x22, 0x33, 0x44])
            .expect("write mem");
        assert_eq!(
            dbg.read_mem(rsp, 4).expect("read mem"),
            vec![0x11, 0x22, 0x33, 0x44],
            "la mémoire écrite doit être relue à l'identique"
        );
    }

    /// Les registres XMM sont bien lus, et bien lus *dans le bon ordre* : un
    /// `addsd` écrit dans les 64 bits bas, un `paddd` dans les quatre cases de
    /// 32 bits. Se tromper d'ordre passerait inaperçu à l'œil (des chiffres
    /// s'affichent quand même) mais enseignerait le contraire de la vérité.
    #[test]
    fn xmm_registers_are_read_from_the_process() {
        let out = assemble::assemble_with_includes(
            Path::new("examples/simd.asm"),
            Path::new("build/test-simd"),
            &[],
        )
        .expect("assemblage de simd.asm");
        let mut dbg = Debugger::launch(&out.binary).expect("launch");

        // Jusqu'au `mov rax, 60` : tous les calculs SSE sont faits.
        for _ in 0..1000 {
            if !dbg.is_alive() || dbg.regs().rax == 60 {
                break;
            }
            dbg.step().expect("step");
        }
        let fp = dbg
            .head()
            .fp
            .as_ref()
            .expect("registres flottants disponibles");

        // xmm0 = (2.0 + 3.0) × 2.0, dans la case basse ; la haute reste nulle.
        assert_eq!(f64::from_bits(fp.xmm[0] as u64), 10.0, "xmm0 (double bas)");
        assert_eq!(
            fp.xmm[0] >> 64,
            0,
            "la moitié haute ne bouge pas sur un addsd"
        );

        // xmm2 = [1,2,3,4] + [10,20,30,40], quatre entiers de 32 bits.
        let lanes: Vec<i32> = (0..4)
            .map(|i| (fp.xmm[2] >> (32 * i)) as u32 as i32)
            .collect();
        assert_eq!(lanes, vec![11, 22, 33, 44], "paddd, case basse en premier");

        // Et ce que l'élève lit dans le panneau dit la même chose.
        assert_eq!(
            crate::simd::lanes(fp.xmm[2], crate::simd::XmmView::I32),
            vec!["11", "22", "33", "44"]
        );
        assert_eq!(
            crate::simd::lanes(fp.xmm[0], crate::simd::XmmView::F64)[0],
            "10"
        );
    }

    /// Le bloc flottant est partagé tant qu'il ne change pas : un programme qui
    /// n'utilise aucun XMM ne doit pas payer 400 octets par instruction.
    #[test]
    fn unchanged_fp_block_is_shared_across_snapshots() {
        let out = assemble::assemble_with_includes(
            Path::new("examples/test.asm"),
            Path::new("build/test-fpshare"),
            &[],
        )
        .expect("assemblage");
        let mut dbg = Debugger::launch(&out.binary).expect("launch");
        for _ in 0..4 {
            dbg.step().expect("step");
        }
        let first = dbg.history[0].fp.as_ref().expect("bloc flottant initial");
        for (i, s) in dbg.history.iter().enumerate() {
            let fp = s.fp.as_ref().expect("bloc flottant");
            assert!(
                Arc::ptr_eq(first, fp),
                "snapshot {i} : test.asm ne touche à aucun XMM, le bloc doit être le même objet"
            );
        }
    }
}
