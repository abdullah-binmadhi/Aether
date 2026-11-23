//! BSSID roaming evaluation & sticky-client detection. Ported from roaming.py.

use super::model::{Band, Telemetry};

fn is_non_dfs_5g(channel_num: i64) -> bool {
    (36..=48).contains(&channel_num) || (149..=165).contains(&channel_num)
}

fn score_ap_candidate_channel(
    signal_dbm: i64,
    band: Band,
    width_mhz: i64,
    channel_num: i64,
    has_collision: bool,
) -> i64 {
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
    let non_dfs_bonus = if band == Band::Band5GHz && is_non_dfs_5g(channel_num) {
        10
    } else {
        0
    };
    let collision_penalty = if has_collision { 16 } else { 0 };
    (base + band_bonus + width_bonus + non_dfs_bonus - collision_penalty).max(0)
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

    let active_has_collision = curr_ch > 0
        && tel
            .neighbors
            .iter()
            .any(|n| n.channel_num == curr_ch && n.signal_dbm >= -75);
    let active_score = score_ap_candidate_channel(
        curr_sig,
        curr_band,
        curr_width,
        curr_ch,
        active_has_collision,
    );
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
            let net_colliding = net.channel_num > 0
                && tel.neighbors.iter().any(|other| {
                    other.channel_num == net.channel_num
                        && other.label != net.label
                        && other.signal_dbm >= -75
                });
            let c_score = score_ap_candidate_channel(
                n_sig,
                net.band,
                net.width_mhz,
                net.channel_num,
                net_colliding,
            );
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
                let c_score =
                    score_ap_candidate_channel(est_sig, Band::Band5GHz, 80, 36, false);
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

#[derive(Clone, Debug, Default)]
pub struct RoamCooldown {
    pub last_roam_at: Option<std::time::Instant>,
    pub attempts: u32,
}

/// Actively steer to a higher-scoring 5GHz/6GHz sibling SSID when sticky-client
/// is detected. Never runs while `is_game_locked` is true and enforces a 90s
/// cooldown so active match sockets are never dropped or ping-ponged.
pub fn execute_smart_roam(
    interface: &str,
    eval: &RoamEval,
    wifi_password: &str,
    is_game_locked: bool,
    cooldown: &mut RoamCooldown,
    apply_os: bool,
) -> Option<String> {
    if is_game_locked || !eval.sticky_client {
        return None;
    }
    if let Some(last) = cooldown.last_roam_at {
        if last.elapsed() < std::time::Duration::from_secs(90) {
            return None;
        }
    }
    let best = eval.best_candidate.as_ref()?;
    if best.gain_score < 12 {
        return None;
    }

    cooldown.last_roam_at = Some(std::time::Instant::now());
    cooldown.attempts += 1;

    if apply_os {
        let out = if wifi_password.is_empty() {
            super::cmd::run(
                &["networksetup", "-setairportnetwork", interface, &best.ssid],
                std::time::Duration::from_millis(3500),
            )
        } else {
            super::cmd::run(
                &[
                    "networksetup",
                    "-setairportnetwork",
                    interface,
                    &best.ssid,
                    wifi_password,
                ],
                std::time::Duration::from_millis(3500),
            )
        };
        if !out.success {
            return None;
        }
    }

    Some(format!(
        "● Smart-Roamed to '{}' ({} Ch{}, {}dBm, +{} score)",
        best.ssid,
        best.band.label(),
        best.channel_num,
        best.signal_dbm,
        best.gain_score
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_eval(sticky: bool, gain: i64) -> RoamEval {
        RoamEval {
            sticky_client: sticky,
            active_score: 40,
            best_candidate: Some(Candidate {
                ssid: "HomeWiFi-5G".into(),
                band: Band::Band5GHz,
                channel_num: 36,
                signal_dbm: -55,
                score: 40 + gain,
                gain_score: gain,
            }),
            verdict: "STICKY".into(),
        }
    }

    #[test]
    fn smart_roam_refuses_when_game_locked() {
        let mut cd = RoamCooldown::default();
        let eval = sample_eval(true, 18);
        assert!(execute_smart_roam("en0", &eval, "", true, &mut cd, false).is_none());
        assert_eq!(cd.attempts, 0);
    }

    #[test]
    fn smart_roam_succeeds_and_enforces_cooldown() {
        let mut cd = RoamCooldown::default();
        let eval = sample_eval(true, 18);
        let msg = execute_smart_roam("en0", &eval, "", false, &mut cd, false);
        assert!(msg.is_some());
        assert_eq!(cd.attempts, 1);
        // Immediate second attempt is blocked by 90s cooldown
        assert!(execute_smart_roam("en0", &eval, "", false, &mut cd, false).is_none());
        assert_eq!(cd.attempts, 1);
    }

    #[test]
    fn non_dfs_clean_5g_scores_higher_than_colliding_dfs() {
        let dfs_clash = score_ap_candidate_channel(-62, Band::Band5GHz, 80, 132, true);
        let clean_unii1 = score_ap_candidate_channel(-64, Band::Band5GHz, 80, 36, false);
        assert!(clean_unii1 - dfs_clash >= 12);
    }
}
