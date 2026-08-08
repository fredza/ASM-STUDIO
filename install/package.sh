#!/usr/bin/env bash
#
# Fabrique l'archive de distribution d'ASM Studio.
#
# Destiné au mainteneur, pas à l'utilisateur : compile en mode release et
# rassemble binaire, ressources, scripts et documentation dans un tarball prêt
# à publier.
#
#   ./install/package.sh
#   → dist/asm-studio-<version de Cargo.toml>-linux-x86_64.tar.gz
#
# Le numéro est toujours celui de Cargo.toml : cet en-tête a affiché « 0.3.0 »
# pendant deux versions, ce qui est précisément ce que le script évite partout
# ailleurs.
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
# Les trois langues, comme l'interface : un élève hispanophone qui ouvre
# l'archive n'a pas à passer par la version anglaise.
for r in README.md README.fr.md README.es.md; do
    if [ -f "${r}" ]; then install -m 644 "${r}" "${STAGE}/${r}"; fi
done
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

# Notes de version : la seule section du CHANGELOG qui concerne cette version.
# Passer le fichier entier en `--notes-file` collerait tout l'historique du
# projet dans la release, jusqu'à la 0.2.1.
readonly NOTES="${DIST}/RELEASE-NOTES-${VERSION}.md"
awk -v want="## [${VERSION}]" '
    index($0, want) == 1 { inside = 1; next }
    inside && /^## \[/   { exit }
    inside               { print }
' CHANGELOG.md > "${NOTES}"
if [ -s "${NOTES}" ]; then
    ok "dist/RELEASE-NOTES-${VERSION}.md"
else
    rm -f -- "${NOTES}"
    err "aucune section « ## [${VERSION}] » dans CHANGELOG.md"
    dim "La release peut se publier sans, mais elle n'aura pas de notes."
fi

echo
dim "Contenu :"
tar -tzf "${DIST}/${PKG}.tar.gz" | sed 's/^/    /'
echo
# Publier ou compléter : une release déjà créée (tag poussé plus tôt, notes
# rédigées à la main) refuse `gh release create` avec « already exists ». Le
# script regarde donc laquelle des deux commandes s'applique, plutôt que d'en
# proposer une qui échouera une fois sur deux.
readonly TAG="v${VERSION}"
if gh release view "${TAG}" >/dev/null 2>&1; then
    dim "La release ${TAG} existe déjà — pour y joindre l'archive :"
    dim "  gh release upload ${TAG} dist/${PKG}.tar.gz dist/${PKG}.tar.gz.sha256 --clobber"
else
    dim "Pour publier une release GitHub :"
    PRERELEASE=""
    case "${VERSION}" in *-*) PRERELEASE=" --prerelease" ;; esac
    dim "  gh release create ${TAG}${PRERELEASE} dist/${PKG}.tar.gz dist/${PKG}.tar.gz.sha256 \\"
    if [ -f "${NOTES}" ]; then
        dim "     --title \"ASM Studio ${VERSION}\" --notes-file \"${NOTES#"${ROOT}"/}\""
    else
        dim "     --title \"ASM Studio ${VERSION}\" --notes \"…\""
    fi
    case "${VERSION}" in
        *-*) dim "  (--prerelease est déjà là : sans lui, la mise à jour automatique"
             dim "   proposerait cette bêta à tous les utilisateurs stables.)" ;;
    esac
fi

# Pousser le tag depuis cette machine : l'agent SSH accepte la clé puis ne rend
# jamais la main sur la signature. Le jeton de `gh` passe, et `-c` ne modifie
# pas la configuration (`origin` reste en SSH).
if ! git rev-parse -q --verify "refs/tags/${TAG}" >/dev/null; then
    echo
    dim "Le tag ${TAG} n'existe pas encore :"
    dim "  git tag -a ${TAG} -m \"ASM Studio ${VERSION}\""
    dim "  git -c credential.helper='!gh auth git-credential' \\"
    dim "      push https://github.com/fredza/ASM-STUDIO.git ${TAG}"
fi
echo
dim "Rappel : le dépôt interrogé par la mise à jour automatique est défini par"
dim "GITHUB_REPO dans src/updater.rs — actuellement :"
dim "  $(sed -n 's/^const GITHUB_REPO: &str = "\(.*\)";/\1/p' src/updater.rs)"
