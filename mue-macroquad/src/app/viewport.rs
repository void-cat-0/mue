//! The window viewport, breakpoints, and safe-area insets as signals.
//!
//! [`App`](crate::App) publishes the logical window size and DPI scale every
//! frame. Read them with [`use_viewport`], derive layout switches with
//! [`breakpoint`] or [`min_width`], and pad content away from notches and
//! system bars with [`safe_area`].
//!
//! Safe-area insets come from the host: the platform layer (Android window
//! insets, iOS `safeAreaInsets`) calls [`set_safe_area`] when they change.
//! They default to zero, which is correct on desktop.
//!
//! ```
//! use mue_macroquad::app::viewport::{breakpoint, safe_area, use_viewport};
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! // 0 below 640 px, 1 from 640 px, 2 from 1024 px.
//! let layout = breakpoint(&[640., 1024.]);
//! let insets = safe_area();
//! let viewport = use_viewport();
//! # let _ = (layout, insets, viewport);
//! # });
//! ```

use mue_core::{
    effect::computed,
    prop::PropValue,
    scope::Scope,
    signal::{Access, ReadSignal, Signal},
};

use crate::math::Edges;

/// The logical viewport used by viewport-relative style units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// Width in logical pixels.
    pub width: f32,
    /// Height in logical pixels.
    pub height: f32,
    /// Physical pixels per logical pixel.
    pub dpi_scale: f32,
}
impl PropValue for Viewport {}

impl Viewport {
    pub fn is_portrait(&self) -> bool {
        self.height > self.width
    }

    pub fn is_landscape(&self) -> bool {
        !self.is_portrait()
    }
}

thread_local! {
    static VIEWPORT: Signal<Viewport> = Scope::null().run(|| {
        Signal::new(Viewport {
            width: 0.0,
            height: 0.0,
            dpi_scale: 1.0,
        })
    });
    static SAFE_AREA: Signal<Edges> = Scope::null().run(|| Signal::new(Edges::ZERO));
}

pub(crate) fn read() -> ReadSignal<Viewport> {
    VIEWPORT.with(|viewport| **viewport)
}

pub(crate) fn set(viewport: Viewport) {
    VIEWPORT.with(|current| {
        if current.get_untracked() != viewport {
            current.set(viewport);
        }
    });
}

/// The current viewport. Tracked reads update when the window resizes.
pub fn use_viewport() -> ReadSignal<Viewport> {
    read()
}

/// Safe-area insets in logical pixels (notches, rounded corners, system bars).
pub fn safe_area() -> ReadSignal<Edges> {
    SAFE_AREA.with(|insets| **insets)
}

/// Publish safe-area insets from the platform layer.
pub fn set_safe_area(insets: Edges) {
    SAFE_AREA.with(|current| {
        if current.get_untracked() != insets {
            current.set(insets);
        }
    });
}

/// How many of the ascending `thresholds` the viewport width has reached.
///
/// With `[640., 1024.]` the value is 0 below 640 px, 1 from 640 px, and 2 from
/// 1024 px. Create it in the scope that uses it.
pub fn breakpoint(thresholds: &[f32]) -> ReadSignal<usize> {
    let thresholds = thresholds.to_vec();
    let viewport = read();
    computed(move |_| {
        let width = viewport.get().width;
        thresholds
            .iter()
            .filter(|threshold| width >= **threshold)
            .count()
    })
}

/// Whether the viewport is at least `width` logical pixels wide.
pub fn min_width(width: f32) -> ReadSignal<bool> {
    let viewport = read();
    computed(move |_| viewport.get().width >= width)
}
