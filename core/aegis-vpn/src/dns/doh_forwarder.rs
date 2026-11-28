use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;

use crate::dns::sinkhole::{build_synthetic_a_response, DnsQuery};

/// Resolves an allowed DNS query inside Loopback Lab mode:
/// - `*.aegis.test` resolves locally to the loopback echo IP `10.66.66.254`
/// - Other queries attempt a protected UDP forward to `upstream_dns:53` with short timeout,
///   falling back cleanly if the device/emulator is offline.
pub fn resolve_loopback_dns(
    raw_query: &[u8],
    query: &DnsQuery,
    upstream_dns: Ipv4Addr,
    echo_ip: Ipv4Addr,
    protect_cb: Option<&dyn Fn(i32) -> bool>,
) -> Option<Vec<u8>> {
    if query.qname == "aegis.test" || query.qname.ends_with(".aegis.test") {
        return Some(build_synthetic_a_response(raw_query, query, echo_ip));
    }

    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    let _ = sock.set_read_timeout(Some(Duration::from_millis(900)));
    let _ = sock.set_write_timeout(Some(Duration::from_millis(500)));

    #[cfg(unix)]
    if let Some(cb) = protect_cb {
        use std::os::fd::AsRawFd;
        if !cb(sock.as_raw_fd()) {
            return None;
        }
    }
    #[cfg(not(unix))]
    let _ = protect_cb;

    let target = SocketAddr::from((upstream_dns, 53));
    if sock.send_to(raw_query, target).is_err() {
        return None;
    }
    let mut buf = vec![0u8; 1500];
    let (n, _) = sock.recv_from(&mut buf).ok()?;
    buf.truncate(n);
    Some(buf)
}
