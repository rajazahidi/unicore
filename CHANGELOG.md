# Changelog

All notable changes to **UniCore OS** are documented in this file following [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and [Semantic Versioning](https://semver.org/).

---

## [2.0.0] — Sovereign Desktop Edition & Industry-Standard Documentation Suite

### Added
- **Complete Industry-Standard Documentation Suite (`docs/`):**
  - Documentation Portal ([`docs/README.md`](docs/README.md))
  - System Architecture Specification with Mermaid diagrams ([`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md))
  - Security Architecture & Formal Threat Model ([`docs/SECURITY.md`](docs/SECURITY.md))
  - Administration & Operations Guide + Troubleshooting Runbook ([`docs/ADMIN_AND_OPERATIONS.md`](docs/ADMIN_AND_OPERATIONS.md))
  - Developer & QA Engineering Guide ([`docs/DEVELOPER_GUIDE.md`](docs/DEVELOPER_GUIDE.md))
  - Hardware Compatibility Matrix ([`docs/HARDWARE_COMPATIBILITY.md`](docs/HARDWARE_COMPATIBILITY.md))
  - Complete End-User Manuals in English ([`docs/USER_MANUAL_EN.md`](docs/USER_MANUAL_EN.md)) and Bahasa Melayu ([`docs/USER_MANUAL_MS.md`](docs/USER_MANUAL_MS.md))
  - Semantic Release History ([`CHANGELOG.md`](CHANGELOG.md))
- Refreshed offline built-in User Guide (`docs/user-guide.md` / `unicore-center --help-guide`) for `v2.0.0`.

### Changed
- Polished `AegisTunnel VPN` (`--vpn`) footer button spacing and header subtitle so all controls fit cleanly at `1280×800` without edge clipping or ellipsis.
- Polished `ResonanceFX Equalizer` (`--equalizer`) spatial effects switch widths and ComboBox spacing so zero labels touch or elide.
- Updated OS release metadata, OTA manifests, and automated Cloud QEMU QA matrix to `2.0.0`.

### Fixed
- Fixed use-after-`dlclose` string dereference in `core/resonance-fx/src/resonance_test.cpp` so C++17 LADSPA self-test passes cleanly on both glibc and musl hosts.

---

## [1.6.0] — Native AegisTunnel PQ-VPN & ResonanceFX PipeWire DSP Edition

### Added
- **Embedded Native AegisTunnel VPN (`core/aegis-proto`, `core/aegis-vpn`, `/usr/bin/unicore-aegis-vpn`, `--vpn`):** Pure-Rust static musl WireGuard engine with AmneziaWG Anti-DPI obfuscation, REALITY TLS 1.3 TCP/443 stealth failover (Happy Eyeballs), Hybrid Post-Quantum `X25519 + ML-KEM-768` live key rotation (`SIGHUP`), Local DNS Ad/Tracker Sinkhole, and Zero-Drop Handover — `0 MB` idle RAM when disconnected.
- **Embedded Native ResonanceFX PipeWire Equalizer & Spatial DSP (`core/resonance-fx`, `/usr/lib/ladspa/libunicore_resonance_fx.so`, `--equalizer`):** Pure C++17 header-free LADSPA 1.1 PipeWire filter-chain studio featuring 10-band coupled Graphic EQ, Pure Bass, Silky Clarity, 3D Surround, Binaural / 8D Orbit, Analog Tube Warmth, UPOLA FFT Convolver IR, Schroeder Reverb, Stereo Chorus/Delay, Loudness, and 7 1-click acoustic presets — `0 MB` idle RAM when off.
- Live Gate 1 QEMU QA end-to-end verification of `aegis0` TUN start/PQ-rotate/stop, `unicore_resonance_fx_sink` PipeWire start/stop, and 2 new Wayland screenshots (20 total).

### Changed
- Upgraded Control Panel (`--control-panel`) from 4-column narrow tiles to a wide 2×5 (10-card) horizontal layout eliminating text elision and adding 1-click AegisTunnel VPN & ResonanceFX Equalizer launch cards.

### Fixed
- Filtered virtual `/dev/fd*` floppy devices from `unicore-storage` (`Storage Manager`).
- Added Windows SDK & MSVC `INCLUDE` path detection to `scripts/cargo-dev.ps1` and `musl-tools` (`musl-gcc`) support in CI & Release workflows.

---

## [1.5.0] — Global Control Panel, International Localization & Sovereign Desktop Edition

### Added
- **Unified Global Control Panel (`--control-panel`), Device Manager (`--devmgr`) & Storage Manager (`--storage`)** with live hardware category inspection, S.M.A.R.T. checks, safe USB unmount, and temp cache cleaner (`0 MB` idle).
- **12 International Languages, 21 Global Keyboard Layouts & 32 World Timezones (`--region`):** Automatic `us,<layout>` + `Alt+Shift` switching for non-Latin scripts (`ru`, `ar`, `jp`, `kr`, `cn`, `th`), bundled `font-noto-arabic` + `font-wqy-zenhei`, Dvorak/Colemak layouts, and 24h/12h Waybar clock format.
- **Multi-Monitor, Screen Rotation (`0°/90°/180°/270°`), DDC/CI Brightness & Live SANE Scanner Detection.**
- **Desktop & Computer Explorer Right-Click Context Menus** (`unicore-desktop-action` + PCManFM actions for Open Terminal, Copy Path, Print File, Extract/Compress, Run in Sandbox).
- **Simplified 5-Card 1-Click Legacy Game Auto-Tune (`--retro-game`)** with collapsible Advanced Hardware Tuning.

### Changed
- Cleaned third-party OS/store brand terminology across desktop menus, settings, and guides in favor of sovereign UniCore OS terminology.

---

## [1.4.0] – [1.4.3] — Mobile Device Certification, Open App Market & Full Persona Audit

### Added
- 1-click **Device GSF Certification Helper (`uniload android gsf-id`)** and **Open App Market (`uniload android install-aurora`)** inside `unicore-center --tab system`.
- Full automated Human & Machine Persona QA simulation inside Gate 1 (web browsing/HTTPS MIME, HTTP download, music & video streaming, Wine/DOSBox gaming, on-demand CUPS printing + Gutenprint, live 2-click Start Menu toggle).

---

## [1.3.0] – [1.3.1] — 10-Theme Desktop Environment Studio

### Added
- 10 complete cohesive desktop environments (`Midnight`, `Daylight`, `Aurora`, `Nordic`, `Dracula`, `Emerald`, `Sunset`, `Sakura`, `Cyberpunk`, `Retro Classic 98`) synchronized across 8 desktop layers (`swaybg`, `waybar`, `labwc`, GTK3/Papirus, `fuzzel`, `foot`, `mako`, and Slint `UiTheme`).

---

## [1.2.0] – [1.2.1] — Universal Hardware & NVIDIA Edition

### Added
- Upgraded base to **Alpine Linux 3.22 + Linux Kernel 6.15 + Mesa 25.1 + Wine 10.7**.
- Native open-source **NVIDIA NVK Vulkan 1.4 (`mesa-vulkan-nouveau`)** + **Open GSP Firmware (`NvGspRm=1`)** + **Hybrid Discrete GPU (`DRI_PRIME=1`)** toggle.
- Early `intel-ucode` + `amd-ucode` CPU microcode, Stage-1 I2C/USB-C initramfs drivers, `pipewire-alsa`, `wireless-regdb`, `bluez-firmware`, `gutenprint`, `btrfs-progs`, `xfsprogs`, `ddcutil`, `bolt`, `nvme-cli`, `v4l-utils`, `libcamera`, and `60-unicore-hardware.conf` Wi-Fi/HDA stability rules.

---

## [1.1.0] — Family Safety & 1-Click Web Apps (PWA)

### Added
- 1-click **Web Apps (PWA)** installer (`--webapps`) with isolated browser profiles (`0 MB` idle).
- **Family Safety & Parental Controls (`--parental`):** Family Safe DNS (`1.1.1.3`/`1.1.1.2`), screen-time limits, bedtime curfew, and per-app blocklist.

---

## [1.0.0] — Production Daily Driver & Zero-Trust Hardening

### Added
- Live **Zero-Trust Security Audit** (`unicore-security-audit` / `uniload security-audit`) enforcing `bwrap` hardening (`--cap-drop ALL --unshare-uts --unshare-ipc`), `0600` credentials, `umask 077` `mktemp`, mandatory Ed25519 OTA signatures, and verified `0 MB`-idle heavy services.
- **Health-Gated A/B OTA Auto-Rollback (`unicore-boot-health`)** and offline bilingual User Guide (`--help-guide`).

---

## [0.1.0] – [0.9.0] — Foundation Releases

- **v0.9.0:** Unified Game Hub (`--games`), pure-Rust `uinput` Gamepad → Keyboard/Mouse Mapper (`--gamepad`), Automatic Save Game Backups & Cloud Sync (`--saves`).
- **v0.8.0:** Phone Link (`KDE Connect` + `scrcpy`), 1-Click Cloud Drives (`rclone` VFS), Read-Only NTFS PC Data Migration Wizard, Hardware & Driver Manager + LVFS (`fwupd`).
- **v0.7.0:** Classic 98/XP/DOS Game Launcher (`cnc-ddraw`, `DXVK v2.4.1`, `VKD3D-Proton`, `OTVDM`, `DOSBox-Staging`), rootless `.iso` mount (`D:\`), smart post-install `.exe` scanner & crash auto-repair.
- **v0.6.0–v0.6.1:** Native Graphical Start Menu, Emoji Picker, Desktop layouts, Papirus icons, USB auto-mount (`udiskie`), translucent OSD (`wob`), 4-step OOBE wizard.
- **v0.5.0:** Branded framebuffer boot splash, GUI Screenshot & MP4 Recorder (`grim`/`slurp`/`wf-recorder`), SMB/SFTP/WebDAV/MTP/AFC mounts, PipeWire Bluetooth audio, bilingual EN/BM, Accessibility pack.
- **v0.1.0–v0.4.0:** Immutable A/B SquashFS + OverlayFS + LUKS2 installer, Non-Destructive Dual-Boot, GRUB Recovery, Universal App Store + on-demand Flatpak + GE-Proton, Multi-Provider AI Copilot, Podman DevBoxes, `nftables` firewall, Ed25519 OTA, UEFI Secure Boot (`shim` + MOK), and ARM64 (`aarch64`) + `Box64`/`Wine WoW64` support.
