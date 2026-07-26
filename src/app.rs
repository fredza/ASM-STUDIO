//! Application eframe : éditeur NASM + débogueur pédagogique.
//!
//! Layout : menu / barre d'outils en haut, barre d'état + bande
//! (Mémoire | Timeline | Console) en bas, Registres/Flags à gauche,
//! Instruction + Pile à droite, éditeur/désassemblage au centre (onglets).
//! L'état affiché est lu dans l'historique du debugger à `view_index`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, RichText};

use crate::assemble;
use crate::debugger::{Debugger, Flags, RunState, Snapshot};
use crate::disasm::{self, Insn};
use crate::{explain, srcmap, syntax, syscall};

// --- Palette ---
const ACCENT: Color32 = Color32::from_rgb(0x4C, 0x8B, 0xF5); // bleu d'accent
const ACTION: Color32 = Color32::from_rgb(0xE8, 0x8A, 0x2E); // orange d'action (Run/Step)
const HEADER: Color32 = Color32::from_rgb(0x8A, 0x9B, 0xB4); // titres de section
const CHANGED: Color32 = Color32::from_rgb(0xF5, 0xA6, 0x23); // valeur modifiée
const FLAG_ON: Color32 = Color32::from_rgb(0x5F, 0xBF, 0x69);
const FLAG_OFF: Color32 = Color32::from_rgb(0x77, 0x77, 0x80);
const RIP_ROW: Color32 = Color32::from_rgb(0x3A, 0x33, 0x1E);
const SEL_ROW: Color32 = Color32::from_rgb(0x2E, 0x2E, 0x38);
const ADDR_COL: Color32 = Color32::from_rgb(0x7F, 0x9C, 0xD1);
const BYTES_COL: Color32 = Color32::from_rgb(0x80, 0x80, 0x88);
const MNEMONIC: Color32 = Color32::from_rgb(0x6E, 0xB4, 0xE8);
const FALSE_COL: Color32 = Color32::from_rgb(0xD9, 0x5B, 0x5B);

// Couleur de la gouttière de numéros de ligne.
const GUTTER: Color32 = Color32::from_rgb(0x60, 0x66, 0x70);
// Animation « CPU vivant ».
const FLASH_DUR: f64 = 0.7; // durée du fondu (secondes)
const FLASH_BRIGHT: Color32 = Color32::from_rgb(0xFF, 0xF2, 0x9A); // pic de pulsation
const PUSH_COL: Color32 = Color32::from_rgb(0x5F, 0xBF, 0x69);
const POP_COL: Color32 = Color32::from_rgb(0xE0, 0x8A, 0x3C);

/// Interpolation linéaire entre deux couleurs (t ∈ [0,1]).
fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// Couleur d'une valeur modifiée, pulsant du clair vers `CHANGED` selon `flash`.
fn changed_color(flash: Option<f32>) -> Color32 {
    changed_color2(flash, CHANGED)
}

/// Comme [`changed_color`] mais vers une couleur de base arbitraire.
fn changed_color2(flash: Option<f32>, base: Color32) -> Color32 {
    match flash {
        Some(p) => lerp_color(FLASH_BRIGHT, base, p),
        None => base,
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Editor,
    Disasm,
}

#[derive(PartialEq, Clone, Copy)]
enum StackTab {
    Stack,
    Heap,
}

/// Icônes de l'app (planche `src/Assets`, découpées dans `assets/icons/`),
/// chargées une fois comme textures egui.
struct Icons {
    editor: egui::TextureHandle,
    assembler: egui::TextureHandle,
    run: egui::TextureHandle,
    debug: egui::TextureHandle,
    registers: egui::TextureHandle,
    stack: egui::TextureHandle,
    heap: egui::TextureHandle,
    // Icônes complémentaires (même thème, générées) — boutons et panneaux.
    stop: egui::TextureHandle,
    pause: egui::TextureHandle,
    next: egui::TextureHandle,
    restart: egui::TextureHandle,
    attach: egui::TextureHandle,
    memory: egui::TextureHandle,
    timeline: egui::TextureHandle,
    console: egui::TextureHandle,
    syscalls: egui::TextureHandle,
    callstack: egui::TextureHandle,
    explorer: egui::TextureHandle,
    instruction: egui::TextureHandle,
}

impl Icons {
    fn load(ctx: &egui::Context) -> Self {
        macro_rules! ic {
            ($name:literal) => {
                load_texture(ctx, $name, include_bytes!(concat!("../assets/icons/", $name, ".png")))
            };
        }
        Icons {
            editor: ic!("editor"),
            assembler: ic!("assembler"),
            run: ic!("run"),
            debug: ic!("debug"),
            registers: ic!("registers"),
            stack: ic!("stack"),
            heap: ic!("heap"),
            stop: ic!("stop"),
            pause: ic!("pause"),
            next: ic!("next"),
            restart: ic!("restart"),
            attach: ic!("attach"),
            memory: ic!("memory"),
            timeline: ic!("timeline"),
            console: ic!("console"),
            syscalls: ic!("syscalls"),
            callstack: ic!("callstack"),
            explorer: ic!("explorer"),
            instruction: ic!("instruction"),
        }
    }
}

/// Décode un PNG et le charge en texture egui (réutilise le décodeur d'eframe).
fn load_texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> egui::TextureHandle {
    let img = eframe::icon_data::from_png_bytes(bytes).expect("PNG d'icône valide");
    let color = egui::ColorImage::from_rgba_unmultiplied(
        [img.width as usize, img.height as usize],
        &img.rgba,
    );
    ctx.load_texture(name, color, egui::TextureOptions::LINEAR)
}

/// Un appel système exécuté, pour le panneau SYSCALLS.
struct SyscallLog {
    name: String,
    args: String,
    number: u64,
    ret: Option<i64>,
}

pub struct App {
    src_path: PathBuf,
    out_dir: PathBuf,
    /// Contenu de l'éditeur (source NASM en cours d'édition).
    source: String,
    /// Modifications non enregistrées.
    dirty: bool,
    binary: Option<PathBuf>,

    dbg: Option<Debugger>,
    disasm: Vec<Insn>,
    /// Mapping adresse → ligne source (1-based) pour le suivi dans l'éditeur.
    src_map: HashMap<u64, usize>,
    selected: Option<u64>,
    /// Instruction ouverte dans le mode « microscope » (fenêtre dédiée).
    microscope: Option<u64>,
    /// Appels système exécutés (panneau SYSCALLS).
    syscalls: Vec<SyscallLog>,
    /// Adresses des frames actives (panneau CALL STACK), suivi call/ret.
    call_stack: Vec<u64>,
    /// Dossier affiché dans l'explorateur de fichiers (panneau de gauche).
    explorer_dir: PathBuf,
    view_index: usize,

    mem_addr: u64,
    mem_input: String,
    /// Octets hexa à écrire en mémoire (laboratoire mémoire).
    mem_poke: String,
    /// Registre en cours d'édition (laboratoire mémoire) et son tampon de saisie.
    edit_reg: Option<&'static str>,
    edit_buf: String,
    /// Demande de focus au premier frame d'édition d'un registre.
    edit_focus: bool,
    console: String,
    status: String,
    /// Décalage vertical de l'éditeur (pour synchroniser la gouttière).
    editor_scroll_y: f32,
    /// Position du curseur dans l'éditeur (1-based), pour la barre d'état.
    editor_ln: usize,
    editor_col: usize,

    tab: Tab,
    stack_tab: StackTab,
    /// Thème sombre actif (mis à jour dans `apply_theme`) — palette de texte.
    dark: bool,
    /// Visibilité des panneaux (menu Affichage).
    show_explorer: bool,
    show_instruction: bool,
    show_cpu_band: bool,
    show_bottom_band: bool,
    show_tooltips: bool,
    /// Animations « CPU vivant » (pulsation des valeurs modifiées au Step).
    animate: bool,
    /// Rend `asmstd.inc` disponible partout (ajoute son dossier aux includes nasm).
    use_asmstd: bool,
    /// Instant (temps egui) du dernier Step, pour animer le fondu.
    flash_time: f64,
    /// Un Step vient d'avoir lieu : mémorise l'instant au prochain frame.
    pending_flash: bool,
    theme_pref: egui::ThemePreference,
    show_settings: bool,
    show_about: bool,
    show_shortcuts: bool,
    /// Icônes (chargées au premier frame, quand le contexte egui existe).
    icons: Option<Icons>,
}

impl App {
    pub fn new() -> Self {
        let src_path = PathBuf::from("examples/test.asm");
        let source = std::fs::read_to_string(&src_path).unwrap_or_else(|_| {
            "section .text\n    global _start\n_start:\n    mov rax, 60\n    xor rdi, rdi\n    syscall\n"
                .to_string()
        });
        let explorer_dir = abs_dir_of(&src_path);
        let mut app = App {
            src_path,
            out_dir: PathBuf::from("build"),
            source,
            dirty: false,
            binary: None,
            dbg: None,
            disasm: Vec::new(),
            src_map: HashMap::new(),
            selected: None,
            microscope: None,
            syscalls: Vec::new(),
            call_stack: Vec::new(),
            explorer_dir,
            view_index: 0,
            mem_addr: 0,
            mem_input: String::new(),
            mem_poke: String::new(),
            edit_reg: None,
            edit_buf: String::new(),
            edit_focus: false,
            console: String::new(),
            status: "Prêt".to_string(),
            editor_scroll_y: 0.0,
            editor_ln: 1,
            editor_col: 1,
            tab: Tab::Editor,
            stack_tab: StackTab::Stack,
            dark: true,
            show_explorer: true,
            show_instruction: true,
            show_cpu_band: true,
            show_bottom_band: true,
            show_tooltips: true,
            animate: true,
            use_asmstd: false,
            flash_time: 0.0,
            pending_flash: false,
            theme_pref: egui::ThemePreference::Dark,
            show_settings: false,
            show_about: false,
            show_shortcuts: false,
            icons: None,
        };
        app.load_settings();
        app
    }

    // ---------- Persistance des réglages ----------

    fn load_settings(&mut self) {
        use egui::ThemePreference;
        let Some(path) = settings_path() else { return };
        let Ok(content) = std::fs::read_to_string(&path) else { return };
        for line in content.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "theme" => {
                    self.theme_pref = match v {
                        "system" => ThemePreference::System,
                        "light" => ThemePreference::Light,
                        _ => ThemePreference::Dark,
                    }
                }
                "tooltips" => self.show_tooltips = v == "true",
                "asmstd" => self.use_asmstd = v == "true",
                "animate" => self.animate = v == "true",
                "show_explorer" => self.show_explorer = v == "true",
                "show_instruction" => self.show_instruction = v == "true",
                "show_cpu_band" => self.show_cpu_band = v == "true",
                "show_bottom_band" => self.show_bottom_band = v == "true",
                _ => {}
            }
        }
    }

    fn save_settings(&self) {
        use egui::ThemePreference;
        let Some(path) = settings_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let theme = match self.theme_pref {
            ThemePreference::System => "system",
            ThemePreference::Light => "light",
            _ => "dark",
        };
        let content = format!(
            "theme={theme}\ntooltips={}\nasmstd={}\nanimate={}\n\
             show_explorer={}\nshow_instruction={}\nshow_cpu_band={}\nshow_bottom_band={}\n",
            self.show_tooltips,
            self.use_asmstd,
            self.animate,
            self.show_explorer,
            self.show_instruction,
            self.show_cpu_band,
            self.show_bottom_band,
        );
        let _ = std::fs::write(&path, content);
    }

    // ---------- Fichiers ----------

    fn log(&mut self, s: &str) {
        self.console.push_str(s);
        if !s.ends_with('\n') {
            self.console.push('\n');
        }
    }

    fn save_source(&mut self) -> bool {
        // Crée le dossier cible s'il n'existe pas (ex. `examples/` absent).
        if let Some(parent) = self.src_path.parent()
            && !parent.as_os_str().is_empty()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            self.log(&format!("Impossible de créer {}: {e}", parent.display()));
            return false;
        }
        match std::fs::write(&self.src_path, &self.source) {
            Ok(_) => {
                self.dirty = false;
                self.status = format!("Enregistré : {}", self.src_path.display());
                true
            }
            Err(e) => {
                self.log(&format!("Erreur d'enregistrement de {}: {e}", self.src_path.display()));
                false
            }
        }
    }

    /// Ouvre la boîte « Enregistrer sous » sur le dossier affiché dans l'explorateur.
    /// Dialogue natif « Enregistrer sous » (portail GNOME/Wayland via rfd) :
    /// la création de dossier est intégrée au sélecteur du système.
    fn open_saveas(&mut self) {
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
    fn open_browser(&mut self) {
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

    fn open_file(&mut self, path: PathBuf) {
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
                self.status = format!("Ouvert : {}", self.src_path.display());
            }
            Err(e) => self.log(&format!("Impossible d'ouvrir {}: {e}", path.display())),
        }
    }

    fn new_file(&mut self) {
        self.source = "section .data\n\nsection .text\n    global _start\n_start:\n    mov rax, 60      ; sys_exit\n    xor rdi, rdi     ; code 0\n    syscall\n".to_string();
        // Le nouveau fichier vise le dossier actuellement affiché dans l'explorateur.
        self.src_path = self.explorer_dir.join("sans-titre.asm");
        self.dirty = true;
        self.dbg = None;
        self.disasm.clear();
        self.binary = None;
        self.tab = Tab::Editor;
        self.status = "Nouveau fichier".to_string();
    }

    // ---------- Build / Run ----------

    /// Répertoires de recherche `%include` pour nasm : dossier du fichier, et
    /// (si activé) dossier d'`asmstd.inc`.
    fn include_dirs(&self) -> Vec<PathBuf> {
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

    /// Enregistre puis assemble (nasm) et lie (ld) le programme de l'utilisateur.
    fn build(&mut self) {
        self.save_source();
        // Artefacts dans un sous-dossier `build/` À CÔTÉ du fichier source
        // (et non plus dans un `build/` global relatif au répertoire courant).
        self.out_dir = abs_dir_of(&self.src_path).join("build");
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
                self.status = "Échec build".to_string();
            }
        }
    }

    fn launch(&mut self) {
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
                self.status = format!("Lancé — RIP @ 0x{:X}", dbg.regs().rip);
                self.log("Running...");
                self.dbg = Some(dbg);
            }
            Err(e) => {
                self.log(&e);
                self.status = "Échec lancement".to_string();
            }
        }
    }

    fn stop(&mut self) {
        self.dbg = None;
        self.status = "Arrêté".to_string();
    }

    fn step(&mut self) {
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
                self.status = format!("Step {} — RIP @ 0x{:X}", d.steps(), d.regs().rip);
            }
            Some(RunState::Exited(code)) => self.status = format!("Terminé (exit {code})"),
            Some(RunState::Signaled) => self.status = "Terminé (signal)".to_string(),
            None => {}
        }
    }

    fn resume_here(&mut self) {
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
                self.status = format!("Repris à l'étape {}", self.view_index);
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
    fn rebuild_trace(&mut self) {
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

    // ---------- Accès à l'état affiché ----------

    /// Progression de l'animation « CPU vivant » : `Some(0.0..=1.0)` pendant la
    /// pulsation (0 = juste après le Step, 1 = fin), `None` sinon. Demande un
    /// repaint tant que ça anime.
    fn flash_progress(&self, ui: &egui::Ui) -> Option<f32> {
        if !self.animate {
            return None;
        }
        let elapsed = ui.input(|i| i.time) - self.flash_time;
        if !(0.0..FLASH_DUR).contains(&elapsed) {
            return None;
        }
        ui.ctx().request_repaint();
        Some((elapsed / FLASH_DUR) as f32)
    }

    fn snap(&self) -> Option<&Snapshot> {
        let d = self.dbg.as_ref()?;
        d.history.get(self.view_index.min(d.history.len().saturating_sub(1)))
    }
    fn prev_snap(&self) -> Option<&Snapshot> {
        let d = self.dbg.as_ref()?;
        let i = self.view_index.min(d.history.len().saturating_sub(1));
        d.history.get(i.saturating_sub(1))
    }
    fn is_head_view(&self) -> bool {
        matches!(&self.dbg, Some(d) if self.view_index >= d.history.len() - 1)
    }
    fn can_step(&self) -> bool {
        self.dbg.as_ref().is_some_and(|d| d.is_alive()) && self.is_head_view()
    }
    fn can_read_memory(&self) -> bool {
        self.is_head_view() && self.dbg.as_ref().is_some_and(|d| d.is_alive())
    }
    fn view_rip(&self) -> Option<u64> {
        self.snap().map(|s| s.regs.rip)
    }

    /// Ligne source (0-based) correspondant à RIP courant, pour l'éditeur.
    fn current_source_line(&self) -> Option<usize> {
        let rip = self.view_rip()?;
        self.src_map.get(&rip).map(|l| l.saturating_sub(1))
    }

    /// États (avant, après) de l'exécution de l'instruction à `addr`, retrouvés
    /// dans l'historique. `after` est `None` si l'instruction n'a pas encore été
    /// exécutée (ou est la dernière étape). Utilisé par le mode microscope.
    fn microscope_states(&self, addr: u64) -> Option<(&Snapshot, Option<&Snapshot>)> {
        let d = self.dbg.as_ref()?;
        let i = d.history.iter().position(|s| s.regs.rip == addr)?;
        Some((&d.history[i], d.history.get(i + 1)))
    }
    fn set_view(&mut self, idx: i64) {
        if let Some(d) = &self.dbg {
            self.view_index = idx.clamp(0, (d.history.len() - 1) as i64) as usize;
        }
    }

    /// Ajoute une infobulle (raccourci) seulement si l'option est activée.
    fn tip(&self, resp: egui::Response, text: &str) -> egui::Response {
        if self.show_tooltips {
            resp.on_hover_text(text)
        } else {
            resp
        }
    }

    // ---------- Palette de texte sensible au thème ----------
    // En thème clair, les couleurs « sombres » de la maquette deviennent
    // illisibles : on renvoie des variantes plus foncées.

    /// Couleur des titres de section / libellés secondaires.
    fn c_header(&self) -> Color32 {
        if self.dark { HEADER } else { Color32::from_rgb(0x3B, 0x4A, 0x63) }
    }
    /// Couleur des mnémoniques / accents bleus.
    fn c_mnemonic(&self) -> Color32 {
        if self.dark { MNEMONIC } else { Color32::from_rgb(0x1B, 0x5E, 0xA8) }
    }
    /// Couleur des adresses.
    fn c_addr(&self) -> Color32 {
        if self.dark { ADDR_COL } else { Color32::from_rgb(0x2A, 0x53, 0x86) }
    }
    /// Couleur des octets bruts / texte discret monospace.
    fn c_bytes(&self) -> Color32 {
        if self.dark { BYTES_COL } else { Color32::from_rgb(0x60, 0x64, 0x70) }
    }
    /// Fond de la ligne RIP dans le désassemblage.
    fn c_rip_row(&self) -> Color32 {
        if self.dark { RIP_ROW } else { Color32::from_rgb(0xFF, 0xEE, 0xB0) }
    }
    /// Fond d'une ligne sélectionnée / survolée dans le désassemblage.
    fn c_sel_row(&self) -> Color32 {
        if self.dark { SEL_ROW } else { Color32::from_rgb(0xD5, 0xE2, 0xF4) }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.icons.is_none() {
            self.icons = Some(Icons::load(ctx));
        }
        self.apply_theme(ctx);
        if self.pending_flash {
            self.flash_time = ctx.input(|i| i.time);
            self.pending_flash = false;
        }
        self.handle_shortcuts(ctx);

        self.menu_bar(ctx);
        self.toolbar(ctx);
        self.status_bar(ctx);

        // Marge interne unique pour TOUS les panneaux (bandes, latéraux, centre)
        // → rythme vertical cohérent et séparateurs d'en-tête alignés.
        let pad = egui::Margin::symmetric(8.0, 6.0);
        let band_frame = egui::Frame::central_panel(&ctx.style()).inner_margin(pad);

        // Bande basse : MEMORY | TIMELINE | CONSOLE.
        if self.show_bottom_band {
            egui::TopBottomPanel::bottom("bottom_band")
                .resizable(true)
                .default_height(196.0)
                .frame(band_frame)
                .show(ctx, |ui| {
                    let h = ui.available_height();
                    let cw = ((ui.available_width() - 20.0) / 3.0).max(60.0);
                    ui.horizontal_top(|ui| {
                        col(ui, cw, h, |ui| self.memory_ui(ui));
                        ui.separator();
                        col(ui, cw, h, |ui| self.timeline_col_ui(ui));
                        ui.separator();
                        col(ui, ui.available_width(), h, |ui| self.console_ui(ui));
                    });
                });
        }

        // Bande centrale : REGISTERS | STACK | CALL STACK | SYSCALLS.
        // (FLAGS est désormais au bas du panneau INSTRUCTION ; le désassemblage
        // a son propre onglet au centre.)
        if self.show_cpu_band {
            egui::TopBottomPanel::bottom("mid_band")
                .resizable(true)
                .default_height(226.0)
                .frame(band_frame)
                .show(ctx, |ui| {
                    let h = ui.available_height();
                    let cw = ((ui.available_width() - 30.0) / 4.0).max(90.0);
                    ui.horizontal_top(|ui| {
                        col(ui, cw * 1.4, h, |ui| self.registers_ui(ui));
                        ui.separator();
                        col(ui, cw * 1.3, h, |ui| self.stack_ui(ui));
                        ui.separator();
                        col(ui, cw * 0.9, h, |ui| self.callstack_ui(ui));
                        ui.separator();
                        col(ui, ui.available_width(), h, |ui| self.syscalls_ui(ui));
                    });
                });
        }

        // Explorateur à gauche, INSTRUCTION à droite, éditeur au centre.
        // Marge interne IDENTIQUE pour les trois → leurs en-têtes (et donc les
        // séparateurs sous EXPLORER / onglets éditeur / INSTRUCTION) s'alignent.
        if self.show_explorer {
            egui::SidePanel::left("explorer_panel")
                .resizable(true)
                .default_width(180.0)
                .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(pad))
                .show(ctx, |ui| self.explorer_ui(ui));
        }
        if self.show_instruction {
            egui::SidePanel::right("instruction_panel")
                .resizable(true)
                .default_width(272.0)
                .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(pad))
                .show(ctx, |ui| self.instruction_ui(ui));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(pad))
            .show(ctx, |ui| self.center_ui(ui));

        self.about_window(ctx);
        self.shortcuts_window(ctx);
        self.settings_window(ctx);
        self.microscope_window(ctx);
    }
}

impl App {
    // ---------- Raccourcis ----------

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::Key;
        // Ignore les raccourcis d'action quand l'éditeur a le focus (sauf Ctrl+*).
        let (step, run, stop, build, save, open, new, first, prev, next, last) = ctx.input(|i| {
            let c = i.modifiers.ctrl;
            (
                i.key_pressed(Key::F10) || i.key_pressed(Key::F8),
                i.key_pressed(Key::F5),
                i.key_pressed(Key::Escape) || (i.modifiers.shift && i.key_pressed(Key::F5)),
                c && i.key_pressed(Key::B),
                c && i.key_pressed(Key::S),
                c && i.key_pressed(Key::O),
                c && i.key_pressed(Key::N),
                i.key_pressed(Key::Home),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                i.key_pressed(Key::End),
            )
        });
        if save {
            self.save_source();
        }
        if open {
            self.open_browser();
        }
        if new {
            self.new_file();
        }
        if build {
            self.build();
        }
        if run {
            self.launch();
        }
        if stop {
            self.stop();
        }
        if step {
            self.step();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
            self.show_shortcuts = true;
        }
        // Affichage : Ctrl+1..4 bascule chaque panneau.
        let (t_expl, t_instr, t_cpu, t_bottom) = ctx.input(|i| {
            let c = i.modifiers.ctrl;
            (
                c && i.key_pressed(Key::Num1),
                c && i.key_pressed(Key::Num2),
                c && i.key_pressed(Key::Num3),
                c && i.key_pressed(Key::Num4),
            )
        });
        if t_expl {
            self.show_explorer = !self.show_explorer;
        }
        if t_instr {
            self.show_instruction = !self.show_instruction;
        }
        if t_cpu {
            self.show_cpu_band = !self.show_cpu_band;
        }
        if t_bottom {
            self.show_bottom_band = !self.show_bottom_band;
        }
        if t_expl || t_instr || t_cpu || t_bottom {
            self.save_settings();
        }
        // Timeline seulement si l'éditeur n'a pas le focus (évite le conflit ←/→).
        let editing = ctx.memory(|m| m.focused().is_some());
        if self.dbg.is_some() && !editing {
            if first {
                self.set_view(0);
            }
            if prev {
                self.set_view(self.view_index as i64 - 1);
            }
            if next {
                self.set_view(self.view_index as i64 + 1);
            }
            if last {
                self.set_view(i64::MAX);
            }
        }
    }

    // ---------- Boîtes de dialogue ----------

    /// Mode « microscope » : tout ce qui se passe pour UNE instruction.
    fn microscope_window(&mut self, ctx: &egui::Context) {
        let Some(addr) = self.microscope else { return };
        let Some(insn) = self.disasm.iter().find(|i| i.address == addr).cloned() else {
            self.microscope = None;
            return;
        };
        let flags_now = self.snap().map(|s| Flags::from_eflags(s.regs.eflags)).unwrap_or_default();
        let e = explain::explain(&insn.mnemonic, &insn.operands, flags_now);
        let cycles = explain::cycles_estimate(&insn.mnemonic);

        // Données dynamiques (avant/après) clonées => pas d'emprunt de self dans la closure.
        let dynamics = self.microscope_states(addr).map(|(b, a)| {
            (
                b.regs.clone(),
                b.stack.clone(),
                a.map(|s| (s.regs.clone(), s.stack.clone())),
            )
        });

        // Couleurs figées avant la closure (pas d'accès à self dedans).
        let (hdr, mnem_c, addr_c, bytes_c) =
            (self.c_header(), self.c_mnemonic(), self.c_addr(), self.c_bytes());
        let mut open = true;
        let mut close = false;
        egui::Window::new(format!("🔬 Microscope — {} {}", insn.mnemonic, insn.operands))
            .collapsible(false)
            .resizable(true)
            .default_width(580.0)
            .default_height(560.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().id_salt("microscope_scroll").show(ui, |ui| {
                    // --- Identité de l'instruction ---
                    egui::Grid::new("micro_id").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                        ui.label(RichText::new("Adresse").strong());
                        ui.label(RichText::new(format!("0x{:08X}", insn.address)).monospace().color(addr_c));
                        ui.end_row();
                        ui.label(RichText::new("Octets machine").strong());
                        ui.label(RichText::new(insn.bytes_hex()).monospace().color(bytes_c));
                        ui.end_row();
                        ui.label(RichText::new("Décodage").strong());
                        ui.label(
                            RichText::new(format!("{} {}", insn.mnemonic, insn.operands))
                                .monospace()
                                .color(mnem_c),
                        );
                        ui.end_row();
                        ui.label(RichText::new("Catégorie").strong());
                        ui.label(e.category);
                        ui.end_row();
                        ui.label(RichText::new("Cycles estimés").strong());
                        ui.label(RichText::new(cycles).color(CHANGED))
                            .on_hover_text("Ordre de grandeur pédagogique, pas une mesure exacte.");
                        ui.end_row();
                    });

                    ui.add_space(8.0);
                    ui.label(RichText::new("Que fait cette instruction ?").strong().color(hdr));
                    ui.label(&e.description);

                    ui.add_space(6.0);
                    ui.hyperlink_to(
                        format!("📖 Référence Intel de {} (felixcloutier.com)", insn.mnemonic.to_uppercase()),
                        explain::doc_url(&insn.mnemonic),
                    )
                    .on_hover_text("Ouvre la page de l'instruction dans le navigateur\n(mirror du manuel Intel SDM).");

                    ui.add_space(8.0);
                    ui.separator();

                    match &dynamics {
                        Some((before, _bstack, Some((after, _astack)))) => {
                            // ΔRSP + écriture/lecture pile.
                            let d = after.rsp as i128 - before.rsp as i128;
                            if d != 0 {
                                ui.label(RichText::new("Pile (RSP)").strong().color(hdr));
                                if d < 0 {
                                    ui.colored_label(
                                        PUSH_COL,
                                        format!(
                                            "RSP : 0x{:X} → 0x{:X}  (−{} octets, PUSH)",
                                            before.rsp, after.rsp, -d
                                        ),
                                    );
                                } else {
                                    ui.colored_label(
                                        POP_COL,
                                        format!(
                                            "RSP : 0x{:X} → 0x{:X}  (+{} octets, POP)",
                                            before.rsp, after.rsp, d
                                        ),
                                    );
                                }
                                ui.add_space(6.0);
                            }

                            // Registres modifiés.
                            ui.label(RichText::new("Registres modifiés").strong().color(hdr));
                            let mut any = false;
                            egui::Grid::new("micro_regs").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
                                for ((n, ov), (_, nv)) in
                                    before.named().iter().zip(after.named())
                                {
                                    if *ov != nv {
                                        any = true;
                                        ui.label(RichText::new(*n).monospace().strong());
                                        ui.label(RichText::new(format!("0x{ov:016X}")).monospace().weak());
                                        ui.label("→");
                                        ui.label(RichText::new(format!("0x{nv:016X}")).monospace().color(CHANGED));
                                        ui.end_row();
                                    }
                                }
                            });
                            if !any {
                                ui.weak("aucun registre modifié.");
                            }

                            ui.add_space(6.0);
                            // Flags modifiés.
                            ui.label(RichText::new("Flags").strong().color(hdr));
                            let (fb, fa) = (Flags::from_eflags(before.eflags), Flags::from_eflags(after.eflags));
                            let mut fchanged = false;
                            ui.horizontal_wrapped(|ui| {
                                for ((n, ov), (_, nv)) in fb.named().iter().zip(fa.named()) {
                                    if *ov != nv {
                                        fchanged = true;
                                        ui.label(
                                            RichText::new(format!("{n}: {}→{}", *ov as u8, nv as u8))
                                                .monospace()
                                                .color(CHANGED),
                                        );
                                    }
                                }
                            });
                            if !fchanged {
                                ui.weak("aucun flag modifié.");
                            }

                            ui.add_space(8.0);
                            // Schéma pile avant / après.
                            ui.label(RichText::new("Pile — avant / après").strong().color(hdr));
                            ui.columns(2, |c| {
                                micro_stack(&mut c[0], addr_c, "avant", before.rsp, _bstack);
                                micro_stack(&mut c[1], addr_c, "après", after.rsp, _astack);
                            });
                        }
                        Some((_before, _bstack, None)) => {
                            ui.weak(
                                "Instruction à exécuter à l'étape courante — avancez d'un pas (Step) \
                                 pour voir ses effets dynamiques.",
                            );
                            micro_static_flags(ui, hdr, &e);
                        }
                        None => {
                            ui.weak(
                                "Cette instruction n'a pas encore été exécutée dans l'historique \
                                 (effets dynamiques indisponibles).",
                            );
                            micro_static_flags(ui, hdr, &e);
                        }
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.vertical_centered(|ui| {
                        if ui.button("Fermer").clicked() {
                            close = true;
                        }
                    });
                });
            });
        if !open || close {
            self.microscope = None;
        }
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let mut open = true;
        egui::Window::new("À propos")
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading(RichText::new("ASM Studio").color(self.c_mnemonic()));
                    ui.label("IDE pédagogique NASM x86-64");
                });
                ui.add_space(8.0);
                ui.separator();
                egui::Grid::new("about_grid")
                    .num_columns(2)
                    .spacing([16.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Version");
                        ui.label(RichText::new(env!("CARGO_PKG_VERSION")).monospace().strong());
                        ui.end_row();
                        ui.label("Build");
                        ui.label(RichText::new(env!("GIT_HASH")).monospace().strong());
                        ui.end_row();
                        ui.label("Date");
                        ui.label(RichText::new(env!("BUILD_DATE")).monospace());
                        ui.end_row();
                        ui.label("Licence");
                        ui.hyperlink_to("MIT (explication)", "https://opensource.org/license/mit")
                            .on_hover_text("Ouvrir le texte officiel de la licence MIT");
                        ui.end_row();
                    });
                ui.separator();
                ui.add_space(6.0);
                ui.vertical_centered(|ui| {
                    if ui.button("Fermer").clicked() {
                        self.show_about = false;
                    }
                });
            });
        if !open {
            self.show_about = false;
        }
    }

    /// Applique le thème choisi (Système / Sombre / Clair) + le style moderne.
    fn apply_theme(&mut self, ctx: &egui::Context) {
        use egui::{FontId, Rounding, Theme, ThemePreference, TextStyle, vec2};
        let dark = match self.theme_pref {
            ThemePreference::Dark => true,
            ThemePreference::Light => false,
            ThemePreference::System => {
                ctx.input(|i| i.raw.system_theme) != Some(Theme::Light)
            }
        };
        self.dark = dark; // pour la palette de texte sensible au thème
        let mut style = (*ctx.style()).clone();
        let mut v = if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        v.window_rounding = Rounding::same(8.0);
        v.menu_rounding = Rounding::same(6.0);
        v.selection.bg_fill = ACCENT.linear_multiply(0.45);
        v.hyperlink_color = ACCENT;
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
            &mut v.widgets.noninteractive,
        ] {
            w.rounding = Rounding::same(5.0);
        }
        if dark {
            v.panel_fill = Color32::from_rgb(0x1E, 0x1E, 0x22);
            v.window_fill = Color32::from_rgb(0x25, 0x25, 0x2B);
            v.extreme_bg_color = Color32::from_rgb(0x17, 0x17, 0x1B);
            v.faint_bg_color = Color32::from_rgb(0x28, 0x28, 0x30);
        } else {
            // Thème clair : texte par défaut nettement sombre pour le contraste,
            // et fonds légèrement teintés pour délimiter les panneaux.
            v.override_text_color = Some(Color32::from_rgb(0x1C, 0x20, 0x28));
            v.panel_fill = Color32::from_rgb(0xF4, 0xF5, 0xF8);
            v.window_fill = Color32::from_rgb(0xFB, 0xFB, 0xFD);
            v.extreme_bg_color = Color32::from_rgb(0xFF, 0xFF, 0xFF);
            v.faint_bg_color = Color32::from_rgb(0xEA, 0xEC, 0xF1);
            v.hyperlink_color = Color32::from_rgb(0x1B, 0x5E, 0xA8);
        }
        style.visuals = v;
        style.spacing.item_spacing = vec2(8.0, 6.0);
        style.spacing.button_padding = vec2(9.0, 4.0);
        // Barres de défilement « solides » (réservent leur largeur) plutôt que
        // flottantes : elles ne se dessinent plus par-dessus le contenu.
        style.spacing.scroll = egui::style::ScrollStyle::solid();
        style.text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
        style.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
        style.text_styles.insert(TextStyle::Monospace, FontId::monospace(13.0));
        style.text_styles.insert(TextStyle::Heading, FontId::proportional(18.0));
        style.text_styles.insert(TextStyle::Small, FontId::proportional(11.0));
        ctx.set_style(style);
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        use egui::ThemePreference;
        let mut open = true;
        let mut changed = false;
        egui::Window::new("Réglages")
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(RichText::new("Thème").strong());
                ui.add_space(4.0);
                changed |= ui
                    .radio_value(&mut self.theme_pref, ThemePreference::System, "Système (suit l'OS)")
                    .changed();
                changed |= ui
                    .radio_value(&mut self.theme_pref, ThemePreference::Dark, "Sombre")
                    .changed();
                changed |= ui
                    .radio_value(&mut self.theme_pref, ThemePreference::Light, "Clair")
                    .changed();
                ui.add_space(4.0);
                ui.weak("Note : la coloration du code est optimisée pour le thème sombre.");
                ui.separator();

                ui.label(RichText::new("Interface").strong());
                ui.add_space(4.0);
                changed |= ui
                    .checkbox(
                        &mut self.show_tooltips,
                        "Afficher les infobulles des raccourcis (au survol des boutons)",
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.animate,
                        "Animations « CPU vivant » (pulsation des valeurs modifiées)",
                    )
                    .changed();
                ui.separator();

                ui.label(RichText::new("Bibliothèque asmstd").strong());
                ui.add_space(4.0);
                changed |= ui
                    .checkbox(
                        &mut self.use_asmstd,
                        "Activer asmstd (call asm.write, asm.exit, asm.mkdir…)",
                    )
                    .on_hover_text(
                        "Rend asmstd.inc disponible pour %include depuis n'importe quel fichier.\n\
                         Masque les numéros de syscalls derrière des noms lisibles.",
                    )
                    .changed();
                ui.weak("Dans le code : %include \"asmstd.inc\" puis call asm.write");
                ui.separator();

                ui.vertical_centered(|ui| {
                    if ui.button("Fermer").clicked() {
                        self.show_settings = false;
                    }
                });
            });
        if changed {
            self.save_settings();
        }
        if !open {
            self.show_settings = false;
        }
    }

    fn shortcuts_window(&mut self, ctx: &egui::Context) {
        if !self.show_shortcuts {
            return;
        }
        let mut open = true;
        egui::Window::new("Raccourcis clavier")
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                let rows = [
                    ("F1", "Aide / raccourcis"),
                    ("F5", "Lancer / Restart"),
                    ("F10 / F8", "Step (une instruction)"),
                    ("Échap / Maj+F5", "Stop"),
                    ("Ctrl+B", "Assembler + Lier"),
                    ("Ctrl+S", "Enregistrer"),
                    ("Ctrl+O", "Ouvrir"),
                    ("Ctrl+N", "Nouveau"),
                    ("← / →", "Timeline : précédent / suivant"),
                    ("Home / End", "Timeline : début / fin"),
                    ("Ctrl+1", "Afficher/masquer l'explorateur"),
                    ("Ctrl+2", "Afficher/masquer l'instruction"),
                    ("Ctrl+3", "Afficher/masquer la bande CPU"),
                    ("Ctrl+4", "Afficher/masquer la bande basse"),
                ];
                egui::Grid::new("shortcuts_grid")
                    .num_columns(2)
                    .spacing([24.0, 6.0])
                    .show(ui, |ui| {
                        for (k, d) in rows {
                            ui.label(RichText::new(k).monospace().strong().color(self.c_mnemonic()));
                            ui.label(d);
                            ui.end_row();
                        }
                    });
                ui.separator();
                ui.vertical_centered(|ui| {
                    if ui.button("Fermer").clicked() {
                        self.show_shortcuts = false;
                    }
                });
            });
        if !open {
            self.show_shortcuts = false;
        }
    }

    // ---------- Menu ----------

    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menubar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Fichier", |ui| {
                    if ui.button("Nouveau            Ctrl+N").clicked() {
                        self.new_file();
                        ui.close_menu();
                    }
                    if ui.button("Ouvrir…            Ctrl+O").clicked() {
                        self.open_browser();
                        ui.close_menu();
                    }
                    if ui.button("Enregistrer        Ctrl+S").clicked() {
                        self.save_source();
                        ui.close_menu();
                    }
                    if ui.button("Enregistrer sous…").clicked() {
                        self.open_saveas();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quitter").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Build", |ui| {
                    if ui.button("Assembler + Lier   Ctrl+B").clicked() {
                        self.build();
                        ui.close_menu();
                    }
                    if ui.button("Exécuter (Lancer)  F5").clicked() {
                        self.launch();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Debug", |ui| {
                    if ui.button("Lancer / Restart   F5").clicked() {
                        self.launch();
                        ui.close_menu();
                    }
                    if ui.button("Step               F10").clicked() {
                        self.step();
                        ui.close_menu();
                    }
                    if ui.button("Stop               Échap").clicked() {
                        self.stop();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Affichage", |ui| {
                    ui.label(RichText::new("Panneaux").small().weak());
                    let mut changed = false;
                    changed |= ui.checkbox(&mut self.show_explorer, "Explorateur          Ctrl+1").changed();
                    changed |= ui.checkbox(&mut self.show_instruction, "Instruction          Ctrl+2").changed();
                    changed |= ui.checkbox(&mut self.show_cpu_band, "Bande CPU (registres…)  Ctrl+3").changed();
                    changed |= ui.checkbox(&mut self.show_bottom_band, "Bande basse (mémoire…)  Ctrl+4").changed();
                    ui.separator();
                    if ui.button("Tout afficher").clicked() {
                        self.show_explorer = true;
                        self.show_instruction = true;
                        self.show_cpu_band = true;
                        self.show_bottom_band = true;
                        changed = true;
                        ui.close_menu();
                    }
                    if changed {
                        self.save_settings();
                    }
                });
                ui.menu_button("Aide", |ui| {
                    if ui.button("Réglages…").clicked() {
                        self.show_settings = true;
                        ui.close_menu();
                    }
                    if ui.button("Raccourcis clavier…").clicked() {
                        self.show_shortcuts = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("À propos ASM Studio…").clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                });
            });
        });
    }

    // ---------- Barre d'outils ----------

    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                let running = self.dbg.as_ref().is_some_and(|d| d.is_alive());
                let can_step = self.can_step();
                // Handles clonés (Arc bon marché) => pas d'emprunt de self dans la barre.
                let ic = |f: fn(&Icons) -> &egui::TextureHandle| self.icons.as_ref().map(|i| f(i).clone());
                let (ic_run, ic_debug, ic_build) = (ic(|i| &i.run), ic(|i| &i.debug), ic(|i| &i.assembler));
                let (ic_pause, ic_next, ic_stop) = (ic(|i| &i.pause), ic(|i| &i.next), ic(|i| &i.stop));
                let (ic_restart, ic_attach) = (ic(|i| &i.restart), ic(|i| &i.attach));

                // Run : accent quand inactif, grisé quand un programme tourne.
                if self
                    .tip(accent_button(ui, ic_run.as_ref(), "Run", !running), "Lancer (F5)")
                    .clicked()
                {
                    self.launch();
                }
                // Pause : non implémenté (step-by-step uniquement), toujours grisé.
                ui.add_enabled(false, icon_btn_widget(ic_pause.as_ref(), "Pause"));
                // Step : accent quand disponible.
                if self
                    .tip(accent_button(ui, ic_debug.as_ref(), "Step", can_step), "Pas à pas (F10)")
                    .clicked()
                {
                    self.step();
                }
                // Next (step-over) : même comportement que Step pour l'instant.
                if self
                    .tip(
                        ui.add_enabled(can_step, icon_btn_widget(ic_next.as_ref(), "Next")),
                        "Passer l'appel (non implémenté — agit comme Step)",
                    )
                    .clicked()
                {
                    self.step();
                }
                // Stop.
                if self.tip(bordered_button(ui, ic_stop.as_ref(), "Stop", running), "Arrêter (Échap)").clicked() {
                    self.stop();
                }
                // Restart = relancer depuis le début.
                if self
                    .tip(icon_button(ui, ic_restart.as_ref(), "Restart"), "Relancer (F5)")
                    .clicked()
                {
                    self.launch();
                }
                ui.separator();
                if self
                    .tip(icon_button(ui, ic_build.as_ref(), "Build"), "Assembler + Lier (Ctrl+B)")
                    .clicked()
                {
                    self.build();
                }
                // Attach : non implémenté.
                ui.add_enabled(false, icon_btn_widget(ic_attach.as_ref(), "Attach"));
                // (Réglages : accessible via le menu Aide — pas de doublon ici.)
            });
            ui.add_space(3.0);
        });
    }

    // ---------- Barre d'état ----------

    fn status_bar(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("statusbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                match &self.dbg {
                    Some(d) if d.is_alive() => {
                        ui.colored_label(FLAG_ON, "● Running");
                        ui.separator();
                        ui.label(format!("PID {}", d.pid()));
                    }
                    Some(d) => {
                        let msg = match d.state {
                            RunState::Exited(c) => format!("○ Exited ({c})"),
                            _ => "○ Terminé".to_string(),
                        };
                        ui.colored_label(FLAG_OFF, msg);
                    }
                    None => {
                        ui.colored_label(FLAG_OFF, "○ Prêt");
                    }
                }
                ui.separator();
                ui.label(RichText::new("Arch : x86_64").color(self.c_header()));
                ui.separator();
                ui.label(RichText::new("Mode : 64-bit").color(self.c_header()));
                if let Some(s) = self.snap() {
                    ui.separator();
                    ui.label(format!("Arrêté à : 0x{:X}", s.regs.rip));
                    if let Some(next) = self.next_addr() {
                        ui.separator();
                        ui.colored_label(CHANGED, format!("Suivant : 0x{next:X}"));
                    }
                }
                // À droite : position curseur, encodage, syntaxe.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("NASM").color(ACCENT).strong());
                    ui.separator();
                    ui.label(RichText::new("UTF-8").color(self.c_header()));
                    ui.separator();
                    ui.label(
                        RichText::new(format!("Ln {}, Col {}", self.editor_ln, self.editor_col))
                            .color(self.c_header()),
                    );
                });
            });
        });
    }

    fn next_addr(&self) -> Option<u64> {
        let rip = self.view_rip()?;
        let idx = self.disasm.iter().position(|i| i.address == rip)?;
        self.disasm.get(idx + 1).map(|i| i.address)
    }

    // ---------- Bande basse ----------

    /// Timeline en colonne (bande basse), style mockup.
    fn timeline_col_ui(&mut self, ui: &mut egui::Ui) {
        header_icon(ui, self.c_header(), self.icons.as_ref().map(|i| &i.timeline), "TIMELINE");
        let Some(last) = self.dbg.as_ref().map(|d| d.history.len() - 1) else {
            ui.weak("— lancez un programme");
            return;
        };
        // Pastilles numérotées (façon mockup) si peu d'étapes ; sinon slider.
        let mut goto: Option<usize> = None;
        if last <= 60 {
            egui::ScrollArea::horizontal().id_salt("timeline_dots").max_height(30.0).show(ui, |ui| {
                ui.horizontal(|ui| {
                    for i in 0..=last {
                        let cur = i == self.view_index;
                        let txt = RichText::new(format!("{i}")).monospace().size(11.0).color(
                            if cur { Color32::WHITE } else { self.c_header() },
                        );
                        let mut btn = egui::Button::new(txt).min_size(egui::vec2(22.0, 22.0)).rounding(egui::Rounding::same(11.0));
                        if cur {
                            btn = btn.fill(ACTION);
                        }
                        if ui.add(btn).clicked() {
                            goto = Some(i);
                        }
                    }
                });
            });
        } else {
            let mut idx = self.view_index;
            ui.spacing_mut().slider_width = (ui.available_width() - 16.0).max(80.0);
            if ui.add(egui::Slider::new(&mut idx, 0..=last).show_value(false)).changed() {
                goto = Some(idx);
            }
        }
        if let Some(i) = goto {
            self.set_view(i as i64);
        }

        // Étape courante : « Instruction N/last : mnémonique ».
        if let Some(s) = self.snap()
            && let Some(insn) = self.disasm.iter().find(|i| i.address == s.regs.rip)
        {
            ui.label(RichText::new(format!("Instruction {}/{last}", self.view_index)).strong());
            ui.label(
                RichText::new(format!("{} {}", insn.mnemonic, insn.operands))
                    .monospace()
                    .color(self.c_mnemonic()),
            );
        }

        // Contrôles de lecture (⏮ ⏪ ▶ ⏩ ⏭).
        ui.horizontal(|ui| {
            if self.tip(ui.button("⏮"), "Début (Home)").clicked() {
                self.set_view(0);
            }
            if self.tip(ui.button("⏪"), "Précédent (←)").clicked() {
                self.set_view(self.view_index as i64 - 1);
            }
            if self.tip(ui.button("▶"), "Suivant (→)").clicked() {
                self.set_view(self.view_index as i64 + 1);
            }
            if self.tip(ui.button("⏩"), "Suivant (→)").clicked() {
                self.set_view(self.view_index as i64 + 1);
            }
            if self.tip(ui.button("⏭"), "Fin (End)").clicked() {
                self.set_view(i64::MAX);
            }
        });
        if !self.is_head_view()
            && self.tip(ui.button("⟳ Reprendre ici"), "Ré-exécute jusqu'à cette étape").clicked()
        {
            self.resume_here();
        }
    }

    fn memory_ui(&mut self, ui: &mut egui::Ui) {
        // Régions utiles pour le sélecteur (calculées avant l'UI, sans emprunt).
        let regions: Vec<(&str, u64)> = match self.dbg.as_ref().filter(|d| d.is_alive()) {
            Some(d) => {
                let mut v = vec![("Pile (RSP)", d.regs().rsp), ("Base (RBP)", d.regs().rbp)];
                if let Some((h0, _)) = d.heap_range() {
                    v.push(("Tas (heap)", h0));
                }
                v
            }
            None => Vec::new(),
        };
        let mut pick: Option<u64> = None;
        let mem_ic = self.icons.as_ref().map(|i| i.memory.clone());
        let hdr = self.c_header();
        panel_header(ui, |ui| {
            header_title(ui, hdr, mem_ic.as_ref(), "MEMORY");
            // Sélecteur de région (façon mockup).
            egui::ComboBox::from_id_salt("mem_region")
                .selected_text(RichText::new(format!("0x{:012X}..", self.mem_addr)).monospace())
                .show_ui(ui, |ui| {
                    for (label, addr) in &regions {
                        if ui.selectable_label(false, format!("{label}  ·  0x{addr:X}")).clicked() {
                            pick = Some(*addr);
                        }
                    }
                    if regions.is_empty() {
                        ui.weak("(lancez un programme)");
                    }
                });
            ui.label("aller @");
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.mem_input)
                    .desired_width(130.0)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("0x402000"),
            );
            let go = ui.button("Aller").clicked()
                || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if go {
                match parse_hex(&self.mem_input) {
                    Some(a) => {
                        self.mem_addr = a;
                        self.status = format!("Mémoire @ 0x{a:X}");
                    }
                    None => self.status = "Adresse hexa invalide".to_string(),
                }
            }
        });
        if let Some(a) = pick {
            self.mem_addr = a;
            self.mem_input = format!("0x{a:X}");
        }
        if !self.can_read_memory() {
            let msg = match self.dbg.as_ref().map(|d| d.is_alive()) {
                Some(false) => "Programme terminé — relancez pour explorer la mémoire.",
                Some(true) => "Revenez à la dernière étape de la timeline pour lire la mémoire.",
                None => "Lancez un programme pour explorer la mémoire.",
            };
            ui.weak(msg);
            return;
        }

        // Laboratoire mémoire : écrire des octets à l'adresse de base affichée.
        ui.horizontal(|ui| {
            ui.label(RichText::new("✎ écrire @ base :").small());
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.mem_poke)
                    .desired_width(150.0)
                    .font(egui::TextStyle::Monospace)
                    .hint_text("48 65 6C…"),
            );
            let write = ui.button("Écrire").clicked()
                || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if write {
                match parse_hex_bytes(&self.mem_poke) {
                    Some(bytes) if !bytes.is_empty() => {
                        let addr = self.mem_addr;
                        match self.dbg.as_mut().unwrap().write_mem(addr, &bytes) {
                            Ok(_) => {
                                self.status = format!("{} octet(s) écrit(s) @ 0x{addr:X}", bytes.len());
                                self.mem_poke.clear();
                            }
                            Err(e) => self.log(&e),
                        }
                    }
                    _ => self.status = "Octets hexa invalides (ex. « 48 65 6C »)".to_string(),
                }
            }
        });
        ui.separator();

        let (addr_c, bytes_c) = (self.c_addr(), self.c_bytes());
        let dbg = self.dbg.as_ref().unwrap();
        egui::ScrollArea::both()
            .id_salt("mem_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| hex_dump_rows(ui, addr_c, bytes_c, dbg, self.mem_addr, 8));
    }

    fn console_ui(&mut self, ui: &mut egui::Ui) {
        let console_ic = self.icons.as_ref().map(|i| i.console.clone());
        let hdr = self.c_header();
        panel_header(ui, |ui| {
            header_title(ui, hdr, console_ic.as_ref(), "CONSOLE");
            if ui.small_button("effacer").clicked() {
                self.console.clear();
            }
        });
        egui::ScrollArea::vertical()
            .id_salt("console_scroll")
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(RichText::new(&self.console).monospace())
                        .selectable(true)
                        .wrap(),
                );
            });
    }

    // ---------- Registres + Flags ----------

    /// (nom, valeur, valeur précédente) des registres à afficher.
    fn reg_rows(&self) -> Option<Vec<(&'static str, u64, u64)>> {
        let snap = self.snap()?;
        let prev = self.prev_snap()?;
        Some(
            snap.regs
                .named()
                .iter()
                .zip(prev.regs.named())
                .map(|((n, v), (_, p))| (*n, *v, p))
                .collect(),
        )
    }

    fn registers_ui(&mut self, ui: &mut egui::Ui) {
        header_icon(ui, self.c_header(), self.icons.as_ref().map(|i| &i.registers), "REGISTERS");
        let Some(rows) = self.reg_rows() else {
            ui.label("Aucun programme lancé.");
            return;
        };
        // Édition possible seulement quand le processus est vivant et en pause à
        // la dernière étape (ptrace ne peut pas écrire dans un process terminé).
        let editable = self.can_step();
        let hint = if editable {
            "clic sur une valeur pour l'éditer"
        } else if self.dbg.as_ref().is_some_and(|d| !d.is_alive()) {
            "édition indisponible (programme terminé — relancez)"
        } else {
            "édition à la dernière étape (revenez en fin de timeline)"
        };
        ui.label(RichText::new(hint).small().weak());
        let flash = self.flash_progress(ui); // pulsation « CPU vivant »
        let mut commit: Option<(&'static str, u64)> = None;
        let mut stop_edit = false;

        let hdr = self.c_header();
        egui::ScrollArea::vertical()
            .id_salt("regs_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Deux registres par ligne (grille à 4 colonnes) : tout tient sans
                // ascenseur et la largeur du panneau est mieux exploitée.
                egui::Grid::new("regs_grid").num_columns(4).spacing([20.0, 6.0]).show(ui, |ui| {
                    for (i, (name, val, pval)) in rows.iter().enumerate() {
                        let (name, val, pval) = (*name, *val, *pval);
                        ui.label(RichText::new(name).monospace().strong().color(hdr));
                        if self.edit_reg == Some(name) {
                            // Édition : champ hexa + ✓ (valider) / ✗ (annuler).
                            let focus_now = std::mem::take(&mut self.edit_focus);
                            let buf = &mut self.edit_buf;
                            let mut committed: Option<u64> = None;
                            let mut ended = false;
                            ui.horizontal(|ui| {
                                let resp = ui.add(
                                    egui::TextEdit::singleline(buf)
                                        .desired_width(96.0)
                                        .font(egui::TextStyle::Monospace)
                                        .hint_text("hex"),
                                );
                                if focus_now {
                                    resp.request_focus();
                                }
                                let enter =
                                    resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                if ui.small_button("✓").clicked() || enter {
                                    committed = parse_hex(buf);
                                    ended = true;
                                }
                                if ui.small_button("✗").clicked() {
                                    ended = true;
                                }
                            });
                            if let Some(v) = committed {
                                commit = Some((name, v));
                            }
                            if ended {
                                stop_edit = true;
                            }
                        } else {
                            let mut t = RichText::new(format!("0x{val:016X}")).monospace();
                            if val != pval {
                                t = t.color(changed_color(flash));
                            }
                            if editable {
                                let resp = ui
                                    .add(egui::Label::new(t).sense(egui::Sense::click()))
                                    .on_hover_text("Cliquer pour modifier");
                                if resp.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                if resp.clicked() {
                                    self.edit_reg = Some(name);
                                    self.edit_buf = format!("{val:X}");
                                    self.edit_focus = true;
                                }
                            } else {
                                ui.label(t);
                            }
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            });

        // Applique l'édition après le rendu (évite l'emprunt simultané de dbg).
        if let Some((name, v)) = commit {
            self.edit_reg = None;
            match self.dbg.as_mut().unwrap().set_register(name, v) {
                Ok(_) => self.status = format!("{name} = 0x{v:X}"),
                Err(e) => self.log(&e),
            }
        } else if stop_edit {
            self.edit_reg = None;
        }
    }

    fn flags_ui(&self, ui: &mut egui::Ui) {
        header(ui, self.c_header(), "FLAGS");
        let (Some(snap), Some(prev)) = (self.snap(), self.prev_snap()) else {
            ui.weak("—");
            return;
        };
        let flash = self.flash_progress(ui);
        let flags = Flags::from_eflags(snap.regs.eflags);
        let prevf = Flags::from_eflags(prev.regs.eflags);
        // Disposition horizontale compacte (grille 3×2) : tient sans ascenseur
        // dans le panneau étroit. Chaque cellule = « NOM ● valeur ».
        const PER_ROW: usize = 3;
        egui::Grid::new("flags_grid").num_columns(PER_ROW).spacing([16.0, 6.0]).show(ui, |ui| {
            for (i, ((name, val), (_, pval))) in
                flags.named().iter().zip(prevf.named()).enumerate()
            {
                let changed = *val != pval;
                let dot = if changed {
                    changed_color(flash)
                } else if *val {
                    FLAG_ON
                } else {
                    FLAG_OFF
                };
                ui.horizontal(|ui| {
                    let mut nm = RichText::new(*name).monospace().strong();
                    if changed {
                        nm = nm.color(changed_color(flash));
                    }
                    ui.label(nm);
                    ui.label(RichText::new("●").color(dot).size(10.0));
                    ui.label(
                        RichText::new(if *val { "1" } else { "0" })
                            .monospace()
                            .strong()
                            .color(if *val { FLAG_ON } else { FLAG_OFF }),
                    );
                });
                if (i + 1) % PER_ROW == 0 {
                    ui.end_row();
                }
            }
        });
    }

    // ---------- Explorateur de fichiers (panneau de gauche) ----------

    fn explorer_ui(&mut self, ui: &mut egui::Ui) {
        header_icon(ui, self.c_header(), self.icons.as_ref().map(|i| &i.explorer), "EXPLORER");

        // Barre : nom du dossier racine + remonter d'un cran.
        let mut go_up = false;
        ui.horizontal(|ui| {
            if self
                .tip(ui.small_button("⬆"), "Dossier parent comme racine")
                .clicked()
            {
                go_up = true;
            }
            let root = self
                .explorer_dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.explorer_dir.display().to_string());
            ui.label(RichText::new(root).strong().color(self.c_header()))
                .on_hover_text(self.explorer_dir.display().to_string());
        });
        if go_up && let Some(p) = self.explorer_dir.parent() {
            self.explorer_dir = p.to_path_buf();
        }
        ui.separator();

        // Arbre de fichiers (dossiers repliables + fichiers cliquables).
        let asm_col = self.c_mnemonic();
        let other_col = self.c_bytes();
        let cur = self.src_path.clone();
        let mut to_open = None;
        egui::ScrollArea::both().id_salt("explorer_scroll").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().indent = 14.0;
            let root = self.explorer_dir.clone();
            dir_tree(ui, &root, &cur, asm_col, other_col, &mut to_open);
        });
        if let Some(f) = to_open {
            self.open_file(f);
        }
    }

    // ---------- Call stack ----------

    fn callstack_ui(&self, ui: &mut egui::Ui) {
        header_icon(ui, self.c_header(), self.icons.as_ref().map(|i| &i.callstack), "CALL STACK");
        if self.dbg.is_none() {
            ui.weak("—");
            return;
        }
        egui::ScrollArea::vertical().id_salt("callstack_scroll").auto_shrink([false, false]).show(ui, |ui| {
            // Frame courante en haut (RIP), puis les retours empilés.
            let mut depth = self.call_stack.len();
            if let Some(rip) = self.view_rip() {
                ui.label(RichText::new(format!("#{depth}  0x{rip:08X}  (courant)")).monospace().color(CHANGED));
            }
            for addr in self.call_stack.iter().rev() {
                depth = depth.saturating_sub(1);
                ui.label(RichText::new(format!("#{depth}  0x{addr:08X}")).monospace().color(self.c_addr()));
            }
            if self.call_stack.is_empty() {
                ui.weak("(aucun appel en cours)");
            }
        });
    }

    // ---------- Syscalls ----------

    fn syscalls_ui(&self, ui: &mut egui::Ui) {
        header_icon(ui, self.c_header(), self.icons.as_ref().map(|i| &i.syscalls), "SYSCALLS");
        egui::ScrollArea::vertical()
            .id_salt("syscalls_scroll")
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.syscalls.is_empty() {
                    ui.weak("(aucun appel système)");
                }
                for s in &self.syscalls {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&s.name).monospace().strong().color(self.c_mnemonic()));
                        ui.label(RichText::new(format!("#{}", s.number)).monospace().weak().small());
                        match s.ret {
                            Some(r) if r < 0 => badge(ui, "ERREUR", FALSE_COL),
                            Some(_) => badge(ui, "SUCCESS", FLAG_ON),
                            None => badge(ui, "PENDING", self.c_header()),
                        }
                    });
                    // Arguments sur une ligne compacte.
                    if !s.args.is_empty() {
                        ui.label(RichText::new(format!("  {}", s.args)).monospace().small().weak());
                    }
                    // Valeur de retour.
                    match s.ret {
                        Some(r) if r < 0 => {
                            ui.label(
                                RichText::new(format!("  ret  {r}  (errno)"))
                                    .monospace()
                                    .small()
                                    .color(FALSE_COL),
                            );
                        }
                        Some(r) => {
                            ui.label(
                                RichText::new(format!("  ret  {r}"))
                                    .monospace()
                                    .small()
                                    .color(FLAG_ON),
                            );
                        }
                        None => {}
                    }
                    ui.add_space(2.0);
                }
            });
    }

    // ---------- Centre : onglets Éditeur / Désassemblage ----------

    fn center_ui(&mut self, ui: &mut egui::Ui) {
        let hdr = self.c_header();
        let (edit_ic, disasm_ic) = match &self.icons {
            Some(i) => (Some(i.editor.clone()), Some(i.assembler.clone())),
            None => (None, None),
        };
        panel_header(ui, |ui| {
            if icon_tab(ui, edit_ic.as_ref(), "Éditeur", self.tab == Tab::Editor).clicked() {
                self.tab = Tab::Editor;
            }
            if icon_tab(ui, disasm_ic.as_ref(), "Désassemblage", self.tab == Tab::Disasm).clicked() {
                self.tab = Tab::Disasm;
            }
            ui.separator();
            let name = self.src_path.file_name().unwrap_or_default().to_string_lossy();
            let mark = if self.dirty { " ●" } else { "" };
            ui.label(RichText::new(format!("{name}{mark}")).color(hdr));
        });
        // Bandeau RIP (façon mockup) : « RIP : 0x… mnémonique opérandes ».
        if let Some(s) = self.snap()
            && let Some(insn) = self.disasm.iter().find(|i| i.address == s.regs.rip)
        {
            ui.horizontal(|ui| {
                ui.label(RichText::new("▶").color(ACTION));
                ui.label(RichText::new(format!("RIP : 0x{:X}", s.regs.rip)).monospace().color(self.c_addr()));
                ui.label(
                    RichText::new(format!("{} {}", insn.mnemonic, insn.operands))
                        .monospace()
                        .color(self.c_mnemonic()),
                );
            });
        }
        ui.add_space(2.0);
        match self.tab {
            Tab::Editor => self.editor_ui(ui),
            Tab::Disasm => self.disasm_ui(ui),
        }
    }

    fn editor_ui(&mut self, ui: &mut egui::Ui) {
        // Ligne source courante (RIP) à surligner pendant le débogage.
        let hl = self.current_source_line();
        let dark = self.dark;

        // Coloration syntaxique NASM (retour à la ligne désactivé => aligné aux numéros).
        let mut layouter = |ui: &egui::Ui, text: &str, _wrap: f32| {
            ui.fonts(|f| f.layout_job(syntax::highlight(text, dark, hl)))
        };

        // Gouttière : numéros de ligne (▶ + accent sur la ligne courante).
        let line_count = self.source.matches('\n').count() + 1;
        let width = line_count.to_string().len();
        let gfont = egui::FontId::monospace(syntax::FONT_SIZE);
        let mut gutter_job = egui::text::LayoutJob::default();
        for i in 1..=line_count {
            if i > 1 {
                gutter_job.append("\n", 0.0, egui::TextFormat::default());
            }
            let is_cur = hl == Some(i - 1);
            let (marker, color) = if is_cur { ("▶", ACCENT) } else { (" ", GUTTER) };
            gutter_job.append(
                &format!("{marker}{i:>width$}"),
                0.0,
                egui::TextFormat {
                    font_id: gfont.clone(),
                    color,
                    ..Default::default()
                },
            );
        }

        // Largeur du contenu = ligne la plus longue (pour le scroll horizontal).
        let char_w = ui.fonts(|f| f.glyph_width(&gfont, 'M'));
        let max_cols = self.source.lines().map(|l| l.chars().count()).max().unwrap_or(0);
        let content_w = (max_cols as f32 + 2.0) * char_w;

        ui.horizontal_top(|ui| {
            // Gouttière : défilement vertical synchronisé, sans barre ni scroll direct.
            egui::ScrollArea::vertical()
                .id_salt("gutter_scroll")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .enable_scrolling(false)
                .auto_shrink([true, false])
                .vertical_scroll_offset(self.editor_scroll_y)
                .show(ui, |ui| {
                    let galley = ui.fonts(|f| f.layout_job(gutter_job));
                    ui.add(egui::Label::new(galley).selectable(false));
                });
            ui.separator();
            // Éditeur : défilement vertical + horizontal ; la gouttière reste fixe.
            let out = egui::ScrollArea::both()
                .id_salt("editor_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let out = egui::TextEdit::multiline(&mut self.source)
                        .frame(false)
                        .code_editor()
                        .desired_width(content_w.max(ui.available_width()))
                        .desired_rows(28)
                        .lock_focus(true)
                        .layouter(&mut layouter)
                        .show(ui);
                    if out.response.changed() {
                        self.dirty = true;
                    }
                    // Position du curseur (Ln/Col) pour la barre d'état.
                    if let Some(range) = out.cursor_range {
                        let p = range.primary.pcursor;
                        self.editor_ln = p.paragraph + 1;
                        self.editor_col = p.offset + 1;
                    }
                });
            // Synchronise la gouttière sur le défilement vertical de l'éditeur.
            self.editor_scroll_y = out.state.offset.y;
        });
    }

    fn disasm_ui(&mut self, ui: &mut egui::Ui) {
        if self.disasm.is_empty() {
            ui.label("Cliquez sur « Lancer » pour assembler, lier et exécuter votre programme.");
            return;
        }
        let rip = self.view_rip();
        let mut clicked: Option<u64> = None;
        egui::ScrollArea::vertical().id_salt("disasm_scroll").show(ui, |ui| {
            for insn in &self.disasm {
                let is_current = Some(insn.address) == rip;
                let is_selected = Some(insn.address) == self.selected;
                // Forme de fond réservée AVANT le contenu => dessinée derrière le
                // texte (sinon le rectangle masquerait l'instruction).
                let bg = ui.painter().add(egui::Shape::Noop);
                let inner = ui.horizontal(|ui| {
                    if is_current {
                        ui.label(RichText::new("➤").color(CHANGED));
                    } else {
                        ui.label("    ");
                    }
                    ui.label(RichText::new(format!("0x{:08X}", insn.address)).monospace().color(self.c_addr()));
                    ui.label(RichText::new(format!("{:<20}", insn.bytes_hex())).monospace().color(self.c_bytes()));
                    ui.label(RichText::new(format!("{:<7}", insn.mnemonic)).monospace().color(self.c_mnemonic()));
                    ui.label(RichText::new(&insn.operands).monospace());
                });
                let row = inner.response.interact(egui::Sense::click());
                if row.clicked() {
                    clicked = Some(insn.address);
                }
                let fill = if is_current {
                    Some(self.c_rip_row())
                } else if is_selected {
                    Some(self.c_sel_row())
                } else if row.hovered() {
                    Some(self.c_sel_row().linear_multiply(0.5))
                } else {
                    None
                };
                if let Some(color) = fill {
                    let rect = row.rect.expand2(egui::vec2(0.0, 2.0));
                    ui.painter().set(bg, egui::Shape::rect_filled(rect, 3.0, color));
                }
            }
        });
        if let Some(addr) = clicked {
            self.selected = if self.selected == Some(addr) { None } else { Some(addr) };
        }
    }

    // ---------- Panneau INSTRUCTION ----------

    fn instruction_ui(&mut self, ui: &mut egui::Ui) {
        let bulb_ic = self.icons.as_ref().map(|i| i.instruction.clone());
        let hdr = self.c_header();
        panel_header(ui, |ui| {
            header_title(ui, hdr, None, "INSTRUCTION");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                icon_img(ui, bulb_ic.as_ref(), 16.0);
            });
        });

        // FLAGS épinglé au bas du panneau INSTRUCTION (le cadre par défaut du
        // panneau dessine le trait de séparation avec le contenu au-dessus).
        egui::TopBottomPanel::bottom("instr_flags")
            .resizable(false)
            .show_inside(ui, |ui| self.flags_ui(ui));

        let target = self.selected.or_else(|| self.view_rip());
        let Some(addr) = target else {
            ui.label("Lancez le programme, puis cliquez une instruction.");
            return;
        };
        let Some(insn) = self.disasm.iter().find(|i| i.address == addr).cloned() else {
            ui.label("—");
            return;
        };
        let flags = self.snap().map(|s| Flags::from_eflags(s.regs.eflags)).unwrap_or_default();
        let e = explain::explain(&insn.mnemonic, &insn.operands, flags);
        let mnem_col = self.c_mnemonic();

        // Ligne 1 : nom de l'instruction + bouton Microscope (aligné à droite).
        ui.horizontal(|ui| {
            ui.label(RichText::new(&e.title).size(16.0).strong().color(mnem_col));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("🔬 Microscope")
                    .on_hover_text("Tout voir sur cette seule instruction")
                    .clicked()
                {
                    self.microscope = Some(addr);
                }
            });
        });
        // Ligne 2 : catégorie + repère (instruction courante / sélection) à droite.
        ui.horizontal(|ui| {
            ui.label(RichText::new(e.category).italics().weak().size(12.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let tag = if self.selected.is_some() {
                    "(sélection)"
                } else {
                    "(instruction courante)"
                };
                ui.label(RichText::new(tag).small().weak());
            });
        });

        // Description pédagogique + lien vers la référence officielle.
        ui.add_space(8.0);
        card(ui, |ui| {
            ui.label(RichText::new(&e.description).size(13.0));
        });
        ui.add_space(6.0);
        ui.hyperlink_to(
            format!("📖 Référence Intel de {} ↗", insn.mnemonic.to_uppercase()),
            explain::doc_url(&insn.mnemonic),
        )
        .on_hover_text("Ouvre la page de l'instruction (manuel Intel SDM, felixcloutier.com)");

        if let Some(cond) = &e.condition {
            ui.add_space(4.0);
            ui.label(RichText::new("Condition").strong());
            ui.label(RichText::new(cond).monospace());
            // Effet : où mène le saut si la condition est vraie.
            if !insn.operands.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new("Effet").strong());
                ui.label(
                    RichText::new(format!(
                        "Si la condition est vraie, RIP = {}.",
                        insn.operands
                    ))
                    .monospace(),
                );
            }
            ui.add_space(4.0);
            let hdr2 = self.c_header();
            card(ui, |ui| {
                    ui.label(RichText::new("État actuel").small().strong().color(hdr2));
                    ui.horizontal(|ui| {
                        for (name, val) in &e.relevant_flags {
                            let c = if *val { FLAG_ON } else { FLAG_OFF };
                            ui.label(
                                RichText::new(format!("{name} = {}", *val as u8))
                                    .monospace()
                                    .color(c),
                            );
                        }
                    });
                    if let Some(taken) = e.taken {
                        ui.add_space(4.0);
                        let (txt, col) = if taken {
                            ("✔ Condition vraie — le saut sera pris.", FLAG_ON)
                        } else {
                            ("✘ Condition fausse — pas de saut.", FALSE_COL)
                        };
                        let fill = if taken {
                            FLAG_ON.linear_multiply(0.12)
                        } else {
                            FALSE_COL.linear_multiply(0.12)
                        };
                        egui::Frame::default()
                            .fill(fill)
                            .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                            .rounding(egui::Rounding::same(4.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new(txt).color(col).strong());
                            });
                    }
                });
        }
        if !e.affects_flags.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Flags positionnés").strong());
            ui.label(RichText::new(e.affects_flags.join("  ")).monospace().color(CHANGED));
        }
    }

    // ---------- Pile / Tas ----------

    fn stack_ui(&mut self, ui: &mut egui::Ui) {
        // Handles clonés (Arc) => la closure peut muter self.stack_tab.
        let (stack_ic, heap_ic) = match &self.icons {
            Some(i) => (Some(i.stack.clone()), Some(i.heap.clone())),
            None => (None, None),
        };
        panel_header(ui, |ui| {
            if icon_tab(ui, stack_ic.as_ref(), "Pile", self.stack_tab == StackTab::Stack).clicked() {
                self.stack_tab = StackTab::Stack;
            }
            if icon_tab(ui, heap_ic.as_ref(), "Tas", self.stack_tab == StackTab::Heap).clicked() {
                self.stack_tab = StackTab::Heap;
            }
        });
        match self.stack_tab {
            StackTab::Stack => self.stack_view(ui),
            StackTab::Heap => self.heap_view(ui),
        }
    }

    fn stack_view(&self, ui: &mut egui::Ui) {
        let Some(snap) = self.snap() else {
            ui.label("—");
            return;
        };
        let flash = self.flash_progress(ui);
        let (rsp, rbp) = (snap.regs.rsp, snap.regs.rbp);

        // Badge PUSH / POP d'après la variation de RSP par rapport à l'étape précédente.
        if let Some(prev) = self.prev_snap() {
            let prsp = prev.regs.rsp;
            if rsp < prsp {
                ui.label(RichText::new("⬇ PUSH").strong().color(changed_color2(flash, PUSH_COL)));
            } else if rsp > prsp {
                ui.label(RichText::new("⬆ POP").strong().color(changed_color2(flash, POP_COL)));
            } else {
                ui.label("");
            }
        }

        let prev_stack = self.prev_snap().map(|p| p.stack.clone()).unwrap_or_default();
        egui::ScrollArea::vertical()
            .id_salt("stack_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
            egui::Grid::new("stack_grid").num_columns(3).spacing([10.0, 3.0]).show(ui, |ui| {
                for (i, val) in snap.stack.iter().enumerate() {
                    let addr = rsp.wrapping_add((i as u64) * 8);
                    let changed = prev_stack.get(i) != Some(val);
                    ui.label(RichText::new(format!("0x{addr:012X}")).monospace().color(self.c_addr()));
                    let mut vt = RichText::new(format!("0x{val:016X}")).monospace();
                    if changed {
                        vt = vt.color(changed_color(flash));
                    }
                    ui.label(vt);
                    let marker = if addr == rsp && addr == rbp {
                        "← RSP,RBP"
                    } else if addr == rsp {
                        "← RSP"
                    } else if addr == rbp {
                        "← RBP"
                    } else {
                        ""
                    };
                    ui.label(RichText::new(marker).monospace().color(CHANGED));
                    ui.end_row();
                }
            });
        });
    }

    /// Vue du tas (segment `[heap]` de /proc/<pid>/maps), en hexadécimal.
    fn heap_view(&self, ui: &mut egui::Ui) {
        if !self.can_read_memory() {
            let msg = match self.dbg.as_ref().map(|d| d.is_alive()) {
                Some(false) => "Programme terminé — relancez pour explorer le tas.",
                Some(true) => "Revenez à la dernière étape de la timeline pour lire le tas.",
                None => "Lancez un programme pour explorer le tas.",
            };
            ui.weak(msg);
            return;
        }
        let (hdr, addr_c, bytes_c) = (self.c_header(), self.c_addr(), self.c_bytes());
        let dbg = self.dbg.as_ref().unwrap();
        let Some((start, end)) = dbg.heap_range() else {
            ui.weak(
                "Aucun tas pour ce programme : le segment [heap] n'apparaît qu'après un appel \
                 brk/mmap (allocation dynamique). Un programme n'utilisant que .data/.bss ou la \
                 pile n'a pas de tas.",
            );
            return;
        };
        let size = end - start;
        ui.label(
            RichText::new(format!("[heap] 0x{start:X} – 0x{end:X}  ({size} octets)"))
                .monospace()
                .color(hdr),
        );
        ui.add_space(2.0);
        let rows = size.div_ceil(16).min(16);
        egui::ScrollArea::both().id_salt("heap_scroll").auto_shrink([false, false]).show(ui, |ui| {
            hex_dump_rows(ui, addr_c, bytes_c, dbg, start, rows);
        });
    }
}

/// Affiche `rows` lignes de 16 octets (hex + ASCII) à partir de `base`.
fn hex_dump_rows(ui: &mut egui::Ui, addr_c: Color32, bytes_c: Color32, dbg: &Debugger, base: u64, rows: u64) {
    for row in 0..rows {
        let addr = base.wrapping_add(row * 16);
        let (hex, ascii) = match dbg.read_mem(addr, 16) {
            Ok(bytes) => {
                let hex = bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ");
                let ascii: String = bytes
                    .iter()
                    .map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' })
                    .collect();
                (hex, ascii)
            }
            Err(_) => ("?? ".repeat(16).trim_end().to_string(), ".".repeat(16)),
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("0x{addr:08X}")).monospace().color(addr_c));
            ui.label(RichText::new(hex).monospace().color(bytes_c));
            ui.label(RichText::new(ascii).monospace().weak());
        });
    }
}

// ---------- Helpers ----------

/// Hauteur fixe de la ligne d'en-tête d'un panneau, pour aligner les
/// séparateurs de tous les panneaux au même niveau (certains en-têtes ont des
/// boutons/combos plus hauts qu'un simple libellé).
const HEADER_H: f32 = 24.0;

/// En-tête de panneau à hauteur fixe : rend `content` (titre + éventuels
/// contrôles) dans une ligne de `HEADER_H`, puis un séparateur. Tous les
/// panneaux passent par ici → leurs séparateurs sont alignés.
fn panel_header(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(3.0);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), HEADER_H),
        egui::Layout::left_to_right(egui::Align::Center),
        content,
    );
    ui.separator();
}

/// Encadré « carte » moderne : fond légèrement teinté, coins arrondis et marge
/// interne, sur toute la largeur disponible. Structure et aère le contenu
/// (utile pour une app pédagogique).
fn card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .rounding(egui::Rounding::same(6.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            content(ui);
        });
}

/// Icône optionnelle + titre de section, à placer dans un `panel_header`.
fn header_title(ui: &mut egui::Ui, hdr: Color32, icon: Option<&egui::TextureHandle>, text: &str) {
    icon_img(ui, icon, 15.0);
    ui.label(RichText::new(text).strong().color(hdr).size(12.5));
}

/// Titre de section simple (sans contrôle) à hauteur fixe.
fn header(ui: &mut egui::Ui, hdr: Color32, text: &str) {
    panel_header(ui, |ui| header_title(ui, hdr, None, text));
}

/// En-tête de section avec une icône optionnelle à gauche du titre.
fn header_icon(ui: &mut egui::Ui, hdr: Color32, icon: Option<&egui::TextureHandle>, text: &str) {
    panel_header(ui, |ui| header_title(ui, hdr, icon, text));
}

/// Affiche une petite icône carrée (rien si `icon` est `None`).
fn icon_img(ui: &mut egui::Ui, icon: Option<&egui::TextureHandle>, size: f32) {
    if let Some(t) = icon {
        ui.add(egui::Image::new((t.id(), egui::vec2(size, size))));
    }
}

/// Alloue une colonne de largeur `w` et hauteur `h` puis y rend `add`.
fn col(ui: &mut egui::Ui, w: f32, h: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(w, h),
        egui::Layout::top_down(egui::Align::Min),
        add,
    );
}

/// Petite colonne de pile (microscope) : adresse + valeur, à partir de `rsp`.
fn micro_stack(ui: &mut egui::Ui, addr_c: Color32, label: &str, rsp: u64, stack: &[u64]) {
    ui.label(RichText::new(label).italics().weak());
    egui::Grid::new(format!("micro_stack_{label}"))
        .num_columns(2)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            for (i, val) in stack.iter().take(6).enumerate() {
                let addr = rsp.wrapping_add((i as u64) * 8);
                let mark = if i == 0 { "→" } else { " " };
                ui.label(
                    RichText::new(format!("{mark} 0x{addr:012X}"))
                        .monospace()
                        .color(addr_c),
                );
                ui.label(RichText::new(format!("0x{val:016X}")).monospace());
                ui.end_row();
            }
        });
}

/// Flags positionnés (info statique) quand l'instruction n'a pas d'avant/après.
fn micro_static_flags(ui: &mut egui::Ui, hdr: Color32, e: &explain::Explanation) {
    ui.add_space(4.0);
    if e.affects_flags.is_empty() {
        ui.weak("Cette instruction ne modifie aucun flag.");
    } else {
        ui.label(RichText::new("Flags positionnés").strong().color(hdr));
        ui.label(RichText::new(e.affects_flags.join("  ")).monospace().color(CHANGED));
    }
}

fn parse_hex(s: &str) -> Option<u64> {
    let s = s.trim();
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u64::from_str_radix(s, 16).ok()
}

/// Analyse une suite d'octets hexadécimaux (« 48 65 6C » ou « 48656C »).
fn parse_hex_bytes(s: &str) -> Option<Vec<u8>> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty() || !cleaned.len().is_multiple_of(2) {
        return None;
    }
    (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).ok())
        .collect()
}

/// Nom d'affichage d'un chemin (dernier segment).
fn file_name(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

/// Entrées d'un dossier : (sous-dossiers, tous les fichiers), triés, en masquant
/// les entrées cachées (préfixe `.`). Pour l'explorateur en arbre.
fn list_entries(dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if file_name(&p).starts_with('.') {
                continue;
            }
            if p.is_dir() {
                dirs.push(p);
            } else {
                files.push(p);
            }
        }
    }
    dirs.sort();
    files.sort();
    (dirs, files)
}

/// True si le fichier est une source assembleur (`.asm`/`.s`).
fn is_asm(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "asm" || e == "s")
}

/// Rend récursivement l'arbre d'un dossier (style explorateur d'IDE) : dossiers
/// repliables (`CollapsingHeader`), puis fichiers cliquables. Le fichier ouvert
/// est surligné ; le clic sur un fichier renseigne `to_open`.
fn dir_tree(
    ui: &mut egui::Ui,
    dir: &Path,
    current: &Path,
    asm_col: Color32,
    other_col: Color32,
    to_open: &mut Option<PathBuf>,
) {
    let (dirs, files) = list_entries(dir);
    for d in dirs {
        egui::CollapsingHeader::new(RichText::new(format!("🗀  {}", file_name(&d))).color(asm_col))
            .id_salt(&d)
            .default_open(false)
            .show(ui, |ui| dir_tree(ui, &d, current, asm_col, other_col, to_open));
    }
    for f in files {
        let is_cur = f == current;
        let col = if is_cur {
            CHANGED
        } else if is_asm(&f) {
            asm_col
        } else {
            other_col
        };
        let label = RichText::new(format!("🗎  {}", file_name(&f))).color(col);
        if ui.add(egui::SelectableLabel::new(is_cur, label)).clicked() {
            *to_open = Some(f);
        }
    }
}

/// Bouton avec bordure verte (actif/disponible) ou rouge (inactif).
fn bordered_button(
    ui: &mut egui::Ui,
    icon: Option<&egui::TextureHandle>,
    label: &str,
    enabled: bool,
) -> egui::Response {
    let color = if enabled { FLAG_ON } else { FALSE_COL };
    let btn = match btn_icon(icon) {
        Some(img) => egui::Button::image_and_text(img, label),
        None => egui::Button::new(label),
    }
    .stroke(egui::Stroke::new(1.5_f32, color));
    ui.add_enabled(enabled, btn)
}

/// Construit un widget bouton (icône + libellé) sans l'ajouter — pour
/// `ui.add_enabled(...)`. La source d'image est `'static` (TextureId).
fn icon_btn_widget(icon: Option<&egui::TextureHandle>, label: &'static str) -> egui::Button<'static> {
    match btn_icon(icon) {
        Some(img) => egui::Button::image_and_text(img, label),
        None => egui::Button::new(label),
    }
}

/// Source d'image dimensionnée pour un bouton (16px), à partir d'une icône.
fn btn_icon(icon: Option<&egui::TextureHandle>) -> Option<egui::load::SizedTexture> {
    icon.map(|t| egui::load::SizedTexture::new(t.id(), egui::vec2(16.0, 16.0)))
}

/// Bouton d'accent (fond ACCENT si actif, grisé sinon) — pour Run et Step.
fn accent_button(
    ui: &mut egui::Ui,
    icon: Option<&egui::TextureHandle>,
    label: &str,
    enabled: bool,
) -> egui::Response {
    let btn = match (enabled, btn_icon(icon)) {
        (true, Some(img)) => {
            egui::Button::image_and_text(img, RichText::new(label).color(Color32::WHITE)).fill(ACTION)
        }
        (true, None) => egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(ACTION),
        (false, Some(img)) => egui::Button::image_and_text(img, label),
        (false, None) => egui::Button::new(label),
    };
    ui.add_enabled(enabled, btn)
}

/// Bouton ordinaire avec icône optionnelle à gauche du libellé.
fn icon_button(ui: &mut egui::Ui, icon: Option<&egui::TextureHandle>, label: &str) -> egui::Response {
    match btn_icon(icon) {
        Some(img) => ui.add(egui::Button::image_and_text(img, label)),
        None => ui.button(label),
    }
}

/// Onglet sélectionnable avec l'icône DANS le bouton (respecte le padding).
/// Remplace `icon_img(...) + selectable_label(...)` où l'icône débordait.
fn icon_tab(
    ui: &mut egui::Ui,
    icon: Option<&egui::TextureHandle>,
    label: &str,
    selected: bool,
) -> egui::Response {
    let btn = match btn_icon(icon) {
        Some(img) => egui::Button::image_and_text(img, label),
        None => egui::Button::new(label),
    }
    .selected(selected)
    .rounding(egui::Rounding::same(6.0));
    ui.add(btn)
}

/// Petit badge coloré (texte sur fond semi-transparent).
fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::default()
        .fill(color.linear_multiply(0.22))
        .inner_margin(egui::Margin::symmetric(5.0, 2.0))
        .rounding(egui::Rounding::same(4.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small().strong().color(color));
        });
}

/// Chemin du fichier de réglages persistants (XDG : ~/.config/asm_studio/settings.conf).
fn settings_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("asm_studio").join("settings.conf"))
}

/// Répertoire contenant l'`asmstd.inc` fourni, s'il est trouvable.
fn asmstd_dir() -> Option<PathBuf> {
    let dir = PathBuf::from("examples");
    dir.join("asmstd.inc").exists().then_some(dir)
}

/// Répertoire absolu contenant `path` (remonte à `current_dir` si besoin).
fn abs_dir_of(path: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    abs.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abs_dir_is_absolute_and_navigable() {
        // À partir d'un chemin relatif, on obtient un dossier absolu dont on
        // peut remonter le parent (ce qui faisait échouer le navigateur avant).
        let dir = abs_dir_of(Path::new("examples/test.asm"));
        assert!(dir.is_absolute(), "le dossier doit être absolu");
        assert!(dir.ends_with("examples"));
        assert!(dir.parent().is_some(), "on doit pouvoir remonter (..)");
    }

    #[test]
    fn list_entries_finds_asm_example() {
        let (_dirs, files) = list_entries(&abs_dir_of(Path::new("examples/test.asm")));
        assert!(
            files.iter().any(|f| f.file_name().unwrap() == "test.asm"),
            "test.asm doit apparaître dans l'explorateur"
        );
    }

    #[test]
    fn parse_hex_bytes_accepts_spaced_and_contiguous() {
        assert_eq!(parse_hex_bytes("48 65 6C"), Some(vec![0x48, 0x65, 0x6C]));
        assert_eq!(parse_hex_bytes("48656C"), Some(vec![0x48, 0x65, 0x6C]));
        assert_eq!(parse_hex_bytes("4"), None, "longueur impaire invalide");
        assert_eq!(parse_hex_bytes("zz"), None, "non-hexa invalide");
    }

    /// Vérifie que la logique timeline (head-follow + clamp min/max) est correcte,
    /// indépendamment du rendu egui.
    #[test]
    fn timeline_view_index_clamps_and_follows_head() {
        let mut app = App::new();
        // Fichier/dossier de sortie dédiés pour ne pas courir avec les autres tests.
        app.src_path = PathBuf::from("build/tl-test.asm");
        app.out_dir = PathBuf::from("build/tl");
        app.source = "section .text\n global _start\n_start:\n mov rax,5\n mov rbx,8\n \
                       cmp rax,rbx\n mov rax,60\n xor rdi,rdi\n syscall\n"
            .to_string();

        app.launch();
        assert!(app.dbg.is_some(), "le programme doit être lancé");
        assert_eq!(app.view_index, 0, "au lancement, on est à l'étape 0");

        // Quelques pas : la vue doit suivre la tête.
        for _ in 0..4 {
            app.step();
        }
        let last = app.dbg.as_ref().unwrap().history.len() - 1;
        assert_eq!(app.view_index, last, "la vue suit la tête après Step");

        // Bornes : min = 0, max = last.
        app.set_view(0);
        assert_eq!(app.view_index, 0);
        app.set_view(-10);
        assert_eq!(app.view_index, 0, "borne min respectée");
        app.set_view(100_000);
        assert_eq!(app.view_index, last, "borne max respectée");

        // Scrubbing : l'état affiché change bien selon l'index.
        app.set_view(1);
        let rip1 = app.snap().unwrap().regs.rip;
        app.set_view(2);
        let rip2 = app.snap().unwrap().regs.rip;
        assert_ne!(rip1, rip2, "changer d'étape doit changer l'état affiché (RIP)");
    }

    /// `rebuild_trace` journalise les appels système depuis l'historique, et
    /// `resume_here` resynchronise la trace (pas de données figées du run précédent).
    #[test]
    fn trace_rebuilds_syscalls_and_resume_is_consistent() {
        let mut app = App::new();
        app.src_path = PathBuf::from("build/trace-test.asm");
        app.out_dir = PathBuf::from("build/trace");
        app.source = "section .text\n global _start\n_start:\n mov rax,60\n xor rdi,rdi\n syscall\n"
            .to_string();

        app.launch();
        assert!(app.dbg.is_some(), "le programme doit être lancé");
        assert!(app.syscalls.is_empty(), "aucun syscall avant d'exécuter");

        // Exécute tout le programme : le syscall exit doit être journalisé une fois.
        for _ in 0..8 {
            app.step();
        }
        assert_eq!(app.syscalls.len(), 1, "un seul appel système (exit)");
        assert_eq!(app.syscalls[0].number, 60, "exit = 60");
        assert!(app.syscalls[0].ret.is_none(), "exit ne revient pas");

        // Revenir avant le syscall puis « Reprendre ici » : la trace doit refléter
        // la nouvelle position (le syscall n'a pas encore été exécuté).
        app.set_view(1);
        app.resume_here();
        assert!(
            app.syscalls.is_empty(),
            "après resume avant le syscall, la trace ne doit pas rester figée"
        );
    }
}
