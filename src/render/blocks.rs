//! Parts shared by the scenes: information rows, bars, gauges, titles and color blocks.
//!
//! Column arithmetic lives here. [`row_overhead`] states how many columns an information
//! row uses besides its value, and [`info_row`] draws exactly that many columns plus the
//! (truncated) value, so the scenes can fit values without measuring rendered text.

use crate::config::{FieldEntry, InfoStyle};
use crate::field::Field;
use crate::info::SysInfo;
use crate::render::{text_width, truncate_text, Line, Span, Style};
use crate::theme::Color;

use super::scene::RenderCtx;

/// Cells of the bar drawn in front of a value that has a gauge.
const ROW_BAR_CELLS: usize = 10;
const MAX_COLOR_BLOCKS: usize = 8;
const GAUGE_LABEL_W: usize = 10;
/// Columns of a gauge line besides its bar: label, a space, a space, a four column value.
const GAUGE_OVERHEAD: usize = GAUGE_LABEL_W + 6;
const GAUGE_BAR_MIN: usize = 6;
/// Widest gauge line a single column shows: a bar of 30 cells.
pub const GAUGE_LINE_MAX: usize = GAUGE_OVERHEAD + 30;
const GAUGES: [Field; 9] = [
    Field::CpuUsage,
    Field::GpuUsage,
    Field::Memory,
    Field::Swap,
    Field::Disk,
    Field::Vram,
    Field::CpuTemp,
    Field::Battery,
    Field::Brightness,
];

const GOOD: Color = Color::new(0x50, 0xFA, 0x7B);
const WARN: Color = Color::new(0xF1, 0xFA, 0x8C);
const BAD: Color = Color::new(0xFF, 0x55, 0x55);
/// Unfilled part of a bar.
const MUTED: Color = Color::new(0x4A, 0x4A, 0x5E);

/// Powerline separators: a solid triangle pointing right and one pointing left.
const ARROW_RIGHT: &str = "\u{e0b0}";
const ARROW_LEFT: &str = "\u{e0b2}";

/// One information row ready to be drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub field: Field,
    /// Trimmed icon. Empty when the entry has none.
    pub icon: String,
    pub label: String,
    pub value: String,
    pub color: Color,
    /// Bar fill, for entries with `bar` enabled whose field has a gauge.
    pub gauge: Option<f64>,
}

/// Which configured panels an entry list is built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Both,
}

/// Enabled entries of the requested panels, in configuration order.
///
/// Colors come from the position among all enabled entries (left panel first), so an
/// entry keeps its color whichever panel or scene shows it. Values that are absent are
/// skipped when `hide_empty` is set and shown as "n/a" otherwise.
pub fn entries(ctx: &RenderCtx, side: Side) -> Vec<Entry> {
    let left: Vec<&FieldEntry> = ctx.cfg.fields.left.iter().filter(|e| e.enabled).collect();
    let right: Vec<&FieldEntry> = ctx.cfg.fields.right.iter().filter(|e| e.enabled).collect();
    let mut out = Vec::new();
    if matches!(side, Side::Left | Side::Both) {
        out.extend(
            left.iter()
                .enumerate()
                .filter_map(|(index, entry)| entry_for(ctx, entry, index)),
        );
    }
    if matches!(side, Side::Right | Side::Both) {
        let offset = left.len();
        out.extend(
            right
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| entry_for(ctx, entry, offset + index)),
        );
    }
    out
}

fn entry_for(ctx: &RenderCtx, entry: &FieldEntry, index: usize) -> Option<Entry> {
    let (value, gauge) = match ctx.info.get(entry.field) {
        Some(value) => {
            let gauge = if entry.bar {
                ctx.info.gauge(entry.field)
            } else {
                None
            };
            (value.to_string(), gauge)
        }
        None if ctx.cfg.layout.hide_empty => return None,
        None => ("n/a".to_string(), None),
    };
    Some(Entry {
        field: entry.field,
        icon: if ctx.cfg.layout.icons {
            entry.icon.trim().to_string()
        } else {
            String::new()
        },
        label: entry.label.clone(),
        value,
        color: palette_color(&ctx.cfg.colors.palette, index),
        gauge,
    })
}

/// Palette color for position `index`, cycling through the palette.
fn palette_color(palette: &[Color], index: usize) -> Color {
    if palette.is_empty() {
        Color::WHITE
    } else {
        palette[index % palette.len()]
    }
}

/// Columns the value of an entry takes, bar included.
pub fn value_width(entry: &Entry) -> usize {
    let text = text_width(&entry.value);
    if entry.gauge.is_some() {
        ROW_BAR_CELLS + 1 + text
    } else {
        text
    }
}

/// The icon as drawn: trimmed, and empty when it occupies no columns.
fn shown_icon(icon: &str) -> &str {
    let icon = icon.trim();
    if text_width(icon) == 0 {
        ""
    } else {
        icon
    }
}

/// Columns an icon takes in a row, 0 when it is not drawn.
pub fn icon_width(icon: &str) -> usize {
    text_width(shown_icon(icon))
}

/// Columns of an information row that are not the value: the label padded to `label_w`,
/// the icon with its separating space, and the separators and padding of the style.
/// `icon_w` is 0 when the row has no icon.
pub fn row_overhead(style: InfoStyle, label_w: usize, icon_w: usize) -> usize {
    let icon_part = if icon_w > 0 { icon_w + 1 } else { 0 };
    match style {
        InfoStyle::Powerline => label_w + 4 + icon_part,
        InfoStyle::Plain => label_w + 2 + icon_part,
    }
}

/// [`row_overhead`] for a concrete entry.
pub fn entry_overhead(entry: &Entry, style: InfoStyle, label_w: usize) -> usize {
    row_overhead(style, label_w, icon_width(&entry.icon))
}

/// Columns before the label in a plain, non-mirrored row: the icon and its space.
pub fn label_offset(entry: &Entry) -> usize {
    match icon_width(&entry.icon) {
        0 => 0,
        width => width + 1,
    }
}

#[derive(Clone, Copy)]
enum Align {
    Left,
    Right,
}

/// Fit `text` to exactly `width` columns, cutting it with an ellipsis when it is wider.
fn fit_text(text: &str, width: usize, align: Align) -> String {
    let cut = truncate_text(text, width);
    let fill = " ".repeat(width.saturating_sub(text_width(&cut)));
    match align {
        Align::Left => format!("{cut}{fill}"),
        Align::Right => format!("{fill}{cut}"),
    }
}

/// Shorter spellings of a value, tried in order when the value does not fit its columns.
fn compact_forms(field: Field, value: &str) -> Vec<String> {
    match field {
        Field::Cpu => cpu_forms(value),
        Field::Gpu => {
            let short = value
                .split_whitespace()
                .filter(|word| !matches!(*word, "Corporation" | "Inc." | "Series"))
                .collect::<Vec<_>>()
                .join(" ");
            if short == value {
                Vec::new()
            } else {
                vec![short]
            }
        }
        _ => Vec::new(),
    }
}

fn cpu_forms(value: &str) -> Vec<String> {
    let mut forms = Vec::new();
    let base = match value.rfind(" @ ") {
        Some(at) => {
            let base = value[..at].to_string();
            forms.push(base.clone());
            base
        }
        None => value.to_string(),
    };
    if let Some(bare) = strip_core_count(&base) {
        forms.push(bare.to_string());
    }
    forms
}

/// `text` without a trailing core count such as " (12)", if it has one.
fn strip_core_count(text: &str) -> Option<&str> {
    let body = text.strip_suffix(')')?;
    let open = body.rfind(" (")?;
    let digits = &body[open + 2..];
    (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())).then(|| &text[..open])
}

/// The value as it should be drawn in `width` columns: unchanged when it fits, else the first
/// shorter form that fits, else unchanged so that the caller cuts it.
fn fit_value(field: Field, value: &str, width: usize) -> String {
    if text_width(value) <= width {
        return value.to_string();
    }
    compact_forms(field, value)
        .into_iter()
        .find(|form| text_width(form) <= width)
        .unwrap_or_else(|| value.to_string())
}

fn value_text(entry: &Entry, text: &str, color: Color) -> Line {
    let text = Span::new(text, Style::new().fg(color));
    match entry.gauge {
        Some(ratio) => {
            let mut line = bar(ratio, ROW_BAR_CELLS, entry.field == Field::Battery);
            line.push(Span::plain(" "));
            line.push(text);
            line
        }
        None => Line::from_spans(vec![text]),
    }
}

/// One information row. The value is cut to `value_w` columns; the row is then exactly
/// `row_overhead(style, label_w, icon_width)` plus the value's width.
///
/// Mirrored rows (left panel) put the value first and the label last, so that the
/// label sits next to the logo.
pub fn info_row(
    entry: &Entry,
    style: InfoStyle,
    mirrored: bool,
    label_w: usize,
    value_w: usize,
    value_color: Color,
) -> Line {
    let icon = shown_icon(&entry.icon);
    let color = entry.color;
    let text = match entry.gauge {
        Some(_) => entry.value.clone(),
        None => fit_value(entry.field, &entry.value, value_w),
    };
    let value = value_text(entry, &text, value_color).truncated(value_w);
    let label_style = Style::new().fg(color).bold();
    let mut line = Line::new();
    match (style, mirrored) {
        (InfoStyle::Powerline, false) => {
            let lead = if icon.is_empty() {
                String::new()
            } else {
                format!("{icon} ")
            };
            let label = fit_text(&entry.label, label_w, Align::Left);
            let segment = Style::new().fg(color.contrast_text()).bg(color).bold();
            line.push(Span::new(format!(" {lead}{label} "), segment));
            line.push(Span::new(ARROW_RIGHT, Style::new().fg(color)));
            line.push(Span::plain(" "));
            line.append(value);
        }
        (InfoStyle::Powerline, true) => {
            let trail = if icon.is_empty() {
                String::new()
            } else {
                format!(" {icon}")
            };
            let label = fit_text(&entry.label, label_w, Align::Right);
            let segment = Style::new().fg(color.contrast_text()).bg(color).bold();
            line.append(value);
            line.push(Span::plain(" "));
            line.push(Span::new(ARROW_LEFT, Style::new().fg(color)));
            line.push(Span::new(format!(" {label}{trail} "), segment));
        }
        (InfoStyle::Plain, false) => {
            if !icon.is_empty() {
                line.push(Span::new(format!("{icon} "), Style::new().fg(color)));
            }
            let label = fit_text(&entry.label, label_w, Align::Left);
            line.push(Span::new(label, label_style));
            line.push(Span::plain("  "));
            line.append(value);
        }
        (InfoStyle::Plain, true) => {
            line.append(value);
            line.push(Span::plain("  "));
            let label = fit_text(&entry.label, label_w, Align::Right);
            line.push(Span::new(label, label_style));
            if !icon.is_empty() {
                line.push(Span::new(format!(" {icon}"), Style::new().fg(color)));
            }
        }
    }
    line
}

/// Clamp a ratio to 0..=1. NaN counts as empty.
fn unit_interval(value: f64) -> f64 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

/// A horizontal bar `width` cells wide. Filled cells are `█` in a color that tells how
/// full the bar is: green below 60 %, yellow below 85 %, red above. Empty cells are `░` in
/// a muted color. With `invert` (battery) the thresholds apply to the empty part, so a
/// low level is red.
pub fn bar(ratio: f64, width: usize, invert: bool) -> Line {
    let ratio = unit_interval(ratio);
    let level = if invert { 1.0 - ratio } else { ratio };
    let color = if level < 0.6 {
        GOOD
    } else if level < 0.85 {
        WARN
    } else {
        BAD
    };
    let filled = ((ratio * width as f64).round() as usize).min(width);
    Line::from_spans(vec![
        Span::new("█".repeat(filled), Style::new().fg(color)),
        Span::new("░".repeat(width - filled), Style::new().fg(MUTED)),
    ])
}

/// A strip of three-cell blocks, one per palette color (at most eight).
pub fn color_blocks(palette: &[Color]) -> Line {
    let mut line = Line::new();
    for color in palette.iter().take(MAX_COLOR_BLOCKS) {
        line.push(Span::new("███", Style::new().fg(*color)));
    }
    line
}

/// Gauge lines for every field that has a gauge: `"{label:<10} {bar} {pct:>3}%"`, with CPU
/// temperature shown in degrees instead. The bar takes all the room the width leaves
/// (at least 6 cells); callers that want a shorter bar pass a smaller width, see
/// [`GAUGE_LINE_MAX`]. Labels take the palette in turn, values the `value` color.
pub fn gauge_lines(info: &SysInfo, palette: &[Color], value: Color, width: usize) -> Vec<Line> {
    let bar_w = width.saturating_sub(GAUGE_OVERHEAD).max(GAUGE_BAR_MIN);
    let mut out = Vec::new();
    for field in GAUGES {
        let Some(ratio) = info.gauge(field) else {
            continue;
        };
        let ratio = unit_interval(ratio);
        let text = if field == Field::CpuTemp {
            let celsius = info.gauges.cpu_temp.unwrap_or(0.0).round() as i64;
            format!("{celsius:>2}°C")
        } else {
            let pct = (ratio * 100.0).round() as u32;
            format!("{pct:>3}%")
        };
        let label_style = Style::new().fg(palette_color(palette, out.len())).bold();
        let mut line = Line::new();
        line.push(Span::new(
            fit_text(field.label(), GAUGE_LABEL_W, Align::Left),
            label_style,
        ));
        line.push(Span::plain(" "));
        line.append(bar(ratio, bar_w, field == Field::Battery));
        line.push(Span::new(format!(" {text}"), Style::new().fg(value)));
        out.push(line.truncated(width));
    }
    out
}

/// The formatted title (`{user}` and `{host}` replaced), or `None` when the title is
/// disabled or formats to nothing.
pub fn title_text(ctx: &RenderCtx) -> Option<String> {
    let title = &ctx.cfg.title;
    if !title.enabled {
        return None;
    }
    let user = ctx.info.get(Field::User).unwrap_or("");
    let host = ctx.info.get(Field::Host).unwrap_or("");
    let text = title.format.replace("{user}", user).replace("{host}", host);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Title line plus a rule under it, both at most `ctx.width` columns.
pub fn title_lines(ctx: &RenderCtx) -> Vec<Line> {
    title_lines_within(ctx, ctx.width)
}

/// Title line and separator rule, left aligned and at most `max_width` columns. The rule
/// repeats the separator to the title's width and is omitted when the separator is empty.
pub fn title_lines_within(ctx: &RenderCtx, max_width: usize) -> Vec<Line> {
    let Some(text) = title_text(ctx) else {
        return Vec::new();
    };
    let title = truncate_text(&text, max_width);
    let title_w = text_width(&title);
    if title_w == 0 {
        return Vec::new();
    }
    let colors = &ctx.cfg.colors;
    let mut lines = vec![Line::from_spans(vec![Span::new(
        title,
        Style::new().fg(colors.title).bold(),
    )])];
    let rule = ctx.cfg.title.separator.as_str();
    if let Some(repeats) = title_w.checked_div(text_width(rule)) {
        let mut line = Line::from_spans(vec![Span::new(
            rule.repeat(repeats),
            Style::new().fg(colors.separator),
        )]);
        line.pad_to(title_w);
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::logo::LogoSet;

    fn context<'a>(info: &'a SysInfo, cfg: &'a Config, logos: &'a LogoSet) -> RenderCtx<'a> {
        RenderCtx {
            info,
            cfg,
            logos,
            width: 120,
        }
    }

    fn entry(icon: &str, label: &str, value: &str, gauge: Option<f64>) -> Entry {
        Entry {
            field: Field::Memory,
            icon: icon.to_string(),
            label: label.to_string(),
            value: value.to_string(),
            color: Color::new(1, 2, 3),
            gauge,
        }
    }

    fn fg_of(line: &Line) -> Option<Color> {
        line.spans.first().and_then(|span| span.style.fg)
    }

    #[test]
    fn info_row_width_is_overhead_plus_value() {
        let styles = [InfoStyle::Powerline, InfoStyle::Plain];
        let icons = ["", "x", "\u{f17c}", "ram"];
        let labels = ["Memory", "宽宽"];
        for style in styles {
            for mirrored in [false, true] {
                for icon in icons {
                    for label in labels {
                        for gauge in [None, Some(0.4)] {
                            for value_w in [0, 3, 40] {
                                let item = entry(icon, label, "6.21 GiB", gauge);
                                let label_w = 8;
                                let row = info_row(
                                    &item,
                                    style,
                                    mirrored,
                                    label_w,
                                    value_w,
                                    Color::WHITE,
                                );
                                let overhead = row_overhead(style, label_w, icon_width(icon));
                                let expected = overhead + value_width(&item).min(value_w);
                                assert_eq!(
                                    row.width(),
                                    expected,
                                    "{style:?} mirrored={mirrored} icon={icon:?} label={label:?} gauge={gauge:?} value_w={value_w}: {:?}",
                                    row.plain_text()
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn powerline_segments_carry_their_background() {
        let item = entry("x", "Mem", "1", None);
        let ansi = crate::render::to_ansi(&[info_row(
            &item,
            InfoStyle::Powerline,
            false,
            3,
            1,
            Color::WHITE,
        )]);
        assert!(ansi.contains("48;2;1;2;3"), "{ansi:?}");
        assert!(ansi.contains("\u{e0b0}"), "{ansi:?}");

        let mirrored = crate::render::to_ansi(&[info_row(
            &item,
            InfoStyle::Powerline,
            true,
            3,
            1,
            Color::WHITE,
        )]);
        assert!(mirrored.contains("\u{e0b2}"), "{mirrored:?}");
        assert!(mirrored.ends_with("\x1b[0m\n"), "{mirrored:?}");
    }

    #[test]
    fn overhead_matches_the_drawn_separators() {
        let item = entry("x", "Mem", "1", None);
        let row = info_row(&item, InfoStyle::Powerline, false, 3, 1, Color::WHITE);
        assert_eq!(row.plain_text(), " x Mem \u{e0b0} 1");
        assert_eq!(row_overhead(InfoStyle::Powerline, 3, 1), 9);
        let plain = info_row(&item, InfoStyle::Plain, false, 3, 1, Color::WHITE);
        assert_eq!(plain.plain_text(), "x Mem  1");
        assert_eq!(row_overhead(InfoStyle::Plain, 3, 1), 7);
    }

    #[test]
    fn empty_icon_takes_no_columns() {
        let item = entry("   ", "Mem", "1", None);
        assert_eq!(icon_width(&item.icon), 0);
        let row = info_row(&item, InfoStyle::Plain, false, 3, 5, Color::WHITE);
        assert_eq!(row.plain_text(), "Mem  1");
        assert_eq!(row_overhead(InfoStyle::Plain, 3, 0), 5);
    }

    #[test]
    fn cpu_value_loses_frequency_and_core_count_before_it_is_cut() {
        let value = "AMD Ryzen 5 5600GT (12) @ 3.59 GHz";
        assert_eq!(
            compact_forms(Field::Cpu, value),
            ["AMD Ryzen 5 5600GT (12)", "AMD Ryzen 5 5600GT"]
        );
        assert_eq!(fit_value(Field::Cpu, value, 60), value);
        assert_eq!(fit_value(Field::Cpu, value, 23), "AMD Ryzen 5 5600GT (12)");
        assert_eq!(fit_value(Field::Cpu, value, 18), "AMD Ryzen 5 5600GT");
        assert_eq!(fit_value(Field::Cpu, value, 10), value);

        let mut item = entry("", "CPU", value, None);
        item.field = Field::Cpu;
        let row = info_row(&item, InfoStyle::Plain, false, 3, 23, Color::WHITE);
        assert_eq!(row.plain_text(), "CPU  AMD Ryzen 5 5600GT (12)");
    }

    #[test]
    fn compact_forms_cover_only_cpu_and_gpu_values() {
        assert!(compact_forms(Field::Memory, "6.21 GiB @ 3.59 GHz (12)").is_empty());
        assert_eq!(
            compact_forms(Field::Gpu, "AMD Corporation Radeon Series"),
            ["AMD Radeon"]
        );
        assert!(compact_forms(Field::Gpu, "AMD Radeon").is_empty());
    }

    #[test]
    fn bar_fills_by_ratio_and_picks_thresholds() {
        assert_eq!(bar(0.3, 10, false).plain_text(), "███░░░░░░░");
        assert_eq!(fg_of(&bar(0.3, 10, false)), Some(GOOD));
        assert_eq!(fg_of(&bar(0.7, 10, false)), Some(WARN));
        assert_eq!(fg_of(&bar(0.9, 10, false)), Some(BAD));
        assert_eq!(fg_of(&bar(0.9, 10, true)), Some(GOOD));
        assert_eq!(fg_of(&bar(0.1, 10, true)), Some(BAD));
        assert_eq!(bar(1.5, 4, false).plain_text(), "████");
        assert_eq!(bar(-1.0, 4, false).plain_text(), "░░░░");
        assert_eq!(bar(f64::NAN, 4, false).plain_text(), "░░░░");
        assert_eq!(bar(0.5, 0, false).width(), 0);
    }

    #[test]
    fn color_blocks_take_three_cells_per_color_up_to_eight() {
        let palette = |count: u8| (0..count).map(|i| Color::new(i, 0, 0)).collect::<Vec<_>>();
        assert_eq!(color_blocks(&palette(3)).width(), 9);
        assert_eq!(color_blocks(&palette(10)).width(), 24);
        assert_eq!(color_blocks(&[]).width(), 0);
    }

    #[test]
    fn gauge_lines_cover_the_sample_and_fit_their_width() {
        let info = SysInfo::sample();
        let palette = [Color::new(1, 2, 3)];
        let value = Color::WHITE;
        assert_eq!(gauge_lines(&info, &palette, value, 60).len(), 9);
        for width in [4, 8, 20, 40, 60, 90] {
            for line in gauge_lines(&info, &palette, value, width) {
                assert!(
                    line.width() <= width,
                    "width {width}: {:?}",
                    line.plain_text()
                );
            }
        }
        assert!(gauge_lines(&SysInfo::default(), &palette, value, 60).is_empty());
    }

    #[test]
    fn gauge_bar_fills_the_given_width() {
        let info = SysInfo::sample();
        let palette = [Color::new(1, 2, 3)];
        // Label 10, space, bar, space, four value columns.
        let lines = gauge_lines(&info, &palette, Color::WHITE, 40);
        let first = lines[0].plain_text();
        assert!(first.starts_with("CPU Usage "), "{first}");
        assert_eq!(lines[0].width(), 40);
        assert_eq!(
            gauge_lines(&info, &palette, Color::WHITE, 90)[0].width(),
            90
        );
        assert_eq!(GAUGE_LINE_MAX, 46);
    }

    #[test]
    fn gauge_labels_cycle_the_palette_and_values_take_their_color() {
        let info = SysInfo::sample();
        let palette = [Color::new(1, 0, 0), Color::new(0, 1, 0)];
        let value = Color::new(9, 9, 9);
        let lines = gauge_lines(&info, &palette, value, 60);
        for (index, line) in lines.iter().enumerate() {
            assert_eq!(line.spans[0].style.fg, Some(palette[index % 2]));
            assert!(line.spans[0].style.bold);
            let last = line.spans.last();
            assert_eq!(last.and_then(|span| span.style.fg), Some(value));
        }
    }

    #[test]
    fn empty_bar_cells_are_muted() {
        let line = bar(0.3, 10, false);
        assert_eq!(line.spans[1].text, "░░░░░░░");
        assert_eq!(line.spans[1].style.fg, Some(MUTED));
        assert_eq!(bar(1.0, 4, false).spans.len(), 1);
    }

    #[test]
    fn title_rule_spans_the_title_width() {
        let info = SysInfo::sample();
        let mut cfg = Config::default();
        let logos = LogoSet::default();
        let lines = title_lines(&context(&info, &cfg, &logos));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].plain_text(), "atlas@workstation");
        assert_eq!(lines[1].plain_text(), "─".repeat(17));

        cfg.title.separator = String::new();
        assert_eq!(title_lines(&context(&info, &cfg, &logos)).len(), 1);

        cfg.title.enabled = false;
        assert!(title_lines(&context(&info, &cfg, &logos)).is_empty());
    }

    #[test]
    fn title_is_cut_to_the_requested_width() {
        let info = SysInfo::sample();
        let cfg = Config::default();
        let logos = LogoSet::default();
        let lines = title_lines_within(&context(&info, &cfg, &logos), 9);
        assert_eq!(lines[0].width(), 9);
        assert_eq!(lines[1].width(), 9);
    }

    #[test]
    fn colors_follow_the_configured_position_on_both_sides() {
        let info = SysInfo::sample();
        let cfg = Config::default();
        let logos = LogoSet::default();
        let ctx = context(&info, &cfg, &logos);
        let both = entries(&ctx, Side::Both);
        let left = entries(&ctx, Side::Left);
        let right = entries(&ctx, Side::Right);
        assert_eq!(both.len(), left.len() + right.len());
        assert_eq!(&both[..left.len()], &left[..]);
        assert_eq!(&both[left.len()..], &right[..]);
        let palette = &cfg.colors.palette;
        assert_eq!(both[0].color, palette[0]);
        assert_eq!(right[0].color, palette[left.len() % palette.len()]);
    }

    #[test]
    fn absent_values_are_hidden_or_marked() {
        let info = SysInfo::default();
        let mut cfg = Config::default();
        let logos = LogoSet::default();
        assert!(entries(&context(&info, &cfg, &logos), Side::Both).is_empty());

        cfg.layout.hide_empty = false;
        let shown = entries(&context(&info, &cfg, &logos), Side::Both);
        assert_eq!(shown.len(), 13);
        assert!(shown
            .iter()
            .all(|item| item.value == "n/a" && item.gauge.is_none()));
    }

    #[test]
    fn icons_are_drawn_unless_turned_off() {
        let info = SysInfo::sample();
        let mut cfg = Config::default();
        let logos = LogoSet::default();
        let icon = Field::Cpu.icon();
        let cpu_row = |cfg: &Config| {
            let shown = entries(&context(&info, cfg, &logos), Side::Right);
            let cpu = shown.iter().find(|item| item.field == Field::Cpu).unwrap();
            info_row(cpu, InfoStyle::Plain, false, 3, 40, Color::WHITE).plain_text()
        };
        assert!(cpu_row(&cfg).contains(icon));

        cfg.layout.icons = false;
        assert!(!cpu_row(&cfg).contains(icon));
    }

    #[test]
    fn gauge_is_only_drawn_for_bar_entries() {
        let info = SysInfo::sample();
        let mut cfg = Config::default();
        let logos = LogoSet::default();
        let plain = entries(&context(&info, &cfg, &logos), Side::Right);
        assert!(plain.iter().all(|item| item.gauge.is_none()));

        cfg.fields.right[3].bar = true;
        let barred = entries(&context(&info, &cfg, &logos), Side::Right);
        assert!(barred[3].gauge.is_some());
        assert_eq!(barred[3].field, Field::Memory);
    }
}
