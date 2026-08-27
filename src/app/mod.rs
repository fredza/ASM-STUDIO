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
mod edit_ops;
mod complete;
mod pedagogy;
mod dock;
mod palette;
mod predict;
mod inspect;
mod unsaved;
mod ui_exercise;
mod ui_tutorial;
mod widgets;
// `pub(crate)` : `license_path()` doit être atteignable depuis `crate::license`,
// hors de l'arbre de `app`.
pub(crate) mod paths;
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
pub(super) fn text_inputs() -> [(egui::Id, dock::Panel); 6] {
    [
        (editor_id(), dock::Panel::Editor),
        (egui::Id::new("kb_mem_goto"), dock::Panel::Memory),
        (egui::Id::new("kb_mem_poke"), dock::Panel::Memory),
        (egui::Id::new("kb_reg_edit"), dock::Panel::Registers),
        (find_query_id(), dock::Panel::Editor),
        (find_replace_id(), dock::Panel::Editor),
    ]
}

/// Id stable du champ de recherche de l'éditeur (Ctrl+F).
pub(super) fn find_query_id() -> egui::Id {
    egui::Id::new("kb_find_query")
}

/// Id stable du champ de remplacement de l'éditeur (Ctrl+H).
pub(super) fn find_replace_id() -> egui::Id {
    egui::Id::new("kb_find_replace")
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
//
// Ces couleurs ne sont plus des constantes mais des lectures du thème courant
// (voir [`crate::theme`]) : c'est ce qui permet d'en ajouter un sans repasser
// par ici. Le coût est une lecture atomique par appel, sur un chemin où chaque
// libellé en fait déjà une poignée — invisible à l'échelle d'une image.

/// Contenu texte du presse-papiers, s'il y en a un.
///
/// egui sait *écrire* dans le presse-papiers (`Context::copy_text`) mais pas le
/// lire : côté lecture, il ne transmet que l'événement de collage clavier, ce
/// qui ne permet pas de déclencher un collage depuis un bouton. On passe donc
/// par `arboard`, que eframe embarque déjà.
///
/// Rend `None` si le presse-papiers est vide, illisible, ou ne contient pas de
/// texte — trois cas où il n'y a rien à coller, et un seul message à montrer.
pub(super) fn clipboard_text() -> Option<String> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    clipboard.get_text().ok().filter(|s| !s.trim().is_empty())
}

/// Accent principal : liens, sélection, repère de la ligne courante.
pub(super) fn accent() -> Color32 {
    crate::theme::current().ui.accent
}
/// Accent d'action (Lancer / Pas à pas).
pub(super) fn action() -> Color32 {
    crate::theme::current().ui.action
}
/// Valeur qui vient de changer.
pub(super) fn changed_col() -> Color32 {
    crate::theme::current().ui.changed
}
pub(super) fn flag_on() -> Color32 {
    crate::theme::current().ui.ok
}
pub(super) fn flag_off() -> Color32 {
    crate::theme::current().ui.off
}
/// Erreur, condition fausse.
pub(super) fn false_col() -> Color32 {
    crate::theme::current().ui.error
}
/// Réserve, restriction : ce qui marche, mais pas partout ni jusqu'au bout.
pub(super) fn warn_col() -> Color32 {
    crate::theme::current().ui.warn
}
/// Couleur de la gouttière de numéros de ligne.
pub(super) fn gutter_col() -> Color32 {
    crate::theme::current().ui.gutter
}
/// Pastille de point d'arrêt, dans la gouttière.
pub(super) fn breakpoint_col() -> Color32 {
    crate::theme::current().ui.error
}
/// Pic de la pulsation « CPU vivant ».
pub(super) fn flash_bright() -> Color32 {
    crate::theme::current().ui.flash
}
pub(super) fn push_col() -> Color32 {
    crate::theme::current().ui.ok
}
pub(super) fn pop_col() -> Color32 {
    crate::theme::current().ui.warn
}

// Taille au-delà de laquelle la console est rognée par le début, et taille
// conservée après rognage. L'écart entre les deux est ce qui est jeté d'un
// coup : le prendre large espace les rognages, dont chacun recopie tout ce
// qui reste. Un demi-mégaoctet représente déjà plusieurs milliers de lignes,
// bien plus que ce qu'un élève relit.
pub(super) const CONSOLE_MAX: usize = 512 * 1024;
pub(super) const CONSOLE_KEEP: usize = 384 * 1024;
/// Longueur de la liste des fichiers récents. Dix tient dans un menu sans
/// défilement et couvre largement les allers-retours d'une séance de travail.
pub(super) const MAX_RECENT: usize = 10;
// Animation « CPU vivant ».
pub(super) const FLASH_DUR: f64 = 0.7; // durée du fondu (secondes)

// Squelettes de départ d'un nouveau fichier, un par format visé. Ils tiennent
// en quelques lignes : ce qu'il faut pour que le fichier s'assemble et se
// termine proprement, pas un programme d'exemple — celui-là est dans
// `examples/`. Chacun s'arrête à la frontière du format : `_start` + `syscall`
// pour ELF, `main` + `ExitProcess` pour PE.
pub(super) const SKELETON_ELF: &str = "section .data\n\nsection .text\n    global _start\n_start:\n    mov rax, 60      ; sys_exit\n    xor rdi, rdi     ; code 0\n    syscall\n";
pub(super) const SKELETON_PE_CONSOLE: &str = "bits 64\ndefault rel                 ; adressage relatif à RIP, comme sous Windows\n\nsection .data\n\nsection .text\n    global main\n    extern ExitProcess      ; kernel32.dll — le lieur l'inscrit dans les imports\n\nmain:\n    sub     rsp, 40         ; 32 d'espace d'ombre + 8 d'alignement, avant tout appel\n\n    xor     ecx, ecx        ; code de sortie 0\n    call    ExitProcess     ; ne revient jamais\n";
pub(super) const SKELETON_PE_GUI: &str = "bits 64\ndefault rel                 ; adressage relatif à RIP, comme sous Windows\n\nsection .data\n    titre   db \"ASM Studio\", 0\n    texte   db \"Bonjour depuis un PE64 !\", 0\n\nsection .text\n    global main\n    extern MessageBoxA      ; user32.dll\n    extern ExitProcess      ; kernel32.dll\n\nmain:\n    sub     rsp, 40         ; 32 d'espace d'ombre + 8 d'alignement\n\n    xor     ecx, ecx        ; 1er argument : aucune fenêtre parente\n    lea     rdx, [texte]    ; 2e : le message\n    lea     r8, [titre]     ; 3e : le titre\n    xor     r9d, r9d        ; 4e : MB_OK\n    call    MessageBoxA\n\n    xor     ecx, ecx        ; code de sortie 0\n    call    ExitProcess\n";

/// Interpolation linéaire entre deux couleurs (t ∈ [0,1]).
pub(super) fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// Couleur d'une valeur modifiée, pulsant du clair vers `changed_col()` selon `flash`.
pub(super) fn changed_color(flash: Option<f32>) -> Color32 {
    changed_color2(flash, changed_col())
}


/// Comme [`changed_color`] mais vers une couleur de base arbitraire.
pub(super) fn changed_color2(flash: Option<f32>, base: Color32) -> Color32 {
    match flash {
        Some(p) => lerp_color(flash_bright(), base, p),
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
    /// Espace d'écriture sans outils d'exécution : explorateur et éditeur.
    Editor,
    /// Tous les panneaux et tous les registres.
    Full,
}

impl UiMode {
    pub(crate) fn key(self) -> &'static str {
        match self {
            UiMode::Learning => "learning",
            UiMode::Editor => "editor",
            UiMode::Full => "full",
        }
    }
    pub(crate) fn from_key(k: &str) -> UiMode {
        match k {
            "editor" => UiMode::Editor,
            "full" => UiMode::Full,
            _ => UiMode::Learning,
        }
    }
    pub(crate) fn label(self, lang: Lang) -> &'static str {
        match self {
            UiMode::Learning => i18n::tr3(lang, "Apprentissage", "Learning", "Aprendizaje"),
            UiMode::Editor => i18n::tr3(lang, "Éditeur seul", "Editor only", "Solo editor"),
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
            UiMode::Editor => i18n::tr3(
                lang,
                "Écriture sans distraction : uniquement l'explorateur et l'éditeur.",
                "Distraction-free writing: only the explorer and editor.",
                "Escritura sin distracciones: solo el explorador y el editor.",
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
    /// Les registres tels qu'ils étaient au moment de l'appel. Gardés plutôt
    /// que la phrase déjà rédigée : l'explication se recalcule à l'affichage,
    /// et suit donc la langue si elle change en cours de session.
    pub(super) regs: crate::debugger::Registers,
}

pub struct App {
    pub(super) src_path: PathBuf,
    pub(super) out_dir: PathBuf,
    /// Contenu de l'éditeur (source NASM en cours d'édition).
    pub(super) source: String,
    /// Contenu de référence : ce que `source` valait au dernier enregistrement,
    /// à l'ouverture du fichier, ou au chargement d'un squelette. L'état
    /// « modifié » ([`App::dirty`]) s'en déduit par comparaison, au lieu d'être
    /// un drapeau que chaque chemin d'édition doit penser à lever — c'est ce
    /// qui laissait passer des modifications non signalées, et donc perdues
    /// sans un mot au moment de fermer. Corollaire gratuit : revenir à la main
    /// au texte enregistré efface le marqueur « ● », comme dans un vrai IDE.
    pub(super) saved_source: String,
    pub(super) binary: Option<PathBuf>,
    /// Projet actuellement ouvert, s'il y en a un. Un fichier `.asm` seul
    /// garde exactement l'ancien comportement ; le manifeste ne devient actif
    /// que lorsqu'il a été ouvert explicitement.
    pub(super) project: Option<crate::project::Project>,

    pub(super) dbg: Option<Debugger>,
    pub(super) disasm: Vec<Insn>,
    /// Mapping adresse → ligne source (1-based) pour le suivi dans l'éditeur.
    pub(super) src_map: HashMap<u64, usize>,
    pub(super) selected: Option<u64>,
    /// Instruction ouverte dans le mode « microscope » (fenêtre dédiée).
    pub(super) microscope: Option<u64>,
    /// Adresse → index dans `disasm`. Reconstruit à chaque lancement : la
    /// trace remonte l'historique instruction par instruction, et une
    /// recherche linéaire dans le désassemblage y coûtait le prix fort.
    pub(super) disasm_index: HashMap<u64, usize>,
    /// Appels système exécutés (panneau SYSCALLS).
    pub(super) syscalls: Vec<SyscallLog>,
    /// Adresses des frames actives (panneau CALL STACK), suivi call/ret.
    pub(super) call_stack: Vec<u64>,
    /// Nombre de transitions de l'historique déjà dépouillées par
    /// `extend_trace` : au pas suivant, seule la nouvelle est à traiter.
    pub(super) trace_cursor: usize,
    /// L'appel système final (celui qui a tué le processus, sans snapshot
    /// successeur) a déjà été journalisé.
    pub(super) trace_tail_done: bool,
    /// Pas lancé dont la finalisation reste à faire — le programme est
    /// suspendu dans un appel système (voir `RunState::Running`).
    pub(super) step_in_flight: bool,
    /// « Continuer » interrompu par un appel système bloquant, à reprendre dès
    /// que celui-ci aura rendu la main. `Some(None)` pour un « continuer »
    /// ordinaire, `Some(Some(addr))` pour un pas par-dessus.
    pub(super) run_pending: Option<Option<u64>>,
    /// Appel système sur le point de s'exécuter, mémorisé avant le pas pour
    /// être journalisé avec sa valeur de retour une fois le pas achevé.
    pub(super) pending_syscall: Option<(String, u64)>,
    /// Ligne que l'élève s'apprête à envoyer sur l'entrée standard du
    /// programme (champ de saisie du panneau Console).
    pub(super) stdin_input: String,
    /// Le focus a déjà été donné au champ de saisie pour l'attente en cours :
    /// il ne sera pas repris tant que le programme n'aura pas de nouveau
    /// besoin d'une entrée.
    pub(super) stdin_focus_claimed: bool,
    /// Même garde, pour le champ de la fenêtre « Sortie du programme ».
    /// Cette fenêtre s'ouvre justement au moment où un `read` bloque ; elle
    /// doit donc pouvoir recevoir la frappe sans voler le focus à chaque frame.
    pub(super) program_output_input_focus_claimed: bool,
    /// Points d'arrêt, par numéro de ligne source (1-based), avec leur
    /// condition éventuelle (`None` = s'arrêter à chaque passage). Posés sur la
    /// ligne et non sur l'adresse : c'est ce que l'élève voit, et ça survit à
    /// un réassemblage qui déplace le code.
    pub(super) breakpoints: std::collections::BTreeMap<usize, Option<crate::breakpoint::Condition>>,
    /// Ligne dont on est en train d'éditer la condition (fenêtre dédiée),
    /// avec le texte saisi et l'erreur d'analyse à afficher.
    pub(super) bp_cond_line: Option<usize>,
    pub(super) bp_cond_input: String,
    pub(super) bp_cond_error: Option<String>,
    /// Demande de focus du champ au premier frame d'ouverture.
    pub(super) bp_cond_focus: bool,
    /// Derniers fichiers ouverts, le plus récent en tête (menu Fichier ▸
    /// Récents). Persisté avec les réglages : reprendre son exercice de la
    /// veille ne devrait pas demander de renaviguer dans l'arborescence.
    pub(super) recent_files: Vec<PathBuf>,
    /// Dossier affiché dans l'explorateur de fichiers (panneau de gauche).
    pub(super) explorer_dir: PathBuf,
    /// Fichier retenu au clavier dans l'explorateur (surligné, ouvert par Entrée).
    pub(super) explorer_selected: Option<PathBuf>,
    /// Entrée dont le nom est édité directement dans l'explorateur.
    pub(super) explorer_renaming: Option<PathBuf>,
    pub(super) explorer_rename_input: String,
    /// Boîte légère de création de dossier depuis l'explorateur.
    pub(super) explorer_new_folder: bool,
    pub(super) explorer_new_folder_input: String,
    /// Entrée dont la suppression attend une confirmation explicite.
    pub(super) explorer_delete: Option<PathBuf>,
    pub(super) view_index: usize,

    pub(super) mem_addr: u64,
    pub(super) mem_input: String,
    /// Octets hexa à écrire en mémoire (laboratoire mémoire).
    pub(super) mem_poke: String,
    /// Registre retenu au clavier dans le panneau REGISTERS (index dans
    /// `Registers::named`), surligné et éditable par Entrée.
    pub(super) reg_sel: usize,
    /// Nombre de colonnes de registres réellement affichées au dernier rendu
    /// (adapté à la largeur du panneau). ↑/↓ sautent d'autant de registres pour
    /// suivre ce que l'œil voit dans la grille.
    pub(super) reg_cols: usize,
    /// Lecture choisie pour les registres XMM (panneau SSE/FPU). Les mêmes
    /// seize octets sont deux `double` ou seize entiers selon l'instruction :
    /// c'est l'élève qui dit laquelle il vient d'écrire.
    pub(super) xmm_view: crate::simd::XmmView,
    /// Masque les registres XMM entièrement nuls, pour ne garder que ceux qui
    /// travaillent (la plupart des programmes n'en utilisent que deux ou trois).
    pub(super) simd_hide_zero: bool,
    /// Système visé par l'assemblage. La cible Windows produit un vrai `.exe`
    /// PE64, que l'IDE sait lire mais pas exécuter : `ptrace` n'existe que sous
    /// Linux, et prétendre le contraire serait le seul vrai mensonge possible
    /// ici (voir [`crate::assemble::Target`]).
    pub(super) target: crate::assemble::Target,
    /// Description du dernier binaire assemblé, pour le panneau FORMAT.
    pub(super) format_info: Option<crate::binfmt::Overview>,
    /// L'assemblage Windows est-il proposé ?
    ///
    /// Une cible supplémentaire est une question de plus posée à qui apprend
    /// l'assembleur ; ceux qui n'en ont pas besoin la coupent, et l'IDE
    /// redevient ce qu'il était : un outil pour Linux. Décocher ramène la cible
    /// courante à Linux — laisser un `.exe` en cours sans plus aucun menu pour
    /// en sortir serait un cul-de-sac.
    pub(super) pe_enabled: bool,
    /// Exécutable Windows en cours d'exécution sous Wine, le cas échéant.
    ///
    /// Sa sortie va dans la même console que celle d'un programme Linux, mais
    /// il n'y a ni registres ni timeline derrière : Wine exécute, il ne se
    /// laisse pas déboguer instruction par instruction (voir
    /// [`crate::winerun`]).
    pub(super) wine: Option<crate::winerun::WineRun>,
    /// Registre en cours d'édition (laboratoire mémoire) et son tampon de saisie.
    pub(super) edit_reg: Option<&'static str>,
    pub(super) edit_buf: String,
    /// Demande de focus au premier frame d'édition d'un registre.
    pub(super) edit_focus: bool,
    pub(super) console: String,
    /// Ce que le programme a écrit, et rien d'autre : ni « Running… », ni les
    /// appels système journalisés, ni les erreurs de l'IDE. C'est le texte
    /// qu'un utilisateur verrait en lançant le binaire depuis un terminal, et
    /// que la boîte « Sortie du programme » montre tel quel.
    ///
    /// Doublon assumé d'une partie de `console` : la console mêle les deux flux
    /// pour raconter le déroulement, on ne peut donc pas l'y démêler après coup.
    pub(super) program_output: String,
    pub(super) show_program_output: bool,
    pub(super) status: String,
    /// Décalage vertical de l'éditeur (pour synchroniser la gouttière).
    pub(super) editor_scroll_y: f32,
    /// Position du curseur dans l'éditeur (1-based), pour la barre d'état.
    pub(super) editor_ln: usize,
    pub(super) editor_col: usize,
    /// Position du curseur dans l'éditeur (octets dans `source`), telle que
    /// laissée par le rendu précédent — utilisée pour l'appariement de
    /// parenthèses de la frame courante (un cran de retard imperceptible).
    pub(super) editor_cursor_byte: usize,
    /// Sélection courante de l'éditeur (indices de CARACTÈRES, comme le curseur
    /// d'egui), relevée au dernier rendu. C'est sur elle que travaillent les
    /// gestes d'édition de [`edit_ops`], déclenchés au clavier avant que
    /// l'éditeur ne soit redessiné.
    pub(super) editor_sel: (usize, usize),
    /// Sélection à replacer au prochain rendu, quand une opération d'édition a
    /// bougé le texte sous le curseur. Seul `editor_ui` peut l'écrire dans la
    /// mémoire d'egui, d'où cette boîte aux lettres.
    pub(super) pending_editor_sel: Option<(usize, usize)>,
    /// Boîte « aller à la ligne » (Ctrl+G).
    pub(super) show_goto_line: bool,
    pub(super) goto_line_input: String,
    pub(super) goto_line_focus: bool,
    /// La liste d'autocomplétion était affichée au dernier rendu. Les
    /// raccourcis, joués AVANT le rendu, s'en servent pour savoir à qui
    /// appartiennent ↑↓, Tab et Entrée.
    pub(super) complete_open: bool,
    /// Proposition retenue dans la liste d'autocomplétion.
    pub(super) complete_sel: usize,
    /// Début du mot pour lequel la liste a été écartée (Échap, ou complétion
    /// acceptée) : elle ne se rouvre qu'au mot suivant.
    pub(super) complete_dismissed: Option<usize>,
    /// Barre de recherche/remplacement (Ctrl+F / Ctrl+H) de l'éditeur.
    pub(super) show_find: bool,
    /// Affiche en plus la ligne de remplacement (Ctrl+H) plutôt que la seule recherche.
    pub(super) find_replace_mode: bool,
    pub(super) find_query: String,
    pub(super) find_replace_text: String,
    pub(super) find_case_sensitive: bool,
    /// Index (dans la liste des correspondances, recalculée à chaque frame)
    /// de la correspondance active.
    pub(super) find_current: usize,
    /// Ligne (0-based) vers laquelle faire défiler l'éditeur au prochain rendu,
    /// consommée par `editor_ui` — le même patron que `scroll_to_sel` pour les
    /// panneaux ancrables.
    pub(super) pending_scroll_to_line: Option<usize>,
    /// Labels de premier niveau actuellement repliés (par nom, pas par ligne :
    /// survit aux éditions ailleurs dans le fichier). Non vide => l'éditeur
    /// bascule en vue lecture seule (voir `folded_editor_ui`).
    pub(super) folded_labels: std::collections::BTreeSet<String>,
    pub(super) stack_tab: StackTab,
    /// Barre des actions d'exécution (Lancer, Suivant, Continuer…). Elle peut
    /// être masquée pour libérer de la hauteur sur un petit écran.
    pub(super) show_toolbar: bool,
    pub(super) show_tooltips: bool,
    /// Inspection au survol dans l'éditeur (voir [`inspect`]) : la valeur du
    /// mot sous le pointeur, affichée sur place.
    pub(super) inspect_hover: bool,
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
    /// Thème demandé : un thème nommé du catalogue, ou « celui du système ».
    pub(super) theme_pref: crate::theme::Choice,
    /// Langue de l'interface (Réglages).
    pub(super) lang: Lang,
    pub(super) show_settings: bool,
    pub(super) show_about: bool,
    pub(super) show_license: bool,
    pub(super) show_shortcuts: bool,
    pub(super) show_calculator: bool,
    /// Saisie de la calculatrice multi-base (texte brut, parsé selon `calc_base`).
    pub(super) calc_input: String,
    /// Second opérande de la calculatrice, et opération choisie.
    pub(super) calc_input_b: String,
    pub(super) calc_op: parse::CalcOp,
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
    /// Parcours guidé : sa progression et la leçon ouverte.
    ///
    /// Son ACTIVATION n'est pas rangée ici : elle se lit sur [`UiMode`]. Un
    /// booléen `tutorial_enabled` a longtemps doublé le mode, et rien ne les
    /// tenait d'accord — on pouvait être en mode « Apprentissage », l'étiquette
    /// affichée dans la barre d'état, alors que le panneau ✦ n'offrait plus le
    /// parcours. Le mode le porte désormais seul (voir [`App::tutorial_enabled`]).
    pub(super) tutorial_progress: crate::tutorial::Progress,
    pub(super) tutorial_current: Option<String>,
    /// Grande boîte du parcours guidé. Elle n'est pas persistée : au prochain
    /// lancement, l'élève retrouve l'IDE et peut reprendre le parcours quand il
    /// le souhaite, sans une fenêtre pédagogique imposée au-dessus de son code.
    pub(super) show_tutorial_dialog: bool,
    /// Indices déjà demandés, et pour quelle leçon. L'identifiant est gardé avec
    /// le compte pour que changer de leçon reparte de zéro sans qu'aucun chemin
    /// d'ouverture n'ait à y penser — il y en a quatre, et l'oubli d'un seul
    /// livrerait la solution d'une leçon à qui vient d'ouvrir la suivante.
    /// Volontairement non persisté : les indices se redemandent d'une session à
    /// l'autre, ce qui coûte un clic et laisse une chance de trouver seul.
    pub(super) tutorial_hints: Option<(String, usize)>,
    /// Mode d'affichage ET contexte d'apprentissage : apprentissage (épuré,
    /// parcours guidé offert) ou complet.
    pub(super) mode: UiMode,
    /// Bandeau d'accueil (mode apprentissage) écarté une fois pour toutes.
    pub(super) welcome_dismissed: bool,
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
    /// Licence en vigueur (désassemblage, registres/flags, timeline).
    pub(super) license: crate::license::LicenseState,
    /// Fenêtre de saisie de licence — distincte de `show_license`, qui n'affiche
    /// que le texte légal de `LICENSE.md`.
    pub(super) show_license_gate: bool,
    pub(super) license_input: String,
    pub(super) license_error: Option<String>,
    /// Rappel de licence affiché à intervalle irrégulier — distinct de
    /// `show_license_gate` : celui-ci est la carte d'accroche, dont le seul
    /// bouton d'action ouvre la vraie boîte de collage.
    pub(super) show_license_nag: bool,
    /// Demande de confirmation avant de désactiver la licence installée
    /// (bouton « Désactiver… » de la fenêtre « À propos »). La suppression du
    /// fichier n'a lieu qu'une fois cette confirmation acceptée : c'est une
    /// action irréversible sans le bloc de licence d'origine sous la main.
    pub(super) confirm_license_reset: bool,
    /// `true` quand `show_license_nag` a été ouverte pour bloquer une
    /// fermeture de fenêtre (voir `check_close_request`), plutôt que par le
    /// rappel périodique : change le bouton secondaire en « Quitter quand
    /// même » au lieu de « Plus tard ».
    pub(super) exit_pending: bool,
    /// `true` une fois que l'utilisateur a cliqué « Quitter quand même » sur
    /// la carte de rappel. Sans ça, le `ViewportCommand::Close` qu'on envoie
    /// nous-mêmes redéclenche `close_requested()` à la frame suivante :
    /// `check_close_request` l'interceptait alors une seconde fois et
    /// annulait sa propre fermeture (bouton visiblement sans effet).
    pub(super) quit_confirmed: bool,
    /// Action réclamée par l'élève, mise en attente le temps de lui demander
    /// quoi faire du travail non enregistré qu'elle écraserait (voir
    /// [`unsaved`]). `None` : rien en suspens, la boîte est fermée.
    pub(super) unsaved_prompt: Option<unsaved::PendingAction>,
    /// Un nouveau fichier attend de savoir pour quel format il est écrit : un
    /// squelette ELF et un squelette PE ne commencent pas par les mêmes lignes,
    /// et poser la question vaut mieux que d'imposer Linux puis de laisser
    /// l'élève découvrir l'erreur au moment d'assembler. `false` : rien en
    /// suspens. La question ne se pose que si l'assemblage Windows est activé.
    pub(super) new_file_prompt: bool,
    /// Création d'un projet : le nom et la cible sont demandés avant d'écrire
    /// `asmstudio.toml`, `src/main.asm` et `include/`.
    pub(super) new_project_prompt: bool,
    pub(super) new_project_name: String,
    pub(super) new_project_target: crate::assemble::Target,
    /// L'abandon des modifications a été confirmé pour la fermeture en cours :
    /// le `Close` qu'on réémet soi-même ne doit pas rouvrir la question.
    pub(super) discard_confirmed: bool,
    /// Fermeture à réémettre au prochain frame. La boîte « non enregistré »
    /// décide de quitter, mais c'est `update` qui a le viewport sous la main.
    pub(super) quit_requested: bool,
    /// Prochain rappel de licence (ouvre `show_license_nag` tout seul),
    /// en secondes de l'horloge egui (`ctx.input(|i| i.time)`). `None` tant
    /// qu'aucune échéance n'a encore été tirée pour cette session.
    pub(super) nag_next_at: Option<f64>,
}

/// Un réglage persisté : sa clé dans `settings.conf`, comment le lire depuis
/// l'application, comment l'y réappliquer.
///
/// Les valeurs transitent en texte parce que c'est ce qu'est le fichier : une
/// ligne `clé=valeur`. Chaque entrée porte donc sa propre conversion, ce qui
/// laisse cohabiter dans la même table un booléen, un identifiant de thème et
/// une liste de leçons terminées.
struct Setting {
    key: &'static str,
    read: fn(&App) -> String,
    write: fn(&mut App, &str),
}

/// Tous les réglages scalaires, dans l'ordre où ils sont écrits.
///
/// Une seule table, parce qu'il y en avait deux : une énumération dans
/// `apply_settings` et une autre dans `settings_content`, qui devaient rester
/// d'accord à la main. Oublier la première laissait un réglage qui marche
/// jusqu'au prochain lancement, et personne pour le dire ; oublier la seconde
/// l'empêchait d'être écrit du tout. Le round-trip est maintenant vrai par
/// construction, et le test qui le vérifie n'a plus de liste à tenir non plus
/// (voir `every_preference_survives_a_settings_round_trip`).
///
/// Deux clés n'y sont pas, faute d'entrer dans le moule d'une ligne unique :
/// `recent`, qui en occupe autant qu'il y a de fichiers, et `dock`, qui doit
/// être appliqué en dernier (voir [`App::apply_settings`]).
const SETTINGS: &[Setting] = &[
    // Les identifiants « system », « dark » et « light » sont ceux
    // qu'écrivaient les versions précédentes : un réglage existant est relu
    // tel quel.
    Setting {
        key: "theme",
        read: |a| a.theme_pref.key().to_string(),
        write: |a, v| a.theme_pref = crate::theme::Choice::from_key(v),
    },
    Setting { key: "lang", read: |a| a.lang.key().to_string(), write: |a, v| a.lang = Lang::from_key(v) },
    Setting { key: "mode", read: |a| a.mode.key().to_string(), write: |a, v| a.mode = UiMode::from_key(v) },
    Setting { key: "tooltips", read: |a| a.show_tooltips.to_string(), write: |a, v| a.show_tooltips = v == "true" },
    Setting { key: "toolbar", read: |a| a.show_toolbar.to_string(), write: |a, v| a.show_toolbar = v == "true" },
    Setting { key: "inspect_hover", read: |a| a.inspect_hover.to_string(), write: |a, v| a.inspect_hover = v == "true" },
    Setting { key: "asmstd", read: |a| a.use_asmstd.to_string(), write: |a, v| a.use_asmstd = v == "true" },
    Setting { key: "animate", read: |a| a.animate.to_string(), write: |a, v| a.animate = v == "true" },
    Setting { key: "pedagogy_anim", read: |a| a.pedagogy_anim.to_string(), write: |a, v| a.pedagogy_anim = v == "true" },
    Setting { key: "pedagogy_memview", read: |a| a.pedagogy_memview.to_string(), write: |a, v| a.pedagogy_memview = v == "true" },
    Setting { key: "pedagogy_predict", read: |a| a.pedagogy_predict.to_string(), write: |a, v| a.pedagogy_predict = v == "true" },
    // La clé « tutorial » a disparu : le parcours suit le mode. Un fichier de
    // réglages qui la porte encore n'en souffre pas — `apply_settings` ignore
    // les clés qu'il ne connaît pas.
    Setting {
        key: "tutorial_done",
        read: |a| a.tutorial_progress.to_string(),
        write: |a, v| a.tutorial_progress = crate::tutorial::Progress::parse(v),
    },
    Setting {
        key: "tutorial_current",
        read: |a| a.tutorial_current.clone().unwrap_or_default(),
        write: |a, v| a.tutorial_current = (!v.is_empty()).then(|| v.to_string()),
    },
    Setting { key: "welcome_dismissed", read: |a| a.welcome_dismissed.to_string(), write: |a, v| a.welcome_dismissed = v == "true" },
    Setting { key: "target", read: |a| a.target.key().to_string(), write: |a, v| a.target = crate::assemble::Target::from_key(v) },
    Setting { key: "pe", read: |a| a.pe_enabled.to_string(), write: |a, v| a.pe_enabled = v == "true" },
];

impl App {
    pub fn new() -> Self {
        setup_examples();
        let src_path = elf_examples_dir().join("hello_world.asm");
        let source = std::fs::read_to_string(&src_path).unwrap_or_else(|_| {
            "section .text\n    global _start\n_start:\n    mov rax, 60\n    xor rdi, rdi\n    syscall\n"
                .to_string()
        });
        // Le catalogue s'ouvre sur ses deux catégories : ELF et Windows.
        let explorer_dir = examples_dir();
        let out_dir = data_dir().join("build");
        let mut app = App {
            src_path,
            out_dir,
            saved_source: source.clone(),
            source,
            binary: None,
            project: None,
            dbg: None,
            disasm: Vec::new(),
            src_map: HashMap::new(),
            selected: None,
            microscope: None,
            disasm_index: HashMap::new(),
            syscalls: Vec::new(),
            call_stack: Vec::new(),
            trace_cursor: 0,
            trace_tail_done: false,
            step_in_flight: false,
            run_pending: None,
            pending_syscall: None,
            stdin_input: String::new(),
            stdin_focus_claimed: false,
            program_output_input_focus_claimed: false,
            breakpoints: std::collections::BTreeMap::new(),
            bp_cond_line: None,
            bp_cond_input: String::new(),
            bp_cond_error: None,
            bp_cond_focus: false,
            recent_files: Vec::new(),
            explorer_dir,
            explorer_selected: None,
            explorer_renaming: None,
            explorer_rename_input: String::new(),
            explorer_new_folder: false,
            explorer_new_folder_input: String::new(),
            explorer_delete: None,
            view_index: 0,
            mem_addr: 0,
            mem_input: String::new(),
            mem_poke: String::new(),
            reg_sel: 0,
            reg_cols: 2,
            // Deux `double` par défaut : c'est la forme des premiers pas en
            // flottant (`addsd`, `cvtsi2sd`), avant tout calcul vectoriel.
            xmm_view: crate::simd::XmmView::F64,
            simd_hide_zero: true,
            target: crate::assemble::Target::Linux,
            // Proposée par défaut : la fonctionnalité existe, autant qu'elle se
            // découvre. Une case dans les Réglages suffit à la retirer.
            pe_enabled: true,
            format_info: None,
            wine: None,
            edit_reg: None,
            edit_buf: String::new(),
            edit_focus: false,
            console: String::new(),
            program_output: String::new(),
            show_program_output: false,
            status: String::new(),
            editor_scroll_y: 0.0,
            editor_ln: 1,
            editor_col: 1,
            editor_cursor_byte: 0,
            editor_sel: (0, 0),
            pending_editor_sel: None,
            show_goto_line: false,
            goto_line_input: String::new(),
            goto_line_focus: false,
            complete_open: false,
            complete_sel: 0,
            complete_dismissed: None,
            show_find: false,
            find_replace_mode: false,
            find_query: String::new(),
            find_replace_text: String::new(),
            find_case_sensitive: false,
            find_current: 0,
            pending_scroll_to_line: None,
            folded_labels: std::collections::BTreeSet::new(),
            stack_tab: StackTab::Stack,
            show_toolbar: true,
            show_tooltips: true,
            inspect_hover: true,
            animate: true,
            pedagogy_anim: false,
            pedagogy_memview: false,
            use_asmstd: false,
            flash_time: 0.0,
            pending_flash: false,
            theme_pref: crate::theme::Choice::default(),
            lang: Lang::Fr,
            show_settings: false,
            show_about: false,
            show_license: false,
            show_shortcuts: false,
            show_calculator: false,
            calc_input: String::new(),
            calc_input_b: String::new(),
            calc_op: parse::CalcOp::And,
            // Hexadécimal par défaut : c'est la base dans laquelle on lit un
            // registre, une adresse ou un masque.
            calc_base: 16,
            icons: None,
            welcome_dismissed: false,
            tutorial_progress: crate::tutorial::Progress::default(),
            tutorial_current: None,
            show_tutorial_dialog: false,
            tutorial_hints: None,
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
            license: crate::license::LicenseState::Missing,
            show_license_gate: false,
            license_input: String::new(),
            license_error: None,
            show_license_nag: false,
            confirm_license_reset: false,
            exit_pending: false,
            quit_confirmed: false,
            unsaved_prompt: None,
            new_file_prompt: false,
            new_project_prompt: false,
            new_project_name: "mon-projet".to_string(),
            new_project_target: crate::assemble::Target::Linux,
            discard_confirmed: false,
            quit_requested: false,
            nag_next_at: None,
        };
        app.load_settings();
        app.license = crate::license::load();
        // Plus d'ouverture automatique au lancement : un rappel systématique
        // à chaque démarrage était trop intrusif. La fenêtre se rouvre plutôt
        // toute seule à intervalle irrégulier pendant que l'app tourne (voir
        // `nag_next_at` dans `update()`), et reste sinon accessible à la main
        // (menu Aide, palette).
        //
        // Une licence stockée mais devenue invalide (ex. mise à jour vers une
        // version différente) doit expliquer pourquoi, pas seulement rouvrir
        // une fenêtre de saisie vide, dès qu'elle s'affichera.
        if let crate::license::LicenseState::Invalid(reason) = &app.license {
            app.license_error = Some(reason.clone());
        }
        app
    }

    /// Comme [`Self::new`], et ouvre le fichier nommé sur la ligne de commande.
    ///
    /// Sans cela l'application ignorait son argument : lancée avec un chemin,
    /// elle s'ouvrait sur son écran d'accueil comme si de rien n'était. Un
    /// outil qui lui passe la main — Desdec, qui exporte une fonction en NASM
    /// — n'avait alors d'autre recours que de copier le chemin et de laisser
    /// l'utilisateur l'ouvrir lui-même.
    ///
    /// Un chemin qui n'existe pas ou ne se lit pas laisse l'accueil habituel :
    /// démarrer sur une erreur pour un argument que personne n'a peut-être
    /// tapé serait pire que de l'ignorer.
    #[must_use]
    pub fn new_opening(path: Option<std::path::PathBuf>) -> Self {
        let mut app = Self::new();
        if let Some(path) = path.filter(|path| path.is_file()) {
            app.open_file(path);
        }
        app
    }

    // ---------- Persistance des réglages ----------

    pub(super) fn load_settings(&mut self) {
        // (voir [`SETTINGS`] pour la table qui décrit le format)
        // Les tests ne doivent pas dépendre de la configuration de la machine :
        // sinon leur résultat change selon la langue choisie par l'utilisateur.
        if cfg!(test) {
            return;
        }
        let Some(path) = settings_path() else { return };
        let Ok(content) = std::fs::read_to_string(&path) else { return };
        self.apply_settings(&content);
    }

    /// Applique un fichier de réglages déjà lu. Séparé de [`load_settings`]
    /// pour que le format se teste sans toucher au disque de l'utilisateur —
    /// ni dépendre de ce qui s'y trouve.
    pub(super) fn apply_settings(&mut self, content: &str) {
        // La disposition est appliquée en dernier : elle dépend des autres
        // réglages (langue pour les titres) et remplace l'arbre par défaut.
        let mut saved_dock: Option<String> = None;
        for line in content.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                // Une ligne par fichier, dans l'ordre où elles ont été
                // écrites : pas de séparateur à choisir, donc rien à échapper
                // dans un chemin qui en contiendrait un.
                "recent" => {
                    if !v.is_empty() {
                        self.recent_files.push(grouped_recent_path(PathBuf::from(v)));
                    }
                }
                "dock" => saved_dock = Some(v.to_string()),
                // Tout le reste vient de la table : une clé inconnue (réglage
                // retiré, fichier bricolé) est ignorée sans bruit.
                key => {
                    if let Some(s) = SETTINGS.iter().find(|s| s.key == key) {
                        (s.write)(self, v);
                    }
                }
            }
        }
        match saved_dock.filter(|layout| !layout.trim().is_empty()) {
            Some(layout) => self.apply_dock_layout(&layout),
            // Pas de disposition enregistrée : celle du mode relu, sinon un
            // réglage `mode=full` afficherait la disposition d'apprentissage.
            None => self.dock = Some(dock::layout_for(self.mode)),
        }
        self.keep_examples_closed_outside_learning();
    }

    pub(super) fn save_settings(&self) {
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
        let _ = std::fs::write(&path, self.settings_content());
    }

    /// Le fichier de réglages, tel qu'il sera écrit. Séparé de
    /// [`save_settings`] pour la même raison que [`apply_settings`] : le format
    /// se vérifie alors sans écrire nulle part.
    pub(super) fn settings_content(&self) -> String {
        let mut out = String::new();
        for s in SETTINGS {
            out.push_str(s.key);
            out.push('=');
            out.push_str(&(s.read)(self));
            out.push('\n');
        }
        for p in &self.recent_files {
            out.push_str(&format!("recent={}\n", p.display()));
        }
        // En dernier, comme il est relu en dernier.
        out.push_str(&format!("dock={}\n", self.dock_layout_string()));
        out
    }

    // ---------- Travail non enregistré ----------

    /// Le tampon d'édition diffère de ce qui est sur le disque.
    pub(super) fn dirty(&self) -> bool {
        self.source != self.saved_source
    }

    /// Prend le contenu courant pour référence : il n'y a plus rien à
    /// enregistrer. À appeler après une écriture réussie, une lecture de
    /// fichier, ou l'insertion d'un texte de départ que l'élève n'a pas encore
    /// touché — un squelette vierge n'est pas du travail à sauver.
    pub(super) fn mark_saved(&mut self) {
        self.saved_source = self.source.clone();
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
    /// Le programme peut avancer d'un pas — et, par la même occasion, recevoir
    /// une écriture de registre ou de mémoire. `is_ready` et non `is_alive` :
    /// un processus suspendu dans un appel système est bien vivant, mais ptrace
    /// n'a pas la main dessus, et tout ce qui est proposé ici y échouerait.
    pub(super) fn can_step(&self) -> bool {
        self.target.is_runnable()
            && self.dbg.as_ref().is_some_and(|d| d.is_ready())
            && self.is_head_view()
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

    // ---------- Palette de texte du thème courant ----------
    // Ces méthodes ne choisissaient qu'entre deux jeux de couleurs codés en
    // dur, « sombre » et « clair ». Elles lisent maintenant le thème (voir
    // [`crate::theme`]), qui en compte autant qu'on veut. Elles restent des
    // méthodes de `App` — c'est ainsi que les appelle tout le code d'affichage.

    /// Couleur des titres de section / libellés secondaires.
    pub(super) fn c_header(&self) -> Color32 {
        crate::theme::current().ui.header
    }
    /// Couleur des mnémoniques / accents bleus.
    pub(super) fn c_mnemonic(&self) -> Color32 {
        crate::theme::current().ui.mnemonic
    }
    /// Couleur des adresses.
    pub(super) fn c_addr(&self) -> Color32 {
        crate::theme::current().ui.addr
    }
    /// Couleur des octets bruts / texte discret monospace.
    pub(super) fn c_bytes(&self) -> Color32 {
        crate::theme::current().ui.bytes
    }
    /// Fond de la ligne RIP dans le désassemblage.
    pub(super) fn c_rip_row(&self) -> Color32 {
        crate::theme::current().ui.rip_row
    }
    /// Fond d'une ligne sélectionnée / survolée dans le désassemblage.
    pub(super) fn c_sel_row(&self) -> Color32 {
        crate::theme::current().ui.sel_row
    }

    /// Nombre de boîtes de dialogue (fenêtres flottantes) actuellement ouvertes.
    /// Sert à repérer, dans `update`, l'image où l'une vient de se fermer : en
    /// rendu à la demande — c'est le cas ici, on ne repeint que sur événement —
    /// l'image qui EFFACE la fenêtre fermée n'est pas toujours présentée d'elle-
    /// même (frame callback Wayland, animations coupées…). Un repaint explicite
    /// à cet instant garantit que le dialogue disparaît sans attendre le prochain
    /// mouvement de souris.
    fn open_dialog_count(&self) -> usize {
        use crate::updater::UpdateState;
        let updater_shown = matches!(
            self.updater.state,
            UpdateState::Checking
                | UpdateState::Available(_)
                | UpdateState::Downloading(_)
                | UpdateState::Done
                | UpdateState::Error(_)
        );
        [
            self.show_about,
            self.show_license,
            self.show_shortcuts,
            self.show_settings,
            self.show_calculator,
            self.show_goto_line,
            self.show_program_output,
            self.show_license_gate,
            self.show_license_nag,
            self.confirm_license_reset,
            self.palette_open,
            self.unsaved_prompt.is_some(),
            self.new_file_prompt,
            self.new_project_prompt,
            self.bp_cond_line.is_some(),
            self.pedagogy_predict,
            self.microscope.is_some(),
            self.diagnosis.is_some(),
            self.show_tutorial_dialog,
            updater_shown,
        ]
        .into_iter()
        .filter(|&open| open)
        .count()
    }

    /// À appeler après avoir rendu toutes les boîtes de dialogue, avec le nombre
    /// qui était ouvert AVANT. Si l'une s'est refermée pendant l'image, force un
    /// rendu : voir [`open_dialog_count`](Self::open_dialog_count) pour le motif.
    fn repaint_on_dialog_close(&self, ctx: &egui::Context, opened_before: usize) {
        if self.open_dialog_count() < opened_before {
            ctx.request_repaint();
        }
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
        // Sortie du programme tracé, et fin d'un pas resté en suspens sur un
        // appel système bloquant. Avant toute peinture : la console et la
        // barre d'état de cette frame doivent montrer l'état à jour.
        self.poll_debugger(ctx);
        // Et, pour la cible Windows, le programme confié à Wine : il écrit dans
        // la même console, sur le même rythme d'une sonde par frame.
        self.poll_wine(ctx);
        if self.pending_flash {
            self.flash_time = ctx.input(|i| i.time);
            self.pending_flash = false;
        }
        self.handle_shortcuts(ctx);

        self.menu_bar(ctx);
        self.toolbar(ctx);
        self.welcome_banner(ctx);
        self.status_bar(ctx);

        // Toute la zone centrale est un arbre de panneaux ancrables : chaque
        // panneau est un onglet que l'on déplace, empile ou détache en fenêtre.
        self.dock_ui(ctx);

        // Une boîte fermée par son bouton (« Fermer », « Valider »…) bascule son
        // état APRÈS avoir été peinte cette image. On mémorise combien étaient
        // ouvertes avant, pour forcer un rendu si l'une s'est refermée : sinon,
        // en rendu à la demande, l'ancienne image resterait affichée jusqu'au
        // prochain événement (dialogue « collé » à l'écran).
        let dialogs_before = self.open_dialog_count();
        self.tutorial_dialog_ui(ctx);
        self.about_window(ctx);
        self.license_window(ctx);
        self.shortcuts_window(ctx);
        self.settings_window(ctx);
        self.microscope_window(ctx);
        self.breakpoint_condition_window(ctx);
        self.goto_line_window(ctx);
        self.calculator_window(ctx);
        self.program_output_window(ctx);
        self.palette_window(ctx);
        self.predict_window(ctx);
        self.diagnosis_window(ctx);
        self.update_window(ctx);
        self.check_license_nag(ctx);
        self.check_close_request(ctx);
        self.unsaved_window(ctx);
        // Après elle : la question du format ne se pose qu'une fois le sort du
        // travail en cours réglé, sinon deux boîtes se superposeraient.
        self.new_file_format_window(ctx);
        self.new_project_window(ctx);
        // La boîte « non enregistré » décide de quitter, mais c'est ici qu'on
        // tient le viewport : la fermeture est réémise au frame suivant, une
        // fois `discard_confirmed` posé pour ne pas reposer la question.
        if std::mem::take(&mut self.quit_requested) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.license_nag_window(ctx);
        self.license_gate_window(ctx);
        self.license_reset_confirm_window(ctx);
        self.repaint_on_dialog_close(ctx, dialogs_before);
        self.updater.poll();
    }
}

impl App {
    /// Rouvre `show_license_nag` tout seul, à intervalle irrégulier, tant
    /// qu'aucune licence n'est active — plutôt qu'à chaque lancement (trop
    /// intrusif). Le premier délai est tiré ici, au premier appel, faute
    /// d'horloge egui disponible dans `App::new()`.
    fn check_license_nag(&mut self, ctx: &egui::Context) {
        if self.is_licensed() {
            return;
        }
        let now = ctx.input(|i| i.time);
        match self.nag_next_at {
            None => self.nag_next_at = Some(now + Self::random_nag_interval()),
            Some(t) if now >= t => {
                self.show_license_nag = true;
                self.nag_next_at = Some(now + Self::random_nag_interval());
            }
            _ => {
                // Sans interaction utilisateur, egui ne redessine pas de
                // lui-même : on force un réveil de temps à autre pour que
                // l'échéance soit vérifiée même si l'app reste ouverte sans
                // qu'on y touche.
                ctx.request_repaint_after(std::time::Duration::from_secs(60));
            }
        }
    }

    /// Intercepte une tentative de fermeture (croix de la fenêtre ou Fichier ▸
    /// Quitter, les deux passent par le même événement) pour deux motifs, dans
    /// cet ordre :
    ///
    /// 1. du travail non enregistré — c'est irréversible, et c'est la seule
    ///    chose qu'on ne peut pas rendre à l'élève après coup ;
    /// 2. l'absence de licence : la carte de rappel s'ouvre une dernière fois,
    ///    avec un geste explicite pour quitter quand même plutôt qu'une
    ///    fermeture silencieuse qui ne rappelle jamais rien à personne.
    ///
    /// L'ordre compte : les deux boîtes s'ouvriraient sinon l'une par-dessus
    /// l'autre sur le même événement.
    fn check_close_request(&mut self, ctx: &egui::Context) {
        // Travail à perdre : la question passe avant tout le reste, y compris
        // pour un utilisateur licencié (qui sortirait sinon d'ici sans contrôle).
        if !self.discard_confirmed
            && self.dirty()
            && ctx.input(|i| i.viewport().close_requested())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.unsaved_prompt = Some(unsaved::PendingAction::Quit);
            return;
        }
        if self.is_licensed() {
            return;
        }
        // `quit_confirmed` : une fois « Quitter quand même » cliqué, on laisse
        // filer — sinon le `Close` qu'on envoie soi-même repasserait par ici
        // et s'auto-annulerait (voir la doc du champ). Mais envoyer `Close`
        // ne fait que programmer un événement pour la frame suivante (voir
        // `egui_winit::process_viewport_commands` et le commentaire
        // d'eframe sur `WindowEvent::CloseRequested` : « we may need to
        // repaint... perhaps twice »). En rendu à la demande, sans repaint
        // demandé, cette frame suivante n'arrive jamais toute seule et
        // l'appli semble ne pas vouloir quitter : on force donc un réveil
        // continu jusqu'à ce qu'eframe ait fini de traiter la fermeture.
        if self.quit_confirmed {
            ctx.request_repaint();
            return;
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.show_license_nag = true;
            self.exit_pending = true;
        }
    }

    /// Entre 25 et 45 minutes : assez espacé pour ne pas agacer, comme
    /// demandé. Pseudo-aléatoire via les nanosecondes de l'horloge système —
    /// pas besoin d'une dépendance `rand` pour un simple délai d'agacement.
    fn random_nag_interval() -> f64 {
        const MIN_SECS: f64 = 25.0 * 60.0;
        const SPAN_SECS: f64 = 20.0 * 60.0;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        MIN_SECS + (nanos as f64 / u32::MAX as f64) * SPAN_SECS
    }
}

// ---------- Helpers ----------
































#[cfg(test)]
mod tests {
    use super::*;

    /// Chaque case à cocher des Réglages doit survivre à la fermeture de
    /// l'application. L'oubli type est d'ajouter un réglage à la fenêtre sans
    /// l'ajouter au fichier : il fonctionne, jusqu'au prochain lancement.
    #[test]
    fn every_preference_survives_a_settings_round_trip() {
        for s in SETTINGS {
            // Un réglage à deux états se teste dans les deux sens : écrit en
            // dur à `true`, il passerait un test qui ne vérifie que `true`.
            // Les autres (thème, langue, cible…) ont leurs propres tests, qui
            // savent quelles valeurs ont un sens ; ici on vérifie au moins que
            // la clé fait l'aller-retour.
            let default = (s.read)(&App::new());
            let values: &[&str] = match default.as_str() {
                "true" | "false" => &["false", "true"],
                other => &[other],
            };
            for value in values {
                let mut app = App::new();
                (s.write)(&mut app, value);
                let content = app.settings_content();
                let mut reloaded = App::new();
                reloaded.apply_settings(&content);
                assert_eq!(&(s.read)(&reloaded), value, "« {} » perdu à la relecture", s.key);
            }
        }
    }

    /// Relire ce qu'on vient d'écrire doit redonner exactement le même fichier.
    /// C'est ce qui attrape une clé écrite que personne ne relit — le réglage
    /// est alors perdu au lancement suivant, sans rien signaler.
    #[test]
    fn a_settings_file_read_back_writes_itself_identically() {
        let app = App::new();
        let content = app.settings_content();
        let mut reloaded = App::new();
        reloaded.apply_settings(&content);
        assert_eq!(reloaded.settings_content(), content);
    }

    /// Un thème choisi doit se retrouver au lancement suivant : c'est tout
    /// l'intérêt du réglage.
    #[test]
    fn the_chosen_theme_survives_a_settings_round_trip() {
        for t in crate::theme::THEMES.iter() {
            let mut app = App::new();
            app.theme_pref = crate::theme::Choice::Named(t.id);
            let content = app.settings_content();

            let mut reloaded = App::new();
            reloaded.apply_settings(&content);
            assert_eq!(reloaded.theme_pref, crate::theme::Choice::Named(t.id), "{}", t.id);
        }
        let mut app = App::new();
        app.theme_pref = crate::theme::Choice::System;
        let content = app.settings_content();
        let mut reloaded = App::new();
        reloaded.apply_settings(&content);
        assert_eq!(reloaded.theme_pref, crate::theme::Choice::System);
    }

    /// Les réglages écrits par les versions d'avant le catalogue ne connaissent
    /// que `system`, `dark` et `light`. Ils doivent continuer d'être relus, sans
    /// quoi tout le monde retrouverait le thème par défaut à la mise à jour.
    #[test]
    fn settings_written_before_the_theme_catalogue_are_still_read() {
        for (key, expected) in [
            ("system", crate::theme::Choice::System),
            ("dark", crate::theme::Choice::Named("dark")),
            ("light", crate::theme::Choice::Named("light")),
        ] {
            let mut app = App::new();
            app.apply_settings(&format!("theme={key}\n"));
            assert_eq!(app.theme_pref, expected, "réglage « theme={key} »");
        }
    }

    /// Chaque thème doit pouvoir être appliqué et l'application se peindre avec,
    /// de bout en bout. C'est le test qui attrape un thème ajouté à la va-vite.
    #[test]
    fn every_theme_paints_the_whole_application() {
        // Ce test écrit le thème global : il ne peut pas tourner pendant qu'un
        // autre juge une couleur peinte.
        let _theme = crate::theme::lock_for_test();
        for t in crate::theme::THEMES.iter() {
            let mut app = App::new();
            app.set_ui_mode(UiMode::Full);
            app.theme_pref = crate::theme::Choice::Named(t.id);
            app.show_settings = true;
            let ctx = egui::Context::default();
            let _ = ctx.run(Default::default(), |ctx| {
                app.apply_theme(ctx);
                app.dock_ui(ctx);
                app.settings_window(ctx);
            });
            assert_eq!(crate::theme::current().id, t.id, "{} n'a pas été appliqué", t.id);
        }
    }

    /// Le compteur reflète bien chaque boîte de dialogue, une par une.
    #[test]
    fn open_dialog_count_tracks_each_window() {
        let mut app = App::new();
        assert_eq!(app.open_dialog_count(), 0, "aucune boîte au départ");
        app.show_settings = true;
        assert_eq!(app.open_dialog_count(), 1);
        app.microscope = Some(0x1000);
        assert_eq!(app.open_dialog_count(), 2);
        app.show_settings = false;
        app.microscope = None;
        assert_eq!(app.open_dialog_count(), 0);
    }

    /// Fermer une boîte pendant l'image doit forcer un rendu : sinon, en mode
    /// rendu à la demande, l'ancienne image resterait « collée » à l'écran
    /// jusqu'au prochain événement. C'est le cœur du correctif.
    #[test]
    fn closing_a_dialog_requests_a_repaint() {
        use std::time::Duration;
        let app = App::new(); // plus aucune boîte ouverte (0)
        let opened_before = 1; // ... alors qu'une l'était en début d'image

        let ctx = egui::Context::default();
        let out = ctx.run(Default::default(), |ctx| {
            app.repaint_on_dialog_close(ctx, opened_before);
        });
        let delay = out.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        assert_eq!(delay, Duration::ZERO, "la fermeture doit replanifier un rendu");
    }

    /// À l'inverse, sans fermeture, on ne réveille pas l'application pour rien :
    /// le mode rendu à la demande doit rester économe.
    #[test]
    fn a_stable_frame_does_not_force_a_repaint() {
        use std::time::Duration;
        let mut app = App::new();
        app.show_settings = true; // une boîte ouverte, et elle le reste
        let opened_before = app.open_dialog_count();

        let ctx = egui::Context::default();
        // La première image d'un contexte neuf demande toujours un rendu de plus
        // (stabilisation polices/layout) : on stabilise avant de mesurer.
        for _ in 0..4 {
            let _ = ctx.run(Default::default(), |_| {});
        }
        let out = ctx.run(Default::default(), |ctx| {
            app.repaint_on_dialog_close(ctx, opened_before);
        });
        let delay = out.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        assert!(delay > Duration::from_secs(1), "aucune fermeture ⇒ pas de repaint forcé");
    }

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

    // ---------- Rappel de licence (nag) ----------

    fn run_at(app: &mut App, ctx: &egui::Context, time: f64) {
        let input = egui::RawInput { time: Some(time), ..Default::default() };
        let _ = ctx.run(input, |ctx| app.check_license_nag(ctx));
    }

    #[test]
    fn first_check_schedules_a_future_nag_without_opening_it() {
        let mut app = App::new();
        assert!(!app.is_licensed());
        let ctx = egui::Context::default();
        run_at(&mut app, &ctx, 0.0);
        assert!(!app.show_license_nag, "pas d'ouverture au premier lancement");
        assert!(app.nag_next_at.is_some_and(|t| t > 0.0), "une échéance future doit être tirée");
    }

    #[test]
    fn nag_opens_once_the_deadline_is_reached_and_schedules_the_next_one() {
        let mut app = App::new();
        let ctx = egui::Context::default();
        run_at(&mut app, &ctx, 0.0);
        let first_deadline = app.nag_next_at.unwrap();

        // Juste avant l'échéance : rien ne s'ouvre encore.
        run_at(&mut app, &ctx, first_deadline - 1.0);
        assert!(!app.show_license_nag);

        // À l'échéance (ou après) : la fenêtre s'ouvre, et une nouvelle
        // échéance future est tirée pour le prochain rappel.
        run_at(&mut app, &ctx, first_deadline);
        assert!(app.show_license_nag, "l'échéance est atteinte : le rappel s'ouvre");
        let second_deadline = app.nag_next_at.unwrap();
        assert!(second_deadline > first_deadline, "un nouveau délai est programmé");
    }

    #[test]
    fn a_valid_license_never_triggers_the_nag() {
        let mut app = App::new();
        app.license = crate::license::valid_for_tests();
        let ctx = egui::Context::default();
        run_at(&mut app, &ctx, 0.0);
        run_at(&mut app, &ctx, 1_000_000.0);
        assert!(!app.show_license_nag, "licencié : jamais de rappel");
        assert!(app.nag_next_at.is_none(), "aucune échéance n'est même tirée");
    }

    // ---------- Blocage de la fermeture tant que non licencié ----------

    /// Simule un événement de fermeture (croix de fenêtre, ou `ViewportCommand::Close`
    /// envoyé par Fichier ▸ Quitter — les deux se traduisent par le même événement).
    fn close_requested_input() -> egui::RawInput {
        let mut input = egui::RawInput::default();
        input.viewports.insert(
            input.viewport_id,
            egui::ViewportInfo { events: vec![egui::ViewportEvent::Close], ..Default::default() },
        );
        input
    }

    #[test]
    fn unlicensed_close_is_cancelled_and_opens_the_nag() {
        let mut app = App::new();
        assert!(!app.is_licensed());
        let ctx = egui::Context::default();
        let out = ctx.run(close_requested_input(), |ctx| app.check_close_request(ctx));

        assert!(app.show_license_nag, "la fermeture doit ouvrir la carte de rappel");
        assert!(app.exit_pending, "on doit savoir que c'est une tentative de fermeture");
        let commands = &out.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(
            commands.contains(&egui::ViewportCommand::CancelClose),
            "la fermeture doit être annulée le temps de montrer le rappel"
        );
    }

    #[test]
    fn licensed_close_is_never_intercepted() {
        let mut app = App::new();
        app.license = crate::license::valid_for_tests();
        let ctx = egui::Context::default();
        let out = ctx.run(close_requested_input(), |ctx| app.check_close_request(ctx));

        assert!(!app.show_license_nag, "licencié : la fermeture doit se dérouler normalement");
        let commands = &out.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(!commands.contains(&egui::ViewportCommand::CancelClose));
    }

    /// Régression : après avoir cliqué « Quitter quand même », le `Close`
    /// qu'on envoie soi-même ne doit plus être réintercepté à la frame
    /// suivante — sinon le bouton n'a visiblement aucun effet (la carte se
    /// rouvre en boucle au lieu de laisser l'appli se fermer).
    #[test]
    fn confirmed_quit_is_never_intercepted_again() {
        let mut app = App::new();
        app.quit_confirmed = true; // posé par le bouton « Quitter quand même »
        let ctx = egui::Context::default();
        let out = ctx.run(close_requested_input(), |ctx| app.check_close_request(ctx));

        assert!(!app.show_license_nag, "la carte ne doit pas se rouvrir après confirmation");
        let commands = &out.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(
            !commands.contains(&egui::ViewportCommand::CancelClose),
            "la fermeture confirmée ne doit plus jamais être annulée"
        );
    }

    /// Régression : `ViewportCommand::Close` ne fait que programmer un
    /// événement pour la frame suivante (voir la doc de `check_close_request`).
    /// Sans repaint forcé tant que `quit_confirmed`, en rendu à la demande
    /// cette frame n'arrive jamais et l'appli ne quitte jamais vraiment —
    /// c'est ce qui rendait le bouton « Quitter quand même » silencieusement
    /// inopérant.
    #[test]
    fn quit_confirmed_keeps_requesting_a_repaint_until_eframe_closes() {
        use std::time::Duration;
        let mut app = App::new();
        app.quit_confirmed = true;
        let ctx = egui::Context::default();
        // Même sans nouvel événement de fermeture, tant qu'on n'a pas
        // effectivement quitté on continue à réclamer un rendu immédiat.
        let out = ctx.run(Default::default(), |ctx| app.check_close_request(ctx));
        let delay = out.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        assert_eq!(delay, Duration::ZERO, "un réveil immédiat doit être programmé");
    }

    // ---------- Blocage de la fermeture tant qu'il reste du travail ----------

    /// Une application dont le tampon diffère de ce qui est sur le disque.
    fn app_with_unsaved_work() -> App {
        let mut app = App::new();
        app.source.push_str("\n    nop      ; le travail de l'élève\n");
        assert!(app.dirty());
        app
    }

    /// Le cas qui faisait perdre du travail : fermer avec un exercice à moitié
    /// écrit partait sans un mot.
    #[test]
    fn closing_with_unsaved_work_is_cancelled_and_asks() {
        let mut app = app_with_unsaved_work();
        let ctx = egui::Context::default();
        let out = ctx.run(close_requested_input(), |ctx| app.check_close_request(ctx));

        assert_eq!(
            app.unsaved_prompt,
            Some(unsaved::PendingAction::Quit),
            "la question doit porter sur la fermeture"
        );
        let commands = &out.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(commands.contains(&egui::ViewportCommand::CancelClose));
    }

    /// Le travail passe avant la licence : les deux boîtes s'ouvriraient sinon
    /// l'une par-dessus l'autre sur le même événement de fermeture.
    #[test]
    fn unsaved_work_takes_precedence_over_the_license_nag() {
        let mut app = app_with_unsaved_work();
        assert!(!app.is_licensed());
        let ctx = egui::Context::default();
        let _ = ctx.run(close_requested_input(), |ctx| app.check_close_request(ctx));
        assert!(!app.show_license_nag, "une seule question à la fois");
        assert!(app.unsaved_prompt.is_some());
    }

    /// Une licence en règle ne dispense pas de la question : c'est le travail
    /// qu'on protège, pas l'enregistrement du produit.
    #[test]
    fn a_licensed_user_is_asked_too() {
        let mut app = app_with_unsaved_work();
        app.license = crate::license::valid_for_tests();
        let ctx = egui::Context::default();
        let out = ctx.run(close_requested_input(), |ctx| app.check_close_request(ctx));
        assert!(app.unsaved_prompt.is_some());
        let commands = &out.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(commands.contains(&egui::ViewportCommand::CancelClose));
    }

    /// Régression : « Abandonner » réémet un `Close` qui repasse ici. Sans
    /// `discard_confirmed`, le tampon étant toujours modifié, la question se
    /// reposerait indéfiniment et l'application ne quitterait jamais.
    #[test]
    fn a_confirmed_discard_lets_the_close_through() {
        let mut app = app_with_unsaved_work();
        app.license = crate::license::valid_for_tests();
        app.discard_confirmed = true;
        let ctx = egui::Context::default();
        let out = ctx.run(close_requested_input(), |ctx| app.check_close_request(ctx));

        assert!(app.unsaved_prompt.is_none(), "la question ne doit pas se reposer");
        let commands = &out.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(!commands.contains(&egui::ViewportCommand::CancelClose));
    }

    /// Rien à perdre : la fermeture d'un tampon propre n'est pas interceptée.
    #[test]
    fn closing_a_clean_buffer_is_never_intercepted_for_that_reason() {
        let mut app = App::new();
        app.license = crate::license::valid_for_tests();
        assert!(!app.dirty());
        let ctx = egui::Context::default();
        let out = ctx.run(close_requested_input(), |ctx| app.check_close_request(ctx));
        assert!(app.unsaved_prompt.is_none());
        let commands = &out.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(!commands.contains(&egui::ViewportCommand::CancelClose));
    }

    #[test]
    fn a_frame_without_close_event_touches_nothing() {
        let mut app = App::new();
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.check_close_request(ctx));
        assert!(!app.show_license_nag);
        assert!(!app.exit_pending);
    }
}
