//! Persistent per-component edit buffers — the immediate-mode state bridge.
//!
//! ## The problem
//!
//! egui widgets need a stable `&mut` to a buffer each frame (to retain cursor /
//! scroll state, and to detect value changes frame-to-frame). But A2UI component
//! values live in the **data model**, resolved fresh each frame via
//! [`DataContext`](a2ui_base::model::data_context::DataContext). There is no
//! stable `&mut String` inside the data model we can hand to egui — and the data
//! model is the source of truth (downstream `DynamicString` bindings re-resolve
//! from it).
//!
//! ## The bridge
//!
//! [`EditBuffers`] is a set of `HashMap<component_id, value>` owned by
//! [`crate::EguiApp`], persisting across frames. Each frame, for each interactive
//! widget:
//!
//! 1. **Seed** the buffer from the resolved data-model value if it is *stale*
//!    (not yet present, or the data model changed externally — e.g. a server
//!    message wrote a new value).
//! 2. Hand `&mut buf` to the egui widget.
//! 3. **Detect** whether the widget changed the buffer.
//! 4. If changed, emit a
//!    [`PendingInteraction::DataUpdate`](crate::interaction::PendingInteraction::DataUpdate)
//!    so the app writes it back after the walk.
//!
//! [`begin_frame`] clears the per-frame bookkeeping (`synced_this_frame`) but
//! **keeps** the buffers; [`invalidate`] (on sample switch) bumps `generation`
//! and drops everything so stale buffers from the previous sample don't leak in.
//!
//! ## The seed guard
//!
//! When a TextField is focused and being edited, its buffer is *authoritative*
//! (the write-back hasn't happened yet — it's pending, applied post-walk). We
//! must not re-seed it from the data model mid-edit, or the user's un-committed
//! typing would be clobbered. The guard: once a buffer is seeded this frame,
//! it's marked in `synced_this_frame`, and a focused component is never re-seeded
//! if its buffer already exists.

use std::collections::{HashMap, HashSet};

/// Persistent edit buffers for interactive components.
///
/// See the module docs for the seed → widget → detect → writeback lifecycle.
#[derive(Default)]
pub struct EditBuffers {
    /// TextField edit text, keyed by component id.
    text: HashMap<String, String>,
    /// Slider value, keyed by component id.
    number: HashMap<String, f64>,
    /// CheckBox value, keyed by component id.
    boolean: HashMap<String, bool>,
    /// ChoicePicker selected value, keyed by component id.
    choice: HashMap<String, String>,
    /// Component ids whose buffer was seeded from the data model this frame —
    /// guards against re-seeding our own just-written value.
    synced_this_frame: HashSet<String>,
    /// Bumped on sample switch / reset; see [`Self::invalidate`].
    generation: u64,
}

impl EditBuffers {
    /// Clear per-frame bookkeeping at the start of a frame. Buffers persist.
    pub fn begin_frame(&mut self) {
        self.synced_this_frame.clear();
    }

    /// Drop all buffers (sample switch / processor reset). Old buffers from the
    /// previous sample would otherwise shadow the new sample's values.
    pub fn invalidate(&mut self) {
        self.text.clear();
        self.number.clear();
        self.boolean.clear();
        self.choice.clear();
        self.synced_this_frame.clear();
        self.generation = self.generation.wrapping_add(1);
    }

    // -- TextField -------------------------------------------------------

    /// Borrow-or-seed the text buffer for `id`. The buffer is seeded from
    /// `resolved` only when stale (absent, or not yet synced this frame and not
    /// focused — see the seed guard in the module docs).
    ///
    /// Returns the previous resolved value (the value as it stood *before* this
    /// frame's edit, or `resolved` itself for a freshly seeded buffer) so the
    /// caller can detect a change.
    pub fn text_buffer(&mut self, id: &str, resolved: &str, focused: bool) -> &mut String {
        if !self.text.contains_key(id) {
            self.text.insert(id.to_string(), resolved.to_string());
            self.synced_this_frame.insert(id.to_string());
        } else if !focused && !self.synced_this_frame.contains(id) {
            // Buffer exists but may be stale relative to an external data-model
            // change. Re-seed unless the component is focused (authoritative).
            self.text.insert(id.to_string(), resolved.to_string());
            self.synced_this_frame.insert(id.to_string());
        }
        self.text.get_mut(id).expect("just inserted/ensured")
    }

    // -- Slider ----------------------------------------------------------

    /// Borrow-or-seed the number buffer for `id`. Same staleness guard as text.
    pub fn number_buffer(&mut self, id: &str, resolved: f64) -> &mut f64 {
        if !self.number.contains_key(id) || !self.synced_this_frame.contains(id) {
            self.number.insert(id.to_string(), resolved);
            self.synced_this_frame.insert(id.to_string());
        }
        self.number.get_mut(id).expect("just inserted/ensured")
    }

    // -- CheckBox --------------------------------------------------------

    /// Borrow-or-seed the boolean buffer for `id`.
    pub fn boolean_buffer(&mut self, id: &str, resolved: bool) -> &mut bool {
        if !self.boolean.contains_key(id) || !self.synced_this_frame.contains(id) {
            self.boolean.insert(id.to_string(), resolved);
            self.synced_this_frame.insert(id.to_string());
        }
        self.boolean.get_mut(id).expect("just inserted/ensured")
    }

    // -- ChoicePicker ----------------------------------------------------

    /// Borrow-or-seed the choice buffer for `id`.
    pub fn choice_buffer(&mut self, id: &str, resolved: &str) -> &mut String {
        if !self.choice.contains_key(id) || !self.synced_this_frame.contains(id) {
            self.choice.insert(id.to_string(), resolved.to_string());
            self.synced_this_frame.insert(id.to_string());
        }
        self.choice.get_mut(id).expect("just inserted/ensured")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_buffer_seeds_from_resolved_when_absent() {
        let mut eb = EditBuffers::default();
        let buf = eb.text_buffer("f1", "hello", false);
        assert_eq!(buf, "hello");
    }

    #[test]
    fn text_buffer_keeps_buffer_across_frames_until_externally_changed() {
        let mut eb = EditBuffers::default();
        // Frame 1: seed.
        eb.begin_frame();
        *eb.text_buffer("f1", "hello", false) = "world".to_string();
        // Frame 2: buffer exists, focused → keep "world" even though resolved
        // still says "hello" (the write-back is pending / external).
        eb.begin_frame();
        let buf = eb.text_buffer("f1", "hello", true);
        assert_eq!(buf, "world", "focused component keeps its buffer");
    }

    #[test]
    fn text_buffer_reseeds_when_not_focused_and_unsynced() {
        let mut eb = EditBuffers::default();
        eb.begin_frame();
        *eb.text_buffer("f1", "hello", false) = "world".to_string();
        // Frame 2: not focused and unsynced → re-seed from the (now changed)
        // resolved value, simulating an external data-model update.
        eb.begin_frame();
        let buf = eb.text_buffer("f1", "changed", false);
        assert_eq!(buf, "changed");
    }

    #[test]
    fn number_and_boolean_seed() {
        let mut eb = EditBuffers::default();
        eb.begin_frame();
        let n = eb.number_buffer("s", 7.0);
        assert!((*n - 7.0).abs() < 1e-9);
        eb.begin_frame();
        let b = eb.boolean_buffer("c", true);
        assert!(*b);
    }

    #[test]
    fn invalidate_drops_all_buffers() {
        let mut eb = EditBuffers::default();
        eb.begin_frame();
        *eb.text_buffer("f1", "hello", false) = "x".to_string();
        eb.invalidate();
        // After invalidate, a new seed happens (buffer gone).
        let buf = eb.text_buffer("f1", "fresh", false);
        assert_eq!(buf, "fresh");
    }
}
