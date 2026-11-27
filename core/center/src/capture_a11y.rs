//! Backend wrappers for UniCore OS v0.5.0 Daily-Driver Essentials:
//! - Wayland Screenshot & Screen Recorder (`unicore-capture`: grim + slurp + wf-recorder)
//! - Notification & Do Not Disturb control (`unicore-notify`: makoctl)
//! - Rootless Network Drives & Shares (`unicore-shares`: SMB/SFTP/WebDAV/NFS via gio/sshfs)
//! - Lightweight Accessibility Pack (`unicore-a11y`: wvkbd on-screen keyboard + espeak-ng TTS)

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub const CAPTURE_HELPER_BIN: &str = "/usr/libexec/unicore-capture";
pub const NOTIFY_HELPER_BIN: &str = "/usr/libexec/unicore-notify";
pub const SHARES_HELPER_BIN: &str = "/usr/libexec/unicore-shares";
pub const A11Y_HELPER_BIN: &str = "/usr/libexec/unicore-a11y";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureOverview {
    pub recording: bool,
    pub pid: String,
    pub active_file: String,
    pub shots_dir: String,
    pub videos_dir: String,
    pub wf_recorder: bool,
}

impl Default for CaptureOverview {
    fn default() -> Self {
        Self {
            recording: false,
            pid: String::new(),
            active_file: String::new(),
            shots_dir: "/data/home/Pictures/Screenshots".to_string(),
            videos_dir: "/data/home/Videos/Recordings".to_string(),
            wf_recorder: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotifyOverview {
    pub mako_running: bool,
    pub dnd: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkShare {
    pub id: String,
    pub label: String,
    pub uri: String,
    pub username: String,
    pub mounted: bool,
    pub mount_path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct A11yOverview {
    pub osk_available: bool,
    pub osk_running: bool,
    pub tts_available: bool,
}

pub fn parse_capture_status(raw: &str) -> CaptureOverview {
    let mut ov = CaptureOverview::default();
    for line in raw.lines() {
        let Some((k, v)) = line.trim().split_once('=') else {
            continue;
        };
        let val = v.trim();
        match k.trim() {
            "recording" => ov.recording = val == "1" || val.eq_ignore_ascii_case("true"),
            "pid" => ov.pid = val.to_string(),
            "file" => ov.active_file = val.to_string(),
            "shots_dir" if !val.is_empty() => ov.shots_dir = val.to_string(),
            "videos_dir" if !val.is_empty() => ov.videos_dir = val.to_string(),
            "wf_recorder" => ov.wf_recorder = val == "1" || val.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
    ov
}

pub fn read_capture_overview() -> CaptureOverview {
    if !cfg!(target_os = "linux") || !Path::new(CAPTURE_HELPER_BIN).is_file() {
        return CaptureOverview::default();
    }
    Command::new(CAPTURE_HELPER_BIN)
        .arg("status")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_capture_status(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

pub fn run_capture_shot(region: bool, delay_sec: i32) -> Result<String, String> {
    if !cfg!(target_os = "linux") || !Path::new(CAPTURE_HELPER_BIN).is_file() {
        return Err("Screen capture is only available inside UniCore OS Wayland session.".to_string());
    }
    let mode = if region { "region" } else { "full" };
    let delay = match delay_sec {
        1 | 2 | 3 | 5 | 10 => delay_sec.to_string(),
        _ => "0".to_string(),
    };
    let out = Command::new(CAPTURE_HELPER_BIN)
        .args(["shot", mode, &delay])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to invoke unicore-capture: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { "Screenshot failed".to_string() } else { err });
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(if path.is_empty() {
        "Screenshot cancelled".to_string()
    } else {
        format!("Saved & copied screenshot: {path}")
    })
}

pub fn start_screen_recording(region: bool, with_audio: bool) -> Result<String, String> {
    if !cfg!(target_os = "linux") || !Path::new(CAPTURE_HELPER_BIN).is_file() {
        return Err("Screen recording is only available inside UniCore OS Wayland session.".to_string());
    }
    let mode = if region { "region" } else { "full" };
    let audio = if with_audio { "1" } else { "0" };
    let out = Command::new(CAPTURE_HELPER_BIN)
        .args(["record-start", mode, audio])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to start screen recording: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { "Failed to start wf-recorder".to_string() } else { err });
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(if path.is_empty() {
        "Recording cancelled".to_string()
    } else {
        format!("Recording MP4 -> {path}")
    })
}

pub fn stop_screen_recording() -> Result<String, String> {
    if !cfg!(target_os = "linux") || !Path::new(CAPTURE_HELPER_BIN).is_file() {
        return Ok("No active recording".to_string());
    }
    let out = Command::new(CAPTURE_HELPER_BIN)
        .arg("record-stop")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to stop screen recording: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(if stdout.is_empty() {
        "Screen recording saved".to_string()
    } else {
        format!("Saved video: {stdout}")
    })
}

pub fn open_capture_dir(which: &str) -> Result<String, String> {
    let kind = if which == "videos" { "videos" } else { "shots" };
    if !cfg!(target_os = "linux") || !Path::new(CAPTURE_HELPER_BIN).is_file() {
        return Ok(format!("Capture folder ({kind})"));
    }
    let out = Command::new(CAPTURE_HELPER_BIN)
        .args(["open-dir", kind])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to open capture folder: {e}"))?;
    let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(format!("Opened {dir}"))
}

pub fn parse_notify_status(raw: &str) -> NotifyOverview {
    let mut ov = NotifyOverview::default();
    for line in raw.lines() {
        let Some((k, v)) = line.trim().split_once('=') else {
            continue;
        };
        match k.trim() {
            "mako_running" => ov.mako_running = v.trim() == "1",
            "dnd" => ov.dnd = v.trim() == "1",
            _ => {}
        }
    }
    ov
}

pub fn read_notify_overview() -> NotifyOverview {
    if !cfg!(target_os = "linux") || !Path::new(NOTIFY_HELPER_BIN).is_file() {
        return NotifyOverview::default();
    }
    Command::new(NOTIFY_HELPER_BIN)
        .arg("status")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_notify_status(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

pub fn set_dnd_state(enabled: bool) -> Result<String, String> {
    if !cfg!(target_os = "linux") || !Path::new(NOTIFY_HELPER_BIN).is_file() {
        return Ok(if enabled { "Do Not Disturb enabled" } else { "Do Not Disturb disabled" }.to_string());
    }
    let flag = if enabled { "on" } else { "off" };
    let out = Command::new(NOTIFY_HELPER_BIN)
        .args(["set-dnd", flag])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to toggle DND: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn dismiss_all_notifications() -> Result<String, String> {
    if !cfg!(target_os = "linux") || !Path::new(NOTIFY_HELPER_BIN).is_file() {
        return Ok("Notifications dismissed".to_string());
    }
    let _ = Command::new(NOTIFY_HELPER_BIN)
        .arg("dismiss-all")
        .stdin(Stdio::null())
        .output();
    Ok("Dismissed all notifications".to_string())
}

pub fn restore_last_notification() -> Result<String, String> {
    if !cfg!(target_os = "linux") || !Path::new(NOTIFY_HELPER_BIN).is_file() {
        return Ok("Restored last notification".to_string());
    }
    let _ = Command::new(NOTIFY_HELPER_BIN)
        .arg("restore")
        .stdin(Stdio::null())
        .output();
    Ok("Restored last notification".to_string())
}

pub fn validate_share_uri(raw: &str) -> Result<String, String> {
    let clean = raw.trim();
    const ALLOWED: [&str; 5] = ["smb://", "sftp://", "dav://", "davs://", "nfs://"];
    if !ALLOWED.iter().any(|p| clean.starts_with(p)) || clean.len() < 9 {
        return Err("Network Drive URI must start with smb://, sftp://, dav://, davs://, or nfs://".to_string());
    }
    if clean.chars().any(|c| {
        c.is_whitespace()
            || c.is_control()
            || matches!(c, '"' | '\'' | '$' | '`' | '\\' | ';' | '<' | '>' | '|' | '&')
    }) {
        return Err("Network Drive URI contains invalid shell characters.".to_string());
    }
    Ok(clean.to_string())
}

pub fn validate_share_id(raw: &str) -> Result<String, String> {
    let clean = raw.trim();
    if clean.is_empty()
        || clean.len() > 32
        || !clean.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_'))
    {
        return Err(format!("Invalid network share ID '{raw}'"));
    }
    Ok(clean.to_string())
}

pub fn parse_shares_tsv(raw: &str) -> Vec<NetworkShare> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r');
        if trimmed.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = trimmed.split('\t').collect();
        if cols.len() < 3 {
            continue;
        }
        let id = cols[0].trim().to_string();
        if validate_share_id(&id).is_err() {
            continue;
        }
        out.push(NetworkShare {
            id,
            label: cols[1].trim().to_string(),
            uri: cols[2].trim().to_string(),
            username: cols.get(3).copied().unwrap_or("").trim().to_string(),
            mounted: cols.get(4).copied().unwrap_or("0").trim() == "1",
            mount_path: cols.get(5).copied().unwrap_or("").trim().to_string(),
        });
    }
    out
}

pub fn list_network_shares() -> Vec<NetworkShare> {
    if !cfg!(target_os = "linux") || !Path::new(SHARES_HELPER_BIN).is_file() {
        return Vec::new();
    }
    Command::new(SHARES_HELPER_BIN)
        .arg("list")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_shares_tsv(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

pub fn add_network_share(
    label: &str,
    uri: &str,
    username: &str,
    password: &str,
) -> Result<String, String> {
    let clean_label = label.trim();
    if clean_label.is_empty() {
        return Err("Enter a friendly name for this network drive (e.g. Home NAS).".to_string());
    }
    let clean_uri = validate_share_uri(uri)?;
    if !cfg!(target_os = "linux") || !Path::new(SHARES_HELPER_BIN).is_file() {
        return Ok(format!("Saved network drive '{clean_label}' ({clean_uri})"));
    }
    let mut child = Command::new(SHARES_HELPER_BIN)
        .args(["add", clean_label, &clean_uri, username.trim()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to save network drive: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = writeln!(stdin, "{}", password.trim());
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("Failed waiting for unicore-shares: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn share_action(action: &str, id: &str) -> Result<String, String> {
    if !matches!(action, "mount" | "unmount" | "open" | "remove") {
        return Err(format!("Unsupported share action '{action}'"));
    }
    let clean_id = validate_share_id(id)?;
    if !cfg!(target_os = "linux") || !Path::new(SHARES_HELPER_BIN).is_file() {
        return Ok(format!("Share '{clean_id}' {action} completed"));
    }
    let out = Command::new(SHARES_HELPER_BIN)
        .args([action, &clean_id])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to invoke unicore-shares: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            format!("Network share {action} failed")
        } else {
            err
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn parse_a11y_status(raw: &str) -> A11yOverview {
    let mut ov = A11yOverview::default();
    for line in raw.lines() {
        let Some((k, v)) = line.trim().split_once('=') else {
            continue;
        };
        match k.trim() {
            "osk_available" => ov.osk_available = v.trim() == "1",
            "osk_running" => ov.osk_running = v.trim() == "1",
            "tts_available" => ov.tts_available = v.trim() == "1",
            _ => {}
        }
    }
    ov
}

pub fn read_a11y_overview() -> A11yOverview {
    if !cfg!(target_os = "linux") || !Path::new(A11Y_HELPER_BIN).is_file() {
        return A11yOverview::default();
    }
    Command::new(A11Y_HELPER_BIN)
        .arg("status")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_a11y_status(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

pub fn toggle_on_screen_keyboard() -> Result<String, String> {
    if !cfg!(target_os = "linux") || !Path::new(A11Y_HELPER_BIN).is_file() {
        return Ok("On-screen touch keyboard available inside UniCore OS".to_string());
    }
    let out = Command::new(A11Y_HELPER_BIN)
        .arg("osk-toggle")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to toggle on-screen keyboard: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn speak_text_offline(lang_code: &str, text: &str) -> Result<String, String> {
    if !cfg!(target_os = "linux") || !Path::new(A11Y_HELPER_BIN).is_file() {
        return Ok("Offline speech synthesis available inside UniCore OS".to_string());
    }
    let lang = if lang_code.eq_ignore_ascii_case("ms") || lang_code.eq_ignore_ascii_case("bm") {
        "ms"
    } else {
        "en"
    };
    let mut cmd = Command::new(A11Y_HELPER_BIN);
    cmd.args(["speak", lang]);
    if !text.trim().is_empty() {
        cmd.arg(text.trim());
    }
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to speak text: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
