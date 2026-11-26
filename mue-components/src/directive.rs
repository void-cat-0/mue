//! Reusable behavior attached to any styleable builder (Vue's custom
//! directives).
//!
//! A [`Directive`] adds hooks or styles to a node. Attach one with
//! [`WithDirective::directive`]. Built-in directives: [`Hotkey`] and
//! [`ClickOutside`].
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_macroquad::event::keyboard::KeyCode;
//!
//! let _editor = flexbox()
//!     .focusable(true)
//!     .directive(Hotkey::command(KeyCode::S, || println!("save")));
//! ```

use mue_core::{prop::Prop, signal::Access};
use mue_macroquad::{
    event::{
        keyboard::{KeyCode, KeyEvent},
        pointer::use_pointer_down_observer,
    },
    math::Vector,
    node::{NodeHandle, NodeRef},
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

/// Run an action when a press lands outside the node (Vue's
/// `v-click-outside`), for dismissing menus and popovers.
///
/// A press counts as inside when it hits the node or one of its descendants,
/// or lands within the node's bounds with nothing else on top. Presses are
/// observed before they are dispatched, so the action runs even when the
/// press then lands on another control.
///
/// ```
/// use mue_components::prelude::*;
/// use mue_core::prelude::*;
///
/// # let owner = mue_core::scope::OwnedScope::new();
/// # owner.run(|| {
/// let open = signal(true);
/// let _menu = flexbox()
///     .directive(ClickOutside::new(move || open.set(false)).enabled(open));
/// # });
/// ```
pub struct ClickOutside<F> {
    action: F,
    enabled: Prop<bool>,
}

impl<F: FnMut() + 'static> ClickOutside<F> {
    pub fn new(action: F) -> Self {
        Self {
            action,
            enabled: Prop::Static(true),
        }
    }

    /// Only watch presses while `enabled` (for example while a menu is open).
    pub fn enabled(mut self, enabled: impl Into<Prop<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }
}

impl<F: FnMut() + 'static> Directive for ClickOutside<F> {
    fn apply(self, style: &mut Style) {
        let Self {
            mut action,
            enabled,
        } = self;
        let handle = NodeHandle::new();
        style.bind_node_handle(handle);
        use_pointer_down_observer(move |position, path| {
            if !enabled.get_untracked() {
                return;
            }
            let Some(node) = handle.get_untracked() else {
                return;
            };
            if !press_is_inside(node, position, path.iter()) {
                action();
            }
        });
    }
}

fn press_is_inside<'a>(
    node: NodeRef,
    position: Vector,
    path: impl Iterator<Item = &'a NodeRef> + Clone,
) -> bool {
    if path.clone().any(|hit| node.contains(*hit)) {
        return true;
    }
    // A non-interactive node is not in the hit path; fall back to its bounds
    // when only its ancestors were hit (nothing covers it there).
    let within = node
        .rect()
        .and_then(|rect| rect.try_get_untracked())
        .is_some_and(|rect| rect.contains(&position));
    within && path.clone().all(|hit| hit.contains(node))
}
