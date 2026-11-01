//! Tab [2] Spectrum: 5GHz/2.4GHz occupancy heatmap + de-redacted neighbour list.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::spectrum;
use crate::render::RenderState;
use crate::ui::theme::{alert, amber, amber_bold, good, score_color, sparkline};
use crate::ui::widgets::{big_number, hbar_row, render_2d_vertical_graph, score_meter};
use crate::ui::{draw_kpi_strip, titled_block, Kpi};

pub fn render(f: &mut Frame, rs: &RenderState, area: Rect) {
    // KPI strip · (congestion | sweet-spot) · (networks | RF scanner) · chronograph.
    let rows = Layout::vertical([
        Constraint::Length(4), // KPI strip
        Constraint::Length(9), // congestion + sweet-spot
        Constraint::Length(9), // networks + RF scanner
        Constraint::Min(6),    // signal chronograph (full width)
    ])
    .split(area);
    draw_kpi(f, rs, rows[0]);

    let r1 =
        Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)]).split(rows[1]);
    render_heatmap(f, rs, r1[0]);
    render_sweetspot(f, rs, r1[1]);

    let r2 =
        Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)]).split(rows[2]);
    render_neighbors(f, rs, r2[0]);
    render_rf_scanner(f, rs, r2[1]);

    render_chronograph(f, rs, rows[3]);
}

fn draw_kpi(f: &mut Frame, rs: &RenderState, area: Rect) {
    let tel = &rs.telemetry;
    let rep = &rs.spectrum;
    // No stored RF score on the report; derive a simple airspace cleanliness
    // score: start at 100, penalise collisions and heavy neighbour density.
    let neighbors = tel.neighbors.len() as i64;
    let rf_score = (100 - rep.collisions.len() as i64 * 25 - (neighbors / 3).min(20)).clamp(0, 100);
    let sig_hist = &rs.socket_sample.signal_history;
    let empty: Vec<f64> = Vec::new();
    let cards = [
        Kpi {
            title: "RF HEALTH",
            value: format!("{}/100", rf_score),
            unit: if rep.collisions.is_empty() {
                "CLEAN"
            } else {
                "CLASH"
            },
            history: sig_hist,
            floor: -95.0,
            accent: Some(score_color(rf_score)),
        },
        Kpi {
            title: "CHANNEL",
            value: format!("{}", tel.channel_num),
            unit: tel.band.label(),
            history: &empty,
            floor: 0.0,
            accent: None,
        },
        Kpi {
            title: "WIDTH",
            value: format!("{}", if tel.width_mhz > 0 { tel.width_mhz } else { 80 }),
            unit: "MHz",
            history: &empty,
            floor: 0.0,
            accent: None,
        },
        Kpi {
            title: "NEIGHBORS",
            value: format!("{}", neighbors),
            unit: "APs",
            history: &empty,
            floor: 0.0,
            accent: if rep.collisions.is_empty() {
                None
            } else {
                Some(alert())
            },
        },
    ];
    draw_kpi_strip(f, area, &cards);
}

fn render_heatmap(f: &mut Frame, rs: &RenderState, area: Rect) {
    let report = spectrum::analyze(&rs.telemetry);
    let block = titled_block("5GHz & 2.4GHz SPECTRUM CONGESTION & CHANNEL OVERLAP");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width as usize;

    // Build one row per block (+ a 2.4GHz row). Each is a big occupancy bar
    // whose length ∝ neighbour count (scaled to the busiest block), colour by
    // state, with the dBm floor + verdict on the right. This fills the panel
    // with a readable chart instead of a short table over dead space.
    struct Row {
        label: String,
        count: u32,
        rssi: i64,
        colliding: bool,
        mine: bool,
    }
    let mut data: Vec<Row> = report
        .blocks_5g
        .iter()
        .map(|b| Row {
            label: b.label.to_string(),
            count: b.count,
            rssi: if b.count > 0 { b.max_rssi } else { -100 },
            colliding: b.colliding,
            mine: b.mine,
        })
        .collect();
    data.push(Row {
        label: "2.4GHz (1/6/11)".into(),
        count: report.count_24,
        rssi: if report.count_24 > 0 {
            report.max_rssi_24
        } else {
            -100
        },
        colliding: false,
        mine: rs.telemetry.band == crate::core::model::Band::Band2_4GHz,
    });

    let busiest = data.iter().map(|r| r.count).max().unwrap_or(1).max(1);
    // Bar column = inner width minus label(20) + count/dBm/verdict text (~30).
    let bar_w = w.saturating_sub(52).clamp(8, 60);

    let mut lines = vec![Line::from(Span::styled(
        format!(
            "{:<20} {:<22} NEIGHBORS · PEAK · STATE",
            "BAND / BLOCK", "OCCUPANCY"
        ),
        amber_bold(),
    ))];

    for r in &data {
        let filled = ((r.count as f64 / busiest as f64) * bar_w as f64).round() as usize;
        let filled = filled.min(bar_w);
        // Clear blocks show an empty track (spaces), not a dim ░ bar — a clear
        // channel should read as clear, not as noise.
        let track = if r.count == 0 { ' ' } else { '░' };
        let gbar = format!(
            "{}{}",
            "█".repeat(filled),
            track.to_string().repeat(bar_w - filled)
        );
        let (state, style) = if r.colliding {
            ("[!] COLLISION", alert())
        } else if r.mine {
            ("● YOUR BLOCK", amber_bold())
        } else if r.count == 0 {
            ("[OK] CLEAR", good())
        } else {
            ("occupied", amber())
        };
        let rssi_str = if r.count > 0 {
            format!("{} dBm", r.rssi)
        } else {
            "clear".into()
        };
        lines.push(Line::from(Span::styled(
            format!(
                "{:<20} {gbar} {:>3} · {:<8} · {state}",
                truncate(&r.label, 20),
                r.count,
                rssi_str
            ),
            style,
        )));
    }

    // Spacer + recommendation banner, word-wrapped, as the bottom anchor.
    lines.push(Line::from(Span::raw("")));
    let banner = format!(
        "◆ RECOMMENDED: Channel {} — {}  │  {} co-channel collision(s)",
        report.recommended_5g_channel,
        report.recommendation_reason,
        report.collisions.len()
    );
    for l in wrap_text(&banner, w) {
        lines.push(Line::from(Span::styled(l, good())));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_neighbors(f: &mut Frame, rs: &RenderState, area: Rect) {
    let tel = &rs.telemetry;
    let block = titled_block("DETECTED LOCAL WI-FI NETWORKS");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width as usize;

    // Compact one-line RSSI rows (the panel is now a sensible height in a
    // narrower column, not an oversized void). Your AP first, then neighbours
    // by signal strength.
    let bar_w = w.saturating_sub(34).clamp(6, 40);
    let rssi_bar = |dbm: i64| -> String {
        let ratio = ((dbm as f64 + 95.0) / 55.0).clamp(0.0, 1.0);
        let filled = (ratio * bar_w as f64).round() as usize;
        format!(
            "{}{}",
            "█".repeat(filled.min(bar_w)),
            "░".repeat(bar_w - filled.min(bar_w))
        )
    };

    let mut lines = vec![Line::from(Span::styled(
        format!(
            "● {:<16} {:<11} {} {:>4}",
            truncate(&tel.ssid, 16),
            truncate(&tel.channel_label, 11),
            rssi_bar(tel.signal_dbm),
            tel.signal_dbm
        ),
        good(),
    ))];

    let mut neigh: Vec<&crate::core::model::Neighbor> = tel.neighbors.iter().collect();
    neigh.sort_by_key(|a| std::cmp::Reverse(a.signal_dbm));
    let cap = (inner.height as usize).saturating_sub(2);
    for n in neigh.iter().take(cap) {
        let style = if n.same_channel { alert() } else { amber() };
        lines.push(Line::from(Span::styled(
            format!(
                "  {:<16} {:<11} {} {:>4}",
                truncate(&n.label, 16),
                format!("Ch{} {}", n.channel_num, n.band.label()),
                rssi_bar(n.signal_dbm),
                n.signal_dbm
            ),
            style,
        )));
    }

    let same_ch = tel.neighbors.iter().filter(|n| n.same_channel).count();
    lines.push(Line::from(Span::styled(
        format!(
            "— {} net · {} co-channel · strongest {} dBm",
            tel.neighbors.len(),
            same_ch,
            tel.neighbors
                .iter()
                .map(|n| n.signal_dbm)
                .max()
                .unwrap_or(tel.signal_dbm)
        ),
        amber_bold(),
    )));
    f.render_widget(Paragraph::new(lines), inner);
}

/// ★ Always-on Sweet-Spot Locator panel: big score, meter, live RF gauges,
/// Geiger click-rate and a move-this-way hint from the recent signal trend.
fn render_sweetspot(f: &mut Frame, rs: &RenderState, area: Rect) {
    let tel = &rs.telemetry;
    let q = crate::core::acoustic_radar::calculate_quality_score(tel);
    let block = titled_block("★ SWEET-SPOT LOCATOR ([S] audio scan)");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width as usize;

    let style = if q.score >= 70 {
        good()
    } else if q.score >= 50 {
        amber_bold()
    } else if q.score >= 30 {
        amber()
    } else {
        alert()
    };

    // Movement hint from the signal-history trend (warmer = stronger).
    let hist = &rs.socket_sample.signal_history;
    let hint = if hist.len() >= 6 {
        let n = hist.len();
        let recent: f64 = hist[n - 3..].iter().sum::<f64>() / 3.0;
        let prior: f64 = hist[n - 6..n - 3].iter().sum::<f64>() / 3.0;
        if recent > prior + 1.5 {
            ("↑ WARMER — keep moving this way", good())
        } else if recent < prior - 1.5 {
            ("↓ COLDER — go back", alert())
        } else {
            ("• HOLD — stable sweet spot", amber())
        }
    } else {
        ("• sampling…", amber())
    };

    let digits = big_number(q.score);
    let meter = score_meter(q.score, w.saturating_sub(10).clamp(8, 40));
    let gbw = w.saturating_sub(22).clamp(6, 24);
    let lines = vec![
        Line::from(Span::styled(format!("{}  {}", digits[0], meter), style)),
        Line::from(vec![
            Span::styled(format!("{}  ", digits[1]), style),
            Span::styled(q.rating.to_string(), style),
        ]),
        Line::from(Span::styled(
            format!("{}  {:.1} clicks/s", digits[2], q.click_hz),
            style,
        )),
        Line::from(Span::styled(
            hbar_row("Signal", q.signal_dbm as f64, -90.0, -40.0, "dBm", gbw),
            amber(),
        )),
        Line::from(Span::styled(
            hbar_row("SNR", q.snr_db as f64, 5.0, 45.0, "dB", gbw),
            amber(),
        )),
        Line::from(Span::styled(hint.0, hint.1)),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

/// RF Interference Scanner: microwave / Bluetooth / ACI / DFS detectors with a
/// confidence bar each + overall RF airspace verdict.
fn render_rf_scanner(f: &mut Frame, rs: &RenderState, area: Rect) {
    use crate::core::rf_inference as rf;
    let tel = &rs.telemetry;
    let block = titled_block("RF INTERFERENCE SCANNER (non-Wi-Fi)");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width as usize;

    let mw = rf::detect_microwave(tel);
    let bt = rf::detect_bluetooth(tel);
    let (aci, _) = rf::detect_aci(tel);
    let dfs = rf::detect_dfs(tel);
    let report = rf::run_full(tel);

    let bar_w = w.saturating_sub(26).clamp(6, 24);
    let conf_bar = move |c: f64| -> String {
        let filled = (c.clamp(0.0, 1.0) * bar_w as f64).round() as usize;
        format!("{}{}", "█".repeat(filled), "░".repeat(bar_w - filled))
    };

    let rowf = |name: &str, d: &rf::Detection| -> Line<'static> {
        let pct = (d.confidence * 100.0).round() as i64;
        let (tag, style) = if d.detected {
            ("[!]", alert())
        } else {
            ("[OK]", good())
        };
        Line::from(Span::styled(
            format!(
                "{:<10} {} {:>3}% {}",
                name,
                conf_bar(d.confidence),
                pct,
                tag
            ),
            style,
        ))
    };

    let mut lines = vec![
        rowf("Microwave", &mw),
        rowf("Bluetooth", &bt),
        rowf("ACI Bleed", &aci),
        rowf("DFS Radar", &dfs),
        Line::from(Span::styled(
            format!("RF AIRSPACE SCORE: {}/100", report.rf_health_score),
            score_color(report.rf_health_score),
        )),
    ];
    let budget = (inner.height as usize).saturating_sub(lines.len());
    for (i, wl) in wrap_text(&report.primary_advice, w).into_iter().enumerate() {
        if i >= budget {
            break;
        }
        lines.push(Line::from(Span::styled(wl, amber())));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// RF Signal Chronograph: tall signal_dbm vertical graph + SNR/retry sparklines
/// over time — fills the full-width bottom with a real time-series chart.
fn render_chronograph(f: &mut Frame, rs: &RenderState, area: Rect) {
    let ss = &rs.socket_sample;
    let tel = &rs.telemetry;
    let block = titled_block("RF SIGNAL CHRONOGRAPH (signal · SNR · retry over time)");
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 3 {
        return;
    }
    let w = inner.width as usize;
    let graph_h = (inner.height as usize).saturating_sub(3).max(2);

    let sig_hist = if ss.signal_history.is_empty() {
        vec![tel.signal_dbm as f64]
    } else {
        ss.signal_history.clone()
    };
    let lo = sig_hist
        .iter()
        .cloned()
        .fold(tel.signal_dbm as f64, f64::min);
    let hi = sig_hist
        .iter()
        .cloned()
        .fold(tel.signal_dbm as f64, f64::max);
    let drift = (hi - lo).round() as i64;

    let mut lines = vec![Line::from(Span::styled(
        format!(
            "SIGNAL {}dBm (range {:.0}..{:.0}, Δ{}dB) · SNR {}dB · Retry {:.1}%",
            tel.signal_dbm, lo, hi, drift, tel.snr_db, tel.tx_retry_pct
        ),
        amber_bold(),
    ))];
    for g in render_2d_vertical_graph(&sig_hist, graph_h, w, -90.0, -35.0, "d") {
        lines.push(Line::from(Span::styled(g, amber())));
    }
    let sw = w.saturating_sub(16).max(10);
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

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}
