//! Privacy Fortress, DNS Racer & App Telemetry Sniffer View (Tab 10 / [0]).
//!
//! Visualizes real-time DNS resolver race benchmarks, background app outbound
//! telemetry/tracker connections, and active VPN/WireGuard encrypted tunnel status.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::privacy_fortress::PrivacyData;
use crate::render::RenderState;
use crate::ui::theme::{alert, amber, amber_bold, dim, good, invert};
use crate::ui::{draw_kpi_strip, titled_block, Kpi};

pub fn render(f: &mut Frame, rs: &RenderState, area: Rect) {
    let privacy = &rs.privacy_data;

    let rows = Layout::vertical([
        Constraint::Length(4), // Top KPI scorecard strip
        Constraint::Min(12),   // 2-column main grid
    ])
    .split(area);

    draw_kpis(f, privacy, rows[0]);
    draw_main_grid(f, privacy, rows[1]);
}

fn draw_kpis(f: &mut Frame, privacy: &PrivacyData, area: Rect) {
    let active_res_val = if !privacy.tunnel.active_resolver.is_empty() {
        truncate(&privacy.tunnel.active_resolver, 18)
    } else {
        "Default DHCP".to_string()
    };

    let cards = [
        Kpi {
            title: "ACTIVE RESOLVER",
            value: active_res_val,
            unit: if privacy.tunnel.is_vpn_active { "[VPN Routed]" } else { "[Local Gateway]" },
            history: &[],
            floor: 0.0,
            accent: Some(good()),
        },
        Kpi {
            title: "ENCRYPTION POSTURE",
            value: if privacy.tunnel.is_vpn_active {
                "ENCRYPTED".into()
            } else if privacy.tunnel.active_resolver == "1.1.1.1" || privacy.tunnel.active_resolver == "9.9.9.9" {
                "SECURE DNS".into()
            } else {
                "CLEARTEXT".into()
            },
            unit: if privacy.tunnel.is_vpn_active { "[WireGuard/VPN]" } else { "[Port 53 UDP]" },
            history: &[],
            floor: 0.0,
            accent: Some(if privacy.tunnel.is_vpn_active { good() } else { amber_bold() }),
        },
        Kpi {
            title: "TRACKERS DETECTED",
            value: format!("{} PIDs", privacy.telemetry_calls.len()),
            unit: "[Outbound Monitored]",
            history: &[],
            floor: 0.0,
            accent: Some(if privacy.telemetry_calls.is_empty() { good() } else { alert() }),
        },
        Kpi {
            title: "TUNNEL INTEGRITY",
            value: if privacy.tunnel.is_vpn_active {
                "ACTIVE".into()
            } else {
                "NO TUNNEL".into()
            },
            unit: if privacy.tunnel.is_vpn_active { "[utun Shielded]" } else { "[Direct Wi-Fi]" },
            history: &[],
            floor: 0.0,
            accent: Some(if privacy.tunnel.is_vpn_active { good() } else { dim() }),
        },
    ];

    draw_kpi_strip(f, area, &cards);
}

fn draw_main_grid(f: &mut Frame, privacy: &PrivacyData, area: Rect) {
    let cols = Layout::horizontal([
        Constraint::Percentage(44), // Left: 1-Key DNS Racer Matrix
        Constraint::Percentage(56), // Right: App Telemetry & Call-Home Sniffer (expanded for ticker)
    ])
    .split(area);

    draw_dns_racer(f, privacy, cols[0]);
    draw_telemetry_sniffer(f, privacy, cols[1]);
}

fn draw_dns_racer(f: &mut Frame, privacy: &PrivacyData, area: Rect) {
    let is_focused = privacy.active_column == 0;
    let badge = if is_focused { " ▌ACTIVE▐" } else { "" };
    let title = if area.width >= 72 {
        format!("⚡ 1-KEY DNS RACER & RESOLVER MATRIX [Enter] Apply [D] DHCP{badge}")
    } else if area.width >= 48 {
        format!("⚡ 1-KEY DNS RACER MATRIX{badge}")
    } else {
        format!("⚡ DNS RACER{badge}")
    };
    let block = titled_block(&title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();

    // Table Header
    lines.push(Line::from(vec![
        Span::styled("  RESOLVER NAME        ", dim()),
        Span::styled("PRIMARY IP     ", dim()),
        Span::styled("RTT     ", dim()),
        Span::styled("SECURITY", dim()),
    ]));

    let visible_rows = (inner.height as usize).saturating_sub(2);
    let start_idx = if privacy.selected_dns_idx >= visible_rows {
        privacy.selected_dns_idx + 1 - visible_rows
    } else {
        0
    };

    for (i, c) in privacy.dns_candidates.iter().enumerate().skip(start_idx).take(visible_rows) {
        let is_selected = is_focused && i == privacy.selected_dns_idx;
        let arrow = if is_selected { "▶ " } else { "  " };

        let rtt_str = if let Some(ms) = c.rtt_ms {
            format!("{:>4.1} ms", ms)
        } else {
            "  -- ms ".into()
        };

        let rtt_style = match c.rtt_ms {
            Some(ms) if ms < 20.0 => good(),
            Some(ms) if ms < 50.0 => amber_bold(),
            Some(_) => amber(),
            None => dim(),
        };

        let status_text = if c.is_current {
            format!("{} [ACTIVE]", c.grade)
        } else {
            c.grade.to_string()
        };

        lines.push(Line::from(vec![
            Span::styled(arrow, if is_selected { amber_bold() } else { dim() }),
            Span::styled(
                format!("{:<21}", truncate(c.name, 20)),
                if is_selected { invert() } else { amber_bold() },
            ),
            Span::styled(
                format!("{:<15}", c.primary),
                if is_selected { invert() } else { amber() },
            ),
            Span::styled(
                format!("{:<8}", rtt_str),
                if is_selected { invert() } else { rtt_style },
            ),
            Span::styled(
                status_text,
                if is_selected { invert() } else if c.is_current { good() } else { amber() },
            ),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_telemetry_sniffer(f: &mut Frame, privacy: &PrivacyData, area: Rect) {
    let is_focused = privacy.active_column == 1;
    let badge = if is_focused { " ▌ACTIVE▐" } else { "" };
    let count = privacy.telemetry_calls.len();

    let title = if area.width >= 72 {
        format!("◉ APP TELEMETRY & CALL-HOME SNIFFER ({count} Endpoints){badge}")
    } else if area.width >= 48 {
        format!("◉ APP TELEMETRY & SNIFFER ({count}){badge}")
    } else {
        format!("◉ CALL-HOME SNIFFER ({count}){badge}")
    };
    let block = titled_block(&title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let avail_w = inner.width as usize;
    if avail_w < 20 {
        return;
    }

    let mut lines = Vec::new();

    let show_pid = avail_w >= 54;
    let fixed_w = if show_pid {
        2 + 10 + 6 + 18 + 12 // arrow(2) + proc(10) + pid(6) + endpoint(18) + type(12) = 48
    } else {
        2 + 10 + 15 + 11 // arrow(2) + proc(10) + endpoint(15) + type(11) = 38
    };
    let org_w = avail_w.saturating_sub(fixed_w).max(10);

    // Table Header
    if show_pid {
        lines.push(Line::from(vec![
            Span::styled("  PROCESS   ", dim()),
            Span::styled("PID   ", dim()),
            Span::styled("REMOTE ENDPOINT   ", dim()),
            Span::styled("TYPE        ", dim()),
            Span::styled(format!("{:<width$}", "DESTINATION ORG", width = org_w), dim()),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("  PROCESS   ", dim()),
            Span::styled("REMOTE ENDPOINT", dim()),
            Span::styled("TYPE       ", dim()),
            Span::styled(format!("{:<width$}", "DESTINATION ORG", width = org_w), dim()),
        ]));
    }

    if privacy.telemetry_calls.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("   ✔ No active background telemetry or tracking connections detected.", good()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("   All application sockets are currently dormant or local.", dim()),
        ]));
        f.render_widget(Paragraph::new(lines), inner);
        return;
    }

    let visible_rows = (inner.height as usize).saturating_sub(2);
    let start_idx = if privacy.selected_telemetry_idx >= visible_rows {
        privacy.selected_telemetry_idx + 1 - visible_rows
    } else {
        0
    };

    for (i, t) in privacy.telemetry_calls.iter().enumerate().skip(start_idx).take(visible_rows) {
        let is_selected = is_focused && i == privacy.selected_telemetry_idx;
        let arrow = if is_selected { "▶ " } else { "  " };

        let badge_style = match t.risk_badge {
            "[TELEMETRY]" | "[ANALYTICS]" => alert(),
            "[APPLE-SYNC]" => good(),
            _ => amber(),
        };

        let org_ticker = ticker_text(&t.destination_org, org_w, is_selected);

        if show_pid {
            lines.push(Line::from(vec![
                Span::styled(arrow, if is_selected { amber_bold() } else { dim() }),
                Span::styled(
                    format!("{:<10}", truncate(&t.command, 9)),
                    if is_selected { invert() } else { amber_bold() },
                ),
                Span::styled(
                    format!("{:<6}", t.pid),
                    if is_selected { invert() } else { dim() },
                ),
                Span::styled(
                    format!("{:<18}", truncate(&t.remote_endpoint, 17)),
                    if is_selected { invert() } else { amber() },
                ),
                Span::styled(
                    format!("{:<12}", t.risk_badge),
                    if is_selected { invert() } else { badge_style },
                ),
                Span::styled(
                    org_ticker,
                    if is_selected { invert() } else { amber() },
                ),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(arrow, if is_selected { amber_bold() } else { dim() }),
                Span::styled(
                    format!("{:<10}", truncate(&t.command, 9)),
                    if is_selected { invert() } else { amber_bold() },
                ),
                Span::styled(
                    format!("{:<15}", truncate(&t.remote_endpoint, 14)),
                    if is_selected { invert() } else { amber() },
                ),
                Span::styled(
                    format!("{:<11}", t.risk_badge),
                    if is_selected { invert() } else { badge_style },
                ),
                Span::styled(
                    org_ticker,
                    if is_selected { invert() } else { amber() },
                ),
            ]));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn ticker_text(text: &str, width: usize, is_selected: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        return format!("{:<width$}", text, width = width);
    }

    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    // Selected row scrolls at 280ms per character; unselected rows scroll at 420ms
    let step_ms = if is_selected { 280 } else { 420 };
    let step = (ms / step_ms) as usize;

    let sep: Vec<char> = "   •   ".chars().collect();
    let mut cycle = chars.clone();
    cycle.extend(&sep);
    let n = cycle.len();
    let offset = step % n;

    let mut out = String::with_capacity(width);
    for i in 0..width {
        out.push(cycle[(offset + i) % n]);
    }
    out
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}…", &s[..max_len.saturating_sub(1)])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    #[test]
    fn test_ticker_text_properties() {
        // Shorter than width
        let short = "Apple";
        let out_short = ticker_text(short, 10, false);
        assert_eq!(out_short.len(), 10);
        assert_eq!(out_short.trim_end(), "Apple");

        // Longer than width
        let long = "Google Cloud / Edge Infrastructure";
        let out_long = ticker_text(long, 15, false);
        assert_eq!(out_long.chars().count(), 15);

        let out_selected = ticker_text(long, 20, true);
        assert_eq!(out_selected.chars().count(), 20);
    }

    #[test]
    fn test_privacy_fortress_render_no_overflow() {
        let backend = TestBackend::new(120, 35);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = crate::app::App::new("en0", Arc::new(AtomicBool::new(false)));
        let rs = app.build_render();

        terminal.draw(|f| {
            render(f, &rs, f.area());
        }).unwrap();

        let buf = terminal.backend().buffer();
        // Verify every line in the buffer does not wrap and remains within width
        for y in 0..buf.area().height {
            let mut line = String::new();
            for x in 0..buf.area().width {
                line.push_str(buf[(x, y)].symbol());
            }
            assert!(line.chars().count() <= 120);
        }
    }
}
