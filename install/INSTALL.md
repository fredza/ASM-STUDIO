# Installation — ASM Studio

IDE pédagogique pour apprendre l'assembleur NASM x86-64.
**Linux x86-64 uniquement** (l'exécution pas-à-pas repose sur `ptrace`).

---

## En trois commandes

```bash
tar xzf asm-studio-*-linux-x86_64.tar.gz
cd asm-studio-*-linux-x86_64
./install.sh
```

Le script installe dans `~/.local` : **aucun droit root n'est nécessaire**, et
l'application se retrouve dans le menu des applications.

---

## Prérequis

ASM Studio appelle deux outils externes pour transformer votre source en
programme exécutable. Sans eux, l'éditeur fonctionne mais le bouton
**Assembler** échoue.

| Outil | Rôle | Indispensable |
|---|---|---|
| `nasm` ≥ 2.16 | assemble le `.asm` en fichier objet | oui |
| `ld` (binutils) | lie l'objet en exécutable | oui |
| `xdg-desktop-portal` | dialogues « Ouvrir » / « Enregistrer sous » | recommandé |

`install.sh` vérifie leur présence et refuse de continuer si l'un des deux
premiers manque — mieux vaut le savoir maintenant qu'au premier assemblage.

### Installer les prérequis

```bash
# Fedora / RHEL
sudo dnf install nasm binutils xdg-desktop-portal xdg-desktop-portal-gnome

# Debian / Ubuntu
sudo apt install nasm binutils xdg-desktop-portal xdg-desktop-portal-gnome

# Arch
sudo pacman -S nasm binutils xdg-desktop-portal xdg-desktop-portal-gtk

# openSUSE
sudo zypper install nasm binutils xdg-desktop-portal
```

Les bibliothèques graphiques (Wayland, EGL, xkbcommon) sont présentes sur toute
distribution de bureau moderne. La liste exhaustive figure dans
[`DEPENDENCIES.md`](DEPENDENCIES.md).

---

## Options d'installation

```bash
./install.sh                      # ~/.local          (défaut, sans root)
sudo ./install.sh --system        # /usr/local        (tous les utilisateurs)
./install.sh --prefix /opt/asm    # préfixe libre
./install.sh --skip-checks        # ignore l'absence de nasm/ld
./install.sh --help
```

### Fichiers installés

| Chemin | Contenu |
|---|---|
| `PREFIX/bin/asm-studio` | l'exécutable |
| `PREFIX/share/applications/asm-studio.desktop` | entrée du menu |
| `PREFIX/share/icons/hicolor/256x256/apps/asm-studio.png` | icône |

Rien d'autre n'est touché.

### Si la commande `asm-studio` reste introuvable

`~/.local/bin` n'est pas toujours dans le `PATH`. Le script vous prévient, et
la correction est :

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
exec "$SHELL"
```

L'entrée du menu des applications, elle, fonctionne dans tous les cas.

---

## Premier lancement

Au tout premier démarrage, ASM Studio crée son espace de travail :

```
~/.local/share/asm_studio/examples/     dix programmes de démonstration
                                        + quatre exercices auto-corrigés
~/.local/share/asm_studio/build/        artefacts d'assemblage
~/.config/asm_studio/settings.conf      réglages et disposition des panneaux
```

Les exemples ne sont recréés que s'ils ont disparu : vos modifications ne
seront jamais écrasées.

Ouvrez `ex_code_sortie.asm` pour commencer — c'est le premier exercice, et le
panneau **Exercice** s'ouvre tout seul avec l'énoncé.

---

## Désinstallation

```bash
./uninstall.sh                    # retire l'application, garde vos fichiers
./uninstall.sh --system           # si installé avec --system (avec sudo)
./uninstall.sh --purge            # retire AUSSI réglages et programmes
```

Sans `--purge`, vos programmes `.asm` et vos réglages sont conservés.

---

## Compiler depuis les sources

```bash
# Rust stable (édition 2024)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

git clone <dépôt> asm_studio && cd asm_studio
cargo build --release
./install/install.sh              # trouve target/release/asm_studio tout seul
```

Pour fabriquer une archive de distribution :

```bash
./install/package.sh
# → dist/asm-studio-<version>-linux-x86_64.tar.gz  (+ .sha256)
```

`package.sh` compile en release, **lance la suite de tests**, et refuse de
produire l'archive si l'un d'eux échoue.

---

## Vérifier une archive téléchargée

```bash
sha256sum -c asm-studio-*-linux-x86_64.tar.gz.sha256
```

---

## Dépannage

| Symptôme | Cause probable | Correction |
|---|---|---|
| « impossible de lancer nasm » | `nasm` absent du `PATH` | installer le paquet `nasm` |
| « impossible de lancer ld » | `binutils` absent | installer le paquet `binutils` |
| « Ouvrir » n'affiche aucune fenêtre | pas de backend de portail XDG | installer `xdg-desktop-portal-gnome` (ou `-kde`, `-gtk`) |
| Fenêtre noire au lancement | pilote graphique / EGL | essayer `WINIT_UNIX_BACKEND=x11 asm-studio` |
| L'icône n'apparaît pas au menu | cache du bureau pas régénéré | se déconnecter/reconnecter, ou relancer `install.sh` |
| Disposition des panneaux cassée | réglage corrompu | menu **Affichage → Réinitialiser la disposition** |

Si l'application ne démarre pas du tout, lancez-la depuis un terminal : le
message d'erreur y sera visible.

```bash
asm-studio
```

---

## Licence

ASM Studio Personal Free License (ASFL) v1.0 — voir [`LICENSE.md`](../LICENSE.md).

Usage gratuit et sans limite de durée, redistribution de la version officielle
modifiée interdite sans l'accord écrit de l'auteur, vente interdite sans accord écrit de l'auteur.
