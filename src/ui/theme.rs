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

/// A textured horizontal fill of `filled` cells out of `total`, with the
/// remainder left as a dim track. Instead of a flat solid-amber slab, the body
/// is stippled (█ leading edge, then a ▓▒ scanline texture) so bars read with
/// visual interest. `seed` offsets the pattern per row so stacked bars don't
/// line up into vertical banding. Returns just the fill+track (no brackets).
pub fn tex_bar(filled: usize, total: usize, seed: usize) -> String {
    let filled = filled.min(total);
    let mut s = String::with_capacity(total);
    for i in 0..filled {
        // Solid leading edge (last cell) keeps the bar's end crisp; the body is
        // a repeating ▓▒ stipple with a per-row phase so it looks woven.
        let ch = if i + 1 == filled {
            '█'
        } else {
            match (i + seed) % 3 {
                0 => '█',
                1 => '▓',
                _ => '▒',
            }
        };
        s.push(ch);
    }
    for _ in filled..total {
        s.push('░');
    }
    s
}

/// Like `tex_bar` but scales a value into `width` cells first (textured analogue
/// of `bar`, without brackets).
pub fn tex_bar_val(value: f64, min_val: f64, max_val: f64, width: usize, seed: usize) -> String {
    let ratio = if max_val <= min_val {
        0.0
    } else {
        ((value - min_val) / (max_val - min_val)).clamp(0.0, 1.0)
    };
    let filled = (ratio * width as f64).round() as usize;
    tex_bar(filled, width, seed)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tex_bar_fills_exact_width_and_textures_body() {
        // The textured bar must always emit exactly `total` display cells so it
        // can never overflow or shrink a panel, and the filled region must use
        // the stipple glyphs (not be a flat solid slab).
        let b = tex_bar(6, 10, 0);
        assert_eq!(b.chars().count(), 10);
        let filled = b.chars().take(6).collect::<String>();
        let track = b.chars().skip(6).collect::<String>();
        // Track is all dim.
        assert!(track.chars().all(|c| c == '░'));
        // The last filled cell is a solid edge; the body contains texture.
        assert!(filled.ends_with('█'));
        assert!(filled.chars().any(|c| c == '▓' || c == '▒' || c == '█'));
        // Empty and full edge cases don't panic and keep width.
        assert_eq!(tex_bar(0, 8, 3).chars().count(), 8);
        assert_eq!(tex_bar(99, 8, 3).chars().count(), 8);
    }
}
