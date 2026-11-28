use rand::Rng;
use serde::{Deserialize, Serialize};

const WG_HANDSHAKE_INIT: u32 = 1;
const WG_HANDSHAKE_RESP: u32 = 2;
const WG_COOKIE_REPLY: u32 = 3;
const WG_DATA: u32 = 4;

const WG_HANDSHAKE_INIT_SZ: usize = 148;
const WG_HANDSHAKE_RESP_SZ: usize = 92;
const WG_COOKIE_REPLY_SZ: usize = 64;
const WG_DATA_MIN_SZ: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObfuscationConfig {
    pub enabled: bool,
    pub jc: u8,
    pub jmin: u16,
    pub jmax: u16,
    pub s1: u16,
    pub s2: u16,
    pub h1: u32,
    pub h2: u32,
    pub h3: u32,
    pub h4: u32,
}

impl Default for ObfuscationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            jc: 4,
            jmin: 50,
            jmax: 600,
            s1: 42,
            s2: 76,
            h1: 1_928_374_651,
            h2: 2_093_847_561,
            h3: 3_182_736_451,
            h4: 4_012_345_679,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AmneziaObfuscator {
    cfg: ObfuscationConfig,
}

impl AmneziaObfuscator {
    pub fn new(cfg: ObfuscationConfig) -> Self {
        Self { cfg }
    }

    pub fn is_active(&self) -> bool {
        self.cfg.enabled
            && (self.cfg.jc > 0
                || self.cfg.s1 > 0
                || self.cfg.s2 > 0
                || self.cfg.h1 != WG_HANDSHAKE_INIT
                || self.cfg.h2 != WG_HANDSHAKE_RESP
                || self.cfg.h3 != WG_COOKIE_REPLY
                || self.cfg.h4 != WG_DATA)
    }

    pub fn config(&self) -> &ObfuscationConfig {
        &self.cfg
    }

    pub fn generate_junk_packets(&self) -> Vec<Vec<u8>> {
        if !self.cfg.enabled || self.cfg.jc == 0 {
            return Vec::new();
        }
        let mut rng = rand::thread_rng();
        let min_len = (self.cfg.jmin as usize).max(8);
        let max_len = (self.cfg.jmax as usize).max(min_len + 1).min(1280);
        let init_len = WG_HANDSHAKE_INIT_SZ + self.cfg.s1 as usize;
        let resp_len = WG_HANDSHAKE_RESP_SZ + self.cfg.s2 as usize;

        let mut packets = Vec::with_capacity(self.cfg.jc as usize);
        for _ in 0..self.cfg.jc {
            let mut len = rng.gen_range(min_len..=max_len);
            if len == init_len || len == resp_len || len == WG_COOKIE_REPLY_SZ {
                len = (len + 3).min(1280);
            }
            let mut buf = vec![0u8; len];
            rng.fill(&mut buf[..]);
            if buf.len() >= 4 {
                let hdr = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
                if hdr == self.cfg.h4 || hdr == self.cfg.h3 || hdr == WG_DATA {
                    buf[0] ^= 0x5A;
                }
            }
            packets.push(buf);
        }
        packets
    }

    pub fn obfuscate_outbound(&self, wg_pkt: &[u8]) -> (bool, Vec<u8>) {
        if wg_pkt.len() < 4 {
            return (false, wg_pkt.to_vec());
        }
        let msg_type = u32::from_le_bytes([wg_pkt[0], wg_pkt[1], wg_pkt[2], wg_pkt[3]]);
        let is_init = msg_type == WG_HANDSHAKE_INIT;

        if !self.is_active() {
            return (is_init, wg_pkt.to_vec());
        }

        let mut rng = rand::thread_rng();
        match msg_type {
            WG_HANDSHAKE_INIT => {
                let pad_len = self.cfg.s1 as usize;
                let mut out = vec![0u8; pad_len + wg_pkt.len()];
                if pad_len > 0 {
                    rng.fill(&mut out[..pad_len]);
                }
                out[pad_len..pad_len + 4].copy_from_slice(&self.cfg.h1.to_le_bytes());
                out[pad_len + 4..].copy_from_slice(&wg_pkt[4..]);
                (true, out)
            }
            WG_HANDSHAKE_RESP => {
                let pad_len = self.cfg.s2 as usize;
                let mut out = vec![0u8; pad_len + wg_pkt.len()];
                if pad_len > 0 {
                    rng.fill(&mut out[..pad_len]);
                }
                out[pad_len..pad_len + 4].copy_from_slice(&self.cfg.h2.to_le_bytes());
                out[pad_len + 4..].copy_from_slice(&wg_pkt[4..]);
                (false, out)
            }
            WG_COOKIE_REPLY => {
                let mut out = wg_pkt.to_vec();
                out[0..4].copy_from_slice(&self.cfg.h3.to_le_bytes());
                (false, out)
            }
            WG_DATA => {
                let mut out = wg_pkt.to_vec();
                out[0..4].copy_from_slice(&self.cfg.h4.to_le_bytes());
                (false, out)
            }
            _ => (false, wg_pkt.to_vec()),
        }
    }

    pub fn deobfuscate_inbound(&self, pkt: &[u8]) -> Option<Vec<u8>> {
        if pkt.len() < 4 {
            return None;
        }
        if !self.is_active() {
            return Some(pkt.to_vec());
        }

        let s1 = self.cfg.s1 as usize;
        if pkt.len() == s1 + WG_HANDSHAKE_INIT_SZ {
            let hdr = u32::from_le_bytes([pkt[s1], pkt[s1 + 1], pkt[s1 + 2], pkt[s1 + 3]]);
            if hdr == self.cfg.h1 {
                let mut out = pkt[s1..].to_vec();
                out[0..4].copy_from_slice(&WG_HANDSHAKE_INIT.to_le_bytes());
                return Some(out);
            }
        }

        let s2 = self.cfg.s2 as usize;
        if pkt.len() == s2 + WG_HANDSHAKE_RESP_SZ {
            let hdr = u32::from_le_bytes([pkt[s2], pkt[s2 + 1], pkt[s2 + 2], pkt[s2 + 3]]);
            if hdr == self.cfg.h2 {
                let mut out = pkt[s2..].to_vec();
                out[0..4].copy_from_slice(&WG_HANDSHAKE_RESP.to_le_bytes());
                return Some(out);
            }
        }

        let hdr0 = u32::from_le_bytes([pkt[0], pkt[1], pkt[2], pkt[3]]);
        if pkt.len() == WG_COOKIE_REPLY_SZ && hdr0 == self.cfg.h3 {
            let mut out = pkt.to_vec();
            out[0..4].copy_from_slice(&WG_COOKIE_REPLY.to_le_bytes());
            return Some(out);
        }

        if pkt.len() >= WG_DATA_MIN_SZ && hdr0 == self.cfg.h4 {
            let mut out = pkt.to_vec();
            out[0..4].copy_from_slice(&WG_DATA.to_le_bytes());
            return Some(out);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obfuscator_roundtrip_all_packet_types() {
        let obf = AmneziaObfuscator::new(ObfuscationConfig::default());
        assert!(obf.is_active());

        let mut init_pkt = vec![0xAAu8; 148];
        init_pkt[0..4].copy_from_slice(&1u32.to_le_bytes());
        let (is_init, obf_init) = obf.obfuscate_outbound(&init_pkt);
        assert!(is_init);
        assert_eq!(obf_init.len(), 148 + obf.config().s1 as usize);
        assert_eq!(obf.deobfuscate_inbound(&obf_init), Some(init_pkt));

        let mut resp_pkt = vec![0xBBu8; 92];
        resp_pkt[0..4].copy_from_slice(&2u32.to_le_bytes());
        let (_, obf_resp) = obf.obfuscate_outbound(&resp_pkt);
        assert_eq!(obf.deobfuscate_inbound(&obf_resp), Some(resp_pkt));

        let mut data_pkt = vec![0xCCu8; 64];
        data_pkt[0..4].copy_from_slice(&4u32.to_le_bytes());
        let (_, obf_data) = obf.obfuscate_outbound(&data_pkt);
        assert_eq!(obf.deobfuscate_inbound(&obf_data), Some(data_pkt));
    }
}
