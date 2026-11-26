//! Command-line parser and dispatch for `uniload`.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::catalog::{
    filter_catalog_apps, find_catalog_app, install_catalog_app, load_catalog, sync_catalog_from_url,
    DEFAULT_DATA_CATALOG,
};
use crate::hw_profile::{classify_hardware, detect_host_hardware};
use crate::inspector::{inspect_binary, WinEra};
use crate::json_codec::{to_json_pretty, JsonValue};
use crate::sandbox::{
    build_bwrap_args, detect_android_host_status, detect_win32_assets_status, sha256_file,
    ConfinementContext, SandboxManager, SubsystemKind,
};
use crate::updater::{
    build_update_status_json, compare_versions, generate_ed25519_keypair, generate_manifest,
    load_host_update_config, normalize_arch, parse_os_version, read_boot_slot_info,
    render_manifest_pretty, resolve_artifact_url, sign_manifest_file, validate_update_url,
    verify_manifest_signature, GrubEnv, ManifestBuildParams, Slot, UpdateManifest,
};

const DEFAULT_HOST_ARCH: &str = "x86_64";
const DEFAULT_IDLE_TIMEOUT_SEC: f64 = 30.0;

struct CliOptions {
    state_dir: PathBuf,
    host_arch: String,
    positional: Vec<String>,
}

fn default_state_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".uniload_state")
}

fn validate_host_arch(arch: &str) -> Result<(), String> {
    if matches!(arch, "x86_64" | "arm64" | "aarch64" | "x86") {
        Ok(())
    } else {
        Err(format!(
            "Unsupported --host-arch '{arch}'. Choose x86_64, arm64, aarch64, or x86."
        ))
    }
}

fn help_text() -> &'static str {
    "\
uniload - Universal OS built-in binary inspector & sandbox orchestrator

USAGE:
  uniload [--state-dir DIR] [--host-arch ARCH] <COMMAND> [ARGS]

COMMANDS:
  inspect <FILE>                       Inspect .exe/.com/.bat/.msi/.apk/ELF headers (read-only)
  install <FILE> [--name NAME]         Create isolated sandbox and register app
  launch <APP_ID|FILE> [--win-era ERA] [--dry-run] [-- ARGS...]
                                       Activate subsystem and run app (ERA: win98|winxp|win7|win10)
  era <APP_ID> <win98|winxp|win7|win10>
                                       Override Windows era compatibility profile
  config <APP_ID> [--args <ARGS>] [--env K=V,...]
                                       Configure per-app launch arguments & environment variables
  save-backup <APP_ID> <ARCHIVE.tar.gz>
                                       Export isolated app sandbox save data
  save-restore <APP_ID> <ARCHIVE.tar.gz>
                                       Restore app sandbox save data archive
  win32 <status|install-dxvk|install-vkd3d|install-otvdm|install-cnc-ddraw|install-runtimes <APP_ID>|reset-prefix <APP_ID>>
                                       Manage DXVK, VKD3D, OTVDM, cnc-ddraw & DX9/VC++ runtimes
  store <list|install|sync>            Browse & 1-click install verified apps from UniCore Catalog
  android <status|init|ui|setup-arm|sync-shortcuts|clipboard-sync>
                                       Manage built-in Android ART/Waydroid subsystem
  update <status|check|generate-manifest|verify-manifest|verify-file|grubenv|keygen|sign-manifest>
                                       Inspect, sign, verify, and manage A/B OS updates & grubenv
  hw-profile [--env]                   Inspect hardware tier (2005 legacy to modern)
  permissions <APP_ID> [--allow P] [--deny P]
                                       Update per-app sandbox permissions
  bwrap-args <APP_ID>                  Inspect bubblewrap confinement flags for an app
  uninstall <APP_ID>                   Remove app registration and isolated sandbox
  reclaim                              Terminate idle subsystem workers to free RAM
  status                               Show registered apps and subsystem states"
}

fn next_flag_value(args: &[String], idx: &mut usize, flag: &str) -> Result<String, String> {
    *idx += 1;
    args.get(*idx)
        .cloned()
        .ok_or_else(|| format!("Flag '{flag}' requires a value"))
}

fn parse_global_options(args: &[String]) -> Result<Option<CliOptions>, String> {
    let mut state_dir = default_state_dir();
    let mut host_arch = DEFAULT_HOST_ARCH.to_string();
    let mut positional = Vec::new();
    let mut idx = 0usize;

    while idx < args.len() {
        let arg = &args[idx];
        if positional.is_empty() {
            match arg.as_str() {
                "-h" | "--help" => return Ok(None),
                "--state-dir" => {
                    state_dir = PathBuf::from(next_flag_value(args, &mut idx, "--state-dir")?);
                }
                "--host-arch" => {
                    host_arch = next_flag_value(args, &mut idx, "--host-arch")?;
                    validate_host_arch(&host_arch)?;
                }
                _ => positional.push(arg.clone()),
            }
        } else {
            positional.push(arg.clone());
        }
        idx += 1;
    }

    if positional.is_empty() {
        return Err("Missing subcommand. Run with --help for usage.".to_string());
    }
    Ok(Some(CliOptions {
        state_dir,
        host_arch,
        positional,
    }))
}

/// Execute a `uniload` CLI command and return formatted output JSON/help text.
pub fn run_cli(args: &[String]) -> Result<String, String> {
    let Some(opts) = parse_global_options(args)? else {
        return Ok(help_text().to_string());
    };

    let cmd = opts.positional[0].as_str();
    let rest = &opts.positional[1..];

    // Keep read-only binary inspection free of filesystem state side effects.
    if cmd == "inspect" {
        let file = rest
            .first()
            .ok_or_else(|| "inspect requires <FILE>".to_string())?;
        let info = inspect_binary(file)?;
        let mut map = BTreeMap::new();
        map.insert(
            "arch".to_string(),
            JsonValue::Str(info.arch.clone()),
        );
        map.insert("has_dex".to_string(), JsonValue::Bool(info.has_dex));
        map.insert("is_gui".to_string(), JsonValue::Bool(info.is_gui));
        map.insert("is_library".to_string(), JsonValue::Bool(info.is_library));
        map.insert(
            "platform".to_string(),
            JsonValue::Str(info.platform.as_str().to_string()),
        );
        map.insert(
            "requires_cpu_translation".to_string(),
            JsonValue::Bool(info.requires_cpu_translation(&opts.host_arch)),
        );
        map.insert(
            "sub_abis".to_string(),
            JsonValue::Array(info.sub_abis.into_iter().map(JsonValue::Str).collect()),
        );
        map.insert(
            "win_era".to_string(),
            match info.win_era {
                Some(era) => JsonValue::Str(era.as_str().to_string()),
                None => JsonValue::Null,
            },
        );
        return Ok(to_json_pretty(&JsonValue::Object(map)));
    }

    if cmd == "android" {
        return run_android_command(rest);
    }

    if cmd == "win32" {
        return run_win32_command(&opts.state_dir, rest);
    }

    if cmd == "store" {
        return run_store_command(&opts.state_dir, rest);
    }

    if cmd == "update" {
        return run_update_command(rest);
    }

    if cmd == "hw-profile" {
        let mut emit_env = false;
        for flag in rest {
            if flag == "--env" {
                emit_env = true;
            } else {
                return Err(format!("Unexpected argument for hw-profile: {flag}"));
            }
        }
        let profile = classify_hardware(&detect_host_hardware());
        return Ok(if emit_env {
            profile.to_shell_env()
        } else {
            profile.to_json_string()
        });
    }

    let idle_timeout = std::env::var("UNILOAD_IDLE_TIMEOUT")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|&v| v > 0.0)
        .unwrap_or(DEFAULT_IDLE_TIMEOUT_SEC);
    let mut manager = SandboxManager::new(&opts.state_dir, idle_timeout)?;

    match cmd {
        "install" => {
            let file = rest
                .first()
                .ok_or_else(|| "install requires <FILE>".to_string())?;
            let mut name: Option<String> = None;
            let mut idx = 1usize;
            while idx < rest.len() {
                if rest[idx] == "--name" {
                    name = Some(next_flag_value(rest, &mut idx, "--name")?);
                } else {
                    return Err(format!("Unexpected argument for install: {}", rest[idx]));
                }
                idx += 1;
            }
            let sandbox = manager.install(file, name.as_deref())?;
            Ok(to_json_pretty(&sandbox.to_json()))
        }
        "launch" => {
            let target = rest
                .first()
                .ok_or_else(|| "launch requires <APP_ID|FILE>".to_string())?;
            let mut dry_run = false;
            let mut era_override: Option<WinEra> = None;
            let mut extra_args: Vec<String> = Vec::new();
            let mut idx = 1usize;
            while idx < rest.len() {
                if rest[idx] == "--" {
                    extra_args.extend(rest[idx + 1..].iter().cloned());
                    break;
                } else if rest[idx] == "--dry-run" {
                    dry_run = true;
                } else if rest[idx] == "--win-era" {
                    let raw = next_flag_value(rest, &mut idx, "--win-era")?;
                    era_override = Some(WinEra::parse(&raw)?);
                } else {
                    return Err(format!("Unexpected argument for launch: {}", rest[idx]));
                }
                idx += 1;
            }

            let target_path = Path::new(target);
            let app_id = if target_path.is_file() {
                manager.install(target_path, None)?.app_id
            } else {
                target.clone()
            };

            let mut plan = manager.launch_with_era(&app_id, &opts.host_arch, None, era_override)?;
            plan.argv.extend(extra_args);

            if !dry_run && cfg!(target_os = "linux") {
                let status = std::process::Command::new(&plan.runtime_binary)
                    .args(&plan.argv[1..])
                    .envs(&plan.env)
                    .status()
                    .map_err(|e| format!("Failed to spawn {}: {e}", plan.runtime_binary))?;
                let _ = manager.stop_session(&plan.session_id, None);
                if !status.success() {
                    return Err(format!("Subsystem exited with status {status}"));
                }
                return Ok(String::new());
            }

            let mut env_map = BTreeMap::new();
            for (k, v) in plan.env {
                env_map.insert(k, JsonValue::Str(v));
            }
            let mut out = BTreeMap::new();
            out.insert("app_id".to_string(), JsonValue::Str(plan.app_id));
            out.insert(
                "argv".to_string(),
                JsonValue::Array(plan.argv.into_iter().map(JsonValue::Str).collect()),
            );
            out.insert(
                "dry_run".to_string(),
                JsonValue::Bool(dry_run || !cfg!(target_os = "linux")),
            );
            out.insert("env".to_string(), JsonValue::Object(env_map));
            out.insert(
                "permissions".to_string(),
                plan.permissions.to_json(),
            );
            out.insert(
                "runtime_binary".to_string(),
                JsonValue::Str(plan.runtime_binary),
            );
            out.insert("sandbox_dir".to_string(), JsonValue::Str(plan.sandbox_dir));
            out.insert("session_id".to_string(), JsonValue::Str(plan.session_id));
            out.insert(
                "subsystem".to_string(),
                JsonValue::Str(plan.subsystem.as_str().to_string()),
            );
            out.insert(
                "translation_layer".to_string(),
                match plan.translation_layer {
                    Some(layer) => JsonValue::Str(layer),
                    None => JsonValue::Null,
                },
            );
            Ok(to_json_pretty(&JsonValue::Object(out)))
        }
        "era" => {
            let app_id = rest
                .first()
                .ok_or_else(|| "era requires <APP_ID> <win98|winxp|win7|win10|auto>".to_string())?;
            let era_raw = rest
                .get(1)
                .ok_or_else(|| "era requires <win98|winxp|win7|win10|auto>".to_string())?;
            if rest.len() > 2 {
                return Err(format!("Unexpected argument for era: {}", rest[2]));
            }
            let sandbox = if era_raw == "auto" {
                let source = manager.get_app(app_id)?.source_path.clone();
                let detected = inspect_binary(&source)?.win_era;
                manager.set_win_era(app_id, detected)?
            } else {
                manager.set_win_era(app_id, Some(WinEra::parse(era_raw)?))?
            };
            Ok(to_json_pretty(&sandbox.to_json()))
        }
        "config" => {
            let app_id = rest
                .first()
                .ok_or_else(|| "config requires <APP_ID> [--args \"...\"] [--env K=V,...]".to_string())?;
            let existing = manager.get_app(app_id)?.clone();
            let mut next_args = existing.launch_args;
            let mut next_env = existing.env_vars;
            let mut idx = 1usize;
            while idx < rest.len() {
                match rest[idx].as_str() {
                    "--args" => {
                        let raw = next_flag_value(rest, &mut idx, "--args")?;
                        next_args = crate::sandbox::split_launch_args(&raw);
                    }
                    "--env" => {
                        let raw = next_flag_value(rest, &mut idx, "--env")?;
                        next_env = crate::sandbox::parse_env_vars_str(&raw)?;
                    }
                    other => return Err(format!("Unexpected argument for config: {other}")),
                }
                idx += 1;
            }
            let updated = manager.update_launch_config(app_id, next_args, next_env)?;
            Ok(to_json_pretty(&updated.to_json()))
        }
        "save-backup" => {
            let app_id = rest
                .first()
                .ok_or_else(|| "save-backup requires <APP_ID> <DEST_ARCHIVE>".to_string())?;
            let dest = rest
                .get(1)
                .ok_or_else(|| "save-backup requires <DEST_ARCHIVE>".to_string())?;
            let out_path = manager.backup_app_save_data(app_id, Path::new(dest))?;
            let mut out = BTreeMap::new();
            out.insert("app_id".to_string(), JsonValue::Str(app_id.clone()));
            out.insert("archive".to_string(), JsonValue::Str(out_path.display().to_string()));
            out.insert("backed_up".to_string(), JsonValue::Bool(true));
            Ok(to_json_pretty(&JsonValue::Object(out)))
        }
        "save-restore" => {
            let app_id = rest
                .first()
                .ok_or_else(|| "save-restore requires <APP_ID> <SRC_ARCHIVE>".to_string())?;
            let src = rest
                .get(1)
                .ok_or_else(|| "save-restore requires <SRC_ARCHIVE>".to_string())?;
            manager.restore_app_save_data(app_id, Path::new(src))?;
            let mut out = BTreeMap::new();
            out.insert("app_id".to_string(), JsonValue::Str(app_id.clone()));
            out.insert("restored".to_string(), JsonValue::Bool(true));
            Ok(to_json_pretty(&JsonValue::Object(out)))
        }
        "permissions" => {
            let app_id = rest
                .first()
                .ok_or_else(|| "permissions requires <APP_ID>".to_string())?;
            let mut idx = 1usize;
            while idx < rest.len() {
                match rest[idx].as_str() {
                    "--allow" => {
                        let perm = next_flag_value(rest, &mut idx, "--allow")?;
                        manager.set_permission(app_id, &perm, true)?;
                    }
                    "--deny" => {
                        let perm = next_flag_value(rest, &mut idx, "--deny")?;
                        manager.set_permission(app_id, &perm, false)?;
                    }
                    other => return Err(format!("Unexpected argument for permissions: {other}")),
                }
                idx += 1;
            }
            let sandbox = manager.get_app(app_id)?;
            Ok(to_json_pretty(&sandbox.to_json()))
        }
        "bwrap-args" => {
            let app_id = rest
                .first()
                .ok_or_else(|| "bwrap-args requires <APP_ID>".to_string())?;
            if rest.len() > 1 {
                return Err(format!("Unexpected argument for bwrap-args: {}", rest[1]));
            }
            let app = manager.get_app(app_id)?;
            let subsystem = match app.platform {
                crate::inspector::AppPlatform::AndroidApk => SubsystemKind::AndroidArt,
                crate::inspector::AppPlatform::Dos16 => SubsystemKind::Dos,
                crate::inspector::AppPlatform::NativeElf => SubsystemKind::Native,
                _ => SubsystemKind::Win32,
            };
            let video_devices: Vec<String> = fs::read_dir("/dev")
                .ok()
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with("video") || name.starts_with("media") {
                        Some(format!("/dev/{name}"))
                    } else {
                        None
                    }
                })
                .collect();
            let xdg = std::env::var("XDG_RUNTIME_DIR").ok();
            let wl = std::env::var("WAYLAND_DISPLAY").ok();
            let ctx = ConfinementContext {
                xdg_runtime_dir: xdg.as_deref(),
                wayland_display: wl.as_deref(),
                has_x11: Path::new("/tmp/.X11-unix").is_dir(),
                video_devices: &video_devices,
            };
            let args = build_bwrap_args(app, subsystem, &ctx);
            Ok(to_json_pretty(&JsonValue::Array(
                args.into_iter().map(JsonValue::Str).collect(),
            )))
        }
        "uninstall" => {
            let app_id = rest
                .first()
                .ok_or_else(|| "uninstall requires <APP_ID>".to_string())?;
            if rest.len() > 1 {
                return Err(format!("Unexpected argument for uninstall: {}", rest[1]));
            }
            let removed = manager.uninstall(app_id)?;
            let mut out = BTreeMap::new();
            out.insert("app_id".to_string(), JsonValue::Str(removed.app_id));
            out.insert("name".to_string(), JsonValue::Str(removed.name));
            out.insert("sandbox_removed".to_string(), JsonValue::Bool(true));
            Ok(to_json_pretty(&JsonValue::Object(out)))
        }
        "reclaim" => {
            if !rest.is_empty() {
                return Err(format!("reclaim takes no arguments, found: {}", rest[0]));
            }
            run_reclaim_command(&manager)
        }
        "status" => {
            if !rest.is_empty() {
                return Err(format!("status takes no arguments, found: {}", rest[0]));
            }
            let apps: Vec<JsonValue> = manager.list_apps().into_iter().map(|a| a.to_json()).collect();
            let mut out = BTreeMap::new();
            out.insert("apps".to_string(), JsonValue::Array(apps));
            out.insert("subsystems".to_string(), manager.subsystem_status());
            Ok(to_json_pretty(&JsonValue::Object(out)))
        }
        other => Err(format!("Unknown subcommand '{other}'. Run with --help for usage.")),
    }
}

fn run_reclaim_command(manager: &SandboxManager) -> Result<String, String> {
    let mut win32_prefixes_cleaned: usize = 0;
    let mut android_container_stopped = false;

    if cfg!(target_os = "linux") {
        if let Ok(entries) = std::fs::read_dir(&manager.sandboxes_root) {
            for entry in entries.flatten() {
                let prefix = entry.path().join("drive_c_root");
                if crate::sandbox::stop_wineserver_for_prefix(&prefix) {
                    win32_prefixes_cleaned += 1;
                }
            }
        }
        if detect_android_host_status().container_active {
            android_container_stopped = std::process::Command::new("waydroid")
                .args(["container", "stop"])
                .status()
                .is_ok_and(|s| s.success());
        }
    }

    let mut out = BTreeMap::new();
    out.insert(
        "android_container_stopped".to_string(),
        JsonValue::Bool(android_container_stopped),
    );
    out.insert("reclaimed".to_string(), JsonValue::Bool(true));
    out.insert(
        "win32_prefixes_cleaned".to_string(),
        JsonValue::Number(win32_prefixes_cleaned as f64),
    );
    Ok(to_json_pretty(&JsonValue::Object(out)))
}

fn validate_android_profile(profile: &str) -> Result<(), String> {
    if matches!(profile, "vanilla" | "microg" | "gapps") {
        Ok(())
    } else {
        Err(format!(
            "Unsupported Android profile '{profile}'. Choose vanilla, microg, or gapps."
        ))
    }
}

fn run_android_command(rest: &[String]) -> Result<String, String> {
    const ART_BIN: &str = "/usr/libexec/uniload-art";
    let sub = rest
        .first()
        .ok_or_else(|| "android requires <status|init|ui|setup-arm|sync-shortcuts>".to_string())?
        .as_str();
    let tail = &rest[1..];

    let mut dry_run = false;
    let mut profile = "vanilla".to_string();
    let mut check_only = false;
    let mut idx = 0usize;
    while idx < tail.len() {
        match tail[idx].as_str() {
            "--dry-run" => dry_run = true,
            "--check" => check_only = true,
            "--profile" => {
                profile = next_flag_value(tail, &mut idx, "--profile")?;
                validate_android_profile(&profile)?;
            }
            other => return Err(format!("Unexpected argument for android {sub}: {other}")),
        }
        idx += 1;
    }

    let runtime_argv: Vec<String> = match sub {
        "status" => vec!["status".to_string()],
        "init" => vec!["init".to_string(), "--profile".to_string(), profile.clone()],
        "ui" => vec!["ui".to_string()],
        "setup-arm" => {
            if check_only {
                vec!["setup-arm".to_string(), "--check".to_string()]
            } else {
                vec!["setup-arm".to_string()]
            }
        }
        "sync-shortcuts" => vec!["sync-shortcuts".to_string()],
        "clipboard-sync" => vec!["clipboard-sync".to_string()],
        other => {
            return Err(format!(
                "Unknown android action '{other}'. Choose status, init, ui, setup-arm, sync-shortcuts, or clipboard-sync."
            ))
        }
    };

    if !dry_run && cfg!(target_os = "linux") && Path::new(ART_BIN).is_file() {
        let status = std::process::Command::new(ART_BIN)
            .args(&runtime_argv)
            .env("UNILOAD_ANDROID_ROOT", "/data/waydroid")
            .env("UNILOAD_MULTI_WINDOW", "1")
            .status()
            .map_err(|e| format!("Failed to spawn {ART_BIN}: {e}"))?;
        if !status.success() {
            return Err(format!("Android subsystem exited with status {status}"));
        }
        return Ok(String::new());
    }

    let android = detect_android_host_status();

    let mut out = BTreeMap::new();
    out.insert("action".to_string(), JsonValue::Str(sub.to_string()));
    out.insert(
        "android_root".to_string(),
        JsonValue::Str("/data/waydroid".to_string()),
    );
    out.insert(
        "argv".to_string(),
        JsonValue::Array(runtime_argv.into_iter().map(JsonValue::Str).collect()),
    );
    out.insert(
        "arm_translator_ready".to_string(),
        JsonValue::Bool(android.arm_translator_ready),
    );
    out.insert(
        "binderfs_ready".to_string(),
        JsonValue::Bool(android.binderfs_ready),
    );
    out.insert("dry_run".to_string(), JsonValue::Bool(true));
    out.insert(
        "image_ready".to_string(),
        JsonValue::Bool(android.image_ready),
    );
    out.insert("multi_window".to_string(), JsonValue::Bool(true));
    out.insert("profile".to_string(), JsonValue::Str(profile));
    out.insert(
        "runtime_binary".to_string(),
        JsonValue::Str(ART_BIN.to_string()),
    );
    out.insert(
        "waydroid_installed".to_string(),
        JsonValue::Bool(android.waydroid_installed),
    );
    Ok(to_json_pretty(&JsonValue::Object(out)))
}

fn run_win32_command(state_dir: &Path, rest: &[String]) -> Result<String, String> {
    const ASSETS_BIN: &str = "/usr/libexec/uniload-win32-assets";
    let sub = rest
        .first()
        .ok_or_else(|| {
            "win32 requires <status|install-dxvk|install-vkd3d|install-otvdm|install-cnc-ddraw|install-runtimes <APP_ID>>".to_string()
        })?
        .as_str();
    let tail = &rest[1..];

    let mut dry_run = false;
    let mut url: Option<String> = None;
    let mut from_file: Option<String> = None;
    let mut target_app_id: Option<String> = None;
    let mut idx = 0usize;
    while idx < tail.len() {
        match tail[idx].as_str() {
            "--dry-run" => dry_run = true,
            "--url" => url = Some(next_flag_value(tail, &mut idx, "--url")?),
            "--from-file" => from_file = Some(next_flag_value(tail, &mut idx, "--from-file")?),
            other if !other.starts_with('-') && matches!(sub, "install-runtimes" | "reset-prefix") && target_app_id.is_none() => {
                target_app_id = Some(other.to_string());
            }
            other => return Err(format!("Unexpected argument for win32 {sub}: {other}")),
        }
        idx += 1;
    }

    if sub == "reset-prefix" {
        let app_id = target_app_id
            .as_deref()
            .ok_or_else(|| "win32 reset-prefix requires <APP_ID>".to_string())?;
        let mut manager = SandboxManager::new(state_dir, DEFAULT_IDLE_TIMEOUT_SEC)?;
        let _ = &mut manager;
        let prefix = manager.reset_win32_prefix(app_id)?;
        let mut out = BTreeMap::new();
        out.insert("action".to_string(), JsonValue::Str("reset-prefix".to_string()));
        out.insert("app_id".to_string(), JsonValue::Str(app_id.to_string()));
        out.insert("prefix".to_string(), JsonValue::Str(prefix.display().to_string()));
        out.insert("reset".to_string(), JsonValue::Bool(true));
        return Ok(to_json_pretty(&JsonValue::Object(out)));
    }

    let mut runtime_argv: Vec<String> = match sub {
        "status" => vec!["status".to_string()],
        "install-dxvk" => vec!["install".to_string(), "dxvk".to_string()],
        "install-vkd3d" => vec!["install".to_string(), "vkd3d".to_string()],
        "install-otvdm" => vec!["install".to_string(), "otvdm".to_string()],
        "install-cnc-ddraw" => vec!["install".to_string(), "cnc-ddraw".to_string()],
        "install-runtimes" => {
            let app_id = target_app_id.as_deref().ok_or_else(|| {
                "win32 install-runtimes requires <APP_ID>".to_string()
            })?;
            let manager = SandboxManager::new(state_dir, DEFAULT_IDLE_TIMEOUT_SEC)?;
            let app = manager.get_app(app_id)?;
            if !matches!(
                app.platform,
                crate::inspector::AppPlatform::WindowsPe
                    | crate::inspector::AppPlatform::WindowsNe16
                    | crate::inspector::AppPlatform::WindowsMsi
            ) {
                return Err(format!(
                    "App '{}' ({}) is not a Windows application",
                    app.name,
                    app.platform.as_str()
                ));
            }
            let prefix = Path::new(&app.sandbox_dir).join("drive_c_root");
            vec![
                "install-runtimes".to_string(),
                "--prefix".to_string(),
                prefix.display().to_string(),
            ]
        }
        other => {
            return Err(format!(
                "Unknown win32 action '{other}'. Choose status, install-dxvk, install-vkd3d, install-otvdm, install-cnc-ddraw, or install-runtimes."
            ))
        }
    };
    if let Some(u) = &url {
        runtime_argv.push("--url".to_string());
        runtime_argv.push(u.clone());
    }
    if let Some(f) = &from_file {
        runtime_argv.push("--from-file".to_string());
        runtime_argv.push(f.clone());
    }

    if !dry_run && cfg!(target_os = "linux") && Path::new(ASSETS_BIN).is_file() {
        let output = std::process::Command::new(ASSETS_BIN)
            .args(&runtime_argv)
            .output()
            .map_err(|e| format!("Failed to spawn {ASSETS_BIN}: {e}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !output.status.success() {
            return Err(if !stderr.is_empty() {
                stderr
            } else {
                format!("win32 asset action exited with status {}", output.status)
            });
        }
        return Ok(stdout);
    }

    let status = detect_win32_assets_status();
    let mut out = BTreeMap::new();
    out.insert("action".to_string(), JsonValue::Str(sub.to_string()));
    out.insert(
        "argv".to_string(),
        JsonValue::Array(runtime_argv.into_iter().map(JsonValue::Str).collect()),
    );
    out.insert(
        "cnc_ddraw_installed".to_string(),
        JsonValue::Bool(status.cnc_ddraw_installed),
    );
    out.insert(
        "dos_subsystem_ready".to_string(),
        JsonValue::Bool(status.dos_subsystem_ready),
    );
    out.insert("dry_run".to_string(), JsonValue::Bool(true));
    out.insert("dxvk_installed".to_string(), JsonValue::Bool(status.dxvk_installed));
    out.insert("otvdm_installed".to_string(), JsonValue::Bool(status.otvdm_installed));
    out.insert(
        "runtime_binary".to_string(),
        JsonValue::Str(ASSETS_BIN.to_string()),
    );
    out.insert("vkd3d_installed".to_string(), JsonValue::Bool(status.vkd3d_installed));
    out.insert("wine_installed".to_string(), JsonValue::Bool(status.wine_installed));
    out.insert(
        "winetricks_installed".to_string(),
        JsonValue::Bool(status.winetricks_installed),
    );
    Ok(to_json_pretty(&JsonValue::Object(out)))
}

fn read_host_os_version(os_release_path: &Path) -> String {
    fs::read_to_string(os_release_path)
        .map(|s| parse_os_version(&s))
        .unwrap_or_else(|_| "0.1.0".to_string())
}

fn run_update_command(rest: &[String]) -> Result<String, String> {
    let sub = rest
        .first()
        .ok_or_else(|| {
            "update requires <status|check|generate-manifest|verify-manifest|verify-file|grubenv>"
                .to_string()
        })?
        .as_str();
    let tail = &rest[1..];

    match sub {
        "status" => {
            let mut grubenv_path = PathBuf::from("/boot/efi/boot/grub/grubenv");
            let mut boot_env_path = PathBuf::from("/mnt/unicore-boot.env");
            let mut os_release_path = PathBuf::from("/etc/os-release");
            let mut standby_ready: Option<bool> = None;
            let mut idx = 0usize;
            while idx < tail.len() {
                match tail[idx].as_str() {
                    "--grubenv" => {
                        grubenv_path = PathBuf::from(next_flag_value(tail, &mut idx, "--grubenv")?);
                    }
                    "--boot-env" => {
                        boot_env_path =
                            PathBuf::from(next_flag_value(tail, &mut idx, "--boot-env")?);
                    }
                    "--os-release" => {
                        os_release_path =
                            PathBuf::from(next_flag_value(tail, &mut idx, "--os-release")?);
                    }
                    "--standby-ready" => {
                        let raw = next_flag_value(tail, &mut idx, "--standby-ready")?;
                        standby_ready = Some(raw == "1" || raw.eq_ignore_ascii_case("true"));
                    }
                    other => return Err(format!("Unexpected argument for update status: {other}")),
                }
                idx += 1;
            }
            let cfg = load_host_update_config();
            let boot = read_boot_slot_info(&boot_env_path);
            let current_ver = read_host_os_version(&os_release_path);
            let grubenv = GrubEnv::read_file(&grubenv_path).ok();
            Ok(to_json_pretty(&build_update_status_json(
                &boot,
                &current_ver,
                grubenv.as_ref(),
                &cfg,
                standby_ready,
            )))
        }
        "generate-manifest" => {
            let mut version = String::new();
            let mut channel = "stable".to_string();
            let mut arch = DEFAULT_HOST_ARCH.to_string();
            let mut notes = String::new();
            let mut vmlinuz = None;
            let mut initramfs = None;
            let mut squashfs = None;
            let mut base_url = None;
            let mut output_file = None;

            let mut idx = 0usize;
            while idx < tail.len() {
                match tail[idx].as_str() {
                    "--version" => version = next_flag_value(tail, &mut idx, "--version")?,
                    "--channel" => channel = next_flag_value(tail, &mut idx, "--channel")?,
                    "--arch" => {
                        arch = next_flag_value(tail, &mut idx, "--arch")?;
                        validate_host_arch(&arch)?;
                    }
                    "--notes" => notes = next_flag_value(tail, &mut idx, "--notes")?,
                    "--vmlinuz" => {
                        vmlinuz = Some(PathBuf::from(next_flag_value(tail, &mut idx, "--vmlinuz")?))
                    }
                    "--initramfs" => {
                        initramfs = Some(PathBuf::from(next_flag_value(
                            tail,
                            &mut idx,
                            "--initramfs",
                        )?))
                    }
                    "--squashfs" => {
                        squashfs =
                            Some(PathBuf::from(next_flag_value(tail, &mut idx, "--squashfs")?))
                    }
                    "--base-url" => {
                        base_url = Some(next_flag_value(tail, &mut idx, "--base-url")?)
                    }
                    "--output" => {
                        output_file =
                            Some(PathBuf::from(next_flag_value(tail, &mut idx, "--output")?))
                    }
                    other => {
                        return Err(format!(
                            "Unexpected argument for update generate-manifest: {other}"
                        ))
                    }
                }
                idx += 1;
            }

            let vmlinuz_path =
                vmlinuz.ok_or_else(|| "generate-manifest requires --vmlinuz <PATH>".to_string())?;
            let initramfs_path = initramfs
                .ok_or_else(|| "generate-manifest requires --initramfs <PATH>".to_string())?;
            let squashfs_path = squashfs
                .ok_or_else(|| "generate-manifest requires --squashfs <PATH>".to_string())?;
            let manifest = generate_manifest(&ManifestBuildParams {
                version: &version,
                channel: &channel,
                arch: &arch,
                notes: &notes,
                vmlinuz_path: &vmlinuz_path,
                initramfs_path: &initramfs_path,
                squashfs_path: &squashfs_path,
                base_url: base_url.as_deref(),
            })?;
            let rendered = render_manifest_pretty(&manifest);
            if let Some(out_path) = output_file {
                fs::write(&out_path, format!("{rendered}\n"))
                    .map_err(|e| format!("Failed to write {}: {e}", out_path.display()))?;
            }
            Ok(rendered)
        }
        "verify-manifest" => {
            let manifest_path = PathBuf::from(
                tail.first()
                    .ok_or_else(|| "verify-manifest requires <MANIFEST_PATH>".to_string())?,
            );
            let cfg = load_host_update_config();
            let mut sig_path: Option<PathBuf> = None;
            let mut pubkey_path = PathBuf::from(&cfg.pubkey_path);
            let mut allow_unsigned = false;
            let mut require_signature = cfg.require_signature;
            let mut expected_arch = DEFAULT_HOST_ARCH.to_string();
            let mut current_version = read_host_os_version(Path::new("/etc/os-release"));
            let mut base_url = String::new();

            let mut idx = 1usize;
            while idx < tail.len() {
                match tail[idx].as_str() {
                    "--sig" => sig_path = Some(PathBuf::from(next_flag_value(tail, &mut idx, "--sig")?)),
                    "--pubkey" => {
                        pubkey_path = PathBuf::from(next_flag_value(tail, &mut idx, "--pubkey")?)
                    }
                    "--allow-unsigned" => allow_unsigned = true,
                    "--require-signature" => require_signature = true,
                    "--arch" => {
                        expected_arch = next_flag_value(tail, &mut idx, "--arch")?;
                        validate_host_arch(&expected_arch)?;
                    }
                    "--current-version" => {
                        current_version = next_flag_value(tail, &mut idx, "--current-version")?
                    }
                    "--manifest-url" => {
                        base_url = next_flag_value(tail, &mut idx, "--manifest-url")?
                    }
                    other => {
                        return Err(format!(
                            "Unexpected argument for update verify-manifest: {other}"
                        ))
                    }
                }
                idx += 1;
            }

            let raw = fs::read_to_string(&manifest_path)
                .map_err(|e| format!("Failed to read {}: {e}", manifest_path.display()))?;
            let manifest = UpdateManifest::parse_json_str(&raw)?;
            if normalize_arch(&manifest.arch) != normalize_arch(&expected_arch) {
                return Err(format!(
                    "Manifest arch '{}' does not match target arch '{expected_arch}'",
                    manifest.arch
                ));
            }
            let effective_sig = sig_path.or_else(|| {
                let candidate = PathBuf::from(format!("{}.sig", manifest_path.display()));
                candidate.is_file().then_some(candidate)
            });
            let sig_status = verify_manifest_signature(
                &manifest_path,
                effective_sig.as_deref(),
                &pubkey_path,
                allow_unsigned,
                require_signature,
            )?;
            let update_available =
                compare_versions(&manifest.version, &current_version) == Ordering::Greater;

            let mut out = BTreeMap::new();
            out.insert("arch".to_string(), JsonValue::Str(manifest.arch));
            out.insert("channel".to_string(), JsonValue::Str(manifest.channel));
            out.insert(
                "current_version".to_string(),
                JsonValue::Str(current_version),
            );
            out.insert(
                "initramfs_filename".to_string(),
                JsonValue::Str(manifest.initramfs.filename.clone()),
            );
            out.insert(
                "initramfs_sha256".to_string(),
                JsonValue::Str(manifest.initramfs.sha256),
            );
            out.insert(
                "initramfs_url".to_string(),
                JsonValue::Str(resolve_artifact_url(
                    &base_url,
                    &manifest.initramfs.url,
                    &manifest.initramfs.filename,
                )),
            );
            out.insert("notes".to_string(), JsonValue::Str(manifest.notes));
            out.insert(
                "signature_status".to_string(),
                JsonValue::Str(sig_status.as_str().to_string()),
            );
            out.insert(
                "squashfs_filename".to_string(),
                JsonValue::Str(manifest.squashfs.filename.clone()),
            );
            out.insert(
                "squashfs_sha256".to_string(),
                JsonValue::Str(manifest.squashfs.sha256),
            );
            out.insert(
                "squashfs_url".to_string(),
                JsonValue::Str(resolve_artifact_url(
                    &base_url,
                    &manifest.squashfs.url,
                    &manifest.squashfs.filename,
                )),
            );
            out.insert(
                "update_available".to_string(),
                JsonValue::Bool(update_available),
            );
            out.insert("valid".to_string(), JsonValue::Bool(true));
            out.insert("version".to_string(), JsonValue::Str(manifest.version));
            out.insert(
                "vmlinuz_filename".to_string(),
                JsonValue::Str(manifest.vmlinuz.filename.clone()),
            );
            out.insert(
                "vmlinuz_sha256".to_string(),
                JsonValue::Str(manifest.vmlinuz.sha256),
            );
            out.insert(
                "vmlinuz_url".to_string(),
                JsonValue::Str(resolve_artifact_url(
                    &base_url,
                    &manifest.vmlinuz.url,
                    &manifest.vmlinuz.filename,
                )),
            );
            Ok(to_json_pretty(&JsonValue::Object(out)))
        }
        "verify-file" => {
            let file_path = PathBuf::from(
                tail.first()
                    .ok_or_else(|| "verify-file requires <FILE> <EXPECTED_SHA256>".to_string())?,
            );
            let expected = tail
                .get(1)
                .ok_or_else(|| "verify-file requires <FILE> <EXPECTED_SHA256>".to_string())?
                .trim()
                .to_ascii_lowercase();
            if tail.len() > 2 {
                return Err(format!("Unexpected argument for verify-file: {}", tail[2]));
            }
            let actual = sha256_file(&file_path)?;
            if actual != expected {
                return Err(format!(
                    "SHA-256 mismatch for {}: expected {expected}, got {actual}",
                    file_path.display()
                ));
            }
            let mut out = BTreeMap::new();
            out.insert(
                "file".to_string(),
                JsonValue::Str(file_path.display().to_string()),
            );
            out.insert("sha256".to_string(), JsonValue::Str(actual));
            out.insert("verified".to_string(), JsonValue::Bool(true));
            Ok(to_json_pretty(&JsonValue::Object(out)))
        }
        "grubenv" => run_grubenv_command(tail),
        "keygen" => {
            let mut priv_path: Option<PathBuf> = None;
            let mut pub_path: Option<PathBuf> = None;
            let mut idx = 0usize;
            while idx < tail.len() {
                match tail[idx].as_str() {
                    "--private" => {
                        priv_path = Some(PathBuf::from(next_flag_value(tail, &mut idx, "--private")?))
                    }
                    "--public" => {
                        pub_path = Some(PathBuf::from(next_flag_value(tail, &mut idx, "--public")?))
                    }
                    other => return Err(format!("Unexpected argument for update keygen: {other}")),
                }
                idx += 1;
            }
            let priv_out =
                priv_path.ok_or_else(|| "keygen requires --private <PATH>".to_string())?;
            let pub_out = pub_path.ok_or_else(|| "keygen requires --public <PATH>".to_string())?;
            generate_ed25519_keypair(&priv_out, &pub_out)?;
            let mut out = BTreeMap::new();
            out.insert("algorithm".to_string(), JsonValue::Str("ED25519".to_string()));
            out.insert(
                "private_key".to_string(),
                JsonValue::Str(priv_out.display().to_string()),
            );
            out.insert(
                "public_key".to_string(),
                JsonValue::Str(pub_out.display().to_string()),
            );
            Ok(to_json_pretty(&JsonValue::Object(out)))
        }
        "sign-manifest" => {
            let manifest_path = PathBuf::from(
                tail.first()
                    .ok_or_else(|| "sign-manifest requires <MANIFEST_PATH>".to_string())?,
            );
            let mut key_path: Option<PathBuf> = None;
            let mut sig_out: Option<PathBuf> = None;
            let mut idx = 1usize;
            while idx < tail.len() {
                match tail[idx].as_str() {
                    "--key" => {
                        key_path = Some(PathBuf::from(next_flag_value(tail, &mut idx, "--key")?))
                    }
                    "--output" => {
                        sig_out = Some(PathBuf::from(next_flag_value(tail, &mut idx, "--output")?))
                    }
                    other => {
                        return Err(format!(
                            "Unexpected argument for update sign-manifest: {other}"
                        ))
                    }
                }
                idx += 1;
            }
            let priv_key =
                key_path.ok_or_else(|| "sign-manifest requires --key <PRIVATE_KEY>".to_string())?;
            let sig_path = sig_out
                .unwrap_or_else(|| PathBuf::from(format!("{}.sig", manifest_path.display())));
            sign_manifest_file(&manifest_path, &priv_key, &sig_path)?;
            let mut out = BTreeMap::new();
            out.insert(
                "manifest".to_string(),
                JsonValue::Str(manifest_path.display().to_string()),
            );
            out.insert(
                "signature".to_string(),
                JsonValue::Str(sig_path.display().to_string()),
            );
            out.insert("signed".to_string(), JsonValue::Bool(true));
            Ok(to_json_pretty(&JsonValue::Object(out)))
        }
        "check" => {
            let cfg = load_host_update_config();
            let mut url = cfg.update_url.clone();
            let mut allow_insecure_http = cfg.allow_insecure_http;
            let mut allow_unsigned = false;
            let mut require_signature = cfg.require_signature;
            let mut current_version = read_host_os_version(Path::new("/etc/os-release"));

            let mut idx = 0usize;
            while idx < tail.len() {
                match tail[idx].as_str() {
                    "--url" => url = next_flag_value(tail, &mut idx, "--url")?,
                    "--allow-insecure-http" => allow_insecure_http = true,
                    "--allow-unsigned" => allow_unsigned = true,
                    "--require-signature" => require_signature = true,
                    "--current-version" => {
                        current_version = next_flag_value(tail, &mut idx, "--current-version")?
                    }
                    other => return Err(format!("Unexpected argument for update check: {other}")),
                }
                idx += 1;
            }
            validate_update_url(&url, allow_insecure_http)?;

            let tmp_dir = std::env::temp_dir().join(format!("unicore_upd_chk_{}", std::process::id()));
            fs::create_dir_all(&tmp_dir)
                .map_err(|e| format!("Failed to create temp check dir: {e}"))?;
            let manifest_tmp = tmp_dir.join("manifest.json");
            let sig_tmp = tmp_dir.join("manifest.json.sig");

            let fetch_res = std::process::Command::new("curl")
                .args(["-fsSL", "--connect-timeout", "10", "--max-time", "30", "-o"])
                .arg(&manifest_tmp)
                .arg(url.trim())
                .output();
            let cleanup = || {
                let _ = fs::remove_dir_all(&tmp_dir);
            };
            let output = match fetch_res {
                Ok(out) => out,
                Err(e) => {
                    cleanup();
                    return Err(format!("Failed to run curl for update check: {e}"));
                }
            };
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                cleanup();
                return Err(if stderr.is_empty() {
                    format!("Failed to download manifest from {url}")
                } else {
                    format!("Failed to download manifest: {stderr}")
                });
            }

            let sig_url = format!("{}.sig", url.trim());
            let _ = std::process::Command::new("curl")
                .args(["-fsSL", "--connect-timeout", "10", "--max-time", "20", "-o"])
                .arg(&sig_tmp)
                .arg(&sig_url)
                .output();

            let raw = match fs::read_to_string(&manifest_tmp) {
                Ok(s) => s,
                Err(e) => {
                    cleanup();
                    return Err(format!("Failed to read downloaded manifest: {e}"));
                }
            };
            let manifest = match UpdateManifest::parse_json_str(&raw) {
                Ok(m) => m,
                Err(e) => {
                    cleanup();
                    return Err(e);
                }
            };
            let pubkey_path = Path::new(&cfg.pubkey_path);
            let sig_status = match verify_manifest_signature(
                &manifest_tmp,
                sig_tmp.is_file().then_some(sig_tmp.as_path()),
                pubkey_path,
                allow_unsigned,
                require_signature,
            ) {
                Ok(st) => st,
                Err(e) => {
                    cleanup();
                    return Err(e);
                }
            };
            cleanup();

            let update_available =
                compare_versions(&manifest.version, &current_version) == Ordering::Greater;
            let mut out = BTreeMap::new();
            out.insert("channel".to_string(), JsonValue::Str(manifest.channel));
            out.insert(
                "current_version".to_string(),
                JsonValue::Str(current_version),
            );
            out.insert(
                "latest_version".to_string(),
                JsonValue::Str(manifest.version),
            );
            out.insert("notes".to_string(), JsonValue::Str(manifest.notes));
            out.insert(
                "signature_status".to_string(),
                JsonValue::Str(sig_status.as_str().to_string()),
            );
            out.insert(
                "source_url".to_string(),
                JsonValue::Str(url.trim().to_string()),
            );
            out.insert(
                "update_available".to_string(),
                JsonValue::Bool(update_available),
            );
            Ok(to_json_pretty(&JsonValue::Object(out)))
        }
        other => Err(format!(
            "Unknown update subcommand '{other}'. Choose status, check, generate-manifest, verify-manifest, verify-file, grubenv, keygen, or sign-manifest."
        )),
    }
}

fn run_grubenv_command(tail: &[String]) -> Result<String, String> {
    let action = tail
        .first()
        .ok_or_else(|| "grubenv requires <init|show|stage|confirm|rollback>".to_string())?
        .as_str();
    let mut file_path: Option<PathBuf> = None;
    let mut slot: Option<Slot> = None;
    let mut version: Option<String> = None;

    let mut idx = 1usize;
    while idx < tail.len() {
        match tail[idx].as_str() {
            "--file" => file_path = Some(PathBuf::from(next_flag_value(tail, &mut idx, "--file")?)),
            "--slot" => slot = Some(Slot::parse(&next_flag_value(tail, &mut idx, "--slot")?)?),
            "--version" => version = Some(next_flag_value(tail, &mut idx, "--version")?),
            other => return Err(format!("Unexpected argument for update grubenv: {other}")),
        }
        idx += 1;
    }

    let path = file_path.ok_or_else(|| "grubenv requires --file <PATH>".to_string())?;
    match action {
        "init" => {
            let init_slot = slot.unwrap_or(Slot::SlotA);
            let ver = version.unwrap_or_else(|| "0.1.0".to_string());
            let env = GrubEnv::new_initial(init_slot, &ver);
            env.write_file_atomic(&path)?;
            Ok(to_json_pretty(&env.to_json()))
        }
        "show" => {
            let env = GrubEnv::read_file(&path)?;
            Ok(to_json_pretty(&env.to_json()))
        }
        "stage" => {
            let target = slot.ok_or_else(|| "grubenv stage requires --slot <A|B>".to_string())?;
            let ver = version.ok_or_else(|| "grubenv stage requires --version <VER>".to_string())?;
            let mut env = GrubEnv::read_file(&path)?;
            env.stage_update(target, &ver);
            env.write_file_atomic(&path)?;
            Ok(to_json_pretty(&env.to_json()))
        }
        "confirm" => {
            let booted =
                slot.ok_or_else(|| "grubenv confirm requires --slot <A|B>".to_string())?;
            let mut env = GrubEnv::read_file(&path)?;
            let confirmed = env.confirm_boot(booted);
            if confirmed {
                env.write_file_atomic(&path)?;
            }
            let mut obj = match env.to_json() {
                JsonValue::Object(m) => m,
                _ => BTreeMap::new(),
            };
            obj.insert("confirmed".to_string(), JsonValue::Bool(confirmed));
            Ok(to_json_pretty(&JsonValue::Object(obj)))
        }
        "rollback" => {
            let mut env = GrubEnv::read_file(&path)?;
            let target = env.request_rollback()?;
            env.write_file_atomic(&path)?;
            let mut obj = match env.to_json() {
                JsonValue::Object(m) => m,
                _ => BTreeMap::new(),
            };
            obj.insert(
                "rolled_back_to".to_string(),
                JsonValue::Str(target.label().to_string()),
            );
            Ok(to_json_pretty(&JsonValue::Object(obj)))
        }
        other => Err(format!(
            "Unknown grubenv action '{other}'. Choose init, show, stage, confirm, or rollback."
        )),
    }
}

fn run_store_command(state_dir: &Path, rest: &[String]) -> Result<String, String> {
    let sub = rest
        .first()
        .ok_or_else(|| "store requires <list|install|sync>".to_string())?
        .as_str();
    let tail = &rest[1..];

    match sub {
        "list" => {
            let mut category = "all".to_string();
            let mut query = String::new();
            let mut catalog_path: Option<PathBuf> = None;
            let mut idx = 0usize;
            while idx < tail.len() {
                match tail[idx].as_str() {
                    "--category" => category = next_flag_value(tail, &mut idx, "--category")?,
                    "--search" => query = next_flag_value(tail, &mut idx, "--search")?,
                    "--catalog" => {
                        catalog_path = Some(PathBuf::from(next_flag_value(tail, &mut idx, "--catalog")?));
                    }
                    other => return Err(format!("Unexpected argument for store list: {other}")),
                }
                idx += 1;
            }
            let catalog = load_catalog(catalog_path.as_deref())?;
            let filtered = filter_catalog_apps(&catalog.apps, &category, &query);
            let mut out = BTreeMap::new();
            out.insert("version".to_string(), JsonValue::Number(catalog.version as f64));
            out.insert("count".to_string(), JsonValue::Number(filtered.len() as f64));
            out.insert(
                "apps".to_string(),
                JsonValue::Array(filtered.iter().map(|a| a.to_json()).collect()),
            );
            Ok(to_json_pretty(&JsonValue::Object(out)))
        }
        "install" => {
            let id = tail
                .first()
                .ok_or_else(|| "store install requires <CATALOG_ID>".to_string())?;
            let mut catalog_path: Option<PathBuf> = None;
            let mut dry_run = false;
            let mut allow_insecure_http = false;
            let mut idx = 1usize;
            while idx < tail.len() {
                match tail[idx].as_str() {
                    "--catalog" => {
                        catalog_path = Some(PathBuf::from(next_flag_value(tail, &mut idx, "--catalog")?));
                    }
                    "--dry-run" => dry_run = true,
                    "--allow-insecure-http" => allow_insecure_http = true,
                    other => return Err(format!("Unexpected argument for store install: {other}")),
                }
                idx += 1;
            }
            let catalog = load_catalog(catalog_path.as_deref())?;
            let entry = find_catalog_app(&catalog, id)?;
            let store_root = state_dir.join("store");

            if dry_run {
                let mut out = BTreeMap::new();
                out.insert("bundled_path".to_string(), JsonValue::Str(entry.bundled_path.clone()));
                out.insert("download_url".to_string(), JsonValue::Str(entry.download_url.clone()));
                out.insert("dry_run".to_string(), JsonValue::Bool(true));
                out.insert("id".to_string(), JsonValue::Str(entry.id.clone()));
                out.insert("name".to_string(), JsonValue::Str(entry.name.clone()));
                out.insert("sha256".to_string(), JsonValue::Str(entry.sha256.clone()));
                out.insert("store_dir".to_string(), JsonValue::Str(store_root.display().to_string()));
                return Ok(to_json_pretty(&JsonValue::Object(out)));
            }

            let mut manager = SandboxManager::new(state_dir, DEFAULT_IDLE_TIMEOUT_SEC)?;
            let sandbox = install_catalog_app(&mut manager, entry, &store_root, allow_insecure_http)?;
            Ok(to_json_pretty(&sandbox.to_json()))
        }
        "sync" => {
            let url = tail
                .first()
                .ok_or_else(|| "store sync requires <HTTPS_URL>".to_string())?;
            let mut output_path = PathBuf::from(DEFAULT_DATA_CATALOG);
            let mut allow_insecure_http = false;
            let mut idx = 1usize;
            while idx < tail.len() {
                match tail[idx].as_str() {
                    "--output" => {
                        output_path = PathBuf::from(next_flag_value(tail, &mut idx, "--output")?);
                    }
                    "--allow-insecure-http" => allow_insecure_http = true,
                    other => return Err(format!("Unexpected argument for store sync: {other}")),
                }
                idx += 1;
            }
            let catalog = sync_catalog_from_url(url, &output_path, allow_insecure_http)?;
            let mut out = BTreeMap::new();
            out.insert("count".to_string(), JsonValue::Number(catalog.apps.len() as f64));
            out.insert("output".to_string(), JsonValue::Str(output_path.display().to_string()));
            out.insert("synced".to_string(), JsonValue::Bool(true));
            out.insert("version".to_string(), JsonValue::Number(catalog.version as f64));
            Ok(to_json_pretty(&JsonValue::Object(out)))
        }
        other => Err(format!(
            "Unknown store action '{other}'. Choose list, install, or sync."
        )),
    }
}
