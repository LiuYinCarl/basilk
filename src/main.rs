use std::{
    error::Error,
    fmt::Debug,
    io::{self, stdout},
    time::Duration,
};

use cli::Cli;
use ratatui::{
    crossterm::{
        event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
        ExecutableCommand,
    },
    prelude::*,
    widgets::*,
};
use tui_input::{backend::crossterm::EventHandler, Input};

mod cli;
mod json;
mod markdown;
mod migration;
mod note;
mod project;
mod task;
mod timer;
mod ui;
mod util;
mod view;

use json::Json;
use note::Note;
use project::Project;
use task::{Task, TASK_PRIORITIES, TASK_STATUSES};
use timer::{TimerKind, TimerState};
use tui_textarea::TextArea;
use ui::Ui;
use util::Util;
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
    SetTaskEstimate,
    TimerTask,
    SetCountdown,
    ViewHelp,

    ViewNotes,
    AddNote,
    RenameNote,
    DeleteNote,
    ViewNote,
    EditNote,

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
    timer: Option<TimerState>,
    /// When true, the task view renders as a three-lane kanban board
    /// (Up Next / On Going / Done) instead of the classic list.
    board_view: bool,
    /// Currently focused board lane: index into `TASK_STATUSES`.
    board_lane: usize,
    /// Per-lane selection/scroll state for the board view.
    board_lane_states: [ListState; 3],
    /// Global notes, independent of projects.
    notes: Vec<Note>,
    selected_note_index: ListState,
    /// Vertical scroll offset of the note preview page.
    note_scroll: u16,
    /// Editor state while `ViewMode::EditNote` is active.
    note_textarea: Option<TextArea<'static>>,
}

/// What the event loop should do after a key press was handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyAction {
    /// No special action; continue the loop normally.
    None,
    /// Skip the rest of this iteration (mirrors the previous `continue`
    /// inside the event loop: modal navigation and empty-list guards
    /// redraw on the next iteration without ticking the timer).
    Skip,
    /// Quit the application; the caller settles the timer.
    Quit,
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
            timer: None,
            board_view: false,
            board_lane: 0,
            board_lane_states: [
                ListState::default().with_selected(Some(0)),
                ListState::default().with_selected(Some(0)),
                ListState::default().with_selected(Some(0)),
            ],
            notes: Json::read_notes(),
            selected_note_index: ListState::default().with_selected(Some(0)),
            note_scroll: 0,
            note_textarea: None,
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

            if event::poll(Duration::from_millis(250))? {
                if let Event::Key(key) = event::read()? {
                    // Capture only the "Press" event to prevent double input on Windows
                    if key.kind == KeyEventKind::Press {
                        match self.handle_key(
                            key,
                            &mut input,
                            &mut items,
                            &status_items,
                            &priority_items,
                            &delete_confirm_items,
                        ) {
                            KeyAction::Quit => {
                                self.settle_timer();
                                return Ok(());
                            }
                            KeyAction::Skip => continue,
                            KeyAction::None => {}
                        }
                    }
                }
            }

            self.tick_timer();
        }
    }

    /// Dispatch a pressed key to the handler of the current view mode.
    ///
    /// Each mode owns the keys it understands, so the event loop stays
    /// shallow instead of nesting every binding in one giant `match`.
    fn handle_key(
        &mut self,
        key: KeyEvent,
        input: &mut Input,
        items: &mut Vec<ListItem>,
        status_items: &Vec<ListItem>,
        priority_items: &Vec<ListItem>,
        delete_confirm_items: &Vec<ListItem>,
    ) -> KeyAction {
        match self.view_mode {
            ViewMode::ViewProjects => self.handle_view_projects(key, input, items),
            ViewMode::RenameProject => self.handle_rename_project(key, input, items),
            ViewMode::AddProject => self.handle_add_project(key, input, items),
            ViewMode::DeleteProject => self.handle_delete_project(key, items, delete_confirm_items),
            ViewMode::ViewTasks => self.handle_view_tasks(key, input, items),
            ViewMode::RenameTask => self.handle_rename_task(key, input, items),
            ViewMode::ChangeStatusTask => self.handle_change_status_task(key, items, status_items),
            ViewMode::ChangePriorityTask => {
                self.handle_change_priority_task(key, items, priority_items)
            }
            ViewMode::AddTask => self.handle_add_task(key, input, items),
            ViewMode::DeleteTask => self.handle_delete_task(key, items, delete_confirm_items),
            ViewMode::ViewTaskDetails => self.handle_view_task_details(key, input),
            ViewMode::EditTaskNote => self.handle_edit_task_note(key, input, items),
            ViewMode::SetTaskEstimate => self.handle_set_task_estimate(key, input, items),
            ViewMode::TimerTask => self.handle_timer_task(key),
            ViewMode::SetCountdown => self.handle_set_countdown(key, input),
            ViewMode::ViewHelp => {
                self.back_to_previous_view();
                KeyAction::None
            }
            ViewMode::ViewNotes => self.handle_view_notes(key, input, items),
            ViewMode::AddNote => self.handle_add_note(key, input, items),
            ViewMode::RenameNote => self.handle_rename_note(key, input, items),
            ViewMode::DeleteNote => self.handle_delete_note(key, items, delete_confirm_items),
            ViewMode::ViewNote => self.handle_view_note(key, items),
            ViewMode::EditNote => self.handle_edit_note(key),
            ViewMode::InfoMigration => {
                App::change_view(self, ViewMode::ViewProjects);
                KeyAction::None
            }
        }
    }

    fn handle_view_projects(
        &mut self,
        key: KeyEvent,
        input: &mut Input,
        items: &mut Vec<ListItem>,
    ) -> KeyAction {
        use KeyCode::*;
        match key.code {
            Char('h') => {
                self.previous_view_mode = ViewMode::ViewProjects;
                App::change_view(self, ViewMode::ViewHelp);
            }
            Enter | Right | Char('l') => {
                if items.is_empty() {
                    return KeyAction::Skip;
                }

                Task::load_items(self, items);
                self.selected_task_index.select(Some(0));

                // The sync inside `load_items` ran before the
                // selection was reset to the top
                if self.board_view {
                    self.board_sync();
                }

                App::change_view(self, ViewMode::ViewTasks);
            }
            Char('r') => {
                if items.is_empty() {
                    return KeyAction::Skip;
                }

                *input = input
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
                    return KeyAction::Skip;
                }

                App::change_view(self, ViewMode::DeleteProject);
            }
            Down | Tab | Char('j') => {
                self.next(items);
            }
            Up | BackTab | Char('k') => {
                self.previous(items);
            }
            Char('c') => {
                self.previous_view_mode = ViewMode::ViewProjects;

                if self.timer.is_some() {
                    App::change_view(self, ViewMode::TimerTask);
                } else {
                    input.reset();

                    App::change_view(self, ViewMode::SetCountdown);
                }
            }
            Char('m') => {
                Note::load_items(self, items);

                App::change_view(self, ViewMode::ViewNotes);
            }
            Char('q') => {
                return KeyAction::Quit;
            }
            _ => {}
        }
        KeyAction::None
    }

    fn handle_rename_project(
        &mut self,
        key: KeyEvent,
        input: &mut Input,
        items: &mut Vec<ListItem>,
    ) -> KeyAction {
        use KeyCode::*;
        match key.code {
            Enter => {
                Project::rename(self, items, input.value());
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
        }
        KeyAction::None
    }

    fn handle_add_project(
        &mut self,
        key: KeyEvent,
        input: &mut Input,
        items: &mut Vec<ListItem>,
    ) -> KeyAction {
        use KeyCode::*;
        match key.code {
            Esc => {
                App::change_view(self, ViewMode::ViewProjects);
            }
            Enter => {
                if !input.value().is_empty() {
                    Project::create(self, items, input.value());
                    self.selected_project_index
                        .select(Some(self.projects.len() - 1));
                }

                App::change_view(self, ViewMode::ViewProjects);
            }
            _ => {
                input.handle_event(&Event::Key(key));
            }
        }
        KeyAction::None
    }

    fn handle_delete_project(
        &mut self,
        key: KeyEvent,
        items: &mut Vec<ListItem>,
        delete_confirm_items: &Vec<ListItem>,
    ) -> KeyAction {
        if self.handle_modal_nav(key.code, delete_confirm_items, ViewMode::ViewProjects) {
            return KeyAction::Skip;
        }
        if key.code == KeyCode::Enter {
            if self.delete_confirm_index.selected() == Some(0) {
                let deleted_index = self.selected_project_index.selected().unwrap();

                Project::delete(self, items);
                self.selected_project_index.select_previous();

                // Keep the timer binding consistent: a timer on the
                // deleted project is dropped, later indexes shift down
                let bound_index = self
                    .timer
                    .as_ref()
                    .and_then(|t| t.bound.as_ref())
                    .map(|b| b.project_index);

                if bound_index == Some(deleted_index) {
                    self.timer = None;
                } else if let Some(timer) = self.timer.as_mut() {
                    if let Some(bound) = timer.bound.as_mut() {
                        if bound.project_index > deleted_index {
                            bound.project_index -= 1;
                        }
                    }
                }
            }
            self.delete_confirm_index.select(Some(0));
            App::change_view(self, ViewMode::ViewProjects);
        }
        KeyAction::None
    }

    fn handle_view_tasks(
        &mut self,
        key: KeyEvent,
        input: &mut Input,
        items: &mut Vec<ListItem>,
    ) -> KeyAction {
        use KeyCode::*;
        // In board mode the focused lane (not the list) decides
        // whether task actions are available: the list can be
        // empty only because done tasks are hidden, while the
        // Done lane still shows them.
        let no_current_task = if self.board_view {
            self.board_lane_is_empty()
        } else {
            items.is_empty()
        };

        match key.code {
            Char('h') => {
                self.previous_view_mode = ViewMode::ViewTasks;
                App::change_view(self, ViewMode::ViewHelp);
            }
            Esc => {
                Project::load_items(self, items);

                App::change_view(self, ViewMode::ViewProjects);
            }
            Left => {
                if self.board_view {
                    self.board_switch_lane(false);
                } else {
                    Project::load_items(self, items);

                    App::change_view(self, ViewMode::ViewProjects);
                }
            }
            Right => {
                if self.board_view {
                    self.board_switch_lane(true);
                }
            }
            Char('b') => {
                self.board_view = !self.board_view;

                // Rebuild the item list: the board shows
                // done tasks, the list view may hide them.
                // When toggling on, `load_items` also
                // re-syncs the board focus.
                Task::load_items(self, items);
            }
            Enter => {
                if no_current_task {
                    return KeyAction::Skip;
                }

                let index = TASK_STATUSES
                    .into_iter()
                    .position(|t| t == Task::get_current(self).status)
                    .unwrap();

                self.selected_status_task_index.select(Some(index));

                App::change_view(self, ViewMode::ChangeStatusTask);
            }
            Char('p') => {
                if no_current_task {
                    return KeyAction::Skip;
                }

                let index = TASK_PRIORITIES
                    .into_iter()
                    .position(|t| t == Task::get_current(self).priority)
                    .unwrap();

                self.selected_priority_task_index.select(Some(index));

                App::change_view(self, ViewMode::ChangePriorityTask);
            }
            Char('r') => {
                if no_current_task {
                    return KeyAction::Skip;
                }

                *input = input
                    .clone()
                    .with_value(Task::get_current(self).title.clone());

                App::change_view(self, ViewMode::RenameTask);
            }
            Char('n') => {
                input.reset();

                App::change_view(self, ViewMode::AddTask);
            }
            Char('d') => {
                if no_current_task {
                    return KeyAction::Skip;
                }

                App::change_view(self, ViewMode::DeleteTask);
            }
            Char('v') => {
                if no_current_task {
                    return KeyAction::Skip;
                }

                App::change_view(self, ViewMode::ViewTaskDetails);
            }
            Char('e') => {
                if no_current_task {
                    return KeyAction::Skip;
                }

                *input = input
                    .clone()
                    .with_value(Task::get_current(self).note.clone());

                App::change_view(self, ViewMode::EditTaskNote);
            }
            Down | Tab | Char('j') => {
                if self.board_view {
                    self.board_move(true);
                } else {
                    self.next(items);
                }
            }
            Up | BackTab | Char('k') => {
                if self.board_view {
                    self.board_move(false);
                } else {
                    self.previous(items);
                }
            }
            Char('t') => {
                // The board always shows the Done lane, so there
                // is nothing to toggle while it is active
                if !self.board_view {
                    self.hide_done_tasks = !self.hide_done_tasks;
                    Task::load_items(self, items);
                }
            }
            Char('s') => {
                self.previous_view_mode = ViewMode::ViewTasks;

                if self.timer.is_some() {
                    App::change_view(self, ViewMode::TimerTask);
                } else if !no_current_task {
                    let project_index = self.selected_project_index.selected().unwrap();
                    let task_title = Task::get_current(self).title.clone();

                    self.timer = Some(TimerState::new_stopwatch(project_index, task_title));

                    App::change_view(self, ViewMode::TimerTask);
                }
            }
            Char('c') => {
                self.previous_view_mode = ViewMode::ViewTasks;

                if self.timer.is_some() {
                    App::change_view(self, ViewMode::TimerTask);
                } else {
                    input.reset();

                    App::change_view(self, ViewMode::SetCountdown);
                }
            }
            Char('q') => {
                return KeyAction::Quit;
            }
            _ => {}
        }
        KeyAction::None
    }

    fn handle_rename_task(
        &mut self,
        key: KeyEvent,
        input: &mut Input,
        items: &mut Vec<ListItem>,
    ) -> KeyAction {
        use KeyCode::*;
        match key.code {
            Enter => {
                let project_index = self.selected_project_index.selected().unwrap();
                let old_title = Task::get_current(self).title.clone();
                let new_title = input.value().to_string();

                Task::rename(self, items, &new_title);
                input.reset();

                // Keep a running timer bound to the renamed task
                if let Some(timer) = self.timer.as_mut() {
                    if timer.is_bound_to(project_index, &old_title) {
                        if let Some(bound) = timer.bound.as_mut() {
                            bound.task_title = new_title;
                        }
                    }
                }

                App::change_view(self, ViewMode::ViewTasks);
            }
            Esc => {
                input.reset();

                App::change_view(self, ViewMode::ViewTasks);
            }
            _ => {
                input.handle_event(&Event::Key(key));
            }
        }
        KeyAction::None
    }

    fn handle_change_status_task(
        &mut self,
        key: KeyEvent,
        items: &mut Vec<ListItem>,
        status_items: &Vec<ListItem>,
    ) -> KeyAction {
        if self.handle_modal_nav(key.code, status_items, ViewMode::ViewTasks) {
            return KeyAction::Skip;
        }
        if key.code == KeyCode::Enter {
            Task::change_status(
                self,
                items,
                TASK_STATUSES[self.selected_status_task_index.selected().unwrap()],
            );

            self.selected_status_task_index.select(Some(0));
            App::change_view(self, ViewMode::ViewTasks);
        }
        KeyAction::None
    }

    fn handle_change_priority_task(
        &mut self,
        key: KeyEvent,
        items: &mut Vec<ListItem>,
        priority_items: &Vec<ListItem>,
    ) -> KeyAction {
        if self.handle_modal_nav(key.code, priority_items, ViewMode::ViewTasks) {
            return KeyAction::Skip;
        }
        if key.code == KeyCode::Enter {
            Task::change_priority(
                self,
                items,
                TASK_PRIORITIES[self.selected_priority_task_index.selected().unwrap()],
            );

            self.selected_priority_task_index.select(Some(0));
            App::change_view(self, ViewMode::ViewTasks);
        }
        KeyAction::None
    }

    fn handle_add_task(
        &mut self,
        key: KeyEvent,
        input: &mut Input,
        items: &mut Vec<ListItem>,
    ) -> KeyAction {
        use KeyCode::*;
        match key.code {
            Enter => {
                Task::create(self, items, input.value());

                App::change_view(self, ViewMode::ViewTasks);
            }
            Esc => {
                App::change_view(self, ViewMode::ViewTasks);
            }
            _ => {
                input.handle_event(&Event::Key(key));
            }
        }
        KeyAction::None
    }

    fn handle_delete_task(
        &mut self,
        key: KeyEvent,
        items: &mut Vec<ListItem>,
        delete_confirm_items: &Vec<ListItem>,
    ) -> KeyAction {
        if self.handle_modal_nav(key.code, delete_confirm_items, ViewMode::ViewTasks) {
            return KeyAction::Skip;
        }
        if key.code == KeyCode::Enter {
            if self.delete_confirm_index.selected() == Some(0) {
                // Drop a timer bound to the task being deleted
                let project_index = self.selected_project_index.selected().unwrap();
                let task_title = Task::get_current(self).title.clone();
                let bound = matches!(
                    self.timer.as_ref(),
                    Some(t) if t.is_bound_to(project_index, &task_title)
                );
                if bound {
                    self.timer = None;
                }

                self.delete_current_task(items);
            }
            self.delete_confirm_index.select(Some(0));
            App::change_view(self, ViewMode::ViewTasks);
        }
        KeyAction::None
    }

    fn handle_view_task_details(&mut self, key: KeyEvent, input: &mut Input) -> KeyAction {
        use KeyCode::*;
        match key.code {
            Char('e') => {
                *input = input
                    .clone()
                    .with_value(Task::get_current(self).note.clone());

                App::change_view(self, ViewMode::EditTaskNote);
            }
            Char('g') => {
                // Prefill the current estimate; an empty field
                // is less friction than a "0" to delete first
                let current = Task::get_current(self).estimated_hours;
                *input = input.clone().with_value(if current > 0 {
                    current.to_string()
                } else {
                    String::new()
                });

                App::change_view(self, ViewMode::SetTaskEstimate);
            }
            _ => {
                App::change_view(self, ViewMode::ViewTasks);
            }
        }
        KeyAction::None
    }

    fn handle_edit_task_note(
        &mut self,
        key: KeyEvent,
        input: &mut Input,
        items: &mut Vec<ListItem>,
    ) -> KeyAction {
        use KeyCode::*;
        match key.code {
            Enter => {
                Task::update_note(self, items, input.value());
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
        }
        KeyAction::None
    }

    fn handle_set_task_estimate(
        &mut self,
        key: KeyEvent,
        input: &mut Input,
        items: &mut Vec<ListItem>,
    ) -> KeyAction {
        use KeyCode::*;
        match key.code {
            Enter => {
                let raw = input.value().trim().to_string();

                if let Ok(hours) = raw.parse::<u64>() {
                    Task::update_estimate(self, items, hours);
                    input.reset();

                    App::change_view(self, ViewMode::ViewTaskDetails);
                } else if raw.is_empty() {
                    input.reset();

                    App::change_view(self, ViewMode::ViewTaskDetails);
                }
                // Invalid non-empty input: stay in the modal
            }
            Esc => {
                input.reset();

                App::change_view(self, ViewMode::ViewTaskDetails);
            }
            // Only digits make sense here
            Char(c) if !c.is_ascii_digit() => {}
            _ => {
                input.handle_event(&Event::Key(key));
            }
        }
        KeyAction::None
    }

    fn handle_timer_task(&mut self, key: KeyEvent) -> KeyAction {
        if self.timer.as_ref().is_some_and(TimerState::is_finished) {
            // Finished countdown stays on screen; any key dismisses it
            self.settle_timer();
            self.back_to_previous_view();
            return KeyAction::Skip;
        }

        use KeyCode::*;
        match key.code {
            Char(' ') => {
                if let Some(timer) = self.timer.as_mut() {
                    if timer.is_running() {
                        timer.pause();
                    } else {
                        timer.resume();
                    }
                }
            }
            Enter => {
                self.settle_timer();
                self.back_to_previous_view();
            }
            Esc => {
                self.back_to_previous_view();
            }
            _ => {}
        }
        KeyAction::None
    }

    fn handle_set_countdown(&mut self, key: KeyEvent, input: &mut Input) -> KeyAction {
        use KeyCode::*;
        match key.code {
            Enter => {
                let raw = input.value().trim().to_string();
                let minutes = raw.parse::<f64>().unwrap_or(0.0);
                let secs = (minutes * 60.0).round() as u64;

                if secs > 0 {
                    input.reset();

                    self.timer = Some(TimerState::new_countdown(secs));

                    App::change_view(self, ViewMode::TimerTask);
                } else if raw.is_empty() {
                    input.reset();

                    self.back_to_previous_view();
                }
                // Invalid non-empty input: stay in the modal
            }
            Esc => {
                input.reset();

                self.back_to_previous_view();
            }
            // Only digits and a decimal point make sense here
            Char(c) if !c.is_ascii_digit() && c != '.' => {}
            _ => {
                input.handle_event(&Event::Key(key));
            }
        }
        KeyAction::None
    }

    fn handle_view_notes(
        &mut self,
        key: KeyEvent,
        input: &mut Input,
        items: &mut Vec<ListItem>,
    ) -> KeyAction {
        use KeyCode::*;
        match key.code {
            Char('h') => {
                self.previous_view_mode = ViewMode::ViewNotes;
                App::change_view(self, ViewMode::ViewHelp);
            }
            Esc | Left => {
                Project::load_items(self, items);

                App::change_view(self, ViewMode::ViewProjects);
            }
            Enter | Right | Char('l') | Char('v') => {
                if items.is_empty() {
                    return KeyAction::Skip;
                }

                self.note_scroll = 0;

                App::change_view(self, ViewMode::ViewNote);
            }
            Char('n') => {
                input.reset();

                App::change_view(self, ViewMode::AddNote);
            }
            Char('r') => {
                if items.is_empty() {
                    return KeyAction::Skip;
                }

                *input = input
                    .clone()
                    .with_value(Note::get_current(self).title.clone());

                App::change_view(self, ViewMode::RenameNote);
            }
            Char('d') => {
                if items.is_empty() {
                    return KeyAction::Skip;
                }

                App::change_view(self, ViewMode::DeleteNote);
            }
            Down | Tab | Char('j') => {
                self.next(items);
            }
            Up | BackTab | Char('k') => {
                self.previous(items);
            }
            Char('q') => {
                return KeyAction::Quit;
            }
            _ => {}
        }
        KeyAction::None
    }

    fn handle_add_note(
        &mut self,
        key: KeyEvent,
        input: &mut Input,
        items: &mut Vec<ListItem>,
    ) -> KeyAction {
        use KeyCode::*;
        match key.code {
            Enter => {
                if !input.value().is_empty() {
                    Note::create(self, items, input.value());
                    input.reset();

                    self.selected_note_index
                        .select(Some(self.notes.len().saturating_sub(1)));
                }

                App::change_view(self, ViewMode::ViewNotes);
            }
            Esc => {
                input.reset();

                App::change_view(self, ViewMode::ViewNotes);
            }
            _ => {
                input.handle_event(&Event::Key(key));
            }
        }
        KeyAction::None
    }

    fn handle_rename_note(
        &mut self,
        key: KeyEvent,
        input: &mut Input,
        items: &mut Vec<ListItem>,
    ) -> KeyAction {
        use KeyCode::*;
        match key.code {
            Enter => {
                Note::rename(self, items, input.value());
                input.reset();

                App::change_view(self, ViewMode::ViewNotes);
            }
            Esc => {
                input.reset();

                App::change_view(self, ViewMode::ViewNotes);
            }
            _ => {
                input.handle_event(&Event::Key(key));
            }
        }
        KeyAction::None
    }

    fn handle_delete_note(
        &mut self,
        key: KeyEvent,
        items: &mut Vec<ListItem>,
        delete_confirm_items: &Vec<ListItem>,
    ) -> KeyAction {
        if self.handle_modal_nav(key.code, delete_confirm_items, ViewMode::ViewNotes) {
            return KeyAction::Skip;
        }
        if key.code == KeyCode::Enter {
            if self.delete_confirm_index.selected() == Some(0) {
                Note::delete(self, items);
            }
            self.delete_confirm_index.select(Some(0));
            App::change_view(self, ViewMode::ViewNotes);
        }
        KeyAction::None
    }

    /// Full-page Markdown preview: scroll keys adjust `note_scroll`
    /// (clamped against the wrapped content height at render time).
    fn handle_view_note(&mut self, key: KeyEvent, items: &mut Vec<ListItem>) -> KeyAction {
        use KeyCode::*;
        match key.code {
            Char('h') => {
                self.previous_view_mode = ViewMode::ViewNote;
                App::change_view(self, ViewMode::ViewHelp);
            }
            Char('e') => {
                let body = Note::get_current(self).body.clone();
                let mut lines: Vec<String> = body.lines().map(|l| l.to_string()).collect();
                if lines.is_empty() {
                    lines.push(String::new());
                }

                let mut textarea = TextArea::from(lines);
                textarea.set_block(Block::bordered().title(" Edit Note — Esc: save & back "));
                self.note_textarea = Some(textarea);

                App::change_view(self, ViewMode::EditNote);
            }
            Down | Char('j') => {
                self.note_scroll = self.note_scroll.saturating_add(1);
            }
            Up | Char('k') => {
                self.note_scroll = self.note_scroll.saturating_sub(1);
            }
            PageDown => {
                self.note_scroll = self.note_scroll.saturating_add(10);
            }
            PageUp => {
                self.note_scroll = self.note_scroll.saturating_sub(10);
            }
            Home | Char('g') => {
                self.note_scroll = 0;
            }
            End | Char('G') => {
                // Clamped to the real bottom when rendering
                self.note_scroll = u16::MAX;
            }
            Esc | Enter => {
                // The body may have changed in the editor; refresh the
                // list so the snippet is up to date
                Note::load_items(self, items);

                App::change_view(self, ViewMode::ViewNotes);
            }
            Char('q') => {
                return KeyAction::Quit;
            }
            _ => {}
        }
        KeyAction::None
    }

    /// Full-page editor: every key except Esc goes to the textarea;
    /// Esc saves the body and returns to the rendered preview.
    fn handle_edit_note(&mut self, key: KeyEvent) -> KeyAction {
        if key.code == KeyCode::Esc {
            if let Some(textarea) = self.note_textarea.take() {
                let body = textarea.into_lines().join("\n");
                // Skip the write (and the `updated_at` bump) when nothing changed
                if body != Note::get_current(self).body {
                    Note::update_body(self, &body);
                }
            }

            App::change_view(self, ViewMode::ViewNote);
            return KeyAction::None;
        }

        if let Some(textarea) = self.note_textarea.as_mut() {
            textarea.input(key);
        }
        KeyAction::None
    }

    #[allow(clippy::too_many_arguments)]
    fn render(
        &mut self,
        f: &mut Frame,
        area: Rect,
        input: &Input,
        items: &[ListItem],
        status_items: &[ListItem],
        priority_items: &[ListItem],
        delete_confirm_items: &[ListItem],
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

        // Hint and timer readout
        self.render_hint(f, hint_area);

        // Main view: the note preview and editor take the whole main area
        match self.view_mode {
            ViewMode::ViewNote => View::show_note(self, f, main_area),
            ViewMode::EditNote => View::show_note_editor(self, f, main_area),
            _ => View::show_items(self, items, f, main_area),
        }

        // Modal on top of the current view, if any
        self.render_modal(
            f,
            area,
            input,
            status_items,
            priority_items,
            delete_confirm_items,
        );
    }

    /// The hint line, plus the running timer readout (right aligned).
    fn render_hint(&self, f: &mut Frame, hint_area: Rect) {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " h  help",
                Style::default().fg(Color::Green),
            ))),
            hint_area,
        );

        if let Some(timer) = &self.timer {
            let secs = match timer.kind {
                TimerKind::Stopwatch => timer.elapsed().as_secs(),
                TimerKind::Countdown => timer.remaining().unwrap_or_default().as_secs(),
            };

            let icon = match timer.kind {
                TimerKind::Stopwatch => {
                    if timer.is_running() {
                        "▶"
                    } else {
                        "❚❚"
                    }
                }
                TimerKind::Countdown => "▼",
            };

            let color = if !timer.is_running() {
                Color::Yellow
            } else if timer.kind == TimerKind::Countdown && secs <= 10 {
                Color::Red
            } else {
                Color::Green
            };

            // Keep the readout short enough to not overlap the help hint
            let label = match &timer.bound {
                Some(bound) => {
                    let title: String = bound.task_title.chars().take(20).collect();
                    let ellipsis = if bound.task_title.chars().count() > 20 {
                        "…"
                    } else {
                        ""
                    };
                    format!("{}{}", title, ellipsis)
                }
                None => "pomodoro".to_string(),
            };

            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("{} {} {}", icon, Util::format_secs(secs), label),
                    Style::default().fg(color),
                )))
                .alignment(Alignment::Right),
                hint_area,
            );
        }
    }

    /// Render the modal for the current view mode, if any.
    fn render_modal(
        &mut self,
        f: &mut Frame,
        area: Rect,
        input: &Input,
        status_items: &[ListItem],
        priority_items: &[ListItem],
        delete_confirm_items: &[ListItem],
    ) {
        match self.view_mode {
            ViewMode::InfoMigration => View::show_migration_info_modal(f, area),
            ViewMode::AddTask | ViewMode::AddProject | ViewMode::AddNote => {
                View::show_new_item_modal(f, area, input)
            }
            ViewMode::RenameTask | ViewMode::RenameProject | ViewMode::RenameNote => {
                View::show_rename_item_modal(f, area, input)
            }
            ViewMode::EditTaskNote => View::show_edit_note_modal(f, area, input),
            ViewMode::SetCountdown => View::show_countdown_modal(f, area, input),
            ViewMode::SetTaskEstimate => View::show_task_estimate_modal(f, area, input),
            ViewMode::TimerTask => View::show_timer_modal(self, f, area),
            ViewMode::DeleteTask | ViewMode::DeleteProject | ViewMode::DeleteNote => {
                View::show_delete_item_modal(self, delete_confirm_items, f, area)
            }
            ViewMode::ChangeStatusTask => {
                View::show_select_task_status_modal(self, status_items, f, area)
            }
            ViewMode::ChangePriorityTask => {
                View::show_select_task_priority_modal(self, priority_items, f, area)
            }
            ViewMode::ViewHelp => View::show_help_modal(self, f, area),
            ViewMode::ViewTaskDetails => View::show_task_details_modal(self, f, area),
            _ => {}
        }
    }

    fn next(&mut self, items: &[ListItem]) {
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

    fn previous(&mut self, items: &[ListItem]) {
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

    /// Delete the selected task, move the selection to the previous one,
    /// and keep the board focus consistent with the action target
    /// (`select_previous` runs after the sync inside `Task::load_items`,
    /// so the board must be re-synced afterwards).
    fn delete_current_task(&mut self, items: &mut Vec<ListItem>) {
        Task::delete(self, items);
        self.selected_task_index.select_previous();

        if self.board_view {
            self.board_sync();
        }
    }

    /// Number of tasks in a board lane.
    fn board_lane_len(&self, lane: usize) -> usize {
        Task::lane_indices(self, TASK_STATUSES[lane]).len()
    }

    /// Whether the currently focused board lane has no tasks.
    fn board_lane_is_empty(&self) -> bool {
        self.board_lane_len(self.board_lane) == 0
    }

    /// Derive the focused lane and the per-lane selection from
    /// `selected_task_index`. Called at the end of `Task::load_items`
    /// while the board is active, so the board follows a task that
    /// changed lane (status change), and when the board view is
    /// (re)entered.
    fn board_sync(&mut self) {
        let Some(selected) = self.selected_task_index.selected() else {
            return;
        };
        let Some(project) = self
            .projects
            .get(self.selected_project_index.selected().unwrap_or(0))
        else {
            return;
        };
        let Some(task) = project.tasks.get(selected) else {
            return;
        };
        let Some(lane) = TASK_STATUSES.into_iter().position(|s| s == task.status) else {
            return;
        };

        self.board_lane = lane;

        let row = Task::lane_indices(self, TASK_STATUSES[lane])
            .iter()
            .position(|&i| i == selected)
            .unwrap_or(0);
        self.board_lane_states[lane].select(Some(row));
    }

    /// Focus the previous/next board lane (wrapping) and move the task
    /// selection to the remembered row of that lane. Switching to an
    /// empty lane only moves the focus; task actions are guarded by
    /// `board_lane_is_empty`.
    fn board_switch_lane(&mut self, forward: bool) {
        let lane_count = TASK_STATUSES.len();
        let lane = if forward {
            (self.board_lane + 1) % lane_count
        } else {
            (self.board_lane + lane_count - 1) % lane_count
        };
        self.board_lane = lane;

        let indices = Task::lane_indices(self, TASK_STATUSES[lane]);
        if indices.is_empty() {
            return;
        }

        let row = self.board_lane_states[lane]
            .selected()
            .unwrap_or(0)
            .min(indices.len() - 1);
        self.board_lane_states[lane].select(Some(row));
        self.selected_task_index.select(Some(indices[row]));
    }

    /// Move the selection one row up/down within the focused lane
    /// (wrapping), keeping `selected_task_index` pointed at the same task.
    fn board_move(&mut self, down: bool) {
        let lane = self.board_lane;
        let indices = Task::lane_indices(self, TASK_STATUSES[lane]);
        if indices.is_empty() {
            return;
        }

        let row = self.board_lane_states[lane]
            .selected()
            .unwrap_or(0)
            .min(indices.len() - 1);
        let row = if down {
            if row >= indices.len() - 1 {
                0
            } else {
                row + 1
            }
        } else if row == 0 {
            indices.len() - 1
        } else {
            row - 1
        };

        self.board_lane_states[lane].select(Some(row));
        self.selected_task_index.select(Some(indices[row]));
    }

    fn use_state(&mut self) -> &mut ListState {
        match self.view_mode {
            ViewMode::ViewProjects => &mut self.selected_project_index,
            ViewMode::RenameProject => &mut self.selected_project_index,
            ViewMode::AddProject => &mut self.selected_project_index,
            ViewMode::DeleteProject => &mut self.delete_confirm_index,

            ViewMode::ViewTasks => &mut self.selected_task_index,
            ViewMode::RenameTask => &mut self.selected_task_index,
            ViewMode::ChangeStatusTask => &mut self.selected_status_task_index,
            ViewMode::ChangePriorityTask => &mut self.selected_priority_task_index,
            ViewMode::AddTask => &mut self.selected_task_index,
            ViewMode::DeleteTask => &mut self.delete_confirm_index,
            ViewMode::ViewTaskDetails => &mut self.selected_task_index,
            ViewMode::EditTaskNote => &mut self.selected_task_index,
            // Timer modals can be opened from either list view; the list
            // underneath keeps the selection state of the originating view
            ViewMode::TimerTask | ViewMode::SetCountdown => {
                if self.previous_view_mode == ViewMode::ViewProjects {
                    return &mut self.selected_project_index;
                }
                &mut self.selected_task_index
            }
            ViewMode::SetTaskEstimate => &mut self.selected_task_index,

            ViewMode::ViewNotes => &mut self.selected_note_index,
            ViewMode::AddNote => &mut self.selected_note_index,
            ViewMode::RenameNote => &mut self.selected_note_index,
            ViewMode::DeleteNote => &mut self.delete_confirm_index,
            ViewMode::ViewNote => &mut self.selected_note_index,
            ViewMode::EditNote => &mut self.selected_note_index,

            ViewMode::ViewHelp => &mut self.selected_project_index,
            ViewMode::InfoMigration => &mut self.selected_project_index,
        }
    }

    fn change_view(&mut self, mode: ViewMode) {
        self.view_mode = mode
    }

    /// Stop the active timer (if any). A stopwatch accumulates its seconds
    /// into the bound task; a pomodoro countdown is simply discarded.
    fn settle_timer(&mut self) {
        if let Some(timer) = self.timer.take() {
            if let Some(bound) = &timer.bound {
                Task::add_time_spent(
                    self,
                    bound.project_index,
                    &bound.task_title,
                    timer.elapsed().as_secs(),
                );
            }
        }
    }

    /// Return from a modal to whichever list view opened it.
    fn back_to_previous_view(&mut self) {
        let prev = std::mem::replace(&mut self.previous_view_mode, ViewMode::ViewProjects);
        App::change_view(self, prev);
    }

    /// Countdown bookkeeping, run on every event-loop iteration: when the
    /// pomodoro reaches zero, ring the terminal bell once and leave the
    /// timer (and its modal, if open) in the finished state until the
    /// user dismisses it with any key.
    fn tick_timer(&mut self) {
        let hit_zero = matches!(
            self.timer.as_ref(),
            Some(t) if t.is_finished() && !t.rung
        );

        if !hit_zero {
            return;
        }

        print!("\x07");
        let _ = std::io::Write::flush(&mut std::io::stdout());

        if let Some(timer) = self.timer.as_mut() {
            timer.rung = true;
        }
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
            timer: None,
            board_view: false,
            board_lane: 0,
            board_lane_states: [
                ListState::default().with_selected(Some(0)),
                ListState::default().with_selected(Some(0)),
                ListState::default().with_selected(Some(0)),
            ],
            notes: vec![],
            selected_note_index: ListState::default().with_selected(Some(0)),
            note_scroll: 0,
            note_textarea: None,
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
            time_spent_secs: 0,
            estimated_hours: 0,
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
        assert_state(&mut app, ViewMode::ViewNotes, |a| &a.selected_note_index);
        assert_state(&mut app, ViewMode::AddNote, |a| &a.selected_note_index);
        assert_state(&mut app, ViewMode::RenameNote, |a| &a.selected_note_index);
        assert_state(&mut app, ViewMode::ViewNote, |a| &a.selected_note_index);
        assert_state(&mut app, ViewMode::EditNote, |a| &a.selected_note_index);
        assert_state(&mut app, ViewMode::DeleteNote, |a| &a.delete_confirm_index);
    }

    mod board {
        use super::*;
        use crate::task::{
            TASK_PRIORITY_NONE, TASK_STATUS_DONE, TASK_STATUS_ON_GOING, TASK_STATUS_UP_NEXT,
        };
        use test_utils::{make_task, setup_temp_config, ENV_LOCK};

        fn board_app() -> App {
            make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![
                    make_task("ongoing", TASK_STATUS_ON_GOING, TASK_PRIORITY_NONE),
                    make_task("upnext", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE),
                    make_task("done", TASK_STATUS_DONE, TASK_PRIORITY_NONE),
                ],
            }])
        }

        #[test]
        fn board_sync_focuses_the_lane_of_the_selected_task() {
            let mut app = board_app();
            app.selected_task_index.select(Some(0)); // "ongoing"

            app.board_sync();

            // TASK_STATUSES order: UpNext = 0, OnGoing = 1, Done = 2
            assert_eq!(app.board_lane, 1);
            assert_eq!(app.board_lane_states[1].selected(), Some(0));
        }

        #[test]
        fn board_sync_follows_a_task_that_changed_lane() {
            let mut app = board_app();
            app.board_view = true;
            app.selected_task_index.select(Some(0));
            app.projects[0].tasks[0].status = TASK_STATUS_DONE.to_string();

            let mut items = vec![];
            Task::load_items(&mut app, &mut items);

            // The selection follows the task into the Done lane instead of
            // jumping to the first still-visible list item
            assert_eq!(Task::get_current(&mut app).title, "ongoing");
            assert_eq!(app.board_lane, 2);
            assert_eq!(app.selected_task_index.selected(), Some(1));
            assert_eq!(app.board_lane_states[2].selected(), Some(0));
        }

        #[test]
        fn lane_indices_groups_by_status_and_ignores_hide_done() {
            let app = board_app();
            assert!(app.hide_done_tasks);

            assert_eq!(Task::lane_indices(&app, TASK_STATUS_UP_NEXT), vec![1]);
            assert_eq!(Task::lane_indices(&app, TASK_STATUS_ON_GOING), vec![0]);
            assert_eq!(Task::lane_indices(&app, TASK_STATUS_DONE), vec![2]);
        }

        #[test]
        fn board_switch_lane_wraps_and_moves_the_task_selection() {
            let mut app = board_app();
            app.selected_task_index.select(Some(0));
            app.board_sync(); // lane 1 (OnGoing)

            app.board_switch_lane(true); // lane 2 (Done)
            assert_eq!(app.board_lane, 2);
            assert_eq!(app.selected_task_index.selected(), Some(2));

            app.board_switch_lane(true); // wraps to lane 0 (UpNext)
            assert_eq!(app.board_lane, 0);
            assert_eq!(app.selected_task_index.selected(), Some(1));

            app.board_switch_lane(false); // back to lane 2 (Done)
            assert_eq!(app.board_lane, 2);
            assert_eq!(app.selected_task_index.selected(), Some(2));
        }

        #[test]
        fn board_switch_lane_into_an_empty_lane_keeps_the_task_selection() {
            let mut app = board_app();
            app.projects[0]
                .tasks
                .retain(|t| t.status != TASK_STATUS_ON_GOING);
            app.selected_task_index.select(Some(0)); // "upnext"
            app.board_sync();
            assert_eq!(app.board_lane, 0);

            app.board_switch_lane(true); // OnGoing lane, now empty

            assert_eq!(app.board_lane, 1);
            assert!(app.board_lane_is_empty());
            assert_eq!(app.selected_task_index.selected(), Some(0));
        }

        #[test]
        fn board_move_wraps_within_the_lane() {
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![
                    make_task("a", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE),
                    make_task("b", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE),
                ],
            }]);
            app.selected_task_index.select(Some(0));
            app.board_sync();

            app.board_move(true);
            assert_eq!(app.selected_task_index.selected(), Some(1));

            app.board_move(true); // wraps to the top
            assert_eq!(app.selected_task_index.selected(), Some(0));

            app.board_move(false); // wraps to the bottom
            assert_eq!(app.selected_task_index.selected(), Some(1));
        }

        /// Regression test: deleting a task in board mode used to leave the
        /// board focus on the deleted task's successor while
        /// `selected_task_index` (the action target) moved to the
        /// predecessor — the two must agree after the delete sequence.
        #[test]
        fn delete_in_board_mode_keeps_focus_and_action_target_consistent() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![
                    make_task("a", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE),
                    make_task("b", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE),
                    make_task("c", TASK_STATUS_ON_GOING, TASK_PRIORITY_NONE),
                ],
            }]);
            app.board_view = true;

            let mut items = vec![];
            Task::load_items(&mut app, &mut items); // sorted: [c, a, b]
            app.selected_task_index.select(Some(1)); // "a"
            app.board_sync();
            assert_eq!(Task::get_current(&mut app).title, "a");

            // Exercise the same code path as the DeleteTask handler
            app.delete_current_task(&mut items);

            assert_eq!(Task::get_current(&mut app).title, "c");
            let lane = app.board_lane;
            let row = app.board_lane_states[lane].selected().unwrap();
            let lane_indices = Task::lane_indices(&app, TASK_STATUSES[lane]);
            assert_eq!(
                lane_indices.get(row).copied(),
                app.selected_task_index.selected(),
                "board focus and action target diverged"
            );
        }
    }
}
