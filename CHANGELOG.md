## [0.2.1] - 2026-06-15

### 🚀 Features

- *(core)* Add validate module ported from Python SDK

### 🐛 Bug Fixes

- *(workspace)* Add version to internal path deps for publish
- *(slint)* Cfg-gate build.rs so the crate publishes without `backend`

### 🚜 Refactor

- Rename a2ui-core -> a2ui-base (name taken on crates.io)

### 📚 Documentation

- Refresh READMEs for the dual-backend reality
## [0.2.0] - 2026-06-14

### 🚀 Features

- *(gallery)* Numbered sample rows + panel-focus navigation
- *(slint)* Add Slint backend (a2ui-slint) + gallery binary
- *(slint)* Render all 18 component kinds (P7)
- *(slint-gallery)* Add left-hand sample browser sidebar

### 🐛 Bug Fixes

- *(tui)* Render data-templated children (componentId + base_path)

### 💼 Other

- *(workspace)* Restore `cargo install a2ui` bin + workspace release config

### 🚜 Refactor

- Split single crate into a 4-crate Cargo workspace
- *(core)* Lift FocusManager + interaction + 4 handle_events into core

### 📚 Documentation

- Update READMEs for the workspace split
- Document the Slint desktop backend in both READMEs

### ⚙️ Miscellaneous Tasks

- Publish a2ui-slint + a2ui-slint-gallery in dependency order
- Release

### ◀️ Revert

- *(umbrella)* Drop the a2ui bin to keep umbrella a pure re-export lib
## [0.1.2] - 2026-06-13

### 🚀 Features

- Implement callFunction & actionResponse protocol handlers
- Add agent chat TUI example & update render/focus/event APIs
- *(ratatui-css)* Add examples and complete the builder API
- *(ratatui-css)* Add live TUI render examples
- Complete A2UI spec compliance — 8 phases of improvements
- *(image)* Auto-degrading graphics protocols + always-on rendering
- *(core)* Capabilities negotiation, inline catalogs, generic fallback
- *(tui)* Interactive DateTimeInput & AudioPlayer, media assets
- *(tui)* Add interactive DateTimeInput example (15_date_time_input)
- *(tui)* Add custom component example (16_custom_component)
- *(tui)* Intrinsic-size (measure-pass) layout
- *(tui)* Add a2ui-driven sci-fi HUD example (17_scifi_hud)
- *(gallery)* Embed spec into binary + size-optimize release

### 🐛 Bug Fixes

- *(gallery)* Stop stderr pollution corrupting the TUI
- *(tui)* Height auto-adaptation + example interaction fixes

### 📚 Documentation

- Update CHANGELOG for v0.1.2
- Add basic examples for a2ui library
- Add crates.io/docs.rs/license badges and MIT LICENSE
- Add Agent Chat screenshot to READMEs
- Add Invitation Builder screenshot to READMEs
- Refresh login-form and invitation-builder screenshots

### ⚙️ Miscellaneous Tasks

- Add a2ui/ spec directory to .gitignore
- Extract ratatui-css into a standalone crate (ratatui-style)
- *(justfile)* Add cargo publish recipe
- Update AGENTS.md project conventions
- Restore a2ui spec folder
- Release a2ui version 0.1.2
## [0.1.1] - 2026-06-12

### 🚀 Features

- Implement A2UI v1.0 TUI renderer - Phase 1-3 complete
- Implement Basic Catalog - Phase 4 complete
- Add gallery rendered mode keyboard navigation and screenshots to README

### 📚 Documentation

- Add Chinese and English README

### ⚙️ Miscellaneous Tasks

- Init
- Add crates.io metadata and implement component registry
- Release a2ui version 0.1.1
