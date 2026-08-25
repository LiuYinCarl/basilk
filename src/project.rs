use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
};
use serde::{Deserialize, Serialize};

use crate::{
    json::Json,
    task::{Task, TASK_STATUS_DONE},
    App,
};

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct Project {
    pub title: String,
    pub tasks: Vec<Task>,
}

impl Project {
    fn get_indicator_done_tasks_color(percentage: usize) -> ratatui::prelude::Color {
        match percentage {
            0 => Color::DarkGray,
            p if (25..=50).contains(&p) => Color::LightMagenta,
            p if p > 50 && p < 100 => Color::LightYellow,
            100 => Color::LightGreen,
            _ => Color::White,
        }
    }

    pub fn load_items(app: &mut App, items: &mut Vec<ListItem>) {
        items.clear();

        for project in app.projects.iter() {
            let tasks = &project.tasks;

            let done_tasks: Vec<Task> = tasks
                .clone()
                .into_iter()
                .filter(|t| t.status == TASK_STATUS_DONE)
                .collect();

            let percentage = if tasks.is_empty() {
                0
            } else {
                (done_tasks.len() * 100) / tasks.len()
            };

            let lines = vec![Line::from(vec![
                Span::raw(format!("[{}/{}] ", done_tasks.len(), tasks.len(),)).style(
                    Style::default().fg(Project::get_indicator_done_tasks_color(percentage)),
                ),
                Span::raw(project.title.clone()),
            ])];

            items.push(ListItem::from(lines))
        }
    }

    pub fn get_current(app: &mut App) -> &Project {
        &app.projects[app.selected_project_index.selected().unwrap()]
    }

    pub fn create(app: &mut App, items: &mut Vec<ListItem>, value: &str) {
        if value.is_empty() {
            return;
        }

        app.projects.push(Project {
            title: value.to_string(),
            tasks: vec![],
        });

        Json::write(app.projects.clone());
        Project::load_items(app, items)
    }

    pub fn rename(app: &mut App, items: &mut Vec<ListItem>, value: &str) {
        app.projects[app.selected_project_index.selected().unwrap()].title = value.to_string();

        Json::write(app.projects.clone());
        Project::load_items(app, items)
    }

    pub fn delete(app: &mut App, items: &mut Vec<ListItem>) {
        app.projects
            .remove(app.selected_project_index.selected().unwrap());

        Json::write(app.projects.clone());
        Project::load_items(app, items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{make_app, sample_projects, setup_temp_config, ENV_LOCK};

    #[test]
    fn create_appends_project_and_persists() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = make_app(vec![]);
        let mut items = vec![];

        Project::create(&mut app, &mut items, "todo");

        assert_eq!(app.projects.len(), 1);
        assert_eq!(app.projects[0].title, "todo");
        assert_eq!(items.len(), 1);
        assert_eq!(Json::read(), app.projects);
    }

    #[test]
    fn create_with_empty_value_is_a_no_op() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = make_app(vec![]);
        let mut items = vec![];

        Project::create(&mut app, &mut items, "");

        assert!(app.projects.is_empty());
        assert!(items.is_empty());
    }

    #[test]
    fn rename_updates_selected_project() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = make_app(sample_projects());
        let mut items = vec![];
        app.selected_project_index.select(Some(1));

        Project::rename(&mut app, &mut items, "renamed");

        assert_eq!(app.projects[1].title, "renamed");
        assert_eq!(app.projects[0].title, "alpha");
        assert_eq!(Json::read()[1].title, "renamed");
    }

    #[test]
    fn delete_removes_selected_project() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _dir = setup_temp_config();
        let mut app = make_app(sample_projects());
        let mut items = vec![];

        Project::delete(&mut app, &mut items);

        assert_eq!(app.projects.len(), 1);
        assert_eq!(app.projects[0].title, "beta");
        assert_eq!(items.len(), 1);
        assert_eq!(Json::read(), app.projects);
    }

    #[test]
    fn load_items_builds_one_item_per_project() {
        let mut app = make_app(sample_projects());
        let mut items = vec![];

        Project::load_items(&mut app, &mut items);

        assert_eq!(items.len(), 2);
    }

    #[test]
    fn done_tasks_color_thresholds() {
        assert_eq!(Project::get_indicator_done_tasks_color(0), Color::DarkGray);
        assert_eq!(
            Project::get_indicator_done_tasks_color(25),
            Color::LightMagenta
        );
        assert_eq!(
            Project::get_indicator_done_tasks_color(50),
            Color::LightMagenta
        );
        assert_eq!(
            Project::get_indicator_done_tasks_color(75),
            Color::LightYellow
        );
        assert_eq!(
            Project::get_indicator_done_tasks_color(100),
            Color::LightGreen
        );
        // Values outside 0..=100 fall back to white
        assert_eq!(Project::get_indicator_done_tasks_color(1), Color::White);
    }
}
