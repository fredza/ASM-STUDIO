//! Lieur PE64 : transforme un objet COFF (`nasm -f win64`) en exécutable Windows.
//!
//! ASM Studio tourne sous Linux, où `ld` ne sait pas produire de PE et où les
//! bibliothèques d'import du SDK Microsoft (`kernel32.lib`…) n'existent pas.
//! Faire dépendre l'assemblage Windows d'un `lld-link` et d'un SDK à installer
//! reviendrait à ne pas le proposer du tout. Le lien est donc fait ici, à la
//! main : c'est du travail, mais c'est aussi la partie que l'élève doit
//! comprendre — un exécutable n'est qu'un en-tête, des sections, et une table
//! qui dit au chargeur où trouver `ExitProcess`.
//!
//! Ce que le lieur fait :
//!
//! * regroupe les sections de l'objet (`.text`, `.rdata`, `.data`, `.bss`) ;
//! * résout les symboles définis, et transforme les `extern` en imports de DLL
//!   grâce au catalogue de [`dll_for`] ;
//! * pose une table d'import complète (descripteurs, ILT, IAT, noms) ;
//! * fabrique un *thunk* `jmp [rip+…]` par fonction importée, pour qu'un
//!   `call ExitProcess` écrit naïvement atteigne l'entrée d'IAT ;
//! * applique les relocations `REL32`, `REL32_1..5`, `ADDR64` et `ADDR32NB`.
//!
//! Ce qu'il ne fait pas : plusieurs objets à lier ensemble, les exports, les
//! ressources, la relocation de base (l'image est donc chargée à adresse fixe,
//! sans ASLR), et les données de déroulement d'exception (`.pdata`). Un
//! programme d'apprentissage n'en a pas besoin ; le jour où il en aura besoin,
//! ces limites sont dites ici plutôt que découvertes à l'exécution.
//!
//! ## Ce que l'IDE ne peut pas faire, et le dit
//!
//! Le binaire produit est un vrai PE. ASM Studio l'assemble, le donne à lire
//! (voir [`crate::binfmt`]) et, quand Wine est installé, l'exécute pour de bon
//! (voir [`crate::winerun`]). Ce qu'il ne fait pas, c'est le **déboguer** : le
//! pas-à-pas repose sur `ptrace` et sur les adresses de l'image qu'on vient
//! d'écrire, deux choses qu'un PE lancé derrière le chargeur de Wine n'a plus.
//! Le débogueur reste donc réservé à la cible ELF.

use std::collections::BTreeMap;
use std::path::Path;

use crate::i18n::{self, Lang};

use object::pe;
use object::write::pe::{NtHeaders, Writer};
use object::{Object, ObjectSection, ObjectSymbol, RelocationFlags, RelocationTarget};

/// Adresse de chargement. Fixe, faute de section `.reloc` : Windows charge donc
/// l'image ici même. C'est l'adresse classique des exécutables 64 bits, choisie
/// assez haute pour qu'une adresse absolue ne puisse pas se confondre avec un
/// petit entier lu par erreur.
pub const IMAGE_BASE: u64 = 0x0000_0001_4000_0000;

/// Taille d'un thunk `jmp qword [rip+disp32]` (`FF 25` + 4 octets).
const THUNK_LEN: u32 = 6;

/// Noms acceptés pour le point d'entrée, par ordre de préférence.
///
/// `main` d'abord parce que c'est celui qu'écrivent les cours Windows, `start`
/// et `_start` ensuite pour qui vient du monde ELF et garde ses habitudes.
const ENTRY_NAMES: [&str; 5] = ["main", "start", "_start", "WinMain", "mainCRTStartup"];

/// DLL qui fournit une fonction, pour les `extern` du source.
///
/// Sans ce catalogue, l'élève devrait déclarer lui-même de quelle bibliothèque
/// vient `ExitProcess` — une notion qui n'a rien à voir avec l'assembleur qu'il
/// apprend. Les fonctions retenues sont celles qu'un programme écrit à la main
/// appelle vraiment : sortir, écrire à l'écran, lire au clavier, ouvrir un
/// fichier, demander de la mémoire.
///
/// Pour tout le reste, le nom peut porter sa DLL : `extern gdi32$CreatePen`
/// importe `CreatePen` depuis `gdi32.dll`. Le préfixe `__imp_` est également
/// reconnu : il désigne alors l'entrée d'IAT elle-même, comme chez Microsoft.
pub fn dll_for(name: &str) -> Option<&'static str> {
    const KERNEL32: &[&str] = &[
        "ExitProcess",
        "GetStdHandle",
        "WriteFile",
        "ReadFile",
        "WriteConsoleA",
        "WriteConsoleW",
        "ReadConsoleA",
        "ReadConsoleW",
        "GetLastError",
        "SetLastError",
        "Sleep",
        "CreateFileA",
        "CreateFileW",
        "CloseHandle",
        "SetFilePointer",
        "GetFileSize",
        "DeleteFileA",
        "FlushFileBuffers",
        "GetProcessHeap",
        "HeapAlloc",
        "HeapFree",
        "HeapReAlloc",
        "VirtualAlloc",
        "VirtualFree",
        "VirtualProtect",
        "GetCommandLineA",
        "GetCommandLineW",
        "GetModuleHandleA",
        "GetProcAddress",
        "LoadLibraryA",
        "FreeLibrary",
        "GetTickCount",
        "GetTickCount64",
        "GetSystemTime",
        "GetLocalTime",
        "GetConsoleMode",
        "SetConsoleMode",
        "SetConsoleTitleA",
        "GetCurrentProcess",
        "GetCurrentProcessId",
        "TerminateProcess",
        "GetEnvironmentVariableA",
        "Beep",
        "QueryPerformanceCounter",
        "QueryPerformanceFrequency",
    ];
    const USER32: &[&str] = &[
        "MessageBoxA",
        "MessageBoxW",
        "MessageBeep",
        "GetDesktopWindow",
        "CharToOemA",
    ];
    // msvcrt.dll est présent sur toutes les versions de Windows : c'est le
    // chemin le plus court vers `printf` pour qui apprend, sans redistribuable
    // à installer.
    const MSVCRT: &[&str] = &[
        "printf", "sprintf", "vprintf", "scanf", "sscanf", "puts", "putchar", "getchar", "gets",
        "fopen", "fclose", "fprintf", "fgets", "fputs", "fread", "fwrite", "malloc", "calloc",
        "realloc", "free", "exit", "abort", "atoi", "atof", "itoa", "strlen", "strcpy", "strncpy",
        "strcat", "strcmp", "strncmp", "strchr", "strstr", "memcpy", "memmove", "memset", "memcmp",
        "qsort", "rand", "srand", "time", "clock", "system", "toupper", "tolower", "isdigit",
    ];
    if KERNEL32.contains(&name) {
        Some("kernel32.dll")
    } else if USER32.contains(&name) {
        Some("user32.dll")
    } else if MSVCRT.contains(&name) {
        Some("msvcrt.dll")
    } else {
        None
    }
}

/// Sous-système : décide si Windows ouvre une console au lancement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Subsystem {
    /// Application console (`IMAGE_SUBSYSTEM_WINDOWS_CUI`) : une fenêtre de
    /// terminal s'ouvre, et `WriteFile` sur `STD_OUTPUT_HANDLE` y écrit.
    #[default]
    Console,
    /// Application graphique (`IMAGE_SUBSYSTEM_WINDOWS_GUI`) : aucune console,
    /// pour un programme qui n'affiche que des `MessageBox`.
    Gui,
}

/// Une fonction importée d'une DLL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub dll: String,
    pub func: String,
}

/// Ce que le lieur a produit, pour l'afficher dans le journal de compilation.
#[derive(Debug)]
pub struct LinkReport {
    /// Point d'entrée retenu, et sa RVA.
    pub entry: (String, u32),
    /// Fonctions importées, groupées telles qu'elles apparaissent dans la table.
    pub imports: Vec<Import>,
    /// Taille du fichier écrit.
    pub size: u64,
}

/// Où atterrit une section de l'objet dans l'image finale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Out {
    Text,
    RData,
    Data,
    Bss,
}

/// Une section de l'objet, placée dans une section de sortie.
struct Placed {
    out: Out,
    /// Décalage à l'intérieur de la section de sortie.
    offset: u32,
    /// Contenu (vide pour `.bss`).
    data: Vec<u8>,
}

/// Lie `obj` (un COFF x86-64) en un exécutable PE64 écrit dans `out`.
pub fn link(
    obj: &Path,
    out: &Path,
    subsystem: Subsystem,
    lang: Lang,
) -> Result<LinkReport, String> {
    let data = std::fs::read(obj).map_err(|e| format!("lecture de {}: {e}", obj.display()))?;
    let file = object::File::parse(&*data).map_err(|e| format!("objet illisible: {e}"))?;
    if file.format() != object::BinaryFormat::Coff {
        return Err(i18n::tr3(
            lang,
            "l'objet n'est pas au format COFF (assemblez avec « nasm -f win64 »)",
            "the object is not in COFF format (assemble with \"nasm -f win64\")",
            "el objeto no está en formato COFF (ensamble con «nasm -f win64»)",
        )
        .into());
    }
    if file.architecture() != object::Architecture::X86_64 {
        return Err(i18n::tr3(
            lang,
            "seul le x86-64 est pris en charge",
            "only x86-64 is supported",
            "solo se admite x86-64",
        )
        .into());
    }

    // 1) Répartir les sections de l'objet dans les quatre sections de sortie.
    let mut placed: BTreeMap<usize, Placed> = BTreeMap::new();
    let mut lens: BTreeMap<Out, u32> = BTreeMap::new();
    let mut skipped: Vec<String> = Vec::new();
    for sec in file.sections() {
        let name = sec.name().unwrap_or("").to_string();
        let Some(out_kind) = classify(&sec, &name) else {
            if !name.is_empty() {
                skipped.push(name);
            }
            continue;
        };
        // Les accès SSE alignés exigent au moins 16 octets, mais une section
        // peut demander davantage (`section ... align=32`). La fusion de
        // sections ne doit jamais affaiblir cette garantie du fichier objet.
        let cursor = lens.entry(out_kind).or_insert(0);
        let required_align = u32::try_from(sec.align())
            .map_err(|_| format!("section {name}: alignement trop grand"))?
            .max(16);
        *cursor = align(*cursor, required_align);
        let offset = *cursor;
        let bytes = if out_kind == Out::Bss {
            Vec::new()
        } else {
            sec.data()
                .map_err(|e| format!("section {name}: {e}"))?
                .to_vec()
        };
        let size = if out_kind == Out::Bss {
            sec.size() as u32
        } else {
            bytes.len() as u32
        };
        *cursor = offset + size;
        placed.insert(
            sec.index().0,
            Placed {
                out: out_kind,
                offset,
                data: bytes,
            },
        );
    }

    // 2) Recenser les symboles indéfinis : ce sont les imports à résoudre.
    //    L'ordre de découverte est conservé (BTreeMap sur le nom) pour qu'une
    //    même source produise toujours le même exécutable, octet pour octet.
    let mut imports: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut thunk_of: BTreeMap<String, u32> = BTreeMap::new(); // symbole nu → index de thunk
    let mut iat_of: BTreeMap<String, usize> = BTreeMap::new(); // symbole → index d'import
    for sym in file.symbols() {
        if !sym.is_undefined() {
            continue;
        }
        let raw = sym.name().map_err(|e| format!("symbole illisible: {e}"))?;
        if raw.is_empty() {
            continue;
        }
        let (dll, func, direct) = resolve_import(raw, lang)?;
        let n = imports.len();
        let idx = *imports.entry((dll, func)).or_insert(n);
        iat_of.insert(raw.to_string(), idx);
        if !direct {
            let n = thunk_of.len() as u32;
            thunk_of.entry(raw.to_string()).or_insert(n);
        }
    }
    let import_list: Vec<Import> = {
        let mut v: Vec<(usize, Import)> = imports
            .iter()
            .map(|((dll, func), i)| {
                (
                    *i,
                    Import {
                        dll: dll.clone(),
                        func: func.clone(),
                    },
                )
            })
            .collect();
        v.sort_by_key(|(i, _)| *i);
        v.into_iter().map(|(_, imp)| imp).collect()
    };

    // 3) Tailles des sections de sortie. Les thunks vivent à la fin de `.text`.
    let thunks_at = align(*lens.get(&Out::Text).unwrap_or(&0), 16);
    let text_len = thunks_at + thunk_of.len() as u32 * THUNK_LEN;
    let rdata_len = *lens.get(&Out::RData).unwrap_or(&0);
    let data_len = *lens.get(&Out::Data).unwrap_or(&0);
    let bss_len = *lens.get(&Out::Bss).unwrap_or(&0);
    // La table d'import a une taille indépendante de l'adresse où elle atterrit :
    // on la construit une première fois pour la mesurer, une seconde fois à sa
    // vraie place. L'égalité des deux longueurs est vérifiée plus bas.
    let idata_probe = build_idata(0, &import_list);
    let idata_len = idata_probe.bytes.len() as u32;

    // 4) Réserver l'espace : c'est cette passe qui fixe toutes les RVA.
    let mut buffer = Vec::new();
    let mut writer = Writer::new(true, 0x1000, 0x200, &mut buffer);
    let section_count = 1 // .text, toujours présente
        + u16::from(rdata_len > 0)
        + u16::from(idata_len > 0)
        + u16::from(data_len > 0)
        + u16::from(bss_len > 0);
    writer.reserve_dos_header_and_stub();
    writer.reserve_nt_headers(pe::IMAGE_NUMBEROF_DIRECTORY_ENTRIES);
    writer.reserve_section_headers(section_count);
    let text = writer.reserve_text_section(text_len);
    let rdata = (rdata_len > 0).then(|| writer.reserve_rdata_section(rdata_len));
    let idata = (idata_len > 0).then(|| writer.reserve_idata_section(idata_len));
    let data = (data_len > 0).then(|| writer.reserve_data_section(data_len, data_len));
    let bss = (bss_len > 0).then(|| writer.reserve_bss_section(bss_len));

    let rva_of = |out: Out| -> u32 {
        match out {
            Out::Text => text.virtual_address,
            Out::RData => rdata.map_or(0, |r| r.virtual_address),
            Out::Data => data.map_or(0, |r| r.virtual_address),
            Out::Bss => bss.map_or(0, |r| r.virtual_address),
        }
    };

    // 5) Table d'import à sa place définitive.
    let idata_rva = idata.map_or(0, |r| r.virtual_address);
    let idata_built = build_idata(idata_rva, &import_list);
    debug_assert_eq!(
        idata_built.bytes.len(),
        idata_probe.bytes.len(),
        "la table d'import doit avoir la même taille à toute adresse"
    );

    // 6) Adresse de chaque symbole. Un symbole importé vaut l'adresse de son
    //    thunk (`call ExitProcess`), sauf s'il est nommé `__imp_…`, auquel cas
    //    il désigne l'entrée d'IAT elle-même (`call [rel __imp_ExitProcess]`).
    let mut sym_rva: BTreeMap<usize, u32> = BTreeMap::new();
    for sym in file.symbols() {
        let name = sym.name().unwrap_or("");
        if sym.is_undefined() {
            if let Some(idx) = iat_of.get(name) {
                let rva = match thunk_of.get(name) {
                    Some(t) => text.virtual_address + thunks_at + t * THUNK_LEN,
                    None => idata_rva + idata_built.iat_offset[*idx],
                };
                sym_rva.insert(sym.index().0, rva);
            }
            continue;
        }
        let Some(sec_index) = sym.section_index() else {
            continue;
        };
        let Some(p) = placed.get(&sec_index.0) else {
            continue;
        };
        sym_rva.insert(
            sym.index().0,
            rva_of(p.out) + p.offset + sym.address() as u32,
        );
    }

    // 7) Point d'entrée.
    let (entry_name, entry_rva) = ENTRY_NAMES
        .iter()
        .find_map(|want| {
            file.symbols()
                .find(|s| s.is_global() && s.name().is_ok_and(|n| n == *want))
                .and_then(|s| {
                    sym_rva
                        .get(&s.index().0)
                        .map(|rva| (want.to_string(), *rva))
                })
        })
        .ok_or_else(|| {
            format!(
                "{} : {} « global main » ({} {}) {}",
                i18n::tr3(lang, "aucun point d'entrée", "no entry point", "sin punto de entrada"),
                i18n::tr3(lang, "déclarez", "declare", "declare"),
                i18n::tr3(lang, "ou", "or", "o"),
                ENTRY_NAMES[1..].join(", "),
                i18n::tr3(lang, "dans le source", "in the source", "en el código fuente"),
            )
        })?;

    // 8) Relocations : c'est ici que les adresses provisoires de l'objet
    //    deviennent les adresses réelles de l'image.
    let mut contents: BTreeMap<Out, Vec<u8>> = BTreeMap::new();
    contents.insert(Out::Text, vec![0; text_len as usize]);
    if rdata_len > 0 {
        contents.insert(Out::RData, vec![0; rdata_len as usize]);
    }
    if data_len > 0 {
        contents.insert(Out::Data, vec![0; data_len as usize]);
    }
    for (idx, p) in &placed {
        if p.data.is_empty() {
            continue;
        }
        let buf = contents.get_mut(&p.out).expect("section de sortie allouée");
        buf[p.offset as usize..p.offset as usize + p.data.len()].copy_from_slice(&p.data);
        let _ = idx;
    }
    for sec in file.sections() {
        let Some(p) = placed.get(&sec.index().0) else {
            continue;
        };
        if p.data.is_empty() {
            continue;
        }
        for (offset, reloc) in sec.relocations() {
            let RelocationTarget::Symbol(target) = reloc.target() else {
                return Err(i18n::tr3(
                    lang,
                    "relocation vers une cible non symbolique, non prise en charge",
                    "relocation to a non-symbolic target, not supported",
                    "reubicación hacia un destino no simbólico, no admitida",
                )
                .into());
            };
            let name = file
                .symbol_by_index(target)
                .ok()
                .and_then(|s| s.name().ok().map(str::to_string))
                .unwrap_or_default();
            let s = *sym_rva.get(&target.0).ok_or_else(|| {
                match dll_for(&name) {
                    // Le symbole existe au catalogue mais n'a pas été importé :
                    // ce cas ne devrait pas survenir, et le dire vaut mieux que
                    // d'écrire une adresse fausse.
                    Some(_) => format!("symbole « {name} » non résolu (erreur interne du lieur)"),
                    None => unknown_symbol_message(&name, lang),
                }
            })?;
            let place = rva_of(p.out) + p.offset + offset as u32;
            let buf = contents.get_mut(&p.out).expect("section de sortie allouée");
            let at = (p.offset + offset as u32) as usize;
            apply_reloc(buf, at, place, s, reloc.flags(), &name, lang)?;
        }
    }

    // 9) Thunks : `jmp qword [rip+disp32]` vers l'entrée d'IAT.
    {
        let text_buf = contents.get_mut(&Out::Text).expect(".text existe toujours");
        for (name, t) in &thunk_of {
            let idx = iat_of[name];
            let at = (thunks_at + t * THUNK_LEN) as usize;
            let here = text.virtual_address + thunks_at + t * THUNK_LEN;
            let iat = idata_rva + idata_built.iat_offset[idx];
            // Le déplacement se compte depuis la fin de l'instruction, d'où le +6.
            let disp = (iat as i64 - (here as i64 + THUNK_LEN as i64)) as i32;
            text_buf[at] = 0xFF;
            text_buf[at + 1] = 0x25;
            text_buf[at + 2..at + 6].copy_from_slice(&disp.to_le_bytes());
        }
    }

    // 10) Écriture.
    let subsystem_value = match subsystem {
        Subsystem::Console => pe::IMAGE_SUBSYSTEM_WINDOWS_CUI,
        Subsystem::Gui => pe::IMAGE_SUBSYSTEM_WINDOWS_GUI,
    };
    if let Some(r) = idata {
        writer.set_data_directory(
            pe::IMAGE_DIRECTORY_ENTRY_IAT,
            r.virtual_address + idata_built.iat_start,
            idata_built.iat_size,
        );
    }
    writer
        .write_dos_header_and_stub()
        .map_err(|e| format!("en-tête DOS: {e}"))?;
    writer.write_nt_headers(NtHeaders {
        machine: pe::IMAGE_FILE_MACHINE_AMD64,
        time_date_stamp: 0,
        // Pas de `.reloc` : l'image doit être chargée à `IMAGE_BASE`, et le
        // dire au chargeur (RELOCS_STRIPPED) est plus honnête que de le
        // laisser découvrir qu'il n'y a rien à relocaliser.
        characteristics: pe::IMAGE_FILE_EXECUTABLE_IMAGE
            | pe::IMAGE_FILE_LARGE_ADDRESS_AWARE
            | pe::IMAGE_FILE_RELOCS_STRIPPED,
        major_linker_version: 0,
        minor_linker_version: 1,
        address_of_entry_point: entry_rva,
        image_base: IMAGE_BASE,
        major_operating_system_version: 6,
        minor_operating_system_version: 0,
        major_image_version: 0,
        minor_image_version: 0,
        // 6.0 : Windows Vista et au-delà. Une valeur plus basse ferait refuser
        // l'image par les versions récentes.
        major_subsystem_version: 6,
        minor_subsystem_version: 0,
        subsystem: subsystem_value,
        dll_characteristics: pe::IMAGE_DLLCHARACTERISTICS_NX_COMPAT
            | pe::IMAGE_DLLCHARACTERISTICS_TERMINAL_SERVER_AWARE,
        size_of_stack_reserve: 0x100000,
        size_of_stack_commit: 0x1000,
        size_of_heap_reserve: 0x100000,
        size_of_heap_commit: 0x1000,
    });
    writer.write_section_headers();
    writer.write_section(text.file_offset, contents.get(&Out::Text).expect(".text"));
    if let Some(r) = rdata {
        writer.write_section(r.file_offset, contents.get(&Out::RData).expect(".rdata"));
    }
    if let Some(r) = idata {
        writer.write_section(r.file_offset, &idata_built.bytes);
    }
    if let Some(r) = data {
        writer.write_section(r.file_offset, contents.get(&Out::Data).expect(".data"));
    }
    // `.bss` n'a pas d'octets dans le fichier : c'est tout son intérêt.

    std::fs::write(out, &buffer).map_err(|e| format!("écriture de {}: {e}", out.display()))?;
    Ok(LinkReport {
        entry: (entry_name, entry_rva),
        imports: import_list,
        size: buffer.len() as u64,
    })
}

/// Message d'erreur pour un `extern` que le catalogue ne connaît pas. Il doit
/// donner la sortie de secours, sinon l'élève est bloqué sans recours.
fn unknown_symbol_message(name: &str, lang: Lang) -> String {
    match lang {
        Lang::Fr => format!(
            "« {name} » n'est ni défini dans le programme, ni connu du catalogue de DLL.\n\
             S'il s'agit d'une fonction Windows, nommez sa bibliothèque : « extern user32${name} » \
             pour l'importer de user32.dll.\n\
             S'il s'agit d'une étiquette à vous, elle manque au source (faute de frappe ?)."
        ),
        Lang::En => format!(
            "\"{name}\" is neither defined in the program nor known to the DLL catalogue.\n\
             If it is a Windows function, name its library: \"extern user32${name}\" \
             imports it from user32.dll.\n\
             If it is a label of your own, it is missing from the source (a typo?)."
        ),
        Lang::Es => format!(
            "«{name}» no está definido en el programa ni figura en el catálogo de DLL.\n\
             Si es una función de Windows, nombre su biblioteca: «extern user32${name}» \
             la importa de user32.dll.\n\
             Si es una etiqueta suya, falta en el código fuente (¿una errata?)."
        ),
    }
}

/// Décide de quelle DLL vient un symbole indéfini.
///
/// Rend `(dll, fonction, direct)`, où `direct` dit que le symbole désigne
/// l'entrée d'IAT (`__imp_…`) et non une adresse de code appelable.
fn resolve_import(raw: &str, lang: Lang) -> Result<(String, String, bool), String> {
    let (explicit_dll, rest) = match raw.split_once('$') {
        Some((dll, func)) if !dll.is_empty() && !func.is_empty() => {
            let dll = if dll.ends_with(".dll") {
                dll.to_string()
            } else {
                format!("{dll}.dll")
            };
            (Some(dll), func)
        }
        _ => (None, raw),
    };
    let (direct, func) = match rest.strip_prefix("__imp_") {
        Some(f) => (true, f),
        None => (false, rest),
    };
    let dll = match explicit_dll {
        Some(d) => d,
        None => dll_for(func)
            .ok_or_else(|| unknown_symbol_message(raw, lang))?
            .to_string(),
    };
    Ok((dll, func.to_string(), direct))
}

/// Applique une relocation COFF x86-64 en place.
///
/// `place` est la RVA de l'emplacement patché, `target` celle du symbole visé.
/// L'addend est déjà dans les octets à patcher (les relocations COFF sont
/// implicites, contrairement aux `RELA` d'ELF) : il faut donc lire avant
/// d'écrire, sous peine de perdre le `+ 8` d'un `mov rax, [tableau + 8]`.
fn apply_reloc(
    buf: &mut [u8],
    at: usize,
    place: u32,
    target: u32,
    flags: RelocationFlags,
    name: &str,
    lang: Lang,
) -> Result<(), String> {
    let RelocationFlags::Coff { typ } = flags else {
        return Err(format!(
            "{} « {name} »",
            i18n::tr3(
                lang,
                "relocation de format inattendu sur",
                "unexpected relocation format on",
                "formato de reubicación inesperado en",
            )
        ));
    };
    let read32 = |b: &[u8]| i32::from_le_bytes(b[at..at + 4].try_into().expect("4 octets"));
    match typ {
        pe::IMAGE_REL_AMD64_ABSOLUTE => {} // ne rien faire, par définition
        pe::IMAGE_REL_AMD64_ADDR64 => {
            let addend = i64::from_le_bytes(buf[at..at + 8].try_into().expect("8 octets"));
            let value = IMAGE_BASE as i64 + target as i64 + addend;
            buf[at..at + 8].copy_from_slice(&value.to_le_bytes());
        }
        pe::IMAGE_REL_AMD64_ADDR32NB => {
            let value = target as i64 + read32(buf) as i64;
            let v =
                i32::try_from(value).map_err(|_| format!("adresse hors limites sur « {name} »"))?;
            buf[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        // REL32 et ses cinq variantes : la distance se compte depuis la fin de
        // l'instruction, et le suffixe dit combien d'octets la suivent encore
        // (un immédiat, typiquement `cmp dword [rel x], 1`).
        pe::IMAGE_REL_AMD64_REL32
        | pe::IMAGE_REL_AMD64_REL32_1
        | pe::IMAGE_REL_AMD64_REL32_2
        | pe::IMAGE_REL_AMD64_REL32_3
        | pe::IMAGE_REL_AMD64_REL32_4
        | pe::IMAGE_REL_AMD64_REL32_5 => {
            let extra = (typ.0 - pe::IMAGE_REL_AMD64_REL32.0) as i64;
            let addend = read32(buf) as i64;
            let value = target as i64 - (place as i64 + 4 + extra) + addend;
            let v = i32::try_from(value)
                .map_err(|_| format!("saut trop long vers « {name} » (au-delà de ±2 Go)"))?;
            buf[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        pe::IMAGE_REL_AMD64_ADDR32 => {
            return Err(match lang {
                Lang::Fr => format!(
                    "« {name} » est utilisé en adresse absolue 32 bits, que l'image chargée à \
                     0x{IMAGE_BASE:X} ne permet pas. Utilisez un adressage relatif (« lea rax, [rel {name}] »)."
                ),
                Lang::En => format!(
                    "\"{name}\" is used as a 32-bit absolute address, which an image loaded at \
                     0x{IMAGE_BASE:X} does not allow. Use RIP-relative addressing (\"lea rax, [rel {name}]\")."
                ),
                Lang::Es => format!(
                    "«{name}» se usa como dirección absoluta de 32 bits, algo que una imagen cargada en \
                     0x{IMAGE_BASE:X} no permite. Use direccionamiento relativo («lea rax, [rel {name}]»)."
                ),
            });
        }
        other => {
            return Err(format!(
                "{} {other:?} {} (« {name} »)",
                i18n::tr3(lang, "relocation COFF de type", "COFF relocation of type", "reubicación COFF de tipo"),
                i18n::tr3(lang, "non prise en charge", "is not supported", "no admitida"),
            ));
        }
    }
    Ok(())
}

/// Table d'import construite : les octets, plus les repères dont le lieur a
/// besoin pour viser les entrées d'IAT.
struct Idata {
    bytes: Vec<u8>,
    /// Décalage, dans la section, de l'entrée d'IAT de chaque import.
    iat_offset: Vec<u32>,
    /// Décalage et taille de l'IAT complète (répertoire de données n° 12).
    iat_start: u32,
    iat_size: u32,
}

/// Construit la table d'import complète pour une section placée à `base`.
///
/// La disposition est celle qu'attend le chargeur Windows :
///
/// ```text
///   descripteurs (20 octets par DLL, + un nul final)
///   ILT   : une liste de RVA vers les noms, par DLL, terminée par 0
///   IAT   : la même liste — le chargeur y écrit les adresses réelles
///   noms  : « hint » (2 octets) + nom + 0, puis les noms de DLL
/// ```
///
/// La longueur du résultat ne dépend pas de `base` : c'est ce qui permet de la
/// mesurer avant de savoir où la section atterrira.
fn build_idata(base: u32, imports: &[Import]) -> Idata {
    // Grouper par DLL en gardant l'ordre d'apparition.
    let mut dlls: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, imp) in imports.iter().enumerate() {
        match dlls.iter_mut().find(|(d, _)| *d == imp.dll) {
            Some((_, v)) => v.push(i),
            None => dlls.push((imp.dll.clone(), vec![i])),
        }
    }

    let desc_size = 20 * (dlls.len() as u32 + 1);
    let thunk_bytes: u32 = dlls.iter().map(|(_, v)| 8 * (v.len() as u32 + 1)).sum();
    let ilt_start = desc_size;
    let iat_start = ilt_start + thunk_bytes;
    let names_start = iat_start + thunk_bytes;

    // Noms : d'abord les fonctions (hint + nom + nul, aligné pair), puis les DLL.
    let mut names = Vec::new();
    let mut name_rva: Vec<u32> = vec![0; imports.len()];
    for (i, imp) in imports.iter().enumerate() {
        name_rva[i] = base + names_start + names.len() as u32;
        names.extend_from_slice(&0u16.to_le_bytes()); // hint : 0, le chargeur cherchera par nom
        names.extend_from_slice(imp.func.as_bytes());
        names.push(0);
        if names.len() % 2 != 0 {
            names.push(0);
        }
    }
    let mut dll_rva: Vec<u32> = Vec::new();
    for (dll, _) in &dlls {
        dll_rva.push(base + names_start + names.len() as u32);
        names.extend_from_slice(dll.as_bytes());
        names.push(0);
        if names.len() % 2 != 0 {
            names.push(0);
        }
    }

    let total = names_start + names.len() as u32;
    let mut bytes = vec![0u8; total as usize];
    let mut iat_offset = vec![0u32; imports.len()];

    // Descripteurs, ILT et IAT, DLL par DLL.
    let mut cursor = 0u32; // avance en parallèle dans l'ILT et l'IAT
    for (d, (_, funcs)) in dlls.iter().enumerate() {
        let ilt = ilt_start + cursor;
        let iat = iat_start + cursor;
        let desc = d * 20;
        put32(&mut bytes, desc, base + ilt); // OriginalFirstThunk
        put32(&mut bytes, desc + 4, 0); // TimeDateStamp
        put32(&mut bytes, desc + 8, 0); // ForwarderChain
        put32(&mut bytes, desc + 12, dll_rva[d]); // Name
        put32(&mut bytes, desc + 16, base + iat); // FirstThunk
        for (k, i) in funcs.iter().enumerate() {
            let off = 8 * k as u32;
            put64(&mut bytes, (ilt + off) as usize, name_rva[*i] as u64);
            put64(&mut bytes, (iat + off) as usize, name_rva[*i] as u64);
            iat_offset[*i] = iat + off;
        }
        // Les deux listes se terminent par une entrée nulle (déjà à zéro).
        cursor += 8 * (funcs.len() as u32 + 1);
    }
    // Le descripteur final, entièrement nul, est déjà en place.
    bytes[names_start as usize..].copy_from_slice(&names);

    Idata {
        bytes,
        iat_offset,
        iat_start,
        iat_size: thunk_bytes,
    }
}

fn put32(buf: &mut [u8], at: usize, v: u32) {
    buf[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn put64(buf: &mut [u8], at: usize, v: u64) {
    buf[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

fn align(v: u32, to: u32) -> u32 {
    v.div_ceil(to) * to
}

/// À quelle section de sortie appartient une section de l'objet.
///
/// Le nom prime sur la nature devinée par `object` : `nasm` écrit `.rdata` avec
/// des caractéristiques que le lecteur générique classe parfois en données
/// ordinaires, et une constante rangée en `.data` ne serait pas fausse, juste
/// inscriptible pour rien. Les sections de service (`.drectve`, `.debug*`) sont
/// écartées : elles n'ont pas d'existence dans l'image.
fn classify(sec: &object::Section<'_, '_>, name: &str) -> Option<Out> {
    match name {
        ".text" | ".code" => return Some(Out::Text),
        ".rdata" | ".rodata" => return Some(Out::RData),
        ".data" => return Some(Out::Data),
        ".bss" => return Some(Out::Bss),
        n if n.starts_with(".debug")
            || n == ".drectve"
            || n.starts_with(".pdata")
            || n.starts_with(".xdata") =>
        {
            return None;
        }
        _ => {}
    }
    match sec.kind() {
        object::SectionKind::Text => Some(Out::Text),
        object::SectionKind::ReadOnlyData | object::SectionKind::ReadOnlyString => Some(Out::RData),
        object::SectionKind::Data => Some(Out::Data),
        object::SectionKind::UninitializedData => Some(Out::Bss),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object::read::pe::{ImageThunkData, PeFile64};

    /// Assemble un source win64 et le lie, en rendant le chemin de l'exécutable.
    fn build(name: &str, source: &str) -> (std::path::PathBuf, LinkReport) {
        let dir = std::path::Path::new("build").join(format!("pe-{name}"));
        std::fs::create_dir_all(&dir).expect("dossier de sortie");
        let asm = dir.join(format!("{name}.asm"));
        std::fs::write(&asm, source).expect("écriture du source");
        let obj = dir.join(format!("{name}.obj"));
        let out = std::process::Command::new("nasm")
            .args(["-f", "win64"])
            .arg(&asm)
            .arg("-o")
            .arg(&obj)
            .output()
            .expect("nasm doit être installé");
        assert!(
            out.status.success(),
            "nasm: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let exe = dir.join(format!("{name}.exe"));
        let report = link(&obj, &exe, Subsystem::Console, Lang::Fr).expect("le lien doit réussir");
        (exe, report)
    }

    const HELLO: &str = r#"
    bits 64
    default rel

    section .data
        msg     db "Bonjour Windows", 13, 10
        msglen  equ $ - msg
    section .bss
        written resq 1
    section .text
        global main
        extern GetStdHandle
        extern WriteFile
        extern ExitProcess
    main:
        sub     rsp, 40
        mov     ecx, -11
        call    GetStdHandle
        mov     rcx, rax
        lea     rdx, [msg]
        mov     r8d, msglen
        lea     r9, [written]
        mov     qword [rsp + 32], 0
        call    WriteFile
        xor     ecx, ecx
        call    ExitProcess
    "#;

    /// L'exécutable produit se relit comme un PE64 : bon en-tête, bon
    /// sous-système, point d'entrée dans `.text`.
    #[test]
    fn produces_a_readable_pe64() {
        let (exe, report) = build("hello", HELLO);
        let data = std::fs::read(&exe).expect("lecture de l'exe");
        let pe = PeFile64::parse(&*data).expect("le fichier doit être un PE64 valide");

        let oh = pe.nt_headers().optional_header;
        assert_eq!(
            oh.subsystem.get(object::LittleEndian),
            pe::IMAGE_SUBSYSTEM_WINDOWS_CUI,
            "application console"
        );
        assert_eq!(oh.image_base.get(object::LittleEndian), IMAGE_BASE);
        assert_eq!(report.entry.0, "main", "le point d'entrée est main");

        // Le point d'entrée tombe bien dans .text, et pas ailleurs.
        let entry = oh.address_of_entry_point.get(object::LittleEndian);
        let text = pe
            .section_table()
            .iter()
            .find(|s| s.name.starts_with(b".text"))
            .expect("section .text");
        let start = text.virtual_address.get(object::LittleEndian);
        let end = start + text.virtual_size.get(object::LittleEndian);
        assert!(
            (start..end).contains(&entry),
            "entrée 0x{entry:X} hors de .text"
        );
    }

    /// Les trois fonctions `extern` deviennent des imports de kernel32, lisibles
    /// par un outil tiers — c'est ce que Windows lira au chargement.
    #[test]
    fn imports_are_declared_for_the_loader() {
        let (exe, report) = build("imports", HELLO);
        assert_eq!(report.imports.len(), 3, "trois fonctions importées");
        assert!(report.imports.iter().all(|i| i.dll == "kernel32.dll"));

        let data = std::fs::read(&exe).expect("lecture de l'exe");
        let pe = PeFile64::parse(&*data).expect("PE64 valide");
        let mut found: Vec<String> = Vec::new();
        let table = pe
            .import_table()
            .expect("table d'import lisible")
            .expect("table présente");
        let mut descs = table.descriptors().expect("descripteurs");
        while let Some(desc) = descs.next().expect("descripteur suivant") {
            let dll = table
                .name(desc.name.get(object::LittleEndian))
                .expect("nom de DLL");
            assert_eq!(dll, b"kernel32.dll");
            let mut thunks = table
                .thunks(desc.original_first_thunk.get(object::LittleEndian))
                .expect("ILT");
            while let Some(thunk) = thunks
                .next::<object::pe::ImageNtHeaders64>()
                .expect("thunk")
            {
                let (_, name) = table.hint_name(thunk.address()).expect("nom importé");
                found.push(String::from_utf8_lossy(name).into_owned());
            }
        }
        found.sort();
        assert_eq!(found, vec!["ExitProcess", "GetStdHandle", "WriteFile"]);
    }

    /// Un `call` vers une fonction importée doit atterrir sur son thunk, et le
    /// thunk sauter vers l'entrée d'IAT. Si ce chaînage est faux, l'exécutable
    /// se charge quand même et part dans le décor au premier appel — d'où la
    /// vérification instruction par instruction plutôt qu'à l'œil.
    #[test]
    fn call_reaches_the_iat_through_its_thunk() {
        let (exe, _) = build("thunk", HELLO);
        let data = std::fs::read(&exe).expect("lecture de l'exe");
        let pe = PeFile64::parse(&*data).expect("PE64 valide");
        let text = pe
            .section_table()
            .iter()
            .find(|s| s.name.starts_with(b".text"))
            .expect(".text");
        let text_rva = text.virtual_address.get(object::LittleEndian);
        let bytes = text.pe_data(&*data).expect("contenu de .text");

        // Premier `call rel32` rencontré (E8) : celui de GetStdHandle.
        let pos = bytes
            .iter()
            .position(|b| *b == 0xE8)
            .expect("un call dans .text");
        let disp = i32::from_le_bytes(bytes[pos + 1..pos + 5].try_into().unwrap());
        let target = (text_rva as i64 + pos as i64 + 5 + disp as i64) as u32;

        // La cible est un thunk : FF 25 disp32.
        let t = (target - text_rva) as usize;
        assert_eq!(
            &bytes[t..t + 2],
            &[0xFF, 0x25],
            "le call doit viser un thunk jmp [rip+…]"
        );
        let jdisp = i32::from_le_bytes(bytes[t + 2..t + 6].try_into().unwrap());
        let slot = (target as i64 + 6 + jdisp as i64) as u32;

        // Et ce slot est bien une entrée de l'IAT déclarée dans les répertoires.
        let dir = pe
            .data_directories()
            .get(pe::IMAGE_DIRECTORY_ENTRY_IAT)
            .expect("répertoire IAT");
        let iat_start = dir.virtual_address.get(object::LittleEndian);
        let iat_end = iat_start + dir.size.get(object::LittleEndian);
        assert!(
            (iat_start..iat_end).contains(&slot),
            "le thunk saute en 0x{slot:X}, hors de l'IAT [0x{iat_start:X}, 0x{iat_end:X})"
        );
    }

    /// Une constante lue par adresse relative doit pointer sur la bonne valeur :
    /// c'est le test de la relocation REL32 appliquée à `.rdata`/`.data`.
    #[test]
    fn rip_relative_data_access_lands_on_the_value() {
        let (exe, _) = build(
            "reldata",
            r#"
            bits 64
            default rel
            section .data
                magique dd 0xCAFEBABE
            section .text
                global main
                extern ExitProcess
            main:
                sub  rsp, 40
                mov  ecx, [magique]
                call ExitProcess
            "#,
        );
        let data = std::fs::read(&exe).expect("lecture de l'exe");
        let pe = PeFile64::parse(&*data).expect("PE64 valide");
        let sections = pe.section_table();
        let text = sections
            .iter()
            .find(|s| s.name.starts_with(b".text"))
            .expect(".text");
        let text_rva = text.virtual_address.get(object::LittleEndian);
        let code = text.pe_data(&*data).expect("contenu de .text");

        // `mov ecx, [rip+disp32]` = 8B 0D disp32.
        let pos = code
            .windows(2)
            .position(|w| w == [0x8B, 0x0D])
            .expect("le mov depuis la mémoire doit être présent");
        let disp = i32::from_le_bytes(code[pos + 2..pos + 6].try_into().unwrap());
        let target = (text_rva as i64 + pos as i64 + 6 + disp as i64) as u32;

        let dsec = sections
            .iter()
            .find(|s| s.name.starts_with(b".data"))
            .expect(".data");
        let drva = dsec.virtual_address.get(object::LittleEndian);
        let dbytes = dsec.pe_data(&*data).expect("contenu de .data");
        let off = (target - drva) as usize;
        assert_eq!(
            u32::from_le_bytes(dbytes[off..off + 4].try_into().unwrap()),
            0xCAFE_BABE,
            "l'adresse relative doit désigner la constante"
        );
    }

    /// Le catalogue oriente les `extern` vers la bonne DLL, et la syntaxe
    /// « dll$fonction » ouvre la porte à tout le reste de Windows.
    #[test]
    fn symbols_are_routed_to_their_library() {
        assert_eq!(dll_for("ExitProcess"), Some("kernel32.dll"));
        assert_eq!(dll_for("MessageBoxA"), Some("user32.dll"));
        assert_eq!(dll_for("printf"), Some("msvcrt.dll"));
        assert_eq!(dll_for("CreatePen"), None);

        let (dll, func, direct) = resolve_import("gdi32$CreatePen", Lang::Fr).expect("DLL explicite");
        assert_eq!(
            (dll.as_str(), func.as_str(), direct),
            ("gdi32.dll", "CreatePen", false)
        );

        let (dll, func, direct) = resolve_import("__imp_ExitProcess", Lang::Fr).expect("entrée d'IAT");
        assert_eq!(
            (dll.as_str(), func.as_str(), direct),
            ("kernel32.dll", "ExitProcess", true)
        );

        // Et un symbole inconnu explique quoi faire, au lieu d'un code d'erreur.
        let err = resolve_import("MaFonctionAMoi", Lang::Fr).expect_err("doit échouer");
        assert!(
            err.contains("extern user32$"),
            "le message doit montrer la sortie de secours: {err}"
        );
    }

    /// Sans point d'entrée, le lieur refuse et dit lequel il cherchait.
    #[test]
    fn missing_entry_point_is_explained() {
        let dir = std::path::Path::new("build/pe-noentry");
        std::fs::create_dir_all(dir).expect("dossier");
        let asm = dir.join("noentry.asm");
        std::fs::write(&asm, "bits 64\nsection .text\nautre:\n    ret\n").expect("source");
        let obj = dir.join("noentry.obj");
        let out = std::process::Command::new("nasm")
            .args(["-f", "win64"])
            .arg(&asm)
            .arg("-o")
            .arg(&obj)
            .output()
            .expect("nasm");
        assert!(out.status.success());
        let err =
            link(&obj, &dir.join("noentry.exe"), Subsystem::Console, Lang::Fr).expect_err("doit échouer");
        assert!(err.contains("global main"), "message inattendu: {err}");
    }

    /// Le lien est reproductible : deux exécutions donnent le même fichier.
    /// Sans cela, comparer deux exécutables (ou les mettre en cache) serait
    /// impossible, et l'ordre d'itération des symboles finirait par se voir.
    #[test]
    fn linking_is_deterministic() {
        let (a, _) = build("determ-a", HELLO);
        let (b, _) = build("determ-b", HELLO);
        assert_eq!(
            std::fs::read(&a).expect("a"),
            std::fs::read(&b).expect("b"),
            "deux liens du même source doivent donner le même octet"
        );
    }

    /// Si wine est installé, on va jusqu'au bout : le programme tourne pour de
    /// vrai. C'est la seule vérification qui couvre la chaîne entière — table
    /// d'import résolue par un vrai chargeur, thunks empruntés, convention
    /// d'appel Microsoft respectée. Ignoré si wine manque : son absence ne doit
    /// pas faire échouer la suite de tests.
    ///
    /// Deux choses sont contrôlées, parce qu'elles peuvent échouer séparément :
    /// ce que le programme écrit (donc `GetStdHandle` + `WriteFile`, quatre
    /// arguments dont un sur la pile, derrière l'espace d'ombre), et le code
    /// qu'il rend (donc `ExitProcess`, dont l'argument passe par ECX — un
    /// lieur qui se tromperait de thunk sortirait avec un code au hasard).
    #[test]
    fn runs_under_wine_when_available() {
        let wine = |exe: &std::path::Path| {
            std::process::Command::new("wine")
                .arg(exe)
                // Sans cela, wine écrit ses propres avertissements sur stderr et
                // les mêle à ceux du programme.
                .env("WINEDEBUG", "-all")
                .output()
        };
        if std::process::Command::new("wine")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("wine absent : exécution du PE non vérifiée");
            return;
        }

        let (exe, _) = build("wine-hello", HELLO);
        let out = wine(&exe).expect("lancement de wine");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("Bonjour Windows"),
            "sortie inattendue: {stdout:?} / {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            out.status.code(),
            Some(0),
            "le programme doit sortir proprement"
        );

        // Le code de sortie vient d'ECX : c'est la convention Microsoft, et
        // c'est aussi ce qui distingue un appel qui a vraiment atteint
        // ExitProcess d'un saut parti ailleurs.
        let (exe, _) = build(
            "wine-exit",
            r#"
            bits 64
            default rel
            section .text
                global main
                extern ExitProcess
            main:
                sub  rsp, 40
                mov  ecx, 42
                call ExitProcess
            "#,
        );
        let out = wine(&exe).expect("lancement de wine");
        assert_eq!(
            out.status.code(),
            Some(42),
            "ExitProcess doit recevoir 42 par ECX (stderr: {})",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
