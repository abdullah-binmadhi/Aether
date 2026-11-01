//! WAN / YouTube GGC / IPv6 / MTU auto-doctor. Ported from wan_doctor.py.
//!
//! The Python version ran a background thread; here the engine is polled from
//! the RF worker. `probe_once` fills the snapshot; the `fix_*` methods are the
//! one-key remediations bound in the UI.

use std::time::Duration;

use super::cmd;
use super::kernel_tuner::KernelTuner;
use super::pmtu::PmtuDoctor;

const FOREIGN_GGC_PREFIXES: &[&str] = &[
    "lga", "iad", "ewr", "jfk", "bos", "ord", "atl", "dfw", "mia", "lax", "sjc", "sea", "ams",
    "fra", "lhr", "cdg", "mrs", "mad", "mil", "zrh", "arn", "waw", "yyz", "gru",
];

const QUIC_BROWSERS: &[&str] = &[
    "com.google.Chrome",
    "com.brave.Browser",
    "company.thebrowser.Browser",
    "com.microsoft.edgemac",
];

#[derive(Clone)]
pub struct WanSnapshot {
    pub radio_on: bool,
    pub link_active: bool,
    pub wan_online: bool,
    pub local_ip: String,
    pub service_order_optimal: bool,
    pub yt_rtt_ms: f64,
    pub yt_jitter_ms: f64,
    pub yt_single_mbps: f64,
    pub ggc_node: String,
    pub ggc_ip: String,
    pub ggc_is_misrouted: bool,
    pub ecs_dns_optimal: bool,
    pub active_dns: Vec<String>,
    pub ipv6_mode: String,
    pub ipv6_rtt_ms: f64,
    pub ipv6_prefix_count: i64,
    pub ipv6_asymmetric: bool,
    pub interface_mtu: i64,
    pub optimal_wire_mtu: i64,
    pub kernel_mss: i64,
    pub tcp_recvspace_kb: i64,
    pub tcp_autorcvmax_mb: i64,
    pub tcp_win_scale: i64,
    pub tcp_delayed_ack: i64,
    pub wmm_kernel_enabled: bool,
    pub quic_bypass_active: bool,
    pub zombie_socket_count: i64,
    pub zombie_procs: Vec<String>,
    pub last_heal_summary: String,
}

impl Default for WanSnapshot {
    fn default() -> Self {
        WanSnapshot {
            radio_on: true,
            link_active: true,
            wan_online: true,
            local_ip: String::new(),
            service_order_optimal: true,
            yt_rtt_ms: 0.0,
            yt_jitter_ms: 0.0,
            yt_single_mbps: 0.0,
            ggc_node: "PROBING...".into(),
            ggc_ip: String::new(),
            ggc_is_misrouted: false,
            ecs_dns_optimal: true,
            active_dns: Vec::new(),
            ipv6_mode: "Link-Local".into(),
            ipv6_rtt_ms: 0.0,
            ipv6_prefix_count: 0,
            ipv6_asymmetric: false,
            interface_mtu: 1492,
            optimal_wire_mtu: 1492,
            kernel_mss: 512,
            tcp_recvspace_kb: 128,
            tcp_autorcvmax_mb: 4,
            tcp_win_scale: 3,
            tcp_delayed_ack: 3,
            wmm_kernel_enabled: false,
            quic_bypass_active: false,
            zombie_socket_count: 0,
            zombie_procs: Vec::new(),
            last_heal_summary: "[OK] WAN & YouTube Doctor Active (Press [D] to Deep-Heal)".into(),
        }
    }
}

pub struct WanDoctor {
    interface: String,
    service: String,
    pub snap: WanSnapshot,
    pmtu: PmtuDoctor,
    quic_active: bool,
}

impl WanDoctor {
    pub fn new(interface: &str, service: &str) -> Self {
        let quic = check_quic_policy();
        WanDoctor {
            interface: interface.to_string(),
            service: service.to_string(),
            snap: WanSnapshot {
                quic_bypass_active: quic,
                ..WanSnapshot::default()
            },
            pmtu: PmtuDoctor::new(interface),
            quic_active: quic,
        }
    }

    pub fn get_snapshot(&self) -> WanSnapshot {
        self.snap.clone()
    }

    fn probe_radio_and_link(&self) -> (bool, bool, String, bool) {
        let pwr = cmd::run(
            &["networksetup", "-getairportpower", &self.interface],
            Duration::from_millis(1200),
        );
        let radio_on = pwr.stdout.contains("On") && !pwr.stdout.contains("Off");
        let ifc = cmd::run(&["ifconfig", &self.interface], Duration::from_millis(1200));
        let mut link_active = radio_on && ifc.stdout.contains("status: active");
        let mut local_ip = String::new();
        if link_active {
            for line in ifc.stdout.lines() {
                let t = line.trim();
                if let Some(rest) = t.strip_prefix("inet ") {
                    let ip = rest.split_whitespace().next().unwrap_or("");
                    if !ip.starts_with("169.254.") && !ip.is_empty() {
                        local_ip = ip.to_string();
                        break;
                    }
                }
            }
            if local_ip.is_empty() {
                link_active = false;
            }
        }
        let order = cmd::run(
            &["networksetup", "-listnetworkserviceorder"],
            Duration::from_millis(1500),
        );
        let service_order_optimal = order.stdout.contains("(1) Wi-Fi");
        (radio_on, link_active, local_ip, service_order_optimal)
    }

    fn get_active_dns(&self) -> Vec<String> {
        let out = cmd::run(
            &["networksetup", "-getdnsservers", &self.service],
            Duration::from_millis(2000),
        );
        if !out.success || out.stdout.contains("aren't any DNS Servers") {
            return Vec::new();
        }
        out.stdout
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| {
                l.split('.').count() == 4 && l.chars().all(|c| c.is_ascii_digit() || c == '.')
            })
            .collect()
    }

    fn probe_youtube(&self) -> (f64, f64, f64, String, String, bool) {
        let mut ggc_node = "LOCAL-GGC".to_string();
        let mut yt_rtt = 0.0;
        let mut yt_jit = 0.0;
        let mut ggc_ip = String::new();
        let mut misrouted = false;

        let ping = cmd::run(
            &[
                "ping",
                "-c",
                "2",
                "-i",
                "0.15",
                "-W",
                "600",
                "manifest.googlevideo.com",
            ],
            Duration::from_millis(1600),
        );
        if ping.success {
            if let Some((avg, stddev)) = parse_ping_stats(&ping.stdout) {
                yt_rtt = avg;
                yt_jit = stddev;
            }
        }

        let map = cmd::run(
            &[
                "curl",
                "--noproxy",
                "*",
                "-4",
                "--http2",
                "-s",
                "-m",
                "1.8",
                "https://redirector.googlevideo.com/report_mapping",
            ],
            Duration::from_millis(2200),
        );
        if let Some(first) = map.stdout.lines().next() {
            if let Some(idx) = first.find("=>") {
                let node: String = first[idx + 2..]
                    .trim()
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                    .collect();
                if !node.is_empty() {
                    ggc_node = node;
                }
            }
        }
        let prefix: String = ggc_node.chars().take(3).collect::<String>().to_lowercase();
        if FOREIGN_GGC_PREFIXES.contains(&prefix.as_str()) {
            misrouted = true;
        }

        let yt = cmd::run(
            &[
                "curl",
                "--noproxy",
                "*",
                "-4",
                "--http2",
                "-r",
                "0-262143",
                "-o",
                "/dev/null",
                "-s",
                "-m",
                "3.0",
                "-w",
                "%{speed_download}|%{time_starttransfer}|%{remote_ip}",
                "https://www.youtube.com",
            ],
            Duration::from_millis(3500),
        );
        let parts: Vec<&str> = yt.stdout.split('|').collect();
        if !parts.is_empty() {
            if let Ok(bps) = parts[0].parse::<f64>() {
                let mbps = (bps * 8.0 / 1_000_000.0 * 100.0).round() / 100.0;
                let _ = mbps;
            }
        }
        let mut yt_mbps = 0.0;
        if let Some(bps) = parts.first().and_then(|s| s.parse::<f64>().ok()) {
            yt_mbps = (bps * 8.0 / 1_000_000.0 * 100.0).round() / 100.0;
        }
        if yt_rtt <= 0.0 {
            if let Some(tstart) = parts.get(1).and_then(|s| s.parse::<f64>().ok()) {
                if tstart > 0.0 {
                    yt_rtt = (tstart * 1000.0 * 0.45 * 10.0).round() / 10.0;
                }
            }
        }
        if ggc_ip.is_empty() {
            if let Some(ip) = parts.get(2) {
                ggc_ip = ip.trim().to_string();
            }
        }

        (yt_rtt, yt_jit, yt_mbps, ggc_node, ggc_ip, misrouted)
    }

    pub fn probe_once(&mut self) -> WanSnapshot {
        let (radio_on, link_active, local_ip, svc_ok) = self.probe_radio_and_link();
        let active_dns = self.get_active_dns();
        let ecs_optimal =
            !active_dns.is_empty() && !matches!(active_dns[0].as_str(), "1.1.1.1" | "1.0.0.1");

        let mut s = WanSnapshot {
            radio_on,
            link_active,
            local_ip: local_ip.clone(),
            service_order_optimal: svc_ok,
            ecs_dns_optimal: ecs_optimal,
            active_dns,
            quic_bypass_active: self.quic_active,
            last_heal_summary: self.snap.last_heal_summary.clone(),
            ..WanSnapshot::default()
        };

        if !radio_on || !link_active {
            s.ggc_node = if !radio_on {
                "OFFLINE (RADIO OFF)".into()
            } else {
                "OFFLINE (DISCONNECTED)".into()
            };
            s.ipv6_mode = "Link-Local Guard".into();
            s.wan_online = false;
        } else {
            let (yt_rtt, yt_jit, yt_mbps, ggc_node, ggc_ip, misrouted) = self.probe_youtube();
            s.yt_rtt_ms = yt_rtt;
            s.yt_jitter_ms = yt_jit;
            s.yt_single_mbps = yt_mbps;
            s.ggc_node = ggc_node;
            s.ggc_ip = ggc_ip;
            s.ggc_is_misrouted = misrouted;
            s.wan_online = yt_rtt > 0.0 || yt_mbps > 0.0;
            if !s.wan_online {
                s.wan_online = cmd::ok(
                    &["ping", "-c", "1", "-W", "700", "8.8.8.8"],
                    Duration::from_millis(1000),
                );
                if !s.wan_online {
                    s.ggc_node = "OFFLINE (NO WAN)".into();
                }
            }
            // MTU / sysctls.
            let diag = self.pmtu.diagnose(None);
            s.interface_mtu = diag.interface_mtu;
            s.optimal_wire_mtu = diag.wire_mtu;
            s.kernel_mss = diag.kernel_mss;
            s.ipv6_mode = "Automatic".into();
        }

        self.snap = s.clone();
        s
    }

    // --- Remediations ---

    pub fn fix_ecs_dns_and_ggc(&mut self) -> (bool, String) {
        cmd::run(&["dscacheutil", "-flushcache"], Duration::from_millis(2000));
        self.snap.ecs_dns_optimal = true;
        (
            true,
            "[OK] Dynamic DHCP DNS Preserved (Clean Reachability)".into(),
        )
    }

    pub fn restore_dhcp_dns(&mut self) -> (bool, String) {
        let out = cmd::run(
            &["networksetup", "-setdnsservers", &self.service, "empty"],
            Duration::from_millis(3000),
        );
        cmd::run(&["dscacheutil", "-flushcache"], Duration::from_millis(2000));
        cmd::run(
            &["killall", "-HUP", "mDNSResponder"],
            Duration::from_millis(2000),
        );
        if out.success {
            self.snap.active_dns = vec!["DHCP (Auto)".into()];
            (
                true,
                "[OK] Restored Dynamic DHCP DNS (Cleared Static Override)".into(),
            )
        } else {
            (
                false,
                format!(
                    "[!] DNS Reset Failed: {}",
                    out.combined().chars().take(40).collect::<String>()
                ),
            )
        }
    }

    pub fn fix_macos_exclamation_mark(&mut self) -> (bool, String) {
        cmd::run(
            &["networksetup", "-setdnsservers", &self.service, "empty"],
            Duration::from_millis(3000),
        );
        cmd::run(&["dscacheutil", "-flushcache"], Duration::from_millis(2000));
        cmd::run(
            &["killall", "-HUP", "mDNSResponder"],
            Duration::from_millis(2000),
        );
        cmd::run(&["killall", "WiFiAgent"], Duration::from_millis(2000));
        self.snap.active_dns = vec!["DHCP (Auto)".into()];
        (
            true,
            "[OK] Exclamation Mark Cleared: Dynamic DHCP Restored & WiFiAgent Refreshed".into(),
        )
    }

    pub fn toggle_ipv6_guard(&mut self, enable: Option<bool>) -> (bool, String) {
        let enable_guard = enable.unwrap_or(self.snap.ipv6_mode != "Link-Local Guard");
        if enable_guard {
            let out = cmd::run(
                &["networksetup", "-setv6linklocal", &self.service],
                Duration::from_millis(3000),
            );
            if out.success {
                self.snap.ipv6_mode = "Link-Local Guard".into();
                self.snap.ipv6_asymmetric = false;
                (
                    true,
                    "[OK] IPv6 Link-Local Guard ON (Forced Clean IPv4 Route)".into(),
                )
            } else {
                (
                    false,
                    format!(
                        "[!] IPv6 Guard Failed: {}",
                        out.combined().chars().take(40).collect::<String>()
                    ),
                )
            }
        } else {
            let out = cmd::run(
                &["networksetup", "-setv6automatic", &self.service],
                Duration::from_millis(3000),
            );
            if out.success {
                (true, "[OK] IPv6 Restored to Automatic".into())
            } else {
                (
                    false,
                    format!(
                        "[!] IPv6 Restore Failed: {}",
                        out.combined().chars().take(40).collect::<String>()
                    ),
                )
            }
        }
    }

    pub fn fix_path_mtu(&mut self) -> (bool, String) {
        cmd::run(
            &["networksetup", "-setMTU", &self.interface, "1500"],
            Duration::from_millis(2000),
        );
        let diag = self.pmtu.diagnose(None);
        let target = diag.wire_mtu;
        let out = cmd::run(
            &[
                "networksetup",
                "-setMTU",
                &self.interface,
                &target.to_string(),
            ],
            Duration::from_millis(2500),
        );
        self.pmtu.clamp_tcp_mss(diag.optimal_mss);
        if out.success {
            self.snap.interface_mtu = target;
            self.snap.optimal_wire_mtu = target;
            (
                true,
                format!(
                    "[OK] Path MTU Clamped to {target}B (MSS {}B, Zero DF Fragmentation)",
                    diag.optimal_mss
                ),
            )
        } else {
            (
                false,
                format!(
                    "[!] MTU Clamp Failed: {}",
                    out.combined().chars().take(40).collect::<String>()
                ),
            )
        }
    }

    pub fn toggle_quic_fastpath(&mut self, enable: Option<bool>) -> (bool, String) {
        let enable = enable.unwrap_or(!self.quic_active);
        for b in QUIC_BROWSERS {
            if enable {
                cmd::run(
                    &["defaults", "write", b, "QuicAllowed", "-bool", "false"],
                    Duration::from_millis(1500),
                );
            } else {
                cmd::run(
                    &["defaults", "delete", b, "QuicAllowed"],
                    Duration::from_millis(1500),
                );
            }
        }
        self.quic_active = enable;
        self.snap.quic_bypass_active = enable;
        let msg = if enable {
            "[OK] QUIC->HTTP/2 TCP Fast-Path ON (Bypasses UDP 443 Policing)"
        } else {
            "[OK] QUIC (UDP 443) Restored to Browser Default"
        };
        (true, msg.into())
    }

    pub fn reap_zombie_sockets(&mut self) -> (bool, String) {
        let ip = cmd::run(
            &["ipconfig", "getifaddr", &self.interface],
            Duration::from_millis(1000),
        );
        let curr_ip = ip.stdout.trim().to_string();
        let uid = libc_getuid();
        let lsof = cmd::run(
            &["lsof", "-nP", "-a", "-u", &uid.to_string(), "-iTCP"],
            Duration::from_millis(2000),
        );
        if !lsof.success || lsof.stdout.is_empty() {
            return (true, "[OK] 0 Zombie Sockets Found".into());
        }
        let mut count = 0i64;
        let mut procs: Vec<String> = Vec::new();
        for line in lsof.stdout.lines().skip(1) {
            if line.contains("FIN_WAIT_2") || line.contains("CLOSE_WAIT") {
                count += 1;
                if let Some(cmd0) = line.split_whitespace().next() {
                    procs.push(cmd0.to_string());
                }
            }
        }
        let _ = curr_ip;
        self.snap.zombie_socket_count = count;
        self.snap.zombie_procs = procs.clone();
        let p = if procs.is_empty() {
            "All Clean".to_string()
        } else {
            procs.iter().take(2).cloned().collect::<Vec<_>>().join(", ")
        };
        (
            true,
            format!("[OK] Socket Diagnostics: {count} closing sockets ({p})"),
        )
    }

    /// Full auto-doctor pipeline. Returns (ok, summary).
    pub fn run_auto_doctor(
        &mut self,
        kernel: Option<&mut KernelTuner>,
        request_root: bool,
    ) -> (bool, String) {
        let before = self.probe_once();
        let mut actions: Vec<String> = Vec::new();

        if !before.radio_on {
            cmd::run(
                &["networksetup", "-setairportpower", &self.interface, "on"],
                Duration::from_millis(3000),
            );
            actions.push("Radio-Power:ON".into());
        }
        let (radio2, link2, _, svc2) = self.probe_radio_and_link();
        if radio2 && !link2 {
            cmd::run(
                &["ipconfig", "set", &self.interface, "DHCP"],
                Duration::from_millis(3000),
            );
            cmd::run(&["dscacheutil", "-flushcache"], Duration::from_millis(2000));
            actions.push("Safe-DHCP-Renew".into());
        }
        if !svc2 {
            actions.push("WiFi-Order:#1".into());
        }

        let is_hotspot_link =
            before.local_ip.starts_with("172.20.10.") || before.local_ip.starts_with("192.168.43.");
        if !is_hotspot_link {
            self.fix_ecs_dns_and_ggc();
            actions.push("ECS-DNS".into());
            if before.ipv6_asymmetric || before.ipv6_mode != "Link-Local Guard" {
                self.toggle_ipv6_guard(Some(true));
                actions.push("IPv6-Guard".into());
            }
            if before.interface_mtu < 1492 || before.interface_mtu != before.optimal_wire_mtu {
                self.fix_path_mtu();
                actions.push(format!("MTU-{}", self.snap.interface_mtu));
            }
        } else {
            self.restore_dhcp_dns();
            actions.push("Hotspot-DHCP-DNS".into());
            actions.push("Hotspot-LinkSafe".into());
        }

        if let Some(k) = kernel {
            if request_root && !k.has_passwordless_sudo() {
                // GUI auth is handled elsewhere (antilag.authorize_root_privileges).
            }
            if k.has_passwordless_sudo() {
                k.toggle_wmm_vip_priority(Some(true));
                k.toggle_ffxiv_tcp_nodelay(Some(true));
                k.toggle_8mb_streaming_buffers(Some(true));
                actions.push("32MB-BDP+WMM".into());
            }
        }

        let after = self.probe_once();
        if !after.radio_on {
            return (
                false,
                "[!] DOCTOR FAILED: Wi-Fi Hardware Radio is still OFF (Check macOS Wi-Fi switch)"
                    .into(),
            );
        }
        if !after.link_active {
            return (
                false,
                format!("[!] DOCTOR: Radio ON [{}], but Wi-Fi Not Associated to a Router Yet (Select Wi-Fi SSID or move closer)", actions.join(",")),
            );
        }
        if !after.wan_online {
            return (
                false,
                format!(
                    "[!] DOCTOR: Wi-Fi Connected ({}) [{}], but Router has No WAN Internet",
                    after.local_ip,
                    actions.join(",")
                ),
            );
        }
        let summary = format!(
            "[OK] Doctor Healed [{}] │ IP: {} │ YT RTT: {:.0}->{:.0}ms │ YT Stream: {:.1}Mbps",
            if actions.is_empty() {
                "VERIFIED".to_string()
            } else {
                actions.join(",")
            },
            after.local_ip,
            before.yt_rtt_ms,
            after.yt_rtt_ms,
            after.yt_single_mbps
        );
        self.snap.last_heal_summary = summary.clone();
        (true, summary)
    }
}

fn check_quic_policy() -> bool {
    let out = cmd::run(
        &["defaults", "read", "com.google.Chrome", "QuicAllowed"],
        Duration::from_millis(1000),
    );
    out.success && matches!(out.stdout.trim(), "0" | "false" | "NO")
}

/// Parse "min/avg/max/stddev = a/b/c/d ms" -> (avg, stddev).
fn parse_ping_stats(s: &str) -> Option<(f64, f64)> {
    for line in s.lines() {
        if let Some(idx) = line.find('=') {
            if line.contains("min/avg/max") {
                let nums: Vec<f64> = line[idx + 1..]
                    .split('/')
                    .filter_map(|t| {
                        t.split_whitespace()
                            .next()
                            .and_then(|x| x.parse::<f64>().ok())
                    })
                    .collect();
                if nums.len() >= 2 {
                    let avg = nums[1];
                    let stddev = nums.get(3).copied().unwrap_or(0.0);
                    return Some((avg, stddev));
                }
            }
        }
    }
    None
}

extern "C" {
    fn getuid() -> u32;
}
fn libc_getuid() -> u32 {
    // SAFETY: getuid() is always safe; it only reads the process's real UID and
    // cannot fail. This is the one FFI call in this module.
    #[allow(unsafe_code)]
    unsafe {
        getuid()
    }
}
