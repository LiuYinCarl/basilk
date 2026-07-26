use chrono::{DateTime, Local};

use crate::task::{TASK_PRIORITIES, TASK_PRIORITY_NONE};

pub struct Util;

impl Util {
    pub fn get_spaced_title(title: &str) -> String {
        format!(" {} ", title)
    }

    pub fn get_priority_indicator(value: u8) -> String {
        // Priority value is in ascending order
        // but in the visualization the order is reversed to be more intuitive
        // priority: 1 => !!!
        // priority: 2 => !!
        // priority: 3 => !
        let priority_value = if value == TASK_PRIORITY_NONE {
            0
        } else {
            TASK_PRIORITIES
                .into_iter()
                .rev()
                .position(|t| t == value)
                .unwrap_or(0)
        };

        "!!!".chars().take((priority_value).into()).collect()
    }

    /// Format a plain seconds count as HH:MM:SS (timer readouts).
    pub fn format_secs(secs: u64) -> String {
        format!(
            "{:02}:{:02}:{:02}",
            secs / 3600,
            (secs % 3600) / 60,
            secs % 60
        )
    }

    pub fn format_timestamp(timestamp: Option<u64>) -> String {
        timestamp
            .and_then(|ts| DateTime::from_timestamp(ts as i64, 0))
            .map(|dt| {
                let local_dt: DateTime<Local> = dt.into();
                local_dt.format("%Y-%m-%d %H:%M:%S %z").to_string()
            })
            .unwrap_or_else(|| "N/A".to_string())
    }

    pub fn format_duration(start: Option<u64>, end: Option<u64>) -> String {
        match (start, end) {
            (Some(s), Some(e)) => {
                if e > s {
                    let duration_secs = e - s;
                    let days = duration_secs / 86400;
                    let hours = (duration_secs % 86400) / 3600;
                    let minutes = (duration_secs % 3600) / 60;
                    let seconds = duration_secs % 60;

                    if days > 0 {
                        format!("{}d {}h {}m {}s", days, hours, minutes, seconds)
                    } else if hours > 0 {
                        format!("{}h {}m {}s", hours, minutes, seconds)
                    } else if minutes > 0 {
                        format!("{}m {}s", minutes, seconds)
                    } else {
                        format!("{}s", seconds)
                    }
                } else {
                    "Invalid time range".to_string()
                }
            }
            _ => "Task not completed".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaced_title_wraps_with_spaces() {
        assert_eq!(Util::get_spaced_title("hello"), " hello ");
    }

    #[test]
    fn priority_indicator_mapping() {
        assert_eq!(Util::get_priority_indicator(TASK_PRIORITY_NONE), "");
        assert_eq!(Util::get_priority_indicator(1), "!!!");
        assert_eq!(Util::get_priority_indicator(2), "!!");
        assert_eq!(Util::get_priority_indicator(3), "!");
        // Unknown values fall back to an empty indicator
        assert_eq!(Util::get_priority_indicator(99), "");
    }

    #[test]
    fn format_secs_renders_hh_mm_ss() {
        assert_eq!(Util::format_secs(0), "00:00:00");
        assert_eq!(Util::format_secs(59), "00:00:59");
        assert_eq!(Util::format_secs(60), "00:01:00");
        assert_eq!(Util::format_secs(3661), "01:01:01");
        assert_eq!(Util::format_secs(90_061), "25:01:01");
    }

    #[test]
    fn format_timestamp_handles_none_and_epoch() {
        assert_eq!(Util::format_timestamp(None), "N/A");
        assert!(Util::format_timestamp(Some(0)).contains("1970-01-01"));
    }

    #[test]
    fn format_duration_variants() {
        assert_eq!(Util::format_duration(None, Some(10)), "Task not completed");
        assert_eq!(Util::format_duration(Some(10), None), "Task not completed");
        assert_eq!(
            Util::format_duration(Some(10), Some(10)),
            "Invalid time range"
        );
        assert_eq!(Util::format_duration(Some(0), Some(45)), "45s");
        assert_eq!(Util::format_duration(Some(0), Some(90)), "1m 30s");
        assert_eq!(Util::format_duration(Some(0), Some(3700)), "1h 1m 40s");
        assert_eq!(Util::format_duration(Some(0), Some(90061)), "1d 1h 1m 1s");
    }
}
