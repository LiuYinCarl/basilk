# Basilk Agent Guide

Basilk is a TUI-based kanban task manager written in Rust using `ratatui`.

## Essential Commands

- **Build**: `cargo build`
- **Run**: `cargo run`
- **Test**: `cargo test` (unit tests live in per-module `#[cfg(test)]` blocks; `src/property_tests.rs` holds proptest-based fuzz/property tests)
- **Coverage**: `cargo llvm-cov --workspace` (install with `cargo install cargo-llvm-cov`; CI runs it via `taiki-e/install-action` with `--fail-under-lines 95`). Local runs measure ~99.5% line coverage. The only uncovered lines are the real-terminal glue in `main.rs` (`main`, `init_terminal`, `restore_terminal`, `CrosstermSource::next_key`), which cannot run inside a unit test; everything else — including the full event loop and every view/modal render — is covered by in-process tests.
- **Releases**: Tag-driven, no release bot. The version in `Cargo.toml`/`Cargo.lock` is bumped manually — `./scripts/bump-version.sh [patch|minor|major]` (commits and tags `vX.Y.Z`) — and pushing that tag triggers `.github/workflows/release.yml`, which verifies the tag matches `Cargo.toml`, builds binaries for Linux / macOS (Intel + ARM) / Windows and publishes a GitHub Release. The workflow never bumps versions or commits on its own; `workflow_dispatch` re-publishes an existing tag (takes the tag name as input). A `concurrency: group: release` queue prevents racing builds.
- **Benchmarks**: `./scripts/bench.sh` (or `cargo test --release --bin basilk perf_tests -- --ignored --nocapture`) runs the opt-in `perf_tests` module — scaling of `load_items`, full-frame render at 2k tasks, Markdown rendering, JSON persistence. Typical release-build numbers: ~0.3 ms/frame at 2k tasks (250 ms budget), ~2 ms for 10k-task `load_items`, idle CPU 0%.
- **Lint**: `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings` (both used in CI)
- **Format**: `cargo fmt`

## Project Structure

- `src/main.rs`: Entry point, terminal initialization, main event loop, and app state management.
- `src/app.rs` (Wait, I saw `App` in `main.rs`, let me check if there is a separate file or it's all in `main.rs`): App struct and core logic are in `main.rs`.
- `src/cli.rs`: Simple CLI argument handling (e.g., `--version`).
- `src/config.rs`: Configuration management (TOML format).
- `src/json.rs`: Data persistence layer (JSON format).
- `src/markdown.rs`: Markdown → ratatui `Text` renderer (`pulldown-cmark` event stream + style stack) used by the note preview.
- `src/migration.rs`: JSON data schema migrations.
- `src/note.rs`: Global note data model and logic (project-independent Markdown memos).
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
- Data is stored in `basilk_data.json` (a versioned wrapper around the project list, plus a global `notes` list) in the user's config directory; older releases used per-version files (e.g., `911fc.json`) that are migrated on startup.
- `Json::read()` and `Json::write()` handle loading and saving the entire project list; `Json::read_notes()` / `Json::write_notes()` do the same for notes. Both write paths read the counterpart back from disk first, so writing projects never drops notes and vice versa.
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
- **Notes**: `m` in the project view opens the global notes list (`ViewMode::ViewNotes`); notes live in `App.notes` (model in `src/note.rs`) and are stored in the same `basilk_data.json` wrapper (`#[serde(default)]`, so pre-notes files load unchanged; the `e5a1c` migration only bumps the version). `Enter`/`v` opens `ViewNote`, a full-page Markdown preview (`View::show_note` → `markdown::render_markdown`) scrolled via `App.note_scroll`, which is clamped at render time against an estimated wrapped line count (`G` just sets `u16::MAX`). `e` opens `EditNote`, a full-page `tui-textarea` editor stored in `App.note_textarea` (created on entry from the body lines); every key except `Esc` is forwarded to the textarea, and `Esc` saves (`Note::update_body`) and returns to the preview. `ViewNote`/`EditNote` bypass `View::show_items` in `App::render` and take the whole main area.
- **Testing**: Tests share fixtures from `test_utils` in `main.rs`. Any test touching the disk layer must hold `ENV_LOCK` and use `setup_temp_config()` (sets `BASILK_CONFIG_DIR`).
  - **Event loop**: `App::run_with_source` drives the whole draw/dispatch/tick loop against an injected `KeySource`; tests feed it a queue of synthetic `KeyEvent`s (see `tests::event_loop`). `CrosstermSource` (the real terminal poll/read) is the one `KeySource` that tests cannot drive in-process.
  - **Rendering**: `App::render` (and the `View::show_*` functions) are exercised with `ratatui::TestBackend` across every `ViewMode`, including the timer/help/details/delete modals and the board view; `tests::render` asserts on the drawn buffer content (`CompletedFrame::buffer`).
  - **Key handlers**: every `App::handle_*` method has direct unit tests (`tests::handlers`). Note that handlers do **not** set `view_mode` themselves — the `handle_key` dispatcher does — so tests must set `app.view_mode` before calling a handler that depends on `use_state()` (navigation, modal lists).
  - **CLI**: `Cli::parse` is pure and unit-tested; the `--version` print/exit lives in `main` (uncovered glue) and is verified end-to-end by `tests/cli.rs`, which spawns the real binary via `CARGO_BIN_EXE_basilk`.
- **Migrations**: If you change the data schema (e.g., in `Project` or `Task` structs), you **must** add a new migration in `migration.rs` and update `JSON_VERSIONS`.

## Configuration
There is currently no config file mechanism in the codebase (an earlier `src/config.rs` with `ui.show_help` no longer exists). All behavior is compiled in; per-task settings like `estimated_hours` live in the JSON data.
