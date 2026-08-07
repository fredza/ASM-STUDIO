# Système de licence — fonctionnement interne

Document technique à usage interne : explique comment le verrouillage de
licence d'ASM Studio est construit, où il stocke ses données, et comment les
différentes fenêtres s'articulent entre elles. Pas destiné à l'utilisateur
final (voir plutôt `LICENSE.md` pour le texte légal ASFL).

## Vue d'ensemble

Trois panneaux réservés — désassemblage, registres/flags, timeline — sont
verrouillés tant que l'utilisateur n'a pas activé de licence *et* que la
période d'essai n'est plus active. Le système repose sur deux modules
indépendants :

- `src/license.rs` — vérifie une licence signée collée par l'utilisateur.
- `src/trial.rs` — délai de grâce de 14 jours avant que la licence devienne
  obligatoire, à compter du tout premier lancement.

Les deux sont **déclaratifs, pas des verrous cryptographiques absolus** :
le dépôt est public, donc quiconque lit ce fichier (ou le code) sait
exactement comment fonctionne le mécanisme et pourrait recompiler une
version qui ignore le contrôle. Le but n'est pas d'empêcher un contournement
délibéré et technique, mais d'empêcher la falsification d'une licence (grâce
à la signature Ed25519) et de décourager un effacement distrait du
marqueur d'essai.

## `src/license.rs` — la licence signée

**Format collé par l'utilisateur :** `<payload_json_base64>.<signature_base64>`

Le payload JSON contient :

```json
{
  "name": "...",
  "email": "...",
  "version": "0.4.0-beta.2",
  "release_sha3_512": "...",
  "build": "...",
  "issued_at": "...",
  "expires_at": "..."
}
```

- La signature **Ed25519** porte sur les octets JSON bruts du payload.
- `PUBLIC_KEY` (constante dans `license.rs`) est la clé publique de l'outil
  d'émission — `asm-studio-license-tool`, un outil séparé et privé (dépôt
  `~/RustroverProjects/asm-studio-license-tool`, jamais publié), qui n'est
  pas dans ce dépôt. Ce fichier ne fait que *vérifier*, jamais générer.
- ⚠️ **`PUBLIC_KEY` doit correspondre exactement à la `private.key` chargée
  dans l'outil au moment de l'émission.** C'est l'unique point de couplage
  entre les deux dépôts, et le seul mode de panne réellement rencontré :
  générer une nouvelle paire côté outil (`keygen`, ou « Générer une nouvelle
  paire… » dans la GUI) sans recoller le tableau ici fait échouer **toutes**
  les licences avec « signature invalide », sans autre indice — le message
  est le même que pour une licence trafiquée. Symptôme typique : « j'ai suivi
  toutes les instructions, bon binaire, bonne version, et l'appli refuse quand
  même ».
  Vérification en une commande, quand un doute existe :
  ```bash
  python3 -c "
  from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
  from cryptography.hazmat.primitives import serialization
  seed=bytes.fromhex(open('$HOME/RustroverProjects/asm-studio-license-tool/private.key').read().strip())
  pub=Ed25519PrivateKey.from_private_bytes(seed).public_key().public_bytes(
      serialization.Encoding.Raw, serialization.PublicFormat.Raw)
  print('[' + ', '.join('0x%02X'%b for b in pub) + ']')"
  ```
  La sortie doit être **identique** à `PUBLIC_KEY` dans `src/license.rs`
  (la GUI de l'outil affiche le même tableau dans « Clé de signature… »).
  Après toute mise à jour de la constante : recompiler ASM Studio, **et les
  deux profils, pas seulement celui que vous lancez d'habitude**. La clé est
  figée dans chaque binaire à la compilation, et `cargo build` /
  `cargo build --release` ont des caches séparés : recompiler le debug
  seulement donne le symptôme trompeur « la licence marche en debug mais pas
  en release » alors que le code source est identique — c'est le binaire
  release, plus ancien, qui vérifie encore avec l'ancienne clé.
- 🔒 Garde-fou : `src/license.rs` refuse **à la compilation** (`const _`, voir
  `FORBIDDEN_KEYS`) une `PUBLIC_KEY` dont la clé privée est publiquement
  connue — seed tout à zéro, ou paire de test de `mod tests`. Un placeholder
  ou une clé de test collée puis oubliée casse donc la build au lieu de
  produire un binaire qui refuse toutes les vraies licences (et accepte
  n'importe quelle licence forgée par qui lit ce dépôt public).
  `install/package.sh` vérifie en plus, avant d'archiver, que le binaire
  release embarque bien la clé actuellement dans les sources — un binaire
  laissé en cache depuis une compilation antérieure ne peut plus partir en
  distribution.
- `release_sha3_512` est signé (donc infalsifiable) mais **jamais recalculé
  ni comparé** côté client : sert de traçabilité auteur uniquement. Un
  contrôle strict casserait l'usage légitime de qui recompile depuis les
  sources (autorisé par l'ASFL), puisque son binaire — donc son hash —
  diffère forcément de la release officielle.
- `build` (optionnel, `#[serde(default)]`) — même logique que
  `release_sha3_512` : signé, jamais recalculé ni comparé côté client. C'est
  le hash git court (`env!("GIT_HASH")`) affiché dans « À propos », que le
  demandeur communique à l'auteur au moment de demander une licence, via le
  bouton 📋 unique à côté du champ « Build » — `license::version_build_tag()`
  copie `{version}+{build}` (syntaxe des métadonnées de build semver) en une
  seule chaîne, plutôt que deux boutons séparés. Permet de savoir a
  posteriori pour quel build exact une licence a été émise (traçabilité),
  sans jamais bloquer un utilisateur dont le build a changé depuis
  (recompilation, correctif sans bump de version).
  Côté `asm-studio-license-tool`, `normalize_version_build` (bâtie sur
  `split_version_build`, inverse de `version_build_tag`) répartit cette
  chaîne sur les deux champs **dès qu'elle est collée dans le champ
  « Version »** — GUI comme CLI. Sans cette normalisation, on signe une
  licence dont la version vaut `0.4.0-beta.2+d4022ad` : elle est rejetée
  côté client par le contrôle de version, avec un message exact mais
  déroutant (« licence émise pour la version 0.4.0-beta.2+d4022ad, ceci est
  la 0.4.0-beta.2 »).
- `expires_at` (AAAA-MM-JJ, optionnel) — voir « Licences à durée limitée »
  ci-dessous. Absent = licence perpétuelle.
- Vérifiées côté client : la **signature**, la **version**
  (`payload.version` doit égaler `env!("CARGO_PKG_VERSION")`) et, si
  présente, l'**expiration**.

### Licences à durée limitée (`expires_at`)

Optionnel, calculé par l'outil d'émission à partir d'une **durée en jours**
(`--valid-days` en CLI, champ « Durée de validité » dans le GUI) — jamais
saisi à la main comme une date, pour éviter qu'une faute de frappe finisse
figée et infalsifiable dans une licence signée. Absent du payload = licence
perpétuelle (y compris toutes les licences déjà émises avant l'ajout de ce
champ : `#[serde(default)]` côté `LicensePayload`).

`license::verify_with_key` compare la date d'expiration à
`crate::trial::trusted_now()` (voir plus bas, section `src/trial.rs`), pas
à l'horloge système brute : sinon reculer l'horloge après expiration
suffirait à faire réapparaître une licence expirée, exactement comme pour
le délai d'essai. La licence reste valide toute la journée indiquée par
`expires_at` (minuit UTC exclu le lendemain), pas seulement jusqu'à minuit
de la veille.

Le calcul de dates (jour civil ↔ jours depuis l'epoch Unix) utilise
l'algorithme de Howard Hinnant (domaine public), dupliqué à l'identique
dans les deux dépôts (`license.rs::days_from_civil` côté vérification,
`lib.rs::days_from_civil`/`civil_from_days` côté émission) — pas de
dépendance à une crate de dates pour un simple calcul de bornes de jour.

**Stockage disque :** `license.load()` / `license.save()` lisent et écrivent
le bloc collé, tel quel (pas de reparsing/réencodage), dans :

```
$XDG_CONFIG_HOME/asm_studio/license.txt
(ou ~/.config/asm_studio/license.txt si XDG_CONFIG_HOME est absent)
```

`LicenseState` a trois variantes : `Missing` (aucun fichier), `Valid(payload)`,
`Invalid(reason)` (fichier présent mais signature/version invalide — par
exemple une licence émise pour une version antérieure après mise à jour).

En test (`cfg!(test)`), `license::load()` renvoie toujours `Missing` : les
tests ne doivent jamais dépendre d'une licence installée sur la machine de
développement. `license::valid_for_tests()` fournit une licence de
complaisance pour les tests d'autres modules qui ont besoin d'un état
déverrouillé.

## `src/trial.rs` — le délai de grâce

Dès le tout premier lancement, les panneaux réservés restent utilisables
pendant `TRIAL_DAYS` (= 14) jours **sans licence**, avant de retomber sur le
verrouillage. Ce n'est pas un « essai » commercial : il n'y a rien à acheter
derrière, juste un délai avant de devoir s'inscrire (licence gratuite).

- La date de premier lancement (`first_seen`) et la dernière heure observée
  (`last_seen`) sont stockées, **redondées en trois copies** sur des
  répertoires XDG distincts, sous des noms neutres différents
  (`crate::app::paths::trial_marker_paths`) :

  ```
  $XDG_DATA_HOME/asm_studio/.cache_id      (ou ~/.local/share/asm_studio/.cache_id)
  $XDG_CACHE_HOME/asm_studio/.sess_meta    (ou ~/.cache/asm_studio/.sess_meta)
  $XDG_STATE_HOME/asm_studio/.ck           (ou ~/.local/state/asm_studio/.ck)
  ```

  Chaque copie contient `first_seen:last_seen` (secondes Unix ; un entier
  seul, ancien format, reste lu comme valant pour les deux champs).

  Ce n'est toujours pas un verrou absolu (dépôt public, cf. plus haut), mais
  ça ferme les deux contournements réalisables **sans recompiler** :

  1. **Supprimer le marqueur.** Avant, un seul `rm` sur `.cache_id`
     redonnait un essai complet. Maintenant, `trial::reconcile_from_disk`
     lit les trois copies, retient le `first_seen` le plus *ancien* trouvé
     (une copie survivante avec la vraie date l'emporte toujours sur une
     copie manquante ou trafiquée à une date récente) et réécrit celles qui
     manquent. Il faut supprimer les trois à la fois pour obtenir un
     nouvel essai.
  2. **Reculer l'horloge système** une fois l'essai expiré.
     `last_seen` retient le plus *récent* horodatage jamais observé
     (`max` entre les copies et l'heure courante) et l'heure utilisée pour
     le calcul est `max(horloge actuelle, last_seen)` : elle ne redescend
     jamais, donc reculer l'horloge après expiration ne rajeunit pas le
     compte à rebours.

  La réconciliation (lecture + réécriture des trois copies) est mise en
  cache pour la durée du process (`OnceLock`) : elle ne tourne qu'une fois
  par lancement, pas à chaque frame (`is_unlocked()` est appelée en continu
  depuis `dock.rs`).

  Combiner les deux (supprimer les trois copies *et* reculer l'horloge)
  reste possible — mais c'est un geste délibéré, pas un `rm` ou un `date -s`
  isolé trouvé en trente secondes de lecture du code.

- `trial::days_left()` renvoie `TRIAL_DAYS - jours_écoulés` depuis
  `first_seen` jusqu'à l'heure effective ci-dessus (peut être négatif une
  fois le délai dépassé).
- `trial::is_active()` = `days_left() > 0`.
- En test, `days_left()` renvoie toujours `0` (délai déjà passé), pour que
  les tests de verrouillage n'aient pas besoin d'un marqueur factice — la
  réconciliation sur disque n'est jamais exercée en test.

## Les deux portes d'entrée : `is_licensed` / `is_unlocked`

Définies dans `src/app/ui_windows.rs` :

- **`is_licensed()`** — `true` seulement si `LicenseState::Valid`. Pilote
  l'affichage du statut (À propos, menu Aide) : « as-t-on une vraie
  licence ? »
- **`is_unlocked()`** — `is_licensed() || trial::is_active()`. C'est **elle**
  qui pilote réellement le verrouillage des panneaux (`dock.rs` bascule sur
  `locked_panel_ui` si `!is_unlocked()`).

Cette distinction est importante : pendant la période d'essai, l'app est
*unlocked* mais pas *licensed* — le menu Aide propose donc toujours
« Activer une licence… », et le nag continue de rappeler l'inscription
(voir plus bas), même si rien n'est encore verrouillé.

## Les fenêtres, dans l'ordre où l'utilisateur les rencontre

| Fenêtre | Champ `App` | Rôle |
|---|---|---|
| `license_nag_window` | `show_license_nag` | Carte de rappel esthétique, ouverte toute seule à intervalle irrégulier. Un seul geste : activer, ou plus tard. |
| `license_gate_window` | `show_license_gate` | La vraie boîte de collage (zone de texte + Valider/Fermer). Ouverte manuellement (menu Aide, lien dans À propos, panneau verrouillé) ou depuis le nag. |
| `about_window` | `show_about` | Affiche le statut d'activation (licence active / jours d'essai restants / délai dépassé) avec lien « Activer… » si pertinent. |
| `locked_panel_ui` | (pas de flag — état des panneaux du dock) | Remplace le contenu d'un panneau réservé quand `!is_unlocked()`, avec un bouton vers `license_gate_window`. |

### Le nag (`check_license_nag`, dans `src/app/mod.rs`)

Appelé à chaque frame. Tant que `!is_licensed()` (donc y compris pendant
l'essai) :

1. Au premier appel, tire une échéance aléatoire entre 25 et 45 minutes
   (`random_nag_interval`, basé sur les nanosecondes de l'horloge système —
   pas besoin de dépendance `rand` pour un simple délai d'agacement).
2. À l'échéance, ouvre `show_license_nag` et tire la prochaine échéance.
3. Entre deux échéances, force un réveil (`request_repaint_after(60s)`) pour
   que l'échéance soit bien vérifiée même sans interaction utilisateur —
   en rendu à la demande, egui ne redessine pas de lui-même.

La carte de nag (`license_nag_window`) est volontairement **distincte** de
la boîte de collage : pas de champ de saisie, juste une accroche et deux
boutons — « Activer une licence » (ouvre `license_gate_window`) ou
« Plus tard » (referme la carte, la prochaine échéance est déjà planifiée).
Dès que `is_licensed()` devient vrai, `check_license_nag` sort immédiatement
et plus aucune carte ne s'ouvre.

### Une fois la licence active

Trois endroits se désactivent explicitement pour ne plus proposer une
action sans effet utile :

- Menu Aide : l'entrée « Activer une licence… » disparaît
  (`ui_chrome.rs`, condition `!self.is_licensed()`).
- `license_gate_window` : le bouton **Valider** est grisé
  (`ui.add_enabled(!already_licensed, ...)`), pour éviter qu'un collage
  accidentel écrase la licence en place.
- Le nag ne se déclenche plus du tout (`check_license_nag` retourne tôt).

## Activer une nouvelle version de l'outil d'émission

Le seul point de couplage entre ce dépôt et l'outil (privé) qui génère les
licences est `PUBLIC_KEY` dans `src/license.rs`. Quand cet outil existe et
produit sa vraie paire de clés Ed25519 :

1. Remplacer la constante `PUBLIC_KEY` par la vraie clé publique.
2. Republier une version (les licences déjà émises portent la version du
   payload : une incompatibilité de version est un rejet explicite, pas un
   crash silencieux).

## Résumé des chemins sur disque

```
~/.config/asm_studio/settings.conf     réglages généraux
~/.config/asm_studio/license.txt       bloc de licence collé (signature Ed25519)
~/.local/share/asm_studio/.cache_id    marqueur d'essai, copie 1/3 (first_seen:last_seen)
~/.cache/asm_studio/.sess_meta         marqueur d'essai, copie 2/3 (idem)
~/.local/state/asm_studio/.ck          marqueur d'essai, copie 3/3 (idem)
```
