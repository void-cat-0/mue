use super::flexbox::render_container;
use crate::{
    math::Vector,
    node::NodeInner,
    style::{Size, Style, Styleable, StyleableExt},
};
use mue_core::prop::PropValue;
use taffy::LengthPercentageAuto;

/// The point on the parent where the floating child's center is anchored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FloatPlacement {
    TopStart,
    TopCenter,
    #[default]
    TopEnd,
    MiddleStart,
    MiddleCenter,
    MiddleEnd,
    BottomStart,
    BottomCenter,
    BottomEnd,
}
impl PropValue for FloatPlacement {}

impl FloatPlacement {
    fn anchor(self) -> Vector {
        use FloatPlacement::*;
        let x = match self {
            TopStart | MiddleStart | BottomStart => 0.,
            TopCenter | MiddleCenter | BottomCenter => 0.5,
            _ => 1.,
        };
        let y = match self {
            TopStart | TopCenter | TopEnd => 0.,
            MiddleStart | MiddleCenter | MiddleEnd => 0.5,
            _ => 1.,
        };
        Vector::new(x, y)
    }
}

/// Position a badge around a parent edge or center, outside normal flow.
/// Positive offsets move right/down. Add after the parent's other children to paint on top.
#[mue_macros::node]
pub fn float(
    style: &mut Style,
    #[default] placement: FloatPlacement,
    #[default(Size::Px(0.))] offset_x: Size,
    #[default(Size::Px(0.))] offset_y: Size,
) {
    style.provide_defaults(
        Style::new()
            .absolute()
            .items_center()
            .justify_center()
            .inset_left(placement.map(|p| LengthPercentageAuto::percent(p.anchor().x)))
            .inset_top(placement.map(|p| LengthPercentageAuto::percent(p.anchor().y)))
            .margin_left(offset_x)
            .margin_top(offset_y),
    );
    NodeInner::with_mut(|node| node.center_on_layout_origin = true);
    render_container(style);
}
