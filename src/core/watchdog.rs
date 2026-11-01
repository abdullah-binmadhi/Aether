//! Headless link-anomaly watchdog loop. Ported from watchdog.py.

use std::time::Duration;

use super::cmd;
use super::collector::Collector;
use super::dns_engine::DnsEngine;
use super::model::{Band, Telemetry};

pub struct LinkAnomaly {
    pub sticky_mcs: bool,
    pub downgraded_24g: bool,
    pub backup_links: Vec<(String, String)>,
}

/// List active non-loopback interfaces with an IPv4 address (failover candidates).
pub fn list_active_interfaces() -> Vec<(String, String)> {
    let out = cmd::run(&["ifconfig"], Duration::from_millis(2000));
    let mut active = Vec::new();
    let mut curr = String::new();
    for line in out.stdout.lines() {
        if !line.is_empty() && !line.starts_with('\t') && line.contains(':') {
            curr = line.split(':').next().unwrap_or("").to_string();
        } else if !curr.is_empty() && line.contains("inet ") && !line.contains("127.0.0.1") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                active.push((curr.clone(), parts[1].to_string()));
            }
        }
    }
    active
}

pub fn evaluate_link_anomalies(tel: &Telemetry) -> LinkAnomaly {
    let ifaces = list_active_interfaces();
    let backup_links: Vec<(String, String)> = ifaces
        .into_iter()
        .filter(|(iface, _)| iface != &tel.interface)
        .collect();
    let sticky_mcs =
        tel.band.is_fast() && tel.signal_dbm >= -64 && tel.mcs_index > 0 && tel.mcs_index < 4;
    let downgraded_24g = tel.band == Band::Band2_4GHz;
    LinkAnomaly {
        sticky_mcs,
        downgraded_24g,
        backup_links,
    }
}

/// Run the headless watchdog loop. `max_ticks` bounds it for testing; `None` = forever.
pub fn run_watchdog_daemon(interface: &str, interval_sec: u64, max_ticks: Option<u64>) -> i32 {
    let mut collector = Collector::new(interface);
    let mut scope = super::PingScope::new(interface);
    println!("⚡ WIFI TUI Watchdog Daemon started (polling every {interval_sec}s)...");
    let mut tick = 0u64;
    loop {
        tick += 1;
        let snap = scope.probe();
        let tel = collector.collect(snap.avg_ms, snap.loss_pct);
        let anom = evaluate_link_anomalies(&tel);
        if anom.sticky_mcs {
            // Safe link refresh: flush DNS + renew DHCP (never cycles radio).
            DnsEngine::flush_cache();
            cmd::run(
                &["ipconfig", "set", interface, "DHCP"],
                Duration::from_millis(3000),
            );
            println!(
                "[Watchdog] Safe Link Refresh: DHCP renewed & DNS flushed (sticky MCS healed)."
            );
        }
        if max_ticks.map(|m| tick >= m).unwrap_or(false) {
            break;
        }
        std::thread::sleep(Duration::from_secs(interval_sec));
    }
    0
}
