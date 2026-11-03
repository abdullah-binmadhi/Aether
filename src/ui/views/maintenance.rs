//! Tab [7] Zero-Touch Device Maintenance & App Uninstaller.
//!
//! Provides non-destructive RAM & SSD cleanups with strict session protections:
//! - Never touches Zen Browser (or Safari/Chrome) tabs, cookies, or sessions
//! - Never touches CLI tokens, dotfiles, or credentials (~/.config, ~/.aws, ~/.ssh, Keychain)
//! - Independent execution: RAM-only [R], SSD-only [S], or All-in-One [A]
//! - 3 safety tiers: Zero-Touch Safe (Default), Balanced Developer, Deep App Uninstaller
//! - Windowed scrolling for app inventory and expanded live console

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::core::maintenance::MaintenanceSnapshot;
use crate::render::RenderState;
use crate::ui::theme::{alert, amber, amber_bold, bar, dim, good, invert, theme_name};
use crate::ui::{draw_kpi_strip, titled_block, Kpi};

pub fn render(f: &mut Frame, rs: &RenderState, area: Rect) {
    let snap = &rs.maintenance;

    let rows = Layout::vertical([
        Constraint::Length(4), // Top KPI scorecard strip
        Constraint::Length(4), // Mode banner & companion mascot
        Constraint::Min(14),   // 2-column main maintenance grid (generous height)
        Constraint::Length(9), // Live log console (expanded space)
    ])
    .split(area);

    draw_kpis(f, snap, rows[0]);
    draw_banner(f, snap, rows[1]);

    // Split main area into 2 columns: left (charts & checklist) and right (app uninstaller)
    let cols = Layout::horizontal([
        Constraint::Percentage(48),
        Constraint::Percentage(52),
    ])
    .split(rows[2]);

    draw_charts_and_checklist(f, snap, cols[0]);
    draw_app_uninstaller(f, snap, cols[1]);
    draw_live_log(f, snap, rows[3]);
}

fn draw_kpis(f: &mut Frame, snap: &MaintenanceSnapshot, area: Rect) {
    let mode_label = match snap.active_kind {
        1 => "RAM ONLY",
        2 => "SSD ONLY",
        _ => "ALL-IN-ONE",
    };
    let tier_label = match snap.active_safety_level {
        2 => "L2: BALANCED",
        3 => "L3: DEEP CLEAN",
        _ => "L1: ZERO-TOUCH",
    };

    let cards = [
        Kpi {
            title: "RECLAIMABLE SSD",
            value: format!("{:.1} GB", snap.ssd_reclaimable_gb),
            unit: "ELIGIBLE",
            history: &[],
            floor: 0.0,
            accent: Some(good()),
        },
        Kpi {
            title: "RECLAIMABLE RAM",
            value: format!("{:.1} GB", snap.ram_reclaimable_gb),
            unit: "INACTIVE CACHE",
            history: &[],
            floor: 0.0,
            accent: Some(amber_bold()),
        },
        Kpi {
            title: "SAFETY TIER [L]",
            value: tier_label.to_string(),
            unit: "SAFE MODE",
            history: &[],
            floor: 0.0,
            accent: Some(good()),
        },
        Kpi {
            title: "OPTIMIZE MODE",
            value: mode_label.to_string(),
            unit: "[R/S/A]",
            history: &[],
            floor: 0.0,
            accent: Some(amber_bold()),
        },
    ];

    draw_kpi_strip(f, area, &cards);
}

fn draw_banner(f: &mut Frame, snap: &MaintenanceSnapshot, area: Rect) {
    let block = titled_block("ZERO-TOUCH MAINTENANCE COCKPIT");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    let cur_theme = theme_name();
    let tier_str = match snap.active_safety_level {
        2 => "Level 2 (Balanced Developer)",
        3 => "Level 3 (Deep App Uninstaller)",
        _ => "Level 1 (Zero-Touch Safe)",
    };

    if snap.mascot_visible {
        lines.push(Line::from(vec![
            Span::styled("  /\\_/\\   ", amber_bold()),
            Span::styled("SAFETY GUARANTEE: ", good()),
            Span::styled("Zen Browser tabs, sessions & CLI credentials locked & shielded!  ", dim()),
            Span::styled(format!("THEME: [{cur_theme}]"), amber_bold()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" ( o.o )  ", amber_bold()),
            Span::styled("[R] ", amber_bold()),
            Span::styled("RAM Only   ", dim()),
            Span::styled("[S] ", amber_bold()),
            Span::styled("SSD Only   ", dim()),
            Span::styled("[A] ", amber_bold()),
            Span::styled("All-in-One   ", dim()),
            Span::styled("[L/Space] ", good()),
            Span::styled(format!("Tier: {tier_str}   "), amber_bold()),
            Span::styled("[T] ", amber_bold()),
            Span::styled("Theme", dim()),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled(" ◆ SAFETY GUARANTEE: ", good()),
            Span::styled("Zen Browser tabs, sessions & CLI credentials locked & shielded!  ", dim()),
            Span::styled(format!("THEME: [{cur_theme}]"), amber_bold()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(" ◆ CONTROLS: ", amber_bold()),
            Span::styled("[R] RAM Only  [S] SSD Only  [A] All  ", dim()),
            Span::styled("[L/Space] ", good()),
            Span::styled(format!("Tier: {tier_str}  "), amber_bold()),
            Span::styled("[T] Theme  [Tab] Switch Panel  [U] Uninstall App", dim()),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_charts_and_checklist(f: &mut Frame, snap: &MaintenanceSnapshot, area: Rect) {
    let is_focused = snap.active_focus == 0;
    let title = if is_focused {
        "◉ MEMORY & STORAGE BREAKDOWN CHARTS  ▶ [ACTIVE FOCUS]"
    } else {
        "◉ MEMORY & STORAGE BREAKDOWN CHARTS"
    };
    let block = titled_block(title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    let gauge_w = (inner.width as usize).saturating_sub(26).clamp(8, 20);

    // RAM breakdown chart
    lines.push(Line::from(Span::styled(" UNIFIED MEMORY HEADROOM & CACHE:", amber_bold())));
    lines.push(Line::from(vec![
        Span::styled("  Active Apps ", dim()),
        Span::styled(bar(snap.ram_active_gb, 0.0, 16.0, gauge_w), amber()),
        Span::styled(format!(" {:>4.1} GB", snap.ram_active_gb), amber()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Wired Mach  ", dim()),
        Span::styled(bar(snap.ram_wired_gb, 0.0, 16.0, gauge_w), amber()),
        Span::styled(format!(" {:>4.1} GB", snap.ram_wired_gb), amber()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Reclaimable ", dim()),
        Span::styled(bar(snap.ram_reclaimable_gb, 0.0, 16.0, gauge_w), good()),
        Span::styled(format!(" {:>4.1} GB ◄ [PURGEABLE]", snap.ram_reclaimable_gb), good()),
    ]));

    lines.push(Line::from(""));

    // Storage breakdown chart
    lines.push(Line::from(Span::styled(
        format!(" RECLAIMABLE SSD SECTORS ({:.1} GB Total):", snap.ssd_reclaimable_gb),
        amber_bold(),
    )));
    lines.push(Line::from(vec![
        Span::styled("  APFS Snaps  ", dim()),
        Span::styled(bar(snap.ssd_snapshots_gb, 0.0, 30.0, gauge_w), good()),
        Span::styled(format!(" {:>4.1} GB", snap.ssd_snapshots_gb), good()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Xcode/Dev   ", dim()),
        Span::styled(bar(snap.ssd_dev_cache_gb, 0.0, 20.0, gauge_w), amber()),
        Span::styled(format!(" {:>4.1} GB", snap.ssd_dev_cache_gb), amber()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  App Residue ", dim()),
        Span::styled(bar(snap.ssd_residue_gb, 0.0, 10.0, gauge_w), amber()),
        Span::styled(format!(" {:>4.1} GB", snap.ssd_residue_gb), amber()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Crash Logs  ", dim()),
        Span::styled(bar(snap.ssd_logs_gb, 0.0, 5.0, gauge_w), dim()),
        Span::styled(format!(" {:>4.1} GB", snap.ssd_logs_gb), dim()),
    ]));

    lines.push(Line::from(""));

    // Interactive Checklist & Safety Tier Selector
    lines.push(Line::from(Span::styled(" INTERACTIVE CHECKLIST & SAFETY TIER:", amber_bold())));

    // Row 0: Safety Tier
    let tier_label = match snap.active_safety_level {
        2 => "Level 2: Balanced Developer",
        3 => "Level 3: Deep App Uninstaller",
        _ => "Level 1: Zero-Touch Safe",
    };
    let is_tier_sel = is_focused && snap.checklist_cursor == 0;
    let tier_prefix = if is_tier_sel { " ► " } else { "   " };
    let tier_style = if is_tier_sel { invert() } else { good() };
    lines.push(Line::from(vec![
        Span::styled(tier_prefix, amber_bold()),
        Span::styled(format!("[ TIER: {tier_label} ]"), tier_style),
        Span::styled(" ◄ (Space to cycle)", dim()),
    ]));

    // Checkboxes (Rows 1..4)
    let targets = [
        ("Inactive RAM Cache Purge", snap.ram_reclaimable_gb, snap.target_checks[0]),
        ("APFS Local Snapshot Thinning", snap.ssd_snapshots_gb, snap.target_checks[1]),
        ("Xcode DerivedData & Dev Caches", snap.ssd_dev_cache_gb, snap.target_checks[2]),
        ("Stale DiagnosticReports & Logs", snap.ssd_logs_gb, snap.target_checks[3]),
    ];

    for (idx, (label, sz, checked)) in targets.iter().enumerate() {
        let row_idx = idx + 1;
        let is_row_sel = is_focused && snap.checklist_cursor == row_idx;
        let prefix = if is_row_sel { " ► " } else { "   " };
        let box_char = if *checked { "[x]" } else { "[ ]" };
        let box_style = if is_row_sel {
            invert()
        } else if *checked {
            good()
        } else {
            dim()
        };

        lines.push(Line::from(vec![
            Span::styled(prefix, amber_bold()),
            Span::styled(format!("{box_char} {label:<30}"), box_style),
            Span::styled(format!(" (≈ {:.1} GB)", sz), dim()),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_app_uninstaller(f: &mut Frame, snap: &MaintenanceSnapshot, area: Rect) {
    let is_focused = snap.active_focus == 1;
    let n = snap.apps.len();
    let sel = snap.selected_app_idx.min(n.saturating_sub(1));

    let title = if is_focused {
        format!("⊞ DEEP APP UNINSTALLER & RESIDUE HUNTER  ▶ [{}/{}]", if n == 0 { 0 } else { sel + 1 }, n)
    } else {
        format!("⊞ DEEP APP UNINSTALLER & RESIDUE HUNTER  [{}/{}]", if n == 0 { 0 } else { sel + 1 }, n)
    };
    let block = titled_block(&title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();

    // Table Header
    lines.push(Line::from(vec![
        Span::styled("  APP NAME             ", dim()),
        Span::styled("BINARY     ", dim()),
        Span::styled("RESIDUE    ", dim()),
        Span::styled("STATUS", dim()),
    ]));

    // Dynamic windowed scrolling based on inner height
    let detail_lines_count = 6;
    let table_height = (inner.height as usize).saturating_sub(detail_lines_count + 1).max(3);

    let start = if n <= table_height {
        0
    } else if sel < table_height / 2 {
        0
    } else if sel + table_height / 2 >= n {
        n.saturating_sub(table_height)
    } else {
        sel.saturating_sub(table_height / 2)
    };
    let end = (start + table_height).min(n);

    if start > 0 {
        lines.push(Line::from(Span::styled("    ▲ ... more applications above ...", dim())));
    }

    for i in start..end {
        if let Some(app) = snap.apps.get(i) {
            let is_sel = i == sel;
            let prefix = if is_sel { "> " } else { "  " };

            let bin_mb = app.binary_bytes as f64 / (1024.0 * 1024.0);
            let res_mb = (app.total_bytes.saturating_sub(app.binary_bytes)) as f64 / (1024.0 * 1024.0);

            let name_str = if app.name.len() > 19 {
                format!("{}…", &app.name[..18])
            } else {
                app.name.clone()
            };

            let status_str = if app.is_orphan {
                "DEAD RESIDUE ◄"
            } else if app.is_system_protected {
                "System Protected"
            } else {
                "Installed"
            };

            let row_style = if is_sel {
                invert()
            } else if app.is_orphan {
                alert()
            } else {
                amber()
            };

            lines.push(Line::from(vec![
                Span::styled(format!("{prefix}{:<21}", name_str), row_style),
                Span::styled(format!("{:>6.0} MB   ", bin_mb), row_style),
                Span::styled(format!("{:>6.0} MB   ", res_mb), row_style),
                Span::styled(format!("{:<15}", status_str), row_style),
            ]));
        }
    }

    if end < n {
        lines.push(Line::from(Span::styled("    ▼ ... more applications below ...", dim())));
    }

    if snap.apps.is_empty() {
        lines.push(Line::from(Span::styled("  (scanning /Applications for app inventory...)", dim())));
    }

    lines.push(Line::from(""));

    // Detailed breakdown of selected app
    if let Some(selected) = snap.apps.get(sel) {
        lines.push(Line::from(Span::styled(
            format!(" DETAILED FOOTPRINT FOR: {}", selected.name),
            amber_bold(),
        )));
        let tot_mb = selected.total_bytes as f64 / (1024.0 * 1024.0);

        if let Some(ref bin) = selected.binary_path {
            lines.push(Line::from(vec![
                Span::styled("  • Binary: ", dim()),
                Span::styled(format!("{:<38}", bin.display()), amber()),
            ]));
        }
        for (p, sz) in selected.support_paths.iter().take(2) {
            let smb = *sz as f64 / (1024.0 * 1024.0);
            lines.push(Line::from(vec![
                Span::styled("  • Residue: ", dim()),
                Span::styled(format!("{:<30} ({:.1} MB)", p.display(), smb), dim()),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled("  TOTAL FOOTPRINT: ", amber_bold()),
            Span::styled(format!("{:.1} MB   ", tot_mb), good()),
            Span::styled("[U] Complete Removal", alert()),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_live_log(f: &mut Frame, snap: &MaintenanceSnapshot, area: Rect) {
    let block = titled_block("✱ LIVE MAINTENANCE CONSOLE");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let max_lines = inner.height as usize;
    let n = snap.log_messages.len();
    let start = n.saturating_sub(max_lines);

    let mut lines = Vec::new();
    for msg in &snap.log_messages[start..] {
        let style = if msg.contains("[OK]") {
            good()
        } else if msg.contains("[RAM]") || msg.contains("[SSD]") {
            amber_bold()
        } else if msg.contains("[!]") {
            alert()
        } else {
            dim()
        };
        lines.push(Line::from(Span::styled(format!(" › {msg}"), style)));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(" › Console ready. Press [R], [S], or [A] to execute.", dim())));
    }

    f.render_widget(Paragraph::new(lines), inner);
}
