use eframe::egui;

use crate::assemble;
use crate::debugger::{Debugger, RunState};
use crate::disasm;
use crate::i18n;
use crate::srcmap;
use crate::syscall;

use super::{App, SyscallLog};

/// Nombre maximal d'instructions enchaînées par « Continuer » avant de rendre
/// la main à l'interface. Sans ce plafond, une boucle infinie figerait l'IDE
/// exactement comme le faisait l'attente d'un appel système bloquant.
///
/// Un pas coûte une dizaine de microsecondes (deux allers-retours `ptrace` et
/// la capture de la fenêtre de pile) : ce budget borne donc l'à-coup à environ
/// une seconde, au-delà de laquelle la barre d'état invite à relancer.
const RUN_BUDGET: usize = 100_000;

/// Adresses d'arrêt et condition à y vérifier (`None` : arrêt inconditionnel).
type StopMap = std::collections::HashMap<u64, Option<crate::breakpoint::Condition>>;

/// Faut-il s'arrêter dans cet état ? Une condition posée sur une ligne n'est
/// évaluée que lorsque l'exécution y arrive : rien ne sert de la vérifier
/// ailleurs, et c'est ce qui garde le pas à une dizaine de microsecondes.
fn stops_here(stops: &StopMap, regs: &crate::debugger::Registers) -> bool {
    match stops.get(&regs.rip) {
        None => false,
        Some(None) => true,
        Some(Some(cond)) => cond.eval(regs, &crate::debugger::Flags::from_eflags(regs.eflags)),
    }
}

impl App {
    /// Adopte la cible que le source ouvert permet d'identifier.
    ///
    /// Un fichier `.asm` n'embarque pas son format. Quand ses marqueurs sont
    /// non ambigus, les ignorer fait assembler un source Windows en ELF (ou
    /// l'inverse) et produit un diagnostic NASM qui ne correspond pas au
    /// problème. Ouvrir un tel fichier doit donc aussi réactiver l'option PE si
    /// elle était masquée dans les réglages : autrement `set_target` refuserait
    /// précisément la cible que le source réclame.
    pub(super) fn adopt_detected_target(&mut self, source: &str) {
        let Some(target) = assemble::detect_target(source) else {
            return;
        };
        if target.is_windows() {
            self.pe_enabled = true;
        }
        self.set_target(target);
    }

    /// Enregistre puis assemble le programme de l'utilisateur, pour la cible
    /// courante : `nasm` + `ld` sous Linux, `nasm -f win64` + le lieur intégré
    /// pour Windows.
    pub(super) fn build(&mut self) {
        self.save_source();
        let project = self.project.clone();
        // Artefacts dans un sous-dossier `build/` À CÔTÉ du fichier source
        // (et non plus dans un `build/` global relatif au répertoire courant).
        self.out_dir = project
            .as_ref()
            .map(|p| p.root.join("build"))
            .unwrap_or_else(|| super::abs_dir_of(&self.src_path).join("build"));
        let result = match project.as_ref() {
            Some(project) => assemble::assemble_project(project, &self.out_dir, self.target, self.lang),
            None => assemble::assemble_for(&self.src_path, &self.out_dir, &self.include_dirs(), self.target, self.lang),
        };
        match result {
            Ok(out) => {
                self.log(&out.log);
                // Mapping adresse → ligne source (suivi dans l'éditeur).
                self.src_map = disasm::section_address(&out.binary, ".text")
                    .map(|base| srcmap::parse(&out.listing, base))
                    .unwrap_or_default();
                // Description du binaire produit : c'est la seule chose que
                // l'IDE puisse offrir d'un `.exe`, et elle vaut aussi pour un ELF.
                match crate::binfmt::inspect(&out.binary, self.lang) {
                    Ok(info) => self.format_info = Some(info),
                    Err(e) => {
                        self.format_info = None;
                        self.log(&e);
                    }
                }
                self.binary = Some(out.binary);
                self.status = "Build OK".to_string();
            }
            Err(e) => {
                self.log(&e);
                self.binary = None;
                self.format_info = None;
                self.status = i18n::tr(self.lang, "Échec build", "Build failed").to_string();
            }
        }
    }

    /// Applique le réglage « assemblage Windows » après un changement.
    ///
    /// Couper l'option pendant qu'une cible Windows est active laisserait un
    /// état invisible et indéfaisable : plus aucun menu ne montrerait la cible,
    /// mais « Lancer » continuerait de produire un `.exe`. On revient donc à
    /// Linux, et on le dit dans la console.
    pub(super) fn apply_pe_setting(&mut self) {
        if !self.pe_enabled && self.target.is_windows() {
            self.set_target(assemble::Target::Linux);
        }
    }

    /// Change la cible d'assemblage. Le binaire produit pour l'ancienne n'a plus
    /// de sens : on arrête ce qui tourne plutôt que de laisser un débogueur
    /// piloter un exécutable qui n'est plus celui du source affiché.
    pub(super) fn set_target(&mut self, target: crate::assemble::Target) {
        // Garde-fou : la cible Windows n'existe pas tant que l'option est
        // décochée, quel que soit le chemin emprunté (menu, palette, réglage
        // relu d'un fichier écrit à la main).
        if target.is_windows() && !self.pe_enabled {
            return;
        }
        if self.target == target {
            return;
        }
        self.target = target;
        // Dans un projet, la cible fait partie du contrat partagé avec les
        // autres sources. La modifier depuis le menu doit donc aussi modifier
        // le manifeste : autrement la prochaine ouverture reviendrait à une
        // cible différente de celle que l'élève vient de choisir.
        let project_manifest = self.project.as_mut().map(|project| {
            project.target = target;
            (project.manifest.clone(), project.content())
        });
        if let Some((manifest, content)) = project_manifest
            && let Err(e) = std::fs::write(&manifest, content)
        {
            self.log(&format!(
                "{} {}: {e}",
                i18n::tr3(self.lang, "Impossible d'enregistrer la cible du projet dans", "Could not save the project target to", "No se pudo guardar el destino del proyecto en"),
                manifest.display()
            ));
        }
        self.stop();
        self.binary = None;
        self.format_info = None;
        self.save_settings();
        let lang = self.lang;
        self.log(&format!(
            "→ {}",
            match target {
                assemble::Target::Linux => i18n::tr3(
                    lang,
                    "cible Linux (ELF64) : assemblage, exécution et débogage",
                    "Linux target (ELF64): assembling, running and debugging",
                    "destino Linux (ELF64): ensamblado, ejecución y depuración",
                ),
                assemble::Target::Windows | assemble::Target::WindowsGui => i18n::tr3(
                    lang,
                    "cible Windows (PE64) : assemblage, lecture du format, et exécution sous Wine s'il est installé — mais pas de pas-à-pas",
                    "Windows target (PE64): assembling, format inspection, and execution under Wine when installed — but no single-stepping",
                    "destino Windows (PE64): ensamblado, inspección del formato y ejecución con Wine si está instalado — pero sin paso a paso",
                ),
            }
        ));
    }

    /// Relit l'énoncé d'exercice depuis le source courant. Appelé à l'ouverture
    /// d'un fichier et à chaque lancement (le source a pu être édité entre-temps).
    pub(super) fn reload_exercise(&mut self) {
        self.exercise = crate::exercise::parse(&self.source);
        self.checks.clear();
        // Un fichier qui déclare des attentes ouvre son panneau : l'élève ne
        // devrait pas avoir à deviner qu'il y a un énoncé à lire.
        if self.has_exercise() {
            self.show_panel(super::dock::Panel::Exercise);
        }
        for e in &self.exercise.errors.clone() {
            self.log(&format!("⚠ énoncé : {e}"));
        }
    }

    /// Vérifie les attentes de l'exercice contre l'état final observé.
    fn verify_exercise(&mut self, exit_code: Option<i32>) {
        if !self.exercise.is_exercise() {
            return;
        }
        let Some(d) = self.dbg.as_ref() else { return };
        let regs = d.regs().clone();
        self.checks = crate::exercise::check(&self.exercise, &regs, exit_code, &self.source);
        let summary = crate::exercise::summary(&self.checks, self.lang);
        self.log(&summary);
        self.status = summary;
    }

    pub(super) fn launch(&mut self) {
        self.build();
        self.reload_exercise();
        let Some(bin) = self.binary.clone() else {
            return;
        };
        // Cible Windows : désassemblage, lecture du format, puis exécution par
        // Wine s'il est là. Pas de pas-à-pas : le débogueur suit les adresses
        // du binaire qu'il a produit, alors qu'un PE lancé par Wine démarre
        // derrière un chargeur, ailleurs. Mieux vaut ne rien montrer que
        // montrer des registres qui ne sont pas les siens.
        if !self.target.is_runnable() {
            match disasm::disassemble_text(&bin) {
                Ok(insns) => self.disasm = insns,
                Err(e) => self.log(&e),
            }
            self.disasm_index = self
                .disasm
                .iter()
                .enumerate()
                .map(|(i, insn)| (insn.address, i))
                .collect();
            self.dbg = None;
            self.show_panel(super::dock::Panel::Format);
            self.run_under_wine(&bin);
            return;
        }
        match disasm::disassemble_text(&bin) {
            Ok(insns) => self.disasm = insns,
            Err(e) => self.log(&e),
        }
        self.disasm_index = self
            .disasm
            .iter()
            .enumerate()
            .map(|(i, insn)| (insn.address, i))
            .collect();
        self.mem_addr = disasm::section_address(&bin, ".data")
            .or_else(|| disasm::section_address(&bin, ".text"))
            .unwrap_or(0);
        self.mem_input = format!("0x{:X}", self.mem_addr);
        self.selected = None;
        self.syscalls.clear();
        self.call_stack.clear();
        self.trace_cursor = 0;
        self.trace_tail_done = false;
        self.step_in_flight = false;
        self.run_pending = None;
        self.pending_syscall = None;
        self.stdin_input.clear();
        self.stdin_focus_claimed = false;
        self.program_output_input_focus_claimed = false;
        // La boîte montre la sortie de CETTE exécution : garder celle d'avant
        // ferait lire à l'élève un résultat qui n'est plus le sien. La console,
        // elle, garde tout — c'est son rôle de raconter la séance.
        self.program_output.clear();
        self.diagnosis = None;
        self.prediction = None;
        self.pred_input.clear();
        self.view_index = 0;
        self.dbg = None;
        match Debugger::launch(&bin) {
            Ok(dbg) => {
                self.status = format!("{} 0x{:X}", i18n::tr(self.lang, "Lancé — RIP @", "Started — RIP @"), dbg.regs().rip);
                self.log("Running...");
                self.dbg = Some(dbg);
            }
            Err(e) => {
                let msg = e.message(self.lang);
                self.log(&msg);
                self.status = i18n::tr(self.lang, "Échec lancement", "Launch failed").to_string();
            }
        }
    }

    pub(super) fn stop(&mut self) {
        self.dbg = None;
        // Un programme lancé sous Wine ne s'arrête pas tout seul quand on
        // ferme le débogueur : il faut le tuer, sinon une boucle infinie
        // continue de tourner en arrière-plan, invisible.
        if let Some(mut run) = self.wine.take() {
            run.kill();
        }
        // Une consigne d'exécution en attente ne doit pas survivre au
        // programme qu'elle pilotait.
        self.run_pending = None;
        self.step_in_flight = false;
        self.status = i18n::tr(self.lang, "Arrêté", "Stopped").to_string();
    }

    pub(super) fn step(&mut self) {
        if !self.can_step() {
            return;
        }
        // Appel système sur le point de s'exécuter (RIP) : pour le journal
        // console. Mémorisé dans `self` car le pas peut rester en suspens
        // plusieurs frames (appel bloquant) avant d'être journalisable.
        self.pending_syscall = self.dbg.as_ref().and_then(|d| {
            let insn = self.insn_at(d.regs().rip)?;
            (insn.mnemonic == "syscall").then(|| (syscall::format_call(d.regs()), d.regs().rax))
        });

        let lang = self.lang;
        if let Some(d) = self.dbg.as_mut()
            && let Err(e) = d.step()
        {
            let msg = e.message(lang);
            self.log(&msg);
            return;
        }
        self.step_in_flight = true;
        self.finish_step_if_done();
    }

    /// Sonde le débogueur à chaque frame : récupère ce que le programme a
    /// écrit, et finalise le pas en cours dès qu'il s'achève.
    ///
    /// Sans ce sondage, un `read` sur l'entrée standard bloquerait le
    /// `waitpid` — donc l'interface entière — jusqu'à la saisie.
    pub(super) fn poll_debugger(&mut self, ctx: &egui::Context) {
        let lang = self.lang;
        if let Some(d) = self.dbg.as_mut()
            && d.is_waiting()
            && let Err(e) = d.poll()
        {
            let msg = e.message(lang);
            self.log(&msg);
        }
        self.drain_program_output();
        // Un `read` bloquant est aussi une interaction, même si le programme
        // n'a pas encore écrit le moindre octet : afficher sa sortie dédiée
        // donne immédiatement le contexte et le champ de saisie de la console
        // n'apparaît plus comme une demande sortie de nulle part.
        if self.dbg.as_ref().is_some_and(|d| d.is_waiting()) {
            self.show_program_output = true;
        }
        if self.step_in_flight {
            self.finish_step_if_done();
        }
        // L'appel système a rendu la main : on reprend le « Continuer » qu'il
        // avait interrompu, sauf si l'on vient justement d'atteindre un point
        // d'arrêt (`run_until` teste la condition après le pas, pas avant) ou
        // si le programme s'est terminé entre-temps.
        if let Some(extra) = self.run_pending
            && self.dbg.as_ref().is_none_or(|d| !d.is_waiting())
        {
            self.run_pending = None;
            let stops = self.stop_addresses(extra);
            let at_stop = self.snap().is_some_and(|s| stops_here(&stops, &s.regs));
            if !at_stop && self.dbg.as_ref().is_some_and(|d| d.is_ready()) {
                self.run_until(extra);
            }
        }
        // Tant que le programme est suspendu, on redemande une frame : rien
        // d'autre ne réveillerait l'UI quand le syscall rendra la main.
        if self.dbg.as_ref().is_some_and(|d| d.is_waiting()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
    }

    /// Lance l'exécutable Windows sous Wine, ou explique pourquoi il ne se
    /// lancera pas.
    ///
    /// Wine est cherché à chaque lancement plutôt qu'une fois pour toutes :
    /// l'installer pendant que l'IDE tourne doit suffire à s'en servir, sans
    /// redémarrer.
    fn run_under_wine(&mut self, bin: &std::path::Path) {
        self.program_output.clear();
        if !crate::winerun::available() {
            self.wine = None;
            self.log(i18n::tr3(
                self.lang,
                "Exécutable Windows produit. Wine n'étant pas installé, il ne peut pas être lancé ici : le panneau FORMAT montre ce qu'il contient, et le fichier .exe s'exécute tel quel sur une machine Windows. Installez wine pour le lancer depuis l'IDE.",
                "Windows executable produced. Wine is not installed, so it cannot be run here: the FORMAT panel shows what it contains, and the .exe runs as-is on a Windows machine. Install wine to run it from the IDE.",
                "Ejecutable de Windows producido. Wine no está instalado, así que no puede ejecutarse aquí: el panel FORMATO muestra lo que contiene, y el .exe se ejecuta tal cual en una máquina Windows. Instale wine para ejecutarlo desde el IDE.",
            ));
            self.status = i18n::tr3(self.lang, "Assemblé (PE64)", "Assembled (PE64)", "Ensamblado (PE64)").to_string();
            return;
        }
        match crate::winerun::WineRun::spawn(bin) {
            Ok(run) => {
                // Remplacer l'ancien processus le tue (voir `Drop`) : deux
                // programmes de l'élève ne tournent jamais en même temps.
                self.wine = Some(run);
                self.log(i18n::tr3(
                    self.lang,
                    "Exécution sous Wine. Pas de pas-à-pas ici : Wine exécute le programme, il ne le déroule pas instruction par instruction. La sortie arrive dans la console.",
                    "Running under Wine. No single-stepping here: Wine runs the program, it does not walk it instruction by instruction. Output lands in the console.",
                    "Ejecución con Wine. Sin paso a paso aquí: Wine ejecuta el programa, no lo recorre instrucción por instrucción. La salida llega a la consola.",
                ));
                self.status = i18n::tr3(
                    self.lang,
                    "En cours d'exécution (Wine)…",
                    "Running (Wine)…",
                    "En ejecución (Wine)…",
                )
                .to_string();
            }
            Err(e) => {
                self.wine = None;
                self.log(&e);
                self.status = i18n::tr3(self.lang, "Assemblé (PE64)", "Assembled (PE64)", "Ensamblado (PE64)").to_string();
            }
        }
    }

    /// Vérifie les attentes d'un exercice Windows, à partir du seul code de
    /// sortie.
    ///
    /// Wine exécute, il ne déroule pas : il n'y a pas de registres à lire à la
    /// fin. Les attentes portant sur un registre ne sont donc pas *fausses*,
    /// elles sont invérifiables — les compter en échec ferait mentir le panneau.
    /// Elles sont écartées du verdict et signalées dans la console, ce qui est
    /// la seule réponse honnête.
    fn verify_exercise_from_exit(&mut self, exit_code: i32) {
        if !self.exercise.is_exercise() {
            return;
        }
        let unverifiable: Vec<String> = self
            .exercise
            .expectations
            .iter()
            .filter(|e| !matches!(e.subject, crate::exercise::Subject::ExitCode))
            .map(|e| e.label())
            .collect();
        let mut checkable = self.exercise.clone();
        checkable
            .expectations
            .retain(|e| matches!(e.subject, crate::exercise::Subject::ExitCode));

        self.checks = crate::exercise::check(
            &checkable,
            &crate::debugger::Registers::default(),
            Some(exit_code),
            &self.source,
        );
        for label in &unverifiable {
            self.log(&format!(
                "{} {label}",
                i18n::tr3(
                    self.lang,
                    "⚠ attente non vérifiable en cible Windows (pas de débogueur) :",
                    "⚠ expectation not checkable on the Windows target (no debugger):",
                    "⚠ expectativa no verificable en el destino Windows (sin depurador):"
                )
            ));
        }
        if !self.checks.is_empty() {
            let summary = crate::exercise::summary(&self.checks, self.lang);
            self.log(&summary);
            self.status = summary;
        }
    }

    /// Sonde le programme lancé sous Wine : sortie vers la console, puis code
    /// de sortie quand il s'achève. Sans effet si rien ne tourne.
    pub(super) fn poll_wine(&mut self, ctx: &egui::Context) {
        let Some(run) = self.wine.as_mut() else { return };
        let out = run.take_output();
        let done = run.poll();
        if !out.is_empty() {
            self.program_out_push(&out);
        }
        match done {
            Some(code) => {
                self.wine = None;
                let lang = self.lang;
                self.log(&format!(
                    "{} {code}",
                    i18n::tr3(lang, "Terminé, code de sortie", "Finished, exit code", "Terminado, código de salida")
                ));
                self.status = format!(
                    "{} {code}",
                    i18n::tr3(lang, "Terminé (Wine) — code", "Finished (Wine) — code", "Terminado (Wine) — código")
                );
                // Le programme a rendu son code : les leçons et exercices du
                // parcours Windows se corrigent là-dessus.
                self.verify_exercise_from_exit(code);
            }
            // Rien d'autre ne réveillerait l'interface quand le programme
            // écrira : c'est un processus extérieur, pas un événement egui.
            None => ctx.request_repaint_after(std::time::Duration::from_millis(30)),
        }
    }

    /// Verse dans la console ce que le programme a écrit sur sa sortie
    /// standard ou d'erreur, tel quel (le programme mène ses retours à la ligne).
    fn drain_program_output(&mut self) {
        let out = match self.dbg.as_mut() {
            Some(d) => d.take_output(),
            None => return,
        };
        if !out.is_empty() {
            self.program_out_push(&out);
        }
    }

    /// Envoie une ligne au programme suspendu sur un `read`.
    pub(super) fn send_stdin(&mut self) {
        let mut line = std::mem::take(&mut self.stdin_input);
        line.push('\n');
        // Écho dans la console : sinon l'élève ne garde aucune trace de ce
        // qu'il a saisi, un terminal l'affichant d'ordinaire de lui-même. Il
        // compte donc aussi dans la sortie « telle qu'au terminal » — c'est
        // précisément le terminal qui produirait cet écho.
        self.program_out_push(&line);
        let lang = self.lang;
        if let Some(d) = self.dbg.as_mut() {
            if let Err(e) = d.write_stdin(&line) {
                let msg = e.message(lang);
                self.log(&msg);
            }
        } else if let Some(run) = self.wine.as_mut()
            && let Err(e) = run.write_stdin(&line, lang)
        {
            self.log(&e);
        }
    }

    /// Clôt le pas en cours si le débogueur n'attend plus : journal, trace,
    /// prédiction, barre d'état. Sans effet tant que le pas est suspendu.
    fn finish_step_if_done(&mut self) {
        let Some(state) = self.dbg.as_ref().map(|d| d.state) else {
            self.step_in_flight = false;
            return;
        };
        if state == RunState::Running {
            self.show_program_output = true;
            self.status = i18n::tr(
                self.lang,
                "En attente d'une entrée du programme…",
                "Waiting for program input…",
            )
            .to_string();
            return;
        }
        self.step_in_flight = false;
        // Ce que le programme a écrit doit précéder le journal de l'appel
        // système qui l'a produit, et être là dès la fin du pas — sans
        // attendre le sondage de la frame suivante.
        self.drain_program_output();

        if let Some(d) = self.dbg.as_ref() {
            self.view_index = d.history.len() - 1;
        }
        self.pending_flash = true; // déclenche l'animation « CPU vivant »
        // Le nouvel état est en place : la prédiction en attente peut être jugée.
        self.resolve_prediction();

        // Complète pile d'appels + journal syscalls avec les transitions
        // nouvellement apparues dans l'historique.
        self.extend_trace();

        // Journalise l'appel système dans la console (une fois, à son exécution).
        if let Some((call, num)) = self.pending_syscall.take() {
            if syscall::is_exit(num) {
                self.log(&call);
            } else if let Some(d) = self.dbg.as_ref() {
                self.log(&format!("{call} = {}", d.regs().rax as i64));
            }
        }
        match state {
            RunState::Stopped => {
                let d = self.dbg.as_ref().unwrap();
                self.status = format!("{} {} — RIP @ 0x{:X}", i18n::tr(self.lang, "Étape", "Step"), d.steps(), d.regs().rip);
            }
            RunState::Exited(code) => {
                self.status = format!("{} (exit {code})", i18n::tr(self.lang, "Terminé", "Terminated"));
                self.verify_exercise(Some(code));
            }
            RunState::Signaled => {
                self.status = i18n::tr(self.lang, "Terminé (signal)", "Terminated (signal)").to_string();
                self.verify_exercise(None);
            }
            // Faute matérielle : on diagnostique tout de suite et on ouvre la
            // fenêtre d'explication (l'ancien code laissait RIP figé en silence).
            RunState::Faulted(_) => {
                self.diagnose_fault();
                // Un plantage fait échouer l'exercice : pas de code de sortie.
                self.verify_exercise(None);
            }
            RunState::Running => unreachable!("écarté plus haut"),
        }
    }

    /// Enchaîne les pas jusqu'au prochain point d'arrêt, la fin du programme,
    /// ou l'épuisement du budget d'instructions.
    ///
    /// `extra_stop` ajoute une condition d'arrêt ponctuelle aux points
    /// d'arrêt de l'élève : c'est ce qui sert au pas-par-dessus (`step_over`).
    fn run_until(&mut self, extra_stop: Option<u64>) {
        if !self.can_step() {
            return;
        }
        let stops = self.stop_addresses(extra_stop);

        // Sortie et journal se reconstruisent après coup : le premier pas peut
        // très bien être celui qui exécute un appel système.
        self.pending_syscall = None;
        let lang = self.lang;
        let done = match self.dbg.as_mut() {
            Some(d) => match d.run_until(RUN_BUDGET, |regs| stops_here(&stops, regs)) {
                Ok(n) => n,
                Err(e) => {
                    let msg = e.message(lang);
                    self.log(&msg);
                    return;
                }
            },
            None => return,
        };
        self.step_in_flight = true;
        // Un appel système bloquant a coupé l'enchaînement : on note la
        // consigne pour la reprendre au déblocage, sinon « Continuer » sur un
        // programme qui lit une entrée s'arrêterait là sans rien dire.
        self.run_pending = self
            .dbg
            .as_ref()
            .is_some_and(|d| d.is_waiting())
            .then_some(extra_stop);
        self.finish_step_if_done();

        // Budget épuisé sans rencontrer d'arrêt : on le dit plutôt que de
        // laisser croire que le programme s'est arrêté tout seul.
        if done >= RUN_BUDGET && self.dbg.as_ref().is_some_and(|d| d.is_ready()) {
            self.status = format!(
                "{RUN_BUDGET} {}",
                i18n::tr(
                    self.lang,
                    "instructions exécutées, toujours en cours — relancez « Continuer »",
                    "instructions run, still going — hit “Continue” again",
                )
            );
        }
    }

    /// Adresses où l'exécution doit s'interrompre, avec la condition à y
    /// vérifier : les lignes marquées par l'élève, plus une éventuelle cible
    /// ponctuelle inconditionnelle (retour d'un `call`, pour le pas
    /// par-dessus).
    ///
    /// Les conditions sont copiées ici plutôt que consultées à travers `self` :
    /// la fermeture d'arrêt est passée au débogueur, qui est lui-même emprunté
    /// en `&mut` sur `self` pendant tout l'enchaînement.
    fn stop_addresses(&self, extra: Option<u64>) -> StopMap {
        let mut stops: StopMap = self
            .src_map
            .iter()
            .filter_map(|(addr, line)| {
                self.breakpoints.get(line).map(|cond| (*addr, cond.clone()))
            })
            .collect();
        if let Some(addr) = extra {
            stops.insert(addr, None);
        }
        stops
    }

    /// Exécute jusqu'au prochain point d'arrêt (ou la fin du programme).
    pub(super) fn cont(&mut self) {
        self.run_until(None);
    }

    /// Exécute l'instruction courante ; si c'est un `call`, exécute la
    /// fonction appelée d'un bloc et s'arrête à l'instruction suivante.
    pub(super) fn step_over(&mut self) {
        let is_call = self
            .view_rip()
            .and_then(|rip| self.insn_at(rip))
            .is_some_and(|i| i.mnemonic == "call");
        match (is_call, self.next_addr()) {
            (true, Some(ret)) => self.run_until(Some(ret)),
            _ => self.step(),
        }
    }

    /// Pose ou retire un point d'arrêt sur une ligne source (1-based). Le
    /// retirer emporte sa condition : la reposer repart d'une ligne vierge.
    pub(super) fn toggle_breakpoint(&mut self, line: usize) {
        if self.breakpoints.remove(&line).is_none() {
            self.breakpoints.insert(line, None);
        }
    }

    /// Attache une condition à une ligne (le point d'arrêt est posé au besoin),
    /// ou la retire si le texte est vide.
    ///
    /// Renvoie le message d'analyse en cas de syntaxe refusée — la condition
    /// précédente reste alors en place, plutôt que d'être perdue au profit de
    /// rien.
    pub(super) fn set_breakpoint_condition(
        &mut self,
        line: usize,
        text: &str,
    ) -> Result<(), String> {
        let cond = crate::breakpoint::parse(text, self.lang)?;
        self.breakpoints.insert(line, cond);
        Ok(())
    }

    /// Condition posée sur une ligne, s'il y en a une.
    pub(super) fn breakpoint_condition(
        &self,
        line: usize,
    ) -> Option<&crate::breakpoint::Condition> {
        self.breakpoints.get(&line)?.as_ref()
    }

    /// Vrai si cette ligne source (1-based) porte du code exécutable, donc
    /// peut recevoir un point d'arrêt utile.
    pub(super) fn line_is_executable(&self, line: usize) -> bool {
        self.src_map.values().any(|l| *l == line)
    }

    /// Instruction désassemblée à cette adresse, par l'index plutôt qu'en
    /// balayant tout le désassemblage.
    pub(super) fn insn_at(&self, addr: u64) -> Option<&crate::disasm::Insn> {
        self.disasm_index.get(&addr).and_then(|i| self.disasm.get(*i))
    }

    /// Analyse la faute courante, met à jour la barre d'état et ouvre la fenêtre
    /// de diagnostic. Idempotent : rappelé sans effet si le diagnostic existe.
    pub(super) fn diagnose_fault(&mut self) {
        if self.diagnosis.is_some() {
            return;
        }
        let Some(d) = self.dbg.as_ref() else { return };
        let Some(fault) = d.fault() else { return };
        let regions = d.mem_regions();

        // L'instruction fautive écrivait-elle ? Le désassemblage le dit.
        let is_write = self
            .disasm
            .iter()
            .find(|i| i.address == fault.rip)
            .is_some_and(|i| crate::diagnostic::writes_memory(&i.mnemonic, &i.operands));
        let line = self.src_map.get(&fault.rip).copied();

        let diag = crate::diagnostic::diagnose(&fault, &regions, is_write, line, self.lang);
        self.status = format!(
            "✘ {} — {}",
            diag.title,
            crate::diagnostic::cause_label(diag.cause, self.lang)
        );
        self.log(&format!("✘ {} : {}", diag.title, diag.explanation));
        self.diagnosis = Some(diag);
    }

    pub(super) fn resume_here(&mut self) {
        let Some(bin) = self.binary.clone() else { return };
        let target = self.view_index;
        match Debugger::launch(&bin) {
            Ok(mut d) => {
                // `is_ready` et non `is_alive` : un pas resté suspendu dans un
                // appel système laisse `step` sans effet, et la boucle
                // tournerait à vide jusqu'au bout du compte.
                let mut blocked = false;
                for _ in 0..target {
                    if !d.is_ready() {
                        blocked = d.is_waiting();
                        break;
                    }
                    let _ = d.step();
                }
                // Le rejeu réécrit sur la sortie standard tout ce que
                // l'exécution d'origine avait déjà écrit. Ces octets sont dans
                // la console depuis le premier passage : les verser une
                // seconde fois la ferait bégayer.
                let _ = d.take_output();
                self.view_index = d.history.len() - 1;
                self.status = if blocked {
                    // L'entrée saisie la première fois n'est pas rejouée : le
                    // programme réclame la sienne, et on s'arrête là plutôt
                    // que d'annoncer une étape qu'on n'a pas atteinte.
                    format!(
                        "{} {} — {}",
                        i18n::tr(self.lang, "Repris à l'étape", "Resumed at step"),
                        self.view_index,
                        i18n::tr(
                            self.lang,
                            "le programme attend une entrée, saisissez-la pour aller plus loin",
                            "the program is waiting for input, type it to go further",
                        )
                    )
                } else {
                    format!("{} {}", i18n::tr(self.lang, "Repris à l'étape", "Resumed at step"), self.view_index)
                };
                self.selected = None;
                self.dbg = Some(d);
                self.rebuild_trace(); // resynchronise call stack + syscalls
            }
            Err(e) => {
                let msg = e.message(self.lang);
                self.log(&msg);
            }
        }
    }

    /// Repart de zéro sur `call_stack` et `syscalls`. Nécessaire quand
    /// l'historique lui-même change de sens sous nos pieds — au relancement,
    /// et après « Reprendre ici » qui rejoue le programme depuis le début.
    pub(super) fn rebuild_trace(&mut self) {
        self.call_stack.clear();
        self.syscalls.clear();
        self.trace_cursor = 0;
        self.trace_tail_done = false;
        self.extend_trace();
    }

    /// Complète `call_stack` et `syscalls` avec les transitions apparues
    /// depuis le dernier appel. Chaque transition `history[i] → history[i+1]`
    /// correspond à l'exécution de l'instruction à `history[i].rip`.
    ///
    /// Le dépouillement est incrémental : tout reprendre à chaque pas rendait
    /// le coût d'un pas proportionnel à la longueur de l'historique, donc
    /// l'exécution entière quadratique.
    pub(super) fn extend_trace(&mut self) {
        // Petit utilitaire local : décompose "name(args)" en (name, args).
        let log_syscall = |list: &mut Vec<SyscallLog>, regs: &crate::debugger::Registers, ret: Option<i64>| {
            let num = regs.rax;
            let call = syscall::format_call(regs);
            let args = call
                .find('(')
                .map(|p| call[p + 1..].trim_end_matches(')').to_string())
                .unwrap_or_default();
            list.push(SyscallLog { name: syscall::name(num).to_string(), args, number: num, ret, regs: regs.clone() });
        };
        // Sortis de `self` le temps de parcourir l'historique, qui en fait
        // partie aussi.
        let mut call_stack = std::mem::take(&mut self.call_stack);
        let mut syscalls = std::mem::take(&mut self.syscalls);
        let mut cursor = self.trace_cursor;
        let mut tail_done = self.trace_tail_done;

        if let Some(d) = self.dbg.as_ref() {
            let hist = &d.history;
            let last = hist.len().saturating_sub(1);
            for i in cursor..last {
                let cur = &hist[i].regs;
                let next = &hist[i + 1].regs;
                let Some(insn) = self.insn_at(cur.rip) else {
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
            cursor = last;
            // Cas de l'appel qui termine le processus (exit) : il reste en tête de
            // l'historique sans successeur (aucun snapshot après la mort du process).
            if !tail_done
                && !d.is_alive()
                && let Some(head) = hist.last()
                && let Some(insn) = self.insn_at(head.regs.rip)
                && insn.mnemonic == "syscall"
            {
                log_syscall(&mut syscalls, &head.regs, None);
                tail_done = true;
            }
        }
        self.call_stack = call_stack;
        self.syscalls = syscalls;
        self.trace_cursor = cursor;
        self.trace_tail_done = tail_done;
    }

    pub(super) fn next_addr(&self) -> Option<u64> {
        let rip = self.view_rip()?;
        let idx = self.disasm_index.get(&rip)?;
        self.disasm.get(idx + 1).map(|i| i.address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble::Target;
    use std::path::PathBuf;

    /// Une application prête à exécuter `source`. `tag` distingue les
    /// artefacts : sans cela, les tests parallèles assemblent dans le même
    /// dossier et s'écrasent l'un l'autre.
    fn app_with(tag: &str, source: &str) -> App {
        let mut app = App::new();
        app.src_path = PathBuf::from(format!("build/dbgops-{tag}.asm"));
        app.out_dir = PathBuf::from(format!("build/dbgops-{tag}"));
        app.source = source.to_string();
        app.launch();
        app
    }

    #[test]
    fn opening_a_recognisable_source_adopts_its_target() {
        let mut app = App::new();
        app.pe_enabled = false;

        app.adopt_detected_target("global main\nextern ExitProcess\nmain:\n    call ExitProcess\n");

        assert!(app.pe_enabled, "un source PE réactive sa cible");
        assert_eq!(app.target, Target::Windows);

        app.adopt_detected_target("global _start\n_start:\n    syscall\n");

        assert_eq!(app.target, Target::Linux);
    }

    /// Neuf lignes, dont six instructions : les numéros comptent, les tests
    /// posent leurs points d'arrêt dessus.
    const COUNTER: &str = "section .text\n\
                           global _start\n\
                           _start:\n\
                           mov rax,1\n\
                           mov rbx,2\n\
                           mov rcx,3\n\
                           mov rax,60\n\
                           xor rdi,rdi\n\
                           syscall\n";

    /// Un programme qui écrit vraiment, exécuté vraiment : c'est le seul test
    /// qui prouve que la boîte « Sortie du programme » montre bien ce qu'un
    /// terminal montrerait. La console, elle, y ajoute son journal — et c'est
    /// tout l'intérêt de les avoir séparées.
    const GREETER: &str = "section .data\n\
                           msg db \"Bonjour\",10\n\
                           section .text\n\
                           global _start\n\
                           _start:\n\
                           mov rax,1\n\
                           mov rdi,1\n\
                           mov rsi,msg\n\
                           mov rdx,8\n\
                           syscall\n\
                           mov rax,60\n\
                           xor rdi,rdi\n\
                           syscall\n";

    #[test]
    fn the_program_output_holds_exactly_what_the_program_wrote() {
        let mut app = app_with("stdout", GREETER);
        app.cont();

        assert!(
            matches!(app.dbg.as_ref().map(|d| d.state), Some(RunState::Exited(0))),
            "le programme doit aller jusqu'au bout"
        );
        assert_eq!(
            app.program_output, "Bonjour\n",
            "la sortie doit être celle du programme, au caractère près"
        );
        // La console montre la même chose, mais noyée dans le journal : c'est
        // précisément ce dont la boîte débarrasse l'élève.
        assert!(app.console.contains("Bonjour\n"));
        assert!(
            app.console.contains("Running..."),
            "la console garde le journal de l'IDE, la sortie non"
        );
        assert!(
            !app.program_output.contains("Running..."),
            "aucun message de l'IDE ne doit passer dans la sortie du programme"
        );
    }

    /// Relancer repart d'une sortie vierge : sinon l'élève lirait le résultat
    /// de l'exécution précédente en croyant lire le sien.
    #[test]
    fn a_new_run_starts_from_a_blank_output() {
        let mut app = app_with("stdout-again", GREETER);
        app.cont();
        assert_eq!(app.program_output, "Bonjour\n");

        app.launch();
        assert!(app.program_output.is_empty(), "la sortie repart à zéro au lancement");
        app.cont();
        assert_eq!(app.program_output, "Bonjour\n", "et se remplit à nouveau, une fois");
    }

    #[test]
    fn continue_stops_on_the_marked_line() {
        let mut app = app_with("bp", COUNTER);
        app.toggle_breakpoint(6); // « mov rcx,3 »
        app.cont();

        assert_eq!(
            app.current_source_line(),
            Some(5),
            "l'exécution doit s'arrêter sur la ligne marquée (0-based)"
        );
        // Les deux instructions précédentes ont bien été exécutées, chacune
        // avec son snapshot : la timeline ne saute rien.
        assert_eq!(app.dbg.as_ref().expect("dbg").steps(), 2);
        assert_eq!(app.view_index, 2, "la vue suit la tête de l'historique");
    }

    #[test]
    fn continue_without_breakpoints_runs_to_the_end() {
        let mut app = app_with("bp-none", COUNTER);
        app.cont();

        assert!(
            matches!(app.dbg.as_ref().map(|d| d.state), Some(RunState::Exited(0))),
            "sans point d'arrêt, le programme va jusqu'à sa sortie"
        );
    }

    #[test]
    fn a_breakpoint_can_be_taken_back() {
        let mut app = app_with("bp-off", COUNTER);
        app.toggle_breakpoint(6);
        app.toggle_breakpoint(6);
        assert!(app.breakpoints.is_empty());
        app.cont();
        assert!(
            matches!(app.dbg.as_ref().map(|d| d.state), Some(RunState::Exited(0))),
            "plus de point d'arrêt, plus d'arrêt"
        );
    }

    // ---------- Points d'arrêt conditionnels ----------

    /// Une boucle qui décompte : la ligne 5 est franchie dix fois, et c'est
    /// exactement le cas qu'un point d'arrêt nu ne sait pas traiter.
    const LOOP10: &str = "section .text\n\
                          global _start\n\
                          _start:\n\
                          mov rcx,10\n\
                          .tour:\n\
                          dec rcx\n\
                          jnz .tour\n\
                          mov rax,60\n\
                          xor rdi,rdi\n\
                          syscall\n";

    /// Le cœur de la fonctionnalité : dix passages, un seul arrêt.
    #[test]
    fn a_condition_holds_the_stop_until_it_is_true() {
        let mut app = app_with("bp-cond", LOOP10);
        app.toggle_breakpoint(6); // « dec rcx », exécutée dix fois
        app.set_breakpoint_condition(6, "RCX == 3").expect("condition valide");
        app.cont();

        let regs = app.dbg.as_ref().expect("dbg").regs();
        assert_eq!(app.current_source_line(), Some(5), "arrêt sur « dec rcx »");
        assert_eq!(regs.rcx, 3, "et seulement au tour où RCX vaut 3");
    }

    /// Une condition jamais vraie ne retient rien : le programme va au bout,
    /// au lieu de s'arrêter au premier passage comme un point d'arrêt nu.
    #[test]
    fn a_condition_that_never_holds_never_stops() {
        let mut app = app_with("bp-cond-never", LOOP10);
        app.toggle_breakpoint(6);
        app.set_breakpoint_condition(6, "RCX == 0x1234").expect("condition valide");
        app.cont();

        assert!(
            matches!(app.dbg.as_ref().map(|d| d.state), Some(RunState::Exited(0))),
            "condition jamais remplie ⇒ aucun arrêt"
        );
    }

    /// Les drapeaux sont utilisables, et c'est souvent ce qu'on veut observer :
    /// « arrête-toi quand la comparaison a mis ZF ».
    #[test]
    fn a_flag_condition_works_too() {
        let mut app = app_with("bp-cond-flag", LOOP10);
        app.toggle_breakpoint(7); // « jnz .tour »
        app.set_breakpoint_condition(7, "ZF == 1").expect("condition valide");
        app.cont();

        let regs = app.dbg.as_ref().expect("dbg").regs().clone();
        assert_eq!(regs.rcx, 0, "ZF n'est levé que quand le décompte atteint zéro");
    }

    /// Vider le champ retire la condition sans retirer le point d'arrêt.
    #[test]
    fn an_empty_condition_restores_a_plain_breakpoint() {
        let mut app = app_with("bp-cond-clear", LOOP10);
        app.toggle_breakpoint(6);
        app.set_breakpoint_condition(6, "RCX == 3").expect("condition valide");
        assert!(app.breakpoint_condition(6).is_some());

        app.set_breakpoint_condition(6, "  ").expect("vider n'est pas une erreur");
        assert!(app.breakpoint_condition(6).is_none());
        assert!(app.breakpoints.contains_key(&6), "le point d'arrêt, lui, reste");

        app.cont();
        // L'arrêt a lieu AVANT d'exécuter la ligne : au premier tour, le
        // décompte n'a pas encore été touché.
        assert_eq!(app.dbg.as_ref().expect("dbg").regs().rcx, 10, "arrêt dès le premier tour");
    }

    /// Une syntaxe refusée ne doit pas emporter la condition qui marchait.
    #[test]
    fn a_refused_condition_leaves_the_previous_one_in_place() {
        let mut app = app_with("bp-cond-bad", LOOP10);
        app.toggle_breakpoint(6);
        app.set_breakpoint_condition(6, "RCX == 3").expect("condition valide");

        let err = app.set_breakpoint_condition(6, "RCX <> 3").unwrap_err();
        assert!(!err.is_empty(), "l'erreur doit s'expliquer");
        assert_eq!(
            app.breakpoint_condition(6).map(|c| c.to_string()),
            Some("RCX == 3".to_string()),
            "l'ancienne condition tient toujours"
        );
    }

    /// Retirer le point d'arrêt emporte sa condition : le reposer repart d'une
    /// ligne vierge, sans condition fantôme héritée d'une session précédente.
    #[test]
    fn taking_a_breakpoint_back_forgets_its_condition() {
        let mut app = app_with("bp-cond-forget", LOOP10);
        app.toggle_breakpoint(6);
        app.set_breakpoint_condition(6, "RCX == 3").expect("condition valide");
        app.toggle_breakpoint(6); // retiré
        app.toggle_breakpoint(6); // reposé
        assert!(app.breakpoint_condition(6).is_none());
    }

    /// Le pas par-dessus s'arrête au retour du `call` quoi qu'il arrive : sa
    /// cible ponctuelle n'est pas soumise aux conditions de l'élève.
    #[test]
    fn step_over_is_never_held_back_by_a_condition() {
        let mut app = app_with("bp-cond-over", LOOP10);
        app.toggle_breakpoint(6);
        app.set_breakpoint_condition(6, "RCX == 0x1234").expect("condition valide");
        let before = app.dbg.as_ref().expect("dbg").steps();
        app.step_over(); // « mov rcx,10 » : pas un call, donc un pas simple
        assert_eq!(app.dbg.as_ref().expect("dbg").steps(), before + 1);
    }

    /// Un point d'arrêt sur une ligne sans code ne fait rien, et l'interface
    /// le montre (cercle creux) — encore faut-il savoir le reconnaître.
    #[test]
    fn a_line_without_code_is_reported_as_such() {
        let app = app_with("bp-dead", COUNTER);
        assert!(app.line_is_executable(4), "« mov rax,1 » porte du code");
        assert!(!app.line_is_executable(1), "« section .text » n'en porte pas");
    }

    /// Le pas par-dessus franchit l'appel d'un bloc et s'arrête juste après,
    /// au lieu de dérouler la fonction instruction par instruction.
    #[test]
    fn step_over_runs_the_whole_call() {
        let mut app = app_with(
            "over",
            "section .text\n\
             global _start\n\
             _start:\n\
             call inc3\n\
             mov rax,60\n\
             xor rdi,rdi\n\
             syscall\n\
             inc3:\n\
             inc rbx\n\
             inc rbx\n\
             inc rbx\n\
             ret\n",
        );
        // RIP est sur le `call`.
        assert_eq!(app.current_source_line(), Some(3));
        app.step_over();

        assert_eq!(
            app.current_source_line(),
            Some(4),
            "on ressort à l'instruction qui suit le call"
        );
        assert_eq!(
            app.dbg.as_ref().expect("dbg").regs().rbx,
            3,
            "les trois inc de la fonction ont bien tourné"
        );
        // call + 3 inc + ret = 5 instructions, toutes dans l'historique.
        assert_eq!(app.dbg.as_ref().expect("dbg").steps(), 5);
    }

    /// Hors d'un `call`, le pas par-dessus vaut un pas simple.
    #[test]
    fn step_over_on_an_ordinary_instruction_is_a_plain_step() {
        let mut app = app_with("over-plain", COUNTER);
        app.step_over();
        assert_eq!(app.dbg.as_ref().expect("dbg").steps(), 1);
    }

    /// Ce que le programme écrit atterrit dans la console de l'IDE.
    #[test]
    fn program_output_lands_in_the_console() {
        let mut app = app_with(
            "out",
            "section .data\n\
             msg db \"salut\", 10\n\
             section .text\n\
             global _start\n\
             _start:\n\
             mov rax,1\n\
             mov rdi,1\n\
             mov rsi,msg\n\
             mov rdx,6\n\
             syscall\n\
             mov rax,60\n\
             xor rdi,rdi\n\
             syscall\n",
        );
        app.cont();
        assert!(
            app.console.contains("salut\n"),
            "la console doit contenir la sortie du programme, pas seulement le \
             journal des appels système : {}",
            app.console
        );
    }

    /// « Reprendre ici » rejoue le programme depuis le début : il réécrit donc
    /// sur sa sortie ce qui est déjà affiché. La console ne doit pas bégayer.
    #[test]
    fn resuming_does_not_echo_the_output_twice() {
        let mut app = app_with(
            "resume-out",
            "section .data\n\
             msg db \"bonjour\", 10\n\
             section .text\n\
             global _start\n\
             _start:\n\
             mov rax,1\n\
             mov rdi,1\n\
             mov rsi,msg\n\
             mov rdx,8\n\
             syscall\n\
             mov rax,60\n\
             xor rdi,rdi\n\
             syscall\n",
        );
        app.cont();
        assert_eq!(app.console.matches("bonjour").count(), 1, "affiché une fois");

        // On repart d'une étape située APRÈS le `write` — sans quoi le rejeu
        // n'atteindrait pas l'appel système, et le test ne prouverait rien.
        // Quatre `mov` puis le `syscall` : l'étape 5 l'a exécuté.
        app.view_index = 5;
        app.resume_here();
        let ctx = egui::Context::default();
        app.poll_debugger(&ctx);

        assert_eq!(
            app.console.matches("bonjour").count(),
            1,
            "le rejeu ne doit pas remettre la sortie dans la console : {}",
            app.console
        );
    }

    /// « Continuer » sur un programme qui attend une saisie : il s'interrompt
    /// sur le `read`, l'interface reste vivante, et l'enchaînement reprend tout
    /// seul une fois l'entrée fournie.
    #[test]
    fn continue_resumes_by_itself_after_a_blocking_read() {
        let ctx = egui::Context::default();
        let mut app = App::new();
        app.src_path = PathBuf::from("build/dbgops-wait.asm");
        app.out_dir = PathBuf::from("build/dbgops-wait");
        app.source = std::fs::read_to_string("examples/read-stdin.asm").expect("exemple lisible");
        app.launch();

        app.cont();
        assert!(
            app.dbg.as_ref().is_some_and(|d| d.is_waiting()),
            "le programme doit être suspendu sur son read"
        );
        assert!(
            app.show_program_output,
            "un read en attente doit ouvrir la fenêtre de sortie"
        );
        assert!(app.run_pending.is_some(), "le « continuer » est mis en attente");
        // Pendant l'attente, ptrace n'a pas la main : ni pas à pas, ni écriture
        // de registre. Les boutons et l'édition s'appuient là-dessus pour se
        // griser, plutôt que de proposer des gestes qui échoueraient.
        assert!(
            !app.can_step(),
            "un programme suspendu dans un appel système n'avance pas d'un pas"
        );

        // Des frames passent sans que rien ne bouge : l'UI n'est pas bloquée.
        for _ in 0..3 {
            app.poll_debugger(&ctx);
        }
        assert!(app.dbg.as_ref().is_some_and(|d| d.is_waiting()));

        app.stdin_input = "coucou".to_string();
        app.send_stdin();
        assert!(
            app.show_program_output,
            "l'écho de la saisie doit maintenir la sortie visible"
        );
        // Le déblocage prend quelques frames (le syscall doit aboutir).
        for _ in 0..200 {
            app.poll_debugger(&ctx);
            if app.dbg.as_ref().is_none_or(|d| !d.is_alive()) {
                break;
            }
        }
        assert!(
            matches!(app.dbg.as_ref().map(|d| d.state), Some(RunState::Exited(0))),
            "le « continuer » doit avoir repris tout seul et mené le programme à sa fin"
        );
        assert!(app.run_pending.is_none(), "plus rien en attente");
        assert!(
            app.console.contains("coucou"),
            "le programme réécrit ce qu'il a lu : {}",
            app.console
        );
    }

    /// Mesure ponctuelle, hors suite ordinaire (`cargo test --release -- \
    /// --ignored bench`) : 40 000 instructions enchaînées, pour vérifier que le
    /// pas reste à coût constant et que « Continuer » ne s'effondre pas sur une
    /// boucle un peu longue.
    #[test]
    #[ignore = "mesure de performance, pas une vérification"]
    fn bench_forty_thousand_steps() {
        let mut app = App::new();
        app.src_path = PathBuf::from("build/dbgops-bench.asm");
        app.out_dir = PathBuf::from("build/dbgops-bench");
        app.source = std::fs::read_to_string("examples/bench-loop.asm").expect("exemple lisible");
        app.launch();

        let t0 = std::time::Instant::now();
        app.cont();
        let dt = t0.elapsed();
        let steps = app.dbg.as_ref().expect("dbg").steps();
        println!(
            "{steps} instructions en {dt:?} — {:.1} µs/instruction",
            dt.as_secs_f64() * 1e6 / steps as f64
        );
    }

    /// La trace incrémentale doit donner exactement le même résultat que la
    /// reconstruction complète — c'est tout l'enjeu du dépouillement par
    /// morceaux.
    #[test]
    fn the_incremental_trace_matches_a_full_rebuild() {
        let mut app = app_with(
            "trace",
            "section .text\n\
             global _start\n\
             _start:\n\
             call twice\n\
             mov rax,60\n\
             xor rdi,rdi\n\
             syscall\n\
             twice:\n\
             inc rbx\n\
             ret\n",
        );
        for _ in 0..8 {
            app.step();
        }
        let (calls, syscalls) = (
            app.call_stack.clone(),
            app.syscalls.iter().map(|s| s.number).collect::<Vec<_>>(),
        );
        assert!(!syscalls.is_empty(), "le programme fait au moins un appel système");

        app.rebuild_trace();
        assert_eq!(app.call_stack, calls, "même pile d'appels");
        assert_eq!(
            app.syscalls.iter().map(|s| s.number).collect::<Vec<_>>(),
            syscalls,
            "mêmes appels système, sans doublon ni oubli"
        );
    }

    /// Le chemin complet tel que l'élève le vit : cible Windows, F5, et la
    /// sortie du programme arrive dans la console de l'IDE, suivie de son code
    /// de sortie. Sans débogueur derrière — c'est ce que la cible promet, et
    /// c'est aussi ce qu'elle ne promet pas.
    #[test]
    fn the_ide_runs_a_windows_program_and_shows_its_output() {
        if !crate::winerun::available() {
            eprintln!("wine absent : exécution depuis l'IDE non vérifiée");
            return;
        }
        let dir = std::path::PathBuf::from("build/wine-ide");
        std::fs::create_dir_all(&dir).expect("dossier");
        let mut app = App::new();
        app.target = crate::assemble::Target::Windows;
        app.src_path = dir.join("prog.asm");
        app.out_dir = dir.clone();
        app.source = std::fs::read_to_string("examples/hello-windows.asm").expect("exemple Windows");

        let ctx = eframe::egui::Context::default();
        app.launch();
        assert!(app.wine.is_some(), "le programme doit avoir été lancé: {}", app.console);
        assert!(app.dbg.is_none(), "pas de débogueur pour un PE : Wine exécute, il ne déroule pas");

        // Sonder comme le fait la boucle de frame, jusqu'à la fin du programme.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while app.wine.is_some() {
            app.poll_wine(&ctx);
            assert!(std::time::Instant::now() < deadline, "le programme ne finit pas");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            app.program_output.contains("Bonjour depuis un PE64"),
            "sortie du programme: {:?}",
            app.program_output
        );
        assert!(
            app.console.contains("code de sortie 0"),
            "la console doit rapporter le code de sortie: {}",
            app.console
        );
    }
}
