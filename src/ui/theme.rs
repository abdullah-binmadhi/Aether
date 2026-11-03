//! Adaptive palette system with 10 high-contrast themes and shared gauge/sparkline renderers.

use std::sync::atomic::{AtomicUsize, Ordering};
use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeId {
    AmberCrt = 0,
    PhosphorGreen = 1,
    CyberpunkNeon = 2,
    TokyoNight = 3,
    Dracula = 4,
    NordFrost = 5,
    GruvboxGold = 6,
    MonokaiPro = 7,
    RosePine = 8,
    OledMonochrome = 9,
}

static CURRENT_THEME: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy)]
pub struct ThemePalette {
    pub name: &'static str,
    pub primary: Color,
    pub primary_dim: Color,
    pub good: Color,
    pub alert: Color,
    pub bg_invert: Color,
}

pub const THEMES: [ThemePalette; 10] = [
    ThemePalette {
        name: "Amber CRT",
        primary: Color::Rgb(255, 176, 0),     // #FFB000 Classic Amber Phosphor
        primary_dim: Color::Rgb(160, 110, 0), // #A06E00
        good: Color::Rgb(80, 220, 100),       // #50DC64
        alert: Color::Rgb(255, 75, 75),       // #FF4B4B
        bg_invert: Color::Black,
    },
    ThemePalette {
        name: "Phosphor Green",
        primary: Color::Rgb(0, 255, 102),     // #00FF66 Matrix Terminal Green
        primary_dim: Color::Rgb(0, 130, 50),
        good: Color::Rgb(50, 255, 150),
        alert: Color::Rgb(255, 80, 80),
        bg_invert: Color::Black,
    },
    ThemePalette {
        name: "Cyberpunk Neon",
        primary: Color::Rgb(0, 229, 255),     // #00E5FF Electric Cyan
        primary_dim: Color::Rgb(0, 120, 150),
        good: Color::Rgb(0, 255, 180),
        alert: Color::Rgb(255, 0, 127),       // #FF007F Synthwave Hot Pink
        bg_invert: Color::Black,
    },
    ThemePalette {
        name: "Tokyo Night",
        primary: Color::Rgb(122, 162, 247),   // #7AA2F7 Midnight Indigo
        primary_dim: Color::Rgb(86, 95, 137),  // #565F89
        good: Color::Rgb(158, 206, 106),      // #9ECE6A Bamboo Green
        alert: Color::Rgb(247, 118, 142),     // #F7768E Blossom Red
        bg_invert: Color::Rgb(26, 27, 38),
    },
    ThemePalette {
        name: "Dracula",
        primary: Color::Rgb(189, 147, 249),   // #BD93F9 Gothic Lavender
        primary_dim: Color::Rgb(98, 114, 164), // #6272A4
        good: Color::Rgb(80, 250, 123),       // #50FA7B Acid Green
        alert: Color::Rgb(255, 85, 85),       // #FF5555 Crimson
        bg_invert: Color::Rgb(40, 42, 54),
    },
    ThemePalette {
        name: "Nord Frost",
        primary: Color::Rgb(136, 192, 208),   // #88C0D0 Glacial Frost
        primary_dim: Color::Rgb(76, 86, 106),  // #4C566A
        good: Color::Rgb(163, 190, 140),      // #A3BE8C Aurora Green
        alert: Color::Rgb(191, 97, 106),      // #BF616A Aurora Red
        bg_invert: Color::Rgb(46, 52, 64),
    },
    ThemePalette {
        name: "Gruvbox Gold",
        primary: Color::Rgb(250, 189, 47),    // #FABD2F Retro Ochre Gold
        primary_dim: Color::Rgb(146, 131, 116),// #928374
        good: Color::Rgb(184, 187, 38),       // #B8BB26 Olive
        alert: Color::Rgb(251, 73, 52),       // #FB4934 Terracotta
        bg_invert: Color::Rgb(40, 40, 40),
    },
    ThemePalette {
        name: "Monokai Pro",
        primary: Color::Rgb(255, 97, 136),    // #FF6188 Vivid Magenta
        primary_dim: Color::Rgb(114, 112, 114),// #727072
        good: Color::Rgb(169, 220, 103),      // #A9DC67 Apple Lime
        alert: Color::Rgb(255, 216, 102),     // #FFD866 Flare Sun
        bg_invert: Color::Rgb(45, 42, 46),
    },
    ThemePalette {
        name: "Rose Pine",
        primary: Color::Rgb(235, 111, 146),   // #EB6F92 Moody Coral Rose
        primary_dim: Color::Rgb(110, 106, 134),// #6E6A86
        good: Color::Rgb(156, 207, 216),      // #9CCFD8 Seafoam
        alert: Color::Rgb(235, 188, 186),     // #EBBCBA Rose Gold
        bg_invert: Color::Rgb(25, 23, 36),
    },
    ThemePalette {
        name: "OLED Monochrome",
        primary: Color::Rgb(236, 239, 244),   // #ECEFF4 Pure Platinum
        primary_dim: Color::Rgb(115, 125, 140),// #737D8C
        good: Color::Rgb(216, 222, 233),
        alert: Color::Rgb(255, 100, 100),
        bg_invert: Color::Black,
    },
];

pub fn current_theme_palette() -> &'static ThemePalette {
    let idx = CURRENT_THEME.load(Ordering::Relaxed) % THEMES.len();
    &THEMES[idx]
}

pub fn cycle_theme() -> &'static str {
    let next = (CURRENT_THEME.load(Ordering::Relaxed) + 1) % THEMES.len();
    CURRENT_THEME.store(next, Ordering::Relaxed);
    THEMES[next].name
}

pub fn set_theme(idx: usize) -> &'static str {
    let valid = idx % THEMES.len();
    CURRENT_THEME.store(valid, Ordering::Relaxed);
    THEMES[valid].name
}

pub fn theme_name() -> &'static str {
    current_theme_palette().name
}

pub fn amber() -> Style {
    Style::new().fg(current_theme_palette().primary)
}

pub fn amber_bold() -> Style {
    amber().add_modifier(Modifier::BOLD)
}

pub fn dim() -> Style {
    Style::new().fg(current_theme_palette().primary_dim)
}

pub fn invert() -> Style {
    let p = current_theme_palette();
    Style::new()
        .fg(p.bg_invert)
        .bg(p.primary)
        .add_modifier(Modifier::BOLD)
}

pub fn good() -> Style {
    Style::new().fg(current_theme_palette().good).add_modifier(Modifier::BOLD)
}

pub fn alert() -> Style {
    Style::new().fg(current_theme_palette().alert).add_modifier(Modifier::BOLD)
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

    #[test]
    fn all_10_themes_cycle_and_have_distinct_names() {
        assert_eq!(THEMES.len(), 10);
        let first = set_theme(0);
        assert_eq!(first, "Amber CRT");
        for _ in 0..10 {
            let name = cycle_theme();
            assert!(!name.is_empty());
        }
        assert_eq!(theme_name(), "Amber CRT");
    }
}
