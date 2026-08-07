//! Assemblage d'un fichier `.asm` NASM vers un binaire ELF64 exécutable.
//!
//! Enchaîne `nasm -f elf64` (avec listing `.lst` pour un futur mapping
//! adresse ↔ ligne source) puis `ld`. Les artefacts sont écrits dans `out_dir`.

use std::path::{Path, PathBuf};
use std::process::Command;

pub struct BuildOutput {
    /// Chemin du binaire ELF produit, prêt à être lancé sous ptrace.
    pub binary: PathBuf,
    /// Chemin du listing NASM (`.lst`), pour le mapping adresse ↔ ligne source.
    pub listing: PathBuf,
    /// Journal des commandes exécutées (affiché dans la console).
    pub log: String,
}

/// Assemble (nasm) puis lie (ld) `src`, en ajoutant des répertoires de recherche
/// `%include` (`nasm -i`) : par ex. le dossier du fichier et celui d'`asmstd.inc`.
/// Passer `&[]` pour n'ajouter aucun chemin d'include.
pub fn assemble_with_includes(
    src: &Path,
    out_dir: &Path,
    includes: &[PathBuf],
) -> Result<BuildOutput, String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("création de {}: {e}", out_dir.display()))?;

    let stem = src
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "nom de fichier source invalide".to_string())?;

    let obj = out_dir.join(format!("{stem}.o"));
    let listing = out_dir.join(format!("{stem}.lst"));
    let binary = out_dir.join(stem);
    let mut log = String::new();

    // 1) nasm -f elf64 [-i dir/...] src -o obj -l listing
    // nasm attend le chemin collé à l'option et terminé par un séparateur.
    let inc_args: Vec<String> = includes
        .iter()
        .map(|d| format!("-i{}/", d.display()))
        .collect();
    log.push_str(&format!(
        "$ nasm -f elf64 {}{} -o {} -l {}\n",
        inc_args.iter().map(|a| format!("{a} ")).collect::<String>(),
        src.display(),
        obj.display(),
        listing.display()
    ));
    let nasm = Command::new("nasm")
        .args(["-f", "elf64"])
        .args(&inc_args)
        .arg(src)
        .arg("-o")
        .arg(&obj)
        .arg("-l")
        .arg(&listing)
        .output()
        .map_err(|e| format!("impossible de lancer nasm: {e}"))?;
    log.push_str(&String::from_utf8_lossy(&nasm.stderr));
    if !nasm.status.success() {
        return Err(format!("Échec de nasm:\n{log}"));
    }

    // 2) ld -o binary obj
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
