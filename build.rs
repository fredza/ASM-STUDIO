//! Injecte le hash git, la date et le numéro de build comme variables
//! d'environnement de compilation (voir `src/version.rs`).

use std::process::Command;

fn main() {
    let git = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "inconnu".to_string());
    println!("cargo:rustc-env=GIT_HASH={git}");

    let date = Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "?".to_string());
    println!("cargo:rustc-env=BUILD_DATE={date}");

    println!("cargo:rustc-env=BUILD_NUMBER={}", next_build_number());

    // Recompile quand le code, le manifeste ou le commit changent.
    //
    // Déclarer ces dépendances explicitement a un second effet, voulu : dès
    // qu'un `rerun-if-changed` est émis, cargo cesse de surveiller *tous* les
    // fichiers du paquet. `build-number.txt`, que ce script réécrit à chaque
    // passage, ne provoque donc pas une reconstruction en boucle.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/logs/HEAD");
}

/// Lit le compteur de builds, l'incrémente et le réécrit.
///
/// Le compteur vit dans `build-number.txt`, à la racine du dépôt : un fichier
/// d'une ligne qui survit à un `cargo clean` — contrairement à `OUT_DIR`, qui
/// repartirait de zéro et ferait reculer le numéro. Un compteur de build qui
/// recule ne sert plus à identifier un binaire.
///
/// Il n'est pas versionné (voir `.gitignore`) : il compte les compilations
/// d'une machine, pas les livraisons du projet. Ce qui identifie une livraison,
/// ce sont la version et le hash du commit, tous deux affichés à côté.
///
/// Si le fichier est illisible ou abîmé, on repart de la valeur qu'on sait sûre
/// plutôt que d'échouer la compilation : un numéro de build n'est pas une
/// raison de ne pas livrer.
fn next_build_number() -> u64 {
    let path = std::path::Path::new("build-number.txt");
    let current = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let next = current + 1;
    let _ = std::fs::write(path, format!("{next}\n"));
    next
}
