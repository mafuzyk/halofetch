//! Drawing the editor. The view reads [`App`] and only updates list scroll state.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::render::scene::{self, RenderCtx};
use crate::render::{self, text_width, truncate_text};
use crate::theme::Color as ThemeColor;

use super::app::{
    add_rows, parse_palette, AddRow, App, ConfirmChoice, FieldRow, Focus, Popup, Section, Setting,
    Side,
};
use super::input::TextInput;

const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 18;
const WIDE_WIDTH: u16 = 110;
const MENU_WIDTH: u16 = 20;
const LABEL_COLUMNS: usize = 28;
const FIELD_LABEL_COLUMNS: usize = 16;
const MAX_SWATCHES: usize = 8;

/// Colors of the editor chrome. Every other module takes its styles from here.
struct Palette {
    accent: Style,
    muted: Style,
    warn: Style,
    ok: Style,
    error: Style,
}

fn palette() -> Palette {
    Palette {
        accent: Style::default().fg(Color::Rgb(0x9D, 0x85, 0xFF)),
        muted: Style::default().fg(Color::Rgb(0x80, 0x80, 0x80)),
        warn: Style::default().fg(Color::Rgb(0xFF, 0xB8, 0x6C)),
        ok: Style::default().fg(Color::Rgb(0x50, 0xFA, 0x7B)),
        error: Style::default().fg(Color::Rgb(0xFF, 0x55, 0x55)),
    }
}

fn swatch_color(color: ThemeColor) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}

fn border_style(focused: bool) -> Style {
    if focused {
        palette().accent
    } else {
        palette().muted
    }
}

/// Highlight of the selected row: reversed when the list has focus, bold otherwise.
fn selection_style(focused: bool) -> Style {
    if focused {
        Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    }
}

/// Text padded or cut to exactly `width` columns.
fn fit(text: &str, width: usize) -> String {
    let cut = truncate_text(text, width);
    let pad = width.saturating_sub(text_width(&cut));
    format!("{cut}{}", " ".repeat(pad))
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

/// The part of `value` that fits in `avail` columns around the cursor, and the cursor column inside it.
fn input_window(value: &str, cursor_column: usize, avail: usize) -> (String, usize) {
    let chars: Vec<char> = value.chars().collect();
    let mut start = 0;
    let mut hidden = 0;
    while cursor_column.saturating_sub(hidden) >= avail && start < chars.len() {
        hidden += text_width(&chars[start].to_string());
        start += 1;
    }
    let mut visible = String::new();
    let mut used = 0;
    for ch in &chars[start..] {
        let w = text_width(&ch.to_string());
        if used + w > avail {
            break;
        }
        used += w;
        visible.push(*ch);
    }
    (visible, cursor_column.saturating_sub(hidden))
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        draw_too_small(frame, area);
        return;
    }
    if matches!(app.popup, Some(Popup::Fullscreen)) {
        draw_fullscreen(frame, area, app);
        return;
    }
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);
    draw_header(frame, header, app);
    if body.width >= WIDE_WIDTH {
        let [menu, content, preview] = Layout::horizontal([
            Constraint::Length(MENU_WIDTH),
            Constraint::Percentage(42),
            Constraint::Min(0),
        ])
        .areas(body);
        draw_menu(frame, menu, app);
        draw_content(frame, content, app);
        draw_preview(frame, preview, app);
    } else {
        let [top, preview] =
            Layout::vertical([Constraint::Percentage(55), Constraint::Min(0)]).areas(body);
        let [menu, content] =
            Layout::horizontal([Constraint::Length(MENU_WIDTH), Constraint::Min(0)]).areas(top);
        draw_menu(frame, menu, app);
        draw_content(frame, content, app);
        draw_preview(frame, preview, app);
    }
    draw_footer(frame, footer, app);
    draw_popup(frame, area, app);
}

fn draw_too_small(frame: &mut Frame, area: Rect) {
    let row = Rect::new(area.x, area.y + area.height / 2, area.width, 1);
    let message = Paragraph::new(Line::from(Span::styled(
        format!("Terminal too small (need {MIN_WIDTH}×{MIN_HEIGHT}) — press q to quit"),
        palette().warn,
    )))
    .alignment(Alignment::Center);
    frame.render_widget(message, row);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let palette = palette();
    let [left, right] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(12)]).areas(area);
    let title = Line::from(Span::styled(
        " AtlasFetch setup",
        palette.accent.add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(Paragraph::new(title), left);
    let (text, style) = if app.is_dirty() {
        ("● unsaved", palette.warn)
    } else {
        ("✓ saved", palette.ok)
    };
    let status = Line::from(Span::styled(text, style));
    frame.render_widget(Paragraph::new(status).alignment(Alignment::Right), right);
}

fn draw_menu(frame: &mut Frame, area: Rect, app: &mut App) {
    let focused = app.focus == Focus::Menu;
    let items: Vec<ListItem<'static>> = Section::ALL
        .iter()
        .map(|section| ListItem::new(section.label()))
        .collect();
    let block = Block::bordered()
        .title(" Sections ")
        .border_style(border_style(focused));
    let list = List::new(items)
        .block(block)
        .highlight_style(selection_style(focused))
        .highlight_symbol(if focused { "› " } else { "  " });
    frame.render_stateful_widget(list, area, &mut app.menu_state);
}

fn draw_content(frame: &mut Frame, area: Rect, app: &mut App) {
    let section = app.section();
    let focused = app.focus() == Focus::Content;
    let block = Block::bordered()
        .title(format!(" {} ", section.label()))
        .border_style(border_style(focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    match section {
        Section::Appearance => {
            let form = if app.welcome_visible() {
                let [welcome, rest] =
                    Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(inner);
                draw_welcome(frame, welcome);
                rest
            } else {
                inner
            };
            let rows = app.section_rows(section);
            draw_form(frame, form, app, section, &rows, focused);
        }
        Section::Layout => {
            let [form, note] =
                Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas(inner);
            let rows = app.section_rows(section);
            draw_form(frame, form, app, section, &rows, focused);
            let description = app.cfg.scene.description();
            draw_note(frame, note, &[description]);
        }
        Section::Startup => {
            let [form, note] =
                Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas(inner);
            let rows = app.section_rows(section);
            draw_form(frame, form, app, section, &rows, focused);
            draw_note(
                frame,
                note,
                &[
                    "Fetch: print the summary once and exit",
                    "Monitor: keep the summary refreshing in a live view",
                ],
            );
        }
        Section::Logo => draw_logo(frame, inner, app, focused),
        Section::Fields => draw_fields(frame, inner, app, focused),
    }
}

fn draw_welcome(frame: &mut Frame, area: Rect) {
    let palette = palette();
    let lines = vec![
        Line::from(Span::styled(
            "Welcome to AtlasFetch setup.",
            palette.accent.add_modifier(Modifier::BOLD),
        )),
        Line::from("Pick a section with ↑↓ and open it with →. Changes apply live."),
        Line::from(Span::styled(
            "F1 lists every key. Ctrl+S saves, q quits.",
            palette.muted,
        )),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_note(frame: &mut Frame, area: Rect, lines: &[&str]) {
    let muted = palette().muted;
    let lines: Vec<Line> = lines
        .iter()
        .map(|line| Line::from(Span::styled(line.to_string(), muted)))
        .collect();
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

/// A list of settings with label, value, optional swatches and a highlighted selection.
fn draw_form(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
    section: Section,
    rows: &[Setting],
    focused: bool,
) {
    let idx = section.index();
    let label_w = rows
        .iter()
        .map(|setting| text_width(setting.label()))
        .max()
        .unwrap_or(0)
        .min(LABEL_COLUMNS);
    let width = usize::from(area.width).saturating_sub(2);
    let items: Vec<ListItem<'static>> = rows
        .iter()
        .map(|setting| form_item(app, *setting, label_w, width))
        .collect();
    let selected = rows
        .len()
        .checked_sub(1)
        .map(|last| app.form_sel[idx].min(last));
    app.form_states[idx].select(selected);
    let list = List::new(items)
        .highlight_style(selection_style(focused))
        .highlight_symbol(if focused { "› " } else { "  " });
    frame.render_stateful_widget(list, area, &mut app.form_states[idx]);
}

fn form_item(app: &App, setting: Setting, label_w: usize, width: usize) -> ListItem<'static> {
    let palette = palette();
    let swatches = app.setting_swatch(setting);
    let shown = swatches.len().min(MAX_SWATCHES);
    let swatch_w = if shown == 0 { 0 } else { shown * 2 + 1 };
    let raw = app.setting_value(setting);
    let value = if setting == Setting::SavePalette {
        "press Enter".to_string()
    } else if setting.is_choice() || setting.is_number() {
        format!("‹ {raw} ›")
    } else {
        raw
    };
    let value_w = width.saturating_sub(label_w + 2 + swatch_w);
    let mut spans = vec![
        Span::styled(fit(setting.label(), label_w), Style::default()),
        Span::raw("  "),
        Span::styled(truncate_text(&value, value_w), palette.accent),
    ];
    if shown > 0 {
        spans.push(Span::raw(" "));
        for color in swatches.iter().take(MAX_SWATCHES) {
            spans.push(Span::styled(
                "██",
                Style::default().fg(swatch_color(*color)),
            ));
        }
    }
    ListItem::new(Line::from(spans))
}

fn draw_logo(frame: &mut Frame, area: Rect, app: &mut App, focused: bool) {
    let rows = app.section_rows(Section::Logo);
    let form_focused = focused && !app.logo_on_list;
    let rows_h = u16::try_from(rows.len()).unwrap_or(u16::MAX);
    let [top, filter_area, list_area] = Layout::vertical([
        Constraint::Length(rows_h),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(area);
    draw_form(frame, top, app, Section::Logo, &rows, form_focused);

    let muted = palette().muted;
    let typing = focused && app.logo_on_list;
    let prompt = "Filter: ";
    let mut spans = vec![Span::styled(prompt, muted)];
    if app.logo_filter.is_empty() {
        spans.push(Span::styled("____", muted));
    } else {
        spans.push(Span::styled(app.logo_filter.clone(), palette().accent));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), filter_area);
    if typing {
        let offset = text_width(prompt) + text_width(&app.logo_filter);
        let max_x = filter_area.right().saturating_sub(1);
        let x = (filter_area.x + offset as u16).min(max_x);
        frame.set_cursor_position((x, filter_area.y));
    }

    let keys = app.filtered_logo_keys();
    if keys.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No built-in logo matches the filter",
                muted,
            ))),
            list_area,
        );
        return;
    }
    let items: Vec<ListItem<'static>> = keys.into_iter().map(ListItem::new).collect();
    let list = List::new(items)
        .highlight_style(selection_style(typing))
        .highlight_symbol(if typing { "› " } else { "  " });
    frame.render_stateful_widget(list, list_area, &mut app.logo_state);
}

fn draw_fields(frame: &mut Frame, area: Rect, app: &mut App, focused: bool) {
    let rows = app.field_rows();
    let label_w = [Side::Left, Side::Right]
        .iter()
        .flat_map(|side| app.entries(*side).iter())
        .map(|entry| text_width(&entry.label))
        .max()
        .unwrap_or(0)
        .min(FIELD_LABEL_COLUMNS);
    let width = usize::from(area.width).saturating_sub(2);
    let items: Vec<ListItem<'static>> = rows
        .iter()
        .map(|row| field_item(app, *row, label_w, width))
        .collect();
    let list = List::new(items)
        .highlight_style(selection_style(focused))
        .highlight_symbol(if focused { "› " } else { "  " });
    frame.render_stateful_widget(list, area, &mut app.fields_state);
}

fn field_item(app: &App, row: FieldRow, label_w: usize, width: usize) -> ListItem<'static> {
    let palette = palette();
    match row {
        FieldRow::Header(side) => {
            let count = app.entries(side).len();
            let text = format!("{} ({count})", side.label());
            ListItem::new(Line::from(Span::styled(
                text,
                palette.accent.add_modifier(Modifier::BOLD),
            )))
        }
        FieldRow::Entry(side, index) => {
            let Some(entry) = app.entries(side).get(index) else {
                return ListItem::new(Line::default());
            };
            let check = if entry.enabled { "[✓]" } else { "[ ]" };
            let icon = if entry.icon.is_empty() {
                String::new()
            } else {
                format!("{} ", entry.icon)
            };
            let prefix = format!("{check} {icon}");
            let label_style = if entry.enabled {
                Style::default()
            } else {
                palette.muted
            };
            let value = app.info().get(entry.field).map(str::to_string);
            let mut markers: Vec<(&str, Style)> = Vec::new();
            if entry.bar {
                markers.push(("  ▮bar", palette.accent));
            }
            if entry.field.live_only() {
                markers.push(("  (live)", palette.muted));
            }
            if value.is_none() {
                markers.push(("  (empty)", palette.muted));
            }
            let marker_w: usize = markers.iter().map(|(text, _)| text_width(text)).sum();
            let fixed = text_width(&prefix) + label_w + 2 + marker_w;
            let value_w = width.saturating_sub(fixed);

            let mut spans = vec![
                Span::styled(prefix, palette.muted),
                Span::styled(fit(&entry.label, label_w), label_style),
            ];
            if let Some(value) = value.filter(|_| value_w >= 4) {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(truncate_text(&value, value_w), palette.muted));
            }
            for (text, style) in markers {
                spans.push(Span::styled(text, style));
            }
            ListItem::new(Line::from(spans))
        }
    }
}

fn render_scene(app: &App, width: usize) -> Vec<render::Line> {
    let ctx = RenderCtx {
        info: app.info(),
        cfg: &app.cfg,
        logos: app.logos(),
        width,
    };
    scene::render(app.cfg.scene, &ctx)
}

fn draw_preview(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered()
        .title(format!(
            " Preview · {} · {} cols · F2 full screen ",
            app.cfg.scene.label(),
            area.width.saturating_sub(2)
        ))
        .border_style(palette().muted);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines = render_scene(app, usize::from(inner.width));
    frame.render_widget(Paragraph::new(render::to_ratatui(&lines)), inner);
}

fn draw_fullscreen(frame: &mut Frame, area: Rect, app: &App) {
    let lines = render_scene(app, usize::from(area.width));
    frame.render_widget(Paragraph::new(render::to_ratatui(&lines)), area);
}

fn footer_hints(app: &App) -> &'static str {
    if let Some(popup) = &app.popup {
        return match popup {
            Popup::Help { .. } => "↑↓ scroll  PgUp/PgDn page  Esc close",
            Popup::Fullscreen => "any key returns to the editor",
            Popup::Confirm { .. } => "←→ choose  Enter confirm  Esc cancel",
            Popup::Text(_) => "Enter apply  Esc cancel  Ctrl+S save",
            Popup::Themes { .. } => "type to filter  ↑↓ choose  Enter apply  Esc cancel",
            Popup::Entry { editing, .. } => {
                if editing.is_some() {
                    "Enter apply  Esc cancel edit"
                } else {
                    "↑↓ choose  Enter edit  Space toggle  Esc close"
                }
            }
            Popup::Add { .. } => "type to filter  ↑↓ choose  Enter add  Esc close",
        };
    }
    match app.focus() {
        Focus::Menu => "↑↓ section  →/Enter open  F1 help  Ctrl+S save  q quit",
        Focus::Content => match app.section() {
            Section::Logo if app.logo_on_list => {
                "type to filter  ↑↓ choose  Enter apply  Esc clear  ← menu"
            }
            Section::Logo => "↑↓ move  ←→ change  Enter edit  ↓ logo list  Esc menu  F1 help",
            Section::Fields => {
                "↑↓ select  Space show/hide  Shift+↑↓ move  Shift+←→ panel  Enter edit  a add  Del remove  Ctrl+Z undo  Esc menu"
            }
            _ => "↑↓ move  ←→ change  Enter edit  Space toggle  Esc menu  F1 help",
        },
    }
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let palette = palette();
    let (status_text, status_style) = match &app.status {
        Some(status) => (
            status.text.clone(),
            if status.error {
                palette.error
            } else {
                palette.ok
            },
        ),
        None => (String::new(), Style::default()),
    };
    let max_status = usize::from(area.width) / 2;
    let status_w = if status_text.is_empty() {
        0
    } else {
        (text_width(&status_text) + 2).min(max_status)
    };
    let width = u16::try_from(status_w).unwrap_or(0);
    let [hints, status_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(width)]).areas(area);
    let hint_text = truncate_text(footer_hints(app), usize::from(hints.width));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(hint_text, palette.muted))),
        hints,
    );
    if status_w > 0 {
        let text = truncate_text(&status_text, status_w.saturating_sub(2));
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(text, status_style)))
                .alignment(Alignment::Right),
            status_area,
        );
    }
}

/// Clears `rect`, draws its bordered frame and returns the inner area.
fn popup_frame(frame: &mut Frame, rect: Rect, title: &str) -> Rect {
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .title(format!(" {title} "))
        .border_style(palette().accent);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}

fn draw_popup(frame: &mut Frame, area: Rect, app: &mut App) {
    // Taken out of the app so the popup can be drawn with its scroll and list state mutable.
    let Some(mut popup) = app.popup.take() else {
        return;
    };
    render_popup(frame, area, app, &mut popup);
    app.popup = Some(popup);
}

fn render_popup(frame: &mut Frame, area: Rect, app: &App, popup: &mut Popup) {
    match popup {
        Popup::Help { scroll } => draw_help(frame, area, scroll),
        Popup::Fullscreen => {}
        Popup::Confirm { choice } => draw_confirm(frame, area, *choice),
        Popup::Text(text) => draw_text_popup(frame, area, text),
        Popup::Themes { filter, state } => draw_themes(frame, area, app, filter, state),
        Popup::Entry {
            side,
            index,
            state,
            editing,
        } => draw_entry(frame, area, app, *side, *index, state, editing.as_ref()),
        Popup::Add { filter, state, .. } => draw_add(frame, area, app, filter, state),
    }
}

const HELP: &[(&str, &[(&str, &str)])] = &[
    (
        "Everywhere",
        &[
            ("Ctrl+S", "save and exit"),
            ("Ctrl+C", "quit without saving"),
            ("q", "quit; asks first when there are unsaved changes"),
            ("F1 or ?", "this help"),
            ("F2", "full-screen preview; any key returns"),
            ("Ctrl+Z", "restore the last removed field"),
            (
                "Paste",
                "multi-line text in the Logo section becomes the logo",
            ),
        ],
    ),
    (
        "Sections",
        &[
            ("↑ ↓", "choose a section"),
            ("→ Enter Tab", "open the section"),
        ],
    ),
    (
        "Settings",
        &[
            ("↑ ↓", "move between rows"),
            ("← →", "change a choice or number"),
            ("Enter", "edit, open or toggle"),
            ("Space", "toggle a switch"),
            ("Esc Tab", "back to the sections"),
        ],
    ),
    (
        "Logo",
        &[
            ("type", "filter the built-in logos"),
            ("Backspace", "delete a filter character"),
            ("Esc", "clear the filter, then leave the list"),
            ("↑ ↓ PgUp PgDn", "move through the list"),
        ],
    ),
    (
        "Fields",
        &[
            ("↑ ↓", "select an entry"),
            ("Space", "show or hide the entry"),
            ("Shift+↑ ↓", "move within a panel, or across its edge"),
            ("Shift+← →", "move the entry to the left or right panel"),
            ("Enter", "edit label, icon and bar"),
            ("a or Insert", "add a field"),
            ("Delete", "remove the entry"),
        ],
    ),
    (
        "Popups",
        &[
            ("↑ ↓", "move in a list"),
            ("Enter", "apply"),
            ("Esc", "cancel or close"),
        ],
    ),
];

fn help_lines() -> Vec<Line<'static>> {
    let palette = palette();
    let mut lines = Vec::new();
    for (heading, rows) in HELP {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.push(Line::from(Span::styled(
            heading.to_string(),
            palette.accent.add_modifier(Modifier::BOLD),
        )));
        for (key, description) in rows.iter() {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {}", fit(key, 16)),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(description.to_string()),
            ]));
        }
    }
    lines
}

fn draw_help(frame: &mut Frame, area: Rect, scroll: &mut u16) {
    let rect = centered(area, 72, area.height.saturating_sub(2));
    let inner = popup_frame(frame, rect, "Keys");
    let lines = help_lines();
    let visible = usize::from(inner.height);
    let max_scroll = u16::try_from(lines.len().saturating_sub(visible)).unwrap_or(u16::MAX);
    *scroll = (*scroll).min(max_scroll);
    frame.render_widget(Paragraph::new(lines).scroll((*scroll, 0)), inner);
}

fn draw_confirm(frame: &mut Frame, area: Rect, choice: ConfirmChoice) {
    let rect = centered(area, 52, 7);
    let inner = popup_frame(frame, rect, "Unsaved changes");
    let [question, _, buttons, _, hint] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new("Save changes before quitting?").alignment(Alignment::Center),
        question,
    );
    let mut spans = Vec::new();
    for (option, label) in [
        (ConfirmChoice::Save, "Save"),
        (ConfirmChoice::Discard, "Discard"),
        (ConfirmChoice::Cancel, "Cancel"),
    ] {
        let style = if option == choice {
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::default()
        };
        spans.push(Span::styled(format!(" {label} "), style));
        spans.push(Span::raw("  "));
    }
    spans.pop();
    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
        buttons,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "←→ choose  Enter confirm  Esc cancel",
            palette().muted,
        )))
        .alignment(Alignment::Center),
        hint,
    );
}

fn text_title(target: Setting) -> &'static str {
    match target {
        Setting::Palette => "Palette",
        Setting::TitleColor => "Title color",
        Setting::SeparatorColor => "Separator color",
        Setting::ValueColor => "Value color",
        Setting::SavePalette => "Save palette as",
        Setting::TitleFormat => "Title format",
        Setting::TitleSeparator => "Separator",
        Setting::LogoPath => "Logo file",
        other => other.label(),
    }
}

fn text_hint(target: Setting) -> &'static str {
    match target {
        Setting::Palette => "Hex colors separated by spaces, 1 to 16",
        Setting::TitleColor | Setting::SeparatorColor | Setting::ValueColor => "#RRGGBB",
        Setting::SavePalette => "A name for the palette; an existing name needs a second Enter",
        Setting::TitleFormat => "Placeholders: {user} {host}",
        Setting::TitleSeparator => "0 to 4 characters; empty hides the separator",
        Setting::LogoPath => "Path to a text file; ~ is your home directory",
        _ => "",
    }
}

fn prompt_line(input: &TextInput, width: usize) -> (Line<'static>, usize) {
    let prompt = "› ";
    let avail = width.saturating_sub(text_width(prompt));
    let (visible, cursor) = input_window(input.value(), input.cursor_column(), avail);
    let line = Line::from(vec![
        Span::styled(prompt, palette().accent),
        Span::raw(visible),
    ]);
    (line, text_width(prompt) + cursor)
}

fn draw_text_popup(frame: &mut Frame, area: Rect, text: &super::app::TextPopup) {
    let rect = centered(area, 64, 10);
    let inner = popup_frame(frame, rect, text_title(text.target));
    let [input_row, swatch_row, hint_row, error_row, keys_row, _] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(inner);

    let (line, cursor) = prompt_line(&text.input, usize::from(inner.width));
    frame.render_widget(Paragraph::new(line), input_row);
    let max_x = input_row.right().saturating_sub(1);
    let x = (input_row.x + cursor as u16).min(max_x);
    frame.set_cursor_position((x, input_row.y));

    let colors: Vec<ThemeColor> = match text.target {
        Setting::Palette => parse_palette(text.input.value()).unwrap_or_default(),
        Setting::TitleColor | Setting::SeparatorColor | Setting::ValueColor => {
            ThemeColor::from_hex(text.input.value().trim())
                .into_iter()
                .collect()
        }
        _ => Vec::new(),
    };
    if !colors.is_empty() {
        let mut spans = Vec::new();
        for color in colors.iter().take(MAX_SWATCHES * 2) {
            spans.push(Span::styled(
                "██",
                Style::default().fg(swatch_color(*color)),
            ));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), swatch_row);
    }

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text_hint(text.target),
            palette().muted,
        ))),
        hint_row,
    );
    if let Some(error) = &text.error {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                truncate_text(error, usize::from(inner.width)),
                palette().error,
            ))),
            error_row,
        );
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Enter apply  Esc cancel  Ctrl+S save",
            palette().muted,
        ))),
        keys_row,
    );
}

fn draw_themes(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    filter: &TextInput,
    state: &mut ratatui::widgets::ListState,
) {
    let choices = app.filtered_themes(filter.value());
    let height = u16::try_from(choices.len())
        .unwrap_or(u16::MAX)
        .saturating_add(4)
        .max(8);
    let rect = centered(area, 56, height);
    let inner = popup_frame(frame, rect, "Palette");
    let [filter_row, list_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);

    let (line, cursor) = filter_line(filter, usize::from(filter_row.width));
    frame.render_widget(Paragraph::new(line), filter_row);
    let x = (filter_row.x + cursor as u16).min(filter_row.right().saturating_sub(1));
    frame.set_cursor_position((x, filter_row.y));

    if choices.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No palette matches the filter",
                palette().muted,
            ))),
            list_area,
        );
        return;
    }
    let items: Vec<ListItem<'static>> = choices
        .iter()
        .map(|(name, colors)| {
            let mut spans = vec![Span::raw(fit(name, 22)), Span::raw(" ")];
            for color in colors.iter().take(MAX_SWATCHES) {
                spans.push(Span::styled(
                    "██",
                    Style::default().fg(swatch_color(*color)),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let list = List::new(items)
        .highlight_style(selection_style(true))
        .highlight_symbol("› ");
    frame.render_stateful_widget(list, list_area, state);
}

/// "Filter: " followed by the filter text, and the cursor column.
fn filter_line(filter: &TextInput, width: usize) -> (Line<'static>, usize) {
    let prompt = "Filter: ";
    let avail = width.saturating_sub(text_width(prompt));
    let (visible, cursor) = input_window(filter.value(), filter.cursor_column(), avail);
    let line = Line::from(vec![
        Span::styled(prompt, palette().muted),
        Span::raw(visible),
    ]);
    (line, text_width(prompt) + cursor)
}

fn draw_entry(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    side: Side,
    index: usize,
    state: &mut ratatui::widgets::ListState,
    editing: Option<&TextInput>,
) {
    let Some(entry) = app.entries(side).get(index) else {
        return;
    };
    let rect = centered(area, 60, 10);
    let inner = popup_frame(frame, rect, entry.field.label());
    let list_area = inner;
    let label_w = 12;
    let selected = state.selected().unwrap_or(0);
    let palette = palette();
    let bar_text = if entry.bar { "on" } else { "off" };
    let bar_note = if entry.field.has_gauge() {
        ""
    } else {
        "(this field has no gauge)"
    };
    let value_of = |row: usize| -> String {
        match row {
            0 => entry.label.clone(),
            1 => entry.icon.clone(),
            2 => bar_text.to_string(),
            _ => "reset to defaults".to_string(),
        }
    };
    let names = ["Label", "Icon", "Show as bar", "Reset"];
    let mut items = Vec::new();
    for (row, name) in names.iter().enumerate() {
        let mut spans = vec![
            Span::styled(fit(name, label_w), Style::default()),
            Span::raw("  "),
        ];
        match (row, editing) {
            (r, Some(input)) if r == selected && (r == 0 || r == 1) => {
                let avail = usize::from(list_area.width).saturating_sub(label_w + 4);
                let (visible, _) = input_window(input.value(), input.cursor_column(), avail);
                spans.push(Span::styled(visible, palette.accent));
            }
            _ => {
                spans.push(Span::styled(value_of(row), palette.accent));
            }
        }
        if row == 1 && editing.is_none() {
            spans.push(Span::styled("  leave empty for none", palette.muted));
        }
        if row == 2 && !bar_note.is_empty() {
            spans.push(Span::styled(format!("  {bar_note}"), palette.muted));
        }
        items.push(ListItem::new(Line::from(spans)));
    }
    if let Some(input) = editing {
        let (_, cursor) = {
            let avail = usize::from(list_area.width).saturating_sub(label_w + 4);
            input_window(input.value(), input.cursor_column(), avail)
        };
        let y = list_area.y + u16::try_from(selected).unwrap_or(0);
        let x = list_area.x + 2 + (label_w + 2) as u16 + cursor as u16;
        frame.set_cursor_position((x.min(list_area.right().saturating_sub(1)), y));
    }
    let list = List::new(items)
        .highlight_style(selection_style(true))
        .highlight_symbol("› ");
    frame.render_stateful_widget(list, list_area, state);
}

fn field_in_use(app: &App, field: crate::field::Field) -> bool {
    [Side::Left, Side::Right]
        .iter()
        .any(|side| app.entries(*side).iter().any(|entry| entry.field == field))
}

fn draw_add(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    filter: &TextInput,
    state: &mut ratatui::widgets::ListState,
) {
    let rect = centered(area, 66, 22);
    let inner = popup_frame(frame, rect, "Add a field");
    let [filter_row, list_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);

    let (line, cursor) = filter_line(filter, usize::from(filter_row.width));
    frame.render_widget(Paragraph::new(line), filter_row);
    let x = (filter_row.x + cursor as u16).min(filter_row.right().saturating_sub(1));
    frame.set_cursor_position((x, filter_row.y));

    let rows = add_rows(filter.value());
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No field matches the filter",
                palette().muted,
            ))),
            list_area,
        );
        return;
    }
    let palette = palette();
    let items: Vec<ListItem<'static>> = rows
        .iter()
        .map(|row| match row {
            AddRow::Group(group) => ListItem::new(Line::from(Span::styled(
                group.label(),
                palette.accent.add_modifier(Modifier::BOLD),
            ))),
            AddRow::Field(field) => {
                let used = field_in_use(app, *field);
                let text_style = if used {
                    palette.muted
                } else {
                    Style::default()
                };
                let mut spans = vec![
                    Span::styled(fit(field.label(), 16), text_style),
                    Span::styled(format!(" — {}", field.description()), palette.muted),
                ];
                if used {
                    spans.push(Span::styled("  (in use)", palette.muted));
                }
                ListItem::new(Line::from(spans))
            }
        })
        .collect();
    let list = List::new(items)
        .highlight_style(selection_style(true))
        .highlight_symbol("› ");
    frame.render_stateful_widget(list, list_area, state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::info::SysInfo;
    use crate::tui::app::Outcome;
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;

    fn new_app() -> App {
        App::new_for_test(Config::default(), SysInfo::sample())
    }

    fn screen(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
        terminal
            .draw(|frame| draw(frame, app))
            .expect("drawing into the test backend");
        buffer_text(terminal.backend().buffer())
    }

    fn buffer_text(buffer: &Buffer) -> String {
        let mut text = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }

    fn press(app: &mut App, code: KeyCode) {
        app.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    #[test]
    fn wide_terminal_shows_every_section_and_the_preview() {
        let mut app = new_app();
        let text = screen(&mut app, 120, 40);
        for name in [
            "Appearance",
            "Logo",
            "Fields",
            "Layout",
            "Startup",
            "Preview",
        ] {
            assert!(text.contains(name), "missing {name}");
        }
        assert!(text.contains("saved"));
    }

    #[test]
    fn narrow_terminal_stacks_preview_below_content() {
        let mut app = new_app();
        let text = screen(&mut app, 70, 24);
        assert!(text.contains("Appearance"));
        assert!(text.contains("Preview"));
    }

    #[test]
    fn small_terminal_shows_the_size_message() {
        let mut app = new_app();
        let text = screen(&mut app, 59, 17);
        assert!(text.contains("Terminal too small"));
    }

    #[test]
    fn fields_section_lists_both_panels() {
        let mut app = new_app();
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Right);
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("Left panel"));
        assert!(text.contains("Right panel"));
    }

    #[test]
    fn logo_section_shows_filter_and_list() {
        let mut app = new_app();
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Right);
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("Filter:"));
    }

    #[test]
    fn every_popup_renders_without_panicking() {
        let mut app = new_app();
        press(&mut app, KeyCode::Right);
        let popups = [
            Popup::Help { scroll: 0 },
            Popup::Fullscreen,
            Popup::Confirm {
                choice: ConfirmChoice::Save,
            },
            Popup::Text(super::super::app::TextPopup {
                target: Setting::Palette,
                input: TextInput::new("#FF0000 #00FF00", 160),
                error: Some("example".to_string()),
                armed: None,
            }),
            Popup::Themes {
                filter: TextInput::new("", 32),
                state: ratatui::widgets::ListState::default().with_selected(Some(0)),
            },
            Popup::Entry {
                side: Side::Left,
                index: 0,
                state: ratatui::widgets::ListState::default().with_selected(Some(0)),
                editing: None,
            },
            Popup::Add {
                filter: TextInput::new("", 32),
                state: ratatui::widgets::ListState::default().with_selected(Some(1)),
                anchor: None,
            },
        ];
        for popup in popups {
            app.popup = Some(popup);
            let text = screen(&mut app, 80, 30);
            assert!(!text.is_empty());
        }
        assert_eq!(app.outcome(), None::<Outcome>);
    }

    #[test]
    fn help_popup_lists_the_global_keys() {
        let mut app = new_app();
        press(&mut app, KeyCode::F(1));
        let text = screen(&mut app, 100, 40);
        assert!(text.contains("Ctrl+S"));
        assert!(text.contains("Shift+"));
    }

    #[test]
    fn long_values_stay_inside_the_screen_width() {
        let mut app = new_app();
        app.cfg.title.format = "{user}".repeat(10);
        let text = screen(&mut app, 60, 18);
        for line in text.lines() {
            assert!(text_width(line) <= 60);
        }
    }

    #[test]
    fn input_window_keeps_the_cursor_visible() {
        let (visible, cursor) = input_window("abcdefghij", 10, 4);
        assert_eq!(visible, "hij");
        assert_eq!(cursor, 3);
        let (visible, cursor) = input_window("abc", 1, 10);
        assert_eq!(visible, "abc");
        assert_eq!(cursor, 1);
    }
}
