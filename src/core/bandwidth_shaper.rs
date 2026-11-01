//! Per-PID bandwidth throttle via taskpolicy + dnctl. Ported from bandwidth_shaper.py.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use super::cmd;

pub struct BandwidthShaper {
    pub throttled_pids: HashSet<i32>,
    pub pipe_limit_mbps: i64,
    pid_labels: HashMap<i32, String>,
}

impl BandwidthShaper {
    pub fn new() -> Self {
        BandwidthShaper {
            throttled_pids: HashSet::new(),
            pipe_limit_mbps: 0,
            pid_labels: HashMap::new(),
        }
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
            if self.throttled_pids.is_empty() {
                self.clear_dummynet_pipe();
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
    }

    fn clear_dummynet_pipe(&mut self) {
        self.pipe_limit_mbps = 0;
        cmd::run(
            &["sudo", "-n", "dnctl", "-q", "flush"],
            Duration::from_millis(1000),
        );
    }
}
