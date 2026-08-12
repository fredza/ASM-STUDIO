//! Briques d'interface réutilisables, sans état.
//!
//! Tout ce qui est purement visuel et ne touche pas à [`App`] vit ici : en-têtes
//! de panneau à hauteur constante (pour que les séparateurs de tous les panneaux
//! restent alignés), cartes, badges, boutons à icône et vidage hexadécimal.
//!
//! Ces fonctions prennent leurs couleurs en paramètre plutôt que de lire le
//! thème : c'est l'appelant qui décide, via les accesseurs `App::c_*`.

use eframe::egui::{self, Color32, RichText};

use crate::debugger::Debugger;
use crate::explain;

use super::{action, changed_col, false_col, flag_on};
use super::paths::{file_name, is_asm, list_entries};
use std::path::{Path, PathBuf};

/// Hauteur fixe de la ligne d'en-tête d'un panneau, pour aligner les
/// séparateurs de tous les panneaux au même niveau (certains en-têtes ont des
/// boutons/combos plus hauts qu'un simple libellé).
pub(super) const HEADER_H: f32 = 24.0;

/// En-tête de panneau à hauteur fixe : rend `content` (titre + éventuels
/// contrôles) dans une ligne de `HEADER_H`, puis un séparateur. Tous les
/// panneaux passent par ici → leurs séparateurs sont alignés.
pub(super) fn panel_header(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(2.0);
    // Bandeau de titre à la fois léger et bien délimité : l'œil repère les
    // groupes sans confondre cet en-tête avec une carte de contenu. La bordure
    // est volontairement plus douce que celle d'un champ éditable.
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(egui::Stroke::new(1.0_f32, crate::theme::current().ui.border.gamma_multiply(0.72)))
        .inner_margin(egui::Margin::symmetric(8, 3))
        .corner_radius(egui::CornerRadius::same(7))
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

/// Le gabarit commun des boîtes de dialogue : jamais repliable, et centrée sur
/// la zone de travail — pas sur l'écran, ni là où la précédente a été laissée :
/// c'est au milieu du travail en cours que l'œil se trouve déjà.
///
/// Ne fixe que ce que les quinze boîtes de l'application ont en commun. Le
/// reste — taille de départ, redimensionnement, croix de fermeture — s'ajoute
/// derrière, en enchaînant le builder d'egui comme d'habitude.
pub(super) fn dialog_window<'a>(
    ctx: &egui::Context,
    title: impl Into<egui::WidgetText>,
) -> egui::Window<'a> {
    egui::Window::new(title)
        .collapsible(false)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
}

/// Encadré « carte » moderne : fond légèrement teinté, coins arrondis et marge
/// interne, sur toute la largeur disponible. Structure et aère le contenu
/// (utile pour une app pédagogique).
pub(super) fn card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color.linear_multiply(0.96))
        .stroke(egui::Stroke::new(1.0_f32, crate::theme::current().ui.border.gamma_multiply(0.58)))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .corner_radius(egui::CornerRadius::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            content(ui);
        });
}




/// Ce que l'appel système va faire, déplié : la phrase d'effet, puis chaque
/// registre avec le rôle qu'il joue ici, le contenu du tampon s'il y en a un,
/// ce que RAX vaudra au retour, et le piège éventuel.
///
/// `buf` est le contenu déjà lu du tampon décrit par `d.buffer` — la lecture
/// mémoire revient à l'appelant, qui seul sait si le processus est vivant et
/// si la vue affichée est bien celle du présent.
pub(super) fn syscall_details(
    ui: &mut egui::Ui,
    d: &crate::syscall::Description,
    buf: Option<&[u8]>,
    skin: SyscallSkin,
) {
    let SyscallSkin { hdr, mnem, addr_c, bytes_c, labels } = skin;
    // La phrase d'effet d'abord : c'est elle qu'on lit, les registres ne
    // viennent qu'ensuite confirmer d'où sortent les valeurs.
    egui::Frame::default()
        .fill(action().linear_multiply(0.12))
        .stroke(egui::Stroke::new(1.0_f32, action().linear_multiply(0.6)))
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(labels.title).small().strong().color(action()));
                ui.label(RichText::new(d.name).monospace().strong().color(mnem));
            });
            ui.add_space(2.0);
            ui.label(RichText::new(&d.summary).size(12.5));
        });

    // Un registre par ligne : nom, argument, valeur lisible ; le rôle en
    // dessous, en petit. Une grille à trois colonnes serrerait trop les
    // valeurs longues (chemins, adresses).
    ui.add_space(6.0);
    ui.label(RichText::new(labels.args).small().strong().color(hdr));
    for a in &d.args {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(RichText::new(a.reg).monospace().strong().color(mnem));
            if !a.param.is_empty() {
                ui.label(RichText::new(a.param).monospace().small().color(bytes_c));
            }
            ui.label(RichText::new("=").small().weak());
            ui.label(RichText::new(&a.value).monospace().color(addr_c));
        });
        ui.horizontal_wrapped(|ui| {
            ui.add_space(12.0);
            ui.label(RichText::new(&a.role).small().weak());
        });
        ui.add_space(3.0);
    }

    // Le tampon : c'est ici que `msg` cesse d'être une adresse et redevient le
    // texte tapé dans `.data`.
    if let Some(b) = &d.buffer
        && let Some(bytes) = buf
    {
        ui.add_space(4.0);
        card(ui, |ui| {
            ui.label(RichText::new(b.label).small().strong().color(hdr));
            ui.add_space(2.0);
            if b.as_text {
                let text = crate::syscall::text_preview(bytes, 120);
                ui.add(
                    egui::Label::new(
                        RichText::new(if text.is_empty() { "—".to_string() } else { format!("\"{text}\"") })
                            .monospace()
                            .color(changed_col()),
                    )
                    .wrap(),
                );
            }
            let hex = bytes.iter().take(16).map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ");
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("0x{:X}", b.addr)).monospace().small().color(addr_c));
                ui.label(RichText::new(hex).monospace().small().color(bytes_c));
            });
        });
    }

    // Ce que RAX vaudra APRÈS : la question qui suit immédiatement l'appel.
    if let Some(ret) = &d.ret {
        ui.add_space(6.0);
        ui.label(RichText::new(labels.ret).small().strong().color(hdr));
        ui.label(RichText::new(ret).size(12.0));
    }

    // Le piège, s'il y en a un pour ces valeurs-là.
    if let Some(note) = &d.note {
        ui.add_space(6.0);
        egui::Frame::default()
            .fill(false_col().linear_multiply(0.14))
            .stroke(egui::Stroke::new(1.0_f32, false_col().linear_multiply(0.5)))
            .corner_radius(egui::CornerRadius::same(5))
            .inner_margin(egui::Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(format!("⚠ {note}")).size(12.0).color(false_col()));
            });
    }
}

/// Couleurs et intitulés de [`syscall_details`], réunis : le bloc en demande
/// six, et six paramètres de plus rendaient l'appel illisible des deux côtés.
pub(super) struct SyscallSkin {
    pub(super) hdr: Color32,
    pub(super) mnem: Color32,
    pub(super) addr_c: Color32,
    pub(super) bytes_c: Color32,
    pub(super) labels: SyscallLabels,
}

/// Les intitulés de [`syscall_details`]. Le rendu ne connaît pas la langue :
/// il reçoit ses titres tout faits, comme les autres widgets reçoivent leurs
/// couleurs.
pub(super) struct SyscallLabels {
    pub(super) title: &'static str,
    pub(super) args: &'static str,
    pub(super) ret: &'static str,
}

/// Les intitulés traduits, en un appel — deux panneaux affichent ce bloc.
pub(super) fn syscall_labels(lang: crate::i18n::Lang) -> SyscallLabels {
    use crate::i18n::tr3;
    SyscallLabels {
        title: tr3(lang, "⚙ Ce que fait cet appel", "⚙ What this call does", "⚙ Lo que hace esta llamada"),
        args: tr3(lang, "Arguments, registre par registre", "Arguments, register by register", "Argumentos, registro por registro"),
        ret: tr3(lang, "Au retour", "On return", "Al retornar"),
    }
}

/// Affiche une petite icône carrée (rien si `icon` est `None`).
pub(super) fn icon_img(ui: &mut egui::Ui, icon: Option<&egui::TextureHandle>, size: f32) {
    if let Some(t) = icon {
        ui.add(egui::Image::new((t.id(), egui::vec2(size, size))));
    }
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
        ui.label(RichText::new(e.affects_flags.join("  ")).monospace().color(changed_col()));
    }
}

/// Une ligne visible de l'explorateur. L'arbre est aplati avant le rendu pour
/// pouvoir utiliser `ScrollArea::show_rows` : même un gros dossier ne redessine
/// alors que les lignes réellement visibles pendant le défilement.
#[derive(Clone)]
pub(super) struct ExplorerEntry {
    pub(super) path: PathBuf,
    pub(super) depth: usize,
    pub(super) is_dir: bool,
}

/// Action demandée par une ligne de l'explorateur. Les mutations disque restent
/// dans `App`, jamais dans ce widget sans état.
pub(super) enum ExplorerAction {
    Open(PathBuf),
    Select(PathBuf),
    Navigate(PathBuf),
    Rename(PathBuf),
    Delete(PathBuf),
}

/// Les trois libellés du menu contextuel, fournis déjà traduits par le panneau.
pub(super) struct ExplorerRowLabels<'a> {
    pub(super) open_folder: &'a str,
    pub(super) rename: &'a str,
    pub(super) delete: &'a str,
}

/// Les deux teintes d'une ligne : ce qui est de l'assembleur (dossiers et
/// sources `.asm`) et le reste. Elles viennent du thème, que ce module ne
/// connaît pas — comme les libellés viennent de la langue, qu'il ne connaît
/// pas davantage.
pub(super) struct ExplorerRowColors {
    pub(super) asm: Color32,
    pub(super) other: Color32,
}

/// Aplatisse les seuls dossiers actuellement dépliés. L'état de dépliage est
/// persisté par egui, et n'est lu qu'une fois par dossier ouvert — contrairement
/// à l'ancien `CollapsingHeader` récursif rendu entièrement à chaque frame.
pub(super) fn explorer_entries(ui: &egui::Ui, root: &Path) -> Vec<ExplorerEntry> {
    fn visit(ui: &egui::Ui, dir: &Path, depth: usize, out: &mut Vec<ExplorerEntry>) {
        let (dirs, files) = list_entries(dir);
        for path in dirs {
            let id = ui.make_persistent_id(("explorer_open", &path));
            let open = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false).is_open();
            out.push(ExplorerEntry { path: path.clone(), depth, is_dir: true });
            if open {
                visit(ui, &path, depth + 1, out);
            }
        }
        out.extend(files.into_iter().map(|path| ExplorerEntry { path, depth, is_dir: false }));
    }

    let mut entries = Vec::new();
    visit(ui, root, 0, &mut entries);
    entries
}

/// Rend une ligne de l'explorateur moderne : sélection, dépliage, menu contextuel
/// et renommage directement dans l'arbre.
pub(super) fn explorer_row(
    ui: &mut egui::Ui,
    entry: &ExplorerEntry,
    selected: bool,
    scroll_to_selected: bool,
    rename_input: Option<&mut String>,
    labels: ExplorerRowLabels<'_>,
    colors: ExplorerRowColors,
) -> Option<ExplorerAction> {
    let mut action = None;
    // Une grille à colonnes fixes, et non une succession de widgets dont la
    // largeur dépend du glyphe : c'est ce qui garde chevrons, icônes et noms
    // parfaitement alignés entre fichiers et dossiers.
    let row_width = ui.available_width();
    ui.allocate_ui_with_layout(
        egui::vec2(row_width, 22.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
        ui.add_space(entry.depth as f32 * 16.0);
        if entry.is_dir {
            let id = ui.make_persistent_id(("explorer_open", &entry.path));
            let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
            if ui
                .add_sized(
                    [18.0, 22.0],
                    egui::Button::new(if state.is_open() { "⌄" } else { "›" }).frame(false),
                )
                .clicked()
            {
                state.toggle(ui);
                state.store(ui.ctx());
            }
        } else {
            // Un fichier est le contenu du dossier affiché : son icône démarre
            // directement à la marge de son niveau, sans une colonne de flèche
            // vide. C'est la lecture attendue d'un explorateur moderne.
        }
        // Colonne réservée à l'icône : aucun décalage lorsque le système de
        // polices donne une largeur différente aux pictogrammes emoji.
        ui.add_sized(
            [20.0, 22.0],
            egui::Label::new(RichText::new(if entry.is_dir { "🗀" } else { "🗎" }).color(if entry.is_dir { colors.asm } else { colors.other })),
        );
        ui.add_space(2.0);

        if let Some(input) = rename_input {
            let response = ui.add_sized(
                [ui.available_width(), 22.0],
                egui::TextEdit::singleline(input).id_salt(("explorer_rename", &entry.path)),
            );
            response.request_focus();
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                action = Some(ExplorerAction::Rename(entry.path.clone()));
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                action = Some(ExplorerAction::Select(entry.path.clone()));
            }
            return;
        }

        let color = if selected {
            changed_col()
        } else if entry.is_dir || is_asm(&entry.path) {
            colors.asm
        } else {
            colors.other
        };
        // `Button` centre son libellé dans la largeur restante. Une ligne
        // d'explorateur doit au contraire partir de sa marge, quel que soit le
        // nom du fichier. On gère donc l'interaction et le fond de sélection
        // séparément, puis on pose un `Label` explicitement aligné à gauche.
        let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), egui::Sense::click());
        let visuals = ui.style().interact_selectable(&response, selected);
        if selected || response.hovered() {
            ui.painter().rect_filled(rect, egui::CornerRadius::same(4), visuals.weak_bg_fill);
        }
        let painter = ui.painter().with_clip_rect(rect.shrink2(egui::vec2(5.0, 0.0)));
        let galley = painter.layout_no_wrap(
            file_name(&entry.path),
            egui::TextStyle::Button.resolve(ui.style()),
            color,
        );
        painter.galley(
            egui::pos2(rect.left() + 5.0, rect.center().y - galley.size().y * 0.5),
            galley,
            color,
        );
        if selected && scroll_to_selected {
            response.scroll_to_me(Some(egui::Align::Center));
        }
        if response.double_clicked() {
            action = Some(if entry.is_dir {
                ExplorerAction::Navigate(entry.path.clone())
            } else {
                ExplorerAction::Open(entry.path.clone())
            });
        } else if response.clicked() {
            action = Some(if entry.is_dir {
                ExplorerAction::Select(entry.path.clone())
            } else {
                ExplorerAction::Open(entry.path.clone())
            });
        }
        response.context_menu(|ui| {
            if entry.is_dir && ui.button(labels.open_folder).clicked() {
                action = Some(ExplorerAction::Navigate(entry.path.clone()));
                ui.close();
            }
            if ui.button(labels.rename).clicked() {
                action = Some(ExplorerAction::Rename(entry.path.clone()));
                ui.close();
            }
            if ui.button(labels.delete).clicked() {
                action = Some(ExplorerAction::Delete(entry.path.clone()));
                ui.close();
            }
        });
    },
    );
    action
}

/// Bouton avec bordure verte (actif/disponible) ou rouge (inactif).
pub(super) fn bordered_button(
    ui: &mut egui::Ui,
    icon: Option<&egui::TextureHandle>,
    label: &str,
    enabled: bool,
) -> egui::Response {
    let color = if enabled { flag_on() } else { false_col() };
    let btn = match btn_icon(icon) {
        Some(img) => egui::Button::image_and_text(img, label),
        None => egui::Button::new(label),
    }
    .stroke(egui::Stroke::new(1.5_f32, color));
    ui.add_enabled(enabled, btn)
}

/// Source d'image dimensionnée pour un bouton (16px), à partir d'une icône.
pub(super) fn btn_icon(icon: Option<&egui::TextureHandle>) -> Option<egui::load::SizedTexture> {
    icon.map(|t| egui::load::SizedTexture::new(t.id(), egui::vec2(16.0, 16.0)))
}

/// Bouton d'accent (fond accent() si actif, grisé sinon) — pour Run et Step.
pub(super) fn accent_button(
    ui: &mut egui::Ui,
    icon: Option<&egui::TextureHandle>,
    label: &str,
    enabled: bool,
) -> egui::Response {
    let btn = match (enabled, btn_icon(icon)) {
        (true, Some(img)) => {
            egui::Button::image_and_text(img, RichText::new(label).color(Color32::WHITE)).fill(action())
        }
        (true, None) => egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(action()),
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
    .corner_radius(egui::CornerRadius::same(6));
    ui.add(btn)
}

/// Petit badge coloré (texte sur fond semi-transparent).
pub(super) fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::default()
        .fill(color.linear_multiply(0.22))
        .inner_margin(egui::Margin::symmetric(5, 2))
        .corner_radius(egui::CornerRadius::same(4))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small().strong().color(color));
        });
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
