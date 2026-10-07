mod benchmark;
mod cli;
mod config;
mod field;
mod info;
mod live;
mod logo;
mod output;
mod render;
mod theme;
mod tui;
mod update;

use std::fmt::Write as _;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use clap::Parser;
use color_eyre::eyre::{bail, eyre};
use color_eyre::Result;

use cli::{Args, Command, ConfigAction, LogosAction, OutputFormat, PresetAction};
use config::{Config, Scene, StartupMode};
use render::scene::RenderCtx;
use render::{Line, Span, Style};
use theme::Color;

/// Static output width when neither a terminal nor `$COLUMNS` tells us.
const DEFAULT_WIDTH: usize = 100;
/// Width of the palette names column in `preset list`.
const PRESET_NAME_WIDTH: usize = 20;

fn main() -> Result<()> {
    color_eyre::install()?;
    let args = Args::parse();
    if let Some(path) = &args.config {
        config::set_config_path(path.clone());
    }
    match &args.command {
        Some(command) => run_command(&args, command),
        None => run_default(&args),
    }
}

fn run_command(args: &Args, command: &Command) -> Result<()> {
    match command {
        Command::Fetch => print_fetch(args, &Config::load()?),
        Command::Monitor { interval } => {
            require_terminal("monitor")?;
            let cfg = Config::load()?;
            let interval = interval.unwrap_or(cfg.startup.interval_ms);
            live::run(&cfg, interval, scene_for(args, &cfg))
        }
        Command::Config { action } => match action {
            None | Some(ConfigAction::Edit) => edit_config(false),
            Some(ConfigAction::Path) => print_config_path(),
            Some(ConfigAction::Reset) => reset_config(),
        },
        Command::Preset { action } => match action {
            PresetAction::List => list_presets(),
            PresetAction::Apply { name } => apply_preset(name),
        },
        Command::Logos { action } => match action {
            LogosAction::List => list_logos(),
            LogosAction::Show { key } => show_logo(key.as_deref()),
        },
        Command::Benchmark { iterations } => {
            let cfg = Config::load()?;
            benchmark::run(&cfg, scene_for(args, &cfg), *iterations)
        }
        Command::Update => update::run(),
    }
}

/// No subcommand: the legacy flags, then the startup mode. Without a configuration
/// file in an interactive session, the setup editor runs first.
fn run_default(args: &Args) -> Result<()> {
    if args.setup {
        return edit_config(false);
    }
    if args.reset {
        return reset_config();
    }
    if args.list_presets {
        return list_presets();
    }
    if let Some(name) = &args.preset {
        return apply_preset(name);
    }
    if args.update {
        return update::run();
    }
    if args.just_ascii {
        return show_logo(None);
    }
    if matches!(args.format, OutputFormat::Json) {
        return print_json();
    }

    let interactive = is_interactive();
    let cfg = if interactive && !Config::exists() {
        first_run_setup()?
    } else {
        Config::load()?
    };
    if interactive && cfg.startup.mode == StartupMode::Monitor {
        return live::run(&cfg, cfg.startup.interval_ms, scene_for(args, &cfg));
    }
    print_static(&cfg, scene_for(args, &cfg))
}

fn scene_for(args: &Args, cfg: &Config) -> Scene {
    args.scene.unwrap_or(cfg.scene)
}

fn is_interactive() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn require_terminal(what: &str) -> Result<()> {
    if is_interactive() {
        Ok(())
    } else {
        bail!("{what} needs an interactive terminal (stdin and stdout must be a TTY)")
    }
}

/// Runs the editor for a configuration that does not exist yet. Declining to save
/// keeps the defaults.
fn first_run_setup() -> Result<Config> {
    match tui::run(Config::load()?, true)? {
        Some(saved) => {
            saved.save()?;
            Ok(saved)
        }
        None => Config::load(),
    }
}

fn print_fetch(args: &Args, cfg: &Config) -> Result<()> {
    match args.format {
        OutputFormat::Json => print_json(),
        OutputFormat::Ansi => print_static(cfg, scene_for(args, cfg)),
    }
}

fn print_static(cfg: &Config, scene: Scene) -> Result<()> {
    let lines = render_static(cfg, scene, static_width());
    let text = if plain_output() {
        render::to_plain(&lines)
    } else {
        render::to_ansi(&lines)
    };
    emit(&text)
}

/// The static fetch: detect the system, pick the logo that fits, lay out the scene.
fn render_static(cfg: &Config, scene: Scene, width: usize) -> Vec<Line> {
    let system = info::collect();
    let logos = cfg.logo_set(&system);
    let ctx = RenderCtx {
        info: &system,
        cfg,
        logos: &logos,
        width,
    };
    render::scene::render(scene, &ctx)
}

fn print_json() -> Result<()> {
    let system = info::collect();
    emit(&format!("{}\n", output::system_info_json(&system)?))
}

fn static_width() -> usize {
    if io::stdout().is_terminal() {
        if let Ok((columns, _)) = crossterm::terminal::size() {
            if columns > 0 {
                return usize::from(columns);
            }
        }
    }
    std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|&columns| columns > 0)
        .unwrap_or(DEFAULT_WIDTH)
}

/// Colors are dropped only for piped output that asked for `NO_COLOR`.
fn plain_output() -> bool {
    !io::stdout().is_terminal()
        && std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty())
}

/// Writes `text` to stdout. A closed pipe, as in `atlasfetch logos list | head`, is
/// not an error.
fn emit(text: &str) -> Result<()> {
    let mut stdout = io::stdout().lock();
    match stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
    {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn edit_config(first_run: bool) -> Result<()> {
    require_terminal("the configuration editor")?;
    let cfg = Config::load()?;
    if let Some(saved) = tui::run(cfg, first_run)? {
        saved.save()?;
    }
    Ok(())
}

fn print_config_path() -> Result<()> {
    emit(&format!("{}\n", config::config_path()?.display()))
}

fn reset_config() -> Result<()> {
    require_terminal("config reset")?;
    let path = config::config_path()?;
    if path.exists() {
        emit(&format!(
            "Reset configuration at {}? [y/N] ",
            path.display()
        ))?;
        if !read_yes()? {
            return emit("Reset cancelled.\n");
        }
        let backup = backup_path(&path, |candidate| candidate.exists());
        fs::rename(&path, &backup)?;
        emit(&format!(
            "Previous configuration moved to {}\n",
            backup.display()
        ))?;
    }
    edit_config(true)
}

fn read_yes() -> Result<bool> {
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

/// `path.bak`, or `path.bak.N` for the first free N, so older backups are kept.
fn backup_path(path: &Path, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let base = with_suffix(path, "bak");
    if !exists(&base) {
        return base;
    }
    (1..)
        .map(|n| with_suffix(path, &format!("bak.{n}")))
        .find(|candidate| !exists(candidate))
        .unwrap_or(base)
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(suffix);
    PathBuf::from(name)
}

fn list_presets() -> Result<()> {
    let cfg = Config::load()?;
    let mut out = String::from("Available presets:\n");
    for preset in theme::all_themes() {
        writeln!(
            out,
            "  {}  {}",
            padded(preset.name, PRESET_NAME_WIDTH),
            swatch(&preset.colors)
        )?;
    }
    if !cfg.custom_palettes.is_empty() {
        out.push_str("Custom palettes:\n");
        for (name, colors) in &cfg.custom_palettes {
            writeln!(
                out,
                "  {}  {}",
                padded(name, PRESET_NAME_WIDTH),
                swatch(colors)
            )?;
        }
    }
    emit(&out)
}

/// One block of color per palette entry, drawn with the shared canvas.
fn swatch(colors: &[Color]) -> String {
    let spans = colors
        .iter()
        .map(|&color| Span::new("  ", Style::new().bg(color)))
        .collect();
    let line = Line::from_spans(spans);
    render::to_ansi(&[line]).trim_end().to_string()
}

fn padded(text: &str, width: usize) -> String {
    let gap = width.saturating_sub(render::text_width(text));
    format!("{text}{}", " ".repeat(gap))
}

fn apply_preset(name: &str) -> Result<()> {
    let mut cfg = Config::load()?;
    let colors = match theme::find_theme(name) {
        Some(theme) => theme.colors,
        None => cfg
            .custom_palettes
            .get(name)
            .cloned()
            .ok_or_else(|| eyre!("preset '{name}' not found; use 'atlasfetch preset list'"))?,
    };
    cfg.colors.palette = colors;
    cfg.save()?;
    emit(&format!("Preset \"{name}\" applied.\n"))
}

fn list_logos() -> Result<()> {
    let keys = logo::available(config::user_logo_dir().as_deref());
    let mut out = String::new();
    for key in keys {
        out.push_str(&key);
        out.push('\n');
    }
    emit(&out)
}

/// `key` names an embedded or user logo. Without a key, the logo the configuration
/// selects for this machine is shown.
fn show_logo(key: Option<&str>) -> Result<()> {
    let cfg = Config::load()?;
    let shown = match key {
        Some(key) => logo::load_key(key, config::user_logo_dir().as_deref())
            .ok_or_else(|| eyre!("logo '{key}' not found; use 'atlasfetch logos list'"))?,
        None => {
            let system = info::collect();
            let logos = cfg.logo_set(&system);
            logos
                .fitting(usize::MAX)
                .cloned()
                .ok_or_else(|| eyre!("the configured logo source has no logo"))?
        }
    };
    let lines = logo::colorize(&shown, &cfg.colors.palette, cfg.logo.gradient);
    emit(&render::to_ansi(&lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_takes_the_first_free_name() {
        let path = Path::new("/home/u/.config/atlasfetch/config.json");
        let free = backup_path(path, |_| false);
        assert_eq!(
            free,
            Path::new("/home/u/.config/atlasfetch/config.json.bak")
        );

        let taken = |candidate: &Path| {
            candidate.ends_with("config.json.bak") || candidate.ends_with("config.json.bak.1")
        };
        let third = backup_path(path, taken);
        assert_eq!(
            third,
            Path::new("/home/u/.config/atlasfetch/config.json.bak.2")
        );
    }

    #[test]
    fn padding_uses_display_width() {
        assert_eq!(padded("abc", 5), "abc  ");
        assert_eq!(padded("abcdef", 3), "abcdef");
    }

    #[test]
    fn swatch_has_one_block_per_color() {
        let colors = [Color::new(1, 2, 3), Color::new(4, 5, 6)];
        let text = swatch(&colors);
        assert!(text.contains("\x1b[48;2;1;2;3m"));
        assert!(text.contains("\x1b[48;2;4;5;6m"));
        assert!(!text.ends_with('\n'));
    }
}
