//! Draw-only Scene transitions. App owns Scene lifetime, requests and scheduling.
//!
//! Constructors do not touch macroquad or allocate GPU resources. An overlay is
//! drawn over the normally rendered Scene and needs no pictures. A composite
//! receives complete, opaque source/live pictures and draws into a third target.
//! Picture capture/composition supports OpenGL and Vulkan; overlays do not
//! impose a picture-backend restriction.
//!
//! Picture passes use the whole logical, origin-zero window. Overlays impose no
//! picture-viewport restriction and start in the default camera. Composite and
//! capture draws must keep the supplied target camera active. All draws must
//! balance their model-matrix stack; raw miniquad passes are not supported inside
//! these boundaries. The host enters with the default camera, no active model
//! transform and no raw pass.
//! App/cache boundaries flush and restore the default camera and the caller's
//! logical material, mode, scissor and viewport, not arbitrary backend state.
//! Keep materials used in queued draws alive through the boundary flush. On
//! ordinary Scene draws, do not leave a Scene-owned material selected after
//! returning: that Scene may be dropped before the next drawing. App picture
//! passes start with all channels enabled, and effect boundaries restore them
//! on return. Within a callback or ordinary host draw, submit an all-channel
//! pipeline before clearing after a custom OpenGL color-write mask.
//!
//! Attach an effect to a deferred [`SceneRequest`](crate::SceneRequest), or set an
//! App-level factory for defaults. The default is no animation.
//! [`TransitionTiming::AfterSwitch`] commits before its draw segment;
//! [`TransitionTiming::AroundSwitch`] presents an exit segment before committing
//! at a later safe boundary. Only the active top Scene receives ordinary
//! updates. These effects are presentation, not lifecycle callbacks or raw-input
//! filters.
//!
//! A selected composite captures pictures lazily and requires OpenGL or Vulkan.
//! Overlays allocate no pictures and have no picture-backend restriction. Keep
//! callback resources owned for the full effect lifetime; a departing Scene is
//! not retained by the transition. See [`SceneTransition::composite`] and
//! [`SceneTransition::overlay`] for their callback contracts.
//!
//! ```no_run
//! use mue_macroquad::{Scene, SceneContext, SceneTransition, transition::Transition};
//!
//! struct Details;
//! impl Scene for Details {}
//!
//! fn open_details(ctx: &mut SceneContext<'_>) {
//!     ctx.push(Details)
//!         .transition(SceneTransition::crossfade(Transition::ease(0.25)));
//! }
//! ```

mod clock;
mod picture;

pub(crate) use clock::PhaseClock;
pub(crate) use picture::PictureCache;

use crate::CallbackResult;
use macroquad::texture::Texture2D;

use crate::{math::Rect, transition::Transition};

/// Whether drawing starts after the Scene switch or straddles it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransitionTiming {
    AfterSwitch(Transition),
    AroundSwitch { exit: Transition, enter: Transition },
}

/// The Scene switch separates two independently timed drawing segments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionPhase {
    BeforeSwitch,
    AfterSwitch,
}

/// One candidate presentation of the current transition segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransitionFrame {
    pub phase: TransitionPhase,
    /// Clamped time fraction. Only this value controls phase completion.
    pub linear_progress: f32,
    /// `Transition::sample(linear_progress)`, including spring overshoot.
    pub progress: f32,
    /// Accepted finite positive host delta; zero on a segment's first draw.
    pub dt: f32,
    pub viewport: Rect,
    /// Elapsed seconds in this segment, capped at its duration.
    pub elapsed: f32,
}

/// Borrowed complete Scene pictures, valid only during this composite draw.
///
/// These are render-target textures. With the supplied target camera, drawing
/// them using `draw_texture_ex` requires `flip_y: true`, as with other macroquad
/// render-target copies. After resize, if the departed Scene is unavailable,
/// both references can be the same freshly rendered live picture; the clock
/// still continues instead of restarting. Neither is the output target.
#[derive(Clone, Copy)]
pub struct ScenePictures<'a> {
    pub from: &'a Texture2D,
    pub to: &'a Texture2D,
}

type CompositeDraw = dyn for<'a> FnMut(TransitionFrame, ScenePictures<'a>) -> CallbackResult<()>;
type OverlayDraw = dyn FnMut(TransitionFrame) -> CallbackResult<()>;

/// An owned draw callback and timing, not a Scene or a lifecycle controller.
pub struct SceneTransition {
    timing: TransitionTiming,
    draw: Draw,
}

enum Draw {
    Crossfade,
    Composite(Box<CompositeDraw>),
    Overlay(Box<OverlayDraw>),
}

impl SceneTransition {
    /// GPU crossfade of the old complete picture and the current live picture.
    /// Vulkan loads its precompiled shader; OpenGL compiles GLSL on the first draw.
    pub fn crossfade(curve: Transition) -> Self {
        Self {
            timing: TransitionTiming::AfterSwitch(curve),
            draw: Draw::Crossfade,
        }
    }

    /// Draw a custom composite after switching. GPU resources created by the
    /// callback should likewise be initialized lazily during drawing.
    pub fn composite(
        curve: Transition,
        draw: impl for<'a> FnMut(TransitionFrame, ScenePictures<'a>) -> CallbackResult<()> + 'static,
    ) -> Self {
        Self {
            timing: TransitionTiming::AfterSwitch(curve),
            draw: Draw::Composite(Box::new(draw)),
        }
    }

    /// Draw over the normally rendered Scene, without allocating any pictures.
    /// `AroundSwitch` presents the exit endpoint before App commits the switch.
    pub fn overlay(
        timing: TransitionTiming,
        draw: impl FnMut(TransitionFrame) -> CallbackResult<()> + 'static,
    ) -> Self {
        Self {
            timing,
            draw: Draw::Overlay(Box::new(draw)),
        }
    }

    pub(crate) fn timing(&self) -> TransitionTiming {
        self.timing
    }

    pub(crate) fn is_composite(&self) -> bool {
        !matches!(self.draw, Draw::Overlay(_))
    }
}
