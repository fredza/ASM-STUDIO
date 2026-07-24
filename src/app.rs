//! Application eframe : barre d'outils + panneau Registres/Flags (M1).

use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText};

use crate::assemble;
use crate::debugger::{Debugger, Flags, RunState};

/// Couleur d'une valeur qui vient de changer au dernier step.
const CHANGED: Color32 = Color32::from_rgb(0xF5, 0xA6, 0x23); // orange
const FLAG_ON: Color32 = Color32::from_rgb(0x4C, 0xAF, 0x50); // vert
const FLAG_OFF: Color32 = Color32::from_rgb(0x88, 0x88, 0x88); // gris

pub struct App {
    src_path: PathBuf,
    out_dir: PathBuf,
    binary: Option<PathBuf>,
    dbg: Option<Debugger>,
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
            .default_height(140.0)
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

        // --- Flags (panneau droit) ---
        egui::SidePanel::right("flags")
            .resizable(false)
            .default_width(140.0)
            .show(ctx, |ui| {
                ui.label(RichText::new("FLAGS").strong());
                ui.separator();
                let (flags, prev) = match &self.dbg {
                    Some(d) => (d.flags(), d.prev_flags()),
                    None => (Flags::default(), Flags::default()),
                };
                egui::Grid::new("flags_grid").num_columns(2).show(ui, |ui| {
                    for ((name, val), (_, pval)) in flags.named().iter().zip(prev.named()) {
                        let changed = *val != pval;
                        let mut label = RichText::new(*name).monospace();
                        if changed {
                            label = label.color(CHANGED);
                        }
                        ui.label(label);
                        let color = if *val { FLAG_ON } else { FLAG_OFF };
                        ui.label(RichText::new(if *val { "1" } else { "0" }).monospace().color(color));
                        ui.end_row();
                    }
                });
            });

        // --- Registres (centre) ---
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(RichText::new("REGISTERS").strong());
            ui.separator();
            match &self.dbg {
                Some(d) => {
                    egui::Grid::new("regs_grid")
                        .num_columns(2)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            for ((name, val), (_, pval)) in
                                d.regs.named().iter().zip(d.prev.named())
                            {
                                let changed = *val != pval;
                                ui.label(RichText::new(*name).monospace().strong());
                                let mut value =
                                    RichText::new(format!("0x{val:016X}")).monospace();
                                if changed {
                                    value = value.color(CHANGED);
                                }
                                ui.label(value);
                                ui.end_row();
                            }
                        });
                }
                None => {
                    ui.label("Aucun programme lancé. Cliquez sur « Lancer ».");
                }
            }
        });
    }
}
