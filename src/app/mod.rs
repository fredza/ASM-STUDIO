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
mod dock;
mod palette;
mod predict;
mod ui_exercise;
mod ui_tutorial;
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

/// Id stable de la zone de texte de l'éditeur : permet d'y renvoyer le focus
/// clavier depuis n'importe où (F6), sans passer par la souris.
pub(super) fn editor_id() -> egui::Id {
    egui::Id::new("kb_editor")
}

/// Champs de saisie de l'application, avec le panneau auquel ils appartiennent.
///
/// Les flèches doivent piloter le panneau focalisé, SAUF si l'utilisateur tape
/// DANS CE panneau — auquel cas elles déplacent le curseur du texte.
///
/// Deux formulations plus simples ont été essayées et sont fausses :
/// « un widget quelconque a-t-il le focus ? » condamnait toute la navigation
/// dès qu'on avait cliqué une fois dans « aller @ » (egui garde ce focus
/// indéfiniment) ; « un champ de saisie a-t-il le focus ? » avait le même
/// défaut, à peine atténué. Seule l'appartenance au panneau focalisé tranche.
pub(super) fn text_inputs() -> [(egui::Id, dock::Panel); 4] {
    [
        (editor_id(), dock::Panel::Editor),
        (egui::Id::new("kb_mem_goto"), dock::Panel::Memory),
        (egui::Id::new("kb_mem_poke"), dock::Panel::Memory),
        (egui::Id::new("kb_reg_edit"), dock::Panel::Registers),
    ]
}

impl App {
    /// Vrai si l'utilisateur saisit du texte dans le panneau qui a le focus.
    pub(super) fn typing_in_focused_panel(&mut self, ctx: &egui::Context) -> bool {
        let Some(f) = ctx.memory(|m| m.focused()) else { return false };
        // Le champ de la prédiction vit dans une fenêtre flottante, hors de
        // l'arbre : dès qu'il a le focus, il garde les flèches.
        if f == egui::Id::new("kb_pred_input") {
            return true;
        }
        let focused = self.focused_panel();
        text_inputs()
            .iter()
            .any(|(id, panel)| *id == f && Some(*panel) == focused)
    }
}

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

/// Mode d'affichage de l'interface.
///
/// L'application montre par défaut tout ce qu'un débogueur peut montrer, ce qui
/// est écrasant quand on découvre l'assembleur. Le mode apprentissage réduit
/// l'écran à ce qui sert les premières semaines : le code, ce que fait
/// l'instruction courante, les registres, la pile et la sortie du programme.
#[derive(PartialEq, Eq, Clone, Copy, Debug, Default)]
pub(crate) enum UiMode {
    /// Panneaux essentiels, registres généraux seulement.
    #[default]
    Learning,
    /// Tous les panneaux et tous les registres.
    Full,
}

impl UiMode {
    pub(crate) fn key(self) -> &'static str {
        match self {
            UiMode::Learning => "learning",
            UiMode::Full => "full",
        }
    }
    pub(crate) fn from_key(k: &str) -> UiMode {
        match k {
            "full" => UiMode::Full,
            _ => UiMode::Learning,
        }
    }
    pub(crate) fn label(self, lang: Lang) -> &'static str {
        match self {
            UiMode::Learning => i18n::tr3(lang, "Apprentissage", "Learning", "Aprendizaje"),
            UiMode::Full => i18n::tr3(lang, "Complet", "Full", "Completo"),
        }
    }
    pub(crate) fn description(self, lang: Lang) -> &'static str {
        match self {
            UiMode::Learning => i18n::tr3(
                lang,
                "L'essentiel : code, instruction expliquée, registres généraux, pile, console.",
                "The essentials: code, explained instruction, general registers, stack, console.",
                "Lo esencial: código, instrucción explicada, registros generales, pila, consola.",
            ),
            UiMode::Full => i18n::tr3(
                lang,
                "Tout : désassemblage, vue mémoire, vidage hexa, pile d'appels, appels système.",
                "Everything: disassembly, memory view, hex dump, call stack, system calls.",
                "Todo: desensamblado, vista de memoria, volcado hexa, pila de llamadas, syscalls.",
            ),
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
pub(super) enum StackTab {
    Stack,
    Heap,
}

/// Icônes de l'app (planche `src/Assets`, découpées dans `assets/icons/`),
/// chargées une fois comme textures egui.
///
/// Plusieurs icônes ont disparu avec les en-têtes de panneau : la barre
/// d'onglets nomme chaque panneau, un pictogramme répété dans le corps n'était
/// qu'une redite. Ne restent que celles des boutons et des rares en-têtes qui
/// portent des contrôles.
pub(super) struct Icons {
    pub(super) assembler: egui::TextureHandle,
    pub(super) run: egui::TextureHandle,
    pub(super) debug: egui::TextureHandle,
    pub(super) stack: egui::TextureHandle,
    pub(super) heap: egui::TextureHandle,
    // Icônes complémentaires (même thème, générées) — boutons et panneaux.
    pub(super) stop: egui::TextureHandle,
    pub(super) restart: egui::TextureHandle,
    pub(super) memory: egui::TextureHandle,
    pub(super) console: egui::TextureHandle,
}

impl Icons {
    pub(super) fn load(ctx: &egui::Context) -> Self {
        macro_rules! ic {
            ($name:literal) => {
                load_texture(ctx, $name, include_bytes!(concat!("../../assets/icons/", $name, ".png")))
            };
        }
        Icons {
            assembler: ic!("assembler"),
            run: ic!("run"),
            debug: ic!("debug"),
            stack: ic!("stack"),
            heap: ic!("heap"),
            stop: ic!("stop"),
            restart: ic!("restart"),
            memory: ic!("memory"),
            console: ic!("console"),
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
    /// Fichier retenu au clavier dans l'explorateur (surligné, ouvert par Entrée).
    pub(super) explorer_selected: Option<PathBuf>,
    pub(super) view_index: usize,

    pub(super) mem_addr: u64,
    pub(super) mem_input: String,
    /// Octets hexa à écrire en mémoire (laboratoire mémoire).
    pub(super) mem_poke: String,
    /// Registre retenu au clavier dans le panneau REGISTERS (index dans
    /// `Registers::named`), surligné et éditable par Entrée.
    pub(super) reg_sel: usize,
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
    pub(super) stack_tab: StackTab,
    /// Thème sombre actif (mis à jour dans `apply_theme`) — palette de texte.
    pub(super) dark: bool,
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
    /// Panneau dont la sélection vient de bouger au clavier : sa zone défilante
    /// doit amener l'élément retenu à l'écran. Sans cela, les flèches
    /// déplaçaient une sélection qui sortait du cadre visible.
    pub(super) scroll_to_sel: Option<dock::Panel>,
    /// Parcours guidé : activable, avec sa progression et la leçon ouverte.
    pub(super) tutorial_enabled: bool,
    pub(super) tutorial_progress: crate::tutorial::Progress,
    pub(super) tutorial_current: Option<String>,
    /// Mode d'affichage : apprentissage (épuré) ou complet.
    pub(super) mode: UiMode,
    /// Palette de commandes (Ctrl+Maj+P) : ouverte, requête, sélection.
    pub(super) palette_open: bool,
    pub(super) palette_query: String,
    pub(super) palette_sel: usize,
    /// Demande de focus du champ de recherche au premier frame d'ouverture.
    pub(super) palette_focus: bool,
    /// Nom du panneau focalisé, affiché dans la barre d'état. Renseigné par le
    /// rendu de la zone d'ancrage, qui seul connaît le nœud actif.
    pub(super) focused_panel_name: Option<String>,
    /// Demande de relâchement du focus widget au prochain frame : quand le
    /// clavier quitte l'éditeur, ce dernier ne doit plus capter les touches.
    pub(super) ctx_surrender_focus: bool,
    /// Arbre des panneaux ancrables. `Option` car il est sorti de `self` le
    /// temps du rendu : le `TabViewer` a besoin de `&mut App`.
    pub(super) dock: Option<egui_dock::DockState<dock::Panel>>,
    /// Mode pédagogique — prédire la valeur d'un registre avant chaque pas.
    pub(super) pedagogy_predict: bool,
    /// Prédiction en cours (en attente de résolution, ou résolue et affichée).
    pub(super) prediction: Option<predict::Prediction>,
    /// Compteur de réussite des prédictions.
    pub(super) pred_score: predict::Score,
    /// Registre visé par la prochaine prédiction.
    pub(super) pred_reg: &'static str,
    /// Saisie hexa de la valeur prédite.
    pub(super) pred_input: String,
    /// Énoncé et attentes extraits du source courant (vide si ce n'est pas un
    /// exercice). Recalculé à chaque chargement/enregistrement du fichier.
    pub(super) exercise: crate::exercise::Exercise,
    /// Résultat de la dernière vérification, à la sortie du programme.
    pub(super) checks: Vec<crate::exercise::Check>,
    /// Diagnostic de la faute courante (plantage), s'il y en a un.
    /// Recalculé au moment de la faute, effacé au (re)lancement.
    pub(super) diagnosis: Option<crate::diagnostic::Diagnosis>,
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
            explorer_selected: None,
            view_index: 0,
            mem_addr: 0,
            mem_input: String::new(),
            mem_poke: String::new(),
            reg_sel: 0,
            edit_reg: None,
            edit_buf: String::new(),
            edit_focus: false,
            console: String::new(),
            status: String::new(),
            editor_scroll_y: 0.0,
            editor_ln: 1,
            editor_col: 1,
            stack_tab: StackTab::Stack,
            dark: true,
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
            // Activé par défaut : un nouveau venu démarre en mode apprentissage
            // et voit d'emblée le parcours guidé, au lieu d'un écran de
            // débogueur nu. Les utilisateurs déjà installés gardent leur choix,
            // que le fichier de réglages restitue par-dessus ce défaut.
            tutorial_enabled: true,
            tutorial_progress: crate::tutorial::Progress::default(),
            tutorial_current: None,
            scroll_to_sel: None,
            mode: UiMode::Learning,
            palette_open: false,
            palette_query: String::new(),
            palette_sel: 0,
            palette_focus: false,
            focused_panel_name: None,
            ctx_surrender_focus: false,
            dock: Some(dock::learning_layout()),
            pedagogy_predict: false,
            prediction: None,
            pred_score: predict::Score::default(),
            pred_reg: "RAX",
            pred_input: String::new(),
            exercise: crate::exercise::Exercise::default(),
            checks: Vec::new(),
            diagnosis: None,
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
        // Les tests ne doivent pas dépendre de la configuration de la machine :
        // sinon leur résultat change selon la langue choisie par l'utilisateur.
        if cfg!(test) {
            return;
        }
        let Some(path) = settings_path() else { return };
        let Ok(content) = std::fs::read_to_string(&path) else { return };
        // La disposition est appliquée en dernier : elle dépend des autres
        // réglages (langue pour les titres) et remplace l'arbre par défaut.
        let mut saved_dock: Option<String> = None;
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
                "mode" => self.mode = UiMode::from_key(v),
                "tutorial" => self.tutorial_enabled = v == "true",
                "tutorial_done" => self.tutorial_progress = crate::tutorial::Progress::parse(v),
                "tutorial_current" => {
                    self.tutorial_current = (!v.is_empty()).then(|| v.to_string())
                }
                "tooltips" => self.show_tooltips = v == "true",
                "asmstd" => self.use_asmstd = v == "true",
                "animate" => self.animate = v == "true",
                "pedagogy_anim" => self.pedagogy_anim = v == "true",
                "pedagogy_memview" => self.pedagogy_memview = v == "true",
                "pedagogy_predict" => self.pedagogy_predict = v == "true",
                "dock" => saved_dock = Some(v.to_string()),
                _ => {}
            }
        }
        match saved_dock {
            Some(layout) => self.apply_dock_layout(&layout),
            // Pas de disposition enregistrée : celle du mode relu, sinon un
            // réglage `mode=full` afficherait la disposition d'apprentissage.
            None => self.dock = Some(dock::layout_for(self.mode)),
        }
    }

    pub(super) fn save_settings(&self) {
        use egui::ThemePreference;
        // Et surtout, ils ne doivent RIEN écrire dedans : plusieurs exécutent des
        // commandes qui persistent (changement de langue, fermeture de panneau),
        // ce qui modifiait pour de bon les réglages de l'utilisateur.
        if cfg!(test) {
            return;
        }
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
            "theme={theme}\nlang={}\nmode={}\ntooltips={}\nasmstd={}\nanimate={}\n\
             pedagogy_anim={}\npedagogy_memview={}\npedagogy_predict={}\n\
             tutorial={}\ntutorial_done={}\ntutorial_current={}\n\
             dock={}\n",
            self.lang.key(),
            self.mode.key(),
            self.show_tooltips,
            self.use_asmstd,
            self.animate,
            self.pedagogy_anim,
            self.pedagogy_memview,
            self.pedagogy_predict,
            self.tutorial_enabled,
            self.tutorial_progress.to_string(),
            self.tutorial_current.clone().unwrap_or_default(),
            self.dock_layout_string(),
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

    /// Vrai une seule fois si ce panneau doit amener sa sélection à l'écran.
    ///
    /// La demande est nominative : consommer sans vérifier le destinataire
    /// ferait absorber par le premier panneau rendu un défilement destiné à un
    /// autre.
    pub(super) fn take_scroll_request(&mut self, panel: dock::Panel) -> bool {
        if self.scroll_to_sel == Some(panel) {
            self.scroll_to_sel = None;
            true
        } else {
            false
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
            // Une seule fois, au premier frame : recharger les polices à chaque
            // image coûterait cher.
            Self::install_fallback_font(ctx);
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

        // Toute la zone centrale est un arbre de panneaux ancrables : chaque
        // panneau est un onglet que l'on déplace, empile ou détache en fenêtre.
        self.dock_ui(ctx);

        self.about_window(ctx);
        self.shortcuts_window(ctx);
        self.settings_window(ctx);
        self.microscope_window(ctx);
        self.calculator_window(ctx);
        self.palette_window(ctx);
        self.predict_window(ctx);
        self.diagnosis_window(ctx);
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
    /// Un plantage doit produire un diagnostic affichable, et la fenêtre doit se
    /// rendre sans paniquer. C'est le remplacement de l'ancien « Terminé (signal) ».
    #[test]
    fn crash_produces_renderable_diagnosis() {
        let mut app = App::new();
        app.src_path = PathBuf::from("build/crash-test.asm");
        app.out_dir = PathBuf::from("build/crash");
        app.source = "section .text\n global _start\n_start:\n xor rax, rax\n \
                       mov rbx, [rax]\n mov rax,60\n xor rdi,rdi\n syscall\n"
            .to_string();

        app.launch();
        assert!(app.dbg.is_some(), "le programme doit être lancé");
        for _ in 0..6 {
            app.step();
        }

        let diag = app.diagnosis.as_ref().expect("un diagnostic doit être produit");
        assert_eq!(diag.cause, crate::diagnostic::Cause::NullPointer);
        assert!(!diag.title.is_empty() && !diag.hint.is_empty());
        // La barre d'état ne dit plus juste « signal ».
        assert!(app.status.contains(&diag.title), "statut = {}", app.status);

        // La fenêtre se rend sans paniquer.
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.diagnosis_window(ctx));
        assert!(app.diagnosis.is_some(), "la fenêtre reste ouverte tant qu'on ne ferme pas");
    }

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
