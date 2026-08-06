use std::env;

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
        Ok("iterm.app")
            if version_gte(
                &env::var("TERM_PROGRAM_VERSION").unwrap_or_default(),
                3,
                4,
                0,
            ) =>
        {
            return true;
        }
        Ok("konsole")
            if env::var("KONSOLE_VERSION")
                .unwrap_or_default()
                .parse::<u32>()
                .unwrap_or(0)
                >= 220400 =>
        {
            return true;
        }
        _ => {}
    }
    matches!(env::var("TERM").as_deref(), Ok(t) if t.to_lowercase().contains("kitty") || t == "xterm-ghostty")
}

fn sixel_available() -> bool {
    if env::var("FOOT_VERSION").is_ok() {
        return true;
    }
    match env::var("TERM_PROGRAM").as_deref() {
        Ok("vscode")
            if version_gte(
                &env::var("TERM_PROGRAM_VERSION").unwrap_or_default(),
                1,
                80,
                0,
            ) =>
        {
            return true;
        }
        Ok("rio")
            if version_gte(
                &env::var("TERM_PROGRAM_VERSION").unwrap_or_default(),
                12,
                0,
                0,
            ) =>
        {
            return true;
        }
        Ok("mintty") => return true,
        Ok("WezTerm")
            if wezterm_sixel_supported(&env::var("WEZTERM_VERSION").unwrap_or_default()) =>
        {
            return true;
        }
        Ok("konsole")
            if env::var("KONSOLE_VERSION")
                .unwrap_or_default()
                .parse::<u32>()
                .unwrap_or(0)
                >= 220400 =>
        {
            return true;
        }
        // Windows Terminal added sixel support in v1.22.
        Ok("WindowsTerminal")
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
    matches!(env::var("TERM").as_deref(), Ok(t) if t.to_lowercase().starts_with("foot") || t.to_lowercase().starts_with("mlterm"))
}

fn version_gte(version_str: &str, major: u32, minor: u32, patch: u32) -> bool {
    let parts: Vec<u32> = version_str
        .split('.')
        .filter_map(|s| s.parse().ok())
        .collect();
    let v_major = parts.first().copied().unwrap_or(0);
    let v_minor = parts.get(1).copied().unwrap_or(0);
    let v_patch = parts.get(2).copied().unwrap_or(0);
    (v_major, v_minor, v_patch) >= (major, minor, patch)
}

fn wezterm_sixel_supported(version: &str) -> bool {
    let parts: Vec<u32> = version.split('.').filter_map(|s| s.parse().ok()).collect();
    let year = parts.first().copied().unwrap_or(0);
    let month = parts.get(1).copied().unwrap_or(0);
    (year, month) >= (2022, 6)
}
