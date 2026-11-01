//! Tab [1] Cockpit: THE PIPE (RF telemetry) + THE FLOW (4-hop bufferbloat matrix).

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::render::RenderState;
use crate::ui::theme::{alert, amber, amber_bold, bar, good, invert, sparkline};
use crate::ui::titled_block;
use crate::ui::widgets::DECK_MODES;

pub fn render(f: &mut Frame, rs: &RenderState, area: Rect) {
    let rows = Layout::vertical([Constraint::Min(8), Constraint::Length(9)]).split(area);
    let cols =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[0]);
    render_pipe(f, rs, cols[0]);
    render_flow(f, rs, cols[1]);
    let bot =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1]);
    render_analysis(f, rs, bot[0]);
    render_visual_deck(f, rs, bot[1]);
}

fn render_visual_deck(f: &mut Frame, rs: &RenderState, area: Rect) {
    let block = titled_block("VISUAL TELEMETRY DECK ([M] or [ / ] Switch View)");
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 2 {
        return;
    }
    let active = ((rs.visual_mode.wrapping_sub(1)) % 4) + 1;
    let mut pills = Vec::new();
    for (id, label) in DECK_MODES {
        let style = if id == active { invert() } else { amber_bold() };
        pills.push(Span::styled(format!(" [{label}] "), style));
    }
    let mut lines = vec![Line::from(pills)];
    for (idx, l) in rs.deck_lines.iter().enumerate() {
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

fn render_analysis(f: &mut Frame, rs: &RenderState, area: Rect) {
    let analysis = &rs.cockpit_analysis;
    let block = titled_block("HEALTH & WHAT TO DO NOW");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let score = rs.telemetry.health_score;
    let (dial, label) = crate::ui::widgets::health_dial_line(score);
    let score_style = if score >= 72 {
        good()
    } else if score >= 50 {
        amber_bold()
    } else {
        alert()
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(dial, score_style),
            Span::styled(format!("  {label}"), score_style),
        ]),
        Line::from(Span::styled(analysis.recommended_action.clone(), good())),
    ];
    // One or two supporting analysis lines (kept short so the dial stays lead).
    for l in analysis.analysis_lines.iter().take(2) {
        let style = if l.contains("[!]") { alert() } else { amber() };
        lines.push(Line::from(Span::styled(l.clone(), style)));
    }
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

fn render_pipe(f: &mut Frame, rs: &RenderState, area: Rect) {
    let tel = &rs.telemetry;
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

fn render_flow(f: &mut Frame, rs: &RenderState, area: Rect) {
    let block = titled_block("THE FLOW (OSCILLOSCOPE & 4-HOP ISOLATION MATRIX)");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let spark_w = (inner.width as usize).saturating_sub(18).max(10);
    let mut lines = vec![
        Line::from(Span::styled(
            format!(
                "PING {:4.1}ms  Jit {:3.1}ms  Loss {:3.1}% │ ▼{:6.1} ▲{:5.1} KB/s",
                rs.scope_avg_ms,
                rs.scope_jitter_ms,
                rs.scope_loss_pct,
                rs.scope_down_kbs,
                rs.scope_up_kbs
            ),
            amber_bold(),
        )),
        Line::from(Span::styled(
            format!(
                "RTT {} {:.1}ms",
                sparkline(&rs.scope_rtt_history, spark_w, 10.0),
                rs.scope_avg_ms
            ),
            amber_bold(),
        )),
        Line::from(Span::styled(
            format!(
                "T/P {} {:.0}K",
                sparkline(&rs.scope_down_history, spark_w, 1.0),
                rs.scope_down_kbs
            ),
            amber(),
        )),
        Line::from(Span::styled(
            format!(
                "UP  {} {:.0}K",
                sparkline(&rs.scope_up_history, spark_w, 1.0),
                rs.scope_up_kbs
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
                rs.scope_max_bloat,
                if rs.cam_pacer_active { ", CAM on" } else { "" }
            ),
            amber_bold(),
        )),
    ];

    for h in &rs.scope_hops {
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
    let verdict_style = if rs.scope_fault.starts_with("[OK]") {
        good()
    } else {
        alert()
    };
    lines.push(Line::from(Span::styled(
        format!("VERDICT: {}", rs.scope_fault),
        verdict_style,
    )));

    f.render_widget(Paragraph::new(lines), inner);
}
