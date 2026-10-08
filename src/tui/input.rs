//! Single-line text input used by every text popup and text row of the editor.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use unicode_width::UnicodeWidthChar;

/// What a key press did to a text input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputResult {
    Continue,
    Submit,
    Cancel,
}

/// Editable text with a cursor measured in characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextInput {
    value: String,
    /// Cursor position as a character index, `0..=value.chars().count()`.
    cursor: usize,
    max_chars: usize,
}

impl TextInput {
    pub fn new(initial: &str, max_chars: usize) -> Self {
        let mut input = TextInput {
            value: String::new(),
            cursor: 0,
            max_chars,
        };
        input.set(initial);
        input
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn len_chars(&self) -> usize {
        self.value.chars().count()
    }

    /// Replace the value and move the cursor to the end. Longer text is cut to `max_chars`.
    pub fn set(&mut self, text: &str) {
        self.value = text.chars().take(self.max_chars).collect();
        self.cursor = self.len_chars();
    }

    /// Display width of the text before the cursor, for placing the terminal cursor.
    pub fn cursor_column(&self) -> usize {
        self.value
            .chars()
            .take(self.cursor)
            .map(|ch| ch.width().unwrap_or(0))
            .sum()
    }

    /// Insert pasted text at the cursor. Line breaks become spaces; the length limit applies.
    pub fn insert_str(&mut self, text: &str) {
        for ch in text.chars() {
            let ch = if ch == '\n' || ch == '\r' || ch == '\t' {
                ' '
            } else {
                ch
            };
            if !ch.is_control() {
                self.insert_char(ch);
            }
        }
    }

    pub fn handle(&mut self, key: KeyEvent) -> InputResult {
        if key.kind == KeyEventKind::Release {
            return InputResult::Continue;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Enter => return InputResult::Submit,
            KeyCode::Esc => return InputResult::Cancel,
            KeyCode::Char('u') if ctrl => self.clear(),
            KeyCode::Char(ch) if !ctrl && !ch.is_control() => self.insert_char(ch),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.len_chars()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.len_chars(),
            _ => {}
        }
        InputResult::Continue
    }

    fn insert_char(&mut self, ch: char) {
        if self.len_chars() >= self.max_chars {
            return;
        }
        let byte = self.byte_index(self.cursor);
        self.value.insert(byte, ch);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        let byte = self.byte_index(self.cursor);
        self.value.remove(byte);
    }

    fn delete(&mut self) {
        if self.cursor >= self.len_chars() {
            return;
        }
        let byte = self.byte_index(self.cursor);
        self.value.remove(byte);
    }

    fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    fn byte_index(&self, char_index: usize) -> usize {
        self.value
            .char_indices()
            .nth(char_index)
            .map_or(self.value.len(), |(byte, _)| byte)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventState;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn ctrl(ch: char) -> KeyEvent {
        KeyEvent {
            modifiers: KeyModifiers::CONTROL,
            ..key(KeyCode::Char(ch))
        }
    }

    fn type_text(input: &mut TextInput, text: &str) {
        for ch in text.chars() {
            input.handle(key(KeyCode::Char(ch)));
        }
    }

    #[test]
    fn typing_inserts_at_cursor() {
        let mut input = TextInput::new("", 16);
        type_text(&mut input, "hllo");
        input.handle(key(KeyCode::Home));
        input.handle(key(KeyCode::Right));
        type_text(&mut input, "e");
        assert_eq!(input.value(), "hello");
        assert_eq!(input.cursor_column(), 2);
    }

    #[test]
    fn max_chars_is_respected_by_typing_and_set() {
        let mut input = TextInput::new("", 3);
        type_text(&mut input, "abcdef");
        assert_eq!(input.value(), "abc");
        input.set("wxyz");
        assert_eq!(input.value(), "wxy");
    }

    #[test]
    fn backspace_and_delete_work_on_multibyte_text() {
        let mut input = TextInput::new("aé€b", 10);
        input.handle(key(KeyCode::Left));
        input.handle(key(KeyCode::Backspace));
        assert_eq!(input.value(), "aéb");
        input.handle(key(KeyCode::Home));
        input.handle(key(KeyCode::Delete));
        assert_eq!(input.value(), "éb");
    }

    #[test]
    fn ctrl_u_clears_and_enter_esc_signal_results() {
        let mut input = TextInput::new("hello", 10);
        assert_eq!(input.handle(ctrl('u')), InputResult::Continue);
        assert_eq!(input.value(), "");
        assert_eq!(input.handle(key(KeyCode::Enter)), InputResult::Submit);
        assert_eq!(input.handle(key(KeyCode::Esc)), InputResult::Cancel);
    }

    #[test]
    fn paste_turns_newlines_into_spaces_and_respects_limit() {
        let mut input = TextInput::new("", 8);
        input.insert_str("a\nb\tc\u{7}d efgh");
        assert_eq!(input.value(), "a b cd e");
    }

    #[test]
    fn cursor_column_counts_wide_characters_by_width() {
        let input = TextInput::new("日本", 10);
        assert_eq!(input.cursor_column(), 4);
    }

    #[test]
    fn release_events_are_ignored() {
        let mut input = TextInput::new("", 10);
        let release = KeyEvent {
            kind: KeyEventKind::Release,
            ..key(KeyCode::Char('x'))
        };
        input.handle(release);
        assert_eq!(input.value(), "");
    }
}
