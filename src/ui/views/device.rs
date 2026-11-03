//! Tab [6] Hardware, Silicon & System Cockpit.
//!
//! Provides comprehensive macOS host hardware diagnostics inspired by `mo status`:
//! - Apple Silicon core topology (P-cores vs E-cores)
//! - Mach VM Unified Memory breakdown & macOS memory pressure gauge
//! - Apple Smart Battery health, capacity, cycle count & live wattage
//! - POSIX APFS storage container & live disk throughput
//! - Primary network adapters, hardware MAC vs randomized MAC, tunnels
//! - Top CPU/RAM process consumers
//! - Optional CRT companion mascot (Cat) toggleable with [C]

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::device_info::DeviceInfo;
use crate::render::RenderState;
use crate::ui::theme::{alert, amber, amber_bold, bar, dim, good};
use crate::ui::{draw_kpi_strip, titled_block, Kpi};

pub fn render(f: &mut Frame, rs: &RenderState, area: Rect) {
    let dev = &rs.device;

    let rows = Layout::vertical([
        Constraint::Length(4), // Top KPI scorecard strip
        Constraint::Length(5), // Hardware specification & companion banner
        Constraint::Min(10),   // Main 2-column hardware grid
        Constraint::Length(8), // Top process consumers table
    ])
    .split(area);

    draw_kpis(f, dev, rows[0]);
    draw_banner(f, dev, rows[1]);

    // Split main area into 2 columns: left and right
    let cols = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(rows[2]);

    // Left column: CPU & Topology (60%), Storage (40%)
    let left_tiles = Layout::vertical([
        Constraint::Percentage(62),
        Constraint::Percentage(38),
    ])
    .split(cols[0]);

    draw_cpu_topology(f, dev, left_tiles[0]);
    draw_storage(f, dev, left_tiles[1]);

    // Right column: Memory (42%), Battery & Power (34%), Network & Privacy (24%)
    let right_tiles = Layout::vertical([
        Constraint::Percentage(42),
        Constraint::Percentage(34),
        Constraint::Percentage(24),
    ])
    .split(cols[1]);

    draw_memory(f, dev, right_tiles[0]);
    draw_battery(f, dev, right_tiles[1]);
    draw_network(f, dev, right_tiles[2]);

    // Bottom row: Top resource consumers
    draw_top_processes(f, dev, rows[3]);
}

fn draw_kpis(f: &mut Frame, dev: &DeviceInfo, area: Rect) {
    let cpu_accent = if dev.cpu_total_pct >= 85.0 {
        Some(alert())
    } else {
        Some(good())
    };
    let mem_accent = if dev.mem_pressure_label == "CRITICAL" {
        Some(alert())
    } else {
        Some(amber_bold())
    };
    let batt_accent = if dev.battery_level_pct <= 15.0 && dev.battery_power_source != "AC Power" {
        Some(alert())
    } else {
        Some(good())
    };

    let cards = [
        Kpi {
            title: "CPU LOAD",
            value: format!("{:.1}%", dev.cpu_total_pct),
            unit: &dev.thermal_state,
            history: &dev.cpu_history,
            floor: 0.0,
            accent: cpu_accent,
        },
        Kpi {
            title: "UNIFIED RAM",
            value: format!("{:.1}/{:.0}", dev.mem_used_gb, dev.mem_total_gb),
            unit: "GB USED",
            history: &[],
            floor: 0.0,
            accent: mem_accent,
        },
        Kpi {
            title: "BATTERY",
            value: format!("{:.0}%", dev.battery_level_pct),
            unit: if dev.battery_power_source == "AC Power" {
                "[AC]"
            } else {
                "[BATT]"
            },
            history: &[],
            floor: 0.0,
            accent: batt_accent,
        },
        Kpi {
            title: "SYS PRESSURE",
            value: dev.mem_pressure_label.clone(),
            unit: &dev.uptime_str,
            history: &[],
            floor: 0.0,
            accent: Some(if dev.mem_pressure_label == "NORMAL" {
                good()
            } else {
                alert()
            }),
        },
    ];

    draw_kpi_strip(f, area, &cards);
}

fn draw_banner(f: &mut Frame, dev: &DeviceInfo, area: Rect) {
    let block = titled_block("SYSTEM SPECIFICATION & HOST IDENTITY");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let is_ac = dev.battery_power_source == "AC Power" || dev.battery_status.contains("Charging");
    let pwr_lbl = if is_ac { "[AC]" } else { "[BATT]" };

    let mut lines = Vec::new();
    if dev.mascot_visible {
        lines.push(Line::from(vec![
            Span::styled("  /\\_/\\   ", amber_bold()),
            Span::styled("MODEL: ", dim()),
            Span::styled(format!("{}  ", dev.model_name), amber_bold()),
            Span::styled("CHIP: ", dim()),
            Span::styled(
                format!(
                    "{} ({} Cores: {}P + {}E)",
                    dev.chip_name, dev.total_cores, dev.p_cores, dev.e_cores
                ),
                amber_bold(),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" ( o.o )  ", amber_bold()),
            Span::styled("OS: ", dim()),
            Span::styled(format!("{}  ", dev.os_version), amber()),
            Span::styled("UPTIME: ", dim()),
            Span::styled(format!("{}  ", dev.uptime_str), amber()),
            Span::styled("THERMAL: ", dim()),
            Span::styled(
                format!("[{}]  ", dev.thermal_state),
                if dev.thermal_state == "NOMINAL" {
                    good()
                } else {
                    alert()
                },
            ),
            Span::styled("POWER: ", dim()),
            Span::styled(format!("{pwr_lbl} {:.1}W", dev.battery_watts), good()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  > ^ <   ", amber_bold()),
            Span::styled(
                "[C] toggle companion mascot   [R] refresh hardware",
                dim(),
            ),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled(" ◆ MODEL: ", dim()),
            Span::styled(format!("{}  ", dev.model_name), amber_bold()),
            Span::styled("CHIP: ", dim()),
            Span::styled(
                format!(
                    "{} ({} Cores: {}P + {}E)  ",
                    dev.chip_name, dev.total_cores, dev.p_cores, dev.e_cores
                ),
                amber_bold(),
            ),
            Span::styled("OS: ", dim()),
            Span::styled(dev.os_version.clone(), amber()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" ◆ UPTIME: ", dim()),
            Span::styled(format!("{}  ", dev.uptime_str), amber()),
            Span::styled("THERMAL: ", dim()),
            Span::styled(
                format!("[{}]  ", dev.thermal_state),
                if dev.thermal_state == "NOMINAL" {
                    good()
                } else {
                    alert()
                },
            ),
            Span::styled("POWER: ", dim()),
            Span::styled(format!("{pwr_lbl} {:.1}W", dev.battery_watts), good()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" ◆ ", dim()),
            Span::styled(
                "[C] show companion mascot   [R] refresh hardware metrics",
                dim(),
            ),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_cpu_topology(f: &mut Frame, dev: &DeviceInfo, area: Rect) {
    let block = titled_block("◉ CPU & SILICON TOPOLOGY");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    let (l1, l5, l15) = dev.load_avg;
    let gauge_w = (inner.width as usize).saturating_sub(26).clamp(8, 24);

    // Summary row
    lines.push(Line::from(vec![
        Span::styled(" Total Load: ", amber_bold()),
        Span::styled(
            format!("{:.1}% ", dev.cpu_total_pct),
            if dev.cpu_total_pct > 80.0 {
                alert()
            } else {
                good()
            },
        ),
        Span::styled(
            format!("{} ", bar(dev.cpu_total_pct, 0.0, 100.0, gauge_w)),
            amber(),
        ),
        Span::styled(format!("Load Avg: {:.2} {:.2} {:.2}", l1, l5, l15), dim()),
    ]));

    // Performance Cores
    lines.push(Line::from(Span::styled(
        format!(" Performance Cores ({} P-Cores):", dev.p_cores),
        amber_bold(),
    )));
    for (i, pct) in dev.cpu_cores_pct.iter().take(dev.p_cores).enumerate() {
        let b = bar(*pct, 0.0, 100.0, gauge_w);
        lines.push(Line::from(vec![
            Span::styled(format!("  P{i} "), dim()),
            Span::styled(b, amber()),
            Span::styled(
                format!(" {:>5.1}%", pct),
                if *pct > 85.0 { alert() } else { amber() },
            ),
        ]));
    }

    // Efficiency Cores
    lines.push(Line::from(Span::styled(
        format!(" Efficiency Cores ({} E-Cores):", dev.e_cores),
        amber_bold(),
    )));
    for (i, pct) in dev
        .cpu_cores_pct
        .iter()
        .skip(dev.p_cores)
        .take(dev.e_cores)
        .enumerate()
    {
        let b = bar(*pct, 0.0, 100.0, gauge_w);
        lines.push(Line::from(vec![
            Span::styled(format!("  E{i} "), dim()),
            Span::styled(b, amber()),
            Span::styled(
                format!(" {:>5.1}%", pct),
                if *pct > 85.0 { alert() } else { amber() },
            ),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled(" Thermal State: ", dim()),
        Span::styled(
            format!("[{}]", dev.thermal_state),
            if dev.thermal_state == "NOMINAL" {
                good()
            } else {
                alert()
            },
        ),
        Span::styled("  Silicon Spec: ", dim()),
        Span::styled(
            format!("{} ({} Total Cores)", dev.chip_name, dev.total_cores),
            amber(),
        ),
    ]));

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_storage(f: &mut Frame, dev: &DeviceInfo, area: Rect) {
    let block = titled_block("▥ STORAGE & APFS VOLUMES");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    let used_pct = if dev.disk_total_gb > 0.0 {
        (dev.disk_used_gb / dev.disk_total_gb) * 100.0
    } else {
        0.0
    };
    let gauge_w = (inner.width as usize).saturating_sub(28).clamp(8, 26);
    let disk_bar = bar(used_pct, 0.0, 100.0, gauge_w);

    lines.push(Line::from(vec![Span::styled(
        " Macintosh HD (APFS Container):",
        amber_bold(),
    )]));
    lines.push(Line::from(vec![
        Span::styled("  Usage  ", dim()),
        Span::styled(disk_bar, amber()),
        Span::styled(
            format!(" {:.1}%", used_pct),
            if used_pct > 90.0 {
                alert()
            } else {
                amber()
            },
        ),
    ]));
    lines.push(Line::from(vec![Span::styled(
        format!(
            "  Space  {:.1} GB Used  ·  {:.1} GB Free  ·  {:.1} GB Total",
            dev.disk_used_gb, dev.disk_free_gb, dev.disk_total_gb
        ),
        dim(),
    )]));
    lines.push(Line::from(vec![
        Span::styled("  Throughput: ", dim()),
        Span::styled(format!("Read {:.2} MB/s", dev.disk_read_mbs), good()),
        Span::styled("  ·  ", dim()),
        Span::styled(format!("Write {:.2} MB/s", dev.disk_write_mbs), amber()),
    ]));

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_memory(f: &mut Frame, dev: &DeviceInfo, area: Rect) {
    let block = titled_block("⊞ UNIFIED MEMORY & PRESSURE");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    let used_pct = if dev.mem_total_gb > 0.0 {
        (dev.mem_used_gb / dev.mem_total_gb) * 100.0
    } else {
        0.0
    };
    let gauge_w = (inner.width as usize).saturating_sub(30).clamp(8, 26);

    lines.push(Line::from(vec![
        Span::styled(" Used RAM: ", amber_bold()),
        Span::styled(bar(used_pct, 0.0, 100.0, gauge_w), amber()),
        Span::styled(
            format!(
                " {:.1} GB / {:.1} GB ({:.0}%)",
                dev.mem_used_gb, dev.mem_total_gb, used_pct
            ),
            amber_bold(),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  • App Memory  : ", dim()),
        Span::styled(format!("{:.2} GB", dev.mem_app_gb), amber()),
        Span::styled("    • Wired Memory: ", dim()),
        Span::styled(format!("{:.2} GB", dev.mem_wired_gb), amber()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  • Compressed  : ", dim()),
        Span::styled(format!("{:.2} GB", dev.mem_compressed_gb), amber()),
        Span::styled("    • Swap Used   : ", dim()),
        Span::styled(
            format!("{:.1} MB", dev.mem_swap_mb),
            if dev.mem_swap_mb > 500.0 {
                alert()
            } else {
                dim()
            },
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  • Free / Cache: ", dim()),
        Span::styled(
            format!("{:.2} GB / {:.2} GB", dev.mem_free_gb, dev.mem_cached_gb),
            good(),
        ),
    ]));

    let pressure_style = match dev.mem_pressure_label.as_str() {
        "NORMAL" => good(),
        "WARN" | "WARNING" => alert(),
        _ => alert(),
    };
    lines.push(Line::from(vec![
        Span::styled(" Memory Pressure: ", dim()),
        Span::styled(bar(dev.mem_pressure_pct, 0.0, 100.0, 16), pressure_style),
        Span::styled(
            format!(
                " [{}] ({:.0}%)",
                dev.mem_pressure_label, dev.mem_pressure_pct
            ),
            pressure_style,
        ),
    ]));

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_battery(f: &mut Frame, dev: &DeviceInfo, area: Rect) {
    let block = titled_block("◪ BATTERY & POWER SUBSYSTEM");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    let gauge_w = (inner.width as usize).saturating_sub(32).clamp(8, 22);

    let is_ac = dev.battery_power_source == "AC Power" || dev.battery_status.contains("Charging");
    let pwr_badge = if is_ac { "[AC CONNECTED]" } else { "[BATTERY]" };
    let pwr_style = if is_ac { good() } else { amber() };

    lines.push(Line::from(vec![
        Span::styled(" Charge Level : ", amber_bold()),
        Span::styled(bar(dev.battery_level_pct, 0.0, 100.0, gauge_w), amber()),
        Span::styled(
            format!(" {:.1}% ", dev.battery_level_pct),
            amber_bold(),
        ),
        Span::styled(pwr_badge, pwr_style),
    ]));

    let health_style = if dev.battery_health_pct < 80.0 {
        alert()
    } else {
        good()
    };
    lines.push(Line::from(vec![
        Span::styled(" Battery Health: ", dim()),
        Span::styled(format!("{:.1}% ", dev.battery_health_pct), health_style),
        Span::styled(
            format!("({})", dev.battery_condition),
            if dev.battery_condition == "Normal" {
                dim()
            } else {
                alert()
            },
        ),
        Span::styled(
            format!("  ·  Cycle Count: {} cycles", dev.battery_cycles),
            dim(),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled(" Power Flow    : ", dim()),
        Span::styled(format!("{:.1}W", dev.battery_watts), amber_bold()),
        Span::styled(format!("  ·  Status: {}", dev.battery_status), dim()),
        Span::styled(
            if !dev.battery_adapter.is_empty() {
                format!("  ·  Adapter: {}", dev.battery_adapter)
            } else {
                String::new()
            },
            dim(),
        ),
    ]));

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_network(f: &mut Frame, dev: &DeviceInfo, area: Rect) {
    let block = titled_block("⇄ NETWORK ADAPTERS & PRIVACY");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    let mac_badge = if dev.net_mac_is_private {
        "[PRIVATE MAC ACTIVE]"
    } else {
        "[HARDWARE MAC]"
    };
    let mac_style = if dev.net_mac_is_private {
        good()
    } else {
        dim()
    };

    lines.push(Line::from(vec![
        Span::styled(
            format!(" Primary: {} ", dev.net_primary_iface),
            amber_bold(),
        ),
        Span::styled(format!("({})", dev.net_ip), amber()),
        Span::styled(format!("  MAC: {} ", dev.net_mac_hardware), dim()),
        Span::styled(mac_badge, mac_style),
    ]));

    let awdl_str = if dev.net_awdl_active {
        "awdl0: Active (AirDrop/P2P)"
    } else {
        "awdl0: Standby"
    };
    let tunnels_str = if dev.net_tunnels.is_empty() {
        "No VPN / utun tunnels".to_string()
    } else {
        format!("Tunnels: {}", dev.net_tunnels.join(", "))
    };

    lines.push(Line::from(vec![Span::styled(
        format!(" Direct Link: {}  ·  {}", awdl_str, tunnels_str),
        dim(),
    )]));

    lines.push(Line::from(vec![Span::styled(
        format!(
            " Session Total: RX {:.1} MB  ·  TX {:.1} MB",
            dev.net_rx_mb, dev.net_tx_mb
        ),
        dim(),
    )]));

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_top_processes(f: &mut Frame, dev: &DeviceInfo, area: Rect) {
    let block = titled_block("✱ TOP PROCESS CONSUMERS");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    // Header
    lines.push(Line::from(vec![
        Span::styled("   PID   ", dim()),
        Span::styled("NAME                ", dim()),
        Span::styled("CPU %    ", dim()),
        Span::styled("METER           ", dim()),
        Span::styled("RAM MB    ", dim()),
        Span::styled("STATE       ", dim()),
        Span::styled("USER", dim()),
    ]));

    for p in dev.top_processes.iter().take(5) {
        let cpu_bar = bar(p.cpu_pct, 0.0, 100.0, 14);
        let cpu_style = if p.cpu_pct > 50.0 { alert() } else { amber() };
        lines.push(Line::from(vec![
            Span::styled(format!(" {:<7}", p.pid), dim()),
            Span::styled(
                format!(
                    "{:<20}",
                    if p.name.len() > 19 {
                        format!("{}…", &p.name[..18])
                    } else {
                        p.name.clone()
                    }
                ),
                amber_bold(),
            ),
            Span::styled(format!("{:>5.1}%   ", p.cpu_pct), cpu_style),
            Span::styled(format!("{cpu_bar}  "), amber()),
            Span::styled(format!("{:>6.1} MB  ", p.mem_mb), amber()),
            Span::styled(format!("{:<12}", p.state), dim()),
            Span::styled(format!("{:<10}", p.user), dim()),
        ]));
    }

    if dev.top_processes.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (sampling process metrics...)",
            dim(),
        )));
    }

    f.render_widget(Paragraph::new(lines), inner);
}
