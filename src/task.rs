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
    #[serde(default)]
    pub time_spent_secs: u64,
    /// Estimated time the task is expected to take; 0 = no estimate.
    #[serde(default)]
    pub estimated_hours: u64,
}

pub const TASK_STATUS_DONE: &str = "Done";
pub const TASK_STATUS_ON_GOING: &str = "OnGoing";
pub const TASK_STATUS_UP_NEXT: &str = "UpNext";

pub const TASK_PRIORITY_NONE: u8 = 0;

pub const TASK_STATUSES: [&str; 3] = [TASK_STATUS_UP_NEXT, TASK_STATUS_ON_GOING, TASK_STATUS_DONE];

const TASK_STATUSES_SORT_ORDER: [&str; 3] =
    [TASK_STATUS_ON_GOING, TASK_STATUS_UP_NEXT, TASK_STATUS_DONE];

// Ascending order: 1 highest priority; 2 medium; 3 lowest; TASK_PRIORITY_NONE = no priority
pub const TASK_PRIORITIES: [u8; 4] = [1, 2, 3, TASK_PRIORITY_NONE];

impl Task {
    pub fn get_status_color(status: &str) -> ratatui::prelude::Color {
        match status {
            TASK_STATUS_DONE => Color::LightGreen,
            TASK_STATUS_ON_GOING => Color::Yellow,
            TASK_STATUS_UP_NEXT => Color::LightMagenta,
            _ => Color::Gray,
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
            let span = Span::styled(status, Style::new().fg(Task::get_status_color(status)));

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
                time_spent_secs: 0,
                estimated_hours: 0,
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
            // The board always shows the Done lane, so its item list is the
            // full sorted list and visible indices match full-list indices
            if app.hide_done_tasks && !app.board_view && task.status == TASK_STATUS_DONE {
                continue;
            }
            if full_idx == new_index {
                visible_selected = items.len();
            }

            items.push(ListItem::from(Line::from(Task::repr_spans(task, true))))
        }

        app.selected_task_index.select(Some(visible_selected));

        // Keep the board view consistent after any mutation: the selected
        // task may have changed lane (status change) or disappeared.
        // List mode is skipped on purpose: its selection is a *visible*
        // index, which only matches the full-list index the board
        // reasons in while done tasks sort last.
        if app.board_view {
            app.board_sync();
        }
    }

    /// Spans rendering one task line: `[!]` priority prefix (if any),
    /// `[status]` prefix (list view only — the board lane already conveys
    /// the status), the title (crossed out when done), and the `[x%]`
    /// estimate-progress suffix (when an estimate is set). Shared by the
    /// list and board views so the two renderings cannot drift apart.
    pub fn repr_spans(task: &Task, with_status: bool) -> Vec<Span<'static>> {
        let modifier = if task.status == TASK_STATUS_DONE {
            Modifier::CROSSED_OUT
        } else {
            Modifier::empty()
        };

        let mut repr = vec![];

        if task.priority != TASK_PRIORITY_NONE {
            repr.push(Span::styled(
                format!("[{}] ", Util::get_priority_indicator(task.priority)),
                Style::new().fg(Color::Red),
            ));
        }

        if with_status {
            repr.push(Span::styled(
                format!("[{}] ", task.status),
                Style::default()
                    .fg(Task::get_status_color(&task.status))
                    .add_modifier(modifier),
            ));
        }

        repr.push(Span::styled(
            task.title.clone(),
            Style::default().add_modifier(modifier),
        ));

        // Estimate progress for tasks with an estimated duration
        if let Some(pct) = task.estimate_progress(0) {
            repr.push(Span::styled(
                format!(" [{}%]", pct),
                Style::default().fg(if pct >= 100 { Color::Red } else { Color::Cyan }),
            ));
        }

        repr
    }

    /// Full-list indices of the tasks in a status lane, in display (sorted)
    /// order. The board always shows every lane, so `hide_done_tasks` is
    /// deliberately not applied here.
    pub fn lane_indices(app: &App, status: &str) -> Vec<usize> {
        app.projects[app.selected_project_index.selected().unwrap()]
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.status == status)
            .map(|(i, _)| i)
            .collect()
    }

    pub fn get_current(app: &mut App) -> &Task {
        &app.projects[app.selected_project_index.selected().unwrap()].tasks
            [app.selected_task_index.selected().unwrap()]
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
                time_spent_secs: 0,
                estimated_hours: 0,
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

    pub fn update_estimate(app: &mut App, items: &mut Vec<ListItem>, hours: u64) {
        app.projects[app.selected_project_index.selected().unwrap()].tasks
            [app.selected_task_index.selected().unwrap()]
        .estimated_hours = hours;

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

    /// Percentage of the estimate already spent, `None` when no estimate is
    /// set. `extra_secs` accounts for time not yet settled (a running timer).
    pub fn estimate_progress(&self, extra_secs: u64) -> Option<u64> {
        if self.estimated_hours == 0 {
            return None;
        }

        let pct = self.time_spent_secs.saturating_add(extra_secs) as f64
            / self.estimated_hours.saturating_mul(3600) as f64
            * 100.0;
        Some(pct.round() as u64)
    }

    /// Accumulate timer seconds into a task, located by project index and
    /// title (list indexes shift on sort, so the title is the stable identity).
    /// A task deleted while its timer was running is silently skipped.
    pub fn add_time_spent(app: &mut App, project_index: usize, task_title: &str, secs: u64) {
        if secs == 0 {
            return;
        }

        let Some(project) = app.projects.get_mut(project_index) else {
            return;
        };
        let Some(task) = project.tasks.iter_mut().find(|t| t.title == task_title) else {
            return;
        };

        task.time_spent_secs += secs;

        Json::write(app.projects.clone());
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
    fn add_time_spent_accumulates_and_persists() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = app_with_tasks(vec![make_task(
            "t",
            TASK_STATUS_UP_NEXT,
            TASK_PRIORITY_NONE,
        )]);

        Task::add_time_spent(&mut app, 0, "t", 120);
        Task::add_time_spent(&mut app, 0, "t", 30);

        assert_eq!(app.projects[0].tasks[0].time_spent_secs, 150);
        assert_eq!(Json::read()[0].tasks[0].time_spent_secs, 150);
    }

    #[test]
    fn add_time_spent_ignores_zero_secs_and_missing_tasks() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = app_with_tasks(vec![make_task(
            "t",
            TASK_STATUS_UP_NEXT,
            TASK_PRIORITY_NONE,
        )]);

        Task::add_time_spent(&mut app, 0, "t", 0);
        Task::add_time_spent(&mut app, 0, "ghost", 60);
        Task::add_time_spent(&mut app, 9, "t", 60);

        assert_eq!(app.projects[0].tasks[0].time_spent_secs, 0);
    }

    #[test]
    fn update_estimate_changes_selected_task_and_persists() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = app_with_tasks(vec![make_task(
            "t",
            TASK_STATUS_UP_NEXT,
            TASK_PRIORITY_NONE,
        )]);
        let mut items = vec![];

        Task::update_estimate(&mut app, &mut items, 500);

        assert_eq!(app.projects[0].tasks[0].estimated_hours, 500);
        assert_eq!(Json::read()[0].tasks[0].estimated_hours, 500);
    }

    #[test]
    fn new_tasks_have_no_estimate() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = app_with_tasks(vec![]);
        let mut items = vec![];

        Task::create(&mut app, &mut items, "do it");

        assert_eq!(app.projects[0].tasks[0].estimated_hours, 0);
    }

    #[test]
    fn estimate_progress_is_none_without_estimate() {
        let mut task = make_task("t", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE);
        task.time_spent_secs = 3600;
        task.estimated_hours = 0;

        assert_eq!(task.estimate_progress(0), None);
    }

    #[test]
    fn estimate_progress_rounds_and_includes_extra_secs() {
        let mut task = make_task("t", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE);
        task.estimated_hours = 10; // 36000 secs

        task.time_spent_secs = 3600; // 10%
        assert_eq!(task.estimate_progress(0), Some(10));
        // extra (unsettled timer) time counts towards the progress
        assert_eq!(task.estimate_progress(1800), Some(15));

        task.time_spent_secs = 36360; // 101%
        assert_eq!(task.estimate_progress(0), Some(101));
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
