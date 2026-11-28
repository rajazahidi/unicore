//! UniCore OS Native AegisTunnel VPN CLI & Linux TUN Daemon (`/usr/bin/unicore-aegis-vpn`).

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use unicore_aegis_vpn::config::EngineConfig;
use unicore_aegis_vpn::handover::socket_pool::BindSocketFn;
use unicore_aegis_vpn::tunnel::wireguard::generate_keypair_base64;
use unicore_aegis_vpn::tunnel::{start_engine, EngineCommand, TunInput};
use unicore_aegis_vpn::{default_engine_config, parse_wg_conf_to_engine_config, run_self_test};

static STOP_FLAG: AtomicBool = AtomicBool::new(false);
static ROTATE_PQ_FLAG: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn handle_stop_signal(_sig: libc::c_int) {
    STOP_FLAG.store(true, Ordering::SeqCst);
}

#[cfg(unix)]
extern "C" fn handle_hup_signal(_sig: libc::c_int) {
    ROTATE_PQ_FLAG.store(true, Ordering::SeqCst);
}

#[cfg(target_os = "linux")]
fn open_linux_tun(dev_name: &str) -> Result<(i32, String), String> {
    use std::ffi::CString;

    const IFF_TUN: libc::c_short = 0x0001;
    const IFF_NO_PI: libc::c_short = 0x1000;
    const TUNSETIFF: libc::c_ulong = 0x4004_54ca;

    #[repr(C)]
    struct IfReq {
        ifr_name: [u8; 16],
        ifr_flags: libc::c_short,
        _pad: [u8; 22],
    }

    let c_path = CString::new("/dev/net/tun").map_err(|e| e.to_string())?;
    let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDWR) };
    if fd < 0 {
        return Err(format!(
            "Failed to open /dev/net/tun: {}",
            std::io::Error::last_os_error()
        ));
    }

    let mut req = IfReq {
        ifr_name: [0u8; 16],
        ifr_flags: IFF_TUN | IFF_NO_PI,
        _pad: [0u8; 22],
    };
    let bytes = dev_name.as_bytes();
    let copy_len = bytes.len().min(15);
    req.ifr_name[..copy_len].copy_from_slice(&bytes[..copy_len]);

    let rc = unsafe { libc::ioctl(fd, TUNSETIFF as _, &mut req as *mut IfReq) };
    if rc < 0 {
        let err = std::io::Error::last_os_error();
        unsafe { libc::close(fd) };
        return Err(format!("ioctl(TUNSETIFF, {dev_name}) failed: {err}"));
    }

    let end = req.ifr_name.iter().position(|&b| b == 0).unwrap_or(16);
    let actual_name = String::from_utf8_lossy(&req.ifr_name[..end]).to_string();
    Ok((fd, actual_name))
}

fn print_usage() {
    println!(
        "\
unicore-aegis-vpn - UniCore OS Native AegisTunnel VPN Engine

USAGE:
  unicore-aegis-vpn keygen
  unicore-aegis-vpn default-config
  unicore-aegis-vpn self-test
  unicore-aegis-vpn parse-wg <FILE.conf>
  unicore-aegis-vpn up --config <FILE.json> [--dev aegis0] [--telemetry <FILE.json>]"
    );
}

fn run_daemon(config_path: &str, dev_name: &str, telemetry_path: &str) -> Result<(), String> {
    let raw = fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read config {config_path}: {e}"))?;
    let cfg: EngineConfig = serde_json::from_str(&raw)
        .map_err(|e| format!("Failed to parse AegisTunnel JSON config: {e}"))?;

    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGTERM, handle_stop_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, handle_stop_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGHUP, handle_hup_signal as *const () as libc::sighandler_t);
    }

    #[cfg(target_os = "linux")]
    let (tun_input, actual_dev) = {
        let (fd, name) = open_linux_tun(dev_name)?;
        (TunInput::RawFd(fd), name)
    };

    #[cfg(not(target_os = "linux"))]
    let (tun_input, actual_dev) = {
        let _ = dev_name;
        let (_tx, rx) = tokio::sync::mpsc::channel(16);
        let (tx2, _rx2) = tokio::sync::mpsc::channel(16);
        (
            TunInput::Transport(unicore_aegis_vpn::tunnel::TunTransport::Channel {
                inbound_rx: rx,
                outbound_tx: tx2,
            }),
            "mock0".to_string(),
        )
    };

    let bind_cb: BindSocketFn = Arc::new(|_raw_fd, _net_id, _loopback| true);
    let handle = start_engine(cfg, tun_input, 1, format!("Linux:{actual_dev}"), bind_cb)?;

    if let Some(parent) = Path::new(telemetry_path).parent() {
        let _ = fs::create_dir_all(parent);
    }

    while !STOP_FLAG.load(Ordering::SeqCst) {
        if ROTATE_PQ_FLAG.swap(false, Ordering::SeqCst) {
            let _ = handle.cmd_tx.try_send(EngineCommand::RotatePqKey);
        }
        let snap = handle.snapshot();
        if let Ok(json) = serde_json::to_string_pretty(&snap) {
            let tmp = format!("{telemetry_path}.tmp");
            if fs::write(&tmp, &json).is_ok() {
                let _ = fs::rename(&tmp, telemetry_path);
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    handle.stop();
    let _ = fs::remove_file(telemetry_path);
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help") {
        print_usage();
        return;
    }

    match args[0].as_str() {
        "keygen" => {
            let (priv_b64, pub_b64) = generate_keypair_base64();
            let out = serde_json::json!({
                "privateKey": priv_b64,
                "publicKey": pub_b64
            });
            println!("{out}");
        }
        "default-config" => {
            let cfg = default_engine_config();
            match serde_json::to_string_pretty(&cfg) {
                Ok(s) => println!("{s}"),
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
        }
        "self-test" => {
            let rep = run_self_test();
            println!(
                "{}",
                serde_json::to_string_pretty(&rep).unwrap_or_else(|_| "{}".to_string())
            );
            if !rep.all_passed {
                std::process::exit(1);
            }
        }
        "parse-wg" => {
            let Some(path) = args.get(1) else {
                eprintln!("Usage: unicore-aegis-vpn parse-wg <FILE.conf>");
                std::process::exit(2);
            };
            let raw = match fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to read {path}: {e}");
                    std::process::exit(1);
                }
            };
            match parse_wg_conf_to_engine_config(&raw) {
                Ok(cfg) => println!(
                    "{}",
                    serde_json::to_string_pretty(&cfg).unwrap_or_else(|_| "{}".to_string())
                ),
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
        }
        "up" => {
            let mut config_path = "/data/network/aegis-vpn.json".to_string();
            let mut dev_name = "aegis0".to_string();
            let mut telemetry_path = "/run/unicore-aegis/telemetry.json".to_string();
            let mut i = 1usize;
            while i < args.len() {
                match args[i].as_str() {
                    "--config" => {
                        i += 1;
                        if let Some(v) = args.get(i) {
                            config_path = v.clone();
                        }
                    }
                    "--dev" => {
                        i += 1;
                        if let Some(v) = args.get(i) {
                            dev_name = v.clone();
                        }
                    }
                    "--telemetry" => {
                        i += 1;
                        if let Some(v) = args.get(i) {
                            telemetry_path = v.clone();
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            if let Err(e) = run_daemon(&config_path, &dev_name, &telemetry_path) {
                eprintln!("AegisTunnel daemon failed: {e}");
                std::process::exit(1);
            }
        }
        other => {
            eprintln!("Unknown subcommand: {other}");
            print_usage();
            std::process::exit(2);
        }
    }
}
