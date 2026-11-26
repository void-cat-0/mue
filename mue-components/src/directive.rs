//! Reusable behavior attached to any styleable builder (Vue's custom
//! directives).
//!
//! A [`Directive`] adds hooks or styles to a node. Attach one with
//! [`WithDirective::directive`]. Built-in directives: [`Hotkey`],
//! [`ClickOutside`], [`LongPressRepeat`], [`Haptic`], and [`Sound`].
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_macroquad::event::keyboard::KeyCode;
//!
//! let _editor = flexbox()
//!     .focusable(true)
//!     .directive(Hotkey::command(KeyCode::S, || println!("save")));
//! ```

use std::{cell::RefCell, rc::Rc};

use mue_core::{prop::Prop, signal::Access};
use mue_macroquad::{
    event::{
        keyboard::{KeyCode, KeyEvent},
        pointer::{PointerAction, use_pointer_down_observer},
    },
    feedback::{self, HapticStyle, SoundCue},
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

/// Run an action on press, then repeatedly while the press is held (stepper
/// buttons, scroll arrows). The first repeat follows `delay` seconds after
/// the press and the rest come every `interval` seconds, speeding up to
/// twice as fast over the first second and a half of repeating.
///
/// ```
/// use mue_components::prelude::*;
/// use mue_core::prelude::*;
///
/// # let owner = mue_core::scope::OwnedScope::new();
/// # owner.run(|| {
/// let count = signal(0);
/// let _plus = button("+").directive(LongPressRepeat::new(move || count.update(|n| *n += 1)));
/// # });
/// ```
pub struct LongPressRepeat<F> {
    action: F,
    repeater: Repeater,
}

impl<F: FnMut() + 'static> LongPressRepeat<F> {
    /// Repeat after 0.4 s, every 0.1 s.
    pub fn new(action: F) -> Self {
        Self {
            action,
            repeater: Repeater::new(0.4, 0.1),
        }
    }

    pub fn delay(mut self, seconds: f32) -> Self {
        self.repeater.delay = seconds.max(0.);
        self
    }

    pub fn interval(mut self, seconds: f32) -> Self {
        self.repeater.interval = seconds.max(0.01);
        self
    }
}

impl<F: FnMut() + 'static> Directive for LongPressRepeat<F> {
    fn apply(self, style: &mut Style) {
        let action = Rc::new(RefCell::new(self.action));
        let repeater = Rc::new(RefCell::new(self.repeater));
        style.on_pointer_event.append({
            let action = action.clone();
            let repeater = repeater.clone();
            move |event| match event.action() {
                PointerAction::Down => {
                    repeater.borrow_mut().press();
                    (action.borrow_mut())();
                }
                PointerAction::Up | PointerAction::Cancel => repeater.borrow_mut().release(),
                PointerAction::Move => {}
            }
        });
        style.on_update.append(move |dt| {
            let fires = repeater.borrow_mut().advance(*dt);
            for _ in 0..fires {
                (action.borrow_mut())();
            }
        });
    }
}

/// Timing for [`LongPressRepeat`].
#[derive(Clone, Debug)]
struct Repeater {
    delay: f32,
    interval: f32,
    /// Seconds since the press, while held.
    held: Option<f32>,
    /// When the next repeat fires, in seconds since the press.
    next: f32,
}

impl Repeater {
    fn new(delay: f32, interval: f32) -> Self {
        Self {
            delay,
            interval,
            held: None,
            next: 0.,
        }
    }

    fn press(&mut self) {
        self.held = Some(0.);
        self.next = self.delay;
    }

    fn release(&mut self) {
        self.held = None;
    }

    /// Advance by `dt` seconds and return how many repeats are due.
    fn advance(&mut self, dt: f32) -> u32 {
        let Some(held) = self.held.as_mut() else {
            return 0;
        };
        *held += dt.max(0.);
        let held = *held;
        let mut fires = 0;
        while held >= self.next - 1e-4 {
            if fires == 2 {
                // A long frame (or a stall) must not release a burst of
                // repeats: skip the backlog.
                self.next = held + self.interval / 2.;
                break;
            }
            fires += 1;
            let repeating = (self.next - self.delay).max(0.);
            let speedup = 1. + (repeating / 1.5).min(1.);
            self.next += self.interval / speedup;
        }
        fires
    }
}

/// When a feedback directive plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackTrigger {
    /// The pointer goes down on the node.
    Press,
    /// A completed tap (the default).
    Tap,
    LongPress,
}

/// Play a haptic pattern when the node is tapped, pressed, or long-pressed.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _like = button("Like").directive(Haptic::new(HapticStyle::Light));
/// let _menu = flexbox().directive(Haptic::on(FeedbackTrigger::LongPress, HapticStyle::Heavy));
/// ```
pub struct Haptic {
    trigger: FeedbackTrigger,
    style: HapticStyle,
}

impl Haptic {
    pub fn new(style: HapticStyle) -> Self {
        Self::on(FeedbackTrigger::Tap, style)
    }

    pub fn on(trigger: FeedbackTrigger, style: HapticStyle) -> Self {
        Self { trigger, style }
    }
}

impl Directive for Haptic {
    fn apply(self, style: &mut Style) {
        let haptic = self.style;
        on_trigger(style, self.trigger, move || feedback::haptic(haptic));
    }
}

/// Play a UI sound (through the app's [`feedback::set_sound_handler`]) when
/// the node is tapped, pressed, or long-pressed.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _save = button("Save").directive(Sound::new(SoundCue::Click));
/// ```
pub struct Sound {
    trigger: FeedbackTrigger,
    cue: SoundCue,
}

impl Sound {
    pub fn new(cue: SoundCue) -> Self {
        Self::on(FeedbackTrigger::Tap, cue)
    }

    pub fn on(trigger: FeedbackTrigger, cue: SoundCue) -> Self {
        Self { trigger, cue }
    }
}

impl Directive for Sound {
    fn apply(self, style: &mut Style) {
        let cue = self.cue;
        on_trigger(style, self.trigger, move || feedback::sound(cue));
    }
}

fn on_trigger(style: &mut Style, trigger: FeedbackTrigger, play: impl Fn() + 'static) {
    match trigger {
        FeedbackTrigger::Press => style.on_pointer_event.append(move |event| {
            if event.action() == PointerAction::Down {
                play();
            }
        }),
        FeedbackTrigger::Tap => style.on_tap.append(move |_| play()),
        FeedbackTrigger::LongPress => style.on_long_press.append(move |_| play()),
    }
}
