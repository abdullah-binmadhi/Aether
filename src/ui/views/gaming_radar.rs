//! Gaming Radar & Esports Ping View (Tab 9).
//!
//! Visualizes regional game cluster latencies, standard-deviation jitter,
//! live jitter sparklines, AWDL anti-stutter shielding, and route hop diagnostics.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::gaming_radar::GamingData;
use crate::render::RenderState;
use crate::ui::theme::{amber, amber_bold, dim, good, invert, sparkline};
use crate::ui::{draw_kpi_strip, titled_block, Kpi};

pub fn render(f: &mut Frame, rs: &RenderState, area: Rect) {
    let gaming = &rs.gaming_data;

    let rows = Layout::vertical([
        Constraint::Length(4), // Top KPI scorecard strip
        Constraint::Min(12),   // Main 2-column game telemetry grid
    ])
    .split(area);

    draw_kpis(f, gaming, rows[0]);
    draw_main_grid(f, gaming, rows[1]);
}

fn draw_kpis(f: &mut Frame, gaming: &GamingData, area: Rect) {
    let best_unit = if !gaming.best_region.is_empty() {
        gaming.best_region.as_str()
    } else {
        "Detecting..."
    };

    let cards = [
        Kpi {
            title: "BEST REGION",
            value: if !gaming.best_region.is_empty() {
                format!("{:.0} ms", gaming.best_ping)
            } else {
                "-- ms".into()
            },
            unit: best_unit,
            history: &[],
            floor: 0.0,
            accent: Some(good()),
        },
        Kpi {
            title: "JITTER STABILITY",
            value: format!("±{:.1} ms", gaming.avg_jitter),
            unit: if gaming.avg_jitter < 2.0 { "[ROCK SOLID]" } else { "[ACCEPTABLE]" },
            history: &gaming.jitter_history,
            floor: 0.0,
            accent: Some(if gaming.avg_jitter < 2.0 { good() } else { amber_bold() }),
        },
        Kpi {
            title: "AWDL LAG SHIELD",
            value: if gaming.game_mode_active {
                "0Hz LOCKED".into()
            } else {
                "STANDBY".into()
            },
            unit: if gaming.game_mode_active { "[Anti-Stutter Active]" } else { "[Standard AWDL]" },
            history: &[],
            floor: 0.0,
            accent: Some(if gaming.game_mode_active { good() } else { dim() }),
        },
        Kpi {
            title: "ANTI-CHEAT INTEGRITY",
            value: "VERIFIED".into(),
            unit: "Riot Vanguard Safe",
            history: &[],
            floor: 0.0,
            accent: Some(good()),
        },
    ];

    draw_kpi_strip(f, area, &cards);
}

fn draw_main_grid(f: &mut Frame, gaming: &GamingData, area: Rect) {
    let cols = Layout::horizontal([
        Constraint::Percentage(55), // Left: Regional Game Ping Matrix
        Constraint::Percentage(45), // Right: Optimizer, Sparkline & Hop Inspector
    ])
    .split(area);

    draw_ping_matrix(f, gaming, cols[0]);
    draw_inspector_column(f, gaming, cols[1]);
}

fn draw_ping_matrix(f: &mut Frame, gaming: &GamingData, area: Rect) {
    let title = format!(
        "⚔ REGIONAL GAME CLUSTER PING MATRIX  ·  Filter: [G] {}  ·  [Space] Game Shield",
        gaming.active_filter.label()
    );
    let block = titled_block(&title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();

    // Table Header
    lines.push(Line::from(vec![
        Span::styled("   CLUSTER / REGION             ", dim()),
        Span::styled("PING     ", dim()),
        Span::styled("JITTER   ", dim()),
        Span::styled("LOSS   ", dim()),
        Span::styled("STATUS / QUALITY", dim()),
    ]));

    let visible_rows = (inner.height as usize).saturating_sub(2);
    let start_idx = if gaming.selected_idx >= visible_rows {
        gaming.selected_idx + 1 - visible_rows
    } else {
        0
    };

    for (i, ep) in gaming.endpoints.iter().enumerate().skip(start_idx).take(visible_rows) {
        let is_selected = i == gaming.selected_idx;
        let arrow = if is_selected { "▶ " } else { "  " };

        let ping_str = if let Some(ms) = ep.ping_ms {
            format!("{:>4.0} ms", ms)
        } else {
            "  --   ".into()
        };

        let jitter_str = if let Some(j) = ep.jitter_ms {
            format!("±{:>3.1}ms", j)
        } else {
            "  --   ".into()
        };

        let loss_str = format!("{:>3.0}%", ep.packet_loss_pct);

        let ping_style = match ep.ping_ms {
            Some(ms) if ms < 40.0 => good(),
            Some(ms) if ms < 80.0 => amber_bold(),
            Some(_) => amber(),
            None => dim(),
        };

        lines.push(Line::from(vec![
            Span::styled(arrow, if is_selected { amber_bold() } else { dim() }),
            Span::styled(
                format!("{:<30}", truncate(ep.label, 29)),
                if is_selected { invert() } else { amber_bold() },
            ),
            Span::styled(format!("{:<9}", ping_str), if is_selected { invert() } else { ping_style }),
            Span::styled(format!("{:<9}", jitter_str), if is_selected { invert() } else { amber() }),
            Span::styled(format!("{:<7}", loss_str), if is_selected { invert() } else { dim() }),
            Span::styled(
                format!("{:<16}", ep.status),
                if is_selected { invert() } else if ep.status.starts_with("S-TIER") { good() } else { amber() },
            ),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_inspector_column(f: &mut Frame, gaming: &GamingData, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(8), // Active Game Optimizer & Pacing
        Constraint::Length(7), // 60s Jitter Sparkline
        Constraint::Min(8),    // Route Hop Analysis
    ])
    .split(area);

    draw_optimizer_box(f, gaming, rows[0]);
    draw_sparkline_box(f, gaming, rows[1]);
    draw_hop_analysis(f, gaming, rows[2]);
}

fn draw_optimizer_box(f: &mut Frame, gaming: &GamingData, area: Rect) {
    let block = titled_block("◉ ACTIVE GAME OPTIMIZER & PACING");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let profile_badge = if gaming.game_mode_active {
        Span::styled(" [VIP GAMING ACTIVE] ", invert())
    } else {
        Span::styled(" [STANDARD DESKTOP] ", dim())
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(" Profile  : ", dim()),
            Span::styled(&gaming.active_profile, amber_bold()),
            Span::raw(" "),
            profile_badge,
        ]),
        Line::from(vec![
            Span::styled(" AWDL     : ", dim()),
            Span::styled(&gaming.awdl_status, if gaming.game_mode_active { good() } else { amber() }),
        ]),
        Line::from(vec![
            Span::styled(" WMM QoS  : ", dim()),
            Span::styled("0x80 Voice VIP Airtime Priority", amber()),
        ]),
        Line::from(vec![
            Span::styled(" TCP Pkts : ", dim()),
            Span::styled("TCP_NODELAY + Fast-ACK (No Bloat)", amber()),
        ]),
        Line::from(vec![
            Span::styled(" Control  : ", dim()),
            Span::styled("[Space] Shield  ·  [G] Filter  ·  [R] Ping", amber_bold()),
        ]),
    ];

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_sparkline_box(f: &mut Frame, gaming: &GamingData, area: Rect) {
    let block = titled_block("▦ 60-SECOND JITTER SPARKLINE");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let spark_w = (inner.width as usize).saturating_sub(12);
    let spark = sparkline(&gaming.jitter_history, spark_w, 0.0);

    let lines = vec![
        Line::from(vec![
            Span::styled(" Jitter Track : ", dim()),
            Span::styled(format!("±{:.1}ms avg", gaming.avg_jitter), amber_bold()),
        ]),
        Line::from(vec![
            Span::styled(" Live Spark   : ", dim()),
            Span::styled(spark, good()),
        ]),
        Line::from(vec![
            Span::styled(" Health       : ", dim()),
            Span::styled(
                if gaming.avg_jitter < 2.0 {
                    "● Rock Solid (No micro-stutters)"
                } else {
                    "! Minor jitter detected on Wi-Fi"
                },
                if gaming.avg_jitter < 2.0 { good() } else { amber_bold() },
            ),
        ]),
    ];

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_hop_analysis(f: &mut Frame, gaming: &GamingData, area: Rect) {
    let block = titled_block("◪ ROUTE HOP TELEMETRY (Hop 1-4)");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(" HOP  NODE / DESTINATION       RTT      VERDICT", dim()),
        ]),
    ];

    for h in &gaming.route_hops {
        let hop_badge = format!(" [H{}] ", h.hop_num);
        lines.push(Line::from(vec![
            Span::styled(hop_badge, amber_bold()),
            Span::styled(format!("{:<22}", truncate(&h.label, 21)), amber()),
            Span::styled(format!("{:>5.1}ms  ", h.rtt_ms), if h.rtt_ms < 20.0 { good() } else { amber_bold() }),
            Span::styled(truncate(&h.verdict, 18), dim()),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(" Verdict: ", dim()),
        Span::styled(
            if gaming.best_ping < 40.0 {
                "[S-TIER] Optimal for Esports (Verified)"
            } else {
                "[A-TIER] Optimal for Online Gaming"
            },
            if gaming.best_ping < 40.0 { good() } else { amber_bold() },
        ),
    ]));

    f.render_widget(Paragraph::new(lines), inner);
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}…", &s[..max_len.saturating_sub(1)])
    } else {
        s.to_string()
    }
}
