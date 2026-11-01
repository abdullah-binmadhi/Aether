//! Tab [2] Spectrum: 5GHz/2.4GHz occupancy heatmap + de-redacted neighbour list.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::spectrum;
use crate::render::RenderState;
use crate::ui::theme::{alert, amber, amber_bold, good, score_color};
use crate::ui::{draw_kpi_strip, titled_block, Kpi};

pub fn render(f: &mut Frame, rs: &RenderState, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(9),
        Constraint::Min(6),
    ])
    .split(area);
    draw_kpi(f, rs, rows[0]);
    render_heatmap(f, rs, rows[1]);
    render_neighbors(f, rs, rows[2]);
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
        let gbar = format!("{}{}", "█".repeat(filled), "░".repeat(bar_w - filled));
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
    let block = titled_block("DETECTED LOCAL WI-FI NETWORKS & RF SIGNATURES");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width as usize;

    // Signal-strength rank chart: strongest first, one RSSI bar per network,
    // your AP in bold, co-channel neighbours in red. Fills every row the panel
    // has instead of a short list with dead space below.
    let bar_w = w.saturating_sub(48).clamp(8, 50);
    let rssi_bar = |dbm: i64| -> String {
        let ratio = ((dbm as f64 + 95.0) / 55.0).clamp(0.0, 1.0); // -95..-40
        let filled = (ratio * bar_w as f64).round() as usize;
        format!(
            "{}{}",
            "█".repeat(filled.min(bar_w)),
            "░".repeat(bar_w - filled.min(bar_w))
        )
    };

    let mut lines = vec![Line::from(Span::styled(
        format!("{:<22} {:<13} SIGNAL", "NETWORK", "CHANNEL/BAND"),
        amber_bold(),
    ))];

    // Active AP always first.
    lines.push(Line::from(Span::styled(
        format!(
            "● {:<20} {:<13} {} {:>4}dBm",
            truncate(&tel.ssid, 20),
            truncate(&tel.channel_label, 13),
            rssi_bar(tel.signal_dbm),
            tel.signal_dbm
        ),
        good(),
    )));

    // Neighbours sorted by signal strength (descending).
    let mut neigh: Vec<&crate::core::model::Neighbor> = tel.neighbors.iter().collect();
    neigh.sort_by_key(|a| std::cmp::Reverse(a.signal_dbm));
    let cap = (inner.height as usize).saturating_sub(3);
    for n in neigh.iter().take(cap) {
        let style = if n.same_channel { alert() } else { amber() };
        lines.push(Line::from(Span::styled(
            format!(
                "  {:<20} {:<13} {} {:>4}dBm",
                truncate(&n.label, 20),
                format!("{} ({})", n.channel_num, n.band.label()),
                rssi_bar(n.signal_dbm),
                n.signal_dbm
            ),
            style,
        )));
    }

    // Footer summary anchor.
    let same_ch = tel.neighbors.iter().filter(|n| n.same_channel).count();
    let strongest = tel
        .neighbors
        .iter()
        .map(|n| n.signal_dbm)
        .max()
        .unwrap_or(tel.signal_dbm);
    lines.push(Line::from(Span::styled(
        format!(
            "— {} network(s) · {} on your channel · strongest {} dBm",
            tel.neighbors.len(),
            same_ch,
            strongest
        ),
        amber_bold(),
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
