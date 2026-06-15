# a2ui-iced

[![crates.io](https://img.shields.io/crates/v/a2ui-iced.svg)](https://crates.io/crates/a2ui-iced)
[![docs.rs](https://docs.rs/a2ui-iced/badge.svg)](https://docs.rs/a2ui-iced)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

[English](README_EN.md) | 中文

> 📦 **a2ui** crate 生态成员 · Iced Elm 架构桌面后端(可选)
>
> 本 crate 是 [`a2ui`](https://crates.io/crates/a2ui) workspace 的第五渲染后端,完整介绍见[根目录 README](https://github.com/Liangdi/a2ui#readme)。

把 [A2UI](https://github.com/a2ui-project/a2ui) 组件树渲染到**原生桌面窗口**,基于 [Iced](https://github.com/iced-rs/iced)(Elm 架构,固定 0.14)。在五个后端里这是**最干净**的映射:Iced 是 Elm —— `view(&state)` 返回一棵不可变的 `Element` 树,`update(&mut state, msg)` 改状态。所以可交互控件在 `view` 里**直接读 data model**,在 `update` 里**通过 `Message` 回写** —— 既不需要 egui 后端那种 `EditBuffers` 状态桥(即时模式下 data model 整帧被借用),也不需要 bevy 后端那种 reconciler(保留模式 ECS 要 diff/patch 实体树)。**无状态桥,无 diff**。Button 的点击同样复用共享的 `core::components::dispatch_event` + `apply_event_result`。

> **可选依赖**:本 crate 是 workspace 的**非默认成员**(会拉取 wgpu + winit),普通 `cargo build` 不编译它。

## 在生态中的位置

```
┌────────────────────────────────────────────────────────────────────────────┐
│  apps:   a2ui-gallery (TUI)   a2ui-{slint,egui,bevy,iced}-gallery          │
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui] [+ bevy] [+ iced]) │
│  ▶ backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui   a2ui-bevy   a2ui-iced │
│  a2ui-base  (框架无关:Protocol / Model / Catalog / Processor)             │
└────────────────────────────────────────────────────────────────────────────┘
```

`a2ui-iced` 依赖 [`a2ui-base`](https://crates.io/crates/a2ui-base);被 `a2ui-iced-gallery` 与(在 `iced` feature 下的)umbrella `a2ui` 依赖。

## 构建

一切代码都在 `backend` cargo feature 之后,它才拉入 Iced 运行时(wgpu 渲染器 + winit 窗口)。不带该 feature 时,本 crate 只是个空壳(除 `a2ui-base` 外无依赖),保持 workspace 默认构建轻量。

```bash
cargo build -p a2ui-iced --features backend
```

渲染器默认使用 wgpu(GPU),并提供 tiny-skia 软件渲染兜底。

## 为什么不需要状态桥(实现要点)

Iced 是 Elm:`view(&self)` 借用 surface 的 data model / components(只读)构建一棵**拥有自己数据**的元素树(`text(String)`、`text_input(placeholder, value)` 等控件都把传入的 `&str` **拷贝**进自有存储,返回元素的 `'a` 生命周期只绑定到 `on_*` 闭包所捕获的 owned `Message`)。用户交互由控件附着的 `Message` 表达,`update(&mut self, msg)` 在 `view` 返回后(借用已释放)再回写。

因为 `view` 与 `update` 永不重叠,所以**没有** egui 后端那种「collect-then-apply」的 `PendingInteraction` 缓冲,也**没有**需要 seed/detect/writeback 生命周期的 `EditBuffers`。`Message` 流本身就是交互桥。这是其它四个后端里最省心的。

## 模块

| 模块 | 职责 |
|------|------|
| `walker` | 递归把 A2UI 组件树构建成 `Element` 树(纯函数,返回 owned 树) |
| `app` | `IcedApp` —— 持有 surface 状态、提供 Elm `view`/`update` 对 |
| `components` | 各 A2UI 组件的 Iced 实现(真实原生控件) |
| `message` | `Message` —— 控件产出、`update` 消费的 Elm 交互通道 |

## 许可证

MIT
