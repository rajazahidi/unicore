//! Daily laptop/PC hardware settings engine: NetworkManager, BlueZ, PipeWire audio,
//! Wayland display (`wlr-randr`), backlight (`brightnessctl`), Night Light (`wlsunset`),
//! on-demand CUPS printing, and restricted `doas` power control.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub(crate) const SYS_HELPER_BIN: &str = "/usr/libexec/unicore-sys-helper";
pub const NIGHTLIGHT_HELPER_BIN: &str = "/usr/libexec/unicore-nightlight";
pub const PERSONALIZE_HELPER_BIN: &str = "/usr/libexec/unicore-personalize";
pub const DEFAULT_WALLPAPER_PATH: &str = "/usr/share/unicore/wallpaper.png";
pub const WALLPAPER_PRESETS: [(&str, &str); 3] = [
    ("Midnight Blue (Default)", "/usr/share/unicore/wallpaper.png"),
    ("Aurora Violet", "/usr/share/unicore/wallpaper-aurora.png"),
    ("Daylight Slate", "/usr/share/unicore/wallpaper-light.png"),
];
pub const KB_LAYOUTS: [(&str, &str); 6] = [
    ("us", "US English (us)"),
    ("my", "Malay / QWERTY (my)"),
    ("gb", "UK English (gb)"),
    ("de", "German (de)"),
    ("fr", "French (fr)"),
    ("jp", "Japanese (jp)"),
];
pub const DESKTOP_LAYOUTS: [(&str, &str); 3] = [
    ("win11", "Windows 11 Modern (Centered Floating)"),
    ("win10", "Windows 10 / Zorin Classic (Left Bar)"),
    ("macos", "macOS / Zorin Top Dock"),
];
pub const ACCENT_COLORS: [(&str, &str); 5] = [
    ("blue", "Sky Blue (Default)"),
    ("purple", "Indigo Purple"),
    ("green", "Emerald Green"),
    ("amber", "Warm Amber"),
    ("rose", "Coral Rose"),
];
const DEFAULT_CMD_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiNetwork {
    pub in_use: bool,
    pub ssid: String,
    pub signal: i32,
    pub security: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkOverview {
    pub nm_available: bool,
    pub wifi_enabled: bool,
    pub summary: String,
    pub networks: Vec<WifiNetwork>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothDevice {
    pub mac: String,
    pub name: String,
    pub connected: bool,
    pub paired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothOverview {
    pub available: bool,
    pub service_active: bool,
    pub powered: bool,
    pub summary: String,
    pub devices: Vec<BluetoothDevice>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSink {
    pub name: String,
    pub label: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioCardProfile {
    pub card_name: String,
    pub profile_name: String,
    pub label: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioOverview {
    pub available: bool,
    pub volume_pct: i32,
    pub muted: bool,
    pub mic_muted: bool,
    pub sinks: Vec<AudioSink>,
    pub card_profiles: Vec<AudioCardProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayOutput {
    pub name: String,
    pub modes: Vec<String>,
    pub current_mode: String,
    pub scale: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayOverview {
    pub outputs: Vec<DisplayOutput>,
    pub backlight_available: bool,
    pub brightness_pct: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NightLightConfig {
    pub enabled: bool,
    pub temp_k: i32,
}

impl Default for NightLightConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            temp_k: 4000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterInfo {
    pub name: String,
    pub status: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPrinter {
    pub label: String,
    pub uri: String,
    pub suggested_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintersOverview {
    pub available: bool,
    pub service_active: bool,
    pub summary: String,
    pub printers: Vec<PrinterInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirewallOverview {
    pub enabled: bool,
    pub active: bool,
    pub profile: String,
    pub allow_ssh: bool,
    pub rules_count: i32,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VpnConnection {
    pub name: String,
    pub vpn_type: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupTarget {
    pub path: String,
    pub label: String,
}

fn run_capture_with_stdin_timeout(
    bin: &str,
    args: &[&str],
    stdin_line: Option<&str>,
    timeout: Duration,
) -> Result<String, String> {
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .stdin(if stdin_line.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("Failed to run {bin}: {e}"))?;

    if let Some(input) = stdin_line {
        if let Some(mut stdin_pipe) = child.stdin.take() {
            let _ = stdin_pipe.write_all(input.as_bytes());
            if !input.ends_with('\n') {
                let _ = stdin_pipe.write_all(b"\n");
            }
        }
    }

    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(ref mut pipe) = out_pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });
    let err_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(ref mut pipe) = err_pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = out_handle.join();
                    let _ = err_handle.join();
                    return Err(format!("{bin} timed out after {}s", timeout.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = out_handle.join();
                let _ = err_handle.join();
                return Err(format!("Failed waiting for {bin}: {e}"));
            }
        }
    };

    let stdout = out_handle.join().unwrap_or_default();
    let stderr_bytes = err_handle.join().unwrap_or_default();
    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("{bin} exited with status {status}")
        } else {
            stderr
        });
    }
    Ok(String::from_utf8_lossy(&stdout).to_string())
}

fn run_capture_timeout(bin: &str, args: &[&str], timeout: Duration) -> Result<String, String> {
    run_capture_with_stdin_timeout(bin, args, None, timeout)
}

fn run_capture(bin: &str, args: &[&str]) -> Result<String, String> {
    run_capture_timeout(bin, args, DEFAULT_CMD_TIMEOUT)
}

fn split_nmcli_escaped_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                cur.push(next);
            } else {
                cur.push('\\');
            }
        } else if ch == ':' {
            fields.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    fields.push(cur);
    fields
}

/// Parse `nmcli -t -f IN-USE,SSID,SIGNAL,SECURITY device wifi list` output.
pub fn parse_nmcli_wifi_list(raw: &str) -> Vec<WifiNetwork> {
    let mut by_ssid: BTreeMap<String, WifiNetwork> = BTreeMap::new();
    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r');
        if trimmed.is_empty() {
            continue;
        }
        let fields = split_nmcli_escaped_fields(trimmed);
        if fields.len() < 4 {
            continue;
        }
        let in_use = fields[0].trim() == "*";
        let ssid = fields[1].trim().to_string();
        if ssid.is_empty() || ssid == "--" {
            continue;
        }
        let signal = fields[2].trim().parse::<i32>().unwrap_or(0).clamp(0, 100);
        let security_raw = fields[3..].join(":");
        let security = if security_raw.trim().is_empty() || security_raw.trim() == "--" {
            "Open".to_string()
        } else {
            security_raw.trim().to_string()
        };

        let replace = match by_ssid.get(&ssid) {
            None => true,
            Some(existing) => (in_use && !existing.in_use) || signal > existing.signal,
        };
        if replace {
            by_ssid.insert(
                ssid.clone(),
                WifiNetwork {
                    in_use,
                    ssid,
                    signal,
                    security,
                },
            );
        }
    }
    let mut list: Vec<WifiNetwork> = by_ssid.into_values().collect();
    list.sort_by(|a, b| {
        b.in_use
            .cmp(&a.in_use)
            .then_with(|| b.signal.cmp(&a.signal))
            .then_with(|| a.ssid.to_lowercase().cmp(&b.ssid.to_lowercase()))
    });
    list
}

/// Parse `bluetoothctl devices` lines (`Device AA:BB:CC:DD:EE:FF Name`).
pub fn parse_bluetooth_devices(raw: &str) -> Vec<BluetoothDevice> {
    let mut devices = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("Device ") else {
            continue;
        };
        let mut parts = rest.splitn(2, ' ');
        let Some(mac) = parts.next().map(str::trim) else {
            continue;
        };
        if mac.len() != 17 || mac.matches(':').count() != 5 {
            continue;
        }
        let name = parts.next().map(str::trim).filter(|s| !s.is_empty()).unwrap_or(mac);
        devices.push(BluetoothDevice {
            mac: mac.to_string(),
            name: name.to_string(),
            connected: false,
            paired: false,
        });
    }
    devices
}

/// Extract integer volume `0..=100` from `pactl get-sink-volume @DEFAULT_SINK@`.
pub fn parse_pactl_volume(raw: &str) -> i32 {
    for token in raw.split_whitespace() {
        if let Some(num) = token.strip_suffix('%') {
            if let Ok(val) = num.parse::<i32>() {
                return val.clamp(0, 100);
            }
        }
    }
    50
}

/// Parse `pactl list short sinks` lines and mark `default_sink`.
pub fn parse_pactl_sinks(raw: &str, default_sink: &str) -> Vec<AudioSink> {
    let mut sinks = Vec::new();
    for line in raw.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 2 {
            continue;
        }
        let name = cols[1].trim();
        if name.is_empty() {
            continue;
        }
        let label = name
            .trim_start_matches("alsa_output.")
            .trim_start_matches("bluez_output.")
            .replace('.', " ");
        sinks.push(AudioSink {
            name: name.to_string(),
            label,
            is_default: name == default_sink.trim(),
        });
    }
    sinks
}

/// Parse `wlr-randr` text output into structured display outputs & modes.
pub fn parse_wlr_randr(raw: &str) -> Vec<DisplayOutput> {
    let mut outputs = Vec::new();
    let mut current: Option<DisplayOutput> = None;
    let mut in_modes = false;

    for line in raw.lines() {
        if !line.starts_with(' ') && !line.starts_with('\t') && !line.trim().is_empty() {
            if let Some(done) = current.take() {
                outputs.push(done);
            }
            let name = line.split_whitespace().next().unwrap_or("Display").to_string();
            current = Some(DisplayOutput {
                name,
                modes: Vec::new(),
                current_mode: String::new(),
                scale: "1.0".to_string(),
            });
            in_modes = false;
            continue;
        }

        let Some(out) = current.as_mut() else {
            continue;
        };
        let trimmed = line.trim();
        if trimmed == "Modes:" {
            in_modes = true;
            continue;
        }
        if trimmed.starts_with("Enabled:")
            || trimmed.starts_with("Position:")
            || trimmed.starts_with("Transform:")
        {
            in_modes = false;
            continue;
        }
        if let Some(scale_val) = trimmed.strip_prefix("Scale:") {
            in_modes = false;
            if let Ok(parsed) = scale_val.trim().parse::<f32>() {
                out.scale = format!("{parsed:.2}")
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_string();
                if !out.scale.contains('.') {
                    out.scale.push_str(".0");
                }
            }
            continue;
        }
        if in_modes {
            // Example: "1280x800 px, 60.000000 Hz (preferred, current)"
            if let Some((res_part, rest)) = trimmed.split_once(" px,") {
                let res = res_part.trim();
                let hz_num = rest
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse::<f32>().ok())
                    .map(|hz| format!("{hz:.0}"))
                    .unwrap_or_else(|| "60".to_string());
                let mode_str = format!("{res}@{hz_num}Hz");
                if !out.modes.contains(&mode_str) {
                    out.modes.push(mode_str.clone());
                }
                if trimmed.contains("current") || out.current_mode.is_empty() {
                    out.current_mode = mode_str;
                }
            }
        }
    }
    if let Some(done) = current.take() {
        outputs.push(done);
    }
    outputs
}

fn is_safe_display_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@'))
}

/// Build an idempotent POSIX shell script to apply & persist `wlr-randr` output settings.
pub fn render_display_apply_script(
    output: &str,
    mode: &str,
    scale: &str,
) -> Result<String, String> {
    if !is_safe_display_token(output)
        || !is_safe_display_token(mode)
        || !is_safe_display_token(scale)
    {
        return Err("Invalid display setting characters".to_string());
    }
    Ok(format!(
        "#!/bin/sh\nwlr-randr --output {output} --mode {mode} --scale {scale}\n"
    ))
}

pub fn read_network_overview() -> NetworkOverview {
    let Ok(general) = run_capture(
        "nmcli",
        &["--color", "no", "-t", "-f", "STATE,CONNECTIVITY", "general", "status"],
    ) else {
        return NetworkOverview {
            nm_available: false,
            wifi_enabled: false,
            summary: "NetworkManager unavailable".to_string(),
            networks: Vec::new(),
        };
    };
    let wifi_enabled = run_capture("nmcli", &["--color", "no", "radio", "wifi"])
        .map(|s| s.trim() == "enabled")
        .unwrap_or(false);
    let dev_summary = run_capture(
        "nmcli",
        &[
            "--color",
            "no",
            "-t",
            "-f",
            "DEVICE,TYPE,STATE,CONNECTION",
            "device",
            "status",
        ],
    )
    .ok()
    .and_then(|s| {
        s.lines()
            .find(|ln| ln.contains(":connected:"))
            .map(|ln| ln.replace(':', " | "))
    })
    .unwrap_or_else(|| general.trim().replace(':', " | "));
    let networks = if wifi_enabled {
        run_capture(
            "nmcli",
            &[
                "--color",
                "no",
                "-t",
                "-f",
                "IN-USE,SSID,SIGNAL,SECURITY",
                "device",
                "wifi",
                "list",
                "--rescan",
                "no",
            ],
        )
        .map(|s| parse_nmcli_wifi_list(&s))
        .unwrap_or_default()
    } else {
        Vec::new()
    };

    NetworkOverview {
        nm_available: true,
        wifi_enabled,
        summary: if dev_summary.is_empty() {
            "Connected / Ready".to_string()
        } else {
            dev_summary
        },
        networks,
    }
}

pub fn set_wifi_radio(enabled: bool) -> Result<(), String> {
    let val = if enabled { "on" } else { "off" };
    run_capture("nmcli", &["radio", "wifi", val]).map(|_| ())
}

pub fn connect_wifi(ssid: &str, password: &str) -> Result<(), String> {
    let ssid_clean = ssid.trim();
    if ssid_clean.is_empty() {
        return Err("Select or enter a Wi-Fi network (SSID).".to_string());
    }
    if password.is_empty() {
        run_capture("nmcli", &["device", "wifi", "connect", ssid_clean]).map(|_| ())
    } else {
        run_capture(
            "nmcli",
            &["device", "wifi", "connect", ssid_clean, "password", password],
        )
        .map(|_| ())
    }
}

pub fn disconnect_wifi() -> Result<(), String> {
    let active = run_capture("nmcli", &["-t", "-f", "DEVICE,TYPE,STATE", "device", "status"])?;
    for line in active.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 3 && parts[1] == "wifi" && parts[2] == "connected" {
            return run_capture("nmcli", &["device", "disconnect", parts[0]]).map(|_| ());
        }
    }
    Err("No active Wi-Fi interface connected".to_string())
}

pub fn forget_wifi(ssid: &str) -> Result<(), String> {
    let ssid_clean = ssid.trim();
    if ssid_clean.is_empty() {
        return Err("Select a Wi-Fi SSID to forget.".to_string());
    }
    run_capture("nmcli", &["connection", "delete", ssid_clean]).map(|_| ())
}

fn has_bluetooth_hardware() -> bool {
    fs::read_dir("/sys/class/bluetooth")
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

fn is_process_running(comm_name: &str) -> bool {
    let Ok(entries) = fs::read_dir("/proc") else {
        return false;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let comm_path = entry.path().join("comm");
        if let Ok(comm) = fs::read_to_string(comm_path) {
            if comm.trim() == comm_name {
                return true;
            }
        }
    }
    false
}

fn is_bluetoothd_running() -> bool {
    is_process_running("bluetoothd")
}

pub fn is_cupsd_running() -> bool {
    is_process_running("cupsd")
}

pub fn read_bluetooth_overview() -> BluetoothOverview {
    let hw = has_bluetooth_hardware();
    if !is_bluetoothd_running() {
        return BluetoothOverview {
            available: hw,
            service_active: false,
            powered: false,
            summary: if hw {
                "Bluetooth adapter detected (service stopped to save RAM)".to_string()
            } else {
                "No Bluetooth adapter detected (service idle)".to_string()
            },
            devices: Vec::new(),
        };
    }

    let show = run_capture("bluetoothctl", &["show"]);
    let (service_active, powered) = match show {
        Ok(out) => {
            let pwr = out
                .lines()
                .any(|ln| ln.trim().eq_ignore_ascii_case("Powered: yes"));
            (true, pwr)
        }
        Err(_) => (false, false),
    };

    let mut devices = if service_active {
        run_capture("bluetoothctl", &["devices"])
            .map(|s| parse_bluetooth_devices(&s))
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    if service_active {
        if let Ok(paired_out) = run_capture("bluetoothctl", &["devices", "Paired"]) {
            for dev in &mut devices {
                if paired_out.contains(&dev.mac) {
                    dev.paired = true;
                }
            }
        }
        if let Ok(conn_out) = run_capture("bluetoothctl", &["devices", "Connected"]) {
            for dev in &mut devices {
                if conn_out.contains(&dev.mac) {
                    dev.connected = true;
                }
            }
        }
    }

    let summary = if !service_active && !hw {
        "No Bluetooth adapter detected (service idle)".to_string()
    } else if !service_active {
        "Bluetooth adapter detected (service stopped to save RAM)".to_string()
    } else if powered {
        format!("Bluetooth On ({} device(s) listed)", devices.len())
    } else {
        "Bluetooth service running (radio powered off)".to_string()
    };

    BluetoothOverview {
        available: hw || service_active,
        service_active,
        powered,
        summary,
        devices,
    }
}

pub fn set_bluetooth_enabled(enabled: bool) -> Result<(), String> {
    if enabled {
        let _ = run_capture("doas", &[SYS_HELPER_BIN, "bt-start"]);
        run_capture("bluetoothctl", &["power", "on"]).map(|_| ())
    } else {
        let _ = run_capture("bluetoothctl", &["power", "off"]);
        run_capture("doas", &[SYS_HELPER_BIN, "bt-stop"]).map(|_| ())
    }
}

pub fn scan_bluetooth_brief() -> Result<(), String> {
    run_capture_timeout(
        "bluetoothctl",
        &["--timeout", "5", "scan", "on"],
        Duration::from_secs(10),
    )
    .map(|_| ())
}

pub fn bt_device_action(action: &str, mac: &str) -> Result<(), String> {
    if mac.len() != 17 || mac.matches(':').count() != 5 {
        return Err("Select a valid Bluetooth device MAC address.".to_string());
    }
    match action {
        "pair-connect" => {
            let _ = run_capture("bluetoothctl", &["trust", mac]);
            let _ = run_capture("bluetoothctl", &["pair", mac]);
            run_capture("bluetoothctl", &["connect", mac]).map(|_| ())
        }
        "disconnect" => run_capture("bluetoothctl", &["disconnect", mac]).map(|_| ()),
        "remove" => run_capture("bluetoothctl", &["remove", mac]).map(|_| ()),
        other => Err(format!("Unsupported Bluetooth action: {other}")),
    }
}

/// Parse `pactl list cards` to expose selectable audio card profiles (Bluetooth A2DP/HFP, HDMI, Analog).
pub fn parse_pactl_card_profiles(raw: &str) -> Vec<AudioCardProfile> {
    let mut results = Vec::new();
    let mut current_card = String::new();
    let mut active_profile = String::new();
    let mut in_profiles = false;
    let mut pending: Vec<(String, String)> = Vec::new();

    let flush_card = |card: &str,
                      active: &str,
                      profs: &mut Vec<(String, String)>,
                      out: &mut Vec<AudioCardProfile>| {
        if card.is_empty() {
            profs.clear();
            return;
        }
        let short_card = card
            .trim_start_matches("bluez_card.")
            .trim_start_matches("alsa_card.");
        for (pname, desc) in profs.drain(..) {
            out.push(AudioCardProfile {
                card_name: card.to_string(),
                active: pname == active,
                label: format!("{short_card}: {desc}"),
                profile_name: pname,
            });
        }
    };

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Card #") {
            flush_card(&current_card, &active_profile, &mut pending, &mut results);
            current_card.clear();
            active_profile.clear();
            in_profiles = false;
            continue;
        }
        if let Some(name) = trimmed.strip_prefix("Name:") {
            current_card = name.trim().to_string();
            continue;
        }
        if trimmed == "Profiles:" {
            in_profiles = true;
            continue;
        }
        if let Some(act) = trimmed.strip_prefix("Active Profile:") {
            in_profiles = false;
            active_profile = act.trim().to_string();
            continue;
        }
        if trimmed.starts_with("Ports:") || trimmed.starts_with("Properties:") {
            in_profiles = false;
            continue;
        }
        if in_profiles && !trimmed.is_empty() {
            if let Some((pname, rest)) = trimmed.split_once(": ") {
                let pname_clean = pname.trim();
                if pname_clean == "off" || rest.contains("available: no") {
                    continue;
                }
                let desc = rest.split(" (sinks:").next().unwrap_or(rest).trim();
                pending.push((pname_clean.to_string(), desc.to_string()));
            }
        }
    }
    flush_card(&current_card, &active_profile, &mut pending, &mut results);
    results
}

pub fn set_audio_card_profile(card_name: &str, profile_name: &str) -> Result<(), String> {
    if card_name.trim().is_empty() || profile_name.trim().is_empty() {
        return Err("Select a valid audio card profile.".to_string());
    }
    run_capture("pactl", &["set-card-profile", card_name.trim(), profile_name.trim()]).map(|_| ())
}

pub fn read_audio_overview() -> AudioOverview {
    let Ok(vol_raw) = run_capture("pactl", &["get-sink-volume", "@DEFAULT_SINK@"]) else {
        return AudioOverview {
            available: false,
            volume_pct: 50,
            muted: false,
            mic_muted: false,
            sinks: Vec::new(),
            card_profiles: Vec::new(),
        };
    };
    let volume_pct = parse_pactl_volume(&vol_raw);
    let muted = run_capture("pactl", &["get-sink-mute", "@DEFAULT_SINK@"])
        .map(|s| s.contains("yes"))
        .unwrap_or(false);
    let mic_muted = run_capture("pactl", &["get-source-mute", "@DEFAULT_SOURCE@"])
        .map(|s| s.contains("yes"))
        .unwrap_or(false);
    let default_sink = run_capture("pactl", &["get-default-sink"]).unwrap_or_default();
    let sinks = run_capture("pactl", &["list", "short", "sinks"])
        .map(|s| parse_pactl_sinks(&s, &default_sink))
        .unwrap_or_default();
    let card_profiles = run_capture("pactl", &["list", "cards"])
        .map(|s| parse_pactl_card_profiles(&s))
        .unwrap_or_default();

    AudioOverview {
        available: true,
        volume_pct,
        muted,
        mic_muted,
        sinks,
        card_profiles,
    }
}

pub fn set_audio_volume(pct: i32) -> Result<(), String> {
    let clamped = pct.clamp(0, 100);
    run_capture("pactl", &["set-sink-volume", "@DEFAULT_SINK@", &format!("{clamped}%")]).map(|_| ())
}

pub fn set_audio_mute(muted: bool) -> Result<(), String> {
    let flag = if muted { "1" } else { "0" };
    run_capture("pactl", &["set-sink-mute", "@DEFAULT_SINK@", flag]).map(|_| ())
}

pub fn set_mic_mute(muted: bool) -> Result<(), String> {
    let flag = if muted { "1" } else { "0" };
    run_capture("pactl", &["set-source-mute", "@DEFAULT_SOURCE@", flag]).map(|_| ())
}

pub fn set_default_sink(sink_name: &str) -> Result<(), String> {
    run_capture("pactl", &["set-default-sink", sink_name]).map(|_| ())
}

pub fn persistent_display_config_dir() -> PathBuf {
    if cfg!(target_os = "linux") && Path::new("/data/home").is_dir() {
        return PathBuf::from("/data/home/.config/unicore");
    }
    crate::desktop::home_dir().join(".config").join("unicore")
}

pub fn read_display_overview() -> DisplayOverview {
    let outputs = run_capture("wlr-randr", &[])
        .map(|s| parse_wlr_randr(&s))
        .unwrap_or_default();
    let (backlight_available, brightness_pct) = match (
        run_capture("brightnessctl", &["-c", "backlight", "get"]),
        run_capture("brightnessctl", &["-c", "backlight", "max"]),
    ) {
        (Ok(cur), Ok(max)) => {
            let c = cur.trim().parse::<f32>().unwrap_or(0.0);
            let m = max.trim().parse::<f32>().unwrap_or(0.0);
            if m > 0.0 {
                (true, ((c / m) * 100.0).round().clamp(1.0, 100.0) as i32)
            } else {
                (false, 100)
            }
        }
        _ => (false, 100),
    };

    DisplayOverview {
        outputs,
        backlight_available,
        brightness_pct,
    }
}

pub fn apply_and_save_display(output: &str, mode: &str, scale: &str) -> Result<PathBuf, String> {
    let script = render_display_apply_script(output, mode, scale)?;
    run_capture(
        "wlr-randr",
        &["--output", output, "--mode", mode, "--scale", scale],
    )?;

    let cfg_dir = persistent_display_config_dir();
    fs::create_dir_all(&cfg_dir)
        .map_err(|e| format!("Failed to create display config dir: {e}"))?;
    let script_path = cfg_dir.join("display.sh");
    fs::write(&script_path, script)
        .map_err(|e| format!("Failed to write {}: {e}", script_path.display()))?;

    crate::desktop::set_executable(&script_path);
    Ok(script_path)
}

pub fn set_brightness(pct: i32) -> Result<(), String> {
    let clamped = pct.clamp(5, 100);
    run_capture("brightnessctl", &["-c", "backlight", "set", &format!("{clamped}%")])
        .map(|_| ())
}

pub fn run_power_action(action: &str) -> Result<(), String> {
    match action {
        "poweroff" | "reboot" | "suspend" => {
            run_capture("doas", &[SYS_HELPER_BIN, action]).map(|_| ())
        }
        "logout" => run_capture("labwc", &["--exit"]).map(|_| ()),
        other => Err(format!("Unsupported power action: {other}")),
    }
}

pub fn clamp_nightlight_temp(temp_k: i32) -> i32 {
    temp_k.clamp(2700, 5500)
}

pub fn parse_nightlight_conf(raw: &str) -> NightLightConfig {
    let mut cfg = NightLightConfig::default();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            let val = v.trim();
            match k.trim() {
                "NIGHTLIGHT_ENABLED" => {
                    cfg.enabled = matches!(
                        val.to_ascii_lowercase().as_str(),
                        "1" | "true" | "yes" | "on"
                    );
                }
                "NIGHTLIGHT_TEMP" => {
                    if let Ok(parsed) = val.parse::<i32>() {
                        cfg.temp_k = clamp_nightlight_temp(parsed);
                    }
                }
                _ => {}
            }
        }
    }
    cfg
}

pub fn render_nightlight_conf(cfg: &NightLightConfig) -> String {
    format!(
        "NIGHTLIGHT_ENABLED={}\nNIGHTLIGHT_TEMP={}\n",
        if cfg.enabled { 1 } else { 0 },
        clamp_nightlight_temp(cfg.temp_k)
    )
}

pub fn load_nightlight_config(cfg_dir: &Path) -> NightLightConfig {
    fs::read_to_string(cfg_dir.join("nightlight.conf"))
        .map(|raw| parse_nightlight_conf(&raw))
        .unwrap_or_default()
}

pub fn save_and_apply_nightlight(
    cfg_dir: &Path,
    enabled: bool,
    temp_k: i32,
) -> Result<NightLightConfig, String> {
    let cfg = NightLightConfig {
        enabled,
        temp_k: clamp_nightlight_temp(temp_k),
    };
    fs::create_dir_all(cfg_dir)
        .map_err(|e| format!("Failed to create config dir: {e}"))?;
    fs::write(cfg_dir.join("nightlight.conf"), render_nightlight_conf(&cfg))
        .map_err(|e| format!("Failed to save nightlight.conf: {e}"))?;

    if cfg!(target_os = "linux") && Path::new(NIGHTLIGHT_HELPER_BIN).is_file() {
        let mode = if cfg.enabled { "on" } else { "off" };
        let temp_str = cfg.temp_k.to_string();
        run_capture(NIGHTLIGHT_HELPER_BIN, &["set", mode, &temp_str])?;
    }
    Ok(cfg)
}

#[derive(Debug, Clone, PartialEq)]
pub struct PersonalizeConfig {
    pub theme: String,
    pub wallpaper: String,
    pub kb_layout: String,
    pub natural_scroll: bool,
    pub tap_to_click: bool,
    pub pointer_speed: f32,
    pub desktop_icons: bool,
    pub taskbar_style: String,
    pub desktop_layout: String,
    pub accent_color: String,
    pub high_contrast: bool,
    pub large_text: bool,
}

impl Default for PersonalizeConfig {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            wallpaper: DEFAULT_WALLPAPER_PATH.to_string(),
            kb_layout: "us".to_string(),
            natural_scroll: true,
            tap_to_click: true,
            pointer_speed: 0.0,
            desktop_icons: true,
            taskbar_style: "floating".to_string(),
            desktop_layout: "win11".to_string(),
            accent_color: "blue".to_string(),
            high_contrast: false,
            large_text: false,
        }
    }
}

pub fn validate_desktop_layout(raw: &str) -> Result<String, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "win11" | "win10" | "macos" => Ok(lower),
        _ => Err(format!(
            "Unsupported desktop layout '{raw}'. Choose win11, win10, or macos."
        )),
    }
}

pub fn validate_accent_color(raw: &str) -> Result<String, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "blue" | "purple" | "green" | "amber" | "rose" => Ok(lower),
        _ => Err(format!(
            "Unsupported accent color '{raw}'. Choose blue, purple, green, amber, or rose."
        )),
    }
}

pub fn desktop_layout_to_index(raw: &str) -> i32 {
    DESKTOP_LAYOUTS
        .iter()
        .position(|(code, _)| *code == raw)
        .unwrap_or(0) as i32
}

pub fn desktop_layout_from_index(idx: i32) -> &'static str {
    DESKTOP_LAYOUTS
        .get(idx as usize)
        .map(|(code, _)| *code)
        .unwrap_or("win11")
}

pub fn accent_color_to_index(raw: &str) -> i32 {
    ACCENT_COLORS
        .iter()
        .position(|(code, _)| *code == raw)
        .unwrap_or(0) as i32
}

pub fn accent_color_from_index(idx: i32) -> &'static str {
    ACCENT_COLORS
        .get(idx as usize)
        .map(|(code, _)| *code)
        .unwrap_or("blue")
}

pub fn validate_taskbar_style(raw: &str) -> Result<String, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "docked" | "floating" => Ok(lower),
        _ => Err(format!(
            "Unsupported taskbar style '{raw}'. Choose docked or floating."
        )),
    }
}

pub fn validate_theme(raw: &str) -> Result<String, String> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "dark" | "light" => Ok(lower),
        _ => Err(format!("Unsupported theme '{raw}'. Choose dark or light.")),
    }
}

pub fn validate_wallpaper_path(raw: &str) -> Result<String, String> {
    let clean = validate_backup_path(raw)?;
    let lower = clean.to_ascii_lowercase();
    if !(lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp"))
    {
        return Err("Wallpaper path must end with .png, .jpg, .jpeg, or .webp.".to_string());
    }
    Ok(clean)
}

pub fn validate_kb_layout(raw: &str) -> Result<String, String> {
    let lower = raw.trim().to_ascii_lowercase();
    if KB_LAYOUTS.iter().any(|(code, _)| *code == lower) {
        Ok(lower)
    } else {
        Err(format!("Unsupported keyboard layout '{raw}'. Choose us, my, gb, de, fr, or jp."))
    }
}

pub fn clamp_pointer_speed(val: f32) -> f32 {
    let clamped = val.clamp(-0.8, 0.8);
    (clamped * 10.0).round() / 10.0
}

pub fn parse_personalize_conf(raw: &str) -> PersonalizeConfig {
    let mut cfg = PersonalizeConfig::default();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((k, v)) = trimmed.split_once('=') else {
            continue;
        };
        let val = v.trim();
        match k.trim() {
            "UNICORE_THEME" => {
                if let Ok(t) = validate_theme(val) {
                    cfg.theme = t;
                }
            }
            "UNICORE_WALLPAPER" => {
                if let Ok(wp) = validate_wallpaper_path(val) {
                    cfg.wallpaper = wp;
                }
            }
            "UNICORE_KB_LAYOUT" => {
                if let Ok(kb) = validate_kb_layout(val) {
                    cfg.kb_layout = kb;
                }
            }
            "UNICORE_NATURAL_SCROLL" => {
                cfg.natural_scroll = matches!(
                    val.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                );
            }
            "UNICORE_TAP_TO_CLICK" => {
                cfg.tap_to_click = matches!(
                    val.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                );
            }
            "UNICORE_POINTER_SPEED" => {
                if let Ok(parsed) = val.parse::<f32>() {
                    if parsed.is_finite() {
                        cfg.pointer_speed = clamp_pointer_speed(parsed);
                    }
                }
            }
            "UNICORE_DESKTOP_ICONS" => {
                cfg.desktop_icons = matches!(
                    val.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                );
            }
            "UNICORE_TASKBAR_STYLE" => {
                if let Ok(ts) = validate_taskbar_style(val) {
                    cfg.taskbar_style = ts;
                }
            }
            "UNICORE_LAYOUT" => {
                if let Ok(lay) = validate_desktop_layout(val) {
                    cfg.desktop_layout = lay;
                }
            }
            "UNICORE_ACCENT_COLOR" => {
                if let Ok(acc) = validate_accent_color(val) {
                    cfg.accent_color = acc;
                }
            }
            "UNICORE_HIGH_CONTRAST" => {
                cfg.high_contrast = matches!(
                    val.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                );
            }
            "UNICORE_LARGE_TEXT" => {
                cfg.large_text = matches!(
                    val.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                );
            }
            _ => {}
        }
    }
    cfg
}

pub fn render_personalize_conf(cfg: &PersonalizeConfig) -> String {
    let theme = if cfg.theme.eq_ignore_ascii_case("light") {
        "light"
    } else {
        "dark"
    };
    let wallpaper = validate_wallpaper_path(&cfg.wallpaper)
        .unwrap_or_else(|_| DEFAULT_WALLPAPER_PATH.to_string());
    let kb_layout = validate_kb_layout(&cfg.kb_layout).unwrap_or_else(|_| "us".to_string());
    let ns = if cfg.natural_scroll { "yes" } else { "no" };
    let tap = if cfg.tap_to_click { "yes" } else { "no" };
    let di = if cfg.desktop_icons { "yes" } else { "no" };
    let ts = validate_taskbar_style(&cfg.taskbar_style).unwrap_or_else(|_| "floating".to_string());
    let layout = validate_desktop_layout(&cfg.desktop_layout).unwrap_or_else(|_| "win11".to_string());
    let accent = validate_accent_color(&cfg.accent_color).unwrap_or_else(|_| "blue".to_string());
    let hc = if cfg.high_contrast { "yes" } else { "no" };
    let lt = if cfg.large_text { "yes" } else { "no" };
    let speed = clamp_pointer_speed(cfg.pointer_speed);
    format!(
        "UNICORE_THEME={theme}\nUNICORE_WALLPAPER={wallpaper}\nUNICORE_KB_LAYOUT={kb_layout}\nUNICORE_NATURAL_SCROLL={ns}\nUNICORE_TAP_TO_CLICK={tap}\nUNICORE_POINTER_SPEED={speed:.1}\nUNICORE_DESKTOP_ICONS={di}\nUNICORE_TASKBAR_STYLE={ts}\nUNICORE_LAYOUT={layout}\nUNICORE_ACCENT_COLOR={accent}\nUNICORE_HIGH_CONTRAST={hc}\nUNICORE_LARGE_TEXT={lt}\n"
    )
}

pub fn load_personalize_config(cfg_dir: &Path) -> PersonalizeConfig {
    fs::read_to_string(cfg_dir.join("personalize.conf"))
        .map(|raw| parse_personalize_conf(&raw))
        .unwrap_or_default()
}

pub fn save_and_apply_personalize(
    cfg_dir: &Path,
    cfg: &PersonalizeConfig,
) -> Result<PersonalizeConfig, String> {
    let clean = PersonalizeConfig {
        theme: validate_theme(&cfg.theme)?,
        wallpaper: validate_wallpaper_path(&cfg.wallpaper)?,
        kb_layout: validate_kb_layout(&cfg.kb_layout)?,
        natural_scroll: cfg.natural_scroll,
        tap_to_click: cfg.tap_to_click,
        pointer_speed: clamp_pointer_speed(cfg.pointer_speed),
        desktop_icons: cfg.desktop_icons,
        taskbar_style: validate_taskbar_style(&cfg.taskbar_style)?,
        desktop_layout: validate_desktop_layout(&cfg.desktop_layout)?,
        accent_color: validate_accent_color(&cfg.accent_color)?,
        high_contrast: cfg.high_contrast,
        large_text: cfg.large_text,
    };
    fs::create_dir_all(cfg_dir)
        .map_err(|e| format!("Failed to create config dir: {e}"))?;
    fs::write(cfg_dir.join("personalize.conf"), render_personalize_conf(&clean))
        .map_err(|e| format!("Failed to write personalize.conf: {e}"))?;

    if cfg!(target_os = "linux") && Path::new(PERSONALIZE_HELPER_BIN).is_file() {
        let ns = if clean.natural_scroll { "yes" } else { "no" };
        let tap = if clean.tap_to_click { "yes" } else { "no" };
        let di = if clean.desktop_icons { "yes" } else { "no" };
        let hc = if clean.high_contrast { "yes" } else { "no" };
        let lt = if clean.large_text { "yes" } else { "no" };
        let speed = format!("{:.1}", clean.pointer_speed);
        run_capture_timeout(
            PERSONALIZE_HELPER_BIN,
            &[
                "set",
                "--theme",
                &clean.theme,
                "--wallpaper",
                &clean.wallpaper,
                "--kb-layout",
                &clean.kb_layout,
                "--natural-scroll",
                ns,
                "--tap",
                tap,
                "--pointer-speed",
                &speed,
                "--desktop-icons",
                di,
                "--taskbar-style",
                &clean.taskbar_style,
                "--layout",
                &clean.desktop_layout,
                "--accent",
                &clean.accent_color,
                "--high-contrast",
                hc,
                "--large-text",
                lt,
            ],
            Duration::from_secs(8),
        )?;
    }
    Ok(clean)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecureBootOverview {
    pub uefi: bool,
    pub secure_boot: String,
    pub mok_cert: bool,
    pub mok_state: String,
    pub shim_bundled: bool,
}

impl Default for SecureBootOverview {
    fn default() -> Self {
        Self {
            uefi: false,
            secure_boot: "legacy_bios".to_string(),
            mok_cert: false,
            mok_state: "unknown".to_string(),
            shim_bundled: false,
        }
    }
}

impl SecureBootOverview {
    pub fn summary(&self) -> String {
        if !self.uefi {
            return "Legacy BIOS boot (UEFI Secure Boot not applicable)".to_string();
        }
        format!(
            "UEFI Secure Boot: {} | UniCore MOK: {} | Shim: {}",
            self.secure_boot,
            self.mok_state,
            if self.shim_bundled { "bundled" } else { "not bundled" }
        )
    }
}

pub fn parse_secureboot_status(raw: &str) -> SecureBootOverview {
    let mut ov = SecureBootOverview::default();
    for line in raw.lines() {
        let Some((k, v)) = line.trim().split_once('=') else {
            continue;
        };
        match k.trim() {
            "uefi" => ov.uefi = v.trim() == "1",
            "secure_boot" if !v.trim().is_empty() => ov.secure_boot = v.trim().to_string(),
            "mok_cert" => ov.mok_cert = v.trim() == "1",
            "mok_state" if !v.trim().is_empty() => ov.mok_state = v.trim().to_string(),
            "shim_bundled" => ov.shim_bundled = v.trim() == "1",
            _ => {}
        }
    }
    ov
}

pub fn read_secureboot_overview() -> SecureBootOverview {
    if cfg!(target_os = "linux") && Path::new(SYS_HELPER_BIN).is_file() {
        if let Ok(raw) = run_capture_timeout("doas", &[SYS_HELPER_BIN, "sb-status"], Duration::from_secs(5)) {
            return parse_secureboot_status(&raw);
        }
    }
    SecureBootOverview::default()
}

pub fn enroll_secureboot_mok(one_time_pass: &str) -> Result<String, String> {
    let pass = one_time_pass.trim();
    if pass.len() < 8 || pass.len() > 32 {
        return Err("MOK enrollment password must be 8 to 32 characters.".to_string());
    }
    if pass.chars().any(|c| c.is_control()) {
        return Err("MOK enrollment password contains invalid characters.".to_string());
    }
    if !cfg!(target_os = "linux") {
        return Err("UEFI MOK enrollment is only available inside UniCore OS".to_string());
    }
    let mut child = Command::new("doas")
        .args([SYS_HELPER_BIN, "sb-enroll"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to invoke Secure Boot helper: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        writeln!(stdin, "{pass}")
            .map_err(|e| format!("Failed to send MOK password: {e}"))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("Failed waiting for MOK enrollment: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !out.status.success() {
        return Err(if !stderr.is_empty() { stderr } else { stdout });
    }
    Ok(if stdout.is_empty() {
        "MOK enrollment queued! Reboot and confirm 'Enroll MOK' once.".to_string()
    } else {
        stdout
    })
}

pub fn validate_printer_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err("Printer name must be 1-64 characters.".to_string());
    }
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return Err("Printer name cannot be empty.".to_string());
    };
    if !first.is_ascii_alphanumeric()
        || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        return Err(
            "Printer name must start with a letter/digit and contain only A-Z, 0-9, '_', '.', or '-'."
                .to_string(),
        );
    }
    Ok(trimmed.to_string())
}

pub fn validate_printer_uri(uri: &str) -> Result<String, String> {
    let trimmed = uri.trim();
    const ALLOWED_PREFIXES: [&str; 8] = [
        "ipp://", "ipps://", "usb://", "socket://", "lpd://", "http://", "https://", "dnssd://",
    ];
    if !ALLOWED_PREFIXES.iter().any(|p| trimmed.starts_with(p)) {
        return Err(
            "Printer URI must start with ipp://, ipps://, usb://, socket://, lpd://, or dnssd://."
                .to_string(),
        );
    }
    if trimmed.len() <= 8
        || trimmed.chars().any(|c| {
            c.is_whitespace()
                || c.is_control()
                || matches!(c, '"' | '\'' | '$' | '`' | '\\' | ';' | '<' | '>' | '|' | '&')
        })
    {
        return Err("Printer URI contains whitespace or unsafe characters.".to_string());
    }
    Ok(trimmed.to_string())
}

/// Parse `lpstat -p` and `lpstat -d` output into configured printers.
pub fn parse_lpstat_printers(printers_raw: &str, default_raw: &str) -> Vec<PrinterInfo> {
    let default_name = default_raw
        .lines()
        .find_map(|ln| ln.trim().split_once(':').map(|(_, rhs)| rhs.trim().to_string()))
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("none"));

    let mut printers = Vec::new();
    for line in printers_raw.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("printer ") else {
            continue;
        };
        let mut parts = rest.splitn(2, char::is_whitespace);
        let Some(name) = parts.next().map(str::trim).filter(|s| !s.is_empty()) else {
            continue;
        };
        let status = parts
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Ready")
            .to_string();
        printers.push(PrinterInfo {
            name: name.to_string(),
            status,
            is_default: default_name.as_deref() == Some(name),
        });
    }
    printers
}

fn suggest_printer_name_from_uri(uri: &str) -> String {
    let without_scheme = uri.split_once("://").map(|(_, r)| r).unwrap_or(uri);
    let base = without_scheme
        .split(['?', '#'])
        .next()
        .unwrap_or(without_scheme)
        .replace("%20", "_");
    let mut clean = String::new();
    let mut prev_underscore = false;
    for ch in base.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.') {
            clean.push(ch);
            prev_underscore = false;
        } else if !prev_underscore && !clean.is_empty() {
            clean.push('_');
            prev_underscore = true;
        }
        if clean.len() >= 32 {
            break;
        }
    }
    let trimmed = clean.trim_matches('_').to_string();
    if trimmed.is_empty() || !trimmed.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) {
        "UniCore_Printer".to_string()
    } else {
        trimmed
    }
}

/// Parse `lpinfo -v` output into discovered USB / network printer URIs.
pub fn parse_lpinfo_devices(raw: &str) -> Vec<DiscoveredPrinter> {
    let mut list = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let Some(kind) = parts.next() else {
            continue;
        };
        let Some(uri) = parts.next() else {
            continue;
        };
        if validate_printer_uri(uri).is_err() {
            continue;
        }
        let suggested_name = suggest_printer_name_from_uri(uri);
        list.push(DiscoveredPrinter {
            label: format!("[{kind}] {uri}"),
            uri: uri.to_string(),
            suggested_name,
        });
    }
    list
}

pub fn read_printers_overview() -> PrintersOverview {
    let available = Path::new("/etc/init.d/cupsd").exists()
        || Path::new("/etc/init.d/cups").exists()
        || Path::new("/usr/sbin/cupsd").exists();
    if !is_cupsd_running() {
        return PrintersOverview {
            available,
            service_active: false,
            summary: if available {
                "Printer service stopped to save RAM (enable when printing)".to_string()
            } else {
                "CUPS printing packages not installed".to_string()
            },
            printers: Vec::new(),
        };
    }

    let default_raw = run_capture("lpstat", &["-d"]).unwrap_or_default();
    let printers_raw = run_capture("lpstat", &["-p"]).unwrap_or_default();
    let printers = parse_lpstat_printers(&printers_raw, &default_raw);
    let summary = format!("CUPS Active ({} printer(s) configured)", printers.len());
    PrintersOverview {
        available: true,
        service_active: true,
        summary,
        printers,
    }
}

pub fn set_cups_enabled(enabled: bool) -> Result<(), String> {
    let cmd = if enabled { "cups-start" } else { "cups-stop" };
    run_capture("doas", &[SYS_HELPER_BIN, cmd]).map(|_| ())
}

pub fn discover_printers() -> Result<Vec<DiscoveredPrinter>, String> {
    let raw = run_capture_timeout(
        "doas",
        &[SYS_HELPER_BIN, "printer-discover"],
        Duration::from_secs(10),
    )?;
    Ok(parse_lpinfo_devices(&raw))
}

pub fn add_printer(name: &str, uri: &str) -> Result<(), String> {
    let safe_name = validate_printer_name(name)?;
    let safe_uri = validate_printer_uri(uri)?;
    run_capture_timeout(
        "doas",
        &[SYS_HELPER_BIN, "printer-add", &safe_name, &safe_uri, "everywhere"],
        Duration::from_secs(12),
    )
    .map(|_| ())
}

pub fn remove_printer(name: &str) -> Result<(), String> {
    let safe_name = validate_printer_name(name)?;
    run_capture("doas", &[SYS_HELPER_BIN, "printer-remove", &safe_name]).map(|_| ())
}

pub fn set_default_printer(name: &str) -> Result<(), String> {
    let safe_name = validate_printer_name(name)?;
    run_capture("doas", &[SYS_HELPER_BIN, "printer-default", &safe_name]).map(|_| ())
}

pub fn print_test_page(name: &str) -> Result<(), String> {
    let safe_name = validate_printer_name(name)?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("unicore_testprint_{nanos}.txt"));
    fs::write(
        &tmp,
        format!(
            "UniCore OS Test Page\n====================\nPrinter: {safe_name}\nSubsystem: CUPS On-Demand Printing\nStatus: OK\n"
        ),
    )
    .map_err(|e| format!("Failed to prepare test page: {e}"))?;
    let tmp_str = tmp.display().to_string();
    let res = run_capture("lp", &["-d", &safe_name, &tmp_str]).map(|_| ());
    let _ = fs::remove_file(&tmp);
    res
}

pub fn launch_document_scanner() -> Result<(), String> {
    Command::new("simple-scan")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to launch Document Scanner (simple-scan): {e}"))
}

pub fn parse_firewall_status(raw: &str) -> FirewallOverview {
    let mut enabled = true;
    let mut active = false;
    let mut profile = "home".to_string();
    let mut allow_ssh = false;
    let mut rules_count = 0i32;

    for line in raw.lines() {
        let Some((k, v)) = line.trim().split_once('=') else {
            continue;
        };
        let val = v.trim();
        match k.trim() {
            "enabled" => enabled = matches!(val, "1" | "true" | "yes" | "on"),
            "active" => active = matches!(val, "1" | "true" | "yes" | "on"),
            "profile" => {
                profile = if val.eq_ignore_ascii_case("public") {
                    "public".to_string()
                } else {
                    "home".to_string()
                };
            }
            "allow_ssh" => allow_ssh = matches!(val, "1" | "true" | "yes" | "on"),
            "rules_count" => rules_count = val.parse::<i32>().unwrap_or(0).max(0),
            _ => {}
        }
    }

    let summary = if !enabled {
        "Firewall Disabled (inbound connections permitted)".to_string()
    } else if active {
        format!(
            "Firewall Active ({}, SSH {}, {} rules)",
            if profile == "public" { "Public" } else { "Home/LAN" },
            if allow_ssh { "allowed" } else { "blocked" },
            rules_count
        )
    } else {
        "Firewall enabled in config (apply pending)".to_string()
    };

    FirewallOverview {
        enabled,
        active,
        profile,
        allow_ssh,
        rules_count,
        summary,
    }
}

pub fn read_firewall_overview() -> FirewallOverview {
    if !cfg!(target_os = "linux") || !Path::new("/usr/libexec/unicore-firewall").is_file() {
        return FirewallOverview {
            enabled: true,
            active: false,
            profile: "home".to_string(),
            allow_ssh: false,
            rules_count: 0,
            summary: "Firewall idle (nftables helper unavailable on host)".to_string(),
        };
    }
    match run_capture("doas", &[SYS_HELPER_BIN, "fw-status"]) {
        Ok(raw) => parse_firewall_status(&raw),
        Err(err) => FirewallOverview {
            enabled: false,
            active: false,
            profile: "home".to_string(),
            allow_ssh: false,
            rules_count: 0,
            summary: format!("Firewall status error: {err}"),
        },
    }
}

pub fn apply_firewall_settings(enabled: bool, profile: &str, allow_ssh: bool) -> Result<(), String> {
    let flag = if enabled { "on" } else { "off" };
    let prof = if profile.eq_ignore_ascii_case("public") {
        "public"
    } else {
        "home"
    };
    let ssh_flag = if allow_ssh { "1" } else { "0" };
    run_capture("doas", &[SYS_HELPER_BIN, "fw-set", flag, prof, ssh_flag]).map(|_| ())
}

/// Parse `nmcli --color no -t -f NAME,TYPE,ACTIVE connection show` for VPN & WireGuard profiles.
pub fn parse_nmcli_vpn_list(raw: &str) -> Vec<VpnConnection> {
    let mut vpns = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim_end_matches('\r');
        if trimmed.is_empty() {
            continue;
        }
        let fields = split_nmcli_escaped_fields(trimmed);
        if fields.len() < 3 {
            continue;
        }
        let name = fields[0].trim();
        let kind = fields[1].trim().to_ascii_lowercase();
        if name.is_empty() || !matches!(kind.as_str(), "wireguard" | "vpn" | "openvpn") {
            continue;
        }
        let active = fields[2].trim().eq_ignore_ascii_case("yes");
        let label = if kind == "wireguard" {
            "WireGuard".to_string()
        } else {
            "OpenVPN".to_string()
        };
        if !vpns.iter().any(|v: &VpnConnection| v.name == name) {
            vpns.push(VpnConnection {
                name: name.to_string(),
                vpn_type: label,
                active,
            });
        }
    }
    vpns.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    vpns
}

pub fn detect_vpn_import_type(path: &str) -> Result<&'static str, String> {
    let clean = validate_backup_path(path)?;
    let lower = clean.to_ascii_lowercase();
    if lower.ends_with(".conf") {
        Ok("wireguard")
    } else if lower.ends_with(".ovpn") {
        Ok("openvpn")
    } else {
        Err("Select a WireGuard (.conf) or OpenVPN (.ovpn) profile file.".to_string())
    }
}

pub fn list_vpn_connections() -> Vec<VpnConnection> {
    run_capture(
        "nmcli",
        &["--color", "no", "-t", "-f", "NAME,TYPE,ACTIVE", "connection", "show"],
    )
    .map(|raw| parse_nmcli_vpn_list(&raw))
    .unwrap_or_default()
}

pub fn import_vpn_profile(path: &str) -> Result<(), String> {
    let clean = validate_backup_path(path)?;
    let vpn_type = detect_vpn_import_type(&clean)?;
    if !Path::new(&clean).is_file() {
        return Err(format!("VPN profile file not found: {clean}"));
    }
    run_capture_timeout(
        "nmcli",
        &["connection", "import", "type", vpn_type, "file", &clean],
        Duration::from_secs(10),
    )
    .map(|_| ())
}

pub fn toggle_vpn_connection(name: &str, connect: bool) -> Result<(), String> {
    let clean = name.trim();
    if clean.is_empty() {
        return Err("Select a VPN profile first.".to_string());
    }
    let verb = if connect { "up" } else { "down" };
    run_capture_timeout("nmcli", &["connection", verb, clean], Duration::from_secs(15))
        .map(|_| ())
}

pub fn delete_vpn_connection(name: &str) -> Result<(), String> {
    let clean = name.trim();
    if clean.is_empty() {
        return Err("Select a VPN profile to delete.".to_string());
    }
    run_capture("nmcli", &["connection", "delete", clean]).map(|_| ())
}

pub fn validate_backup_path(path: &str) -> Result<String, String> {
    let trimmed = path.trim();
    if !trimmed.starts_with('/') {
        return Err("Path must be an absolute path starting with '/'.".to_string());
    }
    if trimmed.contains("..")
        || trimmed.chars().any(|c| {
            c.is_control()
                || matches!(c, '"' | '\'' | '$' | '`' | '\\' | ';' | '<' | '>' | '|' | '&')
        })
    {
        return Err("Path contains '..' or unsafe characters.".to_string());
    }
    Ok(trimmed.to_string())
}

pub fn validate_backup_passphrase(pass: &str) -> Result<(), String> {
    if pass.len() < 8 || pass.chars().any(|c| c.is_control()) {
        return Err(
            "Encryption passphrase must be at least 8 characters with no control characters."
                .to_string(),
        );
    }
    Ok(())
}

pub fn parse_backup_targets(raw: &str) -> Vec<BackupTarget> {
    let mut targets = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((path, label)) = trimmed.split_once('\t') else {
            continue;
        };
        let p = path.trim();
        let l = label.trim();
        if !p.is_empty() && !l.is_empty() {
            targets.push(BackupTarget {
                path: p.to_string(),
                label: l.to_string(),
            });
        }
    }
    targets
}

pub fn list_backup_targets() -> Vec<BackupTarget> {
    let fallback = vec![BackupTarget {
        path: "/data/home/Backups".to_string(),
        label: "Local Backups (/data/home/Backups)".to_string(),
    }];
    if !cfg!(target_os = "linux") {
        return fallback;
    }
    match run_capture("doas", &[SYS_HELPER_BIN, "backup-targets"]) {
        Ok(raw) => {
            let parsed = parse_backup_targets(&raw);
            if parsed.is_empty() {
                fallback
            } else {
                parsed
            }
        }
        Err(_) => fallback,
    }
}

pub fn run_backup_create(dest_dir: &str, passphrase: Option<&str>) -> Result<String, String> {
    let safe_dir = validate_backup_path(dest_dir)?;
    let out = if let Some(pass) = passphrase {
        validate_backup_passphrase(pass)?;
        run_capture_with_stdin_timeout(
            "doas",
            &[SYS_HELPER_BIN, "backup-create", &safe_dir, "encrypted"],
            Some(pass),
            Duration::from_secs(300),
        )?
    } else {
        run_capture_with_stdin_timeout(
            "doas",
            &[SYS_HELPER_BIN, "backup-create", &safe_dir],
            None,
            Duration::from_secs(300),
        )?
    };
    let archive_path = out
        .lines()
        .find_map(|ln| ln.trim().strip_prefix("BACKUP_OK:"))
        .unwrap_or(&safe_dir)
        .to_string();
    Ok(format!("Backup created: {archive_path}"))
}

pub fn run_backup_restore(archive_path: &str, passphrase: Option<&str>) -> Result<String, String> {
    let safe_arc = validate_backup_path(archive_path)?;
    if !(safe_arc.ends_with(".tar.zst") || safe_arc.ends_with(".tar.zst.enc")) {
        return Err("Archive must end with .tar.zst or .tar.zst.enc".to_string());
    }
    let encrypted = safe_arc.ends_with(".tar.zst.enc") || passphrase.is_some_and(|p| !p.is_empty());
    if encrypted {
        let pass = passphrase.unwrap_or("");
        validate_backup_passphrase(pass)?;
        let _ = run_capture_with_stdin_timeout(
            "doas",
            &[SYS_HELPER_BIN, "backup-restore", &safe_arc, "encrypted"],
            Some(pass),
            Duration::from_secs(300),
        )?;
    } else {
        let _ = run_capture_with_stdin_timeout(
            "doas",
            &[SYS_HELPER_BIN, "backup-restore", &safe_arc],
            None,
            Duration::from_secs(300),
        )?;
    }
    Ok(format!("Restored /data from {safe_arc}"))
}
