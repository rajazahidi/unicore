//! Multi-user account management, swayidle/power profile engine & S.M.A.R.T. disk health helpers.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::settings::SYS_HELPER_BIN;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAccount {
    pub username: String,
    pub uid: u32,
    pub role: String,
    pub has_password: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerOverview {
    pub profile: String,
    pub screen_min: i32,
    pub lock_min: i32,
    pub suspend_bat_min: i32,
    pub battery: String,
    pub on_ac: bool,
    pub swayidle_running: bool,
}

impl Default for PowerOverview {
    fn default() -> Self {
        Self {
            profile: "balanced".to_string(),
            screen_min: 5,
            lock_min: 10,
            suspend_bat_min: 20,
            battery: "N/A".to_string(),
            on_ac: true,
            swayidle_running: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmartDiskHealth {
    pub device: String,
    pub model: String,
    pub health: String,
    pub temp_c: String,
    pub power_on_hours: String,
}

impl Default for SmartDiskHealth {
    fn default() -> Self {
        Self {
            device: "none".to_string(),
            model: "No disk checked yet".to_string(),
            health: "Click Scan Disk Health".to_string(),
            temp_c: "N/A".to_string(),
            power_on_hours: "N/A".to_string(),
        }
    }
}

pub fn validate_new_username(raw: &str) -> Result<String, String> {
    let clean = raw.trim();
    if clean.len() < 2
        || clean.len() > 24
        || !clean.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        || !clean
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'))
    {
        return Err(
            "Username must be 2-24 chars, start with lowercase a-z, and contain only a-z, 0-9, '-' or '_'."
                .to_string(),
        );
    }
    if matches!(
        clean,
        "root" | "daemon" | "bin" | "sys" | "sync" | "nobody" | "all" | "guest" | "system"
    ) {
        return Err(format!("Reserved system username '{clean}' is not allowed."));
    }
    Ok(clean.to_string())
}

pub fn parse_users_list(raw: &str) -> Vec<UserAccount> {
    let mut users = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cols: Vec<&str> = trimmed.split('\t').collect();
        if cols.is_empty() || cols[0].is_empty() {
            continue;
        }
        let uid = cols.get(1).and_then(|s| s.parse::<u32>().ok()).unwrap_or(1000);
        let role = cols.get(2).copied().unwrap_or("standard").to_string();
        let has_password = cols
            .get(3)
            .is_some_and(|s| *s == "1" || s.eq_ignore_ascii_case("true"));
        users.push(UserAccount {
            username: cols[0].to_string(),
            uid,
            role,
            has_password,
        });
    }
    users
}

pub fn parse_power_status(raw: &str) -> PowerOverview {
    let mut ov = PowerOverview::default();
    for line in raw.lines() {
        let Some((k, v)) = line.trim().split_once('=') else {
            continue;
        };
        let val = v.trim();
        match k.trim() {
            "profile" => {
                if matches!(val, "powersave" | "balanced" | "performance") {
                    ov.profile = val.to_string();
                }
            }
            "screen_min" => ov.screen_min = val.parse::<i32>().unwrap_or(5).clamp(0, 120),
            "lock_min" => ov.lock_min = val.parse::<i32>().unwrap_or(10).clamp(0, 120),
            "suspend_bat_min" => ov.suspend_bat_min = val.parse::<i32>().unwrap_or(20).clamp(0, 240),
            "battery" => ov.battery = val.to_string(),
            "on_ac" => ov.on_ac = val != "0",
            "swayidle_running" => ov.swayidle_running = val == "1",
            _ => {}
        }
    }
    ov
}

pub fn parse_smart_health(raw: &str) -> SmartDiskHealth {
    let mut h = SmartDiskHealth::default();
    for line in raw.lines() {
        let Some((k, v)) = line.trim().split_once('=') else {
            continue;
        };
        let val = v.trim().to_string();
        match k.trim() {
            "device" => h.device = val,
            "model" => h.model = val,
            "health" => h.health = val,
            "temp_c" => h.temp_c = val,
            "power_on_hours" => h.power_on_hours = val,
            _ => {}
        }
    }
    h
}

pub fn read_users_list() -> Vec<UserAccount> {
    if !cfg!(target_os = "linux") || !Path::new(SYS_HELPER_BIN).is_file() {
        return vec![UserAccount {
            username: "unicore".to_string(),
            uid: 1000,
            role: "admin".to_string(),
            has_password: false,
        }];
    }
    Command::new("doas")
        .args([SYS_HELPER_BIN, "user-list"])
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_users_list(&String::from_utf8_lossy(&o.stdout)))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| {
            vec![UserAccount {
                username: "unicore".to_string(),
                uid: 1000,
                role: "admin".to_string(),
                has_password: false,
            }]
        })
}

fn run_sys_helper_with_stdin(args: &[&str], stdin_line: Option<&str>) -> Result<String, String> {
    let mut child = Command::new("doas")
        .arg(SYS_HELPER_BIN)
        .args(args)
        .stdin(if stdin_line.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to launch unicore-sys-helper: {e}"))?;

    if let Some(line) = stdin_line {
        if let Some(mut pipe) = child.stdin.take() {
            let _ = writeln!(pipe, "{line}");
        }
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("Failed waiting for unicore-sys-helper: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            format!("Action failed ({})", out.status)
        } else {
            err
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn add_user_account(username: &str, role: &str, password: &str) -> Result<String, String> {
    let clean = validate_new_username(username)?;
    let safe_role = if role == "admin" { "admin" } else { "standard" };
    if password.contains(['\n', '\r', '\0']) {
        return Err("Password contains invalid control characters.".to_string());
    }
    if !password.is_empty() && password.len() < 4 {
        return Err("Password must be at least 4 characters (or empty).".to_string());
    }
    run_sys_helper_with_stdin(&["user-add", &clean, safe_role], Some(password))
}

pub fn delete_user_account(username: &str) -> Result<String, String> {
    let clean = validate_new_username(username)?;
    run_sys_helper_with_stdin(&["user-delete", &clean], None)
}

pub fn set_account_password(username: &str, password: &str) -> Result<String, String> {
    let clean = validate_new_username(username)?;
    if password.contains(['\n', '\r', '\0']) {
        return Err("Password contains invalid control characters.".to_string());
    }
    run_sys_helper_with_stdin(&["user-passwd", &clean], Some(password))
}

pub fn authenticate_user(username: &str, password: &str) -> Result<(), String> {
    let clean = validate_new_username(username)?;
    run_sys_helper_with_stdin(&["auth-user", &clean], Some(password))?;
    Ok(())
}

pub fn read_power_overview() -> PowerOverview {
    if !cfg!(target_os = "linux") || !Path::new(SYS_HELPER_BIN).is_file() {
        return PowerOverview::default();
    }
    run_sys_helper_with_stdin(&["power-status"], None)
        .map(|raw| parse_power_status(&raw))
        .unwrap_or_default()
}

pub fn set_power_profile(profile: &str) -> Result<String, String> {
    if !matches!(profile, "powersave" | "balanced" | "performance") {
        return Err(format!("Unsupported power profile '{profile}'."));
    }
    run_sys_helper_with_stdin(&["power-profile", profile], None)
}

pub fn set_idle_timers(screen_min: i32, lock_min: i32, suspend_bat_min: i32) -> Result<(), String> {
    let s = screen_min.clamp(0, 120).to_string();
    let l = lock_min.clamp(0, 120).to_string();
    let b = suspend_bat_min.clamp(0, 240).to_string();
    run_sys_helper_with_stdin(&["power-idle", &s, &l, &b], None)?;
    if cfg!(target_os = "linux") && Path::new("/usr/libexec/unicore-power").is_file() {
        let _ = Command::new("/usr/libexec/unicore-power")
            .arg("apply-idle")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
    Ok(())
}

pub fn read_smart_health(device: Option<&str>) -> SmartDiskHealth {
    if !cfg!(target_os = "linux") || !Path::new(SYS_HELPER_BIN).is_file() {
        return SmartDiskHealth::default();
    }
    let res = match device {
        Some(d) if !d.trim().is_empty() => run_sys_helper_with_stdin(&["smart-health", d.trim()], None),
        _ => run_sys_helper_with_stdin(&["smart-health"], None),
    };
    res.map(|raw| parse_smart_health(&raw)).unwrap_or_default()
}
