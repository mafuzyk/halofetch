//! Colors, built-in palettes, and logo gradients.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ── Color ────────────────────────────────────────────────────────────────

/// A 24-bit RGB color, stored in config files as `#RRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const WHITE: Color = Color::new(255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b }
    }

    /// Parse `#RRGGBB` or `RRGGBB`.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim();
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        if hex.len() != 6 || !hex.is_ascii() {
            return None;
        }
        let channel = |range: std::ops::Range<usize>| u8::from_str_radix(&hex[range], 16).ok();
        Some(Color::new(channel(0..2)?, channel(2..4)?, channel(4..6)?))
    }

    /// Relative luminance in the 0–1 range (sRGB approximation).
    pub fn luminance(self) -> f64 {
        (0.2126 * f64::from(self.r) + 0.7152 * f64::from(self.g) + 0.0722 * f64::from(self.b))
            / 255.0
    }

    /// A readable text color to draw on top of this color.
    pub fn contrast_text(self) -> Color {
        if self.luminance() > 0.55 {
            Color::new(24, 24, 32)
        } else {
            Color::new(250, 250, 255)
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

impl FromStr for Color {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Color::from_hex(value).ok_or_else(|| format!("'{value}' is not a #RRGGBB color"))
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

// ── Palettes ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: &'static str,
    pub colors: Vec<Color>,
}

macro_rules! theme {
    ($name:expr, [$($hex:expr),+ $(,)?]) => {
        Theme {
            name: $name,
            colors: vec![$(Color::from_hex($hex).expect("built-in palettes use valid hex")),+],
        }
    };
}

/// Every built-in palette, in the order the editor lists them.
pub fn all_themes() -> Vec<Theme> {
    vec![
        theme!(
            "singularityos",
            ["#C084FC", "#A78BFA", "#818CF8", "#6366F1", "#4F46E5"]
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
        theme!("arch", ["#1793D1", "#1793D1", "#4DB6E8", "#1793D1"]),
        theme!("classic", ["#FFFFFF", "#E0E0E0", "#C8C8C8"]),
        // Pride flags
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
    ]
}

pub fn find_theme(name: &str) -> Option<Theme> {
    all_themes().into_iter().find(|theme| theme.name == name)
}

/// Name of the built-in palette that exactly matches `colors`, if any.
pub fn theme_name_for(colors: &[Color]) -> Option<&'static str> {
    all_themes()
        .into_iter()
        .find(|theme| theme.colors == colors)
        .map(|theme| theme.name)
}

/// Distribute `total` positions across `len` palette entries as evenly as
/// possible, keeping each color in one contiguous band.
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
        (rem + (i - thick * rem) / base).min(len - 1)
    }
}

/// Symbolic flags that are not stripes. Currently the intersex flag: a
/// purple ring on yellow.
fn flag_color_at(
    colors: &[Color],
    row: usize,
    col: usize,
    rows: usize,
    cols: usize,
) -> Option<Color> {
    let yellow = Color::new(0xFF, 0xD7, 0x00);
    let purple = Color::new(0x79, 0x02, 0xAA);
    if colors != [yellow, purple] {
        return None;
    }
    let y = row as f64 / rows.max(1) as f64 - 0.5;
    // Terminal cells are about twice as tall as they are wide.
    let x = (col as f64 / cols.max(1) as f64 - 0.5) * 0.9;
    let distance = (x * x + y * y).sqrt();
    Some(if (0.17..0.25).contains(&distance) {
        purple
    } else {
        yellow
    })
}

/// Which way a palette is spread across the logo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Gradient {
    /// One color band per group of rows.
    #[default]
    Horizontal,
    /// One color band per group of columns.
    Vertical,
}

impl Gradient {
    pub fn label(self) -> &'static str {
        match self {
            Gradient::Horizontal => "Horizontal",
            Gradient::Vertical => "Vertical",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Gradient::Horizontal => Gradient::Vertical,
            Gradient::Vertical => Gradient::Horizontal,
        }
    }
}

/// Color of the logo cell at (`row`, `col`) for a logo of `rows` × `cols`.
pub fn logo_color(
    palette: &[Color],
    gradient: Gradient,
    row: usize,
    col: usize,
    rows: usize,
    cols: usize,
) -> Color {
    if palette.is_empty() {
        return Color::WHITE;
    }
    if let Some(color) = flag_color_at(palette, row, col, rows, cols) {
        return color;
    }
    let index = match gradient {
        Gradient::Horizontal => stretch_index(row, rows, palette.len()),
        Gradient::Vertical => stretch_index(col, cols, palette.len()),
    };
    palette[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_and_rejects_garbage() {
        let color = Color::from_hex("#c084fc").unwrap();
        assert_eq!(color, Color::new(0xC0, 0x84, 0xFC));
        assert_eq!(color.to_string(), "#C084FC");
        assert!(Color::from_hex("#12345").is_none());
        assert!(Color::from_hex("#GGGGGG").is_none());
        assert!(Color::from_hex("#ééé").is_none());
    }

    #[test]
    fn colors_serialize_as_hex_strings() {
        let json = serde_json::to_string(&Color::new(1, 2, 255)).unwrap();
        assert_eq!(json, "\"#0102FF\"");
        let back: Color = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Color::new(1, 2, 255));
    }

    #[test]
    fn stretch_index_uses_every_color_in_order() {
        let bands: Vec<usize> = (0..10).map(|i| stretch_index(i, 10, 3)).collect();
        assert_eq!(bands, [0, 0, 0, 0, 1, 1, 1, 2, 2, 2]);
        assert_eq!(stretch_index(1, 2, 5), 1);
    }

    #[test]
    fn every_builtin_theme_has_a_unique_name() {
        let themes = all_themes();
        let mut names: Vec<_> = themes.iter().map(|theme| theme.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), themes.len());
    }
}
