//! Headless-testable user process inspection and Task Manager termination helpers.

use std::fs;
use std::path::Path;
use std::process::Command;

const MAX_TASKS_DISPLAYED: usize = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub ram_mb: u32,
    pub state: String,
    pub cmdline: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedProcStatus {
    pub name: String,
    pub uid: Option<u32>,
    pub ram_mb: u32,
    pub state: String,
}

/// Parse `/proc/<pid>/status` text into name, real UID, RSS (MB), and state string.
pub fn parse_proc_status(raw: &str) -> ParsedProcStatus {
    let mut name = String::new();
    let mut uid = None;
    let mut rss_kb: u32 = 0;
    let mut state = String::new();

    for line in raw.lines() {
        let Some((key, val_raw)) = line.split_once(':') else {
            continue;
        };
        let val = val_raw.trim();
        match key.trim() {
            "Name" => name = val.to_string(),
            "State" => state = val.to_string(),
            "Uid" => {
                uid = val.split_whitespace().next().and_then(|s| s.parse::<u32>().ok());
            }
            "VmRSS" => {
                if let Some(num) = val.split_whitespace().next().and_then(|s| s.parse::<u32>().ok())
                {
                    rss_kb = num;
                }
            }
            _ => {}
        }
    }

    let ram_mb = if rss_kb == 0 {
        0
    } else {
        (rss_kb + 512) / 1024
    };

    ParsedProcStatus {
        name,
        uid,
        ram_mb,
        state,
    }
}

/// Read the real UID of the current process from `<proc_root>/self/status`.
pub fn current_uid_on_proc(proc_root: &Path) -> Option<u32> {
    let raw = fs::read_to_string(proc_root.join("self").join("status")).ok()?;
    parse_proc_status(&raw).uid
}

/// Read `/proc/<pid>` entries under `proc_root`, optionally filtering by real UID and excluding one PID.
pub fn read_processes_from(
    proc_root: &Path,
    filter_uid: Option<u32>,
    exclude_pid: Option<u32>,
) -> Vec<ProcessInfo> {
    let Ok(entries) = fs::read_dir(proc_root) else {
        return Vec::new();
    };

    let mut list = Vec::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(pid_str) = file_name.to_str() else {
            continue;
        };
        if !pid_str.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        if pid <= 1 || exclude_pid == Some(pid) {
            continue;
        }

        let pid_dir = entry.path();
        let Ok(status_raw) = fs::read_to_string(pid_dir.join("status")) else {
            continue;
        };
        let parsed = parse_proc_status(&status_raw);
        if let Some(expected_uid) = filter_uid {
            if parsed.uid != Some(expected_uid) {
                continue;
            }
        }

        let cmdline_raw = fs::read(pid_dir.join("cmdline")).unwrap_or_default();
        let cmdline = cmdline_raw
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect::<Vec<_>>()
            .join(" ");

        // Skip kernel threads or exited zombies with zero resident memory and empty cmdline
        if parsed.ram_mb == 0 && cmdline.is_empty() {
            continue;
        }

        let name = if parsed.name.is_empty() {
            format!("pid-{pid}")
        } else {
            parsed.name
        };
        let detail = if cmdline.is_empty() {
            parsed.state.clone()
        } else if cmdline.chars().count() > 96 {
            let short: String = cmdline.chars().take(93).collect();
            format!("{short}...")
        } else {
            cmdline
        };

        list.push(ProcessInfo {
            pid,
            name,
            ram_mb: parsed.ram_mb,
            state: parsed.state,
            cmdline: detail,
        });
    }

    list.sort_by(|a, b| b.ram_mb.cmp(&a.ram_mb).then_with(|| a.pid.cmp(&b.pid)));
    list.truncate(MAX_TASKS_DISPLAYED);
    list
}

/// List processes owned by the current desktop user on Linux.
pub fn list_user_processes() -> Vec<ProcessInfo> {
    if !cfg!(target_os = "linux") {
        return Vec::new();
    }
    let proc_root = Path::new("/proc");
    let uid = current_uid_on_proc(proc_root);
    read_processes_from(proc_root, uid, Some(std::process::id()))
}

/// Validate that terminating `pid` is safe and permitted for the current user.
pub fn validate_kill_target(
    pid: u32,
    self_pid: u32,
    target_uid: Option<u32>,
    current_uid: Option<u32>,
) -> Result<(), String> {
    if pid <= 1 {
        return Err("Refusing to terminate PID <= 1 (system init/kernel).".to_string());
    }
    if pid == self_pid {
        return Err("Refusing to terminate UniCore Task Manager itself.".to_string());
    }
    match (target_uid, current_uid) {
        (Some(t), Some(c)) if t == c => Ok(()),
        (Some(t), Some(c)) => Err(format!(
            "Permission denied: PID {pid} belongs to UID {t}, current UID is {c}."
        )),
        _ => Err(format!("Cannot verify process owner for PID {pid}.")),
    }
}

/// Terminate a user-owned process (`SIGTERM` by default, or `SIGKILL` when `force` is true).
pub fn terminate_process(pid: u32, force: bool) -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Err("Task termination is only supported on Linux hosts.".to_string());
    }
    let proc_root = Path::new("/proc");
    let status_path = proc_root.join(pid.to_string()).join("status");
    let status_raw = fs::read_to_string(&status_path)
        .map_err(|_| format!("Process PID {pid} is no longer running."))?;
    let target = parse_proc_status(&status_raw);
    let current_uid = current_uid_on_proc(proc_root);
    validate_kill_target(pid, std::process::id(), target.uid, current_uid)?;

    let signal_arg = if force { "-KILL" } else { "-TERM" };
    let pid_arg = pid.to_string();
    let out = Command::new("kill")
        .args([signal_arg, &pid_arg])
        .output()
        .map_err(|e| format!("Failed to invoke kill for PID {pid}: {e}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("kill {signal_arg} {pid} failed with status {}", out.status)
        } else {
            stderr
        });
    }
    Ok(())
}
