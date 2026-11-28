//! UniCore OS v1.6.0 Native Embedded Engines:
//! - AegisTunnel VPN (`unicore-aegis-vpn` + `unicore-aegis-ctl`): WireGuard + AmneziaWG Anti-DPI
//!   + REALITY TCP/443 + Post-Quantum ML-KEM-768 + Local DNS Sinkhole (`0 MB` idle RAM)
//! - ResonanceFX PipeWire Equalizer & Spatial DSP (`libunicore_resonance_fx.so` + `unicore-resonance-ctl`):
//!   10-Band Graphic EQ + Pure Bass + Clarity + 3D Surround + Binaural/8D Orbit + Tube + UPOLA Convolver + Reverb (`0 MB` idle RAM)

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::settings::{persistent_display_config_dir, SYS_HELPER_BIN};

pub const AEGIS_CTL_BIN: &str = "/usr/libexec/unicore-aegis-ctl";
pub const AEGIS_VPN_BIN: &str = "/usr/bin/unicore-aegis-vpn";
pub const RESONANCE_CTL_BIN: &str = "/usr/libexec/unicore-resonance-ctl";

pub const AEGIS_TRANSPORT_MODES: [(&str, &str); 3] = [
    ("AUTO_HAPPY_EYEBALLS", "Auto Happy Eyeballs (UDP + REALITY Fallback)"),
    ("FORCE_UDP", "AmneziaWG Anti-DPI UDP"),
    ("FORCE_REALITY_TCP", "REALITY TLS 1.3 TCP/443 Stealth"),
];

pub const RESONANCE_PRESET_NAMES: [&str; 7] = [
    "Flat Reference",
    "Pure Bass Boost",
    "Vocal & Podcast Studio",
    "Cinema 3D Surround",
    "Hi-Fi Warm Tube",
    "Gaming Footsteps",
    "8D Spatial Orbit Live",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AegisVpnStatus {
    pub installed: bool,
    pub running: bool,
    pub transport: String,
    pub pq_suite: String,
    pub handshake: bool,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub dns_blocked: u64,
    pub pq_epoch: u64,
}

impl Default for AegisVpnStatus {
    fn default() -> Self {
        Self {
            installed: false,
            running: false,
            transport: "Idle (0 MB RAM)".to_string(),
            pq_suite: "X25519+ML-KEM-768 Ready".to_string(),
            handshake: false,
            rx_bytes: 0,
            tx_bytes: 0,
            dns_blocked: 0,
            pq_epoch: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AegisVpnProfile {
    pub endpoint: String,
    pub tunnel_ipv4: String,
    pub upstream_dns: String,
    pub private_key: String,
    pub peer_public_key: String,
    pub transport_mode: String,
    pub obfuscation_enabled: bool,
    pub pq_enabled: bool,
    pub dns_sinkhole_enabled: bool,
    pub loopback_mode: bool,
    pub reality_sni: String,
}

impl Default for AegisVpnProfile {
    fn default() -> Self {
        Self {
            endpoint: "127.0.0.1:51820".to_string(),
            tunnel_ipv4: "10.66.0.2".to_string(),
            upstream_dns: "1.1.1.1".to_string(),
            private_key: String::new(),
            peer_public_key: String::new(),
            transport_mode: "AUTO_HAPPY_EYEBALLS".to_string(),
            obfuscation_enabled: true,
            pq_enabled: true,
            dns_sinkhole_enabled: true,
            loopback_mode: true,
            reality_sni: "www.cloudflare.com".to_string(),
        }
    }
}

pub fn parse_aegis_status(raw: &str) -> AegisVpnStatus {
    let mut out = AegisVpnStatus::default();
    for line in raw.lines() {
        if let Some((k, v)) = line.trim().split_once('=') {
            let val = v.trim();
            match k.trim() {
                "installed" => out.installed = val == "1" || val == "true",
                "running" => out.running = val == "1" || val == "true",
                "transport" => out.transport = val.to_string(),
                "pq_suite" => out.pq_suite = val.to_string(),
                "handshake" => out.handshake = val == "true" || val == "1",
                "rx_bytes" => out.rx_bytes = val.parse().unwrap_or(0),
                "tx_bytes" => out.tx_bytes = val.parse().unwrap_or(0),
                "dns_blocked" => out.dns_blocked = val.parse().unwrap_or(0),
                "pq_epoch" => out.pq_epoch = val.parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    out
}

pub fn read_aegis_status() -> AegisVpnStatus {
    if !cfg!(target_os = "linux") || !Path::new(AEGIS_CTL_BIN).is_file() {
        return AegisVpnStatus::default();
    }
    Command::new(AEGIS_CTL_BIN)
        .arg("status")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_aegis_status(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

pub fn transport_mode_to_index(mode: &str) -> i32 {
    AEGIS_TRANSPORT_MODES
        .iter()
        .position(|(k, _)| *k == mode)
        .unwrap_or(0) as i32
}

pub fn transport_mode_from_index(idx: i32) -> &'static str {
    AEGIS_TRANSPORT_MODES
        .get(idx as usize)
        .map(|(k, _)| *k)
        .unwrap_or("AUTO_HAPPY_EYEBALLS")
}

fn json_escape(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn render_aegis_config_json(p: &AegisVpnProfile) -> String {
    let ep = if p.endpoint.trim().is_empty() {
        "127.0.0.1:51820"
    } else {
        p.endpoint.trim()
    };
    let ip = if p.tunnel_ipv4.trim().is_empty() {
        "10.66.0.2"
    } else {
        p.tunnel_ipv4.trim()
    };
    let dns = if p.upstream_dns.trim().is_empty() {
        "1.1.1.1"
    } else {
        p.upstream_dns.trim()
    };
    let sni = if p.reality_sni.trim().is_empty() {
        "www.cloudflare.com"
    } else {
        p.reality_sni.trim()
    };
    format!(
        "{{\n\
  \"privateKeyBase64\": \"{}\",\n\
  \"peerPublicKeyBase64\": \"{}\",\n\
  \"presharedKeyBase64\": null,\n\
  \"endpoint\": \"{}\",\n\
  \"tunnelIpv4\": \"{}\",\n\
  \"virtualDnsIpv4\": \"10.66.0.1\",\n\
  \"upstreamDnsIpv4\": \"{}\",\n\
  \"persistentKeepalive\": 25,\n\
  \"loopbackMode\": {},\n\
  \"dnsSinkholeEnabled\": {},\n\
  \"obfuscation\": {{\n\
    \"enabled\": {},\n\
    \"jc\": 4,\n\
    \"jmin\": 50,\n\
    \"jmax\": 600,\n\
    \"s1\": 42,\n\
    \"s2\": 76,\n\
    \"h1\": 1928374651,\n\
    \"h2\": 2093847561,\n\
    \"h3\": 3182736451,\n\
    \"h4\": 4012345679\n\
  }},\n\
  \"transportMode\": \"{}\",\n\
  \"reality\": {{\n\
    \"enabled\": true,\n\
    \"sni\": \"{}\",\n\
    \"shortIdHex\": \"0123456789abcdef\"\n\
  }},\n\
  \"pqEnabled\": {},\n\
  \"pqRekeyIntervalSecs\": 600,\n\
  \"blockedDomains\": [\"doubleclick.net\", \"googlesyndication.com\", \"ads.facebook.com\", \"analytics.tiktok.com\"],\n\
  \"allowedDomains\": []\n\
}}\n",
        json_escape(p.private_key.trim()),
        json_escape(p.peer_public_key.trim()),
        json_escape(ep),
        json_escape(ip),
        json_escape(dns),
        p.loopback_mode,
        p.dns_sinkhole_enabled,
        p.obfuscation_enabled,
        json_escape(&p.transport_mode),
        json_escape(sni),
        p.pq_enabled,
    )
}

pub fn save_aegis_profile(p: &AegisVpnProfile) -> Result<String, String> {
    if p.private_key.trim().len() < 32 || p.peer_public_key.trim().len() < 32 {
        return Err("Enter or generate valid Base64 Private & Peer Public Keys first.".to_string());
    }
    let json = render_aegis_config_json(p);
    if !cfg!(target_os = "linux") {
        return Ok("Saved AegisTunnel VPN profile (dry-run)".to_string());
    }
    let mut child = Command::new("doas")
        .args([SYS_HELPER_BIN, "aegis-save-config"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to invoke doas aegis-save-config: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(json.as_bytes())
            .map_err(|e| format!("Failed writing Aegis config: {e}"))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("aegis-save-config wait failed: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn run_aegis_action(action: &str) -> Result<String, String> {
    if !cfg!(target_os = "linux") {
        return Ok(format!("AegisTunnel '{action}' completed (dry-run)"));
    }
    let out = match action {
        "start" | "stop" | "rotate-pq" => Command::new("doas")
            .args([SYS_HELPER_BIN, &format!("aegis-{action}")])
            .stdin(Stdio::null())
            .output(),
        "keygen" | "self-test" => Command::new(AEGIS_CTL_BIN)
            .arg(action)
            .stdin(Stdio::null())
            .output(),
        other => return Err(format!("Unknown AegisTunnel action '{other}'")),
    }
    .map_err(|e| format!("Failed to run AegisTunnel '{action}': {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() {
            format!("AegisTunnel '{action}' exited with {}", out.status)
        } else {
            err
        })
    }
}

pub fn parse_keygen_json(raw: &str) -> Option<(String, String)> {
    let find_str = |key: &str| -> Option<String> {
        let pat = format!("\"{key}\"");
        let pos = raw.find(&pat)?;
        let rest = &raw[pos + pat.len()..];
        let colon = rest.find(':')?;
        let after = rest[colon + 1..].trim_start();
        let stripped = after.strip_prefix('"')?;
        let end = stripped.find('"')?;
        Some(stripped[..end].to_string())
    };
    Some((find_str("privateKey")?, find_str("publicKey")?))
}

// ============================================================================
// ResonanceFX PipeWire Equalizer & Spatial DSP
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResonanceStatus {
    pub installed: bool,
    pub running: bool,
    pub plugin_path: String,
    pub params_path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResonanceParams {
    pub master_enabled: bool,
    pub eq_enabled: bool,
    pub eq_bands: [f32; 10],
    pub pure_bass_enabled: bool,
    pub pure_bass_mode: i32,
    pub pure_bass_gain_db: f32,
    pub clarity_enabled: bool,
    pub clarity_mode: i32,
    pub clarity_gain_db: f32,
    pub surround_enabled: bool,
    pub surround_level_pct: i32,
    pub binaural_enabled: bool,
    pub binaural_mode: i32,
    pub tube_enabled: bool,
    pub tube_warmth_pct: i32,
    pub convolver_enabled: bool,
    pub convolver_preset: i32,
    pub reverb_enabled: bool,
    pub reverb_preset: i32,
    pub chorus_enabled: bool,
    pub delay_enabled: bool,
    pub loudness_enabled: bool,
    pub preamp_db: f32,
}

impl Default for ResonanceParams {
    fn default() -> Self {
        Self {
            master_enabled: true,
            eq_enabled: true,
            eq_bands: [0.0; 10],
            pure_bass_enabled: true,
            pure_bass_mode: 1,
            pure_bass_gain_db: 3.5,
            clarity_enabled: true,
            clarity_mode: 0,
            clarity_gain_db: 2.0,
            surround_enabled: false,
            surround_level_pct: 35,
            binaural_enabled: false,
            binaural_mode: 0,
            tube_enabled: false,
            tube_warmth_pct: 40,
            convolver_enabled: false,
            convolver_preset: 0,
            reverb_enabled: false,
            reverb_preset: 0,
            chorus_enabled: false,
            delay_enabled: false,
            loudness_enabled: false,
            preamp_db: 0.0,
        }
    }
}

pub fn resonance_preset_params(idx: i32) -> ResonanceParams {
    let mut p = ResonanceParams::default();
    match idx {
        1 => {
            // Pure Bass Boost
            p.eq_bands = [6.0, 5.0, 3.0, 1.0, 0.0, 0.0, 1.0, 2.0, 2.5, 2.0];
            p.pure_bass_enabled = true;
            p.pure_bass_mode = 2;
            p.pure_bass_gain_db = 7.5;
            p.clarity_enabled = true;
            p.clarity_gain_db = 2.5;
        }
        2 => {
            // Vocal & Podcast Studio
            p.eq_bands = [-2.0, -1.0, 0.0, 1.5, 3.0, 4.0, 3.5, 2.0, 1.0, 0.0];
            p.pure_bass_enabled = false;
            p.clarity_enabled = true;
            p.clarity_mode = 0;
            p.clarity_gain_db = 3.5;
        }
        3 => {
            // Cinema 3D Surround
            p.eq_bands = [4.0, 3.0, 1.5, 0.0, 0.5, 1.5, 2.0, 2.5, 3.0, 2.0];
            p.pure_bass_enabled = true;
            p.pure_bass_mode = 1;
            p.pure_bass_gain_db = 5.0;
            p.surround_enabled = true;
            p.surround_level_pct = 65;
            p.convolver_enabled = true;
            p.convolver_preset = 1;
            p.reverb_enabled = true;
            p.reverb_preset = 2;
        }
        4 => {
            // Hi-Fi Warm Tube
            p.eq_bands = [2.0, 2.5, 1.5, 1.0, 0.5, 0.0, 0.5, 1.5, 2.5, 3.0];
            p.tube_enabled = true;
            p.tube_warmth_pct = 60;
            p.clarity_enabled = true;
            p.clarity_mode = 2;
            p.clarity_gain_db = 3.0;
            p.binaural_enabled = true;
            p.binaural_mode = 0;
        }
        5 => {
            // Gaming Footsteps
            p.eq_bands = [-1.0, 0.0, 0.5, 1.0, 2.0, 3.5, 5.0, 5.5, 4.0, 2.0];
            p.pure_bass_enabled = false;
            p.clarity_enabled = true;
            p.clarity_mode = 1;
            p.clarity_gain_db = 4.5;
            p.surround_enabled = true;
            p.surround_level_pct = 55;
        }
        6 => {
            // 8D Spatial Orbit Live
            p.eq_bands = [3.0, 2.5, 1.0, 0.0, 0.5, 1.0, 2.0, 2.5, 3.0, 2.5];
            p.binaural_enabled = true;
            p.binaural_mode = 2;
            p.surround_enabled = true;
            p.surround_level_pct = 50;
            p.reverb_enabled = true;
            p.reverb_preset = 1;
        }
        _ => {
            // 0: Flat Reference
            p.pure_bass_enabled = false;
            p.clarity_enabled = false;
            p.eq_bands = [0.0; 10];
        }
    }
    p
}

pub fn parse_resonance_status(raw: &str) -> ResonanceStatus {
    let mut out = ResonanceStatus::default();
    for line in raw.lines() {
        if let Some((k, v)) = line.trim().split_once('=') {
            let val = v.trim();
            match k.trim() {
                "installed" => out.installed = val == "1" || val == "true",
                "running" => out.running = val == "1" || val == "true",
                "plugin" => out.plugin_path = val.to_string(),
                "params" => out.params_path = val.to_string(),
                _ => {}
            }
        }
    }
    out
}

pub fn render_resonance_conf(p: &ResonanceParams) -> String {
    let mut out = String::new();
    let b = |v: bool| if v { 1 } else { 0 };
    out.push_str(&format!("master_enabled={}\n", b(p.master_enabled)));
    out.push_str(&format!("eq_enabled={}\n", b(p.eq_enabled)));
    for (i, g) in p.eq_bands.iter().enumerate() {
        out.push_str(&format!("eq_band_{i}={:.1}\n", g.clamp(-12.0, 12.0)));
    }
    out.push_str(&format!("pure_bass_enabled={}\n", b(p.pure_bass_enabled)));
    out.push_str(&format!("pure_bass_mode={}\n", p.pure_bass_mode.clamp(0, 2)));
    out.push_str(&format!(
        "pure_bass_gain_db={:.1}\n",
        p.pure_bass_gain_db.clamp(0.0, 12.0)
    ));
    out.push_str(&format!("clarity_enabled={}\n", b(p.clarity_enabled)));
    out.push_str(&format!("clarity_mode={}\n", p.clarity_mode.clamp(0, 2)));
    out.push_str(&format!(
        "clarity_gain_db={:.1}\n",
        p.clarity_gain_db.clamp(0.0, 10.0)
    ));
    out.push_str(&format!("surround_enabled={}\n", b(p.surround_enabled)));
    out.push_str(&format!(
        "surround_level={:.2}\n",
        (p.surround_level_pct.clamp(0, 100) as f32) / 100.0
    ));
    out.push_str(&format!("binaural_enabled={}\n", b(p.binaural_enabled)));
    out.push_str(&format!("binaural_mode={}\n", p.binaural_mode.clamp(0, 2)));
    out.push_str("crossfeed_level=0.30\npan_balance=0.0\norbit_speed=0.35\n");
    out.push_str(&format!("tube_enabled={}\n", b(p.tube_enabled)));
    out.push_str(&format!(
        "tube_warmth={:.2}\n",
        (p.tube_warmth_pct.clamp(0, 100) as f32) / 100.0
    ));
    out.push_str(&format!("convolver_enabled={}\n", b(p.convolver_enabled)));
    out.push_str(&format!(
        "convolver_preset={}\n",
        p.convolver_preset.clamp(0, 5)
    ));
    out.push_str(&format!("reverb_enabled={}\n", b(p.reverb_enabled)));
    out.push_str(&format!("reverb_preset={}\n", p.reverb_preset.clamp(0, 6)));
    out.push_str(&format!("chorus_enabled={}\n", b(p.chorus_enabled)));
    out.push_str(&format!("delay_enabled={}\n", b(p.delay_enabled)));
    out.push_str(&format!("loudness_enabled={}\n", b(p.loudness_enabled)));
    out.push_str("loudness_gain_db=2.0\n");
    out.push_str(&format!(
        "preamp_db={:.1}\n",
        p.preamp_db.clamp(-12.0, 12.0)
    ));
    out.push_str("limiter_threshold_db=-0.5\n");
    out
}

pub fn parse_resonance_conf(raw: &str) -> ResonanceParams {
    let mut p = ResonanceParams::default();
    let is_on = |v: &str| matches!(v, "1" | "true" | "yes" | "on");
    for line in raw.lines() {
        let Some((k, v)) = line.trim().split_once('=') else {
            continue;
        };
        let val = v.trim();
        match k.trim() {
            "master_enabled" => p.master_enabled = is_on(val),
            "eq_enabled" => p.eq_enabled = is_on(val),
            k if k.starts_with("eq_band_") => {
                if let Ok(idx) = k[8..].parse::<usize>() {
                    if idx < 10 {
                        if let Ok(g) = val.parse::<f32>() {
                            p.eq_bands[idx] = g.clamp(-12.0, 12.0);
                        }
                    }
                }
            }
            "pure_bass_enabled" => p.pure_bass_enabled = is_on(val),
            "pure_bass_mode" => p.pure_bass_mode = val.parse::<i32>().unwrap_or(1).clamp(0, 2),
            "pure_bass_gain_db" => {
                p.pure_bass_gain_db = val.parse::<f32>().unwrap_or(3.5).clamp(0.0, 12.0)
            }
            "clarity_enabled" => p.clarity_enabled = is_on(val),
            "clarity_mode" => p.clarity_mode = val.parse::<i32>().unwrap_or(0).clamp(0, 2),
            "clarity_gain_db" => {
                p.clarity_gain_db = val.parse::<f32>().unwrap_or(2.0).clamp(0.0, 10.0)
            }
            "surround_enabled" => p.surround_enabled = is_on(val),
            "surround_level" => {
                p.surround_level_pct =
                    ((val.parse::<f32>().unwrap_or(0.35) * 100.0).round() as i32).clamp(0, 100)
            }
            "binaural_enabled" => p.binaural_enabled = is_on(val),
            "binaural_mode" => p.binaural_mode = val.parse::<i32>().unwrap_or(0).clamp(0, 2),
            "tube_enabled" => p.tube_enabled = is_on(val),
            "tube_warmth" => {
                p.tube_warmth_pct =
                    ((val.parse::<f32>().unwrap_or(0.40) * 100.0).round() as i32).clamp(0, 100)
            }
            "convolver_enabled" => p.convolver_enabled = is_on(val),
            "convolver_preset" => p.convolver_preset = val.parse::<i32>().unwrap_or(0).clamp(0, 5),
            "reverb_enabled" => p.reverb_enabled = is_on(val),
            "reverb_preset" => p.reverb_preset = val.parse::<i32>().unwrap_or(0).clamp(0, 6),
            "chorus_enabled" => p.chorus_enabled = is_on(val),
            "delay_enabled" => p.delay_enabled = is_on(val),
            "loudness_enabled" => p.loudness_enabled = is_on(val),
            "preamp_db" => p.preamp_db = val.parse::<f32>().unwrap_or(0.0).clamp(-12.0, 12.0),
            _ => {}
        }
    }
    p
}

pub fn resonance_conf_path() -> PathBuf {
    persistent_display_config_dir().join("resonance-fx.conf")
}

pub fn load_resonance_params() -> ResonanceParams {
    fs::read_to_string(resonance_conf_path())
        .map(|raw| parse_resonance_conf(&raw))
        .unwrap_or_default()
}

pub fn save_and_sync_resonance_params(p: &ResonanceParams) -> Result<String, String> {
    let dir = persistent_display_config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed creating config dir: {e}"))?;
    let path = dir.join("resonance-fx.conf");
    let content = render_resonance_conf(p);
    fs::write(&path, &content).map_err(|e| format!("Failed writing {}: {e}", path.display()))?;
    if let Ok(rt) = std::env::var("XDG_RUNTIME_DIR") {
        let _ = fs::write(Path::new(&rt).join("unicore-resonance-fx.conf"), &content);
    }
    if cfg!(target_os = "linux") && Path::new(RESONANCE_CTL_BIN).is_file() {
        let _ = Command::new(RESONANCE_CTL_BIN)
            .arg("sync")
            .stdin(Stdio::null())
            .output();
    }
    Ok(format!("Applied ResonanceFX DSP settings ({})", path.display()))
}

pub fn read_resonance_status() -> ResonanceStatus {
    if !cfg!(target_os = "linux") || !Path::new(RESONANCE_CTL_BIN).is_file() {
        return ResonanceStatus::default();
    }
    Command::new(RESONANCE_CTL_BIN)
        .arg("status")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_resonance_status(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

pub fn run_resonance_action(action: &str) -> Result<String, String> {
    if !cfg!(target_os = "linux") || !Path::new(RESONANCE_CTL_BIN).is_file() {
        return Ok(format!("ResonanceFX '{action}' completed (dry-run)"));
    }
    let out = Command::new(RESONANCE_CTL_BIN)
        .arg(action)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Failed to run ResonanceFX '{action}': {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() {
            format!("ResonanceFX '{action}' exited with {}", out.status)
        } else {
            err
        })
    }
}
