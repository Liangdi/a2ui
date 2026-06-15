# a2ui-egui

[![crates.io](https://img.shields.io/crates/v/a2ui-egui.svg)](https://crates.io/crates/a2ui-egui)
[![docs.rs](https://docs.rs/a2ui-egui/badge.svg)](https://docs.rs/a2ui-egui)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

[English](README_EN.md) | 中文

> 📦 **a2ui** crate 生态成员 · egui 即时模式桌面后端(可选)
>
> 本 crate 是 [`a2ui`](https://crates.io/crates/a2ui) workspace 的第三渲染后端,完整介绍见[根目录 README](https://github.com/Liangdi/a2ui#readme)。

把 [A2UI](https://github.com/a2ui-project/a2ui) 组件树渲染到**原生桌面窗口**,基于 [egui](https://github.com/emilk/egui)(即时模式 GUI,固定 0.33)。与 [Slint](https://crates.io/crates/a2ui-slint) 后端不同,egui **原生支持递归**,因此无需展平组件树或 `build.rs` 有界深度代码生成 —— `walker::render_node` 直接在 `&mut egui::Ui` 上递归渲染。egui 还提供**真正的可交互原生控件**(TextField / Slider / CheckBox / ComboBox),而 Slint 后端把它们渲染为只读占位符。Button 的点击复用共享的 `core::components::dispatch_event` + `apply_event_result`,与其它两个后端一致。

> **可选依赖**:本 crate 是 workspace 的**非默认成员**(会拉取 winit + glow),普通 `cargo build` 不编译它。

## 在生态中的位置

```
┌───────────────────────────────────────────────────────────────────────┐
│  apps:   a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery  │
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui])          │
│  ▶ backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui            │
│  a2ui-base  (框架无关:Protocol / Model / Catalog / Processor)         │
└───────────────────────────────────────────────────────────────────────┘
```

`a2ui-egui` 依赖 [`a2ui-base`](https://crates.io/crates/a2ui-base);被 `a2ui-egui-gallery` 与(在 `egui` feature 下的)umbrella `a2ui` 依赖。

## 构建

一切代码都在 `backend` cargo feature 之后,它才拉入 egui + eframe 运行时。不带该 feature 时,本 crate 只是个空壳(除 `a2ui-base` 外无依赖),保持 workspace 默认构建轻量。

```bash
cargo build -p a2ui-egui --features backend
```

渲染器使用 glow(OpenGL),需要 GL 栈但无需专用 GPU 驱动。

## 即时模式状态桥(实现要点)

egui 控件每帧都需要一个稳定的 `&mut` 缓冲区(保留光标 / 滚动位置、检测值变化),但 A2UI 的值存在于 **data model**(每帧经由 `DataContext` 重新解析)。`EditBuffers` 这个持久化、按组件 id 索引的 map 桥接二者:每帧**用 data model 值播种**(若过期)→ 把 `&mut` 交给 egui 控件 → **检测变化** → 收集为 `PendingInteraction`,在整棵树遍历结束后(drops 掉 data model 的借用)再统一回写。

这与 TUI gallery 的「drop 借用再 mutate」、Slint host 的「回调再 redraw」是同构的。

## 模块

| 模块 | 职责 |
|------|------|
| `walker` | 递归渲染 A2UI 组件树 → `&mut egui::Ui` |
| `app` | `EguiApp` —— 持有 surface 状态、驱动即时模式渲染循环 |
| `components` | 各 A2UI 组件的 egui 实现(真实原生控件) |
| `edit_state` | `EditBuffers` —— 即时模式 ↔ data model 状态桥 |
| `interaction` | 把 egui 交互映射回共享的 core 交互层 |

## 许可证

MIT
