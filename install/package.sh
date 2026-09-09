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
readonly -a WINDOWS_EXAMPLES=(
    "win_hello_world.asm"
    "win_arithmetic.asm"
    "win_boucle.asm"
    "win_lire_ecrire.asm"
)

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
mkdir -p -- "${STAGE}/assets" "${STAGE}/examples"

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
# Toutes les tailles d'icône, plus les deux SVG sources : `install.sh` pose
# chacune dans son dossier `hicolor`, et se rabat sur le seul `icon.png` s'il
# ne trouve rien d'autre. Une archive qui ne porterait que le 256×256
# déclencherait ce repli sans rien dire, et l'icône serait laide en 32 px.
install -m 644 assets/icon.png assets/icon.svg assets/icon-small.svg "${STAGE}/assets/"
for taille in 16 24 32 48 64 128 256 512; do
    install -m 644 "assets/icon-${taille}.png" "${STAGE}/assets/"
done
ok "ressources (.desktop, icône en 8 tailles + SVG)"

for example in "${WINDOWS_EXAMPLES[@]}"; do
    if [ ! -f "examples_seed/${example}" ]; then
        err "exemple PE64 manquant : examples_seed/${example}"
        exit 1
    fi
    install -m 644 "examples_seed/${example}" "${STAGE}/examples/${example}"
done
ok "4 exemples PE64 essentiels"

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
' CHANGELOG.md > "${DIST}/.changelog-extract"

# Quel fichier prendre, dit avant le changelog. Une release expose quatre
# assets et GitHub les affiche sans un mot : le binaire nu vient en premier,
# c'est celui qu'on télécharge, et c'est le seul qui ne s'installe pas. HTTP
# ne transporte pas le bit d'exécution — arrivé sur le bureau il n'est plus
# exécutable, et le gestionnaire de fichiers répond « aucune application
# n'est installée pour les fichiers Executable », ce qui ne désigne pas le
# vrai problème. C'est arrivé à l'auteur sur sa propre 0.5.0-beta.5 ; ça
# arrivera à tout le monde. Le binaire nu est là pour la mise à jour
# automatique, qui pose le bit elle-même après avoir vérifié la signature.
#
# Le test porte sur l'extrait, pas sur les notes finies : celles-ci ne sont
# jamais vides puisqu'elles commencent par ce bloc, et une section de
# changelog manquante passerait donc inaperçue.
if [ -s "${DIST}/.changelog-extract" ]; then
    {
        printf '### Installation\n\n'
        printf 'Téléchargez **`%s.tar.gz`**, puis :\n\n' "${PKG}"
        printf '```\n'
        printf 'tar xzf %s.tar.gz\n' "${PKG}"
        printf 'cd %s\n' "${PKG}"
        printf './install.sh\n'
        printf '```\n\n'
        printf "L'application rejoint le menu, avec son icône, et \`asm-studio\` le PATH.\n\n"
        printf 'Les deux autres fichiers ne servent pas à une installation manuelle :\n'
        printf '`%s` (sans extension) est le binaire que la mise à jour automatique\n' "${PKG}"
        printf 'télécharge, et `%s.sig` sa signature. Téléchargé à la main, ce binaire\n' "${PKG}"
        printf "arrive sans son bit d'exécution — \`chmod +x\` le rend lançable.\n\n"
        cat "${DIST}/.changelog-extract"
    } > "${NOTES}"
    ok "dist/RELEASE-NOTES-${VERSION}.md"
else
    rm -f -- "${NOTES}"
    err "aucune section « ## [${VERSION}] » dans CHANGELOG.md"
    dim "La release peut se publier sans, mais elle n'aura pas de notes."
fi
rm -f -- "${DIST}/.changelog-extract"

echo
dim "Contenu :"
tar -tzf "${DIST}/${PKG}.tar.gz" | sed 's/^/    /'
echo
# Depuis .github/workflows/release.yml, pousser le tag suffit : la CI
# recompile, retteste, empaquette, SIGNE (secret UPDATE_SIGNING_KEY — voir
# src/bin/release_sign.rs) et publie elle-même les quatre assets, y compris
# le binaire nu et sa signature que ce script, lui, ne produit pas. C'est
# désormais le chemin normal ; ce que ce script vient de construire dans
# dist/ ne sert plus qu'à vérifier localement, avant de pousser, que
# l'archive s'assemble bien.
readonly TAG="v${VERSION}"
if ! git rev-parse -q --verify "refs/tags/${TAG}" >/dev/null; then
    echo
    dim "Pour publier : poussez le tag, la CI se charge du reste (build, tests,"
    dim "signature, publication GitHub) —"
    dim "  git tag -a ${TAG} -m \"ASM Studio ${VERSION}\""
    dim "  git -c credential.helper='!gh auth git-credential' \\"
    dim "      push https://github.com/fredza/ASM-STUDIO.git ${TAG}"
    dim "Suivre la publication :  gh run watch"
else
    dim "Le tag ${TAG} existe déjà — poussez-le s'il ne l'est pas encore, ou"
    dim "rejouez la publication depuis l'onglet Actions (workflow_dispatch)."
fi

echo
dim "Publication manuelle depuis cette machine (sans passer par la CI, donc"
dim "SANS le binaire signé ni sa .sig — la mise à jour automatique ne verra"
dim "pas cette release tant qu'ils ne sont pas ajoutés à la main) :"
if gh release view "${TAG}" >/dev/null 2>&1; then
    dim "  gh release upload ${TAG} dist/${PKG}.tar.gz dist/${PKG}.tar.gz.sha256 --clobber"
else
    PRERELEASE=""
    case "${VERSION}" in *-*) PRERELEASE=" --prerelease" ;; esac
    dim "  gh release create ${TAG}${PRERELEASE} dist/${PKG}.tar.gz dist/${PKG}.tar.gz.sha256 \\"
    if [ -f "${NOTES}" ]; then
        dim "     --title \"ASM Studio ${VERSION}\" --notes-file \"${NOTES#"${ROOT}"/}\""
    else
        dim "     --title \"ASM Studio ${VERSION}\" --notes \"…\""
    fi
fi

echo
dim "Rappel : le dépôt interrogé par la mise à jour automatique est défini par"
dim "GITHUB_REPO dans src/updater.rs — actuellement :"
dim "  $(sed -n 's/^const GITHUB_REPO: &str = "\(.*\)";/\1/p' src/updater.rs)"
