//! Ce qu'une instruction lit, écrit et modifie — y compris implicitement.
//!
//! Le désassemblage montre les opérandes ÉCRITS dans l'instruction. Il ne dit
//! pas que `push` modifie RSP, que `syscall` écrase RCX et R11, ou que `div`
//! lit RDX autant que RAX. Ce sont précisément ces effets invisibles qui font
//! échouer les premiers programmes.
//!
//! Ce module les rend explicites, en séparant ce que l'élève voit dans le texte
//! de l'instruction de ce que le processeur fait en plus.

use crate::i18n::{self, Lang};

/// Une ressource touchée par une instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resource {
    /// Registre nommé, tel qu'écrit dans l'instruction ou impliqué par elle.
    Register(String),
    /// Accès mémoire, avec l'expression d'adresse telle qu'écrite.
    Memory(String),
    /// Le pointeur d'instruction.
    Rip,
    /// Un ou plusieurs drapeaux.
    Flags(Vec<&'static str>),
}

impl Resource {
    pub fn label(&self, lang: Lang) -> String {
        match self {
            Resource::Register(r) => r.to_uppercase(),
            Resource::Memory(a) => format!(
                "{} {a}",
                i18n::tr3(lang, "mémoire", "memory", "memoria")
            ),
            Resource::Rip => "RIP".to_string(),
            Resource::Flags(f) => f.join(" "),
        }
    }
}

/// Bilan des effets d'une instruction.
#[derive(Debug, Clone, Default)]
pub struct Effects {
    /// Ressources lues.
    pub reads: Vec<Resource>,
    /// Ressources écrites.
    pub writes: Vec<Resource>,
    /// Effets que le texte de l'instruction ne montre pas.
    ///
    /// Chaque entrée est une phrase : c'est là que se trouve l'essentiel de la
    /// valeur pédagogique.
    pub implicit: Vec<String>,
}

/// Extrait les opérandes séparés par des virgules, hors crochets.
fn split_operands(operands: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in operands.chars() {
        match c {
            '[' => {
                depth += 1;
                cur.push(c);
            }
            ']' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Classe un opérande : registre, mémoire, ou valeur littérale (ignorée).
fn classify(op: &str) -> Option<Resource> {
    let t = op.trim();
    if t.is_empty() {
        return None;
    }
    if t.contains('[') {
        return Some(Resource::Memory(t.to_string()));
    }
    // Un registre commence par une lettre ; un littéral par un chiffre, un
    // signe, ou est une étiquette de saut (traitée à part).
    let first = t.chars().next()?;
    if first.is_ascii_alphabetic() && !t.contains(' ') {
        return Some(Resource::Register(t.to_string()));
    }
    None
}

/// Instructions dont le premier opérande est LU et non écrit.
fn first_operand_is_read_only(m: &str) -> bool {
    matches!(m, "cmp" | "test" | "push" | "jmp" | "call" | "bt")
}

/// Analyse les effets d'une instruction.
///
/// `affects_flags` vient de [`crate::explain`] : on le reprend plutôt que de le
/// dupliquer, pour que les deux panneaux ne puissent pas se contredire.
pub fn analyse(mnemonic: &str, operands: &str, affects_flags: &[&'static str], lang: Lang) -> Effects {
    let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
    let m = mnemonic.to_lowercase();
    let ops = split_operands(operands);
    let mut e = Effects::default();

    // ---- Opérandes explicites ----
    if let Some(first) = ops.first().and_then(|o| classify(o)) {
        if first_operand_is_read_only(&m) {
            e.reads.push(first);
        } else {
            e.writes.push(first.clone());
            // Une opération de lecture-modification-écriture relit sa
            // destination : add, sub, and, shl… mais pas mov ni lea.
            if !matches!(m.as_str(), "mov" | "movabs" | "movzx" | "movsx" | "movsxd" | "lea" | "pop" | "setz")
                && !m.starts_with("set")
                && !m.starts_with("cmov")
            {
                e.reads.push(first);
            }
        }
    }
    for op in ops.iter().skip(1) {
        if let Some(r) = classify(op) {
            // `lea` calcule une adresse sans lire la mémoire : c'est toute sa
            // raison d'être, et la confusion est fréquente.
            if m == "lea" && matches!(r, Resource::Memory(_)) {
                continue;
            }
            e.reads.push(r);
        }
    }

    // ---- Effets implicites ----
    match m.as_str() {
        "push" | "pushf" | "pushfq" => {
            e.writes.push(Resource::Register("rsp".into()));
            e.reads.push(Resource::Register("rsp".into()));
            e.writes.push(Resource::Memory("[rsp-8]".into()));
            e.implicit.push(t(
                "RSP diminue de 8, puis la valeur est écrite au nouveau sommet de pile.",
                "RSP decreases by 8, then the value is written at the new stack top.",
                "RSP disminuye 8, luego el valor se escribe en la nueva cima de la pila.",
            ).into());
        }
        "pop" | "popf" | "popfq" => {
            e.writes.push(Resource::Register("rsp".into()));
            e.reads.push(Resource::Register("rsp".into()));
            e.reads.push(Resource::Memory("[rsp]".into()));
            e.implicit.push(t(
                "La valeur est lue au sommet de pile, puis RSP augmente de 8.",
                "The value is read at the stack top, then RSP increases by 8.",
                "El valor se lee en la cima de la pila, luego RSP aumenta 8.",
            ).into());
        }
        "call" => {
            e.writes.push(Resource::Rip);
            e.writes.push(Resource::Register("rsp".into()));
            e.writes.push(Resource::Memory("[rsp-8]".into()));
            e.implicit.push(t(
                "L'adresse de l'instruction SUIVANTE est empilée : c'est elle que « ret » ira rechercher.",
                "The address of the NEXT instruction is pushed: that is what \"ret\" will look for.",
                "Se apila la dirección de la instrucción SIGUIENTE: es lo que «ret» buscará.",
            ).into());
        }
        "ret" => {
            e.writes.push(Resource::Rip);
            e.writes.push(Resource::Register("rsp".into()));
            e.reads.push(Resource::Memory("[rsp]".into()));
            e.implicit.push(t(
                "RIP prend la valeur trouvée au sommet de pile. Si les push et les pop ne s'équilibrent pas, c'est une donnée qui est dépilée.",
                "RIP takes the value found at the stack top. If pushes and pops do not balance, a piece of data is popped instead.",
                "RIP toma el valor de la cima de la pila. Si push y pop no se equilibran, se desapila un dato.",
            ).into());
        }
        "leave" => {
            e.writes.push(Resource::Register("rsp".into()));
            e.writes.push(Resource::Register("rbp".into()));
            e.implicit.push(t(
                "Équivaut à « mov rsp, rbp » puis « pop rbp » : démonte le cadre de pile.",
                "Equivalent to \"mov rsp, rbp\" then \"pop rbp\": tears down the stack frame.",
                "Equivale a «mov rsp, rbp» y «pop rbp»: desmonta el marco de pila.",
            ).into());
        }
        "syscall" => {
            e.reads.push(Resource::Register("rax".into()));
            e.writes.push(Resource::Register("rax".into()));
            e.writes.push(Resource::Register("rcx".into()));
            e.writes.push(Resource::Register("r11".into()));
            e.implicit.push(t(
                "RAX porte le numéro d'appel et reçoit le résultat. Les arguments sont dans RDI, RSI, RDX, R10, R8, R9.",
                "RAX carries the call number and receives the result. Arguments are in RDI, RSI, RDX, R10, R8, R9.",
                "RAX lleva el número de llamada y recibe el resultado. Los argumentos van en RDI, RSI, RDX, R10, R8, R9.",
            ).into());
            e.implicit.push(t(
                "RCX et R11 sont ÉCRASÉS par l'instruction : le processeur y range l'adresse de retour et les flags.",
                "RCX and R11 are CLOBBERED by the instruction: the CPU stores the return address and flags there.",
                "RCX y R11 son SOBRESCRITOS: la CPU guarda ahí la dirección de retorno y los flags.",
            ).into());
        }
        "div" | "idiv" => {
            e.reads.push(Resource::Register("rax".into()));
            e.reads.push(Resource::Register("rdx".into()));
            e.writes.push(Resource::Register("rax".into()));
            e.writes.push(Resource::Register("rdx".into()));
            e.implicit.push(t(
                "Le dividende est le couple RDX:RAX sur 128 bits — pas RAX seul. Quotient dans RAX, reste dans RDX.",
                "The dividend is the 128-bit pair RDX:RAX — not RAX alone. Quotient in RAX, remainder in RDX.",
                "El dividendo es el par RDX:RAX de 128 bits — no RAX solo. Cociente en RAX, resto en RDX.",
            ).into());
            e.implicit.push(if m == "div" {
                t(
                    "Préparez RDX avec « xor rdx, rdx » : un reste de calcul y ferait déborder le quotient.",
                    "Prepare RDX with \"xor rdx, rdx\": leftover data there would overflow the quotient.",
                    "Prepare RDX con «xor rdx, rdx»: un residuo desbordaría el cociente.",
                ).into()
            } else {
                t(
                    "Préparez RDX avec « cqo », qui étend le signe de RAX — « xor rdx, rdx » serait faux pour un dividende négatif.",
                    "Prepare RDX with \"cqo\", which sign-extends RAX — \"xor rdx, rdx\" would be wrong for a negative dividend.",
                    "Prepare RDX con «cqo», que extiende el signo de RAX — «xor rdx, rdx» sería incorrecto con dividendo negativo.",
                ).into()
            });
        }
        "mul" | "imul" if ops.len() <= 1 => {
            e.reads.push(Resource::Register("rax".into()));
            e.writes.push(Resource::Register("rax".into()));
            e.writes.push(Resource::Register("rdx".into()));
            e.implicit.push(t(
                "Forme à un opérande : RAX est multiplicande implicite, et le résultat sur 128 bits va dans RDX:RAX.",
                "One-operand form: RAX is the implicit multiplicand, and the 128-bit result goes to RDX:RAX.",
                "Forma de un operando: RAX es multiplicando implícito y el resultado de 128 bits va a RDX:RAX.",
            ).into());
        }
        "loop" | "loope" | "loopz" | "loopne" | "loopnz" => {
            e.reads.push(Resource::Register("rcx".into()));
            e.writes.push(Resource::Register("rcx".into()));
            e.writes.push(Resource::Rip);
            e.implicit.push(t(
                "RCX est décrémenté AVANT le test : la boucle s'arrête quand il atteint zéro.",
                "RCX is decremented BEFORE the test: the loop stops when it reaches zero.",
                "RCX se decrementa ANTES de la prueba: el bucle para al llegar a cero.",
            ).into());
        }
        "cqo" | "cdq" | "cwd" => {
            e.reads.push(Resource::Register("rax".into()));
            e.writes.push(Resource::Register("rdx".into()));
            e.implicit.push(t(
                "Recopie le bit de signe de RAX dans tout RDX, pour préparer une division signée.",
                "Replicates RAX's sign bit across RDX, to prepare a signed division.",
                "Replica el bit de signo de RAX en todo RDX, para preparar una división con signo.",
            ).into());
        }
        _ => {}
    }

    // Opérations de chaîne : RSI/RDI/RCX bougent tout seuls.
    let base = m.trim_end_matches(['b', 'w', 'd', 'q']);
    if matches!(base, "movs" | "stos" | "lods" | "scas" | "cmps") {
        e.implicit.push(t(
            "RSI et/ou RDI avancent automatiquement après chaque élément, dans le sens fixé par DF (cld/std).",
            "RSI and/or RDI advance automatically after each element, in the direction set by DF (cld/std).",
            "RSI y/o RDI avanzan automáticamente tras cada elemento, según DF (cld/std).",
        ).into());
    }

    // Sauts : RIP est la destination, même si l'instruction ne le nomme pas.
    if m.starts_with('j') || m == "jmp" {
        e.writes.push(Resource::Rip);
        if m != "jmp" {
            e.implicit.push(t(
                "Le saut n'a lieu que si la condition est vraie ; sinon l'exécution continue à l'instruction suivante.",
                "The jump only happens if the condition holds; otherwise execution continues at the next instruction.",
                "El salto solo ocurre si la condición se cumple; si no, la ejecución sigue en la instrucción siguiente.",
            ).into());
        }
    }

    // Drapeaux, repris de l'explication pour rester cohérents.
    if !affects_flags.is_empty() {
        e.writes.push(Resource::Flags(affects_flags.to_vec()));
    }

    dedup(&mut e.reads);
    dedup(&mut e.writes);
    e
}

/// Retire les doublons en conservant l'ordre d'apparition.
fn dedup(v: &mut Vec<Resource>) {
    let mut seen: Vec<Resource> = Vec::new();
    v.retain(|r| {
        if seen.contains(r) {
            false
        } else {
            seen.push(r.clone());
            true
        }
    });
}

/// Instructions apparentées, à connaître en même temps que celle-ci.
pub fn related(mnemonic: &str) -> &'static [&'static str] {
    match mnemonic.to_lowercase().as_str() {
        "mov" => &["movzx", "movsx", "lea", "xchg"],
        "movzx" | "movsx" | "movsxd" => &["mov", "cqo", "cdqe"],
        "lea" => &["mov", "add"],
        "add" => &["sub", "adc", "inc", "lea"],
        "sub" => &["add", "sbb", "dec", "cmp"],
        "cmp" => &["sub", "test", "sete", "jе"],
        "test" => &["and", "cmp", "setz"],
        "and" => &["or", "xor", "not", "test"],
        "or" | "xor" | "not" => &["and", "test"],
        "shl" | "sal" => &["shr", "sar", "rol", "imul"],
        "shr" => &["sar", "shl", "div"],
        "sar" => &["shr", "shl", "idiv"],
        "mul" => &["imul", "div", "shl"],
        "imul" => &["mul", "idiv", "shl"],
        "div" => &["idiv", "cqo", "mul"],
        "idiv" => &["div", "cqo", "imul"],
        "push" => &["pop", "call", "sub"],
        "pop" => &["push", "ret", "leave"],
        "call" => &["ret", "push", "jmp"],
        "ret" => &["call", "pop", "leave"],
        "leave" => &["enter", "pop", "mov"],
        "jmp" => &["call", "je", "loop"],
        "syscall" => &["mov", "int3"],
        "loop" => &["dec", "jnz", "cmp"],
        m if m.starts_with("set") => &["cmp", "test", "cmovе"],
        m if m.starts_with("cmov") => &["cmp", "mov", "sete"],
        m if m.starts_with('j') => &["cmp", "test", "jmp"],
        _ => &[],
    }
}

/// Note de convention d'appel, si l'instruction touche l'ABI.
pub fn abi_note(mnemonic: &str, operands: &str, lang: Lang) -> Option<String> {
    let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
    let m = mnemonic.to_lowercase();
    if m == "call" {
        return Some(t(
            "Après cet appel, RAX portera la valeur de retour. RBX et R12–R15 seront intacts ; RCX, RDX, RSI, RDI et R8–R11 peuvent avoir été écrasés.",
            "After this call, RAX will hold the return value. RBX and R12–R15 will be intact; RCX, RDX, RSI, RDI and R8–R11 may have been clobbered.",
            "Tras esta llamada, RAX llevará el valor de retorno. RBX y R12–R15 quedarán intactos; RCX, RDX, RSI, RDI y R8–R11 pueden haberse sobrescrito.",
        ).into());
    }
    if m == "ret" {
        return Some(t(
            "La valeur de retour doit être dans RAX. RBX et R12–R15 doivent avoir retrouvé la valeur qu'ils avaient à l'entrée.",
            "The return value must be in RAX. RBX and R12–R15 must hold the value they had on entry.",
            "El valor de retorno debe estar en RAX. RBX y R12–R15 deben tener el valor que tenían al entrar.",
        ).into());
    }
    // Écriture dans un registre porteur d'argument ou sauvé par l'appelé.
    let first = split_operands(operands).first().cloned().unwrap_or_default().to_uppercase();
    let role = crate::abi::role(&first);
    match role {
        crate::abi::Role::Argument(n) if !first_operand_is_read_only(&m) => Some(format!(
            "{first} {} {} {}.",
            t("porte l'argument", "carries argument", "lleva el argumento"),
            n + 1,
            t("d'un appel de fonction", "of a function call", "de una llamada a función"),
        )),
        crate::abi::Role::CalleeSaved if !first_operand_is_read_only(&m) => Some(format!(
            "{first} {}",
            t(
                "est sauvé par l'appelé : si vous l'écrasez dans une fonction, restaurez-le avant le ret.",
                "is callee-saved: if you overwrite it inside a function, restore it before the ret.",
                "es guardado por el llamado: si lo sobrescribe en una función, restáurelo antes del ret.",
            ),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_reg(v: &[Resource], name: &str) -> bool {
        v.iter().any(|r| matches!(r, Resource::Register(x) if x.eq_ignore_ascii_case(name)))
    }
    fn has_rip(v: &[Resource]) -> bool {
        v.iter().any(|r| *r == Resource::Rip)
    }

    #[test]
    fn operands_are_split_outside_brackets() {
        assert_eq!(split_operands("rax, rbx"), vec!["rax", "rbx"]);
        assert_eq!(split_operands("rax, [rbx + rcx*8]"), vec!["rax", "[rbx + rcx*8]"]);
        assert_eq!(split_operands(""), Vec::<String>::new());
        assert_eq!(split_operands("rax"), vec!["rax"]);
    }

    /// `mov` écrit sa destination sans la lire ; `add` la relit.
    #[test]
    fn read_modify_write_is_distinguished_from_plain_write() {
        let mov = analyse("mov", "rax, rbx", &[], Lang::Fr);
        assert!(has_reg(&mov.writes, "rax"));
        assert!(!has_reg(&mov.reads, "rax"), "mov ne relit pas sa destination");
        assert!(has_reg(&mov.reads, "rbx"));

        let add = analyse("add", "rax, rbx", &["ZF"], Lang::Fr);
        assert!(has_reg(&add.writes, "rax"));
        assert!(has_reg(&add.reads, "rax"), "add relit sa destination");
    }

    /// `cmp` n'écrit AUCUN registre : c'est tout son intérêt.
    #[test]
    fn cmp_writes_no_register() {
        let e = analyse("cmp", "rax, rbx", &["ZF", "SF", "OF", "CF"], Lang::Fr);
        assert!(has_reg(&e.reads, "rax") && has_reg(&e.reads, "rbx"));
        assert!(!has_reg(&e.writes, "rax"), "cmp ne modifie pas rax");
        assert!(
            e.writes.iter().any(|r| matches!(r, Resource::Flags(_))),
            "mais il positionne les flags"
        );
    }

    /// `lea` calcule une adresse SANS lire la mémoire — confusion classique.
    #[test]
    fn lea_does_not_read_memory() {
        let e = analyse("lea", "rdi, [rbx + 8]", &[], Lang::Fr);
        assert!(has_reg(&e.writes, "rdi"));
        assert!(
            !e.reads.iter().any(|r| matches!(r, Resource::Memory(_))),
            "lea ne déréférence pas : {:?}",
            e.reads
        );
    }

    /// Les effets invisibles de push/pop/call/ret sur RSP et la pile.
    #[test]
    fn stack_instructions_declare_their_hidden_effects() {
        let push = analyse("push", "rax", &[], Lang::Fr);
        assert!(has_reg(&push.writes, "rsp"), "push modifie RSP");
        assert!(has_reg(&push.reads, "rax"), "et lit la valeur poussée");
        assert!(!push.implicit.is_empty());

        let call = analyse("call", "0x401000", &[], Lang::Fr);
        assert!(has_rip(&call.writes) && has_reg(&call.writes, "rsp"));
        assert!(call.implicit.iter().any(|s| s.contains("ret")), "doit relier call et ret");

        let ret = analyse("ret", "", &[], Lang::Fr);
        assert!(has_rip(&ret.writes));
        assert!(ret.implicit.iter().any(|s| s.contains("push")), "doit prévenir du déséquilibre");
    }

    /// `syscall` écrase RCX et R11 — invisible dans le texte de l'instruction.
    #[test]
    fn syscall_declares_its_clobbers() {
        let e = analyse("syscall", "", &[], Lang::Fr);
        assert!(has_reg(&e.writes, "rcx"), "RCX est écrasé");
        assert!(has_reg(&e.writes, "r11"), "R11 est écrasé");
        assert!(e.implicit.iter().any(|s| s.contains("R10")), "doit signaler R10 : {:?}", e.implicit);
    }

    /// `div` lit RDX autant que RAX, et le conseil diffère de `idiv`.
    #[test]
    fn division_declares_the_rdx_rax_pair() {
        let d = analyse("div", "rcx", &[], Lang::Fr);
        assert!(has_reg(&d.reads, "rdx") && has_reg(&d.reads, "rax"));
        assert!(has_reg(&d.writes, "rdx") && has_reg(&d.writes, "rax"));
        assert!(d.implicit.iter().any(|s| s.contains("xor rdx")), "{:?}", d.implicit);

        let i = analyse("idiv", "rcx", &[], Lang::Fr);
        assert!(i.implicit.iter().any(|s| s.contains("cqo")), "{:?}", i.implicit);
    }

    #[test]
    fn conditional_jumps_write_rip() {
        let e = analyse("je", "0x401000", &[], Lang::Fr);
        assert!(has_rip(&e.writes));
        assert!(!e.implicit.is_empty(), "doit rappeler que le saut est conditionnel");

        let j = analyse("jmp", "0x401000", &[], Lang::Fr);
        assert!(has_rip(&j.writes));
    }

    #[test]
    fn duplicates_are_removed_but_order_kept() {
        let e = analyse("add", "rax, rax", &[], Lang::Fr);
        let n = e.reads.iter().filter(|r| matches!(r, Resource::Register(x) if x == "rax")).count();
        assert_eq!(n, 1, "rax ne doit apparaître qu'une fois en lecture");
    }

    #[test]
    fn related_instructions_are_offered_for_the_common_ones() {
        for m in ["mov", "add", "cmp", "push", "call", "div", "shl", "je", "sete", "cmove"] {
            assert!(!related(m).is_empty(), "« {m} » sans instruction apparentée");
        }
        assert!(related("vfmadd231pd").is_empty(), "inconnu → aucune suggestion");
    }

    /// La note ABI doit apparaître là où elle compte, et se taire ailleurs.
    #[test]
    fn abi_note_appears_only_where_relevant() {
        assert!(abi_note("call", "func", Lang::Fr).is_some());
        assert!(abi_note("ret", "", Lang::Fr).is_some());
        // Écrire dans RDI, c'est préparer un argument.
        let n = abi_note("mov", "rdi, 1", Lang::Fr).expect("RDI porte un argument");
        assert!(n.contains("RDI") && n.contains("1"));
        // Écrire dans RBX engage à le restaurer.
        assert!(abi_note("mov", "rbx, 5", Lang::Fr).unwrap().contains("sauvé"));
        // Lire RDI dans un cmp n'engage à rien.
        assert!(abi_note("cmp", "rdi, 0", Lang::Fr).is_none());
        // R10 n'est ni argument de fonction ni sauvé par l'appelé.
        assert!(abi_note("mov", "r10, 1", Lang::Fr).is_none());
    }

    #[test]
    fn everything_is_translated() {
        for lang in [Lang::Fr, Lang::En, Lang::Es] {
            for (m, o) in [("syscall", ""), ("push", "rax"), ("div", "rcx"), ("ret", "")] {
                let e = analyse(m, o, &["ZF"], lang);
                for s in &e.implicit {
                    assert!(s.len() > 20, "« {m} » : texte trop court en {lang:?}");
                }
                for r in e.reads.iter().chain(&e.writes) {
                    assert!(!r.label(lang).is_empty());
                }
            }
        }
    }
}
