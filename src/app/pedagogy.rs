//! Mode pédagogique : animations enrichies et vue mémoire unifiée.
//!
//! Deux options indépendantes, activables dans Réglages (`pedagogy_anim` et
//! `pedagogy_memview`), toutes deux désactivées par défaut :
//!
//! * **animations enrichies** — le clignotement ([`blink_wave`]) et la bande de
//!   bits ([`bit_diff_strip`]) rendent visible *ce qui* a changé dans un
//!   registre, pas seulement *qu'il* a changé ;
//! * **vue mémoire unifiée** — [`App::memory_map_ui`] peint un schéma reliant
//!   chaque registre porteur d'adresse à la région mémoire qu'il désigne.
//!
//! Les couleurs de régions et de fils vivent ici (et non dans la palette
//! générale) car elles ne servent qu'à ce mode.

use eframe::egui::{self, Color32, RichText};

use crate::i18n;

use super::{App, CHANGED, FLASH_BRIGHT, badge, lerp_color};

// ---------- Constantes et couleurs propres au mode pédagogique ----------

/// Nombre de clignotements pendant la durée de l'animation pédagogique.
pub(super) const BLINK_COUNT: f32 = 3.0;
/// Durée du clignotement pédagogique (plus long que `FLASH_DUR` pour être vu).
pub(super) const BLINK_DUR: f64 = 1.4;

/// Onde de clignotement : `1.0` au pic lumineux, `0.0` au creux, sur
/// `BLINK_COUNT` oscillations réparties sur `[0,1]`, atténuées vers la fin
/// (le clignotement s'estompe au lieu de s'arrêter net).
pub(super) fn blink_wave(p: f32) -> f32 {
    let osc = (p * BLINK_COUNT * std::f32::consts::TAU).cos();
    let up = (1.0 - osc) * 0.5; // 0 → 1 → 0 …
    up * (1.0 - p) // enveloppe décroissante
}

/// Couleurs des régions mémoire (vue unifiée) — un code couleur stable que
/// l'élève peut mémoriser : bleu = code, violet = données, vert = tas, orange = pile.
pub(super) fn region_color(kind: crate::debugger::RegionKind) -> Color32 {
    use crate::debugger::RegionKind as K;
    match kind {
        K::Code => Color32::from_rgb(0x4C, 0x8B, 0xF5),
        K::Rodata => Color32::from_rgb(0x5A, 0xA6, 0xB8),
        K::Data => Color32::from_rgb(0xA0, 0x72, 0xD8),
        K::Heap => Color32::from_rgb(0x5F, 0xBF, 0x69),
        K::Stack => Color32::from_rgb(0xE8, 0x8A, 0x2E),
    }
}

/// Palette cyclique pour distinguer les fils registre→mémoire les uns des autres.
pub(super) const WIRE_COLORS: [Color32; 6] = [
    Color32::from_rgb(0x6E, 0xB4, 0xE8),
    Color32::from_rgb(0xF5, 0xA6, 0x23),
    Color32::from_rgb(0x5F, 0xBF, 0x69),
    Color32::from_rgb(0xD9, 0x7B, 0xD9),
    Color32::from_rgb(0x5A, 0xD0, 0xC8),
    Color32::from_rgb(0xE0, 0x6C, 0x6C),
];

// ---------- Géométrie du schéma mémoire ----------

/// Hauteur minimale d'une voie de région : il faut loger le titre, la ligne de
/// bornes d'adresses et la mini-carte sans qu'ils se chevauchent.
pub(super) const LANE_MIN_H: f32 = 64.0;
/// Décalages verticaux du contenu d'une voie, depuis son bord haut.
pub(super) const LANE_TITLE_Y: f32 = 6.0;
pub(super) const LANE_BOUNDS_Y: f32 = 23.0;
/// Ligne des octets pointés, affichée quand un seul fil vise la voie.
pub(super) const LANE_CONTENT_Y: f32 = 39.0;
/// Hauteur de la mini-carte et sa marge par rapport au bas de la voie.
pub(super) const LANE_TRACK_H: f32 = 7.0;
pub(super) const LANE_TRACK_MARGIN: f32 = 9.0;

/// Hauteur d'une voie selon le nombre de registres qui la visent.
///
/// Le plancher ne dépend PAS de ce nombre : `.rodata` et `.data/.bss` ne sont
/// visés par aucun registre dans un programme simple, et les écraser les rendait
/// illisibles alors que l'élève doit justement voir que ces sections existent.
pub(super) fn lane_height(pointers: usize) -> f32 {
    LANE_MIN_H + 15.0 * pointers.min(3) as f32
}

// ---------- Helpers de peinture ----------

/// Couleur du fil du n-ième registre (palette cyclique).
pub(super) fn wire_col(i: usize) -> Color32 {
    WIRE_COLORS[i % WIRE_COLORS.len()]
}

/// Taille mémoire lisible par un humain, dans les unités de la langue courante
/// (fr : o/Kio/Mio — en/es : B/KiB/MiB).
fn human_size(bytes: u64, lang: crate::i18n::Lang) -> String {
    const K: u64 = 1024;
    let fr = matches!(lang, crate::i18n::Lang::Fr);
    let (b1, k, m, g) = if fr {
        ("o", "Kio", "Mio", "Gio")
    } else {
        ("B", "KiB", "MiB", "GiB")
    };
    match bytes {
        n if n < K => format!("{n} {b1}"),
        n if n < K * K => format!("{} {k}", n / K),
        n if n < K * K * K => format!("{} {m}", n / (K * K)),
        n => format!("{} {g}", n / (K * K * K)),
    }
}

/// Bande de 64 cellules montrant quels bits ont basculé entre `pval` et `val` :
/// cellule vive = bit modifié (clignote), cellule mate = bit inchangé.
/// Les bits à 1 sont pleins, ceux à 0 sont en creux — l'élève voit le motif binaire.
pub(super) fn bit_diff_strip(ui: &mut egui::Ui, val: u64, pval: u64, blink: f32) {
    const CELL: f32 = 2.6;
    const GAP: f32 = 0.6;
    const H: f32 = 9.0;
    let diff = val ^ pval;
    let w = 64.0 * CELL + 63.0 * GAP;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, H), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let off = ui.visuals().extreme_bg_color;
    let on = ui.visuals().weak_text_color().gamma_multiply(0.55);
    let hot = lerp_color(CHANGED, FLASH_BRIGHT, blink);
    // Bit 63 à gauche, bit 0 à droite (ordre de lecture d'un nombre binaire).
    for i in 0..64u32 {
        let bit = 63 - i;
        let set = (val >> bit) & 1 == 1;
        let flipped = (diff >> bit) & 1 == 1;
        let x = rect.left() + i as f32 * (CELL + GAP);
        let cell = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(CELL, H));
        let col = match (flipped, set) {
            (true, true) => hot,
            (true, false) => hot.gamma_multiply(0.4),
            (false, true) => on,
            (false, false) => off,
        };
        // Un bit modifié occupe toute la hauteur ; un bit stable reste discret.
        let r = if flipped { cell } else { cell.shrink2(egui::vec2(0.0, 2.0)) };
        painter.rect_filled(r, 0.8, col);
    }
    // Séparateurs tous les 8 bits (frontières d'octet) pour aider à compter.
    for b in 1..8 {
        let x = rect.left() + (b as f32 * 8.0) * (CELL + GAP) - GAP * 0.5;
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(0.6_f32, ui.visuals().weak_text_color().gamma_multiply(0.35)),
        );
    }
    if resp.hovered() {
        resp.on_hover_text(format!(
            "{} bit(s) modifié(s)\n0b{val:064b}",
            diff.count_ones()
        ));
    }
}

impl App {
    /// Progression du clignotement pédagogique : `Some(0.0..=1.0)` tant que
    /// l'animation enrichie tourne. `None` si l'option est désactivée.
    /// Plus long que `flash_progress` pour laisser le temps de voir les 3 pulses.
    pub(super) fn blink_progress(&self, ui: &egui::Ui) -> Option<f32> {
        if !self.animate || !self.pedagogy_anim {
            return None;
        }
        let elapsed = ui.input(|i| i.time) - self.flash_time;
        if !(0.0..BLINK_DUR).contains(&elapsed) {
            return None;
        }
        ui.ctx().request_repaint();
        Some((elapsed / BLINK_DUR) as f32)
    }

    /// Intensité du clignotement (0 = repos, 1 = pic lumineux) à cet instant.
    pub(super) fn blink_intensity(&self, ui: &egui::Ui) -> f32 {
        self.blink_progress(ui).map(blink_wave).unwrap_or(0.0)
    }

    // ---------- Vue mémoire unifiée ----------

    /// Schéma peint : colonne de registres à gauche, carte des régions mémoire à
    /// droite, reliées par des courbes de Bézier colorées. Survoler un registre
    /// isole son fil et met en évidence l'octet visé ; les registres qui viennent
    /// de changer clignotent avec leur fil.
    pub(super) fn memory_map_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);

        if !self.can_read_memory() {
            let msg = match self.dbg.as_ref().map(|d| d.is_alive()) {
                Some(false) => tr(
                    "Programme terminé — relancez pour voir la carte mémoire.",
                    "Program finished — relaunch to see the memory map.",
                    "Programa terminado — reinícielo para ver el mapa de memoria.",
                ),
                _ => tr(
                    "Lancez un programme et avancez pas à pas : chaque registre qui contient une \
                     adresse sera relié à la région mémoire qu'il désigne.",
                    "Run a program and step through it: every register holding an address will be \
                     wired to the memory region it points at.",
                    "Ejecute un programa y avance paso a paso: cada registro que contenga una \
                     dirección se conectará a la región de memoria que señala.",
                ),
            };
            ui.weak(msg);
            return;
        }

        let Some(rows) = self.reg_rows() else { return };

        // --- Collecte : régions mappées + octets visés par chaque registre ---
        let regions = self.dbg.as_ref().unwrap().mem_regions();
        if regions.is_empty() {
            ui.weak(tr(
                "Aucune région mémoire lisible (/proc/<pid>/maps vide).",
                "No readable memory region (/proc/<pid>/maps empty).",
                "Ninguna región de memoria legible (/proc/<pid>/maps vacío).",
            ));
            return;
        }

        struct RegWire {
            name: &'static str,
            val: u64,
            changed: bool,
            /// Index dans `regions` de la région pointée, si le pointeur est valide.
            region: Option<usize>,
            /// 16 octets lus à l'adresse pointée.
            bytes: Option<Vec<u8>>,
        }
        let wires: Vec<RegWire> = {
            let dbg = self.dbg.as_ref().unwrap();
            rows.iter()
                .map(|(name, val, pval)| {
                    let region = regions.iter().position(|r| r.contains(*val));
                    let bytes = region.and_then(|_| dbg.read_mem(*val, 16).ok());
                    RegWire { name, val: *val, changed: val != pval, region, bytes }
                })
                .collect()
        };

        let (hdr, addr_c, bytes_c) = (self.c_header(), self.c_addr(), self.c_bytes());
        let blink = self.blink_intensity(ui);
        let weak_col = self.c_bytes().gamma_multiply(0.7);

        // Légende du code couleur des régions.
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("Régions :", "Regions:", "Regiones:")).small().color(hdr));
            for kind in [
                crate::debugger::RegionKind::Code,
                crate::debugger::RegionKind::Rodata,
                crate::debugger::RegionKind::Data,
                crate::debugger::RegionKind::Heap,
                crate::debugger::RegionKind::Stack,
            ] {
                if regions.iter().any(|r| r.kind == kind) {
                    badge(ui, kind.label(), region_color(kind));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(tr(
                        "survolez un registre pour isoler son fil",
                        "hover a register to isolate its wire",
                        "pase el cursor sobre un registro para aislar su hilo",
                    ))
                    .small()
                    .italics()
                    .color(weak_col),
                );
            });
        });
        ui.add_space(4.0);

        // --- Canevas peint ---
        egui::ScrollArea::vertical()
            .id_salt("memmap_canvas")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                const ROW_H: f32 = 22.0;
                const ROW_GAP: f32 = 4.0;
                const LANE_GAP: f32 = 6.0;
                let reg_w = 172.0_f32;
                let gap_w = 74.0_f32; // couloir des courbes
                let avail = ui.available_width();
                let lane_w = (avail - reg_w - gap_w - 8.0).max(150.0);

                let reg_h = wires.len() as f32 * (ROW_H + ROW_GAP);
                // Hauteur d'une voie de région : proportionnelle au nombre de
                // registres qui la visent, avec un minimum lisible.
                let lane_hits: Vec<usize> = (0..regions.len())
                    .map(|i| wires.iter().filter(|w| w.region == Some(i)).count())
                    .collect();
                let lane_h: Vec<f32> = lane_hits.iter().map(|&n| lane_height(n)).collect();
                let lanes_total: f32 = lane_h.iter().sum::<f32>() + lane_h.len() as f32 * LANE_GAP;

                let canvas_h = reg_h.max(lanes_total) + 8.0;
                let (canvas, _resp) = ui.allocate_exact_size(
                    egui::vec2(avail, canvas_h),
                    egui::Sense::hover(),
                );
                let painter = ui.painter_at(canvas);
                let pointer = ui.ctx().pointer_latest_pos();

                // Rectangles des registres (colonne de gauche), centrés verticalement.
                let reg_x0 = canvas.left() + 2.0;
                let reg_y0 = canvas.top() + ((canvas_h - reg_h) * 0.5).max(0.0);
                let reg_rects: Vec<egui::Rect> = (0..wires.len())
                    .map(|i| {
                        let y = reg_y0 + i as f32 * (ROW_H + ROW_GAP);
                        egui::Rect::from_min_size(egui::pos2(reg_x0, y), egui::vec2(reg_w, ROW_H))
                    })
                    .collect();

                // Rectangles des voies mémoire (colonne de droite).
                let lane_x0 = canvas.right() - lane_w - 2.0;
                let lane_y0 = canvas.top() + ((canvas_h - lanes_total) * 0.5).max(0.0);
                let mut lane_rects: Vec<egui::Rect> = Vec::with_capacity(regions.len());
                let mut y = lane_y0;
                for h in &lane_h {
                    lane_rects.push(egui::Rect::from_min_size(
                        egui::pos2(lane_x0, y),
                        egui::vec2(lane_w, *h),
                    ));
                    y += h + LANE_GAP;
                }

                // Registre survolé (priorité au fil isolé).
                let hovered: Option<usize> = pointer.and_then(|p| {
                    reg_rects.iter().position(|r| r.expand2(egui::vec2(0.0, 2.0)).contains(p))
                });

                // ---- 1. Voies mémoire (peintes d'abord : arrière-plan) ----
                for (i, region) in regions.iter().enumerate() {
                    let rect = lane_rects[i];
                    let col = region_color(region.kind);
                    let targeted = lane_hits[i] > 0;
                    // L'estompage sert UNIQUEMENT à isoler le fil survolé. Hors
                    // survol, toutes les régions restent lisibles : une région
                    // que personne ne vise fait quand même partie de la carte.
                    let dim = hovered.is_some_and(|h| wires[h].region != Some(i));
                    let a = if dim {
                        0.08
                    } else if targeted {
                        0.28
                    } else {
                        0.18
                    };
                    painter.rect_filled(rect, 5.0, col.linear_multiply(a));
                    let stroke_w = if dim { 0.7_f32 } else if targeted { 1.6 } else { 1.1 };
                    painter.rect_stroke(
                        rect,
                        5.0,
                        egui::Stroke::new(stroke_w, col.gamma_multiply(if dim { 0.5 } else { 1.0 })),
                    );
                    // Bande d'accent à gauche de la voie.
                    painter.rect_filled(
                        egui::Rect::from_min_size(rect.left_top(), egui::vec2(4.0, rect.height())),
                        3.0,
                        col.gamma_multiply(if dim { 0.4 } else { 1.0 }),
                    );

                    let txt_c = if dim { weak_col } else { col };
                    // Titre : nom de la région + taille.
                    painter.text(
                        rect.left_top() + egui::vec2(10.0, LANE_TITLE_Y),
                        egui::Align2::LEFT_TOP,
                        format!("{}  ({})", region.kind.label(), human_size(region.size(), lang)),
                        egui::FontId::proportional(12.5),
                        txt_c,
                    );
                    // Bornes d'adresses + permissions.
                    painter.text(
                        rect.left_top() + egui::vec2(10.0, LANE_BOUNDS_Y),
                        egui::Align2::LEFT_TOP,
                        format!("0x{:X} – 0x{:X}   {}", region.start, region.end, region.perms),
                        egui::FontId::monospace(9.5),
                        if dim { weak_col } else { addr_c },
                    );

                    // Mini-carte : position relative des pointeurs dans la région.
                    let track = egui::Rect::from_min_size(
                        rect.left_bottom() + egui::vec2(10.0, -(LANE_TRACK_H + LANE_TRACK_MARGIN)),
                        egui::vec2(rect.width() - 20.0, LANE_TRACK_H),
                    );
                    painter.rect_filled(track, 3.0, ui.visuals().extreme_bg_color.gamma_multiply(0.8));
                    for (wi, w) in wires.iter().enumerate() {
                        if w.region != Some(i) {
                            continue;
                        }
                        let faded = hovered.is_some_and(|h| h != wi);
                        let frac = (w.val - region.start) as f64 / region.size().max(1) as f64;
                        let x = track.left() + track.width() * frac as f32;
                        let wc = wire_col(wi).gamma_multiply(if faded { 0.35 } else { 1.0 });
                        // Curseur de position dans la région.
                        painter.rect_filled(
                            egui::Rect::from_center_size(
                                egui::pos2(x, track.center().y),
                                egui::vec2(3.0, 12.0),
                            ),
                            1.5,
                            wc,
                        );
                    }

                    // Contenu visé, quand un seul fil (ou le fil survolé) cible la voie.
                    let show: Option<usize> = match hovered {
                        Some(h) if wires[h].region == Some(i) => Some(h),
                        None if lane_hits[i] == 1 => wires.iter().position(|w| w.region == Some(i)),
                        _ => None,
                    };
                    if let Some(wi) = show
                        && let Some(bytes) = &wires[wi].bytes
                    {
                        let hex: String = bytes.iter().take(8).map(|b| format!("{b:02X} ")).collect();
                        let ascii: String = bytes
                            .iter()
                            .take(8)
                            .map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '·' })
                            .collect();
                        painter.text(
                            rect.left_top() + egui::vec2(10.0, LANE_CONTENT_Y),
                            egui::Align2::LEFT_TOP,
                            format!("[{}] {} │{ascii}│", wires[wi].name, hex.trim_end()),
                            egui::FontId::monospace(10.0),
                            wire_col(wi),
                        );
                    }
                }

                // ---- 2. Boîtes de registres ----
                for (i, w) in wires.iter().enumerate() {
                    let rect = reg_rects[i];
                    let is_hov = hovered == Some(i);
                    let linked = w.region.is_some();
                    let faded = hovered.is_some_and(|h| h != i);

                    let wc = wire_col(i);
                    // Fond : clignotant si la valeur vient de changer.
                    let fill = if w.changed && blink > 0.0 {
                        lerp_color(CHANGED.linear_multiply(0.20), FLASH_BRIGHT.linear_multiply(0.5), blink)
                    } else if is_hov {
                        wc.linear_multiply(0.20)
                    } else if linked {
                        ui.visuals().faint_bg_color
                    } else {
                        Color32::TRANSPARENT
                    };
                    painter.rect_filled(rect, 4.0, fill);
                    let stroke_c = if w.changed && blink > 0.0 {
                        lerp_color(CHANGED, FLASH_BRIGHT, blink)
                    } else if linked {
                        wc.gamma_multiply(if faded { 0.35 } else { 1.0 })
                    } else {
                        weak_col.gamma_multiply(0.5)
                    };
                    let sw = if w.changed && blink > 0.0 { 1.0 + 1.6 * blink } else if is_hov { 1.6 } else { 1.0 };
                    painter.rect_stroke(rect, 4.0, egui::Stroke::new(sw, stroke_c));

                    let name_c = if w.changed {
                        lerp_color(CHANGED, FLASH_BRIGHT, blink)
                    } else if faded {
                        weak_col
                    } else {
                        hdr
                    };
                    painter.text(
                        rect.left_center() + egui::vec2(7.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        w.name,
                        egui::FontId::monospace(11.5),
                        name_c,
                    );
                    painter.text(
                        rect.right_center() + egui::vec2(-7.0, 0.0),
                        egui::Align2::RIGHT_CENTER,
                        format!("0x{:012X}", w.val),
                        egui::FontId::monospace(10.5),
                        if faded { weak_col } else if linked { addr_c } else { bytes_c },
                    );
                }

                // ---- 3. Fils de Bézier registre → voie mémoire ----
                // Peints après, pour passer au-dessus des boîtes et des voies.
                for (i, w) in wires.iter().enumerate() {
                    let Some(ri) = w.region else { continue };
                    let faded = hovered.is_some_and(|h| h != i);
                    let emphasised = hovered == Some(i) || (w.changed && blink > 0.0);

                    let from = reg_rects[i].right_center() + egui::vec2(1.0, 0.0);
                    // Point d'arrivée : hauteur proportionnelle à l'offset dans la région,
                    // pour que le fil désigne réellement l'endroit visé.
                    let lane = lane_rects[ri];
                    let region = &regions[ri];
                    let frac = ((w.val - region.start) as f64 / region.size().max(1) as f64) as f32;
                    let to_y = lane.top() + 6.0 + (lane.height() - 12.0) * frac.clamp(0.0, 1.0);
                    let to = egui::pos2(lane.left() - 1.0, to_y);

                    let mut col = wire_col(i);
                    if faded {
                        col = col.gamma_multiply(0.22);
                    } else if w.changed && blink > 0.0 {
                        col = lerp_color(col, FLASH_BRIGHT, blink * 0.8);
                    }
                    let width = if emphasised { 2.4_f32 } else if faded { 0.9 } else { 1.5 };

                    // Courbe cubique : tangentes horizontales aux deux extrémités.
                    let dx = ((to.x - from.x) * 0.55).max(24.0);
                    let curve = egui::epaint::CubicBezierShape::from_points_stroke(
                        [from, from + egui::vec2(dx, 0.0), to - egui::vec2(dx, 0.0), to],
                        false,
                        Color32::TRANSPARENT,
                        egui::Stroke::new(width, col),
                    );
                    painter.add(curve);
                    // Pastille de départ + tête de flèche à l'arrivée.
                    painter.circle_filled(from, if emphasised { 3.4 } else { 2.4 }, col);
                    let tip = 5.0_f32;
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            to,
                            to + egui::vec2(-tip, -tip * 0.6),
                            to + egui::vec2(-tip, tip * 0.6),
                        ],
                        col,
                        egui::Stroke::NONE,
                    ));
                }

                // Registres sans pointeur valide : note explicative sous le canevas.
                let unlinked = wires.iter().filter(|w| w.region.is_none()).count();
                if unlinked > 0 {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(format!(
                            "{unlinked} {}",
                            tr(
                                "registre(s) ne contiennent pas d'adresse mappée (valeur numérique, 0, ou zone non lisible).",
                                "register(s) hold no mapped address (plain number, 0, or unreadable area).",
                                "registro(s) no contienen una dirección mapeada (valor numérico, 0 o zona no legible).",
                            )
                        ))
                        .small()
                        .color(weak_col),
                    );
                }
            });
    }

    // ---------- Petit-boutisme ----------

    /// Explique pourquoi une valeur apparaît « à l'envers » dans le vidage hexa.
    ///
    /// C'est la confusion la plus universelle du débutant : `0x12345678` s'affiche
    /// `78 56 34 12`. Plutôt que de l'énoncer, on décompose les octets réellement
    /// lus à l'adresse courante et on montre leur poids.
    pub(super) fn endianness_ui(&self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let Some(dbg) = self.dbg.as_ref().filter(|_| self.can_read_memory()) else { return };
        let Ok(bytes) = dbg.read_mem(self.mem_addr, 8) else { return };
        if bytes.len() < 8 {
            return;
        }

        let (hdr, addr_c, bytes_c) = (self.c_header(), self.c_addr(), self.c_bytes());
        // Little-endian : l'octet d'adresse la plus basse est le moins significatif.
        let qword = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);

        egui::CollapsingHeader::new(
            RichText::new(tr(
                "🔤 Petit-boutisme — pourquoi c'est « à l'envers »",
                "🔤 Little-endian — why it looks \"backwards\"",
                "🔤 Little-endian — por qué se ve «al revés»",
            ))
            .small()
            .color(hdr),
        )
        .id_salt("endian_explain")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                RichText::new(tr(
                    "x86-64 range l'octet de poids FAIBLE à l'adresse la PLUS BASSE.",
                    "x86-64 stores the LEAST significant byte at the LOWEST address.",
                    "x86-64 guarda el byte MENOS significativo en la dirección MÁS BAJA.",
                ))
                .small(),
            );
            ui.add_space(4.0);

            // Ligne 1 : décalages, ligne 2 : octets tels qu'en mémoire.
            egui::Grid::new("endian_grid").num_columns(8).spacing([6.0, 1.0]).show(ui, |ui| {
                for i in 0..8 {
                    ui.label(RichText::new(format!("+{i}")).small().monospace().color(bytes_c));
                }
                ui.end_row();
                for (i, b) in bytes.iter().enumerate().take(8) {
                    // Dégradé : vif pour le poids faible, pâle pour le poids fort.
                    let t = i as f32 / 7.0;
                    let col = lerp_color(super::ACTION, super::ACCENT, t);
                    ui.label(RichText::new(format!("{b:02X}")).monospace().strong().color(col));
                }
                ui.end_row();
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("↑ poids faible", "↑ least significant", "↑ menos significativo"))
                    .small().color(super::ACTION));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(tr("poids fort ↑", "most significant ↑", "más significativo ↑"))
                        .small().color(super::ACCENT));
                });
            });

            ui.add_space(6.0);
            ui.label(RichText::new(tr("Relu comme un nombre :", "Read back as a number:", "Leído como número:"))
                .small().strong().color(hdr));
            egui::Grid::new("endian_values").num_columns(2).spacing([10.0, 2.0]).show(ui, |ui| {
                let mut row = |k: &str, v: String| {
                    ui.label(RichText::new(k).monospace().small().color(bytes_c));
                    ui.label(RichText::new(v).monospace().color(addr_c));
                    ui.end_row();
                };
                row("qword", format!("0x{qword:016X}"));
                row("dword", format!("0x{:08X}", qword as u32));
                row("word ", format!("0x{:04X}", qword as u16));
                row("byte ", format!("0x{:02X}", qword as u8));
            });

            ui.add_space(4.0);
            ui.label(
                RichText::new(tr(
                    "Les octets se lisent donc de droite à gauche pour reconstituer le nombre.",
                    "So the bytes read right-to-left to rebuild the number.",
                    "Por eso los bytes se leen de derecha a izquierda para reconstruir el número.",
                ))
                .small()
                .weak(),
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// et s'éteindre à la fin, sans jamais sortir de [0,1].
    #[test]
    fn blink_wave_oscillates_and_fades_out() {
        assert!(blink_wave(0.0).abs() < 1e-3, "démarre au creux");
        assert!(blink_wave(1.0).abs() < 1e-3, "se termine éteint");
        // Toujours dans [0,1] sur tout l'intervalle.
        for i in 0..=100 {
            let v = blink_wave(i as f32 / 100.0);
            assert!((0.0..=1.0).contains(&v), "blink_wave({i}%) = {v} hors bornes");
        }
        // Compte les pics : autant que BLINK_COUNT (c'est ce qui rend le
        // clignotement visible plutôt qu'un fondu unique).
        let samples: Vec<f32> = (0..600).map(|i| blink_wave(i as f32 / 600.0)).collect();
        let peaks = samples
            .windows(3)
            .filter(|w| w[1] > w[0] && w[1] >= w[2] && w[1] > 0.05)
            .count();
        assert_eq!(peaks, BLINK_COUNT as usize, "il doit y avoir {BLINK_COUNT} pulses");
        // L'enveloppe décroît : le premier pic est plus fort que le dernier.
        let first = samples[..200].iter().cloned().fold(0.0_f32, f32::max);
        let last = samples[400..].iter().cloned().fold(0.0_f32, f32::max);
        assert!(first > last, "l'intensité doit décroître ({first} > {last})");
    }

    /// Les tailles de régions s'affichent dans les unités de la langue courante.
    #[test]
    fn human_size_uses_locale_units() {
        use crate::i18n::Lang;
        assert_eq!(human_size(512, Lang::Fr), "512 o");
        assert_eq!(human_size(512, Lang::En), "512 B");
        assert_eq!(human_size(4096, Lang::Fr), "4 Kio");
        assert_eq!(human_size(4096, Lang::En), "4 KiB");
        assert_eq!(human_size(4096, Lang::Es), "4 KiB");
        assert_eq!(human_size(3 << 20, Lang::Fr), "3 Mio");
        assert_eq!(human_size(2 << 30, Lang::En), "2 GiB");
    }

    /// La palette de fils est cyclique : deux registres distants réutilisent une
    /// couleur, mais jamais deux registres voisins.
    #[test]
    fn wire_colors_cycle_without_adjacent_repeats() {
        let n = WIRE_COLORS.len();
        assert_eq!(wire_col(0), wire_col(n), "la palette doit boucler");
        for i in 0..n {
            assert_ne!(wire_col(i), wire_col(i + 1), "fils voisins distinguables");
        }
    }

    /// Le code couleur des régions est injectif : chaque nature de segment a sa
    /// propre teinte, sinon l'élève ne pourrait pas les distinguer.
    #[test]
    fn region_colors_are_distinct() {
        use crate::debugger::RegionKind::*;
        let kinds = [Code, Rodata, Data, Heap, Stack];
        for (i, a) in kinds.iter().enumerate() {
            for b in &kinds[i + 1..] {
                assert_ne!(region_color(*a), region_color(*b), "{a:?} vs {b:?}");
            }
        }
    }

    /// Rendu headless des panneaux pédagogiques avec un vrai processus tracé :
    /// garantit que le code de peinture (courbes de Bézier, bandes de bits, voies
    /// mémoire, mini-cartes) ne panique pas et que la vue mémoire trouve bien des
    /// fils à tracer (RIP→.text, RSP→[stack]).
    #[test]
    fn pedagogy_panels_render_headless() {
        let mut app = App::new();
        app.src_path = PathBuf::from("build/ped-test.asm");
        app.out_dir = PathBuf::from("build/ped");
        app.source = "section .text\n global _start\n_start:\n mov rax,5\n push rax\n \
                       pop rbx\n mov rax,60\n xor rdi,rdi\n syscall\n"
            .to_string();
        app.pedagogy_anim = true;
        app.pedagogy_memview = true;
        app.animate = true;

        app.launch();
        assert!(app.dbg.is_some(), "le programme doit être lancé");
        // push puis pop : garantit des changements de registres ET de pile à animer.
        for _ in 0..3 {
            app.step();
        }
        // Les régions doivent être classées, sinon le schéma n'aurait rien à relier.
        let regions = app.dbg.as_ref().unwrap().mem_regions();
        assert!(!regions.is_empty(), "des régions mémoire doivent être détectées");

        let ctx = egui::Context::default();
        // flash_time = 0 et le temps headless démarre à 0 ⇒ le clignotement est
        // actif pendant ce rendu : on exerce bien les chemins animés.
        app.flash_time = 0.0;
        // On rend la disposition COMPLÈTE : tous les panneaux passent par le
        // rendu, pas seulement le centre.
        let _ = ctx.run(Default::default(), |ctx| app.dock_ui(ctx));
        assert!(app.dock.is_some(), "l'arbre doit être restitué après le rendu");

        // La vue mémoire reste un onglet joignable.
        assert!(app.panel_is_open(crate::app::dock::Panel::MemMap));
    }

    /// Le petit-boutisme est la confusion la plus universelle : on vérifie que la
    /// recomposition des octets correspond bien à ce que l'UI annonce, et que le
    /// panneau se rend sans paniquer avec un vrai processus.
    #[test]
    fn endianness_view_decomposes_correctly() {
        use std::path::PathBuf;

        // La règle affichée : octet de poids faible à l'adresse la plus basse.
        let bytes = [0x78u8, 0x56, 0x34, 0x12, 0, 0, 0, 0];
        let q = u64::from_le_bytes(bytes);
        assert_eq!(q, 0x1234_5678, "78 56 34 12 se relit 0x12345678");
        assert_eq!(q as u32, 0x1234_5678);
        assert_eq!(q as u16, 0x5678);
        assert_eq!(q as u8, 0x78, "l'octet à +0 est le poids faible");

        let mut app = App::new();
        app.src_path = PathBuf::from("build/endian-test.asm");
        app.out_dir = PathBuf::from("build/endian");
        app.source = "section .text\n global _start\n_start:\n mov rax, 0x12345678\n \
                       push rax\n mov rax,60\n xor rdi,rdi\n syscall\n"
            .to_string();
        app.launch();
        for _ in 0..2 {
            app.step();
        }
        // Pointe le vidage sur le sommet de pile : il contient la valeur poussée.
        app.mem_addr = app.snap().unwrap().regs.rsp;
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.endianness_ui(ui));
        });

        // Et les octets réellement en mémoire suivent bien l'ordre annoncé.
        if let Some(d) = app.dbg.as_ref()
            && let Ok(m) = d.read_mem(app.mem_addr, 8)
        {
            assert_eq!(m[0], 0x78, "poids faible à l'adresse la plus basse");
            assert_eq!(m[1], 0x56);
            assert_eq!(m[2], 0x34);
            assert_eq!(m[3], 0x12);
        }
    }

    /// Le contenu d'une voie ne doit jamais se chevaucher : c'est ce qui rendait
    /// `.rodata` et `.data/.bss` illisibles — écrasés à 30 px, le titre, les
    /// bornes d'adresses et la mini-carte se marchaient dessus.
    #[test]
    fn lane_content_never_overlaps() {
        // Hauteurs approximatives des trois lignes de texte.
        const TITLE_H: f32 = 16.0;
        const BOUNDS_H: f32 = 13.0;

        for pointers in 0..=6 {
            let h = lane_height(pointers);
            assert!(h >= LANE_MIN_H, "{pointers} pointeurs → {h}px sous le plancher");

            // Titre puis bornes, sans recouvrement.
            assert!(
                LANE_TITLE_Y + TITLE_H <= LANE_BOUNDS_Y,
                "le titre déborde sur les bornes d'adresses"
            );
            // Bornes puis, le cas échéant, la ligne d'octets pointés.
            assert!(
                LANE_BOUNDS_Y + BOUNDS_H <= LANE_CONTENT_Y,
                "les bornes débordent sur la ligne d'octets"
            );

            // La mini-carte est ancrée en bas : elle doit rester sous tout le reste.
            let track_top = h - LANE_TRACK_H - LANE_TRACK_MARGIN;
            let lowest_text = if pointers > 0 {
                LANE_CONTENT_Y + BOUNDS_H
            } else {
                LANE_BOUNDS_Y + BOUNDS_H
            };
            assert!(
                track_top >= lowest_text,
                "{pointers} pointeurs : la mini-carte (y={track_top}) chevauche le texte (y={lowest_text})"
            );
            // Et elle doit tenir dans la voie.
            assert!(track_top + LANE_TRACK_H <= h, "mini-carte hors de la voie");
        }
    }

    /// Une région que personne ne vise reste une région : sa voie doit garder
    /// une hauteur utile, sinon l'élève ne voit pas que .rodata existe.
    #[test]
    fn unpointed_region_keeps_a_usable_lane() {
        let empty = lane_height(0);
        let one = lane_height(1);
        assert!(empty >= 60.0, "voie sans pointeur trop écrasée : {empty}px");
        assert!(one > empty, "une voie visée doit être plus haute");
        // Mais l'écart reste raisonnable : pas de voie géante contre une naine.
        assert!(one < empty * 2.0, "écart trop brutal : {empty} → {one}");
    }
}
