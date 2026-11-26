//! Scene ownership and the transitions requested by this consumer.
pub(crate) mod details;
pub(crate) mod play_flow;

pub(crate) use details::Details;

use mue_macroquad::{SceneTransition, motion::transition::Transition};

fn scene_fade() -> SceneTransition {
    SceneTransition::crossfade(Transition::linear(0.25))
}
