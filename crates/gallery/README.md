# a2ui-gallery

[![crates.io](https://img.shields.io/crates/v/a2ui-gallery.svg)](https://crates.io/crates/a2ui-gallery)
[![docs.rs](https://docs.rs/a2ui-gallery/badge.svg)](https://docs.rs/a2ui-gallery)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

[English](README_EN.md) | 中文

> 📦 **a2ui** crate 生态成员 · 终端 Gallery 示例浏览器
>
> 本 crate 是 [`a2ui`](https://crates.io/crates/a2ui) workspace 的展示应用(ratatui 后端),完整介绍见[根目录 README](https://github.com/Liangdi/a2ui#readme)。

浏览并逐步渲染 A2UI 规范样例的**终端应用**(bin + lib)。把 [A2UI](https://github.com/a2ui-project/a2ui) 官方 spec 树在编译期嵌入二进制(`include_dir`),因此分发时无需随附磁盘上的 spec 目录。桌面版的 Slint / egui gallery 复用本 crate 的 `sample_loader` 加载同样的样例。

## 在生态中的位置

```
┌───────────────────────────────────────────────────────────────────────┐
│  ▶ apps:   a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery│
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui])          │
│  backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui              │
│  a2ui-base  (框架无关:Protocol / Model / Catalog / Processor)         │
└───────────────────────────────────────────────────────────────────────┘
```

`a2ui-gallery` 同时发布为库(`a2ui_gallery`),其中 `sample_loader` 被三个 gallery 应用共享;`a2ui-slint-gallery` / `a2ui-egui-gallery` 直接依赖它。

## 安装 / 运行

```bash
# 直接运行
cargo run -p a2ui-gallery

# 安装为系统二进制 a2ui_gallery
cargo install a2ui-gallery
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

## 作为库使用

```rust
use a2ui_gallery::sample_loader;

// 从内嵌的 spec 树加载某个 catalog 的样例
let samples = sample_loader::load_samples("v1_0/catalogs/basic/examples");
```

> 开发期可设置 `A2UI_SPEC_DIR` 环境变量,改为从磁盘目录读取样例(便于不重编译地测试 spec 变更)。

## 许可证

MIT
