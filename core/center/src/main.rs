//! Entry point and Slint UI bindings for `unicore-center`.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use slint::{ModelRc, SharedString, VecModel};
use unicore_center::backend::{CenterBackend, InstallRequest};
use unicore_center::capture_a11y::{
    add_network_share, dismiss_all_notifications, list_network_shares, open_capture_dir,
    read_a11y_overview, read_capture_overview, read_notify_overview, restore_last_notification,
    run_capture_shot, set_dnd_state, share_action, speak_text_offline, start_screen_recording,
    stop_screen_recording, toggle_on_screen_keyboard, NetworkShare,
};
use unicore_center::cli::{help_text, parse_args, LaunchMode};
use unicore_center::desktop::{
    platform_display_name, supports_win_era, win_era_display_name, win_era_from_index,
    win_era_to_index,
};
use unicore_center::dev_ai::{
    default_picker_dir, list_directory_entries, read_ai_overview, read_devbox_overview,
    remove_devbox, run_ai_action, run_ai_copilot, run_devbox_create, save_ai_provider,
    spawn_devbox_shell, validate_ai_model_name, PickerEntry, DEVBOX_TEMPLATES,
};
use unicore_center::ecosystem::{
    build_migrate_flags_csv, cloud_connect, cloud_provider_from_index, cloud_remote_action,
    cloud_set_autostart, gpu_mode_from_index, gpu_mode_to_index, hw_fw_check, hw_fw_update,
    hw_reset_audio_bt, hw_set_gpu_mode, hw_set_wifi_powersave, inspect_windows_partition,
    phone_connect_adb_wifi, phone_device_action, phone_mirror_android, phone_service_action,
    phone_set_autostart, read_cloud_overview, read_hw_overview, read_phone_overview,
    run_windows_migration, scan_windows_partitions, WindowsPartition,
};
use unicore_center::gaming::{
    apply_cpu_governor, default_gaming_dir, load_gaming_config, read_host_cpu_governor,
    save_gaming_config, GamingConfig,
};
use unicore_center::i18n::{detect_ui_lang, persist_ui_lang, UiLabels, UiLang};
use unicore_center::installer::{
    list_install_disks, read_boot_state, run_disk_install, run_disk_install_alongside,
    run_disk_install_alongside_encrypted, run_disk_install_encrypted,
    validate_install_confirmation, validate_luks_passphrase, InstallDisk,
};
use unicore_center::oobe::{
    apply_oobe_on_host, save_oobe_state, validate_hostname, validate_user_password,
    OobeSettings, DEFAULT_OOBE_DIR, OOBE_LANGUAGES, OOBE_TIMEZONES,
};
use unicore_center::retro::{
    prepare_retro_source, read_retro_profile_from_env, virtual_desktop_from_index,
    virtual_desktop_to_index, RetroGameProfile, RetroPreset,
};
use unicore_center::settings::{
    accent_color_from_index, accent_color_to_index, add_printer, apply_and_save_display,
    apply_firewall_settings, bt_device_action, clamp_pointer_speed, connect_wifi,
    delete_vpn_connection, desktop_layout_from_index, desktop_layout_to_index, disconnect_wifi,
    discover_printers, enroll_secureboot_mok, forget_wifi, import_vpn_profile,
    launch_document_scanner, list_backup_targets, list_vpn_connections, load_nightlight_config,
    load_personalize_config, persistent_display_config_dir, print_test_page, read_audio_overview,
    read_bluetooth_overview, read_display_overview, read_firewall_overview, read_network_overview,
    read_printers_overview, read_secureboot_overview, remove_printer, run_backup_create,
    run_backup_restore, run_power_action, save_and_apply_nightlight, save_and_apply_personalize,
    scan_bluetooth_brief, set_audio_card_profile, set_audio_mute, set_audio_volume,
    set_bluetooth_enabled, set_brightness, set_cups_enabled, set_default_printer, set_default_sink,
    set_mic_mute, set_wifi_radio, toggle_vpn_connection, validate_backup_passphrase,
    validate_backup_path, AudioCardProfile, AudioSink, BackupTarget, BluetoothDevice,
    DiscoveredPrinter, DisplayOutput, PersonalizeConfig, PrinterInfo, VpnConnection, WifiNetwork,
    KB_LAYOUTS, WALLPAPER_PRESETS,
};
use unicore_center::start_emoji::{
    discover_start_menu_apps, filter_emojis, filter_start_menu_apps, friendly_hw_summary,
    insert_emoji, launch_desktop_command, oobe_bundle_catalog_ids, open_user_folder,
    DesktopAppItem,
};
use unicore_center::store::{
    chunk_into_triples, install_latest_proton_blocking, install_native_browser_blocking,
    install_store_app_blocking, launch_flatpak_app, load_store_views_for_arch,
    read_browser_overview, read_flatpak_overview, read_proton_overview, set_default_browser,
    sync_store_catalog_blocking, uninstall_store_app_blocking, StoreItemView,
};
use unicore_center::taskmgr::{list_user_processes, terminate_process, ProcessInfo};
use unicore_center::updater::{
    apply_local_update, apply_remote_update, check_remote_update, read_update_overview,
    rollback_update, set_auto_check_updates,
};
use unicore_center::users_power::{
    add_user_account, authenticate_user, delete_user_account, read_power_overview,
    read_smart_health, read_users_list, set_account_password, set_idle_timers, set_power_profile,
    UserAccount,
};
use uniload::inspector::WinEra;
use uniload::sandbox::{format_env_vars_str, AppSandbox, PermissionProfile};

slint::include_modules!();

const DISPLAY_SCALES: [&str; 4] = ["1.0", "1.25", "1.5", "2.0"];
const POWER_PROFILES: [&str; 3] = ["powersave", "balanced", "performance"];

#[derive(Default)]
struct SettingsUiState {
    wifi: Vec<WifiNetwork>,
    bt: Vec<BluetoothDevice>,
    sinks: Vec<AudioSink>,
    audio_profiles: Vec<AudioCardProfile>,
    shares: Vec<NetworkShare>,
    outputs: Vec<DisplayOutput>,
    printers: Vec<PrinterInfo>,
    discovered_printers: Vec<DiscoveredPrinter>,
    vpns: Vec<VpnConnection>,
    backup_targets: Vec<BackupTarget>,
    users: Vec<UserAccount>,
    dev_containers: Vec<String>,
    picker_entries: Vec<PickerEntry>,
    picker_dir: std::path::PathBuf,
}

fn apply_ui_language(ui: &MainWindow, lang: UiLang) {
    let l = UiLabels::for_lang(lang);
    ui.set_lang_badge(SharedString::from(lang.code()));
    ui.set_oobe_lang_idx(if matches!(lang, UiLang::Ms) { 1 } else { 0 });
    ui.set_tr(UiText {
        title_center: l.title_center.into(),
        title_open: l.title_open.into(),
        title_power: l.title_power.into(),
        title_oobe: l.title_oobe.into(),
        title_taskmgr: l.title_taskmgr.into(),
        tab_apps: l.tab_apps.into(),
        tab_store: l.tab_store.into(),
        tab_dev_ai: l.tab_dev_ai.into(),
        tab_install: l.tab_install.into(),
        tab_system: l.tab_system.into(),
        tab_settings: l.tab_settings.into(),
        tab_tasks: l.tab_tasks.into(),
        tab_power: l.tab_power.into(),
        power_prompt: l.power_prompt.into(),
        btn_shutdown: l.btn_shutdown.into(),
        btn_reboot: l.btn_reboot.into(),
        btn_sleep: l.btn_sleep.into(),
        btn_logout: l.btn_logout.into(),
        btn_back_center: l.btn_back_center.into(),
        btn_close: l.btn_close.into(),
        oobe_step1: l.oobe_step1.into(),
        oobe_step2: l.oobe_step2.into(),
        oobe_step3: l.oobe_step3.into(),
        oobe_lang: l.oobe_lang.into(),
        oobe_tz: l.oobe_tz.into(),
        oobe_hostname: l.oobe_hostname.into(),
        oobe_pw_hint: l.oobe_pw_hint.into(),
        oobe_pw: l.oobe_pw.into(),
        oobe_pw_confirm: l.oobe_pw_confirm.into(),
        oobe_ready_hint: l.oobe_ready_hint.into(),
        btn_back: l.btn_back.into(),
        btn_skip: l.btn_skip.into(),
        btn_next: l.btn_next.into(),
        btn_finish_install: l.btn_finish_install.into(),
        btn_finish: l.btn_finish.into(),
        open_app_name: l.open_app_name.into(),
        open_app_type: l.open_app_type.into(),
        open_simple_hint: l.open_simple_hint.into(),
        open_show_adv: l.open_show_adv.into(),
        open_hide_adv: l.open_hide_adv.into(),
        btn_run_now: l.btn_run_now.into(),
        btn_install_run: l.btn_install_run.into(),
        btn_install_only: l.btn_install_only.into(),
        btn_open_center: l.btn_open_center.into(),
        btn_cancel: l.btn_cancel.into(),
        chk_create_shortcut: l.chk_create_shortcut.into(),
        taskmgr_heading: l.taskmgr_heading.into(),
        taskmgr_empty: l.taskmgr_empty.into(),
        btn_end_task: l.btn_end_task.into(),
        btn_force_kill: l.btn_force_kill.into(),
        btn_refresh: l.btn_refresh.into(),
        hdr_registered_apps: l.hdr_registered_apps.into(),
        btn_add: l.btn_add.into(),
        btn_launch_app: l.btn_launch_app.into(),
        btn_create_shortcut: l.btn_create_shortcut.into(),
        btn_view_log: l.btn_view_log.into(),
        btn_hide_log: l.btn_hide_log.into(),
        btn_uninstall: l.btn_uninstall.into(),
        subtab_wifi: l.subtab_wifi.into(),
        subtab_bt: l.subtab_bt.into(),
        subtab_audio: l.subtab_audio.into(),
        subtab_display: l.subtab_display.into(),
        subtab_printers: l.subtab_printers.into(),
        subtab_security: l.subtab_security.into(),
        subtab_power_users: l.subtab_power_users.into(),
        chk_night_light: l.chk_night_light.into(),
        lbl_night_temp: l.lbl_night_temp.into(),
        hdr_printers: l.hdr_printers.into(),
        chk_cups_service: l.chk_cups_service.into(),
        btn_scan_printers: l.btn_scan_printers.into(),
        btn_open_scanner: l.btn_open_scanner.into(),
        btn_add_printer: l.btn_add_printer.into(),
        btn_set_default: l.btn_set_default.into(),
        btn_test_page: l.btn_test_page.into(),
        btn_remove_printer: l.btn_remove_printer.into(),
        hdr_firewall: l.hdr_firewall.into(),
        chk_firewall: l.chk_firewall.into(),
        chk_fw_ssh: l.chk_fw_ssh.into(),
        hdr_vpn: l.hdr_vpn.into(),
        btn_import_vpn: l.btn_import_vpn.into(),
        hdr_backup: l.hdr_backup.into(),
        chk_backup_enc: l.chk_backup_enc.into(),
        btn_create_backup: l.btn_create_backup.into(),
        btn_restore_backup: l.btn_restore_backup.into(),
        hdr_gaming_perf: l.hdr_gaming_perf.into(),
        lbl_cpu_governor: l.lbl_cpu_governor.into(),
        chk_game_mode: l.chk_game_mode.into(),
        chk_show_fps: l.chk_show_fps.into(),
        gaming_hint: l.gaming_hint.into(),
        btn_classic_runtimes: l.btn_classic_runtimes.into(),
        hdr_store: l.hdr_store.into(),
        store_search_hint: l.store_search_hint.into(),
        btn_store_install: l.btn_store_install.into(),
        btn_store_launch: l.btn_store_launch.into(),
        btn_store_sync: l.btn_store_sync.into(),
        store_cat_all: l.store_cat_all.into(),
        store_cat_util: l.store_cat_util.into(),
        store_cat_office: l.store_cat_office.into(),
        store_cat_android: l.store_cat_android.into(),
        store_nav_discover: l.store_nav_discover.into(),
        store_featured_tag: l.store_featured_tag.into(),
        store_section_apps: l.store_section_apps.into(),
        store_btn_get: l.store_btn_get.into(),
        store_btn_open: l.store_btn_open.into(),
        store_btn_back: l.store_btn_back.into(),
        store_about_hdr: l.store_about_hdr.into(),
        store_security_hdr: l.store_security_hdr.into(),
        store_feed_toggle: l.store_feed_toggle.into(),
        store_cat_retro: l.store_cat_retro.into(),
        store_retro_banner_title: l.store_retro_banner_title.into(),
        store_retro_banner_hint: l.store_retro_banner_hint.into(),
        hdr_personalize: l.hdr_personalize.into(),
        lbl_theme: l.lbl_theme.into(),
        lbl_wallpaper: l.lbl_wallpaper.into(),
        lbl_kb_layout: l.lbl_kb_layout.into(),
        chk_natural_scroll: l.chk_natural_scroll.into(),
        chk_tap_click: l.chk_tap_click.into(),
        lbl_pointer_speed: l.lbl_pointer_speed.into(),
        btn_apply_personalize: l.btn_apply_personalize.into(),
        title_quick: l.title_quick.into(),
        title_login: l.title_login.into(),
        hdr_power_smart: l.hdr_power_smart.into(),
        hdr_users: l.hdr_users.into(),
        hdr_local_ai: l.hdr_local_ai.into(),
        hdr_devbox: l.hdr_devbox.into(),
        btn_browse: l.btn_browse.into(),
        title_capture: l.title_capture.into(),
        title_ai: l.title_ai.into(),
        lbl_taskbar_style: l.lbl_taskbar_style.into(),
        hdr_accessibility: l.hdr_accessibility.into(),
        chk_high_contrast: l.chk_high_contrast.into(),
        chk_large_text: l.chk_large_text.into(),
        btn_osk_toggle: l.btn_osk_toggle.into(),
        btn_read_aloud: l.btn_read_aloud.into(),
        hdr_net_shares: l.hdr_net_shares.into(),
        btn_add_share: l.btn_add_share.into(),
        btn_mount: l.btn_mount.into(),
        btn_unmount: l.btn_unmount.into(),
        btn_open_folder: l.btn_open_folder.into(),
        lbl_audio_profile: l.lbl_audio_profile.into(),
        tile_dnd: l.tile_dnd.into(),
        tile_capture: l.tile_capture.into(),
        btn_dismiss_notifs: l.btn_dismiss_notifs.into(),
        btn_restore_notif: l.btn_restore_notif.into(),
        store_cat_gaming: l.store_cat_gaming.into(),
        store_cat_creative: l.store_cat_creative.into(),
        store_cat_social: l.store_cat_social.into(),
        store_cat_dev: l.store_cat_dev.into(),
        btn_shot_full: l.btn_shot_full.into(),
        btn_shot_region: l.btn_shot_region.into(),
        btn_rec_start: l.btn_rec_start.into(),
        btn_rec_stop: l.btn_rec_stop.into(),
        chk_rec_audio: l.chk_rec_audio.into(),
        lbl_delay: l.lbl_delay.into(),
        btn_open_shots: l.btn_open_shots.into(),
        btn_open_videos: l.btn_open_videos.into(),
        title_start_menu: l.title_start_menu.into(),
        title_emoji: l.title_emoji.into(),
        start_search_hint: l.start_search_hint.into(),
        start_pinned: l.start_pinned.into(),
        start_all_apps: l.start_all_apps.into(),
        start_cat_all: l.start_cat_all.into(),
        start_cat_internet: l.start_cat_internet.into(),
        start_cat_office: l.start_cat_office.into(),
        start_cat_media: l.start_cat_media.into(),
        start_cat_gaming: l.start_cat_gaming.into(),
        start_cat_system: l.start_cat_system.into(),
        emoji_search_hint: l.emoji_search_hint.into(),
        lbl_desktop_layout: l.lbl_desktop_layout.into(),
        lbl_accent_color: l.lbl_accent_color.into(),
        oobe_step_look: l.oobe_step_look.into(),
        oobe_step_apps: l.oobe_step_apps.into(),
        oobe_apps_hint: l.oobe_apps_hint.into(),
        oobe_pack_office: l.oobe_pack_office.into(),
        oobe_pack_creative: l.oobe_pack_creative.into(),
        oobe_pack_gaming: l.oobe_pack_gaming.into(),
        title_retro_wizard: l.title_retro_wizard.into(),
        title_repair: l.title_repair.into(),
        title_retro_pick: l.title_retro_pick.into(),
        btn_add_retro: l.btn_add_retro.into(),
        hdr_retro_tuning: l.hdr_retro_tuning.into(),
        lbl_retro_preset: l.lbl_retro_preset.into(),
        chk_fps_cap: l.chk_fps_cap.into(),
        chk_single_core: l.chk_single_core.into(),
        lbl_safe_window: l.lbl_safe_window.into(),
        lbl_cd_iso: l.lbl_cd_iso.into(),
        btn_save_retro: l.btn_save_retro.into(),
        btn_scan_game_exe: l.btn_scan_game_exe.into(),
        btn_retro_install_play: l.btn_retro_install_play.into(),
        btn_repair_retry: l.btn_repair_retry.into(),
        btn_use_game_exe: l.btn_use_game_exe.into(),
        subtab_ecosystem: l.subtab_ecosystem.into(),
        title_ecosystem: l.title_ecosystem.into(),
        eco_tab_phone: l.eco_tab_phone.into(),
        eco_tab_cloud: l.eco_tab_cloud.into(),
        eco_tab_migrate: l.eco_tab_migrate.into(),
        eco_tab_drivers: l.eco_tab_drivers.into(),
        hdr_phone_link: l.hdr_phone_link.into(),
        hdr_cloud_drives: l.hdr_cloud_drives.into(),
        hdr_win_migrate: l.hdr_win_migrate.into(),
        hdr_hw_drivers: l.hdr_hw_drivers.into(),
        btn_phone_mirror: l.btn_phone_mirror.into(),
        btn_cloud_connect: l.btn_cloud_connect.into(),
        btn_migrate_scan: l.btn_migrate_scan.into(),
        btn_migrate_start: l.btn_migrate_start.into(),
        btn_hw_reset_audio: l.btn_hw_reset_audio.into(),
        btn_hw_fw_check: l.btn_hw_fw_check.into(),
    });
}

fn parse_hex_color(hex: &str) -> slint::Color {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&clean[0..2], 16),
            u8::from_str_radix(&clean[2..4], 16),
            u8::from_str_radix(&clean[4..6], 16),
        ) {
            return slint::Color::from_rgb_u8(r, g, b);
        }
    }
    slint::Color::from_rgb_u8(2, 132, 199)
}

fn scan_start_menu_apps(sandbox_apps: &[AppSandbox], lang: UiLang) -> Vec<DesktopAppItem> {
    let dirs = [
        Path::new("/usr/share/applications"),
        Path::new("/data/home/applications"),
        Path::new("/data/flatpak/exports/share/applications"),
    ];
    discover_start_menu_apps(&dirs, sandbox_apps, lang)
}

fn refresh_start_menu(ui: &MainWindow, all_apps: &[DesktopAppItem], category: &str, query: &str) {
    let pinned: Vec<&DesktopAppItem> = all_apps.iter().filter(|a| a.pinned).collect();
    let mut pinned_rows = Vec::new();
    for chunk in pinned.chunks(4) {
        let g = |i: usize| chunk.get(i).copied();
        let make = |opt: Option<&DesktopAppItem>| -> (bool, SharedString, SharedString, SharedString, slint::Color) {
            match opt {
                Some(a) => (
                    true,
                    SharedString::from(a.name.as_str()),
                    SharedString::from(a.badge.as_str()),
                    SharedString::from(a.exec_cmd.as_str()),
                    parse_hex_color(&a.accent_hex),
                ),
                None => (
                    false,
                    SharedString::new(),
                    SharedString::new(),
                    SharedString::new(),
                    slint::Color::from_rgb_u8(30, 41, 59),
                ),
            }
        };
        let (h0, n0, b0, e0, c0) = make(g(0));
        let (h1, n1, b1, e1, c1) = make(g(1));
        let (h2, n2, b2, e2, c2) = make(g(2));
        let (h3, n3, b3, e3, c3) = make(g(3));
        pinned_rows.push(StartPinnedRow {
            has_0: h0, n0, b0, e0, c0,
            has_1: h1, n1, b1, e1, c1,
            has_2: h2, n2, b2, e2, c2,
            has_3: h3, n3, b3, e3, c3,
        });
    }
    ui.set_start_pinned_rows(ModelRc::new(VecModel::from(pinned_rows)));

    let filtered = filter_start_menu_apps(all_apps, category, query);
    let mut app_rows = Vec::new();
    for chunk in filtered.chunks(2) {
        let a = &chunk[0];
        let b = chunk.get(1);
        app_rows.push(StartAppRow {
            has_a: true,
            a_name: SharedString::from(a.name.as_str()),
            a_desc: SharedString::from(a.comment.as_str()),
            a_badge: SharedString::from(a.badge.as_str()),
            a_exec: SharedString::from(a.exec_cmd.as_str()),
            a_color: parse_hex_color(&a.accent_hex),
            has_b: b.is_some(),
            b_name: b.map(|x| SharedString::from(x.name.as_str())).unwrap_or_default(),
            b_desc: b.map(|x| SharedString::from(x.comment.as_str())).unwrap_or_default(),
            b_badge: b.map(|x| SharedString::from(x.badge.as_str())).unwrap_or_default(),
            b_exec: b.map(|x| SharedString::from(x.exec_cmd.as_str())).unwrap_or_default(),
            b_color: b
                .map(|x| parse_hex_color(&x.accent_hex))
                .unwrap_or_else(|| slint::Color::from_rgb_u8(30, 41, 59)),
        });
    }
    ui.set_start_app_rows(ModelRc::new(VecModel::from(app_rows)));
}

fn refresh_emoji_grid(ui: &MainWindow, category: &str, query: &str) {
    let items = filter_emojis(category, query);
    let mut rows = Vec::new();
    for chunk in items.chunks(8) {
        let sym = |i: usize| -> SharedString {
            chunk
                .get(i)
                .map(|e| SharedString::from(e.symbol))
                .unwrap_or_default()
        };
        rows.push(EmojiRow {
            e0: sym(0), e1: sym(1), e2: sym(2), e3: sym(3),
            e4: sym(4), e5: sym(5), e6: sym(6), e7: sym(7),
        });
    }
    ui.set_emoji_rows(ModelRc::new(VecModel::from(rows)));
}

fn refresh_capture_and_notify(ui: &MainWindow) {
    let cap = read_capture_overview();
    ui.set_cap_recording(cap.recording);
    ui.set_cap_active_file(SharedString::from(cap.active_file.as_str()));
    ui.set_cap_wf_available(cap.wf_recorder || !cfg!(target_os = "linux"));

    let notif = read_notify_overview();
    ui.set_notify_dnd(notif.dnd);

    let a11y = read_a11y_overview();
    ui.set_a11y_osk_running(a11y.osk_running);
    ui.set_a11y_tts_available(a11y.tts_available || !cfg!(target_os = "linux"));
}

fn refresh_tasks(ui: &MainWindow, tasks_cache: &Rc<RefCell<Vec<ProcessInfo>>>) {
    let procs = list_user_processes();
    let entries: Vec<TaskEntry> = procs
        .iter()
        .map(|p| TaskEntry {
            pid: p.pid as i32,
            name: SharedString::from(p.name.as_str()),
            ram_label: SharedString::from(format!("{} MB", p.ram_mb)),
            detail: SharedString::from(p.cmdline.as_str()),
        })
        .collect();
    ui.set_task_list(ModelRc::new(VecModel::from(entries)));
    if procs.is_empty() {
        ui.set_selected_task_idx(-1);
        ui.set_selected_task_pid(0);
    } else {
        ui.set_selected_task_idx(0);
        ui.set_selected_task_pid(procs[0].pid as i32);
    }
    *tasks_cache.borrow_mut() = procs;
}

fn refresh_boot_and_disks(ui: &MainWindow, disks_cell: &Rc<RefCell<Vec<InstallDisk>>>) {
    let boot = read_boot_state();
    ui.set_sys_boot_mode(SharedString::from(boot.display_summary()));
    let is_live = boot.is_live();
    ui.set_sys_is_live(is_live);

    if !is_live {
        disks_cell.borrow_mut().clear();
        ui.set_inst_disk_labels(ModelRc::new(VecModel::from(vec![SharedString::from(
            "Only available when booted from Live ISO/USB",
        )])));
        ui.set_inst_disk_idx(0);
        ui.set_inst_has_disk(false);
        ui.set_inst_dualboot_hint(SharedString::new());
        return;
    }

    let eligible: Vec<InstallDisk> = list_install_disks()
        .into_iter()
        .filter(|d| d.eligible)
        .collect();
    if eligible.is_empty() {
        disks_cell.borrow_mut().clear();
        ui.set_inst_disk_labels(ModelRc::new(VecModel::from(vec![SharedString::from(
            "No eligible disk (>= 8 GB) found",
        )])));
        ui.set_inst_disk_idx(0);
        ui.set_inst_has_disk(false);
        ui.set_inst_dualboot_hint(SharedString::new());
    } else {
        let labels: Vec<SharedString> = eligible
            .iter()
            .map(|d| {
                let mut tags = Vec::new();
                if d.dualboot_eligible {
                    tags.push(format!("{} free", d.free_gb));
                }
                if d.windows_detected {
                    tags.push("Windows".to_string());
                }
                if tags.is_empty() {
                    SharedString::from(format!("{} ({}) - {}", d.device, d.size_gb, d.model))
                } else {
                    SharedString::from(format!(
                        "{} ({}) - {} [{}]",
                        d.device,
                        d.size_gb,
                        d.model,
                        tags.join(" | ")
                    ))
                }
            })
            .collect();
        let dual_ready: Vec<String> = eligible
            .iter()
            .filter(|d| d.dualboot_eligible)
            .map(|d| format!("{} ({} unallocated)", d.device, d.free_gb))
            .collect();
        let hint = if !dual_ready.is_empty() {
            format!("Dual-Boot Ready: {}", dual_ready.join(", "))
        } else if eligible.iter().any(|d| d.windows_detected) {
            "Windows detected without >= 10 GB unallocated space. Shrink C: in Windows Disk Management for Safe Dual-Boot.".to_string()
        } else {
            String::new()
        };
        *disks_cell.borrow_mut() = eligible;
        ui.set_inst_disk_labels(ModelRc::new(VecModel::from(labels)));
        ui.set_inst_disk_idx(0);
        ui.set_inst_has_disk(true);
        ui.set_inst_dualboot_hint(SharedString::from(hint));
    }
}

fn refresh_update_status(ui: &MainWindow) {
    let upd = read_update_overview();
    ui.set_upd_current_version(SharedString::from(upd.current_version.as_str()));
    ui.set_upd_active_slot(SharedString::from(upd.active_slot.as_str()));
    ui.set_upd_standby_slot(SharedString::from(upd.standby_slot.as_str()));
    ui.set_upd_pending_summary(SharedString::from(upd.pending_summary()));
    if ui.get_upd_url_input().is_empty() && !upd.update_url.is_empty() {
        ui.set_upd_url_input(SharedString::from(upd.update_url.as_str()));
    }
    ui.set_upd_can_update(upd.can_update);
    ui.set_upd_can_rollback(upd.can_rollback);
    ui.set_upd_auto_check(upd.auto_check);
}

fn set_disp_modes_for_output(ui: &MainWindow, output: Option<&DisplayOutput>) {
    let Some(out) = output else {
        ui.set_set_disp_modes(ModelRc::new(VecModel::from(vec![SharedString::from("Auto")])));
        ui.set_set_disp_mode_idx(0);
        ui.set_set_disp_scale_idx(0);
        return;
    };

    let modes: Vec<SharedString> = if out.modes.is_empty() {
        vec![SharedString::from("Auto")]
    } else {
        out.modes.iter().map(|m| SharedString::from(m.as_str())).collect()
    };
    let active_mode_idx = out
        .modes
        .iter()
        .position(|m| m == &out.current_mode)
        .unwrap_or(0) as i32;
    let scale_idx = DISPLAY_SCALES
        .iter()
        .position(|&s| s == out.scale)
        .unwrap_or(0) as i32;

    ui.set_set_disp_modes(ModelRc::new(VecModel::from(modes)));
    ui.set_set_disp_mode_idx(active_mode_idx);
    ui.set_set_disp_scale_idx(scale_idx);
}

fn refresh_settings(ui: &MainWindow, state: &mut SettingsUiState) {
    let net = read_network_overview();
    ui.set_set_net_available(net.nm_available);
    ui.set_set_wifi_enabled(net.wifi_enabled);
    ui.set_set_net_summary(SharedString::from(net.summary.as_str()));
    let wifi_entries: Vec<WifiEntry> = net
        .networks
        .iter()
        .map(|w| WifiEntry {
            in_use: w.in_use,
            ssid: SharedString::from(w.ssid.as_str()),
            signal: w.signal,
            security: SharedString::from(w.security.as_str()),
        })
        .collect();
    ui.set_set_wifi_list(ModelRc::new(VecModel::from(wifi_entries)));
    ui.set_set_selected_wifi_idx(-1);
    state.wifi = net.networks;

    let bt = read_bluetooth_overview();
    ui.set_set_bt_powered(bt.powered);
    ui.set_set_bt_summary(SharedString::from(bt.summary.as_str()));
    let bt_entries: Vec<BtEntry> = bt
        .devices
        .iter()
        .map(|d| BtEntry {
            mac: SharedString::from(d.mac.as_str()),
            name: SharedString::from(d.name.as_str()),
            paired: d.paired,
            connected: d.connected,
        })
        .collect();
    ui.set_set_bt_list(ModelRc::new(VecModel::from(bt_entries)));
    ui.set_set_selected_bt_idx(-1);
    ui.set_set_selected_bt_mac(SharedString::new());
    state.bt = bt.devices;

    let audio = read_audio_overview();
    ui.set_set_audio_available(audio.available);
    ui.set_set_audio_volume(audio.volume_pct as f32);
    ui.set_set_audio_muted(audio.muted);
    ui.set_set_mic_muted(audio.mic_muted);
    let sink_labels: Vec<SharedString> = if audio.sinks.is_empty() {
        vec![SharedString::from("Default Audio Output")]
    } else {
        audio
            .sinks
            .iter()
            .map(|s| SharedString::from(s.label.as_str()))
            .collect()
    };
    let active_sink_idx = audio
        .sinks
        .iter()
        .position(|s| s.is_default)
        .unwrap_or(0) as i32;
    ui.set_set_audio_sinks(ModelRc::new(VecModel::from(sink_labels)));
    ui.set_set_audio_sink_idx(active_sink_idx);
    state.sinks = audio.sinks;

    let prof_labels: Vec<SharedString> = if audio.card_profiles.is_empty() {
        vec![SharedString::from("Default Audio Profile")]
    } else {
        audio
            .card_profiles
            .iter()
            .map(|p| SharedString::from(p.label.as_str()))
            .collect()
    };
    let active_prof_idx = audio
        .card_profiles
        .iter()
        .position(|p| p.active)
        .unwrap_or(0) as i32;
    ui.set_set_audio_profiles(ModelRc::new(VecModel::from(prof_labels)));
    ui.set_set_audio_profile_idx(active_prof_idx);
    state.audio_profiles = audio.card_profiles;

    let shares = list_network_shares();
    let share_rows: Vec<ShareEntry> = shares
        .iter()
        .map(|s| ShareEntry {
            id: SharedString::from(s.id.as_str()),
            label: SharedString::from(s.label.as_str()),
            uri: SharedString::from(s.uri.as_str()),
            username: SharedString::from(s.username.as_str()),
            mounted: s.mounted,
        })
        .collect();
    ui.set_set_share_list(ModelRc::new(VecModel::from(share_rows)));
    if let Some(first) = shares.first() {
        ui.set_set_selected_share_idx(0);
        ui.set_set_selected_share_id(SharedString::from(first.id.as_str()));
    } else {
        ui.set_set_selected_share_idx(-1);
        ui.set_set_selected_share_id(SharedString::new());
    }
    state.shares = shares;
    refresh_capture_and_notify(ui);

    let disp = read_display_overview();
    ui.set_set_backlight_available(disp.backlight_available);
    ui.set_set_brightness_pct(disp.brightness_pct as f32);
    let output_names: Vec<SharedString> = if disp.outputs.is_empty() {
        vec![SharedString::from("Default")]
    } else {
        disp.outputs
            .iter()
            .map(|o| SharedString::from(o.name.as_str()))
            .collect()
    };
    ui.set_set_disp_outputs(ModelRc::new(VecModel::from(output_names)));
    ui.set_set_disp_output_idx(0);
    set_disp_modes_for_output(ui, disp.outputs.first());
    state.outputs = disp.outputs;

    let night = load_nightlight_config(&persistent_display_config_dir());
    ui.set_set_nightlight_enabled(night.enabled);
    ui.set_set_nightlight_temp(night.temp_k as f32);

    let pers = load_personalize_config(&persistent_display_config_dir());
    ui.invoke_set_theme(pers.theme == "light");
    ui.set_set_theme_idx(if pers.theme == "light" { 1 } else { 0 });
    let wp_idx = WALLPAPER_PRESETS
        .iter()
        .position(|(_, path)| *path == pers.wallpaper)
        .unwrap_or(WALLPAPER_PRESETS.len()) as i32;
    ui.set_set_wallpaper_idx(wp_idx);
    ui.set_set_wallpaper_custom(SharedString::from(pers.wallpaper.as_str()));
    let kb_idx = KB_LAYOUTS
        .iter()
        .position(|(code, _)| *code == pers.kb_layout)
        .unwrap_or(0) as i32;
    ui.set_set_kb_layout_idx(kb_idx);
    ui.set_set_natural_scroll(pers.natural_scroll);
    ui.set_set_tap_to_click(pers.tap_to_click);
    ui.set_set_pointer_speed_tenths((pers.pointer_speed * 10.0).round());
    ui.set_set_desktop_icons(pers.desktop_icons);
    ui.set_set_taskbar_style_idx(if pers.taskbar_style == "floating" { 1 } else { 0 });
    ui.set_set_desktop_layout_idx(desktop_layout_to_index(&pers.desktop_layout));
    let acc_idx = accent_color_to_index(&pers.accent_color);
    ui.set_set_accent_idx(acc_idx);
    ui.invoke_set_accent_color(acc_idx);
    ui.set_set_high_contrast(pers.high_contrast);
    ui.set_set_large_text(pers.large_text);
    ui.invoke_set_high_contrast_theme(pers.high_contrast);

    if cfg!(target_os = "linux") {
        if let Ok(out) = std::process::Command::new("date")
            .arg("+%H:%M  •  %a, %d %b")
            .output()
        {
            let txt = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !txt.is_empty() {
                ui.set_login_clock_text(SharedString::from(txt));
            }
        }
    }

    let sb = read_secureboot_overview();
    ui.set_set_sb_summary(SharedString::from(sb.summary()));
    ui.set_set_sb_can_enroll(sb.uefi && sb.mok_cert && sb.mok_state != "enrolled");

    let br = read_browser_overview();
    ui.set_store_browser_default(SharedString::from(br.default_browser.as_str()));
    ui.set_store_firefox_installed(br.firefox_installed);
    ui.set_store_chromium_installed(br.chromium_installed);

    let pr = read_printers_overview();
    ui.set_set_cups_active(pr.service_active);
    ui.set_set_cups_summary(SharedString::from(pr.summary.as_str()));
    let printer_entries: Vec<PrinterEntry> = pr
        .printers
        .iter()
        .map(|p| PrinterEntry {
            name: SharedString::from(p.name.as_str()),
            status: SharedString::from(p.status.as_str()),
            is_default: p.is_default,
        })
        .collect();
    ui.set_set_printer_list(ModelRc::new(VecModel::from(printer_entries)));
    if let Some(first) = pr.printers.first() {
        ui.set_set_selected_printer_idx(0);
        ui.set_set_selected_printer_name(SharedString::from(first.name.as_str()));
    } else {
        ui.set_set_selected_printer_idx(-1);
        ui.set_set_selected_printer_name(SharedString::new());
    }
    state.printers = pr.printers;

    let fw = read_firewall_overview();
    ui.set_set_fw_enabled(fw.enabled);
    ui.set_set_fw_profile_idx(if fw.profile == "public" { 1 } else { 0 });
    ui.set_set_fw_ssh(fw.allow_ssh);
    ui.set_set_fw_summary(SharedString::from(fw.summary.as_str()));

    let vpns = list_vpn_connections();
    let vpn_entries: Vec<VpnEntry> = vpns
        .iter()
        .map(|v| VpnEntry {
            name: SharedString::from(v.name.as_str()),
            vpn_type: SharedString::from(v.vpn_type.as_str()),
            active: v.active,
        })
        .collect();
    ui.set_set_vpn_list(ModelRc::new(VecModel::from(vpn_entries)));
    if let Some(first) = vpns.first() {
        ui.set_set_selected_vpn_idx(0);
        ui.set_set_selected_vpn_name(SharedString::from(first.name.as_str()));
    } else {
        ui.set_set_selected_vpn_idx(-1);
        ui.set_set_selected_vpn_name(SharedString::new());
    }
    state.vpns = vpns;

    let targets = list_backup_targets();
    let target_labels: Vec<SharedString> = targets
        .iter()
        .map(|t| SharedString::from(t.label.as_str()))
        .collect();
    ui.set_set_backup_targets(ModelRc::new(VecModel::from(target_labels)));
    ui.set_set_backup_target_idx(0);
    state.backup_targets = targets;

    let pwr = read_power_overview();
    let prof_idx = POWER_PROFILES
        .iter()
        .position(|&p| p == pwr.profile)
        .unwrap_or(1) as i32;
    ui.set_pwr_profile_idx(prof_idx);
    ui.set_pwr_screen_min(pwr.screen_min as f32);
    ui.set_pwr_lock_min(pwr.lock_min as f32);
    ui.set_pwr_suspend_bat_min(pwr.suspend_bat_min as f32);
    ui.set_pwr_battery_label(SharedString::from(format!(
        "{} ({})",
        pwr.battery,
        if pwr.on_ac { "AC" } else { "Battery" }
    )));

    let users = read_users_list();
    let user_rows: Vec<UserRow> = users
        .iter()
        .map(|u| UserRow {
            username: SharedString::from(u.username.as_str()),
            uid: u.uid as i32,
            role: SharedString::from(u.role.as_str()),
            has_password: u.has_password,
        })
        .collect();
    ui.set_user_list(ModelRc::new(VecModel::from(user_rows)));
    if let Some(first) = users.first() {
        ui.set_selected_user_idx(0);
        ui.set_selected_user_name(SharedString::from(first.username.as_str()));
    } else {
        ui.set_selected_user_idx(-1);
        ui.set_selected_user_name(SharedString::new());
    }
    let login_names: Vec<SharedString> = users
        .iter()
        .map(|u| SharedString::from(u.username.as_str()))
        .collect();
    if !login_names.is_empty() {
        ui.set_login_user_names(ModelRc::new(VecModel::from(login_names)));
        ui.set_login_user_idx(0);
    }
    state.users = users;
}

fn refresh_ecosystem(ui: &MainWindow) {
    let phone = read_phone_overview();
    ui.set_eco_kde_installed(phone.kdeconnect_installed);
    ui.set_eco_kde_running(phone.kdeconnect_running);
    ui.set_eco_kde_autostart(phone.kdeconnect_autostart);
    ui.set_eco_scrcpy_installed(phone.scrcpy_installed);
    let adb_str = if phone.adb_devices.is_empty() {
        "No USB/Wi-Fi Android device detected".to_string()
    } else {
        phone.adb_devices.join(", ")
    };
    ui.set_eco_adb_summary(SharedString::from(adb_str));
    let kde_rows: Vec<KdeDeviceRow> = phone
        .kde_devices
        .iter()
        .map(|d| KdeDeviceRow {
            id: SharedString::from(d.id.as_str()),
            name: SharedString::from(d.name.as_str()),
            paired: d.paired,
            reachable: d.reachable,
        })
        .collect();
    if let Some(first) = phone.kde_devices.first() {
        ui.set_eco_selected_phone_idx(0);
        ui.set_eco_selected_phone_id(SharedString::from(first.id.as_str()));
    } else {
        ui.set_eco_selected_phone_idx(-1);
        ui.set_eco_selected_phone_id(SharedString::new());
    }
    ui.set_eco_kde_devices(ModelRc::new(VecModel::from(kde_rows)));

    let cloud = read_cloud_overview();
    ui.set_eco_rclone_installed(cloud.rclone_installed);
    ui.set_eco_cloud_mount_base(SharedString::from(cloud.mount_base.as_str()));
    let cloud_rows: Vec<CloudRemoteRow> = cloud
        .remotes
        .iter()
        .map(|r| CloudRemoteRow {
            name: SharedString::from(r.name.as_str()),
            provider_type: SharedString::from(r.provider_type.as_str()),
            mounted: r.mounted,
            autostart: r.autostart,
            mount_dir: SharedString::from(r.mount_dir.as_str()),
        })
        .collect();
    if let Some(first) = cloud.remotes.first() {
        ui.set_eco_selected_cloud_idx(0);
        ui.set_eco_selected_cloud_name(SharedString::from(first.name.as_str()));
    } else {
        ui.set_eco_selected_cloud_idx(-1);
        ui.set_eco_selected_cloud_name(SharedString::new());
    }
    ui.set_eco_cloud_remotes(ModelRc::new(VecModel::from(cloud_rows)));

    let hw = read_hw_overview();
    ui.set_eco_hw_gpu(SharedString::from(format!("{} (driver: {})", hw.gpu_model, hw.gpu_driver)));
    ui.set_eco_hw_vulkan(SharedString::from(if hw.vulkan_ready {
        format!("Ready — {}", hw.vulkan_device)
    } else {
        hw.vulkan_device.clone()
    }));
    ui.set_eco_hw_wifi(SharedString::from(format!("{} (driver: {})", hw.wifi_model, hw.wifi_driver)));
    ui.set_eco_hw_audio(SharedString::from(hw.audio_model.as_str()));
    ui.set_eco_hw_usb(SharedString::from(format!("{} device(s): {}", hw.usb_count, hw.usb_summary)));
    ui.set_eco_hw_missing_fw(SharedString::from(hw.missing_firmware.as_str()));
    ui.set_eco_hw_gpu_mode_idx(gpu_mode_to_index(&hw.gpu_mode));
    ui.set_eco_hw_wifi_ps(hw.wifi_powersave != "off");
    ui.set_eco_hw_fwupd_ok(hw.fwupd_available);
}

fn refresh_gaming_store_banners(ui: &MainWindow) {
    let fp = read_flatpak_overview();
    let fp_summary = if fp.installed {
        format!(
            "Flatpak ({}, {} app(s) in {})",
            if fp.flathub_ready { "Flathub active" } else { "Flathub on-demand" },
            fp.app_count,
            fp.flatpak_dir
        )
    } else {
        "Flatpak (Flathub) on-demand in /data/flatpak".to_string()
    };
    ui.set_store_flatpak_summary(SharedString::from(fp_summary));

    let pr = read_proton_overview();
    let pr_summary = if pr.installed_count > 0 {
        format!(
            "GE-Proton: {} installed ({})",
            pr.installed_count,
            pr.versions.join(", ")
        )
    } else {
        "GE-Proton: Not installed yet (click Install Latest)".to_string()
    };
    ui.set_store_proton_summary(SharedString::from(pr_summary));
}

fn refresh_dev_ai(ui: &MainWindow, state: &mut SettingsUiState) {
    let ai = read_ai_overview();
    ui.set_ai_installed(ai.installed);
    ui.set_ai_running(ai.running);
    ui.set_ai_endpoint(SharedString::from(ai.endpoint.as_str()));
    ui.set_ai_recommended_model(SharedString::from(ai.recommended_model.as_str()));
    ui.set_ai_models_summary(SharedString::from(if ai.models.is_empty() {
        "None downloaded yet".to_string()
    } else {
        ai.models.join(", ")
    }));
    let prov_idx = match ai.provider.as_str() {
        "gemini" => 1,
        "openai" => 2,
        _ => 0,
    };
    ui.set_ai_provider_idx(prov_idx);
    if !ai.active_model.is_empty() {
        ui.set_ai_model_input(SharedString::from(ai.active_model.as_str()));
    }
    if !ai.base_url.is_empty() {
        ui.set_ai_base_url_input(SharedString::from(ai.base_url.as_str()));
    }
    ui.set_ai_api_key_set(ai.api_key_set);

    let dev = read_devbox_overview();
    ui.set_dev_tools_summary(SharedString::from(format!(
        "{} | {} | {}",
        dev.git_version, dev.python_version, dev.podman_version
    )));
    ui.set_dev_projects_dir(SharedString::from(dev.projects_dir.as_str()));
    let box_labels: Vec<SharedString> = dev
        .containers
        .iter()
        .map(|c| SharedString::from(c.as_str()))
        .collect();
    ui.set_dev_containers(ModelRc::new(VecModel::from(box_labels)));
    ui.set_selected_dev_idx(if dev.containers.is_empty() { -1 } else { 0 });
    state.dev_containers = dev.containers;
}

fn load_picker_dir(ui: &MainWindow, state: &mut SettingsUiState, dir: &Path) {
    match list_directory_entries(dir) {
        Ok((canon, entries)) => {
            let rows: Vec<PickerRow> = entries
                .iter()
                .map(|e| PickerRow {
                    name: SharedString::from(e.name.as_str()),
                    path: SharedString::from(e.path.as_str()),
                    is_dir: e.is_dir,
                    detail: SharedString::from(e.detail.as_str()),
                })
                .collect();
            ui.set_picker_dir(SharedString::from(canon.display().to_string()));
            ui.set_picker_entries(ModelRc::new(VecModel::from(rows)));
            state.picker_dir = canon;
            state.picker_entries = entries;
        }
        Err(err) => set_status(ui, err, true),
    }
}

fn set_status(ui: &MainWindow, msg: impl Into<String>, is_error: bool) {
    ui.set_status_message(SharedString::from(msg.into()));
    ui.set_status_is_error(is_error);
}

fn report_result(ui: &MainWindow, res: Result<(), String>, ok_msg: impl Into<String>) {
    match res {
        Ok(()) => set_status(ui, ok_msg, false),
        Err(err) => set_status(ui, err, true),
    }
}

fn app_to_entry(app: &AppSandbox) -> AppEntry {
    AppEntry {
        name: SharedString::from(app.name.as_str()),
        platform_label: SharedString::from(platform_display_name(app.platform)),
    }
}

fn apply_selected_app(ui: &MainWindow, app: Option<&AppSandbox>, idx: i32) {
    ui.set_selected_app_idx(idx);
    ui.set_sel_log_visible(false);
    ui.set_sel_log_text(SharedString::new());
    match app {
        Some(a) => {
            ui.set_has_selected_app(true);
            ui.set_sel_name(SharedString::from(a.name.as_str()));
            ui.set_sel_app_id(SharedString::from(a.app_id.as_str()));
            ui.set_sel_platform(SharedString::from(platform_display_name(a.platform)));
            ui.set_sel_arch(SharedString::from(a.arch.as_str()));
            ui.set_sel_source_path(SharedString::from(a.source_path.as_str()));
            ui.set_sel_sandbox_dir(SharedString::from(a.sandbox_dir.as_str()));
            ui.set_sel_supports_era(supports_win_era(a.platform));
            ui.set_sel_win_era_idx(win_era_to_index(a.win_era));
            ui.set_sel_perm_network(a.permissions.network);
            ui.set_sel_perm_gpu(a.permissions.gpu_acceleration);
            ui.set_sel_perm_documents(a.permissions.shared_documents);
            ui.set_sel_perm_camera(a.permissions.camera);
            ui.set_sel_perm_microphone(a.permissions.microphone);
            ui.set_sel_launch_args(SharedString::from(a.launch_args.join(" ")));
            ui.set_sel_env_vars(SharedString::from(format_env_vars_str(&a.env_vars)));
            let retro = read_retro_profile_from_env(&a.env_vars, a.win_era);
            ui.set_sel_retro_preset_idx(retro.preset.to_index());
            ui.set_sel_retro_fps_cap(retro.fps_cap_60);
            ui.set_sel_retro_single_core(retro.single_core_cpu);
            ui.set_sel_retro_vdesk_idx(virtual_desktop_to_index(&retro.virtual_desktop));
            ui.set_sel_retro_cd_iso(SharedString::from(retro.cd_iso_path.as_str()));
        }
        None => {
            ui.set_has_selected_app(false);
            ui.set_sel_name(SharedString::from("No application selected"));
            ui.set_sel_app_id(SharedString::from(""));
            ui.set_sel_platform(SharedString::from("-"));
            ui.set_sel_arch(SharedString::from("-"));
            ui.set_sel_source_path(SharedString::from("-"));
            ui.set_sel_sandbox_dir(SharedString::from("-"));
            ui.set_sel_supports_era(false);
            ui.set_sel_launch_args(SharedString::new());
            ui.set_sel_env_vars(SharedString::new());
        }
    }
}

fn refresh_apps_and_browse(
    ui: &MainWindow,
    backend: &CenterBackend,
    apps_cache: &Rc<RefCell<Vec<AppSandbox>>>,
    prefer_app_id: Option<&str>,
) {
    match backend.list_apps() {
        Ok(apps) => {
            let entries: Vec<AppEntry> = apps.iter().map(app_to_entry).collect();
            let model = Rc::new(VecModel::from(entries));
            ui.set_installed_apps(ModelRc::from(model));

            if apps.is_empty() {
                apply_selected_app(ui, None, -1);
            } else {
                let target_idx = prefer_app_id
                    .and_then(|id| apps.iter().position(|a| a.app_id == id))
                    .unwrap_or(0);
                apply_selected_app(ui, Some(&apps[target_idx]), target_idx as i32);
            }
            *apps_cache.borrow_mut() = apps;
        }
        Err(err) => {
            set_status(ui, err, true);
        }
    }

    let browse: Vec<BrowseFileEntry> = backend
        .browse_candidates()
        .into_iter()
        .map(|item| BrowseFileEntry {
            name: SharedString::from(item.name),
            path: SharedString::from(item.path.display().to_string()),
            kind: SharedString::from(item.kind),
        })
        .collect();
    ui.set_browse_files(ModelRc::from(Rc::new(VecModel::from(browse))));
}

fn store_item_to_entry(item: StoreItemView) -> StoreEntry {
    let (r, g, b) = item.accent_rgb;
    StoreEntry {
        id: SharedString::from(item.id),
        name: SharedString::from(item.name),
        version: SharedString::from(item.version),
        size_label: SharedString::from(item.size_label),
        platform_label: SharedString::from(item.platform_label),
        platform_short: SharedString::from(item.platform_short),
        summary: SharedString::from(item.summary),
        badge: SharedString::from(if item.is_offline {
            "Built-in Offline"
        } else if item.is_flatpak {
            "Flathub Signed"
        } else {
            "SHA-256 Verified"
        }),
        sha256_short: SharedString::from(item.sha256_short),
        category: SharedString::from(item.category),
        icon_text: SharedString::from(item.icon_text),
        accent_color: slint::Color::from_rgb_u8(r, g, b),
        is_flatpak: item.is_flatpak,
        flatpak_id: SharedString::from(item.flatpak_id),
        is_installed: item.is_installed,
        installed_app_id: SharedString::from(item.installed_app_id),
    }
}

fn refresh_store_list(
    ui: &MainWindow,
    backend: &CenterBackend,
    apps: &[AppSandbox],
    lang: UiLang,
) {
    let cat = ui.get_store_category().to_string();
    let query = ui.get_store_search_query().to_string();
    let is_ms = matches!(lang, UiLang::Ms);
    if let Ok(items) = load_store_views_for_arch(
        apps,
        &backend.config.state_dir,
        &cat,
        &query,
        is_ms,
        &backend.config.host_arch,
        None,
    ) {
        let entries: Vec<StoreEntry> = items.into_iter().map(store_item_to_entry).collect();
        let rows: Vec<StoreRow> = chunk_into_triples(&entries)
            .into_iter()
            .map(|(a, b, c)| StoreRow {
                has_a: a.is_some(),
                a: a.unwrap_or_default(),
                has_b: b.is_some(),
                b: b.unwrap_or_default(),
                has_c: c.is_some(),
                c: c.unwrap_or_default(),
            })
            .collect();

        if let Some(featured) = entries
            .iter()
            .find(|e| e.id == "steam")
            .or_else(|| entries.iter().find(|e| e.id == "antigravity"))
            .or_else(|| entries.iter().find(|e| e.id == "notepadpp"))
            .or_else(|| entries.first())
            .cloned()
        {
            ui.set_store_has_featured(true);
            ui.set_store_featured(featured);
        } else {
            ui.set_store_has_featured(false);
        }

        if ui.get_store_detail_open() {
            let current_detail_id = ui.get_store_detail().id;
            if let Some(updated) = entries.iter().find(|e| e.id == current_detail_id).cloned() {
                ui.set_store_detail(updated);
            }
        }

        ui.set_store_rows(ModelRc::new(VecModel::from(rows)));
        ui.set_store_entries(ModelRc::new(VecModel::from(entries)));
    }
}

fn refresh_system_info(ui: &MainWindow, backend: &CenterBackend, lang: UiLang) {
    let sys = backend.system_overview();
    let hw = &sys.hw;
    let specs = &hw.specs;
    let tier = hw.tier.as_str();
    ui.set_hw_summary(SharedString::from(friendly_hw_summary(
        tier,
        specs.ram_mb,
        lang,
    )));
    ui.set_sys_hw_tier(SharedString::from(format!(
        "{tier} (lightweight_shell={}, idle_hibernate={}s)",
        hw.lightweight_shell, hw.idle_hibernate_sec
    )));
    ui.set_sys_hw_cpu(SharedString::from(format!(
        "{} ({} core{})",
        specs.cpu_model,
        specs.cpu_cores,
        if specs.cpu_cores == 1 { "" } else { "s" }
    )));
    ui.set_sys_hw_ram(SharedString::from(format!(
        "{} MB RAM | {} MB zram (swappiness={})",
        specs.ram_mb, hw.zram_mb, hw.swappiness
    )));
    ui.set_sys_hw_caps(SharedString::from(format!(
        "SSE4.2={} | AVX={} | DRM3D={} | Vulkan={} | FW={}",
        specs.has_sse42, specs.has_avx, specs.has_drm_3d, specs.has_vulkan, specs.firmware
    )));
    ui.set_sys_hw_renderers(SharedString::from(format!(
        "Wayland={} | Win32={} | Android={}",
        hw.wlr_renderer, hw.win32_backend_mode, hw.android_graphics
    )));

    let a = &sys.android;
    ui.set_sys_android_binder(SharedString::from(if a.binderfs_ready {
        "Ready (/dev/binderfs mounted)"
    } else {
        "Not mounted"
    }));
    ui.set_sys_android_engine(SharedString::from(if a.waydroid_installed {
        "Waydroid Installed (/usr/bin/waydroid)"
    } else {
        "Missing"
    }));
    ui.set_sys_android_image(SharedString::from(if a.image_ready {
        "Initialized (/data/waydroid/images/system.img)"
    } else {
        "Not initialized yet (click Initialize Android)"
    }));
    ui.set_sys_android_arm(SharedString::from(if a.arm_translator_ready {
        "Installed (libndk_translation / libhoudini)"
    } else {
        "Not installed (x86_64 native APKs ready; ARM requires libndk/houdini)"
    }));
    ui.set_sys_android_container(SharedString::from(if a.container_active {
        "Active"
    } else {
        "Idle / Cold (0 MB RAM used)"
    }));

    let w = &sys.win32_assets;
    ui.set_sys_win32_wine(SharedString::from(if w.wine_installed {
        "Installed (/usr/bin/wine)"
    } else {
        "Not installed (non-x86 or stripped image)"
    }));
    ui.set_sys_win32_dxvk(SharedString::from(if w.dxvk_installed {
        "Provisioned (Vulkan D3D8/9/10/11 active)"
    } else {
        "Optional (click Install DXVK for DirectX 8-11)"
    }));
    ui.set_sys_win32_vkd3d(SharedString::from(if w.vkd3d_installed {
        "Provisioned (Vulkan D3D12 active)"
    } else {
        "Optional (click Install VKD3D for DirectX 12)"
    }));
    ui.set_sys_win32_otvdm(SharedString::from(if w.otvdm_installed {
        "Provisioned (Win16 NE runtime ready)"
    } else {
        "Optional (click Install OTVDM for Win16 NE)"
    }));
    ui.set_sys_win32_cnc_ddraw(SharedString::from(if w.cnc_ddraw_installed {
        "Provisioned (Win98/XP DirectDraw wrapper ready)"
    } else {
        "Optional (click Install cnc-ddraw for Win98 2D games)"
    }));
    ui.set_sys_win32_winetricks(SharedString::from(if w.winetricks_installed {
        "Installed (/usr/bin/winetricks DX9/VC++ helper)"
    } else {
        "Not installed"
    }));
    ui.set_sys_dos_subsystem(SharedString::from(if w.dos_subsystem_ready {
        "Built-in (/usr/libexec/uniload-dos - direct .exe/.com/.bat)"
    } else {
        "Not installed"
    }));
}

fn load_file_into_open_form(ui: &MainWindow, backend: &CenterBackend, path: &Path) -> bool {
    ui.set_install_path_input(SharedString::from(path.display().to_string()));
    match backend.inspect_file(path) {
        Ok(info) => {
            ui.set_open_has_file(true);
            ui.set_open_name(SharedString::from(info.name));
            ui.set_open_path(SharedString::from(info.canonical_path.display().to_string()));
            ui.set_open_platform(SharedString::from(info.platform_label));
            ui.set_open_arch(SharedString::from(info.arch));
            ui.set_open_sub_abis(SharedString::from(info.sub_abis_label));
            ui.set_open_supports_era(info.supports_era);
            ui.set_open_win_era_idx(win_era_to_index(info.win_era));
            ui.set_open_already_installed(info.already_installed_id.is_some());
            ui.set_open_perm_network(info.permissions.network);
            ui.set_open_perm_gpu(info.permissions.gpu_acceleration);
            ui.set_open_perm_documents(info.permissions.shared_documents);
            ui.set_open_perm_camera(info.permissions.camera);
            ui.set_open_perm_microphone(info.permissions.microphone);
            ui.set_open_create_shortcut(true);
            let hint = if info.already_installed_id.is_some() {
                "Already registered in sandbox; review settings or click Save & Launch."
            } else {
                "Inspected cleanly. Configure compatibility and permissions below."
            };
            set_status(ui, hint, false);
            true
        }
        Err(err) => {
            ui.set_open_has_file(false);
            ui.set_open_path(SharedString::from(path.display().to_string()));
            set_status(ui, err, true);
            false
        }
    }
}

fn collect_open_install_request(ui: &MainWindow) -> InstallRequest {
    let era: Option<WinEra> = if ui.get_open_supports_era() {
        Some(win_era_from_index(ui.get_open_win_era_idx()))
    } else {
        None
    };
    InstallRequest {
        app_name: Some(ui.get_open_name().to_string()),
        win_era: era,
        permissions: PermissionProfile {
            network: ui.get_open_perm_network(),
            gpu_acceleration: ui.get_open_perm_gpu(),
            shared_documents: ui.get_open_perm_documents(),
            camera: ui.get_open_perm_camera(),
            microphone: ui.get_open_perm_microphone(),
        },
        create_shortcut: ui.get_open_create_shortcut(),
    }
}

fn run_headless_check(backend: &CenterBackend, file: Option<&Path>) -> Result<(), String> {
    let sys = backend.system_overview();
    let apps = backend.list_apps()?;
    println!(
        "unicore-center check OK: hw_tier={} ram_mb={} registered_apps={}",
        sys.hw.tier.as_str(),
        sys.hw.specs.ram_mb,
        apps.len()
    );
    if let Some(target) = file {
        let info = backend.inspect_file(target)?;
        println!(
            "inspected: name={} platform={} arch={} era={}",
            info.name,
            info.platform.as_str(),
            info.arch,
            info.win_era.map(|e| e.as_str()).unwrap_or("none")
        );
    }
    Ok(())
}

fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match parse_args(&raw_args) {
        Ok(Some(p)) => p,
        Ok(None) => {
            println!("{}", help_text());
            return;
        }
        Err(err) => {
            eprintln!("Error: {err}");
            std::process::exit(2);
        }
    };

    let backend = Rc::new(RefCell::new(CenterBackend::new(parsed.config)));

    if let LaunchMode::CheckOnly(file) = parsed.mode {
        if let Err(err) = run_headless_check(&backend.borrow(), file.as_deref()) {
            eprintln!("Check failed: {err}");
            std::process::exit(1);
        }
        return;
    }

    let ui = match MainWindow::new() {
        Ok(win) => win,
        Err(err) => {
            eprintln!("Failed to initialize UniCore App Center GUI: {err}");
            std::process::exit(1);
        }
    };

    let settings_state = Rc::new(RefCell::new(SettingsUiState::default()));
    let install_disks: Rc<RefCell<Vec<InstallDisk>>> = Rc::new(RefCell::new(Vec::new()));
    let apps_cache: Rc<RefCell<Vec<AppSandbox>>> = Rc::new(RefCell::new(Vec::new()));
    let tasks_cache: Rc<RefCell<Vec<ProcessInfo>>> = Rc::new(RefCell::new(Vec::new()));
    let env_lang = std::env::var("LANG").ok();
    let initial_lang = detect_ui_lang(
        &Path::new(DEFAULT_OOBE_DIR).join("oobe.conf"),
        env_lang.as_deref(),
    );
    let current_lang = Rc::new(RefCell::new(initial_lang));

    let gaming_cfg = load_gaming_config(default_gaming_dir());
    ui.set_gaming_mode_enabled(gaming_cfg.game_mode);
    ui.set_gaming_fps_enabled(gaming_cfg.show_fps);
    ui.set_sys_cpu_governor(SharedString::from(read_host_cpu_governor()));

    let pers_cfg = load_personalize_config(&persistent_display_config_dir());
    ui.invoke_set_theme(pers_cfg.theme == "light");
    ui.invoke_set_high_contrast_theme(pers_cfg.high_contrast);
    ui.set_set_theme_idx(if pers_cfg.theme == "light" { 1 } else { 0 });
    ui.set_set_desktop_layout_idx(desktop_layout_to_index(&pers_cfg.desktop_layout));
    let init_acc = accent_color_to_index(&pers_cfg.accent_color);
    ui.set_set_accent_idx(init_acc);
    ui.invoke_set_accent_color(init_acc);

    let start_apps_cache: Rc<RefCell<Vec<DesktopAppItem>>> = Rc::new(RefCell::new(Vec::new()));
    let win_parts_cache: Rc<RefCell<Vec<WindowsPartition>>> = Rc::new(RefCell::new(Vec::new()));

    {
        let b = backend.borrow();
        apply_ui_language(&ui, initial_lang);
        ui.set_store_category(SharedString::from(parsed.initial_store_category.as_str()));
        refresh_system_info(&ui, &b, initial_lang);
        refresh_apps_and_browse(&ui, &b, &apps_cache, None);
        refresh_store_list(&ui, &b, &apps_cache.borrow(), initial_lang);
        refresh_gaming_store_banners(&ui);
        refresh_boot_and_disks(&ui, &install_disks);
        refresh_update_status(&ui);
        ui.set_active_tab(parsed.initial_tab);
        ui.set_settings_subtab(parsed.initial_settings_subtab);
        match &parsed.mode {
            LaunchMode::OpenDialog(path) => {
                ui.set_open_dialog_only(true);
                ui.set_open_show_advanced(false);
                load_file_into_open_form(&ui, &b, path);
            }
            LaunchMode::PowerDialog => {
                ui.set_power_dialog_only(true);
            }
            LaunchMode::QuickSettings => {
                ui.set_quick_settings_only(true);
                refresh_settings(&ui, &mut settings_state.borrow_mut());
            }
            LaunchMode::LoginGreeter => {
                ui.set_login_dialog_only(true);
                refresh_settings(&ui, &mut settings_state.borrow_mut());
            }
            LaunchMode::OobeWizard => {
                ui.set_oobe_dialog_only(true);
                ui.set_oobe_step(0);
            }
            LaunchMode::TaskManager => {
                ui.set_taskmgr_dialog_only(true);
                refresh_tasks(&ui, &tasks_cache);
            }
            LaunchMode::AiAssistant => {
                ui.set_ai_dialog_only(true);
                refresh_dev_ai(&ui, &mut settings_state.borrow_mut());
            }
            LaunchMode::CaptureTool => {
                ui.set_capture_dialog_only(true);
                refresh_capture_and_notify(&ui);
            }
            LaunchMode::StartMenu => {
                ui.set_start_menu_only(true);
                let uname = std::env::var("USER").unwrap_or_else(|_| "unicore".to_string());
                ui.set_start_user_name(SharedString::from(uname.as_str()));
                let discovered = scan_start_menu_apps(&apps_cache.borrow(), initial_lang);
                refresh_start_menu(&ui, &discovered, "all", "");
                *start_apps_cache.borrow_mut() = discovered;
            }
            LaunchMode::EmojiPicker => {
                ui.set_emoji_dialog_only(true);
                refresh_emoji_grid(&ui, "all", "");
            }
            LaunchMode::RetroWizard(maybe_path) => {
                ui.set_retro_wizard_only(true);
                if let Some(p) = maybe_path {
                    let ps = p.display().to_string();
                    ui.set_retro_src_input(SharedString::from(ps.as_str()));
                    if ps.to_ascii_lowercase().ends_with(".iso") {
                        ui.set_retro_cd_iso_input(SharedString::from(ps.as_str()));
                    }
                }
            }
            LaunchMode::RepairDialog(app_id) => {
                ui.set_repair_dialog_only(true);
                ui.set_repair_app_id(SharedString::from(app_id.as_str()));
                let app_name = b
                    .get_app(app_id)
                    .map(|a| a.name)
                    .unwrap_or_else(|_| app_id.clone());
                ui.set_repair_app_name(SharedString::from(app_name.as_str()));
                let diag = b.diagnose_app_log(app_id);
                let summary = if initial_lang == UiLang::Ms {
                    diag.summary_ms
                } else {
                    diag.summary_en
                };
                ui.set_repair_summary(SharedString::from(summary.as_str()));
                ui.set_repair_verbs(SharedString::from(diag.recommended_verbs.join(" ")));
                ui.set_repair_suggest_2d(diag.suggest_win98_ddraw);
            }
            LaunchMode::RetroTargetPicker(app_id) => {
                ui.set_retro_pick_only(true);
                ui.set_retro_pick_app_id(SharedString::from(app_id.as_str()));
                let app_name = b
                    .get_app(app_id)
                    .map(|a| a.name)
                    .unwrap_or_else(|_| app_id.clone());
                ui.set_retro_pick_app_name(SharedString::from(app_name.as_str()));
                let cands: Vec<SharedString> = b
                    .scan_installed_game_exes(app_id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| SharedString::from(p.display().to_string()))
                    .collect();
                ui.set_retro_pick_candidates(ModelRc::new(VecModel::from(cands)));
                ui.set_retro_pick_selected_idx(0);
            }
            LaunchMode::EcosystemHub(sub) => {
                ui.set_ecosystem_dialog_only(true);
                ui.set_eco_subtab(*sub);
                refresh_ecosystem(&ui);
            }
            LaunchMode::FullCenter | LaunchMode::CheckOnly(_) => {
                if parsed.initial_tab == 3 {
                    refresh_settings(&ui, &mut settings_state.borrow_mut());
                    if parsed.initial_settings_subtab == 7 {
                        refresh_ecosystem(&ui);
                    }
                } else if parsed.initial_tab == 5 {
                    refresh_dev_ai(&ui, &mut settings_state.borrow_mut());
                }
            }
        }
    }

    // Select app in Installed Apps list
    {
        let ui_weak = ui.as_weak();
        let cache = Rc::clone(&apps_cache);
        ui.on_select_app(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            if let Some(app) = cache.borrow().get(idx as usize) {
                apply_selected_app(&ui, Some(app), idx);
            }
        });
    }

    // Change Windows compatibility era for selected app
    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        let cache = Rc::clone(&apps_cache);
        ui.on_selected_era_changed(move |era_idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let app_id = ui.get_sel_app_id().to_string();
            if app_id.is_empty() {
                return;
            }
            let era = win_era_from_index(era_idx);
            match b.borrow().update_win_era(&app_id, era) {
                Ok(updated) => {
                    refresh_apps_and_browse(&ui, &b.borrow(), &cache, Some(&updated.app_id));
                    let _ = b.borrow().create_shortcuts_for_app(&updated);
                    set_status(
                        &ui,
                        format!("Saved {} compatibility for {}", win_era_display_name(era), updated.name),
                        false,
                    );
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    // Toggle sandbox permission for selected app
    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        let cache = Rc::clone(&apps_cache);
        ui.on_selected_permission_toggled(move |perm, allowed| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let app_id = ui.get_sel_app_id().to_string();
            if app_id.is_empty() {
                return;
            }
            match b.borrow().update_permission(&app_id, &perm, allowed) {
                Ok(updated) => {
                    refresh_apps_and_browse(&ui, &b.borrow(), &cache, Some(&updated.app_id));
                    set_status(
                        &ui,
                        format!(
                            "Permission '{}' set to {} for {}",
                            perm,
                            if allowed { "ALLOW" } else { "DENY" },
                            updated.name
                        ),
                        false,
                    );
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    // Launch selected app
    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_launch_selected(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let app_id = ui.get_sel_app_id().to_string();
            let app_name = ui.get_sel_name().to_string();
            if app_id.is_empty() {
                return;
            }
            report_result(
                &ui,
                b.borrow().spawn_app_detached(&app_id),
                format!("Launched {app_name} in isolated sandbox"),
            );
        });
    }

    // Toggle runtime log viewer for selected app
    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_view_log_selected(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            if ui.get_sel_log_visible() {
                ui.set_sel_log_visible(false);
                return;
            }
            let app_id = ui.get_sel_app_id().to_string();
            if app_id.is_empty() {
                return;
            }
            let log_text = b.borrow().read_app_log(&app_id);
            ui.set_sel_log_text(SharedString::from(log_text));
            ui.set_sel_log_visible(true);
        });
    }

    // Export hardware & boot diagnostics to Desktop
    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_export_diagnostics(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            match b.borrow().export_diagnostics() {
                Ok(path) => set_status(&ui, format!("Exported diagnostics to {path}"), false),
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    // Regenerate .desktop shortcuts for selected app
    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_create_shortcut_selected(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let app_id = ui.get_sel_app_id().to_string();
            if app_id.is_empty() {
                return;
            }
            let backend_ref = b.borrow();
            match backend_ref.get_app(&app_id) {
                Ok(app) => match backend_ref.create_shortcuts_for_app(&app) {
                    Ok(paths) => set_status(
                        &ui,
                        format!("Created {} desktop shortcut(s) for {}", paths.len(), app.name),
                        false,
                    ),
                    Err(err) => set_status(&ui, err, true),
                },
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    // Uninstall selected app & remove desktop shortcuts
    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        let cache = Rc::clone(&apps_cache);
        let lang_cell = Rc::clone(&current_lang);
        ui.on_uninstall_selected(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let app_id = ui.get_sel_app_id().to_string();
            if app_id.is_empty() {
                return;
            }
            let backend_ref = b.borrow();
            match backend_ref.uninstall_app(&app_id) {
                Ok((removed, shortcuts)) => {
                    refresh_apps_and_browse(&ui, &backend_ref, &cache, None);
                    refresh_store_list(&ui, &backend_ref, &cache.borrow(), *lang_cell.borrow());
                    set_status(
                        &ui,
                        format!(
                            "Uninstalled '{}' and removed {} shortcut(s)",
                            removed.name, shortcuts
                        ),
                        false,
                    );
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    // Reclaim idle subsystem RAM (wineserver / waydroid container)
    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        let lang_cell = Rc::clone(&current_lang);
        ui.on_reclaim_ram(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let backend_ref = b.borrow();
            match backend_ref.reclaim_subsystems() {
                Ok(_) => {
                    refresh_system_info(&ui, &backend_ref, *lang_cell.borrow());
                    set_status(&ui, "Reclaimed idle Win32/Android subsystem workers", false);
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    // Refresh all state
    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        let disks = Rc::clone(&install_disks);
        let cache = Rc::clone(&apps_cache);
        let lang_cell = Rc::clone(&current_lang);
        ui.on_refresh_all(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let current_id = ui.get_sel_app_id().to_string();
            let prefer = if current_id.is_empty() {
                None
            } else {
                Some(current_id.as_str())
            };
            let backend_ref = b.borrow();
            let lang = *lang_cell.borrow();
            refresh_system_info(&ui, &backend_ref, lang);
            refresh_apps_and_browse(&ui, &backend_ref, &cache, prefer);
            refresh_store_list(&ui, &backend_ref, &cache.borrow(), *lang_cell.borrow());
            refresh_gaming_store_banners(&ui);
            refresh_boot_and_disks(&ui, &disks);
            refresh_update_status(&ui);
            ui.set_sys_cpu_governor(SharedString::from(read_host_cpu_governor()));
        });
    }

    // Inspect path inside Install File tab
    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_inspect_path(move |raw_path| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let trimmed = raw_path.trim().to_string();
            if trimmed.is_empty() {
                set_status(&ui, "Enter a .exe, .com, .bat, .msi, or .apk path to inspect.", true);
                return;
            }
            load_file_into_open_form(&ui, &b.borrow(), Path::new(&trimmed));
        });
    }

    // Install & Launch or Install Only
    let handle_install = |launch_after: bool| {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        let cache = Rc::clone(&apps_cache);
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let path_str = ui.get_open_path().to_string();
            if path_str.is_empty() {
                set_status(&ui, "No application file inspected.", true);
                return;
            }
            let req = collect_open_install_request(&ui);
            let backend_ref = b.borrow();
            match backend_ref.install_file(Path::new(&path_str), &req) {
                Ok(app) => {
                    if launch_after {
                        if let Err(err) = backend_ref.spawn_app_detached(&app.app_id) {
                            refresh_apps_and_browse(&ui, &backend_ref, &cache, Some(&app.app_id));
                            set_status(&ui, err, true);
                            return;
                        }
                    }
                    if ui.get_open_dialog_only() {
                        let _ = slint::quit_event_loop();
                    } else {
                        refresh_apps_and_browse(&ui, &backend_ref, &cache, Some(&app.app_id));
                        ui.set_active_tab(0);
                        let verb = if launch_after {
                            "Installed & launched"
                        } else {
                            "Installed"
                        };
                        set_status(&ui, format!("{verb} '{}'", app.name), false);
                    }
                }
                Err(err) => set_status(&ui, err, true),
            }
        }
    };

    ui.on_install_and_launch_open(handle_install(true));
    ui.on_install_only_open(handle_install(false));

    ui.on_cancel_open(move || {
        let _ = slint::quit_event_loop();
    });

    {
        let ui_weak = ui.as_weak();
        ui.on_switch_to_full_center(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_open_dialog_only(false);
                ui.set_active_tab(0);
            }
        });
    }

    // Android subsystem buttons (graphical background progress for init & setup-arm)
    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_android_action(move |action| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let action_str = action.to_string();
            let profile = match ui.get_android_profile_idx() {
                1 => "microg",
                2 => "gapps",
                _ => "vanilla",
            }
            .to_string();
            if action_str == "ui" {
                report_result(
                    &ui,
                    b.borrow().spawn_android_action(&action_str, &profile),
                    "Launching Android UI...",
                );
                return;
            }
            let cfg = b.borrow().config.clone();
            ui.set_android_busy(true);
            let prog = match action_str.as_str() {
                "init" => format!("Initializing Android ({profile}) into /data/waydroid..."),
                "setup-arm" => "Downloading & installing libndk_translation ARM translator...".to_string(),
                other => format!("Running Android action '{other}'..."),
            };
            ui.set_android_status(SharedString::from(prog.as_str()));
            set_status(&ui, prog, false);
            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = CenterBackend::run_android_setup_blocking(&cfg, &action_str, &profile);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_android_busy(false);
                        match res {
                            Ok(msg) => {
                                ui.set_android_status(SharedString::from(msg.as_str()));
                                set_status(&ui, msg, false);
                                ui.invoke_refresh_all();
                            }
                            Err(err) => {
                                ui.set_android_status(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    // Win32 / Win16 runtime assets buttons
    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_win32_asset_action(move |action| {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(
                &ui,
                b.borrow().spawn_win32_asset_action(&action),
                format!("Started Win32 runtime asset setup '{action}'"),
            );
        });
    }

    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_install_runtimes_selected(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let app_id = ui.get_sel_app_id().to_string();
            if app_id.is_empty() {
                return;
            }
            let cfg = b.borrow().config.clone();
            ui.set_sel_fix_busy(true);
            let msg = format!("Installing classic DX9/VC++ runtimes for '{app_id}'...");
            set_status(&ui, msg, false);
            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = CenterBackend::run_win32_runtimes_blocking(&cfg, &app_id, None);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_sel_fix_busy(false);
                        match res {
                            Ok(ok) => set_status(&ui, ok, false),
                            Err(err) => set_status(&ui, err, true),
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        let lang_cell = Rc::clone(&current_lang);
        ui.on_diagnose_selected_app(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let app_id = ui.get_sel_app_id().to_string();
            if app_id.is_empty() {
                return;
            }
            let diag = b.borrow().diagnose_app_log(&app_id);
            let summary = if *lang_cell.borrow() == UiLang::Ms {
                diag.summary_ms.clone()
            } else {
                diag.summary_en.clone()
            };
            let verbs = diag.recommended_verbs.join(" ");
            ui.set_sel_diag_summary(SharedString::from(summary.as_str()));
            ui.set_sel_diag_verbs(SharedString::from(verbs.as_str()));
            set_status(&ui, summary, diag.issue_detected);
        });
    }

    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_autofix_selected_app(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let app_id = ui.get_sel_app_id().to_string();
            let verbs = ui.get_sel_diag_verbs().trim().to_string();
            if app_id.is_empty() || verbs.is_empty() {
                return;
            }
            let cfg = b.borrow().config.clone();
            ui.set_sel_fix_busy(true);
            let prog = format!("Auto-fixing '{app_id}' with runtimes: {verbs}...");
            ui.set_sel_diag_summary(SharedString::from(prog.as_str()));
            set_status(&ui, prog, false);
            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = CenterBackend::run_win32_runtimes_blocking(&cfg, &app_id, Some(&verbs));
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_sel_fix_busy(false);
                        match res {
                            Ok(ok) => {
                                ui.set_sel_diag_verbs(SharedString::new());
                                ui.set_sel_diag_summary(SharedString::from(ok.as_str()));
                                set_status(&ui, ok, false);
                            }
                            Err(err) => {
                                ui.set_sel_diag_summary(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    // Global Gaming & Performance switches
    {
        let ui_weak = ui.as_weak();
        ui.on_toggle_game_mode(move |enabled| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let cfg = GamingConfig {
                game_mode: enabled,
                show_fps: ui.get_gaming_fps_enabled(),
            };
            if let Err(err) = save_gaming_config(default_gaming_dir(), &cfg) {
                set_status(&ui, err, true);
                return;
            }
            match apply_cpu_governor(enabled) {
                Ok(msg) => {
                    ui.set_sys_cpu_governor(SharedString::from(read_host_cpu_governor()));
                    set_status(&ui, msg, false);
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_toggle_show_fps(move |enabled| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let cfg = GamingConfig {
                game_mode: ui.get_gaming_mode_enabled(),
                show_fps: enabled,
            };
            report_result(
                &ui,
                save_gaming_config(default_gaming_dir(), &cfg),
                if enabled {
                    "FPS overlay (MangoHud / DXVK HUD) enabled for launched games"
                } else {
                    "FPS overlay disabled"
                },
            );
        });
    }

    // First-Boot Setup Wizard (OOBE) callbacks
    {
        let ui_weak = ui.as_weak();
        ui.on_preview_look_oobe(move |theme_idx, layout_idx, accent_idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            ui.invoke_set_theme(theme_idx == 1);
            ui.invoke_set_accent_color(accent_idx);
            ui.set_set_desktop_layout_idx(layout_idx);
            ui.set_set_taskbar_style_idx(if layout_idx == 1 { 0 } else { 1 });
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_complete_oobe(move |open_installer| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let host = match validate_hostname(&ui.get_oobe_hostname()) {
                Ok(h) => h,
                Err(err) => {
                    ui.set_oobe_step(0);
                    set_status(&ui, err, true);
                    return;
                }
            };
            let pass_raw = ui.get_oobe_pass().to_string();
            let pass_confirm = ui.get_oobe_pass_confirm().to_string();
            let validated_pw = match validate_user_password(&pass_raw, &pass_confirm) {
                Ok(p) => p,
                Err(err) => {
                    ui.set_oobe_step(2);
                    set_status(&ui, err, true);
                    return;
                }
            };
            let lang = OOBE_LANGUAGES
                .get(ui.get_oobe_lang_idx() as usize)
                .copied()
                .unwrap_or("en_US.UTF-8")
                .to_string();
            let tz = OOBE_TIMEZONES
                .get(ui.get_oobe_tz_idx() as usize)
                .copied()
                .unwrap_or("UTC")
                .to_string();
            let cfg = OobeSettings {
                language: lang,
                timezone: tz,
                hostname: host,
            };
            if let Err(err) = apply_oobe_on_host(&cfg, validated_pw.as_deref()) {
                set_status(&ui, err, true);
                return;
            }
            // Apply Zorin-style Layout, Theme & Accent chosen in OOBE Step 2
            let cfg_dir = persistent_display_config_dir();
            let mut pers = load_personalize_config(&cfg_dir);
            let layout = desktop_layout_from_index(ui.get_set_desktop_layout_idx());
            let accent = accent_color_from_index(ui.get_set_accent_idx());
            pers.theme = if ui.get_set_theme_idx() == 1 { "light" } else { "dark" }.to_string();
            pers.desktop_layout = layout.to_string();
            pers.taskbar_style = if layout == "win10" { "docked" } else { "floating" }.to_string();
            pers.accent_color = accent.to_string();
            if pers.theme == "light" && pers.wallpaper == "/usr/share/unicore/wallpaper.png" {
                pers.wallpaper = "/usr/share/unicore/wallpaper-light.png".to_string();
            }
            let _ = save_and_apply_personalize(&cfg_dir, &pers);

            let bundle_ids = oobe_bundle_catalog_ids(
                ui.get_oobe_pack_office(),
                ui.get_oobe_pack_creative(),
                ui.get_oobe_pack_gaming(),
            );
            ui.set_oobe_pass(SharedString::new());
            ui.set_oobe_pass_confirm(SharedString::new());
            ui.set_oobe_dialog_only(false);
            if open_installer {
                ui.set_active_tab(2);
                set_status(&ui, "Initial setup complete. Disk Installer ready!", false);
            } else if !bundle_ids.is_empty() {
                ui.set_active_tab(4);
                set_status(
                    &ui,
                    format!(
                        "Welcome to UniCore OS! Selected starter packs ({}) are ready in Store.",
                        bundle_ids.join(", ")
                    ),
                    false,
                );
            } else {
                ui.set_active_tab(0);
                set_status(&ui, "Initial setup complete. Welcome to UniCore OS!", false);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_skip_oobe(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let _ = save_oobe_state(Path::new(DEFAULT_OOBE_DIR), &OobeSettings::default());
            ui.set_oobe_dialog_only(false);
            ui.set_active_tab(0);
            set_status(&ui, "Setup skipped using default settings", false);
        });
    }

    // Language toggle & live OOBE language preview
    {
        let ui_weak = ui.as_weak();
        let lang_cell = Rc::clone(&current_lang);
        let b = Rc::clone(&backend);
        let cache = Rc::clone(&apps_cache);
        let scache = Rc::clone(&start_apps_cache);
        ui.on_toggle_language(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let next = lang_cell.borrow().toggle();
            *lang_cell.borrow_mut() = next;
            apply_ui_language(&ui, next);
            refresh_system_info(&ui, &b.borrow(), next);
            refresh_store_list(&ui, &b.borrow(), &cache.borrow(), next);
            if ui.get_start_menu_only() {
                let discovered = scan_start_menu_apps(&cache.borrow(), next);
                refresh_start_menu(
                    &ui,
                    &discovered,
                    &ui.get_start_category(),
                    &ui.get_start_search_query(),
                );
                *scache.borrow_mut() = discovered;
            }
            if cfg!(target_os = "linux") {
                let _ = persist_ui_lang(Path::new(DEFAULT_OOBE_DIR), next);
            }
            let msg = match next {
                UiLang::En => "Display language switched to English (EN)",
                UiLang::Ms => "Bahasa paparan ditukar kepada Bahasa Melayu (BM)",
            };
            set_status(&ui, msg, false);
        });
    }

    {
        let ui_weak = ui.as_weak();
        let lang_cell = Rc::clone(&current_lang);
        ui.on_oobe_lang_selected(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let next = if idx == 1 { UiLang::Ms } else { UiLang::En };
            *lang_cell.borrow_mut() = next;
            apply_ui_language(&ui, next);
            ui.set_oobe_lang_idx(idx);
        });
    }

    // Task Manager callbacks
    {
        let ui_weak = ui.as_weak();
        let tasks = Rc::clone(&tasks_cache);
        ui.on_refresh_tasks(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            refresh_tasks(&ui, &tasks);
            set_status(&ui, "Task list refreshed", false);
        });
    }

    {
        let ui_weak = ui.as_weak();
        let tasks = Rc::clone(&tasks_cache);
        ui.on_select_task(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            if let Some(proc_info) = tasks.borrow().get(idx as usize) {
                ui.set_selected_task_idx(idx);
                ui.set_selected_task_pid(proc_info.pid as i32);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let tasks = Rc::clone(&tasks_cache);
        ui.on_end_task(move |pid, force| {
            let Some(ui) = ui_weak.upgrade() else { return };
            if pid <= 1 {
                set_status(&ui, "Select a valid user process first", true);
                return;
            }
            report_result(
                &ui,
                terminate_process(pid as u32, force),
                if force {
                    format!("Force-killed PID {pid}")
                } else {
                    format!("Ended task PID {pid}")
                },
            );
            refresh_tasks(&ui, &tasks);
        });
    }

    // Settings & Power callbacks
    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_refresh_settings(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            refresh_settings(&ui, &mut st.borrow_mut());
            set_status(&ui, "Settings refreshed", false);
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_toggle_wifi_radio(move |enabled| {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(
                &ui,
                set_wifi_radio(enabled),
                if enabled { "Wi-Fi enabled" } else { "Wi-Fi disabled" },
            );
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_select_wifi(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let guard = st.borrow();
            if let Some(net) = guard.wifi.get(idx as usize) {
                ui.set_set_selected_wifi_idx(idx);
                ui.set_set_wifi_ssid(SharedString::from(net.ssid.as_str()));
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_connect_wifi(move |ssid, password| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let res = connect_wifi(&ssid, &password);
            if res.is_ok() {
                ui.set_set_wifi_password(SharedString::new());
            }
            report_result(&ui, res, format!("Connected to '{}'", ssid.trim()));
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_disconnect_wifi(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(&ui, disconnect_wifi(), "Disconnected Wi-Fi");
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_forget_wifi(move |ssid| {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(
                &ui,
                forget_wifi(&ssid),
                format!("Forgot saved Wi-Fi '{}'", ssid.trim()),
            );
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_toggle_bluetooth(move |enabled| {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(
                &ui,
                set_bluetooth_enabled(enabled),
                if enabled { "Bluetooth powered on" } else { "Bluetooth stopped" },
            );
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_scan_bluetooth(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(&ui, scan_bluetooth_brief(), "Bluetooth scan complete");
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_select_bt_device(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let guard = st.borrow();
            if let Some(dev) = guard.bt.get(idx as usize) {
                ui.set_set_selected_bt_idx(idx);
                ui.set_set_selected_bt_mac(SharedString::from(dev.mac.as_str()));
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_bt_action(move |action, mac| {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(
                &ui,
                bt_device_action(&action, &mac),
                format!("Bluetooth '{action}' completed for {mac}"),
            );
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_audio_volume_changed(move |pct| {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(&ui, set_audio_volume(pct), format!("Audio volume set to {pct}%"));
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_audio_mute_toggled(move |muted| {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(
                &ui,
                set_audio_mute(muted),
                if muted { "Audio muted" } else { "Audio unmuted" },
            );
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_mic_mute_toggled(move |muted| {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(
                &ui,
                set_mic_mute(muted),
                if muted { "Microphone muted" } else { "Microphone active" },
            );
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_audio_sink_selected(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let sink_info = st.borrow().sinks.get(idx as usize).cloned();
            if let Some(sink) = sink_info {
                report_result(
                    &ui,
                    set_default_sink(&sink.name),
                    format!("Audio output: {}", sink.label),
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_display_output_selected(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let guard = st.borrow();
            set_disp_modes_for_output(&ui, guard.outputs.get(idx as usize));
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_apply_display(move |out_idx, mode_idx, scale_idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let selected = {
                let guard = st.borrow();
                guard.outputs.get(out_idx as usize).map(|o| {
                    let mode = o
                        .modes
                        .get(mode_idx as usize)
                        .cloned()
                        .unwrap_or_else(|| o.current_mode.clone());
                    (o.name.clone(), mode)
                })
            };
            let Some((output, mode)) = selected else {
                set_status(&ui, "No display output available to apply", true);
                return;
            };
            if mode.is_empty() || mode == "Auto" {
                set_status(&ui, "Select a valid resolution/mode first", true);
                return;
            }
            let scale = DISPLAY_SCALES.get(scale_idx as usize).copied().unwrap_or("1.0");
            match apply_and_save_display(&output, &mode, scale) {
                Ok(saved) => set_status(
                    &ui,
                    format!("Applied {output} {mode} @ {scale}x (saved to {})", saved.display()),
                    false,
                ),
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_brightness_changed(move |pct| {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(&ui, set_brightness(pct), format!("Brightness set to {pct}%"));
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_nightlight_changed(move |enabled, temp_k| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let cfg_dir = persistent_display_config_dir();
            match save_and_apply_nightlight(&cfg_dir, enabled, temp_k) {
                Ok(cfg) => {
                    ui.set_set_nightlight_enabled(cfg.enabled);
                    ui.set_set_nightlight_temp(cfg.temp_k as f32);
                    set_status(
                        &ui,
                        if cfg.enabled {
                            format!("Night Light ON ({}K)", cfg.temp_k)
                        } else {
                            "Night Light OFF".to_string()
                        },
                        false,
                    );
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_apply_personalize(
            move |theme_idx, wp_idx, wp_custom, kb_idx, natural_scroll, tap_to_click, speed_tenths, desktop_icons, taskbar_style_idx, layout_idx, accent_idx, high_contrast, large_text| {
                let Some(ui) = ui_weak.upgrade() else { return };
                let theme = if theme_idx == 1 { "light" } else { "dark" }.to_string();
                let wallpaper = WALLPAPER_PRESETS
                    .get(wp_idx as usize)
                    .map(|(_, path)| (*path).to_string())
                    .unwrap_or_else(|| wp_custom.trim().to_string());
                let kb_layout = KB_LAYOUTS
                    .get(kb_idx as usize)
                    .map(|(code, _)| (*code).to_string())
                    .unwrap_or_else(|| "us".to_string());
                let desktop_layout = desktop_layout_from_index(layout_idx).to_string();
                let accent_color = accent_color_from_index(accent_idx).to_string();
                let taskbar_style = if taskbar_style_idx == 1 { "floating" } else { "docked" }.to_string();
                let req = PersonalizeConfig {
                    theme,
                    wallpaper,
                    kb_layout,
                    natural_scroll,
                    tap_to_click,
                    pointer_speed: clamp_pointer_speed((speed_tenths as f32) / 10.0),
                    desktop_icons,
                    taskbar_style,
                    desktop_layout,
                    accent_color,
                    high_contrast,
                    large_text,
                };
                let cfg_dir = persistent_display_config_dir();
                match save_and_apply_personalize(&cfg_dir, &req) {
                    Ok(saved) => {
                        ui.invoke_set_theme(saved.theme == "light");
                        ui.invoke_set_high_contrast_theme(saved.high_contrast);
                        let acc_i = accent_color_to_index(&saved.accent_color);
                        ui.set_set_accent_idx(acc_i);
                        ui.invoke_set_accent_color(acc_i);
                        ui.set_set_desktop_layout_idx(desktop_layout_to_index(&saved.desktop_layout));
                        ui.set_set_wallpaper_custom(SharedString::from(saved.wallpaper.as_str()));
                        ui.set_set_desktop_icons(saved.desktop_icons);
                        ui.set_set_taskbar_style_idx(if saved.taskbar_style == "floating" { 1 } else { 0 });
                        ui.set_set_high_contrast(saved.high_contrast);
                        ui.set_set_large_text(saved.large_text);
                        let msg = format!(
                            "Applied theme={} | layout={} | accent={} | kb={}",
                            saved.theme,
                            saved.desktop_layout,
                            saved.accent_color,
                            saved.kb_layout
                        );
                        ui.set_set_personalize_status(SharedString::from(msg.as_str()));
                        set_status(&ui, msg, false);
                    }
                    Err(err) => {
                        ui.set_set_personalize_status(SharedString::from(err.as_str()));
                        set_status(&ui, err, true);
                    }
                }
            },
        );
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_enroll_secureboot_mok(move |pass| {
            let Some(ui) = ui_weak.upgrade() else { return };
            match enroll_secureboot_mok(&pass) {
                Ok(msg) => {
                    ui.set_set_sb_mok_pass(SharedString::new());
                    ui.set_set_sb_status(SharedString::from(msg.as_str()));
                    set_status(&ui, msg, false);
                }
                Err(err) => {
                    ui.set_set_sb_status(SharedString::from(err.as_str()));
                    set_status(&ui, err, true);
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_toggle_auto_check_updates(move |enabled| {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(
                &ui,
                set_auto_check_updates(enabled),
                if enabled {
                    "Automatic background signed OTA checks enabled (Waybar badge active)"
                } else {
                    "Automatic background OTA checks disabled"
                },
            );
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_toggle_cups(move |enabled| {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(
                &ui,
                set_cups_enabled(enabled),
                if enabled {
                    "CUPS printer service started"
                } else {
                    "CUPS printer service stopped (0 MB idle RAM)"
                },
            );
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_open_document_scanner(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(
                &ui,
                launch_document_scanner(),
                "Launched Document Scanner (simple-scan)",
            );
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_scan_printers(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            match discover_printers() {
                Ok(found) => {
                    if found.is_empty() {
                        ui.set_set_discovered_printers(ModelRc::new(VecModel::from(vec![
                            SharedString::from("No USB/Wi-Fi printers detected (enter IPP URI manually below)"),
                        ])));
                        st.borrow_mut().discovered_printers.clear();
                        set_status(
                            &ui,
                            "Scan finished: no auto-discovered printers (manual IPP URI supported)",
                            false,
                        );
                    } else {
                        let labels: Vec<SharedString> = found
                            .iter()
                            .map(|p| SharedString::from(p.label.as_str()))
                            .collect();
                        ui.set_set_printer_name_input(SharedString::from(
                            found[0].suggested_name.as_str(),
                        ));
                        ui.set_set_printer_uri_input(SharedString::from(found[0].uri.as_str()));
                        ui.set_set_discovered_printers(ModelRc::new(VecModel::from(labels)));
                        ui.set_set_discovered_idx(0);
                        let count = found.len();
                        st.borrow_mut().discovered_printers = found;
                        set_status(&ui, format!("Found {count} printer endpoint(s)"), false);
                    }
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_select_printer(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let guard = st.borrow();
            if let Some(p) = guard.printers.get(idx as usize) {
                ui.set_set_selected_printer_idx(idx);
                ui.set_set_selected_printer_name(SharedString::from(p.name.as_str()));
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_select_discovered_printer(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let guard = st.borrow();
            if let Some(p) = guard.discovered_printers.get(idx as usize) {
                ui.set_set_printer_name_input(SharedString::from(p.suggested_name.as_str()));
                ui.set_set_printer_uri_input(SharedString::from(p.uri.as_str()));
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_add_printer(move |name, uri| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let res = add_printer(&name, &uri);
            report_result(&ui, res, format!("Added printer '{}'", name.trim()));
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_printer_action(move |action, name| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let res = match action.as_str() {
                "default" => set_default_printer(&name),
                "test" => print_test_page(&name),
                "remove" => remove_printer(&name),
                other => Err(format!("Unsupported printer action: {other}")),
            };
            report_result(
                &ui,
                res,
                format!("Printer '{action}' completed for '{}'", name.trim()),
            );
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_toggle_firewall(move |enabled| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let prof = if ui.get_set_fw_profile_idx() == 1 { "public" } else { "home" };
            let ssh = ui.get_set_fw_ssh();
            report_result(
                &ui,
                apply_firewall_settings(enabled, prof, ssh),
                if enabled { "Firewall enabled" } else { "Firewall disabled" },
            );
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_firewall_profile_changed(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let prof = if idx == 1 { "public" } else { "home" };
            let enabled = ui.get_set_fw_enabled();
            let ssh = ui.get_set_fw_ssh();
            report_result(
                &ui,
                apply_firewall_settings(enabled, prof, ssh),
                format!("Firewall profile set to {prof}"),
            );
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_firewall_ssh_toggled(move |ssh| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let enabled = ui.get_set_fw_enabled();
            let prof = if ui.get_set_fw_profile_idx() == 1 { "public" } else { "home" };
            report_result(
                &ui,
                apply_firewall_settings(enabled, prof, ssh),
                if ssh { "Incoming SSH allowed" } else { "Incoming SSH blocked" },
            );
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_select_vpn(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let guard = st.borrow();
            if let Some(vpn) = guard.vpns.get(idx as usize) {
                ui.set_set_selected_vpn_idx(idx);
                ui.set_set_selected_vpn_name(SharedString::from(vpn.name.as_str()));
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_import_vpn(move |path| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let res = import_vpn_profile(&path);
            if res.is_ok() {
                ui.set_set_vpn_import_path(SharedString::new());
            }
            report_result(&ui, res, "VPN profile imported");
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_vpn_action(move |action, name| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let res = match action.as_str() {
                "connect" => toggle_vpn_connection(&name, true),
                "disconnect" => toggle_vpn_connection(&name, false),
                "delete" => delete_vpn_connection(&name),
                other => Err(format!("Unsupported VPN action: {other}")),
            };
            report_result(
                &ui,
                res,
                format!("VPN '{action}' completed for '{}'", name.trim()),
            );
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_refresh_backup_targets(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            refresh_settings(&ui, &mut st.borrow_mut());
            set_status(&ui, "Backup destinations refreshed", false);
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_create_backup(move |target_idx, encrypted, pass| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let dest_dir = st
                .borrow()
                .backup_targets
                .get(target_idx as usize)
                .map(|t| t.path.clone())
                .unwrap_or_else(|| "/data/home/Backups".to_string());
            let pass_opt = if encrypted {
                let p = pass.to_string();
                if let Err(err) = validate_backup_passphrase(&p) {
                    ui.set_set_backup_status(SharedString::from(err.as_str()));
                    set_status(&ui, err, true);
                    return;
                }
                Some(p)
            } else {
                None
            };

            ui.set_set_backup_busy(true);
            let prog = format!("Creating backup in {dest_dir}...");
            ui.set_set_backup_status(SharedString::from(prog.as_str()));
            set_status(&ui, prog, false);

            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = run_backup_create(&dest_dir, pass_opt.as_deref());
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_set_backup_busy(false);
                        match res {
                            Ok(msg) => {
                                ui.set_set_backup_status(SharedString::from(msg.as_str()));
                                set_status(&ui, msg, false);
                            }
                            Err(err) => {
                                ui.set_set_backup_status(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_restore_backup(move |archive_path, pass| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let safe_arc = match validate_backup_path(&archive_path) {
                Ok(p) => p,
                Err(err) => {
                    ui.set_set_backup_status(SharedString::from(err.as_str()));
                    set_status(&ui, err, true);
                    return;
                }
            };
            let pass_opt = if safe_arc.ends_with(".tar.zst.enc") || !pass.is_empty() {
                let p = pass.to_string();
                if let Err(err) = validate_backup_passphrase(&p) {
                    ui.set_set_backup_status(SharedString::from(err.as_str()));
                    set_status(&ui, err, true);
                    return;
                }
                Some(p)
            } else {
                None
            };

            ui.set_set_backup_busy(true);
            let prog = format!("Restoring /data from {safe_arc}...");
            ui.set_set_backup_status(SharedString::from(prog.as_str()));
            set_status(&ui, prog, false);

            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = run_backup_restore(&safe_arc, pass_opt.as_deref());
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_set_backup_busy(false);
                        match res {
                            Ok(msg) => {
                                ui.set_set_backup_status(SharedString::from(msg.as_str()));
                                set_status(&ui, msg, false);
                                ui.invoke_refresh_settings();
                            }
                            Err(err) => {
                                ui.set_set_backup_status(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_power_action(move |action| {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(
                &ui,
                run_power_action(&action),
                format!("Executing power action '{action}'..."),
            );
        });
    }

    // Bare-metal A/B disk installer callbacks
    {
        let ui_weak = ui.as_weak();
        let disks = Rc::clone(&install_disks);
        ui.on_refresh_install_disks(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            refresh_boot_and_disks(&ui, &disks);
            set_status(&ui, "Disk list refreshed", false);
        });
    }

    {
        let ui_weak = ui.as_weak();
        let disks = Rc::clone(&install_disks);
        ui.on_start_disk_install(move |idx, mode_idx, typed, encrypt_data, luks_pass, luks_confirm| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let Some(disk) = disks.borrow().get(idx as usize).cloned() else {
                set_status(&ui, "No eligible target disk selected", true);
                return;
            };
            let alongside = mode_idx == 1;
            if alongside && !disk.dualboot_eligible {
                set_status(
                    &ui,
                    format!(
                        "{} does not have >= 10 GB unallocated GPT space (found {}). Shrink Windows C: in Disk Management first or choose Whole-Disk mode.",
                        disk.device, disk.free_gb
                    ),
                    true,
                );
                return;
            }
            if let Err(err) = validate_install_confirmation(&disk.name, &typed) {
                set_status(&ui, err, true);
                return;
            }
            let luks_pass_str = luks_pass.to_string();
            if encrypt_data {
                if let Err(err) = validate_luks_passphrase(&luks_pass_str, &luks_confirm) {
                    set_status(&ui, err, true);
                    return;
                }
            }

            ui.set_inst_busy(true);
            let progress_msg = match (alongside, encrypt_data) {
                (true, true) => format!(
                    "Safe Dual-Boot: Installing UniCore OS alongside Windows on {} (LUKS2 Encrypted /data)...",
                    disk.device
                ),
                (true, false) => format!(
                    "Safe Dual-Boot: Installing UniCore OS into unallocated space on {} (preserving Windows)...",
                    disk.device
                ),
                (false, true) => format!(
                    "Installing UniCore OS to {} (GPT A/B + LUKS2 Encrypted /data)...",
                    disk.device
                ),
                (false, false) => format!(
                    "Installing UniCore OS to {} (GPT Hybrid A/B + GRUB BIOS/UEFI)...",
                    disk.device
                ),
            };
            ui.set_inst_status(SharedString::from(progress_msg.as_str()));
            set_status(&ui, progress_msg, false);

            let thread_ui = ui.as_weak();
            let dev = disk.device.clone();
            std::thread::spawn(move || {
                let res = match (alongside, encrypt_data) {
                    (true, true) => run_disk_install_alongside_encrypted(&dev, &luks_pass_str),
                    (true, false) => run_disk_install_alongside(&dev),
                    (false, true) => run_disk_install_encrypted(&dev, &luks_pass_str),
                    (false, false) => run_disk_install(&dev),
                };
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_inst_busy(false);
                        match res {
                            Ok(msg) => {
                                ui.set_inst_confirm_input(SharedString::new());
                                ui.set_inst_luks_pass(SharedString::new());
                                ui.set_inst_luks_confirm(SharedString::new());
                                ui.set_inst_status(SharedString::from(msg.as_str()));
                                set_status(&ui, msg, false);
                            }
                            Err(err) => {
                                ui.set_inst_status(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    // A/B OS Updater & Rollback callbacks
    {
        let ui_weak = ui.as_weak();
        ui.on_refresh_update_status(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            refresh_update_status(&ui);
            set_status(&ui, "A/B update status refreshed", false);
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_check_ota_update(move |url| {
            let Some(ui) = ui_weak.upgrade() else { return };
            ui.set_upd_busy(true);
            ui.set_upd_status(SharedString::from("Checking update manifest..."));
            let thread_ui = ui.as_weak();
            let url_str = url.to_string();
            std::thread::spawn(move || {
                let res = check_remote_update(&url_str);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_upd_busy(false);
                        match res {
                            Ok(info) => {
                                let msg = if info.update_available {
                                    format!(
                                        "Update available: {} -> {} (channel: {}, sig: {})",
                                        info.current_version,
                                        info.latest_version,
                                        info.channel,
                                        info.signature_status
                                    )
                                } else {
                                    format!(
                                        "System is up to date ({} >= {}, sig: {})",
                                        info.current_version,
                                        info.latest_version,
                                        info.signature_status
                                    )
                                };
                                ui.set_upd_status(SharedString::from(msg.as_str()));
                                set_status(&ui, msg, false);
                            }
                            Err(err) => {
                                ui.set_upd_status(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    let spawn_update_op = |ui: &MainWindow,
                           progress_msg: &str,
                           needs_reboot_on_ok: bool,
                           op: Box<dyn FnOnce() -> Result<String, String> + Send>| {
        ui.set_upd_busy(true);
        ui.set_upd_status(SharedString::from(progress_msg));
        set_status(ui, progress_msg, false);
        let thread_ui = ui.as_weak();
        std::thread::spawn(move || {
            let res = op();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = thread_ui.upgrade() {
                    ui.set_upd_busy(false);
                    match res {
                        Ok(msg) => {
                            if needs_reboot_on_ok {
                                ui.set_upd_needs_reboot(true);
                            }
                            refresh_update_status(&ui);
                            ui.set_upd_status(SharedString::from(msg.as_str()));
                            set_status(&ui, msg, false);
                        }
                        Err(err) => {
                            ui.set_upd_status(SharedString::from(err.as_str()));
                            set_status(&ui, err, true);
                        }
                    }
                }
            });
        });
    };

    {
        let ui_weak = ui.as_weak();
        ui.on_apply_ota_update(move |url| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let url_str = url.to_string();
            spawn_update_op(
                &ui,
                "Downloading, verifying SHA-256, and flashing standby A/B slot...",
                true,
                Box::new(move || apply_remote_update(&url_str)),
            );
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_apply_local_update(move |path, allow_unsigned| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let path_str = path.to_string();
            spawn_update_op(
                &ui,
                "Verifying and flashing local update into standby A/B slot...",
                true,
                Box::new(move || apply_local_update(&path_str, allow_unsigned)),
            );
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_rollback_os_slot(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            spawn_update_op(
                &ui,
                "Switching GRUB active boot target to standby A/B slot...",
                true,
                Box::new(rollback_update),
            );
        });
    }

    // Built-in 1-Click App Store / Catalog callbacks
    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        let cache = Rc::clone(&apps_cache);
        let lang_cell = Rc::clone(&current_lang);
        ui.on_store_filter_changed(move |cat, query| {
            let Some(ui) = ui_weak.upgrade() else { return };
            ui.set_store_category(cat);
            ui.set_store_search_query(query);
            refresh_store_list(&ui, &b.borrow(), &cache.borrow(), *lang_cell.borrow());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_store_install_app(move |catalog_id| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let id_str = catalog_id.to_string();
            let cfg = b.borrow().config.clone();
            ui.set_store_busy(true);
            let msg = format!("Installing & verifying '{id_str}'...");
            ui.set_store_status(SharedString::from(msg.as_str()));
            set_status(&ui, msg, false);

            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = install_store_app_blocking(&cfg, &id_str);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_store_busy(false);
                        match res {
                            Ok(ok_msg) => {
                                ui.invoke_refresh_all();
                                ui.set_store_status(SharedString::from(ok_msg.as_str()));
                                set_status(&ui, ok_msg, false);
                            }
                            Err(err) => {
                                ui.set_store_status(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_store_uninstall_app(move |catalog_id, installed_id| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let cat_str = catalog_id.to_string();
            let inst_str = installed_id.to_string();
            let cfg = b.borrow().config.clone();
            ui.set_store_busy(true);
            let msg = format!("Uninstalling '{cat_str}'...");
            ui.set_store_status(SharedString::from(msg.as_str()));
            set_status(&ui, msg, false);

            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = uninstall_store_app_blocking(&cfg, &cat_str, &inst_str);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_store_busy(false);
                        match res {
                            Ok(ok_msg) => {
                                ui.invoke_refresh_all();
                                ui.set_store_status(SharedString::from(ok_msg.as_str()));
                                set_status(&ui, ok_msg, false);
                            }
                            Err(err) => {
                                ui.set_store_status(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_store_launch_app(move |app_id, is_flatpak| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let id_str = app_id.to_string();
            if id_str.is_empty() {
                return;
            }
            let res = if is_flatpak {
                launch_flatpak_app(&id_str)
            } else {
                b.borrow().spawn_app_detached(&id_str)
            };
            report_result(
                &ui,
                res,
                format!("Launched '{id_str}' from App Store"),
            );
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_install_latest_proton(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            ui.set_store_proton_busy(true);
            let msg = "Downloading latest GE-Proton release into /data/gaming/proton-ge...".to_string();
            ui.set_store_status(SharedString::from(msg.as_str()));
            set_status(&ui, msg, false);

            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = install_latest_proton_blocking();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_store_proton_busy(false);
                        refresh_gaming_store_banners(&ui);
                        match res {
                            Ok(ok_msg) => {
                                ui.set_store_status(SharedString::from(ok_msg.as_str()));
                                set_status(&ui, ok_msg, false);
                            }
                            Err(err) => {
                                ui.set_store_status(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_store_sync_catalog(move |url| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let url_str = url.trim().to_string();
            ui.set_store_busy(true);
            ui.set_store_status(SharedString::from(
                "Syncing & verifying Ed25519 signed catalog over HTTPS...",
            ));

            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = sync_store_catalog_blocking(&url_str);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_store_busy(false);
                        match res {
                            Ok((count, sig)) => {
                                ui.invoke_refresh_all();
                                let ok_msg = format!(
                                    "Synced catalog ({count} verified apps, signature: {sig})"
                                );
                                ui.set_store_status(SharedString::from(ok_msg.as_str()));
                                set_status(&ui, ok_msg, false);
                            }
                            Err(err) => {
                                ui.set_store_status(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_install_native_browser(move |browser| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let bname = browser.trim().to_string();
            ui.set_store_busy(true);
            let prog = format!("Installing native Wayland '{bname}'...");
            ui.set_store_status(SharedString::from(prog.as_str()));
            set_status(&ui, prog, false);

            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = install_native_browser_blocking(&bname);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_store_busy(false);
                        let br = read_browser_overview();
                        ui.set_store_browser_default(SharedString::from(br.default_browser.as_str()));
                        ui.set_store_firefox_installed(br.firefox_installed);
                        ui.set_store_chromium_installed(br.chromium_installed);
                        match res {
                            Ok(ok) => {
                                ui.set_store_status(SharedString::from(ok.as_str()));
                                set_status(&ui, ok, false);
                            }
                            Err(err) => {
                                ui.set_store_status(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_set_default_browser(move |browser| {
            let Some(ui) = ui_weak.upgrade() else { return };
            match set_default_browser(&browser) {
                Ok(msg) => {
                    ui.set_store_browser_default(browser);
                    ui.set_store_status(SharedString::from(msg.as_str()));
                    set_status(&ui, msg, false);
                }
                Err(err) => {
                    ui.set_store_status(SharedString::from(err.as_str()));
                    set_status(&ui, err, true);
                }
            }
        });
    }

    // Phase 25: App Launch Args / Env Vars / Prefix Reset / Save Backup
    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        let cache = Rc::clone(&apps_cache);
        ui.on_save_app_launch_config(move |args_raw, env_raw| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let app_id = ui.get_sel_app_id().to_string();
            if app_id.is_empty() {
                return;
            }
            match b.borrow().update_launch_config(&app_id, &args_raw, &env_raw) {
                Ok(updated) => {
                    refresh_apps_and_browse(&ui, &b.borrow(), &cache, Some(&updated.app_id));
                    set_status(&ui, format!("Saved launch args/env for '{}'", updated.name), false);
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_reset_app_prefix(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let app_id = ui.get_sel_app_id().to_string();
            if app_id.is_empty() {
                return;
            }
            match b.borrow().reset_win32_prefix(&app_id) {
                Ok(prefix) => set_status(
                    &ui,
                    format!("Reset Wine prefix ({}). It will rebuild on next launch.", prefix.display()),
                    false,
                ),
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_backup_app_save(move |dest| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let app_id = ui.get_sel_app_id().to_string();
            if app_id.is_empty() || dest.trim().is_empty() {
                return;
            }
            match b.borrow().backup_app_save_data(&app_id, Path::new(dest.trim())) {
                Ok(p) => set_status(&ui, format!("Exported app saves to {}", p.display()), false),
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_restore_app_save(move |src| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let app_id = ui.get_sel_app_id().to_string();
            if app_id.is_empty() || src.trim().is_empty() {
                return;
            }
            report_result(
                &ui,
                b.borrow().restore_app_save_data(&app_id, Path::new(src.trim())),
                format!("Restored save archive into '{}'", app_id),
            );
        });
    }

    // Phase 25: Built-in Graphical File Picker Overlay
    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_open_file_picker(move |target| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let start_dir = default_picker_dir();
            ui.set_picker_target(target);
            load_picker_dir(&ui, &mut st.borrow_mut(), &start_dir);
            ui.set_picker_open(true);
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_picker_activate_entry(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let chosen = st.borrow().picker_entries.get(idx as usize).cloned();
            let Some(entry) = chosen else { return };
            if entry.is_dir {
                load_picker_dir(&ui, &mut st.borrow_mut(), Path::new(&entry.path));
                return;
            }
            match ui.get_picker_target().as_str() {
                "install" => ui.set_install_path_input(SharedString::from(entry.path.as_str())),
                "wallpaper" => ui.set_set_wallpaper_custom(SharedString::from(entry.path.as_str())),
                "vpn" => ui.set_set_vpn_import_path(SharedString::from(entry.path.as_str())),
                "backup-restore" => {
                    ui.set_set_backup_restore_path(SharedString::from(entry.path.as_str()))
                }
                "retro-src" => {
                    ui.set_retro_src_input(SharedString::from(entry.path.as_str()));
                    if entry.path.to_ascii_lowercase().ends_with(".iso") {
                        ui.set_retro_cd_iso_input(SharedString::from(entry.path.as_str()));
                    }
                }
                "retro-iso" => {
                    ui.set_retro_cd_iso_input(SharedString::from(entry.path.as_str()))
                }
                "sel-retro-iso" => {
                    ui.set_sel_retro_cd_iso(SharedString::from(entry.path.as_str()))
                }
                _ => {}
            }
            ui.set_picker_open(false);
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_picker_use_current_dir(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            if ui.get_picker_target().as_str() == "retro-src" {
                let dir_str = st.borrow().picker_dir.display().to_string();
                ui.set_retro_src_input(SharedString::from(dir_str.as_str()));
            }
            ui.set_picker_open(false);
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_picker_cancel(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            ui.set_picker_open(false);
        });
    }

    // Phase 23: Login Greeter, Power Profiles, Swayidle Timers, S.M.A.R.T. & Multi-User Accounts
    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_attempt_login(move |user_idx, pass| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let username = st
                .borrow()
                .users
                .get(user_idx as usize)
                .map(|u| u.username.clone())
                .unwrap_or_else(|| "unicore".to_string());
            match authenticate_user(&username, &pass) {
                Ok(()) => {
                    let _ = std::fs::create_dir_all("/run/unicore");
                    let _ = std::fs::write("/run/unicore/active-user", format!("{username}\n"));
                    let _ = slint::quit_event_loop();
                }
                Err(err) => {
                    ui.set_login_error(SharedString::from(format!("Sign-in failed: {err}")));
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_set_power_profile(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let prof = POWER_PROFILES.get(idx as usize).copied().unwrap_or("balanced");
            report_result(
                &ui,
                set_power_profile(prof).map(|_| ()),
                format!("Power profile set to {prof}"),
            );
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_apply_idle_timers(move |scr, lck, susp| {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(
                &ui,
                set_idle_timers(scr, lck, susp),
                format!("Idle timers applied (screen {scr}m, lock {lck}m, battery sleep {susp}m)"),
            );
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_scan_smart_health(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let h = read_smart_health(None);
            ui.set_pwr_smart_device(SharedString::from(h.device.as_str()));
            ui.set_pwr_smart_model(SharedString::from(h.model.as_str()));
            ui.set_pwr_smart_health(SharedString::from(h.health.as_str()));
            ui.set_pwr_smart_temp(SharedString::from(h.temp_c.as_str()));
            ui.set_pwr_smart_hours(SharedString::from(h.power_on_hours.as_str()));
            set_status(&ui, format!("Disk S.M.A.R.T. scan complete ({})", h.device), false);
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_select_user(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let guard = st.borrow();
            if let Some(u) = guard.users.get(idx as usize) {
                ui.set_selected_user_idx(idx);
                ui.set_selected_user_name(SharedString::from(u.username.as_str()));
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_add_user_account(move |name, role_idx, pass| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let role = if role_idx == 1 { "admin" } else { "standard" };
            let res = add_user_account(&name, role, &pass);
            if res.is_ok() {
                ui.set_new_user_name(SharedString::new());
                ui.set_new_user_pass(SharedString::new());
            }
            report_result(&ui, res.map(|_| ()), format!("Added user '{}'", name.trim()));
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_delete_selected_user(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let name = ui.get_selected_user_name().to_string();
            let res = delete_user_account(&name);
            report_result(&ui, res.map(|_| ()), format!("Removed user '{name}'"));
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_set_selected_user_password(move |pass| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let name = ui.get_selected_user_name().to_string();
            let res = set_account_password(&name, &pass);
            if res.is_ok() {
                ui.set_update_user_pass(SharedString::new());
            }
            report_result(&ui, res.map(|_| ()), format!("Updated password for '{name}'"));
            refresh_settings(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_quick_toggle_theme(move |light| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let cfg_dir = persistent_display_config_dir();
            let mut cur = load_personalize_config(&cfg_dir);
            cur.theme = if light { "light" } else { "dark" }.to_string();
            match save_and_apply_personalize(&cfg_dir, &cur) {
                Ok(saved) => ui.invoke_set_theme(saved.theme == "light"),
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    // Phase 24: DevBox (Rootless Podman) & Local AI (Ollama)
    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_refresh_dev_ai(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            refresh_dev_ai(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_select_devbox(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            if idx >= 0 && (idx as usize) < st.borrow().dev_containers.len() {
                ui.set_selected_dev_idx(idx);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_create_devbox(move |tpl_idx, name| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let tpl = DEVBOX_TEMPLATES.get(tpl_idx as usize).copied().unwrap_or("python");
            report_result(
                &ui,
                run_devbox_create(tpl, &name).map(|_| ()),
                format!("Created DevBox '{}' ({tpl})", name.trim()),
            );
            refresh_dev_ai(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_open_devbox_shell(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let idx = ui.get_selected_dev_idx();
            let box_name = st
                .borrow()
                .dev_containers
                .get(idx as usize)
                .and_then(|s| s.split('|').next())
                .map(str::trim)
                .unwrap_or("")
                .to_string();
            if box_name.is_empty() {
                return;
            }
            report_result(
                &ui,
                spawn_devbox_shell(&box_name),
                format!("Opened DevBox shell for '{box_name}'"),
            );
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_remove_devbox(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let idx = ui.get_selected_dev_idx();
            let box_name = st
                .borrow()
                .dev_containers
                .get(idx as usize)
                .and_then(|s| s.split('|').next())
                .map(str::trim)
                .unwrap_or("")
                .to_string();
            if box_name.is_empty() {
                return;
            }
            report_result(
                &ui,
                remove_devbox(&box_name).map(|_| ()),
                format!("Removed DevBox '{box_name}'"),
            );
            refresh_dev_ai(&ui, &mut st.borrow_mut());
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_ai_action(move |action, model| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let act = action.to_string();
            let mdl = model.trim().to_string();
            if act == "pull" {
                if let Err(err) = validate_ai_model_name(&mdl) {
                    set_status(&ui, err, true);
                    return;
                }
            }
            if matches!(act.as_str(), "start" | "stop") {
                let res = run_ai_action(&[act.as_str()]);
                report_result(
                    &ui,
                    res.map(|_| ()),
                    if act == "start" { "Local AI server started" } else { "Local AI server stopped" },
                );
                refresh_dev_ai(&ui, &mut st.borrow_mut());
                return;
            }

            ui.set_ai_busy(true);
            let prog = if act == "pull" {
                format!("Pulling model '{mdl}'...")
            } else {
                "Downloading Ollama engine into /data/ai...".to_string()
            };
            set_status(&ui, prog, false);
            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = if act == "pull" {
                    run_ai_action(&["pull", &mdl])
                } else {
                    run_ai_action(&["install"])
                };
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_ai_busy(false);
                        match res {
                            Ok(msg) => set_status(&ui, if msg.is_empty() { "Local AI action complete".to_string() } else { msg }, false),
                            Err(err) => set_status(&ui, err, true),
                        }
                        ui.invoke_refresh_dev_ai();
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_ai_ask(move |model, prompt| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let mdl = model.trim().to_string();
            let prm = prompt.trim().to_string();
            if let Err(err) = validate_ai_model_name(&mdl) {
                set_status(&ui, err, true);
                return;
            }
            if prm.is_empty() {
                return;
            }
            ui.set_ai_busy(true);
            set_status(&ui, format!("Querying local model '{mdl}'..."), false);
            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = run_ai_action(&["chat", &mdl, &prm]);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_ai_busy(false);
                        match res {
                            Ok(reply) => {
                                ui.set_ai_response_text(SharedString::from(reply.as_str()));
                                set_status(&ui, "Local AI response received", false);
                            }
                            Err(err) => set_status(&ui, err, true),
                        }
                    }
                });
            });
        });
    }

    // Multi-provider AI Copilot & Lock Screen callbacks (v0.4.0)
    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_ai_save_provider(move |provider_idx, model, base_url, api_key| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let provider = match provider_idx {
                1 => "gemini",
                2 => "openai",
                _ => "ollama",
            };
            let default_model = match provider_idx {
                1 => "gemini-2.5-flash",
                2 => "gpt-4o-mini",
                _ => "llama3.2:1b",
            };
            let model_clean = if model.trim().is_empty() {
                default_model.to_string()
            } else {
                model.trim().to_string()
            };
            match save_ai_provider(provider, &model_clean, base_url.trim(), api_key.trim()) {
                Ok(msg) => {
                    ui.set_ai_api_key_input(SharedString::new());
                    refresh_dev_ai(&ui, &mut st.borrow_mut());
                    set_status(&ui, msg, false);
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_ai_copilot_ask(move |prompt, include_ctx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let prm = prompt.trim().to_string();
            if prm.is_empty() {
                return;
            }
            ui.set_ai_busy(true);
            set_status(&ui, "Asking UniCore AI Assistant...", false);
            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = run_ai_copilot(&prm, include_ctx);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_ai_busy(false);
                        match res {
                            Ok(reply) => {
                                ui.set_ai_response_text(SharedString::from(reply.as_str()));
                                set_status(&ui, "AI Assistant replied", false);
                            }
                            Err(err) => {
                                ui.set_ai_response_text(SharedString::from(format!("Error: {err}")));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_ai_quick_nav(move |target| {
            let Some(ui) = ui_weak.upgrade() else { return };
            ui.set_ai_dialog_only(false);
            match target.as_str() {
                "wifi" => {
                    ui.set_active_tab(3);
                    ui.set_settings_subtab(0);
                    refresh_settings(&ui, &mut st.borrow_mut());
                }
                "power" => {
                    ui.set_active_tab(3);
                    ui.set_settings_subtab(6);
                    refresh_settings(&ui, &mut st.borrow_mut());
                }
                "store" => {
                    ui.set_active_tab(4);
                }
                _ => {
                    ui.set_active_tab(0);
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_lock_screen_now(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            if cfg!(target_os = "linux") && Path::new("/usr/libexec/unicore-lock").is_file() {
                let _ = std::process::Command::new("/usr/libexec/unicore-lock")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
                if ui.get_quick_settings_only() {
                    let _ = slint::quit_event_loop();
                }
            } else {
                set_status(&ui, "Lock screen is available inside UniCore OS Wayland session", false);
            }
        });
    }

    // v0.5.0 Audio Card Profiles, Notifications/DND, Capture/Recorder, Network Drives & Accessibility
    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_audio_profile_selected(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let prof_opt = st.borrow().audio_profiles.get(idx as usize).cloned();
            if let Some(p) = prof_opt {
                report_result(
                    &ui,
                    set_audio_card_profile(&p.card_name, &p.profile_name),
                    format!("Audio profile set: {}", p.label),
                );
                refresh_settings(&ui, &mut st.borrow_mut());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_toggle_dnd(move |enabled| {
            let Some(ui) = ui_weak.upgrade() else { return };
            match set_dnd_state(enabled) {
                Ok(msg) => set_status(&ui, msg, false),
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_dismiss_all_notifications(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(&ui, dismiss_all_notifications().map(|_| ()), "Notifications cleared");
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_restore_last_notification(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            report_result(&ui, restore_last_notification().map(|_| ()), "Restored last notification");
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_capture_take_shot(move |region, delay_sec| {
            let Some(ui) = ui_weak.upgrade() else { return };
            ui.set_cap_busy(true);
            ui.set_cap_status(SharedString::from("Capturing screenshot..."));
            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = run_capture_shot(region, delay_sec);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_cap_busy(false);
                        match res {
                            Ok(msg) => {
                                ui.set_cap_status(SharedString::from(msg.as_str()));
                                set_status(&ui, msg, false);
                            }
                            Err(err) => {
                                ui.set_cap_status(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_capture_start_recording(move |region, audio| {
            let Some(ui) = ui_weak.upgrade() else { return };
            ui.set_cap_busy(true);
            ui.set_cap_status(SharedString::from("Starting MP4 screen recording..."));
            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = start_screen_recording(region, audio);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_cap_busy(false);
                        refresh_capture_and_notify(&ui);
                        match res {
                            Ok(msg) => {
                                ui.set_cap_status(SharedString::from(msg.as_str()));
                                set_status(&ui, msg, false);
                            }
                            Err(err) => {
                                ui.set_cap_status(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_capture_stop_recording(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            ui.set_cap_busy(true);
            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = stop_screen_recording();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_cap_busy(false);
                        refresh_capture_and_notify(&ui);
                        match res {
                            Ok(msg) => {
                                ui.set_cap_status(SharedString::from(msg.as_str()));
                                set_status(&ui, msg, false);
                            }
                            Err(err) => {
                                ui.set_cap_status(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_capture_open_dir(move |which| {
            let Some(ui) = ui_weak.upgrade() else { return };
            match open_capture_dir(&which) {
                Ok(msg) => set_status(&ui, msg, false),
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_select_share(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            if let Some(sh) = st.borrow().shares.get(idx as usize) {
                ui.set_set_selected_share_idx(idx);
                ui.set_set_selected_share_id(SharedString::from(sh.id.as_str()));
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let st = Rc::clone(&settings_state);
        ui.on_add_network_share(move |label, uri, user, pass| {
            let Some(ui) = ui_weak.upgrade() else { return };
            match add_network_share(&label, &uri, &user, &pass) {
                Ok(msg) => {
                    ui.set_set_share_label_input(SharedString::new());
                    ui.set_set_share_uri_input(SharedString::new());
                    ui.set_set_share_user_input(SharedString::new());
                    ui.set_set_share_pass_input(SharedString::new());
                    refresh_settings(&ui, &mut st.borrow_mut());
                    set_status(&ui, msg, false);
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_share_action(move |action, id| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let act = action.to_string();
            let sid = id.to_string();
            set_status(&ui, format!("Network drive: {act} '{sid}'..."), false);
            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = share_action(&act, &sid);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        match res {
                            Ok(msg) => {
                                ui.invoke_refresh_settings();
                                set_status(&ui, msg, false);
                            }
                            Err(err) => set_status(&ui, err, true),
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_toggle_osk(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            match toggle_on_screen_keyboard() {
                Ok(msg) => {
                    refresh_capture_and_notify(&ui);
                    set_status(&ui, msg, false);
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let lang_cell = Rc::clone(&current_lang);
        ui.on_speak_clipboard(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let code = lang_cell.borrow().code();
            match speak_text_offline(code, "") {
                Ok(msg) => set_status(&ui, msg, false),
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    // v0.6.0 Native Graphical Start Menu & Emoji Picker callbacks
    {
        let ui_weak = ui.as_weak();
        let scache = Rc::clone(&start_apps_cache);
        ui.on_start_filter_changed(move |cat, query| {
            let Some(ui) = ui_weak.upgrade() else { return };
            refresh_start_menu(&ui, &scache.borrow(), &cat, &query);
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_start_launch_app(move |exec_cmd| {
            let Some(ui) = ui_weak.upgrade() else { return };
            match launch_desktop_command(&exec_cmd) {
                Ok(()) => {
                    let _ = slint::quit_event_loop();
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_start_open_folder(move |folder| {
            let Some(ui) = ui_weak.upgrade() else { return };
            match open_user_folder(&folder) {
                Ok(()) => {
                    let _ = slint::quit_event_loop();
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_emoji_filter_changed(move |cat, query| {
            let Some(ui) = ui_weak.upgrade() else { return };
            refresh_emoji_grid(&ui, &cat, &query);
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_emoji_selected(move |sym| {
            let Some(ui) = ui_weak.upgrade() else { return };
            match insert_emoji(&sym) {
                Ok(msg) => ui.set_emoji_status(SharedString::from(msg.as_str())),
                Err(err) => ui.set_emoji_status(SharedString::from(err.as_str())),
            }
        });
    }

    // v0.7.0 Classic 98/XP Game Launcher, Auto-Repair & Post-Install Target Picker callbacks
    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_retro_install_and_play(
            move |src, name, preset_idx, fps_cap, single_core, vdesk_idx, cd_iso| {
                let Some(ui) = ui_weak.upgrade() else { return };
                let src_s = src.trim().to_string();
                let name_s = name.trim().to_string();
                let cd_s = cd_iso.trim().to_string();
                if src_s.is_empty() {
                    ui.set_retro_status(SharedString::from("Please select a game .exe, .iso, .zip, or folder first."));
                    return;
                }
                let cfg = b.borrow().config.clone();
                ui.set_retro_busy(true);
                ui.set_retro_status(SharedString::from("Preparing Classic 98/XP game & sandbox..."));
                let thread_ui = ui.as_weak();
                std::thread::spawn(move || {
                    let result: Result<String, String> = (|| {
                        let media = prepare_retro_source(&src_s, &name_s)?;
                        let preset = RetroPreset::from_index(preset_idx);
                        let effective_iso = if !cd_s.is_empty() {
                            cd_s
                        } else {
                            media.iso_path.clone()
                        };
                        let profile = RetroGameProfile {
                            preset,
                            fps_cap_60: fps_cap,
                            single_core_cpu: single_core,
                            virtual_desktop: virtual_desktop_from_index(vdesk_idx).to_string(),
                            cd_iso_path: effective_iso,
                        };
                        let worker = CenterBackend::new(cfg);
                        let req = InstallRequest {
                            app_name: Some(if !name_s.is_empty() { name_s } else { media.name }),
                            win_era: match preset {
                                RetroPreset::Classic2d | RetroPreset::DosWin16 => Some(WinEra::Win98),
                                RetroPreset::Classic3d => Some(WinEra::WinXp),
                                RetroPreset::Auto => None,
                            },
                            permissions: PermissionProfile::default(),
                            create_shortcut: true,
                        };
                        let installed = worker.install_file(Path::new(&media.main_exe), &req)?;
                        let tuned = worker.apply_retro_profile(&installed.app_id, &profile)?;
                        worker.spawn_app_detached(&tuned.app_id)?;
                        Ok(format!(
                            "Launched '{}' (Desktop & Start Menu shortcut created!)",
                            tuned.name
                        ))
                    })();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = thread_ui.upgrade() {
                            ui.set_retro_busy(false);
                            match result {
                                Ok(msg) => {
                                    ui.set_retro_status(SharedString::from(msg.as_str()));
                                    ui.invoke_refresh_all();
                                    set_status(&ui, msg, false);
                                }
                                Err(err) => {
                                    ui.set_retro_status(SharedString::from(err.as_str()));
                                    set_status(&ui, err, true);
                                }
                            }
                        }
                    });
                });
            },
        );
    }

    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        let cache = Rc::clone(&apps_cache);
        ui.on_save_sel_retro_profile(move |preset_idx, fps_cap, single_core, vdesk_idx, cd_iso| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let app_id = ui.get_sel_app_id().to_string();
            if app_id.is_empty() {
                return;
            }
            let profile = RetroGameProfile {
                preset: RetroPreset::from_index(preset_idx),
                fps_cap_60: fps_cap,
                single_core_cpu: single_core,
                virtual_desktop: virtual_desktop_from_index(vdesk_idx).to_string(),
                cd_iso_path: cd_iso.trim().to_string(),
            };
            match b.borrow().apply_retro_profile(&app_id, &profile) {
                Ok(updated) => {
                    refresh_apps_and_browse(&ui, &b.borrow(), &cache, Some(&updated.app_id));
                    set_status(
                        &ui,
                        format!("Saved Classic 98/XP settings for '{}'", updated.name),
                        false,
                    );
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_scan_sel_game_exe(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let app_id = ui.get_sel_app_id().to_string();
            if app_id.is_empty() {
                return;
            }
            match b.borrow().scan_installed_game_exes(&app_id) {
                Ok(found) if !found.is_empty() => {
                    let cands: Vec<SharedString> = found
                        .into_iter()
                        .map(|p| SharedString::from(p.display().to_string()))
                        .collect();
                    ui.set_retro_pick_app_id(SharedString::from(app_id.as_str()));
                    ui.set_retro_pick_app_name(ui.get_sel_name());
                    ui.set_retro_pick_candidates(ModelRc::new(VecModel::from(cands)));
                    ui.set_retro_pick_selected_idx(0);
                    ui.set_retro_pick_only(true);
                }
                Ok(_) => set_status(
                    &ui,
                    "No extra game .exe found in drive_c yet (run Setup.exe first if not installed).",
                    false,
                ),
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        ui.on_repair_run_and_retry(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let app_id = ui.get_repair_app_id().to_string();
            if app_id.is_empty() {
                return;
            }
            let verbs = ui.get_repair_verbs().trim().to_string();
            let suggest_2d = ui.get_repair_suggest_2d();
            let cfg = b.borrow().config.clone();
            ui.set_repair_busy(true);
            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res: Result<String, String> = (|| {
                    let worker = CenterBackend::new(cfg.clone());
                    if suggest_2d {
                        let profile = RetroGameProfile {
                            preset: RetroPreset::Classic2d,
                            fps_cap_60: true,
                            single_core_cpu: false,
                            virtual_desktop: "off".to_string(),
                            cd_iso_path: String::new(),
                        };
                        let _ = worker.apply_retro_profile(&app_id, &profile);
                    }
                    if !verbs.is_empty() {
                        CenterBackend::run_win32_runtimes_blocking(&cfg, &app_id, Some(&verbs))?;
                    }
                    worker.spawn_app_detached(&app_id)?;
                    Ok("Repair complete — relaunching game!".to_string())
                })();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_repair_busy(false);
                        match res {
                            Ok(msg) => {
                                ui.set_repair_summary(SharedString::from(msg.as_str()));
                                set_status(&ui, msg, false);
                                let _ = slint::quit_event_loop();
                            }
                            Err(err) => {
                                ui.set_repair_summary(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        let b = Rc::clone(&backend);
        let cache = Rc::clone(&apps_cache);
        ui.on_retro_pick_apply(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let app_id = ui.get_retro_pick_app_id().to_string();
            let cands = ui.get_retro_pick_candidates();
            let Some(chosen) = slint::Model::row_data(&cands, idx as usize) else {
                return;
            };
            match b
                .borrow()
                .switch_app_target_exe(&app_id, Path::new(chosen.as_str()))
            {
                Ok(updated) => {
                    let _ = b.borrow().spawn_app_detached(&updated.app_id);
                    ui.set_retro_pick_only(false);
                    refresh_apps_and_browse(&ui, &b.borrow(), &cache, Some(&updated.app_id));
                    set_status(
                        &ui,
                        format!(
                            "Updated shortcut for '{}' -> {} and launched!",
                            updated.name, chosen
                        ),
                        false,
                    );
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    // === v0.8.0 Ecosystem, Phone Link, Cloud Drives, Windows Migration & Hardware Drivers ===
    {
        let ui_weak = ui.as_weak();
        ui.on_eco_refresh(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            refresh_ecosystem(&ui);
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_eco_phone_service(move |act| {
            let Some(ui) = ui_weak.upgrade() else { return };
            match phone_service_action(act.as_str()) {
                Ok(msg) => {
                    refresh_ecosystem(&ui);
                    let note = if msg.is_empty() {
                        format!("Phone Link action '{act}' completed.")
                    } else {
                        msg
                    };
                    ui.set_eco_status(SharedString::from(note.as_str()));
                    set_status(&ui, note, false);
                }
                Err(err) => {
                    ui.set_eco_status(SharedString::from(err.as_str()));
                    set_status(&ui, err, true);
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_eco_phone_autostart(move |enabled| {
            let Some(ui) = ui_weak.upgrade() else { return };
            match phone_set_autostart(enabled) {
                Ok(_) => {
                    ui.set_eco_kde_autostart(enabled);
                    set_status(
                        &ui,
                        if enabled {
                            "KDE Connect will auto-start on login."
                        } else {
                            "KDE Connect auto-start disabled (0 MB idle RAM)."
                        },
                        false,
                    );
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_eco_phone_device_action(move |act, dev_id| {
            let Some(ui) = ui_weak.upgrade() else { return };
            match phone_device_action(act.as_str(), dev_id.as_str()) {
                Ok(msg) => {
                    refresh_ecosystem(&ui);
                    let note = if msg.is_empty() {
                        format!("Phone '{act}' sent.")
                    } else {
                        msg
                    };
                    ui.set_eco_status(SharedString::from(note.as_str()));
                    set_status(&ui, note, false);
                }
                Err(err) => {
                    ui.set_eco_status(SharedString::from(err.as_str()));
                    set_status(&ui, err, true);
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_eco_phone_mirror(move |screen_off| {
            let Some(ui) = ui_weak.upgrade() else { return };
            match phone_mirror_android("", screen_off) {
                Ok(_) => {
                    ui.set_eco_status(SharedString::from(
                        "Android screen mirroring launched via scrcpy!",
                    ));
                    set_status(&ui, "Android screen mirror launched (scrcpy).", false);
                }
                Err(err) => {
                    ui.set_eco_status(SharedString::from(err.as_str()));
                    set_status(&ui, err, true);
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_eco_phone_adb_wifi(move |target| {
            let Some(ui) = ui_weak.upgrade() else { return };
            match phone_connect_adb_wifi(target.as_str()) {
                Ok(msg) => {
                    refresh_ecosystem(&ui);
                    ui.set_eco_status(SharedString::from(msg.as_str()));
                    set_status(&ui, msg, false);
                }
                Err(err) => {
                    ui.set_eco_status(SharedString::from(err.as_str()));
                    set_status(&ui, err, true);
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_eco_cloud_connect(move |prov_idx, name, url, user, pass| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let provider = cloud_provider_from_index(prov_idx).to_string();
            let name_s = name.to_string();
            let url_s = url.to_string();
            let user_s = user.to_string();
            let pass_s = pass.to_string();
            ui.set_eco_busy(true);
            ui.set_eco_status(SharedString::from(format!(
                "Connecting '{name_s}' ({provider}) — complete browser sign-in if prompted..."
            )));
            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = cloud_connect(&provider, &name_s, &url_s, &user_s, &pass_s);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_eco_busy(false);
                        ui.set_eco_cloud_pass_input(SharedString::new());
                        refresh_ecosystem(&ui);
                        match res {
                            Ok(msg) => {
                                let note = if msg.is_empty() {
                                    format!("Cloud drive '{name_s}' connected & mounted!")
                                } else {
                                    msg
                                };
                                ui.set_eco_status(SharedString::from(note.as_str()));
                                set_status(&ui, note, false);
                            }
                            Err(err) => {
                                ui.set_eco_status(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_eco_cloud_action(move |act, name| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let act_s = act.to_string();
            let name_s = name.to_string();
            ui.set_eco_busy(true);
            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = cloud_remote_action(&act_s, &name_s);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_eco_busy(false);
                        refresh_ecosystem(&ui);
                        match res {
                            Ok(msg) => {
                                ui.set_eco_status(SharedString::from(msg.as_str()));
                                set_status(&ui, msg, false);
                            }
                            Err(err) => {
                                ui.set_eco_status(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_eco_cloud_autostart(move |name, _toggle| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let current = read_cloud_overview()
                .remotes
                .into_iter()
                .find(|r| r.name == name.as_str())
                .map(|r| r.autostart)
                .unwrap_or(false);
            let next = !current;
            match cloud_set_autostart(name.as_str(), next) {
                Ok(_) => {
                    refresh_ecosystem(&ui);
                    set_status(
                        &ui,
                        format!(
                            "Auto-mount for '{}' {}.",
                            name,
                            if next { "enabled" } else { "disabled" }
                        ),
                        false,
                    );
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_eco_cloud_open_dir(move |dir| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let _ = std::fs::create_dir_all(dir.as_str());
            if let Err(err) = open_capture_dir(dir.as_str()) {
                set_status(&ui, err, true);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let parts_rc = Rc::clone(&win_parts_cache);
        ui.on_eco_migrate_scan(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            ui.set_eco_busy(true);
            ui.set_eco_win_info_summary(SharedString::from(
                "Scanning disks for Windows NTFS partitions (read-only)...",
            ));
            match scan_windows_partitions() {
                Ok(parts) => {
                    let labels: Vec<SharedString> = parts
                        .iter()
                        .map(|p| {
                            SharedString::from(format!("{} — {} ({})", p.device, p.label, p.size))
                        })
                        .collect();
                    ui.set_eco_win_part_labels(ModelRc::new(VecModel::from(labels)));
                    ui.set_eco_win_part_idx(0);
                    if let Some(first) = parts.first() {
                        let users: Vec<SharedString> = first
                            .users
                            .iter()
                            .map(|u| SharedString::from(u.as_str()))
                            .collect();
                        ui.set_eco_win_users(ModelRc::new(VecModel::from(users)));
                        ui.set_eco_win_user_idx(0);
                        if let Ok(info) = inspect_windows_partition(&first.device) {
                            let bm = if info.bookmarks_found.is_empty() {
                                "none".to_string()
                            } else {
                                info.bookmarks_found.join(", ")
                            };
                            ui.set_eco_win_info_summary(SharedString::from(format!(
                                "Found Windows on {} • Free /data: {:.1} GB • Fonts: {} • Bookmarks: {}",
                                info.device,
                                info.free_data_mb as f64 / 1024.0,
                                info.fonts_count,
                                bm
                            )));
                            let matches: Vec<String> = info
                                .store_matches
                                .iter()
                                .map(|m| format!("{} -> {}", m.windows_app, m.store_label))
                                .collect();
                            ui.set_eco_win_store_matches(SharedString::from(matches.join("  •  ")));
                        }
                    } else {
                        ui.set_eco_win_users(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
                        ui.set_eco_win_store_matches(SharedString::new());
                        ui.set_eco_win_info_summary(SharedString::from(
                            "No Windows NTFS partition detected on internal/USB drives.",
                        ));
                    }
                    *parts_rc.borrow_mut() = parts;
                }
                Err(err) => {
                    ui.set_eco_win_info_summary(SharedString::from(err.as_str()));
                    set_status(&ui, err, true);
                }
            }
            ui.set_eco_busy(false);
        });
    }

    {
        let ui_weak = ui.as_weak();
        let parts_rc = Rc::clone(&win_parts_cache);
        ui.on_eco_migrate_select_part(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let parts = parts_rc.borrow();
            let Some(part) = parts.get(idx as usize) else {
                return;
            };
            let users: Vec<SharedString> = part
                .users
                .iter()
                .map(|u| SharedString::from(u.as_str()))
                .collect();
            ui.set_eco_win_users(ModelRc::new(VecModel::from(users)));
            ui.set_eco_win_user_idx(0);
            if let Ok(info) = inspect_windows_partition(&part.device) {
                let bm = if info.bookmarks_found.is_empty() {
                    "none".to_string()
                } else {
                    info.bookmarks_found.join(", ")
                };
                ui.set_eco_win_info_summary(SharedString::from(format!(
                    "Found Windows on {} • Free /data: {:.1} GB • Fonts: {} • Bookmarks: {}",
                    info.device,
                    info.free_data_mb as f64 / 1024.0,
                    info.fonts_count,
                    bm
                )));
                let matches: Vec<String> = info
                    .store_matches
                    .iter()
                    .map(|m| format!("{} -> {}", m.windows_app, m.store_label))
                    .collect();
                ui.set_eco_win_store_matches(SharedString::from(matches.join("  •  ")));
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let parts_rc = Rc::clone(&win_parts_cache);
        ui.on_eco_migrate_run(
            move |pidx, uidx, docs, pics, desktop, music, videos, saves, bookmarks, fonts| {
                let Some(ui) = ui_weak.upgrade() else { return };
                let parts = parts_rc.borrow();
                let Some(part) = parts.get(pidx as usize) else {
                    set_status(&ui, "Scan & select a Windows drive first.", true);
                    return;
                };
                let Some(win_user) = part.users.get(uidx as usize) else {
                    set_status(&ui, "Select a Windows user profile first.", true);
                    return;
                };
                let flags = build_migrate_flags_csv(
                    docs, pics, desktop, music, videos, saves, bookmarks, fonts,
                );
                let dev_s = part.device.clone();
                let user_s = win_user.clone();
                ui.set_eco_busy(true);
                ui.set_eco_status(SharedString::from(format!(
                    "Safely copying selected files from {dev_s} (read-only)..."
                )));
                let thread_ui = ui.as_weak();
                std::thread::spawn(move || {
                    let res = run_windows_migration(&dev_s, &user_s, &flags);
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = thread_ui.upgrade() {
                            ui.set_eco_busy(false);
                            match res {
                                Ok(msg) => {
                                    let note = format!("Windows migration complete! ({msg})");
                                    ui.set_eco_status(SharedString::from(note.as_str()));
                                    set_status(&ui, note, false);
                                }
                                Err(err) => {
                                    ui.set_eco_status(SharedString::from(err.as_str()));
                                    set_status(&ui, err, true);
                                }
                            }
                        }
                    });
                });
            },
        );
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_eco_hw_set_gpu(move |idx| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let mode = gpu_mode_from_index(idx);
            match hw_set_gpu_mode(mode) {
                Ok(_) => {
                    let note = format!("Saved GPU mode '{mode}' (applies on next login/reboot).");
                    ui.set_eco_status(SharedString::from(note.as_str()));
                    set_status(&ui, note, false);
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_eco_hw_set_wifi_ps(move |enabled| {
            let Some(ui) = ui_weak.upgrade() else { return };
            match hw_set_wifi_powersave(enabled) {
                Ok(_) => {
                    ui.set_eco_hw_wifi_ps(enabled);
                    let note = if enabled {
                        "Wi-Fi power-save enabled."
                    } else {
                        "Wi-Fi power-save disabled (better stability on Realtek/Intel cards)."
                    };
                    ui.set_eco_status(SharedString::from(note));
                    set_status(&ui, note, false);
                }
                Err(err) => set_status(&ui, err, true),
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_eco_hw_reset_audio_bt(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            ui.set_eco_busy(true);
            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = hw_reset_audio_bt();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_eco_busy(false);
                        match res {
                            Ok(msg) => {
                                ui.set_eco_status(SharedString::from(msg.as_str()));
                                set_status(&ui, msg, false);
                            }
                            Err(err) => set_status(&ui, err, true),
                        }
                    }
                });
            });
        });
    }

    {
        let ui_weak = ui.as_weak();
        ui.on_eco_hw_fw_action(move |act| {
            let Some(ui) = ui_weak.upgrade() else { return };
            let act_s = act.to_string();
            ui.set_eco_busy(true);
            ui.set_eco_hw_fw_log(SharedString::from(
                "Querying official LVFS firmware metadata (fwupd)...",
            ));
            let thread_ui = ui.as_weak();
            std::thread::spawn(move || {
                let res = if act_s == "update" {
                    hw_fw_update()
                } else {
                    hw_fw_check()
                };
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = thread_ui.upgrade() {
                        ui.set_eco_busy(false);
                        match res {
                            Ok(out) => {
                                let brief = if out.trim().is_empty() {
                                    "All firmware is up to date (LVFS).".to_string()
                                } else {
                                    out.lines().take(6).collect::<Vec<_>>().join(" | ")
                                };
                                ui.set_eco_hw_fw_log(SharedString::from(brief.as_str()));
                                set_status(&ui, brief, false);
                            }
                            Err(err) => {
                                ui.set_eco_hw_fw_log(SharedString::from(err.as_str()));
                                set_status(&ui, err, true);
                            }
                        }
                    }
                });
            });
        });
    }

    if let Err(err) = ui.run() {
        eprintln!("UniCore App Center exited with error: {err}");
        std::process::exit(1);
    }
}
