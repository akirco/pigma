//! ratatui widgets and rendering helpers used by the views (tables, player bar,
//! navigation, lyrics, toasts, spinners, breadcrumbs, ...).

mod block;
mod breadcrumb;
mod cards;
mod command_panel;
mod content;
mod equalizer;
mod gradient_line_gauge;
mod help;
mod layout;
mod login;
mod lyrics;
mod navigation;
mod playerbar;
mod queue;
mod scrollbar;
mod skeleton;
mod spinner;
mod splash;
mod style;
mod styled_text;
mod table;
pub(crate) mod terminal;
mod title;
mod toast;
mod topbar;

pub(crate) use style::BlockStyle;

mod render;
pub use render::draw;
