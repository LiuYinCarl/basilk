<p align="center"><img src="./assets/basil-k.jpg" width=240></img></p>
<p align="center"><i>插画由 <a href="https://perchance.org/ai-pixel-art-generator">perchance.org</a> 生成</i></p>

<p align="center"><a href="./README.md">English</a> | 中文</p>

<h1 align="center">basilk</h1>
<p align="center">一个基于终端界面 (TUI) 的任务管理工具，具有简洁的看板逻辑</p>

<img src="./assets/basilk.gif"></img>

## 缘起
那是一个[炎热的八月夜晚](https://www.meteo.it/notizie/meteo-caldo-in-aumento-la-tendenza-verso-ferragosto-c95aa7dc)，我正在整理待办事项，突然觉得需要一个简单、便携的软件来帮助我。**basilk** 由此诞生——既是一个学习 Rust 的暑期项目，也能在任何地方使用。

名字 [_/ˈbæzəlkeɪ/_](https://gabalpha.github.io/read-audio/?p=https://github.com/GabAlpha/basilk/raw/master/assets/basil-k.wav) 源于罗勒（basil）——一种易于种植和维护的植物，而 "k" 代表看板（kanban）。

<details>
<summary>另一个故事</summary>

<p align="center"><img src="./assets/bas-silk.jpg" width=240></img></p>
<p align="center"><i>插画由 <a href="https://perchance.org/ai-pixel-art-generator">perchance.org</a> 生成</i></p>

名字 [_/ˈbæzsɪlk/_](https://gabalpha.github.io/read-audio/?p=https://github.com/GabAlpha/basilk/raw/master/assets/bas-silk.wav) 源自 basil 与 silk 的结合，象征其制作过程的精巧。
</details>

## 关于
**basilk** 以项目为单位组织任务，每个项目内的任务可设置不同的状态（Up Next / On Going / Done）。

数据以 `.json` 格式存储，文件位于：
```
Linux
~/.config/basilk

macOS
~/Library/Application Support/basilk

Windows
<USER>\AppData\Roaming\basilk
```
选择 JSON 格式是为了方便导出。

本项目是原始 [basilk](https://github.com/GabAlpha/basilk) 的一个 fork 版本。如需原始版本，请参考上游仓库。

## 安装

```sh
git clone https://github.com/LiuYinCarl/basilk && cd basilk
cargo install --path .
```

## 使用
运行

```sh
basilk
```

## 快捷键

在应用内按 `h` 即可查看快捷键列表。

### 全局
| 按键 | 功能 |
|---|---|
| `q` | 退出 |

### 项目列表视图
| 按键 | 功能 |
|---|---|
| `↑` `↓`  `k` `j`  `Tab` `Shift+Tab` | 浏览项目 |
| `Enter`  `→`  `l` | 进入项目（查看任务） |
| `m` | 打开便签 |
| `n` | 新建项目 |
| `r` | 重命名所选项目 |
| `d` | 删除所选项目 |
| `c` | 番茄钟（倒计时） |
| `h` | 帮助 |

### 任务列表视图
| 按键 | 功能 |
|---|---|
| `↑` `↓`  `k` `j`  `Tab` `Shift+Tab` | 浏览任务 |
| `←` `→` | 切换泳道（看板视图） |
| `b` | 切换看板 / 列表视图 |
| `Esc`  `←` | 返回项目列表 |
| `Enter` | 更改任务状态 |
| `p` | 更改任务优先级 |
| `n` | 新建任务 |
| `r` | 重命名所选任务 |
| `v` | 查看任务详情 |
| `e` | 编辑任务备注 |
| `d` | 删除所选任务 |
| `t` | 切换显示/隐藏已完成任务 |
| `s` | 所选任务的正向计时器 |
| `c` | 番茄钟（倒计时） |
| `h` | 帮助 |

### 更改状态 / 优先级弹窗
| 按键 | 功能 |
|---|---|
| `↑` `↓`  `k` `j`  `Tab` `Shift+Tab` | 浏览选项 |
| `Enter` | 确认选择 |
| `Esc` | 取消 |

### 输入弹窗（新建 / 重命名 / 编辑备注）
| 按键 | 功能 |
|---|---|
| `Enter` | 确认 |
| `Esc` | 取消 |

### 删除确认弹窗
| 按键 | 功能 |
|---|---|
| `y` | 确认删除 |
| `n` | 取消 |

### 任务详情视图
| 按键 | 功能 |
|---|---|
| `e` | 编辑任务备注 |
| `g` | 编辑预期时长（小时，`0` = 不设预期） |
| 任意其他按键 | 关闭详情 |

### 计时器视图
| 按键 | 功能 |
|---|---|
| `Space` | 暂停 / 继续 |
| `Enter` | 停止（正向计时会把时长累计到绑定的任务） |
| `Esc` | 关闭（计时器在后台继续运行） |

### 便签列表视图
| 按键 | 功能 |
|---|---|
| `↑` `↓`  `k` `j`  `Tab` `Shift+Tab` | 浏览便签 |
| `Enter`  `→`  `l`  `v` | 打开便签预览 |
| `n` | 新建便签 |
| `r` | 重命名所选便签 |
| `d` | 删除所选便签 |
| `Esc`  `←` | 返回项目列表 |
| `h` | 帮助 |

### 便签预览视图
| 按键 | 功能 |
|---|---|
| `↑` `↓`  `k` `j` | 滚动 |
| `PageUp` `PageDown` | 翻页 |
| `g`  `G` | 顶部 / 底部 |
| `e` | 编辑（Markdown 源码） |
| `Esc`  `Enter` | 返回便签列表 |
| `h` | 帮助 |

### 便签编辑视图
| 按键 | 功能 |
|---|---|
| `Esc` | 保存并返回预览 |
| 其他按键 | 多行编辑（由编辑器处理） |

计时器在你切换视图（包括返回项目列表）时持续运行，只有在计时器视图按 `Enter` 停止、或退出程序时才会结算。

- **正向计时**（`s`，任务列表）：绑定所选任务，计时累计到任务的**累计时长**，在详情视图中与任务的**预期时长**（你预计这个任务要做多久，可按 `g` 为每个任务单独设置）并列展示，并显示已消耗预期时长的百分比。
- **番茄钟**（`c`，两个视图均可）：全局倒计时，用于专注时段；归零时响铃提醒，界面停留在结束状态（显示“时间到！”），按任意键关闭。不关联任何任务，也不累计时长。

任务以 `[状态] 标题` 格式显示，可选 `[优先级]` 前缀；设置了预期时长的任务还会带上 `[x%]` 后缀，显示已消耗预期时长的百分比（达到 100% 后变红）。
- 状态：**UpNext**（品红）、**OnGoing**（黄色）、**Done**（绿色，~~删除线~~）
- 优先级：`!`（最高）、`!!`（高）、`!!!`（低）

已完成的任务在任务列表视图中**默认隐藏**，按 `t` 可切换显示。

在任务列表视图按 `b` 可切换到看板**泳道视图**：三条竖排泳道（**Up Next** / **On Going** / **Done**），标题带任务计数，当前聚焦的泳道以其状态颜色高亮边框。用 `←`/`→` 在泳道间切换，`↑`/`↓` 在泳道内选择任务；其余快捷键（`v` 详情、`Enter` 状态、`p` 优先级、计时器等）照常工作，更改任务状态后任务会移动到对应泳道。看板始终显示 Done 泳道，不受 `t` 设置影响（看板模式下 `t` 无操作）。

在项目列表视图按 `m` 可打开**便签**：全局的、不依附于项目的备忘录。便签由标题和 Markdown 正文组成，打开后进入整页渲染预览（标题、粗体/斜体、代码块、列表、引用、链接等），按 `e` 切换到整页多行编辑器修改 Markdown 源码，`Esc` 保存并返回预览。

## 参与贡献
> [!NOTE]
> 本项目目前处于 beta 阶段，可能存在 bug。

如上所述，这是我的第一个 Rust 项目，欢迎任何形式的贡献和帮助！如果你有任何建议、改进或 bug 修复，欢迎提交 pull request 或提出新的 issue。

## 许可证

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=flat&logo=GitHub&labelColor=1D272B&color=819188&logoColor=white)](./LICENSE-MIT)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg?style=flat&logo=GitHub&labelColor=1D272B&color=819188&logoColor=white)](./LICENSE-APACHE)

根据您的选择，许可协议为 [Apache License Version 2.0](./LICENSE-APACHE) 或 [The MIT License](./LICENSE-MIT)。
