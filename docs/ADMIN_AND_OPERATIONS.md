# UniCore OS — Administration & Operations Guide (`v2.0.0`)

**Document ID:** `UC-OPS-2.0`  
**Audience:** System Administrators, IT Support Engineers, Fleet Operators, Power Users

---

## 1. Deployment & Installation Procedures

### 1.1 Creating Bootable Media
UniCore OS publishes hybrid **Legacy BIOS + UEFI** (`x86_64`) and **UEFI** (`aarch64`) ISO images alongside `.iso.sha256` digests on GitHub Releases:

1. **Verify SHA-256 Digest Before Flashing:**
   ```powershell
   # Windows Host
   .\scripts\write-usb-windows.ps1 -VerifyOnly
   ```
   ```sh
   # Linux / macOS
   sha256sum -c unicore-os-x86_64.iso.sha256
   ```
2. **Flash Options:**
   - **Ventoy (Recommended):** Copy `unicore-os-x86_64.iso` onto a Ventoy USB drive (`exFAT`/`NTFS`/`FAT32`). Stage-1 `initramfs-init` natively supports Ventoy ISO loopback across USB 2.0/3.x xHCI and NVMe hosts.
   - **Direct Image Write (Rufus DD Mode / `dd`):**
     ```sh
     sudo dd if=unicore-os-x86_64.iso of=/dev/sdX bs=4M status=progress oflag=sync
     ```

### 1.2 UEFI Secure Boot Enrollment (One-Time)
UniCore OS ships with Microsoft-signed `shim` + `MokManager` and embeds its Machine Owner Key certificate (`ENROLL_UNICORE_MOK.cer`) on the ISO and ESP:
- On first Secure Boot start, `MokManager` prompts to enroll `ENROLL_UNICORE_MOK.cer` from disk — select **Enroll key from disk → Continue → Yes → Reboot**.
- Alternatively, boot once with Secure Boot disabled, open **UniCore Center → System → Secure Boot → Enroll MOK**, set a one-time enrollment password, and re-enable Secure Boot in firmware.

### 1.3 Bare-Metal Disk Installation Modes
Launch **UniCore Center → System → Install to Disk** (or CLI `/usr/sbin/unicore-install`):

| Mode | CLI Flag | Behavior |
| :--- | :--- | :--- |
| **Non-Destructive Dual-Boot** | `--mode alongside` | Allocates A/B system slots + `/data` inside `>= 10 GiB` unallocated GPT free space without touching existing OS partitions; reuses existing ESP safely and auto-generates a dual-boot GRUB menu entry. |
| **Full Disk (Clean GPT)** | `--mode wipe` | Creates GPT with `EFI`, `BIOS_BOOT` (`x86_64`), `UNICORE_SYS_A`, `UNICORE_SYS_B`, and `UNICORE_DATA`. |
| **LUKS2 Encryption** | `--encrypt-data` | Encrypts `/data` (`UNICORE_DATA_LUKS`) using LUKS2 PBKDF2 (minimum 8-character passphrase prompted at Stage-1 boot). |

---

## 2. Over-The-Air (OTA) Updates, Self-Hosting & Rollback

### 2.1 Production OTA Architecture
By default, `/etc/unicore/update.conf` checks `https://github.com/rajazahidi/unicore/releases/latest/download/unicore-manifest-<arch>.json` with `REQUIRE_SIGNATURE=1` verified against `/etc/unicore/update.pub`.

```sh
# Check for updates manually
/usr/libexec/unicore-update-check

# Apply verified A/B OTA update to inactive slot
doas /usr/libexec/unicore-sys-helper ota-apply

# Inspect active slot & trial boot state
uniload update status
```

### 2.2 Hosting an Air-Gapped / Enterprise LAN OTA Mirror
Organizations can host signed releases internally with zero internet dependency:
1. Generate a dedicated fleet Ed25519 keypair:
   ```sh
   uniload update keygen --private fleet-release.key --public fleet-update.pub
   ```
2. Sign the release manifest and catalog:
   ```sh
   uniload update sign-manifest unicore-manifest-x86_64.json --key fleet-release.key --output unicore-manifest-x86_64.json.sig
   ```
3. Serve the directory over HTTP/HTTPS (or test locally via `python3 scripts/ota-local-server.py --dir os-builder/out --port 8090`) and configure `MANIFEST_URL` in `/etc/unicore/update.conf`.

### 2.3 Automatic & Manual Recovery / Rollback
- **Automatic Rollback:** After flashing the standby slot, GRUB sets `trial_boot=1`. On boot, `/usr/libexec/unicore-boot-health` verifies rootfs integrity, `/data` mount, `uniload`, `unicore-center --check`, and the live Wayland compositor before calling `uniload update confirm-boot`. If two consecutive boots fail to confirm, GRUB automatically reverts to the previous healthy slot.
- **Manual Slot Rollback:** Open **UniCore Center → System → OS Updates → Rollback to Previous Slot**, or select **UniCore Recovery → Boot Previous Slot** in GRUB.
- **Non-Destructive Factory Reset:** Select **UniCore Recovery → Reset OS Settings (`unicore_reset_overlay=1`)** in GRUB to reset `/etc` and overlay state while preserving `/data/home` and user files.

---

## 3. Daily System Operations

### 3.1 Networking, Firewall & AegisTunnel VPN
- **Network Profiles:** Managed via `NetworkManager` (`nmcli` or **Control Panel → Wi-Fi & Network**); connections persist under `/data/network`.
- **Stateful Firewall (`nftables`):** Switch between **Home / LAN** (allows mDNS `5353/udp` & Phone Link `1714–1764`) and **Public Wi-Fi** (drops all unsolicited inbound traffic) via **Control Panel → Security & Backup** or `/usr/libexec/unicore-firewall`.
- **AegisTunnel Post-Quantum VPN:**
  ```sh
  # Check status (0 MB idle when stopped)
  /usr/libexec/unicore-aegis-ctl status

  # Generate X25519 keypair or run cryptographic/handover self-test
  /usr/libexec/unicore-aegis-ctl keygen
  /usr/libexec/unicore-aegis-ctl self-test

  # Start / live rotate ML-KEM-768 PQ epoch / stop cleanly
  doas /usr/libexec/unicore-sys-helper aegis-start
  doas /usr/libexec/unicore-sys-helper aegis-rotate-pq
  doas /usr/libexec/unicore-sys-helper aegis-stop
  ```

### 3.2 Audio & ResonanceFX PipeWire DSP Operations
- **PipeWire Audio Stack:** `pipewire`, `wireplumber`, `pipewire-pulse`, and `pipewire-alsa` route all native Linux, browser, and legacy Win32 audio seamlessly.
- **On-Demand ResonanceFX Equalizer (`0 MB` Idle):**
  ```sh
  /usr/libexec/unicore-resonance-ctl status      # Verify installed=1, running=0 at boot
  /usr/libexec/unicore-resonance-ctl self-test   # Verify 48 kHz LADSPA DSP & bypass
  /usr/libexec/unicore-resonance-ctl start       # Creates PipeWire sink unicore_resonance_fx_sink
  /usr/libexec/unicore-resonance-ctl stop        # Cleanly unloads sink (0 MB idle restored)
  ```

### 3.3 Printers (CUPS + Gutenprint) & Scanners (SANE)
- **Printers:** `cupsd` starts strictly on demand (`doas /usr/libexec/unicore-sys-helper cups-start`). Supports driverless IPP Everywhere / AirPrint Wi-Fi printers as well as legacy USB printers via bundled `gutenprint` PPDs.
- **Scanners:** Live detection via `scanimage -L` (`sane-backend-escl`, `hp`, `pixma`, `epson2`, `net`) integrated directly into **Control Panel → Printers & Scanners** and **Device Manager**.

### 3.4 Storage, Encrypted Backups & Diagnostics
- **Storage Management:** Inspect block devices, S.M.A.R.T. health (`smartctl`), safe USB unmount (`unicore-storage unmount <dev>`), and temporary cache cleanup (`unicore-storage clean-cache`) via **Control Panel → Storage Manager**.
- **Encrypted `/data` Backups:** Configure target directory/USB and passphrase in **Control Panel → Security & Backup** (`unicore-backup`). Archives are `tar --zstd` compressed, optionally encrypted with AES-256-CBC PBKDF2 (`100k` iterations), and paired with a `.sha256` manifest.
- **Privacy-Safe Diagnostics Export:** Run `unicore-diag --save-usb` (or click **Export Diagnostics** in UniCore Center) to capture kernel logs, `lspci`/`lsusb`, PipeWire sinks, and `unicore-hw-mgr` status without exporting private keys or user files.

---

## 4. Operational Troubleshooting Runbook

| Symptom | Diagnostic Command | Resolution |
| :--- | :--- | :--- |
| **Black screen or GPU artifacts on older hardware** | `/usr/libexec/unicore-hw-mgr status` | Enable **Safe Software GPU Mode (`Pixman`)** in **Hardware & Drivers** (`--drivers`), or choose **Safe VESA/Software** from GRUB Recovery. |
| **Wi-Fi drops periodically on Realtek/Intel laptops** | `dmesg \| grep -iE 'iwlwifi\|rtw8'` | UniCore pre-applies `/etc/modprobe.d/60-unicore-hardware.conf`; toggle **Wi-Fi Power-Save Fix** in `--drivers` if needed. |
| **No sound or muted speaker after sleep/BT switch** | `pactl info && pactl list short sinks` | Click **1-Click Reset Audio & Bluetooth** in `--drivers` (unmutes ALSA master/PCM & restarts PipeWire/WirePlumber). |
| **NVIDIA GTX 16xx / RTX Vulkan games use software rendering** | `vulkaninfo --summary` | Confirm **NVIDIA Open GSP Firmware** is `on` and enable **Hybrid Discrete GPU (`DRI_PRIME=1`)** in `--drivers`. |
| **USB/Wi-Fi printer not discovered** | `doas /usr/libexec/unicore-sys-helper printer-discover` | Start CUPS service first in **Printers & Scanners** (`cups-start`) and ensure firewall is on **Home / LAN** for mDNS Wi-Fi discovery. |
| **AegisTunnel fails to reach UDP gateway on restrictive Wi-Fi** | `/usr/libexec/unicore-aegis-ctl status` | Select **Auto Happy Eyeballs** or **REALITY TLS 1.3 TCP/443 Stealth** in **AegisTunnel VPN** (`--vpn`). |
| **Classic 90s/2000s PC game runs too fast or crashes on multi-core CPU** | `uniload diagnose-log <log>` | Launch via **Classic Games (`--retro-game`)** and choose **Ultra-Safe Legacy** (locks 1 CPU core + 60 FPS cap + `800x600` window) or run **1-Click Crash Auto-Repair**. |
