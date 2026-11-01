//! Tab [2] Spectrum: 5GHz/2.4GHz occupancy heatmap + de-redacted neighbour list.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::core::spectrum;
use crate::ui::theme::{alert, amber, amber_bold, bar, good};
use crate::ui::titled_block;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let rows =
        Layout::vertical([Constraint::Percentage(48), Constraint::Percentage(52)]).split(area);
    render_heatmap(f, app, rows[0]);
    render_neighbors(f, app, rows[1]);
}

fn render_heatmap(f: &mut Frame, app: &App, area: Rect) {
    let report = spectrum::analyze(&app.telemetry);
    let block = titled_block("5GHz & 2.4GHz SPECTRUM CONGESTION & CHANNEL OVERLAP");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = vec![Line::from(Span::styled(
        format!(
            "{:<20} {:<10} {:<12} {:<16} VERDICT",
            "BAND / BLOCK", "NEIGHBORS", "PEAK SIGNAL", "DENSITY"
        ),
        amber_bold(),
    ))];

    for b in &report.blocks_5g {
        let rssi = if b.count > 0 { b.max_rssi } else { -100 };
        let (verdict, style) = if b.colliding {
            ("[!] COLLISION (YOUR AP + NEIGHBOR)", alert())
        } else if b.mine {
            ("● YOUR ACTIVE 5GHz BLOCK", amber_bold())
        } else if b.count == 0 {
            ("[OK] 100% CLEAR AIRSPACE", good())
        } else {
            ("Occupied", amber())
        };
        let rssi_str = if b.count > 0 {
            format!("{rssi} dBm")
        } else {
            "Clear".into()
        };
        lines.push(Line::from(Span::styled(
            format!(
                "{:<20} {:<10} {:<12} {:<16} {}",
                b.label,
                b.count,
                rssi_str,
                bar(rssi as f64, -100.0, -40.0, 12),
                verdict
            ),
            style,
        )));
    }

    let rssi24 = if report.count_24 > 0 {
        report.max_rssi_24
    } else {
        -95
    };
    lines.push(Line::from(Span::styled(
        format!(
            "{:<20} {:<10} {:<12} {:<16} 2.4GHz floor",
            "2.4GHz (1/6/11)",
            report.count_24,
            if report.count_24 > 0 {
                format!("{rssi24} dBm")
            } else {
                "Clear".into()
            },
            bar(rssi24 as f64, -95.0, -40.0, 12)
        ),
        amber(),
    )));

    lines.push(Line::from(Span::styled(
        format!(
            "◆ Recommended: Channel {} — {}  │  {} co-channel collision(s) detected",
            report.recommended_5g_channel,
            report.recommendation_reason,
            report.collisions.len()
        ),
        good(),
    )));

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_neighbors(f: &mut Frame, app: &App, area: Rect) {
    let tel = &app.telemetry;
    let block = titled_block("DETECTED LOCAL WI-FI NETWORKS & RF SIGNATURES");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = vec![Line::from(Span::styled(
        format!(
            "{:<26} {:<16} {:<9} POWER",
            "NETWORK / RF SIGNATURE", "CHANNEL / BAND", "SIGNAL"
        ),
        amber_bold(),
    ))];

    // Active AP first.
    lines.push(Line::from(Span::styled(
        format!(
            "{:<26} {:<16} {:<9} {}",
            truncate(&format!("● {}", tel.ssid), 26),
            truncate(&tel.channel_label, 16),
            format!("{} dBm", tel.signal_dbm),
            bar(tel.signal_dbm as f64, -95.0, -40.0, 8)
        ),
        good(),
    )));

    let cap = (inner.height as usize).saturating_sub(2);
    for n in tel.neighbors.iter().take(cap) {
        let style = if n.same_channel { alert() } else { amber() };
        lines.push(Line::from(Span::styled(
            format!(
                "{:<26} {:<16} {:<9} {}",
                truncate(&n.label, 26),
                format!("{} ({})", n.channel_num, n.band.label()),
                format!("{} dBm", n.signal_dbm),
                bar(n.signal_dbm as f64, -95.0, -40.0, 8)
            ),
            style,
        )));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}
