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

mod theme;
mod views;
pub mod widgets;

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::Tab;
use crate::render::RenderState;
use theme::{alert, amber, amber_bold, dim, good, invert, score_color};

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const RAIL_W: u16 = 16;

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
    }
    draw_context_footer(f, rs, right[2], spin);

    if rs.palette_active {
        draw_palette(f, rs, area);
    }
}

/// Centered command-palette overlay (VS-Code style): a filter line + the list
/// of matching actions with descriptions.
fn draw_palette(f: &mut Frame, rs: &RenderState, area: Rect) {
    use ratatui::widgets::Clear;
    let w = area.width.saturating_sub(8).clamp(40, 70);
    let h = area.height.saturating_sub(6).clamp(8, 18);
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

    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(2),
        Constraint::Length(1),
    ])
    .split(inner);

    // Filter line.
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  filter: ", dim()),
            Span::styled(format!("{}▏", rs.palette_filter), amber_bold()),
        ])),
        rows[0],
    );

    // Matching items.
    let visible = rows[1].height as usize;
    let start = rs.palette_cursor.saturating_sub(visible.saturating_sub(1));
    let mut lines = Vec::new();
    for (i, (name, desc)) in rs
        .palette_items
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
    {
        let selected = i == rs.palette_cursor;
        let ptr = if selected { "▸ " } else { "  " };
        let name_style = if selected { invert() } else { amber_bold() };
        lines.push(Line::from(vec![
            Span::styled(ptr, amber_bold()),
            Span::styled(format!("{name:<26}"), name_style),
            Span::styled(format!(" {desc}"), dim()),
        ]));
    }
    if rs.palette_items.is_empty() {
        lines.push(Line::from(Span::styled("  (no matching action)", dim())));
    }
    f.render_widget(Paragraph::new(lines), rows[1]);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " ↑↓ move   Enter run   Esc close ",
            invert(),
        ))),
        rows[2],
    );
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
    }
}

fn draw_nav_rail(f: &mut Frame, rs: &RenderState, area: Rect) {
    let block = Block::default().borders(Borders::RIGHT).border_style(dim());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = vec![
        Line::from(Span::styled(" WIFI", amber_bold())),
        Line::from(Span::styled(" RUST", amber_bold())),
        Line::from(""),
    ];
    for t in Tab::all() {
        let (icon, label, num) = t.nav();
        let (glyph, gstyle) = tab_status(rs, t);
        let active = t == rs.active_tab;
        if active {
            // Highlight the whole row.
            lines.push(Line::from(vec![
                Span::styled(format!("{icon} "), invert()),
                Span::styled(format!("{label} "), invert()),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!("{num} "), dim()),
                Span::styled(format!("{icon} "), amber()),
                Span::styled(label, amber()),
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
        Tab::Dashboard => " [Enter] action palette   [R] rescan   [L] zero-lag   [1-5] pages ",
        Tab::Spectrum => " [R] rescan airspace   [S] sweet-spot locator   [1-5] pages ",
        Tab::Processes => " [↑↓] select   [Space] pause/resume   [B] throttle   [N] LAN sweep ",
        Tab::Tuning => " [↑↓] select   [Space] toggle / run   [P] cycle preset   [A] auto-pilot ",
        Tab::SpeedTest => " [Space] full test   [V] quick 5s   [N] Apple RPM ",
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
