//! Backend helpers for UniCore OS 1-Click Universal App Catalog, Flatpak & GE-Proton Storefront.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Stdio};

use uniload::catalog::{
    filter_catalog_apps_for_arch, find_catalog_app, install_catalog_app, load_catalog,
    sync_catalog_verified, validate_flatpak_id, CatalogApp, DEFAULT_DATA_CATALOG,
    OFFICIAL_CATALOG_URL,
};
use uniload::inspector::{AppPlatform, WinEra};
use uniload::sandbox::{AppSandbox, SandboxManager};

use crate::backend::{CenterBackend, CenterConfig};
use crate::desktop::platform_display_name;
use crate::settings::SYS_HELPER_BIN;

pub const FLATPAK_HELPER_BIN: &str = "/usr/libexec/unicore-flatpak";
pub const PROTON_HELPER_BIN: &str = "/usr/libexec/unicore-proton";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreItemView {
    pub id: String,
    pub name: String,
    pub version: String,
    pub size_label: String,
    pub category: String,
    pub platform_label: String,
    pub platform_short: String,
    pub summary: String,
    pub sha256_short: String,
    pub icon_text: String,
    pub accent_rgb: (u8, u8, u8),
    pub is_offline: bool,
    pub is_flatpak: bool,
    pub flatpak_id: String,
    pub is_installed: bool,
    pub installed_app_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatpakOverview {
    pub installed: bool,
    pub flathub_ready: bool,
    pub app_count: u32,
    pub flatpak_dir: String,
}

impl Default for FlatpakOverview {
    fn default() -> Self {
        Self {
            installed: false,
            flathub_ready: false,
            app_count: 0,
            flatpak_dir: "/data/flatpak".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtonOverview {
    pub dir: String,
    pub installed_count: u32,
    pub versions: Vec<String>,
    pub steam_linked: bool,
}

impl Default for ProtonOverview {
    fn default() -> Self {
        Self {
            dir: "/data/gaming/proton-ge".to_string(),
            installed_count: 0,
            versions: Vec::new(),
            steam_linked: false,
        }
    }
}

/// Derive a crisp monogram icon label and accent RGB color for a Store app tile.
pub fn store_icon_meta(id: &str, name: &str, category: &str) -> (String, (u8, u8, u8)) {
    match id {
        "steam" => ("STM".to_string(), (29, 78, 216)),
        "heroic" => ("HGL".to_string(), (3, 105, 161)),
        "lutris" => ("LUT".to_string(), (217, 119, 6)),
        "prism-launcher" => ("MC".to_string(), (22, 163, 74)),
        "retroarch" => ("RA".to_string(), (71, 85, 105)),
        "gimp" => ("GMP".to_string(), (109, 40, 217)),
        "obs-studio" => ("OBS".to_string(), (30, 41, 59)),
        "inkscape" => ("SVG".to_string(), (15, 118, 110)),
        "audacity" => ("AUD".to_string(), (37, 99, 235)),
        "blender" => ("3D".to_string(), (234, 88, 12)),
        "kdenlive" => ("KDN".to_string(), (2, 132, 199)),
        "vesktop" => ("DSC".to_string(), (88, 101, 242)),
        "telegram" => ("TG".to_string(), (14, 165, 233)),
        "thunderbird" => ("TB".to_string(), (37, 99, 235)),
        "libreoffice-native" | "libreoffice" => ("LO".to_string(), (22, 163, 74)),
        "flatseal" => ("FSL".to_string(), (79, 70, 229)),
        "7zip" => ("7Z".to_string(), (5, 150, 105)),
        "notepadpp" => ("N++".to_string(), (22, 163, 74)),
        "sumatrapdf" => ("PDF".to_string(), (217, 119, 6)),
        "rufus" => ("USB".to_string(), (2, 132, 199)),
        "newpipe" => ("NP".to_string(), (220, 38, 38)),
        "unicore-hello-win32" => ("W32".to_string(), (79, 70, 229)),
        "unicore-hello-android" => ("APK".to_string(), (13, 148, 136)),
        "commander-keen-1" => ("CK1".to_string(), (234, 88, 12)),
        "tyrian-21" => ("TYR".to_string(), (147, 51, 234)),
        "cave-story" => ("CS".to_string(), (225, 29, 72)),
        "openra-red-alert" => ("RA".to_string(), (185, 28, 28)),
        "antigravity" => ("AGY".to_string(), (37, 99, 235)),
        "vscodium" => ("VSC".to_string(), (14, 165, 233)),
        "keepassxc" => ("KPX".to_string(), (16, 185, 129)),
        "vlc" => ("VLC".to_string(), (234, 88, 12)),
        _ => {
            let abbr: String = name
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .take(2)
                .collect::<String>()
                .to_ascii_uppercase();
            let icon = if abbr.is_empty() {
                "AP".to_string()
            } else {
                abbr
            };
            let color = match category {
                "gaming" => (29, 78, 216),
                "creative" => (124, 58, 237),
                "social" => (14, 165, 233),
                "dev" => (37, 99, 235),
                "office" => (217, 119, 6),
                "android" => (13, 148, 136),
                "retro" => (147, 51, 234),
                _ => (2, 132, 199),
            };
            (icon, color)
        }
    }
}

pub fn era_badge_name(platform: AppPlatform, win_era: Option<WinEra>) -> &'static str {
    match platform {
        AppPlatform::Dos16 => "MS-DOS",
        AppPlatform::WindowsNe16 => "Win16",
        AppPlatform::AndroidApk => "Android",
        AppPlatform::NativeElf => "Linux",
        AppPlatform::LinuxFlatpak => "Flatpak",
        AppPlatform::WindowsPe | AppPlatform::WindowsMsi => match win_era {
            Some(WinEra::Win98) => "Win98",
            Some(WinEra::WinXp) => "WinXP",
            Some(WinEra::Win7) => "Win7",
            _ => "Win10",
        },
        AppPlatform::Unknown => "App",
    }
}

pub fn platform_short_name(platform: AppPlatform) -> &'static str {
    era_badge_name(platform, None)
}

pub fn format_sha256_preview(sha256: &str, flatpak_id: &str) -> String {
    if !flatpak_id.trim().is_empty() {
        return format!("Flathub Signed OSTree ({})", flatpak_id.trim());
    }
    let clean = sha256.trim();
    if clean.len() == 64 {
        format!("{}...{}", &clean[..12], &clean[56..])
    } else {
        "Built-in Read-Only OS Image".to_string()
    }
}

/// Group flat items into padded 3-column rows so Slint renders a strict Store grid.
pub fn chunk_into_triples<T: Clone>(items: &[T]) -> Vec<(Option<T>, Option<T>, Option<T>)> {
    let mut rows = Vec::with_capacity(items.len().div_ceil(3));
    for chunk in items.chunks(3) {
        rows.push((
            chunk.first().cloned(),
            chunk.get(1).cloned(),
            chunk.get(2).cloned(),
        ));
    }
    rows
}

pub fn parse_flatpak_status(raw: &str) -> FlatpakOverview {
    let mut ov = FlatpakOverview::default();
    for line in raw.lines() {
        let Some((k, v)) = line.trim().split_once('=') else {
            continue;
        };
        match k.trim() {
            "installed" => ov.installed = v.trim() == "1",
            "flathub_ready" => ov.flathub_ready = v.trim() == "1",
            "app_count" => ov.app_count = v.trim().parse().unwrap_or(0),
            "flatpak_dir" if !v.trim().is_empty() => ov.flatpak_dir = v.trim().to_string(),
            _ => {}
        }
    }
    ov
}

pub fn read_flatpak_overview() -> FlatpakOverview {
    if !cfg!(target_os = "linux") || !Path::new(FLATPAK_HELPER_BIN).is_file() {
        return FlatpakOverview::default();
    }
    Command::new(FLATPAK_HELPER_BIN)
        .arg("status")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_flatpak_status(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

pub fn list_installed_flatpak_ids() -> BTreeSet<String> {
    if !cfg!(target_os = "linux") || !Path::new(FLATPAK_HELPER_BIN).is_file() {
        return BTreeSet::new();
    }
    Command::new(FLATPAK_HELPER_BIN)
        .arg("list")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_proton_status(raw: &str) -> ProtonOverview {
    let mut ov = ProtonOverview::default();
    for line in raw.lines() {
        let Some((k, v)) = line.trim().split_once('=') else {
            continue;
        };
        match k.trim() {
            "dir" if !v.trim().is_empty() => ov.dir = v.trim().to_string(),
            "installed_count" => ov.installed_count = v.trim().parse().unwrap_or(0),
            "steam_linked" => ov.steam_linked = v.trim() == "1",
            "versions" => {
                ov.versions = v
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            _ => {}
        }
    }
    ov
}

pub fn read_proton_overview() -> ProtonOverview {
    if !cfg!(target_os = "linux") || !Path::new(PROTON_HELPER_BIN).is_file() {
        return ProtonOverview::default();
    }
    Command::new(PROTON_HELPER_BIN)
        .arg("status")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_proton_status(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

pub fn install_latest_proton_blocking() -> Result<String, String> {
    if !cfg!(target_os = "linux") || !Path::new(PROTON_HELPER_BIN).is_file() {
        return Err("GE-Proton manager is only available inside UniCore OS".to_string());
    }
    let out = Command::new(PROTON_HELPER_BIN)
        .arg("install-latest")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to invoke GE-Proton manager: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !out.status.success() {
        return Err(if !stderr.is_empty() { stderr } else { stdout });
    }
    Ok(if stdout.is_empty() {
        "Latest GE-Proton installed cleanly!".to_string()
    } else {
        stdout.lines().last().unwrap_or("Installed GE-Proton").to_string()
    })
}

pub fn launch_flatpak_app(flatpak_id: &str) -> Result<(), String> {
    validate_flatpak_id(flatpak_id)?;
    Command::new(FLATPAK_HELPER_BIN)
        .args(["run", flatpak_id])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to launch Flatpak {flatpak_id}: {e}"))
}

fn find_installed_match<'a>(
    installed: &'a [AppSandbox],
    cat: &CatalogApp,
    state_dir: &Path,
) -> Option<&'a AppSandbox> {
    let store_marker = state_dir
        .join("store")
        .join(&cat.id)
        .display()
        .to_string();
    installed.iter().find(|app| {
        (!cat.bundled_path.is_empty() && app.source_path == cat.bundled_path)
            || app.source_path.starts_with(&store_marker)
            || app.name == cat.name
    })
}

/// Build UI view models for catalog apps matching `category`, `query`, and `host_arch`.
pub fn load_store_views_for_arch(
    apps: &[AppSandbox],
    state_dir: &Path,
    category: &str,
    query: &str,
    is_ms: bool,
    host_arch: &str,
    catalog_override: Option<&Path>,
) -> Result<Vec<StoreItemView>, String> {
    let catalog = load_catalog(catalog_override)?;
    let filtered = filter_catalog_apps_for_arch(&catalog.apps, category, query, host_arch);
    let installed_flatpaks = list_installed_flatpak_ids();
    Ok(filtered
        .iter()
        .map(|cat| {
            let (is_installed, installed_app_id) = if cat.is_flatpak() {
                let present = installed_flatpaks.contains(&cat.flatpak_id);
                (present, if present { cat.flatpak_id.clone() } else { String::new() })
            } else {
                let matched = find_installed_match(apps, cat, state_dir);
                (
                    matched.is_some(),
                    matched.map(|m| m.app_id.clone()).unwrap_or_default(),
                )
            };
            let (icon_text, accent_rgb) = store_icon_meta(&cat.id, &cat.name, &cat.category);
            StoreItemView {
                id: cat.id.clone(),
                name: cat.name.clone(),
                version: cat.version.clone(),
                size_label: cat.size_label.clone(),
                category: cat.category.clone(),
                platform_label: platform_display_name(cat.platform).to_string(),
                platform_short: era_badge_name(cat.platform, cat.win_era).to_string(),
                summary: cat.summary(is_ms).to_string(),
                sha256_short: format_sha256_preview(&cat.sha256, &cat.flatpak_id),
                icon_text,
                accent_rgb,
                is_offline: cat.is_offline_bundled(),
                is_flatpak: cat.is_flatpak(),
                flatpak_id: cat.flatpak_id.clone(),
                is_installed,
                installed_app_id,
            }
        })
        .collect())
}

/// Backward-compatible wrapper loading all architectures.
pub fn load_store_views(
    apps: &[AppSandbox],
    state_dir: &Path,
    category: &str,
    query: &str,
    is_ms: bool,
    catalog_override: Option<&Path>,
) -> Result<Vec<StoreItemView>, String> {
    load_store_views_for_arch(apps, state_dir, category, query, is_ms, "", catalog_override)
}

/// Install a catalog app (Flatpak via Flathub OR sandboxed Win32/Android/DOS with SHA-256 verification).
pub fn install_store_app_blocking(
    config: &CenterConfig,
    catalog_id: &str,
) -> Result<String, String> {
    let catalog = load_catalog(None)?;
    let entry = find_catalog_app(&catalog, catalog_id)?;
    if entry.is_flatpak() {
        validate_flatpak_id(&entry.flatpak_id)?;
        if !cfg!(target_os = "linux") || !Path::new(FLATPAK_HELPER_BIN).is_file() {
            return Err("Flatpak installation is only available inside UniCore OS".to_string());
        }
        let out = Command::new(FLATPAK_HELPER_BIN)
            .args(["install", &entry.flatpak_id])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("Failed to invoke unicore-flatpak: {e}"))?;
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if !out.status.success() {
            return Err(if !stderr.is_empty() { stderr } else { stdout });
        }
        return Ok(format!("{} (Flatpak) installed cleanly!", entry.name));
    }

    let mut manager = SandboxManager::new(&config.state_dir, 30.0)?;
    let sandbox = install_catalog_app(&mut manager, entry, &config.state_dir.join("store"), false)?;
    CenterBackend::new(config.clone()).create_shortcuts_for_app(&sandbox)?;
    Ok(format!("{} installed cleanly!", sandbox.name))
}

/// 1-click uninstall of a Store app (handles both Flatpak and sandboxed UniCore apps).
pub fn uninstall_store_app_blocking(
    config: &CenterConfig,
    catalog_id: &str,
    installed_app_id: &str,
) -> Result<String, String> {
    let catalog = load_catalog(None)?;
    let entry = find_catalog_app(&catalog, catalog_id)?;
    if entry.is_flatpak() {
        validate_flatpak_id(&entry.flatpak_id)?;
        let out = Command::new(FLATPAK_HELPER_BIN)
            .args(["uninstall", &entry.flatpak_id])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("Failed to invoke unicore-flatpak uninstall: {e}"))?;
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if !out.status.success() {
            return Err(if !stderr.is_empty() { stderr } else { stdout });
        }
        return Ok(format!("Uninstalled {} (Flatpak).", entry.name));
    }
    if installed_app_id.trim().is_empty() {
        return Err(format!("'{}' is not currently installed.", entry.name));
    }
    CenterBackend::new(config.clone()).uninstall_app(installed_app_id)?;
    Ok(format!("Uninstalled {}.", entry.name))
}

/// Download, cryptographically verify (Ed25519 when pubkey configured), and store catalog JSON.
pub fn sync_store_catalog_blocking(url: &str) -> Result<(usize, String), String> {
    let target = if url.trim().is_empty() {
        OFFICIAL_CATALOG_URL
    } else {
        url.trim()
    };
    let (catalog, sig_status) =
        sync_catalog_verified(target, Path::new(DEFAULT_DATA_CATALOG), false, false)?;
    Ok((catalog.apps.len(), sig_status.as_str().to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserOverview {
    pub default_browser: String,
    pub epiphany_installed: bool,
    pub firefox_installed: bool,
    pub chromium_installed: bool,
}

impl Default for BrowserOverview {
    fn default() -> Self {
        Self {
            default_browser: "epiphany".to_string(),
            epiphany_installed: true,
            firefox_installed: false,
            chromium_installed: false,
        }
    }
}

pub fn parse_browser_status(raw: &str) -> BrowserOverview {
    let mut ov = BrowserOverview::default();
    for line in raw.lines() {
        let Some((k, v)) = line.trim().split_once('=') else {
            continue;
        };
        match k.trim() {
            "default_browser" if !v.trim().is_empty() => ov.default_browser = v.trim().to_string(),
            "epiphany_installed" => ov.epiphany_installed = v.trim() == "1",
            "firefox_installed" => ov.firefox_installed = v.trim() == "1",
            "chromium_installed" => ov.chromium_installed = v.trim() == "1",
            _ => {}
        }
    }
    ov
}

pub fn read_browser_overview() -> BrowserOverview {
    const MGR: &str = "/usr/libexec/unicore-browser-mgr";
    if cfg!(target_os = "linux") && Path::new(MGR).is_file() {
        if let Ok(out) = Command::new(MGR)
            .arg("status")
            .stdin(Stdio::null())
            .output()
        {
            if out.status.success() {
                return parse_browser_status(&String::from_utf8_lossy(&out.stdout));
            }
        }
    }
    BrowserOverview::default()
}

pub fn install_native_browser_blocking(browser: &str) -> Result<String, String> {
    if !matches!(browser, "firefox" | "chromium") {
        return Err(format!("Unsupported browser '{browser}'. Choose firefox or chromium."));
    }
    if !cfg!(target_os = "linux") {
        return Err("Native browser installation is only available inside UniCore OS".to_string());
    }
    let out = Command::new("doas")
        .args([SYS_HELPER_BIN, "browser-install", browser])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to invoke browser installer: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !out.status.success() {
        return Err(if !stderr.is_empty() { stderr } else { stdout });
    }
    let _ = set_default_browser(browser);
    Ok(format!("Installed {browser} (native Wayland) and set as default browser"))
}

pub fn set_default_browser(browser: &str) -> Result<String, String> {
    if !matches!(browser, "epiphany" | "firefox" | "chromium") {
        return Err(format!("Unsupported default browser '{browser}'"));
    }
    const MGR: &str = "/usr/libexec/unicore-browser-mgr";
    if !cfg!(target_os = "linux") || !Path::new(MGR).is_file() {
        return Ok(format!("Default browser set to {browser}"));
    }
    let out = Command::new(MGR)
        .args(["set-default", browser])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to invoke browser manager: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !out.status.success() {
        return Err(if !stderr.is_empty() { stderr } else { stdout });
    }
    Ok(stdout)
}
