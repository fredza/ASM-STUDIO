//! Coloration syntaxique NASM pour l'éditeur (style VSCode).
//!
//! Un balayage par ligne, en un seul passage, gère : commentaires (`;` — même
//! à l'intérieur d'une chaîne il n'est pas confondu), chaînes `"…"`/`'…'`,
//! labels (`foo:`) et labels locaux/sections (`.text`), directives (`section`,
//! `global`, `db`…), registres, nombres, et le mnémonique en tête de ligne.

use eframe::egui::{Color32, FontId, TextFormat, text::LayoutJob};

// Palette (tons VSCode « Dark+ »).
const COMMENT: Color32 = Color32::from_rgb(0x6A, 0x99, 0x55);
const MNEMONIC: Color32 = Color32::from_rgb(0x56, 0x9C, 0xD6);
const REGISTER: Color32 = Color32::from_rgb(0x9C, 0xDC, 0xFE);
const NUMBER: Color32 = Color32::from_rgb(0xB5, 0xCE, 0xA8);
const DIRECTIVE: Color32 = Color32::from_rgb(0xC5, 0x86, 0xC0);
const LABEL: Color32 = Color32::from_rgb(0xDC, 0xDC, 0xAA);
const STRING: Color32 = Color32::from_rgb(0xCE, 0x91, 0x78);
const TEXT: Color32 = Color32::from_rgb(0xD4, 0xD4, 0xD4);

/// Taille de police de l'éditeur (partagée avec la gouttière de numéros).
pub const FONT_SIZE: f32 = 13.0;
/// Fond de la ligne courante (RIP) pendant le débogage.
const CURRENT_LINE_BG: Color32 = Color32::from_rgb(0x3A, 0x33, 0x1E);

/// Construit le `LayoutJob` coloré du source complet. `hl_line` (0-based) est la
/// ligne à surligner (ligne courante du débogage), ou `None`.
/// Le retour à la ligne est désactivé pour rester aligné aux numéros de ligne.
pub fn highlight(text: &str, hl_line: Option<usize>) -> LayoutJob {
    let font = FontId::monospace(FONT_SIZE);
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    for (i, line) in text.split_inclusive('\n').enumerate() {
        let bg = if Some(i) == hl_line {
            CURRENT_LINE_BG
        } else {
            Color32::TRANSPARENT
        };
        highlight_line(&mut job, line, &font, bg);
    }
    job
}

fn highlight_line(job: &mut LayoutJob, line: &str, font: &FontId, bg: Color32) {
    let mut rest = line;
    let mut mnem_pending = true;
    while !rest.is_empty() {
        let c = rest.chars().next().unwrap();
        if c == ';' {
            // Commentaire jusqu'à la fin de la ligne.
            push(job, rest, COMMENT, font, bg);
            break;
        } else if c == '"' || c == '\'' {
            let end = string_end(rest, c);
            push(job, &rest[..end], STRING, font, bg);
            rest = &rest[end..];
        } else if is_ident(c) {
            let end = rest.find(|ch: char| !is_ident(ch)).unwrap_or(rest.len());
            let word = &rest[..end];
            let after = &rest[end..];
            push(job, word, classify(word, after, &mut mnem_pending), font, bg);
            rest = after;
        } else {
            // Suite de séparateurs (espaces, virgules, crochets, opérateurs).
            let end = rest
                .find(|ch: char| ch == ';' || ch == '"' || ch == '\'' || is_ident(ch))
                .unwrap_or(rest.len())
                .max(c.len_utf8());
            push(job, &rest[..end], TEXT, font, bg);
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

fn classify(word: &str, after: &str, mnem_pending: &mut bool) -> Color32 {
    if word.starts_with('.') {
        // Label local (.loop) ou nom de section (.text/.data/.bss).
        LABEL
    } else if is_number(word) {
        NUMBER
    } else if is_register(word) {
        REGISTER
    } else if is_directive(word) {
        *mnem_pending = false;
        DIRECTIVE
    } else if after.trim_start().starts_with(':') {
        LABEL
    } else if *mnem_pending {
        *mnem_pending = false;
        MNEMONIC
    } else {
        TEXT
    }
}

fn push(job: &mut LayoutJob, text: &str, color: Color32, font: &FontId, bg: Color32) {
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font.clone(),
            color,
            background: bg,
            ..Default::default()
        },
    );
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

    #[test]
    fn covers_whole_line_without_loss() {
        // Chaque caractère doit être stylé (aucune perte de texte à l'affichage).
        let src = "  mov rax, 5   ; commentaire\n";
        let job = highlight(src, None);
        assert_eq!(job.text, src);
    }

    #[test]
    fn semicolon_inside_string_is_not_a_comment() {
        let src = "    db \"a;b\", 10\n";
        let job = highlight(src, None);
        // Tout le texte est présent et la ligne reste intègre.
        assert_eq!(job.text, src);
        // La chaîne "a;b" est colorée en STRING (une section contient a;b).
        assert!(
            job.sections
                .iter()
                .any(|s| s.format.color == STRING && src[s.byte_range.clone()].contains(';')),
            "le ; dans la chaîne ne doit pas déclencher un commentaire"
        );
    }
}
