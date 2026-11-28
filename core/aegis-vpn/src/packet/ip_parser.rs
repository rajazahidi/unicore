use std::net::{Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpVersion {
    V4,
    V6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportProto {
    Icmp,
    Tcp,
    Udp,
    IcmpV6,
    Other(u8),
}

#[derive(Debug)]
pub struct ParsedIpPacket<'a> {
    pub version: IpVersion,
    pub proto: TransportProto,
    pub header_len: usize,
    pub src_v4: Option<Ipv4Addr>,
    pub dst_v4: Option<Ipv4Addr>,
    pub src_v6: Option<Ipv6Addr>,
    pub dst_v6: Option<Ipv6Addr>,
    pub payload: &'a [u8],
}

#[derive(Debug, Clone, Copy)]
pub struct UdpView<'a> {
    pub src_port: u16,
    pub dst_port: u16,
    pub payload: &'a [u8],
}

pub fn parse_ip_packet(pkt: &[u8]) -> Option<ParsedIpPacket<'_>> {
    if pkt.is_empty() {
        return None;
    }
    match pkt[0] >> 4 {
        4 => {
            if pkt.len() < 20 {
                return None;
            }
            let ihl = ((pkt[0] & 0x0f) as usize) * 4;
            if ihl < 20 || pkt.len() < ihl {
                return None;
            }
            let total_len = u16::from_be_bytes([pkt[2], pkt[3]]) as usize;
            let end = total_len.min(pkt.len());
            if end < ihl {
                return None;
            }
            let proto = match pkt[9] {
                1 => TransportProto::Icmp,
                6 => TransportProto::Tcp,
                17 => TransportProto::Udp,
                other => TransportProto::Other(other),
            };
            let src = Ipv4Addr::new(pkt[12], pkt[13], pkt[14], pkt[15]);
            let dst = Ipv4Addr::new(pkt[16], pkt[17], pkt[18], pkt[19]);
            Some(ParsedIpPacket {
                version: IpVersion::V4,
                proto,
                header_len: ihl,
                src_v4: Some(src),
                dst_v4: Some(dst),
                src_v6: None,
                dst_v6: None,
                payload: &pkt[ihl..end],
            })
        }
        6 => {
            if pkt.len() < 40 {
                return None;
            }
            let payload_len = u16::from_be_bytes([pkt[4], pkt[5]]) as usize;
            let end = (40 + payload_len).min(pkt.len());
            let proto = match pkt[6] {
                6 => TransportProto::Tcp,
                17 => TransportProto::Udp,
                58 => TransportProto::IcmpV6,
                other => TransportProto::Other(other),
            };
            let mut src_bytes = [0u8; 16];
            let mut dst_bytes = [0u8; 16];
            src_bytes.copy_from_slice(&pkt[8..24]);
            dst_bytes.copy_from_slice(&pkt[24..40]);
            Some(ParsedIpPacket {
                version: IpVersion::V6,
                proto,
                header_len: 40,
                src_v4: None,
                dst_v4: None,
                src_v6: Some(Ipv6Addr::from(src_bytes)),
                dst_v6: Some(Ipv6Addr::from(dst_bytes)),
                payload: &pkt[40..end],
            })
        }
        _ => None,
    }
}

pub fn parse_udp(payload: &[u8]) -> Option<UdpView<'_>> {
    if payload.len() < 8 {
        return None;
    }
    let src_port = u16::from_be_bytes([payload[0], payload[1]]);
    let dst_port = u16::from_be_bytes([payload[2], payload[3]]);
    let udp_len = u16::from_be_bytes([payload[4], payload[5]]) as usize;
    if udp_len < 8 || udp_len > payload.len() {
        return Some(UdpView {
            src_port,
            dst_port,
            payload: &payload[8..],
        });
    }
    Some(UdpView {
        src_port,
        dst_port,
        payload: &payload[8..udp_len],
    })
}

fn sum_be16_words(data: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        sum += u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        i += 2;
    }
    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }
    sum
}

pub fn internet_checksum(data: &[u8]) -> u16 {
    let mut sum = sum_be16_words(data);
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

pub fn ipv4_udp_checksum(src: Ipv4Addr, dst: Ipv4Addr, udp_segment: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let s = src.octets();
    let d = dst.octets();
    sum += u16::from_be_bytes([s[0], s[1]]) as u32;
    sum += u16::from_be_bytes([s[2], s[3]]) as u32;
    sum += u16::from_be_bytes([d[0], d[1]]) as u32;
    sum += u16::from_be_bytes([d[2], d[3]]) as u32;
    sum += 17u32; // UDP protocol
    sum += udp_segment.len() as u32;
    sum += sum_be16_words(udp_segment);
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    let res = !(sum as u16);
    if res == 0 {
        0xffff
    } else {
        res
    }
}

pub fn build_ipv4_udp_packet(
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    src_port: u16,
    dst_port: u16,
    payload: &[u8],
) -> Vec<u8> {
    let udp_len = 8 + payload.len();
    let total_len = 20 + udp_len;
    let mut pkt = vec![0u8; total_len];

    // IPv4 header
    pkt[0] = 0x45; // v4, IHL=5 (20 bytes)
    pkt[1] = 0x00;
    pkt[2..4].copy_from_slice(&(total_len as u16).to_be_bytes());
    pkt[4..6].copy_from_slice(&0x1a2bu16.to_be_bytes());
    pkt[6..8].copy_from_slice(&0x4000u16.to_be_bytes()); // Don't Fragment
    pkt[8] = 64; // TTL
    pkt[9] = 17; // UDP
    pkt[12..16].copy_from_slice(&src_ip.octets());
    pkt[16..20].copy_from_slice(&dst_ip.octets());
    let ip_cksum = internet_checksum(&pkt[0..20]);
    pkt[10..12].copy_from_slice(&ip_cksum.to_be_bytes());

    // UDP header + payload
    pkt[20..22].copy_from_slice(&src_port.to_be_bytes());
    pkt[22..24].copy_from_slice(&dst_port.to_be_bytes());
    pkt[24..26].copy_from_slice(&(udp_len as u16).to_be_bytes());
    pkt[28..].copy_from_slice(payload);

    let udp_cksum = ipv4_udp_checksum(src_ip, dst_ip, &pkt[20..]);
    pkt[26..28].copy_from_slice(&udp_cksum.to_be_bytes());
    pkt
}

pub fn ipv6_udp_checksum(src: Ipv6Addr, dst: Ipv6Addr, udp_segment: &[u8]) -> u16 {
    let mut sum: u32 = sum_be16_words(&src.octets()) + sum_be16_words(&dst.octets());
    let len_bytes = (udp_segment.len() as u32).to_be_bytes();
    sum += u16::from_be_bytes([len_bytes[0], len_bytes[1]]) as u32;
    sum += u16::from_be_bytes([len_bytes[2], len_bytes[3]]) as u32;
    sum += 17u32;
    sum += sum_be16_words(udp_segment);
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    let res = !(sum as u16);
    if res == 0 {
        0xffff
    } else {
        res
    }
}

pub fn build_ipv6_udp_packet(
    src_ip: Ipv6Addr,
    dst_ip: Ipv6Addr,
    src_port: u16,
    dst_port: u16,
    payload: &[u8],
) -> Vec<u8> {
    let udp_len = 8 + payload.len();
    let total_len = 40 + udp_len;
    let mut pkt = vec![0u8; total_len];

    pkt[0] = 0x60;
    pkt[4..6].copy_from_slice(&(udp_len as u16).to_be_bytes());
    pkt[6] = 17; // UDP
    pkt[7] = 64; // Hop limit
    pkt[8..24].copy_from_slice(&src_ip.octets());
    pkt[24..40].copy_from_slice(&dst_ip.octets());

    pkt[40..42].copy_from_slice(&src_port.to_be_bytes());
    pkt[42..44].copy_from_slice(&dst_port.to_be_bytes());
    pkt[44..46].copy_from_slice(&(udp_len as u16).to_be_bytes());
    pkt[48..].copy_from_slice(payload);

    let udp_cksum = ipv6_udp_checksum(src_ip, dst_ip, &pkt[40..]);
    pkt[46..48].copy_from_slice(&udp_cksum.to_be_bytes());
    pkt
}

pub fn build_ipv4_icmp_echo_reply(request_pkt: &[u8]) -> Option<Vec<u8>> {
    let parsed = parse_ip_packet(request_pkt)?;
    if parsed.version != IpVersion::V4 || parsed.proto != TransportProto::Icmp {
        return None;
    }
    if parsed.payload.len() < 8 || parsed.payload[0] != 8 {
        return None; // Not ICMP Echo Request
    }
    let src_ip = parsed.dst_v4?;
    let dst_ip = parsed.src_v4?;

    let mut reply_payload = parsed.payload.to_vec();
    reply_payload[0] = 0; // Echo Reply
    reply_payload[2] = 0;
    reply_payload[3] = 0;
    let icmp_cksum = internet_checksum(&reply_payload);
    reply_payload[2..4].copy_from_slice(&icmp_cksum.to_be_bytes());

    let total_len = 20 + reply_payload.len();
    let mut pkt = vec![0u8; total_len];
    pkt[0] = 0x45;
    pkt[2..4].copy_from_slice(&(total_len as u16).to_be_bytes());
    pkt[6..8].copy_from_slice(&0x4000u16.to_be_bytes());
    pkt[8] = 64;
    pkt[9] = 1; // ICMP
    pkt[12..16].copy_from_slice(&src_ip.octets());
    pkt[16..20].copy_from_slice(&dst_ip.octets());
    let ip_cksum = internet_checksum(&pkt[0..20]);
    pkt[10..12].copy_from_slice(&ip_cksum.to_be_bytes());
    pkt[20..].copy_from_slice(&reply_payload);
    Some(pkt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_ipv4_udp_checksum() {
        let src = Ipv4Addr::new(10, 66, 66, 2);
        let dst = Ipv4Addr::new(10, 66, 66, 1);
        let pkt = build_ipv4_udp_packet(src, dst, 53, 44123, b"hello-dns");
        let parsed = parse_ip_packet(&pkt).expect("valid ipv4");
        assert_eq!(parsed.version, IpVersion::V4);
        assert_eq!(parsed.proto, TransportProto::Udp);
        assert_eq!(parsed.src_v4, Some(src));
        assert_eq!(parsed.dst_v4, Some(dst));
        assert_eq!(internet_checksum(&pkt[0..20]), 0);

        let udp = parse_udp(parsed.payload).expect("valid udp");
        assert_eq!(udp.src_port, 53);
        assert_eq!(udp.dst_port, 44123);
        assert_eq!(udp.payload, b"hello-dns");
    }

    #[test]
    fn builds_icmp_echo_reply() {
        let mut req = vec![0u8; 28];
        req[0] = 0x45;
        req[2..4].copy_from_slice(&28u16.to_be_bytes());
        req[8] = 64;
        req[9] = 1;
        req[12..16].copy_from_slice(&[10, 66, 66, 1]);
        req[16..20].copy_from_slice(&[10, 66, 66, 254]);
        let ip_c = internet_checksum(&req[0..20]);
        req[10..12].copy_from_slice(&ip_c.to_be_bytes());
        req[20] = 8; // Echo request
        req[24..28].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        let icmp_c = internet_checksum(&req[20..]);
        req[22..24].copy_from_slice(&icmp_c.to_be_bytes());

        let reply = build_ipv4_icmp_echo_reply(&req).expect("reply");
        let parsed = parse_ip_packet(&reply).unwrap();
        assert_eq!(parsed.src_v4, Some(Ipv4Addr::new(10, 66, 66, 254)));
        assert_eq!(parsed.dst_v4, Some(Ipv4Addr::new(10, 66, 66, 1)));
        assert_eq!(parsed.payload[0], 0);
        assert_eq!(internet_checksum(parsed.payload), 0);
    }
}
