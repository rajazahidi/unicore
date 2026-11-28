//! First-Boot Setup Wizard (OOBE) configuration, validation, and system helper integration.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::settings::SYS_HELPER_BIN;

pub const DEFAULT_OOBE_DIR: &str = "/data/unicore";
pub const OOBE_LANGUAGES: [&str; 12] = [
    "en_US.UTF-8",
    "ms_MY.UTF-8",
    "id_ID.UTF-8",
    "zh_CN.UTF-8",
    "ja_JP.UTF-8",
    "ko_KR.UTF-8",
    "ar_SA.UTF-8",
    "es_ES.UTF-8",
    "fr_FR.UTF-8",
    "de_DE.UTF-8",
    "pt_BR.UTF-8",
    "ru_RU.UTF-8",
];
pub const OOBE_LANGUAGE_LABELS: [&str; 12] = [
    "English (International - en_US.UTF-8)",
    "Bahasa Melayu (Malaysia - ms_MY.UTF-8)",
    "Bahasa Indonesia (id_ID.UTF-8)",
    "中文 简体 (Simplified Chinese - zh_CN.UTF-8)",
    "日本語 (Japanese - ja_JP.UTF-8)",
    "한국어 (Korean - ko_KR.UTF-8)",
    "العربية (Arabic - ar_SA.UTF-8)",
    "Español (Spanish - es_ES.UTF-8)",
    "Français (French - fr_FR.UTF-8)",
    "Deutsch (German - de_DE.UTF-8)",
    "Português (Portuguese - pt_BR.UTF-8)",
    "Русский (Russian - ru_RU.UTF-8)",
];
pub const OOBE_TIMEZONES: [&str; 32] = [
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OobeSettings {
    pub language: String,
    pub timezone: String,
    pub hostname: String,
}

impl Default for OobeSettings {
    fn default() -> Self {
        Self {
            language: "en_US.UTF-8".to_string(),
            timezone: "UTC".to_string(),
            hostname: "unicore-pc".to_string(),
        }
    }
}

pub fn validate_hostname(raw: &str) -> Result<String, String> {
    let clean = raw.trim();
    if clean.is_empty() || clean.len() > 32 {
        return Err("Hostname must be 1 to 32 characters long".to_string());
    }
    let mut chars = clean.chars();
    let Some(first) = chars.next() else {
        return Err("Hostname cannot be empty".to_string());
    };
    if !first.is_ascii_alphanumeric() {
        return Err("Hostname must start with a letter or digit".to_string());
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Hostname may only contain letters, digits, and '-'".to_string());
    }
    Ok(clean.to_string())
}

pub fn validate_timezone(raw: &str) -> Result<String, String> {
    let clean = raw.trim();
    if clean.is_empty() || clean.contains("..") || clean.len() > 64 {
        return Err("Invalid timezone identifier".to_string());
    }
    if !clean
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '/' | '+' | '.' | '-'))
    {
        return Err("Timezone contains unsupported characters".to_string());
    }
    Ok(clean.to_string())
}

pub fn validate_user_password(pass: &str, confirm: &str) -> Result<Option<String>, String> {
    if pass.is_empty() && confirm.is_empty() {
        return Ok(None);
    }
    if pass != confirm {
        return Err("Account passwords do not match".to_string());
    }
    if pass.len() < 4 {
        return Err("Account password must be at least 4 characters (or leave blank to skip)".to_string());
    }
    if pass.chars().any(|c| c.is_control() || c == ':') {
        return Err("Account password contains invalid characters (no colons or control chars)".to_string());
    }
    Ok(Some(pass.to_string()))
}

pub fn render_oobe_conf(cfg: &OobeSettings) -> String {
    format!(
        "UNICORE_LANG=\"{}\"\nUNICORE_TIMEZONE=\"{}\"\nUNICORE_HOSTNAME=\"{}\"\n",
        cfg.language, cfg.timezone, cfg.hostname
    )
}

pub fn parse_oobe_conf(raw: &str) -> OobeSettings {
    let mut cfg = OobeSettings::default();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((k, v_raw)) = trimmed.split_once('=') else {
            continue;
        };
        let v = v_raw.trim().trim_matches('"').trim_matches('\'').trim();
        if v.is_empty() {
            continue;
        }
        match k.trim() {
            "UNICORE_LANG" => cfg.language = v.to_string(),
            "UNICORE_TIMEZONE" => cfg.timezone = v.to_string(),
            "UNICORE_HOSTNAME" => cfg.hostname = v.to_string(),
            _ => {}
        }
    }
    cfg
}

pub fn is_oobe_done(dir: &Path) -> bool {
    dir.join("oobe.done").is_file()
}

pub fn save_oobe_state(dir: &Path, cfg: &OobeSettings) -> Result<(), String> {
    let clean_host = validate_hostname(&cfg.hostname)?;
    let clean_tz = validate_timezone(&cfg.timezone)?;
    let normalized = OobeSettings {
        language: cfg.language.trim().to_string(),
        timezone: clean_tz,
        hostname: clean_host,
    };
    fs::create_dir_all(dir)
        .map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
    fs::write(dir.join("oobe.conf"), render_oobe_conf(&normalized))
        .map_err(|e| format!("Failed to write oobe.conf: {e}"))?;
    fs::write(dir.join("oobe.done"), "completed=1\n")
        .map_err(|e| format!("Failed to write oobe.done: {e}"))?;
    Ok(())
}

pub fn apply_oobe_on_host(
    cfg: &OobeSettings,
    password: Option<&str>,
) -> Result<(), String> {
    let oobe_dir = Path::new(DEFAULT_OOBE_DIR);
    save_oobe_state(oobe_dir, cfg)?;

    if !cfg!(target_os = "linux") {
        return Ok(());
    }

    let out = Command::new("doas")
        .args([SYS_HELPER_BIN, "apply-oobe", &cfg.timezone, &cfg.hostname])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to invoke OOBE system helper: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("apply-oobe exited with status {}", out.status)
        } else {
            stderr
        });
    }

    if let Some(pw) = password.filter(|s| !s.is_empty()) {
        let mut child = Command::new("doas")
            .args([SYS_HELPER_BIN, "set-user-password"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to invoke password helper: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            writeln!(stdin, "{pw}")
                .map_err(|e| format!("Failed to write password to helper: {e}"))?;
        }
        let pw_out = child
            .wait_with_output()
            .map_err(|e| format!("Failed waiting for password helper: {e}"))?;
        if !pw_out.status.success() {
            let stderr = String::from_utf8_lossy(&pw_out.stderr).trim().to_string();
            return Err(if stderr.is_empty() {
                format!("set-user-password exited with status {}", pw_out.status)
            } else {
                stderr
            });
        }
    }

    Ok(())
}
