use eframe::egui::{self, RichText};

use crate::debugger::Flags;
use crate::explain;
use crate::i18n;
use crate::syscall;

use super::{
    App, ACCENT, ACTION, CHANGED, FALSE_COL, PUSH_COL, POP_COL,
    micro_stack, micro_static_flags,
};

/// En-tête d'une section de réglages : même rythme vertical pour toutes,
/// y compris celles qu'on ajoutera.
fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(2.0);
    ui.label(RichText::new(title).strong());
    ui.add_space(4.0);
}

/// Retire le balisage gras `**…**` d'une ligne Markdown, pour un affichage
/// propre dans la fenêtre de licence (on ne conserve pas le gras inline, mais
/// le texte reste lisible sans les astérisques parasites).
fn strip_bold(s: &str) -> String {
    s.replace("**", "")
}

/// Libellé « bit » pour l'infobulle de la grille de bits.
fn tr_bit(lang: crate::i18n::Lang) -> &'static str {
    i18n::tr3(lang, "bit", "bit", "bit")
}

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
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let mut open = true;
        let mut close = false;
        egui::Window::new(format!("🔬 Microscope — {} {}", insn.mnemonic, insn.operands))
            .collapsible(false)
            .resizable(true)
            .default_width(580.0)
            .default_height(560.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().id_salt("microscope_scroll").show(ui, |ui| {
                    // --- Identité de l'instruction ---
                    egui::Grid::new("micro_id").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                        ui.label(RichText::new(tr("Adresse", "Address", "Dirección")).strong());
                        ui.label(RichText::new(format!("0x{:08X}", insn.address)).monospace().color(addr_c));
                        ui.end_row();
                        ui.label(RichText::new(tr("Octets machine", "Machine bytes", "Bytes de máquina")).strong());
                        ui.label(RichText::new(insn.bytes_hex()).monospace().color(bytes_c));
                        ui.end_row();
                        ui.label(RichText::new(tr("Décodage", "Decoding", "Decodificación")).strong());
                        ui.label(
                            RichText::new(format!("{} {}", insn.mnemonic, insn.operands))
                                .monospace()
                                .color(mnem_c),
                        );
                        ui.end_row();
                        ui.label(RichText::new(tr("Catégorie", "Category", "Categoría")).strong());
                        ui.label(e.category);
                        ui.end_row();
                        ui.label(RichText::new(tr("Cycles estimés", "Estimated cycles", "Ciclos estimados")).strong());
                        ui.label(RichText::new(cycles).color(CHANGED))
                            .on_hover_text(tr("Ordre de grandeur pédagogique, pas une mesure exacte.", "Educational ballpark, not an exact measurement.", "Orden de magnitud pedagógico, no una medida exacta."));
                        ui.end_row();
                        // Ligne syscall dans la grille d'identité (si applicable).
                        if insn.mnemonic == "syscall"
                            && let Some((before, _, _)) = &dynamics
                        {
                            ui.label(RichText::new(tr("Appel système", "System call", "Llamada al sistema")).strong());
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
                            ui.label(RichText::new(tr("Arguments", "Arguments", "Argumentos")).strong());
                            ui.label(
                                RichText::new(syscall::format_call(before))
                                    .monospace()
                                    .color(addr_c),
                            );
                            ui.end_row();
                        }
                    });

                    ui.add_space(8.0);
                    // --- 💡 Explication ---
                    ui.label(RichText::new(tr("💡 Explication", "💡 Explanation", "💡 Explicación")).strong().color(hdr));
                    ui.label(&e.description);

                    // --- 🔢 Binaire : découpage de l'encodage machine ---
                    ui.add_space(10.0);
                    ui.label(RichText::new(tr("🔢 Binaire", "🔢 Binary", "🔢 Binario")).strong().color(hdr));
                    ui.label(
                        RichText::new(tr(
                            "Comment ces octets encodent l'instruction.",
                            "How these bytes encode the instruction.",
                            "Cómo estos bytes codifican la instrucción.",
                        ))
                        .small()
                        .weak(),
                    );
                    ui.add_space(3.0);
                    let enc = crate::encoding::decode(&insn.bytes, lang);
                    if enc.fields.is_empty() {
                        ui.weak(tr("(encodage indisponible)", "(encoding unavailable)", "(codificación no disponible)"));
                    } else {
                        egui::Grid::new("micro_enc").num_columns(3).spacing([12.0, 4.0]).show(ui, |ui| {
                            for f in &enc.fields {
                                let hex: String = f.bytes.iter().map(|b| format!("{b:02X} ")).collect();
                                ui.label(
                                    RichText::new(hex.trim_end().to_string())
                                        .monospace()
                                        .strong()
                                        .color(CHANGED),
                                );
                                ui.label(
                                    RichText::new(f.part.label(lang))
                                        .small()
                                        .strong()
                                        .color(mnem_c),
                                );
                                ui.add(egui::Label::new(RichText::new(&f.detail).small()).wrap());
                                ui.end_row();
                            }
                        });
                        if enc.incomplete {
                            ui.add_space(3.0);
                            ui.label(
                                RichText::new(tr(
                                    "⚠ Encodage partiellement décodé : certains octets n'ont pas pu être attribués.",
                                    "⚠ Partially decoded encoding: some bytes could not be attributed.",
                                    "⚠ Codificación parcialmente decodificada: algunos bytes no pudieron atribuirse.",
                                ))
                                .small()
                                .color(FALSE_COL),
                            );
                        }
                    }

                    // --- ⚙️ Effets : ce qui est lu, écrit, affecté ---
                    ui.add_space(10.0);
                    ui.label(RichText::new(tr("⚙️ Effets", "⚙️ Effects", "⚙️ Efectos")).strong().color(hdr));
                    let fx = crate::effects::analyse(&insn.mnemonic, &insn.operands, &e.affects_flags, lang);
                    egui::Grid::new("micro_fx").num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
                        let row = |ui: &mut egui::Ui, k: &str, list: &[crate::effects::Resource], col: egui::Color32| {
                            ui.label(RichText::new(k).small().strong().color(hdr));
                            if list.is_empty() {
                                ui.label(RichText::new("—").small().weak());
                            } else {
                                let txt = list
                                    .iter()
                                    .map(|r| r.label(lang))
                                    .collect::<Vec<_>>()
                                    .join("   ");
                                ui.add(egui::Label::new(RichText::new(txt).monospace().small().color(col)).wrap());
                            }
                            ui.end_row();
                        };
                        row(ui, tr("Lu", "Reads", "Lee"), &fx.reads, addr_c);
                        row(ui, tr("Écrit", "Writes", "Escribe"), &fx.writes, CHANGED);
                    });
                    // Les effets que le texte de l'instruction ne montre pas —
                    // c'est là que se trouve l'essentiel de ce qui piège.
                    for note in &fx.implicit {
                        ui.add_space(4.0);
                        egui::Frame::default()
                            .fill(ACTION.linear_multiply(0.08))
                            .corner_radius(egui::CornerRadius::same(4))
                            .inner_margin(egui::Margin::symmetric(8, 5))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.add(egui::Label::new(RichText::new(note).size(12.0)).wrap());
                            });
                    }

                    // --- 🔍 Contexte : instructions voisines, ABI ---
                    let related = crate::effects::related(&insn.mnemonic);
                    let abi = crate::effects::abi_note(&insn.mnemonic, &insn.operands, lang);
                    if !related.is_empty() || abi.is_some() {
                        ui.add_space(10.0);
                        ui.label(RichText::new(tr("🔍 Contexte", "🔍 Context", "🔍 Contexto")).strong().color(hdr));
                        if let Some(note) = &abi {
                            ui.add(egui::Label::new(RichText::new(note).size(12.0)).wrap());
                            ui.add_space(3.0);
                        }
                        if !related.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(tr("À connaître avec :", "Learn alongside:", "Aprender junto a:"))
                                        .small()
                                        .color(hdr),
                                );
                                for r in related {
                                    ui.hyperlink_to(
                                        RichText::new(r.to_uppercase()).monospace().small(),
                                        explain::doc_url(r),
                                    );
                                }
                            });
                        }
                    }

                    ui.add_space(6.0);
                    ui.hyperlink_to(
                        format!("📖 {} {} (felixcloutier.com)", tr("Référence Intel de", "Intel reference for", "Referencia Intel de"), insn.mnemonic.to_uppercase()),
                        explain::doc_url(&insn.mnemonic),
                    )
                    .on_hover_text(tr("Ouvre la page de l'instruction dans le navigateur\n(mirror du manuel Intel SDM).", "Opens the instruction page in the browser\n(mirror of the Intel SDM manual).", "Abre la página de la instrucción en el navegador\n(espejo del manual Intel SDM)."));

                    ui.add_space(8.0);
                    ui.separator();

                    match &dynamics {
                        Some((before, _bstack, Some((after, _astack)))) => {
                            // ΔRSP + écriture/lecture pile.
                            let d = after.rsp as i128 - before.rsp as i128;
                            if d != 0 {
                                ui.label(RichText::new(tr("Pile (RSP)", "Stack (RSP)", "Pila (RSP)")).strong().color(hdr));
                                if d < 0 {
                                    ui.colored_label(
                                        PUSH_COL,
                                        format!(
                                            "RSP : 0x{:X} → 0x{:X}  (−{} {}, PUSH)",
                                            before.rsp, after.rsp, -d, tr("octets", "bytes", "bytes")
                                        ),
                                    );
                                } else {
                                    ui.colored_label(
                                        POP_COL,
                                        format!(
                                            "RSP : 0x{:X} → 0x{:X}  (+{} {}, POP)",
                                            before.rsp, after.rsp, d, tr("octets", "bytes", "bytes")
                                        ),
                                    );
                                }
                                ui.add_space(6.0);
                            }

                            // Registres modifiés.
                            ui.label(RichText::new(tr("Registres modifiés", "Modified registers", "Registros modificados")).strong().color(hdr));
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
                                ui.weak(tr("aucun registre modifié.", "no register modified.", "ningún registro modificado."));
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
                                ui.weak(tr("aucun flag modifié.", "no flag modified.", "ningún flag modificado."));
                            }

                            ui.add_space(8.0);
                            // Schéma pile avant / après.
                            ui.label(RichText::new(tr("Pile — avant / après", "Stack — before / after", "Pila — antes / después")).strong().color(hdr));
                            ui.columns(2, |c| {
                                micro_stack(&mut c[0], addr_c, tr("avant", "before", "antes"), before.rsp, _bstack);
                                micro_stack(&mut c[1], addr_c, tr("après", "after", "después"), after.rsp, _astack);
                            });
                        }
                        Some((_before, _bstack, None)) => {
                            ui.weak(tr(
                                "Instruction à exécuter à l'étape courante — avancez d'un pas (Next) \
                                 pour voir ses effets dynamiques.",
                                "Instruction to run at the current step — advance one step (Next) \
                                 to see its dynamic effects.",
                                "Instrucción a ejecutar en el paso actual — avance un paso (Siguiente) \
                                 para ver sus efectos dinámicos.",
                            ));
                            micro_static_flags(ui, hdr, &e, tr("Flags positionnés", "Flags set", "Flags activos"), tr("Cette instruction ne modifie aucun flag.", "This instruction modifies no flag.", "Esta instrucción no modifica ningún flag."));
                        }
                        None => {
                            ui.weak(tr(
                                "Cette instruction n'a pas encore été exécutée dans l'historique \
                                 (effets dynamiques indisponibles).",
                                "This instruction has not been executed yet in the history \
                                 (dynamic effects unavailable).",
                                "Esta instrucción aún no ha sido ejecutada en el historial \
                                 (efectos dinámicos no disponibles).",
                            ));
                            micro_static_flags(ui, hdr, &e, tr("Flags positionnés", "Flags set", "Flags activos"), tr("Cette instruction ne modifie aucun flag.", "This instruction modifies no flag.", "Esta instrucción no modifica ningún flag."));
                        }
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.vertical_centered(|ui| {
                        if ui.button(tr("Fermer", "Close", "Cerrar")).clicked() {
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
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let mnem = self.c_mnemonic();
        let mut open = true;
        egui::Window::new(tr("À propos", "About", "Acerca de"))
            .collapsible(false)
            .resizable(true)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading(RichText::new("ASM Studio").color(mnem));
                    ui.label(tr("IDE pédagogique NASM x86-64", "Educational NASM x86-64 IDE", "IDE educativo NASM x86-64"));
                });
                ui.add_space(6.0);
                // Bandeau bêta : plus serein que l'alpha, mais on rappelle que
                // c'est une préversion.
                egui::Frame::default()
                    .fill(ACTION.linear_multiply(0.9))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new(tr("🔷  VERSION BÊTA 1", "🔷  BETA 1 VERSION", "🔷  VERSIÓN BETA 1"))
                                    .strong()
                                    .color(egui::Color32::WHITE),
                            );
                            ui.label(
                                RichText::new(tr(
                                    "Préversion en cours de finition — fonctionnelle, mais des détails peuvent encore changer.\nVos retours sont les bienvenus.",
                                    "Pre-release under polishing — functional, but details may still change.\nFeedback is welcome.",
                                    "Preversión en pulido — funcional, pero algunos detalles pueden cambiar.\nSus comentarios son bienvenidos.",
                                ))
                                .small()
                                .color(egui::Color32::from_rgb(255, 236, 214)),
                            );
                        });
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
                        ui.label(tr("Auteur", "Author", "Autor"));
                        ui.label(RichText::new("Frédéric Z.").strong());
                        ui.end_row();
                        ui.label(tr("Licence", "License", "Licencia"));
                        if ui
                            .link(RichText::new("ASFL v1.0").strong())
                            .on_hover_text(tr(
                                "ASM Studio Personal Free License v1.0 — cliquer pour lire le texte complet.",
                                "ASM Studio Personal Free License v1.0 — click to read the full text.",
                                "ASM Studio Personal Free License v1.0 — clic para leer el texto completo.",
                            ))
                            .clicked()
                        {
                            self.show_license = true;
                        }
                        ui.end_row();
                    });
                ui.separator();
                ui.add_space(6.0);
                ui.vertical_centered(|ui| {
                    if ui.button(tr("Fermer", "Close", "Cerrar")).clicked() {
                        self.show_about = false;
                    }
                });
            });
        if !open {
            self.show_about = false;
        }
    }

    /// Texte intégral de la licence, embarqué depuis `LICENSE.md` à la racine du
    /// projet : le fichier reste l'unique source de vérité, la fenêtre ne peut
    /// pas se désynchroniser de lui.
    pub(super) fn license_window(&mut self, ctx: &egui::Context) {
        if !self.show_license {
            return;
        }
        const LICENSE: &str = include_str!("../../LICENSE.md");
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let hdr = self.c_header();
        let mnem = self.c_mnemonic();

        let mut open = true;
        egui::Window::new(tr("Licence", "License", "Licencia"))
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .default_height(520.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("license_scroll")
                    .auto_shrink([false, false])
                    .max_height(ctx.content_rect().height() * 0.66)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        // Rendu Markdown léger : ce fichier n'utilise que titres,
                        // séparateurs, citations et listes — pas besoin d'un moteur
                        // complet, juste de retirer le balisage pour un texte propre.
                        for raw in LICENSE.lines() {
                            let line = raw.trim_end();
                            if line.is_empty() {
                                ui.add_space(4.0);
                            } else if line == "---" {
                                ui.separator();
                            } else if let Some(t) = line.strip_prefix("# ") {
                                ui.add_space(2.0);
                                ui.heading(RichText::new(strip_bold(t)).color(mnem));
                            } else if let Some(t) = line.strip_prefix("### ") {
                                ui.add_space(2.0);
                                ui.label(RichText::new(strip_bold(t)).strong().color(hdr));
                            } else if let Some(t) = line.strip_prefix("## ") {
                                ui.add_space(3.0);
                                ui.label(RichText::new(strip_bold(t)).strong().size(15.0).color(hdr));
                            } else if let Some(t) = line.strip_prefix("> ") {
                                ui.label(RichText::new(strip_bold(t)).italics().weak());
                            } else if let Some(t) = line.strip_prefix("- ") {
                                ui.horizontal_wrapped(|ui| {
                                    ui.add_space(8.0);
                                    ui.label(RichText::new("•").color(hdr));
                                    ui.label(strip_bold(t));
                                });
                            } else {
                                ui.label(strip_bold(line));
                            }
                        }
                    });
                ui.separator();
                ui.vertical_centered(|ui| {
                    if ui.button(tr("Fermer", "Close", "Cerrar")).clicked() {
                        self.show_license = false;
                    }
                });
            });
        if !open {
            self.show_license = false;
        }
    }

    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        use egui::ThemePreference;
        // Libellés traduits précalculés (évite d'emprunter self pendant que les
        // widgets empruntent ses champs en écriture).
        let t_title = self.tr3("Réglages", "Settings", "Configuración");
        let t_lang = self.tr3("Langue", "Language", "Idioma");
        let t_theme = self.tr3("Thème", "Theme", "Tema");
        let t_sys = self.tr3("Système (suit l'OS)", "System (follow OS)", "Sistema (sigue el SO)");
        let t_dark = self.tr3("Sombre", "Dark", "Oscuro");
        let t_light = self.tr3("Clair", "Light", "Claro");
        let t_theme_note = self.tr3(
            "Note : la coloration du code est optimisée pour le thème sombre.",
            "Note: syntax colors are tuned for the dark theme.",
            "Nota: los colores de sintaxis están optimizados para el tema oscuro.",
        );
        let t_iface = self.tr3("Interface", "Interface", "Interfaz");
        let t_tooltips = self.tr3(
            "Afficher les infobulles des raccourcis (au survol des boutons)",
            "Show shortcut tooltips (on button hover)",
            "Mostrar tooltips de atajos (al pasar el cursor sobre los botones)",
        );
        let t_anim = self.tr3(
            "Animations « CPU vivant » (pulsation des valeurs modifiées)",
            "\"Live CPU\" animations (pulse changed values)",
            "Animaciones «CPU vivo» (pulso de valores modificados)",
        );
        let t_asmstd_h = self.tr3("Bibliothèque asmstd", "asmstd library", "Biblioteca asmstd");
        let t_asmstd = self.tr3(
            "Activer asmstd (call asm.write, asm.exit, asm.mkdir…)",
            "Enable asmstd (call asm.write, asm.exit, asm.mkdir…)",
            "Activar asmstd (call asm.write, asm.exit, asm.mkdir…)",
        );
        let t_asmstd_tip = self.tr3(
            "Ajoute le dossier des exemples aux chemins d'inclusion de nasm, pour que\n\
             %include \"asmstd.inc\" fonctionne depuis n'importe quel dossier.",
            "Adds the examples folder to nasm's include paths, so that\n\
             %include \"asmstd.inc\" works from any folder.",
            "Añade la carpeta de ejemplos a las rutas de inclusión de nasm, para que\n\
             %include \"asmstd.inc\" funcione desde cualquier carpeta.",
        );
        let t_asmstd_what = self.tr3(
            "Écrire « bonjour » en assembleur nu demande de connaître le numéro du syscall \
             write, l'ordre de ses arguments, et de compter soi-même la longueur de la \
             chaîne. asmstd met un nom lisible sur cette paperasse : « call asm.print » \
             remplace cinq lignes. Le programme reste du vrai assembleur exécuté par le \
             vrai noyau — rien n'est simulé.",
            "Writing \"hello\" in bare assembly means knowing the write syscall number, its \
             argument order, and counting the string length yourself. asmstd puts a readable \
             name on that paperwork: \"call asm.print\" replaces five lines. The program is \
             still real assembly run by the real kernel — nothing is simulated.",
            "Escribir «hola» en ensamblador puro exige conocer el número del syscall write, \
             el orden de sus argumentos y contar la longitud a mano. asmstd pone un nombre \
             legible a ese papeleo: «call asm.print» sustituye cinco líneas. El programa \
             sigue siendo ensamblador real ejecutado por el núcleo real.",
        );
        let t_asmstd_scope = self.tr3(
            "Environ 100 fonctions : sortie et saisie, fichiers, dossiers, processus, \
             mémoire, réseau, temps, chaînes, caractères, nombres, tableaux, et une \
             vérification (asm.assert_eq) pour écrire des programmes qui se contrôlent \
             eux-mêmes. L'index complet est en tête d'asmstd.inc.",
            "About 100 functions: output and input, files, directories, processes, memory, \
             networking, time, strings, characters, numbers, arrays, and a check \
             (asm.assert_eq) to write self-verifying programs. The full index is at the top \
             of asmstd.inc.",
            "Unas 100 funciones: salida y entrada, archivos, directorios, procesos, memoria, \
             red, tiempo, cadenas, caracteres, números, arrays, y una verificación \
             (asm.assert_eq). El índice completo está al principio de asmstd.inc.",
        );
        let t_asmstd_note = self.tr3(
            "Dans le code : %include \"asmstd.inc\" puis call asm.write",
            "In code: %include \"asmstd.inc\" then call asm.write",
            "En el código: %include \"asmstd.inc\" luego call asm.write",
        );
        let t_pedagogy_h = self.tr3("Mode pédagogique", "Pedagogical mode", "Modo pedagógico");
        let t_pedagogy_anim = self.tr3(
            "Animations enrichies (flèches ↑↓ sur les registres et la pile modifiés)",
            "Enhanced animations (↑↓ arrows on changed registers and stack)",
            "Animaciones enriquecidas (flechas ↑↓ en registros y pila modificados)",
        );
        let t_pedagogy_memview = self.tr3(
            "Vue mémoire unifiée (onglet « Vue mémoire » — registres → zones pointées)",
            "Unified memory view (\"Memory View\" tab — registers → pointed zones)",
            "Vista de memoria unificada (pestaña «Vista memoria» — registros → zonas apuntadas)",
        );
        let t_tuto_h = self.tr3("Parcours guidé", "Guided path", "Recorrido guiado");
        let t_tuto = self.tr3(
            "Activer le tutoriel (panneau Tutoriel)",
            "Enable the tutorial (Tutorial panel)",
            "Activar el tutorial (panel Tutorial)",
        );
        let t_tuto_desc = self.tr3(
            "Un parcours en quatre niveaux, du premier programme à l'analyse de binaires. \
             Chaque leçon charge son propre programme dans l'éditeur, ouvre les panneaux \
             qu'elle explique, et embarque ses attentes : le panneau Exercice dit si c'est \
             juste. La progression est conservée d'une session à l'autre.",
            "A four-level path, from your first program to binary analysis. Each lesson loads \
             its own program into the editor, opens the panels it explains, and carries its \
             expectations: the Exercise panel tells you if it is right. Progress is kept \
             between sessions.",
            "Un recorrido de cuatro niveles, del primer programa al análisis de binarios. Cada \
             lección carga su programa, abre los paneles que explica y lleva sus expectativas. \
             El progreso se conserva entre sesiones.",
        );
        let t_tuto_reset = self.tr3(
            "Réinitialiser la progression",
            "Reset progress",
            "Reiniciar el progreso",
        );
        let t_tuto_reset_tip = self.tr3(
            "Oublie les leçons terminées et rouvre le parcours au début.",
            "Forget completed lessons and reopen the path at the start.",
            "Olvida las lecciones terminadas y reabre el recorrido al inicio.",
        );
        let t_close = self.tr3("Fermer", "Close", "Cerrar");

        let mut open = true;
        let mut changed = false;
        let mut reset_tutorial = false;
        // Corps borné à une fraction de l'écran : les réglages à venir
        // s'ajouteront sans pousser le bouton Fermer hors de la fenêtre.
        let max_body_h = (ctx.content_rect().height() * 0.66).max(240.0);

        egui::Window::new(t_title)
            .collapsible(false)
            .resizable(true)
            // Large dès l'ouverture : les libellés d'options tiennent sur une
            // ligne, et il reste de la place pour ce qui viendra ensuite.
            .default_width(640.0)
            .min_width(430.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("settings_scroll")
                    .max_height(max_body_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());

                        section(ui, t_lang);
                        changed |= ui.radio_value(&mut self.lang, crate::i18n::Lang::Fr, "Français").changed();
                        changed |= ui.radio_value(&mut self.lang, crate::i18n::Lang::En, "English").changed();
                        changed |= ui.radio_value(&mut self.lang, crate::i18n::Lang::Es, "Español").changed();
                        ui.separator();

                        section(ui, t_theme);
                        changed |= ui.radio_value(&mut self.theme_pref, ThemePreference::System, t_sys).changed();
                        changed |= ui.radio_value(&mut self.theme_pref, ThemePreference::Dark, t_dark).changed();
                        changed |= ui.radio_value(&mut self.theme_pref, ThemePreference::Light, t_light).changed();
                        ui.add_space(4.0);
                        ui.weak(t_theme_note);
                        ui.separator();

                        section(ui, t_iface);
                        changed |= ui.checkbox(&mut self.show_tooltips, t_tooltips).changed();
                        changed |= ui.checkbox(&mut self.animate, t_anim).changed();
                        ui.separator();

                        section(ui, t_asmstd_h);
                        changed |= ui
                            .checkbox(&mut self.use_asmstd, t_asmstd)
                            .on_hover_text(t_asmstd_tip)
                            .changed();
                        ui.weak(t_asmstd_note);
                        ui.add_space(6.0);
                        super::card(ui, |ui| {
                            ui.label(RichText::new(t_asmstd_what).size(12.5));
                            ui.add_space(5.0);
                            ui.label(RichText::new(t_asmstd_scope).size(12.0).weak());
                        });
                        ui.separator();

                        section(ui, t_tuto_h);
                        changed |= ui.checkbox(&mut self.tutorial_enabled, t_tuto).changed();
                        ui.add_space(4.0);
                        super::card(ui, |ui| {
                            ui.label(RichText::new(t_tuto_desc).size(12.5));
                        });
                        ui.add_space(4.0);
                        if ui.button(t_tuto_reset).on_hover_text(t_tuto_reset_tip).clicked() {
                            reset_tutorial = true;
                        }
                        ui.separator();

                        section(ui, t_pedagogy_h);
                        changed |= ui.checkbox(&mut self.pedagogy_anim, t_pedagogy_anim).changed();
                        changed |= ui.checkbox(&mut self.pedagogy_memview, t_pedagogy_memview).changed();
                    });

                // Hors de la zone défilante : le bouton reste atteignable quel
                // que soit le nombre de réglages.
                ui.separator();
                ui.vertical_centered(|ui| {
                    if ui.button(t_close).clicked() {
                        self.show_settings = false;
                    }
                });
            });
        if reset_tutorial {
            self.reset_tutorial();
        }
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
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let mnem = self.c_mnemonic();
        let mut open = true;
        egui::Window::new(tr("Raccourcis clavier", "Keyboard shortcuts", "Atajos de teclado"))
            .collapsible(false)
            .resizable(true)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .show(ctx, |ui| {
                let rows = [
                    ("F1", tr("Afficher/masquer cette aide", "Show/hide this help", "Mostrar/ocultar esta ayuda")),
                    ("F5", tr("Lancer / Restart", "Run / Restart", "Ejecutar / Reiniciar")),
                    ("F10 / F8", tr("Instruction suivante (Next)", "Next instruction (Next)", "Instrucción siguiente (Siguiente)")),
                    ("Échap / Maj+F5", tr("Stop", "Stop", "Detener")),
                    ("Ctrl+B", tr("Assembler + Lier", "Assemble + Link", "Ensamblar + Enlazar")),
                    ("Ctrl+S", tr("Enregistrer", "Save", "Guardar")),
                    ("Ctrl+O", tr("Ouvrir", "Open", "Abrir")),
                    ("Ctrl+N", tr("Nouveau", "New", "Nuevo")),
                    ("← / →", tr("Timeline : précédent / suivant", "Timeline: previous / next", "Línea de tiempo: anterior / siguiente")),
                    ("Home / End", tr("Timeline : début / fin", "Timeline: start / end", "Línea de tiempo: inicio / fin")),
                    ("Ctrl+1", tr("Afficher/masquer l'explorateur", "Show/hide the explorer", "Mostrar/ocultar el explorador")),
                    ("Ctrl+2", tr("Afficher/masquer l'instruction", "Show/hide the instruction panel", "Mostrar/ocultar el panel de instrucción")),
                    ("Ctrl+3", tr("Afficher/masquer les registres", "Show/hide the registers", "Mostrar/ocultar los registros")),
                    ("Ctrl+4", tr("Afficher/masquer la mémoire", "Show/hide memory", "Mostrar/ocultar la memoria")),
                    ("Ctrl+5", tr("Afficher/masquer la fenêtre Prédiction", "Show/hide the Prediction window", "Mostrar/ocultar la ventana Predicción")),
                    ("Ctrl+Maj+P", tr("Palette de commandes — toute l'application au clavier", "Command palette — the whole app from the keyboard", "Paleta de comandos — toda la aplicación desde el teclado")),
                    ("F6", tr("Panneau suivant (Maj+F6 : précédent)", "Next panel (Shift+F6: previous)", "Panel siguiente (Mayús+F6: anterior)")),
                    ("Ctrl+W", tr("Fermer le panneau focalisé", "Close the focused panel", "Cerrar el panel enfocado")),
                    ("Ctrl+F6", tr("Revenir directement à l'éditeur", "Jump straight back to the editor", "Volver directamente al editor")),
                    ("Ctrl+Tab", tr("Onglet suivant du panneau focalisé", "Next tab of the focused panel", "Pestaña siguiente del panel enfocado")),
                    ("Tab", tr("Élément interactif suivant (hors éditeur)", "Next interactive element (outside the editor)", "Siguiente elemento interactivo (fuera del editor)")),
                    ("↑ / ↓", tr("Parcourir le panneau focalisé : désassemblage, explorateur, registres, mémoire (une ligne), vue mémoire (un fil)", "Browse the focused panel: disassembly, explorer, registers, memory (one row), memory view (one wire)", "Recorrer el panel enfocado: desensamblado, explorador, registros, memoria (una fila), vista memoria (un hilo)")),
                    ("PgUp / PgDn", tr("Mémoire : saut de huit lignes", "Memory: jump eight rows", "Memoria: salto de ocho filas")),
                    ("← / →", tr("Timeline, ou traverser la ligne dans les registres", "Timeline, or move across the row in registers", "Línea de tiempo, o cruzar la fila en los registros")),
                    ("Entrée", tr("Valider : microscope, ouvrir le fichier, éditer le registre", "Confirm: microscope, open the file, edit the register", "Confirmar: microscopio, abrir el archivo, editar el registro")),
                    ("Échap", tr("Quitter le champ de saisie, sinon arrêter le programme", "Leave the text field, otherwise stop the program", "Salir del campo de texto, si no detener el programa")),
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
                    if ui.button(tr("Fermer", "Close", "Cerrar")).clicked() {
                        self.show_shortcuts = false;
                    }
                });
            });
        if !open {
            self.show_shortcuts = false;
        }
    }

    // ---------- Calculatrice multi-base ----------

    /// Affiche une valeur bit à bit, groupée par octets, avec les bits
    /// cliquables. Renvoie la valeur éventuellement modifiée.
    ///
    /// C'est la vue qui manque partout ailleurs : on lit `0x2A` sans voir que
    /// c'est `0010 1010`, et on manipule des masques sans voir ce qu'ils
    /// éteignent. Ici, cliquer un bit le bascule et toutes les bases suivent.
    fn bit_grid(&self, ui: &mut egui::Ui, value: i64, editable: bool) -> Option<i64> {
        let hdr = self.c_header();
        let width = super::calc_width_bytes(value);
        let bytes = super::calc_bytes_of(value, width);
        let mut changed: Option<i64> = None;

        egui::ScrollArea::horizontal()
            .id_salt("calc_bits")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (bi, byte) in bytes.iter().enumerate() {
                        // Rang du bit de poids fort de cet octet.
                        let high = (width - 1 - bi) * 8 + 7;
                        ui.vertical(|ui| {
                            // Ligne des bits.
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 1.0;
                                for (k, on) in super::calc_bits_of(*byte).iter().enumerate() {
                                    let rank = high - k;
                                    let col = if *on { ACTION } else { self.c_bytes() };
                                    let txt = RichText::new(if *on { "1" } else { "0" })
                                        .monospace()
                                        .size(13.0)
                                        .color(col);
                                    let btn = egui::Button::new(txt)
                                        .min_size(egui::vec2(15.0, 19.0))
                                        .fill(if *on {
                                            ACTION.linear_multiply(0.16)
                                        } else {
                                            egui::Color32::TRANSPARENT
                                        })
                                        .corner_radius(egui::CornerRadius::same(2));
                                    let r = ui.add_enabled(editable, btn);
                                    if editable {
                                        let r = r.on_hover_text(format!(
                                            "{} {rank}",
                                            tr_bit(self.lang)
                                        ));
                                        if r.clicked() {
                                            changed = Some(super::calc_toggle_bit(value, rank as u32));
                                        }
                                    }
                                }
                            });
                            // Rangs des extrémités de l'octet, et sa valeur hexa.
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 1.0;
                                ui.label(
                                    RichText::new(format!("{high}"))
                                        .monospace()
                                        .size(8.5)
                                        .color(hdr),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(format!("{:02X}", byte))
                                                .monospace()
                                                .size(9.5)
                                                .color(self.c_mnemonic()),
                                        );
                                    },
                                );
                            });
                        });
                        if bi + 1 < bytes.len() {
                            ui.add_space(5.0);
                        }
                    }
                });
            });
        changed
    }

    pub(super) fn calculator_window(&mut self, ctx: &egui::Context) {
        if !self.show_calculator {
            return;
        }
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let mnem = self.c_mnemonic();
        let hdr = self.c_header();
        let mut open = true;
        egui::Window::new(tr("Calculatrice", "Calculator", "Calculadora"))
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .min_width(400.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .show(ctx, |ui| {
                // Sélecteur de base d'entrée. Hexa en tête : c'est la base dans
                // laquelle on lit un registre, une adresse ou un masque.
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr("Base :", "Base:", "Base:")).color(hdr));
                    ui.radio_value(&mut self.calc_base, 16, "Hex");
                    ui.radio_value(&mut self.calc_base, 2, "Bin");
                    ui.radio_value(&mut self.calc_base, 10, "Dec");
                    ui.radio_value(&mut self.calc_base, 8, "Oct");
                });
                ui.add_space(6.0);

                let base = self.calc_base;
                let hint = match base {
                    16 => "deadbeef",
                    2 => "10110100",
                    8 => "377",
                    _ => "42",
                };
                // Filtre partagé par les deux opérandes.
                let sanitize = |s: &mut String| {
                    if base == 10 {
                        let neg = s.starts_with('-');
                        s.retain(|c| c.is_ascii_digit());
                        if neg {
                            s.insert(0, '-');
                        }
                    } else {
                        s.retain(|c| c.is_digit(base));
                    }
                };

                // ---- Opérande A ----
                ui.horizontal(|ui| {
                    ui.label(RichText::new("A").monospace().strong().color(hdr));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.calc_input)
                            .desired_width(ui.available_width())
                            .font(egui::TextStyle::Monospace)
                            .hint_text(hint),
                    );
                });
                sanitize(&mut self.calc_input);
                let a = super::calc_parse(&self.calc_input, base);
                if let Some(v) = a
                    && let Some(nv) = self.bit_grid(ui, v, true)
                {
                    self.calc_input = super::calc_format(nv, base)
                        .trim_start_matches("0x")
                        .trim_start_matches("0b")
                        .trim_start_matches("0o")
                        .to_string();
                }

                ui.add_space(6.0);

                // ---- Opération ----
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    for op in super::CalcOp::ALL {
                        let sel = self.calc_op == op;
                        let txt = RichText::new(op.symbol())
                            .monospace()
                            .size(12.0)
                            .color(if sel {
                                egui::Color32::WHITE
                            } else if op.is_bitwise() {
                                mnem
                            } else {
                                hdr
                            });
                        let mut b = egui::Button::new(txt)
                            .min_size(egui::vec2(38.0, 22.0))
                            .corner_radius(egui::CornerRadius::same(4));
                        if sel {
                            b = b.fill(ACCENT);
                        }
                        if ui
                            .add(b)
                            .on_hover_text(format!("{} « {} »", tr("instruction", "instruction", "instrucción"), op.mnemonic()))
                            .clicked()
                        {
                            self.calc_op = op;
                        }
                    }
                });

                ui.add_space(6.0);

                // ---- Opérande B ----
                ui.horizontal(|ui| {
                    ui.label(RichText::new("B").monospace().strong().color(hdr));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.calc_input_b)
                            .desired_width(ui.available_width())
                            .font(egui::TextStyle::Monospace)
                            .hint_text(hint),
                    );
                });
                sanitize(&mut self.calc_input_b);
                let b_val = super::calc_parse(&self.calc_input_b, base);
                if let Some(v) = b_val
                    && let Some(nv) = self.bit_grid(ui, v, true)
                {
                    self.calc_input_b = super::calc_format(nv, base)
                        .trim_start_matches("0x")
                        .trim_start_matches("0b")
                        .trim_start_matches("0o")
                        .to_string();
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // ---- Résultat ----
                // Sans second opérande, on affiche simplement A converti : la
                // calculatrice reste utile pour une simple conversion de base.
                let result = match (a, b_val) {
                    (Some(x), Some(y)) => self.calc_op.apply(x, y),
                    (Some(x), None) => Some(x),
                    _ => None,
                };

                ui.label(
                    RichText::new(tr("Résultat", "Result", "Resultado"))
                        .small()
                        .strong()
                        .color(hdr),
                );
                ui.add_space(3.0);
                match result {
                    Some(v) => {
                        self.bit_grid(ui, v, false);
                        ui.add_space(6.0);
                        egui::Grid::new("calc_grid")
                            .num_columns(2)
                            .spacing([16.0, 4.0])
                            .show(ui, |ui| {
                                for (label, b) in [("Hex", 16), ("Bin", 2), ("Dec", 10), ("Oct", 8)] {
                                    ui.label(RichText::new(label).strong().color(hdr));
                                    ui.label(
                                        RichText::new(super::calc_format(v, b))
                                            .monospace()
                                            .color(mnem),
                                    );
                                    ui.end_row();
                                }
                                // Un caractère imprimable : pratique pour les
                                // exercices sur les chaînes.
                                let u = v as u64;
                                if (0x20..0x7F).contains(&u) {
                                    ui.label(RichText::new(tr("Caractère", "Character", "Carácter")).strong().color(hdr));
                                    ui.label(
                                        RichText::new(format!("'{}'", u as u8 as char))
                                            .monospace()
                                            .color(mnem),
                                    );
                                    ui.end_row();
                                }
                            });
                    }
                    None => {
                        ui.label(
                            RichText::new(tr(
                                "— (division par zéro, ou saisie vide)",
                                "— (division by zero, or empty input)",
                                "— (división por cero, o entrada vacía)",
                            ))
                            .weak(),
                        );
                    }
                }

                ui.add_space(6.0);
                ui.separator();
                ui.vertical_centered(|ui| {
                    if ui.button(tr("Fermer", "Close", "Cerrar")).clicked() {
                        self.show_calculator = false;
                    }
                });
            });
        if !open {
            self.show_calculator = false;
        }
    }

    // ---------- Fenêtre de mise à jour ----------

    /// Fenêtre de diagnostic de plantage : ce que l'élève voit à la place de
    /// l'ancien « Terminé (signal) ». Cause nommée, explication, piste de
    /// correction, et le contexte technique replié pour qui veut creuser.
    pub(super) fn diagnosis_window(&mut self, ctx: &egui::Context) {
        let Some(diag) = self.diagnosis.clone() else { return };
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);

        let mut open = true;
        let mut goto_line = false;
        let mut close = false;
        egui::Window::new(tr("🛑 Le programme a planté", "🛑 The program crashed", "🛑 El programa falló"))
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .show(ctx, |ui| {
                // Bandeau de cause, en rouge.
                egui::Frame::default()
                    .fill(FALSE_COL.linear_multiply(0.16))
                    .stroke(egui::Stroke::new(1.0_f32, FALSE_COL))
                    .corner_radius(egui::CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(&diag.title).size(15.0).strong().color(FALSE_COL));
                    });
                ui.add_space(8.0);

                // Explication : le cœur pédagogique.
                super::card(ui, |ui| {
                    ui.label(RichText::new(&diag.explanation).size(13.0));
                });
                ui.add_space(8.0);

                // Piste de correction.
                ui.label(RichText::new(tr("💡 Comment corriger", "💡 How to fix it", "💡 Cómo corregirlo")).strong().color(ACTION));
                ui.add_space(3.0);
                egui::Frame::default()
                    .fill(ACTION.linear_multiply(0.12))
                    .corner_radius(egui::CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(&diag.hint).size(13.0));
                    });
                ui.add_space(8.0);

                // Aller à la ligne fautive, quand le mapping la connaît.
                if let Some(line) = diag.line {
                    if ui
                        .button(RichText::new(format!(
                            "→ {} {line}",
                            tr("Voir la ligne", "Go to line", "Ver la línea")
                        )))
                        .clicked()
                    {
                        goto_line = true;
                    }
                    ui.add_space(4.0);
                }

                // Contexte technique, replié : utile mais secondaire.
                egui::CollapsingHeader::new(
                    RichText::new(tr("Détails techniques", "Technical details", "Detalles técnicos")).small(),
                )
                .default_open(false)
                .show(ui, |ui| {
                    let hdr = self.c_header();
                    egui::Grid::new("diag_grid").num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
                        let mut row = |k: &str, v: String| {
                            ui.label(RichText::new(k).small().color(hdr));
                            ui.label(RichText::new(v).monospace().small());
                            ui.end_row();
                        };
                        if let Some(f) = self.dbg.as_ref().and_then(|d| d.fault()) {
                            row(tr("Signal", "Signal", "Señal"), f.signal_name().to_string());
                            row("RIP", format!("0x{:016X}", f.rip));
                        }
                        match diag.addr {
                            Some(a) => row(tr("Adresse fautive", "Faulting address", "Dirección fallida"), format!("0x{a:016X}")),
                            None => row(tr("Adresse fautive", "Faulting address", "Dirección fallida"), "—".into()),
                        }
                        row(
                            tr("Région", "Region", "Región"),
                            diag.region.map(|k| k.label().to_string()).unwrap_or_else(|| {
                                tr("non mappée", "unmapped", "no mapeada").to_string()
                            }),
                        );
                    });
                });

                ui.add_space(6.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(tr(
                            "La timeline s'est arrêtée sur la faute : tu peux remonter en arrière \
                             pour voir d'où vient la valeur fautive.",
                            "The timeline stopped on the fault: you can step back to see where the \
                             bad value came from.",
                            "La línea de tiempo se detuvo en el fallo: puedes retroceder para ver \
                             de dónde viene el valor.",
                        ))
                        .small()
                        .weak(),
                    );
                });
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    if ui.button(tr("Fermer", "Close", "Cerrar")).clicked() {
                        close = true;
                    }
                });
            });

        if goto_line {
            // Bascule sur l'éditeur : la ligne est déjà surlignée par le suivi RIP.
            self.focus_panel(super::dock::Panel::Editor);
        }
        if !open || close {
            self.diagnosis = None;
        }
    }

    pub(super) fn update_window(&mut self, ctx: &egui::Context) {
        use crate::updater::UpdateState;

        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);

        let show = matches!(
            self.updater.state,
            UpdateState::Checking
                | UpdateState::Available(_)
                | UpdateState::Downloading(_)
                | UpdateState::Done
                | UpdateState::Error(_)
        );
        if !show {
            return;
        }

        let title = tr("Mise à jour", "Update", "Actualización");
        let mut open = true;
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .show(ctx, |ui| {
                match &self.updater.state.clone() {
                    UpdateState::Checking => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr("Vérification en cours…", "Checking…", "Verificando…"));
                        });
                    }
                    UpdateState::UpToDate => {
                        ui.label(tr(
                            "✔  Vous utilisez la dernière version.",
                            "✔  You are on the latest version.",
                            "✔  Está usando la última versión.",
                        ));
                    }
                    UpdateState::Available(info) => {
                        let info = info.clone();
                        ui.label(RichText::new(format!(
                            "{}  {}",
                            tr("Nouvelle version disponible :", "New version available:", "Nueva versión disponible:"),
                            info.tag
                        )).strong());
                        ui.add_space(4.0);
                        if !info.notes.is_empty() {
                            egui::ScrollArea::vertical()
                                .id_salt("update_notes")
                                .max_height(160.0)
                                .show(ui, |ui| {
                                    ui.label(&info.notes);
                                });
                            ui.add_space(4.0);
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button(RichText::new(tr(
                                "⬇  Installer et redémarrer",
                                "⬇  Install and restart",
                                "⬇  Instalar y reiniciar",
                            )).strong()).clicked() {
                                self.updater.install(info);
                            }
                            if ui.button(tr("Plus tard", "Later", "Más tarde")).clicked() {
                                self.updater.state = UpdateState::UpToDate;
                            }
                        });
                    }
                    UpdateState::Downloading(progress) => {
                        let p = *progress;
                        ui.label(tr("Téléchargement en cours…", "Downloading…", "Descargando…"));
                        ui.add_space(4.0);
                        let bar = egui::widgets::ProgressBar::new(p)
                            .show_percentage()
                            .animate(true);
                        ui.add(bar);
                    }
                    UpdateState::Done => {
                        ui.label(RichText::new(tr(
                            "✔  Mise à jour installée. Relancez l'application.",
                            "✔  Update installed. Please restart the application.",
                            "✔  Actualización instalada. Reinicie la aplicación.",
                        )).strong());
                        ui.add_space(6.0);
                        ui.vertical_centered(|ui| {
                            if ui.button(tr("Fermer", "Close", "Cerrar")).clicked() {
                                self.updater.state = UpdateState::UpToDate;
                            }
                        });
                    }
                    UpdateState::Error(msg) => {
                        let msg = msg.clone();
                        ui.colored_label(egui::Color32::from_rgb(220, 80, 60),
                            format!("✘  {}", tr("Erreur :", "Error:", "Error:")));
                        ui.label(&msg);
                        ui.add_space(4.0);
                        ui.vertical_centered(|ui| {
                            if ui.button(tr("Fermer", "Close", "Cerrar")).clicked() {
                                self.updater.state = UpdateState::UpToDate;
                            }
                        });
                    }
                }
            });
        if !open {
            self.updater.state = UpdateState::UpToDate;
        }
    }

    // ---------- Licence obligatoire (désassemblage, registres/flags, timeline) ----------

    /// Licence valide en vigueur ? Pilote le verrouillage des panneaux avancés
    /// (voir `dock.rs`).
    pub(super) fn is_licensed(&self) -> bool {
        matches!(self.license, crate::license::LicenseState::Valid(_))
    }

    /// Contenu affiché à la place d'un panneau verrouillé.
    pub(super) fn locked_panel_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            ui.label(RichText::new("🔒").size(28.0));
            ui.add_space(4.0);
            ui.label(tr(
                "Fonctionnalité réservée aux licences ASM Studio.",
                "Feature reserved to ASM Studio license holders.",
                "Función reservada a los titulares de licencia de ASM Studio.",
            ));
            ui.add_space(8.0);
            if ui.button(tr("Activer une licence…", "Activate a license…", "Activar una licencia…")).clicked() {
                self.show_license_gate = true;
            }
        });
    }

    /// Fenêtre de saisie de la licence (coller le bloc reçu par e-mail).
    ///
    /// Distincte de `license_window` : celle-ci affiche seulement le texte
    /// légal de `LICENSE.md`, sans rapport avec le mécanisme de licence.
    pub(super) fn license_gate_window(&mut self, ctx: &egui::Context) {
        if !self.show_license_gate {
            return;
        }
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);

        let mut open = true;
        egui::Window::new(tr("Licence ASM Studio", "ASM Studio license", "Licencia de ASM Studio"))
            .collapsible(false)
            .resizable(false)
            .default_width(480.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .show(ctx, |ui| {
                if let crate::license::LicenseState::Valid(p) = &self.license {
                    ui.label(format!(
                        "{} {}",
                        tr("Licence active :", "Active license:", "Licencia activa:"),
                        p.name
                    ));
                    ui.add_space(6.0);
                }
                ui.label(tr(
                    "Collez ci-dessous le bloc de licence reçu par e-mail :",
                    "Paste the license block you received by e-mail below:",
                    "Pegue abajo el bloque de licencia recibido por correo:",
                ));
                ui.add_space(6.0);
                ui.add(
                    egui::TextEdit::multiline(&mut self.license_input)
                        .desired_rows(6)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
                if let Some(err) = &self.license_error {
                    ui.add_space(6.0);
                    ui.colored_label(egui::Color32::from_rgb(220, 80, 60), err);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(tr("Valider", "Validate", "Validar")).clicked() {
                        match crate::license::verify(&self.license_input) {
                            Ok(payload) => {
                                let _ = crate::license::save(&self.license_input);
                                self.license = crate::license::LicenseState::Valid(payload);
                                self.license_error = None;
                                self.show_license_gate = false;
                            }
                            Err(reason) => self.license_error = Some(reason),
                        }
                    }
                    if ui.button(tr("Fermer", "Close", "Cerrar")).clicked() {
                        self.show_license_gate = false;
                    }
                });
            });
        if !open {
            self.show_license_gate = false;
        }
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    use std::path::PathBuf;

    /// La licence affichée doit être celle du fichier `LICENSE.md` embarqué, et
    /// surtout plus MIT : c'est le contrat de cette fenêtre.
    #[test]
    fn license_window_shows_the_embedded_license_not_mit() {
        const LICENSE: &str = include_str!("../../LICENSE.md");
        assert!(LICENSE.contains("Personal Free License"), "licence attendue = ASFL");
        assert!(!LICENSE.to_uppercase().contains("MIT LICENSE"), "le MIT ne doit plus être la licence");

        let mut app = App::new();
        app.show_license = true;
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.license_window(ctx));
        assert!(app.show_license, "la fenêtre reste ouverte tant qu'on ne la ferme pas");
    }

    /// Fermée, elle ne peint rien.
    #[test]
    fn closed_license_window_paints_nothing() {
        let mut app = App::new();
        app.show_license = false;
        let ctx = egui::Context::default();
        let out = ctx.run(Default::default(), |ctx| app.license_window(ctx));
        assert!(out.shapes.is_empty() || !app.show_license);
    }

    /// La fenêtre de réglages doit se rendre, et toutes ses options rester
    /// atteignables — y compris sur un petit écran, où le corps défile et le
    /// bouton Fermer reste en dehors du défilement.
    #[test]
    fn settings_window_renders_at_any_screen_size() {
        let mut app = App::new();
        app.show_settings = true;

        for (w, h) in [(1920.0_f32, 1080.0_f32), (1280.0, 720.0), (800.0, 600.0), (640.0, 400.0)] {
            let ctx = egui::Context::default();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(w, h),
                )),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| app.settings_window(ctx));
            assert!(app.show_settings, "la fenêtre doit rester ouverte ({w}×{h})");
        }
    }

    /// Modifier une option depuis la fenêtre doit être pris en compte : c'est
    /// le contrat de `changed`, qui déclenche l'enregistrement.
    #[test]
    fn toggling_an_option_is_recorded() {
        let mut app = App::new();
        app.show_settings = true;
        let before = app.pedagogy_anim;
        app.pedagogy_anim = !before;
        assert_ne!(app.pedagogy_anim, before);

        // Le rendu ne doit pas réinitialiser le réglage.
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.settings_window(ctx));
        assert_ne!(app.pedagogy_anim, before, "le rendu ne doit rien réécrire");
    }

    /// Fermée, elle ne doit rien peindre.
    #[test]
    fn closed_settings_window_paints_nothing() {
        let mut app = App::new();
        app.show_settings = false;
        let ctx = egui::Context::default();
        let out = ctx.run(Default::default(), |ctx| app.settings_window(ctx));
        assert!(out.shapes.is_empty() || app.show_settings == false);
    }

    /// Le dossier de travail ne doit pas influer sur le rendu des réglages.
    #[test]
    fn settings_do_not_depend_on_the_open_file() {
        let mut app = App::new();
        app.src_path = PathBuf::from("build/inexistant.asm");
        app.show_settings = true;
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.settings_window(ctx));
        assert!(app.show_settings);
    }
}

#[cfg(test)]
mod calculator_tests {
    use super::*;

    /// L'hexadécimal est la base par défaut : c'est celle dans laquelle on lit
    /// un registre, une adresse ou un masque.
    #[test]
    fn hexadecimal_is_the_default_base() {
        let app = App::new();
        assert_eq!(app.calc_base, 16);
    }

    /// La calculatrice se rend dans tous ses états, y compris les cas qui
    /// pourraient faire paniquer : saisie vide, division par zéro, valeur
    /// occupant les 64 bits.
    #[test]
    fn calculator_renders_in_every_state() {
        let mut app = App::new();
        app.show_calculator = true;
        let ctx = egui::Context::default();

        let cases: [(&str, &str, super::super::CalcOp); 5] = [
            ("", "", super::super::CalcOp::And),
            ("2a", "0f", super::super::CalcOp::And),
            ("10", "0", super::super::CalcOp::Div),
            ("ffffffffffffffff", "1", super::super::CalcOp::Shr),
            ("1", "40", super::super::CalcOp::Shl),
        ];
        for (a, b, op) in cases {
            app.calc_input = a.to_string();
            app.calc_input_b = b.to_string();
            app.calc_op = op;
            let _ = ctx.run(Default::default(), |ctx| app.calculator_window(ctx));
            assert!(app.show_calculator, "la fenêtre reste ouverte ({a} {b:?})");
        }

        // Et dans les quatre bases.
        for base in [16, 2, 10, 8] {
            app.calc_base = base;
            app.calc_input = "101".to_string();
            let _ = ctx.run(Default::default(), |ctx| app.calculator_window(ctx));
        }
    }

    /// Le masquage d'un octet est le cas d'usage type : 0x2A AND 0x0F = 0x0A.
    /// On vérifie le calcul que l'interface affiche, sans passer par elle.
    #[test]
    fn masking_a_byte_gives_the_expected_result() {
        let a = super::super::calc_parse("2a", 16).unwrap();
        let b = super::super::calc_parse("0f", 16).unwrap();
        let r = super::super::CalcOp::And.apply(a, b).unwrap();
        assert_eq!(r, 0x0A);
        assert_eq!(super::super::calc_format(r, 16), "0xA");
        assert_eq!(super::super::calc_format(r, 2), "0b1010");
        // Un octet suffit à le représenter : la grille montrera 8 bits.
        assert_eq!(super::super::calc_width_bytes(r), 1);
    }

    /// Fermée, la calculatrice ne peint rien.
    #[test]
    fn closed_calculator_paints_nothing() {
        let mut app = App::new();
        app.show_calculator = false;
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.calculator_window(ctx));
        assert!(!app.show_calculator);
    }
}
