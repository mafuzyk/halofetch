//! Interactive setup editor. [`run`] takes over the terminal until the user saves or quits.

mod app;
mod input;
mod view;

use std::fs;
use std::io::{self, Stdout};
use std::time::{Duration, Instant};

use color_eyre::Result;
use crossterm::event::{self, DisableBracketedPaste, EnableBracketedPaste};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::config::{self, Config};
use crate::info;

use app::{App, Outcome};

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const CUSTOM_LOGO_FILE: &str = "custom-logo.txt";

/// Raw mode, alternate screen and bracketed paste for as long as the value lives.
/// Dropping it restores the terminal, including while a panic unwinds.
struct TerminalSession;

impl TerminalSession {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let session = TerminalSession;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableBracketedPaste,
            cursor::Hide
        )?;
        Ok(session)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            cursor::Show,
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        let _ = terminal::disable_raw_mode();
    }
}

/// Runs the editor. Returns the configuration when the user saved, `None` when they quit
/// without saving. Pasted logo art is written next to the configuration before returning.
pub fn run(cfg: Config, first_run: bool) -> Result<Option<Config>> {
    let _session = TerminalSession::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut app = App::new(cfg, first_run, info::collect());
    draw(&mut terminal, &mut app)?;

    while app.outcome().is_none() {
        let redraw = if event::poll(POLL_INTERVAL)? {
            app.handle_event(event::read()?);
            true
        } else {
            app.tick(Instant::now())
        };
        if redraw {
            draw(&mut terminal, &mut app)?;
        }
    }

    match app.outcome() {
        Some(Outcome::Save) => {
            if let Some(text) = app.pending_logo_text() {
                write_custom_logo(text)?;
            }
            Ok(Some(app.cfg.clone()))
        }
        _ => Ok(None),
    }
}

fn draw(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    terminal.draw(|frame| view::draw(frame, app))?;
    Ok(())
}

fn write_custom_logo(text: &str) -> Result<()> {
    let dir = config::config_dir()?;
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(CUSTOM_LOGO_FILE), format!("{text}\n"))?;
    Ok(())
}
