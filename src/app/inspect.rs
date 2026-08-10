//! Inspection au survol : ce que vaut, ici et maintenant, le mot sous le
//! pointeur dans l'éditeur.
//!
//! Pendant un pas à pas, la question qui revient sans cesse est « et RSI, il
//! vaut combien à cet instant ? ». Y répondre demandait d'aller chercher le
//! registre dans son panneau, de le retrouver parmi seize, et de revenir au
//! code — trois allers-retours pour une information qui tient en une ligne.
//! Elle s'affiche désormais là où la question se pose : sur le mot lui-même.
//!
//! Quatre sortes de mots sont reconnues, dans cet ordre : registre, drapeau,
//! label défini dans le fichier, littéral numérique. Le reste ne dit rien —
//! une infobulle qui s'ouvre sur « mov » n'apprendrait rien à personne.

use crate::debugger::{Flags, Registers};
use crate::i18n;

use super::App;

/// Ce qu'on affiche au survol : un titre (le mot, tel qu'on l'a compris) et
/// quelques lignes de valeurs.
pub(super) struct Inspection {
    pub(super) title: String,
    pub(super) lines: Vec<String>,
}

/// Le mot qui contient ce caractère (index en caractères, pas en octets).
///
/// Un « mot » ici, c'est ce qui peut nommer quelque chose en NASM : lettres,
/// chiffres, `_`, et le `.` initial des labels locaux (`.boucle`). Le `$` et
/// le `%` des directives en sont exclus : ils ne désignent rien qu'on sache
/// évaluer.
pub(super) fn word_at(source: &str, char_index: usize) -> Option<String> {
    let chars: Vec<char> = source.chars().collect();
    if char_index >= chars.len() {
        return None;
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '.';
    if !is_word(chars[char_index]) {
        return None;
    }
    let mut start = char_index;
    while start > 0 && is_word(chars[start - 1]) {
        start -= 1;
    }
    let mut end = char_index;
    while end + 1 < chars.len() && is_word(chars[end + 1]) {
        end += 1;
    }
    let word: String = chars[start..=end].iter().collect();
    // Un `.` isolé ou en fin de mot (fin de phrase d'un commentaire) ne nomme
    // rien : on ne garde que les vrais labels locaux.
    (!word.is_empty() && word != ".").then_some(word)
}

impl App {
    /// Ce que vaut ce mot dans l'état affiché, ou `None` s'il ne désigne rien
    /// que l'on sache évaluer.
    pub(super) fn inspect(&self, word: &str) -> Option<Inspection> {
        let lang = self.lang;
        let upper = word.to_ascii_uppercase();
        let snap = self.snap();
        let regs = snap.map(|s| &s.regs);

        // 1. Registre (ou sa moitié basse), quand un programme tourne.
        if let Some(regs) = regs
            && let Some((name, value, bits)) = register_value(regs, &upper)
        {
            return Some(Inspection {
                title: name,
                lines: self.register_lines(value, bits),
            });
        }

        // 2. Drapeau.
        if let Some(regs) = regs
            && let Some((name, set)) = Flags::from_eflags(regs.eflags)
                .named()
                .into_iter()
                .find(|(n, _)| *n == upper)
        {
            let state = if set {
                i18n::tr3(lang, "levé", "set", "activado")
            } else {
                i18n::tr3(lang, "baissé", "clear", "desactivado")
            };
            return Some(Inspection {
                title: name.to_string(),
                lines: vec![format!("{} — {state}", u8::from(set))],
            });
        }

        // 3. Label défini dans le fichier.
        if let Some(line) = label_line(&self.source, word) {
            let mut lines = vec![format!("{} {line}", i18n::tr3(lang, "défini ligne", "defined at line", "definido en la línea"))];
            // L'adresse n'existe qu'après un assemblage, et seulement pour un
            // label de code : celui d'une donnée n'a pas de ligne machine.
            if let Some(addr) = self.src_map.iter().find(|(_, l)| **l == line).map(|(a, _)| *a) {
                lines.push(format!("0x{addr:X}"));
            }
            return Some(Inspection { title: word.to_string(), lines });
        }

        // 4. Littéral : la même valeur dans les trois bases, ce qui est
        // exactement ce qu'on veut vérifier en lisant un masque ou un code
        // d'appel système.
        if let Some(value) = parse_literal(&upper) {
            let mut lines = vec![
                format!("0x{value:X}"),
                format!("{value}"),
                format!("0b{value:b}"),
            ];
            if let Some(c) = printable(value) {
                lines.push(format!("'{c}'"));
            }
            return Some(Inspection { title: word.to_string(), lines });
        }

        None
    }

    /// Affiche l'inspection du mot survolé, s'il y en a une à faire.
    ///
    /// Disposée **en ligne** : le nom, puis ses valeurs séparées par un point
    /// médian. En colonne, l'infobulle était un bloc haut et étroit qui couvrait
    /// les lignes de code sous le pointeur — précisément celles qu'on est en
    /// train de lire. En ligne, elle tient sur la hauteur d'une ligne de texte
    /// et se lit d'un seul balayage, comme la barre d'état. Le repli n'a lieu
    /// que si la place manque vraiment (registre pointant sur de la mémoire,
    /// dont le contenu s'ajoute).
    ///
    /// Le réglage « inspection au survol » la coupe entièrement. Rien ne
    /// s'affiche non plus pendant une frappe : l'infobulle passerait juste sous
    /// le curseur au moment le plus gênant.
    pub(super) fn inspect_tooltip(&self, ui: &mut eframe::egui::Ui, char_index: usize) {
        use eframe::egui::{self, RichText};
        if !self.inspect_hover || ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_) | egui::Event::Key { .. }))) {
            return;
        }
        let Some(word) = word_at(&self.source, char_index) else { return };
        let Some(insp) = self.inspect(&word) else { return };
        egui::Tooltip::always_open(
            ui.ctx().clone(),
            ui.layer_id(),
            egui::Id::new("editor_inspect_tip"),
            egui::PopupAnchor::Pointer,
        )
        .show(|ui| {
            // Assez large pour que les cas courants (registre, littéral) tiennent
            // sur une ligne, assez borné pour ne pas barrer l'écran quand un
            // pointeur ramène huit octets de mémoire avec lui.
            ui.set_max_width(560.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new(&insp.title).monospace().strong().color(super::accent()));
                for (i, line) in insp.lines.iter().enumerate() {
                    if i > 0 {
                        ui.label(RichText::new("·").weak());
                    }
                    ui.label(RichText::new(line).monospace());
                }
            });
        });
    }

    /// Valeur d'un registre sous toutes les formes utiles : hexadécimale (la
    /// façon dont on lit un registre), décimale, décimale signée quand elle
    /// diffère, le caractère quand l'octet bas en est un, et ce qui se trouve
    /// à l'adresse pointée quand c'en est une.
    fn register_lines(&self, value: u64, bits: u32) -> Vec<String> {
        let lang = self.lang;
        let width = (bits / 4) as usize;
        let mut lines = vec![format!("0x{value:0width$X}"), format!("{value}")];
        let signed = match bits {
            32 => value as u32 as i32 as i64,
            _ => value as i64,
        };
        if signed < 0 {
            lines.push(format!("{signed} {}", i18n::tr3(lang, "(signé)", "(signed)", "(con signo)")));
        }
        if let Some(c) = printable(value as i128) {
            lines.push(format!("'{c}'"));
        }
        // Contenu pointé : c'est la réponse à « mon pointeur vise-t-il bien ma
        // chaîne ? ». Une adresse invalide échoue sans bruit, ce qui est déjà
        // une réponse.
        if let Some(bytes) = self
            .dbg
            .as_ref()
            .filter(|_| self.can_read_memory() && value > 0xFFFF)
            .and_then(|d| d.read_mem(value, 8).ok())
        {
            let word = u64::from_le_bytes(bytes[..8].try_into().expect("8 octets lus"));
            let text: String = bytes
                .iter()
                .map(|b| if (0x20..0x7F).contains(b) { *b as char } else { '·' })
                .collect();
            lines.push(format!(
                "→ 0x{word:016X}  “{text}”",
            ));
        }
        lines
    }
}

/// Registre nommé, sa valeur et sa largeur en bits. Reconnaît les noms 64 bits
/// (RAX…R15, RIP, EFLAGS) et les moitiés basses (EAX…EDI, R8D…R15D).
fn register_value(regs: &Registers, upper: &str) -> Option<(String, u64, u32)> {
    if let Some((name, value)) = regs.named().into_iter().find(|(n, _)| *n == upper) {
        return Some((name.to_string(), value, 64));
    }
    let full = match upper.strip_suffix('D') {
        Some(base) if base.starts_with('R') => base.to_string(),
        _ => match upper.strip_prefix('E') {
            // « EFLAGS » a déjà été traité au-dessus, comme registre entier.
            Some(rest) if upper != "EFLAGS" => format!("R{rest}"),
            _ => return None,
        },
    };
    let (_, value) = regs.named().into_iter().find(|(n, _)| *n == full)?;
    Some((upper.to_string(), value & 0xFFFF_FFFF, 32))
}

/// Ligne (1-based) où ce label est défini, s'il l'est dans ce fichier.
///
/// Un label NASM est en début de ligne et suivi de `:` — ou, pour une donnée,
/// suivi d'une directive `db`/`dw`/`dd`/`dq`/`equ` sans deux-points.
fn label_line(source: &str, word: &str) -> Option<usize> {
    source.lines().enumerate().find_map(|(i, line)| {
        let line = line.trim_start();
        let rest = line.strip_prefix(word)?;
        let rest = rest.trim_start();
        let is_label = rest.starts_with(':')
            || rest
                .split_whitespace()
                .next()
                .is_some_and(|w| {
                    matches!(w.to_ascii_lowercase().as_str(), "db" | "dw" | "dd" | "dq" | "equ" | "resb" | "resw" | "resd" | "resq" | "times")
                });
        is_label.then_some(i + 1)
    })
}

/// Littéral NASM : décimal, `0x…`/`…h` hexadécimal, `0b…` binaire.
fn parse_literal(upper: &str) -> Option<i128> {
    let digits = upper.replace('_', "");
    if let Some(hex) = digits.strip_prefix("0X") {
        return i128::from_str_radix(hex, 16).ok();
    }
    if let Some(bin) = digits.strip_prefix("0B") {
        return i128::from_str_radix(bin, 2).ok();
    }
    if let Some(hex) = digits.strip_suffix('H') {
        return i128::from_str_radix(hex, 16).ok();
    }
    digits.parse::<i128>().ok()
}

/// Le caractère ASCII imprimable que représente cette valeur, s'il y en a un.
fn printable(value: i128) -> Option<char> {
    (0x20..0x7F).contains(&value).then_some(value as u8 as char)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_is_found_from_anywhere_inside_it() {
        let src = "    mov rax, 42\n";
        // « rax » occupe les caractères 8..11.
        for i in 8..11 {
            assert_eq!(word_at(src, i).as_deref(), Some("rax"), "index {i}");
        }
        assert_eq!(word_at(src, 4).as_deref(), Some("mov"));
        assert_eq!(word_at(src, 13).as_deref(), Some("42"));
    }

    #[test]
    fn punctuation_and_spaces_name_nothing() {
        let src = "mov [rbx+8], al\n";
        assert_eq!(word_at(src, 3), None, "l'espace");
        assert_eq!(word_at(src, 4), None, "le crochet");
        assert_eq!(word_at(src, 5).as_deref(), Some("rbx"));
        assert_eq!(word_at(src, 9).as_deref(), Some("8"));
        assert_eq!(word_at(src, 999), None, "au-delà de la fin");
    }

    #[test]
    fn local_labels_keep_their_leading_dot() {
        let src = "    jnz .boucle\n";
        assert_eq!(word_at(src, 10).as_deref(), Some(".boucle"));
    }

    #[test]
    fn a_number_shows_its_three_bases_and_its_character() {
        let app = App::new();
        let insp = app.inspect("0x41").expect("un littéral s'inspecte toujours");
        assert_eq!(insp.title, "0x41");
        assert!(insp.lines.contains(&"65".to_string()), "{:?}", insp.lines);
        assert!(insp.lines.contains(&"0b1000001".to_string()), "{:?}", insp.lines);
        assert!(insp.lines.contains(&"'A'".to_string()), "{:?}", insp.lines);
    }

    #[test]
    fn decimal_and_nasm_hex_are_both_understood() {
        let app = App::new();
        assert!(app.inspect("60").is_some(), "décimal");
        assert!(app.inspect("2Ah").is_some(), "hexadécimal à la NASM");
        assert!(app.inspect("0b1010").is_some(), "binaire");
    }

    #[test]
    fn a_label_reports_where_it_is_defined() {
        let mut app = App::new();
        app.source = "section .data\nmsg db \"hi\", 10\n\nsection .text\n_start:\n    nop\n".to_string();
        let msg = app.inspect("msg").expect("label de donnée");
        assert!(msg.lines[0].contains('2'), "ligne 2 : {:?}", msg.lines);
        let start = app.inspect("_start").expect("label de code");
        assert!(start.lines[0].contains('5'), "ligne 5 : {:?}", start.lines);
    }

    /// Sans programme lancé, un registre ne vaut rien de particulier : mieux
    /// vaut ne rien dire que d'afficher un zéro qui n'est pas une valeur.
    #[test]
    fn registers_say_nothing_while_nothing_runs() {
        let app = App::new();
        assert!(app.dbg.is_none());
        assert!(app.inspect("rax").is_none());
        assert!(app.inspect("ZF").is_none());
    }

    #[test]
    fn ordinary_words_are_not_inspectable() {
        let app = App::new();
        for word in ["mov", "section", "global", "syscall"] {
            assert!(app.inspect(word).is_none(), "{word}");
        }
    }

    /// Le cas d'usage réel : un programme tourne, on survole un registre, et
    /// on lit sa valeur sans quitter le code des yeux.
    #[test]
    fn a_running_register_shows_what_it_holds() {
        let mut app = App::new();
        app.src_path = std::path::PathBuf::from("build/inspect-test.asm");
        app.out_dir = std::path::PathBuf::from("build/inspect");
        app.source = "section .text\n global _start\n_start:\n mov rax,0x41\n \
                      cmp rax,rax\n mov rax,60\n xor rdi,rdi\n syscall\n"
            .to_string();
        app.launch();
        assert!(app.dbg.is_some(), "le programme doit être lancé");
        app.step(); // « mov rax,0x41 »

        let rax = app.inspect("rax").expect("RAX s'inspecte quand ça tourne");
        assert_eq!(rax.title, "RAX");
        assert!(rax.lines[0].ends_with("41"), "hexa d'abord : {:?}", rax.lines);
        assert!(rax.lines.contains(&"65".to_string()), "et le décimal : {:?}", rax.lines);
        assert!(rax.lines.contains(&"'A'".to_string()), "et le caractère : {:?}", rax.lines);

        // La moitié basse se lit aussi, sur la même valeur.
        assert_eq!(app.inspect("eax").expect("EAX").title, "EAX");

        app.step(); // « cmp rax,rax » : égalité, donc ZF levé
        let zf = app.inspect("ZF").expect("les drapeaux aussi");
        assert_eq!(zf.title, "ZF");
        assert!(zf.lines[0].starts_with('1'), "ZF doit être levé : {:?}", zf.lines);
    }

    // ---------- Rendu de l'infobulle ----------

    /// Positions de chaque morceau de texte peint par l'infobulle, tous calques
    /// confondus. Les formes peuvent être imbriquées (`Shape::Vec`) selon la
    /// façon dont egui regroupe le calque : on descend donc dedans.
    fn tooltip_text_rows(app: &App, char_index: usize) -> Vec<f32> {
        use eframe::egui;
        fn collect(shape: &egui::Shape, out: &mut Vec<f32>) {
            match shape {
                egui::Shape::Text(t) => out.push(t.pos.y),
                egui::Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        // L'infobulle s'ancre au pointeur : sans position de pointeur, egui n'a
        // nulle part où la poser et ne peint rien.
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, 700.0))),
            events: vec![egui::Event::PointerMoved(egui::pos2(300.0, 200.0))],
            ..Default::default()
        };
        // Deux images : la première place l'infobulle, la seconde la peint à sa
        // taille définitive.
        let _ = ctx.run(input(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.inspect_tooltip(ui, char_index));
        });
        let out = ctx.run(input(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.inspect_tooltip(ui, char_index));
        });
        let mut rows = Vec::new();
        for cs in &out.shapes {
            collect(&cs.shape, &mut rows);
        }
        rows
    }

    /// Le point de la demande : les valeurs se lisent SUR UNE LIGNE. En colonne,
    /// l'infobulle était un bloc haut et étroit qui masquait le code sous le
    /// pointeur — celui qu'on est justement en train de lire.
    #[test]
    fn the_tooltip_lays_its_values_out_on_a_single_row() {
        let mut app = App::new();
        app.source = "    mov rax, 0x41\n".to_string();
        let at = app.source.find("0x41").expect("le littéral");

        let rows = tooltip_text_rows(&app, at);
        assert!(rows.len() >= 4, "infobulle trop pauvre : {rows:?}");
        let (min, max) = (
            rows.iter().cloned().fold(f32::MAX, f32::min),
            rows.iter().cloned().fold(f32::MIN, f32::max),
        );
        assert!(max - min < 2.0, "les valeurs s'empilent encore ({min} → {max})");
    }

    /// Et le réglage la coupe pour de bon : plus un seul mot peint.
    #[test]
    fn the_hover_setting_switches_the_tooltip_off() {
        let mut app = App::new();
        app.source = "    mov rax, 0x41\n".to_string();
        let at = app.source.find("0x41").expect("le littéral");
        assert!(!tooltip_text_rows(&app, at).is_empty(), "activée par défaut");

        app.inspect_hover = false;
        assert!(tooltip_text_rows(&app, at).is_empty(), "coupée, elle ne peint rien");
    }

    /// Elle ne dépend PLUS des infobulles de raccourcis : celles-ci parlent des
    /// boutons de la barre d'outils, pas du code. Les confondre rendait le
    /// nouveau réglage sans effet apparent pour qui avait coupé les premières.
    #[test]
    fn hover_inspection_is_independent_from_the_shortcut_tooltips() {
        let mut app = App::new();
        app.source = "    mov rax, 0x41\n".to_string();
        let at = app.source.find("0x41").expect("le littéral");
        app.show_tooltips = false;
        assert!(!tooltip_text_rows(&app, at).is_empty());
    }

    #[test]
    fn half_registers_are_recognized() {
        let regs = Registers { rax: 0x1234_5678_9ABC_DEF0, r8: 0xFFFF_FFFF_0000_002A, ..Default::default() };
        assert_eq!(register_value(&regs, "RAX"), Some(("RAX".to_string(), 0x1234_5678_9ABC_DEF0, 64)));
        assert_eq!(register_value(&regs, "EAX"), Some(("EAX".to_string(), 0x9ABC_DEF0, 32)));
        assert_eq!(register_value(&regs, "R8D"), Some(("R8D".to_string(), 0x2A, 32)));
        assert_eq!(register_value(&regs, "RXX"), None);
    }

    #[test]
    fn eflags_is_not_mistaken_for_half_of_anything() {
        let regs = Registers { eflags: 0x246, ..Default::default() };
        assert_eq!(register_value(&regs, "EFLAGS"), Some(("EFLAGS".to_string(), 0x246, 64)));
    }
}
