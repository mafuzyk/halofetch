mod atomic;
mod benchmark;
mod cli;
mod config;
mod fastfetch;
mod field;
mod image;
mod info;
mod init;
mod live;
mod logo;
mod output;
mod render;
mod theme;
mod tui;
mod update;

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Parser;
use color_eyre::config::{HookBuilder, Theme};
use color_eyre::eyre::{bail, eyre};
use color_eyre::Result;

use cli::{Args, Command, ConfigAction, LogosAction, OutputFormat, PresetAction};
use config::{Config, Scene, StartupMode};
use logo::LogoSource;
use render::scene::RenderCtx;
use render::{Line, Span, Style};
use theme::Color;

/// Static output width when neither a terminal nor `$COLUMNS` tells us.
const DEFAULT_WIDTH: usize = 100;
/// Width of the palette names column in `preset list`.
const PRESET_NAME_WIDTH: usize = 20;

fn main() -> Result<()> {
    install_error_reports()?;
    let args = Args::parse();
    if let Some(path) = &args.config {
        config::set_config_path(path.clone());
        if !path.exists() && !creates_config(&args) {
            bail!(
                "configuration file not found: {}\nCreate it with `halofetch --config {} config`.",
                path.display(),
                path.display()
            );
        }
    }
    match &args.command {
        Some(command) => run_command(&args, command),
        None => run_default(&args),
    }
}

/// Errors are meant for users, so they name no source location. Panics keep the
/// standard hook, which does.
fn install_error_reports() -> Result<()> {
    let no_color = std::env::var_os("NO_COLOR");
    let colored =
        io::stderr().is_terminal() && !plain_output_for(ansi_supported(), no_color.as_deref());
    let theme = if colored { Theme::dark() } else { Theme::new() };
    let (_, eyre_hook) = HookBuilder::default()
        .theme(theme)
        .display_location_section(false)
        .display_env_section(false)
        .into_hooks();
    eyre_hook.install()?;
    Ok(())
}

/// Whether this invocation writes the file given with `--config`, so it may not exist yet.
fn creates_config(args: &Args) -> bool {
    args.setup
        || args.reset
        || args.preset.is_some()
        || matches!(
            args.command,
            Some(Command::Config { .. })
                | Some(Command::Init { .. })
                | Some(Command::Preset {
                    action: PresetAction::Apply { .. }
                })
        )
}

fn run_command(args: &Args, command: &Command) -> Result<()> {
    match command {
        Command::Fetch { watch: false, .. } => {
            let mut cfg = Config::load()?;
            apply_image_override(&mut cfg, args);
            print_fetch(args, &cfg)
        }
        Command::Fetch {
            watch: true,
            interval,
        } => {
            if matches!(args.format, OutputFormat::Json) {
                return Err(eyre!("--watch cannot be combined with --format json"));
            }
            if args.image.is_some() {
                return Err(eyre!("--image is only supported by the static fetch"));
            }
            require_terminal("fetch --watch")?;
            let cfg = Config::load()?;
            let interval = interval.unwrap_or(cfg.startup.interval_ms);
            live::watch(&cfg, scene_for(args, &cfg), Duration::from_millis(interval))
        }
        Command::Monitor { interval } => {
            if matches!(args.format, OutputFormat::Json) {
                return Err(eyre!("monitor cannot be combined with --format json"));
            }
            if args.image.is_some() {
                return Err(eyre!("--image is only supported by the static fetch"));
            }
            require_terminal("monitor")?;
            let cfg = Config::load()?;
            let interval = interval.unwrap_or(cfg.startup.interval_ms);
            live::run(&cfg, interval, scene_for(args, &cfg))
        }
        Command::Config { action } => match action {
            None | Some(ConfigAction::Edit) => edit_config(false),
            Some(ConfigAction::Path) => print_config_path(),
            Some(ConfigAction::Reset) => reset_config(),
            Some(ConfigAction::ImportFastfetch { file, dry_run }) => {
                import_fastfetch(file.as_deref(), *dry_run)
            }
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
        Command::Update { release } => update::run(*release),
        Command::Init { shell, remove } => run_init(*shell, *remove),
    }
}

fn run_init(shell: init::Shell, remove: bool) -> Result<()> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf()))
        .ok_or_else(|| eyre!("cannot determine the home directory"))?;
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute());
    let sharky = std::env::var_os("SHARKY_CONFIG").map(PathBuf::from);
    let zdotdir = std::env::var_os("ZDOTDIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute());
    let path = init::config_path(
        shell,
        &home,
        xdg.as_deref(),
        sharky.as_deref(),
        zdotdir.as_deref(),
    );
    let outcome = if remove {
        init::remove(shell, &path)?
    } else {
        init::install(shell, &path)?
    };
    let name = shell.label();
    let message = match outcome {
        init::Outcome::Created | init::Outcome::Updated => {
            format!(
                "Added the HaloFetch startup hook for {name} to {}.",
                path.display()
            )
        }
        init::Outcome::AlreadyPresent => {
            format!(
                "The HaloFetch startup hook for {name} is already in {}.",
                path.display()
            )
        }
        init::Outcome::Removed => {
            format!(
                "Removed the HaloFetch startup hook for {name} from {}.",
                path.display()
            )
        }
        init::Outcome::NotPresent => {
            format!(
                "No HaloFetch startup hook for {name} in {}.",
                path.display()
            )
        }
    };
    emit(&format!("{message}\n"))
}

fn import_fastfetch(file: Option<&Path>, dry_run: bool) -> Result<()> {
    let source = match file {
        Some(path) => path.to_path_buf(),
        None => fastfetch::default_path()
            .or_else(fastfetch::preferred_path)
            .ok_or_else(|| eyre!("no fastfetch configuration found; pass a file"))?,
    };
    if !source.is_file() {
        bail!("fastfetch configuration not found: {}", source.display());
    }

    let import = fastfetch::parse(&fs::read_to_string(&source)?)?;
    let mut config = import.config;
    config.normalize();
    config.validate()?;
    let destination = config::config_path()?;

    if let Some(data) = &import.logo_data {
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let logo_dir =
            std::path::absolute(parent.join("logos")).unwrap_or_else(|_| parent.join("logos"));
        let logo_path = logo_dir.join("fastfetch-logo.txt");
        if dry_run {
            if logo_path.exists() {
                let backup = backup_path(&logo_path, |candidate| candidate.exists());
                emit(&format!(
                    "Would back up the old logo to {}.\n",
                    backup.display()
                ))?;
            }
            emit(&format!(
                "Would write the inline logo to {}.\n",
                logo_path.display()
            ))?;
        } else {
            fs::create_dir_all(&logo_dir)?;
            if logo_path.exists() {
                let backup = backup_path(&logo_path, |candidate| candidate.exists());
                fs::copy(&logo_path, &backup)?;
                emit(&format!("Logo backup: {}\n", backup.display()))?;
            }
            crate::atomic::write(&logo_path, format!("{data}\n").as_bytes())?;
            config.logo.source = LogoSource::File {
                path: logo_path.to_string_lossy().into_owned(),
            };
            emit(&format!("Inline logo: {}\n", logo_path.display()))?;
        }
    }

    if dry_run {
        if destination.exists() {
            let backup = backup_path(&destination, |candidate| candidate.exists());
            emit(&format!(
                "Would back up the old configuration to {}.\n",
                backup.display()
            ))?;
        }
        emit(&format!(
            "Would import {} into {}.\n",
            source.display(),
            destination.display()
        ))?;
    } else {
        if destination.exists() {
            let backup = backup_path(&destination, |candidate| candidate.exists());
            fs::copy(&destination, &backup)?;
            emit(&format!("Backup: {}\n", backup.display()))?;
        }
        config.save()?;
        emit(&format!(
            "Imported {} into {}.\n",
            source.display(),
            destination.display()
        ))?;
    }

    if !import.mapped.is_empty() {
        emit(&format!("Mapped: {}\n", import.mapped.join(", ")))?;
    }
    if !import.skipped.is_empty() {
        emit(&format!("Skipped: {}\n", import.skipped.join(", ")))?;
    }
    for warning in &import.warnings {
        emit(&format!("Warning: {warning}\n"))?;
    }
    Ok(())
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
        return update::run(false);
    }
    if args.just_ascii {
        return show_logo(None);
    }
    if matches!(args.format, OutputFormat::Json) {
        if args.image.is_some() {
            return Err(eyre!("--image cannot be combined with --format json"));
        }
        return print_json(&Config::load()?);
    }

    let interactive = is_interactive();
    let mut cfg = if interactive && !Config::exists() {
        first_run_setup()?
    } else {
        Config::load()?
    };
    if interactive && cfg.startup.mode == StartupMode::Monitor {
        if args.image.is_some() {
            return Err(eyre!("--image is only supported by the static fetch"));
        }
        return live::run(&cfg, cfg.startup.interval_ms, scene_for(args, &cfg));
    }
    apply_image_override(&mut cfg, args);
    print_static(&cfg, scene_for(args, &cfg))
}

/// `--image` replaces the configured path but keeps the configured protocol and
/// size. An explicit flag must not silently switch a disabled image back on.
fn apply_image_override(cfg: &mut Config, args: &Args) {
    let Some(path) = &args.image else {
        return;
    };
    cfg.logo.image.path = path.to_string_lossy().into_owned();
    if !path.is_file() {
        eprintln!(
            "halofetch: image {} not found; using the character logo",
            path.display()
        );
    }
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
        Err(eyre!(
            "{what} needs an interactive terminal (stdin and stdout must be a TTY)"
        ))
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
        OutputFormat::Json => {
            if args.image.is_some() {
                return Err(eyre!("--image cannot be combined with --format json"));
            }
            print_json(cfg)
        }
        OutputFormat::Ansi => print_static(cfg, scene_for(args, cfg)),
    }
}

fn print_static(cfg: &Config, scene: Scene) -> Result<()> {
    let plain = plain_output();
    let (lines, image, fallback) = render_fetch(cfg, scene, static_width(), !plain);
    let text = if plain {
        render::to_plain(&lines)
    } else if let Some(image) = image {
        match image::render(lines, &image) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("halofetch: {error}; using the character logo");
                let lines = fallback.unwrap_or_else(|| render_static(cfg, scene, static_width()));
                render::to_ansi(&lines)
            }
        }
    } else {
        render::to_ansi(&lines)
    };
    emit(&text)
}

/// The static fetch: detect the system, pick the logo that fits, lay out the scene.
fn render_static(cfg: &Config, scene: Scene, width: usize) -> Vec<Line> {
    render_fetch(cfg, scene, width, false).0
}

/// Lays out the scene, optionally with an image marker in place of the logo. The
/// third value is the same scene rendered with the original character logo, so a
/// failed image render can fall back without losing the logo.
fn render_fetch(
    cfg: &Config,
    scene: Scene,
    width: usize,
    allow_image: bool,
) -> (Vec<Line>, Option<image::ImageSource>, Option<Vec<Line>>) {
    let system = info::collect_for(cfg.needed_fields());
    let (logos, image) = if allow_image {
        cfg.fetch_logo_set(&system)
    } else {
        (cfg.logo_set(&system), None)
    };
    let ctx = RenderCtx {
        info: &system,
        cfg,
        logos: &logos,
        width,
    };
    let lines = render::scene::render(scene, &ctx);
    let fallback = image.as_ref().map(|_| {
        let logos = cfg.logo_set(&system);
        let ctx = RenderCtx {
            info: &system,
            cfg,
            logos: &logos,
            width,
        };
        render::scene::render(scene, &ctx)
    });
    (lines, image, fallback)
}

fn print_json(cfg: &Config) -> Result<()> {
    let system = info::collect_for(cfg.needed_fields());
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

/// Plain text outside a capable terminal, where the console cannot show ANSI escapes, and
/// wherever `NO_COLOR` is set to a non-empty value, terminal or not.
fn plain_output() -> bool {
    !colors_wanted() || plain_output_for(ansi_supported(), std::env::var_os("NO_COLOR").as_deref())
}

/// Whether stdout should carry escapes before `NO_COLOR` is considered. A non-empty
/// `FORCE_COLOR` keeps them for pipes such as `halofetch | less -R`.
fn color_target(terminal: bool, term: Option<&OsStr>, force_color: Option<&OsStr>) -> bool {
    force_color.is_some_and(|value| !value.is_empty())
        || (terminal && term != Some(OsStr::new("dumb")))
}

fn colors_wanted() -> bool {
    color_target(
        io::stdout().is_terminal(),
        std::env::var_os("TERM").as_deref(),
        std::env::var_os("FORCE_COLOR").as_deref(),
    )
}

/// The plain-text rule. Pure, so it is tested without changing the environment.
fn plain_output_for(ansi_supported: bool, no_color: Option<&OsStr>) -> bool {
    !ansi_supported || no_color.is_some_and(|value| !value.is_empty())
}

/// Legacy Windows consoles show escapes as garbage until virtual terminal processing is
/// on. crossterm turns it on here and caches the answer.
#[cfg(windows)]
fn ansi_supported() -> bool {
    crossterm::ansi_support::supports_ansi()
}

#[cfg(not(windows))]
fn ansi_supported() -> bool {
    true
}

/// Rendered lines for the terminal: ANSI, or plain text where `plain_output` says so.
pub(crate) fn styled_text(lines: &[Line]) -> String {
    if plain_output() {
        render::to_plain(lines)
    } else {
        render::to_ansi(lines)
    }
}

/// Writes `text` to stdout. A closed pipe, as in `halofetch logos list | head`, is
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
    let colored = ansi_supported() && colors_wanted();
    let mut out = String::from("Available presets:\n");
    for preset in theme::all_themes() {
        writeln!(
            out,
            "  {}  {}",
            padded(preset.name, PRESET_NAME_WIDTH),
            swatch(&preset.colors, colored)
        )?;
    }
    if !cfg.custom_palettes.is_empty() {
        out.push_str("Custom palettes:\n");
        for (name, colors) in &cfg.custom_palettes {
            writeln!(
                out,
                "  {}  {}",
                padded(name, PRESET_NAME_WIDTH),
                swatch(colors, colored)
            )?;
        }
    }
    emit(&out)
}

/// One block of color per palette entry, drawn with the shared canvas.
fn swatch(colors: &[Color], colored: bool) -> String {
    let spans = colors
        .iter()
        .map(|&color| Span::new("  ", Style::new().bg(color)))
        .collect();
    let line = Line::from_spans(spans);
    // The colors are the content here, so `NO_COLOR` does not blank them.
    let text = if colored {
        render::to_ansi(&[line])
    } else {
        render::to_plain(&[line])
    };
    text.trim_end().to_string()
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
            .ok_or_else(|| eyre!("preset '{name}' not found; use 'halofetch preset list'"))?,
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
            .ok_or_else(|| eyre!("logo '{key}' not found; use 'halofetch logos list'"))?,
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
    emit(&styled_text(&lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_takes_the_first_free_name() {
        let path = Path::new("/home/u/.config/halofetch/config.json");
        let free = backup_path(path, |_| false);
        assert_eq!(free, Path::new("/home/u/.config/halofetch/config.json.bak"));

        let taken = |candidate: &Path| {
            candidate.ends_with("config.json.bak") || candidate.ends_with("config.json.bak.1")
        };
        let third = backup_path(path, taken);
        assert_eq!(
            third,
            Path::new("/home/u/.config/halofetch/config.json.bak.2")
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
        let text = swatch(&colors, true);
        assert!(text.contains("\x1b[48;2;1;2;3m"));
        assert!(text.contains("\x1b[48;2;4;5;6m"));
        assert!(!text.ends_with('\n'));
    }

    #[test]
    fn unset_or_empty_no_color_keeps_colors() {
        assert!(!plain_output_for(true, None));
        assert!(!plain_output_for(true, Some(OsStr::new(""))));
    }

    #[test]
    fn no_color_turns_colors_off_on_a_terminal() {
        assert!(plain_output_for(true, Some(OsStr::new("1"))));
    }

    #[test]
    fn consoles_without_ansi_are_always_plain() {
        assert!(plain_output_for(false, None));
        assert!(plain_output_for(false, Some(OsStr::new(""))));
    }

    #[test]
    fn only_a_capable_terminal_gets_colors() {
        assert!(color_target(true, Some(OsStr::new("xterm")), None));
        assert!(color_target(true, None, None));
        assert!(!color_target(false, Some(OsStr::new("xterm")), None));
        assert!(!color_target(true, Some(OsStr::new("dumb")), None));
    }

    #[test]
    fn force_color_keeps_colors_outside_a_terminal() {
        assert!(color_target(false, None, Some(OsStr::new("1"))));
        assert!(!color_target(false, None, Some(OsStr::new(""))));
    }
}
