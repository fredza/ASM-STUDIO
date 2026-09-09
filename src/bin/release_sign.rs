//! Outil de mainteneur : génère la paire de clés Ed25519 des mises à jour, et
//! signe le binaire de release avec la clé privée.
//!
//! N'est PAS distribué avec ASM Studio — `install/package.sh` ne prend que
//! `target/release/asm_studio` par son nom, jamais celui-ci. Il existe pour
//! que la CI (`.github/workflows/release.yml`) puisse signer chaque
//! publication sans qu'aucune clé privée ne passe jamais par un fichier du
//! dépôt : la graine de 32 octets ne vit que dans le secret GitHub Actions
//! `UPDATE_SIGNING_KEY`, injecté en variable d'environnement au moment de la
//! signature.
//!
//! Usage :
//!   release_sign keygen <fichier de sortie>
//!       Tire 32 octets de `/dev/urandom`, écrit la graine en hexadécimal
//!       dans <fichier de sortie> (créé en 0600, jamais sur la sortie
//!       standard — ce fichier est le SEUL endroit où la clé privée
//!       transite, pour ne jamais finir dans un journal de terminal ou une
//!       transcription de session), et affiche la clé publique en littéral
//!       Rust `[u8; 32]` sur la sortie standard (à coller dans
//!       `UPDATE_PUBLIC_KEY`, src/updater.rs — elle, n'a rien de secret).
//!
//!   release_sign sign <graine hex> <fichier>
//!       Signe <fichier> avec la graine donnée et écrit <fichier>.sig
//!       (signature Ed25519, 64 octets, encodée en Base64 standard) — le
//!       format attendu par `download_signature` dans src/updater.rs.

use std::env;
use std::fs;
use std::process::ExitCode;

use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("keygen") => match args.get(2) {
            Some(out_path) => keygen(out_path),
            None => usage(),
        },
        Some("sign") => match (args.get(2), args.get(3)) {
            (Some(seed_hex), Some(path)) => sign(seed_hex, path),
            _ => usage(),
        },
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("usage : release_sign keygen <fichier de sortie pour la graine>");
    eprintln!("        release_sign sign <graine hex 64 caractères> <fichier>");
    ExitCode::FAILURE
}

fn keygen(out_path: &str) -> ExitCode {
    // `read_exact` plutôt qu'un simple `read` : un unique appel système sur
    // /dev/urandom peut rendre moins de 32 octets, `read_exact` relit tant
    // qu'il en manque.
    let mut seed = [0u8; 32];
    if let Err(e) = fs::File::open("/dev/urandom").and_then(|mut f| {
        use std::io::Read;
        f.read_exact(&mut seed)
    }) {
        eprintln!("lecture de /dev/urandom : {e}");
        return ExitCode::FAILURE;
    }
    let signing_key = SigningKey::from_bytes(&seed);
    let public = signing_key.verifying_key().to_bytes();
    let seed_hex: String = seed.iter().map(|b| format!("{b:02x}")).collect();

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true).mode(0o600);
        let write_result = opts.open(out_path).and_then(|mut f| {
            use std::io::Write;
            f.write_all(seed_hex.as_bytes())
        });
        if let Err(e) = write_result {
            eprintln!("écriture de {out_path} : {e}");
            return ExitCode::FAILURE;
        }
    }
    #[cfg(not(unix))]
    {
        if let Err(e) = fs::write(out_path, &seed_hex) {
            eprintln!("écriture de {out_path} : {e}");
            return ExitCode::FAILURE;
        }
    }

    eprintln!("graine privée écrite dans {out_path} (0600) — jamais affichée");
    println!("Clé publique (à coller dans UPDATE_PUBLIC_KEY, src/updater.rs) :");
    print!("[");
    for (i, b) in public.iter().enumerate() {
        if i > 0 {
            print!(", ");
        }
        print!("0x{b:02X}");
    }
    println!("]");
    ExitCode::SUCCESS
}

fn sign(seed_hex: &str, path: &str) -> ExitCode {
    let Some(seed) = hex_decode_32(seed_hex) else {
        eprintln!("graine invalide : il faut 64 caractères hexadécimaux (32 octets)");
        return ExitCode::FAILURE;
    };
    let signing_key = SigningKey::from_bytes(&seed);

    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("lecture de {path} : {e}");
            return ExitCode::FAILURE;
        }
    };
    let signature = signing_key.sign(&bytes);
    let encoded = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());

    let sig_path = format!("{path}.sig");
    if let Err(e) = fs::write(&sig_path, &encoded) {
        eprintln!("écriture de {sig_path} : {e}");
        return ExitCode::FAILURE;
    }
    println!("{sig_path}");
    ExitCode::SUCCESS
}

fn hex_decode_32(s: &str) -> Option<[u8; 32]> {
    let s = s.trim();
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}
