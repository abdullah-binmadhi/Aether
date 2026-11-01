//! Tab [3] Per-process bandwidth firewall with SIGSTOP/SIGCONT control.
//! BI layout: KPI strip · process bandwidth table · (detail + LAN map) row.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::render::RenderState;
use crate::ui::theme::{alert, amber, amber_bold, bar, good, invert};
use crate::ui::{draw_kpi_strip, titled_block, Kpi};

pub fn render(f: &mut Frame, rs: &RenderState, area: Rect) {
    // KPI strip · process table (sized to rows) · TOP TALKERS chart ·
    // (selected detail + LAN map). The chart absorbs the space the short table
    // used to leave empty.
    let n = rs.processes.len().max(1);
    // Table height = header(2) + rows, capped so the chart + detail still fit.
    let table_h = (n as u16 + 2).clamp(6, 16);
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(table_h),
        Constraint::Min(5),
        Constraint::Length(8),
    ])
    .split(area);
    draw_kpi(f, rs, rows[0]);
    render_table(f, rs, rows[1]);
    render_top_talkers(f, rs, rows[2]);

    let bot =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[3]);
    render_detail(f, rs, bot[0]);
    render_lan(f, rs, bot[1]);
}

fn draw_kpi(f: &mut Frame, rs: &RenderState, area: Rect) {
    let procs = &rs.processes;
    let active = procs.len() as i64;
    let frozen = procs.iter().filter(|p| p.paused).count() as i64;
    let total_up: f64 = procs.iter().map(|p| p.up_kbs).sum();
    let top = procs
        .iter()
        .max_by(|a, b| {
            (a.down_kbs + a.up_kbs)
                .partial_cmp(&(b.down_kbs + b.up_kbs))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|p| p.name.chars().take(10).collect::<String>())
        .unwrap_or_else(|| "—".into());
    let empty: Vec<f64> = Vec::new();
    let cards = [
        Kpi {
            title: "ACTIVE PROCS",
            value: format!("{}", active),
            unit: "procs",
            history: &empty,
            floor: 0.0,
            accent: None,
        },
        Kpi {
            title: "TOP HOG",
            value: top,
            unit: "",
            history: &empty,
            floor: 0.0,
            accent: None,
        },
        Kpi {
            title: "TOTAL UP",
            value: format!("{:.0}", total_up),
            unit: "KB/s",
            history: &empty,
            floor: 0.0,
            accent: None,
        },
        Kpi {
            title: "FROZEN",
            value: format!("{}", frozen),
            unit: "procs",
            history: &empty,
            floor: 0.0,
            accent: if frozen > 0 { Some(alert()) } else { None },
        },
    ];
    draw_kpi_strip(f, area, &cards);
}

fn render_table(f: &mut Frame, rs: &RenderState, area: Rect) {
    let procs = &rs.processes;
    let block = titled_block("PER-PROCESS BANDWIDTH FIREWALL ([Space] Pause │ [U] Resume All)");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = vec![
        Line::from(Span::styled(
            format!(
                "  {:<8} {:<20} {:>11} {:>11} {:>9}  {:<12} STATE",
                "PID", "PROCESS", "▼ DOWN KB/s", "▲ UP KB/s", "SESS MB", "LOAD"
            ),
            amber_bold(),
        )),
        Line::from(Span::styled("─".repeat(inner.width as usize), amber())),
    ];

    if procs.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Sampling per-process bandwidth via nettop…",
            amber(),
        )));
    }

    let cap = (inner.height as usize).saturating_sub(2);
    for (idx, p) in procs.iter().take(cap).enumerate() {
        let selected = idx == rs.proc_cursor;
        let ptr = if selected { "▸" } else { " " };
        let state = if p.paused {
            "[PAUSED]"
        } else if p.is_hog {
            "[!] HOG"
        } else if p.protected {
            "PROTECTED"
        } else {
            "● LIVE"
        };
        let load = p.down_kbs + p.up_kbs * 2.5;
        // LOAD: solid-only bar (no dim ░ trail) so idle processes show almost
        // nothing and a real talker visibly stands out — the old ░-filled bar
        // read as one grey rectangle for the many near-zero rows.
        let load_cells = ((load / 2000.0).clamp(0.0, 1.0) * 10.0).round() as usize;
        let load_bar = format!("{:<10}", "█".repeat(load_cells.min(10)));
        let line = format!(
            "{} {:<8} {:<20} {:>8.1} KB  {:>8.1} KB {:>7.1} MB  {}  {}",
            ptr,
            p.pid,
            truncate(&p.name, 20),
            p.down_kbs,
            p.up_kbs,
            p.total_mb,
            load_bar,
            state
        );
        let style = if selected {
            invert()
        } else if p.paused || p.is_hog {
            alert()
        } else if load < 50.0 {
            good()
        } else {
            amber()
        };
        lines.push(Line::from(Span::styled(line, style)));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

/// Middle tile: horizontal bar chart of the top talkers by session data, so the
/// space the short table used to waste now shows a real distribution chart.
fn render_top_talkers(f: &mut Frame, rs: &RenderState, area: Rect) {
    let block = titled_block("TOP TALKERS — SESSION DATA (MB)");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let w = inner.width as usize;

    let mut procs: Vec<&crate::core::procmon::ProcSample> = rs.processes.iter().collect();
    procs.sort_by(|a, b| {
        b.total_mb
            .partial_cmp(&a.total_mb)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let max_mb = procs.first().map(|p| p.total_mb).unwrap_or(1.0).max(1.0);
    let name_w = 16usize;
    let bar_w = w.saturating_sub(name_w + 14).clamp(8, 60);

    let cap = (inner.height as usize).max(1);
    let mut lines = Vec::new();
    for p in procs.iter().take(cap) {
        let filled = ((p.total_mb / max_mb) * bar_w as f64).round() as usize;
        let filled = filled.min(bar_w);
        let gbar = format!("{}{}", "█".repeat(filled), "░".repeat(bar_w - filled));
        let style = if p.is_hog {
            alert()
        } else if p.total_mb >= max_mb * 0.5 {
            amber_bold()
        } else {
            amber()
        };
        lines.push(Line::from(Span::styled(
            format!(
                "{:<name_w$} {gbar} {:>7.1} MB",
                truncate(&p.name, name_w),
                p.total_mb
            ),
            style,
        )));
    }
    if procs.is_empty() {
        lines.push(Line::from(Span::styled("  (sampling…)", amber())));
    }
    f.render_widget(Paragraph::new(lines), inner);
}
fn render_detail(f: &mut Frame, rs: &RenderState, area: Rect) {
    let block = titled_block("SELECTED PROCESS");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let bw = (inner.width as usize).saturating_sub(18).clamp(8, 28);

    let Some(p) = rs.processes.get(rs.proc_cursor) else {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled("  (no process selected)", amber()))),
            inner,
        );
        return;
    };
    let state = if p.paused {
        ("FROZEN (SIGSTOP)", alert())
    } else if p.is_hog {
        ("BANDWIDTH HOG", alert())
    } else if p.protected {
        ("PROTECTED (system)", amber())
    } else {
        ("LIVE", good())
    };
    let lines = vec![
        Line::from(Span::styled(
            format!("{}  (pid {})", p.name, p.pid),
            amber_bold(),
        )),
        Line::from(Span::styled(
            format!(
                "DOWN {} {:.1} KB/s",
                bar(p.down_kbs, 0.0, 2000.0, bw).trim_matches(|c| c == '[' || c == ']'),
                p.down_kbs
            ),
            good(),
        )),
        Line::from(Span::styled(
            format!(
                "UP   {} {:.1} KB/s",
                bar(p.up_kbs, 0.0, 1000.0, bw).trim_matches(|c| c == '[' || c == ']'),
                p.up_kbs
            ),
            amber(),
        )),
        Line::from(Span::styled(
            format!("Session: {:.1} MB total", p.total_mb),
            amber(),
        )),
        Line::from(Span::styled(format!("State  : {}", state.0), state.1)),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

/// LAN device map tile.
fn render_lan(f: &mut Frame, rs: &RenderState, area: Rect) {
    let lan = &rs.lan_data;
    let block = titled_block("LAN DEVICE MAP");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = vec![Line::from(Span::styled(
        format!("{} device(s) · {}", lan.count, lan.summary),
        if lan.ip_conflict {
            alert()
        } else {
            amber_bold()
        },
    ))];
    let cap = (inner.height as usize).saturating_sub(1);
    for d in lan.devices.iter().take(cap) {
        let line = format!("{:<15} {} {}", d.ip, d.vendor, d.role);
        lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                truncate(&line, (inner.width as usize).saturating_sub(2))
            ),
            amber(),
        )));
    }
    if lan.devices.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Press [N] to run a LAN sweep.",
            amber(),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}
