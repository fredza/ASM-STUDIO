//! Application eframe : menu, barre d'outils, panneaux (Registres/Flags,
//! Désassemblage, Pile, Instruction, Mémoire, Timeline, Console) + barre d'état.
//!
//! L'état affiché est toujours lu dans l'historique du debugger à l'index
//! `view_index` (timeline), ce qui unifie « état courant » et « scrubbing ».

use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText};

use crate::assemble;
use crate::debugger::{Debugger, RunState, Snapshot};
use crate::disasm::{self, Insn};
use crate::{explain, syscall};

// Palette (sombre, proche de la maquette).
const CHANGED: Color32 = Color32::from_rgb(0xF5, 0xA6, 0x23); // valeur modifiée (orange)
const FLAG_ON: Color32 = Color32::from_rgb(0x4C, 0xAF, 0x50); // vert
const FLAG_OFF: Color32 = Color32::from_rgb(0x88, 0x88, 0x88); // gris
const RIP_ROW: Color32 = Color32::from_rgb(0x3A, 0x33, 0x1E); // fond ligne RIP
const SEL_ROW: Color32 = Color32::from_rgb(0x2A, 0x2A, 0x33); // fond sélection
const ADDR_COL: Color32 = Color32::from_rgb(0x7F, 0x9C, 0xD1); // adresses
const BYTES_COL: Color32 = Color32::from_rgb(0x88, 0x88, 0x88); // octets
const MNEMONIC: Color32 = Color32::from_rgb(0x6E, 0xB4, 0xE8); // mnémonique
const FALSE_COL: Color32 = Color32::from_rgb(0xD9, 0x5B, 0x5B); // condition fausse (rouge)

pub struct App {
    src_path: PathBuf,
    out_dir: PathBuf,
    binary: Option<PathBuf>,
    dbg: Option<Debugger>,
    /// Désassemblage de `.text`, calculé une fois au lancement.
    disasm: Vec<Insn>,
    /// Instruction sélectionnée au clic (adresse) pour le panneau d'explication.
    selected: Option<u64>,
    /// Étape affichée dans la timeline (index dans l'historique).
    view_index: usize,
    /// Adresse de base de la fenêtre mémoire hexadécimale.
    mem_addr: u64,
    mem_input: String,
    console: String,
    status: String,
    /// Thème appliqué une seule fois.
    styled: bool,
    /// Affiche la boîte de dialogue « À propos ».
    show_about: bool,
}

impl App {
    pub fn new() -> Self {
        App {
            src_path: PathBuf::from("examples/test.asm"),
            out_dir: PathBuf::from("build"),
            binary: None,
            dbg: None,
            disasm: Vec::new(),
            selected: None,
            view_index: 0,
            mem_addr: 0,
            mem_input: String::new(),
            console: String::new(),
            status: "Prêt".to_string(),
            styled: false,
            show_about: false,
        }
    }

    // ---------- Actions ----------

    fn log(&mut self, s: &str) {
        self.console.push_str(s);
        if !s.ends_with('\n') {
            self.console.push('\n');
        }
    }

    fn build(&mut self) {
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
        if self.binary.is_none() {
            self.build();
        }
        let Some(bin) = self.binary.clone() else {
            return;
        };
        match disasm::disassemble_text(&bin) {
            Ok(insns) => self.disasm = insns,
            Err(e) => self.log(&e),
        }
        // Par défaut, la fenêtre mémoire pointe sur .data (ou .text à défaut).
        self.mem_addr = disasm::section_address(&bin, ".data")
            .or_else(|| disasm::section_address(&bin, ".text"))
            .unwrap_or(0);
        self.mem_input = format!("0x{:X}", self.mem_addr);

        self.selected = None;
        self.view_index = 0;
        self.dbg = None; // Drop tue l'ancien processus tracé
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

    /// Avance d'une instruction (seulement en tête de timeline) et journalise
    /// les appels système rencontrés.
    fn step(&mut self) {
        if !self.can_step() {
            return;
        }
        // Détecte un syscall sur le point de s'exécuter (RIP == instruction syscall).
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
        // La vue suit la tête.
        if let Some(d) = self.dbg.as_ref() {
            self.view_index = d.history.len() - 1;
        }

        // Journalise le syscall (retour lu après exécution, sauf exit).
        if let Some((call, num)) = pending {
            if syscall::is_exit(num) {
                self.log(&call);
            } else if let Some(d) = self.dbg.as_ref() {
                self.log(&format!("{call} = {}", d.regs().rax as i64));
            }
        }

        // Statut.
        match self.dbg.as_ref().map(|d| d.state) {
            Some(RunState::Stopped) => {
                let d = self.dbg.as_ref().unwrap();
                self.status = format!("Step {} — RIP @ 0x{:X}", d.steps(), d.regs().rip);
            }
            Some(RunState::Exited(code)) => {
                self.status = format!("Terminé (exit {code})");
            }
            Some(RunState::Signaled) => self.status = "Terminé (signal)".to_string(),
            None => {}
        }
    }

    /// Re-exécute depuis le début jusqu'à l'étape actuellement affichée, pour
    /// « reprendre » l'exécution depuis un point passé (ptrace ne recule pas).
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

    // ---------- Accès à l'état affiché (via la timeline) ----------

    fn snap(&self) -> Option<&Snapshot> {
        let d = self.dbg.as_ref()?;
        let i = self.view_index.min(d.history.len().saturating_sub(1));
        d.history.get(i)
    }

    fn prev_snap(&self) -> Option<&Snapshot> {
        let d = self.dbg.as_ref()?;
        let i = self.view_index.min(d.history.len().saturating_sub(1));
        d.history.get(i.saturating_sub(1))
    }

    fn is_head_view(&self) -> bool {
        match &self.dbg {
            Some(d) => self.view_index >= d.history.len() - 1,
            None => false,
        }
    }

    fn can_step(&self) -> bool {
        self.dbg.as_ref().is_some_and(|d| d.is_alive()) && self.is_head_view()
    }

    /// Mémoire lisible seulement sur l'état vivant courant (non snapshotée).
    fn can_read_memory(&self) -> bool {
        self.is_head_view() && self.dbg.as_ref().is_some_and(|d| d.is_alive())
    }

    fn view_rip(&self) -> Option<u64> {
        self.snap().map(|s| s.regs.rip)
    }

    fn set_view(&mut self, idx: i64) {
        if let Some(d) = &self.dbg {
            let last = (d.history.len() - 1) as i64;
            self.view_index = idx.clamp(0, last) as usize;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.styled {
            ctx.set_visuals(egui::Visuals::dark());
            self.styled = true;
        }

        self.handle_shortcuts(ctx);

        self.menu_bar(ctx);
        self.toolbar(ctx);
        self.status_bar(ctx);
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

        egui::CentralPanel::default().show(ctx, |ui| self.disasm_ui(ui));

        self.about_window(ctx);
    }
}

impl App {
    // ---------- Raccourcis clavier ----------

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::Key;
        let (step, run, stop, build, first, prev, next, last) = ctx.input(|i| {
            let ctrl = i.modifiers.ctrl;
            (
                i.key_pressed(Key::F10) || i.key_pressed(Key::F8),
                i.key_pressed(Key::F5),
                i.key_pressed(Key::Escape) || (i.modifiers.shift && i.key_pressed(Key::F5)),
                ctrl && i.key_pressed(Key::B),
                i.key_pressed(Key::Home),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                i.key_pressed(Key::End),
            )
        });
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
        if self.dbg.is_some() {
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
                if let Some(d) = &self.dbg {
                    self.view_index = d.history.len() - 1;
                }
            }
        }
    }

    // ---------- Boîte de dialogue « À propos » ----------

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
                        ui.label(
                            RichText::new(env!("CARGO_PKG_VERSION"))
                                .monospace()
                                .strong(),
                        );
                        ui.end_row();
                        ui.label("Build");
                        ui.label(RichText::new(env!("GIT_HASH")).monospace().strong());
                        ui.end_row();
                        ui.label("Date");
                        ui.label(RichText::new(env!("BUILD_DATE")).monospace());
                        ui.end_row();
                        ui.label("Licence");
                        ui.label("MIT");
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

    // ---------- Menu ----------

    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menubar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Fichier", |ui| {
                    ui.label(RichText::new(self.src_path.display().to_string()).weak());
                    ui.separator();
                    if ui.button("Quitter").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Build", |ui| {
                    if ui.button("Assembler        Ctrl+B").clicked() {
                        self.build();
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
                    ui.label("Raccourcis").on_hover_ui(|ui| shortcuts_tooltip(ui));
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
            ui.horizontal(|ui| {
                if ui.button("▶ Lancer").on_hover_text("F5").clicked() {
                    self.launch();
                }
                let can_step = self.can_step();
                if ui
                    .add_enabled(can_step, egui::Button::new("⏭ Step"))
                    .on_hover_text("F10 / F8")
                    .clicked()
                {
                    self.step();
                }
                if ui.button("🔄 Restart").on_hover_text("F5").clicked() {
                    self.launch();
                }
                if ui.button("⏹ Stop").on_hover_text("Échap").clicked() {
                    self.stop();
                }
                if ui.button("🔨 Build").on_hover_text("Ctrl+B").clicked() {
                    self.build();
                }
                ui.separator();
                ui.label(&self.status);
            });
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
                        ui.label(format!("PID: {}", d.pid()));
                    }
                    Some(d) => {
                        let msg = match d.state {
                            RunState::Exited(c) => format!("○ Exited ({c})"),
                            _ => "○ Terminé".to_string(),
                        };
                        ui.colored_label(FLAG_OFF, msg);
                        ui.separator();
                        ui.label(format!("PID: {}", d.pid()));
                    }
                    None => {
                        ui.colored_label(FLAG_OFF, "○ Prêt");
                    }
                }
                ui.separator();
                ui.label("Arch: x86_64");
                ui.separator();
                ui.label("Mode: 64-bit");
                ui.separator();
                if let Some(s) = self.snap() {
                    ui.label(format!("Stopped at: 0x{:X}", s.regs.rip));
                    if let Some(next) = self.next_addr() {
                        ui.separator();
                        ui.colored_label(CHANGED, format!("Next: 0x{next:X}"));
                    }
                }
            });
        });
    }

    /// Adresse de l'instruction qui suit RIP dans le désassemblage.
    fn next_addr(&self) -> Option<u64> {
        let rip = self.view_rip()?;
        let idx = self.disasm.iter().position(|i| i.address == rip)?;
        self.disasm.get(idx + 1).map(|i| i.address)
    }

    // ---------- Bande basse : Mémoire | Timeline | Console ----------

    fn bottom_band(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("bottom_band")
            .resizable(true)
            .default_height(210.0)
            .show(ctx, |ui| {
                ui.columns(3, |c| {
                    self.memory_ui(&mut c[0]);
                    self.timeline_ui(&mut c[1]);
                    self.console_ui(&mut c[2]);
                });
            });
    }

    fn memory_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("MEMORY").strong());
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.mem_input)
                    .desired_width(130.0)
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
            .max_height(150.0)
            .show(ui, |ui| {
                for row in 0..8u64 {
                    let base = self.mem_addr.wrapping_add(row * 16);
                    let (hex, ascii) = match dbg.read_mem(base, 16) {
                        Ok(bytes) => {
                            let hex = bytes
                                .iter()
                                .map(|b| format!("{b:02X}"))
                                .collect::<Vec<_>>()
                                .join(" ");
                            let ascii: String = bytes
                                .iter()
                                .map(|&b| {
                                    if (0x20..0x7f).contains(&b) {
                                        b as char
                                    } else {
                                        '.'
                                    }
                                })
                                .collect();
                            (hex, ascii)
                        }
                        Err(_) => ("?? ".repeat(16).trim_end().to_string(), ".".repeat(16)),
                    };
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("0x{base:08X}"))
                                .monospace()
                                .color(ADDR_COL),
                        );
                        ui.label(RichText::new(hex).monospace().color(BYTES_COL));
                        ui.label(RichText::new(ascii).monospace().weak());
                    });
                }
            });
    }

    fn timeline_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("TIMELINE").strong());
        ui.separator();
        let Some(last) = self.dbg.as_ref().map(|d| d.history.len() - 1) else {
            ui.weak("—");
            return;
        };

        ui.horizontal(|ui| {
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
                self.view_index = last;
            }
        });

        let mut idx = self.view_index;
        if ui
            .add(egui::Slider::new(&mut idx, 0..=last).text("étape"))
            .changed()
        {
            self.view_index = idx;
        }
        ui.label(format!("Instruction {} / {last}", self.view_index));

        if let Some(s) = self.snap() {
            if let Some(insn) = self.disasm.iter().find(|i| i.address == s.regs.rip) {
                ui.label(
                    RichText::new(format!("{} {}", insn.mnemonic, insn.operands)).monospace(),
                );
            }
        }

        if !self.is_head_view() {
            ui.add_space(4.0);
            if ui
                .button("⟳ Reprendre ici")
                .on_hover_text("Ré-exécute jusqu'à cette étape pour continuer")
                .clicked()
            {
                self.resume_here();
            }
        }
    }

    fn console_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("CONSOLE").strong());
            if ui.small_button("effacer").clicked() {
                self.console.clear();
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.console.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY),
                );
            });
    }

    // ---------- Registres + Flags ----------

    fn registers_ui(&self, ui: &mut egui::Ui) {
        ui.label(RichText::new("REGISTERS").strong());
        ui.separator();
        let (Some(snap), Some(prev)) = (self.snap(), self.prev_snap()) else {
            ui.label("Aucun programme lancé.");
            return;
        };

        egui::Grid::new("regs_grid")
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
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

        ui.add_space(8.0);
        ui.label(RichText::new("FLAGS").strong());
        ui.separator();
        let flags = crate::debugger::Flags::from_eflags(snap.regs.eflags);
        let prevf = crate::debugger::Flags::from_eflags(prev.regs.eflags);
        egui::Grid::new("flags_grid").num_columns(2).show(ui, |ui| {
            for ((name, val), (_, pval)) in flags.named().iter().zip(prevf.named()) {
                let mut label = RichText::new(*name).monospace();
                if *val != pval {
                    label = label.color(CHANGED);
                }
                ui.label(label);
                let color = if *val { FLAG_ON } else { FLAG_OFF };
                ui.label(
                    RichText::new(if *val { "1" } else { "0" })
                        .monospace()
                        .color(color),
                );
                ui.end_row();
            }
        });
    }

    // ---------- Désassemblage ----------

    fn disasm_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("DISASSEMBLY").strong());
        ui.separator();
        if self.disasm.is_empty() {
            ui.label("Cliquez sur « Lancer » pour désassembler et exécuter.");
            return;
        }
        let rip = self.view_rip();
        let mut clicked: Option<u64> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for insn in &self.disasm {
                let is_current = Some(insn.address) == rip;
                let is_selected = Some(insn.address) == self.selected;

                let inner = ui.horizontal(|ui| {
                    if is_current {
                        ui.label(RichText::new("➤").color(CHANGED));
                    } else {
                        ui.label("    ");
                    }
                    ui.label(
                        RichText::new(format!("0x{:08X}", insn.address))
                            .monospace()
                            .color(ADDR_COL),
                    );
                    ui.label(
                        RichText::new(format!("{:<20}", insn.bytes_hex()))
                            .monospace()
                            .color(BYTES_COL),
                    );
                    ui.label(
                        RichText::new(format!("{:<7}", insn.mnemonic))
                            .monospace()
                            .color(MNEMONIC),
                    );
                    ui.label(RichText::new(&insn.operands).monospace());
                });

                let row = inner.response.interact(egui::Sense::click());
                if row.clicked() {
                    clicked = Some(insn.address);
                }
                if is_current {
                    ui.painter()
                        .rect_filled(row.rect.expand2(egui::vec2(0.0, 1.0)), 2.0, RIP_ROW);
                }
                if is_selected && !is_current {
                    ui.painter()
                        .rect_filled(row.rect.expand2(egui::vec2(0.0, 1.0)), 2.0, SEL_ROW);
                }
                if row.hovered() {
                    ui.painter()
                        .rect_stroke(row.rect, 2.0, egui::Stroke::new(1.0_f32, ADDR_COL));
                }
            }
        });
        if let Some(addr) = clicked {
            self.selected = if self.selected == Some(addr) {
                None
            } else {
                Some(addr)
            };
        }
    }

    // ---------- Panneau INSTRUCTION (mode explication) ----------

    fn instruction_ui(&self, ui: &mut egui::Ui) {
        ui.label(RichText::new("INSTRUCTION").strong());
        ui.separator();

        let target = self.selected.or_else(|| self.view_rip());
        let Some(addr) = target else {
            ui.label("Lancez le programme, puis cliquez une instruction.");
            return;
        };
        let Some(insn) = self.disasm.iter().find(|i| i.address == addr) else {
            ui.label("—");
            return;
        };
        let flags = self
            .snap()
            .map(|s| crate::debugger::Flags::from_eflags(s.regs.eflags))
            .unwrap_or_default();

        let e = explain::explain(&insn.mnemonic, &insn.operands, flags);

        if self.selected.is_some() {
            ui.label(
                RichText::new("(sélection — reclic pour suivre RIP)")
                    .small()
                    .weak(),
            );
        } else {
            ui.label(RichText::new("(instruction courante)").small().weak());
        }
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
                    ui.label(
                        RichText::new(format!("{name} = {}", *val as u8))
                            .monospace()
                            .color(c),
                    );
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
            ui.label(
                RichText::new(e.affects_flags.join("  "))
                    .monospace()
                    .color(CHANGED),
            );
        }
    }

    // ---------- Pile ----------

    fn stack_ui(&self, ui: &mut egui::Ui) {
        ui.label(RichText::new("STACK").strong());
        ui.separator();
        let Some(snap) = self.snap() else {
            ui.label("—");
            return;
        };
        let rsp = snap.regs.rsp;
        let rbp = snap.regs.rbp;
        egui::Grid::new("stack_grid")
            .num_columns(3)
            .spacing([10.0, 3.0])
            .show(ui, |ui| {
                for (i, val) in snap.stack.iter().enumerate() {
                    let addr = rsp.wrapping_add((i as u64) * 8);
                    ui.label(
                        RichText::new(format!("0x{addr:012X}"))
                            .monospace()
                            .color(ADDR_COL),
                    );
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
    }
}

/// Analyse une adresse hexadécimale saisie (« 0x401000 » ou « 401000 »).
fn parse_hex(s: &str) -> Option<u64> {
    let s = s.trim();
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u64::from_str_radix(s, 16).ok()
}

/// Infobulle listant les raccourcis clavier.
fn shortcuts_tooltip(ui: &mut egui::Ui) {
    let rows = [
        ("F5", "Lancer / Restart"),
        ("F10 / F8", "Step (une instruction)"),
        ("Échap / Maj+F5", "Stop"),
        ("Ctrl+B", "Assembler (Build)"),
        ("←  /  →", "Timeline : précédent / suivant"),
        ("Home / End", "Timeline : début / fin"),
    ];
    egui::Grid::new("shortcuts").num_columns(2).show(ui, |ui| {
        for (k, d) in rows {
            ui.label(RichText::new(k).monospace().strong());
            ui.label(d);
            ui.end_row();
        }
    });
}
