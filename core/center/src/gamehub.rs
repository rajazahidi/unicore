//! UniCore OS v0.9.0 Unified Game Hub, Gamepad Controller Manager & Save Backup helpers.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use uniload::inspector::{AppPlatform, WinEra};
use uniload::sandbox::AppSandbox;

pub const GAMEPAD_HELPER_BIN: &str = "/usr/libexec/unicore-gamepad";
pub const GAMESAVE_HELPER_BIN: &str = "/usr/libexec/unicore-gamesave";

// ============================================================================
// 1. Unified Game Library Hub (98/XP + DOS + Steam + Heroic/Lutris + Android)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameHubEntry {
    pub id: String,
    pub name: String,
    pub platform_badge: String, // "Win98" | "WinXP" | "DOS" | "Win32" | "Steam" | "Launcher" | "Android"
    pub category: String,       // "classic" | "modern" | "android"
    pub exec_cmd: String,
    pub sandbox_app_id: String, // non-empty if backed by uniload sandbox (supports save backup & retro tune)
    pub badge_text: String,
    pub accent_hex: String,
    pub favorite: bool,
    pub playtime_secs: u64,
    pub playtime_label: String,
}

pub fn format_playtime(secs: u64) -> String {
    if secs < 60 {
        return "New".to_string();
    }
    let mins = secs / 60;
    if mins < 60 {
        format!("{mins}m played")
    } else {
        let hrs = mins / 60;
        let rem = mins % 60;
        format!("{hrs}h {rem:02}m")
    }
}

fn monogram_badge(name: &str) -> String {
    let chars: String = name
        .split_whitespace()
        .filter_map(|w| w.chars().find(|c| c.is_ascii_alphanumeric()))
        .take(2)
        .collect::<String>()
        .to_ascii_uppercase();
    if chars.is_empty() {
        "GM".to_string()
    } else {
        chars
    }
}

pub fn default_gamehub_conf_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(home).join(".config").join("unicore");
        if p.parent().is_some_and(|d| d.exists()) {
            return p;
        }
    }
    PathBuf::from("/data/home/.config/unicore")
}

pub fn load_favorites(conf_dir: &Path) -> HashSet<String> {
    let Ok(raw) = fs::read_to_string(conf_dir.join("game-favorites.list")) else {
        return HashSet::new();
    };
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn toggle_favorite(conf_dir: &Path, game_id: &str) -> Result<bool, String> {
    let clean = game_id.trim();
    if clean.is_empty() {
        return Err("Empty game ID".to_string());
    }
    let mut favs = load_favorites(conf_dir);
    let now_fav = if favs.contains(clean) {
        favs.remove(clean);
        false
    } else {
        favs.insert(clean.to_string());
        true
    };
    fs::create_dir_all(conf_dir).map_err(|e| format!("Failed creating config dir: {e}"))?;
    let mut sorted: Vec<String> = favs.into_iter().collect();
    sorted.sort();
    fs::write(conf_dir.join("game-favorites.list"), sorted.join("\n") + "\n")
        .map_err(|e| format!("Failed saving favorites: {e}"))?;
    Ok(now_fav)
}

pub fn parse_playtime_tsv(raw: &str) -> BTreeMap<String, u64> {
    let mut map = BTreeMap::new();
    for line in raw.lines() {
        let Some((k, v)) = line.split_once('\t') else {
            continue;
        };
        if let Ok(secs) = v.trim().parse::<u64>() {
            map.insert(k.trim().to_string(), secs);
        }
    }
    map
}

pub fn load_playtime_map(conf_dir: &Path) -> BTreeMap<String, u64> {
    fs::read_to_string(conf_dir.join("playtime.tsv"))
        .map(|s| parse_playtime_tsv(&s))
        .unwrap_or_default()
}

/// Parse a Valve Steam `appmanifest_<id>.acf` file and return `(appid, name)` if it is a playable game.
pub fn parse_steam_acf(raw: &str) -> Option<(String, String)> {
    let mut appid = String::new();
    let mut name = String::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        let parts: Vec<&str> = trimmed.split('"').collect();
        if parts.len() >= 4 {
            match parts[1] {
                "appid" => appid = parts[3].trim().to_string(),
                "name" => name = parts[3].trim().to_string(),
                _ => {}
            }
        }
    }
    if appid.is_empty() || name.is_empty() {
        return None;
    }
    let lower = name.to_ascii_lowercase();
    if lower.contains("proton ")
        || lower.contains("steam linux runtime")
        || lower.contains("steamworks common redistributables")
    {
        return None;
    }
    Some((appid, name))
}

pub fn scan_steam_acf_dir(steamapps_dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(steamapps_dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let fname = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if fname.starts_with("appmanifest_") && fname.ends_with(".acf") {
            if let Ok(raw) = fs::read_to_string(&p) {
                if let Some(pair) = parse_steam_acf(&raw) {
                    out.push(pair);
                }
            }
        }
    }
    out.sort_by_key(|a| a.1.to_ascii_lowercase());
    out
}

/// Combine all sandbox games (98/XP/DOS/Android), installed Steam ACF titles, and gaming Flatpak launchers.
pub fn discover_unified_games(
    sandbox_apps: &[AppSandbox],
    steam_dirs: &[&Path],
    conf_dir: &Path,
) -> Vec<GameHubEntry> {
    let favs = load_favorites(conf_dir);
    let playtimes = load_playtime_map(conf_dir);
    let mut games = Vec::new();
    let mut seen_ids = HashSet::new();

    for app in sandbox_apps {
        let (pbadge, cat, accent) = match (app.platform, app.win_era) {
            (AppPlatform::Dos16, _) => ("MS-DOS", "classic", "#b45309"),
            (AppPlatform::WindowsNe16, _) => ("Win16", "classic", "#7c3aed"),
            (AppPlatform::WindowsPe, Some(WinEra::Win98)) => ("Win98", "classic", "#9333ea"),
            (AppPlatform::WindowsPe, Some(WinEra::WinXp)) => ("WinXP", "classic", "#7c3aed"),
            (AppPlatform::WindowsPe, Some(WinEra::Win7)) => ("Win7", "modern", "#2563eb"),
            (AppPlatform::WindowsPe, _) | (AppPlatform::WindowsMsi, _) => {
                ("Win32", "modern", "#0284c7")
            }
            (AppPlatform::AndroidApk, _) => ("Android", "android", "#059669"),
            _ => continue,
        };
        let secs = playtimes.get(&app.app_id).copied().unwrap_or(0);
        if seen_ids.insert(app.app_id.clone()) {
            games.push(GameHubEntry {
                id: app.app_id.clone(),
                name: app.name.clone(),
                platform_badge: pbadge.to_string(),
                category: cat.to_string(),
                exec_cmd: format!("/usr/libexec/unicore-app-runner {}", app.app_id),
                sandbox_app_id: app.app_id.clone(),
                badge_text: monogram_badge(&app.name),
                accent_hex: accent.to_string(),
                favorite: favs.contains(&app.app_id),
                playtime_secs: secs,
                playtime_label: format_playtime(secs),
            });
        }
    }

    for dir in steam_dirs {
        for (appid, name) in scan_steam_acf_dir(dir) {
            let gid = format!("steam-{appid}");
            if seen_ids.insert(gid.clone()) {
                let secs = playtimes.get(&gid).copied().unwrap_or(0);
                games.push(GameHubEntry {
                    id: gid.clone(),
                    name: name.clone(),
                    platform_badge: "Steam".to_string(),
                    category: "modern".to_string(),
                    exec_cmd: format!(
                        "flatpak run com.valvesoftware.Steam steam://rungameid/{appid}"
                    ),
                    sandbox_app_id: String::new(),
                    badge_text: monogram_badge(&name),
                    accent_hex: "#1d4ed8".to_string(),
                    favorite: favs.contains(&gid),
                    playtime_secs: secs,
                    playtime_label: format_playtime(secs),
                });
            }
        }
    }

    // Include installed Flatpak game launchers (Steam, Heroic, Lutris, Prism, RetroArch)
    const FLATPAK_LAUNCHERS: &[(&str, &str, &str, &str, &str)] = &[
        ("com.valvesoftware.Steam", "Steam Library", "Steam", "modern", "#1e3a8a"),
        ("com.heroicgameslauncher.hgl", "Heroic (Epic / GOG)", "Heroic", "modern", "#0369a1"),
        ("net.lutris.Lutris", "Lutris Games", "Lutris", "modern", "#d97706"),
        ("org.prismlauncher.PrismLauncher", "Prism (Minecraft)", "Minecraft", "modern", "#15803d"),
        ("org.libretro.RetroArch", "RetroArch Consoles", "Retro", "classic", "#9333ea"),
    ];
    let fp_exports = Path::new("/data/flatpak/exports/share/applications");
    for (fp_id, label, pbadge, cat, hex) in FLATPAK_LAUNCHERS {
        if fp_exports.join(format!("{fp_id}.desktop")).is_file() {
            let gid = format!("flatpak-{fp_id}");
            if seen_ids.insert(gid.clone()) {
                let secs = playtimes.get(&gid).copied().unwrap_or(0);
                games.push(GameHubEntry {
                    id: gid.clone(),
                    name: (*label).to_string(),
                    platform_badge: (*pbadge).to_string(),
                    category: (*cat).to_string(),
                    exec_cmd: format!("flatpak run {fp_id}"),
                    sandbox_app_id: String::new(),
                    badge_text: monogram_badge(label),
                    accent_hex: (*hex).to_string(),
                    favorite: favs.contains(&gid),
                    playtime_secs: secs,
                    playtime_label: format_playtime(secs),
                });
            }
        }
    }

    games.sort_by(|a, b| {
        b.favorite
            .cmp(&a.favorite)
            .then_with(|| b.playtime_secs.cmp(&a.playtime_secs))
            .then_with(|| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()))
    });
    games
}

pub fn filter_games(all: &[GameHubEntry], category: &str, query: &str) -> Vec<GameHubEntry> {
    let cat = category.trim().to_ascii_lowercase();
    let q = query.trim().to_ascii_lowercase();
    all.iter()
        .filter(|g| {
            let cat_ok = match cat.as_str() {
                "" | "all" => true,
                "favorites" | "fav" => g.favorite,
                other => g.category == other,
            };
            let q_ok = q.is_empty()
                || g.name.to_ascii_lowercase().contains(&q)
                || g.platform_badge.to_ascii_lowercase().contains(&q);
            cat_ok && q_ok
        })
        .cloned()
        .collect()
}

// ============================================================================
// 2. Gamepad Manager & Pure-Rust uinput Mapper Controls
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamepadDevice {
    pub dev_path: String,
    pub name: String,
    pub vendor_kind: String,
    pub battery: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamepadOverview {
    pub evtest_available: bool,
    pub uinput_ready: bool,
    pub mapper_running: bool,
    pub mapper_profile: String,
    pub controllers: Vec<GamepadDevice>,
}

impl Default for GamepadOverview {
    fn default() -> Self {
        Self {
            evtest_available: false,
            uinput_ready: false,
            mapper_running: false,
            mapper_profile: "off".to_string(),
            controllers: Vec::new(),
        }
    }
}

pub fn mapper_profile_from_index(idx: i32) -> &'static str {
    match idx {
        1 => "arrows_space",
        2 => "dos_classic",
        _ => "wasd_mouse",
    }
}

pub fn mapper_profile_to_index(raw: &str) -> i32 {
    match raw.trim() {
        "arrows_space" => 1,
        "dos_classic" => 2,
        _ => 0,
    }
}

pub fn parse_gamepad_status(raw: &str) -> GamepadOverview {
    let mut out = GamepadOverview::default();
    for line in raw.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let val = v.trim();
        match k.trim() {
            "evtest_available" => out.evtest_available = val == "1",
            "uinput_ready" => out.uinput_ready = val == "1",
            "mapper_running" => out.mapper_running = val == "1",
            "mapper_profile" => out.mapper_profile = val.to_string(),
            "controllers" => {
                out.controllers.clear();
                for item in val.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                    let parts: Vec<&str> = item.split('|').collect();
                    if parts.len() >= 4 {
                        out.controllers.push(GamepadDevice {
                            dev_path: parts[0].trim().to_string(),
                            name: parts[1].trim().to_string(),
                            vendor_kind: parts[2].trim().to_string(),
                            battery: parts[3].trim().to_string(),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn run_helper(bin: &str, args: &[&str]) -> Result<String, String> {
    if !cfg!(target_os = "linux") || !Path::new(bin).is_file() {
        return Err(format!("Helper '{bin}' is only available on UniCore OS Linux."));
    }
    let out = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed running {bin}: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if out.status.success() {
        Ok(stdout)
    } else {
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

pub fn read_gamepad_overview() -> GamepadOverview {
    run_helper(GAMEPAD_HELPER_BIN, &["status"])
        .map(|s| parse_gamepad_status(&s))
        .unwrap_or_default()
}

pub fn gamepad_start_mapper(profile: &str, dev: &str) -> Result<String, String> {
    let target = if dev.trim().is_empty() {
        "/dev/input/js0"
    } else {
        dev.trim()
    };
    run_helper(GAMEPAD_HELPER_BIN, &["start-map", profile, target])
}

pub fn gamepad_stop_mapper() -> Result<String, String> {
    run_helper(GAMEPAD_HELPER_BIN, &["stop-map"])
}

pub fn gamepad_open_tester() -> Result<String, String> {
    run_helper(GAMEPAD_HELPER_BIN, &["open-tester"])
}

// ============================================================================
// 3. Save Game Auto-Backup & Cloud Sync
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameSaveArchive {
    pub filename: String,
    pub size_kb: u64,
    pub full_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameSaveOverview {
    pub backup_dir: String,
    pub auto_backup: bool,
    pub cloud_remote: String,
    pub backup_count: u32,
    pub backups: Vec<GameSaveArchive>,
}

impl Default for GameSaveOverview {
    fn default() -> Self {
        Self {
            backup_dir: "/home/unicore/Backups/GameSaves".to_string(),
            auto_backup: false,
            cloud_remote: String::new(),
            backup_count: 0,
            backups: Vec::new(),
        }
    }
}

pub fn parse_gamesave_status(raw: &str) -> GameSaveOverview {
    let mut out = GameSaveOverview::default();
    for line in raw.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let val = v.trim();
        match k.trim() {
            "backup_dir" => {
                if !val.is_empty() {
                    out.backup_dir = val.to_string();
                }
            }
            "auto_backup" => out.auto_backup = val == "1",
            "cloud_remote" => out.cloud_remote = val.to_string(),
            "backup_count" => out.backup_count = val.parse().unwrap_or(0),
            "backups" => {
                out.backups.clear();
                for item in val.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                    let parts: Vec<&str> = item.split('|').collect();
                    if parts.len() >= 3 {
                        out.backups.push(GameSaveArchive {
                            filename: parts[0].trim().to_string(),
                            size_kb: parts[1].trim().parse().unwrap_or(0),
                            full_path: parts[2].trim().to_string(),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

pub fn read_gamesave_overview() -> GameSaveOverview {
    run_helper(GAMESAVE_HELPER_BIN, &["status"])
        .map(|s| parse_gamesave_status(&s))
        .unwrap_or_default()
}

pub fn gamesave_set_config(auto_backup: bool, cloud_remote: &str) -> Result<String, String> {
    let flag = if auto_backup { "1" } else { "0" };
    let clean = cloud_remote.trim();
    if clean.is_empty() {
        run_helper(GAMESAVE_HELPER_BIN, &["set-config", flag])
    } else {
        run_helper(GAMESAVE_HELPER_BIN, &["set-config", flag, clean])
    }
}

pub fn gamesave_backup_app(app_id: &str) -> Result<String, String> {
    run_helper(GAMESAVE_HELPER_BIN, &["backup", app_id.trim()])
}

pub fn gamesave_sync_cloud() -> Result<String, String> {
    run_helper(GAMESAVE_HELPER_BIN, &["sync-cloud"])
}
