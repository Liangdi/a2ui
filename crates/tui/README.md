# a2ui-tui

[![crates.io](https://img.shields.io/crates/v/a2ui-tui.svg)](https://crates.io/crates/a2ui-tui)
[![docs.rs](https://docs.rs/a2ui-tui/badge.svg)](https://docs.rs/a2ui-tui)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

[English](README_EN.md) | 中文

> 📦 **a2ui** crate 生态成员 · 默认终端后端(ratatui)
>
> 本 crate 是 [`a2ui`](https://crates.io/crates/a2ui) workspace 的默认渲染后端,完整介绍见[根目录 README](https://github.com/Liangdi/a2ui#readme)。

把 [A2UI](https://github.com/a2ui-project/a2ui) 组件树渲染到**终端字符网格**,基于 [ratatui](https://ratatui.rs/) + [crossterm](https://github.com/crossterm-rs/crossterm) 构建。这是 a2ui 的默认后端(workspace `default-members` 之一),普通 `cargo build` 即编译它。

## 在生态中的位置

```
┌───────────────────────────────────────────────────────────────────────┐
│  apps:   a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery  │
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui])          │
│  ▶ backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui            │
├───────────────────────────────────────────────────────────────────────┤
│  a2ui-base  (框架无关:Protocol / Model / Catalog / Processor)         │
└───────────────────────────────────────────────────────────────────────┘
```

`a2ui-tui` 依赖 [`a2ui-base`](https://crates.io/crates/a2ui-base);`a2ui-gallery` 与 umbrella `a2ui` 依赖它。

## 特性

- ✅ **18 个 TUI 组件**:Text, Row, Column, Button, TextField, Card, Divider, List, CheckBox, Icon, Tabs, Modal, Slider, ChoicePicker, DateTimeInput, Image, Video, AudioPlayer
- ✅ 交互式组件(箭头键调节值):Text、Row、Column、Button、TextField、Slider、CheckBox、ChoicePicker、DateTimeInput
- ✅ **默认开启真实图片渲染**(`ratatui-image`,kitty / iTerm2 / Sixel / Halfblocks 自动降级,仅本地文件路径)
- ✅ 权重分割 / 对齐的布局引擎(`layout_engine`)
- ✅ 键盘焦点管理(`focus_manager`)
- ✅ Minimal + Basic Catalog 组装(`catalogs`)

### 可选特性

| 特性 | 说明 | 启用 |
|------|------|------|
| `audio` | 经 `rodio` 进行真实音频播放(后台线程,仅本地文件路径,需 ALSA 系统库) | `--features audio` |

> 视频无对应特性,终端尚无成熟方案,始终渲染占位符。

## 模块

| 模块 | 职责 |
|------|------|
| `surface` | 递归渲染入口(`SurfaceRenderer`) |
| `component_impl` | `TuiComponent` trait + 注册 |
| `layout_engine` | 权重分割 / 对齐 |
| `focus_manager` | 键盘焦点管理 |
| `components` | 18 个组件实现 |
| `catalogs` | Minimal + Basic Catalog 组装 |

## 用法

```bash
cargo add a2ui-base a2ui-tui
```

```rust
use a2ui_base::message_processor::MessageProcessor;
use a2ui_tui::catalogs::basic::{build_basic_catalog, build_basic_registry};
use a2ui_tui::surface::SurfaceRenderer;

let catalog = build_basic_catalog();
let registry = build_basic_registry();
let mut processor = MessageProcessor::new(vec![catalog]);

processor.process_message(MessageProcessor::parse_message(json)?)?;

let surface = processor.model.get_surface("main").unwrap();
let renderer = SurfaceRenderer::new(surface, &registry, &catalog);
renderer.render(&mut frame, area);
```

> 通过 umbrella 时把 `a2ui_base::` / `a2ui_tui::` 换成 `a2ui::core::` / `a2ui::tui::` 即可。

## 许可证

MIT
