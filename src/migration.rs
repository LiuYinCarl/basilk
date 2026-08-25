use crate::project::Project;
use crate::task::TASK_PRIORITY_NONE;

//                              sha of 0.1.0     0.2.0    0.2.2     timer     notes
pub static JSON_VERSIONS: [&str; 5] = ["6ad96", "911fc", "a4e1b", "c8f21", "e5a1c"];

type MigrationFn = fn(Vec<Project>) -> Vec<Project>;

pub struct Migration;

impl Migration {
    pub fn get_migrations(version: &str, original_json: Vec<Project>) -> Vec<(&str, Vec<Project>)> {
        // Mapper between json version and the relative migration
        let mapper: Vec<(&str, MigrationFn)> = vec![
            ("6ad96", |data| data),
            ("911fc", Migration::add_priority),
            ("a4e1b", Migration::add_note),
            ("c8f21", Migration::add_timer_fields),
            // Notes live outside `Vec<Project>` and default to empty via
            // serde, so this step only bumps the stored version
            ("e5a1c", |data| data),
        ];

        // The start index where the migration are picked
        let start_index = mapper.iter().position(|(key, _val)| *key == version);

        if start_index.is_none() {
            return vec![];
        }

        let mut results = vec![];
        let mut current_data = original_json;

        for (v, migration_fn) in mapper.into_iter().skip(start_index.unwrap() + 1) {
            current_data = migration_fn(current_data);
            results.push((v, current_data.clone()));
        }

        results
    }

    // Migrations
    fn add_priority(mut data: Vec<Project>) -> Vec<Project> {
        for p in data.iter_mut() {
            for t in p.tasks.iter_mut() {
                t.priority = TASK_PRIORITY_NONE;
            }
        }
        data
    }

    fn add_note(mut data: Vec<Project>) -> Vec<Project> {
        for p in data.iter_mut() {
            for t in p.tasks.iter_mut() {
                t.note = "".to_string();
            }
        }
        data
    }

    fn add_timer_fields(mut data: Vec<Project>) -> Vec<Project> {
        for p in data.iter_mut() {
            for t in p.tasks.iter_mut() {
                t.time_spent_secs = 0;
                t.estimated_hours = 0;
            }
        }
        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Task, TASK_PRIORITY_NONE, TASK_STATUS_ON_GOING};

    fn sample_data() -> Vec<Project> {
        vec![Project {
            title: "p".to_string(),
            tasks: vec![Task {
                title: "t".to_string(),
                status: TASK_STATUS_ON_GOING.to_string(),
                priority: 2,
                created_at: None,
                completed_at: None,
                note: "keep me".to_string(),
                time_spent_secs: 42,
                estimated_hours: 7,
            }],
        }]
    }

    #[test]
    fn migrations_from_oldest_version_apply_all_steps() {
        let migrations = Migration::get_migrations("6ad96", sample_data());

        assert_eq!(migrations.len(), 4);
        assert_eq!(migrations[0].0, "911fc");
        assert_eq!(migrations[1].0, "a4e1b");
        assert_eq!(migrations[2].0, "c8f21");
        assert_eq!(migrations[3].0, "e5a1c");

        let task = &migrations[3].1[0].tasks[0];
        assert_eq!(task.priority, TASK_PRIORITY_NONE);
        assert_eq!(task.note, "");
        assert_eq!(task.time_spent_secs, 0);
        assert_eq!(task.estimated_hours, 0);
    }

    #[test]
    fn migrations_from_middle_version_apply_remaining_steps() {
        let migrations = Migration::get_migrations("911fc", sample_data());

        assert_eq!(migrations.len(), 3);
        assert_eq!(migrations[0].0, "a4e1b");
        assert_eq!(migrations[1].0, "c8f21");
        assert_eq!(migrations[2].0, "e5a1c");
        // add_priority is not re-applied: priority is preserved
        assert_eq!(migrations[0].1[0].tasks[0].priority, 2);
    }

    #[test]
    fn no_migrations_for_latest_or_unknown_version() {
        assert!(Migration::get_migrations("e5a1c", sample_data()).is_empty());
        assert!(Migration::get_migrations("unknown", sample_data()).is_empty());
    }

    #[test]
    fn json_versions_are_chained_in_mapper_order() {
        // Every version except the last must produce migrations ending at the last version
        for (i, version) in JSON_VERSIONS.iter().enumerate() {
            let migrations = Migration::get_migrations(version, vec![]);
            assert_eq!(migrations.len(), JSON_VERSIONS.len() - 1 - i);
        }
    }
}
