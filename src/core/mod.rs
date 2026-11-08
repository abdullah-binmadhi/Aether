//! Core engines: telemetry model, native collectors, and network diagnostics.

pub mod acoustic_radar;
pub mod airtime;
pub mod antilag;
pub mod bandwidth_shaper;
pub mod captive;
pub mod channel_advisor;
pub mod cmd;
pub mod collector;
pub mod corewlan;
pub mod daemon;
pub mod device_info;
pub mod disassoc;
pub mod dns_engine;
pub mod game_mode;
pub mod gaming_radar;
pub mod gateway;
pub mod health;
pub mod history;
pub mod hop2;
pub mod hops;
pub mod hotspot;
pub mod kernel_tuner;
pub mod lan_scanner;
pub mod maintenance;
pub mod model;
pub mod orchestrator;
pub mod ping;
pub mod pmtu;
pub mod power_hook;
pub mod preset_engine;
pub mod privacy_fortress;
pub mod privacy_shield;
pub mod procmon;
pub mod profiles;
pub mod rf_inference;
pub mod roaming;
pub mod router_bot;
pub mod security_guard;
pub mod shield;
pub mod socket_matrix;
pub mod spectrum;
pub mod speedtest;
pub mod ticket;
pub mod wan_doctor;
pub mod watchdog;
pub mod zero_jitter;

use std::net::{Ipv4Addr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use hops::{diagnose, HopTracker, ScopeSnapshot};

/// Rolling history length for sparklines.
const HISTORY: usize = 48;

/// Spawns a 25Hz (40ms interval) non-blocking UDP keep-alive pacer to the gateway
/// so the 802.11 radio remains in Constantly Awake Mode (CAM) without DTIM sleep spikes.
fn spawn_cam_25hz_pacer(gw_ip: &str) -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    let target = if gw_ip.is_empty() || gw_ip == "N/A" {
        "127.0.0.1:9".to_string()
    } else {
        format!("{gw_ip}:9")
    };
    let stop_clone = Arc::clone(&stop);
    std::thread::Builder::new()
        .name("aether-cam-25hz-pacer".into())
        .spawn(move || {
            let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else {
                return;
            };
            let _ = sock.set_nonblocking(true);
            let payload = b"AETHER-CAM-25HZ-KEEPALIVE";
            while !stop_clone.load(Ordering::Relaxed) {
                let _ = sock.send_to(payload, &target);
                std::thread::sleep(Duration::from_millis(40));
            }
        })
        .ok();
    stop
}

/// Runs the 4-hop ICMP matrix and tracks throughput history.
pub struct PingScope {
    #[allow(dead_code)] // retained for topology refresh on network change (future)
    pub interface: String,
    pub gateway_ip: String,
    pub isp_ip: String,
    pub cloud_ip: String,
    hop1: HopTracker,
    hop2: HopTracker,
    hop3: HopTracker,
    hop4: HopTracker,
    rtt_history: Vec<f64>,
    down_history: Vec<f64>,
    up_history: Vec<f64>,
    pub cam_pacer_active: bool,
    pub pacer_stop_flag: Option<Arc<AtomicBool>>,
    seq: u16,
    last_rx: u64,
    last_tx: u64,
    down_kbs: f64,
    up_kbs: f64,
}

impl Drop for PingScope {
    fn drop(&mut self) {
        if let Some(flag) = self.pacer_stop_flag.take() {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

impl PingScope {
    pub fn new(interface: &str) -> Self {
        let (gw, isp) = discover_hops(interface);
        let mut hop1 = HopTracker::new(1, "Mac en0", "Local");
        hop1.record(Some(0.1));
        let pacer_stop_flag = if !cfg!(test) {
            Some(spawn_cam_25hz_pacer(&gw))
        } else {
            None
        };
        PingScope {
            interface: interface.to_string(),
            hop2: HopTracker::new(2, "Gateway", &gw),
            hop3: HopTracker::new(
                3,
                if isp == "1.1.1.1" {
                    "Cloudflare"
                } else {
                    "ISP Edge"
                },
                &isp,
            ),
            hop4: HopTracker::new(4, "Google Edge", "8.8.8.8"),
            hop1,
            gateway_ip: gw,
            isp_ip: isp,
            cloud_ip: "8.8.8.8".into(),
            rtt_history: Vec::new(),
            down_history: vec![0.0; HISTORY],
            up_history: vec![0.0; HISTORY],
            cam_pacer_active: true,
            pacer_stop_flag,
            seq: 0,
            last_rx: 0,
            last_tx: 0,
            down_kbs: 0.0,
            up_kbs: 0.0,
        }
    }

    pub fn toggle_cam_pacer(&mut self) -> String {
        self.cam_pacer_active = !self.cam_pacer_active;
        if let Some(flag) = self.pacer_stop_flag.take() {
            flag.store(true, Ordering::Relaxed);
        }
        if self.cam_pacer_active {
            self.pacer_stop_flag = Some(spawn_cam_25hz_pacer(&self.gateway_ip));
            format!(
                "CAM 25Hz Anti-Sleep Pacer ENABLED (40ms UDP keep-alive -> {}).",
                self.gateway_ip
            )
        } else {
            "CAM 25Hz Anti-Sleep Pacer PAUSED.".into()
        }
    }

    fn update_throughput(&mut self) {
        if let Ok(iface) = netdev::get_default_interface() {
            if let Some(stats) = iface.stats {
                if self.last_rx > 0 && stats.rx_bytes >= self.last_rx {
                    self.down_kbs = (stats.rx_bytes - self.last_rx) as f64 / 1024.0;
                    self.up_kbs = (stats.tx_bytes.saturating_sub(self.last_tx)) as f64 / 1024.0;
                    push_cap(&mut self.down_history, self.down_kbs, HISTORY);
                    push_cap(&mut self.up_history, self.up_kbs, HISTORY);
                }
                self.last_rx = stats.rx_bytes;
                self.last_tx = stats.tx_bytes;
            }
        }
    }

    /// One probe cycle across all four hops.
    pub fn probe(&mut self) -> ScopeSnapshot {
        self.update_throughput();
        self.seq = self.seq.wrapping_add(1);

        self.hop1.record(Some(0.1));
        let gw = parse_v4(&self.gateway_ip);
        let isp = parse_v4(&self.isp_ip);
        let cloud = parse_v4(&self.cloud_ip);
        let seq = self.seq;

        // Probe all 3 remote hops concurrently in parallel threads
        let (gw_rtt, isp_rtt, cloud_rtt) = std::thread::scope(|s| {
            let h_gw = s.spawn(move || {
                gw.and_then(|ip| ping::ping_once(ip, seq, Duration::from_millis(200)).map(ms))
            });
            let h_isp = s.spawn(move || {
                isp.and_then(|ip| ping::ping_once(ip, seq, Duration::from_millis(300)).map(ms))
            });
            let h_cloud = s.spawn(move || {
                cloud.and_then(|ip| ping::ping_once(ip, seq, Duration::from_millis(400)).map(ms))
            });
            (
                h_gw.join().unwrap_or(None),
                h_isp.join().unwrap_or(None),
                h_cloud.join().unwrap_or(None),
            )
        });

        self.hop2.record(gw_rtt);
        self.hop3.record(isp_rtt);
        self.hop4.record(cloud_rtt);

        if let Some(r) = cloud_rtt.or(isp_rtt) {
            push_cap(&mut self.rtt_history, r, HISTORY);
        }
        self.snapshot()
    }

    pub fn snapshot(&self) -> ScopeSnapshot {
        let hops = [
            self.hop1.stats(),
            self.hop2.stats(),
            self.hop3.stats(),
            self.hop4.stats(),
        ];
        let jitter = if hops[3].jitter_ms > 0.0 {
            hops[3].jitter_ms
        } else {
            hops[2].jitter_ms
        };
        let max_bloat = hops[1]
            .bloat_delta_ms
            .max(hops[2].bloat_delta_ms)
            .max(hops[3].bloat_delta_ms);
        let avg = if self.rtt_history.is_empty() {
            0.0
        } else {
            self.rtt_history.iter().sum::<f64>() / self.rtt_history.len() as f64
        };
        let diagnosis = diagnose(&hops, jitter, self.cam_pacer_active);

        ScopeSnapshot {
            hops: hops.to_vec(),
            avg_ms: round1(avg),
            jitter_ms: round1(jitter),
            loss_pct: hops[3].loss_pct,
            max_bloat_delta_ms: round1(max_bloat),
            down_kbs: round1(self.down_kbs),
            up_kbs: round1(self.up_kbs),
            cam_pacer_active: self.cam_pacer_active,
            rtt_history: self.rtt_history.clone(),
            down_kbs_history: self.down_history.clone(),
            up_kbs_history: self.up_history.clone(),
            fault_diagnosis: diagnosis,
        }
    }

    pub fn empty_snapshot(&self) -> ScopeSnapshot {
        self.snapshot()
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn push_cap(v: &mut Vec<f64>, x: f64, cap: usize) {
    v.push(x);
    if v.len() > cap {
        let overflow = v.len() - cap;
        v.drain(0..overflow);
    }
}

fn parse_v4(s: &str) -> Option<Ipv4Addr> {
    s.parse().ok()
}

/// Discover gateway (hop 2) and a distinct ISP/edge resolver (hop 3).
fn discover_hops(interface: &str) -> (String, String) {
    let _ = interface; // netdev resolves the default route directly
    let gw = netdev::get_default_gateway()
        .ok()
        .and_then(|d| d.ipv4.first().map(|ip| ip.to_string()))
        .unwrap_or_else(|| "N/A".into());

    // Prefer the DHCP-advertised DNS resolver if it differs from the gateway and
    // from well-known public resolvers; else fall back to Cloudflare.
    let mut isp = "1.1.1.1".to_string();
    if let Ok(iface) = netdev::get_default_interface() {
        for dns in &iface.dns_servers {
            let s = dns.to_string();
            if s != gw
                && !matches!(
                    s.as_str(),
                    "1.1.1.1" | "1.0.0.1" | "8.8.8.8" | "9.9.9.9" | "127.0.0.1"
                )
            {
                isp = s;
                break;
            }
        }
    }
    (gw, isp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ping_scope_cam_pacer_starts_and_stops_real_udp_thread() {
        let mut scope = PingScope::new("en0");
        assert!(scope.cam_pacer_active);

        // Toggle OFF -> stops any running pacer thread and clears flag
        let msg_off = scope.toggle_cam_pacer();
        assert!(!scope.cam_pacer_active);
        assert!(msg_off.contains("PAUSED"));
        assert!(scope.pacer_stop_flag.is_none());

        // Toggle ON -> spawns real 25Hz UDP keep-alive thread with stop flag
        let msg_on = scope.toggle_cam_pacer();
        assert!(scope.cam_pacer_active);
        assert!(msg_on.contains("ENABLED"));
        let flag = scope
            .pacer_stop_flag
            .as_ref()
            .expect("25Hz pacer stop flag must be present")
            .clone();
        assert!(!flag.load(Ordering::Relaxed));

        // Toggle OFF again -> signals the background thread to stop
        scope.toggle_cam_pacer();
        assert!(flag.load(Ordering::Relaxed));
    }
}
