//! Base de connaissances pédagogique : explique une instruction en clair.
//!
//! Pour les sauts conditionnels, la condition est évaluée contre les flags
//! réels afin d'afficher « le saut sera pris / non pris », comme dans la maquette.

use crate::debugger::Flags;
use crate::i18n::{self, Lang};

/// Résultat de l'évaluation d'un saut conditionnel :
/// (condition lisible, saut pris ?, flags pertinents avec leur valeur).
type JccEval = (String, bool, Vec<(&'static str, bool)>);

/// Explication structurée d'une instruction, prête à l'affichage.
pub struct Explanation {
    /// Titre lisible, ex. « JL — Jump if Less (saut si inférieur, signé) ».
    pub title: String,
    /// Catégorie, ex. « Saut conditionnel », « Arithmétique ».
    pub category: &'static str,
    /// Description en français simple de ce que fait l'instruction.
    pub description: String,
    /// Condition booléenne (pour les sauts), ex. « SF ≠ OF ».
    pub condition: Option<String>,
    /// Résultat de la condition avec les flags courants (None si non applicable).
    pub taken: Option<bool>,
    /// Flags pertinents à afficher avec leur valeur courante (nom, valeur).
    pub relevant_flags: Vec<(&'static str, bool)>,
    /// Flags positionnés par l'instruction.
    pub affects_flags: Vec<&'static str>,
}

/// Construit l'explication d'une instruction à partir de son mnémonique,
/// de ses opérandes et de l'état courant des flags.
pub fn explain(mnemonic: &str, operands: &str, flags: Flags, lang: Lang) -> Explanation {
    let m = mnemonic.to_lowercase();
    let t = |fr: &'static str, en: &'static str| i18n::tr(lang, fr, en);

    // --- Sauts conditionnels : condition évaluée contre les flags réels ---
    if let Some((cond, taken, rel)) = eval_jcc(&m, flags, lang) {
        return Explanation {
            title: format!("{} — {}", mnemonic.to_uppercase(), jcc_title(&m, lang)),
            category: t("Saut conditionnel", "Conditional jump"),
            description: format!(
                "{} {}.",
                t(
                    "Saut relatif si la condition est vraie. Cible :",
                    "Relative jump if the condition is true. Target:"
                ),
                if operands.is_empty() { t("(opérande)", "(operand)") } else { operands }
            ),
            condition: Some(cond),
            taken: Some(taken),
            relevant_flags: rel,
            affects_flags: vec![],
        };
    }

    // --- Autres instructions courantes ---
    let (category, description, affects): (&str, String, Vec<&str>) = match m.as_str() {
        "mov" => (
            t("Transfert", "Transfer"),
            t(
                "Copie la source dans la destination (aucun flag modifié). \
                 Note : écrire dans un registre 32 bits (eax) remet à zéro les 32 bits hauts du 64 bits (rax).",
                "Copies the source into the destination (no flag modified). \
                 Note: writing to a 32-bit register (eax) zeroes the upper 32 bits of the 64-bit register (rax).",
            ).to_string(),
            vec![],
        ),
        "movabs" => (
            t("Transfert", "Transfer"),
            t("Charge un immédiat 64 bits complet dans un registre.", "Loads a full 64-bit immediate into a register.").to_string(),
            vec![],
        ),
        "lea" => (
            t("Adressage", "Addressing"),
            t(
                "Load Effective Address : calcule une adresse (base + index*échelle + déplacement) \
                 et la place dans la destination, SANS accéder à la mémoire. Sert aussi d'arithmétique rapide.",
                "Load Effective Address: computes an address (base + index*scale + displacement) \
                 and stores it in the destination WITHOUT accessing memory. Also handy for fast arithmetic.",
            ).to_string(),
            vec![],
        ),
        "push" => (
            t("Pile", "Stack"),
            t("Décrémente RSP de 8 puis écrit l'opérande au sommet de la pile.", "Decrements RSP by 8 then writes the operand at the top of the stack.").to_string(),
            vec![],
        ),
        "pop" => (
            t("Pile", "Stack"),
            t("Lit le sommet de la pile dans la destination puis incrémente RSP de 8.", "Reads the top of the stack into the destination then increments RSP by 8.").to_string(),
            vec![],
        ),
        "add" => (
            t("Arithmétique", "Arithmetic"),
            t("Additionne source à destination. Positionne les flags selon le résultat.", "Adds source to destination. Sets the flags according to the result.").to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF", "AF"],
        ),
        "sub" => (
            t("Arithmétique", "Arithmetic"),
            t("Soustrait source de destination. Positionne les flags selon le résultat.", "Subtracts source from destination. Sets the flags according to the result.").to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF", "AF"],
        ),
        "imul" => (
            t("Arithmétique", "Arithmetic"),
            t("Multiplication signée. CF et OF sont mis à 1 si le résultat déborde de la taille de destination.", "Signed multiplication. CF and OF are set if the result overflows the destination size.").to_string(),
            vec!["CF", "OF"],
        ),
        "mul" => (
            t("Arithmétique", "Arithmetic"),
            t("Multiplication non signée (RDX:RAX). CF/OF indiquent un débordement dans la partie haute.", "Unsigned multiplication (RDX:RAX). CF/OF indicate overflow into the high part.").to_string(),
            vec!["CF", "OF"],
        ),
        "inc" => (
            t("Arithmétique", "Arithmetic"),
            t("Incrémente de 1. Ne modifie PAS CF (contrairement à add).", "Increments by 1. Does NOT modify CF (unlike add).").to_string(),
            vec!["OF", "SF", "ZF", "PF", "AF"],
        ),
        "dec" => (
            t("Arithmétique", "Arithmetic"),
            t("Décrémente de 1. Ne modifie PAS CF.", "Decrements by 1. Does NOT modify CF.").to_string(),
            vec!["OF", "SF", "ZF", "PF", "AF"],
        ),
        "neg" => (
            t("Arithmétique", "Arithmetic"),
            t("Remplace l'opérande par son opposé (complément à deux).", "Replaces the operand with its negation (two's complement).").to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF", "AF"],
        ),
        "cmp" => (
            t("Comparaison", "Comparison"),
            t(
                "Calcule (destination - source) SANS stocker le résultat : seuls les flags sont positionnés. \
                 C'est ce qui prépare un saut conditionnel : ZF=1 si égaux, et SF/OF/CF codent l'ordre.",
                "Computes (destination - source) WITHOUT storing the result: only the flags are set. \
                 This is what prepares a conditional jump: ZF=1 if equal, and SF/OF/CF encode the ordering.",
            ).to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF", "AF"],
        ),
        "test" => (
            t("Comparaison", "Comparison"),
            t(
                "Calcule (destination AND source) sans le stocker : positionne les flags. \
                 `test rax, rax` sert à savoir si rax est nul (ZF=1) ou négatif (SF=1).",
                "Computes (destination AND source) without storing it: sets the flags. \
                 `test rax, rax` tells whether rax is zero (ZF=1) or negative (SF=1).",
            ).to_string(),
            vec!["SF", "ZF", "PF"],
        ),
        "and" => (
            t("Logique", "Logic"),
            t("ET bit à bit. CF et OF sont mis à 0.", "Bitwise AND. CF and OF are cleared.").to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF"],
        ),
        "or" => (
            t("Logique", "Logic"),
            t("OU bit à bit. CF et OF sont mis à 0.", "Bitwise OR. CF and OF are cleared.").to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF"],
        ),
        "xor" => (
            t("Logique", "Logic"),
            t(
                "OU exclusif bit à bit. `xor rax, rax` est l'idiome pour mettre rax à 0 \
                 (plus court que mov rax, 0). CF et OF sont mis à 0.",
                "Bitwise exclusive OR. `xor rax, rax` is the idiom to zero rax \
                 (shorter than mov rax, 0). CF and OF are cleared.",
            ).to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF"],
        ),
        "shl" | "sal" => (
            t("Décalage", "Shift"),
            t("Décale les bits vers la gauche (multiplie par 2 par bit). Le dernier bit sorti va dans CF.", "Shifts bits left (multiplies by 2 per bit). The last bit shifted out goes to CF.").to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF"],
        ),
        "shr" => (
            t("Décalage", "Shift"),
            t("Décale les bits vers la droite (division non signée par 2). Le dernier bit sorti va dans CF.", "Shifts bits right (unsigned division by 2). The last bit shifted out goes to CF.").to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF"],
        ),
        "jmp" => (
            t("Saut", "Jump"),
            t("Saut inconditionnel : RIP prend la valeur de la cible.", "Unconditional jump: RIP takes the target value.").to_string(),
            vec![],
        ),
        "call" => (
            t("Appel", "Call"),
            t("Empile l'adresse de retour (RSP -= 8) puis saute vers la fonction cible.", "Pushes the return address (RSP -= 8) then jumps to the target function.").to_string(),
            vec![],
        ),
        "ret" => (
            t("Appel", "Call"),
            t("Dépile l'adresse de retour dans RIP (RSP += 8) : revient à l'appelant.", "Pops the return address into RIP (RSP += 8): returns to the caller.").to_string(),
            vec![],
        ),
        "syscall" => (
            t("Système", "System"),
            t(
                "Appel système Linux : RAX = numéro, arguments dans RDI, RSI, RDX, R10, R8, R9. \
                 Le noyau exécute l'opération (write, read, exit...) et renvoie le résultat dans RAX.",
                "Linux system call: RAX = number, arguments in RDI, RSI, RDX, R10, R8, R9. \
                 The kernel runs the operation (write, read, exit...) and returns the result in RAX.",
            ).to_string(),
            vec![],
        ),
        "nop" => (t("Divers", "Misc"), t("Ne fait rien (No Operation).", "Does nothing (No Operation).").to_string(), vec![]),
        "leave" => (
            t("Pile", "Stack"),
            t("Équivaut à `mov rsp, rbp ; pop rbp` : démonte le cadre de pile de la fonction.", "Equivalent to `mov rsp, rbp ; pop rbp`: tears down the function's stack frame.").to_string(),
            vec![],
        ),
        _ => (
            t("Inconnu", "Unknown"),
            format!(
                "{} « {mnemonic} » {}",
                t("Instruction", "Instruction"),
                t(": explication non encore répertoriée.", ": explanation not catalogued yet."),
            ),
            vec![],
        ),
    };

    Explanation {
        title: mnemonic.to_uppercase(),
        category,
        description,
        condition: None,
        taken: None,
        relevant_flags: vec![],
        affects_flags: affects,
    }
}

/// Estimation (très approximative) du coût en cycles d'une instruction, pour le
/// mode microscope. Ordres de grandeur pédagogiques, pas des valeurs exactes
/// (elles dépendent de la microarchitecture, du cache, des dépendances…).
pub fn cycles_estimate(mnemonic: &str) -> &'static str {
    match mnemonic.to_lowercase().as_str() {
        "nop" => "~0–1",
        "mov" | "movabs" | "lea" | "xor" | "or" | "and" | "add" | "sub" | "cmp" | "test"
        | "inc" | "dec" | "neg" | "not" | "shl" | "sal" | "shr" | "sar" => "~1",
        "push" | "pop" | "jmp" => "~1–2",
        "je" | "jne" | "jz" | "jnz" | "jg" | "jge" | "jl" | "jle" | "ja" | "jae" | "jb" | "jbe"
        | "js" | "jns" | "jo" | "jno" | "jp" | "jnp" => "~1–2 (0 si bien prédit)",
        "call" | "ret" => "~1–3",
        "imul" | "mul" => "~3–5",
        "div" | "idiv" => "~20–40",
        "syscall" => "~100+ (bascule noyau)",
        _ => "≈ variable",
    }
}

/// Lien vers la référence officielle de l'instruction : le manuel Intel (SDM),
/// via le mirror consultable en ligne felixcloutier.com/x86 (une page par
/// instruction). Les sauts conditionnels partagent la page « Jcc » ; certaines
/// instructions partagent une page groupée.
pub fn doc_url(mnemonic: &str) -> String {
    let m = mnemonic.to_lowercase();
    let slug: &str = match m.as_str() {
        // Sauts conditionnels : page unique « Jcc ».
        "je" | "jz" | "jne" | "jnz" | "jg" | "jnle" | "jge" | "jnl" | "jl" | "jnge" | "jle"
        | "jng" | "ja" | "jnbe" | "jae" | "jnb" | "jnc" | "jb" | "jc" | "jnae" | "jbe" | "jna"
        | "js" | "jns" | "jo" | "jno" | "jp" | "jpe" | "jnp" | "jpo" | "jcxz" | "jecxz"
        | "jrcxz" => "jcc",
        "movabs" => "mov",                          // pseudo-instruction NASM = MOV imm64
        "sal" | "sar" | "shl" | "shr" => "sal:sar:shl:shr",
        "rol" | "ror" | "rcl" | "rcr" => "rcl:rcr:rol:ror",
        // Par défaut, le mnémonique EST le slug (couvre mov, add, sub, and, or,
        // xor, cmp, test, lea, push, pop, call, ret, inc, dec, neg, not, mul,
        // imul, div, idiv, nop, syscall, jmp…).
        other => return format!("https://www.felixcloutier.com/x86/{other}"),
    };
    format!("https://www.felixcloutier.com/x86/{slug}")
}

/// Titre lisible d'un saut conditionnel.
fn jcc_title(m: &str, lang: Lang) -> &'static str {
    let t = |fr: &'static str, en: &'static str| i18n::tr(lang, fr, en);
    match m {
        "je" | "jz" => "Jump if Equal / Zero",
        "jne" | "jnz" => "Jump if Not Equal / Not Zero",
        "jg" | "jnle" => t("Jump if Greater (signé)", "Jump if Greater (signed)"),
        "jge" | "jnl" => t("Jump if Greater or Equal (signé)", "Jump if Greater or Equal (signed)"),
        "jl" | "jnge" => t("Jump if Less (signé)", "Jump if Less (signed)"),
        "jle" | "jng" => t("Jump if Less or Equal (signé)", "Jump if Less or Equal (signed)"),
        "ja" | "jnbe" => t("Jump if Above (non signé)", "Jump if Above (unsigned)"),
        "jae" | "jnb" | "jnc" => t("Jump if Above or Equal (non signé)", "Jump if Above or Equal (unsigned)"),
        "jb" | "jc" | "jnae" => t("Jump if Below (non signé)", "Jump if Below (unsigned)"),
        "jbe" | "jna" => t("Jump if Below or Equal (non signé)", "Jump if Below or Equal (unsigned)"),
        "js" => t("Jump if Sign (négatif)", "Jump if Sign (negative)"),
        "jns" => t("Jump if Not Sign (positif ou nul)", "Jump if Not Sign (positive or zero)"),
        "jo" => "Jump if Overflow",
        "jno" => "Jump if Not Overflow",
        "jp" | "jpe" => "Jump if Parity Even",
        "jnp" | "jpo" => "Jump if Parity Odd",
        _ => t("Saut conditionnel", "Conditional jump"),
    }
}

/// Évalue un saut conditionnel : renvoie (condition lisible, pris ?, flags pertinents).
/// Renvoie None si `m` n'est pas un saut conditionnel connu.
fn eval_jcc(m: &str, f: Flags, lang: Lang) -> Option<JccEval> {
    let zf = ("ZF", f.zf);
    let cf = ("CF", f.cf);
    let sf = ("SF", f.sf);
    let of = ("OF", f.of);
    let pf = ("PF", f.pf);
    let and = i18n::tr(lang, "et", "and");
    let or = i18n::tr(lang, "ou", "or");

    let out = match m {
        "je" | "jz" => ("ZF = 1".into(), f.zf, vec![zf]),
        "jne" | "jnz" => ("ZF = 0".into(), !f.zf, vec![zf]),
        "jg" | "jnle" => (format!("ZF = 0 {and} SF = OF"), !f.zf && (f.sf == f.of), vec![zf, sf, of]),
        "jge" | "jnl" => ("SF = OF".into(), f.sf == f.of, vec![sf, of]),
        "jl" | "jnge" => ("SF ≠ OF".into(), f.sf != f.of, vec![sf, of]),
        "jle" | "jng" => (format!("ZF = 1 {or} SF ≠ OF"), f.zf || (f.sf != f.of), vec![zf, sf, of]),
        "ja" | "jnbe" => (format!("CF = 0 {and} ZF = 0"), !f.cf && !f.zf, vec![cf, zf]),
        "jae" | "jnb" | "jnc" => ("CF = 0".into(), !f.cf, vec![cf]),
        "jb" | "jc" | "jnae" => ("CF = 1".into(), f.cf, vec![cf]),
        "jbe" | "jna" => (format!("CF = 1 {or} ZF = 1"), f.cf || f.zf, vec![cf, zf]),
        "js" => ("SF = 1".into(), f.sf, vec![sf]),
        "jns" => ("SF = 0".into(), !f.sf, vec![sf]),
        "jo" => ("OF = 1".into(), f.of, vec![of]),
        "jno" => ("OF = 0".into(), !f.of, vec![of]),
        "jp" | "jpe" => ("PF = 1".into(), f.pf, vec![pf]),
        "jnp" | "jpo" => ("PF = 0".into(), !f.pf, vec![pf]),
        _ => return None,
    };
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_url_maps_jcc_and_defaults() {
        assert_eq!(doc_url("mov"), "https://www.felixcloutier.com/x86/mov");
        assert_eq!(doc_url("ADD"), "https://www.felixcloutier.com/x86/add");
        // Tous les sauts conditionnels pointent vers la page « jcc ».
        assert_eq!(doc_url("jl"), "https://www.felixcloutier.com/x86/jcc");
        assert_eq!(doc_url("jne"), "https://www.felixcloutier.com/x86/jcc");
        // movabs (pseudo NASM) => page MOV.
        assert_eq!(doc_url("movabs"), "https://www.felixcloutier.com/x86/mov");
        assert_eq!(doc_url("shl"), "https://www.felixcloutier.com/x86/sal:sar:shl:shr");
    }

    #[test]
    fn jl_taken_when_sf_ne_of() {
        // Après cmp 5, 8 : SF=1, OF=0 => SF ≠ OF => jl pris.
        let f = Flags { sf: true, of: false, ..Default::default() };
        let e = explain("jl", "erreur", f, Lang::Fr);
        assert_eq!(e.taken, Some(true));
        assert_eq!(e.condition.as_deref(), Some("SF ≠ OF"));
    }

    #[test]
    fn je_not_taken_when_zf_zero() {
        let f = Flags { zf: false, ..Default::default() };
        let e = explain("je", "cible", f, Lang::Fr);
        assert_eq!(e.taken, Some(false));
    }

    #[test]
    fn cmp_lists_affected_flags() {
        let e = explain("cmp", "rax, rbx", Flags::default(), Lang::Fr);
        assert!(e.affects_flags.contains(&"ZF"));
        assert!(e.taken.is_none());
    }
}
