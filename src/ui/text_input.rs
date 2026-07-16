use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
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

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
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

    pub fn cursor(&self) -> usize {
        self.cursor
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

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    pub fn show_cursor(&self, f: &mut Frame, area: Rect, focused: bool, password: bool) {
        if !focused {
            return;
        }
        let x = area.x + 2 + self.cursor_width(password);
        f.set_cursor_position((x, area.y));
    }

    pub fn show_cursor_at(&self, f: &mut Frame, x: u16, y: u16, focused: bool, password: bool) {
        if !focused {
            return;
        }
        f.set_cursor_position((x + self.cursor_width(password), y));
    }

    pub fn render(
        &self,
        f: &mut Frame,
        area: Rect,
        colors: &crate::theme::Theme,
        focused: bool,
        password: bool,
    ) {
        let border_color = if focused { colors.accent } else { colors.muted };

        let display = if password {
            "*".repeat(self.value.chars().count())
        } else {
            self.value.clone()
        };

        let text_color = if focused { colors.text } else { colors.muted };

        let input_line = Line::from(vec![
            Span::styled(
                "❯ ",
                Style::default()
                    .fg(colors.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(display, Style::default().fg(text_color)),
        ]);

        let block = ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::BOTTOM)
            .border_style(Style::default().fg(border_color));

        let paragraph = Paragraph::new(input_line).block(block);
        f.render_widget(paragraph, area);

        self.show_cursor(f, area, focused, password);
    }
}
