# Dépendances runtime — ASM Studio

Ce document liste tout ce dont l'utilisateur a besoin pour faire tourner le binaire
`asm_studio` précompilé sur Linux x86-64.

---

## Bibliothèques système (liées statiquement ou toujours présentes)

| Bibliothèque | Fournie par | Notes |
|---|---|---|
| `glibc` ≥ 2.17 | distro | `libc.so.6`, `libm.so.6`, `ld-linux-x86-64.so.2` |
| `libgcc_s` | distro | `libgcc_s.so.1` — exceptions C++ internes |

Ces bibliothèques sont présentes sur **toute distribution Linux moderne** ; aucune
installation supplémentaire n'est nécessaire.

---

## Bibliothèques chargées dynamiquement au lancement (`dlopen`)

eframe/egui les charge lui-même au démarrage. Elles doivent être installées
mais ne figurent **pas** dans la sortie `ldd`.

| Bibliothèque | Rôle | Paquet Fedora / DNF | Paquet Ubuntu / APT |
|---|---|---|---|
| `libwayland-client.so` | Fenêtrage Wayland | `wayland` | `libwayland-client0` |
| `libEGL.so` | Contexte OpenGL/Wayland | `mesa-libEGL` | `libegl1` |
| `libGL.so` *(X11 only)* | OpenGL sous X11 | `mesa-libGL` | `libgl1` |
| `libxkbcommon.so` | Disposition clavier | `libxkbcommon` | `libxkbcommon0` |

> **Wayland (GNOME) :** seules les trois premières lignes sont nécessaires.  
> **X11 (fallback) :** remplacer `libEGL` + `libwayland-client` par `libGL` + `libX11`.

---

## Outils externes

| Outil | Rôle | Paquet Fedora / DNF | Paquet Ubuntu / APT |
|---|---|---|---|
| `nasm` ≥ 2.16 | Assembleur — traduit le `.asm` en fichier objet | `nasm` | `nasm` |
| `ld` | Éditeur de liens ELF — produit l'exécutable final (cible Linux) | `binutils` | `binutils` |

Les deux sont **indispensables** : `assemble.rs` les invoque à la suite
(`nasm -f elf64 … -o x.o` puis `ld -o x x.o`). Si l'un manque du `PATH`, le
bouton **Assembler** retourne « impossible de lancer nasm » ou
« impossible de lancer ld ».

La cible Windows (PE64) ne demande, elle, que `nasm` : `nasm -f win64` produit
l'objet COFF, et le lien est fait à l'intérieur d'ASM Studio (`pe_link.rs`). Ni
`lld-link` ni le SDK Microsoft ne sont requis — c'est précisément pour cela que
le lieur est intégré.

`wine` est **facultatif** : sans lui, la cible Windows assemble, désassemble et
décrit le `.exe` sans pouvoir le lancer, et l'IDE le dit. S'il est présent dans
le `PATH`, « Lancer » exécute le programme et sa sortie arrive dans la console,
comme celle d'un programme Linux — sans pas-à-pas, qui reste propre à la cible
ELF. La suite de tests s'en sert aussi ; les tests concernés s'ignorent d'
eux-mêmes quand wine manque.

| Outil | Rôle | Paquet Fedora / DNF | Paquet Ubuntu / APT |
|---|---|---|---|
| `wine` (facultatif) | Exécute le `.exe` produit par la cible Windows | `wine` | `wine` |

`binutils` est presque toujours déjà installé (c'est une dépendance de la
chaîne de compilation C), mais une image de conteneur minimale peut en manquer.

---

## Dialogues fichiers natifs (Enregistrer sous / Ouvrir)

Les dialogues utilisent le **portail XDG** (`xdg-desktop-portal`). Un backend
adapté à l'environnement de bureau doit être installé :

| Environnement | Backend requis | Paquet Fedora / DNF | Paquet Ubuntu / APT |
|---|---|---|---|
| GNOME (Wayland) | `xdg-desktop-portal-gnome` | `xdg-desktop-portal-gnome` | `xdg-desktop-portal-gnome` |
| KDE Plasma | `xdg-desktop-portal-kde` | `xdg-desktop-portal-kde` | `xdg-desktop-portal-kde` |
| Autres (X11…) | `xdg-desktop-portal-gtk` | `xdg-desktop-portal-gtk` | `xdg-desktop-portal-gtk` |

Sans portail, les dialogues « Ouvrir » et « Enregistrer sous » n'ouvrent pas de
fenêtre (la commande tombe en timeout silencieusement).

---

## Installation rapide — Fedora / DNF

```bash
sudo dnf install nasm binutils wayland mesa-libEGL libxkbcommon \
                 xdg-desktop-portal xdg-desktop-portal-gnome
```

## Installation rapide — Ubuntu / Debian / APT

```bash
sudo apt install nasm binutils libwayland-client0 libegl1 libxkbcommon0 \
                 xdg-desktop-portal xdg-desktop-portal-gnome
```

---

## Vérification rapide

```bash
# Nasm et ld disponibles ?
nasm --version
ld --version | head -1

# Portail XDG actif ?
systemctl --user status xdg-desktop-portal
```

---

## Matériel

| Composant | Minimum |
|---|---|
| Processeur | x86-64 (Intel/AMD 64 bits) |
| Affichage | OpenGL ES 2.0 ou OpenGL 3.2 (Mesa suffît) |
| RAM | 64 Mo (typiquement < 30 Mo utilisés) |
| OS | Linux kernel ≥ 4.14, glibc ≥ 2.17 |
