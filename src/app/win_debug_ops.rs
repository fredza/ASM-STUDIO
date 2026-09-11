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
//!
//! # Pourquoi un thread plutôt qu'un appel direct
//!
//! Le protocole RSP est synchrone : `cont()` n'a rien à rendre tant que le
//! débogué ne s'est pas arrêté à nouveau. Appelé depuis le fil d'egui, comme
//! c'était le cas au début, un programme qui ouvre une `MessageBoxA` (boîte
//! modale qui attend un clic sur OK) figeait **tout** l'IDE — pas seulement
//! cette fenêtre — jusqu'au délai interne de vingt secondes du protocole. Le
//! système d'exploitation finissait par afficher « ASM Studio ne répond pas ».
//!
//! Le tuyau non bloquant sondé par frame de [`crate::winerun`] ne s'applique
//! pas ici : RSP n'est pas un flux à lire au fil de l'eau mais un échange
//! requête/réponse, et le réécrire en machine à états incrémentale serait un
//! chantier sans rapport avec le besoin. La session confie donc le
//! `WinDebugger` à un thread qui le **possède** pour toute sa durée
//! ([`WinDebugSession`]), et l'interface ne fait plus que poster des commandes
//! et sonder les réponses, une fois par frame — comme `poll_wine` et
//! `poll_debugger` le font déjà pour d'autres flux.
//!
//! L'annulation est le point délicat : abandonner le thread en arrière-plan
//! laisserait `winedbg` et le programme débogué tourner indéfiniment, invisibles.
//! « Arrêter » tue donc réellement `winedbg` depuis le fil de l'interface
//! ([`crate::win_debugger::WinDbgInterrupt`]) ; la connexion se ferme, la
//! lecture bloquée du thread échoue, il relâche sa session et se termine — et
//! le `join` qui suit ne dure que le temps de ce réveil.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;

use eframe::egui;

use crate::debugger::{Flags, Registers};
use crate::i18n::{self, Lang};
use crate::win_debugger::{WinDbgError, WinDbgInterrupt, WinDbgResult, WinDebugger, WinRunState};

use super::App;
use super::debug_ops::{StopMap, stops_here};

/// Budget de reprises par appel à « Continuer ». Chaque itération est un
/// aller-retour réseau localhost (RSP) vers `winedbg`, pas un `ptrace` local :
/// un budget bien plus modeste que celui du débogueur natif (`RUN_BUDGET`,
/// 100 000) suffit. Il ne protège plus l'interface — le thread de session s'en
/// charge — mais il garde « Continuer » borné, pour que l'élève reçoive un
/// compte rendu plutôt qu'une attente muette sur une boucle qui n'atteint
/// jamais son point d'arrêt.
const WIN_RUN_BUDGET: usize = 2_000;

/// Ce que l'interface demande au thread de session.
enum WinCmd {
    /// Une instruction machine.
    Step,
    /// Reprendre jusqu'au prochain arrêt demandé. Les points d'arrêt voyagent
    /// avec la commande : l'élève a pu en poser depuis la précédente, et le
    /// thread n'a aucun accès à `App` pour aller les relire.
    Cont(StopMap),
    /// Fin de session demandée : le thread sort de sa boucle et relâche le
    /// `WinDebugger` (dont le `Drop` tue `winedbg`).
    Stop,
}

/// Ce que le thread de session rapporte à l'interface.
enum WinEvent {
    /// Le débogué s'est (re)arrêté — ou terminé : voici son état complet.
    Stopped(Box<WinSnapshot>),
    /// La commande a échoué ; message déjà traduit, prêt à journaliser.
    Failed(String),
    /// Le délai d'attente a expiré sans réponse : la session se referme, mais
    /// ce n'est pas une panne de l'IDE — le plus souvent, le programme
    /// attendait un geste que personne ne lui a donné. Distingué de
    /// [`WinEvent::Failed`] pour que la barre d'état le dise en ces termes,
    /// plutôt que d'annoncer un échec sur ce qui est presque toujours un
    /// malentendu (voir [`crate::win_debugger::WinDbgError::Timeout`]).
    GaveUp(String),
}

/// L'état du débogué recopié à chaque arrêt.
///
/// L'interface peint ceci, jamais le `WinDebugger` lui-même : celui-ci vit
/// sur le thread de session, et le lire depuis egui redemanderait exactement
/// le verrou (ou l'attente) qu'on cherche à supprimer.
#[derive(Clone)]
pub(crate) struct WinSnapshot {
    pub state: WinRunState,
    pub regs: Registers,
    pub flags: Flags,
    /// Combien de `0xCC` portent réellement en mémoire à cet instant — ce qui
    /// peut différer un moment de ce que l'éditeur affiche, si un point a été
    /// posé après la dernière commande.
    pub breakpoints: usize,
    /// Vrai quand « Continuer » a épuisé [`WIN_RUN_BUDGET`] sans atteindre
    /// l'arrêt demandé : le programme tourne toujours, il faut relancer.
    pub budget_exhausted: bool,
}

/// Une session de pas-à-pas Windows : le thread qui possède le débogueur,
/// les deux canaux qui lui parlent, et le dernier état connu du débogué.
pub(crate) struct WinDebugSession {
    /// Sous `Option` uniquement pour pouvoir raccrocher explicitement dans
    /// `Drop` : un champ ordinaire ne serait relâché qu'*après* le corps du
    /// `drop`, et le `join` qui s'y trouve attendrait alors un thread encore
    /// bloqué sur `recv()`.
    cmd: Option<Sender<WinCmd>>,
    evt: Receiver<WinEvent>,
    join: Option<JoinHandle<()>>,
    interrupt: WinDbgInterrupt,
    /// `None` tant que le lancement de `winedbg` n'a pas abouti.
    snap: Option<WinSnapshot>,
    /// Une commande est partie et sa réponse n'est pas revenue : les boutons
    /// « Suivant »/« Continuer » se grisent, « Arrêter » reste actif.
    busy: bool,
}

impl WinDebugSession {
    /// Démarre le thread. Rend la main immédiatement : même le lancement de
    /// `winedbg` (plusieurs secondes la première fois que Wine crée son
    /// préfixe) se fait là-bas.
    fn start(exe: PathBuf, stops: StopMap, lang: Lang) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (evt_tx, evt_rx) = mpsc::channel();
        let interrupt = WinDbgInterrupt::new();
        let thread_interrupt = interrupt.clone();
        let join = std::thread::spawn(move || {
            session_thread(exe, stops, lang, thread_interrupt, cmd_rx, evt_tx);
        });
        WinDebugSession {
            cmd: Some(cmd_tx),
            evt: evt_rx,
            join: Some(join),
            interrupt,
            snap: None,
            // Le lancement est lui-même la première commande en vol.
            busy: true,
        }
    }

    /// Poste une commande, sauf si une autre est déjà en cours (sans quoi les
    /// clics s'empileraient dans le canal et s'exécuteraient tous d'affilée,
    /// bien après que l'élève ait cessé d'y penser).
    fn send(&mut self, cmd: WinCmd) -> bool {
        if self.busy {
            return false;
        }
        let Some(tx) = self.cmd.as_ref() else { return false };
        if tx.send(cmd).is_err() {
            return false;
        }
        self.busy = true;
        true
    }

    /// Relève ce que le thread a rapporté depuis la frame précédente, sans
    /// jamais attendre. C'est le seul point de contact, et il est borné par
    /// un `try_recv`.
    fn poll(&mut self) -> Option<WinEvent> {
        match self.evt.try_recv() {
            Ok(WinEvent::Stopped(snap)) => {
                self.busy = false;
                self.snap = Some((*snap).clone());
                Some(WinEvent::Stopped(snap))
            }
            Ok(other) => {
                self.busy = false;
                Some(other)
            }
            // Déconnecté : le thread s'est arrêté sans rien dire (panique, ou
            // fin normale après un `Stop`). Plus rien à attendre de lui.
            Err(TryRecvError::Disconnected) => {
                self.busy = false;
                None
            }
            Err(TryRecvError::Empty) => None,
        }
    }

    /// Une commande est-elle en vol ?
    pub(super) fn is_busy(&self) -> bool {
        self.busy
    }

    /// La session démarre encore : `winedbg` n'a pas rendu son premier état.
    pub(super) fn is_starting(&self) -> bool {
        self.snap.is_none()
    }

    /// Dernier état connu du débogué, ou `None` pendant le démarrage.
    pub(super) fn snapshot(&self) -> Option<&WinSnapshot> {
        self.snap.as_ref()
    }

    /// Le débogué est-il toujours là ? Vrai pendant le démarrage : la session
    /// existe et travaille, la traiter comme finie ferait clignoter les
    /// boutons de la barre d'outils pendant que Wine se met en route.
    pub(super) fn is_alive(&self) -> bool {
        match &self.snap {
            Some(s) => s.state == WinRunState::Stopped,
            None => true,
        }
    }

    /// Prête à recevoir « Suivant » ou « Continuer » : arrêtée, et libre.
    pub(super) fn is_ready(&self) -> bool {
        self.is_alive() && !self.busy
    }
}

impl Drop for WinDebugSession {
    fn drop(&mut self) {
        // Trois gestes, dans cet ordre, chacun pour un cas distinct :
        //   1. `Stop` réveille un thread au repos sur `recv()` ;
        //   2. raccrocher le canal fait la même chose si l'envoi a échoué ;
        //   3. l'interruption tue `winedbg`, seule façon de sortir un thread
        //      bloqué au milieu d'une lecture RSP (une modale attend un clic).
        // Le `join` qui suit ne dure alors que le temps du réveil, et garantit
        // qu'aucun thread ne survit à la session — ni le `winedbg` qu'il tient.
        if let Some(tx) = self.cmd.take() {
            let _ = tx.send(WinCmd::Stop);
        }
        self.interrupt.interrupt();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Ce que l'interface a besoin de savoir, lu d'un coup pendant que le
/// débogueur est encore à portée du thread qui le possède.
fn snapshot(dbg: &WinDebugger, budget_exhausted: bool) -> WinSnapshot {
    WinSnapshot {
        state: dbg.state,
        regs: dbg.regs().clone(),
        flags: dbg.flags(),
        breakpoints: dbg.breakpoints().count(),
        budget_exhausted,
    }
}

/// Reprend l'exécution jusqu'à un arrêt demandé, la fin du programme, ou
/// l'épuisement du budget (rendu vrai dans ce dernier cas).
///
/// Les points d'arrêt sont reposés à chaque tour : un point ajouté depuis le
/// dernier passage n'a encore aucun `0xCC` réel en mémoire, et `set_breakpoint`
/// est sans effet sur une adresse déjà armée — reposer les autres ne coûte rien.
fn run_until(dbg: &mut WinDebugger, stops: &StopMap) -> WinDbgResult<bool> {
    let mut used = 0usize;
    loop {
        for addr in stops.keys() {
            let _ = dbg.set_breakpoint(*addr);
        }
        dbg.cont()?;
        used += 1;
        if !dbg.is_alive() || stops_here(stops, dbg.regs()) {
            return Ok(false);
        }
        if used >= WIN_RUN_BUDGET {
            return Ok(true);
        }
    }
}

/// Le corps du thread de session : il possède le `WinDebugger` du premier
/// `launch` jusqu'à son propre retour, et rien d'autre ne le touche.
fn session_thread(
    exe: PathBuf,
    initial_stops: StopMap,
    lang: Lang,
    interrupt: WinDbgInterrupt,
    cmds: Receiver<WinCmd>,
    evts: Sender<WinEvent>,
) {
    let mut dbg = match WinDebugger::launch_interruptible(&exe, &interrupt) {
        Ok(dbg) => dbg,
        Err(e) => {
            // Coupé pendant le démarrage : personne n'écoute plus, et ce
            // n'est de toute façon pas une panne à raconter.
            if !interrupt.is_cancelled() {
                let _ = evts.send(WinEvent::Failed(e.message(lang)));
            }
            return;
        }
    };
    for addr in initial_stops.keys() {
        let _ = dbg.set_breakpoint(*addr);
    }
    if evts.send(WinEvent::Stopped(Box::new(snapshot(&dbg, false)))).is_err() {
        return;
    }

    while let Ok(cmd) = cmds.recv() {
        let outcome = match cmd {
            WinCmd::Step => dbg.step().map(|()| false),
            WinCmd::Cont(stops) => run_until(&mut dbg, &stops),
            WinCmd::Stop => break,
        };
        match outcome {
            Ok(budget_exhausted) => {
                let snap = Box::new(snapshot(&dbg, budget_exhausted));
                if evts.send(WinEvent::Stopped(snap)).is_err() {
                    break;
                }
            }
            Err(e) => {
                // Une lecture qui échoue parce qu'on vient soi-même de tuer
                // `winedbg` n'est pas une nouvelle : c'est « Arrêter » qui a
                // abouti. L'annoncer ferait apparaître une erreur rouge sur
                // un geste délibéré de l'élève.
                if !interrupt.is_cancelled() {
                    let msg = e.message(lang);
                    let _ = evts.send(match e {
                        WinDbgError::Timeout => WinEvent::GaveUp(msg),
                        _ => WinEvent::Failed(msg),
                    });
                }
                break;
            }
        }
    }
    // `dbg` tombe ici : son `Drop` tue `winedbg` et, avec lui, le débogué.
}

impl App {
    /// Wine et `winedbg` sont-ils utilisables pour ce pas-à-pas ?
    pub(super) fn win_debug_available(&self) -> bool {
        WinDebugger::available()
    }

    /// Vrai si « Suivant »/« Continuer » ont un effet sur la cible courante,
    /// natif ou expérimental confondus — c'est ce qui décide si les boutons
    /// de la barre d'outils principale sont cliquables. Une session occupée
    /// les grise : la commande suivante n'aurait nulle part où aller.
    pub(super) fn can_step_any(&self) -> bool {
        self.can_step()
            || (self.target.is_windows()
                && self.win_debug_available()
                && self.win_dbg.as_ref().is_none_or(|d| d.is_ready()))
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
    ///
    /// Une session tout juste démarrée est encore en train de lancer Wine :
    /// « Continuer » ne peut pas la doubler, et la commande est simplement
    /// ignorée (l'élève la relance quand les boutons redeviennent actifs).
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
            return;
        }
        self.win_debug_cont();
    }

    /// Démarre une session : assemble si besoin, puis confie à un thread le
    /// lancement de `winedbg --gdb` et la pose d'un point d'arrêt sur chaque
    /// ligne déjà marquée par l'élève. Sans effet si une session tourne déjà,
    /// ou si la cible n'est pas Windows.
    ///
    /// Ne bloque pas : le premier état arrive par [`Self::poll_win_debug`],
    /// quelques frames (ou quelques secondes) plus tard.
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
        self.status = i18n::tr3(
            lang,
            "Démarrage du pas-à-pas Windows (Wine)…",
            "Starting Windows step debugging (Wine)…",
            "Iniciando el paso a paso Windows (Wine)…",
        )
        .to_string();
        self.win_dbg = Some(WinDebugSession::start(bin, stops, lang));
    }

    /// Une instruction machine. Sans effet si une commande est déjà en vol.
    pub(super) fn win_debug_step(&mut self) {
        let lang = self.lang;
        if self.win_dbg.as_mut().is_some_and(|s| s.send(WinCmd::Step)) {
            self.status = working_status(lang);
        }
    }

    /// Exécute jusqu'au prochain point d'arrêt de l'élève (condition
    /// comprise) ou la fin du programme. Sans effet si une commande est déjà
    /// en vol.
    pub(super) fn win_debug_cont(&mut self) {
        if self.win_dbg.is_none() {
            return;
        }
        let stops = self.stop_addresses(None);
        let lang = self.lang;
        if self.win_dbg.as_mut().is_some_and(|s| s.send(WinCmd::Cont(stops))) {
            self.status = working_status(lang);
        }
    }

    /// Referme la session (voir `stop()`, qui la relâche déjà — passer par
    /// lui plutôt que par un simple `self.win_dbg = None` garde un seul
    /// message de statut et une seule coupure pour « Arrêter », quelle que
    /// soit la cible en cours).
    ///
    /// Reste cliquable même pendant une commande : c'est précisément le cas
    /// où l'élève en a besoin, et c'est le `Drop` de [`WinDebugSession`] qui
    /// fait le travail — tuer `winedbg`, débloquer le thread, le joindre.
    pub(super) fn win_debug_stop(&mut self) {
        self.stop();
    }

    /// Sonde le thread de session à chaque frame : nouvel état, ou panne.
    ///
    /// C'est le pendant de `poll_debugger`/`poll_wine` pour ce chemin-là.
    /// Tant qu'une commande est en vol, redemande une frame : rien d'autre ne
    /// réveillerait l'interface quand la réponse arrivera, et l'indicateur
    /// « En cours… » resterait figé.
    pub(super) fn poll_win_debug(&mut self, ctx: &egui::Context) {
        // Relevé avant le sondage : c'est lui qui pose le premier état, et
        // « la session vient de démarrer » ne se lit plus après coup.
        let starting = match self.win_dbg.as_ref() {
            Some(session) => session.is_starting(),
            None => return,
        };
        let event = match self.win_dbg.as_mut() {
            Some(session) => session.poll(),
            None => return,
        };
        match event {
            Some(WinEvent::Stopped(snap)) => self.finish_win_debug_step(&snap, starting),
            Some(WinEvent::Failed(msg)) => {
                let lang = self.lang;
                self.log(&msg);
                self.status = i18n::tr3(
                    lang,
                    "Échec du pas-à-pas Windows (Wine)",
                    "Windows step debugging failed (Wine)",
                    "Fallo del paso a paso Windows (Wine)",
                )
                .to_string();
                // Le thread s'est déjà retiré après avoir rapporté sa panne :
                // relâcher la session ne fait plus que joindre un thread fini.
                self.win_dbg = None;
            }
            Some(WinEvent::GaveUp(msg)) => {
                let lang = self.lang;
                self.log(&msg);
                self.status = i18n::tr3(
                    lang,
                    "Pas-à-pas Windows : session refermée après une trop longue attente",
                    "Windows step debugging: session closed after waiting too long",
                    "Paso a paso Windows: sesión cerrada tras una espera demasiado larga",
                )
                .to_string();
                // Même geste que « Arrêter » : le `Drop` tue `winedbg` et
                // joint le thread. Rien de Wine ne survit à cette ligne.
                self.win_dbg = None;
            }
            None => {}
        }
        if self.win_dbg.as_ref().is_some_and(|s| s.is_busy()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
    }

    /// Traduit en barre d'état ce que le thread vient de rapporter.
    /// `starting` distingue le tout premier état — celui qui annonce que la
    /// session est en place — d'un arrêt ordinaire au fil du pas-à-pas.
    fn finish_win_debug_step(&mut self, snap: &WinSnapshot, starting: bool) {
        let lang = self.lang;
        if starting && snap.state == WinRunState::Stopped {
            self.status = format!(
                "{} 0x{:X}",
                i18n::tr3(
                    lang,
                    "Pas-à-pas Windows démarré — RIP @",
                    "Windows step debugging started — RIP @",
                    "Paso a paso Windows iniciado — RIP @",
                ),
                snap.regs.rip
            );
            return;
        }
        if snap.budget_exhausted {
            self.status = format!(
                "{WIN_RUN_BUDGET} {}",
                i18n::tr3(
                    lang,
                    "reprises sans atteindre l'arrêt, toujours en cours — relancez « Continuer »",
                    "resumes without reaching the breakpoint, still going — hit “Continue” again",
                    "reanudaciones sin alcanzar la parada, sigue en curso — pulse «Continuar» de nuevo",
                )
            );
            return;
        }
        match snap.state {
            WinRunState::Stopped => {
                self.status = format!(
                    "{} — RIP @ 0x{:X}",
                    i18n::tr3(lang, "Arrêté", "Stopped", "Detenido"),
                    snap.regs.rip
                );
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

/// Le message de barre d'état pendant qu'une commande tourne — le même que
/// l'indicateur de la fenêtre dédiée, pour que l'élève qui a les yeux sur
/// l'éditeur voie lui aussi que quelque chose se passe.
fn working_status(lang: Lang) -> String {
    i18n::tr3(
        lang,
        "Pas-à-pas Windows : commande en cours…",
        "Windows step debugging: command running…",
        "Paso a paso Windows: comando en curso…",
    )
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble::Target;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

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

    /// Un programme qui ne s'arrête jamais tout seul : la version automatisable
    /// du `MessageBoxA` qui a révélé le gel. Une modale Windows attend un clic
    /// qu'aucun test ne peut donner ; cette boucle attend, elle, quelque chose
    /// qui n'arrivera jamais non plus — mais sans dépendre d'un humain. Dans
    /// les deux cas, `cont()` ne rend pas la main, ce qui est tout ce que le
    /// correctif doit encaisser.
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

    fn app_with(tag: &str, source: &str) -> App {
        let mut app = App::new();
        app.src_path = PathBuf::from(format!("build/windbgops-{tag}.asm"));
        app.out_dir = PathBuf::from(format!("build/windbgops-{tag}"));
        app.source = source.to_string();
        app.set_target(Target::Windows);
        app
    }

    /// Sonde jusqu'à ce que la session soit libre (démarrage ou commande
    /// terminés), comme le ferait la boucle de frames d'egui. Rend faux si le
    /// délai s'épuise, ou si la session a disparu (panne rapportée).
    fn wait_ready(app: &mut App, ctx: &egui::Context, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            app.poll_win_debug(ctx);
            match app.win_dbg.as_ref() {
                None => return false,
                Some(s) if !s.is_busy() => return true,
                Some(_) => {}
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Démarre une session et attend son premier état, comme la fenêtre le
    /// ferait au fil des frames.
    fn start_and_wait(app: &mut App, ctx: &egui::Context) -> bool {
        app.win_debug_start();
        wait_ready(app, ctx, Duration::from_secs(60))
    }

    /// Titre de la boîte du test : distinctif, pour que `xdotool` ne puisse
    /// tomber que sur elle.
    const BOITE_TITRE: &str = "TitreTestMessageBox";

    /// Combien de temps la boîte reste à l'écran avant qu'on y réponde.
    ///
    /// Plus long que les vingt secondes de `REQUEST_TIMEOUT`, et c'est tout
    /// l'intérêt : c'est exactement là que la session mourait.
    const ATTENTE_ELEVE: Duration = Duration::from_secs(25);

    /// Les fenêtres *visibles* qui portent le titre de la boîte — toutes.
    ///
    /// Wine en crée deux pour un seul dialogue (une technique autour de la
    /// vraie), du même titre, et `xdotool search` ne dit pas laquelle recevra
    /// la touche : donner le focus à la mauvaise réussit, `Return` part
    /// dedans, et la boîte reste à l'écran. Ce test a cassé exactement ainsi
    /// le jour où le « bureau virtuel » Wine — qui, en emboîtant tout dans un
    /// conteneur, mettait la bonne en premier — a été désactivé. D'où la
    /// liste entière plutôt que la première ligne ; `--onlyvisible` n'écarte
    /// que celles qu'aucun clic ne pourrait atteindre de toute façon.
    fn boites_visibles() -> Vec<String> {
        let Ok(out) = std::process::Command::new("xdotool")
            .args(["search", "--onlyvisible", "--name", BOITE_TITRE])
            .output()
        else {
            return Vec::new();
        };
        if !out.status.success() {
            return Vec::new();
        }
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// La boîte est-elle à l'écran ?
    fn boite_a_l_ecran() -> Option<String> {
        boites_visibles().into_iter().next()
    }

    /// Répond à la boîte comme le ferait l'élève qui la referme par la croix
    /// de sa fenêtre : `WM_DELETE_WINDOW` (`xdotool windowquit`), et
    /// `MessageBoxA` rend la main.
    ///
    /// Pas une frappe clavier. Une touche envoyée par XTEST va à la fenêtre
    /// qui a le focus, et sous GNOME/Wayland ce focus appartient au
    /// compositeur, pas à `xdotool windowfocus` : il suffisait que quelqu'un
    /// tape dans un terminal à côté pour que le `Return` y parte — le test
    /// passait ou cassait selon ce que l'utilisateur faisait de ses mains
    /// pendant ce temps (vérifié : même code, même machine, résultats
    /// opposés). La demande de fermeture, elle, est adressée à la fenêtre
    /// elle-même, quel que soit le focus. Un vrai geste d'utilisateur tout
    /// autant ; vérifié sur le binaire seul : le programme termine en 0.
    ///
    /// La liste est relue à chaque tentative, et chaque fenêtre visible reçoit
    /// la demande : Wine en crée deux pour un dialogue, la seconde disparaît
    /// dès que la première se ferme, et un échec sur celle-là n'est pas une
    /// nouvelle.
    fn repondre_a_la_boite() -> bool {
        for _ in 0..10 {
            for id in boites_visibles() {
                let _ = std::process::Command::new("xdotool")
                    .args(["windowquit", &id])
                    .stderr(std::process::Stdio::null())
                    .status();
            }
            std::thread::sleep(Duration::from_millis(500));
            if boite_a_l_ecran().is_none() {
                return true;
            }
        }
        false
    }

    /// Le geste réel d'un élève : son programme ouvre une `MessageBoxA`,
    /// « Continuer » attend dessus, et il prend son temps avant de répondre.
    /// Pas une simulation — une vraie demande de fermeture (`xdotool
    /// windowquit`) adressée à la vraie fenêtre Wine, comme la croix de son
    /// cadre, indépendante du focus clavier (voir `repondre_a_la_boite`).
    ///
    /// Ce que ce test verrouille, dans l'ordre où ça a cassé :
    ///
    ///   * la boîte reste à l'écran vingt-cinq secondes — plus que les vingt
    ///     du délai RSP d'origine, qui refermait alors la session sur un
    ///     « Resource temporarily unavailable (os error 11) » alors que le
    ///     programme se portait très bien et n'attendait qu'un clic (voir
    ///     `CONT_TIMEOUT` dans `crate::win_debugger`) ;
    ///   * une fois la boîte refermée, « Continuer » rend la main et le
    ///     programme va jusqu'à son `ExitProcess` ;
    ///   * la session refermée ne laisse aucun processus Wine derrière elle.
    #[test]
    fn clicking_ok_on_a_real_messagebox_does_not_wedge_winedbg() {
        if !WinDebugger::available() {
            eprintln!("wine absent : non vérifié");
            return;
        }
        // Pas `xdotool --version` : il réussit sans écran, et le runner CI de
        // ce dépôt tourne en service sur une machine où il y a bien un
        // xdotool, mais aucun `DISPLAY` — le clic n'aurait alors personne à
        // atteindre, et la boîte ne serait « jamais trouvée ». Interroger
        // l'écran lui-même tranche les deux cas d'un coup.
        let display = std::process::Command::new("xdotool")
            .arg("getdisplaygeometry")
            .output()
            .is_ok_and(|o| o.status.success());
        if !display {
            eprintln!("xdotool absent ou aucun affichage joignable : clic réel non vérifié");
            return;
        }
        // Seul test à piloter une vraie fenêtre : il lui faut l'écran, et
        // wineserver, pour lui tout seul (voir `WINE_TESTS`).
        let _wine = crate::win_debugger::exclusive_wine_test();
        assert!(
            boite_a_l_ecran().is_none(),
            "une fenêtre « {BOITE_TITRE} » traîne déjà : le test ne saurait pas laquelle il vise"
        );
        let source = format!(
            r#"
            bits 64
            default rel
            section .data
                texte db "Cliquez sur OK pour continuer.", 0
                titre db "{BOITE_TITRE}", 0
            section .text
                global main
                extern MessageBoxA
                extern ExitProcess
            main:
                sub     rsp, 40
                xor     rcx, rcx
                lea     rdx, [texte]
                lea     r8, [titre]
                xor     r9d, r9d
                call    MessageBoxA
                xor     ecx, ecx
                call    ExitProcess
            "#
        );
        let ctx = egui::Context::default();
        let mut app = app_with("messagebox-click", &source);
        assert!(start_and_wait(&mut app, &ctx), "la session doit démarrer");

        // Depuis un fil à part : rien d'autre ne peut répondre à la boîte
        // pendant que le thread principal sonde `poll_win_debug` en boucle,
        // exactement comme la boucle de frames de l'interface le ferait.
        // Large : pendant un `cargo test` complet, plusieurs sessions Wine
        // tournent en parallèle et la boîte peut mettre longtemps à s'afficher.
        let clicker = std::thread::spawn(|| {
            let deadline = Instant::now() + Duration::from_secs(90);
            loop {
                if boite_a_l_ecran().is_some() {
                    break;
                }
                if Instant::now() >= deadline {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            std::thread::sleep(ATTENTE_ELEVE);
            Some(repondre_a_la_boite())
        });

        app.win_debug_cont();
        let responded = wait_ready(&mut app, &ctx, Duration::from_secs(180));
        let clicked = clicker.join().unwrap_or(None);
        assert_eq!(
            clicked,
            Some(true),
            "la vraie boîte n'a pas pu être vue puis refermée par xdotool : \
             sans ça le reste du test ne démontrerait rien (état : {:?})",
            app.status
        );
        assert!(
            responded,
            "« Continuer » n'a pas repris la main après {ATTENTE_ELEVE:?} de boîte à l'écran \
             puis un clic réel sur OK"
        );
        let snap = app
            .win_dbg
            .as_ref()
            .expect("la session doit avoir survécu à l'attente")
            .snapshot()
            .expect("un état après le clic");
        assert_eq!(snap.state, WinRunState::Exited(0), "sortie attendue après ExitProcess");

        // Et rien ne survit à la fermeture — ni `winedbg`, ni le débogué.
        app.win_debug_stop();
        assert!(app.win_dbg.is_none());
        let restants = std::process::Command::new("pgrep")
            .args(["-f", "windbgops-messagebox-click.exe"])
            .output()
            .expect("pgrep");
        assert!(
            restants.stdout.is_empty(),
            "processus Wine encore vivants après la fin de la session : {}",
            String::from_utf8_lossy(&restants.stdout)
        );
    }

    /// Vérification ponctuelle : la vraie file App→session→winedbg, sur
    /// l'exemple livré, du démarrage jusqu'à la sortie, comme le ferait un
    /// élève avec Démarrer puis Suivant×3 puis Continuer.
    #[test]
    fn shipped_win_hello_world_through_the_real_app_session() {
        if !WinDebugger::available() {
            eprintln!("wine absent : non vérifié");
            return;
        }
        let _wine = crate::win_debugger::shared_wine_test();
        let source = std::fs::read_to_string("examples_seed/win_hello_world.asm")
            .expect("l'exemple est livré avec l'IDE");
        let ctx = egui::Context::default();
        let mut app = app_with("shipped-hello-real", &source);
        assert!(start_and_wait(&mut app, &ctx), "la session doit démarrer");
        for _ in 0..3 {
            if app.win_dbg.as_ref().is_none_or(|s| !s.is_alive()) {
                break;
            }
            app.win_debug_step();
            assert!(wait_ready(&mut app, &ctx, Duration::from_secs(30)), "le pas doit répondre");
        }
        if app.win_dbg.as_ref().is_some_and(|s| s.is_alive()) {
            app.win_debug_cont();
            assert!(wait_ready(&mut app, &ctx, Duration::from_secs(30)), "continuer doit répondre");
        }
    }

    #[test]
    fn starting_a_session_stops_before_the_first_instruction() {
        if !WinDebugger::available() {
            eprintln!("wine absent : session Windows non vérifiée");
            return;
        }
        let _wine = crate::win_debugger::shared_wine_test();
        let ctx = egui::Context::default();
        let mut app = app_with("start", HELLO);
        assert!(start_and_wait(&mut app, &ctx), "la session doit démarrer");
        let session = app.win_dbg.as_ref().expect("session");
        assert!(session.is_alive());
        assert!(session.snapshot().is_some(), "un premier état doit être arrivé");
    }

    /// Le démarrage rend la main tout de suite : `win_debug_start` ne lance
    /// plus `winedbg` sur le fil de l'interface.
    #[test]
    fn starting_a_session_returns_immediately() {
        if !WinDebugger::available() {
            eprintln!("wine absent : démarrage non bloquant non vérifié");
            return;
        }
        let _wine = crate::win_debugger::shared_wine_test();
        let mut app = app_with("start-async", HELLO);
        // L'assemblage, lui, reste synchrone : on le fait avant de chronométrer,
        // sinon on mesurerait nasm plutôt que le lancement de Wine.
        app.build();
        let t = Instant::now();
        app.win_debug_start();
        let elapsed = t.elapsed();
        assert!(app.win_dbg.is_some(), "la session doit exister aussitôt");
        assert!(
            app.win_dbg.as_ref().is_some_and(|s| s.is_starting()),
            "et n'avoir encore aucun état : le lancement est parti sur son thread"
        );
        assert!(
            elapsed < Duration::from_millis(500),
            "le démarrage a bloqué l'interface {elapsed:?}"
        );
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
        let _wine = crate::win_debugger::shared_wine_test();
        let ctx = egui::Context::default();
        let mut app = app_with("step", HELLO);
        assert!(start_and_wait(&mut app, &ctx), "session");
        let before = app.win_dbg.as_ref().unwrap().snapshot().unwrap().regs.rip;
        app.win_debug_step();
        assert!(wait_ready(&mut app, &ctx, Duration::from_secs(30)), "pas terminé");
        let after = app.win_dbg.as_ref().unwrap().snapshot().unwrap().regs.rip;
        assert_ne!(before, after, "RIP doit avancer d'un pas");
    }

    #[test]
    fn continuing_without_breakpoints_runs_to_the_end() {
        if !WinDebugger::available() {
            eprintln!("wine absent : fin de programme non vérifiée");
            return;
        }
        let _wine = crate::win_debugger::shared_wine_test();
        let ctx = egui::Context::default();
        let mut app = app_with("cont", HELLO);
        assert!(start_and_wait(&mut app, &ctx), "session");
        app.win_debug_cont();
        assert!(wait_ready(&mut app, &ctx, Duration::from_secs(60)), "fin atteinte");
        let state = app.win_dbg.as_ref().unwrap().snapshot().unwrap().state;
        assert_eq!(state, WinRunState::Exited(7));
    }

    /// Le cœur de la fonctionnalité : un point d'arrêt posé dans l'éditeur
    /// (comme pour la cible Linux) arrête bien « Continuer » côté Windows.
    #[test]
    fn a_breakpoint_set_in_the_editor_stops_continue_there() {
        if !WinDebugger::available() {
            eprintln!("wine absent : point d'arrêt Windows non vérifié");
            return;
        }
        let _wine = crate::win_debugger::shared_wine_test();
        let ctx = egui::Context::default();
        let mut app = app_with("bp", HELLO);
        let line = app
            .source
            .lines()
            .position(|l| l.contains("call") && l.contains("ExitProcess"))
            .map(|i| i + 1)
            .expect("la ligne « call ExitProcess » existe dans HELLO");
        app.toggle_breakpoint(line);

        assert!(start_and_wait(&mut app, &ctx), "session");
        app.win_debug_cont();
        assert!(wait_ready(&mut app, &ctx, Duration::from_secs(60)), "arrêt atteint");

        let session = app.win_dbg.as_ref().expect("session active");
        assert!(session.is_alive(), "arrêté sur le point d'arrêt, pas terminé");
    }

    #[test]
    fn stopping_clears_the_session() {
        if !WinDebugger::available() {
            eprintln!("wine absent : arrêt de session non vérifié");
            return;
        }
        let _wine = crate::win_debugger::shared_wine_test();
        let ctx = egui::Context::default();
        let mut app = app_with("stop", HELLO);
        assert!(start_and_wait(&mut app, &ctx), "session");
        app.win_debug_stop();
        assert!(app.win_dbg.is_none());
    }

    /// La régression qui a motivé tout ce chemin : un « Continuer » qui ne
    /// revient jamais (ici une boucle sans fin, hier une `MessageBoxA` qui
    /// attendait un clic) figeait l'IDE entier jusqu'au délai de vingt
    /// secondes du protocole RSP.
    ///
    /// Le test tient les deux moitiés du correctif :
    ///   * pendant que la commande tourne, chaque sondage — ce que fait la
    ///     boucle de frames d'egui — revient en une poignée de millisecondes ;
    ///   * « Arrêter » interrompt vraiment l'attente, en bien moins que les
    ///     vingt secondes du timeout, et ne laisse aucun thread derrière lui
    ///     (le `join` du `Drop` est compris dans le chrono).
    #[test]
    fn a_never_ending_command_leaves_the_interface_responsive() {
        if !WinDebugger::available() {
            eprintln!("wine absent : non-blocage non vérifié");
            return;
        }
        let _wine = crate::win_debugger::shared_wine_test();
        let ctx = egui::Context::default();
        let mut app = app_with("hang", BOUCLE_SANS_FIN);
        assert!(start_and_wait(&mut app, &ctx), "session");

        // Aucun point d'arrêt : « Continuer » part dans la boucle sans fin et
        // n'a plus aucune raison de rendre la main. C'est exactement l'appel
        // qui gelait l'IDE — il doit maintenant se contenter de poster la
        // commande. Avant le correctif, il ne revenait qu'au bout des vingt
        // secondes du délai RSP.
        let t = Instant::now();
        app.win_debug_cont();
        let posted_in = t.elapsed();
        assert!(
            posted_in < Duration::from_millis(50),
            "« Continuer » a bloqué le fil de l'interface {posted_in:?}"
        );
        assert!(
            app.win_dbg.as_ref().is_some_and(|s| s.is_busy()),
            "la commande doit être partie sur le thread de session"
        );

        // Une seconde de « frames », pendant que la commande tourne toujours.
        for _ in 0..20 {
            let t = Instant::now();
            app.poll_win_debug(&ctx);
            let elapsed = t.elapsed();
            assert!(
                elapsed < Duration::from_millis(50),
                "le sondage a bloqué l'interface {elapsed:?}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            app.win_dbg.as_ref().is_some_and(|s| s.is_busy()),
            "la commande doit encore tourner : sinon le test ne prouve rien"
        );

        // Et « Arrêter » coupe court, sans attendre le délai RSP.
        let t = Instant::now();
        app.win_debug_stop();
        let elapsed = t.elapsed();
        assert!(app.win_dbg.is_none(), "la session doit être refermée");
        assert!(
            elapsed < Duration::from_secs(5),
            "« Arrêter » a mis {elapsed:?} à débloquer la commande en cours"
        );
    }

    /// Deux clics coup sur coup ne s'empilent pas : le second est ignoré tant
    /// que le premier n'a pas répondu, sans quoi l'élève se retrouverait avec
    /// une file de pas qui se déroulent tout seuls après coup.
    #[test]
    fn commands_do_not_pile_up_while_one_is_running() {
        if !WinDebugger::available() {
            eprintln!("wine absent : empilement non vérifié");
            return;
        }
        let _wine = crate::win_debugger::shared_wine_test();
        let ctx = egui::Context::default();
        let mut app = app_with("busy", BOUCLE_SANS_FIN);
        assert!(start_and_wait(&mut app, &ctx), "session");

        app.win_debug_cont();
        let session = app.win_dbg.as_mut().expect("session");
        assert!(session.is_busy());
        assert!(!session.is_ready(), "les boutons doivent être grisés");
        assert!(!session.send(WinCmd::Step), "aucune commande de plus n'est acceptée");
        assert!(!app.can_step_any(), "ni depuis la barre d'outils");
    }
}
