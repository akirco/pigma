//! CLI entry: argument definitions (`Cli`) and the `run_cli` dispatcher.
//!
//! Subcommand handlers live in the `status` / `msg` / `completions` submodules;
//! clap definitions live in `defs` and the dispatcher in `run`.

mod completions;
mod defs;
mod msg;
mod run;
mod status;

pub use defs::{Cli, Command, MsgActionArg};
pub use msg::msg;
pub use run::run_cli;
pub use status::status;
