//! CLI flag parsing for `unicore-center`.

use std::path::PathBuf;

use crate::backend::CenterConfig;
use crate::desktop::{
    default_apps_dir, default_desktop_dir, default_state_dir, default_uniload_bin,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchMode {
    FullCenter,
    OpenDialog(PathBuf),
    PowerDialog,
    QuickSettings,
    LoginGreeter,
    OobeWizard,
    TaskManager,
    AiAssistant,
    CaptureTool,
    StartMenu,
    EmojiPicker,
    CheckOnly(Option<PathBuf>),
}

#[derive(Debug, Clone)]
pub struct ParsedCli {
    pub mode: LaunchMode,
    pub initial_tab: i32,
    pub initial_settings_subtab: i32,
    pub initial_store_category: String,
    pub config: CenterConfig,
}

pub fn help_text() -> &'static str {
    "\
unicore-center - UniCore OS GUI App & Security Center

USAGE:
  unicore-center [OPTIONS] [--open <FILE> | --power | --taskmgr]

OPTIONS:
  --open <FILE>          Open Quick Install & Launch dialog for .exe/.msi/.apk/ELF
  --power                Open Shutdown / Reboot / Sleep / Log Out modal dialog
  --quick-settings       Open Win11-style Quick Settings popup panel (Wi-Fi, Audio, Brightness, Theme, Power)
  --login                Open Multi-User Sign-In screen
  --oobe                 Open First-Boot Setup Wizard (OOBE) before entering desktop center
  --taskmgr              Open lightweight Task Manager to inspect or end running apps
  --ai                   Open Multi-Provider System AI Copilot drawer (Ollama / Gemini / OpenAI)
  --capture              Open Screen Capture & MP4 Video Recorder tool (grim + slurp + wf-recorder)
  --start-menu           Open Windows 11 / Zorin-style graphical Start Menu
  --emoji                Open Windows 11-style Emoji Picker (Win+.)
  --tab <NAME>           Open initial tab: apps | store[:retro] | dev | ai | install | system | settings[:wifi|bluetooth|audio|display|appearance|printers|security|power|users]
  --state-dir <DIR>      Override sandbox registry root (default: /data/uniload or .uniload_state)
  --apps-dir <DIR>       Override .desktop applications folder (default: ~/.local/share/applications)
  --desktop-dir <DIR>    Override Desktop shortcuts folder (default: ~/Desktop)
  --uniload-bin <PATH>   Override uniload binary path (default: /usr/bin/uniload)
  --host-arch <ARCH>     Host CPU arch (default: x86_64)
  --check [FILE]         Headless self-check (inspects FILE if provided) without opening display
  -h, --help             Show this help"
}

fn next_value(args: &[String], idx: &mut usize, flag: &str) -> Result<String, String> {
    *idx += 1;
    args.get(*idx)
        .cloned()
        .ok_or_else(|| format!("Flag '{flag}' requires a value"))
}

pub fn parse_args(args: &[String]) -> Result<Option<ParsedCli>, String> {
    let mut mode = LaunchMode::FullCenter;
    let mut initial_tab = 0i32;
    let mut initial_settings_subtab = 0i32;
    let mut initial_store_category = "all".to_string();
    let mut state_dir = default_state_dir();
    let mut apps_dir = default_apps_dir();
    let mut desktop_dir = default_desktop_dir();
    let mut uniload_bin = default_uniload_bin();
    let mut host_arch = std::env::consts::ARCH.to_string();
    let mut positional_file: Option<PathBuf> = None;

    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "-h" | "--help" => return Ok(None),
            "--open" => {
                let path = PathBuf::from(next_value(args, &mut idx, "--open")?);
                mode = LaunchMode::OpenDialog(path);
            }
            "--power" => {
                mode = LaunchMode::PowerDialog;
            }
            "--quick-settings" => {
                mode = LaunchMode::QuickSettings;
            }
            "--login" => {
                mode = LaunchMode::LoginGreeter;
            }
            "--oobe" => {
                mode = LaunchMode::OobeWizard;
            }
            "--taskmgr" => {
                mode = LaunchMode::TaskManager;
            }
            "--ai" => {
                mode = LaunchMode::AiAssistant;
            }
            "--capture" => {
                mode = LaunchMode::CaptureTool;
            }
            "--start-menu" => {
                mode = LaunchMode::StartMenu;
            }
            "--emoji" => {
                mode = LaunchMode::EmojiPicker;
            }
            "--tab" => {
                let raw = next_value(args, &mut idx, "--tab")?;
                let lower = raw.to_ascii_lowercase();
                if let Some(sub) = lower.strip_prefix("settings:") {
                    initial_tab = 3;
                    initial_settings_subtab = match sub {
                        "wifi" | "network" => 0,
                        "bluetooth" | "bt" => 1,
                        "audio" | "sound" => 2,
                        "display" | "brightness" | "appearance" | "theme" | "wallpaper" | "input" => 3,
                        "printers" | "printer" | "cups" => 4,
                        "security" | "firewall" | "backup" | "vpn" => 5,
                        "power" | "battery" | "users" | "smart" => 6,
                        _ => return Err(format!("Unknown settings subtab '{sub}'. Choose wifi, bluetooth, audio, display, appearance, printers, security, power, or users.")),
                    };
                } else if let Some(sub) = lower.strip_prefix("store:") {
                    initial_tab = 4;
                    initial_store_category = match sub {
                        "all" | "discover" => "all".to_string(),
                        "retro" => "retro".to_string(),
                        "gaming" | "games" => "gaming".to_string(),
                        "creative" | "social" | "dev" | "utilities" | "office" | "android" => {
                            sub.to_string()
                        }
                        _ => return Err(format!("Unknown store category '{sub}'. Choose all, gaming, creative, social, dev, office, android, retro, or utilities.")),
                    };
                } else {
                    initial_tab = match lower.as_str() {
                        "apps" => 0,
                        "install" => 1,
                        "system" => 2,
                        "settings" => 3,
                        "store" | "catalog" => 4,
                        "dev" | "ai" | "devbox" => 5,
                        _ => return Err(format!("Unknown --tab '{raw}'. Choose apps, store, dev, install, system, or settings.")),
                    };
                }
            }
            "--state-dir" => {
                state_dir = PathBuf::from(next_value(args, &mut idx, "--state-dir")?);
            }
            "--apps-dir" => {
                apps_dir = PathBuf::from(next_value(args, &mut idx, "--apps-dir")?);
            }
            "--desktop-dir" => {
                let raw = next_value(args, &mut idx, "--desktop-dir")?;
                desktop_dir = if raw.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(raw))
                };
            }
            "--uniload-bin" => {
                uniload_bin = PathBuf::from(next_value(args, &mut idx, "--uniload-bin")?);
            }
            "--host-arch" => {
                host_arch = next_value(args, &mut idx, "--host-arch")?;
            }
            "--check" => {
                let maybe_next = args.get(idx + 1).filter(|s| !s.starts_with('-')).cloned();
                if maybe_next.is_some() {
                    idx += 1;
                }
                mode = LaunchMode::CheckOnly(maybe_next.map(PathBuf::from));
            }
            other if other.starts_with('-') => {
                return Err(format!("Unknown flag '{other}'. Run with --help for usage."));
            }
            other => {
                if positional_file.is_some() {
                    return Err(format!("Unexpected extra argument '{other}'"));
                }
                positional_file = Some(PathBuf::from(other));
            }
        }
        idx += 1;
    }

    if let Some(file) = positional_file {
        if matches!(mode, LaunchMode::FullCenter) {
            mode = LaunchMode::OpenDialog(file);
        }
    }

    Ok(Some(ParsedCli {
        mode,
        initial_tab,
        initial_settings_subtab,
        initial_store_category,
        config: CenterConfig {
            state_dir,
            apps_dir,
            desktop_dir,
            uniload_bin,
            host_arch,
        },
    }))
}
