//! Application eframe : éditeur NASM + débogueur pédagogique.
//!
//! Layout : menu / barre d'outils en haut, barre d'état + bande
//! (Mémoire | Timeline | Console) en bas, Registres/Flags à gauche,
//! Instruction + Pile à droite, éditeur/désassemblage au centre (onglets).
//! L'état affiché est lu dans l'historique du debugger à `view_index`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, RichText};

use crate::debugger::{Debugger, Snapshot};
use crate::disasm::Insn;
use crate::i18n::{self, Lang};
use crate::explain;

mod file_ops;
mod debug_ops;
mod ui_chrome;
mod ui_windows;
mod ui_panels;
mod ui_center;

// --- Palette ---
pub(super) const ACCENT: Color32 = Color32::from_rgb(0x4C, 0x8B, 0xF5); // bleu d'accent
pub(super) const ACTION: Color32 = Color32::from_rgb(0xE8, 0x8A, 0x2E); // orange d'action (Run/Step)
pub(super) const HEADER: Color32 = Color32::from_rgb(0x8A, 0x9B, 0xB4); // titres de section
pub(super) const CHANGED: Color32 = Color32::from_rgb(0xF5, 0xA6, 0x23); // valeur modifiée
pub(super) const FLAG_ON: Color32 = Color32::from_rgb(0x5F, 0xBF, 0x69);
pub(super) const FLAG_OFF: Color32 = Color32::from_rgb(0x77, 0x77, 0x80);
pub(super) const RIP_ROW: Color32 = Color32::from_rgb(0x3A, 0x33, 0x1E);
pub(super) const SEL_ROW: Color32 = Color32::from_rgb(0x2E, 0x2E, 0x38);
pub(super) const ADDR_COL: Color32 = Color32::from_rgb(0x7F, 0x9C, 0xD1);
pub(super) const BYTES_COL: Color32 = Color32::from_rgb(0x80, 0x80, 0x88);
pub(super) const MNEMONIC: Color32 = Color32::from_rgb(0x6E, 0xB4, 0xE8);
pub(super) const FALSE_COL: Color32 = Color32::from_rgb(0xD9, 0x5B, 0x5B);

// Couleur de la gouttière de numéros de ligne.
pub(super) const GUTTER: Color32 = Color32::from_rgb(0x60, 0x66, 0x70);
// Animation « CPU vivant ».
pub(super) const FLASH_DUR: f64 = 0.7; // durée du fondu (secondes)
pub(super) const FLASH_BRIGHT: Color32 = Color32::from_rgb(0xFF, 0xF2, 0x9A); // pic de pulsation
pub(super) const PUSH_COL: Color32 = Color32::from_rgb(0x5F, 0xBF, 0x69);
pub(super) const POP_COL: Color32 = Color32::from_rgb(0xE0, 0x8A, 0x3C);

/// Interpolation linéaire entre deux couleurs (t ∈ [0,1]).
pub(super) fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// Couleur d'une valeur modifiée, pulsant du clair vers `CHANGED` selon `flash`.
pub(super) fn changed_color(flash: Option<f32>) -> Color32 {
    changed_color2(flash, CHANGED)
}

/// Comme [`changed_color`] mais vers une couleur de base arbitraire.
pub(super) fn changed_color2(flash: Option<f32>, base: Color32) -> Color32 {
    match flash {
        Some(p) => lerp_color(FLASH_BRIGHT, base, p),
        None => base,
    }
}

#[derive(PartialEq, Clone, Copy)]
pub(super) enum Tab {
    Editor,
    Disasm,
}

#[derive(PartialEq, Clone, Copy)]
pub(super) enum StackTab {
    Stack,
    Heap,
}

/// Icônes de l'app (planche `src/Assets`, découpées dans `assets/icons/`),
/// chargées une fois comme textures egui.
pub(super) struct Icons {
    pub(super) editor: egui::TextureHandle,
    pub(super) assembler: egui::TextureHandle,
    pub(super) run: egui::TextureHandle,
    pub(super) debug: egui::TextureHandle,
    pub(super) registers: egui::TextureHandle,
    pub(super) stack: egui::TextureHandle,
    pub(super) heap: egui::TextureHandle,
    // Icônes complémentaires (même thème, générées) — boutons et panneaux.
    pub(super) stop: egui::TextureHandle,
    pub(super) pause: egui::TextureHandle,
    pub(super) restart: egui::TextureHandle,
    pub(super) attach: egui::TextureHandle,
    pub(super) memory: egui::TextureHandle,
    pub(super) timeline: egui::TextureHandle,
    pub(super) console: egui::TextureHandle,
    pub(super) syscalls: egui::TextureHandle,
    pub(super) callstack: egui::TextureHandle,
    pub(super) explorer: egui::TextureHandle,
    pub(super) instruction: egui::TextureHandle,
}

impl Icons {
    pub(super) fn load(ctx: &egui::Context) -> Self {
        macro_rules! ic {
            ($name:literal) => {
                load_texture(ctx, $name, include_bytes!(concat!("../../assets/icons/", $name, ".png")))
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
pub(super) struct SyscallLog {
    pub(super) name: String,
    pub(super) args: String,
    pub(super) number: u64,
    pub(super) ret: Option<i64>,
}

pub struct App {
    pub(super) src_path: PathBuf,
    pub(super) out_dir: PathBuf,
    /// Contenu de l'éditeur (source NASM en cours d'édition).
    pub(super) source: String,
    /// Modifications non enregistrées.
    pub(super) dirty: bool,
    pub(super) binary: Option<PathBuf>,

    pub(super) dbg: Option<Debugger>,
    pub(super) disasm: Vec<Insn>,
    /// Mapping adresse → ligne source (1-based) pour le suivi dans l'éditeur.
    pub(super) src_map: HashMap<u64, usize>,
    pub(super) selected: Option<u64>,
    /// Instruction ouverte dans le mode « microscope » (fenêtre dédiée).
    pub(super) microscope: Option<u64>,
    /// Appels système exécutés (panneau SYSCALLS).
    pub(super) syscalls: Vec<SyscallLog>,
    /// Adresses des frames actives (panneau CALL STACK), suivi call/ret.
    pub(super) call_stack: Vec<u64>,
    /// Dossier affiché dans l'explorateur de fichiers (panneau de gauche).
    pub(super) explorer_dir: PathBuf,
    pub(super) view_index: usize,

    pub(super) mem_addr: u64,
    pub(super) mem_input: String,
    /// Octets hexa à écrire en mémoire (laboratoire mémoire).
    pub(super) mem_poke: String,
    /// Registre en cours d'édition (laboratoire mémoire) et son tampon de saisie.
    pub(super) edit_reg: Option<&'static str>,
    pub(super) edit_buf: String,
    /// Demande de focus au premier frame d'édition d'un registre.
    pub(super) edit_focus: bool,
    pub(super) console: String,
    pub(super) status: String,
    /// Décalage vertical de l'éditeur (pour synchroniser la gouttière).
    pub(super) editor_scroll_y: f32,
    /// Position du curseur dans l'éditeur (1-based), pour la barre d'état.
    pub(super) editor_ln: usize,
    pub(super) editor_col: usize,

    pub(super) tab: Tab,
    pub(super) stack_tab: StackTab,
    /// Thème sombre actif (mis à jour dans `apply_theme`) — palette de texte.
    pub(super) dark: bool,
    /// Visibilité des panneaux (menu Affichage).
    pub(super) show_explorer: bool,
    pub(super) show_instruction: bool,
    pub(super) show_cpu_band: bool,
    pub(super) show_bottom_band: bool,
    pub(super) show_tooltips: bool,
    /// Animations « CPU vivant » (pulsation des valeurs modifiées au Step).
    pub(super) animate: bool,
    /// Rend `asmstd.inc` disponible partout (ajoute son dossier aux includes nasm).
    pub(super) use_asmstd: bool,
    /// Instant (temps egui) du dernier Step, pour animer le fondu.
    pub(super) flash_time: f64,
    /// Un Step vient d'avoir lieu : mémorise l'instant au prochain frame.
    pub(super) pending_flash: bool,
    pub(super) theme_pref: egui::ThemePreference,
    /// Langue de l'interface (Réglages).
    pub(super) lang: Lang,
    pub(super) show_settings: bool,
    pub(super) show_about: bool,
    pub(super) show_shortcuts: bool,
    pub(super) show_calculator: bool,
    /// Saisie de la calculatrice multi-base (texte brut, parsé selon `calc_base`).
    pub(super) calc_input: String,
    /// Base d'entrée de la calculatrice : 2, 8, 10 ou 16.
    pub(super) calc_base: u32,
    /// Icônes (chargées au premier frame, quand le contexte egui existe).
    pub(super) icons: Option<Icons>,
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
            status: String::new(),
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
            lang: Lang::Fr,
            show_settings: false,
            show_about: false,
            show_shortcuts: false,
            show_calculator: false,
            calc_input: String::new(),
            calc_base: 10,
            icons: None,
        };
        app.load_settings();
        app
    }

    // ---------- Persistance des réglages ----------

    pub(super) fn load_settings(&mut self) {
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
                "lang" => self.lang = Lang::from_key(v),
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

    pub(super) fn save_settings(&self) {
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
            "theme={theme}\nlang={}\ntooltips={}\nasmstd={}\nanimate={}\n\
             show_explorer={}\nshow_instruction={}\nshow_cpu_band={}\nshow_bottom_band={}\n",
            self.lang.key(),
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

    // ---------- Accès à l'état affiché ----------

    /// Progression de l'animation « CPU vivant » : `Some(0.0..=1.0)` pendant la
    /// pulsation (0 = juste après le Step, 1 = fin), `None` sinon. Demande un
    /// repaint tant que ça anime.
    pub(super) fn flash_progress(&self, ui: &egui::Ui) -> Option<f32> {
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

    pub(super) fn snap(&self) -> Option<&Snapshot> {
        let d = self.dbg.as_ref()?;
        d.history.get(self.view_index.min(d.history.len().saturating_sub(1)))
    }
    pub(super) fn prev_snap(&self) -> Option<&Snapshot> {
        let d = self.dbg.as_ref()?;
        let i = self.view_index.min(d.history.len().saturating_sub(1));
        d.history.get(i.saturating_sub(1))
    }
    pub(super) fn is_head_view(&self) -> bool {
        matches!(&self.dbg, Some(d) if self.view_index >= d.history.len() - 1)
    }
    pub(super) fn can_step(&self) -> bool {
        self.dbg.as_ref().is_some_and(|d| d.is_alive()) && self.is_head_view()
    }
    pub(super) fn can_read_memory(&self) -> bool {
        self.is_head_view() && self.dbg.as_ref().is_some_and(|d| d.is_alive())
    }
    pub(super) fn view_rip(&self) -> Option<u64> {
        self.snap().map(|s| s.regs.rip)
    }

    /// Ligne source (0-based) correspondant à RIP courant, pour l'éditeur.
    pub(super) fn current_source_line(&self) -> Option<usize> {
        let rip = self.view_rip()?;
        self.src_map.get(&rip).map(|l| l.saturating_sub(1))
    }

    /// États (avant, après) de l'exécution de l'instruction à `addr`, retrouvés
    /// dans l'historique. `after` est `None` si l'instruction n'a pas encore été
    /// exécutée (ou est la dernière étape). Utilisé par le mode microscope.
    pub(super) fn microscope_states(&self, addr: u64) -> Option<(&Snapshot, Option<&Snapshot>)> {
        let d = self.dbg.as_ref()?;
        let i = d.history.iter().position(|s| s.regs.rip == addr)?;
        Some((&d.history[i], d.history.get(i + 1)))
    }
    pub(super) fn set_view(&mut self, idx: i64) {
        if let Some(d) = &self.dbg {
            self.view_index = idx.clamp(0, (d.history.len() - 1) as i64) as usize;
        }
    }

    /// Ajoute une infobulle (raccourci) seulement si l'option est activée.
    pub(super) fn tip(&self, resp: egui::Response, text: &str) -> egui::Response {
        if self.show_tooltips {
            resp.on_hover_text(text)
        } else {
            resp
        }
    }

    /// Traduit selon la langue courante : `tr(français, anglais)`.
    pub(super) fn tr(&self, fr: &'static str, en: &'static str) -> &'static str {
        i18n::tr(self.lang, fr, en)
    }

    // ---------- Palette de texte sensible au thème ----------
    // En thème clair, les couleurs « sombres » de la maquette deviennent
    // illisibles : on renvoie des variantes plus foncées.

    /// Couleur des titres de section / libellés secondaires.
    pub(super) fn c_header(&self) -> Color32 {
        if self.dark { HEADER } else { Color32::from_rgb(0x3B, 0x4A, 0x63) }
    }
    /// Couleur des mnémoniques / accents bleus.
    pub(super) fn c_mnemonic(&self) -> Color32 {
        if self.dark { MNEMONIC } else { Color32::from_rgb(0x1B, 0x5E, 0xA8) }
    }
    /// Couleur des adresses.
    pub(super) fn c_addr(&self) -> Color32 {
        if self.dark { ADDR_COL } else { Color32::from_rgb(0x2A, 0x53, 0x86) }
    }
    /// Couleur des octets bruts / texte discret monospace.
    pub(super) fn c_bytes(&self) -> Color32 {
        if self.dark { BYTES_COL } else { Color32::from_rgb(0x60, 0x64, 0x70) }
    }
    /// Fond de la ligne RIP dans le désassemblage.
    pub(super) fn c_rip_row(&self) -> Color32 {
        if self.dark { RIP_ROW } else { Color32::from_rgb(0xFF, 0xEE, 0xB0) }
    }
    /// Fond d'une ligne sélectionnée / survolée dans le désassemblage.
    pub(super) fn c_sel_row(&self) -> Color32 {
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
        self.calculator_window(ctx);
    }
}

// ---------- Helpers ----------

/// Hauteur fixe de la ligne d'en-tête d'un panneau, pour aligner les
/// séparateurs de tous les panneaux au même niveau (certains en-têtes ont des
/// boutons/combos plus hauts qu'un simple libellé).
pub(super) const HEADER_H: f32 = 24.0;

/// En-tête de panneau à hauteur fixe : rend `content` (titre + éventuels
/// contrôles) dans une ligne de `HEADER_H`, puis un séparateur. Tous les
/// panneaux passent par ici → leurs séparateurs sont alignés.
pub(super) fn panel_header(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(2.0);
    // Bandeau de titre teinté (style dashboard) : remplace l'ancien séparateur ;
    // hauteur constante ⇒ tous les bandeaux restent alignés d'un panneau à l'autre.
    egui::Frame::none()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::symmetric(6.0, 2.0))
        .rounding(egui::Rounding::same(5.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), HEADER_H - 4.0),
                egui::Layout::left_to_right(egui::Align::Center),
                content,
            );
        });
    ui.add_space(5.0);
}

/// Encadré « carte » moderne : fond légèrement teinté, coins arrondis et marge
/// interne, sur toute la largeur disponible. Structure et aère le contenu
/// (utile pour une app pédagogique).
pub(super) fn card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
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
pub(super) fn header_title(ui: &mut egui::Ui, hdr: Color32, icon: Option<&egui::TextureHandle>, text: &str) {
    icon_img(ui, icon, 15.0);
    ui.label(RichText::new(text).strong().color(hdr).size(12.5));
}

/// Titre de section simple (sans contrôle) à hauteur fixe.
pub(super) fn header(ui: &mut egui::Ui, hdr: Color32, text: &str) {
    panel_header(ui, |ui| header_title(ui, hdr, None, text));
}

/// En-tête de section avec une icône optionnelle à gauche du titre.
pub(super) fn header_icon(ui: &mut egui::Ui, hdr: Color32, icon: Option<&egui::TextureHandle>, text: &str) {
    panel_header(ui, |ui| header_title(ui, hdr, icon, text));
}

/// Affiche une petite icône carrée (rien si `icon` est `None`).
pub(super) fn icon_img(ui: &mut egui::Ui, icon: Option<&egui::TextureHandle>, size: f32) {
    if let Some(t) = icon {
        ui.add(egui::Image::new((t.id(), egui::vec2(size, size))));
    }
}

/// Alloue une colonne de largeur `w` et hauteur `h` puis y rend `add`.
pub(super) fn col(ui: &mut egui::Ui, w: f32, h: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(w, h),
        egui::Layout::top_down(egui::Align::Min),
        add,
    );
}

/// Petite colonne de pile (microscope) : adresse + valeur, à partir de `rsp`.
pub(super) fn micro_stack(ui: &mut egui::Ui, addr_c: Color32, label: &str, rsp: u64, stack: &[u64]) {
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
pub(super) fn micro_static_flags(ui: &mut egui::Ui, hdr: Color32, e: &explain::Explanation, set_label: &str, none_label: &str) {
    ui.add_space(4.0);
    if e.affects_flags.is_empty() {
        ui.weak(none_label);
    } else {
        ui.label(RichText::new(set_label).strong().color(hdr));
        ui.label(RichText::new(e.affects_flags.join("  ")).monospace().color(CHANGED));
    }
}

pub(super) fn parse_hex(s: &str) -> Option<u64> {
    let s = s.trim();
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u64::from_str_radix(s, 16).ok()
}

/// Analyse une valeur dans la base donnée (2, 8, 10 ou 16).
/// Base 10 : signé (`i64`), supporte le signe `-`. Autres bases : bit-pattern `u64` casté.
/// Renvoie `None` si vide ou hors plage.
pub(super) fn calc_parse(s: &str, base: u32) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if base == 10 {
        return s.parse::<i64>().ok();
    }
    u64::from_str_radix(s, base).ok().map(|v| v as i64)
}

/// Formate `v` dans la base donnée, avec préfixe (`0x`/`0o`/`0b`) sauf en base 10.
/// Hex/Oct/Bin : affiche le motif de bits en non-signé. Dec : affiche signé.
pub(super) fn calc_format(v: i64, base: u32) -> String {
    match base {
        16 => format!("0x{:X}", v as u64),
        8 => format!("0o{:o}", v as u64),
        2 => format!("0b{:b}", v as u64),
        _ => format!("{v}"),
    }
}

/// Analyse une suite d'octets hexadécimaux (« 48 65 6C » ou « 48656C »).
pub(super) fn parse_hex_bytes(s: &str) -> Option<Vec<u8>> {
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
pub(super) fn file_name(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

/// Entrées d'un dossier : (sous-dossiers, tous les fichiers), triés, en masquant
/// les entrées cachées (préfixe `.`). Pour l'explorateur en arbre.
pub(super) fn list_entries(dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
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
pub(super) fn is_asm(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "asm" || e == "s")
}

/// Rend récursivement l'arbre d'un dossier (style explorateur d'IDE) : dossiers
/// repliables (`CollapsingHeader`), puis fichiers cliquables. Le fichier ouvert
/// est surligné ; le clic sur un fichier renseigne `to_open`.
pub(super) fn dir_tree(
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
pub(super) fn bordered_button(
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
pub(super) fn icon_btn_widget(icon: Option<&egui::TextureHandle>, label: &'static str) -> egui::Button<'static> {
    match btn_icon(icon) {
        Some(img) => egui::Button::image_and_text(img, label),
        None => egui::Button::new(label),
    }
}

/// Source d'image dimensionnée pour un bouton (16px), à partir d'une icône.
pub(super) fn btn_icon(icon: Option<&egui::TextureHandle>) -> Option<egui::load::SizedTexture> {
    icon.map(|t| egui::load::SizedTexture::new(t.id(), egui::vec2(16.0, 16.0)))
}

/// Bouton d'accent (fond ACCENT si actif, grisé sinon) — pour Run et Step.
pub(super) fn accent_button(
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
pub(super) fn icon_button(ui: &mut egui::Ui, icon: Option<&egui::TextureHandle>, label: &str) -> egui::Response {
    match btn_icon(icon) {
        Some(img) => ui.add(egui::Button::image_and_text(img, label)),
        None => ui.button(label),
    }
}

/// Onglet sélectionnable avec l'icône DANS le bouton (respecte le padding).
/// Remplace `icon_img(...) + selectable_label(...)` où l'icône débordait.
pub(super) fn icon_tab(
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
pub(super) fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::default()
        .fill(color.linear_multiply(0.22))
        .inner_margin(egui::Margin::symmetric(5.0, 2.0))
        .rounding(egui::Rounding::same(4.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small().strong().color(color));
        });
}

/// Chemin du fichier de réglages persistants (XDG : ~/.config/asm_studio/settings.conf).
pub(super) fn settings_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("asm_studio").join("settings.conf"))
}

/// Répertoire contenant l'`asmstd.inc` fourni, s'il est trouvable.
pub(super) fn asmstd_dir() -> Option<PathBuf> {
    let dir = PathBuf::from("examples");
    dir.join("asmstd.inc").exists().then_some(dir)
}

/// Répertoire absolu contenant `path` (remonte à `current_dir` si besoin).
pub(super) fn abs_dir_of(path: &Path) -> PathBuf {
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

/// Affiche `rows` lignes de 16 octets (hex + ASCII) à partir de `base`.
pub(super) fn hex_dump_rows(ui: &mut egui::Ui, addr_c: Color32, bytes_c: Color32, dbg: &Debugger, base: u64, rows: u64) {
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
    fn calc_parse_reads_each_base() {
        assert_eq!(calc_parse("101", 2), Some(0b101));
        assert_eq!(calc_parse("777", 8), Some(0o777));
        assert_eq!(calc_parse("42", 10), Some(42));
        assert_eq!(calc_parse("dead", 16), Some(0xDEAD));
        assert_eq!(calc_parse("  ff  ", 16), Some(0xFF), "espaces tolérés");
        assert_eq!(calc_parse("", 10), None, "vide → None");
        assert_eq!(calc_parse("-42", 10), Some(-42), "décimal négatif supporté");
    }

    #[test]
    fn calc_format_roundtrips_with_prefix() {
        assert_eq!(calc_format(255, 10), "255");
        assert_eq!(calc_format(255, 16), "0xFF");
        assert_eq!(calc_format(255, 8), "0o377");
        assert_eq!(calc_format(255, 2), "0b11111111");
        // Aller-retour parse ∘ format (sans le préfixe, retiré par le filtre UI).
        let v = 0xCAFE;
        assert_eq!(calc_parse("CAFE", 16), Some(v));
        assert_eq!(calc_format(v, 16), "0xCAFE");
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
