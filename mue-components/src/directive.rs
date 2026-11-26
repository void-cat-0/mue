//! Reusable behavior attached to any styleable builder (Vue's custom
//! directives).
//!
//! A [`Directive`] adds hooks or styles to a node. Attach one with
//! [`WithDirective::directive`]. [`Hotkey`] is a built-in directive.
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_macroquad::event::keyboard::KeyCode;
//!
//! let _editor = flexbox()
//!     .focusable(true)
//!     .directive(Hotkey::command(KeyCode::S, || println!("save")));
//! ```

use mue_macroquad::{
    event::keyboard::{KeyCode, KeyEvent},
    style::{Style, Styleable},
};

/// Behavior that configures a node's style and hooks.
pub trait Directive {
    fn apply(self, style: &mut Style);
}

/// Attach directives to any builder.
pub trait WithDirective: Styleable {
    fn directive(mut self, directive: impl Directive) -> Self {
        directive.apply(self.style_mut());
        self
    }
}

impl<T: Styleable> WithDirective for T {}

/// Run an action when a key chord reaches the node through the focus path
/// (the node or a focused descendant). The event stops propagating when the
/// chord matches. Key repeats are ignored.
pub struct Hotkey<F> {
    key: KeyCode,
    command: bool,
    shift: bool,
    alt: bool,
    action: F,
}

impl<F: FnMut() + 'static> Hotkey<F> {
    /// The key alone.
    pub fn new(key: KeyCode, action: F) -> Self {
        Self {
            key,
            command: false,
            shift: false,
            alt: false,
            action,
        }
    }

    /// Ctrl+key (Cmd+key on macOS).
    pub fn command(key: KeyCode, action: F) -> Self {
        Self {
            command: true,
            ..Self::new(key, action)
        }
    }

    pub fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    pub fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    fn matches(&self, event: &KeyEvent) -> bool {
        event.key == self.key
            && !event.repeat
            && event.command() == self.command
            && event.modifiers.shift == self.shift
            && event.modifiers.alt == self.alt
    }
}

impl<F: FnMut() + 'static> Directive for Hotkey<F> {
    fn apply(mut self, style: &mut Style) {
        style.on_key_down.append(move |event| {
            if self.matches(event) {
                event.stop_propagation();
                (self.action)();
            }
        });
    }
}
