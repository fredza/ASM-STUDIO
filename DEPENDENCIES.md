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
| `nasm` ≥ 2.16 | Assembleur — requis pour compiler les `.asm` | `nasm` | `nasm` |

Sans `nasm` dans le `PATH`, le bouton **Assembler** retourne une erreur.

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
sudo dnf install nasm wayland mesa-libEGL libxkbcommon \
                 xdg-desktop-portal xdg-desktop-portal-gnome
```

## Installation rapide — Ubuntu / Debian / APT

```bash
sudo apt install nasm libwayland-client0 libegl1 libxkbcommon0 \
                 xdg-desktop-portal xdg-desktop-portal-gnome
```

---

## Vérification rapide

```bash
# Nasm disponible ?
nasm --version

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
