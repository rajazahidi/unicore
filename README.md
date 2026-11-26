# UniCore OS (`0.1.0`)

**UniCore OS** is a lightweight, immutable Linux-based desktop operating system (`Alpine 3.20` + `linux-edge 6.12`) built to run **Windows (`Win98`, `WinXP`, `Win7`, `Win10/11` `.exe`/`.msi`/Win16 `NE`)**, **MS-DOS (`.exe`/`.com`/`.bat` embedded seamlessly without third-party launcher branding)**, **Android (`.apk`)**, and **native Linux (`ELF`)** applications inside per-app Zero-Trust `bubblewrap` sandboxes across **2005-era (`512 MB RAM`, no SSE4.2/AVX)** to **modern PCs**.

## Key Capabilities

* **Multi-Platform App Center & 1-Click Store (`unicore-center`)**:
  * Play Store / Microsoft Store-inspired storefront with **Featured Hero Pick**, Discover grid, inline detail view, bilingual UI (**EN / Bahasa Melayu**), **Google Antigravity `2.35.2`**, LibreOffice `26.8.3`, VSCodium, KeePassXC, VLC, SumatraPDF, Notepad++, 7-Zip, Android tools (`F-Droid`, `NewPipe`), and a dedicated **Retro Games Marketplace**.
* **Windows, Retro Gaming & Seamless MS-DOS (`uniload` + Wine/DXVK/VKD3D/OTVDM/DOS)**:
  * Windows era profiles (`Win98`, `WinXP`, `Win7`, `Win10`), offline **DXVK `v2.4.1`** (`d3d8.dll`–`d3d11.dll`), **VKD3D-Proton `v2.13`**, **`cnc-ddraw`**, **`otvdm`** (Win16 `NE`), **`winetricks`** (DX9/VC++), MangoHud FPS overlay, Game Mode CPU governor, and an embedded MS-DOS subsystem (`uniload-dos`).
* **Productivity, Office & Hardware Settings**:
  * Built-in native **Office & Document Suite** (`AbiWord`, `Gnumeric`, `Evince` PDF) + **SANE Document Scanner** (`simple-scan` eSCL/HP/PIXMA/Epson2/net).
  * On-demand **CUPS USB/Wi-Fi Printing** (`0 MB` idle RAM when off), **Night Light** (`wlsunset`), **`Win+V` Clipboard History**, **NTFS/exFAT** read-write USB support, Wi-Fi (`NetworkManager`), on-demand Bluetooth (`BlueZ`), PipeWire audio, and Wayland display scaling (`wlr-randr`).
* **Personalization & Input (`unicore-personalize`)**:
  * Live **Dark / Light Desktop Theme** (`unicore-center`, `labwc`, `waybar`, `fuzzel`, GTK3), curated **1080p wallpapers** + custom paths, **Keyboard Layout** (`us`, `my`, `gb`, `de`, `fr`, `jp`), and **Touchpad / Pointer** controls (Natural Scrolling, Tap-to-Click, Pointer Speed) persisted under `/data/home/.config/unicore/`.
* **Zero-Trust Security, Backup & A/B OTA Updates**:
  * Stateful `nftables` firewall (`Home/LAN` vs `Public Wi-Fi` + optional SSH), **WireGuard & OpenVPN** profiles, **1-Click `/data` Backup & Restore** (`tar --zstd` + optional AES-256-CBC PBKDF2 + `.sha256`), bare-metal GPT Hybrid BIOS+UEFI disk installer with optional **LUKS2** encryption, A/B slot rollback (`grubenv`), and **Ed25519** signed OTA manifests.

## Repository Layout

* `core/orchestrator/` (`uniload`) — Pure-Rust binary inspector, sandbox planner (`bwrap`), Windows/DOS/Android subsystems, hardware profiler (`hw-profile`), verified App Store catalog (`store`), and A/B + Ed25519 OTA engine (`update`).
* `core/center/` (`unicore-center`) — Rust + Slint (`renderer-software`) GUI App & Security Center (`~22–23 MB RSS`) with `--oobe`, `--open`, `--power`, `--taskmgr`, and `--tab store|settings:...`.
* `os-builder/` — Alpine `3.20` (`linux-edge`) rootfs builder, Wayland overlay (`labwc`, `waybar`, `fuzzel`, `pcmanfm`), universal Stage-1 `initramfs-init`, and Hybrid Legacy BIOS + UEFI ISO builder.
* `scripts/` — Windows cargo dev wrapper (`cargo-dev.ps1`), Ed25519 release signer (`sign-release.ps1`), safe Windows USB flasher helper (`write-usb-windows.ps1`), and automated BIOS/UEFI/2005 QA Matrix (`test-release-matrix.py`).
* `docs/` — [Architecture](file:///c:/Project/Processor/docs/architecture.md) and [Release & QA Guide](file:///c:/Project/Processor/docs/release-guide.md).

## Quick Start (Windows Host)

```powershell
# 1. Lint & Test Rust Workspace
cargo clippy --workspace --all-targets -- -D warnings
.\scripts\cargo-dev.ps1 test --workspace

# 2. Build Hybrid BIOS + UEFI ISO
.\os-builder\bootstrap-via-qemu.ps1                # Full clean build
.\os-builder\bootstrap-via-qemu.ps1 -Incremental   # Fast overlay/Rust rebuild (when packages unchanged)

# 3. Verify & Run Release QA Matrix (BIOS + UEFI + 2005 512MB PC + Phase 20D + Ed25519)
python .\scripts\test-release-matrix.py
.\scripts\write-usb-windows.ps1 -ListUsb
.\scripts\sign-release.ps1 -GenerateKey
```
