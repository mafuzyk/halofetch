// Configuration management.
//
// The config file lives at ~/.config/atlasfetch/config.json. It's loaded on
// every `atlasfetch` invocation but the file is tiny (~1 KB) so the overhead
// is negligible. The format uses a version field for future migrations.
//
// Old Python-generated configs are detected by the absence of a "version"
// field and migrated automatically on first load.

use color_eyre::{eyre::bail, Result};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

static CONFIG_OVERRIDE: OnceLock<PathBuf> = OnceLock::new();

use crate::component::Scene;
use crate::theme::Color;

// ── Configuration structs ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub logo: LogoConfig,
    #[serde(default)]
    pub title: TitleConfig,
    #[serde(default)]
    pub separator: SeparatorConfig,
    #[serde(default)]
    pub panel: PanelConfig,
    #[serde(default)]
    pub display: DisplayConfig,
    #[serde(default)]
    pub palette: PaletteConfig,
    #[serde(default)]
    pub live: LiveConfig,
    #[serde(default)]
    pub scene: Scene,
    #[serde(default)]
    pub custom_palettes: std::collections::HashMap<String, Vec<Color>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_live_interval")]
    pub interval_ms: u64,
}

impl Default for LiveConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_ms: default_live_interval(),
        }
    }
}

fn default_live_interval() -> u64 {
    1000
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogoConfig {
    #[serde(default = "default_logo_key")]
    pub key: String,
    #[serde(default = "default_logo_path")]
    pub path: String,
    #[serde(default = "default_logo_colors")]
    pub colors: Vec<Color>,
    #[serde(default = "default_color_dir")]
    pub color_dir: String,
}

fn default_color_dir() -> String {
    "horizontal".into()
}

impl Default for LogoConfig {
    fn default() -> Self {
        LogoConfig {
            key: default_logo_key(),
            path: default_logo_path(),
            colors: default_logo_colors(),
            color_dir: default_color_dir(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TitleConfig {
    #[serde(default = "default_title_format")]
    pub format: String,
    #[serde(default = "default_title_color")]
    pub color: String,
}

impl Default for TitleConfig {
    fn default() -> Self {
        TitleConfig {
            format: default_title_format(),
            color: default_title_color(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeparatorConfig {
    #[serde(default = "default_sep_char")]
    pub char: String,
    #[serde(default = "default_sep_color")]
    pub color: String,
    #[serde(default = "default_sep_length")]
    pub length: usize,
}

impl Default for SeparatorConfig {
    fn default() -> Self {
        SeparatorConfig {
            char: default_sep_char(),
            color: default_sep_color(),
            length: default_sep_length(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelConfig {
    #[serde(default = "default_panel_sep_color")]
    pub sep_color: String,
    #[serde(default = "default_panel_val_color")]
    pub val_color: String,
    #[serde(default = "default_left_pad")]
    pub left_pad: usize,
    #[serde(default = "default_right_pad")]
    pub right_pad: usize,
    #[serde(default = "default_gap")]
    pub gap: usize,
    #[serde(default = "default_max_shift")]
    pub max_shift: usize,
    #[serde(default = "default_max_val_width")]
    pub max_val_width: usize,
}

impl Default for PanelConfig {
    fn default() -> Self {
        PanelConfig {
            sep_color: default_panel_sep_color(),
            val_color: default_panel_val_color(),
            left_pad: default_left_pad(),
            right_pad: default_right_pad(),
            gap: default_gap(),
            max_shift: default_max_shift(),
            max_val_width: default_max_val_width(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayConfig {
    #[serde(default = "default_left_fields")]
    pub left: Vec<FieldDef>,
    #[serde(default = "default_right_fields")]
    pub right: Vec<FieldDef>,
}

impl DisplayConfig {
    fn dedup(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.left.retain(|f| seen.insert(f.field.clone()));
        self.right.retain(|f| seen.insert(f.field.clone()));
    }
}

impl Default for DisplayConfig {
    fn default() -> Self {
        DisplayConfig {
            left: default_left_fields(),
            right: default_right_fields(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaletteConfig {
    #[serde(default = "default_palette_load")]
    pub load: String,
    #[serde(default = "default_palette_processes")]
    pub processes: String,
}

impl Default for PaletteConfig {
    fn default() -> Self {
        PaletteConfig {
            load: default_palette_load(),
            processes: default_palette_processes(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDef {
    pub field: String,
    pub icon: String,
    pub label: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

// ── Defaults ─────────────────────────────────────────────────────────────

fn default_logo_key() -> String {
    "singularityos".into()
}
fn default_logo_path() -> String {
    "~/.config/atlasfetch/logo.txt".into()
}
fn default_logo_colors() -> Vec<Color> {
    vec![
        Color::from_hex("#C084FC"),
        Color::from_hex("#A78BFA"),
        Color::from_hex("#818CF8"),
        Color::from_hex("#6366F1"),
        Color::from_hex("#4F46E5"),
    ]
}
fn default_title_format() -> String {
    "{user}@{host}".into()
}
fn default_title_color() -> String {
    "#FF9A98".into()
}
fn default_sep_char() -> String {
    "\u{2500}".into()
}
fn default_sep_color() -> String {
    "#9D85FF".into()
}
fn default_sep_length() -> usize {
    48
}
fn default_panel_sep_color() -> String {
    "#9D85FF".into()
}
fn default_panel_val_color() -> String {
    "#f5dce3".into()
}
fn default_left_pad() -> usize {
    3
}
fn default_right_pad() -> usize {
    3
}
fn default_gap() -> usize {
    2
}
fn default_max_shift() -> usize {
    2
}
fn default_max_val_width() -> usize {
    999
}
fn default_enabled() -> bool {
    true
}
fn default_palette_load() -> String {
    "blue".into()
}
fn default_palette_processes() -> String {
    "green".into()
}

fn default_left_fields() -> Vec<FieldDef> {
    vec![
        FieldDef {
            field: "os".into(),
            icon: "\u{f17c}".into(),
            label: "OS".into(),
            enabled: true,
        },
        FieldDef {
            field: "user".into(),
            icon: "\u{f007}".into(),
            label: "Usr".into(),
            enabled: true,
        },
        FieldDef {
            field: "kernel".into(),
            icon: "\u{e271}".into(),
            label: "Krn".into(),
            enabled: true,
        },
        FieldDef {
            field: "packages".into(),
            icon: "\u{f1b3}".into(),
            label: "Pkg".into(),
            enabled: true,
        },
        FieldDef {
            field: "shell".into(),
            icon: "\u{f489}".into(),
            label: "Sh".into(),
            enabled: true,
        },
        FieldDef {
            field: "de".into(),
            icon: "\u{f108}".into(),
            label: "DE".into(),
            enabled: false,
        },
        FieldDef {
            field: "wm".into(),
            icon: "\u{f108}".into(),
            label: "WM".into(),
            enabled: true,
        },
    ]
}

fn default_right_fields() -> Vec<FieldDef> {
    vec![
        FieldDef {
            field: "uptime".into(),
            icon: "\u{f017}".into(),
            label: "Up".into(),
            enabled: true,
        },
        FieldDef {
            field: "terminal".into(),
            icon: "\u{f120}".into(),
            label: "Term".into(),
            enabled: true,
        },
        FieldDef {
            field: "font".into(),
            icon: "\u{f031}".into(),
            label: "Font".into(),
            enabled: false,
        },
        FieldDef {
            field: "cpu".into(),
            icon: "\u{f2db}".into(),
            label: "CPU".into(),
            enabled: true,
        },
        FieldDef {
            field: "gpu".into(),
            icon: "\u{f26c}".into(),
            label: "GPU".into(),
            enabled: true,
        },
        FieldDef {
            field: "memory".into(),
            icon: "\u{f1c0}".into(),
            label: "Mem".into(),
            enabled: true,
        },
        FieldDef {
            field: "disk".into(),
            icon: "\u{f0a0}".into(),
            label: "Dsk".into(),
            enabled: true,
        },
        FieldDef {
            field: "local_ip".into(),
            icon: "\u{f0c1}".into(),
            label: "IP".into(),
            enabled: false,
        },
    ]
}

// ── Paths ────────────────────────────────────────────────────────────────

pub fn set_config_path(path: PathBuf) {
    CONFIG_OVERRIDE.set(path).ok();
}

pub fn config_dir() -> Result<PathBuf> {
    if let Some(p) = CONFIG_OVERRIDE.get() {
        return config_parent(p);
    }
    let base = BaseDirs::new()
        .ok_or_else(|| color_eyre::eyre::eyre!("Cannot determine home directory"))?;
    Ok(base.home_dir().join(".config").join("atlasfetch"))
}

pub fn config_path() -> Result<PathBuf> {
    if let Some(p) = CONFIG_OVERRIDE.get() {
        return Ok(p.clone());
    }
    Ok(config_dir()?.join("config.json"))
}

fn config_parent(path: &std::path::Path) -> Result<PathBuf> {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => Ok(parent.to_path_buf()),
        Some(_) => Ok(std::env::current_dir()?),
        None => Err(color_eyre::eyre::eyre!(
            "Invalid config path: {}",
            path.display()
        )),
    }
}

pub fn logo_dir() -> Result<PathBuf> {
    // Look for logos/ next to the binary, then fall back to ~/.config/atlasfetch/logos
    let exe = std::env::current_exe()?;
    let script_dir = exe.parent().unwrap_or(std::path::Path::new("/"));
    let local = script_dir.join("logos");
    if local.exists() {
        return Ok(local);
    }
    Ok(config_dir()?.join("logos"))
}

// ── Load / Save / Migrate ───────────────────────────────────────────────

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            let cfg = Self::default();
            cfg.save()?;
            return Ok(cfg);
        }
        let raw = match fs::read_to_string(&path) {
            Ok(r) => r,
            Err(_) => {
                bail!("Cannot read config file at {:?}", path);
            }
        };
        let has_version = serde_json::from_str::<serde_json::Value>(&raw)
            .ok()
            .and_then(|value| value.get("version").cloned())
            .is_some();

        // Try current format first.
        if let Ok(mut cfg) = serde_json::from_str::<Config>(&raw) {
            cfg.display.dedup();
            if cfg.validate().is_ok() {
                return Ok(cfg);
            }
        }

        // Only versionless documents are eligible for the old Python migration.
        if !has_version {
            if let Ok(old) = serde_json::from_str::<OldConfig>(&raw) {
                let mut cfg = Self::migrate(old);
                cfg.display.dedup();
                cfg.validate()?;
                cfg.save()?;
                return Ok(cfg);
            }
        }
        // Preserve an invalid config before restoring defaults.
        let backup = backup_path(&path);
        fs::rename(&path, &backup)?;
        eprintln!(
            "Warning: invalid config moved to {}; restoring defaults",
            backup.display()
        );
        let cfg = Self::default();
        cfg.save()?;
        Ok(cfg)
    }

    pub fn save(&self) -> Result<()> {
        self.validate()?;
        let path = config_path()?;
        let dir = config_parent(&path)?;
        fs::create_dir_all(&dir)?;
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != 2 {
            bail!("unsupported config version {}; expected 2", self.version);
        }
        if self.logo.colors.is_empty() || self.logo.colors.len() > 64 {
            bail!("logo.colors must contain between 1 and 64 colors");
        }
        if !matches!(self.logo.color_dir.as_str(), "horizontal" | "vertical") {
            bail!("logo.color_dir must be 'horizontal' or 'vertical'");
        }
        for (name, color) in [
            ("title.color", self.title.color.as_str()),
            ("separator.color", self.separator.color.as_str()),
            ("panel.sep_color", self.panel.sep_color.as_str()),
            ("panel.val_color", self.panel.val_color.as_str()),
        ] {
            if Color::from_hex_opt(color).is_none() {
                bail!("{name} must be a #RRGGBB color");
            }
        }
        if self.separator.char.is_empty() || self.separator.char.chars().count() > 4 {
            bail!("separator.char must contain between 1 and 4 characters");
        }
        if self.separator.length > 500 {
            bail!("separator.length cannot exceed 500");
        }
        if self.panel.left_pad > 100
            || self.panel.right_pad > 100
            || self.panel.gap > 100
            || self.panel.max_shift > 100
            || self.panel.max_val_width > 10_000
        {
            bail!("panel spacing or width is outside the supported range");
        }
        if !(100..=60_000).contains(&self.live.interval_ms) {
            bail!("live.interval_ms must be between 100 and 60000");
        }
        for field in self.display.left.iter().chain(&self.display.right) {
            if field.field.trim().is_empty() {
                bail!("display fields cannot have an empty key");
            }
            if field.label.chars().count() > 40 || field.icon.chars().count() > 8 {
                bail!("display field labels or icons are too long");
            }
        }
        for (name, colors) in &self.custom_palettes {
            if name.trim().is_empty() || colors.is_empty() || colors.len() > 64 {
                bail!("custom palette names and color counts must be valid");
            }
        }
        Ok(())
    }

    pub fn default() -> Self {
        let colors = default_logo_colors();
        Config {
            version: 2,
            logo: LogoConfig {
                key: default_logo_key(),
                path: default_logo_path(),
                colors,
                color_dir: default_color_dir(),
            },
            title: TitleConfig {
                format: default_title_format(),
                color: default_title_color(),
            },
            separator: SeparatorConfig {
                char: default_sep_char(),
                color: default_sep_color(),
                length: default_sep_length(),
            },
            panel: PanelConfig {
                sep_color: default_panel_sep_color(),
                val_color: default_panel_val_color(),
                left_pad: default_left_pad(),
                right_pad: default_right_pad(),
                gap: default_gap(),
                max_shift: default_max_shift(),
                max_val_width: default_max_val_width(),
            },
            display: DisplayConfig {
                left: default_left_fields(),
                right: default_right_fields(),
            },
            palette: PaletteConfig {
                load: default_palette_load(),
                processes: default_palette_processes(),
            },
            live: LiveConfig::default(),
            scene: Scene::default(),
            custom_palettes: std::collections::HashMap::new(),
        }
    }

    /// Migrate from the old Python-format config (version 1, flat field arrays).
    fn migrate(old: OldConfig) -> Self {
        let mut cfg = Config::default();
        cfg.version = 2;

        // Logo
        if let Some(l) = old.logo {
            if let Some(key) = l.get("key").and_then(|v| v.as_str()) {
                cfg.logo.key = key.to_string();
            }
            if let Some(path) = l.get("path").and_then(|v| v.as_str()) {
                cfg.logo.path = path.to_string();
            }
            if let Some(colors) = l.get("colors").and_then(|v| v.as_array()) {
                let parsed: Vec<Color> = colors
                    .iter()
                    .filter_map(|c| c.as_str())
                    .filter_map(|s| {
                        if s.starts_with('#') {
                            Color::from_hex_opt(s)
                        } else {
                            None
                        }
                    })
                    .collect();
                if !parsed.is_empty() {
                    cfg.logo.colors = parsed;
                }
            }
        }

        // Title
        if let Some(t) = old.title {
            if let Some(fmt) = t.get("format").and_then(|v| v.as_str()) {
                cfg.title.format = fmt.to_string();
            }
            if let Some(color) = t.get("color").and_then(|v| v.as_str()) {
                cfg.title.color = color.to_string();
            }
        }

        // Separator
        if let Some(s) = old.separator {
            if let Some(ch) = s.get("char").and_then(|v| v.as_str()) {
                cfg.separator.char = ch.to_string();
            }
            if let Some(color) = s.get("color").and_then(|v| v.as_str()) {
                cfg.separator.color = color.to_string();
            }
            if let Some(len) = s.get("length").and_then(|v| v.as_u64()) {
                cfg.separator.length = len as usize;
            }
        }

        // Panel
        if let Some(p) = old.panel {
            if let Some(c) = p.get("sep_color").and_then(|v| v.as_str()) {
                cfg.panel.sep_color = c.to_string();
            }
            if let Some(c) = p.get("val_color").and_then(|v| v.as_str()) {
                cfg.panel.val_color = c.to_string();
            }
            if let Some(v) = p.get("left_pad").and_then(|v| v.as_u64()) {
                cfg.panel.left_pad = v as usize;
            }
            if let Some(v) = p.get("right_pad").and_then(|v| v.as_u64()) {
                cfg.panel.right_pad = v as usize;
            }
            if let Some(v) = p.get("gap").and_then(|v| v.as_u64()) {
                cfg.panel.gap = v as usize;
            }
            if let Some(v) = p.get("max_shift").and_then(|v| v.as_u64()) {
                cfg.panel.max_shift = v as usize;
            }
        }

        // Display fields (migrate old 3-element arrays to FieldDef)
        if let Some(d) = old.display {
            if let Some(left) = d.get("left").and_then(|v| v.as_array()) {
                let migrated: Vec<FieldDef> = left
                    .iter()
                    .filter_map(|item| {
                        let arr = item.as_array()?;
                        if arr.len() >= 3 {
                            Some(FieldDef {
                                field: arr[0].as_str()?.to_string(),
                                icon: arr[1].as_str()?.to_string(),
                                label: arr[2].as_str()?.to_string(),
                                enabled: true,
                            })
                        } else {
                            None
                        }
                    })
                    .collect();
                if !migrated.is_empty() {
                    cfg.display.left = migrated;
                }
            }
            if let Some(right) = d.get("right").and_then(|v| v.as_array()) {
                let migrated: Vec<FieldDef> = right
                    .iter()
                    .filter_map(|item| {
                        let arr = item.as_array()?;
                        if arr.len() >= 3 {
                            Some(FieldDef {
                                field: arr[0].as_str()?.to_string(),
                                icon: arr[1].as_str()?.to_string(),
                                label: arr[2].as_str()?.to_string(),
                                enabled: true,
                            })
                        } else {
                            None
                        }
                    })
                    .collect();
                if !migrated.is_empty() {
                    cfg.display.right = migrated;
                }
            }
        }

        cfg
    }
}

fn backup_path(path: &std::path::Path) -> PathBuf {
    let base = path.with_extension("json.invalid");
    if !base.exists() {
        return base;
    }
    for suffix in 1.. {
        let candidate = path.with_extension(format!("json.invalid.{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parent_uses_the_parent_of_a_nonexistent_file() {
        let path = PathBuf::from("/tmp/atlasfetch-test/custom.json");
        assert_eq!(
            config_parent(&path).unwrap(),
            PathBuf::from("/tmp/atlasfetch-test")
        );
    }

    #[test]
    fn config_parent_accepts_a_filename_in_the_current_directory() {
        assert_eq!(
            config_parent(std::path::Path::new("profile.json")).unwrap(),
            std::env::current_dir().unwrap()
        );
    }

    #[test]
    fn display_dedup_keeps_first_occurrence_across_both_panels() {
        let mut display = DisplayConfig {
            left: vec![field("os"), field("cpu"), field("os")],
            right: vec![field("memory"), field("cpu")],
        };

        display.dedup();

        assert_eq!(
            display
                .left
                .iter()
                .map(|f| f.field.as_str())
                .collect::<Vec<_>>(),
            ["os", "cpu"]
        );
        assert_eq!(
            display
                .right
                .iter()
                .map(|f| f.field.as_str())
                .collect::<Vec<_>>(),
            ["memory"]
        );
    }

    #[test]
    fn validation_rejects_invalid_layout_values() {
        let mut cfg = Config::default();
        cfg.logo.color_dir = "diagonal".into();
        assert!(cfg.validate().is_err());

        let mut cfg = Config::default();
        cfg.panel.gap = 101;
        assert!(cfg.validate().is_err());
    }

    fn field(name: &str) -> FieldDef {
        FieldDef {
            field: name.to_owned(),
            icon: String::new(),
            label: name.to_owned(),
            enabled: true,
        }
    }
}

// ── Old format (Python-generated) ────────────────────────────────────────
// These structs only exist for migration. The old format stored field arrays
// as flat 3-element JSON arrays like ["os", "\uf17c", "OS"].

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct OldConfig {
    #[serde(default)]
    logo: Option<serde_json::Value>,
    #[serde(default)]
    title: Option<serde_json::Value>,
    #[serde(default)]
    separator: Option<serde_json::Value>,
    #[serde(default)]
    panel: Option<serde_json::Value>,
    #[serde(default)]
    display: Option<serde_json::Value>,
    #[serde(default)]
    setup_done: Option<bool>,
    #[serde(default)]
    custom_palettes: Option<serde_json::Value>,
}
