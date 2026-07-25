use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::ascii;
use crate::component;
use crate::config::FieldDef;
use crate::layout::AppLayout;
use crate::theme::Color;

use super::state::{Editor, InputMode, Tab};

fn move_up<T>(items: &mut [T], selected: usize) -> usize {
    if selected > 0 && selected < items.len() {
        items.swap(selected, selected - 1);
        selected - 1
    } else {
        selected.min(items.len().saturating_sub(1))
    }
}

pub(super) fn handle_event(editor: &mut Editor) -> Result<bool> {
    let previous = editor.cfg.clone();
    let keep_running = handle_event_inner(editor)?;

    if editor.cfg != previous {
        editor.changed = true;
        editor.status_message = "Unsaved changes".into();
    }

    Ok(keep_running)
}

fn handle_event_inner(editor: &mut Editor) -> Result<bool> {
    match event::read()? {
        Event::Resize(w, h) => {
            editor.term_width = w;
            editor.term_height = h;
            editor.preview_width = (w.saturating_sub(20)) as usize;
            editor.dirty = true;
        }
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            if key.code == KeyCode::Char('s')
                && key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(
                    editor.input_mode,
                    InputMode::Normal | InputMode::Help | InputMode::ConfirmQuit
                )
            {
                editor.saved = true;
                editor.changed = false;
                editor.status_message = "Saved".into();
                return Ok(false);
            }

            // Handle input modes first
            match editor.input_mode {
                InputMode::Help => {
                    if matches!(
                        key.code,
                        KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
                    ) {
                        editor.input_mode = InputMode::Normal;
                        editor.status_message = "Help closed".into();
                    }
                    return Ok(true);
                }
                InputMode::ConfirmQuit => {
                    match key.code {
                        KeyCode::Char('y') => return Ok(false),
                        KeyCode::Char('s') => {
                            editor.saved = true;
                            editor.changed = false;
                            editor.status_message = "Saved".into();
                            return Ok(false);
                        }
                        KeyCode::Char('n') | KeyCode::Char('q') | KeyCode::Esc => {
                            editor.input_mode = InputMode::Normal;
                            editor.status_message = "Continuing editing".into();
                        }
                        _ => {}
                    }
                    return Ok(true);
                }
                InputMode::EditingCustomPalette => {
                    match key.code {
                        KeyCode::Enter => {
                            let parsed: Vec<Color> = editor
                                .custom_palette_input
                                .split_whitespace()
                                .filter_map(Color::from_hex_opt)
                                .collect();
                            if !parsed.is_empty() {
                                editor.cfg.logo.colors = parsed;
                                editor.dirty = true;
                            }
                            editor.input_mode = InputMode::Normal;
                        }
                        KeyCode::Esc => {
                            editor.input_mode = InputMode::Normal;
                        }
                        KeyCode::Char(c) => {
                            editor.custom_palette_input.push(c);
                        }
                        KeyCode::Backspace => {
                            editor.custom_palette_input.pop();
                        }
                        _ => {}
                    }
                    return Ok(true);
                }
                InputMode::EditingLabel => {
                    match key.code {
                        KeyCode::Enter => {
                            let fields = if editor.panel_focus {
                                &mut editor.cfg.display.right
                            } else {
                                &mut editor.cfg.display.left
                            };
                            let idx = if editor.panel_focus {
                                editor.panel_right_sel
                            } else {
                                editor.panel_left_sel
                            };
                            if idx < fields.len() {
                                fields[idx].label = editor.editing_label_input.clone();
                                editor.dirty = true;
                            }
                            editor.input_mode = InputMode::Normal;
                        }
                        KeyCode::Esc => {
                            editor.input_mode = InputMode::Normal;
                        }
                        KeyCode::Char(c) => {
                            editor.editing_label_input.push(c);
                        }
                        KeyCode::Backspace => {
                            editor.editing_label_input.pop();
                        }
                        _ => {}
                    }
                    return Ok(true);
                }
                InputMode::AddingPanel => {
                    match key.code {
                        KeyCode::Up => {
                            editor.add_panel_sel = editor.add_panel_sel.saturating_sub(1);
                        }
                        KeyCode::Down => {
                            editor.add_panel_sel = (editor.add_panel_sel + 1)
                                .min(editor.add_panel_available.len().saturating_sub(1));
                        }
                        KeyCode::Enter => {
                            if editor.add_panel_sel < editor.add_panel_available.len() {
                                let (k, i, l) = &editor.add_panel_available[editor.add_panel_sel];
                                let fd = FieldDef {
                                    field: k.clone(),
                                    icon: i.clone(),
                                    label: l.clone(),
                                    enabled: true,
                                };
                                // Check if already exists in either panel
                                let exists = editor
                                    .cfg
                                    .display
                                    .left
                                    .iter()
                                    .chain(editor.cfg.display.right.iter())
                                    .any(|f| f.field == fd.field);
                                if !exists {
                                    editor.cfg.display.left.push(fd);
                                    editor.dirty = true;
                                }
                            }
                            editor.input_mode = InputMode::Normal;
                        }
                        KeyCode::Esc => {
                            editor.input_mode = InputMode::Normal;
                        }
                        _ => {}
                    }
                    return Ok(true);
                }
                InputMode::PastingAscii => {
                    match key.code {
                        KeyCode::Enter => {
                            if !editor.paste_buffer.is_empty() {
                                editor.ascii_art = editor.paste_buffer.clone();
                                editor.ascii_source = "pasted".into();
                                editor.ascii_selected = editor.logo_keys.len() + 1; // "Paste" slot
                                editor.dirty = true;
                                editor.changed = true;
                                editor.status_message = "Unsaved pasted ASCII".into();
                            }
                            editor.paste_buffer.clear();
                            editor.input_mode = InputMode::Normal;
                        }
                        KeyCode::Esc => {
                            editor.paste_buffer.clear();
                            editor.input_mode = InputMode::Normal;
                        }
                        KeyCode::Char(c) => {
                            editor.paste_buffer.push(c);
                        }
                        KeyCode::Backspace => {
                            editor.paste_buffer.pop();
                        }
                        _ => {}
                    }
                    return Ok(true);
                }
                InputMode::BrowsingFile => {
                    match key.code {
                        KeyCode::Up => {
                            editor.file_browser_sel = editor.file_browser_sel.saturating_sub(1);
                        }
                        KeyCode::Down => {
                            editor.file_browser_sel = (editor.file_browser_sel + 1)
                                .min(editor.file_browser_entries.len().saturating_sub(1));
                        }
                        KeyCode::Char('~') => {
                            editor.file_browser_cwd = std::env::var("HOME")
                                .map(std::path::PathBuf::from)
                                .unwrap_or_else(|_| "/".into());
                            editor.refresh_file_browser();
                            editor.file_browser_sel = 0;
                        }
                        KeyCode::Backspace => {
                            if editor.file_browser_cwd.pop() {
                                editor.refresh_file_browser();
                                editor.file_browser_sel = 0;
                            }
                        }
                        KeyCode::Enter => {
                            if editor.file_browser_sel < editor.file_browser_entries.len() {
                                let (name, is_dir) =
                                    &editor.file_browser_entries[editor.file_browser_sel];
                                if *is_dir {
                                    editor.file_browser_cwd.push(name);
                                    editor.refresh_file_browser();
                                    editor.file_browser_sel = 0;
                                } else {
                                    let path = editor.file_browser_cwd.join(name);
                                    let path_str = path.to_string_lossy().to_string();
                                    editor.cfg.logo.key.clear();
                                    editor.cfg.logo.path = path_str.clone();
                                    editor.ascii_source = format!("file:{}", path_str);
                                    if let Ok(art) = ascii::load(&editor.cfg) {
                                        editor.ascii_art = art;
                                    }
                                    editor.ascii_selected = editor.logo_keys.len(); // "Custom file" slot
                                    editor.dirty = true;
                                    editor.input_mode = InputMode::Normal;
                                }
                            }
                        }
                        KeyCode::Esc => {
                            editor.input_mode = InputMode::Normal;
                        }
                        _ => {}
                    }
                    return Ok(true);
                }
                InputMode::SearchingAscii => {
                    match key.code {
                        KeyCode::Esc => {
                            editor.ascii_search.clear();
                            editor.input_mode = InputMode::Normal;
                        }
                        KeyCode::Enter | KeyCode::Tab => {
                            editor.input_mode = InputMode::Normal;
                        }
                        KeyCode::Backspace => {
                            if !editor.ascii_search.is_empty() {
                                editor.ascii_search.pop();
                                editor.jump_to_first_matching_logo();
                                editor.dirty = true;
                            }
                        }
                        KeyCode::Up => {
                            let filtered = editor.filtered_logo_indices();
                            if let Some(pos) =
                                filtered.iter().position(|&i| i == editor.ascii_selected)
                            {
                                if pos > 0 {
                                    editor.select_logo_at(filtered[pos - 1]);
                                }
                            } else if !filtered.is_empty() {
                                editor.select_logo_at(filtered[filtered.len() - 1]);
                            }
                        }
                        KeyCode::Down => {
                            let filtered = editor.filtered_logo_indices();
                            if let Some(pos) =
                                filtered.iter().position(|&i| i == editor.ascii_selected)
                            {
                                if pos + 1 < filtered.len() {
                                    editor.select_logo_at(filtered[pos + 1]);
                                }
                            } else if !filtered.is_empty() {
                                editor.select_logo_at(filtered[0]);
                            }
                        }
                        KeyCode::Char(ch)
                            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' =>
                        {
                            editor.ascii_search.push(ch);
                            editor.jump_to_first_matching_logo();
                            editor.dirty = true;
                        }
                        _ => {}
                    }
                    return Ok(true);
                }
                InputMode::Normal => {}
            }

            // Normal mode key handling
            match key.code {
                KeyCode::Char('q') => {
                    return Ok(request_quit(editor));
                }
                KeyCode::Enter | KeyCode::Tab => {
                    editor.tab = editor.tab.next();
                }
                KeyCode::BackTab => {
                    editor.tab = editor.tab.prev();
                }
                KeyCode::Left => {
                    if editor.tab == Tab::Mode {
                        editor.scene_focus = false;
                    }
                    if editor.tab == Tab::Panels {
                        editor.panel_focus = false;
                    }
                }
                KeyCode::Right => {
                    if editor.tab == Tab::Mode {
                        editor.scene_focus = true;
                    }
                    if editor.tab == Tab::Panels {
                        editor.panel_focus = true;
                    }
                }
                KeyCode::Up => match editor.tab {
                    Tab::Mode => 'mode_up: {
                        if editor.monitor_mode {
                            break 'mode_up;
                        }
                        if editor.scene_focus {
                            editor.layout_selected = editor.layout_selected.saturating_sub(1);
                            editor.apply_layout(editor.layout_selected);
                        } else {
                            let scenes = component::Scene::all();
                            if editor.scene_selected > 0 {
                                editor.scene_selected -= 1;
                                editor.cfg.scene = scenes[editor.scene_selected];
                                editor.dirty = true;
                            }
                        }
                    }
                    Tab::Theme => {
                        editor.theme_selected = editor.theme_selected.saturating_sub(1);
                        if editor.theme_selected < editor.themes.len() {
                            editor.cfg.logo.colors =
                                editor.themes[editor.theme_selected].colors.clone();
                            editor.dirty = true;
                        }
                    }
                    Tab::Ascii => {
                        let filtered = editor.filtered_logo_indices();
                        let n = editor.logo_keys.len();
                        if let Some(pos) = filtered.iter().position(|&i| i == editor.ascii_selected)
                        {
                            if pos > 0 {
                                editor.select_logo_at(filtered[pos - 1]);
                            }
                        } else if !filtered.is_empty() {
                            editor.select_logo_at(filtered[filtered.len() - 1]);
                        } else if editor.ascii_selected >= n && editor.ascii_selected != n + 2 {
                            if !filtered.is_empty() {
                                editor.select_logo_at(filtered[filtered.len() - 1]);
                            } else if n > 0 {
                                editor.select_logo_at(0);
                            }
                        }
                    }
                    Tab::Panels => {
                        if editor.panel_focus {
                            editor.panel_right_sel = editor.panel_right_sel.saturating_sub(1);
                        } else {
                            editor.panel_left_sel = editor.panel_left_sel.saturating_sub(1);
                        }
                    }
                    _ => {}
                },
                KeyCode::Down => match editor.tab {
                    Tab::Mode => 'mode_down: {
                        if editor.monitor_mode {
                            break 'mode_down;
                        }
                        if editor.scene_focus {
                            let max = AppLayout::pc_variants().len().saturating_sub(1);
                            editor.layout_selected = (editor.layout_selected + 1).min(max);
                            editor.apply_layout(editor.layout_selected);
                        } else {
                            let scenes = component::Scene::all();
                            if editor.scene_selected + 1 < scenes.len() {
                                editor.scene_selected += 1;
                                editor.cfg.scene = scenes[editor.scene_selected];
                                editor.dirty = true;
                            }
                        }
                    }
                    Tab::Theme => {
                        let max = editor.themes.len().saturating_sub(1);
                        editor.theme_selected = (editor.theme_selected + 1).min(max);
                        if editor.theme_selected < editor.themes.len() {
                            editor.cfg.logo.colors =
                                editor.themes[editor.theme_selected].colors.clone();
                            editor.dirty = true;
                        }
                    }
                    Tab::Ascii => {
                        let filtered = editor.filtered_logo_indices();
                        let n = editor.logo_keys.len();
                        if let Some(pos) = filtered.iter().position(|&i| i == editor.ascii_selected)
                        {
                            if pos + 1 < filtered.len() {
                                editor.select_logo_at(filtered[pos + 1]);
                            } else {
                                if editor.ascii_selected < n {
                                    editor.ascii_selected = n;
                                    editor.dirty = true;
                                } else if editor.ascii_selected == n {
                                    editor.ascii_selected = n + 1;
                                    editor.dirty = true;
                                } else if editor.ascii_selected == n + 1 {
                                    editor.ascii_selected = n + 2;
                                    editor.ascii_source = "disabled".into();
                                    editor.cfg.logo.key = String::new();
                                    editor.cfg.logo.path = "disabled".into();
                                    editor.ascii_art = String::new();
                                    editor.dirty = true;
                                }
                            }
                        } else if !filtered.is_empty() {
                            editor.select_logo_at(filtered[0]);
                        }
                    }
                    Tab::Panels => {
                        let max = if editor.panel_focus {
                            editor.cfg.display.right.len().saturating_sub(1)
                        } else {
                            editor.cfg.display.left.len().saturating_sub(1)
                        };
                        if editor.panel_focus {
                            editor.panel_right_sel = (editor.panel_right_sel + 1).min(max);
                        } else {
                            editor.panel_left_sel = (editor.panel_left_sel + 1).min(max);
                        }
                    }
                    _ => {}
                },
                KeyCode::Char(' ') => {
                    if editor.tab == Tab::Panels {
                        let fields = if editor.panel_focus {
                            &mut editor.cfg.display.right
                        } else {
                            &mut editor.cfg.display.left
                        };
                        let idx = if editor.panel_focus {
                            editor.panel_right_sel
                        } else {
                            editor.panel_left_sel
                        };
                        if idx < fields.len() {
                            fields[idx].enabled = !fields[idx].enabled;
                            editor.dirty = true;
                        }
                    }
                }
                KeyCode::Char('m') => {
                    if editor.tab == Tab::Mode {
                        editor.monitor_mode = !editor.monitor_mode;
                        editor.cfg.live.enabled = editor.monitor_mode;
                        editor.dirty = true;
                    }
                }
                KeyCode::Char('/') => {
                    if editor.tab == Tab::Panels {
                        editor.panel_focus = !editor.panel_focus;
                    }
                    if editor.tab == Tab::Ascii {
                        editor.input_mode = InputMode::SearchingAscii;
                    }
                }
                KeyCode::Char('a') => {
                    if editor.tab == Tab::Panels {
                        editor.input_mode = InputMode::AddingPanel;
                        editor.add_panel_sel = 0;
                    }
                }
                KeyCode::Char('d') => {
                    if editor.tab == Tab::Panels {
                        let fields = if editor.panel_focus {
                            &mut editor.cfg.display.right
                        } else {
                            &mut editor.cfg.display.left
                        };
                        let idx = if editor.panel_focus {
                            editor.panel_right_sel
                        } else {
                            editor.panel_left_sel
                        };
                        if idx < fields.len() {
                            fields.remove(idx);
                            editor.dirty = true;
                        }
                    }
                    if editor.tab == Tab::Ascii {
                        editor.ascii_source = "disabled".into();
                        editor.cfg.logo.key = String::new();
                        editor.cfg.logo.path = "disabled".into();
                        editor.ascii_art = String::new();
                        let n = editor.logo_keys.len();
                        editor.ascii_selected = n + 2;
                        editor.dirty = true;
                    }
                }
                KeyCode::Char('s') => {
                    if editor.tab == Tab::Save {
                        editor.saved = true;
                        editor.changed = false;
                        editor.status_message = "Saved".into();
                        return Ok(false);
                    }
                }
                KeyCode::Char('r') => {
                    if editor.tab == Tab::Panels {
                        let fields = if editor.panel_focus {
                            &mut editor.cfg.display.right
                        } else {
                            &mut editor.cfg.display.left
                        };
                        let idx = if editor.panel_focus {
                            editor.panel_right_sel
                        } else {
                            editor.panel_left_sel
                        };
                        let new_idx = move_up(fields, idx);
                        if new_idx != idx {
                            if editor.panel_focus {
                                editor.panel_right_sel = new_idx;
                            } else {
                                editor.panel_left_sel = new_idx;
                            }
                            editor.dirty = true;
                        }
                    }
                }
                KeyCode::Char('e') => {
                    if editor.tab == Tab::Panels {
                        let fields = if editor.panel_focus {
                            &editor.cfg.display.right
                        } else {
                            &editor.cfg.display.left
                        };
                        let idx = if editor.panel_focus {
                            editor.panel_right_sel
                        } else {
                            editor.panel_left_sel
                        };
                        if idx < fields.len() {
                            editor.editing_label_input = fields[idx].label.clone();
                            editor.input_mode = InputMode::EditingLabel;
                        }
                    }
                }
                KeyCode::Char('v') => {
                    if editor.tab == Tab::Theme {
                        editor.cfg.logo.color_dir = if editor.cfg.logo.color_dir == "vertical" {
                            "horizontal".into()
                        } else {
                            "vertical".into()
                        };
                        editor.dirty = true;
                    }
                }
                KeyCode::Char('c') => {
                    if editor.tab == Tab::Theme {
                        editor.custom_palette_input.clear();
                        editor.input_mode = InputMode::EditingCustomPalette;
                    }
                    if editor.tab == Tab::Ascii {
                        editor.input_mode = InputMode::BrowsingFile;
                        editor.file_browser_sel = 0;
                        editor.refresh_file_browser();
                    }
                }
                KeyCode::Char('p') => {
                    if editor.tab == Tab::Ascii {
                        editor.input_mode = InputMode::PastingAscii;
                    }
                }
                KeyCode::Char('?') => {
                    editor.input_mode = InputMode::Help;
                    editor.status_message = "Help".into();
                }
                KeyCode::Esc => {
                    return Ok(request_quit(editor));
                }
                _ => {}
            }
        }
        _ => {}
    }
    if editor.dirty {
        editor.refresh_preview();
    }
    Ok(true)
}

fn request_quit(editor: &mut Editor) -> bool {
    if editor.changed {
        editor.input_mode = InputMode::ConfirmQuit;
        editor.status_message = "Unsaved changes — confirm before quitting".into();
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::move_up;

    #[test]
    fn panel_reordering_moves_the_selected_item_and_cursor() {
        let mut fields = ["os", "cpu", "memory"];
        assert_eq!(move_up(&mut fields, 2), 1);
        assert_eq!(fields, ["os", "memory", "cpu"]);
        assert_eq!(move_up(&mut fields, 0), 0);
    }
}
