//! UniCore OS v1.5.0 System Hub:
//! - Structured Device Manager (`unicore-devmgr list` & live SANE scanner detection)
//! - Storage Manager (`unicore-storage list/summary/eject/clean-cache`)
//! - International Languages (12), Global Keyboard Layouts (21) & World Regions/Timezones (32)
//! - Multi-Monitor Layout, Screen Rotation (`0/90/180/270`) & DDC/CI External Brightness

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::i18n::{persist_ui_lang, UiLang};
use crate::oobe::{parse_oobe_conf, render_oobe_conf, validate_timezone, DEFAULT_OOBE_DIR};
use crate::settings::{
    persistent_display_config_dir, render_display_apply_script, validate_kb_layout,
    PERSONALIZE_HELPER_BIN, SYS_HELPER_BIN,
};

pub const DEVMGR_HELPER_BIN: &str = "/usr/libexec/unicore-devmgr";
pub const STORAGE_HELPER_BIN: &str = "/usr/libexec/unicore-storage";

pub const INTL_LANGUAGES: [(&str, &str); 12] = [
    ("en_US.UTF-8", "English (International)"),
    ("ms_MY.UTF-8", "Bahasa Melayu (Malaysia)"),
    ("id_ID.UTF-8", "Bahasa Indonesia"),
    ("zh_CN.UTF-8", "中文 (简体 / Simplified Chinese)"),
    ("ja_JP.UTF-8", "日本語 (Japanese)"),
    ("ko_KR.UTF-8", "한국어 (Korean)"),
    ("ar_SA.UTF-8", "العربية (Arabic)"),
    ("es_ES.UTF-8", "Español (Spanish)"),
    ("fr_FR.UTF-8", "Français (French)"),
    ("de_DE.UTF-8", "Deutsch (German)"),
    ("pt_BR.UTF-8", "Português (Portuguese)"),
    ("ru_RU.UTF-8", "Русский (Russian)"),
];

pub const WORLD_TIMEZONES: [&str; 32] = [
    "UTC",
    "Asia/Kuala_Lumpur",
    "Asia/Singapore",
    "Asia/Jakarta",
    "Asia/Bangkok",
    "Asia/Manila",
    "Asia/Ho_Chi_Minh",
    "Asia/Tokyo",
    "Asia/Seoul",
    "Asia/Shanghai",
    "Asia/Hong_Kong",
    "Asia/Kolkata",
    "Asia/Dubai",
    "Asia/Riyadh",
    "Europe/London",
    "Europe/Paris",
    "Europe/Berlin",
    "Europe/Madrid",
    "Europe/Rome",
    "Europe/Amsterdam",
    "Europe/Istanbul",
    "Europe/Moscow",
    "America/New_York",
    "America/Chicago",
    "America/Denver",
    "America/Los_Angeles",
    "America/Toronto",
    "America/Sao_Paulo",
    "Australia/Sydney",
    "Pacific/Auckland",
    "Africa/Cairo",
    "Africa/Johannesburg",
];

pub const DISPLAY_ROTATIONS: [(&str, &str); 4] = [
    ("normal", "Landscape (0°)"),
    ("90", "Portrait (90°)"),
    ("180", "Flipped (180°)"),
    ("270", "Portrait Flipped (270°)"),
];

pub const MULTI_MONITOR_MODES: [(&str, &str); 4] = [
    ("auto", "Auto / Single Display"),
    ("extend-right", "Extend Secondary to Right"),
    ("extend-left", "Extend Secondary to Left"),
    ("mirror", "Mirror / Duplicate Displays"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceEntry {
    pub category: String,
    pub name: String,
    pub driver: String,
    pub bus: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannerEntry {
    pub device_uri: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageVolume {
    pub path: String,
    pub kind: String,
    pub model: String,
    pub size: String,
    pub fstype: String,
    pub mountpoint: String,
    pub removable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StorageSummary {
    pub root_usage: String,
    pub data_usage: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionConfig {
    pub locale: String,
    pub timezone: String,
    pub kb_layout: String,
    pub clock_format: String, // "24h" | "12h"
}

impl Default for RegionConfig {
    fn default() -> Self {
        Self {
            locale: "en_US.UTF-8".to_string(),
            timezone: "UTC".to_string(),
            kb_layout: "us".to_string(),
            clock_format: "24h".to_string(),
        }
    }
}

pub fn parse_devmgr_tsv(raw: &str) -> Vec<DeviceEntry> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts: Vec<&str> = trimmed.split('\t').collect();
        if parts.len() >= 5 {
            out.push(DeviceEntry {
                category: parts[0].trim().to_string(),
                name: parts[1].trim().to_string(),
                driver: parts[2].trim().to_string(),
                bus: parts[3].trim().to_string(),
                status: parts[4].trim().to_string(),
            });
        }
    }
    out
}

pub fn filter_devices(all: &[DeviceEntry], category: &str) -> Vec<DeviceEntry> {
    let cat = category.trim().to_ascii_lowercase();
    if cat.is_empty() || cat == "all" {
        return all.to_vec();
    }
    all.iter()
        .filter(|d| {
            let dc = d.category.to_ascii_lowercase();
            match cat.as_str() {
                "compute" => dc == "cpu" || dc == "graphics",
                "network" => dc == "network" || dc == "bluetooth",
                "audio" => dc == "audio" || dc == "camera",
                "storage" => dc == "storage",
                "usb" => dc == "usb" || dc == "input",
                other => dc == other,
            }
        })
        .cloned()
        .collect()
}

pub fn parse_scanners_tsv(raw: &str) -> Vec<ScannerEntry> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((uri, desc)) = trimmed.split_once('\t') {
            out.push(ScannerEntry {
                device_uri: uri.trim().to_string(),
                description: desc.trim().to_string(),
            });
        }
    }
    out
}

pub fn parse_storage_tsv(raw: &str) -> Vec<StorageVolume> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts: Vec<&str> = trimmed.split('\t').collect();
        if parts.len() >= 7 {
            out.push(StorageVolume {
                path: parts[0].trim().to_string(),
                kind: parts[1].trim().to_string(),
                model: parts[2].trim().to_string(),
                size: parts[3].trim().to_string(),
                fstype: parts[4].trim().to_string(),
                mountpoint: parts[5].trim().to_string(),
                removable: matches!(parts[6].trim(), "1" | "true" | "yes"),
            });
        }
    }
    out
}

pub fn parse_storage_summary(raw: &str) -> StorageSummary {
    let mut out = StorageSummary::default();
    for line in raw.lines() {
        if let Some((k, v)) = line.trim().split_once('=') {
            match k.trim() {
                "root_usage" => out.root_usage = v.trim().to_string(),
                "data_usage" => out.data_usage = v.trim().to_string(),
                _ => {}
            }
        }
    }
    if out.root_usage.is_empty() {
        out.root_usage = "Read-Only Immutable SquashFS".to_string();
    }
    if out.data_usage.is_empty() {
        out.data_usage = "Live RAM Mode".to_string();
    }
    out
}

pub fn validate_display_transform(raw: &str) -> Result<&'static str, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "normal" | "0" => Ok("normal"),
        "90" => Ok("90"),
        "180" => Ok("180"),
        "270" => Ok("270"),
        other => Err(format!("Unsupported display rotation '{other}'. Choose normal, 90, 180, or 270.")),
    }
}

pub fn validate_multi_monitor_mode(raw: &str) -> Result<&'static str, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok("auto"),
        "extend-right" => Ok("extend-right"),
        "extend-left" => Ok("extend-left"),
        "mirror" => Ok("mirror"),
        other => Err(format!("Unsupported multi-monitor mode '{other}'.")),
    }
}

pub fn validate_clock_format(raw: &str) -> Result<&'static str, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "12h" | "12" => Ok("12h"),
        "24h" | "24" => Ok("24h"),
        other => Err(format!("Unsupported clock format '{other}'. Choose 24h or 12h.")),
    }
}

pub fn render_display_apply_script_ext(
    output: &str,
    mode: &str,
    scale: &str,
    transform: &str,
    multi_mode: &str,
    secondary_output: Option<&str>,
) -> Result<String, String> {
    let base = render_display_apply_script(output, mode, scale)?;
    let tf = validate_display_transform(transform)?;
    let mm = validate_multi_monitor_mode(multi_mode)?;
    let mut script = base.trim_end().to_string();
    if tf != "normal" {
        script.push_str(&format!(" --transform {tf}"));
    }
    script.push('\n');
    if let Some(sec) = secondary_output.map(str::trim).filter(|s| !s.is_empty() && *s != output) {
        if sec.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) {
            match mm {
                "extend-right" => {
                    script.push_str(&format!(
                        "wlr-randr --output {sec} --on --right-of {output}\n"
                    ));
                }
                "extend-left" => {
                    script.push_str(&format!(
                        "wlr-randr --output {sec} --on --left-of {output}\n"
                    ));
                }
                "mirror" => {
                    script.push_str(&format!(
                        "wlr-randr --output {sec} --on --pos 0,0\n"
                    ));
                }
                _ => {}
            }
        }
    }
    Ok(script)
}

pub fn apply_and_save_display_ext(
    output: &str,
    mode: &str,
    scale: &str,
    transform: &str,
    multi_mode: &str,
    secondary_output: Option<&str>,
) -> Result<PathBuf, String> {
    let tf = validate_display_transform(transform)?;
    let script =
        render_display_apply_script_ext(output, mode, scale, tf, multi_mode, secondary_output)?;
    let cfg_dir = persistent_display_config_dir();
    fs::create_dir_all(&cfg_dir)
        .map_err(|e| format!("Failed to create display config dir: {e}"))?;
    let script_path = cfg_dir.join("display.sh");
    fs::write(&script_path, &script)
        .map_err(|e| format!("Failed to write {}: {e}", script_path.display()))?;
    crate::desktop::set_executable(&script_path);
    if cfg!(target_os = "linux") {
        let out = Command::new("/bin/sh")
            .arg(&script_path)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("Failed to apply display script: {e}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(if err.is_empty() {
                "Failed to apply display mode/rotation".to_string()
            } else {
                err
            });
        }
    }
    Ok(script_path)
}

pub fn set_external_ddc_brightness(pct: i32) -> Result<String, String> {
    let clamped = pct.clamp(5, 100);
    if !cfg!(target_os = "linux") {
        return Ok(format!("External monitor brightness set to {clamped}% (dry-run)"));
    }
    let out = Command::new("ddcutil")
        .args(["setvcp", "10", &clamped.to_string()])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("ddcutil unavailable: {e}"))?;
    if out.status.success() {
        Ok(format!("External monitor brightness set to {clamped}%"))
    } else {
        Err("No DDC/CI external monitor responded (standard on laptops/VMs)".to_string())
    }
}

pub fn read_device_manager() -> Vec<DeviceEntry> {
    if !cfg!(target_os = "linux") || !Path::new(DEVMGR_HELPER_BIN).is_file() {
        return vec![DeviceEntry {
            category: "CPU".to_string(),
            name: format!("Host Processor ({})", std::env::consts::ARCH),
            driver: "native".to_string(),
            bus: "System".to_string(),
            status: "Healthy".to_string(),
        }];
    }
    Command::new(DEVMGR_HELPER_BIN)
        .arg("list")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_devmgr_tsv(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

pub fn read_scanners() -> Vec<ScannerEntry> {
    if !cfg!(target_os = "linux") || !Path::new(DEVMGR_HELPER_BIN).is_file() {
        return Vec::new();
    }
    Command::new(DEVMGR_HELPER_BIN)
        .arg("scanners")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_scanners_tsv(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

pub fn read_storage_manager() -> (Vec<StorageVolume>, StorageSummary) {
    if !cfg!(target_os = "linux") || !Path::new(STORAGE_HELPER_BIN).is_file() {
        return (Vec::new(), StorageSummary::default());
    }
    let vols = Command::new(STORAGE_HELPER_BIN)
        .arg("list")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_storage_tsv(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default();
    let summary = Command::new(STORAGE_HELPER_BIN)
        .arg("summary")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_storage_summary(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default();
    (vols, summary)
}

pub fn storage_eject_device(dev_path: &str) -> Result<String, String> {
    let clean = dev_path.trim();
    if !clean.starts_with("/dev/") || clean.contains("..") {
        return Err(format!("Invalid storage device path: {clean}"));
    }
    if !cfg!(target_os = "linux") {
        return Ok(format!("Safely ejected {clean} (dry-run)"));
    }
    let out = Command::new("doas")
        .args([SYS_HELPER_BIN, "storage-eject", clean])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to eject {clean}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn storage_clean_cache() -> Result<String, String> {
    if !cfg!(target_os = "linux") || !Path::new(STORAGE_HELPER_BIN).is_file() {
        return Ok("Temporary cache cleaned (0 KB)".to_string());
    }
    let out = Command::new(STORAGE_HELPER_BIN)
        .arg("clean-cache")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to clean cache: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn read_region_config() -> RegionConfig {
    let mut cfg = RegionConfig::default();
    let oobe_path = Path::new(DEFAULT_OOBE_DIR).join("oobe.conf");
    if let Ok(raw) = fs::read_to_string(&oobe_path) {
        let oobe = parse_oobe_conf(&raw);
        cfg.locale = oobe.language;
        cfg.timezone = oobe.timezone;
    } else if let Ok(tz) = fs::read_to_string("/etc/timezone") {
        let t = tz.trim();
        if !t.is_empty() {
            cfg.timezone = t.to_string();
        }
    }
    let pconf = persistent_display_config_dir().join("personalize.conf");
    if let Ok(raw) = fs::read_to_string(&pconf) {
        for line in raw.lines() {
            if let Some((k, v)) = line.trim().split_once('=') {
                match k.trim() {
                    "UNICORE_KB_LAYOUT" => {
                        if let Ok(kb) = validate_kb_layout(v) {
                            cfg.kb_layout = kb;
                        }
                    }
                    "UNICORE_CLOCK_FORMAT" => {
                        if let Ok(cf) = validate_clock_format(v) {
                            cfg.clock_format = cf.to_string();
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    cfg
}

pub fn apply_region_settings(
    locale: &str,
    timezone: &str,
    kb_layout: &str,
    clock_format: &str,
) -> Result<UiLang, String> {
    let clean_tz = validate_timezone(timezone)?;
    let clean_kb = validate_kb_layout(kb_layout)?;
    let clean_cf = validate_clock_format(clock_format)?;
    let clean_loc = INTL_LANGUAGES
        .iter()
        .find(|(code, _)| *code == locale.trim())
        .map(|(code, _)| *code)
        .unwrap_or("en_US.UTF-8");

    let oobe_dir = Path::new(DEFAULT_OOBE_DIR);
    let conf_path = oobe_dir.join("oobe.conf");
    let mut oobe = fs::read_to_string(&conf_path)
        .map(|raw| parse_oobe_conf(&raw))
        .unwrap_or_default();
    oobe.language = clean_loc.to_string();
    oobe.timezone = clean_tz.clone();
    if oobe_dir.is_dir() || fs::create_dir_all(oobe_dir).is_ok() {
        let _ = fs::write(&conf_path, render_oobe_conf(&oobe));
    }

    let ui_lang = UiLang::from_locale_str(clean_loc);
    let _ = persist_ui_lang(oobe_dir, ui_lang);

    if cfg!(target_os = "linux") {
        let _ = Command::new("doas")
            .args([SYS_HELPER_BIN, "set-timezone", &clean_tz])
            .stdin(Stdio::null())
            .output();
        if Path::new(PERSONALIZE_HELPER_BIN).is_file() {
            let _ = Command::new(PERSONALIZE_HELPER_BIN)
                .args(["set", "--kb-layout", &clean_kb, "--clock-format", clean_cf])
                .stdin(Stdio::null())
                .output();
        }
    }
    Ok(ui_lang)
}
