use hkdf::Hkdf;
use ml_kem::{
    kem::{Decapsulate, Encapsulate, Kem, KeyExport, TryKeyInit},
    Ciphertext, DecapsulationKey, EncapsulationKey, MlKem768,
};
use sha2::Sha256;
use x25519_dalek::{EphemeralSecret, PublicKey};

pub const PQ_MAGIC: &[u8; 4] = b"AQPK";
pub const PQ_MSG_INIT: u8 = 1;
pub const PQ_MSG_RESP: u8 = 2;
pub const PQ_MSG_COMMIT: u8 = 3;
pub const PQ_MSG_ACK: u8 = 4;

#[derive(Debug, Clone)]
pub struct PqInitMessage {
    pub epoch: u64,
    pub x25519_eph_pub: [u8; 32],
    pub mlkem_ek: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct PqRespMessage {
    pub epoch: u64,
    pub x25519_eph_pub: [u8; 32],
    pub mlkem_ct: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct PqCommitMessage {
    pub epoch: u64,
}

#[derive(Debug)]
pub enum PqControlPacket {
    Init(PqInitMessage),
    Resp(PqRespMessage),
    Commit(PqCommitMessage),
    Ack(PqCommitMessage),
}

impl PqControlPacket {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            PqControlPacket::Init(m) => {
                let mut out = Vec::with_capacity(4 + 1 + 8 + 32 + 2 + m.mlkem_ek.len());
                out.extend_from_slice(PQ_MAGIC);
                out.push(PQ_MSG_INIT);
                out.extend_from_slice(&m.epoch.to_be_bytes());
                out.extend_from_slice(&m.x25519_eph_pub);
                out.extend_from_slice(&(m.mlkem_ek.len() as u16).to_be_bytes());
                out.extend_from_slice(&m.mlkem_ek);
                out
            }
            PqControlPacket::Resp(m) => {
                let mut out = Vec::with_capacity(4 + 1 + 8 + 32 + 2 + m.mlkem_ct.len());
                out.extend_from_slice(PQ_MAGIC);
                out.push(PQ_MSG_RESP);
                out.extend_from_slice(&m.epoch.to_be_bytes());
                out.extend_from_slice(&m.x25519_eph_pub);
                out.extend_from_slice(&(m.mlkem_ct.len() as u16).to_be_bytes());
                out.extend_from_slice(&m.mlkem_ct);
                out
            }
            PqControlPacket::Commit(m) => {
                let mut out = Vec::with_capacity(4 + 1 + 8);
                out.extend_from_slice(PQ_MAGIC);
                out.push(PQ_MSG_COMMIT);
                out.extend_from_slice(&m.epoch.to_be_bytes());
                out
            }
            PqControlPacket::Ack(m) => {
                let mut out = Vec::with_capacity(4 + 1 + 8);
                out.extend_from_slice(PQ_MAGIC);
                out.push(PQ_MSG_ACK);
                out.extend_from_slice(&m.epoch.to_be_bytes());
                out
            }
        }
    }

    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 13 || &data[0..4] != PQ_MAGIC {
            return None;
        }
        let msg_type = data[4];
        let epoch = u64::from_be_bytes(data[5..13].try_into().ok()?);
        match msg_type {
            PQ_MSG_INIT => {
                if data.len() < 47 {
                    return None;
                }
                let mut x25519_eph_pub = [0u8; 32];
                x25519_eph_pub.copy_from_slice(&data[13..45]);
                let len = u16::from_be_bytes([data[45], data[46]]) as usize;
                if data.len() < 47 + len {
                    return None;
                }
                Some(PqControlPacket::Init(PqInitMessage {
                    epoch,
                    x25519_eph_pub,
                    mlkem_ek: data[47..47 + len].to_vec(),
                }))
            }
            PQ_MSG_RESP => {
                if data.len() < 47 {
                    return None;
                }
                let mut x25519_eph_pub = [0u8; 32];
                x25519_eph_pub.copy_from_slice(&data[13..45]);
                let len = u16::from_be_bytes([data[45], data[46]]) as usize;
                if data.len() < 47 + len {
                    return None;
                }
                Some(PqControlPacket::Resp(PqRespMessage {
                    epoch,
                    x25519_eph_pub,
                    mlkem_ct: data[47..47 + len].to_vec(),
                }))
            }
            PQ_MSG_COMMIT => Some(PqControlPacket::Commit(PqCommitMessage { epoch })),
            PQ_MSG_ACK => Some(PqControlPacket::Ack(PqCommitMessage { epoch })),
            _ => None,
        }
    }
}

pub struct ClientPqExchange {
    pub epoch: u64,
    x25519_secret: EphemeralSecret,
    mlkem_dk: DecapsulationKey<MlKem768>,
}

impl ClientPqExchange {
    pub fn begin(epoch: u64) -> (Self, PqInitMessage) {
        let x25519_secret = EphemeralSecret::random_from_rng(rand::thread_rng());
        let x25519_pub = PublicKey::from(&x25519_secret);
        let (mlkem_dk, mlkem_ek) = MlKem768::generate_keypair();
        let ek_bytes = mlkem_ek.to_bytes().as_slice().to_vec();

        let msg = PqInitMessage {
            epoch,
            x25519_eph_pub: *x25519_pub.as_bytes(),
            mlkem_ek: ek_bytes,
        };
        let state = Self {
            epoch,
            x25519_secret,
            mlkem_dk,
        };
        (state, msg)
    }

    pub fn finish(
        self,
        resp: &PqRespMessage,
        base_psk: Option<[u8; 32]>,
    ) -> Result<[u8; 32], String> {
        if resp.epoch != self.epoch {
            return Err(format!(
                "PQ epoch mismatch: expected {}, got {}",
                self.epoch, resp.epoch
            ));
        }
        let server_x_pub = PublicKey::from(resp.x25519_eph_pub);
        let x_ss = self.x25519_secret.diffie_hellman(&server_x_pub);

        let ct = Ciphertext::<MlKem768>::try_from(resp.mlkem_ct.as_slice())
            .map_err(|_| "Invalid ML-KEM-768 ciphertext length".to_string())?;
        let mlkem_ss = self.mlkem_dk.decapsulate(&ct);

        Ok(derive_hybrid_psk(
            self.epoch,
            x_ss.as_bytes(),
            mlkem_ss.as_slice(),
            base_psk,
        ))
    }
}

pub fn server_respond_pq(
    init: &PqInitMessage,
    base_psk: Option<[u8; 32]>,
) -> Result<(PqRespMessage, [u8; 32]), String> {
    let ek = EncapsulationKey::<MlKem768>::new_from_slice(&init.mlkem_ek)
        .map_err(|_| "Invalid ML-KEM-768 encapsulation key".to_string())?;
    let (ct, mlkem_ss) = ek.encapsulate();

    let server_x_sec = EphemeralSecret::random_from_rng(rand::thread_rng());
    let server_x_pub = PublicKey::from(&server_x_sec);
    let client_x_pub = PublicKey::from(init.x25519_eph_pub);
    let x_ss = server_x_sec.diffie_hellman(&client_x_pub);

    let psk = derive_hybrid_psk(
        init.epoch,
        x_ss.as_bytes(),
        mlkem_ss.as_slice(),
        base_psk,
    );

    let resp = PqRespMessage {
        epoch: init.epoch,
        x25519_eph_pub: *server_x_pub.as_bytes(),
        mlkem_ct: ct.as_slice().to_vec(),
    };
    Ok((resp, psk))
}

fn derive_hybrid_psk(
    epoch: u64,
    x25519_shared: &[u8; 32],
    mlkem_shared: &[u8],
    base_psk: Option<[u8; 32]>,
) -> [u8; 32] {
    let mut ikm = Vec::with_capacity(96);
    ikm.extend_from_slice(x25519_shared);
    ikm.extend_from_slice(mlkem_shared);
    ikm.extend_from_slice(&base_psk.unwrap_or([0u8; 32]));

    let salt = epoch.to_be_bytes();
    let hk = Hkdf::<Sha256>::new(Some(&salt), &ikm);
    let mut out = [0u8; 32];
    hk.expand(b"AegisTunnel-MLKEM768-X25519-PSK-v1", &mut out)
        .expect("32-byte HKDF expand");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hybrid_x25519_mlkem768_derives_identical_psk_and_fits_mtu() {
        let (client_state, init_msg) = ClientPqExchange::begin(1);
        let init_wire = PqControlPacket::Init(init_msg.clone()).encode();
        // Verify ML-KEM-768 Init fits within a single 1280-MTU IPv4/UDP packet (<= 1252 payload bytes)
        assert!(init_wire.len() <= 1252, "init_wire len = {}", init_wire.len());

        let decoded_init = match PqControlPacket::decode(&init_wire).unwrap() {
            PqControlPacket::Init(m) => m,
            _ => panic!("expected Init"),
        };

        let base_psk = Some([0x42u8; 32]);
        let (resp_msg, server_psk) = server_respond_pq(&decoded_init, base_psk).unwrap();
        let resp_wire = PqControlPacket::Resp(resp_msg).encode();
        assert!(resp_wire.len() <= 1252, "resp_wire len = {}", resp_wire.len());

        let decoded_resp = match PqControlPacket::decode(&resp_wire).unwrap() {
            PqControlPacket::Resp(m) => m,
            _ => panic!("expected Resp"),
        };

        let client_psk = client_state.finish(&decoded_resp, base_psk).unwrap();
        assert_eq!(client_psk, server_psk);
        assert_ne!(client_psk, [0u8; 32]);

        let ack_wire = PqControlPacket::Ack(PqCommitMessage { epoch: 1 }).encode();
        assert!(matches!(PqControlPacket::decode(&ack_wire), Some(PqControlPacket::Ack(m)) if m.epoch == 1));
    }
}
