# A2UI — A2UI 协议的 Rust 实现(ratatui 终端 + Slint / egui / Bevy / Iced 桌面后端)

[![crates.io](https://img.shields.io/crates/v/a2ui.svg)](https://crates.io/crates/a2ui)
[![docs.rs](https://docs.rs/a2ui/badge.svg)](https://docs.rs/a2ui)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

[English](README_EN.md) | 中文

[A2UI (Agent to UI) v1.0](https://github.com/a2ui-project/a2ui) 协议的 Rust 实现 —— 一个 JSON 流式 UI 协议,允许 AI Agent 动态生成并更新界面。

同一套框架无关的核心(`a2ui-base`)之上,提供了 **6 个渲染后端**:默认的终端后端 `a2ui-tui`(基于 [ratatui](https://ratatui.rs/)),以及五个**可选的**原生桌面后端 —— [Slint](https://slint.dev/)、[egui](https://github.com/emilk/egui)、[Bevy](https://bevyengine.org)、[Iced](https://github.com/iced-rs/iced)、[Dioxus](https://github.com/DioxusLabs/dioxus)。各后端的渲染保真度与「真输入」能力见[「后端支持矩阵」](#后端支持矩阵)。

项目组织为 Cargo workspace:`a2ui-base`(框架无关核心)+ 6 个后端(`a2ui-tui` / `a2ui-slint` / `a2ui-egui` / `a2ui-bevy` / `a2ui-iced` / `a2ui-dioxus`)+ 各自的 `*-gallery` 展示 app + `a2ui`(umbrella,re-export 保持 `use a2ui::core::...` / `use a2ui::tui::...` 等路径不破)。

## 特性

- ✅ 完整的 A2UI v1.0 协议支持
- ✅ **18 个 TUI 组件**：Text, Row, Column, Button, TextField, Card, Divider, List, CheckBox, Icon, Tabs, Modal, Slider, ChoicePicker, DateTimeInput, Image, Video, AudioPlayer
  - 交互式：Text、Row、Column、Button、TextField、Slider、CheckBox、ChoicePicker、DateTimeInput（箭头键调节值）
  - 占位符（默认）：Image / Video / AudioPlayer 仅渲染文本占位符（`[🖼 description]`、`[▶ url]`、`[♫ url]`），终端无法解码像素/音频/视频。可通过下方「可选特性」开启真实图片/音频渲染。
- ✅ **能力协商（Capabilities negotiation）**：`ClientCapabilities` / `ServerCapabilities` 类型 + 构建器（从已注册 catalog 自动派生 `supportedCatalogIds`）。
- ✅ **内联目录（Inline catalogs）**：服务端可声明 `acceptsInlineCatalogs`；客户端解析并校验内联 catalog JSON（UAX#31 标识符检查），运行时注册 schema-only 函数。
- ✅ **自定义组件兜底渲染**：未知 / 内联自定义组件类型以可见的标签框（类型 + 属性 + 子节点）呈现，而非裸「unknown」错误。
- ✅ **14 个客户端函数**：required, regex, length, numeric, email, and/or/not, formatString, formatNumber, formatCurrency, formatDate, pluralize, openUrl
- ✅ **载荷校验（Payload validation）**：从 Python SDK 移植的完整性 / 拓扑 / 递归与路径校验,外加容错的 `parse_and_fix`(自动修复智能引号、尾逗号等畸形 JSON)。可选挂载到 `MessageProcessor`(`with_validation(cfg)` + `drain_validation()`),默认关闭且不阻断组件加载 —— 面向不可信或 LLM 生成的载荷。
- ✅ **模块化 Cargo workspace 架构**（`a2ui-base` 框架无关 / `a2ui-tui` ratatui backend / `a2ui-gallery` 展示 app / `a2ui` umbrella）
- ✅ JSON Pointer 数据绑定与响应式状态管理
- ✅ Gallery App 示例浏览器（支持消息逐步渲染）
- ✅ **219 个单元/集成测试**（core 127 + tui 61 + gallery e2e 21 + slint 10），包含 A2UI 规范样例的端到端测试

## 截图

**Gallery 示例浏览器**

![Gallery](screenshot/gallery.png)

**登录表单**

![Login Form](screenshot/login-form.png)

**Agent Chat**（AI 对话界面，`08_agent_chat` 示例：多 surface 聊天布局、流式 A2UI 消息、Card / Column / Row / Divider 等富组件）

![Agent Chat](screenshot/agent-chat.png)

**邀请函构建器**（规范样例 `30_live-invitation-builder`：响应式表单布局，TextField / Slider / ChoicePicker / DateTimeInput 等交互组件协同，实时预览邀请内容）

![Invitation Builder](screenshot/invitation-builder.png)

**Sci-fi HUD — 后端对比**（同一份数据、同一套 `updateDataModel` 协议，换不同渲染器；仪表 / 雷达扫描 / 事件日志所有实时值均从 a2ui data model 读出）

| ratatui 终端（`a2ui` 的 `17_scifi_hud`） | Iced 桌面（`a2ui-iced` 的 `17_scifi_hud`） | Dioxus 桌面（`a2ui-dioxus` 的 `17_scifi_hud`） |
|:---:|:---:|:---:|
| ![Sci-fi HUD — ratatui](screenshot/sci-fi-hud-tui.png) | ![Sci-fi HUD — Iced](screenshot/sci-fi-hud-iced.png) | ![Sci-fi HUD — Dioxus](screenshot/sci-fi-hud-dioxus.png) |

ratatui 版用自定义 `TuiComponent` 画 ASCII 仪表 + 字符网格雷达；Iced 版用 `progress_bar` 仪表 + `Canvas` 雷达；Dioxus 版用 CSS 进度条 + **SVG** 雷达扫描,渲染到系统 WebView。三者架构一致——仅**数据**经协议流动,渲染层各自为政。

> 目前 sci-fi HUD 在 **ratatui (TUI)**、**Iced**、**Dioxus** 三个后端实现;Slint / egui / Bevy 的 gallery 渲染标准 spec 样例,暂无 HUD 变体。

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
│  apps:  a2ui-gallery (TUI)   a2ui-{slint,egui,bevy,iced,dioxus}-gallery (桌面)│
├───────────────────────────────────────────────────────────────────────┤
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui] [+ bevy] [+ iced] [+ dioxus]) │
├───────────────────────────────────────────────────────────────────────┤
│  backends:   a2ui-tui (ratatui)   a2ui-slint (Slint, 可选)   a2ui-egui (egui, 可选)   a2ui-bevy (Bevy, 可选)   a2ui-iced (Iced, 可选)   a2ui-dioxus (Dioxus, 可选)│
├───────────────────────────────────────────────────────────────────────┤
│  a2ui-base (框架无关:Protocol / Model / Catalog / Processor)          │
└───────────────────────────────────────────────────────────────────────┘
```

依赖自下而上:`a2ui-base` 同时支撑六个后端 —— `a2ui-tui`(ratatui,默认)、`a2ui-slint`(Slint 桌面,可选)、`a2ui-egui`(egui 桌面,可选)、`a2ui-bevy`(Bevy ECS UI 桌面,可选)、`a2ui-iced`(Iced 桌面,可选)与 `a2ui-dioxus`(Dioxus WebView 桌面,可选)。`a2ui-tui` ← `a2ui-gallery`;`a2ui-slint` ← `a2ui-slint-gallery`;`a2ui-egui` ← `a2ui-egui-gallery`;`a2ui-bevy` ← `a2ui-bevy-gallery`;`a2ui-iced` ← `a2ui-iced-gallery`;`a2ui-dioxus` ← `a2ui-dioxus-gallery`;`a2ui`(umbrella)依赖 core + tui(slint / egui / bevy / iced / dioxus 分别在同名 feature 后)。`a2ui-base` 完全不依赖 ratatui/slint/egui/bevy/iced/dioxus,可独立用于其他 backend。

## 后端支持矩阵

五个后端共享同一套 `a2ui-base` 核心(交互逻辑 / `dispatch_event` / `apply_event_result`),但渲染保真度与「真输入」能力因 GUI 框架而异:

> ✅ 完整渲染(可交互控件接受输入) · 🟡 尽力渲染(只读 / 有限交互) · ⬜ 占位符

| 组件 | TUI (ratatui) | Slint | egui | Bevy | Iced | Dioxus |
|------|:---:|:---:|:---:|:---:|:---:|:---:|
| Text / Row / Column / Card / List / Divider | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Button | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Modal | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| TextField | ✅ | 🟡 | ✅ | ✅ | ✅ | ✅ |
| CheckBox | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Slider | ✅ | 🟡 | ✅ | ✅ | ✅ | ✅ |
| ChoicePicker | ✅ | 🟡 | ✅ | ⬜ | ✅ | 🟡 |
| Tabs | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 |
| DateTimeInput | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 |
| Icon | ✅ | 🟡 | 🟡 | 🟡 | 🟡 | 🟡 |
| Image | ✅² | ⬜ | ⬜ | ⬜ | ⬜ | ⬜ |
| Video | ⬜ | ⬜ | ⬜ | ⬜ | ⬜ | ⬜ |
| AudioPlayer | ✅¹ | ⬜ | ⬜ | ⬜ | ⬜ | ⬜ |

¹ 需 `audio` 特性。
² TUI 经 `ratatui-image` 真实解码并显示图像像素(kitty / iTerm2 / Sixel / Halfblocks 自动降级,仅本地路径);五个桌面后端目前仅渲染文本占位符。

- **TUI 是参考实现**:18 组件全部完整渲染;图片默认开启(`ratatui-image`),音频需 `audio` 特性,视频始终为占位符。
- **可交互输入控件(TextField / Slider / CheckBox / ChoicePicker)的「真输入」**:egui、Bevy、Iced、Dioxus 与 TUI 完整支持(ChoicePicker 在 Dioxus 暂为占位徽章);**Slint 仅 Button / CheckBox 点击可交互**,TextField / Slider 渲染为只读值。
- **Bevy 的 ChoicePicker** 当前为文本标签(`[ChoicePicker: …]`),尚未接入原生选择控件。
- **Iced 是映射最干净的后端**(无状态桥 / 无 diff),五个可交互控件全部原生;**Dioxus 架构最独特**(响应式 signals + WebView/CSS 渲染)。

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

## Iced 桌面后端

除上述后端外,项目还提供 **`a2ui-iced`**:它把 A2UI 组件树渲染到原生桌面窗口,基于 [Iced](https://github.com/iced-rs/iced)(Elm 架构,固定 0.14 版本)。**这是五个后端里最干净的映射** —— Iced 是 Elm:`view(&state)` 返回一棵不可变的 `Element` 树,`update(&mut state, msg)` 改状态。所以可交互控件在 `view` 里直接读 data model,在 `update` 里通过 `Message` 回写:既不需要 egui 的 `EditBuffers` 状态桥,也不需要 bevy 的 reconciler。**无状态桥,无 diff**。Button 的点击同样复用共享的 `core::components::dispatch_event` + `apply_event_result`;Modal 用 `Stack` 叠加的居中浮层呈现。

**它是可选依赖**:`a2ui-iced` 是 workspace 的**非默认成员**(会拉取 wgpu + winit)。普通的 `cargo build` 只编译 ratatui 栈。需要显式构建:

```bash
cargo build -p a2ui-iced --features backend
```

umbrella crate 也在 `iced` cargo feature 之后将后端 re-export 为 `a2ui::iced`。渲染器默认使用 wgpu(GPU),并提供 tiny-skia 软件渲染兜底。

### 运行 Gallery(iced 版)

`a2ui-iced-gallery` 加载相同的内嵌 A2UI 样例:

```bash
cargo run -p a2ui-iced-gallery             # 第一个样例
cargo run -p a2ui-iced-gallery -- 3        # 按 1 起始的序号
cargo run -p a2ui-iced-gallery -- login    # 按名称子串(大小写不敏感)
```

## Dioxus 桌面后端

除上述后端外,项目还提供 **`a2ui-dioxus`**:它把 A2UI 组件树渲染到原生桌面 **WebView** 窗口,基于 [Dioxus](https://github.com/DioxusLabs/dioxus)(响应式 signals 架构,固定 0.7 版本)。**这是六个后端里架构最独特的一个**:

- **响应式 signals** —— Dioxus 像 React:运行时状态放在根的 `Signal` 里,UI 是对它的纯读取。既不需要 Iced 的 `Message` 枚举(Elm view/update),也不需要 egui 的 `EditBuffers` 状态桥。**无消息枚举,无状态桥** —— 信号本身就是交互通道,任何写入自动重渲染订阅了它的组件。
- **递归组件** —— 整棵树是**一个** `A2uiNode` 组件逐节点渲染自身(Dioxus 原生支持递归组件,不像 Slint 要 bounded-depth codegen)。
- **WebView 渲染** —— 渲染到系统 WebView(Linux 用 WebKitGTK),所以深色主题是一份 **CSS 样式表**(`theme::STYLESHEET`),而非逐控件 style 函数;A2UI 组件映射到 HTML 元素 + class。

Button 的点击同样复用共享的 `core::components::dispatch_event` + `apply_event_result`(经 `Rc<dyn Fn(String)>` 回调上交到根);Modal 用居中浮层 + 半透明遮罩呈现。

**它是可选依赖**:`a2ui-dioxus` 是 workspace 的**非默认成员**(会拉取 wry WebView + tao 窗口栈)。普通的 `cargo build` 只编译 ratatui 栈。需要显式构建:

```bash
cargo build -p a2ui-dioxus --features backend
```

umbrella crate 也在 `dioxus` cargo feature 之后将后端 re-export 为 `a2ui::dioxus`。Linux 上需系统安装 **WebKitGTK(`webkit2gtk-4.1`)+ GTK 3**。

### 运行 Gallery(dioxus 版)

`a2ui-dioxus-gallery` 加载相同的内嵌 A2UI 样例:

```bash
cargo run -p a2ui-dioxus-gallery             # 第一个样例
cargo run -p a2ui-dioxus-gallery -- 3        # 按 1 起始的序号
cargo run -p a2ui-dioxus-gallery -- login    # 按名称子串(大小写不敏感)
```

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
| `13_image` | 真实图片渲染（kitty / iTerm2 / Sixel / Halfblocks 自动降级） | `cargo run -p a2ui --example 13_image` |
| `14_audio` | 交互式 AudioPlayer（需 `audio` 特性） | `cargo run -p a2ui --example 14_audio` |
| `15_date_time_input` | 交互式 DateTimeInput | `cargo run -p a2ui --example 15_date_time_input` |
| `16_custom_component` | 自定义组件——实现 `TuiComponent` trait | `cargo run -p a2ui --example 16_custom_component` |
| `17_scifi_hud` | a2ui 驱动的赛博朋克 HUD（见上方截图） | `cargo run -p a2ui --example 17_scifi_hud` |
| `18_validate` | 载荷校验：完整性 / 拓扑 / `parse_and_fix`，STRICT vs RELAXED | `cargo run -p a2ui --example 18_validate` |

> 共 20 个示例（含 `07b` / `07c` 调试变体），完整列表见 `crates/a2ui/examples/`。

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
