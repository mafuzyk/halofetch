use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

use super::{Component, RenderCtx, StyledSpan};
use crate::theme::Color;

pub struct AsciiComponent {
    pub art: String,
}

impl AsciiComponent {
    pub fn new(art: String) -> Self {
        Self { art }
    }

    pub fn render_colored_lines(&self, ctx: &RenderCtx) -> (Vec<Vec<StyledSpan>>, usize) {
        let raw: Vec<&str> = self.art.lines().collect();
        if raw.is_empty() {
            return (Vec::new(), 0);
        }

        let lines: Vec<String> = raw.iter().map(|l| l.trim_end().to_string()).collect();
        let max_w = lines.iter().map(|l| l.width()).max().unwrap_or(0);
        let cols = ctx.palette;
        let total = lines.len();
        let is_vert = ctx.cfg.logo.color_dir == "vertical";

        let mut result = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let mut spans = Vec::new();
            let mut col = 0usize;
            for ch in line.chars() {
                let w = ch.width().unwrap_or(0);
                let flag_c = crate::theme::flag_color_at(cols, i, col, total, max_w, false);
                let color = flag_c.unwrap_or_else(|| {
                    let idx = if is_vert { col } else { i };
                    cols.get(crate::theme::stretch_index(
                        idx,
                        if is_vert { max_w } else { total },
                        cols.len(),
                    ))
                    .copied()
                    .unwrap_or(Color::new(255, 255, 255))
                });
                if ch != ' ' {
                    spans.push(StyledSpan::new(ch.to_string()).fg(color));
                } else {
                    spans.push(StyledSpan::new(" "));
                }
                col += w;
            }
            let row_w: usize = spans.iter().map(|s| s.text.width()).sum();
            if row_w < max_w {
                spans.push(StyledSpan::new(" ".repeat(max_w - row_w)));
            }
            result.push(spans);
        }
        (result, max_w)
    }
}

impl Component for AsciiComponent {
    fn name(&self) -> &str {
        "ascii"
    }

    fn render_ansi(&self, ctx: &RenderCtx) -> String {
        let styled = self.render_styled(ctx);
        let mut out = String::new();
        for line in styled {
            for s in &line {
                if s.bold {
                    out.push_str("\x1b[1m");
                }
                if let Some(fg) = &s.fg {
                    out.push_str(&fg.fg_escape());
                }
                if let Some(bg) = &s.bg {
                    out.push_str(&bg.bg_escape());
                }
                out.push_str(&s.text);
                out.push_str("\x1b[0m");
            }
            out.push('\n');
        }
        out
    }

    fn render_styled(&self, ctx: &RenderCtx) -> Vec<Vec<StyledSpan>> {
        let raw: Vec<&str> = self.art.lines().collect();
        if raw.is_empty() {
            return Vec::new();
        }

        let lines: Vec<String> = raw.iter().map(|l| l.trim_end().to_string()).collect();
        let max_w = lines.iter().map(|l| l.width()).max().unwrap_or(0);
        let center = ctx.term_width.saturating_sub(max_w) / 2;
        let cols = ctx.palette;
        let total = lines.len();
        let is_vert = ctx.cfg.logo.color_dir == "vertical";

        let mut result = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let mut spans = Vec::new();
            if center > 0 {
                spans.push(StyledSpan::new(" ".repeat(center)));
            }
            let mut col = 0usize;
            for ch in line.chars() {
                let w = ch.width().unwrap_or(0);
                let flag_c = crate::theme::flag_color_at(cols, i, col, total, max_w, false);
                let color = flag_c.unwrap_or_else(|| {
                    let idx = if is_vert { col } else { i };
                    cols.get(crate::theme::stretch_index(
                        idx,
                        if is_vert { max_w } else { total },
                        cols.len(),
                    ))
                    .copied()
                    .unwrap_or(Color::new(255, 255, 255))
                });
                if ch != ' ' {
                    spans.push(StyledSpan::new(ch.to_string()).fg(color));
                } else {
                    spans.push(StyledSpan::new(" "));
                }
                col += w;
            }
            let row_w: usize = spans.iter().map(|s| s.text.width()).sum();
            let need = center + max_w;
            if row_w < need {
                spans.push(StyledSpan::new(" ".repeat(need - row_w)));
            }
            result.push(spans);
        }
        result
    }

    fn min_width(&self) -> usize {
        self.art
            .lines()
            .map(|l| l.trim_end().width())
            .max()
            .unwrap_or(0)
    }
    fn min_height(&self) -> usize {
        self.art.lines().count()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
