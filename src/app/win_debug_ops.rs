//! Commandes de la fenêtre de pas-à-pas Windows (voir `ui_win_debug`).
//!
//! Volontairement séparé de [`super::debug_ops`] : ce chemin pilote
//! [`crate::win_debugger::WinDebugger`] plutôt que le débogueur natif
//! `ptrace`, avec une API beaucoup plus étroite (pas d'historique de
//! snapshots, pas de sortie de programme récupérable — voir la doc de
//! module de `crate::win_debugger`). Mélanger les deux dans `debug_ops`
//! aurait fait porter aux tests et aux lecteurs de ce fichier des
//! particularités qui ne concernent qu'une cible parmi les trois.
//!
//! `self.dbg` (natif) et `self.win_dbg` (ici) ne sont jamais actifs en même
//! temps : changer de cible (`set_target`) appelle déjà `stop()`, qui
//! relâche les deux.

use crate::i18n;
use crate::win_debugger::{WinDebugger, WinRunState};

use super::App;
use super::debug_ops::stops_here;

/// Budget de reprises par appel à « Continuer ». Chaque itération est un
/// aller-retour réseau localhost (RSP) vers `winedbg`, pas un `ptrace` local :
/// un budget bien plus modeste que celui du débogueur natif (`RUN_BUDGET`,
/// 100 000) suffit à éviter un blocage perceptible de l'interface — elle-même
/// figée tant que `winedbg` n'a pas répondu, jusqu'à
/// [`crate::win_debugger::WinDebugger`]'s propre délai interne dans le pire cas.
const WIN_RUN_BUDGET: usize = 2_000;

impl App {
    /// Wine et `winedbg` sont-ils utilisables pour ce pas-à-pas ?
    pub(super) fn win_debug_available(&self) -> bool {
        WinDebugger::available()
    }

    /// Vrai si « Suivant »/« Continuer » ont un effet sur la cible courante,
    /// natif ou expérimental confondus — c'est ce qui décide si les boutons
    /// de la barre d'outils principale sont cliquables.
    pub(super) fn can_step_any(&self) -> bool {
        self.can_step()
            || (self.target.is_windows()
                && self.win_debug_available()
                && self.win_dbg.as_ref().is_none_or(|d| d.is_alive()))
    }

    /// Point d'entrée unique de « Suivant » (bouton, F10, menu Exécution),
    /// quelle que soit la cible : natif pour Linux, session Wine
    /// expérimentale démarrée à la volée pour Windows (et sa fenêtre dédiée
    /// amenée au premier plan), message d'indisponibilité sinon.
    pub(super) fn step_any(&mut self) {
        if self.target.is_runnable() {
            self.step();
            return;
        }
        if !self.win_debug_available() {
            self.step(); // message « indisponible » existant (ensure_debuggable_target)
            return;
        }
        self.show_win_debug = true;
        if self.win_dbg.is_none() {
            self.win_debug_start();
        } else {
            self.win_debug_step();
        }
    }

    /// Pendant équivalent de [`Self::step_any`] pour « Continuer ».
    pub(super) fn cont_any(&mut self) {
        if self.target.is_runnable() {
            self.cont();
            return;
        }
        if !self.win_debug_available() {
            self.cont();
            return;
        }
        self.show_win_debug = true;
        if self.win_dbg.is_none() {
            self.win_debug_start();
        }
        self.win_debug_cont();
    }

    /// Démarre une session : assemble si besoin, lance `winedbg --gdb`, et
    /// pose un point d'arrêt matériel sur chaque ligne déjà marquée par
    /// l'élève. Sans effet si une session tourne déjà, ou si la cible n'est
    /// pas Windows.
    pub(super) fn win_debug_start(&mut self) {
        if self.win_dbg.is_some() || !self.target.is_windows() {
            return;
        }
        self.build();
        let Some(bin) = self.binary.clone() else { return };
        // Calculées avant le lancement : `stop_addresses` lit `self`, et ses
        // adresses n'ont besoin d'aucun débogueur pour exister — seulement du
        // binaire tout juste réassemblé, dont `self.src_map` est déjà à jour
        // (posé par `build()`).
        let stops = self.stop_addresses(None);
        let lang = self.lang;
        match WinDebugger::launch(&bin) {
            Ok(mut dbg) => {
                for addr in stops.keys() {
                    let _ = dbg.set_breakpoint(*addr);
                }
                self.status = format!(
                    "{} 0x{:X}",
                    i18n::tr3(
                        lang,
                        "Pas-à-pas Windows démarré — RIP @",
                        "Windows step debugging started — RIP @",
                        "Paso a paso Windows iniciado — RIP @",
                    ),
                    dbg.regs().rip
                );
                self.win_dbg = Some(dbg);
            }
            Err(e) => {
                let msg = e.message(lang);
                self.log(&msg);
                self.status = i18n::tr3(
                    lang,
                    "Échec du lancement (Wine)",
                    "Launch failed (Wine)",
                    "Fallo al iniciar (Wine)",
                )
                .to_string();
            }
        }
    }

    /// Une instruction machine.
    pub(super) fn win_debug_step(&mut self) {
        let lang = self.lang;
        if let Some(d) = self.win_dbg.as_mut()
            && let Err(e) = d.step()
        {
            self.log(&e.message(lang));
        }
        self.finish_win_debug_step();
    }

    /// Exécute jusqu'au prochain point d'arrêt de l'élève (condition
    /// comprise) ou la fin du programme, bornée par [`WIN_RUN_BUDGET`]
    /// reprises. Repose les points d'arrêt à chaque appel : un point ajouté
    /// depuis le dernier passage n'a encore aucun `0xCC` réel en mémoire
    /// (`set_breakpoint` est sans effet sur une adresse déjà armée, donc
    /// reposer celles qui le sont déjà ne coûte rien).
    pub(super) fn win_debug_cont(&mut self) {
        if self.win_dbg.is_none() {
            return;
        }
        let stops = self.stop_addresses(None);
        let lang = self.lang;
        let mut used = 0usize;
        loop {
            let Some(d) = self.win_dbg.as_mut() else { break };
            for addr in stops.keys() {
                let _ = d.set_breakpoint(*addr);
            }
            if let Err(e) = d.cont() {
                self.log(&e.message(lang));
                break;
            }
            used += 1;
            if !d.is_alive() || stops_here(&stops, d.regs()) || used >= WIN_RUN_BUDGET {
                break;
            }
        }
        if used >= WIN_RUN_BUDGET && self.win_dbg.as_ref().is_some_and(|d| d.is_alive()) {
            self.status = format!(
                "{WIN_RUN_BUDGET} {}",
                i18n::tr3(
                    lang,
                    "reprises sans atteindre l'arrêt, toujours en cours — relancez « Continuer »",
                    "resumes without reaching the breakpoint, still going — hit “Continue” again",
                    "reanudaciones sin alcanzar la parada, sigue en curso — pulse «Continuar» de nuevo",
                )
            );
        }
        self.finish_win_debug_step();
    }

    /// Referme la session (voir `stop()`, qui la relâche déjà — passer par
    /// lui plutôt que par un simple `self.win_dbg = None` garde un seul
    /// message de statut et une seule coupure pour « Arrêter », quelle que
    /// soit la cible en cours).
    pub(super) fn win_debug_stop(&mut self) {
        self.stop();
    }

    fn finish_win_debug_step(&mut self) {
        let Some(state) = self.win_dbg.as_ref().map(|d| d.state) else {
            return;
        };
        let lang = self.lang;
        match state {
            WinRunState::Stopped => {
                let rip = self.win_dbg.as_ref().unwrap().regs().rip;
                self.status = format!("{} — RIP @ 0x{rip:X}", i18n::tr3(lang, "Arrêté", "Stopped", "Detenido"));
            }
            WinRunState::Exited(code) => {
                let msg = format!("{} (exit {code})", i18n::tr3(lang, "Terminé", "Terminated", "Terminado"));
                self.log(&msg);
                self.status = msg;
            }
            WinRunState::Signaled => {
                let msg = i18n::tr3(lang, "Terminé (signal)", "Terminated (signal)", "Terminado (señal)").to_string();
                self.log(&msg);
                self.status = msg;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble::Target;
    use std::path::PathBuf;

    /// Le même programme que les tests de `crate::win_debugger`, pour rester
    /// vérifié contre un vrai `winedbg` : écrit une ligne, sort avec le code 7.
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

    fn app_with(tag: &str, source: &str) -> App {
        let mut app = App::new();
        app.src_path = PathBuf::from(format!("build/windbgops-{tag}.asm"));
        app.out_dir = PathBuf::from(format!("build/windbgops-{tag}"));
        app.source = source.to_string();
        app.set_target(Target::Windows);
        app
    }

    #[test]
    fn starting_a_session_stops_before_the_first_instruction() {
        if !WinDebugger::available() {
            eprintln!("wine absent : session Windows non vérifiée");
            return;
        }
        let mut app = app_with("start", HELLO);
        app.win_debug_start();
        assert!(app.win_dbg.is_some(), "la session doit démarrer");
        assert!(app.win_dbg.as_ref().unwrap().is_alive());
    }

    /// Rien ne démarre hors cible Windows : `step_any`/`cont_any` doivent
    /// retomber sur le chemin natif (message d'indisponibilité compris) sans
    /// jamais toucher `win_dbg`.
    #[test]
    fn a_linux_target_never_touches_the_windows_session() {
        let mut app = App::new();
        app.step_any();
        app.cont_any();
        assert!(app.win_dbg.is_none());
    }

    #[test]
    fn stepping_advances_rip() {
        if !WinDebugger::available() {
            eprintln!("wine absent : pas-à-pas Windows non vérifié");
            return;
        }
        let mut app = app_with("step", HELLO);
        app.win_debug_start();
        let before = app.win_dbg.as_ref().expect("session").regs().rip;
        app.win_debug_step();
        let after = app.win_dbg.as_ref().expect("session").regs().rip;
        assert_ne!(before, after, "RIP doit avancer d'un pas");
    }

    #[test]
    fn continuing_without_breakpoints_runs_to_the_end() {
        if !WinDebugger::available() {
            eprintln!("wine absent : fin de programme non vérifiée");
            return;
        }
        let mut app = app_with("cont", HELLO);
        app.win_debug_start();
        app.win_debug_cont();
        assert_eq!(app.win_dbg.as_ref().map(|d| d.state), Some(WinRunState::Exited(7)));
    }

    /// Le cœur de la fonctionnalité : un point d'arrêt posé dans l'éditeur
    /// (comme pour la cible Linux) arrête bien « Continuer » côté Windows.
    #[test]
    fn a_breakpoint_set_in_the_editor_stops_continue_there() {
        if !WinDebugger::available() {
            eprintln!("wine absent : point d'arrêt Windows non vérifié");
            return;
        }
        let mut app = app_with("bp", HELLO);
        let line = app
            .source
            .lines()
            .position(|l| l.contains("call") && l.contains("ExitProcess"))
            .map(|i| i + 1)
            .expect("la ligne « call ExitProcess » existe dans HELLO");
        app.toggle_breakpoint(line);

        app.win_debug_start();
        app.win_debug_cont();

        let d = app.win_dbg.as_ref().expect("session active");
        assert!(d.is_alive(), "arrêté sur le point d'arrêt, pas terminé");
    }

    #[test]
    fn stopping_clears_the_session() {
        if !WinDebugger::available() {
            eprintln!("wine absent : arrêt de session non vérifié");
            return;
        }
        let mut app = app_with("stop", HELLO);
        app.win_debug_start();
        assert!(app.win_dbg.is_some());
        app.win_debug_stop();
        assert!(app.win_dbg.is_none());
    }
}
