//! Tab [3] Per-process bandwidth firewall with SIGSTOP/SIGCONT control.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::render::RenderState;
use crate::ui::theme::{alert, amber, amber_bold, bar, good, invert};
use crate::ui::titled_block;

pub fn render(f: &mut Frame, rs: &RenderState, area: Rect) {
    let procs = &rs.processes;
    let block = titled_block("PER-PROCESS BANDWIDTH FIREWALL ([Space] Pause │ [U] Resume All)");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = vec![
        Line::from(Span::styled(
            format!(
                "  {:<8} {:<22} {:>12} {:>12} {:>10}  {:<12} STATE",
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

    for (idx, p) in procs.iter().enumerate() {
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
        let line = format!(
            "{} {:<8} {:<22} {:>9.1} KB  {:>9.1} KB {:>8.1} MB  {:<12} {}",
            ptr,
            p.pid,
            truncate(&p.name, 22),
            p.down_kbs,
            p.up_kbs,
            p.total_mb,
            bar(load, 0.0, 2000.0, 10),
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

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}
