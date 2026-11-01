//! Captive portal probe & breakout. Ported from captive.py.

use std::time::{Duration, Instant};

use super::cmd;
use super::dns_engine::DnsEngine;

const CAPTIVE_PROBE_URL: &str = "http://captive.apple.com/hotspot-detect.html";

pub struct CaptiveResult {
    pub captive: bool,
    pub portal_url: String,
    pub summary: String,
}

pub struct CaptivePortalEngine {
    service: String,
    last_probe: Option<Instant>,
    captive_detected: bool,
    portal_url: String,
    status_summary: String,
}

impl CaptivePortalEngine {
    pub fn new(service: &str) -> Self {
        CaptivePortalEngine {
            service: service.to_string(),
            last_probe: None,
            captive_detected: false,
            portal_url: String::new(),
            status_summary: "[OK] Direct WAN (No Captive Portal)".into(),
        }
    }

    pub fn probe_portal(&mut self, force: bool) -> CaptiveResult {
        if !force {
            if let Some(t) = self.last_probe {
                if t.elapsed() < Duration::from_secs(15) {
                    return CaptiveResult {
                        captive: self.captive_detected,
                        portal_url: self.portal_url.clone(),
                        summary: self.status_summary.clone(),
                    };
                }
            }
        }
        self.last_probe = Some(Instant::now());

        let out = cmd::run(
            &[
                "curl",
                "--noproxy",
                "*",
                "-4",
                "-s",
                "-i",
                "-m",
                "2.0",
                CAPTIVE_PROBE_URL,
            ],
            Duration::from_millis(2500),
        );
        let body = out.stdout;
        if body.contains("<BODY>Success</BODY>") || body.contains("Success") {
            self.captive_detected = false;
            self.portal_url.clear();
            self.status_summary = "[OK] Direct WAN (No Captive Portal)".into();
        } else if ["301", "302", "303", "307", "308", "200"].iter().any(|c| {
            body.get(..60.min(body.len()))
                .map(|h| h.contains(c))
                .unwrap_or(false)
        }) {
            self.captive_detected = true;
            self.portal_url = body
                .lines()
                .find_map(|l| {
                    let lc = l.to_lowercase();
                    if lc.starts_with("location:") {
                        Some(l[9..].trim().to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| CAPTIVE_PROBE_URL.to_string());
            let short: String = self.portal_url.chars().take(32).collect();
            self.status_summary = format!("[!] CAPTIVE PORTAL TRAP DETECTED ({short})");
        } else {
            self.captive_detected = false;
            self.portal_url.clear();
        }

        CaptiveResult {
            captive: self.captive_detected,
            portal_url: self.portal_url.clone(),
            summary: self.status_summary.clone(),
        }
    }

    pub fn breakout_captive_portal(&self, dns: Option<&DnsEngine>) -> String {
        match dns {
            Some(d) => {
                d.reset_to_dhcp_dns();
            }
            None => {
                cmd::run(
                    &["networksetup", "-setdnsservers", &self.service, "Empty"],
                    Duration::from_millis(2000),
                );
                cmd::run(&["dscacheutil", "-flushcache"], Duration::from_millis(1500));
            }
        }
        let target = if self.portal_url.is_empty() {
            CAPTIVE_PROBE_URL.to_string()
        } else {
            self.portal_url.clone()
        };
        cmd::run(&["open", &target], Duration::from_millis(2000));
        let short: String = target.chars().take(36).collect();
        format!("[OK] Reset DNS to DHCP & launched Captive Portal login ({short}).")
    }
}
