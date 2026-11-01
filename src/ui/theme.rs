//! Amber CRT palette and shared gauge/sparkline renderers.

use ratatui::style::{Color, Modifier, Style};

/// Amber (#ffb000) approximated with the terminal's indexed yellow, matching the
/// original curses theme which used COLOR_YELLOW.
pub fn amber() -> Style {
    Style::new().fg(Color::Yellow)
}
pub fn amber_bold() -> Style {
    amber().add_modifier(Modifier::BOLD)
}
pub fn dim() -> Style {
    Style::new().fg(Color::Yellow).add_modifier(Modifier::DIM)
}
pub fn invert() -> Style {
    Style::new()
        .fg(Color::Black)
        .bg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}
pub fn good() -> Style {
    Style::new().fg(Color::Green).add_modifier(Modifier::BOLD)
}
pub fn alert() -> Style {
    Style::new().fg(Color::Red).add_modifier(Modifier::BOLD)
}

pub fn score_color(score: i64) -> Style {
    if score >= 75 {
        good()
    } else if score >= 50 {
        amber_bold()
    } else {
        alert()
    }
}

const SPARK: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// `[████░░░░]` progress bar, ported from the Python `render_bar`.
pub fn bar(value: f64, min_val: f64, max_val: f64, width: usize) -> String {
    let ratio = if max_val <= min_val {
        0.0
    } else {
        ((value - min_val) / (max_val - min_val)).clamp(0.0, 1.0)
    };
    let filled = (ratio * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

/// Unicode sparkline of the last `width` values, ported from `render_sparkline`.
pub fn sparkline(values: &[f64], width: usize, floor: f64) -> String {
    if width == 0 {
        return String::new();
    }
    if values.is_empty() {
        return "▁".repeat(width);
    }
    let start = values.len().saturating_sub(width);
    let mut slice: Vec<f64> = values[start..].to_vec();
    if slice.len() < width {
        let pad = slice[0];
        let mut padded = vec![pad; width - slice.len()];
        padded.extend_from_slice(&slice);
        slice = padded;
    }
    let hi = slice.iter().cloned().fold(floor + 1.0, f64::max);
    let lo = slice.iter().cloned().fold(floor, f64::min);
    let span = (hi - lo).max(1e-3);
    slice
        .iter()
        .map(|v| {
            let norm = ((v - lo) / span).clamp(0.0, 1.0);
            let idx = ((norm * (SPARK.len() - 1) as f64).round() as usize).min(SPARK.len() - 1);
            SPARK[idx]
        })
        .collect()
}
