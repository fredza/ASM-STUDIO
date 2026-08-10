//! Coloration syntaxique NASM pour l'éditeur.
//!
//! Un balayage par ligne, en un seul passage, gère : commentaires (`;` — même
//! à l'intérieur d'une chaîne il n'est pas confondu), chaînes `"…"`/`'…'`,
//! labels (`foo:`) et labels locaux/sections (`.text`), directives (`section`,
//! `global`, `db`…), registres, nombres, et le mnémonique en tête de ligne.
//!
//! Les couleurs viennent du thème courant (voir [`crate::theme`]) : ce module
//! ne connaît que les CATÉGORIES lexicales, jamais les teintes. Ajouter un
//! thème ne s'y voit pas.
//!
//! Le surlignage des résultats de recherche (`FindHighlight`), de la ligne du
//! curseur et de la parenthèse/du crochet correspondant (`matching_bracket`)
//! sont des passes indépendantes des couleurs lexicales : elles ne font que
//! teinter le fond de segments déjà classés, sans toucher à leur couleur de
//! premier plan.

use eframe::egui::{Color32, FontId, TextFormat, text::LayoutJob};

use crate::theme::Syntax;

/// Taille de police de l'éditeur (partagée avec la gouttière de numéros).
pub const FONT_SIZE: f32 = 13.0;

/// Recherche active à surligner par-dessus la coloration syntaxique.
pub struct FindHighlight<'a> {
    pub query: &'a str,
    pub case_sensitive: bool,
    /// Offset (en octets, dans le texte complet) du début de la correspondance
    /// active — reçoit un fond plus marqué que les autres.
    pub current: Option<usize>,
}

/// Segment à teinter d'un fond particulier, indépendamment de sa couleur de
/// premier plan (recherche, parenthèse appariée…).
struct HlSpan {
    start: usize,
    end: usize,
    bg: Color32,
}

/// Positions (octets, `[début, fin)`) de chaque occurrence de `query` dans
/// `text`. Sans dépendance : recherche naïve, suffisante pour un fichier
/// source de quelques Ko relu à chaque frappe.
///
/// Insensible à la casse ASCII uniquement (`to_ascii_lowercase`) : préserve
/// la longueur en octets, donc les offsets restent valides. Les caractères
/// accentués (commentaires en français) ne sont donc pas repliés — recherche
/// sensible à la casse pour eux, limitation assumée plutôt qu'un `to_lowercase`
/// Unicode qui pourrait décaler les offsets.
pub fn find_matches(text: &str, query: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    let (hay, needle);
    let (hay, needle): (&str, &str) = if case_sensitive {
        (text, query)
    } else {
        hay = text.to_ascii_lowercase();
        needle = query.to_ascii_lowercase();
        (&hay, &needle)
    };
    let mut out = Vec::new();
    let mut start = 0usize;
    while start <= hay.len() {
        let Some(rel) = hay[start..].find(needle) else { break };
        let s = start + rel;
        let e = s + needle.len();
        out.push((s, e));
        start = e;
    }
    out
}

fn is_open_bracket(c: u8) -> bool {
    matches!(c, b'(' | b'[' | b'{')
}

fn is_close_bracket(c: u8) -> bool {
    matches!(c, b')' | b']' | b'}')
}

fn opposite_bracket(c: u8) -> u8 {
    match c {
        b'(' => b')',
        b')' => b'(',
        b'[' => b']',
        b']' => b'[',
        b'{' => b'}',
        b'}' => b'{',
        _ => c,
    }
}

/// Portions de `text` à ignorer pour l'appariement de parenthèses : commentaires
/// (`;` jusqu'à la fin de ligne) et chaînes `"…"`/`'…'`. Même découpage que le
/// lexer de coloration syntaxique, allégé (pas besoin des catégories de mots).
fn ignored_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let mut rest = line;
        let mut pos = offset;
        while !rest.is_empty() {
            let c = rest.chars().next().unwrap();
            if c == ';' {
                out.push((pos, offset + line.len()));
                break;
            } else if c == '"' || c == '\'' {
                let end = string_end(rest, c);
                out.push((pos, pos + end));
                pos += end;
                rest = &rest[end..];
            } else {
                let n = c.len_utf8();
                pos += n;
                rest = &rest[n..];
            }
        }
        offset += line.len();
    }
    out
}

fn is_ignored(ranges: &[(usize, usize)], pos: usize) -> bool {
    ranges.iter().any(|&(s, e)| pos >= s && pos < e)
}

/// Positions (octets) de la parenthèse/du crochet/de l'accolade qui touche le
/// curseur (juste avant ou juste après lui) et de son partenaire, en ignorant
/// les commentaires et les chaînes. `None` si le curseur ne touche aucun
/// bracket, ou si celui-ci n'a pas de partenaire (non apparié).
pub fn matching_bracket(text: &str, cursor: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let is_bracket_at = |i: usize| bytes.get(i).copied().filter(|&c| is_open_bracket(c) || is_close_bracket(c));
    let pos = if is_bracket_at(cursor).is_some() {
        cursor
    } else {
        let prev = cursor.checked_sub(1)?;
        is_bracket_at(prev)?;
        prev
    };
    let ranges = ignored_ranges(text);
    if is_ignored(&ranges, pos) {
        return None;
    }
    let c = bytes[pos];
    let target = opposite_bracket(c);
    if is_open_bracket(c) {
        let mut depth = 0i32;
        for (i, &b) in bytes.iter().enumerate().skip(pos) {
            if is_ignored(&ranges, i) {
                continue;
            }
            if b == c {
                depth += 1;
            } else if b == target {
                depth -= 1;
                if depth == 0 {
                    return Some((pos, i));
                }
            }
        }
        None
    } else {
        let mut depth = 0i32;
        for i in (0..=pos).rev() {
            if is_ignored(&ranges, i) {
                continue;
            }
            if bytes[i] == c {
                depth += 1;
            } else if bytes[i] == target {
                depth -= 1;
                if depth == 0 {
                    return Some((i, pos));
                }
            }
        }
        None
    }
}

/// Construit le `LayoutJob` coloré du source complet. `pal` est la palette du
/// thème courant, `hl_line` (0-based) la ligne à surligner (ligne courante du
/// débogage), `cursor_line` celle où se trouve le curseur d'édition, `find`
/// surligne en plus les correspondances d'une recherche active, et
/// `bracket_match` la paire de brackets sous le curseur (voir [`matching_bracket`]).
/// Le retour à la ligne est désactivé pour rester aligné aux numéros de ligne.
pub fn highlight(
    text: &str,
    pal: &Syntax,
    hl_line: Option<usize>,
    cursor_line: Option<usize>,
    find: Option<&FindHighlight>,
    bracket_match: Option<(usize, usize)>,
) -> LayoutJob {
    let font = FontId::monospace(FONT_SIZE);

    let mut spans = Vec::new();
    if let Some(f) = find {
        for (s, e) in find_matches(text, f.query, f.case_sensitive) {
            let bg = if f.current == Some(s) { pal.match_current_bg } else { pal.match_bg };
            spans.push(HlSpan { start: s, end: e, bg });
        }
    }
    if let Some((a, b)) = bracket_match {
        let len = |i: usize| text[i..].chars().next().map(char::len_utf8).unwrap_or(1);
        spans.push(HlSpan { start: a, end: a + len(a), bg: pal.bracket_bg });
        spans.push(HlSpan { start: b, end: b + len(b), bg: pal.bracket_bg });
    }

    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    let mut offset = 0usize;
    for (i, line) in text.split_inclusive('\n').enumerate() {
        // La ligne RIP prime sur la ligne du curseur : pendant un pas à pas,
        // c'est elle qu'on cherche des yeux.
        let bg = if Some(i) == hl_line {
            pal.line_bg
        } else if Some(i) == cursor_line {
            pal.cursor_line_bg
        } else {
            Color32::TRANSPARENT
        };
        highlight_line(&mut job, pal, line, &font, bg, offset, &spans);
        offset += line.len();
    }
    job
}

fn highlight_line(
    job: &mut LayoutJob,
    pal: &Syntax,
    line: &str,
    font: &FontId,
    bg: Color32,
    line_offset: usize,
    spans: &[HlSpan],
) {
    let mut rest = line;
    let mut pos = line_offset;
    let mut mnem_pending = true;
    while !rest.is_empty() {
        let c = rest.chars().next().unwrap();
        if c == ';' {
            // Commentaire jusqu'à la fin de la ligne.
            push(job, pal.comment, rest, font, bg, pos, spans);
            break;
        } else if c == '"' || c == '\'' {
            let end = string_end(rest, c);
            push(job, pal.string, &rest[..end], font, bg, pos, spans);
            pos += end;
            rest = &rest[end..];
        } else if is_ident(c) {
            let end = rest.find(|ch: char| !is_ident(ch)).unwrap_or(rest.len());
            let word = &rest[..end];
            let after = &rest[end..];
            push(job, classify(word, after, pal, &mut mnem_pending), word, font, bg, pos, spans);
            pos += end;
            rest = after;
        } else {
            // Suite de séparateurs (espaces, virgules, crochets, opérateurs).
            let end = rest
                .find(|ch: char| ch == ';' || ch == '"' || ch == '\'' || is_ident(ch))
                .unwrap_or(rest.len())
                .max(c.len_utf8());
            push(job, pal.text, &rest[..end], font, bg, pos, spans);
            pos += end;
            rest = &rest[end..];
        }
    }
}

/// Indice de fin (exclus) d'une chaîne débutant par `quote`, quote fermante incluse.
fn string_end(s: &str, quote: char) -> usize {
    for (idx, ch) in s.char_indices().skip(1) {
        if ch == quote {
            return idx + ch.len_utf8();
        }
    }
    s.len()
}

fn classify(word: &str, after: &str, pal: &Syntax, mnem_pending: &mut bool) -> Color32 {
    if word.starts_with('.') {
        // Label local (.loop) ou nom de section (.text/.data/.bss).
        pal.label
    } else if is_number(word) {
        pal.number
    } else if is_register(word) {
        pal.register
    } else if is_directive(word) {
        *mnem_pending = false;
        pal.directive
    } else if after.trim_start().starts_with(':') {
        pal.label
    } else if *mnem_pending {
        *mnem_pending = false;
        pal.mnemonic
    } else {
        pal.text
    }
}

/// Pousse `text` (déjà classé, couleur `color`) dans `job`, en découpant aux
/// frontières des `spans` qui le recouvrent pour leur donner un fond distinct
/// — sans jamais toucher à la couleur de premier plan. `base_offset` est la
/// position de `text` dans le texte complet.
fn push(job: &mut LayoutJob, color: Color32, text: &str, font: &FontId, bg: Color32, base_offset: usize, spans: &[HlSpan]) {
    if spans.is_empty() {
        job.append(text, 0.0, TextFormat { font_id: font.clone(), color, background: bg, ..Default::default() });
        return;
    }
    let seg_start = base_offset;
    let seg_end = base_offset + text.len();
    let mut cuts = vec![seg_start, seg_end];
    for s in spans {
        if s.start < seg_end && s.end > seg_start {
            cuts.push(s.start.max(seg_start));
            cuts.push(s.end.min(seg_end));
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a >= b {
            continue;
        }
        let piece = &text[a - seg_start..b - seg_start];
        // Spans non chevauchants (recherche : entre eux : ; bracket : deux
        // segments d'un octet, ne peuvent chevaucher partiellement un span de
        // recherche) : un morceau issu des coupes tombe entièrement dans un
        // span ou entièrement hors de tous — jamais à cheval. En cas d'égalité
        // improbable, le dernier ajouté (bracket, après recherche) l'emporte.
        let piece_bg = spans.iter().rev().find(|s| s.start == a && s.end == b).map_or(bg, |s| s.bg);
        job.append(piece, 0.0, TextFormat { font_id: font.clone(), color, background: piece_bg, ..Default::default() });
    }
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '@' | '$')
}

fn is_number(w: &str) -> bool {
    w.chars().next().is_some_and(|c| c.is_ascii_digit())
}

fn is_register(w: &str) -> bool {
    const REGS: &[&str] = &[
        "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rbp", "rsp", "rip", "r8", "r9", "r10", "r11",
        "r12", "r13", "r14", "r15", "eax", "ebx", "ecx", "edx", "esi", "edi", "ebp", "esp", "r8d",
        "r9d", "r10d", "r11d", "r12d", "r13d", "r14d", "r15d", "r8w", "r9w", "r10w", "r11w",
        "r12w", "r13w", "r14w", "r15w", "ax", "bx", "cx", "dx", "si", "di", "bp", "sp", "al", "bl",
        "cl", "dl", "ah", "bh", "ch", "dh", "sil", "dil", "bpl", "spl", "r8b", "r9b", "r10b",
        "r11b", "r12b", "r13b", "r14b", "r15b",
    ];
    REGS.contains(&w.to_ascii_lowercase().as_str())
}

fn is_directive(w: &str) -> bool {
    const DIRS: &[&str] = &[
        "section", "segment", "global", "extern", "db", "dw", "dd", "dq", "dt", "resb", "resw",
        "resd", "resq", "equ", "times", "align", "default", "bits", "byte", "word", "dword",
        "qword", "incbin", "org",
    ];
    DIRS.contains(&w.to_ascii_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Palette de référence des tests : le thème sombre intégré. Les couleurs
    /// exactes n'ont pas d'importance ici, seule compte la catégorie à laquelle
    /// chaque morceau de texte a été rattaché.
    fn pal() -> &'static Syntax {
        &crate::theme::by_id("dark").expect("le thème sombre fait partie du catalogue").syntax
    }

    #[test]
    fn covers_whole_line_without_loss() {
        // Chaque caractère doit être stylé (aucune perte de texte à l'affichage).
        let src = "  mov rax, 5   ; commentaire\n";
        let job = highlight(src, pal(), None, None, None, None);
        assert_eq!(job.text, src);
    }

    #[test]
    fn semicolon_inside_string_is_not_a_comment() {
        let src = "    db \"a;b\", 10\n";
        let job = highlight(src, pal(), None, None, None, None);
        // Tout le texte est présent et la ligne reste intègre.
        assert_eq!(job.text, src);
        // La chaîne "a;b" est colorée en STRING (une section contient a;b).
        assert!(
            job.sections
                .iter()
                .any(|s| s.format.color == pal().string && src[s.byte_range.clone()].contains(';')),
            "le ; dans la chaîne ne doit pas déclencher un commentaire"
        );
    }

    #[test]
    fn find_matches_locates_every_occurrence_case_insensitively() {
        let text = "mov rax, rax\nadd RAX, 1\n";
        let m = find_matches(text, "rax", false);
        assert_eq!(m.len(), 3, "{m:?}");
        for &(s, e) in &m {
            assert_eq!(text[s..e].to_ascii_lowercase(), "rax");
        }
    }

    #[test]
    fn find_matches_respects_case_sensitivity() {
        let text = "mov rax, RAX\n";
        assert_eq!(find_matches(text, "RAX", true).len(), 1);
        assert_eq!(find_matches(text, "RAX", false).len(), 2);
    }

    #[test]
    fn find_matches_empty_query_yields_nothing() {
        assert!(find_matches("mov rax, 1\n", "", false).is_empty());
    }

    #[test]
    fn highlighting_matches_does_not_lose_or_reorder_text() {
        let src = "    mov rax, rbx ; move rax into itself\n";
        let find = FindHighlight { query: "rax", case_sensitive: false, current: None };
        let job = highlight(src, pal(), None, None, Some(&find), None);
        assert_eq!(job.text, src, "le surlignage ne doit jamais altérer le texte affiché");
        // Les deux occurrences de "rax" doivent porter le fond de correspondance.
        let hits: Vec<_> = job
            .sections
            .iter()
            .filter(|s| s.format.background == pal().match_bg)
            .collect();
        assert_eq!(hits.len(), 2, "{hits:?}");
    }

    #[test]
    fn the_current_match_gets_a_distinct_background() {
        let src = "rax rax\n";
        let matches = find_matches(src, "rax", false);
        let find = FindHighlight { query: "rax", case_sensitive: false, current: Some(matches[1].0) };
        let job = highlight(src, pal(), None, None, Some(&find), None);
        let current_hits = job.sections.iter().filter(|s| s.format.background == pal().match_current_bg).count();
        let other_hits = job.sections.iter().filter(|s| s.format.background == pal().match_bg).count();
        assert_eq!(current_hits, 1, "une seule correspondance active");
        assert_eq!(other_hits, 1, "l'autre garde le fond neutre");
    }

    #[test]
    fn matching_bracket_finds_the_simple_pair() {
        let src = "mov rax, [rbx]\n";
        let open = src.find('[').unwrap();
        let close = src.find(']').unwrap();
        assert_eq!(matching_bracket(src, open), Some((open, close)));
        assert_eq!(matching_bracket(src, close), Some((open, close)), "depuis le fermant aussi");
        // Juste APRÈS le crochet (curseur egui typique après un clic) doit
        // aussi le trouver, via le caractère juste avant.
        assert_eq!(matching_bracket(src, open + 1), Some((open, close)));
    }

    #[test]
    fn matching_bracket_respects_nesting() {
        let src = "mov rax, [rbx + [rcx]]\n";
        let outer_open = src.find('[').unwrap();
        let outer_close = src.rfind(']').unwrap();
        assert_eq!(matching_bracket(src, outer_open), Some((outer_open, outer_close)));
    }

    #[test]
    fn matching_bracket_ignores_brackets_in_comments_and_strings() {
        let src = "mov rax, 1 ; unbalanced ( on purpose\n";
        let paren = src.find('(').unwrap();
        assert_eq!(matching_bracket(src, paren), None, "un bracket en commentaire n'a pas de partenaire");

        let src2 = "db \"(\", 10\n";
        let paren2 = src2.find('(').unwrap();
        assert_eq!(matching_bracket(src2, paren2), None, "idem dans une chaîne");
    }

    #[test]
    fn matching_bracket_none_when_cursor_touches_nothing() {
        assert_eq!(matching_bracket("mov rax, 1\n", 4), None);
    }

    #[test]
    fn matching_bracket_none_when_unbalanced() {
        assert_eq!(matching_bracket("mov rax, [rbx\n", 9), None);
    }
}
