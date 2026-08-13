<p align="center">
  <img src="assets/icon.png" width="96" alt="Icône ASM Studio">
</p>

<h1 align="center">ASM Studio</h1>

<p align="center"><strong>Apprenez NASM x86-64 en observant un véritable processus Linux.</strong></p>

<p align="center">
  <a href="https://github.com/fredza/asm-studio/releases"><img src="https://img.shields.io/badge/version-0.4.7-2f81c1?style=flat-square" alt="Version 0.4.7"></a>
  <img src="https://img.shields.io/badge/plateforme-Linux%20x86__64-f6a434?style=flat-square" alt="Linux x86-64">
  <a href="LICENSE.md"><img src="https://img.shields.io/badge/licence-ASFL%201.0-6baf68?style=flat-square" alt="Licence ASFL 1.0"></a>
</p>

<p align="center"><a href="README.md">English</a> · <strong>Français</strong> · <a href="README.es.md">Español</a></p>

ASM Studio assemble votre source avec `nasm`, la lie avec `ld`, puis l'exécute
sur le vrai noyau Linux. Avancez instruction par instruction grâce à `ptrace` et
observez registres, drapeaux, pile et mémoire dans leur état réel — sans
simulateur de processeur entre les deux.

![ASM Studio — débogueur, flags et leçon guidée](assets/captures/asm_studio-preview.png)

| Examiner les registres vectoriels | Comprendre une instruction |
|---|---|
| ![Panneau des registres SSE et x87](assets/captures/debugger-sse.png) | ![Microscope d'instruction](assets/captures/instruction-microscope.png) |

---

## Sommaire

- [Fonctionnalités](#fonctionnalités)
- [Installation](#installation)
- [Démarrage rapide](#démarrage-rapide)
- [Raccourcis clavier](#raccourcis-clavier)
- [Compiler depuis les sources](#compiler-depuis-les-sources)
- [Dépendances](#dépendances)
- [Structure du projet](#structure-du-projet)
- [Licence](#licence)

---

## Fonctionnalités

- **Débogueur réel** — exécution pas à pas d'un binaire NASM via `ptrace`
  (registres, `SETREGS`, lecture/écriture de `/proc/pid/mem`), pas une machine
  virtuelle simulée.
- **Points d'arrêt, conditionnels au besoin** — un clic dans la gouttière (ou
  `Ctrl+F8`) marque une ligne, `Continuer` (`F9`) y mène d'une traite. Un clic
  droit (ou `Ctrl+Maj+F8`) y attache une condition — `RCX == 0`, `RAX > 0x100`,
  `ZF == 1` — et l'exécution ne s'arrête que si elle est vraie : de quoi
  atteindre le quatre millième tour d'une boucle sans quatre mille
  « Continuer ». `Par-dessus` (`Maj+F10`) franchit un `call` d'un bloc. Chaque
  instruction reste dans la timeline.
- **Inspection au survol** — passer la souris sur un mot du code montre ce
  qu'il vaut à cet instant : un registre en hexa, en décimal, en signé, en
  caractère et avec les octets qu'il pointe ; un drapeau avec son état ; un
  label avec sa ligne et son adresse ; un nombre dans les trois bases.
- **Vraie console** — ce que le programme écrit sur sa sortie standard arrive
  dans l'IDE, et l'on peut lui envoyer de l'entrée : un programme suspendu sur
  un `read` vous attend au lieu de figer l'interface.
- **Trois modes d'affichage** — *Apprentissage* (le parcours et l'essentiel),
  *Éditeur seul* (explorateur + éditeur, sans distraction) et *Complet*
  (désassemblage, vue mémoire, vidage hexa, pile d'appels, appels système).
- **Disposition ancrable** — chaque panneau se glisse, s'empile ou se détache en
  fenêtre flottante (`egui_dock`), à la manière d'un IDE classique.
- **Parcours guidé** — un tutoriel en cinq niveaux (Débutant, Intermédiaire,
  Avancé, Expert, et Windows/PE64) qui introduit progressivement registres,
  tailles, mémoire, drapeaux, pile et appels système. Chaque leçon charge son
  programme, ouvre les panneaux qu'elle explique et propose les exercices qui
  l'entraînent.
- **Exercices auto-corrigés** — trente-six exercices avec vérification
  automatique du résultat, chacun relié à la leçon qui l'explique.
- **Mode « CPU vivant »** — animation des registres et drapeaux modifiés à
  chaque pas, badges d'activité PUSH/POP sur la pile.
- **Prédiction** — devinez l'effet de la prochaine instruction avant de
  l'exécuter, pour ancrer la compréhension.
- **Registres SSE / x87** — les seize registres XMM et la pile x87, lus comme
  l'instruction les lit : deux `double`, quatre `float`, quatre entiers de 32
  bits, seize octets, ou l'hexadécimal brut. Avec le mode d'arrondi de MXCSR et
  les exceptions levées. Écrire `addsd xmm0, xmm1` et lire `5` ne demande plus
  d'acte de foi.
- **Cible Windows (PE64)** — le même source s'assemble en véritable `.exe`
  Windows (`nasm -f win64` et un lieur intégré : ni `lld-link`, ni SDK
  Microsoft à installer). Un `extern ExitProcess` devient une vraie table
  d'import. Le binaire se désassemble, s'ouvre dans le panneau FORMAT et, si
  `wine` est installé, « Lancer » l'exécute pour de bon : sa sortie arrive dans
  la console habituelle. Ce qui reste hors de portée, c'est le pas-à-pas — le
  débogueur parle `ptrace` et suit les adresses de l'image qu'il vient
  d'écrire, que le chargeur de Wine ne conserve pas.
- **Explorateur de format binaire** — en-tête, sections, droits, point
  d'entrée, imports et symboles globaux, présentés de la même façon pour ELF et
  pour PE. Ce qu'une section coûte en mémoire et sur le disque, et pourquoi
  `.bss` ne pèse rien.
- **Projets multi-fichiers** — un `asmstudio.toml` rassemble le point d'entrée,
  les sources NASM et les répertoires `%include` ; sous Linux, ASM Studio
  assemble chaque source puis les lie ensemble.
- **Calculatrice intégrée** — hexadécimal par défaut, vue bit à bit et
  opérations arithmétiques/logiques. Elle lit aussi le texte ASCII comme les
  octets d'un registre : `Hi` vaut `0x4869`, sur huit caractères au plus, avec
  les échappements `\n` et `\xNN`. De quoi voir une chaîne dans toutes les
  bases ou lui appliquer directement un masque de bits.
- **Diagnostic d'erreurs** — messages d'erreur `nasm`/`ld` et de plantage
  runtime reformulés en langage clair.
- **Multilingue** — interface en français, anglais et espagnol.
- **Mise à jour automatique** — vérification des nouvelles versions via GitHub
  Releases.

---

## Installation

### Binaire précompilé

Téléchargez la dernière archive depuis les
[Releases GitHub](https://github.com/fredza/asm-studio/releases), puis :

```bash
tar xzf asm-studio-*-linux-x86_64.tar.gz
cd asm-studio-*/
./install.sh                  # installation utilisateur, dans ~/.local
# ou
sudo ./install.sh --system    # installation système, dans /usr/local
```

Le script installe le binaire, l'icône et le fichier `.desktop`, et vérifie la
présence de `nasm` et `ld`.

### Voir aussi

- [`DEPENDENCIES.md`](DEPENDENCIES.md) — liste complète des bibliothèques
  système requises (Wayland/X11, portail XDG, `nasm`, `binutils`…) et des
  commandes d'installation par distribution.
- [`doc/GUIDE-DEMARRAGE-RAPIDE.md`](doc/GUIDE-DEMARRAGE-RAPIDE.md) — guide
  d'utilisation complet (premier programme, panneaux, raccourcis, dépannage).

---

## Démarrage rapide

Au premier lancement, ASM Studio crée vos dossiers de travail, sépare les
exemples et exercices commentés dans `examples/elf` et `examples/windows`, et
ouvre en **mode Apprentissage** avec un bandeau proposant de démarrer le
tutoriel guidé. Hors Apprentissage, l'explorateur démarre un niveau au-dessus
de `examples`, avec ce dossier pédagogique fermé.

Un premier programme minimal (`Fichier → Nouveau`, `Ctrl+N`) :

```nasm
section .text
    global _start
_start:
    mov rax, 60      ; sys_exit
    xor rdi, rdi     ; code de sortie 0
    syscall
```

Cycle de travail : **Assembler → Lancer → Pas à pas → Timeline**. Détails
complets dans le [guide de démarrage rapide](doc/GUIDE-DEMARRAGE-RAPIDE.md).

---

## Raccourcis clavier

Les principaux ; `F1` affiche la liste complète dans l'application.

| Touche | Action |
|---|---|
| `F1` | Afficher / masquer l'aide des raccourcis |
| `Ctrl+B` | Assembler et lier |
| `F5` | Lancer / relancer |
| `F10` (ou `F8`) | Instruction suivante |
| `Maj+F10` | Pas par-dessus : exécute l'appel d'un bloc |
| `F9` | Continuer jusqu'au prochain point d'arrêt |
| `Ctrl+F8` | Point d'arrêt sur la ligne du curseur (ou clic dans la gouttière) |
| `Ctrl+Maj+F8` | Condition du point d'arrêt (ou clic droit dans la gouttière) |
| `Échap` (ou `Maj+F5`) | Arrêter le programme |
| `←` / `→` | Timeline : étape précédente / suivante |
| `Début` / `Fin` | Timeline : début / fin |
| `Ctrl+N` / `Ctrl+Maj+N` / `Ctrl+O` / `Ctrl+S` | Nouveau fichier / projet / Ouvrir / Enregistrer |
| `Ctrl+F` / `Ctrl+H` | Rechercher / rechercher et remplacer |
| `Ctrl+Maj+P` | Palette de commandes — toute l'application au clavier |
| `Ctrl+1` … `Ctrl+5` | Afficher / masquer un panneau |
| `F6` / `Maj+F6` | Panneau suivant / précédent |

Toute l'interface est pilotable au clavier : la palette de commandes
(`Ctrl+Maj+P`) donne accès à chaque action sans passer par les menus.

---

## Compiler depuis les sources

Prérequis : Rust (édition 2024), `nasm`, `binutils` (`ld`), et les bibliothèques
listées dans [`DEPENDENCIES.md`](DEPENDENCIES.md) (Wayland/EGL, `libxkbcommon`,
portail XDG).

```bash
git clone https://github.com/fredza/asm-studio.git
cd asm-studio
cargo build --release
./target/release/asm_studio
```

Fabriquer une archive de distribution (binaire + ressources + scripts) :

```bash
./install/package.sh
# → dist/asm-studio-<version>-linux-x86_64.tar.gz
```

Lancer les tests :

```bash
cargo test
```

---

## Dépendances

Plateforme : **Linux x86-64** uniquement (l'exécution pas à pas repose sur
`ptrace`). Principales dépendances Rust :

| Crate | Rôle |
|---|---|
| `eframe` / `egui_dock` | interface graphique et disposition ancrable |
| `nix` | `ptrace`, gestion de processus et signaux |
| `capstone` | désassemblage x86-64 |
| `object` | lecture ELF/PE, et écriture de l'exécutable PE |
| `rfd` | dialogues fichiers natifs (portail XDG) |
| `ureq` / `serde` | vérification des mises à jour (GitHub Releases) |

Outils externes requis à l'exécution : `nasm` (assembleur) et `ld` (éditeur de
liens). Voir [`DEPENDENCIES.md`](DEPENDENCIES.md) pour le détail complet
(bibliothèques système, paquets par distribution, vérification rapide).

---

## Structure du projet

```
src/
├── app/            interface (panneaux ancrables, menus, raccourcis, tutoriel…)
├── debugger.rs      pilotage ptrace du processus débogué
├── disasm.rs         désassemblage (capstone)
├── assemble.rs       invocation de nasm / ld (ELF) ou nasm / lieur intégré (PE)
├── pe_link.rs         lieur PE64 : sections, imports, relocations
├── binfmt.rs           explorateur de format binaire (ELF et PE)
├── simd.rs              lecture des registres XMM / x87
├── winerun.rs            exécution du .exe produit, sous Wine
├── tutorial.rs        contenu du parcours guidé
├── exercise.rs         exercices auto-corrigés
├── i18n.rs              traductions FR / EN / ES
└── main.rs                point d'entrée
examples_seed/      exemples et exercices semés au premier lancement
install/            scripts d'installation et de packaging
doc/                guide de démarrage rapide
```

---

## Licence

Distribué sous la **ASM Studio Personal Free License (ASFL) v1.0** — voir
[`LICENSE.md`](LICENSE.md). En résumé : usage libre et gratuit, code source
consultable et modifiable, contributions par *pull request* bienvenues ; la
vente ou la redistribution commerciale du logiciel (original ou modifié) est
interdite sans autorisation écrite de l'auteur.

Copyright © 2026 Frédéric Zawalski.
