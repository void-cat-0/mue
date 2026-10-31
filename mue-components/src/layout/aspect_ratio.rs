use mue_macroquad::node::{NodePlan, flexbox::render_container};
use mue_macroquad::style::{Style, Styleable, StyleableExt};

/// Constrain width / height to a positive ratio. Set one dimension, leaving the other auto.
/// Children retain their normal layout; use `size_full` to fill the frame.
#[mue_macros::node]
#[theme(ratio)]
pub fn aspect_ratio(#[default(4. / 3.)] ratio: f32) -> NodePlan<impl FnOnce(&mut Style)> {
    NodePlan::new(
        Style::new()
            .aspect_ratio(ratio.map(|ratio| {
                assert!(
                    ratio.is_finite() && ratio > 0.,
                    "aspect ratio must be positive"
                );
                Some(ratio)
            }))
            .items_center()
            .justify_center(),
        move |style| render_container(style),
    )
}
