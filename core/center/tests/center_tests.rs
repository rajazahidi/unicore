use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use unicore_center::backend::{CenterBackend, CenterConfig, InstallRequest};
use unicore_center::cli::{parse_args, LaunchMode};
use unicore_center::desktop::{
    platform_display_name, render_desktop_entry, scan_launchable_files, supports_win_era,
    win_era_from_index, win_era_to_index,
};
use unicore_center::gaming::{
    load_gaming_config, parse_gaming_conf, read_cpu_governor_from, render_gaming_conf,
    save_gaming_config, GamingConfig,
};
use unicore_center::i18n::{detect_ui_lang, persist_ui_lang, UiLabels, UiLang};
use unicore_center::settings::{
    clamp_nightlight_temp, detect_vpn_import_type, load_nightlight_config,
    parse_backup_targets, parse_bluetooth_devices, parse_firewall_status, parse_lpinfo_devices,
    parse_lpstat_printers, parse_nightlight_conf, parse_nmcli_vpn_list, parse_nmcli_wifi_list,
    parse_pactl_sinks, parse_pactl_volume, parse_wlr_randr, read_printers_overview,
    render_display_apply_script, render_nightlight_conf, save_and_apply_nightlight,
    validate_backup_passphrase, validate_backup_path, validate_printer_name, validate_printer_uri,
    NightLightConfig,
};
use unicore_center::taskmgr::{parse_proc_status, read_processes_from, validate_kill_target};
use uniload::inspector::WinEra;
use uniload::sandbox::PermissionProfile;

fn make_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "unicore_center_test_{label}_{}_{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Write a minimal synthetic WinXP 32-bit PE header (`MajorSubsystemVersion = 5.1`).
fn build_minimal_pe(major_subsys: u16, minor_subsys: u16) -> Vec<u8> {
    let mut data = vec![0u8; 512];
    data[0] = b'M';
    data[1] = b'Z';
    let pe_off: usize = 0x80;
    data[0x3C..0x40].copy_from_slice(&(pe_off as u32).to_le_bytes());
    data[pe_off..pe_off + 4].copy_from_slice(b"PE\0\0");
    // COFF Machine = i386 (0x014c), OptionalHeaderSize = 0xE0
    let coff = pe_off + 4;
    data[coff..coff + 2].copy_from_slice(&0x014Cu16.to_le_bytes());
    data[coff + 16..coff + 18].copy_from_slice(&0x00E0u16.to_le_bytes());
    // Optional header PE32 magic = 0x010B
    let opt = coff + 20;
    data[opt..opt + 2].copy_from_slice(&0x010Bu16.to_le_bytes());
    // Major/MinorSubsystemVersion at opt + 48
    data[opt + 48..opt + 50].copy_from_slice(&major_subsys.to_le_bytes());
    data[opt + 50..opt + 52].copy_from_slice(&minor_subsys.to_le_bytes());
    // Subsystem = WINDOWS_GUI (2) at opt + 68
    data[opt + 68..opt + 70].copy_from_slice(&2u16.to_le_bytes());
    data
}

#[test]
fn cli_parses_open_and_state_overrides() {
    let args: Vec<String> = vec![
        "--state-dir",
        "/data/uniload",
        "--apps-dir",
        "/home/unicore/.local/share/applications",
        "--desktop-dir",
        "/home/unicore/Desktop",
        "--open",
        "/tmp/demo.exe",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();

    let parsed = parse_args(&args).expect("parse ok").expect("not help");
    assert_eq!(parsed.mode, LaunchMode::OpenDialog(PathBuf::from("/tmp/demo.exe")));
    assert_eq!(parsed.config.state_dir, PathBuf::from("/data/uniload"));
    assert_eq!(
        parsed.config.apps_dir,
        PathBuf::from("/home/unicore/.local/share/applications")
    );
    assert_eq!(
        parsed.config.desktop_dir,
        Some(PathBuf::from("/home/unicore/Desktop"))
    );
}

#[test]
fn backend_inspect_install_permissions_and_desktop_shortcut() {
    let root = make_temp_dir("install_flow");
    let state_dir = root.join("state");
    let apps_dir = root.join("applications");
    let desktop_dir = root.join("Desktop");
    fs::create_dir_all(&desktop_dir).unwrap();

    let sample_exe = root.join("RetroGame98.exe");
    fs::write(&sample_exe, build_minimal_pe(4, 10)).unwrap();

    let backend = CenterBackend::new(CenterConfig {
        state_dir: state_dir.clone(),
        apps_dir: apps_dir.clone(),
        desktop_dir: Some(desktop_dir.clone()),
        uniload_bin: PathBuf::from("/usr/bin/uniload"),
        host_arch: "x86_64".to_string(),
    });

    // 1. Inspect before install detects Win98 era + default privacy-first permissions
    let inspected = backend.inspect_file(&sample_exe).expect("inspect ok");
    assert_eq!(inspected.win_era, Some(WinEra::Win98));
    assert!(inspected.supports_era);
    assert!(inspected.already_installed_id.is_none());
    assert!(!inspected.permissions.shared_documents);

    // 2. Install with overridden WinXP era and enabled shared_documents + network=false
    let installed = backend
        .install_file(
            &sample_exe,
            &InstallRequest {
                app_name: Some("Retro Game".to_string()),
                win_era: Some(WinEra::WinXp),
                permissions: PermissionProfile {
                    network: false,
                    gpu_acceleration: true,
                    shared_documents: true,
                    camera: false,
                    microphone: false,
                },
                create_shortcut: true,
            },
        )
        .expect("install ok");

    assert_eq!(installed.win_era, Some(WinEra::WinXp));
    assert!(!installed.permissions.network);
    assert!(installed.permissions.shared_documents);

    // 3. Verify generated .desktop files launch directly via `uniload launch <app_id>`
    let expected_name = format!("unicore-app-{}.desktop", installed.app_id);
    let menu_desktop = apps_dir.join(&expected_name);
    let user_desktop = desktop_dir.join(&expected_name);
    assert!(menu_desktop.is_file(), "Start Menu .desktop missing");
    assert!(user_desktop.is_file(), "Desktop .desktop missing");

    let desktop_text = fs::read_to_string(&menu_desktop).unwrap();
    let expected_rendered = render_desktop_entry(
        &installed,
        &state_dir,
        &PathBuf::from("/usr/bin/uniload"),
    );
    assert_eq!(desktop_text, expected_rendered);
    assert!(desktop_text.contains("Name=Retro Game"));
    assert!(desktop_text.contains(&format!("launch {}", installed.app_id)));
    assert!(desktop_text.contains("Terminal=false"));

    // 4. Subsequent re-inspection loads persisted WinXP & permissions
    let re_inspected = backend.inspect_file(&sample_exe).expect("re-inspect ok");
    assert_eq!(re_inspected.already_installed_id, Some(installed.app_id.clone()));
    assert_eq!(re_inspected.win_era, Some(WinEra::WinXp));
    assert!(!re_inspected.permissions.network);
    assert!(re_inspected.permissions.shared_documents);

    // 5. Launch command uses --state-dir & app_id directly
    let launch_cmd = backend.build_launch_command(&installed.app_id);
    let argv: Vec<String> = launch_cmd
        .get_args()
        .map(|s| s.to_string_lossy().to_string())
        .collect();
    assert!(argv.contains(&"launch".to_string()));
    assert!(argv.contains(&installed.app_id));

    // 6. Uninstall removes sandbox dir, registry entry, and .desktop shortcuts
    let (removed, shortcut_count) = backend
        .uninstall_app(&installed.app_id)
        .expect("uninstall ok");
    assert_eq!(removed.app_id, installed.app_id);
    assert_eq!(shortcut_count, 2);
    assert!(!menu_desktop.exists());
    assert!(!user_desktop.exists());
    assert!(!PathBuf::from(&installed.sandbox_dir).exists());
    assert!(backend.list_apps().unwrap().is_empty());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn browse_scanner_finds_exe_and_apk_only() {
    let root = make_temp_dir("scanner");
    fs::write(root.join("app.exe"), b"MZ").unwrap();
    fs::write(root.join("mobile.apk"), b"PK").unwrap();
    fs::write(root.join("notes.txt"), b"hello").unwrap();

    let found = scan_launchable_files(std::slice::from_ref(&root));
    let names: Vec<String> = found.into_iter().map(|c| c.name).collect();
    assert_eq!(names, vec!["app.exe", "mobile.apk"]);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cli_parses_tab_and_power_modes() {
    let tab_args: Vec<String> = vec!["--tab", "settings"].into_iter().map(str::to_string).collect();
    let parsed_tab = parse_args(&tab_args).unwrap().unwrap();
    assert_eq!(parsed_tab.mode, LaunchMode::FullCenter);
    assert_eq!(parsed_tab.initial_tab, 3);

    let power_args: Vec<String> = vec!["--power"].into_iter().map(str::to_string).collect();
    let parsed_power = parse_args(&power_args).unwrap().unwrap();
    assert_eq!(parsed_power.mode, LaunchMode::PowerDialog);
}

#[test]
fn settings_parsers_handle_wifi_bt_audio_and_display() {
    let wifi_raw = "\
 :Cafe\\:5G:62:WPA2
*:HomeNet:88:WPA2 WPA3
 :HomeNet:45:WPA2
 :--:50:
 :GuestOpen:30:--
";
    let nets = parse_nmcli_wifi_list(wifi_raw);
    assert_eq!(nets.len(), 3);
    assert_eq!(nets[0].ssid, "HomeNet");
    assert!(nets[0].in_use);
    assert_eq!(nets[0].signal, 88);
    assert_eq!(nets[1].ssid, "Cafe:5G");
    assert_eq!(nets[2].ssid, "GuestOpen");
    assert_eq!(nets[2].security, "Open");

    let bt_raw = "Device AA:BB:CC:11:22:33 Sony WH-1000XM5\nDevice 00:11:22:33:44:55 Mouse\nBad line\n";
    let devs = parse_bluetooth_devices(bt_raw);
    assert_eq!(devs.len(), 2);
    assert_eq!(devs[0].mac, "AA:BB:CC:11:22:33");
    assert_eq!(devs[0].name, "Sony WH-1000XM5");

    let vol_raw = "Volume: front-left: 49152 /  75% / -7.50 dB,   front-right: 49152 /  75% / -7.50 dB";
    assert_eq!(parse_pactl_volume(vol_raw), 75);

    let sinks_raw = "0\talsa_output.pci-0000.analog-stereo\tPipeWire\ts16le\n1\tbluez_output.AA_BB.a2dp-sink\tPipeWire\n";
    let sinks = parse_pactl_sinks(sinks_raw, "alsa_output.pci-0000.analog-stereo");
    assert_eq!(sinks.len(), 2);
    assert!(sinks[0].is_default);
    assert!(!sinks[1].is_default);

    let randr_raw = "\
Virtual-1 \"QEMU\"
  Enabled: yes
  Modes:
    1280x800 px, 60.000000 Hz (preferred, current)
    1024x768 px, 60.000000 Hz
  Scale: 1.000000
";
    let outs = parse_wlr_randr(randr_raw);
    assert_eq!(outs.len(), 1);
    assert_eq!(outs[0].name, "Virtual-1");
    assert_eq!(outs[0].current_mode, "1280x800@60Hz");
    assert_eq!(outs[0].modes, vec!["1280x800@60Hz", "1024x768@60Hz"]);
    assert_eq!(outs[0].scale, "1.0");

    let script = render_display_apply_script("Virtual-1", "1280x800@60Hz", "1.25").unwrap();
    assert!(script.contains("wlr-randr --output Virtual-1 --mode 1280x800@60Hz --scale 1.25"));
    assert!(render_display_apply_script("Virtual-1; rm -rf /", "1280x800@60Hz", "1.0").is_err());
}

#[test]
fn bluetooth_overview_returns_promptly_when_daemon_inactive() {
    let start = std::time::Instant::now();
    let overview = unicore_center::settings::read_bluetooth_overview();
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
    assert!(!overview.service_active);
    assert!(!overview.summary.is_empty());
}

#[test]
fn installer_parsers_and_confirmation_guard_work() {
    let boot = unicore_center::installer::parse_boot_env(
        "UNICORE_BOOT_MODE=live\nUNICORE_BOOT_MEDIA=/dev/sr0\nUNICORE_BOOT_SLOT=\n",
    );
    assert!(boot.is_live());
    assert_eq!(boot.boot_media, "/dev/sr0");
    assert_eq!(boot.active_slot, "");

    let installed = unicore_center::installer::parse_boot_env(
        "UNICORE_BOOT_MODE=installed\nUNICORE_BOOT_MEDIA=/dev/sda3\nUNICORE_BOOT_SLOT=UNICORE_SYS_A\n",
    );
    assert!(!installed.is_live());
    assert_eq!(installed.active_slot, "UNICORE_SYS_A");

    let tsv = "\
/dev/sr0\tsr0\t1.3 GB\tQEMU DVD-ROM\t0\tboot-media
/dev/sda\tsda\t256.0 GB\tSamsung SSD 860\t1\teligible
/dev/sdb\tsdb\t4.0 GB\tTiny USB\t0\ttoo-small
";
    let disks = unicore_center::installer::parse_install_disks(tsv);
    assert_eq!(disks.len(), 3);
    assert!(!disks[0].eligible);
    assert!(disks[1].eligible);
    assert_eq!(disks[1].name, "sda");
    assert!(!disks[2].eligible);

    assert!(unicore_center::installer::validate_install_confirmation("sda", "sda").is_ok());
    assert!(unicore_center::installer::validate_install_confirmation("sda", " sda ").is_ok());
    assert!(unicore_center::installer::validate_install_confirmation("sda", "sdb").is_err());
    assert!(unicore_center::installer::validate_install_confirmation("sda", "").is_err());
    assert!(unicore_center::installer::validate_install_confirmation("sda;rm", "sda;rm").is_err());
    assert!(unicore_center::installer::validate_install_confirmation("sda/1", "sda/1").is_err());
}

#[test]
fn updater_parsers_and_local_path_validation_work() {
    let status_json = r#"{
        "active_slot": "UNICORE_SYS_A",
        "allow_insecure_http": false,
        "boot_mode": "installed",
        "boot_tried": false,
        "can_rollback": true,
        "can_update": true,
        "channel": "stable",
        "current_version": "0.1.0",
        "pending_slot": "UNICORE_SYS_B",
        "pubkey_configured": false,
        "slot_a_version": "0.1.0",
        "slot_b_version": "0.2.0",
        "standby_slot": "UNICORE_SYS_B",
        "update_url": "https://updates.example.com/unicore-manifest.json"
    }"#;
    let overview = unicore_center::updater::parse_update_status_json(status_json).unwrap();
    assert_eq!(overview.active_slot, "UNICORE_SYS_A");
    assert_eq!(overview.standby_slot, "UNICORE_SYS_B");
    assert!(overview.can_update);
    assert!(overview.can_rollback);
    assert!(overview.pending_summary().contains("UNICORE_SYS_B"));

    let check_json = r#"{
        "channel": "stable",
        "current_version": "0.1.0",
        "latest_version": "0.2.0",
        "notes": "Security and kernel update",
        "signature_status": "verified",
        "source_url": "https://updates.example.com/unicore-manifest.json",
        "update_available": true
    }"#;
    let chk = unicore_center::updater::parse_update_check_json(check_json).unwrap();
    assert!(chk.update_available);
    assert_eq!(chk.latest_version, "0.2.0");
    assert_eq!(chk.signature_status, "verified");

    assert!(unicore_center::updater::validate_local_update_path("/data/home/Downloads/unicore.iso").is_ok());
    assert!(unicore_center::updater::validate_local_update_path("relative/path.iso").is_err());
    assert!(unicore_center::updater::validate_local_update_path("/tmp/../etc/passwd").is_err());
    assert!(unicore_center::updater::validate_local_update_path("/tmp/a;rm -rf /").is_err());
}

#[test]
fn oobe_and_luks2_validation_and_encrypted_boot_env_work() {
    let oobe_args: Vec<String> = vec!["--oobe"].into_iter().map(str::to_string).collect();
    let parsed_oobe = parse_args(&oobe_args).unwrap().unwrap();
    assert_eq!(parsed_oobe.mode, LaunchMode::OobeWizard);

    assert_eq!(
        unicore_center::oobe::validate_hostname("my-laptop-01").unwrap(),
        "my-laptop-01"
    );
    assert!(unicore_center::oobe::validate_hostname("-bad").is_err());
    assert!(unicore_center::oobe::validate_hostname("bad;host").is_err());

    assert_eq!(
        unicore_center::oobe::validate_timezone("Asia/Kuala_Lumpur").unwrap(),
        "Asia/Kuala_Lumpur"
    );
    assert!(unicore_center::oobe::validate_timezone("../etc/passwd").is_err());

    assert_eq!(unicore_center::oobe::validate_user_password("", "").unwrap(), None);
    assert!(unicore_center::oobe::validate_user_password("abc", "abc").is_err());
    assert!(unicore_center::oobe::validate_user_password("pass1", "pass2").is_err());
    assert_eq!(
        unicore_center::oobe::validate_user_password("unicore123", "unicore123").unwrap(),
        Some("unicore123".to_string())
    );

    let root = make_temp_dir("oobe_state");
    assert!(!unicore_center::oobe::is_oobe_done(&root));
    let cfg = unicore_center::oobe::OobeSettings {
        language: "ms_MY.UTF-8".to_string(),
        timezone: "Asia/Kuala_Lumpur".to_string(),
        hostname: "unicore-book".to_string(),
    };
    unicore_center::oobe::save_oobe_state(&root, &cfg).unwrap();
    assert!(unicore_center::oobe::is_oobe_done(&root));
    let conf_raw = fs::read_to_string(root.join("oobe.conf")).unwrap();
    let parsed_cfg = unicore_center::oobe::parse_oobe_conf(&conf_raw);
    assert_eq!(parsed_cfg, cfg);
    let _ = fs::remove_dir_all(&root);

    // LUKS2 passphrase guard and encrypted boot env parsing
    assert!(unicore_center::installer::validate_luks_passphrase("short", "short").is_err());
    assert!(unicore_center::installer::validate_luks_passphrase("LongPass123", "OtherPass").is_err());
    assert!(unicore_center::installer::validate_luks_passphrase("UniCoreSecret99!", "UniCoreSecret99!").is_ok());

    let enc_boot = unicore_center::installer::parse_boot_env(
        "UNICORE_BOOT_MODE=installed\nUNICORE_BOOT_MEDIA=/dev/vda3\nUNICORE_BOOT_SLOT=UNICORE_SYS_A\nUNICORE_DATA_ENCRYPTED=1\n",
    );
    assert!(enc_boot.data_encrypted);
    assert!(enc_boot.display_summary().contains("[LUKS2]"));
}

#[test]
fn settings_subtab_cli_parsing_and_app_log_reader_work() {
    let wifi_args: Vec<String> = vec!["--tab", "settings:wifi"].into_iter().map(str::to_string).collect();
    let parsed_wifi = parse_args(&wifi_args).unwrap().unwrap();
    assert_eq!(parsed_wifi.initial_tab, 3);
    assert_eq!(parsed_wifi.initial_settings_subtab, 0);

    let audio_args: Vec<String> = vec!["--tab", "settings:audio"].into_iter().map(str::to_string).collect();
    let parsed_audio = parse_args(&audio_args).unwrap().unwrap();
    assert_eq!(parsed_audio.initial_tab, 3);
    assert_eq!(parsed_audio.initial_settings_subtab, 2);

    let bad_args: Vec<String> = vec!["--tab", "settings:invalid"].into_iter().map(str::to_string).collect();
    assert!(parse_args(&bad_args).is_err());

    let root = make_temp_dir("app_log");
    let backend = CenterBackend::new(CenterConfig {
        state_dir: root.join("state"),
        apps_dir: root.join("apps"),
        desktop_dir: None,
        uniload_bin: PathBuf::from("/usr/bin/uniload"),
        host_arch: "x86_64".to_string(),
    });
    let app_id = format!("testlog_{}", std::process::id());
    let log_path = CenterBackend::app_log_path(&app_id);
    let _ = fs::remove_file(&log_path);
    assert!(backend.read_app_log(&app_id).contains("No runtime log"));

    fs::write(&log_path, "line1\nHello from Win32 sandbox\n").unwrap();
    let txt = backend.read_app_log(&app_id);
    assert!(txt.contains("Hello from Win32 sandbox"));
    let _ = fs::remove_file(&log_path);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn taskmgr_parsing_filtering_and_safety_guards_work() {
    let taskmgr_args: Vec<String> = vec!["--taskmgr"].into_iter().map(str::to_string).collect();
    let parsed = parse_args(&taskmgr_args).unwrap().unwrap();
    assert_eq!(parsed.mode, LaunchMode::TaskManager);

    let status_sample = "\
Name:\tunicore-center
State:\tS (sleeping)
Pid:\t142
Uid:\t1000\t1000\t1000\t1000
VmRSS:\t   23552 kB
";
    let parsed_status = parse_proc_status(status_sample);
    assert_eq!(parsed_status.name, "unicore-center");
    assert_eq!(parsed_status.uid, Some(1000));
    assert_eq!(parsed_status.ram_mb, 23);

    let root = make_temp_dir("proc_mock");
    // PID 1 (must be skipped)
    fs::create_dir_all(root.join("1")).unwrap();
    fs::write(root.join("1/status"), "Name:\tinit\nUid:\t0\t0\t0\t0\nVmRSS:\t2048 kB\n").unwrap();
    // Root daemon (must be filtered out when filter_uid=1000)
    fs::create_dir_all(root.join("55")).unwrap();
    fs::write(root.join("55/status"), "Name:\tpolkitd\nUid:\t0\t0\t0\t0\nVmRSS:\t8192 kB\n").unwrap();
    // Two user processes (400 should sort before 300 due to higher RAM)
    fs::create_dir_all(root.join("300")).unwrap();
    fs::write(root.join("300/status"), "Name:\tfoot\nState:\tS (sleeping)\nUid:\t1000\t1000\t1000\t1000\nVmRSS:\t10240 kB\n").unwrap();
    fs::write(root.join("300/cmdline"), b"foot\0-e\0sh\0").unwrap();
    fs::create_dir_all(root.join("400")).unwrap();
    fs::write(root.join("400/status"), "Name:\tepiphany\nState:\tR (running)\nUid:\t1000\t1000\t1000\t1000\nVmRSS:\t65536 kB\n").unwrap();
    fs::write(root.join("400/cmdline"), b"/usr/bin/epiphany\0").unwrap();

    let procs = read_processes_from(&root, Some(1000), Some(999));
    assert_eq!(procs.len(), 2);
    assert_eq!(procs[0].pid, 400);
    assert_eq!(procs[0].ram_mb, 64);
    assert_eq!(procs[1].pid, 300);
    assert_eq!(procs[1].cmdline, "foot -e sh");

    assert!(validate_kill_target(1, 999, Some(0), Some(1000)).is_err());
    assert!(validate_kill_target(999, 999, Some(1000), Some(1000)).is_err());
    assert!(validate_kill_target(55, 999, Some(0), Some(1000)).is_err());
    assert!(validate_kill_target(400, 999, Some(1000), Some(1000)).is_ok());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn i18n_locale_detection_persistence_and_bilingual_labels_work() {
    let root = make_temp_dir("i18n_test");
    let conf_path = root.join("oobe.conf");

    // Fallback to LANG env var when oobe.conf does not exist yet
    assert_eq!(detect_ui_lang(&conf_path, Some("ms_MY.UTF-8")), UiLang::Ms);
    assert_eq!(detect_ui_lang(&conf_path, Some("en_US.UTF-8")), UiLang::En);
    assert_eq!(detect_ui_lang(&conf_path, None), UiLang::En);

    // Persisting language updates oobe.conf and overrides env var
    persist_ui_lang(&root, UiLang::Ms).unwrap();
    assert_eq!(detect_ui_lang(&conf_path, Some("en_US.UTF-8")), UiLang::Ms);
    persist_ui_lang(&root, UiLang::En).unwrap();
    assert_eq!(detect_ui_lang(&conf_path, Some("ms_MY.UTF-8")), UiLang::En);

    let en = UiLabels::for_lang(UiLang::En);
    let ms = UiLabels::for_lang(UiLang::Ms);
    assert_ne!(en.title_center, ms.title_center);
    assert_ne!(en.btn_end_task, ms.btn_end_task);
    assert_ne!(en.btn_install_run, ms.btn_install_run);
    assert_ne!(en.hdr_gaming_perf, ms.hdr_gaming_perf);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn gaming_config_persistence_and_governor_reader_work() {
    let root = make_temp_dir("gaming_conf");
    assert_eq!(load_gaming_config(&root), GamingConfig::default());

    let cfg = GamingConfig {
        game_mode: true,
        show_fps: true,
    };
    save_gaming_config(&root, &cfg).unwrap();
    let loaded = load_gaming_config(&root);
    assert_eq!(loaded, cfg);

    let rendered = render_gaming_conf(&cfg);
    assert!(rendered.contains("UNICORE_GAME_MODE=1"));
    assert!(rendered.contains("UNICORE_SHOW_FPS=1"));
    assert_eq!(parse_gaming_conf(&rendered), cfg);
    assert_eq!(parse_gaming_conf("UNICORE_GAME_MODE=0\nUNICORE_SHOW_FPS=1\n"), GamingConfig {
        game_mode: false,
        show_fps: true,
    });

    let gov_file = root.join("scaling_governor");
    fs::write(&gov_file, "performance\n").unwrap();
    assert_eq!(read_cpu_governor_from(&gov_file), "performance");
    assert!(read_cpu_governor_from(&root.join("missing_gov")).contains("unavailable"));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn win7_era_dropdown_and_inspection_roundtrip_work() {
    for (idx, era) in [
        (0, WinEra::Win98),
        (1, WinEra::WinXp),
        (2, WinEra::Win7),
        (3, WinEra::Win10),
    ] {
        assert_eq!(win_era_to_index(Some(era)), idx);
        assert_eq!(win_era_from_index(idx), era);
    }

    let root = make_temp_dir("win7_inspect");
    let win7_exe = root.join("Win7Game.exe");
    // 32-bit Vista/Win7 binary with SubsystemVersion = 6.0 -> auto-detects as Win7
    fs::write(&win7_exe, build_minimal_pe(6, 0)).unwrap();

    let backend = CenterBackend::new(CenterConfig {
        state_dir: root.join("state"),
        apps_dir: root.join("apps"),
        desktop_dir: None,
        uniload_bin: PathBuf::from("/usr/bin/uniload"),
        host_arch: "x86_64".to_string(),
    });
    let inspected = backend.inspect_file(&win7_exe).unwrap();
    assert_eq!(inspected.win_era, Some(WinEra::Win7));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn dos16_display_name_and_scanner_support() {
    use uniload::inspector::AppPlatform;

    assert!(platform_display_name(AppPlatform::Dos16).contains("MS-DOS"));
    assert!(!supports_win_era(AppPlatform::Dos16));

    let root = make_temp_dir("dos_scanner");
    fs::write(root.join("DOOM.COM"), b"\xb8\x00\x4c\xcd\x21").unwrap();
    fs::write(root.join("START.BAT"), b"@echo off\r\n").unwrap();
    fs::write(root.join("README.TXT"), b"ignore").unwrap();

    let found = scan_launchable_files(std::slice::from_ref(&root));
    let names: Vec<String> = found.iter().map(|c| c.name.clone()).collect();
    assert_eq!(names, vec!["DOOM.COM", "START.BAT"]);
    assert_eq!(found[0].kind, "MS-DOS COM");
    assert_eq!(found[1].kind, "MS-DOS Batch");

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn store_tab_cli_and_catalog_view_work() {
    use unicore_center::store::load_store_views;

    let store_args: Vec<String> = vec!["--tab", "store"].into_iter().map(str::to_string).collect();
    let parsed_store = parse_args(&store_args).unwrap().unwrap();
    assert_eq!(parsed_store.mode, LaunchMode::FullCenter);
    assert_eq!(parsed_store.initial_tab, 4);

    let root = make_temp_dir("store_views");
    let state_dir = root.join("state");
    let apps_dir = root.join("apps");
    let bundled_exe = root.join("unicore_hello.exe");
    fs::write(&bundled_exe, build_minimal_pe(6, 1)).unwrap();

    let catalog_path = root.join("catalog.json");
    fs::write(
        &catalog_path,
        format!(
            r#"{{
  "version": 1,
  "apps": [
    {{
      "id": "demo-app",
      "name": "Demo App",
      "version": "1.0",
      "size_label": "Offline",
      "category": "utilities",
      "platform": "windows_pe",
      "win_era": "win10",
      "download_url": "",
      "sha256": "",
      "filename": "unicore_hello.exe",
      "archive_entry": "",
      "bundled_path": "{}",
      "summary_en": "English summary",
      "summary_ms": "Ringkasan Melayu"
    }}
  ]
}}"#,
            bundled_exe.display().to_string().replace('\\', "\\\\")
        ),
    )
    .unwrap();

    let backend = CenterBackend::new(CenterConfig {
        state_dir: state_dir.clone(),
        apps_dir,
        desktop_dir: None,
        uniload_bin: PathBuf::from("/usr/bin/uniload"),
        host_arch: "x86_64".to_string(),
    });

    let before_en = load_store_views(
        &backend.list_apps().unwrap(),
        &state_dir,
        "all",
        "",
        false,
        Some(&catalog_path),
    )
    .unwrap();
    assert_eq!(before_en.len(), 1);
    assert_eq!(before_en[0].summary, "English summary");
    assert!(!before_en[0].is_installed);

    let installed = backend
        .install_file(
            &bundled_exe,
            &InstallRequest {
                app_name: Some("Demo App".to_string()),
                win_era: Some(WinEra::Win10),
                permissions: PermissionProfile::default(),
                create_shortcut: false,
            },
        )
        .unwrap();

    let after_ms = load_store_views(
        &backend.list_apps().unwrap(),
        &state_dir,
        "all",
        "",
        true,
        Some(&catalog_path),
    )
    .unwrap();
    assert_eq!(after_ms[0].summary, "Ringkasan Melayu");
    assert!(after_ms[0].is_installed);
    assert_eq!(after_ms[0].installed_app_id, installed.app_id);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn store_grid_triples_and_icon_metadata_work() {
    use unicore_center::store::{chunk_into_triples, era_badge_name, store_icon_meta};
    use uniload::inspector::AppPlatform;

    assert!(chunk_into_triples::<i32>(&[]).is_empty());
    assert_eq!(chunk_into_triples(&[1]), vec![(Some(1), None, None)]);
    assert_eq!(
        chunk_into_triples(&[1, 2, 3, 4]),
        vec![
            (Some(1), Some(2), Some(3)),
            (Some(4), None, None),
        ]
    );
    assert_eq!(
        chunk_into_triples(&[1, 2, 3, 4, 5, 6, 7]).len(),
        3
    );

    assert_eq!(store_icon_meta("7zip", "7-Zip File Manager", "utilities").0, "7Z");
    assert_eq!(store_icon_meta("notepadpp", "Notepad++ Portable", "office").0, "N++");
    assert_eq!(store_icon_meta("sumatrapdf", "SumatraPDF Reader", "office").0, "PDF");
    assert_eq!(store_icon_meta("commander-keen-1", "Commander Keen 1", "retro").0, "CK1");
    assert_eq!(store_icon_meta("cave-story", "Cave Story (Doukutsu)", "retro").0, "CS");
    assert_eq!(store_icon_meta("openra-red-alert", "OpenRA: Red Alert", "retro").0, "RA");
    assert_eq!(store_icon_meta("custom-app", "Krita", "utilities").0, "KR");

    assert_eq!(era_badge_name(AppPlatform::Dos16, None), "MS-DOS");
    assert_eq!(era_badge_name(AppPlatform::WindowsPe, Some(WinEra::Win98)), "Win98");
    assert_eq!(era_badge_name(AppPlatform::WindowsPe, Some(WinEra::WinXp)), "WinXP");
    assert_eq!(era_badge_name(AppPlatform::WindowsPe, Some(WinEra::Win7)), "Win7");

    let retro_args: Vec<String> = vec!["--tab", "store:retro"].into_iter().map(str::to_string).collect();
    let parsed_retro = parse_args(&retro_args).unwrap().unwrap();
    assert_eq!(parsed_retro.initial_tab, 4);
    assert_eq!(parsed_retro.initial_store_category, "retro");
}

#[test]
fn phase20a_printers_and_nightlight_helpers_work() {
    // 1. CLI --tab settings:printers
    let printer_args: Vec<String> = vec!["--tab", "settings:printers"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let parsed = parse_args(&printer_args).unwrap().unwrap();
    assert_eq!(parsed.initial_tab, 3);
    assert_eq!(parsed.initial_settings_subtab, 4);

    // 2. Non-blocking printer overview when CUPS daemon is idle
    let start = std::time::Instant::now();
    let pr_overview = read_printers_overview();
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
    assert!(!pr_overview.service_active);
    assert!(!pr_overview.summary.is_empty());

    // 3. lpstat & lpinfo parsers
    let lpstat_p = "\
printer Office_Laser is idle.  enabled since Tue Nov 25 10:00:00 2026
printer USB_DeskJet disabled since Tue Nov 25 10:01:00 2026
";
    let lpstat_d = "system default destination: Office_Laser\n";
    let parsed_printers = parse_lpstat_printers(lpstat_p, lpstat_d);
    assert_eq!(parsed_printers.len(), 2);
    assert_eq!(parsed_printers[0].name, "Office_Laser");
    assert!(parsed_printers[0].is_default);
    assert_eq!(parsed_printers[1].name, "USB_DeskJet");
    assert!(!parsed_printers[1].is_default);

    let lpinfo_v = "\
file cups-brf:/
network ipp://192.168.1.50:631/ipp/print
direct usb://HP/LaserJet%20Pro?serial=ABC123
";
    let discovered = parse_lpinfo_devices(lpinfo_v);
    assert_eq!(discovered.len(), 2);
    assert_eq!(discovered[0].uri, "ipp://192.168.1.50:631/ipp/print");
    assert!(!discovered[0].suggested_name.is_empty());
    assert_eq!(discovered[1].uri, "usb://HP/LaserJet%20Pro?serial=ABC123");

    // 4. Printer input validators reject unsafe shell chars
    assert!(validate_printer_name("Office_Laser-1").is_ok());
    assert!(validate_printer_name("-bad").is_err());
    assert!(validate_printer_name("bad;rm -rf /").is_err());
    assert!(validate_printer_uri("ipp://192.168.1.50/ipp/print").is_ok());
    assert!(validate_printer_uri("usb://Brother/HL-L2350DW").is_ok());
    assert!(validate_printer_uri("file:///etc/passwd").is_err());
    assert!(validate_printer_uri("ipp://bad;id").is_err());

    // 5. Night Light config parsing, clamping & persistence
    assert_eq!(clamp_nightlight_temp(1200), 2700);
    assert_eq!(clamp_nightlight_temp(9000), 5500);
    let root = make_temp_dir("nightlight");
    let cfg = save_and_apply_nightlight(&root, true, 3600).unwrap();
    assert_eq!(
        cfg,
        NightLightConfig {
            enabled: true,
            temp_k: 3600
        }
    );
    let reloaded = load_nightlight_config(&root);
    assert_eq!(reloaded, cfg);
    let rendered = render_nightlight_conf(&cfg);
    assert_eq!(parse_nightlight_conf(&rendered), cfg);
    let _ = fs::remove_dir_all(&root);

    // 6. Bilingual EN/BM labels
    let en = UiLabels::for_lang(UiLang::En);
    let ms = UiLabels::for_lang(UiLang::Ms);
    assert_ne!(en.subtab_printers, ms.subtab_printers);
    assert_ne!(en.chk_night_light, ms.chk_night_light);
}

#[test]
fn phase20b_firewall_vpn_and_backup_helpers_work() {
    // 1. CLI --tab settings:security / firewall / vpn / backup
    for alias in ["settings:security", "settings:firewall", "settings:vpn", "settings:backup"] {
        let args = vec!["--tab".to_string(), alias.to_string()];
        let parsed = parse_args(&args).unwrap().unwrap();
        assert_eq!(parsed.initial_tab, 3);
        assert_eq!(parsed.initial_settings_subtab, 5);
    }

    // 2. Firewall status parser
    let fw_raw = "enabled=1\nactive=1\nprofile=public\nallow_ssh=1\nrules_count=9\n";
    let fw = parse_firewall_status(fw_raw);
    assert!(fw.enabled);
    assert!(fw.active);
    assert_eq!(fw.profile, "public");
    assert!(fw.allow_ssh);
    assert_eq!(fw.rules_count, 9);
    assert!(fw.summary.contains("Public"));

    let fw_off = parse_firewall_status("enabled=0\nactive=0\nprofile=home\nallow_ssh=0\nrules_count=0\n");
    assert!(!fw_off.enabled);
    assert!(fw_off.summary.contains("Disabled"));

    // 3. VPN nmcli list & import extension validation
    let nm_raw = "\
HomeNet:802-11-wireless:yes
MyOffice\\:OVPN:vpn:no
wg0:wireguard:yes
";
    let vpns = parse_nmcli_vpn_list(nm_raw);
    assert_eq!(vpns.len(), 2);
    assert_eq!(vpns[0].name, "wg0");
    assert_eq!(vpns[0].vpn_type, "WireGuard");
    assert!(vpns[0].active);
    assert_eq!(vpns[1].name, "MyOffice:OVPN");
    assert_eq!(vpns[1].vpn_type, "OpenVPN");
    assert!(!vpns[1].active);

    assert_eq!(detect_vpn_import_type("/data/home/wg0.conf").unwrap(), "wireguard");
    assert_eq!(detect_vpn_import_type("/media/usb/work.ovpn").unwrap(), "openvpn");
    assert!(detect_vpn_import_type("/data/home/file.txt").is_err());
    assert!(detect_vpn_import_type("relative.conf").is_err());

    // 4. Backup target parsing and path/passphrase guards
    let targets = parse_backup_targets(
        "/data/home/Backups\tLocal Backups (/data/home/Backups)\n/media/usb0\tRemovable (/media/usb0)\n",
    );
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[1].path, "/media/usb0");

    assert!(validate_backup_path("/data/home/Backups").is_ok());
    assert!(validate_backup_path("relative/dir").is_err());
    assert!(validate_backup_path("/tmp/../etc").is_err());
    assert!(validate_backup_path("/tmp/bad;rm").is_err());

    assert!(validate_backup_passphrase("short").is_err());
    assert!(validate_backup_passphrase("UniCoreBackup99!").is_ok());

    // 5. Bilingual Phase 20B labels
    let en = UiLabels::for_lang(UiLang::En);
    let ms = UiLabels::for_lang(UiLang::Ms);
    assert_ne!(en.subtab_security, ms.subtab_security);
    assert_ne!(en.hdr_firewall, ms.hdr_firewall);
    assert_ne!(en.btn_create_backup, ms.btn_create_backup);
    assert_ne!(en.btn_open_scanner, ms.btn_open_scanner);
}

#[test]
fn phase20c_office_store_catalog_and_antigravity_work() {
    use unicore_center::store::store_icon_meta;
    use uniload::catalog::{find_catalog_app, parse_catalog_json, validate_sha256_hex, EMBEDDED_CATALOG_JSON};

    // 1. Store monogram icons for Phase 20C catalog apps
    assert_eq!(store_icon_meta("antigravity", "Google Antigravity", "office").0, "AGY");
    assert_eq!(store_icon_meta("libreoffice", "LibreOffice Suite", "office").0, "LO");
    assert_eq!(store_icon_meta("vscodium", "VSCodium Editor", "office").0, "VSC");
    assert_eq!(store_icon_meta("keepassxc", "KeePassXC Portable", "utilities").0, "KPX");
    assert_eq!(store_icon_meta("vlc", "VLC Media Player", "utilities").0, "VLC");

    // 2. Embedded catalog includes Google Antigravity 2.35.2 + 32 Universal Store apps (Flatpak + Win32/Android/DOS)
    let catalog = parse_catalog_json(EMBEDDED_CATALOG_JSON).expect("embedded catalog valid");
    assert!(catalog.apps.len() >= 32);

    let agy = find_catalog_app(&catalog, "antigravity").expect("antigravity in catalog");
    assert_eq!(agy.version, "2.35.2");
    assert_eq!(agy.category, "dev");
    assert!(agy.download_url.starts_with("https://"));
    assert!(validate_sha256_hex(&agy.sha256).is_ok());

    for id in ["libreoffice", "vscodium", "keepassxc", "vlc"] {
        let entry = find_catalog_app(&catalog, id).expect("phase 20c app in catalog");
        assert!(entry.download_url.starts_with("https://"));
        assert!(validate_sha256_hex(&entry.sha256).is_ok());
    }
}

#[test]
fn phase20d_personalization_theme_wallpaper_and_input_work() {
    use unicore_center::settings::{
        clamp_pointer_speed, load_personalize_config, parse_personalize_conf,
        render_personalize_conf, save_and_apply_personalize, validate_kb_layout, validate_theme,
        validate_wallpaper_path, PersonalizeConfig,
    };

    // 1. CLI --tab settings:appearance / theme / wallpaper / input
    for alias in [
        "settings:appearance",
        "settings:theme",
        "settings:wallpaper",
        "settings:input",
    ] {
        let args = vec!["--tab".to_string(), alias.to_string()];
        let parsed = parse_args(&args).unwrap().unwrap();
        assert_eq!(parsed.initial_tab, 3);
        assert_eq!(parsed.initial_settings_subtab, 3);
    }

    // 2. Validators reject unsafe paths, unknown layouts/themes, and clamp speed
    assert_eq!(validate_theme("LIGHT").unwrap(), "light");
    assert!(validate_theme("neon").is_err());

    assert_eq!(
        validate_wallpaper_path("/usr/share/unicore/wallpaper-light.png").unwrap(),
        "/usr/share/unicore/wallpaper-light.png"
    );
    assert!(validate_wallpaper_path("relative.png").is_err());
    assert!(validate_wallpaper_path("/tmp/bad.sh").is_err());
    assert!(validate_wallpaper_path("/tmp/../etc/passwd.png").is_err());

    assert_eq!(validate_kb_layout("MY").unwrap(), "my");
    assert!(validate_kb_layout("xx").is_err());

    assert_eq!(clamp_pointer_speed(-2.0), -0.8);
    assert_eq!(clamp_pointer_speed(1.5), 0.8);
    assert_eq!(clamp_pointer_speed(0.34), 0.3);

    // 3. Config persistence & roundtrip parsing
    let root = make_temp_dir("personalize");
    assert_eq!(load_personalize_config(&root), PersonalizeConfig::default());

    let custom = PersonalizeConfig {
        theme: "light".to_string(),
        wallpaper: "/usr/share/unicore/wallpaper-light.png".to_string(),
        kb_layout: "my".to_string(),
        natural_scroll: false,
        tap_to_click: true,
        pointer_speed: 0.4,
        desktop_icons: false,
        taskbar_style: "floating".to_string(),
        desktop_layout: "macos".to_string(),
        accent_color: "purple".to_string(),
        high_contrast: true,
        large_text: true,
    };
    let saved = save_and_apply_personalize(&root, &custom).unwrap();
    assert_eq!(saved, custom);
    let reloaded = load_personalize_config(&root);
    assert_eq!(reloaded, custom);
    let rendered = render_personalize_conf(&custom);
    assert_eq!(parse_personalize_conf(&rendered), custom);
    let _ = fs::remove_dir_all(&root);

    // 4. Bilingual labels
    let en = UiLabels::for_lang(UiLang::En);
    let ms = UiLabels::for_lang(UiLang::Ms);
    assert_ne!(en.subtab_display, ms.subtab_display);
    assert_ne!(en.hdr_personalize, ms.hdr_personalize);
    assert_ne!(en.btn_apply_personalize, ms.btn_apply_personalize);
}

#[test]
fn phase23_24_25_power_users_devbox_ai_and_picker_work() {
    use unicore_center::dev_ai::{
        list_directory_entries, parse_ai_status, parse_devbox_status, recommend_ai_model_for_ram,
        validate_ai_model_name, validate_devbox_name,
    };
    use unicore_center::users_power::{
        parse_power_status, parse_smart_health, parse_users_list, validate_new_username,
    };

    // 1. CLI flags for Quick Settings, Login, Dev & AI, Power & Users
    let q_parsed = parse_args(&["--quick-settings".to_string()]).unwrap().unwrap();
    assert_eq!(q_parsed.mode, LaunchMode::QuickSettings);

    let l_parsed = parse_args(&["--login".to_string()]).unwrap().unwrap();
    assert_eq!(l_parsed.mode, LaunchMode::LoginGreeter);

    for tab in ["dev", "ai", "devbox"] {
        let p = parse_args(&["--tab".to_string(), tab.to_string()]).unwrap().unwrap();
        assert_eq!(p.initial_tab, 5);
    }
    for sub in ["settings:power", "settings:battery", "settings:users", "settings:smart"] {
        let p = parse_args(&["--tab".to_string(), sub.to_string()]).unwrap().unwrap();
        assert_eq!(p.initial_tab, 3);
        assert_eq!(p.initial_settings_subtab, 6);
    }

    // 2. Phase 23: Multi-user, Power & S.M.A.R.T. parsers & validators
    assert_eq!(validate_new_username("ali_01").unwrap(), "ali_01");
    assert!(validate_new_username("root").is_err());
    assert!(validate_new_username("A_bad").is_err());

    let users = parse_users_list("unicore\t1000\tadmin\t0\nahmad\t1001\tstandard\t1\n");
    assert_eq!(users.len(), 2);
    assert!(!users[0].has_password);
    assert_eq!(users[1].username, "ahmad");
    assert!(users[1].has_password);

    let pwr = parse_power_status(
        "profile=performance\nscreen_min=15\nlock_min=20\nsuspend_bat_min=30\nbattery=82%\non_ac=0\nswayidle_running=1\n",
    );
    assert_eq!(pwr.profile, "performance");
    assert_eq!(pwr.screen_min, 15);
    assert_eq!(pwr.battery, "82%");
    assert!(!pwr.on_ac);
    assert!(pwr.swayidle_running);

    let smart = parse_smart_health(
        "device=/dev/nvme0n1\nmodel=Samsung SSD\nhealth=PASSED\ntemp_c=38C\npower_on_hours=120\n",
    );
    assert_eq!(smart.device, "/dev/nvme0n1");
    assert_eq!(smart.health, "PASSED");
    assert_eq!(smart.temp_c, "38C");

    // 3. Phase 24: DevBox & Local AI helpers
    assert!(recommend_ai_model_for_ram(512).contains("qwen2.5:0.5b"));
    assert_eq!(recommend_ai_model_for_ram(4096), "llama3.2:1b");
    assert_eq!(recommend_ai_model_for_ram(8192), "qwen2.5-coder:3b");
    assert_eq!(recommend_ai_model_for_ram(16384), "qwen2.5-coder:7b");

    assert!(validate_devbox_name("py-lab_1").is_ok());
    assert!(validate_devbox_name("../bad").is_err());
    assert!(validate_ai_model_name("llama3.2:1b").is_ok());
    assert!(validate_ai_model_name("bad;rm").is_err());

    let ai = parse_ai_status(
        "installed=1\nrunning=1\nendpoint=http://127.0.0.1:11434\nrecommended_model=llama3.2:1b\nmodels=qwen2.5:0.5b,llama3.2:1b\n",
    );
    assert!(ai.installed && ai.running);
    assert_eq!(ai.models, vec!["qwen2.5:0.5b", "llama3.2:1b"]);

    let dev = parse_devbox_status(
        "git=git version 2.45\npython=Python 3.12\npodman=podman 5.1\nprojects_dir=/data/dev/projects\ncontainers=py-lab|python|Up\n",
    );
    assert_eq!(dev.containers.len(), 1);
    assert!(dev.podman_version.contains("podman"));

    // 4. Built-in graphical File Picker directory listing
    let root = make_temp_dir("file_picker");
    fs::create_dir_all(root.join("alpha_dir")).unwrap();
    fs::write(root.join("demo.conf"), b"test").unwrap();
    let (_canon, entries) = list_directory_entries(&root).unwrap();
    assert!(entries.iter().any(|e| e.is_dir && e.name.contains("alpha_dir")));
    assert!(entries.iter().any(|e| !e.is_dir && e.name == "demo.conf"));
    let _ = fs::remove_dir_all(&root);

    // 5. Bilingual v0.2.0 UI labels
    let en = UiLabels::for_lang(UiLang::En);
    let ms = UiLabels::for_lang(UiLang::Ms);
    assert_ne!(en.subtab_power_users, ms.subtab_power_users);
    assert_ne!(en.hdr_local_ai, ms.hdr_local_ai);
    assert_ne!(en.hdr_users, ms.hdr_users);
}

#[test]
fn v030_secureboot_browsers_auto_ota_and_win32_diag_work() {
    use unicore_center::settings::{parse_secureboot_status, SecureBootOverview};
    use unicore_center::store::{parse_browser_status, BrowserOverview};
    use uniload::updater::{official_manifest_url, save_data_update_auto_check, UpdateConfig};
    use uniload::win32_diag::diagnose_win32_log;

    let sb = parse_secureboot_status(
        "uefi=1\nsecure_boot=enabled\nmok_cert=1\nmok_state=enrolled\nshim_bundled=1\n",
    );
    assert!(sb.uefi);
    assert_eq!(sb.secure_boot, "enabled");
    assert_eq!(sb.mok_state, "enrolled");
    assert!(sb.shim_bundled);
    assert!(sb.summary().contains("enabled"));
    assert_eq!(SecureBootOverview::default().secure_boot, "legacy_bios");

    let br = parse_browser_status(
        "default_browser=firefox\nepiphany_installed=1\nfirefox_installed=1\nchromium_installed=0\n",
    );
    assert_eq!(br.default_browser, "firefox");
    assert!(br.firefox_installed);
    assert!(!br.chromium_installed);
    assert_eq!(BrowserOverview::default().default_browser, "epiphany");

    assert!(official_manifest_url("x86_64").ends_with("unicore-manifest-x86_64.json"));
    assert!(official_manifest_url("aarch64").ends_with("unicore-manifest-aarch64.json"));
    let root = make_temp_dir("v030_ota");
    let conf = root.join("update.conf");
    save_data_update_auto_check(&conf, false).unwrap();
    let cfg = UpdateConfig::parse(&fs::read_to_string(&conf).unwrap());
    assert!(!cfg.auto_check);
    save_data_update_auto_check(&conf, true).unwrap();
    let cfg2 = UpdateConfig::parse(&fs::read_to_string(&conf).unwrap());
    assert!(cfg2.auto_check);
    let _ = fs::remove_dir_all(&root);

    let diag = diagnose_win32_log("err:module:import_dll Library vcruntime140.dll not found");
    assert!(diag.issue_detected);
    assert!(diag.recommended_verbs.contains(&"vcrun2022".to_string()));
}

#[test]
fn v040_dualboot_taskbar_flatpak_proton_and_ai_copilot_work() {
    use unicore_center::dev_ai::parse_ai_status;
    use unicore_center::installer::parse_install_disks;
    use unicore_center::settings::validate_taskbar_style;
    use unicore_center::store::{
        load_store_views_for_arch, parse_flatpak_status, parse_proton_status,
    };

    // 1. CLI --ai drawer & expanded --tab store:<category>
    let ai_parsed = parse_args(&["--ai".to_string()]).unwrap().unwrap();
    assert_eq!(ai_parsed.mode, LaunchMode::AiAssistant);
    for cat in ["gaming", "creative", "social", "dev", "office", "android", "retro", "utilities"] {
        let p = parse_args(&["--tab".to_string(), format!("store:{cat}")])
            .unwrap()
            .unwrap();
        assert_eq!(p.initial_tab, 4);
        assert_eq!(p.initial_store_category, cat);
    }

    // 2. 9-column Safe Dual-Boot disk TSV parsing
    let tsv = "\
/dev/nvme0n1\tnvme0n1\t512.0 GB\tWD Black SN850\t1\teligible\t48.5 GB\t1\t1
/dev/sda\tsda\t256.0 GB\tKingston SSD\t1\teligible\t0.0 GB\t0\t1
";
    let disks = parse_install_disks(tsv);
    assert_eq!(disks.len(), 2);
    assert!(disks[0].dualboot_eligible);
    assert!(disks[0].windows_detected);
    assert_eq!(disks[0].free_gb, "48.5 GB");
    assert!(!disks[1].dualboot_eligible);
    assert!(disks[1].windows_detected);

    // 3. Taskbar style validator
    assert_eq!(validate_taskbar_style("docked").unwrap(), "docked");
    assert_eq!(validate_taskbar_style("FLOATING").unwrap(), "floating");
    assert!(validate_taskbar_style("invalid").is_err());

    // 4. Flatpak & GE-Proton status parsers + arch-filtered 32-app Universal Store
    let fp = parse_flatpak_status("installed=1\nflathub_ready=1\napp_count=4\nflatpak_dir=/data/flatpak\n");
    assert!(fp.installed && fp.flathub_ready);
    assert_eq!(fp.app_count, 4);

    let pr = parse_proton_status(
        "dir=/data/gaming/proton-ge\ninstalled_count=2\nversions=GE-Proton9-25,GE-Proton9-26\nsteam_linked=1\n",
    );
    assert_eq!(pr.installed_count, 2);
    assert!(pr.steam_linked);
    assert_eq!(pr.versions.len(), 2);

    let root = make_temp_dir("v040_store_arch");
    let x64_gaming =
        load_store_views_for_arch(&[], &root, "gaming", "", false, "x86_64", None).unwrap();
    assert_eq!(x64_gaming.len(), 5);
    assert!(x64_gaming.iter().any(|a| a.id == "steam" && a.is_flatpak));
    let arm_gaming =
        load_store_views_for_arch(&[], &root, "gaming", "", false, "aarch64", None).unwrap();
    assert_eq!(arm_gaming.len(), 2);
    assert!(!arm_gaming.iter().any(|a| a.id == "steam"));
    assert!(arm_gaming.iter().any(|a| a.id == "prism-launcher"));
    let _ = fs::remove_dir_all(&root);

    // 5. Multi-Provider AI Copilot status parser
    let ai = parse_ai_status(
        "installed=0\nrunning=0\nendpoint=http://127.0.0.1:11434\nprovider=gemini\nactive_model=gemini-2.5-flash\nbase_url=https://api.openai.com/v1\napi_key_set=1\nram_mb=1024\n",
    );
    assert_eq!(ai.provider, "gemini");
    assert_eq!(ai.active_model, "gemini-2.5-flash");
    assert!(ai.api_key_set);
    assert_eq!(ai.ram_mb, 1024);
}

#[test]
fn v050_capture_dnd_shares_audio_profiles_a11y_and_bilingual_work() {
    use unicore_center::capture_a11y::{
        parse_a11y_status, parse_capture_status, parse_notify_status, parse_shares_tsv,
        validate_share_id, validate_share_uri,
    };
    use unicore_center::settings::parse_pactl_card_profiles;
    use unicore_center::store::store_icon_meta;

    // 1. CLI --capture dialog flag
    let cap_parsed = parse_args(&["--capture".to_string()]).unwrap().unwrap();
    assert_eq!(cap_parsed.mode, LaunchMode::CaptureTool);

    // 2. Capture & DND status parsers
    let cap = parse_capture_status(
        "recording=1\npid=4421\nfile=/data/home/Videos/Recordings/rec.mp4\nshots_dir=/data/home/Pictures/Screenshots\nvideos_dir=/data/home/Videos/Recordings\nwf_recorder=1\n",
    );
    assert!(cap.recording && cap.wf_recorder);
    assert_eq!(cap.pid, "4421");
    assert!(cap.active_file.ends_with("rec.mp4"));

    let notif = parse_notify_status("mako_running=1\ndnd=1\n");
    assert!(notif.mako_running && notif.dnd);

    // 3. Network Shares URI validation & TSV parsing
    assert!(validate_share_uri("smb://192.168.1.20/Public").is_ok());
    assert!(validate_share_uri("sftp://nas.local/data").is_ok());
    assert!(validate_share_uri("davs://cloud.example.com/remote.php/dav").is_ok());
    assert!(validate_share_uri("file:///etc/passwd").is_err());
    assert!(validate_share_uri("smb://host/share;rm -rf /").is_err());
    assert!(validate_share_id("home-nas").is_ok());
    assert!(validate_share_id("../bad").is_err());

    let shares = parse_shares_tsv(
        "home-nas\tHome NAS\tsmb://192.168.1.20/Media\tali\t1\t/run/user/1000/gvfs/smb\n",
    );
    assert_eq!(shares.len(), 1);
    assert!(shares[0].mounted);
    assert_eq!(shares[0].label, "Home NAS");

    // 4. PipeWire Bluetooth & ALSA Audio Card profile parser
    let cards_raw = "\
Card #42
\tName: bluez_card.AA_BB_CC_11_22_33
\tProfiles:
\t\ta2dp-sink: High Fidelity Playback (A2DP Sink) (sinks: 1, sources: 0, priority: 40, available: yes)
\t\theadset-head-unit: Headset Head Unit (HSP/HFP) (sinks: 1, sources: 1, priority: 30, available: yes)
\t\toff: Off (sinks: 0, sources: 0, priority: 0, available: yes)
\tActive Profile: a2dp-sink
";
    let profiles = parse_pactl_card_profiles(cards_raw);
    assert_eq!(profiles.len(), 2);
    assert_eq!(profiles[0].profile_name, "a2dp-sink");
    assert!(profiles[0].active);
    assert_eq!(profiles[1].profile_name, "headset-head-unit");
    assert!(!profiles[1].active);

    // 5. Accessibility status parser & LocalSend Store monogram + full BM/EN labels
    let a11y = parse_a11y_status("osk_available=1\nosk_running=1\ntts_available=1\n");
    assert!(a11y.osk_available && a11y.osk_running && a11y.tts_available);
    assert_eq!(store_icon_meta("localsend", "LocalSend", "social").0, "LS");

    let en = UiLabels::for_lang(UiLang::En);
    let ms = UiLabels::for_lang(UiLang::Ms);
    assert_ne!(en.title_capture, ms.title_capture);
    assert_ne!(en.hdr_net_shares, ms.hdr_net_shares);
    assert_ne!(en.hdr_accessibility, ms.hdr_accessibility);
    assert_ne!(en.store_cat_gaming, ms.store_cat_gaming);
    assert_ne!(en.tile_dnd, ms.tile_dnd);
}

#[test]
fn v060_start_menu_emoji_layouts_accents_friendly_header_and_oobe_packs_work() {
    use unicore_center::settings::{
        accent_color_from_index, accent_color_to_index, desktop_layout_from_index,
        desktop_layout_to_index, validate_accent_color, validate_desktop_layout, PersonalizeConfig,
    };
    use unicore_center::start_emoji::{
        builtin_pinned_apps, filter_emojis, filter_start_menu_apps, friendly_hw_summary,
        oobe_bundle_catalog_ids, parse_desktop_file,
    };

    // 1. CLI flags for Start Menu & Emoji Picker
    let sm = parse_args(&["--start-menu".to_string()]).unwrap().unwrap();
    assert_eq!(sm.mode, LaunchMode::StartMenu);
    let em = parse_args(&["--emoji".to_string()]).unwrap().unwrap();
    assert_eq!(em.mode, LaunchMode::EmojiPicker);

    // 2. Zorin-style Desktop Layouts (default win11) & 5 Accent Colors
    let def = PersonalizeConfig::default();
    assert_eq!(def.desktop_layout, "win11");
    assert_eq!(def.taskbar_style, "floating");
    assert_eq!(def.accent_color, "blue");
    for (idx, code) in ["win11", "win10", "macos"].iter().enumerate() {
        assert_eq!(validate_desktop_layout(code).unwrap(), *code);
        assert_eq!(desktop_layout_to_index(code), idx as i32);
        assert_eq!(desktop_layout_from_index(idx as i32), *code);
    }
    assert!(validate_desktop_layout("invalid").is_err());

    for (idx, code) in ["blue", "purple", "green", "amber", "rose"].iter().enumerate() {
        assert_eq!(validate_accent_color(code).unwrap(), *code);
        assert_eq!(accent_color_to_index(code), idx as i32);
        assert_eq!(accent_color_from_index(idx as i32), *code);
    }
    assert!(validate_accent_color("neon").is_err());

    // 3. Start Menu pinned apps, .desktop parser & search filter
    let pinned_en = builtin_pinned_apps(UiLang::En);
    let pinned_ms = builtin_pinned_apps(UiLang::Ms);
    assert_eq!(pinned_en.len(), 8);
    assert_ne!(pinned_en[0].name, pinned_ms[0].name);

    let desktop_sample = "[Desktop Entry]\nType=Application\nName=GIMP Studio\nComment=Edit images\nExec=gimp %U\nCategories=Graphics;AudioVideo;\n";
    let parsed = parse_desktop_file(desktop_sample, "gimp").unwrap();
    assert_eq!(parsed.name, "GIMP Studio");
    assert_eq!(parsed.exec_cmd, "gimp");
    assert_eq!(parsed.category, "media");
    assert!(parse_desktop_file("[Desktop Entry]\nName=Hidden\nExec=foo\nNoDisplay=true\n", "h").is_none());

    let mut all = pinned_en.clone();
    all.push(parsed);
    assert_eq!(filter_start_menu_apps(&all, "all", "gimp").len(), 1);
    assert!(filter_start_menu_apps(&all, "media", "").len() >= 2);

    // 4. Emoji Picker search (EN + BM keywords)
    let my_hits = filter_emojis("all", "malaysia");
    assert!(my_hits.iter().any(|e| e.symbol == "🇲🇾"));
    let fire_hits = filter_emojis("symbols", "mantap");
    assert!(fire_hits.iter().any(|e| e.symbol == "🔥"));

    // 5. Friendly non-technical header summary (no legacy_mid/pixman jargon)
    let hdr_en = friendly_hw_summary("legacy_mid", 2997, UiLang::En);
    let hdr_ms = friendly_hw_summary("modern", 8192, UiLang::Ms);
    assert!(hdr_en.contains("Balanced Mode") && !hdr_en.contains("pixman"));
    assert!(hdr_ms.contains("Mod Prestasi") && hdr_ms.contains("8.0 GB RAM"));

    // 6. OOBE 1-click starter bundles
    assert_eq!(oobe_bundle_catalog_ids(true, false, true), vec!["libreoffice", "firefox", "steam", "heroic"]);
    assert!(oobe_bundle_catalog_ids(false, false, false).is_empty());

    // 7. Bilingual v0.6.0 labels
    let en = UiLabels::for_lang(UiLang::En);
    let ms = UiLabels::for_lang(UiLang::Ms);
    assert_ne!(en.title_start_menu, ms.title_start_menu);
    assert_ne!(en.lbl_desktop_layout, ms.lbl_desktop_layout);
    assert_ne!(en.oobe_pack_office, ms.oobe_pack_office);
}

#[test]
fn v070_classic_98_xp_presets_cli_importer_and_bilingual_work() {
    use std::collections::BTreeMap;
    use unicore_center::retro::{
        apply_retro_profile_to_env, parse_retro_import_output, prepare_retro_source,
        read_retro_profile_from_env, virtual_desktop_from_index, virtual_desktop_to_index,
        RetroGameProfile, RetroPreset,
    };
    use unicore_center::start_emoji::builtin_pinned_apps;

    // 1. CLI flags for --retro-game, --repair, and --retro-pick
    let rg = parse_args(&["--retro-game".to_string()]).unwrap().unwrap();
    assert_eq!(rg.mode, LaunchMode::RetroWizard(None));

    let rg_iso = parse_args(&["--retro-game".to_string(), "/tmp/RedAlert2.iso".to_string()])
        .unwrap()
        .unwrap();
    assert_eq!(
        rg_iso.mode,
        LaunchMode::RetroWizard(Some(PathBuf::from("/tmp/RedAlert2.iso")))
    );

    let rep = parse_args(&["--repair".to_string(), "ra2-app".to_string()])
        .unwrap()
        .unwrap();
    assert_eq!(rep.mode, LaunchMode::RepairDialog("ra2-app".to_string()));

    let pick = parse_args(&["--retro-pick".to_string(), "ra2-app".to_string()])
        .unwrap()
        .unwrap();
    assert_eq!(
        pick.mode,
        LaunchMode::RetroTargetPicker("ra2-app".to_string())
    );

    // 2. RetroPreset & Virtual Desktop index/parse roundtrips
    for (idx, preset, code) in [
        (0, RetroPreset::Auto, "auto"),
        (1, RetroPreset::Classic2d, "classic_2d"),
        (2, RetroPreset::Classic3d, "classic_3d"),
        (3, RetroPreset::DosWin16, "dos_win16"),
    ] {
        assert_eq!(preset.as_str(), code);
        assert_eq!(RetroPreset::parse(code), preset);
        assert_eq!(preset.to_index(), idx);
        assert_eq!(RetroPreset::from_index(idx), preset);
    }
    assert_eq!(virtual_desktop_to_index("off"), 0);
    assert_eq!(virtual_desktop_to_index("800x600"), 1);
    assert_eq!(virtual_desktop_to_index("1024x768"), 2);
    assert_eq!(virtual_desktop_from_index(1), "800x600");
    assert_eq!(virtual_desktop_from_index(2), "1024x768");

    // 3. Env persistence roundtrip for Classic 2D (cnc-ddraw + 60 FPS + 1 Core + CD ISO)
    let mut env = BTreeMap::new();
    let prof_2d = RetroGameProfile {
        preset: RetroPreset::Classic2d,
        fps_cap_60: true,
        single_core_cpu: true,
        virtual_desktop: "off".to_string(),
        cd_iso_path: "/data/home/Games/RA2.iso".to_string(),
    };
    let era_2d = apply_retro_profile_to_env(&mut env, &prof_2d);
    assert_eq!(era_2d, Some(WinEra::Win98));
    assert_eq!(env.get("UNICORE_RETRO_PRESET").map(String::as_str), Some("classic_2d"));
    assert_eq!(
        env.get("UNILOAD_VK_BACKEND").map(String::as_str),
        Some("ddraw-compat+wined3d")
    );
    assert_eq!(env.get("UNILOAD_VIRTUAL_DESKTOP").map(String::as_str), Some("off"));
    assert_eq!(env.get("UNICORE_FPS_CAP_60").map(String::as_str), Some("1"));
    assert_eq!(env.get("UNICORE_SINGLE_CORE").map(String::as_str), Some("1"));
    assert_eq!(
        env.get("UNICORE_CD_ISO").map(String::as_str),
        Some("/data/home/Games/RA2.iso")
    );
    assert_eq!(read_retro_profile_from_env(&env, era_2d), prof_2d);

    // Classic 3D (DXVK D3D8/D3D9 + Safe 1024x768 window)
    let prof_3d = RetroGameProfile {
        preset: RetroPreset::Classic3d,
        fps_cap_60: false,
        single_core_cpu: false,
        virtual_desktop: "1024x768".to_string(),
        cd_iso_path: String::new(),
    };
    let era_3d = apply_retro_profile_to_env(&mut env, &prof_3d);
    assert_eq!(era_3d, Some(WinEra::WinXp));
    assert_eq!(
        env.get("UNILOAD_VK_BACKEND").map(String::as_str),
        Some("dxvk-d3d8-d3d9")
    );
    assert_eq!(
        env.get("UNILOAD_VIRTUAL_DESKTOP").map(String::as_str),
        Some("1024x768")
    );
    assert!(!env.contains_key("UNICORE_FPS_CAP_60"));
    assert!(!env.contains_key("UNICORE_SINGLE_CORE"));
    assert!(!env.contains_key("UNICORE_CD_ISO"));
    assert_eq!(read_retro_profile_from_env(&env, era_3d), prof_3d);

    // 4. Retro media output parser & portable game folder scanner
    let parsed = parse_retro_import_output(
        "game_dir=/data/home/Games/Diablo2\nmain_exe=/data/home/Games/Diablo2/Diablo II.exe\nname=Diablo2\niso_path=/data/home/Diablo2.iso\n",
    );
    assert_eq!(parsed.name, "Diablo2");
    assert!(parsed.main_exe.ends_with("Diablo II.exe"));
    assert_eq!(parsed.iso_path, "/data/home/Diablo2.iso");

    let root = make_temp_dir("v070_portable_folder");
    fs::write(root.join("unins000.exe"), vec![0u8; 4096]).unwrap();
    fs::write(root.join("StarCraft.exe"), vec![0u8; 2048]).unwrap();
    let prep = prepare_retro_source(&root.display().to_string(), "StarCraft Brood War").unwrap();
    assert_eq!(prep.name, "StarCraft Brood War");
    assert!(prep.main_exe.ends_with("StarCraft.exe"));
    let _ = fs::remove_dir_all(&root);

    // 5. Start Menu pinned tile #8 opens Classic 98/XP Game Launcher & bilingual EN/BM labels
    let pinned_en = builtin_pinned_apps(UiLang::En);
    let pinned_ms = builtin_pinned_apps(UiLang::Ms);
    assert_eq!(pinned_en[7].id, "unicore-retro");
    assert_eq!(pinned_en[7].exec_cmd, "/usr/bin/unicore-center --retro-game");
    assert_ne!(pinned_en[7].name, pinned_ms[7].name);

    let en = UiLabels::for_lang(UiLang::En);
    let ms = UiLabels::for_lang(UiLang::Ms);
    assert_ne!(en.title_retro_wizard, ms.title_retro_wizard);
    assert_ne!(en.hdr_retro_tuning, ms.hdr_retro_tuning);
    assert_ne!(en.chk_fps_cap, ms.chk_fps_cap);
    assert_ne!(en.chk_single_core, ms.chk_single_core);
    assert_ne!(en.title_repair, ms.title_repair);
    assert_ne!(en.title_retro_pick, ms.title_retro_pick);
    assert_ne!(en.btn_retro_install_play, ms.btn_retro_install_play);
    assert_ne!(en.btn_repair_retry, ms.btn_repair_retry);
}

#[test]
fn v080_ecosystem_phone_cloud_migrate_drivers_and_bilingual_work() {
    use unicore_center::ecosystem::{
        build_migrate_flags_csv, cloud_provider_from_index, gpu_mode_from_index, gpu_mode_to_index,
        match_windows_programs_to_store, parse_cloud_status, parse_hw_status,
        parse_migrate_inspect, parse_migrate_scan_tsv, parse_phone_status,
        validate_adb_wifi_target, validate_cloud_name, validate_gpu_mode, validate_phone_id,
        validate_win_partition, validate_win_username,
    };
    use unicore_center::start_emoji::builtin_ecosystem_apps;

    // 1. CLI flags for --phone, --cloud, --migrate, --drivers & --tab settings:ecosystem
    assert_eq!(
        parse_args(&["--phone".to_string()]).unwrap().unwrap().mode,
        LaunchMode::EcosystemHub(0)
    );
    assert_eq!(
        parse_args(&["--cloud".to_string()]).unwrap().unwrap().mode,
        LaunchMode::EcosystemHub(1)
    );
    assert_eq!(
        parse_args(&["--migrate".to_string()]).unwrap().unwrap().mode,
        LaunchMode::EcosystemHub(2)
    );
    assert_eq!(
        parse_args(&["--drivers".to_string()]).unwrap().unwrap().mode,
        LaunchMode::EcosystemHub(3)
    );
    for alias in ["settings:ecosystem", "settings:phone", "settings:cloud", "settings:migrate", "settings:drivers"] {
        let p = parse_args(&["--tab".to_string(), alias.to_string()])
            .unwrap()
            .unwrap();
        assert_eq!(p.initial_tab, 3);
        assert_eq!(p.initial_settings_subtab, 7);
    }

    // 2. Phone Link (KDE Connect + scrcpy + ADB Wi-Fi)
    assert!(validate_phone_id("abc123_phone:01").is_ok());
    assert!(validate_phone_id("bad;rm -rf /").is_err());
    assert!(validate_adb_wifi_target("192.168.1.45:5555").is_ok());
    assert!(validate_adb_wifi_target("999.1.1.1:5555").is_err());
    assert!(validate_adb_wifi_target("192.168.1.45:80").is_err());

    let ph = parse_phone_status(
        "kdeconnect_installed=1\nkdeconnect_running=1\nkdeconnect_autostart=1\nscrcpy_installed=1\nadb_installed=1\nkde_devices=dev1|Pixel 8|1|1;dev2|Galaxy S23|0|1\nadb_devices=R5CR12345,192.168.1.45:5555\n",
    );
    assert!(ph.kdeconnect_installed && ph.kdeconnect_running && ph.kdeconnect_autostart && ph.scrcpy_installed);
    assert_eq!(ph.kde_devices.len(), 2);
    assert_eq!(ph.kde_devices[0].name, "Pixel 8");
    assert!(ph.kde_devices[0].paired && ph.kde_devices[0].reachable);
    assert!(!ph.kde_devices[1].paired);
    assert_eq!(ph.adb_devices.len(), 2);

    // 3. Cloud Drives (rclone VFS)
    assert!(validate_cloud_name("GoogleDrive_1").is_ok());
    assert!(validate_cloud_name("../bad").is_err());
    assert_eq!(cloud_provider_from_index(0), "gdrive");
    assert_eq!(cloud_provider_from_index(1), "onedrive");
    assert_eq!(cloud_provider_from_index(4), "nextcloud");

    let cl = parse_cloud_status(
        "rclone_installed=1\nmount_base=/home/unicore/Cloud\nremotes=GoogleDrive|drive|1|1|/home/unicore/Cloud/GoogleDrive;OneDrive|onedrive|0|0|/home/unicore/Cloud/OneDrive\n",
    );
    assert!(cl.rclone_installed);
    assert_eq!(cl.remotes.len(), 2);
    assert!(cl.remotes[0].mounted && cl.remotes[0].autostart);
    assert!(!cl.remotes[1].mounted);

    // 4. Windows Migration (RO NTFS + Program Files -> UniCore Store matcher)
    assert!(validate_win_partition("/dev/nvme0n1p3").is_ok());
    assert!(validate_win_partition("/etc/passwd").is_err());
    assert!(validate_win_username("Raja Zahidi").is_ok());
    assert!(validate_win_username("../root").is_err());

    let parts = parse_migrate_scan_tsv("/dev/nvme0n1p3\tWindows11\t238.5G\tRaja,Guest\t42000\n");
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].device, "/dev/nvme0n1p3");
    assert_eq!(parts[0].users, vec!["Raja", "Guest"]);
    assert_eq!(parts[0].free_data_mb, 42000);

    let insp = parse_migrate_inspect(
        "device=/dev/nvme0n1p3\nusers=Raja\nfree_data_mb=42000\nfonts_count=180\nbookmarks_found=chrome,edge\nprogram_dirs=Steam|VideoLAN|7-Zip|Microsoft Office|Discord|Visual Studio Code\n",
    );
    assert_eq!(insp.fonts_count, 180);
    assert_eq!(insp.bookmarks_found, vec!["chrome", "edge"]);
    let store_ids: Vec<&str> = insp.store_matches.iter().map(|m| m.store_id.as_str()).collect();
    assert!(store_ids.contains(&"steam"));
    assert!(store_ids.contains(&"vlc"));
    assert!(store_ids.contains(&"7zip"));
    assert!(store_ids.contains(&"libreoffice"));
    assert!(store_ids.contains(&"vesktop"));
    assert!(store_ids.contains(&"vscodium"));
    assert_eq!(
        match_windows_programs_to_store(&["Steam".to_string(), "Steam2".to_string()]).len(),
        1
    );
    assert_eq!(
        build_migrate_flags_csv(true, true, false, false, false, true, true, true),
        "docs,pics,saves,bookmarks,fonts"
    );

    // 5. Hardware & Driver Manager + LVFS Firmware
    assert_eq!(validate_gpu_mode("SAFE").unwrap(), "safe");
    assert!(validate_gpu_mode("invalid").is_err());
    assert_eq!(gpu_mode_to_index("safe"), 2);
    assert_eq!(gpu_mode_from_index(2), "safe");

    let hw = parse_hw_status(
        "gpu_model=AMD Radeon RX 6600\ngpu_driver=amdgpu\nvulkan_ready=1\nvulkan_device=AMD RADV NAVI23\nwifi_model=Intel AX200\nwifi_driver=iwlwifi\naudio_model=HD Audio\nusb_count=5\nusb_summary=Logitech Mouse; Xbox Controller\nmissing_firmware=none\ngpu_mode=auto\nwifi_powersave=off\nfwupd_available=1\n",
    );
    assert!(hw.vulkan_ready && hw.fwupd_available);
    assert_eq!(hw.gpu_driver, "amdgpu");
    assert_eq!(hw.wifi_powersave, "off");
    assert_eq!(hw.usb_count, 5);

    // 6. Start Menu Ecosystem apps + Bilingual v0.8.0 labels
    let eco_en = builtin_ecosystem_apps(UiLang::En);
    let eco_ms = builtin_ecosystem_apps(UiLang::Ms);
    assert_eq!(eco_en.len(), 4);
    assert_ne!(eco_en[0].name, eco_ms[0].name);
    assert_eq!(eco_en[0].exec_cmd, "/usr/bin/unicore-center --phone");

    let en = UiLabels::for_lang(UiLang::En);
    let ms = UiLabels::for_lang(UiLang::Ms);
    assert_ne!(en.subtab_ecosystem, ms.subtab_ecosystem);
    assert_ne!(en.title_ecosystem, ms.title_ecosystem);
    assert_ne!(en.eco_tab_phone, ms.eco_tab_phone);
    assert_ne!(en.eco_tab_cloud, ms.eco_tab_cloud);
    assert_ne!(en.eco_tab_migrate, ms.eco_tab_migrate);
    assert_ne!(en.eco_tab_drivers, ms.eco_tab_drivers);
}

#[test]
fn v090_unified_gamehub_gamepad_and_cloud_saves_work() {
    use unicore_center::gamehub::{
        discover_unified_games, filter_games, format_playtime, load_favorites,
        mapper_profile_from_index, mapper_profile_to_index, parse_gamepad_status,
        parse_gamesave_status, parse_playtime_tsv, parse_steam_acf, toggle_favorite,
    };
    use unicore_center::start_emoji::builtin_gamehub_apps;

    // 1. CLI flags --games, --gamepad, --saves
    assert_eq!(
        parse_args(&["--games".to_string()]).unwrap().unwrap().mode,
        LaunchMode::GameHub(0)
    );
    assert_eq!(
        parse_args(&["--gamepad".to_string()]).unwrap().unwrap().mode,
        LaunchMode::GameHub(1)
    );
    assert_eq!(
        parse_args(&["--saves".to_string()]).unwrap().unwrap().mode,
        LaunchMode::GameHub(2)
    );

    // 2. Steam ACF parser (skips Proton / Steamworks redistributables)
    let acf_ok = "\"AppState\"\n{\n\t\"appid\"\t\t\"220\"\n\t\"name\"\t\t\"Half-Life 2\"\n}\n";
    assert_eq!(
        parse_steam_acf(acf_ok),
        Some(("220".to_string(), "Half-Life 2".to_string()))
    );
    let acf_proton = "\"AppState\"\n{\n\t\"appid\"\t\t\"1493710\"\n\t\"name\"\t\t\"Proton Experimental\"\n}\n";
    assert!(parse_steam_acf(acf_proton).is_none());

    // 3. Playtime formatter, favorites persistence & unified discovery
    assert_eq!(format_playtime(20), "New");
    assert_eq!(format_playtime(1800), "30m played");
    assert_eq!(format_playtime(8100), "2h 15m");
    let pt = parse_playtime_tsv("steam-220\t8100\nra2\t3600\n");
    assert_eq!(pt.get("steam-220"), Some(&8100));

    let root = make_temp_dir("v090_gamehub");
    let steam_dir = root.join("steamapps");
    let conf_dir = root.join("conf");
    fs::create_dir_all(&steam_dir).unwrap();
    fs::create_dir_all(&conf_dir).unwrap();
    fs::write(steam_dir.join("appmanifest_220.acf"), acf_ok).unwrap();
    fs::write(conf_dir.join("playtime.tsv"), "steam-220\t8100\n").unwrap();

    assert!(toggle_favorite(&conf_dir, "steam-220").unwrap());
    assert!(load_favorites(&conf_dir).contains("steam-220"));

    let games = discover_unified_games(&[], &[steam_dir.as_path()], &conf_dir);
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].name, "Half-Life 2");
    assert_eq!(games[0].platform_badge, "Steam");
    assert!(games[0].favorite);
    assert_eq!(games[0].playtime_label, "2h 15m");
    assert_eq!(filter_games(&games, "favorites", "half").len(), 1);
    assert_eq!(filter_games(&games, "classic", "").len(), 0);
    let _ = fs::remove_dir_all(&root);

    // 4. Gamepad & Save Backup parsers
    assert_eq!(mapper_profile_from_index(1), "arrows_space");
    assert_eq!(mapper_profile_to_index("dos_classic"), 2);
    let gp = parse_gamepad_status(
        "evtest_available=1\nuinput_ready=1\nmapper_running=1\nmapper_profile=wasd_mouse\ncontrollers=/dev/input/js0|Xbox Wireless Controller|Xbox|85%\n",
    );
    assert!(gp.evtest_available && gp.uinput_ready && gp.mapper_running);
    assert_eq!(gp.controllers.len(), 1);
    assert_eq!(gp.controllers[0].vendor_kind, "Xbox");
    assert_eq!(gp.controllers[0].battery, "85%");

    let sv = parse_gamesave_status(
        "backup_dir=/home/unicore/Backups/GameSaves\nauto_backup=1\ncloud_remote=GoogleDrive\nbackup_count=2\nbackups=ra2-20261128.tar.gz|512|/home/unicore/Backups/GameSaves/ra2-20261128.tar.gz\n",
    );
    assert!(sv.auto_backup);
    assert_eq!(sv.cloud_remote, "GoogleDrive");
    assert_eq!(sv.backups.len(), 1);
    assert_eq!(sv.backups[0].size_kb, 512);

    // 5. Start Menu Game Hub entry + bilingual labels
    let gh_en = builtin_gamehub_apps(UiLang::En);
    let gh_ms = builtin_gamehub_apps(UiLang::Ms);
    assert_eq!(gh_en.len(), 1);
    assert_ne!(gh_en[0].name, gh_ms[0].name);

    let en = UiLabels::for_lang(UiLang::En);
    let ms = UiLabels::for_lang(UiLang::Ms);
    assert_ne!(en.title_gamehub, ms.title_gamehub);
    assert_ne!(en.gh_tab_library, ms.gh_tab_library);
    assert_ne!(en.gh_tab_gamepad, ms.gh_tab_gamepad);
    assert_ne!(en.gh_tab_saves, ms.gh_tab_saves);
    assert_ne!(en.btn_sync_cloud, ms.btn_sync_cloud);
}

#[test]
fn v100_security_audit_bilingual_guide_and_cli_work() {
    use unicore_center::guide::{
        bilingual_guide_chapters, format_security_audit_summary, parse_security_audit_output,
    };
    use unicore_center::start_emoji::builtin_guide_apps;

    // 1. CLI --help-guide and --guide
    assert_eq!(
        parse_args(&["--help-guide".to_string()]).unwrap().unwrap().mode,
        LaunchMode::HelpGuide(0)
    );
    assert_eq!(
        parse_args(&["--guide".to_string()]).unwrap().unwrap().mode,
        LaunchMode::HelpGuide(0)
    );

    // 2. Bilingual 6-chapter offline User Guide
    let ch_en = bilingual_guide_chapters(UiLang::En);
    let ch_ms = bilingual_guide_chapters(UiLang::Ms);
    assert_eq!(ch_en.len(), 6);
    assert_eq!(ch_ms.len(), 6);
    for (e, m) in ch_en.iter().zip(ch_ms.iter()) {
        assert_eq!(e.id, m.id);
        assert_ne!(e.title, m.title);
        assert_ne!(e.body, m.body);
        assert!(!e.tip.is_empty() && !m.tip.is_empty());
    }

    // 3. Live Security Audit & 0-MB-idle output parser + formatter
    let sample = "\
status=PASS
checks_passed=6
checks_total=6
doas_conf_locked=1
ota_signature_enforced=1
libexec_perms_ok=1
credentials_0600=1
bwrap_available=1
firewall_ready=1
firewall_active=1
idle_heavy_count=0
idle_heavy_running=none
ram_total_mb=3072
ram_used_mb=210
hw_tier=modern
issues=none
";
    let rep = parse_security_audit_output(sample);
    assert_eq!(rep.status, "PASS");
    assert_eq!(rep.checks_passed, 6);
    assert_eq!(rep.checks_total, 6);
    assert!(rep.doas_locked && rep.ota_signed && rep.libexec_safe && rep.creds_0600 && rep.bwrap_ready && rep.firewall_ready && rep.firewall_active);
    assert_eq!(rep.idle_heavy_count, 0);
    assert_eq!(rep.ram_used_mb, 210);

    let sum_en = format_security_audit_summary(&rep, UiLang::En);
    let sum_ms = format_security_audit_summary(&rep, UiLang::Ms);
    assert!(sum_en.contains("PASS (6/6)"));
    assert!(sum_en.contains("0 MB idle"));
    assert!(sum_ms.contains("PASS (6/6)"));
    assert!(sum_ms.contains("0 MB terbiar"));

    // 4. Start Menu Guide tile & bilingual labels
    let g_en = builtin_guide_apps(UiLang::En);
    let g_ms = builtin_guide_apps(UiLang::Ms);
    assert_eq!(g_en.len(), 1);
    assert_ne!(g_en[0].name, g_ms[0].name);
    assert!(g_en[0].exec_cmd.contains("--help-guide"));

    let en = UiLabels::for_lang(UiLang::En);
    let ms = UiLabels::for_lang(UiLang::Ms);
    assert_ne!(en.title_guide, ms.title_guide);
    assert_ne!(en.btn_open_guide, ms.btn_open_guide);
    assert_ne!(en.guide_sec_hdr, ms.guide_sec_hdr);
    assert_ne!(en.btn_run_sec_audit, ms.btn_run_sec_audit);
}
