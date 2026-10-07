//! Editor state and every change the keys make. Nothing here touches the terminal,
//! so tests drive the app with synthetic events.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::widgets::ListState;

use crate::config::{self, Config, FieldEntry, InfoStyle, Scene, StartupMode};
use crate::field::{Field, FieldGroup};
use crate::info::SysInfo;
use crate::logo::{self, Logo, LogoSet, LogoSource};
use crate::theme::{self, Color};

use super::input::{InputResult, TextInput};

const STATUS_TTL: Duration = Duration::from_secs(4);
const MAX_SPACING: usize = 20;
const MAX_CASCADE: usize = 10;
const MIN_VALUE_WIDTH: usize = 8;
const MAX_VALUE_WIDTH: usize = 200;
const MIN_INTERVAL_MS: u64 = 100;
const MAX_INTERVAL_MS: u64 = 60_000;
const MAX_PALETTE_COLORS: usize = 16;
const MAX_PALETTE_NAME: usize = 32;
const MAX_LABEL: usize = 24;
const MAX_ICON: usize = 4;
const MAX_TITLE_FORMAT: usize = 64;
const MAX_SEPARATOR: usize = 4;
const LOGO_FILTER_MAX: usize = 32;
const ADD_FILTER_MAX: usize = 32;
const THEME_FILTER_MAX: usize = 32;
const PAGE_STEP: usize = 10;
const CUSTOM_LOGO_FILE: &str = "custom-logo.txt";

/// Left menu entries, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Appearance,
    Logo,
    Fields,
    Layout,
    Startup,
}

impl Section {
    pub const ALL: [Section; 5] = [
        Section::Appearance,
        Section::Logo,
        Section::Fields,
        Section::Layout,
        Section::Startup,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Section::Appearance => "Appearance",
            Section::Logo => "Logo",
            Section::Fields => "Fields",
            Section::Layout => "Layout",
            Section::Startup => "Startup",
        }
    }

    pub(super) fn index(self) -> usize {
        match self {
            Section::Appearance => 0,
            Section::Logo => 1,
            Section::Fields => 2,
            Section::Layout => 3,
            Section::Startup => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Menu,
    Content,
}

/// How the editor session ends when it returns a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Save,
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub const fn label(self) -> &'static str {
        match self {
            Side::Left => "Left panel",
            Side::Right => "Right panel",
        }
    }
}

/// One editable row of a form section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Theme,
    Palette,
    Gradient,
    InfoStyle,
    TitleColor,
    SeparatorColor,
    ValueColor,
    SavePalette,
    Scene,
    Gap,
    Padding,
    Cascade,
    MaxValueWidth,
    HideEmpty,
    ColorBlocks,
    ShowTitle,
    TitleFormat,
    TitleSeparator,
    Mode,
    Interval,
    LogoSource,
    LogoCompact,
    LogoPath,
    /// Legacy Windows consoles do not turn pastes into `Event::Paste`, so the logo can be
    /// taken from the clipboard instead.
    #[cfg(windows)]
    PasteLogo,
}

impl Setting {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Theme => "Theme",
            Self::Palette => "Palette",
            Self::Gradient => "Gradient",
            Self::InfoStyle => "Info style",
            Self::TitleColor => "Title color",
            Self::SeparatorColor => "Separator color",
            Self::ValueColor => "Value color",
            Self::SavePalette => "Save palette as…",
            Self::Scene => "Scene",
            Self::Gap => "Gap",
            Self::Padding => "Padding",
            Self::Cascade => "Cascade",
            Self::MaxValueWidth => "Max value width",
            Self::HideEmpty => "Hide empty fields",
            Self::ColorBlocks => "Color blocks",
            Self::ShowTitle => "Show title",
            Self::TitleFormat => "Title format",
            Self::TitleSeparator => "Separator",
            Self::Mode => "Mode",
            Self::Interval => "Refresh interval",
            Self::LogoSource => "Source",
            Self::LogoCompact => "Compact on narrow terminals",
            Self::LogoPath => "File path",
            #[cfg(windows)]
            Self::PasteLogo => "Paste logo from clipboard",
        }
    }

    pub const fn is_bool(self) -> bool {
        matches!(
            self,
            Self::HideEmpty | Self::ColorBlocks | Self::ShowTitle | Self::LogoCompact
        )
    }

    pub const fn is_text(self) -> bool {
        matches!(
            self,
            Self::Palette
                | Self::TitleColor
                | Self::SeparatorColor
                | Self::ValueColor
                | Self::SavePalette
                | Self::TitleFormat
                | Self::TitleSeparator
                | Self::LogoPath
        )
    }

    /// Left and Right change the value of these rows; elsewhere Left returns to the menu.
    pub const fn adjustable(self) -> bool {
        self.is_number() || self.is_choice()
    }

    pub const fn is_number(self) -> bool {
        matches!(
            self,
            Self::Gap | Self::Padding | Self::Cascade | Self::MaxValueWidth | Self::Interval
        )
    }

    pub const fn is_choice(self) -> bool {
        matches!(
            self,
            Self::Theme
                | Self::Gradient
                | Self::InfoStyle
                | Self::Scene
                | Self::Mode
                | Self::LogoSource
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmChoice {
    Save,
    Discard,
    Cancel,
}

impl ConfirmChoice {
    fn shift(self, delta: i32) -> Self {
        const ORDER: [ConfirmChoice; 3] = [
            ConfirmChoice::Save,
            ConfirmChoice::Discard,
            ConfirmChoice::Cancel,
        ];
        let current = ORDER.iter().position(|c| *c == self).unwrap_or(0) as i32;
        let next = (current + delta).clamp(0, ORDER.len() as i32 - 1);
        ORDER[next as usize]
    }
}

/// A popup that edits one value as text.
#[derive(Debug, Clone, PartialEq)]
pub struct TextPopup {
    /// The row the value belongs to; `SavePalette` means "name for a new palette".
    pub target: Setting,
    pub input: TextInput,
    pub error: Option<String>,
    /// Name of a palette that exists and was confirmed once; a second Enter overwrites it.
    pub armed: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Popup {
    Help {
        scroll: u16,
    },
    Fullscreen,
    Confirm {
        choice: ConfirmChoice,
    },
    Text(TextPopup),
    Themes {
        filter: TextInput,
        state: ListState,
    },
    Entry {
        side: Side,
        index: usize,
        state: ListState,
        editing: Option<TextInput>,
    },
    Add {
        filter: TextInput,
        state: ListState,
        anchor: Option<(Side, usize)>,
    },
}

#[derive(Debug, Clone)]
pub struct Status {
    pub text: String,
    pub error: bool,
    pub at: Instant,
}

/// One line of the Fields section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldRow {
    Header(Side),
    Entry(Side, usize),
}

/// One line of the Add popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddRow {
    Group(FieldGroup),
    Field(Field),
}

pub struct App {
    pub cfg: Config,
    saved: Config,
    pub(super) info: SysInfo,
    user_logo_dir: Option<PathBuf>,
    logo_keys: Vec<String>,
    pub(super) logos: LogoSet,
    logo_key: Option<(LogoSource, bool, Option<String>)>,
    pub(super) section: Section,
    pub(super) focus: Focus,
    pub(super) menu_state: ListState,
    pub(super) form_sel: [usize; 5],
    pub(super) form_states: [ListState; 5],
    pub(super) logo_on_list: bool,
    pub(super) logo_filter: String,
    pub(super) logo_state: ListState,
    pub(super) fields_state: ListState,
    pub(super) popup: Option<Popup>,
    pub(super) status: Option<Status>,
    pending_logo: Option<String>,
    undo: Option<(Side, usize, FieldEntry)>,
    quit: Option<Outcome>,
    /// True until the first key press while the editor opened for the first time.
    pub(super) welcome: bool,
}

impl App {
    pub fn new(cfg: Config, first_run: bool, info: SysInfo) -> App {
        let user_dir = config::user_logo_dir();
        let keys = logo::available(user_dir.as_deref());
        App::build(cfg, first_run, info, user_dir, keys)
    }

    #[cfg(test)]
    pub fn new_for_test(cfg: Config, info: SysInfo) -> App {
        let keys = logo::builtin_keys()
            .into_iter()
            .map(str::to_string)
            .collect();
        App::build(cfg, false, info, None, keys)
    }

    fn build(
        cfg: Config,
        first_run: bool,
        info: SysInfo,
        user_logo_dir: Option<PathBuf>,
        logo_keys: Vec<String>,
    ) -> App {
        let mut app = App {
            saved: cfg.clone(),
            cfg,
            info,
            user_logo_dir,
            logo_keys,
            logos: LogoSet::default(),
            logo_key: None,
            section: Section::Appearance,
            focus: Focus::Menu,
            menu_state: ListState::default().with_selected(Some(0)),
            form_sel: [0; 5],
            form_states: std::array::from_fn(|_| ListState::default().with_selected(Some(0))),
            logo_on_list: false,
            logo_filter: String::new(),
            logo_state: ListState::default(),
            fields_state: ListState::default(),
            popup: None,
            status: None,
            pending_logo: None,
            undo: None,
            quit: None,
            welcome: first_run,
        };
        app.refresh_logos();
        app.select_first_entry();
        app.sync_logo_selection();
        app
    }

    /// Whether the configuration differs from what was loaded, or pasted logo art is waiting to be written.
    pub fn is_dirty(&self) -> bool {
        self.cfg != self.saved || self.pending_logo.is_some()
    }

    /// Set once the user chose to leave the editor.
    pub fn outcome(&self) -> Option<Outcome> {
        self.quit
    }

    /// Logo art pasted in this session; `run` writes it next to the configuration when saving.
    pub fn pending_logo_text(&self) -> Option<&str> {
        self.pending_logo.as_deref()
    }

    /// Expire the status message. Returns true when something visible changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        match &self.status {
            Some(status) if now.duration_since(status.at) >= STATUS_TTL => {
                self.status = None;
                true
            }
            _ => false,
        }
    }

    pub fn section(&self) -> Section {
        self.section
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }

    pub fn welcome_visible(&self) -> bool {
        self.welcome
    }

    pub fn info(&self) -> &SysInfo {
        &self.info
    }

    pub fn logos(&self) -> &LogoSet {
        &self.logos
    }

    fn set_status(&mut self, text: impl Into<String>, error: bool) {
        self.status = Some(Status {
            text: text.into(),
            error,
            at: Instant::now(),
        });
    }

    /// Rebuild the preview logo only when the logo settings or pasted art changed.
    fn refresh_logos(&mut self) {
        let key = (
            self.cfg.logo.source.clone(),
            self.cfg.logo.auto_small,
            self.pending_logo.clone(),
        );
        if self.logo_key.as_ref() == Some(&key) {
            return;
        }
        self.logos = match &self.pending_logo {
            Some(text) => LogoSet {
                full: Logo::from_text(text),
                small: None,
            },
            None => logo::resolve(
                &self.cfg.logo.source,
                &self.info.os_ids,
                self.user_logo_dir.as_deref(),
                self.cfg.logo.auto_small,
            ),
        };
        self.logo_key = Some(key);
    }

    // ── Setting values ───────────────────────────────────────────────────

    /// Name of the palette the colors match: a built-in theme first, then a custom one.
    pub(super) fn theme_name(&self) -> Option<String> {
        let colors = &self.cfg.colors.palette;
        if let Some(name) = theme::theme_name_for(colors) {
            return Some(name.to_string());
        }
        self.cfg
            .custom_palettes
            .iter()
            .find(|(_, palette)| *palette == colors)
            .map(|(name, _)| name.clone())
    }

    /// Every palette the theme choice and picker offer: built-ins, then custom ones.
    pub(super) fn theme_choices(&self) -> Vec<(String, Vec<Color>)> {
        let mut choices: Vec<(String, Vec<Color>)> = theme::all_themes()
            .into_iter()
            .map(|theme| (theme.name.to_string(), theme.colors))
            .collect();
        choices.extend(
            self.cfg
                .custom_palettes
                .iter()
                .map(|(name, colors)| (name.clone(), colors.clone())),
        );
        choices
    }

    pub(super) fn setting_value(&self, setting: Setting) -> String {
        let cfg = &self.cfg;
        let on_off = |on: bool| if on { "on" } else { "off" }.to_string();
        match setting {
            Setting::Theme => self.theme_name().unwrap_or_else(|| "custom".to_string()),
            Setting::Palette => format!("{} colors", cfg.colors.palette.len()),
            Setting::Gradient => cfg.logo.gradient.label().to_string(),
            Setting::InfoStyle => match cfg.layout.style {
                InfoStyle::Powerline => "Powerline",
                InfoStyle::Plain => "Plain",
            }
            .to_string(),
            Setting::TitleColor => cfg.colors.title.to_string(),
            Setting::SeparatorColor => cfg.colors.separator.to_string(),
            Setting::ValueColor => cfg.colors.value.to_string(),
            Setting::SavePalette => String::new(),
            Setting::Scene => cfg.scene.label().to_string(),
            Setting::Gap => cfg.layout.gap.to_string(),
            Setting::Padding => cfg.layout.padding.to_string(),
            Setting::Cascade => cfg.layout.cascade.to_string(),
            Setting::MaxValueWidth => match cfg.layout.max_value_width {
                0 => "auto".to_string(),
                width => width.to_string(),
            },
            Setting::HideEmpty => on_off(cfg.layout.hide_empty),
            Setting::ColorBlocks => on_off(cfg.layout.color_blocks),
            Setting::ShowTitle => on_off(cfg.title.enabled),
            Setting::TitleFormat => cfg.title.format.clone(),
            Setting::TitleSeparator => cfg.title.separator.clone(),
            Setting::Mode => match cfg.startup.mode {
                StartupMode::Fetch => "Fetch",
                StartupMode::Monitor => "Monitor",
            }
            .to_string(),
            Setting::Interval => format!("{:.1} s", cfg.startup.interval_ms as f64 / 1000.0),
            Setting::LogoSource => match &cfg.logo.source {
                LogoSource::Auto => format!("Auto ({})", logo::detect_key(&self.info.os_ids)),
                LogoSource::Builtin { key } => format!("Built-in ({key})"),
                LogoSource::File { .. } => "File".to_string(),
                LogoSource::None => "None".to_string(),
            },
            Setting::LogoCompact => on_off(cfg.logo.auto_small),
            Setting::LogoPath => match &cfg.logo.source {
                LogoSource::File { path } => path.clone(),
                _ => String::new(),
            },
            #[cfg(windows)]
            Setting::PasteLogo => "press Enter".to_string(),
        }
    }

    /// Colors drawn as swatches next to a row's value.
    pub(super) fn setting_swatch(&self, setting: Setting) -> Vec<Color> {
        match setting {
            Setting::Palette => self.cfg.colors.palette.clone(),
            Setting::TitleColor => vec![self.cfg.colors.title],
            Setting::SeparatorColor => vec![self.cfg.colors.separator],
            Setting::ValueColor => vec![self.cfg.colors.value],
            _ => Vec::new(),
        }
    }

    /// Rows of a form section, top to bottom. The Fields section has its own list.
    pub(super) fn section_rows(&self, section: Section) -> Vec<Setting> {
        match section {
            Section::Appearance => vec![
                Setting::Theme,
                Setting::Palette,
                Setting::Gradient,
                Setting::InfoStyle,
                Setting::TitleColor,
                Setting::SeparatorColor,
                Setting::ValueColor,
                Setting::SavePalette,
            ],
            Section::Layout => vec![
                Setting::Scene,
                Setting::Gap,
                Setting::Padding,
                Setting::Cascade,
                Setting::MaxValueWidth,
                Setting::HideEmpty,
                Setting::ColorBlocks,
                Setting::ShowTitle,
                Setting::TitleFormat,
                Setting::TitleSeparator,
            ],
            Section::Startup => vec![Setting::Mode, Setting::Interval],
            Section::Logo => {
                let mut rows = vec![Setting::LogoSource, Setting::LogoCompact];
                if matches!(self.cfg.logo.source, LogoSource::File { .. }) {
                    rows.push(Setting::LogoPath);
                }
                #[cfg(windows)]
                rows.push(Setting::PasteLogo);
                rows
            }
            Section::Fields => Vec::new(),
        }
    }
}

impl App {
    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                self.welcome = false;
                self.handle_key(key);
            }
            Event::Paste(text) => self.handle_paste(&text),
            _ => {}
        }
        self.refresh_logos();
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if is_ctrl(&key, 'c') {
            self.quit = Some(Outcome::Discard);
            return;
        }
        if self.popup.is_some() {
            self.handle_popup_key(key);
            return;
        }
        if is_ctrl(&key, 's') {
            self.request_save();
            return;
        }
        if is_ctrl(&key, 'z') {
            self.undo_removal();
            return;
        }
        match key.code {
            KeyCode::F(1) => {
                self.popup = Some(Popup::Help { scroll: 0 });
                return;
            }
            KeyCode::F(2) => {
                self.popup = Some(Popup::Fullscreen);
                return;
            }
            KeyCode::Char('?') if !self.typing() => {
                self.popup = Some(Popup::Help { scroll: 0 });
                return;
            }
            KeyCode::Char('q') if !self.typing() => {
                self.request_quit();
                return;
            }
            _ => {}
        }
        match self.focus {
            Focus::Menu => self.menu_key(key),
            Focus::Content => self.content_key(key),
        }
    }

    /// True while printable keys go into the logo filter.
    fn typing(&self) -> bool {
        self.focus == Focus::Content && self.section == Section::Logo && self.logo_on_list
    }

    fn handle_paste(&mut self, text: &str) {
        match &mut self.popup {
            Some(Popup::Text(popup)) => {
                popup.input.insert_str(text);
                popup.error = text_error(popup.target, popup.input.value());
                return;
            }
            Some(Popup::Themes { filter, state }) => {
                filter.insert_str(text);
                state.select(Some(0));
                return;
            }
            Some(Popup::Add { filter, state, .. }) => {
                filter.insert_str(text);
                state.select(first_add_field(filter.value()));
                return;
            }
            Some(Popup::Entry {
                editing: Some(input),
                ..
            }) => {
                input.insert_str(text);
                return;
            }
            Some(_) => return,
            None => {}
        }
        if self.section != Section::Logo {
            return;
        }
        if text.contains(['\n', '\r']) {
            self.paste_logo(text);
        } else if self.typing() {
            for ch in text.chars().filter(|ch| !ch.is_control()) {
                self.logo_filter.push(ch);
            }
            self.refilter_logos();
        }
    }

    fn request_quit(&mut self) {
        if self.is_dirty() {
            self.popup = Some(Popup::Confirm {
                choice: ConfirmChoice::Save,
            });
        } else {
            self.quit = Some(Outcome::Discard);
        }
    }

    /// Validates the configuration and ends the session with `Save`. A failed
    /// validation keeps the editor open and reports the problem.
    fn request_save(&mut self) -> bool {
        match self.cfg.validate() {
            Ok(()) => {
                self.quit = Some(Outcome::Save);
                true
            }
            Err(err) => {
                self.set_status(format!("Cannot save: {err}"), true);
                false
            }
        }
    }

    /// Ctrl+S inside a popup: an open text input is applied first, then the configuration saved.
    fn popup_save(&mut self) {
        match self.popup.take() {
            Some(Popup::Text(text)) => {
                if let Some(text) = self.submit_text(text) {
                    self.popup = Some(Popup::Text(text));
                    return;
                }
            }
            Some(Popup::Entry {
                side,
                index,
                state,
                editing: Some(input),
            }) => {
                let row = state.selected().unwrap_or(0);
                self.entry_commit(side, index, row, input.value());
            }
            _ => {}
        }
        self.request_save();
    }

    fn handle_popup_key(&mut self, key: KeyEvent) {
        if is_ctrl(&key, 's') {
            self.popup_save();
            return;
        }
        let Some(popup) = self.popup.take() else {
            return;
        };
        self.popup = match popup {
            Popup::Help { scroll } => help_key(key, scroll),
            Popup::Fullscreen => None,
            Popup::Confirm { choice } => self.confirm_key(key, choice),
            Popup::Text(text) => self.text_key(key, text),
            Popup::Themes { filter, state } => self.themes_key(key, filter, state),
            Popup::Entry {
                side,
                index,
                state,
                editing,
            } => self.entry_key(key, side, index, state, editing),
            Popup::Add {
                filter,
                state,
                anchor,
            } => self.add_key(key, filter, state, anchor),
        };
    }

    fn confirm_key(&mut self, key: KeyEvent, choice: ConfirmChoice) -> Option<Popup> {
        match key.code {
            KeyCode::Esc => None,
            KeyCode::Left => Some(Popup::Confirm {
                choice: choice.shift(-1),
            }),
            KeyCode::Right => Some(Popup::Confirm {
                choice: choice.shift(1),
            }),
            KeyCode::Enter => {
                match choice {
                    ConfirmChoice::Save => {
                        self.request_save();
                    }
                    ConfirmChoice::Discard => self.quit = Some(Outcome::Discard),
                    ConfirmChoice::Cancel => {}
                }
                None
            }
            _ => Some(Popup::Confirm { choice }),
        }
    }

    fn text_key(&mut self, key: KeyEvent, mut text: TextPopup) -> Option<Popup> {
        match text.input.handle(key) {
            InputResult::Submit => self.submit_text(text).map(Popup::Text),
            InputResult::Cancel => None,
            InputResult::Continue => {
                text.error = text_error(text.target, text.input.value());
                Some(Popup::Text(text))
            }
        }
    }

    /// Applies a text popup. Returns the popup back when the input is invalid, `None` once applied.
    fn submit_text(&mut self, mut text: TextPopup) -> Option<TextPopup> {
        let raw = text.input.value().to_string();
        if let Some(error) = text_error(text.target, &raw) {
            text.error = Some(error);
            return Some(text);
        }
        let trimmed = raw.trim();
        match text.target {
            Setting::Palette => {
                if let Some(colors) = parse_palette(trimmed) {
                    self.cfg.colors.palette = colors;
                }
            }
            Setting::TitleColor => {
                if let Some(color) = parse_color(trimmed) {
                    self.cfg.colors.title = color;
                }
            }
            Setting::SeparatorColor => {
                if let Some(color) = parse_color(trimmed) {
                    self.cfg.colors.separator = color;
                }
            }
            Setting::ValueColor => {
                if let Some(color) = parse_color(trimmed) {
                    self.cfg.colors.value = color;
                }
            }
            Setting::TitleFormat => self.cfg.title.format = raw.clone(),
            Setting::TitleSeparator => self.cfg.title.separator = raw.clone(),
            Setting::LogoPath => {
                self.cfg.logo.source = LogoSource::File {
                    path: trimmed.to_string(),
                };
            }
            Setting::SavePalette => {
                if self.cfg.custom_palettes.contains_key(trimmed)
                    && text.armed.as_deref() != Some(trimmed)
                {
                    text.error = Some(format!(
                        "'{trimmed}' exists — press Enter again to overwrite"
                    ));
                    text.armed = Some(trimmed.to_string());
                    return Some(text);
                }
                let colors = self.cfg.colors.palette.clone();
                self.cfg.custom_palettes.insert(trimmed.to_string(), colors);
                self.set_status(format!("Saved palette '{trimmed}'"), false);
                return None;
            }
            _ => return None,
        }
        self.set_status(format!("{} updated", text.target.label()), false);
        None
    }

    fn open_text(&mut self, target: Setting) {
        let (initial, max) = match target {
            Setting::Palette => (
                self.cfg
                    .colors
                    .palette
                    .iter()
                    .map(Color::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
                160,
            ),
            Setting::TitleColor => (self.cfg.colors.title.to_string(), 7),
            Setting::SeparatorColor => (self.cfg.colors.separator.to_string(), 7),
            Setting::ValueColor => (self.cfg.colors.value.to_string(), 7),
            Setting::SavePalette => (String::new(), MAX_PALETTE_NAME),
            Setting::TitleFormat => (self.cfg.title.format.clone(), MAX_TITLE_FORMAT),
            Setting::TitleSeparator => (self.cfg.title.separator.clone(), MAX_SEPARATOR),
            Setting::LogoPath => (self.setting_value(Setting::LogoPath), 512),
            _ => return,
        };
        self.popup = Some(Popup::Text(TextPopup {
            target,
            input: TextInput::new(&initial, max),
            error: None,
            armed: None,
        }));
    }

    fn open_themes(&mut self) {
        let choices = self.theme_choices();
        let index = self
            .theme_name()
            .and_then(|name| choices.iter().position(|(n, _)| *n == name))
            .unwrap_or(0);
        self.popup = Some(Popup::Themes {
            filter: TextInput::new("", THEME_FILTER_MAX),
            state: ListState::default().with_selected(Some(index)),
        });
    }

    pub(super) fn filtered_themes(&self, filter: &str) -> Vec<(String, Vec<Color>)> {
        let needle = filter.to_lowercase();
        self.theme_choices()
            .into_iter()
            .filter(|(name, _)| name.to_lowercase().contains(&needle))
            .collect()
    }

    fn themes_key(
        &mut self,
        key: KeyEvent,
        mut filter: TextInput,
        mut state: ListState,
    ) -> Option<Popup> {
        match key.code {
            KeyCode::Esc => return None,
            KeyCode::Enter => {
                let choices = self.filtered_themes(filter.value());
                if let Some((name, colors)) =
                    state.selected().and_then(|i| choices.into_iter().nth(i))
                {
                    self.cfg.colors.palette = colors;
                    self.set_status(format!("Palette: {name}"), false);
                }
                return None;
            }
            _ => {}
        }
        let len = self.filtered_themes(filter.value()).len();
        if !list_nav(&mut state, len, key.code) {
            let before = filter.value().to_string();
            filter.handle(key);
            if filter.value() != before {
                state.select(Some(0));
            }
        }
        Some(Popup::Themes { filter, state })
    }

    /// Cycles through built-in and custom palettes. A custom palette that matches none shows as "custom".
    fn cycle_theme(&mut self, delta: i32) {
        let choices = self.theme_choices();
        let len = choices.len();
        if len == 0 {
            return;
        }
        let current = self
            .theme_name()
            .and_then(|name| choices.iter().position(|(n, _)| *n == name));
        let next = match current {
            Some(index) => (index as i64 + i64::from(delta)).rem_euclid(len as i64) as usize,
            None if delta > 0 => 0,
            None => len - 1,
        };
        if let Some((name, colors)) = choices.into_iter().nth(next) {
            self.cfg.colors.palette = colors;
            self.set_status(format!("Palette: {name}"), false);
        }
    }

    fn menu_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.select_section_offset(-1),
            KeyCode::Down => self.select_section_offset(1),
            KeyCode::Right | KeyCode::Enter | KeyCode::Tab => self.focus = Focus::Content,
            _ => {}
        }
    }

    fn select_section_offset(&mut self, delta: i32) {
        let current = self.section.index() as i64;
        let next = (current + i64::from(delta)).clamp(0, Section::ALL.len() as i64 - 1) as usize;
        if let Some(section) = Section::ALL.get(next).copied() {
            self.select_section(section);
        }
    }

    fn select_section(&mut self, section: Section) {
        self.section = section;
        self.logo_on_list = false;
        self.menu_state.select(Some(section.index()));
        self.sync_logo_selection();
    }

    fn go_menu(&mut self) {
        self.focus = Focus::Menu;
        self.logo_on_list = false;
    }

    fn content_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                self.go_menu();
                return;
            }
            KeyCode::Esc => {
                if self.logo_on_list && !self.logo_filter.is_empty() {
                    self.logo_filter.clear();
                    self.refilter_logos();
                } else {
                    self.go_menu();
                }
                return;
            }
            _ => {}
        }
        match self.section {
            Section::Fields => self.fields_key(key),
            Section::Logo if self.logo_on_list => self.logo_list_key(key),
            _ => self.form_key(key),
        }
    }

    pub(super) fn set_form_row(&mut self, section: Section, row: usize) {
        self.form_sel[section.index()] = row;
        self.form_states[section.index()].select(Some(row));
    }

    fn form_key(&mut self, key: KeyEvent) {
        let section = self.section;
        let rows = self.section_rows(section);
        let Some(last) = rows.len().checked_sub(1) else {
            if key.code == KeyCode::Left {
                self.go_menu();
            }
            return;
        };
        let row = self.form_sel[section.index()].min(last);
        let setting = rows[row];
        match key.code {
            KeyCode::Up => self.set_form_row(section, row.saturating_sub(1)),
            KeyCode::Down => {
                if row < last {
                    self.set_form_row(section, row + 1);
                } else if section == Section::Logo {
                    self.enter_logo_list();
                }
            }
            KeyCode::Left => {
                if setting.adjustable() {
                    self.adjust(setting, -1);
                } else {
                    self.go_menu();
                }
            }
            KeyCode::Right => {
                if setting.adjustable() {
                    self.adjust(setting, 1);
                }
            }
            KeyCode::Enter => self.activate(setting),
            KeyCode::Char(' ') if setting.is_bool() => self.toggle(setting),
            _ => {}
        }
    }

    /// Enter on a row: open its editor, toggle a flag, or step a choice forward.
    fn activate(&mut self, setting: Setting) {
        match setting {
            Setting::Theme => self.open_themes(),
            #[cfg(windows)]
            Setting::PasteLogo => self.paste_clipboard_logo(),
            s if s.is_bool() => self.toggle(s),
            s if s.is_text() => self.open_text(s),
            s if s.is_choice() => self.adjust(s, 1),
            _ => {}
        }
    }

    fn toggle(&mut self, setting: Setting) {
        match setting {
            Setting::HideEmpty => self.cfg.layout.hide_empty = !self.cfg.layout.hide_empty,
            Setting::ColorBlocks => self.cfg.layout.color_blocks = !self.cfg.layout.color_blocks,
            Setting::ShowTitle => self.cfg.title.enabled = !self.cfg.title.enabled,
            Setting::LogoCompact => self.cfg.logo.auto_small = !self.cfg.logo.auto_small,
            _ => {}
        }
    }

    /// Left (-1) and Right (+1) change choices and numbers.
    fn adjust(&mut self, setting: Setting, delta: i32) {
        match setting {
            Setting::Theme => self.cycle_theme(delta),
            Setting::Gradient => self.cfg.logo.gradient = self.cfg.logo.gradient.toggled(),
            Setting::InfoStyle => {
                self.cfg.layout.style = match self.cfg.layout.style {
                    InfoStyle::Powerline => InfoStyle::Plain,
                    InfoStyle::Plain => InfoStyle::Powerline,
                }
            }
            Setting::Scene => {
                let scenes = Scene::ALL;
                let current = scenes
                    .iter()
                    .position(|scene| *scene == self.cfg.scene)
                    .unwrap_or(0) as i64;
                let next = (current + i64::from(delta)).rem_euclid(scenes.len() as i64) as usize;
                if let Some(scene) = scenes.get(next) {
                    self.cfg.scene = *scene;
                }
            }
            Setting::Mode => {
                self.cfg.startup.mode = match self.cfg.startup.mode {
                    StartupMode::Fetch => StartupMode::Monitor,
                    StartupMode::Monitor => StartupMode::Fetch,
                }
            }
            Setting::LogoSource => self.cycle_logo_source(delta),
            Setting::Gap => {
                self.cfg.layout.gap = step_clamped(self.cfg.layout.gap, delta, 0, MAX_SPACING)
            }
            Setting::Padding => {
                self.cfg.layout.padding =
                    step_clamped(self.cfg.layout.padding, delta, 0, MAX_SPACING)
            }
            Setting::Cascade => {
                self.cfg.layout.cascade =
                    step_clamped(self.cfg.layout.cascade, delta, 0, MAX_CASCADE)
            }
            Setting::MaxValueWidth => {
                self.cfg.layout.max_value_width =
                    step_value_width(self.cfg.layout.max_value_width, delta)
            }
            Setting::Interval => {
                self.cfg.startup.interval_ms = step_interval(self.cfg.startup.interval_ms, delta)
            }
            _ => {}
        }
    }

    fn cycle_logo_source(&mut self, delta: i32) {
        let current = match self.cfg.logo.source {
            LogoSource::Auto => 0,
            LogoSource::Builtin { .. } => 1,
            LogoSource::File { .. } => 2,
            LogoSource::None => 3,
        };
        let next = (current + delta).rem_euclid(4);
        self.cfg.logo.source = match next {
            0 => LogoSource::Auto,
            1 => LogoSource::Builtin {
                key: self.builtin_key_or_detected(),
            },
            2 => LogoSource::File {
                path: match &self.cfg.logo.source {
                    LogoSource::File { path } => path.clone(),
                    _ => String::new(),
                },
            },
            _ => LogoSource::None,
        };
        self.sync_logo_selection();
        if matches!(&self.cfg.logo.source, LogoSource::File { path } if path.is_empty()) {
            self.set_status(
                "Select File path and press Enter to choose a logo file",
                false,
            );
        }
    }

    fn builtin_key_or_detected(&self) -> String {
        match &self.cfg.logo.source {
            LogoSource::Builtin { key } => key.clone(),
            _ => logo::detect_key(&self.info.os_ids),
        }
    }
}

fn is_ctrl(key: &KeyEvent, ch: char) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char(ch)
}

fn help_key(key: KeyEvent, scroll: u16) -> Option<Popup> {
    let page = PAGE_STEP as u16;
    let next = match key.code {
        KeyCode::Esc | KeyCode::F(1) | KeyCode::Char('?') => return None,
        KeyCode::Up => scroll.saturating_sub(1),
        KeyCode::Down => scroll.saturating_add(1),
        KeyCode::PageUp => scroll.saturating_sub(page),
        KeyCode::PageDown => scroll.saturating_add(page),
        KeyCode::Home => 0,
        KeyCode::End => u16::MAX,
        _ => scroll,
    };
    Some(Popup::Help { scroll: next })
}

/// Applies a navigation key to a list selection. Returns false for keys that are not navigation.
fn list_nav(state: &mut ListState, len: usize, code: KeyCode) -> bool {
    let is_nav = matches!(
        code,
        KeyCode::Up
            | KeyCode::Down
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End
    );
    if !is_nav {
        return false;
    }
    let Some(last) = len.checked_sub(1) else {
        return true;
    };
    let current = state.selected().unwrap_or(0).min(last);
    let target = match code {
        KeyCode::Up => current.saturating_sub(1),
        KeyCode::Down => (current + 1).min(last),
        KeyCode::PageUp => current.saturating_sub(PAGE_STEP),
        KeyCode::PageDown => (current + PAGE_STEP).min(last),
        KeyCode::Home => 0,
        _ => last,
    };
    state.select(Some(target));
    true
}

fn step_clamped(value: usize, delta: i32, min: usize, max: usize) -> usize {
    (value as i64 + i64::from(delta)).clamp(min as i64, max as i64) as usize
}

/// Max value width: auto (0), then 8, then steps of 4 up to 200.
fn step_value_width(width: usize, delta: i32) -> usize {
    if delta > 0 {
        if width == 0 {
            MIN_VALUE_WIDTH
        } else {
            (width + 4).min(MAX_VALUE_WIDTH)
        }
    } else if width <= MIN_VALUE_WIDTH {
        0
    } else {
        (width - 4).max(MIN_VALUE_WIDTH)
    }
}

/// Refresh interval: steps of 100 ms below 1 s, 500 ms below 5 s, 1 s above.
fn step_interval(ms: u64, delta: i32) -> u64 {
    let next = if delta > 0 {
        if ms < 1_000 {
            (ms / 100 + 1) * 100
        } else if ms < 5_000 {
            (ms / 500 + 1) * 500
        } else {
            (ms / 1_000 + 1) * 1_000
        }
    } else if ms > 5_000 {
        ms.saturating_sub(1) / 1_000 * 1_000
    } else if ms > 1_000 {
        ms.saturating_sub(1) / 500 * 500
    } else {
        ms.saturating_sub(1) / 100 * 100
    };
    next.clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS)
}

impl App {
    // ── Logo section ─────────────────────────────────────────────────────

    pub(super) fn filtered_logo_keys(&self) -> Vec<String> {
        let needle = self.logo_filter.to_lowercase();
        self.logo_keys
            .iter()
            .filter(|key| key.to_lowercase().contains(&needle))
            .cloned()
            .collect()
    }

    /// The filter text changed: the first match is selected and applied.
    fn refilter_logos(&mut self) {
        let len = self.filtered_logo_keys().len();
        self.logo_state
            .select(if len == 0 { None } else { Some(0) });
        self.apply_logo_selection();
    }

    fn apply_logo_selection(&mut self) {
        let keys = self.filtered_logo_keys();
        if let Some(key) = self.logo_state.selected().and_then(|i| keys.get(i)) {
            self.cfg.logo.source = LogoSource::Builtin { key: key.clone() };
        }
    }

    /// Selects the current built-in logo in the list, or keeps a valid selection otherwise.
    fn sync_logo_selection(&mut self) {
        let keys = self.filtered_logo_keys();
        if keys.is_empty() {
            self.logo_state.select(None);
            return;
        }
        let index = match &self.cfg.logo.source {
            LogoSource::Builtin { key } => keys.iter().position(|k| k == key).unwrap_or(0),
            _ => self
                .logo_state
                .selected()
                .unwrap_or(0)
                .min(keys.len().saturating_sub(1)),
        };
        self.logo_state.select(Some(index));
    }

    fn enter_logo_list(&mut self) {
        self.logo_on_list = true;
        if self.logo_state.selected().is_none() && !self.filtered_logo_keys().is_empty() {
            self.logo_state.select(Some(0));
        }
    }

    fn logo_list_key(&mut self, key: KeyEvent) {
        let len = self.filtered_logo_keys().len();
        match key.code {
            KeyCode::Up if self.logo_state.selected().unwrap_or(0) == 0 => {
                self.logo_on_list = false;
                let rows = self.section_rows(Section::Logo);
                self.set_form_row(Section::Logo, rows.len().saturating_sub(1));
            }
            KeyCode::Left => self.go_menu(),
            KeyCode::Enter => self.apply_logo_selection(),
            KeyCode::Backspace => {
                if self.logo_filter.pop().is_some() {
                    self.refilter_logos();
                }
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.logo_filter.chars().count() < LOGO_FILTER_MAX {
                    self.logo_filter.push(ch);
                    self.refilter_logos();
                }
            }
            code => {
                if list_nav(&mut self.logo_state, len, code) {
                    self.apply_logo_selection();
                }
            }
        }
    }

    /// Multi-line paste becomes the logo; it is written next to the configuration on save.
    fn paste_logo(&mut self, text: &str) {
        let cleaned = logo::clean(text);
        if cleaned.is_empty() {
            self.set_status("Nothing to paste: the text has no visible characters", true);
            return;
        }
        let path = match config::config_dir() {
            Ok(dir) => dir.join(CUSTOM_LOGO_FILE),
            Err(err) => {
                self.set_status(format!("Cannot place the pasted logo: {err}"), true);
                return;
            }
        };
        let lines = cleaned.lines().count();
        self.cfg.logo.source = LogoSource::File {
            path: path.to_string_lossy().into_owned(),
        };
        self.pending_logo = Some(cleaned);
        self.set_status(
            format!("Pasted logo ({lines} lines) — saved with the config"),
            false,
        );
    }

    #[cfg(windows)]
    fn paste_clipboard_logo(&mut self) {
        match super::clipboard::read_text() {
            Some(text) => self.paste_logo(&text),
            None => self.set_status("The clipboard holds no text", true),
        }
    }

    // ── Fields section ───────────────────────────────────────────────────

    /// Header and entry rows of the Fields list, left panel first.
    pub(super) fn field_rows(&self) -> Vec<FieldRow> {
        let mut rows = vec![FieldRow::Header(Side::Left)];
        rows.extend((0..self.cfg.fields.left.len()).map(|i| FieldRow::Entry(Side::Left, i)));
        rows.push(FieldRow::Header(Side::Right));
        rows.extend((0..self.cfg.fields.right.len()).map(|i| FieldRow::Entry(Side::Right, i)));
        rows
    }

    pub(super) fn entries(&self, side: Side) -> &[FieldEntry] {
        match side {
            Side::Left => &self.cfg.fields.left,
            Side::Right => &self.cfg.fields.right,
        }
    }

    fn entries_mut(&mut self, side: Side) -> &mut Vec<FieldEntry> {
        match side {
            Side::Left => &mut self.cfg.fields.left,
            Side::Right => &mut self.cfg.fields.right,
        }
    }

    fn select_first_entry(&mut self) {
        let position = self
            .field_rows()
            .iter()
            .position(|row| matches!(row, FieldRow::Entry(..)));
        self.fields_state.select(position);
    }

    fn select_entry(&mut self, side: Side, index: usize) {
        let position = self
            .field_rows()
            .iter()
            .position(|row| *row == FieldRow::Entry(side, index));
        self.fields_state.select(position);
    }

    fn selected_entry(&self) -> Option<(Side, usize)> {
        let rows = self.field_rows();
        match self.fields_state.selected().and_then(|i| rows.get(i)) {
            Some(FieldRow::Entry(side, index)) => Some((*side, *index)),
            _ => None,
        }
    }

    /// Moves the selection to the previous or next entry, skipping panel headers.
    fn step_entry(&mut self, forward: bool) {
        let rows = self.field_rows();
        let Some(mut position) = self.fields_state.selected() else {
            return;
        };
        loop {
            if forward {
                position += 1;
            } else {
                match position.checked_sub(1) {
                    Some(previous) => position = previous,
                    None => return,
                }
            }
            match rows.get(position) {
                None => return,
                Some(FieldRow::Entry(..)) => {
                    self.fields_state.select(Some(position));
                    return;
                }
                Some(FieldRow::Header(_)) => {}
            }
        }
    }

    fn fields_key(&mut self, key: KeyEvent) {
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let selected = self.selected_entry();
        match key.code {
            KeyCode::Up if shift => {
                if let Some((side, index)) = selected {
                    self.move_vertical(side, index, false);
                }
            }
            KeyCode::Down if shift => {
                if let Some((side, index)) = selected {
                    self.move_vertical(side, index, true);
                }
            }
            KeyCode::Left if shift => {
                if let Some((side, index)) = selected {
                    self.move_to_panel(side, index, Side::Left);
                }
            }
            KeyCode::Right if shift => {
                if let Some((side, index)) = selected {
                    self.move_to_panel(side, index, Side::Right);
                }
            }
            KeyCode::Up => self.step_entry(false),
            KeyCode::Down => self.step_entry(true),
            KeyCode::Left => self.go_menu(),
            KeyCode::Char(' ') => {
                if let Some((side, index)) = selected {
                    self.toggle_enabled(side, index);
                }
            }
            KeyCode::Enter => {
                if let Some((side, index)) = selected {
                    self.open_entry(side, index);
                }
            }
            KeyCode::Insert => self.open_add(),
            KeyCode::Char('a') if !key.modifiers.contains(KeyModifiers::CONTROL) => self.open_add(),
            KeyCode::Delete | KeyCode::Backspace => {
                if let Some((side, index)) = selected {
                    self.remove_entry(side, index);
                }
            }
            _ => {}
        }
    }

    /// Moves an entry one place up or down; at a panel edge it crosses into the other panel.
    fn move_vertical(&mut self, side: Side, index: usize, down: bool) {
        let len = self.entries(side).len();
        if index >= len {
            return;
        }
        let moved = if !down {
            if index > 0 {
                self.entries_mut(side).swap(index, index - 1);
                Some((side, index - 1))
            } else if side == Side::Right {
                let entry = self.entries_mut(side).remove(index);
                self.entries_mut(Side::Left).push(entry);
                Some((Side::Left, self.entries(Side::Left).len() - 1))
            } else {
                None
            }
        } else if index + 1 < len {
            self.entries_mut(side).swap(index, index + 1);
            Some((side, index + 1))
        } else if side == Side::Left {
            let entry = self.entries_mut(side).remove(index);
            self.entries_mut(Side::Right).insert(0, entry);
            Some((Side::Right, 0))
        } else {
            None
        };
        if let Some((side, index)) = moved {
            self.select_entry(side, index);
        }
    }

    fn move_to_panel(&mut self, from: Side, index: usize, to: Side) {
        if from == to || index >= self.entries(from).len() {
            return;
        }
        let entry = self.entries_mut(from).remove(index);
        let label = entry.label.clone();
        self.entries_mut(to).push(entry);
        let position = self.entries(to).len() - 1;
        self.select_entry(to, position);
        self.set_status(
            format!("Moved {label} to the {}", to.label().to_lowercase()),
            false,
        );
    }

    fn toggle_enabled(&mut self, side: Side, index: usize) {
        let message = self.entries_mut(side).get_mut(index).map(|entry| {
            entry.enabled = !entry.enabled;
            format!(
                "{} {}",
                entry.label,
                if entry.enabled { "shown" } else { "hidden" }
            )
        });
        if let Some(message) = message {
            self.set_status(message, false);
        }
    }

    fn remove_entry(&mut self, side: Side, index: usize) {
        if index >= self.entries(side).len() {
            return;
        }
        let entry = self.entries_mut(side).remove(index);
        let label = entry.label.clone();
        let remaining = self.entries(side).len();
        if remaining > 0 {
            self.select_entry(side, index.min(remaining - 1));
        } else {
            self.select_first_entry();
        }
        self.undo = Some((side, index, entry));
        self.set_status(format!("Removed {label} — press Ctrl+Z to undo"), false);
    }

    /// Restores the last removed entry at its old position. Only one removal is remembered.
    fn undo_removal(&mut self) {
        let Some((side, index, entry)) = self.undo.take() else {
            self.set_status("Nothing to undo", false);
            return;
        };
        let already_shown = self
            .cfg
            .fields
            .left
            .iter()
            .chain(&self.cfg.fields.right)
            .any(|shown| shown.field == entry.field);
        if already_shown {
            self.set_status(
                format!("{} is shown again, nothing restored", entry.label),
                true,
            );
            return;
        }
        let label = entry.label.clone();
        let list = self.entries_mut(side);
        let position = index.min(list.len());
        list.insert(position, entry);
        self.select_entry(side, position);
        self.set_status(format!("Restored {label}"), false);
    }

    fn open_entry(&mut self, side: Side, index: usize) {
        if index >= self.entries(side).len() {
            return;
        }
        self.popup = Some(entry_popup(
            side,
            index,
            ListState::default().with_selected(Some(0)),
            None,
        ));
    }

    fn open_add(&mut self) {
        self.popup = Some(Popup::Add {
            filter: TextInput::new("", ADD_FILTER_MAX),
            state: ListState::default().with_selected(first_add_field("")),
            anchor: self.selected_entry(),
        });
    }

    /// Writes an edited label or icon. Row 0 is the label, row 1 the icon.
    fn entry_commit(&mut self, side: Side, index: usize, row: usize, value: &str) {
        let Some(field) = self.entries(side).get(index).map(|entry| entry.field) else {
            return;
        };
        if let Some(entry) = self.entries_mut(side).get_mut(index) {
            match row {
                0 => {
                    let label = value.trim();
                    entry.label = if label.is_empty() {
                        field.label().to_string()
                    } else {
                        label.chars().take(MAX_LABEL).collect()
                    };
                }
                1 => entry.icon = value.trim().chars().take(MAX_ICON).collect(),
                _ => {}
            }
        }
    }

    fn toggle_bar(&mut self, side: Side, index: usize) {
        let Some(field) = self.entries(side).get(index).map(|entry| entry.field) else {
            return;
        };
        if !field.has_gauge() {
            self.set_status(
                format!("{} has no gauge to show as a bar", field.label()),
                true,
            );
            return;
        }
        if let Some(entry) = self.entries_mut(side).get_mut(index) {
            entry.bar = !entry.bar;
        }
    }

    fn reset_entry(&mut self, side: Side, index: usize) {
        let Some(field) = self.entries(side).get(index).map(|entry| entry.field) else {
            return;
        };
        if let Some(entry) = self.entries_mut(side).get_mut(index) {
            entry.label = field.label().to_string();
            entry.icon = field.icon().to_string();
            entry.bar = false;
        }
        self.set_status(format!("{} reset to defaults", field.label()), false);
    }

    fn entry_key(
        &mut self,
        key: KeyEvent,
        side: Side,
        index: usize,
        mut state: ListState,
        editing: Option<TextInput>,
    ) -> Option<Popup> {
        let row = state.selected().unwrap_or(0);
        if let Some(mut input) = editing {
            return match input.handle(key) {
                InputResult::Submit => {
                    self.entry_commit(side, index, row, input.value());
                    Some(entry_popup(side, index, state, None))
                }
                InputResult::Cancel => Some(entry_popup(side, index, state, None)),
                InputResult::Continue => Some(entry_popup(side, index, state, Some(input))),
            };
        }
        let entry = self.entries(side).get(index).cloned()?;
        let mut editing = None;
        match key.code {
            KeyCode::Esc => return None,
            KeyCode::Up
            | KeyCode::Down
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End => {
                list_nav(&mut state, ENTRY_ROWS, key.code);
            }
            KeyCode::Enter if row == 0 => {
                editing = Some(TextInput::new(&entry.label, MAX_LABEL));
            }
            KeyCode::Enter if row == 1 => {
                editing = Some(TextInput::new(&entry.icon, MAX_ICON));
            }
            KeyCode::Enter | KeyCode::Char(' ') if row == 2 => self.toggle_bar(side, index),
            KeyCode::Enter if row == 3 => self.reset_entry(side, index),
            _ => {}
        }
        Some(entry_popup(side, index, state, editing))
    }

    fn add_key(
        &mut self,
        key: KeyEvent,
        mut filter: TextInput,
        mut state: ListState,
        mut anchor: Option<(Side, usize)>,
    ) -> Option<Popup> {
        let rows = add_rows(filter.value());
        match key.code {
            KeyCode::Esc => return None,
            KeyCode::Enter => {
                if let Some(AddRow::Field(field)) =
                    state.selected().and_then(|i| rows.get(i)).copied()
                {
                    anchor = self.add_field(field, anchor);
                }
            }
            KeyCode::Up
            | KeyCode::Down
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End => step_add_selection(&mut state, &rows, key.code),
            _ => {
                let before = filter.value().to_string();
                filter.handle(key);
                if filter.value() != before {
                    state.select(first_add_field(filter.value()));
                }
            }
        }
        Some(Popup::Add {
            filter,
            state,
            anchor,
        })
    }

    /// Inserts a field after the anchor (or at the end of the left panel) and returns the new anchor.
    fn add_field(&mut self, field: Field, anchor: Option<(Side, usize)>) -> Option<(Side, usize)> {
        let shown = self
            .cfg
            .fields
            .left
            .iter()
            .chain(&self.cfg.fields.right)
            .any(|entry| entry.field == field);
        if shown {
            self.set_status(format!("{} is already shown", field.label()), true);
            return anchor;
        }
        let (side, position) = match anchor {
            Some((side, index)) => (side, (index + 1).min(self.entries(side).len())),
            None => (Side::Left, self.entries(Side::Left).len()),
        };
        self.entries_mut(side)
            .insert(position, FieldEntry::new(field));
        self.select_entry(side, position);
        self.set_status(format!("Added {}", field.label()), false);
        Some((side, position))
    }
}

/// Rows of the entry editor: label, icon, show as bar, reset.
pub(super) const ENTRY_ROWS: usize = 4;

fn entry_popup(side: Side, index: usize, state: ListState, editing: Option<TextInput>) -> Popup {
    Popup::Entry {
        side,
        index,
        state,
        editing,
    }
}

/// Add popup rows: group headers followed by the fields that match the filter.
pub(super) fn add_rows(filter: &str) -> Vec<AddRow> {
    let needle = filter.trim().to_lowercase();
    let mut rows = Vec::new();
    for group in FieldGroup::ALL {
        let fields: Vec<Field> = Field::ALL
            .iter()
            .copied()
            .filter(|field| field.group() == group && matches_filter(*field, &needle))
            .collect();
        if fields.is_empty() {
            continue;
        }
        rows.push(AddRow::Group(group));
        rows.extend(fields.into_iter().map(AddRow::Field));
    }
    rows
}

fn matches_filter(field: Field, needle: &str) -> bool {
    needle.is_empty()
        || field.label().to_lowercase().contains(needle)
        || field.key().contains(needle)
        || field.description().to_lowercase().contains(needle)
}

fn first_add_field(filter: &str) -> Option<usize> {
    add_rows(filter)
        .iter()
        .position(|row| matches!(row, AddRow::Field(_)))
}

fn next_add_field(rows: &[AddRow], from: usize, forward: bool) -> Option<usize> {
    let is_field = |i: usize| matches!(rows.get(i), Some(AddRow::Field(_)));
    if forward {
        (from + 1..rows.len()).find(|&i| is_field(i))
    } else {
        (0..from).rev().find(|&i| is_field(i))
    }
}

/// Moves the Add popup selection over fields only; group headers are skipped.
fn step_add_selection(state: &mut ListState, rows: &[AddRow], code: KeyCode) {
    let mut current = state.selected().unwrap_or(0);
    let (forward, steps) = match code {
        KeyCode::Down => (true, 1),
        KeyCode::Up => (false, 1),
        KeyCode::PageDown => (true, PAGE_STEP),
        KeyCode::PageUp => (false, PAGE_STEP),
        KeyCode::Home => {
            state.select(first_add_field_row(rows));
            return;
        }
        KeyCode::End => {
            state.select(rows.iter().rposition(|row| matches!(row, AddRow::Field(_))));
            return;
        }
        _ => return,
    };
    let mut moved = false;
    for _ in 0..steps {
        match next_add_field(rows, current, forward) {
            Some(next) => {
                current = next;
                moved = true;
            }
            None => break,
        }
    }
    if moved {
        state.select(Some(current));
    }
}

fn first_add_field_row(rows: &[AddRow]) -> Option<usize> {
    rows.iter().position(|row| matches!(row, AddRow::Field(_)))
}

/// Live validation of a text popup's value. `None` means the value can be applied.
fn text_error(target: Setting, value: &str) -> Option<String> {
    match target {
        Setting::Palette => {
            let tokens: Vec<&str> = value
                .split(|ch: char| ch.is_whitespace() || ch == ',')
                .filter(|token| !token.is_empty())
                .collect();
            if tokens.is_empty() {
                return Some("Enter at least one color".to_string());
            }
            if tokens.len() > MAX_PALETTE_COLORS {
                return Some(format!("Use at most {MAX_PALETTE_COLORS} colors"));
            }
            tokens
                .iter()
                .find(|token| Color::from_hex(token).is_none())
                .map(|token| format!("'{token}' is not a #RRGGBB color"))
        }
        Setting::TitleColor | Setting::SeparatorColor | Setting::ValueColor => {
            parse_color(value.trim())
                .is_none()
                .then(|| "Enter a #RRGGBB color".to_string())
        }
        Setting::SavePalette => value
            .trim()
            .is_empty()
            .then(|| "Enter a name for the palette".to_string()),
        Setting::LogoPath => logo_path_error(value.trim()),
        _ => None,
    }
}

fn logo_path_error(value: &str) -> Option<String> {
    if value.is_empty() {
        return Some("Enter the path to a logo file".to_string());
    }
    let path = logo::expand_home(value);
    if !path.is_file() {
        return Some("File not found".to_string());
    }
    match std::fs::read_to_string(&path) {
        Ok(text) if logo::Logo::from_text(&text).is_some() => None,
        Ok(_) => Some("The file contains no text".to_string()),
        Err(_) => Some("The file cannot be read as text".to_string()),
    }
}

fn parse_color(value: &str) -> Option<Color> {
    Color::from_hex(value)
}

/// Whitespace- or comma-separated `#RRGGBB` values, 1 to 16 of them.
pub(super) fn parse_palette(value: &str) -> Option<Vec<Color>> {
    let colors: Option<Vec<Color>> = value
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter(|token| !token.is_empty())
        .map(Color::from_hex)
        .collect();
    colors.filter(|colors| (1..=MAX_PALETTE_COLORS).contains(&colors.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::all_themes;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn new_app() -> App {
        App::new_for_test(Config::default(), SysInfo::sample())
    }

    fn press_with(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        app.handle_event(Event::Key(KeyEvent::new(code, modifiers)));
    }

    fn press(app: &mut App, code: KeyCode) {
        press_with(app, code, KeyModifiers::NONE);
    }

    fn ctrl(app: &mut App, ch: char) {
        press_with(app, KeyCode::Char(ch), KeyModifiers::CONTROL);
    }

    fn type_text(app: &mut App, text: &str) {
        for ch in text.chars() {
            press(app, KeyCode::Char(ch));
        }
    }

    fn enter_section(app: &mut App, section: Section) {
        app.select_section(section);
        app.focus = Focus::Content;
    }

    #[test]
    fn multi_line_paste_becomes_pending_logo_without_moving_selection() {
        let mut app = new_app();
        enter_section(&mut app, Section::Logo);
        let rows_before = app.form_sel;
        app.handle_event(Event::Paste("  __\n /  \\\n/____\\\n".to_string()));
        assert!(app.pending_logo_text().is_some());
        assert_eq!(app.section, Section::Logo);
        assert_eq!(app.form_sel, rows_before);
        assert!(app.is_dirty());
        assert!(matches!(app.cfg.logo.source, LogoSource::File { .. }));
    }

    #[test]
    fn paste_is_never_a_key_command() {
        let mut app = new_app();
        enter_section(&mut app, Section::Appearance);
        app.handle_event(Event::Paste("q\n".to_string()));
        assert_eq!(app.outcome(), None);
        assert!(app.popup.is_none());
    }

    #[test]
    fn shift_down_at_end_of_left_panel_moves_entry_to_start_of_right_panel() {
        let mut app = new_app();
        enter_section(&mut app, Section::Fields);
        let last_left = app.cfg.fields.left.len() - 1;
        let moved = app.cfg.fields.left[last_left].field;
        app.select_entry(Side::Left, last_left);
        press_with(&mut app, KeyCode::Down, KeyModifiers::SHIFT);
        assert_eq!(app.cfg.fields.left.len(), last_left);
        assert_eq!(app.cfg.fields.right[0].field, moved);
        assert_eq!(app.selected_entry(), Some((Side::Right, 0)));
    }

    #[test]
    fn space_toggles_and_delete_then_ctrl_z_restores_same_index() {
        let mut app = new_app();
        enter_section(&mut app, Section::Fields);
        app.select_entry(Side::Left, 0);
        let original = app.cfg.fields.left[0].clone();
        press(&mut app, KeyCode::Char(' '));
        assert!(!app.cfg.fields.left[0].enabled);
        press(&mut app, KeyCode::Delete);
        assert!(app
            .cfg
            .fields
            .left
            .iter()
            .all(|e| e.field != original.field));
        ctrl(&mut app, 'z');
        assert_eq!(app.cfg.fields.left[0].field, original.field);
        assert!(!app.cfg.fields.left[0].enabled);
    }

    #[test]
    fn typing_in_logo_filter_narrows_list_and_applies_builtin_key() {
        let mut app = new_app();
        enter_section(&mut app, Section::Logo);
        app.enter_logo_list();
        type_text(&mut app, "arch");
        let keys = app.filtered_logo_keys();
        assert!(!keys.is_empty());
        assert!(keys.iter().all(|key| key.contains("arch")));
        assert!(keys.len() < app.logo_keys.len());
        assert_eq!(
            app.cfg.logo.source,
            LogoSource::Builtin {
                key: keys[0].clone()
            }
        );
    }

    #[test]
    fn left_and_right_cycle_palettes_on_theme_row() {
        let mut app = new_app();
        enter_section(&mut app, Section::Appearance);
        press(&mut app, KeyCode::Right);
        assert_eq!(app.cfg.colors.palette, all_themes()[1].colors);
        press(&mut app, KeyCode::Left);
        assert_eq!(app.cfg.colors.palette, all_themes()[0].colors);
    }

    #[test]
    fn ctrl_s_saves_and_ctrl_c_discards() {
        let mut app = new_app();
        ctrl(&mut app, 's');
        assert_eq!(app.outcome(), Some(Outcome::Save));

        let mut app = new_app();
        press(&mut app, KeyCode::Right);
        ctrl(&mut app, 'c');
        assert_eq!(app.outcome(), Some(Outcome::Discard));
    }

    #[test]
    fn q_with_unsaved_changes_opens_confirm_and_esc_never_quits() {
        let mut app = new_app();
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.outcome(), None);

        enter_section(&mut app, Section::Appearance);
        press(&mut app, KeyCode::Right);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.focus, Focus::Menu);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.outcome(), None);

        press(&mut app, KeyCode::Char('q'));
        assert!(matches!(app.popup, Some(Popup::Confirm { .. })));
        assert_eq!(app.outcome(), None);
        press(&mut app, KeyCode::Esc);
        assert!(app.popup.is_none());
        assert_eq!(app.outcome(), None);
    }

    #[test]
    fn confirm_popup_discard_quits_without_saving() {
        let mut app = new_app();
        enter_section(&mut app, Section::Appearance);
        press(&mut app, KeyCode::Right);
        app.focus = Focus::Menu;
        press(&mut app, KeyCode::Char('q'));
        press(&mut app, KeyCode::Right);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.outcome(), Some(Outcome::Discard));
    }

    #[test]
    fn add_popup_marks_used_fields_and_inserts_after_selection() {
        let mut app = new_app();
        enter_section(&mut app, Section::Fields);
        app.select_entry(Side::Left, 0);
        let left_before = app.cfg.fields.left.len();
        press(&mut app, KeyCode::Char('a'));
        assert!(matches!(app.popup, Some(Popup::Add { .. })));

        type_text(&mut app, "locale");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.cfg.fields.left.len(), left_before + 1);
        assert_eq!(app.cfg.fields.left[1].field, Field::Locale);

        press(&mut app, KeyCode::Enter);
        assert_eq!(app.cfg.fields.left.len(), left_before + 1);
    }

    #[test]
    fn add_rows_keep_only_matching_fields() {
        let rows = add_rows("locale");
        assert!(rows.contains(&AddRow::Field(Field::Locale)));
        assert!(!rows.contains(&AddRow::Field(Field::Os)));
        assert!(matches!(rows[0], AddRow::Group(_)));
    }

    #[test]
    fn duplicate_palette_name_needs_second_enter_to_overwrite() {
        let mut cfg = Config::default();
        cfg.custom_palettes
            .insert("mine".to_string(), vec![Color::new(1, 2, 3)]);
        let mut app = App::new_for_test(cfg, SysInfo::sample());
        enter_section(&mut app, Section::Appearance);
        app.set_form_row(Section::Appearance, 7);
        press(&mut app, KeyCode::Enter);
        type_text(&mut app, "mine");
        press(&mut app, KeyCode::Enter);
        assert!(matches!(&app.popup, Some(Popup::Text(t)) if t.error.is_some()));
        press(&mut app, KeyCode::Enter);
        assert!(app.popup.is_none());
        assert_eq!(
            app.cfg.custom_palettes.get("mine"),
            Some(&app.cfg.colors.palette)
        );
    }

    #[test]
    fn palette_text_popup_validates_live_and_applies_valid_hex() {
        let mut app = new_app();
        enter_section(&mut app, Section::Appearance);
        app.set_form_row(Section::Appearance, 1);
        press(&mut app, KeyCode::Enter);
        for _ in 0..200 {
            press(&mut app, KeyCode::Backspace);
        }
        type_text(&mut app, "#GG0000");
        assert!(matches!(&app.popup, Some(Popup::Text(t)) if t.error.is_some()));
        for _ in 0..30 {
            press(&mut app, KeyCode::Backspace);
        }
        type_text(&mut app, "#FF0000, #00FF00");
        press(&mut app, KeyCode::Enter);
        assert!(app.popup.is_none());
        assert_eq!(app.cfg.colors.palette.len(), 2);
    }

    #[test]
    fn gap_clamps_and_interval_steps_follow_the_spec() {
        let mut app = new_app();
        enter_section(&mut app, Section::Layout);
        app.set_form_row(Section::Layout, 1);
        for _ in 0..30 {
            press(&mut app, KeyCode::Right);
        }
        assert_eq!(app.cfg.layout.gap, MAX_SPACING);
        press(&mut app, KeyCode::Left);
        assert_eq!(app.cfg.layout.gap, MAX_SPACING - 1);

        assert_eq!(step_interval(1_000, 1), 1_500);
        assert_eq!(step_interval(1_000, -1), 900);
        assert_eq!(step_interval(5_000, 1), 6_000);
        assert_eq!(step_interval(5_500, -1), 5_000);
        assert_eq!(step_interval(100, -1), 100);
        assert_eq!(step_interval(60_000, 1), 60_000);
    }

    #[test]
    fn max_value_width_steps_through_auto() {
        assert_eq!(step_value_width(0, 1), MIN_VALUE_WIDTH);
        assert_eq!(step_value_width(MIN_VALUE_WIDTH, 1), 12);
        assert_eq!(step_value_width(MIN_VALUE_WIDTH, -1), 0);
        assert_eq!(step_value_width(0, -1), 0);
        assert_eq!(step_value_width(MAX_VALUE_WIDTH, 1), MAX_VALUE_WIDTH);
    }

    #[test]
    fn status_expires_after_four_seconds() {
        let mut app = new_app();
        app.set_status("saved", false);
        let at = app
            .status
            .as_ref()
            .map(|s| s.at)
            .unwrap_or_else(Instant::now);
        assert!(!app.tick(at + Duration::from_secs(1)));
        assert!(app.tick(at + Duration::from_secs(5)));
        assert!(app.status.is_none());
    }

    #[test]
    fn help_popup_closes_with_escape_and_menu_moves_sections() {
        let mut app = new_app();
        press(&mut app, KeyCode::F(1));
        assert!(matches!(app.popup, Some(Popup::Help { .. })));
        press(&mut app, KeyCode::Esc);
        assert!(app.popup.is_none());

        press(&mut app, KeyCode::Down);
        assert_eq!(app.section, Section::Logo);
        press(&mut app, KeyCode::Up);
        assert_eq!(app.section, Section::Appearance);
    }

    #[test]
    fn key_release_events_are_ignored() {
        let mut app = new_app();
        let release = KeyEvent {
            code: KeyCode::Right,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        };
        let before = app.cfg.colors.palette.clone();
        app.handle_event(Event::Key(release));
        assert_eq!(app.cfg.colors.palette, before);
        assert!(!app.welcome_visible());
    }

    #[test]
    fn enter_release_does_not_activate_a_row() {
        let mut app = new_app();
        enter_section(&mut app, Section::Appearance);
        let release = KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        };
        app.handle_event(Event::Key(release));
        assert!(app.popup.is_none());
    }

    #[test]
    fn clipboard_logo_row_is_offered_only_on_windows() {
        let app = new_app();
        let labels: Vec<&str> = app
            .section_rows(Section::Logo)
            .into_iter()
            .map(Setting::label)
            .collect();
        assert_eq!(labels.contains(&"Paste logo from clipboard"), cfg!(windows));
    }
}
