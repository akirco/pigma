/// Overlay state for the `?` help popup listing all keyboard shortcuts.
#[derive(Debug, Clone, Copy, Default)]
pub struct HelpState {
    pub open: bool,
    pub scroll: usize,
    /// Scroll limit of the last rendered popup; refreshed on every draw so
    /// `scroll_down` can clamp at the source instead of letting `scroll`
    /// overflow past the bottom (which would force the user to scroll up
    /// through the surplus before the view moves).
    pub max_scroll: usize,
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
        self.scroll = (self.scroll + 1).min(self.max_scroll);
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_down_stops_at_max_scroll_and_scroll_up_recovers_immediately() {
        let mut help = HelpState {
            max_scroll: 3,
            ..Default::default()
        };

        for _ in 0..10 {
            help.scroll_down();
        }
        assert_eq!(help.scroll, 3);

        help.scroll_up();
        assert_eq!(help.scroll, 2);
    }

    #[test]
    fn scroll_up_never_goes_below_zero() {
        let mut help = HelpState {
            max_scroll: 3,
            ..Default::default()
        };

        help.scroll_up();
        assert_eq!(help.scroll, 0);

        help.scroll_down();
        help.scroll_up();
        help.scroll_up();
        assert_eq!(help.scroll, 0);
    }
}
