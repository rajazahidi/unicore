pub mod loopback;
pub mod wireguard;

use std::collections::VecDeque;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use aegis_proto::obfuscator::AmneziaObfuscator;
use aegis_proto::pq_kem::{ClientPqExchange, PqCommitMessage, PqControlPacket};
use aegis_proto::reality::parse_short_id_8;
use boringtun::noise::TunnResult;
use tokio::sync::mpsc;

use crate::config::{
    DnsEventRecord, EngineConfig, TelemetrySnapshot, TransportMode,
};
use crate::dns::sinkhole::{build_sinkhole_response, parse_dns_query, DnsSinkhole};
use crate::handover::path_scorer::HandoverAdvice;
use crate::handover::socket_pool::{
    BindSocketFn, InboundDatagram, RealityTarget, SocketPool, TransportProtocol,
};
use crate::handover::{
    build_inner_probe_packet, now_epoch_ms, parse_inner_probe_reply, HandoverCoordinator,
};
use crate::packet::ip_parser::{
    build_ipv4_udp_packet, parse_ip_packet, parse_udp, IpVersion, TransportProto,
};
use loopback::LoopbackPeer;
use wireguard::{decode_key_32, generate_keypair_base64, DecapAction, WireGuardSession};

const PQ_CONTROL_PORT: u16 = 5556;
const UDP_BLOCK_FALLBACK_MS: u64 = 600;
const INITIAL_HANDSHAKE_FALLBACK_MS: u64 = 900;
const UDP_RECOVERY_PROBE_INTERVAL_SECS: u64 = 30;

pub enum EngineCommand {
    StandbyNetworkAvailable {
        net_id: i64,
        label: String,
        cutover_immediately: bool,
    },
    PrimaryNetworkLost {
        lost_net_id: i64,
    },
    UpdateWifiRssi(i32),
    TriggerHandoverTest {
        next_label: String,
    },
    UpdateBlocklist {
        blocked: Vec<String>,
        allowed: Vec<String>,
    },
    SetSimulateUdpBlocked(bool),
    RotatePqKey,
    SetTransportMode(TransportMode),
    Stop,
}

pub enum TunInput {
    #[cfg(unix)]
    RawFd(i32),
    Transport(TunTransport),
}

pub enum TunTransport {
    #[cfg(unix)]
    UnixFd(tokio::io::unix::AsyncFd<std::os::fd::OwnedFd>),
    Channel {
        inbound_rx: mpsc::Receiver<Vec<u8>>,
        outbound_tx: mpsc::Sender<Vec<u8>>,
    },
}

// Clean wrapper so we don't need extra proc-macro crate for async_trait
impl TunTransport {
    #[cfg(unix)]
    pub fn from_raw_fd(raw_fd: i32) -> Result<Self, String> {
        use std::os::fd::{FromRawFd, OwnedFd};
        unsafe {
            let flags = libc::fcntl(raw_fd, libc::F_GETFL);
            if flags < 0 {
                return Err("fcntl F_GETFL failed on TUN fd".to_string());
            }
            if libc::fcntl(raw_fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
                return Err("fcntl F_SETFL O_NONBLOCK failed on TUN fd".to_string());
            }
            let owned = OwnedFd::from_raw_fd(raw_fd);
            let async_fd = tokio::io::unix::AsyncFd::register(owned)
                .map_err(|e| format!("AsyncFd::register failed: {e}"))?;
            Ok(Self::UnixFd(async_fd))
        }
    }

    pub async fn read_packet(&mut self, buf: &mut [u8]) -> Option<usize> {
        match self {
            #[cfg(unix)]
            TunTransport::UnixFd(afd) => loop {
                let mut guard = afd.readable().await.ok()?;
                match guard.try_io(|inner| {
                    use std::os::fd::AsRawFd;
                    let n = unsafe {
                        libc::read(
                            inner.get_ref().as_raw_fd(),
                            buf.as_mut_ptr() as *mut libc::c_void,
                            buf.len(),
                        )
                    };
                    if n < 0 {
                        Err(std::io::Error::last_os_error())
                    } else {
                        Ok(n as usize)
                    }
                }) {
                    Ok(Ok(n)) => return Some(n),
                    Ok(Err(_)) => return None,
                    Err(_would_block) => continue,
                }
            },
            TunTransport::Channel { inbound_rx, .. } => {
                let pkt = inbound_rx.recv().await?;
                let len = pkt.len().min(buf.len());
                buf[..len].copy_from_slice(&pkt[..len]);
                Some(len)
            }
        }
    }

    pub async fn write_packet(&self, pkt: &[u8]) -> bool {
        match self {
            #[cfg(unix)]
            TunTransport::UnixFd(afd) => {
                use std::os::fd::AsRawFd;
                let mut attempts = 0;
                loop {
                    let Ok(mut guard) = afd.writable().await else {
                        return false;
                    };
                    match guard.try_io(|inner| {
                        let n = unsafe {
                            libc::write(
                                inner.get_ref().as_raw_fd(),
                                pkt.as_ptr() as *const libc::c_void,
                                pkt.len(),
                            )
                        };
                        if n < 0 {
                            Err(std::io::Error::last_os_error())
                        } else {
                            Ok(n as usize)
                        }
                    }) {
                        Ok(Ok(_)) => return true,
                        Ok(Err(_)) => return false,
                        Err(_would_block) => {
                            attempts += 1;
                            if attempts > 5 {
                                return false;
                            }
                        }
                    }
                }
            }
            TunTransport::Channel { outbound_tx, .. } => {
                outbound_tx.send(pkt.to_vec()).await.is_ok()
            }
        }
    }
}

struct PendingProbe {
    seq: u64,
    started: Instant,
    slot_idx: usize,
    cutover_on_reply: bool,
    reason: String,
    clear_udp_promote_on_cutover: bool,
}

pub struct EngineHandle {
    pub cmd_tx: mpsc::Sender<EngineCommand>,
    pub telemetry: Arc<RwLock<TelemetrySnapshot>>,
    pub stop_flag: Arc<AtomicBool>,
    runtime: Option<tokio::runtime::Runtime>,
}

impl EngineHandle {
    pub fn snapshot(&self) -> TelemetrySnapshot {
        self.telemetry
            .read()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    pub fn stop(mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        let _ = self.cmd_tx.try_send(EngineCommand::Stop);
        if let Some(rt) = self.runtime.take() {
            if tokio::runtime::Handle::try_current().is_ok() {
                let _ = std::thread::spawn(move || {
                    rt.shutdown_timeout(Duration::from_millis(800));
                })
                .join();
            } else {
                rt.shutdown_timeout(Duration::from_millis(800));
            }
        }
    }
}

pub fn start_engine(
    cfg: EngineConfig,
    tun_input: TunInput,
    primary_net_id: i64,
    primary_label: String,
    bind_cb: BindSocketFn,
) -> Result<EngineHandle, String> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| format!("Failed to build Tokio runtime: {e}"))?;

    // Register AsyncFd inside the active Tokio reactor context
    let tun: TunTransport = rt.block_on(async move {
        match tun_input {
            #[cfg(unix)]
            TunInput::RawFd(fd) => TunTransport::from_raw_fd(fd),
            TunInput::Transport(t) => Ok::<TunTransport, String>(t),
        }
    })?;

    let (cmd_tx, cmd_rx) = mpsc::channel::<EngineCommand>(64);
    let initial_proto = match cfg.transport_mode {
        TransportMode::ForceRealityTcp => TransportProtocol::RealityTcp.display_name().to_string(),
        _ => TransportProtocol::UdpAmneziaWg.display_name().to_string(),
    };
    let initial_cipher = if cfg.pq_enabled {
        "X25519 + ML-KEM-768".to_string()
    } else {
        "X25519 (Classical)".to_string()
    };
    let telemetry = Arc::new(RwLock::new(TelemetrySnapshot {
        connected: true,
        active_path_label: primary_label.clone(),
        active_net_id: primary_net_id,
        transport_protocol: initial_proto,
        pq_cipher_suite: initial_cipher,
        ..Default::default()
    }));
    let stop_flag = Arc::new(AtomicBool::new(false));

    let tel_clone = telemetry.clone();
    let stop_clone = stop_flag.clone();

    rt.spawn(async move {
        if let Err(e) = run_tunnel_loop(
            cfg,
            tun,
            primary_net_id,
            primary_label,
            bind_cb,
            cmd_rx,
            tel_clone.clone(),
            stop_clone,
        )
        .await
        {
            log::error!("AegisTunnel engine exited with error: {e}");
        }
        if let Ok(mut guard) = tel_clone.write() {
            guard.connected = false;
        }
    });

    Ok(EngineHandle {
        cmd_tx,
        telemetry,
        stop_flag,
        runtime: Some(rt),
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn run_tunnel_loop(
    cfg: EngineConfig,
    mut tun: TunTransport,
    primary_net_id: i64,
    primary_label: String,
    bind_cb: BindSocketFn,
    mut cmd_rx: mpsc::Receiver<EngineCommand>,
    telemetry: Arc<RwLock<TelemetrySnapshot>>,
    stop_flag: Arc<AtomicBool>,
) -> Result<(), String> {
    let tunnel_ipv4: Ipv4Addr = cfg
        .tunnel_ipv4
        .parse()
        .unwrap_or_else(|_| Ipv4Addr::new(10, 66, 66, 1));
    let upstream_dns: Ipv4Addr = cfg
        .upstream_dns_ipv4
        .parse()
        .unwrap_or_else(|_| Ipv4Addr::new(1, 1, 1, 1));
    let echo_ip = Ipv4Addr::new(10, 66, 66, 254);

    let (client_priv_b64, client_pub_b64) = if cfg.private_key_base64.trim().is_empty() {
        generate_keypair_base64()
    } else {
        let priv_bytes = decode_key_32(&cfg.private_key_base64)?;
        let secret = boringtun::x25519::StaticSecret::from(priv_bytes);
        let pub_b64 = wireguard::encode_key_32(boringtun::x25519::PublicKey::from(&secret).as_bytes());
        (cfg.private_key_base64.clone(), pub_b64)
    };

    let base_psk_bytes = match cfg.preshared_key_base64.as_deref() {
        Some(s) if !s.trim().is_empty() => Some(decode_key_32(s)?),
        _ => None,
    };

    let mut loopback_peer: Option<LoopbackPeer> = None;
    let (remote_addr, peer_pub_b64, reality_target) = if cfg.loopback_mode {
        let protect_only: loopback::ProtectSocketFn = {
            let b = bind_cb.clone();
            Arc::new(move |fd| b(fd, 0, true))
        };
        let peer = LoopbackPeer::spawn(
            client_pub_b64,
            cfg.preshared_key_base64.clone(),
            cfg.obfuscation.clone(),
            cfg.reality.short_id_hex.clone(),
            upstream_dns,
            echo_ip,
            Some(protect_only),
        )
        .await?;
        let target = RealityTarget {
            addr: peer.reality_addr,
            server_pub: peer.reality_server_pub,
            short_id: peer.reality_short_id,
            sni: if cfg.reality.sni.trim().is_empty() {
                "dl.google.com".to_string()
            } else {
                cfg.reality.sni.clone()
            },
        };
        let addr = peer.local_addr;
        let pub_b64 = peer.server_public_key_b64.clone();
        loopback_peer = Some(peer);
        (addr, pub_b64, Some(target))
    } else {
        let addr: SocketAddr = resolve_endpoint_addr(&cfg.endpoint).await?;
        let target = if cfg.reality.enabled && !cfg.reality.server_public_key_base64.trim().is_empty()
        {
            match (
                decode_key_32(&cfg.reality.server_public_key_base64),
                parse_short_id_8(&cfg.reality.short_id_hex),
            ) {
                (Ok(server_pub), Ok(short_id)) => {
                    let port = if cfg.reality.reality_port > 0 {
                        cfg.reality.reality_port
                    } else {
                        443
                    };
                    Some(RealityTarget {
                        addr: SocketAddr::new(addr.ip(), port),
                        server_pub,
                        short_id,
                        sni: cfg.reality.sni.clone(),
                    })
                }
                _ => None,
            }
        } else {
            None
        };
        (addr, cfg.peer_public_key_base64.clone(), target)
    };

    let mut session = WireGuardSession::new(
        &client_priv_b64,
        &peer_pub_b64,
        cfg.preshared_key_base64.as_deref(),
        cfg.persistent_keepalive,
        1,
    )?;

    let obfuscator = AmneziaObfuscator::new(cfg.obfuscation.clone());
    let mut sinkhole = DnsSinkhole::new(
        cfg.dns_sinkhole_enabled,
        &cfg.blocked_domains,
        &cfg.allowed_domains,
    );

    let (udp_rx_tx, mut udp_rx_rx) = mpsc::channel::<InboundDatagram>(256);
    let pool = Arc::new(SocketPool::new(
        remote_addr,
        cfg.loopback_mode,
        bind_cb,
        udp_rx_tx,
    ));

    let mut transport_mode = cfg.transport_mode;
    if transport_mode == TransportMode::ForceRealityTcp {
        if let Some(ref target) = reality_target {
            if let Err(e) = pool
                .open_reality_tcp_slot(0, primary_net_id, primary_label.clone(), target.clone())
                .await
            {
                log::warn!("ForceRealityTcp initial open failed ({e}), falling back to UDP");
                pool.open_udp_slot(0, primary_net_id, primary_label.clone())?;
            }
        } else {
            pool.open_udp_slot(0, primary_net_id, primary_label.clone())?;
        }
    } else {
        pool.open_udp_slot(0, primary_net_id, primary_label.clone())?;
    }

    let mut handover = HandoverCoordinator::new(cfg.handover.clone(), pool.clone());

    let mut tun_read_buf = vec![0u8; 4096];
    let mut enc_buf = vec![0u8; 4096];

    // Proactively start WireGuard handshake immediately
    if let TunnResult::WriteToNetwork(init_pkt) = session.initiate_handshake(&mut enc_buf, true) {
        send_obfuscated(&pool, pool.active_slot_index(), &obfuscator, init_pkt).await;
    }

    let mut tick_interval = tokio::time::interval(Duration::from_millis(150));
    let mut probe_interval = tokio::time::interval(Duration::from_millis(
        cfg.handover.probe_interval_ms.max(400),
    ));
    let mut probe_seq: u64 = 1;
    let mut pending_probe: Option<PendingProbe> = None;
    let mut last_probe_sent_at: Option<(u64, Instant)> = None;
    let mut recent_dns_events: VecDeque<DnsEventRecord> = VecDeque::with_capacity(32);
    let mut dns_total: u64 = 0;
    let mut dns_blocked: u64 = 0;
    let mut tx_bytes: u64 = 0;
    let mut rx_bytes: u64 = 0;
    let mut tx_packets: u64 = 0;
    let mut rx_packets: u64 = 0;

    // Phase 2: Happy-Eyeballs UDP block detection & 30s UDP recovery promote
    let mut udp_unanswered_since: Option<Instant> =
        if pool.active_protocol() == TransportProtocol::UdpAmneziaWg {
            Some(Instant::now())
        } else {
            None
        };
    let mut last_udp_rx = Instant::now();
    let mut initial_handshake_retried = false;
    let mut next_udp_recovery_probe = Instant::now() + Duration::from_secs(UDP_RECOVERY_PROBE_INTERVAL_SECS);

    // Phase 2: Post-Quantum X25519 + ML-KEM-768 PSK Rotation state
    let mut pq_next_epoch_counter: u64 = 0;
    let mut in_flight_pq: Option<(ClientPqExchange, Instant)> = None;
    let mut pending_pq_handshake_send: Option<(u64, Instant, Vec<u8>)> = None;
    let mut pq_last_rotate_epoch_ms: u64 = 0;
    let mut pq_initial_triggered = false;
    let pq_rekey_secs = cfg.pq_rekey_interval_secs.max(60);
    let mut last_pq_auto_rotate = Instant::now();

    while !stop_flag.load(Ordering::Relaxed) {
        tokio::select! {
            maybe_n = tun.read_packet(&mut tun_read_buf) => {
                let Some(n) = maybe_n else { break; };
                let ip_pkt = &tun_read_buf[..n];
                tx_bytes += n as u64;
                tx_packets += 1;

                // 1. Zero-Latency Local DNS Sinkhole check on port 53
                if let Some(dns_reply_ip) = try_sinkhole_intercept(
                    ip_pkt,
                    &sinkhole,
                    &mut dns_total,
                    &mut dns_blocked,
                    &mut recent_dns_events,
                ) {
                    let _ = tun.write_packet(&dns_reply_ip).await;
                    continue;
                }

                // 2. Encapsulate via WireGuard + Anti-DPI Obfuscate -> Send on active TX socket
                match session.encapsulate(ip_pkt, &mut enc_buf) {
                    TunnResult::WriteToNetwork(wg_pkt) => {
                        let active = pool.active_slot_index();
                        send_obfuscated(&pool, active, &obfuscator, wg_pkt).await;

                        // Only trigger an explicit liveness probe if no UDP RX arrived recently;
                        // never false-alarm on one-way OS/app packets that expect no reply.
                        if session.handshake_established
                            && pool.slot_protocol(active) == Some(TransportProtocol::UdpAmneziaWg)
                            && transport_mode == TransportMode::AutoHappyEyeballs
                            && last_udp_rx.elapsed() >= Duration::from_secs(2)
                            && udp_unanswered_since.is_none()
                        {
                            let seq = probe_seq;
                            probe_seq += 1;
                            let probe_ip = build_inner_probe_packet(tunnel_ipv4, echo_ip, seq, now_epoch_ms());
                            if let TunnResult::WriteToNetwork(probe_wg) = session.encapsulate(&probe_ip, &mut enc_buf) {
                                udp_unanswered_since = Some(Instant::now());
                                send_obfuscated(&pool, active, &obfuscator, probe_wg).await;
                            }
                        }
                    }
                    TunnResult::Err(e) => {
                        log::debug!("WireGuard encapsulate warning: {:?}", e);
                    }
                    _ => {}
                }
            }

            maybe_dgram = udp_rx_rx.recv() => {
                let Some(dgram) = maybe_dgram else { break; };
                let Some(wg_pkt) = obfuscator.deobfuscate_inbound(&dgram.data) else {
                    continue;
                };
                rx_bytes += dgram.data.len() as u64;

                match session.decapsulate(Some(remote_addr.ip()), &wg_pkt) {
                    DecapAction::WriteToNetwork(reply_pkt) => {
                        if dgram.protocol == TransportProtocol::UdpAmneziaWg {
                            udp_unanswered_since = None;
                            last_udp_rx = Instant::now();
                        }
                        send_obfuscated(&pool, dgram.slot_idx, &obfuscator, &reply_pkt).await;
                        drain_tunn_queue(&mut session, &pool, dgram.slot_idx, &obfuscator).await;
                    }
                    DecapAction::WriteToTunnelV4(ip_pkt) | DecapAction::WriteToTunnelV6(ip_pkt) => {
                        if dgram.protocol == TransportProtocol::UdpAmneziaWg {
                            udp_unanswered_since = None;
                            last_udp_rx = Instant::now();
                        }
                        match try_handle_pq_reply(&ip_pkt, echo_ip, &mut in_flight_pq, base_psk_bytes) {
                            Some(PqReplyAction::Resp(resp_epoch, new_psk)) => {
                                let commit = PqControlPacket::Commit(PqCommitMessage {
                                    epoch: resp_epoch,
                                })
                                .encode();
                                let commit_ip = build_ipv4_udp_packet(
                                    tunnel_ipv4,
                                    echo_ip,
                                    PQ_CONTROL_PORT,
                                    PQ_CONTROL_PORT,
                                    &commit,
                                );
                                if let TunnResult::WriteToNetwork(wg_commit) =
                                    session.encapsulate(&commit_ip, &mut enc_buf)
                                {
                                    send_obfuscated(
                                        &pool,
                                        pool.active_slot_index(),
                                        &obfuscator,
                                        wg_commit,
                                    )
                                    .await;
                                }
                                if let Some(init_bytes) =
                                    session.stage_pq_rekey(resp_epoch, new_psk, false)
                                {
                                    let wait_ms = if cfg.loopback_mode { 25 } else { 180 };
                                    pending_pq_handshake_send = Some((
                                        resp_epoch,
                                        Instant::now() + Duration::from_millis(wait_ms),
                                        init_bytes,
                                    ));
                                }
                                pq_last_rotate_epoch_ms = now_epoch_ms();
                                last_pq_auto_rotate = Instant::now();
                            }
                            Some(PqReplyAction::Ack(ack_epoch)) => {
                                if let Some((pending_epoch, _, init_bytes)) =
                                    pending_pq_handshake_send.take()
                                {
                                    if pending_epoch == ack_epoch {
                                        send_obfuscated(
                                            &pool,
                                            pool.active_slot_index(),
                                            &obfuscator,
                                            &init_bytes,
                                        )
                                        .await;
                                    } else {
                                        pending_pq_handshake_send =
                                            Some((pending_epoch, Instant::now(), init_bytes));
                                    }
                                }
                            }
                            None => if let Some((seq, rtt)) = parse_inner_probe_reply(&ip_pkt) {
                            let active_slot = pool.active_slot_index();
                            if dgram.slot_idx == active_slot {
                                let actual_rtt = if let Some((l_seq, l_inst)) = last_probe_sent_at {
                                    if l_seq == seq {
                                        l_inst.elapsed().as_secs_f64() * 1000.0
                                    } else {
                                        rtt
                                    }
                                } else {
                                    rtt
                                };
                                handover.scorer.record_rtt_sample(true, actual_rtt);
                            } else {
                                let stand_rtt = pending_probe
                                    .as_ref()
                                    .filter(|p| p.seq == seq)
                                    .map(|p| p.started.elapsed().as_secs_f64() * 1000.0)
                                    .unwrap_or(rtt);
                                handover.scorer.record_rtt_sample(false, stand_rtt);
                            }

                            if let Some(p) = pending_probe.take() {
                                if p.cutover_on_reply && dgram.slot_idx == p.slot_idx {
                                    let elapsed_ms = p.started.elapsed().as_secs_f64() * 1000.0;
                                    handover.execute_atomic_cutover(&p.reason, elapsed_ms);
                                    if p.clear_udp_promote_on_cutover {
                                        udp_unanswered_since = None;
                                        last_udp_rx = Instant::now();
                                    }
                                } else if !p.cutover_on_reply {
                                    pending_probe = Some(p);
                                }
                            }
                        } else {
                            rx_packets += 1;
                            let _ = tun.write_packet(&ip_pkt).await;
                        }
                        }
                    }
                    DecapAction::Done => {
                        if dgram.protocol == TransportProtocol::UdpAmneziaWg {
                            udp_unanswered_since = None;
                            last_udp_rx = Instant::now();
                        }
                    }
                    DecapAction::Err(e) => {
                        log::debug!("WireGuard decapsulate error: {e}");
                    }
                }
            }

            maybe_cmd = cmd_rx.recv() => {
                let Some(cmd) = maybe_cmd else { break; };
                match cmd {
                    EngineCommand::StandbyNetworkAvailable { net_id, label, cutover_immediately } => {
                        let use_reality = pool.active_protocol() == TransportProtocol::RealityTcp
                            || pool.is_udp_blocked_simulated()
                            || transport_mode == TransportMode::ForceRealityTcp;
                        let open_res = match (use_reality, reality_target.as_ref()) {
                            (true, Some(target)) => {
                                handover
                                    .prepare_standby_reality(net_id, label, target.clone())
                                    .await
                            }
                            _ => handover.prepare_standby(net_id, label),
                        };
                        if let Ok(slot_idx) = open_res {
                            let seq = probe_seq;
                            probe_seq += 1;
                            let probe_ip = build_inner_probe_packet(tunnel_ipv4, echo_ip, seq, now_epoch_ms());
                            if let TunnResult::WriteToNetwork(wg_pkt) = session.encapsulate(&probe_ip, &mut enc_buf) {
                                send_obfuscated(&pool, slot_idx, &obfuscator, wg_pkt).await;
                            }
                            pending_probe = Some(PendingProbe {
                                seq,
                                started: Instant::now(),
                                slot_idx,
                                cutover_on_reply: cutover_immediately,
                                reason: if cutover_immediately {
                                    "Network switch cutover".to_string()
                                } else {
                                    "Pre-warmed standby path probe".to_string()
                                },
                                clear_udp_promote_on_cutover: false,
                            });
                        }
                    }
                    EngineCommand::PrimaryNetworkLost { lost_net_id } => {
                        let active_matches = pool.active_socket_info().map(|(id, _)| id == lost_net_id).unwrap_or(false);
                        if active_matches && handover.standby_slot_ready().is_some() {
                            let elapsed = pending_probe.as_ref().map(|p| p.started.elapsed().as_secs_f64() * 1000.0).unwrap_or(1.5);
                            pending_probe = None;
                            handover.execute_atomic_cutover("Primary network lost - instant failover", elapsed);
                        }
                    }
                    EngineCommand::UpdateWifiRssi(rssi) => {
                        handover.scorer.update_wifi_rssi(rssi);
                    }
                    EngineCommand::TriggerHandoverTest { next_label } => {
                        let next_net_id = if cfg.loopback_mode { 9000 + probe_seq as i64 } else { primary_net_id };
                        let use_reality = pool.active_protocol() == TransportProtocol::RealityTcp
                            || pool.is_udp_blocked_simulated()
                            || transport_mode == TransportMode::ForceRealityTcp;
                        let open_res = match (use_reality, reality_target.as_ref()) {
                            (true, Some(target)) => {
                                handover
                                    .prepare_standby_reality(next_net_id, next_label, target.clone())
                                    .await
                            }
                            _ => handover.prepare_standby(next_net_id, next_label),
                        };
                        if let Ok(slot_idx) = open_res {
                            let seq = probe_seq;
                            probe_seq += 1;
                            let probe_ip = build_inner_probe_packet(tunnel_ipv4, echo_ip, seq, now_epoch_ms());
                            if let TunnResult::WriteToNetwork(wg_pkt) = session.encapsulate(&probe_ip, &mut enc_buf) {
                                send_obfuscated(&pool, slot_idx, &obfuscator, wg_pkt).await;
                            }
                            pending_probe = Some(PendingProbe {
                                seq,
                                started: Instant::now(),
                                slot_idx,
                                cutover_on_reply: true,
                                reason: "Make-Before-Break Lab Handover".to_string(),
                                clear_udp_promote_on_cutover: false,
                            });
                        }
                    }
                    EngineCommand::UpdateBlocklist { blocked, allowed } => {
                        sinkhole = DnsSinkhole::new(cfg.dns_sinkhole_enabled, &blocked, &allowed);
                    }
                    EngineCommand::SetSimulateUdpBlocked(blocked) => {
                        pool.set_simulate_udp_blocked(blocked);
                        if blocked {
                            if pool.active_protocol() == TransportProtocol::UdpAmneziaWg
                                && transport_mode == TransportMode::AutoHappyEyeballs
                            {
                                // Trigger an active probe so Happy-Eyeballs measures UDP timeout promptly even when idle
                                let seq = probe_seq;
                                probe_seq += 1;
                                let probe_ip = build_inner_probe_packet(tunnel_ipv4, echo_ip, seq, now_epoch_ms());
                                if let TunnResult::WriteToNetwork(wg_pkt) = session.encapsulate(&probe_ip, &mut enc_buf) {
                                    send_obfuscated(&pool, pool.active_slot_index(), &obfuscator, wg_pkt).await;
                                }
                                if udp_unanswered_since.is_none() {
                                    udp_unanswered_since = Some(Instant::now());
                                }
                            }
                        } else if pool.active_protocol() == TransportProtocol::RealityTcp
                            && transport_mode == TransportMode::AutoHappyEyeballs
                        {
                            // Unblocked in lab/UI: schedule prompt UDP recovery probe rather than waiting full 30s
                            next_udp_recovery_probe = Instant::now() + Duration::from_millis(350);
                        }
                    }
                    EngineCommand::RotatePqKey => {
                        if cfg.pq_enabled && session.handshake_established && in_flight_pq.is_none() {
                            pq_next_epoch_counter = pq_next_epoch_counter.max(session.pq_epoch) + 1;
                            start_pq_exchange(
                                pq_next_epoch_counter,
                                tunnel_ipv4,
                                echo_ip,
                                &mut session,
                                &pool,
                                &obfuscator,
                                &mut enc_buf,
                                &mut in_flight_pq,
                            ).await;
                        }
                    }
                    EngineCommand::SetTransportMode(new_mode) => {
                        transport_mode = new_mode;
                        let current_proto = pool.active_protocol();
                        let (net_id, label) = pool
                            .active_socket_info()
                            .unwrap_or((primary_net_id, primary_label.clone()));

                        match new_mode {
                            TransportMode::ForceRealityTcp if current_proto != TransportProtocol::RealityTcp => {
                                if let Some(ref target) = reality_target {
                                    let started = Instant::now();
                                    if handover.prepare_standby_reality(net_id, label, target.clone()).await.is_ok() {
                                        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                                        handover.execute_atomic_cutover("Manual Switch -> TCP/443 REALITY", elapsed);
                                        pending_probe = None;
                                        udp_unanswered_since = None;
                                    }
                                }
                            }
                            TransportMode::ForceUdp if current_proto != TransportProtocol::UdpAmneziaWg => {
                                let started = Instant::now();
                                if handover.prepare_standby(net_id, label).is_ok() {
                                    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                                    handover.execute_atomic_cutover("Manual Switch -> UDP AmneziaWG", elapsed);
                                    pending_probe = None;
                                    udp_unanswered_since = None;
                                    last_udp_rx = Instant::now();
                                }
                            }
                            TransportMode::AutoHappyEyeballs
                                if current_proto == TransportProtocol::RealityTcp =>
                            {
                                next_udp_recovery_probe = Instant::now() + Duration::from_millis(400);
                            }
                            _ => {}
                        }
                    }
                    EngineCommand::Stop => {
                        stop_flag.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }

            _ = probe_interval.tick() => {
                if session.handshake_established && pending_probe.is_none() {
                    let seq = probe_seq;
                    probe_seq += 1;
                    handover.scorer.record_probe_sent(true);
                    last_probe_sent_at = Some((seq, Instant::now()));
                    let active = pool.active_slot_index();
                    if pool.slot_protocol(active) == Some(TransportProtocol::UdpAmneziaWg)
                        && udp_unanswered_since.is_none()
                    {
                        udp_unanswered_since = Some(Instant::now());
                    }
                    let probe_ip = build_inner_probe_packet(tunnel_ipv4, echo_ip, seq, now_epoch_ms());
                    if let TunnResult::WriteToNetwork(wg_pkt) = session.encapsulate(&probe_ip, &mut enc_buf) {
                        send_obfuscated(&pool, active, &obfuscator, wg_pkt).await;
                    }
                }
            }

            _ = tick_interval.tick() => {
                // WireGuard periodic timers (rekey / keepalive)
                if let TunnResult::WriteToNetwork(timer_pkt) = session.update_timers(&mut enc_buf) {
                    send_obfuscated(&pool, pool.active_slot_index(), &obfuscator, timer_pkt).await;
                }

                // Delayed/fallback pending PQ handshake initiation if Ack UDP packet was not yet received
                if let Some((_, when, ref init_bytes)) = pending_pq_handshake_send {
                    if Instant::now() >= when {
                        let bytes = init_bytes.clone();
                        pending_pq_handshake_send = None;
                        send_obfuscated(&pool, pool.active_slot_index(), &obfuscator, &bytes).await;
                    }
                }

                // Clear stale in-flight PQ exchange after 5s
                if let Some((_, started)) = in_flight_pq {
                    if started.elapsed() > Duration::from_secs(5) {
                        in_flight_pq = None;
                    }
                }

                // Automatic initial ML-KEM-768 upgrade + periodic epoch rotation
                if cfg.pq_enabled && session.handshake_established && in_flight_pq.is_none() {
                    let should_initial = !pq_initial_triggered && session.pq_epoch == 0;
                    let should_periodic = pq_initial_triggered
                        && last_pq_auto_rotate.elapsed() >= Duration::from_secs(pq_rekey_secs);
                    if should_initial || should_periodic {
                        pq_initial_triggered = true;
                        last_pq_auto_rotate = Instant::now();
                        pq_next_epoch_counter = pq_next_epoch_counter.max(session.pq_epoch) + 1;
                        start_pq_exchange(
                            pq_next_epoch_counter,
                            tunnel_ipv4,
                            echo_ip,
                            &mut session,
                            &pool,
                            &obfuscator,
                            &mut enc_buf,
                            &mut in_flight_pq,
                        ).await;
                    }
                }

                // Happy-Eyeballs: UDP blocked for >= threshold -> Zero-Drop fallback to TCP/443 REALITY
                if transport_mode == TransportMode::AutoHappyEyeballs
                    && pool.active_protocol() == TransportProtocol::UdpAmneziaWg
                {
                    if let Some(since) = udp_unanswered_since {
                        if !session.handshake_established
                            && !pool.is_udp_blocked_simulated()
                            && !initial_handshake_retried
                            && since.elapsed() >= Duration::from_millis(450)
                        {
                            initial_handshake_retried = true;
                            if let TunnResult::WriteToNetwork(init_pkt) =
                                session.initiate_handshake(&mut enc_buf, true)
                            {
                                send_obfuscated(&pool, pool.active_slot_index(), &obfuscator, init_pkt).await;
                            }
                        }
                        let threshold_ms = if session.handshake_established {
                            UDP_BLOCK_FALLBACK_MS
                        } else {
                            INITIAL_HANDSHAKE_FALLBACK_MS
                        };
                        if since.elapsed() >= Duration::from_millis(threshold_ms) {
                            if let Some(ref target) = reality_target {
                                let (net_id, label) = pool
                                    .active_socket_info()
                                    .unwrap_or((primary_net_id, primary_label.clone()));
                                let started = Instant::now();
                                if handover.prepare_standby_reality(net_id, label, target.clone()).await.is_ok() {
                                    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                                    pending_probe = None;
                                    handover.execute_atomic_cutover(
                                        "Happy-Eyeballs: UDP Blocked -> TCP/443 REALITY",
                                        elapsed,
                                    );
                                    udp_unanswered_since = None;
                                    next_udp_recovery_probe = Instant::now() + Duration::from_secs(UDP_RECOVERY_PROBE_INTERVAL_SECS);

                                    // If initial handshake hadn't completed before UDP blockage, finish handshake over REALITY immediately
                                    if !session.handshake_established {
                                        if let TunnResult::WriteToNetwork(init_pkt) =
                                            session.initiate_handshake(&mut enc_buf, true)
                                        {
                                            send_obfuscated(&pool, pool.active_slot_index(), &obfuscator, init_pkt).await;
                                        }
                                    }
                                } else {
                                    udp_unanswered_since = Some(Instant::now());
                                }
                            }
                        }
                    }
                }

                // Auto-Promote: when on TCP/443 REALITY in AutoHappyEyeballs, probe UDP every 30s
                if transport_mode == TransportMode::AutoHappyEyeballs
                    && pool.active_protocol() == TransportProtocol::RealityTcp
                    && pending_probe.is_none()
                    && Instant::now() >= next_udp_recovery_probe
                {
                    next_udp_recovery_probe = Instant::now() + Duration::from_secs(UDP_RECOVERY_PROBE_INTERVAL_SECS);
                    let (net_id, label) = pool
                        .active_socket_info()
                        .unwrap_or((primary_net_id, primary_label.clone()));
                    if let Ok(slot_idx) = handover.prepare_standby(net_id, label) {
                        let seq = probe_seq;
                        probe_seq += 1;
                        let probe_ip = build_inner_probe_packet(tunnel_ipv4, echo_ip, seq, now_epoch_ms());
                        if let TunnResult::WriteToNetwork(wg_pkt) = session.encapsulate(&probe_ip, &mut enc_buf) {
                            send_obfuscated(&pool, slot_idx, &obfuscator, wg_pkt).await;
                        }
                        pending_probe = Some(PendingProbe {
                            seq,
                            started: Instant::now(),
                            slot_idx,
                            cutover_on_reply: true,
                            reason: "Auto-Promote: UDP Unblocked -> UDP AmneziaWG".to_string(),
                            clear_udp_promote_on_cutover: true,
                        });
                    }
                }

                // Timeout pending Make-Before-Break cutover / UDP recovery probe
                if let Some(ref p) = pending_probe {
                    let timeout_ms = if p.clear_udp_promote_on_cutover { 750 } else { 180 };
                    if p.cutover_on_reply && p.started.elapsed() > Duration::from_millis(timeout_ms) {
                        if p.clear_udp_promote_on_cutover {
                            // UDP still blocked; silently discard standby UDP slot and stay cleanly on REALITY
                            pending_probe = None;
                            handover.discard_standby();
                        } else {
                            let elapsed = p.started.elapsed().as_secs_f64() * 1000.0;
                            let reason = p.reason.clone();
                            pending_probe = None;
                            handover.execute_atomic_cutover(&reason, elapsed);
                        }
                    }
                }

                // Evaluate adaptive path scoring
                if let HandoverAdvice::CutoverNow(reason) = handover.scorer.evaluate(handover.standby_slot_ready().is_some()) {
                    let elapsed = pending_probe.as_ref().map(|p| p.started.elapsed().as_secs_f64() * 1000.0).unwrap_or(2.0);
                    pending_probe = None;
                    handover.execute_atomic_cutover(reason, elapsed);
                }

                // Publish telemetry snapshot to Kotlin UI
                if let Ok(mut snap) = telemetry.write() {
                    let active = pool.active_socket_info();
                    let standby = pool.standby_socket_info();
                    snap.connected = true;
                    snap.handshake_established = session.handshake_established;
                    if let Some((net_id, label)) = active {
                        snap.active_net_id = net_id;
                        snap.active_path_label = label;
                    }
                    if let Some((s_id, s_label)) = standby {
                        snap.standby_net_id = Some(s_id);
                        snap.standby_path_label = Some(s_label);
                    } else {
                        snap.standby_net_id = None;
                        snap.standby_path_label = None;
                    }
                    snap.rx_bytes = rx_bytes;
                    snap.tx_bytes = tx_bytes;
                    snap.rx_packets = rx_packets;
                    snap.tx_packets = tx_packets;
                    snap.rtt_ms = (handover.scorer.primary.ewma_rtt_ms * 10.0).round() / 10.0;
                    snap.jitter_ms = (handover.scorer.primary.jitter_ms * 10.0).round() / 10.0;
                    snap.packet_loss_pct = (handover.scorer.primary.recent_loss_pct * 10.0).round() / 10.0;
                    snap.dns_queries_total = dns_total;
                    snap.dns_queries_blocked = dns_blocked;
                    snap.handover_count = handover.handover_count;
                    snap.last_handover_ms = (handover.last_handover_ms * 100.0).round() / 100.0;
                    snap.total_handover_dropped_packets = handover.total_dropped_packets;
                    snap.recent_handovers = handover.history.iter().cloned().collect();
                    snap.recent_dns_events = recent_dns_events.iter().cloned().collect();
                    snap.transport_protocol = pool.active_protocol().display_name().to_string();
                    snap.pq_cipher_suite = if cfg.pq_enabled {
                        "X25519 + ML-KEM-768".to_string()
                    } else {
                        "X25519 (Classical)".to_string()
                    };
                    snap.pq_epoch = session.pq_epoch;
                    snap.pq_last_rotate_epoch_ms = pq_last_rotate_epoch_ms;
                    snap.udp_blocked_simulated = pool.is_udp_blocked_simulated();
                }
            }
        }
    }

    pool.close_all();
    if let Some(peer) = loopback_peer {
        peer.stop();
    }
    Ok(())
}

async fn resolve_endpoint_addr(endpoint: &str) -> Result<SocketAddr, String> {
    let clean = endpoint.trim();
    if let Ok(addr) = clean.parse::<SocketAddr>() {
        return Ok(addr);
    }
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host(clean)
        .await
        .map_err(|e| format!("Failed resolving server endpoint '{clean}': {e}"))?
        .collect();
    addrs
        .iter()
        .copied()
        .find(|a| a.is_ipv4())
        .or_else(|| addrs.into_iter().next())
        .ok_or_else(|| format!("Invalid or empty server endpoint '{clean}'"))
}

#[allow(clippy::too_many_arguments)]
async fn start_pq_exchange(
    epoch: u64,
    tunnel_ipv4: Ipv4Addr,
    echo_ip: Ipv4Addr,
    session: &mut WireGuardSession,
    pool: &SocketPool,
    obfuscator: &AmneziaObfuscator,
    enc_buf: &mut [u8],
    in_flight_pq: &mut Option<(ClientPqExchange, Instant)>,
) {
    let (exch, init_msg) = ClientPqExchange::begin(epoch);
    let encoded = PqControlPacket::Init(init_msg).encode();
    let pq_ip = build_ipv4_udp_packet(
        tunnel_ipv4,
        echo_ip,
        PQ_CONTROL_PORT,
        PQ_CONTROL_PORT,
        &encoded,
    );
    if let TunnResult::WriteToNetwork(wg_pkt) = session.encapsulate(&pq_ip, enc_buf) {
        send_obfuscated(pool, pool.active_slot_index(), obfuscator, wg_pkt).await;
        *in_flight_pq = Some((exch, Instant::now()));
    }
}

enum PqReplyAction {
    Resp(u64, [u8; 32]),
    Ack(u64),
}

fn try_handle_pq_reply(
    ip_pkt: &[u8],
    echo_ip: Ipv4Addr,
    in_flight: &mut Option<(ClientPqExchange, Instant)>,
    base_psk: Option<[u8; 32]>,
) -> Option<PqReplyAction> {
    let parsed = parse_ip_packet(ip_pkt)?;
    if parsed.proto != TransportProto::Udp {
        return None;
    }
    let udp = parse_udp(parsed.payload)?;
    if udp.src_port != PQ_CONTROL_PORT && udp.dst_port != PQ_CONTROL_PORT {
        return None;
    }
    if parsed.src_v4 != Some(echo_ip) {
        return None;
    }
    match PqControlPacket::decode(udp.payload)? {
        PqControlPacket::Resp(resp) => {
            let (exch, _) = in_flight.take()?;
            if exch.epoch != resp.epoch {
                return None;
            }
            let psk = exch.finish(&resp, base_psk).ok()?;
            Some(PqReplyAction::Resp(resp.epoch, psk))
        }
        PqControlPacket::Ack(ack) => Some(PqReplyAction::Ack(ack.epoch)),
        _ => None,
    }
}

async fn send_obfuscated(
    pool: &SocketPool,
    slot_idx: usize,
    obfuscator: &AmneziaObfuscator,
    wg_pkt: &[u8],
) {
    let (is_init, obf_pkt) = obfuscator.obfuscate_outbound(wg_pkt);
    if is_init {
        for junk in obfuscator.generate_junk_packets() {
            let _ = pool.send_on_slot(slot_idx, &junk).await;
        }
    }
    let _ = pool.send_on_slot(slot_idx, &obf_pkt).await;
}

async fn drain_tunn_queue(
    session: &mut WireGuardSession,
    pool: &SocketPool,
    slot_idx: usize,
    obfuscator: &AmneziaObfuscator,
) {
    while let DecapAction::WriteToNetwork(pkt) = session.decapsulate(None, &[]) {
        send_obfuscated(pool, slot_idx, obfuscator, &pkt).await;
    }
}

fn try_sinkhole_intercept(
    ip_pkt: &[u8],
    sinkhole: &DnsSinkhole,
    dns_total: &mut u64,
    dns_blocked: &mut u64,
    recent_events: &mut VecDeque<DnsEventRecord>,
) -> Option<Vec<u8>> {
    let parsed = parse_ip_packet(ip_pkt)?;
    if parsed.proto != TransportProto::Udp {
        return None;
    }
    let udp = parse_udp(parsed.payload)?;
    if udp.dst_port != 53 {
        return None;
    }
    let q = parse_dns_query(udp.payload)?;
    *dns_total += 1;

    let blocked = sinkhole.is_blocked(&q.qname);
    if blocked {
        *dns_blocked += 1;
    }
    if recent_events.len() >= 30 {
        recent_events.pop_front();
    }
    recent_events.push_back(DnsEventRecord {
        timestamp_epoch_ms: now_epoch_ms(),
        domain: q.qname.clone(),
        blocked,
        action: if blocked {
            "Sinkhole 0.0.0.0".to_string()
        } else {
            "Encrypted Tunnel".to_string()
        },
    });

    if !blocked {
        return None;
    }

    let dns_resp = build_sinkhole_response(udp.payload, &q);
    match parsed.version {
        IpVersion::V4 => {
            let src = parsed.dst_v4?;
            let dst = parsed.src_v4?;
            Some(build_ipv4_udp_packet(
                src,
                dst,
                udp.dst_port,
                udp.src_port,
                &dns_resp,
            ))
        }
        IpVersion::V6 => {
            let src = parsed.dst_v6?;
            let dst = parsed.src_v6?;
            Some(crate::packet::ip_parser::build_ipv6_udp_packet(
                src,
                dst,
                udp.dst_port,
                udp.src_port,
                &dns_resp,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HandoverConfig, ObfuscationConfig, RealityConfig};

    fn make_dns_ipv4_packet(domain: &str) -> Vec<u8> {
        let mut dns = vec![
            0xaa, 0xbb, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        for label in domain.split('.') {
            dns.push(label.len() as u8);
            dns.extend_from_slice(label.as_bytes());
        }
        dns.push(0);
        dns.extend_from_slice(&1u16.to_be_bytes());
        dns.extend_from_slice(&1u16.to_be_bytes());
        build_ipv4_udp_packet(
            Ipv4Addr::new(10, 66, 66, 1),
            Ipv4Addr::new(10, 66, 66, 2),
            51234,
            53,
            &dns,
        )
    }

    fn base_loopback_config(priv_b64: String) -> EngineConfig {
        EngineConfig {
            private_key_base64: priv_b64,
            peer_public_key_base64: String::new(),
            preshared_key_base64: None,
            endpoint: "127.0.0.1:0".to_string(),
            tunnel_ipv4: "10.66.66.1".to_string(),
            virtual_dns_ipv4: "10.66.66.2".to_string(),
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
            blocked_domains: vec!["doubleclick.net".to_string()],
            allowed_domains: vec![],
        }
    }

    #[test]
    fn end_to_end_loopback_dns_sinkhole_and_zero_drop_handover() {
        let (priv_b64, _) = generate_keypair_base64();
        let cfg = base_loopback_config(priv_b64);

        let (tun_in_tx, tun_in_rx) = mpsc::channel::<Vec<u8>>(64);
        let (tun_out_tx, mut tun_out_rx) = mpsc::channel::<Vec<u8>>(64);
        let tun = TunTransport::Channel {
            inbound_rx: tun_in_rx,
            outbound_tx: tun_out_tx,
        };

        let bind_cb: BindSocketFn = Arc::new(|_, _, _| true);
        let handle = start_engine(
            cfg,
            TunInput::Transport(tun),
            100,
            "Wi-Fi".to_string(),
            bind_cb,
        )
        .expect("engine starts");

        let test_rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        test_rt.block_on(async {
            // 1. Blocked domain intercepted locally by DNS Sinkhole in <1ms
            let blocked_dns = make_dns_ipv4_packet("ads.doubleclick.net");
            tun_in_tx.send(blocked_dns).await.unwrap();
            let sink_reply = tokio::time::timeout(Duration::from_secs(2), tun_out_rx.recv())
                .await
                .expect("sinkhole timeout")
                .expect("sinkhole reply");
            let parsed_sink = parse_ip_packet(&sink_reply).unwrap();
            let udp_sink = parse_udp(parsed_sink.payload).unwrap();
            assert_eq!(udp_sink.src_port, 53);
            assert_eq!(&udp_sink.payload[udp_sink.payload.len() - 4..], &[0, 0, 0, 0]);

            // 2. Allowed lab domain passes through encrypted WireGuard + AmneziaWG -> LoopbackPeer -> TUN
            let allowed_dns = make_dns_ipv4_packet("probe.aegis.test");
            tun_in_tx.send(allowed_dns).await.unwrap();
            let wg_reply = tokio::time::timeout(Duration::from_secs(3), tun_out_rx.recv())
                .await
                .expect("wg dns timeout")
                .expect("wg dns reply");
            let parsed_wg = parse_ip_packet(&wg_reply).unwrap();
            let udp_wg = parse_udp(parsed_wg.payload).unwrap();
            assert_eq!(&udp_wg.payload[udp_wg.payload.len() - 4..], &[10, 66, 66, 254]);

            // 3. Trigger Zero-Drop Handover while streaming packets across cutover
            handle
                .cmd_tx
                .send(EngineCommand::TriggerHandoverTest {
                    next_label: "5G".to_string(),
                })
                .await
                .unwrap();

            for seq in 0u32..10 {
                let echo_pkt = build_ipv4_udp_packet(
                    Ipv4Addr::new(10, 66, 66, 1),
                    Ipv4Addr::new(10, 66, 66, 254),
                    40000,
                    5555,
                    &seq.to_be_bytes(),
                );
                tun_in_tx.send(echo_pkt).await.unwrap();
                tokio::time::sleep(Duration::from_millis(15)).await;
            }

            let mut received = 0usize;
            let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
            while received < 10 && tokio::time::Instant::now() < deadline {
                if let Ok(Some(pkt)) =
                    tokio::time::timeout(Duration::from_millis(500), tun_out_rx.recv()).await
                {
                    if let Some(p) = parse_ip_packet(&pkt) {
                        if let Some(u) = parse_udp(p.payload) {
                            if u.src_port == 5555 && u.payload.len() == 4 {
                                received += 1;
                            }
                        }
                    }
                }
            }
            assert_eq!(received, 10, "Zero-drop handover lost packets");

            tokio::time::sleep(Duration::from_millis(250)).await;
            let snap = handle.snapshot();
            assert!(snap.handshake_established);
            assert_eq!(snap.active_path_label, "5G");
            assert!(snap.handover_count >= 1);
            assert_eq!(snap.total_handover_dropped_packets, 0);
            assert_eq!(snap.dns_queries_blocked, 1);
            assert_eq!(snap.pq_cipher_suite, "X25519 + ML-KEM-768");
            assert!(snap.pq_epoch >= 1);
        });

        handle.stop();
    }

    #[test]
    fn test_happy_eyeballs_reality_fallback_promote_and_pq_rotation() {
        let (priv_b64, _) = generate_keypair_base64();
        let cfg = base_loopback_config(priv_b64);

        let (tun_in_tx, tun_in_rx) = mpsc::channel::<Vec<u8>>(64);
        let (tun_out_tx, mut tun_out_rx) = mpsc::channel::<Vec<u8>>(64);
        let tun = TunTransport::Channel {
            inbound_rx: tun_in_rx,
            outbound_tx: tun_out_tx,
        };

        let bind_cb: BindSocketFn = Arc::new(|_, _, _| true);
        let handle = start_engine(
            cfg,
            TunInput::Transport(tun),
            100,
            "Wi-Fi".to_string(),
            bind_cb,
        )
        .expect("engine starts");

        let test_rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        test_rt.block_on(async {
            // 1. Wait for WG handshake & automatic ML-KEM-768 PQ epoch 1 upgrade
            let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
            loop {
                let s = handle.snapshot();
                if s.handshake_established && s.pq_epoch >= 1 {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "Timed out waiting for PQ epoch 1: {:?}",
                    s
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            assert_eq!(handle.snapshot().transport_protocol, "UDP AmneziaWG");

            // 2. Trigger explicit PQ key rotation (Epoch 2) while passing packets
            handle.cmd_tx.send(EngineCommand::RotatePqKey).await.unwrap();
            let pq_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
            while handle.snapshot().pq_epoch < 2 {
                assert!(
                    tokio::time::Instant::now() < pq_deadline,
                    "Timed out waiting for PQ epoch 2"
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }

            // 3. Simulate ISP blocking UDP -> Happy-Eyeballs auto-fallback to TCP/443 REALITY
            handle
                .cmd_tx
                .send(EngineCommand::SetSimulateUdpBlocked(true))
                .await
                .unwrap();

            let fb_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
            while handle.snapshot().transport_protocol != "TCP/443 REALITY" {
                assert!(
                    tokio::time::Instant::now() < fb_deadline,
                    "Timed out waiting for REALITY fallback: {:?}",
                    handle.snapshot().transport_protocol
                );
                tokio::time::sleep(Duration::from_millis(60)).await;
            }
            assert!(handle.snapshot().udp_blocked_simulated);

            // Stream 10 UDP echo packets cleanly over TCP/443 REALITY
            for seq in 0u32..10 {
                let echo_pkt = build_ipv4_udp_packet(
                    Ipv4Addr::new(10, 66, 66, 1),
                    Ipv4Addr::new(10, 66, 66, 254),
                    40001,
                    5555,
                    &seq.to_be_bytes(),
                );
                tun_in_tx.send(echo_pkt).await.unwrap();
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let mut got = 0usize;
            let echo_dl = tokio::time::Instant::now() + Duration::from_secs(3);
            while got < 10 && tokio::time::Instant::now() < echo_dl {
                if let Ok(Some(pkt)) =
                    tokio::time::timeout(Duration::from_millis(400), tun_out_rx.recv()).await
                {
                    if let Some(p) = parse_ip_packet(&pkt) {
                        if let Some(u) = parse_udp(p.payload) {
                            if u.src_port == 5555 && u.payload.len() == 4 {
                                got += 1;
                            }
                        }
                    }
                }
            }
            assert_eq!(got, 10, "TCP/443 REALITY lost packets");

            // 4. Unblock UDP -> Auto-Promote seamlessly back to UDP AmneziaWG
            handle
                .cmd_tx
                .send(EngineCommand::SetSimulateUdpBlocked(false))
                .await
                .unwrap();
            let promo_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
            while handle.snapshot().transport_protocol != "UDP AmneziaWG" {
                assert!(
                    tokio::time::Instant::now() < promo_deadline,
                    "Timed out waiting for UDP auto-promote"
                );
                tokio::time::sleep(Duration::from_millis(60)).await;
            }

            // Stream 10 more packets over restored UDP AmneziaWG
            for seq in 10u32..20 {
                let echo_pkt = build_ipv4_udp_packet(
                    Ipv4Addr::new(10, 66, 66, 1),
                    Ipv4Addr::new(10, 66, 66, 254),
                    40001,
                    5555,
                    &seq.to_be_bytes(),
                );
                tun_in_tx.send(echo_pkt).await.unwrap();
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let mut got2 = 0usize;
            let echo_dl2 = tokio::time::Instant::now() + Duration::from_secs(3);
            while got2 < 10 && tokio::time::Instant::now() < echo_dl2 {
                if let Ok(Some(pkt)) =
                    tokio::time::timeout(Duration::from_millis(400), tun_out_rx.recv()).await
                {
                    if let Some(p) = parse_ip_packet(&pkt) {
                        if let Some(u) = parse_udp(p.payload) {
                            if u.src_port == 5555 && u.payload.len() == 4 {
                                got2 += 1;
                            }
                        }
                    }
                }
            }
            assert_eq!(got2, 10, "Restored UDP AmneziaWG lost packets");
            assert_eq!(handle.snapshot().total_handover_dropped_packets, 0);
        });

        handle.stop();
    }

    #[test]
    fn ipv6_dns_sinkhole_blocks_aaaa_queries_over_tun() {
        let mut dns = vec![
            0x55, 0x66, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        for label in "ads.doubleclick.net".split('.') {
            dns.push(label.len() as u8);
            dns.extend_from_slice(label.as_bytes());
        }
        dns.push(0);
        dns.extend_from_slice(&28u16.to_be_bytes()); // QTYPE AAAA
        dns.extend_from_slice(&1u16.to_be_bytes());

        let src_v6: std::net::Ipv6Addr = "fd66:66::1".parse().unwrap();
        let dst_v6: std::net::Ipv6Addr = "fd66:66::2".parse().unwrap();
        let ipv6_pkt = crate::packet::ip_parser::build_ipv6_udp_packet(
            src_v6, dst_v6, 51234, 53, &dns,
        );
        let sinkhole = DnsSinkhole::new(true, &["doubleclick.net".to_string()], &[]);
        let mut total = 0u64;
        let mut blocked = 0u64;
        let mut events = VecDeque::new();

        let reply = try_sinkhole_intercept(&ipv6_pkt, &sinkhole, &mut total, &mut blocked, &mut events)
            .expect("expected IPv6 sinkhole response");
        assert_eq!(total, 1);
        assert_eq!(blocked, 1);

        let parsed = parse_ip_packet(&reply).unwrap();
        assert_eq!(parsed.version, IpVersion::V6);
        assert_eq!(parsed.src_v6, Some(dst_v6));
        assert_eq!(parsed.dst_v6, Some(src_v6));
        let udp = parse_udp(parsed.payload).unwrap();
        assert_eq!(udp.src_port, 53);
        assert_eq!(&udp.payload[udp.payload.len() - 16..], &[0u8; 16]);
    }

    #[test]
    fn resolves_ip_literals_and_domain_endpoints() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let v4 = resolve_endpoint_addr("203.0.113.10:51820").await.unwrap();
            assert_eq!(v4.to_string(), "203.0.113.10:51820");

            let v6 = resolve_endpoint_addr("[2001:db8::1]:51820").await.unwrap();
            assert_eq!(v6.to_string(), "[2001:db8::1]:51820");

            let host = resolve_endpoint_addr("localhost:51820").await.unwrap();
            assert_eq!(host.port(), 51820);
        });
    }
}
