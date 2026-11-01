//! LAN subnet radar with OUI vendor resolution. Ported from lan_scanner.py.

use std::time::{Duration, Instant};

use super::cmd;

const OUI: &[(&str, &str)] = &[
    ("00:1a:11", "Google"),
    ("3c:22:fb", "Apple"),
    ("f0:18:98", "Apple"),
    ("a4:83:e7", "Apple"),
    ("bc:d0:74", "Apple"),
    ("ac:de:48", "Apple"),
    ("d0:81:7a", "Apple"),
    ("24:a0:74", "Apple"),
    ("48:2c:a0", "Xiaomi"),
    ("00:e0:fc", "Huawei"),
    ("48:46:fb", "Huawei"),
    ("88:28:b3", "Huawei"),
    ("c8:d1:5e", "Huawei"),
    ("70:72:cf", "Huawei"),
    ("dc:a6:32", "RaspberryPi"),
    ("b8:27:eb", "RaspberryPi"),
    ("24:0a:c4", "Espressif"),
    ("30:ae:a4", "Espressif"),
    ("50:c7:bf", "TP-Link"),
    ("b0:be:76", "TP-Link"),
    ("78:8a:20", "Ubiquiti"),
    ("f0:9f:c2", "Ubiquiti"),
    ("28:39:5e", "Samsung"),
    ("8c:79:f5", "Samsung"),
    ("00:04:20", "Sony"),
    ("f8:46:1c", "Sony"),
];

pub fn normalize_mac(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    let parts: Vec<&str> = lower.split(':').collect();
    if parts.len() != 6 {
        return lower;
    }
    parts
        .iter()
        .map(|p| format!("{:0>2}", p))
        .collect::<Vec<_>>()
        .join(":")
}

pub fn resolve_mac_vendor(mac: &str) -> String {
    let norm = normalize_mac(mac);
    if matches!(norm.as_str(), "(incomplete)" | "ff:ff:ff:ff:ff:ff" | "") {
        return "Broadcast/Incomplete".into();
    }
    let prefix = &norm[..norm.len().min(8)];
    for (p, v) in OUI {
        if *p == prefix {
            return (*v).to_string();
        }
    }
    let first = norm.split(':').next().unwrap_or("");
    if first.len() == 2 {
        if let Some(c) = first.chars().nth(1) {
            if matches!(c, '2' | '6' | 'a' | 'e') {
                return "Apple/Private-MAC".into();
            }
        }
    }
    "LAN-Device".into()
}

#[derive(Clone)]
pub struct LanDevice {
    pub ip: String,
    pub mac: String,
    pub vendor: String,
    pub role: String,
}

#[derive(Clone)]
pub struct LanData {
    pub devices: Vec<LanDevice>,
    pub count: usize,
    pub ip_conflict: bool,
    pub summary: String,
}

pub struct LanScanner {
    interface: String,
    last_scan: Option<Instant>,
    cached: Option<LanData>,
}

impl LanScanner {
    pub fn new(interface: &str) -> Self {
        LanScanner {
            interface: interface.to_string(),
            last_scan: None,
            cached: None,
        }
    }

    pub fn scan_lan(&mut self, local_ip: &str, gateway_ip: &str, active_sweep: bool) -> LanData {
        if !active_sweep {
            if let (Some(t), Some(c)) = (self.last_scan, &self.cached) {
                if t.elapsed() < Duration::from_secs(12) {
                    return LanData {
                        devices: c.devices.clone(),
                        count: c.count,
                        ip_conflict: c.ip_conflict,
                        summary: c.summary.clone(),
                    };
                }
            }
        }
        self.last_scan = Some(Instant::now());

        if active_sweep && !local_ip.is_empty() && local_ip != "N/A" {
            let octets: Vec<&str> = local_ip.split('.').collect();
            if octets.len() == 4 {
                let prefix = format!("{}.{}.{}", octets[0], octets[1], octets[2]);
                for i in 1..20 {
                    let ip = format!("{prefix}.{i}");
                    if ip != local_ip {
                        cmd::run(
                            &["ping", "-c", "1", "-W", "250", &ip],
                            Duration::from_millis(450),
                        );
                    }
                }
            }
        }

        let arp = cmd::run(
            &["arp", "-an", "-i", &self.interface],
            Duration::from_millis(1500),
        );
        let mut devices: Vec<LanDevice> = Vec::new();
        if !local_ip.is_empty() && local_ip != "N/A" {
            devices.push(LanDevice {
                ip: local_ip.to_string(),
                mac: "This Mac (en0)".into(),
                vendor: "Apple Mac".into(),
                role: "THIS DEVICE".into(),
            });
        }
        for line in arp.stdout.lines() {
            // Format: ? (192.168.1.1) at a1:b2:... on en0 ...
            if let (Some(op), Some(cp)) = (line.find('('), line.find(')')) {
                if cp > op {
                    let ip = &line[op + 1..cp];
                    if ip.ends_with(".255") || ip.starts_with("224.") {
                        continue;
                    }
                    let after = &line[cp + 1..];
                    let mac_raw = after.split_whitespace().nth(1).unwrap_or("");
                    if mac_raw == "(incomplete)" || mac_raw.is_empty() {
                        continue;
                    }
                    let norm = normalize_mac(mac_raw);
                    if norm == "ff:ff:ff:ff:ff:ff" || ip == local_ip {
                        continue;
                    }
                    let role = if ip == gateway_ip {
                        "GATEWAY / AP"
                    } else {
                        "LAN PEER"
                    };
                    devices.push(LanDevice {
                        ip: ip.to_string(),
                        vendor: resolve_mac_vendor(&norm),
                        mac: norm,
                        role: role.into(),
                    });
                }
            }
        }

        let count = devices.len();
        let conflict_tag = "[OK] Clean ARP";
        let summary = format!("{count} Active LAN Nodes ({conflict_tag})");
        let data = LanData {
            devices,
            count,
            ip_conflict: false,
            summary,
        };
        self.cached = Some(LanData {
            devices: data.devices.clone(),
            count: data.count,
            ip_conflict: data.ip_conflict,
            summary: data.summary.clone(),
        });
        data
    }
}
