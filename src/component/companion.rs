use unicode_width::UnicodeWidthStr;

use super::{Component, RenderCtx, StyledSpan};
use crate::theme::Color;

pub struct CompanionComponent;

impl Component for CompanionComponent {
    fn name(&self) -> &str {
        "companion"
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
        let info = ctx.info;
        let tw = ctx.term_width;
        let ac = ctx
            .palette
            .first()
            .copied()
            .unwrap_or(Color::new(255, 102, 146));
        let muted = Color::new(140, 140, 160);

        let device = if !info.device.is_empty() {
            &info.device
        } else {
            &info.host
        };
        let mut lines: Vec<Vec<StyledSpan>> = Vec::new();

        // Greeting
        let user = if !info.user.is_empty() {
            &info.user
        } else {
            "explorer"
        };
        lines.push(vec![
            StyledSpan::new("  Hello ").fg(muted),
            StyledSpan::new(user).fg(ac).bold(),
        ]);

        // Device
        lines.push(vec![
            StyledSpan::new("  ").fg(muted),
            StyledSpan::new(device).fg(Color::new(200, 200, 220)),
        ]);

        lines.push(vec![StyledSpan::new("")]);

        if !info.load.is_empty() {
            lines.push(vec![
                StyledSpan::new("  Load (1m): ").fg(muted),
                StyledSpan::new(&info.load).fg(Color::new(200, 200, 220)),
            ]);
        }

        if !info.processes.is_empty() {
            lines.push(vec![
                StyledSpan::new("  Processes: ").fg(muted),
                StyledSpan::new(&info.processes).fg(Color::new(200, 200, 220)),
            ]);
        }

        if !info.battery_health.is_empty() {
            let health_color = match info.battery_health.as_str() {
                "Good" | "Excellent" => Color::new(80, 200, 120),
                "Fair" => Color::new(255, 200, 80),
                _ => Color::new(200, 100, 100),
            };
            lines.push(vec![
                StyledSpan::new("  Battery Health: ").fg(muted),
                StyledSpan::new(&info.battery_health).fg(health_color),
            ]);
        }

        // Battery
        if !info.battery_level.is_empty() {
            let level = info.battery_level.trim_end_matches('%');
            let temp = if !info.battery_temp.is_empty() {
                format!(" | {}", info.battery_temp)
            } else {
                String::new()
            };
            lines.push(vec![
                StyledSpan::new("  Battery: ").fg(muted),
                StyledSpan::new(format!("{}%{}", level, temp)).fg(Color::new(100, 200, 255)),
            ]);
        }

        // Uptime
        let uptime = if !info.uptime.is_empty() {
            &info.uptime
        } else {
            &info.uptime_days
        };
        if !uptime.is_empty() && uptime != "—" {
            lines.push(vec![
                StyledSpan::new("  Uptime: ").fg(muted),
                StyledSpan::new(uptime).fg(Color::new(200, 200, 220)),
            ]);
        }

        // Root status
        if !info.root_status.is_empty() {
            let rc = if info.root_status.contains("active") || info.root_status.contains("Unlocked")
            {
                Color::new(255, 200, 80)
            } else {
                Color::new(200, 100, 100)
            };
            lines.push(vec![
                StyledSpan::new("  Root: ").fg(muted),
                StyledSpan::new(&info.root_status).fg(rc),
            ]);
        }

        // Padding to fill width
        for line in &mut lines {
            let w: usize = line.iter().map(|s| s.text.width()).sum();
            if w < tw {
                line.push(StyledSpan::new(" ".repeat(tw - w)));
            }
        }

        lines
    }

    fn min_width(&self) -> usize {
        20
    }
    fn min_height(&self) -> usize {
        5
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
