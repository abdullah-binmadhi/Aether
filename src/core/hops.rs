//! 4-hop bufferbloat isolation matrix.
//!
//! Ported directly from the Python `HopTracker` / `PingScopeEngine`:
//! RFC 3550 smoothed jitter, 15th-percentile baseline RTT, bufferbloat delta,
//! and the same per-hop and overall verdict thresholds.

use std::collections::VecDeque;

const RTT_HISTORY: usize = 30;
const OUTCOME_HISTORY: usize = 20;

#[derive(Debug, Clone)]
#[allow(dead_code)] // target/avg_ms/baseline_ms are serialized in --json and used by future views
pub struct HopStats {
    pub hop: u8,
    pub label: String,
    pub target: String,
    pub rtt_ms: f64,
    pub avg_ms: f64,
    pub jitter_ms: f64,
    pub loss_pct: f64,
    pub baseline_ms: f64,
    pub bloat_delta_ms: f64,
    pub status: &'static str,
}

#[derive(Debug)]
pub struct HopTracker {
    pub hop: u8,
    pub label: String,
    pub target: String,
    rtts: VecDeque<f64>,
    outcomes: VecDeque<bool>,
    total_probes: u64,
    lost_probes: u64,
    last_rtt_ms: Option<f64>,
    ewma_jitter_ms: f64,
}

impl HopTracker {
    pub fn new(hop: u8, label: &str, target: &str) -> Self {
        HopTracker {
            hop,
            label: label.to_string(),
            target: target.to_string(),
            rtts: VecDeque::with_capacity(RTT_HISTORY),
            outcomes: VecDeque::with_capacity(OUTCOME_HISTORY),
            total_probes: 0,
            lost_probes: 0,
            last_rtt_ms: None,
            ewma_jitter_ms: 0.0,
        }
    }

    pub fn record(&mut self, rtt_ms: Option<f64>) {
        self.total_probes += 1;
        match rtt_ms {
            Some(rtt) => {
                push_capped(&mut self.outcomes, true, OUTCOME_HISTORY);
                if let Some(last) = self.last_rtt_ms {
                    // RFC 3550 smoothed jitter, clamped to drop scheduling outliers.
                    let delta = (rtt - last).abs().min(40.0);
                    self.ewma_jitter_ms = if self.ewma_jitter_ms == 0.0 {
                        delta * 0.5
                    } else {
                        0.85 * self.ewma_jitter_ms + 0.15 * delta
                    };
                }
                self.last_rtt_ms = Some(rtt);
                push_capped(&mut self.rtts, rtt, RTT_HISTORY);
            }
            None => {
                push_capped(&mut self.outcomes, false, OUTCOME_HISTORY);
                self.lost_probes += 1;
            }
        }
    }

    pub fn stats(&self) -> HopStats {
        let base = |status: &'static str, rtt, avg, jit, loss, baseline, bloat| HopStats {
            hop: self.hop,
            label: self.label.clone(),
            target: self.target.clone(),
            rtt_ms: rtt,
            avg_ms: avg,
            jitter_ms: jit,
            loss_pct: loss,
            baseline_ms: baseline,
            bloat_delta_ms: bloat,
            status,
        };

        if self.rtts.is_empty() {
            let loss = if self.total_probes == 0 { 0.0 } else { 100.0 };
            return base("SCANNING", 0.0, 0.0, 0.0, loss, 0.0, 0.0);
        }

        let vals: Vec<f64> = self.rtts.iter().copied().collect();
        let curr = self.last_rtt_ms.unwrap_or(*vals.last().unwrap());
        let avg_ms = vals.iter().sum::<f64>() / vals.len() as f64;
        let jitter_ms = self.ewma_jitter_ms;

        let loss_pct = if !self.outcomes.is_empty() {
            let lost = self.outcomes.iter().filter(|ok| !**ok).count();
            lost as f64 / self.outcomes.len() as f64 * 100.0
        } else {
            self.lost_probes as f64 / self.total_probes.max(1) as f64 * 100.0
        };

        let mut sorted = vals.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let baseline_ms = sorted[(sorted.len() as f64 * 0.15) as usize];
        let bloat_delta_ms = if vals.len() >= 3 {
            (curr - baseline_ms).max(0.0)
        } else {
            0.0
        };

        let status = if loss_pct > 5.0 {
            "[!] LOSS"
        } else if bloat_delta_ms > 24.0 {
            "[!] BLOAT"
        } else if jitter_ms > 15.0 {
            "[!] JITTER"
        } else {
            "[OK] CLEAN"
        };

        base(
            status,
            round1(curr),
            round1(avg_ms),
            round1(jitter_ms),
            round1(loss_pct),
            round1(baseline_ms),
            round1(bloat_delta_ms),
        )
    }
}

fn push_capped<T>(dq: &mut VecDeque<T>, v: T, cap: usize) {
    if dq.len() == cap {
        dq.pop_front();
    }
    dq.push_back(v);
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Overall snapshot across all four hops, with the composite fault diagnosis.
#[derive(Debug, Clone)]
pub struct ScopeSnapshot {
    pub hops: Vec<HopStats>,
    pub avg_ms: f64,
    pub jitter_ms: f64,
    pub loss_pct: f64,
    pub max_bloat_delta_ms: f64,
    pub down_kbs: f64,
    pub up_kbs: f64,
    pub cam_pacer_active: bool,
    pub rtt_history: Vec<f64>,
    pub down_kbs_history: Vec<f64>,
    pub up_kbs_history: Vec<f64>,
    pub fault_diagnosis: String,
}

/// Build the composite diagnosis from the four hop stats (ported verdict ladder).
pub fn diagnose(hops: &[HopStats; 4], jitter: f64, cam_pacer_active: bool) -> String {
    let h2 = &hops[1];
    let h3 = &hops[2];
    let h4 = &hops[3];

    if h2.bloat_delta_ms > 20.0 || h2.rtt_ms > 35.0 {
        format!(
            "[!] HOP 2 BOTTLENECK: Wi-Fi Airlink / Router Queue Bloat (+{:.0}ms)",
            h2.bloat_delta_ms
        )
    } else if h3.bloat_delta_ms > 30.0 || h3.rtt_ms > 60.0 {
        format!(
            "[!] HOP 3 BOTTLENECK: ISP First-Hop Congestion (+{:.0}ms)",
            h3.bloat_delta_ms
        )
    } else if h4.bloat_delta_ms > 40.0 || h4.rtt_ms > 95.0 {
        format!(
            "[!] HOP 4 BOTTLENECK: International Peering Delay ({:.0}ms)",
            h4.rtt_ms
        )
    } else if jitter > 12.0 {
        if cam_pacer_active {
            format!("[!] RF CONTENTION JITTER ({jitter:.1}ms) — CAM Pacer & VIP WMM Mitigating")
        } else {
            format!("[!] RF JITTER SPIKE ({jitter:.1}ms) — Enable CAM Pacer [K] & Zero-Lag [L]")
        }
    } else {
        let tag = if cam_pacer_active {
            "CAM 25Hz + VIP WMM Active"
        } else {
            "Airlink & WAN Optimal"
        };
        format!("[OK] ALL 4 HOPS CLEAN ({tag} | Zero Bufferbloat)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_hop_is_clean() {
        let mut hop = HopTracker::new(2, "Gateway", "192.168.100.1");
        for rtt in [2.0, 2.2, 2.1, 2.3, 2.0] {
            hop.record(Some(rtt));
        }
        let st = hop.stats();
        assert_eq!(st.status, "[OK] CLEAN");
        assert!(st.jitter_ms < 1.0, "jitter {}", st.jitter_ms);
        assert!(st.bloat_delta_ms < 1.0, "bloat {}", st.bloat_delta_ms);
    }

    #[test]
    fn bufferbloat_spike_is_flagged() {
        let mut hop = HopTracker::new(2, "Gateway", "192.168.100.1");
        for rtt in [2.0, 2.2, 2.1, 2.3, 2.0] {
            hop.record(Some(rtt));
        }
        hop.record(Some(40.0));
        let st = hop.stats();
        assert!(st.bloat_delta_ms >= 35.0, "bloat {}", st.bloat_delta_ms);
        assert_eq!(st.status, "[!] BLOAT");
    }

    #[test]
    fn total_loss_reports_100pct() {
        let mut hop = HopTracker::new(3, "ISP", "1.1.1.1");
        for _ in 0..6 {
            hop.record(None);
        }
        let st = hop.stats();
        assert_eq!(st.loss_pct, 100.0);
    }
}
