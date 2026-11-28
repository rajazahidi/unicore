# UniCore OS `v2.0.0` — Official User Manual (English)

Welcome to **UniCore OS `v2.0.0` (Sovereign Desktop Edition)** — a fast, modern, zero-bloat desktop operating system designed to feel immediately familiar while giving you bank-grade privacy, native Post-Quantum VPN, studio-grade audio DSP, and effortless compatibility with Linux, Desktop PC (`.exe`/`.msi`/DOS), Web Apps, and Mobile (`.apk`) applications.

---

## 1. Getting Around Your Desktop

### 1.1 Start Menu, Taskbar & Right-Click Menus
- **Start Menu (`Super` / Meta key or bottom-left grid icon):** Instant search across all installed apps, 8 pinned quick-launch apps, category tabs (*All, Internet, Office, Media, Games, System*), personal folder shortcuts (*Docs, Downloads, Pictures*), Emoji Picker, and Power menu.
- **Taskbar:** Displays pinned shortcuts, open windows, System Tray (background apps & USB Safe Eject), Audio/Volume indicator, AI Copilot pill (`AI`), Quick Settings drawer, Clock (24h or 12h), and Power button.
- **Desktop Right-Click Menu:** Right-click any empty space on the desktop to **Create New Folder**, **Create New Text Document**, open **Control Panel**, **Device Manager**, **Storage Manager**, **Display Settings**, **10 Desktop Themes**, **AegisTunnel VPN**, **ResonanceFX Equalizer**, or **Terminal**.
- **Computer Explorer (`Super + E`):** Browse local folders, internal drives, USB sticks (auto-mounted plug-and-play), and `~/Cloud` drives. Right-click any file or folder to **Open Terminal Here**, **Copy Path**, **Print File**, **Extract Here / Compress to ZIP**, or **Run in Sandbox**.

### 1.2 Essential Keyboard Shortcuts
| Shortcut | Action |
| :--- | :--- |
| `Super` (Meta) | Toggle UniCore Start Menu |
| `Super + E` | Open Computer Explorer |
| `Super + A` | Open AI Copilot |
| `Super + .` | Open Emoji Picker |
| `Super + V` | Open Clipboard History |
| `Super + L` | Lock Screen (frosted blur) |
| `PrintScreen` | Open Screen Capture & MP4 Recorder |
| `Ctrl + Shift + Esc` | Open Task Manager |
| `Alt + Shift` | Switch active non-Latin / US keyboard script |
| `Alt + Tab` | Switch between open windows |

---

## 2. UniCore Control Panel (`--control-panel`)

Open **Start Menu → Control Panel** for a clean 2×5 unified hub covering every system setting:

1. **Wi-Fi & Network:** Connect to Wi-Fi/Ethernet, manage saved networks, mount SMB/SFTP/WebDAV network shares, and configure WireGuard/OpenVPN profiles.
2. **Audio & Bluetooth:** Adjust PipeWire volume/microphone, switch speakers/headphones/Bluetooth A2DP profiles, or launch **ResonanceFX Equalizer**.
3. **Display & Style:** Configure single or multi-monitor layouts (`Left Of`, `Right Of`, `Mirror`), resolution, scaling, screen rotation (`0°/90°/180°/270°`), Night Light, laptop & external DDC/CI monitor brightness, taskbar layout, and **10 Desktop Themes**.
4. **Region, Language & Keyboard (`--region`):**
   - **12 International Languages:** English, Bahasa Melayu, Bahasa Indonesia, Simplified Chinese, Japanese, Korean, Arabic, Spanish, French, German, Portuguese, and Russian.
   - **21 Global Keyboard Layouts:** Including QWERTY, AZERTY, QWERTZ, Dvorak, Colemak, Arabic, Russian, Japanese, Korean, Chinese, Thai, and Vietnamese (non-Latin keyboards automatically pair with US English — press `Alt + Shift` anytime to switch!).
   - **32 World Timezones & 24h/12h Clock Format.**
5. **UniCore Device Manager (`--devmgr`):** Live categorized hardware tree showing CPU, GPU, Network, Audio, Camera, Storage, USB, Bluetooth, Input HID, and SANE Scanners with active kernel drivers.
6. **UniCore Storage Manager (`--storage`):** Inspect drives and partitions, check S.M.A.R.T. health, safely unmount USB drives, and clean temporary cache files.
7. **Printers & Scanners:** Start on-demand CUPS printing (`0 MB` idle when off), discover USB/Wi-Fi printers (IPP Everywhere + Gutenprint), add/remove queues, and detect live SANE document scanners (`Simple Scan`).
8. **Security & Backup:** Switch Firewall (`Home/LAN` vs `Public Wi-Fi`), manage user accounts, create AES-256 encrypted `/data` backups, or launch **AegisTunnel VPN**.
9. **AegisTunnel VPN (`--vpn`):** Native Post-Quantum & Anti-DPI VPN studio.
10. **ResonanceFX Equalizer (`--equalizer`):** Native PipeWire 10-band equalizer & spatial audio studio.

---

## 3. Native AegisTunnel VPN (`--vpn`)

UniCore OS embeds the pure-Rust **AegisTunnel VPN** engine natively at **`0 MB` idle RAM** when disconnected:
- **Setup:** Open **Control Panel → AegisTunnel VPN**, click **Generate Keypair** (keys are saved strictly `chmod 0600`), enter your Server Endpoint and Peer Public Key (or test immediately using **Local Loopback Test Mode**), and click **Connect AegisTunnel**.
- **Anti-DPI & Stealth Transport:** Choose **Auto Happy Eyeballs (UDP + REALITY Fallback)** to start on high-speed AmneziaWG-obfuscated UDP and automatically fail over to **REALITY TLS 1.3 TCP/443** if a network blocks VPN traffic.
- **Hybrid Post-Quantum Security:** Keep **Hybrid PQ ML-KEM-768** enabled and click **Rotate PQ Key** anytime while connected to refresh your post-quantum session key without dropping your connection.
- **Local DNS Sinkhole:** Blocks known ad and tracker domains directly inside the tunnel.

---

## 4. Native ResonanceFX PipeWire Equalizer (`--equalizer`)

Transform speakers or headphones with the built-in C++17 **ResonanceFX Audio DSP Studio** (`0 MB` idle RAM when off):
1. Open **Control Panel → ResonanceFX Equalizer** and click **Enable PipeWire DSP**.
2. Choose one of **7 1-Click Acoustic Presets**: *Flat Reference*, *Pure Bass Boost*, *Vocal & Podcast Studio*, *Cinema 3D Surround*, *Hi-Fi Warm Tube*, *Gaming Footsteps*, or *8D Spatial Orbit Live*.
3. Fine-tune the **10-Band Graphic Equalizer (31 Hz–16 kHz)**, Preamp, **Pure Bass**, **Silky Clarity**, **3D Surround**, **Tube Warmth**, **Binaural / 8D Orbit**, **UPOLA Convolver IR**, **Schroeder Reverb**, **Stereo Chorus**, **Stereo Delay**, and **Loudness**, then click **Apply Live to PipeWire** for instant glitch-free updates.
4. Click **Disable PipeWire DSP (0 MB)** anytime to unload the filter completely.

---

## 5. Apps, Web Apps, Mobile Subsystem & Classic PC Games

- **Universal App Store (`Start Menu → App Store`):** Install verified Linux (Flatpak), Desktop PC, Productivity, Media, and Gaming apps in 1 click with full sandbox permissions control (Network, GPU, Documents, Camera, Microphone).
- **1-Click Web Apps (`--webapps`):** Turn everyday websites into clean standalone Start Menu apps with isolated browser profiles (`0 MB` idle RAM).
- **Mobile App Subsystem (`UniCore Center → System → Mobile Subsystem`):** Run `.apk` apps natively in multi-window mode via Waydroid (`0 MB` idle until launched), with 1-click **Device GSF Certification Helper** (`google.com/android/uncertified`) and **Open App Market (Aurora)** installation.
- **Simplified Classic & Legacy PC Games (`Start Menu → Classic Games` / `--retro-game`):**
  1. Select any `.exe`, `.msi`, Game CD `.iso`, `.zip`/`.7z`, or game folder.
  2. Pick 1 of **5 plain-language Auto-Tune cards**: **Smart Auto**, **90s 2D Classic**, **2000s 3D Classic**, **Retro DOS / 16-Bit**, or **Ultra-Safe Legacy** (automatically locks 1 CPU core, caps 60 FPS, and uses a safe `800×600` window to prevent speed bugs & crashes on modern CPUs!).
  3. Open **Unified Game Hub (`--games`)** to launch all your games, configure USB/Bluetooth controllers (`--gamepad` with Keyboard/Mouse mapper), and back up saves automatically (`--saves`).

---

## 6. 10 Themes, Phone Link, Cloud Drives, Family Safety & Updates

- **10 Desktop Themes (`--themes`):** Switch wallpaper, taskbar, window frames, icons, terminal, notifications, and app colors together in 1 click across *Midnight, Daylight, Aurora, Nordic, Dracula, Emerald, Sunset, Sakura, Cyberpunk,* and *Retro Classic 98*.
- **Phone Link (`--phone`):** Sync phone notifications, SMS, shared clipboard, and wireless file sharing (KDE Connect, `0 MB` idle) plus USB/Wi-Fi screen mirroring (`scrcpy`).
- **Cloud Drives (`--cloud`):** Mount external cloud storage (OAuth/WebDAV/Nextcloud) directly into `~/Cloud`.
- **Safe PC Data Migration (`--migrate`):** Import Documents, Pictures, Music, Videos, Browser Bookmarks, and Fonts from existing NTFS drives mounted strictly **Read-Only**.
- **Family Safety & Parental Controls (`--parental`):** 1-click Family Safe DNS (`1.1.1.3`), daily screen-time limits, bedtime curfew, and per-app blocking.
- **Zero-Downtime A/B OS Updates (`System → OS Updates`):** Cryptographically signed with Ed25519; updates flash to the inactive system slot and automatically roll back if a boot health check ever fails.
