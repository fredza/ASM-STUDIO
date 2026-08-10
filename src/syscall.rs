//! Décodage lisible des appels système Linux x86-64.
//!
//! Convention : RAX = numéro, arguments dans RDI, RSI, RDX, R10, R8, R9,
//! valeur de retour dans RAX.
//!
//! Deux niveaux de lecture cohabitent ici. [`format_call`] rend l'appel tel
//! qu'un `strace` l'écrirait — compact, brut, bon pour un journal. [`describe`]
//! va plus loin : il dit en français (ou en anglais, ou en espagnol) ce que
//! l'appel VA FAIRE, argument par argument. `write(fd=1, buf=0x402000,
//! count=13)` ne dit rien à qui débute ; « écrit 13 octets pris à l'adresse
//! 0x402000 sur la sortie standard (l'écran) » se comprend sans manuel. C'est
//! la différence entre lire les registres et comprendre l'appel.

use crate::debugger::Registers;
use crate::i18n::{self, Lang};

/// Nom de l'appel système d'après son numéro.
///
/// La table couvre ce qu'un programme écrit à la main peut appeler : les
/// entrées/sorties, les fichiers, la mémoire, les processus, le temps. Les
/// numéros absents rendent `"syscall"` — l'appel reste montré, sans nom.
pub fn name(num: u64) -> &'static str {
    match num {
        // Entrées / sorties et fichiers
        0 => "read",
        1 => "write",
        2 => "open",
        3 => "close",
        4 => "stat",
        5 => "fstat",
        6 => "lstat",
        7 => "poll",
        8 => "lseek",
        16 => "ioctl",
        17 => "pread64",
        18 => "pwrite64",
        19 => "readv",
        20 => "writev",
        21 => "access",
        22 => "pipe",
        23 => "select",
        32 => "dup",
        33 => "dup2",
        72 => "fcntl",
        74 => "fsync",
        76 => "truncate",
        77 => "ftruncate",
        78 => "getdents",
        79 => "getcwd",
        80 => "chdir",
        82 => "rename",
        83 => "mkdir",
        84 => "rmdir",
        85 => "creat",
        86 => "link",
        87 => "unlink",
        88 => "symlink",
        89 => "readlink",
        90 => "chmod",
        92 => "chown",
        257 => "openat",
        262 => "newfstatat",
        293 => "pipe2",
        // Mémoire
        9 => "mmap",
        10 => "mprotect",
        11 => "munmap",
        12 => "brk",
        25 => "mremap",
        26 => "msync",
        // Signaux
        13 => "rt_sigaction",
        14 => "rt_sigprocmask",
        15 => "rt_sigreturn",
        37 => "alarm",
        62 => "kill",
        // Processus
        24 => "sched_yield",
        39 => "getpid",
        56 => "clone",
        57 => "fork",
        58 => "vfork",
        59 => "execve",
        60 => "exit",
        61 => "wait4",
        102 => "getuid",
        104 => "getgid",
        107 => "geteuid",
        110 => "getppid",
        231 => "exit_group",
        // Temps
        34 => "pause",
        35 => "nanosleep",
        96 => "gettimeofday",
        100 => "times",
        201 => "time",
        228 => "clock_gettime",
        230 => "clock_nanosleep",
        // Réseau (rare en cours, mais reconnaissable)
        41 => "socket",
        42 => "connect",
        43 => "accept",
        44 => "sendto",
        45 => "recvfrom",
        49 => "bind",
        50 => "listen",
        // Divers
        63 => "uname",
        158 => "arch_prctl",
        186 => "gettid",
        202 => "futex",
        318 => "getrandom",
        _ => "syscall",
    }
}

/// Formate l'appel tel qu'il est SUR LE POINT de s'exécuter (registres avant).
pub fn format_call(regs: &Registers) -> String {
    match regs.rax {
        0 => format!("read(fd={}, buf=0x{:X}, count={})", regs.rdi, regs.rsi, regs.rdx),
        1 => format!("write(fd={}, buf=0x{:X}, count={})", regs.rdi, regs.rsi, regs.rdx),
        2 => format!("open(path=0x{:X}, flags=0x{:X})", regs.rdi, regs.rsi),
        3 => format!("close(fd={})", regs.rdi),
        8 => format!("lseek(fd={}, offset={}, whence={})", regs.rdi, regs.rsi as i64, regs.rdx),
        9 => format!("mmap(addr=0x{:X}, len={}, prot={})", regs.rdi, regs.rsi, regs.rdx),
        11 => format!("munmap(addr=0x{:X}, len={})", regs.rdi, regs.rsi),
        12 => format!("brk(addr=0x{:X})", regs.rdi),
        33 => format!("dup2(oldfd={}, newfd={})", regs.rdi, regs.rsi),
        59 => format!("execve(path=0x{:X}, argv=0x{:X}, envp=0x{:X})", regs.rdi, regs.rsi, regs.rdx),
        60 => format!("exit({})", regs.rdi),
        62 => format!("kill(pid={}, sig={})", regs.rdi, regs.rsi),
        87 => format!("unlink(path=0x{:X})", regs.rdi),
        231 => format!("exit_group({})", regs.rdi),
        257 => format!("openat(dirfd={}, path=0x{:X}, flags=0x{:X})", regs.rdi as i64, regs.rsi, regs.rdx),
        318 => format!("getrandom(buf=0x{:X}, len={})", regs.rdi, regs.rsi),
        n => format!("{}(rdi={}, rsi={}, rdx={})", name(n), regs.rdi, regs.rsi, regs.rdx),
    }
}

/// Vrai si l'appel termine le processus (pas de valeur de retour à afficher).
pub fn is_exit(num: u64) -> bool {
    num == 60 || num == 231
}

// ---------------------------------------------------------------------------
// Description en langue courante
// ---------------------------------------------------------------------------

/// Un argument de l'appel, décodé : le registre qui le porte, le rôle qu'il
/// joue dans CET appel, et sa valeur telle qu'on peut la lire.
pub struct ArgLine {
    /// Registre source, majuscules (« RDI »).
    pub reg: &'static str,
    /// Nom de l'argument dans le manuel (« fd », « count »).
    pub param: &'static str,
    /// Ce que l'argument désigne, en toutes lettres.
    pub role: String,
    /// La valeur, formatée selon sa nature (décimale, hexa, symbolique).
    pub value: String,
}

/// Zone mémoire que l'appel lit ou remplit. L'interface s'en sert pour montrer
/// le contenu réel du tampon : c'est là que `msg` cesse d'être une adresse
/// abstraite pour redevenir le texte qu'on a tapé dans `.data`.
pub struct Buffer {
    pub addr: u64,
    pub len: usize,
    /// Intitulé de la zone (« Texte écrit », « Tampon de réception »).
    pub label: &'static str,
    /// Vrai si le contenu se lit comme du texte (write/read) plutôt que comme
    /// des octets bruts.
    pub as_text: bool,
}

/// Ce que l'appel va faire, expliqué.
pub struct Description {
    pub name: &'static str,
    /// Une phrase : l'effet de l'appel avec ses valeurs présentes.
    pub summary: String,
    /// Le détail argument par argument.
    pub args: Vec<ArgLine>,
    /// Ce que RAX vaudra au retour, et comment le lire. `None` pour les appels
    /// qui ne reviennent pas.
    pub ret: Option<String>,
    /// Tampon à montrer, s'il y en a un.
    pub buffer: Option<Buffer>,
    /// Piège éventuel de cet appel-ci, avec ces valeurs-là.
    pub note: Option<String>,
}

/// Accord en nombre pour « octet », les trois langues.
fn bytes_word(n: u64, lang: Lang) -> String {
    match lang {
        Lang::Fr => format!("{n} octet{}", if n > 1 { "s" } else { "" }),
        Lang::En => format!("{n} byte{}", if n > 1 { "s" } else { "" }),
        Lang::Es => format!("{n} byte{}", if n > 1 { "s" } else { "" }),
    }
}

/// Ce que désigne un descripteur de fichier, pour les trois qui sont ouverts
/// d'office à tout processus.
fn fd_label(fd: u64, lang: Lang) -> String {
    let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
    match fd {
        0 => t("l'entrée standard (le clavier)", "standard input (the keyboard)", "la entrada estándar (el teclado)").to_string(),
        1 => t("la sortie standard (l'écran)", "standard output (the screen)", "la salida estándar (la pantalla)").to_string(),
        2 => t("la sortie d'erreur", "standard error", "la salida de error").to_string(),
        n => format!("{} {n}", t("le descripteur de fichier n°", "file descriptor #", "el descriptor de archivo n.º")),
    }
}

/// Les drapeaux d'`open`, en clair (`0x241` → `O_WRONLY|O_CREAT|O_TRUNC`).
fn open_flags(flags: u64) -> String {
    let mut parts = vec![match flags & 0b11 {
        1 => "O_WRONLY",
        2 => "O_RDWR",
        _ => "O_RDONLY",
    }
    .to_string()];
    for (bit, n) in [(0o100, "O_CREAT"), (0o1000, "O_TRUNC"), (0o2000, "O_APPEND"), (0o200, "O_EXCL"), (0o4000, "O_NONBLOCK")] {
        if flags & bit != 0 {
            parts.push(n.to_string());
        }
    }
    parts.join("|")
}

/// Les protections d'une page mémoire (`mmap`, `mprotect`).
fn prot_flags(prot: u64) -> String {
    if prot == 0 {
        return "PROT_NONE".to_string();
    }
    let mut parts = Vec::new();
    for (bit, n) in [(1, "PROT_READ"), (2, "PROT_WRITE"), (4, "PROT_EXEC")] {
        if prot & bit != 0 {
            parts.push(n);
        }
    }
    parts.join("|")
}

/// Rend un tampon lisible : le texte tel quel, les caractères de contrôle
/// échappés (`\n` reste visible au lieu de casser la ligne), le reste en `·`.
/// Tronqué au-delà de `max` caractères — un aperçu, pas un vidage mémoire.
pub fn text_preview(bytes: &[u8], max: usize) -> String {
    let mut out = String::new();
    for &b in bytes.iter().take(max) {
        match b {
            b'\n' => out.push_str("\\n"),
            b'\t' => out.push_str("\\t"),
            b'\r' => out.push_str("\\r"),
            0 => out.push_str("\\0"),
            0x20..=0x7E => out.push(b as char),
            _ => out.push('·'),
        }
    }
    if bytes.len() > max {
        out.push('…');
    }
    out
}

/// En une phrase, ce que fait cet appel — indépendamment de ses arguments.
///
/// C'est le filet de la bibliothèque : tout appel que [`name`] sait nommer
/// dit au moins à quoi il sert, même quand [`describe`] n'en détaille pas les
/// arguments. Sans cela, la moitié de la table rendrait un nom nu, ce qui
/// n'apprend rien de plus que le numéro.
pub fn gist(num: u64, lang: Lang) -> Option<&'static str> {
    let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
    Some(match num {
        0 => t("Lit des octets depuis un descripteur ouvert.", "Reads bytes from an open descriptor.", "Lee bytes de un descriptor abierto."),
        1 => t("Écrit des octets sur un descripteur ouvert.", "Writes bytes to an open descriptor.", "Escribe bytes en un descriptor abierto."),
        2 | 257 | 85 => t("Ouvre un fichier et rend un descripteur pour y accéder.", "Opens a file and returns a descriptor to reach it.", "Abre un archivo y devuelve un descriptor para acceder a él."),
        3 => t("Referme un descripteur : le noyau libère ce qu'il tenait pour lui.", "Closes a descriptor: the kernel releases what it held for it.", "Cierra un descriptor: el núcleo libera lo que mantenía para él."),
        4 | 5 | 6 | 262 => t("Interroge les métadonnées d'un fichier : taille, droits, dates.", "Queries a file's metadata: size, permissions, dates.", "Consulta los metadatos de un archivo: tamaño, permisos, fechas."),
        7 | 23 => t("Attend qu'au moins un descripteur d'une liste soit prêt.", "Waits until at least one descriptor in a list is ready.", "Espera a que al menos un descriptor de una lista esté listo."),
        8 => t("Déplace la position de lecture/écriture dans un fichier.", "Moves the read/write position inside a file.", "Mueve la posición de lectura/escritura dentro de un archivo."),
        9 => t("Demande au noyau une nouvelle zone de mémoire.", "Asks the kernel for a new memory area.", "Solicita al núcleo una nueva zona de memoria."),
        10 => t("Change les droits (lecture/écriture/exécution) d'une zone mémoire.", "Changes the permissions (read/write/execute) of a memory area.", "Cambia los permisos (lectura/escritura/ejecución) de una zona de memoria."),
        11 => t("Rend au noyau une zone de mémoire obtenue plus tôt.", "Gives back to the kernel a memory area obtained earlier.", "Devuelve al núcleo una zona de memoria obtenida antes."),
        12 => t("Déplace la fin du tas : la façon la plus simple d'obtenir de la mémoire.", "Moves the end of the heap: the simplest way to get memory.", "Mueve el final del montón: la forma más simple de obtener memoria."),
        13..=15 => t("Règle la réaction du programme aux signaux (Ctrl-C, erreurs…).", "Sets how the program reacts to signals (Ctrl-C, faults…).", "Configura la reacción del programa a las señales (Ctrl-C, fallos…)."),
        16 => t("Envoie une commande particulière à un périphérique (terminal, disque…).", "Sends a device-specific command (terminal, disk…).", "Envía una orden particular a un periférico (terminal, disco…)."),
        17 | 18 => t("Lit ou écrit à une position donnée, sans déplacer la position courante.", "Reads or writes at a given position, without moving the current one.", "Lee o escribe en una posición dada, sin mover la posición actual."),
        19 | 20 => t("Lit ou écrit plusieurs zones mémoire en un seul appel.", "Reads or writes several memory areas in a single call.", "Lee o escribe varias zonas de memoria en una sola llamada."),
        21 => t("Vérifie les droits d'accès à un fichier sans l'ouvrir.", "Checks access rights on a file without opening it.", "Comprueba los permisos de acceso a un archivo sin abrirlo."),
        22 | 293 => t("Crée un tube : deux descripteurs reliés, pour faire circuler des octets.", "Creates a pipe: two linked descriptors, to pass bytes along.", "Crea una tubería: dos descriptores enlazados, para hacer circular bytes."),
        24 => t("Rend la main au noyau : laisse un autre programme s'exécuter.", "Yields to the kernel: lets another program run.", "Cede el control al núcleo: deja que otro programa se ejecute."),
        25 => t("Redimensionne une zone mémoire déjà obtenue.", "Resizes a memory area already obtained.", "Redimensiona una zona de memoria ya obtenida."),
        26 | 74 => t("Force l'écriture réelle sur le disque de ce qui était en cache.", "Forces cached data to be really written to disk.", "Fuerza la escritura real en disco de lo que estaba en caché."),
        32 | 33 => t("Duplique un descripteur : deux numéros pour la même destination.", "Duplicates a descriptor: two numbers for the same destination.", "Duplica un descriptor: dos números para el mismo destino."),
        34 => t("Suspend le programme jusqu'à l'arrivée d'un signal.", "Suspends the program until a signal arrives.", "Suspende el programa hasta que llegue una señal."),
        35 | 230 => t("Suspend le programme pour une durée précise.", "Suspends the program for a precise duration.", "Suspende el programa durante un tiempo preciso."),
        37 => t("Programme l'envoi d'un signal à soi-même après un délai.", "Schedules a signal to oneself after a delay.", "Programa el envío de una señal a sí mismo tras un retardo."),
        39 | 110 | 186 => t("Demande un numéro d'identité au noyau (processus, parent, thread).", "Asks the kernel for an identity number (process, parent, thread).", "Pide al núcleo un número de identidad (proceso, padre, hilo)."),
        41..=45 | 49 | 50 => t("Opération réseau : le noyau gère la connexion pour le programme.", "Network operation: the kernel handles the connection for the program.", "Operación de red: el núcleo gestiona la conexión por el programa."),
        56..=58 => t("Crée un second processus, copie de celui-ci.", "Creates a second process, a copy of this one.", "Crea un segundo proceso, copia de este."),
        59 => t("Remplace le programme en cours par un autre, sans changer de processus.", "Replaces the running program with another one, keeping the same process.", "Reemplaza el programa en curso por otro, sin cambiar de proceso."),
        60 | 231 => t("Termine le programme et rend un code au shell.", "Ends the program and hands a status back to the shell.", "Termina el programa y devuelve un código al shell."),
        61 => t("Attend la fin d'un processus enfant et récupère son code de sortie.", "Waits for a child process to end and collects its exit status.", "Espera a que termine un proceso hijo y recoge su código de salida."),
        62 => t("Envoie un signal à un processus — souvent pour l'arrêter.", "Sends a signal to a process — often to stop it.", "Envía una señal a un proceso — a menudo para detenerlo."),
        63 => t("Demande le nom et la version du système.", "Asks for the system's name and version.", "Pide el nombre y la versión del sistema."),
        72 => t("Modifie les propriétés d'un descripteur déjà ouvert.", "Changes the properties of an already open descriptor.", "Modifica las propiedades de un descriptor ya abierto."),
        76 | 77 => t("Fixe la taille d'un fichier : le coupe ou le complète de zéros.", "Sets a file's size: truncates it or pads it with zeros.", "Fija el tamaño de un archivo: lo corta o lo completa con ceros."),
        78 => t("Liste le contenu d'un répertoire.", "Lists the contents of a directory.", "Lista el contenido de un directorio."),
        79 | 80 => t("Lit ou change le répertoire de travail du programme.", "Reads or changes the program's working directory.", "Lee o cambia el directorio de trabajo del programa."),
        82 => t("Renomme ou déplace un fichier.", "Renames or moves a file.", "Renombra o mueve un archivo."),
        83 | 84 => t("Crée ou supprime un répertoire.", "Creates or removes a directory.", "Crea o elimina un directorio."),
        86 | 88 | 89 => t("Manipule un lien vers un fichier (physique ou symbolique).", "Handles a link to a file (hard or symbolic).", "Manipula un enlace a un archivo (físico o simbólico)."),
        87 => t("Efface un nom de fichier du répertoire qui le contient.", "Removes a file name from the directory holding it.", "Borra un nombre de archivo del directorio que lo contiene."),
        90 | 92 => t("Change les droits ou le propriétaire d'un fichier.", "Changes a file's permissions or owner.", "Cambia los permisos o el propietario de un archivo."),
        96 | 100 | 201 | 228 => t("Demande l'heure ou le temps écoulé au noyau.", "Asks the kernel for the time or elapsed time.", "Pide al núcleo la hora o el tiempo transcurrido."),
        102 | 104 | 107 => t("Demande l'identité de l'utilisateur qui exécute le programme.", "Asks for the identity of the user running the program.", "Pide la identidad del usuario que ejecuta el programa."),
        158 => t("Règle un détail propre à x86-64, comme la base du segment FS.", "Sets an x86-64-specific detail, such as the FS segment base.", "Ajusta un detalle propio de x86-64, como la base del segmento FS."),
        202 => t("Met un thread en attente ou en réveille un autre (verrous).", "Puts a thread to sleep or wakes another one (locks).", "Duerme un hilo o despierta a otro (cerrojos)."),
        318 => t("Demande au noyau des octets tirés au hasard.", "Asks the kernel for random bytes.", "Pide al núcleo bytes aleatorios."),
        _ => return None,
    })
}

/// Ce que fait l'appel sur le point de s'exécuter, d'après ses arguments.
///
/// `regs` doit être l'état AVANT le `syscall` : c'est là que RAX porte encore
/// le numéro et non le résultat.
pub fn describe(regs: &Registers, lang: Lang) -> Description {
    let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
    let num = regs.rax;
    let name = name(num);
    let mut args = vec![ArgLine {
        reg: "RAX",
        param: "nr",
        role: t("numéro de l'appel demandé au noyau", "number of the call requested from the kernel", "número de la llamada solicitada al núcleo").to_string(),
        value: format!("{num} = {name}"),
    }];
    let mut note = None;
    let mut buffer = None;

    let (summary, ret) = match num {
        // --- write(fd, buf, count) ---
        1 => {
            let (fd, buf, count) = (regs.rdi, regs.rsi, regs.rdx);
            args.push(ArgLine { reg: "RDI", param: "fd", role: t("où écrire", "where to write", "dónde escribir").to_string(), value: format!("{fd} → {}", fd_label(fd, lang)) });
            args.push(ArgLine { reg: "RSI", param: "buf", role: t("adresse du premier octet à envoyer", "address of the first byte to send", "dirección del primer byte a enviar").to_string(), value: format!("0x{buf:X}") });
            args.push(ArgLine { reg: "RDX", param: "count", role: t("combien d'octets envoyer", "how many bytes to send", "cuántos bytes enviar").to_string(), value: format!("{count}") });
            buffer = Some(Buffer { addr: buf, len: count.min(256) as usize, label: t("Texte envoyé", "Text sent", "Texto enviado"), as_text: true });
            if count == 0 {
                note = Some(t("count vaut 0 : l'appel réussit mais n'écrit rien. Le plus souvent, c'est `len` qui n'a pas été calculé.", "count is 0: the call succeeds but writes nothing. Usually `len` was never computed.", "count vale 0: la llamada tiene éxito pero no escribe nada. Casi siempre `len` no se calculó.").to_string());
            }
            let where_ = if fd == 1 || fd == 2 {
                t(" — ce texte apparaît dans la boîte Sortie.", " — this text shows up in the Output box.", " — este texto aparece en el cuadro Salida.")
            } else {
                ""
            };
            (
                format!("{} {} {} 0x{buf:X} {}{where_}", t("Écrit les", "Writes the", "Escribe los"), bytes_word(count, lang), t("qui commencent à l'adresse", "starting at address", "que empiezan en la dirección"), match lang { Lang::Fr => format!("sur {}", fd_label(fd, lang)), Lang::En => format!("to {}", fd_label(fd, lang)), Lang::Es => format!("en {}", fd_label(fd, lang)) }),
                Some(t("RAX recevra le nombre d'octets réellement écrits (souvent égal à count), ou un nombre négatif en cas d'erreur.", "RAX will hold the number of bytes actually written (usually equal to count), or a negative number on error.", "RAX recibirá el número de bytes realmente escritos (normalmente igual a count), o un número negativo si hay error.").to_string()),
            )
        }
        // --- read(fd, buf, count) ---
        0 => {
            let (fd, buf, count) = (regs.rdi, regs.rsi, regs.rdx);
            args.push(ArgLine { reg: "RDI", param: "fd", role: t("d'où lire", "where to read from", "de dónde leer").to_string(), value: format!("{fd} → {}", fd_label(fd, lang)) });
            args.push(ArgLine { reg: "RSI", param: "buf", role: t("adresse où ranger ce qui sera lu", "address where the data will be stored", "dirección donde se guardará lo leído").to_string(), value: format!("0x{buf:X}") });
            args.push(ArgLine { reg: "RDX", param: "count", role: t("taille du tampon : maximum d'octets à lire", "buffer size: maximum bytes to read", "tamaño del búfer: máximo de bytes a leer").to_string(), value: format!("{count}") });
            buffer = Some(Buffer { addr: buf, len: count.min(256) as usize, label: t("Tampon de réception", "Receiving buffer", "Búfer de recepción"), as_text: true });
            (
                format!("{} {} {} 0x{buf:X}. {}", t("Lit au plus", "Reads at most", "Lee como máximo"), bytes_word(count, lang), match lang { Lang::Fr => format!("depuis {} et les range à partir de", fd_label(fd, lang)), Lang::En => format!("from {} and stores them starting at", fd_label(fd, lang)), Lang::Es => format!("desde {} y los guarda a partir de", fd_label(fd, lang)) }, t("Le programme reste bloqué ici tant que rien n'arrive.", "The program stays blocked here until something arrives.", "El programa queda bloqueado aquí hasta que llegue algo.")),
                Some(t("RAX recevra le nombre d'octets réellement lus — souvent moins que count, et 0 signifie « fin d'entrée ». C'est cette valeur, pas count, qui donne la vraie longueur.", "RAX will hold the number of bytes actually read — often fewer than count, and 0 means end of input. That value, not count, is the real length.", "RAX recibirá el número de bytes realmente leídos — a menudo menos que count, y 0 significa fin de entrada. Ese valor, no count, da la longitud real.").to_string()),
            )
        }
        // --- exit / exit_group(code) ---
        60 | 231 => {
            let code = regs.rdi;
            let visible = code & 0xFF;
            args.push(ArgLine { reg: "RDI", param: "code", role: t("code de retour rendu au shell", "exit status handed back to the shell", "código de retorno devuelto al shell").to_string(), value: format!("{code}") });
            if code > 255 {
                note = Some(format!("{} {visible}.", t("Le shell ne reçoit que les 8 bits de poids faible : ce code sera lu comme", "The shell only receives the low 8 bits: this status will be read as", "El shell solo recibe los 8 bits bajos: este código se leerá como")));
            }
            let meaning = if visible == 0 {
                t("0 signifie « tout s'est bien passé » par convention.", "0 means \"everything went fine\" by convention.", "0 significa «todo salió bien» por convención.")
            } else {
                t("un code non nul signale une erreur, par convention.", "a non-zero status signals an error, by convention.", "un código distinto de cero señala un error, por convención.")
            };
            let all = if num == 231 {
                t(" (exit_group termine aussi tous les autres threads)", " (exit_group also ends every other thread)", " (exit_group también termina los demás hilos)")
            } else {
                ""
            };
            (
                format!("{} {code}{all} : {meaning} {}", t("Termine le programme sur-le-champ avec le code", "Ends the program immediately with status", "Termina el programa de inmediato con el código"), t("Rien de ce qui suit ne s'exécutera.", "Nothing after this runs.", "Nada de lo que sigue se ejecutará.")),
                None,
            )
        }
        // --- open(path, flags, mode) ---
        2 => {
            let (path, flags, mode) = (regs.rdi, regs.rsi, regs.rdx);
            args.push(ArgLine { reg: "RDI", param: "path", role: t("adresse du nom de fichier (chaîne terminée par 0)", "address of the file name (NUL-terminated string)", "dirección del nombre de archivo (cadena terminada en 0)").to_string(), value: format!("0x{path:X}") });
            args.push(ArgLine { reg: "RSI", param: "flags", role: t("mode d'ouverture", "opening mode", "modo de apertura").to_string(), value: format!("0x{flags:X} = {}", open_flags(flags)) });
            if flags & 0o100 != 0 {
                args.push(ArgLine { reg: "RDX", param: "mode", role: t("permissions du fichier créé", "permissions of the created file", "permisos del archivo creado").to_string(), value: format!("0o{mode:o}") });
            }
            buffer = Some(Buffer { addr: path, len: 64, label: t("Chemin visé", "Target path", "Ruta indicada"), as_text: true });
            (
                format!("{} 0x{path:X} {} {}.", t("Ouvre le fichier dont le nom est écrit à l'adresse", "Opens the file whose name is stored at address", "Abre el archivo cuyo nombre está en la dirección"), t("en mode", "in mode", "en modo"), open_flags(flags)),
                Some(t("RAX recevra le descripteur du fichier ouvert (un petit entier ≥ 3), ou un nombre négatif si l'ouverture échoue.", "RAX will hold the descriptor of the opened file (a small integer ≥ 3), or a negative number if opening fails.", "RAX recibirá el descriptor del archivo abierto (un entero pequeño ≥ 3), o un número negativo si falla.").to_string()),
            )
        }
        // --- close(fd) ---
        3 => {
            let fd = regs.rdi;
            args.push(ArgLine { reg: "RDI", param: "fd", role: t("descripteur à refermer", "descriptor to close", "descriptor a cerrar").to_string(), value: format!("{fd} → {}", fd_label(fd, lang)) });
            (
                format!("{} {}.", t("Referme", "Closes", "Cierra"), fd_label(fd, lang)),
                Some(t("RAX recevra 0 si tout s'est bien passé, un nombre négatif sinon.", "RAX will hold 0 on success, a negative number otherwise.", "RAX recibirá 0 si todo fue bien, un número negativo si no.").to_string()),
            )
        }
        // --- lseek(fd, offset, whence) ---
        8 => {
            let (fd, off, whence) = (regs.rdi, regs.rsi as i64, regs.rdx);
            let w = match whence { 1 => "SEEK_CUR", 2 => "SEEK_END", _ => "SEEK_SET" };
            args.push(ArgLine { reg: "RDI", param: "fd", role: t("fichier concerné", "file concerned", "archivo afectado").to_string(), value: format!("{fd}") });
            args.push(ArgLine { reg: "RSI", param: "offset", role: t("déplacement, en octets", "displacement, in bytes", "desplazamiento, en bytes").to_string(), value: format!("{off}") });
            args.push(ArgLine { reg: "RDX", param: "whence", role: t("point de départ du déplacement", "origin of the displacement", "origen del desplazamiento").to_string(), value: format!("{whence} = {w}") });
            (
                format!("{} {off} {} {w}.", t("Déplace la position de lecture/écriture du fichier de", "Moves the file's read/write position by", "Mueve la posición de lectura/escritura del archivo en"), t("octets, à partir de", "bytes, relative to", "bytes, a partir de")),
                Some(t("RAX recevra la nouvelle position absolue dans le fichier.", "RAX will hold the new absolute position in the file.", "RAX recibirá la nueva posición absoluta en el archivo.").to_string()),
            )
        }
        // --- brk(addr) ---
        12 => {
            let addr = regs.rdi;
            args.push(ArgLine { reg: "RDI", param: "addr", role: t("nouvelle fin du tas demandée (0 = simple question)", "requested new end of the heap (0 = just asking)", "nuevo final del montón solicitado (0 = solo preguntar)").to_string(), value: format!("0x{addr:X}") });
            (
                if addr == 0 {
                    t("Demande au noyau où se termine actuellement le tas, sans rien changer. C'est le premier appel de toute allocation.", "Asks the kernel where the heap currently ends, changing nothing. This is the first call of any allocation.", "Pregunta al núcleo dónde termina actualmente el montón, sin cambiar nada. Es la primera llamada de toda asignación.").to_string()
                } else {
                    format!("{} 0x{addr:X} : {}", t("Demande à déplacer la fin du tas jusqu'à", "Asks to move the end of the heap up to", "Pide mover el final del montón hasta"), t("c'est ainsi qu'un programme obtient de la mémoire du noyau.", "this is how a program obtains memory from the kernel.", "así es como un programa obtiene memoria del núcleo."))
                },
                Some(t("RAX recevra la fin du tas après l'opération — égale à la valeur demandée si le noyau a accepté.", "RAX will hold the end of the heap after the operation — equal to the requested value if the kernel agreed.", "RAX recibirá el final del montón tras la operación — igual al valor pedido si el núcleo aceptó.").to_string()),
            )
        }
        // --- mmap / mprotect / munmap ---
        9 => {
            let (addr, len, prot) = (regs.rdi, regs.rsi, regs.rdx);
            args.push(ArgLine { reg: "RDI", param: "addr", role: t("adresse souhaitée (0 = au choix du noyau)", "preferred address (0 = kernel's choice)", "dirección deseada (0 = a elección del núcleo)").to_string(), value: format!("0x{addr:X}") });
            args.push(ArgLine { reg: "RSI", param: "length", role: t("taille de la zone demandée", "size of the requested area", "tamaño de la zona solicitada").to_string(), value: bytes_word(len, lang) });
            args.push(ArgLine { reg: "RDX", param: "prot", role: t("droits sur la zone", "permissions on the area", "permisos sobre la zona").to_string(), value: format!("{prot} = {}", prot_flags(prot)) });
            args.push(ArgLine { reg: "R10", param: "flags", role: t("nature de la zone (privée, anonyme…)", "kind of area (private, anonymous…)", "naturaleza de la zona (privada, anónima…)").to_string(), value: format!("0x{:X}", regs.r10) });
            (
                format!("{} {} ({}).", t("Réclame au noyau une zone mémoire de", "Asks the kernel for a memory area of", "Solicita al núcleo una zona de memoria de"), bytes_word(len, lang), prot_flags(prot)),
                Some(t("RAX recevra l'adresse de la zone obtenue, ou une valeur négative (proche de -1) en cas d'échec.", "RAX will hold the address of the area obtained, or a negative value (close to -1) on failure.", "RAX recibirá la dirección de la zona obtenida, o un valor negativo (cercano a -1) si falla.").to_string()),
            )
        }
        10 => {
            let (addr, len, prot) = (regs.rdi, regs.rsi, regs.rdx);
            args.push(ArgLine { reg: "RDI", param: "addr", role: t("début de la zone", "start of the area", "inicio de la zona").to_string(), value: format!("0x{addr:X}") });
            args.push(ArgLine { reg: "RSI", param: "length", role: t("longueur concernée", "length concerned", "longitud afectada").to_string(), value: bytes_word(len, lang) });
            args.push(ArgLine { reg: "RDX", param: "prot", role: t("nouveaux droits", "new permissions", "nuevos permisos").to_string(), value: format!("{prot} = {}", prot_flags(prot)) });
            (
                format!("{} 0x{addr:X} {} {}.", t("Change les droits de la zone qui commence en", "Changes the permissions of the area starting at", "Cambia los permisos de la zona que empieza en"), t("pour", "to", "a"), prot_flags(prot)),
                Some(t("RAX recevra 0 en cas de succès, un nombre négatif sinon.", "RAX will hold 0 on success, a negative number otherwise.", "RAX recibirá 0 en caso de éxito, un número negativo si no.").to_string()),
            )
        }
        11 => {
            let (addr, len) = (regs.rdi, regs.rsi);
            args.push(ArgLine { reg: "RDI", param: "addr", role: t("début de la zone à rendre", "start of the area to release", "inicio de la zona a liberar").to_string(), value: format!("0x{addr:X}") });
            args.push(ArgLine { reg: "RSI", param: "length", role: t("longueur à rendre", "length to release", "longitud a liberar").to_string(), value: bytes_word(len, lang) });
            (
                format!("{} {} {} 0x{addr:X}.", t("Rend au noyau les", "Gives back to the kernel the", "Devuelve al núcleo los"), bytes_word(len, lang), t("qui commencent en", "starting at", "que empiezan en")),
                Some(t("RAX recevra 0 en cas de succès. Toute lecture ultérieure dans cette zone provoquera une erreur de segmentation.", "RAX will hold 0 on success. Any later read in that area will cause a segmentation fault.", "RAX recibirá 0 en caso de éxito. Cualquier lectura posterior en esa zona provocará un fallo de segmentación.").to_string()),
            )
        }
        // --- nanosleep(req, rem) ---
        35 => {
            args.push(ArgLine { reg: "RDI", param: "req", role: t("adresse d'une structure { secondes, nanosecondes }", "address of a { seconds, nanoseconds } structure", "dirección de una estructura { segundos, nanosegundos }").to_string(), value: format!("0x{:X}", regs.rdi) });
            (
                t("Suspend le programme pendant la durée écrite à l'adresse donnée. Rien ne s'exécute pendant ce temps.", "Suspends the program for the duration stored at the given address. Nothing runs meanwhile.", "Suspende el programa durante el tiempo escrito en la dirección dada. Nada se ejecuta mientras tanto.").to_string(),
                Some(t("RAX recevra 0 si la pause est allée à son terme.", "RAX will hold 0 if the pause ran to completion.", "RAX recibirá 0 si la pausa llegó a su fin.").to_string()),
            )
        }
        // --- getpid() ---
        39 => (
            t("Demande au noyau le numéro (PID) du processus en cours. Aucun argument n'est nécessaire.", "Asks the kernel for the number (PID) of the running process. No argument needed.", "Pide al núcleo el número (PID) del proceso en curso. No necesita argumentos.").to_string(),
            Some(t("RAX recevra le PID.", "RAX will hold the PID.", "RAX recibirá el PID.").to_string()),
        ),
        // --- getrandom(buf, len, flags) ---
        318 => {
            let (buf, len) = (regs.rdi, regs.rsi);
            args.push(ArgLine { reg: "RDI", param: "buf", role: t("adresse où déposer les octets tirés au hasard", "address where the random bytes go", "dirección donde depositar los bytes aleatorios").to_string(), value: format!("0x{buf:X}") });
            args.push(ArgLine { reg: "RSI", param: "buflen", role: t("combien d'octets tirer", "how many bytes to draw", "cuántos bytes generar").to_string(), value: format!("{len}") });
            buffer = Some(Buffer { addr: buf, len: len.min(64) as usize, label: t("Tampon aléatoire", "Random buffer", "Búfer aleatorio"), as_text: false });
            (
                format!("{} {} {} 0x{buf:X}.", t("Remplit de", "Fills with", "Rellena con"), bytes_word(len, lang), t("tirés au hasard la zone qui commence en", "random bytes the area starting at", "bytes aleatorios la zona que empieza en")),
                Some(t("RAX recevra le nombre d'octets effectivement fournis.", "RAX will hold the number of bytes actually provided.", "RAX recibirá el número de bytes realmente entregados.").to_string()),
            )
        }
        // --- openat(dirfd, path, flags, mode) ---
        257 => {
            let (dirfd, path, flags) = (regs.rdi as i64, regs.rsi, regs.rdx);
            let dir = if dirfd == -100 {
                t("AT_FDCWD : chemin lu depuis le répertoire de travail", "AT_FDCWD: path read from the working directory", "AT_FDCWD: ruta leída desde el directorio de trabajo").to_string()
            } else {
                format!("{} {dirfd}", t("relatif au répertoire ouvert n°", "relative to open directory #", "relativo al directorio abierto n.º"))
            };
            args.push(ArgLine { reg: "RDI", param: "dirfd", role: t("point de départ du chemin", "starting point of the path", "punto de partida de la ruta").to_string(), value: dir });
            args.push(ArgLine { reg: "RSI", param: "path", role: t("adresse du nom de fichier (terminé par 0)", "address of the file name (NUL-terminated)", "dirección del nombre de archivo (terminado en 0)").to_string(), value: format!("0x{path:X}") });
            args.push(ArgLine { reg: "RDX", param: "flags", role: t("mode d'ouverture", "opening mode", "modo de apertura").to_string(), value: format!("0x{flags:X} = {}", open_flags(flags)) });
            buffer = Some(Buffer { addr: path, len: 64, label: t("Chemin visé", "Target path", "Ruta indicada"), as_text: true });
            (
                format!("{} 0x{path:X} {} {}. {}", t("Ouvre le fichier nommé à l'adresse", "Opens the file named at address", "Abre el archivo nombrado en la dirección"), t("en mode", "in mode", "en modo"), open_flags(flags), t("C'est la version moderne d'open ; elle seule est utilisée par la bibliothèque C.", "This is the modern form of open; it is the only one the C library uses.", "Es la versión moderna de open; es la única que usa la biblioteca C.")),
                Some(t("RAX recevra le descripteur du fichier ouvert (≥ 3), ou un nombre négatif si l'ouverture échoue.", "RAX will hold the descriptor of the opened file (≥ 3), or a negative number if opening fails.", "RAX recibirá el descriptor del archivo abierto (≥ 3), o un número negativo si falla.").to_string()),
            )
        }
        // --- dup / dup2 ---
        32 | 33 => {
            let old = regs.rdi;
            args.push(ArgLine { reg: "RDI", param: "oldfd", role: t("descripteur à dupliquer", "descriptor to duplicate", "descriptor a duplicar").to_string(), value: format!("{old} → {}", fd_label(old, lang)) });
            if num == 33 {
                let new = regs.rsi;
                args.push(ArgLine { reg: "RSI", param: "newfd", role: t("numéro que doit prendre la copie", "number the copy must take", "número que debe tomar la copia").to_string(), value: format!("{new} → {}", fd_label(new, lang)) });
                (
                    format!("{} {} {}. {}", t("Fait pointer", "Makes", "Hace que"), fd_label(regs.rsi, lang), match lang { Lang::Fr => format!("vers la même destination que {}", fd_label(old, lang)), Lang::En => format!("point to the same destination as {}", fd_label(old, lang)), Lang::Es => format!("apunte al mismo destino que {}", fd_label(old, lang)) }, t("C'est ainsi qu'un shell redirige une sortie vers un fichier.", "This is how a shell redirects an output into a file.", "Así es como un shell redirige una salida a un archivo.")),
                    Some(t("RAX recevra le nouveau numéro de descripteur, ou un nombre négatif en cas d'erreur.", "RAX will hold the new descriptor number, or a negative number on error.", "RAX recibirá el nuevo número de descriptor, o un número negativo si hay error.").to_string()),
                )
            } else {
                (
                    format!("{} {}.", t("Crée une seconde entrée vers la même destination que", "Creates a second entry pointing to the same destination as", "Crea una segunda entrada hacia el mismo destino que"), fd_label(old, lang)),
                    Some(t("RAX recevra le plus petit numéro de descripteur libre.", "RAX will hold the lowest free descriptor number.", "RAX recibirá el número de descriptor libre más bajo.").to_string()),
                )
            }
        }
        // --- access(path, mode) ---
        21 => {
            let (path, mode) = (regs.rdi, regs.rsi);
            let wanted = if mode == 0 {
                "F_OK".to_string()
            } else {
                [(1, "X_OK"), (2, "W_OK"), (4, "R_OK")].iter().filter(|(b, _)| mode & b != 0).map(|(_, n)| *n).collect::<Vec<_>>().join("|")
            };
            args.push(ArgLine { reg: "RDI", param: "path", role: t("adresse du nom de fichier", "address of the file name", "dirección del nombre de archivo").to_string(), value: format!("0x{path:X}") });
            args.push(ArgLine { reg: "RSI", param: "mode", role: t("droits que l'on veut vérifier", "rights to be checked", "permisos que se quieren comprobar").to_string(), value: format!("{mode} = {wanted}") });
            buffer = Some(Buffer { addr: path, len: 64, label: t("Chemin testé", "Tested path", "Ruta comprobada"), as_text: true });
            (
                format!("{} ({wanted}) {}", t("Vérifie les droits", "Checks the rights", "Comprueba los permisos"), t("sur le fichier nommé, sans l'ouvrir.", "on the named file, without opening it.", "sobre el archivo nombrado, sin abrirlo.")),
                Some(t("RAX recevra 0 si l'accès est permis, un nombre négatif sinon.", "RAX will hold 0 if access is granted, a negative number otherwise.", "RAX recibirá 0 si se permite el acceso, un número negativo si no.").to_string()),
            )
        }
        // --- stat / fstat / lstat ---
        4..=6 => {
            let out = regs.rsi;
            if num == 5 {
                args.push(ArgLine { reg: "RDI", param: "fd", role: t("fichier déjà ouvert à examiner", "already open file to inspect", "archivo ya abierto a examinar").to_string(), value: format!("{}", regs.rdi) });
            } else {
                args.push(ArgLine { reg: "RDI", param: "path", role: t("adresse du nom de fichier", "address of the file name", "dirección del nombre de archivo").to_string(), value: format!("0x{:X}", regs.rdi) });
                buffer = Some(Buffer { addr: regs.rdi, len: 64, label: t("Chemin examiné", "Inspected path", "Ruta examinada"), as_text: true });
            }
            args.push(ArgLine { reg: "RSI", param: "statbuf", role: t("adresse où le noyau écrira la fiche du fichier (144 octets)", "address where the kernel writes the file's record (144 bytes)", "dirección donde el núcleo escribirá la ficha del archivo (144 bytes)").to_string(), value: format!("0x{out:X}") });
            (
                format!("{} 0x{out:X} : {}", t("Demande la fiche du fichier (taille, droits, dates) et la fait écrire à l'adresse", "Asks for the file's record (size, permissions, dates) and has it written at address", "Pide la ficha del archivo (tamaño, permisos, fechas) y la hace escribir en la dirección"), t("le noyau remplit la zone, le programme n'a plus qu'à y lire les champs.", "the kernel fills the area; the program then reads the fields from it.", "el núcleo rellena la zona; el programa solo tiene que leer los campos.")),
                Some(t("RAX recevra 0 si le fichier existe et a pu être examiné.", "RAX will hold 0 if the file exists and could be inspected.", "RAX recibirá 0 si el archivo existe y pudo examinarse.").to_string()),
            )
        }
        // --- ioctl(fd, request, arg) ---
        16 => {
            let (fd, req) = (regs.rdi, regs.rsi);
            args.push(ArgLine { reg: "RDI", param: "fd", role: t("périphérique visé", "target device", "periférico afectado").to_string(), value: format!("{fd} → {}", fd_label(fd, lang)) });
            args.push(ArgLine { reg: "RSI", param: "request", role: t("code de la commande, propre au périphérique", "command code, specific to the device", "código de la orden, propio del periférico").to_string(), value: format!("0x{req:X}") });
            args.push(ArgLine { reg: "RDX", param: "arg", role: t("adresse des données de la commande", "address of the command's data", "dirección de los datos de la orden").to_string(), value: format!("0x{:X}", regs.rdx) });
            (
                format!("{} 0x{req:X} {} {}.", t("Envoie la commande", "Sends command", "Envía la orden"), t("à", "to", "a"), fd_label(fd, lang)),
                Some(t("RAX recevra 0 ou une valeur propre à la commande ; négatif signale une erreur.", "RAX will hold 0 or a command-specific value; negative signals an error.", "RAX recibirá 0 o un valor propio de la orden; negativo indica error.").to_string()),
            )
        }
        // --- pipe / pipe2 ---
        22 | 293 => {
            let fds = regs.rdi;
            args.push(ArgLine { reg: "RDI", param: "pipefd", role: t("adresse d'un tableau de 2 entiers, que le noyau remplira", "address of a 2-integer array, which the kernel fills in", "dirección de un arreglo de 2 enteros que el núcleo rellenará").to_string(), value: format!("0x{fds:X}") });
            buffer = Some(Buffer { addr: fds, len: 8, label: t("Paire de descripteurs (lecture, écriture)", "Descriptor pair (read, write)", "Par de descriptores (lectura, escritura)"), as_text: false });
            (
                format!("{} 0x{fds:X} : {}", t("Crée un tube et écrit ses deux descripteurs à l'adresse", "Creates a pipe and writes its two descriptors at address", "Crea una tubería y escribe sus dos descriptores en la dirección"), t("le premier sert à lire, le second à écrire. Ce qu'on écrit dans l'un ressort de l'autre.", "the first one reads, the second one writes. What goes into one comes out of the other.", "el primero sirve para leer, el segundo para escribir. Lo que se escribe en uno sale por el otro.")),
                Some(t("RAX recevra 0 si le tube a été créé.", "RAX will hold 0 if the pipe was created.", "RAX recibirá 0 si se creó la tubería.").to_string()),
            )
        }
        // --- fork / vfork / clone ---
        56..=58 => (
            t("Crée un second processus, copie exacte de celui-ci. Les deux repartent de la même instruction : seule la valeur rendue dans RAX les distingue.", "Creates a second process, an exact copy of this one. Both resume at the same instruction: only the value returned in RAX tells them apart.", "Crea un segundo proceso, copia exacta de este. Ambos continúan en la misma instrucción: solo el valor devuelto en RAX los distingue.").to_string(),
            Some(t("RAX recevra 0 dans l'enfant, et le PID de l'enfant dans le parent. C'est le seul appel qui revient deux fois.", "RAX will hold 0 in the child, and the child's PID in the parent. It is the only call that returns twice.", "RAX recibirá 0 en el hijo, y el PID del hijo en el padre. Es la única llamada que retorna dos veces.").to_string()),
        ),
        // --- execve(path, argv, envp) ---
        59 => {
            let path = regs.rdi;
            args.push(ArgLine { reg: "RDI", param: "path", role: t("adresse du chemin du programme à lancer", "address of the path of the program to run", "dirección de la ruta del programa a lanzar").to_string(), value: format!("0x{path:X}") });
            args.push(ArgLine { reg: "RSI", param: "argv", role: t("adresse du tableau d'arguments, terminé par un pointeur nul", "address of the argument array, NULL-terminated", "dirección del arreglo de argumentos, terminado en puntero nulo").to_string(), value: format!("0x{:X}", regs.rsi) });
            args.push(ArgLine { reg: "RDX", param: "envp", role: t("adresse du tableau d'environnement (peut être nul)", "address of the environment array (may be NULL)", "dirección del arreglo de entorno (puede ser nulo)").to_string(), value: format!("0x{:X}", regs.rdx) });
            buffer = Some(Buffer { addr: path, len: 64, label: t("Programme lancé", "Program launched", "Programa lanzado"), as_text: true });
            (
                t("Remplace le programme en cours par celui dont le chemin est donné : même processus, même PID, mais tout le code et toute la mémoire sont balayés. En cas de succès, cet appel ne revient jamais.", "Replaces the running program with the one at the given path: same process, same PID, but all code and memory are swept away. On success, this call never returns.", "Reemplaza el programa en curso por el de la ruta indicada: mismo proceso, mismo PID, pero todo el código y la memoria se barren. Si tiene éxito, esta llamada nunca retorna.").to_string(),
                Some(t("RAX ne recevra une valeur (négative) que si le lancement a échoué.", "RAX only receives a (negative) value if the launch failed.", "RAX solo recibirá un valor (negativo) si falló el lanzamiento.").to_string()),
            )
        }
        // --- wait4(pid, status, options, rusage) ---
        61 => {
            let (pid, status) = (regs.rdi as i64, regs.rsi);
            args.push(ArgLine { reg: "RDI", param: "pid", role: t("enfant attendu (-1 = n'importe lequel)", "child awaited (-1 = any of them)", "hijo esperado (-1 = cualquiera)").to_string(), value: format!("{pid}") });
            args.push(ArgLine { reg: "RSI", param: "status", role: t("adresse où le noyau écrira le code de sortie de l'enfant", "address where the kernel writes the child's exit status", "dirección donde el núcleo escribirá el código de salida del hijo").to_string(), value: format!("0x{status:X}") });
            (
                t("Attend qu'un processus enfant se termine, et récupère son code de sortie. Le programme reste bloqué ici tant que l'enfant vit.", "Waits for a child process to end and collects its exit status. The program stays blocked here as long as the child lives.", "Espera a que termine un proceso hijo y recoge su código de salida. El programa queda bloqueado aquí mientras el hijo viva.").to_string(),
                Some(t("RAX recevra le PID de l'enfant qui s'est terminé.", "RAX will hold the PID of the child that ended.", "RAX recibirá el PID del hijo que terminó.").to_string()),
            )
        }
        // --- kill(pid, sig) ---
        62 => {
            let (pid, sig) = (regs.rdi as i64, regs.rsi);
            let signame = match sig { 2 => "SIGINT", 9 => "SIGKILL", 11 => "SIGSEGV", 15 => "SIGTERM", 17 => "SIGCHLD", 19 => "SIGSTOP", _ => "" };
            args.push(ArgLine { reg: "RDI", param: "pid", role: t("processus destinataire (0 = tout le groupe)", "target process (0 = the whole group)", "proceso destinatario (0 = todo el grupo)").to_string(), value: format!("{pid}") });
            args.push(ArgLine { reg: "RSI", param: "sig", role: t("numéro du signal envoyé", "number of the signal sent", "número de la señal enviada").to_string(), value: if signame.is_empty() { format!("{sig}") } else { format!("{sig} = {signame}") } });
            (
                format!("{} {sig}{} {} {pid}.", t("Envoie le signal", "Sends signal", "Envía la señal"), if signame.is_empty() { String::new() } else { format!(" ({signame})") }, t("au processus", "to process", "al proceso")),
                Some(t("RAX recevra 0 si le signal a pu être envoyé.", "RAX will hold 0 if the signal could be sent.", "RAX recibirá 0 si se pudo enviar la señal.").to_string()),
            )
        }
        // --- unlink / rmdir / mkdir / chdir : un chemin, un effet ---
        80 | 83 | 84 | 87 => {
            let path = regs.rdi;
            args.push(ArgLine { reg: "RDI", param: "path", role: t("adresse du chemin visé", "address of the target path", "dirección de la ruta indicada").to_string(), value: format!("0x{path:X}") });
            if num == 83 {
                args.push(ArgLine { reg: "RSI", param: "mode", role: t("permissions du répertoire créé", "permissions of the created directory", "permisos del directorio creado").to_string(), value: format!("0o{:o}", regs.rsi) });
            }
            buffer = Some(Buffer { addr: path, len: 64, label: t("Chemin visé", "Target path", "Ruta indicada"), as_text: true });
            (
                match num {
                    80 => t("Change le répertoire de travail du programme pour celui dont le chemin est donné.", "Changes the program's working directory to the given path.", "Cambia el directorio de trabajo del programa al de la ruta dada."),
                    83 => t("Crée un répertoire au chemin donné.", "Creates a directory at the given path.", "Crea un directorio en la ruta dada."),
                    84 => t("Supprime le répertoire donné (il doit être vide).", "Removes the given directory (it must be empty).", "Elimina el directorio dado (debe estar vacío)."),
                    _ => t("Efface ce nom de fichier. Le contenu ne disparaît vraiment que lorsque plus aucun nom ni descripteur ne le retient.", "Removes this file name. The content only really disappears once no name and no descriptor hold it any more.", "Borra este nombre de archivo. El contenido solo desaparece de verdad cuando ningún nombre ni descriptor lo retiene."),
                }
                .to_string(),
                Some(t("RAX recevra 0 en cas de succès, un nombre négatif sinon.", "RAX will hold 0 on success, a negative number otherwise.", "RAX recibirá 0 en caso de éxito, un número negativo si no.").to_string()),
            )
        }
        // --- ftruncate(fd, length) ---
        77 => {
            let (fd, len) = (regs.rdi, regs.rsi);
            args.push(ArgLine { reg: "RDI", param: "fd", role: t("fichier ouvert à redimensionner", "open file to resize", "archivo abierto a redimensionar").to_string(), value: format!("{fd}") });
            args.push(ArgLine { reg: "RSI", param: "length", role: t("taille voulue", "wanted size", "tamaño deseado").to_string(), value: bytes_word(len, lang) });
            (
                format!("{} {}. {}", t("Fixe la taille du fichier à", "Sets the file's size to", "Fija el tamaño del archivo en"), bytes_word(len, lang), t("Ce qui dépasse est coupé ; ce qui manque est comblé de zéros.", "Anything beyond is cut off; anything missing is filled with zeros.", "Lo que sobra se corta; lo que falta se rellena con ceros.")),
                Some(t("RAX recevra 0 en cas de succès.", "RAX will hold 0 on success.", "RAX recibirá 0 en caso de éxito.").to_string()),
            )
        }
        // --- temps ---
        201 | 228 | 96 => {
            let out = if num == 228 { regs.rsi } else { regs.rdi };
            if num == 228 {
                args.push(ArgLine { reg: "RDI", param: "clockid", role: t("horloge consultée (0 = temps réel)", "clock queried (0 = real time)", "reloj consultado (0 = tiempo real)").to_string(), value: format!("{}", regs.rdi) });
            }
            args.push(ArgLine { reg: if num == 228 { "RSI" } else { "RDI" }, param: "tp", role: t("adresse où écrire l'heure (0 = la rendre dans RAX)", "address where the time goes (0 = return it in RAX)", "dirección donde escribir la hora (0 = devolverla en RAX)").to_string(), value: format!("0x{out:X}") });
            (
                t("Demande l'heure courante au noyau, comptée en secondes depuis le 1er janvier 1970.", "Asks the kernel for the current time, counted in seconds since 1 January 1970.", "Pide al núcleo la hora actual, contada en segundos desde el 1 de enero de 1970.").to_string(),
                Some(t("RAX recevra l'heure elle-même, ou 0 si elle a été écrite à l'adresse donnée.", "RAX will hold the time itself, or 0 if it was written at the given address.", "RAX recibirá la hora, o 0 si se escribió en la dirección dada.").to_string()),
            )
        }
        // --- identités ---
        102 | 104 | 107 | 110 | 186 => (
            format!("{} {}. {}", t("Demande au noyau", "Asks the kernel for", "Pide al núcleo"), match num {
                102 => t("le numéro de l'utilisateur qui exécute ce programme", "the number of the user running this program", "el número del usuario que ejecuta este programa"),
                104 => t("le numéro du groupe de l'utilisateur", "the user's group number", "el número del grupo del usuario"),
                107 => t("le numéro d'utilisateur effectif (celui qui décide des droits)", "the effective user number (the one deciding permissions)", "el número de usuario efectivo (el que decide los permisos)"),
                110 => t("le numéro du processus parent", "the parent process's number", "el número del proceso padre"),
                _ => t("le numéro du thread courant", "the current thread's number", "el número del hilo actual"),
            }, t("Aucun argument n'est nécessaire.", "No argument needed.", "No necesita argumentos.")),
            Some(t("RAX recevra ce numéro. Cet appel ne peut pas échouer.", "RAX will hold that number. This call cannot fail.", "RAX recibirá ese número. Esta llamada no puede fallar.").to_string()),
        ),
        // --- pause / sched_yield : aucun argument, un effet ---
        34 | 24 => (
            if num == 34 {
                t("Endort le programme jusqu'à l'arrivée d'un signal. Sans signal, il ne se réveillera jamais.", "Puts the program to sleep until a signal arrives. Without a signal, it never wakes up.", "Duerme el programa hasta que llegue una señal. Sin señal, no despertará nunca.").to_string()
            } else {
                t("Rend la main au noyau sans attendre : laisse un autre programme s'exécuter, puis reprend.", "Yields to the kernel without waiting: lets another program run, then resumes.", "Cede el control al núcleo sin esperar: deja que otro programa se ejecute y luego continúa.").to_string()
            },
            Some(t("RAX recevra 0 (ou un code négatif d'interruption pour pause).", "RAX will hold 0 (or a negative interruption code for pause).", "RAX recibirá 0 (o un código negativo de interrupción para pause).").to_string()),
        ),
        // --- connu de nom, mais pas détaillé ici : la phrase d'intention ---
        n if gist(n, lang).is_some() => {
            for (reg, val) in [("RDI", regs.rdi), ("RSI", regs.rsi), ("RDX", regs.rdx), ("R10", regs.r10), ("R8", regs.r8), ("R9", regs.r9)] {
                args.push(ArgLine { reg, param: "", role: t("argument, rôle propre à cet appel", "argument, meaning specific to this call", "argumento, papel propio de esta llamada").to_string(), value: format!("{val} (0x{val:X})") });
            }
            (
                format!("{} {}", gist(n, lang).unwrap_or_default(), t("Ses arguments sont, dans l'ordre, RDI, RSI, RDX, R10, R8, R9.", "Its arguments are, in order, RDI, RSI, RDX, R10, R8, R9.", "Sus argumentos son, en orden, RDI, RSI, RDX, R10, R8, R9.")),
                Some(t("RAX recevra le résultat : négatif signifie erreur.", "RAX will hold the result: negative means error.", "RAX recibirá el resultado: negativo significa error.").to_string()),
            )
        }
        // --- inconnu : on montre au moins la convention ---
        _ => {
            for (reg, val) in [("RDI", regs.rdi), ("RSI", regs.rsi), ("RDX", regs.rdx), ("R10", regs.r10), ("R8", regs.r8), ("R9", regs.r9)] {
                args.push(ArgLine { reg, param: "", role: t("argument, rôle propre à cet appel", "argument, meaning specific to this call", "argumento, papel propio de esta llamada").to_string(), value: format!("{val} (0x{val:X})") });
            }
            (
                format!("{} {num}. {}", t("Appel système n°", "System call #", "Llamada al sistema n.º"), t("ASM Studio ne connaît pas celui-ci : ses arguments sont, dans l'ordre, RDI, RSI, RDX, R10, R8, R9.", "ASM Studio doesn't know this one: its arguments are, in order, RDI, RSI, RDX, R10, R8, R9.", "ASM Studio no conoce esta: sus argumentos son, en orden, RDI, RSI, RDX, R10, R8, R9.")),
                Some(t("RAX recevra le résultat : négatif signifie erreur.", "RAX will hold the result: negative means error.", "RAX recibirá el resultado: negativo significa error.").to_string()),
            )
        }
    };

    Description { name, summary, args, ret, buffer, note }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regs(rax: u64, rdi: u64, rsi: u64, rdx: u64) -> Registers {
        Registers { rax, rdi, rsi, rdx, ..Default::default() }
    }

    /// L'exemple canonique du cours : `write(1, msg, len)` doit se lire en
    /// français, avec la longueur ET la destination.
    #[test]
    fn write_to_stdout_is_explained_in_words() {
        let d = describe(&regs(1, 1, 0x402000, 13), Lang::Fr);
        assert_eq!(d.name, "write");
        assert!(d.summary.contains("13 octets"), "la longueur doit être dite : {}", d.summary);
        assert!(d.summary.contains("sortie standard"), "la destination doit être dite : {}", d.summary);
        assert!(d.summary.contains("0x402000"), "l'adresse du tampon doit être dite : {}", d.summary);
        // Quatre lignes : RAX, RDI, RSI, RDX.
        assert_eq!(d.args.len(), 4);
        assert_eq!(d.args[1].reg, "RDI");
        assert!(d.args[1].value.contains("écran"), "fd=1 doit être traduit : {}", d.args[1].value);
        let b = d.buffer.expect("write montre le tampon écrit");
        assert_eq!((b.addr, b.len), (0x402000, 13));
        assert!(d.ret.is_some(), "write revient : RAX a un sens après");
    }

    /// `count = 0` est l'erreur la plus fréquente du débutant (`len` oublié) :
    /// elle doit être signalée, pas seulement affichée.
    #[test]
    fn write_of_zero_bytes_warns() {
        let d = describe(&regs(1, 1, 0x402000, 0), Lang::Fr);
        assert!(d.note.is_some(), "un write de 0 octet mérite un avertissement");
    }

    /// `read` bloque et rend une longueur qui n'est pas `count` : les deux
    /// points doivent apparaître.
    #[test]
    fn read_says_it_blocks_and_returns_the_real_length() {
        let d = describe(&regs(0, 0, 0x4020A0, 64), Lang::Fr);
        assert_eq!(d.name, "read");
        assert!(d.summary.contains("bloqué"), "read bloque : {}", d.summary);
        assert!(d.ret.as_deref().unwrap_or("").contains("réellement lus"));
    }

    /// `exit` ne revient pas : pas de valeur de retour à annoncer.
    #[test]
    fn exit_has_no_return_value() {
        let d = describe(&regs(60, 0, 0, 0), Lang::Fr);
        assert!(d.ret.is_none(), "exit ne rend jamais la main");
        assert!(d.summary.contains('0'));
    }

    /// Le noyau ne garde que 8 bits du code de sortie : `exit(256)` est vu
    /// comme un succès par le shell, ce qui déroute sans explication.
    #[test]
    fn exit_status_above_255_is_flagged_as_truncated() {
        let d = describe(&regs(60, 256, 0, 0), Lang::Fr);
        assert!(d.note.expect("troncature signalée").contains("0"));
    }

    /// Un numéro non répertorié ne doit pas produire une page vide : on
    /// rappelle au moins l'ordre des registres d'arguments.
    #[test]
    fn unknown_syscall_still_lists_argument_registers() {
        let d = describe(&regs(9999, 1, 2, 3), Lang::Fr);
        assert_eq!(d.name, "syscall");
        assert_eq!(d.args.len(), 7, "RAX + les six registres d'arguments");
        assert_eq!(d.args[4].reg, "R10", "le 4e argument passe par R10, pas RCX");
    }

    /// Les trois langues répondent, et aucune ne rend une phrase vide.
    #[test]
    fn every_language_answers() {
        for lang in [Lang::Fr, Lang::En, Lang::Es] {
            let d = describe(&regs(1, 1, 0x402000, 13), lang);
            assert!(!d.summary.trim().is_empty(), "résumé vide en {lang:?}");
            assert!(d.args.iter().all(|a| !a.role.trim().is_empty()), "rôle vide en {lang:?}");
        }
    }

    /// Contrat de la bibliothèque : tout appel que l'on sait NOMMER, on sait
    /// aussi dire à quoi il sert — dans les trois langues. Un nom sans phrase
    /// n'apprendrait rien de plus que le numéro.
    #[test]
    fn every_named_syscall_has_a_gist_in_every_language() {
        for n in 0..=400u64 {
            if name(n) == "syscall" {
                continue;
            }
            for lang in [Lang::Fr, Lang::En, Lang::Es] {
                let g = gist(n, lang).unwrap_or_else(|| panic!("{} (#{n}) n'a pas de phrase d'intention", name(n)));
                assert!(!g.trim().is_empty(), "phrase vide pour {} (#{n})", name(n));
            }
        }
    }

    /// Et tout appel nommé produit une description exploitable : un résumé, au
    /// moins la ligne RAX, et pas de champ laissé vide.
    #[test]
    fn every_named_syscall_describes_itself() {
        for n in 0..=400u64 {
            if name(n) == "syscall" {
                continue;
            }
            let d = describe(&regs(n, 1, 0x402000, 8), Lang::Fr);
            assert_eq!(d.name, name(n));
            assert!(d.summary.len() > 20, "résumé trop court pour {} (#{n}) : {}", d.name, d.summary);
            assert!(!d.args.is_empty() && d.args[0].reg == "RAX", "RAX doit ouvrir la liste pour {}", d.name);
            assert!(d.args.iter().all(|a| !a.role.trim().is_empty() && !a.value.trim().is_empty()), "champ vide dans les arguments de {}", d.name);
            assert_eq!(d.ret.is_none(), is_exit(n), "seuls exit/exit_group n'ont pas de retour ({})", d.name);
        }
    }

    /// Les appels détaillés le sont vraiment : ils nomment leurs arguments,
    /// là où le repli générique se contente de « RDI, RSI, RDX… ».
    #[test]
    fn detailed_syscalls_name_their_parameters() {
        for (n, param) in [(1u64, "count"), (0, "count"), (2, "flags"), (257, "flags"), (33, "newfd"), (62, "sig"), (59, "argv"), (22, "pipefd"), (77, "length")] {
            let d = describe(&regs(n, 1, 0x402000, 8), Lang::Fr);
            assert!(d.args.iter().any(|a| a.param == param), "{} devrait nommer son argument `{param}`", d.name);
        }
    }

    /// `fork` est le seul appel qui revient deux fois : la description doit le
    /// dire, sinon le RAX à 0 côté enfant reste incompréhensible.
    #[test]
    fn fork_explains_its_double_return() {
        let d = describe(&regs(57, 0, 0, 0), Lang::Fr);
        assert_eq!(d.name, "fork");
        assert!(d.ret.as_deref().unwrap_or("").contains("deux fois"));
    }

    /// `execve` ne revient pas en cas de succès — un détail qui décide de la
    /// suite du programme.
    #[test]
    fn execve_warns_it_does_not_return() {
        let d = describe(&regs(59, 0x402000, 0, 0), Lang::Fr);
        assert!(d.summary.contains("ne revient jamais"), "{}", d.summary);
        assert!(d.buffer.is_some(), "le chemin du programme lancé doit être montré");
    }

    /// L'aperçu de tampon rend le texte lisible sans casser la mise en page.
    #[test]
    fn preview_escapes_control_characters_and_truncates() {
        assert_eq!(text_preview(b"Bonjour !\n", 32), "Bonjour !\\n");
        assert_eq!(text_preview(&[0xFF, b'A'], 32), "·A");
        assert!(text_preview(b"abcdefghij", 4).ends_with('…'));
    }

    /// Les drapeaux d'ouverture se lisent en symboles, pas en hexa.
    #[test]
    fn open_flags_are_named() {
        assert_eq!(open_flags(0), "O_RDONLY");
        assert_eq!(open_flags(0o1101), "O_WRONLY|O_CREAT|O_TRUNC");
        assert_eq!(prot_flags(3), "PROT_READ|PROT_WRITE");
        assert_eq!(prot_flags(0), "PROT_NONE");
    }
}
