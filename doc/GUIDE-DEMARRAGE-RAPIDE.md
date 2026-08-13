# ASM Studio — Guide de démarrage rapide

> IDE pédagogique pour l'assembleur **NASM x86-64** sous Linux.
> Version 0.4.7 · interface en français / anglais / espagnol.

ASM Studio n'est pas un simulateur : votre programme est **réellement assemblé**
(`nasm`), **lié** (`ld`) et **exécuté par le vrai noyau Linux** sous `ptrace`.
Ce que vous voyez — registres, pile, mémoire, drapeaux — est l'état authentique
du processus, pas une approximation.

---

## Table des matières

1. [Prérequis](#1-prérequis)
2. [Premier lancement](#2-premier-lancement)
3. [L'écran en un coup d'œil](#3-lécran-en-un-coup-dœil)
4. [Votre premier programme en 3 minutes](#4-votre-premier-programme-en-3-minutes)
5. [Le cycle de travail : Assembler → Lancer → Pas à pas → Timeline](#5-le-cycle-de-travail)
6. [Les deux modes d'affichage](#6-les-deux-modes-daffichage)
7. [Les panneaux, un par un](#7-les-panneaux-un-par-un)
8. [Le parcours guidé (tutoriel)](#8-le-parcours-guidé-tutoriel)
9. [Les exercices auto-corrigés](#9-les-exercices-auto-corrigés)
10. [La bibliothèque asmstd](#10-la-bibliothèque-asmstd)
11. [Les outils](#11-les-outils)
12. [Quand ça plante : le diagnostic](#12-quand-ça-plante-le-diagnostic)
13. [Raccourcis clavier](#13-raccourcis-clavier)
14. [Où sont mes fichiers ?](#14-où-sont-mes-fichiers)
15. [Réglages](#15-réglages)
16. [Dépannage](#16-dépannage)

---

## 1. Prérequis

ASM Studio pilote deux outils système qui doivent être installés :

| Outil  | Rôle                    | Installation (Fedora)     | (Debian/Ubuntu)          |
|--------|-------------------------|---------------------------|--------------------------|
| `nasm` | Assembleur              | `sudo dnf install nasm`   | `sudo apt install nasm`  |
| `ld`   | Éditeur de liens        | fourni par `binutils`     | fourni par `binutils`    |

Plateforme : **Linux x86-64** (l'exécution pas-à-pas repose sur `ptrace`).

---

## 2. Premier lancement

Au tout premier démarrage, ASM Studio :

- crée vos dossiers de travail (voir [§14](#14-où-sont-mes-fichiers)) et y **sème
  une vingtaine de programmes** : exemples commentés et exercices à compléter ;
- ouvre en **mode Apprentissage** avec un **bandeau de bienvenue** en haut.

Le bandeau vous offre deux portes d'entrée :

- **▶ Commencer le tutoriel** — ouvre le panneau *Tutoriel* et son parcours guidé ;
- **Ouvrir un exemple** — pointe l'explorateur sur le dossier des exemples.

Le bouton **Écarter** fait disparaître le bandeau pour de bon (il ne reviendra
qu'après une réinitialisation du tutoriel). Vous pouvez tout retrouver plus tard
par les menus.

---

## 3. L'écran en un coup d'œil

De haut en bas :

- **Barre de menus** — Fichier · Exécution · Apprendre · Affichage · Outils · Aide.
- **Barre d'outils** — les actions les plus fréquentes, sous forme de boutons :
  **Lancer** · **Suivant** · **Arrêter** · **Relancer** · | · **Assembler**.
  Le bouton s'illumine (accent) quand l'action est disponible, se grise sinon.
- **Zone centrale** — un ensemble de **panneaux ancrables** (onglets). Glissez un
  onglet pour le **déplacer, l'empiler ou le détacher** en fenêtre flottante.
- **Barre d'état** (en bas) — état du programme :
  `○ Prêt` · `● Running` (avec le PID) · `✔ Exit 0` · `✘ Exit N` · `✘ Signal…`.
  En mode Apprentissage, les détails techniques y sont masqués.

> **Astuce** : un fin liseré signale le **panneau actif** (celui que le clavier
> pilote). `F6` passe au panneau suivant, `Maj+F6` au précédent.

---

## 4. Votre premier programme en 3 minutes

1. **Fichier → Nouveau** (`Ctrl+N`). Un squelette minimal apparaît :

   ```nasm
   section .data

   section .text
       global _start
   _start:
       mov rax, 60      ; sys_exit
       xor rdi, rdi     ; code 0
       syscall
   ```

2. Modifions-le pour renvoyer le code de sortie **42** :

   ```nasm
   section .text
       global _start
   _start:
       mov rax, 60      ; sys_exit
       mov rdi, 42      ; code de sortie
       syscall
   ```

3. **Assembler** (`Ctrl+B`) — `nasm` + `ld`. Les erreurs éventuelles s'affichent
   dans la **Console**, avec le numéro de ligne.

4. **Lancer** (`F5`) — le programme démarre, arrêté sur la première instruction.

5. **Suivant** (`F10`) — avancez d'une instruction. Regardez `RAX` puis `RDI`
   changer dans le panneau *Registres* (les valeurs modifiées **pulsent**).

6. À la fin, la barre d'état affiche `✔ Exit 42`. Bravo — c'est un vrai
   processus Linux qui vient de se terminer.

---

## 5. Le cycle de travail

ASM Studio distingue **assembler** (traduire le texte en binaire) et **exécuter**
(le faire tourner) :

```
   Éditer  ──Ctrl+B──▶  Assembler  ──F5──▶  Lancer  ──F10──▶  Pas à pas  ──▶  Exit
     ▲                                          │
     └──────────── corriger ◀── plantage / résultat inattendu
```

Un point « ● » à côté du nom du fichier signale que vous avez tapé quelque
chose depuis le dernier enregistrement. Rien ne peut plus l'emporter en
silence : créer un fichier, en ouvrir un autre, charger une leçon ou quitter
demande d'abord quoi faire de ce travail — enregistrer, abandonner, ou
renoncer. Et `Fichier ▸ Récents` garde la trace des dix derniers fichiers
ouverts, pour reprendre l'exercice de la veille sans le rechercher.

### La Timeline : avancer **et reculer**

Chaque pas est **enregistré**. Vous pouvez donc **remonter le temps** :

- `←` / `→` : étape précédente / suivante ;
- `Début` / `Fin` (`Home`/`End`) : premier / dernier instant enregistré ;
- **Reprendre ici** (menu Exécution) : repartir en exécution réelle depuis
  l'instant où vous êtes revenu.

C'est l'outil clé pour comprendre un bug : quand une valeur devient fausse,
reculez pas à pas jusqu'à voir **d'où** elle vient.

---

## 6. Les deux modes d'affichage

Menu **Affichage → Mode d'affichage** :

- **Apprentissage** — *l'essentiel* : code, instruction expliquée, registres
  généraux, pile, console. Idéal pour débuter.
- **Complet** — *tout* : désassemblage, vue mémoire, vidage hexadécimal, pile
  d'appels, appels système.

Changer de mode réorganise les panneaux vers la disposition de ce mode. Vous
pouvez toujours **Réinitialiser la disposition** (menu Affichage) si tout est
sens dessus dessous.

---

## 7. Les panneaux, un par un

Activez/désactivez chacun via **Affichage → Panneaux** (cases à cocher). Les
panneaux « Avancé » sont regroupés à part.

| Panneau              | À quoi il sert                                                              |
|----------------------|----------------------------------------------------------------------------|
| **Éditeur**          | Votre code source, coloré, avec suivi de la ligne en cours d'exécution.    |
| **Explorateur**      | Arborescence de fichiers ; double-clic pour ouvrir un `.asm`.              |
| **Instruction**      | Explication en clair de l'instruction courante (ce qu'elle fait, effets).  |
| **Registres**        | Les 16 registres généraux + RIP ; les valeurs modifiées pulsent.           |
| **Flags**            | Les drapeaux du CPU (ZF, SF, CF, OF…) : allumé / éteint.                    |
| **Pile / Tas**       | Contenu de la pile autour de RSP ; suivi des `push`/`pop`.                  |
| **Mémoire**          | Vidage hexadécimal navigable (`↑↓`, `PgUp`/`PgDn`).                         |
| **Vue mémoire**      | Vue unifiée : relie les registres aux zones mémoire qu'ils pointent.        |
| **Désassemblage**    | Le binaire réel décodé (adresses, octets, mnémoniques).                     |
| **Timeline**         | La frise des instants enregistrés ; cliquez pour vous y déplacer.          |
| **Console**          | Sortie du programme, messages de `nasm`/`ld`, journal.                      |
| **Pile d'appels**    | La chaîne des `call` en cours (fonctions imbriquées).                       |
| **Appels système**   | Les `syscall` interceptés, avec numéro, nom et arguments décodés.           |
| **Exercice**         | Les attentes de l'exercice ouvert et si elles sont satisfaites.            |
| **Tutoriel**         | Le parcours guidé (voir §8).                                               |

> Glissez les onglets pour composer **votre** disposition. **Affichage → Tout
> afficher** ouvre tous les panneaux ; **Réinitialiser la disposition** revient
> au standard du mode courant.

---

## 8. Le parcours guidé (tutoriel)

Menu **Apprendre → Parcours guidé** (ou bouton *Commencer le tutoriel* du
bandeau d'accueil). Un parcours **en 4 niveaux, 29 leçons** :

| Niveau         | Leçons | Exemples de thèmes                                             |
|----------------|:-----:|----------------------------------------------------------------|
| **Débutant**       | 9 | premier programme, registres, tailles, arithmétique, mul/div  |
| **Intermédiaire**  | 8 | fonctions, ABI System V, syscalls, tas, tableaux, chaînes      |
| **Avancé**         | 6 | format ELF, édition de liens, PLT/GOT, relocations             |
| **Expert**         | 6 | SIMD, optimisation, rétro-ingénierie, désassemblage, shellcode |

Chaque leçon :

- **charge son propre programme** dans l'éditeur (avec un `; TODO` à compléter) ;
- **ouvre les panneaux** qu'elle explique ;
- **embarque ses attentes** — le panneau *Exercice* vous dit si c'est juste.

La **progression est conservée** d'une session à l'autre. Le menu **Apprendre**
l'affiche (« Progression : 7 / 29 leçons ») et **Apprendre → Reprendre** rouvre
la leçon où vous en étiez, en la nommant. Pour repartir de zéro : **Apprendre →
Réinitialiser la progression** (remet tout à l'état du premier lancement :
bandeau d'accueil et panneau Tutoriel réouverts).

> Le parcours va avec le **mode Apprentissage** : c'est un seul et même état. Le
> nom du mode s'affiche en bas à droite de la barre d'état, et **un clic dessus**
> bascule entre *Apprentissage* et *Complet*.

---

## 9. Les exercices auto-corrigés

Une dizaine d'exercices `ex_*.asm` sont fournis (factorielle, fibonacci, longueur
de chaîne, manipulation de bits, moyenne d'un tableau…). Ouvrez-en un via
**Apprendre → Exemples et exercices…**, complétez le `; TODO`, lancez (`F5`), et
le panneau **Exercice** coche les attentes une à une.

Les exercices rattachés à une leçon sont aussi annoncés dans le sommaire du
parcours (`· 2 ✎`) : cliquer sur ce compte ouvre la leçon qui les porte.

### Écrire vos propres exercices : les directives `;@`

En tête d'un fichier, dans des commentaires, vous **décrivez ce qui est attendu**.
ASM Studio corrige tout seul.

```nasm
;@titre    Calculer 5!
;@enonce   Laisse le résultat dans RAX avant de quitter.
;@attendu  rax == 120
;@attendu  exit == 0
;@interdit imul          ; on veut une vraie boucle, pas un raccourci
;@requis   loop          ; l'instruction « loop » doit apparaître
```

| Directive (FR / EN)              | Effet                                                        |
|----------------------------------|-------------------------------------------------------------|
| `;@titre` / `;@title`            | Titre de l'exercice.                                         |
| `;@enonce` / `;@statement`       | L'énoncé (plusieurs lignes possibles).                       |
| `;@attendu` / `;@expect`         | Une condition à vérifier (voir ci-dessous).                 |
| `;@interdit` / `;@forbid` `<tok>`| Le code **ne doit pas** contenir ce mot (ex. `imul`).       |
| `;@requis` / `;@require` `<tok>` | Le code **doit** contenir ce mot.                           |

**Membre de gauche** d'un `;@attendu` : un registre (`rax`, `rbx`, … `rdi` …) ou
le mot-clé spécial **`exit`** (code de sortie du processus).

**Comparateurs** : `==` `!=` `<` `<=` `>` `>=`.

**Formats de valeur** acceptés à droite :

| Format      | Exemple    |
|-------------|------------|
| Décimal     | `120`      |
| Hexadécimal | `0x1F`     |
| Binaire     | `0b1010`   |
| Négatif     | `-1`       |
| Caractère   | `'A'`      |

Les directives mal formées ne sont **pas** ignorées en silence : elles sont
signalées avec leur numéro de ligne, pour que vous ne croyiez jamais un exercice
« vérifié » alors qu'il ne l'est pas.

---

## 10. La bibliothèque asmstd

Écrire « bonjour » en assembleur nu, c'est connaître le numéro du syscall
`write`, l'ordre de ses arguments, et compter soi-même la longueur de la chaîne.
**asmstd** met un nom lisible sur cette paperasse :

```nasm
%include "asmstd.inc"
    ; ...
    call asm.print       ; remplace cinq lignes de syscall
```

- **~100 fonctions** : sortie/saisie, fichiers, dossiers, processus, mémoire,
  réseau, temps, chaînes, caractères, nombres, tableaux… plus `asm.assert_eq`
  pour écrire des programmes qui **se contrôlent eux-mêmes**.
- L'index complet est **en tête de `asmstd.inc`**.
- Le programme reste **du vrai assembleur exécuté par le vrai noyau** — rien
  n'est simulé.

**Activation** : **Réglages → Bibliothèque asmstd**. Cela ajoute le dossier des
exemples aux chemins d'inclusion de `nasm`, pour que `%include "asmstd.inc"`
fonctionne depuis n'importe quel dossier.

---

## 11. Les outils

- **Points d'arrêt conditionnels** — un clic dans la gouttière pose un point
  d'arrêt ; un **clic droit** (ou `Ctrl+Maj+F8`) lui attache une condition.
  L'exécution ne s'y arrête alors que si elle est vraie :

  ```
  RCX == 0        arrête-toi au dernier tour de la boucle
  RAX > 0x100     … quand la valeur dépasse ce seuil
  ZF == 1         … quand la comparaison précédente a trouvé l'égalité
  RSI != RDI      … quand les deux pointeurs ont divergé
  ```

  On peut comparer des registres (`RAX`…`R15`, `RIP`, et les moitiés basses
  `EAX`, `R8D`), les six drapeaux (`ZF`, `CF`, `OF`, `SF`, `PF`, `AF`, qui
  valent 0 ou 1), et des nombres écrits en décimal, en hexadécimal (`0x2A`) ou
  en binaire (`0b1010`). Une pastille **à trou** dans la gouttière signale un
  point d'arrêt conditionnel, et son infobulle rappelle la condition.

  > À savoir : les registres se comparent en non signé, sauf si un nombre
  > négatif apparaît dans la condition — `RAX == -1` reconnaît donc bien
  > `0xFFFFFFFFFFFFFFFF`. Pour « ce registre est-il négatif ? », écrivez
  > `SF == 1`.

- **Inspection au survol** — posez la souris sur un mot de votre code, sans
  rien cliquer : un registre affiche sa valeur en hexadécimal, en décimal, en
  signé, en caractère, et les huit octets qu'il pointe quand c'est une adresse ;
  un drapeau affiche son état ; un label, sa ligne de définition et son
  adresse ; un nombre, ses trois bases. De quoi vérifier qu'un pointeur vise
  bien sa chaîne sans quitter le code des yeux.
- **Palette de commandes** (`Ctrl+Maj+P`) — toute l'application au clavier :
  tapez quelques lettres, lancez n'importe quelle commande sans la souris.
- **Microscope** — sélectionnez une instruction : identité, **encodage machine
  octet par octet**, effets (registres lus/écrits, flags), cycles estimés,
  contexte ABI, et un lien vers la **référence Intel**. Avant/après si
  l'instruction a déjà été exécutée dans l'historique.
- **Fenêtre Prédiction** (Affichage → Fenêtre Prédiction) — avant d'exécuter une
  instruction, **devinez** la nouvelle valeur d'un registre ; ASM Studio compare
  à l'exécution réelle et tient un score. Excellent pour s'auto-évaluer.
- **Calculatrice multi-base** (Outils) — conversion instantanée Déc / Hex / Oct /
  Bin / ASCII dans les deux sens. Le mode ASCII lit chaque caractère comme un
  octet, dans l'ordre d'une valeur de registre : `Hi` devient `0x4869`.
- **Vérifier les mises à jour** (Outils).

### Calculatrice : nombres, octets et texte

Ouvrez **Outils → Calculatrice**, choisissez la base de saisie, puis entrez A
et, si nécessaire, B. Sans B, la calculatrice sert simplement à convertir A ;
avec B, elle applique l'opération sélectionnée. La vue bit à bit et le tableau
de résultat restent disponibles dans toutes les bases.

Le mode **ASCII** est fait pour relier une chaîne à la valeur qu'elle occupe
dans un registre : chaque caractère vaut un octet et le premier est à gauche.
Ainsi `Hi` vaut `0x4869`. Un registre x86-64 ne contenant que huit octets, la
saisie est limitée à huit caractères (ou huit octets décodés).

Les échappements permettent de manipuler les octets qui ne s'écrivent pas
directement : `\0`, `\t`, `\n`, `\r`, `\\` et `\xNN` (par exemple
`\x0A`). Le résultat ASCII s'affiche entre apostrophes et réutilise ces
échappements pour les octets non imprimables. On peut donc appliquer les
opérations bit à bit usuelles à du texte : `a AND \xDF` donne `A`.

---

## 12. Quand ça plante : le diagnostic

Si le programme provoque une faute (segfault, division par zéro, débordement de
pile…), au lieu d'un sec « Terminé (signal) », une fenêtre **🛑 Le programme a
planté** apparaît :

- la **cause nommée** (ex. déréférencement de pointeur nul) ;
- une **explication** pédagogique ;
- une **piste de correction** ;
- un bouton **→ Voir la ligne** fautive ;
- les **détails techniques** (signal, RIP, adresse, région mémoire) repliés.

Et surtout : **la timeline s'arrête sur la faute**. Reculez (`←`) pour voir d'où
vient la valeur qui a tout fait dérailler.

---

## 13. Raccourcis clavier

`F1` affiche cette liste dans l'application. L'essentiel :

### Fichier & exécution
| Raccourci        | Action                             |
|------------------|------------------------------------|
| `Ctrl+N`         | Nouveau                            |
| `Ctrl+Maj+N`     | Nouveau projet                     |
| `Ctrl+O`         | Ouvrir                             |
| `Ctrl+S`         | Enregistrer                        |
| `Ctrl+B`         | Assembler + Lier                   |
| `F5`             | Lancer / Relancer                  |
| `F10` / `F8`     | Instruction suivante (pas à pas)   |
| `Maj+F10`        | Pas par-dessus (franchit un `call`) |
| `F9`             | Continuer jusqu'au point d'arrêt   |
| `Ctrl+F8`        | Point d'arrêt sur la ligne du curseur |
| `Ctrl+Maj+F8`    | Condition du point d'arrêt         |
| `Échap` / `Maj+F5` | Arrêter                          |

### Timeline
| Raccourci     | Action                            |
|---------------|-----------------------------------|
| `←` / `→`     | Étape précédente / suivante       |
| `Home` / `End`| Début / fin de la timeline        |

### Panneaux & navigation
| Raccourci       | Action                                              |
|-----------------|-----------------------------------------------------|
| `Ctrl+1..5`     | Afficher/masquer explorateur, instruction, registres, mémoire, Prédiction |
| `F6` / `Maj+F6` | Panneau suivant / précédent                         |
| `Ctrl+W`        | Fermer le panneau focalisé                          |
| `Ctrl+F6`       | Revenir directement à l'éditeur                     |
| `Ctrl+Tab`      | Onglet suivant du panneau focalisé                  |
| `Ctrl+Maj+P`    | Palette de commandes                                |
| `↑` / `↓`       | Parcourir le panneau focalisé (désassemblage, registres, mémoire…) |
| `PgUp` / `PgDn` | Mémoire : saut de huit lignes                       |
| `Entrée`        | Valider (microscope, ouvrir le fichier, éditer le registre) |

---

## 14. Où sont mes fichiers ?

ASM Studio respecte les conventions XDG et n'écrit **jamais** hors de votre dossier
personnel :

| Contenu                                   | Emplacement                              |
|-------------------------------------------|------------------------------------------|
| Réglages                                  | `~/.config/asm_studio/settings.conf`     |
| Exemples & exercices, `asmstd.inc`        | `~/.local/share/asm_studio/examples/`    |
| Artefacts d'assemblage (`.o`, binaires)   | `~/.local/share/asm_studio/…`            |

Le raccourci **Apprendre → Exemples et exercices…** pointe l'explorateur interne
directement sur ce dossier — c'est là que vous retrouvez les `ex_*.asm` et que
vous enregistrez votre travail.

> Les nouveaux exemples livrés avec une mise à jour sont **ajoutés** sans écraser
> vos fichiers existants.

---

## 15. Réglages

**Fichier → Préférences…**. Vous y trouvez :

- **Langue** — Français / English / Español.
- **Thème** — Système / Sombre / Clair. *(La coloration du code est optimisée
  pour le thème sombre.)*
- **Interface** — infobulles de raccourcis, animations « CPU vivant ».
- **Bibliothèque asmstd** — active `%include "asmstd.inc"` partout ([§10](#10-la-bibliothèque-asmstd)).
- **Parcours guidé** — ce qu'il contient et où le trouver. Il s'ouvre depuis le
  menu **Apprendre**, pas ici : le parcours suit le mode Apprentissage.
- **Mode pédagogique** — animations enrichies (flèches ↑↓), vue mémoire unifiée.

---

## 16. Dépannage

| Symptôme                                   | Cause probable / solution                                        |
|--------------------------------------------|------------------------------------------------------------------|
| « nasm: command not found » dans la Console | Installez `nasm` ([§1](#1-prérequis)).                            |
| Erreur d'édition de liens                   | Installez `binutils` (fournit `ld`).                             |
| `%include "asmstd.inc"` introuvable         | Activez **asmstd** dans les Réglages ([§10](#10-la-bibliothèque-asmstd)). |
| Le programme se fige au démarrage           | Une lecture bloquante sur l'entrée standard ; utilisez un fichier ou un tube. |
| L'exécution pas-à-pas ne démarre pas        | Assemblez d'abord (`Ctrl+B`) ; corrigez les erreurs de la Console. |
| Tout est en désordre après des glissers     | **Affichage → Réinitialiser la disposition**.                    |

---

*Bon assembleur ! Le meilleur moyen d'apprendre reste le parcours guidé
([§8](#8-le-parcours-guidé-tutoriel)) : chaque leçon est un petit programme qui
échoue tant qu'il n'est pas complété, et se valide tout seul quand il l'est.*
