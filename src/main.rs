// atlasfetch — centered ASCII art with powerline panels
//
// Design: The binary has two modes. The default mode prints system info
// instantly. The `setup` subcommand launches a TUI configurator. Both share
// the same rendering engine so the preview in setup is identical to real
// terminal output.

mod ascii;
mod benchmark;
mod cli;
mod component;
mod config;
mod info;
mod layout;
mod live;
mod output;
mod render;
mod theme;
mod tui;
mod update;
mod widget;

use clap::Parser;
use color_eyre::Result;
use std::io::IsTerminal;

fn main() -> Result<()> {
    color_eyre::install()?;

    let args = cli::Args::parse();

    if let Some(ref cfg_path) = args.config {
        config::set_config_path(cfg_path.clone());
    }

    if let Some(command) = args.command.as_ref() {
        return run_command(command);
    }

    // --list-presets: print and exit
    if args.list_presets {
        list_presets();
        return Ok(());
    }

    // --preset: apply and exit
    if let Some(ref name) = args.preset {
        return apply_preset(name);
    }

    // --update: pull, build, install
    if args.update {
        return update::run();
    }

    // --reset: delete config and launch setup wizard
    if args.reset {
        return reset_config();
    }

    // --scene: override scene
    if let Some(ref s) = args.scene {
        let scene = s.parse::<component::Scene>()?;
        ascii::ensure_logos()?;
        let cfg = config::Config::load()?;
        let info = info::collect()?;
        let ascii_art = ascii::load(&cfg)?;
        let tw = layout::terminal_width();
        let ctx = component::RenderCtx {
            info: &info,
            cfg: &cfg,
            term_width: tw,
            palette: &cfg.logo.colors,
        };
        use component::Component;
        let ascii_comp = component::ascii::AsciiComponent::new(ascii_art.clone());
        let system_comp = component::system::SystemComponent;
        let monitor_comp = component::monitor::MonitorComponent::new();
        let companion_comp = component::companion::CompanionComponent;
        let comps: Vec<&dyn Component> =
            vec![&ascii_comp, &system_comp, &monitor_comp, &companion_comp];
        print!("{}", component::render_scene_ansi(scene, &comps, &ctx));
        return Ok(());
    }

    // --just-ascii: print only the ASCII art
    if args.just_ascii {
        ascii::ensure_logos()?;
        let cfg = config::Config::load()?;
        let ascii_art = ascii::load(&cfg)?;
        print!("{}", render::render_ascii_only(&cfg, &ascii_art));
        return Ok(());
    }

    // setup: launch TUI configurator
    if args.setup {
        return setup();
    }

    if matches!(args.format, cli::OutputFormat::Json) {
        let info = info::collect()?;
        println!("{}", output::system_info_json(&info)?);
        return Ok(());
    }

    // first run: launch setup TUI
    if !config::config_path()?.exists() {
        let mut cfg = config::Config::load()?;
        tui::run(&mut cfg)?;
        cfg.save()?;
        return Ok(());
    }

    let startup_cfg = config::Config::load()?;
    if startup_cfg.live.enabled && std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
    {
        return live::run(startup_cfg.live.interval_ms, None);
    }

    // default: print fetch output
    ascii::ensure_logos()?;
    let cfg = config::Config::load()?;
    let info = info::collect()?;
    let ascii_art = ascii::load(&cfg)?;

    let term_width = layout::terminal_width();
    let scene = cfg.scene;

    let ctx = component::RenderCtx {
        info: &info,
        cfg: &cfg,
        term_width,
        palette: &cfg.logo.colors,
    };

    let output = {
        use component::Component;
        let ascii_comp = component::ascii::AsciiComponent::new(ascii_art.clone());
        let system_comp = component::system::SystemComponent;
        let monitor_comp = component::monitor::MonitorComponent::new();
        let companion_comp = component::companion::CompanionComponent;
        let components: Vec<&dyn Component> =
            vec![&ascii_comp, &system_comp, &monitor_comp, &companion_comp];
        component::render_scene_ansi(scene, &components, &ctx)
    };

    print!("{}", output);
    Ok(())
}

fn run_command(command: &cli::Command) -> Result<()> {
    match command {
        cli::Command::Fetch => render_static_fetch(),
        cli::Command::Config { action: None } => setup(),
        cli::Command::Config {
            action: Some(cli::ConfigAction::Path),
        } => {
            println!("{}", config::config_path()?.display());
            Ok(())
        }
        cli::Command::Config {
            action: Some(cli::ConfigAction::Reset),
        } => reset_config(),
        cli::Command::Preset {
            action: cli::PresetAction::List,
        } => {
            list_presets();
            Ok(())
        }
        cli::Command::Preset {
            action: cli::PresetAction::Apply { name },
        } => apply_preset(name),
        cli::Command::Logos {
            action: cli::LogosAction::List,
        } => {
            for logo in ascii::available_logos()? {
                println!("{logo}");
            }
            Ok(())
        }
        cli::Command::Update => update::run(),
        cli::Command::Benchmark { iterations } => benchmark::run(*iterations),
        cli::Command::Monitor { interval, scene } => live::run(*interval, scene.as_deref()),
    }
}

fn render_static_fetch() -> Result<()> {
    ascii::ensure_logos()?;
    let cfg = config::Config::load()?;
    let system_info = info::collect()?;
    let ascii_art = ascii::load(&cfg)?;
    let ctx = component::RenderCtx {
        info: &system_info,
        cfg: &cfg,
        term_width: layout::terminal_width(),
        palette: &cfg.logo.colors,
    };
    use component::Component;
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
    print!(
        "{}",
        component::render_scene_ansi(cfg.scene, &components, &ctx)
    );
    Ok(())
}

fn list_presets() {
    println!("Available presets:");
    for preset in theme::all_themes() {
        let swatch: String = preset
            .colors
            .iter()
            .map(|color| format!("\x1b[48;2;{};{};{}m  \x1b[0m", color.r, color.g, color.b))
            .collect();
        println!("  {:20} {}", preset.name, swatch);
    }
}

fn apply_preset(name: &str) -> Result<()> {
    let preset = theme::all_themes()
        .into_iter()
        .find(|preset| preset.name == name)
        .ok_or_else(|| {
            color_eyre::eyre::eyre!("preset '{name}' not found; use 'atlasfetch preset list'")
        })?;
    let mut cfg = config::Config::load()?;
    cfg.logo.colors = preset.colors;
    cfg.save()?;
    println!("Preset \"{name}\" applied.");
    Ok(())
}

fn setup() -> Result<()> {
    let mut cfg = config::Config::load()?;
    tui::run(&mut cfg)?;
    cfg.save()
}

fn reset_config() -> Result<()> {
    let path = config::config_path()?;
    if path.exists() {
        std::fs::remove_file(&path)?;
        println!("Config removed.");
    }
    setup()
}
