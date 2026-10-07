//! Styled text canvas shared by the scene renderer, the editor preview and CLI output.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::theme::Color;

const RESET: &str = "\x1b[0m";
const ELLIPSIS: char = '\u{2026}';

/// Text attributes of a span. The default style means "terminal default".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
}

impl Style {
    pub const fn new() -> Self {
        Style {
            fg: None,
            bg: None,
            bold: false,
        }
    }

    pub const fn fg(self, c: Color) -> Self {
        Style {
            fg: Some(c),
            ..self
        }
    }

    pub const fn bg(self, c: Color) -> Self {
        Style {
            bg: Some(c),
            ..self
        }
    }

    pub const fn bold(self) -> Self {
        Style { bold: true, ..self }
    }
}

/// A run of text that shares one style.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

impl Span {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Span {
            text: text.into(),
            style,
        }
    }

    pub fn plain(text: impl Into<String>) -> Self {
        Span::new(text, Style::new())
    }

    /// Display width in terminal columns.
    pub fn width(&self) -> usize {
        text_width(&self.text)
    }
}

/// One row of styled spans.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Line {
    pub spans: Vec<Span>,
}

impl Line {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_spans(spans: Vec<Span>) -> Self {
        let mut line = Line::new();
        for span in spans {
            line.push(span);
        }
        line
    }

    /// Append a span, merging it into the last span when the style matches.
    pub fn push(&mut self, span: Span) {
        if span.text.is_empty() {
            return;
        }
        if let Some(last) = self.spans.last_mut() {
            if last.style == span.style {
                last.text.push_str(&span.text);
                return;
            }
        }
        self.spans.push(span);
    }

    pub fn push_str(&mut self, text: &str, style: Style) {
        self.push(Span::new(text, style));
    }

    /// Append `columns` unstyled spaces.
    pub fn pad(&mut self, columns: usize) {
        if columns > 0 {
            self.push(Span::plain(" ".repeat(columns)));
        }
    }

    /// Append unstyled spaces until the line is `width` columns wide.
    pub fn pad_to(&mut self, width: usize) {
        let current = self.width();
        if current < width {
            self.pad(width - current);
        }
    }

    pub fn append(&mut self, other: Line) {
        for span in other.spans {
            self.push(span);
        }
    }

    pub fn width(&self) -> usize {
        self.spans.iter().map(Span::width).sum()
    }

    pub fn plain_text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }

    /// Cut the line to at most `max` columns. When text is cut, the last
    /// column becomes an ellipsis that keeps the style of the cell it replaces.
    /// A wide character that would straddle the cut is replaced by a space.
    pub fn truncated(&self, max: usize) -> Line {
        if self.width() <= max {
            return self.clone();
        }
        if max == 0 {
            return Line::new();
        }
        let keep = max - 1;
        let mut out = Line::new();
        let mut used = 0;
        let mut ellipsis_style = Style::new();
        'cut: for span in &self.spans {
            for ch in span.text.chars() {
                let width = UnicodeWidthChar::width(ch).unwrap_or(0);
                if used + width > keep {
                    ellipsis_style = span.style;
                    break 'cut;
                }
                out.push(Span::new(ch.to_string(), span.style));
                used += width;
            }
        }
        out.pad(keep - used);
        out.push(Span::new(ELLIPSIS.to_string(), ellipsis_style));
        out
    }
}

/// Display width of `text` in terminal columns.
pub fn text_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Truncate plain text to `max` columns, ending in an ellipsis when cut.
pub fn truncate_text(text: &str, max: usize) -> String {
    Line::from_spans(vec![Span::plain(text)])
        .truncated(max)
        .plain_text()
}

/// SGR sequence that selects `style` from a reset state. Empty for the default style.
fn sgr(style: Style) -> String {
    let mut params: Vec<String> = Vec::new();
    if style.bold {
        params.push("1".to_string());
    }
    if let Some(c) = style.fg {
        params.push(format!("38;2;{};{};{}", c.r, c.g, c.b));
    }
    if let Some(c) = style.bg {
        params.push(format!("48;2;{};{};{}", c.r, c.g, c.b));
    }
    if params.is_empty() {
        String::new()
    } else {
        format!("\x1b[{}m", params.join(";"))
    }
}

/// Spans of a line with empty text removed and trailing unstyled-background
/// spaces dropped.
fn visible_spans(spans: &[Span]) -> Vec<(&str, Style)> {
    let mut parts: Vec<(&str, Style)> = spans
        .iter()
        .filter(|span| !span.text.is_empty())
        .map(|span| (span.text.as_str(), span.style))
        .collect();
    while let Some(last) = parts.last_mut() {
        if last.1.bg.is_some() {
            break;
        }
        let text: &str = last.0;
        let trimmed = text.trim_end_matches(' ');
        if trimmed.is_empty() {
            parts.pop();
        } else {
            last.0 = trimmed;
            break;
        }
    }
    parts
}

/// Render lines as ANSI text with 24-bit colors. Every line ends with `\n`.
pub fn to_ansi(lines: &[Line]) -> String {
    let mut out = String::new();
    for line in lines {
        let mut current = Style::new();
        for (text, style) in visible_spans(&line.spans) {
            if style != current {
                if current != Style::new() {
                    out.push_str(RESET);
                }
                out.push_str(&sgr(style));
                current = style;
            }
            out.push_str(text);
        }
        if current != Style::new() {
            out.push_str(RESET);
        }
        out.push('\n');
    }
    out
}

/// Render lines as plain text. Trailing whitespace of each line is removed and
/// every line ends with `\n`.
pub fn to_plain(lines: &[Line]) -> String {
    let mut out = String::new();
    for line in lines {
        out.push_str(line.plain_text().trim_end());
        out.push('\n');
    }
    out
}

fn ratatui_style(style: Style) -> ratatui::style::Style {
    let mut out = ratatui::style::Style::new();
    if let Some(c) = style.fg {
        out = out.fg(ratatui::style::Color::Rgb(c.r, c.g, c.b));
    }
    if let Some(c) = style.bg {
        out = out.bg(ratatui::style::Color::Rgb(c.r, c.g, c.b));
    }
    if style.bold {
        out = out.add_modifier(ratatui::style::Modifier::BOLD);
    }
    out
}

/// Convert lines for display in a ratatui widget.
pub fn to_ratatui(lines: &[Line]) -> Vec<ratatui::text::Line<'static>> {
    lines
        .iter()
        .map(|line| {
            let spans: Vec<ratatui::text::Span<'static>> = line
                .spans
                .iter()
                .map(|span| {
                    ratatui::text::Span::styled(span.text.clone(), ratatui_style(span.style))
                })
                .collect();
            ratatui::text::Line::from(spans)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Color = Color::new(1, 2, 3);

    fn fg() -> Style {
        Style::new().fg(RED)
    }

    #[test]
    fn same_style_spans_merge() {
        let mut line = Line::new();
        line.push(Span::new("ab", fg()));
        line.push(Span::new("cd", fg()));
        line.push(Span::new("", Style::new()));
        assert_eq!(line.spans, vec![Span::new("abcd", fg())]);
    }

    #[test]
    fn adjacent_same_color_spans_emit_one_escape() {
        let line = Line {
            spans: vec![Span::new("a", fg()), Span::new("b", fg())],
        };
        assert_eq!(to_ansi(&[line]), "\x1b[38;2;1;2;3mab\x1b[0m\n");
    }

    #[test]
    fn style_change_resets_before_new_attributes() {
        let line = Line::from_spans(vec![
            Span::new("a", fg()),
            Span::new("b", Style::new()),
            Span::new("c", fg().bold()),
        ]);
        assert_eq!(
            to_ansi(&[line]),
            "\x1b[38;2;1;2;3ma\x1b[0mb\x1b[1;38;2;1;2;3mc\x1b[0m\n"
        );
    }

    #[test]
    fn trailing_padding_without_background_is_trimmed() {
        let mut line = Line::from_spans(vec![Span::plain("abc")]);
        line.pad(5);
        assert_eq!(to_ansi(&[line]), "abc\n");

        let mut unstyled_tail = Line::from_spans(vec![Span::new("x", Style::new().bg(RED))]);
        unstyled_tail.pad(2);
        assert_eq!(to_ansi(&[unstyled_tail]), "\x1b[48;2;1;2;3mx\x1b[0m\n");

        let background = Style::new().bg(RED);
        let kept = Line::from_spans(vec![
            Span::new("x", background),
            Span::new("  ", background),
        ]);
        assert_eq!(to_ansi(&[kept]), "\x1b[48;2;1;2;3mx  \x1b[0m\n");
    }

    #[test]
    fn bold_and_fg_share_one_sequence() {
        let line = Line::from_spans(vec![Span::new("x", fg().bold())]);
        assert_eq!(to_ansi(&[line]), "\x1b[1;38;2;1;2;3mx\x1b[0m\n");
    }

    #[test]
    fn plain_lines_contain_no_escapes() {
        let lines = [Line::from_spans(vec![Span::plain("hi there")])];
        let ansi = to_ansi(&lines);
        assert_eq!(ansi, "hi there\n");
        assert!(!ansi.contains('\x1b'));
    }

    #[test]
    fn to_plain_trims_line_ends() {
        let mut line = Line::from_spans(vec![Span::plain("abc")]);
        line.pad(3);
        assert_eq!(to_plain(&[line, Line::new()]), "abc\n\n");
    }

    #[test]
    fn width_counts_wide_characters_as_two_columns() {
        assert_eq!(text_width("ab宽"), 4);
        let mut line = Line::from_spans(vec![Span::plain("ab宽")]);
        line.pad_to(6);
        assert_eq!(line.width(), 6);
        line.pad_to(2);
        assert_eq!(line.width(), 6);
    }

    #[test]
    fn truncation_replaces_last_column_with_ellipsis() {
        assert_eq!(truncate_text("abcdefgh", 5), "abcd\u{2026}");
        assert_eq!(truncate_text("abc", 5), "abc");
        assert_eq!(truncate_text("abc", 0), "");
        assert_eq!(truncate_text("abc", 1), "\u{2026}");
    }

    #[test]
    fn truncation_never_splits_wide_characters() {
        assert_eq!(truncate_text("ab宽c", 3), "ab\u{2026}");
        assert_eq!(truncate_text("a宽c", 3), "a \u{2026}");
        let cut = Line::from_spans(vec![Span::plain("a宽c")]).truncated(3);
        assert_eq!(cut.width(), 3);
    }

    #[test]
    fn truncation_keeps_style_of_the_ellipsis_cell() {
        let line = Line::from_spans(vec![Span::new("abc", Style::new()), Span::new("def", fg())]);
        let cut = line.truncated(4);
        assert_eq!(cut.plain_text(), "abc\u{2026}");
        assert_eq!(cut.spans.last().map(|span| span.style), Some(fg()));
    }

    #[test]
    fn to_ratatui_keeps_text_and_style() {
        let line = Line::from_spans(vec![Span::new("x", fg().bold())]);
        let converted = to_ratatui(&[line]);
        let span = &converted[0].spans[0];
        assert_eq!(span.content, "x");
        assert_eq!(span.style.fg, Some(ratatui::style::Color::Rgb(1, 2, 3)));
        assert!(span
            .style
            .add_modifier
            .contains(ratatui::style::Modifier::BOLD));
    }
}
