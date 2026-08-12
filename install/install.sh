#!/usr/bin/env bash
#
# Installe ASM Studio sur un système Linux x86-64.
#
# Par défaut l'installation est faite pour l'utilisateur courant, sans droits
# root, dans ~/.local — ce qui correspond à l'endroit où l'application range
# déjà ses réglages et ses exemples.
#
#   ./install.sh                  installation utilisateur (~/.local)
#   sudo ./install.sh --system    installation pour tous (/usr/local)
#   ./install.sh --prefix /opt/x  préfixe libre
#
set -euo pipefail

# ---------------------------------------------------------------- présentation

readonly APP_NAME="ASM Studio"
readonly BIN_NAME="asm-studio"
readonly ICON_NAME="asm-studio"
readonly DESKTOP_NAME="asm-studio.desktop"
readonly -a WINDOWS_EXAMPLES=(
    "win_hello_world.asm"
    "win_arithmetic.asm"
    "win_boucle.asm"
    "win_lire_ecrire.asm"
)

# Couleurs seulement si la sortie est un terminal (sinon les journaux sont sales).
if [ -t 1 ]; then
    readonly C_OK=$'\033[32m' C_WARN=$'\033[33m' C_ERR=$'\033[31m'
    readonly C_DIM=$'\033[2m' C_BOLD=$'\033[1m' C_OFF=$'\033[0m'
else
    readonly C_OK='' C_WARN='' C_ERR='' C_DIM='' C_BOLD='' C_OFF=''
fi

info()  { printf '%s\n' "$*"; }
ok()    { printf '%s✔%s %s\n' "$C_OK" "$C_OFF" "$*"; }
warn()  { printf '%s⚠%s  %s\n' "$C_WARN" "$C_OFF" "$*" >&2; }
err()   { printf '%s✘%s %s\n' "$C_ERR" "$C_OFF" "$*" >&2; }
step()  { printf '\n%s%s%s\n' "$C_BOLD" "$*" "$C_OFF"; }
dim()   { printf '%s  %s%s\n' "$C_DIM" "$*" "$C_OFF"; }

usage() {
    cat <<EOF
${APP_NAME} — installation

Usage : $0 [options]

Options :
  --prefix DIR   Préfixe d'installation (défaut : \$HOME/.local)
  --system       Raccourci pour --prefix /usr/local (nécessite sudo)
  --skip-checks  N'interrompt pas l'installation si nasm ou ld manquent
  -h, --help     Affiche cette aide

Fichiers installés :
  PREFIX/bin/${BIN_NAME}
  PREFIX/share/applications/${DESKTOP_NAME}
  PREFIX/share/icons/hicolor/256x256/apps/${ICON_NAME}.png
  XDG_DATA_HOME/asm_studio/examples/win_*.asm  (les 4 exemples PE64 essentiels)

Désinstallation : ./uninstall.sh (mêmes options de préfixe)
EOF
}

# ------------------------------------------------------------------- arguments

PREFIX="${HOME}/.local"
SKIP_CHECKS=0

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix)
            [ $# -ge 2 ] || { err "--prefix attend un chemin"; exit 2; }
            PREFIX="$2"; shift 2 ;;
        --prefix=*)  PREFIX="${1#*=}"; shift ;;
        --system)    PREFIX="/usr/local"; shift ;;
        --skip-checks) SKIP_CHECKS=1; shift ;;
        -h|--help)   usage; exit 0 ;;
        *)           err "option inconnue : $1"; usage; exit 2 ;;
    esac
done

readonly PREFIX SKIP_CHECKS
readonly BIN_DIR="${PREFIX}/bin"
readonly APP_DIR="${PREFIX}/share/applications"
readonly ICON_DIR="${PREFIX}/share/icons/hicolor/256x256/apps"

# Répertoire du script : les fichiers à installer sont à côté.
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR

# ------------------------------------------------- localisation des fichiers

# Le binaire est soit à côté du script (archive de distribution), soit dans
# target/release (installation depuis les sources).
find_binary() {
    local candidates=(
        "${SCRIPT_DIR}/${BIN_NAME}"
        "${SCRIPT_DIR}/asm_studio"
        "${SCRIPT_DIR}/../target/release/asm_studio"
    )
    local c
    for c in "${candidates[@]}"; do
        [ -f "$c" ] && { printf '%s' "$c"; return 0; }
    done
    return 1
}

# Idem pour les ressources.
find_asset() {
    local name="$1" c
    for c in "${SCRIPT_DIR}/assets/${name}" "${SCRIPT_DIR}/../assets/${name}"; do
        [ -f "$c" ] && { printf '%s' "$c"; return 0; }
    done
    return 1
}

# Dans l'archive, les exemples sont sous examples/. Depuis les sources, ils
# restent dans examples_seed/. Les quatre fichiers sont exigés : une archive
# incomplète ne doit pas donner l'impression que le support PE64 est installé.
find_windows_examples_dir() {
    local c
    for c in "${SCRIPT_DIR}/examples" "${SCRIPT_DIR}/../examples_seed"; do
        if [ -f "${c}/win_hello_world.asm" ] \
           && [ -f "${c}/win_arithmetic.asm" ] \
           && [ -f "${c}/win_boucle.asm" ] \
           && [ -f "${c}/win_lire_ecrire.asm" ]; then
            printf '%s' "$c"
            return 0
        fi
    done
    return 1
}

# Une installation systeme lancee par sudo doit alimenter l'espace de travail
# de la personne qui a lance sudo, pas celui de root. XDG_DATA_HOME est pris en
# compte pour une installation utilisateur et un XDG vide revient au defaut.
EXAMPLES_OWNER=""
EXAMPLES_HOME="${HOME}"
if [ "$(id -u)" -eq 0 ] && [ -n "${SUDO_USER:-}" ] && [ "${SUDO_USER}" != "root" ]; then
    EXAMPLES_OWNER="${SUDO_USER}"
    EXAMPLES_HOME="$(getent passwd "${SUDO_USER}" | cut -d: -f6)"
    if [ -z "${EXAMPLES_HOME}" ]; then
        err "impossible de trouver le dossier personnel de ${SUDO_USER}"
        exit 1
    fi
fi
if [ -z "${EXAMPLES_OWNER}" ] && [ -n "${XDG_DATA_HOME:-}" ]; then
    EXAMPLES_DATA_HOME="${XDG_DATA_HOME}"
else
    EXAMPLES_DATA_HOME="${EXAMPLES_HOME}/.local/share"
fi
readonly EXAMPLES_OWNER EXAMPLES_HOME EXAMPLES_DATA_HOME
readonly EXAMPLES_DIR="${EXAMPLES_DATA_HOME}/asm_studio/examples"

step "1/5  Recherche des fichiers"

if ! BINARY="$(find_binary)"; then
    err "binaire introuvable."
    dim "Attendu à côté de ce script, ou dans ../target/release/asm_studio."
    dim "Depuis les sources, compilez d'abord :  cargo build --release"
    exit 1
fi
readonly BINARY
ok "binaire : ${BINARY}"

if ! DESKTOP_SRC="$(find_asset "${DESKTOP_NAME}")"; then
    err "fichier ${DESKTOP_NAME} introuvable (attendu dans assets/)."
    exit 1
fi
if ! ICON_SRC="$(find_asset "icon.png")"; then
    err "icône icon.png introuvable (attendue dans assets/)."
    exit 1
fi
readonly DESKTOP_SRC ICON_SRC
ok "ressources : $(dirname -- "${DESKTOP_SRC}")"

if ! WINDOWS_EXAMPLES_SRC="$(find_windows_examples_dir)"; then
    err "exemples PE64 introuvables."
    dim "Attendus dans examples/ (archive) ou examples_seed/ (sources)."
    exit 1
fi
readonly WINDOWS_EXAMPLES_SRC
ok "exemples PE64 : ${WINDOWS_EXAMPLES_SRC}"

# ------------------------------------------------------ vérification runtime

step "2/5  Vérification des dépendances"

missing_required=()
have() { command -v "$1" >/dev/null 2>&1; }

# nasm et ld sont indispensables : sans eux, le bouton Assembler échoue.
if have nasm; then
    ok "nasm  $(nasm -v 2>/dev/null | head -1)"
else
    err "nasm absent — l'assemblage sera impossible."
    missing_required+=("nasm")
fi

if have ld; then
    ok "ld    $(ld --version 2>/dev/null | head -1)"
else
    err "ld absent (binutils) — l'édition de liens sera impossible."
    missing_required+=("binutils")
fi

# Le portail XDG sert aux dialogues « Ouvrir » et « Enregistrer sous ».
if have xdg-desktop-portal || [ -d /usr/libexec/xdg-desktop-portal ] \
   || ls /usr/libexec/xdg-desktop-portal* >/dev/null 2>&1; then
    ok "portail XDG présent (dialogues fichiers)"
else
    warn "xdg-desktop-portal introuvable : les dialogues « Ouvrir » et"
    dim "« Enregistrer sous » risquent de ne pas s'afficher."
    dim "L'application reste utilisable via l'explorateur intégré."
fi

if [ ${#missing_required[@]} -gt 0 ]; then
    echo
    warn "Paquets manquants : ${missing_required[*]}"
    if have dnf; then
        dim "sudo dnf install ${missing_required[*]}"
    elif have apt; then
        dim "sudo apt install ${missing_required[*]}"
    elif have pacman; then
        dim "sudo pacman -S ${missing_required[*]}"
    elif have zypper; then
        dim "sudo zypper install ${missing_required[*]}"
    fi
    if [ "${SKIP_CHECKS}" -eq 0 ]; then
        echo
        err "Installation interrompue. Relancez avec --skip-checks pour passer outre."
        exit 1
    fi
    warn "--skip-checks : on continue malgré tout."
fi

# ---------------------------------------------------------------- installation

step "3/5  Installation dans ${PREFIX}"

# Droits d'écriture : message clair plutôt qu'un « permission denied » brut.
for d in "${BIN_DIR}" "${APP_DIR}" "${ICON_DIR}"; do
    parent="$d"
    while [ ! -d "$parent" ] && [ "$parent" != "/" ]; do
        parent="$(dirname -- "$parent")"
    done
    if [ ! -w "$parent" ]; then
        err "pas de droit d'écriture dans ${parent}"
        dim "Pour une installation système :  sudo $0 --system"
        exit 1
    fi
done

install -d "${BIN_DIR}" "${APP_DIR}" "${ICON_DIR}"

install -m 755 "${BINARY}" "${BIN_DIR}/${BIN_NAME}"
ok "${BIN_DIR}/${BIN_NAME}"

install -m 644 "${ICON_SRC}" "${ICON_DIR}/${ICON_NAME}.png"
ok "${ICON_DIR}/${ICON_NAME}.png"

# Le .desktop porte un marqueur Exec= à remplacer par le chemin réel : sans
# chemin absolu, le lanceur ne trouve pas le binaire hors du PATH du shell.
sed "s|ASM_STUDIO_EXEC|${BIN_DIR}/${BIN_NAME}|" "${DESKTOP_SRC}" \
    > "${APP_DIR}/${DESKTOP_NAME}"
chmod 644 "${APP_DIR}/${DESKTOP_NAME}"
ok "${APP_DIR}/${DESKTOP_NAME}"

# Les exemples se placent dans les donnees utilisateur, meme pour --system.
# Un fichier deja present est volontairement preserve : il peut etre un
# programme que l'eleve a renomme ou modifie depuis une installation precedente.
step "4/5  Installation des exemples PE64"

if [ -n "${EXAMPLES_OWNER}" ]; then
    EXAMPLES_GROUP="$(id -gn "${EXAMPLES_OWNER}")"
    install -d -o "${EXAMPLES_OWNER}" -g "${EXAMPLES_GROUP}" "${EXAMPLES_DIR}"
else
    install -d "${EXAMPLES_DIR}"
fi

installed_examples=0
for example in "${WINDOWS_EXAMPLES[@]}"; do
    destination="${EXAMPLES_DIR}/${example}"
    if [ -e "${destination}" ]; then
        dim "conservé : ${destination}"
        continue
    fi
    if [ -n "${EXAMPLES_OWNER}" ]; then
        install -o "${EXAMPLES_OWNER}" -g "${EXAMPLES_GROUP}" -m 644 \
            "${WINDOWS_EXAMPLES_SRC}/${example}" "${destination}"
    else
        install -m 644 "${WINDOWS_EXAMPLES_SRC}/${example}" "${destination}"
    fi
    installed_examples=$((installed_examples + 1))
done

if [ "${installed_examples}" -gt 0 ]; then
    ok "${installed_examples} exemple(s) PE64 dans ${EXAMPLES_DIR}"
else
    dim "exemples PE64 deja presents : aucune modification"
fi

# ------------------------------------------------------- rafraîchissement UI

step "5/5  Mise à jour des caches du bureau"

if have update-desktop-database; then
    update-desktop-database "${APP_DIR}" 2>/dev/null && ok "base des applications"
else
    dim "update-desktop-database absent — sans effet, l'entrée apparaîtra à la reconnexion."
fi

if have gtk-update-icon-cache; then
    gtk-update-icon-cache -qtf "${PREFIX}/share/icons/hicolor" 2>/dev/null \
        && ok "cache d'icônes" || dim "cache d'icônes non régénéré (sans conséquence)"
fi

# ------------------------------------------------------------------- épilogue

echo
ok "${APP_NAME} est installé."
echo

case ":${PATH}:" in
    *":${BIN_DIR}:"*)
        info "Lancement :  ${BIN_NAME}"
        ;;
    *)
        warn "${BIN_DIR} n'est pas dans votre PATH."
        dim "Lancement direct :  ${BIN_DIR}/${BIN_NAME}"
        dim "Pour l'ajouter durablement :"
        dim "  echo 'export PATH=\"${BIN_DIR}:\$PATH\"' >> ~/.bashrc"
        ;;
esac

info "Ou depuis le menu des applications : « ${APP_NAME} »."
echo
dim "Les quatre exemples PE64 sont installes dans ${EXAMPLES_DIR}."
dim "Les autres programmes d'exemple sont completes au premier lancement."
dim "Les réglages sont enregistrés dans ~/.config/asm_studio/settings.conf."
echo
info "Désinstallation :  ${SCRIPT_DIR}/uninstall.sh"
