use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color as TuiColor, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::{Frame, Terminal};

use crate::component::{self, Component, RenderCtx, SceneOutput};
use crate::{ascii, config, info};

struct LiveTerminal;

impl LiveTerminal {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = terminal::disable_raw_mode();
            return Err(error.into());
        }
        Ok(Self)
    }
}

impl Drop for LiveTerminal {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

pub fn run(interval_ms: u64, scene: Option<&str>) -> Result<()> {
    ascii::ensure_logos()?;
    let cfg = config::Config::load()?;
    let scene = scene.map(str::parse).transpose()?.unwrap_or(cfg.scene);
    let ascii_art = ascii::load(&cfg)?;
    let mut system_info = info::collect()?;
    let ascii_component = component::ascii::AsciiComponent::new(ascii_art);
    let system_component = component::system::SystemComponent;
    let monitor_component = component::monitor::MonitorComponent::new();
    let companion_component = component::companion::CompanionComponent;
    let components: Vec<&dyn Component> = vec![
        &ascii_component,
        &system_component,
        &monitor_component,
        &companion_component,
    ];

    let (cols, rows) = terminal::size()?;
    let initial_shell_rows = shell_rows(rows);
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: initial_shell_rows,
            cols: cols.saturating_sub(2).max(1),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| color_eyre::eyre::eyre!("failed to open shell PTY: {error}"))?;
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let shell_name = Path::new(&shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let mut command = CommandBuilder::new(&shell);
    command.arg("-i");
    // Some Fish presets run another system fetch from `fish_greeting`.
    // Inside this workspace that delays the prompt and makes buffered input
    // look frozen. Keep the user's Fish config, but remove only its greeting
    // from this child shell after the config has loaded.
    if shell_name == "fish" {
        // Fish 4.1+ probes terminal capabilities during startup. The embedded
        // VT renderer does not answer those queries yet, so disable the probe
        // for this process instead of making every startup wait ten seconds.
        command.arg("--features=no-query-term");
        command.arg("-C");
        command.arg("functions --erase fish_greeting");
    }
    command.env("TERM", "xterm-256color");
    if let Ok(cwd) = std::env::current_dir() {
        command.cwd(cwd);
    }
    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| color_eyre::eyre::eyre!("failed to start shell: {error}"))?;
    drop(pair.slave);
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| color_eyre::eyre::eyre!("failed to read shell PTY: {error}"))?;
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|error| color_eyre::eyre::eyre!("failed to write shell PTY: {error}"))?;
    let parser = Arc::new(Mutex::new(vt100::Parser::new(
        initial_shell_rows,
        cols.saturating_sub(2).max(1),
        10_000,
    )));
    let reader_parser = Arc::clone(&parser);
    let shell_dirty = Arc::new(AtomicBool::new(true));
    let reader_dirty = Arc::clone(&shell_dirty);
    std::thread::spawn(move || {
        let mut bytes = [0_u8; 8192];
        while let Ok(count) = reader.read(&mut bytes) {
            if count == 0 {
                break;
            }
            if let Ok(mut parser) = reader_parser.lock() {
                parser.process(&bytes[..count]);
                reader_dirty.store(true, Ordering::Release);
            }
        }
    });

    let _session = LiveTerminal::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let interval = Duration::from_millis(interval_ms);
    let mut last_refresh = Instant::now() - interval;
    let mut monitor_dirty = true;

    loop {
        if last_refresh.elapsed() >= interval {
            info::refresh_live(&mut system_info);
            last_refresh = Instant::now();
            monitor_dirty = true;
        }
        let should_draw = monitor_dirty || shell_dirty.swap(false, Ordering::AcqRel);
        if should_draw {
            terminal.draw(|frame| {
                let area = frame.area();
                let regions = workspace_regions(area);
                let ctx = RenderCtx {
                    info: &system_info,
                    cfg: &cfg,
                    term_width: regions[0].width.saturating_sub(2).max(1) as usize,
                    palette: &cfg.logo.colors,
                };
                let monitor = component::render_scene(scene, &components, &ctx);
                render_monitor(frame, regions[0], &monitor, interval_ms);
                if let Ok(parser) = parser.lock() {
                    render_shell(frame, regions[1], parser.screen());
                }
            })?;
            monitor_dirty = false;
        }

        if child.try_wait()?.is_some() {
            break;
        }
        if event::poll(Duration::from_millis(33))? {
            match event::read()? {
                Event::Key(key)
                    if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                {
                    if key.code == KeyCode::Char('q')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        child.kill()?;
                        break;
                    }
                    let bytes = key_bytes(key);
                    if !bytes.is_empty() {
                        writer.write_all(&bytes)?;
                        writer.flush()?;
                    }
                }
                Event::Resize(width, height) => {
                    let rows = shell_rows(height);
                    let cols = width.saturating_sub(2).max(1);
                    pair.master
                        .resize(PtySize {
                            rows,
                            cols,
                            pixel_width: 0,
                            pixel_height: 0,
                        })
                        .map_err(|error| {
                            color_eyre::eyre::eyre!("failed to resize shell PTY: {error}")
                        })?;
                    if let Ok(mut parser) = parser.lock() {
                        parser.set_size(rows, cols);
                    }
                    terminal.resize(Rect::new(0, 0, width, height))?;
                    monitor_dirty = true;
                }
                Event::Paste(text) => writer.write_all(text.as_bytes())?,
                _ => {}
            }
        }
    }
    Ok(())
}

fn workspace_regions(area: Rect) -> [Rect; 2] {
    let monitor_percent = if area.height < 30 { 40 } else { 55 };
    let regions = Layout::vertical([
        Constraint::Percentage(monitor_percent),
        Constraint::Percentage(100 - monitor_percent),
    ])
    .split(area);
    [regions[0], regions[1]]
}

fn shell_rows(total_rows: u16) -> u16 {
    let area = Rect::new(0, 0, 80, total_rows);
    workspace_regions(area)[1].height.saturating_sub(2).max(1)
}

fn render_monitor(frame: &mut Frame, area: Rect, output: &SceneOutput, interval_ms: u64) {
    let lines = output
        .lines
        .iter()
        .map(|line| {
            Line::from(
                line.iter()
                    .map(|span| {
                        let mut style = Style::default();
                        if let Some(color) = span.fg {
                            style = style.fg(TuiColor::Rgb(color.r, color.g, color.b));
                        }
                        if span.bold {
                            style = style.add_modifier(Modifier::BOLD);
                        }
                        Span::styled(span.text.clone(), style)
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(Text::from(lines)).block(
            Block::default()
                .title(format!(" AtlasFetch · live {interval_ms}ms "))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(TuiColor::Rgb(157, 133, 255))),
        ),
        area,
    );
}

fn render_shell(frame: &mut Frame, area: Rect, screen: &vt100::Screen) {
    let block = Block::default()
        .title(" Shell · Ctrl+Q closes workspace ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(TuiColor::Rgb(100, 100, 125)));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    for row in 0..inner.height {
        for col in 0..inner.width {
            let Some(source) = screen.cell(row, col) else {
                continue;
            };
            if source.is_wide_continuation() {
                continue;
            }
            let mut style = Style::default()
                .fg(vt_color(source.fgcolor()))
                .bg(vt_color(source.bgcolor()));
            if source.bold() {
                style = style.add_modifier(Modifier::BOLD);
            }
            if source.italic() {
                style = style.add_modifier(Modifier::ITALIC);
            }
            if source.underline() {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            if source.inverse() {
                style = style.add_modifier(Modifier::REVERSED);
            }
            if let Some(target) = frame.buffer_mut().cell_mut((inner.x + col, inner.y + row)) {
                target.set_symbol(&source.contents()).set_style(style);
            }
        }
    }
    if !screen.hide_cursor() {
        let (row, col) = screen.cursor_position();
        if row < inner.height && col < inner.width {
            frame.set_cursor_position((inner.x + col, inner.y + row));
        }
    }
}

fn vt_color(color: vt100::Color) -> TuiColor {
    match color {
        vt100::Color::Default => TuiColor::Reset,
        vt100::Color::Idx(index) => TuiColor::Indexed(index),
        vt100::Color::Rgb(red, green, blue) => TuiColor::Rgb(red, green, blue),
    }
}

fn key_bytes(key: KeyEvent) -> Vec<u8> {
    let mut bytes = Vec::new();
    if key.modifiers.contains(KeyModifiers::ALT) {
        bytes.push(0x1b);
    }
    match key.code {
        KeyCode::Char(character) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let value = character.to_ascii_lowercase() as u32;
            if let Ok(value) = u8::try_from(value) {
                bytes.push(value & 0x1f);
            }
        }
        KeyCode::Char(character) => {
            let mut encoded = [0_u8; 4];
            bytes.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
        }
        KeyCode::Enter => bytes.push(b'\r'),
        KeyCode::Tab => bytes.push(b'\t'),
        KeyCode::BackTab => bytes.extend_from_slice(b"\x1b[Z"),
        KeyCode::Backspace => bytes.push(0x7f),
        KeyCode::Esc => bytes.push(0x1b),
        KeyCode::Up => bytes.extend_from_slice(b"\x1b[A"),
        KeyCode::Down => bytes.extend_from_slice(b"\x1b[B"),
        KeyCode::Right => bytes.extend_from_slice(b"\x1b[C"),
        KeyCode::Left => bytes.extend_from_slice(b"\x1b[D"),
        KeyCode::Home => bytes.extend_from_slice(b"\x1b[H"),
        KeyCode::End => bytes.extend_from_slice(b"\x1b[F"),
        KeyCode::Delete => bytes.extend_from_slice(b"\x1b[3~"),
        KeyCode::Insert => bytes.extend_from_slice(b"\x1b[2~"),
        KeyCode::PageUp => bytes.extend_from_slice(b"\x1b[5~"),
        KeyCode::PageDown => bytes.extend_from_slice(b"\x1b[6~"),
        KeyCode::F(1) => bytes.extend_from_slice(b"\x1bOP"),
        KeyCode::F(2) => bytes.extend_from_slice(b"\x1bOQ"),
        KeyCode::F(3) => bytes.extend_from_slice(b"\x1bOR"),
        KeyCode::F(4) => bytes.extend_from_slice(b"\x1bOS"),
        KeyCode::F(number @ 5..=12) => {
            let suffix = match number {
                5 => 15,
                6 => 17,
                7 => 18,
                8 => 19,
                9 => 20,
                10 => 21,
                11 => 23,
                12 => 24,
                _ => unreachable!(),
            };
            bytes.extend_from_slice(format!("\x1b[{suffix}~").as_bytes());
        }
        _ => {}
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::{key_bytes, shell_rows, workspace_regions};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::layout::Rect;

    #[test]
    fn workspace_always_leaves_room_for_the_shell() {
        let regions = workspace_regions(Rect::new(0, 0, 120, 40));
        assert_eq!(regions[0].height + regions[1].height, 40);
        assert!(shell_rows(40) >= 10);
    }

    #[test]
    fn control_keys_are_encoded_for_the_pty() {
        assert_eq!(
            key_bytes(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            [3]
        );
        assert_eq!(
            key_bytes(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
            b"\x1b[A"
        );
    }
}
