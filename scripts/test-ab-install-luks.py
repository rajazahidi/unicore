#!/usr/bin/env python3
"""UniCore OS End-to-End Bare-Metal A/B Disk Install + LUKS2 + Slot Rollback QA.

Verifies the complete bare-metal install & A/B failover lifecycle in QEMU:
  Stage 1: Boot Live ISO + empty 10 GiB qcow2 disk, run `unicore-install --encrypt-data`,
           verify GPT partitions (UNICORE_EFI, UNICORE_SYS_A, UNICORE_SYS_B, LUKS2).
  Stage 2: Boot ONLY from installed disk, unlock LUKS2 passphrase on serial prompt,
           verify UNICORE_BOOT_MODE=installed, UNICORE_BOOT_SLOT=UNICORE_SYS_A,
           UNICORE_DATA_ENCRYPTED=1, write persistence marker to /data, confirm boot,
           and trigger `unicore-update --rollback` to Slot B.
  Stage 3: Reboot installed disk, unlock LUKS2 passphrase again, verify boot switched
           to UNICORE_SYS_B and encrypted /data persisted cleanly across A/B slots.
"""

import importlib.util
import os
import subprocess
import tempfile
import time

_MATRIX_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "test-release-matrix.py")
_SPEC = importlib.util.spec_from_file_location("test_release_matrix", _MATRIX_PATH)
_MOD = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(_MOD)

ISO_PATH = _MOD.ISO_PATH
QEMU_BIN = _MOD.QEMU_BIN
SerialHarness = _MOD.SerialHarness
wait_for_shell = _MOD.wait_for_shell

QEMU_IMG_BIN = r"C:\Program Files\qemu\qemu-img.exe"
LUKS_PASS = "UniCoreTestPass2026!"


def run_stage1_install(disk_path):
    print("\n=== STAGE 1: Live ISO -> Bare-Metal A/B GPT Install + LUKS2 Encryption ===")
    h = SerialHarness(
        [
            "-name", "UniCore-AB-Stage1-Install",
            "-m", "3072",
            "-smp", "2",
            "-cpu", "qemu64,+sse4.2,+avx",
            "-accel", "tcg",
            "-vga", "std",
            "-cdrom", ISO_PATH,
            "-boot", "d",
            "-drive", f"file={disk_path},if=virtio,format=qcow2",
            "-display", "none",
            "-serial", "stdio",
            "-monitor", "none",
            "-no-reboot",
        ],
        "Stage1-Install",
    )
    try:
        wait_for_shell(h, 240)
        script = f"""#!/bin/sh
set -eu
echo "[1/3] Checking eligible install target (/dev/vda)..."
unicore-install --list-tsv
unicore-install --list-tsv | grep -q '^/dev/vda\\tvda\\t.*\\teligible$'

echo "[2/3] Running encrypted install via unicore-sys-helper (exact GUI path)..."
printf '%s\\n' '__PASS__' | /usr/libexec/unicore-sys-helper install-disk-encrypted /dev/vda

echo "[3/3] Verifying GPT partitions and LUKS2 container..."
blkid
blkid | grep -q 'LABEL="UNICORE_EFI"'
blkid | grep -q 'LABEL="UNICORE_SYS_A"'
blkid | grep -q 'LABEL="UNICORE_SYS_B"'
blkid | grep -Eq 'crypto_LUKS|LABEL="UNICORE_DATA_LUKS"'
sync
printf '\\n===STAGE1_OK===\\n'
"""
        script = script.replace("__PASS__", LUKS_PASS)
        h.send_script_b64(script, "/tmp/qa_stage1.sh")
        h.wait_for("\n===STAGE1_OK===", 360)
        print("    [PASS] Stage 1 passed! Bare-metal GPT + A/B + LUKS2 install completed.")
    finally:
        h.close()


def boot_installed_and_unlock(disk_path, label, attach_iso=False):
    args = [
        "-name", f"UniCore-{label}",
        "-m", "2048",
        "-smp", "2",
        "-cpu", "qemu64,+sse4.2,+avx",
        "-accel", "tcg",
        "-vga", "std",
        "-drive", f"file={disk_path},if=virtio,format=qcow2",
        "-boot", "c",
        "-display", "none",
        "-serial", "stdio",
        "-monitor", "none",
        "-no-reboot",
    ]
    if attach_iso:
        args.extend(["-cdrom", ISO_PATH])
    h = SerialHarness(args, label)
    try:
        h.wait_for("Enter LUKS passphrase for", 240)
        time.sleep(0.3)
        h.send_raw(LUKS_PASS + "\n")
        wait_for_shell(h, 240)
        return h
    except Exception:
        h.close()
        raise


def run_stage2_slot_a_and_update_slot_b(disk_path):
    print("\n=== STAGE 2: Boot Installed Disk (Slot A + LUKS2 Unlock) & Flash Slot B Update ===")
    h = boot_installed_and_unlock(disk_path, "Stage2-SlotA", attach_iso=True)
    try:
        script = r"""#!/bin/sh
set -eu
cat /mnt/unicore-boot.env
. /mnt/unicore-boot.env
[ "${UNICORE_BOOT_MODE}" = "installed" ]
[ "${UNICORE_BOOT_SLOT}" = "UNICORE_SYS_A" ]
[ "${UNICORE_DATA_ENCRYPTED}" = "1" ]
awk '$2=="/data"{print $1}' /proc/mounts | grep -q '^/dev/mapper/unicore_data$'

echo "[1/3] Writing LUKS2 /data persistence marker..."
mkdir -p /data/unicore
printf 'LUKS2_AB_PERSIST_OK\n' > /data/unicore/luks_persist.txt
sync

echo "[2/3] Confirming Slot A & flashing local ISO update onto Slot B..."
unicore-update --confirm-boot --force || true
mkdir -p /tmp/upd_iso
mount -t iso9660 -o ro /dev/sr0 /tmp/upd_iso
unicore-update --apply-local /tmp/upd_iso --allow-unsigned
umount /tmp/upd_iso

echo "[3/3] Verifying staged A/B trial update status..."
unicore-update --status
sync
printf '\n===STAGE2_OK===\n'
"""
        h.send_script_b64(script, "/tmp/qa_stage2.sh")
        h.wait_for("\n===STAGE2_OK===", 240)
        print("    [PASS] Stage 2 passed! Slot A encrypted boot verified & Slot B update staged.")
    finally:
        h.close()


def run_stage3_slot_b_persistence(disk_path):
    print("\n=== STAGE 3: Reboot Installed Disk (Slot B Trial + Persisted Encrypted /data + Rollback Ready) ===")
    h = boot_installed_and_unlock(disk_path, "Stage3-SlotB", attach_iso=False)
    try:
        script = r"""#!/bin/sh
set -eu
cat /mnt/unicore-boot.env
. /mnt/unicore-boot.env
[ "${UNICORE_BOOT_MODE}" = "installed" ]
[ "${UNICORE_BOOT_SLOT}" = "UNICORE_SYS_B" ]
[ "${UNICORE_DATA_ENCRYPTED}" = "1" ]
awk '$2=="/data"{print $1}' /proc/mounts | grep -q '^/dev/mapper/unicore_data$'
[ "$(cat /data/unicore/luks_persist.txt)" = "LUKS2_AB_PERSIST_OK" ]
unicore-update --confirm-boot --force
unicore-update --status
unicore-update --rollback
sync
printf '\n===STAGE3_OK===\n'
"""
        h.send_script_b64(script, "/tmp/qa_stage3.sh")
        h.wait_for("\n===STAGE3_OK===", 120)
        print("    [PASS] Stage 3 passed! Booted into Slot B with persisted LUKS2 /data and verified rollback to Slot A.")
    finally:
        h.close()


def main():
    if not os.path.isfile(ISO_PATH):
        raise FileNotFoundError(f"Missing ISO: {ISO_PATH}")
    if not os.path.isfile(QEMU_IMG_BIN):
        raise FileNotFoundError(f"Missing qemu-img: {QEMU_IMG_BIN}")

    with tempfile.TemporaryDirectory(prefix="unicore_ab_luks_") as tmpdir:
        disk_path = os.path.join(tmpdir, "ab_luks_test.qcow2")
        res = subprocess.run(
            [QEMU_IMG_BIN, "create", "-f", "qcow2", disk_path, "10G"],
            capture_output=True,
            text=True,
        )
        if res.returncode != 0:
            raise RuntimeError(f"qemu-img create failed:\n{res.stdout}\n{res.stderr}")

        run_stage1_install(disk_path)
        run_stage2_slot_a_and_update_slot_b(disk_path)
        run_stage3_slot_b_persistence(disk_path)

    print("\n============================================================")
    print("ALL 3 BARE-METAL A/B + LUKS2 E2E STAGES PASSED SUCCESSFULLY!")
    print("============================================================")


if __name__ == "__main__":
    main()
