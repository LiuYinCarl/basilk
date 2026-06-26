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
