# a2ui-dioxus

[![crates.io](https://img.shields.io/crates/v/a2ui-dioxus.svg)](https://crates.io/crates/a2ui-dioxus)
[![docs.rs](https://docs.rs/a2ui-dioxus/badge.svg)](https://docs.rs/a2ui-dioxus)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

[English](README_EN.md) | 中文

> 📦 **a2ui** crate 生态成员 · Dioxus 响应式 WebView 桌面后端(可选)
>
> 本 crate 是 [`a2ui`](https://crates.io/crates/a2ui) workspace 的第六渲染后端,完整介绍见[根目录 README](https://github.com/Liangdi/a2ui#readme)。

把 [A2UI](https://github.com/a2ui-project/a2ui) 组件树渲染到**原生桌面 WebView 窗口**,基于 [Dioxus](https://github.com/DioxusLabs/dioxus)(响应式 signals 架构,固定 0.7)。在六个后端里这是**架构上最独特**的一个:

- **响应式 signals** —— Dioxus 像 React:运行时状态放在根组件的 `Signal` 里,UI 是对它的纯读取。所以——既不需要 Iced 后端的 `Message` 枚举(Elm view/update),也不需要 egui 后端的 `EditBuffers` 状态桥(即时模式整帧借用 data model)。**无消息枚举,无状态桥**。信号本身就是交互通道:任何写入都会自动重渲染订阅了它的组件。
- **递归组件** —— 整棵树就是**一个** `A2uiNode` 组件,它逐节点渲染自身(Dioxus 原生支持递归组件,不像 Slint 要 bounded-depth codegen)。
- **WebView 渲染** —— Dioxus desktop 渲染到系统 WebView(Linux 用 WebKitGTK),所以深色主题是一份 **CSS 样式表**(`theme::STYLESHEET`),而非一组逐控件的 style 函数;A2UI 组件种类映射到普通 HTML 元素 + class。

Button 的点击同样复用共享的 `core::components::dispatch_event` + `apply_event_result`(经 context 注入的 `Rc<dyn Fn(String)>` 回调上交到 gallery 根处理)。

> **可选依赖**:本 crate 是 workspace 的**非默认成员**(会拉取 wry WebView + tao 窗口栈),普通 `cargo build` 不编译它。

## 在生态中的位置

```
┌────────────────────────────────────────────────────────────────────────────┐
│  apps:   a2ui-gallery (TUI)   a2ui-{slint,egui,bevy,iced,dioxus}-gallery   │
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui] [+ bevy] [+ iced] [+ dioxus]) │
│  ▶ backends:   a2ui-tui (ratatui)   a2ui-{slint,egui,bevy,iced,dioxus}     │
│  a2ui-base  (框架无关:Protocol / Model / Catalog / Processor)             │
└────────────────────────────────────────────────────────────────────────────┘
```

`a2ui-dioxus` 只依赖 [`a2ui-base`](https://crates.io/crates/a2ui-base);被 `a2ui-dioxus-gallery` 与(在 `dioxus` feature 下的)umbrella `a2ui` 依赖。

## 构建

一切代码都在 `backend` cargo feature 之后,它才拉入 Dioxus desktop 运行时(wry WebView + tao 窗口)。不带该 feature 时,本 crate 只是个空壳(除 `a2ui-base` 外无依赖),保持 workspace 默认构建轻量。

```bash
cargo build -p a2ui-dioxus --features backend
```

Linux 上链接 **WebKitGTK(`webkit2gtk-4.1`)+ GTK 3**(需系统安装,与其他原生窗口后端的 GTK/X11 依赖同理)。

## 模块

- [`node`](src/node.rs) —— 递归 `A2uiNode` 组件,逐节点匹配 A2UI 组件种类渲染成 HTML(对应 iced 的 `walker` + `components`)。
- [`app`](src/app.rs) —— `Gallery` 根组件(无 prop,从 context 读状态):侧边栏 + 预览面板 + Modal 覆盖层 + Button 激活流程(对应 `IcedApp`)。
- [`theme`](src/theme.rs) —— 整套深色 Catppuccin-Mocha + 绿色主调 CSS(对应 iced 的 `style`)。

## License

MIT
