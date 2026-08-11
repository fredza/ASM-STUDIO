//! Exécution d'un `.exe` PE64 par Wine, quand Wine est installé.
//!
//! ASM Studio assemble du PE (voir [`crate::pe_link`]) mais ne peut pas le
//! *déboguer* : le débogueur parle `ptrace` et suit les adresses du binaire
//! qu'il a lui-même produit, alors qu'un programme Windows lancé par Wine
//! démarre derrière un chargeur, dans un espace d'adressage qui n'est plus
//! celui du listing. Le pas-à-pas resterait donc faux.
//!
//! L'exécuter tout court, en revanche, se tient : l'élève écrit son programme,
//! l'assemble et voit ce qu'il affiche, dans la même console que les programmes
//! Linux. C'est ce que fait ce module, et rien de plus — pas de registres, pas
//! de timeline, pas de points d'arrêt. Ce qu'on montre est vrai ; ce qu'on ne
//! peut pas montrer, on ne le simule pas.
//!
//! Le processus est piloté sans jamais bloquer l'interface : tuyaux non
//! bloquants, sondés une fois par frame comme ceux du débogueur. La toute
//! première exécution de Wine crée `~/.wine` et prend plusieurs secondes —
//! raison de plus pour ne rien attendre sur le fil de l'UI.

use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::{Child, Command, Stdio};

/// Wine est-il utilisable ? Vérifié à chaque lancement plutôt que mis en cache :
/// l'installer pendant que l'IDE tourne doit suffire à s'en servir.
pub fn available() -> bool {
    Command::new("wine")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// Un `.exe` en cours d'exécution sous Wine.
pub struct WineRun {
    child: Child,
    /// Octets lus mais pas encore rendus à l'appelant. Une lecture peut couper
    /// un caractère UTF-8 en deux : la queue incomplète attend le reste plutôt
    /// que de sortir en « � ».
    pending: Vec<u8>,
    /// Entrée acceptée par l'UI mais pas encore écrite dans le tuyau Wine.
    /// Le tuyau est non bloquant : conserver ces octets évite de perdre une
    /// ligne lorsque le programme n'est pas encore prêt à la lire.
    stdin_pending: Vec<u8>,
    /// Code de sortie, une fois le processus terminé.
    exit: Option<i32>,
}

impl WineRun {
    /// Lance `exe` sous Wine, tuyaux branchés.
    pub fn spawn(exe: &Path) -> Result<Self, String> {
        let child = Command::new("wine")
            .arg(exe)
            // Wine bavarde sur stderr (« fixme: … ») à la moindre occasion. Ces
            // lignes ne viennent pas du programme de l'élève et n'ont rien à
            // faire dans sa console.
            .env("WINEDEBUG", "-all")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("impossible de lancer wine: {e}"))?;

        // Les trois extrémités passent en non bloquant : l'UI est mono-thread,
        // et une lecture bloquante sur un programme qui n'écrit rien la figerait.
        for fd in [
            child.stdout.as_ref().map(|p| p.as_raw_fd()),
            child.stderr.as_ref().map(|p| p.as_raw_fd()),
            child.stdin.as_ref().map(|p| p.as_raw_fd()),
        ]
        .into_iter()
        .flatten()
        {
            set_nonblocking(fd);
        }
        Ok(WineRun {
            child,
            pending: Vec::new(),
            stdin_pending: Vec::new(),
            exit: None,
        })
    }

    /// Récupère ce que le programme a écrit depuis le dernier appel (sortie
    /// standard et sortie d'erreur mêlées, comme dans un terminal).
    pub fn take_output(&mut self) -> String {
        let mut buf = [0u8; 8192];
        for fd in [
            self.child.stdout.as_ref().map(|p| p.as_raw_fd()),
            self.child.stderr.as_ref().map(|p| p.as_raw_fd()),
        ]
        .into_iter()
        .flatten()
        {
            loop {
                let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
                if n <= 0 {
                    break; // rien à lire (EAGAIN), ou fin du tuyau
                }
                self.pending.extend_from_slice(&buf[..n as usize]);
            }
        }
        if self.pending.is_empty() {
            return String::new();
        }
        // Ne rendre que ce qui forme des caractères complets ; le reste attend
        // la frame suivante.
        match std::str::from_utf8(&self.pending) {
            Ok(_) => String::from_utf8(std::mem::take(&mut self.pending)).expect("UTF-8 validé"),
            Err(e) => {
                let valid = e.valid_up_to();
                let rest = self.pending.split_off(valid);
                let out =
                    String::from_utf8(std::mem::take(&mut self.pending)).expect("UTF-8 validé");
                self.pending = rest;
                out
            }
        }
    }

    /// Envoie une ligne à l'entrée standard du programme.
    pub fn write_stdin(&mut self, s: &str) -> Result<(), String> {
        self.stdin_pending.extend_from_slice(s.as_bytes());
        self.flush_stdin()
    }

    /// Vide autant que possible l'entrée en attente, sans attendre que Wine
    /// lise. Appelée aussi à chaque sondage pour reprendre une écriture qui
    /// avait rencontré `EAGAIN`.
    fn flush_stdin(&mut self) -> Result<(), String> {
        let Some(stdin) = self.child.stdin.as_mut() else {
            return Err("l'entrée standard du programme est fermée".to_string());
        };
        while !self.stdin_pending.is_empty() {
            match stdin.write(&self.stdin_pending) {
                Ok(0) => return Err("l'entrée standard du programme est fermée".to_string()),
                Ok(n) => {
                    self.stdin_pending.drain(..n);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
                Err(e) => return Err(format!("écriture sur l'entrée du programme: {e}")),
            }
        }
        Ok(())
    }

    /// Le processus a-t-il fini ? Rend son code de sortie la première fois, et
    /// à chaque appel ensuite. Ne bloque jamais.
    pub fn poll(&mut self) -> Option<i32> {
        // Une écriture précédemment différée reprend dès que le programme a
        // libéré de la place dans le tuyau. L'erreur sera rapportée lors de la
        // prochaine saisie utilisateur ; ici, poll doit rester non bloquant.
        let _ = self.flush_stdin();
        if self.exit.is_some() {
            return self.exit;
        }
        match self.child.try_wait() {
            Ok(Some(status)) => {
                // Tué par un signal : `code()` est None, et sous Wine cela
                // signale un plantage côté programme, pas un simple arrêt.
                self.exit = Some(status.code().unwrap_or(-1));
                self.exit
            }
            Ok(None) => None,
            // On ne peut plus l'attendre : le considérer vivant à jamais
            // laisserait l'IDE croire qu'un programme tourne encore.
            Err(_) => {
                self.exit = Some(-1);
                self.exit
            }
        }
    }

    /// Vrai tant que le programme tourne.
    pub fn is_running(&self) -> bool {
        self.exit.is_none()
    }

    /// Arrête le programme (bouton « Arrêter », relance, fermeture de l'IDE).
    pub fn kill(&mut self) {
        if self.exit.is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
            self.exit = Some(-1);
        }
    }
}

impl Drop for WineRun {
    /// Un `.exe` ne doit pas survivre à l'IDE qui l'a lancé — ni à la frappe
    /// suivante sur « Lancer », qui en démarre un autre.
    fn drop(&mut self) {
        self.kill();
    }
}

fn set_nonblocking(fd: std::os::fd::RawFd) {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL, 0);
        if flags >= 0 {
            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble::{self, Target};
    use std::path::PathBuf;

    /// Assemble un source win64 en `.exe`, prêt à être lancé.
    fn build_exe(name: &str, source: &str) -> PathBuf {
        let dir = PathBuf::from("build").join(name);
        std::fs::create_dir_all(&dir).expect("dossier");
        let asm = dir.join(format!("{name}.asm"));
        std::fs::write(&asm, source).expect("source");
        assemble::assemble_for(&asm, &dir, &[], Target::Windows)
            .expect("assemblage PE")
            .binary
    }

    /// Fait tourner le programme jusqu'à sa fin, en sondant comme le ferait
    /// l'interface. Rend (sortie, code de retour).
    fn run_to_completion(run: &mut WineRun) -> (String, i32) {
        let mut out = String::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            out.push_str(&run.take_output());
            if let Some(code) = run.poll() {
                // Le programme a pu écrire juste avant de rendre la main.
                out.push_str(&run.take_output());
                return (out, code);
            }
            assert!(
                std::time::Instant::now() < deadline,
                "le programme ne finit pas"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// Le programme tourne, écrit dans la console de l'IDE et rend son code de
    /// sortie — c'est tout ce que la cible Windows promet.
    #[test]
    fn a_pe_runs_and_reports_what_it_wrote() {
        if !available() {
            eprintln!("wine absent : exécution non vérifiée");
            return;
        }
        let exe = build_exe(
            "winerun-hello",
            r#"
            bits 64
            default rel
            section .data
                msg    db "Salut depuis Wine", 13, 10
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
            "#,
        );
        let mut run = WineRun::spawn(&exe).expect("wine doit démarrer");
        let (out, code) = run_to_completion(&mut run);
        assert!(out.contains("Salut depuis Wine"), "sortie: {out:?}");
        assert_eq!(code, 7, "le code de sortie doit être celui d'ExitProcess");
        assert!(!run.is_running(), "le programme est terminé");
    }

    /// Un programme qui tourne encore doit pouvoir être arrêté — sans quoi une
    /// boucle infinie survivrait à l'IDE.
    #[test]
    fn a_running_program_can_be_stopped() {
        if !available() {
            eprintln!("wine absent : arrêt non vérifié");
            return;
        }
        let exe = build_exe(
            "winerun-loop",
            "bits 64\nsection .text\n global main\nmain:\n boucle:\n  jmp boucle\n",
        );
        let mut run = WineRun::spawn(&exe).expect("wine doit démarrer");
        // Laisser le temps au chargeur d'arriver jusqu'au code.
        std::thread::sleep(std::time::Duration::from_millis(300));
        run.kill();
        assert!(!run.is_running(), "après kill, plus rien ne tourne");
        assert!(run.poll().is_some(), "un code de sortie est disponible");
    }

    /// La sortie arrive par morceaux, et un caractère multi-octets coupé en
    /// deux par une lecture ne doit pas ressortir en « � ».
    #[test]
    fn multibyte_output_is_never_cut_in_half() {
        if !available() {
            eprintln!("wine absent : découpage UTF-8 non vérifié");
            return;
        }
        let exe = build_exe(
            "winerun-utf8",
            r#"
            bits 64
            default rel
            section .data
                msg    db "éàü — accents", 13, 10
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
                xor     ecx, ecx
                call    ExitProcess
            "#,
        );
        let mut run = WineRun::spawn(&exe).expect("wine doit démarrer");
        let (out, code) = run_to_completion(&mut run);
        assert_eq!(code, 0);
        assert!(out.contains("éàü — accents"), "sortie: {out:?}");
        assert!(!out.contains('\u{FFFD}'), "aucun caractère de remplacement");
    }
}
