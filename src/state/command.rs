#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandAction {
    ToggleBordered,
    ToggleSaveOnPlay,
    CycleNavPosition,
    SwitchTheme(String),
}

#[derive(Debug, Clone)]
pub enum CommandItem {
    Action {
        name: String,
        action: CommandAction,
    },
    SubMenu {
        name: String,
        children: Vec<CommandItem>,
    },
}

pub struct CommandPanel {
    pub open: bool,
    pub selected: usize,
    pub levels: Vec<Vec<CommandItem>>,
}

impl Default for CommandPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandPanel {
    pub fn new() -> Self {
        Self {
            open: false,
            selected: 0,
            levels: Vec::new(),
        }
    }

    pub fn current_items(&self) -> Option<&[CommandItem]> {
        self.levels.last().map(Vec::as_slice)
    }

    /// The event loop has no periodic tick, so the panel is redrawn on every
    /// event: keep the two possible titles cached instead of allocating a
    /// `String` per frame.
    pub fn current_title(&self) -> &'static str {
        static COMMANDS: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
            format!(
                "{} COMMANDS {}",
                crate::icons::TITLE_START,
                crate::icons::TITLE_END
            )
        });
        static THEMES: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
            format!(
                "{} THEMES {}",
                crate::icons::TITLE_START,
                crate::icons::TITLE_END
            )
        });
        if self.levels.len() > 1 {
            THEMES.as_str()
        } else {
            COMMANDS.as_str()
        }
    }

    pub fn enter(&mut self) -> Option<CommandAction> {
        let item = self.current_items()?.get(self.selected)?;
        match item {
            CommandItem::Action { action, .. } => Some(action.clone()),
            CommandItem::SubMenu { children, .. } => {
                let children = children.clone();
                self.selected = 0;
                self.levels.push(children);
                None
            }
        }
    }

    pub fn back(&mut self) {
        if self.levels.len() > 1 {
            self.levels.pop();
            self.selected = 0;
        } else {
            self.open = false;
        }
    }
}
