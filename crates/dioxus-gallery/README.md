# a2ui-dioxus-gallery

[![crates.io](https://img.shields.io/crates/v/a2ui-dioxus-gallery.svg)](https://crates.io/crates/a2ui-dioxus-gallery)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

[English](README_EN.md) | 中文

> 📦 **a2ui** crate 生态成员 · Dioxus WebView 桌面 Gallery 示例浏览器
>
> 本 crate 是 [`a2ui`](https://crates.io/crates/a2ui) workspace 的展示应用(Dioxus 后端),完整介绍见[根目录 README](https://github.com/Liangdi/a2ui#readme)。

[`a2ui-gallery`](https://crates.io/crates/a2ui-gallery)(终端版)等其它 gallery 的 Dioxus 桌面对应物:复用同样的内嵌 A2UI spec 样例与同样的 catalog / 函数构建器,但通过 [`a2ui-dioxus`](https://crates.io/crates/a2ui-dioxus) 把样例渲染到真实的 OS WebView 窗口(左侧样例列表 + 右侧预览)。得益于 WebView,输入控件是**真正可交互**的(原生 HTML 表单控件);Modal 以居中浮层 + 半透明遮罩形式叠加在主界面上方。深色主题是一份注入到文档 `<head>` 的 **CSS 样式表**(与 Iced/egui gallery 同一套 Catppuccin-Mocha + 绿色主调)。

## 在生态中的位置

```
┌────────────────────────────────────────────────────────────────────────────┐
│  ▶ apps:   a2ui-gallery (TUI)   a2ui-{slint,egui,bevy,iced,dioxus}-gallery │
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui] [+ bevy] [+ iced] [+ dioxus]) │
│  backends:   a2ui-tui (ratatui)   a2ui-{slint,egui,bevy,iced,dioxus}       │
│  a2ui-base  (框架无关:Protocol / Model / Catalog / Processor)             │
└────────────────────────────────────────────────────────────────────────────┘
```

## 运行

```bash
cargo run -p a2ui-dioxus-gallery             # 第一个样例
cargo run -p a2ui-dioxus-gallery -- 3        # 按 1 起始的序号
cargo run -p a2ui-dioxus-gallery -- login    # 按名称子串(大小写不敏感)
```

启动时会把可用样例(序号 + 名称)打到 stdout。窗口 1080×740。

> Linux 需系统安装 **WebKitGTK(`webkit2gtk-4.1`)+ GTK 3**。

## 它如何把状态交给无 prop 的 Gallery

Dioxus 的 `launch(app)` 接受的是一个 `fn() -> Element`(无参函数指针),且组件 prop 必须 `Clone + PartialEq`(而 `MessageProcessor` 与函数表都不可 Clone)。所以样例列表 + 初始序号(可 `Clone`)经 `LaunchBuilder::with_context` 注入根 context;catalogs + 函数表则在 `app()` 里就地重建并装进 `Signal` / `Rc`,再通过 context 分享给无 prop 的 `Gallery` 与递归 `A2uiNode`。这跟 Iced gallery 那个 `Mutex<Option<…>>` boot hack 不同(Iced 的 boot 闭包必须是 `Fn`,而 Dioxus 的 `use_signal` 初始化器是 `FnOnce`)。

## License

MIT
