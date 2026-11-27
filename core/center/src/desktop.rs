//! Desktop entry generation, display formatting, and local installer scanning.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use uniload::inspector::{AppPlatform, WinEra};
use uniload::sandbox::AppSandbox;

/// Candidate launchable file discovered in user or system directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseCandidate {
    pub name: String,
    pub path: PathBuf,
    pub kind: String,
}

/// Human-readable English platform name for UI badges and `.desktop` comments.
pub fn platform_display_name(platform: AppPlatform) -> &'static str {
    match platform {
        AppPlatform::WindowsPe => "Windows PE (32/64-bit)",
        AppPlatform::WindowsNe16 => "Windows 16-bit (Win3.x/Win98 NE)",
        AppPlatform::WindowsMsi => "Windows MSI Installer",
        AppPlatform::Dos16 => "MS-DOS Application (16-bit)",
        AppPlatform::AndroidApk => "Android APK Package",
        AppPlatform::NativeElf => "Native Linux App (ELF)",
        AppPlatform::LinuxFlatpak => "Native Linux (Flatpak / Flathub)",
        AppPlatform::Unknown => "Unknown Binary",
    }
}

/// Whether the given platform accepts a Win98 / WinXP / Win10 era override.
pub fn supports_win_era(platform: AppPlatform) -> bool {
    matches!(
        platform,
        AppPlatform::WindowsPe | AppPlatform::WindowsNe16 | AppPlatform::WindowsMsi
    )
}

pub fn win_era_display_name(era: WinEra) -> &'static str {
    match era {
        WinEra::Win98 => "Windows 98 (Win98)",
        WinEra::WinXp => "Windows XP (WinXP)",
        WinEra::Win7 => "Windows 7 (Win7)",
        WinEra::Win10 => "Windows 10 / 11 (Win10)",
    }
}

pub fn win_era_to_index(era: Option<WinEra>) -> i32 {
    match era.unwrap_or(WinEra::Win10) {
        WinEra::Win98 => 0,
        WinEra::WinXp => 1,
        WinEra::Win7 => 2,
        WinEra::Win10 => 3,
    }
}

pub fn win_era_from_index(idx: i32) -> WinEra {
    match idx {
        0 => WinEra::Win98,
        1 => WinEra::WinXp,
        2 => WinEra::Win7,
        _ => WinEra::Win10,
    }
}

/// Freedesktop icon name matching `adwaita-icon-theme`.
pub fn desktop_icon_name(platform: AppPlatform) -> &'static str {
    match platform {
        AppPlatform::AndroidApk => "package-x-generic",
        AppPlatform::Dos16 => "applications-games",
        AppPlatform::WindowsPe | AppPlatform::WindowsNe16 | AppPlatform::WindowsMsi => {
            "application-x-executable"
        }
        AppPlatform::NativeElf | AppPlatform::LinuxFlatpak | AppPlatform::Unknown => "system-run",
    }
}

fn quote_desktop_exec_arg(raw: &str) -> String {
    if raw
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | ':' | '+'))
    {
        return raw.to_string();
    }
    let mut out = String::from("\"");
    for ch in raw.chars() {
        match ch {
            '"' | '$' | '`' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            '%' => out.push_str("%%"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn sanitize_desktop_text(raw: &str) -> String {
    raw.chars()
        .filter(|c| !matches!(c, '\r' | '\n'))
        .collect::<String>()
        .trim()
        .to_string()
}

/// Render an XDG `.desktop` entry that launches the installed sandboxed app directly.
pub fn render_desktop_entry(app: &AppSandbox, state_dir: &Path, uniload_bin: &Path) -> String {
    let name = sanitize_desktop_text(&app.name);
    let era_suffix = match app.win_era {
        Some(era) => format!(" [{}]", win_era_display_name(era)),
        None => String::new(),
    };
    let comment = format!("{}{} via UniCore OS", platform_display_name(app.platform), era_suffix);
    let exec_line = format!(
        "{} --state-dir {} launch {}",
        quote_desktop_exec_arg(&uniload_bin.display().to_string()),
        quote_desktop_exec_arg(&state_dir.display().to_string()),
        quote_desktop_exec_arg(&app.app_id)
    );
    let terminal = if app.is_gui { "false" } else { "true" };

    format!(
        "[Desktop Entry]\nType=Application\nVersion=1.0\nName={name}\nComment={comment}\nExec={exec_line}\nIcon={}\nTerminal={terminal}\nCategories=Utility;\nStartupNotify=true\n",
        desktop_icon_name(app.platform)
    )
}

/// Write `<app_id>.desktop` inside `target_dir` and mark executable on Unix.
pub fn write_desktop_shortcut(
    app: &AppSandbox,
    target_dir: &Path,
    state_dir: &Path,
    uniload_bin: &Path,
) -> Result<PathBuf, String> {
    fs::create_dir_all(target_dir).map_err(|e| {
        format!(
            "Failed to create shortcut directory {}: {e}",
            target_dir.display()
        )
    })?;
    let file_path = target_dir.join(format!("unicore-app-{}.desktop", app.app_id));
    let content = render_desktop_entry(app, state_dir, uniload_bin);
    fs::write(&file_path, content)
        .map_err(|e| format!("Failed to write desktop shortcut {}: {e}", file_path.display()))?;

    set_executable(&file_path);

    Ok(file_path)
}

/// Remove generated `.desktop` shortcuts for `app_id` from Start Menu and Desktop.
pub fn remove_desktop_shortcuts(
    app_id: &str,
    apps_dir: &Path,
    desktop_dir: Option<&Path>,
) -> Vec<PathBuf> {
    let filename = format!("unicore-app-{app_id}.desktop");
    let mut removed = Vec::new();
    let mut candidates = vec![apps_dir.join(&filename)];
    if let Some(desk) = desktop_dir {
        candidates.push(desk.join(&filename));
    }
    for path in candidates {
        if path.is_file() && fs::remove_file(&path).is_ok() {
            removed.push(path);
        }
    }
    removed
}

/// Scan a shallow set of directories for `.exe`, `.msi`, `.apk` binaries.
pub fn scan_launchable_files(dirs: &[PathBuf]) -> Vec<BrowseCandidate> {
    let mut results = Vec::new();
    let mut seen = BTreeSet::new();
    for dir in dirs {
        scan_dir_depth(dir, 2, &mut seen, &mut results);
    }
    results.sort_by_key(|a| a.name.to_lowercase());
    results
}

fn scan_dir_depth(
    dir: &Path,
    depth: u8,
    seen: &mut BTreeSet<PathBuf>,
    out: &mut Vec<BrowseCandidate>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.starts_with('.') {
                    scan_dir_depth(&path, depth - 1, seen, out);
                }
            }
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let kind = match ext.as_str() {
            "exe" => "Windows / DOS EXE",
            "com" => "MS-DOS COM",
            "bat" => "MS-DOS Batch",
            "msi" => "Windows MSI",
            "apk" => "Android APK",
            _ => continue,
        };
        let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !seen.insert(key) {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("app")
            .to_string();
        out.push(BrowseCandidate {
            name,
            path,
            kind: kind.to_string(),
        });
    }
}

pub fn default_state_dir() -> PathBuf {
    if let Ok(env_dir) = std::env::var("UNILOAD_STATE_DIR") {
        if !env_dir.trim().is_empty() {
            return PathBuf::from(env_dir);
        }
    }
    let data_dir = PathBuf::from("/data/uniload");
    if cfg!(target_os = "linux") && Path::new("/data").is_dir() {
        return data_dir;
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".uniload_state")
}

pub fn default_apps_dir() -> PathBuf {
    if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
        if !data_home.trim().is_empty() {
            return PathBuf::from(data_home).join("applications");
        }
    }
    home_dir()
        .join(".local")
        .join("share")
        .join("applications")
}

pub fn default_desktop_dir() -> Option<PathBuf> {
    let desktop = home_dir().join("Desktop");
    Some(desktop)
}

pub fn default_browse_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let home = home_dir();
    for candidate in [
        PathBuf::from("/usr/share/unicore"),
        home.join("Downloads"),
        home.join("Documents"),
        home.join("Desktop"),
        home,
    ] {
        if candidate.is_dir() {
            dirs.push(candidate);
        }
    }
    dirs
}

pub fn default_uniload_bin() -> PathBuf {
    if cfg!(target_os = "linux") && Path::new("/usr/bin/uniload").is_file() {
        PathBuf::from("/usr/bin/uniload")
    } else {
        PathBuf::from("uniload")
    }
}

pub(crate) fn set_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(path, perms);
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

pub(crate) fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
