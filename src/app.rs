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

    tab: Tab,
    stack_tab: StackTab,
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
    // Navigateur de fichiers intégré (Ouvrir / Enregistrer sous).
    show_open: bool,
    show_saveas: bool,
    saveas_name: String,
    /// Répertoire courant du navigateur (toujours absolu).
    browse_dir: PathBuf,
}

impl App {
    pub fn new() -> Self {
        let src_path = PathBuf::from("examples/test.asm");
        let source = std::fs::read_to_string(&src_path).unwrap_or_else(|_| {
            "section .text\n    global _start\n_start:\n    mov rax, 60\n    xor rdi, rdi\n    syscall\n"
                .to_string()
        });
        let browse_dir = abs_dir_of(&src_path);
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
            tab: Tab::Editor,
            stack_tab: StackTab::Stack,
            show_tooltips: true,
            animate: true,
            use_asmstd: false,
            flash_time: 0.0,
            pending_flash: false,
            theme_pref: egui::ThemePreference::Dark,
            show_settings: false,
            show_about: false,
            show_shortcuts: false,
            show_open: false,
            show_saveas: false,
            saveas_name: String::new(),
            browse_dir,
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
            "theme={theme}\ntooltips={}\nasmstd={}\nanimate={}\n",
            self.show_tooltips, self.use_asmstd, self.animate
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
        if let Some(parent) = self.src_path.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    self.log(&format!("Impossible de créer {}: {e}", parent.display()));
                    return false;
                }
            }
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

    /// Ouvre la boîte « Enregistrer sous » pré-remplie avec le fichier courant.
    fn open_saveas(&mut self) {
        self.browse_dir = abs_dir_of(&self.src_path);
        self.saveas_name = self
            .src_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "programme.asm".to_string());
        self.show_saveas = true;
    }

    /// Ouvre le navigateur « Ouvrir » sur le dossier du fichier courant.
    fn open_browser(&mut self) {
        self.browse_dir = abs_dir_of(&self.src_path);
        self.show_open = true;
    }

    fn open_file(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                self.source = content;
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
        self.src_path = PathBuf::from("sans-titre.asm");
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
        if let Some(p) = self.src_path.parent() {
            if !p.as_os_str().is_empty() {
                dirs.push(p.to_path_buf());
            }
        }
        if self.use_asmstd {
            if let Some(d) = asmstd_dir() {
                if !dirs.contains(&d) {
                    dirs.push(d);
                }
            }
        }
        dirs
    }

    /// Enregistre puis assemble (nasm) et lie (ld) le programme de l'utilisateur.
    fn build(&mut self) {
        self.save_source();
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
        let pending = self.dbg.as_ref().and_then(|d| {
            let rip = d.regs().rip;
            let is_syscall = self
                .disasm
                .iter()
                .any(|i| i.address == rip && i.mnemonic == "syscall");
            is_syscall.then(|| (syscall::format_call(d.regs()), d.regs().rax))
        });

        if let Some(d) = self.dbg.as_mut() {
            if let Err(e) = d.step() {
                self.log(&e);
                return;
            }
        }
        if let Some(d) = self.dbg.as_ref() {
            self.view_index = d.history.len() - 1;
        }
        self.pending_flash = true; // déclenche l'animation « CPU vivant »
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
            }
            Err(e) => self.log(&e),
        }
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
        if elapsed < 0.0 || elapsed >= FLASH_DUR {
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
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_theme(ctx);
        if self.pending_flash {
            self.flash_time = ctx.input(|i| i.time);
            self.pending_flash = false;
        }
        self.handle_shortcuts(ctx);

        self.menu_bar(ctx);
        self.toolbar(ctx);
        self.status_bar(ctx);
        self.timeline_panel(ctx);
        self.bottom_band(ctx);

        egui::SidePanel::left("regs_panel")
            .resizable(false)
            .default_width(250.0)
            .show(ctx, |ui| self.registers_ui(ui));
        egui::SidePanel::right("instruction_panel")
            .resizable(true)
            .default_width(320.0)
            .show(ctx, |ui| self.instruction_ui(ui));
        egui::SidePanel::right("stack_panel")
            .resizable(false)
            .default_width(300.0)
            .show(ctx, |ui| self.stack_ui(ui));
        egui::CentralPanel::default().show(ctx, |ui| self.center_ui(ui));

        self.about_window(ctx);
        self.shortcuts_window(ctx);
        self.settings_window(ctx);
        self.open_window(ctx);
        self.saveas_window(ctx);
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

    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let mut open = true;
        egui::Window::new("À propos")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading(RichText::new("ASM Studio").color(MNEMONIC));
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
    fn apply_theme(&self, ctx: &egui::Context) {
        use egui::{FontId, Rounding, Theme, ThemePreference, TextStyle, vec2};
        let dark = match self.theme_pref {
            ThemePreference::Dark => true,
            ThemePreference::Light => false,
            ThemePreference::System => {
                ctx.input(|i| i.raw.system_theme) != Some(Theme::Light)
            }
        };
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
        }
        style.visuals = v;
        style.spacing.item_spacing = vec2(8.0, 6.0);
        style.spacing.button_padding = vec2(9.0, 4.0);
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
            .resizable(false)
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
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                let rows = [
                    ("F5", "Lancer / Restart"),
                    ("F10 / F8", "Step (une instruction)"),
                    ("Échap / Maj+F5", "Stop"),
                    ("Ctrl+B", "Assembler + Lier"),
                    ("Ctrl+S", "Enregistrer"),
                    ("Ctrl+O", "Ouvrir"),
                    ("Ctrl+N", "Nouveau"),
                    ("← / →", "Timeline : précédent / suivant"),
                    ("Home / End", "Timeline : début / fin"),
                ];
                egui::Grid::new("shortcuts_grid")
                    .num_columns(2)
                    .spacing([24.0, 6.0])
                    .show(ui, |ui| {
                        for (k, d) in rows {
                            ui.label(RichText::new(k).monospace().strong().color(MNEMONIC));
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

    /// Navigateur de fichiers modernisé (barre de chemin + liste en lignes
    /// pleine largeur). Renvoie (dossier à ouvrir, fichier choisi).
    fn file_browser(&self, ui: &mut egui::Ui, scroll_id: &str) -> (Option<PathBuf>, Option<PathBuf>) {
        let mut new_dir = None;
        let mut picked = None;

        // Barre de chemin.
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::symmetric(8.0, 4.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("📂");
                    ui.monospace(self.browse_dir.display().to_string());
                });
            });
        ui.add_space(6.0);

        // Liste (dossiers puis fichiers .asm), lignes pleine largeur.
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(scroll_id)
                .max_height(300.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let w = ui.available_width();
                    let row = |ui: &mut egui::Ui, text: egui::WidgetText| {
                        ui.add_sized([w, 24.0], egui::SelectableLabel::new(false, text))
                    };
                    if let Some(parent) = self.browse_dir.parent() {
                        if row(ui, "📁  ..".into()).clicked() {
                            new_dir = Some(parent.to_path_buf());
                        }
                    }
                    let (dirs, files) = list_dir(&self.browse_dir);
                    for d in dirs {
                        let name = d.file_name().unwrap_or_default().to_string_lossy().to_string();
                        if row(ui, format!("📁  {name}").into()).clicked() {
                            new_dir = Some(d);
                        }
                    }
                    for f in files {
                        let name = f.file_name().unwrap_or_default().to_string_lossy().to_string();
                        let text = RichText::new(format!("📄  {name}")).color(MNEMONIC);
                        if row(ui, text.into()).clicked() {
                            picked = Some(f);
                        }
                    }
                });
        });
        (new_dir, picked)
    }

    fn saveas_window(&mut self, ctx: &egui::Context) {
        if !self.show_saveas {
            return;
        }
        let mut open = true;
        let mut confirm = false;
        let mut cancel = false;
        let mut new_dir = None;
        let mut picked = None;
        egui::Window::new("Enregistrer sous")
            .collapsible(false)
            .resizable(true)
            .default_width(500.0)
            .default_height(440.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                let (nd, pk) = self.file_browser(ui, "saveas_scroll");
                new_dir = nd;
                picked = pk;
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Nom :");
                    ui.add(egui::TextEdit::singleline(&mut self.saveas_name).desired_width(260.0));
                });
                ui.add_space(6.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("💾  Enregistrer").clicked() {
                        confirm = true;
                    }
                    if ui.button("Annuler").clicked() {
                        cancel = true;
                    }
                });
            });
        if let Some(d) = new_dir {
            self.browse_dir = d;
        }
        // Clic sur un fichier existant => reprend son nom (écrasement).
        if let Some(f) = picked {
            self.saveas_name = f.file_name().unwrap_or_default().to_string_lossy().into_owned();
        }
        if confirm && !self.saveas_name.trim().is_empty() {
            self.src_path = self.browse_dir.join(self.saveas_name.trim());
            self.save_source();
            self.show_saveas = false;
        }
        if cancel || !open {
            self.show_saveas = false;
        }
    }

    fn open_window(&mut self, ctx: &egui::Context) {
        if !self.show_open {
            return;
        }
        let mut open = true;
        let mut cancel = false;
        let mut new_dir = None;
        let mut chosen = None;
        egui::Window::new("Ouvrir un fichier .asm")
            .collapsible(false)
            .resizable(true)
            .default_width(500.0)
            .default_height(440.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                let (nd, pk) = self.file_browser(ui, "open_scroll");
                new_dir = nd;
                chosen = pk;
                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Annuler").clicked() {
                        cancel = true;
                    }
                });
            });
        if let Some(d) = new_dir {
            self.browse_dir = d;
        }
        if let Some(f) = chosen {
            self.open_file(f);
            self.show_open = false;
        }
        if !open || cancel {
            self.show_open = false;
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
                // Running = un programme est en cours (tracé et vivant).
                let running = self.dbg.as_ref().is_some_and(|d| d.is_alive());

                // Lancer : vert + actif quand rien ne tourne ; rouge + inactif sinon.
                if self.tip(bordered_button(ui, "▶  Lancer", !running), "F5").clicked() {
                    self.launch();
                }
                // Précédent : recule d'une étape dans la timeline enregistrée.
                let can_prev = self.dbg.is_some() && self.view_index > 0;
                if self
                    .tip(
                        ui.add_enabled(can_prev, egui::Button::new("◀  Précédent")),
                        "Étape précédente (←)",
                    )
                    .clicked()
                {
                    self.set_view(self.view_index as i64 - 1);
                }
                let can_step = self.can_step();
                if self
                    .tip(ui.add_enabled(can_step, egui::Button::new("⏭  Step")), "F10 / F8")
                    .clicked()
                {
                    self.step();
                }
                if self.tip(ui.button("🔄  Restart"), "F5").clicked() {
                    self.launch();
                }
                // Stop : vert + actif quand un programme tourne ; rouge + inactif sinon.
                if self.tip(bordered_button(ui, "⏹  Stop", running), "Échap").clicked() {
                    self.stop();
                }
                ui.separator();
                if self.tip(ui.button("🔨  Build"), "Assembler + Lier (Ctrl+B)").clicked() {
                    self.build();
                }
                if self.tip(ui.button("💾  Enregistrer"), "Ctrl+S").clicked() {
                    self.save_source();
                }
                ui.separator();
                ui.label(RichText::new(&self.status).color(HEADER));
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
                ui.label("x86_64");
                ui.separator();
                ui.label("64-bit");
                ui.separator();
                if let Some(s) = self.snap() {
                    ui.label(format!("RIP 0x{:X}", s.regs.rip));
                    if let Some(next) = self.next_addr() {
                        ui.separator();
                        ui.colored_label(CHANGED, format!("Next 0x{next:X}"));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(self.src_path.display().to_string()).color(HEADER));
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

    fn bottom_band(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("bottom_band")
            .resizable(true)
            .default_height(210.0)
            .show(ctx, |ui| {
                // Mémoire | (séparateur vertical) | Console.
                ui.horizontal_top(|ui| {
                    let h = ui.available_height();
                    let half = (ui.available_width() - 12.0) * 0.5;
                    ui.allocate_ui_with_layout(
                        egui::vec2(half, h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.memory_ui(ui),
                    );
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.console_ui(ui),
                    );
                });
            });
    }

    /// Barre timeline pleine largeur (au-dessus de la barre d'état).
    fn timeline_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("timeline_panel").show(ctx, |ui| {
            ui.add_space(2.0);
            let Some(last) = self.dbg.as_ref().map(|d| d.history.len() - 1) else {
                ui.horizontal(|ui| {
                    header_inline(ui, "TIMELINE");
                    ui.separator();
                    ui.weak("— lancez un programme pour enregistrer la timeline");
                });
                ui.add_space(2.0);
                return;
            };

            // Ligne 1 : contrôles + libellés (largeur variable, sans le slider).
            ui.horizontal(|ui| {
                header_inline(ui, "TIMELINE");
                ui.separator();
                if self.tip(ui.button("⏮"), "Début (Home)").clicked() {
                    self.set_view(0);
                }
                if self.tip(ui.button("◀"), "Précédent (←)").clicked() {
                    self.set_view(self.view_index as i64 - 1);
                }
                if self.tip(ui.button("▶"), "Suivant (→)").clicked() {
                    self.set_view(self.view_index as i64 + 1);
                }
                if self.tip(ui.button("⏭"), "Fin (End)").clicked() {
                    self.set_view(i64::MAX);
                }
                ui.label(RichText::new(format!("{} / {last}", self.view_index)).monospace().strong());
                if !self.is_head_view()
                    && self
                        .tip(ui.button("⟳ Reprendre ici"), "Ré-exécute jusqu'à cette étape pour continuer")
                        .clicked()
                {
                    self.resume_here();
                }
                if let Some(s) = self.snap() {
                    if let Some(insn) = self.disasm.iter().find(|i| i.address == s.regs.rip) {
                        ui.separator();
                        ui.label(
                            RichText::new(format!("{} {}", insn.mnemonic, insn.operands))
                                .monospace()
                                .color(MNEMONIC),
                        );
                    }
                }
            });

            // Ligne 2 : slider seul, largeur STABLE (= largeur du panneau).
            // (Le mettre sur la même ligne que des libellés variables faisait
            //  « sauter » le curseur : rétroaction largeur ↔ contenu.)
            let mut idx = self.view_index;
            ui.spacing_mut().slider_width = (ui.available_width() - 16.0).max(80.0);
            if ui
                .add(egui::Slider::new(&mut idx, 0..=last).show_value(false))
                .changed()
            {
                self.view_index = idx;
            }
            ui.add_space(2.0);
        });
    }

    fn memory_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            header_inline(ui, "MEMORY");
            ui.label("aller @");
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.mem_input)
                    .desired_width(170.0)
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
            // Raccourcis vers les régions utiles.
            if let Some(d) = self.dbg.as_ref().filter(|d| d.is_alive()) {
                if ui.small_button("pile").on_hover_text("Aller à RSP").clicked() {
                    self.mem_addr = d.regs().rsp;
                    self.mem_input = format!("0x{:X}", self.mem_addr);
                }
            }
        });
        ui.separator();
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

        let dbg = self.dbg.as_ref().unwrap();
        egui::ScrollArea::both()
            .id_salt("mem_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| hex_dump_rows(ui, dbg, self.mem_addr, 8));
    }

    fn console_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            header_inline(ui, "CONSOLE");
            if ui.small_button("effacer").clicked() {
                self.console.clear();
            }
        });
        ui.separator();
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
        header(ui, "REGISTERS");
        let Some(rows) = self.reg_rows() else {
            ui.label("Aucun programme lancé.");
            return;
        };
        // Édition possible seulement en pause, à la dernière étape.
        let editable = self.can_step();
        if editable {
            ui.label(RichText::new("clic sur une valeur pour l'éditer").small().weak());
        }
        let flash = self.flash_progress(ui); // pulsation « CPU vivant »
        let mut commit: Option<(&'static str, u64)> = None;
        let mut stop_edit = false;

        egui::ScrollArea::both()
            .id_salt("regs_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("regs_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                    for (name, val, pval) in &rows {
                        let (name, val, pval) = (*name, *val, *pval);
                        ui.label(RichText::new(name).monospace().strong());
                        if self.edit_reg == Some(name) {
                            // Édition : champ hexa + ✓ (valider) / ✗ (annuler).
                            // On ne capture PAS self dans la closure (emprunts disjoints).
                            let focus_now = std::mem::take(&mut self.edit_focus);
                            let buf = &mut self.edit_buf;
                            let mut committed: Option<u64> = None;
                            let mut ended = false;
                            ui.horizontal(|ui| {
                                let resp = ui.add(
                                    egui::TextEdit::singleline(buf)
                                        .desired_width(120.0)
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
                                // Bouton sans cadre : clic fiable pour éditer.
                                let resp = ui
                                    .add(egui::Button::new(t).frame(false))
                                    .on_hover_text("Cliquer pour modifier");
                                if resp.clicked() {
                                    self.edit_reg = Some(name);
                                    self.edit_buf = format!("{val:X}");
                                    self.edit_focus = true;
                                }
                            } else {
                                ui.label(t);
                            }
                        }
                        ui.end_row();
                    }
                });

                ui.add_space(10.0);
                header(ui, "FLAGS");
                let (ef, pef) = rows
                    .iter()
                    .find(|(n, _, _)| *n == "EFLAGS")
                    .map(|(_, v, p)| (*v, *p))
                    .unwrap_or((0, 0));
                let flags = Flags::from_eflags(ef);
                let prevf = Flags::from_eflags(pef);
                egui::Grid::new("flags_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                    for ((name, val), (_, pval)) in flags.named().iter().zip(prevf.named()) {
                        let changed = *val != pval;
                        let mut label = RichText::new(*name).monospace();
                        if changed {
                            label = label.color(changed_color(flash));
                        }
                        ui.label(label);
                        let color = if changed {
                            changed_color(flash)
                        } else if *val {
                            FLAG_ON
                        } else {
                            FLAG_OFF
                        };
                        ui.label(RichText::new(if *val { "1" } else { "0" }).monospace().color(color));
                        ui.end_row();
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

    // ---------- Centre : onglets Éditeur / Désassemblage ----------

    fn center_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.selectable_label(self.tab == Tab::Editor, "  Éditeur  ").clicked() {
                self.tab = Tab::Editor;
            }
            if ui.selectable_label(self.tab == Tab::Disasm, "  Désassemblage  ").clicked() {
                self.tab = Tab::Disasm;
            }
            ui.separator();
            let name = self.src_path.file_name().unwrap_or_default().to_string_lossy();
            let mark = if self.dirty { " ●" } else { "" };
            ui.label(RichText::new(format!("{name}{mark}")).color(HEADER));
        });
        ui.separator();
        match self.tab {
            Tab::Editor => self.editor_ui(ui),
            Tab::Disasm => self.disasm_ui(ui),
        }
    }

    fn editor_ui(&mut self, ui: &mut egui::Ui) {
        // Ligne source courante (RIP) à surligner pendant le débogage.
        let hl = self.current_source_line();

        // Coloration syntaxique NASM (retour à la ligne désactivé => aligné aux numéros).
        let mut layouter = |ui: &egui::Ui, text: &str, _wrap: f32| {
            ui.fonts(|f| f.layout_job(syntax::highlight(text, hl)))
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
                    let resp = ui.add(
                        egui::TextEdit::multiline(&mut self.source)
                            .frame(false)
                            .code_editor()
                            .desired_width(content_w.max(ui.available_width()))
                            .desired_rows(28)
                            .lock_focus(true)
                            .layouter(&mut layouter),
                    );
                    if resp.changed() {
                        self.dirty = true;
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
                    ui.label(RichText::new(format!("0x{:08X}", insn.address)).monospace().color(ADDR_COL));
                    ui.label(RichText::new(format!("{:<20}", insn.bytes_hex())).monospace().color(BYTES_COL));
                    ui.label(RichText::new(format!("{:<7}", insn.mnemonic)).monospace().color(MNEMONIC));
                    ui.label(RichText::new(&insn.operands).monospace());
                });
                let row = inner.response.interact(egui::Sense::click());
                if row.clicked() {
                    clicked = Some(insn.address);
                }
                let fill = if is_current {
                    Some(RIP_ROW)
                } else if is_selected {
                    Some(SEL_ROW)
                } else if row.hovered() {
                    Some(SEL_ROW.linear_multiply(0.5))
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

    fn instruction_ui(&self, ui: &mut egui::Ui) {
        header(ui, "INSTRUCTION");
        let target = self.selected.or_else(|| self.view_rip());
        let Some(addr) = target else {
            ui.label("Lancez le programme, puis cliquez une instruction.");
            return;
        };
        let Some(insn) = self.disasm.iter().find(|i| i.address == addr) else {
            ui.label("—");
            return;
        };
        let flags = self.snap().map(|s| Flags::from_eflags(s.regs.eflags)).unwrap_or_default();
        let e = explain::explain(&insn.mnemonic, &insn.operands, flags);

        ui.label(
            RichText::new(if self.selected.is_some() {
                "(sélection — reclic pour suivre RIP)"
            } else {
                "(instruction courante)"
            })
            .small()
            .weak(),
        );
        ui.add_space(4.0);
        ui.label(RichText::new(&e.title).heading().color(MNEMONIC));
        ui.label(RichText::new(e.category).italics().weak());
        ui.add_space(6.0);
        ui.label(RichText::new("Description").strong());
        ui.label(&e.description);

        if let Some(cond) = &e.condition {
            ui.add_space(6.0);
            ui.label(RichText::new("Condition").strong());
            ui.label(RichText::new(cond).monospace());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("État actuel :").strong());
                for (name, val) in &e.relevant_flags {
                    let c = if *val { FLAG_ON } else { FLAG_OFF };
                    ui.label(RichText::new(format!("{name} = {}", *val as u8)).monospace().color(c));
                }
            });
            if let Some(taken) = e.taken {
                ui.add_space(6.0);
                let (txt, col) = if taken {
                    ("✔ Condition vraie — le saut sera pris.", FLAG_ON)
                } else {
                    ("✘ Condition fausse — pas de saut (on continue).", FALSE_COL)
                };
                ui.label(RichText::new(txt).color(col).strong());
            }
        }
        if !e.affects_flags.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("Flags positionnés").strong());
            ui.label(RichText::new(e.affects_flags.join("  ")).monospace().color(CHANGED));
        }
    }

    // ---------- Pile / Tas ----------

    fn stack_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.selectable_label(self.stack_tab == StackTab::Stack, "  Pile  ").clicked() {
                self.stack_tab = StackTab::Stack;
            }
            if ui.selectable_label(self.stack_tab == StackTab::Heap, "  Tas  ").clicked() {
                self.stack_tab = StackTab::Heap;
            }
        });
        ui.separator();
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
        egui::ScrollArea::vertical().id_salt("stack_scroll").show(ui, |ui| {
            egui::Grid::new("stack_grid").num_columns(3).spacing([10.0, 3.0]).show(ui, |ui| {
                for (i, val) in snap.stack.iter().enumerate() {
                    let addr = rsp.wrapping_add((i as u64) * 8);
                    let changed = prev_stack.get(i) != Some(val);
                    ui.label(RichText::new(format!("0x{addr:012X}")).monospace().color(ADDR_COL));
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
            ui.weak("Tas lisible sur l'état courant (revenez à la dernière étape).");
            return;
        }
        let dbg = self.dbg.as_ref().unwrap();
        let Some((start, end)) = dbg.heap_range() else {
            ui.weak("Aucun tas alloué : le programme n'a pas encore appelé brk/mmap.");
            return;
        };
        let size = end - start;
        ui.label(
            RichText::new(format!("[heap] 0x{start:X} – 0x{end:X}  ({size} octets)"))
                .monospace()
                .color(HEADER),
        );
        ui.add_space(2.0);
        let rows = ((size + 15) / 16).min(16) as u64;
        egui::ScrollArea::both().id_salt("heap_scroll").auto_shrink([false, false]).show(ui, |ui| {
            hex_dump_rows(ui, dbg, start, rows);
        });
    }
}

/// Affiche `rows` lignes de 16 octets (hex + ASCII) à partir de `base`.
fn hex_dump_rows(ui: &mut egui::Ui, dbg: &Debugger, base: u64, rows: u64) {
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
            ui.label(RichText::new(format!("0x{addr:08X}")).monospace().color(ADDR_COL));
            ui.label(RichText::new(hex).monospace().color(BYTES_COL));
            ui.label(RichText::new(ascii).monospace().weak());
        });
    }
}

// ---------- Helpers ----------

/// Titre de section sur sa propre ligne, style moderne.
fn header(ui: &mut egui::Ui, text: &str) {
    ui.add_space(2.0);
    ui.label(RichText::new(text).strong().color(HEADER).size(12.5));
    ui.separator();
}

/// Titre de section « inline » (dans une ligne horizontale).
fn header_inline(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).strong().color(HEADER).size(12.5));
}

fn parse_hex(s: &str) -> Option<u64> {
    let s = s.trim();
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u64::from_str_radix(s, 16).ok()
}

/// Analyse une suite d'octets hexadécimaux (« 48 65 6C » ou « 48656C »).
fn parse_hex_bytes(s: &str) -> Option<Vec<u8>> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty() || cleaned.len() % 2 != 0 {
        return None;
    }
    (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).ok())
        .collect()
}

/// Liste (dossiers, fichiers .asm) d'un répertoire, triés par nom.
fn list_dir(dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|e| e == "asm" || e == "s") {
                files.push(p);
            }
        }
    }
    dirs.sort();
    files.sort();
    (dirs, files)
}

/// Bouton avec bordure verte (actif/disponible) ou rouge (inactif).
fn bordered_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    let color = if enabled { FLAG_ON } else { FALSE_COL };
    let btn = egui::Button::new(label).stroke(egui::Stroke::new(1.5_f32, color));
    ui.add_enabled(enabled, btn)
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
    fn list_dir_finds_asm_example() {
        let (_dirs, files) = list_dir(&abs_dir_of(Path::new("examples/test.asm")));
        assert!(
            files.iter().any(|f| f.file_name().unwrap() == "test.asm"),
            "test.asm doit apparaître dans le navigateur"
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
}
