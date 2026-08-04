//! Convention d'appel System V AMD64 : ce que le panneau CALL STACK ne dit pas.
//!
//! Les adresses de retour empilées ne suffisent pas à comprendre un appel. Il
//! manque le vocabulaire : quels registres portent les arguments, lesquels
//! survivent à un `call`, et comment le cadre de pile est disposé autour de RBP.
//!
//! Ce module est purement descriptif — il ne lit pas le processus. Il fournit la
//! table de l'ABI et la reconnaissance du prologue/épilogue à partir du
//! désassemblage, que l'UI compose ensuite avec l'état réel.

use crate::i18n::{self, Lang};

/// Rôle d'un registre vis-à-vis de la convention d'appel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Porte le n-ième argument entier (0-based).
    Argument(usize),
    /// Porte la valeur de retour.
    Return,
    /// Sauvé par l'appelé : sa valeur survit à un `call`.
    CalleeSaved,
    /// Sauvé par l'appelant : un `call` peut l'écraser.
    CallerSaved,
    /// Pointeur de pile ou de cadre.
    FramePointer,
    StackPointer,
}

impl Role {
    /// Vrai si la valeur du registre survit à un `call` — la question que se
    /// pose l'élève dont une variable disparaît après un appel.
    pub fn survives_call(self) -> bool {
        matches!(self, Role::CalleeSaved | Role::FramePointer | Role::StackPointer)
    }

    pub fn label(self, lang: Lang) -> String {
        match self {
            Role::Argument(n) => format!(
                "{} {}",
                i18n::tr3(lang, "argument", "argument", "argumento"),
                n + 1
            ),
            Role::Return => i18n::tr3(lang, "valeur de retour", "return value", "valor de retorno").to_string(),
            Role::CalleeSaved => i18n::tr3(lang, "sauvé par l'appelé", "callee-saved", "guardado por el llamado").to_string(),
            Role::CallerSaved => i18n::tr3(lang, "sauvé par l'appelant", "caller-saved", "guardado por el llamador").to_string(),
            Role::FramePointer => i18n::tr3(lang, "base du cadre", "frame base", "base del marco").to_string(),
            Role::StackPointer => i18n::tr3(lang, "sommet de pile", "stack top", "cima de la pila").to_string(),
        }
    }
}

/// Rôle ABI d'un registre général, dans la convention System V AMD64
/// (celle de Linux — Windows utilise un ordre d'arguments différent).
pub fn role(reg: &str) -> Role {
    match reg.to_uppercase().as_str() {
        "RDI" => Role::Argument(0),
        "RSI" => Role::Argument(1),
        "RDX" => Role::Argument(2),
        "RCX" => Role::Argument(3),
        "R8" => Role::Argument(4),
        "R9" => Role::Argument(5),
        "RAX" => Role::Return,
        "RBX" | "R12" | "R13" | "R14" | "R15" => Role::CalleeSaved,
        "RBP" => Role::FramePointer,
        "RSP" => Role::StackPointer,
        // R10 et R11 sont libres pour l'appelant : R11 sert de scratch au
        // noyau lors d'un `syscall` (il y perd sa valeur, comme RCX).
        _ => Role::CallerSaved,
    }
}

/// Les six registres d'arguments entiers, dans l'ordre.
pub const ARG_REGS: [&str; 6] = ["RDI", "RSI", "RDX", "RCX", "R8", "R9"];

/// Ordre des arguments d'un appel système Linux : RCX est remplacé par R10,
/// car `syscall` écrase RCX (il y range l'adresse de retour).
pub const SYSCALL_ARG_REGS: [&str; 6] = ["RDI", "RSI", "RDX", "R10", "R8", "R9"];

/// Où se trouve une case donnée par rapport à RBP, une fois le prologue exécuté.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    /// `[rbp+8]` — l'adresse empilée par `call`.
    ReturnAddress,
    /// `[rbp]` — le RBP de l'appelant, empilé par le prologue.
    SavedFramePointer,
    /// `[rbp-N]` — variable locale réservée par `sub rsp, N`.
    Local,
    /// Au-dessus de l'adresse de retour : arguments passés par la pile
    /// (au-delà du sixième).
    IncomingArgument,
}

impl SlotKind {
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            SlotKind::ReturnAddress => i18n::tr3(lang, "adresse de retour", "return address", "dirección de retorno"),
            SlotKind::SavedFramePointer => i18n::tr3(lang, "RBP de l'appelant", "caller's RBP", "RBP del llamador"),
            SlotKind::Local => i18n::tr3(lang, "variable locale", "local variable", "variable local"),
            SlotKind::IncomingArgument => i18n::tr3(lang, "argument reçu", "incoming argument", "argumento recibido"),
        }
    }
}

/// Classe une adresse de pile par rapport à RBP.
///
/// Le cadre standard, une fois `push rbp ; mov rbp, rsp` exécuté :
///
/// ```text
///   [rbp+16]  argument reçu (7e et suivants)
///   [rbp+8]   adresse de retour   <- empilée par call
///   [rbp+0]   RBP de l'appelant   <- empilé par le prologue
///   [rbp-8]   première locale
/// ```
pub fn classify_slot(addr: u64, rbp: u64) -> Option<SlotKind> {
    // Hors d'un cadre plausible : on ne raconte rien plutôt que de mentir.
    if rbp == 0 {
        return None;
    }
    let off = addr as i64 - rbp as i64;
    Some(match off {
        0 => SlotKind::SavedFramePointer,
        8 => SlotKind::ReturnAddress,
        o if o > 8 => SlotKind::IncomingArgument,
        _ => SlotKind::Local,
    })
}

/// Étape du cadre de pile reconnue dans le code autour de RIP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramePhase {
    /// `push rbp` — le cadre de l'appelant est sauvegardé.
    PrologueSave,
    /// `mov rbp, rsp` — RBP devient la base du nouveau cadre.
    PrologueSetup,
    /// `sub rsp, N` — réservation des variables locales.
    PrologueReserve,
    /// `leave` ou `mov rsp, rbp` — démontage du cadre.
    Epilogue,
    /// `ret` — retour à l'appelant.
    Return,
    /// `call` — entrée dans une fonction.
    Call,
}

impl FramePhase {
    pub fn explain(self, lang: Lang) -> &'static str {
        let t = |fr, en, es| i18n::tr3(lang, fr, en, es);
        match self {
            FramePhase::PrologueSave => t(
                "Prologue 1/3 — « push rbp » sauvegarde la base du cadre de l'appelant, pour \
                 pouvoir la lui rendre intacte.",
                "Prologue 1/3 — \"push rbp\" saves the caller's frame base, so it can be handed \
                 back untouched.",
                "Prólogo 1/3 — «push rbp» guarda la base del marco del llamador, para devolvérsela intacta.",
            ),
            FramePhase::PrologueSetup => t(
                "Prologue 2/3 — « mov rbp, rsp » fige la base du nouveau cadre : les locales se \
                 lisent désormais en [rbp-N], quoi qu'il arrive à RSP ensuite.",
                "Prologue 2/3 — \"mov rbp, rsp\" pins the new frame base: locals are read at \
                 [rbp-N] from now on, whatever happens to RSP afterwards.",
                "Prólogo 2/3 — «mov rbp, rsp» fija la base del nuevo marco: las locales se leen \
                 en [rbp-N], pase lo que pase con RSP.",
            ),
            FramePhase::PrologueReserve => t(
                "Prologue 3/3 — « sub rsp, N » réserve N octets de variables locales en \
                 abaissant le sommet de pile.",
                "Prologue 3/3 — \"sub rsp, N\" reserves N bytes of local variables by lowering \
                 the stack top.",
                "Prólogo 3/3 — «sub rsp, N» reserva N bytes de variables locales bajando la cima.",
            ),
            FramePhase::Epilogue => t(
                "Épilogue — le cadre est démonté : RSP revient à RBP, puis l'ancien RBP est \
                 dépilé. La pile retrouve l'état où « ret » trouvera son adresse.",
                "Epilogue — the frame is torn down: RSP returns to RBP, then the old RBP is \
                 popped. The stack is back to the state where \"ret\" will find its address.",
                "Epílogo — se desmonta el marco: RSP vuelve a RBP y se desapila el RBP anterior.",
            ),
            FramePhase::Return => t(
                "« ret » dépile l'adresse de retour dans RIP. Si les push et les pop ne \
                 s'équilibrent pas, c'est une donnée qui est dépilée — et le programme saute \
                 dans le vide.",
                "\"ret\" pops the return address into RIP. If pushes and pops do not balance, a \
                 piece of data is popped instead — and the program jumps into nowhere.",
                "«ret» desapila la dirección de retorno en RIP. Si los push y pop no se \
                 equilibran, se desapila un dato — y el programa salta al vacío.",
            ),
            FramePhase::Call => t(
                "« call » empile l'adresse de l'instruction suivante puis saute. C'est cette \
                 adresse que « ret » ira rechercher.",
                "\"call\" pushes the address of the next instruction then jumps. That address is \
                 what \"ret\" will look for.",
                "«call» apila la dirección de la instrucción siguiente y salta. Esa dirección es \
                 la que «ret» buscará.",
            ),
        }
    }
}

/// Reconnaît une étape de cadre à partir d'une instruction désassemblée.
pub fn frame_phase(mnemonic: &str, operands: &str) -> Option<FramePhase> {
    let m = mnemonic.to_lowercase();
    let ops = operands.to_lowercase().replace(' ', "");
    Some(match m.as_str() {
        "push" if ops == "rbp" => FramePhase::PrologueSave,
        "mov" if ops == "rbp,rsp" => FramePhase::PrologueSetup,
        "sub" if ops.starts_with("rsp,") => FramePhase::PrologueReserve,
        "leave" => FramePhase::Epilogue,
        "mov" if ops == "rsp,rbp" => FramePhase::Epilogue,
        "ret" => FramePhase::Return,
        "call" => FramePhase::Call,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argument_registers_are_in_system_v_order() {
        for (i, r) in ARG_REGS.iter().enumerate() {
            assert_eq!(role(r), Role::Argument(i), "{r} doit être l'argument {}", i + 1);
        }
        assert_eq!(role("RAX"), Role::Return);
    }

    /// La question qui compte pour l'élève : « ma valeur survit-elle au call ? »
    #[test]
    fn callee_saved_registers_survive_a_call() {
        for r in ["RBX", "R12", "R13", "R14", "R15", "RBP", "RSP"] {
            assert!(role(r).survives_call(), "{r} doit survivre à un call");
        }
        for r in ["RAX", "RCX", "RDX", "RSI", "RDI", "R8", "R9", "R10", "R11"] {
            assert!(!role(r).survives_call(), "{r} ne survit pas à un call");
        }
    }

    /// RCX porte le 4e argument d'une fonction, mais PAS d'un syscall :
    /// `syscall` écrase RCX, d'où R10.
    #[test]
    fn syscall_uses_r10_instead_of_rcx() {
        assert_eq!(ARG_REGS[3], "RCX");
        assert_eq!(SYSCALL_ARG_REGS[3], "R10");
        // Le reste de l'ordre est identique.
        for i in [0, 1, 2, 4, 5] {
            assert_eq!(ARG_REGS[i], SYSCALL_ARG_REGS[i]);
        }
    }

    #[test]
    fn stack_slots_are_classified_around_rbp() {
        let rbp = 0x7fff_0000u64;
        assert_eq!(classify_slot(rbp, rbp), Some(SlotKind::SavedFramePointer));
        assert_eq!(classify_slot(rbp + 8, rbp), Some(SlotKind::ReturnAddress));
        assert_eq!(classify_slot(rbp + 16, rbp), Some(SlotKind::IncomingArgument));
        assert_eq!(classify_slot(rbp - 8, rbp), Some(SlotKind::Local));
        assert_eq!(classify_slot(rbp - 64, rbp), Some(SlotKind::Local));
    }

    /// Sans cadre établi (RBP nul au démarrage), on préfère ne rien annoncer
    /// plutôt que d'étiqueter la pile n'importe comment.
    #[test]
    fn no_frame_means_no_claim() {
        assert_eq!(classify_slot(0x7fff_0000, 0), None);
    }

    #[test]
    fn prologue_and_epilogue_are_recognised() {
        assert_eq!(frame_phase("push", "rbp"), Some(FramePhase::PrologueSave));
        assert_eq!(frame_phase("mov", "rbp, rsp"), Some(FramePhase::PrologueSetup));
        assert_eq!(frame_phase("sub", "rsp, 0x20"), Some(FramePhase::PrologueReserve));
        assert_eq!(frame_phase("leave", ""), Some(FramePhase::Epilogue));
        assert_eq!(frame_phase("mov", "rsp, rbp"), Some(FramePhase::Epilogue));
        assert_eq!(frame_phase("ret", ""), Some(FramePhase::Return));
        assert_eq!(frame_phase("call", "0x401000"), Some(FramePhase::Call));
    }

    /// `push rax` n'est pas un prologue, `mov rbp, rax` non plus : la
    /// reconnaissance doit rester stricte, sinon elle raconte n'importe quoi.
    #[test]
    fn unrelated_instructions_are_not_frame_phases() {
        assert_eq!(frame_phase("push", "rax"), None);
        assert_eq!(frame_phase("mov", "rbp, rax"), None);
        assert_eq!(frame_phase("mov", "rax, rsp"), None);
        assert_eq!(frame_phase("add", "rsp, 8"), None, "add n'est pas une réservation");
        assert_eq!(frame_phase("xor", "rax, rax"), None);
    }

    #[test]
    fn all_phases_and_roles_are_translated() {
        let phases = [
            FramePhase::PrologueSave,
            FramePhase::PrologueSetup,
            FramePhase::PrologueReserve,
            FramePhase::Epilogue,
            FramePhase::Return,
            FramePhase::Call,
        ];
        for lang in [Lang::Fr, Lang::En, Lang::Es] {
            for p in phases {
                assert!(!p.explain(lang).is_empty(), "{p:?} vide en {lang:?}");
            }
            for r in ["RDI", "RAX", "RBX", "R11", "RBP", "RSP"] {
                assert!(!role(r).label(lang).is_empty(), "{r} vide en {lang:?}");
            }
            for k in [
                SlotKind::ReturnAddress,
                SlotKind::SavedFramePointer,
                SlotKind::Local,
                SlotKind::IncomingArgument,
            ] {
                assert!(!k.label(lang).is_empty());
            }
        }
    }
}

/// Vérifie la description de l'ABI contre un vrai cadre de pile monté par un
/// programme NASM : sans cela, la table ci-dessus resterait une affirmation.
#[cfg(test)]
mod integration {
    use super::*;
    use crate::{assemble, debugger::Debugger, disasm};
    use std::path::Path;

    #[test]
    fn labels_match_a_real_stack_frame() {
        let src = "\
section .text
    global _start
_start:
    mov rdi, 7
    call f
    mov rax, 60
    xor rdi, rdi
    syscall
f:
    push rbp
    mov rbp, rsp
    sub rsp, 16
    mov qword [rbp-8], 0x1234
    leave
    ret
";
        std::fs::create_dir_all("build").ok();
        std::fs::write("examples/abi-frame.asm", src).expect("écriture");
        let out = assemble::assemble_with_includes(
            Path::new("examples/abi-frame.asm"),
            Path::new("build/abi-frame"),
            &[],
        )
        .expect("assemblage");
        let insns = disasm::disassemble_text(&out.binary).unwrap_or_default();
        let mut dbg = Debugger::launch(&out.binary).expect("launch");

        // Avance jusqu'à ce que le prologue soit complet (RBP établi et
        // différent de RSP, donc après « sub rsp, 16 »).
        let mut steps = 0;
        while steps < 40 {
            dbg.step().expect("step");
            steps += 1;
            let r = dbg.regs();
            if r.rbp != 0 && r.rbp > r.rsp {
                break;
            }
        }
        let regs = dbg.regs().clone();
        assert!(regs.rbp != 0, "le prologue doit avoir établi RBP");

        // [rbp+8] doit contenir une adresse de retour, c'est-à-dire une adresse
        // qui tombe dans le code désassemblé.
        let ret_slot = regs.rbp + 8;
        assert_eq!(classify_slot(ret_slot, regs.rbp), Some(SlotKind::ReturnAddress));
        let raw = dbg.read_mem(ret_slot, 8).expect("lecture pile");
        let ret_addr = u64::from_le_bytes(raw.try_into().unwrap());
        assert!(
            insns.iter().any(|i| i.address == ret_addr),
            "[rbp+8] = 0x{ret_addr:X} doit être une adresse d'instruction"
        );

        // [rbp] doit contenir le RBP de l'appelant.
        assert_eq!(classify_slot(regs.rbp, regs.rbp), Some(SlotKind::SavedFramePointer));
        // [rbp-8] est bien classé comme locale, et RSP est sous elle.
        assert_eq!(classify_slot(regs.rbp - 8, regs.rbp), Some(SlotKind::Local));
        assert!(regs.rsp < regs.rbp, "les locales sont réservées sous RBP");

        // Le prologue du programme doit être reconnu dans le désassemblage.
        let phases: Vec<FramePhase> = insns
            .iter()
            .filter_map(|i| frame_phase(&i.mnemonic, &i.operands))
            .collect();
        assert!(phases.contains(&FramePhase::PrologueSave), "push rbp attendu");
        assert!(phases.contains(&FramePhase::PrologueSetup), "mov rbp, rsp attendu");
        assert!(phases.contains(&FramePhase::PrologueReserve), "sub rsp, N attendu");
        assert!(phases.contains(&FramePhase::Epilogue), "leave attendu");
        assert!(phases.contains(&FramePhase::Return), "ret attendu");
        assert!(phases.contains(&FramePhase::Call), "call attendu");
    }
}
