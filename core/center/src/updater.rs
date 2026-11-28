//! GUI backend for UniCore OS A/B OTA and local ISO system updates & rollback.

use std::path::Path;
use std::process::{Command, Stdio};

use uniload::json_codec::{parse_json, JsonValue};
use uniload::updater::{
    build_update_status_json, load_host_update_config, parse_os_version, read_boot_slot_info,
    save_data_update_auto_check, validate_update_url, DEFAULT_DATA_UPDATE_CONF,
};

use crate::settings::SYS_HELPER_BIN;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateOverview {
    pub current_version: String,
    pub active_slot: String,
    pub standby_slot: String,
    pub pending_slot: String,
    pub boot_mode: String,
    pub boot_tried: bool,
    pub slot_a_version: String,
    pub slot_b_version: String,
    pub update_url: String,
    pub channel: String,
    pub can_update: bool,
    pub can_rollback: bool,
    pub pubkey_configured: bool,
    pub auto_check: bool,
}

impl UpdateOverview {
    pub fn pending_summary(&self) -> String {
        if !self.pending_slot.is_empty() {
            format!(
                "Trial pending on {} (boot_tried={})",
                self.pending_slot,
                if self.boot_tried { "1" } else { "0" }
            )
        } else if self.can_update {
            format!("Healthy / Confirmed ({})", self.active_slot)
        } else {
            format!("Read-only ({})", self.boot_mode)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCheckResult {
    pub update_available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub channel: String,
    pub notes: String,
    pub source_url: String,
    pub signature_status: String,
}

pub fn parse_update_status_json(raw: &str) -> Result<UpdateOverview, String> {
    let val = parse_json(raw)?;
    let obj = val
        .as_object()
        .ok_or_else(|| "Update status must be a JSON object".to_string())?;
    let get_str = |k: &str| {
        obj.get(k)
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .to_string()
    };
    let get_bool = |k: &str| obj.get(k).and_then(JsonValue::as_bool).unwrap_or(false);

    Ok(UpdateOverview {
        current_version: get_str("current_version"),
        active_slot: get_str("active_slot"),
        standby_slot: get_str("standby_slot"),
        pending_slot: get_str("pending_slot"),
        boot_mode: get_str("boot_mode"),
        boot_tried: get_bool("boot_tried"),
        slot_a_version: get_str("slot_a_version"),
        slot_b_version: get_str("slot_b_version"),
        update_url: get_str("update_url"),
        channel: get_str("channel"),
        can_update: get_bool("can_update"),
        can_rollback: get_bool("can_rollback"),
        pubkey_configured: get_bool("pubkey_configured"),
        auto_check: obj
            .get("auto_check")
            .and_then(JsonValue::as_bool)
            .unwrap_or(true),
    })
}

pub fn parse_update_check_json(raw: &str) -> Result<UpdateCheckResult, String> {
    let val = parse_json(raw)?;
    let obj = val
        .as_object()
        .ok_or_else(|| "Update check result must be a JSON object".to_string())?;
    let get_str = |k: &str| {
        obj.get(k)
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .to_string()
    };

    Ok(UpdateCheckResult {
        update_available: obj
            .get("update_available")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false),
        current_version: get_str("current_version"),
        latest_version: get_str("latest_version"),
        channel: get_str("channel"),
        notes: get_str("notes"),
        source_url: get_str("source_url"),
        signature_status: get_str("signature_status"),
    })
}

pub fn validate_local_update_path(path: &str) -> Result<String, String> {
    let clean = path.trim();
    if clean.is_empty() {
        return Err("Enter a local .iso, manifest.json, or update folder path.".to_string());
    }
    if !clean.starts_with('/') {
        return Err("Local update path must be an absolute Linux path (starting with /).".to_string());
    }
    if clean.contains("..") {
        return Err("Local update path cannot contain '..' segments.".to_string());
    }
    if clean
        .chars()
        .any(|c| c.is_control() || matches!(c, '"' | '\'' | '$' | '`' | '\\' | ';' | '<' | '>' | '|' | '&'))
    {
        return Err("Local update path contains unsafe characters.".to_string());
    }
    Ok(clean.to_string())
}

fn run_sys_helper(args: &[&str]) -> Result<String, String> {
    let out = Command::new("doas")
        .arg(SYS_HELPER_BIN)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to invoke {SYS_HELPER_BIN}: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !out.status.success() {
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(if detail.is_empty() {
            format!("Update helper exited with status {}", out.status)
        } else {
            detail
        });
    }
    Ok(stdout)
}

pub fn read_update_overview() -> UpdateOverview {
    if cfg!(target_os = "linux") {
        if let Ok(raw) = run_sys_helper(&["update-status"]) {
            if let Ok(parsed) = parse_update_status_json(&raw) {
                return parsed;
            }
        }
    }

    let cfg = load_host_update_config();
    let boot = read_boot_slot_info(Path::new("/mnt/unicore-boot.env"));
    let current_ver = std::fs::read_to_string("/etc/os-release")
        .map(|s| parse_os_version(&s))
        .unwrap_or_else(|_| "1.2.0".to_string());
    let json = build_update_status_json(&boot, &current_ver, None, &cfg, Some(false));
    parse_update_status_json(&json.to_string()).unwrap_or(UpdateOverview {
        current_version: current_ver,
        active_slot: "UNICORE_SYS_A".to_string(),
        standby_slot: "UNICORE_SYS_B".to_string(),
        pending_slot: String::new(),
        boot_mode: boot.boot_mode,
        boot_tried: false,
        slot_a_version: String::new(),
        slot_b_version: String::new(),
        update_url: cfg.update_url,
        channel: cfg.channel,
        can_update: false,
        can_rollback: false,
        pubkey_configured: false,
        auto_check: cfg.auto_check,
    })
}

pub fn set_auto_check_updates(enabled: bool) -> Result<(), String> {
    save_data_update_auto_check(Path::new(DEFAULT_DATA_UPDATE_CONF), enabled)
}

pub fn check_remote_update(url: &str) -> Result<UpdateCheckResult, String> {
    let trimmed = url.trim();
    if !trimmed.is_empty() {
        validate_update_url(trimmed, false)?;
        let out = run_sys_helper(&["update-check", trimmed])?;
        parse_update_check_json(&out)
    } else {
        let out = run_sys_helper(&["update-check"])?;
        parse_update_check_json(&out)
    }
}

pub fn apply_remote_update(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    validate_update_url(trimmed, false)?;
    run_sys_helper(&["update-apply-url", trimmed])
}

pub fn apply_local_update(path: &str, allow_unsigned: bool) -> Result<String, String> {
    let clean = validate_local_update_path(path)?;
    if allow_unsigned {
        run_sys_helper(&["update-apply-local", &clean, "allow-unsigned"])
    } else {
        run_sys_helper(&["update-apply-local", &clean])
    }
}

pub fn rollback_update() -> Result<String, String> {
    run_sys_helper(&["update-rollback"])
}
