// TUI setup configurator — launched via `atlasfetch setup`.
//
// Built with ratatui + crossterm.

mod editor;
mod events;
mod state;

use color_eyre::Result;

pub fn run(cfg: &mut crate::config::Config) -> Result<()> {
    editor::run(cfg)
}
