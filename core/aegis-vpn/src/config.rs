pub use aegis_proto::obfuscator::ObfuscationConfig;
pub use aegis_proto::reality::RealityConfig;
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

fn default_pq_rekey_interval_secs() -> u64 {
    600
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransportMode {
    #[default]
    AutoHappyEyeballs,
    ForceUdp,
    ForceRealityTcp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverConfig {
    pub grace_window_ms: u64,
    pub probe_interval_ms: u64,
    pub rtt_spike_multiplier: f64,
    pub loss_threshold_pct: f64,
    pub wifi_rssi_prewarm_dbm: i32,
}

impl Default for HandoverConfig {
    fn default() -> Self {
        Self {
            grace_window_ms: 2000,
            probe_interval_ms: 1500,
            rtt_spike_multiplier: 2.5,
            loss_threshold_pct: 3.0,
            wifi_rssi_prewarm_dbm: -72,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineConfig {
    pub private_key_base64: String,
    pub peer_public_key_base64: String,
    pub preshared_key_base64: Option<String>,
    pub endpoint: String,
    pub tunnel_ipv4: String,
    pub virtual_dns_ipv4: String,
    pub upstream_dns_ipv4: String,
    pub persistent_keepalive: u16,
    pub loopback_mode: bool,
    pub dns_sinkhole_enabled: bool,
    pub obfuscation: ObfuscationConfig,
    #[serde(default)]
    pub handover: HandoverConfig,
    #[serde(default)]
    pub transport_mode: TransportMode,
    #[serde(default)]
    pub reality: RealityConfig,
    #[serde(default = "default_true")]
    pub pq_enabled: bool,
    #[serde(default = "default_pq_rekey_interval_secs")]
    pub pq_rekey_interval_secs: u64,
    #[serde(default)]
    pub blocked_domains: Vec<String>,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverEventRecord {
    pub timestamp_epoch_ms: u64,
    pub from_path: String,
    pub to_path: String,
    pub cutover_ms: f64,
    pub dropped_packets: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsEventRecord {
    pub timestamp_epoch_ms: u64,
    pub domain: String,
    pub blocked: bool,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TelemetrySnapshot {
    pub connected: bool,
    pub handshake_established: bool,
    pub active_path_label: String,
    pub active_net_id: i64,
    pub standby_path_label: Option<String>,
    pub standby_net_id: Option<i64>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rtt_ms: f64,
    pub jitter_ms: f64,
    pub packet_loss_pct: f64,
    pub dns_queries_total: u64,
    pub dns_queries_blocked: u64,
    pub handover_count: u64,
    pub last_handover_ms: f64,
    pub total_handover_dropped_packets: u64,
    pub recent_handovers: Vec<HandoverEventRecord>,
    pub recent_dns_events: Vec<DnsEventRecord>,
    pub transport_protocol: String,
    pub pq_cipher_suite: String,
    pub pq_epoch: u64,
    pub pq_last_rotate_epoch_ms: u64,
    pub udp_blocked_simulated: bool,
}
