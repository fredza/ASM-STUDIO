#!/usr/bin/env bash
#
# Fabrique l'archive de distribution d'ASM Studio.
#
# Destiné au mainteneur, pas à l'utilisateur : compile en mode release et
# rassemble binaire, ressources, scripts et documentation dans un tarball prêt
# à publier.
#
#   ./install/package.sh
#   → dist/asm-studio-0.3.0-linux-x86_64.tar.gz
#
set -euo pipefail

if [ -t 1 ]; then
    readonly C_OK=$'\033[32m' C_ERR=$'\033[31m' C_DIM=$'\033[2m'
    readonly C_BOLD=$'\033[1m' C_OFF=$'\033[0m'
else
    readonly C_OK='' C_ERR='' C_DIM='' C_BOLD='' C_OFF=''
fi
ok()   { printf '%s✔%s %s\n' "$C_OK" "$C_OFF" "$*"; }
err()  { printf '%s✘%s %s\n' "$C_ERR" "$C_OFF" "$*" >&2; }
step() { printf '\n%s%s%s\n' "$C_BOLD" "$*" "$C_OFF"; }
dim()  { printf '%s  %s%s\n' "$C_DIM" "$*" "$C_OFF"; }

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
cd -- "${ROOT}"

# Version lue dans Cargo.toml : une seule source de vérité.
VERSION="$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\(.*\)"/\1/p' Cargo.toml | head -1)"
[ -n "${VERSION}" ] || { err "version introuvable dans Cargo.toml"; exit 1; }
readonly VERSION
readonly ARCH="linux-x86_64"
readonly PKG="asm-studio-${VERSION}-${ARCH}"
readonly DIST="${ROOT}/dist"
readonly STAGE="${DIST}/${PKG}"

step "Compilation (release)"
# Le script de build (hash git, date) n'est réexécuté que si .git/HEAD ou
# .git/logs/HEAD a changé : un binaire release gardé du jour d'avant affiche
# donc un « Build » périmé dans « À propos » — celui-là même que l'utilisateur
# copie pour demander sa licence. On force la réexécution avant de packager.
touch build.rs
cargo build --release
ok "target/release/asm_studio"

# La clé publique de vérification des licences est figée dans le binaire à la
# compilation. Un binaire plus ancien que la dernière modification de
# `src/license.rs` en embarque une autre et refuse toutes les licences avec
# « signature invalide » — c'est ce qui fait « la licence marche en debug mais
# pas en release ». On vérifie que ce qui part en distribution contient bien la
# clé actuellement dans les sources, et pas une clé de test.
if ! python3 - "${ROOT}/src/license.rs" target/release/asm_studio <<'PY'
import re, sys
src, binary = sys.argv[1], sys.argv[2]
m = re.search(r'const PUBLIC_KEY: \[u8; 32\] = \[(.*?)\];', open(src).read(), re.S)
key = bytes(int(x, 16) for x in re.findall(r'0x([0-9A-Fa-f]{2})', m.group(1)))
sys.exit(0 if key in open(binary, 'rb').read() else 1)
PY
then
    err "le binaire release n'embarque pas la PUBLIC_KEY de src/license.rs"
    dim "Recompilez depuis zéro :  cargo clean -p asm_studio && cargo build --release"
    exit 1
fi
ok "clé publique de licence conforme aux sources"

step "Vérifications avant publication"
# Une archive qui ne passe pas les tests n'a rien à faire en ligne.
if cargo test --release --quiet 2>&1 | tail -3; then
    ok "tests au vert"
else
    err "des tests échouent — publication annulée"
    exit 1
fi

step "Assemblage de ${PKG}"
rm -rf -- "${STAGE}"
mkdir -p -- "${STAGE}/assets"

install -m 755 target/release/asm_studio "${STAGE}/asm-studio"
ok "asm-studio (binaire)"

install -m 755 install/install.sh   "${STAGE}/install.sh"
install -m 755 install/uninstall.sh "${STAGE}/uninstall.sh"
ok "install.sh, uninstall.sh"

install -m 644 install/INSTALL.md   "${STAGE}/INSTALL.md"
install -m 644 DEPENDENCIES.md      "${STAGE}/DEPENDENCIES.md"
# En `set -e`, un `[ -f x ] && cmd` faux interrompt le script : d'où le if/fi.
if [ -f LICENSE.md ]; then install -m 644 LICENSE.md "${STAGE}/LICENSE.md"; fi
if [ -f README.md ];  then install -m 644 README.md  "${STAGE}/README.md";  fi
ok "documentation"

install -m 644 assets/asm-studio.desktop "${STAGE}/assets/"
install -m 644 assets/icon.png           "${STAGE}/assets/"
ok "ressources (.desktop, icône)"

step "Archive"
tar -C "${DIST}" -czf "${DIST}/${PKG}.tar.gz" "${PKG}"
rm -rf -- "${STAGE}"

# Somme de contrôle : permet à l'utilisateur de vérifier son téléchargement.
( cd -- "${DIST}" && sha256sum "${PKG}.tar.gz" > "${PKG}.tar.gz.sha256" )

SIZE="$(du -h "${DIST}/${PKG}.tar.gz" | cut -f1)"
ok "dist/${PKG}.tar.gz  (${SIZE})"
ok "dist/${PKG}.tar.gz.sha256"

echo
dim "Contenu :"
tar -tzf "${DIST}/${PKG}.tar.gz" | sed 's/^/    /'
echo
dim "Pour publier une release GitHub :"
dim "  gh release create v${VERSION} dist/${PKG}.tar.gz dist/${PKG}.tar.gz.sha256 \\"
dim "     --title \"ASM Studio ${VERSION}\" --notes-file CHANGELOG.md"
echo
dim "Rappel : le nom du dépôt attendu par la mise à jour automatique est défini"
dim "par GITHUB_REPO dans src/updater.rs — vérifiez-le avant publication."
