# Tester le système de licence — guide interne

Complète `doc/LICENCE-SYSTEME-INTERNE.md` (qui explique le *comment ça
marche*) avec le *comment le tester manuellement*, scénario par scénario.
Document interne, pas destiné à l'utilisateur final.

## 0. Prérequis : savoir ce qu'on teste vraiment

Avant de toucher aux fichiers, vérifie que tu modifies bien ceux que lit
**le binaire que tu es en train d'exécuter** — c'est la cause la plus
fréquente d'un test « qui ne change rien après redémarrage » :

```bash
# 1. Quel binaire tourne, et depuis où ?
ps aux | grep asm_studio | grep -v grep
readlink -f /proc/<PID>/exe        # doit pointer vers target/debug/asm_studio
                                    # (et pas un binaire installé ailleurs,
                                    # p. ex. /usr/local/bin ou un paquet système)

# 2. Quel env voit-il vraiment (XDG_CONFIG_HOME / XDG_DATA_HOME / XDG_CACHE_HOME / XDG_STATE_HOME) ?
tr '\0' '\n' < /proc/<PID>/environ | grep -i XDG

# 3. Quels chemins ça donne concrètement ?
echo "${XDG_CONFIG_HOME:-$HOME/.config}/asm_studio/license.txt"
echo "${XDG_DATA_HOME:-$HOME/.local/share}/asm_studio/.cache_id"
echo "${XDG_CACHE_HOME:-$HOME/.cache}/asm_studio/.sess_meta"
echo "${XDG_STATE_HOME:-$HOME/.local/state}/asm_studio/.ck"
```

Si ces variables ne sont pas positionnées (cas normal sur la plupart des
postes), les chemins par défaut sont :

```
~/.config/asm_studio/license.txt
~/.local/share/asm_studio/.cache_id
~/.cache/asm_studio/.sess_meta
~/.local/state/asm_studio/.ck
```

Les trois derniers sont les **copies redondantes** du marqueur d'essai
(`first_seen:last_seen`, voir `LICENCE-SYSTEME-INTERNE.md`) : `trial.rs`
recompose l'état à partir de celles qui existent et réécrit celles qui
manquent, donc en supprimer une seule ne relance pas l'essai — il faut
manipuler les trois en même temps pour un test propre.

**Toujours arrêter complètement l'application (pas juste minimiser) avant
d'éditer ces fichiers, puis la relancer** : `license::load()` est lu au
démarrage, et l'état du marqueur d'essai est figé pour toute la durée du
process (`OnceLock`, lu/réconcilié une seule fois, au premier appel) —
aucun des deux n'est rechargé à chaud pendant que l'app tourne.

```bash
cat ~/.config/asm_studio/license.txt 2>/dev/null; echo "(absent si vide)"
for f in ~/.local/share/asm_studio/.cache_id ~/.cache/asm_studio/.sess_meta ~/.local/state/asm_studio/.ck; do
  echo "$f: $(cat "$f" 2>/dev/null || echo absent)"
done
```

## 1. Repartir d'un état « tout neuf »

```bash
rm -f ~/.config/asm_studio/license.txt
rm -f ~/.local/share/asm_studio/.cache_id \
      ~/.cache/asm_studio/.sess_meta \
      ~/.local/state/asm_studio/.ck
```

Raccourci pour les trois derniers (marqueurs d'essai uniquement, pas
`license.txt`) : bouton « Réinitialiser la période d'essai » dans la carte
« Test — période d'essai (local) » de la GUI de `asm-studio-license-tool`
(dépôt privé séparé) — même résultat qu'à la main, avec confirmation.

Relance : comportement attendu = période d'essai qui démarre à l'instant
(14 jours), aucun panneau verrouillé, menu Aide affiche « Activer une
licence… », statut « À propos » affiche « Avant inscription gratuite —
encore 14 jour(s) ».

## 2. Scénarios de période d'essai

| Scénario | Manipulation | Résultat attendu |
|---|---|---|
| Essai en cours | marqueurs = maintenant | Panneaux déverrouillés, menu Aide propose « Activer… », nag actif (voir §4) |
| Essai bientôt fini | marqueurs = il y a 13 jours | « À propos » affiche « encore 1 jour(s) » |
| Essai expiré | marqueurs = il y a 20 jours | Panneaux verrouillés (`locked_panel_ui`), « À propos » affiche « ✘ Délai d'inscription dépassé » |

Pour forcer une date précise, écrire **la même valeur dans les trois
copies** (sinon `reconcile` retient le plus ancien `first_seen` et le plus
récent `last_seen` trouvés parmi elles, ce qui peut donner un résultat
surprenant si une seule copie est modifiée) :

```bash
START=$(( $(date +%s) - 20*86400 ))
mkdir -p ~/.local/share/asm_studio ~/.cache/asm_studio ~/.local/state/asm_studio
for f in ~/.local/share/asm_studio/.cache_id \
         ~/.cache/asm_studio/.sess_meta \
         ~/.local/state/asm_studio/.ck; do
  echo "$START:$START" > "$f"
done
```

⚠️ Chaque fichier attend soit `first_seen:last_seen`, soit (ancien format)
un entier seul. Un contenu illisible sur une copie (texte, vide, corrompu)
est traité comme absent pour *cette copie* : tant qu'au moins une des trois
reste lisible, l'essai n'est pas réinitialisé — il faut corrompre/supprimer
les trois à la fois pour observer un essai qui repart de zéro.

⚠️ Test de non-régression du verrou anti-recul d'horloge : lance l'app une
fois avec `START` proche de maintenant (essai en cours), quitte, puis recule
l'horloge système ou réécris `last_seen` à une valeur plus petite dans une
seule des trois copies — au relancement, le compte à rebours ne doit **pas**
augmenter (la valeur la plus haute déjà vue, trouvée dans les deux autres
copies, doit l'emporter).

## 3. Scénarios de licence

### 3.a Licence absente / invalide (rapide, sans clé)

- **Absente** : pas de fichier → `LicenseState::Missing`.
- **Invalide (signature)** : coller n'importe quel texte dans la boîte de
  collage (menu Aide ▸ Activer une licence…) → message d'erreur affiché
  sous le champ, rien n'est enregistré tant que « Valider » n'a pas réussi.
- **Invalide (fichier corrompu sur disque)** :
  ```bash
  echo "n'importe quoi.pas-une-signature" > ~/.config/asm_studio/license.txt
  ```
  Relance → `LicenseState::Invalid(reason)` → panneaux verrouillés dès que
  l'essai est aussi expiré (sinon l'essai prend le relais, voir
  `is_unlocked`).

### 3.b Licence valide de bout en bout (nécessite une paire de clés de test)

`PUBLIC_KEY` dans `src/license.rs` correspond désormais à la `private.key`
de `asm-studio-license-tool` (`~/RustroverProjects/asm-studio-license-tool`,
dépôt séparé et privé) : une licence émise par l'outil est acceptée
directement, sans modifier le code. Trois façons de tester le chemin
« licence valide » (la troisième est la plus fidèle à l'usage réel) :

⚠️ **Avant tout diagnostic de « licence invalide » : vérifier que les deux
clés sont synchronisées** (`LICENCE-SYSTEME-INTERNE.md`, section
`PUBLIC_KEY`). Une paire régénérée côté outil sans recoller le tableau dans
`license.rs` donne « signature invalide » sur *toutes* les licences, quels
que soient la version, le binaire et la date — c'est le piège le plus
coûteux du système, car le message n'oriente pas vers la cause. Penser aussi
à **recompiler ASM Studio** après avoir mis la constante à jour — **les deux
profils**. `cargo build` et `cargo build --release` ont des caches séparés et
la clé est figée dans chaque binaire : ne recompiler que le debug donne
« la licence marche en debug, pas en release » à code source identique. C'est
la panne qui a motivé le garde-fou `FORBIDDEN_KEYS` et le contrôle ajouté à
`install/package.sh` (voir `LICENCE-SYSTEME-INTERNE.md`).

**a) Automatisé (déjà fait, aucune manip nécessaire)** — les tests unitaires
de `src/license.rs` signent avec une paire de test
(`SigningKey::from_bytes(&[7u8;32])`) et vérifient tout le chemin
(signature correcte, altérée, version incompatible, JSON corrompu). Lancer :
```bash
cargo test license::tests
```

**b) Manuel, dans l'appli elle-même** — passe par (c) : la paire de test de
`mod tests` (`SigningKey::from_bytes(&[7u8; 32])`) fait partie des
`FORBIDDEN_KEYS` de `src/license.rs` et **ne compile plus** si elle est
collée dans `PUBLIC_KEY` :

```
error[E0080]: evaluation panicked: PUBLIC_KEY est une clé de test dont la
clé privée est publique : recollez la vraie clé publique
d'asm-studio-license-tool avant de compiler
```

C'est voulu : sa clé privée est en clair dans ce dépôt public, un binaire qui
l'embarquerait accepterait n'importe quelle licence forgée — et refuserait
toutes les vraies. L'oubli de restauration s'était déjà produit ; le garde-fou
le transforme en erreur de compilation plutôt qu'en release inutilisable. Pour
un test manuel, génère une paire jetable avec `keygen` (option (c)) : elle
n'est pas dans la liste, et elle exerce exactement le même chemin.

**c) Avec le vrai outil d'émission** — plus fidèle, et seule façon de
tester une licence à **durée limitée** (`expires_at`, voir
`LICENCE-SYSTEME-INTERNE.md`) sans écrire de test Rust ad hoc :

```bash
cd /tmp && mkdir -p license-smoketest && cd license-smoketest
BIN=~/RustroverProjects/asm-studio-license-tool/target/debug/asm-studio-license-tool
"$BIN" keygen --out private.key   # imprime aussi le tableau PUBLIC_KEY
"$BIN" issue --name "Test" --email t@example.com \
             --version "$(grep '^version' ~/RustroverProjects/asm_studio/Cargo.toml | head -1 | cut -d'"' -f2)" \
             --release-sha3-512 aa \
             --build "$(git rev-parse --short HEAD)" \
             --valid-days 30 \
             --key private.key
```

`--build` est optionnel (traçabilité uniquement, jamais vérifié côté
client — voir `LICENCE-SYSTEME-INTERNE.md`) : en usage réel, c'est la valeur
copiée par l'utilisateur via le bouton 📋 à côté de « Build » dans « À
propos », pas forcément le `HEAD` local de ce dépôt.

Colle le `PUBLIC_KEY` imprimé par `keygen` dans `src/license.rs` (comme à
l'étape 2 ci-dessus), recompile, puis colle la licence imprimée par `issue`
dans la boîte de collage de l'appli. Pour tester une licence **déjà
expirée** : rajoute `--issued-at 2020-01-01` à la commande `issue`
ci-dessus (avec le même `--valid-days 30`) → l'appli doit refuser la
licence avec `licence expirée le 2020-01-31`, pas planter, et retomber sur
l'essai s'il est encore actif (`is_unlocked`) ou sur le verrouillage sinon.
Là encore, **restaurer `PUBLIC_KEY` avant de committer**.

## 4. Le rappel périodique (nag)

Le nag s'ouvre tout seul entre 25 et 45 minutes (`random_nag_interval`,
`src/app/mod.rs`) tant qu'aucune licence n'est active — trop long pour un
test manuel interactif. Pour le déclencher vite :

- **Option simple, sans toucher au code** : les tests automatisés couvrent
  déjà toute la logique de planification sans attendre (`cargo test
  first_check_schedules_a_future_nag_without_opening_it`,
  `nag_opens_once_the_deadline_is_reached_and_schedules_the_next_one`,
  `a_valid_license_never_triggers_the_nag`).
- **Pour le *voir* dans l'appli** : réduis temporairement `MIN_SECS`/
  `SPAN_SECS` dans `random_nag_interval` (`src/app/mod.rs`) à quelques
  secondes, ou force directement `app.show_license_nag = true;` juste après
  `app.license = crate::license::load();` dans `App::new()`. Dans les deux
  cas, **c'est un changement temporaire, à annuler avant de committer** —
  vérifie avec `git diff src/app/mod.rs` avant de pousser quoi que ce soit.

Checklist visuelle une fois la carte ouverte :
- icône ✨, titre « ASM Studio vous plaît ? », mention du nombre de jours
  d'essai restants si l'essai est encore actif ;
- bouton « Activer une licence » → ouvre la boîte de collage, referme la
  carte ;
- bouton « Plus tard » → referme juste la carte, une nouvelle échéance a
  déjà été programmée (revérifiable via `cargo test nag_opens_once`).

## 5. Le blocage à la fermeture (nouveau)

Tant qu'aucune licence n'est active, fermer l'appli (croix de la fenêtre,
**ou** Fichier ▸ Quitter — les deux passent par le même événement de
fermeture) doit :

1. Annuler la fermeture (`ViewportCommand::CancelClose`) ;
2. Ouvrir la carte de rappel avec, cette fois, un bouton **« Quitter quand
   même »** à la place de « Plus tard », et la mention « Vous êtes sur le
   point de quitter ASM Studio. » ;
3. Cliquer « Quitter quand même » ferme réellement l'appli
   (`ViewportCommand::Close`) ; cliquer « Activer une licence » ouvre la
   boîte de collage à la place et n'interrompt plus la session.

Test manuel : lance l'appli sans licence, clique sur la croix de la
fenêtre → la carte doit apparaître au lieu de fermer l'appli. Couverture
automatisée équivalente (sans avoir besoin d'une vraie fenêtre) :
```bash
cargo test -- unlicensed_close_is_cancelled_and_opens_the_nag \
             licensed_close_is_never_intercepted \
             a_frame_without_close_event_touches_nothing \
             confirmed_quit_is_never_intercepted_again
```
Une fois licencié (§3.b), refais le test : la fermeture doit se dérouler
normalement, sans aucune carte.

⚠️ Piège déjà rencontré : envoyer soi-même `ViewportCommand::Close` (bouton
« Quitter quand même ») redéclenche `close_requested()` à la frame suivante,
et sans précaution `check_close_request` l'intercepte une seconde fois —
la carte se rouvre en boucle et le bouton semble ne rien faire. D'où le
champ `quit_confirmed` (`src/app/mod.rs`), posé par ce bouton, qui fait
sortir `check_close_request` tôt pour de bon (voir `confirmed_quit_is_never_intercepted_again`).

## 6. Checklist de non-régression rapide

```bash
cargo build && cargo test
```

Puis, à la main dans l'appli (sans licence, essai frais) :

- [ ] Menu Aide affiche « Activer une licence… »
- [ ] « À propos » affiche le compte à rebours d'essai
- [ ] Panneaux (désassemblage / registres-flags / timeline) déverrouillés
- [ ] Fermer la fenêtre ouvre la carte de rappel (pas de fermeture directe)
- [ ] « Quitter quand même » ferme vraiment l'appli

Après activation d'une licence de test (§3.b) :

- [ ] Menu Aide n'affiche plus « Activer une licence… »
- [ ] Bouton « Valider » grisé si on rouvre la boîte de collage
- [ ] Plus aucun nag, périodique ou à la fermeture
- [ ] Fermer la fenêtre ferme l'appli normalement, sans interception
