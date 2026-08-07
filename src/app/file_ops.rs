use std::path::PathBuf;

use crate::i18n;

use super::{abs_dir_of, asmstd_dir, data_dir, App};

impl App {
    pub(super) fn log(&mut self, s: &str) {
        self.console_push(s);
        if !s.ends_with('\n') {
            self.console_push("\n");
        }
    }

    /// Ajoute du texte à la console en gardant celle-ci bornée.
    ///
    /// Passage obligé : c'est ici que sont écrits aussi bien le journal de
    /// l'IDE que la sortie du programme tracé. Or une boucle `write` — l'étourderie
    /// classique de l'élève qui oublie sa condition d'arrêt — produit des dizaines
    /// de milliers de lignes par « Continuer ». La `String` grandirait sans fin,
    /// et egui, qui remet en page tout le texte à chaque frame, rendrait l'IDE
    /// injouable bien avant que la mémoire ne manque.
    pub(super) fn console_push(&mut self, s: &str) {
        self.console.push_str(s);
        if self.console.len() > super::CONSOLE_MAX {
            self.trim_console();
        }
    }

    /// Ne conserve que la fin de la console — c'est là qu'est le plus récent,
    /// donc le plus utile. On coupe par gros blocs plutôt qu'octet par octet :
    /// chaque troncature décale tout le reste de la chaîne, et le faire à
    /// chaque écriture coûterait plus cher que le débordement qu'on évite.
    fn trim_console(&mut self) {
        let cut = self.console.len() - super::CONSOLE_KEEP;
        // Repart sur une frontière de caractère, puis sur un début de ligne :
        // couper au milieu d'un caractère multi-octets ferait paniquer le
        // découpage, et au milieu d'une ligne tromperait la lecture.
        let cut = (cut..self.console.len())
            .find(|i| self.console.is_char_boundary(*i))
            .unwrap_or(self.console.len());
        let cut = self.console[cut..]
            .find('\n')
            .map_or(cut, |i| cut + i + 1);
        let kept = self.console.split_off(cut);
        self.console = format!(
            "{}\n{kept}",
            i18n::tr3(
                self.lang,
                "[…] début de la console tronqué",
                "[…] start of the console truncated",
                "[…] inicio de la consola truncado",
            )
        );
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

#[cfg(test)]
mod console_tests {
    use super::super::{App, CONSOLE_KEEP, CONSOLE_MAX};

    /// Une boucle `write` étourdie ne doit pas faire enfler la console sans
    /// fin : passé le plafond, c'est le début qui est jeté.
    #[test]
    fn the_console_stays_bounded() {
        let mut app = App::new();
        let line = "sortie du programme qui boucle\n";
        for _ in 0..(CONSOLE_MAX / line.len() + 500) {
            app.console_push(line);
        }
        assert!(
            app.console.len() <= CONSOLE_MAX,
            "console à {} octets, au-delà du plafond de {CONSOLE_MAX}",
            app.console.len()
        );
        assert!(
            app.console.len() >= CONSOLE_KEEP,
            "on jette le début, on ne vide pas tout"
        );
    }

    /// Ce qui est gardé est la fin — la partie récente, celle que l'élève
    /// regarde — et le rognage est annoncé.
    #[test]
    fn trimming_keeps_the_end_and_says_so() {
        let mut app = App::new();
        app.console_push("TOUT DEBUT\n");
        // Assez pour franchir le plafond, quelle que soit la longueur des
        // lignes : on s'arrête sur la taille atteinte, pas sur un compte deviné.
        let mut i = 0;
        while app.console.len() < CONSOLE_MAX {
            app.console_push(&format!("ligne {i}\n"));
            i += 1;
        }
        app.console_push("DERNIERE LIGNE\n");

        assert!(app.console.ends_with("DERNIERE LIGNE\n"), "la fin est intacte");
        assert!(!app.console.contains("TOUT DEBUT"), "le début est parti");
        assert!(app.console.starts_with("[…]"), "le rognage est signalé");
    }

    /// Le rognage tombe sur une frontière de caractère : couper au milieu d'un
    /// caractère multi-octets ferait paniquer le découpage de la chaîne.
    #[test]
    fn trimming_does_not_split_a_multibyte_character() {
        let mut app = App::new();
        // « é » fait deux octets, « … » trois : les frontières ne tombent pas
        // sur des multiples ronds, et une coupe naïve atterrirait dedans.
        let line = "déjà lu… caractères accentués\n";
        for _ in 0..(CONSOLE_MAX / line.len() + 500) {
            app.console_push(line);
        }
        // Le seul fait d'arriver ici sans panique vaut vérification ; on
        // s'assure en plus que le texte reste lisible.
        assert!(app.console.contains("déjà lu…"), "le texte gardé reste intact");
    }
}
