//! Cellular hotspot detection & armor. Ported from hotspot.py.

use std::time::Duration;

use super::antilag::AntiLagEngine;
use super::cmd;
use super::model::Telemetry;

const HOTSPOT_SSID_KEYWORDS: &[&str] = &[
    "iphone", "ipad", "galaxy", "pixel", "android", "hotspot", "oneplus", "xiaomi",
];
const HOTSPOT_THROTTLE_DAEMONS: &[&str] = &[
    "cloudd",
    "photolibraryd",
    "bird",
    "cloudpaird",
    "softwareupdated",
    "AssetCache",
    "photoanalysisd",
    "mediaanalysisd",
    "mds_stores",
];

pub struct HotspotEngine {
    pub armor_active: bool,
    auto_applied_for_ssid: String,
}

impl HotspotEngine {
    pub fn new() -> Self {
        HotspotEngine {
            armor_active: false,
            auto_applied_for_ssid: String::new(),
        }
    }

    pub fn is_cellular_hotspot(tel: &Telemetry) -> bool {
        let ipv4 = &tel.ipv4;
        let gw = &tel.gateway;
        if ipv4.starts_with("172.20.10.") || gw == "172.20.10.1" {
            return true;
        }
        if ipv4.starts_with("192.168.43.") || gw == "192.168.43.1" {
            return true;
        }
        let ssid = tel.ssid.to_lowercase();
        HOTSPOT_SSID_KEYWORDS.iter().any(|k| ssid.contains(k))
    }

    pub fn toggle_hotspot_armor(
        &mut self,
        enable: Option<bool>,
        antilag: Option<&mut AntiLagEngine>,
    ) -> String {
        let target = enable.unwrap_or(!self.armor_active);
        self.armor_active = target;
        if target {
            if let Some(al) = antilag {
                al.toggle_sync_daemons(true);
                AntiLagEngine::debloat_background_queues();
            }
            let mut clamped = 0;
            for proc_name in HOTSPOT_THROTTLE_DAEMONS {
                let res = cmd::run(&["pgrep", "-x", proc_name], Duration::from_millis(600));
                if res.success {
                    for pid in res.stdout.split_whitespace() {
                        if pid.chars().all(|c| c.is_ascii_digit()) && !pid.is_empty() {
                            cmd::run(&["taskpolicy", "-b", "-p", pid], Duration::from_millis(400));
                            clamped += 1;
                        }
                    }
                }
            }
            format!("● HOTSPOT ARMOR ACTIVE (Link-Safe Zero-Disconnect + {clamped} Low-Data QoS Clamps)")
        } else {
            if let Some(al) = antilag {
                al.toggle_sync_daemons(false);
            }
            self.auto_applied_for_ssid.clear();
            "○ HOTSPOT ARMOR OFF (Resumed Standard Broadband Sync Scheduling).".into()
        }
    }

    pub fn evaluate_auto_armor(
        &mut self,
        tel: &Telemetry,
        auto_pilot_enabled: bool,
        antilag: Option<&mut AntiLagEngine>,
    ) -> Option<String> {
        let curr = &tel.ssid;
        if curr.is_empty() || matches!(curr.as_str(), "Disconnected" | "Scanning..." | "N/A") {
            return None;
        }
        let is_hot = Self::is_cellular_hotspot(tel);
        if is_hot && auto_pilot_enabled && !self.armor_active && self.auto_applied_for_ssid != *curr
        {
            self.auto_applied_for_ssid = curr.clone();
            Some(self.toggle_hotspot_armor(Some(true), antilag))
        } else if !is_hot && self.armor_active && !self.auto_applied_for_ssid.is_empty() {
            Some(self.toggle_hotspot_armor(Some(false), antilag))
        } else {
            None
        }
    }
}
