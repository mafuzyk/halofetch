//! Shared ANSI primitives used by the fetch renderer and TUI preview.

use unicode_width::UnicodeWidthStr;

use crate::config::Config;
use crate::theme::Color;

const RESET: &str = "\x1b[0m";

#[derive(Debug, Clone)]
pub struct StyledSegment {
    pub text: String,
    pub fg: Option<Color>,
}

/// Render only the configured ASCII art, colored and centered.
pub fn render_ascii_only(cfg: &Config, ascii_art: &str) -> String {
    let lines = dedent(
        &ascii_art
            .lines()
            .map(str::to_owned)
            .collect::<Vec<String>>(),
    );
    if lines.is_empty() {
        return String::new();
    }

    let term_width = terminal_width();
    let fallback = Color::new(255, 255, 255);
    let max_width = lines
        .iter()
        .map(|line| line.trim_end().width())
        .max()
        .unwrap_or(0);
    let left_padding = term_width.saturating_sub(max_width) / 2;
    let vertical_colors = cfg.logo.color_dir == "vertical";

    let mut output = String::new();
    for (line_index, line) in lines.iter().enumerate() {
        output.push_str(&" ".repeat(left_padding));
        let padded = format!("{:width$}", line.trim_end(), width = max_width);
        for (column, character) in padded.chars().enumerate() {
            let color = crate::theme::flag_color_at(
                &cfg.logo.colors,
                line_index,
                column,
                lines.len(),
                max_width,
                vertical_colors,
            )
            .or_else(|| {
                let index = if vertical_colors { column } else { line_index };
                cfg.logo
                    .colors
                    .get(crate::theme::stretch_index(
                        index,
                        if vertical_colors {
                            max_width
                        } else {
                            lines.len()
                        },
                        cfg.logo.colors.len(),
                    ))
                    .copied()
            })
            .unwrap_or(fallback);

            if character == ' ' {
                output.push(' ');
            } else {
                output.push_str(&color.fg_escape());
                output.push(character);
            }
        }
        output.push_str(RESET);
        output.push('\n');
    }
    output
}

fn dedent(lines: &[String]) -> Vec<String> {
    let indentation = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.chars()
                .take_while(|character| *character == ' ')
                .count()
        })
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|line| line.chars().skip(indentation).collect())
        .collect()
}

fn terminal_width() -> usize {
    crossterm::terminal::size()
        .map(|(width, _)| width as usize)
        .unwrap_or(80)
}

#[cfg(test)]
mod tests {
    use super::dedent;

    #[test]
    fn dedent_removes_only_shared_indentation() {
        let lines = vec!["    one".to_owned(), "      two".to_owned()];
        assert_eq!(dedent(&lines), ["one", "  two"]);
    }
}
