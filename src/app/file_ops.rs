use std::path::PathBuf;

use crate::i18n;

use super::{abs_dir_of, asmstd_dir, data_dir, App};

impl App {
    pub(super) fn log(&mut self, s: &str) {
        self.console.push_str(s);
        if !s.ends_with('\n') {
            self.console.push('\n');
        }
    }

    /// Pointe l'explorateur INTERNE de l'IDE sur le dossier où sont écrits les
    /// exemples et les exercices auto-corrigés (`~/.local/share/…`), et l'amène
    /// au premier plan. C'est là que l'élève retrouve les `ex_*.asm` à compléter
    /// et enregistre son travail ; l'y emmener d'un clic évite de le lui faire
    /// chercher, sans quitter l'IDE.
    pub(super) fn open_examples_dir(&mut self) {
        let dir = data_dir().join("examples");
        // Créé au premier lancement, mais on s'en assure : un dossier absent
        // laisserait l'explorateur vide sans explication.
        let _ = std::fs::create_dir_all(&dir);
        self.explorer_dir = dir.clone();
        self.explorer_selected = None;
        self.show_panel(super::dock::Panel::Explorer);
        self.focus_panel(super::dock::Panel::Explorer);
        self.log(&format!(
            "{} {}",
            i18n::tr(self.lang, "Explorateur :", "Explorer:"),
            dir.display()
        ));
    }

    pub(super) fn save_source(&mut self) -> bool {
        // Crée le dossier cible s'il n'existe pas (ex. `examples/` absent).
        if let Some(parent) = self.src_path.parent()
            && !parent.as_os_str().is_empty()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            self.log(&format!("{} {}: {e}", i18n::tr(self.lang, "Impossible de créer", "Cannot create"), parent.display()));
            return false;
        }
        match std::fs::write(&self.src_path, &self.source) {
            Ok(_) => {
                self.dirty = false;
                self.status = format!("{} {}", i18n::tr(self.lang, "Enregistré :", "Saved:"), self.src_path.display());
                true
            }
            Err(e) => {
                self.log(&format!("{} {}: {e}", i18n::tr(self.lang, "Erreur d'enregistrement de", "Error saving"), self.src_path.display()));
                false
            }
        }
    }

    /// Ouvre la boîte « Enregistrer sous » sur le dossier affiché dans l'explorateur.
    /// Dialogue natif (portail GNOME/Wayland via rfd) piloté sur un thread de fond :
    /// l'UI reste réactive (pas de freeze « ne répond pas ») pendant la sélection.
    /// Le résultat est récupéré dans `poll_file_dialogs`.
    pub(super) fn open_saveas(&mut self) {
        if self.pending_saveas.is_some() {
            return; // un dialogue est déjà ouvert
        }
        let name = self
            .src_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "programme.asm".to_string());
        let dir = self.explorer_dir.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            // AsyncFileDialog + block_on : le futur du portail XDG tourne ICI
            // (thread de fond), sans bloquer la boucle egui du thread principal.
            let path = pollster::block_on(
                rfd::AsyncFileDialog::new()
                    .set_title("Enregistrer sous")
                    .set_directory(&dir)
                    .set_file_name(&name)
                    .add_filter("Assembleur (.asm, .s)", &["asm", "s"])
                    .save_file(),
            )
            .map(|h| h.path().to_path_buf());
            let _ = tx.send(path);
        });
        self.pending_saveas = Some(rx);
    }

    /// Dialogue natif « Ouvrir » (portail GNOME/Wayland via rfd), non bloquant.
    /// Voir [`open_saveas`](Self::open_saveas) pour le motif thread de fond.
    pub(super) fn open_browser(&mut self) {
        if self.pending_open.is_some() {
            return;
        }
        let dir = self.explorer_dir.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let path = pollster::block_on(
                rfd::AsyncFileDialog::new()
                    .set_title("Ouvrir un fichier")
                    .set_directory(&dir)
                    .add_filter("Assembleur (.asm, .s)", &["asm", "s"])
                    .add_filter("Tous les fichiers", &["*"])
                    .pick_file(),
            )
            .map(|h| h.path().to_path_buf());
            let _ = tx.send(path);
        });
        self.pending_open = Some(rx);
    }

    /// Récupère le résultat des dialogues fichiers natifs en cours (thread de fond),
    /// sans bloquer. À appeler chaque frame depuis `update`.
    pub(super) fn poll_file_dialogs(&mut self) {
        use std::sync::mpsc::TryRecvError;
        // Ouvrir.
        if let Some(rx) = &self.pending_open {
            match rx.try_recv() {
                Ok(picked) => {
                    self.pending_open = None;
                    if let Some(path) = picked {
                        self.open_file(path);
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.pending_open = None,
            }
        }
        // Enregistrer sous.
        if let Some(rx) = &self.pending_saveas {
            match rx.try_recv() {
                Ok(picked) => {
                    self.pending_saveas = None;
                    if let Some(mut path) = picked {
                        // Extension .asm par défaut si l'utilisateur n'en fournit pas.
                        if path.extension().is_none() {
                            path.set_extension("asm");
                        }
                        self.explorer_dir = abs_dir_of(&path);
                        self.src_path = path;
                        self.save_source();
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.pending_saveas = None,
            }
        }
    }

    /// Vrai si un dialogue fichier natif est ouvert (attente d'une sélection).
    pub(super) fn dialog_pending(&self) -> bool {
        self.pending_open.is_some() || self.pending_saveas.is_some()
    }

    pub(super) fn open_file(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                self.source = content;
                // L'explorateur reflète le dossier du fichier ouvert.
                self.explorer_dir = abs_dir_of(&path);
                self.src_path = path;
                self.dirty = false;
                self.dbg = None;
                self.disasm.clear();
                self.binary = None;
                self.show_panel(super::dock::Panel::Editor);
                self.reload_exercise();
                self.status = format!("{} {}", i18n::tr(self.lang, "Ouvert :", "Opened:"), self.src_path.display());
            }
            Err(e) => self.log(&format!("{} {}: {e}", i18n::tr(self.lang, "Impossible d'ouvrir", "Cannot open"), path.display())),
        }
    }

    pub(super) fn new_file(&mut self) {
        self.source = "section .data\n\nsection .text\n    global _start\n_start:\n    mov rax, 60      ; sys_exit\n    xor rdi, rdi     ; code 0\n    syscall\n".to_string();
        // Le nouveau fichier vise le dossier actuellement affiché dans l'explorateur.
        self.src_path = self.explorer_dir.join("sans-titre.asm");
        self.dirty = true;
        self.dbg = None;
        self.disasm.clear();
        self.binary = None;
        self.show_panel(super::dock::Panel::Editor);
        self.status = i18n::tr(self.lang, "Nouveau fichier", "New file").to_string();
    }

    /// Répertoires de recherche `%include` pour nasm : dossier du fichier, et
    /// (si activé) dossier d'`asmstd.inc`.
    pub(super) fn include_dirs(&self) -> Vec<std::path::PathBuf> {
        let mut dirs = Vec::new();
        if let Some(p) = self.src_path.parent()
            && !p.as_os_str().is_empty()
        {
            dirs.push(p.to_path_buf());
        }
        if self.use_asmstd
            && let Some(d) = asmstd_dir()
            && !dirs.contains(&d)
        {
            dirs.push(d);
        }
        dirs
    }
}
