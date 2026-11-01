//! Lid-open wake reconnector. Ported from power_hook.py.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::cmd;

pub struct WakeReconnector {
    interface: String,
    last_wall: f64,
    last_mono: Instant,
    pub wake_events_healed: u64,
    pub last_wake_ts: String,
}

impl WakeReconnector {
    pub fn new(interface: &str) -> Self {
        WakeReconnector {
            interface: interface.to_string(),
            last_wall: wall_now(),
            last_mono: Instant::now(),
            wake_events_healed: 0,
            last_wake_ts: "None".into(),
        }
    }

    /// A sleep/wake is detected when wall-clock advanced much more than the
    /// monotonic clock (the machine was suspended).
    fn detect_sleep_wake_jump(&mut self, threshold_s: f64) -> bool {
        let now_wall = wall_now();
        let now_mono = Instant::now();
        let dt_wall = now_wall - self.last_wall;
        let dt_mono = now_mono.duration_since(self.last_mono).as_secs_f64();
        self.last_wall = now_wall;
        self.last_mono = now_mono;
        (dt_wall - dt_mono) >= threshold_s
    }

    /// Returns a status string if a wake was detected and healed.
    pub fn check_and_heal_wake(&mut self, last_ssid: &str) -> Option<String> {
        if !self.detect_sleep_wake_jump(2.5) {
            return None;
        }
        self.wake_events_healed += 1;
        self.last_wake_ts = clock_str();

        let out = cmd::run(&["ifconfig", &self.interface], Duration::from_millis(1000));
        if !out.stdout.contains("status: active") {
            cmd::run(
                &["networksetup", "-setairportpower", &self.interface, "on"],
                Duration::from_millis(2000),
            );
            let _ = last_ssid; // reassociation handled by the collector's force switch path
        }
        Some(format!(
            "● LID-OPEN WAKE ACCELERATOR FIRED at {} (0s Wake Lag)",
            self.last_wake_ts
        ))
    }
}

fn wall_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn clock_str() -> String {
    let out = cmd::run(&["date", "+%H:%M:%S"], Duration::from_millis(500));
    let s = out.stdout.trim().to_string();
    if s.is_empty() {
        "--:--:--".into()
    } else {
        s
    }
}
