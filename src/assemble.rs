//! Assemblage d'un fichier `.asm` NASM vers un binaire ELF64 exécutable.
//!
//! Enchaîne `nasm -f elf64` (avec listing `.lst` pour un futur mapping
//! adresse ↔ ligne source) puis `ld`. Les artefacts sont écrits dans `out_dir`.

use std::path::{Path, PathBuf};
use std::process::Command;

pub struct BuildOutput {
    /// Chemin du binaire ELF produit, prêt à être lancé sous ptrace.
    pub binary: PathBuf,
    /// Journal des commandes exécutées (affiché dans la console).
    pub log: String,
}

pub fn assemble(src: &Path, out_dir: &Path) -> Result<BuildOutput, String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("création de {}: {e}", out_dir.display()))?;

    let stem = src
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "nom de fichier source invalide".to_string())?;

    let obj = out_dir.join(format!("{stem}.o"));
    let listing = out_dir.join(format!("{stem}.lst"));
    let binary = out_dir.join(stem);
    let mut log = String::new();

    // 1) nasm -f elf64 src -o obj -l listing
    log.push_str(&format!(
        "$ nasm -f elf64 {} -o {} -l {}\n",
        src.display(),
        obj.display(),
        listing.display()
    ));
    let nasm = Command::new("nasm")
        .args(["-f", "elf64"])
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
    Ok(BuildOutput { binary, log })
}
