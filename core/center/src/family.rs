//! UniCore OS v1.1.0 1-Click Web Apps (PWA) & Family Parental Controls helpers.

use std::collections::HashMap;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CuratedWebApp {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub category: &'static str,
    pub badge: &'static str,
    pub accent_hex: &'static str,
    pub summary_en: &'static str,
    pub summary_ms: &'static str,
}

pub const CURATED_WEBAPPS: &[CuratedWebApp] = &[
    CuratedWebApp {
        id: "whatsapp",
        name: "WhatsApp Web",
        url: "https://web.whatsapp.com",
        category: "social",
        badge: "WA",
        accent_hex: "#16a34a",
        summary_en: "Chat, voice notes & file sharing in its own window",
        summary_ms: "Sembang, mesej suara & kongsi fail dalam tetingkap sendiri",
    },
    CuratedWebApp {
        id: "youtube",
        name: "YouTube",
        url: "https://www.youtube.com",
        category: "media",
        badge: "YT",
        accent_hex: "#dc2626",
        summary_en: "Watch HD videos & music without browser tab clutter",
        summary_ms: "Tonton video HD & muzik tanpa tab pelayar yang sesak",
    },
    CuratedWebApp {
        id: "gmail",
        name: "Gmail",
        url: "https://mail.google.com",
        category: "internet",
        badge: "GM",
        accent_hex: "#ea580c",
        summary_en: "Fast desktop inbox for Google Mail",
        summary_ms: "Peti masuk desktop pantas untuk Google Mail",
    },
    CuratedWebApp {
        id: "m365",
        name: "Web Office Suite",
        url: "https://www.office.com",
        category: "office",
        badge: "DOC",
        accent_hex: "#2563eb",
        summary_en: "Free online documents, spreadsheets & presentations",
        summary_ms: "Dokumen, hamparan & persembahan dalam talian percuma",
    },
    CuratedWebApp {
        id: "gdocs",
        name: "Cloud Documents",
        url: "https://docs.google.com",
        category: "office",
        badge: "CLD",
        accent_hex: "#0284c7",
        summary_en: "Cloud documents, spreadsheets & live collaboration",
        summary_ms: "Dokumen awan, hamparan & kerjasama pasukan",
    },
    CuratedWebApp {
        id: "canva",
        name: "Canva",
        url: "https://www.canva.com",
        category: "creative",
        badge: "CV",
        accent_hex: "#7c3aed",
        summary_en: "Posters, slides & social media graphic design",
        summary_ms: "Reka bentuk poster, slaid & grafik media sosial",
    },
    CuratedWebApp {
        id: "chatgpt",
        name: "ChatGPT",
        url: "https://chatgpt.com",
        category: "internet",
        badge: "AI",
        accent_hex: "#0f766e",
        summary_en: "AI assistant web app in a clean desktop window",
        summary_ms: "Pembantu AI dalam tetingkap desktop yang kemas",
    },
    CuratedWebApp {
        id: "spotify",
        name: "Spotify Web",
        url: "https://open.spotify.com",
        category: "media",
        badge: "SP",
        accent_hex: "#15803d",
        summary_en: "Stream playlists & podcasts with 0 MB idle bloat",
        summary_ms: "Strim lagu & audio siar dengan 0 MB kembung terbiar",
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebAppEntry {
    pub id: String,
    pub name: String,
    pub url: String,
    pub category: String,
    pub badge: String,
    pub accent_hex: String,
    pub summary: String,
    pub installed: bool,
    pub custom: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentalOverview {
    pub enabled: bool,
    pub dns_filter: String,
    pub daily_limit_min: u32,
    pub bedtime: String,
    pub blocked_apps: String,
    pub today_used_min: u32,
}

impl Default for ParentalOverview {
    fn default() -> Self {
        Self {
            enabled: false,
            dns_filter: "off".to_string(),
            daily_limit_min: 0,
            bedtime: "off".to_string(),
            blocked_apps: String::new(),
            today_used_min: 0,
        }
    }
}

pub fn validate_webapp_id(id: &str) -> Result<String, String> {
    let clean = id.trim().to_ascii_lowercase();
    if clean.is_empty() || clean.len() > 32 {
        return Err("Web app ID must be 1–32 characters (a-z, 0-9, -, _).".to_string());
    }
    let mut chars = clean.chars();
    let Some(first) = chars.next() else {
        return Err("Web app ID cannot be empty.".to_string());
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err("Web app ID must start with a letter or digit.".to_string());
    }
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
        return Err("Web app ID may only contain lowercase letters, digits, '-' or '_'.".to_string());
    }
    Ok(clean)
}

pub fn slugify_webapp_id(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.chars() {
        let low = ch.to_ascii_lowercase();
        if low.is_ascii_lowercase() || low.is_ascii_digit() {
            out.push(low);
            prev_dash = false;
        } else if !out.is_empty() && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
        if out.len() >= 28 {
            break;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "webapp".to_string()
    } else {
        out
    }
}

pub fn validate_webapp_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    if !trimmed.starts_with("https://") || trimmed.len() < 11 || trimmed.len() > 512 {
        return Err("Web app URL must start with https:// (example: https://web.whatsapp.com).".to_string());
    }
    if trimmed.chars().any(|c| {
        c.is_whitespace()
            || c.is_control()
            || matches!(c, '"' | '\'' | '$' | '`' | '\\' | ';' | '<' | '>' | '|' | '&')
    }) {
        return Err("Web app URL contains unsafe characters.".to_string());
    }
    Ok(trimmed.to_string())
}

pub fn validate_blocked_apps_csv(raw: &str) -> Result<String, String> {
    let mut cleaned = Vec::new();
    for item in raw.split(',') {
        let t = item.trim();
        if t.is_empty() {
            continue;
        }
        if !t
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-' | '.'))
        {
            return Err(format!("Invalid blocked app identifier '{t}'."));
        }
        cleaned.push(t.to_string());
    }
    let joined = cleaned.join(",");
    if joined.len() > 512 {
        return Err("Blocked apps list is too long (max 512 characters).".to_string());
    }
    Ok(joined)
}

pub fn parse_webapps_tsv(raw: &str) -> Vec<WebAppEntry> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cols: Vec<&str> = trimmed.split('\t').collect();
        if cols.len() < 3 {
            continue;
        }
        let id = cols[0].trim().to_string();
        let name = cols[1].trim().to_string();
        let url = cols[2].trim().to_string();
        if id.is_empty() || name.is_empty() || !url.starts_with("https://") {
            continue;
        }
        let category = cols.get(3).copied().unwrap_or("internet").trim().to_string();
        let badge = cols.get(4).copied().unwrap_or("WA").trim().to_string();
        let accent_hex = cols.get(5).copied().unwrap_or("#0284c7").trim().to_string();
        out.push(WebAppEntry {
            id,
            name,
            summary: url.clone(),
            url,
            category,
            badge,
            accent_hex,
            installed: true,
            custom: true,
        });
    }
    out
}

pub fn merge_webapp_catalog(installed_rows: &[WebAppEntry], malay: bool) -> Vec<WebAppEntry> {
    let mut inst_map: HashMap<String, &WebAppEntry> = HashMap::new();
    for row in installed_rows {
        inst_map.insert(row.id.clone(), row);
    }

    let mut merged = Vec::new();
    for cur in CURATED_WEBAPPS {
        let is_inst = inst_map.remove(cur.id).is_some();
        merged.push(WebAppEntry {
            id: cur.id.to_string(),
            name: cur.name.to_string(),
            url: cur.url.to_string(),
            category: cur.category.to_string(),
            badge: cur.badge.to_string(),
            accent_hex: cur.accent_hex.to_string(),
            summary: if malay { cur.summary_ms } else { cur.summary_en }.to_string(),
            installed: is_inst,
            custom: false,
        });
    }
    let mut customs: Vec<WebAppEntry> = inst_map.into_values().cloned().collect();
    customs.sort_by_key(|a| a.name.to_ascii_lowercase());
    merged.extend(customs);
    merged
}

pub fn parse_parental_status(raw: &str) -> ParentalOverview {
    let mut ov = ParentalOverview::default();
    for line in raw.lines() {
        let Some((k, v)) = line.trim().split_once('=') else {
            continue;
        };
        let val = v.trim();
        match k.trim() {
            "enabled" => ov.enabled = val == "1" || val == "true",
            "dns_filter" => ov.dns_filter = val.to_string(),
            "daily_limit_min" => ov.daily_limit_min = val.parse().unwrap_or(0),
            "bedtime" => ov.bedtime = val.to_string(),
            "blocked_apps" => ov.blocked_apps = val.to_string(),
            "today_used_min" => ov.today_used_min = val.parse().unwrap_or(0),
            _ => {}
        }
    }
    ov
}

pub fn dns_filter_from_index(idx: i32) -> &'static str {
    match idx {
        1 => "malware",
        2 => "family",
        _ => "off",
    }
}

pub fn dns_filter_to_index(dns: &str) -> i32 {
    match dns {
        "malware" => 1,
        "family" => 2,
        _ => 0,
    }
}

pub fn daily_limit_from_index(idx: i32) -> u32 {
    match idx {
        1 => 60,
        2 => 120,
        3 => 180,
        4 => 240,
        _ => 0,
    }
}

pub fn daily_limit_to_index(mins: u32) -> i32 {
    match mins {
        1..=75 => 1,
        76..=150 => 2,
        151..=210 => 3,
        211..=u32::MAX => 4,
        _ => 0,
    }
}

pub fn bedtime_from_index(idx: i32) -> &'static str {
    match idx {
        1 => "21:00-07:00",
        2 => "22:00-07:00",
        3 => "23:00-06:30",
        _ => "off",
    }
}

pub fn bedtime_to_index(bt: &str) -> i32 {
    match bt {
        "21:00-07:00" => 1,
        "22:00-07:00" => 2,
        "23:00-06:30" => 3,
        _ => 0,
    }
}

fn run_sys_helper(args: &[&str]) -> Result<String, String> {
    if !cfg!(target_os = "linux") {
        return Ok(String::new());
    }
    let out = Command::new("doas")
        .arg("/usr/libexec/unicore-sys-helper")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to run unicore-sys-helper: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() {
            format!("unicore-sys-helper failed ({})", out.status)
        } else {
            err
        })
    }
}

pub fn read_installed_webapps() -> Vec<WebAppEntry> {
    if !cfg!(target_os = "linux") {
        return Vec::new();
    }
    let Ok(out) = Command::new("/usr/libexec/unicore-webapps")
        .arg("list")
        .stdin(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    parse_webapps_tsv(&String::from_utf8_lossy(&out.stdout))
}

pub fn install_webapp(
    id: &str,
    name: &str,
    url: &str,
    category: &str,
    badge: &str,
    accent: &str,
) -> Result<String, String> {
    let clean_id = validate_webapp_id(id)?;
    let clean_url = validate_webapp_url(url)?;
    let clean_name = name.trim();
    if clean_name.is_empty() {
        return Err("Web app name cannot be empty.".to_string());
    }
    if !cfg!(target_os = "linux") {
        return Ok(format!("installed={clean_id}"));
    }
    let out = Command::new("/usr/libexec/unicore-webapps")
        .args([
            "install",
            &clean_id,
            clean_name,
            &clean_url,
            category,
            badge,
            accent,
        ])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to invoke unicore-webapps: {e}"))?;
    if out.status.success() {
        Ok(format!("Installed Web App '{clean_name}' to Start Menu."))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn launch_webapp(id: &str) -> Result<(), String> {
    let clean_id = validate_webapp_id(id)?;
    if !cfg!(target_os = "linux") {
        return Ok(());
    }
    Command::new("/usr/libexec/unicore-webapps")
        .args(["launch", &clean_id])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to launch Web App '{clean_id}': {e}"))?;
    Ok(())
}

pub fn remove_webapp(id: &str) -> Result<String, String> {
    let clean_id = validate_webapp_id(id)?;
    if !cfg!(target_os = "linux") {
        return Ok(format!("removed={clean_id}"));
    }
    let out = Command::new("/usr/libexec/unicore-webapps")
        .args(["remove", &clean_id])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to invoke unicore-webapps: {e}"))?;
    if out.status.success() {
        Ok(format!("Removed Web App '{clean_id}'."))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn read_parental_overview() -> ParentalOverview {
    if !cfg!(target_os = "linux") {
        return ParentalOverview::default();
    }
    let Ok(out) = Command::new("/usr/libexec/unicore-parental")
        .arg("status")
        .stdin(Stdio::null())
        .output()
    else {
        return ParentalOverview::default();
    };
    if !out.status.success() {
        return ParentalOverview::default();
    }
    parse_parental_status(&String::from_utf8_lossy(&out.stdout))
}

pub fn save_parental_policy(
    enabled: bool,
    dns_filter: &str,
    daily_limit_min: u32,
    bedtime: &str,
    blocked_apps: &str,
) -> Result<String, String> {
    let clean_blk = validate_blocked_apps_csv(blocked_apps)?;
    let en_s = if enabled { "1" } else { "0" };
    let lim_s = daily_limit_min.to_string();
    if clean_blk.is_empty() {
        run_sys_helper(&["parental-set", en_s, dns_filter, &lim_s, bedtime])
    } else {
        run_sys_helper(&["parental-set", en_s, dns_filter, &lim_s, bedtime, &clean_blk])
    }
}

pub fn reset_parental_usage() -> Result<String, String> {
    run_sys_helper(&["parental-reset-usage"])
}
