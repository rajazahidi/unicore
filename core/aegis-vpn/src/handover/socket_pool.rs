use aegis_proto::reality::{
    build_reality_client_hello, client_finish_reality_handshake, read_reality_frame,
    read_tls_handshake_record, verify_client_reality_pong, write_reality_frame, REALITY_FRAME_WG,
};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Notify};

pub type BindSocketFn = Arc<dyn Fn(i32, i64, bool) -> bool + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportProtocol {
    UdpAmneziaWg,
    RealityTcp,
}

impl TransportProtocol {
    pub fn display_name(self) -> &'static str {
        match self {
            TransportProtocol::UdpAmneziaWg => "UDP AmneziaWG",
            TransportProtocol::RealityTcp => "TCP/443 REALITY",
        }
    }
}

#[derive(Clone)]
pub struct RealityTarget {
    pub addr: SocketAddr,
    pub server_pub: [u8; 32],
    pub short_id: [u8; 8],
    pub sni: String,
}

pub enum SlotTransport {
    Udp(Arc<UdpSocket>),
    RealityTcp { tx: mpsc::Sender<(u8, Vec<u8>)> },
}

pub struct BoundSocket {
    pub transport: SlotTransport,
    pub protocol: TransportProtocol,
    pub net_id: i64,
    pub label: String,
    pub generation: u64,
    pub cancel: Arc<AtomicBool>,
    pub cancel_notify: Arc<Notify>,
}

pub struct InboundDatagram {
    pub slot_idx: usize,
    pub generation: u64,
    pub protocol: TransportProtocol,
    pub data: Vec<u8>,
}

pub struct SocketPool {
    slots: RwLock<[Option<Arc<BoundSocket>>; 2]>,
    active_tx: AtomicU8,
    next_gen: AtomicU64,
    remote_addr: RwLock<SocketAddr>,
    loopback_mode: bool,
    bind_cb: BindSocketFn,
    rx_tx: mpsc::Sender<InboundDatagram>,
    simulate_udp_blocked: Arc<AtomicBool>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl SocketPool {
    pub fn new(
        remote_addr: SocketAddr,
        loopback_mode: bool,
        bind_cb: BindSocketFn,
        rx_tx: mpsc::Sender<InboundDatagram>,
    ) -> Self {
        Self {
            slots: RwLock::new([None, None]),
            active_tx: AtomicU8::new(0),
            next_gen: AtomicU64::new(1),
            remote_addr: RwLock::new(remote_addr),
            loopback_mode,
            bind_cb,
            rx_tx,
            simulate_udp_blocked: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_simulate_udp_blocked(&self, blocked: bool) {
        self.simulate_udp_blocked.store(blocked, Ordering::Release);
    }

    pub fn is_udp_blocked_simulated(&self) -> bool {
        self.simulate_udp_blocked.load(Ordering::Acquire)
    }

    pub fn active_slot_index(&self) -> usize {
        self.active_tx.load(Ordering::Acquire) as usize
    }

    pub fn standby_slot_index(&self) -> usize {
        1 - self.active_slot_index()
    }

    pub fn active_socket_info(&self) -> Option<(i64, String)> {
        let idx = self.active_slot_index();
        let guard = self.slots.read().ok()?;
        guard[idx].as_ref().map(|s| (s.net_id, s.label.clone()))
    }

    pub fn active_protocol(&self) -> TransportProtocol {
        let idx = self.active_slot_index();
        self.slots
            .read()
            .ok()
            .and_then(|g| g[idx].as_ref().map(|s| s.protocol))
            .unwrap_or(TransportProtocol::UdpAmneziaWg)
    }

    pub fn slot_protocol(&self, slot_idx: usize) -> Option<TransportProtocol> {
        let guard = self.slots.read().ok()?;
        guard[slot_idx].as_ref().map(|s| s.protocol)
    }

    pub fn standby_socket_info(&self) -> Option<(i64, String)> {
        let idx = self.standby_slot_index();
        let guard = self.slots.read().ok()?;
        guard[idx].as_ref().map(|s| (s.net_id, s.label.clone()))
    }

    pub fn open_slot(&self, slot_idx: usize, net_id: i64, label: String) -> Result<u64, String> {
        self.open_udp_slot(slot_idx, net_id, label)
    }

    pub fn open_udp_slot(&self, slot_idx: usize, net_id: i64, label: String) -> Result<u64, String> {
        let remote = *self
            .remote_addr
            .read()
            .map_err(|_| "Remote addr lock poisoned".to_string())?;

        let domain = if remote.is_ipv4() {
            socket2::Domain::IPV4
        } else {
            socket2::Domain::IPV6
        };
        let sock = socket2::Socket::new(domain, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))
            .map_err(|e| format!("Failed to create UDP socket: {e}"))?;
        sock.set_nonblocking(true)
            .map_err(|e| format!("Failed to set nonblocking: {e}"))?;

        let bind_addr: SocketAddr = if self.loopback_mode {
            "127.0.0.1:0".parse().unwrap()
        } else if remote.is_ipv4() {
            "0.0.0.0:0".parse().unwrap()
        } else {
            "[::]:0".parse().unwrap()
        };
        sock.bind(&bind_addr.into())
            .map_err(|e| format!("Failed to bind local UDP socket: {e}"))?;

        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let raw_fd = sock.as_raw_fd();
            if !(self.bind_cb)(raw_fd, net_id, self.loopback_mode) {
                return Err(format!(
                    "Host rejected protectAndBindSocket for netId={net_id} ({label})"
                ));
            }
        }
        #[cfg(not(unix))]
        {
            let _ = (self.bind_cb)(0, net_id, self.loopback_mode);
        }

        sock.connect(&remote.into())
            .map_err(|e| format!("Failed to connect UDP socket to {remote}: {e}"))?;

        let std_udp: std::net::UdpSocket = sock.into();
        let tokio_udp = Arc::new(
            UdpSocket::from_std(std_udp)
                .map_err(|e| format!("Failed to wrap tokio UdpSocket: {e}"))?,
        );

        let generation = self.next_gen.fetch_add(1, Ordering::Relaxed);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_notify = Arc::new(Notify::new());
        let bound = Arc::new(BoundSocket {
            transport: SlotTransport::Udp(tokio_udp.clone()),
            protocol: TransportProtocol::UdpAmneziaWg,
            net_id,
            label,
            generation,
            cancel: cancel.clone(),
            cancel_notify: cancel_notify.clone(),
        });

        self.install_slot(slot_idx, bound)?;

        let rx_tx = self.rx_tx.clone();
        let sim_blocked = self.simulate_udp_blocked.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            loop {
                tokio::select! {
                    _ = cancel_notify.notified() => break,
                    res = tokio_udp.recv(&mut buf) => {
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        match res {
                            Ok(n) if n > 0 => {
                                if sim_blocked.load(Ordering::Relaxed) {
                                    continue;
                                }
                                let pkt = InboundDatagram {
                                    slot_idx,
                                    generation,
                                    protocol: TransportProtocol::UdpAmneziaWg,
                                    data: buf[..n].to_vec(),
                                };
                                if rx_tx.send(pkt).await.is_err() {
                                    break;
                                }
                            }
                            Ok(_) => continue,
                            Err(e) => {
                                if !cancel.load(Ordering::Relaxed) {
                                    log::debug!("UDP slot {slot_idx} recv ended: {e}");
                                }
                                break;
                            }
                        }
                    }
                }
            }
        });

        Ok(generation)
    }

    pub async fn open_reality_tcp_slot(
        &self,
        slot_idx: usize,
        net_id: i64,
        label: String,
        target: RealityTarget,
    ) -> Result<u64, String> {
        let domain = if target.addr.is_ipv4() {
            socket2::Domain::IPV4
        } else {
            socket2::Domain::IPV6
        };
        let sock =
            socket2::Socket::new(domain, socket2::Type::STREAM, Some(socket2::Protocol::TCP))
                .map_err(|e| format!("Failed to create REALITY TCP socket: {e}"))?;
        sock.set_nonblocking(true)
            .map_err(|e| format!("Failed to set TCP nonblocking: {e}"))?;
        let _ = sock.set_nodelay(true);

        if self.loopback_mode {
            let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
            let _ = sock.bind(&bind_addr.into());
        }

        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let raw_fd = sock.as_raw_fd();
            if !(self.bind_cb)(raw_fd, net_id, self.loopback_mode) {
                return Err(format!(
                    "Host rejected protectAndBindSocket for REALITY netId={net_id} ({label})"
                ));
            }
        }
        #[cfg(not(unix))]
        {
            let _ = (self.bind_cb)(0, net_id, self.loopback_mode);
        }

        let connect_fut = async {
            #[cfg(unix)]
            let mut stream = {
                use std::os::fd::{FromRawFd, IntoRawFd};
                let tokio_sock = unsafe { tokio::net::TcpSocket::from_raw_fd(sock.into_raw_fd()) };
                tokio_sock
                    .connect(target.addr)
                    .await
                    .map_err(|e| format!("REALITY TCP connect to {} failed: {e}", target.addr))?
            };
            #[cfg(not(unix))]
            let mut stream = {
                let _ = sock;
                tokio::net::TcpStream::connect(target.addr)
                    .await
                    .map_err(|e| format!("REALITY TCP connect to {} failed: {e}", target.addr))?
            };

            let _ = stream.set_nodelay(true);
            let sni = if target.sni.trim().is_empty() {
                "dl.google.com"
            } else {
                target.sni.trim()
            };
            let (ch_bytes, hs) =
                build_reality_client_hello(&target.server_pub, &target.short_id, sni, now_secs())?;
            stream
                .write_all(&ch_bytes)
                .await
                .map_err(|e| format!("REALITY ClientHello write error: {e}"))?;
            let sh_bytes = read_tls_handshake_record(&mut stream).await?;
            let cipher = client_finish_reality_handshake(hs, &sh_bytes)?;
            let (mut tcp_rx, mut tcp_tx) = stream.into_split();
            let (mut enc_tx, mut dec_rx) = cipher.split();
            // Mutual AEAD server proof prevents captive-portal/MITM ServerHellos from hijacking cutover
            verify_client_reality_pong(&mut tcp_rx, &mut tcp_tx, &mut enc_tx, &mut dec_rx).await?;
            Ok::<_, String>((tcp_rx, tcp_tx, enc_tx, dec_rx))
        };

        let (mut tcp_rx, mut tcp_tx, mut enc_tx, mut dec_rx) =
            tokio::time::timeout(Duration::from_millis(2500), connect_fut)
                .await
                .map_err(|_| format!("REALITY TCP handshake timed out ({})", target.addr))??;

        let (tcp_out_tx, mut tcp_out_rx) = mpsc::channel::<(u8, Vec<u8>)>(256);

        let generation = self.next_gen.fetch_add(1, Ordering::Relaxed);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_notify = Arc::new(Notify::new());
        let bound = Arc::new(BoundSocket {
            transport: SlotTransport::RealityTcp {
                tx: tcp_out_tx.clone(),
            },
            protocol: TransportProtocol::RealityTcp,
            net_id,
            label,
            generation,
            cancel: cancel.clone(),
            cancel_notify: cancel_notify.clone(),
        });

        self.install_slot(slot_idx, bound)?;

        let cancel_wr = cancel.clone();
        let notify_wr = cancel_notify.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = notify_wr.notified() => break,
                    maybe_msg = tcp_out_rx.recv() => {
                        if cancel_wr.load(Ordering::Relaxed) {
                            break;
                        }
                        match maybe_msg {
                            Some((ft, payload)) => {
                                if write_reality_frame(&mut tcp_tx, &mut enc_tx, ft, &payload).await.is_err() {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        let rx_tx = self.rx_tx.clone();
        let cancel_rd = cancel;
        let notify_rd = cancel_notify;
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = notify_rd.notified() => break,
                    res = read_reality_frame(&mut tcp_rx, &mut dec_rx) => {
                        if cancel_rd.load(Ordering::Relaxed) {
                            break;
                        }
                        match res {
                            Ok((frame_type, payload)) => {
                                if frame_type == REALITY_FRAME_WG && !payload.is_empty() {
                                    let pkt = InboundDatagram {
                                        slot_idx,
                                        generation,
                                        protocol: TransportProtocol::RealityTcp,
                                        data: payload,
                                    };
                                    if rx_tx.send(pkt).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                if !cancel_rd.load(Ordering::Relaxed) {
                                    log::debug!("REALITY TCP slot {slot_idx} read ended: {e}");
                                }
                                break;
                            }
                        }
                    }
                }
            }
        });

        Ok(generation)
    }

    fn install_slot(&self, slot_idx: usize, bound: Arc<BoundSocket>) -> Result<(), String> {
        let mut guard = self
            .slots
            .write()
            .map_err(|_| "Socket slots lock poisoned".to_string())?;
        if let Some(old) = guard[slot_idx].take() {
            old.cancel.store(true, Ordering::Relaxed);
            old.cancel_notify.notify_waiters();
        }
        guard[slot_idx] = Some(bound);
        Ok(())
    }

    pub async fn send_on_active(&self, data: &[u8]) -> Result<usize, String> {
        let idx = self.active_slot_index();
        self.send_on_slot(idx, data).await
    }

    pub async fn send_on_slot(&self, slot_idx: usize, data: &[u8]) -> Result<usize, String> {
        let (transport, protocol) = {
            let guard = self
                .slots
                .read()
                .map_err(|_| "Socket slots lock poisoned".to_string())?;
            let bound = guard[slot_idx]
                .as_ref()
                .ok_or_else(|| format!("Slot {slot_idx} has no active socket"))?;
            let tr = match &bound.transport {
                SlotTransport::Udp(u) => SlotTransport::Udp(u.clone()),
                SlotTransport::RealityTcp { tx } => SlotTransport::RealityTcp { tx: tx.clone() },
            };
            (tr, bound.protocol)
        };

        if protocol == TransportProtocol::UdpAmneziaWg
            && self.simulate_udp_blocked.load(Ordering::Acquire)
        {
            // Simulated ISP UDP block silently drops UDP packets cleanly at carrier boundary
            return Ok(data.len());
        }

        match transport {
            SlotTransport::Udp(sock) => sock
                .send(data)
                .await
                .map_err(|e| format!("UDP send on slot {slot_idx} failed: {e}")),
            SlotTransport::RealityTcp { tx } => {
                tx.send((REALITY_FRAME_WG, data.to_vec()))
                    .await
                    .map_err(|_| format!("REALITY TCP send channel closed on slot {slot_idx}"))?;
                Ok(data.len())
            }
        }
    }

    pub fn swap_active_slot(&self, new_active: usize) {
        self.active_tx.store(new_active as u8, Ordering::Release);
    }

    pub fn get_slot_gen(&self, slot_idx: usize) -> Option<u64> {
        let guard = self.slots.read().ok()?;
        guard[slot_idx].as_ref().map(|s| s.generation)
    }

    pub fn close_slot_if_generation(&self, slot_idx: usize, expected_gen: u64) {
        if let Ok(mut guard) = self.slots.write() {
            let should_remove = guard[slot_idx]
                .as_ref()
                .map(|s| s.generation == expected_gen)
                .unwrap_or(false);
            if should_remove {
                if let Some(old) = guard[slot_idx].take() {
                    old.cancel.store(true, Ordering::Relaxed);
                    old.cancel_notify.notify_waiters();
                }
            }
        }
    }

    pub fn close_all(&self) {
        if let Ok(mut guard) = self.slots.write() {
            for slot in guard.iter_mut() {
                if let Some(s) = slot.take() {
                    s.cancel.store(true, Ordering::Relaxed);
                    s.cancel_notify.notify_waiters();
                }
            }
        }
    }
}
