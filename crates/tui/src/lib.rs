pub mod surface;
pub mod component_impl;
pub mod layout_engine;
pub mod focus_manager;
pub mod components;
pub mod catalogs;
pub mod interaction;
// Shared "agent chat" scenario builders (mock AI agent → A2UI protocol message
// streams). Framework-agnostic JSON, so every UI backend's `08_agent_chat`
// example imports this instead of duplicating the scenarios. Lives here, next to
// `catalogs::basic`, because that catalog builder is likewise shared by every
// backend example.
pub mod agent_chat;
