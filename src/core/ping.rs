//! Unprivileged ICMP echo over `SOCK_DGRAM` + `IPPROTO_ICMP`.
//!
//! macOS grants ICMP datagram sockets to unprivileged users, so this needs no
//! root and spawns no `ping` subprocess (the Python original shelled out to
//! `/sbin/ping` on every hop, every 1.5s).

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, Socket, Type};

const COOKIE: [u8; 16] = *b"wifi-tui-cockpit";

fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let (pairs, remainder) = data.as_chunks::<2>();
    for c in pairs {
        sum += u32::from(u16::from_be_bytes([c[0], c[1]]));
    }
    if let [last] = remainder {
        sum += u32::from(*last) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// Send one ICMP echo to `dst` and wait up to `timeout` for the matching reply.
/// Returns the round-trip time, or `None` on timeout / error (treated as loss).
pub fn ping_once(dst: Ipv4Addr, seq: u16, timeout: Duration) -> Option<Duration> {
    icmp_roundtrip(dst, seq, timeout).ok()
}

fn icmp_roundtrip(dst: Ipv4Addr, seq: u16, timeout: Duration) -> io::Result<Duration> {
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::ICMPV4))?;
    sock.set_read_timeout(Some(timeout))?;
    let udp: UdpSocket = sock.into();

    let ident = (std::process::id() & 0xffff) as u16;
    let mut pkt = [0u8; 24];
    pkt[0] = 8; // ICMP echo request
    pkt[4..6].copy_from_slice(&ident.to_be_bytes());
    pkt[6..8].copy_from_slice(&seq.to_be_bytes());
    pkt[8..24].copy_from_slice(&COOKIE);
    let ck = checksum(&pkt);
    pkt[2..4].copy_from_slice(&ck.to_be_bytes());

    let start = Instant::now();
    udp.send_to(&pkt, SocketAddr::new(IpAddr::V4(dst), 0))?;

    let mut buf = [0u8; 1500];
    loop {
        let (n, _) = udp.recv_from(&mut buf)?;
        // The kernel may or may not include the IP header; handle both.
        let off = if n > 0 && buf[0] >> 4 == 4 {
            usize::from(buf[0] & 0x0f) * 4
        } else {
            0
        };
        if n >= off + 24 {
            let icmp = &buf[off..n];
            let is_echo_reply = icmp[0] == 0;
            let seq_ok = u16::from_be_bytes([icmp[6], icmp[7]]) == seq;
            let cookie_ok = icmp[8..24] == COOKIE;
            if is_echo_reply && seq_ok && cookie_ok {
                return Ok(start.elapsed());
            }
        }
        if start.elapsed() > timeout {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "icmp timeout"));
        }
    }
}
