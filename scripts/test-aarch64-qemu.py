#!/usr/bin/env python3
"""UniCore OS ARM64 (aarch64) UEFI + Secure Boot Shim + Box64/Wine Isolated Runtime QEMU QA."""

import os
import shutil
import subprocess
import tempfile
import threading
import time

REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
OUT_DIR = os.path.join(REPO_ROOT, "os-builder", "out")
ISO_PATH = os.path.join(OUT_DIR, "unicore-os-aarch64.iso")


def _find_first(candidates):
    for p in candidates:
        if p and os.path.isfile(p):
            return p
    return candidates[0] if candidates else ""


if os.name == "nt":
    QEMU_ARM64 = r"C:\Program Files\qemu\qemu-system-aarch64.exe"
    OVMF_ARM64 = r"C:\Program Files\qemu\share\edk2-aarch64-code.fd"
    OVMF_ARM64_VARS = ""
    USE_KVM = False
else:
    QEMU_ARM64 = shutil.which("qemu-system-aarch64") or "/usr/bin/qemu-system-aarch64"
    OVMF_ARM64 = _find_first([
        "/usr/share/AAVMF/AAVMF_CODE.fd",
        "/usr/share/qemu-efi-aarch64/QEMU_EFI.fd",
        "/usr/share/edk2/aarch64/QEMU_EFI.fd",
    ])
    OVMF_ARM64_VARS = _find_first([
        "/usr/share/AAVMF/AAVMF_VARS.fd",
    ])
    USE_KVM = os.access("/dev/kvm", os.R_OK | os.W_OK)


def main():
    if not os.path.isfile(ISO_PATH):
        raise FileNotFoundError(f"Missing ARM64 ISO: {ISO_PATH}")
    if not os.path.isfile(QEMU_ARM64):
        raise FileNotFoundError(f"Missing QEMU ARM64 binary: {QEMU_ARM64}")
    if not os.path.isfile(OVMF_ARM64):
        raise FileNotFoundError(f"Missing ARM64 UEFI firmware: {OVMF_ARM64}")

    with tempfile.TemporaryDirectory(prefix="unicore_arm64_qa_") as tmpdir:
        cmd = [
            QEMU_ARM64,
            "-name", "UniCore-QA-ARM64",
            "-M", "virt",
            "-cpu", "host" if USE_KVM else "cortex-a57",
            "-accel", "kvm" if USE_KVM else "tcg",
            "-m", "2048",
            "-smp", "2",
            "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF_ARM64}",
        ]
        if OVMF_ARM64_VARS and os.path.isfile(OVMF_ARM64_VARS):
            vars_copy = os.path.join(tmpdir, "AAVMF_VARS.fd")
            shutil.copyfile(OVMF_ARM64_VARS, vars_copy)
            cmd += ["-drive", f"if=pflash,format=raw,file={vars_copy}"]
        cmd += [
            "-device", "virtio-scsi-pci,id=scsi",
            "-drive", f"id=cd0,if=none,media=cdrom,readonly=on,file={ISO_PATH}",
            "-device", "scsi-cd,drive=cd0",
            "-display", "none",
            "-serial", "stdio",
            "-monitor", "none",
            "-no-reboot",
        ]
        print(f"Launching ARM64 UEFI QA (accel={'kvm' if USE_KVM else 'tcg'}, fw={OVMF_ARM64})...", flush=True)
        proc = subprocess.Popen(
            cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, bufsize=0
        )
        buf = []
        lock = threading.Lock()

        def reader():
            while True:
                ch = proc.stdout.read(1)
                if not ch:
                    break
                with lock:
                    buf.append(ch.decode("utf-8", errors="replace"))

        threading.Thread(target=reader, daemon=True).start()
        try:
            deadline = time.time() + 300
            while time.time() < deadline:
                with lock:
                    text = "".join(buf)
                if "unicore:~#" in text or "~ # " in text:
                    break
                if proc.poll() is not None:
                    raise RuntimeError(f"ARM64 QEMU exited early:\n{text[-2000:]}")
                time.sleep(0.5)
            else:
                with lock:
                    tail = "".join(buf)[-2000:]
                raise TimeoutError(f"Timed out waiting for ARM64 shell:\n{tail}")

            check_script = """#!/bin/sh
set -eu
echo "[ARM64-QA] 1/4 Verifying UEFI, Secure Boot MOK, OTA & v0.4.0-v0.7.0 helpers..."
uname -m | grep -q '^aarch64$'
test -d /sys/firmware/efi
test -f /etc/unicore/update.pub
test -f /etc/unicore/mok.der
test -f /mnt/unicore-media/ENROLL_UNICORE_MOK.cer
test -f /mnt/unicore-media/EFI/BOOT/BOOTAA64.EFI
test -f /mnt/unicore-media/EFI/BOOT/grubaa64.efi
grep -q 'unicore-manifest-aarch64.json' /etc/unicore/update.conf
grep -q '^AUTO_CHECK=1' /etc/unicore/update.conf
/usr/libexec/unicore-secureboot status | grep -q '^uefi=1$'
/usr/libexec/unicore-secureboot status | grep -q '^shim_bundled=1$'
/usr/libexec/unicore-browser-mgr status | grep -q '^default_browser=epiphany$'
/usr/libexec/unicore-flatpak status | grep -q '^installed=1$'
/usr/libexec/unicore-proton status | grep -q '^dir=/data/gaming/proton-ge$'
/usr/libexec/unicore-ai status | grep -q '^provider=ollama$'
/usr/libexec/unicore-ai context | grep -q 'UniCore OS Live Read-Only System Diagnostics'
test -f /etc/udev/rules.d/60-unicore-gamepads.rules
test -d /usr/share/icons/Papirus-Dark
test -s /usr/share/unicore/splash.ppm
/usr/libexec/unicore-capture status | grep -q '^wf_recorder=1$'
/usr/libexec/unicore-notify status | grep -q '^dnd='
/usr/libexec/unicore-a11y status | grep -q '^tts_available=1$'
test -d /usr/lib/spa-0.2/bluez5
test -f /etc/xdg/labwc/menu.xml
test -f /etc/fonts/local.conf
test -x /usr/libexec/unicore-osd
command -v udiskie >/dev/null
command -v wob >/dev/null
command -v wtype >/dev/null
command -v tlp >/dev/null
test -x /usr/libexec/unicore-retro-media
command -v taskset >/dev/null
command -v archivemount >/dev/null
/usr/libexec/unicore-retro-media status | grep -q '^taskset=1$'
test -x /usr/libexec/unicore-phone
test -x /usr/libexec/unicore-cloud
test -x /usr/libexec/unicore-migrate
test -x /usr/libexec/unicore-hw-mgr
command -v kdeconnect-cli >/dev/null
command -v scrcpy >/dev/null
command -v adb >/dev/null
command -v lsusb >/dev/null
command -v fwupdmgr >/dev/null
/usr/libexec/unicore-phone status | grep -q '^kdeconnect_installed=1$'
/usr/libexec/unicore-cloud status | grep -q '^rclone_installed=1$'
/usr/libexec/unicore-hw-mgr status | grep -q '^fwupd_available=1$'
test -x /usr/libexec/unicore-gamepad
test -x /usr/libexec/unicore-gamesave
command -v evtest >/dev/null
uniload gamepad-map --profile dos_classic --dry-run | grep -q '"profile": "dos_classic"'
test -x /usr/libexec/unicore-security-audit
test -x /usr/libexec/unicore-boot-health
uniload security-audit | grep -q '"all_passed": true'
/usr/libexec/unicore-security-audit check | grep -q '^status=PASS$'
/usr/libexec/unicore-security-audit check | grep -q '^idle_heavy_count=0$'
/usr/libexec/unicore-boot-health --dry-run | grep -q '^health_status=HEALTHY$'
test -x /usr/libexec/unicore-webapps
test -x /usr/libexec/unicore-parental
/usr/libexec/unicore-webapps status | grep -q '^installed_count=0$'
/usr/libexec/unicore-parental status | grep -q '^enabled=0$'
test -f /etc/modprobe.d/60-unicore-hardware.conf
command -v sensors >/dev/null
command -v ddcutil >/dev/null
command -v v4l2-ctl >/dev/null
command -v boltctl >/dev/null
command -v amixer >/dev/null
command -v hdparm >/dev/null
test -f /lib/firmware/regulatory.db -o -f /usr/lib/firmware/regulatory.db
/usr/libexec/unicore-hw-mgr status | grep -q '^ddc_available=1$'
/usr/libexec/unicore-hw-mgr status | grep -q '^missing_firmware=none$'
test "$(/usr/libexec/unicore-personalize list-presets | wc -l)" -eq 10
test -s /usr/share/unicore/wallpaper-cyberpunk.png
test -f /usr/share/themes/UniCore-Classic98/openbox-3/themerc

echo "[ARM64-QA] 2/4 Verifying Box64 + Wine 11.0 WoW64 binaries..."
test -x /usr/share/unicore/arm64-win32/run-wine-box64.sh
/usr/share/unicore/arm64-win32/bin/box64 --version
/usr/share/unicore/arm64-win32/run-wine-box64.sh --version

echo "[ARM64-QA] 3/4 Running live Win32 PE cmd.exe via Box64 + Wine 11.0 WoW64..."
mkdir -p /tmp/xdg_qa && chmod 700 /tmp/xdg_qa
rc=0
BOX64_DYNAREC_STRONGMEM=1 XDG_RUNTIME_DIR=/tmp/xdg_qa DISPLAY= WAYLAND_DISPLAY= \
  WINEDEBUG=-all WINEDLLOVERRIDES='mscoree,mshtml,winemenubuilder.exe,winewayland.drv,winex11.drv=' \
  WINEPREFIX=/tmp/qa_pfx \
  timeout -s KILL 420 /usr/share/unicore/arm64-win32/run-wine-box64.sh \
  cmd /c 'echo UNICORE_ARM64_LIVE_WIN32_OK' >/tmp/win32_qa.log 2>&1 || rc=$?
echo "--- /tmp/win32_qa.log (rc=${rc}) ---"
cat /tmp/win32_qa.log || true
echo "--- end log ---"
[ "${rc}" -eq 0 ]
grep -q 'UNICORE_ARM64_LIVE_WIN32_OK' /tmp/win32_qa.log

echo "[ARM64-QA] 4/4 Verifying unicore-center --check..."
/usr/bin/unicore-center --check
echo ===ARM64_QA_OK===
poweroff -f
"""
            payload = (
                "\ncat <<'EOF_ARM64_QA' > /tmp/qa_arm64.sh\n"
                + check_script
                + "EOF_ARM64_QA\n"
                + "chmod +x /tmp/qa_arm64.sh && sh /tmp/qa_arm64.sh || { echo ===ARM64_QA_FAIL===; poweroff -f; }\n"
            )
            for line in payload.splitlines(True):
                proc.stdin.write(line.encode("utf-8"))
                proc.stdin.flush()
                time.sleep(0.02)

            deadline = time.time() + 540
            while time.time() < deadline:
                with lock:
                    text = "".join(buf)
                if "===ARM64_QA_OK===" in text:
                    print("[PASS] ARM64 UEFI ISO + Shim/MOK GRUB + Box64/Wine 11.0 WoW64 live cmd.exe + unicore-center verified!")
                    return
                if "===ARM64_QA_FAIL===" in text:
                    raise RuntimeError(f"ARM64 QA script reported failure:\n{text[-3500:]}")
                if proc.poll() is not None:
                    raise RuntimeError(f"ARM64 QA failed before marker:\n{text[-3500:]}")
                time.sleep(0.5)
            with lock:
                tail = "".join(buf)[-3500:]
            raise TimeoutError(f"Timed out waiting for ===ARM64_QA_OK===:\n{tail}")
        finally:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()


if __name__ == "__main__":
    main()
