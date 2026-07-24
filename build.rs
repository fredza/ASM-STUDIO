//! Injecte le hash git et la date de build comme variables d'environnement
//! de compilation, pour la fenêtre « À propos ».

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

    // Recompile la crate quand le commit change (met le hash à jour).
    // .git/HEAD change au checkout de branche ; .git/logs/HEAD à chaque commit.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/logs/HEAD");
}
