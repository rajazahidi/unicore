//! Integration tests for Universal OS Rust orchestrator (`uniload`).

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use uniload::cli::run_cli;
use uniload::inspector::{inspect_binary, AppPlatform, WinEra};
use uniload::json_codec::{parse_json, JsonValue};
use uniload::sandbox::{
    build_bwrap_args, parse_waydroid_app_list, render_android_desktop_entry, sha256_file,
    sha256_hex, AndroidAppInfo, ConfinementContext, SandboxManager, Sha256, SubsystemKind,
    SubsystemState,
};
use uniload::updater::{
    compare_versions, normalize_arch, validate_update_url, verify_manifest_signature, GrubEnv,
    SignatureStatus, Slot, UpdateConfig, UpdateManifest,
};

struct TempWorkspace {
    root: PathBuf,
}

impl TempWorkspace {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "uniload_test_{label}_{}_{}",
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&root).expect("create temp workspace");
        Self { root }
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn build_pe_bytes_with_ver(
    machine: u16,
    subsystem: u16,
    opt_magic: u16,
    characteristics: u16,
    sub_major: u16,
) -> Vec<u8> {
    let e_lfanew: usize = 0x80;
    let mut buf = vec![0u8; e_lfanew + 24 + 80];
    buf[0..2].copy_from_slice(b"MZ");
    buf[0x3C..0x40].copy_from_slice(&(e_lfanew as u32).to_le_bytes());
    buf[e_lfanew..e_lfanew + 4].copy_from_slice(b"PE\x00\x00");

    let coff = e_lfanew + 4;
    buf[coff..coff + 2].copy_from_slice(&machine.to_le_bytes());
    buf[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes());
    buf[coff + 16..coff + 18].copy_from_slice(&80u16.to_le_bytes());
    buf[coff + 18..coff + 20].copy_from_slice(&characteristics.to_le_bytes());

    let opt = coff + 20;
    buf[opt..opt + 2].copy_from_slice(&opt_magic.to_le_bytes());
    buf[opt + 40..opt + 42].copy_from_slice(&sub_major.to_le_bytes());
    buf[opt + 48..opt + 50].copy_from_slice(&sub_major.to_le_bytes());
    buf[opt + 68..opt + 70].copy_from_slice(&subsystem.to_le_bytes());
    buf
}

fn build_pe_bytes(machine: u16, subsystem: u16, opt_magic: u16, characteristics: u16) -> Vec<u8> {
    build_pe_bytes_with_ver(machine, subsystem, opt_magic, characteristics, 10)
}

fn build_ne16_bytes() -> Vec<u8> {
    let e_lfanew: usize = 0x80;
    let mut buf = vec![0u8; e_lfanew + 64];
    buf[0..2].copy_from_slice(b"MZ");
    buf[0x3C..0x40].copy_from_slice(&(e_lfanew as u32).to_le_bytes());
    buf[e_lfanew..e_lfanew + 2].copy_from_slice(b"NE");
    buf
}

fn build_elf_bytes(machine: u16, little_endian: bool) -> Vec<u8> {
    let mut buf = vec![0u8; 64];
    buf[0..4].copy_from_slice(b"\x7fELF");
    buf[4] = 2;
    buf[5] = if little_endian { 1 } else { 2 };
    let e_type = if little_endian {
        2u16.to_le_bytes()
    } else {
        2u16.to_be_bytes()
    };
    let e_mach = if little_endian {
        machine.to_le_bytes()
    } else {
        machine.to_be_bytes()
    };
    buf[16..18].copy_from_slice(&e_type);
    buf[18..20].copy_from_slice(&e_mach);
    buf
}

/// Build a valid uncompressed (STORED) ZIP archive from `(filename, content)` pairs.
fn build_stored_zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut archive = Vec::new();
    let mut central_dir = Vec::new();

    for (name, data) in entries {
        let name_bytes = name.as_bytes();
        let local_offset = archive.len() as u32;

        // Local file header (30 bytes + filename + stored payload)
        archive.extend_from_slice(b"PK\x03\x04");
        archive.extend_from_slice(&20u16.to_le_bytes()); // version needed
        archive.extend_from_slice(&0u16.to_le_bytes()); // flags
        archive.extend_from_slice(&0u16.to_le_bytes()); // method = STORE
        archive.extend_from_slice(&0u32.to_le_bytes()); // time/date
        archive.extend_from_slice(&0u32.to_le_bytes()); // crc32 (unused by inspector)
        archive.extend_from_slice(&(data.len() as u32).to_le_bytes());
        archive.extend_from_slice(&(data.len() as u32).to_le_bytes());
        archive.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes()); // extra len
        archive.extend_from_slice(name_bytes);
        archive.extend_from_slice(data);

        // Central directory file header (46 bytes + filename)
        central_dir.extend_from_slice(b"PK\x01\x02");
        central_dir.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central_dir.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // flags
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // method
        central_dir.extend_from_slice(&0u32.to_le_bytes()); // time/date
        central_dir.extend_from_slice(&0u32.to_le_bytes()); // crc32
        central_dir.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central_dir.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central_dir.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // extra len
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // comment len
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // disk number
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        central_dir.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central_dir.extend_from_slice(&local_offset.to_le_bytes());
        central_dir.extend_from_slice(name_bytes);
    }

    let cd_offset = archive.len() as u32;
    let cd_size = central_dir.len() as u32;
    let count = entries.len() as u16;
    archive.extend_from_slice(&central_dir);

    // End of central directory record (22 bytes)
    archive.extend_from_slice(b"PK\x05\x06");
    archive.extend_from_slice(&0u16.to_le_bytes()); // disk number
    archive.extend_from_slice(&0u16.to_le_bytes()); // cd start disk
    archive.extend_from_slice(&count.to_le_bytes());
    archive.extend_from_slice(&count.to_le_bytes());
    archive.extend_from_slice(&cd_size.to_le_bytes());
    archive.extend_from_slice(&cd_offset.to_le_bytes());
    archive.extend_from_slice(&0u16.to_le_bytes()); // comment len
    archive
}

fn write_file(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).expect("write fixture file");
}

#[test]
fn inspect_pe_x64_x86_arm64_and_corrupt_mz() {
    let tmp = TempWorkspace::new("pe");

    let exe64 = tmp.root.join("app64.exe");
    write_file(&exe64, &build_pe_bytes(0x8664, 2, 0x020B, 0x0002));
    let info64 = inspect_binary(&exe64).unwrap();
    assert_eq!(info64.platform, AppPlatform::WindowsPe);
    assert_eq!(info64.arch, "x86_64");
    assert!(info64.is_gui);
    assert!(!info64.is_library);
    assert!(!info64.requires_cpu_translation("x86_64"));
    assert!(info64.requires_cpu_translation("arm64"));

    let exe32 = tmp.root.join("cli32.exe");
    write_file(&exe32, &build_pe_bytes(0x014C, 3, 0x010B, 0x0002));
    let info32 = inspect_binary(&exe32).unwrap();
    assert_eq!(info32.arch, "x86");
    assert!(!info32.is_gui);
    assert!(!info32.requires_cpu_translation("x86_64"));

    let dll_arm = tmp.root.join("arm.dll");
    write_file(&dll_arm, &build_pe_bytes(0xAA64, 2, 0x020B, 0x2002));
    let info_arm = inspect_binary(&dll_arm).unwrap();
    assert_eq!(info_arm.arch, "arm64");
    assert!(info_arm.is_library);
    assert!(info_arm.requires_cpu_translation("x86_64"));

    let corrupt = tmp.root.join("corrupt.exe");
    let mut bad = vec![0u8; 128];
    bad[0..2].copy_from_slice(b"MZ");
    write_file(&corrupt, &bad);
    assert_eq!(inspect_binary(&corrupt).unwrap().platform, AppPlatform::Unknown);
}

#[test]
fn inspect_msi_apk_zip_elf_and_missing_file() {
    let tmp = TempWorkspace::new("formats");

    let msi = tmp.root.join("setup.msi");
    let mut msi_bytes = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1".to_vec();
    msi_bytes.extend_from_slice(&[0u8; 64]);
    write_file(&msi, &msi_bytes);
    assert_eq!(inspect_binary(&msi).unwrap().platform, AppPlatform::WindowsMsi);

    let apk_arm = tmp.root.join("game.apk");
    write_file(
        &apk_arm,
        &build_stored_zip_bytes(&[
            ("AndroidManifest.xml", b"manifest"),
            ("classes.dex", b"dex"),
            ("lib/arm64-v8a/libengine.so", b"so"),
        ]),
    );
    let info_arm = inspect_binary(&apk_arm).unwrap();
    assert_eq!(info_arm.platform, AppPlatform::AndroidApk);
    assert_eq!(info_arm.arch, "arm64");
    assert_eq!(info_arm.sub_abis, vec!["arm64-v8a".to_string()]);
    assert!(info_arm.has_dex);
    assert!(info_arm.requires_cpu_translation("x86_64"));
    assert!(!info_arm.requires_cpu_translation("arm64"));

    let apk_pure = tmp.root.join("notes.apk");
    write_file(
        &apk_pure,
        &build_stored_zip_bytes(&[
            ("AndroidManifest.xml", b"manifest"),
            ("classes.dex", b"dex"),
        ]),
    );
    let info_pure = inspect_binary(&apk_pure).unwrap();
    assert_eq!(info_pure.platform, AppPlatform::AndroidApk);
    assert_eq!(info_pure.arch, "noarch");
    assert!(!info_pure.requires_cpu_translation("x86_64"));

    let plain_zip = tmp.root.join("archive.zip");
    write_file(&plain_zip, &build_stored_zip_bytes(&[("readme.txt", b"hi")]));
    assert_eq!(inspect_binary(&plain_zip).unwrap().platform, AppPlatform::Unknown);

    let elf64 = tmp.root.join("native64");
    write_file(&elf64, &build_elf_bytes(0x3E, true));
    let elf64_info = inspect_binary(&elf64).unwrap();
    assert_eq!(elf64_info.platform, AppPlatform::NativeElf);
    assert_eq!(elf64_info.arch, "x86_64");

    let elf_be = tmp.root.join("arm_be");
    write_file(&elf_be, &build_elf_bytes(0xB7, false));
    assert_eq!(inspect_binary(&elf_be).unwrap().arch, "arm64");

    assert!(inspect_binary(tmp.root.join("missing.exe")).is_err());
}

#[test]
fn sandbox_isolation_permissions_and_on_demand_lifecycle() {
    let tmp = TempWorkspace::new("lifecycle");
    let state_dir = tmp.root.join("state");
    let mut manager = SandboxManager::new(&state_dir, 30.0).unwrap();

    let exe_a = tmp.root.join("AppA.exe");
    let exe_b = tmp.root.join("AppB.exe");
    let apk_m = tmp.root.join("Mobile.apk");
    write_file(&exe_a, &build_pe_bytes(0x8664, 2, 0x020B, 0x0002));
    write_file(&exe_b, &build_pe_bytes(0x8664, 2, 0x020B, 0x0002));
    write_file(
        &apk_m,
        &build_stored_zip_bytes(&[
            ("AndroidManifest.xml", b"manifest"),
            ("classes.dex", b"dex"),
            ("lib/arm64-v8a/libnative.so", b"so"),
        ]),
    );

    let app_a = manager.install(&exe_a, None).unwrap();
    let app_b = manager.install(&exe_b, None).unwrap();
    let app_apk = manager.install(&apk_m, None).unwrap();

    assert_ne!(app_a.sandbox_dir, app_b.sandbox_dir);
    assert!(Path::new(&app_a.sandbox_dir)
        .join("drive_c_root/drive_c/windows")
        .is_dir());
    assert!(Path::new(&app_apk.sandbox_dir)
        .join("data/dalvik-cache")
        .is_dir());

    assert!(!app_a.permissions.camera);
    assert!(!app_a.permissions.microphone);
    assert!(!app_a.permissions.shared_documents);
    assert!(app_a.permissions.network);

    manager.set_permission(&app_apk.app_id, "camera", true).unwrap();
    let reloaded = SandboxManager::new(&state_dir, 30.0).unwrap();
    assert!(reloaded.get_app(&app_apk.app_id).unwrap().permissions.camera);
    assert!(manager.set_permission(&app_apk.app_id, "root_access", true).is_err());

    // Initial cold state
    let status0 = manager.subsystem_status();
    let win32_0 = status0
        .as_object()
        .unwrap()
        .get("win32")
        .unwrap()
        .as_object()
        .unwrap();
    assert_eq!(win32_0.get("state").unwrap().as_str(), Some("cold"));

    let plan_a = manager.launch(&app_a.app_id, "x86_64", Some(100.0)).unwrap();
    let plan_b = manager.launch(&app_b.app_id, "x86_64", Some(105.0)).unwrap();
    assert_eq!(plan_a.subsystem, SubsystemKind::Win32);
    assert_eq!(plan_a.translation_layer, None);

    // Stopping one session keeps win32 active
    manager.stop_session(&plan_a.session_id, Some(110.0)).unwrap();
    let states_mid = manager.reap_idle(Some(200.0), false);
    assert_eq!(states_mid.get(&SubsystemKind::Win32), Some(&SubsystemState::Active));
    assert_eq!(states_mid.get(&SubsystemKind::AndroidArt), Some(&SubsystemState::Cold));

    // Stopping second session + advancing past 30s idle hibernates, then reclaims to cold
    manager.stop_session(&plan_b.session_id, Some(210.0)).unwrap();
    let states_pre = manager.reap_idle(Some(239.0), false);
    assert_eq!(states_pre.get(&SubsystemKind::Win32), Some(&SubsystemState::Active));

    let states_hib = manager.reap_idle(Some(240.0), false);
    assert_eq!(
        states_hib.get(&SubsystemKind::Win32),
        Some(&SubsystemState::Hibernated)
    );

    let states_cold = manager.reap_idle(Some(245.0), true);
    assert_eq!(states_cold.get(&SubsystemKind::Win32), Some(&SubsystemState::Cold));

    let apk_plan = manager.launch(&app_apk.app_id, "x86_64", Some(250.0)).unwrap();
    assert_eq!(apk_plan.subsystem, SubsystemKind::AndroidArt);
    assert_eq!(
        apk_plan.translation_layer.as_deref(),
        Some("arm64-to-x86_64")
    );
    assert_eq!(
        apk_plan.env.get("UNILOAD_ANDROID_ROOT").map(String::as_str),
        Some("/data/waydroid")
    );
    assert_eq!(
        apk_plan.env.get("UNILOAD_MULTI_WINDOW").map(String::as_str),
        Some("1")
    );
    assert_eq!(
        apk_plan.env.get("UNILOAD_BINDER_NODE").map(String::as_str),
        Some("/dev/binderfs/binder")
    );
    assert_eq!(apk_plan.argv.first().map(String::as_str), Some("/usr/libexec/uniload-art"));
    assert_eq!(apk_plan.argv.get(1).map(String::as_str), Some("launch-apk"));
}

#[test]
fn android_cli_subcommands_status_init_ui_and_arm_setup() {
    let status_out = run_cli(&["android".to_string(), "status".to_string()]).unwrap();
    let status_json = parse_json(&status_out).unwrap();
    let status_obj = status_json.as_object().unwrap();
    assert_eq!(status_obj.get("action").unwrap().as_str(), Some("status"));
    assert_eq!(
        status_obj.get("android_root").unwrap().as_str(),
        Some("/data/waydroid")
    );
    assert_eq!(status_obj.get("multi_window").unwrap().as_bool(), Some(true));

    let init_out = run_cli(&[
        "android".to_string(),
        "init".to_string(),
        "--profile".to_string(),
        "microg".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();
    let init_json = parse_json(&init_out).unwrap();
    let init_obj = init_json.as_object().unwrap();
    assert_eq!(init_obj.get("profile").unwrap().as_str(), Some("microg"));
    assert_eq!(
        init_obj.get("runtime_binary").unwrap().as_str(),
        Some("/usr/libexec/uniload-art")
    );

    let ui_out = run_cli(&[
        "android".to_string(),
        "ui".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();
    assert_eq!(
        parse_json(&ui_out)
            .unwrap()
            .as_object()
            .unwrap()
            .get("action")
            .unwrap()
            .as_str(),
        Some("ui")
    );

    let arm_out = run_cli(&[
        "android".to_string(),
        "setup-arm".to_string(),
        "--check".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();
    assert_eq!(
        parse_json(&arm_out)
            .unwrap()
            .as_object()
            .unwrap()
            .get("action")
            .unwrap()
            .as_str(),
        Some("setup-arm")
    );

    assert!(run_cli(&[
        "android".to_string(),
        "init".to_string(),
        "--profile".to_string(),
        "invalid_profile".to_string(),
    ])
    .is_err());
}

#[test]
fn hw_profile_cli_emits_valid_json_and_shell_env() {
    let json_out = run_cli(&["hw-profile".to_string()]).unwrap();
    let parsed = parse_json(&json_out).unwrap();
    let obj = parsed.as_object().unwrap();
    assert!(obj.contains_key("tier"));
    assert!(obj.contains_key("zram_mb"));
    assert!(obj.contains_key("wlr_renderer"));
    assert!(obj.contains_key("win32_backend_mode"));

    let env_out = run_cli(&["hw-profile".to_string(), "--env".to_string()]).unwrap();
    assert!(env_out.contains("UNICORE_HW_TIER="));
    assert!(env_out.contains("UNICORE_ZRAM_MB="));
    assert!(env_out.contains("WLR_RENDERER="));
}

#[test]
fn cli_end_to_end_inspect_install_permissions_and_launch() {
    let tmp = TempWorkspace::new("cli");
    let state_dir = tmp.root.join("cli_state");
    let exe_path = tmp.root.join("QuickPad.exe");
    write_file(&exe_path, &build_pe_bytes(0x8664, 2, 0x020B, 0x0002));

    let run = |extra: &[&str]| -> JsonValue {
        let mut argv = vec![
            "--state-dir".to_string(),
            state_dir.display().to_string(),
        ];
        argv.extend(extra.iter().map(|s| (*s).to_string()));
        let stdout = run_cli(&argv).expect("cli command should succeed");
        parse_json(&stdout).expect("cli output should be valid JSON")
    };

    let exe_str = exe_path.display().to_string();
    let inspected = run(&["inspect", &exe_str]);
    assert_eq!(
        inspected.as_object().unwrap().get("platform").unwrap().as_str(),
        Some("windows_pe")
    );
    assert!(
        !state_dir.exists(),
        "inspect must not create state_dir on disk"
    );

    let installed = run(&["install", &exe_str, "--name", "QuickPad"]);
    let app_id = installed
        .as_object()
        .unwrap()
        .get("app_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    let perms = run(&["permissions", &app_id, "--allow", "shared_documents"]);
    let docs_allowed = perms
        .as_object()
        .unwrap()
        .get("permissions")
        .unwrap()
        .as_object()
        .unwrap()
        .get("shared_documents")
        .unwrap()
        .as_bool();
    assert_eq!(docs_allowed, Some(true));

    let launched = run(&["launch", &app_id, "--dry-run", "--", "--safe-mode"]);
    let launched_obj = launched.as_object().unwrap();
    assert_eq!(
        launched_obj.get("subsystem").unwrap().as_str(),
        Some("win32")
    );
    let argv = launched_obj.get("argv").unwrap().as_array().unwrap();
    assert_eq!(argv.last().and_then(JsonValue::as_str), Some("--safe-mode"));
    assert_eq!(
        launched_obj
            .get("env")
            .unwrap()
            .as_object()
            .unwrap()
            .get("UNILOAD_ALLOW_DOCS")
            .unwrap()
            .as_str(),
        Some("1")
    );

    let status = run(&["status"]);
    let apps = status
        .as_object()
        .unwrap()
        .get("apps")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(apps.len(), 1);
}

#[test]
fn win98_winxp_win16_era_detection_and_overrides() {
    let tmp = TempWorkspace::new("win_eras");
    let state_dir = tmp.root.join("era_state");
    let mut manager = SandboxManager::new(&state_dir, 30.0).unwrap();

    let win98_game = tmp.root.join("Retro98.exe");
    write_file(&win98_game, &build_pe_bytes_with_ver(0x014C, 2, 0x010B, 0x0002, 4));
    let info98 = inspect_binary(&win98_game).unwrap();
    assert_eq!(info98.win_era, Some(WinEra::Win98));
    assert_eq!(info98.arch, "x86");

    let winxp_game = tmp.root.join("ClassicXP.exe");
    write_file(&winxp_game, &build_pe_bytes_with_ver(0x014C, 2, 0x010B, 0x0002, 5));
    let infoxp = inspect_binary(&winxp_game).unwrap();
    assert_eq!(infoxp.win_era, Some(WinEra::WinXp));

    let mut pe61 = build_pe_bytes_with_ver(0x8664, 2, 0x020B, 0x0002, 6);
    pe61[0x80 + 24 + 42..0x80 + 24 + 44].copy_from_slice(&1u16.to_le_bytes());
    pe61[0x80 + 24 + 50..0x80 + 24 + 52].copy_from_slice(&1u16.to_le_bytes());
    let win7_game = tmp.root.join("ClassicWin7.exe");
    write_file(&win7_game, &pe61);
    let info7 = inspect_binary(&win7_game).unwrap();
    assert_eq!(info7.win_era, Some(WinEra::Win7));

    let win16_app = tmp.root.join("OldSetup16.exe");
    write_file(&win16_app, &build_ne16_bytes());
    let info16 = inspect_binary(&win16_app).unwrap();
    assert_eq!(info16.platform, AppPlatform::WindowsNe16);
    assert_eq!(info16.arch, "x86_16");
    assert_eq!(info16.win_era, Some(WinEra::Win98));
    assert!(!info16.requires_cpu_translation("x86_64"));

    let app98 = manager.install(&win98_game, None).unwrap();
    let appxp = manager.install(&winxp_game, None).unwrap();
    let app7 = manager.install(&win7_game, None).unwrap();
    let app16 = manager.install(&win16_app, None).unwrap();

    let plan98 = manager.launch(&app98.app_id, "x86_64", Some(10.0)).unwrap();
    assert_eq!(plan98.env.get("UNILOAD_WIN_ERA").map(String::as_str), Some("win98"));
    assert_eq!(plan98.env.get("UNILOAD_WIN_ARCH").map(String::as_str), Some("wow64-32bit"));
    assert_eq!(plan98.env.get("UNILOAD_VIRTUAL_DESKTOP").map(String::as_str), Some("800x600"));
    assert_eq!(plan98.env.get("UNILOAD_VK_BACKEND").map(String::as_str), Some("ddraw-compat+wined3d"));

    let planxp = manager.launch(&appxp.app_id, "x86_64", Some(20.0)).unwrap();
    assert_eq!(planxp.env.get("UNILOAD_WIN_ERA").map(String::as_str), Some("winxp"));
    assert_eq!(planxp.env.get("UNILOAD_VIRTUAL_DESKTOP").map(String::as_str), Some("off"));
    assert_eq!(planxp.env.get("UNILOAD_VK_BACKEND").map(String::as_str), Some("dxvk-d3d8-d3d9"));

    let plan7 = manager.launch(&app7.app_id, "x86_64", Some(25.0)).unwrap();
    assert_eq!(plan7.env.get("UNILOAD_WIN_ERA").map(String::as_str), Some("win7"));
    assert_eq!(plan7.env.get("UNILOAD_VK_BACKEND").map(String::as_str), Some("dxvk-d3d8-d3d11"));

    let plan16 = manager.launch(&app16.app_id, "x86_64", Some(30.0)).unwrap();
    assert_eq!(plan16.env.get("UNILOAD_WIN16_MODE").map(String::as_str), Some("1"));
    assert_eq!(plan16.env.get("UNILOAD_WIN_ARCH").map(String::as_str), Some("win16"));

    // CLI era override & per-launch --win-era override
    let state_arg = state_dir.display().to_string();
    let cli_out = run_cli(&[
        "--state-dir".to_string(),
        state_arg.clone(),
        "era".to_string(),
        appxp.app_id.clone(),
        "win98".to_string(),
    ])
    .unwrap();
    let parsed_era = parse_json(&cli_out).unwrap();
    assert_eq!(
        parsed_era.as_object().unwrap().get("win_era").unwrap().as_str(),
        Some("win98")
    );

    let cli_launch = run_cli(&[
        "--state-dir".to_string(),
        state_arg,
        "launch".to_string(),
        appxp.app_id.clone(),
        "--win-era".to_string(),
        "winxp".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();
    let parsed_launch = parse_json(&cli_launch).unwrap();
    assert_eq!(
        parsed_launch
            .as_object()
            .unwrap()
            .get("env")
            .unwrap()
            .as_object()
            .unwrap()
            .get("UNILOAD_WIN_ERA")
            .unwrap()
            .as_str(),
        Some("winxp")
    );
}

#[test]
fn native_elf_and_uninstall_and_reclaim_lifecycle() {
    let tmp = TempWorkspace::new("native_uninstall");
    let state_dir = tmp.root.join("state");
    let state_arg = state_dir.display().to_string();

    let elf = tmp.root.join("hello_native");
    write_file(&elf, &build_elf_bytes(0x3E, true));

    let run = |args: &[&str]| -> JsonValue {
        let mut full = vec!["--state-dir".to_string(), state_arg.clone()];
        full.extend(args.iter().map(|s| (*s).to_string()));
        let output = run_cli(&full).unwrap_or_else(|e| panic!("CLI failed for {args:?}: {e}"));
        parse_json(&output).expect("valid json output")
    };

    let installed = run(&["install", &elf.display().to_string(), "--name", "HelloElf"]);
    let app_id = installed
        .as_object()
        .unwrap()
        .get("app_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let sandbox_dir = PathBuf::from(
        installed
            .as_object()
            .unwrap()
            .get("sandbox_dir")
            .unwrap()
            .as_str()
            .unwrap(),
    );
    assert!(sandbox_dir.is_dir());

    // Verify Native ELF routes via /usr/libexec/uniload-native with UNILOAD_ALLOW_GPU=1
    let plan1 = run(&["launch", &app_id, "--dry-run"]);
    let argv1 = plan1.as_object().unwrap().get("argv").unwrap().as_array().unwrap();
    assert_eq!(argv1[0].as_str(), Some("/usr/libexec/uniload-native"));
    assert!(argv1[1].as_str().unwrap().ends_with("hello_native"));
    let env1 = plan1.as_object().unwrap().get("env").unwrap().as_object().unwrap();
    assert_eq!(env1.get("UNILOAD_ALLOW_GPU").and_then(JsonValue::as_str), Some("1"));

    // Deny GPU and verify UNILOAD_ALLOW_GPU flips to 0
    run(&["permissions", &app_id, "--deny", "gpu_acceleration"]);
    let plan2 = run(&["launch", &app_id, "--dry-run"]);
    let env2 = plan2.as_object().unwrap().get("env").unwrap().as_object().unwrap();
    assert_eq!(env2.get("UNILOAD_ALLOW_GPU").and_then(JsonValue::as_str), Some("0"));

    // Uninstall removes sandbox dir & registry entry
    let uninst = run(&["uninstall", &app_id]);
    assert_eq!(
        uninst.as_object().unwrap().get("sandbox_removed").and_then(JsonValue::as_bool),
        Some(true)
    );
    assert!(!sandbox_dir.exists());
    let status = run(&["status"]);
    assert!(status.as_object().unwrap().get("apps").unwrap().as_array().unwrap().is_empty());

    // Missing app_id returns clean error
    assert!(run_cli(&[
        "--state-dir".to_string(),
        state_arg.clone(),
        "uninstall".to_string(),
        "nonexistent-id".to_string(),
    ])
    .is_err());

    // Reclaim succeeds with structured JSON
    let reclaim = run(&["reclaim"]);
    assert_eq!(
        reclaim.as_object().unwrap().get("reclaimed").and_then(JsonValue::as_bool),
        Some(true)
    );
}

#[test]
fn streaming_sha256_file_and_version_and_url_validation() {
    let tmp = TempWorkspace::new("sha_stream");
    let big_file = tmp.root.join("payload.bin");
    let mut payload = Vec::with_capacity(150_000);
    for i in 0..150_000usize {
        payload.push(((i * 31 + 7) & 0xFF) as u8);
    }
    write_file(&big_file, &payload);

    let expected = sha256_hex(&payload);
    let mut hasher = Sha256::new();
    hasher.update(&payload[..17]);
    hasher.update(&payload[17..65_555]);
    hasher.update(&payload[65_555..]);
    assert_eq!(hasher.finalize_hex(), expected);
    assert_eq!(sha256_file(&big_file).unwrap(), expected);

    assert_eq!(compare_versions("0.2.0", "0.1.0"), Ordering::Greater);
    assert_eq!(compare_versions("v0.2.0", "0.2.0"), Ordering::Equal);
    assert_eq!(compare_versions("0.2.0-rc1", "0.2.0"), Ordering::Less);
    assert_eq!(compare_versions("0.2.0-rc2", "0.2.0-rc1"), Ordering::Greater);
    assert_eq!(compare_versions("0.10.0", "0.2.9"), Ordering::Greater);

    let base_cfg = UpdateConfig::parse(
        "UPDATE_URL=\"https://updates.example.com/manifest.json\"\nUPDATE_CHANNEL=\"stable\"\nALLOW_INSECURE_HTTP=0\nREQUIRE_SIGNATURE=0\n",
    );
    let merged_cfg = base_cfg.merge("UPDATE_CHANNEL=\"beta\"\nALLOW_INSECURE_HTTP=1\nREQUIRE_SIGNATURE=1\n");
    assert_eq!(merged_cfg.update_url, "https://updates.example.com/manifest.json");
    assert_eq!(merged_cfg.channel, "beta");
    assert!(merged_cfg.allow_insecure_http);
    assert!(merged_cfg.require_signature);
    assert!(merged_cfg.render().contains("REQUIRE_SIGNATURE=1"));

    assert!(validate_update_url("https://releases.example.com/manifest.json", false).is_ok());
    assert!(validate_update_url("http://127.0.0.1:8080/manifest.json", false).is_err());
    assert!(validate_update_url("http://127.0.0.1:8080/manifest.json", true).is_ok());
    assert!(validate_update_url("https://bad.example.com/$(id)", false).is_err());
    assert!(validate_update_url("ftp://bad.example.com/a", true).is_err());
}

#[test]
fn update_manifest_generate_verify_and_tamper_detection() {
    let tmp = TempWorkspace::new("manifest");
    let vm = tmp.root.join("vmlinuz-x86_64");
    let initrd = tmp.root.join("initramfs-x86_64.img");
    let squash = tmp.root.join("unicore-rootfs-x86_64.squashfs");
    let manifest_path = tmp.root.join("unicore-manifest.json");
    write_file(&vm, b"kernel-bytes-v0.2.0");
    write_file(&initrd, b"initramfs-bytes-v0.2.0");
    write_file(&squash, &vec![0x5Au8; 80_000]);

    let gen_out = run_cli(&[
        "update".to_string(),
        "generate-manifest".to_string(),
        "--version".to_string(),
        "0.2.0".to_string(),
        "--channel".to_string(),
        "stable".to_string(),
        "--arch".to_string(),
        "x86_64".to_string(),
        "--notes".to_string(),
        "OTA update release".to_string(),
        "--vmlinuz".to_string(),
        vm.display().to_string(),
        "--initramfs".to_string(),
        initrd.display().to_string(),
        "--squashfs".to_string(),
        squash.display().to_string(),
        "--base-url".to_string(),
        "https://cdn.example.com/0.2.0".to_string(),
        "--output".to_string(),
        manifest_path.display().to_string(),
    ])
    .unwrap();
    let parsed_gen = UpdateManifest::parse_json_str(&gen_out).unwrap();
    assert_eq!(parsed_gen.version, "0.2.0");
    assert_eq!(
        parsed_gen.squashfs.url,
        "https://cdn.example.com/0.2.0/unicore-rootfs-x86_64.squashfs"
    );

    // Enforce signature when pubkey exists, allow when --allow-unsigned is passed,
    // and fail when REQUIRE_SIGNATURE=1 is active without a configured pubkey.
    let missing_pub = tmp.root.join("missing.pub");
    assert_eq!(
        verify_manifest_signature(&manifest_path, None, &missing_pub, false, false).unwrap(),
        SignatureStatus::NoPubkeyConfigured
    );
    assert!(verify_manifest_signature(&manifest_path, None, &missing_pub, false, true).is_err());
    assert_eq!(
        verify_manifest_signature(&manifest_path, None, &missing_pub, true, true).unwrap(),
        SignatureStatus::UnsignedAllowed
    );

    let fake_pub = tmp.root.join("update.pub");
    write_file(&fake_pub, b"dummy-pubkey");
    assert!(verify_manifest_signature(&manifest_path, None, &fake_pub, false, false).is_err());
    assert_eq!(
        verify_manifest_signature(&manifest_path, None, &fake_pub, true, false).unwrap(),
        SignatureStatus::UnsignedAllowed
    );

    let verified_out = run_cli(&[
        "update".to_string(),
        "verify-manifest".to_string(),
        manifest_path.display().to_string(),
        "--pubkey".to_string(),
        fake_pub.display().to_string(),
        "--allow-unsigned".to_string(),
        "--current-version".to_string(),
        "0.1.0".to_string(),
    ])
    .unwrap();
    let vjson = parse_json(&verified_out).unwrap();
    let vobj = vjson.as_object().unwrap();
    assert_eq!(vobj.get("valid").and_then(JsonValue::as_bool), Some(true));
    assert_eq!(
        vobj.get("update_available").and_then(JsonValue::as_bool),
        Some(true)
    );
    assert_eq!(
        vobj.get("signature_status").and_then(JsonValue::as_str),
        Some("unsigned_allowed")
    );

    // Verify file SHA-256 passes, then fails on tampered payload
    let sq_sha = vobj
        .get("squashfs_sha256")
        .and_then(JsonValue::as_str)
        .unwrap()
        .to_string();
    assert!(run_cli(&[
        "update".to_string(),
        "verify-file".to_string(),
        squash.display().to_string(),
        sq_sha.clone(),
    ])
    .is_ok());

    write_file(&squash, b"tampered-squashfs");
    assert!(run_cli(&[
        "update".to_string(),
        "verify-file".to_string(),
        squash.display().to_string(),
        sq_sha,
    ])
    .is_err());

    // Reject path traversal filename inside manifest
    let malicious = gen_out.replace("unicore-rootfs-x86_64.squashfs", "../evil.squashfs");
    assert!(UpdateManifest::parse_json_str(&malicious).is_err());
}

#[test]
fn grubenv_1024_ab_trial_auto_rollback_confirm_and_manual_rollback() {
    let tmp = TempWorkspace::new("grubenv");
    let grubenv_path = tmp.root.join("grubenv");

    // 1. Initial Slot A (0.1.0), exact 1024-byte GRUB environment block
    let mut env = GrubEnv::new_initial(Slot::SlotA, "0.1.0");
    let rendered = env.render_1024().unwrap();
    assert_eq!(rendered.len(), 1024);
    env.write_file_atomic(&grubenv_path).unwrap();
    assert_eq!(fs::read(&grubenv_path).unwrap().len(), 1024);

    // 2. Stage 0.2.0 into Slot B -> trial boot enters Slot B with boot_tried=1
    env.stage_update(Slot::SlotB, "0.2.0");
    let (boot1, rolled_back1) = env.simulate_grub_boot();
    assert_eq!(boot1, Slot::SlotB);
    assert!(!rolled_back1);
    assert!(env.boot_tried());

    // 3. Simulated crash (never confirmed) -> next boot auto-rolls back to Slot A
    let (boot2, rolled_back2) = env.simulate_grub_boot();
    assert_eq!(boot2, Slot::SlotA);
    assert!(rolled_back2);
    assert_eq!(env.pending_slot(), None);
    assert!(!env.boot_tried());

    // 4. Re-stage Slot B -> trial boot -> confirm healthy boot -> Slot B becomes active
    env.stage_update(Slot::SlotB, "0.2.0");
    let (boot3, rolled_back3) = env.simulate_grub_boot();
    assert_eq!(boot3, Slot::SlotB);
    assert!(!rolled_back3);
    assert!(env.confirm_boot(Slot::SlotB));
    assert_eq!(env.active_slot(), Slot::SlotB);
    assert_eq!(env.pending_slot(), None);
    assert_eq!(env.slot_version(Slot::SlotA), "0.1.0");
    assert_eq!(env.slot_version(Slot::SlotB), "0.2.0");

    // 5. CLI grubenv roundtrip + manual rollback to Slot A
    env.write_file_atomic(&grubenv_path).unwrap();
    let rb_out = run_cli(&[
        "update".to_string(),
        "grubenv".to_string(),
        "rollback".to_string(),
        "--file".to_string(),
        grubenv_path.display().to_string(),
    ])
    .unwrap();
    let rb_json = parse_json(&rb_out).unwrap();
    assert_eq!(
        rb_json
            .as_object()
            .unwrap()
            .get("rolled_back_to")
            .and_then(JsonValue::as_str),
        Some("UNICORE_SYS_A")
    );
    assert_eq!(fs::read(&grubenv_path).unwrap().len(), 1024);
    let reloaded = GrubEnv::read_file(&grubenv_path).unwrap();
    assert_eq!(reloaded.active_slot(), Slot::SlotA);
}

#[test]
fn sandbox_bwrap_confinement_enforces_all_five_permissions() {
    let tmp = TempWorkspace::new("bwrap_confinement");
    let state_dir = tmp.root.join("state");
    let state_arg = state_dir.display().to_string();
    let mut manager = SandboxManager::new(&state_dir, 30.0).unwrap();

    let elf = tmp.root.join("scoped_app");
    write_file(&elf, &build_elf_bytes(0x3E, true));
    let app = manager.install(&elf, Some("ScopedApp")).unwrap();

    let video_devs = vec!["/dev/video0".to_string()];
    let ctx = ConfinementContext {
        xdg_runtime_dir: Some("/run/user/1000"),
        wayland_display: Some("wayland-0"),
        has_x11: true,
        video_devices: &video_devs,
    };

    // Default privacy-first permissions: network=true, gpu=true, docs=false, camera=false, mic=false
    let args_default = build_bwrap_args(&app, SubsystemKind::Native, &ctx);
    assert!(args_default.windows(3).any(|w| w == ["--ro-bind", "/usr", "/usr"]));
    assert!(args_default.windows(3).any(|w| w == ["--ro-bind-try", "/dev/null", "/etc/doas.conf"]));
    assert!(args_default.windows(4).any(|w| w == ["--tmpfs", "/data", "--dir", "/data/network"]));
    assert!(args_default.windows(3).any(|w| w == ["--bind", &app.sandbox_dir, &app.sandbox_dir]));
    assert!(args_default.windows(3).any(|w| w == ["--dev-bind-try", "/dev/dri", "/dev/dri"]));
    assert!(!args_default.contains(&"--unshare-net".to_string()));
    assert!(!args_default.contains(&"/dev/video0".to_string()));
    assert!(!args_default.contains(&"/dev/snd".to_string()));
    assert!(!args_default.contains(&"/data/home/Documents".to_string()));
    assert!(args_default.contains(&"--unshare-pid".to_string()));

    // Win32 omits --unshare-pid so sibling wineserver IPC works inside WINEPREFIX
    let args_win32 = build_bwrap_args(&app, SubsystemKind::Win32, &ctx);
    assert!(!args_win32.contains(&"--unshare-pid".to_string()));

    // Flip all 5 permissions and verify bwrap confinement flags track each one
    manager.set_permission(&app.app_id, "network", false).unwrap();
    manager
        .set_permission(&app.app_id, "gpu_acceleration", false)
        .unwrap();
    manager.set_permission(&app.app_id, "camera", true).unwrap();
    manager.set_permission(&app.app_id, "microphone", true).unwrap();
    let updated = manager
        .set_permission(&app.app_id, "shared_documents", true)
        .unwrap();

    let args_flipped = build_bwrap_args(&updated, SubsystemKind::Native, &ctx);
    assert!(args_flipped.contains(&"--unshare-net".to_string()));
    assert!(!args_flipped.contains(&"/dev/dri".to_string()));
    assert!(args_flipped.windows(3).any(|w| w == ["--dev-bind", "/dev/video0", "/dev/video0"]));
    assert!(args_flipped.windows(3).any(|w| w == ["--dev-bind-try", "/dev/snd", "/dev/snd"]));
    assert!(args_flipped.windows(3).any(|w| w == ["--bind", "/data/home/Documents", "/data/home/Documents"]));

    // Verify launch env exports UNILOAD_SANDBOX_DIR, UNILOAD_APP_SOURCE, UNILOAD_SANDBOX_CONFINEMENT
    let plan = manager.launch(&app.app_id, "x86_64", Some(10.0)).unwrap();
    assert_eq!(
        plan.env.get("UNILOAD_SANDBOX_DIR").map(String::as_str),
        Some(app.sandbox_dir.as_str())
    );
    assert_eq!(
        plan.env.get("UNILOAD_APP_SOURCE").map(String::as_str),
        Some(app.source_path.as_str())
    );
    assert_eq!(
        plan.env.get("UNILOAD_SANDBOX_CONFINEMENT").map(String::as_str),
        Some("bwrap")
    );

    // Verify `uniload bwrap-args <APP_ID>` CLI emits valid JSON array
    let cli_out = run_cli(&[
        "--state-dir".to_string(),
        state_arg,
        "bwrap-args".to_string(),
        app.app_id,
    ])
    .unwrap();
    let parsed = parse_json(&cli_out).unwrap();
    let arr = parsed.as_array().unwrap();
    assert!(arr.iter().any(|v| v.as_str() == Some("--unshare-net")));
    assert!(arr.iter().any(|v| v.as_str() == Some("--unshare-pid")));
}

#[test]
fn phase10_android_shortcuts_and_phase11_arch_normalization() {
    let waydroid_out = "\
Name: F-Droid
packageName: org.fdroid.fdroid
vendorID: 0

Name: UniCore Hello
packageName: org.unicore.hello
";
    let apps = parse_waydroid_app_list(waydroid_out);
    assert_eq!(
        apps,
        vec![
            AndroidAppInfo {
                name: "F-Droid".to_string(),
                package_name: "org.fdroid.fdroid".to_string(),
            },
            AndroidAppInfo {
                name: "UniCore Hello".to_string(),
                package_name: "org.unicore.hello".to_string(),
            },
        ]
    );
    let desktop = render_android_desktop_entry(&apps[0]);
    assert!(desktop.contains("Name=F-Droid"));
    assert!(desktop.contains("Exec=/usr/bin/waydroid app launch org.fdroid.fdroid"));

    let sync_dry = run_cli(&[
        "android".to_string(),
        "sync-shortcuts".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();
    let sync_json = parse_json(&sync_dry).unwrap();
    assert_eq!(
        sync_json
            .as_object()
            .unwrap()
            .get("action")
            .and_then(JsonValue::as_str),
        Some("sync-shortcuts")
    );

    assert_eq!(normalize_arch("aarch64"), "arm64");
    assert_eq!(normalize_arch("arm64"), "arm64");
    assert_eq!(normalize_arch("amd64"), "x86_64");
    assert_eq!(normalize_arch("i686"), "x86");

    let tmp = TempWorkspace::new("arch_manifest");
    let vm = tmp.root.join("vmlinuz-aarch64");
    let initrd = tmp.root.join("initramfs-aarch64.img");
    let sq = tmp.root.join("unicore-rootfs-aarch64.squashfs");
    let mf = tmp.root.join("unicore-manifest.json");
    write_file(&vm, b"arm64-kernel");
    write_file(&initrd, b"arm64-initrd");
    write_file(&sq, b"arm64-squashfs");

    run_cli(&[
        "update".to_string(),
        "generate-manifest".to_string(),
        "--version".to_string(),
        "0.2.0".to_string(),
        "--arch".to_string(),
        "arm64".to_string(),
        "--vmlinuz".to_string(),
        vm.display().to_string(),
        "--initramfs".to_string(),
        initrd.display().to_string(),
        "--squashfs".to_string(),
        sq.display().to_string(),
        "--output".to_string(),
        mf.display().to_string(),
    ])
    .unwrap();

    let v_out = run_cli(&[
        "update".to_string(),
        "verify-manifest".to_string(),
        mf.display().to_string(),
        "--arch".to_string(),
        "aarch64".to_string(),
        "--allow-unsigned".to_string(),
    ])
    .unwrap();
    assert_eq!(
        parse_json(&v_out)
            .unwrap()
            .as_object()
            .unwrap()
            .get("valid")
            .and_then(JsonValue::as_bool),
        Some(true)
    );
}

#[test]
fn win32_assets_cli_status_and_install_dry_run() {
    let status_out = run_cli(&[
        "win32".to_string(),
        "status".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();
    let status_json = parse_json(&status_out).unwrap();
    let status_obj = status_json.as_object().unwrap();
    assert_eq!(status_obj.get("action").and_then(JsonValue::as_str), Some("status"));
    assert_eq!(
        status_obj.get("runtime_binary").and_then(JsonValue::as_str),
        Some("/usr/libexec/uniload-win32-assets")
    );

    let dxvk_out = run_cli(&[
        "win32".to_string(),
        "install-dxvk".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();
    let dxvk_json = parse_json(&dxvk_out).unwrap();
    assert_eq!(
        dxvk_json
            .as_object()
            .unwrap()
            .get("action")
            .and_then(JsonValue::as_str),
        Some("install-dxvk")
    );

    let ddraw_out = run_cli(&[
        "win32".to_string(),
        "install-cnc-ddraw".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();
    let ddraw_json = parse_json(&ddraw_out).unwrap();
    assert_eq!(
        ddraw_json
            .as_object()
            .unwrap()
            .get("action")
            .and_then(JsonValue::as_str),
        Some("install-cnc-ddraw")
    );

    let tmp = TempWorkspace::new("win32_rt");
    let state_dir = tmp.root.join("state");
    let state_arg = state_dir.display().to_string();
    let game_exe = tmp.root.join("Classic.exe");
    write_file(&game_exe, &build_pe_bytes_with_ver(0x014C, 2, 0x010B, 0x0002, 5));
    let mut manager = SandboxManager::new(&state_dir, 30.0).unwrap();
    let installed = manager.install(&game_exe, None).unwrap();

    let rt_out = run_cli(&[
        "--state-dir".to_string(),
        state_arg,
        "win32".to_string(),
        "install-runtimes".to_string(),
        installed.app_id.clone(),
        "--dry-run".to_string(),
    ])
    .unwrap();
    let rt_json = parse_json(&rt_out).unwrap();
    let rt_argv = rt_json
        .as_object()
        .unwrap()
        .get("argv")
        .and_then(JsonValue::as_array)
        .unwrap();
    assert_eq!(rt_argv[0].as_str(), Some("install-runtimes"));
    assert_eq!(rt_argv[1].as_str(), Some("--prefix"));

    assert!(run_cli(&["win32".to_string(), "bad-action".to_string()]).is_err());
}

#[test]
fn inspect_and_launch_embedded_dos_subsystem() {
    let tmp = TempWorkspace::new("dos16");
    let game_dir = tmp.root.join("DOOM93");
    fs::create_dir_all(&game_dir).unwrap();

    // Valid MS-DOS MZ executable (e_cp = 1 page, e_cparhdr = 2 paragraphs = 32 bytes, e_lfanew = 0).
    let dos_exe = game_dir.join("DOOM.EXE");
    let mut mz = vec![0u8; 64];
    mz[0..2].copy_from_slice(b"MZ");
    mz[4..6].copy_from_slice(&1u16.to_le_bytes());
    mz[8..10].copy_from_slice(&2u16.to_le_bytes());
    write_file(&dos_exe, &mz);

    let exe_info = inspect_binary(&dos_exe).unwrap();
    assert_eq!(exe_info.platform, AppPlatform::Dos16);
    assert_eq!(exe_info.arch, "x86_16");
    assert_eq!(exe_info.win_era, None);
    assert!(exe_info.is_gui);
    assert!(!exe_info.requires_cpu_translation("x86_64"));
    assert!(!exe_info.requires_cpu_translation("arm64"));

    // Valid MS-DOS .COM and .BAT files also classify as AppPlatform::Dos16.
    let com_file = game_dir.join("HELLO.COM");
    write_file(&com_file, b"\xb8\x00\x4c\xcd\x21");
    assert_eq!(inspect_binary(&com_file).unwrap().platform, AppPlatform::Dos16);

    let bat_file = game_dir.join("PLAY.BAT");
    write_file(&bat_file, b"@echo off\r\nDOOM.EXE\r\n");
    assert_eq!(inspect_binary(&bat_file).unwrap().platform, AppPlatform::Dos16);

    // Install & launch via embedded SubsystemKind::Dos without exposing third-party launchers.
    let state_dir = tmp.root.join("state");
    let mut manager = SandboxManager::new(&state_dir, 30.0).unwrap();
    let app = manager.install(&dos_exe, Some("DOOM 1993")).unwrap();
    assert_eq!(app.platform, AppPlatform::Dos16);

    let plan = manager.launch(&app.app_id, "x86_64", Some(1.0)).unwrap();
    assert_eq!(plan.subsystem, SubsystemKind::Dos);
    assert_eq!(plan.runtime_binary, "/usr/libexec/uniload-dos");
    assert_eq!(plan.env.get("UNILOAD_SUBSYSTEM").map(String::as_str), Some("dos"));
    assert!(plan.env.contains_key("UNILOAD_DOS_CONF"));
    assert!(plan.env.contains_key("UNILOAD_DOS_DRIVE_C"));
    assert!(!plan.env.get("UNILOAD_DOS_WORKDIR").unwrap().is_empty());

    let bwrap_out = run_cli(&[
        "--state-dir".to_string(),
        state_dir.display().to_string(),
        "bwrap-args".to_string(),
        app.app_id.clone(),
    ])
    .unwrap();
    assert!(bwrap_out.contains("DOOM93"));

    // Also verify status --dry-run reports dos_subsystem_ready field instead of dosbox_installed.
    let status_out = run_cli(&[
        "win32".to_string(),
        "status".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();
    let status_json = parse_json(&status_out).unwrap();
    let status_obj = status_json.as_object().unwrap();
    assert!(status_obj.contains_key("dos_subsystem_ready"));
    assert!(!status_obj.contains_key("dosbox_installed"));
}

#[test]
fn catalog_parse_filter_sha256_and_offline_install() {
    use uniload::catalog::{
        filter_catalog_apps, filter_catalog_apps_for_arch, install_catalog_app,
        parse_catalog_json, validate_flatpak_id, validate_safe_rel_path, verify_file_sha256,
        CatalogApp, EMBEDDED_CATALOG_JSON,
    };

    let catalog = parse_catalog_json(EMBEDDED_CATALOG_JSON).unwrap();
    assert_eq!(catalog.version, 2);
    assert_eq!(catalog.apps.len(), 33);
    for app in &catalog.apps {
        if !app.download_url.is_empty() {
            assert!(app.download_url.starts_with("https://"), "{}", app.id);
            assert_eq!(app.sha256.len(), 64, "{}", app.id);
        }
        if app.is_flatpak() {
            assert!(validate_flatpak_id(&app.flatpak_id).is_ok(), "{}", app.id);
        }
    }

    let dev = filter_catalog_apps(&catalog.apps, "dev", "");
    assert_eq!(dev.len(), 3);
    assert!(dev.iter().any(|a| a.id == "antigravity" && a.version == "2.35.2"));
    let office = filter_catalog_apps(&catalog.apps, "office", "");
    assert_eq!(office.len(), 3);
    assert!(office.iter().any(|a| a.id == "libreoffice-native" && a.is_flatpak()));
    let gaming_x64 = filter_catalog_apps_for_arch(&catalog.apps, "gaming", "", "x86_64");
    assert_eq!(gaming_x64.len(), 5);
    let gaming_arm = filter_catalog_apps_for_arch(&catalog.apps, "gaming", "", "aarch64");
    assert_eq!(gaming_arm.len(), 2);
    assert!(validate_flatpak_id("com.valvesoftware.Steam").is_ok());
    assert!(validate_flatpak_id("bad;rm").is_err());
    let pdf = filter_catalog_apps(&catalog.apps, "all", "pdf");
    assert_eq!(pdf.len(), 1);
    assert_eq!(pdf[0].id, "sumatrapdf");
    let android = filter_catalog_apps(&catalog.apps, "android", "");
    assert_eq!(android.len(), 2);
    let retro = filter_catalog_apps(&catalog.apps, "retro", "");
    assert_eq!(retro.len(), 4);
    assert!(retro.iter().any(|a| a.id == "commander-keen-1" && a.platform == AppPlatform::Dos16));
    assert!(retro.iter().any(|a| a.id == "tyrian-21" && a.platform == AppPlatform::Dos16));
    assert!(retro
        .iter()
        .any(|a| a.id == "cave-story" && a.win_era == Some(WinEra::Win98)));

    assert!(validate_safe_rel_path("../evil.exe").is_err());
    assert!(validate_safe_rel_path("/abs/evil.exe").is_err());
    assert!(validate_safe_rel_path("sub/app.exe").is_ok());

    let list_out = run_cli(&["store".to_string(), "list".to_string()]).unwrap();
    let list_json = parse_json(&list_out).unwrap();
    assert_eq!(list_json.as_object().unwrap().get("apps").and_then(JsonValue::as_array).unwrap().len(), 33);

    let dry_out = run_cli(&[
        "store".to_string(),
        "install".to_string(),
        "7zip".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();
    let dry_json = parse_json(&dry_out).unwrap();
    let dry_obj = dry_json.as_object().unwrap();
    assert_eq!(dry_obj.get("dry_run").and_then(JsonValue::as_bool), Some(true));
    assert_eq!(
        dry_obj.get("sha256").and_then(JsonValue::as_str),
        Some("bdd1a33de78618d16ee4ce148b849932c05d0015491c34887846d431d29f308e")
    );

    // Verify local/offline install with real SHA-256 passes, and tampered SHA-256 fails.
    let tmp = TempWorkspace::new("catalog_offline");
    let sample_exe = tmp.root.join("tool.exe");
    write_file(&sample_exe, &build_pe_bytes(0x8664, 2, 0x020B, 0x0002));
    let real_sha = sha256_file(&sample_exe).unwrap();
    assert_eq!(verify_file_sha256(&sample_exe, &real_sha).unwrap(), real_sha);

    let state_dir = tmp.root.join("state");
    let mut manager = SandboxManager::new(&state_dir, 30.0).unwrap();
    let mut entry = CatalogApp {
        id: "sample-tool".to_string(),
        name: "Sample Tool".to_string(),
        version: "1.0".to_string(),
        size_label: "1 KB".to_string(),
        category: "utilities".to_string(),
        platform: AppPlatform::WindowsPe,
        win_era: Some(WinEra::Win7),
        flatpak_id: String::new(),
        arches: Vec::new(),
        download_url: String::new(),
        sha256: real_sha,
        filename: "tool.exe".to_string(),
        archive_entry: String::new(),
        bundled_path: sample_exe.display().to_string(),
        summary_en: "Sample".to_string(),
        summary_ms: "Contoh".to_string(),
    };
    let installed = install_catalog_app(&mut manager, &entry, &state_dir.join("store"), false).unwrap();
    assert_eq!(installed.win_era, Some(WinEra::Win7));
    assert!(manager.get_app(&installed.app_id).is_ok());

    entry.sha256 = "0".repeat(64);
    assert!(install_catalog_app(&mut manager, &entry, &state_dir.join("store"), false).is_err());

    // Multi-file Store apps/games under .../store/<id>/app/ expose UNILOAD_APP_WORKDIR for data/saves.
    let store_game_dir = state_dir.join("store").join("cave-story").join("app");
    fs::create_dir_all(&store_game_dir).unwrap();
    let store_game_exe = store_game_dir.join("Doukutsu.exe");
    write_file(&store_game_exe, &build_pe_bytes(0x014C, 2, 0x010B, 0x0002));
    let sg_app = manager.install(&store_game_exe, Some("Cave Story")).unwrap();
    let sg_plan = manager.launch(&sg_app.app_id, "x86_64", Some(2.0)).unwrap();
    assert!(sg_plan
        .env
        .get("UNILOAD_APP_WORKDIR")
        .is_some_and(|w| w.contains("cave-story")));
}

#[test]
fn phase25_app_launch_config_env_and_prefix_reset_work() {
    use uniload::sandbox::{format_env_vars_str, parse_env_vars_str, split_launch_args};

    let args = split_launch_args("-windowed -res \"1024 768\" --fast");
    assert_eq!(args, vec!["-windowed", "-res", "1024 768", "--fast"]);

    let env = parse_env_vars_str("WINEARCH=win32, DXVK_HUD=fps").unwrap();
    assert_eq!(env.get("WINEARCH").map(String::as_str), Some("win32"));
    assert_eq!(env.get("DXVK_HUD").map(String::as_str), Some("fps"));
    assert!(format_env_vars_str(&env).contains("DXVK_HUD=fps"));
    assert!(parse_env_vars_str("1BAD_KEY=x").is_err());

    let tmp = TempWorkspace::new("adv_config");
    let state_dir = tmp.root.join("state");
    let game_exe = tmp.root.join("AdvGame.exe");
    write_file(&game_exe, &build_pe_bytes_with_ver(0x014C, 2, 0x010B, 0x0002, 5));

    let cli_inst = run_cli(&[
        "--state-dir".to_string(),
        state_dir.display().to_string(),
        "install".to_string(),
        game_exe.display().to_string(),
    ])
    .unwrap();
    let app_id = parse_json(&cli_inst)
        .unwrap()
        .as_object()
        .unwrap()
        .get("app_id")
        .and_then(JsonValue::as_str)
        .unwrap()
        .to_string();

    run_cli(&[
        "--state-dir".to_string(),
        state_dir.display().to_string(),
        "config".to_string(),
        app_id.clone(),
        "--args".to_string(),
        "-fullscreen -nosound".to_string(),
        "--env".to_string(),
        "PROTON_NO_ESYNC=1, MY_FLAG=yes".to_string(),
    ])
    .unwrap();

    let launch_out = run_cli(&[
        "--state-dir".to_string(),
        state_dir.display().to_string(),
        "launch".to_string(),
        app_id.clone(),
        "--dry-run".to_string(),
    ])
    .unwrap();
    let ljson = parse_json(&launch_out).unwrap();
    let lobj = ljson.as_object().unwrap();
    let argv: Vec<String> = lobj
        .get("argv")
        .and_then(JsonValue::as_array)
        .unwrap()
        .iter()
        .filter_map(JsonValue::as_str)
        .map(str::to_string)
        .collect();
    assert!(argv.ends_with(&["-fullscreen".to_string(), "-nosound".to_string()]));
    let env_obj = lobj.get("env").and_then(JsonValue::as_object).unwrap();
    assert_eq!(env_obj.get("MY_FLAG").and_then(JsonValue::as_str), Some("yes"));

    let reset_out = run_cli(&[
        "--state-dir".to_string(),
        state_dir.display().to_string(),
        "win32".to_string(),
        "reset-prefix".to_string(),
        app_id,
    ])
    .unwrap();
    assert_eq!(
        parse_json(&reset_out)
            .unwrap()
            .as_object()
            .unwrap()
            .get("reset")
            .and_then(JsonValue::as_bool),
        Some(true)
    );
}
