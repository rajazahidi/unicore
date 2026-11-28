use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use aegis_proto::obfuscator::{AmneziaObfuscator, ObfuscationConfig};
use aegis_proto::pq_kem::{server_respond_pq, PqControlPacket};
use aegis_proto::reality::{
    build_reality_server_hello, parse_short_id_8, read_reality_frame, read_tls_handshake_record,
    verify_reality_client_hello_with_replay, write_reality_frame, RealityReplayCache,
    REALITY_FRAME_PING, REALITY_FRAME_WG,
};
use boringtun::noise::TunnResult;
use x25519_dalek::{PublicKey, StaticSecret};

use crate::dns::doh_forwarder::resolve_loopback_dns;
use crate::dns::sinkhole::parse_dns_query;
use crate::packet::ip_parser::{
    build_ipv4_icmp_echo_reply, build_ipv4_udp_packet, parse_ip_packet, parse_udp, IpVersion,
    TransportProto,
};
use crate::tunnel::wireguard::{decode_key_32, generate_keypair_base64, DecapAction, WireGuardSession};

pub type ProtectSocketFn = Arc<dyn Fn(i32) -> bool + Send + Sync>;

pub struct LoopbackPeer {
    pub local_addr: SocketAddr,
    pub reality_addr: SocketAddr,
    pub server_public_key_b64: String,
    pub reality_server_pub: [u8; 32],
    pub reality_short_id: [u8; 8],
    stop_flag: Arc<AtomicBool>,
    tasks: Vec<JoinHandle<()>>,
}

struct LoopbackCore {
    session: WireGuardSession,
    obf: AmneziaObfuscator,
    base_psk: Option<[u8; 32]>,
    echo_ip: Ipv4Addr,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl LoopbackPeer {
    pub async fn spawn(
        client_public_key_b64: String,
        preshared_key_b64: Option<String>,
        obfuscation: ObfuscationConfig,
        reality_short_id_hex: String,
        upstream_dns: Ipv4Addr,
        echo_ip: Ipv4Addr,
        protect_cb: Option<ProtectSocketFn>,
    ) -> Result<Self, String> {
        let (server_priv_b64, server_pub_b64) = generate_keypair_base64();
        let udp_socket = UdpSocket::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("Failed to bind loopback peer UDP socket: {e}"))?;
        let local_addr = udp_socket
            .local_addr()
            .map_err(|e| format!("Failed to get loopback peer addr: {e}"))?;

        let reality_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("Failed to bind loopback REALITY TCP listener: {e}"))?;
        let reality_addr = reality_listener
            .local_addr()
            .map_err(|e| format!("Failed to get loopback REALITY addr: {e}"))?;

        let reality_secret = StaticSecret::random_from_rng(rand::thread_rng());
        let reality_pub = PublicKey::from(&reality_secret);
        let reality_priv_bytes = reality_secret.to_bytes();
        let reality_pub_bytes = *reality_pub.as_bytes();
        let reality_short_id = parse_short_id_8(&reality_short_id_hex)
            .unwrap_or_else(|_| parse_short_id_8("aegis001aegis001").unwrap());

        let base_psk = match preshared_key_b64.as_deref() {
            Some(s) if !s.trim().is_empty() => Some(decode_key_32(s)?),
            _ => None,
        };

        let session = WireGuardSession::new(
            &server_priv_b64,
            &client_public_key_b64,
            preshared_key_b64.as_deref(),
            0,
            77,
        )?;
        let obf = AmneziaObfuscator::new(obfuscation);
        let core = Arc::new(Mutex::new(LoopbackCore {
            session,
            obf,
            base_psk,
            echo_ip,
        }));

        let stop_flag = Arc::new(AtomicBool::new(false));
        let mut tasks = Vec::with_capacity(2);

        // 1. UDP AmneziaWG Loopback Task
        {
            let core_udp = core.clone();
            let stop_udp = stop_flag.clone();
            let protect_udp = protect_cb.clone();
            let udp_task = tokio::spawn(async move {
                let mut rx_buf = vec![0u8; 4096];
                while !stop_udp.load(Ordering::Relaxed) {
                    let recv = tokio::time::timeout(
                        std::time::Duration::from_millis(200),
                        udp_socket.recv_from(&mut rx_buf),
                    )
                    .await;

                    let (n, peer_addr) = match recv {
                        Ok(Ok(pair)) => pair,
                        Ok(Err(_)) => break,
                        Err(_) => continue,
                    };

                    let replies = Self::process_wireguard_datagram(
                        &core_udp,
                        peer_addr.ip(),
                        &rx_buf[..n],
                        upstream_dns,
                        echo_ip,
                        &protect_udp,
                    )
                    .await;
                    for pkt in replies {
                        let _ = udp_socket.send_to(&pkt, peer_addr).await;
                    }
                }
            });
            tasks.push(udp_task);
        }

        // 2. TCP/443 REALITY TLS 1.3 Loopback Task
        {
            let core_tcp = core;
            let stop_tcp = stop_flag.clone();
            let protect_tcp = protect_cb;
            let replay_cache = Arc::new(StdMutex::new(RealityReplayCache::new()));
            let tcp_task = tokio::spawn(async move {
                while !stop_tcp.load(Ordering::Relaxed) {
                    let accept_res = tokio::time::timeout(
                        std::time::Duration::from_millis(200),
                        reality_listener.accept(),
                    )
                    .await;
                    let (stream, peer_addr) = match accept_res {
                        Ok(Ok(v)) => v,
                        Ok(Err(_)) => break,
                        Err(_) => continue,
                    };
                    let core_conn = core_tcp.clone();
                    let stop_conn = stop_tcp.clone();
                    let protect_conn = protect_tcp.clone();
                    let cache_conn = replay_cache.clone();
                    tokio::spawn(async move {
                        Self::handle_reality_conn(
                            stream,
                            peer_addr,
                            reality_priv_bytes,
                            reality_short_id,
                            cache_conn,
                            core_conn,
                            upstream_dns,
                            echo_ip,
                            protect_conn,
                            stop_conn,
                        )
                        .await;
                    });
                }
            });
            tasks.push(tcp_task);
        }

        Ok(Self {
            local_addr,
            reality_addr,
            server_public_key_b64: server_pub_b64,
            reality_server_pub: reality_pub_bytes,
            reality_short_id,
            stop_flag,
            tasks,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_reality_conn(
        mut stream: TcpStream,
        peer_addr: SocketAddr,
        server_priv: [u8; 32],
        short_id: [u8; 8],
        replay_cache: Arc<StdMutex<RealityReplayCache>>,
        core: Arc<Mutex<LoopbackCore>>,
        upstream_dns: Ipv4Addr,
        echo_ip: Ipv4Addr,
        protect_cb: Option<ProtectSocketFn>,
        stop_flag: Arc<AtomicBool>,
    ) {
        let _ = stream.set_nodelay(true);
        let ch_record = match tokio::time::timeout(
            std::time::Duration::from_secs(2),
            read_tls_handshake_record(&mut stream),
        )
        .await
        {
            Ok(Ok(rec)) => rec,
            _ => return,
        };

        let verified = {
            let mut guard = replay_cache.lock().expect("replay cache lock");
            verify_reality_client_hello_with_replay(
                &ch_record,
                &server_priv,
                &[short_id],
                now_secs(),
                &mut guard,
            )
        };
        let Some(verified) = verified else {
            // Mirror active probe behavior by closing invalid/replayed handshake gracefully in lab mode
            return;
        };

        let (sh, cipher) = build_reality_server_hello(&verified);
        if stream.write_all(&sh).await.is_err() {
            return;
        }

        let (mut tcp_rx, mut tcp_tx) = stream.into_split();
        let (mut enc_tx, mut dec_rx) = cipher.split();
        let (out_tx, mut out_rx) = mpsc::channel::<(u8, Vec<u8>)>(128);

        let writer = tokio::spawn(async move {
            while let Some((ft, payload)) = out_rx.recv().await {
                if write_reality_frame(&mut tcp_tx, &mut enc_tx, ft, &payload)
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        while !stop_flag.load(Ordering::Relaxed) {
            let Ok((frame_type, payload)) = read_reality_frame(&mut tcp_rx, &mut dec_rx).await else {
                break;
            };
            match frame_type {
                REALITY_FRAME_PING => {
                    if out_tx.send((REALITY_FRAME_PING, payload)).await.is_err() {
                        break;
                    }
                }
                REALITY_FRAME_WG => {
                    let replies = Self::process_wireguard_datagram(
                        &core,
                        peer_addr.ip(),
                        &payload,
                        upstream_dns,
                        echo_ip,
                        &protect_cb,
                    )
                    .await;
                    for pkt in replies {
                        if out_tx.send((REALITY_FRAME_WG, pkt)).await.is_err() {
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
        writer.abort();
    }

    async fn process_wireguard_datagram(
        core: &Arc<Mutex<LoopbackCore>>,
        src_ip: IpAddr,
        datagram: &[u8],
        upstream_dns: Ipv4Addr,
        echo_ip: Ipv4Addr,
        protect_cb: &Option<ProtectSocketFn>,
    ) -> Vec<Vec<u8>> {
        let mut outbound = Vec::new();
        let mut inbound_ip_pkt: Option<Vec<u8>> = None;

        {
            let mut guard = core.lock().await;
            let Some(wg_pkt) = guard.obf.deobfuscate_inbound(datagram) else {
                return outbound;
            };

            match guard.session.decapsulate(Some(src_ip), &wg_pkt) {
                DecapAction::WriteToNetwork(reply) => {
                    let (_, obf_reply) = guard.obf.obfuscate_outbound(&reply);
                    outbound.push(obf_reply);
                    while let DecapAction::WriteToNetwork(pkt) = guard.session.decapsulate(None, &[]) {
                        let (_, obf_pkt) = guard.obf.obfuscate_outbound(&pkt);
                        outbound.push(obf_pkt);
                    }
                }
                DecapAction::WriteToTunnelV4(ip_pkt) => {
                    // PQ control messages handled synchronously inside lock cleanly
                    if let Some(pq_reply) =
                        Self::try_handle_pq_control(&mut guard, &ip_pkt)
                    {
                        let mut enc_buf = vec![0u8; 4096];
                        if let TunnResult::WriteToNetwork(enc_reply) =
                            guard.session.encapsulate(&pq_reply, &mut enc_buf)
                        {
                            let (_, obf_reply) = guard.obf.obfuscate_outbound(enc_reply);
                            outbound.push(obf_reply);
                        }
                    } else {
                        inbound_ip_pkt = Some(ip_pkt);
                    }
                }
                DecapAction::Done | DecapAction::WriteToTunnelV6(_) => {}
                DecapAction::Err(e) => {
                    log::debug!("Loopback peer decapsulate warning: {e}");
                }
            }
        }

        if let Some(ip_pkt) = inbound_ip_pkt {
            let protect_clone = protect_cb.clone();
            let reply_ip = tokio::task::spawn_blocking(move || {
                Self::handle_lab_ip_packet(
                    &ip_pkt,
                    upstream_dns,
                    echo_ip,
                    protect_clone.as_deref(),
                )
            })
            .await
            .ok()
            .flatten();

            if let Some(reply_ip) = reply_ip {
                let mut guard = core.lock().await;
                let mut enc_buf = vec![0u8; 4096];
                if let TunnResult::WriteToNetwork(enc_reply) =
                    guard.session.encapsulate(&reply_ip, &mut enc_buf)
                {
                    let (_, obf_reply) = guard.obf.obfuscate_outbound(enc_reply);
                    outbound.push(obf_reply);
                }
            }
        }

        outbound
    }

    fn try_handle_pq_control(core: &mut LoopbackCore, ip_pkt: &[u8]) -> Option<Vec<u8>> {
        let parsed = parse_ip_packet(ip_pkt)?;
        if parsed.version != IpVersion::V4 || parsed.proto != TransportProto::Udp {
            return None;
        }
        let src_ip = parsed.src_v4?;
        let dst_ip = parsed.dst_v4?;
        if dst_ip != core.echo_ip {
            return None;
        }
        let udp = parse_udp(parsed.payload)?;
        if udp.dst_port != 5556 {
            return None;
        }
        let pkt = PqControlPacket::decode(udp.payload)?;
        match pkt {
            PqControlPacket::Init(init) => {
                let (resp, next_psk) = server_respond_pq(&init, core.base_psk).ok()?;
                core.session.stage_pq_rekey(init.epoch, next_psk, true);
                let encoded = PqControlPacket::Resp(resp).encode();
                Some(build_ipv4_udp_packet(
                    dst_ip,
                    src_ip,
                    5556,
                    udp.src_port,
                    &encoded,
                ))
            }
            PqControlPacket::Commit(commit) => {
                let ack = PqControlPacket::Ack(commit).encode();
                Some(build_ipv4_udp_packet(
                    dst_ip,
                    src_ip,
                    5556,
                    udp.src_port,
                    &ack,
                ))
            }
            PqControlPacket::Resp(_) | PqControlPacket::Ack(_) => None,
        }
    }

    fn handle_lab_ip_packet(
        ip_pkt: &[u8],
        upstream_dns: Ipv4Addr,
        echo_ip: Ipv4Addr,
        protect_cb: Option<&(dyn Fn(i32) -> bool + Send + Sync)>,
    ) -> Option<Vec<u8>> {
        let parsed = parse_ip_packet(ip_pkt)?;
        if parsed.version != IpVersion::V4 {
            return None;
        }
        let src_ip = parsed.src_v4?;
        let dst_ip = parsed.dst_v4?;

        match parsed.proto {
            TransportProto::Icmp => build_ipv4_icmp_echo_reply(ip_pkt),
            TransportProto::Udp => {
                let udp = parse_udp(parsed.payload)?;
                if udp.dst_port == 53 {
                    let q = parse_dns_query(udp.payload)?;
                    let dns_reply = resolve_loopback_dns(
                        udp.payload,
                        &q,
                        upstream_dns,
                        echo_ip,
                        protect_cb.map(|f| f as &dyn Fn(i32) -> bool),
                    )?;
                    Some(build_ipv4_udp_packet(
                        dst_ip,
                        src_ip,
                        udp.dst_port,
                        udp.src_port,
                        &dns_reply,
                    ))
                } else if udp.dst_port == 5555 || udp.dst_port == 7 || dst_ip == echo_ip {
                    Some(build_ipv4_udp_packet(
                        dst_ip,
                        src_ip,
                        udp.dst_port,
                        udp.src_port,
                        udp.payload,
                    ))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn stop(self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        for task in self.tasks {
            task.abort();
        }
    }
}
