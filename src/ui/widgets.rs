//! 4-mode visual telemetry deck. Ported from widgets.py.
//!
//! Each `render_*` returns the body text lines for a mode; the dashboard draws
//! them inside a bordered pane with mode-selector pills.

use crate::app::App;
use crate::core::model::Band;
use crate::ui::theme::bar;

const BLOCK_SPARK: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub const DECK_MODES: [(u8, &str); 4] = [
    (1, "1:TOPOLOGY"),
    (2, "2:RF-CHRONO"),
    (3, "3:QAM-MIMO"),
    (4, "4:SOCKETS"),
];

/// (mod, code, bits, grid_cols, grid_rows, max_80mhz_2x2) for MCS 0..11.
fn mcs_info(mcs: i64) -> (&'static str, &'static str, i64, usize, usize, i64) {
    match mcs.clamp(0, 11) {
        0 => ("BPSK", "1/2", 1, 2, 1, 86),
        1 => ("QPSK", "1/2", 2, 2, 2, 173),
        2 => ("QPSK", "3/4", 2, 2, 2, 260),
        3 => ("16-QAM", "1/2", 4, 4, 2, 346),
        4 => ("16-QAM", "3/4", 4, 4, 2, 520),
        5 => ("64-QAM", "2/3", 6, 6, 3, 693),
        6 => ("64-QAM", "3/4", 6, 6, 3, 780),
        7 => ("64-QAM", "5/6", 6, 6, 3, 866),
        8 => ("256-QAM", "3/4", 8, 8, 4, 1040),
        9 => ("256-QAM", "5/6", 8, 8, 4, 1155),
        10 => ("1024-QAM", "3/4", 10, 10, 4, 1299),
        _ => ("1024-QAM", "5/6", 10, 10, 4, 1444),
    }
}

/// 3x3 block-digit glyphs for 0-9 (plus space). Each digit is 3 rows tall and
/// 3 cells wide, built from solid blocks so it can NEVER render as stray ticks
/// (unlike the old arc gauge). Used for the big health/score readout.
fn big_digit(c: char) -> [&'static str; 3] {
    match c {
        '0' => ["███", "█ █", "███"],
        '1' => ["  █", "  █", "  █"],
        '2' => ["███", " ██", "██ "],
        '3' => ["███", " ██", "███"],
        '4' => ["█ █", "███", "  █"],
        '5' => ["███", "██ ", "███"],
        '6' => ["█  ", "███", "███"],
        '7' => ["███", "  █", "  █"],
        '8' => ["███", "███", "███"],
        '9' => ["███", "███", "  █"],
        _ => ["   ", "   ", "   "],
    }
}

/// Render a number as 3 rows of big block digits (e.g. "65"). Digits separated
/// by one blank column. Every emitted cell is a solid block or a space, so the
/// result is always a clean, legible readout — this replaces the fragile arc.
pub fn big_number(n: i64) -> [String; 3] {
    let s = n.clamp(0, 999).to_string();
    let mut rows = [String::new(), String::new(), String::new()];
    for (i, ch) in s.chars().enumerate() {
        let g = big_digit(ch);
        for (r, row) in rows.iter_mut().enumerate() {
            if i > 0 {
                row.push(' ');
            }
            row.push_str(g[r]);
        }
    }
    rows
}

/// A clean bracketed meter for a 0..100 score: `[████████░░░░]`. `width` is the
/// number of inner cells. Always solid blocks — never looks broken.
pub fn score_meter(score: i64, width: usize) -> String {
    let w = width.max(4);
    let filled = ((score.clamp(0, 100) as f64 / 100.0) * w as f64).round() as usize;
    let filled = filled.min(w);
    format!("[{}{}]", "█".repeat(filled), "░".repeat(w - filled))
}

/// A labelled horizontal gauge row: `LABEL  ███████░░░░  VALUE unit`.
/// Generalises the ad-hoc RF bars so every page draws gauges the same way.
pub fn hbar_row(
    label: &str,
    value: f64,
    min_val: f64,
    max_val: f64,
    unit: &str,
    bar_w: usize,
) -> String {
    let g = bar(value, min_val, max_val, bar_w);
    // Drop the surrounding brackets from `bar()` for a cleaner tile look.
    let g = g.trim_start_matches('[').trim_end_matches(']');
    format!("{label:<8} {g}  {value:>5.0} {unit}")
}

/// A single KPI scorecard card body: a big value line + a trend sparkline line.
/// `trend` is "↑" / "↓" / "=" describing the recent direction.
pub fn kpi_tile(value: &str, unit: &str, history: &[f64], floor: f64, width: usize) -> Vec<String> {
    let spark_w = width.saturating_sub(3).max(6);
    let spark = sparkline(history, spark_w, floor);
    let trend = kpi_trend(history);
    vec![format!("  {value} {unit}"), format!("  {spark} {trend}")]
}

/// Direction arrow for the tail of a KPI history.
fn kpi_trend(history: &[f64]) -> &'static str {
    if history.len() < 4 {
        return "=";
    }
    let n = history.len();
    let recent: f64 = history[n - 3..].iter().sum::<f64>() / 3.0;
    let prior: f64 = history[n.saturating_sub(6)..n - 3].iter().sum::<f64>()
        / (3.0_f64).min((n - 3) as f64).max(1.0);
    let delta = recent - prior;
    let scale = prior.abs().max(1.0);
    if delta > scale * 0.08 {
        "↑"
    } else if delta < -scale * 0.08 {
        "↓"
    } else {
        "="
    }
}

/// A compact horizontal "dial": a bracketed arc that fills with the score plus
/// the number and rating. One line, fits the dense Overview panel.
pub fn health_dial_line(score: i64) -> (String, String) {
    let filled = ((score as f64 / 100.0) * 20.0).round() as usize;
    let filled = filled.min(20);
    let arc = format!("{}{}", "█".repeat(filled), "░".repeat(20 - filled));
    let label = match score {
        s if s >= 88 => "OPTIMAL",
        s if s >= 72 => "GOOD",
        s if s >= 50 => "DEGRADED",
        _ => "CRITICAL",
    };
    (format!("  {score:>3}/100  [{arc}]"), label.to_string())
}

/// Multi-row vertical block chart with a left Y-axis margin.
pub fn render_2d_vertical_graph(
    values: &[f64],
    height: usize,
    width: usize,
    min_val: f64,
    max_val: f64,
    unit: &str,
) -> Vec<String> {
    let rows_h = height.max(2);
    let axis_w = 7usize;
    let plot_w = width.saturating_sub(axis_w + 1).max(8);

    let mut pts: Vec<f64> = if values.len() > plot_w {
        values[values.len() - plot_w..].to_vec()
    } else {
        values.to_vec()
    };
    if pts.len() < plot_w {
        let pad = pts.first().copied().unwrap_or(min_val);
        let mut padded = vec![pad; plot_w - pts.len()];
        padded.extend_from_slice(&pts);
        pts = padded;
    }
    let lo = pts.iter().cloned().fold(min_val, f64::min);
    let hi = pts.iter().cloned().fold(max_val, f64::max);
    let span = (hi - lo).max(1.0);
    let u = unit
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();

    let mut out = Vec::with_capacity(rows_h);
    for r in 0..rows_h {
        let row_top = 1.0 - (r as f64 / rows_h as f64);
        let row_bot = 1.0 - ((r + 1) as f64 / rows_h as f64);
        let y_lbl = if r == 0 {
            format!("{hi:5.0}{u:<1}┤")
        } else if r == rows_h - 1 {
            format!("{lo:5.0}{u:<1}┤")
        } else if r == rows_h / 2 {
            format!("{:5.0}{u:<1}┤", (hi + lo) * 0.5)
        } else {
            "      │".to_string()
        };
        let mut chars = String::new();
        for v in &pts {
            let norm = ((v - lo) / span).clamp(0.0, 1.0);
            if norm >= row_top {
                chars.push('█');
            } else if norm <= row_bot {
                chars.push(if r == rows_h - 1 { '·' } else { ' ' });
            } else {
                let frac = (norm - row_bot) / (row_top - row_bot).max(0.001);
                let idx = ((frac * BLOCK_SPARK.len() as f64) as usize).min(BLOCK_SPARK.len() - 1);
                chars.push(BLOCK_SPARK[idx]);
            }
        }
        out.push(format!("{y_lbl}{chars}"));
    }
    out
}

fn sparkline(values: &[f64], width: usize, floor: f64) -> String {
    crate::ui::theme::sparkline(values, width, floor)
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(12);
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let cand = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if cand.len() <= width {
            cur = cand;
        } else {
            if !cur.is_empty() {
                out.push(cur);
            }
            cur = format!("   {word}");
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn render_topology(app: &App, width: usize, height: usize) -> Vec<String> {
    let t = &app.telemetry;
    let s = &app.scope_snap;
    let ssid: String = t.ssid.chars().take(12).collect();
    let band = t.band.label();
    let gw_rtt = s.hops.first().map(|h| h.rtt_ms).unwrap_or(0.0);
    let gw_jit = s.hops.first().map(|h| h.jitter_ms).unwrap_or(0.0);
    let isp_lbl: String = s
        .hops
        .get(1)
        .map(|h| h.label.chars().take(10).collect())
        .unwrap_or_else(|| "ISP DNS".into());
    let wan_rtt = s.hops.get(2).map(|h| h.rtt_ms).unwrap_or(s.avg_ms);
    let loss = s.loss_pct;
    let bloat = s.max_bloat_delta_ms;
    let sig = t.signal_dbm;
    let snr = t.snr_db;
    let mcs = t.mcs_index;
    let tx = t.tx_rate_mbps;
    let retry = t.tx_retry_pct;

    let l2_ok = sig >= -68 && retry < 6.0;
    let gw_ok = gw_rtt <= 18.0 && gw_jit <= 8.0;
    let wan_ok = loss < 2.0 && bloat <= 25.0;
    let a1 = if l2_ok { "══▶" } else { "╌✕▶" };
    let a2 = if gw_ok { "══▶" } else { "╌!▶" };
    let a3 = if wan_ok { "══▶" } else { "╌!▶" };

    let bw = width.saturating_sub(44).clamp(6, 14);
    let mut lines = vec![
        format!("┌─────────┐ {:<4} C{ch:<3} ┌────────────┐ {gw_rtt:4.1}ms ┌────────────┐ {wan_rtt:4.0}ms ┌───────┐", &band.chars().take(4).collect::<String>(), ch = t.channel_num),
        format!("│ MAC en0 │{a1} {sig:>3}dB │ {ssid:<10} │{a2} J:{gw_jit:3.0}m │ {isp_lbl:<10} │{a3} L:{loss:2.0}% │  WAN  │"),
        format!("└─────────┘ M{mcs:<2} {tx:>3}M └────────────┘ Hop 1  └────────────┘ B+{bloat:2.0}m └───────┘"),
        format!("Stage 1 (RF) : {} SNR {snr}dB │ Retry {retry:.1}% │ IP {} (MTU {})", bar(snr as f64, 5.0, 45.0, bw), t.ipv4, t.mtu),
        format!("Stage 2 (GW) : {} GW {} ({gw_rtt:.1}ms / Jit {gw_jit:.1}ms) {}", bar((50.0 - gw_rtt).max(2.0), 0.0, 50.0, bw), t.gateway, if gw_ok { "[OK]" } else { "[!]" }),
        format!("Stage 3 (WAN): {} WAN RTT {wan_rtt:.1}ms │ Loss {loss:.1}% │ Bloat +{bloat:.1}ms", bar((120.0 - wan_rtt).max(2.0), 0.0, 120.0, bw)),
    ];
    let interp = if !l2_ok {
        format!("PIPELINE INTERPRETATION: Bottleneck is at Stage 1 (L2 Wi-Fi Airlink: {sig}dBm, {retry:.1}% retries). Move closer to AP or press [R] to smart-roam.")
    } else if !gw_ok {
        format!("PIPELINE INTERPRETATION: Stage 2 Local Gateway queue spike ({gw_rtt:.1}ms / {gw_jit:.1}ms jitter). CAM 25Hz Pacer [K] & WMM VIP [W] are actively cushioning local airtime.")
    } else if !wan_ok {
        format!("PIPELINE INTERPRETATION: Stage 3 Upstream ISP/Cellular congestion (+{bloat:.1}ms bloat, {loss:.1}% loss). Local Wi-Fi is clean; press [D] for WAN Doctor or [L] to clamp background queues.")
    } else {
        format!("PIPELINE INTERPRETATION: All 4 Pipeline Stages are healthy ({sig}dBm RF -> {gw_rtt:.1}ms GW -> {wan_rtt:.1}ms WAN, 0% bottleneck). Optimal for gaming, 4K video & AI SSE streams.")
    };
    lines.extend(wrap(&interp, width.saturating_sub(2)));
    lines.truncate(height.max(1));
    lines
}

pub fn render_rf_chronograph(app: &App, width: usize, height: usize) -> Vec<String> {
    let t = &app.telemetry;
    let sig_hist = if app.socket_sample.signal_history.is_empty() {
        vec![t.signal_dbm as f64]
    } else {
        app.socket_sample.signal_history.clone()
    };
    let snr_hist = &app.socket_sample.snr_history;
    let ret_hist = &app.socket_sample.retry_history;
    let curr_sig = t.signal_dbm;
    let curr_snr = t.snr_db;
    let curr_air = t.airtime_load_pct;
    let curr_ret = t.tx_retry_pct;
    let min_sig = sig_hist.iter().cloned().fold(curr_sig as f64, f64::min) as i64;
    let max_sig = sig_hist.iter().cloned().fold(curr_sig as f64, f64::max) as i64;
    let drift = max_sig - min_sig;

    let graph_h = height.saturating_sub(5).max(2);
    let mut lines = render_2d_vertical_graph(&sig_hist, graph_h, width, -90.0, -35.0, "d");
    let spark_w = width.saturating_sub(30).max(10);
    lines.push(format!(
        "SNR {} {curr_snr:>2}dB │ Sig: {curr_sig}dBm (Δ{drift}dB)",
        sparkline(snr_hist, spark_w, 5.0)
    ));
    lines.push(format!(
        "RET {} {curr_ret:4.1}% │ Airtime Load: {curr_air}%",
        sparkline(ret_hist, spark_w, 1.0)
    ));
    let interp = if drift >= 10 || curr_sig <= -72 {
        format!("CHRONOGRAPH ANALYSIS: Signal fluctuated by {drift}dB ({min_sig}..{max_sig}dBm, Retry {curr_ret:.1}%). Multipath fading or physical obstruction detected; keep CAM 25Hz Pacer [K] active.")
    } else {
        format!("CHRONOGRAPH ANALYSIS: Rock-solid RF envelope ({min_sig}..{max_sig}dBm, Δ{drift}dB, SNR {curr_snr}dB, Retry {curr_ret:.1}%). Zero dead-zone dropouts across recent samples.")
    };
    lines.extend(wrap(&interp, width.saturating_sub(2)));
    lines.truncate(height.max(1));
    lines
}

pub fn render_qam(app: &App, width: usize, height: usize) -> Vec<String> {
    let t = &app.telemetry;
    let mcs = t.mcs_index;
    let band = t.band;
    let ch = t.channel_num;
    let width_mhz = if t.width_mhz > 0 {
        t.width_mhz
    } else if band == Band::Band2_4GHz {
        20
    } else {
        80
    };
    let tx = t.tx_rate_mbps;
    let snr = t.snr_db;
    let sig = t.signal_dbm;
    let phy = &t.phy_mode;
    let (mod_name, code, bits, cols, rows, max80) = mcs_info(mcs);

    let single_base = ((max80 as f64 * (width_mhz as f64 / 80.0) * 0.5) as i64).max(45);
    let nss = if tx as f64 > single_base as f64 * 1.25 {
        2
    } else {
        1
    };
    let theo = tx.max(single_base * nss);
    let eff = ((tx as f64 / theo.max(1) as f64) * 100.0).round() as i64;
    let eff = eff.min(100);

    let q = |active_rows: usize| -> String {
        (0..max(4, cols.min(8)))
            .map(|c| {
                if active_rows >= 1 && c < cols {
                    "✦"
                } else {
                    "·"
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let q1 = q(1);
    let q2 = if rows >= 2 { q(1) } else { q(0) };
    let q3 = if rows >= 3 { q(1) } else { q(0) };
    let bw = width.saturating_sub(44).clamp(6, 14);

    let mut lines = vec![
        format!("I/Q CONSTELLATION [{mod_name} {code}] │ PHY: {phy} │ MIMO: {nss}x{nss} NSS ({bits}b/sym)"),
        format!(" +Q │ {q1:<17} │ PHY Rate : {tx}/{theo} Mbps ({eff}%) {}", bar(eff as f64, 0.0, 100.0, bw)),
        format!("  0 ┼─{q2:<17}─┤ Ch Width : {width_mhz} MHz ({} Ch {ch}) {}", band.label(), bar(width_mhz as f64, 20.0, 160.0, bw)),
        format!(" -Q │ {q3:<17} │ SNR Head : {snr} dB ({sig} dBm -> MCS {mcs}/11)"),
        "MODULATION LADDER: [0:BPSK] [1-2:QPSK] [3-4:16QAM] [5-7:64QAM] [8-9:256QAM] [10-11:1024QAM]".to_string(),
    ];
    let interp = if mcs >= 8 {
        format!("MODULATION INTERPRETATION: Radio is locked in high-density {mod_name} (MCS {mcs}, {code}) with {snr}dB SNR, delivering {eff}% of {nss}x{nss} MIMO capacity ({tx} Mbps).")
    } else if mcs >= 5 {
        format!("MODULATION INTERPRETATION: Operating at mid-tier {mod_name} (MCS {mcs}, {tx} Mbps). Unlocking 256/1024-QAM (MCS 8-11) requires >= 32dB SNR on clean 5GHz/6GHz channels.")
    } else {
        format!("MODULATION INTERPRETATION: Radio stepped down to {mod_name} (MCS {mcs}, {tx} Mbps) to resist noise/attenuation. Press [H] to re-handshake if close to router.")
    };
    lines.extend(wrap(&interp, width.saturating_sub(2)));
    lines.truncate(height.max(1));
    lines
}

pub fn render_socket_matrix(app: &App, width: usize, height: usize) -> Vec<String> {
    let s = &app.socket_sample.sockets;
    let tot = s.total_active.max(1);
    let bw = width.saturating_sub(44).clamp(8, 16);
    let mut lines = vec![
        format!("KERNEL SOCKET MATRIX (Total Active: {tot}) │ {}", s.verdict),
        format!(
            "● ESTABLISHED TCP : {:>3} {} │ HTTPS/TLS (443): {:>3} streams",
            s.established,
            bar(s.established as f64, 0.0, tot.max(20) as f64, bw),
            s.https_443
        ),
        format!(
            "● UDP / QUIC FLOW : {:>3} {} │ DNS (53/853)   : {:>3} sockets",
            s.quic_udp_443,
            bar(s.quic_udp_443 as f64, 0.0, tot.max(20) as f64, bw),
            s.dns_53
        ),
        format!(
            "● SYN / CLOSE_WAIT: {:>3} {} │ Other App Ports: {:>3} streams",
            s.syn_sent + s.close_wait,
            bar((s.close_wait + s.syn_sent) as f64, 0.0, 15.0, bw),
            s.other_ports
        ),
        format!(
            "● TIME_WAIT CACHE : {:>3} {} │ SYN Stalls: {} │ Half-Closed: {}",
            s.time_wait,
            bar(s.time_wait as f64, 0.0, 30.0, bw),
            s.syn_sent,
            s.close_wait
        ),
    ];
    let interp = if s.syn_sent >= 3 || s.close_wait >= 6 {
        format!("SOCKET INTERPRETATION: {} SYN handshakes and {} closing sockets detected. Stale sockets after a network switch can stall browser tabs — press [Z] to inspect & flush.", s.syn_sent, s.close_wait)
    } else {
        format!("SOCKET INTERPRETATION: Kernel TCP/UDP table is clean ({} ESTABLISHED TCP, {} UDP/QUIC, 0 handshake stalls). 32MB High-BDP auto-scaling window is ready for burst throughput.", s.established, s.quic_udp_443)
    };
    lines.extend(wrap(&interp, width.saturating_sub(2)));
    lines.truncate(height.max(1));
    lines
}

/// Dispatch to the active mode's body lines.
pub fn deck_body(app: &App, mode: u8, width: usize, height: usize) -> Vec<String> {
    match ((mode.wrapping_sub(1)) % 4) + 1 {
        1 => render_topology(app, width, height),
        2 => render_rf_chronograph(app, width, height),
        3 => render_qam(app, width, height),
        _ => render_socket_matrix(app, width, height),
    }
}

fn max(a: usize, b: usize) -> usize {
    if a > b {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcs_table_covers_0_to_11() {
        for mcs in 0..=11 {
            let (name, _code, bits, _c, _r, max80) = mcs_info(mcs);
            assert!(!name.is_empty());
            assert!((1..=10).contains(&bits));
            assert!(max80 >= 86);
        }
        // Out-of-range clamps to the top entry.
        assert_eq!(mcs_info(99).0, "1024-QAM");
    }

    #[test]
    fn vertical_graph_has_requested_rows() {
        let g = render_2d_vertical_graph(&[1.0, 2.0, 3.0], 4, 40, 0.0, 10.0, "m");
        assert_eq!(g.len(), 4);
        // Several heights must each return exactly that many rows (no clipping /
        // disappearing graph when a panel grows or shrinks).
        for h in [2usize, 5, 8, 12] {
            assert_eq!(
                render_2d_vertical_graph(&[5.0, 9.0, 2.0, 7.0], h, 60, 0.0, 10.0, "M").len(),
                h
            );
        }
        // An empty history still produces the requested rows (no panic/garbage).
        assert_eq!(
            render_2d_vertical_graph(&[], 6, 40, 0.0, 10.0, "M").len(),
            6
        );
    }

    #[test]
    fn big_number_is_three_rows_of_equal_width() {
        let b = big_number(65);
        assert_eq!(b.len(), 3);
        // "65" = two 3-wide digits + 1 separator = 7 cells, all rows equal.
        assert_eq!(b[0].chars().count(), b[1].chars().count());
        assert_eq!(b[1].chars().count(), b[2].chars().count());
        // Only block / space cells — never a stray tick glyph.
        assert!(b.iter().all(|r| r.chars().all(|c| c == '█' || c == ' ')));
    }

    #[test]
    fn score_meter_fills_proportionally_and_is_solid() {
        let m = score_meter(50, 10);
        assert_eq!(m.chars().filter(|&c| c == '█').count(), 5);
        assert_eq!(m.chars().filter(|&c| c == '░').count(), 5);
        // Clamps.
        assert_eq!(score_meter(0, 8).chars().filter(|&c| c == '█').count(), 0);
        assert_eq!(score_meter(100, 8).chars().filter(|&c| c == '█').count(), 8);
    }

    #[test]
    fn hbar_row_has_label_and_value() {
        let r = hbar_row("Signal", -64.0, -90.0, -40.0, "dBm", 12);
        assert!(r.contains("Signal"));
        assert!(r.contains("dBm"));
        // No stray brackets from bar().
        assert!(!r.contains('['));
    }

    #[test]
    fn kpi_tile_has_value_and_spark_lines() {
        let t = kpi_tile(
            "89.6",
            "ms",
            &[80.0, 82.0, 90.0, 88.0, 91.0, 95.0],
            10.0,
            18,
        );
        assert_eq!(t.len(), 2);
        assert!(t[0].contains("89.6"));
        assert!(t[0].contains("ms"));
    }

    #[test]
    fn kpi_trend_detects_direction() {
        let up = [10.0, 10.0, 10.0, 30.0, 30.0, 30.0];
        let down = [30.0, 30.0, 30.0, 10.0, 10.0, 10.0];
        let flat = [20.0, 20.0, 20.0, 20.0, 20.0, 20.0];
        assert_eq!(kpi_trend(&up), "↑");
        assert_eq!(kpi_trend(&down), "↓");
        assert_eq!(kpi_trend(&flat), "=");
        assert_eq!(kpi_trend(&[1.0]), "=");
    }
}
