//! 802.11 disassociation reason decoder. Ported from disassoc_tracker.py.

use std::time::{Duration, Instant};

use super::cmd;

/// (code, title, remediation) for a decoded reason.
pub struct Decoded {
    pub code: String,
    pub title: String,
    pub remediation: String,
}

fn reason_map(key: &str) -> Option<(&'static str, &'static str)> {
    Some(match key {
        "0" => ("No Recent Drop", "Link stable — zero disassociation events"),
        "1" => (
            "Unspecified 802.11 Reason",
            "Router or AP reset link; press [R] to re-handshake",
        ),
        "2" => (
            "Previous Auth No Longer Valid",
            "WPA key cache expired; press [E] to verify password",
        ),
        "3" => (
            "Deauthenticated (STA Leaving)",
            "Normal client roam or network switch",
        ),
        "4" => (
            "Disassociated Due to Inactivity",
            "AP kicked idle radio; keep [K] 25Hz CAM Pacer ON",
        ),
        "6" => (
            "Class 2 Frame from Non-Auth STA",
            "AP state desync; press [H] to re-associate",
        ),
        "7" => (
            "Class 3 Frame from Non-Assoc STA",
            "AP state desync during roam; press [H] to cure",
        ),
        "8" => (
            "Disassociated (STA Leaving BSS)",
            "Client initiated roam or switched SSID",
        ),
        "14" => (
            "MIC Failure (WPA Integrity Error)",
            "RF corruption or WPA password mismatch",
        ),
        "15" => (
            "4-Way WPA Handshake Timeout",
            "Weak signal or wrong password during key exchange",
        ),
        "16" => (
            "Group Key Handshake Timeout",
            "Router GTK rekey failed; disable router PMF or [K] Pacer",
        ),
        "23" => (
            "802.1X Authentication Failed",
            "Enterprise/RADIUS auth rejected",
        ),
        "34" => (
            "Disassociated for Unspecified QoS / Poor Link",
            "Signal dropped too low; press [R] to roam",
        ),
        "INVALID_AKMS" => (
            "Auth Key Mgmt Switch (AKM)",
            "Switched between WPA2/WPA3 or Hotspot APs",
        ),
        "BEACON_LOST" => (
            "AP Beacon Loss (Missed Beacons)",
            "Keep [K] 25Hz CAM Pacer ON or move closer to AP",
        ),
        "BEACONLOSS" => (
            "AP Beacon Loss (Missed Beacons)",
            "Keep [K] 25Hz CAM Pacer ON or move closer to AP",
        ),
        "ROAM_INITIATED" => (
            "macOS CoreWiFi Roam Triggered",
            "Transitioned between BSSIDs",
        ),
        _ => return None,
    })
}

pub fn decode(code_or_str: &str) -> Decoded {
    let key = code_or_str.trim().to_uppercase();
    if let Some((title, rem)) = reason_map(&key) {
        return Decoded {
            code: key,
            title: title.into(),
            remediation: rem.into(),
        };
    }
    // Embedded digits.
    let num: String = key.chars().filter(|c| c.is_ascii_digit()).collect();
    if !num.is_empty() {
        if let Some((title, rem)) = reason_map(&num) {
            return Decoded {
                code: format!("Code {num}"),
                title: title.into(),
                remediation: rem.into(),
            };
        }
    }
    if key.is_empty() || matches!(key.as_str(), "NONE" | "NULL" | "(NULL)") {
        return Decoded {
            code: "0".into(),
            title: "No Recent Drop".into(),
            remediation: "Link stable — zero disassociation events".into(),
        };
    }
    let short: String = key.chars().take(18).collect();
    Decoded {
        code: short.clone(),
        title: format!("802.11 Event ({short})"),
        remediation: "Keep [K] 25Hz CAM Pacer active to prevent idle AP drops".into(),
    }
}

pub struct DisassocTracker {
    interface: String,
    last_check: Option<Instant>,
    cached_summary: String,
}

impl DisassocTracker {
    pub fn new(interface: &str) -> Self {
        DisassocTracker {
            interface: interface.to_string(),
            last_check: None,
            cached_summary: "[OK] 0 Unplanned Drops (Link Stable)".into(),
        }
    }

    pub fn check_recent_drops(&mut self, force: bool) -> String {
        if !force {
            if let Some(t) = self.last_check {
                if t.elapsed() < Duration::from_secs(10) {
                    return self.cached_summary.clone();
                }
            }
        }
        self.last_check = Some(Instant::now());

        let mut reason_raw = "0".to_string();
        let stdin = format!("show State:/Network/Interface/{}/AirPort\n", self.interface);
        let out = cmd::run_with_stdin(&["scutil"], Some(&stdin), Duration::from_millis(1200));
        for line in out.stdout.lines() {
            let l = line.trim();
            if let Some(v) = l
                .strip_prefix("DisconnectReason :")
                .or_else(|| l.strip_prefix("DisconnectReason:"))
            {
                reason_raw = v.trim().to_string();
            }
        }

        if matches!(reason_raw.as_str(), "0" | "" | "None") {
            let logout = cmd::run(
                &[
                    "/usr/bin/log",
                    "show",
                    "--predicate",
                    "process == \"airportd\"",
                    "--last",
                    "5m",
                ],
                Duration::from_millis(1800),
            );
            // Last PreviousDisconnectReason wins.
            for line in logout.stdout.lines() {
                if let Some(idx) = line.find("WiFiNetworkPreviousDisconnectReason") {
                    let tail = &line[idx..];
                    if let Some(eq) = tail.find('=') {
                        let val: String = tail[eq + 1..]
                            .trim()
                            .trim_matches('"')
                            .chars()
                            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                            .collect();
                        if !val.is_empty() {
                            reason_raw = val;
                        }
                    }
                }
            }
        }

        let decoded = decode(&reason_raw);
        let summary = if matches!(
            decoded.code.as_str(),
            "0" | "3" | "8" | "INVALID_AKMS" | "ROAM_INITIATED"
        ) {
            format!("[OK] Last Event: {} ({})", decoded.title, decoded.code)
        } else {
            format!(
                "[!] Drop Reason ({}): {} -> {}",
                decoded.code, decoded.title, decoded.remediation
            )
        };
        self.cached_summary = summary.clone();
        summary
    }
}
