//! Base de connaissances pédagogique : explique une instruction en clair.
//!
//! Pour les sauts conditionnels, la condition est évaluée contre les flags
//! réels afin d'afficher « le saut sera pris / non pris », comme dans la maquette.

use crate::debugger::Flags;

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
pub fn explain(mnemonic: &str, operands: &str, flags: Flags) -> Explanation {
    let m = mnemonic.to_lowercase();

    // --- Sauts conditionnels : condition évaluée contre les flags réels ---
    if let Some((cond, taken, rel)) = eval_jcc(&m, flags) {
        return Explanation {
            title: format!("{} — {}", mnemonic.to_uppercase(), jcc_title(&m)),
            category: "Saut conditionnel",
            description: format!(
                "Saut relatif si la condition est vraie. Cible : {}.",
                if operands.is_empty() { "(opérande)" } else { operands }
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
            "Transfert",
            "Copie la source dans la destination (aucun flag modifié). \
             Note : écrire dans un registre 32 bits (eax) remet à zéro les 32 bits hauts du 64 bits (rax)."
                .to_string(),
            vec![],
        ),
        "movabs" => (
            "Transfert",
            "Charge un immédiat 64 bits complet dans un registre.".to_string(),
            vec![],
        ),
        "lea" => (
            "Adressage",
            "Load Effective Address : calcule une adresse (base + index*échelle + déplacement) \
             et la place dans la destination, SANS accéder à la mémoire. Sert aussi d'arithmétique rapide."
                .to_string(),
            vec![],
        ),
        "push" => (
            "Pile",
            "Décrémente RSP de 8 puis écrit l'opérande au sommet de la pile.".to_string(),
            vec![],
        ),
        "pop" => (
            "Pile",
            "Lit le sommet de la pile dans la destination puis incrémente RSP de 8.".to_string(),
            vec![],
        ),
        "add" => (
            "Arithmétique",
            "Additionne source à destination. Positionne les flags selon le résultat.".to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF", "AF"],
        ),
        "sub" => (
            "Arithmétique",
            "Soustrait source de destination. Positionne les flags selon le résultat.".to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF", "AF"],
        ),
        "imul" => (
            "Arithmétique",
            "Multiplication signée. CF et OF sont mis à 1 si le résultat déborde de la taille de destination."
                .to_string(),
            vec!["CF", "OF"],
        ),
        "mul" => (
            "Arithmétique",
            "Multiplication non signée (RDX:RAX). CF/OF indiquent un débordement dans la partie haute."
                .to_string(),
            vec!["CF", "OF"],
        ),
        "inc" => (
            "Arithmétique",
            "Incrémente de 1. Ne modifie PAS CF (contrairement à add).".to_string(),
            vec!["OF", "SF", "ZF", "PF", "AF"],
        ),
        "dec" => (
            "Arithmétique",
            "Décrémente de 1. Ne modifie PAS CF.".to_string(),
            vec!["OF", "SF", "ZF", "PF", "AF"],
        ),
        "neg" => (
            "Arithmétique",
            "Remplace l'opérande par son opposé (complément à deux).".to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF", "AF"],
        ),
        "cmp" => (
            "Comparaison",
            "Calcule (destination - source) SANS stocker le résultat : seuls les flags sont positionnés. \
             C'est ce qui prépare un saut conditionnel : ZF=1 si égaux, et SF/OF/CF codent l'ordre."
                .to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF", "AF"],
        ),
        "test" => (
            "Comparaison",
            "Calcule (destination AND source) sans le stocker : positionne les flags. \
             `test rax, rax` sert à savoir si rax est nul (ZF=1) ou négatif (SF=1)."
                .to_string(),
            vec!["SF", "ZF", "PF"],
        ),
        "and" => (
            "Logique",
            "ET bit à bit. CF et OF sont mis à 0.".to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF"],
        ),
        "or" => (
            "Logique",
            "OU bit à bit. CF et OF sont mis à 0.".to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF"],
        ),
        "xor" => (
            "Logique",
            "OU exclusif bit à bit. `xor rax, rax` est l'idiome pour mettre rax à 0 \
             (plus court que mov rax, 0). CF et OF sont mis à 0."
                .to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF"],
        ),
        "shl" | "sal" => (
            "Décalage",
            "Décale les bits vers la gauche (multiplie par 2 par bit). Le dernier bit sorti va dans CF."
                .to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF"],
        ),
        "shr" => (
            "Décalage",
            "Décale les bits vers la droite (division non signée par 2). Le dernier bit sorti va dans CF."
                .to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF"],
        ),
        "jmp" => (
            "Saut",
            "Saut inconditionnel : RIP prend la valeur de la cible.".to_string(),
            vec![],
        ),
        "call" => (
            "Appel",
            "Empile l'adresse de retour (RSP -= 8) puis saute vers la fonction cible.".to_string(),
            vec![],
        ),
        "ret" => (
            "Appel",
            "Dépile l'adresse de retour dans RIP (RSP += 8) : revient à l'appelant.".to_string(),
            vec![],
        ),
        "syscall" => (
            "Système",
            "Appel système Linux : RAX = numéro, arguments dans RDI, RSI, RDX, R10, R8, R9. \
             Le noyau exécute l'opération (write, read, exit...) et renvoie le résultat dans RAX."
                .to_string(),
            vec![],
        ),
        "nop" => ("Divers", "Ne fait rien (No Operation).".to_string(), vec![]),
        "leave" => (
            "Pile",
            "Équivaut à `mov rsp, rbp ; pop rbp` : démonte le cadre de pile de la fonction.".to_string(),
            vec![],
        ),
        _ => (
            "Inconnu",
            format!("Instruction « {mnemonic} » : explication non encore répertoriée."),
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

/// Titre lisible d'un saut conditionnel.
fn jcc_title(m: &str) -> &'static str {
    match m {
        "je" | "jz" => "Jump if Equal / Zero",
        "jne" | "jnz" => "Jump if Not Equal / Not Zero",
        "jg" | "jnle" => "Jump if Greater (signé)",
        "jge" | "jnl" => "Jump if Greater or Equal (signé)",
        "jl" | "jnge" => "Jump if Less (signé)",
        "jle" | "jng" => "Jump if Less or Equal (signé)",
        "ja" | "jnbe" => "Jump if Above (non signé)",
        "jae" | "jnb" | "jnc" => "Jump if Above or Equal (non signé)",
        "jb" | "jc" | "jnae" => "Jump if Below (non signé)",
        "jbe" | "jna" => "Jump if Below or Equal (non signé)",
        "js" => "Jump if Sign (négatif)",
        "jns" => "Jump if Not Sign (positif ou nul)",
        "jo" => "Jump if Overflow",
        "jno" => "Jump if Not Overflow",
        "jp" | "jpe" => "Jump if Parity Even",
        "jnp" | "jpo" => "Jump if Parity Odd",
        _ => "Saut conditionnel",
    }
}

/// Évalue un saut conditionnel : renvoie (condition lisible, pris ?, flags pertinents).
/// Renvoie None si `m` n'est pas un saut conditionnel connu.
fn eval_jcc(m: &str, f: Flags) -> Option<(String, bool, Vec<(&'static str, bool)>)> {
    let zf = ("ZF", f.zf);
    let cf = ("CF", f.cf);
    let sf = ("SF", f.sf);
    let of = ("OF", f.of);
    let pf = ("PF", f.pf);

    let out = match m {
        "je" | "jz" => ("ZF = 1".into(), f.zf, vec![zf]),
        "jne" | "jnz" => ("ZF = 0".into(), !f.zf, vec![zf]),
        "jg" | "jnle" => ("ZF = 0 et SF = OF".into(), !f.zf && (f.sf == f.of), vec![zf, sf, of]),
        "jge" | "jnl" => ("SF = OF".into(), f.sf == f.of, vec![sf, of]),
        "jl" | "jnge" => ("SF ≠ OF".into(), f.sf != f.of, vec![sf, of]),
        "jle" | "jng" => ("ZF = 1 ou SF ≠ OF".into(), f.zf || (f.sf != f.of), vec![zf, sf, of]),
        "ja" | "jnbe" => ("CF = 0 et ZF = 0".into(), !f.cf && !f.zf, vec![cf, zf]),
        "jae" | "jnb" | "jnc" => ("CF = 0".into(), !f.cf, vec![cf]),
        "jb" | "jc" | "jnae" => ("CF = 1".into(), f.cf, vec![cf]),
        "jbe" | "jna" => ("CF = 1 ou ZF = 1".into(), f.cf || f.zf, vec![cf, zf]),
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
    fn jl_taken_when_sf_ne_of() {
        // Après cmp 5, 8 : SF=1, OF=0 => SF ≠ OF => jl pris.
        let f = Flags { sf: true, of: false, ..Default::default() };
        let e = explain("jl", "erreur", f);
        assert_eq!(e.taken, Some(true));
        assert_eq!(e.condition.as_deref(), Some("SF ≠ OF"));
    }

    #[test]
    fn je_not_taken_when_zf_zero() {
        let f = Flags { zf: false, ..Default::default() };
        let e = explain("je", "cible", f);
        assert_eq!(e.taken, Some(false));
    }

    #[test]
    fn cmp_lists_affected_flags() {
        let e = explain("cmp", "rax, rbx", Flags::default());
        assert!(e.affects_flags.contains(&"ZF"));
        assert!(e.taken.is_none());
    }
}
