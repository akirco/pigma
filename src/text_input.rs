use ratatui::Frame;
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone)]
pub struct TextInput {
    pub value: String,
    cursor: usize,
}

impl Default for TextInput {
    fn default() -> Self {
        Self::new()
    }
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            value: String::new(),
            cursor: 0,
        }
    }

    fn byte_index(&self) -> usize {
        self.value
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.value.len())
    }

    pub fn enter_char(&mut self, ch: char) {
        let byte_idx = self.byte_index();
        self.value.insert(byte_idx, ch);
        self.cursor += 1;
    }

    pub fn delete_char(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let before = self.value.chars().take(self.cursor - 1);
        let after = self.value.chars().skip(self.cursor);
        self.value = before.chain(after).collect();
        self.cursor -= 1;
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.value.chars().count() {
            self.cursor += 1;
        }
    }

    pub fn cursor_width(&self, password: bool) -> u16 {
        if password {
            self.cursor as u16
        } else {
            self.value
                .chars()
                .take(self.cursor)
                .map(|c| UnicodeWidthChar::width(c).unwrap_or(1))
                .sum::<usize>() as u16
        }
    }

    pub fn show_cursor_at(&self, f: &mut Frame, x: u16, y: u16, focused: bool, password: bool) {
        if !focused {
            return;
        }
        f.set_cursor_position((x + self.cursor_width(password), y));
    }
}
