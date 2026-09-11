//! Client RPC synchrone vers l'agent de la VM — l'équivalent macOS de
//! [`crate::debugger::Debugger`] sur Linux, avec *exactement* la même
//! surface publique (`launch`, `step`, `poll`, `run_until`, `read_mem`,
//! `write_mem`, `set_register`, `take_output`, `write_stdin`, `watch`,
//! `unwatch`, `mem_regions`, `heap_range`, `fault`, `load_bias`, `regs`,
//! `head`, `history`, `steps`, `pid`), pour que les sites d'appel dans
//! `app/debug_ops.rs` n'aient à changer que leur import (voir le plan de
//! portage macOS).
//!
//! `read_mem`/`mem_regions`/`heap_range` sont `&self` sur `Debugger` (une
//! lecture `/proc/<pid>/mem` locale n'a rien à muter) mais ont ici besoin
//! d'un aller-retour réseau : la connexion est donc derrière un `RefCell`
//! plutôt que de forcer ces méthodes en `&mut self`, ce qui aurait fait
//! remonter la mutabilité jusque dans des fonctions d'affichage de l'UI qui
//! ne devraient pas en avoir besoin.
//!
//! Différence assumée avec `WinDebugSession` (le précédent RPC déjà présent
//! dans ce code, pour `winedbg`) : pas de thread dédié + canaux `mpsc`. Cette
//! machinerie existe là-bas parce qu'une `MessageBox` modale peut bloquer
//! `winedbg` indéfiniment, sans qu'aucun budget ne le prévienne. Ici, chaque
//! opération est *déjà* bornée côté agent — `debugger::Debugger::poll` a un
//! budget de 20 ms, `run_until` un budget de pas maximum — donc un simple
//! délai de lecture sur le socket TCP suffit à absorber une VM plantée, et
//! l'appel synchrone direct colle au plus près de la convention d'appel déjà
//! utilisée pour le chemin natif.

use std::cell::RefCell;
use std::io::BufReader;
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

pub use crate::vm_protocol::{
    DbgError, DbgResult, Fault, Flags, FpRegisters, MemRegion, RegionKind, Registers, RunState,
    STACK_WINDOW, Snapshot, SignalName, Watchpoint, WatchHit,
};
use crate::vm_protocol::{self as proto, AGENT_PORT, Request, Response};

/// Délai de lecture par appel. Assez large pour un `RunUntil` au budget
/// maximal (mesuré : ~8 s pour 100 000 pas à 12 000 pas/s en TCG) sans
/// laisser l'UI geler indéfiniment si la VM est vraiment perdue.
const CALL_TIMEOUT: Duration = Duration::from_secs(20);

struct Connection {
    write: TcpStream,
    read: BufReader<TcpStream>,
}

pub struct VmDebugger {
    conn: RefCell<Connection>,
    /// Champs publics, comme sur `Debugger` : les sites d'appel y accèdent
    /// directement (`self.dbg.history`, `self.dbg.state`), jamais par
    /// méthode — l'alignement doit être exact pour que seul l'import change.
    pub history: Vec<Snapshot>,
    pub state: RunState,
    watchpoints: Vec<Watchpoint>,
    load_bias: u64,
    pid: i32,
}

impl VmDebugger {
    /// Ouvre une connexion à l'agent et démarre le binaire préparé par le
    /// dernier [`link_via_vm`] réussi.
    ///
    /// Signature différente de `Debugger::launch(&Path)` — voir le module doc
    /// : sur macOS, le binaire n'a pas de chemin local pertinent pour l'agent
    /// (il est déjà dans l'invité). Les deux sites d'appel (`app/debug_ops.rs`)
    /// portent un petit embranchement `cfg` pour ça — seul écart au principe
    /// « importer suffit ».
    pub fn launch() -> DbgResult<Self> {
        let conn = RefCell::new(connect()?);
        let mut dbg = VmDebugger {
            conn,
            history: Vec::new(),
            state: RunState::Stopped,
            watchpoints: Vec::new(),
            load_bias: 0,
            pid: 0,
        };
        match dbg.call(Request::Launch)? {
            Response::Launch(Ok(r)) => {
                dbg.load_bias = r.load_bias;
                dbg.pid = r.pid;
                dbg.history.push(r.initial);
                dbg.state = RunState::Stopped;
                Ok(dbg)
            }
            Response::Launch(Err(e)) => Err(e),
            _ => Err(DbgError::ConnectionLost("réponse inattendue à Launch".into())),
        }
    }

    fn call(&self, req: Request) -> DbgResult<Response> {
        let mut conn = self.conn.borrow_mut();
        proto::write_message(&mut conn.write, &req).map_err(io_err)?;
        proto::read_message(&mut conn.read).map_err(io_err)
    }

    pub fn step(&mut self) -> DbgResult<()> {
        if self.state != RunState::Stopped {
            return Ok(());
        }
        match self.call(Request::Step)? {
            Response::Stepped(Ok(r)) => self.apply_step(r),
            Response::Stepped(Err(e)) => Err(e),
            _ => Err(DbgError::ConnectionLost("réponse inattendue à Step".into())),
        }
    }

    pub fn poll(&mut self) -> DbgResult<()> {
        if self.state != RunState::Running {
            return Ok(());
        }
        match self.call(Request::Poll)? {
            Response::Stepped(Ok(r)) => self.apply_step(r),
            Response::Stepped(Err(e)) => Err(e),
            _ => Err(DbgError::ConnectionLost("réponse inattendue à Poll".into())),
        }
    }

    fn apply_step(&mut self, r: proto::StepResult) -> DbgResult<()> {
        self.state = r.state;
        if let Some(snap) = r.snapshot {
            self.history.push(snap);
        }
        Ok(())
    }

    /// Voir `Debugger::run_until` : `stop` reste une closure côté appelant,
    /// mais elle ne franchit jamais le fil — c'est `stops` (adresses, avec
    /// leur condition en texte brut, revue côté agent par
    /// `breakpoint::parse`) qui porte la même information sous forme de
    /// données. Les appelants de ce module passent déjà `StopMap`/condition
    /// sous forme de données (voir `app/debug_ops.rs::stops_here`), donc ce
    /// n'est qu'un changement de représentation, pas de logique.
    pub fn run_until_remote(&mut self, budget: usize, stops: Vec<proto::StopSpec>) -> DbgResult<usize> {
        match self.call(Request::RunUntil { budget, stops })? {
            Response::RunUntil(Ok(r)) => {
                self.state = r.state;
                self.history.extend(r.snapshots);
                Ok(r.steps)
            }
            Response::RunUntil(Err(e)) => Err(e),
            _ => Err(DbgError::ConnectionLost("réponse inattendue à RunUntil".into())),
        }
    }

    pub fn read_mem(&self, addr: u64, len: usize) -> DbgResult<Vec<u8>> {
        match self.call(Request::ReadMem { addr, len })? {
            Response::Mem(r) => r,
            _ => Err(DbgError::ConnectionLost("réponse inattendue à ReadMem".into())),
        }
    }

    /// Écrit en mémoire, puis remplace le snapshot de tête par celui que
    /// l'agent renvoie (relu après l'écriture) — même effet que le
    /// `refresh_head` interne de `Debugger::write_mem`, fait ici via le fil
    /// plutôt qu'en mémoire partagée.
    pub fn write_mem(&mut self, addr: u64, bytes: &[u8]) -> DbgResult<()> {
        let bytes = bytes.to_vec();
        match self.call(Request::WriteMem { addr, bytes })? {
            Response::Stepped(Ok(r)) => self.replace_head(r),
            Response::Stepped(Err(e)) => Err(e),
            _ => Err(DbgError::ConnectionLost("réponse inattendue à WriteMem".into())),
        }
    }

    pub fn set_register(&mut self, name: &str, value: u64) -> DbgResult<()> {
        let name = name.to_string();
        match self.call(Request::SetRegister { name, value })? {
            Response::Stepped(Ok(r)) => self.replace_head(r),
            Response::Stepped(Err(e)) => Err(e),
            _ => Err(DbgError::ConnectionLost("réponse inattendue à SetRegister".into())),
        }
    }

    fn replace_head(&mut self, r: proto::StepResult) -> DbgResult<()> {
        self.state = r.state;
        if let Some(snap) = r.snapshot {
            if let Some(last) = self.history.last_mut() {
                *last = snap;
            } else {
                self.history.push(snap);
            }
        }
        Ok(())
    }

    pub fn take_output(&mut self) -> String {
        match self.call(Request::TakeOutput) {
            Ok(Response::Output(s)) => s,
            _ => String::new(),
        }
    }

    pub fn write_stdin(&mut self, line: &str) -> DbgResult<()> {
        let text = line.to_string();
        match self.call(Request::WriteStdin { text })? {
            Response::Unit(r) => r,
            _ => Err(DbgError::ConnectionLost("réponse inattendue à WriteStdin".into())),
        }
    }

    pub fn has_stdin(&self) -> bool {
        true
    }

    pub fn watch(&mut self, addr: u64, len: usize) -> DbgResult<()> {
        match self.call(Request::Watch { addr, len })? {
            Response::Unit(Ok(())) => {
                self.watchpoints.retain(|w| w.addr != addr);
                self.watchpoints.push(Watchpoint { addr, len });
                Ok(())
            }
            Response::Unit(Err(e)) => Err(e),
            _ => Err(DbgError::ConnectionLost("réponse inattendue à Watch".into())),
        }
    }

    pub fn unwatch(&mut self, addr: u64) {
        let _ = self.call(Request::Unwatch { addr });
        self.watchpoints.retain(|w| w.addr != addr);
    }

    pub fn watchpoints(&self) -> &[Watchpoint] {
        &self.watchpoints
    }

    pub fn take_watch_hit(&mut self) -> Option<WatchHit> {
        match self.call(Request::TakeWatchHit) {
            Ok(Response::WatchHit(h)) => h,
            _ => None,
        }
    }

    pub fn fault(&self) -> Option<Fault> {
        match self.state {
            RunState::Faulted(f) => Some(f),
            _ => None,
        }
    }

    pub fn head(&self) -> &Snapshot {
        self.history.last().expect("history non vide")
    }

    pub fn regs(&self) -> &Registers {
        &self.head().regs
    }

    pub fn steps(&self) -> usize {
        self.history.len() - 1
    }

    pub fn is_alive(&self) -> bool {
        matches!(self.state, RunState::Stopped | RunState::Running)
    }

    pub fn is_ready(&self) -> bool {
        self.state == RunState::Stopped
    }

    pub fn is_waiting(&self) -> bool {
        self.state == RunState::Running
    }

    pub fn pid(&self) -> i32 {
        self.pid
    }

    pub fn load_bias(&self) -> u64 {
        self.load_bias
    }

    pub fn mem_regions(&self) -> Vec<MemRegion> {
        match self.call(Request::MemRegions) {
            Ok(Response::MemRegions(r)) => r,
            _ => Vec::new(),
        }
    }

    pub fn heap_range(&self) -> Option<(u64, u64)> {
        match self.call(Request::HeapRange) {
            Ok(Response::HeapRange(r)) => r,
            _ => None,
        }
    }
}

impl Drop for VmDebugger {
    /// Ne jamais bloquer la fermeture de l'IDE sur la VM — même convention
    /// que `Debugger::drop` (Linux, `SIGKILL` local, jamais de réseau) et
    /// `WinDebugSession::drop` (interrompt explicitement un thread bloqué
    /// plutôt que d'attendre). `self.call` normal est borné par
    /// `CALL_TIMEOUT` (20 s d'écriture *et* 20 s de lecture) : bien trop
    /// long pour un quit. Ici on ne lit même pas la réponse — juste une
    /// tentative bonne volonté d'arrêter proprement le programme tracé côté
    /// agent, avec un délai d'écriture volontairement court.
    fn drop(&mut self) {
        let mut conn = self.conn.borrow_mut();
        let _ = conn.write.set_write_timeout(Some(Duration::from_millis(300)));
        let _ = proto::write_message(&mut conn.write, &Request::Stop);
    }
}

/// Connexion directe au port bien connu de l'agent (voir le commentaire sur
/// [`AGENT_PORT`]) — pas de dépendance à [`crate::vm_session::VmSession`],
/// qui ne fait que gérer le cycle de vie du processus QEMU et l'état affiché
/// à l'utilisateur pendant le démarrage. Une VM qui n'a pas fini de démarrer
/// se traduit ici par une connexion refusée, donc une `DbgError` ordinaire —
/// pas un cas à part.
fn connect() -> DbgResult<Connection> {
    let addr = SocketAddr::from(([127, 0, 0, 1], AGENT_PORT));
    let write = TcpStream::connect_timeout(&addr, CALL_TIMEOUT).map_err(io_err)?;
    write.set_read_timeout(Some(CALL_TIMEOUT)).map_err(io_err)?;
    write.set_write_timeout(Some(CALL_TIMEOUT)).map_err(io_err)?;
    let read = write.try_clone().map_err(io_err)?;
    Ok(Connection { write, read: BufReader::new(read) })
}

fn io_err(e: std::io::Error) -> DbgError {
    if e.kind() == std::io::ErrorKind::TimedOut || e.kind() == std::io::ErrorKind::WouldBlock {
        DbgError::Timeout
    } else {
        DbgError::ConnectionLost(e.to_string())
    }
}

/// Lie les objets fournis à l'intérieur de la VM (`ld`, exécuté dans
/// l'invité — voir `assemble.rs`) : connexion ponctuelle, indépendante d'une
/// session `VmDebugger`, puisqu'un lien peut réussir sans qu'on débogue
/// ensuite (juste « Assembler »).
pub fn link_via_vm(objects: Vec<(String, Vec<u8>)>, ld_args: Vec<String>) -> DbgResult<proto::LinkResult> {
    let mut conn = connect()?;
    let objects = objects.into_iter().map(|(name, bytes)| proto::LinkObject { name, bytes }).collect();
    proto::write_message(&mut conn.write, &Request::Link { objects, ld_args }).map_err(io_err)?;
    match proto::read_message(&mut conn.read).map_err(io_err)? {
        Response::Link(r) => r,
        _ => Err(DbgError::ConnectionLost("réponse inattendue à Link".into())),
    }
}
