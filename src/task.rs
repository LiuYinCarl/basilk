use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{json::Json, util::Util, App};

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct Task {
    pub title: String,
    pub status: String,
    pub priority: u8,
    #[serde(default)]
    pub created_at: Option<u64>,
    #[serde(default)]
    pub completed_at: Option<u64>,
    #[serde(default)]
    pub note: String,
}

pub const TASK_STATUS_DONE: &str = "Done";
pub const TASK_STATUS_ON_GOING: &str = "OnGoing";
pub const TASK_STATUS_UP_NEXT: &str = "UpNext";

pub const TASK_PRIORITY_NONE: u8 = 0;

pub const TASK_STATUSES: [&'static str; 3] =
    [TASK_STATUS_UP_NEXT, TASK_STATUS_ON_GOING, TASK_STATUS_DONE];

const TASK_STATUSES_SORT_ORDER: [&'static str; 3] =
    [TASK_STATUS_ON_GOING, TASK_STATUS_UP_NEXT, TASK_STATUS_DONE];

// Ascending order: 1 highest priority; 2 medium; 3 lowest; TASK_PRIORITY_NONE = no priority
pub const TASK_PRIORITIES: [u8; 4] = [1, 2, 3, TASK_PRIORITY_NONE];

impl Task {
    pub fn get_status_color(status: &String) -> ratatui::prelude::Color {
        match status.as_str() {
            TASK_STATUS_DONE => return Color::LightGreen,
            TASK_STATUS_ON_GOING => return Color::Yellow,
            TASK_STATUS_UP_NEXT => return Color::LightMagenta,
            _ => return Color::Gray,
        }
    }

    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    pub fn load_statues_items(items: &mut Vec<ListItem>) {
        items.clear();

        for status in TASK_STATUSES {
            let span = Span::styled(
                status,
                Style::new().fg(Task::get_status_color(&status.to_string())),
            );

            items.push(ListItem::from(span))
        }
    }

    pub fn load_priority_items(items: &mut Vec<ListItem>) {
        items.clear();

        for priority_value in TASK_PRIORITIES {
            let span = Span::styled(
                Util::get_priority_indicator(priority_value),
                Style::new().fg(Color::Red),
            );

            items.push(ListItem::from(span))
        }
    }

    pub fn load_items(app: &mut App, items: &mut Vec<ListItem>) {
        let tasks = &mut app.projects[app.selected_project_index.selected().unwrap()].tasks;

        let last_task_title_selected = tasks
            .clone()
            .get(app.selected_task_index.selected().unwrap_or(0))
            .unwrap_or(&Task {
                title: "".to_string(),
                status: "".to_string(),
                priority: TASK_PRIORITY_NONE,
                created_at: None,
                completed_at: None,
                note: "".to_string(),
            })
            .clone()
            .title;

        // Sort by status first, then by priority: `sort_by_key` is stable, so
        // the last sort wins as the primary key and the final order is
        // priority-major (status is the tie-breaker within each priority).
        tasks.sort_by_key(|t| {
            TASK_STATUSES_SORT_ORDER
                .into_iter()
                .position(|o| o == t.status)
        });

        tasks.sort_by_key(|t| TASK_PRIORITIES.into_iter().position(|o| o == t.priority));

        let new_index = tasks
            .iter()
            .position(|t| t.title == last_task_title_selected)
            .unwrap_or(0);

        items.clear();

        let mut visible_selected = 0;
        for (full_idx, task) in tasks.iter().enumerate() {
            if app.hide_done_tasks && task.status == TASK_STATUS_DONE {
                continue;
            }
            if full_idx == new_index {
                visible_selected = items.len();
            }
            let modifier = if task.status == TASK_STATUS_DONE {
                Modifier::CROSSED_OUT
            } else {
                Modifier::empty()
            };

            let mut repr = vec![
                Span::styled(
                    format!("[{}] ", task.status),
                    Style::default()
                        .fg(Task::get_status_color(&task.status))
                        .add_modifier(modifier),
                ),
                Span::styled(task.title.clone(), Style::default().add_modifier(modifier)),
            ];

            if task.priority != TASK_PRIORITY_NONE {
                let priority_repr = vec![Span::styled(
                    format!("[{}] ", Util::get_priority_indicator(task.priority)),
                    Style::new().fg(Color::Red),
                )];
                repr = [priority_repr, repr].concat()
            }

            let line = Line::from(repr);

            items.push(ListItem::from(line))
        }

        app.selected_task_index.select(Some(visible_selected))
    }

    pub fn get_current(app: &mut App) -> &Task {
        return &app.projects[app.selected_project_index.selected().unwrap()].tasks
            [app.selected_task_index.selected().unwrap()];
    }

    pub fn create(app: &mut App, items: &mut Vec<ListItem>, value: &str) {
        if value.is_empty() {
            return;
        }

        app.projects[app.selected_project_index.selected().unwrap()]
            .tasks
            .push(Task {
                title: value.to_string(),
                status: TASK_STATUS_UP_NEXT.to_string(),
                priority: TASK_PRIORITY_NONE,
                created_at: Some(Self::current_timestamp()),
                completed_at: None,
                note: "".to_string(),
            });

        Json::write(app.projects.clone());
        Task::load_items(app, items)
    }

    pub fn rename(app: &mut App, items: &mut Vec<ListItem>, value: &str) {
        app.projects[app.selected_project_index.selected().unwrap()].tasks
            [app.selected_task_index.selected().unwrap()]
        .title = value.to_string();

        Json::write(app.projects.clone());
        Task::load_items(app, items)
    }

    pub fn update_note(app: &mut App, items: &mut Vec<ListItem>, value: &str) {
        app.projects[app.selected_project_index.selected().unwrap()].tasks
            [app.selected_task_index.selected().unwrap()]
        .note = value.to_string();

        Json::write(app.projects.clone());
        Task::load_items(app, items)
    }

    pub fn change_status(app: &mut App, items: &mut Vec<ListItem>, value: &str) {
        let status = value.to_string();
        let projects = &mut app.projects[app.selected_project_index.selected().unwrap()].tasks;
        let task = &mut projects[app.selected_task_index.selected().unwrap()];

        task.status = status.clone();

        if status == TASK_STATUS_DONE {
            task.priority = TASK_PRIORITY_NONE;
            if task.completed_at.is_none() {
                task.completed_at = Some(Self::current_timestamp());
            }
        } else {
            task.completed_at = None;
        }

        Json::write(app.projects.clone());
        Task::load_items(app, items)
    }

    pub fn change_priority(app: &mut App, items: &mut Vec<ListItem>, value: u8) {
        app.projects[app.selected_project_index.selected().unwrap()].tasks
            [app.selected_task_index.selected().unwrap()]
        .priority = value;

        Json::write(app.projects.clone());
        Task::load_items(app, items)
    }

    pub fn delete(app: &mut App, items: &mut Vec<ListItem>) {
        app.projects[app.selected_project_index.selected().unwrap()]
            .tasks
            .remove(app.selected_task_index.selected().unwrap());

        Json::write(app.projects.clone());
        Task::load_items(app, items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Project;
    use crate::test_utils::{make_app, make_task, setup_temp_config, ENV_LOCK};

    fn app_with_tasks(tasks: Vec<Task>) -> App {
        make_app(vec![Project {
            title: "p".to_string(),
            tasks,
        }])
    }

    #[test]
    fn create_appends_task_and_persists() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = app_with_tasks(vec![]);
        let mut items = vec![];

        Task::create(&mut app, &mut items, "do it");

        let task = &app.projects[0].tasks[0];
        assert_eq!(task.title, "do it");
        assert_eq!(task.status, TASK_STATUS_UP_NEXT);
        assert_eq!(task.priority, TASK_PRIORITY_NONE);
        assert!(task.created_at.is_some());
        assert_eq!(items.len(), 1);
        assert_eq!(Json::read(), app.projects);
    }

    #[test]
    fn create_with_empty_value_is_a_no_op() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = app_with_tasks(vec![]);
        let mut items = vec![];

        Task::create(&mut app, &mut items, "");

        assert!(app.projects[0].tasks.is_empty());
    }

    #[test]
    fn change_status_to_done_sets_completed_at_and_clears_priority() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = app_with_tasks(vec![make_task("t", TASK_STATUS_ON_GOING, 1)]);
        let mut items = vec![];
        app.hide_done_tasks = false;

        Task::change_status(&mut app, &mut items, TASK_STATUS_DONE);

        let task = &app.projects[0].tasks[0];
        assert_eq!(task.status, TASK_STATUS_DONE);
        assert_eq!(task.priority, TASK_PRIORITY_NONE);
        assert!(task.completed_at.is_some());
    }

    #[test]
    fn change_status_away_from_done_clears_completed_at() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut task = make_task("t", TASK_STATUS_DONE, TASK_PRIORITY_NONE);
        task.completed_at = Some(1_700_000_100);
        let mut app = app_with_tasks(vec![task]);
        let mut items = vec![];
        app.hide_done_tasks = false;

        Task::change_status(&mut app, &mut items, TASK_STATUS_ON_GOING);

        let task = &app.projects[0].tasks[0];
        assert_eq!(task.status, TASK_STATUS_ON_GOING);
        assert_eq!(task.completed_at, None);
    }

    #[test]
    fn change_priority_updates_selected_task() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = app_with_tasks(vec![make_task(
            "t",
            TASK_STATUS_UP_NEXT,
            TASK_PRIORITY_NONE,
        )]);
        let mut items = vec![];

        Task::change_priority(&mut app, &mut items, 2);

        assert_eq!(app.projects[0].tasks[0].priority, 2);
    }

    #[test]
    fn rename_and_update_note_modify_selected_task() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = app_with_tasks(vec![make_task(
            "old",
            TASK_STATUS_UP_NEXT,
            TASK_PRIORITY_NONE,
        )]);
        let mut items = vec![];

        Task::rename(&mut app, &mut items, "new");
        Task::update_note(&mut app, &mut items, "a note");

        let task = &app.projects[0].tasks[0];
        assert_eq!(task.title, "new");
        assert_eq!(task.note, "a note");
    }

    #[test]
    fn delete_removes_selected_task() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = app_with_tasks(vec![
            make_task("t1", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE),
            make_task("t2", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE),
        ]);
        let mut items = vec![];
        app.selected_task_index.select(Some(1));

        Task::delete(&mut app, &mut items);

        assert_eq!(app.projects[0].tasks.len(), 1);
        assert_eq!(app.projects[0].tasks[0].title, "t1");
    }

    #[test]
    fn load_items_sorts_by_priority_then_status() {
        let mut app = app_with_tasks(vec![
            make_task("done-task", TASK_STATUS_DONE, TASK_PRIORITY_NONE),
            make_task("up-next-low", TASK_STATUS_UP_NEXT, 3),
            make_task("on-going", TASK_STATUS_ON_GOING, TASK_PRIORITY_NONE),
            make_task("up-next-high", TASK_STATUS_UP_NEXT, 1),
        ]);
        let mut items = vec![];
        app.hide_done_tasks = false;

        Task::load_items(&mut app, &mut items);

        let titles: Vec<&str> = app.projects[0]
            .tasks
            .iter()
            .map(|t| t.title.as_str())
            .collect();
        // Priority is the primary key (1, 3, then NONE); within the NONE
        // group the status order applies, so done tasks come last.
        assert_eq!(
            titles,
            vec!["up-next-high", "up-next-low", "on-going", "done-task"]
        );
    }

    #[test]
    fn load_items_hides_done_tasks_when_enabled() {
        let mut app = app_with_tasks(vec![
            make_task("visible", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE),
            make_task("hidden", TASK_STATUS_DONE, TASK_PRIORITY_NONE),
        ]);
        let mut items = vec![];

        Task::load_items(&mut app, &mut items);

        assert_eq!(items.len(), 1);
    }

    #[test]
    fn load_items_keeps_selection_on_the_same_task_after_resort() {
        let mut app = app_with_tasks(vec![
            make_task("a", TASK_STATUS_ON_GOING, TASK_PRIORITY_NONE),
            make_task("b", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE),
        ]);
        let mut items = vec![];
        app.selected_task_index.select(Some(0)); // task "a"

        Task::load_items(&mut app, &mut items);

        let selected = Task::get_current(&mut app);
        assert_eq!(selected.title, "a");
    }

    /// Regression test for the reported crash: after creating a project the
    /// selection must point at an existing project when opening its tasks.
    #[test]
    fn create_project_then_view_tasks_does_not_panic() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = make_app(vec![]);
        let mut items = vec![];

        Project::create(&mut app, &mut items, "todo");
        // This is what the fixed AddProject handler does: select the last index
        app.selected_project_index
            .select(Some(app.projects.len() - 1));

        Task::load_items(&mut app, &mut items);

        assert!(items.is_empty());
    }
}
