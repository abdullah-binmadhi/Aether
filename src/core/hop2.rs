//! Hop 2 anti-bloat detection & repair. Ported from hop2_repair.py.

use std::time::Duration;

use super::cmd;
use super::hops::ScopeSnapshot;
use super::model::{Band, Telemetry};

pub struct Hop2Anomaly {
    pub anomaly: bool,
    pub rtt_ms: f64,
    pub bloat_ms: f64,
    pub jitter_ms: f64,
    pub diagnosis: String,
}

pub fn detect_anomaly(scope: &ScopeSnapshot) -> Hop2Anomaly {
    if scope.hops.len() < 2 {
        return Hop2Anomaly {
            anomaly: false,
            rtt_ms: 0.0,
            bloat_ms: 0.0,
            jitter_ms: 0.0,
            diagnosis: "Clean".into(),
        };
    }
    let h2 = &scope.hops[1];
    let rtt = h2.rtt_ms;
    let bloat = h2.bloat_delta_ms;
    let jitter = h2.jitter_ms;
    let gw_ip = &h2.target;
    let is_bloated = bloat > 15.0 || rtt > 25.0;
    let is_jittery = jitter > 5.0;
    let has_anomaly = is_bloated || is_jittery;

    let diagnosis = if has_anomaly {
        let mut reasons = Vec::new();
        if is_bloated {
            reasons.push(format!("Airlink Queue Bloat (+{bloat:.0}ms)"));
        }
        if is_jittery {
            reasons.push(format!("Jitter Spikes ({jitter:.1}ms)"));
        }
        format!("[!] HOP 2 BOTTLENECK on {gw_ip}: {}", reasons.join(" & "))
    } else {
        format!("[OK] Hop 2 Airlink Clean ({rtt:.1}ms │ Jit {jitter:.1}ms)")
    };

    Hop2Anomaly {
        anomaly: has_anomaly,
        rtt_ms: rtt,
        bloat_ms: bloat,
        jitter_ms: jitter,
        diagnosis,
    }
}

pub fn evaluate_band_efficiency(tel: &Telemetry) -> String {
    let band = tel.band;
    let sig = tel.signal_dbm;
    let snr = tel.snr_db;
    let retry = tel.tx_retry_pct;
    let mcs = tel.mcs_index;
    let mut advice = Vec::new();

    if band.is_fast() {
        if sig < -70 {
            advice.push(format!(
                "5GHz Wall Attenuation: Weak signal ({sig}dBm) causes Rate Adaptation drops (MCS {mcs}). 2.4GHz may provide 3x flatter latency through walls."
            ));
        }
        if retry > 5.0 {
            advice.push(format!(
                "Airlink Retransmissions High ({retry:.1}%): 802.11 frame retries are inflating Hop 2 bufferbloat."
            ));
        }
        if advice.is_empty() {
            advice.push(
                "5GHz Signal Strong: Optimal for close line-of-sight gigabit streaming.".into(),
            );
        }
    } else {
        advice.push(format!(
            "2.4GHz Penetration Active (Signal {sig}dBm, SNR {snr}dB): Excellent wall penetration. Narrow 20MHz channel provides flat latency."
        ));
    }
    let _ = band;
    advice.join(" │ ")
}

/// Stage 5 of the pipeline: flush the gateway ARP entry (link-safe).
pub fn refresh_gateway_arp(gateway: &str) {
    if !gateway.is_empty() && gateway != "N/A" {
        let _ = cmd::run(&["arp", "-d", gateway], Duration::from_millis(1000));
    }
}

/// Band label helper used by the repair summary.
pub fn band_label(tel: &Telemetry) -> &'static str {
    match tel.band {
        Band::Band2_4GHz => "2.4GHz",
        Band::Band5GHz => "5GHz",
        Band::Band6GHz => "6GHz",
        _ => "N/A",
    }
}
