use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::{HashMap, VecDeque};
use subtle::ConstantTimeEq;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};

type HmacSha256 = Hmac<Sha256>;

pub const REALITY_FRAME_WG: u8 = 0x01;
pub const REALITY_FRAME_PQ: u8 = 0x02;
pub const REALITY_FRAME_PING: u8 = 0x03;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RealityConfig {
    pub enabled: bool,
    pub sni: String,
    pub short_id_hex: String,
    pub server_public_key_base64: String,
    pub reality_port: u16,
}

impl Default for RealityConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sni: "dl.google.com".to_string(),
            short_id_hex: "aegis001aegis001".to_string(),
            server_public_key_base64: String::new(),
            reality_port: 443,
        }
    }
}

pub fn parse_short_id_8(hex_str: &str) -> Result<[u8; 8], String> {
    let clean = hex_str.trim();
    let mut out = [0u8; 8];
    if clean.len() == 16 && clean.bytes().all(|b| b.is_ascii_hexdigit()) {
        for (i, slot) in out.iter_mut().enumerate() {
            let s = &clean[i * 2..i * 2 + 2];
            *slot = u8::from_str_radix(s, 16).map_err(|e| format!("Invalid hex: {e}"))?;
        }
        return Ok(out);
    }
    // Deterministically hash arbitrary short IDs so human-friendly strings work seamlessly too
    let hk = Hkdf::<Sha256>::new(Some(b"AegisShortId"), clean.as_bytes());
    hk.expand(b"short-id-8", &mut out)
        .map_err(|e| format!("HKDF error: {e}"))?;
    Ok(out)
}

pub struct RealityClientHandshake {
    pub client_random: [u8; 32],
    pub session_id: [u8; 32],
    pub eph_secret: StaticSecret,
    pub auth_shared: [u8; 32],
}

pub struct VerifiedRealityClient {
    pub sni: String,
    pub short_id: [u8; 8],
    pub client_random: [u8; 32],
    pub session_id: [u8; 32],
    pub client_eph_pub: [u8; 32],
    pub auth_shared: [u8; 32],
}

/// Bounded anti-replay cache blocking captured ClientHello replays within the ±180s window.
#[derive(Default)]
pub struct RealityReplayCache {
    seen: HashMap<[u8; 32], u64>,
    order: VecDeque<([u8; 32], u64)>,
}

impl RealityReplayCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check_and_insert(&mut self, session_id: [u8; 32], now_secs: u64) -> bool {
        while let Some(&(old_id, ts)) = self.order.front() {
            if now_secs.saturating_sub(ts) > 200 || self.seen.len() >= 8192 {
                self.order.pop_front();
                if self.seen.get(&old_id).copied() == Some(ts) {
                    self.seen.remove(&old_id);
                }
            } else {
                break;
            }
        }
        if self.seen.contains_key(&session_id) {
            return false;
        }
        self.seen.insert(session_id, now_secs);
        self.order.push_back((session_id, now_secs));
        true
    }
}

#[derive(Clone)]
pub struct RealityTxCipher {
    cipher: ChaCha20Poly1305,
    dir: u8,
    seq: u64,
}

#[derive(Clone)]
pub struct RealityRxCipher {
    cipher: ChaCha20Poly1305,
    dir: u8,
    seq: u64,
}

#[derive(Clone)]
pub struct RealityStreamCipher {
    pub tx: RealityTxCipher,
    pub rx: RealityRxCipher,
}

impl RealityTxCipher {
    pub fn encrypt_record(&mut self, frame_type: u8, payload: &[u8]) -> Result<Vec<u8>, String> {
        if payload.len() > 16000 {
            return Err(format!("REALITY frame payload too large: {}", payload.len()));
        }
        let seq = self.seq;
        self.seq = self
            .seq
            .checked_add(1)
            .ok_or_else(|| "REALITY TX sequence exhausted".to_string())?;

        let mut plain = Vec::with_capacity(1 + payload.len());
        plain.push(frame_type);
        plain.extend_from_slice(payload);

        let nonce = make_nonce(self.dir, seq);
        let ct = self
            .cipher
            .encrypt(&nonce, plain.as_slice())
            .map_err(|_| "AEAD encrypt error".to_string())?;

        // Wrap in TLS 1.3 Application Data record header (0x17 0x03 0x03 len)
        let mut rec = Vec::with_capacity(5 + ct.len());
        rec.push(0x17);
        rec.push(0x03);
        rec.push(0x03);
        rec.extend_from_slice(&(ct.len() as u16).to_be_bytes());
        rec.extend_from_slice(&ct);
        Ok(rec)
    }
}

impl RealityRxCipher {
    pub fn decrypt_record_payload(&mut self, ct: &[u8]) -> Result<(u8, Vec<u8>), String> {
        let seq = self.seq;
        self.seq = self
            .seq
            .checked_add(1)
            .ok_or_else(|| "REALITY RX sequence exhausted".to_string())?;
        let nonce = make_nonce(self.dir, seq);
        let plain = self
            .cipher
            .decrypt(&nonce, ct)
            .map_err(|_| "AEAD decrypt error".to_string())?;
        if plain.is_empty() {
            return Err("Empty REALITY frame".to_string());
        }
        Ok((plain[0], plain[1..].to_vec()))
    }
}

fn make_nonce(dir: u8, seq: u64) -> Nonce {
    let mut n = [0u8; 12];
    n[0] = dir;
    n[4..12].copy_from_slice(&seq.to_be_bytes());
    *Nonce::from_slice(&n)
}

impl RealityStreamCipher {
    fn new(tx_key: [u8; 32], tx_dir: u8, rx_key: [u8; 32], rx_dir: u8) -> Self {
        Self {
            tx: RealityTxCipher {
                cipher: ChaCha20Poly1305::new((&tx_key).into()),
                dir: tx_dir,
                seq: 0,
            },
            rx: RealityRxCipher {
                cipher: ChaCha20Poly1305::new((&rx_key).into()),
                dir: rx_dir,
                seq: 0,
            },
        }
    }

    pub fn split(self) -> (RealityTxCipher, RealityRxCipher) {
        (self.tx, self.rx)
    }

    pub fn encrypt_record(&mut self, frame_type: u8, payload: &[u8]) -> Result<Vec<u8>, String> {
        self.tx.encrypt_record(frame_type, payload)
    }

    pub fn decrypt_record_payload(&mut self, ct: &[u8]) -> Result<(u8, Vec<u8>), String> {
        self.rx.decrypt_record_payload(ct)
    }
}

/// Constructs an authentic TLS 1.3 ClientHello embedding an X25519 KeyShare and
/// authenticated ShortID/timestamp token inside the 32-byte `legacy_session_id`.
pub fn build_reality_client_hello(
    server_pub: &[u8; 32],
    short_id: &[u8; 8],
    sni: &str,
    now_secs: u64,
) -> Result<(Vec<u8>, RealityClientHandshake), String> {
    let mut rng = rand::thread_rng();
    let mut client_random = [0u8; 32];
    rng.fill_bytes(&mut client_random);

    let eph_secret = StaticSecret::random_from_rng(rng);
    let eph_pub = PublicKey::from(&eph_secret);
    let server_pk = PublicKey::from(*server_pub);
    let auth_shared = *eph_secret.diffie_hellman(&server_pk).as_bytes();

    let auth_key = derive_auth_key(&auth_shared, short_id);
    let mut session_id = [0u8; 32];
    let ts_bytes = now_secs.to_be_bytes();
    for i in 0..8 {
        session_id[i] = ts_bytes[i] ^ auth_key[i];
        session_id[8 + i] = short_id[i] ^ auth_key[8 + i];
    }
    let tag = compute_client_auth_tag(&auth_key, &client_random, now_secs, sni);
    session_id[16..32].copy_from_slice(&tag[..16]);

    // Build TLS Extensions
    let mut exts = Vec::new();
    // 1. SNI (0x0000)
    let sni_bytes = sni.as_bytes();
    let mut sni_ext = Vec::new();
    let sni_list_len = (3 + sni_bytes.len()) as u16;
    sni_ext.extend_from_slice(&sni_list_len.to_be_bytes());
    sni_ext.push(0x00); // host_name
    sni_ext.extend_from_slice(&(sni_bytes.len() as u16).to_be_bytes());
    sni_ext.extend_from_slice(sni_bytes);
    push_extension(&mut exts, 0x0000, &sni_ext);

    // 2. ALPN (0x0010): h2, http/1.1
    let alpn_payload: &[u8] = &[0x00, 0x0c, 0x02, b'h', b'2', 0x08, b'h', b't', b't', b'p', b'/', b'1', b'.', b'1'];
    push_extension(&mut exts, 0x0010, alpn_payload);

    // 3. supported_groups (0x000a): x25519 (0x001d), secp256r1 (0x0017)
    push_extension(&mut exts, 0x000a, &[0x00, 0x04, 0x00, 0x1d, 0x00, 0x17]);

    // 4. supported_versions (0x002b): TLS 1.3 (0x0304), TLS 1.2 (0x0303)
    push_extension(&mut exts, 0x002b, &[0x04, 0x03, 0x04, 0x03, 0x03]);

    // 5. key_share (0x0033): X25519 ephemeral public key (32 bytes)
    let mut ks = Vec::new();
    ks.extend_from_slice(&36u16.to_be_bytes()); // client_shares length
    ks.extend_from_slice(&0x001du16.to_be_bytes()); // x25519
    ks.extend_from_slice(&32u16.to_be_bytes());
    ks.extend_from_slice(eph_pub.as_bytes());
    push_extension(&mut exts, 0x0033, &ks);

    // Build ClientHello body
    let mut ch = Vec::new();
    ch.extend_from_slice(&[0x03, 0x03]); // legacy_version TLS 1.2
    ch.extend_from_slice(&client_random);
    ch.push(32); // session_id length
    ch.extend_from_slice(&session_id);
    // Cipher suites (TLS_AES_128_GCM_SHA256, TLS_AES_256_GCM_SHA384, TLS_CHACHA20_POLY1305_SHA256)
    ch.extend_from_slice(&[0x00, 0x06, 0x13, 0x01, 0x13, 0x02, 0x13, 0x03]);
    ch.extend_from_slice(&[0x01, 0x00]); // legacy_compression_methods: null
    ch.extend_from_slice(&(exts.len() as u16).to_be_bytes());
    ch.extend_from_slice(&exts);

    // Handshake header (type 0x01 + u24 len)
    let mut hs = Vec::with_capacity(4 + ch.len());
    hs.push(0x01);
    let ch_len = ch.len() as u32;
    hs.push(((ch_len >> 16) & 0xff) as u8);
    hs.push(((ch_len >> 8) & 0xff) as u8);
    hs.push((ch_len & 0xff) as u8);
    hs.extend_from_slice(&ch);

    // TLS Record header (0x16 0x03 0x01 + u16 len)
    let mut record = Vec::with_capacity(5 + hs.len());
    record.extend_from_slice(&[0x16, 0x03, 0x01]);
    record.extend_from_slice(&(hs.len() as u16).to_be_bytes());
    record.extend_from_slice(&hs);

    Ok((
        record,
        RealityClientHandshake {
            client_random,
            session_id,
            eph_secret,
            auth_shared,
        },
    ))
}

/// Verifies an incoming TLS ClientHello on the VPS gateway.
/// Returns `None` if this is a normal browser, curl, or active DPI prober (causing gateway to relay to decoy).
pub fn verify_reality_client_hello(
    raw: &[u8],
    server_priv: &[u8; 32],
    allowed_short_ids: &[[u8; 8]],
    now_secs: u64,
) -> Option<VerifiedRealityClient> {
    if raw.len() < 48 || raw[0] != 0x16 || raw[5] != 0x01 {
        return None;
    }
    let rec_len = u16::from_be_bytes([raw[3], raw[4]]) as usize;
    if raw.len() < 5 + rec_len || rec_len < 42 {
        return None;
    }
    let ch = &raw[9..5 + rec_len]; // Skip Record(5) + Handshake(4)
    if ch.len() < 35 {
        return None;
    }
    let mut client_random = [0u8; 32];
    client_random.copy_from_slice(&ch[2..34]);
    let sid_len = ch[34] as usize;
    if sid_len != 32 || ch.len() < 35 + sid_len + 2 {
        return None;
    }
    let session_id: [u8; 32] = ch[35..67].try_into().ok()?;

    let mut pos = 67usize;
    let cs_len = u16::from_be_bytes([ch[pos], ch[pos + 1]]) as usize;
    pos += 2 + cs_len;
    if pos >= ch.len() {
        return None;
    }
    let comp_len = ch[pos] as usize;
    pos += 1 + comp_len;
    if pos + 2 > ch.len() {
        return None;
    }
    let ext_total = u16::from_be_bytes([ch[pos], ch[pos + 1]]) as usize;
    pos += 2;
    if pos + ext_total > ch.len() {
        return None;
    }

    let mut sni = String::new();
    let mut client_eph_pub: Option<[u8; 32]> = None;

    let ext_end = pos + ext_total;
    while pos + 4 <= ext_end {
        let etype = u16::from_be_bytes([ch[pos], ch[pos + 1]]);
        let elen = u16::from_be_bytes([ch[pos + 2], ch[pos + 3]]) as usize;
        pos += 4;
        if pos + elen > ext_end {
            return None;
        }
        let eval = &ch[pos..pos + elen];
        if etype == 0x0000 && eval.len() >= 5 && eval[2] == 0x00 {
            let nlen = u16::from_be_bytes([eval[3], eval[4]]) as usize;
            if 5 + nlen <= eval.len() {
                if let Ok(s) = std::str::from_utf8(&eval[5..5 + nlen]) {
                    sni = s.to_string();
                }
            }
        } else if etype == 0x0033 && eval.len() >= 38 {
            // key_share: u16 list_len, u16 group(0x001d), u16 klen(32), [u8; 32]
            let mut kpos = 2usize;
            while kpos + 4 <= eval.len() {
                let grp = u16::from_be_bytes([eval[kpos], eval[kpos + 1]]);
                let klen = u16::from_be_bytes([eval[kpos + 2], eval[kpos + 3]]) as usize;
                kpos += 4;
                if kpos + klen > eval.len() {
                    break;
                }
                if grp == 0x001d && klen == 32 {
                    let mut pk = [0u8; 32];
                    pk.copy_from_slice(&eval[kpos..kpos + 32]);
                    client_eph_pub = Some(pk);
                    break;
                }
                kpos += klen;
            }
        }
        pos += elen;
    }

    let client_eph_pub = client_eph_pub?;
    let server_sk = StaticSecret::from(*server_priv);
    let client_eph_pk = PublicKey::from(client_eph_pub);
    let auth_shared = *server_sk.diffie_hellman(&client_eph_pk).as_bytes();

    for sid_candidate in allowed_short_ids {
        let auth_key = derive_auth_key(&auth_shared, sid_candidate);
        let mut ts_bytes = [0u8; 8];
        let mut unmasked_sid = [0u8; 8];
        for i in 0..8 {
            ts_bytes[i] = session_id[i] ^ auth_key[i];
            unmasked_sid[i] = session_id[8 + i] ^ auth_key[8 + i];
        }
        if unmasked_sid.ct_eq(sid_candidate).unwrap_u8() != 1 {
            continue;
        }
        let ts = u64::from_be_bytes(ts_bytes);
        if now_secs.abs_diff(ts) > 180 {
            continue;
        }
        let expected_tag = compute_client_auth_tag(&auth_key, &client_random, ts, &sni);
        if session_id[16..32].ct_eq(&expected_tag[..16]).unwrap_u8() == 1 {
            return Some(VerifiedRealityClient {
                sni,
                short_id: *sid_candidate,
                client_random,
                session_id,
                client_eph_pub,
                auth_shared,
            });
        }
    }

    None
}

/// Verifies an incoming ClientHello AND enforces one-time anti-replay using `RealityReplayCache`.
pub fn verify_reality_client_hello_with_replay(
    raw: &[u8],
    server_priv: &[u8; 32],
    allowed_short_ids: &[[u8; 8]],
    now_secs: u64,
    replay_cache: &mut RealityReplayCache,
) -> Option<VerifiedRealityClient> {
    let verified = verify_reality_client_hello(raw, server_priv, allowed_short_ids, now_secs)?;
    if !replay_cache.check_and_insert(verified.session_id, now_secs) {
        return None;
    }
    Some(verified)
}

pub fn build_reality_server_hello(verified: &VerifiedRealityClient) -> (Vec<u8>, RealityStreamCipher) {
    let mut rng = rand::thread_rng();
    let mut server_random = [0u8; 32];
    rng.fill_bytes(&mut server_random);

    let srv_eph_sec = EphemeralSecret::random_from_rng(rng);
    let srv_eph_pub = PublicKey::from(&srv_eph_sec);
    let cli_eph_pk = PublicKey::from(verified.client_eph_pub);
    let eph_shared = *srv_eph_sec.diffie_hellman(&cli_eph_pk).as_bytes();

    let (srv_to_cli, cli_to_srv) = derive_stream_keys(
        &verified.auth_shared,
        &eph_shared,
        &verified.client_random,
        &server_random,
    );

    let mut exts = Vec::new();
    // supported_versions: TLS 1.3 (0x0304)
    push_extension(&mut exts, 0x002b, &[0x03, 0x04]);
    // key_share: x25519 + 32 bytes
    let mut ks = Vec::with_capacity(36);
    ks.extend_from_slice(&0x001du16.to_be_bytes());
    ks.extend_from_slice(&32u16.to_be_bytes());
    ks.extend_from_slice(srv_eph_pub.as_bytes());
    push_extension(&mut exts, 0x0033, &ks);

    let mut sh = Vec::new();
    sh.extend_from_slice(&[0x03, 0x03]); // legacy_version
    sh.extend_from_slice(&server_random);
    sh.push(32); // RFC 8446 legacy_session_id_echo (32 bytes)
    sh.extend_from_slice(&verified.session_id);
    sh.extend_from_slice(&[0x13, 0x03]); // TLS_CHACHA20_POLY1305_SHA256
    sh.push(0x00); // compression
    sh.extend_from_slice(&(exts.len() as u16).to_be_bytes());
    sh.extend_from_slice(&exts);

    let mut hs = Vec::with_capacity(4 + sh.len());
    hs.push(0x02); // ServerHello
    let len = sh.len() as u32;
    hs.push(((len >> 16) & 0xff) as u8);
    hs.push(((len >> 8) & 0xff) as u8);
    hs.push((len & 0xff) as u8);
    hs.extend_from_slice(&sh);

    let mut out = Vec::with_capacity(5 + hs.len() + 6);
    out.extend_from_slice(&[0x16, 0x03, 0x03]);
    out.extend_from_slice(&(hs.len() as u16).to_be_bytes());
    out.extend_from_slice(&hs);
    // Optional dummy ChangeCipherSpec for middlebox compatibility
    out.extend_from_slice(&[0x14, 0x03, 0x03, 0x00, 0x01, 0x01]);

    // Server transmits with (`srv_to_cli`, dir=1), receives with (`cli_to_srv`, dir=2)
    (out, RealityStreamCipher::new(srv_to_cli, 1, cli_to_srv, 2))
}

pub fn client_finish_reality_handshake(
    hs: RealityClientHandshake,
    server_hello_record: &[u8],
) -> Result<RealityStreamCipher, String> {
    if server_hello_record.len() < 48 || server_hello_record[0] != 0x16 || server_hello_record[5] != 0x02 {
        return Err("Invalid REALITY ServerHello".to_string());
    }
    let rec_len = u16::from_be_bytes([server_hello_record[3], server_hello_record[4]]) as usize;
    if server_hello_record.len() < 5 + rec_len || rec_len < 42 {
        return Err("Truncated REALITY ServerHello".to_string());
    }
    let sh = &server_hello_record[9..5 + rec_len];
    let mut server_random = [0u8; 32];
    server_random.copy_from_slice(&sh[2..34]);
    let sid_len = sh[34] as usize;
    if 35 + sid_len + 3 > sh.len() {
        return Err("Invalid session_id_echo in ServerHello".to_string());
    }
    if sid_len != 32 || sh[35..67].ct_eq(&hs.session_id).unwrap_u8() != 1 {
        return Err("Mismatched legacy_session_id_echo in ServerHello".to_string());
    }
    let mut pos = 35 + sid_len + 3; // skip cipher(2) + comp(1)
    if pos + 2 > sh.len() {
        return Err("Missing extensions in ServerHello".to_string());
    }
    let ext_total = u16::from_be_bytes([sh[pos], sh[pos + 1]]) as usize;
    pos += 2;
    let ext_end = (pos + ext_total).min(sh.len());

    let mut srv_eph_pub: Option<[u8; 32]> = None;
    while pos + 4 <= ext_end {
        let etype = u16::from_be_bytes([sh[pos], sh[pos + 1]]);
        let elen = u16::from_be_bytes([sh[pos + 2], sh[pos + 3]]) as usize;
        pos += 4;
        if pos + elen > ext_end {
            break;
        }
        let eval = &sh[pos..pos + elen];
        if etype == 0x0033 && eval.len() >= 36 {
            let grp = u16::from_be_bytes([eval[0], eval[1]]);
            let klen = u16::from_be_bytes([eval[2], eval[3]]) as usize;
            if grp == 0x001d && klen == 32 {
                let mut pk = [0u8; 32];
                pk.copy_from_slice(&eval[4..36]);
                srv_eph_pub = Some(pk);
                break;
            }
        }
        pos += elen;
    }

    let srv_eph_pub = srv_eph_pub.ok_or_else(|| "Missing X25519 key_share in ServerHello".to_string())?;
    let srv_pk = PublicKey::from(srv_eph_pub);
    let eph_shared = *hs.eph_secret.diffie_hellman(&srv_pk).as_bytes();
    let (srv_to_cli, cli_to_srv) = derive_stream_keys(
        &hs.auth_shared,
        &eph_shared,
        &hs.client_random,
        &server_random,
    );

    // Client transmits with (`cli_to_srv`, dir=2), receives with (`srv_to_cli`, dir=1)
    Ok(RealityStreamCipher::new(cli_to_srv, 2, srv_to_cli, 1))
}

pub async fn verify_client_reality_pong<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    reader: &mut R,
    writer: &mut W,
    tx: &mut RealityTxCipher,
    rx: &mut RealityRxCipher,
) -> Result<(), String> {
    let mut challenge = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut challenge);
    write_reality_frame(writer, tx, REALITY_FRAME_PING, &challenge).await?;
    let (ft, pong) = read_reality_frame(reader, rx).await?;
    if ft != REALITY_FRAME_PING || pong.len() != challenge.len() || pong.ct_eq(&challenge).unwrap_u8() != 1 {
        return Err("REALITY server failed AEAD mutual challenge".to_string());
    }
    Ok(())
}

pub async fn read_tls_handshake_record<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Vec<u8>, String> {
    let mut hdr = [0u8; 5];
    reader
        .read_exact(&mut hdr)
        .await
        .map_err(|e| format!("TLS handshake hdr read error: {e}"))?;
    if hdr[0] != 0x16 {
        return Err(format!("Expected TLS Handshake record 0x16, got 0x{:02x}", hdr[0]));
    }
    let rec_len = u16::from_be_bytes([hdr[3], hdr[4]]) as usize;
    if rec_len > 16384 {
        return Err(format!("Oversized TLS handshake record: {rec_len}"));
    }
    let mut out = Vec::with_capacity(5 + rec_len);
    out.extend_from_slice(&hdr);
    out.resize(5 + rec_len, 0);
    reader
        .read_exact(&mut out[5..])
        .await
        .map_err(|e| format!("TLS handshake body read error: {e}"))?;
    Ok(out)
}

pub async fn write_reality_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    cipher: &mut RealityTxCipher,
    frame_type: u8,
    payload: &[u8],
) -> Result<(), String> {
    let rec = cipher.encrypt_record(frame_type, payload)?;
    writer
        .write_all(&rec)
        .await
        .map_err(|e| format!("REALITY TCP write error: {e}"))
}

pub async fn read_reality_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
    cipher: &mut RealityRxCipher,
) -> Result<(u8, Vec<u8>), String> {
    loop {
        let mut hdr = [0u8; 5];
        reader
            .read_exact(&mut hdr)
            .await
            .map_err(|e| format!("REALITY TCP read hdr error: {e}"))?;
        let rec_type = hdr[0];
        let rec_len = u16::from_be_bytes([hdr[3], hdr[4]]) as usize;
        if rec_len > 16384 + 256 {
            return Err(format!("Oversized TLS record: {rec_len}"));
        }
        let mut body = vec![0u8; rec_len];
        reader
            .read_exact(&mut body)
            .await
            .map_err(|e| format!("REALITY TCP read body error: {e}"))?;
        if rec_type == 0x14 {
            // Skip ChangeCipherSpec record
            continue;
        }
        if rec_type != 0x17 {
            return Err(format!("Unexpected TLS record type 0x{rec_type:02x}"));
        }
        return cipher.decrypt_record_payload(&body);
    }
}

fn push_extension(buf: &mut Vec<u8>, ext_type: u16, payload: &[u8]) {
    buf.extend_from_slice(&ext_type.to_be_bytes());
    buf.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    buf.extend_from_slice(payload);
}

fn derive_auth_key(auth_shared: &[u8; 32], short_id: &[u8; 8]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(Some(short_id), auth_shared);
    let mut key = [0u8; 32];
    hk.expand(b"Aegis-REALITY-Auth-v1", &mut key)
        .expect("HKDF auth_key");
    key
}

fn compute_client_auth_tag(
    auth_key: &[u8; 32],
    client_random: &[u8; 32],
    ts_secs: u64,
    sni: &str,
) -> [u8; 32] {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(auth_key).expect("HMAC key");
    mac.update(client_random);
    mac.update(&ts_secs.to_be_bytes());
    mac.update(sni.as_bytes());
    mac.finalize().into_bytes().into()
}

fn derive_stream_keys(
    auth_shared: &[u8; 32],
    eph_shared: &[u8; 32],
    client_random: &[u8; 32],
    server_random: &[u8; 32],
) -> ([u8; 32], [u8; 32]) {
    let mut ikm = [0u8; 64];
    ikm[..32].copy_from_slice(auth_shared);
    ikm[32..].copy_from_slice(eph_shared);

    let mut salt = [0u8; 64];
    salt[..32].copy_from_slice(client_random);
    salt[32..].copy_from_slice(server_random);

    let hk = Hkdf::<Sha256>::new(Some(&salt), &ikm);
    let mut s2c = [0u8; 32];
    let mut c2s = [0u8; 32];
    hk.expand(b"Aegis-REALITY-S2C-v1", &mut s2c).unwrap();
    hk.expand(b"Aegis-REALITY-C2S-v1", &mut c2s).unwrap();
    (s2c, c2s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reality_handshake_authenticates_valid_client_and_rejects_prober() {
        let server_priv = [0x21u8; 32];
        let server_sk = StaticSecret::from(server_priv);
        let server_pub = *PublicKey::from(&server_sk).as_bytes();
        let short_id = parse_short_id_8("aegis001aegis001").unwrap();
        let now = 1_794_835_000u64;

        let (ch_bytes, client_hs) =
            build_reality_client_hello(&server_pub, &short_id, "dl.google.com", now).unwrap();

        // Valid client passes verification
        let verified =
            verify_reality_client_hello(&ch_bytes, &server_priv, &[short_id], now + 5)
                .expect("valid reality client");
        assert_eq!(verified.sni, "dl.google.com");

        let (sh_bytes, mut srv_cipher) = build_reality_server_hello(&verified);
        let mut cli_cipher = client_finish_reality_handshake(client_hs, &sh_bytes).unwrap();

        let rec = cli_cipher
            .encrypt_record(REALITY_FRAME_WG, b"encrypted-wg-frame")
            .unwrap();
        // Record header starts with TLS 1.3 ApplicationData (0x17 0x03 0x03)
        assert_eq!(&rec[0..3], &[0x17, 0x03, 0x03]);
        let (ftype, plain) = srv_cipher.decrypt_record_payload(&rec[5..]).unwrap();
        assert_eq!(ftype, REALITY_FRAME_WG);
        assert_eq!(plain, b"encrypted-wg-frame");

        // Wrong short_id / active prober is rejected -> gateway routes to decoy site!
        let wrong_id = parse_short_id_8("deadbeefcafebabe").unwrap();
        assert!(
            verify_reality_client_hello(&ch_bytes, &server_priv, &[wrong_id], now).is_none()
        );

        // Stale replay (>180s) rejected
        assert!(
            verify_reality_client_hello(&ch_bytes, &server_priv, &[short_id], now + 500).is_none()
        );

        // One-time anti-replay cache allows first handshake and blocks captured replays
        let mut cache = RealityReplayCache::new();
        assert!(
            verify_reality_client_hello_with_replay(&ch_bytes, &server_priv, &[short_id], now + 10, &mut cache)
                .is_some()
        );
        assert!(
            verify_reality_client_hello_with_replay(&ch_bytes, &server_priv, &[short_id], now + 11, &mut cache)
                .is_none()
        );
    }
}
