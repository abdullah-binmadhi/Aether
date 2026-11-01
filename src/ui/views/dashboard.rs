//! Tab [1] Cockpit: THE PIPE (RF telemetry) + THE FLOW (4-hop bufferbloat matrix).

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::ui::theme::{alert, amber, amber_bold, bar, good, invert, sparkline};
use crate::ui::titled_block;

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::vertical([Constraint::Min(8), Constraint::Length(9)]).split(area);
    let cols =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[0]);
    render_pipe(f, app, cols[0]);
    render_flow(f, app, cols[1]);
    let bot =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1]);
    render_analysis(f, app, bot[0]);
    render_visual_deck(f, app, bot[1]);
}

fn render_visual_deck(f: &mut Frame, app: &App, area: Rect) {
    use crate::ui::widgets::{deck_body, DECK_MODES};
    let block = titled_block("VISUAL TELEMETRY DECK ([M] or [ / ] Switch View)");
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 2 {
        return;
    }
    let active = ((app.visual_mode.wrapping_sub(1)) % 4) + 1;
    let mut pills = Vec::new();
    for (id, label) in DECK_MODES {
        let style = if id == active { invert() } else { amber_bold() };
        pills.push(Span::styled(format!(" [{label}] "), style));
    }
    let body = deck_body(
        app,
        app.visual_mode,
        inner.width as usize,
        (inner.height as usize).saturating_sub(1),
    );
    let mut lines = vec![Line::from(pills)];
    for (idx, l) in body.iter().enumerate() {
        let style = if l.contains("[!]") || l.contains("╌✕") {
            alert()
        } else if idx == 0
            || l.contains("INTERPRETATION:")
            || l.contains("ANALYSIS:")
            || l.contains("[OK]")
            || l.contains("══▶")
        {
            if l.contains("[OK]") || l.contains("══▶") {
                good()
            } else {
                amber_bold()
            }
        } else {
            amber()
        };
        lines.push(Line::from(Span::styled(l.clone(), style)));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_analysis(f: &mut Frame, app: &App, area: Rect) {
    let analysis = app.preset.generate_cockpit_analysis(
        &app.telemetry,
        app.scope_snap.jitter_ms,
        app.scope_snap.max_bloat_delta_ms,
        app.scope_snap.avg_ms,
        app.scope.cam_pacer_active,
        &app.spectrum,
        &app.wan_snap,
        app.antilag.root_authorized,
    );
    let block = titled_block("LIVE RF & WORKLOAD INTERPRETATION");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut lines: Vec<Line> = analysis
        .analysis_lines
        .iter()
        .map(|l| {
            let style = if l.contains("[!]") { alert() } else { amber() };
            Line::from(Span::styled(l.clone(), style))
        })
        .collect();
    lines.push(Line::from(Span::styled(
        analysis.recommended_action.clone(),
        good(),
    )));
    let mut badges = Vec::new();
    for (name, _verdict, ok) in &analysis.workloads {
        let short: String = name.split_whitespace().next().unwrap_or(name).to_string();
        badges.push(Span::styled(
            format!(" {}{} ", if *ok { "✓" } else { "✗" }, short),
            if *ok { good() } else { alert() },
        ));
    }
    lines.push(Line::from(badges));
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_pipe(f: &mut Frame, app: &App, area: Rect) {
    let tel = &app.telemetry;
    let block = titled_block("THE PIPE (RF & AIRTIME TELEMETRY)");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let band_badge = if tel.is_hotspot {
        Span::styled(format!("[HOTSPOT {}]", tel.band.label()), amber_bold())
    } else if tel.band.is_fast() {
        Span::styled(format!("[{} LOCK]", tel.band.label()), good())
    } else {
        Span::styled(format!("[{} SLOW]", tel.band.label()), alert())
    };

    let bw = 14usize;
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("SSID      : {}", tel.ssid), amber_bold()),
        Span::raw("  "),
        band_badge,
    ])];
    if !tel.ssid_available {
        lines.push(Line::from(Span::styled(
            "            (name hidden — grant Location permission to reveal)",
            amber(),
        )));
    }
    lines.extend([
        Line::from(Span::styled(
            format!(
                "IP -> GW  : {} -> {} ({})",
                tel.ipv4, tel.gateway, tel.phy_mode
            ),
            amber(),
        )),
        Line::from(Span::styled(
            format!("Channel   : {}", tel.channel_label),
            amber(),
        )),
        Line::from(Span::styled(
            format!("Security  : {}", tel.security),
            amber(),
        )),
        Line::from(Span::styled(
            format!(
                "Signal    : {} {:>4} dBm",
                bar(tel.signal_dbm as f64, -90.0, -40.0, bw),
                tel.signal_dbm
            ),
            amber(),
        )),
        Line::from(Span::styled(
            format!(
                "SNR Margin: {} {:>4} dB",
                bar(tel.snr_db as f64, 5.0, 45.0, bw),
                tel.snr_db
            ),
            amber(),
        )),
        Line::from(Span::styled(
            format!(
                "Link Rate : {} {:>4} Mbps",
                bar(tel.tx_rate_mbps as f64, 0.0, 866.0, bw),
                tel.tx_rate_mbps
            ),
            amber_bold(),
        )),
        Line::from(Span::styled(
            format!(
                "MCS Index : {} {:>4} / 11",
                bar(tel.mcs_index as f64, 0.0, 11.0, bw),
                tel.mcs_index
            ),
            amber(),
        )),
        Line::from(Span::styled(
            format!(
                "AWDL/DNS  : {} │ DNS {} │ MTU {}",
                if tel.awdl_active {
                    "○ STD"
                } else {
                    "● ZERO-LAG"
                },
                tel.dns_servers
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "Auto".into()),
                tel.mtu
            ),
            if tel.awdl_active { amber() } else { good() },
        )),
    ]);

    if !tel.alerts.is_empty() {
        lines.push(Line::from(Span::styled("─ ALERTS ─", amber())));
        for a in tel.alerts.iter().take(3) {
            lines.push(Line::from(Span::styled(format!("[!] {a}"), alert())));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_flow(f: &mut Frame, app: &App, area: Rect) {
    let s = &app.scope_snap;
    let block = titled_block("THE FLOW (OSCILLOSCOPE & 4-HOP ISOLATION MATRIX)");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let spark_w = (inner.width as usize).saturating_sub(18).max(10);
    let mut lines = vec![
        Line::from(Span::styled(
            format!(
                "PING {:4.1}ms  Jit {:3.1}ms  Loss {:3.1}% │ ▼{:6.1} ▲{:5.1} KB/s",
                s.avg_ms, s.jitter_ms, s.loss_pct, s.down_kbs, s.up_kbs
            ),
            amber_bold(),
        )),
        Line::from(Span::styled(
            format!(
                "RTT {} {:.1}ms",
                sparkline(&s.rtt_history, spark_w, 10.0),
                s.avg_ms
            ),
            amber_bold(),
        )),
        Line::from(Span::styled(
            format!(
                "T/P {} {:.0}K",
                sparkline(&s.down_kbs_history, spark_w, 1.0),
                s.down_kbs
            ),
            amber(),
        )),
        Line::from(Span::styled(
            format!(
                "UP  {} {:.0}K",
                sparkline(&s.up_kbs_history, spark_w, 1.0),
                s.up_kbs
            ),
            amber(),
        )),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            format!(
                "{:<4} {:<12} {:>7} {:>7} {:>7}  STATUS  (max bloat +{:.0}ms{})",
                "HOP",
                "NODE",
                "RTT",
                "JIT",
                "BLOAT",
                s.max_bloat_delta_ms,
                if s.cam_pacer_active { ", CAM on" } else { "" }
            ),
            amber_bold(),
        )),
    ];

    for h in &s.hops {
        let style = if h.status.starts_with("[!]") {
            alert()
        } else if h.status.starts_with("[OK]") {
            good()
        } else {
            amber()
        };
        lines.push(Line::from(Span::styled(
            format!(
                "[{}] {:<12} {:>5.1}ms {:>5.1}ms +{:>4.0}ms  {}",
                h.hop, h.label, h.rtt_ms, h.jitter_ms, h.bloat_delta_ms, h.status
            ),
            style,
        )));
    }

    lines.push(Line::from(Span::raw("")));
    let verdict_style = if s.fault_diagnosis.starts_with("[OK]") {
        good()
    } else {
        alert()
    };
    lines.push(Line::from(Span::styled(
        format!("VERDICT: {}", s.fault_diagnosis),
        verdict_style,
    )));

    f.render_widget(Paragraph::new(lines), inner);
}
