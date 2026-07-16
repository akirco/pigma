use std::collections::HashMap;
use std::str::FromStr;
use std::sync::OnceLock;

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub name: String,
    pub bg: Color,
    pub surface: Color,
    pub text: Color,
    pub accent: Color,
    pub highlight: Color,
    pub muted: Color,
    pub error: Color,
    pub warning: Color,
}

pub fn cstr(s: &str) -> Color {
    Color::from_str(s).unwrap_or_else(|e| {
        log::warn!("Invalid color '{}' in theme, using fallback: {}", s, e);
        Color::Reset
    })
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            bg: cstr("#0e0e0e"),
            surface: cstr("#160e12"),
            text: cstr("#ffffff"),
            accent: cstr("#c20c0c"),
            highlight: cstr("#4ade80"),
            muted: cstr("#555555"),
            error: cstr("#f4535a"),
            warning: cstr("#587fa6"),
        }
    }
}

impl Theme {
    pub fn terminal() -> Self {
        Self {
            name: "terminal".to_string(),
            bg: Color::Indexed(0),
            text: Color::Indexed(15),
            accent: Color::Indexed(1),
            highlight: Color::Indexed(5),
            muted: Color::Indexed(4),
            error: Color::Indexed(9),
            warning: Color::Indexed(3),
            surface: Color::Indexed(8),
        }
    }

    pub fn dracula() -> Self {
        Self {
            name: "dracula".to_string(),
            bg: cstr("#282a36"),
            surface: cstr("#44475a"),
            text: cstr("#f8f8f2"),
            accent: cstr("#bd93f9"),
            highlight: cstr("#50fa7b"),
            muted: cstr("#6272a4"),
            error: cstr("#ff5555"),
            warning: cstr("#f1fa8c"),
        }
    }

    pub fn nord() -> Self {
        Self {
            name: "nord".to_string(),
            bg: cstr("#2e3440"),
            surface: cstr("#3b4252"),
            text: cstr("#eceff4"),
            accent: cstr("#88c0d0"),
            highlight: cstr("#a3be8c"),
            muted: cstr("#616e88"),
            error: cstr("#bf616a"),
            warning: cstr("#ebcb8b"),
        }
    }

    pub fn gruvbox_dark() -> Self {
        Self {
            name: "gruvbox".to_string(),
            bg: cstr("#282828"),
            surface: cstr("#3c3836"),
            text: cstr("#ebdbb2"),
            accent: cstr("#d65d0e"),
            highlight: cstr("#b8bb26"),
            muted: cstr("#928374"),
            error: cstr("#fb4934"),
            warning: cstr("#fabd2f"),
        }
    }

    pub fn solarized_dark() -> Self {
        Self {
            name: "solarized".to_string(),
            bg: cstr("#002b36"),
            surface: cstr("#073642"),
            text: cstr("#839496"),
            accent: cstr("#268bd2"),
            highlight: cstr("#859900"),
            muted: cstr("#586e75"),
            error: cstr("#dc322f"),
            warning: cstr("#b58900"),
        }
    }

    pub fn tokyo_night() -> Self {
        Self {
            name: "tokyo-night".to_string(),
            bg: cstr("#1a1b26"),
            surface: cstr("#24283b"),
            text: cstr("#c0caf5"),
            accent: cstr("#7aa2f7"),
            highlight: cstr("#9ece6a"),
            muted: cstr("#565f89"),
            error: cstr("#f7768e"),
            warning: cstr("#e0af68"),
        }
    }

    pub fn catppuccin_mocha() -> Self {
        Self {
            name: "catppuccin".to_string(),
            bg: cstr("#1e1e2e"),
            surface: cstr("#313244"),
            text: cstr("#cdd6f4"),
            accent: cstr("#cba6f7"),
            highlight: cstr("#a6e3a1"),
            muted: cstr("#6c7086"),
            error: cstr("#f38ba8"),
            warning: cstr("#f9e2af"),
        }
    }

    pub fn one_dark() -> Self {
        Self {
            name: "one-dark".to_string(),
            bg: cstr("#282c34"),
            surface: cstr("#3e4451"),
            text: cstr("#abb2bf"),
            accent: cstr("#61afef"),
            highlight: cstr("#98c379"),
            muted: cstr("#5c6370"),
            error: cstr("#e06c75"),
            warning: cstr("#e5c07b"),
        }
    }

    pub fn monokai() -> Self {
        Self {
            name: "monokai".to_string(),
            bg: cstr("#272822"),
            surface: cstr("#3e3d32"),
            text: cstr("#f8f8f2"),
            accent: cstr("#f92672"),
            highlight: cstr("#a6e22e"),
            muted: cstr("#75715e"),
            error: cstr("#f92672"),
            warning: cstr("#e6db74"),
        }
    }

    pub fn rose_pine() -> Self {
        Self {
            name: "rose-pine".to_string(),
            bg: cstr("#191724"),
            surface: cstr("#26233a"),
            text: cstr("#e0def4"),
            accent: cstr("#eb6f92"),
            highlight: cstr("#31748f"),
            muted: cstr("#6e6a86"),
            error: cstr("#eb6f92"),
            warning: cstr("#f6c177"),
        }
    }

    pub fn kanagawa() -> Self {
        Self {
            name: "kanagawa".to_string(),
            bg: cstr("#1f1f28"),
            surface: cstr("#2a2a37"),
            text: cstr("#dcd7ba"),
            accent: cstr("#7e9cd8"),
            highlight: cstr("#98bb6c"),
            muted: cstr("#727169"),
            error: cstr("#c34043"),
            warning: cstr("#e6c384"),
        }
    }

    /// Look up a theme color field by name (e.g. "accent", "highlight", "muted").
    pub fn field_color(&self, name: &str) -> Color {
        match name {
            "bg" => self.bg,
            "surface" => self.surface,
            "text" => self.text,
            "accent" => self.accent,
            "highlight" => self.highlight,
            "muted" => self.muted,
            "error" => self.error,
            "warning" => self.warning,
            _ => {
                log::warn!("Unknown theme field: \"{name}\", falling back to accent");
                self.accent
            }
        }
    }
}

fn builtin_themes() -> &'static HashMap<String, Theme> {
    static THEMES: OnceLock<HashMap<String, Theme>> = OnceLock::new();
    THEMES.get_or_init(|| {
        let themes: Vec<Theme> = vec![
            Theme::default(),
            Theme::terminal(),
            Theme::dracula(),
            Theme::nord(),
            Theme::gruvbox_dark(),
            Theme::solarized_dark(),
            Theme::tokyo_night(),
            Theme::catppuccin_mocha(),
            Theme::one_dark(),
            Theme::monokai(),
            Theme::rose_pine(),
            Theme::kanagawa(),
        ];
        themes
            .into_iter()
            .map(|t| {
                let n = t.name.clone();
                (n, t)
            })
            .collect()
    })
}

pub struct ThemeRegistry {
    extras: HashMap<String, Theme>,
}

impl ThemeRegistry {
    pub fn new(extras: Vec<Theme>) -> Self {
        Self {
            extras: extras.into_iter().map(|t| (t.name.clone(), t)).collect(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&Theme> {
        self.extras.get(name).or_else(|| builtin_themes().get(name))
    }

    pub fn all_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.extras.keys().map(|s| s.as_str()).collect();
        for k in builtin_themes().keys() {
            if !self.extras.contains_key(k) {
                names.push(k);
            }
        }
        names
    }
}
