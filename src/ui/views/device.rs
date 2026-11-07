//! Tab [6] Hardware, Silicon & System Cockpit.
//!
//! Provides comprehensive cross-platform host hardware diagnostics:
//! - Exact host model & identity (macOS IODeviceTree, Linux DMI, Windows WMI)
//! - CPU Silicon spec & dynamic core topology (hybrid P+E cores or symmetric core matrix)
//! - GPU Silicon & live telemetry (Apple Silicon AGXAccelerator, NVIDIA, AMD, Intel)
//! - Mach VM / Linux Unified Memory breakdown & memory pressure gauge
//! - Physical NVMe / SATA drive controller model & live I/O throughput
//! - Battery health, cycle count, live wattage, or desktop AC mains detection
//! - Primary network adapters, physical Wi-Fi silicon chipset & MAC privacy
//! - Top CPU/RAM process consumers
//! - Optional CRT companion mascot (Cat) toggleable with [C]

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::device_info::DeviceInfo;
use crate::render::RenderState;
use crate::ui::theme::{alert, amber, amber_bold, bar, dim, good, sparkline};
use crate::ui::{draw_kpi_strip, titled_block, Kpi};

pub fn render(f: &mut Frame, rs: &RenderState, area: Rect) {
    let dev = &rs.device;

    let banner_h = if area.height >= 36 { 6 } else { 5 };
    let proc_h = if area.height >= 40 { 6 } else { 5 };

    let rows = Layout::vertical([
        Constraint::Length(4),        // Top KPI scorecard strip
        Constraint::Length(banner_h), // Hardware specification & identity banner
        Constraint::Min(10),          // Main 2-column hardware grid (dynamically expands)
        Constraint::Length(proc_h),   // Top process consumers table (compact, zero dead space)
    ])
    .split(area);

    draw_kpis(f, dev, rows[0]);
    draw_banner(f, dev, rows[1]);

    // Split main area into 2 balanced columns: left and right
    let cols = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(rows[2]);

    // Left column: CPU & Topology (Min 8, expands dynamically to consume leftover space),
    // GPU Engine (5), Physical Storage (5)
    let left_tiles = Layout::vertical([
        Constraint::Min(8),
        Constraint::Length(5),
        Constraint::Length(5),
    ])
    .split(cols[0]);

    draw_cpu_topology(f, dev, left_tiles[0]);
    draw_gpu(f, dev, left_tiles[1]);
    draw_storage(f, dev, left_tiles[2]);

    // Right column: Memory (Min 8, expands dynamically), Battery & Power (5), Network & Privacy (5)
    let right_tiles = Layout::vertical([
        Constraint::Min(8),
        Constraint::Length(5),
        Constraint::Length(5),
    ])
    .split(cols[1]);

    draw_memory(f, dev, right_tiles[0]);
    draw_battery(f, dev, right_tiles[1]);
    draw_network(f, dev, right_tiles[2]);

    // Bottom row: Top resource consumers (sized to fit proc_h with zero dead space)
    draw_top_processes(f, dev, rows[3]);
}

fn draw_kpis(f: &mut Frame, dev: &DeviceInfo, area: Rect) {
    let cpu_accent = if dev.cpu_total_pct >= 85.0 {
        Some(alert())
    } else {
        Some(good())
    };
    let gpu_accent = if dev.gpu_load_pct >= 85.0 {
        Some(alert())
    } else {
        Some(amber_bold())
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

    let pwr_unit = if dev.battery_power_source == "AC Power" {
        "[AC]"
    } else {
        "[BATT]"
    };

    let is_desktop_no_batt = dev.battery_status == "No Battery"
        || (dev.battery_power_source == "AC Power"
            && dev.battery_cycles == 0
            && dev.battery_level_pct >= 99.0
            && dev.battery_adapter == "Direct AC Line Power");

    let cpu_unit_str = format!("{} CPU CORES", dev.total_cores);
    let gpu_unit_str = if dev.gpu_cores > 0 {
        format!("{} GPU CORES", dev.gpu_cores)
    } else {
        format!("{:.0}M VRAM", dev.gpu_vram_used_mb)
    };

    let cards = [
        Kpi {
            title: "CPU LOAD",
            value: format!("{:.1}%", dev.cpu_total_pct),
            unit: &cpu_unit_str,
            history: &dev.cpu_history,
            floor: 0.0,
            accent: cpu_accent,
        },
        Kpi {
            title: "GPU LOAD",
            value: format!("{:.1}%", dev.gpu_load_pct),
            unit: &gpu_unit_str,
            history: &[],
            floor: 0.0,
            accent: gpu_accent,
        },
        Kpi {
            title: "SYSTEM RAM",
            value: format!("{:.1}/{:.0}", dev.mem_used_gb, dev.mem_total_gb),
            unit: "GB USED",
            history: &[],
            floor: 0.0,
            accent: mem_accent,
        },
        Kpi {
            title: if is_desktop_no_batt { "POWER" } else { "BATTERY" },
            value: if is_desktop_no_batt {
                format!("{:.1}W", dev.battery_watts)
            } else {
                format!("{:.0}%", dev.battery_level_pct)
            },
            unit: pwr_unit,
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
    let cores_desc = if dev.is_hybrid_cpu {
        format!("{} CPU Cores: {}P + {}E", dev.total_cores, dev.p_cores, dev.e_cores)
    } else {
        format!("{} CPU Cores", dev.total_cores)
    };

    let gpu_desc = if dev.gpu_cores > 0 {
        format!("{} ({} GPU Cores)", dev.gpu_name, dev.gpu_cores)
    } else {
        dev.gpu_name.clone()
    };

    if dev.mascot_visible {
        lines.push(Line::from(vec![
            Span::styled("  /\\_/\\   ", amber_bold()),
            Span::styled("MODEL: ", dim()),
            Span::styled(format!("{} [{}]  ", dev.model_name, dev.model_identifier), amber_bold()),
            Span::styled("CHIP: ", dim()),
            Span::styled(format!("{} ({cores_desc})", dev.chip_name), amber_bold()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" ( o.o )  ", amber_bold()),
            Span::styled("GPU: ", dim()),
            Span::styled(format!("{gpu_desc}  "), amber()),
            Span::styled("RAM: ", dim()),
            Span::styled(format!("{:.0} GB {}  ", dev.mem_total_gb, dev.mem_type), amber()),
            Span::styled("SSD: ", dim()),
            Span::styled(format!("{} ({})", dev.disk_model, dev.disk_protocol), amber()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  > ^ <   ", amber_bold()),
            Span::styled("OS: ", dim()),
            Span::styled(format!("{}  ", dev.os_version), amber()),
            Span::styled("WIFI: ", dim()),
            Span::styled(format!("{}  ", dev.wifi_chipset), amber()),
            Span::styled("PWR: ", dim()),
            Span::styled(format!("{pwr_lbl} {:.1}W", dev.battery_watts), good()),
        ]));
        if inner.height >= 4 {
            lines.push(Line::from(vec![
                Span::styled("          ", amber_bold()),
                Span::styled(
                    "[C] toggle companion mascot   [R] refresh hardware metrics",
                    dim(),
                ),
            ]));
        }
    } else {
        lines.push(Line::from(vec![
            Span::styled(" ◆ MODEL: ", dim()),
            Span::styled(format!("{} [{}]  ", dev.model_name, dev.model_identifier), amber_bold()),
            Span::styled("CHIP: ", dim()),
            Span::styled(format!("{} ({cores_desc})  ", dev.chip_name), amber_bold()),
            Span::styled("GPU: ", dim()),
            Span::styled(gpu_desc, amber()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" ◆ RAM: ", dim()),
            Span::styled(format!("{:.0} GB {}  ", dev.mem_total_gb, dev.mem_type), amber()),
            Span::styled("STORAGE: ", dim()),
            Span::styled(format!("{} ({})  ", dev.disk_model, dev.disk_protocol), amber()),
            Span::styled("WIFI: ", dim()),
            Span::styled(dev.wifi_chipset.clone(), amber()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" ◆ OS: ", dim()),
            Span::styled(format!("{}  ", dev.os_version), amber()),
            Span::styled("UPTIME: ", dim()),
            Span::styled(format!("{}  ", dev.uptime_str), amber()),
            Span::styled("THERMAL: ", dim()),
            Span::styled(
                format!("[{}]  ", dev.thermal_state),
                if dev.thermal_state.to_uppercase().contains("NOMINAL") || dev.thermal_state.to_uppercase().contains("COOL") { good() } else { alert() },
            ),
            Span::styled("POWER: ", dim()),
            Span::styled(format!("{pwr_lbl} {:.1}W", dev.battery_watts), good()),
        ]));
        if inner.height >= 4 {
            lines.push(Line::from(vec![
                Span::styled(" ◆ ", dim()),
                Span::styled(
                    "[C] show companion mascot   [R] refresh hardware metrics",
                    dim(),
                ),
            ]));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_cpu_topology(f: &mut Frame, dev: &DeviceInfo, area: Rect) {
    let block = titled_block("◉ CPU & SILICON TOPOLOGY");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let avail_w = inner.width as usize;
    let avail_h = inner.height as usize;
    if avail_w < 20 || avail_h < 3 {
        return;
    }

    let mut lines = Vec::new();
    let (l1, l5, _l15) = dev.load_avg;

    // Line 1: Silicon Identity & Core Breakdown
    let cores_spec = if dev.is_hybrid_cpu {
        format!("{} ({} CPU Cores: {}P + {}E)", dev.chip_name, dev.total_cores, dev.p_cores, dev.e_cores)
    } else {
        format!("{} ({} CPU Cores)", dev.chip_name, dev.total_cores)
    };
    lines.push(Line::from(vec![
        Span::styled(" Silicon: ", dim()),
        Span::styled(cores_spec, amber_bold()),
    ]));

    // Line 2: Total Load Meter + Thermal + Load Average (Strictly budgeted to never wrap)
    let therm_str = if dev.thermal_state.to_uppercase().contains("NOMINAL") || dev.thermal_state.to_uppercase().contains("COOL") {
        "[Cool]"
    } else if dev.thermal_state.to_uppercase().contains("FAIR") {
        "[Fair]"
    } else {
        "[Hot]"
    };
    let therm_style = if therm_str == "[Cool]" { good() } else { alert() };

    let load_fixed_w = 7 + 6 + 10 + 12; // ~35 chars
    let gauge_w = avail_w.saturating_sub(load_fixed_w).clamp(4, 16);

    lines.push(Line::from(vec![
        Span::styled(" Load: ", dim()),
        Span::styled(
            format!("{:>5.1}% ", dev.cpu_total_pct),
            if dev.cpu_total_pct > 80.0 { alert() } else { good() },
        ),
        Span::styled(bar(dev.cpu_total_pct, 0.0, 100.0, gauge_w), amber()),
        Span::styled(format!(" {therm_str}"), therm_style),
        Span::styled(format!("  1m:{:.1} 5m:{:.1}", l1, l5), dim()),
    ]));

    let cols_per_row: usize = if avail_w >= 140 {
        6
    } else if avail_w >= 92 {
        4
    } else if avail_w >= 44 {
        2
    } else {
        1
    };

    let total_seps = cols_per_row.saturating_sub(1) * 3;
    let cell_w = avail_w.saturating_sub(total_seps + 2) / cols_per_row.max(1);
    let mini_bar_w = cell_w.saturating_sub(13).clamp(3, 11);

    let render_chunk = |chunk: &[(usize, f64, &str)]| -> Line<'static> {
        let mut spans = Vec::new();
        for (i, &(idx, pct, prefix)) in chunk.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(" │ ", dim()));
            }

            let bar_str = bar(pct, 0.0, 100.0, mini_bar_w);
            let val_style = if pct > 85.0 {
                alert()
            } else if pct > 50.0 {
                amber_bold()
            } else {
                amber()
            };

            let lbl = if idx < 10 {
                format!(" {prefix}{idx} ")
            } else {
                format!(" {prefix}{idx}")
            };
            spans.push(Span::styled(lbl, dim()));
            spans.push(Span::styled(bar_str, amber()));
            spans.push(Span::styled(format!(" {:>5.1}%", pct), val_style));

            let used_w = 4 + mini_bar_w + 7;
            let pad = cell_w.saturating_sub(used_w);
            if pad > 0 && i + 1 < chunk.len() {
                spans.push(Span::raw(" ".repeat(pad)));
            }
        }
        Line::from(spans)
    };

    let total_measured = dev.cpu_cores_pct.len();

    if dev.is_hybrid_cpu {
        let p_count = dev.p_cores.min(total_measured);
        lines.push(Line::from(Span::styled(
            format!(" Performance Cores ({} P-Cores):", p_count),
            amber_bold(),
        )));

        let p_vec: Vec<(usize, f64, &'static str)> = dev
            .cpu_cores_pct
            .iter()
            .take(p_count)
            .copied()
            .enumerate()
            .map(|(idx, pct)| (idx, pct, "P"))
            .collect();

        for chunk in p_vec.chunks(cols_per_row) {
            lines.push(render_chunk(chunk));
        }

        let e_start = p_count;
        let e_end = (p_count + dev.e_cores).min(total_measured);
        let e_count = e_end.saturating_sub(e_start);

        if e_count > 0 {
            lines.push(Line::from(Span::styled(
                format!(" Efficiency Cores ({} E-Cores):", e_count),
                amber_bold(),
            )));

            let e_vec: Vec<(usize, f64, &'static str)> = dev
                .cpu_cores_pct
                .iter()
                .skip(e_start)
                .take(e_count)
                .copied()
                .enumerate()
                .map(|(idx, pct)| (idx, pct, "E"))
                .collect();

            for chunk in e_vec.chunks(cols_per_row) {
                lines.push(render_chunk(chunk));
            }
        }

        // Remainder cores if any
        let rem_start = e_end;
        if rem_start < total_measured {
            let rem_count = total_measured - rem_start;
            lines.push(Line::from(Span::styled(
                format!(" Additional Compute Units ({} Cores):", rem_count),
                amber_bold(),
            )));

            let rem_vec: Vec<(usize, f64, &'static str)> = dev
                .cpu_cores_pct
                .iter()
                .skip(rem_start)
                .copied()
                .enumerate()
                .map(|(idx, pct)| (rem_start + idx, pct, "C"))
                .collect();

            for chunk in rem_vec.chunks(cols_per_row) {
                lines.push(render_chunk(chunk));
            }
        }
    } else {
        lines.push(Line::from(Span::styled(
            format!(" Processor Compute Cores ({} Cores):", total_measured),
            amber_bold(),
        )));

        let c_vec: Vec<(usize, f64, &'static str)> = dev
            .cpu_cores_pct
            .iter()
            .copied()
            .enumerate()
            .map(|(idx, pct)| (idx, pct, "C"))
            .collect();

        for chunk in c_vec.chunks(cols_per_row) {
            lines.push(render_chunk(chunk));
        }
    }

    // Dynamic Vertical Filler: Eliminates dead space dynamically based on available height
    let remaining_h = avail_h.saturating_sub(lines.len());

    if remaining_h >= 2 {
        let spark_w = avail_w.saturating_sub(19).clamp(8, 28);
        let spark = sparkline(&dev.cpu_history, spark_w, 0.0);
        lines.push(Line::from(vec![
            Span::styled(" Load Trend (30s): ", dim()),
            Span::styled(spark, amber_bold()),
        ]));
    }

    if remaining_h >= 3 {
        lines.push(Line::from(vec![
            Span::styled(" Arch: ", dim()),
            Span::styled(format!("{}  ·  ", dev.soc_architecture), amber()),
            Span::styled(if dev.is_hybrid_cpu { "P+E Hybrid Heterogeneous" } else { "SMP Symmetric Matrix" }, dim()),
        ]));
    }

    if remaining_h >= 4 {
        lines.push(Line::from(vec![
            Span::styled(" Cache: ", dim()),
            Span::styled(if dev.is_hybrid_cpu {
                "L1 192KB/128KB  ·  L2 16MB (P) / 4MB (E)"
            } else {
                "Unified High-Speed L2/L3 Cache Hierarchy"
            }, amber()),
        ]));
    }

    if remaining_h >= 5 {
        lines.push(Line::from(vec![
            Span::styled(" Scheduler: ", dim()),
            Span::styled(format!("Mach Ticks · 1m:{:.1}  5m:{:.1}  15m:{:.1}", l1, l5, dev.load_avg.2), dim()),
        ]));
    }

    if remaining_h == 1 {
        lines.push(Line::from(vec![
            Span::styled(" Arch: ", dim()),
            Span::styled(format!("{} · [{}]", dev.soc_architecture, dev.thermal_state), amber()),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_gpu(f: &mut Frame, dev: &DeviceInfo, area: Rect) {
    let block = titled_block("▦ GPU & GRAPHICS ACCELERATOR");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    let gauge_w = (inner.width as usize).saturating_sub(28).clamp(8, 22);

    let gpu_sub = if dev.gpu_cores > 0 {
        format!("· {} GPU Cores [Metal 3]", dev.gpu_cores)
    } else {
        format!("[{}]", dev.soc_architecture)
    };

    lines.push(Line::from(vec![
        Span::styled(format!(" {} ", dev.gpu_name), amber_bold()),
        Span::styled(gpu_sub, amber()),
    ]));

    let load_bar = bar(dev.gpu_load_pct, 0.0, 100.0, gauge_w);
    let load_style = if dev.gpu_load_pct > 80.0 { alert() } else { good() };

    lines.push(Line::from(vec![
        Span::styled("  Utilization : ", dim()),
        Span::styled(load_bar, amber()),
        Span::styled(format!(" {:>5.1}%", dev.gpu_load_pct), load_style),
    ]));

    lines.push(Line::from(vec![
        Span::styled("  Active VRAM : ", dim()),
        Span::styled(format!("{:.0} MB", dev.gpu_vram_used_mb), amber()),
        Span::styled("  ·  Pool: ", dim()),
        Span::styled(format!("{:.0} MB (Unified)", dev.gpu_vram_alloc_mb), amber_bold()),
    ]));

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_storage(f: &mut Frame, dev: &DeviceInfo, area: Rect) {
    let block = titled_block("▥ STORAGE & PHYSICAL DRIVE");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    let used_pct = if dev.disk_total_gb > 0.0 {
        (dev.disk_used_gb / dev.disk_total_gb) * 100.0
    } else {
        0.0
    };
    let gauge_w = (inner.width as usize).saturating_sub(30).clamp(6, 20);
    let disk_bar = bar(used_pct, 0.0, 100.0, gauge_w);

    lines.push(Line::from(vec![
        Span::styled(format!(" {} ", dev.disk_model), amber_bold()),
        Span::styled(format!("({})", dev.disk_protocol), dim()),
    ]));

    lines.push(Line::from(vec![
        Span::styled("  Capacity : ", dim()),
        Span::styled(disk_bar, amber()),
        Span::styled(
            format!(" {:.1}% ", used_pct),
            if used_pct > 90.0 { alert() } else { amber() },
        ),
        Span::styled(format!("({:.0}/{:.0} GB)", dev.disk_used_gb, dev.disk_total_gb), dim()),
    ]));

    lines.push(Line::from(vec![
        Span::styled("  Live I/O : ", dim()),
        Span::styled(format!("Read {:.2} MB/s", dev.disk_read_mbs), good()),
        Span::styled("  ·  ", dim()),
        Span::styled(format!("Write {:.2} MB/s", dev.disk_write_mbs), amber()),
    ]));

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_memory(f: &mut Frame, dev: &DeviceInfo, area: Rect) {
    let block = titled_block("⊞ SYSTEM MEMORY & ARCHITECTURE");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    let used_pct = if dev.mem_total_gb > 0.0 {
        (dev.mem_used_gb / dev.mem_total_gb) * 100.0
    } else {
        0.0
    };
    let gauge_w = (inner.width as usize).saturating_sub(30).clamp(8, 24);

    lines.push(Line::from(vec![
        Span::styled(format!(" RAM: {:.0} GB ", dev.mem_total_gb), amber_bold()),
        Span::styled(format!("({})", dev.mem_type), dim()),
    ]));

    lines.push(Line::from(vec![
        Span::styled(" Used RAM: ", amber_bold()),
        Span::styled(bar(used_pct, 0.0, 100.0, gauge_w), amber()),
        Span::styled(
            format!(
                " {:.1}/{:.0} GB ({:.0}%)",
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
            if dev.mem_swap_mb > 500.0 { alert() } else { dim() },
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
        Span::styled(bar(dev.mem_pressure_pct, 0.0, 100.0, 14), pressure_style),
        Span::styled(
            format!(
                " [{}] ({:.0}%)",
                dev.mem_pressure_label, dev.mem_pressure_pct
            ),
            pressure_style,
        ),
    ]));

    let avail_h = inner.height as usize;
    let remaining_h = avail_h.saturating_sub(lines.len());
    if remaining_h >= 2 {
        lines.push(Line::from(vec![
            Span::styled(" Memory Fabric: ", dim()),
            Span::styled("Unified High-Speed Memory Architecture", amber()),
        ]));
    }
    if remaining_h >= 3 {
        lines.push(Line::from(vec![
            Span::styled(" Page Subsystem: ", dim()),
            Span::styled("Mach VM 16 KB Hardware Page Granularity", dim()),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_battery(f: &mut Frame, dev: &DeviceInfo, area: Rect) {
    let block = titled_block("◪ BATTERY & POWER SUBSYSTEM");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    let gauge_w = (inner.width as usize).saturating_sub(32).clamp(8, 22);

    let is_desktop_no_batt = dev.battery_status == "No Battery"
        || (dev.battery_power_source == "AC Power"
            && dev.battery_cycles == 0
            && dev.battery_level_pct >= 99.0
            && dev.battery_adapter == "Direct AC Line Power");

    if is_desktop_no_batt {
        lines.push(Line::from(vec![
            Span::styled(" Power Source : ", amber_bold()),
            Span::styled("[DIRECT MAINS / WALL POWER]", good()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" Topology     : ", dim()),
            Span::styled("Stationary Desktop / Rackmount Platform (No Battery)", amber()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" Power Flow   : ", dim()),
            Span::styled(format!("{:.1}W", dev.battery_watts), amber_bold()),
            Span::styled("  ·  Condition: Direct Line Power", dim()),
        ]));
    } else {
        let is_ac = dev.battery_power_source == "AC Power"
            || dev.battery_status.contains("Charging")
            || dev.battery_status.contains("Charged");
        let pwr_badge = if is_ac { "[AC CONNECTED]" } else { "[BATTERY]" };
        let pwr_style = if is_ac { good() } else { amber() };

        lines.push(Line::from(vec![
            Span::styled("  Charge   : ", amber_bold()),
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
        let cond = if dev.battery_condition.contains("Service") {
            "Service"
        } else {
            dev.battery_condition.as_str()
        };

        lines.push(Line::from(vec![
            Span::styled("  Health   : ", dim()),
            Span::styled(format!("{:.1}% ", dev.battery_health_pct), health_style),
            Span::styled(
                format!("({cond})  ·  Cycles: {}", dev.battery_cycles),
                dim(),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled("  Power    : ", dim()),
            Span::styled(format!("{:.1}W", dev.battery_watts), amber_bold()),
            Span::styled(format!("  ·  Status: {}", dev.battery_status), dim()),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_network(f: &mut Frame, dev: &DeviceInfo, area: Rect) {
    let block = titled_block("⇄ NETWORK SILICON & PRIVACY");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    let mac_badge = if dev.net_mac_is_private {
        "[PRIVATE MAC]"
    } else {
        "[HARDWARE MAC]"
    };
    let mac_style = if dev.net_mac_is_private {
        good()
    } else {
        dim()
    };

    lines.push(Line::from(vec![
        Span::styled(" Wi-Fi Silicon: ", dim()),
        Span::styled(dev.wifi_chipset.clone(), amber_bold()),
    ]));

    lines.push(Line::from(vec![
        Span::styled(
            format!(" Primary: {} ", dev.net_primary_iface),
            amber_bold(),
        ),
        Span::styled(format!("· {} ", dev.net_mac_hardware), dim()),
        Span::styled(mac_badge, mac_style),
    ]));

    let awdl_str = if dev.net_awdl_active {
        "awdl0 Active"
    } else {
        "awdl0 Standby"
    };

    lines.push(Line::from(vec![Span::styled(
        format!(
            " Direct Link: {}  ·  RX {:.0} MB / TX {:.0} MB",
            awdl_str, dev.net_rx_mb, dev.net_tx_mb
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

    let max_procs = (inner.height as usize).saturating_sub(1);
    for p in dev.top_processes.iter().take(max_procs) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    #[test]
    fn test_device_view_render_all_core_scales() {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = crate::app::App::new("en0", Arc::new(AtomicBool::new(false)));

        // 1. User's exact setup: 8 CPU cores (4P + 4E) + 10 GPU cores
        let mut rs_8_cpu_10_gpu = app.build_render();
        rs_8_cpu_10_gpu.device.chip_name = "Apple M2".into();
        rs_8_cpu_10_gpu.device.total_cores = 8;
        rs_8_cpu_10_gpu.device.p_cores = 4;
        rs_8_cpu_10_gpu.device.e_cores = 4;
        rs_8_cpu_10_gpu.device.is_hybrid_cpu = true;
        rs_8_cpu_10_gpu.device.cpu_cores_pct = vec![12.0, 18.5, 9.1, 14.2, 3.4, 2.1, 4.0, 1.8];
        rs_8_cpu_10_gpu.device.gpu_name = "Apple M2 GPU".into();
        rs_8_cpu_10_gpu.device.gpu_cores = 10;
        rs_8_cpu_10_gpu.device.gpu_load_pct = 15.0;

        terminal.draw(|f| {
            render(f, &rs_8_cpu_10_gpu, f.area());
        }).unwrap();

        // 2. 10 CPU cores (6P + 4E) + 16 GPU cores (M2 Pro)
        let mut rs_10_cpu = app.build_render();
        rs_10_cpu.device.chip_name = "Apple M2 Pro".into();
        rs_10_cpu.device.total_cores = 10;
        rs_10_cpu.device.p_cores = 6;
        rs_10_cpu.device.e_cores = 4;
        rs_10_cpu.device.is_hybrid_cpu = true;
        rs_10_cpu.device.cpu_cores_pct = vec![10.0; 10];
        rs_10_cpu.device.gpu_name = "Apple M2 Pro GPU".into();
        rs_10_cpu.device.gpu_cores = 16;

        terminal.draw(|f| {
            render(f, &rs_10_cpu, f.area());
        }).unwrap();

        // 3. 24 CPU cores symmetric (e.g. AMD Ryzen Threadripper / Linux Server)
        let mut rs_24_cpu = app.build_render();
        rs_24_cpu.device.chip_name = "AMD Ryzen 9 7900X".into();
        rs_24_cpu.device.total_cores = 24;
        rs_24_cpu.device.p_cores = 24;
        rs_24_cpu.device.e_cores = 0;
        rs_24_cpu.device.is_hybrid_cpu = false;
        rs_24_cpu.device.cpu_cores_pct = vec![25.0; 24];

        terminal.draw(|f| {
            render(f, &rs_24_cpu, f.area());
        }).unwrap();

        // 4. Test narrow terminal width (68x24) to ensure dynamic packing
        let narrow_backend = TestBackend::new(68, 24);
        let mut narrow_terminal = Terminal::new(narrow_backend).unwrap();
        narrow_terminal.draw(|f| {
            render(f, &rs_8_cpu_10_gpu, f.area());
        }).unwrap();
    }
}
