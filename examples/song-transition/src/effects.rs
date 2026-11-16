//! The effect owns only an artwork asset and the geometry needed for its two phases.
use crate::artwork::{self, Anchor, Artwork, Surface, ease_out_quart};
use mue_macroquad::{SceneTransition, TransitionPhase, TransitionTiming, transition::Transition};
use std::{cell::Cell, rc::Rc};

/// A narrowly shared return anchor. Library refreshes it after normal layout;
/// the overlay reads it after Library draws. It never contains a Scene/Node owner.
#[derive(Clone, Default)]
pub struct ReturnTarget(Rc<Cell<Option<Anchor>>>);
impl ReturnTarget {
    pub fn update(&self, anchor: Option<Anchor>) {
        self.0.set(anchor);
    }
    fn get(&self) -> Option<Anchor> {
        self.0.get()
    }
}

pub fn expand(artwork: Rc<Artwork>, origin: Anchor) -> SceneTransition {
    SceneTransition::overlay(
        TransitionTiming::AroundSwitch {
            exit: Transition::linear(0.45),
            enter: Transition::linear(0.3),
        },
        move |frame| {
            let full = Surface::fullscreen(frame.viewport, artwork.aspect());
            let (surface, alpha) = match frame.phase {
                TransitionPhase::BeforeSwitch => (
                    origin
                        .in_viewport(frame.viewport)
                        .lerp(full, ease_out_quart(frame.linear_progress)),
                    1.,
                ),
                TransitionPhase::AfterSwitch => (full, 1. - ease_out_quart(frame.linear_progress)),
            };
            artwork::draw(&artwork, surface, alpha);
            Ok(())
        },
    )
}

pub fn shrink(artwork: Rc<Artwork>, target: ReturnTarget) -> SceneTransition {
    SceneTransition::overlay(
        TransitionTiming::AroundSwitch {
            exit: Transition::linear(0.24),
            enter: Transition::linear(0.4),
        },
        move |frame| {
            let full = Surface::fullscreen(frame.viewport, artwork.aspect());
            let (surface, alpha) = match frame.phase {
                TransitionPhase::BeforeSwitch => (full, ease_out_quart(frame.linear_progress)),
                TransitionPhase::AfterSwitch => match target.get() {
                    Some(anchor) => (
                        full.lerp(
                            anchor.in_viewport(frame.viewport),
                            ease_out_quart(frame.linear_progress),
                        ),
                        1.,
                    ),
                    None => (full, 1. - ease_out_quart(frame.linear_progress)),
                },
            };
            artwork::draw(&artwork, surface, alpha);
            Ok(())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mue_macroquad::math::Rect;
    #[test]
    fn return_anchor_shares_only_copied_geometry() {
        let target = ReturnTarget::default();
        let reader = target.clone();
        let viewport = Rect::new(0., 0., 1000., 720.);
        let anchor = Anchor {
            surface: Surface::card(Rect::new(40., 200., 320., 210.), viewport, 1.5),
            viewport,
        };
        assert_eq!(reader.get(), None);
        target.update(Some(anchor));
        assert_eq!(reader.get(), Some(anchor));
        target.update(None);
        assert_eq!(reader.get(), None);
    }
}
