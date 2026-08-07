# Changelog technique — système de licence (usage interne)

Pendant du `CHANGELOG.md` public, mais pour ce qui concerne le système de
licence : ici tous les détails techniques réels, sans reformulation
discrète. Ne pas publier tel quel — voir aussi `LICENCE-SYSTEME-INTERNE.md`
(comment ça marche) et `TEST-LICENCE.md` (comment le tester).

## [0.4.0-beta.2] - 2026-08-06 (suite 3)

### Corrigé — `version+build` collé dans le champ « Version » de l'outil

Deuxième cause de « licence invalide », indépendante de la précédente et
introduite par le bouton 📋 fusionné : la chaîne copiée depuis « À propos »
est un `{version}+{build}` d'un seul tenant, et le réflexe est de la coller
dans le champ **« Version ASM Studio »** — pas dans le champ de collage
séparé ajouté juste au-dessus. L'outil signait alors une licence dont la
version valait `0.4.0-beta.2+d4022ad`, rejetée côté client par le contrôle
de version (message exact, mais qui ne dit pas quoi corriger).

Le champ de collage dédié était donc un pansement mal placé : il ajoutait
une saisie de plus sans empêcher l'erreur qu'il était censé éviter.

- **`lib.rs`** : `normalize_version_build(version, build) -> (String, String)`
  (bâtie sur `split_version_build`) — si la version contient un `+`, elle est
  répartie sur les deux champs. Un `build` déjà renseigné explicitement n'est
  jamais écrasé. Tests :
  `normalize_splits_a_version_field_containing_the_build`,
  `normalize_never_overwrites_an_explicit_build`,
  `normalize_leaves_a_plain_version_untouched`.
- **GUI** : champ de collage séparé **supprimé** (une saisie de moins). Le
  champ « Version ASM Studio » se normalise à chaque frappe, et `issue()`
  renormalise avant de signer (filet de sécurité). `resolved_build`, devenue
  redondante, est supprimée.
- **CLI** : `--version "0.4.0-beta.2+d4022ad"` est normalisé de la même
  façon, au lieu d'être signé tel quel.

Vérifié de bout en bout : licence réémise avec `--version` contenant le
build → payload `"version":"0.4.0-beta.2"`, `"build":"d4022ad"`, signature
validée hors de l'app avec la `PUBLIC_KEY` synchronisée.

## [0.4.0-beta.2] - 2026-08-06 (suite 2)

### Corrigé — `PUBLIC_KEY` désynchronisée de la clé privée de l'outil

Symptôme : une licence fraîchement émise (60 jours, bonne version, bon
binaire) refusée par ASM Studio. Cause réelle : `license.rs::PUBLIC_KEY`
valait `[0x8E, 0x07, 0xE9, …]` alors que la `private.key` chargée dans
`asm-studio-license-tool` dérive `[0xD9, 0xF0, 0xC2, …]` — une paire
régénérée côté outil sans recoller le tableau côté client. La vérification
Ed25519 échoue alors systématiquement, avec le même message que pour une
licence trafiquée (« signature invalide ») : rien n'oriente vers la cause,
et tout le reste (version, expiration, hash) semble correct.

- `PUBLIC_KEY` mise à jour avec la clé publique réellement dérivée de la
  `private.key` en place, et sa doc réécrite (ce n'était plus un
  « placeholder » depuis un moment, mais le commentaire le disait encore —
  ce qui rendait la désynchronisation d'autant plus difficile à voir).
  Licence de contrôle réémise et signature revérifiée hors de l'app avant
  de conclure.
- `LICENCE-SYSTEME-INTERNE.md` : section `PUBLIC_KEY` réécrite avec la
  commande de vérification (dériver la clé publique depuis `private.key` et
  comparer les deux tableaux) et le rappel « recompiler après mise à jour ».
- `TEST-LICENCE.md` §3.b : ne parle plus de placeholder, et met ce contrôle
  en tête du diagnostic « licence invalide ».
- **CLI de l'outil** : `--build ""` (typiquement une substitution shell qui
  a échoué) était sérialisé en `"build":""` ; désormais normalisé en
  « non communiqué », comme l'absence d'option.

### Changé — GUI de l'outil : plus de défilement global

La fenêtre empilait cinq cartes dans un `ScrollArea` vertical : tout le
contenu défilait, y compris le formulaire d'émission.

- Disposition en **deux colonnes** (`ui.columns(2, …)`) qui remplissent la
  fenêtre : à gauche le formulaire d'émission, à droite la licence émise (si
  présente) puis l'historique. Nouvelle aide `card_filling` — carte qui
  s'étire jusqu'en bas de la colonne, pour que la fenêtre soit remplie de
  haut en bas au lieu de laisser un vide.
- **Barre de titre** (`TopBottomPanel::top`) : titre, statut de la clé, et
  deux boutons ouvrant les actions rares en **fenêtres dédiées** (« Clé de
  signature… », « Outils de test… ») au lieu de les empiler dans le flux.
  La fenêtre de clé s'ouvre d'elle-même au démarrage s'il n'y a pas de clé.
- **Bandeau d'erreur** en pied de fenêtre (`TopBottomPanel::bottom`), avec
  une croix pour le masquer : il n'occupe de place que lorsqu'il y a
  quelque chose à dire et ne décale plus le formulaire.
- Le hash SHA3-512 affiché est tronqué à 32 caractères (valeur complète en
  infobulle) : 128 caractères hex débordaient de la colonne.
- Fenêtre par défaut 1020×700 (min 880×600) ; `collapsible_card`, devenue
  inutile, est supprimée. Seul l'historique conserve un défilement interne
  — inévitable, il est de taille non bornée.

## [0.4.0-beta.2] - 2026-08-06 (suite)

### Ajouté (côté `asm-studio-license-tool`, dépôt privé) — bouton de réinitialisation de l'essai

`TEST-LICENCE.md` §1 documentait jusqu'ici trois commandes `rm` manuelles
(une par copie XDG) pour retester le scénario « premier lancement ».
Ajout d'un bouton dans la GUI de l'outil d'émission, carte « Test —
période d'essai (local) », qui fait la même chose en un clic (avec
confirmation, `rfd::MessageDialog`, même pattern que l'écrasement d'une clé
existante).

- **`lib.rs`** : `trial_marker_paths()` (privée) duplique à l'identique
  `asm_studio/src/app/paths.rs::trial_marker_paths` — pas de dépendance
  croisée entre les deux dépôts (l'un est un binaire sans `lib.rs`, l'autre
  privé). `reset_trial_markers() -> io::Result<Vec<PathBuf>>` (publique)
  supprime les marqueurs présents, idempotent (absent = pas une erreur).
  Logique de suppression extraite en `remove_existing` pour être testable
  sans dépendre des vraies variables XDG de l'environnement de test.
  Tests : `trial_marker_paths_use_the_expected_leaf_names`,
  `remove_existing_deletes_present_files_and_skips_missing_ones`.
- **`bin/gui.rs`** : `App::reset_trial` (confirmation puis appel à
  `reset_trial_markers`, résultat affiché dans `trial_reset_status`).
  N'agit que sur la machine locale, jamais sur une licence.

Rappel : si les trois chemins XDG venaient à changer côté `asm_studio`
(`app/paths.rs`), il faudra répercuter le même changement dans
`trial_marker_paths` de l'outil — duplication assumée, comme
`days_from_civil` pour les dates.

## [0.4.0-beta.2] - 2026-08-06

### Changé — un seul bouton copier pour version+build

Avant : deux boutons 📋 séparés dans « À propos » (un pour « Version », un
pour « Build »), à copier-coller un par un dans l'outil d'émission.
Fusionnés en un seul, sur la ligne « Build », qui copie
`{version}+{build}` (syntaxe des métadonnées de build semver, ex.
`0.4.0-beta.2+a1b2c3d`) — la ligne « Version » n'a plus de bouton, juste la
valeur affichée.

- **`asm_studio/src/license.rs`** : nouvelle fonction
  `version_build_tag() -> String` (`format!("{}+{}", CARGO_PKG_VERSION,
  GIT_HASH)`), testée (`version_build_tag_concatenates_both_with_a_plus`).
- **`asm_studio/src/app/ui_windows.rs`** : le bouton sur la ligne « Build »
  appelle `crate::license::version_build_tag()` au lieu de copier
  `GIT_HASH` seul ; celui de la ligne « Version » est retiré.
- **`asm-studio-license-tool`** (dépôt séparé, privé) : nouvelle fonction
  `split_version_build(&str) -> Option<(String, String)>` dans `lib.rs`
  (inverse de `version_build_tag`, testée :
  `split_version_build_separates_both_parts`,
  `split_version_build_tolerates_surrounding_whitespace`,
  `split_version_build_rejects_malformed_input`). Côté GUI (`bin/gui.rs`),
  nouveau champ « Coller version+build » au-dessus du formulaire : sur
  modification, `App::apply_version_build_paste` répartit automatiquement
  le contenu collé dans les champs « Version ASM Studio » et « Build »
  existants (qui restent éditables manuellement si besoin — le paste ne
  fait que les pré-remplir). CLI (`main.rs`) inchangée : `--version` et
  `--build` restent deux options séparées, le paste unique est une
  commodité GUI uniquement.

## [0.4.0-beta.2] - 2026-08-05 (suite 3)

### Ajouté — traçabilité par numéro de build (`build`)

Objectif : pouvoir relier a posteriori une licence émise au build exact
(hash git court, `env!("GIT_HASH")`) que le demandeur utilisait au moment de
la demande — utile en support (« cette licence a-t-elle bien été émise pour
ce qu'il tourne vraiment ? ») sans jamais imposer de verrou technique sur ce
numéro (une recompilation ou un correctif sans bump de version change le
hash git légitimement).

- **`asm_studio/src/app/ui_windows.rs`** (`about_window`) : le champ
  « Build » gagne un bouton 📋, comme « Version » déjà présent — c'est le
  canal par lequel l'utilisateur communique son build exact à l'auteur avant
  émission.
- **`asm_studio/src/license.rs`** : `LicensePayload` gagne
  `build: Option<String>` (`#[serde(default)]`, licences déjà émises sans ce
  champ toujours lisibles). Même statut que `release_sha3_512` : signé,
  jamais recalculé ni comparé côté client, `#[allow(dead_code)]` (pas encore
  affiché dans l'UI, disponible pour un usage futur).
- **`asm-studio-license-tool`** (dépôt séparé, privé) : `LicensePayload` et
  `IssuedRecord` gagnent le même champ. CLI : `--build <hash>` optionnel sur
  `issue`. GUI : champ « Build (« À propos » du demandeur) », inclus dans
  l'historique affiché (`v{version} · {date} · build {hash}`).

Tests ajoutés : `license::tests::license_without_build_field_is_still_readable`,
`license_with_build_field_carries_it_through` côté `asm_studio` ;
`build_field_is_included_when_present_and_omitted_when_absent` côté
`asm-studio-license-tool`.

## [0.4.0-beta.2] - 2026-08-05 (suite 2)

### Ajouté — licences à durée limitée (`expires_at`)

Jusqu'ici, une licence signée était forcément perpétuelle (verrouillée
uniquement à une version). Ajout d'un champ optionnel `expires_at`
(AAAA-MM-JJ) au payload, des deux côtés du couplage :

- **`asm-studio-license-tool`** (dépôt séparé, privé) : `LicensePayload`
  et `IssuedRecord` gagnent `expires_at: Option<String>`. On ne saisit
  jamais une date d'expiration à la main — seulement une **durée en jours**
  (`--valid-days <N>` en CLI, champ « Durée de validité (jours) » dans le
  GUI, avec aperçu live de la date calculée). `lib.rs::expiry_date_from`
  fait le calcul (`issued_at` + N jours), via l'inverse
  (`days_from_civil`) de l'algorithme déjà utilisé par `today_utc_date`
  (Howard Hinnant, domaine public). Absence de `--valid-days`/champ vide ou
  "0" = licence perpétuelle, comme avant.
- **`asm_studio/src/license.rs`** : `LicensePayload` gagne le même champ
  (`#[serde(default)]` : les licences déjà émises sans ce champ restent
  lisibles, traitées comme perpétuelles). `verify_with_key` rejette avec
  `"licence expirée le {date}"` si l'heure de confiance a dépassé la fin de
  la journée d'expiration (borne exclusive, calculée par
  `days_from_civil` + 1 jour × 86400 — dupliqué depuis l'outil d'émission,
  pas de dépendance à une crate de dates côté client non plus).

**Point important : l'heure de confiance, pas l'horloge système brute.**
Vérifier l'expiration contre `SystemTime::now()` directement aurait
réintroduit exactement le contournement par recul d'horloge qu'on vient de
fermer sur `trial.rs` (voir l'entrée précédente) — sauf que cette fois sur
une licence payante, pas juste un essai gratuit. `crate::trial::trusted_now`
a donc été extraite en fonction `pub(crate)` séparée d'`is_active`/
`days_left`, réutilisant le même `TRIAL_ANCHOR` (`OnceLock`) et les mêmes
marqueurs redondants et auto-réparants déjà en place : `license::verify`
l'appelle directement, sans dupliquer de fichiers ni de logique de
réconciliation. `trusted_now()` renvoie `now_unix()` sans persistance en
test (`cfg!(test)`), comme le reste du module.

L'affichage « À propos » (`ui_windows.rs::about_window`) montre désormais
« ✔ Activée — {nom} (valable jusqu'au {date}) » quand `expires_at` est
présent, rien de plus si la licence est perpétuelle.

Tests ajoutés : `license::tests::license_valid_until_a_future_date_is_accepted`,
`expired_license_is_rejected_with_the_date_named`,
`license_is_still_valid_on_its_last_day_of_expiry`,
`malformed_expiry_date_is_rejected_without_panicking`,
`license_without_expiry_field_is_perpetual`, `days_from_civil_matches_known_dates`
côté `asm_studio` ; `expiry_date_from_adds_whole_days`,
`expiry_date_from_crosses_month_and_year_boundaries`,
`expiry_date_from_rejects_malformed_issued_at`,
`days_from_civil_and_civil_from_days_round_trip` côté
`asm-studio-license-tool`.

## [0.4.0-beta.2] - 2026-08-05 (suite)

### Durci — les deux contournements du délai d'essai réalisables sans recompiler

Constat : `trial.rs` ne stockait qu'**un seul** marqueur en clair
(`~/.local/share/asm_studio/.cache_id`, un entier Unix lisible tel quel), et
`days_remaining` ne se protégeait pas contre un recul de l'horloge système.
Deux contournements triviaux, sans lire une ligne de code Rust ni
recompiler :

1. `rm ~/.local/share/asm_studio/.cache_id` puis relancer → nouvel essai
   complet de 14 jours, à volonté.
2. Une fois l'essai expiré, reculer l'horloge système (`date -s` ou
   `faketime`) avant de relancer → `now - start` redevient petit,
   `days_remaining` redonne un solde positif, sans toucher à aucun fichier.

Les deux sont corrigés dans `src/trial.rs` (voir la doc de module et
`LICENCE-SYSTEME-INTERNE.md` pour le détail) :

- **Marqueurs redondants et auto-réparants.** Trois copies
  (`crate::app::paths::trial_marker_paths`), sur `$XDG_DATA_HOME`,
  `$XDG_CACHE_HOME`, `$XDG_STATE_HOME`, sous trois noms différents
  (`.cache_id`, `.sess_meta`, `.ck`). `reconcile_from_disk` lit les trois,
  retient le `first_seen` le plus ancien trouvé (une copie survivante avec
  la vraie date l'emporte sur une copie manquante ou trafiquée) et réécrit
  celles qui manquent. Supprimer une seule copie ne fait plus rien ; il faut
  les trois à la fois.
- **Haute marque anti-recul d'horloge.** Chaque copie stocke désormais
  `first_seen:last_seen` (l'ancien format à un seul entier reste lu, pour
  les deux champs). `last_seen` = le plus grand horodatage jamais observé
  (`max` entre les copies et l'heure courante). L'heure utilisée pour le
  calcul du solde est `max(horloge actuelle, last_seen)` : elle ne
  redescend jamais, donc reculer l'horloge après expiration ne rajeunit
  plus le compte à rebours.
- **Mise en cache par process (`OnceLock`).** La réconciliation (lecture +
  réécriture des trois copies) ne tourne qu'une fois par lancement, pas à
  chaque frame — `is_unlocked()` est appelée en continu depuis `dock.rs`
  (`Panel::Disasm/Flags/Registers/Timeline`), ça aurait sinon fait trois
  écritures disque par frame.

Ce qui reste possible, en connaissance de cause (même philosophie que le
reste du système, cf. doc de module) : supprimer les **trois** copies **et**
reculer l'horloge en même temps, ou recompiler en désactivant le contrôle.
C'est un geste délibéré et technique, pas un `rm` ou un `date -s` isolé
trouvé en trente secondes de lecture du code — l'objectif énoncé depuis le
début (décourager l'effacement/réglage distrait, pas stopper un
contournement voulu) est inchangé, seul le curseur du « trivial » a bougé.

Tests ajoutés dans `src/trial.rs` (fonctions pures `reconcile_values` /
`parse_marker`, sans toucher au disque) :
`deleting_one_of_three_copies_does_not_reset_the_trial`,
`a_forged_copy_claiming_a_fresh_start_does_not_win_over_survivors`,
`rolling_back_the_clock_does_not_lower_the_high_water_mark`,
`legacy_single_integer_marker_is_still_readable`, entre autres.

### Ajouté

- **Carte de rappel (« nag ») dédiée** — `App::license_nag_window`
  (`src/app/ui_windows.rs`), pilotée par le champ `show_license_nag`
  (`src/app/mod.rs`). Distincte de `license_gate_window` (la boîte de
  collage) : ici pas de champ de saisie, juste une accroche (médaillon ✨,

- **Carte de rappel (« nag ») dédiée** — `App::license_nag_window`
  (`src/app/ui_windows.rs`), pilotée par le champ `show_license_nag`
  (`src/app/mod.rs`). Distincte de `license_gate_window` (la boîte de
  collage) : ici pas de champ de saisie, juste une accroche (médaillon ✨,
  titre, argumentaire, compte à rebours d'essai si actif) et deux boutons.
  Remplace l'ancien comportement qui rouvrait directement la boîte de
  collage technique à chaque rappel périodique.
- **Blocage de fermeture tant que non licencié** —
  `App::check_close_request` (`src/app/mod.rs`). Intercepte
  `ctx.input(|i| i.viewport().close_requested())` (déclenché par la croix
  de la fenêtre OU par `Fichier ▸ Quitter`, qui envoie le même
  `ViewportCommand::Close` — voir `ui_chrome.rs:452`), envoie
  `ViewportCommand::CancelClose`, ouvre `show_license_nag` avec
  `exit_pending = true`. Le bouton secondaire de la carte devient alors
  « Quitter quand même » (au lieu de « Plus tard »), qui envoie lui-même
  `ViewportCommand::Close` pour finir de quitter.

### Corrigé (deux bugs trouvés en test manuel réel, absents des tests headless)

**Bug 1 — boucle d'auto-interception.** Premier jet : le bouton « Quitter
quand même » appelait juste `ctx.send_viewport_cmd(ViewportCommand::Close)`.
Mais `check_close_request` tourne à *chaque* frame et n'avait aucun moyen de
distinguer « nouvelle tentative de fermeture » de « c'est moi qui viens de
renvoyer Close » : à la frame suivante, `close_requested()` redevenait vrai
(propre `Close` qui revient), et `check_close_request` l'annulait à nouveau
avec `CancelClose` — la carte se rouvrait en boucle silencieuse, le bouton
semblait ne rien faire.

Correctif : champ `quit_confirmed: bool` (`src/app/mod.rs`), posé par le
bouton avant d'envoyer `Close`. `check_close_request` sort tôt (sans jamais
renvoyer `CancelClose`) dès que ce flag est vrai. Test de régression :
`confirmed_quit_is_never_intercepted_again`.

**Bug 2 — `Close` ne suffit pas seul en rendu à la demande.** Même avec le
correctif ci-dessus, quitter restait silencieusement inopérant. Diagnostic
en lisant les sources vendues des crates :

- `egui-winit-0.33.3/src/lib.rs`, `process_viewport_commands` :
  ```rust
  ViewportCommand::Close => {
      info.events.push(egui::ViewportEvent::Close);
  }
  ```
  `ViewportCommand::Close` ne fait que *programmer* un événement
  `ViewportEvent::Close` pour la frame **suivante** (celle-ci sera relue par
  `close_requested()` au prochain tour). Rien n'est traité dans la frame
  courante.
- `eframe-0.33.3/src/native/epi_integration.rs`, `Integration::update` :
  ```rust
  let close_requested = raw_input.viewport().close_requested();
  // … app.update() tourne …
  if is_root_viewport && close_requested {
      let canceled = full_output.viewport_output[&ViewportId::ROOT]
          .commands.contains(&egui::ViewportCommand::CancelClose);
      if !canceled { self.close = true; }
  }
  ```
  `self.close` (interne à eframe, lu via `should_close()`) n'est mis à
  `true` qu'à la frame où `close_requested()` était déjà vrai *en entrée* —
  donc à la frame *après* celle où on a envoyé `Close`.
- `eframe-0.33.3/src/native/glow_integration.rs`, autour de la ligne 830,
  sur le `WindowEvent::CloseRequested` natif (le vrai clic sur la croix
  OS) — eframe lui-même documente le problème dans son propre code :
  ```rust
  // We may need to repaint both us and our parent to close the window,
  // and perhaps twice (once to notice the close-event, once again to
  // enforce it). `request_repaint_of` does a double-repaint though:
  self.integration.egui_ctx.request_repaint_of(viewport_id);
  ```
  Autrement dit : même le chemin natif (clic OS) a besoin d'un repaint
  **explicite** pour enchaîner les deux frames nécessaires (une pour
  *constater* la demande de fermeture, une pour *l'appliquer* une fois que
  `should_close()` est vrai). En rendu à la demande (`request_repaint_after`
  utilisé ailleurs dans ce projet, ex. `check_license_nag`), sans repaint
  demandé, cette deuxième frame n'arrive jamais toute seule — l'appli reste
  ouverte indéfiniment après un clic sur « Quitter quand même ».

Correctif : deux appels à `ctx.request_repaint()` —
1. Dans le bouton lui-même (`ui_windows.rs`), juste après
   `send_viewport_cmd(Close)`, pour déclencher la première frame suivante.
2. Dans `check_close_request`, tant que `quit_confirmed` est vrai : redemande
   un repaint immédiat à *chaque* frame, jusqu'à ce qu'eframe ait
   effectivement fermé l'appli (peu coûteux : ça ne dure qu'une poignée de
   frames juste avant la sortie du process).

Test de régression : `quit_confirmed_keeps_requesting_a_repaint_until_eframe_closes`
(vérifie `repaint_delay == Duration::ZERO`).

### Changé

- **Menu Aide** (`ui_chrome.rs` ~515) : l'entrée « Activer une licence… »
  n'est rendue que si `!self.is_licensed()`.
- **Boîte de collage** (`ui_windows.rs`, `license_gate_window`) : le bouton
  « Valider » passe par `ui.add_enabled(!self.is_licensed(), …)` — grisé une
  fois une licence déjà active, pour éviter un remplacement accidentel par
  un collage erroné.

### Champs `App` ajoutés (`src/app/mod.rs`)

| Champ | Rôle |
|---|---|
| `show_license_nag: bool` | Carte de rappel ouverte (périodique ou fermeture) |
| `exit_pending: bool` | La carte est ouverte à cause d'une tentative de fermeture — change le libellé du bouton secondaire |
| `quit_confirmed: bool` | L'utilisateur a cliqué « Quitter quand même » — désarme définitivement l'interception et force les repaints jusqu'à la fermeture réelle |

### Pourquoi rien de tout ça n'est dans le changelog public

Le changelog public (`CHANGELOG.md`) documente des fonctionnalités pour les
utilisateurs, pas la mécanique d'un système de licence ni les détails d'un
bug d'intégration eframe/winit — d'où ce fichier séparé, à usage interne.
