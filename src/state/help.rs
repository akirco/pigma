/// Overlay state for the `?` help popup listing all keyboard shortcuts.
#[derive(Debug, Clone, Copy, Default)]
pub struct HelpState {
    pub open: bool,
    pub scroll: usize,
}

impl HelpState {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        if !self.open {
            self.scroll = 0;
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.scroll = 0;
    }

    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }
}
