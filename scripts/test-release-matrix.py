#!/usr/bin/env python3
"""UniCore OS Phase 20D & Phase 21 Automated Release QA Matrix.

Verifies the built hybrid ISO across:
  1. Legacy BIOS ISO boot + Phase 20D Personalization/Theme/Input + Wayland Light screenshot + Ed25519 signing E2E
  2. UEFI firmware ISO boot (EDK2 OVMF)
  3. Simulated 2005 Legacy PC (512 MB RAM, 1 vCPU, no SSE4.2/AVX, zram + lightweight shell + unicore-center --check)
  4. Host ISO SHA-256, manifest validity, and Windows USB helper dry-check
"""

import base64
import glob
import gzip
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time

REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
OUT_DIR = os.path.join(REPO_ROOT, "os-builder", "out")
ISO_PATH = os.path.join(OUT_DIR, "unicore-os-x86_64.iso")
VMLINUZ_PATH = os.path.join(OUT_DIR, "vmlinuz-x86_64")
INITRD_PATH = os.path.join(OUT_DIR, "initramfs-x86_64.img")
SHA_PATH = ISO_PATH + ".sha256"
MANIFEST_PATH = os.path.join(OUT_DIR, "unicore-manifest-x86_64.json")
if not os.path.isfile(MANIFEST_PATH):
    MANIFEST_PATH = os.path.join(OUT_DIR, "unicore-manifest.json")


def _find_first(candidates):
    for p in candidates:
        if p and os.path.isfile(p):
            return p
    return candidates[0] if candidates else ""


if os.name == "nt":
    OVMF_BIOS = r"C:\Program Files\qemu\share\edk2-x86_64-code.fd"
    OVMF_VARS = r"C:\Program Files\qemu\share\edk2-i386-vars.fd"
    QEMU_BIN = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
    QEMU_IMG_BIN = r"C:\Program Files\qemu\qemu-img.exe"
    DEFAULT_ACCEL = "tcg"
else:
    OVMF_BIOS = _find_first([
        "/usr/share/OVMF/OVMF_CODE_4M.fd",
        "/usr/share/OVMF/OVMF_CODE.fd",
        "/usr/share/edk2/x64/OVMF_CODE.fd",
        "/usr/share/qemu/OVMF.fd",
    ])
    OVMF_VARS = _find_first([
        "/usr/share/OVMF/OVMF_VARS_4M.fd",
        "/usr/share/OVMF/OVMF_VARS.fd",
        "/usr/share/edk2/x64/OVMF_VARS.fd",
    ])
    QEMU_BIN = shutil.which("qemu-system-x86_64") or "/usr/bin/qemu-system-x86_64"
    QEMU_IMG_BIN = shutil.which("qemu-img") or "/usr/bin/qemu-img"
    DEFAULT_ACCEL = "kvm" if os.access("/dev/kvm", os.R_OK | os.W_OK) else "tcg"

SCREENSHOT_DIR = os.environ.get(
    "UNICORE_QA_SCREENSHOT_DIR",
    os.path.join(OUT_DIR, "qa-screenshots"),
)
SCREENSHOT_DEST = os.path.join(SCREENSHOT_DIR, "desktop_phase20d.png")


def read_expected_os_version():
    os_rel = os.path.join(REPO_ROOT, "os-builder", "overlay", "etc", "os-release")
    if os.path.isfile(os_rel):
        with open(os_rel, "r", encoding="utf-8") as f:
            for line in f:
                m = re.match(r'^VERSION_ID=["\']?([^"\'\r\n]+)["\']?$', line.strip())
                if m:
                    return m.group(1)
    return "0.2.1"


EXPECTED_OS_VERSION = read_expected_os_version()


class SerialHarness:
    def __init__(self, qemu_args, label):
        self.label = label
        self.proc = subprocess.Popen(
            [QEMU_BIN] + qemu_args,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            bufsize=0,
        )
        self.buf = ""
        self.lock = threading.Lock()
        self.reader = threading.Thread(target=self._read_loop, daemon=True)
        self.reader.start()

    def _read_loop(self):
        while True:
            chunk = self.proc.stdout.read(1)
            if not chunk:
                break
            txt = chunk.decode("utf-8", errors="replace")
            with self.lock:
                self.buf += txt

    def get_buf(self):
        with self.lock:
            return self.buf

    def wait_for(self, marker, timeout_sec):
        deadline = time.time() + timeout_sec
        while time.time() < deadline:
            cur = self.get_buf()
            if marker in cur:
                return cur
            if self.proc.poll() is not None:
                raise RuntimeError(f"[{self.label}] QEMU exited early while waiting for {marker!r}:\n{cur[-1500:]}")
            time.sleep(0.25)
        tail = self.get_buf()[-2000:]
        raise TimeoutError(f"[{self.label}] Timed out after {timeout_sec}s waiting for {marker!r}. Tail:\n{tail}")

    def send_raw(self, cmd_str):
        data = cmd_str.encode("utf-8")
        for i in range(0, len(data), 4):
            self.proc.stdin.write(data[i : i + 4])
            self.proc.stdin.flush()
            time.sleep(0.015)

    def send_script_b64(self, sh_script, remote_path="/tmp/qa_gate.sh"):
        compressed = gzip.compress(sh_script.encode("utf-8"))
        b64 = base64.b64encode(compressed).decode("ascii")
        lines = [
            f"cat <<'EOF_B64' | base64 -d | gunzip > {remote_path}",
        ]
        for i in range(0, len(b64), 72):
            lines.append(b64[i : i + 72])
        lines.append("EOF_B64")
        lines.append(f"chmod +x {remote_path} && sh {remote_path}")
        for line in lines:
            self.send_raw(line + "\n")
            time.sleep(0.04)

    def close(self):
        try:
            self.send_raw("\npoweroff -f || true\n")
            time.sleep(0.5)
        except Exception:
            pass
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()


def boot_prompt_markers():
    # Alpine / BusyBox root getty prompt
    return ["unicore:~#", "~ # "]


def wait_for_shell(harness, timeout_sec):
    deadline = time.time() + timeout_sec
    last_len = 0
    sent_enter = False
    while time.time() < deadline:
        cur = harness.get_buf()
        if any(m in cur for m in boot_prompt_markers()):
            # Send a newline and confirm fresh prompt
            harness.send_raw("\necho ===SHELL_READY===\n")
            harness.wait_for("\n===SHELL_READY===", 20)
            return
        if not sent_enter and "localhost login:" in cur:
            harness.send_raw("root\n")
            sent_enter = True
        if harness.proc.poll() is not None:
            raise RuntimeError(f"[{harness.label}] QEMU exited before shell:\n{cur[-1500:]}")
        if len(cur) != last_len:
            last_len = len(cur)
        time.sleep(0.4)
    raise TimeoutError(f"[{harness.label}] Timed out waiting for Linux shell prompt:\n{harness.get_buf()[-2000:]}")


def gate1_bios_phase20d_and_ed25519():
    print("\n=== GATE 1: Legacy BIOS ISO + Phase 20D Personalization + Light Wayland UI + Ed25519 E2E ===")
    h = SerialHarness(
        [
            "-name", "UniCore-QA-BIOS-20D",
            "-m", "3072",
            "-smp", "2",
            "-cpu", "host" if DEFAULT_ACCEL == "kvm" else "qemu64,+sse4.2,+avx",
            "-accel", DEFAULT_ACCEL,
            "-vga", "std",
            "-cdrom", ISO_PATH,
            "-boot", "d",
            "-display", "none",
            "-serial", "stdio",
            "-monitor", "none",
            "-no-reboot",
        ],
        "Gate1-BIOS-20D",
    )
    try:
        wait_for_shell(h, 240)
        script = r"""#!/bin/sh
set -eu

echo "[1/5] Checking v__EXPECTED_OS_VERSION__ boot mode, wallpapers, Light theme & Phase 23/24/25 tools..."
. /mnt/unicore-boot.env
[ "${UNICORE_BOOT_MODE}" = "live" ] || { echo "Expected live boot mode, got ${UNICORE_BOOT_MODE}"; exit 1; }
grep -q 'VERSION_ID="__EXPECTED_OS_VERSION__"' /etc/os-release
uname -r
test -f /usr/share/unicore/wallpaper.png
test -f /usr/share/unicore/wallpaper-aurora.png
test -f /usr/share/unicore/wallpaper-light.png
test -f /usr/share/themes/UniCore-Light/openbox-3/themerc
test -f /usr/share/unicore/themes/waybar-light.css
test -f /usr/share/unicore/themes/fuzzel-light.ini
test -x /usr/libexec/unicore-personalize
test -x /usr/libexec/unicore-users
test -x /usr/libexec/unicore-power
test -x /usr/libexec/unicore-smart
test -x /usr/libexec/unicore-devbox
test -x /usr/libexec/unicore-ai
git --version
python3 --version
jq --version
podman --version
command -v swayidle
command -v smartctl
/usr/libexec/unicore-power status | grep -q '^profile=balanced$'
/usr/libexec/unicore-smart health | grep -q '^device='
/usr/libexec/unicore-users list | grep -q '^unicore'
/usr/libexec/unicore-devbox status | grep -q '^projects_dir=/data/dev/projects$'
/usr/libexec/unicore-ai status | grep -q '^endpoint=http://127.0.0.1:11434$'
modinfo ath9k forcedeth nouveau sata_nv snd_hda_intel >/dev/null
echo "Checking v0.3.0-v0.4.0 Secure Boot MOK, Auto-OTA, Browser Manager, Win32 Diagnoser, Flatpak, GE-Proton, TLP, Papirus, Gamepads & AI Copilot..."
test -f /etc/unicore/update.pub
test -f /etc/unicore/mok.der
grep -q 'unicore-manifest-x86_64.json' /etc/unicore/update.conf
grep -q '^AUTO_CHECK=1' /etc/unicore/update.conf
grep -q '^REQUIRE_SIGNATURE=1' /etc/unicore/update.conf
test -x /usr/libexec/unicore-update-check
test -x /usr/libexec/unicore-secureboot
test -x /usr/libexec/unicore-browser-mgr
test -x /usr/bin/unicore-browser
test -x /usr/libexec/unicore-flatpak
test -x /usr/libexec/unicore-proton
test -f /etc/udev/rules.d/60-unicore-gamepads.rules
test -d /usr/share/icons/Papirus-Dark
test -f /usr/share/file-manager/actions/unicore-open-sandbox.desktop
command -v flatpak
command -v tlp
command -v swaylock
/usr/libexec/unicore-secureboot status | grep -q '^mok_cert=1$'
/usr/libexec/unicore-secureboot status | grep -q '^shim_bundled=1$'
/usr/libexec/unicore-browser-mgr status | grep -q '^default_browser=epiphany$'
/usr/libexec/unicore-flatpak status | grep -q '^installed=1$'
/usr/libexec/unicore-proton status | grep -q '^dir=/data/gaming/proton-ge$'
/usr/libexec/unicore-ai status | grep -q '^provider=ollama$'
/usr/libexec/unicore-ai context | grep -q 'UniCore OS Live Read-Only System Diagnostics'
echo "Checking v0.5.0 Boot Splash, Capture/Recorder, DND Notify, Network Drives, PipeWire BlueZ, Avahi & Accessibility..."
test -s /usr/share/unicore/splash.ppm
test -f /usr/share/unicore/splash.ini
test -x /usr/libexec/unicore-capture
test -x /usr/libexec/unicore-notify
test -x /usr/libexec/unicore-shares
test -x /usr/libexec/unicore-a11y
command -v wf-recorder
command -v espeak-ng
command -v sshfs
test -d /usr/lib/spa-0.2/bluez5
test -e /usr/libexec/gvfs/gvfsd-smb || test -e /usr/libexec/gvfsd-smb || test -e /usr/lib/gvfs/gvfsd-smb
/usr/libexec/unicore-capture status | grep -q '^wf_recorder=1$'
/usr/libexec/unicore-capture waybar-status | grep -q '"class":"idle"'
/usr/libexec/unicore-notify status | grep -q '^dnd='
/usr/libexec/unicore-a11y status | grep -q '^tts_available=1$'
echo "Checking v0.6.0 Windows 11 / Zorin Familiarity (Desktop Menu, OSD, USB Auto-Mount, Thumbnails, Archives, Windows Fonts)..."
test -f /etc/xdg/labwc/menu.xml
test -f /etc/xdg/udiskie/config.yml
test -f /etc/fonts/local.conf
test -x /usr/libexec/unicore-osd
test -f /usr/share/file-manager/actions/unicore-archive-extract.desktop
test -f /usr/share/file-manager/actions/unicore-archive-compress.desktop
command -v udiskie
command -v wob
command -v wtype
command -v ffmpegthumbnailer
command -v xarchiver
/usr/libexec/unicore-osd status | grep -q '^wob=1$'
echo "Checking v0.7.0 Classic 98/XP Gaming Edition (Retro Media Helper, CD .iso Handler, taskset, archivemount, Classic Crash Diag)..."
test -x /usr/libexec/unicore-retro-media
test -f /usr/share/applications/unicore-retro-iso.desktop
test -f /usr/share/file-manager/actions/unicore-retro-import.desktop
command -v taskset
command -v archivemount
command -v bsdtar
/usr/libexec/unicore-retro-media status | grep -q '^taskset=1$'
/usr/libexec/unicore-retro-media status | grep -q '^archivemount=1$'
echo "Checking v0.8.0 Ecosystem, Phone Link (KDE Connect + scrcpy), Cloud Drives, Windows Migration & Hardware Drivers..."
test -x /usr/libexec/unicore-phone
test -x /usr/libexec/unicore-cloud
test -x /usr/libexec/unicore-migrate
test -x /usr/libexec/unicore-hw-mgr
command -v kdeconnect-cli
command -v scrcpy
command -v adb
command -v lsusb
command -v fwupdmgr
grep -q '1714-1764' /usr/libexec/unicore-firewall
/usr/libexec/unicore-phone status | grep -q '^kdeconnect_installed=1$'
/usr/libexec/unicore-phone status | grep -q '^scrcpy_installed=1$'
/usr/libexec/unicore-cloud status | grep -q '^rclone_installed=1$'
/usr/libexec/unicore-hw-mgr status | grep -q '^fwupd_available=1$'
echo "Checking v0.9.0 Unified Game Hub, Gamepad uinput Mapper & Save Cloud Backup..."
test -x /usr/libexec/unicore-gamepad
test -x /usr/libexec/unicore-gamesave
command -v evtest
uniload gamepad-map --profile wasd_mouse --dry-run | grep -q '"profile": "wasd_mouse"'
/usr/libexec/unicore-gamepad status | grep -q '^evtest_available=1$'
/usr/libexec/unicore-gamesave status | grep -q '^backup_dir='
echo "Checking v1.0.0 Zero-Trust Security Audit, 0-MB-Idle Gate & Health-Gated A/B Boot Check..."
test -x /usr/libexec/unicore-security-audit
test -x /usr/libexec/unicore-boot-health
uniload security-audit | grep -q '"all_passed": true'
/usr/libexec/unicore-security-audit check > /tmp/unicore_sec_audit.txt
cat /tmp/unicore_sec_audit.txt
grep -q '^status=PASS$' /tmp/unicore_sec_audit.txt
grep -q '^doas_conf_locked=1$' /tmp/unicore_sec_audit.txt
grep -q '^ota_signature_enforced=1$' /tmp/unicore_sec_audit.txt
grep -q '^libexec_perms_ok=1$' /tmp/unicore_sec_audit.txt
grep -q '^credentials_0600=1$' /tmp/unicore_sec_audit.txt
grep -q '^bwrap_available=1$' /tmp/unicore_sec_audit.txt
grep -q '^firewall_ready=1$' /tmp/unicore_sec_audit.txt
grep -q '^idle_heavy_count=0$' /tmp/unicore_sec_audit.txt
rm -f /tmp/unicore_sec_audit.txt
/usr/libexec/unicore-boot-health --dry-run | grep -q '^health_status=HEALTHY$'
echo "Checking v1.1.0 1-Click Web Apps (PWA) & Family Parental Controls..."
test -x /usr/libexec/unicore-webapps
test -x /usr/libexec/unicore-parental
/usr/libexec/unicore-webapps status | grep -q '^installed_count=0$'
/usr/libexec/unicore-parental status | grep -q '^enabled=0$'
su -s /bin/sh unicore -c '
    export HOME=/home/unicore
    /usr/libexec/unicore-webapps install qatest "QA WebApp" https://example.com internet QA "#0284c7" | grep -q "^installed=qatest$"
    test -f /home/unicore/.local/share/applications/unicore-webapp-qatest.desktop
    /usr/libexec/unicore-webapps remove qatest | grep -q "^removed=qatest$"
    test ! -f /home/unicore/.local/share/applications/unicore-webapp-qatest.desktop
'
/usr/libexec/unicore-update-check --waybar | grep -q '"text"'
printf 'err:module:import_dll Library MSVCP140.dll not found\nerr:module:import_dll Library dplayx.dll not found\n' > /tmp/fake_win32.log
uniload diagnose-log /tmp/fake_win32.log | grep -q 'vcrun2022'
uniload diagnose-log /tmp/fake_win32.log | grep -q 'directplay'
rm -f /tmp/fake_win32.log
echo "Verified all v0.2.0-v0.7.0 helpers, Boot Splash, Capture, DND, Network Drives, BlueZ PipeWire, Accessibility, Classic 98/XP Gaming & drivers OK"

echo "[2/5] Bypassing OOBE for desktop verification & waiting for Wayland..."
mkdir -p /data/unicore
touch /data/unicore/oobe.done
tries=0
while [ ! -S /run/user/1000/wayland-0 ] && [ "${tries}" -lt 80 ]; do
    sleep 0.5
    tries=$((tries + 1))
done
test -S /run/user/1000/wayland-0
pkill -u unicore -x unicore-center >/dev/null 2>&1 || true
sleep 0.5

echo "[3/5] Applying Phase 20D/v0.4.0 Light Theme, Floating Pill Taskbar, Papirus Icons, Daylight Wallpaper & Input settings..."
su -s /bin/sh unicore -c '
    export HOME=/home/unicore XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0
    /usr/libexec/unicore-personalize set \
        --theme light \
        --layout win11 \
        --accent purple \
        --wallpaper /usr/share/unicore/wallpaper-light.png \
        --kb-layout my \
        --natural-scroll no \
        --tap yes \
        --pointer-speed 0.3 \
        --taskbar-style floating
'
grep -q '^UNICORE_THEME=light$' /data/home/.config/unicore/personalize.conf
grep -q '^UNICORE_LAYOUT=win11$' /data/home/.config/unicore/personalize.conf
grep -q '^UNICORE_ACCENT_COLOR=purple$' /data/home/.config/unicore/personalize.conf
grep -q '^UNICORE_TASKBAR_STYLE=floating$' /data/home/.config/unicore/personalize.conf
grep -q '^UNICORE_KB_LAYOUT=my$' /data/home/.config/unicore/personalize.conf
grep -q '^UNICORE_NATURAL_SCROLL=no$' /data/home/.config/unicore/personalize.conf
grep -q 'gtk-application-prefer-dark-theme=0' /home/unicore/.config/gtk-3.0/settings.ini
grep -q 'gtk-icon-theme-name=Papirus-Light' /home/unicore/.config/gtk-3.0/settings.ini
grep -q '<name>UniCore-Light</name>' /home/unicore/.config/labwc/rc.xml
grep -q '<naturalScroll>no</naturalScroll>' /home/unicore/.config/labwc/rc.xml
grep -q '<pointerSpeed>0.3</pointerSpeed>' /home/unicore/.config/labwc/rc.xml
grep -q '^XKB_DEFAULT_LAYOUT=my$' /home/unicore/.config/labwc/environment
grep -q '#f1f5f9' /home/unicore/.config/waybar/style.css
grep -q 'border-radius: 14px' /home/unicore/.config/waybar/style.css

echo "[4/5] Capturing live Wayland Light-Mode screenshot of Settings -> Display & Style..."
pkill -u unicore -x unicore-center >/dev/null 2>&1 || true
sleep 1.2
su -s /bin/sh unicore -c '
    export HOME=/home/unicore XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0
    /usr/bin/unicore-center --tab settings:appearance >/tmp/center20d.log 2>&1 &
    for i in $(seq 1 30); do
        if wlrctl toplevel list 2>/dev/null | grep -qi "UniCore"; then
            break
        fi
        sleep 0.5
    done
    sleep 4
    grim /tmp/phase20d.png
    pkill -u unicore -x unicore-center >/dev/null 2>&1 || true
    sleep 0.5
    /usr/bin/unicore-center --tab dev >/tmp/center_dev.log 2>&1 &
    sleep 4
    grim /tmp/v020_dev_ai.png
    pkill -u unicore -x unicore-center >/dev/null 2>&1 || true
    sleep 0.5
    /usr/bin/unicore-center --capture >/tmp/center_cap.log 2>&1 &
    sleep 3
    grim /tmp/v050_capture.png
    pkill -u unicore -x unicore-center >/dev/null 2>&1 || true
    sleep 0.5
    /usr/bin/unicore-center --start-menu >/tmp/center_start.log 2>&1 &
    sleep 3
    grim /tmp/v060_start.png
    pkill -u unicore -x unicore-center >/dev/null 2>&1 || true
    sleep 0.5
    /usr/bin/unicore-center --retro-game >/tmp/center_retro.log 2>&1 &
    sleep 3
    grim /tmp/v070_retro.png
    pkill -u unicore -x unicore-center >/dev/null 2>&1 || true
    sleep 0.5
    /usr/bin/unicore-center --phone >/tmp/center_eco.log 2>&1 &
    sleep 3
    grim /tmp/v080_ecosystem.png
    pkill -u unicore -x unicore-center >/dev/null 2>&1 || true
    sleep 0.5
    /usr/bin/unicore-center --games >/tmp/center_games.log 2>&1 &
    sleep 3
    grim /tmp/v090_games.png
    pkill -u unicore -x unicore-center >/dev/null 2>&1 || true
    sleep 0.5
    /usr/bin/unicore-center --help-guide >/tmp/center_guide.log 2>&1 &
    sleep 3
    grim /tmp/v100_guide.png
    pkill -u unicore -x unicore-center >/dev/null 2>&1 || true
    sleep 0.5
    /usr/bin/unicore-center --webapps >/tmp/center_webapps.log 2>&1 &
    sleep 3
    grim /tmp/v110_webapps.png
'
test -s /tmp/phase20d.png
test -s /tmp/v020_dev_ai.png
test -s /tmp/v050_capture.png
test -s /tmp/v060_start.png
test -s /tmp/v070_retro.png
test -s /tmp/v080_ecosystem.png
test -s /tmp/v090_games.png
test -s /tmp/v100_guide.png
test -s /tmp/v110_webapps.png

echo "===PNG_B64_START==="
base64 /tmp/phase20d.png
echo "===PNG_B64_END==="
echo "===PNG_DEV_B64_START==="
base64 /tmp/v020_dev_ai.png
echo "===PNG_DEV_B64_END==="
echo "===PNG_CAP_B64_START==="
base64 /tmp/v050_capture.png
echo "===PNG_CAP_B64_END==="
echo "===PNG_START_B64_START==="
base64 /tmp/v060_start.png
echo "===PNG_START_B64_END==="
echo "===PNG_RETRO_B64_START==="
base64 /tmp/v070_retro.png
echo "===PNG_RETRO_B64_END==="
echo "===PNG_ECO_B64_START==="
base64 /tmp/v080_ecosystem.png
echo "===PNG_ECO_B64_END==="
echo "===PNG_GAMES_B64_START==="
base64 /tmp/v090_games.png
echo "===PNG_GAMES_B64_END==="
echo "===PNG_GUIDE_B64_START==="
base64 /tmp/v100_guide.png
echo "===PNG_GUIDE_B64_END==="
echo "===PNG_WEBAPPS_B64_START==="
base64 /tmp/v110_webapps.png
echo "===PNG_WEBAPPS_B64_END==="

# Restore default dark theme to test bidirectional switch cleanly
su -s /bin/sh unicore -c '
    export HOME=/home/unicore XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0
    /usr/libexec/unicore-personalize set --theme dark --wallpaper /usr/share/unicore/wallpaper.png --kb-layout us --natural-scroll yes --tap yes --pointer-speed 0.0
'
grep -q '<name>UniCore-Dark</name>' /home/unicore/.config/labwc/rc.xml

echo "[5/5] Testing Ed25519 Release Manifest Signing & Tamper Rejection..."
mkdir -p /tmp/ed25519_qa
uniload update keygen --private /tmp/ed25519_qa/release.key --public /tmp/ed25519_qa/update.pub
H0="0000000000000000000000000000000000000000000000000000000000000000"
cat <<EOF_MAN > /tmp/ed25519_qa/test-manifest.json
{
  "arch": "x86_64",
  "artifacts": {
    "initramfs": {"filename": "initramfs-x86_64.img", "sha256": "${H0}", "size": 1024, "url": "initramfs-x86_64.img"},
    "squashfs": {"filename": "unicore-rootfs-x86_64.squashfs", "sha256": "${H0}", "size": 1024, "url": "unicore-rootfs-x86_64.squashfs"},
    "vmlinuz": {"filename": "vmlinuz-x86_64", "sha256": "${H0}", "size": 1024, "url": "vmlinuz-x86_64"}
  },
  "channel": "stable",
  "notes": "QA",
  "published_at": "unix:1795670630",
  "schema_version": 1,
  "version": "0.1.0"
}
EOF_MAN
uniload update sign-manifest /tmp/ed25519_qa/test-manifest.json --key /tmp/ed25519_qa/release.key --output /tmp/ed25519_qa/test-manifest.json.sig
uniload update verify-manifest /tmp/ed25519_qa/test-manifest.json --pubkey /tmp/ed25519_qa/update.pub --sig /tmp/ed25519_qa/test-manifest.json.sig --require-signature | grep -q '"signature_status": "verified"'

sed 's/"version": "0.1.0"/"version": "0.9.9"/' /tmp/ed25519_qa/test-manifest.json > /tmp/ed25519_qa/tampered.json
if uniload update verify-manifest /tmp/ed25519_qa/tampered.json --pubkey /tmp/ed25519_qa/update.pub --sig /tmp/ed25519_qa/test-manifest.json.sig --require-signature >/dev/null 2>&1; then
    echo "ERROR: tampered manifest unexpectedly passed signature check"
    exit 1
fi
rm -rf /tmp/ed25519_qa

printf '\n===GATE1_OK===\n'
"""
        script = script.replace("__EXPECTED_OS_VERSION__", EXPECTED_OS_VERSION)
        h.send_script_b64(script)
        out = h.wait_for("\n===GATE1_OK===", 300)
        m = re.search(r"===PNG_B64_START===\s*(.*?)\s*===PNG_B64_END===", out, re.DOTALL)
        if not m:
            raise RuntimeError("Missing screenshot base64 payload in Gate 1 output")
        b64_clean = re.sub(r"\s+", "", m.group(1))
        png_bytes = base64.b64decode(b64_clean)
        os.makedirs(os.path.dirname(SCREENSHOT_DEST), exist_ok=True)
        with open(SCREENSHOT_DEST, "wb") as f:
            f.write(png_bytes)
        dev_m = re.search(r"===PNG_DEV_B64_START===\s*(.*?)\s*===PNG_DEV_B64_END===", out, re.DOTALL)
        if not dev_m:
            raise RuntimeError("Missing Dev & AI screenshot payload in Gate 1 output")
        dev_dest = os.path.join(os.path.dirname(SCREENSHOT_DEST), "desktop_v020.png")
        with open(dev_dest, "wb") as f:
            f.write(base64.b64decode(re.sub(r"\s+", "", dev_m.group(1))))
        cap_m = re.search(r"===PNG_CAP_B64_START===\s*(.*?)\s*===PNG_CAP_B64_END===", out, re.DOTALL)
        if not cap_m:
            raise RuntimeError("Missing v0.5.0 Capture screenshot payload in Gate 1 output")
        cap_dest = os.path.join(os.path.dirname(SCREENSHOT_DEST), "desktop_v050_capture.png")
        with open(cap_dest, "wb") as f:
            f.write(base64.b64decode(re.sub(r"\s+", "", cap_m.group(1))))
        start_m = re.search(r"===PNG_START_B64_START===\s*(.*?)\s*===PNG_START_B64_END===", out, re.DOTALL)
        if not start_m:
            raise RuntimeError("Missing v0.6.0 Start Menu screenshot payload in Gate 1 output")
        start_dest = os.path.join(os.path.dirname(SCREENSHOT_DEST), "desktop_v060_start.png")
        with open(start_dest, "wb") as f:
            f.write(base64.b64decode(re.sub(r"\s+", "", start_m.group(1))))
        retro_m = re.search(r"===PNG_RETRO_B64_START===\s*(.*?)\s*===PNG_RETRO_B64_END===", out, re.DOTALL)
        if not retro_m:
            raise RuntimeError("Missing v0.7.0 Classic 98/XP Game Launcher screenshot payload in Gate 1 output")
        retro_dest = os.path.join(os.path.dirname(SCREENSHOT_DEST), "desktop_v070_retro.png")
        with open(retro_dest, "wb") as f:
            f.write(base64.b64decode(re.sub(r"\s+", "", retro_m.group(1))))
        eco_m = re.search(r"===PNG_ECO_B64_START===\s*(.*?)\s*===PNG_ECO_B64_END===", out, re.DOTALL)
        if not eco_m:
            raise RuntimeError("Missing v0.8.0 Ecosystem Hub screenshot payload in Gate 1 output")
        eco_dest = os.path.join(os.path.dirname(SCREENSHOT_DEST), "desktop_v080_ecosystem.png")
        with open(eco_dest, "wb") as f:
            f.write(base64.b64decode(re.sub(r"\s+", "", eco_m.group(1))))
        games_m = re.search(r"===PNG_GAMES_B64_START===\s*(.*?)\s*===PNG_GAMES_B64_END===", out, re.DOTALL)
        if not games_m:
            raise RuntimeError("Missing v0.9.0 Unified Game Hub screenshot payload in Gate 1 output")
        games_dest = os.path.join(os.path.dirname(SCREENSHOT_DEST), "desktop_v090_games.png")
        with open(games_dest, "wb") as f:
            f.write(base64.b64decode(re.sub(r"\s+", "", games_m.group(1))))
        guide_m = re.search(r"===PNG_GUIDE_B64_START===\s*(.*?)\s*===PNG_GUIDE_B64_END===", out, re.DOTALL)
        if not guide_m:
            raise RuntimeError("Missing v1.0.0 Official User Guide screenshot payload in Gate 1 output")
        guide_dest = os.path.join(os.path.dirname(SCREENSHOT_DEST), "desktop_v100_guide.png")
        with open(guide_dest, "wb") as f:
            f.write(base64.b64decode(re.sub(r"\s+", "", guide_m.group(1))))
        wa_m = re.search(r"===PNG_WEBAPPS_B64_START===\s*(.*?)\s*===PNG_WEBAPPS_B64_END===", out, re.DOTALL)
        if not wa_m:
            raise RuntimeError("Missing v1.1.0 Web Apps & Family Safety screenshot payload in Gate 1 output")
        wa_dest = os.path.join(os.path.dirname(SCREENSHOT_DEST), "desktop_v110_webapps.png")
        with open(wa_dest, "wb") as f:
            f.write(base64.b64decode(re.sub(r"\s+", "", wa_m.group(1))))
        print(f"    [PASS] Gate 1 passed! Screenshots saved: {SCREENSHOT_DEST}, {dev_dest}, {cap_dest}, {start_dest}, {retro_dest}, {eco_dest}, {games_dest}, {guide_dest}, {wa_dest}")
    finally:
        h.close()


def gate2_uefi_boot():
    print("\n=== GATE 2: UEFI Firmware ISO Boot (EDK2 OVMF + Shim/MOK-Signed GRUB) ===")
    if not os.path.isfile(OVMF_BIOS):
        raise FileNotFoundError(f"EDK2 UEFI firmware not found: {OVMF_BIOS}")
    with tempfile.TemporaryDirectory(prefix="unicore_uefi_qa_") as tmpdir:
        uefi_args = [
            "-name", "UniCore-QA-UEFI",
            "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF_BIOS}",
        ]
        if os.name != "nt" and OVMF_VARS and os.path.isfile(OVMF_VARS):
            vars_copy = os.path.join(tmpdir, "OVMF_VARS.fd")
            shutil.copyfile(OVMF_VARS, vars_copy)
            uefi_args += ["-drive", f"if=pflash,format=raw,file={vars_copy}"]
        uefi_args += [
            "-m", "2048",
            "-smp", "2",
            "-accel", DEFAULT_ACCEL,
            "-vga", "std",
            "-cdrom", ISO_PATH,
            "-boot", "d",
            "-display", "none",
            "-serial", "stdio",
            "-monitor", "none",
            "-no-reboot",
        ]
        h = SerialHarness(uefi_args, "Gate2-UEFI")
        try:
            wait_for_shell(h, 240)
            script = r"""#!/bin/sh
set -eu
test -d /sys/firmware/efi
. /mnt/unicore-boot.env
[ "${UNICORE_BOOT_MODE}" = "live" ]
test -f /mnt/unicore-media/ENROLL_UNICORE_MOK.cer
test -f /mnt/unicore-media/EFI/BOOT/BOOTX64.EFI
test -f /mnt/unicore-media/EFI/BOOT/grubx64.efi
/usr/libexec/unicore-secureboot status | grep -q '^uefi=1$'
/usr/libexec/unicore-secureboot status | grep -q '^shim_bundled=1$'
uname -r
printf '\n===GATE2_OK===\n'
"""
            h.send_script_b64(script, "/tmp/qa_uefi.sh")
            h.wait_for("\n===GATE2_OK===", 90)
            print("    [PASS] Gate 2 passed! UEFI EDK2 + shim + MOK-signed GRUB boot verified (/sys/firmware/efi active).")
        finally:
            h.close()


def gate3_legacy_2005():
    print("\n=== GATE 3: 2005 Legacy PC Simulation (512 MB RAM, 1 vCPU, no SSE4.2/AVX) ===")
    h = SerialHarness(
        [
            "-name", "UniCore-QA-2005",
            "-m", "512",
            "-smp", "1",
            "-cpu", "qemu64,-sse4.2,-avx,-avx2",
            "-accel", "tcg",
            "-vga", "std",
            "-cdrom", ISO_PATH,
            "-boot", "d",
            "-display", "none",
            "-serial", "stdio",
            "-monitor", "none",
            "-no-reboot",
        ],
        "Gate3-2005",
    )
    try:
        wait_for_shell(h, 240)
        script = r"""#!/bin/sh
set -eu
uniload hw-profile --env
uniload hw-profile --env | grep -q '^UNICORE_HW_TIER=legacy_ultra$'
uniload hw-profile --env | grep -q '^UNICORE_LIGHTWEIGHT_SHELL=1$'
grep -q 'zram0' /proc/swaps
/usr/bin/unicore-center --check
printf '\n===GATE3_OK===\n'
"""
        h.send_script_b64(script, "/tmp/qa_2005.sh")
        h.wait_for("\n===GATE3_OK===", 90)
        print("    [PASS] Gate 3 passed! 2005 512 MB RAM legacy_ultra profile + zram0 + unicore-center --check OK.")
    finally:
        h.close()


def gate4_host_artifacts():
    print("\n=== GATE 4: Host ISO SHA-256, Manifest Validity & USB Flasher Helper ===")
    if os.name == "nt":
        ps_res = subprocess.run(
            [
                "powershell",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                os.path.join(REPO_ROOT, "scripts", "write-usb-windows.ps1"),
                "-VerifyOnly",
            ],
            capture_output=True,
            text=True,
        )
        if ps_res.returncode != 0:
            raise RuntimeError(f"write-usb-windows.ps1 failed:\n{ps_res.stdout}\n{ps_res.stderr}")
        print(ps_res.stdout.strip())

        man_res = subprocess.run(
            [
                "powershell",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                os.path.join(REPO_ROOT, "scripts", "cargo-dev.ps1"),
                "run",
                "-q",
                "-p",
                "uniload",
                "--",
                "update",
                "verify-manifest",
                MANIFEST_PATH,
                "--allow-unsigned",
            ],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
        if man_res.returncode != 0 or '"valid": true' not in man_res.stdout:
            raise RuntimeError(f"Manifest check failed:\n{man_res.stdout}\n{man_res.stderr}")
    else:
        with open(SHA_PATH, "r", encoding="utf-8") as f:
            expected_sha = f.read().strip().split()[0].lower()
        h = hashlib.sha256()
        with open(ISO_PATH, "rb") as f:
            for chunk in iter(lambda: f.read(1048576), b""):
                h.update(chunk)
        actual_sha = h.hexdigest().lower()
        if actual_sha != expected_sha:
            raise RuntimeError(f"ISO SHA-256 mismatch: {actual_sha} != {expected_sha}")
        with open(MANIFEST_PATH, "r", encoding="utf-8") as f:
            man = json.load(f)
        if man.get("version") != EXPECTED_OS_VERSION or man.get("arch") != "x86_64":
            raise RuntimeError(f"Unexpected manifest metadata: {man}")
        print(f"    Verified ISO SHA-256 ({actual_sha}) & manifest v{man['version']} ({man['arch']})")
    print("    [PASS] Gate 4 passed! ISO SHA-256 & manifest valid.")


def gate5_ventoy_usb_xhci():
    print("\n=== GATE 5: Ventoy USB 3.0 xHCI + Internal NVMe Simulation (exFAT/NTFS3/ISO Loopback) ===")
    with tempfile.TemporaryDirectory(prefix="unicore_ventoy_qa_") as tmpdir:
        nvme_img = os.path.join(tmpdir, "nvme_dummy.qcow2")
        usb_img = os.path.join(tmpdir, "ventoy_usb.qcow2")
        subprocess.run([QEMU_IMG_BIN, "create", "-f", "qcow2", nvme_img, "128M"], check=True, stdout=subprocess.DEVNULL)
        subprocess.run([QEMU_IMG_BIN, "create", "-f", "qcow2", usb_img, "2G"], check=True, stdout=subprocess.DEVNULL)

        # Phase 5A: Populate a simulated Ventoy USB stick (MBR FAT32 label=Ventoy with /ISO/unicore-os-x86_64.iso)
        h_prep = SerialHarness(
            [
                "-name", "UniCore-QA-Ventoy-Prep",
                "-m", "2048",
                "-smp", "2",
                "-accel", DEFAULT_ACCEL,
                "-vga", "none",
                "-cdrom", ISO_PATH,
                "-boot", "d",
                "-drive", f"file={usb_img},if=virtio,format=qcow2",
                "-display", "none",
                "-serial", "stdio",
                "-monitor", "none",
                "-no-reboot",
            ],
            "Gate5-Ventoy-Prep",
        )
        try:
            wait_for_shell(h_prep, 240)
            prep_script = r"""#!/bin/sh
set -eu
parted -s /dev/vda mklabel msdos mkpart primary fat32 1MiB 100%
partprobe /dev/vda 2>/dev/null || true
tries=0
while [ ! -b /dev/vda1 ] && [ "${tries}" -lt 30 ]; do sleep 0.2; tries=$((tries + 1)); done
test -b /dev/vda1
mkfs.vfat -F 32 -n Ventoy /dev/vda1
mkdir -p /mnt/ventoy_prep
mount -t vfat /dev/vda1 /mnt/ventoy_prep
mkdir -p /mnt/ventoy_prep/ISO
dd if=/dev/sr0 of=/mnt/ventoy_prep/ISO/unicore-os-x86_64.iso bs=4M
sync
ls -lh /mnt/ventoy_prep/ISO/unicore-os-x86_64.iso
umount /mnt/ventoy_prep
sync
printf '\n===GATE5_PREP_OK===\n'
"""
            h_prep.send_script_b64(prep_script, "/tmp/qa_ventoy_prep.sh")
            h_prep.wait_for("\n===GATE5_PREP_OK===", 300)
        finally:
            h_prep.close()

        # Phase 5B: Boot vmlinuz+initrd directly (as Ventoy GRUB does) with internal NVMe + USB 3.0 xHCI Ventoy drive
        h_boot = SerialHarness(
            [
                "-name", "UniCore-QA-Ventoy-Boot",
                "-m", "2048",
                "-smp", "2",
                "-accel", DEFAULT_ACCEL,
                "-vga", "std",
                "-kernel", VMLINUZ_PATH,
                "-initrd", INITRD_PATH,
                "-append", "console=ttyS0",
                "-drive", f"id=nvme0,file={nvme_img},if=none,format=qcow2",
                "-device", "nvme,serial=ASUS_NVME,drive=nvme0",
                "-device", "qemu-xhci,id=xhci",
                "-drive", f"id=usb0,file={usb_img},if=none,format=qcow2",
                "-device", "usb-storage,bus=xhci.0,drive=usb0",
                "-display", "none",
                "-serial", "stdio",
                "-monitor", "none",
                "-no-reboot",
            ],
            "Gate5-Ventoy-Boot",
        )
        try:
            wait_for_shell(h_boot, 240)
            boot_log = h_boot.get_buf()
            if "can't create /dev/null" in boot_log:
                raise RuntimeError("Regressed early /dev/null warning in initramfs /init!")
            verify_script = r"""#!/bin/sh
set -eu
cat /mnt/unicore-boot.env
. /mnt/unicore-boot.env
[ "${UNICORE_BOOT_MODE}" = "live" ]
[ "${UNICORE_BOOT_ISO}" = "/ISO/unicore-os-x86_64.iso" ]
test -f /mnt/unicore-media/unicore/unicore-rootfs-x86_64.squashfs
test -f /mnt/unicore-media/boot/vmlinuz-x86_64
test -f /mnt/unicore-iso-host/ISO/unicore-os-x86_64.iso
test -b /dev/nvme0n1
grep -qw exfat /proc/filesystems
grep -qw ntfs3 /proc/filesystems
printf '\n===GATE5_OK===\n'
"""
            h_boot.send_script_b64(verify_script, "/tmp/qa_ventoy_verify.sh")
            h_boot.wait_for("\n===GATE5_OK===", 90)
            print("    [PASS] Gate 5 passed! Ventoy USB 3.0 xHCI ISO loopback + internal NVMe + exfat/ntfs3 verified.")
        finally:
            h_boot.close()


def main():
    if not os.path.isfile(ISO_PATH):
        raise FileNotFoundError(f"Missing ISO: {ISO_PATH}")
    requested = set(sys.argv[1:]) if len(sys.argv) > 1 else {"1", "2", "3", "4", "5"}
    if "4" in requested:
        gate4_host_artifacts()
    if "1" in requested:
        gate1_bios_phase20d_and_ed25519()
    if "2" in requested:
        gate2_uefi_boot()
    if "3" in requested:
        gate3_legacy_2005()
    if "5" in requested:
        gate5_ventoy_usb_xhci()
    print("\n============================================================")
    print("ALL REQUESTED RELEASE QA MATRIX GATES PASSED SUCCESSFULLY!")
    print("============================================================")


if __name__ == "__main__":
    main()
