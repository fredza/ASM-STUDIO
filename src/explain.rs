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
    let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);

    // --- Sauts conditionnels : condition évaluée contre les flags réels ---
    if let Some((cond, taken, rel)) = eval_jcc(&m, flags, lang) {
        return Explanation {
            title: format!("{} — {}", mnemonic.to_uppercase(), jcc_title(&m, lang)),
            category: t("Saut conditionnel", "Conditional jump", "Salto condicional"),
            description: format!(
                "{} {}.",
                t(
                    "Saut relatif si la condition est vraie. Cible :",
                    "Relative jump if the condition is true. Target:",
                    "Salto relativo si la condición es verdadera. Destino:"
                ),
                if operands.is_empty() { t("(opérande)", "(operand)", "(operando)") } else { operands }
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
            t("Transfert", "Transfer", "Transferencia"),
            t(
                "Copie la source dans la destination (aucun flag modifié). \
                 Note : écrire dans un registre 32 bits (eax) remet à zéro les 32 bits hauts du 64 bits (rax).",
                "Copies the source into the destination (no flag modified). \
                 Note: writing to a 32-bit register (eax) zeroes the upper 32 bits of the 64-bit register (rax).",
                "Copia la fuente en el destino (sin flags modificados). \
                 Nota: escribir en un registro de 32 bits (eax) pone a cero los 32 bits superiores de 64 bits (rax).",
            ).to_string(),
            vec![],
        ),
        "movabs" => (
            t("Transfert", "Transfer", "Transferencia"),
            t("Charge un immédiat 64 bits complet dans un registre.", "Loads a full 64-bit immediate into a register.", "Carga un inmediato de 64 bits completo en un registro.").to_string(),
            vec![],
        ),
        "lea" => (
            t("Adressage", "Addressing", "Direccionamiento"),
            t(
                "Load Effective Address : calcule une adresse (base + index*échelle + déplacement) \
                 et la place dans la destination, SANS accéder à la mémoire. Sert aussi d'arithmétique rapide.",
                "Load Effective Address: computes an address (base + index*scale + displacement) \
                 and stores it in the destination WITHOUT accessing memory. Also handy for fast arithmetic.",
                "Load Effective Address: calcula una dirección (base + índice*escala + desplazamiento) \
                 y la almacena en el destino SIN acceder a la memoria. También sirve para aritmética rápida.",
            ).to_string(),
            vec![],
        ),
        "push" => (
            t("Pile", "Stack", "Pila"),
            t("Décrémente RSP de 8 puis écrit l'opérande au sommet de la pile.", "Decrements RSP by 8 then writes the operand at the top of the stack.", "Decrementa RSP en 8 y luego escribe el operando en la cima de la pila.").to_string(),
            vec![],
        ),
        "pop" => (
            t("Pile", "Stack", "Pila"),
            t("Lit le sommet de la pile dans la destination puis incrémente RSP de 8.", "Reads the top of the stack into the destination then increments RSP by 8.", "Lee la cima de la pila en el destino y luego incrementa RSP en 8.").to_string(),
            vec![],
        ),
        "add" => (
            t("Arithmétique", "Arithmetic", "Aritmética"),
            t("Additionne source à destination. Positionne les flags selon le résultat.", "Adds source to destination. Sets the flags according to the result.", "Suma la fuente al destino. Posiciona los flags según el resultado.").to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF", "AF"],
        ),
        "sub" => (
            t("Arithmétique", "Arithmetic", "Aritmética"),
            t("Soustrait source de destination. Positionne les flags selon le résultat.", "Subtracts source from destination. Sets the flags according to the result.", "Resta la fuente del destino. Posiciona los flags según el resultado.").to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF", "AF"],
        ),
        "imul" => (
            t("Arithmétique", "Arithmetic", "Aritmética"),
            t("Multiplication signée. CF et OF sont mis à 1 si le résultat déborde de la taille de destination.", "Signed multiplication. CF and OF are set if the result overflows the destination size.", "Multiplicación con signo. CF y OF se ponen a 1 si el resultado desborda el tamaño del destino.").to_string(),
            vec!["CF", "OF"],
        ),
        "mul" => (
            t("Arithmétique", "Arithmetic", "Aritmética"),
            t("Multiplication non signée (RDX:RAX). CF/OF indiquent un débordement dans la partie haute.", "Unsigned multiplication (RDX:RAX). CF/OF indicate overflow into the high part.", "Multiplicación sin signo (RDX:RAX). CF/OF indican desbordamiento en la parte alta.").to_string(),
            vec!["CF", "OF"],
        ),
        "inc" => (
            t("Arithmétique", "Arithmetic", "Aritmética"),
            t("Incrémente de 1. Ne modifie PAS CF (contrairement à add).", "Increments by 1. Does NOT modify CF (unlike add).", "Incrementa en 1. NO modifica CF (al contrario que add).").to_string(),
            vec!["OF", "SF", "ZF", "PF", "AF"],
        ),
        "dec" => (
            t("Arithmétique", "Arithmetic", "Aritmética"),
            t("Décrémente de 1. Ne modifie PAS CF.", "Decrements by 1. Does NOT modify CF.", "Decrementa en 1. NO modifica CF.").to_string(),
            vec!["OF", "SF", "ZF", "PF", "AF"],
        ),
        "neg" => (
            t("Arithmétique", "Arithmetic", "Aritmética"),
            t("Remplace l'opérande par son opposé (complément à deux).", "Replaces the operand with its negation (two's complement).", "Reemplaza el operando por su negativo (complemento a dos).").to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF", "AF"],
        ),
        "cmp" => (
            t("Comparaison", "Comparison", "Comparación"),
            t(
                "Calcule (destination - source) SANS stocker le résultat : seuls les flags sont positionnés. \
                 C'est ce qui prépare un saut conditionnel : ZF=1 si égaux, et SF/OF/CF codent l'ordre.",
                "Computes (destination - source) WITHOUT storing the result: only the flags are set. \
                 This is what prepares a conditional jump: ZF=1 if equal, and SF/OF/CF encode the ordering.",
                "Calcula (destino - fuente) SIN almacenar el resultado: solo se posicionan los flags. \
                 Esto prepara un salto condicional: ZF=1 si iguales, y SF/OF/CF codifican el orden.",
            ).to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF", "AF"],
        ),
        "test" => (
            t("Comparaison", "Comparison", "Comparación"),
            t(
                "Calcule (destination AND source) sans le stocker : positionne les flags. \
                 `test rax, rax` sert à savoir si rax est nul (ZF=1) ou négatif (SF=1).",
                "Computes (destination AND source) without storing it: sets the flags. \
                 `test rax, rax` tells whether rax is zero (ZF=1) or negative (SF=1).",
                "Calcula (destino AND fuente) sin almacenarlo: posiciona los flags. \
                 `test rax, rax` sirve para saber si rax es nulo (ZF=1) o negativo (SF=1).",
            ).to_string(),
            vec!["SF", "ZF", "PF"],
        ),
        "and" => (
            t("Logique", "Logic", "Lógica"),
            t("ET bit à bit. CF et OF sont mis à 0.", "Bitwise AND. CF and OF are cleared.", "AND bit a bit. CF y OF se ponen a 0.").to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF"],
        ),
        "or" => (
            t("Logique", "Logic", "Lógica"),
            t("OU bit à bit. CF et OF sont mis à 0.", "Bitwise OR. CF and OF are cleared.", "OR bit a bit. CF y OF se ponen a 0.").to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF"],
        ),
        "xor" => (
            t("Logique", "Logic", "Lógica"),
            t(
                "OU exclusif bit à bit. `xor rax, rax` est l'idiome pour mettre rax à 0 \
                 (plus court que mov rax, 0). CF et OF sont mis à 0.",
                "Bitwise exclusive OR. `xor rax, rax` is the idiom to zero rax \
                 (shorter than mov rax, 0). CF and OF are cleared.",
                "OR exclusivo bit a bit. `xor rax, rax` es el idioma para poner rax a 0 \
                 (más corto que mov rax, 0). CF y OF se ponen a 0.",
            ).to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF"],
        ),
        "shl" | "sal" => (
            t("Décalage", "Shift", "Desplazamiento"),
            t("Décale les bits vers la gauche (multiplie par 2 par bit). Le dernier bit sorti va dans CF.", "Shifts bits left (multiplies by 2 per bit). The last bit shifted out goes to CF.", "Desplaza los bits hacia la izquierda (multiplica por 2 por bit). El último bit expulsado va a CF.").to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF"],
        ),
        "shr" => (
            t("Décalage", "Shift", "Desplazamiento"),
            t("Décale les bits vers la droite (division non signée par 2). Le dernier bit sorti va dans CF.", "Shifts bits right (unsigned division by 2). The last bit shifted out goes to CF.", "Desplaza los bits hacia la derecha (división sin signo por 2). El último bit expulsado va a CF.").to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF"],
        ),
        "jmp" => (
            t("Saut", "Jump", "Salto"),
            t("Saut inconditionnel : RIP prend la valeur de la cible.", "Unconditional jump: RIP takes the target value.", "Salto incondicional: RIP toma el valor del destino.").to_string(),
            vec![],
        ),
        "call" => (
            t("Appel", "Call", "Llamada"),
            t("Empile l'adresse de retour (RSP -= 8) puis saute vers la fonction cible.", "Pushes the return address (RSP -= 8) then jumps to the target function.", "Apila la dirección de retorno (RSP -= 8) y luego salta a la función destino.").to_string(),
            vec![],
        ),
        "ret" => (
            t("Appel", "Call", "Llamada"),
            t("Dépile l'adresse de retour dans RIP (RSP += 8) : revient à l'appelant.", "Pops the return address into RIP (RSP += 8): returns to the caller.", "Desapila la dirección de retorno en RIP (RSP += 8): regresa al llamador.").to_string(),
            vec![],
        ),
        "syscall" => (
            t("Système", "System", "Sistema"),
            t(
                "Appel système Linux : RAX = numéro, arguments dans RDI, RSI, RDX, R10, R8, R9. \
                 Le noyau exécute l'opération (write, read, exit...) et renvoie le résultat dans RAX.",
                "Linux system call: RAX = number, arguments in RDI, RSI, RDX, R10, R8, R9. \
                 The kernel runs the operation (write, read, exit...) and returns the result in RAX.",
                "Llamada al sistema Linux: RAX = número, argumentos en RDI, RSI, RDX, R10, R8, R9. \
                 El núcleo ejecuta la operación (write, read, exit...) y devuelve el resultado en RAX.",
            ).to_string(),
            vec![],
        ),
        "nop" => (t("Divers", "Misc", "Miscelánea"), t("Ne fait rien (No Operation).", "Does nothing (No Operation).", "No hace nada (No Operation).").to_string(), vec![]),
        "leave" => (
            t("Pile", "Stack", "Pila"),
            t("Équivaut à `mov rsp, rbp ; pop rbp` : démonte le cadre de pile de la fonction.", "Equivalent to `mov rsp, rbp ; pop rbp`: tears down the function's stack frame.", "Equivale a `mov rsp, rbp ; pop rbp`: desmonta el marco de pila de la función.").to_string(),
            vec![],
        ),
        "div" => (
            t("Arithmétique", "Arithmetic", "Aritmética"),
            t(
                "Division NON signée de RDX:RAX (128 bits) par l'opérande : quotient dans RAX, \
                 reste dans RDX. RDX doit être remis à zéro avant (`xor rdx, rdx`), sinon le \
                 dividende est faussé et le quotient déborde — le processeur lève alors une \
                 exception, comme pour une division par zéro.",
                "UNSIGNED division of RDX:RAX (128 bits) by the operand: quotient in RAX, \
                 remainder in RDX. RDX must be cleared first (`xor rdx, rdx`), otherwise the \
                 dividend is wrong and the quotient overflows — the CPU then raises an \
                 exception, just like a division by zero.",
                "División SIN SIGNO de RDX:RAX (128 bits) por el operando: cociente en RAX, \
                 resto en RDX. RDX debe ponerse a cero antes (`xor rdx, rdx`), si no el \
                 dividendo es incorrecto y el cociente desborda — la CPU lanza entonces una \
                 excepción, como en una división por cero.",
            ).to_string(),
            vec![],
        ),
        "idiv" => (
            t("Arithmétique", "Arithmetic", "Aritmética"),
            t(
                "Division SIGNÉE de RDX:RAX par l'opérande : quotient dans RAX, reste dans RDX. \
                 Le dividende doit d'abord être étendu en signe avec `cqo` (et non `xor rdx, rdx`, \
                 qui ne conviendrait qu'au non signé).",
                "SIGNED division of RDX:RAX by the operand: quotient in RAX, remainder in RDX. \
                 The dividend must first be sign-extended with `cqo` (not `xor rdx, rdx`, which \
                 would only suit the unsigned case).",
                "División CON SIGNO de RDX:RAX por el operando: cociente en RAX, resto en RDX. \
                 El dividendo debe extenderse en signo con `cqo` (no `xor rdx, rdx`, que solo \
                 valdría para el caso sin signo).",
            ).to_string(),
            vec![],
        ),
        "sar" => (
            t("Décalage", "Shift", "Desplazamiento"),
            t(
                "Décalage arithmétique à droite : recopie le bit de signe à gauche, ce qui \
                 préserve le signe. Diviser par 2 un nombre signé se fait avec `sar`, pas `shr` \
                 (qui, lui, insère des zéros et transformerait −8 en un très grand positif).",
                "Arithmetic right shift: replicates the sign bit on the left, preserving the \
                 sign. Halving a signed number uses `sar`, not `shr` (which inserts zeros and \
                 would turn −8 into a huge positive).",
                "Desplazamiento aritmético a la derecha: replica el bit de signo a la izquierda, \
                 preservando el signo. Dividir por 2 un número con signo usa `sar`, no `shr` \
                 (que inserta ceros y convertiría −8 en un positivo enorme).",
            ).to_string(),
            vec!["CF", "OF", "SF", "ZF", "PF"],
        ),
        "rol" | "ror" => (
            t("Décalage", "Shift", "Desplazamiento"),
            t(
                "Rotation des bits : ceux qui sortent d'un côté rentrent de l'autre. Aucun bit \
                 n'est perdu, contrairement à un décalage.",
                "Bit rotation: bits leaving one side re-enter on the other. No bit is lost, \
                 unlike a shift.",
                "Rotación de bits: los que salen por un lado entran por el otro. No se pierde \
                 ningún bit, a diferencia de un desplazamiento.",
            ).to_string(),
            vec!["CF", "OF"],
        ),
        "rcl" | "rcr" => (
            t("Décalage", "Shift", "Desplazamiento"),
            t(
                "Rotation à travers la retenue : CF participe au cycle comme un bit supplémentaire.",
                "Rotate through carry: CF takes part in the cycle as an extra bit.",
                "Rotación a través del acarreo: CF participa en el ciclo como un bit extra.",
            ).to_string(),
            vec!["CF", "OF"],
        ),
        "not" => (
            t("Logique", "Logic", "Lógica"),
            t(
                "Inverse tous les bits (complément à un). Aucun flag modifié — contrairement à \
                 `neg`, qui calcule l'opposé arithmétique.",
                "Inverts every bit (one's complement). No flag modified — unlike `neg`, which \
                 computes the arithmetic opposite.",
                "Invierte todos los bits (complemento a uno). Sin flags modificados — a \
                 diferencia de `neg`, que calcula el opuesto aritmético.",
            ).to_string(),
            vec![],
        ),
        "movzx" => (
            t("Transfert", "Transfer", "Transferencia"),
            t(
                "Copie une valeur plus petite en complétant par des ZÉROS (extension non signée). \
                 Sert à lire un octet ou un mot dans un registre 64 bits sans traîner d'anciens bits.",
                "Copies a smaller value padding with ZEROS (unsigned extension). Used to read a \
                 byte or word into a 64-bit register without dragging along old bits.",
                "Copia un valor más pequeño rellenando con CEROS (extensión sin signo). Sirve \
                 para leer un byte o palabra en un registro de 64 bits sin arrastrar bits viejos.",
            ).to_string(),
            vec![],
        ),
        "movsx" | "movsxd" => (
            t("Transfert", "Transfer", "Transferencia"),
            t(
                "Copie une valeur plus petite en recopiant son bit de signe (extension signée). \
                 −1 sur 8 bits (0xFF) devient −1 sur 64 bits, là où `movzx` donnerait 255.",
                "Copies a smaller value replicating its sign bit (signed extension). −1 in 8 bits \
                 (0xFF) becomes −1 in 64 bits, where `movzx` would give 255.",
                "Copia un valor más pequeño replicando su bit de signo (extensión con signo). −1 \
                 en 8 bits (0xFF) pasa a −1 en 64 bits, donde `movzx` daría 255.",
            ).to_string(),
            vec![],
        ),
        "cqo" | "cdq" | "cwd" | "cbw" | "cdqe" | "cwde" => (
            t("Arithmétique", "Arithmetic", "Aritmética"),
            t(
                "Étend le signe du dividende avant une division signée : recopie le bit de signe \
                 de RAX dans tout RDX. À placer systématiquement avant `idiv`.",
                "Sign-extends the dividend before a signed division: replicates RAX's sign bit \
                 across all of RDX. Always place it before `idiv`.",
                "Extiende el signo del dividendo antes de una división con signo: replica el bit \
                 de signo de RAX en todo RDX. Colócalo siempre antes de `idiv`.",
            ).to_string(),
            vec![],
        ),
        "adc" => (
            t("Arithmétique", "Arithmetic", "Aritmética"),
            t(
                "Addition avec retenue : ajoute aussi CF. Sert à chaîner des additions sur plus \
                 de 64 bits.",
                "Add with carry: also adds CF. Used to chain additions beyond 64 bits.",
                "Suma con acarreo: añade también CF. Sirve para encadenar sumas de más de 64 bits.",
            ).to_string(),
            vec!["CF", "OF", "SF", "ZF", "AF", "PF"],
        ),
        "sbb" => (
            t("Arithmétique", "Arithmetic", "Aritmética"),
            t(
                "Soustraction avec emprunt : retranche aussi CF. Pendant de `adc` pour les \
                 soustractions multi-mots.",
                "Subtract with borrow: also subtracts CF. Counterpart of `adc` for multi-word \
                 subtraction.",
                "Resta con préstamo: resta también CF. Contraparte de `adc` para restas multi-palabra.",
            ).to_string(),
            vec!["CF", "OF", "SF", "ZF", "AF", "PF"],
        ),
        "xchg" => (
            t("Transfert", "Transfer", "Transferencia"),
            t(
                "Échange le contenu des deux opérandes en une instruction. Sur une adresse \
                 mémoire, l'opération est atomique — d'où son usage pour les verrous.",
                "Swaps both operands' contents in one instruction. On a memory address the \
                 operation is atomic — hence its use for locks.",
                "Intercambia el contenido de ambos operandos en una instrucción. Sobre memoria \
                 la operación es atómica — de ahí su uso para cerrojos.",
            ).to_string(),
            vec![],
        ),
        "bswap" => (
            t("Transfert", "Transfer", "Transferencia"),
            t(
                "Inverse l'ordre des octets du registre : convertit entre petit-boutisme et \
                 gros-boutisme (utile pour les formats réseau).",
                "Reverses the register's byte order: converts between little-endian and \
                 big-endian (useful for network formats).",
                "Invierte el orden de bytes del registro: convierte entre little-endian y \
                 big-endian (útil para formatos de red).",
            ).to_string(),
            vec![],
        ),
        "loop" | "loope" | "loopz" | "loopne" | "loopnz" => (
            t("Boucle", "Loop", "Bucle"),
            t(
                "Décrémente RCX puis saute si RCX ≠ 0. Compact, mais moins rapide qu'un \
                 `dec` suivi d'un `jnz` sur les processeurs modernes.",
                "Decrements RCX then jumps if RCX ≠ 0. Compact, but slower than a `dec` \
                 followed by `jnz` on modern CPUs.",
                "Decrementa RCX y luego salta si RCX ≠ 0. Compacto, pero más lento que un `dec` \
                 seguido de `jnz` en CPU modernas.",
            ).to_string(),
            vec![],
        ),
        "bt" | "bts" | "btr" | "btc" => (
            t("Logique", "Logic", "Lógica"),
            t(
                "Teste un bit précis et le recopie dans CF ; les variantes le mettent à 1 (bts), \
                 à 0 (btr) ou l'inversent (btc).",
                "Tests a specific bit and copies it into CF; variants set it (bts), clear it \
                 (btr) or flip it (btc).",
                "Prueba un bit concreto y lo copia en CF; las variantes lo ponen a 1 (bts), a 0 \
                 (btr) o lo invierten (btc).",
            ).to_string(),
            vec!["CF"],
        ),
        "bsf" | "bsr" | "tzcnt" | "lzcnt" => (
            t("Logique", "Logic", "Lógica"),
            t(
                "Cherche la position du premier bit à 1 (depuis le bas pour bsf/tzcnt, depuis le \
                 haut pour bsr/lzcnt).",
                "Finds the position of the first set bit (from the bottom for bsf/tzcnt, from the \
                 top for bsr/lzcnt).",
                "Busca la posición del primer bit a 1 (desde abajo para bsf/tzcnt, desde arriba \
                 para bsr/lzcnt).",
            ).to_string(),
            vec!["ZF"],
        ),
        "popcnt" => (
            t("Logique", "Logic", "Lógica"),
            t(
                "Compte le nombre de bits à 1 dans l'opérande.",
                "Counts the number of set bits in the operand.",
                "Cuenta el número de bits a 1 en el operando.",
            ).to_string(),
            vec!["ZF"],
        ),
        "enter" => (
            t("Pile", "Stack", "Pila"),
            t(
                "Monte le cadre de pile d'une fonction (équivaut à `push rbp ; mov rbp, rsp` plus \
                 une réservation). Pendant de `leave`.",
                "Sets up a function's stack frame (equivalent to `push rbp ; mov rbp, rsp` plus a \
                 reservation). Counterpart of `leave`.",
                "Monta el marco de pila de una función (equivale a `push rbp ; mov rbp, rsp` más \
                 una reserva). Contraparte de `leave`.",
            ).to_string(),
            vec![],
        ),
        "pushf" | "pushfq" | "popf" | "popfq" => (
            t("Pile", "Stack", "Pila"),
            t(
                "Empile ou dépile le registre des flags (RFLAGS) : permet de sauvegarder puis de \
                 restaurer l'état des comparaisons.",
                "Pushes or pops the flags register (RFLAGS): lets you save and restore the state \
                 of comparisons.",
                "Apila o desapila el registro de flags (RFLAGS): permite guardar y restaurar el \
                 estado de las comparaciones.",
            ).to_string(),
            vec![],
        ),
        "cld" | "std" => (
            t("Chaînes", "String", "Cadenas"),
            t(
                "Fixe le sens de parcours des instructions de chaîne : `cld` avance (DF = 0), \
                 `std` recule (DF = 1).",
                "Sets the direction for string instructions: `cld` forward (DF = 0), `std` \
                 backward (DF = 1).",
                "Fija el sentido de las instrucciones de cadena: `cld` avanza (DF = 0), `std` \
                 retrocede (DF = 1).",
            ).to_string(),
            vec![],
        ),
        "endbr64" | "endbr32" => (
            t("Divers", "Misc", "Miscelánea"),
            t(
                "Marque une cible de saut indirect autorisée (protection CET du processeur). \
                 Ne fait rien d'autre : à traiter comme un `nop`.",
                "Marks a permitted indirect-branch target (CPU CET protection). Does nothing \
                 else: treat it as a `nop`.",
                "Marca un destino de salto indirecto permitido (protección CET de la CPU). No \
                 hace nada más: trátalo como un `nop`.",
            ).to_string(),
            vec![],
        ),
        "int3" | "int" | "ud2" | "hlt" => (
            t("Système", "System", "Sistema"),
            t(
                "Interrompt l'exécution : point d'arrêt (`int3`), instruction volontairement \
                 invalide (`ud2`) ou arrêt du processeur (`hlt`).",
                "Interrupts execution: breakpoint (`int3`), deliberately invalid instruction \
                 (`ud2`) or CPU halt (`hlt`).",
                "Interrumpe la ejecución: punto de parada (`int3`), instrucción inválida a \
                 propósito (`ud2`) o parada de la CPU (`hlt`).",
            ).to_string(),
            vec![],
        ),
        // Familles reconnues par préfixe : setcc, cmovcc, et les opérations de chaîne.
        _ if m.starts_with("set") && m.len() <= 6 => (
            t("Comparaison", "Comparison", "Comparación"),
            t(
                "Écrit 1 dans un octet si la condition est vraie, 0 sinon. Convertit un résultat \
                 de comparaison en valeur, sans saut.",
                "Writes 1 into a byte if the condition is true, 0 otherwise. Turns a comparison \
                 result into a value, without branching.",
                "Escribe 1 en un byte si la condición es verdadera, 0 si no. Convierte un \
                 resultado de comparación en valor, sin saltar.",
            ).to_string(),
            vec![],
        ),
        _ if m.starts_with("cmov") => (
            t("Transfert", "Transfer", "Transferencia"),
            t(
                "Copie conditionnelle : la copie n'a lieu que si la condition est vraie. Évite un \
                 saut, donc une éventuelle mauvaise prédiction de branchement.",
                "Conditional move: the copy only happens if the condition is true. Avoids a \
                 branch, and thus a possible misprediction.",
                "Copia condicional: la copia solo ocurre si la condición es verdadera. Evita un \
                 salto, y por tanto una posible predicción errónea.",
            ).to_string(),
            vec![],
        ),
        _ if matches!(
            m.trim_end_matches(['b', 'w', 'd', 'q']),
            "movs" | "stos" | "lods" | "scas" | "cmps"
        ) =>
        (
            t("Chaînes", "String", "Cadenas"),
            t(
                "Opération de chaîne : travaille sur [RSI] et/ou [RDI] puis avance ces pointeurs \
                 automatiquement (selon DF). Préfixée par `rep`, elle se répète RCX fois.",
                "String operation: works on [RSI] and/or [RDI] then advances those pointers \
                 automatically (per DF). Prefixed with `rep`, it repeats RCX times.",
                "Operación de cadena: trabaja sobre [RSI] y/o [RDI] y luego avanza esos punteros \
                 automáticamente (según DF). Con el prefijo `rep`, se repite RCX veces.",
            ).to_string(),
            vec![],
        ),
        _ => (
            t("Inconnu", "Unknown", "Desconocido"),
            format!(
                "{} « {mnemonic} » {}",
                t("Instruction", "Instruction", "Instrucción"),
                t(": explication non encore répertoriée.", ": explanation not catalogued yet.", ": explicación aún no catalogada."),
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
        "movzx" | "movsx" | "movsxd" | "cqo" | "cdq" | "cwd" | "cbw" | "cdqe" | "cwde"
        | "adc" | "sbb" | "rol" | "ror" | "bswap" | "endbr64" => "~1",
        "rcl" | "rcr" => "~2–3",
        "cld" | "std" => "~2–4",
        "int" | "int3" | "int1" | "ud2" | "hlt" => "≈ arrêt",
        "push" | "pop" | "jmp" | "xchg" => "~1–2",
        "bt" | "bts" | "btr" | "btc" | "bsf" | "bsr" | "tzcnt" | "lzcnt" | "popcnt" => "~1–3",
        "loop" | "loope" | "loopz" | "loopne" | "loopnz" => "~2–5",
        "enter" | "leave" | "pushfq" | "popfq" => "~2–5",
        "je" | "jne" | "jz" | "jnz" | "jg" | "jge" | "jl" | "jle" | "ja" | "jae" | "jb" | "jbe"
        | "js" | "jns" | "jo" | "jno" | "jp" | "jnp" => "~1–2 (0 si bien prédit)",
        "call" | "ret" => "~1–3",
        "imul" | "mul" => "~3–5",
        "div" | "idiv" => "~20–40",
        "syscall" => "~100+ (bascule noyau)",
        m if m.starts_with("cmov") => "~1",
        // Opérations de chaîne : coût par élément, multiplié par RCX avec `rep`.
        m if matches!(
            m.trim_end_matches(['b', 'w', 'd', 'q']),
            "movs" | "stos" | "lods" | "scas" | "cmps"
        ) => "~1–5 par élément",
        m if m.starts_with("set") && m.len() <= 6 => "~1",
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
        "movsx" | "movsxd" => "movsx:movsxd",
        "cwd" | "cdq" | "cqo" => "cwd:cdq:cqo",
        "cbw" | "cwde" | "cdqe" => "cbw:cwde:cdqe",
        "pushf" | "pushfq" => "pushf:pushfd:pushfq",
        "popf" | "popfq" => "popf:popfd:popfq",
        "loop" | "loope" | "loopz" | "loopne" | "loopnz" => "loop:loopcc",
        "int" | "int3" | "int1" => "intn:into:int3:int1",
        // Familles conditionnelles et opérations de chaîne : une page par famille.
        _ if m.starts_with("cmov") => "cmovcc",
        _ if m.starts_with("set") && m.len() <= 6 => "setcc",
        "movsb" | "movsw" | "movsq" => "movs:movsb:movsw:movsd:movsq",
        "stos" | "stosb" | "stosw" | "stosd" | "stosq" => "stos:stosb:stosw:stosd:stosq",
        "lods" | "lodsb" | "lodsw" | "lodsd" | "lodsq" => "lods:lodsb:lodsw:lodsd:lodsq",
        "scas" | "scasb" | "scasw" | "scasd" | "scasq" => "scas:scasb:scasw:scasd:scasq",
        "cmpsb" | "cmpsw" | "cmpsq" => "cmps:cmpsb:cmpsw:cmpsd:cmpsq",
        // Par défaut, le mnémonique EST le slug (couvre mov, add, sub, and, or,
        // xor, cmp, test, lea, push, pop, call, ret, inc, dec, neg, not, mul,
        // imul, div, idiv, nop, syscall, jmp…).
        other => return format!("https://www.felixcloutier.com/x86/{other}"),
    };
    format!("https://www.felixcloutier.com/x86/{slug}")
}

/// Titre lisible d'un saut conditionnel.
fn jcc_title(m: &str, lang: Lang) -> &'static str {
    let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
    match m {
        "je" | "jz" => "Jump if Equal / Zero",
        "jne" | "jnz" => "Jump if Not Equal / Not Zero",
        "jg" | "jnle" => t("Jump if Greater (signé)", "Jump if Greater (signed)", "Jump if Greater (con signo)"),
        "jge" | "jnl" => t("Jump if Greater or Equal (signé)", "Jump if Greater or Equal (signed)", "Jump if Greater or Equal (con signo)"),
        "jl" | "jnge" => t("Jump if Less (signé)", "Jump if Less (signed)", "Jump if Less (con signo)"),
        "jle" | "jng" => t("Jump if Less or Equal (signé)", "Jump if Less or Equal (signed)", "Jump if Less or Equal (con signo)"),
        "ja" | "jnbe" => t("Jump if Above (non signé)", "Jump if Above (unsigned)", "Jump if Above (sin signo)"),
        "jae" | "jnb" | "jnc" => t("Jump if Above or Equal (non signé)", "Jump if Above or Equal (unsigned)", "Jump if Above or Equal (sin signo)"),
        "jb" | "jc" | "jnae" => t("Jump if Below (non signé)", "Jump if Below (unsigned)", "Jump if Below (sin signo)"),
        "jbe" | "jna" => t("Jump if Below or Equal (non signé)", "Jump if Below or Equal (unsigned)", "Jump if Below or Equal (sin signo)"),
        "js" => t("Jump if Sign (négatif)", "Jump if Sign (negative)", "Jump if Sign (negativo)"),
        "jns" => t("Jump if Not Sign (positif ou nul)", "Jump if Not Sign (positive or zero)", "Jump if Not Sign (positivo o cero)"),
        "jo" => "Jump if Overflow",
        "jno" => "Jump if Not Overflow",
        "jp" | "jpe" => "Jump if Parity Even",
        "jnp" | "jpo" => "Jump if Parity Odd",
        _ => t("Saut conditionnel", "Conditional jump", "Salto condicional"),
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
    let and = i18n::tr3(lang, "et", "and", "y");
    let or = i18n::tr3(lang, "ou", "or", "o");

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

    /// Le panneau INSTRUCTION ne doit jamais rester muet sur une instruction
    /// courante : chaque famille doit avoir une explication et une catégorie
    /// autres que « Inconnu ».
    #[test]
    fn common_instructions_are_all_covered() {
        let mnemonics = [
            // Transfert et adressage
            "mov", "movabs", "lea", "movzx", "movsx", "movsxd", "xchg", "bswap",
            // Pile
            "push", "pop", "leave", "enter", "pushfq", "popfq",
            // Arithmétique
            "add", "sub", "imul", "mul", "div", "idiv", "inc", "dec", "neg",
            "adc", "sbb", "cqo", "cdq", "cwd", "cbw", "cdqe",
            // Logique et bits
            "and", "or", "xor", "not", "test", "bt", "bts", "btr", "btc",
            "bsf", "bsr", "popcnt", "tzcnt", "lzcnt",
            // Décalages
            "shl", "sal", "shr", "sar", "rol", "ror", "rcl", "rcr",
            // Contrôle
            "jmp", "call", "ret", "cmp", "loop", "loopne",
            // Conditionnelles sans saut
            "sete", "setne", "setl", "setge", "setz", "seta",
            "cmove", "cmovne", "cmovl", "cmovge",
            // Chaînes
            "movsb", "stosb", "lodsb", "scasb", "cmpsb", "stosq",
            // Système et divers
            "syscall", "nop", "endbr64", "int3", "ud2", "hlt", "cld", "std",
        ];
        let flags = Flags::default();
        for m in mnemonics {
            let e = explain(m, "rax, rbx", flags, Lang::Fr);
            assert_ne!(
                e.category,
                i18n::tr3(Lang::Fr, "Inconnu", "Unknown", "Desconocido"),
                "« {m} » n'a pas d'explication"
            );
            assert!(!e.description.is_empty(), "« {m} » : description vide");
            assert!(
                !e.description.contains("non encore répertoriée"),
                "« {m} » retombe sur le texte par défaut"
            );
            // Un ordre de grandeur de cycles doit être proposé.
            assert_ne!(cycles_estimate(m), "≈ variable", "« {m} » : cycles non estimés");
            // Et un lien de documentation plausible.
            let url = doc_url(m);
            assert!(url.starts_with("https://www.felixcloutier.com/x86/"), "{m} → {url}");
        }
    }

    /// Les trois langues doivent produire un texte pour chaque famille.
    #[test]
    fn new_families_are_translated() {
        for m in ["div", "sar", "movzx", "movsx", "cqo", "sete", "cmove", "stosb", "endbr64"] {
            for lang in [Lang::Fr, Lang::En, Lang::Es] {
                let e = explain(m, "", Flags::default(), lang);
                assert!(!e.description.is_empty(), "{m} vide en {lang:?}");
                assert!(
                    !e.description.contains("non encore répertoriée")
                        && !e.description.contains("not catalogued")
                        && !e.description.contains("no catalogada"),
                    "{m} non traduit en {lang:?}"
                );
            }
        }
    }

    /// La distinction shr/sar est un piège classique : les deux explications
    /// doivent différer et sar doit parler du signe.
    #[test]
    fn shr_and_sar_are_distinguished() {
        let shr = explain("shr", "rax, 1", Flags::default(), Lang::Fr);
        let sar = explain("sar", "rax, 1", Flags::default(), Lang::Fr);
        assert_ne!(shr.description, sar.description);
        assert!(sar.description.contains("signe"), "sar doit expliquer le signe");
    }

    /// div (non signé) et idiv (signé) ne doivent pas donner le même conseil
    /// d'extension du dividende — c'est la cause d'erreur la plus fréquente.
    #[test]
    fn div_and_idiv_give_different_advice() {
        let div = explain("div", "rcx", Flags::default(), Lang::Fr);
        let idiv = explain("idiv", "rcx", Flags::default(), Lang::Fr);
        assert!(div.description.contains("xor rdx, rdx"), "div → mise à zéro de RDX");
        assert!(idiv.description.contains("cqo"), "idiv → extension de signe");
        assert_ne!(div.description, idiv.description);
    }

    /// Un mnémonique vraiment inconnu doit encore retomber proprement sur le
    /// texte par défaut, sans paniquer.
    #[test]
    fn unknown_mnemonic_still_falls_back() {
        let e = explain("vfmadd231pd", "", Flags::default(), Lang::Fr);
        assert_eq!(e.category, i18n::tr3(Lang::Fr, "Inconnu", "Unknown", "Desconocido"));
        assert!(e.description.contains("vfmadd231pd"));
    }
}
