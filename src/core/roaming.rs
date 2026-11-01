//! BSSID roaming evaluation & sticky-client detection. Ported from roaming.py.

use super::model::{Band, Telemetry};

fn score_ap_candidate(signal_dbm: i64, band: Band, width_mhz: i64) -> i64 {
    let base = (signal_dbm + 100).clamp(0, 65);
    let band_bonus = match band {
        Band::Band6GHz => 24,
        Band::Band5GHz => 18,
        _ => 0,
    };
    let width_bonus = if width_mhz >= 80 {
        12
    } else if width_mhz >= 40 {
        6
    } else {
        0
    };
    base + band_bonus + width_bonus
}

#[derive(Clone)]
pub struct Candidate {
    pub ssid: String,
    pub band: Band,
    pub channel_num: i64,
    pub signal_dbm: i64,
    pub score: i64,
    pub gain_score: i64,
}

pub struct RoamEval {
    pub sticky_client: bool,
    pub active_score: i64,
    pub best_candidate: Option<Candidate>,
    pub verdict: String,
}

pub fn evaluate_roaming(tel: &Telemetry) -> RoamEval {
    let active_ssid = tel.ssid.clone();
    if matches!(active_ssid.as_str(), "Disconnected" | "Scanning..." | "N/A") {
        return RoamEval {
            sticky_client: false,
            active_score: 0,
            best_candidate: None,
            verdict: "OFFLINE".into(),
        };
    }

    let curr_sig = tel.signal_dbm;
    let curr_band = tel.band;
    let curr_width = tel.width_mhz;
    let curr_ch = tel.channel_num;

    let lower = active_ssid.to_lowercase();
    if tel.is_hotspot || lower.contains("iphone") || lower.contains("hotspot") {
        return RoamEval {
            sticky_client: false,
            active_score: 95,
            best_candidate: None,
            verdict: format!(
                "[OK] Hotspot Lock Optimal ({} Ch{curr_ch}, {curr_sig}dBm)",
                curr_band.label()
            ),
        };
    }

    let active_score = score_ap_candidate(curr_sig, curr_band, curr_width);
    let sib_5g = tel.sibling_5g_ssid.clone();
    let mut candidates: Vec<Candidate> = Vec::new();

    for net in &tel.neighbors {
        let n_ssid = &net.label;
        let n_sig = net.signal_dbm;
        let is_own = n_ssid.to_lowercase().contains(&lower)
            || sib_5g
                .as_ref()
                .map(|s| !s.is_empty() && n_ssid.to_lowercase().contains(&s.to_lowercase()))
                .unwrap_or(false);
        if is_own && n_sig >= -76 {
            let c_score = score_ap_candidate(n_sig, net.band, net.width_mhz);
            let ssid = if sib_5g.is_some() && net.band.is_fast() {
                sib_5g.clone().unwrap()
            } else {
                active_ssid.clone()
            };
            candidates.push(Candidate {
                ssid,
                band: net.band,
                channel_num: net.channel_num,
                signal_dbm: n_sig,
                score: c_score,
                gain_score: c_score - active_score,
            });
        }
    }

    // Synthetic 5GHz sibling when on 2.4 and a sibling is known.
    if curr_band == Band::Band2_4GHz {
        if let Some(sib) = &sib_5g {
            if !candidates.iter().any(|c| c.band.is_fast()) {
                let est_sig = (curr_sig - 5).max(-72);
                let c_score = score_ap_candidate(est_sig, Band::Band5GHz, 80);
                candidates.push(Candidate {
                    ssid: sib.clone(),
                    band: Band::Band5GHz,
                    channel_num: 36,
                    signal_dbm: est_sig,
                    score: c_score,
                    gain_score: c_score - active_score,
                });
            }
        }
    }

    candidates.sort_by_key(|a| std::cmp::Reverse(a.score));
    let best = candidates.first().cloned();

    let mut sticky_client = false;
    let mut verdict = format!(
        "[OK] Locked to Optimal AP ({} Ch{curr_ch}, {curr_sig}dBm)",
        curr_band.label()
    );
    if let Some(b) = &best {
        if b.gain_score >= 12 {
            sticky_client = true;
            verdict = format!(
                "[!] STICKY CLIENT: Better AP '{}' ({} Ch{}, {}dBm) -> Press [R] to Roam",
                b.ssid,
                b.band.label(),
                b.channel_num,
                b.signal_dbm
            );
        }
    }
    if !sticky_client && curr_sig <= -72 {
        sticky_client = true;
        verdict = format!(
            "[!] WEAK AP LOCK ({curr_sig}dBm on Ch{curr_ch}) -> Press [R] to Trigger Roam Scan"
        );
    }

    RoamEval {
        sticky_client,
        active_score,
        best_candidate: best,
        verdict,
    }
}
