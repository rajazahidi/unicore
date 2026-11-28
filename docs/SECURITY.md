# UniCore OS — Security Architecture & Threat Model (`v2.0.0`)

**Document ID:** `UC-SEC-2.0`  
**Classification:** Public Security Reference  
**Verification Tool:** `/usr/libexec/unicore-security-audit` · `uniload security-audit`

---

## 1. Security Philosophy & Design Goals

UniCore OS adopts a **defense-in-depth, zero-trust desktop security posture** engineered to protect everyday users and enterprise endpoints without sacrificing usability or low-RAM performance:

1. **Immutable OS Integrity:** The root filesystem is read-only (`SquashFS`) and updated exclusively through cryptographically signed A/B images.
2. **Least-Privilege Application Confinement:** Untrusted desktop, legacy PC, web, and mobile applications run in isolated `bubblewrap` sandboxes with explicit capability dropping and deny-by-default hardware access.
3. **Minimal Trusted Computing Base (TCB):** Root privilege escalation from the standard user (`unicore`) is prohibited globally; only an explicit, audited allowlist of non-interactive actions in `/usr/libexec/unicore-sys-helper` is permitted via `doas`.
4. **Quantum-Resilient Network Privacy & Encryption at Rest:** Native LUKS2 disk encryption, PBKDF2-hardened AES-256 backups, and hybrid X25519 + NIST FIPS 203 ML-KEM-768 VPN transport.

---

## 2. Threat Model & Trust Boundaries

| Threat Scenario | Mitigation Control in UniCore OS | Verification Mechanism |
| :--- | :--- | :--- |
| **T1: Malicious `.exe` / `ELF` / Web App attempts to read personal files, Wi-Fi passwords, or `doas` config** | `bwrap` sandbox mounts `/usr`/`/etc` read-only, masks `/etc/doas.conf`, `/etc/doas.d`, `/etc/shadow`, and `/etc/NetworkManager/system-connections` over `/dev/null`/`tmpfs`, isolates `/data` and `/tmp`, and denies `/home/Documents` unless explicitly enabled. | `uniload bwrap-args <id>` · `unicore-security-audit` |
| **T2: Sandbox escape via kernel capabilities or PID/IPC/UTS spoofing** | `--cap-drop ALL`, `--new-session`, `--die-with-parent`, `--unshare-uts`, `--unshare-ipc`, plus `--unshare-pid` for native ELF apps. | Unit test `v100_bwrap_hardening_and_security_audit_cli_work` |
| **T3: Tampered OTA update or MITM App Store catalog** | Offline **Ed25519** signing of `unicore-manifest-*.json` and `unicore-catalog.json` (`REQUIRE_SIGNATURE=1` verified against embedded `/etc/unicore/update.pub`) + strict streaming SHA-256 verification of every binary/ISO artifact. | Gate 1 Ed25519 tamper-rejection test; CI release verification |
| **T4: Evil-Maid / Bootkit modification of kernel or bootloader** | UEFI Secure Boot chain using Microsoft-signed upstream `shim` (`BOOTX64.EFI`/`BOOTAA64.EFI`) + UniCore MOK-signed GRUB (`grubx64.efi`/`grubaa64.efi`) & kernel. | Gate 2 UEFI Shim/MOK QA · `unicore-secureboot status` |
| **T5: Physical theft of laptop or removable disk** | Optional **LUKS2** (`PBKDF2`) full encryption of `UNICORE_DATA_LUKS` unlocked in Stage-1 `initramfs` before desktop start; encrypted backup archives use AES-256-CBC + PBKDF2 (`100,000` iterations) + SHA-256 digest. | Automated 3-Stage A/B + LUKS2 QEMU QA (`test-ab-install-luks.py`) |
| **T6: Privilege escalation via helper scripts or symlink attacks** | Strict username regex validation, `umask 077` + `mktemp` for all privileged temporary files, non-writable `0755`/`root:root` `/usr/libexec/unicore-*` helpers, locked `0600` `/etc/doas.d/unicore.conf`, and `0600` secret file enforcement. | `/usr/libexec/unicore-security-audit check` |
| **T7: Unsolicited LAN/Public Wi-Fi network intrusion** | Stateful `nftables` firewall (`unicore_fw`) drops all unsolicited inbound connections by default (`Public Wi-Fi` blocks all inbound; `Home/LAN` permits only mDNS `5353/udp` and optional Phone Link `1714–1764`). | `unicore-firewall status` |
| **T8: Network DPI censorship or "Harvest Now, Decrypt Later" quantum eavesdropping** | Native **AegisTunnel VPN** (`unicore-aegis-vpn`) combines WireGuard AEAD, AmneziaWG anti-DPI obfuscation, REALITY TLS 1.3 TCP/443 fallback, and Hybrid `X25519 + ML-KEM-768` rotating PSKs. | `unicore-aegis-vpn self-test` · `aegis-proto` unit tests |

---

## 3. Cryptographic Standards Inventory

| Subsystem | Primitive / Standard | Key / Parameter Storage | Notes |
| :--- | :--- | :--- | :--- |
| **OTA & Catalog Signing** | Ed25519 (RFC 8032, raw 64-byte `.sig`) | Offline private key (GitHub Secret); public PEM at `/etc/unicore/update.pub` | Mandatory when `REQUIRE_SIGNATURE=1` |
| **Artifact Digests** | SHA-256 (NIST FIPS 180-4) | Embedded in signed manifest & `.iso.sha256` | Verified in streaming mode before flashing |
| **Disk Encryption (`/data`)** | LUKS2 (`aes-xts-plain64`, PBKDF2) | User passphrase (`>= 8` chars), never written to disk | Unlocked in Stage-1 `initramfs-init` |
| **Backup Encryption** | OpenSSL AES-256-CBC + PBKDF2 (`100,000` iter) + SHA-256 | User passphrase via secure stdin/env | Protected against Tar-Slip path traversal on restore |
| **AegisTunnel VPN** | Noise_IKpsk2 (ChaCha20-Poly1305, BLAKE2s, X25519) + **ML-KEM-768** (FIPS 203) | `/data/network/aegis-vpn.json` (`chmod 0600`, `root:root`) | Live PQ epoch rotation via `SIGHUP` |
| **UEFI Secure Boot** | RSA-2048/SHA-256 MOK + Microsoft UEFI CA Shim | `ENROLL_UNICORE_MOK.cer` bundled on ISO & ESP | Zero third-party kernel modules required |

---

## 4. Privilege Separation (`doas` Allowlist)

The desktop user (`unicore`) has **no general `sudo`/`doas` root shell**. `/etc/doas.d/unicore.conf` (`0600 root:root`) restricts privilege elevation solely to validated subcommands of `/usr/libexec/unicore-sys-helper`, including:

- **Power & Session:** `poweroff`, `reboot`, `suspend`
- **System Configuration:** `set-timezone`, `set-hostname`, `set-passwd`, `user-add`, `user-del`
- **Hardware & Services (On-Demand):** `bt-start`, `bt-stop`, `cups-start`, `cups-stop`, `printer-discover`, `printer-add`, `printer-remove`, `hw-apply`, `storage-unmount`
- **Security & Networking:** `firewall-apply`, `vpn-apply`, `aegis-save-config`, `aegis-start`, `aegis-stop`, `aegis-rotate-pq`, `parental-apply`
- **OS Lifecycle & Subsystems:** `install-disk`, `ota-apply`, `ota-rollback`, `mok-enroll`, `android-init`, `android-start`, `android-stop`, `android-restart`, `android-gsf-id`, `android-install-aurora`

Every subcommand strictly validates arguments (rejecting shell metacharacters, path traversal `..`, and invalid identifiers) and executes without invoking an untrusted subshell.

---

## 5. Sandbox Permission Model (`permissions.json`)

Every registered application under `/data/sandboxes/<app_id>/` carries an explicit JSON policy enforced at launch by `/usr/libexec/uniload-sandbox-exec`:

| Permission Key | Default | Runtime Enforcement When `false` |
| :--- | :--- | :--- |
| `gpu_acceleration` | `true` | Omits `/dev/dri` bind mount; forces `LIBGL_ALWAYS_SOFTWARE=1`, `GALLIUM_DRIVER=llvmpipe` |
| `network` | `true` | Isolates network namespace via `bwrap --unshare-net` |
| `shared_documents` | `false` | Omits `/data/home/Documents` bind mount (app sees only its private sandbox) |
| `camera` | `false` | Omits `/dev/video*` and `/dev/media*` device nodes |
| `microphone` | `false` | Omits `/dev/snd` capture access and sets `PULSE_SOURCE=/dev/null` |

---

## 6. Continuous Security Auditing & Credential Hygiene

Operators and end users can execute a live zero-trust posture audit at any time via **Start Menu → User Guide → Run Security Audit**, `uniload security-audit`, or `/usr/libexec/unicore-security-audit check`:

- Confirms `/etc/doas.d/unicore.conf` is locked (`0600`, `root:root`) with no wildcard `permit nopass unicore` rule.
- Confirms `REQUIRE_SIGNATURE=1` and `/etc/unicore/update.pub` integrity.
- Confirms all `/usr/libexec/unicore-*` helpers are owned by `root:root` and non-world-writable.
- Confirms all sensitive credential files (`/data/ai/provider.conf`, `/data/network/aegis-vpn.json`, `/data/home/.config/rclone/rclone.conf`) enforce `0600` permissions.
- Confirms `bwrap` and `nftables` firewall availability and `idle_heavy_count=0` (`0 MB`-idle invariant).

### Reporting Security Vulnerabilities
Security researchers discovering a vulnerability in UniCore OS, `uniload`, `unicore-center`, `AegisTunnel`, or `ResonanceFX` are requested to open a private security advisory on the official repository (`https://github.com/rajazahidi/unicore/security/advisories/new`) with reproduction steps and affected version.
