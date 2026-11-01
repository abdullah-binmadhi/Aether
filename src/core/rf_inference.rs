//! Non-Wi-Fi RF interference inference. Ported from rf_inference.py.

use super::model::{Band, Telemetry};

#[derive(Clone)]
pub struct Detection {
    pub detected: bool,
    pub confidence: f64,
    pub threat: &'static str,
    pub reason: String,
    pub advice: String,
}

fn is_24g(tel: &Telemetry) -> bool {
    tel.band == Band::Band2_4GHz || (1..=14).contains(&tel.channel_num)
}

pub fn detect_microwave(tel: &Telemetry) -> Detection {
    let ch = tel.channel_num;
    let noise = tel.noise_dbm;
    let retry = tel.tx_retry_pct;
    let in_band = (7..=11).contains(&ch);
    let noise_spiked = noise > -86;
    let retries_elevated = retry > 12.0;
    let detected = is_24g(tel) && in_band && (noise_spiked || retries_elevated);
    let confidence = if detected {
        if noise_spiked && retries_elevated {
            0.90
        } else {
            0.65
        }
    } else {
        0.0
    };
    Detection {
        detected,
        confidence,
        threat: "Microwave 2.45GHz RF Burst",
        reason: if detected {
            format!("Microwave Magnetron Leakage on Ch {ch}: 2.45GHz RF burst (Noise {noise}dBm, Retries {retry:.1}%)")
        } else {
            String::new()
        },
        advice: if detected {
            "Switch AP to 2.4GHz Channel 1, or move device to 5GHz UNII-1/3 to completely bypass microwave RF.".into()
        } else {
            String::new()
        },
    }
}

pub fn detect_bluetooth(tel: &Telemetry) -> Detection {
    let noise = tel.noise_dbm;
    let retry = tel.tx_retry_pct;
    let clean_noise = noise <= -88;
    let bt_range = (5.0..=18.0).contains(&retry);
    let detected = is_24g(tel) && clean_noise && bt_range;
    Detection {
        detected,
        confidence: if detected { 0.70 } else { 0.0 },
        threat: "Bluetooth AFH Coexistence Clash",
        reason: if detected {
            format!("Bluetooth 2.4GHz AFH Hopping Collision (Retries {retry:.1}%, Noise {noise}dBm normal)")
        } else {
            String::new()
        },
        advice: if detected {
            "Keep Bluetooth audio peripherals >1 meter away from Wi-Fi antenna or migrate to 5GHz Wi-Fi.".into()
        } else {
            String::new()
        },
    }
}

pub fn detect_aci(tel: &Telemetry) -> (Detection, usize) {
    let my_ch = tel.channel_num;
    let standard = [1, 6, 11, 14];
    let mut offenders = Vec::new();
    for net in &tel.neighbors {
        let net_ch = net.channel_num;
        if (1..=14).contains(&net_ch)
            && !standard.contains(&net_ch)
            && (net_ch - my_ch).abs() <= 3
            && my_ch != net_ch
        {
            offenders.push((net.label.clone(), net_ch, net.signal_dbm));
        }
    }
    let detected = !offenders.is_empty();
    let (reason, advice) = if detected {
        let (ref ssid, ch, sig) = offenders[0];
        (
            format!("Adjacent Channel Bleed on Ch {my_ch}: Neighbor '{ssid}' broadcasting on non-standard Ch {ch} ({sig}dBm)"),
            "Change router channel to standard 1, 6, or 11 to align OFDM subcarrier nulls.".to_string(),
        )
    } else {
        (String::new(), String::new())
    };
    (
        Detection {
            detected,
            confidence: if detected { 0.85 } else { 0.0 },
            threat: "Adjacent Channel Interference (ACI Bleed)",
            reason,
            advice,
        },
        offenders.len(),
    )
}

pub fn detect_dfs(tel: &Telemetry) -> Detection {
    let ch = tel.channel_num;
    let is_dfs = (52..=144).contains(&ch);
    let summary = &tel.disassoc_summary;
    let detected = is_dfs
        && (summary.contains("Radar")
            || summary.contains("CSA")
            || summary.contains("Channel Switch"));
    let confidence = if detected {
        0.90
    } else if is_dfs {
        0.20
    } else {
        0.0
    };
    let (reason, advice) = if is_dfs {
        (
            format!("Active on 5GHz DFS Channel {ch} (UNII-2/2e). Subject to 802.11h weather radar backoff."),
            "Lock router channel to UNII-1 (Ch 36-48) or UNII-3 (Ch 149-161) for zero radar drops.".to_string(),
        )
    } else {
        (String::new(), String::new())
    };
    Detection {
        detected,
        confidence,
        threat: "5GHz DFS Radar Backoff (UNII-2)",
        reason,
        advice,
    }
}

pub struct RfReport {
    pub rf_health_score: i64,
    pub has_interference: bool,
    pub summary: String,
    pub primary_advice: String,
}

pub fn run_full(tel: &Telemetry) -> RfReport {
    let mw = detect_microwave(tel);
    let bt = detect_bluetooth(tel);
    let (aci, _) = detect_aci(tel);
    let dfs = detect_dfs(tel);

    let mut threats: Vec<&Detection> = Vec::new();
    for d in [&mw, &bt, &aci, &dfs] {
        if d.detected {
            threats.push(d);
        }
    }
    let mut score = 100i64;
    if mw.detected {
        score -= 35;
    }
    if aci.detected {
        score -= 25;
    }
    if bt.detected {
        score -= 15;
    }
    if dfs.detected {
        score -= 20;
    }
    score = score.clamp(10, 100);

    // threats[0] in Python order is mw, bt, aci, dfs filtered — mw first if present.
    let (summary, primary_advice) = if threats.is_empty() {
        (
            "[OK] Clean RF Airspace — Zero non-Wi-Fi or adjacent channel interference detected."
                .to_string(),
            "Airspace optimal for low-latency gaming & gigabit streaming.".to_string(),
        )
    } else {
        let p = threats[0];
        (
            format!("[!] RF INTERFERENCE DETECTED: {}", p.reason),
            p.advice.clone(),
        )
    };

    RfReport {
        rf_health_score: score,
        has_interference: !threats.is_empty(),
        summary,
        primary_advice,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::Telemetry;

    #[test]
    fn every_detector_confidence_is_a_valid_fraction() {
        // The AIRSPACE RF-scanner panel renders each detector's confidence as a
        // 0..=100% bar; confidence must always be a clean fraction in [0,1].
        let tel = Telemetry::default();
        let mw = detect_microwave(&tel);
        let bt = detect_bluetooth(&tel);
        let (aci, _) = detect_aci(&tel);
        let dfs = detect_dfs(&tel);
        for d in [&mw, &bt, &aci, &dfs] {
            assert!(
                (0.0..=1.0).contains(&d.confidence),
                "confidence out of range: {}",
                d.confidence
            );
            // A non-detection must carry zero confidence (empty bar).
            if !d.detected {
                assert_eq!(d.confidence, 0.0);
            }
        }
        // The overall report score is always a clamped 10..=100.
        let rep = run_full(&tel);
        assert!((10..=100).contains(&rep.rf_health_score));
    }
}
