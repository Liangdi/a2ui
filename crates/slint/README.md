# a2ui-slint

[![crates.io](https://img.shields.io/crates/v/a2ui-slint.svg)](https://crates.io/crates/a2ui-slint)
[![docs.rs](https://docs.rs/a2ui-slint/badge.svg)](https://docs.rs/a2ui-slint)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

[English](README_EN.md) | 中文

> 📦 **a2ui** crate 生态成员 · Slint 原生桌面后端(可选)
>
> 本 crate 是 [`a2ui`](https://crates.io/crates/a2ui) workspace 的第二渲染后端,完整介绍见[根目录 README](https://github.com/Liangdi/a2ui#readme)。

把 [A2UI](https://github.com/a2ui-project/a2ui) 组件树渲染到**原生桌面窗口**,基于 [Slint](https://slint.dev/)(固定 1.16)。框架无关的交互逻辑(焦点遍历、事件分发)共享在 [`a2ui-base`](https://crates.io/crates/a2ui-base) 中,因此它与终端后端在键盘 / 按钮交互上表现一致。

> **可选且较重**:本 crate 是 workspace 的**非默认成员**,普通 `cargo build` 不编译它。

## 在生态中的位置

```
┌───────────────────────────────────────────────────────────────────────┐
│  apps:   a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery  │
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui])          │
│  ▶ backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui            │
│  a2ui-base  (框架无关:Protocol / Model / Catalog / Processor)         │
└───────────────────────────────────────────────────────────────────────┘
```

`a2ui-slint` 依赖 [`a2ui-base`](https://crates.io/crates/a2ui-base);被 `a2ui-slint-gallery` 与(在 `slint` feature 下的)umbrella `a2ui` 依赖。

## 构建

一切代码都在 `backend` cargo feature 之后,它才拉入 Slint 运行时。不带该 feature 时,本 crate 只是个空壳(除 `a2ui-base` 外无依赖),保持 workspace 默认构建轻量。

```bash
cargo build -p a2ui-slint --features backend
```

渲染器使用 `renderer-software` + `backend-winit`,**无需 GPU / OpenGL 驱动**即可运行。

## 组件覆盖

全部 18 个 A2UI 组件类型均可渲染:

- **富渲染**:Text / Button / Column / Row / Card / TextField / CheckBox / Slider(Button 与 CheckBox 的点击通过共享的 `core::components::dispatch_event` 分发)
- **尽力渲染**:Divider / Icon / Tabs / Modal / List / ChoicePicker / DateTimeInput
- **占位符**:Image / Video / AudioPlayer 渲染为带标签的占位符

## 实现要点:为什么需要展平组件树

Slint **无法表达递归**(既不支持递归 struct,也不支持自引用组件 —— 见 [slint-ui/slint#4218](https://github.com/slint-ui/slint/issues/4218))。因此 `live_tree` 不是嵌套树,而是把组件树展平为一个 `Vec<LiveNode>`,通过基于索引的 `children` 引用;`build.rs` 代码生成了**有界深度**的组件链 `Node0`(叶子)→ … → `Node7`(根)。A2UI 树通常很浅,深度 7 足以覆盖实际 UI;更深的子树会被截断为 `…`。

> 这是未来贡献者最需要了解的关键约束。若需要原生递归 + 真正可交互的输入控件,改用 [`a2ui-egui`](https://crates.io/crates/a2ui-egui) 后端。

## 模块

| 模块 | 职责 |
|------|------|
| `live_tree` | 展平的节点数组(规避 Slint 递归限制) |
| `host` | `SurfaceHost::run` —— 持有状态、驱动 Slint 事件循环 |
| `ui` | `include_modules!()` 引入的生成模块 + `LiveNode` 类型 |

## 许可证

MIT
