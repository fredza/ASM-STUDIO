//! Fenêtre de pas-à-pas Windows (expérimental) — voir `win_debug_ops`.

use eframe::egui::{self, RichText};

use crate::i18n;
use crate::win_debugger::WinRunState;

use super::{App, dialog_window, false_col, flag_on};

impl App {
    pub(super) fn win_debug_window(&mut self, ctx: &egui::Context) {
        if !self.show_win_debug {
            return;
        }
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let mut open = true;
        dialog_window(
            ctx,
            tr(
                "Pas-à-pas Windows (Wine) — expérimental",
                "Windows step debugging (Wine) — experimental",
                "Paso a paso Windows (Wine) — experimental",
            ),
        )
        .resizable(true)
        .default_width(420.0)
        .min_width(340.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(tr(
                    "winedbg fait tourner le programme derrière un vrai chargeur Windows : \
                     les registres et l'arrêt sont réels, mais rien de la sortie du programme \
                     n'apparaît ici, et aucun exercice ni prédiction ne s'y vérifie. Le débogueur \
                     tourne à part : si le programme attend quelque chose qu'on ne peut pas encore \
                     lui donner (une saisie, un clic sur une boîte de dialogue), l'IDE reste \
                     utilisable et « Arrêter » coupe court.",
                    "winedbg runs the program behind a real Windows loader: registers and \
                     stopping are real, but none of the program's output shows up here, and no \
                     exercise or prediction is checked here. The debugger runs on its own thread: \
                     if the program waits for something it cannot yet be given (input, a click on \
                     a dialog box), the IDE stays usable and “Stop” cuts it short.",
                    "winedbg ejecuta el programa detrás de un cargador de Windows real: los \
                     registros y la parada son reales, pero nada de la salida del programa \
                     aparece aquí, y ningún ejercicio ni predicción se verifica aquí. El depurador \
                     se ejecuta aparte: si el programa espera algo que aún no se le puede dar (una \
                     entrada, un clic en un cuadro de diálogo), el IDE sigue utilizable y \
                     «Detener» lo interrumpe.",
                ))
                .small()
                .weak(),
            );
            ui.add_space(6.0);

            if !self.target.is_windows() {
                ui.colored_label(
                    false_col(),
                    tr(
                        "La cible courante n'est pas Windows (PE64) : changez de cible dans le menu Exécution.",
                        "The current target is not Windows (PE64): change target from the Run menu.",
                        "El destino actual no es Windows (PE64): cambie el destino en el menú Ejecución.",
                    ),
                );
            } else if self.win_dbg.is_none() {
                if !self.win_debug_available() {
                    ui.colored_label(
                        false_col(),
                        tr(
                            "Wine ou winedbg introuvable : installez Wine pour utiliser ce pas-à-pas.",
                            "Wine or winedbg not found: install Wine to use this step debugger.",
                            "No se encontró Wine o winedbg: instale Wine para usar este depurador.",
                        ),
                    );
                } else if ui.button(tr("Démarrer", "Start", "Iniciar")).clicked() {
                    self.win_debug_start();
                }
            } else {
                self.win_debug_session_ui(ui, &tr);
            }
        });
        if !open {
            self.show_win_debug = false;
        }
    }

    fn win_debug_session_ui(&mut self, ui: &mut egui::Ui, tr: &dyn Fn(&'static str, &'static str, &'static str) -> &'static str) {
        let Some(session) = self.win_dbg.as_ref() else { return };
        // Une commande est en vol : le débogueur travaille sur son thread, et
        // rien ici n'est à jour. Le dire explicitement est tout le sujet — un
        // élève qui ne voit rien bouger croit l'IDE figé, ce qu'il était
        // vraiment avant que la session ne passe sur un thread à elle.
        let busy = session.is_busy();
        if session.is_starting() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tr(
                    "Démarrage de winedbg… (Wine peut prendre quelques secondes)",
                    "Starting winedbg… (Wine may take a few seconds)",
                    "Iniciando winedbg… (Wine puede tardar unos segundos)",
                ));
            });
            ui.add_space(8.0);
            if ui.button(tr("Arrêter", "Stop", "Detener")).clicked() {
                self.win_debug_stop();
            }
            return;
        }
        let Some(snap) = session.snapshot().cloned() else { return };
        let alive = snap.state == WinRunState::Stopped;
        let state = snap.state;
        let regs = snap.regs;
        let flags = snap.flags;
        // Ce qui porte vraiment un `0xCC` en mémoire au dernier arrêt, pas ce
        // que l'éditeur affiche — les deux peuvent diverger un instant si un
        // point a été posé après le dernier « Continuer ».
        let bp_count = snap.breakpoints;
        let line = self.src_map.get(&regs.rip).copied();

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(match state {
                    WinRunState::Stopped => tr("Arrêté", "Stopped", "Detenido"),
                    WinRunState::Exited(_) => tr("Terminé", "Exited", "Terminado"),
                    WinRunState::Signaled => tr("Terminé (signal)", "Terminated (signal)", "Terminado (señal)"),
                })
                .strong(),
            );
            if let WinRunState::Exited(code) = state {
                ui.label(format!("(exit {code})"));
            }
        });
        ui.label(RichText::new(format!("RIP = 0x{:X}", regs.rip)).monospace());
        if let Some(l) = line {
            ui.label(format!("{} {l}", tr("ligne", "line", "línea")));
        }
        ui.add_space(6.0);

        egui::Grid::new("win_debug_regs")
            .num_columns(4)
            .spacing([12.0, 3.0])
            .show(ui, |ui| {
                for (i, (name, val)) in regs.named().into_iter().enumerate() {
                    ui.label(RichText::new(name).monospace().weak());
                    ui.label(RichText::new(format!("0x{val:016X}")).monospace());
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for (name, on) in flags.named() {
                let col = if on { flag_on() } else { ui.visuals().weak_text_color() };
                ui.label(RichText::new(name).monospace().color(col));
            }
        });
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            // Grisés pendant une commande : la suivante n'aurait nulle part
            // où aller (le thread est occupé) et s'empilerait pour se dérouler
            // toute seule plus tard. « Arrêter », lui, reste cliquable en
            // permanence — c'est justement le seul recours quand le programme
            // s'est arrêté sur quelque chose qui n'arrivera jamais.
            let ready = alive && !busy;
            if ui.add_enabled(ready, egui::Button::new(tr("Suivant", "Next", "Siguiente"))).clicked() {
                self.win_debug_step();
            }
            if ui.add_enabled(ready, egui::Button::new(tr("Continuer", "Continue", "Continuar"))).clicked() {
                self.win_debug_cont();
            }
            if ui.button(tr("Arrêter", "Stop", "Detener")).clicked() {
                self.win_debug_stop();
            }
            if busy {
                ui.spinner();
                ui.label(
                    RichText::new(tr(
                        "En cours… (« Arrêter » interrompt)",
                        "Working… (“Stop” interrupts)",
                        "En curso… («Detener» interrumpe)",
                    ))
                    .weak(),
                );
            }
        });

        if bp_count > 0 {
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!(
                    "{bp_count} {}",
                    tr(
                        "point(s) d'arrêt posé(s) dans l'éditeur, appliqués ici aussi",
                        "breakpoint(s) set in the editor, applied here too",
                        "punto(s) de interrupción puestos en el editor, aplicados aquí también",
                    )
                ))
                .small()
                .weak(),
            );
        }
    }
}
