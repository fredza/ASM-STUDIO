//! Licence obligatoire pour le désassemblage, les registres/flags et la
//! timeline.
//!
//! Format collé par l'utilisateur : `<payload_json_base64>.<signature_base64>`.
//! La signature Ed25519 porte sur les octets JSON bruts du payload. La clé
//! privée correspondante n'existe pas dans ce dépôt : elle vit dans l'outil
//! tiers (séparé, privé) qui émet les licences ; ici on ne fait que vérifier.
//!
//! Le champ `release_sha3_512` du payload est signé donc infalsifiable, mais
//! n'est JAMAIS recalculé ni comparé ici : il sert de traçabilité côté auteur
//! (« cette licence a bien été émise pour le binaire officiel de telle
//! version »), pas de verrou technique. Un contrôle strict casserait l'usage
//! légitime de quiconque recompile depuis les sources (autorisé par l'ASFL) :
//! son binaire, donc son hash, diffère forcément de celui de la release
//! officielle. Seules la signature et la version comptent côté client.

use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;

use crate::app::paths::license_path;

/// Clé publique de l'outil de génération de licences (dépôt privé séparé).
///
/// Placeholder tant que cet outil n'existe pas : dérivée de la seed
/// `[0u8; 32]`, uniquement pour que `VerifyingKey::from_bytes` réussisse (un
/// remplissage à zéro ne correspond à aucun point de courbe valide). À
/// remplacer par la vraie clé publique dès que l'outil tiers en génère une,
/// puis republier une version — seul point de couplage entre les deux dépôts.
const PUBLIC_KEY: [u8; 32] = [
    0x3B, 0x6A, 0x27, 0xBC, 0xCE, 0xB6, 0xA4, 0x2D, 0x62, 0xA3, 0xA8, 0xD0, 0x2A, 0x6F, 0x0D, 0x73,
    0x65, 0x32, 0x15, 0x77, 0x1D, 0xE2, 0x43, 0xA6, 0x3A, 0xC0, 0x48, 0xA1, 0x8B, 0x59, 0xDA, 0x29,
];

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LicensePayload {
    pub(crate) name: String,
    #[allow(dead_code)] // affiché plus tard dans la fenêtre de licence si besoin
    pub(crate) email: String,
    pub(crate) version: String,
    /// Traçabilité côté auteur uniquement — voir la doc de module.
    #[allow(dead_code)]
    pub(crate) release_sha3_512: String,
    #[allow(dead_code)]
    pub(crate) issued_at: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) enum LicenseState {
    #[default]
    Missing,
    Valid(LicensePayload),
    Invalid(String),
}

/// Vérifie `raw` avec une clé donnée : signature Ed25519, puis version.
/// Paramétrée sur la clé pour que les tests signent avec une paire de test,
/// sans dépendre de la vraie clé privée (absente de ce dépôt).
fn verify_with_key(raw: &str, key: &VerifyingKey) -> Result<LicensePayload, String> {
    let cleaned: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    let (payload_b64, sig_b64) = cleaned.split_once('.').ok_or("format de licence invalide")?;

    let engine = base64::engine::general_purpose::STANDARD;
    let payload_bytes = engine
        .decode(payload_b64)
        .map_err(|_| "licence : encodage invalide")?;
    let sig_bytes = engine
        .decode(sig_b64)
        .map_err(|_| "licence : signature mal encodée")?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| "licence : signature de longueur incorrecte")?;

    key.verify(&payload_bytes, &Signature::from_bytes(&sig_arr))
        .map_err(|_| "signature invalide")?;

    let payload: LicensePayload =
        serde_json::from_slice(&payload_bytes).map_err(|_| "contenu de licence illisible")?;

    let current = env!("CARGO_PKG_VERSION");
    if payload.version != current {
        return Err(format!(
            "licence émise pour la version {}, ceci est la {current}",
            payload.version
        ));
    }
    Ok(payload)
}

/// Vérifie `raw` avec la clé publique embarquée.
pub(crate) fn verify(raw: &str) -> Result<LicensePayload, String> {
    let key = VerifyingKey::from_bytes(&PUBLIC_KEY).expect("clé publique embarquée invalide");
    verify_with_key(raw, &key)
}

/// Charge et vérifie la licence stockée sur disque, si présente.
pub(crate) fn load() -> LicenseState {
    // Comme `load_settings` dans `app/mod.rs` : les tests ne doivent pas
    // dépendre d'une licence installée sur la machine de développement.
    if cfg!(test) {
        return LicenseState::Missing;
    }
    let Some(path) = license_path() else { return LicenseState::Missing };
    let Ok(raw) = std::fs::read_to_string(&path) else { return LicenseState::Missing };
    match verify(&raw) {
        Ok(payload) => LicenseState::Valid(payload),
        Err(reason) => LicenseState::Invalid(reason),
    }
}

/// Sauvegarde la chaîne collée telle quelle (pas de reparsing/réencodage : le
/// fichier disque doit rester bit-à-bit identique à ce qui a été signé).
pub(crate) fn save(raw: &str) -> std::io::Result<()> {
    let path = license_path()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "pas de HOME"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, raw.trim())
}

/// Licence valide de complaisance, pour les tests d'autres modules qui ont
/// besoin d'un panneau réellement déverrouillé (ex. rendu de `registers_ui`)
/// sans exercer le mécanisme de licence lui-même.
#[cfg(test)]
pub(crate) fn valid_for_tests() -> LicenseState {
    LicenseState::Valid(LicensePayload {
        name: "Test".to_string(),
        email: "test@example.com".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        release_sha3_512: String::new(),
        issued_at: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    /// Paire de test déterministe, indépendante de `PUBLIC_KEY` : prouve que
    /// la vérification dépend bien de la clé passée, pas d'un raccourci.
    fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn sign(payload_json: &str, sk: &SigningKey) -> String {
        let engine = base64::engine::general_purpose::STANDARD;
        let sig = sk.sign(payload_json.as_bytes());
        format!(
            "{}.{}",
            engine.encode(payload_json.as_bytes()),
            engine.encode(sig.to_bytes())
        )
    }

    fn payload_json(version: &str) -> String {
        format!(
            r#"{{"name":"Jean Dupont","email":"jean@example.com","version":"{version}","release_sha3_512":"aa","issued_at":"2026-08-04"}}"#
        )
    }

    #[test]
    fn valid_signature_and_current_version_is_ok() {
        let sk = test_key();
        let raw = sign(&payload_json(env!("CARGO_PKG_VERSION")), &sk);
        let payload = verify_with_key(&raw, &sk.verifying_key()).expect("doit être valide");
        assert_eq!(payload.name, "Jean Dupont");
    }

    #[test]
    fn version_mismatch_is_rejected_with_both_versions_named() {
        let sk = test_key();
        let raw = sign(&payload_json("0.0.1-jamais-cette-version"), &sk);
        let err = verify_with_key(&raw, &sk.verifying_key()).unwrap_err();
        assert!(err.contains("0.0.1-jamais-cette-version"));
        assert!(err.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn tampered_payload_is_rejected_without_panicking() {
        let sk = test_key();
        let raw = sign(&payload_json(env!("CARGO_PKG_VERSION")), &sk);
        let (payload_b64, sig_b64) = raw.split_once('.').unwrap();
        // Décode, modifie un octet, réencode : la signature ne correspond plus.
        let engine = base64::engine::general_purpose::STANDARD;
        let mut bytes = engine.decode(payload_b64).unwrap();
        bytes[0] ^= 0xFF;
        let tampered = format!("{}.{}", engine.encode(bytes), sig_b64);
        let err = verify_with_key(&tampered, &sk.verifying_key()).unwrap_err();
        assert_eq!(err, "signature invalide");
    }

    #[test]
    fn malformed_input_is_rejected_without_panicking() {
        let key = test_key().verifying_key();
        assert!(verify_with_key("sans-point-separateur", &key).is_err());
        assert!(verify_with_key("!!!.!!!", &key).is_err());
        assert!(verify_with_key("YQ==.YQ==", &key).is_err()); // signature trop courte
    }

    #[test]
    fn pasted_line_breaks_are_tolerated() {
        let sk = test_key();
        let raw = sign(&payload_json(env!("CARGO_PKG_VERSION")), &sk);
        let (a, b) = raw.split_once('.').unwrap();
        let messy = format!("  {a}\n.\n{b}  \n");
        assert!(verify_with_key(&messy, &sk.verifying_key()).is_ok());
    }

    #[test]
    fn corrupt_json_but_correctly_signed_is_rejected_without_panicking() {
        let sk = test_key();
        let raw = sign("pas du json valide", &sk);
        let err = verify_with_key(&raw, &sk.verifying_key()).unwrap_err();
        assert_eq!(err, "contenu de licence illisible");
    }
}
