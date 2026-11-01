//! Gateway port prober. Ported from router_bot.py (login path omitted — unused by UI).

use std::collections::BTreeMap;
use std::net::TcpStream;
use std::time::Duration;

use super::cmd;

#[derive(Clone)]
pub struct RouterStatus {
    pub ip: String,
    pub ports: BTreeMap<String, bool>,
    pub summary: String,
}

pub struct RouterBot {
    pub router_ip: String,
}

impl RouterBot {
    pub fn new(router_ip: &str) -> Self {
        RouterBot {
            router_ip: router_ip.to_string(),
        }
    }

    fn resolve_ip(&self) -> String {
        if !self.router_ip.is_empty() && self.router_ip != "N/A" {
            return self.router_ip.clone();
        }
        let out = cmd::run(
            &["route", "-n", "get", "default"],
            Duration::from_millis(1500),
        );
        out.stdout
            .lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix("gateway:")
                    .map(|s| s.trim().to_string())
            })
            .unwrap_or_else(|| {
                if self.router_ip.is_empty() {
                    "N/A".into()
                } else {
                    self.router_ip.clone()
                }
            })
    }

    pub fn probe_ports(&self) -> RouterStatus {
        let ip = self.resolve_ip();
        let mut ports = BTreeMap::new();
        if ip == "N/A" {
            ports.insert("53".into(), false);
            ports.insert("80".into(), false);
            ports.insert("443".into(), false);
            return RouterStatus {
                ip,
                ports,
                summary: "No Active Gateway".into(),
            };
        }
        for port in [53u16, 80, 443] {
            let open = TcpStream::connect_timeout(
                &format!("{ip}:{port}").parse().unwrap(),
                Duration::from_millis(400),
            )
            .is_ok();
            ports.insert(port.to_string(), open);
        }
        let summary = if ports["443"] {
            "HTTPS (443) ONLINE — Gateway Web UI Ready"
        } else if ports["80"] {
            "HTTP (80) ONLINE, HTTPS (443) Closed"
        } else if ports["53"] {
            "DNS Relay (53) ONLINE (Hotspot / Gateway)"
        } else {
            "Management Ports Unreachable"
        };
        RouterStatus {
            ip,
            ports,
            summary: summary.into(),
        }
    }

    /// Attempt a gateway admin login (base64 password, generic endpoint).
    /// Ported from router_bot.attempt_login.
    pub fn attempt_login(&self, username: &str, password: &str) -> String {
        use std::time::Duration;
        let ip = self.resolve_ip();
        if ip == "N/A" {
            return "No active gateway to log into.".into();
        }
        let b64 = base64_encode(password.as_bytes());
        let data = format!("userid={username}&password={b64}");
        let url = format!("https://{ip}/activatesession.cmd?action=logon");
        let out = super::cmd::run(
            &[
                "curl", "-k", "-s", "-m", "3", "-X", "POST", "-d", &data, &url,
            ],
            Duration::from_secs(4),
        );
        if out.stdout.is_empty() {
            format!("Gateway {ip}:443 refused connection.")
        } else if out.stdout.contains("Invalid User") || out.stdout.contains("Bad.html") {
            format!("Gateway rejected credentials for '{username}'.")
        } else {
            format!("Authenticated to {ip} as '{username}'!")
        }
    }
}

/// Minimal RFC 4648 base64 (std has no encoder; avoids a dependency).
fn base64_encode(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::base64_encode;

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"hunter2"), "aHVudGVyMg==");
    }
}
