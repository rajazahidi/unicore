# UniCore OS v1.1.0 — Official Bilingual User Guide (EN / BM)

Welcome to **UniCore OS v1.1.0 ("Family, Web Apps & Polish Edition")** — a fast, zero-bloat, immutable desktop operating system designed to feel immediately familiar to Windows and Zorin OS users, while effortlessly running **Linux, Windows (Win98/XP/7/10/11 & MS-DOS), Web Apps (PWA), and Android** apps on hardware ranging from **2005 512 MB PCs** to **2026 modern x86_64 & ARM64 laptops**.

*Selamat datang ke **UniCore OS v1.0.0 ("Production Daily Driver")** — sistem operasi desktop moden yang pantas, bebas kembung (*zero-bloat*), dan mudah digunakan seperti Windows/Zorin, serta menyokong aplikasi **Linux, Windows (Win98/XP/7/10/11 & MS-DOS) dan Android** dari PC lama 2005 (512 MB RAM) hingga komputer riba 2026 (`x86_64` & `ARM64`).*

> **Offline Built-In Reader:** On UniCore OS desktop, open **Start Menu → User Guide / Panduan Pengguna** or run `unicore-center --help-guide` at any time — no internet connection required!

---

## 1. Quick Start & Keyboard Shortcuts / Mula Pantas & Pintasan

| Shortcut / Pintasan | Action (EN) | Tindakan (BM) |
| :--- | :--- | :--- |
| **`Super` (Windows Key)** | Open Graphical Start Menu | Buka Menu Mula Bergrafik |
| **`Super + .`** | Open Emoji Picker | Buka Pemilih Emoji |
| **`Super + A`** | Open AI Copilot (Ollama / Gemini / OpenAI) | Buka Pembantu AI |
| **`Super + E`** | Open File Explorer (`pcmanfm`) | Buka Pengurus Fail |
| **`Super + L`** | Lock Screen (`swaylock`) | Kunci Skrin |
| **`PrintScreen`** | Open Screen Capture & MP4 Recorder | Tangkap Skrin & Rakam MP4 |
| **`Ctrl + Shift + Esc`** | Open Task Manager | Buka Pengurus Tugas |
| **`Alt + Tab`** | Switch Open Windows | Tukar Tetingkap Aktif |

### Desktop Styles (Appearance Settings / Gaya Desktop)
Open **Settings → Display & Style** (`unicore-center --tab settings:appearance`) to switch with 1 click between:
- **Win11 Centered** (default centered taskbar)
- **Win10 Classic** (left-aligned taskbar)
- **macOS Dock** (floating bottom dock)
- Plus **Dark / Light / High Contrast** themes and **6 Accent Colors**!

---

## 2. Booting Live USB, Safe Dual-Boot & LUKS2 Install / Live USB, Dual-Boot & LUKS2

1. **Create Bootable USB:** Copy `unicore-os-1.0.0-x86_64.iso` directly onto a **Ventoy** USB drive (no reformatting needed!), or write with **Rufus / BalenaEtcher**.
2. **Secure Boot Support:** UniCore OS ships with Microsoft-signed `shim` + `MokManager`. On first UEFI Secure Boot launch, choose **Enroll MOK** (`ENROLL_UNICORE_MOK.cer`) once, or enroll from **System → Secure Boot** inside UniCore Center.
3. **Install to Disk (`System → Install to Disk`):**
   - **Install Alongside Windows / Dual-Boot:** Safely installs UniCore OS into unallocated free disk space without touching your Windows NTFS partition, and adds a clean **Windows Boot Manager** entry to GRUB!
   - **Full Disk (Wipe):** Formats the target drive with an immutable **A/B System Partition** layout (`UNICORE_SYS_A`, `UNICORE_SYS_B`, `UNICORE_DATA`).
   - **Optional LUKS2 Encryption:** Tick **Encrypt `/data` with LUKS2** and set a passphrase (minimum 8 characters) to protect all personal documents, saved passwords, and app sandboxes at rest.

---

## 3. Apps, Store & Classic 98/XP/DOS Gaming / Aplikasi, Kedai & Game Klasik 98/XP

### Unified Game Hub (`unicore-center --games`)
Open **Start Menu → Unified Game Hub / Hab Permainan** to see all your games in one clean grid:
- **Windows 98/XP/DOS (`uniload`)**, **Steam**, **Heroic (Epic/GOG)**, **Lutris**, **RetroArch**, and **Android (`.apk`)**.
- Toggle **Game Mode** (CPU performance governor) and **FPS HUD** (MangoHud / DXVK HUD) with 1 click.
- **Gamepad & Classic Mapper (`--gamepad`):** Detects Xbox, PlayStation, Switch & 8BitDo controllers (USB/Bluetooth) with live battery readout, `evtest` button tester, and a built-in **Pure-Rust `uinput` Gamepad → Keyboard/Mouse mapper** (`WASD + Mouse`, `Arrows + Space`, `MS-DOS Classic`) for vintage games with no controller support!
- **Auto-Backup Game Saves & Cloud Sync (`--saves`):** Automatically saves up to 5 snapshots per sandbox game to `~/Backups/GameSaves` on exit, with 1-click sync to your mounted **Cloud Drive** (`~/Cloud/<Drive>/UniCore-GameSaves`).

### Playing Classic 1998–2008 Windows & DOS Games (`--retro-game`)
Double-click any `.exe`, `.msi`, CD `.iso`, or `.zip` game — or open **Classic 98/XP Wizard**:
- Automatic era presets: **Win98 2D DirectDraw (`cnc-ddraw`)**, **WinXP 3D DirectX 8/9 (`DXVK`)**, **Win16 (`OTVDM`)**, and **MS-DOS (`DOSBox-Staging`)**.
- Built-in 1-click fixes: **Cap 60 FPS** (stops speed bugs), **Lock 1 CPU Core (`taskset`)** (stops multi-core freezes), **Safe 800×600 / 1024×768 Virtual Window**, **CD `.iso` auto-mount as `D:\`**, and **1-Click Crash Auto-Repair** (auto-detects missing `d3dx9` / `vcrun` / `ddraw` DLLs)!

---

## 4. Phone Link, Cloud Drives, Windows Migration & Hardware Drivers

Open **Settings → Phone & Cloud** or launch directly from Start Menu:
1. **Phone Link (`--phone`):**
   - **KDE Connect (`24.02.2`):** Sync phone notifications, SMS, shared clipboard, touchpad mouse, and Wi-Fi file sharing. Stays at **`0 MB` idle RAM** until enabled!
   - **Android Screen Mirror (`scrcpy` + `adb`):** Mirror and control your Android screen over USB or Wi-Fi (`IP:5555`), with optional `--screen-off` battery-saver mode.
2. **1-Click Cloud Drives (`--cloud`):**
   - Connect **Google Drive, OneDrive, Dropbox, MEGA, or Nextcloud/WebDAV** via `rclone` VFS cache into `~/Cloud/<Name>` (visible directly in File Explorer sidebar).
   - Credentials are locked to `chmod 0600` inside `/data/home/.config/rclone/rclone.conf`.
3. **Safe Windows Migration Wizard (`--migrate`):**
   - Mounts Windows NTFS partitions strictly **Read-Only (`ro`)** — zero risk to your Windows installation!
   - Copies **Documents, Pictures, Desktop, Music, Videos, Saved Games, Windows Fonts**, converts **Chrome & Edge Bookmarks** to importable HTML, and scans `Program Files` to recommend matching apps in **UniCore Store**.
4. **Hardware & Driver Health (`--drivers`):**
   - Plain-language status for GPU, Vulkan, Wi-Fi, Audio, USB devices, and missing firmware alerts.
   - 1-click fixes: **Safe GPU Mode (Pixman)** for ancient/quirky GPUs, **Disable Wi-Fi Power-Save** for unstable laptops, **1-Click Reset Audio & Bluetooth**, and on-demand **LVFS Firmware Updates (`fwupdmgr`)**.

---

## 5. Zero-Trust Security, Firewall & Encrypted Backups

- **App Sandbox Confinement (`bubblewrap`):** Every app runs with `--cap-drop ALL`, `--die-with-parent`, `--new-session`, `--unshare-uts`, private `/tmp` and `/data`, hidden `/etc/doas.conf` and `/etc/shadow`, and per-app switches for **Network, GPU, Documents, Camera, and Microphone**.
- **Stateful Firewall (`nftables` / `unicore_fw`):** Drops unsolicited inbound traffic by default. *Home* profile allows local mDNS (`5353`) and KDE Connect LAN (`1714–1764`); *Public* profile locks all inbound ports tight.
- **0-MB-Idle Architecture:** Heavy optional services (`kdeconnectd`, `cupsd`, `fwupd`, `ollama`, `bluetoothd`, `waydroid`) consume **0 MB RAM** until you turn them on.
- **Encrypted `/data` Backups (`Settings → Security`):** Back up all personal files, sandboxes, and Wi-Fi/settings to local disk or USB with **AES-256-CBC (PBKDF2 100,000 iterations) + SHA-256 verification** and Tar-Slip protection.
- **Live Security Audit:** Run `/usr/libexec/unicore-security-audit` or click **Run Security Audit** inside **User Guide** (`--help-guide`) to verify all zero-trust controls in real time.

---

## 6. Immutable A/B OTA Updates, Auto-Rollback & Troubleshooting

- **Signed Ed25519 Updates:** Every official release manifest and catalog is signed offline with Ed25519 (`REQUIRE_SIGNATURE=1` verified against `/etc/unicore/update.pub`) plus SHA-256 verification of kernel, initramfs, and SquashFS.
- **Zero-Downtime A/B Slots + Automatic Rollback:**
  - Updates flash cleanly to the standby slot (`UNICORE_SYS_B` while running `UNICORE_SYS_A`) without touching `/data`.
  - On reboot, `unicore-boot-health` verifies rootfs, `/data`, `uniload`, `unicore-center`, and the Wayland desktop session **before** confirming the boot (`confirm-boot`).
  - If an update ever fails to boot twice, GRUB **automatically rolls back** to your previous working slot!
  - You can also manually trigger a rollback anytime from **System → OS Updates → Rollback to Previous Slot**.
- **Collecting Hardware Diagnostics (`unicore-diag`):** If you encounter a hardware quirk, run `unicore-diag --save-usb` (or click **Export Diagnostics** in System tab) to save a clean, privacy-safe report bundle.

---

## 7. 1-Click Web Apps (PWA) & Family Parental Controls / Aplikasi Web & Kawalan Ibu Bapa (`v1.1.0`)

* **1-Click Web Apps (`unicore-center --webapps`):** Turn everyday websites (**WhatsApp Web, YouTube, Gmail, Microsoft 365, Google Docs, Canva, ChatGPT, Spotify**, or any custom `https://` site) into clean standalone desktop apps in your Start Menu with isolated browser profiles and **`0 MB` idle RAM**.
* **Family Safety & Parental Controls (`unicore-center --parental`):**
  * **Family Safe DNS:** Enable Cloudflare for Families (`1.1.1.3` blocks malware + adult content, or `1.1.1.2` malware-only) persisted cleanly at boot via NetworkManager.
  * **Daily Screen-Time Limit & Bedtime Curfew:** Set daily usage limits (`60–240 min`) and bedtime windows (`21:00–07:00`, `22:00–07:00`, etc.) with 1-click daily timer reset.
  * **Per-App Blocklist:** Restrict specific sandbox apps or web apps (`webapp:youtube`, `steam`, etc.) cleanly without any heavy background daemon.

---

*Built with care for everyday reliability — UniCore OS v1.1.0.*
