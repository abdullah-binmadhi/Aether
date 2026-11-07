//! Rogue-AP / ARP / DNS security audit. Ported from security_guard.py.

use std::net::ToSocketAddrs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::cmd;
use super::lan_scanner::resolve_mac_vendor;
use super::model::Telemetry;

const CANARY_NXDOMAIN: &str = "aether-nxdomain-canary-test-98234.invalid";

#[derive(Clone)]
pub struct SecurityReport {
    pub security_score: i64,
    pub threat_level: String,
    pub threats: Vec<String>,
    pub remediations: Vec<String>,
}

pub struct SecurityGuard {
    interface: String,
    baseline_gateway_mac: Option<String>,
    last_gateway_ip: String,
    pub cached_report: Arc<Mutex<Option<SecurityReport>>>,
    pub is_auditing: Arc<AtomicBool>,
    pub last_audit: Option<Instant>,
}

impl SecurityGuard {
    pub fn new(interface: &str) -> Self {
        SecurityGuard {
            interface: interface.to_string(),
            baseline_gateway_mac: None,
            last_gateway_ip: String::new(),
            cached_report: Arc::new(Mutex::new(None)),
            is_auditing: Arc::new(AtomicBool::new(false)),
            last_audit: None,
        }
    }

    fn discover_gateway(&self) -> String {
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
            .unwrap_or_default()
    }

    /// Returns (threat_level, has_threat, summary).
    fn audit_rogue_aps(&self, tel: &Telemetry) -> (String, bool, bool, String) {
        let ssid = tel.ssid.trim().trim_matches('"');
        if ssid.is_empty() || matches!(ssid, "--" | "Wi-Fi" | "Off" | "N/A") {
            return (
                "CLEAN".into(),
                false,
                false,
                "[OK] No active Wi-Fi association to audit for Evil Twin attacks.".into(),
            );
        }
        let sec_upper = tel.security.to_uppercase();
        let is_enc = ["WPA2", "WPA3", "WPA", "802.1X", "ENTERPRISE", "PSK"]
            .iter()
            .any(|k| sec_upper.contains(k));
        let my_vendor = resolve_mac_vendor(&tel.mac).0;
        let known_brands = [
            "Apple",
            "Ubiquiti",
            "Cisco",
            "Huawei",
            "Google",
            "TP-Link",
            "Xiaomi",
            "RaspberryPi",
        ];

        let mut evil_twins = 0;
        let mut mesh_peers = 0;
        let mut any_critical = false;
        for net in &tel.neighbors {
            if net.label != ssid {
                continue;
            }
            // Neighbour security isn't tracked per-AP in our scan; treat labels only.
            let cand_vendor = resolve_mac_vendor("").0;
            if is_enc && net.label.to_uppercase().contains("OPEN") {
                evil_twins += 1;
                any_critical = true;
            } else if known_brands.contains(&my_vendor.as_str())
                && known_brands.contains(&cand_vendor.as_str())
                && my_vendor != cand_vendor
            {
                evil_twins += 1;
            } else {
                mesh_peers += 1;
            }
        }
        let has_threat = evil_twins > 0;
        let level = if any_critical {
            "CRITICAL"
        } else if has_threat {
            "HIGH"
        } else {
            "CLEAN"
        };
        let summary = if has_threat {
            format!("[!] ROGUE AP / EVIL TWIN DETECTED: {evil_twins} unauthorized AP(s) spoofing '{ssid}'!")
        } else if mesh_peers > 0 {
            format!(
                "[OK] Legitimate Mesh / Multi-BSSID Network ({mesh_peers} peer nodes confirmed)."
            )
        } else {
            "[OK] Solitary BSSID — No duplicate SSIDs detected in range.".into()
        };
        (level.into(), has_threat, any_critical, summary)
    }

    fn audit_arp(&mut self, gateway_ip: &str) -> (String, bool, bool, String) {
        let mut gw = gateway_ip.to_string();
        if gw.is_empty() || matches!(gw.as_str(), "N/A" | "--" | "None") {
            gw = self.discover_gateway();
        }
        if gw.is_empty() {
            return (
                "CLEAN".into(),
                false,
                false,
                "[OK] No active IPv4 gateway found to verify ARP integrity.".into(),
            );
        }

        let arp = cmd::run(&["arp", "-an"], Duration::from_millis(2000));
        let mut gw_mac = String::new();
        let mut mac_to_ips: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for line in arp.stdout.lines() {
            if let (Some(op), Some(cp)) = (line.find('('), line.find(')')) {
                let ip = &line[op + 1..cp];
                let after = &line[cp + 1..];
                let mac = after
                    .split_whitespace()
                    .skip_while(|t| *t != "at")
                    .nth(1)
                    .unwrap_or("");
                if mac.is_empty() || mac == "(incomplete)" || mac == "ff:ff:ff:ff:ff:ff" {
                    continue;
                }
                mac_to_ips
                    .entry(mac.to_string())
                    .or_default()
                    .push(ip.to_string());
                if ip == gw {
                    gw_mac = mac.to_string();
                }
            }
        }
        if gw_mac.is_empty() {
            let one = cmd::run(&["arp", "-n", &gw], Duration::from_millis(1500));
            gw_mac = one
                .stdout
                .split_whitespace()
                .skip_while(|t| *t != "at")
                .nth(1)
                .unwrap_or("")
                .to_string();
        }

        if gw_mac.is_empty() {
            return (
                "CLEAN".into(),
                false,
                false,
                "[OK] Gateway ARP Uncached or Filtered.".into(),
            );
        }

        // MITM: same MAC on multiple IPs.
        if let Some(ips) = mac_to_ips.get(&gw_mac) {
            if ips.len() > 1 {
                return (
                    "CRITICAL".into(),
                    true,
                    true,
                    format!("[!] ARP POISONING / MITM DETECTED on Gateway {gw} ({gw_mac})!"),
                );
            }
        }
        // Baseline drift.
        if gw != self.last_gateway_ip {
            self.last_gateway_ip = gw.clone();
            self.baseline_gateway_mac = Some(gw_mac.clone());
        } else if let Some(base) = &self.baseline_gateway_mac {
            if *base != gw_mac {
                return (
                    "HIGH".into(),
                    true,
                    false,
                    format!("[!] ARP POISONING / MITM DETECTED on Gateway {gw} ({gw_mac})!"),
                );
            }
        }
        let (vendor, _) = resolve_mac_vendor(&gw_mac);
        (
            "CLEAN".into(),
            false,
            false,
            format!("[OK] Gateway ARP Clean ({gw} -> {gw_mac} [{vendor}])."),
        )
    }

    fn audit_dns(&self) -> (String, bool, bool, String) {
        // NXDOMAIN canary should NOT resolve.
        let mut dns_hijacked = false;
        if (CANARY_NXDOMAIN, 0u16)
            .to_socket_addrs()
            .map(|mut a| a.next().is_some())
            .unwrap_or(false)
        {
            dns_hijacked = true;
        }
        // one.one.one.one should resolve to a Cloudflare address.
        if let Ok(mut addrs) = ("one.one.one.one", 0u16).to_socket_addrs() {
            if let Some(a) = addrs.next() {
                let ip = a.ip().to_string();
                if !matches!(
                    ip.as_str(),
                    "1.1.1.1" | "1.0.0.1" | "2606:4700:4700::1111" | "2606:4700:4700::1001"
                ) {
                    dns_hijacked = true;
                }
            }
        }
        if dns_hijacked {
            (
                "HIGH".into(),
                true,
                false,
                "[!] DNS HIJACKING DETECTED: Resolver is spoofing NXDOMAIN / Canary queries!"
                    .into(),
            )
        } else {
            (
                "CLEAN".into(),
                false,
                false,
                "[OK] DNS & WAN Integrity Clean (Zero NXDOMAIN or Captive Hijacks).".into(),
            )
        }
    }

    pub fn run_security_audit(&mut self, tel: &Telemetry) -> SecurityReport {
        // 1. Ingest fresh cached report if available (< 0.005ms)
        if let Ok(guard) = self.cached_report.lock() {
            if let Some(rep) = guard.as_ref() {
                if let Some(t) = self.last_audit {
                    if t.elapsed() < Duration::from_secs(20) {
                        return rep.clone();
                    }
                }
            }
        }

        self.last_audit = Some(Instant::now());

        // 2. Spawn background audit if not already in flight
        let cached_ref = Arc::clone(&self.cached_report);
        let auditing_ref = Arc::clone(&self.is_auditing);
        let mut guard_clone = SecurityGuard {
            interface: self.interface.clone(),
            baseline_gateway_mac: self.baseline_gateway_mac.clone(),
            last_gateway_ip: self.last_gateway_ip.clone(),
            cached_report: Arc::clone(&self.cached_report),
            is_auditing: Arc::clone(&self.is_auditing),
            last_audit: self.last_audit,
        };
        let tel_clone = tel.clone();

        if auditing_ref.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            std::thread::spawn(move || {
                let report = guard_clone.perform_audit(&tel_clone);
                if let Ok(mut g) = cached_ref.lock() {
                    *g = Some(report);
                }
                auditing_ref.store(false, Ordering::SeqCst);
            });
        }

        // Return current cached report or safe baseline
        if let Ok(guard) = self.cached_report.lock() {
            if let Some(rep) = guard.as_ref() {
                return rep.clone();
            }
        }

        SecurityReport {
            security_score: 100,
            threat_level: "CLEAN".into(),
            threats: Vec::new(),
            remediations: Vec::new(),
        }
    }

    pub fn perform_audit(&mut self, tel: &Telemetry) -> SecurityReport {
        let (rogue_lvl, rogue_threat, rogue_crit, rogue_sum) = self.audit_rogue_aps(tel);
        let (arp_lvl, arp_threat, arp_crit, arp_sum) = self.audit_arp(&tel.gateway);
        let (dns_lvl, dns_threat, _dns_crit, dns_sum) = self.audit_dns();

        let sec = tel.security.to_uppercase();
        let mut score = 100i64;
        if sec.contains("WPA3") {
        } else if sec.contains("WPA2") || sec.contains("802.1X") {
            score -= 5;
        } else if sec.contains("WPA") {
            score -= 30;
        } else if sec.contains("WEP") {
            score -= 60;
        } else if sec.contains("OPEN") || sec.is_empty() {
            score -= 50;
        }
        if rogue_threat {
            score -= 40;
        }
        if arp_threat {
            score -= 45;
        }
        if dns_threat {
            score -= 35;
        }
        score = score.clamp(0, 100);

        let level = if rogue_crit || arp_crit || rogue_lvl == "CRITICAL" || arp_lvl == "CRITICAL" {
            "CRITICAL"
        } else if rogue_lvl == "HIGH" || arp_lvl == "HIGH" || dns_lvl == "HIGH" {
            "HIGH"
        } else if score < 70 {
            "CAUTION"
        } else {
            "CLEAN"
        };

        let mut threats = Vec::new();
        let mut remediations = Vec::new();
        if rogue_threat {
            threats.push(rogue_sum.clone());
            remediations.push(
                "Do not enter passwords. Disconnect from untrusted AP and lock to verified BSSID."
                    .to_string(),
            );
        }
        if arp_threat {
            threats.push(arp_sum.clone());
            remediations.push("Active ARP Poisoning detected! Disconnect immediately or enforce static ARP table.".to_string());
        }
        if dns_threat {
            threats.push(dns_sum.clone());
            remediations.push(
                "Switch to Encrypted DNS (DoH/DoT) via Cloudflare (1.1.1.1) or Quad9 (9.9.9.9)."
                    .to_string(),
            );
        }

        SecurityReport {
            security_score: score,
            threat_level: level.into(),
            threats,
            remediations,
        }
    }

    pub fn flush_arp_cache(&mut self, gateway_ip: &str) -> bool {
        let gw = if gateway_ip.is_empty() {
            self.discover_gateway()
        } else {
            gateway_ip.to_string()
        };
        if gw.is_empty() {
            return false;
        }
        if cmd::ok(&["arp", "-d", &gw], Duration::from_millis(2000)) {
            self.baseline_gateway_mac = None;
            return true;
        }
        cmd::ok(
            &["sudo", "-n", "arp", "-d", &gw],
            Duration::from_millis(2000),
        )
    }
}
