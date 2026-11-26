use mue_core::{effect::computed, signal::Access};
use mue_macroquad::{
    node::{Component, flexbox},
    style::{Size, Styleable, StyleableExt},
    viewport::safe_area as insets,
};

/// A full-size column padded by the platform's safe-area insets (notches,
/// rounded corners, system bars). The host publishes the insets with
/// [`mue_macroquad::viewport::set_safe_area`]; on desktop they are zero.
/// Disable individual edges with the boolean setters.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _screen = safe_area().bottom(false).children(text("Content"));
/// ```
#[mue_macros::component]
pub fn safe_area(
    #[default(true)] top: bool,
    #[default(true)] right: bool,
    #[default(true)] bottom: bool,
    #[default(true)] left: bool,
) -> impl Component {
    let edges = insets();
    let side = move |enabled: mue_core::prop::Prop<bool>,
                     pick: fn(mue_macroquad::math::Edges) -> f32| {
        computed(move |_| {
            if enabled.get() {
                Size::Px(pick(edges.get()))
            } else {
                Size::Px(0.)
            }
        })
    };
    flexbox()
        .flex_column()
        .size_full()
        .padding_top(side(top, |e| e.top))
        .padding_right(side(right, |e| e.right))
        .padding_bottom(side(bottom, |e| e.bottom))
        .padding_left(side(left, |e| e.left))
}
