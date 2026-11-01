//! Tab [4] Kernel/DNS/Privacy tuning lab. Ported from optimizer.py.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::preset_engine::preset_definitions;
use crate::render::RenderState;
use crate::ui::theme::{alert, amber, amber_bold, bar, good, invert};
use crate::ui::titled_block;

pub fn render(f: &mut Frame, rs: &RenderState, area: Rect) {
    let rows = Layout::vertical([Constraint::Length(6), Constraint::Min(6)]).split(area);
    render_preset_bar(f, rs, rows[0]);
    let cols =
        Layout::horizontal([Constraint::Percentage(54), Constraint::Percentage(46)]).split(rows[1]);
    render_controls(f, rs, cols[0]);
    render_wan_doctor(f, rs, cols[1]);
}

fn render_preset_bar(f: &mut Frame, rs: &RenderState, area: Rect) {
    let auto = rs.auto_pilot_enabled;
    let title = format!(
        "CLOSED-LOOP AUTO-ADAPT PRESET ENGINE ([A] Auto-Adapt: {} │ [P] Cycle Preset)",
        if auto { "● ON" } else { "○ MANUAL" }
    );
    let block = titled_block(&title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let defs = preset_definitions();
    let eff = rs.preset_effective_id;
    let sel = rs.preset_selected_id;

    let mut pills = Vec::new();
    for p in &defs {
        let active = p.id == eff || (p.id == 1 && sel == 1 && auto);
        let style = if active { invert() } else { amber_bold() };
        pills.push(Span::styled(format!(" [{}] ", p.short), style));
    }

    let active_p = defs.iter().find(|p| p.id == eff).unwrap_or(&defs[1]);
    let mode_tag = if auto && sel == 1 {
        "AUTO-ADAPTED"
    } else {
        "MANUAL LOCK"
    };
    let issues = rs
        .preset_detected_issues
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(" │ ");

    let lines = vec![
        Line::from(pills),
        Line::from(Span::styled(
            format!(
                "ACTIVE PRESET : {} [{mode_tag}] ── {}",
                active_p.name, rs.preset_auto_reason
            ),
            good(),
        )),
        Line::from(Span::styled(
            format!("DIAGNOSIS LOOP: {issues}"),
            if issues.contains("[!]") {
                alert()
            } else {
                good()
            },
        )),
        Line::from(Span::styled(
            format!("VERIFICATION  : {}", rs.preset_before_after),
            good(),
        )),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_controls(f: &mut Frame, rs: &RenderState, area: Rect) {
    let block = titled_block("12 WORKLOAD TUNING MODULES (j/k Select, Space/Key Toggle)");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let controls = &rs.tuning_controls;
    let name_w = (inner.width as usize).saturating_sub(34).max(18);
    let mut lines = Vec::new();
    for (idx, c) in controls.iter().enumerate() {
        let selected = idx == rs.preset_cursor_idx;
        let ptr = if selected { "▸" } else { " " };
        let pill = if c.active { "[● ON ]" } else { "[○ OFF]" };
        let name: String = c.name.chars().take(name_w).collect();
        let line = format!(
            "{ptr} {pill} [{}] {:<width$} {}",
            c.key,
            name,
            c.status,
            width = name_w
        );
        let style = if selected {
            invert()
        } else if c.active {
            good()
        } else {
            amber()
        };
        lines.push(Line::from(Span::styled(line, style)));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn render_wan_doctor(f: &mut Frame, rs: &RenderState, area: Rect) {
    let block = titled_block("WAN & YOUTUBE GGC DOCTOR + ECS DNS RACER");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let wd = &rs.wan_snap;
    let active_dns = if rs.telemetry.dns_servers.is_empty() {
        "DHCP Router Default".to_string()
    } else {
        rs.telemetry.dns_servers.join(", ")
    };

    let mut lines = vec![
        Line::from(Span::styled(format!("Bound DNS : {active_dns}"), good())),
        Line::from(Span::styled(
            format!(
                "{:<3} {:<17} {:<14} {:<7} BAR",
                "#", "PROVIDER", "IP", "RTT"
            ),
            amber_bold(),
        )),
    ];

    for (idx, res) in rs.dns_results.iter().take(4).enumerate() {
        let ms = res.avg_ms;
        let speed = (120.0 - ms.min(115.0)).max(5.0);
        let ms_str = if res.reachable {
            format!("{:.0}ms", ms)
        } else {
            "DEAD".to_string()
        };
        let name: String = res.name.chars().take(16).collect();
        let line = format!(
            "#{:<2} {:<17} {:<14} {:<7} {}",
            idx + 1,
            name,
            res.primary,
            ms_str,
            bar(speed, 0.0, 120.0, 6)
        );
        let style = if idx == 0 { good() } else { amber() };
        lines.push(Line::from(Span::styled(line, style)));
    }

    lines.push(Line::from(Span::styled(
        "─".repeat(inner.width as usize),
        amber(),
    )));
    let yt_rtt_s = if wd.yt_rtt_ms > 0.0 {
        format!("{:.0}ms", wd.yt_rtt_ms)
    } else {
        "Probing".into()
    };
    let yt_mbps_s = if wd.yt_single_mbps > 0.0 {
        format!("{:.1}Mbps", wd.yt_single_mbps)
    } else {
        "Probing".into()
    };
    lines.push(Line::from(Span::styled(
        format!(
            "YT GGC  : {} ({yt_rtt_s}) │ Stream: {yt_mbps_s} │ IPv6: {}",
            wd.ggc_node, wd.ipv6_mode
        ),
        if wd.ecs_dns_optimal { good() } else { alert() },
    )));
    lines.push(Line::from(Span::styled(
        format!(
            "TCP SCALE: Scale={}, MSS={} │ WMM QoS={}",
            rs.kernel_state
                .get("win_scale_factor")
                .cloned()
                .unwrap_or_else(|| "8".into()),
            rs.kernel_state
                .get("mssdflt")
                .cloned()
                .unwrap_or_else(|| "1452".into()),
            rs.kernel_state
                .get("wmm_wifi")
                .cloned()
                .unwrap_or_else(|| "0".into()),
        ),
        amber(),
    )));
    let arp = rs.arp_info.status.replace('✔', "[OK]").replace('⚠', "[!]");
    lines.push(Line::from(Span::styled(
        format!("SECURITY: {arp} │ Zombies: {}", wd.zombie_socket_count),
        if arp.contains("[!]") || wd.zombie_socket_count >= 5 {
            alert()
        } else {
            good()
        },
    )));
    lines.push(Line::from(Span::styled(
        "[D] Doctor  [6] IPv6-Guard  [Y] QUIC->TCP  [J] ISP-Ticket  [S] Root Lock",
        good(),
    )));

    f.render_widget(Paragraph::new(lines), inner);
}
