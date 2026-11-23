//! Tab [1] OVERVIEW — Zero-Jargon Auto-Pilot Telemetry Cockpit:
//!   Top KPI strip   (Connection Quality · Response Time · Wi-Fi Strength · Live Speed)
//!   2x2 tile grid   (Auto-Pilot Live Protections · Smoothness & Speed Monitor ·
//!                    Where Is The Slowdown? · Auto-Pilot Activity Feed & Readiness)

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::render::RenderState;
use crate::ui::theme::{alert, amber, amber_bold, dim, good, invert, score_color, sparkline};
use crate::ui::widgets::render_2d_vertical_graph;
use crate::ui::{draw_kpi_strip, grid, titled_block, Kpi};

pub fn render(f: &mut Frame, rs: &RenderState, area: Rect) {
    let rows = Layout::vertical([Constraint::Length(4), Constraint::Min(6)]).split(area);
    draw_kpi(f, rs, rows[0]);

    let tiles = grid(rows[1], 2, 2);
    render_autopilot_protections(f, rs, tiles[0]);
    render_smoothness_monitor(f, rs, tiles[1]);
    render_connection_path(f, rs, tiles[2]);
    render_activity_and_readiness(f, rs, tiles[3]);
}

fn fmt_speed(kbs: f64) -> String {
    let mbps = (kbs * 8.0) / 1000.0;
    if mbps >= 1.0 {
        format!("{mbps:.1}M")
    } else {
        format!("{kbs:.0}K")
    }
}

fn draw_kpi(f: &mut Frame, rs: &RenderState, area: Rect) {
    let tel = &rs.telemetry;
    let health_accent = score_color(tel.health_score);
    let quality_word = if tel.health_score >= 88 {
        "EXCELLENT"
    } else if tel.health_score >= 72 {
        "GOOD"
    } else if tel.health_score >= 50 {
        "BUSY"
    } else {
        "SLOW"
    };

    let ping_word = if rs.scope_avg_ms <= 0.1 {
        "Probing"
    } else if rs.scope_avg_ms <= 30.0 {
        "Smooth"
    } else if rs.scope_avg_ms <= 70.0 {
        "Okay"
    } else {
        "Delayed"
    };

    let strength_word = if tel.signal_dbm >= -60 {
        "Strong"
    } else if tel.signal_dbm >= -70 {
        "Good"
    } else if tel.signal_dbm >= -78 {
        "Fair"
    } else {
        "Weak"
    };
    let strength_accent = if tel.signal_dbm >= -70 {
        Some(good())
    } else {
        Some(amber())
    };

    let sig_hist = &rs.socket_sample.signal_history;
    let speed_str = format!(
        "▼{} ▲{}",
        fmt_speed(rs.scope_down_kbs),
        fmt_speed(rs.scope_up_kbs)
    );

    let cards = [
        Kpi {
            title: "CONNECTION QUALITY",
            value: format!("{}/100", tel.health_score),
            unit: quality_word,
            history: &rs.scope_rtt_history,
            floor: 0.0,
            accent: Some(health_accent),
        },
        Kpi {
            title: "RESPONSE TIME",
            value: format!("{:.1} ms", rs.scope_avg_ms),
            unit: ping_word,
            history: &rs.scope_rtt_history,
            floor: 0.0,
            accent: if rs.scope_avg_ms <= 40.0 {
                Some(good())
            } else {
                Some(amber())
            },
        },
        Kpi {
            title: "WIFI STRENGTH",
            value: strength_word.into(),
            unit: tel.band.label(),
            history: sig_hist,
            floor: -95.0,
            accent: strength_accent,
        },
        Kpi {
            title: "LIVE SPEED",
            value: speed_str,
            unit: "bps",
            history: &rs.scope_down_history,
            floor: 0.0,
            accent: Some(good()),
        },
    ];
    draw_kpi_strip(f, area, &cards);
}

/// Tile 1: Plain-English Live Telemetry for all 6 Auto-Pilot features.
fn render_autopilot_protections(f: &mut Frame, rs: &RenderState, area: Rect) {
    let block = titled_block("⚙ AUTO-PILOT LIVE PROTECTIONS  ([A] Toggle)");
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let w = inner.width as usize;
    let mut lines = Vec::new();

    if rs.auto_pilot_enabled {
        lines.push(Line::from(vec![
            Span::styled(
                format!(" ● AUTO-PILOT ON: {} ", rs.autopilot_plain_mode),
                invert(),
            ),
        ]));
    } else {
        lines.push(Line::from(vec![Span::styled(
            " ○ AUTO-PILOT PAUSED (Manual) — Press [A] to Turn On ",
            dim(),
        )]));
    }

    let two_line_per_item = inner.height >= 14;
    if two_line_per_item {
        lines.push(Line::from(Span::raw("")));
    }

    for feat in &rs.autopilot_features {
        if lines.len() >= inner.height as usize {
            break;
        }
        let badge_style = if feat.active { good() } else { dim() };
        if two_line_per_item && lines.len() + 1 < inner.height as usize {
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", feat.badge), badge_style),
                Span::styled(feat.name, amber_bold()),
            ]));
            let indent = "            ";
            let desc = truncate(&feat.plain_status, w.saturating_sub(indent.len()));
            lines.push(Line::from(Span::styled(
                format!("{indent}{desc}"),
                if feat.active { amber() } else { dim() },
            )));
        } else {
            let prefix_len = feat.badge.chars().count() + 2 + feat.name.chars().count() + 3;
            let desc = truncate(&feat.plain_status, w.saturating_sub(prefix_len));
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", feat.badge), badge_style),
                Span::styled(format!("{} · ", feat.name), amber_bold()),
                Span::styled(desc, amber()),
            ]));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

/// Tile 2: Smoothness & Speed Monitor oscilloscope ([M] switches series).
fn render_smoothness_monitor(f: &mut Frame, rs: &RenderState, area: Rect) {
    let mode = ((rs.visual_mode.wrapping_sub(1)) % 4) + 1;
    let (title, series, floor, max, unit, scolor): (&str, &[f64], f64, f64, &str, _) = match mode {
        2 => (
            "DOWNLOAD SPEED MONITOR  ([M] Switch Graph)",
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
            "UPLOAD SPEED MONITOR  ([M] Switch Graph)",
            &rs.scope_up_history,
            0.0,
            rs.scope_up_history.iter().cloned().fold(10.0_f64, f64::max),
            "K",
            amber_bold(),
        ),
        4 => (
            "COMBINED SMOOTHNESS & SPEED  ([M] Switch Graph)",
            &rs.scope_rtt_history,
            0.0,
            50.0,
            "m",
            amber(),
        ),
        _ => (
            "SMOOTHNESS & SPEED MONITOR  ([M] Switch Graph)",
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
    let graph_h = (inner.height as usize).saturating_sub(3).max(2);
    let mut lines = vec![Line::from(Span::styled(
        format!(
            "Smoothness: ±{:.1}ms · Dropped: {:.1}% · Spikes Blocked: {}",
            rs.scope_jitter_ms, rs.scope_loss_pct, rs.autopilot_blocked_spikes
        ),
        amber_bold(),
    ))];
    for g in render_2d_vertical_graph(series, graph_h, w, floor, max, unit) {
        lines.push(Line::from(Span::styled(g, scolor)));
    }
    let sw = w.saturating_sub(16).max(8);
    lines.push(Line::from(Span::styled(
        format!(
            "DOWN  {} {}/s",
            sparkline(&rs.scope_down_history, sw, 1.0),
            fmt_speed(rs.scope_down_kbs)
        ),
        good(),
    )));
    lines.push(Line::from(Span::styled(
        format!(
            "UP    {} {}/s",
            sparkline(&rs.scope_up_history, sw, 1.0),
            fmt_speed(rs.scope_up_kbs)
        ),
        amber(),
    )));
    f.render_widget(Paragraph::new(lines), inner);
}

fn plain_hop_name(hop: u8, fallback: &str) -> &str {
    match hop {
        1 => "Your Mac Wi-Fi",
        2 => "Home Wi-Fi Router",
        3 => "Internet Provider",
        4 => "Game & Video Servers",
        _ => fallback,
    }
}

/// Tile 3: Plain-English 4-stage connection path ("Where is the slowdown?").
fn render_connection_path(f: &mut Frame, rs: &RenderState, area: Rect) {
    let block = titled_block("WHERE IS THE SLOWDOWN? (CONNECTION PATH)");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width as usize;
    let label_w = 20usize;
    let bar_max = w.saturating_sub(label_w + 22).clamp(6, 36);

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
        let is_bad = h.status.starts_with("[!]");
        let style = if is_bad { alert() } else { good() };
        let status_word = if is_bad { "! Busy" } else { "✓ Ok  " };
        let name = plain_hop_name(h.hop, &h.label);
        lines.push(Line::from(Span::styled(
            format!(
                "[{}] {:<width$} {} {:>5.1}ms {}",
                h.hop,
                truncate(name, label_w),
                g,
                h.rtt_ms,
                status_word,
                width = label_w
            ),
            style,
        )));
    }

    let is_clean = rs.scope_fault.starts_with("[OK]") && rs.scope_max_bloat <= 18.0;
    let vstyle = if is_clean { good() } else { alert() };
    let plain_verdict = if is_clean {
        "✓ Everything from your Mac to the internet is running smoothly.".to_string()
    } else if rs.scope_max_bloat > 18.0 {
        "! Traffic jam detected at your Wi-Fi Router — Auto-Pilot is clearing it.".to_string()
    } else {
        format!("! {}", rs.scope_fault)
    };
    let verdict_lines = wrap_text(&format!("VERDICT: {plain_verdict}"), w);

    let used = lines.len();
    let reserved = verdict_lines.len() + 2;
    let leftover = (inner.height as usize).saturating_sub(used + reserved);
    if leftover >= 3 {
        lines.push(Line::from(Span::styled(
            "── RESPONSE TIME OVER TIME ──",
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

/// Tile 4: Workload Readiness Badges + Live Timestamped Auto-Pilot Activity Log.
fn render_activity_and_readiness(f: &mut Frame, rs: &RenderState, area: Rect) {
    let block = titled_block("AUTO-PILOT ACTIVITY FEED & READINESS");
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    let w = inner.width as usize;

    let mut lines = Vec::new();
    let game_ok = rs.scope_jitter_ms <= 8.0 && rs.scope_max_bloat <= 18.0;
    let video_ok = rs.telemetry.tx_rate_mbps >= 70 || rs.telemetry.is_hotspot;
    let call_ok = rs.scope_loss_pct < 2.0 && rs.scope_jitter_ms <= 12.0;

    lines.push(Line::from(vec![
        Span::styled(
            if game_ok {
                " [✓ Gaming: Smooth] "
            } else {
                " [! Gaming: Busy] "
            },
            if game_ok { good() } else { alert() },
        ),
        Span::styled(
            if video_ok {
                " [✓ 4K Video: Instant] "
            } else {
                " [! 4K Video: Slow] "
            },
            if video_ok { good() } else { alert() },
        ),
        Span::styled(
            if call_ok {
                " [✓ Calls: Clear] "
            } else {
                " [! Calls: Choppy] "
            },
            if call_ok { good() } else { alert() },
        ),
    ]));

    lines.push(Line::from(Span::raw("")));
    lines.push(Line::from(Span::styled(
        "LIVE LOG (Proof Auto-Pilot Is Working):",
        amber_bold(),
    )));

    for entry in &rs.autopilot_log {
        if lines.len() >= inner.height as usize {
            break;
        }
        let msg = truncate(&entry.plain_message, w.saturating_sub(11));
        lines.push(Line::from(vec![
            Span::styled(format!("{}  ", entry.timestamp), dim()),
            Span::styled(
                msg,
                if entry.plain_message.starts_with('✓') {
                    good()
                } else {
                    amber()
                },
            ),
        ]));
    }

    if lines.len() + 2 <= inner.height as usize {
        lines.push(Line::from(Span::raw("")));
        for wl in wrap_text(&rs.autopilot_summary, w) {
            if lines.len() >= inner.height as usize {
                break;
            }
            lines.push(Line::from(Span::styled(wl, dim())));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    #[test]
    fn redesigned_overview_renders_zero_jargon_telemetry_cleanly() {
        let backend = TestBackend::new(130, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::new("en0", Arc::new(AtomicBool::new(false)));
        let rs = app.build_render();

        terminal
            .draw(|f| {
                render(f, &rs, f.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                text.push_str(buf[(x, y)].symbol());
            }
            text.push('\n');
        }

        assert!(text.contains("AUTO-PILOT LIVE PROTECTIONS"));
        assert!(text.contains("Priority Fast-Lane"));
        assert!(text.contains("Background Scan Blocker"));
        assert!(text.contains("WHERE IS THE SLOWDOWN?"));
        assert!(text.contains("LIVE LOG"));
        assert!(text.contains("CONNECTION QUALITY"));
    }
}
