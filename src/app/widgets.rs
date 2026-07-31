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

use super::{ACTION, CHANGED, FALSE_COL, FLAG_ON};
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
        ui.label(RichText::new(e.affects_flags.join("  ")).monospace().color(CHANGED));
    }
}

/// Rend récursivement l'arbre d'un dossier (style explorateur d'IDE) : dossiers
/// repliables (`CollapsingHeader`), puis fichiers cliquables. Le fichier ouvert
/// est surligné ; le clic sur un fichier renseigne `to_open`.
pub(super) fn dir_tree(
    ui: &mut egui::Ui,
    dir: &Path,
    current: &Path,
    // Amène `current` à l'écran : demandé après un déplacement au clavier.
    scroll_to_current: bool,
    asm_col: Color32,
    other_col: Color32,
    to_open: &mut Option<PathBuf>,
) {
    let (dirs, files) = list_entries(dir);
    for d in dirs {
        egui::CollapsingHeader::new(RichText::new(format!("🗀  {}", file_name(&d))).color(asm_col))
            .id_salt(&d)
            .default_open(false)
            .show(ui, |ui| {
                dir_tree(ui, &d, current, scroll_to_current, asm_col, other_col, to_open)
            });
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
        let resp = ui.add(egui::SelectableLabel::new(is_cur, label));
        if is_cur && scroll_to_current {
            resp.scroll_to_me(Some(egui::Align::Center));
        }
        if resp.clicked() {
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

