use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Clear, HighlightSpacing, List, ListItem, Paragraph, Wrap},
    Frame,
};
use tui_input::Input;

use crate::{
    markdown::render_markdown,
    note::Note,
    project::Project,
    task::{Task, TASK_PRIORITY_NONE, TASK_STATUSES},
    timer::TimerKind,
    ui::Ui,
    util::Util,
    App, ViewMode,
};

pub struct View {}

/// Approximate number of lines a paragraph occupies once wrapped to
/// `width` columns (ratatui 0.27 keeps `Paragraph::line_count` private).
/// Word wrapping can produce slightly more lines than this estimate;
/// it is only used to clamp the note preview scroll offset.
fn wrapped_line_count(text: &Text, width: u16) -> usize {
    let width = usize::from(width).max(1);

    text.lines
        .iter()
        .map(|line| line.width().div_ceil(width))
        .map(|n| n.max(1))
        .sum()
}

/// One 5-row block glyph for a digit or colon in the big timer readout.
fn digit_glyph(c: char) -> [&'static str; 5] {
    match c {
        '0' => ["███", "█ █", "█ █", "█ █", "███"],
        '1' => ["  █", "  █", "  █", "  █", "  █"],
        '2' => ["███", "  █", "███", "█  ", "███"],
        '3' => ["███", "  █", "███", "  █", "███"],
        '4' => ["█ █", "█ █", "███", "  █", "  █"],
        '5' => ["███", "█  ", "███", "  █", "███"],
        '6' => ["███", "█  ", "███", "█ █", "███"],
        '7' => ["███", "  █", "  █", "  █", "  █"],
        '8' => ["███", "█ █", "███", "█ █", "███"],
        '9' => ["███", "█ █", "███", "  █", "███"],
        ':' => [" ", "█", " ", "█", " "],
        _ => ["   ", "   ", "   ", "   ", "   "],
    }
}

/// Render an HH:MM:SS readout as 5-row block digits so the timer is
/// readable at a glance instead of a single text line.
fn big_digit_lines(readout: &str, color: Color) -> Vec<Line<'static>> {
    (0..5)
        .map(|row| {
            let text = readout
                .chars()
                .map(|c| digit_glyph(c)[row])
                .collect::<Vec<_>>()
                .join(" ");
            Line::from(Span::styled(
                text,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ))
        })
        .collect()
}

impl View {
    pub fn show_new_item_modal(f: &mut Frame, area: Rect, input: &Input) {
        Ui::create_input_modal("New", f, area, input)
    }

    pub fn show_migration_info_modal(f: &mut Frame, area: Rect) {
        let widget = Paragraph::new(Text::from(vec![
            Line::raw("New migrations were applied!"),
            Line::raw("Check the changelog"),
        ]))
        .alignment(Alignment::Center)
        .block(Block::bordered());

        Ui::create_modal(f, 30, 4, area, widget)
    }

    pub fn show_rename_item_modal(f: &mut Frame, area: Rect, input: &Input) {
        Ui::create_input_modal("Rename", f, area, input)
    }

    pub fn show_edit_note_modal(f: &mut Frame, area: Rect, input: &Input) {
        Ui::create_input_modal("Note", f, area, input)
    }

    pub fn show_countdown_modal(f: &mut Frame, area: Rect, input: &Input) {
        Ui::create_input_modal("Countdown (minutes)", f, area, input)
    }

    pub fn show_task_estimate_modal(f: &mut Frame, area: Rect, input: &Input) {
        Ui::create_input_modal("Estimate (hours, 0 = none)", f, area, input)
    }

    pub fn show_timer_modal(app: &App, f: &mut Frame, area: Rect) {
        let Some(timer) = app.timer.as_ref() else {
            return;
        };

        let finished = timer.is_finished();

        let (readout, target) = match timer.kind {
            TimerKind::Stopwatch => (Util::format_secs(timer.elapsed().as_secs()), None),
            TimerKind::Countdown => (
                Util::format_secs(timer.remaining().unwrap_or_default().as_secs()),
                Some(Util::format_secs(timer.target_secs)),
            ),
        };

        let running_low = timer.is_running()
            && timer.kind == TimerKind::Countdown
            && timer.remaining().unwrap_or_default().as_secs() <= 10;

        let readout_color = if finished || running_low {
            Color::Red
        } else if timer.is_running() {
            Color::Green
        } else {
            Color::Yellow
        };

        let mut lines = vec![Line::raw("")];
        lines.extend(big_digit_lines(&readout, readout_color));
        lines.push(Line::raw(""));

        if let Some(target) = target {
            lines.push(Line::from(Span::styled(
                format!("of {}", target),
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::raw(""));
        }

        match &timer.bound {
            Some(bound) => {
                lines.push(Line::from(vec![
                    Span::styled("Task: ", Style::default().fg(Color::Cyan)),
                    Span::raw(&bound.task_title),
                ]));
                lines.push(Line::raw(""));

                // Live estimate progress, including the unsettled time on this timer
                let estimate_line = app
                    .projects
                    .get(bound.project_index)
                    .and_then(|p| p.tasks.iter().find(|t| t.title == bound.task_title))
                    .and_then(|t| {
                        t.estimate_progress(timer.elapsed().as_secs())
                            .map(|pct| (t.estimated_hours, pct))
                    });

                if let Some((hours, pct)) = estimate_line {
                    lines.push(Line::from(vec![
                        Span::styled("Estimate: ", Style::default().fg(Color::Cyan)),
                        Span::styled(
                            format!("{}h ({}% spent)", hours, pct),
                            Style::default().fg(if pct >= 100 {
                                Color::Red
                            } else {
                                Color::DarkGray
                            }),
                        ),
                    ]));
                    lines.push(Line::raw(""));
                }
            }
            None => {
                lines.push(Line::from(Span::styled(
                    "pomodoro",
                    Style::default().fg(Color::Cyan),
                )));
                lines.push(Line::raw(""));
            }
        }

        let status = if finished {
            "time's up!"
        } else if timer.is_running() {
            "running"
        } else {
            "paused"
        };
        lines.push(Line::from(Span::styled(
            status,
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            if finished {
                "Press any key to close"
            } else {
                "Space pause/resume · Enter stop · Esc background"
            },
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )));

        let title = match timer.kind {
            TimerKind::Stopwatch => " Timer ",
            TimerKind::Countdown => " Pomodoro ",
        };

        let widget = Paragraph::new(Text::from(lines))
            .alignment(Alignment::Center)
            .block(Block::bordered().title(title));

        Ui::create_modal(f, 60, 18, area, widget)
    }

    pub fn show_delete_item_modal(
        app: &mut App,
        confirm_items: &Vec<ListItem>,
        f: &mut Frame,
        area: Rect,
    ) {
        let title = match app.view_mode {
            ViewMode::DeleteTask => &Task::get_current(app).title,
            ViewMode::DeleteProject => &Project::get_current(app).title,
            ViewMode::DeleteNote => &Note::get_current(app).title,
            _ => "",
        };

        let area = Ui::create_rect_area(30, 5, area);

        let list_widget = List::new(confirm_items.clone())
            .highlight_style(Style::default().add_modifier(Modifier::BOLD))
            .highlight_symbol("> ")
            .highlight_spacing(HighlightSpacing::Always)
            .block(Block::bordered().title(format!("Delete \"{}\"?", title)));

        f.render_widget(Clear, area);
        f.render_stateful_widget(list_widget, area, app.use_state())
    }

    pub fn show_select_task_status_modal(
        app: &mut App,
        status_items: &Vec<ListItem>,
        f: &mut Frame,
        area: Rect,
    ) {
        let area = Ui::create_rect_area(20, 5, area);

        let task_status_list_widget = List::new(status_items.clone())
            .highlight_style(Style::default().add_modifier(Modifier::BOLD))
            .highlight_symbol("> ")
            .highlight_spacing(HighlightSpacing::Always)
            .block(Block::bordered().title("Status"));

        f.render_widget(Clear, area);
        f.render_stateful_widget(task_status_list_widget, area, app.use_state())
    }

    pub fn show_select_task_priority_modal(
        app: &mut App,
        priority_items: &Vec<ListItem>,
        f: &mut Frame,
        area: Rect,
    ) {
        let area = Ui::create_rect_area(20, 6, area);

        let task_status_list_widget = List::new(priority_items.clone())
            .highlight_style(Style::default().add_modifier(Modifier::BOLD))
            .highlight_symbol("> ")
            .highlight_spacing(HighlightSpacing::Always)
            .block(Block::bordered().title("Priority"));

        f.render_widget(Clear, area);
        f.render_stateful_widget(task_status_list_widget, area, app.use_state())
    }

    pub fn show_items(app: &mut App, items: &Vec<ListItem>, f: &mut Frame, area: Rect) {
        // Timer modals show the list of the view they were opened from;
        // the help modal shows the list of the view it was opened from
        // (classified via `previous_view_mode`). Without this, opening
        // help over the projects/notes view would fall into the tasks
        // branch and call `Project::get_current`, which panics when the
        // project list is empty (or its selection was cleared by an
        // empty-list render).
        let from_projects =
            matches!(
                app.view_mode,
                ViewMode::ViewProjects
                    | ViewMode::AddProject
                    | ViewMode::RenameProject
                    | ViewMode::DeleteProject
                    | ViewMode::InfoMigration
            ) || (matches!(app.view_mode, ViewMode::TimerTask | ViewMode::SetCountdown)
                && app.previous_view_mode == ViewMode::ViewProjects)
                || (app.view_mode == ViewMode::ViewHelp
                    && app.previous_view_mode == ViewMode::ViewProjects);

        let from_notes = matches!(
            app.view_mode,
            ViewMode::ViewNotes | ViewMode::AddNote | ViewMode::RenameNote | ViewMode::DeleteNote
        ) || (app.view_mode == ViewMode::ViewHelp
            && matches!(
                app.previous_view_mode,
                ViewMode::ViewNotes | ViewMode::ViewNote | ViewMode::EditNote
            ));

        if !from_projects && !from_notes && app.board_view {
            View::show_board(app, f, area);
            return;
        }

        let block: Block = if from_projects {
            Block::bordered()
        } else if from_notes {
            Block::bordered().title(" Notes ")
        } else {
            Block::bordered().title(Util::get_spaced_title(&Project::get_current(app).title))
        };

        // Iterate through all elements in the `items` and stylize them.
        let items = items.clone();

        // Create a List from all list items and highlight the currently selected one
        let items = List::new(items)
            .highlight_style(Style::default().add_modifier(Modifier::BOLD))
            .highlight_symbol("> ")
            .highlight_spacing(HighlightSpacing::Always)
            .block(block);

        if app.view_mode == ViewMode::ChangeStatusTask
            || app.view_mode == ViewMode::ChangePriorityTask
            || app.view_mode == ViewMode::DeleteTask
            || app.view_mode == ViewMode::DeleteProject
            || app.view_mode == ViewMode::DeleteNote
        {
            f.render_widget(items, area)
        } else {
            f.render_stateful_widget(items, area, app.use_state());
        }
    }

    /// Kanban-style board: one vertical lane per task status. The focused
    /// lane gets the status color for its border and title; every lane is
    /// always shown (the Done lane ignores `hide_done_tasks`).
    pub fn show_board(app: &mut App, f: &mut Frame, area: Rect) {
        const LANE_TITLES: [&str; 3] = ["Up Next", "On Going", "Done"];

        let outer =
            Block::bordered().title(Util::get_spaced_title(&Project::get_current(app).title));
        let inner = outer.inner(area);
        f.render_widget(outer, area);

        let columns = Layout::horizontal([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .spacing(1)
        .split(inner);

        for (lane, status) in TASK_STATUSES.into_iter().enumerate() {
            let indices = Task::lane_indices(app, status);
            let status_color = Task::get_status_color(&status.to_string());
            let focused = lane == app.board_lane;

            let lane_style = if focused {
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let lane_block = Block::bordered()
                .title(Line::from(Span::styled(
                    format!(" {} ({}) ", LANE_TITLES[lane], indices.len()),
                    if focused {
                        lane_style
                    } else {
                        Style::default()
                    },
                )))
                .border_style(lane_style);

            if indices.is_empty() {
                let placeholder = Paragraph::new(Line::from(Span::styled(
                    "(empty)",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )))
                .alignment(Alignment::Center)
                .block(lane_block);

                f.render_widget(placeholder, columns[lane]);
                continue;
            }

            // Own every string so the item list outlives the `tasks` borrow
            // (the mutable lane state is borrowed again at render time).
            let lane_items: Vec<ListItem> = {
                let tasks = &app.projects[app.selected_project_index.selected().unwrap()].tasks;

                indices
                    .iter()
                    .map(|&i| ListItem::new(Line::from(Task::repr_spans(&tasks[i], false))))
                    .collect()
            };

            let list = List::new(lane_items)
                .highlight_style(Style::default().add_modifier(Modifier::BOLD))
                .highlight_symbol("> ")
                .highlight_spacing(HighlightSpacing::Always)
                .block(lane_block);

            f.render_stateful_widget(list, columns[lane], &mut app.board_lane_states[lane]);
        }
    }

    pub fn show_task_details_modal(app: &mut App, f: &mut Frame, area: Rect) {
        let task = Task::get_current(app);

        let priority_text = if task.priority == TASK_PRIORITY_NONE {
            "None".to_string()
        } else {
            format!(
                "{} ({})",
                task.priority,
                Util::get_priority_indicator(task.priority)
            )
        };

        let mut lines = vec![
            Line::from(vec![
                Span::styled(
                    "Task: ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(&task.title),
            ]),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    &task.status,
                    Style::default().fg(Task::get_status_color(&task.status)),
                ),
            ]),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Priority: ", Style::default().fg(Color::Cyan)),
                Span::styled(priority_text, Style::default().fg(Color::Red)),
            ]),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Note: ", Style::default().fg(Color::Cyan)),
                Span::raw(&task.note),
            ]),
            Line::raw(""),
        ];

        // Add creation time if available
        if task.created_at.is_some() {
            lines.push(Line::from(vec![
                Span::styled("Created: ", Style::default().fg(Color::Cyan)),
                Span::raw(Util::format_timestamp(task.created_at)),
            ]));
            lines.push(Line::raw(""));
        }

        // Add completion time if available
        if task.completed_at.is_some() {
            lines.push(Line::from(vec![
                Span::styled("Completed: ", Style::default().fg(Color::Cyan)),
                Span::raw(Util::format_timestamp(task.completed_at)),
            ]));
            lines.push(Line::raw(""));
        }

        // Add time consumed if both timestamps are available
        if task.created_at.is_some() {
            lines.push(Line::from(vec![
                Span::styled("Time Consumed: ", Style::default().fg(Color::Cyan)),
                Span::raw(Util::format_duration(task.created_at, task.completed_at)),
            ]));
            lines.push(Line::raw(""));
        }

        // Accumulated timer time vs. the estimated duration
        lines.push(Line::from(vec![
            Span::styled("Time Spent: ", Style::default().fg(Color::Cyan)),
            Span::raw(format!(
                "{}h {}m ({})",
                task.time_spent_secs / 3600,
                (task.time_spent_secs % 3600) / 60,
                Util::format_secs(task.time_spent_secs),
            )),
        ]));
        lines.push(Line::raw(""));

        let estimate_text = if task.estimated_hours > 0 {
            format!(
                "{}h ({:.2}% spent)",
                task.estimated_hours,
                task.time_spent_secs as f64 / task.estimated_hours.saturating_mul(3600) as f64
                    * 100.0,
            )
        } else {
            "none".to_string()
        };
        lines.push(Line::from(vec![
            Span::styled("Estimate: ", Style::default().fg(Color::Cyan)),
            Span::raw(estimate_text),
        ]));
        lines.push(Line::raw(""));

        lines.push(Line::raw(""));
        lines.push(Line::from(vec![Span::styled(
            "e edit note · g edit estimate · any other key to close",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )]));

        let widget = Paragraph::new(Text::from(lines))
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title(" Task Details "));

        Ui::create_modal(f, 60, 22, area, widget)
    }

    /// Full-page note preview: the Markdown body rendered with styles,
    /// scrollable via `app.note_scroll` (clamped here against the wrapped
    /// content height so `G` can just set `u16::MAX`).
    pub fn show_note(app: &mut App, f: &mut Frame, area: Rect) {
        let (title, body) = {
            let note = Note::get_current(app);
            (note.title.clone(), note.body.clone())
        };

        let text = if body.trim().is_empty() {
            Text::from(Line::from(Span::styled(
                "empty note — press e to write some Markdown",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )))
        } else {
            render_markdown(&body)
        };

        let inner_height = area.height.saturating_sub(2) as usize;
        let content_lines = wrapped_line_count(&text, area.width.saturating_sub(2));

        let paragraph = Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(Block::bordered().title(Util::get_spaced_title(&title)));

        app.note_scroll = app
            .note_scroll
            .min(content_lines.saturating_sub(inner_height) as u16);

        f.render_widget(paragraph.scroll((app.note_scroll, 0)), area);
    }

    /// Full-page note editor: the `tui-textarea` created when entering
    /// `ViewMode::EditNote`, rendered with its own block and cursor.
    pub fn show_note_editor(app: &mut App, f: &mut Frame, area: Rect) {
        if let Some(textarea) = app.note_textarea.as_mut() {
            f.render_widget(&*textarea, area);
        }
    }

    pub fn show_help_modal(app: &mut App, f: &mut Frame, area: Rect) {
        let bindings: &[(&str, &str)] = match app.previous_view_mode {
            ViewMode::ViewProjects => &[
                ("↑ ↓  k j", "next/prev"),
                ("Enter  →  l", "go to tasks"),
                ("m", "notes"),
                ("n", "new"),
                ("r", "rename"),
                ("d", "delete"),
                ("c", "pomodoro timer"),
                ("h", "help"),
                ("q", "quit"),
            ],
            ViewMode::ViewNotes => &[
                ("↑ ↓  k j", "next/prev"),
                ("Enter  →  l  v", "preview"),
                ("n", "new"),
                ("r", "rename"),
                ("d", "delete"),
                ("Esc  ←", "back"),
                ("h", "help"),
                ("q", "quit"),
            ],
            ViewMode::ViewNote => &[
                ("↑ ↓  k j", "scroll"),
                ("PgUp  PgDn", "page up/down"),
                ("g  G", "top/bottom"),
                ("e", "edit (Esc saves)"),
                ("Esc  Enter", "back"),
                ("h", "help"),
                ("q", "quit"),
            ],
            ViewMode::ViewTasks if app.board_view => &[
                ("↑ ↓  k j", "next/prev in lane"),
                ("← →", "switch lane"),
                ("b", "list view"),
                ("Esc", "back"),
                ("Enter", "change status"),
                ("p", "change priority"),
                ("n", "new"),
                ("r", "rename"),
                ("v", "details"),
                ("e", "note"),
                ("v → g", "details: edit estimate"),
                ("d", "delete"),
                ("s", "stopwatch timer"),
                ("c", "pomodoro timer"),
                ("h", "help"),
                ("q", "quit"),
            ],
            ViewMode::ViewTasks => &[
                ("↑ ↓  k j", "next/prev"),
                ("b", "board view"),
                ("Esc  ←", "back"),
                ("Enter", "change status"),
                ("p", "change priority"),
                ("n", "new"),
                ("r", "rename"),
                ("v", "details"),
                ("e", "note"),
                ("v → g", "details: edit estimate"),
                ("d", "delete"),
                ("t", "toggle done"),
                ("s", "stopwatch timer"),
                ("c", "pomodoro timer"),
                ("h", "help"),
                ("q", "quit"),
            ],
            _ => &[],
        };

        let mut lines: Vec<Line> = bindings
            .iter()
            .map(|(keys, desc)| {
                Line::from(vec![
                    Span::styled(
                        format!("{:<16}", keys),
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(*desc),
                ])
            })
            .collect();

        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "Press any key to close",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )));

        let widget = Paragraph::new(Text::from(lines))
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title(" Help "));

        // Sized with a little headroom above the longest binding list
        Ui::create_modal(f, 42, 23, area, widget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        note::Note,
        project::Project,
        task::{TASK_PRIORITY_NONE, TASK_STATUS_DONE, TASK_STATUS_ON_GOING, TASK_STATUS_UP_NEXT},
        test_utils::{make_app, make_task},
    };
    use ratatui::{backend::TestBackend, Terminal};
    use tui_textarea::TextArea;

    /// Rendering must not panic on any lane composition (all lanes filled,
    /// some empty, board narrower than the content).
    #[test]
    fn show_board_renders_without_panicking() {
        for (width, height) in [(120, 30), (60, 20), (30, 10), (12, 4)] {
            let mut app = make_app(vec![Project {
                title: "p".to_string(),
                tasks: vec![
                    make_task("up-next", TASK_STATUS_UP_NEXT, 1),
                    make_task("on-going", TASK_STATUS_ON_GOING, TASK_PRIORITY_NONE),
                    make_task("done", TASK_STATUS_DONE, TASK_PRIORITY_NONE),
                ],
            }]);
            app.board_view = true;
            app.board_sync();

            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|f| View::show_board(&mut app, f, f.size()))
                .unwrap();
        }
    }

    #[test]
    fn show_board_renders_empty_lanes_without_panicking() {
        let mut app = make_app(vec![Project {
            title: "p".to_string(),
            tasks: vec![make_task("only", TASK_STATUS_UP_NEXT, TASK_PRIORITY_NONE)],
        }]);
        app.board_view = true;
        app.board_sync();

        let backend = TestBackend::new(90, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| View::show_board(&mut app, f, f.size()))
            .unwrap();
    }

    fn note_app(body: String) -> App {
        let mut app = make_app(vec![]);
        app.notes = vec![Note {
            title: "n".to_string(),
            body,
            created_at: None,
            updated_at: None,
        }];
        app
    }

    /// The preview must render at any size and clamp an out-of-range
    /// scroll offset (e.g. after pressing `G`) into the content.
    #[test]
    fn show_note_renders_without_panicking_and_clamps_scroll() {
        for (width, height) in [(120, 30), (60, 20), (30, 10), (12, 4)] {
            let mut app = note_app("# Title\n\nsome **bold** text\n\n- a\n- b\n".repeat(20));
            app.note_scroll = u16::MAX;

            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|f| View::show_note(&mut app, f, f.size()))
                .unwrap();

            assert!(app.note_scroll < u16::MAX);
        }
    }

    #[test]
    fn show_note_renders_empty_body_hint() {
        let mut app = note_app(String::new());

        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| View::show_note(&mut app, f, f.size()))
            .unwrap();
    }

    #[test]
    fn show_note_editor_renders_without_panicking() {
        let mut app = note_app(String::new());
        app.note_textarea = Some(TextArea::from(vec!["# hi".to_string(), "body".to_string()]));

        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| View::show_note_editor(&mut app, f, f.size()))
            .unwrap();
    }

    /// Regression test: opening help over the notes view (or the projects
    /// view) with an empty project list used to fall into the tasks
    /// branch of `show_items` and panic in `Project::get_current` — the
    /// more so because ratatui clears the list selection (`select(None)`)
    /// when rendering an empty list.
    #[test]
    fn show_items_with_help_over_notes_or_projects_tolerates_empty_projects() {
        for previous in [ViewMode::ViewNotes, ViewMode::ViewProjects] {
            let mut app = make_app(vec![]);
            app.view_mode = ViewMode::ViewHelp;
            app.previous_view_mode = previous;
            app.selected_project_index.select(None);
            let items = vec![];

            let backend = TestBackend::new(80, 24);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|f| View::show_items(&mut app, &items, f, f.size()))
                .unwrap();
        }
    }
}
