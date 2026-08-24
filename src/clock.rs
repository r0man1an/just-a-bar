use crate::config::{ClockFormat, ClockMode};

pub fn format_now(mode: ClockMode, format: ClockFormat) -> String {
    let now = chrono::Local::now();
    let time = match format {
        ClockFormat::Hour24 => now.format("%H:%M").to_string(),
        ClockFormat::Hour12 => now.format("%I:%M %p").to_string(),
    };
    match mode {
        ClockMode::Time => time,
        ClockMode::DateTime => format!("{}, {}", now.format("%-d. %b"), time),
    }
}
