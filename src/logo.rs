//! Logo discovery, cleaning and colorization.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use unicode_width::UnicodeWidthChar;

use crate::render::{text_width, Line, Span, Style};
use crate::theme::{self, Color, Gradient};

include!(concat!(env!("OUT_DIR"), "/logos_generated.rs"));

const BRAILLE_BLANK: char = '\u{2800}';

/// A cleaned ASCII-art logo.
#[derive(Debug, Clone, PartialEq)]
pub struct Logo {
    pub lines: Vec<String>,
    /// Maximum display width of the lines.
    pub width: usize,
}

impl Logo {
    /// Clean `text` and build a logo. `None` when nothing visible remains.
    pub fn from_text(text: &str) -> Option<Logo> {
        let cleaned = clean(text);
        if cleaned.is_empty() {
            return None;
        }
        let lines: Vec<String> = cleaned.lines().map(str::to_string).collect();
        let width = lines.iter().map(|line| text_width(line)).max().unwrap_or(0);
        Some(Logo { lines, width })
    }

    pub fn height(&self) -> usize {
        self.lines.len()
    }
}

/// Where the logo comes from. Serialized as `{"kind": "auto"}` and similar.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum LogoSource {
    #[default]
    Auto,
    Builtin {
        key: String,
    },
    File {
        path: String,
    },
    None,
}

/// Full-size logo and optional compact variant.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LogoSet {
    pub full: Option<Logo>,
    pub small: Option<Logo>,
}

impl LogoSet {
    /// Largest variant whose width is at most `max_width` (full first, then small).
    pub fn fitting(&self, max_width: usize) -> Option<&Logo> {
        [self.full.as_ref(), self.small.as_ref()]
            .into_iter()
            .flatten()
            .find(|logo| logo.width <= max_width)
    }
}

/// Strip escape sequences, expand tabs, remove the indentation shared by all
/// visible lines, trim line ends and drop leading and trailing blank lines.
pub fn clean(text: &str) -> String {
    let text = strip_ansi(text).replace('\t', "    ");
    let lines: Vec<&str> = text.lines().collect();

    // Ordinary spaces and braille blanks both count as indentation, since
    // community logo files use either to align art.
    let indent = lines
        .iter()
        .filter(|line| !is_blank(line))
        .map(|line| line.chars().take_while(|&c| is_blank_cell(c)).count())
        .min()
        .unwrap_or(0);

    let cleaned: Vec<String> = lines
        .iter()
        .map(|line| {
            let shifted: String = line.chars().skip(indent).collect();
            shifted
                .trim_end_matches(|c: char| c.is_whitespace() || c == BRAILLE_BLANK)
                .to_string()
        })
        .collect();

    match (
        cleaned.iter().position(|line| !line.is_empty()),
        cleaned.iter().rposition(|line| !line.is_empty()),
    ) {
        (Some(first), Some(last)) => cleaned[first..=last].join("\n"),
        _ => String::new(),
    }
}

fn is_blank_cell(c: char) -> bool {
    c == ' ' || c == BRAILLE_BLANK
}

fn is_blank(line: &str) -> bool {
    line.chars()
        .all(|c| c.is_whitespace() || c == BRAILLE_BLANK)
}

/// Remove CSI sequences (`ESC [ ... final`) and two-byte escapes.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
        } else if chars.next_if_eq(&'[').is_some() {
            // Parameter and intermediate bytes end at the first byte in 0x40..=0x7E.
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        } else {
            chars.next();
        }
    }
    out
}

/// Keys of the logos embedded at build time, sorted, without `*_small` variants.
pub fn builtin_keys() -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = embedded_keys()
        .iter()
        .copied()
        .filter(|key| is_listable(key))
        .collect();
    keys.sort_unstable();
    keys
}

fn is_listable(name: &str) -> bool {
    !name.starts_with('.') && !name.ends_with("_small")
}

/// Built-in keys plus the logo files in `user_dir`, sorted and deduplicated.
pub fn available(user_dir: Option<&Path>) -> Vec<String> {
    let mut keys: Vec<String> = builtin_keys().into_iter().map(str::to_string).collect();
    if let Some(dir) = user_dir {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let Ok(name) = entry.file_name().into_string() else {
                    continue;
                };
                if is_listable(&name) && entry.path().is_file() {
                    keys.push(name);
                }
            }
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

/// Logo for `key`: a file in `user_dir` wins over the embedded logo.
pub fn load_key(key: &str, user_dir: Option<&Path>) -> Option<Logo> {
    if key.is_empty() || key.starts_with('.') || key.contains(['/', '\\']) {
        return None;
    }
    if let Some(dir) = user_dir {
        if let Ok(text) = std::fs::read_to_string(dir.join(key)) {
            if let Some(logo) = Logo::from_text(&text) {
                return Some(logo);
            }
        }
    }
    get_embedded(key).and_then(Logo::from_text)
}

/// First OS id that has a built-in logo (trying `-` as `_` too), else `"linux"`.
pub fn detect_key(os_ids: &[String]) -> String {
    let keys = builtin_keys();
    os_ids
        .iter()
        .find_map(|id| {
            [id.clone(), id.replace('-', "_")]
                .into_iter()
                .find(|candidate| keys.contains(&candidate.as_str()))
        })
        .unwrap_or_else(|| "linux".to_string())
}

fn key_set(key: &str, user_dir: Option<&Path>, auto_small: bool) -> Option<LogoSet> {
    let full = load_key(key, user_dir)?;
    let small = if auto_small {
        load_key(&format!("{key}_small"), user_dir)
    } else {
        None
    };
    Some(LogoSet {
        full: Some(full),
        small,
    })
}

/// Logos for a configured source. An unknown builtin key falls back to the
/// logo detected from `os_ids`.
pub fn resolve(
    source: &LogoSource,
    os_ids: &[String],
    user_dir: Option<&Path>,
    auto_small: bool,
) -> LogoSet {
    let detected = || detect_key(os_ids);
    match source {
        LogoSource::None => LogoSet::default(),
        LogoSource::File { path } => LogoSet {
            full: std::fs::read_to_string(expand_home(path))
                .ok()
                .and_then(|text| Logo::from_text(&text)),
            small: None,
        },
        LogoSource::Auto => key_set(&detected(), user_dir, auto_small).unwrap_or_default(),
        LogoSource::Builtin { key } => key_set(key, user_dir, auto_small)
            .or_else(|| key_set(&detected(), user_dir, auto_small))
            .unwrap_or_default(),
    }
}

/// Replace a leading `~` with `$HOME`. Paths without `~` are returned unchanged.
pub fn expand_home(path: &str) -> PathBuf {
    let rest = if path == "~" {
        Some("")
    } else {
        path.strip_prefix("~/")
    };
    match (rest, std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

/// Color every visible cell of the logo with the palette. Lines are padded to
/// `logo.width`; unstyled cells (spaces and braille blanks) carry no color.
pub fn colorize(logo: &Logo, palette: &[Color], gradient: Gradient) -> Vec<Line> {
    let rows = logo.lines.len();
    logo.lines
        .iter()
        .enumerate()
        .map(|(row, text)| {
            let mut line = Line::new();
            let mut col = 0;
            for ch in text.chars() {
                if is_blank_cell(ch) {
                    line.push(Span::plain(ch.to_string()));
                } else {
                    let color = theme::logo_color(palette, gradient, row, col, rows, logo.width);
                    line.push(Span::new(ch.to_string(), Style::new().fg(color)));
                }
                col += UnicodeWidthChar::width(ch).unwrap_or(0);
            }
            line.pad_to(logo.width);
            line
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_removes_only_shared_indentation() {
        assert_eq!(clean("    /\\  \n   /__\\\n"), " /\\\n/__\\");
    }

    #[test]
    fn clean_treats_braille_blanks_as_indentation() {
        assert_eq!(clean("\u{2800}A\n\u{2800}\u{2800}B"), "A\n\u{2800}B");
    }

    #[test]
    fn clean_strips_ansi_sequences() {
        assert_eq!(clean("\x1b[1;31mAB\x1b[0m\n\x1b[32mCD\x1b[0m"), "AB\nCD");
    }

    #[test]
    fn clean_expands_tabs_and_drops_blank_edges() {
        assert_eq!(clean("\n\n\tX\n\t\tY\n\n"), "X\n    Y");
        assert_eq!(clean("  \n   \n"), "");
    }

    #[test]
    fn logo_width_is_display_width() {
        let logo = Logo::from_text("宽宽\nab").expect("visible text");
        assert_eq!(logo.width, 4);
        assert_eq!(logo.height(), 2);
        assert!(Logo::from_text(" \n\u{2800}\n").is_none());
    }

    #[test]
    fn detect_key_uses_first_known_id() {
        let ids = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(detect_key(&ids(&["ubuntu"])), "ubuntu");
        assert_eq!(detect_key(&ids(&["manjaro", "arch"])), "manjaro");
        assert_eq!(detect_key(&ids(&["foo", "arch"])), "arch");
        assert_eq!(detect_key(&ids(&["foo-bar", "arch"])), "arch");
        assert_eq!(detect_key(&[]), "linux");
    }

    #[test]
    fn fitting_prefers_full_then_small() {
        let logo = |width| Logo {
            lines: vec!["x".repeat(width)],
            width,
        };
        let set = LogoSet {
            full: Some(logo(40)),
            small: Some(logo(20)),
        };
        assert_eq!(set.fitting(50).map(|l| l.width), Some(40));
        assert_eq!(set.fitting(30).map(|l| l.width), Some(20));
        assert!(set.fitting(10).is_none());
        assert!(LogoSet::default().fitting(100).is_none());
    }

    #[test]
    fn colorize_merges_runs_of_one_color() {
        let logo = Logo {
            lines: vec!["AAA".to_string()],
            width: 3,
        };
        let lines = colorize(&logo, &[Color::new(9, 9, 9)], Gradient::Horizontal);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 1);
        assert_eq!(lines[0].spans[0].text, "AAA");
        assert_eq!(lines[0].spans[0].style.fg, Some(Color::new(9, 9, 9)));
    }

    #[test]
    fn colorize_pads_short_lines_and_leaves_blanks_unstyled() {
        let logo = Logo {
            lines: vec!["A \u{2800}".to_string(), "AB".to_string()],
            width: 3,
        };
        let lines = colorize(&logo, &[Color::WHITE], Gradient::Vertical);
        assert_eq!(lines[0].width(), 3);
        assert_eq!(lines[0].plain_text(), "A \u{2800}");
        assert_eq!(lines[0].spans[1].style.fg, None);
        assert_eq!(lines[1].plain_text(), "AB ");
    }

    #[test]
    fn expand_home_only_touches_tilde_prefix() {
        assert_eq!(expand_home("/etc/logo.txt"), PathBuf::from("/etc/logo.txt"));
        assert!(expand_home("~/logo.txt").ends_with("logo.txt"));
    }

    #[test]
    fn load_key_rejects_paths() {
        assert!(load_key("../secret", None).is_none());
        assert!(load_key("", None).is_none());
    }

    #[test]
    fn resolve_none_is_empty() {
        let set = resolve(&LogoSource::None, &[], None, true);
        assert_eq!(set, LogoSet::default());
    }

    #[test]
    fn resolve_unknown_builtin_falls_back_to_detected_key() {
        let ids = vec!["arch".to_string()];
        let set = resolve(
            &LogoSource::Builtin {
                key: "no-such-logo".to_string(),
            },
            &ids,
            None,
            false,
        );
        assert!(set.full.is_some());
        assert!(set.small.is_none());
    }

    #[test]
    fn resolve_loads_small_variant_when_requested() {
        let ids = vec!["arch".to_string()];
        let set = resolve(&LogoSource::Auto, &ids, None, true);
        assert!(set.full.is_some());
        assert!(set.small.is_some());
    }

    #[test]
    fn resolve_missing_file_is_empty() {
        let source = LogoSource::File {
            path: "/nonexistent/halofetch-logo".to_string(),
        };
        assert_eq!(resolve(&source, &[], None, true), LogoSet::default());
    }

    #[test]
    fn available_lists_sorted_keys_without_small_variants() {
        let keys = available(None);
        assert!(keys.iter().any(|key| key == "arch"));
        assert!(keys.iter().all(|key| !key.ends_with("_small")));
        let mut sorted = keys.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(keys, sorted);
    }

    #[test]
    fn logo_source_serializes_with_kind_tag() {
        let parsed: LogoSource =
            serde_json::from_str(r#"{"kind":"builtin","key":"arch"}"#).unwrap();
        assert_eq!(
            parsed,
            LogoSource::Builtin {
                key: "arch".to_string()
            }
        );
        let auto: LogoSource = serde_json::from_str(r#"{"kind":"auto"}"#).unwrap();
        assert_eq!(auto, LogoSource::Auto);
        assert_eq!(
            serde_json::to_string(&LogoSource::None).unwrap(),
            r#"{"kind":"none"}"#
        );
    }
}
