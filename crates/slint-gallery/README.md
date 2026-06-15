# a2ui-slint-gallery

[![crates.io](https://img.shields.io/crates/v/a2ui-slint-gallery.svg)](https://crates.io/crates/a2ui-slint-gallery)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

[English](README_EN.md) | 中文

> 📦 **a2ui** crate 生态成员 · Slint 桌面 Gallery 示例浏览器
>
> 本 crate 是 [`a2ui`](https://crates.io/crates/a2ui) workspace 的展示应用(Slint 后端),完整介绍见[根目录 README](https://github.com/Liangdi/a2ui#readme)。

[`a2ui-gallery`](https://crates.io/crates/a2ui-gallery)(终端版)的 Slint 桌面对应物:复用同样的内嵌 A2UI spec 样例与同样的 catalog / 函数构建器,但通过 [`a2ui-slint`](https://crates.io/crates/a2ui-slint) 把样例渲染到真实的 OS 窗口(左侧样例列表 + 右侧预览)。

## 在生态中的位置

```
┌───────────────────────────────────────────────────────────────────────┐
│  ▶ apps:   a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery│
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui])          │
│  backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui              │
│  a2ui-base  (框架无关:Protocol / Model / Catalog / Processor)         │
└───────────────────────────────────────────────────────────────────────┘
```

## 运行

```bash
cargo run -p a2ui-slint-gallery             # 第一个样例
cargo run -p a2ui-slint-gallery -- 3        # 按 1 起始的序号
cargo run -p a2ui-slint-gallery -- login    # 按名称子串(大小写不敏感)
```

启动时会在终端打印完整的带编号样例列表。渲染器使用 `renderer-software` + `backend-winit`,**无需 GPU / OpenGL 驱动**即可运行。

## 安装

```bash
cargo install a2ui-slint-gallery
```

> 开发期可设置 `A2UI_SPEC_DIR` 环境变量,改为从磁盘目录读取样例(便于不重编译地测试 spec 变更)。

## 许可证

MIT
