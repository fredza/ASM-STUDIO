//! Application eframe : éditeur NASM + débogueur pédagogique.
//!
//! Layout : menu / barre d'outils en haut, barre d'état + bande
//! (Mémoire | Timeline | Console) en bas, Registres/Flags à gauche,
//! Instruction + Pile à droite, éditeur/désassemblage au centre (onglets).
//! L'état affiché est lu dans l'historique du debugger à `view_index`.

use std::collections::HashMap;
use std::path::PathBuf;

use eframe::egui::{self, Color32};

use crate::debugger::{Debugger, Snapshot};
use crate::disasm::Insn;
use crate::i18n::{self, Lang};

mod file_ops;
mod debug_ops;
mod ui_chrome;
mod ui_windows;
mod ui_panels;
mod ui_center;
mod pedagogy;
mod widgets;
mod paths;
mod parse;

// Remontés dans `app` pour que les modules d'UI gardent leurs
// `use super::{card, parse_hex, …}` : un module enfant voit les imports privés
// de son parent, inutile de les rendre publics.
use widgets::*;
use paths::*;
use parse::*;

use crate::updater::Updater;

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

#[derive(PartialEq, Clone, Copy, Debug)]
pub(super) enum Tab {
    Editor,
    Disasm,
    MemMap,
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
    /// Mode pédagogique — animations enrichies (flèches, fondu directionnel).
    pub(super) pedagogy_anim: bool,
    /// Mode pédagogique — vue mémoire unifiée registres→zones pointées.
    pub(super) pedagogy_memview: bool,
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
    /// Dialogue « Ouvrir » natif en cours sur un thread de fond (sinon `None`).
    /// Sondé chaque frame → l'UI ne se fige pas pendant que le sélecteur est ouvert.
    pub(super) updater: Updater,
    pub(super) pending_open: Option<std::sync::mpsc::Receiver<Option<PathBuf>>>,
    /// Dialogue « Enregistrer sous » natif en cours sur un thread de fond.
    pub(super) pending_saveas: Option<std::sync::mpsc::Receiver<Option<PathBuf>>>,
}

impl App {
    pub fn new() -> Self {
        setup_examples();
        let src_path = data_dir().join("examples").join("hello_world.asm");
        let source = std::fs::read_to_string(&src_path).unwrap_or_else(|_| {
            "section .text\n    global _start\n_start:\n    mov rax, 60\n    xor rdi, rdi\n    syscall\n"
                .to_string()
        });
        let explorer_dir = abs_dir_of(&src_path);
        let out_dir = data_dir().join("build");
        let mut app = App {
            src_path,
            out_dir,
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
            pedagogy_anim: false,
            pedagogy_memview: false,
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
            updater: Updater::new(),
            pending_open: None,
            pending_saveas: None,
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
                "pedagogy_anim" => self.pedagogy_anim = v == "true",
                "pedagogy_memview" => self.pedagogy_memview = v == "true",
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
             pedagogy_anim={}\npedagogy_memview={}\n\
             show_explorer={}\nshow_instruction={}\nshow_cpu_band={}\nshow_bottom_band={}\n",
            self.lang.key(),
            self.show_tooltips,
            self.use_asmstd,
            self.animate,
            self.pedagogy_anim,
            self.pedagogy_memview,
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

    pub(super) fn tr3(&self, fr: &'static str, en: &'static str, es: &'static str) -> &'static str {
        i18n::tr3(self.lang, fr, en, es)
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
        // Workaround egui#5008 : sur Linux, egui-winit active l'IME quand un TextEdit
        // a le focus mais ignore ensuite les événements IME → les dead keys (accents)
        // sont avalés. On désactive l'IME : xkbcommon compose les accents directement
        // dans les événements clavier sans passer par le protocole IME.
        ctx.send_viewport_cmd(egui::ViewportCommand::IMEAllowed(false));
        self.apply_theme(ctx);
        // Récupère le résultat d'un dialogue fichier natif (thread de fond) et,
        // tant qu'il est ouvert, force un repaint périodique pour continuer à sonder.
        self.poll_file_dialogs();
        if self.dialog_pending() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
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
        self.update_window(ctx);
        self.updater.poll();
    }
}

// ---------- Helpers ----------
































#[cfg(test)]
mod tests {
    use super::*;

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
