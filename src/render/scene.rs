//! Scene layouts. Each scene turns the configuration and the system information into
//! styled lines that fit the requested width.

use crate::config::{Colors, Config, InfoStyle, Scene};
use crate::info::SysInfo;
use crate::logo::{self, Logo, LogoSet};
use crate::render::{text_width, truncate_text, Line, Span, Style};

use super::blocks::{self, Entry, Side};

/// Everything a scene needs to draw itself.
pub struct RenderCtx<'a> {
    pub info: &'a SysInfo,
    pub cfg: &'a Config,
    pub logos: &'a LogoSet,
    /// Columns available to the scene.
    pub width: usize,
}

/// Narrowest value column a classic panel may get before the scene falls back to stacking.
const CLASSIC_MIN_VALUE_W: usize = 10;
/// Narrowest information column beside the logo in the side scene.
const SIDE_MIN_INFO_W: usize = 30;
const DASH_BOX_GAP: usize = 1;
const DASH_MIN_SYSTEM_INNER: usize = 30;
/// Columns a logo needs beside the System box: its own box, the gap, and the System box
/// at its narrowest (inner width plus the two border columns).
const DASH_LOGO_RESERVE: usize = FRAME_CHROME + DASH_BOX_GAP + DASH_MIN_SYSTEM_INNER + 2;
/// The Resources box uses two columns when its inner width (between the borders) is at
/// least this.
const DASH_TWO_COLUMN_INNER: usize = 70;
const RESOURCE_COLUMN_GAP: usize = 3;
const SYSTEM_COLUMN_GAP: usize = 4;
/// Columns a box spends on its borders and the one space inside each border.
const FRAME_CHROME: usize = 4;
const DEFAULT_BOX_TITLE: &str = "AtlasFetch";
const SYSTEM_TITLE: &str = "System";
const RESOURCES_TITLE: &str = "Resources";

/// Lines of `scene` for the given context. Every line is at most `ctx.width` columns wide.
pub fn render(scene: Scene, ctx: &RenderCtx) -> Vec<Line> {
    if ctx.width == 0 {
        return Vec::new();
    }
    let lines = match scene {
        Scene::Classic => classic(ctx),
        Scene::Side => side(ctx),
        Scene::Dashboard => dashboard(ctx),
    };
    lines.iter().map(|line| line.truncated(ctx.width)).collect()
}

fn pick_logo<'a>(ctx: &RenderCtx<'a>, max_width: usize) -> Option<&'a Logo> {
    ctx.logos.fitting(max_width)
}

fn colored_logo(ctx: &RenderCtx, logo: &Logo) -> Vec<Line> {
    logo::colorize(logo, &ctx.cfg.colors.palette, ctx.cfg.logo.gradient)
}

/// The color rule row, when color blocks are enabled and there is a palette.
fn color_row(ctx: &RenderCtx) -> Option<Line> {
    let palette = &ctx.cfg.colors.palette;
    (ctx.cfg.layout.color_blocks && !palette.is_empty()).then(|| blocks::color_blocks(palette))
}

fn label_width(entries: &[Entry]) -> usize {
    entries
        .iter()
        .map(|entry| text_width(&entry.label))
        .max()
        .unwrap_or(0)
}

/// Value columns allowed by `room`, further limited by the configured maximum width.
fn value_limit(room: usize, max_value_width: usize) -> usize {
    if max_value_width > 0 {
        room.min(max_value_width)
    } else {
        room
    }
}

/// Value width of a classic panel: the widest value, limited by the room left after the
/// widest row overhead.
fn panel_value_width(
    entries: &[Entry],
    style: InfoStyle,
    label_w: usize,
    room: usize,
    max_value_width: usize,
) -> usize {
    if entries.is_empty() {
        return 0;
    }
    let overhead = entries
        .iter()
        .map(|entry| blocks::entry_overhead(entry, style, label_w))
        .max()
        .unwrap_or(0);
    let natural = entries.iter().map(blocks::value_width).max().unwrap_or(0);
    value_limit(natural.min(room.saturating_sub(overhead)), max_value_width)
}

/// Widest value of a classic panel at natural width, capped by the configured maximum.
fn natural_value_width(entries: &[Entry], max_value_width: usize) -> usize {
    let widest = entries.iter().map(blocks::value_width).max().unwrap_or(0);
    value_limit(widest, max_value_width)
}

/// Span of a classic panel at natural width: the widest overhead, the widest value and
/// the cascade. Zero for an empty panel. The gap to the logo is not included.
fn panel_natural_span(
    entries: &[Entry],
    style: InfoStyle,
    label_w: usize,
    max_value_width: usize,
    cascade: usize,
) -> usize {
    if entries.is_empty() {
        return 0;
    }
    let overhead = entries
        .iter()
        .map(|entry| blocks::entry_overhead(entry, style, label_w))
        .max()
        .unwrap_or(0);
    overhead + natural_value_width(entries, max_value_width) + cascade
}

/// Splits `avail` columns between two panels with natural spans `left` and `right`. When
/// both fit, each keeps its span. Otherwise each gets up to half, and a panel that needs
/// less than half passes the rest to the other one.
fn allocate_spans(left: usize, right: usize, avail: usize) -> (usize, usize) {
    if left + right <= avail {
        return (left, right);
    }
    let half = avail / 2;
    if left < half {
        (left, avail - left)
    } else if right < half {
        (avail - right, right)
    } else {
        (half, avail - half)
    }
}

/// True when a non-empty panel is cut below the narrowest readable value width.
fn squeezed(entries: &[Entry], value_w: usize, max_value_width: usize) -> bool {
    !entries.is_empty()
        && value_w < CLASSIC_MIN_VALUE_W.min(natural_value_width(entries, max_value_width))
}

/// Plain (non-mirrored) rows, each value cut to the room left after its own overhead.
fn fitted_rows(
    ctx: &RenderCtx,
    entries: &[Entry],
    style: InfoStyle,
    label_w: usize,
    avail: usize,
) -> Vec<Line> {
    entries
        .iter()
        .map(|entry| {
            let room = avail.saturating_sub(blocks::entry_overhead(entry, style, label_w));
            let value_w = value_limit(room, ctx.cfg.layout.max_value_width);
            blocks::info_row(entry, style, false, label_w, value_w, ctx.cfg.colors.value)
        })
        .collect()
}

/// Title, then the rows, then the color rule: the information column of the side scene.
fn info_block(ctx: &RenderCtx, entries: &[Entry], label_w: usize, avail: usize) -> Vec<Line> {
    let mut out = blocks::title_lines_within(ctx, avail);
    out.extend(fitted_rows(
        ctx,
        entries,
        ctx.cfg.layout.style,
        label_w,
        avail,
    ));
    if let Some(rule) = color_row(ctx) {
        out.push(Line::new());
        out.push(rule);
    }
    out
}

/// `line` preceded by `columns` unstyled spaces.
fn indent(columns: usize, line: Line) -> Line {
    let mut out = Line::new();
    out.pad(columns);
    out.append(line);
    out
}

fn centered(line: Line, width: usize) -> Line {
    let offset = width.saturating_sub(line.width()) / 2;
    indent(offset, line)
}

/// How far row `index` of a panel with `count` rows moves toward the logo. The outermost
/// rows move the full `cascade`, the middle row not at all.
fn inward(index: usize, count: usize, cascade: usize) -> usize {
    if count <= 1 {
        return 0;
    }
    let middle = (count - 1) as f64 / 2.0;
    let offset = (index as f64 / middle - 1.0).abs();
    (offset * cascade as f64).round() as usize
}

/// The index of `row` within a panel that starts at `top` and has `len` rows.
fn offset_index(row: usize, top: usize, len: usize) -> Option<usize> {
    row.checked_sub(top).filter(|index| *index < len)
}

fn classic(ctx: &RenderCtx) -> Vec<Line> {
    let width = ctx.width;
    let layout = &ctx.cfg.layout;
    let colors = &ctx.cfg.colors;
    let margin = layout.padding;
    let gap = layout.gap;
    let cascade = layout.cascade;
    let style = layout.style;
    let cap = layout.max_value_width;

    let left = blocks::entries(ctx, Side::Left);
    let right = blocks::entries(ctx, Side::Right);
    let logo = match pick_logo(ctx, width.saturating_sub(2 * margin)) {
        Some(logo) if !(left.is_empty() && right.is_empty()) => logo,
        _ => return stacked(ctx),
    };
    let left_label_w = label_width(&left);
    let right_label_w = label_width(&right);

    // The composition is always centered. Panels keep their natural span when everything
    // fits; otherwise they share the room beside the logo and their values are cut.
    let left_nat = panel_natural_span(&left, style, left_label_w, cap, cascade);
    let right_nat = panel_natural_span(&right, style, right_label_w, cap, cascade);
    let gaps = gap * (usize::from(!left.is_empty()) + usize::from(!right.is_empty()));
    let avail = width.saturating_sub(2 * margin + logo.width + gaps);
    let (left_span, right_span) = allocate_spans(left_nat, right_nat, avail);
    let left_block = if left.is_empty() { 0 } else { gap + left_span };
    let right_block = if right.is_empty() {
        0
    } else {
        gap + right_span
    };
    let total = left_block + logo.width + right_block;
    let start = width.saturating_sub(total) / 2;
    let logo_x = start + left_block;
    let left_value_w = panel_value_width(
        &left,
        style,
        left_label_w,
        left_span.saturating_sub(cascade),
        cap,
    );
    let right_value_w = panel_value_width(
        &right,
        style,
        right_label_w,
        right_span.saturating_sub(cascade),
        cap,
    );
    if squeezed(&left, left_value_w, cap) || squeezed(&right, right_value_w, cap) {
        return stacked(ctx);
    }
    let logo_end = logo_x + logo.width;

    let left_rows: Vec<Line> = left
        .iter()
        .map(|entry| blocks::info_row(entry, style, true, left_label_w, left_value_w, colors.value))
        .collect();
    let right_rows: Vec<Line> = right
        .iter()
        .map(|entry| {
            blocks::info_row(
                entry,
                style,
                false,
                right_label_w,
                right_value_w,
                colors.value,
            )
        })
        .collect();

    let art = colored_logo(ctx, logo);
    let height = logo.height();
    let total = height.max(left_rows.len()).max(right_rows.len());
    let logo_top = (total - height) / 2;
    let left_top = (total - left_rows.len()) / 2;
    let right_top = (total - right_rows.len()) / 2;

    let title = blocks::title_lines(ctx);
    let title_w = title.iter().map(Line::width).max().unwrap_or(0);
    let title_x = (logo_x + logo.width / 2)
        .saturating_sub(title_w / 2)
        .min(width.saturating_sub(title_w));
    let mut out: Vec<Line> = title
        .into_iter()
        .map(|line| indent(title_x, line))
        .collect();
    if !out.is_empty() {
        out.push(Line::new());
    }
    for row in 0..total {
        let mut line = Line::new();
        if let Some(index) = offset_index(row, left_top, left_rows.len()) {
            let end = (logo_x + inward(index, left_rows.len(), cascade))
                .saturating_sub(gap)
                .min(logo_x);
            let text = left_rows[index].truncated(end);
            line.pad(end - text.width());
            line.append(text);
        }
        line.pad_to(logo_x);
        match row.checked_sub(logo_top).and_then(|i| art.get(i)) {
            Some(art_line) => line.append(art_line.clone()),
            None => line.pad(logo.width),
        }
        if let Some(index) = offset_index(row, right_top, right_rows.len()) {
            let start = (logo_end + gap)
                .saturating_sub(inward(index, right_rows.len(), cascade))
                .max(logo_end);
            line.pad_to(start);
            line.append(right_rows[index].truncated(width.saturating_sub(start)));
        }
        out.push(line);
    }

    if let Some(rule) = color_row(ctx) {
        out.push(Line::new());
        out.push(centered(rule, width));
    }
    out
}

/// Classic fallback: logo, title and every row in one centered column.
fn stacked(ctx: &RenderCtx) -> Vec<Line> {
    let width = ctx.width;
    let layout = &ctx.cfg.layout;
    let inner = width.saturating_sub(2 * layout.padding);
    let mut out = Vec::new();
    if let Some(logo) = pick_logo(ctx, inner) {
        out.extend(
            colored_logo(ctx, logo)
                .into_iter()
                .map(|line| centered(line, width)),
        );
        out.push(Line::new());
    }
    out.extend(
        blocks::title_lines(ctx)
            .into_iter()
            .map(|line| centered(line, width)),
    );
    let all = blocks::entries(ctx, Side::Both);
    let rows = fitted_rows(ctx, &all, layout.style, label_width(&all), inner);
    let block_w = rows.iter().map(Line::width).max().unwrap_or(0);
    let block_x = width.saturating_sub(block_w) / 2;
    out.extend(rows.into_iter().map(|row| indent(block_x, row)));
    if let Some(rule) = color_row(ctx) {
        out.push(Line::new());
        out.push(centered(rule, width));
    }
    out
}

fn side(ctx: &RenderCtx) -> Vec<Line> {
    let width = ctx.width;
    let margin = ctx.cfg.layout.padding;
    let gap = ctx.cfg.layout.gap;
    let all = blocks::entries(ctx, Side::Both);
    let label_w = label_width(&all);

    if let Some(logo) = pick_logo(ctx, width.saturating_sub(margin + gap + SIDE_MIN_INFO_W)) {
        let info_x = margin + logo.width + gap;
        let info = info_block(ctx, &all, label_w, width.saturating_sub(info_x));
        let total = logo.height().max(info.len());
        let mut art_lines = colored_logo(ctx, logo).into_iter();
        let mut info_lines = info.into_iter();
        let mut out = Vec::with_capacity(total);
        for _ in 0..total {
            let mut line = Line::new();
            line.pad(margin);
            match art_lines.next() {
                Some(art_line) => line.append(art_line),
                None => line.pad(logo.width),
            }
            line.pad_to(info_x);
            if let Some(text) = info_lines.next() {
                line.append(text);
            }
            out.push(line);
        }
        return out;
    }

    let mut out = Vec::new();
    if let Some(logo) = pick_logo(ctx, width.saturating_sub(2 * margin)) {
        out.extend(
            colored_logo(ctx, logo)
                .into_iter()
                .map(|line| indent(margin, line)),
        );
        out.push(Line::new());
    }
    let info = info_block(ctx, &all, label_w, width.saturating_sub(margin));
    out.extend(info.into_iter().map(|line| indent(margin, line)));
    out
}

fn dashboard(ctx: &RenderCtx) -> Vec<Line> {
    let margin = ctx.cfg.layout.padding;
    let avail = ctx.width.saturating_sub(2 * margin);
    if avail < FRAME_CHROME {
        return Vec::new();
    }
    let title = blocks::title_text(ctx).unwrap_or_else(|| DEFAULT_BOX_TITLE.to_string());
    let mut out = match pick_logo(ctx, avail.saturating_sub(DASH_LOGO_RESERVE)) {
        Some(logo) => logo_and_system(ctx, logo, &title, avail),
        None => stacked_boxes(ctx, &title, avail),
    };
    out.extend(resources_box(ctx, avail));
    out.into_iter().map(|line| indent(margin, line)).collect()
}

/// Logo box and System box side by side, both with the same inner height.
fn logo_and_system(ctx: &RenderCtx, logo: &Logo, title: &str, avail: usize) -> Vec<Line> {
    let colors = &ctx.cfg.colors;
    let logo_box_w = logo.width + FRAME_CHROME;
    let system_w = avail.saturating_sub(logo_box_w + DASH_BOX_GAP);
    let mut system = system_body(ctx, system_w.saturating_sub(FRAME_CHROME));
    let inner_h = logo.height().max(system.len());

    let mut logo_body = vec![Line::new(); (inner_h - logo.height()) / 2];
    logo_body.extend(colored_logo(ctx, logo));
    logo_body.resize(inner_h, Line::new());
    system.resize(inner_h, Line::new());

    let logo_box = frame(title, logo_box_w, &logo_body, colors);
    let system_box = frame(SYSTEM_TITLE, system_w, &system, colors);
    logo_box
        .into_iter()
        .zip(system_box)
        .map(|(mut left, right)| {
            left.pad(DASH_BOX_GAP);
            left.append(right);
            left
        })
        .collect()
}

/// Logo box across the full width (when the logo fits), then the System box below it.
fn stacked_boxes(ctx: &RenderCtx, title: &str, avail: usize) -> Vec<Line> {
    let colors = &ctx.cfg.colors;
    let content = avail.saturating_sub(FRAME_CHROME);
    let mut out = Vec::new();
    if let Some(logo) = pick_logo(ctx, content) {
        let offset = content.saturating_sub(logo.width) / 2;
        let body: Vec<Line> = colored_logo(ctx, logo)
            .into_iter()
            .map(|line| indent(offset, line))
            .collect();
        out.extend(frame(title, avail, &body, colors));
    }
    out.extend(frame(
        SYSTEM_TITLE,
        avail,
        &system_body(ctx, content),
        colors,
    ));
    out
}

/// Rows of the System box, then the color rule. The rows are one column, or two when the
/// content holds the widest row twice over. The rule is indented to the labels.
fn system_body(ctx: &RenderCtx, content: usize) -> Vec<Line> {
    let all = blocks::entries(ctx, Side::Both);
    let label_w = label_width(&all);
    let widest = all
        .iter()
        .map(|entry| {
            blocks::entry_overhead(entry, InfoStyle::Plain, label_w) + blocks::value_width(entry)
        })
        .max()
        .unwrap_or(0);
    let mut out = if content >= 2 * widest + SYSTEM_COLUMN_GAP {
        two_column_rows(ctx, &all, content)
    } else {
        fitted_rows(ctx, &all, InfoStyle::Plain, label_w, content)
    };
    if let Some(rule) = color_row(ctx) {
        let offset = all.iter().map(blocks::label_offset).max().unwrap_or(0);
        out.push(Line::new());
        out.push(indent(offset, rule));
    }
    out
}

/// The first half of the entries on the left, the rest on the right. Each column has its
/// own label width and every value is cut to its column.
fn two_column_rows(ctx: &RenderCtx, entries: &[Entry], content: usize) -> Vec<Line> {
    let (left, right) = entries.split_at(entries.len().div_ceil(2));
    let column = content.saturating_sub(SYSTEM_COLUMN_GAP) / 2;
    let left_rows = fitted_rows(ctx, left, InfoStyle::Plain, label_width(left), column);
    let right_rows = fitted_rows(ctx, right, InfoStyle::Plain, label_width(right), column);
    let total = left_rows.len().max(right_rows.len());
    let mut left_rows = left_rows.into_iter();
    let mut right_rows = right_rows.into_iter();
    (0..total)
        .map(|_| {
            let mut line = Line::new();
            if let Some(text) = left_rows.next() {
                line.append(text);
            }
            line.pad_to(column + SYSTEM_COLUMN_GAP);
            if let Some(text) = right_rows.next() {
                line.append(text);
            }
            line
        })
        .collect()
}

/// The Resources box with the gauges, in one column or two. `None` when there are no gauges.
/// In two columns each bar fills its column, so the right column ends at the box edge.
fn resources_box(ctx: &RenderCtx, avail: usize) -> Vec<Line> {
    let palette = &ctx.cfg.colors.palette;
    let value = ctx.cfg.colors.value;
    let content = avail.saturating_sub(FRAME_CHROME);
    let body: Vec<Line> = if avail.saturating_sub(2) >= DASH_TWO_COLUMN_INNER {
        let column = content.saturating_sub(RESOURCE_COLUMN_GAP) / 2;
        blocks::gauge_lines(ctx.info, palette, value, column)
            .chunks(2)
            .map(|pair| {
                let mut row = Line::new();
                for (index, gauge) in pair.iter().enumerate() {
                    if index == 1 {
                        row.pad_to(column + RESOURCE_COLUMN_GAP);
                    }
                    row.append(gauge.clone());
                }
                row
            })
            .collect()
    } else {
        blocks::gauge_lines(
            ctx.info,
            palette,
            value,
            content.min(blocks::GAUGE_LINE_MAX),
        )
    };
    if body.is_empty() {
        return Vec::new();
    }
    frame(RESOURCES_TITLE, avail, &body, &ctx.cfg.colors)
}

/// A rounded box `total_w` columns wide around `body`. The body is cut or padded to the
/// content width, which is `total_w` minus the borders and one space on each side.
fn frame(title: &str, total_w: usize, body: &[Line], colors: &Colors) -> Vec<Line> {
    let border = Style::new().fg(colors.separator);
    let content = total_w.saturating_sub(FRAME_CHROME);
    let mut out = Vec::with_capacity(body.len() + 2);
    out.push(frame_top(title, total_w, colors));
    for line in body {
        let mut inner = line.truncated(content);
        inner.pad_to(content);
        let mut row = Line::new();
        row.push(Span::new("│ ", border));
        row.append(inner);
        row.push(Span::new(" │", border));
        out.push(row);
    }
    let bottom = format!("╰{}╯", "─".repeat(total_w.saturating_sub(2)));
    out.push(Line::from_spans(vec![Span::new(bottom, border)]));
    out
}

/// Top edge of a box: `╭─ Title ───╮`, or a plain edge when there is no title that fits.
fn frame_top(title: &str, total_w: usize, colors: &Colors) -> Line {
    let border = Style::new().fg(colors.separator);
    let title = truncate_text(title, total_w.saturating_sub(6));
    if title.is_empty() {
        let plain = format!("╭{}╮", "─".repeat(total_w.saturating_sub(2)));
        return Line::from_spans(vec![Span::new(plain, border)]);
    }
    let fill = total_w.saturating_sub(text_width(&title) + 5);
    let mut line = Line::new();
    line.push(Span::new("╭─ ", border));
    line.push(Span::new(title, Style::new().fg(colors.title).bold()));
    line.push(Span::new(format!(" {}╮", "─".repeat(fill)), border));
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::Field;
    use crate::logo::LogoSource;
    use crate::render::to_plain;

    const WIDTHS: [usize; 7] = [24, 40, 60, 80, 100, 140, 200];
    const LEFT_LABELS: [&str; 7] = ["OS", "Kernel", "Packages", "Shell", "Terminal", "DE", "WM"];
    const RIGHT_LABELS: [&str; 6] = ["Uptime", "CPU", "GPU", "Memory", "Disk", "Battery"];

    /// A small logo drawn with `#`, plus a compact variant.
    fn test_logos() -> LogoSet {
        LogoSet {
            full: Logo::from_text("  ##\n ####\n######\n  ##"),
            small: Logo::from_text("##\n##"),
        }
    }

    fn render_plain(
        scene: Scene,
        cfg: &Config,
        info: &SysInfo,
        logos: &LogoSet,
        width: usize,
    ) -> String {
        let ctx = RenderCtx {
            info,
            cfg,
            logos,
            width,
        };
        to_plain(&render(scene, &ctx))
    }

    fn has_any(text: &str, labels: &[&str]) -> bool {
        labels.iter().any(|label| text.contains(label))
    }

    #[test]
    fn every_scene_fits_every_width() {
        let info = SysInfo::sample();
        for scene in Scene::ALL {
            for style in [InfoStyle::Powerline, InfoStyle::Plain] {
                for logos in [test_logos(), LogoSet::default()] {
                    let mut cfg = Config::default();
                    cfg.layout.style = style;
                    for width in WIDTHS {
                        let ctx = RenderCtx {
                            info: &info,
                            cfg: &cfg,
                            logos: &logos,
                            width,
                        };
                        let lines = render(scene, &ctx);
                        assert!(
                            !lines.is_empty(),
                            "{scene:?} {style:?} width {width}: no output"
                        );
                        for line in &lines {
                            assert!(
                                line.width() <= width,
                                "{scene:?} {style:?} width {width}: {:?}",
                                line.plain_text()
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn classic_places_the_logo_between_two_panels() {
        let info = SysInfo::sample();
        let mut cfg = Config::default();
        cfg.layout.cascade = 2;
        let text = render_plain(Scene::Classic, &cfg, &info, &test_logos(), 140);
        let joined = text.lines().any(|line| {
            let Some(glyph) = line.find('#') else {
                return false;
            };
            has_any(&line[..glyph], &LEFT_LABELS) && has_any(&line[glyph..], &RIGHT_LABELS)
        });
        assert!(joined, "{text}");
    }

    #[test]
    fn classic_centers_the_composition_without_cutting_values() {
        let mut info = SysInfo::sample();
        info.set(Field::Os, "Ubuntu 24.04.5 LTS");
        let mut cfg = Config::default();
        cfg.layout.cascade = 2;
        cfg.fields
            .right
            .retain(|entry| entry.field == Field::Uptime);
        let bar = "#".repeat(50);
        let logos = LogoSet {
            full: Logo::from_text(&format!("{bar}\n{bar}\n{bar}")),
            small: Logo::from_text("##\n##"),
        };
        let text = render_plain(Scene::Classic, &cfg, &info, &logos, 120);
        assert!(text.contains("Ubuntu 24.04.5 LTS"), "{text}");
    }

    #[test]
    fn classic_stays_joined_and_centered_when_panels_shrink() {
        let info = SysInfo::sample();
        let mut cfg = Config::default();
        cfg.layout.cascade = 2;
        let bar = "#".repeat(50);
        let logos = LogoSet {
            full: Logo::from_text(&format!("{bar}\n{bar}\n{bar}")),
            small: Logo::from_text("##\n##"),
        };
        let width = 120;
        let text = render_plain(Scene::Classic, &cfg, &info, &logos, width);
        let lines: Vec<&str> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("OS") && line.contains("Uptime")),
            "{text}"
        );
        let left_margin = lines
            .iter()
            .map(|line| text_width(line) - text_width(line.trim_start()))
            .min()
            .unwrap_or(0);
        let right_margin = width
            - lines
                .iter()
                .map(|line| text_width(line.trim_end()))
                .max()
                .unwrap_or(0);
        assert!(
            left_margin.abs_diff(right_margin) <= 1,
            "margins {left_margin} and {right_margin}\n{text}"
        );
    }

    #[test]
    fn allocate_spans_keeps_natural_spans_when_they_fit() {
        assert_eq!(allocate_spans(20, 30, 60), (20, 30));
        assert_eq!(allocate_spans(30, 30, 60), (30, 30));
    }

    #[test]
    fn allocate_spans_passes_the_rest_to_the_larger_panel() {
        assert_eq!(allocate_spans(10, 80, 60), (10, 50));
        assert_eq!(allocate_spans(80, 10, 60), (50, 10));
    }

    #[test]
    fn allocate_spans_splits_evenly_when_both_need_more_than_half() {
        assert_eq!(allocate_spans(45, 42, 60), (30, 30));
        assert_eq!(allocate_spans(40, 41, 61), (30, 31));
    }

    #[test]
    fn classic_stacks_when_narrow() {
        let info = SysInfo::sample();
        let cfg = Config::default();
        let text = render_plain(Scene::Classic, &cfg, &info, &test_logos(), 50);
        let lines: Vec<&str> = text.lines().collect();
        assert!(
            lines
                .iter()
                .all(|line| !(has_any(line, &LEFT_LABELS) && has_any(line, &RIGHT_LABELS))),
            "{text}"
        );
        let logo_row = lines.iter().rposition(|line| line.contains('#'));
        let label_row = lines.iter().position(|line| line.contains("OS"));
        assert!(
            matches!((logo_row, label_row), (Some(logo), Some(label)) if label > logo),
            "{text}"
        );
    }

    #[test]
    fn side_puts_the_title_right_of_the_logo() {
        let info = SysInfo::sample();
        let cfg = Config::default();
        let text = render_plain(Scene::Side, &cfg, &info, &test_logos(), 100);
        let first = text.lines().next().unwrap_or_default();
        let column = first
            .find("atlas@workstation")
            .map(|byte| text_width(&first[..byte]));
        assert!(matches!(column, Some(col) if col > 8), "{text}");
    }

    #[test]
    fn dashboard_has_system_and_resources_boxes() {
        let info = SysInfo::sample();
        let cfg = Config::default();
        for width in [100, 140] {
            let text = render_plain(Scene::Dashboard, &cfg, &info, &test_logos(), width);
            assert!(text.contains("System"), "{text}");
            assert!(text.contains("Resources"), "{text}");
            assert!(text.contains('╭') && text.contains('╰'), "{text}");
        }
    }

    #[test]
    fn dashboard_without_gauges_has_no_resources_box() {
        let info = SysInfo::default();
        let cfg = Config::default();
        let text = render_plain(Scene::Dashboard, &cfg, &info, &test_logos(), 100);
        assert!(text.contains("System"), "{text}");
        assert!(!text.contains("Resources"), "{text}");
    }

    #[test]
    fn dashboard_frame_lines_are_aligned() {
        let info = SysInfo::sample();
        let cfg = Config::default();
        let logos = test_logos();
        for width in [100, 140, 200] {
            let ctx = RenderCtx {
                info: &info,
                cfg: &cfg,
                logos: &logos,
                width,
            };
            let lines = render(Scene::Dashboard, &ctx);
            let mut boxes = 0;
            for line in &lines {
                let plain = line.plain_text();
                let edge = plain.trim_start().chars().next();
                if matches!(edge, Some('│' | '╭' | '╰')) {
                    boxes += 1;
                    // Boxes start after the left padding and end at the right padding.
                    assert_eq!(line.width(), width - cfg.layout.padding, "{plain:?}");
                }
            }
            assert!(boxes > 0);
        }
    }

    #[test]
    fn dashboard_system_box_uses_two_columns_only_when_wide() {
        let info = SysInfo::sample();
        let cfg = Config::default();
        let logos = test_logos();
        let wide = render_plain(Scene::Dashboard, &cfg, &info, &logos, 200);
        // The first half of the entries (OS to WM) shares rows with the rest (Uptime on).
        assert!(
            wide.lines()
                .any(|line| line.contains("OS") && line.contains("Uptime")),
            "{wide}"
        );
        let narrow = render_plain(Scene::Dashboard, &cfg, &info, &logos, 100);
        assert!(
            !narrow
                .lines()
                .any(|line| line.contains("OS") && line.contains("Uptime")),
            "{narrow}"
        );
    }

    #[test]
    #[ignore = "prints the scene renders for visual review"]
    fn print_scene_renders() {
        let info = SysInfo::sample();
        let cfg = Config::default();
        let logos = logo::resolve(&LogoSource::Auto, &info.os_ids, None, true);
        for width in [140, 60] {
            for scene in Scene::ALL {
                println!("===== {} at {width} =====", scene.label());
                print!("{}", render_plain(scene, &cfg, &info, &logos, width));
            }
        }
    }
}
