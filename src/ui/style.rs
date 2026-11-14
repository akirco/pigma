//! Shared visual style handed to widgets: theme colors, border mode and the
//! animation tick used by gradient borders.

use crate::config::{BorderConfig, Theme};

pub(crate) struct BlockStyle<'a> {
    pub colors: &'a Theme,
    pub border: &'a BorderConfig,
    pub tick: u64,
}
