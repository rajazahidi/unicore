//! Bare-metal disk installer backend (`unicore-install`) and boot mode telemetry.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::settings::SYS_HELPER_BIN;

const INSTALL_BIN: &str = "/usr/sbin/unicore-install";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootState {
    pub mode: String,
    pub boot_media: String,
    pub active_slot: String,
    pub data_encrypted: bool,
}

impl Default for BootState {
    fn default() -> Self {
        Self {
            mode: "unknown".to_string(),
            boot_media: String::new(),
            active_slot: String::new(),
            data_encrypted: false,
        }
    }
}

impl BootState {
    pub fn is_live(&self) -> bool {
        self.mode == "live"
    }

    pub fn display_summary(&self) -> String {
        let enc = if self.data_encrypted { " [LUKS2]" } else { "" };
        match self.mode.as_str() {
            "live" => {
                if self.boot_media.is_empty() {
                    "Live RAM Mode (Installable to Disk)".to_string()
                } else {
                    format!("Live RAM Mode from {} (Installable to Disk)", self.boot_media)
                }
            }
            "installed" => {
                if self.active_slot.is_empty() {
                    format!("Installed A/B System{enc}")
                } else {
                    format!("Installed A/B System (Slot: {}){enc}", self.active_slot)
                }
            }
            "qemu-direct" => format!("QEMU Direct SquashFS Mode{enc}"),
            _ => "Standard Boot Mode".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallDisk {
    pub device: String,
    pub name: String,
    pub size_gb: String,
    pub model: String,
    pub eligible: bool,
    pub reason: String,
}

fn is_clean_disk_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// Parse `/mnt/unicore-boot.env` key/value pairs into `BootState`.
pub fn parse_boot_env(raw: &str) -> BootState {
    let mut state = BootState::default();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, val_raw)) = trimmed.split_once('=') else {
            continue;
        };
        let val = val_raw.trim().trim_matches('"').trim_matches('\'');
        match key.trim() {
            "UNICORE_BOOT_MODE" if !val.is_empty() => state.mode = val.to_string(),
            "UNICORE_BOOT_MEDIA" => state.boot_media = val.to_string(),
            "UNICORE_BOOT_SLOT" => state.active_slot = val.to_string(),
            "UNICORE_DATA_ENCRYPTED" => {
                state.data_encrypted = val == "1" || val.eq_ignore_ascii_case("true");
            }
            _ => {}
        }
    }
    state
}

/// Parse `/usr/sbin/unicore-install --list-tsv` lines:
/// `device\tname\tsize_gb\tmodel\teligible\treason`.
pub fn parse_install_disks(tsv: &str) -> Vec<InstallDisk> {
    let mut disks = Vec::new();
    for line in tsv.lines() {
        let trimmed = line.trim_end_matches('\r');
        if trimmed.trim().is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = trimmed.split('\t').collect();
        if cols.len() < 5 {
            continue;
        }
        let device = cols[0].trim().to_string();
        let name = cols[1].trim().to_string();
        if device.is_empty() || !is_clean_disk_name(&name) {
            continue;
        }
        let size_gb = cols[2].trim().to_string();
        let model = cols[3].trim().to_string();
        let eligible_raw = cols[4].trim();
        let eligible = eligible_raw == "1" || eligible_raw.eq_ignore_ascii_case("true");
        let reason = cols.get(5).copied().unwrap_or("").trim().to_string();
        disks.push(InstallDisk {
            device,
            name,
            size_gb,
            model,
            eligible,
            reason,
        });
    }
    disks
}

/// Validate that the user typed the exact selected whole-disk name (e.g. `sda` or `nvme0n1`).
pub fn validate_install_confirmation(selected_name: &str, typed: &str) -> Result<(), String> {
    let expected = selected_name.trim();
    if expected.is_empty() || !is_clean_disk_name(expected) {
        return Err("Invalid target disk identifier".to_string());
    }
    let got = typed.trim();
    if got != expected {
        return Err(format!(
            "Confirmation mismatch: type exact device name '{expected}' to wipe & install"
        ));
    }
    Ok(())
}

pub fn read_boot_state() -> BootState {
    if !cfg!(target_os = "linux") {
        return BootState::default();
    }
    fs::read_to_string("/mnt/unicore-boot.env")
        .map(|s| parse_boot_env(&s))
        .unwrap_or_default()
}

pub fn list_install_disks() -> Vec<InstallDisk> {
    if !cfg!(target_os = "linux") {
        return Vec::new();
    }
    let Ok(out) = Command::new(INSTALL_BIN)
        .arg("--list-tsv")
        .stdin(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    parse_install_disks(&String::from_utf8_lossy(&out.stdout))
}

/// Validate LUKS2 disk encryption passphrase (minimum 8 chars, exact match, no newlines/NUL).
pub fn validate_luks_passphrase(pass: &str, confirm: &str) -> Result<(), String> {
    if pass.len() < 8 {
        return Err("LUKS2 encryption passphrase must be at least 8 characters".to_string());
    }
    if pass.contains('\n') || pass.contains('\r') || pass.contains('\0') {
        return Err("LUKS2 passphrase cannot contain newlines or NUL bytes".to_string());
    }
    if pass != confirm {
        return Err("LUKS2 encryption passphrases do not match".to_string());
    }
    Ok(())
}

fn validate_target_device(device: &str) -> Result<&str, String> {
    let dev_clean = device.trim();
    let Some(name_part) = dev_clean.strip_prefix("/dev/") else {
        return Err(format!("Invalid target device path: '{dev_clean}'"));
    };
    if !is_clean_disk_name(name_part) {
        return Err(format!("Unsafe target device name: '{dev_clean}'"));
    }
    Ok(dev_clean)
}

pub fn run_disk_install(device: &str) -> Result<String, String> {
    let dev_clean = validate_target_device(device)?;

    let out = Command::new("doas")
        .args([SYS_HELPER_BIN, "install-disk", dev_clean])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to launch disk installer: {e}"))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(if detail.is_empty() {
            format!("Disk installation failed with status {}", out.status)
        } else {
            detail
        });
    }

    Ok(format!(
        "UniCore OS installed cleanly to {dev_clean} (Hybrid BIOS+UEFI GPT A/B). Remove Live USB/ISO and reboot."
    ))
}

pub fn run_disk_install_encrypted(device: &str, passphrase: &str) -> Result<String, String> {
    let dev_clean = validate_target_device(device)?;
    validate_luks_passphrase(passphrase, passphrase)?;

    let mut child = Command::new("doas")
        .args([SYS_HELPER_BIN, "install-disk-encrypted", dev_clean])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to launch encrypted disk installer: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        writeln!(stdin, "{passphrase}")
            .map_err(|e| format!("Failed to pass LUKS passphrase to helper: {e}"))?;
    }

    let out = child
        .wait_with_output()
        .map_err(|e| format!("Failed waiting for encrypted disk installer: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(if detail.is_empty() {
            format!("Encrypted disk installation failed with status {}", out.status)
        } else {
            detail
        });
    }

    let _ = Path::new(dev_clean);
    Ok(format!(
        "UniCore OS installed cleanly to {dev_clean} with LUKS2 encrypted /data. Remove Live USB/ISO and reboot."
    ))
}
