//! UniCore OS v0.8.0 Ecosystem, Phone Link, Cloud Drives, Windows Migration,
//! and Hardware & Driver Manager helpers for `unicore-center`.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub const PHONE_HELPER_BIN: &str = "/usr/libexec/unicore-phone";
pub const CLOUD_HELPER_BIN: &str = "/usr/libexec/unicore-cloud";
const SYS_HELPER_BIN: &str = "/usr/libexec/unicore-sys-helper";

fn run_sys_helper(args: &[&str]) -> Result<String, String> {
    if !cfg!(target_os = "linux") || !Path::new(SYS_HELPER_BIN).is_file() {
        return Err("System helper is only available on UniCore OS Linux.".to_string());
    }
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
            format!("System helper exited with status {}", out.status)
        } else {
            detail
        });
    }
    Ok(stdout)
}

// ============================================================================
// 1. Phone Link (KDE Connect + scrcpy + ADB)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KdeDevice {
    pub id: String,
    pub name: String,
    pub paired: bool,
    pub reachable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PhoneOverview {
    pub kdeconnect_installed: bool,
    pub kdeconnect_running: bool,
    pub kdeconnect_autostart: bool,
    pub scrcpy_installed: bool,
    pub adb_installed: bool,
    pub kde_devices: Vec<KdeDevice>,
    pub adb_devices: Vec<String>,
}

pub fn validate_phone_id(raw: &str) -> Result<String, String> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 96 {
        return Err("Invalid device ID length.".to_string());
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
    {
        return Err("Device ID contains unsafe characters.".to_string());
    }
    Ok(s.to_string())
}

pub fn validate_adb_wifi_target(raw: &str) -> Result<String, String> {
    let s = raw.trim();
    let Some((ip, port_str)) = s.split_once(':') else {
        return Err("Use IP:PORT format (e.g. 192.168.1.45:5555).".to_string());
    };
    let octets: Vec<&str> = ip.split('.').collect();
    if octets.len() != 4 || octets.iter().any(|o| o.parse::<u8>().is_err()) {
        return Err("Invalid IPv4 address for Wi-Fi ADB.".to_string());
    }
    let port: u16 = port_str
        .parse()
        .map_err(|_| "Invalid port number for Wi-Fi ADB.".to_string())?;
    if port < 1024 {
        return Err("Wi-Fi ADB port must be >= 1024.".to_string());
    }
    Ok(s.to_string())
}

pub fn parse_phone_status(raw: &str) -> PhoneOverview {
    let mut out = PhoneOverview::default();
    for line in raw.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let val = v.trim();
        match k.trim() {
            "kdeconnect_installed" => out.kdeconnect_installed = val == "1",
            "kdeconnect_running" => out.kdeconnect_running = val == "1",
            "kdeconnect_autostart" => out.kdeconnect_autostart = val == "1",
            "scrcpy_installed" => out.scrcpy_installed = val == "1",
            "adb_installed" => out.adb_installed = val == "1",
            "adb_devices" => {
                out.adb_devices = val
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            "kde_devices" => {
                out.kde_devices.clear();
                for item in val.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                    let parts: Vec<&str> = item.split('|').collect();
                    if parts.len() >= 4 {
                        out.kde_devices.push(KdeDevice {
                            id: parts[0].trim().to_string(),
                            name: parts[1].trim().to_string(),
                            paired: parts[2].trim() == "1",
                            reachable: parts[3].trim() == "1",
                        });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn run_user_helper(bin: &str, args: &[&str], stdin_data: Option<&str>) -> Result<String, String> {
    if !cfg!(target_os = "linux") || !Path::new(bin).is_file() {
        return Err(format!("Helper '{bin}' is only available on UniCore OS Linux."));
    }
    let mut cmd = Command::new(bin);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    if stdin_data.is_some() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }
    let mut child = cmd.spawn().map_err(|e| format!("Failed to start {bin}: {e}"))?;
    if let Some(payload) = stdin_data {
        if let Some(mut sin) = child.stdin.take() {
            let _ = writeln!(sin, "{payload}");
        }
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("Failed waiting on {bin}: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if out.status.success() {
        Ok(stdout)
    } else {
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

pub fn read_phone_overview() -> PhoneOverview {
    run_user_helper(PHONE_HELPER_BIN, &["status"], None)
        .map(|s| parse_phone_status(&s))
        .unwrap_or_default()
}

pub fn phone_service_action(action: &str) -> Result<String, String> {
    match action {
        "start" | "stop" | "refresh" | "open-kde-settings" => {
            run_user_helper(PHONE_HELPER_BIN, &[action], None)
        }
        _ => Err(format!("Unsupported phone action: {action}")),
    }
}

pub fn phone_set_autostart(enabled: bool) -> Result<String, String> {
    run_user_helper(
        PHONE_HELPER_BIN,
        &["set-autostart", if enabled { "1" } else { "0" }],
        None,
    )
}

pub fn phone_device_action(action: &str, device_id: &str) -> Result<String, String> {
    let clean = validate_phone_id(device_id)?;
    match action {
        "pair" | "unpair" | "ping" | "ring" => {
            run_user_helper(PHONE_HELPER_BIN, &[action, &clean], None)
        }
        _ => Err(format!("Unsupported device action: {action}")),
    }
}

pub fn phone_send_file(device_id: &str, file_path: &str) -> Result<String, String> {
    let clean = validate_phone_id(device_id)?;
    let fp = file_path.trim();
    if fp.is_empty() || !fp.starts_with('/') || fp.contains("..") {
        return Err("Select a valid local file path to send to your phone.".to_string());
    }
    run_user_helper(PHONE_HELPER_BIN, &["send-file", &clean, fp], None)
}

pub fn phone_mirror_android(serial: &str, screen_off: bool) -> Result<String, String> {
    let mut args: Vec<String> = vec!["mirror".to_string()];
    if !serial.trim().is_empty() {
        args.push(validate_phone_id(serial)?);
    }
    if screen_off {
        args.push("--screen-off".to_string());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_user_helper(PHONE_HELPER_BIN, &refs, None)
}

pub fn phone_connect_adb_wifi(target: &str) -> Result<String, String> {
    let clean = validate_adb_wifi_target(target)?;
    run_user_helper(PHONE_HELPER_BIN, &["adb-wifi", &clean], None)
}

// ============================================================================
// 2. Cloud Drives (rclone mount VFS -> ~/Cloud/<Name>)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudRemote {
    pub name: String,
    pub provider_type: String,
    pub mounted: bool,
    pub autostart: bool,
    pub mount_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudOverview {
    pub rclone_installed: bool,
    pub mount_base: String,
    pub remotes: Vec<CloudRemote>,
}

impl Default for CloudOverview {
    fn default() -> Self {
        Self {
            rclone_installed: false,
            mount_base: "/home/unicore/Cloud".to_string(),
            remotes: Vec::new(),
        }
    }
}

pub fn validate_cloud_name(raw: &str) -> Result<String, String> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 32 {
        return Err("Cloud drive name must be 1-32 characters.".to_string());
    }
    let mut chars = s.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphanumeric());
    if !first_ok || !s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')) {
        return Err("Use letters, digits, '-' or '_' only (e.g. GoogleDrive).".to_string());
    }
    Ok(s.to_string())
}

pub fn cloud_provider_from_index(idx: i32) -> &'static str {
    match idx {
        1 => "onedrive",
        2 => "dropbox",
        3 => "mega",
        4 => "nextcloud",
        _ => "gdrive",
    }
}

pub fn parse_cloud_status(raw: &str) -> CloudOverview {
    let mut out = CloudOverview::default();
    for line in raw.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let val = v.trim();
        match k.trim() {
            "rclone_installed" => out.rclone_installed = val == "1",
            "mount_base" => {
                if !val.is_empty() {
                    out.mount_base = val.to_string();
                }
            }
            "remotes" => {
                out.remotes.clear();
                for item in val.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                    let parts: Vec<&str> = item.split('|').collect();
                    if parts.len() >= 5 {
                        out.remotes.push(CloudRemote {
                            name: parts[0].trim().to_string(),
                            provider_type: parts[1].trim().to_string(),
                            mounted: parts[2].trim() == "1",
                            autostart: parts[3].trim() == "1",
                            mount_dir: parts[4].trim().to_string(),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

pub fn read_cloud_overview() -> CloudOverview {
    run_user_helper(CLOUD_HELPER_BIN, &["status"], None)
        .map(|s| parse_cloud_status(&s))
        .unwrap_or_default()
}

pub fn cloud_connect(
    provider: &str,
    name: &str,
    url: &str,
    user: &str,
    pass: &str,
) -> Result<String, String> {
    let clean_name = validate_cloud_name(name)?;
    match provider {
        "gdrive" | "onedrive" | "dropbox" => {
            run_user_helper(CLOUD_HELPER_BIN, &["connect", provider, &clean_name], None)
        }
        "mega" => {
            let u = user.trim();
            if u.is_empty() || pass.is_empty() {
                return Err("MEGA requires your email and password.".to_string());
            }
            run_user_helper(CLOUD_HELPER_BIN, &["connect", "mega", &clean_name, u], Some(pass))
        }
        "nextcloud" | "webdav" => {
            let clean_url = url.trim();
            let u = user.trim();
            if !clean_url.starts_with("https://") && !clean_url.starts_with("http://") {
                return Err("Nextcloud/WebDAV URL must start with https:// or http://".to_string());
            }
            if u.is_empty() || pass.is_empty() {
                return Err("Nextcloud/WebDAV requires username and app password.".to_string());
            }
            run_user_helper(
                CLOUD_HELPER_BIN,
                &["connect", "nextcloud", &clean_name, clean_url, u],
                Some(pass),
            )
        }
        _ => Err(format!("Unsupported cloud provider: {provider}")),
    }
}

pub fn cloud_remote_action(action: &str, name: &str) -> Result<String, String> {
    let clean = validate_cloud_name(name)?;
    match action {
        "mount" | "unmount" | "remove" => run_user_helper(CLOUD_HELPER_BIN, &[action, &clean], None),
        _ => Err(format!("Unsupported cloud action: {action}")),
    }
}

pub fn cloud_set_autostart(name: &str, enabled: bool) -> Result<String, String> {
    let clean = validate_cloud_name(name)?;
    run_user_helper(
        CLOUD_HELPER_BIN,
        &["set-autostart", &clean, if enabled { "1" } else { "0" }],
        None,
    )
}

// ============================================================================
// 3. Windows Migration Wizard (Read-Only NTFS + Program Files -> Store Matcher)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsPartition {
    pub device: String,
    pub label: String,
    pub size: String,
    pub users: Vec<String>,
    pub free_data_mb: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowsInspectInfo {
    pub device: String,
    pub users: Vec<String>,
    pub free_data_mb: u64,
    pub fonts_count: u32,
    pub bookmarks_found: Vec<String>,
    pub program_dirs: Vec<String>,
    pub store_matches: Vec<StoreMatchSuggestion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreMatchSuggestion {
    pub windows_app: String,
    pub store_id: String,
    pub store_label: String,
}

pub fn validate_win_partition(raw: &str) -> Result<String, String> {
    let s = raw.trim();
    if !s.starts_with("/dev/") || s.contains("..") || s.len() > 32 {
        return Err("Invalid Windows partition path.".to_string());
    }
    let tail = &s[5..];
    if !tail.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("Partition name contains invalid characters.".to_string());
    }
    Ok(s.to_string())
}

pub fn validate_win_username(raw: &str) -> Result<String, String> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 64 || s.contains("..") || s.contains('/') || s.contains('\\') {
        return Err("Invalid Windows user profile name.".to_string());
    }
    Ok(s.to_string())
}

pub fn parse_migrate_scan_tsv(raw: &str) -> Vec<WindowsPartition> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 4 {
            continue;
        }
        let dev = cols[0].trim();
        if !dev.starts_with("/dev/") {
            continue;
        }
        let users = cols[3]
            .split(',')
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .map(str::to_string)
            .collect();
        let free_data_mb = cols
            .get(4)
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0);
        out.push(WindowsPartition {
            device: dev.to_string(),
            label: cols[1].trim().to_string(),
            size: cols[2].trim().to_string(),
            users,
            free_data_mb,
        });
    }
    out
}

pub fn match_windows_programs_to_store(program_dirs: &[String]) -> Vec<StoreMatchSuggestion> {
    const RULES: &[(&[&str], &str, &str)] = &[
        (&["steam"], "steam", "Steam (Gaming)"),
        (&["epic games", "gog galaxy", "heroic"], "heroic", "Heroic Games Launcher"),
        (&["discord"], "vesktop", "Vesktop (Discord)"),
        (&["telegram"], "telegram", "Telegram Desktop"),
        (&["videolan", "vlc"], "vlc", "VLC Media Player"),
        (&["7-zip", "7zip", "winrar"], "7zip", "7-Zip File Manager"),
        (&["notepad++"], "notepadpp", "Notepad++"),
        (&["microsoft office", "libreoffice", "wps office"], "libreoffice", "LibreOffice Suite"),
        (&["gimp", "photoshop", "adobe photoshop"], "gimp", "GIMP Image Editor"),
        (&["obs-studio", "obs studio"], "obs-studio", "OBS Studio"),
        (&["blender"], "blender", "Blender 3D"),
        (&["audacity"], "audacity", "Audacity Audio Editor"),
        (&["inkscape"], "inkscape", "Inkscape Vector"),
        (&["visual studio code", "vscode", "vscodium"], "vscodium", "VSCodium Editor"),
        (&["keepass", "keepassxc"], "keepassxc", "KeePassXC Vault"),
        (&["sumatrapdf", "adobe acrobat", "acrobat reader"], "sumatrapdf", "SumatraPDF Reader"),
        (&["thunderbird"], "thunderbird", "Mozilla Thunderbird"),
        (&["kdenlive"], "kdenlive", "Kdenlive Video Editor"),
        (&["retroarch"], "retroarch", "RetroArch Emulators"),
        (&["localsend"], "localsend", "LocalSend Wi-Fi Share"),
        (&["minecraft", "prism"], "prism-launcher", "Prism Launcher"),
    ];
    let mut out = Vec::new();
    let mut seen_store = std::collections::HashSet::new();
    for dir in program_dirs {
        let lower = dir.to_ascii_lowercase();
        for (needles, store_id, label) in RULES {
            if needles.iter().any(|n| lower.contains(n)) && seen_store.insert(*store_id) {
                out.push(StoreMatchSuggestion {
                    windows_app: dir.clone(),
                    store_id: (*store_id).to_string(),
                    store_label: (*label).to_string(),
                });
            }
        }
    }
    out
}

pub fn parse_migrate_inspect(raw: &str) -> WindowsInspectInfo {
    let mut info = WindowsInspectInfo::default();
    for line in raw.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let val = v.trim();
        match k.trim() {
            "device" => info.device = val.to_string(),
            "users" => {
                info.users = val
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            "free_data_mb" => info.free_data_mb = val.parse().unwrap_or(0),
            "fonts_count" => info.fonts_count = val.parse().unwrap_or(0),
            "bookmarks_found" => {
                info.bookmarks_found = val
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            "program_dirs" => {
                info.program_dirs = val
                    .split('|')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            _ => {}
        }
    }
    info.store_matches = match_windows_programs_to_store(&info.program_dirs);
    info
}

pub fn scan_windows_partitions() -> Result<Vec<WindowsPartition>, String> {
    if !cfg!(target_os = "linux") {
        return Ok(Vec::new());
    }
    let raw = run_sys_helper(&["migrate-scan"])?;
    Ok(parse_migrate_scan_tsv(&raw))
}

pub fn inspect_windows_partition(dev: &str) -> Result<WindowsInspectInfo, String> {
    let clean = validate_win_partition(dev)?;
    let raw = run_sys_helper(&["migrate-inspect", &clean])?;
    Ok(parse_migrate_inspect(&raw))
}

#[allow(clippy::too_many_arguments)]
pub fn build_migrate_flags_csv(
    docs: bool,
    pics: bool,
    desktop: bool,
    music: bool,
    videos: bool,
    saves: bool,
    bookmarks: bool,
    fonts: bool,
) -> String {
    let mut flags = Vec::new();
    if docs { flags.push("docs"); }
    if pics { flags.push("pics"); }
    if desktop { flags.push("desktop"); }
    if music { flags.push("music"); }
    if videos { flags.push("videos"); }
    if saves { flags.push("saves"); }
    if bookmarks { flags.push("bookmarks"); }
    if fonts { flags.push("fonts"); }
    flags.join(",")
}

pub fn run_windows_migration(dev: &str, win_user: &str, flags_csv: &str) -> Result<String, String> {
    let clean_dev = validate_win_partition(dev)?;
    let clean_user = validate_win_username(win_user)?;
    if flags_csv.trim().is_empty() {
        return Err("Select at least one category to import from Windows.".to_string());
    }
    run_sys_helper(&["migrate-import", &clean_dev, &clean_user, flags_csv.trim()])
}

// ============================================================================
// 4. Hardware & Driver Health Manager + On-Demand LVFS Firmware (fwupd)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareOverview {
    pub gpu_model: String,
    pub gpu_driver: String,
    pub vulkan_ready: bool,
    pub vulkan_device: String,
    pub wifi_model: String,
    pub wifi_driver: String,
    pub audio_model: String,
    pub usb_count: u32,
    pub usb_summary: String,
    pub missing_firmware: String,
    pub gpu_mode: String,       // "auto" | "hw" | "safe"
    pub wifi_powersave: String, // "on" | "off"
    pub fwupd_available: bool,
}

impl Default for HardwareOverview {
    fn default() -> Self {
        Self {
            gpu_model: "Not queried".to_string(),
            gpu_driver: "-".to_string(),
            vulkan_ready: false,
            vulkan_device: "-".to_string(),
            wifi_model: "-".to_string(),
            wifi_driver: "-".to_string(),
            audio_model: "PipeWire".to_string(),
            usb_count: 0,
            usb_summary: "-".to_string(),
            missing_firmware: "none".to_string(),
            gpu_mode: "auto".to_string(),
            wifi_powersave: "on".to_string(),
            fwupd_available: false,
        }
    }
}

pub fn gpu_mode_to_index(mode: &str) -> i32 {
    match mode.trim() {
        "hw" => 1,
        "safe" => 2,
        _ => 0,
    }
}

pub fn gpu_mode_from_index(idx: i32) -> &'static str {
    match idx {
        1 => "hw",
        2 => "safe",
        _ => "auto",
    }
}

pub fn validate_gpu_mode(raw: &str) -> Result<String, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "auto" | "hw" | "safe" => Ok(lower),
        _ => Err(format!("Unsupported GPU mode: {raw}")),
    }
}

pub fn parse_hw_status(raw: &str) -> HardwareOverview {
    let mut out = HardwareOverview::default();
    for line in raw.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let val = v.trim();
        match k.trim() {
            "gpu_model" => out.gpu_model = val.to_string(),
            "gpu_driver" => out.gpu_driver = val.to_string(),
            "vulkan_ready" => out.vulkan_ready = val == "1",
            "vulkan_device" => out.vulkan_device = val.to_string(),
            "wifi_model" => out.wifi_model = val.to_string(),
            "wifi_driver" => out.wifi_driver = val.to_string(),
            "audio_model" => out.audio_model = val.to_string(),
            "usb_count" => out.usb_count = val.parse().unwrap_or(0),
            "usb_summary" => out.usb_summary = val.to_string(),
            "missing_firmware" => out.missing_firmware = val.to_string(),
            "gpu_mode" => out.gpu_mode = val.to_string(),
            "wifi_powersave" => out.wifi_powersave = val.to_string(),
            "fwupd_available" => out.fwupd_available = val == "1",
            _ => {}
        }
    }
    out
}

pub fn read_hw_overview() -> HardwareOverview {
    if !cfg!(target_os = "linux") {
        return HardwareOverview::default();
    }
    run_sys_helper(&["hw-status"])
        .map(|s| parse_hw_status(&s))
        .unwrap_or_default()
}

pub fn hw_set_gpu_mode(mode: &str) -> Result<String, String> {
    let clean = validate_gpu_mode(mode)?;
    run_sys_helper(&["hw-set-gpu", &clean])
}

pub fn hw_set_wifi_powersave(enabled: bool) -> Result<String, String> {
    run_sys_helper(&["hw-set-wifi-ps", if enabled { "on" } else { "off" }])
}

pub fn hw_reset_audio_bt() -> Result<String, String> {
    run_sys_helper(&["hw-reset-audio-bt"])
}

pub fn hw_fw_check() -> Result<String, String> {
    run_sys_helper(&["hw-fw-check"])
}

pub fn hw_fw_update() -> Result<String, String> {
    run_sys_helper(&["hw-fw-update"])
}
