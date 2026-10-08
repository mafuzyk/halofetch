//! Command-line definitions. Subcommands are the documented interface; the hidden
//! top-level flags are spellings from earlier releases that keep working.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::config::Scene;

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Ansi,
    Json,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print one static fetch and exit
    Fetch {
        /// Keep the scene on screen and redraw it in place until q, Esc or Ctrl+C
        #[arg(short, long)]
        watch: bool,
        /// Refresh interval in milliseconds with --watch [default: startup.interval_ms]
        #[arg(short, long, requires = "watch", value_parser = clap::value_parser!(u64).range(100..=60_000))]
        interval: Option<u64>,
    },
    /// Refresh system metrics in place above an embedded shell
    Monitor {
        /// Refresh interval in milliseconds [default: startup.interval_ms]
        #[arg(short, long, value_parser = clap::value_parser!(u64).range(100..=60_000))]
        interval: Option<u64>,
    },
    /// Edit, print the path of, or reset the configuration
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// List or apply color palettes
    Preset {
        #[command(subcommand)]
        action: PresetAction,
    },
    /// List embedded logos, or show one colored with the current palette
    Logos {
        #[command(subcommand)]
        action: LogosAction,
    },
    /// Measure system information collection and rendering
    Benchmark {
        /// Number of measured runs
        #[arg(short = 'n', long, default_value_t = 5, value_parser = clap::value_parser!(u16).range(1..=100))]
        iterations: u16,
    },
    /// Pull, rebuild, and install from the source checkout
    Update,
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Open the configuration editor
    Edit,
    /// Print the active configuration path
    Path,
    /// Move the configuration aside and open the editor with defaults
    Reset,
}

#[derive(Debug, Subcommand)]
pub enum PresetAction {
    /// List built-in and custom palettes
    List,
    /// Use a palette for the logo colors
    Apply { name: String },
}

#[derive(Debug, Subcommand)]
pub enum LogosAction {
    /// List logo keys
    List,
    /// Print a logo colored with the current palette (the configured logo without KEY)
    Show { key: Option<String> },
}

#[derive(Debug, Parser)]
#[command(
    name = "halofetch",
    about = "Centered ASCII art with powerline panels",
    version
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[cfg_attr(
        not(windows),
        doc = "Configuration file [default: ~/.config/halofetch/config.json]"
    )]
    #[cfg_attr(
        windows,
        doc = r"Configuration file [default: %APPDATA%\halofetch\config.json]"
    )]
    #[arg(short = 'c', long = "config", global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Output format of the system information
    #[arg(long, value_enum, default_value_t, global = true)]
    pub format: OutputFormat,

    /// Scene to render: classic, side, or dashboard [default: configured scene]
    #[arg(long, value_parser = parse_scene, global = true, value_name = "SCENE")]
    pub scene: Option<Scene>,

    /// Open the configuration editor (same as `config`)
    #[arg(short = 'i', long = "setup", hide = true)]
    pub setup: bool,

    /// Apply a palette and exit (same as `preset apply`)
    #[arg(long = "preset", hide = true, value_name = "NAME")]
    pub preset: Option<String>,

    /// List palettes (same as `preset list`)
    #[arg(long = "list-presets", hide = true)]
    pub list_presets: bool,

    /// Pull, rebuild, and install (same as `update`)
    #[arg(long = "update", hide = true)]
    pub update: bool,

    /// Reset the configuration and open the editor (same as `config reset`)
    #[arg(long = "reset", hide = true)]
    pub reset: bool,

    /// Print only the configured logo (same as `logos show`)
    #[arg(long = "just-ascii", hide = true)]
    pub just_ascii: bool,
}

fn parse_scene(text: &str) -> Result<Scene, String> {
    text.parse()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(arguments: &[&str]) -> Args {
        Args::try_parse_from(arguments).expect("arguments are valid")
    }

    #[test]
    fn definitions_are_consistent() {
        Args::command().debug_assert();
    }

    #[test]
    fn scene_is_accepted_before_and_after_the_subcommand() {
        let before = parse(&["halofetch", "--scene", "side", "fetch"]);
        let after = parse(&["halofetch", "fetch", "--scene", "side"]);
        assert_eq!(before.scene, Some(Scene::Side));
        assert_eq!(after.scene, Some(Scene::Side));
    }

    #[test]
    fn scene_aliases_are_parsed_and_unknown_scenes_rejected() {
        assert_eq!(
            parse(&["halofetch", "--scene", "cockpit"]).scene,
            Some(Scene::Dashboard)
        );
        assert!(Args::try_parse_from(["halofetch", "--scene", "nope"]).is_err());
    }

    #[test]
    fn monitor_interval_is_optional_and_ranged() {
        let given = parse(&["halofetch", "monitor", "-i", "500"]);
        assert!(matches!(
            given.command,
            Some(Command::Monitor {
                interval: Some(500)
            })
        ));
        let default = parse(&["halofetch", "monitor"]);
        assert!(matches!(
            default.command,
            Some(Command::Monitor { interval: None })
        ));
        assert!(Args::try_parse_from(["halofetch", "monitor", "-i", "5"]).is_err());
    }

    #[test]
    fn fetch_watch_and_interval_are_parsed() {
        assert!(matches!(
            parse(&["halofetch", "fetch"]).command,
            Some(Command::Fetch {
                watch: false,
                interval: None
            })
        ));
        assert!(matches!(
            parse(&["halofetch", "fetch", "--watch"]).command,
            Some(Command::Fetch {
                watch: true,
                interval: None
            })
        ));
        assert!(matches!(
            parse(&["halofetch", "fetch", "-w", "-i", "500"]).command,
            Some(Command::Fetch {
                watch: true,
                interval: Some(500)
            })
        ));
        assert!(Args::try_parse_from(["halofetch", "fetch", "--interval", "500"]).is_err());
    }

    #[test]
    fn benchmark_runs_default_to_five() {
        let short = parse(&["halofetch", "benchmark", "-n", "3"]);
        assert!(matches!(
            short.command,
            Some(Command::Benchmark { iterations: 3 })
        ));
        let default = parse(&["halofetch", "benchmark"]);
        assert!(matches!(
            default.command,
            Some(Command::Benchmark { iterations: 5 })
        ));
    }

    #[test]
    fn config_without_action_is_allowed() {
        let args = parse(&["halofetch", "config"]);
        assert!(matches!(
            args.command,
            Some(Command::Config { action: None })
        ));
    }

    #[test]
    fn legacy_flags_still_parse() {
        let args = parse(&["halofetch", "--preset", "agender", "--just-ascii"]);
        assert_eq!(args.preset.as_deref(), Some("agender"));
        assert!(args.just_ascii);
        assert!(parse(&["halofetch", "-i"]).setup);
    }
}
