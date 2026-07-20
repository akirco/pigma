pub mod gradient;
pub mod time;

pub use gradient::gradient_color;
pub use time::{
    clock_time, format_duration, format_duration_into, local_timestamp, parse_duration_secs,
};
