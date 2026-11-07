//! Amber CRT terminal UI built on ratatui. Renders purely from a `RenderState`
//! snapshot — the UI thread never touches an engine, so it is always responsive.
//!
//! Layout (the v3 "grand cockpit"):
//!   ┌──────────┬───────────────────────────────────────────┐
//!   │ NAV RAIL │ HEADER (ssid · band · rate · health)       │
//!   │          ├───────────────────────────────────────────┤
//!   │ ◉ OVER   │ BODY (the active page)                     │
//!   │ ◈ AIR    │                                            │
//!   │ ...      ├───────────────────────────────────────────┤
//!   │          │ CONTEXT FOOTER (status + per-page keys)     │
//!   └──────────┴───────────────────────────────────────────┘

pub mod theme;
mod views;
pub mod widgets;

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::Tab;
use crate::render::RenderState;
use theme::{alert, amber, amber_bold, dim, good, invert, score_color};

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const RAIL_W: u16 = 20;

pub fn draw(f: &mut Frame, rs: &RenderState, spin: usize) {
    let area = f.area();
    if area.height < 14 || area.width < 64 {
        let msg = Paragraph::new(format!(
            "Terminal too small ({}x{}). Resize to at least 64x14.",
            area.width, area.height
        ))
        .style(ratatui::style::Style::new().fg(ratatui::style::Color::Red));
        f.render_widget(msg, area);
        return;
    }

    // Split: nav rail | right column.
    let cols = Layout::horizontal([Constraint::Length(RAIL_W), Constraint::Min(40)]).split(area);
    draw_nav_rail(f, rs, cols[0]);

    let right = Layout::vertical([
        Constraint::Length(3), // header
        Constraint::Min(6),    // body
        Constraint::Length(2), // context footer
    ])
    .split(cols[1]);

    draw_header(f, rs, right[0]);
    match rs.active_tab {
        Tab::Dashboard => views::dashboard::render(f, rs, right[1]),
        Tab::Spectrum => views::spectrum::render(f, rs, right[1]),
        Tab::Processes => views::processes::render(f, rs, right[1]),
        Tab::Tuning => views::optimizer::render(f, rs, right[1]),
        Tab::SpeedTest => views::speedtest::render(f, rs, right[1]),
        Tab::Device => views::device::render(f, rs, right[1]),
        Tab::Maintenance => views::maintenance::render(f, rs, right[1]),
        Tab::LanRadar => views::lan_radar::render(f, rs, right[1]),
        Tab::GamingRadar => views::gaming_radar::render(f, rs, right[1]),
        Tab::PrivacyFortress => views::privacy_fortress::render(f, rs, right[1]),
    }
    draw_context_footer(f, rs, right[2], spin);

    if rs.palette_active {
        draw_palette(f, rs, area);
    }
}

/// Word-wrapper for Action Inspector descriptions without external crates.
fn wrap_palette_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current_line = String::new();
    for word in text.split_whitespace() {
        if current_line.is_empty() {
            current_line.push_str(word);
        } else if current_line.chars().count() + 1 + word.chars().count() <= max_width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    lines
}

/// Centered Spotlight Inspector Action Palette overlay:
/// - Top: Spotlight search input with live match counter
/// - Left pane: Curated action list with category badges & organic cursor
/// - Right pane: Live Action Inspector & Impact Card (Details, impact, safety, hotkey)
/// - Bottom: Pill keybinding footer ribbon
fn draw_palette(f: &mut Frame, rs: &RenderState, area: Rect) {
    use ratatui::widgets::Clear;

    let max_w = area.width.saturating_sub(2);
    let w = area.width.saturating_sub(6).clamp(68, 96).min(max_w);
    let max_h = area.height.saturating_sub(2);
    let h = area.height.saturating_sub(4).clamp(16, 22).min(max_h);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let popup = Rect {
        x,
        y,
        width: w,
        height: h,
    };

    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(amber_bold())
        .title(Span::styled(" ACTION PALETTE ", invert()));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let is_wide = inner.width >= 56 && inner.height >= 10;

    if is_wide {
        // Two-pane Spotlight Inspector layout
        let rows = Layout::vertical([
            Constraint::Length(2), // search bar (1) + divider (1)
            Constraint::Min(6),    // two-pane body
            Constraint::Length(1), // bottom divider (1)
            Constraint::Length(1), // footer ribbon (1)
        ])
        .split(inner);

        let left_width = (rows[1].width * 46 / 100).max(28);
        let cols = Layout::horizontal([
            Constraint::Length(left_width),
            Constraint::Length(1), // vertical divider │
            Constraint::Min(24),   // right inspector card
        ])
        .split(rows[1]);

        // 1. Search Bar & Match Counter (rows[0])
        let count_str = format!("[ {} / {} Actions ]", rs.palette_items.len(), rs.palette_total_count);
        let prompt_prefix = " › ";
        let avail_w = rows[0].width as usize;

        let mut search_spans = vec![
            Span::styled(prompt_prefix, amber_bold()),
        ];
        let query_display = if rs.palette_filter.is_empty() {
            "Type to search actions (e.g. 'doctor', 'game', 'ram', 'speed')..."
        } else {
            &rs.palette_filter
        };
        let query_style = if rs.palette_filter.is_empty() { dim() } else { amber_bold() };
        search_spans.push(Span::styled(query_display, query_style));
        if !rs.palette_filter.is_empty() {
            search_spans.push(Span::styled("▏", amber_bold()));
        }

        let left_len = prompt_prefix.chars().count() + query_display.chars().count() + if rs.palette_filter.is_empty() { 0 } else { 1 };
        let count_len = count_str.chars().count() + 1;
        if avail_w > left_len + count_len + 2 {
            let pad = avail_w - left_len - count_len;
            search_spans.push(Span::raw(" ".repeat(pad)));
            search_spans.push(Span::styled(count_str, dim()));
        }

        let left_sep_len = cols[0].width as usize;
        let right_sep_len = cols[2].width as usize;
        let top_sep_line = Line::from(vec![
            Span::styled("─".repeat(left_sep_len), dim()),
            Span::styled("┬", dim()),
            Span::styled("─".repeat(right_sep_len), dim()),
        ]);
        f.render_widget(Paragraph::new(vec![Line::from(search_spans), top_sep_line]), rows[0]);

        // 2. Vertical Divider (cols[1])
        let v_div_lines: Vec<Line> = (0..cols[1].height)
            .map(|_| Line::from(Span::styled("│", dim())))
            .collect();
        f.render_widget(Paragraph::new(v_div_lines), cols[1]);

        // 3. Left Pane: Actions List (cols[0])
        let visible = cols[0].height as usize;
        let start = rs.palette_cursor.saturating_sub(visible.saturating_sub(1));
        let mut list_lines = Vec::new();

        for (i, item) in rs.palette_items.iter().enumerate().skip(start).take(visible) {
            let selected = i == rs.palette_cursor;
            let tag = match item.category.as_str() {
                "NETWORK" => "NET",
                "GAMING" => "GAME",
                "OPTIMIZE" => "BOOST",
                "SYSTEM" => "SYS",
                "PRIVACY" => "PRIV",
                "PRESET" => "MODE",
                "DIAGNOSTICS" => "DIAG",
                "BENCHMARK" => "BENCH",
                "INTERFACE" => "THEME",
                "STORAGE" => "DISK",
                other => if other.len() > 4 { &other[..4] } else { other },
            };

            let (ptr, ptr_style) = if selected {
                ("▶ ", amber_bold())
            } else {
                ("  ", dim())
            };

            let tag_str = format!("[{:<4}] ", tag);
            let tag_style = if selected {
                amber_bold()
            } else {
                dim()
            };

            let name_style = if selected {
                amber_bold()
            } else {
                Style::default().fg(Color::White)
            };

            let max_name_len = (cols[0].width as usize).saturating_sub(ptr.len() + tag_str.len());
            let truncated_name = if item.name.chars().count() > max_name_len {
                let take_len = max_name_len.saturating_sub(1);
                format!("{}…", &item.name.chars().take(take_len).collect::<String>())
            } else {
                item.name.clone()
            };

            list_lines.push(Line::from(vec![
                Span::styled(ptr, ptr_style),
                Span::styled(tag_str, tag_style),
                Span::styled(truncated_name, name_style),
            ]));
        }

        if rs.palette_items.is_empty() {
            list_lines.push(Line::from(vec![
                Span::styled("  ", dim()),
                Span::styled("(no matching actions found)", dim()),
            ]));
        }
        f.render_widget(Paragraph::new(list_lines), cols[0]);

        // 4. Right Pane: Action Inspector Card (cols[2])
        let mut inspector_lines = Vec::new();
        if let Some(item) = rs.palette_items.get(rs.palette_cursor) {
            // Title
            inspector_lines.push(Line::from(vec![
                Span::styled(" ", dim()),
                Span::styled(item.name.to_uppercase(), amber_bold()),
            ]));

            // Category badge
            inspector_lines.push(Line::from(vec![
                Span::styled(" Category: ", dim()),
                Span::styled(&item.category, amber_bold()),
            ]));

            // Direct Hotkey
            let hk_display = item.hotkey.as_deref().unwrap_or("Palette only (no direct binding)");
            let hk_style = if item.hotkey.is_some() { good() } else { dim() };
            inspector_lines.push(Line::from(vec![
                Span::styled(" Hotkey:   ", dim()),
                Span::styled(hk_display, hk_style),
            ]));

            // Safety posture
            let safety_style = if item.safety.contains("Root") {
                alert()
            } else {
                good()
            };
            inspector_lines.push(Line::from(vec![
                Span::styled(" Safety:   ", dim()),
                Span::styled(&item.safety, safety_style),
            ]));

            // Real-world Impact
            inspector_lines.push(Line::from(vec![
                Span::styled(" Impact:   ", dim()),
                Span::styled(&item.impact, amber_bold()),
            ]));

            // Subtle divider
            let div_w = cols[2].width.saturating_sub(2) as usize;
            inspector_lines.push(Line::from(Span::styled(format!(" {}", "─".repeat(div_w)), dim())));

            // One-line summary
            inspector_lines.push(Line::from(vec![
                Span::styled(" ", dim()),
                Span::styled(&item.desc, dim()),
            ]));
            inspector_lines.push(Line::from(""));

            // Plain-English detailed explanation
            let wrap_w = (cols[2].width.saturating_sub(4) as usize).max(10);
            for line in wrap_palette_text(&item.details, wrap_w) {
                inspector_lines.push(Line::from(vec![
                    Span::styled(" ", dim()),
                    Span::styled(line, Style::default().fg(Color::White)),
                ]));
            }

            // Execution action prompt
            inspector_lines.push(Line::from(""));
            inspector_lines.push(Line::from(vec![
                Span::styled(" › Press ", dim()),
                Span::styled("[Enter]", amber_bold()),
                Span::styled(" to execute action now", dim()),
            ]));
        } else {
            inspector_lines.push(Line::from(""));
            inspector_lines.push(Line::from(Span::styled(" (No action selected)", dim())));
            inspector_lines.push(Line::from(""));
            inspector_lines.push(Line::from(Span::styled(" Filter actions by typing keywords:", dim())));
            inspector_lines.push(Line::from(Span::styled("   • doctor, boost, zerolag, wmm", dim())));
            inspector_lines.push(Line::from(Span::styled("   • speed, benchmark, roam, captive", dim())));
            inspector_lines.push(Line::from(Span::styled("   • ram, snapshot, theme, root", dim())));
        }
        f.render_widget(Paragraph::new(inspector_lines), cols[2]);

        // 5. Bottom Junction Divider (rows[2])
        let bot_sep_line = Line::from(vec![
            Span::styled("─".repeat(left_sep_len), dim()),
            Span::styled("┴", dim()),
            Span::styled("─".repeat(right_sep_len), dim()),
        ]);
        f.render_widget(Paragraph::new(bot_sep_line), rows[2]);

        // 6. Footer Ribbon (rows[3])
        let footer_ribbon = Line::from(vec![
            Span::styled(" ", dim()),
            Span::styled("[↑/↓]", amber_bold()),
            Span::styled(" Navigate   ", dim()),
            Span::styled("[Enter]", amber_bold()),
            Span::styled(" Execute Action   ", dim()),
            Span::styled("[Esc]", amber_bold()),
            Span::styled(" Close", dim()),
        ]);
        f.render_widget(Paragraph::new(footer_ribbon), rows[3]);
    } else {
        // Fallback for compact/narrow terminals (< 56 cols)
        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(2),
            Constraint::Length(1),
        ])
        .split(inner);

        // Filter line
        let filter_display = if rs.palette_filter.is_empty() {
            Span::styled("Type to search...", dim())
        } else {
            Span::styled(format!("{}▏", rs.palette_filter), amber_bold())
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" › ", amber_bold()),
                filter_display,
            ])),
            rows[0],
        );

        // Compact list
        let visible = rows[1].height as usize;
        let start = rs.palette_cursor.saturating_sub(visible.saturating_sub(1));
        let mut lines = Vec::new();
        for (i, item) in rs.palette_items.iter().enumerate().skip(start).take(visible) {
            let selected = i == rs.palette_cursor;
            let (ptr, ptr_style) = if selected { ("▶ ", amber_bold()) } else { ("  ", dim()) };
            let name_style = if selected { amber_bold() } else { Style::default().fg(Color::White) };
            lines.push(Line::from(vec![
                Span::styled(ptr, ptr_style),
                Span::styled(format!("{:<22} ", item.name), name_style),
                Span::styled(&item.desc, dim()),
            ]));
        }
        if rs.palette_items.is_empty() {
            lines.push(Line::from(Span::styled("  (no matching actions)", dim())));
        }
        f.render_widget(Paragraph::new(lines), rows[1]);

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" [↑/↓] Navigate   [Enter] Run   [Esc] Close", dim()),
            ])),
            rows[2],
        );
    }
}

/// Per-page health glyph so a problem shows in the rail without visiting it.
fn tab_status(rs: &RenderState, tab: Tab) -> (&'static str, ratatui::style::Style) {
    let ok = ("●", good());
    let warn = ("!", alert());
    match tab {
        Tab::Dashboard => {
            if rs.telemetry.health_score >= 72 {
                ok
            } else {
                warn
            }
        }
        Tab::Spectrum => {
            if rs.spectrum.collisions.is_empty() {
                ok
            } else {
                warn
            }
        }
        Tab::Processes => {
            if rs.processes.iter().any(|p| p.is_hog) {
                warn
            } else {
                ok
            }
        }
        Tab::Tuning => {
            if rs.wan_snap.ecs_dns_optimal {
                ok
            } else {
                warn
            }
        }
        Tab::SpeedTest => ok,
        Tab::Device => {
            if rs.device.thermal_state != "CRITICAL" && rs.device.mem_pressure_label != "CRITICAL" {
                ok
            } else {
                warn
            }
        }
        Tab::Maintenance => {
            if rs.maintenance.ssd_reclaimable_gb > 25.0 {
                warn
            } else {
                ok
            }
        }
        Tab::LanRadar => {
            if rs.lan_data.ip_conflict {
                warn
            } else {
                ok
            }
        }
        Tab::GamingRadar => {
            if rs.gaming_data.avg_jitter > 5.0 {
                warn
            } else {
                ok
            }
        }
        Tab::PrivacyFortress => {
            if rs.privacy_data.telemetry_calls.len() > 10 {
                warn
            } else {
                ok
            }
        }
    }
}

fn draw_nav_rail(f: &mut Frame, rs: &RenderState, area: Rect) {
    let block = Block::default().borders(Borders::RIGHT).border_style(dim());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = vec![
        Line::from(Span::styled(" AETHER", amber_bold())),
        Line::from(Span::styled(" COCKPIT", dim())),
        Line::from(""),
    ];
    for t in Tab::all() {
        let (icon, label, num) = t.nav();
        let (glyph, gstyle) = tab_status(rs, t);
        let active = t == rs.active_tab;
        if active {
            // Highlight the whole row with consistent width.
            lines.push(Line::from(vec![
                Span::styled(format!("{num} {icon} {:<13}", label), invert()),
                Span::styled(format!(" {glyph}"), invert()),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!("{num} "), dim()),
                Span::styled(format!("{icon} "), amber()),
                Span::styled(format!("{:<13}", label), amber()),
                Span::raw(" "),
                Span::styled(glyph, gstyle),
            ]));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(" p palette", dim())));
    lines.push(Line::from(Span::styled(" q quit", dim())));
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_header(f: &mut Frame, rs: &RenderState, area: Rect) {
    let tel = &rs.telemetry;
    let block = Block::default().borders(Borders::ALL).border_style(amber());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let ssid = if tel.ssid_available {
        tel.ssid.clone()
    } else {
        format!("{} (inferred)", tel.ssid)
    };
    let (_icon, page, _n) = rs.active_tab.nav();
    let left = Line::from(vec![
        Span::styled(format!(" {page} "), invert()),
        Span::raw("  "),
        Span::styled(
            format!(
                "● {ssid}  ·  {}  ·  {} Mbps",
                tel.band.label(),
                tel.tx_rate_mbps
            ),
            amber(),
        ),
    ]);
    let right = Line::from(vec![
        Span::styled(
            format!("HEALTH {}/100 ", tel.health_score),
            score_color(tel.health_score),
        ),
        Span::styled(
            format!("[{}] ", tel.health_rating.label()),
            score_color(tel.health_score),
        ),
    ]);

    let chunks = Layout::horizontal([Constraint::Min(20), Constraint::Length(26)]).split(inner);
    // When Game Mode is active, show a controller badge in the left region.
    if rs.game_mode_active {
        let left = Line::from(vec![
            Span::styled(format!(" {page} "), invert()),
            Span::raw("  "),
            Span::styled(" ▌GAME MODE▐ ", good()),
            Span::styled(
                format!(" {}  ·  {} Mbps", tel.band.label(), tel.tx_rate_mbps),
                amber(),
            ),
        ]);
        f.render_widget(Paragraph::new(left), chunks[0]);
    } else {
        f.render_widget(Paragraph::new(left), chunks[0]);
    }
    f.render_widget(Paragraph::new(right).alignment(Alignment::Right), chunks[1]);
}

fn draw_context_footer(f: &mut Frame, rs: &RenderState, area: Rect, spin: usize) {
    let keys = match rs.active_tab {
        Tab::Dashboard => " [Enter] action palette   [R] rescan   [L] zero-lag   [1-0] pages ",
        Tab::Spectrum => " [R] rescan airspace   [S] sweet-spot locator   [1-0] pages ",
        Tab::Processes => " [↑↓] select   [Space] pause/resume   [B] throttle   [N] LAN sweep ",
        Tab::Tuning => " [↑↓] select   [Space] toggle / run   [P] cycle preset   [A] auto-pilot ",
        Tab::SpeedTest => " [Space] full test   [V] quick 5s   [N] Apple RPM ",
        Tab::Device => " [C] toggle mascot   [R] refresh hardware   [1-0] pages   [Q] quit ",
        Tab::Maintenance => " [R] RAM  [S] SSD  [A] All  [Space] Toggle  [U] Uninstall  [←/→] Panels  [PgUp/Dn] Scroll ",
        Tab::LanRadar => " [↑↓] Select host   [P] Port scan   [R] Rescan subnet   [Space] Trust   [1-0] pages ",
        Tab::GamingRadar => " [↑↓] Select region   [G] Filter game   [Space] Game Shield   [R] Rescan ping   [1-0] pages ",
        Tab::PrivacyFortress => " [↑↓] Select row   [Enter] Apply DNS   [D] Reset DHCP   [R] Race all   [Tab/←→] Pane   [1-0] pages ",
    };
    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);

    let (prefix, style) = if rs.busy {
        (format!(" {} ", SPINNER[spin % SPINNER.len()]), good())
    } else {
        (" › ".to_string(), amber_bold())
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("{prefix}{}", rs.status_msg),
            style,
        ))),
        chunks[0],
    );
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(keys, invert()))),
        chunks[1],
    );
}

/// A bordered amber block with a title, used by all views.
pub fn titled_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(amber())
        .title(Span::styled(format!(" {title} "), amber_bold()))
}

/// One KPI scorecard card: a titled bordered tile with a big value line and a
/// trend sparkline line underneath (BI-dashboard style).
pub struct Kpi<'a> {
    pub title: &'a str,
    pub value: String,
    pub unit: &'a str,
    pub history: &'a [f64],
    pub floor: f64,
    /// Colour accent for the value (e.g. graded by score). None = amber_bold.
    pub accent: Option<ratatui::style::Style>,
}

/// Draw the KPI scorecard strip: `cards.len()` equal-width tiles across `area`.
/// Shared chrome on every page, so the thin header imbalance is gone.
pub fn draw_kpi_strip(f: &mut Frame, area: Rect, cards: &[Kpi]) {
    if cards.is_empty() {
        return;
    }
    let constraints: Vec<Constraint> = cards
        .iter()
        .map(|_| Constraint::Ratio(1, cards.len() as u32))
        .collect();
    let cols = Layout::horizontal(constraints).split(area);
    for (i, card) in cards.iter().enumerate() {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(dim())
            .title(Span::styled(format!(" {} ", card.title), dim()));
        let inner = block.inner(cols[i]);
        f.render_widget(block, cols[i]);
        let accent = card.accent.unwrap_or_else(amber_bold);
        let body = widgets::kpi_tile(
            &card.value,
            card.unit,
            card.history,
            card.floor,
            inner.width as usize,
        );
        let mut lines = Vec::new();
        for (j, l) in body.iter().enumerate() {
            let style = if j == 0 { accent } else { amber() };
            lines.push(Line::from(Span::styled(l.clone(), style)));
        }
        f.render_widget(Paragraph::new(lines), inner);
    }
}

/// Split an area into a `cols` x `rows` grid of equal tiles, row-major.
pub fn grid(area: Rect, cols: usize, rows: usize) -> Vec<Rect> {
    let row_c: Vec<Constraint> = (0..rows)
        .map(|_| Constraint::Ratio(1, rows as u32))
        .collect();
    let row_areas = Layout::vertical(row_c).split(area);
    let mut out = Vec::with_capacity(cols * rows);
    for ra in row_areas.iter() {
        let col_c: Vec<Constraint> = (0..cols)
            .map(|_| Constraint::Ratio(1, cols as u32))
            .collect();
        let cas = Layout::horizontal(col_c).split(*ra);
        for ca in cas.iter() {
            out.push(*ca);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use crate::app::{Action, App};

    #[test]
    fn test_wrap_palette_text_properties() {
        let text = "Runs an 8-stage diagnostic and automated repair sequence for DNS and MTU.";
        let wrapped = wrap_palette_text(text, 25);
        assert!(!wrapped.is_empty());
        for line in &wrapped {
            assert!(line.chars().count() <= 25, "Line '{}' exceeded 25 chars", line);
        }
        // Wrapping empty or 0 width
        assert_eq!(wrap_palette_text("", 20), Vec::<String>::new());
        assert_eq!(wrap_palette_text("hello world", 0), vec!["hello world".to_string()]);
    }

    #[test]
    fn test_palette_render_wide_spotlight_inspector() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new("en0", Arc::new(AtomicBool::new(false)));
        app.apply_action(Action::PaletteOpen);

        let rs = app.build_render();
        assert!(rs.palette_active);
        assert_eq!(rs.palette_total_count, 27);
        assert_eq!(rs.palette_items.len(), 27);

        terminal
            .draw(|f| {
                draw(f, &rs, 0);
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

        // Verify spotlight search header, live match count, and category tags
        assert!(text.contains("ACTION PALETTE"));
        assert!(text.contains("27 Actions"));
        assert!(text.contains("[NET ]") || text.contains("[GAME]") || text.contains("[BOOST"));

        // Verify Inspector card elements
        assert!(text.contains("Category:"));
        assert!(text.contains("Hotkey:"));
        assert!(text.contains("Safety:"));
        assert!(text.contains("Impact:"));
        assert!(text.contains("execute action now"));

        // Verify footer ribbon
        assert!(text.contains("Navigate"));
        assert!(text.contains("Execute Action"));
    }

    #[test]
    fn test_palette_render_compact_narrow() {
        let backend = TestBackend::new(54, 18);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new("en0", Arc::new(AtomicBool::new(false)));
        app.apply_action(Action::PaletteOpen);

        let rs = app.build_render();
        assert!(rs.palette_active);

        // Must render cleanly without panic on narrow terminals
        terminal
            .draw(|f| {
                draw_palette(f, &rs, f.area());
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

        assert!(text.contains("ACTION PALETTE"));
        assert!(text.contains("Run"));
    }

    #[test]
    fn test_palette_filtering_and_inspector_sync() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new("en0", Arc::new(AtomicBool::new(false)));
        app.apply_action(Action::PaletteOpen);

        // Filter by "doctor"
        for c in "doctor".chars() {
            app.apply_action(Action::PaletteChar(c));
        }

        let rs_doc = app.build_render();
        assert_eq!(rs_doc.palette_items.len(), 1);
        assert_eq!(rs_doc.palette_items[0].name, "WAN & YouTube Doctor");

        terminal
            .draw(|f| {
                draw(f, &rs_doc, 0);
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

        assert!(text.contains("WAN & YOUTUBE DOCTOR"));
        assert!(text.contains("[ 1 / 27 Actions ]"));
        assert!(text.contains("DNS latency -15ms"));

        // Filter with no matches
        for c in "xyz999".chars() {
            app.apply_action(Action::PaletteChar(c));
        }
        let rs_empty = app.build_render();
        assert_eq!(rs_empty.palette_items.len(), 0);

        terminal
            .draw(|f| {
                draw(f, &rs_empty, 0);
            })
            .unwrap();

        let buf_empty = terminal.backend().buffer();
        let mut text_empty = String::new();
        for y in 0..buf_empty.area.height {
            for x in 0..buf_empty.area.width {
                text_empty.push_str(buf_empty[(x, y)].symbol());
            }
            text_empty.push('\n');
        }

        assert!(text_empty.contains("no matching actions found"));
        assert!(text_empty.contains("No action selected"));
    }
}
