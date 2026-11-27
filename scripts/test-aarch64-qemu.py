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

            check_cmd = (
                "\nset -e\n"
                "uname -m | grep -q '^aarch64$'\n"
                "test -d /sys/firmware/efi\n"
                "test -f /etc/unicore/update.pub\n"
                "test -f /etc/unicore/mok.der\n"
                "test -f /mnt/unicore-media/ENROLL_UNICORE_MOK.cer\n"
                "test -f /mnt/unicore-media/EFI/BOOT/BOOTAA64.EFI\n"
                "test -f /mnt/unicore-media/EFI/BOOT/grubaa64.efi\n"
                "grep -q 'unicore-manifest-aarch64.json' /etc/unicore/update.conf\n"
                "grep -q '^AUTO_CHECK=1' /etc/unicore/update.conf\n"
                "/usr/libexec/unicore-secureboot status | grep -q '^uefi=yes$'\n"
                "/usr/libexec/unicore-browser-mgr status | grep -q '^default='\n"
                "test -x /usr/share/unicore/arm64-win32/run-wine-box64.sh\n"
                "/usr/share/unicore/arm64-win32/bin/box64 --version\n"
                "/usr/share/unicore/arm64-win32/run-wine-box64.sh --version\n"
                "WINEDEBUG=-all WINEDLLOVERRIDES='mscoree,mshtml=' WINEPREFIX=/tmp/qa_pfx /usr/share/unicore/arm64-win32/run-wine-box64.sh cmd /c 'echo UNICORE_ARM64_LIVE_WIN32_OK' | grep -q 'UNICORE_ARM64_LIVE_WIN32_OK'\n"
                "/usr/bin/unicore-center --check\n"
                "echo ===ARM64_QA_OK===\n"
                "poweroff -f\n"
            )
            proc.stdin.write(check_cmd.encode("utf-8"))
            proc.stdin.flush()

            deadline = time.time() + 240
            while time.time() < deadline:
                with lock:
                    text = "".join(buf)
                if "===ARM64_QA_OK===" in text:
                    print("[PASS] ARM64 UEFI ISO + Shim/MOK GRUB + Box64/Wine 11.0 WoW64 live cmd.exe + unicore-center verified!")
                    return
                if proc.poll() is not None:
                    raise RuntimeError(f"ARM64 QA failed before marker:\n{text[-2500:]}")
                time.sleep(0.5)
            with lock:
                tail = "".join(buf)[-2500:]
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
