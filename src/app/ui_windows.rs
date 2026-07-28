use eframe::egui::{self, RichText};

use crate::debugger::Flags;
use crate::explain;
use crate::i18n;
use crate::syscall;

use super::{
    App, CHANGED, PUSH_COL, POP_COL,
    micro_stack, micro_static_flags,
    calc_parse, calc_format,
};

impl App {
    // ---------- Boîtes de dialogue ----------

    /// Mode « microscope » : tout ce qui se passe pour UNE instruction.
    pub(super) fn microscope_window(&mut self, ctx: &egui::Context) {
        let Some(addr) = self.microscope else { return };
        let Some(insn) = self.disasm.iter().find(|i| i.address == addr).cloned() else {
            self.microscope = None;
            return;
        };
        let flags_now = self.snap().map(|s| Flags::from_eflags(s.regs.eflags)).unwrap_or_default();
        let e = explain::explain(&insn.mnemonic, &insn.operands, flags_now, self.lang);
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
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str| i18n::tr(lang, fr, en);
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
                        ui.label(RichText::new(tr("Adresse", "Address")).strong());
                        ui.label(RichText::new(format!("0x{:08X}", insn.address)).monospace().color(addr_c));
                        ui.end_row();
                        ui.label(RichText::new(tr("Octets machine", "Machine bytes")).strong());
                        ui.label(RichText::new(insn.bytes_hex()).monospace().color(bytes_c));
                        ui.end_row();
                        ui.label(RichText::new(tr("Décodage", "Decoding")).strong());
                        ui.label(
                            RichText::new(format!("{} {}", insn.mnemonic, insn.operands))
                                .monospace()
                                .color(mnem_c),
                        );
                        ui.end_row();
                        ui.label(RichText::new(tr("Catégorie", "Category")).strong());
                        ui.label(e.category);
                        ui.end_row();
                        ui.label(RichText::new(tr("Cycles estimés", "Estimated cycles")).strong());
                        ui.label(RichText::new(cycles).color(CHANGED))
                            .on_hover_text(tr("Ordre de grandeur pédagogique, pas une mesure exacte.", "Educational ballpark, not an exact measurement."));
                        ui.end_row();
                        // Ligne syscall dans la grille d'identité (si applicable).
                        if insn.mnemonic == "syscall"
                            && let Some((before, _, _)) = &dynamics
                        {
                            ui.label(RichText::new(tr("Appel système", "System call")).strong());
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("#{}", before.rax))
                                        .monospace()
                                        .color(bytes_c),
                                );
                                ui.label(
                                    RichText::new(syscall::name(before.rax))
                                        .monospace()
                                        .strong()
                                        .color(mnem_c),
                                );
                            });
                            ui.end_row();
                            ui.label(RichText::new(tr("Arguments", "Arguments")).strong());
                            ui.label(
                                RichText::new(syscall::format_call(before))
                                    .monospace()
                                    .color(addr_c),
                            );
                            ui.end_row();
                        }
                    });

                    ui.add_space(8.0);
                    ui.label(RichText::new(tr("Que fait cette instruction ?", "What does this instruction do?")).strong().color(hdr));
                    ui.label(&e.description);

                    ui.add_space(6.0);
                    ui.hyperlink_to(
                        format!("📖 {} {} (felixcloutier.com)", tr("Référence Intel de", "Intel reference for"), insn.mnemonic.to_uppercase()),
                        explain::doc_url(&insn.mnemonic),
                    )
                    .on_hover_text(tr("Ouvre la page de l'instruction dans le navigateur\n(mirror du manuel Intel SDM).", "Opens the instruction page in the browser\n(mirror of the Intel SDM manual)."));

                    ui.add_space(8.0);
                    ui.separator();

                    match &dynamics {
                        Some((before, _bstack, Some((after, _astack)))) => {
                            // ΔRSP + écriture/lecture pile.
                            let d = after.rsp as i128 - before.rsp as i128;
                            if d != 0 {
                                ui.label(RichText::new(tr("Pile (RSP)", "Stack (RSP)")).strong().color(hdr));
                                if d < 0 {
                                    ui.colored_label(
                                        PUSH_COL,
                                        format!(
                                            "RSP : 0x{:X} → 0x{:X}  (−{} {}, PUSH)",
                                            before.rsp, after.rsp, -d, tr("octets", "bytes")
                                        ),
                                    );
                                } else {
                                    ui.colored_label(
                                        POP_COL,
                                        format!(
                                            "RSP : 0x{:X} → 0x{:X}  (+{} {}, POP)",
                                            before.rsp, after.rsp, d, tr("octets", "bytes")
                                        ),
                                    );
                                }
                                ui.add_space(6.0);
                            }

                            // Registres modifiés.
                            ui.label(RichText::new(tr("Registres modifiés", "Modified registers")).strong().color(hdr));
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
                                ui.weak(tr("aucun registre modifié.", "no register modified."));
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
                                ui.weak(tr("aucun flag modifié.", "no flag modified."));
                            }

                            ui.add_space(8.0);
                            // Schéma pile avant / après.
                            ui.label(RichText::new(tr("Pile — avant / après", "Stack — before / after")).strong().color(hdr));
                            ui.columns(2, |c| {
                                micro_stack(&mut c[0], addr_c, tr("avant", "before"), before.rsp, _bstack);
                                micro_stack(&mut c[1], addr_c, tr("après", "after"), after.rsp, _astack);
                            });
                        }
                        Some((_before, _bstack, None)) => {
                            ui.weak(tr(
                                "Instruction à exécuter à l'étape courante — avancez d'un pas (Next) \
                                 pour voir ses effets dynamiques.",
                                "Instruction to run at the current step — advance one step (Next) \
                                 to see its dynamic effects.",
                            ));
                            micro_static_flags(ui, hdr, &e, tr("Flags positionnés", "Flags set"), tr("Cette instruction ne modifie aucun flag.", "This instruction modifies no flag."));
                        }
                        None => {
                            ui.weak(tr(
                                "Cette instruction n'a pas encore été exécutée dans l'historique \
                                 (effets dynamiques indisponibles).",
                                "This instruction has not been executed yet in the history \
                                 (dynamic effects unavailable).",
                            ));
                            micro_static_flags(ui, hdr, &e, tr("Flags positionnés", "Flags set"), tr("Cette instruction ne modifie aucun flag.", "This instruction modifies no flag."));
                        }
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.vertical_centered(|ui| {
                        if ui.button(tr("Fermer", "Close")).clicked() {
                            close = true;
                        }
                    });
                });
            });
        if !open || close {
            self.microscope = None;
        }
    }

    pub(super) fn about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str| i18n::tr(lang, fr, en);
        let mnem = self.c_mnemonic();
        let mut open = true;
        egui::Window::new(tr("À propos", "About"))
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading(RichText::new("ASM Studio").color(mnem));
                    ui.label(tr("IDE pédagogique NASM x86-64", "Educational NASM x86-64 IDE"));
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
                        ui.label(tr("Licence", "License"));
                        ui.hyperlink_to(tr("MIT (explication)", "MIT (explanation)"), "https://opensource.org/license/mit")
                            .on_hover_text(tr("Ouvrir le texte officiel de la licence MIT", "Open the official MIT license text"));
                        ui.end_row();
                    });
                ui.separator();
                ui.add_space(6.0);
                ui.vertical_centered(|ui| {
                    if ui.button(tr("Fermer", "Close")).clicked() {
                        self.show_about = false;
                    }
                });
            });
        if !open {
            self.show_about = false;
        }
    }

    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        use egui::ThemePreference;
        // Libellés traduits précalculés (évite d'emprunter self pendant que les
        // widgets empruntent ses champs en écriture).
        let t_title = self.tr("Réglages", "Settings");
        let t_lang = self.tr("Langue", "Language");
        let t_theme = self.tr("Thème", "Theme");
        let t_sys = self.tr("Système (suit l'OS)", "System (follow OS)");
        let t_dark = self.tr("Sombre", "Dark");
        let t_light = self.tr("Clair", "Light");
        let t_theme_note = self.tr(
            "Note : la coloration du code est optimisée pour le thème sombre.",
            "Note: syntax colors are tuned for the dark theme.",
        );
        let t_iface = self.tr("Interface", "Interface");
        let t_tooltips = self.tr(
            "Afficher les infobulles des raccourcis (au survol des boutons)",
            "Show shortcut tooltips (on button hover)",
        );
        let t_anim = self.tr(
            "Animations « CPU vivant » (pulsation des valeurs modifiées)",
            "\"Live CPU\" animations (pulse changed values)",
        );
        let t_asmstd_h = self.tr("Bibliothèque asmstd", "asmstd library");
        let t_asmstd = self.tr(
            "Activer asmstd (call asm.write, asm.exit, asm.mkdir…)",
            "Enable asmstd (call asm.write, asm.exit, asm.mkdir…)",
        );
        let t_asmstd_tip = self.tr(
            "Rend asmstd.inc disponible pour %include depuis n'importe quel fichier.\n\
             Masque les numéros de syscalls derrière des noms lisibles.",
            "Makes asmstd.inc available for %include from any file.\n\
             Hides syscall numbers behind readable names.",
        );
        let t_asmstd_note = self.tr(
            "Dans le code : %include \"asmstd.inc\" puis call asm.write",
            "In code: %include \"asmstd.inc\" then call asm.write",
        );
        let t_close = self.tr("Fermer", "Close");

        let mut open = true;
        let mut changed = false;
        egui::Window::new(t_title)
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(RichText::new(t_lang).strong());
                ui.add_space(4.0);
                changed |= ui.radio_value(&mut self.lang, crate::i18n::Lang::Fr, "Français").changed();
                changed |= ui.radio_value(&mut self.lang, crate::i18n::Lang::En, "English").changed();
                ui.separator();

                ui.label(RichText::new(t_theme).strong());
                ui.add_space(4.0);
                changed |= ui.radio_value(&mut self.theme_pref, ThemePreference::System, t_sys).changed();
                changed |= ui.radio_value(&mut self.theme_pref, ThemePreference::Dark, t_dark).changed();
                changed |= ui.radio_value(&mut self.theme_pref, ThemePreference::Light, t_light).changed();
                ui.add_space(4.0);
                ui.weak(t_theme_note);
                ui.separator();

                ui.label(RichText::new(t_iface).strong());
                ui.add_space(4.0);
                changed |= ui.checkbox(&mut self.show_tooltips, t_tooltips).changed();
                changed |= ui.checkbox(&mut self.animate, t_anim).changed();
                ui.separator();

                ui.label(RichText::new(t_asmstd_h).strong());
                ui.add_space(4.0);
                changed |= ui
                    .checkbox(&mut self.use_asmstd, t_asmstd)
                    .on_hover_text(t_asmstd_tip)
                    .changed();
                ui.weak(t_asmstd_note);
                ui.separator();

                ui.vertical_centered(|ui| {
                    if ui.button(t_close).clicked() {
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

    pub(super) fn shortcuts_window(&mut self, ctx: &egui::Context) {
        if !self.show_shortcuts {
            return;
        }
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str| i18n::tr(lang, fr, en);
        let mnem = self.c_mnemonic();
        let mut open = true;
        egui::Window::new(tr("Raccourcis clavier", "Keyboard shortcuts"))
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                let rows = [
                    ("F1", tr("Aide / raccourcis", "Help / shortcuts")),
                    ("F5", tr("Lancer / Restart", "Run / Restart")),
                    ("F10 / F8", tr("Instruction suivante (Next)", "Next instruction (Next)")),
                    ("Échap / Maj+F5", tr("Stop", "Stop")),
                    ("Ctrl+B", tr("Assembler + Lier", "Assemble + Link")),
                    ("Ctrl+S", tr("Enregistrer", "Save")),
                    ("Ctrl+O", tr("Ouvrir", "Open")),
                    ("Ctrl+N", tr("Nouveau", "New")),
                    ("← / →", tr("Timeline : précédent / suivant", "Timeline: previous / next")),
                    ("Home / End", tr("Timeline : début / fin", "Timeline: start / end")),
                    ("Ctrl+1", tr("Afficher/masquer l'explorateur", "Show/hide the explorer")),
                    ("Ctrl+2", tr("Afficher/masquer l'instruction", "Show/hide the instruction panel")),
                    ("Ctrl+3", tr("Afficher/masquer la bande CPU", "Show/hide the CPU band")),
                    ("Ctrl+4", tr("Afficher/masquer la bande basse", "Show/hide the bottom band")),
                ];
                egui::Grid::new("shortcuts_grid")
                    .num_columns(2)
                    .spacing([24.0, 6.0])
                    .show(ui, |ui| {
                        for (k, d) in rows {
                            ui.label(RichText::new(k).monospace().strong().color(mnem));
                            ui.label(d);
                            ui.end_row();
                        }
                    });
                ui.separator();
                ui.vertical_centered(|ui| {
                    if ui.button(tr("Fermer", "Close")).clicked() {
                        self.show_shortcuts = false;
                    }
                });
            });
        if !open {
            self.show_shortcuts = false;
        }
    }

    // ---------- Calculatrice multi-base ----------

    pub(super) fn calculator_window(&mut self, ctx: &egui::Context) {
        if !self.show_calculator {
            return;
        }
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str| i18n::tr(lang, fr, en);
        let mnem = self.c_mnemonic();
        let hdr = self.c_header();
        let mut open = true;
        egui::Window::new(tr("Calculatrice", "Calculator"))
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                // Sélecteur de base d'entrée.
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr("Base d'entrée :", "Input base:")).color(hdr));
                    ui.radio_value(&mut self.calc_base, 10, "Dec");
                    ui.radio_value(&mut self.calc_base, 16, "Hex");
                    ui.radio_value(&mut self.calc_base, 2, "Bin");
                    ui.radio_value(&mut self.calc_base, 8, "Oct");
                });
                ui.add_space(4.0);

                // Champ de saisie.
                let hint = match self.calc_base {
                    16 => "deadbeef",
                    2  => "10110100",
                    8  => "377",
                    _  => "42",
                };
                ui.add(
                    egui::TextEdit::singleline(&mut self.calc_input)
                        .desired_width(ui.available_width())
                        .font(egui::TextStyle::Monospace)
                        .hint_text(hint),
                );
                // Filtre les caractères invalides pour la base courante.
                // En base 10, le signe '-' est autorisé en première position.
                let base = self.calc_base;
                if base == 10 {
                    let neg = self.calc_input.starts_with('-');
                    self.calc_input.retain(|c| c.is_ascii_digit());
                    if neg {
                        self.calc_input.insert(0, '-');
                    }
                } else {
                    self.calc_input.retain(|c| c.is_digit(base));
                }

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                let parsed = calc_parse(&self.calc_input, self.calc_base);

                // Grille de résultats.
                egui::Grid::new("calc_grid")
                    .num_columns(2)
                    .spacing([16.0, 6.0])
                    .show(ui, |ui| {
                        for (label, base) in [("Dec", 10), ("Hex", 16), ("Oct", 8), ("Bin", 2)] {
                            ui.label(RichText::new(label).strong().color(hdr));
                            let txt = match parsed {
                                Some(v) => calc_format(v, base),
                                None => "—".to_string(),
                            };
                            ui.label(RichText::new(txt).monospace().color(mnem));
                            ui.end_row();
                        }
                    });

                ui.add_space(6.0);
                ui.separator();
                ui.vertical_centered(|ui| {
                    if ui.button(tr("Fermer", "Close")).clicked() {
                        self.show_calculator = false;
                    }
                });
            });
        if !open {
            self.show_calculator = false;
        }
    }
}
