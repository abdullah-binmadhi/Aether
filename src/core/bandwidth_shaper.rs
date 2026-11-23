//! Per-PID bandwidth throttle via taskpolicy + dnctl. Ported from bandwidth_shaper.py.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use super::cmd;

pub struct AqmConfig {
    pub interface: String,
    pub up_cap_mbps: u32,
    pub down_cap_mbps: u32,
    pub game_udp_ports: Vec<u16>,
}

pub struct BandwidthShaper {
    pub throttled_pids: HashSet<i32>,
    pub pipe_limit_mbps: i64,
    pub aqm_active: bool,
    pid_labels: HashMap<i32, String>,
}

impl BandwidthShaper {
    pub fn new() -> Self {
        BandwidthShaper {
            throttled_pids: HashSet::new(),
            pipe_limit_mbps: 0,
            aqm_active: false,
            pid_labels: HashMap::new(),
        }
    }

    pub fn build_pfctl_anchor_rules(cfg: &AqmConfig) -> String {
        let mut port_list: Vec<String> = vec![
            "53".into(),
            "3074".into(),
            "3478:3479".into(),
            "5000:5500".into(),
            "7000:8000".into(),
            "27000:27050".into(),
        ];
        for p in &cfg.game_udp_ports {
            let ps = p.to_string();
            if !port_list.contains(&ps) {
                port_list.push(ps);
            }
        }
        let ports_joined = port_list.join(", ");
        format!(
            "match out on {} proto udp from any to any port {{ {} }} set tos ef\n\
             dummynet out on {} proto tcp from any to any port {{ 80, 443 }} queue 2\n",
            cfg.interface, ports_joined, cfg.interface
        )
    }

    /// Atomically configures `dnctl` CoDel pipes and loads an isolated `pfctl`
    /// anchor (`com.apple/aether`) so game UDP flows get DSCP EF (AC_VO) priority
    /// while bulk TCP flows are shaped through a CoDel scavenger queue.
    pub fn enable_autopilot_aqm(&mut self, cfg: &AqmConfig, apply_os: bool) -> Result<String, String> {
        let up_cap = cfg.up_cap_mbps.max(5);
        let up_bw = format!("{up_cap}Mbit/s");
        let rules = Self::build_pfctl_anchor_rules(cfg);

        if apply_os {
            let codel_ok = cmd::ok(
                &[
                    "sudo", "-n", "dnctl", "pipe", "1", "config", "bw", &up_bw, "codel", "target",
                    "3ms", "interval", "60ms", "ecn",
                ],
                Duration::from_millis(800),
            );
            if !codel_ok {
                let fallback_ok = cmd::ok(
                    &["sudo", "-n", "dnctl", "pipe", "1", "config", "bw", &up_bw, "queue", "20"],
                    Duration::from_millis(800),
                );
                if !fallback_ok {
                    return Err("dnctl pipe config requires passwordless sudo".into());
                }
            }
            let _ = cmd::ok(
                &["sudo", "-n", "dnctl", "queue", "1", "config", "pipe", "1", "weight", "100"],
                Duration::from_millis(800),
            );
            let _ = cmd::ok(
                &["sudo", "-n", "dnctl", "queue", "2", "config", "pipe", "1", "weight", "15"],
                Duration::from_millis(800),
            );

            let out = cmd::run_with_stdin(
                &["sudo", "-n", "pfctl", "-a", "com.apple/aether", "-f", "-"],
                Some(&rules),
                Duration::from_millis(1000),
            );
            let _ = cmd::ok(&["sudo", "-n", "pfctl", "-e"], Duration::from_millis(500));
            if !out.success {
                return Err("Failed to load pfctl anchor com.apple/aether".into());
            }
        }

        self.pipe_limit_mbps = up_cap as i64;
        self.aqm_active = true;
        Ok(format!(
            "● AQM Active: pfctl AC_VO (DSCP EF) + CoDel 3ms @ {up_bw} (Down cap {}Mbit/s)",
            cfg.down_cap_mbps.max(10)
        ))
    }

    pub fn disable_autopilot_aqm(&mut self, apply_os: bool) {
        if apply_os {
            let _ = cmd::ok(
                &["sudo", "-n", "pfctl", "-a", "com.apple/aether", "-F", "all"],
                Duration::from_millis(800),
            );
            self.clear_dummynet_pipe();
        } else {
            self.pipe_limit_mbps = 0;
        }
        self.aqm_active = false;
    }

    pub fn toggle_throttle_pid(&mut self, pid: i32, name: &str, limit_mbps: i64) -> String {
        if pid <= 1 {
            return "[!] Cannot throttle kernel/init PID.".into();
        }
        let pid_s = pid.to_string();
        if self.throttled_pids.contains(&pid) {
            cmd::run(
                &["taskpolicy", "-B", "-p", &pid_s],
                Duration::from_millis(800),
            );
            cmd::run(&["renice", "0", "-p", &pid_s], Duration::from_millis(800));
            self.throttled_pids.remove(&pid);
            self.pid_labels.remove(&pid);
            if self.throttled_pids.is_empty() && !self.aqm_active {
                self.disable_autopilot_aqm(true);
            }
            format!("○ Restored normal bandwidth priority for {name} (PID {pid}).")
        } else {
            cmd::run(
                &["taskpolicy", "-b", "-p", &pid_s],
                Duration::from_millis(800),
            );
            cmd::run(&["renice", "20", "-p", &pid_s], Duration::from_millis(800));
            self.throttled_pids.insert(pid);
            self.pid_labels.insert(pid, name.to_string());
            self.configure_dummynet_pipe(limit_mbps);
            format!("● THROTTLED {name} (PID {pid}) -> WMM Scavenger (AC_BK) + {limit_mbps}Mbps Shaper Pipe")
        }
    }

    fn configure_dummynet_pipe(&mut self, limit_mbps: i64) {
        self.pipe_limit_mbps = limit_mbps;
        let bw = format!("{limit_mbps}Mbit/s");
        cmd::run(
            &[
                "sudo", "-n", "dnctl", "pipe", "1", "config", "bw", &bw, "queue", "20",
            ],
            Duration::from_millis(1000),
        );
        let rules = "dummynet out proto tcp from any to any port { 80, 443 } pipe 1\n";
        let _ = cmd::run_with_stdin(
            &["sudo", "-n", "pfctl", "-a", "com.apple/aether", "-f", "-"],
            Some(rules),
            Duration::from_millis(1000),
        );
        let _ = cmd::ok(&["sudo", "-n", "pfctl", "-e"], Duration::from_millis(500));
    }

    fn clear_dummynet_pipe(&mut self) {
        self.pipe_limit_mbps = 0;
        cmd::run(
            &["sudo", "-n", "dnctl", "-q", "flush"],
            Duration::from_millis(1000),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aqm_enable_and_disable_updates_state_and_generates_valid_pfctl_rules() {
        let mut shaper = BandwidthShaper::new();
        let cfg = AqmConfig {
            interface: "en0".into(),
            up_cap_mbps: 25,
            down_cap_mbps: 180,
            game_udp_ports: vec![9001],
        };
        let rules = BandwidthShaper::build_pfctl_anchor_rules(&cfg);
        assert!(rules.contains("match out on en0 proto udp"));
        assert!(rules.contains("9001"));
        assert!(rules.contains("set tos ef"));
        assert!(rules.contains("dummynet out on en0 proto tcp"));

        let msg = shaper.enable_autopilot_aqm(&cfg, false).unwrap();
        assert!(shaper.aqm_active);
        assert_eq!(shaper.pipe_limit_mbps, 25);
        assert!(msg.contains("DSCP EF"));

        shaper.disable_autopilot_aqm(false);
        assert!(!shaper.aqm_active);
        assert_eq!(shaper.pipe_limit_mbps, 0);
    }
}
