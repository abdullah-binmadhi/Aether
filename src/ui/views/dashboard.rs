//! Tab [1] OVERVIEW — BI-dashboard layout:
//!   KPI scorecard strip  (Health · Ping · Signal · Link rate)
//!   2x2 tile grid        (RF gauges · Latency oscilloscope · 4-hop bars · Advice)
//! Fills the whole body — no empty lower half, nothing truncates mid-word.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::render::RenderState;
use crate::ui::theme::{alert, amber, amber_bold, good, score_color, sparkline};
use crate::ui::widgets::{hbar_row, render_2d_vertical_graph};
use crate::ui::{draw_kpi_strip, grid, titled_block, Kpi};

pub fn render(f: &mut Frame, rs: &RenderState, area: Rect) {
    // KPI strip (4 rows tall) on top, tile grid fills the rest.
    let rows = Layout::vertical([Constraint::Length(4), Constraint::Min(6)]).split(area);
    draw_kpi(f, rs, rows[0]);

    let tiles = grid(rows[1], 2, 2);
    render_rf_gauges(f, rs, tiles[0]);
    render_oscilloscope(f, rs, tiles[1]);
    render_hop_bars(f, rs, tiles[2]);
    render_advice(f, rs, tiles[3]);
}

fn draw_kpi(f: &mut Frame, rs: &RenderState, area: Rect) {
    let tel = &rs.telemetry;
    let health_accent = score_color(tel.health_score);
    let sig_hist = &rs.socket_sample.signal_history;
    let rate_hist: Vec<f64> = vec![tel.tx_rate_mbps as f64]; // link rate is mostly stable
    let cards = [
        Kpi {
            title: "HEALTH",
            value: format!("{}/100", tel.health_score),
            unit: tel.health_rating.label(),
            history: &rs.scope_rtt_history, // visual trend proxy
            floor: 0.0,
            accent: Some(health_accent),
        },
        Kpi {
            title: "PING",
            value: format!("{:.1}", rs.scope_avg_ms),
            unit: "ms",
            history: &rs.scope_rtt_history,
            floor: 0.0,
            accent: None,
        },
        Kpi {
            title: "SIGNAL",
            value: format!("{}", tel.signal_dbm),
            unit: "dBm",
            history: sig_hist,
            floor: -95.0,
            accent: None,
        },
        Kpi {
            title: "LINK RATE",
            value: format!("{}", tel.tx_rate_mbps),
            unit: "Mbps",
            history: &rate_hist,
            floor: 0.0,
            accent: None,
        },
    ];
    draw_kpi_strip(f, area, &cards);
}

/// Tile 1: RF quality as a stack of labelled horizontal gauges.
fn render_rf_gauges(f: &mut Frame, rs: &RenderState, area: Rect) {
    let tel = &rs.telemetry;
    let block = titled_block("RF QUALITY");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let bw = (inner.width as usize).saturating_sub(24).clamp(8, 24);

    let gauges = [
        ("Signal", tel.signal_dbm as f64, -90.0, -40.0, "dBm"),
        ("SNR", tel.snr_db as f64, 5.0, 45.0, "dB"),
        ("MCS", tel.mcs_index as f64, 0.0, 11.0, "/11"),
        ("Airtime", tel.airtime_load_pct as f64, 0.0, 100.0, "%"),
        ("Retry", tel.tx_retry_pct, 0.0, 20.0, "%"),
    ];
    let mut lines = Vec::new();
    for (label, v, lo, hi, unit) in gauges {
        // Colour: Signal/SNR/MCS good when high; Airtime/Retry good when low.
        let ratio = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
        let healthy_high = !matches!(label, "Airtime" | "Retry");
        let good_val = if healthy_high {
            ratio >= 0.5
        } else {
            ratio <= 0.4
        };
        let style = if good_val { good() } else { amber() };
        lines.push(Line::from(Span::styled(
            hbar_row(label, v, lo, hi, unit, bw),
            style,
        )));
    }
    lines.push(Line::from(Span::styled(
        format!(
            "DNS {} · MTU {} · {}",
            tel.dns_servers
                .first()
                .cloned()
                .unwrap_or_else(|| "Auto".into()),
            tel.mtu,
            if tel.awdl_active {
                "AWDL STD"
            } else {
                "ZERO-LAG"
            }
        ),
        if tel.awdl_active { amber() } else { good() },
    )));

    // Fill the rest of the panel with a tall signal-over-time graph + SNR/Retry
    // sparklines, sized from the real leftover height so it always fits.
    let used = lines.len();
    let leftover = (inner.height as usize).saturating_sub(used + 2);
    if leftover >= 3 {
        let ss = &rs.socket_sample;
        let sig_hist = if ss.signal_history.is_empty() {
            vec![tel.signal_dbm as f64]
        } else {
            ss.signal_history.clone()
        };
        lines.push(Line::from(Span::styled(
            "── SIGNAL OVER TIME ──",
            amber_bold(),
        )));
        let gh = leftover.saturating_sub(2).max(2);
        for g in render_2d_vertical_graph(&sig_hist, gh, inner.width as usize, -90.0, -35.0, "d") {
            lines.push(Line::from(Span::styled(g, amber())));
        }
        let sw = (inner.width as usize).saturating_sub(16).max(10);
        lines.push(Line::from(Span::styled(
            format!(
                "SNR   {} {}dB",
                sparkline(&ss.snr_history, sw, 5.0),
                tel.snr_db
            ),
            good(),
        )));
        lines.push(Line::from(Span::styled(
            format!(
                "RETRY {} {:.1}%",
                sparkline(&ss.retry_history, sw, 0.0),
                tel.tx_retry_pct
            ),
            if tel.tx_retry_pct > 8.0 {
                alert()
            } else {
                amber()
            },
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// Tile 2: tall oscilloscope. [M] / [ ] cycles which series it plots so the key
/// has an obvious visible effect on Overview:
///   1 = LATENCY (RTT)   2 = DOWNLOAD   3 = UPLOAD   4 = COMBINED
fn render_oscilloscope(f: &mut Frame, rs: &RenderState, area: Rect) {
    let mode = ((rs.visual_mode.wrapping_sub(1)) % 4) + 1;
    let (title, series, floor, max, unit, scolor): (&str, &[f64], f64, f64, &str, _) = match mode {
        2 => (
            "THROUGHPUT OSCILLOSCOPE ▼ DOWNLOAD  [M] switch view",
            &rs.scope_down_history,
            0.0,
            rs.scope_down_history
                .iter()
                .cloned()
                .fold(10.0_f64, f64::max),
            "K",
            good(),
        ),
        3 => (
            "THROUGHPUT OSCILLOSCOPE ▲ UPLOAD  [M] switch view",
            &rs.scope_up_history,
            0.0,
            rs.scope_up_history.iter().cloned().fold(10.0_f64, f64::max),
            "K",
            amber_bold(),
        ),
        4 => (
            "COMBINED OSCILLOSCOPE (RTT + ▼▲)  [M] switch view",
            &rs.scope_rtt_history,
            0.0,
            50.0,
            "m",
            amber(),
        ),
        _ => (
            "LATENCY OSCILLOSCOPE (RTT)  [M] switch view",
            &rs.scope_rtt_history,
            0.0,
            50.0,
            "m",
            amber(),
        ),
    };

    let block = titled_block(title);
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 3 {
        return;
    }
    let w = inner.width as usize;
    // Reserve 3 lines for the header + down/up sparks; rest is the main graph.
    let graph_h = (inner.height as usize).saturating_sub(3).max(2);
    let mut lines = vec![Line::from(Span::styled(
        format!(
            "PING {:.1}ms · Jit {:.1}ms · Loss {:.1}% · ▼{:.0}K ▲{:.0}K",
            rs.scope_avg_ms,
            rs.scope_jitter_ms,
            rs.scope_loss_pct,
            rs.scope_down_kbs,
            rs.scope_up_kbs
        ),
        amber_bold(),
    ))];
    for g in render_2d_vertical_graph(series, graph_h, w, floor, max, unit) {
        lines.push(Line::from(Span::styled(g, scolor)));
    }
    let sw = w.saturating_sub(14).max(8);
    lines.push(Line::from(Span::styled(
        format!(
            "DOWN {} {:.0}K",
            sparkline(&rs.scope_down_history, sw, 1.0),
            rs.scope_down_kbs
        ),
        good(),
    )));
    lines.push(Line::from(Span::styled(
        format!(
            "UP   {} {:.0}K",
            sparkline(&rs.scope_up_history, sw, 1.0),
            rs.scope_up_kbs
        ),
        amber(),
    )));
    f.render_widget(Paragraph::new(lines), inner);
}

/// Tile 3: one bar per hop, length ∝ RTT, colour = status.
fn render_hop_bars(f: &mut Frame, rs: &RenderState, area: Rect) {
    let block = titled_block("4-HOP PATH");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width as usize;
    // Label column holds the full hop name (e.g. "Google Edge"); size the bar
    // from the REAL inner width so nothing is clipped on a wide panel.
    let label_w = 12usize;
    let bar_max = w.saturating_sub(label_w + 16).clamp(6, 48);

    // Scale bars to the slowest hop so the longest is full-width.
    let max_rtt = rs
        .scope_hops
        .iter()
        .map(|h| h.rtt_ms)
        .fold(1.0_f64, f64::max);

    let mut lines = Vec::new();
    for (hi, h) in rs.scope_hops.iter().enumerate() {
        let filled = ((h.rtt_ms / max_rtt) * bar_max as f64).round() as usize;
        let filled = filled.min(bar_max);
        let g = crate::ui::theme::tex_bar(filled, bar_max, hi);
        let style = if h.status.starts_with("[!]") {
            alert()
        } else {
            good()
        };
        lines.push(Line::from(Span::styled(
            format!(
                "[{}] {:<width$} {} {:>6.1}ms",
                h.hop,
                truncate(&h.label, label_w),
                g,
                h.rtt_ms,
                width = label_w
            ),
            style,
        )));
    }

    // Verdict text (wrapped) — compute now so we can reserve its rows and give
    // the rest of the panel to a tall RTT-over-time graph.
    let vstyle = if rs.scope_fault.starts_with("[OK]") {
        good()
    } else {
        alert()
    };
    let verdict_lines: Vec<String> = {
        let mut v = vec!["VERDICT:".to_string()];
        v.extend(wrap_text(&rs.scope_fault, w));
        v
    };

    // Tall latency-over-time graph fills the gap between the hop bars and the
    // verdict, sized from the real leftover height so nothing is clipped.
    let used = lines.len();
    let reserved = verdict_lines.len() + 2; // spacer + header + verdict block
    let leftover = (inner.height as usize).saturating_sub(used + reserved);
    if leftover >= 3 {
        lines.push(Line::from(Span::styled(
            "── LATENCY OVER TIME ──",
            amber_bold(),
        )));
        let gh = leftover.saturating_sub(1).max(2);
        for g in render_2d_vertical_graph(&rs.scope_rtt_history, gh, w, 0.0, 50.0, "m") {
            lines.push(Line::from(Span::styled(g, amber())));
        }
    }

    lines.push(Line::from(Span::raw("")));
    for l in verdict_lines {
        lines.push(Line::from(Span::styled(l, vstyle)));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// Truncate to `max` display chars with an ellipsis only when genuinely needed.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}

/// Tile 4: big score readout + meter + WHAT TO DO NOW (word-wrapped) + badges.
fn render_advice(f: &mut Frame, rs: &RenderState, area: Rect) {
    let analysis = &rs.cockpit_analysis;
    let score = rs.telemetry.health_score;
    let block = titled_block("WHAT TO DO NOW");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let style = score_color(score);
    let w = inner.width as usize;
    let label = rs.telemetry.health_rating.label();

    let mut lines = Vec::new();
    // Clean gauge header: a meter line on top, then 3-row big-block digits beside
    // the score/label — no crowding, never stray ticks.
    if inner.height >= 6 {
        let digits = crate::ui::widgets::big_number(score);
        let meter = crate::ui::widgets::score_meter(score, w.saturating_sub(4).clamp(10, 50));
        lines.push(Line::from(Span::styled(meter, style)));
        lines.push(Line::from(Span::styled(digits[0].clone(), style)));
        lines.push(Line::from(vec![
            Span::styled(format!("{}  ", digits[1]), style),
            Span::styled(format!("{score}/100  {label}"), style),
        ]));
        lines.push(Line::from(Span::styled(digits[2].clone(), style)));
    } else {
        lines.push(Line::from(Span::styled(
            format!(
                "{}  {score}/100  {label}",
                crate::ui::widgets::score_meter(score, w.saturating_sub(16).clamp(8, 30))
            ),
            style,
        )));
    }

    // Recommended action, word-wrapped so nothing truncates.
    for l in wrap_text(&analysis.recommended_action, w) {
        lines.push(Line::from(Span::styled(l, good())));
    }
    // Supporting analysis lines — fill the remaining panel height (wrapped),
    // leaving one row for the workload badge strip.
    let budget = (inner.height as usize).saturating_sub(lines.len() + 1);
    let mut used = 0usize;
    for l in &analysis.analysis_lines {
        let st = if l.contains("[!]") { alert() } else { amber() };
        for wl in wrap_text(l, w) {
            if used >= budget {
                break;
            }
            lines.push(Line::from(Span::styled(wl, st)));
            used += 1;
        }
        if used >= budget {
            break;
        }
    }

    // Workload readiness badges.
    let mut badges = Vec::new();
    for (name, _v, ok) in &analysis.workloads {
        let short: String = name.split_whitespace().next().unwrap_or(name).to_string();
        badges.push(Span::styled(
            format!(" {}{} ", if *ok { "✓" } else { "✗" }, short),
            if *ok { good() } else { alert() },
        ));
    }
    if !badges.is_empty() {
        lines.push(Line::from(badges));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// Minimal greedy word-wrap for a single paragraph.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(12);
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let cand = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if cand.chars().count() <= width {
            cur = cand;
        } else {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}
