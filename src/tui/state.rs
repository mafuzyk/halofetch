use ratatui::text::Line;

use crate::component;
use crate::config::Config;
use crate::info::SysInfo;
use crate::layout::AppLayout;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Tab {
    Welcome,
    Theme,
    Mode,
    Panels,
    Ascii,
    Save,
}

impl Tab {
    pub(super) fn all() -> [Tab; 6] {
        [
            Tab::Welcome,
            Tab::Theme,
            Tab::Mode,
            Tab::Panels,
            Tab::Ascii,
            Tab::Save,
        ]
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Tab::Welcome => " Welcome ",
            Tab::Theme => " Theme ",
            Tab::Mode => " Mode ",
            Tab::Panels => " Panels ",
            Tab::Ascii => " ASCII ",
            Tab::Save => " Save ",
        }
    }

    pub(super) fn next(self) -> Self {
        match self {
            Tab::Welcome => Tab::Theme,
            Tab::Theme => Tab::Mode,
            Tab::Mode => Tab::Panels,
            Tab::Panels => Tab::Ascii,
            Tab::Ascii => Tab::Save,
            Tab::Save => Tab::Welcome,
        }
    }

    pub(super) fn prev(self) -> Self {
        match self {
            Tab::Welcome => Tab::Save,
            Tab::Save => Tab::Ascii,
            Tab::Ascii => Tab::Panels,
            Tab::Panels => Tab::Mode,
            Tab::Mode => Tab::Theme,
            Tab::Theme => Tab::Welcome,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum InputMode {
    Normal,
    EditingCustomPalette,
    EditingLabel,
    AddingPanel,
    PastingAscii,
    BrowsingFile,
    SearchingAscii,
    Help,
    ConfirmQuit,
}

pub(super) struct Editor {
    pub(super) cfg: Config,
    pub(super) info: SysInfo,
    pub(super) tab: Tab,
    pub(super) app_layout: AppLayout,
    pub(super) monitor_mode: bool,
    pub(super) scene_focus: bool,
    pub(super) themes: Vec<theme::Theme>,
    pub(super) theme_selected: usize,
    pub(super) custom_palette_input: String,
    pub(super) logo_keys: Vec<String>,
    pub(super) ascii_art: String,
    pub(super) ascii_component: component::ascii::AsciiComponent,
    pub(super) ascii_source: String,
    pub(super) ascii_is_small: bool,
    pub(super) panel_focus: bool,
    pub(super) panel_left_sel: usize,
    pub(super) panel_right_sel: usize,
    pub(super) add_panel_available: Vec<(String, String, String)>,
    pub(super) add_panel_sel: usize,
    pub(super) editing_label_input: String,
    pub(super) file_browser_cwd: std::path::PathBuf,
    pub(super) file_browser_entries: Vec<(String, bool)>,
    pub(super) file_browser_sel: usize,
    pub(super) scene_selected: usize,
    pub(super) layout_selected: usize,
    pub(super) ascii_selected: usize,
    pub(super) ascii_search: String,
    pub(super) monitor_comp: component::monitor::MonitorComponent,
    pub(super) system_comp: component::system::SystemComponent,
    pub(super) companion_comp: component::companion::CompanionComponent,
    pub(super) input_mode: InputMode,
    pub(super) paste_buffer: String,
    pub(super) saved: bool,
    pub(super) preview_width: usize,
    pub(super) preview_lines: Vec<Line<'static>>,
    pub(super) term_width: u16,
    pub(super) term_height: u16,
    pub(super) dirty: bool,
    pub(super) changed: bool,
    pub(super) status_message: String,
}

#[cfg(test)]
mod tests {
    use super::Tab;

    #[test]
    fn tab_navigation_wraps_in_both_directions() {
        assert_eq!(Tab::Save.next(), Tab::Welcome);
        assert_eq!(Tab::Welcome.prev(), Tab::Save);
        for tab in Tab::all() {
            assert_eq!(tab.next().prev(), tab);
        }
    }
}
