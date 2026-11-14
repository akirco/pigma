//! Shared application state: the top-level `State` that bundles the
//! navigation/search/login/help sub-state, plus the payload types those
//! layers are built on (`content`, `page`, `search`, `splash`).

mod command;
mod content;
mod help;
mod login;
mod navigation;
mod root;
mod search;
mod splash;

pub use command::*;
pub use content::*;
pub use help::*;
pub use login::*;
pub use navigation::*;
pub use root::*;
pub use search::*;
pub use splash::*;
