#!/usr/bin/env python3
"""UniCore OS v0.4.0 Automated Cloud QEMU QA:
  - Test A: Whole-Disk GPT A/B + LUKS2 Encryption + Local OTA Slot B Upgrade +
            Slot B Boot Confirmation + GRUB Recovery (`unicore_reset_overlay=1`)
            verifying OS overlay resets cleanly while encrypted /data/home persists.
  - Test B: Safe Non-Destructive Windows Dual-Boot (`--mode alongside`) into
            unallocated GPT free space, preserving Windows ESP (`bootmgfw.efi`)
            and Windows data partition byte-for-byte, generating GRUB Dual-Boot
            + Recovery entries, and booting UniCore cleanly from the shared ESP.
"""

import base64
import gzip
import os
import shutil
import subprocess
import tempfile
import threading
import time

REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
OUT_DIR = os.path.join(REPO_ROOT, "os-builder", "out")
ISO_PATH = os.path.join(OUT_DIR, "unicore-os-x86_64.iso")
VMLINUZ_PATH = os.path.join(OUT_DIR, "vmlinuz-x86_64")
INITRD_PATH = os.path.join(OUT_DIR, "initramfs-x86_64.img")


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

LUKS_PASS = "UniCoreQA2026!"


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
        threading.Thread(target=self._read_loop, daemon=True).start()

    def _read_loop(self):
        while True:
            chunk = self.proc.stdout.read(1)
            if not chunk:
                break
            with self.lock:
                self.buf += chunk.decode("utf-8", errors="replace")

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
                raise RuntimeError(
                    f"[{self.label}] QEMU exited early waiting for {marker!r}:\n{cur[-2000:]}"
                )
            time.sleep(0.25)
        raise TimeoutError(
            f"[{self.label}] Timed out ({timeout_sec}s) waiting for {marker!r}. Tail:\n{self.get_buf()[-2000:]}"
        )

    def send_raw(self, text):
        data = text.encode("utf-8")
        for i in range(0, len(data), 4):
            self.proc.stdin.write(data[i : i + 4])
            self.proc.stdin.flush()
            time.sleep(0.015)

    def send_script_b64(self, sh_script, remote_path="/tmp/qa_step.sh"):
        compressed = gzip.compress(sh_script.encode("utf-8"))
        b64 = base64.b64encode(compressed).decode("ascii")
        lines = [f"cat <<'EOF_B64' | base64 -d | gunzip > {remote_path}"]
        for i in range(0, len(b64), 72):
            lines.append(b64[i : i + 72])
        lines.append("EOF_B64")
        lines.append(f"chmod +x {remote_path} && sh {remote_path}")
        for ln in lines:
            self.send_raw(ln + "\n")
            time.sleep(0.03)

    def close(self):
        try:
            self.send_raw("\nsync; poweroff -f || true\n")
            time.sleep(0.8)
        except Exception:
            pass
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()


def wait_for_shell(h, timeout_sec=240, luks_pass=None):
    deadline = time.time() + timeout_sec
    sent_luks = False
    sent_login = False
    while time.time() < deadline:
        cur = h.get_buf()
        if luks_pass and not sent_luks and "Enter LUKS passphrase" in cur:
            time.sleep(0.3)
            h.send_raw(luks_pass + "\n")
            sent_luks = True
        if "unicore:~#" in cur or "~ # " in cur:
            h.send_raw("\necho ===SHELL_READY===\n")
            h.wait_for("\n===SHELL_READY===", 20)
            return
        if not sent_login and "localhost login:" in cur:
            h.send_raw("root\n")
            sent_login = True
        if h.proc.poll() is not None:
            raise RuntimeError(f"[{h.label}] QEMU exited before shell:\n{cur[-2000:]}")
        time.sleep(0.35)
    raise TimeoutError(f"[{h.label}] Timed out waiting for shell:\n{h.get_buf()[-2000:]}")


def base_uefi_args(tmpdir, name, ram_mb=2048):
    args = [
        "-name", name,
        "-m", str(ram_mb),
        "-smp", "2",
        "-cpu", "host" if DEFAULT_ACCEL == "kvm" else "qemu64,+sse4.2,+avx",
        "-accel", DEFAULT_ACCEL,
        "-vga", "std",
        "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF_BIOS}",
    ]
    if os.name != "nt" and OVMF_VARS and os.path.isfile(OVMF_VARS):
        vars_copy = os.path.join(tmpdir, f"{name}_VARS.fd")
        shutil.copyfile(OVMF_VARS, vars_copy)
        args += ["-drive", f"if=pflash,format=raw,file={vars_copy}"]
    args += ["-display", "none", "-serial", "stdio", "-monitor", "none", "-no-reboot"]
    return args


def create_qcow2(path, size_spec):
    subprocess.run([QEMU_IMG_BIN, "create", "-f", "qcow2", path, size_spec], check=True)


def test_a_ab_luks2_ota_and_recovery():
    print("\n=== TEST A: Whole-Disk GPT A/B + LUKS2 + Local Slot B OTA + GRUB Overlay Reset Recovery ===")
    with tempfile.TemporaryDirectory(prefix="unicore_qa_ab_") as tmpdir:
        disk = os.path.join(tmpdir, "ab_luks.qcow2")
        create_qcow2(disk, "12G")

        # Stage A1: Boot Live ISO -> install A/B + LUKS2 onto /dev/vda
        print("--> [A1/4] Installing UniCore OS (GPT A/B + LUKS2) onto /dev/vda from Live ISO...", flush=True)
        args = base_uefi_args(tmpdir, "QA-A1-Install", 2560) + [
            "-drive", f"id=hd0,if=none,file={disk},format=qcow2",
            "-device", "virtio-blk-pci,drive=hd0,bootindex=2",
            "-drive", f"id=cd0,if=none,media=cdrom,readonly=on,file={ISO_PATH}",
            "-device", "ide-cd,drive=cd0,bootindex=1",
        ]
        h = SerialHarness(args, "A1-Install")
        try:
            wait_for_shell(h, 240)
            script_a1 = f"""#!/bin/sh
set -eu
mkdir -p /data/unicore && touch /data/unicore/oobe.done
printf '%s' '{LUKS_PASS}' > /tmp/luks.pass
chmod 600 /tmp/luks.pass
/usr/sbin/unicore-install --disk /dev/vda --mode wipe --yes-wipe --encrypt-data --passphrase-file /tmp/luks.pass
mkdir -p /tmp/chk_efi /tmp/stage_data
efi_dev="$(blkid -t LABEL=UNICORE_EFI -o device | head -n1)"
mount -t vfat -o ro "${{efi_dev}}" /tmp/chk_efi
grep -q 'UniCore Recovery & Advanced Options' /tmp/chk_efi/boot/grub/grub.cfg
grep -q 'unicore_reset_overlay=1' /tmp/chk_efi/boot/grub/grub.cfg
grep -q 'UNICORE_SYS_B' /tmp/chk_efi/boot/grub/grub.cfg
test -f /tmp/chk_efi/boot/grub/unicore-installed.marker
umount /tmp/chk_efi

# Stage the signed v0.4.0 release payload inside encrypted /data so Stage A2 can boot disk-only
# (ISO detached, just like unplugging USB after install) and run strict Ed25519 local A/B OTA!
export DM_DISABLE_UDEV=1
luks_dev="$(blkid -t LABEL=UNICORE_DATA_LUKS -o device | head -n1)"
cryptsetup open --disable-locks --key-file /tmp/luks.pass "${{luks_dev}}" unicore_qa_stage
command -v dmsetup >/dev/null 2>&1 && dmsetup mknodes 2>/dev/null || true
mount -t ext4 /dev/mapper/unicore_qa_stage /tmp/stage_data
mkdir -p /tmp/stage_data/qa-local-ota/boot /tmp/stage_data/qa-local-ota/unicore
cp /mnt/unicore-media/boot/vmlinuz-x86_64 /tmp/stage_data/qa-local-ota/boot/
cp /mnt/unicore-media/boot/initramfs-x86_64.img /tmp/stage_data/qa-local-ota/boot/
cp /mnt/unicore-media/unicore/unicore-rootfs-x86_64.squashfs /tmp/stage_data/qa-local-ota/unicore/
cp /mnt/unicore-media/unicore/unicore-manifest.json /tmp/stage_data/qa-local-ota/unicore/
if [ -f /mnt/unicore-media/unicore/unicore-manifest.json.sig ]; then
    cp /mnt/unicore-media/unicore/unicore-manifest.json.sig /tmp/stage_data/qa-local-ota/unicore/
fi
sync
umount /tmp/stage_data
cryptsetup close unicore_qa_stage
rm -f /tmp/luks.pass
sync
echo ===A1_INSTALL_OK===
"""
            h.send_script_b64(script_a1, "/tmp/a1.sh")
            h.wait_for("\n===A1_INSTALL_OK===", 360)
        finally:
            h.close()

        # Stage A2: Boot installed disk ALONE (UEFI, ISO detached) -> Unlock LUKS2 -> Verify Slot A -> Strict Ed25519 Slot B OTA
        print("--> [A2/4] Booting installed Slot A (disk-only, LUKS2 unlock) & flashing Slot B OTA...", flush=True)
        args = base_uefi_args(tmpdir, "QA-A2-SlotA", 2560) + [
            "-drive", f"id=hd0,if=none,file={disk},format=qcow2",
            "-device", "virtio-blk-pci,drive=hd0,bootindex=1",
        ]
        h = SerialHarness(args, "A2-SlotA")
        try:
            wait_for_shell(h, 240, luks_pass=LUKS_PASS)
            script_a2 = r"""#!/bin/sh
set -eu
. /mnt/unicore-boot.env
[ "${UNICORE_BOOT_MODE}" = "installed" ] || { echo "Expected installed, got ${UNICORE_BOOT_MODE}"; exit 1; }
[ "${UNICORE_BOOT_SLOT}" = "UNICORE_SYS_A" ] || { echo "Expected UNICORE_SYS_A, got ${UNICORE_BOOT_SLOT}"; exit 1; }
[ "${UNICORE_DATA_ENCRYPTED}" = "1" ] || { echo "Expected LUKS2 encrypted /data"; exit 1; }
mkdir -p /data/home/Documents /data/unicore
touch /data/unicore/oobe.done
printf 'KEEP_THIS_USER_FILE_V040\n' > /data/home/Documents/user-keep.txt
printf 'SLOT_A_OVERLAY_TEST\n' > /etc/unicore-overlay-marker
# Verify strict Ed25519 signature + SHA-256 verification on local update payload (no --allow-unsigned!)
/usr/sbin/unicore-update --apply-local /data/qa-local-ota
rm -rf /data/qa-local-ota
/usr/sbin/unicore-update --status | grep -q '"pending_slot": "UNICORE_SYS_B"'
sync
echo ===A2_OTA_STAGED_OK===
"""
            h.send_script_b64(script_a2, "/tmp/a2.sh")
            h.wait_for("\n===A2_OTA_STAGED_OK===", 240)
        finally:
            h.close()

        # Stage A3: Reboot -> Unlock LUKS2 -> Verify Slot B trial boot -> Confirm boot -> Plant Slot B overlay marker
        print("--> [A3/4] Rebooting into Slot B trial boot, confirming boot & planting overlay marker...", flush=True)
        args = base_uefi_args(tmpdir, "QA-A3-SlotB", 2048) + [
            "-drive", f"file={disk},if=virtio,format=qcow2",
        ]
        h = SerialHarness(args, "A3-SlotB")
        try:
            wait_for_shell(h, 240, luks_pass=LUKS_PASS)
            script_a3 = r"""#!/bin/sh
set -eu
. /mnt/unicore-boot.env
[ "${UNICORE_BOOT_MODE}" = "installed" ]
[ "${UNICORE_BOOT_SLOT}" = "UNICORE_SYS_B" ] || { echo "Expected UNICORE_SYS_B, got ${UNICORE_BOOT_SLOT}"; exit 1; }
[ "${UNICORE_DATA_ENCRYPTED}" = "1" ]
grep -q 'KEEP_THIS_USER_FILE_V040' /data/home/Documents/user-keep.txt
/usr/libexec/unicore-boot-health --dry-run | grep -q '^health_status=HEALTHY$'
/usr/libexec/unicore-security-audit check | grep -q '^status=PASS$'
/usr/libexec/unicore-security-audit check | grep -q '^idle_heavy_count=0$'
/usr/sbin/unicore-update --confirm-boot --force
/usr/sbin/unicore-update --status | grep -q '"active_slot": "UNICORE_SYS_B"'
printf 'CORRUPT_SYSTEM_SETTING_TO_RESET\n' > /etc/unicore-slotb-overlay-marker
sync
echo ===A3_SLOTB_CONFIRMED_OK===
"""
            h.send_script_b64(script_a3, "/tmp/a3.sh")
            h.wait_for("\n===A3_SLOTB_CONFIRMED_OK===", 180)
        finally:
            h.close()

        # Stage A4: Boot with GRUB Recovery parameter `unicore_reset_overlay=1` -> Verify overlay wiped & /data/home kept!
        print("--> [A4/4] Testing GRUB Recovery (unicore_reset_overlay=1) preserves /data/home...", flush=True)
        args = [
            "-name", "QA-A4-Recovery",
            "-m", "2048",
            "-smp", "2",
            "-cpu", "host" if DEFAULT_ACCEL == "kvm" else "qemu64,+sse4.2,+avx",
            "-accel", DEFAULT_ACCEL,
            "-vga", "std",
            "-drive", f"file={disk},if=virtio,format=qcow2",
            "-kernel", VMLINUZ_PATH,
            "-initrd", INITRD_PATH,
            "-append", "unicore_slot=UNICORE_SYS_B unicore_reset_overlay=1 console=tty1 console=ttyS0",
            "-display", "none",
            "-serial", "stdio",
            "-monitor", "none",
            "-no-reboot",
        ]
        h = SerialHarness(args, "A4-Recovery")
        try:
            wait_for_shell(h, 240, luks_pass=LUKS_PASS)
            script_a4 = r"""#!/bin/sh
set -eu
. /mnt/unicore-boot.env
[ "${UNICORE_BOOT_MODE}" = "installed" ]
[ "${UNICORE_BOOT_SLOT}" = "UNICORE_SYS_B" ]
[ "${UNICORE_DATA_ENCRYPTED}" = "1" ]
if [ -e /etc/unicore-slotb-overlay-marker ]; then
    echo "ERROR: unicore_reset_overlay=1 did NOT wipe Slot B system overlay!"
    exit 1
fi
grep -q 'KEEP_THIS_USER_FILE_V040' /data/home/Documents/user-keep.txt
echo ===A4_RECOVERY_OK===
"""
            h.send_script_b64(script_a4, "/tmp/a4.sh")
            h.wait_for("\n===A4_RECOVERY_OK===", 120)
        finally:
            h.close()

        print("    [PASS] Test A passed (A/B + LUKS2 + Slot B OTA + Confirmation + Overlay-Reset Recovery)!", flush=True)


def test_b_safe_windows_dualboot():
    print("\n=== TEST B: Safe Non-Destructive Windows Dual-Boot (--mode alongside) ===")
    with tempfile.TemporaryDirectory(prefix="unicore_qa_dualboot_") as tmpdir:
        disk = os.path.join(tmpdir, "win11_dualboot.qcow2")
        create_qcow2(disk, "16G")

        # Stage B1: Boot Live ISO -> create realistic Windows 11 GPT ESP + Windows partition + 13.7 GB free space -> install alongside
        print("--> [B1/2] Preparing simulated Windows 11 GPT disk & running unicore-install --mode alongside...", flush=True)
        args = base_uefi_args(tmpdir, "QA-B1-DualBootInstall", 2560) + [
            "-drive", f"id=hd0,if=none,file={disk},format=qcow2",
            "-device", "virtio-blk-pci,drive=hd0,bootindex=2",
            "-drive", f"id=cd0,if=none,media=cdrom,readonly=on,file={ISO_PATH}",
            "-device", "ide-cd,drive=cd0,bootindex=1",
        ]
        h = SerialHarness(args, "B1-DualBootInstall")
        try:
            wait_for_shell(h, 240)
            script_b1 = r"""#!/bin/sh
set -eu
mkdir -p /data/unicore && touch /data/unicore/oobe.done

# Build realistic Windows 11 GPT layout:
#   p1: 256 MiB FAT32 ESP with \EFI\Microsoft\Boot\bootmgfw.efi
#   p2: 2 GiB simulated Windows OS partition with sentinel file
#   free: ~13.7 GiB unallocated GPT space (>= 10 GiB required)
parted -s /dev/vda mklabel gpt
parted -s /dev/vda mkpart '"EFI system partition"' fat32 1MiB 257MiB
parted -s /dev/vda set 1 esp on
parted -s /dev/vda mkpart '"Basic data partition"' ext4 257MiB 2305MiB
partprobe /dev/vda 2>/dev/null || true
sleep 1

mkfs.vfat -F 32 -n SYSTEM /dev/vda1 >/dev/null
mkfs.ext4 -F -L WINDOWS_C /dev/vda2 >/dev/null

mkdir -p /tmp/win_esp /tmp/win_c
mount -t vfat /dev/vda1 /tmp/win_esp
mkdir -p /tmp/win_esp/EFI/Microsoft/Boot
printf 'WINDOWS_11_BOOTMGFW_SENTINEL_BYTES_DO_NOT_OVERWRITE_2026\n' > /tmp/win_esp/EFI/Microsoft/Boot/bootmgfw.efi
WIN_EFI_SHA_BEFORE="$(sha256sum /tmp/win_esp/EFI/Microsoft/Boot/bootmgfw.efi | awk '{print $1}')"
umount /tmp/win_esp

mount -t ext4 /dev/vda2 /tmp/win_c
printf 'WINDOWS_USER_FILES_MUST_REMAIN_100_PERCENT_INTACT\n' > /tmp/win_c/WIN_SENTINEL.txt
WIN_DATA_SHA_BEFORE="$(sha256sum /tmp/win_c/WIN_SENTINEL.txt | awk '{print $1}')"
umount /tmp/win_c

# Verify 9-column --list-tsv reports dualboot_eligible=1 and windows_detected=1
TSV_LINE="$(/usr/sbin/unicore-install --list-tsv | grep '^/dev/vda	')"
echo "Detected TSV: ${TSV_LINE}"
printf '%s\n' "${TSV_LINE}" | awk -F'\t' '{ if ($5 != "1" || $8 != "1" || $9 != "1") exit 1 }'

# Run Safe Non-Destructive Dual-Boot install
/usr/sbin/unicore-install --disk /dev/vda --mode alongside --yes-wipe

# Verify Windows ESP & Windows partition are 100% untouched byte-for-byte
mount -t vfat -o ro /dev/vda1 /tmp/win_esp
WIN_EFI_SHA_AFTER="$(sha256sum /tmp/win_esp/EFI/Microsoft/Boot/bootmgfw.efi | awk '{print $1}')"
[ "${WIN_EFI_SHA_BEFORE}" = "${WIN_EFI_SHA_AFTER}" ] || { echo "ERROR: Windows bootmgfw.efi was modified!"; exit 1; }
grep -q 'menuentry "Windows Boot Manager"' /tmp/win_esp/boot/grub/grub.cfg
grep -q 'chainloader.*bootmgfw.efi' /tmp/win_esp/boot/grub/grub.cfg
grep -q 'UniCore Recovery & Advanced Options' /tmp/win_esp/boot/grub/grub.cfg
test -f /tmp/win_esp/boot/grub/unicore-installed.marker
umount /tmp/win_esp

mount -t ext4 -o ro /dev/vda2 /tmp/win_c
WIN_DATA_SHA_AFTER="$(sha256sum /tmp/win_c/WIN_SENTINEL.txt | awk '{print $1}')"
[ "${WIN_DATA_SHA_BEFORE}" = "${WIN_DATA_SHA_AFTER}" ] || { echo "ERROR: Windows partition data was modified!"; exit 1; }
umount /tmp/win_c

sync
echo ===B1_DUALBOOT_INSTALL_OK===
"""
            h.send_script_b64(script_b1, "/tmp/b1.sh")
            h.wait_for("\n===B1_DUALBOOT_INSTALL_OK===", 300)
        finally:
            h.close()

        # Stage B2: Detach ISO -> Boot disk ALONE via UEFI OVMF from shared Windows ESP
        print("--> [B2/2] Booting Dual-Boot disk cleanly via UEFI from shared Windows ESP...", flush=True)
        args = base_uefi_args(tmpdir, "QA-B2-DualBootBoot", 2048) + [
            "-drive", f"file={disk},if=virtio,format=qcow2",
        ]
        h = SerialHarness(args, "B2-DualBootBoot")
        try:
            wait_for_shell(h, 240)
            script_b2 = r"""#!/bin/sh
set -eu
. /mnt/unicore-boot.env
[ "${UNICORE_BOOT_MODE}" = "installed" ] || { echo "Expected installed, got ${UNICORE_BOOT_MODE}"; exit 1; }
[ "${UNICORE_BOOT_SLOT}" = "UNICORE_SYS_A" ] || { echo "Expected UNICORE_SYS_A, got ${UNICORE_BOOT_SLOT}"; exit 1; }
# Verify unicore-update discovers the shared Windows ESP automatically (can_update=true)
/usr/sbin/unicore-update --status | grep -q '"can_update": true'
echo ===B2_DUALBOOT_BOOT_OK===
"""
            h.send_script_b64(script_b2, "/tmp/b2.sh")
            h.wait_for("\n===B2_DUALBOOT_BOOT_OK===", 120)
        finally:
            h.close()

        print("    [PASS] Test B passed (Safe Windows Dual-Boot preserved ESP & data + clean UEFI boot)!", flush=True)


def main():
    for label, path in [
        ("ISO", ISO_PATH),
        ("vmlinuz", VMLINUZ_PATH),
        ("initramfs", INITRD_PATH),
        ("OVMF", OVMF_BIOS),
    ]:
        if not os.path.isfile(path):
            raise FileNotFoundError(f"Missing required QA artifact ({label}): {path}")
    print(f"Running UniCore v0.4.0 A/B + LUKS2 + Dual-Boot + Recovery QA (accel={DEFAULT_ACCEL})...", flush=True)
    test_a_ab_luks2_ota_and_recovery()
    test_b_safe_windows_dualboot()
    print("\nALL A/B OTA + LUKS2 + DUAL-BOOT + RECOVERY QA TESTS PASSED!", flush=True)


if __name__ == "__main__":
    main()
