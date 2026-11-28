# UniCore OS `v2.0.0` — Official Offline Bilingual Quick Guide (EN / BM)

Welcome to **UniCore OS `v2.0.0` ("Sovereign Desktop Edition")** — powered by **Alpine 3.22, Linux Kernel 6.15, Mesa 25.1 (with NVIDIA NVK Vulkan 1.4 & Open GSP), PipeWire, Wine 10.7, Native AegisTunnel Post-Quantum VPN, and Native ResonanceFX PipeWire 10-Band Equalizer** — engineered for `x86_64` (2005 512 MB PCs through 2026 laptops) and `aarch64` (ARM64 UEFI).

*Selamat datang ke **UniCore OS `v2.0.0` ("Edisi Desktop Berdaulat")** — pantas, kalis rosak (*immutable*), bebas kembung (*0 MB idle*), dilengkapi **Pusat Kawalan Global, 12 Bahasa, 21 Papan Kekunci, Pengurus Peranti & Storan, VPN AegisTunnel Pasca-Kuantum, Equalizer PipeWire ResonanceFX, serta Studio 10 Tema Desktop**.*

> **Full Documentation Portal:** See [`docs/README.md`](README.md) for the complete [English Manual](USER_MANUAL_EN.md), [Manual Bahasa Melayu](USER_MANUAL_MS.md), [Architecture](ARCHITECTURE.md), [Security](SECURITY.md), [Admin & Operations](ADMIN_AND_OPERATIONS.md), [Hardware Matrix](HARDWARE_COMPATIBILITY.md), and [Developer Guide](DEVELOPER_GUIDE.md).

---

## 1. Quick Start & Shortcuts / Mula Pantas & Pintasan

| Shortcut / Pintasan | Action (EN) | Tindakan (BM) |
| :--- | :--- | :--- |
| **`Super`** | Open Graphical Start Menu | Buka Menu Mula Bergrafik |
| **`Super + E`** | Open Computer Explorer | Buka Penjelajah Komputer |
| **`Super + A`** | Open AI Copilot (Ollama / Gemini / OpenAI) | Buka Pembantu AI |
| **`Super + .`** | Open Emoji Picker | Buka Pemilih Emoji |
| **`Super + V`** | Clipboard History | Sejarah Papan Keratan |
| **`Super + L`** | Lock Screen | Kunci Skrin |
| **`PrintScreen`** | Screen Capture & MP4 Recorder | Tangkap Skrin & Rakam MP4 |
| **`Ctrl + Shift + Esc`** | Task Manager | Pengurus Tugas |
| **`Alt + Shift`** | Switch Non-Latin / US Keyboard Script | Tukar Papan Kekunci Bukan-Latin / US |

---

## 2. Control Panel, Languages, Keyboards & Devices / Pusat Kawalan & Peranti

Open **Start Menu → Control Panel** (`unicore-center --control-panel`) for 1-click access to:
- **Region, Language & Keyboard (`--region`):** 12 international languages, 21 global keyboard layouts (Arabic, Cyrillic, CJK, Thai, Dvorak, Colemak with automatic `Alt+Shift` toggle), 32 world timezones, and 24h/12h taskbar clock.
- **Device Manager (`--devmgr`) & Storage Manager (`--storage`):** Inspect CPU, GPU, Network, Audio, Camera, USB, Bluetooth, HID & SANE Scanners; check S.M.A.R.T. health, safely eject USB drives, and clean temporary caches.
- **Display & 10 Themes (`--themes`):** Multi-monitor layout, screen rotation (`0°/90°/180°/270°`), external DDC/CI brightness, and 10 complete desktop themes.
- **Printers & Scanners:** On-demand CUPS USB/Wi-Fi printing (IPP Everywhere + Gutenprint) and live SANE scanner detection (`0 MB` idle when off).

---

## 3. Native AegisTunnel VPN & ResonanceFX Equalizer (`0 MB` Idle)

- **AegisTunnel VPN (`--vpn`):** Native pure-Rust WireGuard with **AmneziaWG Anti-DPI**, **REALITY TLS 1.3 TCP/443 Stealth Failover (Happy Eyeballs)**, **Hybrid Post-Quantum ML-KEM-768** key rotation, and **Local DNS Ad/Tracker Sinkhole** (`chmod 0600` keys).
- **ResonanceFX Equalizer (`--equalizer`):** Native C++17 PipeWire LADSPA 1.1 studio with **10-Band Graphic EQ (31 Hz–16 kHz)**, **7 1-Click Acoustic Presets**, **Pure Bass**, **Silky Clarity**, **3D Surround**, **Binaural / 8D Spatial Orbit**, **Tube Warmth**, and **UPOLA Convolver IR**.

---

## 4. Apps, Mobile Subsystem & Simplified Classic PC Games

- **Universal App Store (`--tab store`) & Web Apps (`--webapps`):** Sandbox-isolated Linux (Flatpak), Desktop PC, Web (PWA), and Mobile apps.
- **Mobile Subsystem (`--tab system`):** On-demand Waydroid `.apk` runner with 1-click **Device GSF Certification Helper** (`google.com/android/uncertified`) and **Open App Market (Aurora)**.
- **Simplified Legacy Game Auto-Tune (`--retro-game`) & Unified Game Hub (`--games`):** 5 plain-language 1-click presets (*Smart Auto, 90s 2D Classic, 2000s 3D Classic, Retro DOS / 16-Bit, Ultra-Safe Legacy*) with automatic 1-core CPU lock, 60 FPS cap, rootless CD `.iso` mount (`D:\`), Gamepad Mapper (`--gamepad`), and Auto Save Backup (`--saves`).

---

## 5. Phone Link, Cloud Drives, Family Safety & Zero-Trust A/B Updates

- **Phone Link (`--phone`), Cloud Drives (`--cloud`) & PC Data Migration (`--migrate`):** KDE Connect + `scrcpy` mirroring, `rclone` VFS `~/Cloud` mounts, and read-only NTFS file/font/bookmark migration.
- **Family Safety (`--parental`):** 1-click Family Safe DNS (`1.1.1.3`), screen-time limits, bedtime curfew, and per-app blocking.
- **Zero-Trust Security & A/B OTA:** `bubblewrap` confinement, `nftables` firewall, optional LUKS2 `/data` encryption, AES-256 backups, Ed25519 signed A/B OTA updates with health-gated auto-rollback (`unicore-boot-health`), and live `unicore-security-audit`.
