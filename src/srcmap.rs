//! Mapping adresse virtuelle → ligne source, à partir du listing NASM (`.lst`).
//!
//! Le listing donne, pour chaque ligne source émettant du code, un offset
//! relatif à la section. On le convertit en adresse virtuelle avec la base de
//! `.text`, afin de surligner dans l'éditeur la ligne correspondant à RIP.

use std::collections::HashMap;
use std::path::Path;

/// Construit `adresse virtuelle -> numéro de ligne (1-based)` pour `.text`.
pub fn parse(listing: &Path, text_base: u64) -> HashMap<u64, usize> {
    let mut map = HashMap::new();
    let Ok(content) = std::fs::read_to_string(listing) else {
        return map;
    };

    // On ne mappe que la section .text (seule exécutée). On suit la section
    // courante d'après les directives `section`/`segment` rencontrées.
    let mut in_text = false;
    for raw in content.lines() {
        let low = raw.to_ascii_lowercase();
        if low.contains("section") || low.contains("segment") {
            if low.contains(".text") {
                in_text = true;
            } else if low.contains(".data") || low.contains(".bss") || low.contains(".rodata") {
                in_text = false;
            }
        }

        let trimmed = raw.trim_start();
        let digits_end = trimmed.find(|c: char| !c.is_ascii_digit()).unwrap_or(0);
        if digits_end == 0 {
            continue; // pas de numéro de ligne
        }
        let Ok(line_no) = trimmed[..digits_end].parse::<usize>() else {
            continue;
        };
        let rest = trimmed[digits_end..].trim_start();
        let first = rest.split_whitespace().next().unwrap_or("");
        // Une ligne de code commence par un offset de 8 chiffres hexadécimaux.
        if in_text && first.len() == 8 && first.bytes().all(|b| b.is_ascii_hexdigit())
            && let Ok(off) = u64::from_str_radix(first, 16) {
                map.entry(text_base.wrapping_add(off)).or_insert(line_no);
            }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{assemble, disasm};
    use std::path::Path as P;

    #[test]
    fn maps_entry_address_to_start_label_line() {
        let out = assemble::assemble_with_includes(
            P::new("examples/test.asm"),
            P::new("build/test-srcmap"),
            &[],
        )
        .expect("assemblage");
        let base = disasm::section_address(&out.binary, ".text").expect(".text");
        let map = parse(&out.listing, base);
        // La 1re instruction (mov rax,5) est à la base de .text, ligne 11 du source.
        assert_eq!(map.get(&base).copied(), Some(11));
    }
}
