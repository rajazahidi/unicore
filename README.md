# UniCore OS (`0.2.1`)

**UniCore OS** is a lightweight, immutable Linux-based desktop operating system (`Alpine 3.20` + `linux-edge 6.12`) built to run **Windows (`Win98`, `WinXP`, `Win7`, `Win10/11` `.exe`/`.msi`/Win16 `NE`)**, **MS-DOS (`.exe`/`.com`/`.bat` embedded seamlessly without third-party launcher branding)**, **Android (`.apk`)**, and **native Linux (`ELF`)** applications inside per-app Zero-Trust `bubblewrap` sandboxes across **2005-era (`512 MB RAM`, no SSE4.2/AVX)** to **modern PCs**.

## Key Capabilities

* **Multi-Platform App Center, Quick Settings & 1-Click Store (`unicore-center`)**:
  * Play Store / Microsoft Store-inspired storefront with **Featured Hero Pick**, Discover grid, inline detail view, bilingual UI (**EN / Bahasa Melayu**), **Google Antigravity `2.35.2`**, LibreOffice `26.8.3`, VSCodium, KeePassXC, VLC, SumatraPDF, Notepad++, 7-Zip, Android tools (`F-Droid`, `NewPipe`), and a dedicated **Retro Games Marketplace**.
  * Win11-style **`--quick-settings` tray popup**, built-in graphical **File Picker**, and per-app advanced launch arguments, environment variables, Wine prefix reset, and save-data archive backup/restore.
* **Daily Laptop Pack (Multi-User A/B Sync, Power & S.M.A.R.T.)**:
  * Smart Multi-User login (`unicore-users` + `--login` greeter synced across A/B slots), `swayidle` auto-lock & battery-only sleep, **Power Saver / Balanced / Performance** profiles (`unicore-power`), and on-demand **S.M.A.R.T.** disk health checks (`unicore-smart`).
* **Developer Environments & Local AI (`0 MB` Idle RAM)**:
  * Built-in `git`, `python3`, `openssh-client`, `jq`, and rootless **Podman DevBoxes** (`Python`, `Node.js`, `Rust`, `Ubuntu` in `/data/dev`), plus on-demand **Local AI (Ollama)** in `/data/ai` with RAM-aware model recommendations and `127.0.0.1:11434` endpoint.
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
* `scripts/` — Unified environment controller (`unicore-env.ps1`), local LAN/QEMU OTA server (`ota-local-server.py`), Windows cargo wrapper (`cargo-dev.ps1`), Ed25519 release signer (`sign-release.ps1`), safe Windows USB flasher helper (`write-usb-windows.ps1`), and automated 5-gate QA Matrix (`test-release-matrix.py`) + A/B LUKS2 E2E (`test-ab-install-luks.py`).
* `docs/` — [Architecture](docs/architecture.md), [Release & QA Guide](docs/release-guide.md), and [v1.0 Readiness Checklist](docs/v1-readiness-checklist.md).

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
