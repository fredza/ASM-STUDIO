//! Couche d'exécution pas-à-pas via `ptrace` (Linux/x86-64).
//!
//! On `fork()` un enfant qui fait `PTRACE_TRACEME` puis `execve` le binaire.
//! Le parent (le thread qui a forké — ici le thread principal de l'UI) pilote
//! l'enfant avec `PTRACE_SINGLESTEP` et lit les registres via `PTRACE_GETREGS`.

use std::ffi::CString;
use std::path::Path;

use nix::sys::ptrace;
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, Pid, execv, fork};

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

/// État complet du CPU à une étape donnée, conservé pour la timeline (M5).
#[derive(Clone)]
pub struct Snapshot {
    pub regs: Registers,
    /// Fenêtre de pile (`STACK_WINDOW` mots de 64 bits) à partir de RSP.
    pub stack: Vec<u64>,
}

pub struct Debugger {
    child: Pid,
    pub state: RunState,
    /// Historique des états, un par instruction exécutée (index 0 = état initial).
    /// Permet le scrubbing de la timeline sans ré-exécuter (record-and-replay).
    pub history: Vec<Snapshot>,
}

impl Debugger {
    /// Lance le binaire et s'arrête juste avant sa première instruction.
    pub fn launch(binary: &Path) -> Result<Self, String> {
        let path = binary
            .to_str()
            .ok_or_else(|| "chemin binaire non-UTF8".to_string())?;
        let cpath = CString::new(path).map_err(|e| e.to_string())?;

        match unsafe { fork() }.map_err(|e| format!("fork: {e}"))? {
            ForkResult::Child => {
                // Dans l'enfant : uniquement des appels async-signal-safe avant execve.
                let _ = ptrace::traceme();
                let _ = execv(&cpath, std::slice::from_ref(&cpath));
                // execve a échoué : on sort sans dérouler la pile du parent.
                unsafe { libc::_exit(127) };
            }
            ForkResult::Parent { child } => {
                // Premier arrêt attendu : SIGTRAP juste après execve, avant la
                // 1re instruction. Si l'enfant est déjà mort, execve a échoué.
                match waitpid(child, None).map_err(|e| format!("waitpid initial: {e}"))? {
                    WaitStatus::Stopped(_, _) => {}
                    WaitStatus::Exited(_, code) => {
                        return Err(format!(
                            "le programme n'a pas démarré (execve a échoué, code {code})"
                        ));
                    }
                    other => return Err(format!("état initial inattendu: {other:?}")),
                }
                let regs = read_regs(child)?;
                let snap = snapshot_of(child, &regs);
                Ok(Debugger {
                    child,
                    state: RunState::Stopped,
                    history: vec![snap],
                })
            }
        }
    }

    /// Exécute exactement une instruction machine et enregistre un snapshot.
    pub fn step(&mut self) -> Result<(), String> {
        if self.state != RunState::Stopped {
            return Ok(());
        }
        ptrace::step(self.child, None).map_err(|e| format!("singlestep: {e}"))?;
        match waitpid(self.child, None).map_err(|e| format!("waitpid: {e}"))? {
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
                let snap = snapshot_of(self.child, &regs);
                self.history.push(snap);
                self.state = RunState::Faulted(Fault {
                    signal: sig,
                    addr: fault_addr(self.child),
                    rip: regs.rip,
                });
            }
            _ => {
                let regs = read_regs(self.child)?;
                let snap = snapshot_of(self.child, &regs);
                self.history.push(snap);
            }
        }
        Ok(())
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

    pub fn is_alive(&self) -> bool {
        self.state == RunState::Stopped
    }

    /// PID du processus tracé.
    pub fn pid(&self) -> i32 {
        self.child.as_raw()
    }

    /// Lit `len` octets à l'adresse `addr` dans l'espace mémoire du tracé (état
    /// courant vivant), via `/proc/<pid>/mem`.
    pub fn read_mem(&self, addr: u64, len: usize) -> Result<Vec<u8>, String> {
        read_mem_pid(self.child, addr, len)
    }

    /// Modifie un registre (processus arrêté requis), puis met à jour le
    /// snapshot courant. Utilisé par le « laboratoire mémoire ».
    pub fn set_register(&mut self, name: &str, value: u64) -> Result<(), String> {
        if self.state != RunState::Stopped {
            return Err("le processus n'est pas arrêté".to_string());
        }
        let mut r = ptrace::getregs(self.child).map_err(|e| format!("getregs: {e}"))?;
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
            _ => return Err(format!("registre inconnu: {name}")),
        }
        ptrace::setregs(self.child, r).map_err(|e| format!("setregs: {e}"))?;
        self.refresh_head()
    }

    /// Écrit des octets en mémoire (processus arrêté), puis met à jour le snapshot.
    pub fn write_mem(&mut self, addr: u64, bytes: &[u8]) -> Result<(), String> {
        if self.state != RunState::Stopped {
            return Err("le processus n'est pas arrêté".to_string());
        }
        write_mem_pid(self.child, addr, bytes)?;
        self.refresh_head()
    }

    /// Recharge le snapshot de tête depuis le processus (après une édition).
    fn refresh_head(&mut self) -> Result<(), String> {
        let regs = read_regs(self.child)?;
        let snap = snapshot_of(self.child, &regs);
        if let Some(h) = self.history.last_mut() {
            *h = snap;
        }
        Ok(())
    }

    /// Toutes les régions mappées du processus (`/proc/<pid>/maps`), pour la vue
    /// mémoire unifiée. Les régions sans intérêt pédagogique (bibliothèques
    /// partagées, vvar/vdso) sont écartées afin de garder un schéma lisible.
    pub fn mem_regions(&self) -> Vec<MemRegion> {
        let Ok(maps) = std::fs::read_to_string(format!("/proc/{}/maps", self.child.as_raw())) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for line in maps.lines() {
            let mut it = line.split_whitespace();
            let Some(range) = it.next() else { continue };
            let Some(perms) = it.next() else { continue };
            let Some((s, e)) = range.split_once('-') else { continue };
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
        if self.state == RunState::Stopped {
            let _ = nix::sys::signal::kill(self.child, nix::sys::signal::Signal::SIGKILL);
            let _ = waitpid(self.child, None);
        }
    }
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

fn read_regs(pid: Pid) -> Result<Registers, String> {
    let raw = ptrace::getregs(pid).map_err(|e| format!("getregs: {e}"))?;
    Ok(Registers::from_raw(&raw))
}

/// Lit `len` octets à `addr` dans l'espace mémoire du processus `pid`.
fn read_mem_pid(pid: Pid, addr: u64, len: usize) -> Result<Vec<u8>, String> {
    use std::fs::File;
    use std::os::unix::fs::FileExt;

    let path = format!("/proc/{}/mem", pid.as_raw());
    let f = File::open(&path).map_err(|e| format!("open {path}: {e}"))?;
    let mut buf = vec![0u8; len];
    f.read_exact_at(&mut buf, addr)
        .map_err(|e| format!("read @0x{addr:X}: {e}"))?;
    Ok(buf)
}

/// Écrit `bytes` à `addr` dans l'espace mémoire du processus `pid`.
fn write_mem_pid(pid: Pid, addr: u64, bytes: &[u8]) -> Result<(), String> {
    use std::fs::OpenOptions;
    use std::os::unix::fs::FileExt;

    let path = format!("/proc/{}/mem", pid.as_raw());
    let f = OpenOptions::new()
        .write(true)
        .open(&path)
        .map_err(|e| format!("open {path}: {e}"))?;
    f.write_all_at(bytes, addr)
        .map_err(|e| format!("write @0x{addr:X}: {e}"))?;
    Ok(())
}

/// Lit `count` mots de 64 bits à partir de `addr` (little-endian).
/// Renvoie 0 pour les mots illisibles (au-delà de la pile mappée).
fn read_qwords_pid(pid: Pid, addr: u64, count: usize) -> Vec<u64> {
    (0..count)
        .map(|i| {
            let a = addr.wrapping_add((i as u64) * 8);
            read_mem_pid(pid, a, 8)
                .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
                .unwrap_or(0)
        })
        .collect()
}

/// Capture un snapshot (registres + fenêtre de pile) de l'état courant.
fn snapshot_of(pid: Pid, regs: &Registers) -> Snapshot {
    Snapshot {
        regs: regs.clone(),
        stack: read_qwords_pid(pid, regs.rsp, STACK_WINDOW),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble;
    use std::path::Path;

    /// Assemble l'exemple, le lance sous ptrace, le fait avancer jusqu'à la fin.
    /// Vérifie qu'après `cmp rax, rbx` (5 vs 8) les flags sont cohérents :
    /// résultat négatif => ZF=0, SF=1, CF=1 (emprunt), OF=0.
    #[test]
    fn step_through_example_sets_flags() {
        // Dossier dédié : évite toute collision avec les autres tests parallèles.
        let out = assemble::assemble_with_includes(Path::new("examples/test.asm"), Path::new("build/test-dbg"), &[])
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
        let out = assemble::assemble_with_includes(Path::new("examples/test.asm"), Path::new("build/test-hist"), &[])
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
        let out = assemble::assemble_with_includes(Path::new("examples/heap.asm"), Path::new("build/test-heap"), &[])
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
        assert!(regions.windows(2).all(|w| w[0].start <= w[1].start), "régions triées");
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
        dbg.write_mem(rsp, &[0x11, 0x22, 0x33, 0x44]).expect("write mem");
        assert_eq!(
            dbg.read_mem(rsp, 4).expect("read mem"),
            vec![0x11, 0x22, 0x33, 0x44],
            "la mémoire écrite doit être relue à l'identique"
        );
    }
}
