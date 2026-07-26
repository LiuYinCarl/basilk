use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Clear, HighlightSpacing, List, ListItem, Paragraph, Wrap},
    Frame,
};
use tui_input::Input;

use crate::{
    project::Project,
    task::{Task, TASK_PRIORITY_NONE},
    timer::TimerKind,
    ui::Ui,
    util::Util,
    App, ViewMode,
};

pub struct View {}

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

        let readout_color = if running_low {
            Color::Red
        } else if timer.is_running() {
            Color::Green
        } else {
            Color::Yellow
        };

        let mut lines = vec![
            Line::from(Span::styled(
                readout,
                Style::default()
                    .fg(readout_color)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
        ];

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

        lines.push(Line::from(Span::styled(
            if timer.is_running() {
                "running"
            } else {
                "paused"
            },
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "Space pause/resume · Enter stop · Esc background",
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

        Ui::create_modal(f, 50, 13, area, widget)
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
        // Timer modals show the list of the view they were opened from
        let from_projects =
            matches!(
                app.view_mode,
                ViewMode::ViewProjects
                    | ViewMode::AddProject
                    | ViewMode::RenameProject
                    | ViewMode::DeleteProject
            ) || (matches!(app.view_mode, ViewMode::TimerTask | ViewMode::SetCountdown)
                && app.previous_view_mode == ViewMode::ViewProjects);

        let block: Block = if from_projects {
            Block::bordered()
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
        {
            f.render_widget(items, area)
        } else {
            f.render_stateful_widget(items, area, app.use_state());
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

    pub fn show_help_modal(app: &mut App, f: &mut Frame, area: Rect) {
        let bindings: &[(&str, &str)] = match app.previous_view_mode {
            ViewMode::ViewProjects => &[
                ("↑ ↓  k j", "next/prev"),
                ("Enter  →  l", "go to tasks"),
                ("n", "new"),
                ("r", "rename"),
                ("d", "delete"),
                ("c", "pomodoro timer"),
                ("h", "help"),
                ("q", "quit"),
            ],
            ViewMode::ViewTasks => &[
                ("↑ ↓  k j", "next/prev"),
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

        Ui::create_modal(f, 42, 19, area, widget)
    }
}
