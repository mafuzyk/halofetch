use unicode_width::UnicodeWidthStr;

use super::{Component, RenderCtx, StyledSpan};
use crate::theme::Color;
use crate::widget::{FieldWidget, RenderCtx as WidgetCtx};

pub struct SystemComponent;

impl SystemComponent {
    pub fn render_left_styled(&self, ctx: &RenderCtx, avail: usize) -> Vec<Vec<StyledSpan>> {
        let mut lines = Vec::new();
        for (i, fd) in ctx
            .cfg
            .display
            .left
            .iter()
            .filter(|f| f.enabled)
            .enumerate()
        {
            let p = ctx
                .palette
                .get(i % ctx.palette.len().max(1))
                .copied()
                .unwrap_or(Color::new(200, 200, 200));
            let wctx = WidgetCtx {
                info: ctx.info,
                panel_cfg: &ctx.cfg.panel,
                max_width: avail,
                fg_color: p,
            };
            let out = FieldWidget::from_def(fd.clone()).render_inherent(&wctx);
            let mut row = Vec::new();
            for s in &out.styled {
                row.push(StyledSpan::new(&s.text).fg(s.fg.unwrap_or(Color::new(200, 200, 200))));
            }
            lines.push(row);
        }
        lines
    }

    pub fn render_right_styled(&self, ctx: &RenderCtx, avail: usize) -> Vec<Vec<StyledSpan>> {
        let mut lines = Vec::new();
        for (i, fd) in ctx
            .cfg
            .display
            .right
            .iter()
            .filter(|f| f.enabled)
            .enumerate()
        {
            let offset = (ctx.palette.len() / 2).max(1);
            let wctx = WidgetCtx {
                info: ctx.info,
                panel_cfg: &ctx.cfg.panel,
                max_width: avail,
                fg_color: ctx
                    .palette
                    .get((i + offset) % ctx.palette.len())
                    .copied()
                    .unwrap_or(Color::new(200, 200, 200)),
            };
            let out = FieldWidget::from_def(fd.clone()).render_inherent(&wctx);
            let mut row = Vec::new();
            for s in &out.styled {
                row.push(StyledSpan::new(&s.text).fg(s.fg.unwrap_or(Color::new(200, 200, 200))));
            }
            lines.push(row);
        }
        lines
    }
}

impl Component for SystemComponent {
    fn name(&self) -> &str {
        "system"
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
        let mut lines = Vec::new();
        let left = &ctx.cfg.display.left;
        let right = &ctx.cfg.display.right;

        let n = left.len().max(right.len());
        let half = ctx.term_width / 2;

        for i in 0..n {
            let mut line = Vec::new();

            if let Some(fd) = left.get(i).filter(|f| f.enabled) {
                let p = ctx
                    .palette
                    .get(i % ctx.palette.len().max(1))
                    .copied()
                    .unwrap_or(crate::theme::Color::new(200, 200, 200));
                let wctx = WidgetCtx {
                    info: ctx.info,
                    panel_cfg: &ctx.cfg.panel,
                    max_width: half.saturating_sub(4),
                    fg_color: p,
                };
                let out = FieldWidget::from_def(fd.clone()).render_inherent(&wctx);
                for s in &out.styled {
                    line.push(
                        StyledSpan::new(&s.text)
                            .fg(s.fg.unwrap_or(crate::theme::Color::new(200, 200, 200))),
                    );
                }
            }

            let cur: usize = line.iter().map(|s| s.text.width()).sum();
            if cur < half {
                line.push(StyledSpan::new(" ".repeat(half - cur)));
            }

            if let Some(fd) = right.get(i).filter(|f| f.enabled) {
                let right_off = (ctx.palette.len() / 2).max(1);
                let wctx = WidgetCtx {
                    info: ctx.info,
                    panel_cfg: &ctx.cfg.panel,
                    max_width: half.saturating_sub(4),
                    fg_color: ctx
                        .palette
                        .get((i + right_off) % ctx.palette.len())
                        .copied()
                        .unwrap_or(crate::theme::Color::new(200, 200, 200)),
                };
                let out = FieldWidget::from_def(fd.clone()).render_inherent(&wctx);
                for s in &out.styled {
                    line.push(
                        StyledSpan::new(&s.text)
                            .fg(s.fg.unwrap_or(crate::theme::Color::new(200, 200, 200))),
                    );
                }
            }

            lines.push(line);
        }
        lines
    }

    fn min_width(&self) -> usize {
        30
    }
    fn min_height(&self) -> usize {
        5
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
