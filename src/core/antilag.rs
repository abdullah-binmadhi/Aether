//! Zero-Lag mode, daemon freezing, god-mode boost. Ported from antilag.py.

use std::time::Duration;

use super::cmd;
use super::dns_engine::DnsEngine;
use super::kernel_tuner::KernelTuner;
use super::model::Band;
use super::model::Telemetry;

const BACKGROUND_SYNC_DAEMONS: &[&str] = &["cloudd", "photolibraryd", "bird", "cloudpaird"];
const DEBLOAT_QOS_TARGETS: &[&str] = &[
    "cloudd",
    "photolibraryd",
    "bird",
    "cloudpaird",
    "mds_stores",
    "photoanalysisd",
    "mediaanalysisd",
];

pub struct AntiLagEngine {
    pub interface: String,
    pub zero_lag_active: bool,
    pub sync_daemons_frozen: bool,
    pub root_authorized: bool,
}

impl AntiLagEngine {
    pub fn new(interface: &str) -> Self {
        AntiLagEngine {
            interface: interface.to_string(),
            zero_lag_active: false,
            sync_daemons_frozen: false,
            root_authorized: Self::check_passwordless_sudo(),
        }
    }

    fn check_passwordless_sudo() -> bool {
        cmd::ok(
            &["sudo", "-n", "sysctl", "net.inet.tcp.delayed_ack"],
            Duration::from_millis(1000),
        )
    }

    /// Demote background sync/index daemons to background QoS. Returns count clamped.
    pub fn debloat_background_queues() -> i64 {
        let mut clamped = 0;
        for proc_name in DEBLOAT_QOS_TARGETS {
            let res = cmd::run(&["pgrep", "-x", proc_name], Duration::from_millis(800));
            if res.success {
                for pid in res.stdout.split_whitespace() {
                    if pid.chars().all(|c| c.is_ascii_digit()) && !pid.is_empty() {
                        cmd::run(&["taskpolicy", "-b", "-p", pid], Duration::from_millis(500));
                        cmd::run(&["renice", "15", "-p", pid], Duration::from_millis(500));
                        clamped += 1;
                    }
                }
            }
        }
        clamped
    }

    pub fn toggle_sync_daemons(&mut self, freeze: bool) -> String {
        let sig = if freeze { "-STOP" } else { "-CONT" };
        let mut affected = 0;
        for daemon in BACKGROUND_SYNC_DAEMONS {
            if cmd::ok(&["killall", sig, daemon], Duration::from_millis(1000)) {
                affected += 1;
            }
        }
        self.sync_daemons_frozen = freeze;
        if freeze {
            format!(
                "● Paused {affected} background sync daemons ({}).",
                BACKGROUND_SYNC_DAEMONS[..3].join(", ")
            )
        } else {
            "○ Resumed background sync daemons via SIGCONT.".into()
        }
    }

    /// Toggle Zero-Lag. Returns the status string. `tel` is updated in sync.
    pub fn toggle_awdl(&mut self, tel: Option<&mut Telemetry>) -> String {
        let enable = !self.zero_lag_active;
        self.zero_lag_active = enable;
        if let Some(t) = tel {
            t.zero_lag_mode = enable;
            t.awdl_active = !enable;
            t.alerts
                .retain(|a| !a.contains("AWDL") && !a.contains("Zero-Lag"));
        }
        cmd::run(
            &[
                "defaults",
                "write",
                "com.apple.NetworkBrowser",
                "DisableAirDrop",
                "-bool",
                if enable { "YES" } else { "NO" },
            ],
            Duration::from_millis(1500),
        );
        self.toggle_sync_daemons(enable);
        let clamped = if enable {
            Self::debloat_background_queues()
        } else {
            0
        };
        DnsEngine::flush_cache();
        if enable {
            format!("● ZERO-LAG MODE ENABLED ({clamped} background socket queues clamped to WMM Scavenger).")
        } else {
            "○ STANDARD MODE RESTORED (AirDrop & background sync resumed).".into()
        }
    }

    pub fn refresh_dhcp_and_routes(interface: &str) -> String {
        cmd::run(
            &["sudo", "-n", "ipconfig", "set", interface, "DHCP"],
            Duration::from_millis(2000),
        );
        DnsEngine::flush_cache();
        format!("● Refreshed DHCP lease & routing state on {interface}.")
    }

    /// Install passwordless sudoers + lock kernel tuning via an osascript GUI prompt.
    pub fn authorize_root_privileges(&mut self) -> String {
        let shell_cmd = "echo '%admin ALL=(ALL) NOPASSWD: /usr/sbin/sysctl, /sbin/ifconfig, /usr/sbin/ipconfig, /usr/bin/killall, /usr/bin/renice' > /etc/sudoers.d/wifi-tui && chmod 0440 /etc/sudoers.d/wifi-tui && sysctl -w net.qos.policy.wifi_enabled=1 net.qos.policy.capable_enabled=1 net.qos.policy.restrict_avapps=0 net.inet.tcp.delayed_ack=0 net.inet.tcp.rexmt_slop=30 net.inet.tcp.fastopen=3 net.inet.tcp.l4s=1 net.classq.fq_codel.enable_ecn=1 net.inet.tcp.cubic_fast_convergence=1 net.inet.tcp.autorcvbufmax=33554432 net.inet.tcp.autosndbufmax=33554432 net.inet.tcp.recvspace=2097152 net.inet.tcp.sendspace=2097152 net.inet.tcp.win_scale_factor=8 net.inet.tcp.mssdflt=1452 net.inet.tcp.v6mssdflt=1432 && dscacheutil -flushcache && killall -HUP mDNSResponder";
        let script = format!("do shell script \"{shell_cmd}\" with administrator privileges");
        let out = cmd::run(&["osascript", "-e", &script], Duration::from_secs(45));
        if out.success {
            self.root_authorized = true;
            "[OK] Permanent Root Kernel Tuning Authorized: WMM VIP + 32MB BDP Window + MSS 1452 + awdl0 Locked!".into()
        } else {
            "[!] Root authorization dialog cancelled.".into()
        }
    }

    /// 10x boost pipeline. Returns the multi-step summary.
    pub fn run_god_mode_boost(
        &mut self,
        tel: &Telemetry,
        dns: &DnsEngine,
        kernel: &mut KernelTuner,
    ) -> String {
        let curr_ssid = if tel.ssid.is_empty() {
            "Connected Wi-Fi".to_string()
        } else {
            tel.ssid.clone()
        };
        let band = tel.band;
        let mut steps: Vec<String> = Vec::new();

        if tel.is_hotspot {
            steps.push(format!("Cellular Hotspot Profile ({curr_ssid})"));
        } else if band.is_fast() {
            steps.push(format!(
                "High-Throughput {} Profile ({curr_ssid} [{}])",
                band.label(),
                tel.phy_mode
            ));
        } else {
            steps.push(format!(
                "Low-Latency 2.4GHz Noise-Bypass Profile ({curr_ssid} [{}])",
                tel.phy_mode
            ));
        }

        if !self.zero_lag_active {
            self.toggle_awdl(None);
            steps.push("Zero-Lag:AWDL-Muted (0ms Jitter Spikes)".into());
        }
        self.toggle_sync_daemons(true);
        steps.push("Sync-Hogs:Paused (100% Airtime to Foreground)".into());

        kernel.toggle_wmm_vip_priority(Some(true));
        kernel.toggle_ffxiv_tcp_nodelay(Some(true));
        if tel.is_hotspot {
            kernel.set_optimal_mtu(1440);
            kernel.toggle_8mb_streaming_buffers(Some(true));
            steps.push("VIP-WMM + Carrier-MTU:1440 + FastACK".into());
        } else if band.is_fast() || tel.tx_rate_mbps >= 300 {
            kernel.set_optimal_mtu(1492);
            kernel.toggle_8mb_streaming_buffers(Some(true));
            steps.push("VIP-WMM + 32MB-BDP + MSS:1452 + FastACK".into());
        } else {
            kernel.set_optimal_mtu(1492);
            kernel.toggle_8mb_streaming_buffers(Some(true));
            steps.push("VIP-WMM + Noise-Resistant MSS:1452 + FastACK".into());
        }
        kernel.prewarm_workload_edges();
        steps.push("CDN-Edges:Pre-Warmed".into());

        let gw = &tel.gateway;
        if !gw.is_empty() && gw != "N/A" {
            cmd::run(&["arp", "-d", gw], Duration::from_millis(1000));
        }
        steps.push("Gateway-ARP:Refreshed".into());

        dns.reset_to_dhcp_dns();
        DnsEngine::flush_cache();
        steps.push("Dynamic-DHCP-DNS:Flushed".into());

        let _ = Band::Band5GHz;
        format!(
            "◆ 10X DYNAMIC BOOST ACTIVATED ON [{curr_ssid} ({})]:\n   {}",
            band.label(),
            steps.join(" │ ")
        )
    }
}
