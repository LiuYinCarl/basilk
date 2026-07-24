use std::{
    error::Error,
    fmt::Debug,
    io::{self, stdout},
};

use cli::Cli;
use ratatui::{
    crossterm::{
        event::{self, Event, KeyCode, KeyEventKind},
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
        ExecutableCommand,
    },
    prelude::*,
    widgets::*,
};
use tui_input::{backend::crossterm::EventHandler, Input};

mod cli;
mod json;
mod migration;
mod project;
mod task;
mod ui;
mod util;
mod view;

use json::Json;
use project::Project;
use task::{Task, TASK_PRIORITIES, TASK_STATUSES};
use ui::Ui;
use view::View;

#[derive(Default, PartialEq, Debug)]
pub enum ViewMode {
    #[default]
    ViewProjects,
    RenameProject,
    AddProject,
    DeleteProject,

    ViewTasks,
    RenameTask,
    ChangeStatusTask,
    ChangePriorityTask,
    AddTask,
    DeleteTask,
    ViewTaskDetails,
    EditTaskNote,
    ViewHelp,

    InfoMigration,
}

pub struct App {
    // TODO: Better list state mgmt
    selected_project_index: ListState,
    selected_task_index: ListState,
    selected_status_task_index: ListState,
    selected_priority_task_index: ListState,
    delete_confirm_index: ListState,
    view_mode: ViewMode,
    previous_view_mode: ViewMode,
    projects: Vec<Project>,
    hide_done_tasks: bool,
}

fn init_terminal() -> Result<Terminal<impl Backend>, Box<dyn Error>> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal() -> Result<(), Box<dyn Error>> {
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    Cli::read();

    // Check the version of the json file
    let were_applied_migrations = Json::check()?;

    // setup terminal
    let terminal = init_terminal()?;

    // create app and run it
    App::setup().run(terminal, were_applied_migrations)?;

    restore_terminal()?;

    Ok(())
}

impl App {
    fn setup() -> Self {
        Self {
            selected_project_index: ListState::default().with_selected(Some(0)),
            selected_task_index: ListState::default().with_selected(Some(0)),
            selected_status_task_index: ListState::default().with_selected(Some(0)),
            selected_priority_task_index: ListState::default().with_selected(Some(0)),
            delete_confirm_index: ListState::default().with_selected(Some(0)),
            view_mode: ViewMode::default(),
            previous_view_mode: ViewMode::default(),
            projects: Json::read(),
            hide_done_tasks: true,
        }
    }

    fn run(
        &mut self,
        mut terminal: Terminal<impl Backend>,
        were_applied_migrations: bool,
    ) -> io::Result<()> {
        let mut input = Input::default();

        let mut items: Vec<ListItem> = vec![];
        Project::load_items(self, &mut items);

        let mut status_items: Vec<ListItem> = vec![];
        Task::load_statues_items(&mut status_items);

        let mut priority_items: Vec<ListItem> = vec![];
        Task::load_priority_items(&mut priority_items);

        let mut delete_confirm_items: Vec<ListItem> = vec![];
        Ui::load_delete_confirm_items(&mut delete_confirm_items);

        if were_applied_migrations {
            self.view_mode = ViewMode::InfoMigration
        }

        loop {
            terminal.draw(|f| {
                self.render(
                    f,
                    f.size(),
                    &input,
                    &items,
                    &status_items,
                    &priority_items,
                    &delete_confirm_items,
                )
            })?;

            if let Event::Key(key) = event::read()? {
                // Capture only the "Press" event to prevent double input on Windows
                if key.kind == KeyEventKind::Press {
                    use KeyCode::*;
                    match self.view_mode {
                        ViewMode::ViewProjects => match key.code {
                            Char('h') => {
                                self.previous_view_mode = ViewMode::ViewProjects;
                                App::change_view(self, ViewMode::ViewHelp);
                            }
                            Enter | Right | Char('l') => {
                                if items.is_empty() {
                                    continue;
                                }

                                Task::load_items(self, &mut items);
                                self.selected_task_index.select(Some(0));

                                App::change_view(self, ViewMode::ViewTasks);
                            }
                            Char('r') => {
                                if items.is_empty() {
                                    continue;
                                }

                                input = input
                                    .clone()
                                    .with_value(Project::get_current(self).title.clone());

                                App::change_view(self, ViewMode::RenameProject);
                            }
                            Char('n') => {
                                input.reset();

                                App::change_view(self, ViewMode::AddProject);
                            }
                            Char('d') => {
                                if items.is_empty() {
                                    continue;
                                }

                                App::change_view(self, ViewMode::DeleteProject);
                            }
                            Down | Tab | Char('j') => {
                                self.next(&items);
                            }
                            Up | BackTab | Char('k') => {
                                self.previous(&items);
                            }
                            Char('q') => {
                                return Ok(());
                            }
                            _ => {}
                        },
                        ViewMode::RenameProject => match key.code {
                            Enter => {
                                Project::rename(self, &mut items, input.value());
                                input.reset();

                                App::change_view(self, ViewMode::ViewProjects);
                            }
                            Esc => {
                                input.reset();

                                App::change_view(self, ViewMode::ViewProjects);
                            }
                            _ => {
                                input.handle_event(&Event::Key(key));
                            }
                        },
                        ViewMode::AddProject => match key.code {
                            Esc => {
                                App::change_view(self, ViewMode::ViewProjects);
                            }
                            Enter => {
                                if !input.value().is_empty() {
                                    Project::create(self, &mut items, input.value());
                                    self.selected_project_index
                                        .select(Some(self.projects.len() - 1));
                                }

                                App::change_view(self, ViewMode::ViewProjects);
                            }
                            _ => {
                                input.handle_event(&Event::Key(key));
                            }
                        },
                        ViewMode::DeleteProject => {
                            if self.handle_modal_nav(
                                key.code,
                                &delete_confirm_items,
                                ViewMode::ViewProjects,
                            ) {
                                continue;
                            }
                            if key.code == Enter {
                                if self.delete_confirm_index.selected() == Some(0) {
                                    Project::delete(self, &mut items);
                                    self.selected_project_index.select_previous();
                                }
                                self.delete_confirm_index.select(Some(0));
                                App::change_view(self, ViewMode::ViewProjects);
                            }
                        }

                        ViewMode::ViewTasks => match key.code {
                            Char('h') => {
                                self.previous_view_mode = ViewMode::ViewTasks;
                                App::change_view(self, ViewMode::ViewHelp);
                            }
                            Esc | Left => {
                                Project::load_items(self, &mut items);

                                App::change_view(self, ViewMode::ViewProjects);
                            }
                            Enter => {
                                if items.is_empty() {
                                    continue;
                                }

                                let index = TASK_STATUSES
                                    .into_iter()
                                    .position(|t| t == &Task::get_current(self).status)
                                    .unwrap();

                                self.selected_status_task_index.select(Some(index));

                                App::change_view(self, ViewMode::ChangeStatusTask);
                            }
                            Char('p') => {
                                if items.is_empty() {
                                    continue;
                                }

                                let index = TASK_PRIORITIES
                                    .into_iter()
                                    .position(|t| t == Task::get_current(self).priority)
                                    .unwrap();

                                self.selected_priority_task_index.select(Some(index));

                                App::change_view(self, ViewMode::ChangePriorityTask);
                            }
                            Char('r') => {
                                if items.is_empty() {
                                    continue;
                                }

                                input = input
                                    .clone()
                                    .with_value(Task::get_current(self).title.clone());

                                App::change_view(self, ViewMode::RenameTask);
                            }
                            Char('n') => {
                                input.reset();

                                App::change_view(self, ViewMode::AddTask);
                            }
                            Char('d') => {
                                if items.is_empty() {
                                    continue;
                                }

                                App::change_view(self, ViewMode::DeleteTask);
                            }
                            Char('v') => {
                                if items.is_empty() {
                                    continue;
                                }

                                App::change_view(self, ViewMode::ViewTaskDetails);
                            }
                            Char('e') => {
                                if items.is_empty() {
                                    continue;
                                }

                                input = input
                                    .clone()
                                    .with_value(Task::get_current(self).note.clone());

                                App::change_view(self, ViewMode::EditTaskNote);
                            }
                            Down | Tab | Char('j') => {
                                self.next(&items);
                            }
                            Up | BackTab | Char('k') => {
                                self.previous(&items);
                            }
                            Char('t') => {
                                self.hide_done_tasks = !self.hide_done_tasks;
                                Task::load_items(self, &mut items);
                            }
                            Char('q') => {
                                return Ok(());
                            }
                            _ => {}
                        },
                        ViewMode::RenameTask => match key.code {
                            Enter => {
                                Task::rename(self, &mut items, input.value());
                                input.reset();

                                App::change_view(self, ViewMode::ViewTasks);
                            }
                            Esc => {
                                input.reset();

                                App::change_view(self, ViewMode::ViewTasks);
                            }
                            _ => {
                                input.handle_event(&Event::Key(key));
                            }
                        },
                        ViewMode::ChangeStatusTask => {
                            if self.handle_modal_nav(key.code, &status_items, ViewMode::ViewTasks) {
                                continue;
                            }
                            if key.code == Enter {
                                Task::change_status(
                                    self,
                                    &mut items,
                                    TASK_STATUSES
                                        [self.selected_status_task_index.selected().unwrap()],
                                );

                                self.selected_status_task_index.select(Some(0));
                                App::change_view(self, ViewMode::ViewTasks);
                            }
                        }
                        ViewMode::ChangePriorityTask => {
                            if self.handle_modal_nav(key.code, &priority_items, ViewMode::ViewTasks)
                            {
                                continue;
                            }
                            if key.code == Enter {
                                Task::change_priority(
                                    self,
                                    &mut items,
                                    TASK_PRIORITIES
                                        [self.selected_priority_task_index.selected().unwrap()],
                                );

                                self.selected_priority_task_index.select(Some(0));
                                App::change_view(self, ViewMode::ViewTasks);
                            }
                        }
                        ViewMode::AddTask => match key.code {
                            Enter => {
                                Task::create(self, &mut items, input.value());

                                App::change_view(self, ViewMode::ViewTasks);
                            }
                            Esc => {
                                App::change_view(self, ViewMode::ViewTasks);
                            }
                            _ => {
                                input.handle_event(&Event::Key(key));
                            }
                        },
                        ViewMode::DeleteTask => {
                            if self.handle_modal_nav(
                                key.code,
                                &delete_confirm_items,
                                ViewMode::ViewTasks,
                            ) {
                                continue;
                            }
                            if key.code == Enter {
                                if self.delete_confirm_index.selected() == Some(0) {
                                    Task::delete(self, &mut items);
                                    self.selected_task_index.select_previous();
                                }
                                self.delete_confirm_index.select(Some(0));
                                App::change_view(self, ViewMode::ViewTasks);
                            }
                        }
                        ViewMode::ViewTaskDetails => match key.code {
                            Char('e') => {
                                input = input
                                    .clone()
                                    .with_value(Task::get_current(self).note.clone());

                                App::change_view(self, ViewMode::EditTaskNote);
                            }
                            _ => {
                                App::change_view(self, ViewMode::ViewTasks);
                            }
                        },
                        ViewMode::EditTaskNote => match key.code {
                            Enter => {
                                Task::update_note(self, &mut items, input.value());
                                input.reset();

                                App::change_view(self, ViewMode::ViewTaskDetails);
                            }
                            Esc => {
                                input.reset();

                                App::change_view(self, ViewMode::ViewTaskDetails);
                            }
                            _ => {
                                input.handle_event(&Event::Key(key));
                            }
                        },

                        ViewMode::ViewHelp => match key.code {
                            _ => {
                                let prev = std::mem::replace(
                                    &mut self.previous_view_mode,
                                    ViewMode::ViewProjects,
                                );
                                App::change_view(self, prev);
                            }
                        },

                        ViewMode::InfoMigration => match key.code {
                            _ => {
                                App::change_view(self, ViewMode::ViewProjects);
                            }
                        },
                    }
                }
            }
        }
    }

    fn render(
        &mut self,
        f: &mut Frame,
        area: Rect,
        input: &Input,
        items: &Vec<ListItem>,
        status_items: &Vec<ListItem>,
        priority_items: &Vec<ListItem>,
        delete_confirm_items: &Vec<ListItem>,
    ) {
        let layout = Layout::vertical([
            Constraint::Percentage(2),
            Constraint::Percentage(96),
            Constraint::Percentage(2),
        ]);

        let [header_area, main_area, hint_area] = layout.areas(area);

        // Header
        f.render_widget(
            Paragraph::new(format!("::{}::", env!("CARGO_PKG_NAME"))).centered(),
            header_area,
        );

        // Hint
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " h  help",
                Style::default().fg(Color::Green),
            ))),
            hint_area,
        );

        // Main view
        View::show_items(self, items, f, main_area);

        // Other views
        if self.view_mode == ViewMode::InfoMigration {
            View::show_migration_info_modal(f, area);
        }

        if self.view_mode == ViewMode::AddTask || self.view_mode == ViewMode::AddProject {
            View::show_new_item_modal(f, area, input)
        }

        if self.view_mode == ViewMode::RenameTask || self.view_mode == ViewMode::RenameProject {
            View::show_rename_item_modal(f, area, input)
        }

        if self.view_mode == ViewMode::EditTaskNote {
            View::show_edit_note_modal(f, area, input)
        }

        if self.view_mode == ViewMode::DeleteTask || self.view_mode == ViewMode::DeleteProject {
            View::show_delete_item_modal(self, delete_confirm_items, f, area)
        }

        if self.view_mode == ViewMode::ChangeStatusTask {
            View::show_select_task_status_modal(self, status_items, f, area)
        }

        if self.view_mode == ViewMode::ChangePriorityTask {
            View::show_select_task_priority_modal(self, priority_items, f, area)
        }

        if self.view_mode == ViewMode::ViewHelp {
            View::show_help_modal(self, f, area)
        }

        if self.view_mode == ViewMode::ViewTaskDetails {
            View::show_task_details_modal(self, f, area)
        }
    }

    fn next(&mut self, items: &Vec<ListItem>) -> () {
        if items.is_empty() {
            return;
        }

        let i = match self.use_state().selected() {
            Some(i) => {
                if i >= items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };

        self.use_state().select(Some(i))
    }

    fn previous(&mut self, items: &Vec<ListItem>) {
        if items.is_empty() {
            return;
        }

        let i = match self.use_state().selected() {
            Some(i) => {
                if i == 0 {
                    items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };

        self.use_state().select(Some(i))
    }

    fn use_state(&mut self) -> &mut ListState {
        match self.view_mode {
            ViewMode::ViewProjects => return &mut self.selected_project_index,
            ViewMode::RenameProject => return &mut self.selected_project_index,
            ViewMode::AddProject => return &mut self.selected_project_index,
            ViewMode::DeleteProject => return &mut self.delete_confirm_index,

            ViewMode::ViewTasks => return &mut self.selected_task_index,
            ViewMode::RenameTask => return &mut self.selected_task_index,
            ViewMode::ChangeStatusTask => return &mut self.selected_status_task_index,
            ViewMode::ChangePriorityTask => return &mut self.selected_priority_task_index,
            ViewMode::AddTask => return &mut self.selected_task_index,
            ViewMode::DeleteTask => return &mut self.delete_confirm_index,
            ViewMode::ViewTaskDetails => return &mut self.selected_task_index,
            ViewMode::EditTaskNote => return &mut self.selected_task_index,

            ViewMode::ViewHelp => return &mut self.selected_project_index,
            ViewMode::InfoMigration => return &mut self.selected_project_index,
        };
    }

    fn change_view(&mut self, mode: ViewMode) {
        self.view_mode = mode
    }

    fn handle_modal_nav(
        &mut self,
        key: KeyCode,
        items: &Vec<ListItem>,
        return_mode: ViewMode,
    ) -> bool {
        match key {
            KeyCode::Esc => {
                self.use_state().select(Some(0));
                App::change_view(self, return_mode);
                true
            }
            KeyCode::Down | KeyCode::BackTab | KeyCode::Char('j') => {
                self.next(items);
                true
            }
            KeyCode::Up | KeyCode::Tab | KeyCode::Char('k') => {
                self.previous(items);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod property_tests;

#[cfg(test)]
pub(crate) mod test_utils {
    use super::*;
    use crate::task::{TASK_PRIORITY_NONE, TASK_STATUS_UP_NEXT};
    use std::sync::Mutex;
    use tempfile::TempDir;

    /// Serializes tests that mutate the process-wide `BASILK_CONFIG_DIR`
    /// env var and the JSON `VERSION` state.
    pub(crate) static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Points the JSON storage at a fresh temporary directory.
    /// The returned `TempDir` must be kept alive for the duration of the test.
    pub(crate) fn setup_temp_config() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::env::set_var("BASILK_CONFIG_DIR", dir.path());
        Json::check().unwrap();
        dir
    }

    pub(crate) fn make_app(projects: Vec<Project>) -> App {
        App {
            selected_project_index: ListState::default().with_selected(Some(0)),
            selected_task_index: ListState::default().with_selected(Some(0)),
            selected_status_task_index: ListState::default().with_selected(Some(0)),
            selected_priority_task_index: ListState::default().with_selected(Some(0)),
            delete_confirm_index: ListState::default().with_selected(Some(0)),
            view_mode: ViewMode::default(),
            previous_view_mode: ViewMode::default(),
            projects,
            hide_done_tasks: true,
        }
    }

    pub(crate) fn make_task(title: &str, status: &str, priority: u8) -> Task {
        Task {
            title: title.to_string(),
            status: status.to_string(),
            priority,
            created_at: Some(1_700_000_000),
            completed_at: None,
            note: String::new(),
        }
    }

    pub(crate) fn sample_projects() -> Vec<Project> {
        vec![
            Project {
                title: "alpha".to_string(),
                tasks: vec![
                    make_task("a1", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE),
                    make_task("a2", TASK_STATUS_UP_NEXT, 1),
                ],
            },
            Project {
                title: "beta".to_string(),
                tasks: vec![],
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_utils::make_app;

    #[test]
    fn next_on_empty_items_does_not_panic() {
        let mut app = make_app(vec![]);
        let items: Vec<ListItem> = vec![];

        app.next(&items);
        app.previous(&items);

        assert_eq!(app.selected_project_index.selected(), Some(0));
    }

    #[test]
    fn next_wraps_around_at_the_end() {
        let mut app = make_app(vec![]);
        let items: Vec<ListItem> = vec![ListItem::from("a"), ListItem::from("b")];

        app.next(&items);
        assert_eq!(app.selected_project_index.selected(), Some(1));

        app.next(&items);
        assert_eq!(app.selected_project_index.selected(), Some(0));
    }

    #[test]
    fn previous_wraps_around_at_the_start() {
        let mut app = make_app(vec![]);
        let items: Vec<ListItem> = vec![ListItem::from("a"), ListItem::from("b")];

        app.previous(&items);
        assert_eq!(app.selected_project_index.selected(), Some(1));
    }

    #[test]
    fn use_state_maps_view_modes_to_the_right_list_state() {
        fn assert_state(app: &mut App, mode: ViewMode, expected: fn(&App) -> *const ListState) {
            app.view_mode = mode;
            let actual = app.use_state() as *const ListState;
            assert_eq!(actual, expected(app));
        }

        let mut app = make_app(vec![]);

        assert_state(&mut app, ViewMode::ViewProjects, |a| {
            &a.selected_project_index
        });
        assert_state(&mut app, ViewMode::RenameProject, |a| {
            &a.selected_project_index
        });
        assert_state(&mut app, ViewMode::ViewTasks, |a| &a.selected_task_index);
        assert_state(&mut app, ViewMode::RenameTask, |a| &a.selected_task_index);
        assert_state(&mut app, ViewMode::ViewTaskDetails, |a| {
            &a.selected_task_index
        });
        assert_state(&mut app, ViewMode::ChangeStatusTask, |a| {
            &a.selected_status_task_index
        });
        assert_state(&mut app, ViewMode::ChangePriorityTask, |a| {
            &a.selected_priority_task_index
        });
        assert_state(&mut app, ViewMode::DeleteProject, |a| {
            &a.delete_confirm_index
        });
        assert_state(&mut app, ViewMode::DeleteTask, |a| &a.delete_confirm_index);
    }
}
