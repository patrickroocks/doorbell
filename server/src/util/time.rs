use chrono::{DateTime, Local, Utc};

/// Formats a UTC time as a local time string in the format "YYYY-MM-DD HH:MM:SS".
pub fn formatLocal(utcTime: Option<DateTime<Utc>>) -> String {
    if let Some(t) = utcTime {
        let localTime = t.with_timezone(&Local);
        return localTime.format("%Y-%m-%d %H:%M:%S").to_string();
    }
    "Unknown".to_string()
}
