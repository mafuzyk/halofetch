//! Configuration: schema, defaults, validation, migration and persistence.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::OnceLock;

use color_eyre::eyre::{bail, eyre};
use color_eyre::Result;
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::field::Field;
use crate::info::SysInfo;
use crate::logo::{self, LogoSet, LogoSource};
use crate::theme::{Color, Gradient};

pub const VERSION: u32 = 3;

const MAX_PALETTE_COLORS: usize = 16;
const MIN_INTERVAL_MS: u64 = 100;
const MAX_INTERVAL_MS: u64 = 60_000;
const DEFAULT_INTERVAL_MS: u64 = 1_000;
const MAX_SPACING: usize = 20;
const MAX_CASCADE: usize = 10;
const MIN_VALUE_WIDTH: usize = 8;
const MAX_VALUE_WIDTH: usize = 200;
const MAX_TITLE_FORMAT: usize = 64;
const MAX_SEPARATOR: usize = 4;
const MAX_LABEL: usize = 24;
const MAX_ICON: usize = 4;
const MAX_PALETTE_NAME: usize = 32;
/// Version 2 used 999 to mean "no limit on the value width".
const LEGACY_UNLIMITED_WIDTH: usize = 999;
/// Abbreviated labels written by versions 1 and 2.
const LEGACY_LABELS: [&str; 11] = [
    "Usr", "Krn", "Pkg", "Sh", "Term", "Mem", "Dsk", "Up", "Proc", "Res", "IP",
];

static CONFIG_OVERRIDE: OnceLock<PathBuf> = OnceLock::new();

// ── Schema ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scene {
    #[default]
    Classic,
    Side,
    Dashboard,
}

impl Scene {
    pub const ALL: [Scene; 3] = [Scene::Classic, Scene::Side, Scene::Dashboard];

    pub fn key(self) -> &'static str {
        match self {
            Scene::Classic => "classic",
            Scene::Side => "side",
            Scene::Dashboard => "dashboard",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Scene::Classic => "Classic",
            Scene::Side => "Side",
            Scene::Dashboard => "Dashboard",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Scene::Classic => "Logo centered between two information panels",
            Scene::Side => "Logo on the left, information on the right",
            Scene::Dashboard => "Framed blocks with resource gauges",
        }
    }
}

impl FromStr for Scene {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.trim().to_ascii_lowercase().as_str() {
            "classic" => Ok(Scene::Classic),
            "side" | "classicfetch" | "classic-fetch" | "classic_fetch" | "fastfetch" => {
                Ok(Scene::Side)
            }
            "dashboard" | "cockpit" => Ok(Scene::Dashboard),
            _ => {
                let names: Vec<&str> = Scene::ALL.iter().map(|scene| scene.key()).collect();
                Err(format!(
                    "unknown scene '{text}', expected one of: {}",
                    names.join(", ")
                ))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StartupMode {
    #[default]
    Fetch,
    Monitor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InfoStyle {
    #[default]
    Powerline,
    Plain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IconSet {
    #[default]
    Nerd,
    Unicode,
    None,
}

impl IconSet {
    pub const ALL: [IconSet; 3] = [IconSet::Nerd, IconSet::Unicode, IconSet::None];
}

/// `layout.icons` as written now, or as the boolean that older versions used.
#[derive(Deserialize)]
#[serde(untagged)]
enum IconsWire {
    Flag(bool),
    Set(IconSet),
}

fn deserialize_icons<'de, D>(deserializer: D) -> Result<IconSet, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(match IconsWire::deserialize(deserializer)? {
        IconsWire::Flag(true) => IconSet::Nerd,
        IconsWire::Flag(false) => IconSet::None,
        IconsWire::Set(set) => set,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub scene: Scene,
    pub startup: Startup,
    pub logo: LogoConfig,
    pub colors: Colors,
    pub title: Title,
    pub layout: Layout,
    pub fields: Fields,
    pub custom_palettes: BTreeMap<String, Vec<Color>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Startup {
    pub mode: StartupMode,
    pub interval_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LogoConfig {
    pub source: LogoSource,
    pub gradient: Gradient,
    pub auto_small: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Colors {
    pub palette: Vec<Color>,
    pub title: Color,
    pub separator: Color,
    pub value: Color,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Title {
    pub enabled: bool,
    pub format: String,
    pub separator: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub style: InfoStyle,
    pub gap: usize,
    pub padding: usize,
    pub cascade: usize,
    /// 0 means unlimited; otherwise values are cut at this many columns.
    pub max_value_width: usize,
    pub hide_empty: bool,
    #[serde(deserialize_with = "deserialize_icons")]
    pub icons: IconSet,
    pub color_blocks: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fields {
    pub left: Vec<FieldEntry>,
    pub right: Vec<FieldEntry>,
}

/// One row of the information panels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "FieldEntryWire")]
pub struct FieldEntry {
    pub field: Field,
    pub label: String,
    pub icon: String,
    pub enabled: bool,
    /// Draw a gauge instead of plain text. Only meaningful for fields with a gauge.
    pub bar: bool,
}

/// Hand-edited entries may omit the label and icon; they then take the field's own.
#[derive(Deserialize)]
struct FieldEntryWire {
    field: String,
    label: Option<String>,
    icon: Option<String>,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
    #[serde(default)]
    bar: bool,
}

fn enabled_by_default() -> bool {
    true
}

impl TryFrom<FieldEntryWire> for FieldEntry {
    type Error = String;

    fn try_from(wire: FieldEntryWire) -> Result<Self, Self::Error> {
        let field = Field::from_key(wire.field.trim())
            .ok_or_else(|| format!("unknown field '{}'", wire.field))?;
        Ok(FieldEntry {
            field,
            label: wire.label.unwrap_or_else(|| field.label().to_string()),
            icon: wire.icon.unwrap_or_else(|| field.icon().to_string()),
            enabled: wire.enabled,
            bar: wire.bar,
        })
    }
}

impl FieldEntry {
    pub fn new(field: Field) -> Self {
        FieldEntry {
            field,
            label: field.label().to_string(),
            icon: field.icon().to_string(),
            enabled: true,
            bar: false,
        }
    }
}

// ── Defaults ─────────────────────────────────────────────────────────────

fn default_palette() -> Vec<Color> {
    // Same colors as the built-in "amethyst" theme; a test keeps them in sync.
    vec![
        Color::new(0xC0, 0x84, 0xFC),
        Color::new(0xA7, 0x8B, 0xFA),
        Color::new(0x81, 0x8C, 0xF8),
        Color::new(0x63, 0x66, 0xF1),
        Color::new(0x4F, 0x46, 0xE5),
    ]
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: VERSION,
            scene: Scene::default(),
            startup: Startup::default(),
            logo: LogoConfig::default(),
            colors: Colors::default(),
            title: Title::default(),
            layout: Layout::default(),
            fields: Fields::default(),
            custom_palettes: BTreeMap::new(),
        }
    }
}

impl Default for Startup {
    fn default() -> Self {
        Startup {
            mode: StartupMode::default(),
            interval_ms: DEFAULT_INTERVAL_MS,
        }
    }
}

impl Default for LogoConfig {
    fn default() -> Self {
        LogoConfig {
            source: LogoSource::default(),
            gradient: Gradient::default(),
            auto_small: true,
        }
    }
}

impl Default for Colors {
    fn default() -> Self {
        Colors {
            palette: default_palette(),
            title: Color::new(0xFF, 0x9A, 0x98),
            separator: Color::new(0x9D, 0x85, 0xFF),
            value: Color::new(0xF5, 0xDC, 0xE3),
        }
    }
}

impl Default for Title {
    fn default() -> Self {
        Title {
            enabled: true,
            format: "{user}@{host}".to_string(),
            separator: "\u{2500}".to_string(),
        }
    }
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            style: InfoStyle::default(),
            gap: 3,
            padding: 2,
            cascade: 0,
            max_value_width: 0,
            hide_empty: true,
            icons: IconSet::Nerd,
            color_blocks: true,
        }
    }
}

impl Default for Fields {
    fn default() -> Self {
        let mut local_ip = FieldEntry::new(Field::LocalIp);
        local_ip.enabled = false;
        Fields {
            left: [
                Field::Os,
                Field::Kernel,
                Field::Packages,
                Field::Shell,
                Field::Terminal,
                Field::De,
                Field::Wm,
            ]
            .into_iter()
            .map(FieldEntry::new)
            .collect(),
            right: [
                Field::Uptime,
                Field::Cpu,
                Field::Gpu,
                Field::Memory,
                Field::Disk,
                Field::Battery,
            ]
            .into_iter()
            .map(FieldEntry::new)
            .chain(std::iter::once(local_ip))
            .collect(),
        }
    }
}

// ── Paths ────────────────────────────────────────────────────────────────

const CONFIG_DIR: &str = "halofetch";
const LEGACY_DIR: &str = "atlasfetch";
pub fn set_config_path(path: PathBuf) {
    // Only the first call has an effect; the override is fixed for the process.
    let _ = CONFIG_OVERRIDE.set(path);
}

pub fn config_dir() -> Result<PathBuf> {
    if let Some(path) = CONFIG_OVERRIDE.get() {
        return config_parent(path);
    }
    default_config_dir()
}

#[cfg(not(windows))]
fn default_config_dir() -> Result<PathBuf> {
    // The XDG spec says relative values must be ignored.
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
    {
        let dir = xdg.join(CONFIG_DIR);
        migrate_once(&dir);
        return Ok(dir);
    }
    let base = BaseDirs::new().ok_or_else(|| eyre!("cannot determine the home directory"))?;
    let dir = base.home_dir().join(".config").join(CONFIG_DIR);
    migrate_once(&dir);
    Ok(dir)
}

#[cfg(windows)]
fn default_config_dir() -> Result<PathBuf> {
    let base = BaseDirs::new().ok_or_else(|| eyre!("cannot determine the home directory"))?;
    let dir = base.config_dir().join(CONFIG_DIR);
    migrate_once(&dir);
    Ok(dir)
}

// Tests must never move the real configuration directory.
#[cfg(not(test))]
fn migrate_once(new_dir: &Path) {
    static MIGRATION: std::sync::Once = std::sync::Once::new();
    MIGRATION.call_once(|| migrate_legacy_dir(new_dir));
}

#[cfg(test)]
fn migrate_once(_new_dir: &Path) {}

fn migrate_legacy_dir(new_dir: &Path) {
    if new_dir.exists() {
        return;
    }
    let Some(parent) = new_dir.parent() else {
        return;
    };
    let legacy = parent.join(LEGACY_DIR);
    if !legacy.is_dir() {
        return;
    }
    match move_dir(&legacy, new_dir) {
        Ok(()) => eprintln!(
            "halofetch: moved the configuration from {} to {}",
            legacy.display(),
            new_dir.display()
        ),
        Err(err) => eprintln!(
            "halofetch: warning: could not migrate the configuration from {}: {err}",
            legacy.display()
        ),
    }
}

fn move_dir(legacy: &Path, new_dir: &Path) -> std::io::Result<()> {
    if fs::rename(legacy, new_dir).is_ok() {
        return Ok(());
    }
    if let Err(err) = copy_dir(legacy, new_dir) {
        let _ = fs::remove_dir_all(new_dir);
        return Err(err);
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

pub fn config_path() -> Result<PathBuf> {
    match CONFIG_OVERRIDE.get() {
        Some(path) => Ok(path.clone()),
        None => Ok(config_dir()?.join("config.json")),
    }
}

/// User logo directory, when it exists.
pub fn user_logo_dir() -> Option<PathBuf> {
    config_dir()
        .ok()
        .map(|dir| dir.join("logos"))
        .filter(|dir| dir.is_dir())
}

fn config_parent(path: &Path) -> Result<PathBuf> {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => Ok(parent.to_path_buf()),
        Some(_) => Ok(std::env::current_dir()?),
        None => Err(eyre!("invalid config path: {}", path.display())),
    }
}

/// `path` with `.suffix` appended to its full file name.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(suffix);
    PathBuf::from(name)
}

/// `path.suffix`, or `path.suffix.1`, `path.suffix.2`... when earlier ones exist.
fn numbered_sibling(path: &Path, suffix: &str) -> PathBuf {
    let base = with_suffix(path, suffix);
    if !base.exists() {
        return base;
    }
    (1..)
        .map(|n| with_suffix(path, &format!("{suffix}.{n}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or(base)
}

// ── Load / save ──────────────────────────────────────────────────────────

impl Config {
    pub fn exists() -> bool {
        config_path().is_ok_and(|path| path.exists())
    }

    pub fn load() -> Result<Config> {
        Config::load_from(&config_path()?)
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&config_path()?)
    }

    /// Reads the configuration at `path`. A missing file yields defaults and is
    /// not created. A version 1 or 2 file is migrated: the original is kept as a
    /// backup and the migrated file is written. A file that cannot be used is
    /// moved aside and defaults are returned.
    pub(crate) fn load_from(path: &Path) -> Result<Config> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Config::default());
            }
            Err(err) => return Err(eyre!("cannot read {}: {err}", path.display())),
        };
        let value: Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(err) => {
                return Ok(quarantine(
                    path,
                    &format!("the file is not valid JSON ({err})"),
                ))
            }
        };
        match value.get("version") {
            None => load_legacy(path, value),
            Some(version) => match version.as_u64() {
                Some(2) => load_legacy(path, value),
                Some(3) => load_current(path, value),
                _ => Ok(quarantine(
                    path,
                    &format!("unsupported configuration version {version}"),
                )),
            },
        }
    }

    /// Writes the configuration atomically: a temporary file, then a rename.
    pub(crate) fn save_to(&self, path: &Path) -> Result<()> {
        self.validate()?;
        fs::create_dir_all(config_parent(path)?)?;
        let json = serde_json::to_string_pretty(self)?;
        let tmp = with_suffix(path, "tmp");
        fs::write(&tmp, format!("{json}\n"))?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != VERSION {
            bail!(
                "unsupported configuration version {}; expected {VERSION}",
                self.version
            );
        }
        if !(1..=MAX_PALETTE_COLORS).contains(&self.colors.palette.len()) {
            bail!("colors.palette must contain 1 to {MAX_PALETTE_COLORS} colors");
        }
        if !(MIN_INTERVAL_MS..=MAX_INTERVAL_MS).contains(&self.startup.interval_ms) {
            bail!("startup.interval_ms must be between {MIN_INTERVAL_MS} and {MAX_INTERVAL_MS}");
        }
        if self.layout.gap > MAX_SPACING || self.layout.padding > MAX_SPACING {
            bail!("layout.gap and layout.padding must be at most {MAX_SPACING}");
        }
        if self.layout.cascade > MAX_CASCADE {
            bail!("layout.cascade must be at most {MAX_CASCADE}");
        }
        if self.layout.max_value_width != 0
            && !(MIN_VALUE_WIDTH..=MAX_VALUE_WIDTH).contains(&self.layout.max_value_width)
        {
            bail!("layout.max_value_width must be 0 or between {MIN_VALUE_WIDTH} and {MAX_VALUE_WIDTH}");
        }
        if self.title.format.chars().count() > MAX_TITLE_FORMAT {
            bail!("title.format must be at most {MAX_TITLE_FORMAT} characters");
        }
        if self.title.separator.chars().count() > MAX_SEPARATOR {
            bail!("title.separator must be at most {MAX_SEPARATOR} characters");
        }
        let mut seen = HashSet::new();
        for entry in self.fields.left.iter().chain(&self.fields.right) {
            if entry.label.chars().count() > MAX_LABEL {
                bail!(
                    "the label of {} must be at most {MAX_LABEL} characters",
                    entry.field.key()
                );
            }
            if entry.icon.chars().count() > MAX_ICON {
                bail!(
                    "the icon of {} must be at most {MAX_ICON} characters",
                    entry.field.key()
                );
            }
            if !seen.insert(entry.field) {
                bail!("field {} appears more than once", entry.field.key());
            }
        }
        for (name, palette) in &self.custom_palettes {
            if name.trim().is_empty() || name.chars().count() > MAX_PALETTE_NAME {
                bail!("custom palette names must be 1 to {MAX_PALETTE_NAME} characters");
            }
            if !(1..=MAX_PALETTE_COLORS).contains(&palette.len()) {
                bail!("custom palette '{name}' must contain 1 to {MAX_PALETTE_COLORS} colors");
            }
        }
        Ok(())
    }

    /// Clamps values into their ranges, drops duplicate fields (the first
    /// occurrence, left panel before right, wins) and restores an empty palette.
    pub fn normalize(&mut self) {
        let palette = &mut self.colors.palette;
        if palette.is_empty() {
            *palette = default_palette();
        }
        palette.truncate(MAX_PALETTE_COLORS);

        self.startup.interval_ms = self
            .startup
            .interval_ms
            .clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS);
        self.layout.gap = self.layout.gap.min(MAX_SPACING);
        self.layout.padding = self.layout.padding.min(MAX_SPACING);
        self.layout.cascade = self.layout.cascade.min(MAX_CASCADE);
        if self.layout.max_value_width != 0 {
            self.layout.max_value_width = self
                .layout
                .max_value_width
                .clamp(MIN_VALUE_WIDTH, MAX_VALUE_WIDTH);
        }
        self.title.format = truncate_chars(&self.title.format, MAX_TITLE_FORMAT);
        self.title.separator = truncate_chars(&self.title.separator, MAX_SEPARATOR);

        for entry in self
            .fields
            .left
            .iter_mut()
            .chain(self.fields.right.iter_mut())
        {
            entry.label = truncate_chars(&entry.label, MAX_LABEL);
            entry.icon = truncate_chars(&entry.icon, MAX_ICON);
        }
        let mut seen = HashSet::new();
        self.fields.left.retain(|entry| seen.insert(entry.field));
        self.fields.right.retain(|entry| seen.insert(entry.field));

        let mut palettes = BTreeMap::new();
        for (name, mut colors) in std::mem::take(&mut self.custom_palettes) {
            let name = truncate_chars(name.trim(), MAX_PALETTE_NAME);
            if name.is_empty() || colors.is_empty() {
                continue;
            }
            colors.truncate(MAX_PALETTE_COLORS);
            palettes.entry(name).or_insert(colors);
        }
        self.custom_palettes = palettes;
    }

    pub fn logo_set(&self, info: &SysInfo) -> LogoSet {
        logo::resolve(
            &self.logo.source,
            &info.os_ids,
            user_logo_dir().as_deref(),
            self.logo.auto_small,
        )
    }
}

fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn load_current(path: &Path, value: Value) -> Result<Config> {
    let mut config: Config = match serde_json::from_value(value) {
        Ok(config) => config,
        Err(err) => return Ok(quarantine(path, &format!("invalid configuration ({err})"))),
    };
    config.normalize();
    if let Err(err) = config.validate() {
        return Ok(quarantine(path, &format!("invalid configuration ({err})")));
    }
    Ok(config)
}

fn load_legacy(path: &Path, value: Value) -> Result<Config> {
    let config = match migrate(value) {
        Ok(config) => config,
        Err(err) => return Ok(quarantine(path, &err.to_string())),
    };
    let backup = numbered_sibling(path, "bak");
    fs::copy(path, &backup).map_err(|err| eyre!("cannot back up {}: {err}", path.display()))?;
    config.save_to(path)?;
    eprintln!(
        "halofetch: migrated configuration to v{VERSION} (backup: {})",
        backup.display()
    );
    Ok(config)
}

/// Moves an unusable file aside and returns defaults. Nothing is written back.
fn quarantine(path: &Path, reason: &str) -> Config {
    let target = numbered_sibling(path, "invalid");
    match fs::rename(path, &target) {
        Ok(()) => eprintln!(
            "halofetch: warning: {reason}; moved the file to {} and using defaults",
            target.display()
        ),
        Err(err) => eprintln!(
            "halofetch: warning: {reason}; could not move the file aside ({err}); using defaults"
        ),
    }
    Config::default()
}

// ── Migration ────────────────────────────────────────────────────────────

/// Converts a version 1 or version 2 document into the current format. Missing
/// keys keep their defaults. The result is normalized.
pub fn migrate(value: Value) -> Result<Config> {
    if !value.is_object() {
        bail!("the configuration is not a JSON object");
    }
    let mut config = Config::default();

    if let Some(scene) = text(&value, &["scene"]) {
        config.scene = scene.parse().unwrap_or_default();
    }
    if lookup(&value, &["live", "enabled"]).and_then(Value::as_bool) == Some(true) {
        config.startup.mode = StartupMode::Monitor;
    }
    if let Some(ms) = lookup(&value, &["live", "interval_ms"]).and_then(Value::as_u64) {
        config.startup.interval_ms = ms.clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS);
    }

    config.logo.source = legacy_logo_source(&value);
    if text(&value, &["logo", "color_dir"]) == Some("vertical") {
        config.logo.gradient = Gradient::Vertical;
    }
    if let Some(colors) = lookup(&value, &["logo", "colors"]).and_then(Value::as_array) {
        config.colors.palette = colors.iter().filter_map(color_of).collect();
    }

    if let Some(format) = text(&value, &["title", "format"]) {
        config.title.format = format.to_string();
    }
    if let Some(color) = color_at(&value, &["title", "color"]) {
        config.colors.title = color;
    }
    if let Some(ch) = text(&value, &["separator", "char"]) {
        config.title.separator = ch.to_string();
    }
    if let Some(color) = color_at(&value, &["separator", "color"]) {
        config.colors.separator = color;
    }

    if let Some(color) = color_at(&value, &["panel", "val_color"]) {
        config.colors.value = color;
    }
    if let Some(gap) = count_at(&value, &["panel", "gap"]) {
        config.layout.gap = gap;
    }
    if let Some(padding) = count_at(&value, &["panel", "left_pad"]) {
        config.layout.padding = padding;
    }
    if let Some(cascade) = count_at(&value, &["panel", "max_shift"]) {
        config.layout.cascade = cascade;
    }
    if let Some(width) = count_at(&value, &["panel", "max_val_width"]) {
        config.layout.max_value_width = if width >= LEGACY_UNLIMITED_WIDTH {
            0
        } else {
            width.clamp(MIN_VALUE_WIDTH, MAX_VALUE_WIDTH)
        };
    }

    if let Some(entries) = legacy_fields(lookup(&value, &["display", "left"])) {
        config.fields.left = entries;
    }
    if let Some(entries) = legacy_fields(lookup(&value, &["display", "right"])) {
        config.fields.right = entries;
    }

    if let Some(palettes) = lookup(&value, &["custom_palettes"]).and_then(Value::as_object) {
        for (name, colors) in palettes {
            if let Some(colors) = colors.as_array() {
                config
                    .custom_palettes
                    .insert(name.clone(), colors.iter().filter_map(color_of).collect());
            }
        }
    }

    config.normalize();
    Ok(config)
}

fn legacy_logo_source(value: &Value) -> LogoSource {
    if let Some(key) = text(value, &["logo", "key"])
        .map(str::trim)
        .filter(|key| !key.is_empty())
    {
        return LogoSource::Builtin {
            key: key.to_string(),
        };
    }
    let path = text(value, &["logo", "path"]).map_or("", str::trim);
    if path.eq_ignore_ascii_case("disabled") {
        LogoSource::None
    } else if !path.is_empty() && logo::expand_home(path).is_file() {
        LogoSource::File {
            path: path.to_string(),
        }
    } else {
        LogoSource::Auto
    }
}

/// Version 1 stores fields as `[key, icon, label]` arrays, version 2 as objects.
/// `None` when the value is not an array.
fn legacy_fields(value: Option<&Value>) -> Option<Vec<FieldEntry>> {
    Some(value?.as_array()?.iter().filter_map(legacy_field).collect())
}

fn legacy_field(item: &Value) -> Option<FieldEntry> {
    let (raw_key, icon, label, enabled) = match item {
        Value::Object(_) => (
            text(item, &["field"])?,
            text(item, &["icon"]),
            text(item, &["label"]),
            item.get("enabled").and_then(Value::as_bool).unwrap_or(true),
        ),
        Value::Array(parts) => (
            parts.first()?.as_str()?,
            parts.get(1).and_then(Value::as_str),
            parts.get(2).and_then(Value::as_str),
            true,
        ),
        _ => return None,
    };

    let raw_key = raw_key.trim();
    let (key, bar) = match raw_key.strip_suffix("_bar") {
        Some(key) => (key, true),
        None => (raw_key, false),
    };
    let field = Field::from_key(key)?;

    let label = match label {
        Some(label) if !is_legacy_label(label) => label.to_string(),
        _ => field.label().to_string(),
    };
    let icon = icon.map_or_else(|| field.icon().to_string(), str::to_string);

    Some(FieldEntry {
        field,
        label,
        icon,
        enabled,
        bar,
    })
}

fn is_legacy_label(label: &str) -> bool {
    LEGACY_LABELS.contains(&label) || label.ends_with(" [bar]")
}

/// Version 1 stores colors as `"#RRGGBB"`, version 2 as `{"r", "g", "b"}`.
fn color_of(value: &Value) -> Option<Color> {
    match value {
        Value::String(hex) => Color::from_hex(hex),
        Value::Object(_) => {
            let channel = |name: &str| {
                value
                    .get(name)
                    .and_then(Value::as_u64)
                    .and_then(|n| u8::try_from(n).ok())
            };
            Some(Color::new(channel("r")?, channel("g")?, channel("b")?))
        }
        _ => None,
    }
}

fn lookup<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |node, key| node.get(*key))
}

fn text<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    lookup(value, path)?.as_str()
}

fn color_at(value: &Value, path: &[&str]) -> Option<Color> {
    text(value, path).and_then(Color::from_hex)
}

fn count_at(value: &Value, path: &[&str]) -> Option<usize> {
    lookup(value, path)
        .and_then(Value::as_u64)
        .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme;
    use serde_json::json;

    const LEGACY_V2: &str = r##"{
  "version": 2,
  "logo": {
    "key": "arch",
    "path": "~/.config/halofetch/logo.txt",
    "colors": [{"r": 255, "g": 0, "b": 0}, {"r": 0, "g": 0, "b": 255}],
    "color_dir": "vertical"
  },
  "title": {"format": "{user}@{host}", "color": "#ff9a98"},
  "separator": {"char": "=", "color": "#9d85ff", "length": 48},
  "panel": {
    "sep_color": "#9D85FF", "val_color": "#f5dce3", "left_pad": 4, "right_pad": 3,
    "gap": 5, "max_shift": 1, "max_val_width": 40
  },
  "display": {
    "left": [
      {"field": "os", "icon": "\uf17c", "label": "OS", "enabled": true},
      {"field": "user", "icon": "\uf007", "label": "Usr", "enabled": true},
      {"field": "memory_bar", "icon": "\uf1c0", "label": "Mem [bar]", "enabled": true},
      {"field": "unknown_key", "icon": "", "label": "Other", "enabled": true}
    ],
    "right": [
      {"field": "local_ip", "icon": "", "label": "Custom IP", "enabled": false},
      {"field": "cpu", "icon": "\uf2db", "label": "CPU [bar]", "enabled": true}
    ]
  },
  "live": {"enabled": true, "interval_ms": 250},
  "scene": "cockpit",
  "custom_palettes": {"ocean": [{"r": 0, "g": 119, "b": 190}, "#00FFFF"]}
}"##;

    const LEGACY_V1: &str = r##"{
  "logo": {"key": "", "path": "DISABLED", "colors": ["#C084FC", "#4F46E5"], "color_dir": "horizontal"},
  "title": {"format": "{user}", "color": "#FF9A98"},
  "separator": {"char": "-", "color": "#9D85FF", "length": 48},
  "panel": {"sep_color": "#9D85FF", "val_color": "#F5DCE3", "left_pad": 3, "right_pad": 3, "gap": 2, "max_shift": 2, "max_val_width": 999},
  "display": {
    "left": [["os", "\uf17c", "OS"], ["kernel", "\ue271", "Krn"]],
    "right": [["uptime", "\uf017", "Up"], ["disk_bar", "\uf0a0", "Dsk"]]
  }
}"##;

    /// Temporary directory, removed when the guard is dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("halofetch-config-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        fn file(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn color(hex: &str) -> Color {
        Color::from_hex(hex).unwrap()
    }

    fn broken(edit: impl FnOnce(&mut Config)) -> bool {
        let mut config = Config::default();
        edit(&mut config);
        config.validate().is_err()
    }

    #[test]
    fn defaults_are_valid_and_round_trip_through_json() {
        let config = Config::default();
        assert_eq!(config.version, VERSION);
        assert!(config.validate().is_ok());
        let json = serde_json::to_string(&config).unwrap();
        let parsed: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn default_palette_matches_the_amethyst_theme() {
        let theme = theme::find_theme("amethyst").unwrap();
        assert_eq!(Config::default().colors.palette, theme.colors);
    }

    #[test]
    fn default_fields_keep_local_ip_hidden() {
        let fields = Config::default().fields;
        let left: Vec<Field> = fields.left.iter().map(|entry| entry.field).collect();
        assert_eq!(
            left,
            [
                Field::Os,
                Field::Kernel,
                Field::Packages,
                Field::Shell,
                Field::Terminal,
                Field::De,
                Field::Wm
            ]
        );
        let right: Vec<Field> = fields.right.iter().map(|entry| entry.field).collect();
        assert_eq!(
            right,
            [
                Field::Uptime,
                Field::Cpu,
                Field::Gpu,
                Field::Memory,
                Field::Disk,
                Field::Battery,
                Field::LocalIp
            ]
        );
        let local_ip = fields.right.last().unwrap();
        assert!(!local_ip.enabled);
    }

    #[test]
    fn scene_keys_and_aliases_parse() {
        assert_eq!("Classic".parse::<Scene>(), Ok(Scene::Classic));
        assert_eq!("side".parse::<Scene>(), Ok(Scene::Side));
        for alias in [
            "classicfetch",
            "classic-fetch",
            "classic_fetch",
            "fastfetch",
        ] {
            assert_eq!(alias.parse::<Scene>(), Ok(Scene::Side));
        }
        assert_eq!("COCKPIT".parse::<Scene>(), Ok(Scene::Dashboard));
        let err = "nope".parse::<Scene>().unwrap_err();
        assert!(err.contains("classic, side, dashboard"));
    }

    #[test]
    fn scene_serializes_as_its_key() {
        assert_eq!(
            serde_json::to_value(Scene::Dashboard).unwrap(),
            json!("dashboard")
        );
    }

    #[test]
    fn field_entry_takes_missing_label_and_icon_from_the_field() {
        let entry: FieldEntry = serde_json::from_value(json!({"field": "cpu"})).unwrap();
        assert_eq!(entry.field, Field::Cpu);
        assert_eq!(entry.label, Field::Cpu.label());
        assert_eq!(entry.icon, Field::Cpu.icon());
        assert!(entry.enabled);
        assert!(!entry.bar);
    }

    #[test]
    fn field_entry_accepts_legacy_keys_and_rejects_unknown_ones() {
        let entry: FieldEntry = serde_json::from_value(json!({"field": "battery_level"})).unwrap();
        assert_eq!(entry.field, Field::Battery);
        assert!(serde_json::from_value::<FieldEntry>(json!({"field": "nope"})).is_err());
    }

    #[test]
    fn validation_rejects_out_of_range_values() {
        assert!(broken(|c| c.colors.palette.clear()));
        assert!(broken(|c| c.startup.interval_ms = 50));
        assert!(broken(|c| c.layout.gap = 21));
        assert!(broken(|c| c.layout.cascade = 11));
        assert!(broken(|c| c.layout.max_value_width = 5));
        assert!(broken(|c| c.title.separator = "toolong".to_string()));
        assert!(broken(|c| c.title.format = "x".repeat(65)));
        assert!(broken(|c| c.fields.left[0].label = "y".repeat(25)));
        assert!(broken(|c| c.version = 2));
        assert!(broken(|c| c.fields.right.push(FieldEntry::new(Field::Os))));
        assert!(broken(|c| {
            c.custom_palettes.insert(String::new(), vec![Color::WHITE]);
        }));
    }

    #[test]
    fn normalize_clamps_and_keeps_the_first_of_duplicate_fields() {
        let mut config = Config::default();
        config.colors.palette.clear();
        config.startup.interval_ms = 5;
        config.layout.gap = 99;
        config.layout.cascade = 99;
        config.layout.max_value_width = 3;
        config.fields.left = vec![FieldEntry::new(Field::Cpu)];
        config.fields.right = vec![FieldEntry::new(Field::Cpu), FieldEntry::new(Field::Os)];

        config.normalize();

        assert_eq!(config.colors.palette, default_palette());
        assert_eq!(config.startup.interval_ms, MIN_INTERVAL_MS);
        assert_eq!(config.layout.gap, MAX_SPACING);
        assert_eq!(config.layout.cascade, MAX_CASCADE);
        assert_eq!(config.layout.max_value_width, MIN_VALUE_WIDTH);
        let right: Vec<Field> = config
            .fields
            .right
            .iter()
            .map(|entry| entry.field)
            .collect();
        assert_eq!(right, [Field::Os]);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn normalize_truncates_long_text() {
        let mut config = Config::default();
        config.title.format = "x".repeat(100);
        config.fields.left[0].label = "y".repeat(40);
        config.fields.left[0].icon = "z".repeat(9);

        config.normalize();

        assert_eq!(config.title.format.chars().count(), MAX_TITLE_FORMAT);
        assert_eq!(config.fields.left[0].label.chars().count(), MAX_LABEL);
        assert_eq!(config.fields.left[0].icon.chars().count(), MAX_ICON);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn migrates_a_version_2_document() {
        let config = migrate(serde_json::from_str(LEGACY_V2).unwrap()).unwrap();

        assert_eq!(config.version, VERSION);
        assert_eq!(config.scene, Scene::Dashboard);
        assert_eq!(
            config.startup,
            Startup {
                mode: StartupMode::Monitor,
                interval_ms: 250
            }
        );
        assert_eq!(
            config.logo.source,
            LogoSource::Builtin {
                key: "arch".to_string()
            }
        );
        assert_eq!(config.logo.gradient, Gradient::Vertical);
        assert_eq!(
            config.colors.palette,
            vec![Color::new(255, 0, 0), Color::new(0, 0, 255)]
        );
        assert_eq!(config.colors.title, color("#FF9A98"));
        assert_eq!(config.colors.separator, color("#9D85FF"));
        assert_eq!(config.colors.value, color("#F5DCE3"));
        assert_eq!(config.title.format, "{user}@{host}");
        assert_eq!(config.title.separator, "=");
        assert_eq!(config.layout.gap, 5);
        assert_eq!(config.layout.padding, 4);
        assert_eq!(config.layout.cascade, 1);
        assert_eq!(config.layout.max_value_width, 40);
        assert_eq!(
            config.custom_palettes["ocean"],
            vec![color("#0077BE"), color("#00FFFF")]
        );

        let left: Vec<(Field, &str, bool)> = config
            .fields
            .left
            .iter()
            .map(|entry| (entry.field, entry.label.as_str(), entry.bar))
            .collect();
        assert_eq!(
            left,
            [
                (Field::Os, "OS", false),
                (Field::User, "User", false),
                (Field::Memory, "Memory", true),
            ]
        );
        assert_eq!(config.fields.left[0].icon, "\u{f17c}");

        assert_eq!(config.fields.right.len(), 2);
        assert_eq!(config.fields.right[0].label, "Custom IP");
        assert_eq!(config.fields.right[0].icon, "");
        assert!(!config.fields.right[0].enabled);
        assert_eq!(config.fields.right[1].label, "CPU");
    }

    #[test]
    fn migrates_a_version_1_document_with_array_fields() {
        let config = migrate(serde_json::from_str(LEGACY_V1).unwrap()).unwrap();

        assert_eq!(config.logo.source, LogoSource::None);
        assert_eq!(
            config.colors.palette,
            vec![color("#C084FC"), color("#4F46E5")]
        );
        assert_eq!(config.colors.value, color("#F5DCE3"));
        assert_eq!(config.title.format, "{user}");
        assert_eq!(config.title.separator, "-");
        assert_eq!(config.layout.gap, 2);
        assert_eq!(config.layout.cascade, 2);
        assert_eq!(config.layout.max_value_width, 0);
        assert_eq!(config.startup.mode, StartupMode::Fetch);

        let left: Vec<(Field, &str, bool)> = config
            .fields
            .left
            .iter()
            .map(|entry| (entry.field, entry.label.as_str(), entry.bar))
            .collect();
        assert_eq!(
            left,
            [(Field::Os, "OS", false), (Field::Kernel, "Kernel", false)]
        );
        let right: Vec<(Field, &str, bool, bool)> = config
            .fields
            .right
            .iter()
            .map(|entry| (entry.field, entry.label.as_str(), entry.bar, entry.enabled))
            .collect();
        assert_eq!(
            right,
            [
                (Field::Uptime, "Uptime", false, true),
                (Field::Disk, "Disk", true, true),
            ]
        );
    }

    #[test]
    fn document_without_known_keys_migrates_to_defaults() {
        assert_eq!(migrate(json!({})).unwrap(), Config::default());
        assert!(migrate(json!([1, 2])).is_err());
    }

    #[test]
    fn legacy_scene_names_map_to_current_scenes() {
        let scene_of = |name: &str| migrate(json!({"scene": name})).unwrap().scene;
        assert_eq!(scene_of("classic"), Scene::Classic);
        assert_eq!(scene_of("classicfetch"), Scene::Side);
        assert_eq!(scene_of("cockpit"), Scene::Dashboard);
        assert_eq!(scene_of("unknown"), Scene::Classic);
    }

    #[test]
    fn logo_path_becomes_a_file_source_only_when_the_file_exists() {
        let scratch = Scratch::new("logo-source");
        let logo_file = scratch.file("logo.txt");
        fs::write(&logo_file, "art").unwrap();
        let existing = logo_file.display().to_string();
        let missing = scratch.file("missing.txt").display().to_string();

        let config = migrate(json!({"logo": {"key": "", "path": existing.clone()}})).unwrap();
        assert_eq!(config.logo.source, LogoSource::File { path: existing });

        let config = migrate(json!({"logo": {"key": "", "path": missing}})).unwrap();
        assert_eq!(config.logo.source, LogoSource::Auto);
    }

    #[test]
    fn logo_key_wins_over_path_and_disabled_path_hides_the_logo() {
        let config = migrate(json!({"logo": {"key": "arch", "path": "disabled"}})).unwrap();
        assert_eq!(
            config.logo.source,
            LogoSource::Builtin {
                key: "arch".to_string()
            }
        );
        let config = migrate(json!({"logo": {"key": "", "path": "Disabled"}})).unwrap();
        assert_eq!(config.logo.source, LogoSource::None);
    }

    #[test]
    fn missing_file_loads_defaults_without_creating_it() {
        let scratch = Scratch::new("missing");
        let path = scratch.file("config.json");
        assert_eq!(Config::load_from(&path).unwrap(), Config::default());
        assert!(!path.exists());
    }

    #[test]
    fn saved_configuration_loads_back_unchanged() {
        let scratch = Scratch::new("round-trip");
        let path = scratch.file("config.json");
        let config = Config {
            scene: Scene::Side,
            startup: Startup {
                interval_ms: 500,
                ..Startup::default()
            },
            ..Config::default()
        };
        config.save_to(&path).unwrap();
        assert_eq!(Config::load_from(&path).unwrap(), config);
    }

    #[test]
    fn saving_an_invalid_configuration_writes_nothing() {
        let scratch = Scratch::new("invalid-save");
        let path = scratch.file("config.json");
        let mut config = Config::default();
        config.colors.palette.clear();
        assert!(config.save_to(&path).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn version_2_file_is_migrated_and_the_original_kept_as_backup() {
        let scratch = Scratch::new("migrate-file");
        let path = scratch.file("config.json");
        fs::write(&path, LEGACY_V2).unwrap();

        let config = Config::load_from(&path).unwrap();

        assert_eq!(config.scene, Scene::Dashboard);
        assert_eq!(
            fs::read_to_string(scratch.file("config.json.bak")).unwrap(),
            LEGACY_V2
        );
        assert!(fs::read_to_string(&path)
            .unwrap()
            .contains("\"version\": 3"));
        assert_eq!(Config::load_from(&path).unwrap(), config);
    }

    #[test]
    fn backups_are_numbered_instead_of_overwritten() {
        let scratch = Scratch::new("backup-numbering");
        let path = scratch.file("config.json");
        fs::write(scratch.file("config.json.bak"), "older").unwrap();
        fs::write(&path, LEGACY_V1).unwrap();

        Config::load_from(&path).unwrap();

        assert_eq!(
            fs::read_to_string(scratch.file("config.json.bak")).unwrap(),
            "older"
        );
        assert_eq!(
            fs::read_to_string(scratch.file("config.json.bak.1")).unwrap(),
            LEGACY_V1
        );
    }

    #[test]
    fn unusable_files_are_moved_aside_and_defaults_are_used() {
        let scratch = Scratch::new("quarantine");
        let cases = [
            ("syntax", "{ not json"),
            ("version", r#"{"version": 4}"#),
            ("content", r#"{"version": 3, "scene": "nope"}"#),
            ("not-object", "[1, 2]"),
        ];
        for (name, content) in cases {
            let path = scratch.file(&format!("{name}.json"));
            fs::write(&path, content).unwrap();

            assert_eq!(
                Config::load_from(&path).unwrap(),
                Config::default(),
                "{name}"
            );
            assert!(!path.exists(), "{name}");
            assert!(
                scratch.file(&format!("{name}.json.invalid")).exists(),
                "{name}"
            );
        }
    }

    #[test]
    fn out_of_range_values_in_a_current_file_are_clamped_not_discarded() {
        let scratch = Scratch::new("clamp-file");
        let path = scratch.file("config.json");
        fs::write(&path, r#"{"version": 3, "startup": {"interval_ms": 5}}"#).unwrap();

        let config = Config::load_from(&path).unwrap();

        assert_eq!(config.startup.interval_ms, MIN_INTERVAL_MS);
        assert!(path.exists());
    }

    #[test]
    fn current_file_without_icons_key_keeps_nerd_icons() {
        let scratch = Scratch::new("icons-default");
        let path = scratch.file("config.json");
        fs::write(&path, r#"{"version": 3, "layout": {"gap": 4}}"#).unwrap();

        let config = Config::load_from(&path).unwrap();

        assert_eq!(config.layout.gap, 4);
        assert_eq!(config.layout.icons, IconSet::Nerd);
    }

    #[test]
    fn boolean_icons_setting_still_loads() {
        let scratch = Scratch::new("icons-bool");
        let path = scratch.file("config.json");
        fs::write(&path, r#"{"version": 3, "layout": {"icons": true}}"#).unwrap();
        let on = Config::load_from(&path).unwrap();
        fs::write(&path, r#"{"version": 3, "layout": {"icons": false}}"#).unwrap();
        let off = Config::load_from(&path).unwrap();

        assert_eq!(on.layout.icons, IconSet::Nerd);
        assert_eq!(off.layout.icons, IconSet::None);
    }

    #[test]
    fn icon_set_name_loads_from_a_current_file() {
        let scratch = Scratch::new("icons-name");
        let path = scratch.file("config.json");
        fs::write(&path, r#"{"version": 3, "layout": {"icons": "unicode"}}"#).unwrap();

        let config = Config::load_from(&path).unwrap();

        assert_eq!(config.layout.icons, IconSet::Unicode);
    }

    #[test]
    fn saved_icon_set_is_written_as_a_string() {
        let scratch = Scratch::new("icons-save");
        let path = scratch.file("config.json");
        Config::default().save_to(&path).unwrap();

        let saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();

        assert_eq!(saved["layout"]["icons"], "nerd");
    }

    #[test]
    fn config_parent_uses_the_directory_of_the_file() {
        assert_eq!(
            config_parent(Path::new("/tmp/halofetch/custom.json")).unwrap(),
            PathBuf::from("/tmp/halofetch")
        );
    }

    #[test]
    fn migrates_the_legacy_directory_when_the_new_one_is_missing() {
        let scratch = Scratch::new("migrate-legacy");
        let legacy = scratch.file(LEGACY_DIR);
        let new_dir = scratch.file(CONFIG_DIR);
        fs::create_dir_all(legacy.join("logos")).unwrap();
        fs::write(legacy.join("config.json"), "{}").unwrap();
        fs::write(legacy.join("logos").join("x.txt"), "logo").unwrap();

        migrate_legacy_dir(&new_dir);

        assert!(new_dir.join("config.json").is_file());
        assert!(new_dir.join("logos").join("x.txt").is_file());
        assert!(!legacy.exists());
    }

    #[test]
    fn keeps_both_directories_when_the_new_one_exists() {
        let scratch = Scratch::new("keep-existing");
        let legacy = scratch.file(LEGACY_DIR);
        let new_dir = scratch.file(CONFIG_DIR);
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("config.json"), "legacy").unwrap();
        fs::create_dir_all(&new_dir).unwrap();
        fs::write(new_dir.join("config.json"), "new").unwrap();

        migrate_legacy_dir(&new_dir);

        assert_eq!(
            fs::read_to_string(new_dir.join("config.json")).unwrap(),
            "new"
        );
        assert_eq!(
            fs::read_to_string(legacy.join("config.json")).unwrap(),
            "legacy"
        );
    }

    #[test]
    fn does_nothing_without_a_legacy_directory() {
        let scratch = Scratch::new("no-legacy");
        let new_dir = scratch.file(CONFIG_DIR);

        migrate_legacy_dir(&new_dir);

        assert!(!new_dir.exists());
    }
}
