//! Gateway port prober. Ported from router_bot.py (login path omitted — unused by UI).

use std::collections::BTreeMap;
use std::net::TcpStream;
use std::time::Duration;

use super::cmd;

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
}
