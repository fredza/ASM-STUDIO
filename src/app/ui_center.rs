use eframe::egui::{self, RichText};

use crate::debugger::Flags;
use crate::explain;
use crate::i18n;
use crate::syntax;
use crate::syscall;

use super::{
    App, ACCENT, ACTION, BREAKPOINT, CHANGED, FLAG_ON, FLAG_OFF, FALSE_COL, GUTTER,
    badge, card,
};

impl App {
    // ---------- Centre : onglets Éditeur / Désassemblage ----------

    /// Onglet « Éditeur » : nom de fichier, repère RIP, puis la zone de texte.
    ///
    /// Le sélecteur d'onglets a disparu — c'est la barre d'onglets de la zone
    /// d'ancrage qui joue ce rôle désormais. Restent les deux informations que
    /// l'élève lit sans quitter son code : quel fichier, et où en est le CPU.
    pub(super) fn editor_tab_ui(&mut self, ui: &mut egui::Ui) {
        let hdr = self.c_header();
        ui.horizontal(|ui| {
            let name = self.src_path.file_name().unwrap_or_default().to_string_lossy();
            let mark = if self.dirty { " ●" } else { "" };
            ui.label(RichText::new(format!("{name}{mark}")).color(hdr));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.rip_banner(ui);
            });
        });
        ui.separator();
        if self.show_find {
            self.find_bar_ui(ui);
            ui.add_space(2.0);
        }
        let fold_ranges = compute_fold_ranges(&self.source);
        if fold_ranges.iter().any(|f| self.folded_labels.contains(&f.label)) {
            self.folded_editor_ui(ui, &fold_ranges);
        } else {
            self.editor_ui(ui);
        }
    }

    /// Vue en lecture seule affichée tant qu'au moins un label est replié :
    /// chaque ligne visible est une rangée à part (numéro + coloration
    /// syntaxique d'une seule ligne), les zones repliées cédant la place à un
    /// marqueur cliquable. On n'édite jamais à travers un repli — pas besoin
    /// de synchroniser texte réel et texte affiché, ni de gérer un curseur ici.
    fn folded_editor_ui(&mut self, ui: &mut egui::Ui, fold_ranges: &[FoldRange]) {
        let dark = self.dark;
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let lines: Vec<String> = self.source.lines().map(str::to_string).collect();
        let width = lines.len().to_string().len();
        let mut to_fold: Option<String> = None;
        let mut to_unfold: Option<String> = None;

        egui::ScrollArea::both()
            .id_salt("editor_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut i = 0usize;
                while i < lines.len() {
                    // Zone repliable démarrant à cette ligne, repliée ou non.
                    let range_here = fold_ranges.iter().find(|f| f.start_line == i);
                    let folded_here = range_here.filter(|f| self.folded_labels.contains(&f.label));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{:>width$}", i + 1)).monospace().color(GUTTER));
                        let job = syntax::highlight(&format!("{}\n", lines[i]), dark, None, None, None);
                        let galley = ui.fonts_mut(|f| f.layout_job(job));
                        if let Some(range) = range_here.filter(|_| folded_here.is_none()) {
                            // Label encore déplié, avec un corps à replier.
                            let resp = ui.add(egui::Label::new(galley).sense(egui::Sense::click()));
                            if resp.clicked() {
                                to_fold = Some(range.label.clone());
                            }
                        } else {
                            ui.add(egui::Label::new(galley));
                        }
                    });
                    if let Some(f) = folded_here {
                        let hidden = f.end_line - f.start_line;
                        ui.horizontal(|ui| {
                            ui.add_space((width + 2) as f32 * 7.0);
                            let text = format!(
                                "⋯ {hidden} {}",
                                tr(
                                    "ligne(s) repliée(s) — cliquer pour déplier",
                                    "line(s) folded — click to unfold",
                                    "línea(s) plegada(s) — clic para desplegar",
                                )
                            );
                            if ui.selectable_label(false, RichText::new(text).italics().color(ACCENT)).clicked() {
                                to_unfold = Some(f.label.clone());
                            }
                        });
                        i = f.end_line + 1;
                    } else {
                        i += 1;
                    }
                }
            });

        if let Some(name) = to_fold {
            self.folded_labels.insert(name);
        }
        if let Some(name) = to_unfold {
            self.folded_labels.remove(&name);
        }
    }

    /// Replie le label de premier niveau qui contient la position du curseur
    /// (telle qu'elle était au dernier rendu éditable — voir `editor_cursor_byte`).
    /// Sans effet si le curseur n'est dans aucun label, ou si celui-ci n'a pas
    /// de corps à replier.
    pub(super) fn fold_label_at_cursor(&mut self) {
        let ranges = compute_fold_ranges(&self.source);
        let byte = self.editor_cursor_byte.min(self.source.len());
        let cursor_line = self.source[..byte].matches('\n').count();
        if let Some(f) = ranges.iter().find(|f| cursor_line >= f.start_line && cursor_line <= f.end_line) {
            self.folded_labels.insert(f.label.clone());
        }
    }

    /// Ctrl+Maj+] : dépliage total (échappatoire simple, sans devoir viser
    /// chaque marqueur un par un).
    pub(super) fn unfold_all(&mut self) {
        self.folded_labels.clear();
    }

    /// Barre de recherche/remplacement (Ctrl+F / Ctrl+H), au-dessus du texte.
    pub(super) fn find_bar_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        let matches = self.find_matches();
        self.find_current = if matches.is_empty() { 0 } else { self.find_current % matches.len() };

        egui::Frame::default()
            .fill(ui.visuals().extreme_bg_color)
            .inner_margin(egui::Margin::symmetric(8, 6))
            .corner_radius(egui::CornerRadius::same(4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.find_query)
                            .id(super::find_query_id())
                            .desired_width(180.0)
                            .hint_text(tr("Rechercher", "Find", "Buscar")),
                    );
                    if resp.changed() {
                        self.find_current = 0;
                    }
                    if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if ui.input(|i| i.modifiers.shift) {
                            self.find_prev();
                        } else {
                            self.find_next();
                        }
                    }

                    if matches.is_empty() {
                        if !self.find_query.is_empty() {
                            ui.colored_label(FALSE_COL, tr("Aucun résultat", "No results", "Sin resultados"));
                        }
                    } else {
                        ui.label(format!("{}/{}", self.find_current + 1, matches.len()));
                    }
                    if ui.small_button("◀").on_hover_text(tr("Précédent", "Previous", "Anterior")).clicked() {
                        self.find_prev();
                    }
                    if ui.small_button("▶").on_hover_text(tr("Suivant", "Next", "Siguiente")).clicked() {
                        self.find_next();
                    }
                    if ui
                        .selectable_label(self.find_case_sensitive, "Aa")
                        .on_hover_text(tr("Respecter la casse", "Match case", "Distinguir mayúsculas"))
                        .clicked()
                    {
                        self.find_case_sensitive = !self.find_case_sensitive;
                        self.find_current = 0;
                    }
                    if ui
                        .small_button("✕")
                        .on_hover_text(tr("Fermer (Échap)", "Close (Esc)", "Cerrar (Esc)"))
                        .clicked()
                    {
                        self.show_find = false;
                    }
                });
                if self.find_replace_mode {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.find_replace_text)
                                .id(super::find_replace_id())
                                .desired_width(180.0)
                                .hint_text(tr("Remplacer par", "Replace with", "Reemplazar con")),
                        );
                        if ui.button(tr("Remplacer", "Replace", "Reemplazar")).clicked() {
                            self.find_replace_current();
                        }
                        if ui.button(tr("Tout remplacer", "Replace all", "Reemplazar todo")).clicked() {
                            self.find_replace_all();
                        }
                    });
                }
            });
    }

    /// Correspondances de la recherche active dans le source courant.
    pub(super) fn find_matches(&self) -> Vec<(usize, usize)> {
        syntax::find_matches(&self.source, &self.find_query, self.find_case_sensitive)
    }

    pub(super) fn find_next(&mut self) {
        let matches = self.find_matches();
        if matches.is_empty() {
            return;
        }
        self.find_current = (self.find_current + 1) % matches.len();
        self.request_scroll_to_current_match(&matches);
    }

    pub(super) fn find_prev(&mut self) {
        let matches = self.find_matches();
        if matches.is_empty() {
            return;
        }
        self.find_current = (self.find_current + matches.len() - 1) % matches.len();
        self.request_scroll_to_current_match(&matches);
    }

    fn request_scroll_to_current_match(&mut self, matches: &[(usize, usize)]) {
        let Some(&(start, _)) = matches.get(self.find_current) else { return };
        let line = self.source[..start].matches('\n').count();
        self.pending_scroll_to_line = Some(line);
    }

    /// Remplace la correspondance active par `find_replace_text`.
    pub(super) fn find_replace_current(&mut self) {
        let matches = self.find_matches();
        let Some(&(s, e)) = matches.get(self.find_current) else { return };
        self.source.replace_range(s..e, &self.find_replace_text);
        self.dirty = true;
    }

    /// Remplace toutes les correspondances. Parcourt de la fin vers le début
    /// pour que les octets déjà remplacés ne décalent pas les offsets suivants.
    pub(super) fn find_replace_all(&mut self) {
        let matches = self.find_matches();
        if matches.is_empty() {
            return;
        }
        for &(s, e) in matches.iter().rev() {
            self.source.replace_range(s..e, &self.find_replace_text);
        }
        self.dirty = true;
        self.find_current = 0;
    }

    /// Repère « ▶ RIP : 0x… mnémonique opérandes », si un programme tourne.
    pub(super) fn rip_banner(&self, ui: &mut egui::Ui) {
        let Some(s) = self.snap() else { return };
        let Some(insn) = self.disasm.iter().find(|i| i.address == s.regs.rip) else { return };
        ui.horizontal(|ui| {
            ui.label(RichText::new("▶").color(ACTION));
            ui.label(
                RichText::new(format!("RIP : 0x{:X}", s.regs.rip))
                    .monospace()
                    .color(self.c_addr()),
            );
            ui.label(
                RichText::new(format!("{} {}", insn.mnemonic, insn.operands))
                    .monospace()
                    .color(self.c_mnemonic()),
            );
        });
    }

    /// Déplace la sélection du désassemblage d'une instruction (clavier).
    /// Sans sélection, part de l'instruction courante (RIP).
    pub(super) fn move_disasm_selection(&mut self, down: bool) {
        if self.disasm.is_empty() {
            return;
        }
        let cur = self
            .selected
            .or_else(|| self.view_rip())
            .and_then(|a| self.disasm.iter().position(|i| i.address == a));
        let next = match (cur, down) {
            (None, _) => 0,
            (Some(i), true) => (i + 1).min(self.disasm.len() - 1),
            (Some(i), false) => i.saturating_sub(1),
        };
        self.selected = Some(self.disasm[next].address);
        self.scroll_to_sel = Some(super::dock::Panel::Disasm);
    }

    pub(super) fn editor_ui(&mut self, ui: &mut egui::Ui) {
        // Ligne source courante (RIP) à surligner pendant le débogage.
        let hl = self.current_source_line();
        let dark = self.dark;

        // Recherche active (Ctrl+F) : surlignage par-dessus la coloration
        // syntaxique, sans le calculer si la barre est fermée ou vide.
        let matches = self.find_matches();
        let current_offset = (!matches.is_empty())
            .then(|| matches[self.find_current % matches.len()].0);
        let find_highlight = (self.show_find && !self.find_query.is_empty()).then_some(syntax::FindHighlight {
            query: self.find_query.as_str(),
            case_sensitive: self.find_case_sensitive,
            current: current_offset,
        });

        // Parenthèse/crochet correspondant à celui que touche le curseur, tel
        // qu'il était à la fin de la frame précédente — un cran de retard
        // imperceptible, dans le même esprit que la ligne RIP surlignée.
        let bracket_match = syntax::matching_bracket(&self.source, self.editor_cursor_byte);

        // Coloration syntaxique NASM (retour à la ligne désactivé => aligné aux numéros).
        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _wrap: f32| {
            ui.fonts_mut(|f| {
                f.layout_job(syntax::highlight(text.as_str(), dark, hl, find_highlight.as_ref(), bracket_match))
            })
        };

        // Gouttière : numéros de ligne (▶ + accent sur la ligne courante).
        let line_count = self.source.matches('\n').count() + 1;
        let width = line_count.to_string().len();
        let gfont = egui::FontId::monospace(syntax::FONT_SIZE);
        let mut gutter_job = egui::text::LayoutJob::default();
        for i in 1..=line_count {
            if i > 1 {
                gutter_job.append("\n", 0.0, egui::TextFormat::default());
            }
            let is_cur = hl == Some(i - 1);
            let (marker, color) = if is_cur { ("▶", ACCENT) } else { (" ", GUTTER) };
            gutter_job.append(
                &format!("{marker}{i:>width$}"),
                0.0,
                egui::TextFormat {
                    font_id: gfont.clone(),
                    color,
                    ..Default::default()
                },
            );
        }

        // Largeur du contenu = ligne la plus longue (pour le scroll horizontal).
        let char_w = ui.fonts_mut(|f| f.glyph_width(&gfont, 'M'));
        let max_cols = self.source.lines().map(|l| l.chars().count()).max().unwrap_or(0);
        let content_w = (max_cols as f32 + 2.0) * char_w;

        // Défilement forcé vers une correspondance de recherche visée
        // (Ctrl+F/F3) : `take()` consomme la demande, la frame suivante rend
        // la main au défilement naturel piloté par l'utilisateur.
        let forced_scroll = self.pending_scroll_to_line.take().map(|line| {
            let row_h = ui.fonts_mut(|f| f.row_height(&gfont));
            (line as f32 * row_h - ui.available_height() / 2.0).max(0.0)
        });

        // Cliché d'avant frappe, pour repérer après coup ce qu'un seul
        // caractère tapé a changé (fermeture automatique des paires).
        let pre_edit_source = self.source.clone();

        // Ligne cliquée dans la gouttière, traitée après le rendu : `self` y
        // est emprunté par le layouter et les zones défilantes.
        let mut gutter_click: Option<usize> = None;

        // Points d'arrêt à peindre, résolus avant le rendu pour la même raison
        // (ligne, porte du code). Tant que rien n'a été assemblé, aucune ligne
        // n'a d'adresse : on ne peut pas encore dire lesquelles portent du
        // code, et tout point d'arrêt est plausible.
        let mapped = !self.src_map.is_empty();
        let bp_marks: Vec<(usize, bool)> = self
            .breakpoints
            .iter()
            .map(|l| (*l, !mapped || self.line_is_executable(*l)))
            .collect();

        // `layouter` emprunte `self.find_query` via `find_highlight` : tant
        // qu'il est capturé par les fermetures ci-dessous, `self` ne peut pas
        // être remprunté en `&mut` — l'auto-fermeture des paires doit donc
        // attendre la sortie de `ui.horizontal_top` pour s'exécuter.
        let (changed, cursor_range) = ui
            .horizontal_top(|ui| {
                // Gouttière : défilement vertical synchronisé, sans barre ni scroll direct.
                egui::ScrollArea::vertical()
                    .id_salt("gutter_scroll")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .scroll_source(egui::scroll_area::ScrollSource::NONE)
                    .auto_shrink([true, false])
                    .vertical_scroll_offset(forced_scroll.unwrap_or(self.editor_scroll_y))
                    .show(ui, |ui| {
                        let galley = ui.fonts_mut(|f| f.layout_job(gutter_job));
                        let row_h = galley.size().y / line_count.max(1) as f32;
                        let resp = ui.add(
                            egui::Label::new(galley)
                                .selectable(false)
                                .sense(egui::Sense::click()),
                        );
                        // Pastilles peintes par-dessus les numéros plutôt
                        // qu'insérées dans le texte : un glyphe de plus dans la
                        // gouttière décalerait les numéros des seules lignes qui
                        // en portent un.
                        let painter = ui.painter();
                        let x = resp.rect.left() + row_h * 0.35;
                        for (line, executable) in &bp_marks {
                            let y = resp.rect.top() + (*line as f32 - 0.5) * row_h;
                            let c = egui::pos2(x, y);
                            let r = row_h * 0.24;
                            // Cercle creux : point d'arrêt posé sur une ligne
                            // sans code (commentaire, `section`, ligne vide),
                            // que l'exécution ne rencontrera jamais.
                            if *executable {
                                painter.circle_filled(c, r, BREAKPOINT);
                            } else {
                                painter.circle_stroke(c, r, (1.5, BREAKPOINT));
                            }
                        }
                        // Clic dans la gouttière : bascule le point d'arrêt de la
                        // ligne visée, si elle porte du code exécutable.
                        if resp.clicked()
                            && let Some(pos) = resp.interact_pointer_pos()
                        {
                            let line = ((pos.y - resp.rect.top()) / row_h) as usize + 1;
                            if line <= line_count {
                                gutter_click = Some(line);
                            }
                        }
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                    });
                ui.separator();
                // Éditeur : défilement vertical + horizontal ; la gouttière reste fixe.
                let mut editor_scroll = egui::ScrollArea::both()
                    .id_salt("editor_scroll")
                    .auto_shrink([false, false]);
                if let Some(y) = forced_scroll {
                    editor_scroll = editor_scroll.vertical_scroll_offset(y);
                }
                let out = editor_scroll.show(ui, |ui| {
                    let out = egui::TextEdit::multiline(&mut self.source)
                        .id(super::editor_id())
                        .frame(false)
                        .code_editor()
                        .desired_width(content_w.max(ui.available_width()))
                        .desired_rows(28)
                        .lock_focus(true)
                        .layouter(&mut layouter)
                        .show(ui);
                    let changed = out.response.changed();
                    if changed {
                        self.dirty = true;
                    }
                    // Position du curseur (Ln/Col) pour la barre d'état.
                    if let Some(range) = out.cursor_range {
                        // Le curseur ne donne plus qu'un index de caractère :
                        // on retrouve ligne et colonne en comptant les sauts de
                        // ligne avant lui.
                        let idx = range.primary.index;
                        let before: String = self.source.chars().take(idx).collect();
                        self.editor_ln = before.matches('\n').count() + 1;
                        self.editor_col = before.chars().rev().take_while(|&c| c != '\n').count() + 1;
                        self.editor_cursor_byte = before.len();
                    }
                    (changed, out.cursor_range)
                });
                // Synchronise la gouttière sur le défilement vertical de l'éditeur.
                self.editor_scroll_y = out.state.offset.y;
                out.inner
            })
            .inner;

        if changed {
            self.auto_pair_after_edit(&pre_edit_source, cursor_range);
        }
        if let Some(line) = gutter_click {
            self.toggle_breakpoint(line);
        }
    }

    /// Après une frappe unique dans l'éditeur, complète les paires ouvrantes
    /// `( [ { " '` par leur fermant, saute par-dessus un fermant déjà présent
    /// plutôt que de le dupliquer, et nettoie une paire vide au Retour arrière.
    ///
    /// Ne manipule jamais le curseur persisté par egui : chaque octet
    /// ajouté/retiré l'est STRICTEMENT avant ou après sa position (jamais À sa
    /// position), ce qui suffit à le laisser au bon endroit à la frame
    /// suivante sans toucher à `egui::Memory`.
    ///
    /// Heuristique volontairement simple : le « saut par-dessus » ne distingue
    /// pas un fermant auto-inséré d'un fermant tapé à la main — cohérent avec
    /// la plupart des éditeurs légers, au prix d'avaler un `)` voulu à dessein
    /// juste avant un `)` existant (cas rare en NASM).
    fn auto_pair_after_edit(&mut self, before: &str, cursor_range: Option<egui::text::CCursorRange>) {
        let Some(range) = cursor_range else { return };
        if range.primary != range.secondary {
            return; // une sélection est encore active : rien à faire.
        }
        let char_idx = range.primary.index;

        // Un caractère vient de disparaître (Retour arrière/Suppr) : si c'est
        // un ouvrant dont le fermant le suit immédiatement à vide, il part avec.
        if self.source.len() + 1 == before.len() {
            let Some(removed) = before.chars().nth(char_idx) else { return };
            let Some(closer) = pair_closer(removed) else { return };
            let byte_idx = byte_offset_of_char(&self.source, char_idx);
            if self.source[byte_idx..].starts_with(closer) {
                self.source.remove(byte_idx);
            }
            return;
        }

        // Un seul caractère vient d'être ajouté (frappe normale — un collage
        // d'un unique caractère est indiscernable et se traite pareil, sans
        // conséquence).
        if self.source.len() != before.len() + 1 || char_idx == 0 {
            return;
        }
        let byte_idx = byte_offset_of_char(&self.source, char_idx);
        let typed = self.source[..byte_idx].chars().next_back().unwrap();

        if let Some(closer) = pair_closer(typed) {
            self.source.insert(byte_idx, closer);
        } else if is_closer(typed) && self.source[byte_idx..].starts_with(typed) {
            self.source.remove(byte_idx - typed.len_utf8());
        }
    }

    pub(super) fn disasm_ui(&mut self, ui: &mut egui::Ui) {
        if self.disasm.is_empty() {
            ui.label(i18n::tr3(self.lang,
                "Cliquez sur « Lancer » pour assembler, lier et exécuter votre programme.",
                "Click \"Run\" to assemble, link and execute your program.",
                "Haga clic en «Ejecutar» para ensamblar, enlazar y ejecutar su programa.",
            ));
            return;
        }
        let rip = self.view_rip();
        let scroll_here = self.take_scroll_request(super::dock::Panel::Disasm);
        let mut clicked: Option<u64> = None;
        // Défilement sur les DEUX axes, comme CALL STACK et SYSCALLS : une
        // instruction à opérandes mémoire (« mov qword [rbp - 0x18], rax »)
        // dépasse une colonne étroite, et la tronquer n'apprend rien.
        // `auto_shrink` désactivé : sans cela la zone se rétrécit sur son
        // contenu et le panneau laisse un large vide à sa droite.
        egui::ScrollArea::both()
            .id_salt("disasm_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
            // Le surlignage d'une ligne déborde de 2 px en haut pour se souder à
            // la ligne suivante. Sans cette marge, celui de la PREMIÈRE ligne
            // dépassait au-dessus de la zone : il s'y faisait rogner à plat,
            // dessinant un liseré collé sous la barre d'onglets.
            ui.add_space(2.0);
            // Largeur de la ligne : tout le panneau, ou le contenu s'il est plus
            // large (défilement horizontal). Le surlignage et la zone cliquable
            // couvrent alors la ligne entière, et non le seul texte.
            let full = ui.max_rect();
            for insn in &self.disasm {
                let is_current = Some(insn.address) == rip;
                let is_selected = Some(insn.address) == self.selected;
                // Forme de fond réservée AVANT le contenu => dessinée derrière le
                // texte (sinon le rectangle masquerait l'instruction).
                let bg = ui.painter().add(egui::Shape::Noop);
                let inner = ui.horizontal(|ui| {
                    if is_current {
                        ui.label(RichText::new("➤").color(CHANGED));
                    } else {
                        ui.label("    ");
                    }
                    ui.label(RichText::new(format!("0x{:08X}", insn.address)).monospace().color(self.c_addr()));
                    ui.label(RichText::new(format!("{:<20}", insn.bytes_hex())).monospace().color(self.c_bytes()));
                    ui.label(RichText::new(format!("{:<7}", insn.mnemonic)).monospace().color(self.c_mnemonic()));
                    ui.add(egui::Label::new(RichText::new(&insn.operands).monospace()).extend());
                });
                let mut rect = inner.response.rect;
                rect.min.x = full.left();
                rect.max.x = rect.max.x.max(full.right());
                let row = ui.interact(
                    rect,
                    ui.id().with(("disasm_row", insn.address)),
                    egui::Sense::click(),
                );
                if row.clicked() {
                    clicked = Some(insn.address);
                }
                // La sélection déplacée au clavier doit rester visible.
                if is_selected && scroll_here {
                    row.scroll_to_me(Some(egui::Align::Center));
                }
                let fill = if is_current {
                    Some(self.c_rip_row())
                } else if is_selected {
                    Some(self.c_sel_row())
                } else if row.hovered() {
                    Some(self.c_sel_row().linear_multiply(0.5))
                } else {
                    None
                };
                if let Some(color) = fill {
                    let rect = rect.expand2(egui::vec2(0.0, 2.0));
                    ui.painter().set(bg, egui::Shape::rect_filled(rect, 3.0, color));
                }
            }
        });
        if let Some(addr) = clicked {
            self.selected = if self.selected == Some(addr) { None } else { Some(addr) };
        }
    }

    // ---------- Panneau INSTRUCTION ----------

    pub(super) fn instruction_ui(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let tr = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);

        // FLAGS n'est plus épinglé ici : c'est un panneau ancrable à part
        // entière, et le garder aussi en bas de INSTRUCTION affichait les mêmes
        // six drapeaux deux fois à l'écran.
        let target = self.selected.or_else(|| self.view_rip());
        let Some(addr) = target else {
            ui.label(tr(
                "Lancez le programme, puis cliquez une instruction.",
                "Run the program, then click an instruction.",
                "Ejecute el programa y luego haga clic en una instrucción.",
            ));
            return;
        };
        let Some(insn) = self.disasm.iter().find(|i| i.address == addr).cloned() else {
            ui.label("—");
            return;
        };
        let flags = self.snap().map(|s| Flags::from_eflags(s.regs.eflags)).unwrap_or_default();
        let e = explain::explain(&insn.mnemonic, &insn.operands, flags, self.lang);
        let mnem_col = self.c_mnemonic();

        // Ligne 1 : nom de l'instruction + bouton Microscope (aligné à droite).
        ui.horizontal(|ui| {
            ui.label(RichText::new(&e.title).size(16.0).strong().color(mnem_col));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(tr("🔬 Microscope", "🔬 Microscope", "🔬 Microscopio"))
                    .on_hover_text(tr("Tout voir sur cette seule instruction", "See everything about this one instruction", "Ver todo sobre esta instrucción"))
                    .clicked()
                {
                    self.microscope = Some(addr);
                }
            });
        });
        // Ligne 2 : catégorie + repère (instruction courante / sélection) à droite.
        ui.horizontal(|ui| {
            ui.label(RichText::new(e.category).italics().weak().size(12.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let tag = if self.selected.is_some() {
                    tr("(sélection)", "(selection)", "(selección)")
                } else {
                    tr("(instruction courante)", "(current instruction)", "(instrucción actual)")
                };
                ui.label(RichText::new(tag).small().weak());
            });
        });

        // Pastille syscall : numéro RAX + nom, sur une ligne compacte.
        if insn.mnemonic == "syscall"
            && let Some(snap) = self.snap()
        {
            let rax = snap.regs.rax;
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                badge(ui, &format!("#{rax}"), self.c_bytes());
                ui.label(
                    RichText::new(syscall::name(rax))
                        .monospace()
                        .strong()
                        .color(self.c_mnemonic()),
                );
            });
        }

        // Étape de cadre d'appel : replace l'instruction dans la mécanique du
        // prologue/épilogue, que le panneau CALL STACK ne raconte pas.
        if let Some(phase) = crate::abi::frame_phase(&insn.mnemonic, &insn.operands) {
            ui.add_space(6.0);
            egui::Frame::default()
                .fill(ACTION.linear_multiply(0.12))
                .stroke(egui::Stroke::new(1.0_f32, ACTION.linear_multiply(0.6)))
                .corner_radius(egui::CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(tr("🧱 Cadre d'appel", "🧱 Call frame", "🧱 Marco de llamada"))
                            .small()
                            .strong()
                            .color(ACTION),
                    );
                    ui.add_space(2.0);
                    ui.label(RichText::new(phase.explain(self.lang)).size(12.5));
                });
        }

        // Registres d'arguments d'un appel de fonction : sur un « call », c'est
        // là que l'ordre System V prend son sens.
        if insn.mnemonic == "call"
            && let Some(snap) = self.snap()
        {
            let hdr2 = self.c_header();
            let regs = snap.regs.clone();
            ui.add_space(6.0);
            card(ui, |ui| {
                ui.label(
                    RichText::new(tr("Arguments (ABI System V)", "Arguments (System V ABI)", "Argumentos (ABI System V)"))
                        .small()
                        .strong()
                        .color(hdr2),
                );
                egui::Grid::new("abi_call_args").num_columns(3).spacing([8.0, 2.0]).show(ui, |ui| {
                    for (i, rn) in crate::abi::ARG_REGS.iter().enumerate() {
                        let v = regs.named().iter().find(|(n, _)| n == rn).map(|(_, v)| *v).unwrap_or(0);
                        ui.label(RichText::new(format!("arg{}", i + 1)).small().weak());
                        ui.label(RichText::new(*rn).monospace().small().color(hdr2));
                        ui.label(RichText::new(format!("0x{v:X}")).monospace().small());
                        ui.end_row();
                    }
                });
                ui.label(
                    RichText::new(tr(
                        "La valeur de retour reviendra dans RAX. RBX, R12–R15 seront intacts ; \
                         les autres peuvent être écrasés.",
                        "The return value will come back in RAX. RBX, R12–R15 will be intact; \
                         the others may be clobbered.",
                        "El valor de retorno volverá en RAX. RBX, R12–R15 quedarán intactos; \
                         los demás pueden ser sobrescritos.",
                    ))
                    .small()
                    .weak(),
                );
            });
        }

        // Registres d'arguments pour un appel système : donne le sens des
        // valeurs plutôt que de laisser l'élève deviner l'ordre.
        if insn.mnemonic == "syscall"
            && let Some(snap) = self.snap()
        {
            let hdr2 = self.c_header();
            let regs = snap.regs.clone();
            ui.add_space(6.0);
            card(ui, |ui| {
                ui.label(
                    RichText::new(tr("Arguments (ABI syscall)", "Arguments (syscall ABI)", "Argumentos (ABI syscall)"))
                        .small()
                        .strong()
                        .color(hdr2),
                );
                egui::Grid::new("abi_syscall_args").num_columns(3).spacing([8.0, 2.0]).show(ui, |ui| {
                    for (i, rn) in crate::abi::SYSCALL_ARG_REGS.iter().enumerate() {
                        let v = regs.named().iter().find(|(n, _)| n == rn).map(|(_, v)| *v).unwrap_or(0);
                        ui.label(RichText::new(format!("arg{}", i + 1)).small().weak());
                        ui.label(RichText::new(*rn).monospace().small().color(hdr2));
                        ui.label(RichText::new(format!("0x{v:X}")).monospace().small());
                        ui.end_row();
                    }
                });
                ui.label(
                    RichText::new(tr(
                        "syscall écrase RCX et R11 : le 4e argument passe par R10.",
                        "syscall clobbers RCX and R11: the 4th argument goes through R10.",
                        "syscall sobrescribe RCX y R11: el 4.º argumento pasa por R10.",
                    ))
                    .small()
                    .weak(),
                );
            });
        }

        // Description pédagogique + lien vers la référence officielle.
        ui.add_space(8.0);
        card(ui, |ui| {
            ui.label(RichText::new(&e.description).size(13.0));
        });
        ui.add_space(6.0);
        ui.hyperlink_to(
            format!("📖 {} {} ↗", tr("Référence Intel de", "Intel reference for", "Referencia Intel de"), insn.mnemonic.to_uppercase()),
            explain::doc_url(&insn.mnemonic),
        )
        .on_hover_text(tr(
            "Ouvre la page de l'instruction (manuel Intel SDM, felixcloutier.com)",
            "Opens the instruction page (Intel SDM manual, felixcloutier.com)",
            "Abre la página de la instrucción (manual Intel SDM, felixcloutier.com)",
        ));

        if let Some(cond) = &e.condition {
            ui.add_space(4.0);
            ui.label(RichText::new(tr("Condition", "Condition", "Condición")).strong());
            ui.label(RichText::new(cond).monospace());
            // Effet : où mène le saut si la condition est vraie.
            if !insn.operands.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(tr("Effet", "Effect", "Efecto")).strong());
                ui.label(
                    RichText::new(format!(
                        "{} {}.",
                        tr("Si la condition est vraie, RIP =", "If the condition is true, RIP =", "Si la condición es verdadera, RIP ="),
                        insn.operands
                    ))
                    .monospace(),
                );
            }
            ui.add_space(4.0);
            let hdr2 = self.c_header();
            card(ui, |ui| {
                    ui.label(RichText::new(tr("État actuel", "Current state", "Estado actual")).small().strong().color(hdr2));
                    ui.horizontal(|ui| {
                        for (name, val) in &e.relevant_flags {
                            let c = if *val { FLAG_ON } else { FLAG_OFF };
                            ui.label(
                                RichText::new(format!("{name} = {}", *val as u8))
                                    .monospace()
                                    .color(c),
                            );
                        }
                    });
                    if let Some(taken) = e.taken {
                        ui.add_space(4.0);
                        let (txt, col) = if taken {
                            (tr("✔ Condition vraie — le saut sera pris.", "✔ Condition true — the jump will be taken.", "✔ Condición verdadera — el salto se tomará."), FLAG_ON)
                        } else {
                            (tr("✘ Condition fausse — pas de saut.", "✘ Condition false — no jump.", "✘ Condición falsa — sin salto."), FALSE_COL)
                        };
                        let fill = if taken {
                            FLAG_ON.linear_multiply(0.12)
                        } else {
                            FALSE_COL.linear_multiply(0.12)
                        };
                        egui::Frame::default()
                            .fill(fill)
                            .inner_margin(egui::Margin::symmetric(8, 4))
                            .corner_radius(egui::CornerRadius::same(4))
                            .show(ui, |ui| {
                                ui.label(RichText::new(txt).color(col).strong());
                            });
                    }
                });
        }
        if !e.affects_flags.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(tr("Flags positionnés", "Flags set", "Flags activos")).strong());
            ui.label(RichText::new(e.affects_flags.join("  ")).monospace().color(CHANGED));
        }
    }
}

/// Fermant attendu pour un caractère ouvrant de paire, sinon `None`.
fn pair_closer(opener: char) -> Option<char> {
    match opener {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        '\'' => Some('\''),
        _ => None,
    }
}

fn is_closer(c: char) -> bool {
    matches!(c, ')' | ']' | '}' | '"' | '\'')
}

/// Offset en octets du `char_idx`-ième caractère de `s` (fin de chaîne si
/// hors bornes — un curseur en bout de texte est un cas normal).
fn byte_offset_of_char(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(s.len())
}

// ---------- Pliage de code ----------
//
// Repli « simple » : uniquement les labels de PREMIER NIVEAU (`_start:`, pas
// `.loop:`) dont le corps n'est pas vide. Un label local imbriqué se replie
// avec le label qui le précède plutôt que d'offrir son propre repli — évite
// d'avoir à suivre une hiérarchie d'indentation pour un bénéfice marginal.

/// Une zone repliable : `start_line` (le label, toujours visible) jusqu'à
/// `end_line` inclus (dernière ligne de son corps).
#[derive(Debug)]
struct FoldRange {
    start_line: usize,
    end_line: usize,
    label: String,
}

fn is_label_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || matches!(c, '_' | '@' | '$')
}

fn is_label_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '$')
}

/// La ligne déclare-t-elle un label de premier niveau (`foo:`, pas `.foo:`) ?
fn top_level_label(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with(is_label_ident_start) {
        return None;
    }
    let end = trimmed.find(|c: char| !is_label_ident_char(c)).unwrap_or(trimmed.len());
    trimmed[end..].starts_with(':').then(|| &trimmed[..end])
}

/// Zones repliables du source : un label de premier niveau par zone, avec au
/// moins une ligne de corps avant le label suivant (ou la fin du fichier).
fn compute_fold_ranges(source: &str) -> Vec<FoldRange> {
    let lines: Vec<&str> = source.lines().collect();
    let labels: Vec<(usize, &str)> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| top_level_label(l).map(|name| (i, name)))
        .collect();
    labels
        .iter()
        .enumerate()
        .filter_map(|(k, &(start, name))| {
            let end = labels.get(k + 1).map(|&(l, _)| l - 1).unwrap_or(lines.len().saturating_sub(1));
            (end > start).then(|| FoldRange { start_line: start, end_line: end, label: name.to_string() })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::dock::Panel;
    use crate::disasm::Insn;

    fn insn(addr: u64) -> Insn {
        Insn {
            address: addr,
            bytes: vec![0x90],
            mnemonic: "nop".into(),
            operands: String::new(),
        }
    }

    /// Ctrl+Tab fait défiler les onglets DU NŒUD qui a le focus. Sans nœud
    /// focalisé, l'appel doit rester sans effet plutôt que de paniquer.
    #[test]
    fn tab_cycle_is_safe_without_focus() {
        let mut app = App::new();
        app.set_ui_mode(crate::app::UiMode::Full);
        app.cycle_tab(false);
        app.cycle_tab(true);
        // La disposition reste intacte.
        assert!(app.panel_is_open(Panel::Editor));
        assert!(app.panel_is_open(Panel::Disasm));
    }

    /// Après focalisation d'un panneau, le cycle change bien l'onglet actif du
    /// nœud — Éditeur / Désassemblage / Vue mémoire partagent le même nœud.
    #[test]
    fn tab_cycle_moves_within_the_focused_node() {
        let mut app = App::new();
        // L'éditeur ne partage un nœud avec le désassemblage qu'en mode complet.
        app.set_ui_mode(crate::app::UiMode::Full);
        app.focus_panel(Panel::Editor);
        assert_eq!(app.focused_panel(), Some(Panel::Editor));

        app.cycle_tab(false);
        let after = app.focused_panel();
        assert_ne!(after, Some(Panel::Editor), "le cycle doit changer d'onglet");
        assert!(
            matches!(after, Some(Panel::Disasm) | Some(Panel::MemMap)),
            "on reste dans le nœud du centre, obtenu : {after:?}"
        );

        // Un tour complet ramène à l'éditeur.
        app.cycle_tab(false);
        app.cycle_tab(false);
        assert_eq!(app.focused_panel(), Some(Panel::Editor));
    }

    /// Les flèches parcourent le désassemblage et s'arrêtent aux bornes.
    #[test]
    fn disasm_selection_moves_and_clamps() {
        let mut app = App::new();
        app.disasm = vec![insn(0x10), insn(0x20), insn(0x30)];

        app.move_disasm_selection(true);
        assert_eq!(app.selected, Some(0x10));
        app.move_disasm_selection(true);
        assert_eq!(app.selected, Some(0x20));
        app.move_disasm_selection(true);
        assert_eq!(app.selected, Some(0x30));
        app.move_disasm_selection(true);
        assert_eq!(app.selected, Some(0x30), "borne basse respectée");

        app.move_disasm_selection(false);
        assert_eq!(app.selected, Some(0x20));
        app.move_disasm_selection(false);
        assert_eq!(app.selected, Some(0x10));
        app.move_disasm_selection(false);
        assert_eq!(app.selected, Some(0x10), "borne haute respectée");
    }

    /// Peint le désassemblage dans un panneau de taille connue et renvoie
    /// (zone du panneau, rectangle du surlignage de la ligne sélectionnée).
    fn disasm_row_highlight(app: &mut App) -> (egui::Rect, egui::Rect) {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 600.0))),
            ..Default::default()
        };
        let sel = app.c_sel_row();
        let mut avail = egui::Rect::ZERO;
        let out = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                avail = ui.max_rect();
                app.disasm_ui(ui);
            });
        });
        // Repéré par sa COULEUR, et non par sa position : un surlignage mal
        // placé doit être trouvé puis jugé, pas passer pour absent.
        let row = out
            .shapes
            .iter()
            .find_map(|cs| match &cs.shape {
                egui::Shape::Rect(r) if r.fill == sel => Some(r.rect),
                _ => None,
            })
            .expect("la ligne sélectionnée doit être surlignée");
        (avail, row)
    }

    /// Le panneau occupait naguère la largeur de son texte (≈ 330 px sur 884) :
    /// le surlignage et la zone cliquable s'arrêtaient au dernier caractère,
    /// laissant un grand vide à droite. Une ligne doit couvrir tout le panneau.
    #[test]
    fn disasm_rows_span_the_full_panel_width() {
        let mut app = App::new();
        app.disasm = vec![insn(0x10), insn(0x20), insn(0x30)];
        app.selected = Some(0x20);
        let (avail, row) = disasm_row_highlight(&mut app);
        assert!(
            (row.left() - avail.left()).abs() < 1.0 && (row.right() - avail.right()).abs() < 1.0,
            "ligne large de {} sur {} disponibles ({row:?} dans {avail:?})",
            row.width(),
            avail.width()
        );
    }

    /// Le surlignage déborde de 2 px vers le haut pour se souder à la ligne
    /// suivante. Sur la PREMIÈRE ligne ce débord sortait de la zone et s'y
    /// faisait rogner à plat : un liseré parasite sous la barre d'onglets.
    #[test]
    fn first_disasm_row_does_not_bleed_above_the_panel() {
        let mut app = App::new();
        app.disasm = vec![insn(0x10), insn(0x20), insn(0x30)];
        app.selected = Some(0x10);
        let (avail, row) = disasm_row_highlight(&mut app);
        assert!(
            row.top() >= avail.top() - 0.01,
            "le surlignage remonte à {} au-dessus du bord {}",
            row.top(),
            avail.top()
        );
    }

    #[test]
    fn disasm_selection_is_safe_when_empty() {
        let mut app = App::new();
        app.disasm.clear();
        app.move_disasm_selection(true);
        assert_eq!(app.selected, None);
    }

    // ---------- Recherche / remplacement ----------

    fn app_with_source(src: &str) -> App {
        let mut app = App::new();
        app.source = src.to_string();
        app
    }

    #[test]
    fn find_next_cycles_through_matches_and_wraps() {
        let mut app = app_with_source("mov rax, rax\nadd rax, 1\n");
        app.find_query = "rax".into();
        assert_eq!(app.find_current, 0);

        app.find_next();
        assert_eq!(app.find_current, 1);
        app.find_next();
        assert_eq!(app.find_current, 2);
        app.find_next();
        assert_eq!(app.find_current, 0, "doit boucler après la dernière correspondance");
    }

    #[test]
    fn find_prev_wraps_backwards() {
        let mut app = app_with_source("rax rax rax\n");
        app.find_query = "rax".into();
        app.find_prev();
        assert_eq!(app.find_current, 2, "depuis 0, précédent boucle sur la dernière");
    }

    #[test]
    fn find_next_prev_are_safe_without_matches() {
        let mut app = app_with_source("mov rbx, 1\n");
        app.find_query = "introuvable".into();
        app.find_next();
        app.find_prev();
        assert_eq!(app.find_current, 0);
    }

    #[test]
    fn find_next_requests_a_scroll_to_the_matched_line() {
        let mut app = app_with_source("mov rbx, 1\nmov rcx, 2\nmov rax, 3\n");
        app.find_query = "rax".into();
        app.find_next();
        assert_eq!(app.pending_scroll_to_line, Some(2), "\"rax\" est sur la 3e ligne (0-based: 2)");
    }

    #[test]
    fn find_replace_current_only_touches_the_active_match() {
        let mut app = app_with_source("mov rax, rax\n");
        app.find_query = "rax".into();
        app.find_replace_text = "rbx".into();
        app.find_current = 1; // la seconde occurrence
        app.find_replace_current();
        assert_eq!(app.source, "mov rax, rbx\n");
        assert!(app.dirty);
    }

    #[test]
    fn find_replace_all_handles_every_occurrence_without_shifting_offsets() {
        let mut app = app_with_source("rax rax rax\n");
        app.find_query = "rax".into();
        app.find_replace_text = "rbx".into();
        app.find_replace_all();
        assert_eq!(app.source, "rbx rbx rbx\n");
    }

    #[test]
    fn find_replace_all_is_safe_without_matches() {
        let mut app = app_with_source("mov rbx, 1\n");
        app.find_query = "introuvable".into();
        app.find_replace_text = "x".into();
        app.find_replace_all();
        assert_eq!(app.source, "mov rbx, 1\n", "rien à remplacer, rien ne doit changer");
    }

    #[test]
    fn find_respects_case_sensitivity_toggle() {
        let mut app = app_with_source("mov RAX, rax\n");
        app.find_query = "rax".into();
        assert_eq!(app.find_matches().len(), 2, "insensible à la casse par défaut");
        app.find_case_sensitive = true;
        assert_eq!(app.find_matches().len(), 1);
    }

    // ---------- Auto-fermeture des paires ----------

    fn ccursor(idx: usize) -> Option<egui::text::CCursorRange> {
        Some(egui::text::CCursorRange::one(egui::text::CCursor::new(idx)))
    }

    #[test]
    fn typing_an_opener_inserts_its_closer() {
        let mut app = app_with_source("x(");
        app.auto_pair_after_edit("x", ccursor(2));
        assert_eq!(app.source, "x()");
    }

    #[test]
    fn typing_a_quote_inserts_its_matching_quote() {
        let mut app = app_with_source("x\"");
        app.auto_pair_after_edit("x", ccursor(2));
        assert_eq!(app.source, "x\"\"");
    }

    #[test]
    fn typing_a_closer_right_before_an_existing_one_skips_over_it() {
        // Avant la frappe : « x) », curseur juste avant le « ) » existant.
        // egui a déjà inséré le « ) » tapé : « x)) », curseur à l'index 2.
        let mut app = app_with_source("x))");
        app.auto_pair_after_edit("x)", ccursor(2));
        assert_eq!(app.source, "x)", "le ) tapé en trop doit disparaître, pas être dupliqué");
    }

    #[test]
    fn backspacing_an_opener_removes_its_empty_matching_closer_too() {
        // Avant : « x() », curseur après le « ( » (index 2). Retour arrière
        // supprime le « ( » : source « x) », curseur à l'index 1.
        let mut app = app_with_source("x)");
        app.auto_pair_after_edit("x()", ccursor(1));
        assert_eq!(app.source, "x", "l'ouvrant ET son fermant vide doivent partir ensemble");
    }

    #[test]
    fn backspacing_an_opener_with_content_inside_only_removes_the_opener() {
        // « x(y) » -> Retour arrière sur « ( » -> « xy) » : le « ) » ne doit
        // pas partir, il ne suit plus immédiatement (du contenu les sépare).
        let mut app = app_with_source("xy)");
        app.auto_pair_after_edit("x(y)", ccursor(1));
        assert_eq!(app.source, "xy)", "rien de plus à supprimer : le fermant n'est pas adjacent");
    }

    #[test]
    fn ordinary_typing_is_left_untouched() {
        let mut app = app_with_source("ab");
        app.auto_pair_after_edit("a", ccursor(2));
        assert_eq!(app.source, "ab", "une lettre ordinaire ne déclenche aucune paire");
    }

    #[test]
    fn multi_character_changes_are_never_reinterpreted_as_typing() {
        // Un collage ou un Ctrl+Z peut changer la longueur de plus d'un
        // caractère : l'auto-fermeture ne doit jamais s'en mêler.
        let mut app = app_with_source("abc(");
        app.auto_pair_after_edit("a", ccursor(4));
        assert_eq!(app.source, "abc(", "changement de plus d'un caractère : on n'y touche pas");
    }

    #[test]
    fn a_selection_still_active_after_the_edit_is_left_alone() {
        let mut app = app_with_source("x(");
        let range = egui::text::CCursorRange {
            primary: egui::text::CCursor::new(2),
            secondary: egui::text::CCursor::new(0),
            h_pos: None,
        };
        app.auto_pair_after_edit("x", Some(range));
        assert_eq!(app.source, "x(", "une sélection encore active : on ne complète rien");
    }

    // ---------- Pliage de code ----------

    #[test]
    fn top_level_label_is_recognised_and_local_labels_are_not() {
        assert_eq!(top_level_label("_start:"), Some("_start"));
        assert_eq!(top_level_label("  main:"), Some("main"), "l'indentation ne change rien");
        assert_eq!(top_level_label("_start: mov rax, 1"), Some("_start"), "code sur la même ligne toléré");
        assert_eq!(top_level_label(".loop:"), None, "un label local n'a pas son propre repli");
        assert_eq!(top_level_label("mov rax, 1"), None);
        assert_eq!(top_level_label("; _start: en commentaire"), None);
    }

    #[test]
    fn compute_fold_ranges_covers_the_body_up_to_the_next_top_level_label() {
        let src = "_start:\n mov rax, 1\n.loop:\n mov rbx, 2\nend:\n mov rcx, 3\n";
        let ranges = compute_fold_ranges(src);
        assert_eq!(ranges.len(), 2, "{ranges:?}", );
        assert_eq!(ranges[0].label, "_start");
        assert_eq!(ranges[0].start_line, 0);
        assert_eq!(ranges[0].end_line, 3, "le corps de _start engloutit .loop, jusqu'à end (exclu)");
        assert_eq!(ranges[1].label, "end");
        assert_eq!(ranges[1].start_line, 4);
        assert_eq!(ranges[1].end_line, 5, "dernier label : corps jusqu'à la fin du fichier");
    }

    #[test]
    fn a_label_immediately_followed_by_another_has_no_fold_range() {
        // Corps vide : rien à replier, la ligne ne doit pas apparaître.
        let src = "foo:\nbar:\n mov rax, 1\n";
        let ranges = compute_fold_ranges(src);
        assert_eq!(ranges.len(), 1, "{ranges:?}");
        assert_eq!(ranges[0].label, "bar");
    }

    #[test]
    fn compute_fold_ranges_is_empty_without_any_label() {
        assert!(compute_fold_ranges("mov rax, 1\nmov rbx, 2\n").is_empty());
    }

    #[test]
    fn fold_label_at_cursor_folds_the_label_containing_the_cursor_line() {
        let mut app = app_with_source("_start:\n mov rax, 1\n mov rbx, 2\n");
        // Curseur sur la ligne 1 (« mov rax, 1 ») : à l'intérieur du corps de _start.
        app.editor_cursor_byte = app.source.find("mov rax").unwrap();
        app.fold_label_at_cursor();
        assert!(app.folded_labels.contains("_start"));
    }

    #[test]
    fn fold_label_at_cursor_is_a_no_op_outside_any_foldable_body() {
        let mut app = app_with_source("mov rax, 1\n");
        app.editor_cursor_byte = 0;
        app.fold_label_at_cursor();
        assert!(app.folded_labels.is_empty(), "aucun label ici : rien à replier");
    }

    #[test]
    fn unfold_all_clears_every_fold() {
        let mut app = app_with_source("_start:\n mov rax, 1\nend:\n mov rbx, 2\n");
        app.folded_labels.insert("_start".to_string());
        app.folded_labels.insert("end".to_string());
        app.unfold_all();
        assert!(app.folded_labels.is_empty());
    }

    /// La bascule éditable ↔ lecture seule (dans `editor_tab_ui`) et le rendu
    /// replié lui-même ne doivent jamais paniquer, replis actifs ou non.
    #[test]
    fn folded_view_renders_without_panicking() {
        let mut app = app_with_source("_start:\n mov rax, 1\n mov rbx, 2\nend:\n mov rcx, 3\n");
        app.set_ui_mode(crate::app::UiMode::Full);
        app.folded_labels.insert("_start".to_string());
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.dock_ui(ctx));
    }

    /// Replier le DERNIER label (corps jusqu'à la fin du fichier, sans ligne
    /// suivante) ne doit pas paniquer — cas limite de `end_line`.
    #[test]
    fn folding_the_last_label_up_to_end_of_file_renders_without_panicking() {
        let mut app = app_with_source("_start:\n mov rax, 1\nend:\n mov rbx, 2\n mov rcx, 3\n");
        app.set_ui_mode(crate::app::UiMode::Full);
        app.folded_labels.insert("end".to_string());
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.dock_ui(ctx));
    }

    /// Un nom de fichier sans retour à la ligne final ne doit pas non plus
    /// faire paniquer le calcul des replis ni le rendu.
    #[test]
    fn source_without_a_trailing_newline_is_safe() {
        let mut app = app_with_source("_start:\n mov rax, 1");
        app.set_ui_mode(crate::app::UiMode::Full);
        assert_eq!(compute_fold_ranges(&app.source).len(), 1);
        app.folded_labels.insert("_start".to_string());
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| app.dock_ui(ctx));
    }
}
