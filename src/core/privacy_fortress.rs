//! Privacy Fortress, DNS Racer & App Telemetry Sniffer (Tab 10 / [0]).
//!
//! Provides zero-hardcoding 1-key DNS racer benchmarking major public resolvers
//! against the local ISP gateway, background app telemetry/tracker connection sniffing,
//! and VPN/WireGuard encrypted tunnel auditing.

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::cmd;

#[derive(Debug, Clone)]
pub struct DnsCandidateEntry {
    pub name: &'static str,
    pub primary: &'static str,
    pub secondary: &'static str,
    pub features: &'static str,
    pub rtt_ms: Option<f64>,
    pub grade: &'static str,
    pub is_current: bool,
}

#[derive(Debug, Clone)]
pub struct AppTelemetryEntry {
    pub command: String,
    pub pid: u32,
    pub remote_endpoint: String,
    pub destination_org: String,
    pub classification: String,
    pub risk_badge: &'static str,
}

#[derive(Debug, Clone, Default)]
pub struct TunnelSnapshot {
    pub active_tunnel: String,
    pub active_resolver: String,
    pub encryption_posture: String,
    pub trackers_count: usize,
    pub is_vpn_active: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PrivacyData {
    pub dns_candidates: Vec<DnsCandidateEntry>,
    pub selected_dns_idx: usize,
    pub telemetry_calls: Vec<AppTelemetryEntry>,
    pub selected_telemetry_idx: usize,
    pub active_column: usize, // 0 = DNS Racer, 1 = App Telemetry
    pub tunnel: TunnelSnapshot,
    pub is_racing: bool,
    pub last_race_summary: String,
}

pub const DEFAULT_DNS_RESOLVERS: &[(&str, &str, &str, &str)] = &[
    ("Cloudflare Anycast", "1.1.1.1", "1.0.0.1", "DNSSEC + DoH / Privacy First"),
    ("Quad9 Security", "9.9.9.9", "149.112.112.112", "Malware & Phishing Threat Shield"),
    ("Google Public DNS", "8.8.8.8", "8.8.4.4", "Global Geo-Distributed Anycast"),
    ("NextDNS Privacy", "45.90.28.0", "45.90.30.0", "Encrypted Custom Telemetry Block"),
    ("AdGuard DNS", "94.140.14.14", "94.140.15.15", "Adverts & Behavioral Trackers Block"),
    ("Cisco OpenDNS", "208.67.222.222", "208.67.220.220", "Parental & Content Filtering"),
];

pub struct PrivacyFortress {
    pub dns_candidates: Vec<DnsCandidateEntry>,
    pub selected_dns_idx: usize,
    pub telemetry_calls: Vec<AppTelemetryEntry>,
    pub selected_telemetry_idx: usize,
    pub active_column: usize,
    pub last_sample: Option<Instant>,
    pub cached: Option<PrivacyData>,
    pub bg_snapshot: Arc<Mutex<Option<(Vec<DnsCandidateEntry>, Vec<AppTelemetryEntry>, TunnelSnapshot)>>>,
    pub is_refreshing: Arc<AtomicBool>,
}

impl PrivacyFortress {
    pub fn new() -> Self {
        let mut candidates = Vec::new();

        // Local ISP placeholder (dynamically updated)
        candidates.push(DnsCandidateEntry {
            name: "Local ISP / Gateway",
            primary: "DHCP Assigned",
            secondary: "Local Default",
            features: "ECS Local / Edge Routing",
            rtt_ms: None,
            grade: "Pending",
            is_current: true,
        });

        for (name, p, s, feat) in DEFAULT_DNS_RESOLVERS {
            candidates.push(DnsCandidateEntry {
                name,
                primary: p,
                secondary: s,
                features: feat,
                rtt_ms: None,
                grade: "Pending",
                is_current: false,
            });
        }

        let bg_snapshot = Arc::new(Mutex::new(None));
        let is_refreshing = Arc::new(AtomicBool::new(false));

        // Pre-warm privacy audit immediately on startup in background
        spawn_background_privacy_scan(candidates.clone(), Arc::clone(&bg_snapshot), Arc::clone(&is_refreshing));

        PrivacyFortress {
            dns_candidates: candidates,
            selected_dns_idx: 0,
            telemetry_calls: Vec::new(),
            selected_telemetry_idx: 0,
            active_column: 0,
            last_sample: None,
            cached: None,
            bg_snapshot,
            is_refreshing,
        }
    }

    pub fn cursor_up(&mut self) {
        if self.active_column == 0 {
            if self.selected_dns_idx > 0 {
                self.selected_dns_idx -= 1;
            }
        } else if self.selected_telemetry_idx > 0 {
            self.selected_telemetry_idx -= 1;
        }
        if let Some(c) = self.cached.as_mut() {
            c.selected_dns_idx = self.selected_dns_idx;
            c.selected_telemetry_idx = self.selected_telemetry_idx;
        }
    }

    pub fn cursor_down(&mut self) {
        if self.active_column == 0 {
            let max = self.dns_candidates.len().saturating_sub(1);
            if self.selected_dns_idx < max {
                self.selected_dns_idx += 1;
            }
        } else {
            let max = self.telemetry_calls.len().saturating_sub(1);
            if self.selected_telemetry_idx < max {
                self.selected_telemetry_idx += 1;
            }
        }
        if let Some(c) = self.cached.as_mut() {
            c.selected_dns_idx = self.selected_dns_idx;
            c.selected_telemetry_idx = self.selected_telemetry_idx;
        }
    }

    pub fn toggle_column(&mut self) {
        self.active_column = if self.active_column == 0 { 1 } else { 0 };
        if let Some(c) = self.cached.as_mut() {
            c.active_column = self.active_column;
        }
    }

    pub fn trigger_race(&mut self) {
        self.last_sample = Some(Instant::now());
        spawn_background_privacy_scan(
            self.dns_candidates.clone(),
            Arc::clone(&self.bg_snapshot),
            Arc::clone(&self.is_refreshing),
        );
    }

    pub fn sample(&mut self, is_manual_race: bool) -> PrivacyData {
        let now = Instant::now();

        // 1. Ingest any fresh snapshot from background thread (< 0.005ms)
        if let Ok(mut guard) = self.bg_snapshot.lock() {
            if let Some((dns, telem, tunnel)) = guard.take() {
                self.dns_candidates = dns;
                self.telemetry_calls = telem;
                if let Some(c) = self.cached.as_mut() {
                    c.tunnel = tunnel;
                }
            }
        }

        let should_refresh = is_manual_race
            || self
                .last_sample
                .map(|t| now.duration_since(t) > Duration::from_secs(15))
                .unwrap_or(true);

        if should_refresh {
            self.last_sample = Some(now);
            spawn_background_privacy_scan(
                self.dns_candidates.clone(),
                Arc::clone(&self.bg_snapshot),
                Arc::clone(&self.is_refreshing),
            );
        }

        let is_racing = self.is_refreshing.load(Ordering::Relaxed);
        let fastest_summary = if let Some(best) = self
            .dns_candidates
            .iter()
            .filter_map(|c| c.rtt_ms)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
        {
            format!("Fastest: {:.1} ms", best)
        } else if is_racing {
            "Racing Resolvers...".into()
        } else {
            "DNS Ready".into()
        };

        let tunnel = self
            .cached
            .as_ref()
            .map(|c| c.tunnel.clone())
            .unwrap_or_else(|| TunnelSnapshot {
                active_tunnel: "Scanning network tunnel...".into(),
                active_resolver: "Default DHCP".into(),
                encryption_posture: "[CLEARTEXT] Standard Port 53 UDP".into(),
                trackers_count: 0,
                is_vpn_active: false,
            });

        let data = PrivacyData {
            dns_candidates: self.dns_candidates.clone(),
            selected_dns_idx: self.selected_dns_idx,
            telemetry_calls: self.telemetry_calls.clone(),
            selected_telemetry_idx: self.selected_telemetry_idx,
            active_column: self.active_column,
            tunnel,
            is_racing,
            last_race_summary: fastest_summary,
        };

        self.cached = Some(data.clone());
        data
    }

    pub fn race_dns_candidates(&mut self, active_resolver_ip: &str) {
        self.dns_candidates = race_dns_parallel(self.dns_candidates.clone(), active_resolver_ip);
    }

    pub fn apply_selected_dns(&mut self, service: &str) -> String {
        if let Some(cand) = self.dns_candidates.get(self.selected_dns_idx) {
            if cand.name.contains("ISP") {
                return self.reset_to_dhcp(service);
            }
            let out = cmd::run(
                &["networksetup", "-setdnsservers", service, cand.primary, cand.secondary],
                Duration::from_millis(3000),
            );
            flush_dns_cache();
            if out.success {
                format!("[OK] Applied {} ({}, {}) as active resolver.", cand.name, cand.primary, cand.secondary)
            } else {
                format!("[!] Could not apply DNS: {}", out.stderr)
            }
        } else {
            "[!] No resolver selected".into()
        }
    }

    pub fn reset_to_dhcp(&mut self, service: &str) -> String {
        let out = cmd::run(
            &["networksetup", "-setdnsservers", service, "Empty"],
            Duration::from_millis(3000),
        );
        flush_dns_cache();
        if out.success {
            "[OK] Reset DNS to local ISP router DHCP defaults.".into()
        } else {
            format!("[!] Could not reset DNS: {}", out.stderr)
        }
    }
}

pub fn spawn_background_privacy_scan(
    candidates_template: Vec<DnsCandidateEntry>,
    bg_snapshot: Arc<Mutex<Option<(Vec<DnsCandidateEntry>, Vec<AppTelemetryEntry>, TunnelSnapshot)>>>,
    is_refreshing: Arc<AtomicBool>,
) {
    if is_refreshing.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return; // Already refreshing
    }

    std::thread::spawn(move || {
        let tunnel = detect_tunnel_and_dns();
        let telemetry = sniff_active_telemetry();
        let dns = race_dns_parallel(candidates_template, &tunnel.active_resolver);

        if let Ok(mut guard) = bg_snapshot.lock() {
            *guard = Some((dns, telemetry, tunnel));
        }

        is_refreshing.store(false, Ordering::SeqCst);
    });
}

pub fn race_dns_parallel(
    mut candidates: Vec<DnsCandidateEntry>,
    active_resolver_ip: &str,
) -> Vec<DnsCandidateEntry> {
    let results: Vec<(usize, Option<f64>, bool, &'static str)> = std::thread::scope(|s| {
        let handles: Vec<_> = candidates
            .iter()
            .enumerate()
            .map(|(idx, cand)| {
                let primary = cand.primary;
                let name = cand.name;
                s.spawn(move || {
                    let target_ip = if name.contains("ISP") {
                        if !active_resolver_ip.is_empty() && active_resolver_ip != "Default DHCP" {
                            active_resolver_ip
                        } else {
                            "192.168.1.1"
                        }
                    } else {
                        primary
                    };

                    let rtt = measure_udp_dns(target_ip, "google.com");
                    let is_current = primary == active_resolver_ip
                        || (name.contains("ISP")
                            && (active_resolver_ip.is_empty()
                                || active_resolver_ip.starts_with("192.168.")
                                || active_resolver_ip.starts_with("10.")
                                || active_resolver_ip.starts_with("84.")));
                    let grade = match rtt {
                        Some(ms) if ms < 15.0 => "A+ (Ultra-Fast)",
                        Some(ms) if ms < 35.0 => "A  (Fast)",
                        Some(ms) if ms < 75.0 => "B+ (Optimal)",
                        Some(ms) if ms < 120.0 => "B  (Moderate)",
                        Some(_) => "C  (High Latency)",
                        None => "FAIL / BLOCKED",
                    };
                    (idx, rtt, is_current, grade)
                })
            })
            .collect();

        handles
            .into_iter()
            .map(|h| h.join().unwrap_or((0, None, false, "FAIL / BLOCKED")))
            .collect()
    });

    for (idx, rtt, is_curr, grade) in results {
        if let Some(c) = candidates.get_mut(idx) {
            c.rtt_ms = rtt;
            c.is_current = is_curr;
            c.grade = grade;
        }
    }

    // Sort reachable candidates first by lowest RTT
    candidates.sort_by(|a, b| {
        let rtt_a = a.rtt_ms.unwrap_or(9999.0);
        let rtt_b = b.rtt_ms.unwrap_or(9999.0);
        rtt_a.partial_cmp(&rtt_b).unwrap_or(std::cmp::Ordering::Equal)
    });

    candidates
}

pub fn measure_udp_dns(server_ip: &str, query_domain: &str) -> Option<f64> {
    // Construct standard DNS A-record query packet
    let mut packet = Vec::with_capacity(64);
    // 1. Transaction ID
    packet.extend_from_slice(&[0x12, 0x34]);
    // 2. Flags: Standard query, Recursion Desired (0x0100)
    packet.extend_from_slice(&[0x01, 0x00]);
    // 3. QDCOUNT: 1
    packet.extend_from_slice(&[0x00, 0x01]);
    // 4. ANCOUNT: 0, NSCOUNT: 0, ARCOUNT: 0
    packet.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    // 5. QNAME: length-prefixed domain labels
    for part in query_domain.split('.') {
        if part.is_empty() {
            continue;
        }
        packet.push(part.len() as u8);
        packet.extend_from_slice(part.as_bytes());
    }
    packet.push(0x00); // Root label null terminator

    // 6. QTYPE: Type A (0x0001)
    packet.extend_from_slice(&[0x00, 0x01]);
    // 7. QCLASS: Class IN (0x0001)
    packet.extend_from_slice(&[0x00, 0x01]);

    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.set_read_timeout(Some(Duration::from_millis(250))).ok()?;
    socket.set_write_timeout(Some(Duration::from_millis(200))).ok()?;

    let target: SocketAddr = format!("{server_ip}:53").parse().ok()?;

    let t0 = Instant::now();
    socket.send_to(&packet, target).ok()?;

    let mut buf = [0u8; 512];
    let (len, _) = socket.recv_from(&mut buf).ok()?;
    if len >= 12 {
        let dt = t0.elapsed().as_secs_f64() * 1000.0;
        Some((dt * 10.0).round() / 10.0)
    } else {
        None
    }
}

pub fn detect_tunnel_and_dns() -> TunnelSnapshot {
    // 1. Check for utun interfaces (WireGuard, OpenVPN, Tailscale, Cloudflare WARP)
    let if_out = cmd::run(&["ifconfig"], Duration::from_millis(1000));
    let mut utun_name = None;
    for line in if_out.stdout.lines() {
        if line.starts_with("utun") {
            if let Some(name) = line.split(':').next() {
                // Ignore loopback utun if flags are not UP
                if line.contains("<UP,") || line.contains("POINTOPOINT") {
                    utun_name = Some(name.to_string());
                    break;
                }
            }
        }
    }

    // 2. Check active resolver from `scutil --dns`
    let sc_out = cmd::run(&["scutil", "--dns"], Duration::from_millis(1000));
    let mut resolver_ip = String::new();
    for line in sc_out.stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("nameserver[0]") {
            if let Some(ip) = trimmed.split(':').nth(1) {
                resolver_ip = ip.trim().to_string();
                break;
            }
        }
    }

    if resolver_ip.is_empty() {
        resolver_ip = "Default DHCP Gateway".into();
    }

    let is_vpn = utun_name.is_some();
    let tunnel_desc = if let Some(ref u) = utun_name {
        format!("{u} [Encrypted VPN Tunnel Active]")
    } else {
        "Direct Wi-Fi / No VPN Tunnel".into()
    };

    let encryption = if is_vpn {
        "[ENCRYPTED] WireGuard / IPsec Tunnel".into()
    } else if resolver_ip == "1.1.1.1" || resolver_ip == "9.9.9.9" {
        "[ENCRYPTED CAPABLE] Anycast DNSSEC".into()
    } else {
        "[CLEARTEXT] Standard Port 53 UDP".into()
    };

    TunnelSnapshot {
        active_tunnel: tunnel_desc,
        active_resolver: resolver_ip,
        encryption_posture: encryption,
        trackers_count: 0,
        is_vpn_active: is_vpn,
    }
}

pub fn sniff_active_telemetry() -> Vec<AppTelemetryEntry> {
    let out = cmd::run(
        &["lsof", "-i", "-P", "-n", "-sTCP:ESTABLISHED"],
        Duration::from_millis(1500),
    );

    let mut entries = Vec::new();
    let lines: Vec<&str> = out.stdout.lines().collect();

    for line in lines.iter().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            continue;
        }

        let cmd_name = parts[0];
        let pid = parts[1].parse::<u32>().unwrap_or(0);
        let node_name = parts[parts.len() - 2]; // e.g. 192.168.100.2:61186->142.251.127.84:443

        if let Some(dest) = node_name.split("->").nth(1) {
            let dest_ip = dest.split(':').next().unwrap_or("");

            // Filter out LAN local traffic
            if dest_ip.starts_with("192.168.") || dest_ip.starts_with("10.") || dest_ip.starts_with("127.") {
                continue;
            }

            let (org, class, badge) = classify_telemetry(cmd_name, dest_ip);
            entries.push(AppTelemetryEntry {
                command: cmd_name.to_string(),
                pid,
                remote_endpoint: dest.to_string(),
                destination_org: org.to_string(),
                classification: class.to_string(),
                risk_badge: badge,
            });
        }
    }

    // Deduplicate by (command, destination_org)
    entries.sort_by(|a, b| a.command.cmp(&b.command));
    entries.dedup_by(|a, b| a.command == b.command && a.destination_org == b.destination_org);
    entries
}

fn classify_telemetry(cmd: &str, ip: &str) -> (&'static str, &'static str, &'static str) {
    if ip.starts_with("142.250.") || ip.starts_with("142.251.") || ip.starts_with("172.217.") {
        ("Google Cloud / Edge", "Google Analytics & Sync", "[TELEMETRY]")
    } else if ip.starts_with("52.") || ip.starts_with("54.") || ip.starts_with("3.") {
        ("Amazon AWS / CloudFront", "Telemetry & Metric Ingestion", "[ANALYTICS]")
    } else if ip.starts_with("17.") {
        ("Apple Infrastructure", "APNS & Diagnostics Push", "[APPLE-SYNC]")
    } else if ip.starts_with("104.") || ip.starts_with("172.67.") {
        ("Cloudflare Anycast", "Telemetry / Edge Verification", "[EDGE-CALL]")
    } else if ip.starts_with("20.") || ip.starts_with("40.") {
        ("Microsoft Azure", "Telemetry Ingestion (Vortex)", "[TELEMETRY]")
    } else if cmd.to_lowercase().contains("spotify") {
        ("Spotify Edge", "Playback Telemetry & Metrics", "[ANALYTICS]")
    } else if cmd.to_lowercase().contains("discord") {
        ("Discord Gateway", "Heartbeat & Analytics Call", "[MONITORED]")
    } else {
        ("External Internet Host", "Direct Application Socket", "[MONITORED]")
    }
}

pub fn flush_dns_cache() {
    let _ = cmd::run(&["dscacheutil", "-flushcache"], Duration::from_millis(1500));
    let _ = cmd::run(&["killall", "-HUP", "mDNSResponder"], Duration::from_millis(1500));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fortress_initialization() {
        let f = PrivacyFortress::new();
        assert!(!f.dns_candidates.is_empty());
        assert!(f.dns_candidates.iter().any(|c| c.primary == "1.1.1.1"));
        assert!(f.dns_candidates.iter().any(|c| c.primary == "8.8.8.8"));
        assert!(f.dns_candidates.iter().any(|c| c.primary == "9.9.9.9"));
    }

    #[test]
    fn test_navigation_and_column_toggle() {
        let mut f = PrivacyFortress::new();
        assert_eq!(f.active_column, 0);

        f.toggle_column();
        assert_eq!(f.active_column, 1);

        f.toggle_column();
        assert_eq!(f.active_column, 0);

        f.cursor_down();
        assert_eq!(f.selected_dns_idx, 1);

        f.cursor_up();
        assert_eq!(f.selected_dns_idx, 0);
    }

    #[test]
    fn test_telemetry_classifier() {
        let (org, _class, badge) = classify_telemetry("Chrome", "142.251.127.84");
        assert_eq!(org, "Google Cloud / Edge");
        assert_eq!(badge, "[TELEMETRY]");

        let (org, _, _) = classify_telemetry("launchd", "17.253.1.1");
        assert_eq!(org, "Apple Infrastructure");
    }
}
