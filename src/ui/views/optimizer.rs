//! Tab [4] TUNE & FIX — toggle/run model. Replaces the old cryptic hotkey grid
//! with a navigable list: ↑↓ to move, Space/Enter to flip a toggle or run an
//! action. Every row has a plain-English name and one-line description.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::preset_engine::preset_definitions;
use crate::render::{RenderState, TuneKind};
use crate::ui::theme::{alert, amber, amber_bold, dim, good, invert};
use crate::ui::titled_block;

pub fn render(f: &mut Frame, rs: &RenderState, area: Rect) {
    let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(6)]).split(area);
    render_preset_radio(f, rs, rows[0]);
    let cols =
        Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)]).split(rows[1]);
    render_tune_list(f, rs, cols[0]);
    render_detail(f, rs, cols[1]);
}

fn render_preset_radio(f: &mut Frame, rs: &RenderState, area: Rect) {
    let auto = rs.auto_pilot_enabled;
    let block = titled_block("PRESET  (P to cycle · A toggles Auto-Pilot)");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let defs = preset_definitions();
    let eff = rs.preset_effective_id;
    let sel = rs.preset_selected_id;
    let mut spans = Vec::new();
    for p in &defs {
        let active = p.id == eff || (p.id == 1 && sel == 1 && auto);
        let dot = if active { "(●)" } else { "( )" };
        // Short label without the leading symbols.
        let name = p.short.split_once(':').map(|(_, b)| b).unwrap_or(p.short);
        let style = if active { invert() } else { amber() };
        spans.push(Span::styled(format!(" {dot} {name} "), style));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn render_tune_list(f: &mut Frame, rs: &RenderState, area: Rect) {
    let block = titled_block("TUNE & FIX");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    for (idx, row) in rs.tune_rows.iter().enumerate() {
        match row.kind {
            TuneKind::Header => {
                lines.push(Line::from(Span::styled(
                    format!("── {} ", row.label),
                    dim(),
                )));
            }
            TuneKind::Toggle => {
                let selected = idx == rs.tune_cursor;
                let ptr = if selected { "▸" } else { " " };
                let on = row.state.unwrap_or(false);
                let pill = if on { "[ ● ON  ]" } else { "[ ○ OFF ]" };
                let pill_style = if on { good() } else { amber() };
                let name_style = if selected { invert() } else { amber() };
                lines.push(Line::from(vec![
                    Span::styled(format!("{ptr} "), amber_bold()),
                    Span::styled(pill.to_string(), pill_style),
                    Span::raw("  "),
                    Span::styled(format!("{:<28}", row.label), name_style),
                ]));
            }
            TuneKind::Run => {
                let selected = idx == rs.tune_cursor;
                let ptr = if selected { "▸" } else { " " };
                let name_style = if selected { invert() } else { amber() };
                lines.push(Line::from(vec![
                    Span::styled(format!("{ptr} "), amber_bold()),
                    Span::styled("[  RUN  ]".to_string(), amber_bold()),
                    Span::raw("  "),
                    Span::styled(format!("{:<28}", row.label), name_style),
                ]));
            }
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_detail(f: &mut Frame, rs: &RenderState, area: Rect) {
    let rows = Layout::vertical([Constraint::Length(5), Constraint::Min(4)]).split(area);

    // Top: the selected row's plain-English explanation.
    let sel = rs.tune_rows.get(rs.tune_cursor);
    let block = titled_block("WHAT THIS DOES");
    let inner = block.inner(rows[0]);
    f.render_widget(block, rows[0]);
    if let Some(row) = sel {
        let state_line = match row.state {
            Some(true) => Span::styled("Currently: ON", good()),
            Some(false) => Span::styled("Currently: OFF", amber()),
            None => Span::styled("One-shot action", amber_bold()),
        };
        let lines = vec![
            Line::from(Span::styled(row.label.clone(), amber_bold())),
            Line::from(Span::styled(row.detail.clone(), amber())),
            Line::from(state_line),
        ];
        f.render_widget(
            Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: true }),
            inner,
        );
    }

    // Bottom: live WAN doctor summary so the page still shows network health.
    let wd = &rs.wan_snap;
    let block2 = titled_block("WAN & DNS HEALTH");
    let inner2 = block2.inner(rows[1]);
    f.render_widget(block2, rows[1]);
    let dns = if rs.telemetry.dns_servers.is_empty() {
        "DHCP Router Default".to_string()
    } else {
        rs.telemetry.dns_servers.join(", ")
    };
    let yt = if wd.yt_rtt_ms > 0.0 {
        format!("{:.0}ms", wd.yt_rtt_ms)
    } else {
        "probing".into()
    };
    let arp = rs.arp_info.status.replace('✔', "[OK]").replace('⚠', "[!]");
    let lines = vec![
        Line::from(Span::styled(format!("DNS     : {dns}"), good())),
        Line::from(Span::styled(
            format!("YouTube : {} ({yt})", wd.ggc_node),
            if wd.ecs_dns_optimal { good() } else { alert() },
        )),
        Line::from(Span::styled(format!("IPv6    : {}", wd.ipv6_mode), amber())),
        Line::from(Span::styled(
            format!("Security: {arp}"),
            if arp.contains("[!]") { alert() } else { good() },
        )),
        Line::from(Span::styled(
            format!("Zombies : {} stuck sockets", wd.zombie_socket_count),
            if wd.zombie_socket_count >= 5 {
                alert()
            } else {
                amber()
            },
        )),
    ];
    f.render_widget(Paragraph::new(lines), inner2);
}
