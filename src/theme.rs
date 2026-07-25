// Color and theme management.
//
// Colors are stored as RGB triples and serialized as hex strings (#RRGGBB)
// for human readability. The theme list is the source of truth for presets;
// the config stores whichever theme the user chose (or a custom palette).

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

// ── Color ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b }
    }

    /// Parse a hex color like "#FF6692" or "#ff6692"
    pub fn from_hex(hex: &str) -> Self {
        Self::from_hex_opt(hex).unwrap_or(Color::new(255, 255, 255))
    }

    pub fn from_hex_opt(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        if hex.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Color { r, g, b })
    }

    #[allow(dead_code)]
    pub fn to_hex_string(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// ANSI true-color foreground escape sequence
    pub fn fg_escape(&self) -> String {
        format!("\x1b[38;2;{};{};{}m", self.r, self.g, self.b)
    }

    /// ANSI true-color background escape sequence
    #[allow(dead_code)]
    pub fn bg_escape(&self) -> String {
        format!("\x1b[48;2;{};{};{}m", self.r, self.g, self.b)
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

impl FromStr for Color {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex_opt(s).ok_or_else(|| format!("Invalid color: {}", s))
    }
}

// ── Theme ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
    pub colors: Vec<Color>,
}

/// All built-in themes.
/// The first 7 colors are used for ASCII art rendering; additional colors
/// beyond 7 are ignored but kept for future use.
macro_rules! theme {
    ($name:expr, [$($c:expr),+ $(,)?]) => {
        Theme {
            name: $name,
            colors: vec![$(Color::from_hex($c)),+],
        }
    };
}

pub fn all_themes() -> Vec<Theme> {
    vec![
        // LGBTQ+ flags
        theme!(
            "xenogender",
            ["#FF6692", "#FF9A98", "#FFB883", "#FBFFA8", "#85BCFF", "#9D85FF", "#A510FF"]
        ),
        theme!(
            "trans",
            ["#55CDFC", "#55CDFC", "#F7A8B8", "#FFFFFF", "#F7A8B8", "#55CDFC", "#55CDFC"]
        ),
        theme!("nb", ["#FFF430", "#FFFFFF", "#9C59D1", "#2C2C2C"]),
        theme!(
            "genderfluid",
            ["#FF75A2", "#FFFFFF", "#C011D7", "#2C2C2C", "#3170D0"]
        ),
        theme!("pan", ["#FF218C", "#FFD800", "#21B1FF"]),
        theme!(
            "bi",
            ["#D60270", "#D60270", "#9B4F96", "#0038A8", "#0038A8"]
        ),
        theme!("ace", ["#000000", "#A4A4A4", "#FFFFFF", "#810081"]),
        theme!(
            "lesbian",
            ["#D52D00", "#D52D00", "#FF9A56", "#FFFFFF", "#D362A4", "#A30262", "#A30262"]
        ),
        theme!(
            "gay",
            ["#078D70", "#26CEAA", "#98E8C1", "#FFFFFF", "#7BADE2", "#5049CC", "#3D1A78"]
        ),
        theme!("intersex", ["#FFD700", "#7902AA"]),
        theme!(
            "aromantic",
            ["#3DA542", "#A8D47A", "#FFFFFF", "#BABABA", "#000000"]
        ),
        theme!(
            "agender",
            ["#000000", "#BABABA", "#FFFFFF", "#B4FF3B", "#FFFFFF", "#BABABA", "#000000"]
        ),
        // Themes
        theme!(
            "arch",
            ["#1793D1", "#1793D1", "#1793D1", "#1793D1", "#1793D1"]
        ),
        theme!(
            "catppuccin-mocha",
            ["#f5c2e7", "#cba6f7", "#94e2d5", "#a6e3a1", "#f9e2af", "#fab387", "#89b4fa"]
        ),
        theme!(
            "catppuccin-latte",
            ["#dd7878", "#8839ef", "#40a02b", "#fe640b", "#df8e1d", "#04a5e5", "#209fb5"]
        ),
        theme!(
            "dracula",
            ["#ff5555", "#ff79c6", "#bd93f9", "#50fa7b", "#f1fa8c", "#ffb86c", "#8be9fd"]
        ),
        theme!(
            "gruvbox",
            ["#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#fb4934"]
        ),
        theme!(
            "tokyonight",
            ["#f7768e", "#bb9af7", "#7dcfff", "#9ece6a", "#e0af68", "#73daca", "#ff9e64"]
        ),
        theme!(
            "nord",
            ["#bf616a", "#d08770", "#ebcb8b", "#a3be8c", "#b48ead", "#88c0d0", "#81a1c1"]
        ),
        theme!(
            "everforest",
            ["#e67e80", "#e69875", "#dbbc7f", "#a7c080", "#7fbbb3", "#83c092", "#d3c6aa"]
        ),
        theme!(
            "solarized-dark",
            ["#dc322f", "#cb4b16", "#b58900", "#859900", "#6c71c4", "#268bd2", "#2aa198"]
        ),
        theme!(
            "monokai",
            ["#f92672", "#fd971f", "#e6db74", "#a6e22e", "#66d9ef", "#ae81ff", "#f8f8f2"]
        ),
        theme!(
            "one-dark",
            ["#e06c75", "#d19a66", "#e5c07b", "#98c379", "#56b6c2", "#61afef", "#c678dd"]
        ),
        theme!(
            "rose-pine",
            ["#eb6f92", "#f6c177", "#ebbcba", "#31748f", "#9ccfd8", "#c4a7e7", "#e0def4"]
        ),
        theme!(
            "synthwave",
            ["#ff7edb", "#ff7edb", "#36f9f6", "#36f9f6", "#ffe066", "#ffe066", "#b4a0ff"]
        ),
        // SingularityOS
        theme!(
            "singularityos",
            ["#C084FC", "#A78BFA", "#818CF8", "#6366F1", "#4F46E5"]
        ),
        // Classic (all white)
        theme!(
            "classic",
            ["#FFFFFF", "#FFFFFF", "#FFFFFF", "#FFFFFF", "#FFFFFF"]
        ),
    ]
}

/// Stretch index: distributes `total` positions across `len` palette entries
/// as evenly as possible — each entry gets at least `total/len` rows, and the
/// first `total % len` entries get one extra.  No colour is ever skipped when
/// `total >= len`, and the mapping stays contiguous.
pub fn stretch_index(i: usize, total: usize, len: usize) -> usize {
    if len <= 1 || total == 0 {
        return 0;
    }
    if total <= len {
        return i.min(len - 1);
    }
    let base = total / len;
    let rem = total % len;
    let thick = base + 1;
    if i < thick * rem {
        i / thick
    } else {
        rem + (i - thick * rem) / base
    }
}

/// Return a flag-pattern colour for a given position, if the palette matches
/// a known symbolic flag (e.g. intersex).  `swapped` flips the foreground /
/// background roles (used by the `v` direction toggle).
pub fn flag_color_at(
    colors: &[Color],
    row: usize,
    col: usize,
    total_rows: usize,
    total_cols: usize,
    swapped: bool,
) -> Option<Color> {
    // ── Intersex flag ──────────────────────────────────────────────────
    // Yellow (#FFD700) background with a purple (#7902AA) ring (outline circle).
    if colors.len() >= 2 {
        let yellow = Color {
            r: 0xFF,
            g: 0xD7,
            b: 0x00,
        };
        let purple = Color {
            r: 0x79,
            g: 0x02,
            b: 0xAA,
        };
        if (colors[0] == yellow || colors[0] == purple)
            && (colors[1] == yellow || colors[1] == purple)
            && colors[0] != colors[1]
        {
            let (bg, ring) = if colors[0] == yellow {
                (&colors[0], &colors[1])
            } else {
                (&colors[1], &colors[0])
            };
            let r = row as f64 / total_rows.max(1) as f64;
            let c = col as f64 / total_cols.max(1) as f64;
            let dist = ((r - 0.5).powi(2) + (c - 0.5).powi(2)).sqrt();
            let in_ring = (0.18..0.22).contains(&dist);
            return Some(if swapped {
                if in_ring {
                    *bg
                } else {
                    *ring
                }
            } else {
                if in_ring {
                    *ring
                } else {
                    *bg
                }
            });
        }
    }
    None
}

#[allow(dead_code)]
pub fn find_theme(name: &str) -> Option<Theme> {
    all_themes().into_iter().find(|t| t.name == name)
}

/// The default theme name used for new configs.
#[allow(dead_code)]
pub const DEFAULT_THEME: &str = "singularityos";

/// Pre-computed list of theme names for the TUI.
#[allow(dead_code)]
pub const PRESET_THEMES: &[&str] = &[
    "xenogender",
    "trans",
    "nb",
    "genderfluid",
    "pan",
    "bi",
    "ace",
    "lesbian",
    "gay",
    "intersex",
    "aromantic",
    "agender",
    "arch",
    "catppuccin-mocha",
    "catppuccin-latte",
    "dracula",
    "gruvbox",
    "tokyonight",
    "nord",
    "everforest",
    "solarized-dark",
    "monokai",
    "one-dark",
    "rose-pine",
    "synthwave",
    "singularityos",
    "classic",
];
