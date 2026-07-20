#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandAction {
    ToggleBordered,
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

    pub fn current_items(&self) -> Option<&Vec<CommandItem>> {
        self.levels.last()
    }

    pub fn current_title(&self) -> &str {
        if self.levels.len() > 1 {
            "THEMES"
        } else {
            "COMMANDS"
        }
    }

    pub fn enter(&mut self) -> Option<CommandAction> {
        let item = self.current_items()?[self.selected].clone();
        match item {
            CommandItem::Action { action, .. } => Some(action),
            CommandItem::SubMenu { children, .. } => {
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
