# a2ui-base

[![crates.io](https://img.shields.io/crates/v/a2ui-base.svg)](https://crates.io/crates/a2ui-base)
[![docs.rs](https://docs.rs/a2ui-base/badge.svg)](https://docs.rs/a2ui-base)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

[English](README_EN.md) | 中文

> 📦 **a2ui** crate 生态成员 · 框架无关核心层
>
> 本 crate 是 [`a2ui`](https://crates.io/crates/a2ui) workspace 的基础子 crate,完整介绍见[根目录 README](https://github.com/Liangdi/a2ui#readme)。

[A2UI (Agent to UI) v1.0](https://github.com/a2ui-project/a2ui) 协议的**框架无关核心层**:协议类型、组件 / 数据模型、Catalog、消息处理器、能力协商、校验,以及所有 UI 后端共享的交互层。**不依赖任何 UI 框架**(ratatui / Slint / egui 都不引入),可独立用于其他 backend 或纯协议解析场景。

## 在生态中的位置

```
┌───────────────────────────────────────────────────────────────────────┐
│  apps:  a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery   │
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui])          │
│  backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui              │
├───────────────────────────────────────────────────────────────────────┤
│  ▶ a2ui-base  (框架无关:Protocol / Model / Catalog / Processor)       │
└───────────────────────────────────────────────────────────────────────┘
```

`a2ui-base` 是整个 workspace 的地基 —— 三个后端(ratatui / Slint / egui)都建立在它之上。框架无关的交互逻辑(焦点遍历 `focus`、事件结果应用 `interaction`、组件行为 `components`)统一放在这里,保证不同后端在键盘 / 按钮交互上行为一致。

## 模块

| 模块 | 职责 |
|------|------|
| `protocol` | A2UI v1.0 JSON 消息类型(server→client、client→server) |
| `model` | 运行时组件树、surface、JSON Pointer 数据绑定 |
| `catalog` | Catalog、组件 API、函数实现、schema-only 函数、内联 catalog |
| `message_processor` | 消息解析 → 状态变更;`process_message` / `parse_jsonl` |
| `capabilities` | `ClientCapabilities` / `ServerCapabilities` 协商 + 内联 catalog 解析(UAX#31 校验) |
| `validate` | 协议校验(`ValidationConfig` / `ValidationReport`) |
| `observable` | 响应式状态管理 |
| `focus` / `interaction` / `components` | 后端共享的交互层(焦点遍历、`EventResult` 应用、组件 `handle_event`) |

## 用法

```bash
cargo add a2ui-base
```

```rust
use a2ui_base::message_processor::MessageProcessor;
use a2ui_base::catalog::Catalog;

let mut processor = MessageProcessor::new(vec![/* catalogs */]);

// 解析并处理一条 JSON 消息
let msg = MessageProcessor::parse_message(r#"{"version":"v1.0",...}"#)?;
processor.process_message(msg)?;

// 读取协议产生的回传消息(functionResponse / actionResponse / ...)
let outgoing = processor.drain_outgoing();
```

> 想直接得到一个能渲染的终端 UI?组合 [`a2ui-tui`](https://crates.io/crates/a2ui-tui) 一起用;或通过 umbrella [`a2ui`](https://crates.io/crates/a2ui) 以 `a2ui::core::...` 路径访问本 crate。

## 许可证

MIT
