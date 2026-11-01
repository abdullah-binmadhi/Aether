//! Composite 0-100 Wi-Fi health score.
//!
//! Ported line-for-line from the Python `compute_health_score`, including the
//! same penalty weights, thresholds, and alert strings.

use super::model::{Band, HealthRating, Telemetry};

pub struct HealthResult {
    pub score: i64,
    pub rating: HealthRating,
    pub alerts: Vec<String>,
}

/// `has_5g_sibling` mirrors the original `sibling_5g_ssid` truthiness check: the
/// 2.4GHz penalty only applies when a faster band is actually available.
pub fn compute_health(
    tel: &Telemetry,
    ping_ms: f64,
    loss_pct: f64,
    has_5g_sibling: bool,
) -> HealthResult {
    if !tel.link_active || tel.band == Band::Offline {
        return HealthResult {
            score: 0,
            rating: HealthRating::Offline,
            alerts: vec!["Wi-Fi is disconnected.".into()],
        };
    }

    let mut score: i64 = 100;
    let mut alerts: Vec<String> = Vec::new();

    // 1. Band check (2.4GHz penalty only with a 5GHz sibling and not on a hotspot).
    if tel.band == Band::Band2_4GHz && has_5g_sibling && !tel.is_hotspot {
        score -= 22;
        alerts.push("Downgraded to 2.4GHz band. Press [B] to lock 5GHz.".into());
    }

    // 2. Signal / SNR.
    if tel.snr_db < 18 {
        score -= 25;
        alerts.push(format!("Weak Signal-to-Noise ratio ({} dB).", tel.snr_db));
    } else if tel.snr_db < 25 {
        score -= 10;
    }

    // 3. Sticky low-MCS (strong signal on 5/6GHz but MCS < 4).
    if tel.band.is_fast() && tel.signal_dbm >= -64 && tel.mcs_index > 0 && tel.mcs_index < 4 {
        score -= 18;
        alerts.push(format!(
            "Sticky Low-MCS bug (Signal {} dBm, MCS={}). Press [H] to cure.",
            tel.signal_dbm, tel.mcs_index
        ));
    }

    // 4. Co-channel collision.
    for other in &tel.neighbors {
        if other.channel_num == tel.channel_num && tel.channel_num > 0 && other.signal_dbm >= -70 {
            score -= 14;
            alerts.push(format!(
                "Co-channel collision on Ch {} ({} dBm neighbor).",
                tel.channel_num, other.signal_dbm
            ));
            break;
        }
    }

    // 5. AWDL background hopping.
    if tel.awdl_active {
        score -= 6;
        alerts.push("AWDL background hopping active. Press [L] for Zero-Lag mode.".into());
    }

    // 6. Ping & loss.
    if loss_pct > 2.0 {
        score -= ((loss_pct * 3.0) as i64).min(30);
        alerts.push(format!("Packet loss detected ({loss_pct:.1}%)."));
    }
    if ping_ms > 80.0 {
        score -= 15;
        alerts.push(format!("Elevated ping latency ({ping_ms:.0} ms)."));
    }

    score = score.clamp(5, 100);
    let rating = if score >= 88 {
        HealthRating::Optimal
    } else if score >= 72 {
        HealthRating::Good
    } else if score >= 50 {
        HealthRating::Degraded
    } else {
        HealthRating::Critical
    };

    HealthResult {
        score,
        rating,
        alerts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_5ghz_link_scores_100() {
        let mut tel = Telemetry {
            band: Band::Band5GHz,
            channel_num: 36,
            signal_dbm: -55,
            snr_db: 37,
            tx_rate_mbps: 866,
            mcs_index: 9,
            awdl_active: false,
            link_active: true,
            ..Telemetry::default()
        };
        tel.neighbors.clear();
        let r = compute_health(&tel, 12.0, 0.0, false);
        assert_eq!(r.score, 100);
        assert_eq!(r.rating, HealthRating::Optimal);
    }

    #[test]
    fn two_point_four_with_sibling_is_penalized() {
        let tel = Telemetry {
            band: Band::Band2_4GHz,
            snr_db: 30,
            link_active: true,
            is_hotspot: false,
            ..Telemetry::default()
        };
        let with = compute_health(&tel, 10.0, 0.0, true).score;
        let without = compute_health(&tel, 10.0, 0.0, false).score;
        assert_eq!(without - with, 22);
    }
}
