pub mod ascii;
pub mod companion;
pub mod monitor;
pub mod system;

use serde::{Deserialize, Serialize};
use std::any::Any;
use std::fmt;
use std::str::FromStr;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::config::Config;
use crate::info::SysInfo;
use crate::theme::Color;

pub trait Component: Send + Sync {
    fn name(&self) -> &str;
    #[allow(dead_code)]
    fn render_ansi(&self, ctx: &RenderCtx) -> String;
    fn render_styled(&self, ctx: &RenderCtx) -> Vec<Vec<StyledSpan>>;
    #[allow(dead_code)]
    fn min_width(&self) -> usize;
    #[allow(dead_code)]
    fn min_height(&self) -> usize;
    fn as_any(&self) -> &dyn Any;
}

pub struct RenderCtx<'a> {
    pub info: &'a SysInfo,
    pub cfg: &'a Config,
    pub term_width: usize,
    pub palette: &'a [Color],
}

#[derive(Debug, Clone)]
pub struct StyledSpan {
    pub text: String,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
}

impl StyledSpan {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            fg: None,
            bg: None,
            bold: false,
        }
    }
    pub fn fg(mut self, c: Color) -> Self {
        self.fg = Some(c);
        self
    }
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scene {
    #[default]
    Classic,
    Dashboard,
    Cockpit,
    #[serde(alias = "classic_fetch", alias = "classic-fetch")]
    ClassicFetch,
}

#[allow(dead_code)]
impl Scene {
    pub fn all() -> &'static [Scene] {
        &[
            Scene::Classic,
            Scene::Dashboard,
            Scene::Cockpit,
            Scene::ClassicFetch,
        ]
    }
    pub fn name(&self) -> &'static str {
        match self {
            Scene::Classic => "Classic",
            Scene::Dashboard => "Terminal Dashboard",
            Scene::Cockpit => "Cockpit",
            Scene::ClassicFetch => "ClassicFetch",
        }
    }
    pub fn key(&self) -> &'static str {
        match self {
            Scene::Classic => "classic",
            Scene::Dashboard => "dashboard",
            Scene::Cockpit => "cockpit",
            Scene::ClassicFetch => "classicfetch",
        }
    }
    pub fn description(&self) -> &'static str {
        match self {
            Scene::Classic => "ASCII centered, left/right powerline panels",
            Scene::Dashboard => "Multi-panel TUI dashboard layout",
            Scene::Cockpit => "ASCII centered, panels arranged around it",
            Scene::ClassicFetch => "Fastfetch-style: logo left, information right, no panels",
        }
    }
    pub fn min_width(&self) -> usize {
        match self {
            Scene::Classic => 80,
            Scene::Dashboard => 80,
            Scene::Cockpit => 60,
            Scene::ClassicFetch => 70,
        }
    }
}

impl FromStr for Scene {
    type Err = SceneParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "classic" => Ok(Self::Classic),
            "dashboard" => Ok(Self::Dashboard),
            "cockpit" => Ok(Self::Cockpit),
            "classicfetch" | "classic_fetch" | "classic-fetch" => Ok(Self::ClassicFetch),
            _ => Err(SceneParseError(value.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneParseError(String);

impl fmt::Display for SceneParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown scene '{}'; available scenes: classic, dashboard, cockpit, classicfetch",
            self.0
        )
    }
}

impl std::error::Error for SceneParseError {}

pub struct SceneOutput {
    pub lines: Vec<Vec<StyledSpan>>,
}

pub fn render_scene_ansi(scene: Scene, components: &[&dyn Component], ctx: &RenderCtx) -> String {
    let output = render_scene(scene, components, ctx);
    spans_to_ansi(&output.lines)
}

fn spans_to_ansi(lines: &[Vec<StyledSpan>]) -> String {
    let mut out = String::new();
    for line in lines {
        for span in line {
            if span.bold {
                out.push_str("\x1b[1m");
            }
            if let Some(fg) = &span.fg {
                out.push_str(&fg.fg_escape());
            }
            if let Some(bg) = &span.bg {
                out.push_str(&bg.bg_escape());
            }
            out.push_str(&span.text);
            out.push_str("\x1b[0m");
        }
        out.push('\n');
    }
    out
}

pub fn render_scene(scene: Scene, components: &[&dyn Component], ctx: &RenderCtx) -> SceneOutput {
    match scene {
        Scene::Classic => render_classic(components, ctx),
        Scene::Dashboard => render_dashboard(components, ctx),
        Scene::Cockpit => render_cockpit(components, ctx),
        Scene::ClassicFetch => render_classicfetch(components, ctx),
    }
}

/// Convert styled scene output to plain text for tests and non-ANSI consumers.
#[cfg(test)]
pub fn scene_to_plain(output: &SceneOutput) -> String {
    let mut text = String::new();
    for line in &output.lines {
        for span in line {
            text.push_str(&span.text);
        }
        text.push('\n');
    }
    text
}

fn find_component<'a>(components: &[&'a dyn Component], name: &str) -> Option<&'a dyn Component> {
    components.iter().find(|c| c.name() == name).copied()
}

fn wrap_block(title: &str, lines: &[Vec<StyledSpan>], term_w: usize) -> Vec<Vec<StyledSpan>> {
    let mut out = Vec::new();
    if !title.is_empty() {
        let sep = "\u{2500}";
        let t = format!(" {} ", title);
        let side = (term_w.saturating_sub(t.width())) / 2;
        let header = format!("{}{}{}", sep.repeat(side), t, sep.repeat(side));
        out.push(vec![StyledSpan::new(header)]);
    }
    for line in lines {
        let mut padded = line.to_vec();
        let w: usize = line.iter().map(|s| s.text.width()).sum();
        if w < term_w {
            padded.push(StyledSpan::new(" ".repeat(term_w - w)));
        }
        out.push(padded);
    }
    out
}

fn cascade_offset(i: usize, total: usize, max_shift: usize) -> usize {
    if total <= 1 {
        return 0;
    }
    let mid = (total - 1) as f64 / 2.0;
    if mid <= 0.0 {
        return 0;
    }
    let rel = (i as f64 / mid - 1.0).abs();
    (rel * max_shift as f64).round() as usize
}

fn render_classic(components: &[&dyn Component], ctx: &RenderCtx) -> SceneOutput {
    let ascii = find_component(components, "ascii");
    let system = find_component(components, "system");
    let monitor = find_component(components, "monitor");

    let (ascii_lines, ascii_w) = ascii
        .and_then(|c| c.as_any().downcast_ref::<ascii::AsciiComponent>())
        .map(|c| c.render_colored_lines(ctx))
        .unwrap_or_default();

    let sys = system.and_then(|c| c.as_any().downcast_ref::<system::SystemComponent>());

    let left_pad = ctx.cfg.panel.left_pad;
    let right_pad = ctx.cfg.panel.right_pad;
    let gap = ctx.cfg.panel.gap.max(1);
    let max_shift = ctx.cfg.panel.max_shift;

    let logo_origin = if ascii_w > 0 && ascii_w < ctx.term_width {
        (ctx.term_width.saturating_sub(ascii_w)) / 2
    } else {
        0
    };

    // Available width for left/right panels (accounting for padding, gap, shift, and ASCII block)
    let left_avail = logo_origin
        .saturating_sub(left_pad + max_shift + gap + 1)
        .max(4);
    let right_avail = ctx
        .term_width
        .saturating_sub(logo_origin + ascii_w + gap + right_pad + max_shift + 1)
        .max(4);

    let left_lines = sys
        .map(|s| s.render_left_styled(ctx, left_avail))
        .unwrap_or_default();
    let right_lines = sys
        .map(|s| s.render_right_styled(ctx, right_avail))
        .unwrap_or_default();
    let mon_lines = monitor.map(|c| c.render_styled(ctx)).unwrap_or_default();

    let left_w = left_lines
        .iter()
        .map(|row| row.iter().map(|s| s.text.width()).sum::<usize>())
        .max()
        .unwrap_or(0);
    let right_w = right_lines
        .iter()
        .map(|row| row.iter().map(|s| s.text.width()).sum::<usize>())
        .max()
        .unwrap_or(0);

    let lh = ascii_lines.len();
    let n = left_lines.len().max(right_lines.len()).max(mon_lines.len());
    let start_row = if lh > 0 { lh.saturating_sub(n) / 2 } else { 0 };
    let total_rows = if lh == 0 { n } else { lh.max(start_row + n) };

    let mut all = Vec::new();

    // ── Title ──
    let title_color =
        Color::from_hex_opt(&ctx.cfg.title.color).unwrap_or(Color::new(255, 154, 152));
    let title_text = ctx
        .cfg
        .title
        .format
        .replace("{user}", &ctx.info.user)
        .replace("{host}", &ctx.info.host);
    if !title_text.is_empty() {
        let mut title_line = vec![
            StyledSpan::new("  "),
            StyledSpan::new(title_text).fg(title_color).bold(),
        ];
        let tw: usize = title_line.iter().map(|s| s.text.width()).sum();
        if tw < ctx.term_width {
            title_line.push(StyledSpan::new(" ".repeat(ctx.term_width - tw)));
        }
        all.push(title_line);
    }

    // ── Separator ──
    let sep_color =
        Color::from_hex_opt(&ctx.cfg.separator.color).unwrap_or(Color::new(157, 133, 255));
    let sep_len = ctx
        .cfg
        .separator
        .length
        .min(ctx.term_width.saturating_sub(4));
    if sep_len > 0 {
        let sep_str: String = ctx.cfg.separator.char.repeat(sep_len);
        let mut sep_line = vec![
            StyledSpan::new("  "),
            StyledSpan::new(sep_str).fg(sep_color),
        ];
        let sw: usize = sep_line.iter().map(|s| s.text.width()).sum();
        if sw < ctx.term_width {
            sep_line.push(StyledSpan::new(" ".repeat(ctx.term_width - sw)));
        }
        all.push(sep_line);
    }

    for i in 0..total_rows {
        let in_range = lh == 0 || (i >= start_row && i < start_row + n);
        let row_idx = if lh > 0 {
            i.saturating_sub(start_row)
        } else {
            i
        };
        let s = cascade_offset(row_idx, n, max_shift);

        let mut line = Vec::new();

        if in_range {
            // ── Left padding with cascade shift ──
            line.push(StyledSpan::new(" ".repeat(left_pad + s)));

            // ── Left panel (right-aligned within its block) ──
            if row_idx < left_lines.len() {
                let cw: usize = left_lines[row_idx].iter().map(|s| s.text.width()).sum();
                if cw < left_w {
                    line.push(StyledSpan::new(" ".repeat(left_w - cw)));
                }
                line.extend(left_lines[row_idx].clone());
            } else {
                line.push(StyledSpan::new(" ".repeat(left_w.max(1))));
            }

            // ── Gap before ASCII ──
            let cur: usize = line.iter().map(|s| s.text.width()).sum();
            let target = logo_origin.saturating_sub(gap);
            if target > cur {
                line.push(StyledSpan::new(" ".repeat(target - cur)));
            }
            if ascii_w > 0 {
                line.push(StyledSpan::new(" ".repeat(gap)));
            }

            // ── ASCII ──
            if i < ascii_lines.len() {
                line.extend(ascii_lines[i].clone());
            } else if ascii_w > 0 {
                line.push(StyledSpan::new(" ".repeat(ascii_w)));
            }

            // ── Right panel (left-aligned) ──
            if row_idx < right_lines.len() {
                let cur: usize = line.iter().map(|s| s.text.width()).sum();
                let r_target = ctx.term_width.saturating_sub(right_pad + s + right_w);
                if r_target > cur {
                    line.push(StyledSpan::new(" ".repeat(r_target - cur)));
                }
                line.extend(right_lines[row_idx].clone());
            }

            // ── Monitor panel ──
            if row_idx < mon_lines.len() {
                let cur: usize = line.iter().map(|s| s.text.width()).sum();
                let mon_w: usize = mon_lines[row_idx].iter().map(|s| s.text.width()).sum();
                if cur + 2 + mon_w <= ctx.term_width {
                    line.push(StyledSpan::new(" ".repeat(2)));
                    line.extend(mon_lines[row_idx].clone());
                }
            }
        } else {
            // ── ASCII only (no panels) — center the logo ──
            if logo_origin > 0 {
                line.push(StyledSpan::new(" ".repeat(logo_origin)));
            }
            if i < ascii_lines.len() {
                line.extend(ascii_lines[i].clone());
            }
        }

        // ── Fill remaining width ──
        let w: usize = line.iter().map(|s| s.text.width()).sum();
        if w < ctx.term_width {
            line.push(StyledSpan::new(" ".repeat(ctx.term_width - w)));
        }

        all.push(line);
    }

    SceneOutput { lines: all }
}

fn render_dashboard(components: &[&dyn Component], ctx: &RenderCtx) -> SceneOutput {
    let ascii = find_component(components, "ascii");
    let system = find_component(components, "system");
    let monitor = find_component(components, "monitor");
    let companion = find_component(components, "companion");

    let half = ctx.term_width / 2;
    let column_width = half.saturating_sub(2).max(20);
    let column_ctx = RenderCtx {
        info: ctx.info,
        cfg: ctx.cfg,
        term_width: column_width,
        palette: ctx.palette,
    };
    let (ascii_lines, ascii_width) = ascii
        .and_then(|component| component.as_any().downcast_ref::<ascii::AsciiComponent>())
        .map(|component| component.render_colored_lines(ctx))
        .unwrap_or_default();
    let sys_lines = system
        .map(|c| {
            wrap_block(
                "SYSTEM",
                &render_system_at_width(c, &column_ctx, column_width),
                column_width,
            )
        })
        .unwrap_or_default();
    let mon_lines = monitor
        .map(|c| wrap_block("MONITOR", &c.render_styled(&column_ctx), column_width))
        .unwrap_or_default();
    let comp_lines = companion
        .map(|c| wrap_block("STATUS", &c.render_styled(&column_ctx), column_width))
        .unwrap_or_default();

    let mut all = Vec::new();

    for i in 0..ascii_lines.len().max(sys_lines.len()) {
        let mut left = Vec::new();
        if i < ascii_lines.len() {
            let pad = half.saturating_sub(ascii_width) / 2;
            if pad > 0 {
                left.push(StyledSpan::new(" ".repeat(pad)));
            }
            left.extend(ascii_lines[i].clone());
        }
        pad_spans_to(&mut left, half);
        let mut line = left;
        if i < sys_lines.len() {
            line.extend(sys_lines[i].clone());
        }
        let w: usize = line.iter().map(|s| s.text.width()).sum();
        if w < ctx.term_width {
            line.push(StyledSpan::new(" ".repeat(ctx.term_width - w)));
        }
        all.push(line);
    }

    all.push(vec![StyledSpan::new("\u{2500}".repeat(ctx.term_width))]);

    for i in 0..mon_lines.len().max(comp_lines.len()) {
        let mut line = mon_lines.get(i).cloned().unwrap_or_default();
        pad_spans_to(&mut line, half);
        if i < comp_lines.len() {
            line.extend(comp_lines[i].clone());
        }
        let w: usize = line.iter().map(|s| s.text.width()).sum();
        if w < ctx.term_width {
            line.push(StyledSpan::new(" ".repeat(ctx.term_width - w)));
        }
        all.push(line);
    }
    SceneOutput { lines: all }
}

fn render_cockpit(components: &[&dyn Component], ctx: &RenderCtx) -> SceneOutput {
    let ascii = find_component(components, "ascii");
    let system = find_component(components, "system");
    let monitor = find_component(components, "monitor");

    let (ascii_lines, ascii_width) = ascii
        .and_then(|component| component.as_any().downcast_ref::<ascii::AsciiComponent>())
        .map(|component| component.render_colored_lines(ctx))
        .unwrap_or_default();
    let half = ctx.term_width / 2;
    let half_ctx = RenderCtx {
        info: ctx.info,
        cfg: ctx.cfg,
        term_width: half.saturating_sub(2).max(20),
        palette: ctx.palette,
    };
    let sys_lines = system
        .map(|c| render_system_at_width(c, &half_ctx, half_ctx.term_width))
        .unwrap_or_default();
    let mon_lines = monitor
        .map(|c| c.render_styled(&half_ctx))
        .unwrap_or_default();

    let mut all = Vec::new();

    all.push(vec![StyledSpan::new("\u{2501}".repeat(ctx.term_width))]);

    for line in &ascii_lines {
        let mut l = Vec::new();
        let pad = ctx.term_width.saturating_sub(ascii_width) / 2;
        if pad > 0 {
            l.push(StyledSpan::new(" ".repeat(pad)));
        }
        l.extend(line.clone());
        let w2: usize = l.iter().map(|s| s.text.width()).sum();
        if w2 < ctx.term_width {
            l.push(StyledSpan::new(" ".repeat(ctx.term_width - w2)));
        }
        all.push(l);
    }

    all.push(vec![StyledSpan::new("\u{2501}".repeat(ctx.term_width))]);

    for i in 0..sys_lines.len().max(mon_lines.len()) {
        let mut line = Vec::new();
        if i < sys_lines.len() {
            line.extend(sys_lines[i].clone());
        }
        let w: usize = line.iter().map(|s| s.text.width()).sum();
        if w < half {
            line.push(StyledSpan::new(" ".repeat(half - w)));
        }
        if i < mon_lines.len() {
            line.extend(mon_lines[i].clone());
        }
        let w2: usize = line.iter().map(|s| s.text.width()).sum();
        if w2 < ctx.term_width {
            line.push(StyledSpan::new(" ".repeat(ctx.term_width - w2)));
        }
        all.push(line);
    }

    all.push(vec![StyledSpan::new("\u{2501}".repeat(ctx.term_width))]);
    SceneOutput { lines: all }
}

fn pad_spans_to(line: &mut Vec<StyledSpan>, width: usize) {
    let current: usize = line.iter().map(|span| span.text.width()).sum();
    if current < width {
        line.push(StyledSpan::new(" ".repeat(width - current)));
    }
}

fn render_system_at_width(
    component: &dyn Component,
    ctx: &RenderCtx,
    width: usize,
) -> Vec<Vec<StyledSpan>> {
    if width >= 72 {
        return component.render_styled(ctx);
    }
    if let Some(system) = component.as_any().downcast_ref::<system::SystemComponent>() {
        let mut lines = system.render_left_styled(ctx, width);
        lines.extend(system.render_right_styled(ctx, width));
        lines
    } else {
        component.render_styled(ctx)
    }
}

pub fn render_monitor_split(components: &[&dyn Component], ctx: &RenderCtx) -> SceneOutput {
    let ascii = find_component(components, "ascii");
    let monitor = find_component(components, "monitor");

    let half = ctx.term_width / 2;
    let (ascii_lines, ascii_width) = ascii
        .and_then(|component| component.as_any().downcast_ref::<ascii::AsciiComponent>())
        .map(|component| component.render_colored_lines(ctx))
        .unwrap_or_default();
    let fetch_width = half.saturating_sub(2);
    let fetch_ctx = RenderCtx {
        info: ctx.info,
        cfg: ctx.cfg,
        term_width: fetch_width,
        palette: ctx.palette,
    };
    let gap = usize::from(ascii_width > 0) * 2;
    let info_lines = render_classicfetch_info(
        &fetch_ctx,
        fetch_width.saturating_sub(ascii_width + gap).max(16),
    );
    let fetch_lines = compose_logo_and_info(ascii_lines, ascii_width, info_lines, fetch_width, gap);

    // Monitor rendered in its own half
    let mon_ctx = RenderCtx {
        info: ctx.info,
        cfg: ctx.cfg,
        term_width: half.saturating_sub(4),
        palette: ctx.palette,
    };
    let mon_lines = monitor
        .map(|c| {
            wrap_block(
                " MONITOR ",
                &c.render_styled(&mon_ctx),
                half.saturating_sub(4),
            )
        })
        .unwrap_or_default();

    let n = fetch_lines.len().max(mon_lines.len());
    let mut all = Vec::new();

    all.push(vec![StyledSpan::new("\u{2501}".repeat(ctx.term_width))]);

    for i in 0..n {
        let mut line = Vec::new();

        let mut left = fetch_lines.get(i).cloned().unwrap_or_default();
        pad_spans_to(&mut left, half);
        line.extend(left);

        // Right half: monitor
        if i < mon_lines.len() {
            line.extend(mon_lines[i].clone());
        } else {
            line.push(StyledSpan::new(" ".repeat(half.saturating_sub(4))));
        }

        let w: usize = line.iter().map(|s| s.text.width()).sum();
        if w < ctx.term_width {
            line.push(StyledSpan::new(" ".repeat(ctx.term_width - w)));
        }
        all.push(line);
    }

    all.push(vec![StyledSpan::new("\u{2501}".repeat(ctx.term_width))]);
    SceneOutput { lines: all }
}

fn render_classicfetch(components: &[&dyn Component], ctx: &RenderCtx) -> SceneOutput {
    let ascii = find_component(components, "ascii");
    let (ascii_lines, ascii_w) = ascii
        .and_then(|component| component.as_any().downcast_ref::<ascii::AsciiComponent>())
        .map(|component| component.render_colored_lines(ctx))
        .unwrap_or_default();
    let gap = if ascii_w > 0 { 3 } else { 0 };
    let info_width = ctx.term_width.saturating_sub(ascii_w + gap);
    let info_lines = render_classicfetch_info(ctx, info_width.max(20));

    SceneOutput {
        lines: compose_logo_and_info(ascii_lines, ascii_w, info_lines, ctx.term_width, gap),
    }
}

fn compose_logo_and_info(
    ascii_lines: Vec<Vec<StyledSpan>>,
    ascii_width: usize,
    info_lines: Vec<Vec<StyledSpan>>,
    total_width: usize,
    gap: usize,
) -> Vec<Vec<StyledSpan>> {
    if ascii_width == 0 {
        return info_lines;
    }
    if total_width.saturating_sub(ascii_width + gap) < 20 {
        let mut lines = ascii_lines;
        if !lines.is_empty() && !info_lines.is_empty() {
            lines.push(Vec::new());
        }
        lines.extend(info_lines);
        return lines;
    }

    let row_count = ascii_lines.len().max(info_lines.len());
    let mut lines = Vec::with_capacity(row_count);
    for row in 0..row_count {
        let mut line = ascii_lines.get(row).cloned().unwrap_or_default();
        let logo_line_width: usize = line.iter().map(|span| span.text.width()).sum();
        line.push(StyledSpan::new(
            " ".repeat(ascii_width.saturating_sub(logo_line_width) + gap),
        ));
        if let Some(info_line) = info_lines.get(row) {
            line.extend(info_line.clone());
        }
        lines.push(line);
    }
    lines
}

fn render_classicfetch_info(ctx: &RenderCtx, width: usize) -> Vec<Vec<StyledSpan>> {
    let title_color =
        Color::from_hex_opt(&ctx.cfg.title.color).unwrap_or(Color::new(255, 154, 152));
    let value_color =
        Color::from_hex_opt(&ctx.cfg.panel.val_color).unwrap_or(Color::new(245, 220, 227));
    let separator_color =
        Color::from_hex_opt(&ctx.cfg.separator.color).unwrap_or(Color::new(157, 133, 255));
    let title = ctx
        .cfg
        .title
        .format
        .replace("{user}", &ctx.info.user)
        .replace("{host}", &ctx.info.host);
    let mut lines = Vec::new();

    if !title.is_empty() {
        lines.push(vec![StyledSpan::new(title.clone()).fg(title_color).bold()]);
        let separator_len = ctx
            .cfg
            .separator
            .length
            .min(title.width().max(1))
            .min(width);
        lines.push(vec![StyledSpan::new(
            ctx.cfg.separator.char.repeat(separator_len),
        )
        .fg(separator_color)]);
    }

    for (index, field) in ctx
        .cfg
        .display
        .left
        .iter()
        .chain(ctx.cfg.display.right.iter())
        .filter(|field| field.enabled)
        .enumerate()
    {
        let key_color = ctx
            .palette
            .get(index % ctx.palette.len().max(1))
            .copied()
            .unwrap_or(Color::new(200, 200, 200));
        let key = if field.icon.trim().is_empty() {
            field.label.clone()
        } else {
            format!("{} {}", field.icon, field.label)
        };
        let prefix = format!("{key}: ");
        let available = width.saturating_sub(prefix.width());
        let value = ctx.info.get(&field.field).unwrap_or("?");
        let value = truncate_to_width(value, available);
        lines.push(vec![
            StyledSpan::new(prefix).fg(key_color).bold(),
            StyledSpan::new(value).fg(value_color),
        ]);
    }
    lines
}

fn truncate_to_width(value: &str, width: usize) -> String {
    if value.width() <= width {
        return value.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let content_width = width.saturating_sub(1);
    let mut output = String::new();
    for character in value.chars() {
        if output.width() + character.width().unwrap_or(0) > content_width {
            break;
        }
        output.push(character);
    }
    output.push('…');
    output
}

#[cfg(test)]
mod tests {
    use super::{
        ascii::AsciiComponent, scene_to_plain, system::SystemComponent, Component, RenderCtx, Scene,
    };
    use crate::config::Config;
    use crate::info::SysInfo;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn parses_every_public_scene_name() {
        for scene in Scene::all() {
            assert_eq!(scene.key().parse::<Scene>().unwrap(), *scene);
        }
    }

    #[test]
    fn accepts_classicfetch_aliases() {
        assert_eq!(
            "classic-fetch".parse::<Scene>().unwrap(),
            Scene::ClassicFetch
        );
        assert_eq!(
            "classic_fetch".parse::<Scene>().unwrap(),
            Scene::ClassicFetch
        );
    }

    #[test]
    fn rejects_removed_split_scene() {
        assert!("split".parse::<Scene>().is_err());
    }

    #[test]
    fn scene_layout_snapshots_are_stable() {
        let info = fixture_info();
        let cfg = Config::default();
        let ascii = AsciiComponent::new("  /\\\n /  \\\n/____\\".to_owned());
        let system = SystemComponent;
        let components: Vec<&dyn Component> = vec![&ascii, &system];
        let cases = [
            (Scene::Classic, 40, 7_780_660_512_022_877_646),
            (Scene::Classic, 80, 17_836_539_954_096_141_241),
            (Scene::Dashboard, 80, 11_220_703_844_841_875_226),
            (Scene::Cockpit, 60, 12_856_565_631_492_553_597),
            (Scene::ClassicFetch, 70, 1_032_133_439_270_468_935),
            (Scene::ClassicFetch, 120, 1_032_133_439_270_468_935),
        ];

        let actual: Vec<u64> = cases
            .iter()
            .map(|(scene, width, _)| {
                let ctx = RenderCtx {
                    info: &info,
                    cfg: &cfg,
                    term_width: *width,
                    palette: &cfg.logo.colors,
                };
                let plain = scene_to_plain(&super::render_scene(*scene, &components, &ctx));
                snapshot_hash(&plain)
            })
            .collect();
        let expected: Vec<u64> = cases.iter().map(|(_, _, hash)| *hash).collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn classicfetch_places_logo_left_and_plain_information_right() {
        let info = fixture_info();
        let cfg = Config::default();
        let ascii = AsciiComponent::new(" /\\\n/__/".to_owned());
        let system = SystemComponent;
        let components: Vec<&dyn Component> = vec![&ascii, &system];
        let ctx = RenderCtx {
            info: &info,
            cfg: &cfg,
            term_width: 100,
            palette: &cfg.logo.colors,
        };

        let plain = scene_to_plain(&super::render_scene(Scene::ClassicFetch, &components, &ctx));
        let first_line = plain.lines().next().expect("first classicfetch row");
        let logo_position = first_line.find("/\\").expect("logo on first row");
        let title_position = first_line
            .find("atlas@workstation")
            .expect("title on first row");

        assert!(logo_position < title_position);
        assert!(!plain.contains('\u{e0b0}'));
    }

    #[test]
    fn every_scene_respects_its_documented_minimum_width() {
        let info = fixture_info();
        let cfg = Config::default();
        let ascii = AsciiComponent::new("  /\\\n /  \\\n/____\\".to_owned());
        let system = SystemComponent;
        let components: Vec<&dyn Component> = vec![&ascii, &system];

        for scene in Scene::all() {
            let width = scene.min_width();
            let ctx = RenderCtx {
                info: &info,
                cfg: &cfg,
                term_width: width,
                palette: &cfg.logo.colors,
            };
            let output = super::render_scene(*scene, &components, &ctx);
            for line in output.lines {
                let rendered_width: usize = line.iter().map(|span| span.text.width()).sum();
                assert!(
                    rendered_width <= width,
                    "{} rendered {rendered_width} columns at its {width}-column minimum",
                    scene.key()
                );
            }
        }
    }

    fn fixture_info() -> SysInfo {
        SysInfo {
            user: "atlas".into(),
            host: "workstation".into(),
            os: "Runic Linux".into(),
            kernel: "6.12.0-test".into(),
            uptime: "2h 34m".into(),
            packages: "424".into(),
            shell: "fish".into(),
            terminal: "kitty".into(),
            cpu: "Example CPU".into(),
            gpu: "Example GPU".into(),
            memory: "4.0/16.0G".into(),
            disk: "32/128G".into(),
            wm: "atlasWM".into(),
            ..SysInfo::default()
        }
    }

    fn snapshot_hash(text: &str) -> u64 {
        text.bytes().fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        })
    }
}
