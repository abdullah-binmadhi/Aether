//! Amber CRT terminal UI built on ratatui.

mod theme;
mod views;
pub mod widgets;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, Tab};
use theme::{amber, amber_bold, dim, invert, score_color};

/// Top-level draw: vital bar, active view, command deck.
pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    if area.height < 12 || area.width < 60 {
        let msg = Paragraph::new(format!(
            "Terminal too small ({}x{}). Resize to at least 60x12.",
            area.width, area.height
        ))
        .style(ratatui::style::Style::new().fg(ratatui::style::Color::Red));
        f.render_widget(msg, area);
        return;
    }

    let chunks = Layout::vertical([
        Constraint::Length(2), // vital bar
        Constraint::Min(6),    // body
        Constraint::Length(2), // command deck
    ])
    .split(area);

    draw_vital_bar(f, app, chunks[0]);
    match app.active_tab {
        Tab::Dashboard => views::dashboard::render(f, app, chunks[1]),
        Tab::Spectrum => views::spectrum::render(f, app, chunks[1]),
        Tab::Processes => views::processes::render(f, app, chunks[1]),
        Tab::Tuning => views::optimizer::render(f, app, chunks[1]),
        Tab::SpeedTest => views::speedtest::render(f, app, chunks[1]),
    }
    draw_command_deck(f, app, chunks[2]);
}

fn draw_vital_bar(f: &mut Frame, app: &App, area: Rect) {
    let tel = &app.telemetry;

    let mut spans = vec![Span::styled(" // WIFI COCKPIT v2 // ", invert())];
    spans.push(Span::raw(" "));

    let ssid_disp = if tel.ssid_available {
        tel.ssid.clone()
    } else {
        format!("{} (inferred)", tel.ssid)
    };
    spans.push(Span::styled(
        format!(
            "● {} ({}) │ {} Mbps │ ",
            ssid_disp,
            tel.band.label(),
            tel.tx_rate_mbps
        ),
        amber(),
    ));
    spans.push(Span::styled(
        format!("{}/100 [{}]", tel.health_score, tel.health_rating.label()),
        score_color(tel.health_score),
    ));

    // Right-anchored tabs.
    let mut tab_spans = Vec::new();
    for t in Tab::all() {
        let style = if t == app.active_tab {
            invert()
        } else {
            amber_bold()
        };
        tab_spans.push(Span::styled(format!(" {} ", t.short(area.width)), style));
    }

    let chunks = Layout::horizontal([Constraint::Min(10), Constraint::Length(58)]).split(area);
    f.render_widget(Paragraph::new(Line::from(spans)), chunks[0]);
    f.render_widget(
        Paragraph::new(Line::from(tab_spans)).right_aligned(),
        chunks[1],
    );

    // Separator rule on the second row.
    let rule = "━".repeat(area.width as usize);
    let rule_area = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: 1,
    };
    f.render_widget(Paragraph::new(rule).style(dim()), rule_area);
}

fn draw_command_deck(f: &mut Frame, app: &App, area: Rect) {
    let keys = match app.active_tab {
        Tab::Dashboard => " [M]Visual [O]Hop2 [D]Doctor [E]WiFi-Key [R]Roam [G]Boost [L]Zero-Lag [J]Ticket [Q]Quit ",
        Tab::Spectrum => " [E]WiFi-Key [R]Roam [B]Lock-5GHz [S]SweetSpot [C]Gateway [J]Ticket [Q]Quit ",
        Tab::Processes => {
            " [j/k]Select [Space]Pause(SIGSTOP) [B]Shape [N]Scan-LAN [U]Resume [Q]Quit "
        }
        Tab::Tuning => " [j/k]Select [Space]Toggle [D]Doctor [F]Clear-DNS [P]Preset [A]Auto [6]IPv6 [S]Root ",
        Tab::SpeedTest => " [Space]Full-Suite [V]Quick-5s [N]Apple-RPM [D]Doctor [G]Boost [Q]Quit ",
    };
    let chunks = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" >> {}", app.status_msg),
            amber_bold(),
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
