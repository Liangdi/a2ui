# A2UI — 基于 Ratatui 的 TUI 渲染器

[![crates.io](https://img.shields.io/crates/v/a2ui.svg)](https://crates.io/crates/a2ui)
[![docs.rs](https://docs.rs/a2ui/badge.svg)](https://docs.rs/a2ui)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

[English](README_EN.md) | 中文

一个 Rust 实现的 [A2UI (Agent to UI) v1.0](https://github.com/a2ui-project/a2ui) 协议终端渲染器，基于 [ratatui](https://ratatui.rs/) 构建。

A2UI 是一个 JSON 流式 UI 协议，允许 AI Agent 动态生成和更新终端用户界面。

## 特性

- ✅ 完整的 A2UI v1.0 协议支持
- ✅ **18 个 TUI 组件**：Text, Row, Column, Button, TextField, Card, Divider, List, CheckBox, Icon, Tabs, Modal, Slider, ChoicePicker, DateTimeInput, Image, Video, AudioPlayer
  - 交互式：Text、Row、Column、Button、TextField、Slider、CheckBox、ChoicePicker、DateTimeInput（箭头键调节值）
  - 占位符（默认）：Image / Video / AudioPlayer 仅渲染文本占位符（`[🖼 description]`、`[▶ url]`、`[♫ url]`），终端无法解码像素/音频/视频。可通过下方「可选特性」开启真实图片/音频渲染。
- ✅ **能力协商（Capabilities negotiation）**：`ClientCapabilities` / `ServerCapabilities` 类型 + 构建器（从已注册 catalog 自动派生 `supportedCatalogIds`）。
- ✅ **内联目录（Inline catalogs）**：服务端可声明 `acceptsInlineCatalogs`；客户端解析并校验内联 catalog JSON（UAX#31 标识符检查），运行时注册 schema-only 函数。
- ✅ **自定义组件兜底渲染**：未知 / 内联自定义组件类型以可见的标签框（类型 + 属性 + 子节点）呈现，而非裸「unknown」错误。
- ✅ **14 个客户端函数**：required, regex, length, numeric, email, and/or/not, formatString, formatNumber, formatCurrency, formatDate, pluralize, openUrl
- ✅ 模块化分层架构（Core Layer + TUI Layer）
- ✅ JSON Pointer 数据绑定与响应式状态管理
- ✅ Gallery App 示例浏览器（支持消息逐步渲染）
- ✅ **150 个单元/集成测试**（core 102 + tui 48），包含 A2UI 规范样例的端到端测试

## 截图

**Gallery 示例浏览器**

![Gallery](screenshot/gallery.png)

**登录表单**

![Login Form](screenshot/login-form.png)

**Agent Chat**（AI 对话界面，`08_agent_chat` 示例：多 surface 聊天布局、流式 A2UI 消息、Card / Column / Row / Divider 等富组件）

![Agent Chat](screenshot/agent-chat.png)

**邀请函构建器**（规范样例 `30_live-invitation-builder`：响应式表单布局，TextField / Slider / ChoicePicker / DateTimeInput 等交互组件协同，实时预览邀请内容）

![Invitation Builder](screenshot/invitation-builder.png)

## 快速开始

```bash
# 运行 Gallery App
cargo run

# 最小化能力协商握手演示（无 TUI）
cargo run --example 12_handshake
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
┌─────────────────────────────────────┐
│  Gallery App (main.rs)              │  ← 示例应用
├─────────────────────────────────────┤
│  TUI Layer (src/tui/)              │  ← ratatui 组件实现
│  Surface, Components, Catalogs     │
├─────────────────────────────────────┤
│  Core Layer (src/core/)            │  ← 框架无关
│  Protocol, Models, Catalog,        │
│  MessageProcessor, Observable      │
└─────────────────────────────────────┘
```

### 项目结构

```
src/
├── lib.rs                    # Crate 根
├── main.rs                   # Gallery App 入口
├── core/                     # 框架无关层
│   ├── error.rs              # 错误类型
│   ├── protocol/             # A2UI 协议类型
│   │   ├── common_types.rs   # DynamicString, FunctionCall, ChildList...
│   │   ├── server_to_client.rs
│   │   └── client_to_server.rs
│   ├── model/                # 状态模型
│   │   ├── data_model.rs     # JSON Pointer 数据存储
│   │   ├── component_model.rs
│   │   ├── surface_model.rs
│   │   ├── data_context.rs   # 作用域数据访问 + 动态值解析
│   │   └── ...
│   ├── catalog/              # 目录系统
│   │   ├── catalog.rs        # Catalog 组件/函数注册
│   │   ├── basic_functions.rs # 14 个 Basic Catalog 函数
│   │   └── ...
│   ├── observable/           # EventStream, Signal
│   └── message_processor.rs  # 消息解析 → 状态变更
├── tui/                      # ratatui 渲染层
│   ├── surface.rs            # 递归渲染入口
│   ├── component_impl.rs     # TuiComponent trait
│   ├── layout_engine.rs      # 权重分割 / 对齐
│   ├── focus_manager.rs      # 键盘焦点管理
│   ├── components/           # 18 个组件实现
│   └── catalogs/             # Minimal + Basic Catalog 组装
└── gallery/                  # Gallery 示例应用
    ├── app.rs                # 主事件循环
    └── sample_loader.rs      # 加载 JSON 样例
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
| `01_hello_world` | 最简单的 A2UI 程序 | `cargo run --example 01_hello_world` |
| `02_jsonl_stream` | JSONL 流式处理与逐步渲染 | `cargo run --example 02_jsonl_stream` |
| `03_data_binding` | JSON Pointer 响应式数据绑定 | `cargo run --example 03_data_binding` |
| `04_login_form` | 完整表单：输入、验证、焦点管理、Action | `cargo run --example 04_login_form` |
| `05_custom_function` | 自定义 Catalog 函数 | `cargo run --example 05_custom_function` |
| `06_call_function` | 服务端 `callFunction` 消息与 `functionResponse` | `cargo run --example 06_call_function` |
| `07_action_response` | `actionResponse` 与 `responsePath` 响应式更新 | `cargo run --example 07_action_response` |
| `12_handshake` | 能力协商握手（Capabilities negotiation） | `cargo run --example 12_handshake` |

## 可选特性 (Optional Features)

图片渲染**内置且默认开启**：默认 `cargo build` 即通过 `ratatui-image` 进行真实图片渲染（kitty / iTerm2 / Sixel / Halfblocks 自动降级），仅支持本地文件路径，无法加载时回退为占位符。以下为额外的**可选**特性，默认关闭：

| 特性 | 说明 | 启用 | 限制 |
|------|------|------|------|
| `audio` | 通过 `rodio` 进行真实音频播放（后台线程） | `--features audio` | **仅支持本地文件路径**；需安装 ALSA 系统开发库（Fedora: `alsa-lib-devel`，Debian: `libasound2-dev`）；失败时静默回退为占位符 |
| — (Video) | 视频无对应特性 | — | 终端尚无成熟的 TUI 视频方案，始终渲染占位符 |

## 作为库使用

```rust
use a2ui::core::message_processor::MessageProcessor;
use a2ui::core::catalog::Catalog;
use a2ui::tui::catalogs::basic::{build_basic_catalog, build_basic_registry};

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

## 许可证

MIT
