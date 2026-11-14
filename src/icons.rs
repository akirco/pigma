//! Terminal glyphs used by the UI.
//!
//! Keep icon codepoints here so changing a font or a single icon does not
//! require touching rendering logic. Nerd Font codepoints are written as
//! escapes with the rendered glyph in the comment.

// --- Playback controls (Nerd Font) ---
pub(crate) const CONTROL_PLAY: &str = "\u{f03e4}";
pub(crate) const CONTROL_PAUSE: &str = "\u{f040a}";
pub(crate) const CONTROL_PREV: &str = "\u{f049}";
pub(crate) const CONTROL_NEXT: &str = "\u{f050}";
pub(crate) const LIKE_FILLED: &str = "\u{f004}";
pub(crate) const LIKE_OUTLINE: &str = "\u{f08a}";
pub(crate) const VOLUME_MUTE: &str = "\u{f026}";
pub(crate) const VOLUME_LOW: &str = "\u{f027}";
pub(crate) const VOLUME_HIGH: &str = "\u{f028}";

// --- Play modes (Nerd Font) ---
pub(crate) const MODE_SEQUENTIAL: &str = "\u{F049E}";
pub(crate) const MODE_REPEAT_ONE: &str = "\u{F0458}";
pub(crate) const MODE_REPEAT_ALL: &str = "\u{F0456}";
pub(crate) const MODE_SHUFFLE: &str = "\u{F049F}";
pub(crate) const MODE_HEARTBEAT: &str = "\u{F0430}";

// --- Navigation, titles and list chrome ---
pub(crate) const SEARCH: &str = "\u{F002}";
pub(crate) const MUSIC_NOTE: &str = "\u{266a}"; // ♪
pub(crate) const PLAY_INDICATOR: &str = "\u{25b6}"; // ▶
pub(crate) const NAV_CAPSULE_START: &str = "\u{e0b2}";
pub(crate) const NAV_CAPSULE_END: &str = "\u{e0b0}";
pub(crate) const SECTION_MARK: &str = "\u{258e}"; // ▎
pub(crate) const TITLE_START: &str = "\u{25ba}"; // ►
pub(crate) const TITLE_END: &str = "\u{25c4}"; // ◄

// --- Status and feedback ---
pub(crate) const WARNING: &str = "\u{26a0}"; // ⚠
pub(crate) const ERROR_MARK: &str = "\u{2717}"; // ✗
pub(crate) const PENDING_MARK: &str = "\u{25cc}"; // ◌
pub(crate) const BULLET: char = '\u{25cf}'; // ●
pub(crate) const REFRESH: &str = "\u{21bb}"; // ↻
pub(crate) const HEART_FILLED: &str = "\u{2665}"; // ♥
pub(crate) const HEART_OUTLINE: &str = "\u{2661}"; // ♡
pub(crate) const NEXT_INDICATOR: &str = "\u{23ed}"; // ⏭
pub(crate) const VIP: &str = "\u{265b}"; // ♛
pub(crate) const LOGO: &str = "\u{2593}"; // ▓
pub(crate) const SCROLLBAR_THUMB: &str = "\u{2502}"; // │

// --- Widget defaults ---
pub(crate) const COVER_PLACEHOLDER: char = '\u{2591}'; // ░
pub(crate) const SPINNER_FILLED: char = '▰';
pub(crate) const PROGRESS_FILLED: &str = "\u{2501}"; // ━
pub(crate) const PROGRESS_UNFILLED: &str = "\u{2500}"; // ─
pub(crate) const SPEED: &str = "\u{26a1}"; // ⚡
pub(crate) const NAV_POSITION: &str = "\u{25e7}"; // ◧
pub(crate) const QUEUE: &str = "\u{25a3}"; // ▣
pub(crate) const CLEAR_QUEUE: &str = "\u{f48e}";
pub(crate) const UPLOAD: &str = "\u{2b06}"; // ⬆
pub(crate) const UPLOADED_MARK: &str = "\u{2191}"; // ↑
pub(crate) const SPLASH_MARK: &str = "\u{258d}"; // ▍
pub(crate) const SPINNER_FRAMES: &[&str] = &[
    "▰▱▱▱▱▱▱",
    "▰▰▱▱▱▱▱",
    "▰▰▰▱▱▱▱",
    "▰▰▰▰▱▱▱",
    "▰▰▰▰▰▱▱",
    "▰▰▰▰▰▰▱",
    "▰▰▰▰▰▰▰",
    "▰▱▱▱▱▱▱",
];
