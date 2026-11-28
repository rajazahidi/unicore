# UniCore OS (`1.3.1` — 10-Theme Desktop Studio & Universal Hardware Edition)

**UniCore OS** is a lightweight, immutable Linux-based desktop operating system (`Alpine 3.22` + `linux-edge 6.15` + `Mesa 25.1` + `Wine 10.7`) built to run **native Linux (`Flatpak` / `ELF`)**, **Windows (`Win98`, `WinXP`, `Win7`, `Win10/11` `.exe`/`.msi`/Win16 `NE`, plus `Box64` + `Wine 11.0 WoW64` on `aarch64`)**, **MS-DOS (`.exe`/`.com`/`.bat`)**, **Web Apps (PWA)**, and **Android (`.apk`)** applications inside Zero-Trust sandboxes across **2005-era (`512 MB RAM`, no SSE4.2/AVX)** to **2026 modern Intel/AMD/NVIDIA x86_64 & ARM64 PCs and laptops**.

> **Official Bilingual Documentation:** Read the complete [Official User Guide (EN / BM)](docs/user-guide.md) — also built directly into the OS offline via **Start Menu → User Guide / Panduan Pengguna** (`unicore-center --help-guide`).

## Key Capabilities

* **10-Theme Desktop Studio (`v1.3.0`)**:
  * **10 Complete Curated Desktop Theme Environments (`--themes`, `unicore-personalize preset <id>`)**: Switch in 1 click between **UniCore Midnight**, **UniCore Daylight**, **Aurora Violet**, **Nordic Frost**, **Dracula Neon**, **Emerald Forest**, **Amber Sunset**, **Sakura Blossom**, **Cyberpunk Synth**, and **Retro Classic 98** via a visual 2×5 live-preview gallery (`0 MB` idle overhead).
  * **Full 8-Layer Cohesive Live Switching**: Every theme synchronizes its dedicated 1280×800 HD wallpaper (`swaybg`), Waybar layout & colors (`win11`/`win10`/`macos`), Labwc/Openbox window titlebars & corner radius (`0px`–`12px`), GTK3 + Papirus icons, Fuzzel launcher, Foot terminal palette, Mako notifications, and live Slint `UiTheme` UI colors simultaneously.

* **Universal Hardware & NVIDIA Edition (`v1.2.0` / `v1.2.1`)**:
  * **Alpine `3.22` + Kernel `6.15.0` + Mesa `25.1.9` + Wine `10.7`**: Modern 2024–2026 laptop hardware support (Intel Core Ultra / Xe, AMD Ryzen AI 300 / RDNA 4, Wi-Fi 7, USB4/Thunderbolt 4, SoundWire/SOF Audio, `pipewire-alsa` seamless legacy/Win32 audio routing, and `wireless-regdb` + `bluez-firmware` for **zero missing kernel firmware**).
  * **Native Open-Source NVIDIA Vulkan 1.4 (`mesa-vulkan-nouveau` NVK) + GSP Firmware + Hybrid Optimus PRIME (`DRI_PRIME=1`)**: Real hardware-accelerated Vulkan 1.4 for **NVIDIA GTX 16xx & RTX 20xx/30xx/40xx** directly inside Alpine musl (powers DXVK, VKD3D-Proton, Steam & Wayland), with a 1-click **NVIDIA Open GSP Firmware** toggle and **Hybrid Discrete GPU (`DRI_PRIME=1`)** selector in `unicore-center --drivers`.
  * **Early CPU Microcode (`intel-ucode` + `amd-ucode`), Stage-1 I2C/USB-C Boot & Full Laptop Wi-Fi/BT Firmware**: Prepends early CPU microcode straight into `initramfs-x86_64.img`, bundles `i2c/busses` + `usb/typec` drivers in Stage-1 initramfs for modern laptop keyboards & USB-C docks at LUKS2 unlock, preserves Qualcomm Atheros (`qca`) & Cypress laptop Wi-Fi/BT firmware in `slim` builds, and ships `60-unicore-hardware.conf` (disabling Realtek `rtw88`/`rtw89` & Intel `iwlwifi` power-save drops and HDA speaker popping).
  * **Universal Laptop Hardware, Printers & Filesystems Suite (`0 MB` Idle)**: Adds `gutenprint` (offline CUPS drivers for legacy USB printers), `btrfs-progs` + `xfsprogs` + `hdparm`, `lm-sensors`, `acpid` (battery-triggered), `upower`, `iio-sensor-proxy`, `ddcutil` (external monitor hardware brightness), `v4l-utils` + `libcamera` (webcams), `bolt` (Thunderbolt/USB4), `nvme-cli`, `dmidecode`, `ethtool`, `b43-fwcutter`, and automatic ALSA unmute inside 1-Click Audio/BT Reset.

* **Family Safety, 1-Click Web Apps (PWA) & UI Polish (`v1.1.0`)**:
  * **1-Click Desktop Web Apps (`--webapps`, `unicore-webapps`)**: Install **WhatsApp Web, YouTube, Gmail, Microsoft 365, Google Docs, Canva, ChatGPT, Spotify**, or any custom `https://` URL directly into your Start Menu as clean standalone app windows with isolated profiles (`0 MB` extra ISO bloat, `0 MB` idle RAM).
  * **Family Safety & Parental Controls (`--parental`, `unicore-parental`)**: 1-click **Family Safe DNS** (Cloudflare `1.1.1.3` malware + adult filter or `1.1.1.2` malware-only persisted via NetworkManager), **Daily Screen-Time Limits**, **Bedtime Curfew**, and **Per-App Blocklist** enforced across sandboxes & web apps (`0 MB` idle when disabled).
  * **UI Micro-Polish & Installed A/B Health Verification**: Refined `--help-guide` window (zero duplicate header, zero sidebar title truncation) and live `unicore-boot-health` + `unicore-security-audit` verification inside the installed LUKS2 A/B Slot B QA gate.

* **Production Daily Driver Hardening, Health-Gated A/B Rollback & Offline Guide (`v1.0.0`)**:
  * **Full Zero-Trust Security Audit & `bwrap` Hardening (`unicore-security-audit`, `uniload security-audit`)**: Enforces `mktemp` (`umask 077`) temp-file safety across all root/backup/update/AI scripts, strict username validation, `--unshare-uts` + `--unshare-ipc` + `--cap-drop ALL` + `--new-session` + `--die-with-parent` sandbox isolation, `0600` credential checks, and signed Ed25519 OTA enforcement (`REQUIRE_SIGNATURE=1`).
  * **Verified 0-MB-Idle Architecture & RAM Budget Gate**: Automated QA verifies that heavy optional subsystems (`kdeconnectd`, `cupsd`, `fwupd`, `ollama`, `bluetoothd`) stay at **`0 MB` idle RAM** on boot until activated by the user.
  * **Health-Gated A/B OTA Auto-Rollback (`unicore-boot-health`)**: Installed A/B slots are confirmed (`confirm-boot`) only after rootfs, `/data`, `uniload`, `unicore-center --check`, and the live Wayland compositor pass health checks; failed updates automatically roll back via GRUB.
  * **Official Bilingual User Guide (`docs/user-guide.md` + `unicore-center --help-guide`)**: 6-chapter 100% offline EN/BM handbook built into Start Menu & UniCore Center alongside a live 1-click Security Audit viewer.

* **Unified Game Hub & Controller Edition (`v0.9.0`)**:
  * **Unified Game Library (`--games`)**: Automatically gathers **Win98/XP/DOS**, **Steam (`appmanifest_*.acf`)**, **Heroic (Epic/GOG)**, **Lutris**, **Prism (Minecraft)**, **RetroArch**, and **Android (`.apk`)** games in one visual hub with platform badges, Favorites, local Playtime tracking, instant search, Game Mode & MangoHud FPS toggles, and 1-click Play.
  * **Gamepad Controller Manager & Pure-Rust `uinput` Mapper (`--gamepad`, `uniload gamepad-map`)**: Auto-detects USB & Bluetooth Xbox, PlayStation, Nintendo Switch & 8BitDo controllers with battery readout, live `evtest` tester, and a 0-MB-idle pure-Rust Gamepad → Keyboard/Mouse mapper (`wasd_mouse`, `arrows_space`, `dos_classic`) for vintage 98/XP/DOS games.
  * **Automatic Save Game Backups & Cloud Drive Sync (`--saves`, `unicore-gamesave`)**: Auto-backs up game saves on clean exit (keeping latest 5 snapshots in `~/Backups/GameSaves/`) with optional 1-click sync to any connected `v0.8.0` Cloud Drive (`~/Cloud/<Drive>/UniCore-GameSaves/`).

* **Ecosystem, Phone Link, Cloud Drives & Windows Migration Edition (`v0.8.0`)**:
  * **Full Phone Link (`--phone`, `KDE Connect` + `scrcpy` + `adb`)**: Built-in KDE Connect (`0 MB` idle RAM when off, auto-allowed on `Home/LAN` firewall ports `1714–1764` and blocked on `Public Wi-Fi`) for phone notifications, SMS, Wi-Fi clipboard & file sharing, PLUS 1-click **Android Screen Mirroring (`scrcpy`)** over USB or Wi-Fi ADB with screen-off control.
  * **1-Click Cloud Drives (`--cloud`, `rclone mount` VFS → `~/Cloud`)**: Connect **Google Drive**, **Microsoft OneDrive**, **Dropbox**, **MEGA**, or **Nextcloud/WebDAV** via OAuth/credentials (`chmod 600`) and browse them directly in File Explorer (`~/Cloud/<Name>`) with on-demand VFS streaming and optional Auto-Mount on login.
  * **Safe Windows Migration Wizard (`--migrate`, Read-Only NTFS)**: Auto-detects Windows NTFS partitions (strictly mounted `ro`), safely copies *Documents, Pictures, Desktop, Music, Videos, Game Saves*, converts Chrome/Edge bookmarks into importable HTML, imports official Windows fonts (`C:\Windows\Fonts`), and matches `Program Files` against the UniCore Store.
  * **Hardware & Driver Health Manager + LVFS Firmware (`--drivers`, `unicore-hw-mgr` + `fwupd`)**: Plain-language GPU/Vulkan/Wi-Fi/Audio/USB status & missing firmware detector, 1-click **Safe Software GPU Mode (`Pixman`)**, **Wi-Fi Power-Save fix** (stops Realtek/Intel Wi-Fi drops), **1-Click Reset Audio & Bluetooth**, and on-demand official LVFS BIOS/UEFI/SSD firmware checks (`0 MB` idle RAM).

* **Classic 98/XP Gaming Edition (`v0.7.0`)**:
  * **1-Click Classic 98/XP Game Launcher (`--retro-game`, pinned in Start Menu & `+ 98/XP Game`)**: Pick any `.exe`, `.msi`, **Game CD `.iso`**, **`.zip`/`.7z` portable archive**, or game folder and choose from 3 plain-language presets: **Classic 2D (Win98/XP `cnc-ddraw` with crisp 4:3 scaling)**, **Classic 3D (WinXP DXVK D3D8/D3D9 with automatic OpenGL fallback on legacy GPUs)**, or **DOS / Win16 (90s)**.
  * **Rootless Game CD `.iso` Mount as Drive `D:\` (`unicore-retro-media`)**: Double-click any `.iso` or right-click *"Play/Import as Classic 98/XP Game"* in File Explorer to auto-mount (`archivemount` + rootless `bsdtar`/`7z` fallback), read `AUTORUN.INF`/`Setup.exe`, and keep the CD mounted on Wine `D:\` for DRM/music checks.
  * **Smart Post-Install `.exe` Auto-Scanner (`--retro-pick`)**: After a `Setup.exe` finishes inside a sandbox, UniCore automatically scans `drive_c` (ignoring uninstallers/redists) and lets you switch the shortcut to the real playable game `.exe` in 1 click.
  * **1-Click Classic Troubleshooting Switches & Automatic Crash Auto-Repair (`--repair`)**: Per-game toggles for **Cap 60 FPS** (`DXVK_FRAME_RATE=60` + `ddraw.ini maxfps=60`), **Lock 1 CPU Core** (`taskset -c 0` + `singlecpu=true`), **Safe Windowed Screen** (`off` / `800x600` / `1024x768`), expanded Classic Runtime Pack (`vcrun6`, `directplay`, `dsound`, `corefonts`, `d3dx9`), and automatic fast-crash popup diagnosis (`DirectPlay`, `DirectSound`, `Quartz/Indeo`, `DirectDraw`, `3dfx Glide`, `VC++`/`DX9`).

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
