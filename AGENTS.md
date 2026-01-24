# Basilk Agent Guide

Basilk is a TUI-based kanban task manager written in Rust using `ratatui`.

## Essential Commands

- **Build**: `cargo build`
- **Run**: `cargo run`
- **Test**: `cargo test` (Note: Currently the project has no automated tests in `src/`)
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
- `src/ui.rs`: UI utility functions for creating modals and layouts.
- `src/view.rs`: Higher-level UI rendering logic (rendering specific views/modals).
- `src/util.rs`: Miscellaneous utility functions.

## Code Patterns

### App State Management
The `App` struct in `main.rs` manages the application state, including selected indices for projects and tasks, the current `ViewMode`, and loaded data.

### View Modes
`ViewMode` enum in `main.rs` defines the different screens and states (e.g., `ViewProjects`, `AddTask`, `ViewTasks`).

### Data Persistence
- Data is stored in JSON files named after a version hash (e.g., `911fc.json`) in the user's config directory.
- `Json::read()` and `Json::write()` handle loading and saving the entire project list.
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
- **Data Loading**: `Project::reload` and `Task::reload` read the entire JSON file from disk. Changes are written back to disk immediately after most operations (create, rename, delete, change status/priority).
- **Sorting**: Tasks are sorted by status and priority during `Task::load_items`.
- **Migrations**: If you change the data schema (e.g., in `Project` or `Task` structs), you **must** add a new migration in `migration.rs` and update `JSON_VERSIONS`.

## Configuration
Stored in `config.toml` in the config directory. Currently only supports `ui.show_help`.
