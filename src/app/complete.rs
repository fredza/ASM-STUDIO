//! Autocomplétion de l'éditeur : mnémoniques, registres, directives, symboles
//! du fichier, et fonctions de la bibliothèque `asmstd`.
//!
//! Le public de cet IDE apprend l'assembleur. Il ne connaît pas encore la
//! différence entre `movzx` et `movsx`, ni quels registres existent en 8 bits,
//! ni l'ordre des arguments d'`asm.write`. Une liste qui s'ouvre sous les
//! doigts, avec à droite la CATÉGORIE de chaque entrée — ou, pour une fonction
//! d'`asmstd`, sa signature —, remplace un aller-retour vers la documentation à
//! chaque ligne. C'est là que l'autocomplétion vaut le plus, bien plus que dans
//! un éditeur pour quelqu'un qui sait déjà.
//!
//! Ce qui est proposé dépend de la position dans la ligne : en tête on écrit un
//! mnémonique ou une directive, après on écrit un opérande — registre, symbole,
//! appel. Proposer `rax` en début de ligne, ou `syscall` en opérande, ne ferait
//! que rallonger la liste.
//!
//! Les noms déclarés dans le fichier sont relevés sous **toutes** leurs formes
//! NASM (voir [`symbols`]) : `nom:`, mais aussi `msg db …`, `len equ …`,
//! `%define`, `extern`. N'en connaître qu'une revient à ignorer précisément les
//! noms qu'on tape le plus.
//!
//! Comme [`super::edit_ops`], la partie qui décide vit dans des fonctions pures
//! (`word_at`, `symbols`, `candidates`, `accept`) ; seul l'affichage connaît egui.

use std::sync::LazyLock;

use eframe::egui::{self, RichText};

use super::{App, accent};
use super::edit_ops::Edit;
use crate::i18n::{self, Lang};

/// Nature d'une proposition — donne son libellé de droite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Mnemonic,
    Register,
    Directive,
    /// Un symbole défini dans le fichier ouvert, avec sa nature et sa ligne
    /// (1-based).
    Symbol(SymbolKind, usize),
    /// Une fonction de la bibliothèque `asmstd`, avec sa signature.
    Asmstd(&'static str),
}

/// Ce qu'un nom déclaré dans le source désigne. NASM les écrit de trois façons
/// différentes, et l'élève ne peut pas les distinguer sans relire son fichier —
/// c'est justement ce que la liste lui épargne.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SymbolKind {
    /// `_start:`, `.boucle:` — une adresse dans le code.
    Label,
    /// `msg db "Bonjour"`, `buf resb 64` — une zone de données.
    Data,
    /// `len equ $ - msg`, `%define TAILLE 64` — une constante.
    Constant,
    /// `extern printf` — un symbole défini ailleurs.
    External,
}

/// Une proposition affichée dans la liste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Candidate {
    pub(super) text: String,
    pub(super) kind: Kind,
}

impl Candidate {
    /// Ce qui s'affiche à droite : la catégorie, en clair et traduit.
    pub(super) fn hint(&self, lang: Lang) -> String {
        let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
        match self.kind {
            // La catégorie vient de la base pédagogique : « Arithmétique »,
            // « Saut conditionnel »… Elle est déjà traduite, et reste juste si
            // l'explication de l'instruction évolue.
            Kind::Mnemonic => crate::explain::explain(
                &self.text,
                "",
                crate::debugger::Flags::default(),
                lang,
            )
            .category
            .to_string(),
            Kind::Register => {
                let bits = register_bits(&self.text);
                format!("{} {bits} {}", t("registre", "register", "registro"), t("bits", "bits", "bits"))
            }
            Kind::Directive => t("directive", "directive", "directiva").to_string(),
            Kind::Symbol(what, line) => {
                let what = match what {
                    SymbolKind::Label => t("label", "label", "etiqueta"),
                    SymbolKind::Data => t("donnée", "data", "dato"),
                    SymbolKind::Constant => t("constante", "constant", "constante"),
                    SymbolKind::External => t("externe", "external", "externo"),
                };
                format!("{what} — {} {line}", t("ligne", "line", "línea"))
            }
            // La signature relevée dans asmstd.inc : « (rdi=fd, rsi=buf) -> rax ».
            // C'est exactement ce qu'on va chercher dans le fichier quand on ne
            // se souvient plus de l'ordre des arguments.
            Kind::Asmstd(signature) => signature.to_string(),
        }
    }
}

/// Largeur d'un registre x86-64, d'après son nom.
fn register_bits(name: &str) -> u32 {
    let n = name.to_ascii_lowercase();
    if n.starts_with('r') && !n.ends_with('d') && !n.ends_with('w') && !n.ends_with('b') {
        64
    } else if n.starts_with('e') || n.ends_with('d') {
        32
    } else if n.ends_with('w') || matches!(n.as_str(), "ax" | "bx" | "cx" | "dx" | "si" | "di" | "bp" | "sp") {
        16
    } else {
        8
    }
}

/// Mnémoniques proposés. Le sous-ensemble qu'on rencontre en apprenant : les
/// extensions vectorielles n'y sont pas, elles noieraient la liste sans jamais
/// servir dans un cours d'introduction.
const MNEMONICS: &[&str] = &[
    // Transfert
    "mov", "movzx", "movsx", "movsxd", "lea", "xchg", "push", "pop", "cmovz", "cmovnz", "cmove",
    "cmovne", "cmovl", "cmovg", "cmovle", "cmovge",
    // Arithmétique
    "add", "sub", "inc", "dec", "neg", "mul", "imul", "div", "idiv", "adc", "sbb",
    "cqo", "cdq", "cwd",
    // Logique et bits
    "and", "or", "xor", "not", "test", "shl", "shr", "sal", "sar", "rol", "ror", "bt", "bts",
    "btr", "bsf", "bsr", "popcnt",
    // Comparaison et sauts
    "cmp", "jmp", "je", "jne", "jz", "jnz", "jl", "jle", "jg", "jge", "jb", "jbe", "ja", "jae",
    "js", "jns", "jo", "jno", "jc", "jnc", "jecxz", "jrcxz", "loop",
    // Positionnement conditionnel
    "sete", "setne", "setl", "setle", "setg", "setge", "setb", "seta", "setz", "setnz",
    // Appels et pile
    "call", "ret", "leave", "enter", "syscall", "int",
    // Chaînes
    "movsb", "movsq", "stosb", "stosq", "lodsb", "scasb", "cmpsb", "rep", "repe", "repne",
    // Divers
    "nop", "hlt", "cld", "std", "clc", "stc", "cmc", "pushfq", "popfq",
];

/// Registres proposés — la même liste que celle qui les colore, pour qu'un nom
/// accepté ici soit bien reconnu par la coloration syntaxique.
const REGISTERS: &[&str] = &[
    "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rbp", "rsp", "r8", "r9", "r10", "r11", "r12",
    "r13", "r14", "r15", "eax", "ebx", "ecx", "edx", "esi", "edi", "ebp", "esp", "r8d", "r9d",
    "r10d", "r11d", "r12d", "r13d", "r14d", "r15d", "ax", "bx", "cx", "dx", "si", "di", "bp",
    "sp", "r8w", "r9w", "r10w", "r11w", "r12w", "r13w", "r14w", "r15w", "al", "bl", "cl", "dl",
    "ah", "bh", "ch", "dh", "sil", "dil", "bpl", "spl", "r8b", "r9b", "r10b", "r11b", "r12b",
    "r13b", "r14b", "r15b",
];

const DIRECTIVES: &[&str] = &[
    "section", "global", "extern", "db", "dw", "dd", "dq", "resb", "resw", "resd", "resq",
    "equ", "times", "align", "byte", "word", "dword", "qword", "default", "bits", "org",
    "%include", "%define", "%macro", "%endmacro", "%ifdef", "%endif",
];

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '@' | '$' | '%')
}

/// Le mot en cours de frappe juste avant `cursor` : son début (indice de
/// caractère) et son texte. `None` si le curseur ne touche pas un mot.
pub(super) fn word_at(text: &str, cursor: usize) -> Option<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let cursor = cursor.min(chars.len());
    let mut start = cursor;
    while start > 0 && is_word_char(chars[start - 1]) {
        start -= 1;
    }
    if start == cursor {
        return None;
    }
    Some((start, chars[start..cursor].iter().collect()))
}

/// La ligne où se trouve `cursor` ne contient-elle, avant lui, que des espaces ?
/// Autrement dit : est-on en train d'écrire le PREMIER mot de la ligne ?
fn at_line_start(text: &str, word_start: usize) -> bool {
    text.chars()
        .take(word_start)
        .collect::<String>()
        .rsplit('\n')
        .next()
        .is_some_and(|before| before.trim().is_empty())
}

/// Mots-clés qui, en deuxième position, font du premier mot de la ligne une
/// DÉCLARATION. `db` et consorts réservent des données, `equ` définit une
/// constante — dans les deux cas sans les deux-points d'un label.
const DATA_KEYWORDS: &[&str] = &[
    "db", "dw", "dd", "dq", "dt", "ddq", "do", "resb", "resw", "resd", "resq", "rest", "incbin",
];

/// Symboles déclarés dans le source, avec leur nature et leur ligne (1-based).
///
/// NASM en connaît plusieurs orthographes, et n'en imposer qu'une était le
/// défaut de la première version : seuls les `nom:` étaient vus. Une ligne
/// aussi banale que `len equ $ - msg` — celle que produit tout programme qui
/// écrit une chaîne — passait donc à travers, et `len` n'était jamais proposé.
fn symbols(source: &str) -> Vec<(String, SymbolKind, usize)> {
    let mut out = Vec::new();
    for (i, raw) in source.lines().enumerate() {
        let line = i + 1;
        let code = raw.split(';').next().unwrap_or("");
        let trimmed = code.trim_start();

        // `%define NOM …` / `%assign NOM …` : la constante du préprocesseur.
        if let Some(rest) = trimmed
            .strip_prefix("%define")
            .or_else(|| trimmed.strip_prefix("%assign"))
            && let Some(name) = first_word(rest)
        {
            out.push((name, SymbolKind::Constant, line));
            continue;
        }
        // `extern nom[, nom2]` : des symboles définis ailleurs.
        if let Some(rest) = trimmed.strip_prefix("extern ") {
            out.extend(
                rest.split(',')
                    .filter_map(first_word)
                    .map(|n| (n, SymbolKind::External, line)),
            );
            continue;
        }

        let Some(name) = first_word(trimmed) else { continue };
        // Un nombre en tête de ligne n'est pas un nom ; une instruction non
        // plus. Sans ce filtre, `mov` deviendrait un « label » dès qu'une ligne
        // commencerait par lui — ce qui est le cas de presque toutes.
        if name.starts_with(|c: char| c.is_ascii_digit()) {
            continue;
        }
        let rest = trimmed[name.len()..].trim_start();
        // `nom:` — un label, avec ou sans code derrière lui.
        if rest.starts_with(':') {
            out.push((name, SymbolKind::Label, line));
            continue;
        }
        // `nom db …` / `nom equ …` — la déclaration sans deux-points.
        let Some(keyword) = first_word(rest) else { continue };
        let k = keyword.to_ascii_lowercase();
        if k == "equ" {
            out.push((name, SymbolKind::Constant, line));
        } else if DATA_KEYWORDS.contains(&k.as_str()) {
            out.push((name, SymbolKind::Data, line));
        }
    }
    out
}

/// Le premier mot de `s` (après les espaces), s'il en commence bien un.
fn first_word(s: &str) -> Option<String> {
    let s = s.trim_start();
    let end = s.find(|c: char| !is_word_char(c)).unwrap_or(s.len());
    (end > 0).then(|| s[..end].to_string())
}

/// Fonctions de la bibliothèque `asmstd`, relevées une fois pour toutes dans
/// `asmstd.inc` : `(nom complet, signature)`.
///
/// Le fichier déclare un label parent `asm:` dont chaque fonction est un label
/// local (`.print:`), suivi en commentaire de sa signature — `(rdi=ptr) -> rax`.
/// C'est précisément ce qu'on ne retient pas : une centaine de fonctions, et
/// l'ordre des arguments à chaque appel. La liste le donne sans quitter le code.
static ASMSTD: LazyLock<Vec<(String, &'static str)>> = LazyLock::new(|| {
    const SOURCE: &str = include_str!("../../examples/asmstd.inc");
    SOURCE
        .lines()
        .filter_map(|line| {
            // Les fonctions sont les labels locaux en colonne zéro : une ligne
            // indentée est du corps, pas une déclaration.
            let rest = line.strip_prefix('.')?;
            let end = rest.find(':')?;
            let name = &rest[..end];
            if name.is_empty() || !name.chars().all(is_word_char) {
                return None;
            }
            // Seuls les labels COMMENTÉS sont des fonctions publiques : le
            // fichier en compte une bonne centaine d'autres, internes
            // (`.strlen_lp`, `.itoa_wr`…), qui sont des étiquettes de boucle et
            // qu'on n'appelle jamais de l'extérieur. La signature en commentaire
            // est ce qui distingue les deux — et c'est aussi ce qu'on affiche.
            let signature = rest[end + 1..].trim().trim_start_matches(';').trim();
            (!signature.is_empty()).then(|| (format!("asm.{name}"), signature))
        })
        .collect()
});

/// Les propositions pour le mot `prefix` commencé à `word_start`, dans `source`.
///
/// Vide si le préfixe ne colle à rien — la liste ne s'ouvre pas pour rien.
pub(super) fn candidates(source: &str, word_start: usize, prefix: &str) -> Vec<Candidate> {
    if prefix.is_empty() {
        return Vec::new();
    }
    let p = prefix.to_ascii_lowercase();
    let starts = |s: &str| s.to_ascii_lowercase().starts_with(&p);
    let mut out: Vec<Candidate> = Vec::new();

    // En tête de ligne on écrit une instruction ou une directive ; ailleurs, un
    // opérande. Les symboles du fichier ne servent qu'en opérande : en tête de
    // ligne on en DÉCLARE un nouveau, et proposer ceux qui existent déjà ne
    // ferait qu'encombrer.
    if at_line_start(source, word_start) {
        out.extend(MNEMONICS.iter().filter(|m| starts(m)).map(|m| Candidate {
            text: (*m).to_string(),
            kind: Kind::Mnemonic,
        }));
        out.extend(DIRECTIVES.iter().filter(|d| starts(d)).map(|d| Candidate {
            text: (*d).to_string(),
            kind: Kind::Directive,
        }));
    } else {
        out.extend(REGISTERS.iter().filter(|r| starts(r)).map(|r| Candidate {
            text: (*r).to_string(),
            kind: Kind::Register,
        }));
        // Un même nom peut être déclaré deux fois (redéfinition d'une
        // constante) : on ne le propose qu'une, celle de la première ligne.
        let mut seen: Vec<String> = Vec::new();
        for (name, what, line) in symbols(source) {
            if starts(&name) && !seen.contains(&name) {
                seen.push(name.clone());
                out.push(Candidate { text: name, kind: Kind::Symbol(what, line) });
            }
        }
        // Les fonctions d'asmstd n'existent que si le fichier l'inclut : les
        // proposer sans ça ferait écrire un `call` que nasm refuserait.
        if source.contains("asmstd.inc") {
            out.extend(
                ASMSTD
                    .iter()
                    .filter(|(name, _)| starts(name))
                    .map(|(name, sig)| Candidate { text: name.clone(), kind: Kind::Asmstd(sig) }),
            );
        }
        out.extend(DIRECTIVES.iter().filter(|d| starts(d)).map(|d| Candidate {
            text: (*d).to_string(),
            kind: Kind::Directive,
        }));
    }
    // Un mot déjà écrit en entier et seul de son espèce n'a rien à proposer :
    // la liste ne doit pas rester ouverte sur ce qu'on vient de taper.
    if out.len() == 1 && out[0].text.eq_ignore_ascii_case(prefix) {
        return Vec::new();
    }
    // Le plus court d'abord : « je » avant « jecxz », c'est celui qu'on visait
    // en tapant deux lettres.
    out.sort_by(|a, b| a.text.len().cmp(&b.text.len()).then_with(|| a.text.cmp(&b.text)));
    out.truncate(12);
    out
}

/// Remplace le mot `[start, cursor)` par `chosen`, curseur à sa suite.
pub(super) fn accept(text: &str, start: usize, cursor: usize, chosen: &str) -> Edit {
    let mut out: String = text.chars().take(start).collect();
    out.push_str(chosen);
    out.extend(text.chars().skip(cursor));
    let pos = start + chosen.chars().count();
    Edit { text: out, sel: (pos, pos) }
}

impl App {
    /// Les propositions courantes, d'après le curseur relevé au dernier rendu.
    /// Vide dès que rien ne colle — c'est aussi ce qui referme la liste.
    pub(super) fn completions(&self) -> Vec<Candidate> {
        let (cursor, _) = self.editor_sel;
        let Some((start, prefix)) = word_at(&self.source, cursor) else {
            return Vec::new();
        };
        // Deux lettres avant de proposer quoi que ce soit : dès la première, la
        // liste s'ouvrirait à chaque frappe et couvrirait le code.
        if prefix.chars().count() < 2 || self.complete_dismissed == Some(start) {
            return Vec::new();
        }
        candidates(&self.source, start, &prefix)
    }

    /// Accepte la proposition retenue.
    pub(super) fn accept_completion(&mut self) {
        let list = self.completions();
        let Some(chosen) = list.get(self.complete_sel) else { return };
        let (cursor, _) = self.editor_sel;
        let Some((start, _)) = word_at(&self.source, cursor) else { return };
        let edit = accept(&self.source, start, cursor, &chosen.text.clone());
        self.apply_edit(edit);
        self.complete_sel = 0;
        // Sans cela, le mot complété — qui est lui-même un préfixe valide —
        // rouvrirait aussitôt la liste sur place.
        self.complete_dismissed = Some(start);
    }

    /// Referme la liste jusqu'au prochain mot (Échap).
    pub(super) fn dismiss_completion(&mut self) {
        if let Some((start, _)) = word_at(&self.source, self.editor_sel.0) {
            self.complete_dismissed = Some(start);
        }
        self.complete_sel = 0;
    }

    /// Rouvre la liste sur le mot courant (Ctrl+Espace), même après un Échap.
    pub(super) fn force_completion(&mut self) {
        self.complete_dismissed = None;
        self.complete_sel = 0;
        self.focus_panel(super::dock::Panel::Editor);
    }

    /// Déplace la sélection dans la liste, en boucle.
    pub(super) fn move_completion(&mut self, down: bool) {
        let n = self.completions().len();
        if n == 0 {
            return;
        }
        self.complete_sel = if down {
            (self.complete_sel + 1) % n
        } else {
            (self.complete_sel + n - 1) % n
        };
    }

    /// La liste flottante, ancrée sous le curseur. Renvoie `true` si elle est
    /// affichée — ce qui indique aux raccourcis que ↑↓ et Entrée lui appartiennent.
    pub(super) fn completion_popup(&mut self, ui: &egui::Ui, cursor_screen_pos: egui::Pos2) -> bool {
        let list = self.completions();
        if list.is_empty() {
            self.complete_sel = 0;
            return false;
        }
        if self.complete_sel >= list.len() {
            self.complete_sel = list.len() - 1;
        }
        let lang = self.lang;
        let hdr = self.c_header();
        let sel = self.complete_sel;
        let mut chosen: Option<usize> = None;

        egui::Area::new(egui::Id::new("editor_completion"))
            .order(egui::Order::Foreground)
            .fixed_pos(cursor_screen_pos + egui::vec2(0.0, 4.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(340.0);
                    for (i, c) in list.iter().enumerate() {
                        let is_sel = i == sel;
                        let bg = if is_sel { accent().linear_multiply(0.25) } else { egui::Color32::TRANSPARENT };
                        let r = egui::Frame::default()
                            .fill(bg)
                            .corner_radius(egui::CornerRadius::same(4))
                            .inner_margin(egui::Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let mut txt = RichText::new(&c.text).monospace();
                                    if is_sel {
                                        txt = txt.strong().color(accent());
                                    }
                                    ui.label(txt);
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| ui.label(RichText::new(c.hint(lang)).small().color(hdr)),
                                    );
                                });
                            });
                        let resp = r.response.interact(egui::Sense::click());
                        if resp.clicked() {
                            chosen = Some(i);
                        }
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                    }
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(i18n::tr3(
                            lang,
                            "↑↓ choisir · Tab ou Entrée insérer · Échap fermer",
                            "↑↓ choose · Tab or Enter insert · Esc close",
                            "↑↓ elegir · Tab o Enter insertar · Esc cerrar",
                        ))
                        .small()
                        .weak(),
                    );
                });
            });

        if let Some(i) = chosen {
            self.complete_sel = i;
            self.accept_completion();
        }
        true
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_at_reads_the_identifier_that_ends_at_the_cursor() {
        let src = "    mov rax, 1";
        assert_eq!(word_at(src, 7), Some((4, "mov".to_string())));
        assert_eq!(word_at(src, 6), Some((4, "mo".to_string())));
        assert_eq!(word_at(src, 4), None, "au début du mot, rien n'est encore tapé");
        assert_eq!(word_at(src, 3), None, "dans les espaces, aucun mot");
    }

    #[test]
    fn the_first_word_of_a_line_gets_mnemonics_and_the_rest_gets_registers() {
        let src = "    mo\n";
        let head = candidates(src, 4, "mo");
        assert!(head.iter().any(|c| c.text == "mov" && c.kind == Kind::Mnemonic));
        assert!(!head.iter().any(|c| c.kind == Kind::Register), "pas de registre en tête de ligne");

        let src = "    mov ra\n";
        let operand = candidates(src, 8, "ra");
        assert!(operand.iter().any(|c| c.text == "rax" && c.kind == Kind::Register));
        assert!(!operand.iter().any(|c| c.kind == Kind::Mnemonic), "pas de mnémonique en opérande");
    }

    #[test]
    fn labels_of_the_file_are_proposed_as_jump_targets() {
        let src = "_start:\n    mov rax, 1\nboucle_principale:\n    jmp bou\n";
        let at = src.find("bou\n").unwrap();
        // Les indices sont en caractères ; le source est en ASCII pur ici.
        let list = candidates(src, at, "bou");
        assert!(
            list.iter().any(|c| c.text == "boucle_principale"
                && c.kind == Kind::Symbol(SymbolKind::Label, 3)),
            "{list:?}"
        );
    }

    /// Le défaut signalé : dans un programme qui écrit une chaîne, `len` est
    /// déclaré par `len equ $ - msg` — sans deux-points. Ne reconnaître que
    /// les `nom:` laissait donc de côté exactement les noms qu'on tape le plus.
    #[test]
    fn symbols_declared_without_a_colon_are_recognised() {
        let src = "section .data\n                   msg db \"Bonjour\", 10\n                   len equ $ - msg\n                   buf resb 64\n                   %define TAILLE 4096\n                   extern printf\n";
        let found = symbols(src);
        let kind_of = |n: &str| found.iter().find(|(name, _, _)| name == n).map(|(_, k, l)| (*k, *l));
        assert_eq!(kind_of("msg"), Some((SymbolKind::Data, 2)));
        assert_eq!(kind_of("len"), Some((SymbolKind::Constant, 3)));
        assert_eq!(kind_of("buf"), Some((SymbolKind::Data, 4)));
        assert_eq!(kind_of("TAILLE"), Some((SymbolKind::Constant, 5)));
        assert_eq!(kind_of("printf"), Some((SymbolKind::External, 6)));
    }

    /// Et de bout en bout : taper `len` en opérande doit le proposer.
    #[test]
    fn a_constant_is_proposed_as_an_operand() {
        let src = "msg db \"Bonjour\", 10\nlen equ $ - msg\n_start:\n    mov rdx, le\n";
        let at = src.rfind("le\n").unwrap();
        let list = candidates(src, at, "le");
        assert!(
            list.iter().any(|c| c.text == "len" && c.kind == Kind::Symbol(SymbolKind::Constant, 2)),
            "« len » manquant : {list:?}"
        );
        // Et `lea` reste hors de la liste : c'est un mnémonique, pas un opérande.
        assert!(!list.iter().any(|c| c.text == "lea"), "{list:?}");
    }

    /// Une instruction ordinaire n'est pas une déclaration : sans ce garde-fou,
    /// `mov` deviendrait un « label » dès la première ligne de code venue.
    #[test]
    fn ordinary_instructions_are_not_taken_for_declarations() {
        let declared: Vec<String> = symbols("    mov rax, 1\n    section .data\n    db 10\n")
            .into_iter()
            .map(|(n, _, _)| n)
            .collect();
        assert!(declared.is_empty(), "{declared:?}");
    }

    #[test]
    fn a_comment_that_looks_like_a_declaration_is_not_one() {
        assert!(symbols("; faux:\n").is_empty());
        assert!(symbols("; len equ 3\n").is_empty());
        assert_eq!(symbols("vrai:\n"), vec![("vrai".to_string(), SymbolKind::Label, 1)]);
    }

    /// Les fonctions d'asmstd ne sont proposées que si le fichier l'inclut :
    /// sinon on ferait écrire un `call` que nasm refuserait.
    #[test]
    fn asmstd_functions_are_proposed_only_when_the_library_is_included() {
        let with = "%include \"asmstd.inc\"\n_start:\n    call asm.pr\n";
        let at = with.rfind("asm.pr").unwrap();
        let list = candidates(with, at + 6, "asm.pr");
        assert!(
            list.iter().any(|c| c.text == "asm.print" && matches!(c.kind, Kind::Asmstd(_))),
            "{list:?}"
        );

        let without = "_start:\n    call asm.pr\n";
        let at = without.rfind("asm.pr").unwrap();
        assert!(candidates(without, at + 6, "asm.pr").is_empty());
    }

    /// Le catalogue asmstd ne doit contenir que les fonctions PUBLIQUES : le
    /// fichier compte autant d'étiquettes internes (`.strlen_lp`), qu'on
    /// n'appelle jamais et qui doubleraient la liste pour rien.
    #[test]
    fn the_asmstd_catalogue_holds_documented_functions_only() {
        assert!(ASMSTD.len() > 80, "catalogue étrangement court : {}", ASMSTD.len());
        for (name, signature) in ASMSTD.iter() {
            assert!(name.starts_with("asm."), "{name} mal préfixé");
            assert!(!signature.is_empty(), "{name} sans signature");
            assert!(!name.contains("_lp"), "{name} est une étiquette interne");
        }
        let names: Vec<&String> = ASMSTD.iter().map(|(n, _)| n).collect();
        for expected in ["asm.print", "asm.write", "asm.exit", "asm.strlen"] {
            assert!(names.iter().any(|n| *n == expected), "{expected} manquant");
        }
    }

    /// Un mot déjà complet et sans rival ne doit pas laisser la liste ouverte
    /// par-dessus le code qu'on est en train d'écrire.
    #[test]
    fn a_finished_word_with_a_single_match_proposes_nothing() {
        assert!(candidates("    syscall\n", 4, "syscall").is_empty());
        // Mais « je » garde de quoi choisir (jecxz, etc.).
        assert!(!candidates("    je\n", 4, "je").is_empty());
    }

    #[test]
    fn nothing_is_proposed_for_a_prefix_that_matches_nothing() {
        assert!(candidates("    zzz\n", 4, "zzz").is_empty());
    }

    #[test]
    fn accepting_replaces_only_the_word_being_typed() {
        let src = "    mo rax, 1";
        let e = accept(src, 4, 6, "mov");
        assert_eq!(e.text, "    mov rax, 1");
        assert_eq!(e.sel, (7, 7));
    }

    #[test]
    fn register_widths_are_read_from_the_name() {
        assert_eq!(register_bits("rax"), 64);
        assert_eq!(register_bits("r12"), 64);
        assert_eq!(register_bits("eax"), 32);
        assert_eq!(register_bits("r12d"), 32);
        assert_eq!(register_bits("ax"), 16);
        assert_eq!(register_bits("r12w"), 16);
        assert_eq!(register_bits("al"), 8);
        assert_eq!(register_bits("r12b"), 8);
    }

    /// Chaque proposition doit savoir se présenter, dans les trois langues :
    /// une entrée sans libellé passerait inaperçue jusqu'à l'exécution.
    #[test]
    fn every_kind_has_a_hint_in_every_language() {
        let cases = [
            Candidate { text: "mov".into(), kind: Kind::Mnemonic },
            Candidate { text: "rax".into(), kind: Kind::Register },
            Candidate { text: "section".into(), kind: Kind::Directive },
            Candidate { text: "_start".into(), kind: Kind::Symbol(SymbolKind::Label, 1) },
            Candidate { text: "msg".into(), kind: Kind::Symbol(SymbolKind::Data, 2) },
            Candidate { text: "len".into(), kind: Kind::Symbol(SymbolKind::Constant, 3) },
            Candidate { text: "printf".into(), kind: Kind::Symbol(SymbolKind::External, 4) },
            Candidate { text: "asm.print".into(), kind: Kind::Asmstd("(rdi=ptr) -> rax") },
        ];
        for c in cases {
            for lang in [Lang::Fr, Lang::En, Lang::Es] {
                assert!(!c.hint(lang).is_empty(), "{c:?} sans libellé en {lang:?}");
            }
        }
    }

    /// De bout en bout : l'éditeur peint, la liste s'ouvre, une flèche déplace
    /// le choix, et l'accepter écrit le mot dans le source. C'est le seul test
    /// qui passe par la galley (position du curseur à l'écran), là où une API
    /// egui qui bouge se ferait sentir.
    #[test]
    fn the_list_opens_under_the_cursor_and_inserts_what_is_chosen() {
        let mut app = App::new();
        app.set_ui_mode(crate::app::UiMode::Full);
        app.source = "    mo".into();
        app.editor_sel = (6, 6);

        let ctx = egui::Context::default();
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, 700.0))),
            ..Default::default()
        };
        // L'éditeur doit avoir le curseur : la liste ne s'ouvre pas au-dessus
        // d'un panneau dont on a cliqué ailleurs.
        ctx.memory_mut(|m| m.request_focus(crate::app::editor_id()));
        let _ = ctx.run(input(), |ctx| app.dock_ui(ctx));
        // Le premier rendu donne le focus ; le second voit la liste.
        app.editor_sel = (6, 6);
        let _ = ctx.run(input(), |ctx| app.dock_ui(ctx));
        assert!(app.complete_open, "la liste devrait être ouverte sur « mo »");

        app.move_completion(true);
        let chosen = app.completions()[app.complete_sel].text.clone();
        app.accept_completion();
        assert_eq!(app.source, format!("    {chosen}"));
    }

    /// Les registres proposés doivent être ceux que la coloration syntaxique
    /// reconnaît : accepter un nom qui s'afficherait ensuite en texte ordinaire
    /// donnerait l'impression d'une faute de frappe.
    #[test]
    fn every_proposed_register_is_coloured_as_one() {
        for r in REGISTERS {
            let src = format!("    mov {r}, 1\n");
            let job = crate::syntax::highlight(
                &src,
                &crate::theme::by_id("dark").unwrap().syntax,
                None,
                None,
                None,
                None,
            );
            let col = crate::theme::by_id("dark").unwrap().syntax.register;
            assert!(
                job.sections.iter().any(|s| s.format.color == col && src[s.byte_range.clone()] == **r),
                "{r} n'est pas reconnu comme un registre"
            );
        }
    }
}
