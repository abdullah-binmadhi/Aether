//! Dead-zone drift logger + scorecard. Ported from history_logger.py.

use std::collections::VecDeque;
use std::io::Write;
use std::time::{Duration, Instant};

use super::model::Telemetry;

fn history_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let aether_p = std::path::Path::new(&home).join(".config/aether/history.jsonl");
    if aether_p.exists() {
        return aether_p;
    }
    let legacy_p = std::path::Path::new(&home).join(".config/wifi-tui/history.jsonl");
    if legacy_p.exists() {
        return legacy_p;
    }
    aether_p
}

#[derive(Clone)]
struct Sample {
    signal_dbm: i64,
    rtt_ms: f64,
    jitter_ms: f64,
    ssid: String,
}

pub struct HistoryLogger {
    samples: VecDeque<Sample>,
    last_log: Option<Instant>,
    dead_zone_drops: i64,
    maxlen: usize,
}

impl HistoryLogger {
    pub fn new() -> Self {
        HistoryLogger {
            samples: VecDeque::new(),
            last_log: None,
            dead_zone_drops: 0,
            maxlen: 360,
        }
    }

    pub fn record_sample(
        &mut self,
        tel: &Telemetry,
        avg_ms: f64,
        jitter_ms: f64,
        loss_pct: f64,
        bloat_ms: f64,
        min_interval_s: f64,
    ) -> bool {
        if let Some(t) = self.last_log {
            if t.elapsed() < Duration::from_secs_f64(min_interval_s) {
                return false;
            }
        }
        self.last_log = Some(Instant::now());

        let sig = tel.signal_dbm;
        if sig <= -75 || loss_pct >= 10.0 || tel.ssid == "Disconnected" {
            self.dead_zone_drops += 1;
        }

        let sample = Sample {
            signal_dbm: sig,
            rtt_ms: avg_ms,
            jitter_ms,
            ssid: tel.ssid.clone(),
        };
        if self.samples.len() == self.maxlen {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);

        // Append JSONL line (best-effort).
        let path = history_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let line = serde_json::json!({
                "ts": now_str(),
                "ssid": tel.ssid,
                "band": tel.band.label(),
                "channel_num": tel.channel_num,
                "signal_dbm": sig,
                "snr_db": tel.snr_db,
                "mcs_index": tel.mcs_index,
                "tx_rate_mbps": tel.tx_rate_mbps,
                "airtime_load_pct": tel.airtime_load_pct,
                "rtt_ms": avg_ms,
                "jitter_ms": jitter_ms,
                "bloat_delta_ms": bloat_ms,
                "loss_pct": loss_pct,
            });
            let _ = writeln!(f, "{line}");
        }
        true
    }

    pub fn scorecard(&self) -> String {
        if self.samples.is_empty() {
            return "Session Drift: 0 Dead-Zone Drops │ Grade A+".into();
        }
        let items = &self.samples;
        let sigs: Vec<i64> = items
            .iter()
            .map(|s| s.signal_dbm)
            .filter(|v| *v > -99)
            .collect();
        let rtts: Vec<f64> = items
            .iter()
            .map(|s| s.rtt_ms)
            .filter(|v| *v > 0.0)
            .collect();
        let jits: Vec<f64> = items.iter().map(|s| s.jitter_ms).collect();

        let min_rssi = sigs.iter().cloned().min().unwrap_or(-90);
        let max_rssi = sigs.iter().cloned().max().unwrap_or(-50);
        let avg_rtt = if rtts.is_empty() {
            0.0
        } else {
            (rtts.iter().sum::<f64>() / rtts.len() as f64 * 10.0).round() / 10.0
        };
        let max_jit = jits.iter().cloned().fold(0.0_f64, f64::max);
        let max_jit = (max_jit * 10.0).round() / 10.0;

        let grade = if self.dead_zone_drops == 0 && min_rssi >= -67 && max_jit < 15.0 {
            "A+ (ZERO DEAD ZONES)"
        } else if self.dead_zone_drops <= 1 && min_rssi >= -72 {
            "A  (SOLID COVERAGE)"
        } else if min_rssi >= -76 {
            "B  (EDGE DRIFT)"
        } else {
            "C  (DEAD ZONE DETECTED)"
        };

        format!(
            "Drift [{} pts]: RSSI {min_rssi}..{max_rssi}dBm │ Avg RTT {avg_rtt}ms │ Drops:{} │ {grade}",
            items.len(),
            self.dead_zone_drops
        )
    }
}

fn now_str() -> String {
    // Lightweight local timestamp without pulling in chrono.
    let out = super::cmd::run(&["date", "+%Y-%m-%d %H:%M:%S"], Duration::from_millis(500));
    let s = out.stdout.trim().to_string();
    if s.is_empty() {
        "----".into()
    } else {
        s
    }
}
