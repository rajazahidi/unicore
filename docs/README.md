# UniCore OS Documentation Portal (`v2.0.0`)

Welcome to the official engineering, operational, security, and end-user documentation suite for **UniCore OS `v2.0.0` (Sovereign Desktop Edition)** — a lightweight, immutable, zero-trust desktop operating system engineered on **Alpine Linux 3.22 (`musl`)**, **Linux Kernel 6.15 (`linux-edge`)**, **Mesa 25.1**, **Wayland (`labwc` + `waybar`)**, **PipeWire**, and **Rust + Slint** for `x86_64` (2005 512 MB legacy PCs through 2026 Intel/AMD/NVIDIA systems) and `aarch64` (ARM64 UEFI).

---

## Documentation Index by Audience

| Audience | Document | Scope & Contents |
| :--- | :--- | :--- |
| **End Users (English)** | [User Manual (English)](USER_MANUAL_EN.md) | Complete desktop handbook: Start Menu, Control Panel, 12 Languages, 21 Keyboards, 32 Timezones, 10 Themes, AegisTunnel VPN, ResonanceFX Equalizer, Classic Games, Printers/Scanners, Phone Link & Cloud Drives. |
| **Pengguna Akhir (BM)** | [Manual Pengguna (Bahasa Melayu)](USER_MANUAL_MS.md) | Panduan lengkap Bahasa Melayu merangkumi semua menu, Pusat Kawalan, Bahasa/Papan Kekunci, Pengurus Peranti & Storan, VPN AegisTunnel, Equalizer ResonanceFX, Game Klasik & Keselamatan. |
| **Offline Quick Reference** | [Built-In Bilingual Guide](user-guide.md) | Compact bilingual reference also bundled directly inside UniCore OS (`Start Menu → User Guide` / `unicore-center --help-guide`). |
| **System Architects** | [System Architecture](ARCHITECTURE.md) | 5-layer architecture, immutable A/B SquashFS + OverlayFS + LUKS2 design, `0 MB`-idle lifecycle, `uniload` multi-era runtime, AegisTunnel PQ-VPN & ResonanceFX PipeWire DSP pipelines. |
| **Security Auditors** | [Security Architecture & Threat Model](SECURITY.md) | Threat model, UEFI Secure Boot (`shim` + MOK), Ed25519 signed OTA & catalog, `bubblewrap` confinement, `doas` privilege boundaries, LUKS2/AES-256 encryption, ML-KEM-768 PQ KEM & live security audit. |
| **IT & Fleet Administrators** | [Administration & Operations Guide](ADMIN_AND_OPERATIONS.md) | Bare-metal/Ventoy/Dual-Boot/LUKS2 deployment, A/B OTA hosting & air-gapped updates, Ed25519 key management, health-gated rollback, CUPS/SANE/SMB/firewall operations & troubleshooting runbook. |
| **Hardware Evaluators** | [Hardware Compatibility Matrix](HARDWARE_COMPATIBILITY.md) | Tiered hardware profiles (`legacy_ultra` 512 MB → `modern` 2026 & `aarch64`), GPU/NVIDIA NVK+GSP/`DRI_PRIME`, Wi-Fi/BT firmware, SOF audio, USB4/Thunderbolt, printers, scanners & gamepads. |
| **Core Contributors** | [Developer & QA Engineering Guide](DEVELOPER_GUIDE.md) | Source layout, Rust workspace (`uniload`, `unicore-center`, `aegis-proto`, `unicore-aegis-vpn`), C++17 `ResonanceFX` LADSPA plugin, Slint UI architecture, ISO build pipeline & 5-Gate + A/B + ARM64 QEMU QA. |
| **Release Managers** | [Release History (`CHANGELOG.md`)](../CHANGELOG.md) | Complete semantic version history from `v0.1.0` through `v2.0.0` (Keep a Changelog format). |

---

## Core Engineering Invariants

Every release of UniCore OS is continuously verified against six non-negotiable engineering invariants in automated CI and Cloud QEMU QA:

1. **Immutable A/B Core with Health-Gated Rollback:** The root operating system (`UNICORE_SYS_A` / `UNICORE_SYS_B`) is mounted read-only (`SquashFS`). Over-the-air updates write only to the inactive slot and are confirmed only after `unicore-boot-health` verifies rootfs, `/data`, `uniload`, `unicore-center`, and the live Wayland desktop session.
2. **Verified `0 MB`-Idle Architecture:** Heavy optional subsystems — **AegisTunnel VPN**, **ResonanceFX PipeWire DSP**, **CUPS printing**, **Bluetooth (`bluetoothd`)**, **Phone Link (`kdeconnectd`)**, **LVFS (`fwupd`)**, **Local AI (`ollama`)**, **Win32/Wine**, and **Mobile Container (`waydroid`)** — consume **`0 MB` RAM** at boot until explicitly activated by the user.
3. **Zero-Trust Confinement & Least Privilege:** Untrusted applications execute inside `bubblewrap` sandboxes (`--cap-drop ALL --new-session --die-with-parent --unshare-uts --unshare-ipc`) with explicit 5-permission toggles, while privileged system actions traverse a strictly whitelisted `/usr/libexec/unicore-sys-helper` via `doas`.
4. **End-to-End Cryptographic Integrity:** UEFI Secure Boot (`shim` + MOK), mandatory Ed25519 release manifest & App Store catalog verification (`REQUIRE_SIGNATURE=1`), SHA-256 artifact verification, optional LUKS2 `/data` encryption, and AES-256-CBC (PBKDF2-HMAC-SHA256) encrypted backups.
5. **Universal Hardware Longevity (2005–2026, `x86_64` + `aarch64`):** Baseline `x86-64-v1` compilation (no SSE4.2/AVX required), adaptive `zram` & renderer profiling (`pixman` → `gles2` → `vulkan`), early Intel/AMD CPU microcode, open-source NVIDIA NVK Vulkan 1.4 + GSP firmware, and isolated ARM64 `Box64` + `Wine 11.0 WoW64`.
6. **Sovereign Native Experience:** Clean, self-contained UniCore OS terminology, 12 international languages, 21 global keyboard layouts, 32 timezones, 10 cohesive desktop themes, embedded Post-Quantum VPN, and real-time 10-band PipeWire audio DSP with zero third-party telemetry.
