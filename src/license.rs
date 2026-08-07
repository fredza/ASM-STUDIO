//! Licence obligatoire pour le désassemblage, les registres/flags et la
//! timeline.
//!
//! Format collé par l'utilisateur : `<payload_json_base64>.<signature_base64>`.
//! La signature Ed25519 porte sur les octets JSON bruts du payload. La clé
//! privée correspondante n'existe pas dans ce dépôt : elle vit dans l'outil
//! tiers (séparé, privé) qui émet les licences ; ici on ne fait que vérifier.
//!
//! Les champs `release_sha3_512` et `build` du payload sont signés donc
//! infalsifiables, mais ne sont JAMAIS recalculés ni comparés ici : ils
//! servent de traçabilité côté auteur (« cette licence a bien été émise pour
//! le binaire officiel de telle version, tel build ») lors de l'émission,
//! pas de verrou technique. `build` reprend le hash git court affiché dans
//! « À propos » (`env!("GIT_HASH")`, identique côté client) au moment où la
//! licence est demandée — l'utilisateur le copie depuis « À propos » et le
//! communique à l'auteur, qui le colle dans l'outil d'émission
//! (`asm-studio-license-tool`). Un contrôle strict casserait l'usage
//! légitime de quiconque recompile depuis les sources (autorisé par l'ASFL)
//! ou reçoit un correctif sans changement de version : son build, donc son
//! hash git, diffère forcément de celui au moment de l'émission. Seules la
//! signature, la version et l'expiration (si présente) comptent côté
//! client.
//!
//! `expires_at` (AAAA-MM-JJ, optionnel) permet des licences à durée limitée
//! (calculée à l'émission par l'outil tiers, jamais recalculée ici) plutôt
//! que des licences perpétuelles. Absent = perpétuelle, y compris pour
//! toutes les licences émises avant l'ajout de ce champ (compatibilité
//! ascendante via `#[serde(default)]`). La comparaison utilise
//! `crate::trial::trusted_now()`, pas l'horloge système brute : sinon
//! reculer l'horloge après expiration suffirait à faire réapparaître une
//! licence expirée, comme pour le délai d'essai (voir `crate::trial`).

use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;

use crate::app::paths::license_path;

/// Clé publique de l'outil de génération de licences (dépôt privé séparé).
///
/// Doit correspondre exactement à la `private.key` chargée dans
/// `asm-studio-license-tool` au moment de l'émission : la GUI de l'outil
/// affiche le tableau à coller ici dès qu'une clé est chargée. Toute
/// désynchronisation (nouvelle paire générée côté outil, tableau pas
/// répercuté ici) fait échouer *toutes* les licences avec « signature
/// invalide », sans autre indice — c'est le seul point de couplage entre
/// les deux dépôts, il se vérifie en comparant les deux tableaux.
const PUBLIC_KEY: [u8; 32] = [0x6C, 0x2B, 0x5E, 0x9D, 0xD0, 0x82, 0x9F, 0x0D, 0x80, 0x44, 0xC4, 0x96, 0xC6, 0x07, 0x3B, 0x4C, 0xBA, 0x22, 0x96, 0x71, 0xB8, 0xD8, 0xD4, 0xFE, 0xBA, 0x4C, 0xDF, 0x77, 0x72, 0x49, 0x78, 0xF5];

/// Clés publiques dont la clé privée est publiquement connue, donc à ne
/// jamais embarquer dans un binaire : celle de la seed tout à zéro (valeur
/// obtenue quand `keygen` n'a pas encore tourné, ou qu'un `private.key` vide
/// a été chargé) et celle de la paire de test de `mod tests`
/// (`SigningKey::from_bytes(&[7u8; 32])`, en clair dans ce dépôt public).
///
/// Le scénario que `FORBIDDEN_KEYS` interdit s'est déjà produit : `TEST-LICENCE.md`
/// demande de coller une clé de test dans `PUBLIC_KEY` puis de **restaurer la
/// vraie avant de committer**, et la restauration a été oubliée. Le binaire
/// alors produit refuse *toutes* les vraies licences avec « signature
/// invalide », sans indice sur la cause — et accepte n'importe quelle licence
/// forgée par qui lit ce dépôt. Le symptôme est particulièrement trompeur
/// quand seul un des deux profils est recompilé après correction : la licence
/// « marche en debug mais pas en release » alors que le code est identique,
/// seule la constante figée dans chaque binaire diffère.
const FORBIDDEN_KEYS: [[u8; 32]; 2] = [
    // seed [0u8; 32]
    [0x3B, 0x6A, 0x27, 0xBC, 0xCE, 0xB6, 0xA4, 0x2D, 0x62, 0xA3, 0xA8, 0xD0, 0x2A, 0x6F, 0x0D, 0x73, 0x65, 0x32, 0x15, 0x77, 0x1D, 0xE2, 0x43, 0xA6, 0x3A, 0xC0, 0x48, 0xA1, 0x8B, 0x59, 0xDA, 0x29],
    // seed [7u8; 32] — `tests::test_key`
    [0xEA, 0x4A, 0x6C, 0x63, 0xE2, 0x9C, 0x52, 0x0A, 0xBE, 0xF5, 0x50, 0x7B, 0x13, 0x2E, 0xC5, 0xF9, 0x95, 0x47, 0x76, 0xAE, 0xBE, 0xBE, 0x7B, 0x92, 0x42, 0x1E, 0xEA, 0x69, 0x14, 0x46, 0xD2, 0x2C],
];

/// Refuse à la *compilation* (debug comme release, donc aussi tout paquet de
/// distribution) un binaire qui embarquerait une des [`FORBIDDEN_KEYS`] : le
/// seul moment où l'oubli est encore rattrapable sans rien republier. Boucles
/// `while` et comparaison octet par octet parce qu'un contexte `const`
/// n'admet ni `for` ni `==` sur tableaux.
const _: () = {
    let mut i = 0;
    while i < FORBIDDEN_KEYS.len() {
        let mut same = true;
        let mut j = 0;
        while j < 32 {
            if PUBLIC_KEY[j] != FORBIDDEN_KEYS[i][j] {
                same = false;
            }
            j += 1;
        }
        assert!(
            !same,
            "PUBLIC_KEY est une clé de test dont la clé privée est publique : \
             recollez la vraie clé publique d'asm-studio-license-tool avant de compiler"
        );
        i += 1;
    }
};

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
    /// Hash git court (`env!("GIT_HASH")`) du build pour lequel la licence a
    /// été demandée. Traçabilité côté auteur uniquement — voir la doc de
    /// module. Absent sur les licences émises avant l'ajout de ce champ
    /// (`#[serde(default)]`).
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) build: Option<String>,
    /// Date d'expiration (AAAA-MM-JJ), incluse dans la journée indiquée.
    /// Absente = licence perpétuelle — voir la doc de module.
    #[serde(default)]
    pub(crate) expires_at: Option<String>,
}

/// Convertit une date civile (algorithme de Howard Hinnant, domaine public)
/// en jours depuis l'epoch Unix — l'inverse de `civil_from_days` côté outil
/// d'émission (`asm-studio-license-tool/src/lib.rs`). Aucune dépendance à
/// une crate de dates pour un simple calcul de bornes de jour.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // [0, 11]
    let doy = (153 * mp as i64 + 2) / 5 + d as i64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

/// Borne unix (secondes) de fin de journée exclusive pour une date
/// `AAAA-MM-JJ` : la licence reste valide tant que l'heure de confiance est
/// strictement inférieure à cette borne (donc valide toute la journée
/// indiquée, jusqu'à minuit UTC exclu le lendemain).
fn end_of_day_unix(date: &str) -> Result<u64, String> {
    let mut parts = date.splitn(3, '-');
    let (Some(y), Some(m), Some(d)) = (parts.next(), parts.next(), parts.next()) else {
        return Err("date d'expiration illisible dans la licence".to_string());
    };
    let err = || "date d'expiration illisible dans la licence".to_string();
    let y: i64 = y.parse().map_err(|_| err())?;
    let m: u32 = m.parse().map_err(|_| err())?;
    let d: u32 = d.parse().map_err(|_| err())?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return Err(err());
    }
    let days = days_from_civil(y, m, d);
    Ok((days + 1) as u64 * 86400)
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

    if let Some(expires_at) = &payload.expires_at {
        let boundary = end_of_day_unix(expires_at)?;
        if crate::trial::trusted_now() >= boundary {
            return Err(format!("licence expirée le {expires_at}"));
        }
    }

    Ok(payload)
}

/// Concatène version et build (`{version}+{build}`, syntaxe des métadonnées
/// de build semver) en une seule chaîne copiable depuis « À propos » — c'est
/// ce que l'utilisateur colle dans l'outil d'émission de licences pour
/// renseigner les deux d'un coup. Voir `LicensePayload::build`.
pub(crate) fn version_build_tag() -> String {
    format!("{}+{}", env!("CARGO_PKG_VERSION"), env!("GIT_HASH"))
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

/// Supprime la licence stockée sur disque — l'appli retombe alors sur
/// `LicenseState::Missing` au prochain `load()`, donc sur le délai d'essai
/// s'il court encore, sur le verrouillage sinon.
///
/// Ne touche pas aux marqueurs de `crate::trial` : les remettre à zéro d'un
/// clic offrirait depuis l'interface exactement le contournement que leurs
/// trois copies redondantes cherchent à rendre délibéré. Désactiver une
/// licence ne rend donc pas un délai d'essai déjà écoulé.
///
/// Une licence déjà absente n'est pas une erreur (`NotFound` ignoré) : le
/// résultat visé — plus de licence sur disque — est atteint dans les deux cas.
pub(crate) fn remove() -> std::io::Result<()> {
    // Même garde que `load` : un test qui exerce le bouton « Désactiver »
    // effacerait sinon la licence installée sur la machine de développement.
    if cfg!(test) {
        return Ok(());
    }
    let path = license_path()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "pas de HOME"))?;
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
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
        build: None,
        expires_at: None,
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

    fn payload_json_with_expiry(version: &str, expires_at: &str) -> String {
        format!(
            r#"{{"name":"Jean Dupont","email":"jean@example.com","version":"{version}","release_sha3_512":"aa","issued_at":"2026-08-04","expires_at":"{expires_at}"}}"#
        )
    }

    /// Le garde-fou `const _` ne protège que des clés qu'il connaît : si la
    /// paire de test change ici, `FORBIDDEN_KEYS` doit changer aussi, sinon
    /// l'oubli redevient silencieux. Ce test relie les deux.
    #[test]
    fn the_test_key_is_listed_among_the_forbidden_ones() {
        assert!(
            FORBIDDEN_KEYS.contains(&test_key().verifying_key().to_bytes()),
            "ajoutez la clé publique de `test_key` à FORBIDDEN_KEYS"
        );
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
    fn license_without_expiry_field_is_perpetual() {
        // `payload_json` (sans `expires_at`) doit rester valide indéfiniment :
        // compatibilité ascendante avec les licences émises avant ce champ.
        let sk = test_key();
        let raw = sign(&payload_json(env!("CARGO_PKG_VERSION")), &sk);
        let payload = verify_with_key(&raw, &sk.verifying_key()).expect("doit être valide");
        assert!(payload.expires_at.is_none());
    }

    #[test]
    fn version_build_tag_concatenates_both_with_a_plus() {
        let tag = version_build_tag();
        let (version, build) = tag.split_once('+').expect("doit contenir un '+'");
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
        assert_eq!(build, env!("GIT_HASH"));
    }

    #[test]
    fn license_without_build_field_is_still_readable() {
        // Compatibilité ascendante : les licences émises avant l'ajout du
        // champ `build` (traçabilité, voir doc de module) restent valides.
        let sk = test_key();
        let raw = sign(&payload_json(env!("CARGO_PKG_VERSION")), &sk);
        let payload = verify_with_key(&raw, &sk.verifying_key()).expect("doit être valide");
        assert!(payload.build.is_none());
    }

    #[test]
    fn license_with_build_field_carries_it_through() {
        // `build` est signé mais purement informatif : il ne doit jamais
        // faire échouer la vérification, seulement être lu.
        let sk = test_key();
        let payload_json = format!(
            r#"{{"name":"Jean Dupont","email":"jean@example.com","version":"{}","release_sha3_512":"aa","issued_at":"2026-08-04","build":"a1b2c3d"}}"#,
            env!("CARGO_PKG_VERSION")
        );
        let raw = sign(&payload_json, &sk);
        let payload = verify_with_key(&raw, &sk.verifying_key()).expect("doit être valide");
        assert_eq!(payload.build.as_deref(), Some("a1b2c3d"));
    }

    #[test]
    fn license_valid_until_a_future_date_is_accepted() {
        let sk = test_key();
        let raw = sign(
            &payload_json_with_expiry(env!("CARGO_PKG_VERSION"), "2999-12-31"),
            &sk,
        );
        assert!(verify_with_key(&raw, &sk.verifying_key()).is_ok());
    }

    #[test]
    fn expired_license_is_rejected_with_the_date_named() {
        let sk = test_key();
        let raw = sign(
            &payload_json_with_expiry(env!("CARGO_PKG_VERSION"), "2000-01-01"),
            &sk,
        );
        let err = verify_with_key(&raw, &sk.verifying_key()).unwrap_err();
        assert!(err.contains("2000-01-01"), "message obtenu : {err}");
    }

    #[test]
    fn license_is_still_valid_on_its_last_day_of_expiry() {
        // La licence reste valide toute la journée indiquée par `expires_at`,
        // pas seulement jusqu'à minuit de la veille.
        let sk = test_key();
        let today = crate::trial::trusted_now() / 86_400 * 86_400; // minuit UTC aujourd'hui
        let (y, m, d) = {
            // Reconstruit AAAA-MM-JJ à partir d'aujourd'hui pour ne pas
            // dépendre d'une date codée en dur qui finirait par expirer.
            let days = (today / 86_400) as i64;
            civil_from_days_for_test(days)
        };
        let raw = sign(
            &payload_json_with_expiry(env!("CARGO_PKG_VERSION"), &format!("{y:04}-{m:02}-{d:02}")),
            &sk,
        );
        assert!(verify_with_key(&raw, &sk.verifying_key()).is_ok());
    }

    #[test]
    fn malformed_expiry_date_is_rejected_without_panicking() {
        let sk = test_key();
        for bad in ["pas-une-date", "2026-13-01", "2026-01-99", "2026/01/01", ""] {
            let raw = sign(&payload_json_with_expiry(env!("CARGO_PKG_VERSION"), bad), &sk);
            assert!(
                verify_with_key(&raw, &sk.verifying_key()).is_err(),
                "devait être rejeté : {bad:?}"
            );
        }
    }

    /// Inverse de `days_from_civil`, utilisée seulement pour construire une
    /// date de test « aujourd'hui » sans dépendre d'une constante qui
    /// expirerait avec le temps. Algorithme de Howard Hinnant, domaine
    /// public (même famille que `days_from_civil` ci-dessus).
    fn civil_from_days_for_test(z: i64) -> (i64, u32, u32) {
        let z = z + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = (z - era * 146097) as u64;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe as i64 + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let y = if m <= 2 { y + 1 } else { y };
        (y, m, d)
    }

    #[test]
    fn days_from_civil_matches_known_dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2022, 1, 8), 19_000);
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
