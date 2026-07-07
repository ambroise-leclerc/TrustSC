//! Bounded outbound event plane (ADR-015): the mirror image of
//! [`crate::realtime::FrameInputs`]. Where `FrameInputs` carries application data one way *into*
//! the frame, [`FrameEvents`] carries operator interaction one way *out* of it — by data, not by
//! callback. The adapter fills the queue from hit-tested platform events; the application drains
//! it exactly once per frame and applies what it drained to its own state (for text entry,
//! typically through [`TextInputModel`]), then echoes the result back through
//! `FrameInputs::set_text`. The renderer stores nothing.
//!
//! Everything is sized at construction: the queue never reallocates, and on overflow the newest
//! event is dropped and counted — a visible, auditable fact, never a silent one.

use mdux_ui::SystemEvent;

/// Default [`FrameEvents`] capacity: comfortably above what one frame of human interaction can
/// produce, small enough that the queue stays cache-resident.
pub const DEFAULT_FRAME_EVENT_CAPACITY: usize = 64;

/// One operator interaction, delivered to the application by `source` key (the same keying
/// discipline as `FrameInputs`). `CaretMoved` carries the *target* caret position; consumers
/// clamp it to their buffer length, so `Home`/`End` need no dedicated variants (position `0` /
/// `u16::MAX`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WidgetEvent {
    /// A `Button` was pressed and released inside its bounds.
    ButtonPressed { source: &'static str },
    /// A `CriticalButton` was pressed. `TriggerHalt` is dispatched by the framework itself
    /// (audited orderly shutdown); the application only ever observes `NoOp` here.
    CriticalButtonPressed {
        node_id: &'static str,
        action: SystemEvent,
    },
    /// A printable character was typed into the focused `TextInput`.
    CharTyped {
        source: &'static str,
        character: char,
    },
    /// Backspace in the focused `TextInput`: remove the character before the caret.
    Backspace { source: &'static str },
    /// Delete in the focused `TextInput`: remove the character at the caret.
    Delete { source: &'static str },
    /// The caret of the focused `TextInput` should move to `position` (clamped by the consumer).
    CaretMoved { source: &'static str, position: u16 },
    /// Entry in the focused `TextInput` was committed (Enter).
    TextCommitted { source: &'static str },
    /// Focus moved to `source`, or cleared (`None`).
    FocusChanged { source: Option<&'static str> },
}

/// The bounded event queue. Capacity is allocated once at construction and never grows; pushing
/// into a full queue drops the incoming event and increments a saturating counter that the
/// diagnostics surface, so a dropped burst is evidence rather than a mystery.
#[derive(Clone, Debug)]
pub struct FrameEvents {
    events: Vec<WidgetEvent>,
    capacity: usize,
    dropped: u32,
}

impl FrameEvents {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_FRAME_EVENT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            events: Vec::with_capacity(capacity),
            capacity,
            dropped: 0,
        }
    }

    /// Queues `event`, or drops it (returning `false` and counting the drop) when the queue is
    /// full. Drop-newest is deliberate: under a burst, the interactions already accepted keep
    /// their order and meaning.
    pub fn push(&mut self, event: WidgetEvent) -> bool {
        if self.events.len() == self.capacity {
            self.dropped = self.dropped.saturating_add(1);
            return false;
        }
        self.events.push(event);
        true
    }

    /// Drains every queued event in arrival order. Called by the application closure once per
    /// frame; the drop counter is *not* reset — it accumulates for the diagnostics.
    pub fn drain(&mut self) -> std::vec::Drain<'_, WidgetEvent> {
        self.events.drain(..)
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Total events dropped to overflow since construction (saturating).
    pub fn dropped_events(&self) -> u32 {
        self.dropped
    }
}

impl Default for FrameEvents {
    fn default() -> Self {
        Self::new()
    }
}

/// Bounded editing model for one `TextInput` source: the application-owned buffer of the
/// controlled component (ADR-015). Full caret editing from the first version — insert-at-caret,
/// backspace, delete, and absolute caret moves (arrows/Home/End arrive as [`WidgetEvent::CaretMoved`]).
/// The buffer is reserved at construction and never reallocates; `max_length` is enforced here
/// and re-enforced (with the charset) at the `FrameInputs::set_text` boundary.
#[derive(Clone, Debug)]
pub struct TextInputModel {
    source: &'static str,
    buffer: String,
    /// Caret as a character index, `0..=len`.
    caret: usize,
    max_length: usize,
}

impl TextInputModel {
    pub fn new(source: &'static str, max_length: u16) -> Self {
        let max_length = usize::from(max_length);
        Self {
            source,
            // Reserved for the worst UTF-8 case so insertions never reallocate.
            buffer: String::with_capacity(max_length * 4),
            caret: 0,
            max_length,
        }
    }

    pub fn source(&self) -> &'static str {
        self.source
    }

    pub fn as_str(&self) -> &str {
        &self.buffer
    }

    /// Caret position as a character index.
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// Current length in characters.
    pub fn len(&self) -> usize {
        self.buffer.chars().count()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn max_length(&self) -> usize {
        self.max_length
    }

    /// Applies one drained event. Events for other sources and non-editing events are ignored.
    /// Returns `true` when the model's state (buffer or caret) changed.
    pub fn apply(&mut self, event: &WidgetEvent) -> bool {
        match *event {
            WidgetEvent::CharTyped { source, character } if source == self.source => {
                if self.len() >= self.max_length {
                    return false;
                }
                let at = self.byte_index(self.caret);
                self.buffer.insert(at, character);
                self.caret += 1;
                true
            }
            WidgetEvent::Backspace { source } if source == self.source => {
                if self.caret == 0 {
                    return false;
                }
                let at = self.byte_index(self.caret - 1);
                self.buffer.remove(at);
                self.caret -= 1;
                true
            }
            WidgetEvent::Delete { source } if source == self.source => {
                if self.caret >= self.len() {
                    return false;
                }
                let at = self.byte_index(self.caret);
                self.buffer.remove(at);
                true
            }
            WidgetEvent::CaretMoved { source, position } if source == self.source => {
                let clamped = usize::from(position).min(self.len());
                if clamped == self.caret {
                    return false;
                }
                self.caret = clamped;
                true
            }
            _ => false,
        }
    }

    fn byte_index(&self, char_index: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(char_index)
            .map(|(index, _)| index)
            .unwrap_or(self.buffer.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_events_drop_newest_on_overflow_and_count_saturating() {
        let mut events = FrameEvents::with_capacity(2);
        assert!(events.push(WidgetEvent::ButtonPressed { source: "A" }));
        assert!(events.push(WidgetEvent::ButtonPressed { source: "B" }));
        assert!(!events.push(WidgetEvent::ButtonPressed { source: "C" }));
        assert_eq!(events.len(), 2);
        assert_eq!(events.dropped_events(), 1);

        // The accepted events keep arrival order; the overflow victim is the newest.
        let drained: Vec<_> = events.drain().collect();
        assert_eq!(
            drained,
            vec![
                WidgetEvent::ButtonPressed { source: "A" },
                WidgetEvent::ButtonPressed { source: "B" },
            ]
        );

        // Draining frees capacity but never resets the audit counter.
        assert!(events.is_empty());
        assert_eq!(events.dropped_events(), 1);
        assert!(events.push(WidgetEvent::TextCommitted { source: "A" }));
    }

    #[test]
    fn text_input_model_edits_at_the_caret_within_bounds() {
        let mut model = TextInputModel::new("PATIENT_ID", 4);

        for character in ['A', 'B', 'D'] {
            assert!(model.apply(&WidgetEvent::CharTyped { source: "PATIENT_ID", character }));
        }
        assert_eq!(model.as_str(), "ABD");
        assert_eq!(model.caret(), 3);

        // Move the caret back and insert: caret editing, not append-only.
        assert!(model.apply(&WidgetEvent::CaretMoved { source: "PATIENT_ID", position: 2 }));
        assert!(model.apply(&WidgetEvent::CharTyped { source: "PATIENT_ID", character: 'C' }));
        assert_eq!(model.as_str(), "ABCD");
        assert_eq!(model.caret(), 3);

        // The buffer is full: further insertions are refused.
        assert!(!model.apply(&WidgetEvent::CharTyped { source: "PATIENT_ID", character: 'E' }));
        assert_eq!(model.as_str(), "ABCD");

        // Delete removes at the caret, backspace before it.
        assert!(model.apply(&WidgetEvent::Delete { source: "PATIENT_ID" }));
        assert_eq!(model.as_str(), "ABC");
        assert!(model.apply(&WidgetEvent::Backspace { source: "PATIENT_ID" }));
        assert_eq!(model.as_str(), "AB");
        assert_eq!(model.caret(), 2);

        // Home / End arrive as absolute positions, clamped to the buffer length.
        assert!(model.apply(&WidgetEvent::CaretMoved { source: "PATIENT_ID", position: 0 }));
        assert_eq!(model.caret(), 0);
        assert!(!model.apply(&WidgetEvent::Backspace { source: "PATIENT_ID" }));
        assert!(model.apply(&WidgetEvent::CaretMoved {
            source: "PATIENT_ID",
            position: u16::MAX,
        }));
        assert_eq!(model.caret(), 2);
        assert!(!model.apply(&WidgetEvent::Delete { source: "PATIENT_ID" }));
    }

    #[test]
    fn text_input_model_ignores_other_sources_and_non_editing_events() {
        let mut model = TextInputModel::new("PATIENT_ID", 8);

        assert!(!model.apply(&WidgetEvent::CharTyped { source: "OTHER", character: 'X' }));
        assert!(!model.apply(&WidgetEvent::ButtonPressed { source: "PATIENT_ID" }));
        assert!(!model.apply(&WidgetEvent::TextCommitted { source: "PATIENT_ID" }));
        assert!(!model.apply(&WidgetEvent::FocusChanged { source: Some("PATIENT_ID") }));
        assert!(model.is_empty());
        assert_eq!(model.caret(), 0);
    }
}
