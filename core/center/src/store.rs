//! Backend helpers for UniCore OS 1-Click App Catalog & Storefront.

use std::path::Path;

use uniload::catalog::{
    filter_catalog_apps, find_catalog_app, install_catalog_app, load_catalog,
    sync_catalog_from_url, CatalogApp, DEFAULT_DATA_CATALOG,
};
use uniload::inspector::{AppPlatform, WinEra};
use uniload::sandbox::{AppSandbox, SandboxManager};

use crate::backend::{CenterBackend, CenterConfig};
use crate::desktop::platform_display_name;

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
    pub is_installed: bool,
    pub installed_app_id: String,
}

/// Derive a crisp monogram icon label and accent RGB color for a Store app tile.
pub fn store_icon_meta(id: &str, name: &str, category: &str) -> (String, (u8, u8, u8)) {
    match id {
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
        "libreoffice" => ("LO".to_string(), (22, 163, 74)),
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

pub fn format_sha256_preview(sha256: &str) -> String {
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

/// Build UI view models for catalog apps matching `category` and `query`.
pub fn load_store_views(
    apps: &[AppSandbox],
    state_dir: &Path,
    category: &str,
    query: &str,
    is_ms: bool,
    catalog_override: Option<&Path>,
) -> Result<Vec<StoreItemView>, String> {
    let catalog = load_catalog(catalog_override)?;
    let filtered = filter_catalog_apps(&catalog.apps, category, query);
    Ok(filtered
        .iter()
        .map(|cat| {
            let matched = find_installed_match(apps, cat, state_dir);
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
                sha256_short: format_sha256_preview(&cat.sha256),
                icon_text,
                accent_rgb,
                is_offline: cat.is_offline_bundled(),
                is_installed: matched.is_some(),
                installed_app_id: matched.map(|m| m.app_id.clone()).unwrap_or_default(),
            }
        })
        .collect())
}

/// Download/verify/extract a catalog app and register both its sandbox and `.desktop` shortcuts.
pub fn install_store_app_blocking(
    config: &CenterConfig,
    catalog_id: &str,
) -> Result<AppSandbox, String> {
    let catalog = load_catalog(None)?;
    let entry = find_catalog_app(&catalog, catalog_id)?;
    let mut manager = SandboxManager::new(&config.state_dir, 30.0)?;
    let sandbox = install_catalog_app(&mut manager, entry, &config.state_dir.join("store"), false)?;
    CenterBackend::new(config.clone()).create_shortcuts_for_app(&sandbox)?;
    Ok(sandbox)
}

/// Download and validate a custom HTTPS `catalog.json` to `/data/unicore/catalog.json`.
pub fn sync_store_catalog_blocking(url: &str) -> Result<usize, String> {
    let catalog = sync_catalog_from_url(url, Path::new(DEFAULT_DATA_CATALOG), false)?;
    Ok(catalog.apps.len())
}
