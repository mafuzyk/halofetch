#![allow(dead_code)]

use color_eyre::Result;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color as TuiColor, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;
use ratatui::Terminal;
use std::io;
use unicode_width::UnicodeWidthStr;

use crate::ascii;
use crate::component;
use crate::config::{self, Config};
use crate::info;
use crate::layout::AppLayout;
use crate::theme::{self, Color};

use super::state::{Editor, InputMode, Tab};

impl Editor {
    pub(super) fn new(cfg: Config) -> Result<Self> {
        let info = info::collect()?;
        let logo_keys = ascii::available_logos()?;
        let ascii_art = ascii::load(&cfg)?;
        let ascii_is_small = {
            let small_key = format!("{}_small", cfg.logo.key);
            let tw = crate::layout::terminal_width();
            tw < 65 && ascii::has_variant(&small_key)
        };

        // Determine initial ASCII source
        let ascii_source = if cfg.logo.key.is_empty() {
            if cfg.logo.path.to_lowercase() == "disabled" {
                "disabled".into()
            } else {
                format!("file:{}", cfg.logo.path)
            }
        } else {
            format!("builtin:{}", cfg.logo.key)
        };

        let app_layout = AppLayout::Centered;
        let scene_selected = component::Scene::all()
            .iter()
            .position(|scene| *scene == cfg.scene)
            .unwrap_or(0);

        let themes = theme::all_themes();
        let theme_selected = themes
            .iter()
            .position(|t| t.colors == cfg.logo.colors)
            .unwrap_or(0);

        let mut available: Vec<(String, String, String)> = vec![
            ("os", "\u{f17c}", "OS"),
            ("host", "\u{f109}", "Host"),
            ("user", "\u{f007}", "Usr"),
            ("kernel", "\u{e271}", "Krn"),
            ("uptime", "\u{f017}", "Up"),
            ("packages", "\u{f1b3}", "Pkg"),
            ("shell", "\u{f489}", "Sh"),
            ("terminal", "\u{f120}", "Term"),
            ("cpu", "\u{f2db}", "CPU"),
            ("gpu", "\u{f26c}", "GPU"),
            ("memory", "\u{f1c0}", "Mem"),
            ("disk", "\u{f0a0}", "Dsk"),
            ("wm", "\u{f108}", "WM"),
            ("load", "\u{f0e7}", "Load"),
            ("processes", "\u{f013}", "Proc"),
            ("local_ip", "\u{f0c1}", "IP"),
            ("resolution", "\u{f108}", "Res"),
            ("de", "\u{f11b}", "DE"),
            ("font", "\u{f031}", "Font"),
            ("vram", "\u{f26c}", "VRAM"),
            ("flatpak", "\u{f2d8}", "Flatpak"),
            ("snap", "\u{f1b3}", "Snap"),
        ]
        .into_iter()
        .map(|(k, i, l)| (k.to_string(), i.to_string(), l.to_string()))
        .collect();
        let bar_fields = [
            "cpu",
            "gpu",
            "memory",
            "disk",
            "vram",
            "load",
            "cpu_temp",
            "battery_level",
            "brightness",
            "signal",
            "storage",
        ];
        for &base in &bar_fields {
            if let Some((_, icon, label)) = available.iter().find(|(k, _, _)| k == base) {
                available.push((
                    format!("{}_bar", base),
                    icon.clone(),
                    format!("{} [bar]", label),
                ));
            }
        }

        let (tw, th) = terminal::size()?;
        let layout_selected = AppLayout::pc_variants()
            .iter()
            .position(|l| *l == app_layout)
            .unwrap_or(0);
        let ascii_component = component::ascii::AsciiComponent::new(ascii_art.clone());
        let ascii_selected = match ascii_source.split_once(':') {
            Some(("builtin", k)) => logo_keys.iter().position(|lk| lk == k).unwrap_or(0),
            _ if ascii_source.starts_with("file:") => logo_keys.len(),
            _ if ascii_source == "pasted" => logo_keys.len() + 1,
            _ => logo_keys.len() + 2,
        };
        let monitor_mode = cfg.live.enabled;
        let mut ed = Self {
            cfg,
            info,
            tab: Tab::Welcome,
            app_layout,
            layout_selected,
            scene_selected,
            scene_focus: false,
            monitor_mode,
            themes,
            theme_selected,
            custom_palette_input: String::new(),
            logo_keys,
            ascii_art,
            ascii_component,
            ascii_source,
            ascii_selected,
            ascii_search: String::new(),
            ascii_is_small,
            panel_focus: false,
            panel_left_sel: 0,
            panel_right_sel: 0,
            add_panel_available: available,
            add_panel_sel: 0,
            editing_label_input: String::new(),
            file_browser_cwd: std::env::current_dir().unwrap_or_else(|_| "/".into()),
            file_browser_entries: Vec::new(),
            file_browser_sel: 0,
            monitor_comp: component::monitor::MonitorComponent::new(),
            system_comp: component::system::SystemComponent,
            companion_comp: component::companion::CompanionComponent,
            input_mode: InputMode::Normal,
            paste_buffer: String::new(),
            saved: false,
            preview_width: tw.saturating_sub(20) as usize,
            preview_lines: Vec::new(),
            term_width: tw,
            term_height: th,
            dirty: true,
            changed: false,
            status_message: "Ready".into(),
        };
        ed.refresh_file_browser();
        ed.refresh_preview();
        Ok(ed)
    }

    pub(super) fn apply_layout(&mut self, idx: usize) {
        let layouts = AppLayout::pc_variants();
        if idx < layouts.len() {
            let l = layouts[idx];
            self.app_layout = l;
            self.cfg.panel.gap = l.gap();
            self.cfg.panel.left_pad = l.padding();
            self.cfg.panel.right_pad = l.padding();
            self.cfg.panel.max_val_width = l.max_panel_width();
            self.dirty = true;
        }
    }

    pub(super) fn refresh_file_browser(&mut self) {
        let mut entries = Vec::new();
        if let Ok(dir) = std::fs::read_dir(&self.file_browser_cwd) {
            for entry in dir.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                entries.push((name, is_dir));
            }
        }
        entries.sort_by(|a, b| {
            if a.1 != b.1 {
                b.1.cmp(&a.1)
            } else {
                a.0.cmp(&b.0)
            }
        });
        self.file_browser_entries = entries;
    }

    fn ensure_ascii_fits(&mut self) {
        let term_w = crate::layout::terminal_width();
        let art_width = self
            .ascii_art
            .lines()
            .map(|l| l.trim_end().width())
            .max()
            .unwrap_or(0);
        let small_key = format!("{}_small", self.cfg.logo.key);
        let has_small = crate::ascii::has_variant(&small_key);

        // If the art overflows the terminal, try switching to _small
        if art_width > 0
            && term_w.saturating_sub(art_width) < 10
            && has_small
            && !self.ascii_is_small
        {
            if let Ok(art) = crate::ascii::load_variant(&small_key) {
                self.ascii_art = art;
                self.ascii_is_small = true;
            }
        }

        // If the terminal has grown enough, restore the full-sized art
        if self.ascii_is_small {
            if let Ok(art) = crate::ascii::load_variant(&self.cfg.logo.key) {
                let full_w = art.lines().map(|l| l.trim_end().width()).max().unwrap_or(0);
                if term_w.saturating_sub(full_w) >= 10 {
                    self.ascii_art = art;
                    self.ascii_is_small = false;
                }
            }
        }
    }

    pub(super) fn filtered_logo_indices(&self) -> Vec<usize> {
        let q = self.ascii_search.to_lowercase();
        self.logo_keys
            .iter()
            .enumerate()
            .filter(|(_, k)| q.is_empty() || k.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }

    pub(super) fn select_logo_at(&mut self, idx: usize) {
        if idx < self.logo_keys.len() {
            self.ascii_selected = idx;
            let key = &self.logo_keys[idx];
            self.ascii_source = format!("builtin:{}", key);
            self.cfg.logo.key = key.clone();
            if let Ok(art) = ascii::load(&self.cfg) {
                self.ascii_art = art;
            }
            self.dirty = true;
        }
    }

    pub(super) fn jump_to_first_matching_logo(&mut self) {
        let filtered = self.filtered_logo_indices();
        if let Some(&first) = filtered.first() {
            if self.ascii_selected >= self.logo_keys.len()
                || !filtered.contains(&self.ascii_selected)
            {
                self.select_logo_at(first);
            }
        }
    }

    fn prepare_saved_config(&mut self) -> Result<()> {
        if self.ascii_source == "pasted" {
            let config_path = config::config_path()?;
            let parent = config_path
                .parent()
                .ok_or_else(|| color_eyre::eyre::eyre!("config path has no parent"))?;
            std::fs::create_dir_all(parent)?;
            let path = parent.join("custom-ascii.txt");
            let temporary = parent.join("custom-ascii.txt.tmp");
            std::fs::write(&temporary, self.ascii_art.trim_end())?;
            std::fs::rename(&temporary, &path)?;
            self.cfg.logo.key.clear();
            self.cfg.logo.path = path.to_string_lossy().into_owned();
            self.ascii_source = format!("file:{}", path.display());
        }
        Ok(())
    }

    pub(super) fn refresh_preview(&mut self) {
        self.ensure_ascii_fits();
        let tw = self.preview_width.max(20);

        let scene = self.cfg.scene;

        self.ascii_component = component::ascii::AsciiComponent::new(self.ascii_art.clone());

        use component::Component;
        let components: Vec<&dyn Component> = vec![
            &self.ascii_component,
            &self.system_comp,
            &self.monitor_comp,
            &self.companion_comp,
        ];

        let ctx = component::RenderCtx {
            info: &self.info,
            cfg: &self.cfg,
            term_width: tw,
            palette: &self.cfg.logo.colors,
        };

        let output = if self.monitor_mode {
            component::render_monitor_split(&components, &ctx)
        } else {
            component::render_scene(scene, &components, &ctx)
        };

        let mut lines: Vec<Line> = Vec::new();

        for output_line in &output.lines {
            let spans: Vec<Span> = output_line
                .iter()
                .map(|s| {
                    let mut style = Style::default();
                    if let Some(fg) = &s.fg {
                        style = style.fg(TuiColor::Rgb(fg.r, fg.g, fg.b));
                    }
                    if let Some(bg) = &s.bg {
                        style = style.bg(TuiColor::Rgb(bg.r, bg.g, bg.b));
                    }
                    if s.bold {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    Span::styled(s.text.clone(), style)
                })
                .collect();
            if !spans.is_empty() {
                lines.push(Line::from(spans));
            }
        }

        self.preview_lines = lines;
        self.dirty = false;
    }
}

fn tui_color(c: &Color) -> TuiColor {
    TuiColor::Rgb(c.r, c.g, c.b)
}

// ── Main UI ──────────────────────────────────────────────────────────────

fn render_editor(frame: &mut Frame, editor: &mut Editor) {
    let area = frame.area();
    if area.width < 52 || area.height < 16 {
        frame.render_widget(
            Paragraph::new(Text::from(vec![
                Line::from(Span::styled(
                    "AtlasFetch Setup",
                    Style::default()
                        .fg(TuiColor::Rgb(157, 133, 255))
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from("Terminal too small."),
                Line::from("Resize to at least 52 × 16."),
                Line::from("Press q to exit."),
            ]))
            .alignment(ratatui::layout::Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            ),
            area,
        );
        return;
    }

    let shell = ratatui::layout::Layout::vertical([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .split(area);
    render_header(frame, shell[0], editor);

    let (content, preview) = if shell[1].width >= 100 {
        let columns = ratatui::layout::Layout::horizontal([
            Constraint::Percentage(45),
            Constraint::Percentage(55),
        ])
        .split(shell[1]);
        (columns[0], columns[1])
    } else {
        let rows = ratatui::layout::Layout::vertical([
            Constraint::Percentage(48),
            Constraint::Percentage(52),
        ])
        .split(shell[1]);
        (rows[0], rows[1])
    };

    let preview_width = preview.width.saturating_sub(2).max(20) as usize;
    if preview_width != editor.preview_width {
        editor.preview_width = preview_width;
        editor.dirty = true;
    }

    render_sidebar(frame, content, editor);
    render_preview_panel(frame, preview, editor);
    render_status_bar(frame, shell[2], editor);

    if !matches!(
        editor.input_mode,
        InputMode::Normal | InputMode::SearchingAscii
    ) {
        render_overlay(frame, area, editor);
    }
}

fn render_header(frame: &mut Frame, area: Rect, editor: &Editor) {
    let (state, state_color) = if editor.changed {
        ("● UNSAVED", TuiColor::Rgb(255, 184, 108))
    } else {
        ("✓ SAVED", TuiColor::Rgb(80, 200, 120))
    };
    let title = Line::from(vec![
        Span::styled(
            " ◢ ATLASFETCH ◣ ",
            Style::default()
                .fg(TuiColor::Rgb(157, 133, 255))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "CONFIGURATOR",
            Style::default().fg(TuiColor::Rgb(180, 180, 200)),
        ),
        Span::raw("  "),
        Span::styled(
            state,
            Style::default()
                .fg(state_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(title).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(TuiColor::Rgb(80, 80, 110))),
        ),
        area,
    );
}

fn render_status_bar(frame: &mut Frame, area: Rect, editor: &Editor) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} ", editor.status_message),
                Style::default().fg(TuiColor::Rgb(180, 180, 200)),
            ),
            Span::raw("  "),
            Span::styled("Ctrl+S", Style::default().fg(TuiColor::Rgb(133, 188, 255))),
            Span::raw(" save  "),
            Span::styled("?", Style::default().fg(TuiColor::Rgb(133, 188, 255))),
            Span::raw(" help  "),
            Span::styled("q", Style::default().fg(TuiColor::Rgb(255, 102, 146))),
            Span::raw(" quit"),
        ])),
        area,
    );
}

fn render_overlay(frame: &mut Frame, area: Rect, editor: &Editor) {
    let popup = centered_rect(72, 70, area);
    frame.render_widget(Clear, popup);

    if editor.input_mode == InputMode::AddingPanel {
        let items: Vec<ListItem> = editor
            .add_panel_available
            .iter()
            .enumerate()
            .map(|(index, (key, _, label))| {
                let prefix = if index == editor.add_panel_sel {
                    "▸ "
                } else {
                    "  "
                };
                ListItem::new(format!("{prefix}{label} ({key})"))
            })
            .collect();
        frame.render_widget(
            List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title(" Add field · ↑↓ select · Enter add · Esc cancel "),
                )
                .highlight_style(Style::default().bg(TuiColor::Rgb(60, 60, 80))),
            popup,
        );
        return;
    }

    if editor.input_mode == InputMode::BrowsingFile {
        let items: Vec<ListItem> = editor
            .file_browser_entries
            .iter()
            .enumerate()
            .map(|(index, (name, is_dir))| {
                let cursor = if index == editor.file_browser_sel {
                    "▸"
                } else {
                    " "
                };
                let kind = if *is_dir { "DIR " } else { "FILE" };
                ListItem::new(format!("{cursor} [{kind}] {name}"))
            })
            .collect();
        frame.render_widget(
            List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title(format!(
                            " {} · Backspace parent · ~ home ",
                            editor.file_browser_cwd.display()
                        )),
                )
                .highlight_style(Style::default().bg(TuiColor::Rgb(60, 60, 80))),
            popup,
        );
        return;
    }

    let (title, text) = match editor.input_mode {
        InputMode::EditingCustomPalette => {
            let parsed: Vec<Color> = editor
                .custom_palette_input
                .split_whitespace()
                .filter_map(Color::from_hex_opt)
                .collect();
            let mut swatches = vec![Span::raw("Preview  ")];
            for color in &parsed {
                swatches.push(Span::styled("  ", Style::default().bg(tui_color(color))));
                swatches.push(Span::raw(" "));
            }
            (
                " Custom palette ",
                Text::from(vec![
                    Line::from(swatches),
                    Line::from(""),
                    Line::from(editor.custom_palette_input.clone()),
                    Line::from(""),
                    Line::from("Type space-separated #RRGGBB colors."),
                    Line::from("Enter apply · Esc cancel"),
                ]),
            )
        }
        InputMode::EditingLabel => (
            " Rename field ",
            Text::from(vec![
                Line::from(format!("Label: {}", editor.editing_label_input)),
                Line::from(""),
                Line::from("Enter apply · Esc cancel"),
            ]),
        ),
        InputMode::PastingAscii => (
            " Paste ASCII ",
            Text::from(vec![
                Line::from(format!("Buffer: {} characters", editor.paste_buffer.len())),
                Line::from(""),
                Line::from(editor.paste_buffer.clone()),
                Line::from(""),
                Line::from("Enter apply · Esc cancel"),
            ]),
        ),
        InputMode::Help => (
            " Keyboard guide ",
            Text::from(vec![
                Line::from(Span::styled(
                    "GLOBAL",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from("Tab / Shift+Tab   next / previous section"),
                Line::from("↑ ↓                 move through options"),
                Line::from("← →                 change focused column"),
                Line::from("Ctrl+S              save and exit"),
                Line::from("q / Esc             quit safely"),
                Line::from(""),
                Line::from(Span::styled(
                    "CONTEXT",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from("Panels  Space toggle · a add · d delete · r move · e rename"),
                Line::from("ASCII   / search · c file · p paste · d disable"),
                Line::from("Theme   c custom palette · v color direction"),
                Line::from("Mode    m live monitor"),
                Line::from(""),
                Line::from("Press ? or Esc to close."),
            ]),
        ),
        InputMode::ConfirmQuit => (
            " Unsaved changes ",
            Text::from(vec![
                Line::from(Span::styled(
                    "Discard your changes?",
                    Style::default()
                        .fg(TuiColor::Rgb(255, 184, 108))
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from("y       discard and exit"),
                Line::from("s       save and exit"),
                Line::from("n / Esc continue editing"),
            ]),
        ),
        InputMode::Normal
        | InputMode::SearchingAscii
        | InputMode::AddingPanel
        | InputMode::BrowsingFile => {
            return;
        }
    };

    frame.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(TuiColor::Rgb(157, 133, 255))),
        ),
        popup,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let rows = ratatui::layout::Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    ratatui::layout::Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(rows[1])[1]
}

// ── Sidebar ──────────────────────────────────────────────────────────────

fn render_sidebar(frame: &mut Frame, area: Rect, editor: &Editor) {
    let v = ratatui::layout::Layout::vertical([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(3),
    ]);
    let c = v.split(area);
    render_tabs(frame, c[0], editor);
    render_tab_content(frame, c[1], editor);
    render_hints(frame, c[2], editor);
}

fn render_tabs(frame: &mut Frame, area: Rect, editor: &Editor) {
    let tabs: Vec<Span> = Tab::all()
        .iter()
        .map(|t| {
            let active = *t == editor.tab;
            Span::styled(
                t.label(),
                if active {
                    Style::default()
                        .fg(TuiColor::Rgb(157, 133, 255))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TuiColor::Rgb(120, 120, 120))
                },
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(Line::from(tabs)), area);
}

fn render_hints(frame: &mut Frame, area: Rect, editor: &Editor) {
    let text = match editor.tab {
        Tab::Welcome => " Tab/Enter begin",
        Tab::Theme => " ↑↓ theme  c custom palette  v color direction",
        Tab::Mode => " ↑↓ scene/layout  ←→ focus  m live monitor",
        Tab::Ascii => " ↑↓ logo  / search  d disable  c file  p paste",
        Tab::Panels => " ↑↓ navigate  Space toggle  / panel  a add  d delete  r move  e rename",
        Tab::Save => " s save & exit  q exit safely",
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text,
            Style::default().fg(TuiColor::Rgb(140, 140, 140)),
        ))),
        area,
    );
}

fn render_tab_content(frame: &mut Frame, area: Rect, editor: &Editor) {
    match editor.tab {
        Tab::Welcome => render_welcome_tab(frame, area, editor),
        Tab::Theme => render_theme_tab(frame, area, editor),
        Tab::Mode => render_mode_tab(frame, area, editor),
        Tab::Ascii => render_ascii_tab(frame, area, editor),
        Tab::Panels => render_panels_tab(frame, area, editor),
        Tab::Save => render_save_tab(frame, area, editor),
    }
}

// ── Welcome tab ──────────────────────────────────────────────────────────

fn render_welcome_tab(frame: &mut Frame, area: Rect, _editor: &Editor) {
    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(Span::styled(
                "  Welcome to atlasfetch setup!",
                Style::default()
                    .fg(TuiColor::Rgb(133, 188, 255))
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("  Use Tab to navigate through the configuration tabs:"),
            Line::from(""),
            Line::from("  1. Theme  — pick a color theme for your fetch"),
            Line::from("  2. Mode   — choose scene, layout, or Monitor mode"),
            Line::from("  3. Panels — add, remove, and reorder info fields"),
            Line::from("  4. ASCII  — select or disable ASCII art logos"),
            Line::from("  5. Save   — save your configuration"),
            Line::from(""),
            Line::from(Span::styled(
                "  Press Tab or Enter to start configuring!",
                Style::default().fg(TuiColor::Rgb(157, 133, 255)),
            )),
        ]))
        .block(
            Block::default()
                .title("Welcome")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(TuiColor::Rgb(133, 188, 255))),
        ),
        area,
    );
}

// ── Mode tab ─────────────────────────────────────────────────────────────

fn render_mode_tab(frame: &mut Frame, area: Rect, editor: &Editor) {
    let rows =
        ratatui::layout::Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).split(area);

    // Monitor toggle
    let monitor_label = if editor.monitor_mode {
        "▶ Monitor Mode"
    } else {
        "  Monitor Mode"
    };
    let monitor_style = if editor.monitor_mode {
        Style::default()
            .fg(TuiColor::Rgb(255, 102, 146))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TuiColor::Rgb(140, 140, 140))
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(monitor_label, monitor_style),
            Span::styled(
                " — Start AtlasFetch as monitor + shell",
                Style::default().fg(TuiColor::Rgb(120, 120, 120)),
            ),
        ])),
        rows[0],
    );

    if editor.monitor_mode {
        // In monitor mode, just show info
        frame.render_widget(
            Paragraph::new(Text::from(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  Monitor Mode active",
                    Style::default()
                        .fg(TuiColor::Rgb(255, 102, 146))
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from("  Press m to toggle off"),
                Line::from(""),
                Line::from("  After saving, atlasfetch starts with your live"),
                Line::from("  scene above a real interactive shell (PTY)."),
                Line::from("  Use Ctrl+Q to close it; atlasfetch fetch stays static."),
            ])),
            rows[1],
        );
        return;
    }

    let halves = ratatui::layout::Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(rows[1]);

    let scenes = component::Scene::all();
    let scene_items: Vec<ListItem> = scenes
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let sel = !editor.scene_focus && i == editor.scene_selected;
            ListItem::new(Line::from(vec![
                Span::styled(
                    if sel { "▸ " } else { "  " },
                    Style::default().fg(TuiColor::Rgb(133, 188, 255)),
                ),
                Span::styled(
                    s.name(),
                    if sel {
                        Style::default()
                            .fg(TuiColor::Rgb(255, 255, 255))
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(TuiColor::Rgb(200, 200, 200))
                    },
                ),
                Span::raw("  "),
                Span::styled(
                    s.description(),
                    Style::default().fg(TuiColor::Rgb(120, 120, 120)),
                ),
            ]))
        })
        .collect();
    let scene_block = Block::default()
        .title(" Scene ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if editor.scene_focus {
            TuiColor::Rgb(100, 100, 100)
        } else {
            TuiColor::Rgb(157, 133, 255)
        }));
    frame.render_widget(List::new(scene_items).block(scene_block), halves[0]);

    let layouts = AppLayout::pc_variants();
    let layout_items: Vec<ListItem> = layouts
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let sel = editor.scene_focus && i == editor.layout_selected;
            ListItem::new(Line::from(vec![
                Span::styled(
                    if sel { "▶ " } else { "  " },
                    Style::default().fg(TuiColor::Rgb(255, 184, 131)),
                ),
                Span::styled(
                    l.name(),
                    if sel {
                        Style::default()
                            .fg(TuiColor::Rgb(255, 255, 255))
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(TuiColor::Rgb(200, 200, 200))
                    },
                ),
                Span::raw("  "),
                Span::styled(
                    l.description(),
                    Style::default().fg(TuiColor::Rgb(120, 120, 120)),
                ),
            ]))
        })
        .collect();
    let layout_block = Block::default()
        .title(" Layout ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if editor.scene_focus {
            TuiColor::Rgb(157, 133, 255)
        } else {
            TuiColor::Rgb(100, 100, 100)
        }));
    frame.render_widget(List::new(layout_items).block(layout_block), halves[1]);
}

// ── Theme tab ────────────────────────────────────────────────────────────

fn render_theme_tab(frame: &mut Frame, area: Rect, editor: &Editor) {
    let top = ratatui::layout::Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(4),
        Constraint::Length(3),
    ])
    .split(area);

    let items: Vec<ListItem> = editor
        .themes
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let swatch: Vec<Span> = t
                .colors
                .iter()
                .map(|c| Span::styled("  ", Style::default().bg(tui_color(c))))
                .collect();
            let mut line = vec![
                Span::styled(
                    if i == editor.theme_selected {
                        "▸ "
                    } else {
                        "  "
                    },
                    Style::default().fg(TuiColor::Rgb(133, 188, 255)),
                ),
                Span::raw(format!("{:15}", t.name)),
            ];
            line.extend(swatch);
            ListItem::new(Line::from(line))
        })
        .collect();
    let mut state =
        ratatui::widgets::ListState::default().with_selected(Some(editor.theme_selected));
    frame.render_stateful_widget(
        List::new(items).block(
            Block::default()
                .title("Theme Presets")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(TuiColor::Rgb(255, 154, 152))),
        ),
        top[0],
        &mut state,
    );

    let swatch: Vec<Span> = editor
        .cfg
        .logo
        .colors
        .iter()
        .map(|c| Span::styled("  ", Style::default().bg(tui_color(c))))
        .collect();
    frame.render_widget(
        Paragraph::new(Line::from(swatch)).block(
            Block::default()
                .title("Current Palette")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(TuiColor::Rgb(255, 154, 152))),
        ),
        top[1],
    );

    let dir_label = if editor.cfg.logo.color_dir == "vertical" {
        "Vertical"
    } else {
        "Horizontal"
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" v toggle color direction — currently: {}", dir_label),
            Style::default().fg(TuiColor::Rgb(140, 140, 140)),
        )))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        ),
        top[2],
    );
}

// ── ASCII tab ────────────────────────────────────────────────────────────

fn render_ascii_tab(frame: &mut Frame, area: Rect, editor: &Editor) {
    let q = editor.ascii_search.to_lowercase();
    let n = editor.logo_keys.len();

    let items: Vec<ListItem> = editor
        .logo_keys
        .iter()
        .enumerate()
        .filter(|(_, key)| q.is_empty() || key.to_lowercase().contains(&q))
        .map(|(i, key)| {
            let sel = i == editor.ascii_selected;
            ListItem::new(Line::from(vec![
                Span::styled(
                    if sel { "▶ " } else { "  " },
                    Style::default().fg(if sel {
                        TuiColor::Rgb(255, 184, 131)
                    } else {
                        TuiColor::Rgb(60, 60, 70)
                    }),
                ),
                Span::styled(
                    key.clone(),
                    if sel {
                        Style::default()
                            .fg(TuiColor::Rgb(255, 255, 255))
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(TuiColor::Rgb(200, 200, 200))
                    },
                ),
            ]))
        })
        .collect();
    let mut items = items;
    // Special entries always at the bottom
    let has_file = editor.ascii_selected == n;
    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            if has_file { "▶ " } else { "  " },
            Style::default().fg(if has_file {
                TuiColor::Rgb(255, 184, 131)
            } else {
                TuiColor::Rgb(60, 60, 70)
            }),
        ),
        Span::styled(
            "[ Custom file ]",
            if has_file {
                Style::default()
                    .fg(TuiColor::Rgb(255, 255, 255))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TuiColor::Rgb(200, 200, 200))
            },
        ),
    ])));
    let is_pasted = editor.ascii_selected == n + 1;
    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            if is_pasted { "▶ " } else { "  " },
            Style::default().fg(if is_pasted {
                TuiColor::Rgb(255, 184, 131)
            } else {
                TuiColor::Rgb(60, 60, 70)
            }),
        ),
        Span::styled(
            "[ Paste ASCII ]",
            if is_pasted {
                Style::default()
                    .fg(TuiColor::Rgb(255, 255, 255))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TuiColor::Rgb(200, 200, 200))
            },
        ),
    ])));
    let disabled = editor.ascii_selected == n + 2;
    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            if disabled { "▶ " } else { "  " },
            Style::default().fg(if disabled {
                TuiColor::Rgb(255, 102, 146)
            } else {
                TuiColor::Rgb(60, 60, 70)
            }),
        ),
        Span::styled(
            "[ Disabled ]",
            if disabled {
                Style::default()
                    .fg(TuiColor::Rgb(255, 102, 146))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TuiColor::Rgb(200, 200, 200))
            },
        ),
    ])));

    let search_hint = if editor.input_mode == InputMode::SearchingAscii {
        if editor.ascii_search.is_empty() {
            " Search: type to filter (Esc to cancel)".to_string()
        } else {
            format!(" Search: {}", editor.ascii_search)
        }
    } else {
        " / to search".to_string()
    };

    let list_block = Block::default()
        .title("ASCII Art")
        .title_bottom(search_hint)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(TuiColor::Rgb(255, 184, 131)));
    let mut list_state =
        ratatui::widgets::ListState::default().with_selected(Some(editor.ascii_selected));
    frame.render_stateful_widget(List::new(items).block(list_block), area, &mut list_state);
}

// ── Panels tab ───────────────────────────────────────────────────────────

fn render_panels_tab(frame: &mut Frame, area: Rect, editor: &Editor) {
    let c = ratatui::layout::Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(area);

    // Left panel
    let left_items: Vec<ListItem> = editor
        .cfg
        .display
        .left
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let check = if f.enabled { "✓" } else { " " };
            let sel = !editor.panel_focus && editor.panel_left_sel == i;
            ListItem::new(Line::from(vec![
                Span::styled(
                    if sel { "▸ " } else { "  " },
                    Style::default().fg(TuiColor::Rgb(133, 188, 255)),
                ),
                Span::raw(format!(" [{}] {} {}", check, f.icon, f.label)),
            ]))
        })
        .collect();
    frame.render_widget(
        List::new(left_items).block(
            Block::default()
                .title(if !editor.panel_focus {
                    "Left [focused]"
                } else {
                    "Left"
                })
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(if !editor.panel_focus {
                    Style::default().fg(TuiColor::Rgb(133, 188, 255))
                } else {
                    Style::default().fg(TuiColor::Rgb(80, 80, 80))
                }),
        ),
        c[0],
    );

    // Right panel
    let right_items: Vec<ListItem> = editor
        .cfg
        .display
        .right
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let check = if f.enabled { "✓" } else { " " };
            let sel = editor.panel_focus && editor.panel_right_sel == i;
            ListItem::new(Line::from(vec![
                Span::styled(
                    if sel { "▸ " } else { "  " },
                    Style::default().fg(TuiColor::Rgb(133, 188, 255)),
                ),
                Span::raw(format!(" [{}] {} {}", check, f.icon, f.label)),
            ]))
        })
        .collect();
    frame.render_widget(
        List::new(right_items).block(
            Block::default()
                .title(if editor.panel_focus {
                    "Right [focused]"
                } else {
                    "Right"
                })
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(if editor.panel_focus {
                    Style::default().fg(TuiColor::Rgb(133, 188, 255))
                } else {
                    Style::default().fg(TuiColor::Rgb(80, 80, 80))
                }),
        ),
        c[1],
    );
}

// ── Save tab ─────────────────────────────────────────────────────────────

fn render_save_tab(frame: &mut Frame, area: Rect, editor: &Editor) {
    let theme_name = editor
        .themes
        .iter()
        .find(|t| t.colors == editor.cfg.logo.colors)
        .map(|t| t.name)
        .unwrap_or("custom");

    let n_enabled = editor
        .cfg
        .display
        .left
        .iter()
        .chain(editor.cfg.display.right.iter())
        .filter(|f| f.enabled)
        .count();
    let ascii_info = match editor.ascii_source.split_once(':') {
        Some(("builtin", k)) => format!("Built-in: {}", k),
        Some(("file", p)) => format!("File: {}", p),
        Some(("pasted", _)) => "Pasted ASCII".into(),
        _ => "Disabled".into(),
    };
    let config_location = config::config_path()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "active config path".into());

    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Configuration Summary",
                Style::default()
                    .fg(TuiColor::Rgb(133, 188, 255))
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(format!("  Layout:    {}", editor.app_layout.name())),
            Line::from(format!("  Theme:     {}", theme_name)),
            Line::from(format!("  ASCII:     {}", ascii_info)),
            Line::from(format!("  Fields:    {} enabled", n_enabled)),
            Line::from(format!("  Config:    {config_location}")),
            Line::from(""),
            Line::from(Span::styled(
                "  s — Save & Exit",
                Style::default().fg(TuiColor::Rgb(133, 188, 255)),
            )),
            Line::from(Span::styled(
                "  q     — Discard & Exit",
                Style::default().fg(TuiColor::Rgb(255, 102, 146)),
            )),
        ]))
        .block(
            Block::default()
                .title("Save")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(TuiColor::Rgb(133, 188, 255))),
        ),
        area,
    );
}

// ── Preview ──────────────────────────────────────────────────────────────

fn render_preview_panel(frame: &mut Frame, area: Rect, editor: &Editor) {
    let title = if editor.monitor_mode {
        " Preview — Monitor Mode  LIVE ".to_string()
    } else {
        let scenes = component::Scene::all();
        let scene_name = scenes
            .get(editor.scene_selected)
            .map(|s| s.name())
            .unwrap_or("Classic");
        let layout_name = editor.app_layout.name();
        format!(" Preview — {} / {} ", scene_name, layout_name)
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if editor.monitor_mode {
            Style::default().fg(TuiColor::Rgb(255, 102, 146))
        } else {
            Style::default().fg(TuiColor::Rgb(133, 188, 255))
        });
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(Text::from(editor.preview_lines.clone()))
            .style(Style::default().fg(TuiColor::Rgb(200, 200, 200))),
        inner,
    );
}

// ── Event handling ───────────────────────────────────────────────────────

// ── Main entry ───────────────────────────────────────────────────────────

struct TerminalSession;

impl TerminalSession {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen, cursor::Hide) {
            let _ = terminal::disable_raw_mode();
            return Err(error.into());
        }
        Ok(Self)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, cursor::Show);
    }
}

pub fn run(cfg: &mut Config) -> Result<()> {
    let _session = TerminalSession::enter()?;
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut editor = Editor::new(cfg.clone())?;

    let res = loop {
        terminal.draw(|frame| render_editor(frame, &mut editor))?;

        // Auto-refresh every 1.5s when in monitor mode for live data
        if editor.monitor_mode {
            if crossterm::event::poll(std::time::Duration::from_millis(1500))? {
                if !super::events::handle_event(&mut editor)? {
                    break Ok(());
                }
            } else {
                editor.dirty = true;
                editor.refresh_preview();
            }
        } else if !super::events::handle_event(&mut editor)? {
            break Ok(());
        }
    };

    // Copy changes back if saved
    if editor.saved {
        editor.prepare_saved_config()?;
        cfg.logo = editor.cfg.logo.clone();
        cfg.panel = editor.cfg.panel.clone();
        cfg.display = editor.cfg.display.clone();
        cfg.scene = editor.cfg.scene;
        cfg.title = editor.cfg.title.clone();
        cfg.separator = editor.cfg.separator.clone();
        cfg.palette = editor.cfg.palette.clone();
        cfg.live = editor.cfg.live.clone();
    }

    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn rendered_text(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn full_layout_exposes_global_actions_and_preview() {
        let mut editor = Editor::new(Config::default()).expect("editor fixture");
        editor.changed = true;
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).expect("test terminal");

        terminal
            .draw(|frame| render_editor(frame, &mut editor))
            .expect("render");
        let text = rendered_text(&terminal);

        assert!(text.contains("ATLASFETCH"));
        assert!(text.contains("UNSAVED"));
        assert!(text.contains("Ctrl+S"));
        assert!(text.contains("Preview"));
    }

    #[test]
    fn narrow_terminal_gets_an_actionable_fallback() {
        let mut editor = Editor::new(Config::default()).expect("editor fixture");
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).expect("test terminal");

        terminal
            .draw(|frame| render_editor(frame, &mut editor))
            .expect("render");
        let text = rendered_text(&terminal);

        assert!(text.contains("Terminal too small"));
        assert!(text.contains("52 × 16"));
    }

    #[test]
    fn quit_confirmation_is_rendered_over_the_editor() {
        let mut editor = Editor::new(Config::default()).expect("editor fixture");
        editor.changed = true;
        editor.input_mode = InputMode::ConfirmQuit;
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal");

        terminal
            .draw(|frame| render_editor(frame, &mut editor))
            .expect("render");
        let text = rendered_text(&terminal);

        assert!(text.contains("Unsaved changes"));
        assert!(text.contains("save and exit"));
        assert!(text.contains("continue editing"));
    }
}
