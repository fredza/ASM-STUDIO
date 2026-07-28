use crate::assemble;
use crate::debugger::{Debugger, RunState};
use crate::disasm;
use crate::i18n;
use crate::srcmap;
use crate::syscall;

use super::{App, SyscallLog};

impl App {
    /// Enregistre puis assemble (nasm) et lie (ld) le programme de l'utilisateur.
    pub(super) fn build(&mut self) {
        self.save_source();
        // Artefacts dans un sous-dossier `build/` À CÔTÉ du fichier source
        // (et non plus dans un `build/` global relatif au répertoire courant).
        self.out_dir = super::abs_dir_of(&self.src_path).join("build");
        let includes = self.include_dirs();
        match assemble::assemble_with_includes(&self.src_path, &self.out_dir, &includes) {
            Ok(out) => {
                self.log(&out.log);
                // Mapping adresse → ligne source (suivi dans l'éditeur).
                self.src_map = disasm::section_address(&out.binary, ".text")
                    .map(|base| srcmap::parse(&out.listing, base))
                    .unwrap_or_default();
                self.binary = Some(out.binary);
                self.status = "Build OK".to_string();
            }
            Err(e) => {
                self.log(&e);
                self.binary = None;
                self.status = i18n::tr(self.lang, "Échec build", "Build failed").to_string();
            }
        }
    }

    pub(super) fn launch(&mut self) {
        self.build();
        let Some(bin) = self.binary.clone() else {
            return;
        };
        match disasm::disassemble_text(&bin) {
            Ok(insns) => self.disasm = insns,
            Err(e) => self.log(&e),
        }
        self.mem_addr = disasm::section_address(&bin, ".data")
            .or_else(|| disasm::section_address(&bin, ".text"))
            .unwrap_or(0);
        self.mem_input = format!("0x{:X}", self.mem_addr);
        self.selected = None;
        self.syscalls.clear();
        self.call_stack.clear();
        self.view_index = 0;
        self.dbg = None;
        match Debugger::launch(&bin) {
            Ok(dbg) => {
                self.status = format!("{} 0x{:X}", i18n::tr(self.lang, "Lancé — RIP @", "Started — RIP @"), dbg.regs().rip);
                self.log("Running...");
                self.dbg = Some(dbg);
            }
            Err(e) => {
                self.log(&e);
                self.status = i18n::tr(self.lang, "Échec lancement", "Launch failed").to_string();
            }
        }
    }

    pub(super) fn stop(&mut self) {
        self.dbg = None;
        self.status = i18n::tr(self.lang, "Arrêté", "Stopped").to_string();
    }

    pub(super) fn step(&mut self) {
        if !self.can_step() {
            return;
        }
        // Appel système sur le point de s'exécuter (RIP) : pour le journal console.
        let pending = self.dbg.as_ref().and_then(|d| {
            let insn = self.disasm.iter().find(|i| i.address == d.regs().rip)?;
            (insn.mnemonic == "syscall").then(|| (syscall::format_call(d.regs()), d.regs().rax))
        });

        if let Some(d) = self.dbg.as_mut()
            && let Err(e) = d.step()
        {
            self.log(&e);
            return;
        }
        if let Some(d) = self.dbg.as_ref() {
            self.view_index = d.history.len() - 1;
        }
        self.pending_flash = true; // déclenche l'animation « CPU vivant »

        // Reconstruit pile d'appels + journal syscalls depuis l'historique complet
        // (source unique, cohérente après Step ET après « Reprendre ici »).
        self.rebuild_trace();

        // Journalise l'appel système dans la console (une fois, à son exécution).
        if let Some((call, num)) = pending {
            if syscall::is_exit(num) {
                self.log(&call);
            } else if let Some(d) = self.dbg.as_ref() {
                self.log(&format!("{call} = {}", d.regs().rax as i64));
            }
        }
        match self.dbg.as_ref().map(|d| d.state) {
            Some(RunState::Stopped) => {
                let d = self.dbg.as_ref().unwrap();
                self.status = format!("{} {} — RIP @ 0x{:X}", i18n::tr(self.lang, "Étape", "Step"), d.steps(), d.regs().rip);
            }
            Some(RunState::Exited(code)) => self.status = format!("{} (exit {code})", i18n::tr(self.lang, "Terminé", "Terminated")),
            Some(RunState::Signaled) => self.status = i18n::tr(self.lang, "Terminé (signal)", "Terminated (signal)").to_string(),
            None => {}
        }
    }

    pub(super) fn resume_here(&mut self) {
        let Some(bin) = self.binary.clone() else { return };
        let target = self.view_index;
        match Debugger::launch(&bin) {
            Ok(mut d) => {
                for _ in 0..target {
                    if !d.is_alive() {
                        break;
                    }
                    let _ = d.step();
                }
                self.view_index = d.history.len() - 1;
                self.status = format!("{} {}", i18n::tr(self.lang, "Repris à l'étape", "Resumed at step"), self.view_index);
                self.selected = None;
                self.dbg = Some(d);
                self.rebuild_trace(); // resynchronise call stack + syscalls
            }
            Err(e) => self.log(&e),
        }
    }

    /// Reconstruit `call_stack` et `syscalls` depuis l'historique complet du
    /// debugger : source unique de vérité pour ces deux panneaux. Chaque
    /// transition `history[i] → history[i+1]` correspond à l'exécution de
    /// l'instruction à `history[i].rip`.
    pub(super) fn rebuild_trace(&mut self) {
        let mut call_stack = Vec::new();
        let mut syscalls = Vec::new();
        // Petit utilitaire local : décompose "name(args)" en (name, args).
        let log_syscall = |list: &mut Vec<SyscallLog>, regs: &crate::debugger::Registers, ret: Option<i64>| {
            let num = regs.rax;
            let call = syscall::format_call(regs);
            let args = call
                .find('(')
                .map(|p| call[p + 1..].trim_end_matches(')').to_string())
                .unwrap_or_default();
            list.push(SyscallLog { name: syscall::name(num).to_string(), args, number: num, ret });
        };
        if let Some(d) = self.dbg.as_ref() {
            let hist = &d.history;
            for i in 0..hist.len().saturating_sub(1) {
                let cur = &hist[i].regs;
                let next = &hist[i + 1].regs;
                let Some(insn) = self.disasm.iter().find(|x| x.address == cur.rip) else {
                    continue;
                };
                match insn.mnemonic.as_str() {
                    "call" => call_stack.push(next.rip),
                    "ret" => {
                        call_stack.pop();
                    }
                    "syscall" => {
                        let ret = (!syscall::is_exit(cur.rax)).then_some(next.rax as i64);
                        log_syscall(&mut syscalls, cur, ret);
                    }
                    _ => {}
                }
            }
            // Cas de l'appel qui termine le processus (exit) : il reste en tête de
            // l'historique sans successeur (aucun snapshot après la mort du process).
            if !d.is_alive()
                && let Some(head) = hist.last()
                && let Some(insn) = self.disasm.iter().find(|x| x.address == head.regs.rip)
                && insn.mnemonic == "syscall"
            {
                log_syscall(&mut syscalls, &head.regs, None);
            }
        }
        self.call_stack = call_stack;
        self.syscalls = syscalls;
    }

    pub(super) fn next_addr(&self) -> Option<u64> {
        let rip = self.view_rip()?;
        let idx = self.disasm.iter().position(|i| i.address == rip)?;
        self.disasm.get(idx + 1).map(|i| i.address)
    }
}
