//! Windows 11 / Zorin OS familiarity modules for `unicore-center`:
//! - Native Graphical Start Menu (`--start-menu`) app discovery, search & launch
//! - Windows 11 Emoji Picker (`--emoji` / `Win+.`)
//! - Friendly non-technical hardware header summary
//! - OOBE 1-click Starter App Bundles

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use uniload::sandbox::AppSandbox;

use crate::i18n::UiLang;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopAppItem {
    pub id: String,
    pub name: String,
    pub comment: String,
    pub category: String,
    pub exec_cmd: String,
    pub badge: String,
    pub accent_hex: String,
    pub pinned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmojiItem {
    pub symbol: &'static str,
    pub name: &'static str,
    pub keywords: &'static str,
    pub category: &'static str,
}

pub const CURATED_EMOJIS: &[EmojiItem] = &[
    // Smileys
    EmojiItem { symbol: "😀", name: "Grinning", keywords: "smile happy gembira senyum", category: "smileys" },
    EmojiItem { symbol: "😂", name: "Joy", keywords: "laugh lol gelak ketawa", category: "smileys" },
    EmojiItem { symbol: "🥹", name: "Touched", keywords: "grateful cute terharu", category: "smileys" },
    EmojiItem { symbol: "😊", name: "Blush", keywords: "smile warm manis", category: "smileys" },
    EmojiItem { symbol: "😍", name: "Heart Eyes", keywords: "love suka cinta", category: "smileys" },
    EmojiItem { symbol: "🥰", name: "Smiling Hearts", keywords: "love sayang", category: "smileys" },
    EmojiItem { symbol: "😎", name: "Cool", keywords: "sunglasses hebat santai", category: "smileys" },
    EmojiItem { symbol: "🤔", name: "Thinking", keywords: "hmm fikir tanya", category: "smileys" },
    EmojiItem { symbol: "😅", name: "Sweat Smile", keywords: "relief lega", category: "smileys" },
    EmojiItem { symbol: "😭", name: "Crying", keywords: "sad nangis sedih", category: "smileys" },
    EmojiItem { symbol: "😮", name: "Surprised", keywords: "wow terkejut", category: "smileys" },
    EmojiItem { symbol: "😴", name: "Sleeping", keywords: "sleep tidur penat", category: "smileys" },
    EmojiItem { symbol: "🥳", name: "Party", keywords: "celebrate tahniah", category: "smileys" },
    EmojiItem { symbol: "🤯", name: "Mind Blown", keywords: "amazing hebat gila", category: "smileys" },
    EmojiItem { symbol: "😇", name: "Innocent", keywords: "angel baik", category: "smileys" },
    EmojiItem { symbol: "😤", name: "Determined", keywords: "focus geram", category: "smileys" },
    // Gestures & People
    EmojiItem { symbol: "👍", name: "Thumbs Up", keywords: "ok yes bagus setuju", category: "gestures" },
    EmojiItem { symbol: "👎", name: "Thumbs Down", keywords: "no tidak", category: "gestures" },
    EmojiItem { symbol: "👏", name: "Clap", keywords: "bravo tepuk tahniah", category: "gestures" },
    EmojiItem { symbol: "🙏", name: "Thank You", keywords: "please terima kasih doa", category: "gestures" },
    EmojiItem { symbol: "🤝", name: "Handshake", keywords: "deal sepakat salam", category: "gestures" },
    EmojiItem { symbol: "👋", name: "Wave", keywords: "hello hai salam bye", category: "gestures" },
    EmojiItem { symbol: "💪", name: "Strong", keywords: "power semangat kuat", category: "gestures" },
    EmojiItem { symbol: "✌️", name: "Peace", keywords: "victory menang", category: "gestures" },
    EmojiItem { symbol: "🫶", name: "Heart Hands", keywords: "love appreciate", category: "gestures" },
    EmojiItem { symbol: "👌", name: "OK Hand", keywords: "perfect mantap", category: "gestures" },
    EmojiItem { symbol: "🫡", name: "Salute", keywords: "respect siap bos", category: "gestures" },
    EmojiItem { symbol: "🙌", name: "Raising Hands", keywords: "hooray syukur", category: "gestures" },
    // Work & Tech
    EmojiItem { symbol: "💻", name: "Laptop", keywords: "pc computer kerja", category: "work" },
    EmojiItem { symbol: "📱", name: "Phone", keywords: "mobile telefon android", category: "work" },
    EmojiItem { symbol: "📁", name: "Folder", keywords: "files dokumen", category: "work" },
    EmojiItem { symbol: "📄", name: "Document", keywords: "doc word report", category: "work" },
    EmojiItem { symbol: "📧", name: "Email", keywords: "mail mesej", category: "work" },
    EmojiItem { symbol: "📷", name: "Camera", keywords: "photo screenshot gambar", category: "work" },
    EmojiItem { symbol: "🎬", name: "Video", keywords: "record movie rakam", category: "work" },
    EmojiItem { symbol: "🎵", name: "Music", keywords: "audio lagu bunyi", category: "work" },
    EmojiItem { symbol: "🎮", name: "Gaming", keywords: "game main steam", category: "work" },
    EmojiItem { symbol: "⚙️", name: "Settings", keywords: "gear tetapan", category: "work" },
    EmojiItem { symbol: "🔒", name: "Lock", keywords: "security kunci selamat", category: "work" },
    EmojiItem { symbol: "🚀", name: "Rocket", keywords: "fast launch laju", category: "work" },
    EmojiItem { symbol: "💡", name: "Idea", keywords: "tip cadangan lampu", category: "work" },
    EmojiItem { symbol: "📅", name: "Calendar", keywords: "date tarikh jadual", category: "work" },
    EmojiItem { symbol: "☕", name: "Coffee", keywords: "kopi teh rehat", category: "work" },
    EmojiItem { symbol: "🧠", name: "AI Brain", keywords: "ai smart pintar", category: "work" },
    // Symbols & Flags
    EmojiItem { symbol: "❤️", name: "Red Heart", keywords: "love cinta hati", category: "symbols" },
    EmojiItem { symbol: "🔥", name: "Fire", keywords: "hot padu mantap", category: "symbols" },
    EmojiItem { symbol: "✨", name: "Sparkles", keywords: "shine baru bersih", category: "symbols" },
    EmojiItem { symbol: "⭐", name: "Star", keywords: "favorite bintang", category: "symbols" },
    EmojiItem { symbol: "💯", name: "100", keywords: "perfect penuh", category: "symbols" },
    EmojiItem { symbol: "🎉", name: "Celebration", keywords: "tada tahniah", category: "symbols" },
    EmojiItem { symbol: "✅", name: "Check Mark", keywords: "done lulus siap", category: "symbols" },
    EmojiItem { symbol: "❌", name: "Cross Mark", keywords: "no gagal tutup", category: "symbols" },
    EmojiItem { symbol: "⚠️", name: "Warning", keywords: "alert amaran", category: "symbols" },
    EmojiItem { symbol: "⚡", name: "Lightning", keywords: "fast kuasa laju", category: "symbols" },
    EmojiItem { symbol: "🌍", name: "Globe", keywords: "world web internet", category: "symbols" },
    EmojiItem { symbol: "🇲🇾", name: "Malaysia", keywords: "my malaysia bendera", category: "symbols" },
    EmojiItem { symbol: "🇸🇬", name: "Singapore", keywords: "sg singapura", category: "symbols" },
    EmojiItem { symbol: "🇮🇩", name: "Indonesia", keywords: "id indonesia", category: "symbols" },
    EmojiItem { symbol: "📌", name: "Pin", keywords: "pinned semat", category: "symbols" },
    EmojiItem { symbol: "🔔", name: "Bell", keywords: "notification notifikasi", category: "symbols" },
];

pub fn filter_emojis(category: &str, query: &str) -> Vec<EmojiItem> {
    let cat = category.trim().to_ascii_lowercase();
    let q = query.trim().to_ascii_lowercase();
    CURATED_EMOJIS
        .iter()
        .copied()
        .filter(|e| {
            let cat_ok = cat.is_empty() || cat == "all" || e.category == cat;
            let q_ok = q.is_empty()
                || e.name.to_ascii_lowercase().contains(&q)
                || e.keywords.to_ascii_lowercase().contains(&q)
                || e.symbol.contains(&q);
            cat_ok && q_ok
        })
        .collect()
}

pub fn insert_emoji(symbol: &str) -> Result<String, String> {
    let clean = symbol.trim();
    if clean.is_empty() {
        return Err("Empty emoji selection".to_string());
    }
    if cfg!(target_os = "linux") {
        // Copy to Wayland clipboard first so Ctrl+V always works everywhere
        let _ = Command::new("wl-copy")
            .arg("--")
            .arg(clean)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        // Best-effort direct typing via wtype after focus returns
        let escaped = clean.replace('\'', "");
        let _ = Command::new("/bin/sh")
            .args([
                "-c",
                &format!("sleep 0.15; command -v wtype >/dev/null 2>&1 && wtype '{escaped}' >/dev/null 2>&1 || true"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
    Ok(format!("Copied & ready: {clean}"))
}

/// Curated 8 core apps pinned at the top of the Windows 11 / Zorin-style Start Menu.
pub fn builtin_pinned_apps(lang: UiLang) -> Vec<DesktopAppItem> {
    let ms = matches!(lang, UiLang::Ms);
    vec![
        DesktopAppItem {
            id: "unicore-browser".to_string(),
            name: if ms { "Pelayar Web" } else { "Web Browser" }.to_string(),
            comment: if ms { "Layari internet & laman web" } else { "Browse the web" }.to_string(),
            category: "internet".to_string(),
            exec_cmd: "/usr/bin/unicore-browser".to_string(),
            badge: "WEB".to_string(),
            accent_hex: "#0284c7".to_string(),
            pinned: true,
        },
        DesktopAppItem {
            id: "unicore-files".to_string(),
            name: if ms { "Penjelajah Komputer" } else { "Computer Explorer" }.to_string(),
            comment: if ms { "Buka folder, USB, cakera & fail" } else { "Browse folders, USB, drives & files" }.to_string(),
            category: "system".to_string(),
            exec_cmd: "pcmanfm /home/unicore".to_string(),
            badge: "DIR".to_string(),
            accent_hex: "#d97706".to_string(),
            pinned: true,
        },
        DesktopAppItem {
            id: "unicore-store".to_string(),
            name: if ms { "Kedai Aplikasi" } else { "App Store" }.to_string(),
            comment: if ms { "Pasang 33+ aplikasi Desktop, Linux & Mudah Alih" } else { "Install 33+ Desktop, Linux & Mobile apps" }.to_string(),
            category: "system".to_string(),
            exec_cmd: "/usr/bin/unicore-center --tab store".to_string(),
            badge: "APP".to_string(),
            accent_hex: "#2563eb".to_string(),
            pinned: true,
        },
        DesktopAppItem {
            id: "unicore-office".to_string(),
            name: if ms { "Dokumen & Pejabat" } else { "Office Writer" }.to_string(),
            comment: if ms { "Sunting dokumen (.docx/.odt) & teks" } else { "Edit rich documents (.docx/.odt) & notes" }.to_string(),
            category: "office".to_string(),
            exec_cmd: "sh -c 'if command -v libreoffice >/dev/null 2>&1; then exec libreoffice --writer; else exec abiword; fi'".to_string(),
            badge: "DOC".to_string(),
            accent_hex: "#059669".to_string(),
            pinned: true,
        },
        DesktopAppItem {
            id: "unicore-capture".to_string(),
            name: if ms { "Tangkap & Rakam" } else { "Screen Capture" }.to_string(),
            comment: if ms { "Tangkapan skrin & rakaman video MP4" } else { "Screenshots & MP4 screen recorder" }.to_string(),
            category: "media".to_string(),
            exec_cmd: "/usr/bin/unicore-center --capture".to_string(),
            badge: "REC".to_string(),
            accent_hex: "#e11d48".to_string(),
            pinned: true,
        },
        DesktopAppItem {
            id: "unicore-ai".to_string(),
            name: if ms { "Pembantu AI" } else { "AI Copilot" }.to_string(),
            comment: if ms { "Pembantu AI tempatan atau awan (Super+A)" } else { "Local or cloud AI assistant (Super+A)" }.to_string(),
            category: "internet".to_string(),
            exec_cmd: "/usr/bin/unicore-center --ai".to_string(),
            badge: "AI".to_string(),
            accent_hex: "#6366f1".to_string(),
            pinned: true,
        },
        DesktopAppItem {
            id: "unicore-settings".to_string(),
            name: if ms { "Pusat Kawalan" } else { "Control Panel" }.to_string(),
            comment: if ms { "Rangkaian, Bunyi, Paparan, Bahasa, Peranti & Storan" } else { "Network, Audio, Display, Region, Devices & Storage" }.to_string(),
            category: "system".to_string(),
            exec_cmd: "/usr/bin/unicore-center --control-panel".to_string(),
            badge: "CP".to_string(),
            accent_hex: "#0f766e".to_string(),
            pinned: true,
        },
        DesktopAppItem {
            id: "unicore-retro".to_string(),
            name: if ms { "Game Klasik" } else { "Classic Games" }.to_string(),
            comment: if ms { "Auto-talaan 1-klik untuk game PC lama (.exe/.iso/.zip)" } else { "1-click auto-tune for classic PC games (.exe/.iso/.zip)" }.to_string(),
            category: "gaming".to_string(),
            exec_cmd: "/usr/bin/unicore-center --retro-game".to_string(),
            badge: "98".to_string(),
            accent_hex: "#9333ea".to_string(),
            pinned: true,
        },
    ]
}

fn classify_desktop_categories(raw_cats: &str) -> (&'static str, &'static str) {
    let lower = raw_cats.to_ascii_lowercase();
    if lower.contains("game") {
        ("gaming", "#9333ea")
    } else if lower.contains("network") || lower.contains("webbrowser") || lower.contains("chat") {
        ("internet", "#0284c7")
    } else if lower.contains("office") || lower.contains("wordprocessor") || lower.contains("spreadsheet") {
        ("office", "#059669")
    } else if lower.contains("audiovideo") || lower.contains("graphics") || lower.contains("video") || lower.contains("audio") {
        ("media", "#e11d48")
    } else {
        ("system", "#475569")
    }
}

fn clean_desktop_exec(raw: &str) -> String {
    let mut out = Vec::new();
    for tok in raw.split_whitespace() {
        if matches!(tok, "%f" | "%F" | "%u" | "%U" | "%i" | "%c" | "%k" | "%d" | "%D" | "%n" | "%N") {
            continue;
        }
        out.push(tok);
    }
    out.join(" ")
}

fn make_badge(name: &str) -> String {
    let letters: String = name
        .split_whitespace()
        .filter_map(|w| w.chars().find(|c| c.is_ascii_alphanumeric()))
        .take(2)
        .collect::<String>()
        .to_ascii_uppercase();
    if letters.is_empty() {
        "AP".to_string()
    } else {
        letters
    }
}

/// Parse an XDG `.desktop` file into a Start Menu entry (skips `NoDisplay=true` / `Hidden=true`).
pub fn parse_desktop_file(raw: &str, id_hint: &str) -> Option<DesktopAppItem> {
    let mut in_main = false;
    let mut name = String::new();
    let mut comment = String::new();
    let mut exec_raw = String::new();
    let mut categories = String::new();
    let mut no_display = false;
    let mut hidden = false;
    let mut entry_type = String::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_main = trimmed == "[Desktop Entry]";
            continue;
        }
        if !in_main {
            continue;
        }
        let Some((k, v)) = trimmed.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let val = v.trim();
        match key {
            "Type" => entry_type = val.to_string(),
            "Name" => name = val.to_string(),
            "Comment" | "GenericName" if comment.is_empty() => comment = val.to_string(),
            "Exec" => exec_raw = val.to_string(),
            "Categories" => categories = val.to_string(),
            "NoDisplay" => no_display = val.eq_ignore_ascii_case("true"),
            "Hidden" => hidden = val.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    if no_display || hidden || name.is_empty() || exec_raw.is_empty() {
        return None;
    }
    if !entry_type.is_empty() && !entry_type.eq_ignore_ascii_case("Application") {
        return None;
    }
    let exec_cmd = clean_desktop_exec(&exec_raw);
    if exec_cmd.is_empty() {
        return None;
    }
    let (cat, hex) = classify_desktop_categories(&categories);
    Some(DesktopAppItem {
        id: id_hint.to_string(),
        badge: make_badge(&name),
        name,
        comment,
        category: cat.to_string(),
        exec_cmd,
        accent_hex: hex.to_string(),
        pinned: false,
    })
}

pub fn builtin_ecosystem_apps(lang: UiLang) -> Vec<DesktopAppItem> {
    let ms = lang == UiLang::Ms;
    vec![
        DesktopAppItem {
            id: "unicore-phone".to_string(),
            name: if ms { "Sambung Telefon" } else { "Phone Link" }.to_string(),
            comment: if ms { "KDE Connect notifikasi/SMS & cermin skrin Android (scrcpy)" } else { "KDE Connect notifications/SMS & Android screen mirror (scrcpy)" }.to_string(),
            category: "internet".to_string(),
            exec_cmd: "/usr/bin/unicore-center --phone".to_string(),
            badge: "PH".to_string(),
            accent_hex: "#0284c7".to_string(),
            pinned: false,
        },
        DesktopAppItem {
            id: "unicore-cloud".to_string(),
            name: if ms { "Pemacu Awan (Cloud)" } else { "Cloud Drives" }.to_string(),
            comment: if ms { "Lekap pemacu awan OAuth, WebDAV & Nextcloud ke ~/Cloud" } else { "Mount OAuth, WebDAV & Nextcloud drives in ~/Cloud" }.to_string(),
            category: "internet".to_string(),
            exec_cmd: "/usr/bin/unicore-center --cloud".to_string(),
            badge: "CL".to_string(),
            accent_hex: "#2563eb".to_string(),
            pinned: false,
        },
        DesktopAppItem {
            id: "unicore-migrate".to_string(),
            name: if ms { "Migrasi Data PC" } else { "PC Data Migration" }.to_string(),
            comment: if ms { "Import selamat fail, penanda buku & fon dari partisyen NTFS" } else { "Safely import files, bookmarks & fonts from NTFS drives" }.to_string(),
            category: "system".to_string(),
            exec_cmd: "/usr/bin/unicore-center --migrate".to_string(),
            badge: "MG".to_string(),
            accent_hex: "#059669".to_string(),
            pinned: false,
        },
        DesktopAppItem {
            id: "unicore-drivers".to_string(),
            name: if ms { "Perkakasan & Pemacu" } else { "Hardware & Drivers" }.to_string(),
            comment: if ms { "Status GPU/Wi-Fi/USB, baiki audio/BT & firmware LVFS" } else { "GPU/Wi-Fi/USB status, audio/BT fix & LVFS firmware updates" }.to_string(),
            category: "system".to_string(),
            exec_cmd: "/usr/bin/unicore-center --drivers".to_string(),
            badge: "HW".to_string(),
            accent_hex: "#d97706".to_string(),
            pinned: false,
        },
    ]
}

pub fn builtin_gamehub_apps(lang: UiLang) -> Vec<DesktopAppItem> {
    let ms = lang == UiLang::Ms;
    vec![
        DesktopAppItem {
            id: "unicore-gamehub".to_string(),
            name: if ms { "Hab Permainan (Game Hub)" } else { "Unified Game Hub" }.to_string(),
            comment: if ms { "Semua game 98/XP, DOS, Steam, Heroic, Android + Gamepad & Cloud Save" } else { "All 98/XP, DOS, Steam, Heroic, Android games + Gamepad & Cloud Saves" }.to_string(),
            category: "gaming".to_string(),
            exec_cmd: "/usr/bin/unicore-center --games".to_string(),
            badge: "GH".to_string(),
            accent_hex: "#7c3aed".to_string(),
            pinned: false,
        },
    ]
}

pub fn builtin_guide_apps(lang: UiLang) -> Vec<DesktopAppItem> {
    let ms = lang == UiLang::Ms;
    vec![
        DesktopAppItem {
            id: "unicore-guide".to_string(),
            name: if ms { "Panduan Pengguna" } else { "User Guide" }.to_string(),
            comment: if ms { "Panduan rasmi luar talian BM/EN, pintasan & audit keselamatan" } else { "Official offline EN/BM user guide, shortcuts & live security audit" }.to_string(),
            category: "system".to_string(),
            exec_cmd: "/usr/bin/unicore-center --help-guide".to_string(),
            badge: "DOC".to_string(),
            accent_hex: "#0d9488".to_string(),
            pinned: false,
        },
    ]
}

pub fn builtin_v110_apps(lang: UiLang) -> Vec<DesktopAppItem> {
    let ms = lang == UiLang::Ms;
    vec![
        DesktopAppItem {
            id: "unicore-webapps".to_string(),
            name: if ms { "Aplikasi Web (PWA)" } else { "Web Apps (PWA)" }.to_string(),
            comment: if ms { "Pasang 1-klik WhatsApp, YouTube, Gmail, M365, Canva & URL tersuai" } else { "1-click install WhatsApp, YouTube, Gmail, M365, Canva & custom URLs" }.to_string(),
            category: "internet".to_string(),
            exec_cmd: "/usr/bin/unicore-center --webapps".to_string(),
            badge: "PWA".to_string(),
            accent_hex: "#0284c7".to_string(),
            pinned: false,
        },
        DesktopAppItem {
            id: "unicore-parental".to_string(),
            name: if ms { "Kawalan Ibu Bapa" } else { "Family & Parental" }.to_string(),
            comment: if ms { "Penapis DNS Keluarga (1.1.1.3), had masa skrin & sekatan aplikasi" } else { "Family Safe DNS (1.1.1.3), daily screen-time limits & app blocking" }.to_string(),
            category: "system".to_string(),
            exec_cmd: "/usr/bin/unicore-center --parental".to_string(),
            badge: "FAM".to_string(),
            accent_hex: "#059669".to_string(),
            pinned: false,
        },
    ]
}

pub fn builtin_v130_apps(lang: UiLang) -> Vec<DesktopAppItem> {
    let ms = lang == UiLang::Ms;
    vec![DesktopAppItem {
        id: "unicore-themes".to_string(),
        name: if ms { "10 Tema Desktop" } else { "10 Desktop Themes" }.to_string(),
        comment: if ms {
            "Studio 10 tema lengkap: Midnight, Daylight, Aurora, Nordic, Dracula, Emerald, Sunset, Sakura, Cyberpunk & Classic 98"
        } else {
            "10 complete desktop environments: Midnight, Daylight, Aurora, Nordic, Dracula, Emerald, Sunset, Sakura, Cyberpunk & Classic 98"
        }
        .to_string(),
        category: "system".to_string(),
        exec_cmd: "/usr/bin/unicore-center --themes".to_string(),
        badge: "TH".to_string(),
        accent_hex: "#ec4899".to_string(),
        pinned: false,
    }]
}

pub fn builtin_v150_apps(lang: UiLang) -> Vec<DesktopAppItem> {
    let ms = lang == UiLang::Ms;
    vec![
        DesktopAppItem {
            id: "unicore-devmgr".to_string(),
            name: if ms { "Pengurus Peranti" } else { "Device Manager" }.to_string(),
            comment: if ms { "Semak semua peranti CPU, GPU, Rangkaian, Audio, USB & pemacu" } else { "Inspect all CPU, GPU, Network, Audio, USB devices & kernel drivers" }.to_string(),
            category: "system".to_string(),
            exec_cmd: "/usr/bin/unicore-center --devmgr".to_string(),
            badge: "DM".to_string(),
            accent_hex: "#0284c7".to_string(),
            pinned: false,
        },
        DesktopAppItem {
            id: "unicore-storage".to_string(),
            name: if ms { "Pengurus Storan" } else { "Storage Manager" }.to_string(),
            comment: if ms { "Cakera, partisyen, kesihatan S.M.A.R.T, keluar USB & bersihkan cache" } else { "Disks, partitions, S.M.A.R.T health, safe USB eject & cache cleaner" }.to_string(),
            category: "system".to_string(),
            exec_cmd: "/usr/bin/unicore-center --storage".to_string(),
            badge: "ST".to_string(),
            accent_hex: "#d97706".to_string(),
            pinned: false,
        },
        DesktopAppItem {
            id: "unicore-region".to_string(),
            name: if ms { "Bahasa, Wilayah & Papan Kekunci" } else { "Region, Language & Keyboard" }.to_string(),
            comment: if ms { "12 bahasa antarabangsa, 21 susun atur papan kekunci & 32 zon masa" } else { "12 international languages, 21 global keyboards & 32 world timezones" }.to_string(),
            category: "system".to_string(),
            exec_cmd: "/usr/bin/unicore-center --region".to_string(),
            badge: "RG".to_string(),
            accent_hex: "#059669".to_string(),
            pinned: false,
        },
    ]
}

pub fn builtin_v160_apps(lang: UiLang) -> Vec<DesktopAppItem> {
    let ms = lang == UiLang::Ms;
    vec![
        DesktopAppItem {
            id: "unicore-aegis-vpn".to_string(),
            name: "AegisTunnel VPN".to_string(),
            comment: if ms { "VPN WireGuard + Anti-DPI AmneziaWG + REALITY + PQ ML-KEM-768 (0 MB idle)" } else { "WireGuard + AmneziaWG Anti-DPI + REALITY + PQ ML-KEM-768 VPN (0 MB idle)" }.to_string(),
            category: "internet".to_string(),
            exec_cmd: "/usr/bin/unicore-center --vpn".to_string(),
            badge: "VPN".to_string(),
            accent_hex: "#0284c7".to_string(),
            pinned: false,
        },
        DesktopAppItem {
            id: "unicore-resonance-fx".to_string(),
            name: if ms { "Equalizer ResonanceFX" } else { "ResonanceFX Equalizer" }.to_string(),
            comment: if ms { "Equalizer 10-jalur PipeWire + Pure Bass + 3D Surround + 8D Orbit + Convolver (0 MB idle)" } else { "PipeWire 10-band EQ + Pure Bass + 3D Surround + 8D Orbit + Convolver DSP (0 MB idle)" }.to_string(),
            category: "media".to_string(),
            exec_cmd: "/usr/bin/unicore-center --equalizer".to_string(),
            badge: "EQ".to_string(),
            accent_hex: "#9333ea".to_string(),
            pinned: false,
        },
    ]
}

pub fn discover_start_menu_apps(
    scan_dirs: &[&Path],
    sandbox_apps: &[AppSandbox],
    lang: UiLang,
) -> Vec<DesktopAppItem> {
    let mut items = builtin_pinned_apps(lang);
    items.extend(builtin_v150_apps(lang));
    items.extend(builtin_v160_apps(lang));
    items.extend(builtin_ecosystem_apps(lang));
    items.extend(builtin_gamehub_apps(lang));
    items.extend(builtin_guide_apps(lang));
    items.extend(builtin_v110_apps(lang));
    items.extend(builtin_v130_apps(lang));
    let mut seen_names: HashSet<String> = items
        .iter()
        .map(|i| i.name.to_ascii_lowercase())
        .collect();

    // Add registered UniCore sandbox apps (.exe/.apk/etc.) prominently
    for app in sandbox_apps {
        let key = app.name.to_ascii_lowercase();
        if seen_names.insert(key) {
            items.push(DesktopAppItem {
                id: format!("sandbox:{}", app.app_id),
                badge: make_badge(&app.name),
                name: app.name.clone(),
                comment: format!("UniCore App ({})", app.platform.as_str()),
                category: if app.platform.as_str().contains("win") || app.platform.as_str().contains("dos") {
                    "gaming".to_string()
                } else {
                    "system".to_string()
                },
                exec_cmd: format!("/usr/libexec/unicore-app-runner {}", app.app_id),
                accent_hex: "#0284c7".to_string(),
                pinned: false,
            });
        }
    }

    // Scan XDG desktop directories
    let mut discovered = Vec::new();
    for dir in scan_dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let id_hint = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("app");
            if let Some(item) = parse_desktop_file(&raw, id_hint) {
                let key = item.name.to_ascii_lowercase();
                if seen_names.insert(key) {
                    discovered.push(item);
                }
            }
        }
    }
    discovered.sort_by_key(|a| a.name.to_ascii_lowercase());
    items.extend(discovered);
    items
}

pub fn filter_start_menu_apps(
    all: &[DesktopAppItem],
    category: &str,
    query: &str,
) -> Vec<DesktopAppItem> {
    let cat = category.trim().to_ascii_lowercase();
    let q = query.trim().to_ascii_lowercase();
    all.iter()
        .filter(|app| {
            let cat_ok = cat.is_empty()
                || cat == "all"
                || (cat == "pinned" && app.pinned)
                || app.category == cat;
            let q_ok = q.is_empty()
                || app.name.to_ascii_lowercase().contains(&q)
                || app.comment.to_ascii_lowercase().contains(&q)
                || app.category.contains(&q);
            cat_ok && q_ok
        })
        .cloned()
        .collect()
}

/// Filter apps for the "All Applications" section below Pinned Apps.
/// When viewing the unfiltered "All" list with an empty search query, exclude apps
/// already shown in the top Pinned Apps grid to avoid visual duplication.
pub fn filter_start_menu_all_section(
    all: &[DesktopAppItem],
    category: &str,
    query: &str,
) -> Vec<DesktopAppItem> {
    let mut filtered = filter_start_menu_apps(all, category, query);
    let cat = category.trim();
    if (cat.is_empty() || cat.eq_ignore_ascii_case("all")) && query.trim().is_empty() {
        filtered.retain(|a| !a.pinned);
    }
    filtered
}

pub fn launch_desktop_command(exec_cmd: &str) -> Result<(), String> {
    let trimmed = exec_cmd.trim();
    if trimmed.is_empty() {
        return Err("Empty command".to_string());
    }
    if cfg!(target_os = "linux") {
        Command::new("/bin/sh")
            .args(["-c", &format!("{trimmed} </dev/null >/dev/null 2>&1 &")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to launch '{trimmed}': {e}"))?;
    }
    Ok(())
}

pub fn open_user_folder(folder: &str) -> Result<(), String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/unicore".to_string());
    let target = match folder.trim().to_ascii_lowercase().as_str() {
        "documents" => format!("{home}/Documents"),
        "downloads" => format!("{home}/Downloads"),
        "pictures" => format!("{home}/Pictures"),
        "videos" => format!("{home}/Videos"),
        "desktop" => format!("{home}/Desktop"),
        _ => home,
    };
    if cfg!(target_os = "linux") {
        Command::new("pcmanfm")
            .arg(&target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to open folder {target}: {e}"))?;
    }
    Ok(())
}

/// Format a warm, non-technical bilingual header status string for everyday users.
/// Full technical specs remain visible in `System -> Hardware & Diagnostics`.
pub fn friendly_hw_summary(tier: &str, ram_mb: u64, lang: UiLang) -> String {
    let ram_label = if ram_mb >= 1024 {
        format!("{:.1} GB RAM", (ram_mb as f64) / 1024.0)
    } else if ram_mb > 0 {
        format!("{ram_mb} MB RAM")
    } else {
        "UniCore PC".to_string()
    };
    let (mode_en, mode_ms) = match tier {
        "modern" => ("Performance Mode", "Mod Prestasi"),
        "legacy_ultra" => ("Lite Mode", "Mod Ringan"),
        _ => ("Balanced Mode", "Mod Seimbang"),
    };
    match lang {
        UiLang::Ms => format!("{mode_ms}  •  {ram_label}  •  Sedia Digunakan"),
        UiLang::En => format!("{mode_en}  •  {ram_label}  •  Ready to Use"),
    }
}

/// Curated 1-click OOBE starter app packs mapped to `catalog.json` IDs.
pub fn oobe_bundle_catalog_ids(office: bool, creative: bool, gaming: bool) -> Vec<&'static str> {
    let mut ids = Vec::new();
    if office {
        ids.push("libreoffice");
        ids.push("firefox");
    }
    if creative {
        ids.push("gimp");
        ids.push("vlc");
    }
    if gaming {
        ids.push("steam");
        ids.push("heroic");
    }
    ids
}
