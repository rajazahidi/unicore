# UniCore OS — Road to `v1.0.0` Perfection & Bare-Metal Readiness Checklist

This checklist defines the concrete engineering environment, hardware verification gates, and production milestones required to graduate UniCore OS from **`v0.3.0` (Feature-Complete Multi-Arch)** to **`v1.0.0` (Production Daily Driver)**.

---

## 1. Local Development & Perfection Environment (Ready Now)

Use the unified host controller (`scripts/unicore-env.ps1`) for all workflows:

```powershell
# 1. Audit local host toolchain, disk space, hooks, QEMU/UEFI, keys & Ventoy USB
.\scripts\unicore-env.ps1 doctor

# 2. Run full local CI mirror (LF check, clippy -D warnings, cargo test, musl check, py_compile)
.\scripts\unicore-env.ps1 check

# 3. Incremental ISO rebuild (~20-25 min) or clean full rebuild (~55-70 min)
.\scripts\unicore-env.ps1 build-fast
.\scripts\unicore-env.ps1 build

# 4. Run full 5-Gate QEMU matrix (BIOS, UEFI, 2005 512MB, Host SHA, Ventoy USB3+NVMe) + 3-Stage A/B LUKS2
.\scripts\unicore-env.ps1 qa

# 5. Deploy & SHA-256 verify onto connected Ventoy USB stick
.\scripts\unicore-env.ps1 deploy-ventoy

# 6. Start Local LAN & QEMU OTA Update Server (port 8090) for live A/B update testing
.\scripts\unicore-env.ps1 ota-serve

# 7. Import hardware diagnostics collected on real PCs (`unicore-diag --save-usb`)
.\scripts\unicore-env.ps1 import-diag

# 8. Reclaim disk space from disposable builder intermediates
.\scripts\unicore-env.ps1 clean
```

---

## 2. Bare-Metal Hardware Sign-Off Matrix (Required for `v1.0.0`)

Collect a real diagnostic bundle on each target machine (`unicore-diag --save-usb` -> `.\scripts\unicore-env.ps1 import-diag`):

| Target Class | Example Machine | Boot (Ventoy/Rufus) | Display & Cursor | Wi-Fi / LAN | Audio | Input (KB/Mouse/Touchpad) | Suspend / Resume | Disk Install (A/B + LUKS2) |
|---|---|---|---|---|---|---|---|---|
| **2005–2009 Legacy BIOS PC** | HP Compaq dx2355 (2 GB DDR2, GeForce 6150SE, TP-Link TL-WN851ND `ath9k`) | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| **Modern UEFI Laptop** | ASUS Laptop (NVMe, Intel/AMD Wi-Fi, Touchpad, Battery) | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |
| **Low-RAM PC (512 MB–1 GB)** | 2005 Pentium 4 / Atom (`legacy_ultra` + `zram0`) | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] | [ ] |

---

## 3. Production Security, OTA Hosting & Release Infrastructure

- [x] **GitHub Remote + CI Active:** Push repository to GitHub (`rajazahidi/unicore`) so `.github/workflows/ci.yml` and `.github/workflows/release.yml` run automatically on every commit and tag.
- [x] **Production Ed25519 Release Signing & Signed Online Catalog (`v0.3.0`):**
  - Generate offline Ed25519 keypair (`.\scripts\sign-release.ps1 -GenerateKey` or `uniload update keygen --private release.key --public update.pub`).
  - Store `release.key` and `update.pub` in GitHub Repository Secrets (`UNICORE_RELEASE_KEY`, `UNICORE_RELEASE_PUB`) — never commit private keys to Git.
  - Per-arch OTA manifests (`unicore-manifest-x86_64.json`, `unicore-manifest-aarch64.json`, plus backward-compatible `unicore-manifest.json` alias) and online Store catalog (`unicore-catalog.json`) are signed and verified with `REQUIRE_SIGNATURE=1`.
- [x] **UEFI Secure Boot Support (`v0.3.0`):**
  - Upstream Microsoft-signed `shimx64.efi`/`shimaa64.efi` + `MokManager` (`mmx64.efi`/`mmaa64.efi`) + UniCore MOK-signed standalone GRUB & Linux kernel (`vmlinuz-*`), plus `ENROLL_UNICORE_MOK.cer` and GUI enrollment panel in `unicore-center`.
- [x] **Dual-Architecture Releases (`x86_64` + `aarch64`, `v0.3.0`):**
  - Simultaneous signed ISOs for `x86_64` (BIOS/UEFI hybrid) and `aarch64` (UEFI + embedded isolated Box64 + Wine 11.0 WoW64 runtime).
- [ ] **Over-the-Network A/B OTA Test on Real Hardware:**
  - Serve a new build with `.\scripts\unicore-env.ps1 ota-serve` and apply an A/B Slot B update + rollback test on a real installed PC over Wi-Fi/LAN.

---

## 4. Real-Hardware Application & Subsystem Smoke Suite

- [ ] **Daily Desktop:** Epiphany browser playback (YouTube/HTML5), AbiWord/Gnumeric/Evince, CUPS network/USB printing, SANE document scanning.
- [ ] **Legacy & Modern Gaming:** DOSBox-Staging retro title, Win16 NE (`OTVDM`), Win98/XP DirectDraw (`cnc-ddraw`), DirectX 9–11 (`DXVK` on Vulkan-capable GPU).
- [ ] **Developer & Local AI:** Rootless Podman container build/run in `/data/dev`, Ollama on-demand install/pull/stop (`0 MB` idle RAM) on >= 4 GB RAM machine.
- [ ] **Android (Waydroid):** First-time init, APK install via `uniload`, clipboard sync, clean stop/reclaim.
