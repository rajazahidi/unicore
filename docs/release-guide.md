# UniCore OS (`0.2.1`) — Release, OTA & Hardware QA Guide

This guide covers managing the local dev environment, verifying, flashing, signing, serving OTA updates, and testing UniCore OS builds across **Legacy BIOS**, **UEFI**, **2005-era Low-Spec PCs**, and **Ventoy USB 3.0**.

---

## 1. Unified Environment Controller (Windows)

```powershell
# Audit host tools, disk space, Git hooks, QEMU/UEFI, ISO & Ventoy USB:
.\scripts\unicore-env.ps1 doctor

# Run full local CI checks (LF, clippy -D warnings, cargo test, musl, py_compile):
.\scripts\unicore-env.ps1 check

# Rebuild ISO incrementally (reuses existing SquashFS) or clean full build:
.\scripts\unicore-env.ps1 build-fast
.\scripts\unicore-env.ps1 build

# Deploy latest ISO directly onto connected Ventoy USB with SHA-256 verification:
.\scripts\unicore-env.ps1 deploy-ventoy

# Serve local LAN + QEMU HTTP OTA updates on port 8090:
.\scripts\unicore-env.ps1 ota-serve

# Import bare-metal diagnostic reports saved via `unicore-diag --save-usb`:
.\scripts\unicore-env.ps1 import-diag
```

---

## 2. Verify & Flash Hybrid ISO to USB (Windows)

```powershell
# Verify ISO SHA-256 & list removable USB drives safely (internal drives excluded):
.\scripts\write-usb-windows.ps1 -ListUsb

# SHA-256 integrity verification only:
.\scripts\write-usb-windows.ps1 -VerifyOnly
```

Recommended USB tools:
- **Ventoy** (copy `os-builder\out\unicore-os-x86_64.iso` onto the Ventoy `exFAT`/`NTFS`/`FAT32` drive — supported natively by Stage-1 ISO loopback)
- **Rufus** (`DD Image mode` recommended, or `ISO Image mode`)
- **Linux / macOS:** `sudo dd if=os-builder/out/unicore-os-x86_64.iso of=/dev/sdX bs=4M status=progress oflag=sync`

---

## 3. Ed25519 OTA Release Manifest Signing

UniCore OS A/B updates use SHA-256 artifact digests plus **Ed25519** signatures verified strictly when `REQUIRE_SIGNATURE=1` in `/etc/unicore/update.conf`:

```powershell
# Generate Ed25519 keypair (if absent), sign unicore-manifest.json, and verify strictly (requires OpenSSL on Windows):
.\scripts\sign-release.ps1 -GenerateKey
```

Or on Linux / inside UniCore OS directly:

```sh
uniload update keygen --private release.key --public update.pub
uniload update sign-manifest os-builder/out/unicore-manifest.json --key release.key --output os-builder/out/unicore-manifest.json.sig
uniload update verify-manifest os-builder/out/unicore-manifest.json --pubkey update.pub --sig os-builder/out/unicore-manifest.json.sig --require-signature
```

Embed `update.pub` into production OS images via:

```powershell
.\os-builder\bootstrap-via-qemu.ps1 -UpdatePubKey .\os-builder\keys\update.pub -SignKey .\os-builder\keys\release.key
```

---

## 4. Automated Release QA Matrix (5 Gates + A/B LUKS2)

Requires QEMU (`C:\Program Files\qemu\qemu-system-x86_64.exe` + `edk2-x86_64-code.fd`) and Python 3:

```powershell
.\scripts\unicore-env.ps1 qa
```

This runs:
1. **`scripts/test-release-matrix.py` (5 Gates):**
   - **Gate 1 — Legacy BIOS + Phase 20D/23/24/25 + Ed25519 E2E**: Boots GRUB BIOS from Live ISO, verifies `VERSION_ID="0.2.0"`, Phase 23–25 helpers, `ath9k`/`forcedeth`/`nouveau`/`sata_nv`/`snd_hda_intel` kernel drivers, Light/Dark themes, Malay keyboard layout, captures `desktop_phase20d.png` + `desktop_v020.png`, and verifies Ed25519 signing & tamper rejection.
   - **Gate 2 — UEFI Firmware (`edk2-x86_64-code.fd`)**: Boots GRUB EFI and confirms `/sys/firmware/efi`.
   - **Gate 3 — 2005 Legacy PC (`512 MB RAM`, 1 core, no SSE4.2/AVX)**: Confirms `legacy_ultra`, `zram0`, and `unicore-center --check`.
   - **Gate 4 — Host Integrity**: Checks `.iso.sha256`, `uniload update verify-manifest`, and `write-usb-windows.ps1 -VerifyOnly`.
   - **Gate 5 — Ventoy USB 3.0 xHCI + Internal NVMe Simulation**: Boots `vmlinuz` + `initramfs` directly with an internal NVMe SSD + USB 3.0 xHCI Ventoy partition containing `/ISO/unicore-os-x86_64.iso`, verifying zero `/dev/null` warnings, ISO loopback mount, `/mnt/unicore-media`, and loaded `exfat`/`ntfs3` drivers.
2. **`scripts/test-ab-install-luks.py` (3 Stages):**
   - Full bare-metal GPT A/B install + LUKS2 `/data` encryption -> Slot A unlock & Slot B OTA flash -> Slot B reboot with persisted encrypted `/data` + rollback verification.
