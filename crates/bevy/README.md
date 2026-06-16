# a2ui-bevy

[![crates.io](https://img.shields.io/crates/v/a2ui-bevy.svg)](https://crates.io/crates/a2ui-bevy)
[![docs.rs](https://docs.rs/a2ui-bevy/badge.svg)](https://docs.rs/a2ui-bevy)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Liangdi/a2ui/blob/master/LICENSE)

[English](README_EN.md) | 中文

> 📦 **a2ui** crate 生态成员 · Bevy ECS UI 后端(可选)
>
> 本 crate 是 [`a2ui`](https://crates.io/crates/a2ui) workspace 的第四渲染后端,完整介绍见[根目录 README](https://github.com/Liangdi/a2ui#readme)。

把 [A2UI](https://github.com/a2ui-project/a2ui) 组件树翻译成**保留式(retained)Bevy UI 实体树**,基于 [Bevy](https://bevyengine.org) 0.18 的 ECS UI 栈。与 [egui](https://crates.io/crates/a2ui-egui) 后端(即时模式,每帧重建并用 `EditBuffers` map 携带控件状态)不同,Bevy 是**保留式 ECS**:控件是跨帧存活的实体。由于 Bevy 的可交互控件(`bevy_ui_widgets` 的 Button / Checkbox / Slider 以及外部 `bevy_ui_text_input`)只有在**实体身份跨帧保持不变**时才能正确维持拖拽 / 悬停 / 焦点 / 光标状态,本后端引入了一个 **React 风格的 reconciler**:它维护一个稳定的 `HashMap<component_id, Entity>`,每帧增量地 spawn / update / despawn / reorder。因为文本输入实体持久存在,它自己持有光标与编辑状态,所以**无需** `EditBuffers` map。Button / 值变化交互复用共享的 `core::components::dispatch_event` + `apply_event_result`,与其它后端一致。

> **可选依赖**:本 crate 是 workspace 的**非默认成员**(会拉取 Bevy 的 wgpu + winit 工具链),普通 `cargo build` 不编译它。

## 在生态中的位置

```
┌─────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│  apps:   a2ui-gallery (TUI)   a2ui-slint-gallery   a2ui-egui-gallery   a2ui-bevy-gallery   a2ui-iced-gallery│
│  umbrella:   a2ui  (re-export core + tui [+ slint] [+ egui] [+ bevy] [+ iced])                              │
│  ▶ backends:   a2ui-tui (ratatui)   a2ui-slint   a2ui-egui   a2ui-bevy   a2ui-iced                          │
│  a2ui-base  (框架无关:Protocol / Model / Catalog / Processor)                                               │
└─────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

`a2ui-bevy` 依赖 [`a2ui-base`](https://crates.io/crates/a2ui-base);被 `a2ui-bevy-gallery` 与(在 `bevy` feature 下的)umbrella `a2ui` 依赖。

## 构建

一切代码都在 `backend` cargo feature 之后,它才拉入 Bevy 运行时 + `bevy_ui_text_input`。不带该 feature 时,本 crate 只是个空壳(除 `a2ui-base` 外无依赖),保持 workspace 默认构建轻量。

```bash
cargo build -p a2ui-bevy --features backend
```

渲染器使用 wgpu,需要 GPU/wgpu 栈但无需游戏专用工具链。

## 示例

`crates/bevy/examples/17_scifi_hud.rs` 是 ratatui 版 [`17_scifi_hud`](../a2ui/examples/17_scifi_hud.rs) 的 Bevy 对应版:同一份数据、同样的「data model 是唯一真源」架构,换用 Bevy 原生 UI 渲染。它不走本 crate 的 reconciler —— **布局就是 Bevy 实体树**:HUD 实体树在 `Startup` 里 spawn 一次,每帧由一个 `Update` system 从 data model 读出最新值、原位 mutate 已有实体(`Text` / `Node.width` / `BackgroundColor` / `TextColor`)。这正是保留式 ECS 的长处:实体身份跨帧保持,无需每帧重建、无需状态桥。仪表用 flex 条、雷达用 ASCII 字符网格(呼应 ratatui 原版;Bevy UI 无 canvas)。动画由一个 ~80 ms 的 `Timer` 资源推进 `tick` system(等价于 ratatui 版的 `event::poll`)。

```bash
cargo run -p a2ui-bevy --example 17_scifi_hud --features backend
```

> 截图由 [`scripts/capture_bevy_screenshot.sh`](../scripts/capture_bevy_screenshot.sh) 产生。锁定的 GNOME Wayland 下桌面截图工具不可用(`org.gnome.Shell.Screenshot` D-Bus 被拒、X11 工具看不见 Wayland 原生窗口),故示例内置一个 env(`A2UI_SCREENSHOT_PATH`)触发的自截图模式:直接读窗口渲染目标(`Screenshot::primary_window()` + `save_to_disk`),与合成器无关,然后退出。

## Reconciler(实现要点)

Bevy 的可交互控件只有当其实体逐帧存活时才能正确工作 —— 每帧重建(Slint / egui 的做法)会让滑块每帧乱跳、文本光标每帧丢失。所以 reconciler 针对 `A2uiState` 中稳定的 `node_map: HashMap<component_id, Entity>` 做两遍 diff/patch:

1. **Plan**(对 A2UI model 的只读遍历)—— 为每个应当存在的组件收集一个 `PlanNode`:类型、解析后的字段、父节点,以及它挂在哪个根下(surface 还是 overlay)。
2. **Apply**(对 `node_map` + `Commands` 的可变遍历)—— spawn 新实体、despawn 被移除的实体、重新挂载 / 重排,并调用 `render` 中幂等的 `apply_*` 更新器以镜像解析后的值。

这是 egui 每帧重建 + `EditBuffers` 桥接的保留式对应物:身份由实体 map 保持,而不是每帧重新播种。

渲染循环每帧以 Bevy system 的形式运行:`collect_interactions`(控件的 `EntityEvent` + 文本输入 diff → `PendingInteraction`)→ `apply_interactions`(通过共享 core 流水线 mutate `MessageProcessor`,标记树为脏)→ `reconcile`(diff/patch 实体树)。

## 模块

| 模块 | 职责 |
|------|------|
| `reconcile` | React 风格 diff/patch —— 维护稳定的 `component_id → Entity` map,spawn / update / despawn / reorder 使存活树镜像 model |
| `render` | 按组件类型幂等的更新器 —— 重新应用 Bevy 组件以镜像解析后的 A2UI 值 |
| `interaction` | 把 `bevy_ui_widgets` 的 `EntityEvent` + 文本输入 diff 映射为 `PendingInteraction`,再通过共享 core 流水线应用 |
| `plugin` | `A2uiPlugin` —— 注册渲染循环 system + observer,生成基础 UI |
| `state` | `A2uiState`(`NonSend` 资源)—— 持有 processor、函数 map、focus、open-modals 以及 `node_map` |
| `sample_browser` | 左侧样例列表;点击某行切换加载的样例 |

## 许可证

MIT
