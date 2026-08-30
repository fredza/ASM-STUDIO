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
use std::collections::HashSet;
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

// ---------------------------------------------------------------------------
// Explorateur de fichiers
//
// L'arbre est dessiné à la main plutôt qu'assemblé à coups de `Button` et de
// `CollapsingHeader` : une ligne d'explorateur est une bande pleine largeur où
// chevron, icône et nom occupent des colonnes fixes, et rien de tout cela ne
// s'obtient d'une suite de widgets dont la largeur dépend du glyphe rendu.
//
// L'état déplié/replié n'est pas non plus dans la mémoire d'egui : il vit dans
// `App::explorer_expanded`, donc le clavier voit exactement l'arbre que la
// souris voit — c'est ce qui manquait pour que ↑/↓ parcourent autre chose que
// la racine.
// ---------------------------------------------------------------------------

/// Hauteur d'une ligne de l'arbre : `ScrollArea::show_rows` a besoin qu'elle
/// soit connue d'avance, panneau et ligne doivent donc lire la même constante.
pub(super) const EXPLORER_ROW_H: f32 = 24.0;

/// Retrait d'un niveau de profondeur.
const EXPLORER_INDENT: f32 = 14.0;

/// Profondeur au-delà de laquelle on cesse de descendre. Un lien symbolique
/// circulaire ferait sinon tourner l'aplatissement jusqu'à la pile.
const EXPLORER_MAX_DEPTH: usize = 24;

/// Une ligne visible de l'explorateur. L'arbre est aplati avant le rendu pour
/// pouvoir utiliser `ScrollArea::show_rows` : même un gros dossier ne redessine
/// alors que les lignes réellement visibles pendant le défilement.
#[derive(Clone)]
pub(super) struct ExplorerEntry {
    pub(super) path: PathBuf,
    pub(super) depth: usize,
    pub(super) is_dir: bool,
    /// Dossier déplié ? Relevé une fois pendant l'aplatissement.
    pub(super) open: bool,
}

/// Action demandée par une ligne de l'explorateur. Les mutations disque restent
/// dans `App`, jamais dans ce widget sans état.
///
/// Renommer se dit en trois temps — commencer, valider, abandonner — et non
/// par une bascule unique : Entrée, Échap et le clic ailleurs veulent chacun
/// quelque chose de précis, et l'ancienne bascule les confondait.
pub(super) enum ExplorerAction {
    /// Ouvrir le fichier dans l'éditeur.
    Open(PathBuf),
    /// Déplier ou replier un dossier, sans changer de racine.
    Toggle(PathBuf),
    /// Prendre ce dossier comme racine de l'explorateur.
    Navigate(PathBuf),
    /// Poser la sélection sans rien ouvrir.
    Select(PathBuf),
    BeginRename(PathBuf),
    CommitRename,
    CancelRename,
    Delete(PathBuf),
    /// Créer un dossier DANS celui-ci.
    NewFolderIn(PathBuf),
    /// Créer un fichier DANS celui-ci.
    NewFileIn(PathBuf),
    CopyPath(PathBuf),
}

/// Les libellés du menu contextuel, fournis déjà traduits par le panneau.
pub(super) struct ExplorerRowLabels<'a> {
    pub(super) open: &'a str,
    pub(super) expand: &'a str,
    pub(super) set_root: &'a str,
    pub(super) new_file: &'a str,
    pub(super) new_folder: &'a str,
    pub(super) rename: &'a str,
    pub(super) copy_path: &'a str,
    pub(super) delete: &'a str,
}

/// Les teintes d'une ligne. Elles viennent du thème, que ce module ne connaît
/// pas — comme les libellés viennent de la langue, qu'il ne connaît pas non
/// plus.
pub(super) struct ExplorerRowColors {
    /// Sources assembleur : ce sont elles que l'on vient chercher.
    pub(super) asm: Color32,
    /// Les autres fichiers.
    pub(super) other: Color32,
    /// Nom d'un dossier, et texte courant de la ligne.
    pub(super) text: Color32,
    /// Icône de dossier.
    pub(super) folder: Color32,
    /// Fond de la ligne sélectionnée, puis de la ligne survolée.
    pub(super) sel_bg: Color32,
    pub(super) hover_bg: Color32,
    /// Texte de la ligne sélectionnée, et trait qui la marque à gauche.
    pub(super) sel_fg: Color32,
    pub(super) accent: Color32,
    /// Chevrons et traits verticaux de retrait.
    pub(super) dim: Color32,
}

/// Ce que la ligne doit signaler en plus de son nom.
pub(super) struct ExplorerRowMarks {
    pub(super) selected: bool,
    /// C'est le fichier actuellement ouvert dans l'éditeur.
    pub(super) open_in_editor: bool,
    /// Amener cette ligne à l'écran (sélection déplacée au clavier).
    pub(super) scroll_to: bool,
    /// Afficher le chemin complet au survol (réglage « info-bulles »).
    pub(super) path_tip: bool,
}

/// L'état du renommage, quand c'est CETTE ligne que l'on renomme.
pub(super) struct ExplorerRename<'a> {
    pub(super) input: &'a mut String,
    /// Consommé au premier rendu du champ. Le focus se demande **une fois** :
    /// le redemander à chaque image empêchait le champ de le perdre, donc
    /// `lost_focus()` de se produire — Entrée ne validait jamais — et le
    /// reprenait à tout autre widget cliqué pendant ce temps.
    pub(super) focus: &'a mut bool,
}

/// Aplatit les seuls dossiers dépliés, dans l'ordre où ils s'affichent.
pub(super) fn explorer_entries(expanded: &HashSet<PathBuf>, root: &Path) -> Vec<ExplorerEntry> {
    fn visit(expanded: &HashSet<PathBuf>, dir: &Path, depth: usize, out: &mut Vec<ExplorerEntry>) {
        if depth > EXPLORER_MAX_DEPTH {
            return;
        }
        let (dirs, files) = list_entries(dir);
        for path in dirs {
            let open = expanded.contains(&path);
            out.push(ExplorerEntry { path: path.clone(), depth, is_dir: true, open });
            if open {
                visit(expanded, &path, depth + 1, out);
            }
        }
        out.extend(
            files
                .into_iter()
                .map(|path| ExplorerEntry { path, depth, is_dir: false, open: false }),
        );
    }

    let mut entries = Vec::new();
    visit(expanded, root, 0, &mut entries);
    entries
}

/// Le triangle de dépliage, dessiné et non écrit : un glyphe dépendrait de la
/// police retenue par le système, et se décalerait d'un thème à l'autre.
fn chevron(painter: &egui::Painter, c: egui::Pos2, color: Color32, open: bool) {
    let r = 3.6;
    let pts = if open {
        vec![
            egui::pos2(c.x - r, c.y - r * 0.55),
            egui::pos2(c.x + r, c.y - r * 0.55),
            egui::pos2(c.x, c.y + r * 0.75),
        ]
    } else {
        vec![
            egui::pos2(c.x - r * 0.55, c.y - r),
            egui::pos2(c.x + r * 0.75, c.y),
            egui::pos2(c.x - r * 0.55, c.y + r),
        ]
    };
    painter.add(egui::Shape::convex_polygon(pts, color, egui::Stroke::NONE));
}

/// Icône de dossier : un corps et sa languette.
fn folder_icon(painter: &egui::Painter, c: egui::Pos2, color: Color32) {
    let body = egui::Rect::from_center_size(c + egui::vec2(0.0, 1.0), egui::vec2(14.0, 9.0));
    let tab = egui::Rect::from_min_size(
        egui::pos2(body.left(), body.top() - 2.5),
        egui::vec2(6.0, 3.0),
    );
    painter.rect_filled(tab, egui::CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 }, color);
    painter.rect_filled(body, egui::CornerRadius::same(2), color);
}

/// Icône de fichier : une feuille et ses trois lignes de texte.
fn file_icon(painter: &egui::Painter, c: egui::Pos2, color: Color32) {
    let sheet = egui::Rect::from_center_size(c, egui::vec2(11.0, 13.0));
    painter.rect_stroke(
        sheet,
        egui::CornerRadius::same(2),
        egui::Stroke::new(1.2_f32, color),
        egui::StrokeKind::Inside,
    );
    for i in 0..3 {
        let y = (sheet.top() + 4.0 + i as f32 * 3.0).round() + 0.5;
        painter.line_segment(
            [egui::pos2(sheet.left() + 2.5, y), egui::pos2(sheet.right() - 2.5, y)],
            egui::Stroke::new(1.0_f32, color.gamma_multiply(0.6)),
        );
    }
}

/// Au premier rendu du champ de renommage, présélectionne le **radical** du nom
/// et non l'extension : on renomme `boucle.asm` en `boucle-corrigee.asm`, on ne
/// change presque jamais le `.asm`. C'est le geste de VS Code et des
/// gestionnaires de fichiers.
fn select_stem(ctx: &egui::Context, id: egui::Id, name: &str) {
    let stem = Path::new(name)
        .file_stem()
        .map_or(name.chars().count(), |s| s.to_string_lossy().chars().count());
    let mut state = egui::widgets::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
        egui::text::CCursor::new(0),
        egui::text::CCursor::new(stem),
    )));
    state.store(ctx, id);
}

/// Rend une ligne de l'explorateur : bande de sélection pleine largeur, traits
/// de retrait, chevron, icône, nom élidé, et le menu contextuel qui va avec.
pub(super) fn explorer_row(
    ui: &mut egui::Ui,
    entry: &ExplorerEntry,
    marks: &ExplorerRowMarks,
    rename: Option<ExplorerRename<'_>>,
    labels: &ExplorerRowLabels<'_>,
    colors: &ExplorerRowColors,
) -> Option<ExplorerAction> {
    let mut action = None;
    // Pendant le renommage, la bande ne capte plus le clic : il appartient au
    // champ de saisie posé par-dessus.
    let sense = if rename.is_some() { egui::Sense::hover() } else { egui::Sense::click() };
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), EXPLORER_ROW_H), sense);
    let painter = ui.painter().with_clip_rect(rect);

    if marks.selected {
        painter.rect_filled(rect, egui::CornerRadius::same(4), colors.sel_bg);
        // Le liseré d'accent à gauche : la bande seule se confond avec un
        // survol, surtout sur un thème où les deux fonds sont proches.
        painter.rect_filled(
            egui::Rect::from_min_size(rect.left_top(), egui::vec2(2.0, rect.height())),
            egui::CornerRadius::same(1),
            colors.accent,
        );
    } else if response.hovered() {
        painter.rect_filled(rect, egui::CornerRadius::same(4), colors.hover_bg);
    }

    // Traits de retrait : à trois niveaux de profondeur, l'œil ne rattache plus
    // un fichier à son dossier sans eux.
    let base = rect.left() + 6.0;
    for level in 0..entry.depth {
        let x = (base + level as f32 * EXPLORER_INDENT + 5.0).round() + 0.5;
        painter.line_segment(
            [egui::pos2(x, rect.top() + 1.0), egui::pos2(x, rect.bottom() - 1.0)],
            egui::Stroke::new(1.0_f32, colors.dim.gamma_multiply(0.45)),
        );
    }

    let mut x = base + entry.depth as f32 * EXPLORER_INDENT;
    if entry.is_dir {
        chevron(&painter, egui::pos2(x + 5.0, rect.center().y), colors.dim, entry.open);
    }
    x += 13.0;
    let is_source = is_asm(&entry.path);
    let icon_color = if entry.is_dir {
        colors.folder
    } else if is_source {
        colors.asm
    } else {
        colors.other
    };
    if entry.is_dir {
        folder_icon(&painter, egui::pos2(x + 8.0, rect.center().y), icon_color);
    } else {
        file_icon(&painter, egui::pos2(x + 8.0, rect.center().y), icon_color);
    }
    x += 21.0;

    if let Some(rn) = rename {
        let field = egui::Rect::from_min_max(
            egui::pos2(x, rect.top() + 1.0),
            egui::pos2(rect.right() - 4.0, rect.bottom() - 1.0),
        );
        let resp = ui.put(
            field,
            egui::TextEdit::singleline(rn.input)
                .id(super::explorer_rename_id())
                .margin(egui::Margin::symmetric(4, 1)),
        );
        if std::mem::take(rn.focus) {
            resp.request_focus();
            select_stem(ui.ctx(), resp.id, rn.input);
            resp.scroll_to_me(Some(egui::Align::Center));
        }
        // Échap abandonne ; sortir du champ autrement — Entrée, ou un clic
        // ailleurs — vaut validation, comme dans tous les explorateurs.
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            action = Some(ExplorerAction::CancelRename);
        } else if resp.lost_focus() {
            action = Some(ExplorerAction::CommitRename);
        }
        return action;
    }

    // Ce sont les ICÔNES qui portent le type, pas les noms : une colonne de
    // noms bariolés se lit moins bien qu'une colonne de noms d'une seule
    // couleur. Seuls s'en écartent la ligne sélectionnée, le fichier ouvert, et
    // ce qui n'est pas de l'assembleur — atténué, car on ne vient pas le
    // chercher.
    let name_color = if marks.selected {
        colors.sel_fg
    } else if marks.open_in_editor {
        colors.accent
    } else if entry.is_dir || is_source {
        colors.text
    } else {
        colors.other
    };
    // Un nom trop long s'élide au lieu d'être coupé net : dans un panneau
    // étroit, `…` dit qu'il en manque, un rognage laisse croire au nom entier.
    let mut job = egui::text::LayoutJob::single_section(
        file_name(&entry.path),
        egui::TextFormat {
            font_id: egui::TextStyle::Button.resolve(ui.style()),
            color: name_color,
            ..Default::default()
        },
    );
    job.wrap = egui::text::TextWrapping {
        max_width: (rect.right() - 12.0 - x).max(8.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    painter.galley(
        egui::pos2(x, rect.center().y - galley.size().y * 0.5),
        galley,
        name_color,
    );
    // Le fichier ouvert dans l'éditeur se repère même quand la sélection est
    // ailleurs : sans ce point, rien ne dit lequel des dix `.asm` on édite.
    if marks.open_in_editor {
        painter.circle_filled(egui::pos2(rect.right() - 7.0, rect.center().y), 2.5, colors.accent);
    }

    if marks.scroll_to {
        response.scroll_to_me(Some(egui::Align::Center));
    }
    if response.clicked() {
        action = Some(if entry.is_dir {
            ExplorerAction::Toggle(entry.path.clone())
        } else {
            ExplorerAction::Open(entry.path.clone())
        });
    }
    response.context_menu(|ui| {
        // Le clic droit sélectionne aussi : agir sur une ligne sans la
        // sélectionner laisserait le clavier travailler ailleurs.
        if !marks.selected {
            action = Some(ExplorerAction::Select(entry.path.clone()));
        }
        if entry.is_dir {
            if ui.button(labels.expand).clicked() {
                action = Some(ExplorerAction::Toggle(entry.path.clone()));
                ui.close();
            }
            if ui.button(labels.set_root).clicked() {
                action = Some(ExplorerAction::Navigate(entry.path.clone()));
                ui.close();
            }
        } else if ui.button(labels.open).clicked() {
            action = Some(ExplorerAction::Open(entry.path.clone()));
            ui.close();
        }
        ui.separator();
        // Créer se fait DANS le dossier visé, ou à côté du fichier visé : c'est
        // là que l'on regarde au moment du clic droit.
        let target = if entry.is_dir {
            entry.path.clone()
        } else {
            entry.path.parent().map_or_else(|| entry.path.clone(), Path::to_path_buf)
        };
        if ui.button(labels.new_file).clicked() {
            action = Some(ExplorerAction::NewFileIn(target.clone()));
            ui.close();
        }
        if ui.button(labels.new_folder).clicked() {
            action = Some(ExplorerAction::NewFolderIn(target));
            ui.close();
        }
        ui.separator();
        if ui.button(labels.rename).clicked() {
            action = Some(ExplorerAction::BeginRename(entry.path.clone()));
            ui.close();
        }
        if ui.button(labels.copy_path).clicked() {
            action = Some(ExplorerAction::CopyPath(entry.path.clone()));
            ui.close();
        }
        if ui.button(egui::RichText::new(labels.delete).color(false_col())).clicked() {
            action = Some(ExplorerAction::Delete(entry.path.clone()));
            ui.close();
        }
    });
    if marks.path_tip {
        response.on_hover_text(entry.path.display().to_string());
    }
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
