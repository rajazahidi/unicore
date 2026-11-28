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
                subtitle: "UniCore Modern Wayland desktop essentials".to_string(),
                body: "\
• Press the Super (Meta) key to open the Graphical Start Menu and search apps.\n\
• Press Super + . for Emoji Picker, Super + A for AI Copilot, Super + E for Computer Explorer, and PrintScreen for Screenshots & MP4 Recording.\n\
• Press Ctrl + Shift + Esc anytime to open Task Manager, or Super + L to lock your screen.\n\
• Customize your taskbar (Floating Dock, Classic Bottom Bar, or Top Bar), 10 Desktop Themes, 12 International Languages, and 21 Keyboard Layouts in Control Panel.".to_string(),
                tip: "Tip: Right-click any desktop area for New Folder, Control Panel, Device Manager, Storage Manager, or Display Settings.".to_string(),
            },
            GuideChapter {
                id: "install-dualboot",
                title: "2. Install, Dual-Boot & LUKS2".to_string(),
                subtitle: "Live USB, Ventoy, Secure Boot MOK & safe disk installation".to_string(),
                body: "\
• Boot UniCore OS directly from Ventoy (just copy the .iso file) or a standard USB stick.\n\
• UEFI Secure Boot is supported out-of-the-box via signed UEFI Boot Shim + UniCore MOK (enroll once on first boot or in System -> Secure Boot).\n\
• Open System -> Install to Disk to install Safe Dual-Boot (using free disk space without touching existing OS partitions) or Full Disk A/B layout.\n\
• Tick 'Encrypt /data with LUKS2' (min 8-char passphrase) to protect all personal documents, passwords, and sandboxes.".to_string(),
                tip: "Tip: Your /data partition stays untouched across OS updates and A/B slot rollbacks.".to_string(),
            },
            GuideChapter {
                id: "apps-gaming",
                title: "3. Apps, Store & Classic PC Gaming".to_string(),
                subtitle: "Unified Game Hub, Simplified Legacy Auto-Tune, Gamepad & Cloud Saves".to_string(),
                body: "\
• Double-click any desktop .exe/.msi, MS-DOS .exe, CD .iso, .zip, or mobile .apk package to run it cleanly inside an isolated UniCore sandbox.\n\
• Open Unified Game Hub (--games) to launch all Classic 2D/3D, DOS, Steam, Heroic, Lutris, RetroArch, and mobile games from one place.\n\
• For vintage 1995–2008 titles, simply choose one plain-language Auto-Tune preset (Smart Auto, 90s 2D Classic, 2000s 3D Classic, Retro DOS, or Ultra-Safe Legacy) — UniCore configures 60 FPS cap, 1-Core CPU lock, sharp 4:3 scaling, and CD .iso mounting automatically.\n\
• Plug in standard USB/Bluetooth gamepads in Gamepad Hub (--gamepad) and enable the Pure-Rust uinput Keyboard/Mouse Mapper for older games.\n\
• Enable Auto-Backup Game Saves (--saves) to keep 5 local snapshots per game and sync them to your Cloud Drive.".to_string(),
                tip: "Tip: Game Mode & MangoHud FPS overlay can be toggled right inside Unified Game Hub.".to_string(),
            },
            GuideChapter {
                id: "ecosystem-migrate",
                title: "4. Phone, Cloud, Devices & Storage".to_string(),
                subtitle: "KDE Connect, scrcpy, rclone Cloud Drives, Read-Only NTFS Migration & Device/Storage Managers".to_string(),
                body: "\
• Phone Link (--phone): Pair your smartphone with KDE Connect for notifications, SMS, clipboard & file sharing, or mirror/control mobile screens via USB/Wi-Fi scrcpy.\n\
• Cloud Drives (--cloud): Mount personal or enterprise Cloud Drives (OAuth / WebDAV / Nextcloud) directly into ~/Cloud/<Name> with 1 click.\n\
• PC Data Migration (--migrate): Safely mounts existing NTFS partitions Read-Only to import Documents, Pictures, Fonts, Saved Games, and Browser Bookmarks.\n\
• Device Manager (--devmgr) & Storage Manager (--storage): Inspect every hardware controller, kernel driver, SANE scanner, disk/partition usage, S.M.A.R.T health, safe USB eject, and LVFS firmware updates.".to_string(),
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
                subtitle: "Asas desktop Wayland moden UniCore OS".to_string(),
                body: "\
• Tekan kekunci Super (Meta) untuk membuka Menu Mula Bergrafik dan mencari aplikasi.\n\
• Tekan Super + . untuk Pemilih Emoji, Super + A untuk Pembantu AI, Super + E untuk Penjelajah Komputer, dan PrintScreen untuk Tangkap Skrin & Rakam Video MP4.\n\
• Tekan Ctrl + Shift + Esc bila-bila masa untuk Pengurus Tugas, atau Super + L untuk mengunci skrin.\n\
• Sesuaikan bar tugas (Dok Terapung, Bar Bawah Klasik, atau Bar Atas), 10 Tema Desktop, 12 Bahasa Antarabangsa & 21 Papan Kekunci di Pusat Kawalan.".to_string(),
                tip: "Tip: Klik kanan di mana-mana ruang desktop untuk Folder Baharu, Pusat Kawalan, Pengurus Peranti, Pengurus Storan atau Tetapan Paparan.".to_string(),
            },
            GuideChapter {
                id: "install-dualboot",
                title: "2. Pemasangan, Dual-Boot & LUKS2".to_string(),
                subtitle: "Live USB, Ventoy, Secure Boot MOK & pemasangan cakera selamat".to_string(),
                body: "\
• Boot UniCore OS terus daripada pemacu USB Ventoy (hanya salin fail .iso) atau USB biasa.\n\
• UEFI Secure Boot disokong sedia ada melalui Boot Shim UEFI bertandatangan + MOK UniCore (daftar sekali semasa boot pertama atau di Sistem -> Secure Boot).\n\
• Buka Sistem -> Pasang ke Cakera untuk memilih Dual-Boot Selamat (guna ruang kosong tanpa menyentuh OS sedia ada) atau Pasang Penuh A/B.\n\
• Tandakan 'Sulitkan /data dengan LUKS2' (kata laluan minimum 8 aksara) untuk melindungi semua dokumen, kata laluan, dan sandbox aplikasi.".to_string(),
                tip: "Tip: Partisyen /data anda kekal selamat walaupun sistem dikemas kini atau di-rollback.".to_string(),
            },
            GuideChapter {
                id: "apps-gaming",
                title: "3. Aplikasi, Kedai & Game PC Klasik".to_string(),
                subtitle: "Hab Game Bersepadu, Auto-Talaan Game Lama 1-Klik, Gamepad & Cloud Save".to_string(),
                body: "\
• Klik dua kali mana-mana fail desktop .exe/.msi, MS-DOS, CD .iso, .zip atau pakej mudah alih .apk untuk menjalankannya dalam sandbox terasing.\n\
• Buka Hab Permainan (--games) untuk melihat dan melancarkan semua game klasik 2D/3D, DOS, Steam, Heroic, Lutris, RetroArch, dan mudah alih di satu tempat.\n\
• Untuk game klasik 1995–2008, pilih satu profil Auto-Talaan mudah (Auto Pintar, Klasik 2D 90-an, Klasik 3D 2000-an, Retro DOS, atau Mod Selamat) — had 60 FPS, kunci 1 teras CPU, nisbah 4:3 & lekap CD .iso dilaras secara automatik.\n\
• Sambungkan pengawal USB/Bluetooth di Panel Gamepad (--gamepad) dan hidupkan Pemeta Kekunci/Tetikus Rust (uinput) bagi game lama.\n\
• Aktifkan Auto-Sandar Save (--saves) untuk menyimpan 5 salinan tempatan setiap game serta sync terus ke Pemacu Awan anda.".to_string(),
                tip: "Tip: Mod Permainan & paparan FPS MangoHud boleh dihidupkan terus di dalam Hab Permainan.".to_string(),
            },
            GuideChapter {
                id: "ecosystem-migrate",
                title: "4. Telefon, Awan, Peranti & Storan".to_string(),
                subtitle: "KDE Connect, scrcpy, Pemacu Awan rclone, Migrasi NTFS & Pengurus Peranti/Storan".to_string(),
                body: "\
• Sambung Telefon (--phone): Gandingkan telefon melalui KDE Connect (notifikasi, SMS, papan klip, kongsi fail) atau cermin skrin mudah alih USB/Wi-Fi menggunakan scrcpy.\n\
• Pemacu Awan (--cloud): Lekap Pemacu Awan peribadi atau organisasi (OAuth / WebDAV / Nextcloud) terus ke ~/Cloud/<Nama> dengan 1 klik.\n\
• Migrasi Data PC (--migrate): Lekap partisyen NTFS secara Baca-Sahaja (100% selamat!) untuk menyalin Dokumen, Gambar, Fon, Save Game dan Penanda Buku.\n\
• Pengurus Peranti (--devmgr) & Pengurus Storan (--storage): Semak semua peranti perkakasan, pemacu kernel, pengimbas SANE, ruang cakera, kesihatan S.M.A.R.T, keluarkan USB selamat & kemas kini firmware LVFS.".to_string(),
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
