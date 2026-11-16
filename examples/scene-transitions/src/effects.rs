//! Draw-only effects; callbacks do not borrow or mutate Scene state.
use macroquad::prelude::*;
use mue_macroquad::{SceneTransition, TransitionPhase, TransitionTiming, transition::Transition};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    Fade,
    Slide,
    Cover,
}

impl Effect {
    pub(crate) fn transition(self) -> SceneTransition {
        match self {
            Self::Fade => SceneTransition::crossfade(Transition::ease(0.35)),
            Self::Slide => {
                SceneTransition::composite(Transition::ease_out(0.35), |frame, pictures| {
                    let v = frame.viewport;
                    let (from_x, to_x) = slide_offsets(v.w, frame.progress);
                    for (texture, x) in [(pictures.from, from_x), (pictures.to, to_x)] {
                        draw_texture_ex(
                            texture,
                            v.x + x,
                            v.y,
                            WHITE,
                            DrawTextureParams {
                                dest_size: Some(vec2(v.w, v.h)),
                                // Pictures are render-target textures, not image assets.
                                flip_y: true,
                                ..Default::default()
                            },
                        );
                    }
                    Ok(())
                })
            }
            Self::Cover => SceneTransition::overlay(
                TransitionTiming::AroundSwitch {
                    exit: Transition::ease_out(0.3),
                    enter: Transition::ease_out(0.3),
                },
                |frame| {
                    let v = frame.viewport;
                    let alpha = cover_opacity(frame.phase, frame.progress);
                    draw_rectangle(v.x, v.y, v.w, v.h, Color::new(0., 0., 0., alpha));
                    Ok(())
                },
            ),
        }
    }
}

fn slide_offsets(width: f32, progress: f32) -> (f32, f32) {
    (-width * progress, width * (1. - progress))
}

fn cover_opacity(phase: TransitionPhase, progress: f32) -> f32 {
    match phase {
        TransitionPhase::BeforeSwitch => progress,
        TransitionPhase::AfterSwitch => 1. - progress,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slide_moves_complete_pictures_edge_to_edge() {
        assert_eq!(slide_offsets(640., 0.), (0., 640.));
        assert_eq!(slide_offsets(640., 0.5), (-320., 320.));
        assert_eq!(slide_offsets(640., 1.), (-640., 0.));
        for progress in [0., 0.25, 0.5, 0.75, 1.] {
            let (from, to) = slide_offsets(640., progress);
            assert_eq!(from + 640., to);
        }
    }

    #[test]
    fn cover_is_opaque_on_both_sides_of_the_switch() {
        assert_eq!(cover_opacity(TransitionPhase::BeforeSwitch, 0.), 0.);
        assert_eq!(cover_opacity(TransitionPhase::BeforeSwitch, 1.), 1.);
        assert_eq!(cover_opacity(TransitionPhase::AfterSwitch, 0.), 1.);
        assert_eq!(cover_opacity(TransitionPhase::AfterSwitch, 1.), 0.);
        assert_eq!(cover_opacity(TransitionPhase::BeforeSwitch, 0.25), 0.25);
        assert_eq!(cover_opacity(TransitionPhase::AfterSwitch, 0.25), 0.75);
    }
}
