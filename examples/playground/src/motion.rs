use super::{controls::*, responsive};
use mue_core::prelude::*;
use mue_macroquad::{
    animation::Animation,
    math::{Matrix, vec2},
    node::*,
    shader::Brush,
    style::{Styleable, StyleableExt, percent, vh, vw},
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
        .w_full()
        .min_width(0.)
        .gap(vh(3.))
        .children((
            controls(vec![a, b, c, d, e, f, g, hc, i, j, k, l, m, n, o, p]),
            center()
                .w_full()
                .height(vh(52.5))
                .min_height(responsive::ui(220.))
                .children(
                    stack()
                        .min_width(0.)
                        .width(w.select(percent(65.), percent(40.)))
                        .height(h.select(vh(37.5), vh(21.25)))
                        .min_height(responsive::ui(140.))
                        .radius(radius.select(70., 12.))
                        .padding((
                            padding.select(vw(4.), vw(1.5)),
                            padding.select(vh(4.), vh(1.5)),
                        ))
                        .margin((margin.select(vw(3.), vw(0.)), margin.select(vh(3.), vh(0.))))
                        .opacity(opacity.select(0.35, 1.))
                        .background(color.select(Brush::Solid(CORAL), Brush::Solid(BLUE)))
                        .inset_left(x.select(percent(6.), percent(0.)))
                        .inset_top(y.select(percent(6.), percent(0.)))
                        .gap(gap.select(vh(3.5), vh(1.)))
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
                                .font_size(responsive::font(font.select(42., 26.)))
                                .line_height(responsive::ui(leading.select(64., 38.)))
                                .letter_spacing(responsive::ui(letter.select(3., 0.)))
                                .animation(SPRING),
                            text("Retarget any property")
                                .font_size(responsive::font(18.))
                                .line_height(responsive::ui(28.)),
                        )),
                ),
            button("Selected curve", move || {
                mue_macroquad::animation::with_animation(curve.get(), || pulse.toggle())
            }),
            center()
                .width(pulse.select(percent(90.), percent(30.)))
                .height(vh(8.125))
                .min_height(responsive::ui(44.))
                .radius(16.)
                .background(CORAL)
                .children(
                    text("Curve")
                        .font_size(responsive::font(22.))
                        .line_height(responsive::ui(32.)),
                ),
        ))
        .into_node()
}
