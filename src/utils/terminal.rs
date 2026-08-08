use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProtocol {
    Kitty,
    Sixel,
}

pub fn best_image_protocol() -> Option<ImageProtocol> {
    if kitty_available() {
        Some(ImageProtocol::Kitty)
    } else if sixel_available() {
        Some(ImageProtocol::Sixel)
    } else {
        None
    }
}

fn kitty_available() -> bool {
    if env::var("KITTY_WINDOW_ID").is_ok()
        || env::var("KITTY_PID").is_ok()
        || env::var("GHOSTTY_RESOURCES_DIR").is_ok()
    {
        return true;
    }

    match env::var("TERM_PROGRAM").as_deref() {
        Ok("kitty" | "ghostty" | "rio" | "WezTerm") => return true,
        Ok("iterm.app") => {
            if version_gte(
                &env::var("TERM_PROGRAM_VERSION").unwrap_or_default(),
                3,
                5,
                0,
            ) {
                return true;
            }
        }
        Ok("konsole") if is_konsole_version_gte(22, 4, 0) => {
            return true;
        }
        _ => {}
    }

    matches!(
        env::var("TERM").as_deref(),
        Ok(t) if t.to_lowercase().contains("kitty") || t == "xterm-ghostty"
    )
}

fn sixel_available() -> bool {
    if env::var("FOOT_VERSION").is_ok() {
        return true;
    }

    if env::var("WT_SESSION").is_ok() {
        return true;
    }

    match env::var("TERM_PROGRAM").as_deref() {
        Ok("vscode") => {
            if version_gte(
                &env::var("TERM_PROGRAM_VERSION").unwrap_or_default(),
                1,
                80,
                0,
            ) {
                return true;
            }
        }
        Ok("rio") => {
            // Rio 在 0.0.12 后开始较好地支持图形协议
            if version_gte(
                &env::var("TERM_PROGRAM_VERSION").unwrap_or_default(),
                0,
                0,
                12,
            ) {
                return true;
            }
        }
        Ok("mintty") => return true,
        Ok("WezTerm") => {
            if wezterm_sixel_supported(&env::var("WEZTERM_VERSION").unwrap_or_default()) {
                return true;
            }
        }
        Ok("konsole") => {
            if is_konsole_version_gte(22, 4, 0) {
                return true;
            }
        }
        Ok("WindowsTerminal" | "Windows_Terminal")
            if version_gte(
                &env::var("TERM_PROGRAM_VERSION").unwrap_or_default(),
                1,
                22,
                0,
            ) =>
        {
            return true;
        }
        _ => {}
    }

    matches!(
        env::var("TERM").as_deref(),
        Ok(t) if t.to_lowercase().starts_with("foot") || t.to_lowercase().starts_with("mlterm")
    )
}

fn version_gte(version_str: &str, major: u32, minor: u32, patch: u32) -> bool {
    let parts: Vec<u32> = version_str
        .split('.')
        .map(|s| {
            s.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
        })
        .filter_map(|s| s.parse().ok())
        .collect();

    let v_major = parts.first().copied().unwrap_or(0);
    let v_minor = parts.get(1).copied().unwrap_or(0);
    let v_patch = parts.get(2).copied().unwrap_or(0);

    (v_major, v_minor, v_patch) >= (major, minor, patch)
}

fn wezterm_sixel_supported(version: &str) -> bool {
    if let Some(date_part) = version.split('-').next()
        && let Ok(date_num) = date_part.parse::<u32>()
    {
        return date_num >= 20220600;
    }
    false
}

fn is_konsole_version_gte(major: u32, minor: u32, patch: u32) -> bool {
    let ver_str = env::var("KONSOLE_VERSION").unwrap_or_default();
    if ver_str.contains('.') {
        version_gte(&ver_str, major, minor, patch)
    } else if let Ok(num) = ver_str.parse::<u32>() {
        let target = major * 10000 + minor * 100 + patch;
        num >= target
    } else {
        false
    }
}
