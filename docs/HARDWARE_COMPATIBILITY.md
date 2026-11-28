# UniCore OS — Hardware Compatibility Matrix (`v2.0.0`)

**Document ID:** `UC-HCL-2.0`  
**Kernel & Graphics Baseline:** Linux `6.15` (`linux-edge`) · Mesa `25.1` · Wayland (`labwc` / `wlroots`) · PipeWire + `pipewire-alsa` + SOF

---

## 1. Adaptive Hardware Tiers (`uniload hw-profile`)

At every boot, `uniload hw-profile` automatically detects system RAM, CPU instruction sets, and GPU acceleration capabilities to select the optimal runtime tier without manual tweaking:

| Profile Tier | Target Hardware Era | Minimum Specs | Rendering & Graphics | Memory & Shell Strategy |
| :--- | :--- | :--- | :--- | :--- |
| **`legacy_ultra`** | 2005–2010 PCs & Netbooks | 64-bit `x86-64-v1` (no SSE4.2/AVX needed), **512 MB–1.5 GB RAM** | `WLR_RENDERER=pixman` (software Wayland) + Wine GDI | `75%` `zram` swap · Lightweight shell · `bluetoothd` skipped |
| **`legacy_mid`** | 2010–2016 Laptops & Desktops | Dual-core `x86_64`, **1.5–3.5 GB RAM** or OpenGL-only GPU | `WLR_RENDERER=gles2` + `WineD3D` OpenGL | `50%` `zram` swap · Full desktop features |
| **`modern`** | 2016–2026 Modern PCs & Laptops | Multi-core Intel/AMD/NVIDIA, **>= 4 GB RAM** + Vulkan GPU | Wayland DMA-BUF + **Vulkan 1.4 (`DXVK`/`VKD3D-Proton`)** | `zram` + TLP battery optimization + full hardware offload |
| **`aarch64`** | Modern ARM64 UEFI Systems & SBCs | 64-bit ARMv8+ UEFI, **>= 2 GB RAM** | Native ARM64 Vulkan (`Panfrost`/`Freedreno`/`Broadcom`) + **Box64 + Wine 11.0 WoW64** | Isolated `glibc 2.39` Win32 compatibility runtime |

---

## 2. Subsystem & Peripheral Support Matrix

### 2.1 Processors & Firmware
| Component | Supported Hardware | Driver / Package Implementation |
| :--- | :--- | :--- |
| **Intel x86_64 CPUs** | Pentium 4 EM64T / Core 2 Duo through **Intel Core Ultra (Meteor/Lunar/Arrow Lake)** | `x86-64-v1` baseline + early `intel-ucode` prepended in Stage-1 `initramfs` |
| **AMD x86_64 CPUs** | Athlon 64 through **AMD Ryzen AI 300 / Zen 5** | `x86-64-v1` baseline + early `amd-ucode` prepended in Stage-1 `initramfs` |
| **ARM64 CPUs** | ARMv8 / ARMv9 UEFI desktops, laptops & single-board computers | Native `aarch64-unknown-linux-musl` OS + `Box64` dynamic recompiler |
| **BIOS / UEFI / Firmware** | Legacy BIOS (2005+), UEFI 2.x, Secure Boot, LVFS capsules | Hybrid BIOS+UEFI ISO · Microsoft-signed `shim` + MOK · On-demand `fwupdmgr` |

### 2.2 Graphics & Display
| Vendor / Feature | Supported Hardware | Status & Notes |
| :--- | :--- | :--- |
| **Intel Graphics** | Gen4 GMA → Iris Xe → **Arc / Xe2** | Native Mesa `iris`/`crocus` + `anv` Vulkan + VA-API hardware video decode |
| **AMD Radeon** | GCN → RDNA 1 / 2 / 3 / **RDNA 4** | Native Mesa `radeonsi` + `RADV` Vulkan + VA-API / VDPAU |
| **NVIDIA (Turing / Ampere / Ada)** | **GTX 16xx, RTX 20xx, 30xx, 40xx** | Open-source **Mesa NVK Vulkan 1.4 (`mesa-vulkan-nouveau`)** + **Open GSP Firmware (`NvGspRm=1`)** toggle in `--drivers` |
| **NVIDIA (Legacy / Pre-Turing)** | GeForce 6150SE, 8000–GTX 10xx | Automatic `nouveau` OpenGL / safe `Pixman` software mode fallback |
| **Hybrid Dual-GPU Laptops (Optimus)** | Intel/AMD iGPU + Discrete GPU | 1-click **Hybrid Discrete GPU (`DRI_PRIME=1`)** toggle in `--drivers` |
| **Multi-Monitor, Rotation & Brightness** | Internal eDP + HDMI / DisplayPort / USB-C | `wlr-randr` scaling/rotation (`0°/90°/180°/270°`), `brightnessctl` laptop backlight, `ddcutil` external DDC/CI monitor brightness |

### 2.3 Networking, Wireless & Bluetooth
| Component | Supported Chipsets | Tuning & Notes |
| :--- | :--- | :--- |
| **Intel Wi-Fi** | AC 7260/8265/9260, Wi-Fi 6/6E AX200/AX210, **Wi-Fi 7 BE200** | `iwlwifi` + `wireless-regdb` + power-save stability rules (`60-unicore-hardware.conf`) |
| **Realtek Wi-Fi** | `rtw88` (8821CE/8822CE) & `rtw89` (8852AE/8852BE/8852CE/8922AE) | Power-save & ASPM drops disabled in `/etc/modprobe.d/60-unicore-hardware.conf` |
| **Qualcomm Atheros & Broadcom** | `ath9k`, `ath10k`, `ath11k`, `ath12k` (`qca` firmware preserved in `slim`), `b43-fwcutter` | Full laptop Wi-Fi/BT coverage with zero missing firmware warnings |
| **Ethernet & USB Tethering** | Realtek `r8169`, Intel `e1000e`/`igc`, NVIDIA `forcedeth`, USB CDC/RNDIS | `NetworkManager` + `ethtool` + `usb-modeswitch` |
| **Bluetooth** | Intel, Realtek, Qualcomm, Cypress laptop BT 4.0–5.4 | On-demand `BlueZ` + `bluez-firmware` + PipeWire SPA A2DP/HFP (`0 MB` idle when off) |

### 2.4 Audio, Cameras, Input, Storage & Peripherals
| Category | Supported Hardware & Standards | Built-In Stack |
| :--- | :--- | :--- |
| **Audio** | Intel/AMD HDA, Modern SoundWire / **Intel SOF**, USB DACs, HDMI Audio, BT Headsets | `PipeWire` + `WirePlumber` + `pipewire-alsa` + `sof-firmware` + `alsa-ucm-conf` + HDA anti-pop tuning + **ResonanceFX 10-Band DSP** |
| **Laptop Input & Docks** | I2C HID touchpads/keyboards, PS/2, USB-C / **USB4 / Thunderbolt 4** | Stage-1 `initramfs` includes `i2c/busses` + `usb/typec` (works at LUKS2 unlock!) + `bolt` (`boltctl`) |
| **Storage & Filesystems** | NVMe, SATA AHCI, Legacy PATA/IDE, SD/MMC, USB 1.1–3.x | `ext4`, `btrfs`, `xfs`, `NTFS-3G` (read/write), `exFAT`, `FAT32`, `smartmontools`, `nvme-cli`, `hdparm` |
| **Printers** | Driverless IPP Everywhere / AirPrint Wi-Fi + Legacy USB (HP, Canon, Epson, Brother) | On-demand `CUPS` + `cups-filters` + `Gutenprint` (`0 MB` idle when off) |
| **Scanners** | Network eSCL/AirScan, HP, Canon PIXMA, Epson, SANE Network | `SANE` (`escl`, `hp`, `pixma`, `epson2`, `net`) + `simple-scan` + live detection in Control Panel & Device Manager |
| **Webcams & Sensors** | UVC USB / Laptop MIPI webcams, ambient/orientation sensors | `v4l-utils` (`v4l2-ctl`), `libcamera`, `iio-sensor-proxy`, `lm-sensors`, `upower` |
| **Game Controllers** | Xbox, PlayStation DualShock/DualSense, Switch Pro, 8BitDo (USB & BT) | Kernel HID drivers + `evtest` + Pure-Rust `uinput` Gamepad → Keyboard/Mouse Mapper |
