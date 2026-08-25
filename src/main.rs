use std::{
    env,
    error::Error,
    fmt::Debug,
    io::{self, stdout},
    process::exit,
    time::Duration,
};

use cli::{Cli, CliAction};
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

/// Source of key events for the event loop. The production implementation
/// reads from the terminal; tests inject a queue of synthetic events so the
/// whole loop (draw, key dispatch, timer tick) runs in-process.
trait KeySource {
    fn next_key(&mut self) -> io::Result<Option<KeyEvent>>;
}

/// Reads one key event from the terminal, blocking up to 250 ms so the
/// timer can tick between key presses; `None` on timeout. Touches the real
/// terminal, so it is excluded from coverage (tests use `QueuedKeys`).
struct CrosstermSource;

impl KeySource for CrosstermSource {
    fn next_key(&mut self) -> io::Result<Option<KeyEvent>> {
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                return Ok(Some(key));
            }
        }
        Ok(None)
    }
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
    if Cli::parse(env::args().skip(1)) == CliAction::ShowVersion {
        print!(env!("CARGO_PKG_VERSION"));
        exit(0);
    }

    // Check the version of the json file
    let were_applied_migrations = Json::check()?;

    // setup terminal
    let terminal = init_terminal()?;

    // create app and run it
    App::setup().run_with_source(terminal, were_applied_migrations, &mut CrosstermSource)?;

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

    /// Run the event loop against a `KeySource`. The production entry point
    /// is `main` (with `CrosstermSource`); tests drive this method directly
    /// with a queue of synthetic keys so the whole loop runs in-process.
    fn run_with_source(
        &mut self,
        mut terminal: Terminal<impl Backend>,
        were_applied_migrations: bool,
        source: &mut impl KeySource,
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

            if let Some(key) = source.next_key()? {
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

            // Help renders the list of the view it was opened from, so it uses
            // that view's selection state. Using the project state here used
            // to clear `selected_project_index` when the list behind the help
            // modal was empty (ratatui resets the state of an empty list),
            // which then panicked in `Project::get_current` on the next frame.
            ViewMode::ViewHelp => match self.previous_view_mode {
                ViewMode::ViewTasks => &mut self.selected_task_index,
                ViewMode::ViewNotes | ViewMode::ViewNote | ViewMode::EditNote => {
                    &mut self.selected_note_index
                }
                _ => &mut self.selected_project_index,
            },
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
    use crate::test_utils::{make_app, make_task, sample_projects, setup_temp_config, ENV_LOCK};
    use crate::timer::TimerTaskBinding;
    use ratatui::crossterm::event::KeyModifiers;

    // ---------- Shared helpers for the handler / loop / render tests ----------

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn release_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Release)
    }

    fn input_with(value: &str) -> Input {
        Input::default().with_value(value.to_string())
    }

    fn confirm_items() -> Vec<ListItem<'static>> {
        let mut items = vec![];
        Ui::load_delete_confirm_items(&mut items);
        items
    }

    fn note(title: &str, body: &str) -> Note {
        Note {
            title: title.to_string(),
            body: body.to_string(),
            created_at: None,
            updated_at: None,
        }
    }

    /// A stopped (paused) stopwatch with a fixed accumulated time, so tests
    /// do not depend on wall-clock timing.
    fn paused_stopwatch(secs: u64, project_index: usize, task_title: &str) -> TimerState {
        TimerState {
            kind: TimerKind::Stopwatch,
            target_secs: 0,
            accumulated: Duration::from_secs(secs),
            started_at: None,
            bound: Some(TimerTaskBinding {
                project_index,
                task_title: task_title.to_string(),
            }),
            rung: false,
        }
    }

    /// A stopped countdown with a fixed accumulated time.
    fn paused_countdown(target_secs: u64, accumulated_secs: u64) -> TimerState {
        TimerState {
            kind: TimerKind::Countdown,
            target_secs,
            accumulated: Duration::from_secs(accumulated_secs),
            started_at: None,
            bound: None,
            rung: false,
        }
    }

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

    // ------------------------------------------------------------------------
    // Key handlers
    // ------------------------------------------------------------------------
    mod handlers {
        use super::*;
        use crate::note::Note;
        use crate::project::Project;
        use crate::task::{
            TASK_PRIORITY_NONE, TASK_STATUS_DONE, TASK_STATUS_ON_GOING, TASK_STATUS_UP_NEXT,
        };
        use tui_textarea::TextArea;

        fn tasks_app() -> App {
            make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![
                    make_task("a", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE),
                    make_task("b", TASK_STATUS_UP_NEXT, 1),
                ],
            }])
        }

        // ---- ViewProjects ----

        #[test]
        fn projects_h_opens_help() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            let action = app.handle_view_projects(key(KeyCode::Char('h')), &mut input, &mut items);

            assert_eq!(action, KeyAction::None);
            assert_eq!(app.view_mode, ViewMode::ViewHelp);
            assert_eq!(app.previous_view_mode, ViewMode::ViewProjects);
        }

        #[test]
        fn projects_enter_on_an_empty_list_skips() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            let action = app.handle_view_projects(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(action, KeyAction::Skip);
            assert_eq!(app.view_mode, ViewMode::ViewProjects);
        }

        #[test]
        fn projects_enter_opens_tasks_and_loads_them() {
            let mut app = tasks_app();
            let mut input = Input::default();
            let mut items = vec![];
            Project::load_items(&mut app, &mut items);

            let action = app.handle_view_projects(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(action, KeyAction::None);
            assert_eq!(app.view_mode, ViewMode::ViewTasks);
            assert_eq!(app.selected_task_index.selected(), Some(0));
            assert_eq!(items.len(), 2);
        }

        #[test]
        fn projects_enter_in_board_mode_resyncs_the_board() {
            let mut app = tasks_app();
            app.board_view = true;
            let mut input = Input::default();
            let mut items = vec![];
            Project::load_items(&mut app, &mut items);

            app.handle_view_projects(key(KeyCode::Right), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewTasks);
            assert_eq!(app.board_lane, 0); // UpNext lane of the selected task
            assert_eq!(app.board_lane_states[0].selected(), Some(0));
        }

        #[test]
        fn projects_l_opens_tasks_too() {
            let mut app = tasks_app();
            let mut input = Input::default();
            let mut items = vec![];
            Project::load_items(&mut app, &mut items);

            app.handle_view_projects(key(KeyCode::Char('l')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewTasks);
        }

        #[test]
        fn projects_r_starts_renaming_the_selected_project() {
            let mut app = make_app(sample_projects());
            let mut input = Input::default();
            let mut items = vec![];
            Project::load_items(&mut app, &mut items);

            app.handle_view_projects(key(KeyCode::Char('r')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::RenameProject);
            assert_eq!(input.value(), "alpha");
        }

        #[test]
        fn projects_r_on_an_empty_list_skips() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            assert_eq!(
                app.handle_view_projects(key(KeyCode::Char('r')), &mut input, &mut items),
                KeyAction::Skip
            );
        }

        #[test]
        fn projects_n_starts_adding_a_project() {
            let mut app = make_app(vec![]);
            let mut input = input_with("stale");
            let mut items = vec![];

            app.handle_view_projects(key(KeyCode::Char('n')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::AddProject);
            assert_eq!(input.value(), "");
        }

        #[test]
        fn projects_d_opens_the_delete_modal() {
            let mut app = make_app(sample_projects());
            let mut input = Input::default();
            let mut items = vec![];
            Project::load_items(&mut app, &mut items);

            app.handle_view_projects(key(KeyCode::Char('d')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::DeleteProject);
        }

        #[test]
        fn projects_d_on_an_empty_list_skips() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            assert_eq!(
                app.handle_view_projects(key(KeyCode::Char('d')), &mut input, &mut items),
                KeyAction::Skip
            );
        }

        #[test]
        fn projects_navigation_keys_move_the_selection() {
            let mut app = make_app(sample_projects());
            let mut input = Input::default();
            let mut items = vec![];
            Project::load_items(&mut app, &mut items);

            app.handle_view_projects(key(KeyCode::Down), &mut input, &mut items);
            assert_eq!(app.selected_project_index.selected(), Some(1));

            app.handle_view_projects(key(KeyCode::Char('j')), &mut input, &mut items);
            assert_eq!(app.selected_project_index.selected(), Some(0)); // wraps

            app.handle_view_projects(key(KeyCode::Up), &mut input, &mut items);
            assert_eq!(app.selected_project_index.selected(), Some(1)); // wraps

            app.handle_view_projects(key(KeyCode::BackTab), &mut input, &mut items);
            app.handle_view_projects(key(KeyCode::Char('k')), &mut input, &mut items);
            assert_eq!(app.selected_project_index.selected(), Some(1));
        }

        #[test]
        fn projects_c_starts_a_countdown_when_no_timer_runs() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            app.handle_view_projects(key(KeyCode::Char('c')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::SetCountdown);
            assert_eq!(app.previous_view_mode, ViewMode::ViewProjects);
            assert!(app.timer.is_none());
        }

        #[test]
        fn projects_c_opens_the_timer_when_one_runs() {
            let mut app = make_app(vec![]);
            app.timer = Some(paused_stopwatch(10, 0, "t"));
            let mut input = Input::default();
            let mut items = vec![];

            app.handle_view_projects(key(KeyCode::Char('c')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::TimerTask);
            assert_eq!(app.previous_view_mode, ViewMode::ViewProjects);
        }

        #[test]
        fn projects_m_opens_the_notes_view() {
            let mut app = make_app(vec![]);
            app.notes = vec![note("n", "body")];
            let mut input = Input::default();
            let mut items = vec![];

            app.handle_view_projects(key(KeyCode::Char('m')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewNotes);
            assert_eq!(items.len(), 1);
        }

        #[test]
        fn projects_q_quits() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            assert_eq!(
                app.handle_view_projects(key(KeyCode::Char('q')), &mut input, &mut items),
                KeyAction::Quit
            );
        }

        #[test]
        fn projects_unknown_keys_do_nothing() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            let action = app.handle_view_projects(key(KeyCode::Char('z')), &mut input, &mut items);

            assert_eq!(action, KeyAction::None);
            assert_eq!(app.view_mode, ViewMode::ViewProjects);
        }

        // ---- RenameProject / AddProject / DeleteProject ----

        #[test]
        fn rename_project_enter_renames_and_returns() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(sample_projects());
            let mut input = input_with("renamed");
            let mut items = vec![];

            app.handle_rename_project(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewProjects);
            assert_eq!(app.projects[0].title, "renamed");
            assert_eq!(input.value(), "");
        }

        #[test]
        fn rename_project_esc_cancels() {
            let mut app = make_app(sample_projects());
            let mut input = input_with("discard me");
            let mut items = vec![];

            app.handle_rename_project(key(KeyCode::Esc), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewProjects);
            assert_eq!(input.value(), "");
            assert_eq!(app.projects[0].title, "alpha");
        }

        #[test]
        fn rename_project_other_keys_edit_the_input() {
            let mut app = make_app(sample_projects());
            app.view_mode = ViewMode::RenameProject;
            let mut input = input_with("ab");
            let mut items = vec![];

            app.handle_rename_project(key(KeyCode::Char('c')), &mut input, &mut items);

            assert_eq!(input.value(), "abc");
            assert_eq!(app.view_mode, ViewMode::RenameProject);
        }

        #[test]
        fn add_project_esc_cancels() {
            let mut app = make_app(vec![]);
            let mut input = input_with("todo");
            let mut items = vec![];

            app.handle_add_project(key(KeyCode::Esc), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewProjects);
            assert!(app.projects.is_empty());
        }

        #[test]
        fn add_project_empty_enter_just_returns() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            app.handle_add_project(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewProjects);
            assert!(app.projects.is_empty());
        }

        #[test]
        fn add_project_enter_creates_and_selects_the_new_project() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![]);
            let mut input = input_with("todo");
            let mut items = vec![];

            app.handle_add_project(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewProjects);
            assert_eq!(app.projects.len(), 1);
            assert_eq!(app.projects[0].title, "todo");
            assert_eq!(app.selected_project_index.selected(), Some(0));
        }

        #[test]
        fn add_project_other_keys_edit_the_input() {
            let mut app = make_app(vec![]);
            app.view_mode = ViewMode::AddProject;
            let mut input = Input::default();
            let mut items = vec![];

            app.handle_add_project(key(KeyCode::Char('x')), &mut input, &mut items);

            assert_eq!(input.value(), "x");
            assert_eq!(app.view_mode, ViewMode::AddProject);
        }

        #[test]
        fn delete_project_esc_returns_and_resets_the_selection() {
            let mut app = make_app(sample_projects());
            let mut items = vec![];
            let confirm = confirm_items();

            let action = app.handle_delete_project(key(KeyCode::Esc), &mut items, &confirm);

            assert_eq!(action, KeyAction::Skip);
            assert_eq!(app.view_mode, ViewMode::ViewProjects);
            assert_eq!(app.delete_confirm_index.selected(), Some(0));
        }

        #[test]
        fn delete_project_navigation_moves_the_confirm_selection() {
            let mut app = make_app(sample_projects());
            app.view_mode = ViewMode::DeleteProject;
            let mut items = vec![];
            let confirm = confirm_items();

            app.handle_delete_project(key(KeyCode::Down), &mut items, &confirm);
            assert_eq!(app.delete_confirm_index.selected(), Some(1));

            app.handle_delete_project(key(KeyCode::Char('k')), &mut items, &confirm);
            assert_eq!(app.delete_confirm_index.selected(), Some(0));

            app.handle_delete_project(key(KeyCode::Char('j')), &mut items, &confirm);
            assert_eq!(app.delete_confirm_index.selected(), Some(1));
        }

        #[test]
        fn delete_project_confirm_deletes_and_returns() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(sample_projects());
            let mut items = vec![];
            Project::load_items(&mut app, &mut items);
            let confirm = confirm_items();

            app.handle_delete_project(key(KeyCode::Enter), &mut items, &confirm);

            assert_eq!(app.view_mode, ViewMode::ViewProjects);
            assert_eq!(app.projects.len(), 1);
            assert_eq!(app.projects[0].title, "beta");
            assert_eq!(app.delete_confirm_index.selected(), Some(0));
        }

        #[test]
        fn delete_project_cancel_keeps_the_project() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(sample_projects());
            app.delete_confirm_index.select(Some(1));
            let mut items = vec![];
            let confirm = confirm_items();

            app.handle_delete_project(key(KeyCode::Enter), &mut items, &confirm);

            assert_eq!(app.projects.len(), 2);
            assert_eq!(app.view_mode, ViewMode::ViewProjects);
        }

        #[test]
        fn delete_project_drops_a_timer_bound_to_it() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(sample_projects()); // alpha=0, beta=1
            app.selected_project_index.select(Some(1));
            app.timer = Some(paused_stopwatch(10, 1, "t"));
            let mut items = vec![];
            let confirm = confirm_items();

            app.handle_delete_project(key(KeyCode::Enter), &mut items, &confirm);

            assert!(app.timer.is_none());
        }

        #[test]
        fn delete_project_reindexes_a_timer_bound_to_a_later_project() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![
                Project {
                    title: "a".to_string(),
                    tasks: vec![],
                },
                Project {
                    title: "b".to_string(),
                    tasks: vec![],
                },
                Project {
                    title: "c".to_string(),
                    tasks: vec![],
                },
            ]);
            app.selected_project_index.select(Some(1)); // delete "b"
            app.timer = Some(paused_stopwatch(10, 2, "t")); // bound to "c"
            let mut items = vec![];
            let confirm = confirm_items();

            app.handle_delete_project(key(KeyCode::Enter), &mut items, &confirm);

            let bound = app.timer.as_ref().unwrap().bound.as_ref().unwrap();
            assert_eq!(bound.project_index, 1);
        }

        // ---- ViewTasks ----

        #[test]
        fn tasks_h_opens_help() {
            let mut app = tasks_app();
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);

            app.handle_view_tasks(key(KeyCode::Char('h')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewHelp);
            assert_eq!(app.previous_view_mode, ViewMode::ViewTasks);
        }

        #[test]
        fn tasks_esc_returns_to_projects() {
            let mut app = tasks_app();
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);

            app.handle_view_tasks(key(KeyCode::Esc), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewProjects);
            assert_eq!(items.len(), 1); // project items reloaded
        }

        #[test]
        fn tasks_left_returns_in_list_mode_but_switches_lane_in_board_mode() {
            let mut app = tasks_app();
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);

            app.handle_view_tasks(key(KeyCode::Left), &mut input, &mut items);
            assert_eq!(app.view_mode, ViewMode::ViewProjects);

            // Board mode: Left moves the lane focus instead of going back
            let mut app = tasks_app();
            app.board_view = true;
            app.view_mode = ViewMode::ViewTasks;
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            app.selected_task_index.select(Some(0));
            app.board_sync();
            assert_eq!(app.board_lane, 0);

            app.handle_view_tasks(key(KeyCode::Left), &mut input, &mut items);
            assert_eq!(app.view_mode, ViewMode::ViewTasks);
            assert_eq!(app.board_lane, 2); // wrapped backwards to Done
        }

        #[test]
        fn tasks_right_switches_lane_in_board_mode() {
            let mut app = tasks_app();
            app.board_view = true;
            app.view_mode = ViewMode::ViewTasks;
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            app.selected_task_index.select(Some(0));
            app.board_sync();

            app.handle_view_tasks(key(KeyCode::Right), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewTasks);
            assert_eq!(app.board_lane, 1);
        }

        #[test]
        fn tasks_b_toggles_the_board_and_rebuilds_the_items() {
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![
                    make_task("a", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE),
                    make_task("done", TASK_STATUS_DONE, TASK_PRIORITY_NONE),
                ],
            }]);
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            assert_eq!(items.len(), 1); // done tasks are hidden in the list

            app.handle_view_tasks(key(KeyCode::Char('b')), &mut input, &mut items);
            assert!(app.board_view);
            assert_eq!(items.len(), 2); // the board always shows the Done lane

            app.handle_view_tasks(key(KeyCode::Char('b')), &mut input, &mut items);
            assert!(!app.board_view);
        }

        #[test]
        fn tasks_enter_opens_the_status_modal() {
            let mut app = tasks_app();
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            assert_eq!(Task::get_current(&mut app).status, TASK_STATUS_UP_NEXT);

            app.handle_view_tasks(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ChangeStatusTask);
            assert_eq!(app.selected_status_task_index.selected(), Some(0));
        }

        #[test]
        fn tasks_enter_on_an_empty_list_skips() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            let action = app.handle_view_tasks(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(action, KeyAction::Skip);
        }

        #[test]
        fn tasks_actions_on_an_empty_board_lane_skip() {
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![make_task("a", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE)],
            }]);
            app.board_view = true;
            app.view_mode = ViewMode::ViewTasks;
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            app.board_lane = 2; // the Done lane is empty

            for code in [
                KeyCode::Enter,
                KeyCode::Char('p'),
                KeyCode::Char('r'),
                KeyCode::Char('d'),
                KeyCode::Char('v'),
                KeyCode::Char('e'),
            ] {
                assert_eq!(
                    app.handle_view_tasks(key(code), &mut input, &mut items),
                    KeyAction::Skip
                );
            }

            // 's' is also a no-op without a current task, but stays in the view
            app.handle_view_tasks(key(KeyCode::Char('s')), &mut input, &mut items);
            assert!(app.timer.is_none());
            assert_eq!(app.view_mode, ViewMode::ViewTasks);
        }

        #[test]
        fn tasks_p_opens_the_priority_modal() {
            let mut app = tasks_app();
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);

            app.handle_view_tasks(key(KeyCode::Char('p')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ChangePriorityTask);
            let task = Task::get_current(&mut app);
            let index = TASK_PRIORITIES
                .into_iter()
                .position(|p| p == task.priority)
                .unwrap();
            assert_eq!(app.selected_priority_task_index.selected(), Some(index));
        }

        #[test]
        fn tasks_r_prefills_the_rename_input() {
            let mut app = tasks_app();
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let title = Task::get_current(&mut app).title.clone();

            app.handle_view_tasks(key(KeyCode::Char('r')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::RenameTask);
            assert_eq!(input.value(), title);
        }

        #[test]
        fn tasks_n_starts_adding_a_task() {
            let mut app = tasks_app();
            let mut input = input_with("stale");
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);

            app.handle_view_tasks(key(KeyCode::Char('n')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::AddTask);
            assert_eq!(input.value(), "");
        }

        #[test]
        fn tasks_d_opens_the_delete_modal() {
            let mut app = tasks_app();
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);

            app.handle_view_tasks(key(KeyCode::Char('d')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::DeleteTask);
        }

        #[test]
        fn tasks_v_opens_the_details_view() {
            let mut app = tasks_app();
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);

            app.handle_view_tasks(key(KeyCode::Char('v')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewTaskDetails);
        }

        #[test]
        fn tasks_e_prefills_the_note_editor() {
            let mut app = tasks_app();
            app.projects[0].tasks[0].note = "my note".to_string();
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);

            app.handle_view_tasks(key(KeyCode::Char('e')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::EditTaskNote);
            assert_eq!(input.value(), "my note");
        }

        #[test]
        fn tasks_board_jk_moves_within_the_lane() {
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![
                    make_task("x", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE),
                    make_task("y", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE),
                ],
            }]);
            app.board_view = true;
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            app.selected_task_index.select(Some(0));
            app.board_sync();

            app.handle_view_tasks(key(KeyCode::Char('j')), &mut input, &mut items);
            assert_eq!(app.selected_task_index.selected(), Some(1));

            app.handle_view_tasks(key(KeyCode::Char('k')), &mut input, &mut items);
            assert_eq!(app.selected_task_index.selected(), Some(0));
        }

        #[test]
        fn tasks_jk_navigate_the_list_in_list_mode() {
            let mut app = tasks_app();
            app.view_mode = ViewMode::ViewTasks;
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            app.selected_task_index.select(Some(0));

            app.handle_view_tasks(key(KeyCode::Char('j')), &mut input, &mut items);
            assert_eq!(app.selected_task_index.selected(), Some(1));

            app.handle_view_tasks(key(KeyCode::Char('k')), &mut input, &mut items);
            assert_eq!(app.selected_task_index.selected(), Some(0));
        }

        #[test]
        fn tasks_t_toggles_done_visibility() {
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![
                    make_task("a", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE),
                    make_task("done", TASK_STATUS_DONE, TASK_PRIORITY_NONE),
                ],
            }]);
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            assert_eq!(items.len(), 1);

            app.handle_view_tasks(key(KeyCode::Char('t')), &mut input, &mut items);

            assert!(!app.hide_done_tasks);
            assert_eq!(items.len(), 2);
        }

        #[test]
        fn tasks_t_is_a_no_op_in_board_mode() {
            let mut app = tasks_app();
            app.board_view = true;
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);

            app.handle_view_tasks(key(KeyCode::Char('t')), &mut input, &mut items);

            assert!(app.hide_done_tasks);
        }

        #[test]
        fn tasks_s_starts_a_stopwatch_for_the_selected_task() {
            let mut app = tasks_app();
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let title = Task::get_current(&mut app).title.clone();

            app.handle_view_tasks(key(KeyCode::Char('s')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::TimerTask);
            assert_eq!(app.previous_view_mode, ViewMode::ViewTasks);
            let timer = app.timer.as_ref().unwrap();
            assert_eq!(timer.kind, TimerKind::Stopwatch);
            assert_eq!(timer.bound.as_ref().unwrap().project_index, 0);
            assert_eq!(timer.bound.as_ref().unwrap().task_title, title);
        }

        #[test]
        fn tasks_s_opens_the_timer_when_one_runs() {
            let mut app = tasks_app();
            app.timer = Some(paused_stopwatch(10, 0, "a"));
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);

            app.handle_view_tasks(key(KeyCode::Char('s')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::TimerTask);
            assert_eq!(app.previous_view_mode, ViewMode::ViewTasks);
        }

        #[test]
        fn tasks_c_opens_the_countdown_or_timer() {
            let mut app = tasks_app();
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);

            app.handle_view_tasks(key(KeyCode::Char('c')), &mut input, &mut items);
            assert_eq!(app.view_mode, ViewMode::SetCountdown);

            app.timer = Some(paused_stopwatch(10, 0, "a"));
            app.handle_view_tasks(key(KeyCode::Char('c')), &mut input, &mut items);
            assert_eq!(app.view_mode, ViewMode::TimerTask);
        }

        #[test]
        fn tasks_q_quits() {
            let mut app = tasks_app();
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);

            assert_eq!(
                app.handle_view_tasks(key(KeyCode::Char('q')), &mut input, &mut items),
                KeyAction::Quit
            );
        }

        #[test]
        fn tasks_unknown_keys_do_nothing() {
            let mut app = tasks_app();
            app.view_mode = ViewMode::ViewTasks;
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);

            let action = app.handle_view_tasks(key(KeyCode::Char('z')), &mut input, &mut items);

            assert_eq!(action, KeyAction::None);
            assert_eq!(app.view_mode, ViewMode::ViewTasks);
        }

        // ---- RenameTask / status / priority / AddTask / DeleteTask ----

        #[test]
        fn rename_task_enter_renames_and_retargets_the_timer() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let old_title = Task::get_current(&mut app).title.clone();
            app.timer = Some(paused_stopwatch(10, 0, &old_title));
            let mut input = input_with("renamed");

            app.handle_rename_task(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewTasks);
            assert_eq!(Task::get_current(&mut app).title, "renamed");
            assert_eq!(
                app.timer
                    .as_ref()
                    .unwrap()
                    .bound
                    .as_ref()
                    .unwrap()
                    .task_title,
                "renamed"
            );
            assert_eq!(input.value(), "");
        }

        #[test]
        fn rename_task_esc_cancels() {
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let mut input = input_with("discard");

            app.handle_rename_task(key(KeyCode::Esc), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewTasks);
            assert_eq!(input.value(), "");
        }

        #[test]
        fn rename_task_other_keys_edit_the_input() {
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let mut input = input_with("ab");

            app.handle_rename_task(key(KeyCode::Char('c')), &mut input, &mut items);

            assert_eq!(input.value(), "abc");
        }

        #[test]
        fn change_status_enter_applies_the_selected_status() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            app.selected_status_task_index.select(Some(1)); // OnGoing

            app.handle_change_status_task(key(KeyCode::Enter), &mut items, &vec![]);

            assert_eq!(app.view_mode, ViewMode::ViewTasks);
            assert_eq!(Task::get_current(&mut app).status, TASK_STATUS_ON_GOING);
            assert_eq!(app.selected_status_task_index.selected(), Some(0));
        }

        #[test]
        fn change_status_navigation_moves_the_selection() {
            let mut app = tasks_app();
            app.view_mode = ViewMode::ChangeStatusTask;
            let mut items = vec![];
            let mut status_items = vec![];
            Task::load_statues_items(&mut status_items);

            let action =
                app.handle_change_status_task(key(KeyCode::Down), &mut items, &status_items);

            assert_eq!(action, KeyAction::Skip);
            assert_eq!(app.selected_status_task_index.selected(), Some(1));
        }

        #[test]
        fn change_priority_enter_applies_the_selected_priority() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            app.selected_priority_task_index.select(Some(2)); // priority 3

            app.handle_change_priority_task(key(KeyCode::Enter), &mut items, &vec![]);

            assert_eq!(app.view_mode, ViewMode::ViewTasks);
            assert_eq!(Task::get_current(&mut app).priority, 3);
            assert_eq!(app.selected_priority_task_index.selected(), Some(0));
        }

        #[test]
        fn change_priority_navigation_moves_the_selection() {
            let mut app = tasks_app();
            app.view_mode = ViewMode::ChangePriorityTask;
            let mut items = vec![];
            let mut priority_items = vec![];
            Task::load_priority_items(&mut priority_items);

            let action = app.handle_change_priority_task(
                key(KeyCode::Char('j')),
                &mut items,
                &priority_items,
            );

            assert_eq!(action, KeyAction::Skip);
            assert_eq!(app.selected_priority_task_index.selected(), Some(1));
        }

        #[test]
        fn add_task_enter_creates_the_task() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let count = app.projects[0].tasks.len();
            let mut input = input_with("new task");

            app.handle_add_task(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewTasks);
            assert_eq!(app.projects[0].tasks.len(), count + 1);
            assert_eq!(items.len(), count + 1);
        }

        #[test]
        fn add_task_esc_cancels() {
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let count = app.projects[0].tasks.len();
            let mut input = input_with("new task");

            app.handle_add_task(key(KeyCode::Esc), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewTasks);
            assert_eq!(app.projects[0].tasks.len(), count);
        }

        #[test]
        fn add_task_other_keys_edit_the_input() {
            let mut app = tasks_app();
            let mut items = vec![];
            let mut input = Input::default();

            app.handle_add_task(key(KeyCode::Char('x')), &mut input, &mut items);

            assert_eq!(input.value(), "x");
        }

        #[test]
        fn delete_task_confirm_removes_the_task() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let count = app.projects[0].tasks.len();
            let confirm = confirm_items();

            app.handle_delete_task(key(KeyCode::Enter), &mut items, &confirm);

            assert_eq!(app.view_mode, ViewMode::ViewTasks);
            assert_eq!(app.projects[0].tasks.len(), count - 1);
        }

        #[test]
        fn delete_task_cancel_keeps_the_task() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let count = app.projects[0].tasks.len();
            app.delete_confirm_index.select(Some(1));
            let confirm = confirm_items();

            app.handle_delete_task(key(KeyCode::Enter), &mut items, &confirm);

            assert_eq!(app.projects[0].tasks.len(), count);
            assert_eq!(app.view_mode, ViewMode::ViewTasks);
        }

        #[test]
        fn delete_task_drops_a_bound_timer() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let title = Task::get_current(&mut app).title.clone();
            app.timer = Some(paused_stopwatch(10, 0, &title));
            let confirm = confirm_items();

            app.handle_delete_task(key(KeyCode::Enter), &mut items, &confirm);

            assert!(app.timer.is_none());
        }

        #[test]
        fn delete_task_esc_returns() {
            let mut app = tasks_app();
            let mut items = vec![];
            let confirm = confirm_items();

            let action = app.handle_delete_task(key(KeyCode::Esc), &mut items, &confirm);

            assert_eq!(action, KeyAction::Skip);
            assert_eq!(app.view_mode, ViewMode::ViewTasks);
        }

        // ---- ViewTaskDetails / EditTaskNote / SetTaskEstimate ----

        #[test]
        fn task_details_e_edits_the_note() {
            let mut app = tasks_app();
            app.projects[0].tasks[0].note = "my note".to_string();
            let mut input = Input::default();

            app.handle_view_task_details(key(KeyCode::Char('e')), &mut input);

            assert_eq!(app.view_mode, ViewMode::EditTaskNote);
            assert_eq!(input.value(), "my note");
        }

        #[test]
        fn task_details_g_edits_the_estimate() {
            let mut app = tasks_app();
            app.projects[0].tasks[0].estimated_hours = 7;
            let mut input = Input::default();

            app.handle_view_task_details(key(KeyCode::Char('g')), &mut input);

            assert_eq!(app.view_mode, ViewMode::SetTaskEstimate);
            assert_eq!(input.value(), "7");
        }

        #[test]
        fn task_details_g_without_an_estimate_starts_empty() {
            let mut app = tasks_app();
            let mut input = Input::default();

            app.handle_view_task_details(key(KeyCode::Char('g')), &mut input);

            assert_eq!(app.view_mode, ViewMode::SetTaskEstimate);
            assert_eq!(input.value(), "");
        }

        #[test]
        fn task_details_any_other_key_closes() {
            let mut app = tasks_app();
            let mut input = Input::default();

            app.handle_view_task_details(key(KeyCode::Esc), &mut input);

            assert_eq!(app.view_mode, ViewMode::ViewTasks);
        }

        #[test]
        fn edit_task_note_enter_saves() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let mut input = input_with("the note");

            app.handle_edit_task_note(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewTaskDetails);
            assert_eq!(Task::get_current(&mut app).note, "the note");
            assert_eq!(input.value(), "");
        }

        #[test]
        fn edit_task_note_esc_cancels() {
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let mut input = input_with("discard");

            app.handle_edit_task_note(key(KeyCode::Esc), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewTaskDetails);
            assert_eq!(input.value(), "");
            assert_eq!(Task::get_current(&mut app).note, "");
        }

        #[test]
        fn edit_task_note_other_keys_edit_the_input() {
            let mut app = tasks_app();
            let mut items = vec![];
            let mut input = input_with("ab");

            app.handle_edit_task_note(key(KeyCode::Char('c')), &mut input, &mut items);

            assert_eq!(input.value(), "abc");
        }

        #[test]
        fn set_estimate_enter_applies_the_value() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let mut input = input_with("  5  ");

            app.handle_set_task_estimate(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewTaskDetails);
            assert_eq!(Task::get_current(&mut app).estimated_hours, 5);
        }

        #[test]
        fn set_estimate_empty_enter_returns_without_changes() {
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let mut input = Input::default();

            app.handle_set_task_estimate(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewTaskDetails);
            assert_eq!(Task::get_current(&mut app).estimated_hours, 0);
        }

        #[test]
        fn set_estimate_invalid_enter_stays_in_the_modal() {
            let mut app = tasks_app();
            app.view_mode = ViewMode::SetTaskEstimate;
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let mut input = input_with("abc");

            app.handle_set_task_estimate(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::SetTaskEstimate);
            assert_eq!(Task::get_current(&mut app).estimated_hours, 0);
        }

        #[test]
        fn set_estimate_esc_returns() {
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let mut input = input_with("5");

            app.handle_set_task_estimate(key(KeyCode::Esc), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewTaskDetails);
            assert_eq!(input.value(), "");
        }

        #[test]
        fn set_estimate_filters_non_digit_keys() {
            let mut app = tasks_app();
            let mut items = vec![];
            let mut input = Input::default();

            app.handle_set_task_estimate(key(KeyCode::Char('a')), &mut input, &mut items);
            assert_eq!(input.value(), "");

            app.handle_set_task_estimate(key(KeyCode::Char('7')), &mut input, &mut items);
            assert_eq!(input.value(), "7");

            app.handle_set_task_estimate(key(KeyCode::Backspace), &mut input, &mut items);
            assert_eq!(input.value(), "");
        }

        // ---- TimerTask / SetCountdown ----

        #[test]
        fn timer_task_space_pauses_and_resumes() {
            let mut app = make_app(vec![]);
            app.timer = Some(TimerState::new_stopwatch(0, "t".to_string()));

            app.handle_timer_task(key(KeyCode::Char(' ')));
            assert!(!app.timer.as_ref().unwrap().is_running());

            app.handle_timer_task(key(KeyCode::Char(' ')));
            assert!(app.timer.as_ref().unwrap().is_running());
        }

        #[test]
        fn timer_task_enter_settles_and_returns() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let title = Task::get_current(&mut app).title.clone();
            app.timer = Some(paused_stopwatch(90, 0, &title));
            app.previous_view_mode = ViewMode::ViewTasks;

            app.handle_timer_task(key(KeyCode::Enter));

            assert!(app.timer.is_none());
            assert_eq!(app.view_mode, ViewMode::ViewTasks);
            assert_eq!(Task::get_current(&mut app).time_spent_secs, 90);
        }

        #[test]
        fn timer_task_esc_returns_without_settling() {
            let mut app = make_app(vec![]);
            app.timer = Some(paused_stopwatch(90, 0, "t"));
            app.previous_view_mode = ViewMode::ViewProjects;

            app.handle_timer_task(key(KeyCode::Esc));

            assert!(app.timer.is_some());
            assert_eq!(app.view_mode, ViewMode::ViewProjects);
        }

        #[test]
        fn timer_task_finished_countdown_dismisses_on_any_key() {
            let mut app = make_app(vec![]);
            app.timer = Some(paused_countdown(60, 60)); // finished
            app.previous_view_mode = ViewMode::ViewTasks;

            let action = app.handle_timer_task(key(KeyCode::Char('x')));

            assert_eq!(action, KeyAction::Skip);
            assert!(app.timer.is_none());
            assert_eq!(app.view_mode, ViewMode::ViewTasks);
        }

        #[test]
        fn set_countdown_enter_starts_a_countdown() {
            let mut app = make_app(vec![]);
            app.previous_view_mode = ViewMode::ViewProjects;
            let mut input = input_with("2.5");

            app.handle_set_countdown(key(KeyCode::Enter), &mut input);

            assert_eq!(app.view_mode, ViewMode::TimerTask);
            let timer = app.timer.as_ref().unwrap();
            assert_eq!(timer.kind, TimerKind::Countdown);
            assert_eq!(timer.target_secs, 150);
            assert_eq!(input.value(), "");
        }

        #[test]
        fn set_countdown_empty_enter_returns_to_the_previous_view() {
            let mut app = make_app(vec![]);
            app.previous_view_mode = ViewMode::ViewProjects;
            let mut input = Input::default();

            app.handle_set_countdown(key(KeyCode::Enter), &mut input);

            assert_eq!(app.view_mode, ViewMode::ViewProjects);
            assert!(app.timer.is_none());
        }

        #[test]
        fn set_countdown_invalid_enter_stays_in_the_modal() {
            let mut app = make_app(vec![]);
            app.view_mode = ViewMode::SetCountdown;
            let mut input = input_with("abc");

            app.handle_set_countdown(key(KeyCode::Enter), &mut input);

            assert_eq!(app.view_mode, ViewMode::SetCountdown);
            assert!(app.timer.is_none());
        }

        #[test]
        fn set_countdown_esc_returns() {
            let mut app = make_app(vec![]);
            app.previous_view_mode = ViewMode::ViewNotes;
            let mut input = input_with("25");

            app.handle_set_countdown(key(KeyCode::Esc), &mut input);

            assert_eq!(app.view_mode, ViewMode::ViewNotes);
            assert_eq!(input.value(), "");
        }

        #[test]
        fn set_countdown_filters_input() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();

            app.handle_set_countdown(key(KeyCode::Char('a')), &mut input);
            assert_eq!(input.value(), "");

            app.handle_set_countdown(key(KeyCode::Char('.')), &mut input);
            assert_eq!(input.value(), ".");

            app.handle_set_countdown(key(KeyCode::Char('9')), &mut input);
            assert_eq!(input.value(), ".9");
        }

        // ---- ViewNotes / AddNote / RenameNote / DeleteNote ----

        #[test]
        fn notes_h_opens_help() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            app.handle_view_notes(key(KeyCode::Char('h')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewHelp);
            assert_eq!(app.previous_view_mode, ViewMode::ViewNotes);
        }

        #[test]
        fn notes_esc_returns_to_projects() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            app.handle_view_notes(key(KeyCode::Esc), &mut input, &mut items);
            assert_eq!(app.view_mode, ViewMode::ViewProjects);

            app.handle_view_notes(key(KeyCode::Left), &mut input, &mut items);
            assert_eq!(app.view_mode, ViewMode::ViewProjects);
        }

        #[test]
        fn notes_enter_on_an_empty_list_skips() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            let action = app.handle_view_notes(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(action, KeyAction::Skip);
        }

        #[test]
        fn notes_enter_opens_the_preview_and_resets_the_scroll() {
            let mut app = make_app(vec![]);
            app.notes = vec![note("n", "body")];
            let mut input = Input::default();
            let mut items = vec![];
            Note::load_items(&mut app, &mut items);
            app.note_scroll = 5;

            app.handle_view_notes(key(KeyCode::Char('v')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewNote);
            assert_eq!(app.note_scroll, 0);
        }

        #[test]
        fn notes_n_starts_adding_a_note() {
            let mut app = make_app(vec![]);
            let mut input = input_with("stale");
            let mut items = vec![];

            app.handle_view_notes(key(KeyCode::Char('n')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::AddNote);
            assert_eq!(input.value(), "");
        }

        #[test]
        fn notes_r_prefills_the_rename_input() {
            let mut app = make_app(vec![]);
            app.notes = vec![note("my note", "")];
            let mut input = Input::default();
            let mut items = vec![];
            Note::load_items(&mut app, &mut items);

            app.handle_view_notes(key(KeyCode::Char('r')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::RenameNote);
            assert_eq!(input.value(), "my note");
        }

        #[test]
        fn notes_r_on_an_empty_list_skips() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            assert_eq!(
                app.handle_view_notes(key(KeyCode::Char('r')), &mut input, &mut items),
                KeyAction::Skip
            );
        }

        #[test]
        fn notes_d_opens_the_delete_modal() {
            let mut app = make_app(vec![]);
            app.notes = vec![note("n", "")];
            let mut input = Input::default();
            let mut items = vec![];
            Note::load_items(&mut app, &mut items);

            app.handle_view_notes(key(KeyCode::Char('d')), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::DeleteNote);
        }

        #[test]
        fn notes_d_on_an_empty_list_skips() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            assert_eq!(
                app.handle_view_notes(key(KeyCode::Char('d')), &mut input, &mut items),
                KeyAction::Skip
            );
        }

        #[test]
        fn notes_unknown_keys_do_nothing() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            let action = app.handle_view_notes(key(KeyCode::Char('z')), &mut input, &mut items);

            assert_eq!(action, KeyAction::None);
            assert_eq!(app.view_mode, ViewMode::ViewProjects);
        }

        #[test]
        fn notes_navigation_moves_the_selection() {
            let mut app = make_app(vec![]);
            app.view_mode = ViewMode::ViewNotes;
            app.notes = vec![note("n1", ""), note("n2", "")];
            let mut input = Input::default();
            let mut items = vec![];
            Note::load_items(&mut app, &mut items);

            app.handle_view_notes(key(KeyCode::Down), &mut input, &mut items);
            assert_eq!(app.selected_note_index.selected(), Some(1));

            app.handle_view_notes(key(KeyCode::Char('k')), &mut input, &mut items);
            assert_eq!(app.selected_note_index.selected(), Some(0));
        }

        #[test]
        fn notes_q_quits() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            assert_eq!(
                app.handle_view_notes(key(KeyCode::Char('q')), &mut input, &mut items),
                KeyAction::Quit
            );
        }

        #[test]
        fn add_note_enter_creates_and_selects_the_new_note() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![]);
            let mut input = input_with("shopping");
            let mut items = vec![];

            app.handle_add_note(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewNotes);
            assert_eq!(app.notes.len(), 1);
            assert_eq!(app.notes[0].title, "shopping");
            assert_eq!(app.selected_note_index.selected(), Some(0));
        }

        #[test]
        fn add_note_empty_enter_just_returns() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            app.handle_add_note(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewNotes);
            assert!(app.notes.is_empty());
        }

        #[test]
        fn add_note_esc_cancels() {
            let mut app = make_app(vec![]);
            let mut input = input_with("shopping");
            let mut items = vec![];

            app.handle_add_note(key(KeyCode::Esc), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewNotes);
            assert!(app.notes.is_empty());
        }

        #[test]
        fn add_note_other_keys_edit_the_input() {
            let mut app = make_app(vec![]);
            let mut input = Input::default();
            let mut items = vec![];

            app.handle_add_note(key(KeyCode::Char('x')), &mut input, &mut items);

            assert_eq!(input.value(), "x");
        }

        #[test]
        fn rename_note_enter_renames_and_returns() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![]);
            app.notes = vec![note("old", "")];
            let mut input = input_with("new");
            let mut items = vec![];

            app.handle_rename_note(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewNotes);
            assert_eq!(app.notes[0].title, "new");
        }

        #[test]
        fn rename_note_esc_cancels() {
            let mut app = make_app(vec![]);
            app.notes = vec![note("old", "")];
            let mut input = input_with("discard");
            let mut items = vec![];

            app.handle_rename_note(key(KeyCode::Esc), &mut input, &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewNotes);
            assert_eq!(app.notes[0].title, "old");
        }

        #[test]
        fn rename_note_other_keys_edit_the_input() {
            let mut app = make_app(vec![]);
            let mut input = input_with("ab");
            let mut items = vec![];

            app.handle_rename_note(key(KeyCode::Char('c')), &mut input, &mut items);

            assert_eq!(input.value(), "abc");
        }

        #[test]
        fn delete_note_confirm_removes_the_note() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![]);
            app.notes = vec![note("n1", ""), note("n2", "")];
            app.selected_note_index.select(Some(1));
            let mut items = vec![];
            Note::load_items(&mut app, &mut items);
            let confirm = confirm_items();

            app.handle_delete_note(key(KeyCode::Enter), &mut items, &confirm);

            assert_eq!(app.view_mode, ViewMode::ViewNotes);
            assert_eq!(app.notes.len(), 1);
            assert_eq!(app.notes[0].title, "n1");
        }

        #[test]
        fn delete_note_cancel_keeps_the_note() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![]);
            app.notes = vec![note("n1", ""), note("n2", "")];
            app.delete_confirm_index.select(Some(1));
            let mut items = vec![];
            Note::load_items(&mut app, &mut items);
            let confirm = confirm_items();

            app.handle_delete_note(key(KeyCode::Enter), &mut items, &confirm);

            assert_eq!(app.notes.len(), 2);
        }

        #[test]
        fn delete_note_esc_returns() {
            let mut app = make_app(vec![]);
            app.notes = vec![note("n1", "")];
            let mut items = vec![];
            let confirm = confirm_items();

            let action = app.handle_delete_note(key(KeyCode::Esc), &mut items, &confirm);

            assert_eq!(action, KeyAction::Skip);
            assert_eq!(app.view_mode, ViewMode::ViewNotes);
        }

        // ---- ViewNote / EditNote ----

        #[test]
        fn view_note_scrolling_keys() {
            let mut app = make_app(vec![]);
            app.notes = vec![note("n", "body")];
            let mut items = vec![];

            app.handle_view_note(key(KeyCode::Down), &mut items);
            assert_eq!(app.note_scroll, 1);
            app.handle_view_note(key(KeyCode::Char('j')), &mut items);
            assert_eq!(app.note_scroll, 2);
            app.handle_view_note(key(KeyCode::Up), &mut items);
            assert_eq!(app.note_scroll, 1);
            app.handle_view_note(key(KeyCode::Char('k')), &mut items);
            assert_eq!(app.note_scroll, 0);
            app.handle_view_note(key(KeyCode::Up), &mut items); // clamped at 0
            assert_eq!(app.note_scroll, 0);
            app.handle_view_note(key(KeyCode::PageDown), &mut items);
            assert_eq!(app.note_scroll, 10);
            app.handle_view_note(key(KeyCode::PageUp), &mut items);
            assert_eq!(app.note_scroll, 0);
            app.handle_view_note(key(KeyCode::End), &mut items);
            assert_eq!(app.note_scroll, u16::MAX);
            app.handle_view_note(key(KeyCode::Home), &mut items);
            assert_eq!(app.note_scroll, 0);
            app.handle_view_note(key(KeyCode::Char('g')), &mut items);
            assert_eq!(app.note_scroll, 0);
        }

        #[test]
        fn view_note_e_opens_the_editor() {
            let mut app = make_app(vec![]);
            app.notes = vec![note("n", "# body")];
            let mut items = vec![];

            app.handle_view_note(key(KeyCode::Char('e')), &mut items);

            assert_eq!(app.view_mode, ViewMode::EditNote);
            let textarea = app.note_textarea.as_ref().unwrap();
            assert_eq!(textarea.lines(), &["# body"]);
        }

        #[test]
        fn view_note_h_opens_help() {
            let mut app = make_app(vec![]);
            app.notes = vec![note("n", "")];
            let mut items = vec![];

            app.handle_view_note(key(KeyCode::Char('h')), &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewHelp);
            assert_eq!(app.previous_view_mode, ViewMode::ViewNote);
        }

        #[test]
        fn view_note_esc_returns_to_the_list() {
            let mut app = make_app(vec![]);
            app.notes = vec![note("n", "")];
            let mut items = vec![];
            Note::load_items(&mut app, &mut items);
            items.clear();

            app.handle_view_note(key(KeyCode::Enter), &mut items);

            assert_eq!(app.view_mode, ViewMode::ViewNotes);
            assert_eq!(items.len(), 1); // list refreshed
        }

        #[test]
        fn view_note_e_with_an_empty_body_starts_with_one_line() {
            let mut app = make_app(vec![]);
            app.notes = vec![note("n", "")];
            let mut items = vec![];

            app.handle_view_note(key(KeyCode::Char('e')), &mut items);

            assert_eq!(app.view_mode, ViewMode::EditNote);
            assert_eq!(
                app.note_textarea.as_ref().unwrap().lines(),
                &["".to_string()]
            );
        }

        #[test]
        fn view_note_unknown_keys_do_nothing() {
            let mut app = make_app(vec![]);
            app.notes = vec![note("n", "")];
            let mut items = vec![];

            let action = app.handle_view_note(key(KeyCode::Char('z')), &mut items);

            assert_eq!(action, KeyAction::None);
        }

        #[test]
        fn view_note_q_quits() {
            let mut app = make_app(vec![]);
            app.notes = vec![note("n", "")];
            let mut items = vec![];

            assert_eq!(
                app.handle_view_note(key(KeyCode::Char('q')), &mut items),
                KeyAction::Quit
            );
        }

        #[test]
        fn edit_note_esc_saves_changes_and_returns_to_the_preview() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![]);
            app.notes = vec![note("n", "old")];
            app.note_textarea = Some(TextArea::from(vec!["new body".to_string()]));

            app.handle_edit_note(key(KeyCode::Esc));

            assert_eq!(app.view_mode, ViewMode::ViewNote);
            assert!(app.note_textarea.is_none());
            assert_eq!(app.notes[0].body, "new body");
            assert!(app.notes[0].updated_at.is_some());
        }

        #[test]
        fn edit_note_esc_without_changes_skips_the_write() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![]);
            app.notes = vec![note("n", "")];
            app.note_textarea = Some(TextArea::from(vec![String::new()]));

            app.handle_edit_note(key(KeyCode::Esc));

            assert_eq!(app.view_mode, ViewMode::ViewNote);
            assert_eq!(app.notes[0].updated_at, None);
        }

        #[test]
        fn edit_note_esc_without_a_textarea_returns() {
            let mut app = make_app(vec![]);
            app.notes = vec![note("n", "")];

            app.handle_edit_note(key(KeyCode::Esc));

            assert_eq!(app.view_mode, ViewMode::ViewNote);
        }

        #[test]
        fn edit_note_other_keys_go_to_the_textarea() {
            let mut app = make_app(vec![]);
            app.view_mode = ViewMode::EditNote;
            app.notes = vec![note("n", "")];
            app.note_textarea = Some(TextArea::from(vec![String::new()]));

            app.handle_edit_note(key(KeyCode::Char('a')));

            assert_eq!(app.view_mode, ViewMode::EditNote);
            assert_eq!(
                app.note_textarea.as_ref().unwrap().lines(),
                &["a".to_string()]
            );
        }

        // ---- handle_modal_nav ----

        #[test]
        fn delete_project_leaves_a_countdown_untouched() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(sample_projects());
            app.selected_project_index.select(Some(1));
            app.timer = Some(paused_countdown(60, 10)); // not bound to any task
            let mut items = vec![];
            let confirm = confirm_items();

            app.handle_delete_project(key(KeyCode::Enter), &mut items, &confirm);

            assert!(app.timer.is_some());
            assert_eq!(app.timer.as_ref().unwrap().kind, TimerKind::Countdown);
        }

        #[test]
        fn rename_task_without_a_timer_is_fine() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let mut input = input_with("renamed");

            app.handle_rename_task(key(KeyCode::Enter), &mut input, &mut items);

            assert!(app.timer.is_none());
            assert_eq!(Task::get_current(&mut app).title, "renamed");
        }

        #[test]
        fn timer_task_space_without_a_timer_is_a_no_op() {
            let mut app = make_app(vec![]);

            app.handle_timer_task(key(KeyCode::Char(' '))); // must not panic

            assert!(app.timer.is_none());
        }

        #[test]
        fn delete_project_keeps_a_timer_bound_to_an_earlier_project() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![
                Project {
                    title: "a".to_string(),
                    tasks: vec![],
                },
                Project {
                    title: "b".to_string(),
                    tasks: vec![],
                },
            ]);
            app.selected_project_index.select(Some(1)); // delete "b"
            app.timer = Some(paused_stopwatch(10, 0, "t")); // bound to "a"
            let mut items = vec![];
            let confirm = confirm_items();

            app.handle_delete_project(key(KeyCode::Enter), &mut items, &confirm);

            let bound = app.timer.as_ref().unwrap().bound.as_ref().unwrap();
            assert_eq!(bound.project_index, 0); // unchanged
        }

        #[test]
        fn rename_task_leaves_a_timer_bound_to_another_task_alone() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            app.timer = Some(paused_stopwatch(10, 0, "other task"));
            let mut input = input_with("renamed");

            app.handle_rename_task(key(KeyCode::Enter), &mut input, &mut items);

            assert_eq!(
                app.timer
                    .as_ref()
                    .unwrap()
                    .bound
                    .as_ref()
                    .unwrap()
                    .task_title,
                "other task"
            );
        }

        #[test]
        fn next_and_previous_with_no_selection_start_at_zero() {
            let mut app = make_app(vec![]);
            let items: Vec<ListItem> = vec![ListItem::from("a"), ListItem::from("b")];
            app.selected_project_index.select(None);

            app.next(&items);
            assert_eq!(app.selected_project_index.selected(), Some(0));

            app.selected_project_index.select(None);
            app.previous(&items);
            assert_eq!(app.selected_project_index.selected(), Some(0));
        }

        #[test]
        fn board_sync_without_a_selection_is_a_no_op() {
            let mut app = tasks_app();
            app.selected_task_index.select(None);

            app.board_sync(); // must not panic

            assert_eq!(app.board_lane, 0);
        }

        #[test]
        fn board_sync_with_an_out_of_range_project_is_a_no_op() {
            let mut app = tasks_app();
            app.selected_project_index.select(Some(5)); // only one project

            app.board_sync(); // must not panic

            assert_eq!(app.board_lane, 0);
        }

        #[test]
        fn board_move_on_an_empty_lane_is_a_no_op() {
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![make_task("a", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE)],
            }]);
            app.board_lane = 2; // Done lane is empty

            app.board_move(true);
            app.board_move(false);

            assert_eq!(app.selected_task_index.selected(), Some(0));
        }

        #[test]
        fn modal_nav_esc_resets_and_returns() {
            let mut app = make_app(vec![]);
            app.view_mode = ViewMode::DeleteProject;
            app.delete_confirm_index.select(Some(1));
            let items = confirm_items();

            let handled = app.handle_modal_nav(KeyCode::Esc, &items, ViewMode::ViewProjects);

            assert!(handled);
            assert_eq!(app.view_mode, ViewMode::ViewProjects);
            assert_eq!(app.delete_confirm_index.selected(), Some(0));
        }

        #[test]
        fn modal_nav_keys_move_the_selection() {
            let mut app = make_app(vec![]);
            app.view_mode = ViewMode::DeleteProject;
            let items = confirm_items();

            assert!(app.handle_modal_nav(KeyCode::Down, &items, ViewMode::ViewProjects));
            assert_eq!(app.delete_confirm_index.selected(), Some(1));
            assert!(app.handle_modal_nav(KeyCode::Tab, &items, ViewMode::ViewProjects));
            assert_eq!(app.delete_confirm_index.selected(), Some(0));
            assert!(app.handle_modal_nav(KeyCode::Char('j'), &items, ViewMode::ViewProjects));
            assert_eq!(app.delete_confirm_index.selected(), Some(1));
            assert!(app.handle_modal_nav(KeyCode::Char('k'), &items, ViewMode::ViewProjects));
            assert_eq!(app.delete_confirm_index.selected(), Some(0));
        }

        #[test]
        fn modal_nav_other_keys_are_not_handled() {
            let mut app = make_app(vec![]);
            let items = confirm_items();

            assert!(!app.handle_modal_nav(KeyCode::Enter, &items, ViewMode::ViewProjects));
        }

        // ---- misc ----

        #[test]
        fn delete_current_task_moves_to_the_previous_task() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = tasks_app();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            app.selected_task_index.select(Some(1));

            app.delete_current_task(&mut items);

            assert_eq!(app.projects[0].tasks.len(), 1);
            assert_eq!(app.selected_task_index.selected(), Some(0));
        }

        #[test]
        fn handle_key_dispatches_every_view_mode() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = tasks_app();
            app.notes = vec![note("n", "")];
            app.timer = Some(paused_countdown(60, 10));
            let mut input = Input::default();
            let mut items = vec![];
            Task::load_items(&mut app, &mut items);
            let mut status_items = vec![];
            Task::load_statues_items(&mut status_items);
            let mut priority_items = vec![];
            Task::load_priority_items(&mut priority_items);
            let delete_confirm_items = confirm_items();

            let modes = [
                ViewMode::ViewProjects,
                ViewMode::RenameProject,
                ViewMode::AddProject,
                ViewMode::DeleteProject,
                ViewMode::ViewTasks,
                ViewMode::RenameTask,
                ViewMode::ChangeStatusTask,
                ViewMode::ChangePriorityTask,
                ViewMode::AddTask,
                ViewMode::DeleteTask,
                ViewMode::ViewTaskDetails,
                ViewMode::EditTaskNote,
                ViewMode::SetTaskEstimate,
                ViewMode::TimerTask,
                ViewMode::SetCountdown,
                ViewMode::ViewHelp,
                ViewMode::ViewNotes,
                ViewMode::AddNote,
                ViewMode::RenameNote,
                ViewMode::DeleteNote,
                ViewMode::ViewNote,
                ViewMode::EditNote,
                ViewMode::InfoMigration,
            ];

            for mode in modes {
                app.view_mode = mode;
                app.handle_key(
                    key(KeyCode::Char('q')),
                    &mut input,
                    &mut items,
                    &status_items,
                    &priority_items,
                    &delete_confirm_items,
                );
            }
        }

        #[test]
        fn use_state_maps_the_remaining_view_modes() {
            fn assert_state(app: &mut App, mode: ViewMode, expected: fn(&App) -> *const ListState) {
                app.view_mode = mode;
                let actual = app.use_state() as *const ListState;
                assert_eq!(actual, expected(app));
            }

            let mut app = make_app(vec![]);

            assert_state(&mut app, ViewMode::AddProject, |a| {
                &a.selected_project_index
            });
            assert_state(&mut app, ViewMode::AddTask, |a| &a.selected_task_index);
            assert_state(&mut app, ViewMode::EditTaskNote, |a| &a.selected_task_index);
            assert_state(&mut app, ViewMode::SetTaskEstimate, |a| {
                &a.selected_task_index
            });
            assert_state(&mut app, ViewMode::ViewHelp, |a| &a.selected_project_index);
            assert_state(&mut app, ViewMode::InfoMigration, |a| {
                &a.selected_project_index
            });

            // Help uses the selection state of the view it was opened from
            app.previous_view_mode = ViewMode::ViewTasks;
            assert_state(&mut app, ViewMode::ViewHelp, |a| &a.selected_task_index);
            app.previous_view_mode = ViewMode::ViewNotes;
            assert_state(&mut app, ViewMode::ViewHelp, |a| &a.selected_note_index);
            app.previous_view_mode = ViewMode::ViewNote;
            assert_state(&mut app, ViewMode::ViewHelp, |a| &a.selected_note_index);
            app.previous_view_mode = ViewMode::ViewTaskDetails;
            assert_state(&mut app, ViewMode::ViewHelp, |a| &a.selected_project_index);

            app.previous_view_mode = ViewMode::ViewProjects;
            assert_state(&mut app, ViewMode::TimerTask, |a| &a.selected_project_index);
            assert_state(&mut app, ViewMode::SetCountdown, |a| {
                &a.selected_project_index
            });

            app.previous_view_mode = ViewMode::ViewTasks;
            assert_state(&mut app, ViewMode::TimerTask, |a| &a.selected_task_index);
            assert_state(&mut app, ViewMode::SetCountdown, |a| &a.selected_task_index);
        }
    }

    // ------------------------------------------------------------------------
    // Event loop (`run_with_source`)
    // ------------------------------------------------------------------------
    mod event_loop {
        use super::*;
        use crate::task::{TASK_PRIORITY_NONE, TASK_STATUS_UP_NEXT};
        use crate::test_utils::make_task;
        use ratatui::{backend::TestBackend, Terminal};
        use std::collections::VecDeque;

        struct QueuedKeys {
            keys: VecDeque<Option<KeyEvent>>,
        }

        impl QueuedKeys {
            fn from_keys(keys: Vec<KeyEvent>) -> Self {
                Self {
                    keys: keys.into_iter().map(Some).collect(),
                }
            }
        }

        impl KeySource for QueuedKeys {
            fn next_key(&mut self) -> io::Result<Option<KeyEvent>> {
                Ok(self.keys.pop_front().flatten())
            }
        }

        fn run(app: &mut App, keys: Vec<KeyEvent>, migrations: bool) -> io::Result<()> {
            let backend = TestBackend::new(80, 24);
            let terminal = Terminal::new(backend).unwrap();
            let mut source = QueuedKeys::from_keys(keys);
            app.run_with_source(terminal, migrations, &mut source)
        }

        #[test]
        fn quit_key_ends_the_loop() {
            let mut app = make_app(vec![]);

            let result = run(&mut app, vec![key(KeyCode::Char('q'))], false);

            assert!(result.is_ok());
            assert_eq!(app.view_mode, ViewMode::ViewProjects);
        }

        #[test]
        fn crossterm_source_polls_without_panicking() {
            let mut source = CrosstermSource;
            // On CI stdin is /dev/null (no events); on a local tty this waits
            // up to 250 ms and returns `None` unless the user happens to type.
            // Either outcome is fine — the poll itself must not panic.
            let _ = source.next_key();
        }

        #[test]
        fn applied_migrations_start_in_the_info_view() {
            let mut app = make_app(vec![]);

            let result = run(
                &mut app,
                vec![key(KeyCode::Char('x')), key(KeyCode::Char('q'))],
                true,
            );

            assert!(result.is_ok());
            // 'x' dismissed the migration info, 'q' quit from ViewProjects
            assert_eq!(app.view_mode, ViewMode::ViewProjects);
        }

        #[test]
        fn skip_keys_redraw_and_continue() {
            let mut app = make_app(vec![]); // no projects: Enter skips

            let result = run(
                &mut app,
                vec![key(KeyCode::Enter), key(KeyCode::Char('q'))],
                false,
            );

            assert!(result.is_ok());
        }

        #[test]
        fn timeouts_tick_the_timer_without_input() {
            let mut app = make_app(vec![]);
            let mut source = QueuedKeys {
                keys: VecDeque::from([None, None, Some(key(KeyCode::Char('q')))]),
            };
            let backend = TestBackend::new(80, 24);
            let terminal = Terminal::new(backend).unwrap();

            let result = app.run_with_source(terminal, false, &mut source);

            assert!(result.is_ok());
        }

        #[test]
        fn release_events_are_ignored() {
            let mut app = make_app(vec![]);
            // If the Release event were handled, 'n' would open AddProject and
            // the following 'q' would just be input text — the loop would not
            // terminate, so reaching the end proves the filter works.
            let keys = vec![release_key(KeyCode::Char('n')), key(KeyCode::Char('q'))];

            let result = run(&mut app, keys, false);

            assert!(result.is_ok());
            assert_eq!(app.view_mode, ViewMode::ViewProjects);
        }

        #[test]
        fn quitting_settles_a_running_stopwatch() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![make_task("t", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE)],
            }]);
            app.timer = Some(paused_stopwatch(45, 0, "t"));

            let result = run(&mut app, vec![key(KeyCode::Char('q'))], false);

            assert!(result.is_ok());
            assert!(app.timer.is_none());
            assert_eq!(app.projects[0].tasks[0].time_spent_secs, 45);
        }
    }

    // ------------------------------------------------------------------------
    // Timer behaviour (tick / settle / navigation helpers)
    // ------------------------------------------------------------------------
    mod timer_behaviour {
        use super::*;
        use crate::task::{TASK_PRIORITY_NONE, TASK_STATUS_UP_NEXT};
        use crate::test_utils::make_task;

        #[test]
        fn tick_timer_rings_the_bell_once_at_zero() {
            let mut app = make_app(vec![]);
            app.timer = Some(paused_countdown(60, 60));

            app.tick_timer();
            assert!(app.timer.as_ref().unwrap().rung);

            // Already rung: nothing more happens (and it must not panic)
            app.tick_timer();
            assert!(app.timer.as_ref().unwrap().rung);
        }

        #[test]
        fn tick_timer_does_nothing_without_a_timer() {
            let mut app = make_app(vec![]);

            app.tick_timer();

            assert!(app.timer.is_none());
        }

        #[test]
        fn tick_timer_ignores_running_timers() {
            let mut app = make_app(vec![]);
            app.timer = Some(TimerState::new_stopwatch(0, "t".to_string()));

            app.tick_timer();

            assert!(!app.timer.as_ref().unwrap().rung);
        }

        #[test]
        fn settle_timer_accumulates_stopwatch_seconds_into_the_task() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![make_task("t", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE)],
            }]);
            app.timer = Some(paused_stopwatch(90, 0, "t"));

            app.settle_timer();

            assert!(app.timer.is_none());
            assert_eq!(app.projects[0].tasks[0].time_spent_secs, 90);
        }

        #[test]
        fn settle_timer_discards_a_countdown() {
            let mut app = make_app(vec![]);
            app.timer = Some(paused_countdown(60, 10));

            app.settle_timer();

            assert!(app.timer.is_none());
        }

        #[test]
        fn back_to_previous_view_restores_and_resets() {
            let mut app = make_app(vec![]);
            app.previous_view_mode = ViewMode::ViewNotes;

            app.back_to_previous_view();

            assert_eq!(app.view_mode, ViewMode::ViewNotes);
            assert_eq!(app.previous_view_mode, ViewMode::ViewProjects);
        }

        #[test]
        fn setup_reads_persisted_state() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();

            let app = App::setup();

            assert!(app.projects.is_empty());
            assert!(app.notes.is_empty());
            assert_eq!(app.view_mode, ViewMode::ViewProjects);
            assert_eq!(app.selected_project_index.selected(), Some(0));
            assert!(app.hide_done_tasks);
            assert!(!app.board_view);
            assert!(app.timer.is_none());
        }
    }

    // ------------------------------------------------------------------------
    // Rendering (all view modes, hint readouts, modal content)
    // ------------------------------------------------------------------------
    mod render {
        use super::*;
        use crate::task::{
            TASK_PRIORITY_NONE, TASK_STATUS_DONE, TASK_STATUS_ON_GOING, TASK_STATUS_UP_NEXT,
        };
        use crate::test_utils::make_task;
        use ratatui::{backend::TestBackend, Terminal};

        fn draw_with(app: &mut App, input: &Input, width: u16, height: u16) -> String {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            let mut items = vec![];
            Project::load_items(app, &mut items);
            let mut status_items = vec![];
            Task::load_statues_items(&mut status_items);
            let mut priority_items = vec![];
            Task::load_priority_items(&mut priority_items);
            let mut delete_confirm_items = vec![];
            Ui::load_delete_confirm_items(&mut delete_confirm_items);

            let frame = terminal
                .draw(|f| {
                    app.render(
                        f,
                        f.size(),
                        input,
                        &items,
                        &status_items,
                        &priority_items,
                        &delete_confirm_items,
                    )
                })
                .unwrap();

            frame.buffer.content.iter().map(|c| c.symbol()).collect()
        }

        fn draw_text(app: &mut App, width: u16, height: u16) -> String {
            let input = Input::default();
            draw_with(app, &input, width, height)
        }

        #[test]
        fn render_never_panics_in_any_view_mode() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![
                    make_task("a", TASK_STATUS_UP_NEXT, 1),
                    make_task("b", TASK_STATUS_ON_GOING, TASK_PRIORITY_NONE),
                    make_task("c", TASK_STATUS_DONE, TASK_PRIORITY_NONE),
                ],
            }]);
            app.projects[0].tasks[0].note = "note".to_string();
            app.projects[0].tasks[1].estimated_hours = 2;
            app.notes = vec![note("n", "# hi")];
            app.timer = Some(paused_countdown(60, 30));

            let modes = [
                ViewMode::ViewProjects,
                ViewMode::RenameProject,
                ViewMode::AddProject,
                ViewMode::DeleteProject,
                ViewMode::ViewTasks,
                ViewMode::RenameTask,
                ViewMode::ChangeStatusTask,
                ViewMode::ChangePriorityTask,
                ViewMode::AddTask,
                ViewMode::DeleteTask,
                ViewMode::ViewTaskDetails,
                ViewMode::EditTaskNote,
                ViewMode::SetTaskEstimate,
                ViewMode::TimerTask,
                ViewMode::SetCountdown,
                ViewMode::ViewHelp,
                ViewMode::ViewNotes,
                ViewMode::AddNote,
                ViewMode::RenameNote,
                ViewMode::DeleteNote,
                ViewMode::ViewNote,
                ViewMode::EditNote,
                ViewMode::InfoMigration,
            ];

            for mode in modes {
                app.view_mode = mode;
                app.previous_view_mode = ViewMode::ViewProjects;
                draw_text(&mut app, 80, 24);
                draw_text(&mut app, 40, 10); // tiny sizes must not panic either
            }

            // Board rendering goes through the same `show_items` path
            app.view_mode = ViewMode::ViewTasks;
            app.board_view = true;
            app.selected_task_index.select(Some(0));
            app.board_sync();
            draw_text(&mut app, 80, 24);
        }

        #[test]
        fn hint_shows_help_and_no_timer_readout_without_a_timer() {
            let mut app = make_app(vec![]);

            let text = draw_text(&mut app, 120, 60);

            assert!(text.contains("h  help"));
            assert!(!text.contains("pomodoro"));
        }

        #[test]
        fn hint_renders_a_running_stopwatch() {
            let mut app = make_app(vec![]);
            app.timer = Some(TimerState::new_stopwatch(0, "t".to_string()));

            let text = draw_text(&mut app, 120, 60);

            assert!(text.contains('▶'));
        }

        #[test]
        fn hint_renders_a_paused_stopwatch() {
            let mut app = make_app(vec![]);
            app.timer = Some(paused_stopwatch(65, 0, "short"));

            let text = draw_text(&mut app, 120, 60);

            assert!(text.contains("❚❚"));
            assert!(text.contains("00:01:05"));
            assert!(text.contains("short"));
        }

        #[test]
        fn hint_truncates_long_task_titles_with_an_ellipsis() {
            let mut app = make_app(vec![]);
            app.timer = Some(paused_stopwatch(0, 0, &"x".repeat(30)));

            let text = draw_text(&mut app, 120, 60);

            assert!(text.contains('…'));
            assert_eq!(text.matches('x').count(), 20);
        }

        #[test]
        fn hint_renders_a_low_running_countdown() {
            let mut app = make_app(vec![]);
            app.timer = Some(TimerState::new_countdown(10)); // ~10s left

            let text = draw_text(&mut app, 120, 60);

            assert!(text.contains('▼'));
        }

        #[test]
        fn hint_renders_an_unbound_timer_as_pomodoro() {
            let mut app = make_app(vec![]);
            app.timer = Some(paused_countdown(600, 60));

            let text = draw_text(&mut app, 120, 60);

            assert!(text.contains("pomodoro"));
            assert!(text.contains("00:09:00"));
        }

        #[test]
        fn timer_modal_running_countdown_turns_red_when_low() {
            let mut app = make_app(vec![]);
            // Running (started just now) with ~10s left: red readout
            app.timer = Some(TimerState::new_countdown(10));
            app.view_mode = ViewMode::TimerTask;

            let text = draw_text(&mut app, 80, 24);

            assert!(text.contains("running"));
            assert!(text.contains("█"));
        }

        #[test]
        fn timer_modal_running_countdown_is_green_when_plenty_left() {
            let mut app = make_app(vec![]);
            app.timer = Some(TimerState::new_countdown(600));
            app.view_mode = ViewMode::TimerTask;

            let text = draw_text(&mut app, 80, 24);

            assert!(text.contains("running"));
        }

        #[test]
        fn timer_modal_stopwatch_shows_an_over_estimate_in_red() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![make_task("t", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE)],
            }]);
            app.projects[0].tasks[0].estimated_hours = 1;
            app.projects[0].tasks[0].time_spent_secs = 7200; // 200%
            app.timer = Some(paused_stopwatch(0, 0, "t"));
            app.view_mode = ViewMode::TimerTask;
            app.previous_view_mode = ViewMode::ViewTasks;

            let text = draw_text(&mut app, 80, 24);

            assert!(text.contains("1h (200% spent)"));
        }

        #[test]
        fn timer_modal_bound_without_an_estimate_hides_the_estimate_line() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![make_task("t", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE)],
            }]);
            app.timer = Some(paused_stopwatch(0, 0, "t"));
            app.view_mode = ViewMode::TimerTask;
            app.previous_view_mode = ViewMode::ViewTasks;

            let text = draw_text(&mut app, 80, 24);

            assert!(text.contains("Task: t"));
            assert!(!text.contains("Estimate:"));
        }

        #[test]
        fn timer_modal_finished_countdown_shows_time_is_up() {
            let mut app = make_app(vec![]);
            app.timer = Some(paused_countdown(60, 60)); // finished
            app.view_mode = ViewMode::TimerTask;
            app.previous_view_mode = ViewMode::ViewProjects;

            let text = draw_text(&mut app, 80, 24);

            assert!(text.contains("time's up!"));
            assert!(text.contains("Press any key to close"));
        }

        #[test]
        fn timer_modal_stopwatch_shows_estimate_progress() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![make_task("t", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE)],
            }]);
            app.projects[0].tasks[0].estimated_hours = 2; // 7200s
            app.projects[0].tasks[0].time_spent_secs = 3600; // 50%
            app.timer = Some(paused_stopwatch(0, 0, "t"));
            app.view_mode = ViewMode::TimerTask;
            app.previous_view_mode = ViewMode::ViewTasks;

            let text = draw_text(&mut app, 80, 24);

            assert!(text.contains("Task: t"));
            assert!(text.contains("Estimate:"));
            assert!(text.contains("2h (50% spent)"));
        }

        #[test]
        fn timer_modal_countdown_shows_the_target() {
            let mut app = make_app(vec![]);
            app.timer = Some(paused_countdown(1500, 600));
            app.view_mode = ViewMode::TimerTask;

            let text = draw_text(&mut app, 80, 24);

            assert!(text.contains("Pomodoro"));
            assert!(text.contains("of 00:25:00"));
            // The readout itself is rendered as block digits
            assert!(text.contains("█"));
        }

        #[test]
        fn timer_modal_handles_a_missing_timer_gracefully() {
            let mut app = make_app(vec![]);
            app.view_mode = ViewMode::TimerTask;

            draw_text(&mut app, 80, 24); // must not panic
        }

        #[test]
        fn details_modal_renders_all_task_fields() {
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![make_task("t", TASK_STATUS_ON_GOING, 2)],
            }]);
            app.projects[0].tasks[0].created_at = Some(1_700_000_000);
            app.projects[0].tasks[0].completed_at = Some(1_700_000_100);
            app.projects[0].tasks[0].note = "a note".to_string();
            app.projects[0].tasks[0].time_spent_secs = 5400; // 1h 30m
            app.projects[0].tasks[0].estimated_hours = 3; // 50% of 3h
            app.view_mode = ViewMode::ViewTaskDetails;

            let text = draw_text(&mut app, 80, 24);

            assert!(text.contains("Task: t"));
            assert!(text.contains("Status: OnGoing"));
            assert!(text.contains("Priority: 2 (!!)"));
            assert!(text.contains("Note: a note"));
            assert!(text.contains("Created:"));
            assert!(text.contains("Completed:"));
            assert!(text.contains("Time Consumed:"));
            assert!(text.contains("Time Spent:"));
            assert!(text.contains("1h 30m"));
            assert!(text.contains("Estimate: 3h (50.00% spent)"));
        }

        #[test]
        fn details_modal_without_dates_or_estimate() {
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![make_task("t", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE)],
            }]);
            app.projects[0].tasks[0].created_at = None;
            app.projects[0].tasks[0].completed_at = None;
            app.view_mode = ViewMode::ViewTaskDetails;

            let text = draw_text(&mut app, 80, 24);

            assert!(text.contains("Priority: None"));
            assert!(text.contains("Estimate: none"));
            assert!(!text.contains("Created:"));
            assert!(!text.contains("Completed:"));
        }

        #[test]
        fn delete_modal_titles_the_selected_item() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![Project {
                title: "proj".to_string(),
                tasks: vec![make_task("task", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE)],
            }]);
            app.notes = vec![note("note", "")];

            app.view_mode = ViewMode::DeleteTask;
            assert!(draw_text(&mut app, 80, 24).contains("Delete \"task\"?"));

            app.view_mode = ViewMode::DeleteProject;
            assert!(draw_text(&mut app, 80, 24).contains("Delete \"proj\"?"));

            app.view_mode = ViewMode::DeleteNote;
            assert!(draw_text(&mut app, 80, 24).contains("Delete \"note\"?"));
        }

        #[test]
        fn help_over_an_empty_task_list_keeps_the_project_selection() {
            // Regression: entering a project with no tasks, opening help, and
            // letting the loop redraw used to clear `selected_project_index`
            // (the empty task list was rendered with the project state) and
            // panic in `Project::get_current` on the next frame.
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![],
            }]);
            let backend = TestBackend::new(80, 24);
            let mut terminal = Terminal::new(backend).unwrap();
            let mut input = Input::default();
            let mut items = vec![];
            Project::load_items(&mut app, &mut items);
            let mut status_items = vec![];
            Task::load_statues_items(&mut status_items);
            let mut priority_items = vec![];
            Task::load_priority_items(&mut priority_items);
            let mut delete_confirm_items = vec![];
            Ui::load_delete_confirm_items(&mut delete_confirm_items);

            // projects -> Enter -> tasks (empty) -> h -> help
            app.handle_view_projects(key(KeyCode::Enter), &mut input, &mut items);
            app.handle_view_tasks(key(KeyCode::Char('h')), &mut input, &mut items);

            for _ in 0..3 {
                terminal
                    .draw(|f| {
                        app.render(
                            f,
                            f.size(),
                            &input,
                            &items,
                            &status_items,
                            &priority_items,
                            &delete_confirm_items,
                        )
                    })
                    .unwrap();
            }

            assert_eq!(app.view_mode, ViewMode::ViewHelp);
            assert_eq!(app.selected_project_index.selected(), Some(0));
        }

        #[test]
        fn help_over_an_empty_notes_list_keeps_the_project_selection() {
            let _guard = ENV_LOCK.lock().unwrap();
            let _dir = setup_temp_config();
            let mut app = make_app(vec![]); // no projects and no notes
            app.view_mode = ViewMode::ViewHelp;
            app.previous_view_mode = ViewMode::ViewNotes;

            let text = draw_text(&mut app, 80, 24);
            draw_text(&mut app, 80, 24);

            assert!(text.contains("Press any key to close"));
            assert_eq!(app.selected_project_index.selected(), Some(0));
        }

        #[test]
        fn help_modal_lists_the_bindings_of_the_previous_view() {
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![make_task("t", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE)],
            }]);
            app.view_mode = ViewMode::ViewHelp;

            app.previous_view_mode = ViewMode::ViewProjects;
            assert!(draw_text(&mut app, 80, 24).contains("go to tasks"));

            app.previous_view_mode = ViewMode::ViewNotes;
            assert!(draw_text(&mut app, 80, 24).contains("preview"));

            app.previous_view_mode = ViewMode::ViewNote;
            assert!(draw_text(&mut app, 80, 24).contains("scroll"));

            app.previous_view_mode = ViewMode::ViewTasks;
            app.board_view = false;
            assert!(draw_text(&mut app, 80, 24).contains("toggle done"));

            app.board_view = true;
            assert!(draw_text(&mut app, 80, 24).contains("switch lane"));

            app.previous_view_mode = ViewMode::ViewTaskDetails; // no bindings
            assert!(draw_text(&mut app, 80, 24).contains("Press any key to close"));
        }

        #[test]
        fn input_modal_scrolls_a_long_value() {
            let mut app = make_app(vec![]);
            app.view_mode = ViewMode::AddProject;
            let input = input_with(&"x".repeat(200));

            let text = draw_with(&mut app, &input, 40, 10);

            assert!(text.contains(&"x".repeat(30)));
        }
    }
}
