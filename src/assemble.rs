//! Assemblage d'un fichier `.asm` NASM vers un exécutable ELF64 ou PE64.
//!
//! Deux cibles cohabitent, et elles ne vont pas aussi loin l'une que l'autre :
//!
//! * [`Target::Linux`] — `nasm -f elf64` puis `ld`. C'est la cible complète :
//!   le binaire produit se désassemble, s'exécute et se débogue pas à pas.
//! * [`Target::Windows`] et [`Target::WindowsGui`] — `nasm -f win64` puis le lieur intégré
//!   ([`crate::pe_link`]). Le `.exe` est un vrai PE, lisible par Windows et par
//!   les outils d'analyse ; avec Wine installé, l'IDE le lance et récupère sa
//!   sortie ([`crate::winerun`]). Ce qui manque, c'est le pas-à-pas : le
//!   débogueur parle `ptrace` et suit les adresses de l'image assemblée, que le
//!   chargeur de Wine ne conserve pas.
//!
//! Le listing `.lst` est demandé dans les deux cas : c'est lui qui porte le
//! mapping adresse ↔ ligne source.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::i18n::{self, Lang};
use crate::project::Project;

/// Système visé par l'assemblage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Target {
    /// ELF64 Linux, exécuté et débogué par l'IDE.
    #[default]
    Linux,
    /// PE64 Windows en application console : Windows ouvre un terminal, où
    /// `WriteFile` sur la sortie standard écrit. Sous Linux, l'IDE le lance par
    /// Wine s'il est installé, et sa sortie arrive dans la console habituelle.
    Windows,
    /// PE64 Windows en application graphique : aucune console ne s'ouvre, pour
    /// un programme qui ne parle que par `MessageBox`.
    WindowsGui,
}

impl Target {
    /// Clé stable pour la persistance des réglages.
    pub fn key(self) -> &'static str {
        match self {
            Target::Linux => "linux",
            Target::Windows => "windows",
            Target::WindowsGui => "windows-gui",
        }
    }

    pub fn from_key(s: &str) -> Target {
        match s {
            "windows" => Target::Windows,
            "windows-gui" => Target::WindowsGui,
            _ => Target::Linux,
        }
    }

    /// Le binaire produit peut-il être lancé par le débogueur ?
    pub fn is_runnable(self) -> bool {
        self == Target::Linux
    }

    /// Cible Windows, console ou graphique.
    pub fn is_windows(self) -> bool {
        matches!(self, Target::Windows | Target::WindowsGui)
    }

    /// Format de sortie passé à `nasm -f`.
    fn nasm_format(self) -> &'static str {
        match self {
            Target::Linux => "elf64",
            Target::Windows | Target::WindowsGui => "win64",
        }
    }
}

/// Pour quelle cible ce source est-il écrit ? `None` quand rien ne tranche.
///
/// Un fichier ouvert n'annonce pas son format, et le lire avec la mauvaise
/// cible ne produit pas un avertissement mais une erreur de nasm qui ne parle
/// pas du bon sujet : `extern ExitProcess` refusé en `elf64`, ou `_start`
/// introuvable en `win64`. Les deux mondes se distinguent pourtant à des signes
/// francs, qu'aucun programme ne porte par hasard.
///
/// La règle est délibérément prudente : ce qui décide doit être hors de portée
/// de l'autre monde. `syscall` n'existe pas dans un programme Windows, et
/// `ExitProcess` n'existe pas dans un programme Linux ; en revanche `main` ou
/// `default rel` se rencontrent des deux côtés et ne prouvent rien. Un source
/// qui porterait les deux marques — un fichier à moitié converti — ne renvoie
/// rien plutôt que de trancher au hasard.
pub fn detect_target(source: &str) -> Option<Target> {
    // Les commentaires mentent : une explication peut nommer `syscall` dans un
    // fichier Windows, et le tutoriel ne s'en prive pas. Seul le code compte.
    let code = source
        .lines()
        .map(|l| l.split(';').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();

    let has_word = |w: &str| {
        code.match_indices(w).any(|(i, _)| {
            let before = code[..i].chars().next_back();
            let after = code[i + w.len()..].chars().next();
            let edge = |c: Option<char>| !c.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.');
            edge(before) && edge(after)
        })
    };

    // Un `extern` d'une fonction Windows, ou l'une des DLL nommées à la volée
    // (`extern gdi32$CreatePen`), ne laisse aucun doute.
    let windows = has_word("exitprocess")
        || has_word("getstdhandle")
        || has_word("writefile")
        || has_word("messageboxa")
        || has_word("messageboxw")
        || code.contains("$")
        || code.contains("-f win64");
    // `syscall` est l'instruction que Windows n'a pas, et `_start` le point
    // d'entrée que son lieur ne cherche pas.
    let linux = has_word("syscall") || has_word("_start");

    match (windows, linux) {
        (true, false) => Some(Target::Windows),
        (false, true) => Some(Target::Linux),
        _ => None,
    }
}

/// Ce que l'on demande au lieur, en plus de la cible.
///
/// Séparé de [`Target`] à dessein : le format produit ne change pas — c'est un
/// ELF64 dans les deux cas — seule change la façon dont le noyau le chargera.
/// Le défaut est le lien historique, sans aucune option : un binaire assemblé
/// sans rien demander est octet pour octet celui d'avant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LinkOptions {
    /// Lier en exécutable position-indépendant (`ld -pie`), chargeable à
    /// n'importe quelle adresse. Le code doit alors être écrit pour : `default
    /// rel` et `lea reg, [rel étiquette]` plutôt qu'une adresse absolue, sans
    /// quoi `ld` refuse le relogement plutôt que de produire un binaire faux.
    pub pie: bool,
}

impl LinkOptions {
    /// Arguments supplémentaires passés à `ld`.
    ///
    /// `--no-dynamic-linker` ne quitte jamais `-pie` : sans lui, `ld` place
    /// dans le binaire un `PT_INTERP` réclamant `/lib/ld64.so.1`, que personne
    /// n'installe. Le programme ne démarre alors pas du tout, et le noyau
    /// répond « fichier introuvable » — en parlant de l'interpréteur, pas du
    /// programme, ce dont l'élève n'a aucun moyen de se douter.
    ///
    /// `-z text` ne quitte jamais `-pie` non plus, et c'est le plus important
    /// des deux : sans lui, une adresse absolue (`mov reg, étiquette`) n'est
    /// pas forcément refusée — certaines versions de `ld` (vérifié : 2.42
    /// accepte, 2.46 refuse, pour la même adresse) l'acceptent avec un simple
    /// avertissement (« creating DT_TEXTREL in a PIE ») et posent un
    /// relogement dynamique dans `.rela.dyn`. Rien ne l'applique jamais : sans
    /// éditeur dynamique (`--no-dynamic-linker`), aucun `ld.so` ne le fait, et
    /// notre `_start` écrit à la main n'a pas le bout de code que fournit la
    /// glibc pour un « static PIE ». Le binaire démarre, le registre chargé
    /// par l'adresse absolue reste celui du lien plutôt que celui du
    /// chargement — l'écriture qui en dépend échoue en silence, sans le
    /// moindre message. `-z text` transforme ce piège en un vrai refus au
    /// lien, sur toutes les versions : c'est justement ce que le chapitre
    /// enseigne à reconnaître, pas une exécution qui a l'air de réussir.
    fn ld_args(self) -> &'static [&'static str] {
        if self.pie {
            &["-pie", "--no-dynamic-linker", "-z", "text"]
        } else {
            &[]
        }
    }

    /// Les mêmes arguments tels qu'ils apparaissent dans le journal, préfixe
    /// d'espace compris (vide quand il n'y en a aucun).
    fn logged(self) -> String {
        self.ld_args().iter().map(|a| format!(" {a}")).collect()
    }
}

pub struct BuildOutput {
    /// Chemin du binaire produit (ELF prêt pour ptrace, ou `.exe` PE64).
    pub binary: PathBuf,
    /// Chemin du listing NASM (`.lst`), pour le mapping adresse ↔ ligne source.
    pub listing: PathBuf,
    /// Journal des commandes exécutées (affiché dans la console).
    pub log: String,
}

/// Assemble (nasm) puis lie (ld) `src` pour Linux, en ajoutant des répertoires
/// de recherche `%include` (`nasm -i`) : par ex. le dossier du fichier et celui
/// d'`asmstd.inc`. Passer `&[]` pour n'ajouter aucun chemin d'include.
///
/// Réservé aux tests depuis que la cible fait partie de l'appel : le reste du
/// programme passe par [`assemble_for`], qui, lui, demande laquelle.
#[cfg(test)]
pub fn assemble_with_includes(
    src: &Path,
    out_dir: &Path,
    includes: &[PathBuf],
) -> Result<BuildOutput, String> {
    assemble_for(src, out_dir, includes, Target::Linux, Lang::Fr)
}

/// Assemble `src` pour la cible demandée.
///
/// `lang` ne sert qu'aux messages : le journal d'assemblage et les erreurs
/// partent droit dans la console de l'élève, et resteraient sinon en français
/// dans une interface qu'il a mise en anglais ou en espagnol. Ce que nasm et
/// ld écrivent eux-mêmes garde évidemment leur langue à eux.
pub fn assemble_for(
    src: &Path,
    out_dir: &Path,
    includes: &[PathBuf],
    target: Target,
    lang: Lang,
) -> Result<BuildOutput, String> {
    assemble_for_with(src, out_dir, includes, target, LinkOptions::default(), lang)
}

/// Comme [`assemble_for`], mais en disant aussi ce qu'on attend du lieur.
///
/// Deux portes plutôt qu'un paramètre de plus partout : les options de lien ne
/// concernent qu'un seul appelant — l'IDE, qui les persiste dans ses réglages.
/// Tout le reste (tests, contrôle d'alignement, panneau FORMAT, exécution sous
/// Wine) lie comme il l'a toujours fait, et continue de le dire en une ligne.
pub fn assemble_for_with(
    src: &Path,
    out_dir: &Path,
    includes: &[PathBuf],
    target: Target,
    opts: LinkOptions,
    lang: Lang,
) -> Result<BuildOutput, String> {
    match target {
        Target::Linux => assemble_elf(src, out_dir, includes, opts, lang),
        Target::Windows | Target::WindowsGui => assemble_pe(src, out_dir, includes, target, lang),
    }
}

/// Assemble toutes les sources d'un projet puis les lie ensemble.
///
/// Un projet Linux produit un objet par fichier, ce qui permet de séparer les
/// routines (`math.asm`, `io.asm`…) du point d'entrée. Le lieur PE intégré ne
/// sait pour l'instant lire qu'un seul objet COFF : refuser franchement deux
/// sources vaut mieux que de jeter silencieusement les suivantes.
/// Un projet ne s'assemble que depuis l'IDE : les options de lien sont ici un
/// paramètre ordinaire, là où [`assemble_for`] garde une porte sans options
/// pour ses nombreux autres appelants.
pub fn assemble_project(
    project: &Project,
    out_dir: &Path,
    target: Target,
    opts: LinkOptions,
    lang: Lang,
) -> Result<BuildOutput, String> {
    if target.is_windows() {
        if project.sources.len() != 1 {
            return Err(i18n::tr3(
                lang,
                "Le lieur PE64 intégré ne lie pas encore plusieurs fichiers. Gardez une seule source dans sources, ou construisez ce projet pour Linux.",
                "The built-in PE64 linker cannot link several files yet. Keep one source in sources, or build this project for Linux.",
                "El enlazador PE64 integrado aún no enlaza varios archivos. Mantenga una sola fuente en sources o compile este proyecto para Linux.",
            )
            .to_string());
        }
        return assemble_for(&project.entry, out_dir, &project.include_dirs(), target, lang);
    }

    std::fs::create_dir_all(out_dir).map_err(|e| {
        let what = i18n::tr3(lang, "création de", "creating", "creación de");
        format!("{what} {}: {e}", out_dir.display())
    })?;
    let objects_dir = out_dir.join("objects");
    std::fs::create_dir_all(&objects_dir).map_err(|e| {
        let what = i18n::tr3(lang, "création de", "creating", "creación de");
        format!("{what} {}: {e}", objects_dir.display())
    })?;

    let mut log = String::new();
    let mut objects = Vec::new();
    let mut entry_listing = None;
    for (index, source) in project.sources.iter().enumerate() {
        let stem = source.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
            i18n::tr3(lang, "nom de source invalide", "invalid source name", "nombre de fuente no válido").to_string()
        })?;
        // L'index évite une collision entre `src/util.asm` et `tests/util.asm`.
        let object = objects_dir.join(format!("{index:03}-{stem}.o"));
        let listing = objects_dir.join(format!("{index:03}-{stem}.lst"));
        log.push_str(&nasm_to(source, &object, &listing, &project.include_dirs(), target, lang)?);
        if source == &project.entry {
            entry_listing = Some(listing);
        }
        objects.push(object);
    }
    let listing = entry_listing.ok_or_else(|| {
        i18n::tr3(lang, "l'entrée du projet n'est pas dans sources", "the project entry is not in sources", "la entrada del proyecto no está en sources").to_string()
    })?;
    let name = project
        .root
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("programme");
    let binary = out_dir.join(name);
    log.push_str(&format!("$ ld{} -o {} {}\n", opts.logged(), binary.display(), objects.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(" ")));
    let named_objects: Vec<(String, &Path)> = objects
        .iter()
        .map(|p| (p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), p.as_path()))
        .collect();
    link_elf(&named_objects, &binary, opts, &mut log, lang)?;
    log.push_str("Build OK\n");
    append_stack_alignment_warning(&mut log, &binary, &listing, target, lang);
    Ok(BuildOutput { binary, listing, log })
}

/// Ajoute au journal l'avertissement d'alignement de pile, s'il y a lieu.
///
/// Le contrôle est purement consultatif — voir [`crate::stack_check`]. Il tourne
/// une fois le binaire écrit, ne peut qu'ajouter du texte, et ne remet jamais en
/// cause un assemblage réussi : c'est le dernier moment où l'IDE peut prévenir
/// l'élève avant qu'il ne lance un programme dont le plantage ne lui
/// apprendrait rien.
fn append_stack_alignment_warning(
    log: &mut String,
    binary: &Path,
    listing: &Path,
    target: Target,
    lang: Lang,
) {
    if let Some(warning) = crate::stack_check::warn_for_binary(binary, listing, target, lang) {
        log.push_str(&warning);
    }
}

/// Assemble et lie un objet COFF en exécutable PE64, sans outil externe autre
/// que `nasm` : le lien est fait par [`crate::pe_link`].
fn assemble_pe(
    src: &Path,
    out_dir: &Path,
    includes: &[PathBuf],
    target: Target,
    lang: Lang,
) -> Result<BuildOutput, String> {
    let (stem, listing, mut log) = nasm(src, out_dir, includes, target, lang)?;
    let obj = out_dir.join(format!("{stem}.obj"));
    let binary = out_dir.join(format!("{stem}.exe"));

    let subsystem = if target == Target::WindowsGui {
        crate::pe_link::Subsystem::Gui
    } else {
        crate::pe_link::Subsystem::Console
    };
    log.push_str(&format!("$ (lieur PE intégré) -o {}\n", binary.display()));
    let report = crate::pe_link::link(&obj, &binary, subsystem, lang)
        .map_err(|e| {
            let head = i18n::tr3(lang, "Échec du lien PE", "PE link failed", "Error de enlazado PE");
            format!("{head}:\n{log}{e}\n")
        })?;
    log.push_str(&format!(
        "  {} : {} (RVA 0x{:X})\n",
        i18n::tr3(lang, "point d'entrée", "entry point", "punto de entrada"),
        report.entry.0,
        report.entry.1
    ));
    for imp in &report.imports {
        log.push_str(&format!(
            "  {} : {} ← {}\n",
            i18n::tr3(lang, "import", "import", "importación"),
            imp.func,
            imp.dll
        ));
    }
    log.push_str(&format!(
        "Build OK — {} {} (PE64 Windows)\n",
        report.size,
        i18n::tr3(lang, "octets", "bytes", "bytes")
    ));
    append_stack_alignment_warning(&mut log, &binary, &listing, target, lang);
    Ok(BuildOutput { binary, listing, log })
}

/// Lance `nasm` pour la cible donnée. Rend le radical du nom de fichier, le
/// chemin du listing et le journal, l'objet étant nommé d'après la cible
/// (`.o` pour ELF, `.obj` pour COFF, comme le veut chaque monde).
fn nasm(
    src: &Path,
    out_dir: &Path,
    includes: &[PathBuf],
    target: Target,
    lang: Lang,
) -> Result<(String, PathBuf, String), String> {
    std::fs::create_dir_all(out_dir).map_err(|e| {
        let what = i18n::tr3(lang, "création de", "creating", "creación de");
        format!("{what} {}: {e}", out_dir.display())
    })?;
    let stem = src
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| {
            i18n::tr3(
                lang,
                "nom de fichier source invalide",
                "invalid source file name",
                "nombre de archivo fuente no válido",
            )
            .to_string()
        })?
        .to_string();
    let obj = out_dir.join(format!(
        "{stem}.{}",
        if target.is_windows() { "obj" } else { "o" }
    ));
    let listing = out_dir.join(format!("{stem}.lst"));

    let log = nasm_to(src, &obj, &listing, includes, target, lang)?;
    Ok((stem, listing, log))
}

/// Lance NASM vers des chemins d'objet et de listing explicites. La forme est
/// utilisée par un fichier seul comme par chacune des sources d'un projet.
fn nasm_to(
    src: &Path,
    obj: &Path,
    listing: &Path,
    includes: &[PathBuf],
    target: Target,
    lang: Lang,
) -> Result<String, String> {
    if let Some(parent) = obj.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("création de {}: {e}", parent.display()))?;
    }
    // nasm attend le chemin collé à l'option et terminé par un séparateur.
    let inc_args: Vec<String> = includes
        .iter()
        .map(|d| format!("-i{}/", d.display()))
        .collect();
    let mut log = format!(
        "$ nasm -f {} {}{} -o {} -l {}\n",
        target.nasm_format(),
        inc_args.iter().map(|a| format!("{a} ")).collect::<String>(),
        src.display(),
        obj.display(),
        listing.display()
    );
    let out = Command::new("nasm")
        .args(["-f", target.nasm_format()])
        .args(&inc_args)
        .arg(src)
        .arg("-o")
        .arg(obj)
        .arg("-l")
        .arg(listing)
        .output()
        .map_err(|e| {
            let what = i18n::tr3(lang, "impossible de lancer", "could not run", "no se pudo ejecutar");
            format!("{what} nasm: {e}")
        })?;
    log.push_str(&String::from_utf8_lossy(&out.stderr));
    if !out.status.success() {
        return Err(format!(
            "{}:\n{log}",
            i18n::tr3(lang, "Échec de nasm", "nasm failed", "Error de nasm")
        ));
    }
    Ok(log)
}

fn assemble_elf(
    src: &Path,
    out_dir: &Path,
    includes: &[PathBuf],
    opts: LinkOptions,
    lang: Lang,
) -> Result<BuildOutput, String> {
    let (stem, listing, mut log) = nasm(src, out_dir, includes, Target::Linux, lang)?;
    let obj = out_dir.join(format!("{stem}.o"));
    let binary = out_dir.join(&stem);

    log.push_str(&format!("$ ld{} -o {} {}\n", opts.logged(), binary.display(), obj.display()));
    link_elf(&[(format!("{stem}.o"), &obj)], &binary, opts, &mut log, lang)?;

    log.push_str("Build OK\n");
    append_stack_alignment_warning(&mut log, &binary, &listing, Target::Linux, lang);
    Ok(BuildOutput { binary, listing, log })
}

/// Lie des objets ELF64 en un exécutable : `ld` local sous Linux, `ld`
/// *dans la VM* sur macOS (aucun linker ELF natif là-bas — voir le plan de
/// portage). `objects` porte le nom à donner à chaque objet côté agent (pour
/// le journal, sans conséquence sur le résultat) et son chemin local.
fn link_elf(
    objects: &[(String, &Path)],
    binary: &Path,
    opts: LinkOptions,
    log: &mut String,
    lang: Lang,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let ld = Command::new("ld")
            .args(opts.ld_args())
            .arg("-o")
            .arg(binary)
            .args(objects.iter().map(|(_, path)| path))
            .output()
            .map_err(|e| {
                let what = i18n::tr3(lang, "impossible de lancer", "could not run", "no se pudo ejecutar");
                format!("{what} ld: {e}")
            })?;
        log.push_str(&String::from_utf8_lossy(&ld.stderr));
        if !ld.status.success() {
            return Err(format!("{}:\n{log}", i18n::tr3(lang, "Échec de ld", "ld failed", "Error de ld")));
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let mut payload = Vec::with_capacity(objects.len());
        for (name, path) in objects {
            let bytes = std::fs::read(path).map_err(|e| format!("lecture de {}: {e}", path.display()))?;
            payload.push((name.clone(), bytes));
        }
        let ld_args: Vec<String> = opts.ld_args().iter().map(|s| s.to_string()).collect();
        match crate::vm_debugger::link_via_vm(payload, ld_args) {
            Ok(r) => {
                log.push_str(&r.log);
                std::fs::write(binary, &r.binary).map_err(|e| format!("écriture de {}: {e}", binary.display()))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(mut perms) = std::fs::metadata(binary).map(|m| m.permissions()) {
                        perms.set_mode(0o755);
                        let _ = std::fs::set_permissions(binary, perms);
                    }
                }
                Ok(())
            }
            Err(e) => Err(format!(
                "{}:\n{log}{}",
                i18n::tr3(lang, "Échec de ld (VM)", "ld failed (VM)", "Error de ld (VM)"),
                e.message(lang)
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un échec d'assemblage est la première chose qu'un débutant voit, et
    /// souvent plusieurs fois par séance. Il doit lui parler dans la langue
    /// qu'il a choisie — le message de nasm, lui, reste celui de nasm.
    #[test]
    fn build_errors_speak_the_interface_language() {
        let dir = Path::new("build/i18n-build-errors");
        std::fs::create_dir_all(dir).expect("dossier de test");
        let src = dir.join("casse.asm");
        // Un crochet jamais refermé : nasm ne peut rien en faire. (Un mot
        // inventé seul sur sa ligne ne suffirait pas — nasm y verrait un
        // label, qu'il accepte sans les deux-points.)
        std::fs::write(&src, "section .text\n    global _start\n_start:\n    mov rax, [\n")
            .expect("écriture du source fautif");

        let msg = |lang| match assemble_for(&src, dir, &[], Target::Linux, lang) {
            Err(e) => e,
            Ok(_) => panic!("ce source ne doit pas s'assembler"),
        };
        let (fr, en, es) = (msg(Lang::Fr), msg(Lang::En), msg(Lang::Es));
        assert!(fr.starts_with("Échec de nasm"), "{fr}");
        assert!(en.starts_with("nasm failed"), "{en}");
        assert!(es.starts_with("Error de nasm"), "{es}");
        // Ce que nasm a écrit doit survivre à la traduction : c'est lui qui dit
        // quelle ligne est fautive.
        for m in [&fr, &en, &es] {
            assert!(m.contains("casse.asm"), "{m}");
        }
    }

    /// La bibliothèque asmstd doit s'assembler via `%include` (dossier examples).
    #[test]
    fn asmstd_include_resolves() {
        let out = assemble_with_includes(
            Path::new("examples/hello_asmstd.asm"),
            Path::new("build/test-asmstd"),
            &[PathBuf::from("examples")],
        )
        .expect("hello_asmstd.asm doit s'assembler avec asmstd.inc");
        assert!(out.binary.exists(), "le binaire doit être produit");
    }

    /// L'exemple Windows livré doit s'assembler en `.exe` par les deux
    /// sous-systèmes, et le journal doit nommer ce que le lieur a importé —
    /// c'est ce que l'élève lit dans la console après son Ctrl+B.
    #[test]
    fn the_windows_example_builds_as_a_pe() {
        for (target, dir) in [
            (Target::Windows, "build/pe-example-cui"),
            (Target::WindowsGui, "build/pe-example-gui"),
        ] {
            let out = assemble_for(
                Path::new("examples/hello-windows.asm"),
                Path::new(dir),
                &[],
                target,
                Lang::Fr,
            )
            .expect("hello-windows.asm doit s'assembler");
            assert_eq!(out.binary.extension().and_then(|e| e.to_str()), Some("exe"));
            assert!(out.binary.exists(), "le .exe doit être écrit");
            assert!(out.log.contains("point d'entrée : main"), "journal: {}", out.log);
            assert!(out.log.contains("WriteFile ← kernel32.dll"), "journal: {}", out.log);
        }
    }

    /// Les exemples essentiels ont chacun leur version PE64. Ils ne sont pas
    /// de simples copies ELF : chaque fichier importe l'API Windows nécessaire
    /// et devient un vrai `.exe` console.
    #[test]
    fn essential_windows_examples_build_as_console_pe64() {
        for (source, import) in [
            ("win_hello_world.asm", "WriteFile ← kernel32.dll"),
            ("win_arithmetic.asm", "WriteFile ← kernel32.dll"),
            ("win_boucle.asm", "WriteFile ← kernel32.dll"),
            ("win_lire_ecrire.asm", "ReadFile ← kernel32.dll"),
        ] {
            let stem = source.trim_end_matches(".asm");
            let out = assemble_for(
                &PathBuf::from("examples_seed").join(source),
                &PathBuf::from("build/pe-essentials").join(stem),
                &[],
                Target::Windows,
                Lang::Fr,
            )
            .unwrap_or_else(|e| panic!("{source} doit s'assembler : {e}"));
            assert_eq!(out.binary.extension().and_then(|e| e.to_str()), Some("exe"));
            assert!(out.binary.is_file(), "{source} : .exe absent");
            assert!(out.log.contains(import), "{source} : import absent\n{}", out.log);
        }
    }

    /// Wine est le contrôle de bout en bout : il charge la table d'import de
    /// nos exemples comme le ferait Windows et vérifie aussi le chemin ReadFile
    /// → WriteFile. L'absence de Wine ne doit pas empêcher les tests portables.
    #[test]
    fn essential_windows_examples_run_under_wine_when_available() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        if Command::new("wine").arg("--version").output().is_err() {
            eprintln!("wine absent : exécution PE des essentiels non vérifiée");
            return;
        }
        let build = |source: &str| {
            assemble_for(
                &PathBuf::from("examples_seed").join(source),
                &PathBuf::from("build/pe-essentials-wine").join(source.trim_end_matches(".asm")),
                &[],
                Target::Windows,
                Lang::Fr,
            )
            .unwrap_or_else(|e| panic!("{source} : {e}"))
            .binary
        };
        let run = |exe: &Path, input: Option<&str>| {
            let mut child = Command::new("wine")
                .arg(exe)
                .env("WINEDEBUG", "-all")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Wine doit se lancer");
            if let Some(input) = input {
                child.stdin.take().expect("stdin Wine").write_all(input.as_bytes()).expect("saisie Wine");
            }
            child.wait_with_output().expect("Wine doit se terminer")
        };

        for (source, expected) in [
            ("win_hello_world.asm", "Bonjour depuis Windows PE64 !"),
            ("win_arithmetic.asm", "Resultat = 8"),
            ("win_boucle.asm", "1\r\n2\r\n"),
        ] {
            let out = run(&build(source), None);
            assert!(out.status.success(), "{source} : {}", String::from_utf8_lossy(&out.stderr));
            assert!(String::from_utf8_lossy(&out.stdout).contains(expected), "{source} : sortie {:?}", out.stdout);
        }

        let out = run(&build("win_lire_ecrire.asm"), Some("Z\n"));
        assert!(out.status.success(), "lire/ecrire : {}", String::from_utf8_lossy(&out.stderr));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("Tapez un caractere"), "invite manquante : {stdout:?}");
        assert!(stdout.ends_with('Z'), "caractère lu non réaffiché : {stdout:?}");
    }

    /// La cible se persiste et se relit : sans cela, l'élève retrouve la cible
    /// Linux à chaque démarrage sans savoir pourquoi son `.exe` a disparu.
    #[test]
    fn target_round_trips_through_its_key() {
        for t in [Target::Linux, Target::Windows, Target::WindowsGui] {
            assert_eq!(Target::from_key(t.key()), t, "{t:?} ne se relit pas");
        }
        // Une clé inconnue (réglage d'une version future, fichier abîmé) ne
        // doit pas empêcher de démarrer : Linux, la cible qui marche partout.
        assert_eq!(Target::from_key("plan9"), Target::Linux);
    }

    #[test]
    fn source_target_detection_uses_code_not_comments() {
        assert_eq!(
            detect_target("global _start\n_start:\n    syscall\n"),
            Some(Target::Linux)
        );
        assert_eq!(
            detect_target("global main\nextern ExitProcess\nmain:\n    call ExitProcess\n"),
            Some(Target::Windows)
        );
        assert_eq!(
            detect_target("; syscall and ExitProcess are only documentation\nglobal main\nmain:\n    ret\n"),
            None
        );
        assert_eq!(
            detect_target("global _start\nextern ExitProcess\n_start:\n    syscall\n"),
            None,
            "un source mélangé ne doit pas choisir une cible arbitrairement"
        );
    }
}

#[cfg(test)]
mod pie_tests {
    use super::*;
    use object::Object;

    /// Le type ELF réellement écrit dans l'en-tête, celui que `readelf -h`
    /// affiche. C'est la seule preuve qui vaille : le drapeau passé à `ld` ne
    /// dit que l'intention.
    fn elf_kind(binary: &Path) -> object::ObjectKind {
        let data = std::fs::read(binary).expect("lecture du binaire");
        object::File::parse(&*data).expect("ELF lisible").kind()
    }

    /// L'option change le type du binaire produit, et rien d'autre : le même
    /// source donne un `ET_EXEC` sans elle, un `ET_DYN` avec — et les deux
    /// s'exécutent et rendent le même résultat.
    #[test]
    fn the_pie_option_turns_the_executable_into_a_dyn_that_still_runs() {
        let src = Path::new("examples_seed/pie_rip_relatif.asm");

        let plain = assemble_for(src, Path::new("build/pie-off"), &[], Target::Linux, Lang::Fr)
            .expect("l'exemple PIE s'assemble aussi en lien ordinaire");
        assert_eq!(elf_kind(&plain.binary), object::ObjectKind::Executable);
        assert!(plain.log.contains("$ ld -o"), "journal : {}", plain.log);
        assert!(!plain.log.contains("-pie"), "journal : {}", plain.log);

        let pie = assemble_for_with(
            src,
            Path::new("build/pie-on"),
            &[],
            Target::Linux,
            LinkOptions { pie: true },
            Lang::Fr,
        )
        .expect("l'exemple PIE se lie en position-indépendant");
        assert_eq!(elf_kind(&pie.binary), object::ObjectKind::Dynamic);
        assert!(
            pie.log.contains("$ ld -pie --no-dynamic-linker -z text -o"),
            "journal : {}",
            pie.log
        );

        // Un ELF de type DYN qui ne démarre pas serait un progrès purement
        // décoratif : c'est l'exécution qui prouve que le chargeur s'en sort.
        for binary in [&plain.binary, &pie.binary] {
            let run = Command::new(binary).output().expect("le binaire doit s'exécuter");
            assert!(run.status.success(), "{}: {:?}", binary.display(), run.status);
            assert_eq!(
                String::from_utf8_lossy(&run.stdout).lines().count(),
                3,
                "{} : le compteur en .data doit faire trois tours",
                binary.display()
            );
        }
    }

    /// Un code qui adresse en absolu doit être refusé PAR LE LIEUR, franchement,
    /// plutôt que produire un binaire qui lira n'importe où. C'est aussi la
    /// leçon du chapitre : l'erreur de relogement est le symptôme à reconnaître.
    #[test]
    fn absolute_addressing_is_refused_at_link_time_in_pie() {
        let dir = Path::new("build/pie-absolu");
        std::fs::create_dir_all(dir).expect("dossier de test");
        let src = dir.join("absolu.asm");
        std::fs::write(
            &src,
            "section .data\nmsg db \"x\",10\nsection .text\nglobal _start\n_start:\n    mov rsi, msg\n    mov rax, 60\n    xor rdi, rdi\n    syscall\n",
        )
        .expect("écriture du source");

        // Sans -pie, ce même source est parfaitement valide.
        assemble_for(&src, dir, &[], Target::Linux, Lang::Fr).expect("lien ordinaire");

        let err = match assemble_for_with(
            &src,
            dir,
            &[],
            Target::Linux,
            LinkOptions { pie: true },
            Lang::Fr,
        ) {
            Err(e) => e,
            Ok(_) => panic!("ld doit refuser une adresse absolue en -pie"),
        };
        assert!(err.starts_with("Échec de ld"), "{err}");
    }

    /// Le filet de sécurité de l'option : tant qu'elle n'est pas cochée, tout le
    /// catalogue livré se lie exactement comme avant. Un `ld` qui se serait mis
    /// à passer `-pie` de lui-même produirait des binaires d'un autre type, et
    /// refuserait la plupart de ces sources.
    #[test]
    fn the_default_link_still_produces_plain_executables_everywhere() {
        use object::Object as _;
        let mut checked = 0;
        for dir in ["examples", "examples_seed"] {
            let Ok(entries) = std::fs::read_dir(dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("asm") {
                    continue;
                }
                let Ok(source) = std::fs::read_to_string(&path) else { continue };
                if detect_target(&source).is_some_and(Target::is_windows) {
                    continue; // le lien PE ne prend pas d'option
                }
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("x");
                let out_dir = PathBuf::from("build/pie-survey").join(stem);
                let Ok(out) = assemble_for(
                    &path,
                    &out_dir,
                    &[PathBuf::from("examples")],
                    Target::Linux,
                    Lang::Fr,
                ) else {
                    continue; // ne s'assemble pas seul : hors sujet ici
                };
                checked += 1;
                assert!(
                    out.log.contains("$ ld -o") && !out.log.contains("-pie"),
                    "{} lié autrement que par défaut :\n{}",
                    path.display(),
                    out.log
                );
                let data = std::fs::read(&out.binary).expect("lecture du binaire");
                let kind = object::File::parse(&*data).expect("ELF lisible").kind();
                assert_eq!(
                    kind,
                    object::ObjectKind::Executable,
                    "{} n'est plus un ET_EXEC",
                    path.display()
                );
            }
        }
        assert!(checked > 20, "trop peu de sources vérifiées ({checked})");
    }

    /// Les programmes de départ des leçons sont l'autre moitié du corpus : ils
    /// vivent dans le binaire, et personne ne les verrait changer de type.
    #[test]
    fn lesson_starters_are_linked_as_before_too() {
        let dir = Path::new("build/pie-survey-lessons");
        let mut checked = 0;
        for lesson in crate::tutorial::catalogue() {
            let Some(starter) = lesson.starter else { continue };
            if lesson.target().is_windows() {
                continue;
            }
            let out_dir = dir.join(lesson.id);
            std::fs::create_dir_all(&out_dir).expect("dossier de test");
            let src = out_dir.join("lecon.asm");
            std::fs::write(&src, starter).expect("écriture");
            let Ok(out) = assemble_for(&src, &out_dir, &[], Target::Linux, Lang::Fr) else {
                continue; // un starter à trous peut ne pas s'assembler tel quel
            };
            checked += 1;
            assert!(
                !out.log.contains("-pie"),
                "{} lié en -pie sans qu'on l'ait demandé :\n{}",
                lesson.id,
                out.log
            );
            assert_eq!(elf_kind(&out.binary), object::ObjectKind::Executable, "{}", lesson.id);
        }
        assert!(checked > 20, "trop peu de leçons vérifiées ({checked})");
    }
}

#[cfg(test)]
mod asmstd_tests {
    use super::*;
    use std::path::Path;

    /// asmstd doit s'assembler ET donner les bons résultats.
    ///
    /// Le programme de contrôle exerce les fonctions utilitaires (caractères,
    /// chaînes, mémoire, arithmétique, tableaux) et empile chaque résultat ;
    /// il les affiche ensuite dans l'ordre inverse. Écrire de l'assembleur
    /// sans l'exécuter ne prouve rien.
    #[test]
    fn asmstd_utilities_produce_correct_results() {
        let out = assemble_with_includes(
            Path::new("examples/asmstd-check.asm"),
            Path::new("build/asmstd-check"),
            &[Path::new("examples").to_path_buf()],
        )
        .expect("asmstd-check.asm doit s'assembler");

        let run = std::process::Command::new(&out.binary)
            .output()
            .expect("le binaire doit s'exécuter");
        let stdout = String::from_utf8_lossy(&run.stdout);
        let got: Vec<i64> = stdout
            .lines()
            .filter_map(|l| l.trim().parse().ok())
            .collect();

        // Les valeurs sont dépilées, donc lues à l'envers de l'ordre d'empilage.
        let expected: Vec<i64> = vec![
            1,    // strncmp("Bonjour…", "bonjour") != 0  → casse significative
            0,    // memcmp(s1, s1, 5) == 0
            7,    // arr_reverse : premier élément devient l'ancien dernier
            9,    // arr_sort : plus grand en queue
            1,    // arr_sort : plus petit en tête
            2,    // arr_find(9) → index 2
            1,    // arr_min
            9,    // arr_max
            25,   // arr_sum
            10,   // clamp(20, 0, 10)
            9,    // max(3, 9)
            0,    // divmod par zéro : neutralisé, pas d'exception
            3,    // 17 / 5
            2,    // 17 % 5
            1024, // pow(2, 10)
            12,   // lcm(4, 6)
            6,    // gcd(48, 18)
            42,   // abs(-42)
            82,   // str_reverse : 'R' de "RUOJNOB"
            1,    // str_upper a bien majusculé
            8,    // strchr('M') → index 8
            3,    // str_count('o') dans "Bonjour Monde"
            8,    // strstr("Monde") → index 8
            1,    // is_space(' ')
        ];
        assert_eq!(
            got.len(),
            expected.len(),
            "nombre de résultats inattendu :\n{stdout}"
        );
        for (i, (g, e)) in got.iter().zip(&expected).enumerate() {
            assert_eq!(g, e, "résultat {i} : obtenu {g}, attendu {e}\n{stdout}");
        }
    }
}
#[cfg(test)]
mod project_assembly_tests {
    use super::*;

    #[test]
    fn a_linux_project_links_several_sources() {
        let dir = Path::new("build/test-project-multi");
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir.join("src")).expect("dossier projet");
        std::fs::write(
            dir.join("src/main.asm"),
            "section .text\nglobal _start\nextern twice\n_start:\n    mov rdi, 21\n    call twice\n    mov rdi, rax\n    mov rax, 60\n    syscall\n",
        )
        .expect("source principal");
        std::fs::write(
            dir.join("src/math.asm"),
            "section .text\nglobal twice\ntwice:\n    lea rax, [rdi + rdi]\n    ret\n",
        )
        .expect("source secondaire");
        let manifest = dir.join(crate::project::MANIFEST_NAME);
        std::fs::write(
            &manifest,
            "entry = \"src/main.asm\"\nsources = [\"src/main.asm\", \"src/math.asm\"]\nincludes = []\n",
        )
        .expect("manifest");
        let project = Project::load(&manifest).expect("manifest valide");

        let out = assemble_project(&project, &dir.join("build"), Target::Linux, LinkOptions::default(), Lang::Fr)
            .expect("projet multi-fichiers");
        assert!(out.binary.is_file());
        let status = Command::new(&out.binary).status().expect("exécution");
        assert_eq!(status.code(), Some(42));
        let _ = std::fs::remove_dir_all(dir);
    }
}
