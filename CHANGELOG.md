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
