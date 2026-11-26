//! Global Gaming & Performance settings (`gaming.conf` + CPU scaling governor control).

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::oobe::DEFAULT_OOBE_DIR;
use crate::settings::SYS_HELPER_BIN;

pub const GAMING_CONF_NAME: &str = "gaming.conf";
pub const SYS_CPUFREQ_GOV_PATH: &str = "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GamingConfig {
    pub game_mode: bool,
    pub show_fps: bool,
}

pub fn parse_gaming_conf(raw: &str) -> GamingConfig {
    let mut cfg = GamingConfig::default();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((k, v)) = trimmed.split_once('=') else {
            continue;
        };
        let enabled = matches!(v.trim().trim_matches('"'), "1" | "true" | "yes");
        match k.trim() {
            "UNICORE_GAME_MODE" => cfg.game_mode = enabled,
            "UNICORE_SHOW_FPS" => cfg.show_fps = enabled,
            _ => {}
        }
    }
    cfg
}

pub fn render_gaming_conf(cfg: &GamingConfig) -> String {
    format!(
        "UNICORE_GAME_MODE={}\nUNICORE_SHOW_FPS={}\n",
        if cfg.game_mode { 1 } else { 0 },
        if cfg.show_fps { 1 } else { 0 }
    )
}

pub fn load_gaming_config(dir: &Path) -> GamingConfig {
    let conf_path = dir.join(GAMING_CONF_NAME);
    fs::read_to_string(conf_path)
        .map(|raw| parse_gaming_conf(&raw))
        .unwrap_or_default()
}

pub fn save_gaming_config(dir: &Path, cfg: &GamingConfig) -> Result<(), String> {
    fs::create_dir_all(dir)
        .map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
    let conf_path = dir.join(GAMING_CONF_NAME);
    fs::write(&conf_path, render_gaming_conf(cfg))
        .map_err(|e| format!("Failed to write {}: {e}", conf_path.display()))
}

pub fn read_cpu_governor_from(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                "default".to_string()
            } else {
                trimmed.to_string()
            }
        }
        Err(_) => "unavailable (VM / fixed clock)".to_string(),
    }
}

pub fn read_host_cpu_governor() -> String {
    if !cfg!(target_os = "linux") {
        return "host-managed".to_string();
    }
    read_cpu_governor_from(Path::new(SYS_CPUFREQ_GOV_PATH))
}

pub fn apply_cpu_governor(game_mode: bool) -> Result<String, String> {
    if !cfg!(target_os = "linux") {
        return Ok(if game_mode {
            "Game Mode preference saved".to_string()
        } else {
            "Balanced mode saved".to_string()
        });
    }
    let gov = if game_mode { "performance" } else { "schedutil" };
    let out = Command::new("doas")
        .args([SYS_HELPER_BIN, "set-cpu-governor", gov])
        .output()
        .map_err(|e| format!("Failed to invoke CPU governor helper: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("set-cpu-governor failed with status {}", out.status)
        } else {
            stderr
        });
    }
    let msg = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(if msg.is_empty() {
        format!("CPU governor set to {gov}")
    } else {
        msg
    })
}

pub fn default_gaming_dir() -> &'static Path {
    Path::new(DEFAULT_OOBE_DIR)
}
