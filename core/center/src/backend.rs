//! Application controller bridging the Slint UI with `uniload` sandboxes and host telemetry.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use uniload::hw_profile::{classify_hardware, detect_host_hardware, HardwareProfile};
use uniload::inspector::{inspect_binary, AppPlatform, WinEra};
pub use uniload::sandbox::{AndroidHostStatus, Win32AssetsStatus};
use uniload::sandbox::{
    detect_android_host_status, detect_win32_assets_status, AppSandbox, PermissionProfile,
    SandboxManager,
};

use crate::desktop::{
    default_browse_dirs, platform_display_name, remove_desktop_shortcuts, scan_launchable_files,
    supports_win_era, write_desktop_shortcut, BrowseCandidate,
};

const IDLE_TIMEOUT_SEC: f64 = 30.0;

#[derive(Debug, Clone)]
pub struct CenterConfig {
    pub state_dir: PathBuf,
    pub apps_dir: PathBuf,
    pub desktop_dir: Option<PathBuf>,
    pub uniload_bin: PathBuf,
    pub host_arch: String,
}

#[derive(Debug, Clone)]
pub struct InspectedFileInfo {
    pub name: String,
    pub canonical_path: PathBuf,
    pub platform: AppPlatform,
    pub platform_label: String,
    pub arch: String,
    pub sub_abis_label: String,
    pub is_gui: bool,
    pub supports_era: bool,
    pub win_era: Option<WinEra>,
    pub permissions: PermissionProfile,
    pub already_installed_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InstallRequest {
    pub app_name: Option<String>,
    pub win_era: Option<WinEra>,
    pub permissions: PermissionProfile,
    pub create_shortcut: bool,
}

#[derive(Debug, Clone)]
pub struct SystemOverview {
    pub hw: HardwareProfile,
    pub android: AndroidHostStatus,
    pub win32_assets: Win32AssetsStatus,
}

pub struct CenterBackend {
    pub config: CenterConfig,
}

impl CenterBackend {
    pub fn new(config: CenterConfig) -> Self {
        Self { config }
    }

    fn open_manager(&self) -> Result<SandboxManager, String> {
        SandboxManager::new(&self.config.state_dir, IDLE_TIMEOUT_SEC)
    }

    pub fn list_apps(&self) -> Result<Vec<AppSandbox>, String> {
        let manager = self.open_manager()?;
        Ok(manager.list_apps().into_iter().cloned().collect())
    }

    pub fn get_app(&self, app_id: &str) -> Result<AppSandbox, String> {
        let manager = self.open_manager()?;
        manager.get_app(app_id).cloned()
    }

    pub fn inspect_file(&self, path: &Path) -> Result<InspectedFileInfo, String> {
        let canonical = fs::canonicalize(path)
            .map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
        let info = inspect_binary(&canonical)?;
        let manager = self.open_manager()?;
        let canonical_str = canonical.to_string_lossy().to_string();
        let existing = manager
            .list_apps()
            .into_iter()
            .find(|app| app.source_path == canonical_str)
            .cloned();

        let derived_name = canonical
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("app")
            .to_string();

        let (name, win_era, permissions, existing_id) = match existing {
            Some(app) => (
                app.name,
                app.win_era.or(info.win_era),
                app.permissions,
                Some(app.app_id),
            ),
            None => (
                derived_name,
                info.win_era,
                PermissionProfile::default(),
                None,
            ),
        };

        let sub_abis_label = if info.sub_abis.is_empty() {
            "none".to_string()
        } else {
            info.sub_abis.join(", ")
        };

        Ok(InspectedFileInfo {
            name,
            canonical_path: canonical,
            platform: info.platform,
            platform_label: platform_display_name(info.platform).to_string(),
            arch: info.arch,
            sub_abis_label,
            is_gui: info.is_gui,
            supports_era: supports_win_era(info.platform),
            win_era,
            permissions,
            already_installed_id: existing_id,
        })
    }

    /// Install an application binary, apply requested era/permissions, and optionally write `.desktop` files.
    pub fn install_file(
        &self,
        path: &Path,
        req: &InstallRequest,
    ) -> Result<AppSandbox, String> {
        let mut manager = self.open_manager()?;
        let clean_name = req
            .app_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let mut sandbox = manager.install(path, clean_name)?;

        if supports_win_era(sandbox.platform) {
            if let Some(era) = req.win_era {
                sandbox = manager.set_win_era(&sandbox.app_id, Some(era))?;
            }
        }

        sandbox = manager.set_permissions(&sandbox.app_id, req.permissions.clone())?;

        if req.create_shortcut {
            self.create_shortcuts_for_app(&sandbox)?;
        }

        Ok(sandbox)
    }

    pub fn update_permission(
        &self,
        app_id: &str,
        permission_name: &str,
        allowed: bool,
    ) -> Result<AppSandbox, String> {
        let mut manager = self.open_manager()?;
        manager.set_permission(app_id, permission_name, allowed)
    }

    pub fn update_win_era(&self, app_id: &str, era: WinEra) -> Result<AppSandbox, String> {
        let mut manager = self.open_manager()?;
        manager.set_win_era(app_id, Some(era))
    }

    pub fn uninstall_app(&self, app_id: &str) -> Result<(AppSandbox, usize), String> {
        let mut manager = self.open_manager()?;
        let removed = manager.uninstall(app_id)?;
        let removed_shortcuts = remove_desktop_shortcuts(
            app_id,
            &self.config.apps_dir,
            self.config.desktop_dir.as_deref(),
        );
        Ok((removed, removed_shortcuts.len()))
    }

    pub fn reclaim_subsystems(&self) -> Result<String, String> {
        let output = Command::new(&self.config.uniload_bin)
            .arg("--state-dir")
            .arg(&self.config.state_dir)
            .arg("--host-arch")
            .arg(&self.config.host_arch)
            .arg("reclaim")
            .output()
            .map_err(|e| format!("Failed to invoke uniload reclaim: {e}"))?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Reclaim failed: {err}"));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub fn create_shortcuts_for_app(&self, app: &AppSandbox) -> Result<Vec<PathBuf>, String> {
        let mut written = Vec::new();
        written.push(write_desktop_shortcut(
            app,
            &self.config.apps_dir,
            &self.config.state_dir,
            &self.config.uniload_bin,
        )?);
        if let Some(desktop_dir) = &self.config.desktop_dir {
            // Only populate ~/Desktop when user's Desktop folder exists or is explicitly configured.
            if desktop_dir.is_dir() {
                written.push(write_desktop_shortcut(
                    app,
                    desktop_dir,
                    &self.config.state_dir,
                    &self.config.uniload_bin,
                )?);
            }
        }
        Ok(written)
    }

    /// Build the detached `uniload launch <app_id>` command (separated cleanly for testing).
    pub fn build_launch_command(&self, app_id: &str) -> Command {
        let mut cmd = Command::new(&self.config.uniload_bin);
        cmd.arg("--state-dir")
            .arg(&self.config.state_dir)
            .arg("--host-arch")
            .arg(&self.config.host_arch)
            .arg("launch")
            .arg(app_id)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd
    }

    /// Path to per-app runtime stdout/stderr log written by `unicore-app-runner`.
    pub fn app_log_path(app_id: &str) -> PathBuf {
        let safe: String = app_id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' })
            .collect();
        let file_name = format!("unicore-app-{safe}.log");
        if cfg!(target_os = "linux") {
            PathBuf::from("/tmp").join(file_name)
        } else {
            std::env::temp_dir().join(file_name)
        }
    }

    /// Read the last 60 lines of an application's runtime log for GUI diagnostics.
    pub fn read_app_log(&self, app_id: &str) -> String {
        let path = Self::app_log_path(app_id);
        match fs::read_to_string(&path) {
            Ok(raw) => {
                let lines: Vec<&str> = raw.lines().collect();
                let start = lines.len().saturating_sub(60);
                let tail = lines[start..].join("\n");
                if tail.trim().is_empty() {
                    format!("Log file '{}' is empty.", path.display())
                } else {
                    tail
                }
            }
            Err(_) => format!(
                "No runtime log recorded yet for '{app_id}' ({}). Launch the app first.",
                path.display()
            ),
        }
    }

    /// Collect hardware, boot, network, audio, and kernel diagnostics to Desktop (`unicore-diag.txt`).
    pub fn export_diagnostics(&self) -> Result<String, String> {
        const DIAG_BIN: &str = "/usr/bin/unicore-diag";
        if !cfg!(target_os = "linux") {
            return Err("Hardware diagnostics export is only available inside UniCore OS".to_string());
        }
        if !Path::new(DIAG_BIN).is_file() {
            return Err(format!("Diagnostics utility not found at {DIAG_BIN}"));
        }
        let out = Command::new(DIAG_BIN)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("Failed to run {DIAG_BIN}: {e}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(if err.is_empty() {
                format!("unicore-diag exited with status {}", out.status)
            } else {
                err
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Spawn the sandboxed application without blocking the GUI event loop.
    pub fn spawn_app_detached(&self, app_id: &str) -> Result<(), String> {
        const RUNNER_BIN: &str = "/usr/libexec/unicore-app-runner";
        let manager = self.open_manager()?;
        let app = manager.get_app(app_id)?.clone();
        drop(manager);

        if cfg!(target_os = "linux") && Path::new(RUNNER_BIN).is_file() {
            let mut cmd = Command::new(RUNNER_BIN);
            cmd.arg(&app.app_id)
                .arg(&app.name)
                .arg(&self.config.uniload_bin)
                .arg(&self.config.state_dir)
                .arg(&self.config.host_arch)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            return cmd
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("Failed to launch {app_id}: {e}"));
        }

        let mut cmd = self.build_launch_command(app_id);
        cmd.spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to launch {app_id}: {e}"))
    }

    pub fn browse_candidates(&self) -> Vec<BrowseCandidate> {
        scan_launchable_files(&default_browse_dirs())
    }

    pub fn system_overview(&self) -> SystemOverview {
        SystemOverview {
            hw: classify_hardware(&detect_host_hardware()),
            android: detect_android_host_status(),
            win32_assets: detect_win32_assets_status(),
        }
    }

    /// Spawn Win32/Win16 runtime asset downloader (`install-dxvk`, `install-vkd3d`, `install-otvdm`, `install-cnc-ddraw`) in a terminal.
    pub fn spawn_win32_asset_action(&self, action: &str) -> Result<(), String> {
        if !matches!(
            action,
            "install-dxvk" | "install-vkd3d" | "install-otvdm" | "install-cnc-ddraw"
        ) {
            return Err(format!("Unsupported Win32 asset action: {action}"));
        }
        let uniload = self.config.uniload_bin.display().to_string();
        let shell_cmd = format!(
            "\"{uniload}\" win32 {action}; echo ''; printf 'Done. Press Enter to close...'; read -r _"
        );
        Command::new("foot")
            .args(["-T", "UniCore Win32 Asset Setup", "-e", "/bin/sh", "-c", &shell_cmd])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to start Win32 asset action {action}: {e}"))
    }

    /// Install classic DX9 / VC++ runtimes into the selected Win32 app's sandbox via `winetricks`.
    pub fn spawn_install_legacy_runtimes(&self, app_id: &str) -> Result<(), String> {
        let manager = self.open_manager()?;
        let _app = manager.get_app(app_id)?;
        let uniload = self.config.uniload_bin.display().to_string();
        let state_dir = self.config.state_dir.display().to_string();
        let shell_cmd = format!(
            "\"{uniload}\" --state-dir \"{state_dir}\" win32 install-runtimes \"{app_id}\"; echo ''; printf 'Done. Press Enter to close...'; read -r _"
        );
        Command::new("foot")
            .args([
                "-T",
                "UniCore Classic Game Runtimes",
                "-e",
                "/bin/sh",
                "-c",
                &shell_cmd,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to install classic runtimes for {app_id}: {e}"))
    }

    /// Spawn Android subsystem setup or UI action via `uniload` (inside terminal for setup commands).
    pub fn spawn_android_action(&self, action: &str, profile: &str) -> Result<(), String> {
        let uniload = self.config.uniload_bin.display().to_string();
        let mut cmd = match action {
            "ui" => {
                let mut c = Command::new(&self.config.uniload_bin);
                c.arg("android").arg("ui");
                c
            }
            "init" => {
                let shell_cmd = format!(
                    "\"{uniload}\" android init --profile {profile}; echo ''; printf 'Done. Press Enter to close...'; read -r _"
                );
                let mut c = Command::new("foot");
                c.args(["-T", "UniCore Android Initialization", "-e", "/bin/sh", "-c", &shell_cmd]);
                c
            }
            "setup-arm" => {
                let shell_cmd = format!(
                    "\"{uniload}\" android setup-arm; echo ''; printf 'Done. Press Enter to close...'; read -r _"
                );
                let mut c = Command::new("foot");
                c.args(["-T", "UniCore ARM Translator Setup", "-e", "/bin/sh", "-c", &shell_cmd]);
                c
            }
            "sync-shortcuts" => {
                let mut c = Command::new(&self.config.uniload_bin);
                c.arg("android").arg("sync-shortcuts");
                c
            }
            other => return Err(format!("Unsupported Android GUI action: {other}")),
        };
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to start Android {action}: {e}"))
    }
}
