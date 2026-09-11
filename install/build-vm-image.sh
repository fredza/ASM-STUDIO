#!/usr/bin/env bash
#
# Construit l'image disque de la VM Linux qui héberge `ld` + le pas-à-pas
# ptrace côté macOS — voir le plan de portage macOS et src/vm_session.rs.
# Réservé au mainteneur : l'utilisateur final récupère l'image toute faite,
# il ne lance jamais ce script.
#
#   ./install/build-vm-image.sh
#   → ~/Library/Application Support/ASM Studio/vm/asmstudio-vm.qcow2
#
# macOS uniquement. Dépendances : QEMU (`brew install qemu`), la toolchain
# croisée `x86_64-unknown-linux-gnu` (`brew install
# messense/macos-cross-toolchains/x86_64-unknown-linux-gnu`, voir
# .cargo/config.toml pour la raison du choix glibc plutôt que musl),
# `rustup target add x86_64-unknown-linux-gnu`. `bsdtar`, `python3`, `curl`
# sont déjà sur toute install macOS.
#
# Principe : un installeur Debian netinst, entièrement automatisé par
# preseed (aucune interaction), démarré via `-kernel`/`-initrd` extraits de
# l'ISO plutôt que par le prompt isolinux — plus fiable à scripter. Trois
# pièges déjà rencontrés en mettant ça au point, tous ont leur trace dans le
# code ci-dessous :
#
#   1. `-cdrom` doit rester attaché même en démarrant par `-kernel`/`-initrd`
#      extraits à part : sans lui, l'installeur échoue sur « Detect and
#      mount installation media », qu'il cherche pour une partie de ses
#      composants même en installation réseau.
#   2. Le GRUB du système *installé* n'a par défaut aucune sortie sur la
#      console série (seul celui de l'installeur, dont les paramètres ne
#      survivent pas à l'installation, en avait une) — d'où
#      `console=ttyS0` forcé dans `/etc/default/grub` en fin d'installation.
#   3. `PermitRootLogin prohibit-password` est le défaut Debian : root passe
#      en console mais pas par mot de passe SSH tant qu'on ne le change pas
#      explicitement (uniquement fait avec `--debug-ssh`).
#
set -euo pipefail

# ---------------------------------------------------------------- présentation

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
have()  { command -v "$1" >/dev/null 2>&1; }

readonly HTTP_PORT_DEFAULT=8765
readonly DISK_SIZE_DEFAULT="3G"
readonly INSTALL_TIMEOUT=1200   # 20 min — une install complète en a pris ~10.
readonly BOOT_TIMEOUT=90        # même valeur que src/vm_session.rs::BOOT_TIMEOUT ;
                                 # mesuré à l'usage : jusqu'à ~35 s sur une image
                                 # fraîchement installée, TCG (pas d'accélération
                                 # matérielle sur cette paire d'architectures).

usage() {
    cat <<EOF
Construction de l'image VM d'ASM Studio (backend macOS) — réservé au mainteneur

Usage : $0 [options]

Options :
  --debug-ssh       Installe openssh-server, active le mot de passe root
                     ("asmstudio") pour déboguer l'image après coup. Attaque
                     la surface pour rien dans une image destinée à
                     l'utilisateur final — JAMAIS par défaut.
  --http-port PORT  Port du serveur HTTP local qui sert preseed/agent le
                     temps de l'installation (défaut : ${HTTP_PORT_DEFAULT})
  --disk-size SIZE  Taille du disque de la VM (défaut : ${DISK_SIZE_DEFAULT})
  --iso-url URL     ISO netinst à utiliser au lieu de la dernière stable
                     détectée automatiquement sur cdimage.debian.org
  --skip-verify     N'allume pas l'image finie pour vérifier que l'agent
                     répond (plus rapide, moins sûr)
  --keep-tmp        Garde le dossier de travail (diagnostic de ce script)
  -h, --help        Cette aide

Sortie : ~/Library/Application Support/ASM Studio/vm/asmstudio-vm.qcow2
(le chemin que lit src/vm_session.rs::image_path()) — remplacée seulement
une fois la construction ET la vérification terminées avec succès ; un
échec en cours de route n'affecte jamais une image existante qui marchait.
EOF
}

# ------------------------------------------------------------------- arguments

HTTP_PORT="${HTTP_PORT_DEFAULT}"
DISK_SIZE="${DISK_SIZE_DEFAULT}"
DEBUG_SSH=0
SKIP_VERIFY=0
KEEP_TMP=0
ISO_URL_OVERRIDE=""

while [ $# -gt 0 ]; do
    case "$1" in
        --debug-ssh)   DEBUG_SSH=1; shift ;;
        --http-port)   [ $# -ge 2 ] || { err "--http-port attend un port"; exit 2; }
                       HTTP_PORT="$2"; shift 2 ;;
        --http-port=*) HTTP_PORT="${1#*=}"; shift ;;
        --disk-size)   [ $# -ge 2 ] || { err "--disk-size attend une taille"; exit 2; }
                       DISK_SIZE="$2"; shift 2 ;;
        --disk-size=*) DISK_SIZE="${1#*=}"; shift ;;
        --iso-url)     [ $# -ge 2 ] || { err "--iso-url attend une URL"; exit 2; }
                       ISO_URL_OVERRIDE="$2"; shift 2 ;;
        --iso-url=*)   ISO_URL_OVERRIDE="${1#*=}"; shift ;;
        --skip-verify) SKIP_VERIFY=1; shift ;;
        --keep-tmp)    KEEP_TMP=1; shift ;;
        -h|--help)     usage; exit 0 ;;
        *)             err "option inconnue : $1"; usage; exit 2 ;;
    esac
done

readonly HTTP_PORT DISK_SIZE DEBUG_SSH SKIP_VERIFY KEEP_TMP ISO_URL_OVERRIDE

if [ "$(uname -s)" != "Darwin" ]; then
    err "ce script construit l'image pour le backend VM macOS — inutile ailleurs"
    exit 1
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
cd -- "${ROOT}"

# Même port que `crate::vm_protocol::AGENT_PORT` (src/vm_protocol.rs) — pas
# négociable, l'agent et `vm_session.rs` l'ont tous les deux en dur.
readonly AGENT_PORT=7878

readonly FINAL_IMAGE="${HOME}/Library/Application Support/ASM Studio/vm/asmstudio-vm.qcow2"
readonly CACHE_DIR="${HOME}/Library/Caches/asmstudio-vm-build"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/asmstudio-vm-build.XXXXXX")"
readonly WORK_DIR

# ------------------------------------------------------------------- nettoyage

QEMU_PID=""
HTTPD_PID=""
cleanup() {
    [ -n "${QEMU_PID}" ] && kill "${QEMU_PID}" >/dev/null 2>&1 || true
    [ -n "${HTTPD_PID}" ] && kill "${HTTPD_PID}" >/dev/null 2>&1 || true
    if [ "${KEEP_TMP}" -eq 1 ]; then
        dim "dossier de travail gardé (--keep-tmp) : ${WORK_DIR}"
    else
        rm -rf -- "${WORK_DIR}"
    fi
}
trap cleanup EXIT INT TERM

# ------------------------------------------------------------- outils requis

step "Vérification des outils"
missing=0
for tool in qemu-system-x86_64 bsdtar python3 curl cargo rustup; do
    if have "${tool}"; then
        ok "${tool}"
    else
        err "${tool} introuvable"
        missing=1
    fi
done
if ! rustup target list --installed | grep -qx "x86_64-unknown-linux-gnu"; then
    err "cible rustup x86_64-unknown-linux-gnu absente"
    dim "  rustup target add x86_64-unknown-linux-gnu"
    missing=1
else
    ok "cible rustup x86_64-unknown-linux-gnu"
fi
if ! have x86_64-linux-gnu-gcc && ! have x86_64-unknown-linux-gnu-gcc; then
    err "lieur croisé x86_64-linux-gnu-gcc introuvable"
    dim "  brew install messense/macos-cross-toolchains/x86_64-unknown-linux-gnu"
    missing=1
else
    ok "lieur croisé x86_64-linux-gnu-gcc"
fi
if [ "${missing}" -eq 1 ]; then
    err "des outils manquent — voir ci-dessus"
    exit 1
fi

# ------------------------------------------------------------ agent croisé

step "Compilation de l'agent (x86_64-unknown-linux-gnu, release)"
# Le lieur croisé est passé en variable d'environnement, pas via un
# `.cargo/config.toml` commité : un override de ce fichier pour la target
# `x86_64-unknown-linux-gnu` s'applique aussi à une compilation NATIVE sur une
# machine Linux (cette même target), où `x86_64-linux-gnu-gcc` n'existe pas —
# ça cassait `cargo build` pour tout développeur Linux du dépôt. La variable
# ne s'applique, elle, qu'à cette invocation.
if have x86_64-linux-gnu-gcc; then
    cross_linker="x86_64-linux-gnu-gcc"
else
    cross_linker="x86_64-unknown-linux-gnu-gcc"
fi
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="${cross_linker}" \
    cargo build --release --bin asmstudio-agent --features vm-agent --target x86_64-unknown-linux-gnu
readonly AGENT_BIN="${ROOT}/target/x86_64-unknown-linux-gnu/release/asmstudio-agent"
[ -f "${AGENT_BIN}" ] || { err "agent introuvable après compilation : ${AGENT_BIN}"; exit 1; }
ok "$(du -h "${AGENT_BIN}" | cut -f1)  ${AGENT_BIN#"${ROOT}"/}"

# ------------------------------------------------------- installeur Debian

step "Image d'installation Debian (netinst)"
mkdir -p -- "${CACHE_DIR}"
if [ -n "${ISO_URL_OVERRIDE}" ]; then
    iso_url="${ISO_URL_OVERRIDE}"
    iso_name="$(basename "${iso_url}")"
else
    listing_url="https://cdimage.debian.org/debian-cd/current/amd64/iso-cd/"
    iso_name="$(curl -fsSL "${listing_url}" | grep -oE 'debian-[0-9.]+-amd64-netinst\.iso' | sort -u | tail -1)"
    [ -n "${iso_name}" ] || { err "ISO netinst introuvable sur ${listing_url}"; exit 1; }
    iso_url="${listing_url}${iso_name}"
fi
readonly ISO_PATH="${CACHE_DIR}/${iso_name}"
if [ -f "${ISO_PATH}" ]; then
    ok "en cache : ${iso_name}"
else
    info "téléchargement de ${iso_name}…"
    curl -fL --progress-bar -o "${ISO_PATH}.part" "${iso_url}"
    mv -- "${ISO_PATH}.part" "${ISO_PATH}"
    ok "téléchargé : ${iso_name}"
fi

# Noyau + initrd extraits directement de l'ISO (pas de montage nécessaire) :
# on les passe à QEMU via `-kernel`/`-initrd` pour injecter les paramètres
# du preseed sans dépendre du prompt isolinux (pas d'attente, pas
# d'interaction possible à automatiser de façon fiable).
bsdtar -C "${WORK_DIR}" -xf "${ISO_PATH}" install.amd/vmlinuz install.amd/initrd.gz
ok "vmlinuz + initrd.gz extraits"

# --------------------------------------------------------- preseed + agent

step "Préparation du preseed"
readonly HTTPD_DIR="${WORK_DIR}/httpd"
mkdir -p -- "${HTTPD_DIR}"
install -m 644 "${AGENT_BIN}" "${HTTPD_DIR}/asmstudio-agent"

cat > "${HTTPD_DIR}/asmstudio-agent.service" <<'EOF'
[Unit]
Description=ASM Studio VM agent
After=network.target

[Service]
ExecStart=/usr/local/bin/asmstudio-agent
Restart=always
User=root

[Install]
WantedBy=multi-user.target
EOF

# Noms d'environnement distincts des variables bash `readonly` de même sens
# (DEBUG_SSH, HTTP_PORT) : `VAR=val commande` échoue sur une variable déjà
# en lecture seule dans ce shell, même juste pour la passer à un enfant.
ASMSTUDIO_DEBUG_SSH="${DEBUG_SSH}" ASMSTUDIO_HTTP_PORT="${HTTP_PORT}" ASMSTUDIO_HTTPD_DIR="${HTTPD_DIR}" python3 - <<'PYEOF'
import os

debug_ssh = os.environ["ASMSTUDIO_DEBUG_SSH"] == "1"
http_port = os.environ["ASMSTUDIO_HTTP_PORT"]
httpd_dir = os.environ["ASMSTUDIO_HTTPD_DIR"]

# `chpasswd`/`PermitRootLogin` se font depuis `late_command`, pas via une
# question `passwd/*` de l'installeur : ce sont des réglages du système
# *installé* (mot de passe SSH, pas mot de passe de session), posés une
# fois le système de base en place. `in-target` exécute dans le chroot
# /target, donc "/etc/ssh/sshd_config" y désigne déjà le bon fichier.
late_ssh = ""
if debug_ssh:
    late_ssh = (
        '    in-target sh -c "echo root:asmstudio | chpasswd"; \\\n'
        "    in-target sed -i 's/^#\\?PermitRootLogin.*/PermitRootLogin yes/' "
        "/etc/ssh/sshd_config; \\\n"
    )

packages = "nasm binutils"
if debug_ssh:
    packages += " openssh-server"

preseed = f"""### Généré par install/build-vm-image.sh — ne pas éditer à la main,
### éditer le script et reconstruire à la place.

d-i debian-installer/locale string en_US.UTF-8
d-i keyboard-configuration/xkb-keymap select us

d-i netcfg/choose_interface select auto
d-i netcfg/get_hostname string asmstudio-vm
d-i netcfg/get_domain string unassigned-domain
d-i netcfg/wireless_wep string

d-i mirror/country string manual
d-i mirror/http/hostname string deb.debian.org
d-i mirror/http/directory string /debian
d-i mirror/http/proxy string

d-i clock-setup/utc boolean true
d-i time/zone string UTC
d-i clock-setup/ntp boolean true

# Disque entier, une seule partition, ni LVM ni chiffrement : une VM
# jetable et reconstruite au besoin, pas un poste à protéger.
d-i partman-auto/method string regular
d-i partman-auto/choose_recipe select atomic
d-i partman-partitioning/confirm_write_new_label boolean true
d-i partman/choose_partition select finish
d-i partman/confirm boolean true
d-i partman/confirm_nooverwrite boolean true

# Mot de passe root aléatoire et jamais journalisé quand --debug-ssh est
# absent : root existe (l'installeur l'exige) mais personne ne peut s'en
# servir, console comme SSH.
d-i passwd/root-login boolean true
d-i passwd/root-password-crypted password *
d-i passwd/make-user boolean false

# Aucune tâche tasksel (pas de bureau, pas même « standard utilities ») :
# seuls nasm/binutils comptent ici.
tasksel tasksel/first multiselect
d-i pkgsel/include string {packages}
d-i pkgsel/upgrade select none
popularity-contest popularity-contest/participate boolean false

d-i grub-installer/only_debian boolean true
d-i grub-installer/bootdev string default

d-i finish-install/reboot_in_progress note
d-i debian-installer/exit/poweroff boolean true

# Dépose l'agent et son service systemd depuis le serveur HTTP local que le
# script hôte démarre pour l'occasion (voir install/build-vm-image.sh) —
# c'est aussi par lui que ce preseed lui-même a été récupéré. La console
# série et un GRUB sans attente valent pour toute image, débogage ou pas :
# aucune sortie visible autrement (le GRUB de l'installeur a sa propre
# configuration, qui ne survit pas à l'installation), et un boot
# déterministe et rapide est justement le but d'une image de production.
d-i preseed/late_command string \\
    in-target mkdir -p /usr/local/bin; \\
    wget -q -O /target/usr/local/bin/asmstudio-agent http://10.0.2.2:{http_port}/asmstudio-agent; \\
    in-target chmod 755 /usr/local/bin/asmstudio-agent; \\
    wget -q -O /target/etc/systemd/system/asmstudio-agent.service http://10.0.2.2:{http_port}/asmstudio-agent.service; \\
    in-target systemctl enable asmstudio-agent.service; \\
    in-target sed -i 's/^GRUB_CMDLINE_LINUX_DEFAULT=.*/GRUB_CMDLINE_LINUX_DEFAULT="console=ttyS0"/' /etc/default/grub; \\
    in-target sed -i 's/^GRUB_TIMEOUT=.*/GRUB_TIMEOUT=0/' /etc/default/grub; \\
    {late_ssh}in-target update-grub;
"""

with open(os.path.join(httpd_dir, "preseed.cfg"), "w") as f:
    f.write(preseed)

print(f"preseed.cfg écrit ({len(preseed)} octets, debug_ssh={debug_ssh})")
PYEOF
ok "preseed.cfg, agent et service prêts dans ${HTTPD_DIR#"${WORK_DIR}"/}"

# --------------------------------------------------------------- serveur HTTP

step "Serveur HTTP local (port ${HTTP_PORT})"
python3 -m http.server "${HTTP_PORT}" --bind 127.0.0.1 --directory "${HTTPD_DIR}" \
    > "${WORK_DIR}/httpd.log" 2>&1 &
HTTPD_PID=$!
sleep 1
if ! kill -0 "${HTTPD_PID}" 2>/dev/null; then
    err "le serveur HTTP n'a pas démarré (port ${HTTP_PORT} déjà pris ?)"
    cat "${WORK_DIR}/httpd.log" >&2
    exit 1
fi
ok "en écoute sur 127.0.0.1:${HTTP_PORT} (pid ${HTTPD_PID})"

# ------------------------------------------------------------ installation

step "Installation automatisée (jusqu'à ${INSTALL_TIMEOUT}s)"
readonly BUILD_IMAGE="${WORK_DIR}/asmstudio-vm.qcow2"
qemu-img create -f qcow2 "${BUILD_IMAGE}" "${DISK_SIZE}" >/dev/null
dim "disque neuf : ${DISK_SIZE}"

qemu-system-x86_64 -M q35 -m 2048 -smp 2 -nographic \
    -kernel "${WORK_DIR}/install.amd/vmlinuz" \
    -initrd "${WORK_DIR}/install.amd/initrd.gz" \
    -append "auto=true priority=critical url=http://10.0.2.2:${HTTP_PORT}/preseed.cfg hostname=asmstudio-vm domain= interface=auto console=ttyS0 ---" \
    -cdrom "${ISO_PATH}" \
    -drive "file=${BUILD_IMAGE},if=virtio,format=qcow2" \
    -netdev user,id=n0 -device virtio-net-pci,netdev=n0 \
    -no-reboot \
    > "${WORK_DIR}/install.log" 2>&1 &
QEMU_PID=$!

(
    sleep "${INSTALL_TIMEOUT}"
    if kill -0 "${QEMU_PID}" 2>/dev/null; then
        kill "${QEMU_PID}" 2>/dev/null || true
    fi
) &
WATCHDOG_PID=$!

set +e
wait "${QEMU_PID}"
QEMU_EXIT=$?
set -e
kill "${WATCHDOG_PID}" 2>/dev/null || true
wait "${WATCHDOG_PID}" 2>/dev/null || true
QEMU_PID=""

if ! grep -q "reboot: Power down" "${WORK_DIR}/install.log"; then
    err "l'installation ne s'est pas terminée proprement (code ${QEMU_EXIT})"
    dim "journal : ${WORK_DIR}/install.log $([ "${KEEP_TMP}" -eq 1 ] || echo '(perdu — relancez avec --keep-tmp)')"
    exit 1
fi
ok "installation terminée, VM éteinte proprement"

kill "${HTTPD_PID}" >/dev/null 2>&1 || true
HTTPD_PID=""

# --------------------------------------------------------------- vérification

if [ "${SKIP_VERIFY}" -eq 1 ]; then
    warn "--skip-verify : image non vérifiée avant installation"
else
    step "Vérification (démarrage réel, ping de l'agent)"
    qemu-system-x86_64 -M q35 -m 1024 -smp 2 -display none \
        -drive "file=${BUILD_IMAGE},if=virtio,format=qcow2" \
        -netdev "user,id=n0,hostfwd=tcp:127.0.0.1:${AGENT_PORT}-:${AGENT_PORT}" \
        -device virtio-net-pci,netdev=n0 \
        -qmp "unix:${WORK_DIR}/qmp.sock,server,nowait" \
        > "${WORK_DIR}/verify.log" 2>&1 &
    QEMU_PID=$!

    # Une simple connexion TCP réussie ne prouve rien : le réseau utilisateur
    # de QEMU (SLIRP) accepte la connexion côté hôte dès que QEMU démarre,
    # bien avant que l'invité ait fini de démarrer et que l'agent y tourne
    # vraiment — mesuré : jusqu'à une trentaine de secondes d'écart entre un
    # `connect()` qui réussit et l'agent qui répond vraiment sur une image
    # fraîchement installée. Voir `src/vm_session.rs::probe_once`, qui a le
    # même correctif côté application. Seul un aller-retour Ping → Pong
    # complet, retenté jusqu'à ${BOOT_TIMEOUT}s, prouve que l'agent tourne.
    ASMSTUDIO_AGENT_PORT="${AGENT_PORT}" ASMSTUDIO_BOOT_TIMEOUT="${BOOT_TIMEOUT}" python3 -c '
import socket, os, sys, time

port = int(os.environ["ASMSTUDIO_AGENT_PORT"])
deadline = time.time() + int(os.environ["ASMSTUDIO_BOOT_TIMEOUT"])
last_err = None
while time.time() < deadline:
    try:
        s = socket.create_connection(("127.0.0.1", port), timeout=2)
        s.settimeout(2)
        s.sendall(b"\"Ping\"\n")
        resp = s.recv(4096)
        s.close()
        if resp.strip() == b"\"Pong\"":
            sys.exit(0)
        last_err = f"réponse inattendue : {resp!r}"
    except OSError as e:
        last_err = str(e)
    time.sleep(1)
print(f"agent injoignable : {last_err}", file=sys.stderr)
sys.exit(1)
' || { err "l'agent ne répond pas (Ping → Pong) après ${BOOT_TIMEOUT}s"; exit 1; }
    ok "agent joignable, Ping → Pong"

    # Extinction propre par QMP (bouton power virtuel) : pas besoin de SSH
    # dans l'image, contrairement au débogage interactif de ce script.
    python3 -c "
import socket, json, os, time
sock = socket.socket(socket.AF_UNIX)
sock.settimeout(10)
sock.connect('${WORK_DIR}/qmp.sock')
sock.recv(4096)
sock.sendall(json.dumps({'execute': 'qmp_capabilities'}).encode() + b'\n')
sock.recv(4096)
sock.sendall(json.dumps({'execute': 'system_powerdown'}).encode() + b'\n')
sock.recv(4096)
sock.close()
"
    stopped=0
    for _ in $(seq 1 30); do
        if ! kill -0 "${QEMU_PID}" 2>/dev/null; then
            stopped=1
            break
        fi
        sleep 1
    done
    if [ "${stopped}" -eq 0 ]; then
        warn "extinction propre trop lente — arrêt forcé (l'image reste valide)"
        kill "${QEMU_PID}" 2>/dev/null || true
    fi
    QEMU_PID=""
    ok "VM de vérification éteinte"
fi

# -------------------------------------------------------------- installation

step "Mise en place"
mkdir -p -- "$(dirname -- "${FINAL_IMAGE}")"
mv -f -- "${BUILD_IMAGE}" "${FINAL_IMAGE}"
ok "$(du -h "${FINAL_IMAGE}" | cut -f1)  ${FINAL_IMAGE}"

if [ "${DEBUG_SSH}" -eq 1 ]; then
    echo
    warn "image construite avec --debug-ssh : root/asmstudio par SSH (port 22,"
    warn "non redirigé par défaut — voir src/vm_session.rs si besoin d'y accéder)."
    warn "Ne JAMAIS distribuer cette image telle quelle."
fi
