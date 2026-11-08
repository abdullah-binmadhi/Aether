//! Tab [8] LAN Radar & Subnet Airspace Cockpit.
//!
//! Provides local network host discovery, hardware OUI vendor resolution,
//! single-host TCP port scanning, and rogue ARP intruder detection.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::core::lan_scanner::{LanData, SCAN_PORTS};
use crate::render::RenderState;
use crate::ui::theme::{alert, amber, amber_bold, dim, good, invert};
use crate::ui::{draw_kpi_strip, titled_block, Kpi};

pub fn render(f: &mut Frame, rs: &RenderState, area: Rect) {
    let lan = &rs.lan_data;

    let rows = Layout::vertical([
        Constraint::Length(4), // Top KPI scorecard strip
        Constraint::Min(12),   // Main 2-column LAN grid
    ])
    .split(area);

    draw_kpis(f, lan, rows[0]);

    // Split main area into 2 columns: left (device table) and right (inspector + security log)
    let cols = Layout::horizontal([
        Constraint::Percentage(58),
        Constraint::Percentage(42),
    ])
    .split(rows[1]);

    draw_device_table(f, lan, cols[0]);

    // Right column: Host inspector (58%), Subnet Integrity (42%)
    let right_tiles = Layout::vertical([
        Constraint::Percentage(58),
        Constraint::Percentage(42),
    ])
    .split(cols[1]);

    draw_host_inspector(f, lan, right_tiles[0]);
    draw_subnet_security(f, rs, lan, right_tiles[1]);
}

fn draw_kpis(f: &mut Frame, lan: &LanData, area: Rect) {
    let sec_accent = if lan.ip_conflict {
        Some(alert())
    } else {
        Some(good())
    };

    let (sel_ip, sel_vendor) = if let Some(d) = lan.devices.get(lan.selected_idx) {
        (d.ip.clone(), d.vendor.as_str())
    } else {
        ("None".to_string(), "No host")
    };

    let cards = [
        Kpi {
            title: "ACTIVE HOSTS",
            value: format!("{} Nodes", lan.count),
            unit: &lan.subnet_cidr,
            history: &[],
            floor: 0.0,
            accent: Some(good()),
        },
        Kpi {
            title: "GATEWAY RTT",
            value: format!("{:.1} ms", lan.gateway_rtt_ms),
            unit: &lan.gateway_ip,
            history: &[],
            floor: 0.0,
            accent: Some(if lan.gateway_rtt_ms < 10.0 { good() } else { alert() }),
        },
        Kpi {
            title: "SECURITY POSTURE",
            value: if lan.ip_conflict { "ALERT".into() } else { "SECURE".into() },
            unit: if lan.ip_conflict { "Duplicate MAC" } else { "Zero Spoof" },
            history: &[],
            floor: 0.0,
            accent: sec_accent,
        },
        Kpi {
            title: "SELECTED HOST",
            value: sel_ip,
            unit: sel_vendor,
            history: &[],
            floor: 0.0,
            accent: Some(amber_bold()),
        },
    ];

    draw_kpi_strip(f, area, &cards);
}

fn draw_device_table(f: &mut Frame, lan: &LanData, area: Rect) {
    let title = format!("⌖ LOCAL SUBNET HOSTS ({} Online)", lan.count);
    let block = titled_block(&title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();

    // Table Header
    lines.push(Line::from(vec![
        Span::styled("   IP ADDRESS      ", dim()),
        Span::styled("HOSTNAME           ", dim()),
        Span::styled("HARDWARE SILICON   ", dim()),
        Span::styled("STATUS / ROLE    ", dim()),
        Span::styled("PING", dim()),
    ]));

    let visible_rows = (inner.height as usize).saturating_sub(2);
    let start_idx = if lan.selected_idx >= visible_rows {
        lan.selected_idx + 1 - visible_rows
    } else {
        0
    };

    for (i, d) in lan.devices.iter().enumerate().skip(start_idx).take(visible_rows) {
        let is_selected = i == lan.selected_idx;
        let arrow = if is_selected { "▶ " } else { "  " };

        let ping_str = if let Some(ms) = d.ping_ms {
            format!("{:>4.1}ms", ms)
        } else {
            "  -- ".into()
        };

        let trust_badge = if d.is_self {
            "[YOU]"
        } else if d.is_gateway {
            "[GATEWAY]"
        } else if d.is_trusted {
            "[TRUSTED]"
        } else {
            "[UNVERIFIED]"
        };

        let trust_style = if d.is_self || d.is_gateway {
            good()
        } else if d.is_trusted {
            amber()
        } else {
            dim()
        };

        lines.push(Line::from(vec![
            Span::styled(arrow, if is_selected { amber_bold() } else { dim() }),
            Span::styled(format!("{:<16}", d.ip), if is_selected { invert() } else { amber_bold() }),
            Span::styled(
                format!("{:<19}", truncate(&d.hostname, 18)),
                if is_selected { invert() } else { amber() },
            ),
            Span::styled(
                format!("{:<19}", truncate(&d.vendor, 18)),
                if is_selected { invert() } else { dim() },
            ),
            Span::styled(format!("{:<17}", trust_badge), if is_selected { invert() } else { trust_style }),
            Span::styled(ping_str, if is_selected { invert() } else { good() }),
        ]));
    }

    if lan.devices.is_empty() {
        lines.push(Line::from(Span::styled("  (no network hosts detected - press [R] to scan)", dim())));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_host_inspector(f: &mut Frame, lan: &LanData, area: Rect) {
    let sel_dev = lan.devices.get(lan.selected_idx);
    let title = if let Some(d) = sel_dev {
        format!("◉ HOST INSPECTOR: {}", d.ip)
    } else {
        "◉ HOST INSPECTOR".to_string()
    };

    let block = titled_block(&title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();

    if let Some(d) = sel_dev {
        lines.push(Line::from(vec![
            Span::styled(" Hostname    : ", dim()),
            Span::styled(&d.hostname, amber_bold()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" Hardware OUI: ", dim()),
            Span::styled(&d.vendor, amber()),
            Span::styled(format!("  ({})", d.category), dim()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" MAC Address : ", dim()),
            Span::styled(&d.mac, dim()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" Network Role: ", dim()),
            Span::styled(&d.role, amber_bold()),
            Span::styled("  ·  Ping: ", dim()),
            Span::styled(
                if let Some(ms) = d.ping_ms { format!("{ms:.1} ms") } else { "N/A".into() },
                good(),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled(" Trust Posture: ", dim()),
            Span::styled(
                if d.is_trusted { "[TRUSTED PEER]" } else { "[UNVERIFIED NODE]" },
                if d.is_trusted { good() } else { alert() },
            ),
            Span::styled("  (press [Space] to toggle)", dim()),
        ]));

        lines.push(Line::from(Span::styled("──────────────────────────────────────────", dim())));
        lines.push(Line::from(Span::styled(" OPEN TCP SERVICES & PORTS:", amber_bold())));

        if d.open_ports.is_empty() {
            lines.push(Line::from(Span::styled(
                "   (press [P] to run multi-port TCP scan)",
                dim(),
            )));
        } else {
            let mut port_spans = Vec::new();
            port_spans.push(Span::styled("   ", dim()));
            for p in &d.open_ports {
                let name = SCAN_PORTS
                    .iter()
                    .find(|(port, _)| port == p)
                    .map(|(_, name)| *name)
                    .unwrap_or("App");
                port_spans.push(Span::styled(format!("[Port {p}: {name}] "), good()));
            }
            lines.push(Line::from(port_spans));
        }
    } else {
        lines.push(Line::from(Span::styled("  Select a host on the left panel to inspect.", dim())));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_subnet_security(f: &mut Frame, rs: &RenderState, lan: &LanData, area: Rect) {
    let block = titled_block("◪ SUBNET INTEGRITY & ARP / EVIL-TWIN GUARD");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();

    let arp_alert = rs.arp_info.status.contains('⚠') || rs.arp_info.status.contains("[!]");
    let sec_style = if lan.ip_conflict || arp_alert {
        alert()
    } else {
        good()
    };

    lines.push(Line::from(vec![
        Span::styled(" Subnet Status : ", dim()),
        Span::styled(&lan.security_posture, sec_style),
    ]));
    lines.push(Line::from(vec![
        Span::styled(" ARP / BSSID   : ", dim()),
        Span::styled(
            rs.arp_info.status.replace('✔', "[OK]").replace('⚠', "[!]"),
            if arp_alert { alert() } else { good() },
        ),
        Span::styled(format!(" · {}", rs.telemetry.security), dim()),
    ]));
    lines.push(Line::from(vec![
        Span::styled(" Gateway IP/MAC: ", dim()),
        Span::styled(format!("{} [{}]", lan.gateway_ip, lan.gateway_mac), amber()),
    ]));

    lines.push(Line::from(Span::styled(" Audit Event Log:", dim())));

    for log in lan.logs.iter().rev().take(3) {
        let style = if log.contains("[ALERT]") { alert() } else { dim() };
        lines.push(Line::from(Span::styled(format!("  {log}"), style)));
    }

    if lan.logs.is_empty() {
        lines.push(Line::from(Span::styled("  [OK] No ARP poisoning or IP collision events detected.", good())));
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}…", &s[..max.saturating_sub(1)])
    } else {
        s.to_string()
    }
}
