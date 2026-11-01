//! Sweet-spot quality score & Geiger click-rate. Ported from acoustic_radar.py.

use super::model::Telemetry;

pub struct QualityScore {
    pub score: i64,
    pub rating: &'static str,
    pub color_tag: &'static str,
    pub click_hz: f64,
    pub interval_ms: i64,
    pub meter: String,
    pub signal_dbm: i64,
    pub snr_db: i64,
    pub mcs_index: i64,
}

pub fn calculate_quality_score(tel: &Telemetry) -> QualityScore {
    let sig = tel.signal_dbm as f64;
    let snr = tel.snr_db as f64;
    let mcs = tel.mcs_index as f64;
    let tx_rate = tel.tx_rate_mbps as f64;
    let retries = tel.tx_retry_pct;

    let rssi_pts = ((sig + 90.0) * (35.0 / 45.0)).clamp(0.0, 35.0);
    let snr_pts = ((snr - 5.0) * (35.0 / 40.0)).clamp(0.0, 35.0);
    let mcs_pts = (mcs * (20.0 / 11.0)).clamp(0.0, 20.0);
    let tx_pts = (tx_rate / 100.0).clamp(0.0, 10.0);
    let retry_penalty = ((retries - 3.0) * 1.5).clamp(0.0, 15.0);
    let raw = (rssi_pts + snr_pts + mcs_pts + tx_pts - retry_penalty).round() as i64;
    let score = raw.clamp(5, 100);

    let click_hz = 0.5 + ((score as f64 - 5.0) / 95.0) * 15.5;
    let interval_ms = (1000.0 / click_hz.max(0.5)).round() as i64;

    let (rating, color_tag) = if score >= 85 {
        ("PEAK SWEET SPOT", "AMBER_CRT_BRIGHT")
    } else if score >= 70 {
        ("EXCELLENT", "GREEN")
    } else if score >= 50 {
        ("GOOD (STABLE)", "AMBER")
    } else if score >= 30 {
        ("FAIR (WALL LOSS)", "AMBER_DIM")
    } else {
        ("DEAD ZONE (ATTENUATED)", "RED_ALERT")
    };

    let filled = (score as f64 * 0.24).round() as usize;
    let filled = filled.min(24);
    let meter = format!(
        "[{}{}] {}%",
        "■".repeat(filled),
        "░".repeat(24 - filled),
        score
    );

    QualityScore {
        score,
        rating,
        color_tag,
        click_hz,
        interval_ms,
        meter,
        signal_dbm: tel.signal_dbm,
        snr_db: tel.snr_db,
        mcs_index: tel.mcs_index,
    }
}
