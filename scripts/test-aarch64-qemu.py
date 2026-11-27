#!/usr/bin/env python3
"""UniCore OS ARM64 (aarch64) UEFI + Box64/Wine Isolated Runtime QEMU QA."""

import os
import subprocess
import sys
import threading
import time

REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
OUT_DIR = os.path.join(REPO_ROOT, "os-builder", "out")
ISO_PATH = os.path.join(OUT_DIR, "unicore-os-aarch64.iso")
QEMU_ARM64 = r"C:\Program Files\qemu\qemu-system-aarch64.exe"
OVMF_ARM64 = r"C:\Program Files\qemu\share\edk2-aarch64-code.fd"


def main():
    if not os.path.isfile(ISO_PATH):
        raise FileNotFoundError(f"Missing ARM64 ISO: {ISO_PATH}")
    if not os.path.isfile(QEMU_ARM64):
        raise FileNotFoundError(f"Missing QEMU ARM64 binary: {QEMU_ARM64}")
    if not os.path.isfile(OVMF_ARM64):
        raise FileNotFoundError(f"Missing ARM64 UEFI firmware: {OVMF_ARM64}")

    cmd = [
        QEMU_ARM64,
        "-name", "UniCore-QA-ARM64",
        "-M", "virt",
        "-cpu", "cortex-a57",
        "-m", "2048",
        "-smp", "2",
        "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF_ARM64}",
        "-device", "virtio-scsi-pci,id=scsi",
        "-drive", f"id=cd0,if=none,media=cdrom,readonly=on,file={ISO_PATH}",
        "-device", "scsi-cd,drive=cd0",
        "-display", "none",
        "-serial", "stdio",
        "-monitor", "none",
        "-no-reboot",
    ]
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
                raise RuntimeError(f"ARM64 QEMU exited early:\n{text[-1500:]}")
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
            "grep -q 'unicore-manifest-aarch64.json' /etc/unicore/update.conf\n"
            "test -x /usr/share/unicore/arm64-win32/run-wine-box64.sh\n"
            "/usr/share/unicore/arm64-win32/run-wine-box64.sh --version\n"
            "/usr/bin/unicore-center --check\n"
            "echo ===ARM64_QA_OK===\n"
            "poweroff -f\n"
        )
        proc.stdin.write(check_cmd.encode("utf-8"))
        proc.stdin.flush()

        deadline = time.time() + 180
        while time.time() < deadline:
            with lock:
                text = "".join(buf)
            if "===ARM64_QA_OK===" in text:
                print("[PASS] ARM64 UEFI ISO + Box64/Wine x64 isolated runtime + unicore-center verified!")
                return
            if proc.poll() is not None:
                raise RuntimeError(f"ARM64 QA failed before marker:\n{text[-1500:]}")
            time.sleep(0.5)
        raise TimeoutError("Timed out waiting for ===ARM64_QA_OK===")
    finally:
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()


if __name__ == "__main__":
    main()
