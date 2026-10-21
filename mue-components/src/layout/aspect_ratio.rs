use mue_macroquad::node::flexbox::render_container;
use mue_macroquad::style::{Style, Styleable, StyleableExt};

/// Constrain width / height to a positive ratio. Set one dimension, leaving the other auto.
/// Children retain their normal layout; use `size_full` to fill the frame.
#[mue_macros::node]
pub fn aspect_ratio(style: &mut Style, #[default(4. / 3.)] ratio: f32) {
    style.provide_defaults(
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
    );
    render_container(style);
}
