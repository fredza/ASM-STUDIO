//! Vérification statique de l'alignement de la pile avant chaque `call`.
//!
//! Sur x86-64, les deux ABI (System V sous Linux, Win64 sous Windows) posent la
//! même règle : **juste avant un `call`, RSP doit être un multiple de 16**. La
//! fonction appelée y compte pour ranger ses `movdqa`/`movaps`, et le code des
//! bibliothèques système en use largement.
//!
//! L'élève qui l'oublie n'obtient pas un message d'erreur : il obtient un
//! plantage à l'intérieur d'une DLL ou de la libc, à une adresse qui ne désigne
//! aucune de ses lignes, parfois intermittent. Le piège classique tient en trois
//! instructions :
//!
//! ```text
//! main:
//!     sub  rsp, 40      ; RSP redevient multiple de 16 → l'appel est correct
//!     call MessageBoxA
//!     add  rsp, 40      ; ...et l'alignement est défait
//!     call ExitProcess  ; désaligné : plantage incompréhensible
//! ```
//!
//! Ce module attrape ce motif **à l'assemblage**, sur le désassemblage du
//! binaire produit, et ajoute un avertissement au journal de build. Il ne
//! bloque rien : c'est un avertissement de compilateur, pas une erreur.
//!
//! # Pourquoi un module séparé de [`crate::diagnostic`]
//!
//! [`crate::diagnostic`] explique une faute **déjà survenue** : il part d'un
//! signal, d'une carte mémoire et de registres capturés par le débogueur. Ici
//! rien n'a encore tourné — il n'y a qu'un fichier objet et son désassemblage.
//! Les entrées, les dépendances (aucune sur `debugger`) et le moment d'appel
//! (dans le pipeline d'assemblage, pas dans la boucle de débogage) diffèrent
//! entièrement ; le seul point commun est le ton des messages.
//!
//! # Portée volontairement étroite
//!
//! L'analyse est **linéaire** : elle suit le fil des instructions depuis le
//! point d'entrée et abandonne — sans rien dire — au premier saut, à la
//! première boucle, au premier `ret` ou à la première écriture de RSP qu'elle
//! ne sait pas chiffrer. Une vraie analyse de flot de contrôle serait hors de
//! portée ici, et surtout : le moindre faux positif détruirait la confiance de
//! l'élève dans l'avertissement. Le cas linéaire suffit à couvrir le piège réel.
//!
//! Second garde-fou, tout aussi délibéré : seuls les **appels qui sortent du
//! programme** sont signalés — ceux qui passent par un talon d'import (`jmp
//! qword [rip+…]`, ce que fabriquent aussi bien [`crate::pe_link`] que la PLT
//! d'un ELF lié dynamiquement) ou directement par une entrée d'IAT/GOT. C'est
//! là que la règle mord : le code système est plein d'instructions SSE alignées.
//! Une routine que l'élève a écrite lui-même, elle, se moque de l'alignement —
//! et les exemples livrés avec l'IDE l'appellent couramment après un `push`
//! (voir `pile_demo.asm`, `asmstd-check.asm`). Crier au loup sur ces
//! programmes-là, qui marchent, ferait ignorer l'avertissement le jour où il
//! porte vraiment.

use crate::assemble::Target;
use crate::disasm::Insn;
use crate::i18n::{self, Lang};

/// Décalage de RSP modulo 16 au tout premier instant du programme.
///
/// Les deux mondes ne partent pas du même pied, et c'est mesuré, pas déduit :
///
/// * ELF Linux — le noyau saute à `_start` **sans empiler d'adresse de retour** :
///   RSP pointe sur `argc` et l'ABI System V le garantit multiple de 16. Décalage 0.
/// * PE Windows — le chargeur *appelle* le point d'entrée : l'adresse de retour
///   est sur la pile, RSP vaut donc 8 de plus qu'un multiple de 16. Décalage 8.
///   C'est ce qui explique le `sub rsp, 40` (32 d'espace d'ombre + 8) partout
///   dans les exemples Windows : 40 n'est pas un multiple de 16, et c'est exprès.
fn entry_phase(target: Target) -> i64 {
    if target.is_windows() { 8 } else { 0 }
}

/// Un `call` atteint avec une pile désalignée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Misaligned {
    /// Adresse virtuelle de l'instruction `call` fautive.
    pub address: u64,
    /// Ce que le `call` visait, tel que le désassembleur l'écrit.
    pub target: String,
    /// De combien RSP dépasse le multiple de 16 (toujours entre 1 et 15).
    pub excess: i64,
}

/// Suit le fil linéaire des instructions depuis `entry` et rend le premier
/// `call` atteint avec RSP non multiple de 16.
///
/// * `phase` — reste de RSP modulo 16 à l'entrée (voir [`entry_phase`]) ;
/// * `leaves_program` — reçoit les opérandes d'un `call` désaligné et dit s'il
///   sort du programme ; seuls ceux-là valent un avertissement. Un appel
///   désaligné vers une routine locale est laissé passer, et l'analyse
///   continue : le piège peut se trouver plus loin.
///
/// Rend `None` dès que le chemin cesse d'être linéaire : c'est le choix
/// délibéré de ne jamais deviner.
pub fn first_misaligned_call(
    insns: &[Insn],
    entry: u64,
    phase: i64,
    mut leaves_program: impl FnMut(&str) -> bool,
) -> Option<Misaligned> {
    let start = insns.iter().position(|i| i.address == entry)?;
    // Delta cumulé appliqué à RSP depuis l'entrée (négatif quand la pile descend).
    let mut delta: i64 = 0;

    for insn in &insns[start..] {
        let m = insn.mnemonic.to_ascii_lowercase();
        let ops = insn.operands.trim();

        // Un préfixe 0x66 fait d'un `push`/`pop` une affaire de 2 octets, et de
        // bien d'autres instructions autre chose que ce que leur mnémonique
        // laisse croire. Trop rare pour valoir un cas particulier : on s'arrête.
        if insn.bytes.first() == Some(&0x66) {
            return None;
        }

        match m.as_str() {
            "call" => {
                let excess = (phase + delta).rem_euclid(16);
                if excess != 0 && leaves_program(ops) {
                    return Some(Misaligned {
                        address: insn.address,
                        target: ops.to_string(),
                        excess,
                    });
                }
                // Un appel qui revient rend la pile telle qu'il l'a trouvée :
                // le delta ne bouge pas. Un appel qui ne revient pas non plus,
                // puisque plus rien de ce qui suit ne s'exécutera.
            }
            // `push`/`pop` 64 bits : 8 octets, sans avoir à lire l'opérande.
            "push" | "pushf" | "pushfq" => delta -= 8,
            "pop" | "popf" | "popfq" => delta += 8,
            "add" | "sub" => match rsp_immediate(ops) {
                Some(n) if m == "sub" => delta -= n,
                Some(n) => delta += n,
                // `sub rsp, rax` : la valeur n'est pas dans le code. Abandon.
                None if writes_rsp(ops) => return None,
                None => {}
            },
            // Tout ce qui rompt le fil : sauts (`jmp`, `je`, `jne`…), boucles,
            // retours, et les manipulations de pile qu'on ne chiffre pas.
            _ if breaks_linear_flow(&m) => return None,
            _ if writes_rsp(ops) => return None,
            _ => {}
        }
    }
    None
}

/// L'instruction interrompt-elle le fil linéaire ?
///
/// Au moindre doute on répond oui : ne rien dire coûte moins cher qu'un
/// avertissement à côté de la plaque.
fn breaks_linear_flow(mnemonic: &str) -> bool {
    // Tous les sauts conditionnels commencent par `j`, comme `jmp` — et aucune
    // autre instruction x86-64 ne commence par `j`.
    mnemonic.starts_with('j')
        || mnemonic.starts_with("loop")
        || mnemonic.starts_with("ret")
        || mnemonic.starts_with("iret")
        || mnemonic.starts_with("int")
        || matches!(mnemonic, "leave" | "enter" | "hlt" | "ud2" | "xchg" | "pusha" | "popa")
}

/// L'instruction écrit-elle dans RSP ? On regarde uniquement l'opérande de
/// destination : `mov qword [rsp + 32], 0` écrit *par* RSP, pas *dans* RSP.
fn writes_rsp(operands: &str) -> bool {
    let dest = operands.split(',').next().unwrap_or("").trim();
    dest.eq_ignore_ascii_case("rsp") || dest.eq_ignore_ascii_case("esp") || dest.eq_ignore_ascii_case("sp")
}

/// Extrait `N` de `rsp, N` quand `N` est un littéral. Rend `None` pour tout le
/// reste — autre destination, ou opérande qui n'est pas une constante écrite.
fn rsp_immediate(operands: &str) -> Option<i64> {
    let (dest, src) = operands.split_once(',')?;
    if !dest.trim().eq_ignore_ascii_case("rsp") {
        return None;
    }
    let src = src.trim();
    let (neg, digits) = match src.strip_prefix('-') {
        Some(rest) => (true, rest.trim()),
        None => (false, src),
    };
    // Capstone écrit les immédiats en hexa (`0x28`) ; le décimal reste possible.
    let value = match digits.strip_prefix("0x").or_else(|| digits.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16).ok()?,
        None => digits.parse::<u64>().ok()?,
    };
    // `as i64` sur un immédiat 64 bits rend bien le nombre signé qu'il code.
    let value = value as i64;
    Some(if neg { value.checked_neg()? } else { value })
}

/// L'opérande désigne-t-il un accès mémoire relatif à RIP ?
///
/// Capstone écrit ces opérandes `qword ptr [rip + 0x1034]` : c'est la forme
/// d'une entrée d'IAT (Windows) ou de GOT (Linux), donc d'une adresse que le
/// chargeur, et non l'assembleur, a remplie.
fn is_rip_relative_memory(operands: &str) -> bool {
    operands.contains("[rip")
}

/// Adresse absolue écrite en clair dans l'opérande d'un `call`/`jmp` direct.
fn literal_address(operands: &str) -> Option<u64> {
    let s = operands.trim();
    let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
    u64::from_str_radix(hex, 16).ok()
}

/// Cet appel quitte-t-il le code de l'élève pour celui du système ?
///
/// Deux formes, et deux seulement :
///
/// * `call qword [rip+…]` — l'appel lit son adresse dans l'IAT/GOT ; c'est ce
///   qu'écrit `call [rel __imp_ExitProcess]` ;
/// * `call 0x…` où l'adresse visée porte un talon `jmp qword [rip+…]` —
///   la forme habituelle, celle que [`crate::pe_link`] fabrique pour qu'un
///   `call ExitProcess` écrit naïvement atteigne l'IAT, et celle de la PLT
///   d'un ELF lié dynamiquement.
///
/// Tout le reste — `call routine`, `call rax`, `call [rsi]` — reste du code de
/// l'élève, où l'alignement n'a aucune conséquence observable.
fn leaves_the_program(binary: &std::path::Path, operands: &str) -> bool {
    if is_rip_relative_memory(operands) {
        return true;
    }
    let Some(target) = literal_address(operands) else {
        return false;
    };
    // Deux instructions suffisent : un talon protégé par CET commence par
    // `endbr64`, le saut vient juste derrière.
    let stub = crate::disasm::disassemble_at(binary, target, 2);
    let jump = match stub.first() {
        Some(i) if i.mnemonic.eq_ignore_ascii_case("endbr64") => stub.get(1),
        other => other,
    };
    jump.is_some_and(|i| {
        let m = i.mnemonic.to_ascii_lowercase();
        (m == "jmp" || m == "bnd jmp") && is_rip_relative_memory(&i.operands)
    })
}

/// Analyse le binaire produit et rend l'avertissement à ajouter au journal,
/// ou `None` s'il n'y a rien à signaler.
///
/// Tout échec de lecture ou de désassemblage rend `None` : un binaire qu'on
/// n'arrive pas à relire n'est pas une raison de déranger l'élève, et surtout
/// pas de faire échouer un assemblage qui, lui, a réussi.
pub fn warn_for_binary(
    binary: &std::path::Path,
    listing: &std::path::Path,
    target: Target,
    lang: Lang,
) -> Option<String> {
    let insns = crate::disasm::disassemble_text(binary).ok()?;
    let entry = crate::disasm::entry_address(binary)?;
    let bad = first_misaligned_call(&insns, entry, entry_phase(target), |ops| {
        leaves_the_program(binary, ops)
    })?;

    let base = crate::disasm::section_address(binary, ".text");
    let line = base
        .map(|b| crate::srcmap::parse(listing, b))
        .unwrap_or_default()
        .get(&bad.address)
        .copied();

    Some(render(&bad, line, lang))
}

/// Met en mots l'avertissement, dans la langue de l'interface.
fn render(bad: &Misaligned, line: Option<usize>, lang: Lang) -> String {
    let where_ = match line {
        Some(n) => match lang {
            Lang::Fr => format!("ligne {n}"),
            Lang::En => format!("line {n}"),
            Lang::Es => format!("línea {n}"),
        },
        None => format!("0x{:X}", bad.address),
    };
    let call = if bad.target.is_empty() {
        "call".to_string()
    } else {
        format!("call {}", bad.target)
    };

    let head = i18n::tr3(
        lang,
        "Attention — pile désalignée avant un appel",
        "Warning — stack misaligned before a call",
        "Atención — pila desalineada antes de una llamada",
    );
    let body = match lang {
        Lang::Fr => format!(
            "  Sur x86-64, RSP doit être un multiple de 16 juste avant chaque « call » :\n\
             \x20 c'est la règle des deux ABI, Linux comme Windows. Ici RSP vaut {} de trop.\n\
             \x20 Le programme s'assemble et démarre quand même, mais il plantera à l'intérieur\n\
             \x20 de la fonction appelée : l'adresse de l'erreur ne désignera aucune de vos\n\
             \x20 lignes, le message ne parlera pas d'alignement, et le plantage peut même\n\
             \x20 n'arriver qu'une fois sur deux. Impossible à deviner depuis le symptôme.\n\
             \x20 Piste : additionnez tout ce qui bouge RSP entre le début et cet appel\n\
             \x20 (« push », « pop », « sub rsp, N », « add rsp, N »). En particulier, ne\n\
             \x20 défaites pas par « add rsp, N » l'alignement établi en début de fonction\n\
             \x20 s'il reste un appel à faire : rendez la pile une seule fois, à la toute fin.",
            bad.excess
        ),
        Lang::En => format!(
            "  On x86-64, RSP must be a multiple of 16 right before every `call`: both\n\
             \x20 ABIs agree, Linux and Windows alike. Here RSP is {} too high.\n\
             \x20 The program still assembles and starts, but it will crash inside the\n\
             \x20 function you called: the faulting address will point at none of your\n\
             \x20 lines, the message will say nothing about alignment, and the crash may\n\
             \x20 even happen only every other run. Unguessable from the symptom alone.\n\
             \x20 Hint: add up everything that moves RSP between the entry and this call\n\
             \x20 (`push`, `pop`, `sub rsp, N`, `add rsp, N`). In particular, do not undo\n\
             \x20 with `add rsp, N` the alignment set up at the start of the function while\n\
             \x20 a call is still to come: give the stack back once, at the very end.",
            bad.excess
        ),
        Lang::Es => format!(
            "  En x86-64, RSP debe ser múltiplo de 16 justo antes de cada «call»: es la\n\
             \x20 regla de ambas ABI, tanto en Linux como en Windows. Aquí RSP sobra en {}.\n\
             \x20 El programa se ensambla y arranca igualmente, pero fallará dentro de la\n\
             \x20 función llamada: la dirección del error no señalará ninguna de sus líneas,\n\
             \x20 el mensaje no hablará de alineación y el fallo puede incluso ocurrir solo\n\
             \x20 una vez de cada dos. Imposible de adivinar a partir del síntoma.\n\
             \x20 Pista: sume todo lo que mueve RSP entre el inicio y esta llamada («push»,\n\
             \x20 «pop», «sub rsp, N», «add rsp, N»). En particular, no deshaga con\n\
             \x20 «add rsp, N» la alineación establecida al principio de la función si aún\n\
             \x20 queda una llamada: devuelva la pila una sola vez, al final del todo.",
            bad.excess
        ),
    };
    // Le deux-points français veut son espace insécable devant ; pas les autres.
    let colon = i18n::tr3(lang, " : ", ": ", ": ");
    format!("{head} ({where_}){colon}{call}\n{body}\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble::{self, Target};
    use std::path::{Path, PathBuf};

    /// Construit un source pour la cible demandée et rend le journal de build.
    fn build_log(dir: &str, source: &str, target: Target) -> String {
        let dir = PathBuf::from("build").join(dir);
        std::fs::create_dir_all(&dir).expect("dossier de test");
        let src = dir.join("essai.asm");
        std::fs::write(&src, source).expect("écriture du source");
        let out = assemble::assemble_for(&src, &dir, &[], target, Lang::Fr)
            .unwrap_or_else(|e| panic!("ce source doit s'assembler : {e}"));
        assert!(
            out.binary.is_file(),
            "l'avertissement ne doit pas empêcher la production du binaire"
        );
        out.log
    }

    const WARNING_FR: &str = "pile désalignée avant un appel";

    /// Petites instructions fabriquées à la main, pour éprouver le compte de
    /// pile sans passer par nasm.
    fn insns(asm: &[(&str, &str)]) -> Vec<Insn> {
        asm.iter()
            .enumerate()
            .map(|(i, (m, o))| Insn {
                address: 0x1000 + i as u64 * 4,
                bytes: vec![0x90],
                mnemonic: m.to_string(),
                operands: o.to_string(),
            })
            .collect()
    }

    /// Le piège en entier : `sub rsp, 40` aligne, `add rsp, 40` défait
    /// l'alignement, et le second appel part dans le mur. C'est le programme qui
    /// a réellement planté sous Wine, à une adresse qui ne parlait de rien.
    #[test]
    fn the_classic_restore_before_a_second_call_is_caught_on_windows() {
        let log = build_log(
            "stack-align-win-bad",
            "bits 64\ndefault rel\n\
             section .data\n  caption db \"t\", 0\n  texte db \"t\", 0\n\
             section .text\n  global main\n  extern MessageBoxA\n  extern ExitProcess\n\
             main:\n\
             \x20   sub rsp, 40\n\
             \x20   xor ecx, ecx\n\
             \x20   lea rdx, [texte]\n\
             \x20   lea r8, [caption]\n\
             \x20   xor r9d, r9d\n\
             \x20   call MessageBoxA\n\
             \x20   add rsp, 40\n\
             \x20   xor ecx, ecx\n\
             \x20   call ExitProcess\n",
            Target::Windows,
        );
        assert!(log.contains(WARNING_FR), "avertissement absent :\n{log}");
        // Le `call ExitProcess` fautif est à la 19e ligne du source ci-dessus :
        // citer la bonne ligne est la moitié de l'intérêt du message.
        assert!(log.contains("(ligne 19)"), "ligne non citée :\n{log}");
        // Et le premier appel, lui, était correct : c'est bien le second qui
        // doit être montré du doigt.
        assert!(!log.contains("(ligne 16)"), "mauvais appel désigné :\n{log}");
        // L'assemblage reste un succès : ce n'est qu'un avertissement.
        assert!(log.contains("Build OK"), "l'assemblage doit rester un succès :\n{log}");
    }

    /// La règle est celle des deux ABI, et le code qui la vérifie est le même.
    /// Côté ELF, l'appel qui sort du programme passe par un talon PLT — celui
    /// que la leçon « PLT et GOT » fait justement écrire à la main.
    #[test]
    fn a_misaligned_call_through_a_plt_stub_is_caught_on_linux() {
        let log = build_log(
            "stack-align-elf-bad",
            "section .data\n  got dq routine\n\
             section .text\n  global _start\n\
             routine:\n\
             \x20   ret\n\
             talon:\n\
             \x20   jmp qword [rel got]\n\
             _start:\n\
             \x20   push rbp\n\
             \x20   call talon\n\
             \x20   mov rax, 60\n\
             \x20   xor edi, edi\n\
             \x20   syscall\n",
            Target::Linux,
        );
        assert!(log.contains(WARNING_FR), "avertissement absent :\n{log}");
        assert!(log.contains("(ligne 11)"), "ligne non citée :\n{log}");
    }

    /// `_start` reçoit une pile déjà alignée : le décalage d'entrée n'est pas le
    /// même que sous Windows, et se tromper là-dessus signalerait tout ELF ou
    /// aucun. C'est mesuré (`and rsp, 15` au point d'entrée rend 0 sous Linux,
    /// 8 sous Wine), pas supposé.
    #[test]
    fn each_target_starts_from_its_own_entry_offset() {
        assert_eq!(entry_phase(Target::Linux), 0);
        assert_eq!(entry_phase(Target::Windows), 8);
        assert_eq!(entry_phase(Target::WindowsGui), 8);
    }

    /// Une pile équilibrée ne doit rien déclencher : `sub rsp, 40` posé une fois
    /// pour toutes, deux appels, aucune restauration prématurée.
    #[test]
    fn a_balanced_prologue_stays_silent() {
        let log = build_log(
            "stack-align-win-good",
            "bits 64\ndefault rel\n\
             section .text\n  global main\n  extern GetStdHandle\n  extern ExitProcess\n\
             main:\n\
             \x20   sub rsp, 40\n\
             \x20   mov ecx, -11\n\
             \x20   call GetStdHandle\n\
             \x20   xor ecx, ecx\n\
             \x20   call ExitProcess\n",
            Target::Windows,
        );
        assert!(!log.contains(WARNING_FR), "faux positif :\n{log}");
    }

    /// Une boucle : l'analyse doit abandonner proprement — ni avertissement, ni
    /// panique — alors même que le compte de pile écrit après le saut serait faux.
    #[test]
    fn a_loop_makes_the_analysis_give_up_quietly() {
        let log = build_log(
            "stack-align-loop",
            "bits 64\ndefault rel\n\
             section .text\n  global main\n  extern ExitProcess\n\
             main:\n\
             \x20   sub rsp, 40\n\
             \x20   mov rcx, 3\n\
             boucle:\n\
             \x20   push rcx\n\
             \x20   dec rcx\n\
             \x20   jnz boucle\n\
             \x20   xor ecx, ecx\n\
             \x20   call ExitProcess\n",
            Target::Windows,
        );
        assert!(!log.contains(WARNING_FR), "faux positif après un saut :\n{log}");
    }

    /// Une taille de pile calculée à l'exécution : rien n'est déductible du
    /// code, donc rien ne doit être dit.
    #[test]
    fn a_computed_stack_adjustment_makes_the_analysis_give_up() {
        let external = |_: &str| true;
        let code = insns(&[("sub", "rsp, rax"), ("call", "0x2000")]);
        assert_eq!(first_misaligned_call(&code, 0x1000, 8, external), None);
        // Le même code avec un immédiat, lui, se laisse chiffrer.
        let code = insns(&[("sub", "rsp, 0x8"), ("call", "0x2000")]);
        assert_eq!(first_misaligned_call(&code, 0x1000, 8, external), None);
        let code = insns(&[("sub", "rsp, 0x10"), ("call", "0x2000")]);
        assert_eq!(
            first_misaligned_call(&code, 0x1000, 8, external).map(|m| m.excess),
            Some(8)
        );
    }

    /// Un appel désaligné vers une routine locale ne dit rien, mais ne doit pas
    /// non plus arrêter l'analyse : le vrai piège peut venir juste après.
    #[test]
    fn a_local_misaligned_call_is_stepped_over_not_reported() {
        let code = insns(&[
            ("push", "rbp"),
            ("call", "0x9000"), // routine locale : sans conséquence
            ("call", "0x9100"), // fonction système : celui-là compte
        ]);
        let external = |ops: &str| ops == "0x9100";
        let found = first_misaligned_call(&code, 0x1000, 0, external).expect("appel signalé");
        assert_eq!(found.target, "0x9100");
        assert_eq!(found.excess, 8);
    }

    /// Lecture des opérandes : écrire *dans* RSP et écrire *par* RSP ne doivent
    /// pas être confondus.
    #[test]
    fn only_writes_to_rsp_itself_count() {
        assert!(writes_rsp("rsp, rax"));
        assert!(!writes_rsp("qword ptr [rsp + 32], 0"));
        assert!(!writes_rsp("rbp, rsp"));
        assert_eq!(rsp_immediate("rsp, 0x28"), Some(40));
        assert_eq!(rsp_immediate("rsp, 40"), Some(40));
        assert_eq!(rsp_immediate("rbp, 0x28"), None);
        assert_eq!(rsp_immediate("rsp, rax"), None);
        assert!(is_rip_relative_memory("qword ptr [rip + 0x1034]"));
        assert!(!is_rip_relative_memory("qword ptr [rsi]"));
        assert_eq!(literal_address("0x140001030"), Some(0x140001030));
        assert_eq!(literal_address("rax"), None);
    }

    /// Les trois langues doivent être servies : le journal part droit dans la
    /// console de l'élève, qui a pu mettre l'IDE en anglais ou en espagnol.
    #[test]
    fn the_warning_speaks_the_interface_language() {
        let bad = Misaligned { address: 0x401000, target: "0x401030".into(), excess: 8 };
        let fr = render(&bad, Some(12), Lang::Fr);
        let en = render(&bad, Some(12), Lang::En);
        let es = render(&bad, Some(12), Lang::Es);
        assert!(fr.contains("pile désalignée") && fr.contains("(ligne 12)"), "{fr}");
        assert!(en.contains("stack misaligned") && en.contains("(line 12)"), "{en}");
        assert!(es.contains("pila desalineada") && es.contains("(línea 12)"), "{es}");
        // Sans listing exploitable, l'adresse remplace la ligne plutôt que rien.
        let sans = render(&bad, None, Lang::Fr);
        assert!(sans.contains("0x401000"), "{sans}");
    }

    /// Le filet de sécurité : aucun exemple ni exercice livré avec l'IDE ne doit
    /// déclencher l'avertissement. Un avertissement qui crie à tort sur le code
    /// fourni ne serait plus jamais lu.
    #[test]
    fn no_shipped_example_triggers_the_warning() {
        let mut checked = 0;
        for dir in ["examples", "examples_seed"] {
            let Ok(entries) = std::fs::read_dir(dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("asm") {
                    continue;
                }
                let Ok(source) = std::fs::read_to_string(&path) else { continue };
                let target = assemble::detect_target(&source).unwrap_or(Target::Linux);
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("x");
                let out_dir = PathBuf::from("build/stack-align-survey").join(stem);
                let Ok(out) = assemble::assemble_for(
                    &path,
                    &out_dir,
                    &[PathBuf::from("examples")],
                    target,
                    Lang::Fr,
                ) else {
                    continue; // ne s'assemble pas seul : hors sujet ici
                };
                checked += 1;
                assert!(
                    !out.log.contains(WARNING_FR),
                    "faux positif sur {} :\n{}",
                    path.display(),
                    out.log
                );
            }
        }
        assert!(checked > 20, "trop peu d'exemples vérifiés ({checked})");
    }

    /// Les programmes de départ des leçons sont écrits en dur dans le binaire :
    /// ce sont eux que l'élève assemble le plus souvent, et un avertissement qui
    /// s'allumerait au hasard des leçons ne serait plus jamais lu.
    ///
    /// Une seule leçon doit le déclencher : « L'espace d'ombre », dont le
    /// programme de départ contient précisément le piège qu'elle enseigne. La
    /// liste est donc close, et le test échoue aussi bien si une leçon s'y
    /// ajoute que si celle-là cesse d'y être.
    #[test]
    fn the_shadow_space_lesson_is_the_only_starter_that_warns() {
        let dir = Path::new("build/stack-align-lessons");
        let mut warned = Vec::new();
        let mut checked = 0;
        for lesson in crate::tutorial::catalogue() {
            let Some(starter) = lesson.starter else { continue };
            let out_dir = dir.join(lesson.id);
            std::fs::create_dir_all(&out_dir).expect("dossier de test");
            let src = out_dir.join("lecon.asm");
            std::fs::write(&src, starter).expect("écriture");
            let Ok(out) = assemble::assemble_for(&src, &out_dir, &[], lesson.target(), Lang::Fr)
            else {
                continue; // un starter à trous peut ne pas s'assembler tel quel
            };
            checked += 1;
            if out.log.contains(WARNING_FR) {
                warned.push(lesson.id);
            }
        }
        assert!(checked > 20, "trop peu de leçons vérifiées ({checked})");
        assert_eq!(warned, ["win_pile"], "leçons averties inattendues");
    }

    /// La leçon « L'espace d'ombre » enseigne précisément ce piège, et son
    /// programme de départ le contient : `sub rsp, 0` au lieu de `sub rsp, 40`.
    /// L'avertissement doit s'y déclencher — c'est la meilleure preuve qu'il
    /// vise juste — puis se taire dès que l'exercice est fait.
    #[test]
    fn the_shadow_space_lesson_is_a_true_positive() {
        let lesson = crate::tutorial::find("win_pile").expect("la leçon existe");
        let starter = lesson.starter.expect("elle a un programme de départ");
        let log = build_log("stack-align-shadow", starter, lesson.target());
        assert!(log.contains(WARNING_FR), "avertissement absent :\n{log}");

        // Et une fois l'exercice fait, le silence revient.
        let corrige = starter.replace("sub     rsp, 0 ", "sub     rsp, 40");
        assert_ne!(corrige, starter, "le remplacement doit s'appliquer");
        let log = build_log("stack-align-shadow-ok", &corrige, lesson.target());
        assert!(!log.contains(WARNING_FR), "faux positif sur le corrigé :\n{log}");
    }
}
