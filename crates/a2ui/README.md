# a2ui

[![crates.io](https://img.shields.io/crates/v/a2ui.svg)](https://crates.io/crates/a2ui)
[![docs.rs](https://docs.rs/a2ui/badge.svg)](https://docs.rs/a2ui)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

[English](README_EN.md) | 中文

> 📦 **a2ui** crate 生态 · umbrella crate(统一入口)
>
> 这是 [`a2ui`](https://crates.io/crates/a2ui) workspace 的伞 crate,把各子 crate 重新导出到稳定的 `a2ui::core` / `a2ui::tui` 路径下。完整介绍见[根目录 README](https://github.com/Liangdi/a2ui#readme)。

[A2UI (Agent to UI) v1.0](https://github.com/a2ui-project/a2ui) 协议的 Rust 实现:渲染由 AI Agent 动态生成、JSON 流式驱动的用户界面。本 crate 是 **umbrella**,一行依赖即可拿到核心层与默认终端后端;Slint / egui 桌面后端按需在 feature 后开启。

## 生态全景

```
┌───────────────────────────────────────────────────────────────────────┐
│  apps:  a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery   │
├───────────────────────────────────────────────────────────────────────┤
│  ▶ a2ui  (umbrella: re-export core + tui [+ slint] [+ egui])          │
├───────────────────────────────────────────────────────────────────────┤
│  backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui              │
│  a2ui-base  (框架无关:Protocol / Model / Catalog / Processor)         │
└───────────────────────────────────────────────────────────────────────┘
```

| 子 crate | 作用 | 在 umbrella 下的路径 |
|----------|------|----------------------|
| [`a2ui-base`](https://crates.io/crates/a2ui-base) | 框架无关核心层 | `a2ui::core` |
| [`a2ui-tui`](https://crates.io/crates/a2ui-tui) | ratatui 终端后端(默认) | `a2ui::tui` |
| [`a2ui-slint`](https://crates.io/crates/a2ui-slint) | Slint 桌面后端(可选) | `a2ui::slint`(`slint` feature) |
| [`a2ui-egui`](https://crates.io/crates/a2ui-egui) | egui 桌面后端(可选) | `a2ui::egui`(`egui` feature) |

> 默认只 re-export `core` + `tui`。两个桌面后端较重(Slint 工具链 / winit + glow),故按需开启。

## 特性

| 特性 | 说明 | 启用 |
|------|------|------|
| `slint` | 把 Slint 后端 re-export 为 `a2ui::slint` | `--features slint` |
| `egui` | 把 egui 后端 re-export 为 `a2ui::egui` | `--features egui` |
| `audio` | 转发给 `a2ui-tui` 的真实音频播放 | `--features audio` |

## 用法

```bash
cargo add a2ui            # 核心 + 默认终端后端
cargo add a2ui --features egui   # 额外开启 egui 桌面后端
```

```rust
// 路径保持稳定 —— 这正是 umbrella 存在的意义
use a2ui::core::message_processor::MessageProcessor;
use a2ui::tui::catalogs::basic::{build_basic_catalog, build_basic_registry};
```

## 示例

本 crate 自带 17 个示例,是上手 A2UI 的最佳入口:

```bash
cargo run -p a2ui --example 01_hello_world
cargo run -p a2ui --example 04_login_form
cargo run -p a2ui --example 12_handshake      # 能力协商握手
```

完整示例表见[根目录 README](https://github.com/Liangdi/a2ui#readme)。

## 许可证

MIT
