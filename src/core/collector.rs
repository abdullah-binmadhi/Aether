//! Assembles a full `Telemetry` snapshot from CoreWLAN (RF) + netdev (interface).
//!
//! SSID handling: macOS 14+ masks `ssid()` unless the process has Location
//! permission. When masked, we fall back to the first saved preferred network
//! (macOS keeps the connected SSID at index 0), exactly like the Python original,
//! and flag `ssid_available = false` so the UI can show a hint.

use std::process::Command;

use super::corewlan::{self, phy_mode_label, security_label};
use super::health::compute_health;
use super::model::{Band, HealthRating, Telemetry};

const HOTSPOT_KEYWORDS: &[&str] = &[
    "iphone", "ipad", "galaxy", "pixel", "android", "hotspot", "oneplus", "xiaomi",
];

pub struct Collector {
    pub interface: String,
    saved_networks: Vec<String>,
}

impl Collector {
    pub fn new(interface: &str) -> Self {
        Collector {
            interface: interface.to_string(),
            saved_networks: discover_saved_networks(interface),
        }
    }

    /// Full snapshot. `ping_ms`/`loss_pct` come from the hop matrix so health
    /// reflects live latency.
    pub fn collect(&mut self, ping_ms: f64, loss_pct: f64) -> Telemetry {
        let mut tel = Telemetry {
            interface: self.interface.clone(),
            ..Telemetry::default()
        };

        // --- RF via CoreWLAN ---
        let rf = corewlan::read_rf();
        if let Some(rf) = &rf {
            tel.radio_on = rf.power_on;
            tel.signal_dbm = rf.rssi_dbm;
            tel.noise_dbm = rf.noise_dbm;
            tel.snr_db = (rf.rssi_dbm - rf.noise_dbm).max(0);
            tel.tx_rate_mbps = rf.tx_rate_mbps;
            tel.transmit_power_mw = rf.transmit_power_mw;
            tel.phy_mode = phy_mode_label(rf.phy_mode_raw).to_string();
            tel.security = security_label(rf.security_raw).to_string();
            tel.channel_num = rf.channel_num;
            tel.band = rf.channel_band;
            tel.width_mhz = rf.channel_width_mhz;
            tel.channel_label = if rf.channel_num > 0 {
                format!(
                    "{} ({}, {}MHz)",
                    rf.channel_num,
                    rf.channel_band.label(),
                    rf.channel_width_mhz
                )
            } else {
                "N/A".into()
            };
            tel.neighbors = rf.neighbors.clone();
            if let Some(hw) = &rf.hardware_address {
                tel.mac = hw.clone();
            }
        }

        // --- SSID resolution (handles macOS masking) ---
        match rf.as_ref().and_then(|r| r.ssid.clone()) {
            Some(ssid) if !ssid.is_empty() => {
                tel.ssid = ssid;
                tel.ssid_available = true;
            }
            _ => {
                tel.ssid_available = false;
                tel.ssid = self
                    .saved_networks
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Wi-Fi (name hidden)".into());
            }
        }
        tel.saved_networks = self.saved_networks.clone();
        // Resolve dual-band siblings by shared prefix (e.g. "Home.5G" / "Home").
        let (sib5, sib24) = resolve_siblings(&tel.ssid, &self.saved_networks);
        tel.sibling_5g_ssid = sib5;
        tel.sibling_24g_ssid = sib24;

        // --- Interface / link via netdev (no subprocess) ---
        if let Ok(iface) = netdev::get_default_interface() {
            tel.mtu = iface.mtu.map(|m| m as i64).unwrap_or(tel.mtu);
            tel.ipv6_active = !iface.ipv6.is_empty();
            if let Some(ip) = iface.ipv4.first() {
                tel.ipv4 = ip.addr().to_string();
            }
            if let Some(mac) = iface.mac_addr {
                let m = mac.to_string();
                // Keep the CoreWLAN hardware address if netdev only has the
                // masked privacy MAC (02:00:00:00:00:00).
                if m != "02:00:00:00:00:00" || tel.mac == "N/A" {
                    tel.mac = m;
                }
            }
            if let Some(gw) = &iface.gateway {
                if let Some(g) = gw.ipv4.first() {
                    tel.gateway = g.to_string();
                }
            }
            tel.dns_servers = iface
                .dns_servers
                .iter()
                .map(|d| d.to_string())
                .take(3)
                .collect();
            tel.link_active = iface.is_running() && tel.ipv4 != "N/A" && !tel.ipv4.is_empty();
        }

        // --- AWDL state (netdev reports awdl0 running flag) ---
        tel.awdl_active = awdl_active();

        // --- Hotspot detection (subnet + SSID keywords) ---
        tel.is_hotspot = is_hotspot(&tel);

        // --- Radio-off / disconnected overrides ---
        if !tel.radio_on {
            tel.ssid = "Radio Off".into();
            tel.band = Band::Offline;
            tel.link_active = false;
        } else if !tel.link_active {
            tel.band = Band::Offline;
        }

        // --- Health score ---
        let has_5g_sibling = self.saved_networks.iter().any(|s| {
            let u = s.to_uppercase();
            (u.contains("5G") || u.contains("6G")) && !u.contains("2.4G")
        });
        let h = compute_health(&tel, ping_ms, loss_pct, has_5g_sibling);
        tel.health_score = h.score;
        tel.health_rating = h.rating;
        tel.alerts = h.alerts;
        if !tel.link_active {
            tel.health_rating = HealthRating::Offline;
        }

        tel
    }
}

fn is_hotspot(tel: &Telemetry) -> bool {
    if tel.ipv4.starts_with("172.20.10.") || tel.gateway == "172.20.10.1" {
        return true;
    }
    if tel.ipv4.starts_with("192.168.43.") || tel.gateway == "192.168.43.1" {
        return true;
    }
    let ssid = tel.ssid.to_lowercase();
    HOTSPOT_KEYWORDS.iter().any(|k| ssid.contains(k))
}

/// Read saved preferred networks. macOS keeps the connected SSID at index 0.
fn discover_saved_networks(interface: &str) -> Vec<String> {
    let out = Command::new("networksetup")
        .args(["-listpreferredwirelessnetworks", interface])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    out.lines()
        .skip(1) // first line is a header
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && l != "<redacted>")
        .collect()
}

/// Find 5GHz and 2.4GHz sibling SSIDs that share the active network's prefix.
fn resolve_siblings(active: &str, saved: &[String]) -> (Option<String>, Option<String>) {
    let base = active
        .split(['.', '_', '-'])
        .next()
        .unwrap_or(active)
        .trim()
        .to_lowercase();
    let mut sib5 = None;
    let mut sib24 = None;
    if base.len() >= 3 {
        for s in saved {
            if !s.to_lowercase().starts_with(&base) {
                continue;
            }
            let u = s.to_uppercase();
            let is_5 = (u.contains("5G") || u.contains("6G"))
                && !u.contains("2.4G")
                && !u.contains("2.5G");
            let is_24 = u.contains("2.4") || u.contains("2.5") || u.contains("2G");
            if is_5 && sib5.is_none() {
                sib5 = Some(s.clone());
            } else if is_24 && sib24.is_none() {
                sib24 = Some(s.clone());
            }
        }
    }
    (sib5, sib24)
}

/// True if the awdl0 interface is up+running (AWDL discovery active).
fn awdl_active() -> bool {
    netdev::get_interfaces()
        .into_iter()
        .find(|i| i.name == "awdl0")
        .map(|i| i.is_running())
        .unwrap_or(false)
}
