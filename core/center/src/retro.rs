//! Classic Legacy PC Gaming Profile, CD `.iso` / `.zip` Importer,
//! and 1-Click Auto-Tune / Auto-Repair helpers for `unicore-center`.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Stdio};

use uniload::inspector::WinEra;

pub const RETRO_MEDIA_HELPER_BIN: &str = "/usr/libexec/unicore-retro-media";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetroPreset {
    Auto,
    Classic2d,
    Classic3d,
    DosWin16,
    UltraSafe,
}

impl RetroPreset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Classic2d => "classic_2d",
            Self::Classic3d => "classic_3d",
            Self::DosWin16 => "dos_win16",
            Self::UltraSafe => "ultra_safe",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "classic_2d" | "2d" | "win98" => Self::Classic2d,
            "classic_3d" | "3d" | "winxp" => Self::Classic3d,
            "dos_win16" | "dos" | "win16" => Self::DosWin16,
            "ultra_safe" | "safe" | "legacy_safe" => Self::UltraSafe,
            _ => Self::Auto,
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::Auto => 0,
            Self::Classic2d => 1,
            Self::Classic3d => 2,
            Self::DosWin16 => 3,
            Self::UltraSafe => 4,
        }
    }

    pub fn from_index(idx: i32) -> Self {
        match idx {
            1 => Self::Classic2d,
            2 => Self::Classic3d,
            3 => Self::DosWin16,
            4 => Self::UltraSafe,
            _ => Self::Auto,
        }
    }

    /// Simplified 1-click Auto-Tune defaults `(fps_cap_60, single_core_cpu, virtual_desktop_idx)`.
    pub fn auto_tune_defaults(self) -> (bool, bool, i32) {
        match self {
            Self::Auto => (false, false, 0),
            Self::Classic2d => (true, true, 0),
            Self::Classic3d => (true, false, 0),
            Self::DosWin16 => (false, true, 0),
            Self::UltraSafe => (true, true, 1), // 60 FPS + 1 CPU core + 800x600 safe window
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetroGameProfile {
    pub preset: RetroPreset,
    pub fps_cap_60: bool,
    pub single_core_cpu: bool,
    pub virtual_desktop: String, // "off" | "800x600" | "1024x768"
    pub cd_iso_path: String,
}

impl Default for RetroGameProfile {
    fn default() -> Self {
        Self {
            preset: RetroPreset::Auto,
            fps_cap_60: false,
            single_core_cpu: false,
            virtual_desktop: "off".to_string(),
            cd_iso_path: String::new(),
        }
    }
}

pub fn virtual_desktop_to_index(raw: &str) -> i32 {
    match raw.trim() {
        "800x600" => 1,
        "1024x768" => 2,
        _ => 0,
    }
}

pub fn virtual_desktop_from_index(idx: i32) -> &'static str {
    match idx {
        1 => "800x600",
        2 => "1024x768",
        _ => "off",
    }
}

/// Extract the persisted Classic 98/XP gaming profile from an `AppSandbox`'s `env_vars`.
pub fn read_retro_profile_from_env(
    env: &BTreeMap<String, String>,
    era: Option<WinEra>,
) -> RetroGameProfile {
    let preset = env
        .get("UNICORE_RETRO_PRESET")
        .map(|s| RetroPreset::parse(s))
        .unwrap_or(match era {
            Some(WinEra::Win98) => RetroPreset::Classic2d,
            Some(WinEra::WinXp) => RetroPreset::Classic3d,
            _ => RetroPreset::Auto,
        });
    let fps_cap_60 = env
        .get("UNICORE_FPS_CAP_60")
        .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let single_core_cpu = env
        .get("UNICORE_SINGLE_CORE")
        .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let virtual_desktop = env
        .get("UNILOAD_VIRTUAL_DESKTOP")
        .map(|s| s.trim().to_string())
        .filter(|s| matches!(s.as_str(), "off" | "800x600" | "1024x768"))
        .unwrap_or_else(|| "off".to_string());
    let cd_iso_path = env
        .get("UNICORE_CD_ISO")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    RetroGameProfile {
        preset,
        fps_cap_60,
        single_core_cpu,
        virtual_desktop,
        cd_iso_path,
    }
}

/// Write Classic 98/XP tuning overrides cleanly into `env_vars` and return the recommended `WinEra`.
pub fn apply_retro_profile_to_env(
    env: &mut BTreeMap<String, String>,
    profile: &RetroGameProfile,
) -> Option<WinEra> {
    if profile.preset == RetroPreset::Auto {
        env.remove("UNICORE_RETRO_PRESET");
        env.remove("UNILOAD_VK_BACKEND");
    } else {
        env.insert(
            "UNICORE_RETRO_PRESET".to_string(),
            profile.preset.as_str().to_string(),
        );
    }

    let recommended_era = match profile.preset {
        RetroPreset::Classic2d | RetroPreset::UltraSafe => {
            env.insert(
                "UNILOAD_VK_BACKEND".to_string(),
                "ddraw-compat+wined3d".to_string(),
            );
            Some(WinEra::Win98)
        }
        RetroPreset::Classic3d => {
            env.insert(
                "UNILOAD_VK_BACKEND".to_string(),
                "dxvk-d3d8-d3d9".to_string(),
            );
            Some(WinEra::WinXp)
        }
        RetroPreset::DosWin16 => Some(WinEra::Win98),
        RetroPreset::Auto => None,
    };

    let effective_fps = profile.fps_cap_60 || profile.preset == RetroPreset::UltraSafe;
    if effective_fps {
        env.insert("UNICORE_FPS_CAP_60".to_string(), "1".to_string());
    } else {
        env.remove("UNICORE_FPS_CAP_60");
    }

    let effective_single_core = profile.single_core_cpu || profile.preset == RetroPreset::UltraSafe;
    if effective_single_core {
        env.insert("UNICORE_SINGLE_CORE".to_string(), "1".to_string());
    } else {
        env.remove("UNICORE_SINGLE_CORE");
    }

    let vd = profile.virtual_desktop.trim();
    if matches!(vd, "800x600" | "1024x768") {
        env.insert("UNILOAD_VIRTUAL_DESKTOP".to_string(), vd.to_string());
    } else if profile.preset == RetroPreset::UltraSafe {
        env.insert("UNILOAD_VIRTUAL_DESKTOP".to_string(), "800x600".to_string());
    } else if profile.preset == RetroPreset::Classic2d {
        // cnc-ddraw handles crisp 4:3 fullscreen natively without forcing an 800x600 Wine box
        env.insert("UNILOAD_VIRTUAL_DESKTOP".to_string(), "off".to_string());
    } else {
        env.remove("UNILOAD_VIRTUAL_DESKTOP");
    }

    let iso = profile.cd_iso_path.trim();
    if !iso.is_empty() {
        env.insert("UNICORE_CD_ISO".to_string(), iso.to_string());
    } else {
        env.remove("UNICORE_CD_ISO");
    }

    recommended_era
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportedRetroMedia {
    pub game_dir: String,
    pub main_exe: String,
    pub name: String,
    pub iso_path: String,
}

pub fn parse_retro_import_output(raw: &str) -> ImportedRetroMedia {
    let mut out = ImportedRetroMedia::default();
    for line in raw.lines() {
        if let Some((k, v)) = line.split_once('=') {
            let val = v.trim().to_string();
            match k.trim() {
                "game_dir" | "mount_dir" => out.game_dir = val,
                "main_exe" | "autorun_exe" => out.main_exe = val,
                "name" | "label" => out.name = val,
                "iso_path" => out.iso_path = val,
                _ => {}
            }
        }
    }
    out
}

/// Import or prepare a classic game from a `.exe`, `.msi`, Game CD `.iso`, `.zip`/`.7z` archive, or folder.
pub fn prepare_retro_source(src: &str, custom_name: &str) -> Result<ImportedRetroMedia, String> {
    let trimmed = src.trim();
    if trimmed.is_empty() {
        return Err("Select a game .exe, .msi, CD .iso, .zip archive, or folder first.".to_string());
    }
    let p = Path::new(trimmed);
    let lower = trimmed.to_ascii_lowercase();
    let is_direct_bin = p.is_file()
        && (lower.ends_with(".exe")
            || lower.ends_with(".msi")
            || lower.ends_with(".com")
            || lower.ends_with(".bat"));

    if is_direct_bin {
        let name = if !custom_name.trim().is_empty() {
            custom_name.trim().to_string()
        } else {
            p.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Classic Game")
                .to_string()
        };
        return Ok(ImportedRetroMedia {
            game_dir: p
                .parent()
                .map(|d| d.display().to_string())
                .unwrap_or_default(),
            main_exe: trimmed.to_string(),
            name,
            iso_path: String::new(),
        });
    }

    // For a folder on any OS, scan directly in pure Rust if helper binary is not present
    if p.is_dir() && (!cfg!(target_os = "linux") || !Path::new(RETRO_MEDIA_HELPER_BIN).is_file()) {
        let found = uniload::sandbox::scan_playable_exes_in_dir(p);
        let main_exe = found
            .first()
            .map(|e| e.display().to_string())
            .ok_or_else(|| format!("No playable .exe found inside {}", p.display()))?;
        let name = if !custom_name.trim().is_empty() {
            custom_name.trim().to_string()
        } else {
            p.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Classic Game")
                .to_string()
        };
        return Ok(ImportedRetroMedia {
            game_dir: p.display().to_string(),
            main_exe,
            name,
            iso_path: String::new(),
        });
    }

    if !Path::new(RETRO_MEDIA_HELPER_BIN).is_file() {
        return Err(format!(
            "Classic media helper ({RETRO_MEDIA_HELPER_BIN}) is only available on UniCore OS Linux."
        ));
    }

    let mut args = vec!["import-game", trimmed];
    if !custom_name.trim().is_empty() {
        args.push(custom_name.trim());
    }
    let output = Command::new(RETRO_MEDIA_HELPER_BIN)
        .args(&args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to run unicore-retro-media: {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if err.is_empty() {
            format!("Game import failed with status {}", output.status)
        } else {
            err
        });
    }
    let parsed = parse_retro_import_output(&String::from_utf8_lossy(&output.stdout));
    if parsed.main_exe.is_empty() {
        return Err("No .exe or Setup.exe found inside the selected CD/archive.".to_string());
    }
    Ok(parsed)
}
