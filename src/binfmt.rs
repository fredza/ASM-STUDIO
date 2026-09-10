//! Lecture d'un exécutable : en-tête, sections, imports, symboles.
//!
//! « Qu'est-ce qu'un exécutable ? » n'est pas une question annexe quand on
//! apprend l'assembleur : c'est là que le code écrit devient un fichier que le
//! système accepte de charger. Le panneau FORMAT ouvre le binaire produit
//! — ELF sous Linux, PE sous Windows — et montre la même chose des deux côtés :
//! où commence l'exécution, quelles sections existent, ce que chacune contient,
//! et quelles fonctions viennent d'ailleurs.
//!
//! Les deux formats se lisent volontairement à travers la même structure. La
//! leçon est là : `.text`, `.data`, un point d'entrée et une table de liaison
//! existent des deux côtés ; ce sont les noms et l'emballage qui changent.
//!
//! Ce module ne fait que décrire. Il ne charge rien, n'exécute rien, et
//! fonctionne sur un `.exe` Windows depuis Linux — c'est même son intérêt
//! principal, puisque le débogueur, lui, ne sait pas le dérouler.

use std::path::Path;

use object::{Object, ObjectSection, ObjectSymbol};

use crate::i18n::{self, Lang};

/// Une section du binaire, telle qu'elle est montrée à l'élève.
#[derive(Debug, Clone)]
pub struct SectionInfo {
    pub name: String,
    /// Adresse virtuelle une fois chargée (0 si la section n'est pas chargée).
    pub address: u64,
    pub size: u64,
    /// Taille occupée dans le fichier — nulle pour `.bss`, et c'est la moitié
    /// de la leçon : une section peut exister en mémoire sans coûter un octet
    /// sur le disque.
    pub file_size: u64,
    /// Droits résumés, façon `rwx`.
    pub perms: String,
    /// À quoi sert cette section, en une phrase.
    pub role: String,
}

/// Une fonction empruntée à une bibliothèque extérieure.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    pub library: String,
    pub name: String,
}

/// Tout ce que le panneau FORMAT affiche d'un binaire.
#[derive(Debug, Clone)]
pub struct Overview {
    /// « ELF64 », « PE32+ », « COFF »…
    pub format: String,
    pub arch: String,
    /// Exécutable, objet relogeable, bibliothèque partagée.
    pub kind: String,
    /// Point d'entrée (adresse virtuelle complète, base d'image comprise).
    pub entry: u64,
    /// Adresse de chargement de l'image (0 pour un ELF non relogeable classique).
    pub image_base: u64,
    pub sections: Vec<SectionInfo>,
    pub imports: Vec<ImportInfo>,
    /// Symboles globaux définis, avec leur adresse : les étiquettes du source.
    pub symbols: Vec<(String, u64)>,
    /// Remarques pédagogiques sur ce binaire précis.
    pub notes: Vec<String>,
    /// Taille du fichier sur le disque.
    pub file_size: u64,
    /// Pile non exécutable — une des trois protections du chapitre "NX, RELRO
    /// et PIE". Vrai dès qu'aucun `PT_GNU_STACK` ne réclame `PF_X` : sans ce
    /// segment, Linux ne rend pas la pile exécutable pour autant, il applique
    /// son défaut, qui est justement NX (vérifié dans les tests). `None` hors
    /// ELF — le PE n'a pas cette notion, son équivalent `/NXCOMPAT` est un bit
    /// d'en-tête que ce module ne lit pas encore — et `None` aussi si les
    /// program headers ne se relisent pas : non mesuré n'est pas absent.
    pub nx: Option<bool>,
    /// Ce que le chargeur doit écrire pendant le chargement est remis en
    /// lecture seule ensuite (`PT_GNU_RELRO`) — une autre des trois
    /// protections du même chapitre. Même règle de `None` que [`Self::nx`].
    /// La troisième, PIE, se lit dans [`Self::kind`].
    pub relro: Option<bool>,
}

/// Ouvre `path` et le décrit. `lang` sert aux explications, pas à la lecture.
pub fn inspect(path: &Path, lang: Lang) -> Result<Overview, String> {
    let data = std::fs::read(path).map_err(|e| format!("lecture de {}: {e}", path.display()))?;
    let file = object::File::parse(&*data).map_err(|e| format!("format non reconnu: {e}"))?;
    let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);

    let format = match file.format() {
        object::BinaryFormat::Elf => {
            if file.is_64() { "ELF64" } else { "ELF32" }
        }
        object::BinaryFormat::Pe => {
            // « PE32+ » est le nom officiel du PE 64 bits : les adresses y sont
            // sur 64 bits, mais le format reste celui du PE32.
            if file.is_64() { "PE32+" } else { "PE32" }
        }
        object::BinaryFormat::Coff => "COFF",
        object::BinaryFormat::MachO => "Mach-O",
        _ => "?",
    }
    .to_string();

    let arch = match file.architecture() {
        object::Architecture::X86_64 => "x86-64".to_string(),
        object::Architecture::I386 => "x86 (32 bits)".to_string(),
        object::Architecture::Aarch64 => "ARM64".to_string(),
        other => format!("{other:?}"),
    };

    let kind = match file.kind() {
        object::ObjectKind::Executable => {
            t("exécutable", "executable", "ejecutable").to_string()
        }
        object::ObjectKind::Relocatable => t(
            "objet relogeable (pas encore lié)",
            "relocatable object (not linked yet)",
            "objeto reubicable (aún sin enlazar)",
        )
        .to_string(),
        // Un exécutable position-indépendant (`ld -pie`) est un `ET_DYN`, tout
        // comme une bibliothèque partagée : c'est le même type ELF. Ce qui les
        // sépare est le point d'entrée — un programme en a un, une
        // bibliothèque le laisse à zéro. Annoncer « bibliothèque partagée » à
        // l'élève qui vient de lier son propre programme en PIE serait le
        // désigner par ce qu'il n'est pas.
        object::ObjectKind::Dynamic if file.entry() != 0 => t(
            "exécutable position-indépendant (PIE)",
            "position-independent executable (PIE)",
            "ejecutable independiente de la posición (PIE)",
        )
        .to_string(),
        object::ObjectKind::Dynamic => {
            t("bibliothèque partagée", "shared library", "biblioteca compartida").to_string()
        }
        other => format!("{other:?}"),
    };

    let image_base = file.relative_address_base();

    let mut sections = Vec::new();
    for sec in file.sections() {
        let name = sec.name().unwrap_or("?").to_string();
        let file_size = sec.file_range().map_or(0, |(_, len)| len);
        let flags_rwx = perms_of(&sec);
        sections.push(SectionInfo {
            role: section_role(&name, lang).to_string(),
            name,
            address: sec.address(),
            size: sec.size(),
            file_size,
            perms: flags_rwx,
        });
    }

    let imports: Vec<ImportInfo> = file
        .imports()
        .map(|list| {
            list.flatten()
                .map(|i| ImportInfo {
                    library: String::from_utf8_lossy(i.library()).into_owned(),
                    // Une DLL peut être appelée par numéro plutôt que par nom :
                    // le dire vaut mieux que d'afficher une ligne vide.
                    name: match i.name() {
                        object::NameOrOrdinal::Name(n) => String::from_utf8_lossy(n).into_owned(),
                        object::NameOrOrdinal::Ordinal(o) => format!("#{o}"),
                    },
                })
                .collect()
        })
        .unwrap_or_default();

    let mut symbols: Vec<(String, u64)> = file
        .symbols()
        .filter(|s| s.is_global() && s.is_definition())
        .filter_map(|s| s.name().ok().map(|n| (n.to_string(), s.address())))
        .collect();
    symbols.sort_by_key(|(_, a)| *a);

    let mut notes = Vec::new();
    if file.format() == object::BinaryFormat::Pe {
        notes.push(
            t(
                "Ce binaire est un exécutable Windows. ASM Studio l'assemble, le lit, et l'exécute si Wine est installé — mais il ne le déroule pas instruction par instruction : le pas-à-pas reste réservé à la cible Linux.",
                "This is a Windows executable. ASM Studio assembles it, reads it, and runs it when Wine is installed — but it does not walk it instruction by instruction: single-stepping stays reserved for the Linux target.",
                "Este es un ejecutable de Windows. ASM Studio lo ensambla, lo lee y lo ejecuta si Wine está instalado — pero no lo recorre instrucción por instrucción: el paso a paso queda reservado al destino Linux.",
            )
            .to_string(),
        );
        notes.push(
            t(
                "Les fonctions importées sont résolues au chargement : le programme ne contient pas leur code, seulement leur nom et une case où Windows écrira leur adresse (l'IAT).",
                "Imported functions are resolved at load time: the program holds no code for them, only their name and a slot where Windows will write their address (the IAT).",
                "Las funciones importadas se resuelven al cargar: el programa no contiene su código, solo su nombre y una casilla donde Windows escribirá su dirección (la IAT).",
            )
            .to_string(),
        );
    }
    if sections.iter().any(|s| s.file_size == 0 && s.size > 0) {
        notes.push(
            t(
                "Une section pèse zéro octet dans le fichier mais occupe de la place en mémoire : c'est .bss, les variables non initialisées, que le système met à zéro au chargement.",
                "One section takes zero bytes in the file but occupies memory: that is .bss, the uninitialized variables, which the system zeroes at load time.",
                "Una sección ocupa cero bytes en el archivo pero sí en memoria: es .bss, las variables sin inicializar, que el sistema pone a cero al cargar.",
            )
            .to_string(),
        );
    }
    if file.format() == object::BinaryFormat::Elf
        && file.kind() == object::ObjectKind::Dynamic
        && file.entry() != 0
    {
        notes.push(
            t(
                "Les adresses ci-dessous sont celles du lien, pas celles de l'exécution : ce binaire est chargeable n'importe où, et le noyau le placera ailleurs. C'est pourquoi son code n'a le droit de désigner ses données que par une distance depuis RIP.",
                "The addresses below are link addresses, not run addresses: this binary is loadable anywhere, and the kernel will put it elsewhere. That is why its code may only name its data by a distance from RIP.",
                "Las direcciones de abajo son las del enlace, no las de la ejecución: este binario se puede cargar en cualquier lugar, y el núcleo lo situará en otro sitio. Por eso su código solo puede designar sus datos por una distancia desde RIP.",
            )
            .to_string(),
        );
    }
    let protections = if file.format() == object::BinaryFormat::Elf {
        elf_protections(&data)
    } else {
        None
    };
    let (nx, relro) = match &protections {
        Some(p) => (Some(p.nx), Some(p.relro)),
        None => (None, None),
    };
    if protections.as_ref().is_some_and(|p| !p.stack_marked) {
        notes.push(
            t(
                "Aucun segment GNU_STACK dans ce binaire : NASM n'écrit pas la section \
                 .note.GNU-stack que gcc ajoute d'habitude, donc ld ne pose aucun marqueur et \
                 laisse le noyau trancher. Sous Linux x86-64, un binaire 64 bits sans marqueur \
                 reçoit une pile non exécutable : la protection est bien là, mais tenue par le \
                 défaut du noyau plutôt que réclamée par le fichier. Un programme issu de gcc, \
                 lui, porte le marqueur et l'exige.",
                "No GNU_STACK segment in this binary: NASM does not emit the .note.GNU-stack \
                 section that gcc usually adds, so ld writes no marker at all and leaves the \
                 decision to the kernel. On Linux x86-64 a 64-bit binary with no marker gets a \
                 non-executable stack: the protection is there, but held up by the kernel default \
                 rather than demanded by the file. A gcc-built program carries the marker and \
                 requires it.",
                "Ningún segmento GNU_STACK en este binario: NASM no escribe la sección \
                 .note.GNU-stack que gcc suele añadir, así que ld no pone ningún marcador y deja \
                 decidir al núcleo. En Linux x86-64, un binario de 64 bits sin marcador recibe una \
                 pila no ejecutable: la protección está ahí, pero sostenida por el valor \
                 predeterminado del núcleo y no exigida por el archivo. Un programa hecho con gcc \
                 sí lleva el marcador y lo exige.",
            )
            .to_string(),
        );
    }
    if imports.is_empty() && file.kind() == object::ObjectKind::Executable {
        notes.push(
            t(
                "Aucun import : ce programme ne dépend d'aucune bibliothèque et parle directement au noyau par ses appels système.",
                "No imports: this program depends on no library and talks to the kernel directly through system calls.",
                "Sin importaciones: este programa no depende de ninguna biblioteca y habla directamente con el núcleo mediante llamadas al sistema.",
            )
            .to_string(),
        );
    }

    Ok(Overview {
        format,
        arch,
        kind,
        entry: file.entry(),
        image_base,
        sections,
        imports,
        symbols,
        notes,
        file_size: data.len() as u64,
        nx,
        relro,
    })
}

/// Ce que les program headers d'un ELF disent des protections.
struct ElfProtections {
    /// Pile non exécutable.
    nx: bool,
    /// Un `PT_GNU_STACK` est présent, quel que soit son contenu : la pile est
    /// alors décrite par le fichier, et non laissée au défaut du noyau. Sert
    /// à expliquer le cas d'ici, où ce segment manque — pas à décider [`nx`].
    ///
    /// [`nx`]: Self::nx
    stack_marked: bool,
    relro: bool,
}

/// Lit les program headers `PT_GNU_STACK`/`PT_GNU_RELRO` d'un ELF pour en
/// tirer deux des trois protections du chapitre "NX, RELRO et PIE" (la
/// troisième, PIE, se lit sur le type ELF lui-même — voir [`inspect`]).
///
/// L'absence de `PT_GNU_STACK` ne vaut pas pile exécutable : c'est même
/// l'inverse. `ld` n'écrit ce segment que si un objet d'entrée porte une
/// section `.note.GNU-stack` — ce que NASM ne fait pas — et le noyau, faute
/// de marqueur, applique son défaut, qui pour un binaire 64 bits sous x86-64
/// est une pile non exécutable. Seul un `PT_GNU_STACK` portant `PF_X`
/// (c'est-à-dire `ld -z execstack`) rend vraiment la pile exécutable. Les
/// deux tests plus bas le vérifient sur des binaires réellement liés.
///
/// `None` si l'en-tête ne se relit pas — ne devrait pas arriver, puisque
/// `object::File::parse` l'a déjà accepté plus haut, mais mieux vaut alors
/// ne rien afficher que d'annoncer des protections absentes qu'on n'a pas su
/// mesurer.
///
/// Volontairement silencieux sur le distinguo RELRO partiel/complet (ce
/// dernier suppose en plus une liaison immédiate, `-z now`, que le lieur
/// natif de cet IDE ne demande jamais) : un module qui ne fait que décrire
/// n'invente pas un état qu'aucun binaire produit ici ne peut prendre.
fn elf_protections(data: &[u8]) -> Option<ElfProtections> {
    use object::read::elf::FileHeader as _;
    use object::read::elf::ProgramHeader as _;
    let header = object::elf::FileHeader64::<object::Endianness>::parse(data).ok()?;
    let endian = header.endian().ok()?;
    let headers = header.program_headers(endian, data).ok()?;
    let mut prot = ElfProtections { nx: true, stack_marked: false, relro: false };
    for ph in headers {
        match ph.p_type(endian) {
            object::elf::PT_GNU_STACK => {
                prot.stack_marked = true;
                prot.nx = ph.p_flags(endian).0 & object::elf::PF_X.0 == 0;
            }
            object::elf::PT_GNU_RELRO => prot.relro = true,
            _ => {}
        }
    }
    Some(prot)
}

/// Droits d'une section, résumés en `rwx`.
fn perms_of(sec: &object::Section<'_, '_>) -> String {
    let (r, w, x) = match sec.flags() {
        object::SectionFlags::Elf { sh_flags, .. } => (
            true,
            sh_flags.0 & object::elf::SHF_WRITE.0 != 0,
            sh_flags.0 & object::elf::SHF_EXECINSTR.0 != 0,
        ),
        object::SectionFlags::Coff { characteristics } => (
            characteristics.0 & object::pe::IMAGE_SCN_MEM_READ.0 != 0,
            characteristics.0 & object::pe::IMAGE_SCN_MEM_WRITE.0 != 0,
            characteristics.0 & object::pe::IMAGE_SCN_MEM_EXECUTE.0 != 0,
        ),
        _ => (true, false, false),
    };
    format!(
        "{}{}{}",
        if r { "r" } else { "-" },
        if w { "w" } else { "-" },
        if x { "x" } else { "-" }
    )
}

/// Ce que contient une section usuelle. Le nom est le même des deux côtés pour
/// les trois premières — c'est ce qui permet de dire à l'élève que les formats
/// diffèrent moins qu'il n'y paraît.
pub fn section_role(name: &str, lang: Lang) -> &'static str {
    let t = |fr: &'static str, en: &'static str, es: &'static str| i18n::tr3(lang, fr, en, es);
    match name {
        ".text" | ".code" => t(
            "le code machine : c'est ici que RIP se promène",
            "the machine code: this is where RIP walks",
            "el código máquina: aquí es donde pasea RIP",
        ),
        ".data" => t(
            "les variables initialisées, modifiables",
            "initialized, writable variables",
            "las variables inicializadas y modificables",
        ),
        ".bss" => t(
            "les variables non initialisées : zéro octet dans le fichier, mises à zéro au chargement",
            "uninitialized variables: zero bytes in the file, zeroed at load time",
            "variables sin inicializar: cero bytes en el archivo, puestas a cero al cargar",
        ),
        ".rodata" | ".rdata" => t(
            "les constantes : lisibles, non modifiables",
            "constants: readable, not writable",
            "las constantes: legibles, no modificables",
        ),
        ".idata" => t(
            "la table d'import : les fonctions que Windows ira chercher dans les DLL",
            "the import table: the functions Windows will fetch from the DLLs",
            "la tabla de importación: las funciones que Windows buscará en las DLL",
        ),
        ".edata" => t(
            "la table d'export : ce que ce fichier offre aux autres",
            "the export table: what this file offers to others",
            "la tabla de exportación: lo que este archivo ofrece a los demás",
        ),
        ".reloc" => t(
            "les corrections à appliquer si l'image n'est pas chargée à son adresse préférée",
            "the fixups to apply if the image is not loaded at its preferred address",
            "las correcciones a aplicar si la imagen no se carga en su dirección preferida",
        ),
        ".plt" | ".got" | ".got.plt" => t(
            "l'aiguillage vers les fonctions des bibliothèques partagées",
            "the switchboard to shared-library functions",
            "el conmutador hacia las funciones de bibliotecas compartidas",
        ),
        ".symtab" | ".strtab" | ".shstrtab" => t(
            "les noms des symboles et des sections — pour les outils, pas pour le processeur",
            "symbol and section names — for the tools, not for the processor",
            "los nombres de símbolos y secciones — para las herramientas, no para el procesador",
        ),
        ".comment" => t(
            "une signature laissée par les outils d'assemblage",
            "a signature left by the build tools",
            "una firma dejada por las herramientas de compilación",
        ),
        n if n.starts_with(".debug") => t(
            "les informations de débogage",
            "debugging information",
            "la información de depuración",
        ),
        _ => t("section propre à ce programme", "program-specific section", "sección propia de este programa"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble::{self, Target};
    use std::path::{Path, PathBuf};

    fn build(name: &str, target: Target) -> PathBuf {
        let out = assemble::assemble_for(
            Path::new("examples/test.asm"),
            &Path::new("build").join(name),
            &[],
            target,
            crate::i18n::Lang::Fr,
        );
        out.expect("assemblage").binary
    }

    /// Un ELF produit par la chaîne Linux se décrit : format, entrée, sections.
    #[test]
    fn elf_executable_is_described() {
        let bin = build("fmt-elf", Target::Linux);
        let o = inspect(&bin, Lang::Fr).expect("lecture ELF");
        assert_eq!(o.format, "ELF64");
        assert_eq!(o.arch, "x86-64");
        assert!(o.entry != 0, "un exécutable a un point d'entrée");
        assert!(o.sections.iter().any(|s| s.name == ".text" && s.perms.contains('x')));
        assert!(
            o.symbols.iter().any(|(n, _)| n == "_start"),
            "le symbole d'entrée doit être listé"
        );
        // Vérifié avec `readelf -lW` : le lien natif ne pose aucun segment
        // GNU_STACK (NASM n'écrit pas .note.GNU-stack) ni GNU_RELRO (pas de
        // section dynamique à protéger sans -pie). Sans GNU_STACK, la pile
        // n'en est pas exécutable pour autant — voir
        // `an_unmarked_stack_is_not_executable_at_run_time`.
        assert_eq!(o.nx, Some(true), "sans GNU_STACK, le noyau laisse la pile non exécutable");
        assert_eq!(o.relro, Some(false), "pas de segment GNU_RELRO sans -pie");
        assert!(
            o.notes.iter().any(|n| n.contains("GNU_STACK")),
            "l'absence de marqueur doit être expliquée, pas seulement chiffrée"
        );
    }

    /// La preuve que l'absence de `PT_GNU_STACK` ne veut pas dire « pile
    /// exécutable » : un binaire lié comme ceux d'ASM Studio meurt d'un
    /// SIGSEGV en sautant sur du code posé sur la pile, alors que le même
    /// objet lié `-z execstack` s'exécute jusqu'au bout. C'est ce qui autorise
    /// le panneau FORMAT à afficher NX en vert pour ces binaires.
    #[test]
    #[cfg(target_os = "linux")]
    fn an_unmarked_stack_is_not_executable_at_run_time() {
        let dir = Path::new("build/fmt-execstack");
        std::fs::create_dir_all(dir).expect("dossier");
        let asm = dir.join("saut_pile.asm");
        // Écrit « mov eax,60 ; xor edi,edi ; syscall » sur la pile, puis y saute.
        std::fs::write(
            &asm,
            "bits 64\nsection .text\n  global _start\n_start:\n  sub rsp, 16\n  \
             mov byte [rsp+0], 0xb8\n  mov dword [rsp+1], 60\n  mov byte [rsp+5], 0x31\n  \
             mov byte [rsp+6], 0xff\n  mov byte [rsp+7], 0x0f\n  mov byte [rsp+8], 0x05\n  \
             jmp rsp\n",
        )
        .expect("source");
        let bin = assemble::assemble_for(&asm, dir, &[], Target::Linux, crate::i18n::Lang::Fr)
            .expect("assemblage")
            .binary;

        let o = inspect(&bin, Lang::Fr).expect("lecture ELF");
        assert_eq!(o.nx, Some(true), "aucun GNU_STACK : NX par défaut du noyau");
        let status = std::process::Command::new(&bin).status().expect("exécution");
        assert!(
            !status.success(),
            "sauter sur la pile doit échouer : c'est ce que NX veut dire"
        );

        // Le même objet, relié en réclamant explicitement une pile exécutable,
        // va au bout — sans quoi le test ci-dessus ne prouverait rien.
        let exec_stack = dir.join("saut_pile_execstack");
        let ok = std::process::Command::new("ld")
            .args(["-z", "execstack", "-o"])
            .arg(&exec_stack)
            .arg(dir.join("saut_pile.o"))
            .status()
            .expect("ld");
        assert!(ok.success(), "lien -z execstack");
        assert_eq!(
            inspect(&exec_stack, Lang::Fr).expect("lecture").nx,
            Some(false),
            "un GNU_STACK portant PF_X, lui, veut bien dire pile exécutable"
        );
        assert!(
            std::process::Command::new(&exec_stack).status().expect("exécution").success(),
            "avec -z execstack, le même code doit s'exécuter"
        );
    }

    /// Un exécutable lié en PIE porte un segment GNU_RELRO (le lieur natif
    /// l'ajoute dès qu'il y a une section dynamique à protéger) mais reste,
    /// lui aussi, sans marqueur GNU_STACK : ce n'est pas -pie qui change ça.
    #[test]
    fn pie_executable_gains_relro_but_still_no_stack_marker() {
        let dir = Path::new("build/fmt-elf-pie");
        std::fs::create_dir_all(dir).expect("dossier");
        let bin = assemble::assemble_for_with(
            Path::new("examples_seed/pie_rip_relatif.asm"),
            dir,
            &[],
            Target::Linux,
            assemble::LinkOptions { pie: true },
            crate::i18n::Lang::Fr,
        )
        .expect("lien PIE")
        .binary;
        let o = inspect(&bin, Lang::Fr).expect("lecture ELF PIE");
        assert_eq!(o.relro, Some(true), "un lien -pie pose un segment GNU_RELRO");
        assert_eq!(o.nx, Some(true), "toujours pas de GNU_STACK, donc toujours le défaut du noyau");
        assert!(
            o.notes.iter().any(|n| n.contains("GNU_STACK")),
            "-pie n'ajoute pas le marqueur : l'explication reste due à l'élève"
        );
    }

    /// Le même source assemblé pour Windows se décrit de la même façon — c'est
    /// tout l'intérêt de l'explorateur : montrer que les deux formats disent la
    /// même chose autrement.
    #[test]
    fn pe_executable_is_described_the_same_way() {
        let dir = Path::new("build/fmt-pe");
        std::fs::create_dir_all(dir).expect("dossier");
        let asm = dir.join("win.asm");
        std::fs::write(
            &asm,
            "bits 64\ndefault rel\nsection .data\n  n dq 7\nsection .text\n  global main\n  \
             extern ExitProcess\nmain:\n  sub rsp, 40\n  mov ecx, [n]\n  call ExitProcess\n",
        )
        .expect("source");
        let bin = assemble::assemble_for(&asm, dir, &[], Target::Windows, crate::i18n::Lang::Fr)
            .expect("assemblage PE")
            .binary;

        let o = inspect(&bin, Lang::Fr).expect("lecture PE");
        assert_eq!(o.format, "PE32+");
        assert_eq!(o.arch, "x86-64");
        assert_eq!(o.image_base, crate::pe_link::IMAGE_BASE);
        assert!(o.entry >= o.image_base, "l'entrée est une adresse virtuelle complète");
        assert!(o.sections.iter().any(|s| s.name == ".text"));
        assert!(
            o.imports.iter().any(|i| i.name == "ExitProcess" && i.library == "kernel32.dll"),
            "l'import doit être visible: {:?}",
            o.imports
        );
        assert!(
            o.notes.iter().any(|n| n.contains("pas à pas") || n.contains("instruction par instruction")),
            "l'IDE doit prévenir qu'il ne déroulera pas ce binaire instruction par instruction"
        );
        // NX/RELRO sont une notion ELF (GNU_STACK/GNU_RELRO) : les taire pour
        // un PE plutôt qu'annoncer une protection absente qui n'a pas de sens
        // ici évite de laisser croire que ce format n'en offre aucune.
        assert_eq!(o.nx, None, "NX est une notion ELF, pas PE");
        assert_eq!(o.relro, None, "RELRO est une notion ELF, pas PE");
    }

    /// Chaque section usuelle a une explication, dans les trois langues.
    #[test]
    fn every_usual_section_is_explained() {
        for lang in [Lang::Fr, Lang::En, Lang::Es] {
            for name in [".text", ".data", ".bss", ".rdata", ".idata", ".plt"] {
                assert!(
                    !section_role(name, lang).is_empty(),
                    "{name} sans explication en {lang:?}"
                );
            }
            // Une section inconnue reçoit quand même une phrase.
            assert!(!section_role(".maSection", lang).is_empty());
        }
    }
}
