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
    assemble_for(src, out_dir, includes, Target::Linux)
}

/// Assemble `src` pour la cible demandée.
pub fn assemble_for(
    src: &Path,
    out_dir: &Path,
    includes: &[PathBuf],
    target: Target,
) -> Result<BuildOutput, String> {
    match target {
        Target::Linux => assemble_elf(src, out_dir, includes),
        Target::Windows | Target::WindowsGui => assemble_pe(src, out_dir, includes, target),
    }
}

/// Assemble et lie un objet COFF en exécutable PE64, sans outil externe autre
/// que `nasm` : le lien est fait par [`crate::pe_link`].
fn assemble_pe(
    src: &Path,
    out_dir: &Path,
    includes: &[PathBuf],
    target: Target,
) -> Result<BuildOutput, String> {
    let (stem, listing, mut log) = nasm(src, out_dir, includes, target)?;
    let obj = out_dir.join(format!("{stem}.obj"));
    let binary = out_dir.join(format!("{stem}.exe"));

    let subsystem = if target == Target::WindowsGui {
        crate::pe_link::Subsystem::Gui
    } else {
        crate::pe_link::Subsystem::Console
    };
    log.push_str(&format!("$ (lieur PE intégré) -o {}\n", binary.display()));
    let report = crate::pe_link::link(&obj, &binary, subsystem)
        .map_err(|e| format!("Échec du lien PE:\n{log}{e}\n"))?;
    log.push_str(&format!(
        "  point d'entrée : {} (RVA 0x{:X})\n",
        report.entry.0, report.entry.1
    ));
    for imp in &report.imports {
        log.push_str(&format!("  import : {} ← {}\n", imp.func, imp.dll));
    }
    log.push_str(&format!("Build OK — {} octets (PE64 Windows)\n", report.size));
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
) -> Result<(String, PathBuf, String), String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("création de {}: {e}", out_dir.display()))?;
    let stem = src
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "nom de fichier source invalide".to_string())?
        .to_string();
    let obj = out_dir.join(format!(
        "{stem}.{}",
        if target.is_windows() { "obj" } else { "o" }
    ));
    let listing = out_dir.join(format!("{stem}.lst"));

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
        .arg(&obj)
        .arg("-l")
        .arg(&listing)
        .output()
        .map_err(|e| format!("impossible de lancer nasm: {e}"))?;
    log.push_str(&String::from_utf8_lossy(&out.stderr));
    if !out.status.success() {
        return Err(format!("Échec de nasm:\n{log}"));
    }
    Ok((stem, listing, log))
}

fn assemble_elf(
    src: &Path,
    out_dir: &Path,
    includes: &[PathBuf],
) -> Result<BuildOutput, String> {
    let (stem, listing, mut log) = nasm(src, out_dir, includes, Target::Linux)?;
    let obj = out_dir.join(format!("{stem}.o"));
    let binary = out_dir.join(&stem);

    // ld -o binary obj
    log.push_str(&format!("$ ld -o {} {}\n", binary.display(), obj.display()));
    let ld = Command::new("ld")
        .arg("-o")
        .arg(&binary)
        .arg(&obj)
        .output()
        .map_err(|e| format!("impossible de lancer ld: {e}"))?;
    log.push_str(&String::from_utf8_lossy(&ld.stderr));
    if !ld.status.success() {
        return Err(format!("Échec de ld:\n{log}"));
    }

    log.push_str("Build OK\n");
    Ok(BuildOutput { binary, listing, log })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            )
            .expect("hello-windows.asm doit s'assembler");
            assert_eq!(out.binary.extension().and_then(|e| e.to_str()), Some("exe"));
            assert!(out.binary.exists(), "le .exe doit être écrit");
            assert!(out.log.contains("point d'entrée : main"), "journal: {}", out.log);
            assert!(out.log.contains("WriteFile ← kernel32.dll"), "journal: {}", out.log);
        }
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
