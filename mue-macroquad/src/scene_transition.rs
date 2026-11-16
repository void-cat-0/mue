//! Draw-only Scene transitions. App owns Scene lifetime, requests and scheduling.
//!
//! Constructors do not touch macroquad or allocate GPU resources. An overlay is
//! drawn over the normally rendered Scene and needs no pictures. A composite
//! receives complete, opaque source/live pictures and draws into a third target.
//! Picture capture/composition currently requires the OpenGL backend; overlays
//! do not impose that restriction.
//!
//! Picture passes use the whole logical, origin-zero window. Overlays impose no
//! picture-viewport restriction and start in the default camera. Composite and
//! capture draws must keep the supplied target camera active. All draws must
//! balance their model-matrix stack; raw miniquad passes are not supported inside
//! these boundaries. The host enters with the default camera and no raw pass.
//! App/cache boundaries flush and restore the default camera, rather than
//! preserving an arbitrary caller camera or raw pass.

mod clock;
mod picture;

pub(crate) use clock::PhaseClock;
pub(crate) use picture::PictureCache;

use anyhow::{Result, bail};
use macroquad::{material::Material, texture::Texture2D};

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

type CompositeDraw = dyn for<'a> FnMut(TransitionFrame, ScenePictures<'a>) -> Result<()>;
type OverlayDraw = dyn FnMut(TransitionFrame) -> Result<()>;

/// An owned draw callback and timing, not a Scene or a lifecycle controller.
pub struct SceneTransition {
    timing: TransitionTiming,
    draw: Draw,
}

enum Draw {
    Crossfade { material: Option<Material> },
    Composite(Box<CompositeDraw>),
    Overlay(Box<OverlayDraw>),
}

impl SceneTransition {
    /// GPU crossfade of the old complete picture and the current live picture.
    /// Shader compilation is deferred until the first composite draw.
    pub fn crossfade(curve: Transition) -> Self {
        Self {
            timing: TransitionTiming::AfterSwitch(curve),
            draw: Draw::Crossfade { material: None },
        }
    }

    /// Draw a custom composite after switching. GPU resources created by the
    /// callback should likewise be initialized lazily during drawing.
    pub fn composite(
        curve: Transition,
        draw: impl for<'a> FnMut(TransitionFrame, ScenePictures<'a>) -> Result<()> + 'static,
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
        draw: impl FnMut(TransitionFrame) -> Result<()> + 'static,
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

    // PictureCache supplies the camera/state boundary, also in headless mode.
    pub(crate) fn draw_overlay(&mut self, frame: TransitionFrame) -> Result<()> {
        match &mut self.draw {
            Draw::Overlay(draw) => draw(frame),
            _ => bail!("a composite transition requires Scene pictures"),
        }
    }

    pub(crate) fn draw_composite(
        &mut self,
        frame: TransitionFrame,
        pictures: ScenePictures<'_>,
    ) -> Result<()> {
        match &mut self.draw {
            Draw::Crossfade { material } => {
                if material.is_none() {
                    *material = Some(picture::make_crossfade_material()?);
                }
                picture::crossfade(material.as_ref().unwrap(), frame, pictures);
                Ok(())
            }
            Draw::Composite(draw) => draw(frame, pictures),
            Draw::Overlay(_) => bail!("an overlay transition does not composite Scene pictures"),
        }
    }
}

#[cfg(test)]
mod tests;
