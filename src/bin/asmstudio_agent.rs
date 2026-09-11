//! Agent qui tourne *dans* la VM Linux du backend macOS. Compilé uniquement
//! pour la cible croisée `x86_64-unknown-linux-musl` (voir Cargo.toml,
//! feature `vm-agent`) — jamais construit par un `cargo build` ordinaire.
//!
//! Ne réimplémente rien : il tient un vrai `asm_studio::debugger::Debugger`
//! (le même moteur ptrace que le binaire Linux natif) et se contente de
//! traduire entre lui et le format d'échange de
//! [`asm_studio::vm_protocol`]. La seule logique qui lui est propre est
//! l'exécution de `ld` (l'autre moitié de ce que `assemble.rs` fait
//! localement sur Linux) et la traduction des types.

use std::io::BufReader;
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use asm_studio::breakpoint;
use asm_studio::debugger::{self, Debugger};
use asm_studio::i18n::Lang;
use asm_studio::vm_protocol::{self as proto, Request, Response};

const AGENT_PORT: u16 = 7878;
const BUILD_DIR: &str = "/root/build";
const CURRENT_BINARY: &str = "/root/current";

fn main() {
    let listener = TcpListener::bind(("0.0.0.0", AGENT_PORT)).expect("liaison du port de l'agent");
    std::fs::create_dir_all(BUILD_DIR).ok();
    loop {
        match listener.accept() {
            Ok((stream, _)) => handle_connection(stream),
            Err(_) => continue,
        }
    }
}

/// Une connexion = une session : le `Debugger` vit tant que l'hôte reste
/// connecté (ce qui correspond exactement à une session de débogage côté
/// UI). `Link` peut arriver sur sa propre connexion, avant qu'aucune session
/// n'existe — c'est pour ça que le binaire lié atterrit à un chemin fixe
/// (`CURRENT_BINARY`) plutôt que de rester en mémoire de connexion.
fn handle_connection(stream: TcpStream) {
    let mut write = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut read = BufReader::new(stream);
    let mut dbg: Option<Debugger> = None;
    loop {
        let req: Request = match proto::read_message(&mut read) {
            Ok(r) => r,
            Err(_) => return,
        };
        let resp = handle_request(req, &mut dbg);
        if proto::write_message(&mut write, &resp).is_err() {
            return;
        }
    }
}

fn handle_request(req: Request, dbg: &mut Option<Debugger>) -> Response {
    match req {
        Request::Ping => Response::Pong,
        Request::Link { objects, ld_args } => Response::Link(do_link(objects, ld_args)),
        Request::Launch => Response::Launch(do_launch(dbg)),
        Request::Step => Response::Stepped(with_dbg(dbg, |d| {
            let before = d.history.len();
            d.step()?;
            Ok(step_result(d, before))
        })),
        Request::Poll => Response::Stepped(with_dbg(dbg, |d| {
            let before = d.history.len();
            d.poll()?;
            Ok(step_result(d, before))
        })),
        Request::RunUntil { budget, stops } => Response::RunUntil(do_run_until(dbg, budget, stops)),
        Request::ReadMem { addr, len } => Response::Mem(with_dbg(dbg, |d| d.read_mem(addr, len))),
        Request::WriteMem { addr, bytes } => Response::Stepped(with_dbg(dbg, |d| {
            d.write_mem(addr, &bytes)?;
            Ok(head_result(d))
        })),
        Request::SetRegister { name, value } => Response::Stepped(with_dbg(dbg, |d| {
            d.set_register(&name, value)?;
            Ok(head_result(d))
        })),
        Request::TakeOutput => Response::Output(dbg.as_mut().map(|d| d.take_output()).unwrap_or_default()),
        Request::WriteStdin { text } => Response::Unit(with_dbg(dbg, |d| d.write_stdin(&text))),
        Request::Watch { addr, len } => Response::Unit(with_dbg(dbg, |d| d.watch(addr, len))),
        Request::Unwatch { addr } => {
            if let Some(d) = dbg.as_mut() {
                d.unwatch(addr);
            }
            Response::Unit(Ok(()))
        }
        Request::Watchpoints => Response::Watchpoints(
            dbg.as_ref()
                .map(|d| d.watchpoints().iter().map(|w| proto::Watchpoint { addr: w.addr, len: w.len }).collect())
                .unwrap_or_default(),
        ),
        Request::TakeWatchHit => {
            Response::WatchHit(dbg.as_mut().and_then(|d| d.take_watch_hit()).map(convert_watch_hit))
        }
        Request::MemRegions => Response::MemRegions(
            dbg.as_ref().map(|d| d.mem_regions().iter().map(convert_region).collect()).unwrap_or_default(),
        ),
        Request::HeapRange => Response::HeapRange(dbg.as_ref().and_then(|d| d.heap_range())),
        Request::Fault => Response::Fault(dbg.as_ref().and_then(|d| d.fault()).map(convert_fault)),
        Request::LoadBias => Response::LoadBias(dbg.as_ref().map(|d| d.load_bias()).unwrap_or(0)),
        Request::Stop => {
            *dbg = None;
            Response::Unit(Ok(()))
        }
    }
}

fn with_dbg<T>(dbg: &mut Option<Debugger>, f: impl FnOnce(&mut Debugger) -> debugger::DbgResult<T>) -> proto::DbgResult<T> {
    match dbg.as_mut() {
        Some(d) => f(d).map_err(convert_err),
        None => Err(no_session()),
    }
}

fn no_session() -> proto::DbgError {
    proto::DbgError::System { what: "session".into(), err: "aucune session active".into() }
}

fn do_launch(dbg: &mut Option<Debugger>) -> proto::DbgResult<proto::LaunchResult> {
    let real = Debugger::launch(Path::new(CURRENT_BINARY)).map_err(convert_err)?;
    let load_bias = real.load_bias();
    let pid = real.pid();
    let initial = convert_snapshot(real.head());
    *dbg = Some(real);
    Ok(proto::LaunchResult { load_bias, initial, pid })
}

fn do_run_until(dbg: &mut Option<Debugger>, budget: usize, stops: Vec<proto::StopSpec>) -> proto::DbgResult<proto::RunUntilResult> {
    let parsed: Vec<(u64, Option<breakpoint::Condition>)> = stops
        .into_iter()
        .map(|s| {
            let cond = s.condition_text.and_then(|t| breakpoint::parse(&t, Lang::Fr).ok().flatten());
            (s.addr, cond)
        })
        .collect();
    with_dbg(dbg, |d| {
        let before = d.history.len();
        let steps = d.run_until(budget, |regs| {
            parsed.iter().any(|(addr, cond)| {
                if regs.rip != *addr {
                    return false;
                }
                match cond {
                    None => true,
                    Some(c) => c.eval(regs, &debugger::Flags::from_eflags(regs.eflags)),
                }
            })
        })?;
        let snapshots = d.history[before..].iter().map(convert_snapshot).collect();
        Ok(proto::RunUntilResult { steps, state: convert_state(d.state), snapshots })
    })
}

fn step_result(d: &Debugger, before_len: usize) -> proto::StepResult {
    let snapshot = if d.history.len() > before_len { Some(convert_snapshot(d.head())) } else { None };
    proto::StepResult { state: convert_state(d.state), snapshot }
}

fn head_result(d: &Debugger) -> proto::StepResult {
    proto::StepResult { state: convert_state(d.state), snapshot: Some(convert_snapshot(d.head())) }
}

// ---------------------------------------------------------------------
// Lien : l'autre moitié de ce qu'`assemble.rs` fait localement sur Linux
// (voir `assemble_elf`/`assemble_project`), exécutée ici parce que c'est la
// seule chose que macOS ne sait pas faire (pas de linker ELF natif).
// ---------------------------------------------------------------------

fn do_link(objects: Vec<proto::LinkObject>, ld_args: Vec<String>) -> proto::DbgResult<proto::LinkResult> {
    std::fs::create_dir_all(BUILD_DIR).map_err(|e| io_dbg_err("mkdir", e))?;
    let mut obj_paths = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        let path = format!("{BUILD_DIR}/{i:03}-{}", sanitize(&obj.name));
        std::fs::write(&path, &obj.bytes).map_err(|e| io_dbg_err("write objet", e))?;
        obj_paths.push(path);
    }
    let out_path = format!("{BUILD_DIR}/out");
    let output = Command::new("ld")
        .args(&ld_args)
        .arg("-o")
        .arg(&out_path)
        .args(&obj_paths)
        .output()
        .map_err(|e| io_dbg_err("ld", e))?;
    let mut log = String::from_utf8_lossy(&output.stdout).into_owned();
    log.push_str(&String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        return Err(proto::DbgError::System { what: "ld".into(), err: log });
    }
    let binary = std::fs::read(&out_path).map_err(|e| io_dbg_err("lecture du binaire lié", e))?;
    std::fs::copy(&out_path, CURRENT_BINARY).map_err(|e| io_dbg_err("copie vers le chemin d'exécution", e))?;
    let mut perms = std::fs::metadata(CURRENT_BINARY).map_err(|e| io_dbg_err("permissions", e))?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(CURRENT_BINARY, perms).map_err(|e| io_dbg_err("chmod", e))?;
    Ok(proto::LinkResult { log, binary })
}

fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect()
}

fn io_dbg_err(what: &str, e: std::io::Error) -> proto::DbgError {
    proto::DbgError::System { what: what.to_string(), err: e.to_string() }
}

// ---------------------------------------------------------------------
// Traduction des types réels de `debugger` vers le format d'échange.
// ---------------------------------------------------------------------

fn convert_err(e: debugger::DbgError) -> proto::DbgError {
    match e {
        debugger::DbgError::BadPath => proto::DbgError::BadPath,
        debugger::DbgError::PtraceDenied => proto::DbgError::PtraceDenied,
        debugger::DbgError::NoStart => proto::DbgError::NoStart,
        debugger::DbgError::ExitedBeforeStart(c) => proto::DbgError::ExitedBeforeStart(c),
        debugger::DbgError::UnexpectedInitialState(s) => proto::DbgError::UnexpectedInitialState(s),
        debugger::DbgError::HistoryFull(n) => proto::DbgError::HistoryFull(n),
        debugger::DbgError::NoStdin => proto::DbgError::NoStdin,
        debugger::DbgError::NotStopped => proto::DbgError::NotStopped,
        debugger::DbgError::UnknownRegister(s) => proto::DbgError::UnknownRegister(s),
        debugger::DbgError::System { what, err } => proto::DbgError::System { what, err },
    }
}

fn convert_regs(r: &debugger::Registers) -> proto::Registers {
    proto::Registers {
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

fn convert_fp(fp: &debugger::FpRegisters) -> proto::FpRegisters {
    proto::FpRegisters { xmm: fp.xmm, st: fp.st, fcw: fp.fcw, fsw: fp.fsw, ftw: fp.ftw, mxcsr: fp.mxcsr }
}

fn convert_snapshot(s: &debugger::Snapshot) -> proto::Snapshot {
    proto::Snapshot { regs: convert_regs(&s.regs), stack: s.stack, fp: s.fp.as_deref().map(convert_fp) }
}

fn convert_state(s: debugger::RunState) -> proto::RunState {
    match s {
        debugger::RunState::Stopped => proto::RunState::Stopped,
        debugger::RunState::Running => proto::RunState::Running,
        debugger::RunState::Exited(c) => proto::RunState::Exited(c),
        debugger::RunState::Signaled => proto::RunState::Signaled,
        debugger::RunState::Faulted(f) => proto::RunState::Faulted(convert_fault(f)),
    }
}

fn convert_fault(f: debugger::Fault) -> proto::Fault {
    use nix::sys::signal::Signal::*;
    let signal = match f.signal {
        SIGSEGV => proto::SignalName::SIGSEGV,
        SIGFPE => proto::SignalName::SIGFPE,
        SIGILL => proto::SignalName::SIGILL,
        SIGBUS => proto::SignalName::SIGBUS,
        _ => proto::SignalName::Other,
    };
    proto::Fault { signal, addr: f.addr, rip: f.rip }
}

fn convert_region(r: &debugger::MemRegion) -> proto::MemRegion {
    let kind = match r.kind {
        debugger::RegionKind::Code => proto::RegionKind::Code,
        debugger::RegionKind::Data => proto::RegionKind::Data,
        debugger::RegionKind::Rodata => proto::RegionKind::Rodata,
        debugger::RegionKind::Heap => proto::RegionKind::Heap,
        debugger::RegionKind::Stack => proto::RegionKind::Stack,
    };
    proto::MemRegion { start: r.start, end: r.end, kind, perms: r.perms.clone() }
}

fn convert_watch_hit(h: debugger::WatchHit) -> proto::WatchHit {
    proto::WatchHit { addr: h.addr, before: h.before, after: h.after, step: h.step }
}
