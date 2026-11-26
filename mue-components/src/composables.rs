//! Reusable reactive helpers (Vue's composables): call them in a component
//! body and use the returned signals and methods.
//!
//! ```
//! use mue_components::prelude::*;
//!
//! #[component]
//! fn details() -> impl Component {
//!     let section = use_disclosure(false);
//!     stack().children((
//!         button("More").on_activate(move || section.toggle()),
//!         presence(section.is_open(), || text("Hidden details")),
//!     ))
//! }
//! ```

use std::{cell::Cell, rc::Rc};

use mue_core::effect::computed;

use mue_core::{
    effect::{on_cleanup, watch, watch_effect},
    prelude::*,
};
use mue_macroquad::app::runtime::{TimeoutHandle, set_timeout};

/// An open/closed state for dialogs, menus, and collapsible sections.
#[derive(Clone, Copy)]
pub struct Disclosure {
    open: Signal<bool>,
}

impl Disclosure {
    pub fn is_open(&self) -> ReadSignal<bool> {
        *self.open
    }

    pub fn signal(&self) -> Signal<bool> {
        self.open
    }

    pub fn open(&self) {
        self.open.set(true);
    }

    pub fn close(&self) {
        self.open.set(false);
    }

    pub fn toggle(&self) {
        self.open.set(!self.open.get_untracked());
    }
}

/// Create a [`Disclosure`] in the current scope.
pub fn use_disclosure(initially_open: bool) -> Disclosure {
    Disclosure {
        open: signal(initially_open),
    }
}

/// A boolean signal with `toggle`; equivalent to a disclosure.
pub fn use_toggle(initial: bool) -> Disclosure {
    use_disclosure(initial)
}

/// Follow `source` after it has stopped changing for `delay` seconds.
///
/// Useful for search fields: the debounced value changes once typing pauses.
/// Timers advance with the App frame loop.
pub fn use_debounced<T: Clone + PartialEq + 'static>(
    source: ReadSignal<T>,
    delay: f32,
) -> ReadSignal<T> {
    let output = signal(source.get_clone_untracked());
    let pending: Rc<Cell<Option<TimeoutHandle>>> = Rc::default();
    watch(source, {
        let pending = pending.clone();
        move |value| {
            if let Some(previous) = pending.take() {
                previous.cancel();
            }
            let value = value.get_clone_untracked();
            pending.set(Some(set_timeout(delay, move || {
                if !output.is_disposed() {
                    output.set(value);
                }
            })));
        }
    });
    watch_effect(move || {
        let pending = pending.clone();
        on_cleanup(move || {
            if let Some(previous) = pending.take() {
                previous.cancel();
            }
        });
    });
    *output
}

/// Call `tick` every `period` seconds while the current scope lives.
pub fn use_interval(period: f32, tick: impl FnMut() + 'static) {
    let alive = Rc::new(Cell::new(true));
    let tick = Rc::new(std::cell::RefCell::new(tick));
    fn schedule(period: f32, alive: Rc<Cell<bool>>, tick: Rc<std::cell::RefCell<dyn FnMut()>>) {
        set_timeout(period.max(0.001), move || {
            if alive.get() {
                (tick.borrow_mut())();
                schedule(period, alive, tick);
            }
        });
    }
    schedule(period, alive.clone(), tick);
    watch_effect(move || {
        let alive = alive.clone();
        on_cleanup(move || alive.set(false));
    });
}

/// The system clipboard (text only). On platforms without a clipboard,
/// `paste` returns `None` and `copy` does nothing.
#[derive(Clone, Copy, Default)]
pub struct Clipboard;

impl Clipboard {
    pub fn copy(&self, text: &str) {
        macroquad::miniquad::window::clipboard_set(text);
    }

    pub fn paste(&self) -> Option<String> {
        macroquad::miniquad::window::clipboard_get()
    }
}

/// Access the system clipboard.
pub fn use_clipboard() -> Clipboard {
    Clipboard
}

/// Which of the ascending width `thresholds` the window has reached; see
/// [`mue_macroquad::app::viewport::breakpoint`].
pub fn use_breakpoint(thresholds: &[f32]) -> ReadSignal<usize> {
    mue_macroquad::app::viewport::breakpoint(thresholds)
}

/// Safe-area insets in logical pixels (notches, rounded corners, system bars).
pub fn use_safe_area() -> ReadSignal<mue_macroquad::math::Edges> {
    mue_macroquad::app::viewport::safe_area()
}

/// The soft keyboard, for layouts that adapt to it (scroll areas already keep
/// focused fields visible).
#[derive(Clone, Copy)]
pub struct Keyboard {
    pub visible: ReadSignal<bool>,
    /// How far it covers the window from the bottom, in logical pixels.
    pub inset: ReadSignal<f32>,
    /// Its frame in logical window pixels (empty while hidden).
    pub frame: ReadSignal<mue_macroquad::math::Rect>,
}

pub fn use_keyboard() -> Keyboard {
    Keyboard {
        visible: mue_macroquad::input::text_input::keyboard_visible(),
        inset: mue_macroquad::input::text_input::keyboard_inset(),
        frame: mue_macroquad::input::text_input::keyboard_frame(),
    }
}

/// Light or dark appearance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    #[default]
    Dark,
}

/// The app's color mode preference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorMode {
    Light,
    Dark,
    /// Follow the system scheme ([`set_system_color_scheme`]).
    #[default]
    System,
}

thread_local! {
    static COLOR_MODE: Signal<ColorMode> =
        mue_core::scope::Scope::null().run(|| Signal::new(ColorMode::System));
    static SYSTEM_SCHEME: Signal<ColorScheme> =
        mue_core::scope::Scope::null().run(|| Signal::new(ColorScheme::Dark));
    static REDUCED_MOTION: Signal<bool> =
        mue_core::scope::Scope::null().run(|| Signal::new(false));
}

/// Report the operating system color scheme (hosts call this; the default is dark).
pub fn set_system_color_scheme(scheme: ColorScheme) {
    SYSTEM_SCHEME.with(|signal| {
        if signal.get_untracked() != scheme {
            signal.set(scheme);
        }
    });
}

/// Report the system reduced-motion preference (hosts call this).
pub fn set_prefers_reduced_motion(reduce: bool) {
    REDUCED_MOTION.with(|signal| {
        if signal.get_untracked() != reduce {
            signal.set(reduce);
        }
    });
}

/// The app-wide color mode (Vue's `useColorMode`): a preference that can be
/// light, dark, or system, and the resolved scheme to style with.
#[derive(Clone, Copy)]
pub struct ColorModeState {
    /// The preference; write it to switch modes.
    pub mode: Signal<ColorMode>,
    /// The scheme in effect after resolving `System`. Tracked.
    pub scheme: ReadSignal<ColorScheme>,
}

impl ColorModeState {
    pub fn is_dark(&self) -> bool {
        self.scheme.get() == ColorScheme::Dark
    }

    /// Switch between light and dark (leaving `System`).
    pub fn toggle(&self) {
        let next = match self.scheme.get_untracked() {
            ColorScheme::Dark => ColorMode::Light,
            ColorScheme::Light => ColorMode::Dark,
        };
        self.mode.set(next);
    }
}

pub fn use_color_mode() -> ColorModeState {
    let mode = COLOR_MODE.with(|signal| *signal);
    let system = SYSTEM_SCHEME.with(|signal| **signal);
    let scheme = computed(move |_| match mode.get() {
        ColorMode::Light => ColorScheme::Light,
        ColorMode::Dark => ColorScheme::Dark,
        ColorMode::System => system.get(),
    });
    ColorModeState { mode, scheme }
}

/// Evaluate a CSS media query against the window (Vue's `useMediaQuery`).
/// Tracked.
///
/// Supported features: `min-width`, `max-width`, `min-height`, `max-height`
/// (in `px`), `orientation: portrait | landscape`, `hover: hover | none`,
/// `pointer: fine | coarse`, `prefers-reduced-motion: reduce |
/// no-preference`, and `prefers-color-scheme: light | dark` (the resolved
/// color mode). Combine features with `and`, alternatives with commas, and
/// negate a query with a leading `not`. Unknown features never match.
///
/// ```
/// use mue_components::prelude::*;
///
/// # let owner = mue_core::scope::OwnedScope::new();
/// # owner.run(|| {
/// let wide = use_media_query("(min-width: 768px) and (orientation: landscape)");
/// let touch = use_media_query("(hover: none), (pointer: coarse)");
/// # let _ = (wide, touch);
/// # });
/// ```
pub fn use_media_query(query: &str) -> ReadSignal<bool> {
    let alternatives: Vec<(bool, Vec<Feature>)> =
        query.split(',').map(parse_media_alternative).collect();
    let viewport = mue_macroquad::app::viewport::use_viewport();
    let hover = mue_macroquad::input::hover::hover_capable();
    let reduced = REDUCED_MOTION.with(|signal| **signal);
    let scheme = use_color_mode().scheme;
    computed(move |_| {
        let viewport = viewport.get();
        alternatives.iter().any(|(negated, features)| {
            let all = features.iter().all(|feature| match *feature {
                Feature::MinWidth(px) => viewport.width >= px,
                Feature::MaxWidth(px) => viewport.width <= px,
                Feature::MinHeight(px) => viewport.height >= px,
                Feature::MaxHeight(px) => viewport.height <= px,
                Feature::Portrait(portrait) => viewport.is_portrait() == portrait,
                Feature::Hover(can) => hover.get() == can,
                Feature::FinePointer(fine) => hover.get() == fine,
                Feature::ReducedMotion(reduce) => reduced.get() == reduce,
                Feature::Dark(dark) => (scheme.get() == ColorScheme::Dark) == dark,
                Feature::Never => false,
            });
            all != *negated
        })
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Feature {
    MinWidth(f32),
    MaxWidth(f32),
    MinHeight(f32),
    MaxHeight(f32),
    Portrait(bool),
    Hover(bool),
    FinePointer(bool),
    ReducedMotion(bool),
    Dark(bool),
    Never,
}

fn parse_media_alternative(query: &str) -> (bool, Vec<Feature>) {
    let query = query.trim();
    let (negated, rest) = match query.strip_prefix("not ") {
        Some(rest) => (true, rest),
        None => (false, query),
    };
    let features = rest
        .split(" and ")
        .map(str::trim)
        .filter(|part| !part.is_empty() && *part != "all" && *part != "screen")
        .map(parse_media_feature)
        .collect();
    (negated, features)
}

fn parse_media_feature(part: &str) -> Feature {
    let inner = part.trim_start_matches('(').trim_end_matches(')');
    let (name, value) = match inner.split_once(':') {
        Some((name, value)) => (name.trim(), value.trim()),
        None => (inner.trim(), ""),
    };
    let px = || value.trim_end_matches("px").trim().parse::<f32>().ok();
    match (name, value) {
        ("min-width", _) => px().map_or(Feature::Never, Feature::MinWidth),
        ("max-width", _) => px().map_or(Feature::Never, Feature::MaxWidth),
        ("min-height", _) => px().map_or(Feature::Never, Feature::MinHeight),
        ("max-height", _) => px().map_or(Feature::Never, Feature::MaxHeight),
        ("orientation", "portrait") => Feature::Portrait(true),
        ("orientation", "landscape") => Feature::Portrait(false),
        ("hover", "hover") => Feature::Hover(true),
        ("hover", "none") => Feature::Hover(false),
        ("pointer", "fine") => Feature::FinePointer(true),
        ("pointer", "coarse") => Feature::FinePointer(false),
        ("prefers-reduced-motion", "reduce") | ("prefers-reduced-motion", "") => {
            Feature::ReducedMotion(true)
        }
        ("prefers-reduced-motion", "no-preference") => Feature::ReducedMotion(false),
        ("prefers-color-scheme", "dark") => Feature::Dark(true),
        ("prefers-color-scheme", "light") => Feature::Dark(false),
        _ => Feature::Never,
    }
}
