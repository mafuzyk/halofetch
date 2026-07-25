// CLI argument parsing via clap derive.
// Kept minimal: the subcommand model splits normal execution from setup.

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Ansi,
    Json,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print one static fetch and exit, ignoring live startup preference
    Fetch,
    /// Open, inspect, or reset configuration
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// List or apply color presets
    Preset {
        #[command(subcommand)]
        action: PresetAction,
    },
    /// Work with embedded ASCII logos
    Logos {
        #[command(subcommand)]
        action: LogosAction,
    },
    /// Pull, rebuild, and install from the source checkout
    Update,
    /// Measure system information collection time
    Benchmark {
        /// Number of measured collections
        #[arg(short, long, default_value_t = 5, value_parser = clap::value_parser!(u16).range(1..=100))]
        iterations: u16,
    },
    /// Continuously refresh live system metrics in place
    Monitor {
        /// Refresh interval in milliseconds
        #[arg(short, long, default_value_t = 1000, value_parser = clap::value_parser!(u64).range(100..=60_000))]
        interval: u64,
        /// Scene to render continuously (default: configured scene)
        #[arg(long)]
        scene: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Print the active config path
    Path,
    /// Reset the active config and open the editor
    Reset,
}

#[derive(Debug, Subcommand)]
pub enum PresetAction {
    /// List built-in presets
    List,
    /// Apply a preset
    Apply { name: String },
}

#[derive(Debug, Subcommand)]
pub enum LogosAction {
    /// List embedded logo keys
    List,
}

#[derive(Parser, Debug)]
#[command(
    name = "atlasfetch",
    about = "Centered ASCII art with powerline panels",
    version
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Launch interactive setup TUI
    #[arg(short = 'i', long = "setup")]
    pub setup: bool,

    /// Apply a preset palette and exit
    #[arg(long = "preset")]
    pub preset: Option<String>,

    /// List available presets with color swatches
    #[arg(long = "list-presets")]
    pub list_presets: bool,

    /// Pull latest source, rebuild, and install
    #[arg(long = "update")]
    pub update: bool,

    /// Delete config and run setup wizard from scratch
    #[arg(long = "reset")]
    pub reset: bool,

    /// Print only the ASCII art (centered, colored), no system info
    #[arg(long = "just-ascii")]
    pub just_ascii: bool,

    /// Scene: classic, dashboard, cockpit, classicfetch
    #[arg(long = "scene")]
    pub scene: Option<String>,

    /// Path to config file (default: ~/.config/atlasfetch/config.json)
    #[arg(short = 'c', long = "config", global = true)]
    pub config: Option<std::path::PathBuf>,

    /// Output format for normal system information
    #[arg(long, value_enum, default_value_t)]
    pub format: OutputFormat,
}
