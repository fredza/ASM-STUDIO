//! Traduction d'une faute matérielle en explication compréhensible.
//!
//! Un segfault brut (« Terminated (signal) ») n'apprend rien. Ici on croise
//! quatre sources pour reconstituer *ce que l'élève a écrit de faux* :
//!
//! * le signal et `si_addr` ([`Fault`]) — quelle adresse a été refusée ;
//! * la carte mémoire ([`MemRegion`]) — cette adresse tombe-t-elle dans une
//!   zone connue, et avec quelles permissions ;
//! * RIP et le désassemblage — quelle instruction a fauté, en lecture ou en
//!   écriture ;
//! * la table adresse→ligne — où pointer dans l'éditeur.
//!
//! Le résultat est un [`Diagnosis`] : une cause probable nommée, un texte
//! explicatif, et une piste de correction.

use crate::debugger::{Fault, MemRegion, RegionKind};
use crate::i18n::{self, Lang};

/// Cause probable d'une faute, déduite du contexte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// Déréférencement d'un pointeur nul (adresse 0 ou proche de 0).
    NullPointer,
    /// Écriture dans une région en lecture seule (`.text`, `.rodata`).
    WriteToReadOnly,
    /// Adresse hors de toute région mappée : pointeur fantaisiste.
    WildPointer,
    /// RIP lui-même est invalide : le flot d'exécution a déraillé
    /// (typiquement un `ret` avec une pile déséquilibrée).
    BadInstructionPointer,
    /// Accès juste sous la pile : débordement.
    StackOverflow,
    /// Division par zéro (ou débordement de division).
    DivisionByZero,
    /// Opcode invalide.
    IllegalInstruction,
    /// Accès mal aligné / erreur de bus.
    Misaligned,
    /// Faute reconnue mais sans cause plus précise.
    Unknown,
}

/// Diagnostic complet, prêt à afficher.
#[derive(Debug, Clone)]
pub struct Diagnosis {
    pub cause: Cause,
    /// Titre court, ex. « Déréférencement de pointeur nul ».
    pub title: String,
    /// Explication en langue courante, plusieurs phrases.
    pub explanation: String,
    /// Piste concrète de correction.
    pub hint: String,
    /// Ligne source (1-based) à surligner, si connue.
    pub line: Option<usize>,
    /// Adresse fautive, si le noyau l'a fournie.
    pub addr: Option<u64>,
    /// Région contenant l'adresse fautive, si elle est mappée.
    pub region: Option<RegionKind>,
}

/// Seuil sous lequel une adresse est considérée « nulle ou presque » : un
/// déréférencement de `[rax+16]` avec RAX=0 fauterait à l'adresse 16, pas 0.
const NULL_ZONE: u64 = 0x1000;

/// Analyse une faute et produit un diagnostic.
///
/// * `fault` — signal + adresse fautive + RIP ;
/// * `regions` — carte mémoire du processus (au moment de la faute) ;
/// * `is_write` — l'instruction fautive écrivait-elle en mémoire (déduite du
///   désassemblage par l'appelant) ;
/// * `line` — ligne source correspondant à RIP.
pub fn diagnose(
    fault: &Fault,
    regions: &[MemRegion],
    is_write: bool,
    line: Option<usize>,
    lang: Lang,
) -> Diagnosis {
    use nix::sys::signal::Signal::*;

    let tr = |fr: &str, en: &str, es: &str| -> String {
        match lang {
            Lang::Fr => fr.to_string(),
            Lang::En => en.to_string(),
            Lang::Es => es.to_string(),
        }
    };

    let addr = fault.addr;
    let region = addr.and_then(|a| regions.iter().find(|r| r.contains(a)).map(|r| r.kind));
    let rip_mapped = regions.iter().any(|r| r.contains(fault.rip));

    // SIGFPE / SIGILL / SIGBUS ont une cause immédiate, sans analyse d'adresse.
    let cause = match fault.signal {
        SIGFPE => Cause::DivisionByZero,
        SIGILL => Cause::IllegalInstruction,
        SIGBUS => Cause::Misaligned,
        // SIGSEGV : c'est là que le contexte compte.
        _ => classify_segv(addr, region, rip_mapped, is_write, regions),
    };

    let (title, explanation, hint) = describe(cause, fault, addr, region, &tr);

    Diagnosis { cause, title, explanation, hint, line, addr, region }
}

/// Cœur du classement d'un SIGSEGV.
fn classify_segv(
    addr: Option<u64>,
    region: Option<RegionKind>,
    rip_mapped: bool,
    is_write: bool,
    regions: &[MemRegion],
) -> Cause {
    // RIP hors de tout code exécutable : le programme ne sait plus où il est.
    // C'est le symptôme du `ret` sur adresse de retour écrasée — priorité haute,
    // car dans ce cas l'adresse fautive *est* RIP et le reste induirait en erreur.
    if !rip_mapped {
        return Cause::BadInstructionPointer;
    }

    let Some(a) = addr else { return Cause::Unknown };

    // Écriture dans une région mappée mais non inscriptible.
    if is_write && matches!(region, Some(RegionKind::Code | RegionKind::Rodata)) {
        return Cause::WriteToReadOnly;
    }

    if a < NULL_ZONE {
        return Cause::NullPointer;
    }

    // Juste sous la pile : la pile a grandi au-delà de sa région.
    if region.is_none()
        && let Some(stack) = regions.iter().find(|r| r.kind == RegionKind::Stack)
        && a < stack.start
        && stack.start - a < 0x10_0000
    {
        return Cause::StackOverflow;
    }

    if region.is_none() {
        return Cause::WildPointer;
    }

    Cause::Unknown
}

/// Textes du diagnostic pour une cause donnée.
fn describe(
    cause: Cause,
    fault: &Fault,
    addr: Option<u64>,
    region: Option<RegionKind>,
    tr: &impl Fn(&str, &str, &str) -> String,
) -> (String, String, String) {
    let a = addr.unwrap_or(0);
    let zone = region.map(|k| k.label()).unwrap_or("—");

    match cause {
        Cause::NullPointer => (
            tr(
                "Déréférencement de pointeur nul",
                "Null pointer dereference",
                "Desreferencia de puntero nulo",
            ),
            tr(
                &format!(
                    "L'instruction a tenté d'accéder à l'adresse 0x{a:X}, tout en bas de \
                     l'espace mémoire. Cette zone n'est jamais mappée : c'est ce qui arrive \
                     quand on utilise un registre valant 0 comme adresse, par exemple \
                     « mov rbx, [rax] » alors que RAX = 0."
                ),
                &format!(
                    "The instruction tried to access address 0x{a:X}, at the very bottom of \
                     the address space. That area is never mapped: this happens when a \
                     register holding 0 is used as an address, e.g. \"mov rbx, [rax]\" while \
                     RAX = 0."
                ),
                &format!(
                    "La instrucción intentó acceder a la dirección 0x{a:X}, en la base del \
                     espacio de memoria. Esa zona nunca está mapeada: ocurre cuando se usa \
                     un registro que vale 0 como dirección, p. ej. «mov rbx, [rax]» con RAX = 0."
                ),
            ),
            tr(
                "Vérifie que le registre utilisé entre crochets a bien reçu une adresse \
                 (un label de section .data, ou le retour d'une allocation) avant cet accès.",
                "Check that the register used inside brackets actually received an address \
                 (a .data label, or the result of an allocation) before this access.",
                "Comprueba que el registro entre corchetes recibió una dirección (una \
                 etiqueta de .data, o el resultado de una asignación) antes de este acceso.",
            ),
        ),

        Cause::WriteToReadOnly => (
            tr(
                "Écriture dans une zone en lecture seule",
                "Write to read-only memory",
                "Escritura en memoria de solo lectura",
            ),
            tr(
                &format!(
                    "L'instruction a tenté d'écrire à l'adresse 0x{a:X}, qui appartient à \
                     {zone}. Cette région est chargée en lecture seule par le noyau. Les \
                     chaînes déclarées dans « section .rodata » et le code de \
                     « section .text » ne sont pas modifiables."
                ),
                &format!(
                    "The instruction tried to write to address 0x{a:X}, which belongs to \
                     {zone}. That region is mapped read-only by the kernel. Strings declared \
                     in \"section .rodata\" and code in \"section .text\" cannot be modified."
                ),
                &format!(
                    "La instrucción intentó escribir en la dirección 0x{a:X}, que pertenece a \
                     {zone}. Esa región es de solo lectura. Las cadenas de «section .rodata» \
                     y el código de «section .text» no se pueden modificar."
                ),
            ),
            tr(
                "Déplace la donnée dans « section .data » (inscriptible) au lieu de \
                 « section .rodata », ou réserve un tampon dans « section .bss ».",
                "Move the data into \"section .data\" (writable) instead of \"section .rodata\", \
                 or reserve a buffer in \"section .bss\".",
                "Mueve el dato a «section .data» (escribible) en vez de «section .rodata», o \
                 reserva un búfer en «section .bss».",
            ),
        ),

        Cause::BadInstructionPointer => (
            tr(
                "Le programme a sauté dans le vide",
                "Execution jumped into nowhere",
                "El programa saltó al vacío",
            ),
            tr(
                &format!(
                    "RIP vaut 0x{:X}, une adresse qui ne contient aucun code exécutable. Le \
                     processeur ne lit donc plus tes instructions. La cause la plus fréquente \
                     est un « ret » exécuté avec une pile déséquilibrée : « ret » saute à \
                     l'adresse trouvée au sommet de la pile, et si un « push » n'a pas été \
                     rendu par un « pop », ce sommet contient une donnée au lieu de l'adresse \
                     de retour.",
                    fault.rip
                ),
                &format!(
                    "RIP is 0x{:X}, an address holding no executable code, so the CPU is no \
                     longer reading your instructions. The most common cause is a \"ret\" \
                     executed with an unbalanced stack: \"ret\" jumps to the address found at \
                     the top of the stack, and if a \"push\" was never matched by a \"pop\", \
                     that top holds data instead of the return address.",
                    fault.rip
                ),
                &format!(
                    "RIP vale 0x{:X}, una dirección sin código ejecutable, así que la CPU ya \
                     no lee tus instrucciones. La causa más común es un «ret» con la pila \
                     desequilibrada: «ret» salta a la dirección en la cima de la pila, y si un \
                     «push» no se compensó con un «pop», esa cima contiene un dato.",
                    fault.rip
                ),
            ),
            tr(
                "Compte tes « push » et tes « pop » dans la fonction : il doit y en avoir \
                 autant. Utilise le panneau PILE juste avant le « ret » pour vérifier que le \
                 sommet contient bien une adresse de code.",
                "Count your \"push\" and \"pop\" in the function: there must be as many of \
                 each. Use the STACK panel just before the \"ret\" to check the top really \
                 holds a code address.",
                "Cuenta tus «push» y «pop» en la función: deben ser tantos. Usa el panel PILA \
                 justo antes del «ret» para verificar que la cima contiene una dirección de código.",
            ),
        ),

        Cause::StackOverflow => (
            tr("Débordement de pile", "Stack overflow", "Desbordamiento de pila"),
            tr(
                &format!(
                    "L'adresse 0x{a:X} se trouve juste en dessous de la région de pile. La \
                     pile croît vers les adresses basses ; y descendre trop loin sort de la \
                     zone autorisée. C'est le symptôme d'une récursion sans condition d'arrêt, \
                     ou d'un « sub rsp, N » avec un N énorme."
                ),
                &format!(
                    "Address 0x{a:X} sits just below the stack region. The stack grows towards \
                     lower addresses; going too far down leaves the allowed area. This is the \
                     symptom of recursion without a base case, or a \"sub rsp, N\" with a huge N."
                ),
                &format!(
                    "La dirección 0x{a:X} está justo debajo de la región de pila. La pila crece \
                     hacia direcciones bajas; bajar demasiado sale de la zona permitida. Es \
                     síntoma de recursión sin caso base, o de un «sub rsp, N» con N enorme."
                ),
            ),
            tr(
                "Vérifie la condition d'arrêt de ta récursion, ou réduis la taille réservée \
                 par « sub rsp, N ».",
                "Check your recursion's base case, or reduce the amount reserved by \"sub rsp, N\".",
                "Revisa el caso base de tu recursión, o reduce lo reservado por «sub rsp, N».",
            ),
        ),

        Cause::WildPointer => (
            tr("Adresse mémoire invalide", "Invalid memory address", "Dirección de memoria inválida"),
            tr(
                &format!(
                    "L'adresse 0x{a:X} n'appartient à aucune région allouée au programme : ni \
                     code, ni données, ni pile, ni tas. Le registre utilisé comme adresse \
                     contenait donc autre chose qu'un pointeur — souvent une *valeur* qu'on a \
                     confondue avec l'adresse où elle est rangée."
                ),
                &format!(
                    "Address 0x{a:X} belongs to no region allocated to the program: neither \
                     code, data, stack nor heap. The register used as an address held \
                     something other than a pointer — often a *value* mistaken for the address \
                     where it is stored."
                ),
                &format!(
                    "La dirección 0x{a:X} no pertenece a ninguna región del programa: ni código, \
                     ni datos, ni pila, ni montículo. El registro usado como dirección contenía \
                     algo distinto de un puntero — a menudo un *valor* confundido con su dirección."
                ),
            ),
            tr(
                "Ouvre l'onglet « Vue mémoire » : il montre quels registres contiennent une \
                 adresse valide et lesquels non. Attention à la différence entre « mov rax, msg » \
                 (l'adresse) et « mov rax, [msg] » (le contenu).",
                "Open the \"Memory View\" tab: it shows which registers hold a valid address and \
                 which do not. Mind the difference between \"mov rax, msg\" (the address) and \
                 \"mov rax, [msg]\" (the contents).",
                "Abre la pestaña «Vista memoria»: muestra qué registros contienen una dirección \
                 válida. Ojo con la diferencia entre «mov rax, msg» (la dirección) y «mov rax, [msg]».",
            ),
        ),

        Cause::DivisionByZero => (
            tr("Division par zéro", "Division by zero", "División por cero"),
            tr(
                "L'instruction « div » ou « idiv » a été exécutée avec un diviseur nul, ou son \
                 quotient ne tient pas dans le registre de destination. Le processeur lève une \
                 exception matérielle plutôt que de produire un résultat.",
                "The \"div\" or \"idiv\" instruction ran with a zero divisor, or its quotient does \
                 not fit in the destination register. The CPU raises a hardware exception rather \
                 than producing a result.",
                "La instrucción «div» o «idiv» se ejecutó con divisor cero, o su cociente no cabe \
                 en el registro destino. La CPU lanza una excepción de hardware.",
            ),
            tr(
                "Teste le diviseur avant la division : « cmp rcx, 0 » puis « je ». Pense aussi à \
                 étendre le dividende avec « cqo » (signé) ou « xor rdx, rdx » (non signé) — un \
                 RDX résiduel fait déborder le quotient.",
                "Test the divisor first: \"cmp rcx, 0\" then \"je\". Also remember to extend the \
                 dividend with \"cqo\" (signed) or \"xor rdx, rdx\" (unsigned) — a leftover RDX \
                 overflows the quotient.",
                "Prueba el divisor antes: «cmp rcx, 0» y «je». Recuerda extender el dividendo con \
                 «cqo» (con signo) o «xor rdx, rdx» (sin signo) — un RDX residual desborda el cociente.",
            ),
        ),

        Cause::IllegalInstruction => (
            tr("Instruction illégale", "Illegal instruction", "Instrucción ilegal"),
            tr(
                &format!(
                    "Les octets à l'adresse 0x{:X} ne forment pas une instruction x86-64 valide. \
                     Soit l'exécution est arrivée dans une zone de données prise pour du code, \
                     soit l'instruction demande une extension que ce processeur n'a pas.",
                    fault.rip
                ),
                &format!(
                    "The bytes at address 0x{:X} do not form a valid x86-64 instruction. Either \
                     execution reached a data area mistaken for code, or the instruction needs a \
                     CPU extension this machine lacks.",
                    fault.rip
                ),
                &format!(
                    "Los bytes en 0x{:X} no forman una instrucción x86-64 válida. O la ejecución \
                     llegó a una zona de datos tomada por código, o la instrucción requiere una \
                     extensión que esta CPU no tiene.",
                    fault.rip
                ),
            ),
            tr(
                "Vérifie qu'aucun saut ne tombe dans « section .data », et qu'un « ret » ou un \
                 « jmp » termine bien chaque bloc de code.",
                "Check that no jump lands in \"section .data\", and that a \"ret\" or \"jmp\" \
                 properly terminates each code block.",
                "Verifica que ningún salto caiga en «section .data», y que un «ret» o «jmp» \
                 termine cada bloque de código.",
            ),
        ),

        Cause::Misaligned => (
            tr("Accès mémoire mal aligné", "Misaligned memory access", "Acceso a memoria mal alineado"),
            tr(
                &format!(
                    "L'accès à l'adresse 0x{a:X} viole une contrainte d'alignement du processeur. \
                     Certaines instructions (notamment SSE/AVX) exigent une adresse multiple de \
                     16 ou 32."
                ),
                &format!(
                    "The access at address 0x{a:X} violates a CPU alignment constraint. Some \
                     instructions (notably SSE/AVX) require an address that is a multiple of 16 or 32."
                ),
                &format!(
                    "El acceso a 0x{a:X} viola una restricción de alineación de la CPU. Algunas \
                     instrucciones (SSE/AVX) exigen una dirección múltiplo de 16 o 32."
                ),
            ),
            tr(
                "Aligne le tampon avec « align 16 » dans la section, ou utilise la variante non \
                 alignée de l'instruction (movups au lieu de movaps).",
                "Align the buffer with \"align 16\" in the section, or use the unaligned variant \
                 of the instruction (movups instead of movaps).",
                "Alinea el búfer con «align 16», o usa la variante no alineada (movups en vez de movaps).",
            ),
        ),

        Cause::Unknown => (
            format!(
                "{} ({})",
                tr("Faute mémoire", "Memory fault", "Fallo de memoria"),
                fault.signal_name()
            ),
            tr(
                &format!(
                    "Le programme a reçu {} en tentant d'accéder à 0x{a:X} (zone : {zone}). \
                     Le contexte ne permet pas d'identifier une cause classique.",
                    fault.signal_name()
                ),
                &format!(
                    "The program received {} while accessing 0x{a:X} (region: {zone}). The \
                     context does not match a classic cause.",
                    fault.signal_name()
                ),
                &format!(
                    "El programa recibió {} al acceder a 0x{a:X} (zona: {zone}). El contexto no \
                     coincide con una causa clásica.",
                    fault.signal_name()
                ),
            ),
            tr(
                "Inspecte les registres à l'étape de la faute : la timeline s'est arrêtée juste \
                 dessus, tu peux revenir en arrière pour voir d'où vient la valeur fautive.",
                "Inspect the registers at the faulting step: the timeline stopped right on it, so \
                 you can step back to see where the bad value came from.",
                "Inspecciona los registros en el paso del fallo: la línea de tiempo se detuvo ahí, \
                 puedes retroceder para ver de dónde viene el valor.",
            ),
        ),
    }
}

/// Vrai si cette instruction écrit en mémoire — utilisé pour distinguer une
/// lecture interdite d'une écriture interdite. Heuristique volontairement
/// simple : la destination est le premier opérande, entre crochets.
pub fn writes_memory(mnemonic: &str, operands: &str) -> bool {
    let m = mnemonic.to_lowercase();
    // Instructions qui écrivent implicitement en mémoire.
    if matches!(m.as_str(), "push" | "call" | "pushf" | "pushfq" | "stos" | "stosb" | "stosw" | "stosd" | "stosq") {
        return true;
    }
    // Sinon : premier opérande déréférencé ⇒ écriture, sauf pour les
    // instructions qui ne font que lire leur premier opérande.
    if matches!(m.as_str(), "cmp" | "test" | "push" | "jmp" | "call") {
        return false;
    }
    operands
        .split(',')
        .next()
        .is_some_and(|dst| dst.contains('['))
}

/// Étiquette courte d'une cause, pour la barre d'état.
pub fn cause_label(cause: Cause, lang: Lang) -> &'static str {
    match cause {
        Cause::NullPointer => i18n::tr3(lang, "pointeur nul", "null pointer", "puntero nulo"),
        Cause::WriteToReadOnly => i18n::tr3(lang, "écriture interdite", "read-only write", "escritura prohibida"),
        Cause::WildPointer => i18n::tr3(lang, "adresse invalide", "invalid address", "dirección inválida"),
        Cause::BadInstructionPointer => i18n::tr3(lang, "RIP invalide", "invalid RIP", "RIP inválido"),
        Cause::StackOverflow => i18n::tr3(lang, "débordement de pile", "stack overflow", "desbordamiento de pila"),
        Cause::DivisionByZero => i18n::tr3(lang, "division par zéro", "division by zero", "división por cero"),
        Cause::IllegalInstruction => i18n::tr3(lang, "instruction illégale", "illegal instruction", "instrucción ilegal"),
        Cause::Misaligned => i18n::tr3(lang, "mauvais alignement", "misaligned", "mal alineado"),
        Cause::Unknown => i18n::tr3(lang, "faute mémoire", "memory fault", "fallo de memoria"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nix::sys::signal::Signal;

    fn region(start: u64, end: u64, kind: RegionKind, perms: &str) -> MemRegion {
        MemRegion { start, end, kind, perms: perms.to_string() }
    }

    /// Carte mémoire typique d'un binaire NASM statique.
    fn maps() -> Vec<MemRegion> {
        vec![
            region(0x400000, 0x401000, RegionKind::Code, "r-xp"),
            region(0x401000, 0x402000, RegionKind::Rodata, "r--p"),
            region(0x402000, 0x403000, RegionKind::Data, "rw-p"),
            region(0x7fff0000, 0x7ffff000, RegionKind::Stack, "rw-p"),
        ]
    }

    fn fault(sig: Signal, addr: Option<u64>, rip: u64) -> Fault {
        Fault { signal: sig, addr, rip }
    }

    #[test]
    fn null_dereference_is_identified() {
        let d = diagnose(&fault(Signal::SIGSEGV, Some(0), 0x400080), &maps(), false, Some(5), Lang::Fr);
        assert_eq!(d.cause, Cause::NullPointer);
        assert_eq!(d.line, Some(5));
        assert!(d.explanation.contains("RAX = 0"), "doit citer le cas classique");
    }

    /// `mov rbx, [rax+16]` avec RAX=0 faute à l'adresse 16, pas 0 : la zone
    /// basse entière doit être reconnue comme nulle.
    #[test]
    fn small_offset_from_null_is_still_null() {
        let d = diagnose(&fault(Signal::SIGSEGV, Some(16), 0x400080), &maps(), false, None, Lang::Fr);
        assert_eq!(d.cause, Cause::NullPointer);
    }

    #[test]
    fn write_to_rodata_is_identified() {
        let d = diagnose(&fault(Signal::SIGSEGV, Some(0x401234), 0x400080), &maps(), true, None, Lang::Fr);
        assert_eq!(d.cause, Cause::WriteToReadOnly);
        assert_eq!(d.region, Some(RegionKind::Rodata));
        assert!(d.hint.contains(".data"), "la piste doit proposer .data");
    }

    /// Lire .rodata est parfaitement légal : ce n'est pas cette cause.
    #[test]
    fn read_from_rodata_is_not_a_readonly_violation() {
        let d = diagnose(&fault(Signal::SIGSEGV, Some(0x401234), 0x400080), &maps(), false, None, Lang::Fr);
        assert_ne!(d.cause, Cause::WriteToReadOnly);
    }

    /// Le cas le plus précieux : `ret` avec pile déséquilibrée. RIP part dans
    /// le vide, et le diagnostic doit parler de push/pop.
    #[test]
    fn unmapped_rip_points_at_stack_imbalance() {
        let d = diagnose(&fault(Signal::SIGSEGV, Some(0xDEAD), 0xDEAD), &maps(), false, None, Lang::Fr);
        assert_eq!(d.cause, Cause::BadInstructionPointer);
        assert!(d.explanation.contains("ret"), "doit nommer ret");
        assert!(d.hint.contains("push"), "la piste doit parler de push/pop");
    }

    /// RIP invalide prime sur l'analyse d'adresse : sinon on expliquerait un
    /// pointeur nul alors que le vrai problème est le flot d'exécution.
    #[test]
    fn bad_rip_takes_priority_over_null_addr() {
        let d = diagnose(&fault(Signal::SIGSEGV, Some(0), 0xBADC0DE), &maps(), false, None, Lang::Fr);
        assert_eq!(d.cause, Cause::BadInstructionPointer);
    }

    #[test]
    fn stack_overflow_detected_just_below_stack() {
        let just_below = 0x7fff0000 - 0x100;
        let d = diagnose(&fault(Signal::SIGSEGV, Some(just_below), 0x400080), &maps(), true, None, Lang::Fr);
        assert_eq!(d.cause, Cause::StackOverflow);
    }

    /// Loin de tout : pointeur fantaisiste, pas un débordement de pile.
    #[test]
    fn far_unmapped_address_is_a_wild_pointer() {
        let d = diagnose(&fault(Signal::SIGSEGV, Some(0x1234_5678_9000), 0x400080), &maps(), false, None, Lang::Fr);
        assert_eq!(d.cause, Cause::WildPointer);
    }

    #[test]
    fn sigfpe_and_sigill_bypass_address_analysis() {
        let d = diagnose(&fault(Signal::SIGFPE, None, 0x400080), &maps(), false, None, Lang::Fr);
        assert_eq!(d.cause, Cause::DivisionByZero);
        assert!(d.hint.contains("cqo"), "doit mentionner l'extension du dividende");

        let d = diagnose(&fault(Signal::SIGILL, None, 0x400080), &maps(), false, None, Lang::Fr);
        assert_eq!(d.cause, Cause::IllegalInstruction);
    }

    /// Les trois langues doivent produire un diagnostic non vide.
    #[test]
    fn all_languages_produce_text() {
        for lang in [Lang::Fr, Lang::En, Lang::Es] {
            let d = diagnose(&fault(Signal::SIGSEGV, Some(0), 0x400080), &maps(), false, None, lang);
            assert!(!d.title.is_empty(), "titre vide en {lang:?}");
            assert!(!d.explanation.is_empty(), "explication vide en {lang:?}");
            assert!(!d.hint.is_empty(), "piste vide en {lang:?}");
            assert!(!cause_label(d.cause, lang).is_empty());
        }
    }

    #[test]
    fn writes_memory_distinguishes_read_from_write() {
        assert!(writes_memory("mov", "[rax], rbx"), "destination mémoire");
        assert!(!writes_memory("mov", "rbx, [rax]"), "source mémoire = lecture");
        assert!(writes_memory("push", "rax"), "push écrit sur la pile");
        assert!(writes_memory("call", "func"), "call empile l'adresse de retour");
        assert!(!writes_memory("cmp", "[rax], rbx"), "cmp ne fait que lire");
        assert!(!writes_memory("test", "[rax], rbx"), "test ne fait que lire");
        assert!(!writes_memory("mov", "rax, rbx"), "aucun accès mémoire");
        assert!(writes_memory("add", "[rsi], 1"), "add en place écrit");
    }
}

/// Tests d'intégration : on fait réellement planter des programmes NASM et on
/// vérifie que le diagnostic tombe juste. C'est le seul moyen de valider la
/// chaîne complète ptrace → siginfo → carte mémoire → cause.
#[cfg(test)]
mod integration {
    use super::*;
    use crate::{assemble, debugger::Debugger, disasm};
    use std::path::Path;

    /// Assemble, exécute jusqu'à la faute, et renvoie le diagnostic.
    fn crash(name: &str) -> Diagnosis {
        let src = format!("examples/{name}.asm");
        let out = assemble::assemble_with_includes(
            Path::new(&src),
            Path::new(&format!("build/diag-{name}")),
            &[],
        )
        .expect("assemblage");
        let insns = disasm::disassemble_text(&out.binary).unwrap_or_default();
        let mut dbg = Debugger::launch(&out.binary).expect("launch");

        for _ in 0..200 {
            if dbg.fault().is_some() || !dbg.is_alive() {
                break;
            }
            dbg.step().expect("step");
        }
        let fault = dbg.fault().unwrap_or_else(|| panic!("{name} devait fauter"));
        let regions = dbg.mem_regions();
        let is_write = insns
            .iter()
            .find(|i| i.address == fault.rip)
            .is_some_and(|i| writes_memory(&i.mnemonic, &i.operands));
        diagnose(&fault, &regions, is_write, None, Lang::Fr)
    }

    #[test]
    fn real_null_dereference() {
        let d = crash("segv-test");
        assert_eq!(d.cause, Cause::NullPointer, "titre obtenu : {}", d.title);
        assert_eq!(d.addr, Some(0), "si_addr doit être 0");
    }

    #[test]
    fn real_write_to_rodata() {
        let d = crash("roret");
        assert_eq!(d.cause, Cause::WriteToReadOnly, "titre obtenu : {}", d.title);
        assert_eq!(d.region, Some(RegionKind::Rodata));
    }

    #[test]
    fn real_unbalanced_ret() {
        let d = crash("badret");
        assert_eq!(d.cause, Cause::BadInstructionPointer, "titre obtenu : {}", d.title);
    }

    #[test]
    fn real_division_by_zero() {
        let d = crash("divzero");
        assert_eq!(d.cause, Cause::DivisionByZero, "titre obtenu : {}", d.title);
    }

    /// Le progrès essentiel : l'exécution s'ARRÊTE sur la faute au lieu de
    /// boucler en silence sur la même instruction avec RIP figé.
    #[test]
    fn execution_halts_instead_of_spinning() {
        let out = assemble::assemble_with_includes(
            Path::new("examples/segv-test.asm"),
            Path::new("build/diag-halt"),
            &[],
        )
        .expect("assemblage");
        let mut dbg = Debugger::launch(&out.binary).expect("launch");
        for _ in 0..10 {
            dbg.step().expect("step");
        }
        assert!(dbg.fault().is_some(), "la faute doit être capturée");
        assert!(!dbg.is_alive(), "le programme ne doit plus être considéré vivant");
        // Le snapshot de la faute est conservé pour inspection.
        let n = dbg.history.len();
        dbg.step().expect("step après faute");
        assert_eq!(dbg.history.len(), n, "plus aucun snapshot ajouté après la faute");
    }
}
