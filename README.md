# UniCore OS (`0.6.1`)

**UniCore OS** is a lightweight, immutable Linux-based desktop operating system (`Alpine 3.20` + `linux-edge 6.12`) built to run **native Linux (`Flatpak` / `ELF`)**, **Windows (`Win98`, `WinXP`, `Win7`, `Win10/11` `.exe`/`.msi`/Win16 `NE`, plus `Box64` + `Wine 11.0 WoW64` on `aarch64`)**, **MS-DOS (`.exe`/`.com`/`.bat`)**, and **Android (`.apk`)** applications inside Zero-Trust sandboxes across **2005-era (`512 MB RAM`, no SSE4.2/AVX)** to **modern x86_64 & ARM64 PCs**.

## Key Capabilities

* **Windows 11 & Zorin Familiarity Edition (`v0.6.1`)**:
  * **Native Graphical Start Menu (`--start-menu` / `Win` key)**: Instant search, 2×4 Pinned Apps grid, Category filters (*All, Internet, Office, Media, Games, System*), user quick folders (*Docs, Downloads, Pictures*), Emoji & Power controls (`Alt+Space` preserved for `fuzzel`).
  * **Windows 11 Modern Default + Zorin 1-Click Layouts & 5 Accent Colors**: Centered floating icon-first taskbar (`Font Awesome 6` + real Papirus `{icon}` app icons) by default, with 1-click **Windows 10 / Zorin Classic** and **macOS Top Dock** layouts plus 5 live accent colors (**Sky Blue**, **Indigo Purple**, **Emerald Green**, **Warm Amber**, **Coral Rose**).
  * **Zero-Friction Daily Driver**: Desktop right-click menu (`labwc/menu.xml`), double-click `.exe`/`.msi`/`.apk` & archive manager (`xarchiver` + `7zip` + *Extract Here* / *Compress to ZIP*), File Explorer thumbnails (`tumbler` + `ffmpegthumbnailer`), USB Plug & Play auto-mount + tray Safely Eject (`udiskie`), translucent Volume & Brightness keyboard OSD (`wob`), Windows 11 Emoji Picker (`Win+.`), metric-compatible Windows Office fonts (`Carlito`/Calibri, `Liberation`/Arial & Times New Roman), friendly non-technical header status, and a 4-step OOBE wizard with layout/accent preview + optional 1-click Starter App Packs.

* **Daily-Driver Essentials, Boot Splash & Accessibility (`v0.5.0`)**:
  * **Branded Framebuffer Boot Splash** (`busybox fbsplash`, 0 MB bloat, auto-clears for LUKS2/Safe Mode), **Waybar System Tray** (Steam/Discord/Telegram/LocalSend), compact 1280px Floating Taskbar, **Noto Color Emoji**, and **Do Not Disturb (DND)** + Alert controls (`makoctl`).
  * **GUI Screenshot & MP4 Screen Recorder (`--capture`, `Print` / `Win+Shift+R`)**: Timed Full/Region screenshots (`grim` + `slurp`) & MP4 recording (`wf-recorder` + optional PipeWire audio) with a live red `REC` pill on Waybar.
  * **Network Drives & Phone Sharing**: Rootless Windows SMB (`smb://`), SFTP (`sshfs`), WebDAV/Nextcloud (`davs://`), Android MTP & iPhone AFC mounts, **Avahi mDNS** (`.local`), plus **LocalSend** Wi-Fi AirDrop in the 33-app Universal Store.
  * **PipeWire Bluetooth Audio (`pipewire-spa-bluez` A2DP/HFP)** with audio card profile switcher, **100% Bilingual EN / Bahasa Melayu**, and a **0-MB-idle Accessibility Pack** (High-Contrast, Large Text, `wvkbd` touch keyboard, offline `espeak-ng` EN/BM speech).

* **Tier-1 Desktop UX, Safe Dual-Boot & GRUB Recovery (`v0.4.0`)**:
  * **Safe Non-Destructive Windows Dual-Boot (`--mode alongside`)**: Installs A/B slots into `>= 10 GiB` unallocated GPT space without shrinking NTFS, reuses the Windows ESP safely, and auto-adds **Windows Boot Manager** + **UniCore Recovery & Advanced Options** (Slot A, Slot B Rollback, Reset OS Settings `unicore_reset_overlay=1` preserving `/data/home`, Safe VESA/Serial) to GRUB.
  * **Modern Desktop Polish**: `Papirus-Dark`/`Papirus-Light` icon themes, selectable **Docked (Win11)** vs **Floating Pill (macOS)** taskbar styles, `swaylock-effects` frosted blur+clock lockscreen, `labwc` drag-to-edge window snapping + preview Alt+Tab switcher, upgraded `--quick-settings` 2×3 tile tray & `--login` greeter, **TLP** laptop battery optimizer, ARM64 Vulkan (`panfrost`, `broadcom`, `freedreno`), and full **Xbox/PlayStation/Switch** gamepad support.
* **Universal 32-App Store, On-Demand Flatpak & GE-Proton (`unicore-center`)**:
  * Unified storefront combining **on-demand Flatpak (Flathub)** in `/data/flatpak` (`0 MB` ISO bloat — **Steam**, **Heroic**, **Lutris**, **Prism Launcher**, **RetroArch**, **GIMP**, **OBS Studio**, **Inkscape**, **Audacity**, **Blender**, **Kdenlive**, **Vesktop**, **Telegram**, **Thunderbird**, **LibreOffice Native**, **Flatseal**) plus SHA-256 verified Win32/Android/DOS apps across 9 categories with automatic `x86_64`/`aarch64` filtering, 1-click Uninstall, and built-in **GE-Proton Manager** (`unicore-proton`).
* **Multi-Provider System AI Copilot (`Win+A` / Waybar `AI` pill) & DevBox (`0 MB` Idle RAM)**:
  * Supports **Local Offline Ollama** (`0 MB` idle RAM) PLUS optional **Google Gemini API** and **OpenAI-Compatible API** (keys stored `chmod 600` in `/data/ai/provider.conf`) so even 512 MB–2 GB legacy PCs get bilingual EN/BM AI diagnostics with read-only live telemetry and safe 1-click navigation actions.
  * Built-in `git`, `python3`, `openssh-client`, `jq`, and rootless **Podman DevBoxes** (`Python`, `Node.js`, `Rust`, `Ubuntu` in `/data/dev`).
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
