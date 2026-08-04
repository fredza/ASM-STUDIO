//! Disposition ancrable : chaque panneau est un onglet déplaçable.
//!
//! Les panneaux vivaient dans des `SidePanel` / `TopBottomPanel` figés : leur
//! place et leur taille étaient décidées dans le code, et l'élève ne pouvait que
//! les masquer. Ici, chaque panneau devient un onglet d'un arbre `egui_dock` —
//! on le glisse ailleurs, on l'empile avec un autre, ou on le sort en fenêtre
//! flottante, à la manière de Photoshop.
//!
//! Le rendu de chaque panneau n'a pas bougé : [`TabViewer::ui`] appelle les
//! mêmes méthodes `App::*_ui` qu'avant. Seul le conteneur change.

use eframe::egui::{self, WidgetText};
use egui_dock::{DockState, NodeIndex, SurfaceIndex, TabViewer};

use super::App;
use crate::i18n::{self, Lang};

/// Un panneau ancrable. Chaque variante correspond à une méthode `App::*_ui`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Panel {
    Editor,
    Disasm,
    MemMap,
    Explorer,
    Instruction,
    Flags,
    Registers,
    Stack,
    Memory,
    Timeline,
    Console,
    CallStack,
    Syscalls,
    Exercise,
}

impl Panel {
    /// Tous les panneaux, dans l'ordre du menu Affichage.
    pub(crate) const ALL: [Panel; 14] = [
        Panel::Editor,
        Panel::Disasm,
        Panel::MemMap,
        Panel::Explorer,
        Panel::Instruction,
        Panel::Flags,
        Panel::Registers,
        Panel::Stack,
        Panel::Memory,
        Panel::Timeline,
        Panel::Console,
        Panel::CallStack,
        Panel::Syscalls,
        Panel::Exercise,
    ];

    /// Titre affiché sur l'onglet.
    pub(crate) fn title(self, lang: Lang) -> String {
        let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        match self {
            Panel::Editor => t("Éditeur", "Editor", "Editor"),
            Panel::Disasm => t("Désassemblage", "Disassembly", "Desensamblado"),
            Panel::MemMap => t("Vue mémoire", "Memory View", "Vista memoria"),
            Panel::Explorer => t("Explorateur", "Explorer", "Explorador"),
            Panel::Instruction => t("Instruction", "Instruction", "Instrucción"),
            Panel::Flags => "Flags",
            Panel::Registers => t("Registres", "Registers", "Registros"),
            Panel::Stack => t("Pile / Tas", "Stack / Heap", "Pila / Montículo"),
            Panel::Memory => t("Mémoire", "Memory", "Memoria"),
            Panel::Timeline => t("Timeline", "Timeline", "Línea de tiempo"),
            Panel::Console => "Console",
            Panel::CallStack => t("Pile d'appels", "Call stack", "Pila de llamadas"),
            Panel::Syscalls => t("Appels système", "Syscalls", "Llamadas al sistema"),
            Panel::Exercise => t("Exercices", "Exercises", "Ejercicios"),
        }
        .to_string()
    }

    /// Clé stable pour la persistance de la disposition (indépendante de la langue).
    pub(crate) fn key(self) -> &'static str {
        match self {
            Panel::Editor => "editor",
            Panel::Disasm => "disasm",
            Panel::MemMap => "memmap",
            Panel::Explorer => "explorer",
            Panel::Instruction => "instruction",
            Panel::Flags => "flags",
            Panel::Registers => "registers",
            Panel::Stack => "stack",
            Panel::Memory => "memory",
            Panel::Timeline => "timeline",
            Panel::Console => "console",
            Panel::CallStack => "callstack",
            Panel::Syscalls => "syscalls",
            Panel::Exercise => "exercise",
        }
    }

    pub(crate) fn from_key(k: &str) -> Option<Panel> {
        Panel::ALL.into_iter().find(|p| p.key() == k)
    }
}

/// Disposition par défaut : reproduit l'agencement historique de l'application,
/// pour que rien ne dépayse au premier lancement.
///
/// ```text
///   ┌──────────┬──────────────────────────┬─────────────┐
///   │ EXPLORER │  Éditeur / Désasm /      │ Instruction │
///   │          │  Vue mémoire             │ / Exercice  │
///   │          ├──────────────────────────┤             │
///   │          │ Registres│Flags│Pile│…   │             │
///   │          ├──────────────────────────┤             │
///   │          │ Mémoire │Timeline│Console│             │
///   └──────────┴──────────────────────────┴─────────────┘
/// ```
pub(crate) fn default_layout() -> DockState<Panel> {
    // Surface principale : le centre, avec ses trois onglets empilés.
    let mut state = DockState::new(vec![Panel::Editor, Panel::Disasm, Panel::MemMap]);
    let surface = state.main_surface_mut();

    // Explorateur à gauche du centre.
    let [center, _explorer] = surface.split_left(NodeIndex::root(), 0.16, vec![Panel::Explorer]);
    // Instruction et Exercice à droite, sur toute la hauteur : le panneau a
    // retrouvé la place que lui prenait le doublon de FLAGS.
    let [center, _right] =
        surface.split_right(center, 0.78, vec![Panel::Instruction, Panel::Exercise]);
    // Bande CPU sous le centre. FLAGS y rejoint les registres : les drapeaux
    // sont de l'état processeur, ils se lisent avec eux — et non dans un coin.
    let [center, cpu] = surface.split_below(
        center,
        0.52,
        vec![
            Panel::Registers,
            Panel::Flags,
            Panel::Stack,
            Panel::CallStack,
            Panel::Syscalls,
        ],
    );
    // Bande basse sous la bande CPU.
    surface.split_below(cpu, 0.52, vec![Panel::Memory, Panel::Timeline, Panel::Console]);
    let _ = center;
    state
}

/// Panneaux réservés au mode complet : ils supposent des notions qui viennent
/// plus tard (code machine, adressage, conventions d'appel, appels système).
pub(crate) const ADVANCED: [Panel; 5] = [
    Panel::Disasm,
    Panel::MemMap,
    Panel::Memory,
    Panel::CallStack,
    Panel::Syscalls,
];

/// Disposition du mode apprentissage : neuf panneaux au lieu de quatorze.
///
/// ```text
///   ┌──────────┬────────────────────────┬─────────────┐
///   │ EXPLORER │  Éditeur               │ Instruction │
///   │          │                        │ / Exercice  │
///   │          ├────────────────────────┤             │
///   │          │ Registres │Flags│ Pile │             │
///   │          ├────────────────────────┤             │
///   │          │ Console │ Timeline     │             │
///   └──────────┴────────────────────────┴─────────────┘
/// ```
pub(crate) fn learning_layout() -> DockState<Panel> {
    let mut state = DockState::new(vec![Panel::Editor]);
    let surface = state.main_surface_mut();
    let [center, _explorer] = surface.split_left(NodeIndex::root(), 0.17, vec![Panel::Explorer]);
    let [center, _right] =
        surface.split_right(center, 0.72, vec![Panel::Instruction, Panel::Exercise]);
    let [center, cpu] = surface.split_below(
        center,
        0.55,
        vec![Panel::Registers, Panel::Flags, Panel::Stack],
    );
    surface.split_below(cpu, 0.55, vec![Panel::Console, Panel::Timeline]);
    let _ = center;
    state
}

/// Disposition correspondant au mode demandé.
pub(crate) fn layout_for(mode: super::UiMode) -> DockState<Panel> {
    match mode {
        super::UiMode::Learning => learning_layout(),
        super::UiMode::Full => default_layout(),
    }
}

/// Adaptateur entre `egui_dock` et les méthodes de rendu de [`App`].
pub(super) struct Viewer<'a> {
    pub(super) app: &'a mut App,
}

impl TabViewer for Viewer<'_> {
    type Tab = Panel;

    fn title(&mut self, tab: &mut Panel) -> WidgetText {
        tab.title(self.app.lang).into()
    }

    /// Id stable et indépendant de la langue : changer de langue ne doit pas
    /// faire perdre sa place à un onglet.
    fn id(&mut self, tab: &mut Panel) -> egui::Id {
        egui::Id::new(("dock_panel", tab.key()))
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Panel) {
        let app = &mut *self.app;
        // Disasm, Flags, Registers et Timeline sont réservés aux licences
        // ASM Studio (voir `crate::license`) : sans licence valide, le contenu
        // réel cède la place à `locked_panel_ui` (src/app/ui_windows.rs).
        match tab {
            Panel::Editor => app.editor_tab_ui(ui),
            Panel::Disasm => if app.is_licensed() { app.disasm_ui(ui) } else { app.locked_panel_ui(ui) },
            Panel::MemMap => app.memory_map_ui(ui),
            Panel::Explorer => app.explorer_ui(ui),
            Panel::Instruction => app.instruction_ui(ui),
            Panel::Flags => if app.is_licensed() { app.flags_ui(ui) } else { app.locked_panel_ui(ui) },
            Panel::Registers => if app.is_licensed() { app.registers_ui(ui) } else { app.locked_panel_ui(ui) },
            Panel::Stack => app.stack_ui(ui),
            Panel::Memory => app.memory_ui(ui),
            Panel::Timeline => if app.is_licensed() { app.timeline_col_ui(ui) } else { app.locked_panel_ui(ui) },
            Panel::Console => app.console_ui(ui),
            Panel::CallStack => app.callstack_ui(ui),
            Panel::Syscalls => app.syscalls_ui(ui),
            Panel::Exercise => app.exercise_ui(ui),
        }
    }

    /// Tout panneau peut être fermé : on le rouvre depuis le menu Affichage.
    fn closeable(&mut self, _tab: &mut Panel) -> bool {
        true
    }

    /// Tout panneau peut être détaché en fenêtre flottante.
    fn allowed_in_windows(&self, _tab: &mut Panel) -> bool {
        true
    }
}

impl App {
    /// Panneau actuellement ouvert quelque part dans la disposition ?
    pub(super) fn panel_is_open(&self, panel: Panel) -> bool {
        self.dock
            .as_ref()
            .is_some_and(|d| d.iter_all_tabs().any(|(_, t)| *t == panel))
    }

    /// Affiche le panneau : le met au premier plan s'il existe déjà, sinon
    /// l'ajoute en fenêtre flottante (sans bousculer la disposition en place).
    pub(super) fn show_panel(&mut self, panel: Panel) {
        let Some(dock) = self.dock.as_mut() else { return };
        if let Some((surface, node, tab)) = dock.find_tab(&panel) {
            dock.set_active_tab((surface, node, tab));
        } else {
            dock.add_window(vec![panel]);
        }
    }

    /// Ferme toutes les occurrences d'un panneau.
    pub(super) fn hide_panel(&mut self, panel: Panel) {
        let Some(dock) = self.dock.as_mut() else { return };
        while let Some(loc) = dock.find_tab(&panel) {
            dock.remove_tab(loc);
        }
    }

    pub(super) fn toggle_panel(&mut self, panel: Panel) {
        if self.panel_is_open(panel) {
            self.hide_panel(panel);
        } else {
            self.show_panel(panel);
        }
    }

    /// Panneau actif du nœud qui a le focus clavier.
    pub(super) fn focused_panel(&mut self) -> Option<Panel> {
        self.dock.as_mut()?.find_active_focused().map(|(_, t)| *t)
    }

    /// Fait défiler les onglets du nœud qui a le focus (Ctrl+Tab).
    ///
    /// Cycle DANS le nœud plutôt qu'entre panneaux quelconques : c'est le
    /// comportement attendu d'une barre d'onglets, et il suit l'utilisateur
    /// quand il réorganise sa disposition.
    pub(super) fn cycle_tab(&mut self, backwards: bool) {
        let Some(dock) = self.dock.as_mut() else { return };
        let Some((surface, node)) = dock.focused_leaf() else { return };
        let n = dock[surface][node].tabs_count();
        if n < 2 {
            return;
        }
        let active = match &dock[surface][node] {
            egui_dock::Node::Leaf(leaf) => leaf.active.0,
            _ => return,
        };
        let next = if backwards { (active + n - 1) % n } else { (active + 1) % n };
        dock.set_active_tab((surface, node, egui_dock::TabIndex(next)));
    }

    /// Donne le focus clavier au panneau et le met au premier plan.
    pub(super) fn focus_panel(&mut self, panel: Panel) {
        let Some(dock) = self.dock.as_mut() else { return };
        if let Some((surface, node, tab)) = dock.find_tab(&panel) {
            dock.set_active_tab((surface, node, tab));
            dock.set_focused_node_and_surface((surface, node));
        }
    }

    /// Ordre de parcours au clavier : les panneaux tels qu'ils apparaissent
    /// dans l'arbre, surface principale d'abord.
    pub(super) fn focus_order(&self) -> Vec<Panel> {
        let Some(dock) = self.dock.as_ref() else { return Vec::new() };
        let mut docked = Vec::new();
        let mut windowed = Vec::new();
        for ((surface, _), t) in dock.iter_all_tabs() {
            if surface == SurfaceIndex::main() {
                docked.push(*t);
            } else {
                windowed.push(*t);
            }
        }
        docked.extend(windowed);
        docked
    }

    /// F6 / Maj+F6 : passe au panneau suivant (ou précédent) de la disposition.
    ///
    /// C'est le point d'entrée du clavier dans l'application : sans lui, on ne
    /// peut atteindre qu'un panneau à la souris.
    pub(super) fn focus_next_panel(&mut self, backwards: bool) {
        let order = self.focus_order();
        if order.is_empty() {
            return;
        }
        let current = self.focused_panel();
        let i = current
            .and_then(|c| order.iter().position(|p| *p == c))
            .unwrap_or(0);
        let n = order.len();
        // Sans panneau focalisé, F6 saisit le premier plutôt que le second.
        let next = match current {
            None => order[0],
            Some(_) if backwards => order[(i + n - 1) % n],
            Some(_) => order[(i + 1) % n],
        };
        self.focus_panel(next);
        // Le focus d'un widget resterait sinon capté par l'éditeur.
        if next != Panel::Editor {
            self.ctx_surrender_focus = true;
        }
    }

    /// Rend la zone d'ancrage.
    ///
    /// Le `DockState` est sorti de `self` le temps du rendu : `TabViewer` a
    /// besoin de `&mut App`, et l'état ne peut pas être emprunté deux fois.
    pub(super) fn dock_ui(&mut self, ctx: &egui::Context) {
        let Some(mut dock) = self.dock.take() else { return };
        let mut style = egui_dock::Style::from_egui(&ctx.style());
        style.tab_bar.fill_tab_bar = true;
        style.tab.tab_body.stroke.width = 0.0;
        // Croix de fermeture rouge : l'action est destructrice, elle doit se
        // distinguer du reste de la barre d'onglets. Plus sombre au repos,
        // pleinement rouge au survol — pour ne pas crier en permanence. Sans
        // fond : seul le fin « × » reste, ce qui l'allège nettement (la TAILLE
        // du glyphe, elle, est figée par egui_dock et n'est pas réglable).
        style.buttons.close_tab_color = super::FALSE_COL.gamma_multiply(0.75);
        style.buttons.close_tab_active_color = super::FALSE_COL;
        style.buttons.close_tab_bg_fill = egui::Color32::TRANSPARENT;

        let mut focused_name = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(0.0))
            .show(ctx, |ui| {
                // Une SEULE croix par onglet : la rouge. egui_dock 0.18 ajoute
                // par défaut, sur la barre d'onglets, une croix « tout fermer »
                // et un chevron de repli — d'où la double croix. On les coupe
                // pour retrouver la barre épurée d'avant.
                //
                // `show_window_close_buttons` (déprécié, remplaçant absent en
                // 0.18) reste à false : un panneau détaché garde lui aussi sa
                // seule croix rouge d'onglet, la fenêtre se refermant quand elle
                // se vide. La croix « normale » ne subsiste que sur la fenêtre
                // principale de l'application, fournie par le système.
                #[allow(deprecated)]
                egui_dock::DockArea::new(&mut dock)
                    .style(style)
                    .draggable_tabs(true)
                    .show_close_buttons(true)
                    .show_leaf_close_all_buttons(false)
                    .show_leaf_collapse_buttons(false)
                    .show_window_close_buttons(false)
                    .show_window_collapse_buttons(false)
                    .show_inside(ui, &mut Viewer { app: self });

                // Anneau de focus : sans lui, F6 semble ne rien faire. C'est le
                // seul repère qui dise à l'élève quel panneau reçoit ses touches.
                // Discret à dessein — un filet d'un pixel, en accent très
                // atténué : il situe le focus sans encadrer lourdement le panneau.
                //
                // Peint DANS la couche du panneau central, après le contenu de
                // l'arbre : il passe donc au-dessus des panneaux, mais sous les
                // fenêtres. Sur une couche Foreground il débordait par-dessus
                // « À propos » ou « Raccourcis clavier ».
                if let Some((rect, panel)) = dock.find_active_focused() {
                    focused_name = Some(panel.title(self.lang));
                    ui.painter().rect_stroke(
                        rect.shrink(1.0),
                        4.0,
                        egui::Stroke::new(1.0_f32, super::ACCENT.gamma_multiply(0.35)),
                        egui::StrokeKind::Middle,
                    );
                }
            });
        self.focused_panel_name = focused_name;

        self.dock = Some(dock);
    }


    /// Sérialise la disposition : une ligne par onglet, `surface:clé`.
    ///
    /// On n'enregistre pas la géométrie exacte de l'arbre (l'API d'egui_dock ne
    /// l'expose pas sans serde) mais l'essentiel pour l'élève : quels panneaux
    /// sont ouverts, et lesquels flottent.
    pub(super) fn dock_layout_string(&self) -> String {
        let Some(dock) = self.dock.as_ref() else { return String::new() };
        dock.iter_all_tabs()
            .map(|((surface, _), t)| {
                let kind = if surface == SurfaceIndex::main() { "d" } else { "w" };
                format!("{kind}:{}", t.key())
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Reconstruit une disposition depuis [`App::dock_layout_string`] : on part
    /// de la disposition par défaut, on retire ce qui n'y figure plus, et on
    /// ajoute en fenêtre ce qui était détaché.
    pub(super) fn apply_dock_layout(&mut self, saved: &str) {
        if saved.trim().is_empty() {
            return;
        }
        let mut wanted_docked = Vec::new();
        let mut wanted_windowed = Vec::new();
        for entry in saved.split(',') {
            let Some((kind, key)) = entry.split_once(':') else { continue };
            let Some(p) = Panel::from_key(key.trim()) else { continue };
            if kind.trim() == "w" {
                wanted_windowed.push(p);
            } else {
                wanted_docked.push(p);
            }
        }
        if wanted_docked.is_empty() && wanted_windowed.is_empty() {
            return;
        }
        self.dock = Some(layout_for(self.mode));
        for p in Panel::ALL {
            if !wanted_docked.contains(&p) {
                self.hide_panel(p);
            }
        }
        // Un panneau avancé peut être ouvert ponctuellement depuis le menu
        // Affichage même en mode Apprentissage (voir `view_menu`), mais il ne
        // doit pas ressusciter tout seul au lancement suivant : sinon la
        // première image que voit l'élève est celle du mode complet.
        let skip_advanced = self.mode != super::UiMode::Full;
        for p in wanted_windowed {
            if skip_advanced && ADVANCED.contains(&p) {
                continue;
            }
            if let Some(dock) = self.dock.as_mut() {
                dock.add_window(vec![p]);
            }
        }
    }

    /// Remet la disposition d'origine du mode courant.
    pub(super) fn reset_dock_layout(&mut self) {
        self.dock = Some(layout_for(self.mode));
        self.save_settings();
    }

    /// Bascule vers un mode d'affichage et applique sa disposition.
    ///
    /// Changer de mode REMPLACE la disposition : c'est le sens du réglage.
    /// Sans effet si le mode est déjà celui demandé, pour ne pas détruire un
    /// agencement que l'élève vient de composer.
    pub(super) fn set_ui_mode(&mut self, mode: super::UiMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        self.dock = Some(layout_for(mode));
        self.save_settings();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_panel_has_a_unique_stable_key() {
        let mut keys: Vec<&str> = Panel::ALL.iter().map(|p| p.key()).collect();
        keys.sort_unstable();
        let n = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), n, "clés de panneaux dupliquées");
        // Aller-retour clé → panneau.
        for p in Panel::ALL {
            assert_eq!(Panel::from_key(p.key()), Some(p), "{p:?}");
        }
        assert_eq!(Panel::from_key("inconnu"), None);
    }

    #[test]
    fn panel_titles_exist_in_every_language() {
        for p in Panel::ALL {
            for lang in [Lang::Fr, Lang::En, Lang::Es] {
                assert!(!p.title(lang).is_empty(), "{p:?} sans titre en {lang:?}");
            }
        }
    }

    /// La disposition par défaut doit contenir TOUS les panneaux : sinon un
    /// panneau serait injoignable au premier lancement.
    #[test]
    fn default_layout_contains_every_panel() {
        let state = default_layout();
        let present: Vec<Panel> = state.iter_all_tabs().map(|(_, t)| *t).collect();
        for p in Panel::ALL {
            assert!(present.contains(&p), "{p:?} absent de la disposition par défaut");
        }
        assert_eq!(
            present.len(),
            Panel::ALL.len(),
            "panneau dupliqué ou manquant : {present:?}"
        );
    }

    /// Le centre doit rester la zone principale, avec l'éditeur au premier plan.
    #[test]
    fn default_layout_focuses_the_editor() {
        let state = default_layout();
        let found = state.find_tab(&Panel::Editor);
        assert!(found.is_some(), "l'éditeur doit être présent");
        let (surface, _, _) = found.unwrap();
        assert_eq!(surface, SurfaceIndex::main(), "l'éditeur est dans la surface principale");
    }

    /// Fermer puis rouvrir un panneau doit fonctionner pour chacun d'eux : c'est
    /// le contrat du menu Affichage.
    #[test]
    fn every_panel_can_be_closed_and_reopened() {
        let mut app = App::new();
        app.set_ui_mode(super::super::UiMode::Full);
        for p in Panel::ALL {
            assert!(app.panel_is_open(p), "{p:?} devrait être ouvert au départ");
            app.hide_panel(p);
            assert!(!app.panel_is_open(p), "{p:?} devrait être fermé");
            app.show_panel(p);
            assert!(app.panel_is_open(p), "{p:?} devrait être rouvert");
        }
    }

    /// `hide_panel` retire TOUTES les occurrences : un panneau ajouté deux fois
    /// ne doit pas rester à moitié ouvert.
    #[test]
    fn hiding_removes_every_occurrence() {
        let mut app = App::new();
        if let Some(d) = app.dock.as_mut() {
            d.add_window(vec![Panel::Console]);
        }
        let count = |app: &App| {
            app.dock
                .as_ref()
                .map(|d| d.iter_all_tabs().filter(|(_, t)| **t == Panel::Console).count())
                .unwrap_or(0)
        };
        assert_eq!(count(&app), 2, "console présente deux fois");
        app.hide_panel(Panel::Console);
        assert_eq!(count(&app), 0, "toutes les occurrences doivent partir");
    }

    /// La disposition survit à un aller-retour par la chaîne de persistance,
    /// y compris la distinction ancré / détaché.
    #[test]
    fn layout_round_trips_through_settings() {
        let mut app = App::new();
        app.set_ui_mode(super::super::UiMode::Full);
        app.hide_panel(Panel::Console);
        app.hide_panel(Panel::MemMap);
        if let Some(d) = app.dock.as_mut() {
            d.add_window(vec![Panel::Timeline]);
        }
        app.hide_panel(Panel::Timeline);
        if let Some(d) = app.dock.as_mut() {
            d.add_window(vec![Panel::Timeline]);
        }
        let saved = app.dock_layout_string();
        assert!(saved.contains("w:timeline"), "la timeline détachée : {saved}");
        assert!(!saved.contains("console"), "console fermée : {saved}");

        let mut restored = App::new();
        restored.set_ui_mode(super::super::UiMode::Full);
        restored.apply_dock_layout(&saved);
        assert!(!restored.panel_is_open(Panel::Console), "console doit rester fermée");
        assert!(!restored.panel_is_open(Panel::MemMap), "vue mémoire doit rester fermée");
        assert!(restored.panel_is_open(Panel::Timeline), "timeline doit être restaurée");
        assert!(restored.panel_is_open(Panel::Editor), "éditeur toujours là");
    }

    /// Un panneau avancé détaché (ex. Désassemblage, ouvert une fois depuis
    /// le menu en mode Apprentissage) ne doit pas ressusciter en fenêtre
    /// flottante au lancement suivant : sinon l'élève retrouve le mode
    /// complet sans l'avoir demandé. Signalé après qu'un `w:disasm` oublié
    /// dans `settings.conf` rouvrait la fenêtre à chaque démarrage.
    #[test]
    fn advanced_floating_panel_does_not_survive_a_restart_in_learning_mode() {
        let mut app = App::new();
        assert_eq!(app.mode, super::super::UiMode::Learning);
        if let Some(d) = app.dock.as_mut() {
            d.add_window(vec![Panel::Disasm]);
        }
        let saved = app.dock_layout_string();
        assert!(saved.contains("w:disasm"), "le désassemblage détaché : {saved}");

        let mut restored = App::new();
        restored.apply_dock_layout(&saved);
        assert!(
            !restored.panel_is_open(Panel::Disasm),
            "le désassemblage ne doit pas se rouvrir seul en mode apprentissage"
        );
        assert!(restored.panel_is_open(Panel::Editor), "éditeur toujours là");
    }

    /// Une disposition vide ou illisible ne doit pas effacer les panneaux :
    /// on garde la disposition par défaut plutôt qu'un écran nu.
    #[test]
    fn corrupt_layout_falls_back_to_default() {
        for saved in ["", "   ", "n_importe_quoi", "x:zzz,y:www"] {
            let mut app = App::new();
            app.set_ui_mode(super::super::UiMode::Full);
            app.apply_dock_layout(saved);
            assert!(
                app.panel_is_open(Panel::Editor),
                "disposition « {saved} » ne doit pas vider l'écran"
            );
        }
    }

    /// Réinitialiser rétablit tous les panneaux.
    #[test]
    fn reset_restores_every_panel() {
        let mut app = App::new();
        app.set_ui_mode(super::super::UiMode::Full);
        for p in Panel::ALL {
            app.hide_panel(p);
        }
        assert!(!app.panel_is_open(Panel::Editor));
        app.reset_dock_layout();
        for p in Panel::ALL {
            assert!(app.panel_is_open(p), "{p:?} manquant après réinitialisation");
        }
    }

    /// La chaîne écrite dans les réglages doit être exactement celle que le
    /// lecteur sait reprendre. On ne passe PAS par le fichier réel : `App::new`
    /// lit `XDG_CONFIG_HOME`, et le modifier depuis un test rendrait tous les
    /// autres dépendants de l'ordre d'exécution.
    #[test]
    fn serialized_layout_matches_the_parser() {
        let mut app = App::new();
        app.set_ui_mode(super::super::UiMode::Full);
        app.hide_panel(Panel::Console);
        app.hide_panel(Panel::Syscalls);
        if let Some(d) = app.dock.as_mut() {
            d.add_window(vec![Panel::Timeline]);
        }

        let saved = app.dock_layout_string();
        // Format attendu : « d:clé » ancré, « w:clé » détaché, séparés par des virgules.
        for entry in saved.split(',') {
            let (kind, key) = entry.split_once(':').unwrap_or_else(|| panic!("entrée mal formée : {entry}"));
            assert!(matches!(kind, "d" | "w"), "préfixe inattendu : {kind}");
            assert!(Panel::from_key(key).is_some(), "clé inconnue : {key}");
        }
        assert!(!saved.contains(":console"), "panneau fermé listé : {saved}");
        assert!(!saved.contains(":syscalls"), "panneau fermé listé : {saved}");

        // Et le lecteur restitue exactement cet ensemble.
        let mut restored = App::new();
        restored.set_ui_mode(super::super::UiMode::Full);
        restored.apply_dock_layout(&saved);
        for p in Panel::ALL {
            assert_eq!(
                restored.panel_is_open(p),
                app.panel_is_open(p),
                "{p:?} : état non restitué"
            );
        }
    }

    /// F6 doit atteindre CHAQUE panneau de la disposition, sans en sauter ni
    /// tourner en rond avant d'avoir tout vu. C'est la promesse « tout le
    /// clavier » : sans elle, un panneau resterait accessible à la souris seule.
    #[test]
    fn f6_reaches_every_panel_in_one_cycle() {
        let mut app = App::new();
        app.set_ui_mode(super::super::UiMode::Full);
        let total = app.focus_order().len();
        assert_eq!(total, Panel::ALL.len(), "l'ordre doit couvrir tous les panneaux");

        let mut seen = Vec::new();
        for _ in 0..total {
            app.focus_next_panel(false);
            if let Some(p) = app.focused_panel() {
                seen.push(p);
            }
        }
        for p in Panel::ALL {
            assert!(seen.contains(&p), "{p:?} jamais atteint par F6");
        }
    }

    /// Maj+F6 parcourt le même ensemble en sens inverse.
    #[test]
    fn shift_f6_walks_backwards() {
        let mut app = App::new();
        app.focus_panel(Panel::Editor);
        app.focus_next_panel(false);
        let forward = app.focused_panel();
        app.focus_next_panel(true);
        assert_eq!(app.focused_panel(), Some(Panel::Editor), "retour sur ses pas");
        assert_ne!(forward, Some(Panel::Editor));
    }

    /// Un panneau fermé sort du parcours : F6 ne doit pas s'arrêter sur du vide.
    #[test]
    fn focus_order_skips_closed_panels() {
        let mut app = App::new();
        app.set_ui_mode(super::super::UiMode::Full);
        app.hide_panel(Panel::Console);
        app.hide_panel(Panel::Flags);
        let order = app.focus_order();
        assert!(!order.contains(&Panel::Console), "console fermée mais parcourue");
        assert!(!order.contains(&Panel::Flags), "flags fermé mais parcouru");
        assert_eq!(order.len(), Panel::ALL.len() - 2);
    }

    /// Sans aucun panneau, la navigation ne doit pas paniquer ni boucler.
    #[test]
    fn focus_navigation_is_safe_when_empty() {
        let mut app = App::new();
        for p in Panel::ALL {
            app.hide_panel(p);
        }
        assert!(app.focus_order().is_empty());
        app.focus_next_panel(false);
        app.focus_next_panel(true);
        assert_eq!(app.focused_panel(), None);
    }

    /// Les panneaux détachés en fenêtre restent atteignables au clavier, après
    /// les panneaux ancrés.
    #[test]
    fn detached_panels_stay_reachable() {
        let mut app = App::new();
        app.set_ui_mode(super::super::UiMode::Full);
        app.hide_panel(Panel::Timeline);
        if let Some(d) = app.dock.as_mut() {
            d.add_window(vec![Panel::Timeline]);
        }
        let order = app.focus_order();
        assert!(order.contains(&Panel::Timeline), "panneau détaché injoignable");
        assert_eq!(
            order.last(),
            Some(&Panel::Timeline),
            "les détachés viennent après les ancrés"
        );
    }

    /// Le mode apprentissage doit être réellement plus simple : strictement
    /// moins de panneaux, et aucun panneau avancé.
    #[test]
    fn learning_mode_is_actually_simpler() {
        let learning: Vec<Panel> = learning_layout().iter_all_tabs().map(|(_, t)| *t).collect();
        let full: Vec<Panel> = default_layout().iter_all_tabs().map(|(_, t)| *t).collect();

        assert!(
            learning.len() < full.len(),
            "apprentissage {} panneaux vs complet {}",
            learning.len(),
            full.len()
        );
        for p in ADVANCED {
            assert!(!learning.contains(&p), "{p:?} est avancé, hors du mode apprentissage");
            assert!(full.contains(&p), "{p:?} doit exister en mode complet");
        }
        // Mais l'essentiel doit y être : sans éditeur ni instruction, le mode
        // n'apprendrait rien.
        for p in [Panel::Editor, Panel::Instruction, Panel::Registers, Panel::Console] {
            assert!(learning.contains(&p), "{p:?} manque au mode apprentissage");
        }
        // Aucun doublon.
        let mut u = learning.clone();
        u.sort_by_key(|p| p.key());
        u.dedup();
        assert_eq!(u.len(), learning.len(), "panneau dupliqué : {learning:?}");
    }

    /// Changer de mode remplace la disposition ; rester dans le même mode ne
    /// doit RIEN toucher, sinon on détruirait l'agencement composé par l'élève.
    #[test]
    fn switching_mode_replaces_layout_but_re_selecting_does_not() {
        use super::super::UiMode;
        let mut app = App::new();
        assert_eq!(app.mode, UiMode::Learning, "apprentissage par défaut");
        assert!(!app.panel_is_open(Panel::Disasm), "pas de désassemblage au départ");

        app.set_ui_mode(UiMode::Full);
        assert!(app.panel_is_open(Panel::Disasm), "le mode complet l'ouvre");

        // L'élève ferme un panneau, puis re-sélectionne le mode déjà actif.
        app.hide_panel(Panel::Console);
        app.set_ui_mode(UiMode::Full);
        assert!(!app.panel_is_open(Panel::Console), "sa disposition doit être préservée");

        // Changer réellement de mode remet la disposition du nouveau mode.
        app.set_ui_mode(UiMode::Learning);
        assert!(app.panel_is_open(Panel::Console), "l'apprentissage rétablit la console");
        assert!(!app.panel_is_open(Panel::Syscalls));
    }

    /// La clé du mode doit faire un aller-retour, et une valeur inconnue
    /// retomber sur l'apprentissage plutôt que d'échouer.
    #[test]
    fn mode_key_round_trips() {
        use super::super::UiMode;
        for m in [UiMode::Learning, UiMode::Full] {
            assert_eq!(UiMode::from_key(m.key()), m);
            for lang in [Lang::Fr, Lang::En, Lang::Es] {
                assert!(!m.label(lang).is_empty());
                assert!(!m.description(lang).is_empty());
            }
        }
        assert_eq!(UiMode::from_key("n_importe_quoi"), UiMode::Learning);
    }




    /// « Exemples et exercices » amène l'explorateur INTERNE sur le dossier des
    /// exemples et le rend visible — sans processus externe.
    #[test]
    fn opening_examples_points_the_internal_explorer_there() {
        let mut app = App::new();
        app.open_examples_dir();
        assert!(
            app.explorer_dir.ends_with("examples"),
            "l'explorateur doit pointer sur examples, vu {:?}",
            app.explorer_dir
        );
        assert!(app.panel_is_open(Panel::Explorer), "l'explorateur doit être visible");
    }

    /// Sans licence, Disasm/Flags/Registers/Timeline doivent afficher le
    /// message verrouillé au lieu de leur contenu réel — sans jamais paniquer.
    #[test]
    fn locked_panels_render_without_panicking_when_unlicensed() {
        let mut app = App::new();
        app.set_ui_mode(super::super::UiMode::Full);
        assert!(!app.is_licensed());
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.dock_ui(ctx));
    }

    /// Avec une licence valide, les mêmes panneaux se rendent aussi sans
    /// paniquer (contenu réel cette fois, plus le message verrouillé).
    #[test]
    fn locked_panels_render_without_panicking_when_licensed() {
        let mut app = App::new();
        app.set_ui_mode(super::super::UiMode::Full);
        app.license = crate::license::valid_for_tests();
        assert!(app.is_licensed());
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.dock_ui(ctx));
    }
}
