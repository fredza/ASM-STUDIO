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
}

pub struct Debugger {
    child: Pid,
    pub state: RunState,
    /// Registres à l'instruction courante.
    pub regs: Registers,
    /// Registres à l'instruction précédente (pour la coloration du diff).
    pub prev: Registers,
    /// Nombre d'instructions exécutées depuis le lancement.
    pub steps: u64,
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
                let _ = execv(&cpath, &[cpath.clone()]);
                // execve a échoué : on sort sans dérouler la pile du parent.
                unsafe { libc::_exit(127) };
            }
            ForkResult::Parent { child } => {
                // Premier arrêt : juste après execve, avant la 1re instruction.
                waitpid(child, None).map_err(|e| format!("waitpid initial: {e}"))?;
                let regs = read_regs(child)?;
                Ok(Debugger {
                    child,
                    state: RunState::Stopped,
                    prev: regs.clone(),
                    regs,
                    steps: 0,
                })
            }
        }
    }

    /// Exécute exactement une instruction machine.
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
            _ => {
                self.prev = self.regs.clone();
                self.regs = read_regs(self.child)?;
                self.steps += 1;
            }
        }
        Ok(())
    }

    pub fn flags(&self) -> Flags {
        Flags::from_eflags(self.regs.eflags)
    }

    pub fn prev_flags(&self) -> Flags {
        Flags::from_eflags(self.prev.eflags)
    }

    pub fn is_alive(&self) -> bool {
        self.state == RunState::Stopped
    }

    /// Lit `len` octets à l'adresse `addr` dans l'espace mémoire du tracé,
    /// via `/proc/<pid>/mem`.
    pub fn read_mem(&self, addr: u64, len: usize) -> Result<Vec<u8>, String> {
        use std::fs::File;
        use std::os::unix::fs::FileExt;

        let path = format!("/proc/{}/mem", self.child.as_raw());
        let f = File::open(&path).map_err(|e| format!("open {path}: {e}"))?;
        let mut buf = vec![0u8; len];
        f.read_exact_at(&mut buf, addr)
            .map_err(|e| format!("read @0x{addr:X}: {e}"))?;
        Ok(buf)
    }

    /// Lit `count` mots de 64 bits à partir de `addr` (little-endian).
    /// Renvoie une valeur nulle pour les mots illisibles (au-delà de la pile mappée).
    pub fn read_qwords(&self, addr: u64, count: usize) -> Vec<u64> {
        (0..count)
            .map(|i| {
                let a = addr + (i as u64) * 8;
                self.read_mem(a, 8)
                    .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
                    .unwrap_or(0)
            })
            .collect()
    }
}

impl Drop for Debugger {
    fn drop(&mut self) {
        // Évite les zombies si on relance ou on ferme pendant l'exécution.
        if self.state == RunState::Stopped {
            let _ = ptrace::kill(self.child);
            let _ = waitpid(self.child, None);
        }
    }
}

fn read_regs(pid: Pid) -> Result<Registers, String> {
    let raw = ptrace::getregs(pid).map_err(|e| format!("getregs: {e}"))?;
    Ok(Registers::from_raw(&raw))
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
        let out = assemble::assemble(Path::new("examples/test.asm"), Path::new("build"))
            .expect("assemblage");
        let mut dbg = Debugger::launch(&out.binary).expect("launch");

        // mov rax,5 ; push rax ; mov rbx,8 ; cmp rax,rbx  => 4 steps jusqu'au cmp inclus.
        for _ in 0..4 {
            dbg.step().expect("step");
            assert!(dbg.is_alive(), "le programme ne devrait pas être terminé");
        }
        let f = dbg.flags();
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
}
