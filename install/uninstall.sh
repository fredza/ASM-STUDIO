#!/usr/bin/env bash
#
# Désinstalle ASM Studio.
#
# Par défaut, seuls les fichiers installés par install.sh sont retirés. Les
# données personnelles (réglages, exemples, programmes écrits par l'utilisateur)
# sont conservées sauf demande explicite : perdre son travail par surprise n'est
# pas acceptable.
#
set -euo pipefail

readonly APP_NAME="ASM Studio"
readonly BIN_NAME="asm-studio"
readonly ICON_NAME="asm-studio"
readonly DESKTOP_NAME="asm-studio.desktop"

if [ -t 1 ]; then
    readonly C_OK=$'\033[32m' C_WARN=$'\033[33m' C_ERR=$'\033[31m'
    readonly C_DIM=$'\033[2m' C_BOLD=$'\033[1m' C_OFF=$'\033[0m'
else
    readonly C_OK='' C_WARN='' C_ERR='' C_DIM='' C_BOLD='' C_OFF=''
fi

info() { printf '%s\n' "$*"; }
ok()   { printf '%s✔%s %s\n' "$C_OK" "$C_OFF" "$*"; }
warn() { printf '%s⚠%s  %s\n' "$C_WARN" "$C_OFF" "$*" >&2; }
err()  { printf '%s✘%s %s\n' "$C_ERR" "$C_OFF" "$*" >&2; }
step() { printf '\n%s%s%s\n' "$C_BOLD" "$*" "$C_OFF"; }
dim()  { printf '%s  %s%s\n' "$C_DIM" "$*" "$C_OFF"; }

usage() {
    cat <<EOF
${APP_NAME} — désinstallation

Usage : $0 [options]

Options :
  --prefix DIR   Préfixe utilisé à l'installation (défaut : \$HOME/.local)
  --system       Raccourci pour --prefix /usr/local (nécessite sudo)
  --purge        Supprime AUSSI les données personnelles :
                   ~/.config/asm_studio  (réglages)
                   ~/.local/share/asm_studio  (exemples, artefacts de build)
  -y, --yes      Ne pose aucune question (implique une confirmation)
  -h, --help     Affiche cette aide
EOF
}

PREFIX="${HOME}/.local"
PURGE=0
ASSUME_YES=0

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix)
            [ $# -ge 2 ] || { err "--prefix attend un chemin"; exit 2; }
            PREFIX="$2"; shift 2 ;;
        --prefix=*) PREFIX="${1#*=}"; shift ;;
        --system)   PREFIX="/usr/local"; shift ;;
        --purge)    PURGE=1; shift ;;
        -y|--yes)   ASSUME_YES=1; shift ;;
        -h|--help)  usage; exit 0 ;;
        *)          err "option inconnue : $1"; usage; exit 2 ;;
    esac
done

readonly PREFIX PURGE ASSUME_YES
readonly BIN_DIR="${PREFIX}/bin"
readonly APP_DIR="${PREFIX}/share/applications"
readonly ICON_DIR="${PREFIX}/share/icons/hicolor/256x256/apps"

have() { command -v "$1" >/dev/null 2>&1; }

# ------------------------------------------------------------------ inventaire

targets=(
    "${BIN_DIR}/${BIN_NAME}"
    "${APP_DIR}/${DESKTOP_NAME}"
    "${ICON_DIR}/${ICON_NAME}.png"
)

step "Fichiers à retirer"
found=0
for f in "${targets[@]}"; do
    if [ -e "$f" ]; then
        info "  ${f}"
        found=1
    fi
done
if [ "$found" -eq 0 ]; then
    warn "aucun fichier installé trouvé sous ${PREFIX}."
    dim "Si l'installation a utilisé un autre préfixe, passez --prefix."
fi

# Données personnelles.
readonly DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/asm_studio"
readonly CONF_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/asm_studio"

if [ "${PURGE}" -eq 1 ]; then
    step "Données personnelles à supprimer (--purge)"
    for d in "${CONF_DIR}" "${DATA_DIR}"; do
        [ -e "$d" ] && info "  ${d}"
    done
    warn "Vos programmes .asm enregistrés dans ces dossiers seront perdus."
fi

# --------------------------------------------------------------- confirmation

if [ "${ASSUME_YES}" -eq 0 ]; then
    echo
    printf 'Confirmer la désinstallation ? [o/N] '
    read -r reply
    case "${reply}" in
        [oOyY]|[oO][uU][iI]|[yY][eE][sS]) ;;
        *) info "Annulé."; exit 0 ;;
    esac
fi

# --------------------------------------------------------------- suppression

step "Suppression"
for f in "${targets[@]}"; do
    if [ -e "$f" ]; then
        rm -f -- "$f" && ok "${f}"
    fi
done

if [ "${PURGE}" -eq 1 ]; then
    for d in "${CONF_DIR}" "${DATA_DIR}"; do
        if [ -d "$d" ]; then
            rm -rf -- "$d" && ok "${d}"
        fi
    done
fi

# ------------------------------------------------------ rafraîchissement UI

if have update-desktop-database && [ -d "${APP_DIR}" ]; then
    update-desktop-database "${APP_DIR}" 2>/dev/null || true
fi
if have gtk-update-icon-cache && [ -d "${PREFIX}/share/icons/hicolor" ]; then
    gtk-update-icon-cache -qtf "${PREFIX}/share/icons/hicolor" 2>/dev/null || true
fi

echo
ok "${APP_NAME} est désinstallé."
if [ "${PURGE}" -eq 0 ]; then
    dim "Vos réglages et vos programmes sont conservés :"
    dim "  ${CONF_DIR}"
    dim "  ${DATA_DIR}"
    dim "Pour les supprimer aussi :  $0 --purge"
fi
