# UniCore OS (`0.1.0`) — Release & Hardware QA Guide

This guide covers verifying, flashing, signing, and testing UniCore OS release builds across **Legacy BIOS**, **UEFI**, and **2005-era Low-Spec PCs**.

---

## 1. Verify & Flash Hybrid ISO to USB (Windows)

```powershell
# Verify ISO SHA-256 & list removable USB drives safely (internal drives excluded):
.\scripts\write-usb-windows.ps1 -ListUsb

# SHA-256 integrity verification only:
.\scripts\write-usb-windows.ps1 -VerifyOnly
```

Recommended USB tools:
- **Rufus** (`DD Image mode` recommended, or `ISO Image mode`)
- **Ventoy** (copy `os-builder\out\unicore-os-x86_64.iso` to the Ventoy volume)
- **Linux / macOS:** `sudo dd if=os-builder/out/unicore-os-x86_64.iso of=/dev/sdX bs=4M status=progress oflag=sync`

---

## 2. Ed25519 OTA Release Manifest Signing

UniCore OS A/B updates use SHA-256 artifact digests plus optional **Ed25519** signatures verified strictly when `REQUIRE_SIGNATURE=1` in `/etc/unicore/update.conf`:

```powershell
# Generate Ed25519 keypair (if absent), sign unicore-manifest.json, and verify strictly:
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

## 3. Automated Release QA Matrix (BIOS + UEFI + 2005 512 MB PC)

Requires QEMU (`C:\Program Files\qemu\qemu-system-x86_64.exe` + `edk2-x86_64-code.fd`) and Python 3:

```powershell
python .\scripts\test-release-matrix.py
```

The QA matrix tests all 4 release gates against `os-builder\out\unicore-os-x86_64.iso`:
1. **Gate 1 — Legacy BIOS + Phase 20D + Ed25519 E2E**: Boots GRUB BIOS from Live ISO, verifies `unicore-personalize` Light/Dark themes, wallpapers, Malay keyboard layout (`XKB_DEFAULT_LAYOUT=my`), touchpad config, captures live Light-mode Wayland UI screenshot, and validates Ed25519 signature acceptance + tamper rejection.
2. **Gate 2 — UEFI Firmware (`edk2-x86_64-code.fd`)**: Boots GRUB EFI from Live ISO and confirms `/sys/firmware/efi`.
3. **Gate 3 — 2005 Legacy PC (`512 MB RAM`, 1 core, no SSE4.2/AVX)**: Boots Linux 6.12 cleanly, confirms `UNICORE_HW_TIER=legacy_ultra`, `UNICORE_LIGHTWEIGHT_SHELL=1`, active `zram0` swap, and `unicore-center --check`.
4. **Gate 4 — Host Integrity**: Checks `.iso.sha256`, `uniload update verify-manifest`, and `write-usb-windows.ps1 -VerifyOnly`.

### Bare-Metal A/B Disk Install + LUKS2 + Rollback E2E

To verify GPT disk installation, LUKS2 `/data` encryption, Slot A boot, `unicore-update --rollback`, and Slot B persistence end-to-end inside a temporary 10 GiB QEMU disk:

```powershell
python .\scripts\test-ab-install-luks.py
```

