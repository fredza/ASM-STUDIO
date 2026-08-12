use std::path::PathBuf;

use crate::i18n;

use super::{abs_dir_of, asmstd_dir, data_dir, App};

/// Ajoute du texte à un tampon d'affichage en le gardant borné, et ne conserve
/// que la fin — c'est là qu'est le plus récent, donc le plus utile.
///
/// On coupe par gros blocs plutôt qu'octet par octet : chaque troncature décale
/// tout le reste de la chaîne, et le faire à chaque écriture coûterait plus cher
/// que le débordement qu'on évite.
fn push_bounded(buf: &mut String, s: &str, lang: i18n::Lang) {
    buf.push_str(s);
    if buf.len() <= super::CONSOLE_MAX {
        return;
    }
    let cut = buf.len() - super::CONSOLE_KEEP;
    // Repart sur une frontière de caractère, puis sur un début de ligne :
    // couper au milieu d'un caractère multi-octets ferait paniquer le
    // découpage, et au milieu d'une ligne tromperait la lecture.
    let cut = (cut..buf.len()).find(|i| buf.is_char_boundary(*i)).unwrap_or(buf.len());
    let cut = buf[cut..].find('\n').map_or(cut, |i| cut + i + 1);
    let kept = buf.split_off(cut);
    *buf = format!(
        "{}\n{kept}",
        i18n::tr3(
            lang,
            "[…] début de la console tronqué",
            "[…] start of the console truncated",
            "[…] inicio de la consola truncado",
        )
    );
}

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
        let lang = self.lang;
        push_bounded(&mut self.console, s, lang);
    }

    /// Écrit ce qui vient du programme lui-même : la console (mêlée au journal,
    /// pour le déroulement) et le tampon de sortie pure (pour la boîte
    /// « Sortie du programme »).
    ///
    /// Les deux destinations sont indissociables — écrire dans l'une sans
    /// l'autre ferait diverger silencieusement ce que montrent les deux vues.
    pub(super) fn program_out_push(&mut self, s: &str) {
        let lang = self.lang;
        push_bounded(&mut self.program_output, s, lang);
        push_bounded(&mut self.console, s, lang);
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
                self.mark_saved();
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
                        // Le fichier n'existe qu'après l'écriture : les récents
                        // ne l'enregistrent donc qu'en cas de succès, sinon
                        // `prune_recent` le retirerait aussitôt.
                        if self.save_source() {
                            let saved = self.src_path.clone();
                            self.push_recent(&saved);
                            self.save_settings();
                        }
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

    /// Ouvre un fichier, après s'être assuré que le travail en cours ne part
    /// pas avec (voir [`super::unsaved`]).
    pub(super) fn open_file(&mut self, path: PathBuf) {
        self.guarded(super::unsaved::PendingAction::OpenFile(path));
    }

    pub(super) fn open_file_now(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                // Un fichier ouvert n'annonce pas son format : il se lit dans
                // le code. Sans cela, ouvrir un source Windows depuis
                // l'explorateur laissait la cible sur Linux, et Build répondait
                // par une erreur de nasm sur `extern ExitProcess` — un message
                // qui ne parle pas du vrai problème.
                self.adopt_detected_target(&content);
                self.source = content;
                // L'explorateur reflète le dossier du fichier ouvert.
                self.explorer_dir = abs_dir_of(&path);
                self.src_path = path;
                self.mark_saved();
                self.dbg = None;
                self.disasm.clear();
                self.binary = None;
                self.show_panel(super::dock::Panel::Editor);
                self.reload_exercise();
                let opened = self.src_path.clone();
                self.push_recent(&opened);
                self.save_settings();
                self.status = format!("{} {}", i18n::tr(self.lang, "Ouvert :", "Opened:"), self.src_path.display());
            }
            Err(e) => self.log(&format!("{} {}: {e}", i18n::tr(self.lang, "Impossible d'ouvrir", "Cannot open"), path.display())),
        }
    }

    /// Repart d'un squelette vierge, après s'être assuré que le travail en
    /// cours ne part pas avec (voir [`super::unsaved`]).
    pub(super) fn new_file(&mut self) {
        self.guarded(super::unsaved::PendingAction::NewFile);
    }

    /// Le squelette de départ ne dépend pas de l'humeur : il dépend du format
    /// visé. Un fichier ELF commence par `_start` et se termine par `syscall` ;
    /// un fichier PE commence par `main` et appelle `ExitProcess`. Écrire l'un
    /// pour assembler l'autre ne produit pas un avertissement, mais une erreur
    /// de nasm à laquelle un débutant ne comprend rien.
    pub(super) fn new_file_now(&mut self) {
        // Tant que l'assemblage Windows est décoché, il n'y a qu'un format
        // possible : poser la question serait un choix à une seule réponse.
        if self.pe_enabled {
            self.new_file_prompt = true;
            return;
        }
        self.create_new_file(crate::assemble::Target::Linux);
    }

    /// Crée le nouveau fichier pour le format demandé, et pose la cible qui va
    /// avec : le squelette et le `nasm -f` doivent parler du même format.
    pub(super) fn create_new_file(&mut self, target: crate::assemble::Target) {
        use crate::assemble::Target;
        self.new_file_prompt = false;
        self.set_target(target);
        self.source = match target {
            Target::Linux => super::SKELETON_ELF,
            Target::Windows => super::SKELETON_PE_CONSOLE,
            Target::WindowsGui => super::SKELETON_PE_GUI,
        }
        .to_string();
        // Le nouveau fichier vise le dossier actuellement affiché dans
        // l'explorateur, et porte l'extension du monde dont il relève.
        let name = if target.is_windows() { "sans-titre-win.asm" } else { "sans-titre.asm" };
        self.src_path = self.explorer_dir.join(name);
        // Le squelette n'est pas du travail : tant que l'élève n'y a pas
        // touché, fermer ou changer de fichier ne lui coûte rien, et il serait
        // absurde de lui poser la question.
        self.mark_saved();
        self.dbg = None;
        self.disasm.clear();
        self.binary = None;
        self.show_panel(super::dock::Panel::Editor);
        self.status = i18n::tr(self.lang, "Nouveau fichier", "New file").to_string();
    }

    /// Inscrit un fichier en tête des récents. Le même chemin ne s'y trouve
    /// qu'une fois : le rouvrir le remonte au lieu de l'y ajouter deux fois.
    ///
    /// Les chemins sont rendus absolus avant d'entrer : la liste survit à la
    /// session, alors qu'un chemin relatif ne veut plus rien dire dès que le
    /// répertoire courant a changé.
    pub(super) fn push_recent(&mut self, path: &std::path::Path) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.recent_files.retain(|p| p != &path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(super::MAX_RECENT);
    }

    /// Récents encore ouvrables. Un fichier renommé, déplacé ou supprimé entre
    /// deux séances est retiré ici plutôt que proposé pour rien : cliquer une
    /// entrée morte n'apprend rien à personne.
    pub(super) fn prune_recent(&mut self) {
        self.recent_files.retain(|p| p.is_file());
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
mod recent_tests {
    use super::super::{App, MAX_RECENT};
    use std::path::PathBuf;

    #[test]
    fn the_last_opened_file_comes_first() {
        let mut app = App::new();
        app.push_recent(&PathBuf::from("/tmp/a.asm"));
        app.push_recent(&PathBuf::from("/tmp/b.asm"));
        assert_eq!(app.recent_files[0], PathBuf::from("/tmp/b.asm"));
        assert_eq!(app.recent_files.len(), 2);
    }

    /// Rouvrir un fichier le remonte, il ne s'ajoute pas une seconde fois.
    #[test]
    fn reopening_moves_the_entry_up_without_duplicating_it() {
        let mut app = App::new();
        for p in ["/tmp/a.asm", "/tmp/b.asm", "/tmp/a.asm"] {
            app.push_recent(&PathBuf::from(p));
        }
        assert_eq!(app.recent_files, vec![PathBuf::from("/tmp/a.asm"), PathBuf::from("/tmp/b.asm")]);
    }

    #[test]
    fn the_list_never_outgrows_the_menu() {
        let mut app = App::new();
        for i in 0..(MAX_RECENT + 7) {
            app.push_recent(&PathBuf::from(format!("/tmp/ex{i}.asm")));
        }
        assert_eq!(app.recent_files.len(), MAX_RECENT);
        assert_eq!(
            app.recent_files[0],
            PathBuf::from(format!("/tmp/ex{}.asm", MAX_RECENT + 6)),
            "le plus récent reste en tête"
        );
    }

    /// Un fichier supprimé ou déplacé entre deux séances quitte la liste.
    #[test]
    fn vanished_files_are_pruned() {
        let mut app = App::new();
        let alive = std::env::temp_dir().join("asm-studio-recent-test.asm");
        std::fs::write(&alive, "; test\n").expect("écriture du fichier témoin");
        app.push_recent(&alive);
        app.push_recent(&PathBuf::from("/tmp/asm-studio-jamais-existe-1234.asm"));

        app.prune_recent();

        assert_eq!(app.recent_files.len(), 1, "seul le fichier réel survit");
        let _ = std::fs::remove_file(&alive);
    }

    /// Aller-retour par le fichier de réglages : c'est ce qui fait que la liste
    /// survit à la fermeture de l'application.
    #[test]
    fn the_list_survives_a_settings_round_trip() {
        let mut app = App::new();
        // Un chemin contenant une espace et un « = » : le format une-ligne-par-
        // fichier doit les rendre tels quels, sans échappement.
        app.push_recent(&PathBuf::from("/tmp/mes exercices/tp=1.asm"));
        app.push_recent(&PathBuf::from("/tmp/b.asm"));
        let content = app.settings_content();

        let mut reloaded = App::new();
        reloaded.apply_settings(&content);
        assert_eq!(reloaded.recent_files, app.recent_files);
    }

    /// Le fichier ouvert au lancement ne doit pas se retrouver dans la liste
    /// sans qu'on l'ait ouvert : elle raconte ce que l'élève a fait.
    #[test]
    fn a_fresh_install_has_an_empty_list() {
        let app = App::new();
        assert!(app.recent_files.is_empty());
    }
}

#[cfg(test)]
mod new_file_tests {
    use super::super::App;
    use crate::assemble::Target;

    /// Tant que l'assemblage Windows est décoché, il n'y a qu'un format
    /// possible : le fichier se crée sans poser de question.
    #[test]
    fn without_the_windows_target_no_question_is_asked() {
        let mut app = App::new();
        app.pe_enabled = false;
        app.new_file();
        assert!(!app.new_file_prompt, "une seule réponse possible, donc pas de question");
        assert!(app.source.contains("_start"), "squelette ELF");
        assert_eq!(app.target, Target::Linux);
    }

    /// Avec les deux formats offerts, le nouveau fichier demande lequel — et ne
    /// touche à rien tant que la réponse n'est pas venue.
    #[test]
    fn with_both_formats_the_question_is_asked_before_anything_changes() {
        let mut app = App::new();
        app.pe_enabled = true;
        app.source = "; mon travail\n".to_string();
        app.mark_saved();

        app.new_file();

        assert!(app.new_file_prompt, "la question doit être posée");
        assert_eq!(app.source, "; mon travail\n", "rien n'est écrasé avant la réponse");
    }

    /// Chaque réponse pose SON squelette et SA cible : un fichier PE qui
    /// s'assemblerait en ELF n'irait pas plus loin que la première erreur de
    /// nasm, et c'est exactement ce qu'on veut éviter à un débutant.
    #[test]
    fn each_answer_lays_down_its_own_skeleton_and_target() {
        for (target, needle) in [
            (Target::Linux, "syscall"),
            (Target::Windows, "ExitProcess"),
            (Target::WindowsGui, "MessageBoxA"),
        ] {
            let mut app = App::new();
            app.pe_enabled = true;
            app.new_file();

            app.create_new_file(target);

            assert!(!app.new_file_prompt, "la boîte se referme sur la réponse");
            assert_eq!(app.target, target, "la cible suit le squelette");
            assert!(
                app.source.contains(needle),
                "squelette {target:?} : « {needle} » attendu, source :\n{}",
                app.source
            );
            assert!(!app.dirty(), "un squelette n'est pas encore du travail");
            assert_eq!(
                app.src_path.extension().and_then(|e| e.to_str()),
                Some("asm")
            );
        }
    }

    /// Les trois squelettes doivent s'ASSEMBLER tels quels : un point de départ
    /// qui ne compile pas est pire que pas de point de départ du tout.
    #[test]
    fn every_skeleton_assembles_as_is() {
        use std::path::PathBuf;
        for (target, tag) in [
            (Target::Linux, "elf"),
            (Target::Windows, "pe"),
            (Target::WindowsGui, "pe-gui"),
        ] {
            let mut app = App::new();
            app.pe_enabled = true;
            app.create_new_file(target);

            let dir = PathBuf::from(format!("build/squelette-{tag}"));
            std::fs::create_dir_all(&dir).expect("dossier de travail");
            app.out_dir = dir.clone();
            app.src_path = dir.join("sans-titre.asm");
            app.build();

            assert!(
                app.binary.is_some(),
                "squelette {target:?} : l'assemblage échoue\n{}",
                app.console
            );
        }
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

    /// Le partage des eaux : le journal de l'IDE ne doit jamais atterrir dans
    /// la sortie du programme, sinon la boîte « Sortie du programme » ne
    /// répondrait plus à la question qu'elle pose. Dans l'autre sens, ce que le
    /// programme écrit va bien dans les deux — la console raconte tout.
    #[test]
    fn the_ide_log_never_reaches_the_program_output() {
        let mut app = App::new();
        app.log("Running...");
        app.program_out_push("Hello, world!\n");
        app.log("✘ SIGSEGV");

        assert_eq!(
            app.program_output, "Hello, world!\n",
            "seule la sortie du programme doit s'y trouver"
        );
        for msg in ["Running...", "Hello, world!", "SIGSEGV"] {
            assert!(app.console.contains(msg), "la console garde tout : {msg} manque");
        }
    }

    /// Le tampon de sortie hérite du plafond de la console : une boucle `write`
    /// étourdie le ferait enfler tout autant.
    #[test]
    fn the_program_output_stays_bounded_too() {
        let mut app = App::new();
        let line = "boucle sans condition d'arret\n";
        for _ in 0..(CONSOLE_MAX / line.len() + 500) {
            app.program_out_push(line);
        }
        assert!(
            app.program_output.len() <= CONSOLE_MAX,
            "sortie à {} octets, au-delà du plafond de {CONSOLE_MAX}",
            app.program_output.len()
        );
        assert!(app.program_output.len() >= CONSOLE_KEEP, "on jette le début, pas tout");
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
