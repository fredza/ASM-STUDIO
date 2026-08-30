//! Les gestes d'édition qu'on attend d'un éditeur : indenter, commenter,
//! déplacer, dupliquer, supprimer une ligne, aller à un numéro de ligne.
//!
//! Tout ce qui touche au TEXTE est écrit ici en **fonctions pures** : elles
//! prennent le source et la sélection (en caractères, comme le curseur d'egui),
//! rendent le nouveau source et la nouvelle sélection, et ne connaissent ni
//! `App` ni egui. C'est ce qui les rend testables sans ouvrir de fenêtre — et
//! ce sont exactement les fonctions où une erreur d'indice se voit le moins à
//! la relecture.
//!
//! Les indices sont en **caractères** et non en octets : c'est l'unité du
//! curseur d'egui (`CCursor`), et mélanger les deux décale tout dès qu'un
//! commentaire contient un accent.

use eframe::egui;

use super::App;

/// Un cran d'indentation. Quatre espaces plutôt qu'une tabulation : la largeur
/// d'un `\t` dépend de l'éditeur qui relira le fichier, et un source
/// d'assembleur s'aligne en colonnes.
pub(super) const INDENT: &str = "    ";
const INDENT_WIDTH: usize = 4;

/// Le résultat d'une opération d'édition : le texte obtenu et la sélection à
/// replacer (indices de caractères, début et fin).
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Edit {
    pub(super) text: String,
    pub(super) sel: (usize, usize),
}

/// Les lignes du texte, chacune gardant son `\n` final s'il y en a un, avec
/// l'indice (en caractères) de leur premier caractère.
fn lines_with_offsets(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut chars = 0usize;
    for line in text.split_inclusive('\n') {
        out.push((chars, line));
        chars += line.chars().count();
    }
    // Un texte vide n'a aucune ligne pour `split_inclusive`, alors qu'on peut
    // parfaitement y poser le curseur : sans cette ligne vide, la moindre
    // opération y accéderait hors bornes.
    //
    // En revanche, le `\n` final d'un fichier n'ouvre PAS de ligne
    // supplémentaire : « un\ndeux\n » en compte deux, comme `str::lines`. Sinon
    // « descendre la dernière ligne » l'échangerait avec un fantôme, et y
    // insérerait une ligne vide.
    if out.is_empty() {
        out.push((0, ""));
    }
    out
}

/// Numéros (0-based) des lignes touchées par la sélection `[a, b]`, bornes
/// comprises.
///
/// Une sélection qui s'arrête PILE au début d'une ligne n'inclut pas cette
/// ligne : sélectionner trois lignes entières à la souris place la fin sur la
/// quatrième, qu'on ne veut évidemment pas voir commentée avec les autres.
fn touched_lines(text: &str, a: usize, b: usize) -> (usize, usize) {
    let lines = lines_with_offsets(text);
    let (a, b) = (a.min(b), a.max(b));
    let line_of = |pos: usize| {
        lines
            .iter()
            .rposition(|(start, _)| *start <= pos)
            .unwrap_or(0)
    };
    let first = line_of(a);
    let mut last = line_of(b);
    if last > first && lines[last].0 == b {
        last -= 1;
    }
    (first, last)
}

/// Recompose le texte à partir de ses lignes.
fn join(lines: &[String]) -> String {
    lines.concat()
}

/// Le texte découpé en lignes possédées (chacune avec son `\n`).
fn owned_lines(text: &str) -> Vec<String> {
    lines_with_offsets(text).into_iter().map(|(_, l)| l.to_string()).collect()
}

/// Indice du premier caractère de la ligne `n`.
fn line_start(lines: &[String], n: usize) -> usize {
    lines.iter().take(n).map(|l| l.chars().count()).sum()
}

/// L'indentation en tête de `line` (espaces et tabulations), en caractères.
pub(super) fn leading_indent(line: &str) -> &str {
    let end = line.find(|c: char| c != ' ' && c != '\t').unwrap_or(line.len());
    &line[..end]
}

/// Ajoute un cran d'indentation à chaque ligne touchée.
///
/// Les lignes vides sont laissées telles quelles : les indenter ne produirait
/// que des espaces en fin de ligne, invisibles et pénibles à retirer ensuite.
pub(super) fn indent(text: &str, sel: (usize, usize)) -> Edit {
    let mut lines = owned_lines(text);
    let (first, last) = touched_lines(text, sel.0, sel.1);
    let mut added_before_start = 0usize;
    let mut added_total = 0usize;
    for (n, line) in lines.iter_mut().enumerate().take(last + 1).skip(first) {
        if line.trim().is_empty() {
            continue;
        }
        line.insert_str(0, INDENT);
        added_total += INDENT_WIDTH;
        if n == first {
            added_before_start = INDENT_WIDTH;
        }
    }
    let (a, b) = (sel.0.min(sel.1), sel.0.max(sel.1));
    Edit {
        text: join(&lines),
        sel: (a + added_before_start, b + added_total),
    }
}

/// Retire un cran d'indentation à chaque ligne touchée (au plus : une ligne
/// moins indentée que ça revient simplement en colonne zéro).
pub(super) fn outdent(text: &str, sel: (usize, usize)) -> Edit {
    let mut lines = owned_lines(text);
    let (first, last) = touched_lines(text, sel.0, sel.1);
    let mut removed_before_start = 0usize;
    let mut removed_total = 0usize;
    for (n, line) in lines.iter_mut().enumerate().take(last + 1).skip(first) {
        let ws = leading_indent(line);
        // Une tabulation compte pour un cran entier, sinon jusqu'à quatre espaces.
        let cut = if ws.starts_with('\t') { 1 } else { ws.chars().take(INDENT_WIDTH).count() };
        if cut == 0 {
            continue;
        }
        line.replace_range(..cut, "");
        removed_total += cut;
        if n == first {
            removed_before_start = cut;
        }
    }
    let (a, b) = (sel.0.min(sel.1), sel.0.max(sel.1));
    Edit {
        text: join(&lines),
        sel: (a.saturating_sub(removed_before_start), b.saturating_sub(removed_total)),
    }
}

/// Remplace la sélection (souvent vide) par un cran d'indentation — le Tab
/// « ordinaire », quand on ne travaille pas sur un bloc de lignes.
pub(super) fn insert_indent(text: &str, sel: (usize, usize)) -> Edit {
    let (a, b) = (sel.0.min(sel.1), sel.0.max(sel.1));
    let mut out: String = text.chars().take(a).collect();
    out.push_str(INDENT);
    out.extend(text.chars().skip(b));
    let pos = a + INDENT_WIDTH;
    Edit { text: out, sel: (pos, pos) }
}

/// Commente ou décommente les lignes touchées, comme dans tous les éditeurs :
/// si elles sont **toutes** déjà commentées on décommente, sinon on commente.
///
/// Le `;` se pose à l'indentation la moins profonde du bloc, pas en colonne
/// zéro : le code reste aligné pendant qu'il est mis de côté.
pub(super) fn toggle_comment(text: &str, sel: (usize, usize)) -> Edit {
    let mut lines = owned_lines(text);
    let (first, last) = touched_lines(text, sel.0, sel.1);
    let body = |l: &str| l.trim_end_matches(['\n', '\r']).trim().to_string();
    let content: Vec<String> = (first..=last).map(|n| body(&lines[n])).collect();
    // Les lignes vides ne comptent ni pour ni contre : un bloc dont toutes les
    // lignes pleines sont commentées se décommente, même s'il est aéré.
    let filled: Vec<&String> = content.iter().filter(|c| !c.is_empty()).collect();
    if filled.is_empty() {
        return Edit { text: text.to_string(), sel };
    }
    let all_commented = filled.iter().all(|c| c.starts_with(';'));

    let column = (first..=last)
        .filter(|n| !body(&lines[*n]).is_empty())
        .map(|n| leading_indent(&lines[n]).chars().count())
        .min()
        .unwrap_or(0);

    let mut delta_first = 0isize;
    let mut delta_total = 0isize;
    for (n, line) in lines.iter_mut().enumerate().take(last + 1).skip(first) {
        if body(line).is_empty() {
            continue;
        }
        let d = if all_commented { uncomment_line(line) } else { comment_line(line, column) };
        delta_total += d;
        if n == first {
            delta_first = d;
        }
    }
    let (a, b) = (sel.0.min(sel.1), sel.0.max(sel.1));
    let shift = |x: usize, d: isize| (x as isize + d).max(0) as usize;
    Edit {
        text: join(&lines),
        sel: (shift(a, delta_first), shift(b, delta_total)),
    }
}

/// Insère `"; "` à la colonne `column`. Renvoie le nombre de caractères ajoutés.
fn comment_line(line: &mut String, column: usize) -> isize {
    let at = line.char_indices().nth(column).map_or(line.len(), |(b, _)| b);
    line.insert_str(at, "; ");
    2
}

/// Retire le `;` en tête (et l'espace qui le suit s'il y en a un). Renvoie le
/// nombre de caractères retirés, en négatif.
fn uncomment_line(line: &mut String) -> isize {
    let ws = leading_indent(line).len();
    let rest = &line[ws..];
    if !rest.starts_with(';') {
        return 0;
    }
    let cut = if rest[1..].starts_with(' ') { 2 } else { 1 };
    line.replace_range(ws..ws + cut, "");
    -(cut as isize)
}

/// Déplace les lignes touchées d'un rang vers le haut ou vers le bas.
///
/// Sans effet aux extrémités : la première ligne ne monte pas plus haut.
pub(super) fn move_lines(text: &str, sel: (usize, usize), down: bool) -> Edit {
    let mut lines = owned_lines(text);
    let (first, last) = touched_lines(text, sel.0, sel.1);
    if (down && last + 1 >= lines.len()) || (!down && first == 0) {
        return Edit { text: text.to_string(), sel };
    }
    // La dernière ligne du fichier n'a pas de `\n` : la faire passer au milieu
    // souderait deux lignes. On rétablit donc les terminaisons après coup, en
    // gardant l'absence de `\n` pour celle qui finit le fichier.
    let (a, b) = (sel.0.min(sel.1), sel.0.max(sel.1));
    let block: Vec<String> = lines.drain(first..=last).collect();
    let at = if down { first + 1 } else { first - 1 };
    for (k, l) in block.into_iter().enumerate() {
        lines.insert(at + k, l);
    }
    fix_line_endings(&mut lines, text.ends_with('\n'));
    let shift = line_start(&lines, at) as isize - line_start_in(text, first) as isize;
    let shift_pos = |x: usize| (x as isize + shift).max(0) as usize;
    Edit { text: join(&lines), sel: (shift_pos(a), shift_pos(b)) }
}

/// Indice (caractères) du début de la ligne `n` dans `text`.
fn line_start_in(text: &str, n: usize) -> usize {
    lines_with_offsets(text).get(n).map_or(0, |(s, _)| *s)
}

/// Rétablit un `\n` à la fin de chaque ligne. La dernière n'en porte un que si
/// le fichier se terminait déjà par un saut de ligne — `trailing` le dit.
///
/// Sans cela, déplacer ou dupliquer la dernière ligne d'un fichier qui n'en
/// finit pas par un `\n` souderait deux lignes, et l'opération inverse
/// retirerait sans le dire le saut de ligne final d'un fichier qui en avait un.
fn fix_line_endings(lines: &mut [String], trailing: bool) {
    let n = lines.len();
    for (i, l) in lines.iter_mut().enumerate() {
        let last = i + 1 == n;
        let want = !last || trailing;
        let has = l.ends_with('\n');
        if want && !has {
            l.push('\n');
        } else if !want && has {
            l.pop();
        }
    }
}

/// Duplique les lignes touchées juste en dessous, la sélection suivant la copie
/// — c'est elle qu'on vient éditer.
pub(super) fn duplicate_lines(text: &str, sel: (usize, usize)) -> Edit {
    let mut lines = owned_lines(text);
    let (first, last) = touched_lines(text, sel.0, sel.1);
    let block: Vec<String> = lines[first..=last].to_vec();
    for (k, l) in block.into_iter().enumerate() {
        lines.insert(last + 1 + k, l);
    }
    fix_line_endings(&mut lines, text.ends_with('\n'));
    // Le curseur suit la copie : il se décale de tout ce qui vient d'être
    // inséré — le bloc, plus le `\n` qu'il a parfois fallu ajouter à ce qui
    // était jusque-là la dernière ligne du fichier.
    let before = text.chars().count();
    let after: usize = lines.iter().map(|l| l.chars().count()).sum();
    let delta = after - before;
    let (a, b) = (sel.0.min(sel.1), sel.0.max(sel.1));
    Edit { text: join(&lines), sel: (a + delta, b + delta) }
}

/// Supprime les lignes touchées. Le curseur se pose au début de la ligne qui a
/// pris leur place (ou de la dernière ligne, si l'on supprimait la fin).
pub(super) fn delete_lines(text: &str, sel: (usize, usize)) -> Edit {
    let mut lines = owned_lines(text);
    let (first, last) = touched_lines(text, sel.0, sel.1);
    lines.drain(first..=last);
    if lines.is_empty() {
        return Edit { text: String::new(), sel: (0, 0) };
    }
    fix_line_endings(&mut lines, text.ends_with('\n'));
    let n = first.min(lines.len() - 1);
    let pos = line_start(&lines, n);
    Edit { text: join(&lines), sel: (pos, pos) }
}

/// L'indentation à réappliquer après un saut de ligne, connaissant la ligne
/// qu'on vient de quitter.
///
/// Reprend son indentation, et en ajoute un cran si elle DÉCLARE un label
/// (`_start:`, `.loop:`) sans code derrière : c'est là que le corps commence.
pub(super) fn indent_after(previous_line: &str) -> String {
    let trimmed = previous_line.trim_end();
    let mut indent = leading_indent(previous_line).to_string();
    let code = trimmed.trim_start();
    let declares_label = code.ends_with(':') && !code.starts_with(';');
    if declares_label {
        indent.push_str(INDENT);
    }
    indent
}

impl App {
    // ---------- Application d'une édition ----------

    /// Applique une édition au source et retient la sélection à replacer au
    /// prochain rendu.
    ///
    /// Le curseur ne peut pas être posé d'ici : il vit dans la mémoire d'egui,
    /// que seul le rendu de l'éditeur a sous la main. C'est `editor_ui` qui
    /// consomme `pending_editor_sel` juste avant de dessiner le champ.
    pub(super) fn apply_edit(&mut self, edit: Edit) {
        if edit.text == self.source {
            // Rien n'a bougé (bord du fichier atteint) : ne pas replacer le
            // curseur évite de casser une sélection que l'utilisateur garde.
            return;
        }
        self.source = edit.text;
        // Le curseur est retenu des DEUX côtés : `pending_editor_sel` pour
        // qu'egui le replace au prochain rendu, `editor_sel` pour que deux
        // gestes qui s'enchaînent (indenter puis commenter) partent tous les
        // deux du bon endroit, sans attendre une image intermédiaire.
        self.pending_editor_sel = Some(edit.sel);
        self.editor_sel = edit.sel;
    }

    /// La sélection courante de l'éditeur (indices de caractères), telle que le
    /// dernier rendu l'a relevée.
    pub(super) fn editor_selection(&self) -> (usize, usize) {
        self.editor_sel
    }

    pub(super) fn editor_indent(&mut self) {
        let sel = self.editor_selection();
        let (a, b) = (sel.0.min(sel.1), sel.0.max(sel.1));
        // Sur une sélection qui tient dans une ligne, Tab reste un Tab : il
        // remplace ce qui est sélectionné par un cran, au lieu de pousser toute
        // la ligne. Le critère est le saut de ligne, pas le numéro de ligne :
        // une ligne entière prise AVEC son `\n` tient dans une seule ligne au
        // sens de `touched_lines`, et il faut pourtant la décaler, pas
        // l'effacer.
        let crosses_lines = self.source.chars().skip(a).take(b - a).any(|c| c == '\n');
        let edit = if crosses_lines {
            indent(&self.source, sel)
        } else {
            insert_indent(&self.source, sel)
        };
        self.apply_edit(edit);
    }

    pub(super) fn editor_outdent(&mut self) {
        let edit = outdent(&self.source, self.editor_selection());
        self.apply_edit(edit);
    }

    pub(super) fn editor_toggle_comment(&mut self) {
        let edit = toggle_comment(&self.source, self.editor_selection());
        self.apply_edit(edit);
    }

    pub(super) fn editor_move_lines(&mut self, down: bool) {
        let edit = move_lines(&self.source, self.editor_selection(), down);
        self.apply_edit(edit);
    }

    pub(super) fn editor_duplicate_lines(&mut self) {
        let edit = duplicate_lines(&self.source, self.editor_selection());
        self.apply_edit(edit);
    }

    pub(super) fn editor_delete_lines(&mut self) {
        let edit = delete_lines(&self.source, self.editor_selection());
        self.apply_edit(edit);
    }

    // ---------- Aller à la ligne ----------

    /// Ouvre la boîte « aller à la ligne », pré-remplie de la ligne courante.
    pub(super) fn open_goto_line(&mut self) {
        self.show_goto_line = true;
        self.goto_line_input = self.editor_ln.to_string();
        self.goto_line_focus = true;
    }

    /// Place le curseur au début de la ligne `line` (1-based) et l'amène à
    /// l'écran. Un numéro hors bornes est ramené dans le fichier plutôt que
    /// rejeté : « ligne 9999 » veut dire « la fin ».
    pub(super) fn goto_line(&mut self, line: usize) {
        let lines = owned_lines(&self.source);
        let n = line.max(1).min(lines.len()) - 1;
        let pos = line_start(&lines, n);
        self.pending_editor_sel = Some((pos, pos));
        self.editor_sel = (pos, pos);
        self.pending_scroll_to_line = Some(n);
        self.editor_ln = n + 1;
        self.editor_col = 1;
    }

    /// Fenêtre « aller à la ligne » (Ctrl+G).
    pub(super) fn goto_line_window(&mut self, ctx: &egui::Context) {
        if !self.show_goto_line {
            return;
        }
        let t_title = self.tr3("Aller à la ligne", "Go to line", "Ir a la línea");
        let total = self.source.lines().count().max(1);
        let t_hint = format!("1 – {total}");
        let t_go = self.tr3("Aller", "Go", "Ir");
        let t_cancel = self.tr3("Annuler", "Cancel", "Cancelar");
        let mut open = true;
        let mut go = false;
        egui::Window::new(t_title)
            .id(egui::Id::new("goto_line"))
            .collapsible(false)
            .resizable(false)
            .pivot(egui::Align2::CENTER_TOP)
            .default_pos(ctx.content_rect().center_top() + egui::vec2(0.0, 120.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.goto_line_input)
                            .id(egui::Id::new("kb_goto_line"))
                            .desired_width(90.0)
                            .hint_text(t_hint),
                    );
                    if std::mem::take(&mut self.goto_line_focus) {
                        resp.request_focus();
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        go = true;
                    }
                    if ui.button(t_go).clicked() {
                        go = true;
                    }
                    if ui.button(t_cancel).clicked() {
                        self.show_goto_line = false;
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.show_goto_line = false;
        }
        if go {
            if let Ok(n) = self.goto_line_input.trim().parse::<usize>() {
                self.goto_line(n);
                self.focus_panel(super::dock::Panel::Editor);
                ctx.memory_mut(|m| m.request_focus(super::editor_id()));
            }
            self.show_goto_line = false;
        }
        if !open {
            self.show_goto_line = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sélection vide posée à `pos`.
    fn at(pos: usize) -> (usize, usize) {
        (pos, pos)
    }

    #[test]
    fn touched_lines_stops_before_a_selection_that_ends_at_a_line_start() {
        let src = "un\ndeux\ntrois\n";
        // Sélection de « un\ndeux\n » : elle finit au début de « trois », qui
        // ne doit pas être du voyage.
        let end = "un\ndeux\n".chars().count();
        assert_eq!(touched_lines(src, 0, end), (0, 1));
        // Un seul caractère de plus, et « trois » en fait partie.
        assert_eq!(touched_lines(src, 0, end + 1), (0, 2));
    }

    #[test]
    fn indent_adds_one_step_to_every_touched_line_but_not_to_empty_ones() {
        let src = "mov rax, 1\n\nmov rbx, 2\n";
        let e = indent(src, (0, src.chars().count()));
        assert_eq!(e.text, "    mov rax, 1\n\n    mov rbx, 2\n", "la ligne vide reste vide");
    }

    #[test]
    fn outdent_undoes_indent_and_stops_at_column_zero() {
        let src = "    mov rax, 1\n  mov rbx, 2\nmov rcx, 3\n";
        let e = outdent(src, (0, src.chars().count()));
        assert_eq!(e.text, "mov rax, 1\nmov rbx, 2\nmov rcx, 3\n");
        // Rien à retirer : le texte ne bouge plus.
        let again = outdent(&e.text, (0, e.text.chars().count()));
        assert_eq!(again.text, e.text);
    }

    #[test]
    fn tab_on_an_empty_selection_inserts_a_step_at_the_cursor() {
        let src = "movrax";
        let e = insert_indent(src, at(3));
        assert_eq!(e.text, "mov    rax");
        assert_eq!(e.sel, at(7), "le curseur suit ce qu'on vient d'insérer");
    }

    #[test]
    fn comment_toggles_on_then_off_and_comes_back_to_the_original() {
        let src = "    mov rax, 1\n    mov rbx, 2\n";
        let all = (0, src.chars().count());
        let on = toggle_comment(src, all);
        assert_eq!(on.text, "    ; mov rax, 1\n    ; mov rbx, 2\n");
        let off = toggle_comment(&on.text, (0, on.text.chars().count()));
        assert_eq!(off.text, src, "décommenter doit rendre le texte de départ");
    }

    /// Le `;` se pose à l'indentation la MOINS profonde du bloc : le code garde
    /// sa forme pendant qu'il est mis de côté.
    #[test]
    fn comment_aligns_on_the_shallowest_line_of_the_block() {
        let src = "  a\n      b\n";
        let e = toggle_comment(src, (0, src.chars().count()));
        assert_eq!(e.text, "  ; a\n  ;     b\n");
    }

    /// Un bloc dont une seule ligne est commentée se commente en entier : c'est
    /// « tout ou rien », comme partout ailleurs.
    #[test]
    fn a_partially_commented_block_gets_commented_entirely() {
        let src = "; a\nb\n";
        let e = toggle_comment(src, (0, src.chars().count()));
        assert_eq!(e.text, "; ; a\n; b\n");
    }

    #[test]
    fn empty_lines_do_not_prevent_a_block_from_being_uncommented() {
        let src = "; a\n\n; b\n";
        let e = toggle_comment(src, (0, src.chars().count()));
        assert_eq!(e.text, "a\n\nb\n", "les lignes vides ne comptent pas");
    }

    #[test]
    fn move_lines_swaps_with_the_neighbour_in_both_directions() {
        let src = "un\ndeux\ntrois\n";
        let down = move_lines(src, at(0), true);
        assert_eq!(down.text, "deux\nun\ntrois\n");
        let up = move_lines(&down.text, at(down.sel.0), false);
        assert_eq!(up.text, src, "remonter doit rendre l'ordre de départ");
    }

    /// Descendre la ligne du curseur doit emmener le curseur avec elle, sinon
    /// une seconde pression déplacerait une AUTRE ligne.
    #[test]
    fn the_cursor_follows_the_line_it_moves() {
        let src = "un\ndeux\ntrois\n";
        let first = move_lines(src, at(0), true);
        let second = move_lines(&first.text, at(first.sel.0), true);
        assert_eq!(second.text, "deux\ntrois\nun\n");
    }

    #[test]
    fn move_lines_does_nothing_at_the_edges() {
        let src = "un\ndeux\n";
        assert_eq!(move_lines(src, at(0), false).text, src, "la première ne monte pas");
        let last = src.chars().count() - 1;
        assert_eq!(move_lines(src, at(last), true).text, src, "la dernière ne descend pas");
    }

    /// Le fichier ne finit pas toujours par un saut de ligne : déplacer sa
    /// dernière ligne vers le haut souderait deux lignes si l'on n'y prenait
    /// pas garde.
    #[test]
    fn moving_the_last_line_of_a_file_without_trailing_newline_keeps_the_lines_apart() {
        let src = "un\ndeux";
        let e = move_lines(src, at(src.chars().count()), false);
        assert_eq!(e.text, "deux\nun");
    }

    #[test]
    fn duplicate_puts_the_copy_below_and_the_cursor_on_it() {
        let src = "mov rax, 1\nmov rbx, 2\n";
        let e = duplicate_lines(src, at(0));
        assert_eq!(e.text, "mov rax, 1\nmov rax, 1\nmov rbx, 2\n");
        assert_eq!(e.sel, at("mov rax, 1\n".chars().count()), "le curseur suit la copie");
    }

    #[test]
    fn duplicate_handles_a_file_without_a_trailing_newline() {
        let e = duplicate_lines("mov rax, 1", at(0));
        assert_eq!(e.text, "mov rax, 1\nmov rax, 1");
    }

    #[test]
    fn delete_lines_removes_the_block_and_parks_the_cursor_where_it_was() {
        let src = "un\ndeux\ntrois\n";
        let e = delete_lines(src, at(3)); // dans « deux »
        assert_eq!(e.text, "un\ntrois\n");
        assert_eq!(e.sel, at(3), "le curseur passe au début de « trois »");
    }

    #[test]
    fn deleting_the_last_line_parks_the_cursor_inside_what_remains() {
        let src = "un\ndeux";
        let e = delete_lines(src, at(src.chars().count()));
        assert_eq!(e.text, "un");
        assert!(e.sel.0 <= e.text.chars().count(), "curseur hors du texte : {:?}", e.sel);
    }

    #[test]
    fn deleting_everything_is_safe() {
        let e = delete_lines("une seule ligne", (0, 15));
        assert_eq!(e.text, "");
        assert_eq!(e.sel, at(0));
    }

    #[test]
    fn auto_indent_repeats_the_indentation_and_opens_a_body_after_a_label() {
        assert_eq!(indent_after("    mov rax, 1"), "    ");
        assert_eq!(indent_after("_start:"), INDENT, "le corps du label s'indente d'un cran");
        assert_eq!(indent_after("  .loop:"), format!("  {INDENT}"));
        assert_eq!(indent_after("mov rax, 1"), "");
        assert_eq!(indent_after("; _start:"), "", "un commentaire n'ouvre pas de corps");
    }

    /// Les indices sont en caractères : un commentaire accentué ne doit pas
    /// décaler ce qui suit. C'est l'erreur qui ne se voit qu'en français.
    #[test]
    fn accented_text_does_not_shift_the_indices() {
        let src = "; déjà vu\nmov rax, 1\n";
        let e = indent(src, at(src.chars().count() - 2));
        assert_eq!(e.text, "; déjà vu\n    mov rax, 1\n");
    }

    #[test]
    fn goto_line_clamps_instead_of_failing() {
        let mut app = App::new();
        app.source = "un\ndeux\ntrois\n".into();
        app.goto_line(2);
        assert_eq!(app.pending_editor_sel, Some(at(3)));
        assert_eq!(app.editor_ln, 2);

        app.goto_line(9999);
        assert_eq!(app.editor_ln, 3, "au-delà de la fin : on va à la dernière ligne");
        app.goto_line(0);
        assert_eq!(app.editor_ln, 1, "au-dessus du début : on va au début");
    }

    /// Tab, selon ce que couvre la sélection. Le cas qui manquait : une
    /// sélection dans une seule ligne poussait la ligne entière au lieu d'être
    /// remplacée par un cran — le commentaire de `editor_indent` disait déjà le
    /// contraire du code.
    #[test]
    fn tab_replaces_a_selection_held_in_one_line_and_shifts_the_rest() {
        // Curseur seul : un cran s'insère là où l'on est.
        let mut app = App::new();
        app.source = "movrax".into();
        app.editor_sel = at(3);
        app.editor_indent();
        assert_eq!(app.source, "mov    rax");

        // « rax » sélectionné : il cède la place au cran.
        let mut app = App::new();
        app.source = "mov rax, 1\nmov rbx, 2\n".into();
        app.editor_sel = (4, 7);
        app.editor_indent();
        assert_eq!(app.source, "mov     , 1\nmov rbx, 2\n");
        assert_eq!(app.pending_editor_sel, Some(at(8)), "le curseur suit le cran");

        // Une ligne entière prise avec son saut de ligne se DÉCALE : la
        // remplacer l'effacerait.
        let mut app = App::new();
        app.source = "mov rax, 1\nmov rbx, 2\n".into();
        app.editor_sel = (0, 11);
        app.editor_indent();
        assert_eq!(app.source, "    mov rax, 1\nmov rbx, 2\n");

        // Sélection à cheval sur deux lignes : les deux se décalent.
        let mut app = App::new();
        app.source = "mov rax, 1\nmov rbx, 2\n".into();
        app.editor_sel = (0, 14);
        app.editor_indent();
        assert_eq!(app.source, "    mov rax, 1\n    mov rbx, 2\n");
    }

    /// Une opération qui ne change rien ne doit pas replacer le curseur : cela
    /// écraserait la sélection que l'utilisateur vient de faire.
    #[test]
    fn an_edit_that_changes_nothing_leaves_the_cursor_alone() {
        let mut app = App::new();
        app.source = "un\ndeux\n".into();
        app.pending_editor_sel = None;
        app.editor_sel = at(0);
        app.editor_move_lines(false); // première ligne : rien à faire
        assert_eq!(app.pending_editor_sel, None);
    }
}
