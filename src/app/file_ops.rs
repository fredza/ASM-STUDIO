use std::path::PathBuf;

use crate::i18n;

use super::{abs_dir_of, asmstd_dir, App};

impl App {
    pub(super) fn log(&mut self, s: &str) {
        self.console.push_str(s);
        if !s.ends_with('\n') {
            self.console.push('\n');
        }
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
    /// Dialogue natif « Enregistrer sous » (portail GNOME/Wayland via rfd) :
    /// la création de dossier est intégrée au sélecteur du système.
    pub(super) fn open_saveas(&mut self) {
        let name = self
            .src_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "programme.asm".to_string());
        let picked = rfd::FileDialog::new()
            .set_title("Enregistrer sous")
            .set_directory(&self.explorer_dir)
            .set_file_name(&name)
            .add_filter("Assembleur (.asm, .s)", &["asm", "s"])
            .save_file();
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

    /// Dialogue natif « Ouvrir » (portail GNOME/Wayland via rfd).
    pub(super) fn open_browser(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title("Ouvrir un fichier")
            .set_directory(&self.explorer_dir)
            .add_filter("Assembleur (.asm, .s)", &["asm", "s"])
            .add_filter("Tous les fichiers", &["*"])
            .pick_file();
        if let Some(path) = picked {
            self.open_file(path);
        }
    }

    pub(super) fn open_file(&mut self, path: PathBuf) {
        use super::Tab;
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
                self.tab = Tab::Editor;
                self.status = format!("{} {}", i18n::tr(self.lang, "Ouvert :", "Opened:"), self.src_path.display());
            }
            Err(e) => self.log(&format!("{} {}: {e}", i18n::tr(self.lang, "Impossible d'ouvrir", "Cannot open"), path.display())),
        }
    }

    pub(super) fn new_file(&mut self) {
        use super::Tab;
        self.source = "section .data\n\nsection .text\n    global _start\n_start:\n    mov rax, 60      ; sys_exit\n    xor rdi, rdi     ; code 0\n    syscall\n".to_string();
        // Le nouveau fichier vise le dossier actuellement affiché dans l'explorateur.
        self.src_path = self.explorer_dir.join("sans-titre.asm");
        self.dirty = true;
        self.dbg = None;
        self.disasm.clear();
        self.binary = None;
        self.tab = Tab::Editor;
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
