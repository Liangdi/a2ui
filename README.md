# A2UI — A2UI 协议的 Rust 实现(ratatui 终端 + Slint 桌面)

[![crates.io](https://img.shields.io/crates/v/a2ui.svg)](https://crates.io/crates/a2ui)
[![docs.rs](https://docs.rs/a2ui/badge.svg)](https://docs.rs/a2ui)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

[English](README_EN.md) | 中文

一个 Rust 实现的 [A2UI (Agent to UI) v1.0](https://github.com/a2ui-project/a2ui) 协议终端渲染器，基于 [ratatui](https://ratatui.rs/) 构建。

A2UI 是一个 JSON 流式 UI 协议，允许 AI Agent 动态生成和更新终端用户界面。

项目组织为 Cargo workspace:`a2ui-base`(框架无关核心)+ `a2ui-tui`(ratatui backend)+ `a2ui-gallery`(展示 app)+ `a2ui`(umbrella，re-export 保持 `use a2ui::core::...` / `use a2ui::tui::...` 不破)。此外还有一个**可选的**第二后端 `a2ui-slint`,它把 A2UI 组件树渲染到原生桌面窗口(基于 [Slint](https://slint.dev/),pinned 1.16),详见下方[「Slint 桌面后端」](#slint-桌面后端)。

## 特性

- ✅ 完整的 A2UI v1.0 协议支持
- ✅ **18 个 TUI 组件**：Text, Row, Column, Button, TextField, Card, Divider, List, CheckBox, Icon, Tabs, Modal, Slider, ChoicePicker, DateTimeInput, Image, Video, AudioPlayer
  - 交互式：Text、Row、Column、Button、TextField、Slider、CheckBox、ChoicePicker、DateTimeInput（箭头键调节值）
  - 占位符（默认）：Image / Video / AudioPlayer 仅渲染文本占位符（`[🖼 description]`、`[▶ url]`、`[♫ url]`），终端无法解码像素/音频/视频。可通过下方「可选特性」开启真实图片/音频渲染。
- ✅ **能力协商（Capabilities negotiation）**：`ClientCapabilities` / `ServerCapabilities` 类型 + 构建器（从已注册 catalog 自动派生 `supportedCatalogIds`）。
- ✅ **内联目录（Inline catalogs）**：服务端可声明 `acceptsInlineCatalogs`；客户端解析并校验内联 catalog JSON（UAX#31 标识符检查），运行时注册 schema-only 函数。
- ✅ **自定义组件兜底渲染**：未知 / 内联自定义组件类型以可见的标签框（类型 + 属性 + 子节点）呈现，而非裸「unknown」错误。
- ✅ **14 个客户端函数**：required, regex, length, numeric, email, and/or/not, formatString, formatNumber, formatCurrency, formatDate, pluralize, openUrl
- ✅ **模块化 Cargo workspace 架构**（`a2ui-base` 框架无关 / `a2ui-tui` ratatui backend / `a2ui-gallery` 展示 app / `a2ui` umbrella）
- ✅ JSON Pointer 数据绑定与响应式状态管理
- ✅ Gallery App 示例浏览器（支持消息逐步渲染）
- ✅ **182 个单元/集成测试**（core 91 + tui 61 + gallery e2e 21 + slint 9），包含 A2UI 规范样例的端到端测试

## 截图

**Gallery 示例浏览器**

![Gallery](screenshot/gallery.png)

**登录表单**

![Login Form](screenshot/login-form.png)

**Agent Chat**（AI 对话界面，`08_agent_chat` 示例：多 surface 聊天布局、流式 A2UI 消息、Card / Column / Row / Divider 等富组件）

![Agent Chat](screenshot/agent-chat.png)

**邀请函构建器**（规范样例 `30_live-invitation-builder`：响应式表单布局，TextField / Slider / ChoicePicker / DateTimeInput 等交互组件协同，实时预览邀请内容）

![Invitation Builder](screenshot/invitation-builder.png)

**Sci-fi HUD**（赛博朋克战术 HUD，`17_scifi_hud` 示例：自定义 `TuiComponent` 面板组合成遥测 / 雷达 / 事件日志，仪表、扫描、事件等所有实时数据均通过 a2ui `updateDataModel` 协议推送驱动）

![Sci-fi HUD](screenshot/sci-fi-hud.png)

## 快速开始

```bash
# 运行 Gallery App
cargo run -p a2ui-gallery

# 安装 Gallery App（提供 `a2ui_gallery` 二进制）
cargo install a2ui-gallery

# 运行示例（位于 umbrella crate）
cargo run -p a2ui --example 12_handshake
```

### 操作说明

| 按键 | 功能 |
|------|------|
| `↑`/`k`, `↓`/`j` | 导航样例列表 |
| `Enter` | 选择当前样例并渲染 |
| `n` | 逐步处理下一条消息 |
| `a` | 处理所有剩余消息 |
| `r` | 重置并重新播放 |
| `Tab` | 切换焦点 |
| `Esc` | 返回列表 / 退出 |

## 架构

```
┌───────────────────────────────────────────────────────────────────────┐
│  apps:  a2ui-gallery (TUI)   a2ui-slint-gallery (桌面)   a2ui-egui-gallery (桌面)│
├───────────────────────────────────────────────────────────────────────┤
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui])          │
├───────────────────────────────────────────────────────────────────────┤
│  backends:   a2ui-tui (ratatui)   a2ui-slint (Slint, 可选)   a2ui-egui (egui, 可选)│
├───────────────────────────────────────────────────────────────────────┤
│  a2ui-base (框架无关:Protocol / Model / Catalog / Processor)          │
└───────────────────────────────────────────────────────────────────────┘
```

依赖自下而上:`a2ui-base` 同时支撑三个后端 —— `a2ui-tui`(ratatui,默认)、`a2ui-slint`(Slint 桌面,可选)与 `a2ui-egui`(egui 桌面,可选)。`a2ui-tui` ← `a2ui-gallery`;`a2ui-slint` ← `a2ui-slint-gallery`;`a2ui-egui` ← `a2ui-egui-gallery`;`a2ui`(umbrella)依赖 core + tui(slint / egui 分别在 `slint` / `egui` feature 后)。`a2ui-base` 完全不依赖 ratatui/slint/egui,可独立用于其他 backend。

### 项目结构

```
crates/
├── core/              # a2ui-base：框架无关层
│   └── src/
│       ├── protocol/ model/ catalog/ observable/
│       ├── message_processor.rs   # 消息解析 → 状态变更
│       ├── capabilities.rs        # 能力协商 + 内联 catalog 解析
│       └── error.rs event.rs
├── tui/               # a2ui-tui：ratatui 渲染层
│   └── src/
│       ├── surface.rs             # 递归渲染入口
│       ├── component_impl.rs      # TuiComponent trait + 注册
│       ├── layout_engine.rs       # 权重分割 / 对齐
│       ├── focus_manager.rs       # 键盘焦点管理
│       ├── components/            # 18 个组件实现
│       └── catalogs/              # Minimal + Basic Catalog 组装
├── gallery/           # a2ui-gallery：Gallery App (bin + lib)
│   ├── src/                       # app.rs / sample_loader.rs / main.rs
│   ├── tests/e2e.rs               # 端到端测试（加载 spec 样例）
│   └── a2ui/specification/        # 编译期嵌入的 spec 树
├── slint/             # a2ui-slint：Slint 桌面后端(可选,非默认成员)
│   ├── build.rs                  # 代码生成有界深度 Node0..N7(规避 Slint 递归限制)
│   └── src/                      # live_tree(扁平节点数组) / host / ui
├── slint-gallery/     # a2ui-slint-gallery：桌面 Gallery App (bin,左列表 + 右预览)
│   └── src/main.rs
├── egui/              # a2ui-egui：egui 即时模式桌面后端(可选,非默认成员)
│   └── src/                      # walker(递归渲染) / app / edit_state / interaction
├── egui-gallery/      # a2ui-egui-gallery：桌面 Gallery App (bin,左列表 + 右预览)
│   └── src/main.rs
└── a2ui/              # a2ui：umbrella，re-export core+tui [+slint] [+egui]
    ├── src/lib.rs
    └── examples/                  # 17 个示例
```

## Slint 桌面后端

除 ratatui 终端后端外,项目还提供 **`a2ui-slint`**:它将 A2UI 组件树渲染到**原生桌面窗口**(基于 [Slint](https://slint.dev/),固定 1.16 版本)。框架无关的交互逻辑(焦点遍历、事件分发、`EventResult` 应用)共享在 `a2ui-base` 中,因此两个后端在键盘 / 按钮交互上表现一致。

**它是可选且较重的依赖**:`a2ui-slint` 是 workspace 的**非默认成员**(会拉取 Slint 工具链 + GUI 系统库)。普通的 `cargo build` 只编译 ratatui 栈。需要显式构建 Slint 后端:

```bash
cargo build -p a2ui-slint --features backend
```

umbrella crate 也在 `slint` cargo feature 之后将后端 re-export 为 `a2ui::slint`。

### 运行 Gallery(桌面版)

`a2ui-slint-gallery` 加载与 ratatui gallery 相同的内嵌 A2UI 样例,在窗口中展示。启动时会在终端打印完整的带编号样例列表:

```bash
cargo run -p a2ui-slint-gallery             # 第一个样例
cargo run -p a2ui-slint-gallery -- 3        # 按 1 起始的序号
cargo run -p a2ui-slint-gallery -- login    # 按名称子串(大小写不敏感)
```

渲染器使用 `renderer-software` + `backend-winit`,**无需 GPU / OpenGL 驱动**即可运行。

### 组件覆盖

全部 18 个 A2UI 组件类型均可渲染:

- **富渲染**:Text / Button / Column / Row / Card / TextField / CheckBox / Slider(Button 与 CheckBox 的点击通过共享的 `core::components::dispatch_event` 分发)
- **尽力渲染**:Divider / Icon / Tabs / Modal / List / ChoicePicker / DateTimeInput
- **占位符**:Image / Video / AudioPlayer 渲染为带标签的占位符(二进制媒体不会带入 Slint 树)

### 实现要点:为什么需要展平组件树

Slint **无法表达递归**(既不支持递归 struct,也不支持自引用组件 —— 见 [slint-ui/slint#4218](https://github.com/slint-ui/slint/issues/4218))。因此 `live_tree` 不是嵌套树,而是把组件树展平为一个 `Vec<LiveNode>`,通过基于索引的 `children` 引用;`build.rs` 代码生成了一个**有界深度**的组件链 `Node0`(叶子)→ … → `Node7`(根)。A2UI 树通常很浅,深度 7 足以覆盖实际 UI;更深的子树会被截断为 `…`。这是未来贡献者最需要了解的关键约束。

### 当前限制

- 超过 7 层的树会被截断;
- TextField 能显示其值,但尚未接入原生可编辑输入控件;
- Tabs / ChoicePicker / DateTimeInput 可渲染,但它们的键盘处理未进入共享 core 的 dispatch(Slint 侧除 Button / CheckBox 外的交互尚未接通)。

## egui 桌面后端

除 ratatui 与 Slint 外,项目还提供 **`a2ui-egui`**:它把 A2UI 组件树渲染到原生桌面窗口,基于 [egui](https://github.com/emilk/egui)(即时模式 GUI,固定 0.33 版本)。与 Slint 后端不同,egui **原生支持递归**,因此不需要展平组件树或 `build.rs` 有界深度代码生成;`walker::render_node` 直接在 `&mut egui::Ui` 上递归渲染。egui 还提供**真正的可交互原生控件**(TextField / Slider / CheckBox / ComboBox),而 Slint 后端把它们渲染为只读占位符。Button 的点击复用共享的 `core::components::dispatch_event` + `apply_event_result`,与其它两个后端一致;Modal 用原生的 `egui::Window` 浮层呈现。

**它是可选依赖**:`a2ui-egui` 是 workspace 的**非默认成员**(会拉取 winit + glow)。普通的 `cargo build` 只编译 ratatui 栈。需要显式构建:

```bash
cargo build -p a2ui-egui --features backend
```

umbrella crate 也在 `egui` cargo feature 之后将后端 re-export 为 `a2ui::egui`。

### 即时模式状态桥(实现要点)

egui 控件每帧都需要一个稳定的 `&mut` 缓冲区(保留光标 / 滚动位置、检测值变化),但 A2UI 的值存在于 **data model**(每帧经由 `DataContext` 重新解析)。`EditBuffers` 这个持久化、按组件 id 索引的 map 桥接二者:每帧**用 data model 值播种**(若过期)→ 把 `&mut` 交给 egui 控件 → **检测变化** → 收集为 `PendingInteraction`,在整棵树遍历结束后(drops 掉 data model 的借用)再统一回写。这与 TUI gallery 的「drop 借用再 mutate」、Slint host 的「回调再 redraw」是同构的。

### 运行 Gallery(egui 版)

`a2ui-egui-gallery` 加载相同的内嵌 A2UI 样例:

```bash
cargo run -p a2ui-egui-gallery             # 第一个样例
cargo run -p a2ui-egui-gallery -- 3        # 按 1 起始的序号
cargo run -p a2ui-egui-gallery -- login    # 按名称子串(大小写不敏感)
```

渲染器使用 glow(OpenGL),需要 GL 栈但无需专用 GPU 驱动。

## 协议概览

A2UI 使用 JSON 流式消息驱动 UI 渲染：

```jsonl
{"version":"v1.0","createSurface":{"surfaceId":"main","catalogId":"https://a2ui.org/.../catalog.json"}}
{"version":"v1.0","updateComponents":{"surfaceId":"main","components":[...]}}
{"version":"v1.0","updateDataModel":{"surfaceId":"main","path":"/user/name","value":"Alice"}}
{"version":"v1.0","deleteSurface":{"surfaceId":"main"}}
```

## 示例

| 示例 | 说明 | 运行 |
|------|------|------|
| `01_hello_world` | 最简单的 A2UI 程序 | `cargo run -p a2ui --example 01_hello_world` |
| `02_jsonl_stream` | JSONL 流式处理与逐步渲染 | `cargo run -p a2ui --example 02_jsonl_stream` |
| `03_data_binding` | JSON Pointer 响应式数据绑定 | `cargo run -p a2ui --example 03_data_binding` |
| `04_login_form` | 完整表单：输入、验证、焦点管理、Action | `cargo run -p a2ui --example 04_login_form` |
| `05_custom_function` | 自定义 Catalog 函数 | `cargo run -p a2ui --example 05_custom_function` |
| `06_call_function` | 服务端 `callFunction` 消息与 `functionResponse` | `cargo run -p a2ui --example 06_call_function` |
| `07_action_response` | `actionResponse` 与 `responsePath` 响应式更新 | `cargo run -p a2ui --example 07_action_response` |
| `12_handshake` | 能力协商握手（Capabilities negotiation） | `cargo run -p a2ui --example 12_handshake` |

## 可选特性 (Optional Features)

图片渲染**内置且默认开启**：默认 `cargo build` 即通过 `ratatui-image` 进行真实图片渲染（kitty / iTerm2 / Sixel / Halfblocks 自动降级），仅支持本地文件路径，无法加载时回退为占位符。以下为额外的**可选**特性，默认关闭：

> 桌面 GUI 后端见上方[「Slint 桌面后端」](#slint-桌面后端)章节(独立 workspace 成员,非 ratatui feature)。

| 特性 | 说明 | 启用 | 限制 |
|------|------|------|------|
| `audio` | 通过 `rodio` 进行真实音频播放（后台线程） | `--features audio` | **仅支持本地文件路径**；需安装 ALSA 系统开发库（Fedora: `alsa-lib-devel`，Debian: `libasound2-dev`）；失败时静默回退为占位符 |
| — (Video) | 视频无对应特性 | — | 终端尚无成熟的 TUI 视频方案，始终渲染占位符 |

## 作为库使用

`a2ui-base` 完全框架无关，可独立用于非 ratatui 场景，或作为其他 backend 的基础（项目已基于它实现了 [Slint 桌面后端](#slint-桌面后端)）：

```bash
# 方式一：直接依赖（最精简，推荐用于库）
cargo add a2ui-base a2ui-tui

# 方式二：通过 umbrella（保持 a2ui:: 路径）
cargo add a2ui
```

```rust
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::catalog::Catalog;
use a2ui_tui::catalogs::basic::{build_basic_catalog, build_basic_registry};
use a2ui_tui::surface::SurfaceRenderer;

// 创建处理器（带 Basic Catalog）
let catalog = build_basic_catalog();
let registry = build_basic_registry();
let mut processor = MessageProcessor::new(vec![catalog]);

// 解析并处理消息
let msg = MessageProcessor::parse_message(r#"{"version":"v1.0","createSurface":{...}}"#)?;
processor.process_message(msg)?;

// 渲染（在 ratatui Frame 中）
let surface = processor.model.get_surface("main").unwrap();
let renderer = SurfaceRenderer::new(surface, &registry, &catalog);
renderer.render(&mut frame, area);
```

> 通过 umbrella 时，把 `a2ui_base::` / `a2ui_tui::` 换成 `a2ui::core::` / `a2ui::tui::` 即可，其余不变。

## 许可证

MIT
