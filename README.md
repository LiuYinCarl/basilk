<p align="center"><img src="./assets/basil-k.jpg" width=240></img></p>
<p align="center"><i>illustration generated using <a href="https://perchance.org/ai-pixel-art-generator">perchance.org</a></i></p>

<p align="center">English | <a href="./README.zh-CN.md">中文</a></p>

<h1 align="center">basilk</h1>
<p align="center">A Terminal User Interface (TUI) to manage your tasks with minimal kanban logic</p>

<img src="./assets/basilk.gif"></img>

## History
It was a [very hot August night](https://www.meteo.it/notizie/meteo-caldo-in-aumento-la-tendenza-verso-ferragosto-c95aa7dc), and I was organizing my activities when at a certain point I felt the need for a software that could help me with this, something simple and portable. **basilk** is created as a summer project to learn Rust and to be able to use the software anywhere. 

The name [_/ˈbæzəlkeɪ/_](https://gabalpha.github.io/read-audio/?p=https://github.com/GabAlpha/basilk/raw/master/assets/basil-k.wav) comes from the basil plant, which is easy to grow and maintain, and "k" stands for kanban.

<details>
<summary>Another story</summary>

<p align="center"><img src="./assets/bas-silk.jpg" width=240></img></p>
<p align="center"><i>illustration generated using <a href="https://perchance.org/ai-pixel-art-generator">perchance.org</a></i></p>

The name [_/ˈbæzsɪlk/_](https://gabalpha.github.io/read-audio/?p=https://github.com/GabAlpha/basilk/raw/master/assets/bas-silk.wav) comes from the union of basil and silk as a symbol of elaborateness due to its production process.
</details>

## About
**basilk** is structured to create projects and within each project to create tasks with a specific status (Up Next/On Going/Done).

The data structure is saved in `.json` format and is available in the directory:
```
Linux
~/.config/basilk

macOS
~/Library/Application Support/basilk

Windows
<USER>\AppData\Roaming\basilk
```
The choice to use the JSON format is to make easier to export

This is a fork of the original [basilk](https://github.com/GabAlpha/basilk) project. For the original version, please refer to the upstream repository.

## Installation

```sh
git clone https://github.com/LiuYinCarl/basilk && cd basilk
cargo install --path .
```

Prebuilt binaries for Linux, macOS (Intel & Apple Silicon) and Windows are
attached to every [GitHub Release](https://github.com/LiuYinCarl/basilk/releases).

## Versioning & Releases

Basilk follows [Semantic Versioning](https://semver.org/) (`MAJOR.MINOR.PATCH`,
stored in `Cargo.toml`). The [Release workflow](.github/workflows/release.yml)
publishes a new version automatically:

- **Every push to `master` that changes code** (`src/`, `tests/`, `Cargo.toml`,
  `Cargo.lock`) bumps the **patch** version, tags it `vX.Y.Z` and publishes
  release binaries for Linux, macOS and Windows.
- Prefix a commit subject with **`[major]`** or **`[minor]`** to bump that
  component instead (patch is the default).
- The manual **Run workflow** button on the Release workflow also works, with
  an explicit `patch`/`minor`/`major` choice.
- For a local manual release: `./scripts/bump-version.sh [patch|minor|major] [--push]`.

The version in `Cargo.toml`/`Cargo.lock` is always the source of truth, and
the release binaries are built from the tagged commit.

## Usage
Run

```sh
basilk
```

## Keybindings

Press `h` to view the keybinding list in-app.

### Global
| Key | Action |
|---|---|
| `q` | Quit |

### Project List View
| Key | Action |
|---|---|
| `↑` `↓`  `k` `j`  `Tab` `Shift+Tab` | Navigate projects |
| `Enter`  `→`  `l` | Enter project (view tasks) |
| `m` | Open notes |
| `n` | New project |
| `r` | Rename selected project |
| `d` | Delete selected project |
| `c` | Pomodoro (countdown) timer |
| `h` | Help |

### Task List View
| Key | Action |
|---|---|
| `↑` `↓`  `k` `j`  `Tab` `Shift+Tab` | Navigate tasks |
| `←` `→` | Switch lane (board view) |
| `b` | Toggle board / list view |
| `Esc`  `←` | Back to project list |
| `Enter` | Change task status |
| `p` | Change task priority |
| `n` | New task |
| `r` | Rename selected task |
| `v` | View task details |
| `e` | Edit task note |
| `d` | Delete selected task |
| `t` | Toggle show/hide completed tasks |
| `s` | Stopwatch timer for selected task |
| `c` | Pomodoro (countdown) timer |
| `h` | Help |

### Change Status / Priority Modals
| Key | Action |
|---|---|
| `↑` `↓`  `k` `j`  `Tab` `Shift+Tab` | Navigate options |
| `Enter` | Confirm selection |
| `Esc` | Cancel |

### Input Modals (New / Rename / Edit Note)
| Key | Action |
|---|---|
| `Enter` | Confirm |
| `Esc` | Cancel |

### Delete Confirmation Modals
| Key | Action |
|---|---|
| `y` | Confirm delete |
| `n` | Cancel |

### Task Details View
| Key | Action |
|---|---|
| `e` | Edit task note |
| `g` | Edit estimated time (hours, `0` = no estimate) |
| Any other key | Close details |

### Timer View
| Key | Action |
|---|---|
| `Space` | Pause / resume |
| `Enter` | Stop (a stopwatch saves its elapsed time to the bound task) |
| `Esc` | Close (timer keeps running in the background) |

### Notes List View
| Key | Action |
|---|---|
| `↑` `↓`  `k` `j`  `Tab` `Shift+Tab` | Navigate notes |
| `Enter`  `→`  `l`  `v` | Open the note preview |
| `n` | New note |
| `r` | Rename selected note |
| `d` | Delete selected note |
| `Esc`  `←` | Back to project list |
| `h` | Help |

### Note Preview View
| Key | Action |
|---|---|
| `↑` `↓`  `k` `j` | Scroll |
| `PageUp` `PageDown` | Page up / down |
| `g`  `G` | Top / bottom |
| `e` | Edit (Markdown source) |
| `Esc`  `Enter` | Back to notes list |
| `h` | Help |

### Note Editor View
| Key | Action |
|---|---|
| `Esc` | Save and return to the preview |
| Any other key | Editing (multi-line, handled by the editor) |

The timer keeps running while you navigate (even back to the project list); it stops when you press `Enter` in the timer view or when you quit.

- **Stopwatch** (`s`, task list): bound to the selected task; the elapsed time accumulates into the task's **Time Spent**, shown in the details view next to the task's **Estimate** (how long you expect the task to take, editable per task with `g`; the details view shows what percentage of the estimate has been spent).
- **Pomodoro** (`c`, both views): a global countdown for focus sessions; at zero it rings the terminal bell and stays on screen (showing `time's up!`) until you press any key. It is not tied to any task, so nothing is accumulated.

Tasks are displayed as `[Status] Title` with optional `[Priority]` prefix and, for tasks with an estimate set, a `[x%]` suffix showing how much of the estimate has been spent (red once it reaches 100%).
- Statuses: **UpNext** (magenta), **OnGoing** (yellow), **Done** (green, ~~crossed out~~)
- Priorities: `!` (highest), `!!` (high), `!!!` (low)

Completed tasks are **hidden by default** in the task list view. Press `t` to toggle their visibility.

Press `b` in the task list view to switch to a kanban **board view**: three vertical lanes (**Up Next** / **On Going** / **Done**), each titled with its task count, the focused lane highlighted in its status color. Use `←`/`→` to move between lanes and `↑`/`↓` to select a task within a lane; every other shortcut (`v` details, `Enter` status, `p` priority, timers, …) works the same, and changing a task's status moves it to the matching lane. The board always shows the Done lane, regardless of the `t` setting (which is a no-op while the board is active).

Press `m` in the project list view to open **notes**: global, project-independent memos. A note is a titled entry whose body is Markdown; opening one shows a full-page rendered preview (headings, bold/italic, code blocks, lists, quotes, links), and `e` switches to a full-page multi-line editor for the Markdown source (`Esc` saves and returns to the preview).

## Contributing
> [!NOTE]  
> This project is now in beta version and is expected to have bugs

As I mentioned above, this is my first project in Rust, so contributions and help are welcome! If you have any suggestions, improvements, or bug fixes, feel free to submit a pull request or open a new issue.

### Testing & coverage

The test suite runs with `cargo test` and includes property tests (`src/property_tests.rs`). Coverage is measured with [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) (`cargo llvm-cov --workspace`) and enforced in CI (`--fail-under-lines 95`); the suite currently sits around **99.5% line coverage** — the only uncovered lines are the real-terminal glue in `main.rs` (`main`, `init_terminal`, `restore_terminal`, `CrosstermSource::next_key`), which cannot run inside a unit test. The full event loop, every view mode, and every key handler are exercised in-process via synthetic events and a `TestBackend`.

## License

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=flat&logo=GitHub&labelColor=1D272B&color=819188&logoColor=white)](./LICENSE-MIT)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg?style=flat&logo=GitHub&labelColor=1D272B&color=819188&logoColor=white)](./LICENSE-APACHE)

Licensed under either of [Apache License Version 2.0](./LICENSE-APACHE) or [The MIT License](./LICENSE-MIT) at your option.
