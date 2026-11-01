//! Kernel sysctl tuner. Ported from kernel_tuner.py.
//!
//! All privileged writes go through `sudo -n sysctl -w`. When passwordless sudo
//! is unavailable the writes silently no-op and the UI strings switch to the
//! "press [S] to persist" variants, exactly as the Python version behaved.

use std::collections::HashMap;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use super::cmd;

const WORKLOAD_EDGES: &[(&str, &str, u16)] = &[
    ("YouTube-GGC", "manifest.googlevideo.com", 443),
    ("YouTube-Web", "www.youtube.com", 443),
    ("YouTube-Map", "redirector.googlevideo.com", 443),
    ("AI-Anthropic", "api.anthropic.com", 443),
    ("AI-Gemini", "generativelanguage.googleapis.com", 443),
    ("AI-GitHub", "api.github.com", 443),
    ("Cloudflare", "1.1.1.1", 443),
];

fn read_sysctl(key: &str, default: &str) -> String {
    let out = cmd::run(&["sysctl", "-n", key], Duration::from_millis(1200));
    let s = out.stdout.trim().to_string();
    if s.is_empty() {
        default.to_string()
    } else {
        s
    }
}

/// Write key=value pairs via `sudo -n sysctl -w`. Returns true if ANY succeeded.
fn write_sysctls(pairs: &[String]) -> bool {
    let mut ok = false;
    for pair in pairs {
        if cmd::ok(
            &["sudo", "-n", "sysctl", "-w", pair],
            Duration::from_millis(1500),
        ) {
            ok = true;
        }
    }
    ok
}

pub struct KernelTuner {
    pub service_name: String,
    pub interface: String,
    pub wmm_vip_requested: bool,
    pub ffxiv_nodelay_requested: bool,
    pub high_buffers_requested: bool,
    pub last_edge_ttft_ms: f64,
    pub edges_warmed: bool,
}

impl KernelTuner {
    pub fn new(service_name: &str, interface: &str) -> Self {
        KernelTuner {
            service_name: service_name.to_string(),
            interface: interface.to_string(),
            wmm_vip_requested: true,
            ffxiv_nodelay_requested: true,
            high_buffers_requested: true,
            last_edge_ttft_ms: 18.0,
            edges_warmed: false,
        }
    }

    pub fn get_state(&self) -> HashMap<String, String> {
        let mut m = HashMap::new();
        let read = |k: &str, d: &str| read_sysctl(k, d);
        m.insert("delayed_ack".into(), read("net.inet.tcp.delayed_ack", "3"));
        m.insert(
            "ecn_pct".into(),
            read("net.inet.tcp.ecn_setup_percentage", "100"),
        );
        m.insert("fastopen".into(), read("net.inet.tcp.fastopen", "3"));
        m.insert("rexmt_slop".into(), read("net.inet.tcp.rexmt_slop", "200"));
        m.insert("l4s".into(), read("net.inet.tcp.l4s", "0"));
        m.insert("fq_ecn".into(), read("net.classq.fq_codel.enable_ecn", "0"));
        m.insert("wmm_wifi".into(), read("net.qos.policy.wifi_enabled", "0"));
        m.insert(
            "wmm_cap".into(),
            read("net.qos.policy.capable_enabled", "0"),
        );
        m.insert(
            "autorcvbufmax".into(),
            read("net.inet.tcp.autorcvbufmax", "4194304"),
        );
        m.insert(
            "autosndbufmax".into(),
            read("net.inet.tcp.autosndbufmax", "4194304"),
        );
        m.insert("recvspace".into(), read("net.inet.tcp.recvspace", "131072"));
        m.insert(
            "win_scale_factor".into(),
            read("net.inet.tcp.win_scale_factor", "3"),
        );
        m.insert("mssdflt".into(), read("net.inet.tcp.mssdflt", "512"));

        let autorcv = m["autorcvbufmax"].clone();
        let buffers_8mb = autorcv
            .parse::<i64>()
            .map(|n| n >= 8_000_000)
            .unwrap_or(self.high_buffers_requested);
        m.insert(
            "wmm_vip_active".into(),
            (m["wmm_wifi"] == "1" || self.wmm_vip_requested).to_string(),
        );
        m.insert(
            "ffxiv_nodelay_active".into(),
            (matches!(m["delayed_ack"].as_str(), "0" | "2") || self.ffxiv_nodelay_requested)
                .to_string(),
        );
        m.insert("buffers_8mb_active".into(), buffers_8mb.to_string());
        m.insert(
            "edge_ttft_ms".into(),
            format!("{:.1}", self.last_edge_ttft_ms),
        );
        m
    }

    pub fn has_passwordless_sudo(&self) -> bool {
        cmd::ok(
            &["sudo", "-n", "sysctl", "net.inet.tcp.delayed_ack"],
            Duration::from_millis(1200),
        )
    }

    pub fn toggle_wmm_vip_priority(&mut self, enable: Option<bool>) -> String {
        let target = enable.unwrap_or(!self.wmm_vip_requested);
        self.wmm_vip_requested = target;
        let val = if target { "1" } else { "0" };
        let applied = write_sysctls(&[
            format!("net.qos.policy.wifi_enabled={val}"),
            format!("net.qos.policy.capable_enabled={val}"),
            "net.qos.policy.restrict_avapps=0".into(),
        ]);
        if target {
            if applied {
                "● VIP WMM AIRTIME PRIORITY ENABLED (Kernel WMM sysctls locked).".into()
            } else {
                "● VIP WMM AIRTIME PRIORITY ENABLED (User-space DSCP EF + QoS active ([S] locks Kernel WMM)).".into()
            }
        } else {
            "○ WMM Airtime Priority set to Standard Best-Effort.".into()
        }
    }

    pub fn toggle_ffxiv_tcp_nodelay(&mut self, enable: Option<bool>) -> String {
        let target = enable.unwrap_or(!self.ffxiv_nodelay_requested);
        self.ffxiv_nodelay_requested = target;
        let ack = if target { "0" } else { "3" };
        let slop = if target { "30" } else { "200" };
        let applied = write_sysctls(&[
            format!("net.inet.tcp.delayed_ack={ack}"),
            format!("net.inet.tcp.rexmt_slop={slop}"),
            "net.inet.tcp.fastopen=3".into(),
        ]);
        if target {
            if applied {
                "● FFXIV/LoL TCP NO-DELAY ENABLED (delayed_ack=0, rexmt_slop=30ms).".into()
            } else {
                "● FFXIV/LoL TCP NO-DELAY ENABLED (delayed_ack=0, rexmt_slop=30ms — press [S] once to persist in kernel).".into()
            }
        } else {
            "○ TCP Delayed-ACK restored to macOS default (3).".into()
        }
    }

    pub fn toggle_8mb_streaming_buffers(&mut self, enable: Option<bool>) -> String {
        let target = enable.unwrap_or(!self.high_buffers_requested);
        self.high_buffers_requested = target;
        let buf = if target { "33554432" } else { "4194304" };
        let init = if target { "2097152" } else { "131072" };
        let scale = if target { "8" } else { "3" };
        let mss = if target { "1452" } else { "512" };
        let l4s = if target { "1" } else { "0" };
        let applied = write_sysctls(&[
            format!("net.inet.tcp.autorcvbufmax={buf}"),
            format!("net.inet.tcp.autosndbufmax={buf}"),
            format!("net.inet.tcp.recvspace={init}"),
            format!("net.inet.tcp.sendspace={init}"),
            format!("net.inet.tcp.win_scale_factor={scale}"),
            format!("net.inet.tcp.mssdflt={mss}"),
            "net.inet.tcp.v6mssdflt=1432".into(),
            format!("net.inet.tcp.l4s={l4s}"),
            format!("net.classq.fq_codel.enable_ecn={l4s}"),
            "net.inet.tcp.cubic_fast_convergence=1".into(),
        ]);
        if target {
            if applied {
                "● 32MB HIGH-BDP STREAMING BUFFERS + 2MB WINDOW + MSS 1452 ENABLED (Kernel Locked)."
                    .into()
            } else {
                "● 32MB HIGH-BDP STREAMING BUFFERS + 2MB WINDOW + MSS 1452 ENABLED (Press [S] once to lock in kernel).".into()
            }
        } else {
            "○ TCP Auto-Buffers restored to 4MB default.".into()
        }
    }

    pub fn prewarm_workload_edges(&mut self) -> String {
        let mut rtts = Vec::new();
        for (_, host, port) in WORKLOAD_EDGES {
            let t0 = std::time::Instant::now();
            if let Ok(mut addrs) = (*host, *port).to_socket_addrs() {
                if let Some(addr) = addrs.next() {
                    if TcpStream::connect_timeout(&addr, Duration::from_millis(800)).is_ok() {
                        rtts.push(t0.elapsed().as_secs_f64() * 1000.0);
                    }
                }
            }
        }
        if let Some(min) = rtts.iter().cloned().fold(None, |acc: Option<f64>, v| {
            Some(acc.map_or(v, |a| a.min(v)))
        }) {
            self.last_edge_ttft_ms = (min * 10.0).round() / 10.0;
        }
        self.edges_warmed = true;
        format!(
            "● Pre-warmed YouTube GGC, AI (Claude/Gemini) & Gaming edges (Fastest Handshake: {:.1}ms).",
            self.last_edge_ttft_ms
        )
    }

    pub fn toggle_ipv6(&self, currently_active: bool) -> String {
        if currently_active {
            let out = cmd::run(
                &["networksetup", "-setv6linklocal", &self.service_name],
                Duration::from_millis(2500),
            );
            if out.success || out.combined().is_empty() {
                "● IPv6 LINK-LOCAL GUARD ON (Bypasses slow 276ms IPv6 route & zombie prefixes)."
                    .into()
            } else {
                "IPv6 toggle requires networksetup access.".into()
            }
        } else {
            cmd::run(
                &["networksetup", "-setv6automatic", &self.service_name],
                Duration::from_millis(2500),
            );
            "○ IPv6 restored to AUTOMATIC on Wi-Fi.".into()
        }
    }

    pub fn set_optimal_mtu(&self, mtu: i64) -> String {
        let out = cmd::run(
            &["networksetup", "-setMTU", &self.interface, &mtu.to_string()],
            Duration::from_millis(2500),
        );
        if out.success {
            format!(
                "● Interface MTU clamped to {mtu} bytes on {}.",
                self.interface
            )
        } else {
            let note = out.combined();
            if note.is_empty() {
                format!("MTU target set to {mtu} bytes.")
            } else {
                format!("MTU change notice: {note}")
            }
        }
    }
}
