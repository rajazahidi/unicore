use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone)]
pub struct DnsQuery {
    pub txid: u16,
    pub qname: String,
    pub qtype: u16,
    pub qclass: u16,
    pub question_end: usize,
    pub rd: bool,
}

#[derive(Debug, Default)]
struct TrieNode {
    terminal: bool,
    children: HashMap<String, TrieNode>,
}

#[derive(Debug, Default)]
struct DomainTrie {
    root: TrieNode,
}

impl DomainTrie {
    fn insert(&mut self, domain: &str) {
        let normalized = normalize_domain(domain);
        if normalized.is_empty() {
            return;
        }
        let mut node = &mut self.root;
        for label in normalized.rsplit('.') {
            node = node.children.entry(label.to_string()).or_default();
        }
        node.terminal = true;
    }

    fn matches_suffix(&self, domain: &str) -> bool {
        let normalized = normalize_domain(domain);
        if normalized.is_empty() {
            return false;
        }
        let mut node = &self.root;
        for label in normalized.rsplit('.') {
            match node.children.get(label) {
                Some(next) => {
                    if next.terminal {
                        return true;
                    }
                    node = next;
                }
                None => return false,
            }
        }
        false
    }
}

pub fn normalize_domain(domain: &str) -> String {
    domain.trim().trim_end_matches('.').to_ascii_lowercase()
}

#[derive(Debug)]
pub struct DnsSinkhole {
    enabled: bool,
    blocked: DomainTrie,
    allowed: DomainTrie,
    blocked_count: usize,
}

impl DnsSinkhole {
    pub fn new(enabled: bool, blocked_domains: &[String], allowed_domains: &[String]) -> Self {
        let mut blocked = DomainTrie::default();
        for d in blocked_domains {
            blocked.insert(d);
        }
        let mut allowed = DomainTrie::default();
        for d in allowed_domains {
            allowed.insert(d);
        }
        Self {
            enabled,
            blocked,
            allowed,
            blocked_count: blocked_domains.len(),
        }
    }

    pub fn rule_count(&self) -> usize {
        self.blocked_count
    }

    pub fn is_blocked(&self, domain: &str) -> bool {
        if !self.enabled {
            return false;
        }
        let clean = normalize_domain(domain);
        if clean.is_empty() {
            return false;
        }
        if self.allowed.matches_suffix(&clean) {
            return false;
        }
        self.blocked.matches_suffix(&clean)
    }
}

pub fn parse_dns_query(pkt: &[u8]) -> Option<DnsQuery> {
    if pkt.len() < 12 {
        return None;
    }
    let txid = u16::from_be_bytes([pkt[0], pkt[1]]);
    let flags = u16::from_be_bytes([pkt[2], pkt[3]]);
    // QR bit must be 0 (query)
    if (flags & 0x8000) != 0 {
        return None;
    }
    let qdcount = u16::from_be_bytes([pkt[4], pkt[5]]);
    if qdcount == 0 {
        return None;
    }

    let mut pos = 12usize;
    let mut labels = Vec::new();
    let mut steps = 0usize;
    loop {
        if pos >= pkt.len() || steps > 128 {
            return None;
        }
        steps += 1;
        let len = pkt[pos] as usize;
        pos += 1;
        if len == 0 {
            break;
        }
        // Reject compressed labels in question header for safety
        if (len & 0xc0) != 0 {
            return None;
        }
        if pos + len > pkt.len() {
            return None;
        }
        let label = std::str::from_utf8(&pkt[pos..pos + len]).ok()?;
        labels.push(label.to_ascii_lowercase());
        pos += len;
    }

    if pos + 4 > pkt.len() || labels.is_empty() {
        return None;
    }
    let qtype = u16::from_be_bytes([pkt[pos], pkt[pos + 1]]);
    let qclass = u16::from_be_bytes([pkt[pos + 2], pkt[pos + 3]]);
    let question_end = pos + 4;

    Some(DnsQuery {
        txid,
        qname: labels.join("."),
        qtype,
        qclass,
        question_end,
        rd: (flags & 0x0100) != 0,
    })
}

/// Builds a valid RFC-1035 DNS answer returning 0.0.0.0 (A), :: (AAAA), or NXDOMAIN for other qtypes.
pub fn build_sinkhole_response(raw_query: &[u8], q: &DnsQuery) -> Vec<u8> {
    let question_slice = &raw_query[12..q.question_end];
    let mut resp = Vec::with_capacity(12 + question_slice.len() + 32);

    resp.extend_from_slice(&q.txid.to_be_bytes());
    let rd_bit = if q.rd { 0x0100u16 } else { 0 };
    let (rcode, ancount) = match q.qtype {
        1 | 28 => (0u16, 1u16), // NoError with 0.0.0.0 or ::
        _ => (3u16, 0u16),      // NXDOMAIN
    };
    let flags = 0x8080u16 | rd_bit | rcode; // QR=1, RA=1
    resp.extend_from_slice(&flags.to_be_bytes());
    resp.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT = 1
    resp.extend_from_slice(&ancount.to_be_bytes());
    resp.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
    resp.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT
    resp.extend_from_slice(question_slice);

    if ancount == 1 {
        // Pointer to QNAME at offset 12 (0xC00C)
        resp.extend_from_slice(&0xC00Cu16.to_be_bytes());
        resp.extend_from_slice(&q.qtype.to_be_bytes());
        resp.extend_from_slice(&q.qclass.to_be_bytes());
        resp.extend_from_slice(&60u32.to_be_bytes()); // TTL 60s
        if q.qtype == 1 {
            resp.extend_from_slice(&4u16.to_be_bytes());
            resp.extend_from_slice(&Ipv4Addr::UNSPECIFIED.octets());
        } else {
            resp.extend_from_slice(&16u16.to_be_bytes());
            resp.extend_from_slice(&Ipv6Addr::UNSPECIFIED.octets());
        }
    }
    resp
}

/// Builds a synthetic A/AAAA response for lab/loopback testing (e.g. *.aegis.test -> 10.66.66.254).
pub fn build_synthetic_a_response(raw_query: &[u8], q: &DnsQuery, ipv4: Ipv4Addr) -> Vec<u8> {
    let question_slice = &raw_query[12..q.question_end];
    let mut resp = Vec::with_capacity(12 + question_slice.len() + 20);
    resp.extend_from_slice(&q.txid.to_be_bytes());
    let rd_bit = if q.rd { 0x0100u16 } else { 0 };
    let ancount = if q.qtype == 1 { 1u16 } else { 0u16 };
    let flags = 0x8080u16 | rd_bit;
    resp.extend_from_slice(&flags.to_be_bytes());
    resp.extend_from_slice(&1u16.to_be_bytes());
    resp.extend_from_slice(&ancount.to_be_bytes());
    resp.extend_from_slice(&0u16.to_be_bytes());
    resp.extend_from_slice(&0u16.to_be_bytes());
    resp.extend_from_slice(question_slice);
    if ancount == 1 {
        resp.extend_from_slice(&0xC00Cu16.to_be_bytes());
        resp.extend_from_slice(&1u16.to_be_bytes());
        resp.extend_from_slice(&q.qclass.to_be_bytes());
        resp.extend_from_slice(&30u32.to_be_bytes());
        resp.extend_from_slice(&4u16.to_be_bytes());
        resp.extend_from_slice(&ipv4.octets());
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_dns_query(domain: &str, qtype: u16) -> Vec<u8> {
        let mut pkt = vec![
            0x12, 0x34, // txid
            0x01, 0x00, // standard query, RD=1
            0x00, 0x01, // qdcount=1
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        for label in domain.split('.') {
            pkt.push(label.len() as u8);
            pkt.extend_from_slice(label.as_bytes());
        }
        pkt.push(0);
        pkt.extend_from_slice(&qtype.to_be_bytes());
        pkt.extend_from_slice(&1u16.to_be_bytes()); // IN
        pkt
    }

    #[test]
    fn sinkhole_matches_subdomains_and_respects_allowlist() {
        let sinkhole = DnsSinkhole::new(
            true,
            &[
                "doubleclick.net".to_string(),
                "tracker.example.com".to_string(),
            ],
            &["safe.doubleclick.net".to_string()],
        );

        assert!(sinkhole.is_blocked("doubleclick.net"));
        assert!(sinkhole.is_blocked("ad.doubleclick.net"));
        assert!(sinkhole.is_blocked("SUB.TRACKER.EXAMPLE.COM."));
        assert!(!sinkhole.is_blocked("safe.doubleclick.net"));
        assert!(!sinkhole.is_blocked("api.safe.doubleclick.net"));
        assert!(!sinkhole.is_blocked("wikipedia.org"));
    }

    #[test]
    fn builds_valid_zero_ip_response() {
        let raw = sample_dns_query("ads.doubleclick.net", 1);
        let q = parse_dns_query(&raw).expect("parsed");
        assert_eq!(q.txid, 0x1234);
        assert_eq!(q.qname, "ads.doubleclick.net");
        assert_eq!(q.qtype, 1);

        let resp = build_sinkhole_response(&raw, &q);
        assert_eq!(&resp[0..2], &[0x12, 0x34]);
        assert_eq!(resp[2] & 0x80, 0x80); // QR=1
        assert_eq!(u16::from_be_bytes([resp[6], resp[7]]), 1); // ANCOUNT=1
        assert_eq!(&resp[resp.len() - 4..], &[0, 0, 0, 0]);
    }
}
