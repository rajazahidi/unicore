//! Sandbox provisioning, permission control, and on-demand subsystem lifecycle.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::inspector::{inspect_binary, AppPlatform, BinaryInfo, WinEra};
use crate::json_codec::{parse_json, to_json_pretty, JsonValue};

/// Built-in OS execution subsystems (no third-party launchers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SubsystemKind {
    Win32,
    AndroidArt,
    Dos,
    Native,
}

impl SubsystemKind {
    pub const ALL: [SubsystemKind; 4] = [
        SubsystemKind::Win32,
        SubsystemKind::AndroidArt,
        SubsystemKind::Dos,
        SubsystemKind::Native,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            SubsystemKind::Win32 => "win32",
            SubsystemKind::AndroidArt => "android_art",
            SubsystemKind::Dos => "dos",
            SubsystemKind::Native => "native",
        }
    }
}

impl fmt::Display for SubsystemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// On-demand lifecycle states to minimize idle RAM usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubsystemState {
    Cold,
    Warming,
    Active,
    Hibernated,
}

impl SubsystemState {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubsystemState::Cold => "cold",
            SubsystemState::Warming => "warming",
            SubsystemState::Active => "active",
            SubsystemState::Hibernated => "hibernated",
        }
    }
}

/// Privacy-first per-application permission defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionProfile {
    pub network: bool,
    pub gpu_acceleration: bool,
    pub shared_documents: bool,
    pub camera: bool,
    pub microphone: bool,
}

impl Default for PermissionProfile {
    fn default() -> Self {
        Self {
            network: true,
            gpu_acceleration: true,
            shared_documents: false,
            camera: false,
            microphone: false,
        }
    }
}

impl PermissionProfile {
    pub fn set_permission(&mut self, name: &str, allowed: bool) -> Result<(), String> {
        match name {
            "network" => self.network = allowed,
            "gpu_acceleration" => self.gpu_acceleration = allowed,
            "shared_documents" => self.shared_documents = allowed,
            "camera" => self.camera = allowed,
            "microphone" => self.microphone = allowed,
            other => {
                return Err(format!(
                    "Unknown permission '{other}'. Valid options: camera, gpu_acceleration, microphone, network, shared_documents"
                ))
            }
        }
        Ok(())
    }

    pub fn to_json(&self) -> JsonValue {
        let mut map = BTreeMap::new();
        map.insert("camera".to_string(), JsonValue::Bool(self.camera));
        map.insert(
            "gpu_acceleration".to_string(),
            JsonValue::Bool(self.gpu_acceleration),
        );
        map.insert("microphone".to_string(), JsonValue::Bool(self.microphone));
        map.insert("network".to_string(), JsonValue::Bool(self.network));
        map.insert(
            "shared_documents".to_string(),
            JsonValue::Bool(self.shared_documents),
        );
        JsonValue::Object(map)
    }

    pub fn from_json(value: &JsonValue) -> Self {
        let mut defaults = Self::default();
        let Some(obj) = value.as_object() else {
            return defaults;
        };
        if let Some(v) = obj.get("network").and_then(JsonValue::as_bool) {
            defaults.network = v;
        }
        if let Some(v) = obj.get("gpu_acceleration").and_then(JsonValue::as_bool) {
            defaults.gpu_acceleration = v;
        }
        if let Some(v) = obj.get("shared_documents").and_then(JsonValue::as_bool) {
            defaults.shared_documents = v;
        }
        if let Some(v) = obj.get("camera").and_then(JsonValue::as_bool) {
            defaults.camera = v;
        }
        if let Some(v) = obj.get("microphone").and_then(JsonValue::as_bool) {
            defaults.microphone = v;
        }
        defaults
    }
}

/// Isolated sandbox metadata for an installed application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSandbox {
    pub app_id: String,
    pub name: String,
    pub platform: AppPlatform,
    pub arch: String,
    pub sub_abis: Vec<String>,
    pub is_gui: bool,
    pub win_era: Option<WinEra>,
    pub source_path: String,
    pub sandbox_dir: String,
    pub permissions: PermissionProfile,
    pub launch_args: Vec<String>,
    pub env_vars: BTreeMap<String, String>,
}

impl AppSandbox {
    pub fn to_json(&self) -> JsonValue {
        let mut map = BTreeMap::new();
        map.insert("app_id".to_string(), JsonValue::Str(self.app_id.clone()));
        map.insert("arch".to_string(), JsonValue::Str(self.arch.clone()));
        let mut env_obj = BTreeMap::new();
        for (k, v) in &self.env_vars {
            env_obj.insert(k.clone(), JsonValue::Str(v.clone()));
        }
        map.insert("env_vars".to_string(), JsonValue::Object(env_obj));
        map.insert("is_gui".to_string(), JsonValue::Bool(self.is_gui));
        map.insert(
            "launch_args".to_string(),
            JsonValue::Array(self.launch_args.iter().map(|s| JsonValue::Str(s.clone())).collect()),
        );
        map.insert("name".to_string(), JsonValue::Str(self.name.clone()));
        map.insert(
            "permissions".to_string(),
            self.permissions.to_json(),
        );
        map.insert(
            "platform".to_string(),
            JsonValue::Str(self.platform.as_str().to_string()),
        );
        map.insert(
            "sandbox_dir".to_string(),
            JsonValue::Str(self.sandbox_dir.clone()),
        );
        map.insert(
            "source_path".to_string(),
            JsonValue::Str(self.source_path.clone()),
        );
        map.insert(
            "sub_abis".to_string(),
            JsonValue::Array(
                self.sub_abis
                    .iter()
                    .map(|s| JsonValue::Str(s.clone()))
                    .collect(),
            ),
        );
        map.insert(
            "win_era".to_string(),
            match self.win_era {
                Some(era) => JsonValue::Str(era.as_str().to_string()),
                None => JsonValue::Null,
            },
        );
        JsonValue::Object(map)
    }

    pub fn from_json(value: &JsonValue) -> Result<Self, String> {
        let obj = value
            .as_object()
            .ok_or_else(|| "App entry must be a JSON object".to_string())?;
        let get_str = |k: &str| -> Result<String, String> {
            obj.get(k)
                .and_then(JsonValue::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("Missing string field '{k}'"))
        };
        let sub_abis = obj
            .get("sub_abis")
            .and_then(JsonValue::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(JsonValue::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let launch_args = obj
            .get("launch_args")
            .and_then(JsonValue::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(JsonValue::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let mut env_vars = BTreeMap::new();
        if let Some(env_obj) = obj.get("env_vars").and_then(JsonValue::as_object) {
            for (k, v) in env_obj {
                if let Some(val) = v.as_str() {
                    env_vars.insert(k.clone(), val.to_string());
                }
            }
        }
        let permissions = obj
            .get("permissions")
            .map(PermissionProfile::from_json)
            .unwrap_or_default();
        let is_gui = obj
            .get("is_gui")
            .and_then(JsonValue::as_bool)
            .unwrap_or(true);
        let win_era = match obj.get("win_era").and_then(JsonValue::as_str) {
            Some(raw) => Some(WinEra::parse(raw)?),
            None => None,
        };

        Ok(Self {
            app_id: get_str("app_id")?,
            name: get_str("name")?,
            platform: AppPlatform::parse(&get_str("platform")?)?,
            arch: get_str("arch")?,
            sub_abis,
            is_gui,
            win_era,
            source_path: get_str("source_path")?,
            sandbox_dir: get_str("sandbox_dir")?,
            permissions,
            launch_args,
            env_vars,
        })
    }
}

/// Split simple whitespace/quoted launch arguments safely without shell expansion.
pub fn split_launch_args(raw: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut in_quote: Option<char> = None;
    for ch in raw.chars() {
        match in_quote {
            Some(q) if ch == q => in_quote = None,
            Some(_) => cur.push(ch),
            None if ch == '\'' || ch == '"' => in_quote = Some(ch),
            None if ch.is_whitespace() => {
                if !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                }
            }
            None => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        args.push(cur);
    }
    args
}

/// Parse `KEY=VAL` environment overrides separated by commas or newlines.
pub fn parse_env_vars_str(raw: &str) -> Result<BTreeMap<String, String>, String> {
    let mut map = BTreeMap::new();
    for piece in raw.split([',', '\n']) {
        let trimmed = piece.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((k, v)) = trimmed.split_once('=') else {
            return Err(format!("Invalid environment override '{trimmed}'. Use KEY=VALUE."));
        };
        let key = k.trim();
        let mut chars = key.chars();
        let Some(first) = chars.next() else {
            return Err("Environment variable name cannot be empty.".to_string());
        };
        if !(first.is_ascii_alphabetic() || first == '_')
            || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(format!("Unsafe environment variable name '{key}'."));
        }
        if v.chars().any(|c| c.is_control()) {
            return Err(format!("Environment value for '{key}' contains control characters."));
        }
        map.insert(key.to_string(), v.trim().to_string());
    }
    Ok(map)
}

pub fn format_env_vars_str(map: &BTreeMap<String, String>) -> String {
    map.iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Host runtime environment context used to construct `bubblewrap` confinement arguments.
#[derive(Debug, Clone)]
pub struct ConfinementContext<'a> {
    pub xdg_runtime_dir: Option<&'a str>,
    pub wayland_display: Option<&'a str>,
    pub has_x11: bool,
    pub video_devices: &'a [String],
}

fn is_safe_app_workdir(dir: &Path) -> bool {
    let s = dir.to_string_lossy();
    let is_abs_or_win = dir.is_absolute() || (cfg!(windows) && s.len() >= 3);
    if !is_abs_or_win {
        return false;
    }
    let trimmed = s.trim_end_matches(['/', '\\']);
    !trimmed.is_empty()
        && !matches!(
            trimmed,
            "/" | "/tmp"
                | "/var"
                | "/usr"
                | "/etc"
                | "/bin"
                | "/sbin"
                | "/lib"
                | "/home"
                | "/home/unicore"
                | "/data"
                | "/data/home"
        )
}

/// Return safe writable game/app directory for multi-file Store packages (`.../store/<id>/...`).
pub fn store_app_workdir(source_path: &str) -> Option<PathBuf> {
    if !source_path.contains("/store/") && !source_path.contains("\\store\\") {
        return None;
    }
    let parent = Path::new(source_path).parent()?;
    is_safe_app_workdir(parent).then(|| parent.to_path_buf())
}

/// Build deterministic `bubblewrap` (`bwrap`) arguments enforcing the app's 5 sandbox permissions.
pub fn build_bwrap_args(
    app: &AppSandbox,
    subsystem: SubsystemKind,
    ctx: &ConfinementContext<'_>,
) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let push_pair = |out: &mut Vec<String>, flag: &str, a: &str, b: &str| {
        out.push(flag.to_string());
        out.push(a.to_string());
        out.push(b.to_string());
    };

    push_pair(&mut args, "--ro-bind", "/usr", "/usr");
    push_pair(&mut args, "--ro-bind-try", "/lib", "/lib");
    push_pair(&mut args, "--ro-bind-try", "/lib64", "/lib64");
    push_pair(&mut args, "--ro-bind-try", "/bin", "/bin");
    push_pair(&mut args, "--ro-bind-try", "/sbin", "/sbin");
    push_pair(&mut args, "--ro-bind", "/etc", "/etc");

    args.extend(["--tmpfs".to_string(), "/etc/doas.d".to_string()]);
    push_pair(&mut args, "--ro-bind-try", "/dev/null", "/etc/doas.conf");
    push_pair(&mut args, "--ro-bind-try", "/dev/null", "/etc/shadow");

    args.extend([
        "--proc".to_string(),
        "/proc".to_string(),
        "--dev".to_string(),
        "/dev".to_string(),
        "--tmpfs".to_string(),
        "/tmp".to_string(),
        "--tmpfs".to_string(),
        "/run".to_string(),
        "--tmpfs".to_string(),
        "/data".to_string(),
        "--dir".to_string(),
        "/data/network".to_string(),
    ]);

    push_pair(&mut args, "--bind", &app.sandbox_dir, &app.sandbox_dir);

    let sandbox_prefix = format!("{}/", app.sandbox_dir.trim_end_matches('/'));
    if subsystem == SubsystemKind::Dos {
        if let Some(parent) = Path::new(&app.source_path)
            .parent()
            .filter(|p| is_safe_app_workdir(p))
        {
            let parent_str = parent.to_string_lossy();
            push_pair(&mut args, "--bind", &parent_str, &parent_str);
        }
    } else if matches!(subsystem, SubsystemKind::Win32 | SubsystemKind::Native) {
        if let Some(workdir) = store_app_workdir(&app.source_path) {
            let workdir_str = workdir.to_string_lossy();
            push_pair(&mut args, "--bind", &workdir_str, &workdir_str);
        }
    }
    if !app.source_path.starts_with("/usr/")
        && app.source_path != app.sandbox_dir
        && !app.source_path.starts_with(&sandbox_prefix)
    {
        push_pair(&mut args, "--ro-bind", &app.source_path, &app.source_path);
    }

    if let Some(xdg) = ctx.xdg_runtime_dir.filter(|s| !s.is_empty()) {
        args.push("--dir".to_string());
        args.push(xdg.to_string());
        if let Some(wl) = ctx.wayland_display.filter(|s| !s.is_empty()) {
            let sock = format!("{xdg}/{wl}");
            push_pair(&mut args, "--bind", &sock, &sock);
        }
        let pw = format!("{xdg}/pipewire-0");
        let pulse = format!("{xdg}/pulse");
        push_pair(&mut args, "--bind-try", &pw, &pw);
        push_pair(&mut args, "--bind-try", &pulse, &pulse);
    }

    if ctx.has_x11 {
        push_pair(&mut args, "--bind-try", "/tmp/.X11-unix", "/tmp/.X11-unix");
    }

    if !app.permissions.network {
        args.push("--unshare-net".to_string());
    }
    if app.permissions.gpu_acceleration {
        push_pair(&mut args, "--dev-bind-try", "/dev/dri", "/dev/dri");
    }
    if app.permissions.camera {
        for dev in ctx.video_devices {
            push_pair(&mut args, "--dev-bind", dev, dev);
        }
    }
    if app.permissions.microphone {
        push_pair(&mut args, "--dev-bind-try", "/dev/snd", "/dev/snd");
    }
    if app.permissions.shared_documents {
        push_pair(
            &mut args,
            "--bind",
            "/data/home/Documents",
            "/data/home/Documents",
        );
    }
    if subsystem == SubsystemKind::Native {
        args.push("--unshare-pid".to_string());
    }

    args.extend([
        "--die-with-parent".to_string(),
        "--new-session".to_string(),
        "--cap-drop".to_string(),
        "ALL".to_string(),
    ]);
    args
}

/// Metadata for an Android app reported by `waydroid app list`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidAppInfo {
    pub name: String,
    pub package_name: String,
}

/// Parse `Name:` / `packageName:` stanzas emitted by `waydroid app list`.
pub fn parse_waydroid_app_list(raw: &str) -> Vec<AndroidAppInfo> {
    let mut apps = Vec::new();
    let mut cur_name: Option<String> = None;
    let mut cur_pkg: Option<String> = None;

    let flush = |apps: &mut Vec<AndroidAppInfo>, name: &mut Option<String>, pkg: &mut Option<String>| {
        if let Some(p) = pkg.take() {
            let n = name.take().unwrap_or_else(|| p.clone());
            apps.push(AndroidAppInfo {
                name: n,
                package_name: p,
            });
        } else {
            *name = None;
        }
    };

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush(&mut apps, &mut cur_name, &mut cur_pkg);
            continue;
        }
        if let Some(val) = trimmed.strip_prefix("Name:") {
            if cur_pkg.is_some() {
                flush(&mut apps, &mut cur_name, &mut cur_pkg);
            }
            let clean = val.trim();
            if !clean.is_empty() {
                cur_name = Some(clean.to_string());
            }
        } else if let Some(val) = trimmed.strip_prefix("packageName:") {
            let clean = val.trim();
            if !clean.is_empty() {
                cur_pkg = Some(clean.to_string());
            }
        }
    }
    flush(&mut apps, &mut cur_name, &mut cur_pkg);
    apps
}

/// Sanitize an Android package identifier (`a-z`, `0-9`, `.`, `-`, `_`).
pub fn sanitize_package_id(pkg: &str) -> String {
    let cleaned: String = pkg
        .trim()
        .chars()
        .map(|c| {
            let lower = c.to_ascii_lowercase();
            if lower.is_ascii_alphanumeric() || matches!(lower, '.' | '-' | '_') {
                lower
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "android_app".to_string()
    } else {
        cleaned
    }
}

/// Render an XDG `.desktop` entry launching an installed Waydroid app directly.
pub fn render_android_desktop_entry(app: &AndroidAppInfo) -> String {
    let clean_name: String = app
        .name
        .chars()
        .filter(|c| !matches!(c, '\r' | '\n'))
        .collect::<String>()
        .trim()
        .to_string();
    let safe_pkg = sanitize_package_id(&app.package_name);
    format!(
        "[Desktop Entry]\nType=Application\nVersion=1.0\nName={clean_name}\nComment=Android App ({safe_pkg}) via UniCore OS\nExec=/usr/bin/waydroid app launch {safe_pkg}\nIcon=package-x-generic\nTerminal=false\nCategories=Utility;\nStartupNotify=true\n"
    )
}

/// Concrete OS subsystem invocation plan generated by the orchestrator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPlan {
    pub app_id: String,
    pub session_id: String,
    pub subsystem: SubsystemKind,
    pub runtime_binary: String,
    pub sandbox_dir: String,
    pub argv: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub translation_layer: Option<String>,
    pub permissions: PermissionProfile,
}

/// Tracks active app references and idle hibernation for a single subsystem.
#[derive(Debug, Clone)]
pub struct SubsystemRuntime {
    pub kind: SubsystemKind,
    pub state: SubsystemState,
    pub active_sessions: BTreeSet<String>,
    pub last_active_at: f64,
    pub idle_timeout_sec: f64,
}

impl SubsystemRuntime {
    pub fn new(kind: SubsystemKind, idle_timeout_sec: f64) -> Self {
        Self {
            kind,
            state: SubsystemState::Cold,
            active_sessions: BTreeSet::new(),
            last_active_at: 0.0,
            idle_timeout_sec,
        }
    }

    pub fn acquire(&mut self, session_id: &str, now: Option<f64>) -> SubsystemState {
        let ts = now.unwrap_or_else(current_timestamp_secs);
        if matches!(
            self.state,
            SubsystemState::Cold | SubsystemState::Hibernated
        ) {
            self.state = SubsystemState::Warming;
        }
        self.active_sessions.insert(session_id.to_string());
        self.state = SubsystemState::Active;
        self.last_active_at = ts;
        self.state
    }

    pub fn release(&mut self, session_id: &str, now: Option<f64>) -> bool {
        let ts = now.unwrap_or_else(current_timestamp_secs);
        let existed = self.active_sessions.remove(session_id);
        if self.active_sessions.is_empty() {
            self.last_active_at = ts;
        }
        existed
    }

    pub fn evaluate_idle(&mut self, now: Option<f64>) -> SubsystemState {
        let ts = now.unwrap_or_else(current_timestamp_secs);
        if self.state == SubsystemState::Active
            && self.active_sessions.is_empty()
            && (ts - self.last_active_at) >= self.idle_timeout_sec
        {
            self.state = SubsystemState::Hibernated;
        }
        self.state
    }

    pub fn reclaim(&mut self) -> SubsystemState {
        if self.state == SubsystemState::Hibernated && self.active_sessions.is_empty() {
            self.state = SubsystemState::Cold;
        }
        self.state
    }
}

struct SubsystemSpec {
    runtime_binary: &'static str,
    sandbox_subdirs: &'static [&'static str],
}

fn platform_to_subsystem(platform: AppPlatform) -> Option<SubsystemKind> {
    match platform {
        AppPlatform::WindowsPe | AppPlatform::WindowsNe16 | AppPlatform::WindowsMsi => {
            Some(SubsystemKind::Win32)
        }
        AppPlatform::Dos16 => Some(SubsystemKind::Dos),
        AppPlatform::AndroidApk => Some(SubsystemKind::AndroidArt),
        AppPlatform::NativeElf | AppPlatform::LinuxFlatpak => Some(SubsystemKind::Native),
        AppPlatform::Unknown => None,
    }
}

fn win32_era_backend(era: WinEra, gpu_accel: bool) -> &'static str {
    if !gpu_accel {
        return "software";
    }
    if let Ok(hw_mode) = std::env::var("UNILOAD_HW_WIN32_MODE") {
        if hw_mode == "gdi-legacy" {
            return "gdi-legacy";
        }
        if hw_mode == "wined3d-opengl" {
            return match era {
                WinEra::Win98 => "ddraw-compat+wined3d",
                WinEra::WinXp | WinEra::Win7 | WinEra::Win10 => "wined3d-opengl",
            };
        }
    }
    match era {
        WinEra::Win98 => "ddraw-compat+wined3d",
        WinEra::WinXp => "dxvk-d3d8-d3d9",
        WinEra::Win7 => "dxvk-d3d8-d3d11",
        WinEra::Win10 => "dxvk+vkd3d",
    }
}

fn subsystem_spec(kind: SubsystemKind) -> SubsystemSpec {
    match kind {
        SubsystemKind::Win32 => SubsystemSpec {
            runtime_binary: "/usr/libexec/uniload-win32",
            sandbox_subdirs: &[
                "drive_c_root/drive_c/users/sandbox",
                "drive_c_root/drive_c/windows",
            ],
        },
        SubsystemKind::AndroidArt => SubsystemSpec {
            runtime_binary: "/usr/libexec/uniload-art",
            sandbox_subdirs: &["data/app", "data/data", "data/dalvik-cache"],
        },
        SubsystemKind::Dos => SubsystemSpec {
            runtime_binary: "/usr/libexec/uniload-dos",
            sandbox_subdirs: &["drive_c", "config"],
        },
        SubsystemKind::Native => SubsystemSpec {
            runtime_binary: "/usr/libexec/uniload-native",
            sandbox_subdirs: &["home", "cache"],
        },
    }
}

/// Host readiness status for the built-in Android ART / Waydroid subsystem.
#[derive(Debug, Clone)]
pub struct AndroidHostStatus {
    pub binderfs_ready: bool,
    pub waydroid_installed: bool,
    pub image_ready: bool,
    pub arm_translator_ready: bool,
    pub container_active: bool,
}

/// Inspect host filesystem markers for the built-in Android subsystem.
pub fn detect_android_host_status() -> AndroidHostStatus {
    AndroidHostStatus {
        binderfs_ready: Path::new("/dev/binderfs/binder").exists(),
        waydroid_installed: Path::new("/usr/bin/waydroid").is_file(),
        image_ready: Path::new("/data/waydroid/images/system.img").is_file(),
        arm_translator_ready: Path::new("/data/waydroid/overlay/system/lib64/libndk_translation.so")
            .is_file()
            || Path::new("/data/waydroid/overlay/system/lib64/libhoudini.so").is_file(),
        container_active: Path::new("/run/waydroid-container.pid").exists()
            || Path::new("/var/lib/lxc/waydroid/waydroid.pid").exists(),
    }
}

/// Host readiness status for Win32 / Win16 / MS-DOS runtime assets.
#[derive(Debug, Clone)]
pub struct Win32AssetsStatus {
    pub wine_installed: bool,
    pub dxvk_installed: bool,
    pub vkd3d_installed: bool,
    pub otvdm_installed: bool,
    pub cnc_ddraw_installed: bool,
    pub winetricks_installed: bool,
    pub dos_subsystem_ready: bool,
}

fn dir_has_dlls(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file()
            && p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
        {
            return true;
        }
        if p.is_dir() && dir_has_dlls(&p) {
            return true;
        }
    }
    false
}

/// Inspect host filesystem markers for Win32/Win16 runtimes & DirectX Vulkan translation assets.
pub fn detect_win32_assets_status() -> Win32AssetsStatus {
    Win32AssetsStatus {
        wine_installed: Path::new("/usr/bin/wine").is_file() || Path::new("/usr/bin/wine64").is_file(),
        dxvk_installed: [
            "/data/uniload/dxvk",
            "/usr/share/dxvk",
            "/usr/lib/dxvk",
        ]
        .iter()
        .any(|d| dir_has_dlls(Path::new(d))),
        vkd3d_installed: ["/data/uniload/vkd3d", "/usr/share/vkd3d-proton"]
            .iter()
            .any(|d| dir_has_dlls(Path::new(d))),
        otvdm_installed: Path::new("/data/uniload/otvdm/otvdm.exe").is_file()
            || Path::new("/usr/share/unicore/otvdm/otvdm.exe").is_file(),
        cnc_ddraw_installed: ["/data/uniload/cnc-ddraw", "/usr/share/cnc-ddraw"]
            .iter()
            .any(|d| dir_has_dlls(Path::new(d))),
        winetricks_installed: Path::new("/usr/bin/winetricks").is_file(),
        dos_subsystem_ready: Path::new("/usr/libexec/uniload-dos").is_file()
            && (Path::new("/usr/bin/dosbox").is_file()
                || Path::new("/usr/bin/dosbox-staging").is_file()),
    }
}

fn build_subsystem_invocation(
    kind: SubsystemKind,
    runtime_binary: &str,
    app: &AppSandbox,
    translation: Option<&str>,
    era_override: Option<WinEra>,
) -> (Vec<String>, BTreeMap<String, String>) {
    let mut env = BTreeMap::new();
    let sandbox_root = Path::new(&app.sandbox_dir);
    let argv = match kind {
        SubsystemKind::Win32 => {
            let effective_era = era_override.or(app.win_era).unwrap_or(WinEra::Win10);
            let win_arch = match app.platform {
                AppPlatform::WindowsNe16 => "win16",
                _ if app.arch == "x86" => "wow64-32bit",
                _ => "win64",
            };
            let virtual_desktop = if effective_era == WinEra::Win98 && app.is_gui {
                "800x600"
            } else {
                "off"
            };
            env.insert("UNILOAD_SUBSYSTEM".to_string(), "win32".to_string());
            env.insert(
                "UNILOAD_WIN_ERA".to_string(),
                effective_era.as_str().to_string(),
            );
            env.insert("UNILOAD_WIN_ARCH".to_string(), win_arch.to_string());
            env.insert(
                "UNILOAD_WIN16_MODE".to_string(),
                if app.platform == AppPlatform::WindowsNe16 {
                    "1"
                } else {
                    "0"
                }
                .to_string(),
            );
            env.insert(
                "UNILOAD_VIRTUAL_DESKTOP".to_string(),
                virtual_desktop.to_string(),
            );
            env.insert(
                "WINEPREFIX".to_string(),
                sandbox_root.join("drive_c_root").display().to_string(),
            );
            env.insert(
                "WINEDLLOVERRIDES".to_string(),
                "winemenubuilder.exe=d".to_string(),
            );
            env.insert(
                "UNILOAD_VK_BACKEND".to_string(),
                win32_era_backend(effective_era, app.permissions.gpu_acceleration).to_string(),
            );
            if let Some(layer) = translation {
                env.insert("UNILOAD_CPU_TRANSLATOR".to_string(), layer.to_string());
            }
            if let Some(workdir) = store_app_workdir(&app.source_path) {
                env.insert(
                    "UNILOAD_APP_WORKDIR".to_string(),
                    workdir.display().to_string(),
                );
            }
            if app.platform == AppPlatform::WindowsMsi {
                vec![
                    runtime_binary.to_string(),
                    "msiexec".to_string(),
                    "/i".to_string(),
                    app.source_path.clone(),
                ]
            } else {
                vec![runtime_binary.to_string(), app.source_path.clone()]
            }
        }
        SubsystemKind::AndroidArt => {
            env.insert("UNILOAD_SUBSYSTEM".to_string(), "android_art".to_string());
            env.insert(
                "ANDROID_DATA".to_string(),
                sandbox_root.join("data").display().to_string(),
            );
            env.insert(
                "UNILOAD_ANDROID_ROOT".to_string(),
                "/data/waydroid".to_string(),
            );
            env.insert("UNILOAD_MULTI_WINDOW".to_string(), "1".to_string());
            env.insert("UNILOAD_ANDROID_ARCH".to_string(), app.arch.clone());
            env.insert(
                "UNILOAD_BINDER_NODE".to_string(),
                "/dev/binderfs/binder".to_string(),
            );
            let android_gpu = if !app.permissions.gpu_acceleration
                || matches!(
                    std::env::var("UNILOAD_HW_ANDROID_GRAPHICS").as_deref(),
                    Ok("swiftshader")
                )
            {
                "swiftshader"
            } else {
                "vulkan-dma-buf"
            };
            env.insert("UNILOAD_GRAPHICS".to_string(), android_gpu.to_string());
            if let Some(layer) = translation {
                env.insert("UNILOAD_NDK_TRANSLATOR".to_string(), layer.to_string());
            }
            vec![
                runtime_binary.to_string(),
                "launch-apk".to_string(),
                "--apk".to_string(),
                app.source_path.clone(),
                "--sandbox".to_string(),
                app.app_id.clone(),
            ]
        }
        SubsystemKind::Dos => {
            env.insert("UNILOAD_SUBSYSTEM".to_string(), "dos".to_string());
            env.insert(
                "UNILOAD_DOS_DRIVE_C".to_string(),
                sandbox_root.join("drive_c").display().to_string(),
            );
            env.insert(
                "UNILOAD_DOS_CONF".to_string(),
                sandbox_root
                    .join("config")
                    .join("dos.conf")
                    .display()
                    .to_string(),
            );
            env.insert(
                "HOME".to_string(),
                sandbox_root.join("config").display().to_string(),
            );
            let workdir = Path::new(&app.source_path)
                .parent()
                .filter(|p| is_safe_app_workdir(p))
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            env.insert("UNILOAD_DOS_WORKDIR".to_string(), workdir);
            vec![runtime_binary.to_string(), app.source_path.clone()]
        }
        SubsystemKind::Native => {
            env.insert("UNILOAD_SUBSYSTEM".to_string(), "native".to_string());
            env.insert(
                "HOME".to_string(),
                sandbox_root.join("home").display().to_string(),
            );
            vec![runtime_binary.to_string(), app.source_path.clone()]
        }
    };
    (argv, env)
}

fn current_timestamp_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

/// Stop any running `wineserver` bound to `wine_prefix`, dropping UID/GID to the prefix owner when needed.
pub fn stop_wineserver_for_prefix(wine_prefix: &Path) -> bool {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        use std::os::unix::process::CommandExt;
        use std::process::{Command, Stdio};

        if !wine_prefix.is_dir() {
            return false;
        }
        let mut cmd = Command::new("wineserver");
        cmd.arg("-k")
            .env("WINEPREFIX", wine_prefix)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        if let (Ok(prefix_meta), Ok(self_meta)) =
            (fs::metadata(wine_prefix), fs::metadata("/proc/self"))
        {
            if self_meta.uid() == 0 && prefix_meta.uid() != 0 {
                cmd.uid(prefix_meta.uid()).gid(prefix_meta.gid());
            }
        }
        cmd.status().is_ok_and(|s| s.success())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = wine_prefix;
        false
    }
}

fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "app".to_string()
    } else {
        out
    }
}

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
    0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
    0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
    0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
    0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
    0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
    0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
    0xc67178f2,
];

/// Streaming FIPS 180-4 SHA-256 hasher with constant memory overhead.
pub struct Sha256 {
    h: [u32; 8],
    buf: [u8; 64],
    buf_len: usize,
    total_len: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    pub fn new() -> Self {
        Self {
            h: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c,
                0x1f83d9ab, 0x5be0cd19,
            ],
            buf: [0u8; 64],
            buf_len: 0,
            total_len: 0,
        }
    }

    fn compress(h: &mut [u32; 8], chunk: &[u8]) {
        let mut w = [0u32; 64];
        for (i, word) in chunk[..64].as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        let delta = [a, b, c, d, e, f, g, hh];
        for (slot, add) in h.iter_mut().zip(delta) {
            *slot = slot.wrapping_add(add);
        }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.total_len = self.total_len.wrapping_add(data.len() as u64);

        if self.buf_len > 0 {
            let take = (64 - self.buf_len).min(data.len());
            self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
            self.buf_len += take;
            data = &data[take..];
            if self.buf_len == 64 {
                Self::compress(&mut self.h, &self.buf);
                self.buf_len = 0;
            }
        }

        while data.len() >= 64 {
            Self::compress(&mut self.h, &data[..64]);
            data = &data[64..];
        }

        if !data.is_empty() {
            self.buf[..data.len()].copy_from_slice(data);
            self.buf_len = data.len();
        }
    }

    pub fn finalize_hex(mut self) -> String {
        let bit_len = self.total_len.wrapping_mul(8);
        self.buf[self.buf_len] = 0x80;
        self.buf_len += 1;

        if self.buf_len > 56 {
            self.buf[self.buf_len..64].fill(0);
            Self::compress(&mut self.h, &self.buf);
            self.buf_len = 0;
        }

        self.buf[self.buf_len..56].fill(0);
        self.buf[56..64].copy_from_slice(&bit_len.to_be_bytes());
        Self::compress(&mut self.h, &self.buf);

        let mut hex = String::with_capacity(64);
        for word in self.h {
            use std::fmt::Write as _;
            let _ = write!(hex, "{word:08x}");
        }
        hex
    }
}

/// Pure-Rust FIPS 180-4 SHA-256 digest returned as lowercase hex.
pub fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    hasher.finalize_hex()
}

/// Stream-hash a file in 64 KiB chunks so large SquashFS images use constant RAM.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    use std::io::Read;

    let mut file =
        fs::File::open(path).map_err(|e| format!("Failed to open {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65_536];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize_hex())
}

fn compute_app_id(name: &str, canonical_path: &Path) -> String {
    let hash = sha256_hex(canonical_path.to_string_lossy().as_bytes());
    format!("{}-{}", slugify(name), &hash[..8])
}

/// Manages per-app sandboxes, permissions, and on-demand subsystem states.
pub struct SandboxManager {
    pub state_dir: PathBuf,
    pub sandboxes_root: PathBuf,
    pub registry_path: PathBuf,
    pub idle_timeout_sec: f64,
    apps: BTreeMap<String, AppSandbox>,
    runtimes: BTreeMap<SubsystemKind, SubsystemRuntime>,
    session_to_subsystem: BTreeMap<String, SubsystemKind>,
    session_counter: u64,
}

impl SandboxManager {
    pub fn new(state_dir: impl AsRef<Path>, idle_timeout_sec: f64) -> Result<Self, String> {
        let state_dir = state_dir.as_ref().to_path_buf();
        let sandboxes_root = state_dir.join("sandboxes");
        let registry_path = state_dir.join("registry.json");

        fs::create_dir_all(&sandboxes_root)
            .map_err(|e| format!("Failed to create sandbox root: {e}"))?;

        let mut runtimes = BTreeMap::new();
        for kind in SubsystemKind::ALL {
            runtimes.insert(kind, SubsystemRuntime::new(kind, idle_timeout_sec));
        }

        let mut manager = Self {
            state_dir,
            sandboxes_root,
            registry_path,
            idle_timeout_sec,
            apps: BTreeMap::new(),
            runtimes,
            session_to_subsystem: BTreeMap::new(),
            session_counter: 0,
        };
        manager.load_registry()?;
        Ok(manager)
    }

    fn load_registry(&mut self) -> Result<(), String> {
        if !self.registry_path.is_file() {
            return Ok(());
        }
        let raw = fs::read_to_string(&self.registry_path)
            .map_err(|e| format!("Failed to read registry: {e}"))?;
        let parsed = parse_json(&raw)?;
        let Some(apps_obj) = parsed
            .as_object()
            .and_then(|root| root.get("apps"))
            .and_then(JsonValue::as_object)
        else {
            return Ok(());
        };
        for (app_id, entry) in apps_obj {
            self.apps
                .insert(app_id.clone(), AppSandbox::from_json(entry)?);
        }
        Ok(())
    }

    fn save_registry(&self) -> Result<(), String> {
        let mut apps_map = BTreeMap::new();
        for (app_id, app) in &self.apps {
            apps_map.insert(app_id.clone(), app.to_json());
        }
        let mut root = BTreeMap::new();
        root.insert("apps".to_string(), JsonValue::Object(apps_map));

        let tmp_path = self.registry_path.with_extension("tmp");
        fs::write(&tmp_path, to_json_pretty(&JsonValue::Object(root)))
            .map_err(|e| format!("Failed to write temp registry: {e}"))?;
        fs::rename(&tmp_path, &self.registry_path)
            .map_err(|e| format!("Failed to persist registry: {e}"))?;
        Ok(())
    }

    fn write_permissions_file(&self, sandbox: &AppSandbox) -> Result<(), String> {
        let perm_path = Path::new(&sandbox.sandbox_dir).join("permissions.json");
        fs::write(perm_path, to_json_pretty(&sandbox.permissions.to_json()))
            .map_err(|e| format!("Failed to write permissions.json: {e}"))
    }

    pub fn install(
        &mut self,
        binary_path: impl AsRef<Path>,
        name: Option<&str>,
    ) -> Result<AppSandbox, String> {
        let path = fs::canonicalize(binary_path.as_ref())
            .map_err(|e| format!("Cannot resolve {}: {e}", binary_path.as_ref().display()))?;
        let info = inspect_binary(&path)?;
        let subsystem = platform_to_subsystem(info.platform)
            .ok_or_else(|| format!("Unsupported binary format: {}", path.display()))?;
        let spec = subsystem_spec(subsystem);

        let app_name = name
            .map(str::to_string)
            .or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "app".to_string());
        let app_id = compute_app_id(&app_name, &path);
        let sandbox_dir = self.sandboxes_root.join(&app_id);

        for subdir in spec.sandbox_subdirs {
            fs::create_dir_all(sandbox_dir.join(subdir))
                .map_err(|e| format!("Failed to create sandbox dir: {e}"))?;
        }

        let sandbox = AppSandbox {
            app_id: app_id.clone(),
            name: app_name,
            platform: info.platform,
            arch: info.arch,
            sub_abis: info.sub_abis,
            is_gui: info.is_gui,
            win_era: info.win_era,
            source_path: path.display().to_string(),
            sandbox_dir: sandbox_dir.display().to_string(),
            permissions: PermissionProfile::default(),
            launch_args: Vec::new(),
            env_vars: BTreeMap::new(),
        };

        self.write_permissions_file(&sandbox)?;
        self.apps.insert(app_id, sandbox.clone());
        self.save_registry()?;
        Ok(sandbox)
    }

    pub fn get_app(&self, app_id: &str) -> Result<&AppSandbox, String> {
        self.apps
            .get(app_id)
            .ok_or_else(|| format!("Application not registered: {app_id}"))
    }

    pub fn list_apps(&self) -> Vec<&AppSandbox> {
        self.apps.values().collect()
    }

    pub fn set_permission(
        &mut self,
        app_id: &str,
        permission_name: &str,
        allowed: bool,
    ) -> Result<AppSandbox, String> {
        let updated = {
            let sandbox = self
                .apps
                .get_mut(app_id)
                .ok_or_else(|| format!("Application not registered: {app_id}"))?;
            sandbox.permissions.set_permission(permission_name, allowed)?;
            sandbox.clone()
        };
        self.write_permissions_file(&updated)?;
        self.save_registry()?;
        Ok(updated)
    }

    pub fn update_launch_config(
        &mut self,
        app_id: &str,
        launch_args: Vec<String>,
        env_vars: BTreeMap<String, String>,
    ) -> Result<AppSandbox, String> {
        let updated = {
            let sandbox = self
                .apps
                .get_mut(app_id)
                .ok_or_else(|| format!("Application not registered: {app_id}"))?;
            sandbox.launch_args = launch_args;
            sandbox.env_vars = env_vars;
            sandbox.clone()
        };
        self.save_registry()?;
        Ok(updated)
    }

    /// Remove and recreate an empty `drive_c_root` Wine prefix for a Win32/Win16 app without uninstalling it.
    pub fn reset_win32_prefix(&self, app_id: &str) -> Result<PathBuf, String> {
        let app = self.get_app(app_id)?;
        if platform_to_subsystem(app.platform) != Some(SubsystemKind::Win32) {
            return Err(format!(
                "Reset Wine prefix only applies to Win32/Win16 apps, not {}",
                app.platform
            ));
        }
        let canon_root = fs::canonicalize(&self.sandboxes_root)
            .unwrap_or_else(|_| self.sandboxes_root.clone());
        let canon_sandbox = fs::canonicalize(&app.sandbox_dir)
            .map_err(|e| format!("Failed to resolve sandbox dir: {e}"))?;
        if !canon_sandbox.starts_with(&canon_root) {
            return Err("Refusing prefix reset outside sandboxes root.".to_string());
        }
        let prefix = canon_sandbox.join("drive_c_root");
        if prefix.exists() {
            let _ = stop_wineserver_for_prefix(&prefix);
            fs::remove_dir_all(&prefix)
                .map_err(|e| format!("Failed to clear Wine prefix: {e}"))?;
        }
        fs::create_dir_all(&prefix)
            .map_err(|e| format!("Failed to recreate Wine prefix dir: {e}"))?;
        Ok(prefix)
    }

    /// Archive app sandbox user/save state into a compressed tar archive.
    pub fn backup_app_save_data(&self, app_id: &str, dest_archive: &Path) -> Result<PathBuf, String> {
        let app = self.get_app(app_id)?;
        let sandbox_dir = Path::new(&app.sandbox_dir);
        if !sandbox_dir.is_dir() {
            return Err(format!("Sandbox directory missing: {}", sandbox_dir.display()));
        }
        if let Some(parent) = dest_archive.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create backup dir: {e}"))?;
        }
        let status = std::process::Command::new("tar")
            .arg("-czf")
            .arg(dest_archive)
            .arg("-C")
            .arg(sandbox_dir)
            .arg(".")
            .status()
            .map_err(|e| format!("Failed to run tar backup: {e}"))?;
        if !status.success() {
            return Err(format!("tar save-backup exited with status {status}"));
        }
        Ok(dest_archive.to_path_buf())
    }

    /// Restore app sandbox user/save state from a compressed tar archive into its existing sandbox dir.
    pub fn restore_app_save_data(&self, app_id: &str, src_archive: &Path) -> Result<(), String> {
        let app = self.get_app(app_id)?;
        if !src_archive.is_file() {
            return Err(format!("Save archive not found: {}", src_archive.display()));
        }
        let sandbox_dir = Path::new(&app.sandbox_dir);
        fs::create_dir_all(sandbox_dir)
            .map_err(|e| format!("Failed to prepare sandbox dir: {e}"))?;
        let prefix = sandbox_dir.join("drive_c_root");
        if prefix.is_dir() {
            let _ = stop_wineserver_for_prefix(&prefix);
        }
        let status = std::process::Command::new("tar")
            .arg("-xzf")
            .arg(src_archive)
            .arg("-C")
            .arg(sandbox_dir)
            .status()
            .map_err(|e| format!("Failed to run tar restore: {e}"))?;
        if !status.success() {
            return Err(format!("tar save-restore exited with status {status}"));
        }
        Ok(())
    }

    pub fn set_permissions(
        &mut self,
        app_id: &str,
        permissions: PermissionProfile,
    ) -> Result<AppSandbox, String> {
        let updated = {
            let sandbox = self
                .apps
                .get_mut(app_id)
                .ok_or_else(|| format!("Application not registered: {app_id}"))?;
            sandbox.permissions = permissions;
            sandbox.clone()
        };
        self.write_permissions_file(&updated)?;
        self.save_registry()?;
        Ok(updated)
    }

    pub fn set_win_era(&mut self, app_id: &str, era: Option<WinEra>) -> Result<AppSandbox, String> {
        let updated = {
            let sandbox = self
                .apps
                .get_mut(app_id)
                .ok_or_else(|| format!("Application not registered: {app_id}"))?;
            if platform_to_subsystem(sandbox.platform) != Some(SubsystemKind::Win32) {
                return Err(format!(
                    "Windows era override only applies to Win32/Win16 apps, not {}",
                    sandbox.platform
                ));
            }
            sandbox.win_era = era.or(Some(WinEra::Win10));
            sandbox.clone()
        };
        self.save_registry()?;
        Ok(updated)
    }

    pub fn uninstall(&mut self, app_id: &str) -> Result<AppSandbox, String> {
        let removed = self
            .apps
            .remove(app_id)
            .ok_or_else(|| format!("Application not registered: {app_id}"))?;
        self.save_registry()?;

        let sandbox_path = PathBuf::from(&removed.sandbox_dir);
        if sandbox_path.exists() {
            let canon_root = fs::canonicalize(&self.sandboxes_root)
                .unwrap_or_else(|_| self.sandboxes_root.clone());
            let canon_sandbox = fs::canonicalize(&sandbox_path)
                .map_err(|e| format!("Failed to resolve sandbox dir for removal: {e}"))?;
            if canon_sandbox == canon_root || !canon_sandbox.starts_with(&canon_root) {
                return Err(format!(
                    "Refusing to delete path outside sandbox root: {}",
                    canon_sandbox.display()
                ));
            }
            let wine_prefix = canon_sandbox.join("drive_c_root");
            if wine_prefix.is_dir() && stop_wineserver_for_prefix(&wine_prefix) {
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            let mut last_err = None;
            for attempt in 0..5 {
                match fs::remove_dir_all(&canon_sandbox) {
                    Ok(()) => {
                        last_err = None;
                        break;
                    }
                    Err(e) => {
                        last_err = Some(e);
                        if attempt < 4 {
                            std::thread::sleep(std::time::Duration::from_millis(120));
                        }
                    }
                }
            }
            if let Some(err) = last_err {
                return Err(format!(
                    "Failed to remove sandbox dir {}: {err}",
                    canon_sandbox.display()
                ));
            }
        }
        Ok(removed)
    }

    pub fn launch(
        &mut self,
        app_id: &str,
        host_arch: &str,
        now: Option<f64>,
    ) -> Result<ExecutionPlan, String> {
        self.launch_with_era(app_id, host_arch, now, None)
    }

    pub fn launch_with_era(
        &mut self,
        app_id: &str,
        host_arch: &str,
        now: Option<f64>,
        era_override: Option<WinEra>,
    ) -> Result<ExecutionPlan, String> {
        let sandbox = self.get_app(app_id)?.clone();
        let subsystem = platform_to_subsystem(sandbox.platform)
            .ok_or_else(|| format!("Unsupported platform for launch: {}", sandbox.platform))?;
        let spec = subsystem_spec(subsystem);

        self.session_counter += 1;
        let session_id = format!("{app_id}#s{}", self.session_counter);
        if let Some(runtime) = self.runtimes.get_mut(&subsystem) {
            runtime.acquire(&session_id, now);
        }
        self.session_to_subsystem
            .insert(session_id.clone(), subsystem);

        let info = BinaryInfo {
            platform: sandbox.platform,
            arch: sandbox.arch.clone(),
            sub_abis: sandbox.sub_abis.clone(),
            is_gui: sandbox.is_gui,
            is_library: false,
            has_dex: false,
            win_era: sandbox.win_era,
        };
        let translation = if info.requires_cpu_translation(host_arch) {
            Some(format!("{}-to-{host_arch}", sandbox.arch))
        } else {
            None
        };

        let (mut argv, mut env) = build_subsystem_invocation(
            subsystem,
            spec.runtime_binary,
            &sandbox,
            translation.as_deref(),
            era_override,
        );
        for extra in &sandbox.launch_args {
            argv.push(extra.clone());
        }
        for (k, v) in &sandbox.env_vars {
            env.insert(k.clone(), v.clone());
        }
        let flag = |b: bool| if b { "1" } else { "0" }.to_string();
        env.insert(
            "UNILOAD_ALLOW_NETWORK".to_string(),
            flag(sandbox.permissions.network),
        );
        env.insert(
            "UNILOAD_ALLOW_GPU".to_string(),
            flag(sandbox.permissions.gpu_acceleration),
        );
        env.insert(
            "UNILOAD_ALLOW_CAMERA".to_string(),
            flag(sandbox.permissions.camera),
        );
        env.insert(
            "UNILOAD_ALLOW_MIC".to_string(),
            flag(sandbox.permissions.microphone),
        );
        env.insert(
            "UNILOAD_ALLOW_DOCS".to_string(),
            flag(sandbox.permissions.shared_documents),
        );
        env.insert(
            "UNILOAD_SANDBOX_DIR".to_string(),
            sandbox.sandbox_dir.clone(),
        );
        env.insert(
            "UNILOAD_APP_SOURCE".to_string(),
            sandbox.source_path.clone(),
        );
        env.insert(
            "UNILOAD_SANDBOX_CONFINEMENT".to_string(),
            "bwrap".to_string(),
        );

        Ok(ExecutionPlan {
            app_id: sandbox.app_id,
            session_id,
            subsystem,
            runtime_binary: spec.runtime_binary.to_string(),
            sandbox_dir: sandbox.sandbox_dir,
            argv,
            env,
            translation_layer: translation,
            permissions: sandbox.permissions,
        })
    }

    pub fn stop_session(
        &mut self,
        session_id: &str,
        now: Option<f64>,
    ) -> Result<SubsystemState, String> {
        let subsystem = self
            .session_to_subsystem
            .remove(session_id)
            .ok_or_else(|| format!("Unknown active session: {session_id}"))?;
        let runtime = self
            .runtimes
            .get_mut(&subsystem)
            .ok_or_else(|| format!("Missing subsystem runtime: {subsystem}"))?;
        runtime.release(session_id, now);
        Ok(runtime.state)
    }

    pub fn reap_idle(
        &mut self,
        now: Option<f64>,
        reclaim_cold: bool,
    ) -> BTreeMap<SubsystemKind, SubsystemState> {
        let mut states = BTreeMap::new();
        for (&kind, runtime) in &mut self.runtimes {
            runtime.evaluate_idle(now);
            if reclaim_cold {
                runtime.reclaim();
            }
            states.insert(kind, runtime.state);
        }
        states
    }

    pub fn subsystem_status(&self) -> JsonValue {
        let mut root = BTreeMap::new();
        for (kind, runtime) in &self.runtimes {
            let mut info = BTreeMap::new();
            info.insert(
                "state".to_string(),
                JsonValue::Str(runtime.state.as_str().to_string()),
            );
            info.insert(
                "active_sessions".to_string(),
                JsonValue::Array(
                    runtime
                        .active_sessions
                        .iter()
                        .map(|s| JsonValue::Str(s.clone()))
                        .collect(),
                ),
            );
            info.insert(
                "idle_timeout_sec".to_string(),
                JsonValue::Number(runtime.idle_timeout_sec),
            );
            root.insert(kind.as_str().to_string(), JsonValue::Object(info));
        }
        JsonValue::Object(root)
    }
}

#[cfg(test)]
mod tests {
    use super::sha256_hex;

    #[test]
    fn sha256_matches_nist_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
