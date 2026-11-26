//! Keyboard, text, and focus events delivered along the focus path.
//!
//! Key and character events go to the focused node first, then bubble to its
//! ancestors until a handler calls [`KeyEvent::stop_propagation`]. With no
//! focused node they go to the displayed roots, topmost first, so a root
//! handler works as a global shortcut. See [`crate::input::focus`].

use std::cell::Cell;

pub use miniquad::{KeyCode, KeyMods};

/// A key press or release.
#[derive(Debug, Clone)]
pub struct KeyEvent {
    pub key: KeyCode,
    pub modifiers: KeyMods,
    /// The platform repeated a held key. Always false for releases.
    pub repeat: bool,
    handled: Cell<bool>,
}

impl KeyEvent {
    pub fn new(key: KeyCode, modifiers: KeyMods, repeat: bool) -> Self {
        Self {
            key,
            modifiers,
            repeat,
            handled: Cell::new(false),
        }
    }

    /// Stop bubbling to ancestors and skip the default action (such as Tab
    /// moving focus).
    pub fn stop_propagation(&self) {
        self.handled.set(true);
    }

    pub fn is_handled(&self) -> bool {
        self.handled.get()
    }

    /// Whether Ctrl (or Cmd on macOS) is held, the usual shortcut modifier.
    pub fn command(&self) -> bool {
        if cfg!(target_os = "macos") {
            self.modifiers.logo
        } else {
            self.modifiers.ctrl
        }
    }
}

/// A character produced by text input.
#[derive(Debug, Clone)]
pub struct CharEvent {
    pub character: char,
    pub modifiers: KeyMods,
    pub repeat: bool,
    handled: Cell<bool>,
}

impl CharEvent {
    pub fn new(character: char, modifiers: KeyMods, repeat: bool) -> Self {
        Self {
            character,
            modifiers,
            repeat,
            handled: Cell::new(false),
        }
    }

    pub fn stop_propagation(&self) {
        self.handled.set(true);
    }

    pub fn is_handled(&self) -> bool {
        self.handled.get()
    }
}

/// What moved focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusReason {
    /// A pointer press on a focusable node, or outside any (blur).
    Pointer,
    /// Tab, Shift+Tab, or another keyboard navigation.
    Keyboard,
    /// A call such as [`NodeRef::focus`](crate::node::NodeRef::focus) or auto focus.
    Programmatic,
}

/// Delivered by `on_focus` and `on_blur`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusEvent {
    pub reason: FocusReason,
    /// Whether the focus should be shown with a focus ring (keyboard modality).
    pub visible: bool,
}
