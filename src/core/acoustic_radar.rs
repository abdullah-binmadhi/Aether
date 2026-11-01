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

/// Live terminal sweet-spot locator with Geiger-style acoustic ticks (terminal
/// bell). Ported from `run_live_locator`. Blocks for `duration_sec`.
pub fn run_live_locator(interface: &str, duration_sec: f64, audio: bool) {
    use std::io::Write;
    use std::time::{Duration, Instant};

    let mut collector = super::collector::Collector::new(interface);
    let mut scope = super::PingScope::new(interface);
    let end = Instant::now() + Duration::from_secs_f64(duration_sec);

    println!("\n// ACOUSTIC SWEET-SPOT RADAR // MOVE YOUR LAPTOP TO FIND MAXIMUM RF //");
    println!("{}", "=".repeat(76));
    println!("  Press Ctrl+C to stop locator.\n");

    let mut last_tick = Instant::now();
    while Instant::now() < end {
        let snap = scope.probe();
        let tel = collector.collect(snap.avg_ms, snap.loss_pct);
        let q = calculate_quality_score(&tel);

        if last_tick.elapsed() >= Duration::from_millis(q.interval_ms.max(1) as u64) {
            if audio {
                print!("\x07"); // terminal bell = Geiger tick
                let _ = std::io::stdout().flush();
            }
            last_tick = Instant::now();
        }

        print!(
            "\r  {} │ {:<20} │ {}dBm SNR {}dB (MCS {}) │ {:4.1} clicks/s",
            q.meter, q.rating, q.signal_dbm, q.snr_db, q.mcs_index, q.click_hz
        );
        let _ = std::io::stdout().flush();
        std::thread::sleep(Duration::from_millis(80));
    }
    println!("\n\n[OK] Sweet-Spot Radar scan stopped.");
}
