use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use boringtun::noise::{Tunn, TunnResult};
use boringtun::x25519::{PublicKey, StaticSecret};
use std::net::IpAddr;
use std::time::{Duration, Instant};

pub fn decode_key_32(b64: &str) -> Result<[u8; 32], String> {
    let bytes = BASE64
        .decode(b64.trim())
        .map_err(|e| format!("Invalid base64 key: {e}"))?;
    if bytes.len() != 32 {
        return Err(format!("Expected 32-byte key, got {}", bytes.len()));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

pub fn encode_key_32(bytes: &[u8; 32]) -> String {
    BASE64.encode(bytes)
}

pub fn generate_keypair_base64() -> (String, String) {
    let secret = StaticSecret::random_from_rng(rand::thread_rng());
    let public = PublicKey::from(&secret);
    (
        BASE64.encode(secret.to_bytes()),
        BASE64.encode(public.as_bytes()),
    )
}

pub enum DecapAction {
    Done,
    WriteToNetwork(Vec<u8>),
    WriteToTunnelV4(Vec<u8>),
    WriteToTunnelV6(Vec<u8>),
    Err(String),
}

struct SessionSlot {
    tunn: Tunn,
    established: bool,
}

struct PendingSlot {
    epoch: u64,
    psk: [u8; 32],
    slot: SessionSlot,
    is_responder: bool,
}

enum DrainTarget {
    Active,
    Pending,
    Retiring,
}

pub struct WireGuardSession {
    priv_bytes: [u8; 32],
    peer_pub_bytes: [u8; 32],
    current_psk: Option<[u8; 32]>,
    keepalive: Option<u16>,
    next_index: u32,
    active: SessionSlot,
    pending: Option<PendingSlot>,
    retiring: Option<(SessionSlot, Instant)>,
    last_drain: DrainTarget,
    pub pq_epoch: u64,
    pub handshake_established: bool,
}

impl WireGuardSession {
    pub fn new(
        private_key_b64: &str,
        peer_public_key_b64: &str,
        preshared_key_b64: Option<&str>,
        persistent_keepalive: u16,
        index: u32,
    ) -> Result<Self, String> {
        let priv_bytes = decode_key_32(private_key_b64)?;
        let peer_pub_bytes = decode_key_32(peer_public_key_b64)?;
        let psk = match preshared_key_b64 {
            Some(s) if !s.trim().is_empty() => Some(decode_key_32(s)?),
            _ => None,
        };

        let secret = StaticSecret::from(priv_bytes);
        let peer_pub = PublicKey::from(peer_pub_bytes);
        let keepalive = if persistent_keepalive > 0 {
            Some(persistent_keepalive)
        } else {
            None
        };

        let tunn = Tunn::new(secret, peer_pub, psk, keepalive, index, None);
        Ok(Self {
            priv_bytes,
            peer_pub_bytes,
            current_psk: psk,
            keepalive,
            next_index: index.saturating_add(1),
            active: SessionSlot {
                tunn,
                established: false,
            },
            pending: None,
            retiring: None,
            last_drain: DrainTarget::Active,
            pq_epoch: 0,
            handshake_established: false,
        })
    }

    pub fn initiate_handshake<'a>(&mut self, dst: &'a mut [u8], force: bool) -> TunnResult<'a> {
        self.active.tunn.format_handshake_initiation(dst, force)
    }

    /// Stages a new dual-session `Tunn` with the PQ-derived hybrid `PresharedKey` without
    /// interrupting `active` traffic until the new handshake completes (Zero-Drop PQ Rekey).
    pub fn stage_pq_rekey(
        &mut self,
        epoch: u64,
        new_psk: [u8; 32],
        is_responder: bool,
    ) -> Option<Vec<u8>> {
        let idx = self.next_index;
        self.next_index = self.next_index.wrapping_add(1);
        if self.next_index == 0 {
            self.next_index = 1;
        }
        let secret = StaticSecret::from(self.priv_bytes);
        let peer_pub = PublicKey::from(self.peer_pub_bytes);
        let mut tunn = Tunn::new(secret, peer_pub, Some(new_psk), self.keepalive, idx, None);

        let init_pkt = if !is_responder {
            let mut buf = vec![0u8; 512];
            match tunn.format_handshake_initiation(&mut buf, true) {
                TunnResult::WriteToNetwork(slice) => Some(slice.to_vec()),
                _ => None,
            }
        } else {
            None
        };

        self.pending = Some(PendingSlot {
            epoch,
            psk: new_psk,
            slot: SessionSlot {
                tunn,
                established: false,
            },
            is_responder,
        });
        init_pkt
    }

    pub fn emit_pending_handshake_init(&mut self) -> Option<Vec<u8>> {
        let pending = self.pending.as_mut()?;
        let mut buf = vec![0u8; 512];
        match pending.slot.tunn.format_handshake_initiation(&mut buf, true) {
            TunnResult::WriteToNetwork(slice) => Some(slice.to_vec()),
            _ => None,
        }
    }

    pub fn encapsulate<'a>(&mut self, src: &[u8], dst: &'a mut [u8]) -> TunnResult<'a> {
        self.active.tunn.encapsulate(src, dst)
    }

    fn convert_result(res: TunnResult<'_>) -> DecapAction {
        match res {
            TunnResult::Done => DecapAction::Done,
            TunnResult::WriteToNetwork(b) => DecapAction::WriteToNetwork(b.to_vec()),
            TunnResult::WriteToTunnelV4(b, _) => DecapAction::WriteToTunnelV4(b.to_vec()),
            TunnResult::WriteToTunnelV6(b, _) => DecapAction::WriteToTunnelV6(b.to_vec()),
            TunnResult::Err(e) => DecapAction::Err(format!("{e:?}")),
        }
    }

    fn promote_pending(&mut self) {
        if let Some(p) = self.pending.take() {
            self.current_psk = Some(p.psk);
            self.pq_epoch = p.epoch;
            let old_active = std::mem::replace(&mut self.active, p.slot);
            self.retiring = Some((old_active, Instant::now()));
            self.last_drain = DrainTarget::Active;
            self.handshake_established = true;
        }
    }

    pub fn decapsulate(&mut self, src_addr: Option<IpAddr>, datagram: &[u8]) -> DecapAction {
        let mut dst = [0u8; 4096];

        if datagram.is_empty() {
            let res = match self.last_drain {
                DrainTarget::Active => self.active.tunn.decapsulate(None, &[], &mut dst),
                DrainTarget::Pending => match self.pending.as_mut() {
                    Some(p) => p.slot.tunn.decapsulate(None, &[], &mut dst),
                    None => self.active.tunn.decapsulate(None, &[], &mut dst),
                },
                DrainTarget::Retiring => match self.retiring.as_mut() {
                    Some((slot, _)) => slot.tunn.decapsulate(None, &[], &mut dst),
                    None => TunnResult::Done,
                },
            };
            return Self::convert_result(res);
        }

        let parsed_pkt = Tunn::parse_incoming_packet(datagram).ok();
        let is_handshake_init = matches!(parsed_pkt, Some(boringtun::noise::Packet::HandshakeInit(_)));
        let is_handshake_resp_or_cookie = matches!(
            parsed_pkt,
            Some(boringtun::noise::Packet::HandshakeResponse(_))
                | Some(boringtun::noise::Packet::PacketCookieReply(_))
        );
        let is_data = matches!(parsed_pkt, Some(boringtun::noise::Packet::PacketData(_)));

        // 1. Try pending dual-Tunn PQ slot when appropriate (new handshake or pending responder)
        if let Some(ref mut pending) = self.pending {
            let should_try_pending = if pending.is_responder {
                is_handshake_init || (is_data && pending.slot.established)
            } else {
                is_handshake_resp_or_cookie || (is_data && pending.slot.established)
            };

            if should_try_pending {
                let action = Self::convert_result(pending.slot.tunn.decapsulate(
                    src_addr,
                    datagram,
                    &mut dst,
                ));
                if !matches!(action, DecapAction::Err(_)) {
                    let established_now = if pending.is_responder {
                        is_handshake_init && matches!(action, DecapAction::WriteToNetwork(_))
                    } else {
                        is_handshake_resp_or_cookie
                            && matches!(action, DecapAction::WriteToNetwork(_))
                    };
                    if established_now || is_data {
                        pending.slot.established = true;
                        self.last_drain = DrainTarget::Pending;
                        // Promote immediately on initiator upon HandshakeResponse,
                        // and on responder upon HandshakeInit response (retiring keeps old RX alive)
                        self.promote_pending();
                    }
                    return action;
                }
            }
        }

        // 2. Normal active session
        let active_action = Self::convert_result(self.active.tunn.decapsulate(
            src_addr,
            datagram,
            &mut dst,
        ));
        if !matches!(active_action, DecapAction::Err(_)) {
            self.last_drain = DrainTarget::Active;
            if (is_handshake_resp_or_cookie || is_data || is_handshake_init)
                && matches!(
                    active_action,
                    DecapAction::WriteToTunnelV4(_)
                        | DecapAction::WriteToTunnelV6(_)
                        | DecapAction::WriteToNetwork(_)
                )
            {
                self.active.established = true;
                self.handshake_established = true;
            }
            return active_action;
        }

        // 3. Fallback to retiring session during PQ rotation grace window (Zero-Drop RX)
        if let Some((ref mut retiring_slot, started)) = self.retiring {
            if started.elapsed() < Duration::from_secs(3) {
                let ret_action = Self::convert_result(retiring_slot.tunn.decapsulate(
                    src_addr,
                    datagram,
                    &mut dst,
                ));
                if !matches!(ret_action, DecapAction::Err(_)) {
                    self.last_drain = DrainTarget::Retiring;
                    return ret_action;
                }
            } else {
                self.retiring = None;
            }
        }

        active_action
    }

    pub fn update_timers<'a>(&mut self, dst: &'a mut [u8]) -> TunnResult<'a> {
        if let Some((_, started)) = self.retiring {
            if started.elapsed() >= Duration::from_secs(3) {
                self.retiring = None;
            }
        }
        self.active.tunn.update_timers(dst)
    }
}
