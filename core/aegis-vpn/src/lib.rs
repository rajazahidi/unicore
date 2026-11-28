//! UniCore OS Native AegisTunnel VPN Core (headless Linux / cross-platform, zero JNI).
//!
//! Embeds the pure Rust WireGuard (`boringtun`), AmneziaWG Anti-DPI, REALITY TCP/443,
//! Hybrid Post-Quantum X25519 + ML-KEM-768, Local DNS Sinkhole, and Zero-Drop Handover engine.

pub mod config;
pub mod dns;
pub mod handover;
pub mod packet;
pub mod tunnel;

use aegis_proto::obfuscator::{AmneziaObfuscator, ObfuscationConfig};
use aegis_proto::pq_kem::{server_respond_pq, ClientPqExchange, PqControlPacket};
use aegis_proto::reality::{parse_short_id_8, RealityConfig};
use serde::Serialize;

use crate::config::{EngineConfig, HandoverConfig, TransportMode};
use crate::dns::sinkhole::DnsSinkhole;
use crate::tunnel::wireguard::generate_keypair_base64;

#[derive(Debug, Clone, Serialize)]
pub struct SelfTestReport {
    pub keygen_ok: bool,
    pub amnezia_dpi_ok: bool,
    pub pq_mlkem768_ok: bool,
    pub reality_short_id_ok: bool,
    pub dns_sinkhole_ok: bool,
    pub all_passed: bool,
}

/// Exercise all core AegisTunnel cryptographic, anti-DPI, post-quantum, and DNS sinkhole primitives.
pub fn run_self_test() -> SelfTestReport {
    let (priv_b64, pub_b64) = generate_keypair_base64();
    let keygen_ok = priv_b64.len() >= 40 && pub_b64.len() >= 40 && priv_b64 != pub_b64;

    let obf = AmneziaObfuscator::new(ObfuscationConfig::default());
    let mut wg_init = vec![0xA5u8; 148];
    wg_init[0..4].copy_from_slice(&1u32.to_le_bytes());
    let (is_init, obf_pkt) = obf.obfuscate_outbound(&wg_init);
    let amnezia_dpi_ok =
        obf.is_active() && is_init && obf.deobfuscate_inbound(&obf_pkt).as_deref() == Some(&wg_init[..]);

    let (client_pq, init_msg) = ClientPqExchange::begin(1);
    let wire_init = PqControlPacket::Init(init_msg).encode();
    let pq_mlkem768_ok = match PqControlPacket::decode(&wire_init) {
        Some(PqControlPacket::Init(dec_init)) => match server_respond_pq(&dec_init, None) {
            Ok((resp_msg, srv_psk)) => match client_pq.finish(&resp_msg, None) {
                Ok(cli_psk) => cli_psk == srv_psk && cli_psk != [0u8; 32],
                Err(_) => false,
            },
            Err(_) => false,
        },
        _ => false,
    };

    let reality_short_id_ok = parse_short_id_8("0123456789abcdef").is_ok();

    let sinkhole = DnsSinkhole::new(
        true,
        &["ads.tracker.example".to_string(), "telemetry.bad".to_string()],
        &["safe.ads.tracker.example".to_string()],
    );
    let dns_sinkhole_ok = sinkhole.is_blocked("sub.ads.tracker.example")
        && !sinkhole.is_blocked("safe.ads.tracker.example")
        && !sinkhole.is_blocked("unicore.example");

    let all_passed =
        keygen_ok && amnezia_dpi_ok && pq_mlkem768_ok && reality_short_id_ok && dns_sinkhole_ok;

    SelfTestReport {
        keygen_ok,
        amnezia_dpi_ok,
        pq_mlkem768_ok,
        reality_short_id_ok,
        dns_sinkhole_ok,
        all_passed,
    }
}

/// Default ready-to-edit AegisTunnel configuration (Loopback demo enabled until user sets real peer).
pub fn default_engine_config() -> EngineConfig {
    let (priv_key, peer_pub) = generate_keypair_base64();
    EngineConfig {
        private_key_base64: priv_key,
        peer_public_key_base64: peer_pub,
        preshared_key_base64: None,
        endpoint: "127.0.0.1:51820".to_string(),
        tunnel_ipv4: "10.66.0.2".to_string(),
        virtual_dns_ipv4: "10.66.0.1".to_string(),
        upstream_dns_ipv4: "1.1.1.1".to_string(),
        persistent_keepalive: 25,
        loopback_mode: true,
        dns_sinkhole_enabled: true,
        obfuscation: ObfuscationConfig::default(),
        handover: HandoverConfig::default(),
        transport_mode: TransportMode::AutoHappyEyeballs,
        reality: RealityConfig::default(),
        pq_enabled: true,
        pq_rekey_interval_secs: 600,
        blocked_domains: vec![
            "doubleclick.net".to_string(),
            "googlesyndication.com".to_string(),
            "ads.facebook.com".to_string(),
            "analytics.tiktok.com".to_string(),
            "telemetry.microsoft.com".to_string(),
        ],
        allowed_domains: Vec::new(),
    }
}

/// Parse a standard WireGuard / AmneziaWG `.conf` file into an AegisTunnel `EngineConfig`.
pub fn parse_wg_conf_to_engine_config(raw: &str) -> Result<EngineConfig, String> {
    let mut cfg = default_engine_config();
    cfg.loopback_mode = false;
    let mut obf = ObfuscationConfig::default();
    let mut saw_priv = false;
    let mut saw_pub = false;
    let mut saw_obf_key = false;

    for line in raw.lines() {
        let trimmed = line.split('#').next().unwrap_or("").trim();
        if trimmed.is_empty() || trimmed.starts_with('[') {
            continue;
        }
        let Some((k, v)) = trimmed.split_once('=') else {
            continue;
        };
        let key = k.trim().to_ascii_lowercase();
        let val = v.trim();
        match key.as_str() {
            "privatekey" => {
                cfg.private_key_base64 = val.to_string();
                saw_priv = true;
            }
            "publickey" => {
                cfg.peer_public_key_base64 = val.to_string();
                saw_pub = true;
            }
            "presharedkey" => {
                if !val.is_empty() {
                    cfg.preshared_key_base64 = Some(val.to_string());
                }
            }
            "endpoint" => {
                cfg.endpoint = val.to_string();
            }
            "address" => {
                let first = val.split(',').next().unwrap_or(val).trim();
                let ip = first.split('/').next().unwrap_or(first).trim();
                if !ip.is_empty() {
                    cfg.tunnel_ipv4 = ip.to_string();
                }
            }
            "dns" => {
                let first = val.split(',').next().unwrap_or(val).trim();
                if !first.is_empty() {
                    cfg.upstream_dns_ipv4 = first.to_string();
                }
            }
            "persistentkeepalive" => {
                if let Ok(sec) = val.parse::<u16>() {
                    cfg.persistent_keepalive = sec;
                }
            }
            "jc" => {
                if let Ok(n) = val.parse::<u8>() {
                    obf.jc = n;
                    saw_obf_key = true;
                }
            }
            "jmin" => {
                if let Ok(n) = val.parse::<u16>() {
                    obf.jmin = n;
                    saw_obf_key = true;
                }
            }
            "jmax" => {
                if let Ok(n) = val.parse::<u16>() {
                    obf.jmax = n;
                    saw_obf_key = true;
                }
            }
            "s1" => {
                if let Ok(n) = val.parse::<u16>() {
                    obf.s1 = n;
                    saw_obf_key = true;
                }
            }
            "s2" => {
                if let Ok(n) = val.parse::<u16>() {
                    obf.s2 = n;
                    saw_obf_key = true;
                }
            }
            "h1" => {
                if let Ok(n) = val.parse::<u32>() {
                    obf.h1 = n;
                    saw_obf_key = true;
                }
            }
            "h2" => {
                if let Ok(n) = val.parse::<u32>() {
                    obf.h2 = n;
                    saw_obf_key = true;
                }
            }
            "h3" => {
                if let Ok(n) = val.parse::<u32>() {
                    obf.h3 = n;
                    saw_obf_key = true;
                }
            }
            "h4" => {
                if let Ok(n) = val.parse::<u32>() {
                    obf.h4 = n;
                    saw_obf_key = true;
                }
            }
            _ => {}
        }
    }

    if !saw_priv || !saw_pub {
        return Err("Invalid WireGuard/AmneziaWG profile: missing PrivateKey or PublicKey".to_string());
    }
    obf.enabled = saw_obf_key;
    cfg.obfuscation = obf;
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aegis_self_test_and_wg_parser_pass() {
        let rep = run_self_test();
        assert!(rep.keygen_ok);
        assert!(rep.amnezia_dpi_ok);
        assert!(rep.pq_mlkem768_ok);
        assert!(rep.reality_short_id_ok);
        assert!(rep.dns_sinkhole_ok);
        assert!(rep.all_passed);

        let sample = "\
[Interface]
PrivateKey = AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
Address = 10.77.0.5/32
DNS = 9.9.9.9
Jc = 5
S1 = 30
S2 = 60

[Peer]
PublicKey = BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=
Endpoint = 203.0.113.10:51820
PersistentKeepalive = 20
";
        let parsed = parse_wg_conf_to_engine_config(sample).unwrap();
        assert_eq!(parsed.tunnel_ipv4, "10.77.0.5");
        assert_eq!(parsed.upstream_dns_ipv4, "9.9.9.9");
        assert_eq!(parsed.endpoint, "203.0.113.10:51820");
        assert!(parsed.obfuscation.enabled);
        assert_eq!(parsed.obfuscation.jc, 5);
        assert_eq!(parsed.obfuscation.s1, 30);
    }
}
