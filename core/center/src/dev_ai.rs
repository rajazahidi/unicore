//! Developer Podman DevBox, Ollama Local AI helper wrappers & Pure-Rust File Picker.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const DEVBOX_HELPER_BIN: &str = "/usr/libexec/unicore-devbox";
pub const AI_HELPER_BIN: &str = "/usr/libexec/unicore-ai";
pub const DEVBOX_TEMPLATES: [&str; 4] = ["python", "node", "rust", "ubuntu"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevBoxOverview {
    pub git_version: String,
    pub python_version: String,
    pub podman_version: String,
    pub projects_dir: String,
    pub containers: Vec<String>,
}

impl Default for DevBoxOverview {
    fn default() -> Self {
        Self {
            git_version: "unavailable".to_string(),
            python_version: "unavailable".to_string(),
            podman_version: "unavailable".to_string(),
            projects_dir: "/data/dev/projects".to_string(),
            containers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAiOverview {
    pub installed: bool,
    pub running: bool,
    pub endpoint: String,
    pub recommended_model: String,
    pub models: Vec<String>,
}

impl Default for LocalAiOverview {
    fn default() -> Self {
        Self {
            installed: false,
            running: false,
            endpoint: "http://127.0.0.1:11434".to_string(),
            recommended_model: recommend_ai_model_for_ram(4096),
            models: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub detail: String,
}

/// Suggest an appropriate local LLM based on system RAM so low-spec machines avoid OOM.
pub fn recommend_ai_model_for_ram(ram_mb: u64) -> String {
    if ram_mb < 2500 {
        format!("qwen2.5:0.5b (Low-RAM PC warning: {ram_mb} MB)")
    } else if ram_mb < 7500 {
        "llama3.2:1b".to_string()
    } else if ram_mb < 15000 {
        "qwen2.5-coder:3b".to_string()
    } else {
        "qwen2.5-coder:7b".to_string()
    }
}

pub fn validate_devbox_name(raw: &str) -> Result<String, String> {
    let clean = raw.trim();
    if clean.is_empty()
        || clean.len() > 32
        || !clean
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        || !clean
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
    {
        return Err("DevBox name must be 1-32 alphanumeric/'-'/'_' characters.".to_string());
    }
    Ok(clean.to_string())
}

pub fn validate_ai_model_name(raw: &str) -> Result<String, String> {
    let clean = raw.trim();
    if clean.is_empty()
        || clean.len() > 64
        || !clean
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        || !clean
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '/' | '-'))
    {
        return Err("Invalid model name (example: llama3.2:1b or qwen2.5:0.5b).".to_string());
    }
    Ok(clean.to_string())
}

pub fn parse_devbox_status(raw: &str) -> DevBoxOverview {
    let mut ov = DevBoxOverview::default();
    for line in raw.lines() {
        let Some((k, v)) = line.trim().split_once('=') else {
            continue;
        };
        let val = v.trim();
        match k.trim() {
            "git" => ov.git_version = val.to_string(),
            "python" => ov.python_version = val.to_string(),
            "podman" => ov.podman_version = val.to_string(),
            "projects_dir" => ov.projects_dir = val.to_string(),
            "containers" => {
                ov.containers = val
                    .split(';')
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

pub fn parse_ai_status(raw: &str) -> LocalAiOverview {
    let mut ov = LocalAiOverview::default();
    for line in raw.lines() {
        let Some((k, v)) = line.trim().split_once('=') else {
            continue;
        };
        let val = v.trim();
        match k.trim() {
            "installed" => ov.installed = val == "1" || val.eq_ignore_ascii_case("true"),
            "running" => ov.running = val == "1" || val.eq_ignore_ascii_case("true"),
            "endpoint" => ov.endpoint = val.to_string(),
            "recommended_model" => ov.recommended_model = val.to_string(),
            "models" => {
                ov.models = val
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

pub fn read_devbox_overview() -> DevBoxOverview {
    if !cfg!(target_os = "linux") || !Path::new(DEVBOX_HELPER_BIN).is_file() {
        return DevBoxOverview::default();
    }
    Command::new(DEVBOX_HELPER_BIN)
        .arg("status")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_devbox_status(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

pub fn run_devbox_create(template: &str, name: &str) -> Result<String, String> {
    if !DEVBOX_TEMPLATES.contains(&template) {
        return Err(format!("Unsupported DevBox template '{template}'"));
    }
    let clean = validate_devbox_name(name)?;
    let out = Command::new(DEVBOX_HELPER_BIN)
        .args(["create", template, &clean])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to launch unicore-devbox: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            "DevBox create failed".to_string()
        } else {
            err
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn spawn_devbox_shell(name: &str) -> Result<(), String> {
    let clean = validate_devbox_name(name)?;
    Command::new(DEVBOX_HELPER_BIN)
        .args(["shell", &clean])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to open DevBox terminal: {e}"))
}

pub fn remove_devbox(name: &str) -> Result<String, String> {
    let clean = validate_devbox_name(name)?;
    let out = Command::new(DEVBOX_HELPER_BIN)
        .args(["remove", &clean])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to run unicore-devbox remove: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(format!("Removed DevBox '{clean}'"))
}

pub fn read_ai_overview() -> LocalAiOverview {
    if !cfg!(target_os = "linux") || !Path::new(AI_HELPER_BIN).is_file() {
        return LocalAiOverview::default();
    }
    Command::new(AI_HELPER_BIN)
        .arg("status")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_ai_status(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

pub fn run_ai_action(args: &[&str]) -> Result<String, String> {
    let out = Command::new(AI_HELPER_BIN)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to run unicore-ai: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            format!("unicore-ai failed with status {}", out.status)
        } else {
            err
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn default_picker_dir() -> PathBuf {
    if cfg!(target_os = "linux") && Path::new("/data/home").is_dir() {
        PathBuf::from("/data/home")
    } else if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        PathBuf::from(home)
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
    }
}

/// Built-in cross-platform graphical directory listing for File Picker dialog.
pub fn list_directory_entries(dir: &Path) -> Result<(PathBuf, Vec<PickerEntry>), String> {
    let canon = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let read_dir = fs::read_dir(&canon)
        .map_err(|e| format!("Cannot browse {}: {e}", canon.display()))?;

    let mut dirs = Vec::new();
    let mut files = Vec::new();

    if let Some(parent) = canon.parent() {
        dirs.push(PickerEntry {
            name: ".. (Parent Folder)".to_string(),
            path: parent.display().to_string(),
            is_dir: true,
            detail: "DIR".to_string(),
        });
    }

    for entry in read_dir.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            dirs.push(PickerEntry {
                name: format!("[DIR] {name}"),
                path: path.display().to_string(),
                is_dir: true,
                detail: "Folder".to_string(),
            });
        } else if meta.is_file() {
            let kb = meta.len() / 1024;
            let detail = if kb >= 1024 {
                format!("{:.1} MB", kb as f64 / 1024.0)
            } else {
                format!("{kb} KB")
            };
            files.push(PickerEntry {
                name,
                path: path.display().to_string(),
                is_dir: false,
                detail,
            });
        }
    }

    dirs.sort_by_key(|a| a.name.to_lowercase());
    files.sort_by_key(|a| a.name.to_lowercase());
    dirs.extend(files);
    Ok((canon, dirs))
}
