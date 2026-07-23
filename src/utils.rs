pub mod gradient;
pub mod time;
pub mod youtube;

pub use gradient::gradient_color;
pub use time::{
    clock_time, format_duration, format_duration_into, local_timestamp, parse_duration_secs,
};
pub use youtube::{clean_search_query, parse_duration_str, parse_view_count, score_match};
