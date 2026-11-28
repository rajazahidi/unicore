pub mod path_scorer;
pub mod socket_pool;

use std::collections::VecDeque;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{HandoverConfig, HandoverEventRecord};
use crate::packet::ip_parser::{build_ipv4_udp_packet, parse_ip_packet, parse_udp, TransportProto};
use path_scorer::PathScorer;
use socket_pool::{RealityTarget, SocketPool};

pub const PROBE_PORT: u16 = 5555;
const PROBE_MAGIC: &[u8] = b"AEGIS_PROBE:";

pub fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn build_inner_probe_packet(
    tunnel_ip: Ipv4Addr,
    echo_ip: Ipv4Addr,
    seq: u64,
    sent_ms: u64,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(32);
    payload.extend_from_slice(PROBE_MAGIC);
    payload.extend_from_slice(&seq.to_be_bytes());
    payload.extend_from_slice(&sent_ms.to_be_bytes());
    build_ipv4_udp_packet(tunnel_ip, echo_ip, PROBE_PORT, PROBE_PORT, &payload)
}

/// Checks whether a decrypted IPv4 packet is an echoed internal handover RTT probe.
/// Returns `Some((seq, rtt_ms))` if it is a probe reply.
pub fn parse_inner_probe_reply(ip_pkt: &[u8]) -> Option<(u64, f64)> {
    let parsed = parse_ip_packet(ip_pkt)?;
    if parsed.proto != TransportProto::Udp {
        return None;
    }
    let udp = parse_udp(parsed.payload)?;
    if udp.src_port != PROBE_PORT && udp.dst_port != PROBE_PORT {
        return None;
    }
    if !udp.payload.starts_with(PROBE_MAGIC) || udp.payload.len() < PROBE_MAGIC.len() + 16 {
        return None;
    }
    let offset = PROBE_MAGIC.len();
    let seq = u64::from_be_bytes(udp.payload[offset..offset + 8].try_into().ok()?);
    let sent_ms = u64::from_be_bytes(udp.payload[offset + 8..offset + 16].try_into().ok()?);
    let now = now_epoch_ms();
    let rtt = now.saturating_sub(sent_ms) as f64;
    Some((seq, rtt.max(0.2)))
}

pub struct HandoverCoordinator {
    pub cfg: HandoverConfig,
    pub pool: Arc<SocketPool>,
    pub scorer: PathScorer,
    pub handover_count: u64,
    pub last_handover_ms: f64,
    pub total_dropped_packets: u64,
    pub history: VecDeque<HandoverEventRecord>,
    pending_standby: Option<(usize, u64, i64, String)>,
}

impl HandoverCoordinator {
    pub fn new(cfg: HandoverConfig, pool: Arc<SocketPool>) -> Self {
        let scorer = PathScorer::new(cfg.clone());
        Self {
            cfg,
            pool,
            scorer,
            handover_count: 0,
            last_handover_ms: 0.0,
            total_dropped_packets: 0,
            history: VecDeque::with_capacity(32),
            pending_standby: None,
        }
    }

    /// Prepares a standby UDP socket on the alternate network BEFORE cutting over active traffic.
    pub fn prepare_standby(&mut self, net_id: i64, label: String) -> Result<usize, String> {
        let standby_slot = self.pool.standby_slot_index();
        let generation = self.pool.open_udp_slot(standby_slot, net_id, label.clone())?;
        self.pending_standby = Some((standby_slot, generation, net_id, label));
        Ok(standby_slot)
    }

    /// Prepares a standby Zero-Domain TCP/443 REALITY TLS 1.3 transport BEFORE cutting over.
    pub async fn prepare_standby_reality(
        &mut self,
        net_id: i64,
        label: String,
        target: RealityTarget,
    ) -> Result<usize, String> {
        let standby_slot = self.pool.standby_slot_index();
        let generation = self
            .pool
            .open_reality_tcp_slot(standby_slot, net_id, label.clone(), target)
            .await?;
        self.pending_standby = Some((standby_slot, generation, net_id, label));
        Ok(standby_slot)
    }

    pub fn discard_standby(&mut self) {
        if let Some((slot, generation, _, _)) = self.pending_standby.take() {
            if self.pool.active_slot_index() != slot {
                self.pool.close_slot_if_generation(slot, generation);
            }
        }
    }

    pub fn standby_slot_ready(&self) -> Option<usize> {
        self.pending_standby.as_ref().map(|(slot, _, _, _)| *slot)
    }

    /// Atomically promotes the standby socket to active TX while keeping the old socket open
    /// for `grace_window_ms` to catch all in-flight RX packets (Zero-Drop Make-Before-Break).
    pub fn execute_atomic_cutover(&mut self, reason: &str, elapsed_ms: f64) -> Option<HandoverEventRecord> {
        let (new_slot, _new_gen, _new_net_id, new_label) = self.pending_standby.take()?;
        let old_slot = self.pool.active_slot_index();
        if old_slot == new_slot {
            return None;
        }

        let old_info = self.pool.active_socket_info();
        let old_gen = self.pool.get_slot_gen(old_slot);
        let from_label = old_info
            .as_ref()
            .map(|(_, l)| l.clone())
            .unwrap_or_else(|| "Primary".to_string());

        self.pool.swap_active_slot(new_slot);

        if let Some(expected_gen) = old_gen {
            let grace_ms = self.cfg.grace_window_ms;
            let pool_ref = self.pool.clone();
            // Keep old RX slot alive during grace window so zero in-flight packets drop
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(grace_ms)).await;
                if pool_ref.active_slot_index() != old_slot {
                    pool_ref.close_slot_if_generation(old_slot, expected_gen);
                }
            });
        }

        self.handover_count += 1;
        self.last_handover_ms = elapsed_ms;

        let event = HandoverEventRecord {
            timestamp_epoch_ms: now_epoch_ms(),
            from_path: from_label,
            to_path: new_label,
            cutover_ms: (elapsed_ms * 100.0).round() / 100.0,
            dropped_packets: 0,
            reason: reason.to_string(),
        };
        if self.history.len() >= 24 {
            self.history.pop_front();
        }
        self.history.push_back(event.clone());
        Some(event)
    }
}
