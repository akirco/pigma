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
    pub muted: Color,
    pub border: Color,
    pub error: Color,
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
            muted: cstr("#555555"),
            border: cstr("#555555"),
            error: cstr("#f4535a"),
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
            muted: Color::Indexed(4),
            border: Color::Indexed(4),
            error: Color::Indexed(9),
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
            muted: cstr("#6272a4"),
            border: cstr("#6272a4"),
            error: cstr("#ff5555"),
        }
    }

    pub fn nord() -> Self {
        Self {
            name: "nord".to_string(),
            bg: cstr("#2e3440"),
            surface: cstr("#3b4252"),
            text: cstr("#eceff4"),
            accent: cstr("#88c0d0"),
            muted: cstr("#616e88"),
            border: cstr("#616e88"),
            error: cstr("#bf616a"),
        }
    }

    pub fn gruvbox_dark() -> Self {
        Self {
            name: "gruvbox".to_string(),
            bg: cstr("#282828"),
            surface: cstr("#3c3836"),
            text: cstr("#ebdbb2"),
            accent: cstr("#d65d0e"),
            muted: cstr("#928374"),
            border: cstr("#928374"),
            error: cstr("#fb4934"),
        }
    }

    pub fn solarized_dark() -> Self {
        Self {
            name: "solarized".to_string(),
            bg: cstr("#002b36"),
            surface: cstr("#073642"),
            text: cstr("#839496"),
            accent: cstr("#268bd2"),
            muted: cstr("#586e75"),
            border: cstr("#586e75"),
            error: cstr("#dc322f"),
        }
    }

    pub fn tokyo_night() -> Self {
        Self {
            name: "tokyo-night".to_string(),
            bg: cstr("#1a1b26"),
            surface: cstr("#24283b"),
            text: cstr("#c0caf5"),
            accent: cstr("#7aa2f7"),
            muted: cstr("#565f89"),
            border: cstr("#565f89"),
            error: cstr("#f7768e"),
        }
    }

    pub fn catppuccin_mocha() -> Self {
        Self {
            name: "catppuccin".to_string(),
            bg: cstr("#1e1e2e"),
            surface: cstr("#313244"),
            text: cstr("#cdd6f4"),
            accent: cstr("#cba6f7"),
            muted: cstr("#6c7086"),
            border: cstr("#6c7086"),
            error: cstr("#f38ba8"),
        }
    }

    pub fn one_dark() -> Self {
        Self {
            name: "one-dark".to_string(),
            bg: cstr("#282c34"),
            surface: cstr("#3e4451"),
            text: cstr("#abb2bf"),
            accent: cstr("#61afef"),
            muted: cstr("#5c6370"),
            border: cstr("#5c6370"),
            error: cstr("#e06c75"),
        }
    }

    pub fn monokai() -> Self {
        Self {
            name: "monokai".to_string(),
            bg: cstr("#272822"),
            surface: cstr("#3e3d32"),
            text: cstr("#f8f8f2"),
            accent: cstr("#f92672"),
            muted: cstr("#75715e"),
            border: cstr("#75715e"),
            error: cstr("#f92672"),
        }
    }

    pub fn rose_pine() -> Self {
        Self {
            name: "rose-pine".to_string(),
            bg: cstr("#191724"),
            surface: cstr("#26233a"),
            text: cstr("#e0def4"),
            accent: cstr("#eb6f92"),
            muted: cstr("#6e6a86"),
            border: cstr("#6e6a86"),
            error: cstr("#eb6f92"),
        }
    }

    pub fn kanagawa() -> Self {
        Self {
            name: "kanagawa".to_string(),
            bg: cstr("#1f1f28"),
            surface: cstr("#2a2a37"),
            text: cstr("#dcd7ba"),
            accent: cstr("#7e9cd8"),
            muted: cstr("#727169"),
            border: cstr("#727169"),
            error: cstr("#c34043"),
        }
    }

    //light theme
    pub fn solarized_light() -> Self {
        Self {
            name: "solarized-light".to_string(),
            bg: cstr("#fdf6e3"),
            surface: cstr("#eee8d5"),
            text: cstr("#5d8796"),
            accent: cstr("#268bd2"),
            muted: cstr("#93a1a1"),
            border: cstr("#93a1a1"),
            error: cstr("#dc322f"),
        }
    }

    pub fn catppuccin_latte() -> Self {
        Self {
            name: "catppuccin-latte".to_string(),
            bg: cstr("#eff1f5"),
            surface: cstr("#e6e9ef"),
            text: cstr("#4c4f69"),
            accent: cstr("#7287fd"),
            muted: cstr("#9ca0b0"),
            border: cstr("#9ca0b0"),
            error: cstr("#d20f39"),
        }
    }

    pub fn one_light() -> Self {
        Self {
            name: "one-light".to_string(),
            bg: cstr("#fafafa"),
            surface: cstr("#f0f0f0"),
            text: cstr("#383a42"),
            accent: cstr("#4078f2"),
            muted: cstr("#a0a1a7"),
            border: cstr("#a0a1a7"),
            error: cstr("#e45649"),
        }
    }

    pub fn github_light() -> Self {
        Self {
            name: "github-light".to_string(),
            bg: cstr("#ffffff"),
            surface: cstr("#f6f8fa"),
            text: cstr("#24292f"),
            accent: cstr("#0969da"),
            muted: cstr("#57606a"),
            border: cstr("#57606a"),
            error: cstr("#cf222e"),
        }
    }

    pub fn gruvbox_light() -> Self {
        Self {
            name: "gruvbox-light".to_string(),
            bg: cstr("#fbf1c7"),
            surface: cstr("#ebdbb2"),
            text: cstr("#3c3836"),
            accent: cstr("#d65d0e"),
            muted: cstr("#928374"),
            border: cstr("#928374"),
            error: cstr("#cc241d"),
        }
    }

    pub fn cyberpunk_hot() -> Self {
        Self {
            name: "cyberpunk-hot".to_string(),
            bg: cstr("#0a0b10"),
            surface: cstr("#1a1a2e"),
            text: cstr("#ff0066"),
            accent: cstr("#ffcc00"),
            muted: cstr("#00f0ff"),
            border: cstr("0"),
            error: cstr("#b000ff"),
        }
    }

    pub fn cyberpunk_fury() -> Self {
        Self {
            name: "cyberpunk-fury".to_string(),
            bg: cstr("#0d0a14"),
            surface: cstr("#2a1a3a"),
            text: cstr("#ffdd00"),
            accent: cstr("#ff00aa"),
            muted: cstr("#00ccff"),
            border: cstr("0"),
            error: cstr("#ff3300"),
        }
    }

    pub fn cyberpunk_volt() -> Self {
        Self {
            name: "cyberpunk-volt".to_string(),
            bg: cstr("#080c14"),
            surface: cstr("#111827"),
            text: cstr("#00f0ff"),
            accent: cstr("#ff0066"),
            muted: cstr("#ccff00"),
            border: cstr("0"),
            error: cstr("#00ff66"),
        }
    }

    /// Look up a theme color field by name (e.g. "accent", "muted", "error", "border").
    pub fn field_color(&self, name: &str) -> Color {
        match name {
            "bg" => self.bg,
            "surface" => self.surface,
            "text" => self.text,
            "accent" => self.accent,
            "muted" => self.muted,
            "border" => self.border,
            "error" => self.error,
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
            Theme::github_light(),
            Theme::gruvbox_light(),
            Theme::one_light(),
            Theme::catppuccin_latte(),
            Theme::solarized_light(),
            Theme::cyberpunk_volt(),
            Theme::cyberpunk_fury(),
            Theme::cyberpunk_hot(),
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
