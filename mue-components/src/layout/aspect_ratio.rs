use mue_macroquad::node::flexbox::use_container;
use mue_macroquad::node::{Node, NodeContext};
use mue_macroquad::style::{Style, Styleable, StyleableExt};

/// Constrain width / height to a positive ratio. Set one dimension, leaving the other auto.
/// Children retain their normal layout; use `size_full` to fill the frame.
#[mue_macros::node]
pub fn aspect_ratio(ctx: NodeContext, #[default(4. / 3.)] ratio: f32) -> Node {
    ctx.finish(
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
        move |style| {
            use_container(style);
        },
    )
}
