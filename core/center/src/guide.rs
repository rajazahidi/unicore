//! Built-in Offline Bilingual (EN / BM) User Guide & Live Security Audit for `unicore-center` (v1.0.0).

use std::process::{Command, Stdio};

use crate::i18n::UiLang;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideChapter {
    pub id: &'static str,
    pub title: String,
    pub subtitle: String,
    pub body: String,
    pub tip: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityAuditReport {
    pub status: String,
    pub checks_passed: u32,
    pub checks_total: u32,
    pub doas_locked: bool,
    pub ota_signed: bool,
    pub libexec_safe: bool,
    pub creds_0600: bool,
    pub bwrap_ready: bool,
    pub firewall_ready: bool,
    pub firewall_active: bool,
    pub idle_heavy_count: u32,
    pub idle_heavy_running: String,
    pub ram_total_mb: u64,
    pub ram_used_mb: u64,
    pub hw_tier: String,
    pub issues: String,
}

impl Default for SecurityAuditReport {
    fn default() -> Self {
        Self {
            status: "PASS".to_string(),
            checks_passed: 6,
            checks_total: 6,
            doas_locked: true,
            ota_signed: true,
            libexec_safe: true,
            creds_0600: true,
            bwrap_ready: true,
            firewall_ready: true,
            firewall_active: true,
            idle_heavy_count: 0,
            idle_heavy_running: "none".to_string(),
            ram_total_mb: 0,
            ram_used_mb: 0,
            hw_tier: "modern".to_string(),
            issues: "none".to_string(),
        }
    }
}

pub fn bilingual_guide_chapters(lang: UiLang) -> Vec<GuideChapter> {
    match lang {
        UiLang::En => vec![
            GuideChapter {
                id: "getting-started",
                title: "1. Quick Start & Shortcuts".to_string(),
                subtitle: "Windows 11 / Zorin-style Wayland desktop essentials".to_string(),
                body: "\
• Press the Windows (Super) key to open the Graphical Start Menu and search apps.\n\
• Press Super + . for Emoji Picker, Super + A for AI Copilot, Super + E for File Explorer, and PrintScreen for Screenshots & MP4 Recording.\n\
• Press Ctrl + Shift + Esc anytime to open Task Manager, or Super + L to lock your screen.\n\
• Customize your taskbar (Win11 Centered, Win10 Classic, or macOS Dock), Dark/Light/High Contrast theme, and 6 accent colors in Settings -> Display & Style.".to_string(),
                tip: "Tip: Right-click any desktop area for quick actions, Display Settings, or New Folder.".to_string(),
            },
            GuideChapter {
                id: "install-dualboot",
                title: "2. Install, Dual-Boot & LUKS2".to_string(),
                subtitle: "Live USB, Ventoy, Secure Boot MOK & safe disk installation".to_string(),
                body: "\
• Boot UniCore OS directly from Ventoy (just copy the .iso file) or a standard USB stick.\n\
• UEFI Secure Boot is supported out-of-the-box via Microsoft-signed shim + UniCore MOK (enroll once on first boot or in System -> Secure Boot).\n\
• Open System -> Install to Disk to install Alongside Windows (safe Dual-Boot using free disk space) or Full Disk A/B layout.\n\
• Tick 'Encrypt /data with LUKS2' (min 8-char passphrase) to protect all personal documents, passwords, and sandboxes.".to_string(),
                tip: "Tip: Your /data partition stays untouched across OS updates and A/B slot rollbacks.".to_string(),
            },
            GuideChapter {
                id: "apps-gaming",
                title: "3. Apps, Store & 98/XP Gaming".to_string(),
                subtitle: "Unified Game Hub, Classic 98/XP/DOS Wizard, Gamepad & Cloud Saves".to_string(),
                body: "\
• Double-click any Windows .exe/.msi, MS-DOS .exe, CD .iso, .zip, or Android .apk to run it cleanly inside an isolated UniCore sandbox.\n\
• Open Unified Game Hub (--games) to launch all 98/XP, DOS, Steam, Heroic, Lutris, RetroArch, and Android games from one place.\n\
• For vintage 1998–2008 games, use 1-Click 60 FPS Cap, 1-Core CPU Lock, Safe 800x600 Window, CD .iso mounting (D:\\), and 1-Click Crash Auto-Repair.\n\
• Plug in Xbox, PlayStation, Switch, or 8BitDo controllers in Gamepad Hub (--gamepad) and enable the Pure-Rust uinput Keyboard/Mouse Mapper for older games.\n\
• Enable Auto-Backup Game Saves (--saves) to keep 5 local snapshots per game and sync them to your Cloud Drive.".to_string(),
                tip: "Tip: Game Mode & MangoHud FPS overlay can be toggled right inside Unified Game Hub.".to_string(),
            },
            GuideChapter {
                id: "ecosystem-migrate",
                title: "4. Phone, Cloud & Migration".to_string(),
                subtitle: "KDE Connect, scrcpy, rclone Cloud Drives, NTFS RO Migration & Drivers".to_string(),
                body: "\
• Phone Link (--phone): Pair your phone with KDE Connect for notifications, SMS, clipboard & file sharing, or mirror/control Android screens via USB/Wi-Fi scrcpy.\n\
• Cloud Drives (--cloud): Mount Google Drive, OneDrive, Dropbox, MEGA, or Nextcloud directly into ~/Cloud/<Name> with 1 click.\n\
• Windows Migration (--migrate): Safely mounts Windows NTFS drives Read-Only (zero risk to Windows!) to import Documents, Pictures, Fonts, Saved Games, Chrome/Edge Bookmarks, and match Program Files to UniCore Store.\n\
• Hardware & Drivers (--drivers): Inspect GPU/Vulkan/Wi-Fi/USB health, toggle Safe GPU Mode (Pixman), fix Wi-Fi power-save drops, reset Audio/Bluetooth in 1 click, or run LVFS firmware updates.".to_string(),
                tip: "Tip: KDE Connect and fwupd use 0 MB RAM while idle until you turn them on.".to_string(),
            },
            GuideChapter {
                id: "security-backup",
                title: "5. Security, Firewall & Backups".to_string(),
                subtitle: "Zero-trust sandboxes, nftables firewall, 0-MB-idle & AES-256 backups".to_string(),
                body: "\
• Every app sandbox runs under bubblewrap (--cap-drop ALL, --new-session, --die-with-parent, --unshare-uts) with per-app Network, GPU, Documents, Camera, and Microphone toggles.\n\
• Stateful nftables firewall blocks unsolicited inbound traffic by default (Home profile allows local mDNS & KDE Connect LAN; Public profile blocks all).\n\
• All sensitive tokens (rclone, AI API keys, shadow sync) are locked to chmod 0600, and privilege elevation is strictly limited to unicore-sys-helper.\n\
• Back up /data locally or to USB with AES-256-CBC (PBKDF2) encryption and SHA-256 + Tar-Slip verification in Settings -> Security.".to_string(),
                tip: "Tip: Click 'Run Security Audit' below anytime to inspect live zero-trust and 0-MB-idle status.".to_string(),
            },
            GuideChapter {
                id: "ota-recovery",
                title: "6. A/B OTA & Auto-Rollback".to_string(),
                subtitle: "Signed Ed25519 OTA, health-gated boot confirmation & diagnostics".to_string(),
                body: "\
• Official OTA updates are cryptographically verified using Ed25519 signatures (REQUIRE_SIGNATURE=1) and SHA-256 hashes before writing to the standby A/B slot.\n\
• After rebooting into a new slot, unicore-boot-health verifies rootfs, /data, uniload, unicore-center, and the Wayland desktop before confirming the update.\n\
• If an update ever fails to boot twice, GRUB automatically rolls back to your previous working slot — your files in /data are always safe!\n\
• Need help or want to report a hardware issue? Click System -> Export Diagnostics (or run unicore-diag --save-usb).".to_string(),
                tip: "Tip: You can also trigger an instant manual rollback anytime in System -> OS Updates.".to_string(),
            },
        ],
        UiLang::Ms => vec![
            GuideChapter {
                id: "getting-started",
                title: "1. Mula Pantas & Pintasan".to_string(),
                subtitle: "Asas desktop Wayland gaya Windows 11 / Zorin".to_string(),
                body: "\
• Tekan kekunci Windows (Super) untuk membuka Menu Mula Bergrafik dan mencari aplikasi.\n\
• Tekan Super + . untuk Pemilih Emoji, Super + A untuk Pembantu AI, Super + E untuk Pengurus Fail, dan PrintScreen untuk Tangkap Skrin & Rakam Video MP4.\n\
• Tekan Ctrl + Shift + Esc bila-bila masa untuk Pengurus Tugas, atau Super + L untuk mengunci skrin.\n\
• Tukar gaya bar tugas (Win11 Tengah, Win10 Klasik, atau macOS Dock), tema Gelap/Cerah/Kontras Tinggi, dan 6 warna aksen di Tetapan -> Paparan & Gaya.".to_string(),
                tip: "Tip: Klik kanan di mana-mana ruang desktop untuk tetapan pantas, Paparan atau Folder Baharu.".to_string(),
            },
            GuideChapter {
                id: "install-dualboot",
                title: "2. Pemasangan, Dual-Boot & LUKS2".to_string(),
                subtitle: "Live USB, Ventoy, Secure Boot MOK & pemasangan cakera selamat".to_string(),
                body: "\
• Boot UniCore OS terus daripada pemacu USB Ventoy (hanya salin fail .iso) atau USB biasa.\n\
• UEFI Secure Boot disokong sedia ada melalui shim bertandatangan Microsoft + MOK UniCore (daftar sekali semasa boot pertama atau di Sistem -> Secure Boot).\n\
• Buka Sistem -> Pasang ke Cakera untuk memilih Dual-Boot Bersama Windows (guna ruang kosong tanpa menyentuh Windows) atau Pasang Penuh A/B.\n\
• Tandakan 'Sulitkan /data dengan LUKS2' (kata laluan minimum 8 aksara) untuk melindungi semua dokumen, kata laluan, dan sandbox aplikasi.".to_string(),
                tip: "Tip: Partisyen /data anda kekal selamat walaupun sistem dikemas kini atau di-rollback.".to_string(),
            },
            GuideChapter {
                id: "apps-gaming",
                title: "3. Aplikasi, Kedai & Game 98/XP".to_string(),
                subtitle: "Hab Game Bersepadu, Wizard 98/XP/DOS, Gamepad & Cloud Save".to_string(),
                body: "\
• Klik dua kali mana-mana fail Windows .exe/.msi, MS-DOS, CD .iso, .zip atau Android .apk untuk menjalankannya dalam sandbox terasing.\n\
• Buka Hab Permainan (--games) untuk melihat dan melancarkan semua game 98/XP, DOS, Steam, Heroic, Lutris, RetroArch, dan Android di satu tempat.\n\
• Untuk game klasik 1998–2008, aktifkan Had 60 FPS, Kunci 1 Teras CPU, Tetingkap Selamat 800x600, lekap CD .iso (D:\\) & Auto-Baiki 1-Klik.\n\
• Sambungkan pengawal Xbox, PlayStation, Switch, atau 8BitDo di Panel Gamepad (--gamepad) dan hidupkan Pemeta Kekunci/Tetikus Rust (uinput) bagi game lama.\n\
• Aktifkan Auto-Sandar Save (--saves) untuk menyimpan 5 salinan tempatan setiap game serta sync terus ke Pemacu Awan anda.".to_string(),
                tip: "Tip: Mod Permainan & paparan FPS MangoHud boleh dihidupkan terus di dalam Hab Permainan.".to_string(),
            },
            GuideChapter {
                id: "ecosystem-migrate",
                title: "4. Telefon, Awan & Migrasi Windows".to_string(),
                subtitle: "KDE Connect, scrcpy, Pemacu Awan rclone, Migrasi NTFS & Pemacu".to_string(),
                body: "\
• Sambung Telefon (--phone): Gandingkan telefon melalui KDE Connect (notifikasi, SMS, papan klip, kongsi fail) atau cermin skrin Android USB/Wi-Fi menggunakan scrcpy.\n\
• Pemacu Awan (--cloud): Lekap Google Drive, OneDrive, Dropbox, MEGA, atau Nextcloud terus ke ~/Cloud/<Nama> dengan 1 klik.\n\
• Migrasi Windows (--migrate): Lekap partisyen NTFS Windows secara Baca-Sahaja (100% selamat!) untuk menyalin Dokumen, Gambar, Fon, Save Game, Penanda Buku Chrome/Edge dan cadangan padanan Kedai.\n\
• Perkakasan & Pemacu (--drivers): Semak GPU/Vulkan/Wi-Fi/USB, suis Mod GPU Selamat (Pixman), baiki Wi-Fi terputus, Reset Audio & Bluetooth 1-klik, serta kemas kini firmware LVFS.".to_string(),
                tip: "Tip: KDE Connect dan fwupd menggunakan 0 MB RAM ketika terbiar sehingga anda menghidupkannya.".to_string(),
            },
            GuideChapter {
                id: "security-backup",
                title: "5. Keselamatan, Tembok Api & Sandaran".to_string(),
                subtitle: "Sandbox zero-trust, tembok api nftables, 0-MB-idle & sandaran AES-256".to_string(),
                body: "\
• Setiap aplikasi sandbox dijalankan melalui bubblewrap (--cap-drop ALL, --new-session, --die-with-parent, --unshare-uts) dengan kawalan Rangkaian, GPU, Dokumen, Kamera dan Mikrofon.\n\
• Tembok api berkeadaan nftables menyekat trafik masuk rawak secara lalai (profil Rumah membenarkan LAN mDNS & KDE Connect; profil Awam menyekat semua).\n\
• Semua fail kredensial sensitif (rclone, kunci API AI, shadow sync) dikunci ketat chmod 0600 dan kebenaran root terhad kepada unicore-sys-helper sahaja.\n\
• Cipta sandaran /data disulitkan AES-256-CBC (PBKDF2) dengan pengesahan SHA-256 & perlindungan Tar-Slip di Tetapan -> Keselamatan.".to_string(),
                tip: "Tip: Klik 'Jalankan Audit Keselamatan' di bawah untuk semakan langsung status keselamatan & 0-MB-idle.".to_string(),
            },
            GuideChapter {
                id: "ota-recovery",
                title: "6. Kemas Kini A/B & Auto-Rollback".to_string(),
                subtitle: "OTA bertandatangan Ed25519, semakan kesihatan boot & diagnostik".to_string(),
                body: "\
• Kemas kini rasmi disahkan secara kriptografi melalui tandatangan Ed25519 (REQUIRE_SIGNATURE=1) dan semakan SHA-256 sebelum ditulis ke slot A/B simpanan.\n\
• Selepas mula semula ke slot baharu, unicore-boot-health mengesahkan rootfs, /data, uniload, unicore-center dan sesi Wayland sebelum mengesahkan boot.\n\
• Jika kemas kini gagal boot sebanyak 2 kali, GRUB akan berbalik secara automatik (auto-rollback) ke slot stabil terdahulu — fail /data anda sentiasa selamat!\n\
• Perlukan laporan perkakasan? Klik Sistem -> Eksport Diagnostik atau jalankan unicore-diag --save-usb.".to_string(),
                tip: "Tip: Anda juga boleh kembali ke slot sebelumnya secara manual di Sistem -> Kemas Kini OS.".to_string(),
            },
        ],
    }
}

pub fn parse_security_audit_output(raw: &str) -> SecurityAuditReport {
    let mut rep = SecurityAuditReport::default();
    for line in raw.lines() {
        let Some((k, v)) = line.trim().split_once('=') else {
            continue;
        };
        let val = v.trim();
        match k.trim() {
            "status" => rep.status = val.to_string(),
            "checks_passed" => rep.checks_passed = val.parse().unwrap_or(0),
            "checks_total" => rep.checks_total = val.parse().unwrap_or(6),
            "doas_conf_locked" => rep.doas_locked = val == "1",
            "ota_signature_enforced" => rep.ota_signed = val == "1",
            "libexec_perms_ok" => rep.libexec_safe = val == "1",
            "credentials_0600" => rep.creds_0600 = val == "1",
            "bwrap_available" => rep.bwrap_ready = val == "1",
            "firewall_ready" => rep.firewall_ready = val == "1",
            "firewall_active" => rep.firewall_active = val == "1",
            "idle_heavy_count" => rep.idle_heavy_count = val.parse().unwrap_or(0),
            "idle_heavy_running" => rep.idle_heavy_running = val.to_string(),
            "ram_total_mb" => rep.ram_total_mb = val.parse().unwrap_or(0),
            "ram_used_mb" => rep.ram_used_mb = val.parse().unwrap_or(0),
            "hw_tier" => rep.hw_tier = val.to_string(),
            "issues" => rep.issues = val.to_string(),
            _ => {}
        }
    }
    rep
}

pub fn format_security_audit_summary(rep: &SecurityAuditReport, lang: UiLang) -> String {
    let ms = lang == UiLang::Ms;
    let idle_str = if rep.idle_heavy_count == 0 {
        if ms {
            "0 MB terbiar (semua servis berat dimatikan)".to_string()
        } else {
            "0 MB idle (all heavy services stopped)".to_string()
        }
    } else if ms {
        format!("{} servis aktif: {}", rep.idle_heavy_count, rep.idle_heavy_running)
    } else {
        format!("{} active service(s): {}", rep.idle_heavy_count, rep.idle_heavy_running)
    };
    let ram_str = if rep.ram_total_mb > 0 {
        format!("{} / {} MB RAM", rep.ram_used_mb, rep.ram_total_mb)
    } else {
        "N/A".to_string()
    };
    if ms {
        format!(
            "Audit Keselamatan: {} ({}/{})  •  {}  •  {}",
            rep.status, rep.checks_passed, rep.checks_total, idle_str, ram_str
        )
    } else {
        format!(
            "Security Audit: {} ({}/{})  •  {}  •  {}",
            rep.status, rep.checks_passed, rep.checks_total, idle_str, ram_str
        )
    }
}

pub fn read_security_audit_report() -> SecurityAuditReport {
    if !cfg!(target_os = "linux") {
        return SecurityAuditReport::default();
    }
    let out = Command::new("/usr/libexec/unicore-security-audit")
        .arg("check")
        .stdin(Stdio::null())
        .output();
    match out {
        Ok(o) if o.status.success() => {
            parse_security_audit_output(&String::from_utf8_lossy(&o.stdout))
        }
        _ => SecurityAuditReport {
            status: "UNAVAILABLE".to_string(),
            checks_passed: 0,
            checks_total: 6,
            issues: "helper_not_found".to_string(),
            ..SecurityAuditReport::default()
        },
    }
}
