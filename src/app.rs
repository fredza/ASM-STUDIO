//! Application eframe : barre d'outils + Registres/Flags + Désassemblage + Pile.

use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText};

use crate::assemble;
use crate::debugger::{Debugger, RunState};
use crate::disasm::{self, Insn};

/// Couleur d'une valeur qui vient de changer au dernier step.
const CHANGED: Color32 = Color32::from_rgb(0xF5, 0xA6, 0x23); // orange
const FLAG_ON: Color32 = Color32::from_rgb(0x4C, 0xAF, 0x50); // vert
const FLAG_OFF: Color32 = Color32::from_rgb(0x88, 0x88, 0x88); // gris
const RIP_ROW: Color32 = Color32::from_rgb(0x3A, 0x33, 0x1E); // fond de la ligne RIP
const ADDR_COL: Color32 = Color32::from_rgb(0x7F, 0x9C, 0xD1); // adresses (bleuté)
const BYTES_COL: Color32 = Color32::from_rgb(0x88, 0x88, 0x88); // octets (gris)
const MNEMONIC: Color32 = Color32::from_rgb(0x6E, 0xB4, 0xE8); // mnémonique

/// Nombre de mots de pile affichés à partir de RSP.
const STACK_ROWS: usize = 12;

pub struct App {
    src_path: PathBuf,
    out_dir: PathBuf,
    binary: Option<PathBuf>,
    dbg: Option<Debugger>,
    /// Désassemblage de `.text`, calculé une fois au lancement.
    disasm: Vec<Insn>,
    console: String,
    status: String,
}

impl App {
    pub fn new() -> Self {
        App {
            src_path: PathBuf::from("examples/test.asm"),
            out_dir: PathBuf::from("build"),
            binary: None,
            dbg: None,
            disasm: Vec::new(),
            console: String::new(),
            status: "Prêt".to_string(),
        }
    }

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
        // Désassemble le .text pour la vue centrale.
        match disasm::disassemble_text(&bin) {
            Ok(insns) => self.disasm = insns,
            Err(e) => self.log(&e),
        }
        self.dbg = None; // relâche l'ancien processus (Drop tue le tracé)
        match Debugger::launch(&bin) {
            Ok(dbg) => {
                self.status = format!("Lancé — RIP @ 0x{:X}", dbg.regs.rip);
                self.log("Running...");
                self.dbg = Some(dbg);
            }
            Err(e) => {
                self.log(&e);
                self.status = "Échec lancement".to_string();
            }
        }
    }

    fn step(&mut self) {
        let Some(dbg) = self.dbg.as_mut() else { return };
        if let Err(e) = dbg.step() {
            self.log(&e);
            return;
        }
        match dbg.state {
            RunState::Stopped => {
                self.status = format!("Step {} — RIP @ 0x{:X}", dbg.steps, dbg.regs.rip);
            }
            RunState::Exited(code) => {
                self.status = format!("Terminé (exit {code})");
                self.log(&format!("exit({code})"));
            }
            RunState::Signaled => {
                self.status = "Terminé (signal)".to_string();
            }
        }
    }

    /// Adresse de l'instruction courante (RIP), si un programme est vivant.
    fn current_rip(&self) -> Option<u64> {
        self.dbg.as_ref().filter(|d| d.is_alive()).map(|d| d.regs.rip)
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Barre d'outils ---
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("🔨 Build").clicked() {
                    self.build();
                }
                if ui.button("▶ Lancer").clicked() {
                    self.launch();
                }
                let can_step = self.dbg.as_ref().is_some_and(Debugger::is_alive);
                if ui.add_enabled(can_step, egui::Button::new("⏭ Step")).clicked() {
                    self.step();
                }
                if ui.button("🔄 Restart").clicked() {
                    self.launch();
                }
                if ui.button("⏹ Stop").clicked() {
                    self.dbg = None;
                    self.status = "Arrêté".to_string();
                }
                ui.separator();
                ui.label(&self.status);
            });
        });

        // --- Console (bas) ---
        egui::TopBottomPanel::bottom("console")
            .resizable(true)
            .default_height(120.0)
            .show(ctx, |ui| {
                ui.label(RichText::new("CONSOLE").strong());
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.console.as_str())
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY),
                        );
                    });
            });

        // --- Registres + Flags (gauche) ---
        egui::SidePanel::left("regs_panel")
            .resizable(false)
            .default_width(240.0)
            .show(ctx, |ui| {
                self.registers_ui(ui);
            });

        // --- Pile (droite) ---
        egui::SidePanel::right("stack_panel")
            .resizable(false)
            .default_width(280.0)
            .show(ctx, |ui| {
                self.stack_ui(ui);
            });

        // --- Désassemblage (centre) ---
        egui::CentralPanel::default().show(ctx, |ui| {
            self.disasm_ui(ui);
        });
    }
}

impl App {
    fn registers_ui(&self, ui: &mut egui::Ui) {
        ui.label(RichText::new("REGISTERS").strong());
        ui.separator();
        match &self.dbg {
            Some(d) => {
                egui::Grid::new("regs_grid")
                    .num_columns(2)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        for ((name, val), (_, pval)) in d.regs.named().iter().zip(d.prev.named()) {
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
                let (flags, prev) = (d.flags(), d.prev_flags());
                egui::Grid::new("flags_grid").num_columns(2).show(ui, |ui| {
                    for ((name, val), (_, pval)) in flags.named().iter().zip(prev.named()) {
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
            None => {
                ui.label("Aucun programme lancé.");
            }
        }
    }

    fn disasm_ui(&self, ui: &mut egui::Ui) {
        ui.label(RichText::new("DISASSEMBLY").strong());
        ui.separator();
        if self.disasm.is_empty() {
            ui.label("Cliquez sur « Lancer » pour désassembler et exécuter.");
            return;
        }
        let rip = self.current_rip();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for insn in &self.disasm {
                let is_current = Some(insn.address) == rip;
                let row = egui::RichText::new(format!("0x{:08X}", insn.address))
                    .monospace()
                    .color(ADDR_COL);

                let resp = ui.horizontal(|ui| {
                    // Flèche RIP
                    if is_current {
                        ui.label(RichText::new("➤").color(CHANGED));
                    } else {
                        ui.label("  ");
                    }
                    ui.label(row);
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

                // Surligne la ligne courante.
                if is_current {
                    ui.painter().rect_filled(
                        resp.response.rect.expand2(egui::vec2(0.0, 1.0)),
                        2.0,
                        RIP_ROW,
                    );
                }
            }
        });
    }

    fn stack_ui(&self, ui: &mut egui::Ui) {
        ui.label(RichText::new("STACK").strong());
        ui.separator();
        let Some(dbg) = self.dbg.as_ref().filter(|d| d.is_alive()) else {
            ui.label("—");
            return;
        };
        let rsp = dbg.regs.rsp;
        let rbp = dbg.regs.rbp;
        let words = dbg.read_qwords(rsp, STACK_ROWS);
        egui::Grid::new("stack_grid")
            .num_columns(3)
            .spacing([10.0, 3.0])
            .show(ui, |ui| {
                for (i, val) in words.iter().enumerate() {
                    let addr = rsp + (i as u64) * 8;
                    ui.label(
                        RichText::new(format!("0x{addr:012X}"))
                            .monospace()
                            .color(ADDR_COL),
                    );
                    ui.label(RichText::new(format!("0x{val:016X}")).monospace());
                    // Repères RSP / RBP.
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
