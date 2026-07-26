//! Property-based (fuzz-style) tests.
//!
//! These tests feed randomized inputs — including invalid statuses and
//! priorities — into the data layer to make sure nothing panics and the
//! documented invariants hold for any input.
use proptest::prelude::*;

use crate::migration::{Migration, JSON_VERSIONS};
use crate::project::Project;
use crate::task::{Task, TASK_STATUS_DONE, TASK_STATUS_ON_GOING, TASK_STATUS_UP_NEXT};
use crate::test_utils::make_app;
use crate::util::Util;

fn arb_status() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        TASK_STATUS_UP_NEXT.to_string(),
        TASK_STATUS_ON_GOING.to_string(),
        TASK_STATUS_DONE.to_string(),
        // Unknown status values must not crash anything
        "Bogus".to_string(),
    ])
}

fn arb_task() -> impl Strategy<Value = Task> {
    (
        ".*",
        arb_status(),
        any::<u8>(),
        any::<Option<u64>>(),
        any::<Option<u64>>(),
        ".*",
        any::<u64>(),
        any::<u64>(),
    )
        .prop_map(
            |(
                title,
                status,
                priority,
                created_at,
                completed_at,
                note,
                time_spent_secs,
                estimated_hours,
            )| {
                Task {
                    title,
                    status,
                    priority,
                    created_at,
                    completed_at,
                    note,
                    time_spent_secs,
                    estimated_hours,
                }
            },
        )
}

fn arb_projects() -> impl Strategy<Value = Vec<Project>> {
    prop::collection::vec(
        (".*", prop::collection::vec(arb_task(), 0..8))
            .prop_map(|(title, tasks)| Project { title, tasks }),
        0..5,
    )
}

proptest! {
    #[test]
    fn serde_round_trip(projects in arb_projects()) {
        let raw = serde_json::to_string(&projects).unwrap();
        let parsed: Vec<Project> = serde_json::from_str(&raw).unwrap();
        prop_assert_eq!(projects, parsed);
    }

    #[test]
    fn migrations_never_panic_and_preserve_shape(
        projects in arb_projects(),
        version in prop::sample::select(JSON_VERSIONS.to_vec()),
    ) {
        let task_count: usize = projects.iter().map(|p| p.tasks.len()).sum();
        let migrations = Migration::get_migrations(version, projects);

        prop_assert!(migrations.len() < JSON_VERSIONS.len());
        for (_, data) in &migrations {
            prop_assert_eq!(
                data.iter().map(|p| p.tasks.len()).sum::<usize>(),
                task_count
            );
        }
    }

    #[test]
    fn task_load_items_never_panics_and_selects_a_valid_index(
        tasks in prop::collection::vec(arb_task(), 0..8),
        hide_done in any::<bool>(),
        selected in any::<usize>(),
    ) {
        let mut app = make_app(vec![Project {
            title: "p".to_string(),
            tasks,
        }]);
        app.hide_done_tasks = hide_done;
        app.selected_task_index.select(Some(selected));
        let mut items = vec![];

        Task::load_items(&mut app, &mut items);

        let expected_visible = app.projects[0]
            .tasks
            .iter()
            .filter(|t| !(hide_done && t.status == TASK_STATUS_DONE))
            .count();
        prop_assert_eq!(items.len(), expected_visible);
        if !items.is_empty() {
            prop_assert!(app.selected_task_index.selected().unwrap() < items.len());
        }
    }

    #[test]
    fn project_load_items_matches_project_count(projects in arb_projects()) {
        let mut app = make_app(projects.clone());
        let mut items = vec![];

        Project::load_items(&mut app, &mut items);

        prop_assert_eq!(items.len(), projects.len());
    }

    #[test]
    fn format_duration_never_panics(a in any::<Option<u64>>(), b in any::<Option<u64>>()) {
        let out = Util::format_duration(a, b);
        prop_assert!(!out.is_empty());
    }

    #[test]
    fn format_timestamp_never_panics(ts in any::<Option<u64>>()) {
        let _ = Util::format_timestamp(ts);
    }

    #[test]
    fn priority_indicator_is_bounded(value in any::<u8>()) {
        prop_assert!(Util::get_priority_indicator(value).chars().count() <= 3);
    }
}
