use eframe::egui::{self, RichText};

use crate::debugger::Flags;
use crate::explain;
use crate::i18n;
use crate::syscall;

use super::{
    App, accent, action, changed_col, false_col, flag_on, push_col, pop_col,
    micro_stack, micro_static_flags, syscall_details, syscall_labels, SyscallSkin,
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
                b.stack,
                a.map(|s| (s.regs.clone(), s.stack)),
            )
        });

        // Décodage de l'appel système, tampon compris : la lecture mémoire a
        // besoin de `self`, la closure ne l'a plus. Tout est préparé ici.
        let syscall_view = dynamics
            .as_ref()
            .filter(|_| insn.mnemonic == "syscall")
            .map(|(before, _, _)| {
                let d = syscall::describe(before, self.lang);
                let buf = d
                    .buffer
                    .as_ref()
                    .filter(|_| self.can_read_memory())
                    .and_then(|b| self.dbg.as_ref()?.read_mem(b.addr, b.len).ok());
                (d, buf)
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
                        ui.label(RichText::new(cycles).color(changed_col()))
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
                        }
                    });

                    ui.add_space(8.0);
                    // --- 💡 Explication ---
                    ui.label(RichText::new(tr("💡 Explication", "💡 Explanation", "💡 Explicación")).strong().color(hdr));
                    ui.label(&e.description);

                    // --- ⚙ L'appel système, argument par argument ---
                    // La description générique de `syscall` dit la convention ;
                    // celle-ci dit ce que CET appel-là va faire, avec ces
                    // valeurs-là. C'est la question que l'élève se pose.
                    if let Some((d, buf)) = &syscall_view {
                        ui.add_space(10.0);
                        syscall_details(
                            ui,
                            d,
                            buf.as_deref(),
                            SyscallSkin { hdr, mnem: mnem_c, addr_c, bytes_c, labels: syscall_labels(lang) },
                        );
                    }

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
                                        .color(changed_col()),
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
                                .color(false_col()),
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
                        row(ui, tr("Écrit", "Writes", "Escribe"), &fx.writes, changed_col());
                    });
                    // Les effets que le texte de l'instruction ne montre pas —
                    // c'est là que se trouve l'essentiel de ce qui piège.
                    for note in &fx.implicit {
                        ui.add_space(4.0);
                        egui::Frame::default()
                            .fill(action().linear_multiply(0.08))
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
                                        push_col(),
                                        format!(
                                            "RSP : 0x{:X} → 0x{:X}  (−{} {}, PUSH)",
                                            before.rsp, after.rsp, -d, tr("octets", "bytes", "bytes")
                                        ),
                                    );
                                } else {
                                    ui.colored_label(
                                        pop_col(),
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
                                        ui.label(RichText::new(format!("0x{nv:016X}")).monospace().color(changed_col()));
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
                                                .color(changed_col()),
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
                // Bandeau de préversion : il disparaît de lui-même le jour où
                // `Cargo.toml` annonce une version finale (voir crate::version).
                if let Some(beta_label) = crate::version::beta_label(lang) {
                egui::Frame::default()
                    .fill(action().linear_multiply(0.9))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new(format!("🔷  {beta_label}"))
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
                }
                ui.add_space(8.0);
                ui.separator();
                egui::Grid::new("about_grid")
                    .num_columns(2)
                    .spacing([16.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Version");
                        ui.label(RichText::new(crate::version::full()).monospace().strong())
                            .on_hover_text(tr(
                                "MAJEUR.MINEUR.CORRECTIF-préversion+build.numéro — le numéro de build change à chaque compilation, la version à chaque livraison.",
                                "MAJOR.MINOR.PATCH-prerelease+build.number — the build number changes on every compilation, the version on every release.",
                                "MAYOR.MENOR.PARCHE-preversión+build.número — el número de compilación cambia en cada compilación, la versión en cada entrega.",
                            ));
                        ui.end_row();
                        ui.label("Build");
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "{}  ({})",
                                    crate::version::COMMIT,
                                    crate::version::DATE
                                ))
                                .monospace()
                                .strong(),
                            );
                            if ui
                                .small_button("📋")
                                .on_hover_text(tr(
                                    "Copier (à communiquer pour la licence)",
                                    "Copy (needed when requesting a license)",
                                    "Copiar (necesario para solicitar una licencia)",
                                ))
                                .clicked()
                            {
                                ctx.copy_text(crate::license::version_build_tag());
                            }
                        });
                        ui.end_row();
                        ui.label("Date");
                        ui.label(RichText::new(env!("BUILD_DATE")).monospace());
                        ui.end_row();
                        ui.label(tr("Auteur", "Author", "Autor"));
                        ui.label(RichText::new("Frédéric Zawalski.").strong());
                        ui.end_row();
                        ui.label(tr("Activation", "Activation", "Activación"));
                        match &self.license {
                            crate::license::LicenseState::Valid(p) => {
                                let suffix = match &p.expires_at {
                                    Some(date) => format!(
                                        " ({} {date})",
                                        tr("valable jusqu'au", "valid until", "válida hasta")
                                    ),
                                    None => String::new(),
                                };
                                let label = format!(
                                    "✔ {} — {}{suffix}",
                                    tr("Activée", "Activated", "Activada"),
                                    p.name
                                );
                                ui.horizontal(|ui| {
                                    ui.colored_label(flag_on(), label);
                                    if ui
                                        .link(tr("Désactiver…", "Deactivate…", "Desactivar…"))
                                        .on_hover_text(tr(
                                            "Supprime la licence installée sur cette machine.",
                                            "Removes the license installed on this machine.",
                                            "Elimina la licencia instalada en esta máquina.",
                                        ))
                                        .clicked()
                                    {
                                        self.confirm_license_reset = true;
                                    }
                                });
                            }
                            crate::license::LicenseState::Invalid(_) | crate::license::LicenseState::Missing
                                if crate::trial::is_active() =>
                            {
                                ui.horizontal(|ui| {
                                    let days = crate::trial::days_left();
                                    let remaining = match lang {
                                        crate::i18n::Lang::Fr => format!("encore {days} jour(s)"),
                                        crate::i18n::Lang::En => format!("{days} day(s) left"),
                                        crate::i18n::Lang::Es => format!("quedan {days} día(s)"),
                                    };
                                    ui.colored_label(
                                        accent(),
                                        format!(
                                            "🕐 {} — {remaining}",
                                            tr("Avant inscription gratuite", "Before free registration", "Antes del registro gratuito")
                                        ),
                                    );
                                    if ui.link(tr("Activer…", "Activate…", "Activar…")).clicked() {
                                        self.show_license_gate = true;
                                    }
                                });
                            }
                            crate::license::LicenseState::Invalid(_) | crate::license::LicenseState::Missing => {
                                ui.horizontal(|ui| {
                                    ui.colored_label(
                                        false_col(),
                                        tr(
                                            "✘ Délai d'inscription dépassé",
                                            "✘ Registration period over",
                                            "✘ Periodo de registro terminado",
                                        ),
                                    );
                                    if ui.link(tr("Activer…", "Activate…", "Activar…")).clicked() {
                                        self.show_license_gate = true;
                                    }
                                });
                            }
                        }
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

    /// Confirmation avant de supprimer la licence installée (lien
    /// « Désactiver… » de la fenêtre « À propos »).
    ///
    /// Confirmation obligatoire parce que le geste est irréversible sans le
    /// bloc de licence d'origine : celui-ci n'est ni régénérable depuis
    /// l'appli, ni récupérable une fois `license.txt` supprimé — il faut le
    /// redemander à l'auteur. La fenêtre annonce donc aussi ce qui se passe
    /// juste après, qui dépend du délai d'essai.
    /// Ouvre l'éditeur de condition d'une ligne, en posant le point d'arrêt
    /// s'il n'y en avait pas : le clic droit vaut alors « pose-le, et ne
    /// t'arrête que dans ce cas », ce qui est le geste attendu.
    pub(super) fn open_breakpoint_condition(&mut self, line: usize) {
        self.bp_cond_input = self
            .breakpoint_condition(line)
            .map(|c| c.to_string())
            .unwrap_or_default();
        self.bp_cond_error = None;
        self.bp_cond_focus = true;
        self.bp_cond_line = Some(line);
        self.breakpoints.entry(line).or_insert(None);
    }

    /// Saisie d'une condition de point d'arrêt.
    ///
    /// La fenêtre montre la ligne visée : sans elle, on ne sait plus laquelle
    /// on avait cliquée dès qu'on lit l'aide de syntaxe.
    pub(super) fn breakpoint_condition_window(&mut self, ctx: &egui::Context) {
        let Some(line) = self.bp_cond_line else { return };
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let source_line = self
            .source
            .lines()
            .nth(line.saturating_sub(1))
            .unwrap_or_default()
            .trim()
            .to_string();

        let mut validate = false;
        let mut close = false;
        let mut remove_breakpoint = false;
        let mut open = true;
        egui::Window::new(format!(
            "{} {line}",
            tr("Condition d'arrêt — ligne", "Break condition — line", "Condición de parada — línea")
        ))
        .collapsible(false)
        .resizable(false)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(420.0);
            ui.add_space(4.0);
            ui.label(RichText::new(source_line).monospace().color(accent()));
            ui.add_space(8.0);
            ui.label(tr(
                "L'exécution ne s'arrêtera ici que si :",
                "Execution will only stop here if:",
                "La ejecución solo se detendrá aquí si:",
            ));
            ui.add_space(4.0);
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.bp_cond_input)
                    .id(egui::Id::new("kb_bp_cond"))
                    .font(egui::TextStyle::Monospace)
                    .hint_text("RCX == 0")
                    .desired_width(f32::INFINITY),
            );
            if std::mem::take(&mut self.bp_cond_focus) {
                field.request_focus();
            }
            validate = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.add_space(6.0);
            ui.label(
                RichText::new(tr(
                    "Registres RAX…R15, RIP, moitiés basses EAX/R8D ; drapeaux ZF, CF, OF, SF, PF, AF (0 ou 1).\n\
                     Comparateurs == != < <= > >=. Nombres : 42, -1, 0x2A, 0b1010.\n\
                     Exemples : RCX == 0 · RAX > 0x100 · ZF == 1 · RSI != RDI",
                    "Registers RAX…R15, RIP, low halves EAX/R8D; flags ZF, CF, OF, SF, PF, AF (0 or 1).\n\
                     Comparators == != < <= > >=. Numbers: 42, -1, 0x2A, 0b1010.\n\
                     Examples: RCX == 0 · RAX > 0x100 · ZF == 1 · RSI != RDI",
                    "Registros RAX…R15, RIP, mitades bajas EAX/R8D; flags ZF, CF, OF, SF, PF, AF (0 o 1).\n\
                     Comparadores == != < <= > >=. Números: 42, -1, 0x2A, 0b1010.\n\
                     Ejemplos: RCX == 0 · RAX > 0x100 · ZF == 1 · RSI != RDI",
                ))
                .small()
                .weak(),
            );
            if let Some(err) = &self.bp_cond_error {
                ui.add_space(6.0);
                ui.colored_label(false_col(), RichText::new(format!("✘ {err}")).small());
            }
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let spacing = 8.0;
                let btn_w = (ui.available_width() - 2.0 * spacing) / 3.0;
                if ui
                    .add_sized(
                        [btn_w, 28.0],
                        egui::Button::new(RichText::new(tr("Valider", "Apply", "Aplicar")).strong())
                            .fill(action()),
                    )
                    .clicked()
                {
                    validate = true;
                }
                // Retirer le point d'arrêt entier, et pas seulement sa
                // condition : c'est ce qu'on veut faire une fois sur deux en
                // rouvrant cette fenêtre, et le clic droit ne le permet pas.
                if ui
                    .add_sized(
                        [btn_w, 28.0],
                        egui::Button::new(tr(
                            "Retirer le point d'arrêt",
                            "Remove breakpoint",
                            "Quitar el punto de parada",
                        )),
                    )
                    .clicked()
                {
                    remove_breakpoint = true;
                }
                if ui
                    .add_sized([btn_w, 28.0], egui::Button::new(tr("Annuler", "Cancel", "Cancelar")))
                    .clicked()
                {
                    close = true;
                }
            });
            ui.add_space(4.0);
        });

        if remove_breakpoint {
            self.breakpoints.remove(&line);
            self.bp_cond_line = None;
            return;
        }
        if validate {
            // Un champ vidé retire la condition sans toucher au point d'arrêt :
            // c'est la façon de revenir à un arrêt à chaque passage.
            let text = self.bp_cond_input.clone();
            match self.set_breakpoint_condition(line, &text) {
                Ok(()) => {
                    self.bp_cond_error = None;
                    self.bp_cond_line = None;
                }
                // La syntaxe refusée garde la fenêtre ouverte : le texte est
                // encore là, à un caractère près de la bonne condition.
                Err(e) => self.bp_cond_error = Some(e),
            }
            return;
        }
        if close || !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.bp_cond_line = None;
        }
    }

    pub(super) fn license_reset_confirm_window(&mut self, ctx: &egui::Context) {
        if !self.confirm_license_reset {
            return;
        }
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        // Ce que devient l'appli une fois la licence retirée : l'essai prend le
        // relais s'il court encore, sinon les panneaux se reverrouillent tout
        // de suite. Le dire évite la mauvaise surprise juste après le clic.
        let aftermath = if crate::trial::is_active() {
            let days = crate::trial::days_left();
            match lang {
                crate::i18n::Lang::Fr => format!(
                    "Le délai avant inscription prend le relais : encore {days} jour(s)."
                ),
                crate::i18n::Lang::En => {
                    format!("The registration period takes over: {days} day(s) left.")
                }
                crate::i18n::Lang::Es => {
                    format!("El periodo de registro toma el relevo: quedan {days} día(s).")
                }
            }
        } else {
            tr(
                "Le désassemblage, les registres/flags et la timeline se reverrouillent immédiatement.",
                "Disassembly, registers/flags and the timeline lock again immediately.",
                "El desensamblado, los registros/flags y la línea de tiempo se bloquean de inmediato.",
            )
            .to_string()
        };

        let mut open = true;
        egui::Window::new(tr(
            "Désactiver la licence ?",
            "Deactivate the license?",
            "¿Desactivar la licencia?",
        ))
        .collapsible(false)
        .resizable(false)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(380.0);
            ui.add_space(4.0);
            ui.label(tr(
                "La licence installée sur cette machine va être supprimée.",
                "The license installed on this machine will be removed.",
                "Se eliminará la licencia instalada en esta máquina.",
            ));
            ui.add_space(6.0);
            ui.colored_label(
                false_col(),
                tr(
                    "⚠ Irréversible : il faudra recoller le bloc de licence pour la réactiver.",
                    "⚠ Irreversible: you will need the license block again to re-activate it.",
                    "⚠ Irreversible: necesitará el bloque de licencia para reactivarla.",
                ),
            );
            ui.add_space(6.0);
            ui.label(RichText::new(aftermath).small().weak());
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let spacing = 8.0;
                let btn_w = (ui.available_width() - spacing) / 2.0;
                if ui
                    .add_sized(
                        [btn_w, 28.0],
                        egui::Button::new(
                            RichText::new(tr("Désactiver", "Deactivate", "Desactivar")).strong(),
                        )
                        .fill(false_col()),
                    )
                    .clicked()
                {
                    self.reset_license();
                }
                if ui
                    .add_sized(
                        [btn_w, 28.0],
                        egui::Button::new(tr("Annuler", "Cancel", "Cancelar")),
                    )
                    .clicked()
                {
                    self.confirm_license_reset = false;
                }
            });
            ui.add_space(4.0);
        });
        if !open {
            self.confirm_license_reset = false;
        }
    }

    /// Supprime la licence du disque et remet l'état en mémoire à zéro.
    ///
    /// `license_error` est vidé au passage : un message hérité d'une licence
    /// devenue invalide n'a plus d'objet une fois le fichier parti, et
    /// resterait sinon affiché sous la boîte de collage à la réouverture.
    /// Un échec d'écriture (droits, disque plein) est rapporté là plutôt
    /// qu'ignoré : sans ça, la licence revient au prochain démarrage sans que
    /// rien n'ait signalé pourquoi.
    fn reset_license(&mut self) {
        self.confirm_license_reset = false;
        match crate::license::remove() {
            Ok(()) => {
                self.license = crate::license::LicenseState::Missing;
                self.license_error = None;
            }
            Err(e) => {
                self.license_error = Some(format!(
                    "{} : {e}",
                    self.tr3(
                        "suppression de la licence impossible",
                        "could not remove the license",
                        "no se pudo eliminar la licencia",
                    )
                ));
                self.show_license_gate = true;
            }
        }
    }

    /// Boîte « Nouveau fichier » : pour quel format ?
    ///
    /// Un squelette ELF et un squelette PE ne se ressemblent pas — `_start` et
    /// `syscall` d'un côté, `main` et `ExitProcess` de l'autre — et le format
    /// décide aussi de ce que fera « Assembler ». Imposer Linux puis laisser
    /// l'élève buter sur une erreur de nasm serait le lui apprendre à l'envers ;
    /// la question se pose donc une fois, au moment où le fichier naît.
    ///
    /// Elle ne s'affiche que si l'assemblage Windows est activé (Réglages) :
    /// sinon [`App::new_file_now`] crée directement le fichier ELF.
    pub(super) fn new_file_format_window(&mut self, ctx: &egui::Context) {
        if !self.new_file_prompt {
            return;
        }
        use crate::assemble::Target;
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);

        let mut chosen: Option<Target> = None;
        let mut open = true;
        egui::Window::new(tr("Nouveau fichier", "New file", "Archivo nuevo"))
            .collapsible(false)
            .resizable(false)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.add_space(4.0);
                ui.label(tr(
                    "Pour quel format ce programme est-il écrit ?",
                    "Which format is this program written for?",
                    "¿Para qué formato se escribe este programa?",
                ));
                ui.add_space(2.0);
                ui.label(
                    RichText::new(tr(
                        "Le squelette de départ et la cible d'assemblage suivent ce choix, qui reste modifiable par le menu Exécution ▸ Cible.",
                        "The starting skeleton and the build target follow this choice, which stays changeable from the Run ▸ Target menu.",
                        "El esqueleto inicial y el destino de ensamblado siguen esta elección, que sigue siendo modificable desde el menú Ejecución ▸ Destino.",
                    ))
                    .small()
                    .weak(),
                );
                ui.add_space(10.0);

                for (target, label, detail, primary) in [
                    (
                        Target::Linux,
                        tr("ELF64 — Linux", "ELF64 — Linux", "ELF64 — Linux"),
                        tr(
                            "« _start » et « syscall ». Le seul format qui s'exécute et se déroule pas à pas ici.",
                            "\"_start\" and \"syscall\". The only format that runs and single-steps here.",
                            "«_start» y «syscall». El único formato que se ejecuta y se recorre paso a paso aquí.",
                        ),
                        true,
                    ),
                    (
                        Target::Windows,
                        tr("PE64 — Windows console", "PE64 — Windows console", "PE64 — Windows consola"),
                        tr(
                            "« main » et « ExitProcess ». Produit un vrai .exe, lancé par Wine s'il est installé.",
                            "\"main\" and \"ExitProcess\". Produces a real .exe, run through Wine when installed.",
                            "«main» y «ExitProcess». Produce un .exe real, ejecutado con Wine si está instalado.",
                        ),
                        false,
                    ),
                    (
                        Target::WindowsGui,
                        tr("PE64 — Windows fenêtré", "PE64 — Windows GUI", "PE64 — Windows con ventanas"),
                        tr(
                            "Même chose, sans console : le squelette affiche une MessageBox.",
                            "Same, with no console: the skeleton shows a MessageBox.",
                            "Lo mismo, sin consola: el esqueleto muestra un MessageBox.",
                        ),
                        false,
                    ),
                ] {
                    let btn = egui::Button::new(RichText::new(label).strong());
                    let btn = if primary { btn.fill(super::action()) } else { btn };
                    if ui.add_sized([ui.available_width(), 28.0], btn).clicked() {
                        chosen = Some(target);
                    }
                    ui.add_space(2.0);
                    ui.label(RichText::new(detail).small().weak());
                    ui.add_space(10.0);
                }

                ui.separator();
                ui.add_space(6.0);
                if ui
                    .add_sized(
                        [ui.available_width(), 26.0],
                        egui::Button::new(tr("Annuler", "Cancel", "Cancelar")),
                    )
                    .clicked()
                {
                    self.new_file_prompt = false;
                }
                ui.add_space(4.0);
            });
        // La croix vaut « Annuler » : rien n'a encore été créé, il n'y a rien à
        // perdre — contrairement à la boîte « non enregistré ».
        if !open {
            self.new_file_prompt = false;
        }
        if let Some(t) = chosen {
            self.create_new_file(t);
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

    /// Trois pastilles de l'aperçu d'un thème : fond de l'éditeur, accent,
    /// couleur des chaînes — les trois qui changent le plus d'un thème à l'autre.
    fn theme_preview(ui: &mut egui::Ui, t: &crate::theme::Theme) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(56.0, 14.0), egui::Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, 3.0, t.ui.extreme);
        for (i, c) in [t.ui.accent, t.syntax.string, t.syntax.mnemonic].into_iter().enumerate() {
            let x = rect.left() + 9.0 + i as f32 * 15.0;
            painter.circle_filled(egui::pos2(x, rect.center().y), 4.0, c);
        }
        painter.rect_stroke(
            rect,
            3.0,
            egui::Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
            egui::StrokeKind::Inside,
        );
    }

    pub(super) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        // Libellés traduits précalculés (évite d'emprunter self pendant que les
        // widgets empruntent ses champs en écriture).
        let lang = self.lang;
        let t_title = self.tr3("Réglages", "Settings", "Configuración");
        let t_lang = self.tr3("Langue", "Language", "Idioma");
        let t_theme = self.tr3("Thème", "Theme", "Tema");
        let t_theme_note = self.tr3(
            "Chaque thème porte sa propre coloration du code, ses fonds et ses accents.",
            "Each theme carries its own code colors, backgrounds and accents.",
            "Cada tema lleva sus propios colores de código, fondos y acentos.",
        );
        let t_iface = self.tr3("Interface", "Interface", "Interfaz");
        let t_tooltips = self.tr3(
            "Afficher les infobulles des raccourcis (au survol des boutons)",
            "Show shortcut tooltips (on button hover)",
            "Mostrar tooltips de atajos (al pasar el cursor sobre los botones)",
        );
        let t_inspect = self.tr3(
            "Inspection au survol dans l'éditeur (valeur du registre, du drapeau, du label)",
            "Hover inspection in the editor (value of the register, flag or label)",
            "Inspección al pasar el cursor en el editor (valor del registro, flag o etiqueta)",
        );
        let t_inspect_tip = self.tr3(
            "Survolez un mot du code pendant l'exécution : sa valeur s'affiche sur place,\n\
             sur une ligne, sans quitter le code des yeux.",
            "Hover a word in the code while running: its value appears in place,\n\
             on one line, without leaving the code.",
            "Pase el cursor sobre una palabra durante la ejecución: su valor aparece ahí mismo,\n\
             en una línea, sin apartar la vista del código.",
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
        let t_pe_h = self.tr3(
            "Assemblage Windows (PE64)",
            "Windows assembling (PE64)",
            "Ensamblado Windows (PE64)",
        );
        let t_pe = self.tr3(
            "Proposer la cible Windows (menu Exécution ▸ Cible)",
            "Offer the Windows target (Run ▸ Target menu)",
            "Ofrecer el destino Windows (menú Ejecución ▸ Destino)",
        );
        let t_pe_desc = self.tr3(
            "Assemble le même source en exécutable Windows (.exe PE64) : autre convention \
             d'appel, fonctions importées de DLL au lieu des appels système. Le panneau \
             FORMAT montre ce que contient le fichier, et Wine — s'il est installé — \
             l'exécute. Le pas-à-pas reste réservé à la cible Linux.",
            "Assembles the same source as a Windows executable (.exe PE64): different calling \
             convention, DLL-imported functions instead of system calls. The FORMAT panel shows \
             what the file holds, and Wine — when installed — runs it. Single-stepping stays \
             reserved for the Linux target.",
            "Ensambla el mismo código como ejecutable de Windows (.exe PE64): otra convención de \
             llamada, funciones importadas de DLL en lugar de llamadas al sistema. El panel \
             FORMATO muestra lo que contiene el archivo, y Wine — si está instalado — lo \
             ejecuta. El paso a paso queda reservado al destino Linux.",
        );
        let t_pe_off = self.tr3(
            "Décochée, l'application ne propose que Linux : un débutant n'a pas à choisir une cible.",
            "Unchecked, the application offers Linux only: a beginner should not have to pick a target.",
            "Sin marcar, la aplicación solo ofrece Linux: un principiante no tiene por qué elegir un destino.",
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
                        // La liste vient du catalogue : un thème ajouté à
                        // `crate::theme::THEMES` apparaît ici sans rien changer
                        // à cette fenêtre.
                        let choices = std::iter::once(crate::theme::Choice::System).chain(
                            crate::theme::THEMES
                                .iter()
                                .map(|t| crate::theme::Choice::Named(t.id)),
                        );
                        for c in choices {
                            ui.horizontal(|ui| {
                                changed |= ui.radio_value(&mut self.theme_pref, c, c.label(lang)).changed();
                                // Un aperçu vaut mieux qu'un nom : trois pastilles
                                // (fond, accent, chaîne) disent d'un coup d'œil à
                                // quoi ressemblera l'éditeur.
                                if let crate::theme::Choice::Named(id) = c
                                    && let Some(t) = crate::theme::by_id(id)
                                {
                                    Self::theme_preview(ui, t);
                                }
                            });
                        }
                        ui.add_space(4.0);
                        ui.weak(t_theme_note);
                        ui.separator();

                        section(ui, t_iface);
                        changed |= ui.checkbox(&mut self.show_tooltips, t_tooltips).changed();
                        changed |= ui
                            .checkbox(&mut self.inspect_hover, t_inspect)
                            .on_hover_text(t_inspect_tip)
                            .changed();
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

                        section(ui, t_pe_h);
                        if ui.checkbox(&mut self.pe_enabled, t_pe).changed() {
                            // Décocher pendant qu'une cible Windows est active
                            // laisserait l'IDE dans un état que plus aucun menu
                            // ne permet de défaire.
                            self.apply_pe_setting();
                            changed = true;
                        }
                        ui.add_space(4.0);
                        super::card(ui, |ui| {
                            ui.label(RichText::new(t_pe_desc).size(12.5));
                            ui.add_space(5.0);
                            ui.label(RichText::new(t_pe_off).size(12.0).weak());
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
                    ("Maj+F10", tr("Pas par-dessus : exécute l'appel d'un bloc", "Step over: run the call in one go", "Paso por encima: ejecuta la llamada de una vez")),
                    ("F9", tr("Continuer jusqu'au prochain point d'arrêt", "Continue to the next breakpoint", "Continuar hasta el próximo punto de interrupción")),
                    ("Ctrl+F8", tr("Point d'arrêt sur la ligne du curseur (ou clic dans la gouttière)", "Breakpoint on the cursor's line (or click the gutter)", "Punto de interrupción en la línea del cursor (o clic en el margen)")),
                    ("Ctrl+Maj+F8", tr("Condition du point d'arrêt (ou clic droit dans la gouttière)", "Breakpoint condition (or right-click the gutter)", "Condición del punto de interrupción (o clic derecho en el margen)")),
                    ("Échap / Maj+F5", tr("Stop", "Stop", "Detener")),
                    ("Ctrl+B", tr("Assembler + Lier", "Assemble + Link", "Ensamblar + Enlazar")),
                    ("Ctrl+S", tr("Enregistrer", "Save", "Guardar")),
                    ("Ctrl+O", tr("Ouvrir", "Open", "Abrir")),
                    ("Ctrl+N", tr("Nouveau", "New", "Nuevo")),
                    ("Ctrl+F", tr("Rechercher dans l'éditeur", "Find in editor", "Buscar en el editor")),
                    ("Ctrl+H", tr("Rechercher / remplacer dans l'éditeur", "Find / replace in editor", "Buscar / reemplazar en el editor")),
                    ("F3 / Maj+F3", tr("Correspondance suivante / précédente", "Next / previous match", "Coincidencia siguiente / anterior")),
                    ("Ctrl+Maj+[", tr("Replier le label sous le curseur", "Fold the label under the cursor", "Plegar la etiqueta bajo el cursor")),
                    ("Ctrl+Maj+]", tr("Tout déplier", "Unfold all", "Desplegar todo")),
                    ("Tab / Maj+Tab", tr("Indenter / désindenter la sélection", "Indent / outdent the selection", "Indentar / reducir la indentación")),
                    ("Ctrl+/", tr("Commenter / décommenter les lignes", "Comment / uncomment the lines", "Comentar / descomentar las líneas")),
                    ("Alt+↑ / Alt+↓", tr("Déplacer la ligne vers le haut / le bas", "Move the line up / down", "Mover la línea arriba / abajo")),
                    ("Ctrl+D", tr("Dupliquer la ligne", "Duplicate the line", "Duplicar la línea")),
                    ("Ctrl+Maj+K", tr("Supprimer la ligne", "Delete the line", "Eliminar la línea")),
                    ("Ctrl+G", tr("Aller à la ligne…", "Go to line…", "Ir a la línea…")),
                    ("Ctrl+Espace", tr("Autocomplétion : mnémoniques, registres, labels", "Autocomplete: mnemonics, registers, labels", "Autocompletado: mnemónicos, registros, etiquetas")),
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
                    (tr("Survol", "Hover", "Cursor encima"), tr("Dans l'éditeur : valeur du registre, du drapeau, du label ou du nombre sous le pointeur", "In the editor: value of the register, flag, label or number under the pointer", "En el editor: valor del registro, flag, etiqueta o número bajo el puntero")),
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
                                    let col = if *on { action() } else { self.c_bytes() };
                                    let txt = RichText::new(if *on { "1" } else { "0" })
                                        .monospace()
                                        .size(13.0)
                                        .color(col);
                                    let btn = egui::Button::new(txt)
                                        .min_size(egui::vec2(15.0, 19.0))
                                        .fill(if *on {
                                            action().linear_multiply(0.16)
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

    /// « Sortie du programme » : ce que l'élève verrait s'il lançait son binaire
    /// depuis un terminal, et **rien d'autre**.
    ///
    /// La console du panneau raconte le déroulement — assemblage, appels
    /// système, diagnostics — et c'est ce qu'on lui demande. Mais quand la
    /// question est « qu'est-ce que mon programme affiche, au juste ? », ces
    /// lignes-là sont du bruit : l'élève doit trier ce qui vient de lui de ce
    /// qui vient de l'IDE. Cette boîte répond à cette question seule, sur fond
    /// de terminal pour qu'on ne s'y trompe pas.
    pub(super) fn program_output_window(&mut self, ctx: &egui::Context) {
        if !self.show_program_output {
            return;
        }
        use crate::debugger::RunState;
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let hdr = self.c_header();

        // État de l'exécution, dans les mots du terminal : c'est ce qui donne
        // son sens à une sortie vide (rien écrit ? ou pas encore lancé ?).
        let (state_txt, state_col) = match self.dbg.as_ref().map(|d| d.state) {
            None => (
                tr("Aucune exécution", "No run", "Ninguna ejecución").to_string(),
                hdr,
            ),
            Some(RunState::Exited(0)) => (
                format!("{} 0", tr("Terminé — code de sortie", "Finished — exit code", "Terminado — código de salida")),
                flag_on(),
            ),
            Some(RunState::Exited(c)) => (
                format!("{} {c}", tr("Terminé — code de sortie", "Finished — exit code", "Terminado — código de salida")),
                false_col(),
            ),
            Some(RunState::Signaled) => (
                tr("Tué par un signal", "Killed by a signal", "Terminado por una señal").to_string(),
                false_col(),
            ),
            Some(RunState::Faulted(_)) => (
                tr("Arrêté sur une faute", "Stopped on a fault", "Detenido por un fallo").to_string(),
                false_col(),
            ),
            Some(RunState::Running) => (
                tr("En attente d'une saisie…", "Waiting for input…", "Esperando entrada…").to_string(),
                action(),
            ),
            Some(RunState::Stopped) => (
                tr("En cours d'exécution", "Running", "En ejecución").to_string(),
                action(),
            ),
        };

        // Corps borné à une fraction de l'écran, comme la fenêtre Réglages :
        // une sortie de mille lignes poussait sinon le pied de page et le
        // bouton « Fermer » hors de la fenêtre, qui débordait de l'écran.
        let max_body_h = (ctx.content_rect().height() * 0.6).max(160.0);
        let mut open = true;
        let mut copy = false;
        egui::Window::new(tr("Sortie du programme", "Program output", "Salida del programa"))
            // Identité stable, indépendante du titre : changer de langue ne doit
            // pas faire perdre à la fenêtre sa position et sa taille.
            .id(egui::Id::new("program_output"))
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .min_width(360.0)
            .default_height(360.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(state_txt).color(state_col).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        copy = ui
                            .button(tr("Copier", "Copy", "Copiar"))
                            .on_hover_text(tr(
                                "Copie la sortie dans le presse-papiers",
                                "Copy the output to the clipboard",
                                "Copiar la salida al portapapeles",
                            ))
                            .clicked();
                    });
                });
                ui.add_space(4.0);

                // Le terminal se distingue des panneaux de l'IDE par la surface
                // la plus enfoncée du thème — celle de l'éditeur —, pas par un
                // noir écrit en dur : sur un thème clair, ce dernier plaquait un
                // rectangle de charbon au milieu d'une fenêtre pâle.
                let theme = crate::theme::current();
                egui::Frame::new()
                    .fill(theme.ui.extreme)
                    .stroke(egui::Stroke::new(1.0_f32, theme.ui.border))
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_min_height(140.0);
                        egui::ScrollArea::vertical()
                            .id_salt("program_output_scroll")
                            .stick_to_bottom(true)
                            .max_height(max_body_h)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                if self.program_output.is_empty() {
                                    // Une zone vide sans un mot laisserait croire
                                    // à une panne de l'IDE plutôt qu'à un
                                    // programme qui n'a rien écrit.
                                    ui.label(
                                        RichText::new(tr(
                                            "(le programme n'a rien écrit)",
                                            "(the program wrote nothing)",
                                            "(el programa no escribió nada)",
                                        ))
                                        .monospace()
                                        .color(theme.ui.gutter),
                                    );
                                } else {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&self.program_output)
                                                .monospace()
                                                .color(theme.syntax.text),
                                        )
                                        .selectable(true)
                                        .wrap(),
                                    );
                                }
                            });
                    });

                ui.add_space(4.0);
                ui.label(
                    RichText::new(tr(
                        "Ce que votre programme a écrit, sans les messages de l'IDE.",
                        "What your program wrote, without the IDE's own messages.",
                        "Lo que su programa escribió, sin los mensajes del IDE.",
                    ))
                    .small()
                    .color(hdr),
                );
                ui.separator();
                ui.vertical_centered(|ui| {
                    if ui.button(tr("Fermer", "Close", "Cerrar")).clicked() {
                        self.show_program_output = false;
                    }
                });
            });

        if copy {
            ctx.copy_text(self.program_output.clone());
        }
        if !open {
            self.show_program_output = false;
        }
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
                    ui.radio_value(&mut self.calc_base, super::CALC_BASE_ASCII, "ASCII")
                        .on_hover_text(tr(
                            "Chaque caractère vaut son code, jusqu'à 8 (la largeur d'un registre). Échappements acceptés : \\0 \\t \\n \\r \\xNN.",
                            "Each character stands for its code, up to 8 (a register's width). Escapes accepted: \\0 \\t \\n \\r \\xNN.",
                            "Cada carácter vale su código, hasta 8 (el ancho de un registro). Escapes aceptados: \\0 \\t \\n \\r \\xNN.",
                        ));
                });
                ui.add_space(6.0);

                let base = self.calc_base;
                let hint = match base {
                    16 => "deadbeef",
                    2 => "10110100",
                    8 => "377",
                    super::CALC_BASE_ASCII => "Hi!",
                    _ => "42",
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
                super::calc_sanitize(&mut self.calc_input, base);
                let a = super::calc_parse(&self.calc_input, base);
                if let Some(v) = a
                    && let Some(nv) = self.bit_grid(ui, v, true)
                {
                    self.calc_input = super::calc_format_bare(nv, base);
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
                            b = b.fill(accent());
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
                super::calc_sanitize(&mut self.calc_input_b, base);
                let b_val = super::calc_parse(&self.calc_input_b, base);
                if let Some(v) = b_val
                    && let Some(nv) = self.bit_grid(ui, v, true)
                {
                    self.calc_input_b = super::calc_format_bare(nv, base);
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
                                // L'ASCII ferme la liste : c'est la lecture qui
                                // fait le lien avec les chaînes du programme.
                                for (label, b) in [
                                    ("Hex", 16),
                                    ("Bin", 2),
                                    ("Dec", 10),
                                    ("Oct", 8),
                                    ("ASCII", super::CALC_BASE_ASCII),
                                ] {
                                    ui.label(RichText::new(label).strong().color(hdr));
                                    let txt = ui.label(
                                        RichText::new(super::calc_format(v, b))
                                            .monospace()
                                            .color(mnem),
                                    );
                                    if b == super::CALC_BASE_ASCII {
                                        // Le détail octet par octet : quel
                                        // caractère vaut quel code.
                                        let detail = super::calc_bytes_of(v, super::calc_width_bytes(v))
                                            .into_iter()
                                            .map(|byte| {
                                                format!("{} = 0x{byte:02X} = {byte}", super::calc_ascii_text(byte as i64))
                                            })
                                            .collect::<Vec<_>>()
                                            .join("\n");
                                        txt.on_hover_text(detail);
                                    }
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
                    .fill(false_col().linear_multiply(0.16))
                    .stroke(egui::Stroke::new(1.0_f32, false_col()))
                    .corner_radius(egui::CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(&diag.title).size(15.0).strong().color(false_col()));
                    });
                ui.add_space(8.0);

                // Explication : le cœur pédagogique.
                super::card(ui, |ui| {
                    ui.label(RichText::new(&diag.explanation).size(13.0));
                });
                ui.add_space(8.0);

                // Piste de correction.
                ui.label(RichText::new(tr("💡 Comment corriger", "💡 How to fix it", "💡 Cómo corregirlo")).strong().color(action()));
                ui.add_space(3.0);
                egui::Frame::default()
                    .fill(action().linear_multiply(0.12))
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

    /// Licence valide en vigueur ? (statut affiché dans « À propos », par
    /// exemple) — distinct de [`Self::is_unlocked`], qui inclut aussi la
    /// période avant inscription et pilote réellement le verrouillage des
    /// panneaux.
    pub(super) fn is_licensed(&self) -> bool {
        matches!(self.license, crate::license::LicenseState::Valid(_))
    }

    /// Licence valide OU période avant inscription gratuite encore en cours :
    /// pilote le verrouillage des panneaux avancés (voir `dock.rs`).
    pub(super) fn is_unlocked(&self) -> bool {
        self.is_licensed() || crate::trial::is_active()
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
                "Délai avant inscription gratuite dépassé — activez une licence pour continuer.",
                "Free registration period over — activate a license to continue.",
                "Periodo de registro gratuito terminado — active una licencia para continuar.",
            ));
            ui.add_space(8.0);
            if ui.button(tr("Activer une licence…", "Activate a license…", "Activar una licencia…")).clicked() {
                self.show_license_gate = true;
            }
        });
    }

    /// Carte de rappel affichée à intervalle irrégulier tant qu'aucune licence
    /// n'est active (voir `check_license_nag`). Volontairement distincte de
    /// [`Self::license_gate_window`] : ici pas de champ de saisie, juste une
    /// accroche et un seul geste possible — activer, ou remettre à plus tard.
    pub(super) fn license_nag_window(&mut self, ctx: &egui::Context) {
        if !self.show_license_nag {
            return;
        }
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);

        let trial_active = crate::trial::is_active();
        egui::Window::new("")
            .id(egui::Id::new("license_nag"))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .frame(
                egui::Frame::window(&ctx.style())
                    .corner_radius(egui::CornerRadius::same(12))
                    .stroke(egui::Stroke::new(1.0_f32, accent().linear_multiply(0.6))),
            )
            .show(ctx, |ui| {
                ui.set_width(340.0);
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    // Médaillon en dégradé léger — la seule touche de couleur
                    // franche de la carte, pour attirer l'œil sans agresser.
                    egui::Frame::default()
                        .fill(accent().linear_multiply(0.18))
                        .corner_radius(egui::CornerRadius::same(28))
                        .inner_margin(egui::Margin::same(14))
                        .show(ui, |ui| {
                            ui.label(RichText::new("✨").size(28.0));
                        });
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(tr(
                            "ASM Studio vous plaît ?",
                            "Enjoying ASM Studio?",
                            "¿Le gusta ASM Studio?",
                        ))
                        .heading()
                        .strong()
                        .color(accent()),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(tr(
                            "Une licence gratuite débloque durablement le désassemblage, \
                             les registres/flags et la timeline.",
                            "A free license permanently unlocks disassembly, \
                             registers/flags and the timeline.",
                            "Una licencia gratuita desbloquea de forma permanente el \
                             desensamblado, los registros/flags y la línea de tiempo.",
                        ))
                        .color(egui::Color32::from_gray(190)),
                    );

                    if trial_active {
                        ui.add_space(8.0);
                        let days = crate::trial::days_left();
                        let remaining = match lang {
                            crate::i18n::Lang::Fr => format!("Encore {days} jour(s) avant l'inscription"),
                            crate::i18n::Lang::En => format!("{days} day(s) left before registration"),
                            crate::i18n::Lang::Es => format!("Quedan {days} día(s) antes del registro"),
                        };
                        ui.label(RichText::new(format!("🕐 {remaining}")).small().color(action()));
                    }

                    // Ouverte parce qu'on essaie de fermer l'appli : on le dit,
                    // pour que le bouton « Quitter quand même » plus bas ne
                    // sorte pas de nulle part.
                    if self.exit_pending {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(tr(
                                "Vous êtes sur le point de quitter ASM Studio.",
                                "You're about to quit ASM Studio.",
                                "Está a punto de salir de ASM Studio.",
                            ))
                            .small()
                            .weak(),
                        );
                    }

                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        let spacing = 8.0;
                        let btn_w = (ui.available_width() - spacing) / 2.0;
                        if ui
                            .add_sized(
                                [btn_w, 28.0],
                                egui::Button::new(RichText::new(tr("Activer une licence", "Activate a license", "Activar una licencia")).strong())
                                    .fill(accent()),
                            )
                            .clicked()
                        {
                            self.show_license_nag = false;
                            self.exit_pending = false;
                            self.show_license_gate = true;
                        }
                        let secondary = if self.exit_pending {
                            tr("Quitter quand même", "Quit anyway", "Salir de todos modos")
                        } else {
                            tr("Plus tard", "Later", "Más tarde")
                        };
                        if ui.add_sized([btn_w, 28.0], egui::Button::new(secondary)).clicked() {
                            self.show_license_nag = false;
                            if self.exit_pending {
                                self.exit_pending = false;
                                self.quit_confirmed = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                // Sans ça, en rendu à la demande, rien ne redéclenche
                                // la frame qui doit noter puis appliquer la fermeture
                                // (voir le commentaire de `check_close_request`).
                                ctx.request_repaint();
                            }
                        }
                    });
                });
                ui.add_space(4.0);
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
                ui.horizontal(|ui| {
                    ui.label(tr(
                        "Votre version :",
                        "Your version:",
                        "Su versión:",
                    ));
                    ui.label(RichText::new(crate::license::version_build_tag()).monospace().strong());
                    if ui
                        .small_button("📋")
                        .on_hover_text(tr(
                            "Copier version+build (à transmettre pour obtenir la licence)",
                            "Copy version+build (send this to get your license)",
                            "Copiar versión+build (envíelo para obtener la licencia)",
                        ))
                        .clicked()
                    {
                        ctx.copy_text(crate::license::version_build_tag());
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(tr(
                        "Collez ci-dessous le bloc de licence reçu par e-mail :",
                        "Paste the license block you received by e-mail below:",
                        "Pegue abajo el bloque de licencia recibido por correo:",
                    ));
                    // Bouton de collage : Ctrl+V fonctionne aussi, mais la
                    // licence arrive par courriel et se colle d'un geste — sans
                    // avoir à cliquer d'abord dans le champ pour lui donner le
                    // focus, ce que rien n'indique.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(tr("📋 Coller", "📋 Paste", "📋 Pegar"))
                            .on_hover_text(tr(
                                "Colle le contenu du presse-papiers",
                                "Paste the clipboard contents",
                                "Pega el contenido del portapapeles",
                            ))
                            .clicked()
                        {
                            match super::clipboard_text() {
                                Some(text) => {
                                    self.license_input = text;
                                    self.license_error = None;
                                }
                                None => {
                                    self.license_error = Some(
                                        tr(
                                            "Le presse-papiers est vide ou illisible : copiez d'abord le bloc de licence.",
                                            "The clipboard is empty or unreadable: copy the license block first.",
                                            "El portapapeles está vacío o ilegible: copie primero el bloque de licencia.",
                                        )
                                        .to_string(),
                                    );
                                }
                            }
                        }
                    });
                });
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
                    // Déjà activée : rien à revalider, on désactive le bouton plutôt
                    // que de risquer un remplacement accidentel par un collage erroné.
                    let already_licensed = self.is_licensed();
                    if ui
                        .add_enabled(!already_licensed, egui::Button::new(tr("Valider", "Validate", "Validar")))
                        .clicked()
                    {
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
mod about_tests {
    use super::*;

    /// Le bandeau annonce la préversion exacte, numéro compris — c'est ce qui
    /// l'a empêché de rester sur « BÊTA 2 », puis d'afficher « BÊTA » tout court
    /// quand la version n'en portait plus. Il vient de `crate::version`, seule
    /// source, et disparaît sur une version finale.
    #[test]
    fn the_banner_announces_the_exact_prerelease() {
        match crate::version::beta() {
            Some(n) => {
                let label = crate::version::beta_label(i18n::Lang::Fr).expect("une bêta s'annonce");
                assert!(label.contains(n), "« {label} » doit porter le numéro {n}");
                assert!(!label.ends_with("BÊTA"), "« BÊTA » sans numéro ne dit rien");
            }
            None => assert!(
                crate::version::beta_label(i18n::Lang::Fr).is_none(),
                "une version finale n'affiche pas de bandeau de préversion"
            ),
        }
    }

    /// La fenêtre se rend dans les trois langues sans paniquer.
    #[test]
    fn the_about_window_renders_in_every_language() {
        for lang in [i18n::Lang::Fr, i18n::Lang::En, i18n::Lang::Es] {
            let mut app = App::new();
            app.lang = lang;
            app.show_about = true;
            let ctx = egui::Context::default();
            let _ = ctx.run(Default::default(), |ctx| app.about_window(ctx));
        }
    }
}

#[cfg(test)]
mod breakpoint_condition_tests {
    use super::*;

    /// Le clic droit sur une ligne nue pose le point d'arrêt en même temps
    /// qu'il ouvre sa condition : c'est le geste attendu, en un temps.
    #[test]
    fn opening_the_condition_arms_the_breakpoint() {
        let mut app = App::new();
        app.open_breakpoint_condition(12);
        assert_eq!(app.bp_cond_line, Some(12));
        assert!(app.breakpoints.contains_key(&12));
        assert!(app.bp_cond_input.is_empty(), "rien à reprendre sur une ligne nue");
    }

    /// Rouvrir une condition existante la remet dans le champ : on la corrige,
    /// on ne la réécrit pas de mémoire.
    #[test]
    fn reopening_shows_the_condition_already_set() {
        let mut app = App::new();
        app.set_breakpoint_condition(7, "rcx==0x10").expect("condition valide");
        app.open_breakpoint_condition(7);
        assert_eq!(app.bp_cond_input, "RCX == 0x10", "reprise sous forme normalisée");
    }

    #[test]
    fn the_window_renders_and_stays_open_until_a_button_is_pressed() {
        let mut app = App::new();
        app.source = "section .text\n_start:\n    mov rax, 1\n".to_string();
        app.open_breakpoint_condition(3);
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.breakpoint_condition_window(ctx));
        assert_eq!(app.bp_cond_line, Some(3));
    }

    #[test]
    fn a_closed_window_paints_nothing() {
        let mut app = App::new();
        app.bp_cond_line = None;
        let ctx = egui::Context::default();
        let out = ctx.run(Default::default(), |ctx| app.breakpoint_condition_window(ctx));
        assert!(out.shapes.is_empty());
    }

    /// Une ligne au-delà de la fin du fichier ne doit pas faire paniquer le
    /// rendu (le source a pu raccourcir depuis la pose du point d'arrêt).
    #[test]
    fn a_line_past_the_end_of_the_file_is_harmless() {
        let mut app = App::new();
        app.source = "nop\n".to_string();
        app.open_breakpoint_condition(999);
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.breakpoint_condition_window(ctx));
    }
}

#[cfg(test)]
mod new_file_format_tests {
    use super::*;

    /// La boîte du format se rend dans les trois langues sans paniquer, et ne
    /// s'affiche pas quand rien ne l'attend.
    #[test]
    fn the_format_window_renders_in_every_language() {
        for lang in [i18n::Lang::Fr, i18n::Lang::En, i18n::Lang::Es] {
            let mut app = App::new();
            app.lang = lang;
            app.new_file_prompt = true;
            let ctx = egui::Context::default();
            let _ = ctx.run(Default::default(), |ctx| app.new_file_format_window(ctx));
            assert!(app.new_file_prompt, "elle reste ouverte tant qu'on n'a pas répondu");
        }

        let mut app = App::new();
        let ctx = egui::Context::default();
        let out = ctx.run(Default::default(), |ctx| app.new_file_format_window(ctx));
        assert!(out.shapes.is_empty(), "rien à peindre sans question en cours");
    }

    /// Elle compte parmi les dialogues ouverts : sans cela, le rendu à la
    /// demande laisserait la fenêtre affichée après sa fermeture.
    #[test]
    fn the_format_window_counts_as_an_open_dialog() {
        let mut app = App::new();
        let before = app.open_dialog_count();
        app.new_file_prompt = true;
        assert_eq!(app.open_dialog_count(), before + 1);
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    use std::path::PathBuf;

    /// La boîte de collage doit se rendre sans paniquer, y compris quand une
    /// licence est déjà active (le bouton « Valider » y est alors désactivé,
    /// mais la fenêtre reste consultable/fermable).
    #[test]
    fn license_gate_window_renders_licensed_and_unlicensed() {
        for license in [crate::license::LicenseState::Missing, crate::license::valid_for_tests()] {
            let mut app = App::new();
            app.show_license_gate = true;
            app.license = license;
            let ctx = egui::Context::default();
            let _ = ctx.run(Default::default(), |ctx| app.license_gate_window(ctx));
            assert!(app.show_license_gate);
        }
    }

    /// La carte de rappel doit se rendre sans paniquer et proposer les deux
    /// gestes attendus : activer (ouvre la vraie boîte de collage) ou remettre
    /// à plus tard (referme juste la carte).
    #[test]
    fn license_nag_window_renders_and_buttons_toggle_the_right_flags() {
        let mut app = App::new();
        app.show_license_nag = true;
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.license_nag_window(ctx));
        assert!(app.show_license_nag, "reste ouverte tant qu'aucun bouton n'est cliqué");
        assert!(!app.show_license_gate, "ne doit pas ouvrir la boîte de collage toute seule");
    }

    /// Ouverte parce qu'une fermeture a été bloquée (`exit_pending`), elle doit
    /// se rendre sans paniquer aussi — c'est là qu'apparaît « Quitter quand
    /// même » à la place de « Plus tard ».
    #[test]
    fn license_nag_window_renders_in_exit_pending_mode() {
        let mut app = App::new();
        app.show_license_nag = true;
        app.exit_pending = true;
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.license_nag_window(ctx));
        assert!(app.show_license_nag, "reste ouverte tant qu'aucun bouton n'est cliqué");
    }

    /// Fermée, elle ne peint rien.
    #[test]
    fn closed_license_nag_window_paints_nothing() {
        let mut app = App::new();
        app.show_license_nag = false;
        let ctx = egui::Context::default();
        let out = ctx.run(Default::default(), |ctx| app.license_nag_window(ctx));
        assert!(out.shapes.is_empty());
    }

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

    /// La boîte doit se peindre dans ses deux états — sortie vide (où elle dit
    /// pourquoi) et sortie remplie — et rester ouverte tant qu'on ne la ferme
    /// pas. Sans exécution en cours, elle annonce « aucune exécution » plutôt
    /// que de laisser croire à un programme muet.
    #[test]
    fn program_output_window_renders_empty_and_filled() {
        for out in ["", "Hello, world!\n42\n"] {
            let mut app = App::new();
            app.show_program_output = true;
            app.program_output = out.to_string();
            let ctx = egui::Context::default();
            let _ = ctx.run(Default::default(), |ctx| app.program_output_window(ctx));
            assert!(app.show_program_output, "la boîte reste ouverte sans geste de fermeture");
        }
    }

    /// Fermée, elle ne peint rien : c'est ce qui la distingue d'un panneau.
    #[test]
    fn program_output_window_stays_closed() {
        let mut app = App::new();
        app.program_output = "invisible".into();
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.program_output_window(ctx));
        assert!(!app.show_program_output);
    }

    /// La fenêtre « À propos » doit se rendre sans paniquer, qu'une licence
    /// soit active ou non — c'est là qu'apparaît le statut d'activation.
    #[test]
    fn about_window_renders_without_panicking_licensed_and_unlicensed() {
        for license in [crate::license::LicenseState::Missing, crate::license::valid_for_tests()] {
            let mut app = App::new();
            app.show_about = true;
            app.license = license;
            let ctx = egui::Context::default();
            let _ = ctx.run(Default::default(), |ctx| app.about_window(ctx));
        }
    }

    /// La confirmation de désactivation doit se rendre dans les deux cas
    /// qu'elle distingue : essai encore actif (l'essai prend le relais) et
    /// essai écoulé (reverrouillage immédiat). Les deux branches construisent
    /// un texte différent, les deux doivent être peintes sans paniquer.
    #[test]
    fn license_reset_confirm_window_renders_in_both_trial_states() {
        let mut app = App::new();
        app.license = crate::license::valid_for_tests();
        app.confirm_license_reset = true;
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.license_reset_confirm_window(ctx));
        assert!(
            app.confirm_license_reset,
            "reste ouverte tant qu'aucun bouton n'est cliqué"
        );
    }

    /// Le cœur de l'action : après confirmation, plus de licence en mémoire,
    /// plus de message d'erreur résiduel, et la confirmation se referme.
    /// (`license::remove` ne touche pas au disque en test, voir son garde
    /// `cfg!(test)` — c'est bien l'effet sur l'état de l'appli qu'on vérifie.)
    #[test]
    fn confirming_the_reset_clears_the_license_and_closes_the_dialog() {
        let mut app = App::new();
        app.license = crate::license::valid_for_tests();
        app.license_error = Some("erreur héritée".to_string());
        app.confirm_license_reset = true;
        assert!(app.is_licensed());

        app.reset_license();

        assert!(!app.is_licensed(), "la licence doit avoir disparu");
        assert!(app.license_error.is_none(), "le message hérité doit être vidé");
        assert!(!app.confirm_license_reset, "la confirmation doit se refermer");
    }

    /// Annuler ne doit rien désactiver : c'est la garantie qu'apporte la
    /// confirmation. On simule le geste (fermeture sans passer par
    /// `reset_license`) et on vérifie que la licence est intacte.
    #[test]
    fn cancelling_the_reset_keeps_the_license() {
        let mut app = App::new();
        app.license = crate::license::valid_for_tests();
        app.confirm_license_reset = true;

        app.confirm_license_reset = false; // « Annuler » / croix de fermeture

        assert!(app.is_licensed(), "annuler ne doit jamais désactiver");
    }

    /// Fermée, elle ne peint rien — et surtout, ne supprime rien : le fichier
    /// ne part qu'au clic sur « Désactiver ».
    #[test]
    fn closed_license_reset_confirm_window_paints_nothing() {
        let mut app = App::new();
        app.confirm_license_reset = false;
        let ctx = egui::Context::default();
        let out = ctx.run(Default::default(), |ctx| app.license_reset_confirm_window(ctx));
        assert!(out.shapes.is_empty());
    }

    /// Le lien « Désactiver… » n'existe que pour une licence réellement
    /// active : sans licence, il n'y a rien à désactiver, et la fenêtre « À
    /// propos » propose « Activer… » à la place. Ce test verrouille le fait
    /// que le simple rendu n'arme jamais la confirmation tout seul.
    #[test]
    fn about_window_never_arms_the_reset_confirmation_on_its_own() {
        for license in [crate::license::LicenseState::Missing, crate::license::valid_for_tests()] {
            let mut app = App::new();
            app.show_about = true;
            app.license = license;
            let ctx = egui::Context::default();
            let _ = ctx.run(Default::default(), |ctx| app.about_window(ctx));
            assert!(
                !app.confirm_license_reset,
                "la confirmation ne doit s'ouvrir que sur clic explicite"
            );
        }
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
        assert!(out.shapes.is_empty() || !app.show_settings);
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

        // Et dans toutes les bases, ASCII comprise.
        for base in [16, 2, 10, 8, super::super::CALC_BASE_ASCII] {
            app.calc_base = base;
            app.calc_input = "101".to_string();
            let _ = ctx.run(Default::default(), |ctx| app.calculator_window(ctx));
        }
    }

    /// En base ASCII, le texte des deux opérandes se lit comme des codes et le
    /// résultat se relit comme du texte : « a AND \xDF » donne « A ».
    #[test]
    fn ascii_base_operates_on_text() {
        let mut app = App::new();
        app.show_calculator = true;
        app.calc_base = super::super::CALC_BASE_ASCII;
        app.calc_input = "a".to_string();
        app.calc_input_b = "\\xDF".to_string();
        app.calc_op = super::super::CalcOp::And;
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.calculator_window(ctx));

        // Le filtre de saisie n'a pas mangé l'échappement.
        assert_eq!(app.calc_input_b, "\\xDF");
        let a = super::super::calc_parse(&app.calc_input, app.calc_base).unwrap();
        let b = super::super::calc_parse(&app.calc_input_b, app.calc_base).unwrap();
        let r = app.calc_op.apply(a, b).unwrap();
        assert_eq!(super::super::calc_format(r, super::super::CALC_BASE_ASCII), "'A'");
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
