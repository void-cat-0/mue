use super::controls::*;
use mue_core::prelude::*;
use mue_macroquad::{
    animation::Animation,
    math::{Matrix, vec2},
    node::*,
    shader::Brush,
    style::{Styleable, StyleableExt},
};

pub fn page() -> Node {
    let (w, a) = toggle("Width");
    let (h, b) = toggle("Height");
    let (radius, c) = toggle("Radius");
    let (padding, d) = toggle("Padding");
    let (margin, e) = toggle("Margin");
    let (opacity, f) = toggle("Opacity");
    let (color, g) = toggle("Color");
    let (font, hc) = toggle("Font size");
    let (leading, i) = toggle("Line height");
    let (gap, j) = toggle("Gap");
    let (x, k) = toggle("X");
    let (y, l) = toggle("Y");
    let (rotate, m) = toggle("Rotate");
    let (scale, n) = toggle("Scale");
    let (letter, o) = toggle("Letter spacing");
    let (curve, p) = choice(
        "Curve",
        vec![
            ("Spring", Animation::spring(0.65, 0.3)),
            ("Soft", Animation::spring(0.9, 0.)),
            ("Bounce", Animation::spring(0.8, 0.65)),
            ("Linear", Animation::linear(0.6)),
            ("Ease", Animation::ease_in_out(0.6)),
        ],
    );
    let pulse = signal(false);
    stack()
        .gap(24.)
        .children((
            controls(vec![a, b, c, d, e, f, g, hc, i, j, k, l, m, n, o, p]),
            center().height(420.).children(
                stack()
                    .width(w.select(520., 270.))
                    .height(h.select(300., 170.))
                    .radius(radius.select(70., 12.))
                    .padding((padding.select(35., 12.),))
                    .margin((margin.select(30., 0.),))
                    .opacity(opacity.select(0.35, 1.))
                    .background(color.select(Brush::Solid(CORAL), Brush::Solid(BLUE)))
                    .inset_left(x.select(60., 0.))
                    .inset_top(y.select(45., 0.))
                    .gap(gap.select(28., 8.))
                    .animation(SPRING)
                    .transform(computed(move |_| {
                        Matrix::from_scale_angle_translation(
                            vec2(1., 1.) * if scale.get() { 1.15 } else { 1. },
                            if rotate.get() { 0.12 } else { 0. },
                            vec2(0., 0.),
                        )
                    }))
                    .children((
                        heading("Spring")
                            .font_size(font.select(42., 26.))
                            .line_height(leading.select(64., 38.))
                            .letter_spacing(letter.select(3., 0.))
                            .animation(SPRING),
                        text("Retarget any property")
                            .font_size(18.)
                            .line_height(28.),
                    )),
            ),
            button("Selected curve", move || {
                mue_macroquad::animation::with_animation(curve.get(), || pulse.toggle())
            }),
            center()
                .width(pulse.select(700., 180.))
                .height(65.)
                .radius(16.)
                .background(CORAL)
                .children(text("Curve").font_size(22.)),
        ))
        .into_node()
}
