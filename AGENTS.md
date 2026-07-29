# Basilk Agent Guide

Basilk is a TUI-based kanban task manager written in Rust using `ratatui`.

## Essential Commands

- **Build**: `cargo build`
- **Run**: `cargo run`
- **Test**: `cargo test` (unit tests live in per-module `#[cfg(test)]` blocks; `src/property_tests.rs` holds proptest-based fuzz/property tests)
- **Lint**: `cargo fmt --all -- --check` (used in CI)
- **Format**: `cargo fmt`

## Project Structure

- `src/main.rs`: Entry point, terminal initialization, main event loop, and app state management.
- `src/app.rs` (Wait, I saw `App` in `main.rs`, let me check if there is a separate file or it's all in `main.rs`): App struct and core logic are in `main.rs`.
- `src/cli.rs`: Simple CLI argument handling (e.g., `--version`).
- `src/config.rs`: Configuration management (TOML format).
- `src/json.rs`: Data persistence layer (JSON format).
- `src/migration.rs`: JSON data schema migrations.
- `src/project.rs`: Project data model and logic.
- `src/task.rs`: Task data model, status/priority constants, and logic.
- `src/timer.rs`: Task-bound stopwatch/countdown runtime state (not persisted); on settle the elapsed seconds are accumulated into `Task.time_spent_secs`.
- `src/ui.rs`: UI utility functions for creating modals and layouts.
- `src/view.rs`: Higher-level UI rendering logic (rendering specific views/modals).
- `src/util.rs`: Miscellaneous utility functions.
- `src/property_tests.rs`: `#[cfg(test)]`-only module with proptest property/fuzz tests.

## Code Patterns

### App State Management
The `App` struct in `main.rs` manages the application state, including selected indices for projects and tasks, the current `ViewMode`, and loaded data.

### View Modes
`ViewMode` enum in `main.rs` defines the different screens and states (e.g., `ViewProjects`, `AddTask`, `ViewTasks`).

### Data Persistence
- Data is stored in `basilk_data.json` (a versioned wrapper around the project list) in the user's config directory; older releases used per-version files (e.g., `911fc.json`) that are migrated on startup.
- `Json::read()` and `Json::write()` handle loading and saving the entire project list.
- Set the `BASILK_CONFIG_DIR` env var to redirect storage (used by tests with a temp dir).
- Migrations are handled in `migration.rs` by mapping version hashes to transformation functions.

### TUI Logic
- Uses `ratatui` with `crossterm` backend.
- `App::render` in `main.rs` delegates rendering to `View` methods in `view.rs`.
- Modals are created using `Ui` helper methods in `ui.rs`.

## Conventions

- **Naming**: Standard Rust naming conventions (CamelCase for types, snake_case for functions/variables).
- **Static Constants**: Used for task statuses (`TASK_STATUS_DONE`, etc.) and priorities.
- **Error Handling**: Uses `Box<dyn Error>` in `main` and `Result` elsewhere. `unwrap()` is frequently used in data operations.

## Gotchas

- **Input Handling**: `tui-input` is used for text fields. Note that the event loop in `main.rs` filters for `KeyEventKind::Press` to avoid double-processing on Windows.
- **Event Loop**: The main loop uses `event::poll(250ms)` instead of a blocking read so the task timer (`App.timer`) can tick and redraw every second; `App::tick_timer` runs after each iteration.
- **Timers**: `s` in the task view starts a stopwatch bound to the selected task; `c` in either view starts a global pomodoro countdown (`src/timer.rs`, binding is `Option<TimerTaskBinding>`). A stopwatch persists its seconds on settle (`App::settle_timer` → `Task::add_time_spent` into `time_spent_secs`): on stop or on quit; a pomodoro never persists — at zero it rings the terminal bell once and stays visible in a finished state (modal shows big block digits and "time's up!") until the user dismisses it with any key. Timers keep running across view switches; deleting the bound task drops the timer, deleting a project drops or re-indexes it. Timer modals return to the view they were opened from via `previous_view_mode`.
- **Time Estimate**: Each task has an `estimated_hours` field (0 = no estimate, editable with `g` in the task details view); the details view and timer modal show the percentage of the estimate already spent, and the task list renders a `[x%]` suffix. Progress math lives in `Task::estimate_progress` (saturating arithmetic — arbitrary JSON values must not overflow). Timers are bound to a task by `(project_index, task_title)`; renaming a bound task updates the timer, deleting it drops the timer.
- **Data Loading**: `Project::reload` and `Task::reload` read the entire JSON file from disk. Changes are written back to disk immediately after most operations (create, rename, delete, change status/priority).
- **Sorting**: Tasks are sorted during `Task::load_items`. `sort_by_key` is stable and the priority sort runs last, so the final order is **priority-major** with status as the tie-breaker; done tasks (priority reset to NONE) end up last.
- **Board View**: `b` in the task view toggles a kanban board (three lanes: Up Next / On Going / Done) rendered by `View::show_board` in `view.rs`. It is a display mode of `ViewMode::ViewTasks`, not a separate `ViewMode`, so all task keybindings work unchanged. State lives in `App.board_view`, `App.board_lane`, and `App.board_lane_states` (per-lane `ListState`); `selected_task_index` (full sorted-list index) remains the selection source of truth — lane navigation just translates (lane, row) into it via `Task::lane_indices`, and `App::board_sync` (called at the end of `Task::load_items` when the board is active, plus explicitly after `DeleteTask`'s `select_previous` via `App::delete_current_task`) re-derives the lane/row after any mutation so the focus follows a task that changed status. While the board is active, `Task::load_items` skips the `hide_done_tasks` filter (the board always shows the Done lane) and `t` is a no-op; `←`/`→` switch lanes (`←` no longer goes back to projects), `Esc` still does. Task lines are built by the shared `Task::repr_spans` helper so the list and board renderings cannot drift.
- **Testing**: Tests share fixtures from `test_utils` in `main.rs`. Any test touching the disk layer must hold `ENV_LOCK` and use `setup_temp_config()` (sets `BASILK_CONFIG_DIR`).
- **Migrations**: If you change the data schema (e.g., in `Project` or `Task` structs), you **must** add a new migration in `migration.rs` and update `JSON_VERSIONS`.

## Configuration
There is currently no config file mechanism in the codebase (an earlier `src/config.rs` with `ui.show_help` no longer exists). All behavior is compiled in; per-task settings like `estimated_hours` live in the JSON data.
