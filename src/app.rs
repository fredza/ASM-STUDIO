//! Application eframe : éditeur NASM + débogueur pédagogique.
//!
//! Layout : menu / barre d'outils en haut, barre d'état + bande
//! (Mémoire | Timeline | Console) en bas, Registres/Flags à gauche,
//! Instruction + Pile à droite, éditeur/désassemblage au centre (onglets).
//! L'état affiché est lu dans l'historique du debugger à `view_index`.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, RichText, text::LayoutJob};

use crate::assemble;
use crate::debugger::{Debugger, Flags, RunState, Snapshot};
use crate::disasm::{self, Insn};
use crate::{explain, syscall};

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

// --- Coloration syntaxique NASM (style VSCode) ---
const SYN_COMMENT: Color32 = Color32::from_rgb(0x6A, 0x99, 0x55);
const SYN_MNEMONIC: Color32 = Color32::from_rgb(0x56, 0x9C, 0xD6);
const SYN_REGISTER: Color32 = Color32::from_rgb(0x9C, 0xDC, 0xFE);
const SYN_NUMBER: Color32 = Color32::from_rgb(0xB5, 0xCE, 0xA8);
const SYN_DIRECTIVE: Color32 = Color32::from_rgb(0xC5, 0x86, 0xC0);
const SYN_LABEL: Color32 = Color32::from_rgb(0xDC, 0xDC, 0xAA);
const SYN_TEXT: Color32 = Color32::from_rgb(0xD4, 0xD4, 0xD4);

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Editor,
    Disasm,
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
    selected: Option<u64>,
    view_index: usize,

    mem_addr: u64,
    mem_input: String,
    console: String,
    status: String,

    tab: Tab,
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
        App {
            src_path,
            out_dir: PathBuf::from("build"),
            source,
            dirty: false,
            binary: None,
            dbg: None,
            disasm: Vec::new(),
            selected: None,
            view_index: 0,
            mem_addr: 0,
            mem_input: String::new(),
            console: String::new(),
            status: "Prêt".to_string(),
            tab: Tab::Editor,
            theme_pref: egui::ThemePreference::Dark,
            show_settings: false,
            show_about: false,
            show_shortcuts: false,
            show_open: false,
            show_saveas: false,
            saveas_name: String::new(),
            browse_dir,
        }
    }

    // ---------- Fichiers ----------

    fn log(&mut self, s: &str) {
        self.console.push_str(s);
        if !s.ends_with('\n') {
            self.console.push('\n');
        }
    }

    fn save_source(&mut self) -> bool {
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

    /// Enregistre puis assemble (nasm) et lie (ld) le programme de l'utilisateur.
    fn build(&mut self) {
        self.save_source();
        match assemble::assemble(&self.src_path, &self.out_dir) {
            Ok(out) => {
                self.log(&out.log);
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
                self.tab = Tab::Disasm;
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
    fn set_view(&mut self, idx: i64) {
        if let Some(d) = &self.dbg {
            self.view_index = idx.clamp(0, (d.history.len() - 1) as i64) as usize;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_theme(ctx);
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
        egui::Window::new("Réglages")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(RichText::new("Thème").strong());
                ui.add_space(4.0);
                ui.radio_value(&mut self.theme_pref, ThemePreference::System, "Système (suit l'OS)");
                ui.radio_value(&mut self.theme_pref, ThemePreference::Dark, "Sombre");
                ui.radio_value(&mut self.theme_pref, ThemePreference::Light, "Clair");
                ui.add_space(4.0);
                ui.weak("Note : la coloration du code est optimisée pour le thème sombre.");
                ui.separator();
                ui.vertical_centered(|ui| {
                    if ui.button("Fermer").clicked() {
                        self.show_settings = false;
                    }
                });
            });
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

    fn saveas_window(&mut self, ctx: &egui::Context) {
        if !self.show_saveas {
            return;
        }
        let mut open = true;
        let mut confirm = false;
        let mut cancel = false;
        let mut new_dir: Option<PathBuf> = None;
        egui::Window::new("Enregistrer sous")
            .collapsible(false)
            .resizable(true)
            .default_width(460.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("📂");
                    ui.monospace(self.browse_dir.display().to_string());
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("saveas_scroll")
                    .max_height(240.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if let Some(parent) = self.browse_dir.parent() {
                            if ui.button("📁 ..").clicked() {
                                new_dir = Some(parent.to_path_buf());
                            }
                        }
                        let (dirs, files) = list_dir(&self.browse_dir);
                        for d in dirs {
                            let name = d.file_name().unwrap_or_default().to_string_lossy().to_string();
                            if ui.button(format!("📁 {name}")).clicked() {
                                new_dir = Some(d);
                            }
                        }
                        for f in files {
                            let name = f.file_name().unwrap_or_default().to_string_lossy().to_string();
                            // Clic sur un fichier existant => reprend son nom (écrasement).
                            if ui.button(RichText::new(format!("📄 {name}")).color(HEADER)).clicked() {
                                self.saveas_name = name;
                            }
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Nom :");
                    ui.add(egui::TextEdit::singleline(&mut self.saveas_name).desired_width(220.0));
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("💾 Enregistrer").clicked() {
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
        let mut chosen: Option<PathBuf> = None;
        let mut new_dir: Option<PathBuf> = None;
        egui::Window::new("Ouvrir un fichier .asm")
            .collapsible(false)
            .resizable(true)
            .default_width(460.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("📂");
                    ui.monospace(self.browse_dir.display().to_string());
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("browser_scroll")
                    .max_height(320.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if let Some(parent) = self.browse_dir.parent() {
                            if ui.button("📁 ..").clicked() {
                                new_dir = Some(parent.to_path_buf());
                            }
                        }
                        let (dirs, files) = list_dir(&self.browse_dir);
                        for d in dirs {
                            let name = d.file_name().unwrap_or_default().to_string_lossy().to_string();
                            if ui.button(format!("📁 {name}")).clicked() {
                                new_dir = Some(d);
                            }
                        }
                        for f in files {
                            let name = f.file_name().unwrap_or_default().to_string_lossy().to_string();
                            if ui
                                .button(RichText::new(format!("📄 {name}")).color(MNEMONIC))
                                .clicked()
                            {
                                chosen = Some(f);
                            }
                        }
                    });
                ui.separator();
                if ui.button("Annuler").clicked() {
                    cancel = true;
                }
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
                if bordered_button(ui, "▶  Lancer", !running)
                    .on_hover_text("F5")
                    .clicked()
                {
                    self.launch();
                }
                // Précédent : recule d'une étape dans la timeline enregistrée.
                let can_prev = self.dbg.is_some() && self.view_index > 0;
                if ui
                    .add_enabled(can_prev, egui::Button::new("◀  Précédent"))
                    .on_hover_text("Étape précédente (←)")
                    .clicked()
                {
                    self.set_view(self.view_index as i64 - 1);
                }
                let can_step = self.can_step();
                if ui
                    .add_enabled(can_step, egui::Button::new("⏭  Step"))
                    .on_hover_text("F10 / F8")
                    .clicked()
                {
                    self.step();
                }
                if ui.button("🔄  Restart").on_hover_text("F5").clicked() {
                    self.launch();
                }
                // Stop : vert + actif quand un programme tourne ; rouge + inactif sinon.
                if bordered_button(ui, "⏹  Stop", running)
                    .on_hover_text("Échap")
                    .clicked()
                {
                    self.stop();
                }
                ui.separator();
                if ui.button("🔨  Build").on_hover_text("Assembler + Lier (Ctrl+B)").clicked() {
                    self.build();
                }
                if ui.button("💾  Enregistrer").on_hover_text("Ctrl+S").clicked() {
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
                ui.columns(2, |c| {
                    self.memory_ui(&mut c[0]);
                    self.console_ui(&mut c[1]);
                });
            });
    }

    /// Barre timeline pleine largeur (au-dessus de la barre d'état).
    fn timeline_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("timeline_panel").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                header_inline(ui, "TIMELINE");
                ui.separator();
                let Some(last) = self.dbg.as_ref().map(|d| d.history.len() - 1) else {
                    ui.weak("— lancez un programme pour enregistrer la timeline");
                    return;
                };
                if ui.button("⏮").on_hover_text("Début (Home)").clicked() {
                    self.set_view(0);
                }
                if ui.button("◀").on_hover_text("Précédent (←)").clicked() {
                    self.set_view(self.view_index as i64 - 1);
                }
                if ui.button("▶").on_hover_text("Suivant (→)").clicked() {
                    self.set_view(self.view_index as i64 + 1);
                }
                if ui.button("⏭").on_hover_text("Fin (End)").clicked() {
                    self.set_view(i64::MAX);
                }
                ui.label(RichText::new(format!("{} / {last}", self.view_index)).monospace().strong());

                if !self.is_head_view()
                    && ui
                        .button("⟳ Reprendre ici")
                        .on_hover_text("Ré-exécute jusqu'à cette étape pour continuer")
                        .clicked()
                {
                    self.resume_here();
                }

                // Instruction de l'étape (à droite).
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

                // Slider occupant tout l'espace restant.
                let mut idx = self.view_index;
                ui.spacing_mut().slider_width = (ui.available_width() - 24.0).max(80.0);
                if ui
                    .add(egui::Slider::new(&mut idx, 0..=last).show_value(false))
                    .changed()
                {
                    self.view_index = idx;
                }
            });
            ui.add_space(2.0);
        });
    }

    fn memory_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            header_inline(ui, "MEMORY");
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.mem_input)
                    .desired_width(120.0)
                    .font(egui::TextStyle::Monospace),
            );
            let go = ui.button("Aller").clicked()
                || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if go {
                if let Some(a) = parse_hex(&self.mem_input) {
                    self.mem_addr = a;
                }
            }
        });
        ui.separator();
        if !self.can_read_memory() {
            ui.weak("Mémoire lisible sur l'état courant (revenez à la dernière étape).");
            return;
        }
        let dbg = self.dbg.as_ref().unwrap();
        egui::ScrollArea::vertical()
            .id_salt("mem_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for row in 0..8u64 {
                    let base = self.mem_addr.wrapping_add(row * 16);
                    let (hex, ascii) = match dbg.read_mem(base, 16) {
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
                        ui.label(RichText::new(format!("0x{base:08X}")).monospace().color(ADDR_COL));
                        ui.label(RichText::new(hex).monospace().color(BYTES_COL));
                        ui.label(RichText::new(ascii).monospace().weak());
                    });
                }
            });
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

    fn registers_ui(&self, ui: &mut egui::Ui) {
        header(ui, "REGISTERS");
        let (Some(snap), Some(prev)) = (self.snap(), self.prev_snap()) else {
            ui.label("Aucun programme lancé.");
            return;
        };
        egui::Grid::new("regs_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
            for ((name, val), (_, pval)) in snap.regs.named().iter().zip(prev.regs.named()) {
                ui.label(RichText::new(*name).monospace().strong());
                let mut value = RichText::new(format!("0x{val:016X}")).monospace();
                if *val != pval {
                    value = value.color(CHANGED);
                }
                ui.label(value);
                ui.end_row();
            }
        });
        ui.add_space(10.0);
        header(ui, "FLAGS");
        let flags = Flags::from_eflags(snap.regs.eflags);
        let prevf = Flags::from_eflags(prev.regs.eflags);
        egui::Grid::new("flags_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
            for ((name, val), (_, pval)) in flags.named().iter().zip(prevf.named()) {
                let mut label = RichText::new(*name).monospace();
                if *val != pval {
                    label = label.color(CHANGED);
                }
                ui.label(label);
                let color = if *val { FLAG_ON } else { FLAG_OFF };
                ui.label(RichText::new(if *val { "1" } else { "0" }).monospace().color(color));
                ui.end_row();
            }
        });
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
        let mut layouter = |ui: &egui::Ui, text: &str, wrap: f32| {
            let mut job = highlight_nasm(text);
            job.wrap.max_width = wrap;
            ui.fonts(|f| f.layout_job(job))
        };
        egui::ScrollArea::both().id_salt("editor_scroll").show(ui, |ui| {
            let resp = ui.add(
                egui::TextEdit::multiline(&mut self.source)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(28)
                    .lock_focus(true)
                    .layouter(&mut layouter),
            );
            if resp.changed() {
                self.dirty = true;
            }
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

    // ---------- Pile ----------

    fn stack_ui(&self, ui: &mut egui::Ui) {
        header(ui, "STACK");
        let Some(snap) = self.snap() else {
            ui.label("—");
            return;
        };
        let (rsp, rbp) = (snap.regs.rsp, snap.regs.rbp);
        egui::ScrollArea::vertical().id_salt("stack_scroll").show(ui, |ui| {
            egui::Grid::new("stack_grid").num_columns(3).spacing([10.0, 3.0]).show(ui, |ui| {
                for (i, val) in snap.stack.iter().enumerate() {
                    let addr = rsp.wrapping_add((i as u64) * 8);
                    ui.label(RichText::new(format!("0x{addr:012X}")).monospace().color(ADDR_COL));
                    ui.label(RichText::new(format!("0x{val:016X}")).monospace());
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

/// Coloration syntaxique NASM → LayoutJob (comments, mnémoniques, registres…).
fn highlight_nasm(text: &str) -> LayoutJob {
    let font = egui::FontId::monospace(13.0);
    let mut job = LayoutJob::default();
    for line in text.split_inclusive('\n') {
        let (code, comment) = match line.find(';') {
            Some(i) => (&line[..i], &line[i..]),
            None => (line, ""),
        };
        append_code(&mut job, code, &font);
        if !comment.is_empty() {
            append(&mut job, comment, SYN_COMMENT, &font);
        }
    }
    job
}

fn append(job: &mut LayoutJob, text: &str, color: Color32, font: &egui::FontId) {
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: font.clone(),
            color,
            ..Default::default()
        },
    );
}

/// Découpe une portion de code (hors commentaire) en tokens colorés.
fn append_code(job: &mut LayoutJob, code: &str, font: &egui::FontId) {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '@';
    let mut rest = code;
    let mut mnem_pending = true;
    while !rest.is_empty() {
        let c = rest.chars().next().unwrap();
        if is_ident(c) {
            let end = rest.find(|ch: char| !is_ident(ch)).unwrap_or(rest.len());
            let word = &rest[..end];
            let after = &rest[end..];
            let is_label = after.trim_start().starts_with(':');
            let color = if is_number(word) {
                SYN_NUMBER
            } else if is_register(word) {
                SYN_REGISTER
            } else if is_directive(word) {
                SYN_DIRECTIVE
            } else if is_label {
                SYN_LABEL
            } else if mnem_pending {
                mnem_pending = false;
                SYN_MNEMONIC
            } else {
                SYN_TEXT
            };
            append(job, word, color, font);
            rest = after;
        } else {
            let end = rest.find(is_ident).unwrap_or(rest.len());
            append(job, &rest[..end], SYN_TEXT, font);
            rest = &rest[end..];
        }
    }
}

fn is_number(w: &str) -> bool {
    w.chars().next().is_some_and(|c| c.is_ascii_digit())
}

fn is_register(w: &str) -> bool {
    const REGS: &[&str] = &[
        "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rbp", "rsp", "rip", "r8", "r9", "r10", "r11",
        "r12", "r13", "r14", "r15", "eax", "ebx", "ecx", "edx", "esi", "edi", "ebp", "esp", "r8d",
        "r9d", "r10d", "r11d", "r12d", "r13d", "r14d", "r15d", "ax", "bx", "cx", "dx", "si", "di",
        "bp", "sp", "al", "bl", "cl", "dl", "ah", "bh", "ch", "dh", "sil", "dil",
    ];
    let l = w.to_ascii_lowercase();
    REGS.contains(&l.as_str())
}

fn is_directive(w: &str) -> bool {
    const DIRS: &[&str] = &[
        "section", "global", "extern", "db", "dw", "dd", "dq", "dt", "resb", "resw", "resd", "resq",
        "equ", "times", "align", "default", "bits", "byte", "word", "dword", "qword",
    ];
    let l = w.to_ascii_lowercase();
    DIRS.contains(&l.as_str())
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
    fn syntax_highlight_covers_whole_line() {
        // Chaque caractère doit être stylé (aucune perte de texte à l'affichage).
        let src = "  mov rax, 5   ; commentaire\n";
        let job = highlight_nasm(src);
        assert_eq!(job.text, src);
    }
}
