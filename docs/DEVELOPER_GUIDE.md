# UniCore OS — Developer & QA Engineering Guide (`v2.0.0`)

**Document ID:** `UC-DEV-2.0`  
**Audience:** OS Engineers, Rust/C++/Slint Contributors, QA Automation Engineers

---

## 1. Repository Structure

```text
unicore/
├── Cargo.toml                  # Rust workspace (uniload, unicore-center, aegis-proto, unicore-aegis-vpn)
├── CHANGELOG.md                # Semantic release history (Keep a Changelog)
├── core/
│   ├── orchestrator/           # `uniload` — binary inspector, bwrap planner, A/B OTA, Ed25519, gamepad uinput
│   ├── center/                 # `unicore-center` — Slint GUI (~22 MB RSS), Control Panel, Store, VPN & EQ studios
│   │   ├── src/                # Rust backend modules (cli, i18n, settings, aegis_rfx, start_emoji, retro, etc.)
│   │   ├── ui/app.slint        # Unified Slint UI specification & UiTheme tokens
│   │   └── tests/              # Integration & unit tests (center_tests.rs)
│   ├── aegis-proto/            # Pure-Rust AmneziaWG obfuscator, REALITY TLS 1.3 mimicry & Hybrid ML-KEM-768 KEM
│   ├── aegis-vpn/              # `unicore-aegis-vpn` — static musl WireGuard + Linux TUN + DNS Sinkhole + Handover
│   └── resonance-fx/           # Pure C++17 PipeWire LADSPA 1.1 plugin (`libunicore_resonance_fx.so` + self-test)
├── os-builder/
│   ├── build-rootfs.sh         # Alpine 3.22 rootfs, packages, theme assets, kernel/initramfs & Hybrid ISO builder
│   ├── initramfs-init          # Stage-1 universal boot script (Ventoy ISO loopback, LUKS2, OverlayFS, A/B slots)
│   ├── overlay/                # Rootfs overlay (/etc, /usr/bin, /usr/sbin, /usr/libexec, /usr/share)
│   ├── arm64-run-wine-box64.sh # Isolated ARM64 glibc + Box64 + Wine WoW64 launcher
│   └── keys/update.pub         # Public Ed25519 release verification key
├── scripts/
│   ├── unicore-env.ps1         # Unified Windows dev controller (doctor, check, build, build-fast, qa, ota-serve)
│   ├── cargo-dev.ps1           # Auto-detects MSVC/Windows SDK LIB+INCLUDE and rust-lld for musl cross-checks
│   ├── test-release-matrix.py  # 5-Gate x86_64 QEMU QA + 20 live Wayland desktop screenshots
│   ├── test-ab-install-luks.py # 3-Stage Bare-Metal GPT A/B + LUKS2 + Dual-Boot + Recovery QEMU QA
│   ├── test-aarch64-qemu.py    # ARM64 UEFI + Shim/MOK + Box64/Wine live cmd.exe QEMU QA
│   └── ota-local-server.py     # Local LAN / QEMU HTTP OTA test server
├── docs/                       # Industry-standard documentation portal
└── .github/workflows/
    ├── ci.yml                  # Push/PR CI (LF, sh -n, clippy -D warnings, cargo test, C++17 DSP test, musl check)
    ├── release.yml             # Multi-arch release builder (x86_64 + aarch64), Secure Boot MOK & Ed25519 signing
    └── qa-release.yml          # 3-Job Cloud QEMU QA verification against published GitHub Release assets
```

---

## 2. Local Development Environment Setup

### 2.1 Prerequisites
- **Rust (`rustup` stable)** with `clippy` and targets `x86_64-unknown-linux-musl` + `aarch64-unknown-linux-musl`
- **C/C++ Toolchain:**
  - *Windows Host:* Visual Studio Build Tools (`MSVC x64` + Windows 10/11 SDK — auto-discovered by `scripts/cargo-dev.ps1`)
  - *Linux Host:* `gcc`, `g++`, `make`, `musl-tools`
- **Python 3.10+** (QA harness & build utilities)
- **QEMU + EDK2 OVMF** (optional for local ISO VM testing)

### 2.2 Everyday Developer Commands (Windows PowerShell)
```powershell
# Inspect local toolchain health, disk space, Git hooks & USB drives
.\scripts\unicore-env.ps1 doctor

# Run the complete local CI check suite (LF, clippy -D warnings, 77+ tests, musl, py_compile)
.\scripts\unicore-env.ps1 check

# Run targeted Cargo commands with auto-configured MSVC LIB/INCLUDE
.\scripts\cargo-dev.ps1 test -p unicore-center
.\scripts\cargo-dev.ps1 test -p unicore-aegis-vpn
```

### 2.3 Building & Testing ResonanceFX C++17 DSP (Linux / Alpine)
```sh
make -C core/resonance-fx test     # Compiles libunicore_resonance_fx.so + unicore-resonance-test and runs 48kHz DSP + bypass self-test
make -C core/resonance-fx clean
```

---

## 3. Coding Standards & Engineering Gates

Every Pull Request and release commit MUST pass the following hard gates:

1. **Strict LF Line Endings:** Every Linux script and configuration file under `os-builder/` and `core/resonance-fx/` MUST use `\n` (LF), never `\r\n` (CRLF).
2. **POSIX Shell Verification:** Every `#!/bin/sh` script under `os-builder/overlay/` and `os-builder/initramfs-init` must pass `sh -n`, and `os-builder/build-rootfs.sh` must pass `bash -n`.
3. **Zero Rust Compiler / Clippy Warnings:** `cargo clippy --workspace --all-targets -- -D warnings` must exit `0` across all 4 crates (`uniload`, `unicore-center`, `aegis-proto`, `unicore-aegis-vpn`).
4. **Zero Unwired Slint Callbacks:** Every `callback` declared on the root `MainWindow` in `core/center/ui/app.slint` MUST be wired via `.on_<name>(...)` in `core/center/src/main.rs`.
5. **Verified `0 MB`-Idle Invariant:** Never auto-start optional daemons (`unicore-aegis-vpn`, `unicore-resonance-ctl`, `cupsd`, `kdeconnectd`, `ollama`, `fwupd`) in `/etc/local.d/` or `labwc/autostart`.
6. **Sovereign Terminology:** Keep all user-facing strings, menus, and documentation clean of third-party OS/store branding (preserve protocol/file-format names such as `.exe`, `.apk`, `NTFS`, `SMB`, `WireGuard`, `PipeWire`, and official `google.com/android/uncertified`).
7. **Privilege Safety:** New root operations must be added as strictly argument-validated subcommands inside `os-builder/overlay/usr/libexec/unicore-sys-helper` with `umask 077` `mktemp` handling — never widen `/etc/doas.d/unicore.conf`.

---

## 4. Architecture of Automated Multi-Arch CI & Cloud QEMU QA

### 4.1 `UniCore CI` (`.github/workflows/ci.yml`)
Triggered on every push and PR to `main`:
- Verifies LF line endings, `bash -n` / `sh -n`, `py_compile`, and `ota-local-server.py --self-test`.
- Runs `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` (77+ unit & integration tests).
- Compiles & executes `make -C core/resonance-fx test` (C++17 LADSPA 1.1 `dlopen` + 11-stage DSP + bypass verification).
- Verifies cross-architecture `musl` compilation via `musl-gcc` (`x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`).

### 4.2 `UniCore OS Release` (`.github/workflows/release.yml`)
Triggered on `v*` semantic version tags across native `ubuntu-latest` (`x86_64`) and `ubuntu-24.04-arm` (`aarch64`) runners:
1. Builds static musl `uniload` & `unicore-aegis-vpn` (`musl-gcc` + `rust-lld -C link-self-contained=yes`).
2. Extracts native Alpine 3.22 `musl` dynamic sysroot and builds `libunicore_resonance_fx.so` + `unicore-resonance-test` + dynamic musl `unicore-center`.
3. Runs native Alpine 3.22 container smoke-tests on all 3 binaries (`unicore-center --check`, `unicore-aegis-vpn self-test`, `unicore-resonance-test`).
4. Packages offline Win32/Win16 runtimes (`x86_64`) or builds & smoke-tests the isolated `Box64` + `Wine 11.0 WoW64` runtime (`aarch64` executing live `cmd.exe`).
5. Builds SquashFS + Hybrid ISO + Microsoft-signed UEFI `shim`/MOK-signed GRUB + offline **Ed25519** signed manifests & Store catalog (18 total release assets).

### 4.3 `UniCore OS Release QEMU QA` (`.github/workflows/qa-release.yml`)
Validates published release artifacts inside real QEMU virtual machines across 3 parallel jobs:
- **`qa-x86_64` (5 Gates):**
  - *Gate 1 (Legacy BIOS Live ISO):* Verifies `VERSION_ID`, all `unicore-*` helpers, `unicore-security-audit` (`idle_heavy_count=0`), `unicore-boot-health`, live persona simulation (HTTPS MIME, HTTP download, `mpv` audio/video, Wine/DOSBox, on-demand CUPS print job + 0 MB teardown, live **ResonanceFX** PipeWire sink start/stop, live **AegisTunnel** `aegis0` start/PQ-rotate/stop, 2-click Start Menu toggle, 10-Theme live switching), Ed25519 sign + tamper rejection, and captures **20 live Wayland screenshots**.
  - *Gate 2 (UEFI EDK2 OVMF):* Verifies `/sys/firmware/efi`, `shim` + MOK certificate & signed `grubx64.efi`.
  - *Gate 3 (2005 Legacy 512 MB PC):* Boots with 1 vCPU (`qemu64,-sse4.2,-avx`), 512 MB RAM; verifies `legacy_ultra`, `zram0`, and `unicore-center --check`.
  - *Gate 4 (Host Integrity):* Verifies `.iso.sha256` and `uniload update verify-manifest`.
  - *Gate 5 (Ventoy USB 3.0 xHCI + NVMe):* Verifies ISO loopback boot over simulated Ventoy USB 3.0 alongside an internal NVMe SSD.
- **`qa-ab-dualboot` (3 Stages):** Bare-metal GPT install with LUKS2 encryption + safe Dual-Boot → Slot A unlock + live Slot B OTA update → Slot B boot verifying encrypted `/data` persistence, `unicore-boot-health`, `unicore-security-audit`, and GRUB recovery rollback.
- **`qa-aarch64`:** Boots published ARM64 ISO under UEFI `AAVMF`, verifies `shim` + MOK + live `Box64` + `Wine WoW64` executing `cmd.exe`.
