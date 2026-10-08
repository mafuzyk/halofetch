//! Monitor workspace: system metrics refreshed in place above an embedded shell.
//!
//! The shell runs in a pseudo-terminal and is drawn through a `vt100` parser, so it
//! behaves like a normal terminal session. The upper region shows the scene and grows
//! with it, up to 60% of the screen.

use std::io::{self, Read, Write};
use std::path::Path;
#[cfg(any(windows, test))]
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use color_eyre::eyre::eyre;
use color_eyre::Result;
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::Command;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color as TuiColor, Modifier, Style};
use ratatui::text::Text;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::{Frame, Terminal};

use crate::config::{Config, Scene};
use crate::info::{self, CpuSampler, SysInfo};
use crate::logo::LogoSet;
use crate::render::scene::RenderCtx;
use crate::render::{self, Line};
use crate::styled_text;
use crate::tui::{disable_bracketed_paste, enable_bracketed_paste};

/// The information region takes at most this share of the screen height.
const MONITOR_MAX_PERCENT: u32 = 60;
const SCROLLBACK_LINES: usize = 10_000;
const EVENT_POLL: Duration = Duration::from_millis(33);
const BORDER_COLOR: TuiColor = TuiColor::Rgb(157, 133, 255);
const SHELL_BORDER_COLOR: TuiColor = TuiColor::Rgb(100, 100, 125);
const PASTE_START: &str = "\x1b[200~";
const PASTE_END: &str = "\x1b[201~";
/// Cursor position request (DSR 6). ConPTY sends one when it starts and draws nothing
/// until it gets an answer; shells such as fish ask it too.
const CURSOR_QUERY: &[u8] = b"\x1b[6n";

type Term = Terminal<CrosstermBackend<io::Stdout>>;

pub fn run(cfg: &Config, interval_ms: u64, scene: Scene) -> Result<()> {
    let system = info::collect();
    let logos = cfg.logo_set(&system);
    let mut workspace = Workspace {
        cfg,
        scene,
        interval_ms,
        system,
        logos,
        sampler: CpuSampler::new(),
    };

    let (columns, rows) = terminal::size()?;
    let area = Rect::new(0, 0, columns, rows);
    let [_, shell_area] = split(area, workspace.lines(area).len());
    let (shell_rows, shell_cols) = shell_size(shell_area);
    let mut shell = Shell::spawn(shell_rows, shell_cols)?;

    let _session = LiveTerminal::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let interval = Duration::from_millis(interval_ms);
    let mut next_refresh = Instant::now();
    let mut dirty = true;

    loop {
        if Instant::now() >= next_refresh {
            workspace.refresh();
            next_refresh = Instant::now() + interval;
            dirty = true;
        }
        let shell_changed = shell.dirty.swap(false, Ordering::AcqRel);
        if dirty || shell_changed {
            workspace.draw(&mut terminal, &mut shell)?;
            dirty = false;
        }
        if shell.exited() {
            break;
        }
        if !event::poll(EVENT_POLL)? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                if is_quit(key) {
                    break;
                }
                let bytes = key_bytes(key);
                if !shell.send(&bytes) {
                    break;
                }
            }
            Event::Paste(text) => {
                let bytes = paste_bytes(&text, shell.bracketed_paste());
                if !shell.send(&bytes) {
                    break;
                }
            }
            Event::Resize(..) => dirty = true,
            _ => {}
        }
    }
    shell.kill();
    Ok(())
}

/// What the monitor shows: the configuration, the system values and the scene.
struct Workspace<'a> {
    cfg: &'a Config,
    scene: Scene,
    interval_ms: u64,
    system: SysInfo,
    logos: LogoSet,
    sampler: CpuSampler,
}

impl Workspace<'_> {
    fn refresh(&mut self) {
        info::refresh_live(&mut self.system, &mut self.sampler);
    }

    /// Scene lines for the information region of a screen `area`.
    fn lines(&self, area: Rect) -> Vec<Line> {
        let ctx = RenderCtx {
            info: &self.system,
            cfg: self.cfg,
            logos: &self.logos,
            width: usize::from(area.width.saturating_sub(2)),
        };
        render::scene::render(self.scene, &ctx)
    }

    /// Draws both regions. The shell is resized to its region first, so a scene that
    /// grew or shrank also resizes the shell.
    fn draw(&self, terminal: &mut Term, shell: &mut Shell) -> Result<()> {
        let size = terminal.size()?;
        let area = Rect::new(0, 0, size.width, size.height);
        let lines = self.lines(area);
        let [monitor_area, shell_area] = split(area, lines.len());
        let (rows, cols) = shell_size(shell_area);
        shell.resize(rows, cols)?;
        terminal.draw(|frame| {
            render_monitor(frame, monitor_area, &lines, self.interval_ms);
            if let Ok(parser) = shell.parser.lock() {
                render_shell(frame, shell_area, parser.screen());
            }
        })?;
        Ok(())
    }
}

/// The information region fits its content, capped at 60% of the height. The shell
/// gets everything below it.
fn split(area: Rect, content_lines: usize) -> [Rect; 2] {
    let cap = u32::from(area.height) * MONITOR_MAX_PERCENT / 100;
    let wanted = u32::try_from(content_lines.saturating_add(2)).unwrap_or(u32::MAX);
    let height = u16::try_from(cap.min(wanted)).unwrap_or(area.height);
    let [top, bottom] =
        Layout::vertical([Constraint::Length(height), Constraint::Min(0)]).areas(area);
    [top, bottom]
}

/// PTY size inside the borders of a region. Never zero, because a zero-sized PTY is
/// rejected by some systems.
fn shell_size(region: Rect) -> (u16, u16) {
    (
        region.height.saturating_sub(2).max(1),
        region.width.saturating_sub(2).max(1),
    )
}

struct Shell {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    parser: Arc<Mutex<vt100::Parser>>,
    /// Set by the reader thread when the shell printed something.
    dirty: Arc<AtomicBool>,
    rows: u16,
    cols: u16,
}

impl Shell {
    fn spawn(rows: u16, cols: u16) -> Result<Shell> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| eyre!("failed to open shell PTY: {error}"))?;
        let child = pair
            .slave
            .spawn_command(shell_command())
            .map_err(|error| eyre!("failed to start shell: {error}"))?;
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| eyre!("failed to read shell PTY: {error}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| eyre!("failed to write shell PTY: {error}"))?;

        let writer = Arc::new(Mutex::new(writer));
        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, SCROLLBACK_LINES)));
        let dirty = Arc::new(AtomicBool::new(true));
        let reader_parser = Arc::clone(&parser);
        let reader_dirty = Arc::clone(&dirty);
        let reader_writer = Arc::clone(&writer);
        std::thread::spawn(move || {
            let mut bytes = [0_u8; 8192];
            let mut tail = Vec::new();
            while let Ok(count) = reader.read(&mut bytes) {
                if count == 0 {
                    break;
                }
                let chunk = &bytes[..count];
                let queries = cursor_queries(&mut tail, chunk);
                let Ok(mut parser) = reader_parser.lock() else {
                    break;
                };
                parser.process(chunk);
                reader_dirty.store(true, Ordering::Release);
                if queries == 0 {
                    continue;
                }
                let (row, col) = parser.screen().cursor_position();
                drop(parser);
                let reply = cursor_report(row, col).repeat(queries);
                if let Ok(mut writer) = reader_writer.lock() {
                    let _ = writer
                        .write_all(reply.as_bytes())
                        .and_then(|()| writer.flush());
                }
            }
        });

        Ok(Shell {
            master: pair.master,
            child,
            writer,
            parser,
            dirty,
            rows,
            cols,
        })
    }

    fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
        if (rows, cols) == (self.rows, self.cols) {
            return Ok(());
        }
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| eyre!("failed to resize shell PTY: {error}"))?;
        if let Ok(mut parser) = self.parser.lock() {
            parser.set_size(rows, cols);
        }
        self.rows = rows;
        self.cols = cols;
        Ok(())
    }

    /// Whether the shell asked for bracketed paste. Pastes are wrapped only then.
    fn bracketed_paste(&self) -> bool {
        self.parser
            .lock()
            .is_ok_and(|parser| parser.screen().bracketed_paste())
    }

    /// `false` when the shell no longer accepts input.
    fn send(&mut self, bytes: &[u8]) -> bool {
        if bytes.is_empty() {
            return true;
        }
        let Ok(mut writer) = self.writer.lock() else {
            return false;
        };
        writer
            .write_all(bytes)
            .and_then(|()| writer.flush())
            .is_ok()
    }

    fn exited(&mut self) -> bool {
        !matches!(self.child.try_wait(), Ok(None))
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
    }
}

/// The user's shell, interactive. Fish runs with its greeting removed and its
/// terminal query disabled: a greeting that runs another fetch delays the prompt, and
/// the embedded terminal does not answer the queries.
#[cfg(not(windows))]
fn shell_command() -> CommandBuilder {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let shell_name = Path::new(&shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let mut command = CommandBuilder::new(&shell);
    command.arg("-i");
    if shell_name == "fish" {
        command.arg("--features=no-query-term");
        command.arg("-C");
        command.arg("functions --erase fish_greeting");
    }
    command.env("TERM", "xterm-256color");
    if let Ok(cwd) = std::env::current_dir() {
        command.cwd(cwd);
    }
    command
}

/// The shell on Windows: `$SHELL` when it names an existing file (Git Bash exports an
/// MSYS path that Windows cannot start), else PowerShell, else the console interpreter.
/// No `TERM`: only the Unix-style shells below understand the interactive option.
#[cfg(windows)]
fn shell_command() -> CommandBuilder {
    let shell = windows_shell(
        std::env::var_os("SHELL")
            .map(PathBuf::from)
            .filter(|path| path.is_file()),
        find_on_path,
        std::env::var_os("COMSPEC").map(PathBuf::from),
    );
    let mut command = CommandBuilder::new(&shell);
    for arg in windows_shell_args(&shell) {
        command.arg(arg);
    }
    if let Ok(cwd) = std::env::current_dir() {
        command.cwd(cwd);
    }
    command
}

/// The order of the Windows shell choice. Pure, so the order is tested on every platform.
#[cfg(any(windows, test))]
fn windows_shell(
    shell: Option<PathBuf>,
    find: impl Fn(&str) -> Option<PathBuf>,
    comspec: Option<PathBuf>,
) -> PathBuf {
    shell
        .or_else(|| find("pwsh.exe"))
        .or_else(|| find("powershell.exe"))
        .or(comspec)
        .unwrap_or_else(|| PathBuf::from("cmd.exe"))
}

/// Arguments for the Windows shell, chosen by the file name without its extension.
/// Bash, zsh and fish get the same interactive option as on Unix, fish with the greeting
/// workaround. PowerShell and cmd get none.
#[cfg(any(windows, test))]
fn windows_shell_args(shell: &Path) -> &'static [&'static str] {
    let file = shell.to_string_lossy();
    let file = file.rsplit(['\\', '/']).next().unwrap_or_default();
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    match stem.to_ascii_lowercase().as_str() {
        "fish" => &[
            "-i",
            "--features=no-query-term",
            "-C",
            "functions --erase fish_greeting",
        ],
        "bash" | "zsh" => &["-i"],
        _ => &[],
    }
}

/// The first file called `name` in the directories listed in `PATH`.
#[cfg(windows)]
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

struct LiveTerminal;

impl LiveTerminal {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let entered =
            execute!(io::stdout(), EnterAlternateScreen).and_then(|()| enable_bracketed_paste());
        if let Err(error) = entered {
            let _ = terminal::disable_raw_mode();
            return Err(error.into());
        }
        Ok(Self)
    }
}

impl Drop for LiveTerminal {
    fn drop(&mut self) {
        disable_bracketed_paste();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

fn is_quit(key: KeyEvent) -> bool {
    key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL)
}

/// Redraws the static scene in place every `interval` until q, Esc or Ctrl+C. No shell is
/// started, and the system is detected once and refreshed between frames.
pub fn watch(cfg: &Config, scene: Scene, interval: Duration) -> Result<()> {
    let mut system = info::collect();
    let logos = cfg.logo_set(&system);
    let mut sampler = CpuSampler::new();
    let _session = WatchTerminal::enter()?;
    let mut stdout = io::stdout();
    let mut drawn = None;
    loop {
        let (width, _) = terminal::size()?;
        let ctx = RenderCtx {
            info: &system,
            cfg,
            logos: &logos,
            width: usize::from(width),
        };
        let text = styled_text(&render::scene::render(scene, &ctx));
        stdout.write_all(frame_bytes(drawn, width, &text).as_bytes())?;
        stdout.flush()?;
        drawn = Some((text.lines().count(), width));
        if wait_for_quit(interval)? {
            return Ok(());
        }
        info::refresh_live(&mut system, &mut sampler);
    }
}

/// Raw mode and a hidden cursor for the length of a watch. Dropping it restores the
/// terminal on every exit path, errors included.
struct WatchTerminal;

impl WatchTerminal {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), cursor::Hide) {
            let _ = terminal::disable_raw_mode();
            return Err(error.into());
        }
        Ok(Self)
    }
}

impl Drop for WatchTerminal {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), cursor::Show);
        let _ = terminal::disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = stdout.write_all(b"\r\n");
        let _ = stdout.flush();
    }
}

fn is_watch_quit(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q' | 'Q') | KeyCode::Esc)
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

/// Waits up to `interval` for input and returns `true` when the user asked to quit. Each
/// read is followed by a drain of the pending events, and the wait ends at the deadline,
/// so a burst of resize events does not shorten it.
fn wait_for_quit(interval: Duration) -> Result<bool> {
    let deadline = Instant::now() + interval;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !event::poll(remaining)? {
            return Ok(false);
        }
        loop {
            let quit = matches!(
                event::read()?,
                Event::Key(key) if key.kind == KeyEventKind::Press && is_watch_quit(key)
            );
            if quit {
                return Ok(true);
            }
            if !event::poll(Duration::ZERO)? {
                break;
            }
        }
    }
}

/// Bytes that draw `text` over the frame on screen. `drawn` holds the row count and the
/// width of that frame, or `None` before the first one. A width change clears the screen,
/// and a shorter frame clears the rows it leaves behind. Raw mode is on while drawing, so
/// every line ends with a carriage return.
fn frame_bytes(drawn: Option<(usize, u16)>, width: u16, text: &str) -> String {
    let rows = text.lines().count();
    let mut out = String::new();
    let previous = match drawn {
        Some((_, drawn_width)) if drawn_width != width => {
            out.push_str("\x1b[2J\x1b[H");
            None
        }
        other => other,
    };
    if let Some((previous_rows, _)) = previous {
        if previous_rows > 0 {
            let up = u16::try_from(previous_rows).unwrap_or(u16::MAX);
            let _ = cursor::MoveUp(up).write_ansi(&mut out);
        }
        out.push('\r');
    }
    for line in text.lines() {
        out.push_str(line);
        out.push_str("\x1b[K\r\n");
    }
    if previous.is_some_and(|(previous_rows, _)| previous_rows > rows) {
        out.push_str("\x1b[J");
    }
    out
}

fn render_monitor(frame: &mut Frame, area: Rect, lines: &[Line], interval_ms: u64) {
    frame.render_widget(
        Paragraph::new(Text::from(render::to_ratatui(lines))).block(
            Block::default()
                .title(format!(" HaloFetch · live {interval_ms}ms "))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(BORDER_COLOR)),
        ),
        area,
    );
}

fn render_shell(frame: &mut Frame, area: Rect, screen: &vt100::Screen) {
    let block = Block::default()
        .title(" Shell · Ctrl+Q closes workspace ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SHELL_BORDER_COLOR));
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
            // An empty symbol prints nothing, so every cell after it on the row would be
            // drawn one column to the left.
            let contents = source.contents();
            let symbol = if contents.is_empty() { " " } else { &contents };
            if let Some(target) = frame.buffer_mut().cell_mut((inner.x + col, inner.y + row)) {
                target.set_symbol(symbol).set_style(style);
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

/// Cursor position requests in `chunk`, counting one split across reads. `tail` keeps
/// the bytes a request may continue from and is updated for the next read.
fn cursor_queries(tail: &mut Vec<u8>, chunk: &[u8]) -> usize {
    tail.extend_from_slice(chunk);
    let count = tail
        .windows(CURSOR_QUERY.len())
        .filter(|window| *window == CURSOR_QUERY)
        .count();
    let keep = tail.len().saturating_sub(CURSOR_QUERY.len() - 1);
    tail.drain(..keep);
    count
}

/// Answer to a cursor position request, 1-based as the terminal reports it.
fn cursor_report(row: u16, col: u16) -> String {
    format!("\x1b[{};{}R", row + 1, col + 1)
}

/// Bytes sent to the shell for a pasted text. The text is wrapped in bracketed paste
/// markers only when the shell enabled them, and an end marker inside the text is
/// removed so it cannot end the paste early.
fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    if !bracketed {
        return text.as_bytes().to_vec();
    }
    let body = text.replace(PASTE_END, "");
    let mut bytes = Vec::with_capacity(body.len() + PASTE_START.len() + PASTE_END.len());
    bytes.extend_from_slice(PASTE_START.as_bytes());
    bytes.extend_from_slice(body.as_bytes());
    bytes.extend_from_slice(PASTE_END.as_bytes());
    bytes
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
                _ => 24,
            };
            bytes.extend_from_slice(format!("\x1b[{suffix}~").as_bytes());
        }
        _ => {}
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::{
        cursor_queries, cursor_report, frame_bytes, key_bytes, paste_bytes, shell_size, split,
        windows_shell, windows_shell_args,
    };
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::layout::Rect;
    use std::path::{Path, PathBuf};

    /// A `PATH` lookup that finds exactly the listed names.
    fn on_path(names: &'static [&'static str]) -> impl Fn(&str) -> Option<PathBuf> {
        move |name| {
            names
                .contains(&name)
                .then(|| PathBuf::from(format!(r"C:\bin\{name}")))
        }
    }

    #[test]
    fn region_fits_its_content_and_leaves_the_rest_to_the_shell() {
        let [monitor, shell] = split(Rect::new(0, 0, 120, 40), 10);
        assert_eq!((monitor.height, shell.height), (12, 28));
        assert_eq!(monitor.height + shell.height, 40);
    }

    #[test]
    fn region_is_capped_at_sixty_percent_of_the_screen() {
        let [monitor, shell] = split(Rect::new(0, 0, 120, 40), 100);
        assert_eq!((monitor.height, shell.height), (24, 16));
    }

    #[test]
    fn shell_size_is_the_inside_of_the_border_and_never_zero() {
        assert_eq!(shell_size(Rect::new(0, 0, 80, 30)), (28, 78));
        assert_eq!(shell_size(Rect::new(0, 0, 1, 2)), (1, 1));
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
        assert_eq!(
            key_bytes(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE)),
            b"\x1b[24~"
        );
    }

    #[test]
    fn cursor_queries_are_found_even_when_split_across_reads() {
        let mut tail = Vec::new();
        assert_eq!(cursor_queries(&mut tail, b"\x1b[6n"), 1);
        assert_eq!(cursor_queries(&mut tail, b"ab\x1b"), 0);
        assert_eq!(cursor_queries(&mut tail, b"["), 0);
        assert_eq!(cursor_queries(&mut tail, b"6nxx\x1b[6n"), 2);
        assert_eq!(cursor_queries(&mut tail, b"\x1b[5n"), 0);
        assert_eq!(cursor_report(0, 0), "\x1b[1;1R");
        assert_eq!(cursor_report(4, 11), "\x1b[5;12R");
    }

    #[test]
    fn paste_is_raw_unless_the_shell_asked_for_brackets() {
        assert_eq!(paste_bytes("ls -l", false), b"ls -l");
        assert_eq!(paste_bytes("ls -l", true), b"\x1b[200~ls -l\x1b[201~");
    }

    #[test]
    fn paste_cannot_end_itself_early() {
        assert_eq!(paste_bytes("a\x1b[201~b", true), b"\x1b[200~ab\x1b[201~");
    }

    #[test]
    fn windows_shell_uses_the_environment_shell_first() {
        let shell = windows_shell(
            Some(PathBuf::from(r"C:\tools\bash.exe")),
            on_path(&["pwsh.exe"]),
            None,
        );
        assert_eq!(shell, PathBuf::from(r"C:\tools\bash.exe"));
    }

    #[test]
    fn windows_shell_prefers_pwsh_then_windows_powershell() {
        let both = on_path(&["pwsh.exe", "powershell.exe"]);
        assert_eq!(
            windows_shell(None, &both, None),
            PathBuf::from(r"C:\bin\pwsh.exe")
        );
        let legacy = on_path(&["powershell.exe"]);
        assert_eq!(
            windows_shell(None, &legacy, None),
            PathBuf::from(r"C:\bin\powershell.exe")
        );
    }

    #[test]
    fn windows_shell_falls_back_to_comspec_then_cmd() {
        let none = on_path(&[]);
        let comspec = PathBuf::from(r"C:\Windows\system32\cmd.exe");
        assert_eq!(windows_shell(None, &none, Some(comspec.clone())), comspec);
        assert_eq!(windows_shell(None, &none, None), PathBuf::from("cmd.exe"));
    }

    #[test]
    fn unix_style_shells_get_the_interactive_option() {
        let bash = Path::new(r"C:\Program Files\Git\usr\bin\bash.exe");
        assert_eq!(windows_shell_args(bash), &["-i"][..]);
        assert_eq!(
            windows_shell_args(Path::new("C:/tools/Zsh.EXE")),
            &["-i"][..]
        );
        assert_eq!(windows_shell_args(Path::new("bash")), &["-i"][..]);
    }

    #[test]
    fn fish_also_gets_the_greeting_workaround() {
        assert_eq!(
            windows_shell_args(Path::new(r"C:\bin\fish.exe")),
            &[
                "-i",
                "--features=no-query-term",
                "-C",
                "functions --erase fish_greeting"
            ][..]
        );
    }

    #[test]
    fn powershell_and_cmd_get_no_arguments() {
        for shell in [
            r"C:\Program Files\PowerShell\7\pwsh.exe",
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
            r"C:\Windows\system32\cmd.exe",
        ] {
            assert!(windows_shell_args(Path::new(shell)).is_empty(), "{shell}");
        }
    }

    #[test]
    fn first_watch_frame_moves_nothing() {
        assert_eq!(
            frame_bytes(None, 80, "one\ntwo\n"),
            "one\x1b[K\r\ntwo\x1b[K\r\n"
        );
    }

    #[test]
    fn watch_frame_of_the_same_height_moves_up_over_the_previous_one() {
        assert!(frame_bytes(Some((2, 80)), 80, "one\ntwo\n").starts_with("\x1b[2A\r"));
    }

    #[test]
    fn shorter_watch_frame_clears_the_rows_below() {
        assert!(frame_bytes(Some((3, 80)), 80, "one\n").ends_with("\x1b[J"));
        assert!(!frame_bytes(Some((1, 80)), 80, "one\ntwo\n").ends_with("\x1b[J"));
    }

    #[test]
    fn watch_frame_after_a_width_change_clears_the_screen() {
        let frame = frame_bytes(Some((2, 80)), 100, "one\ntwo\n");
        assert!(frame.starts_with("\x1b[2J\x1b[H"));
        assert!(!frame.contains("\x1b[2A"));
    }
}
