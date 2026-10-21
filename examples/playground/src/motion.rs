use super::{controls::*, responsive};
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    math::{Matrix, vec2},
    shader::Brush,
    style::{Styleable, percent, vh, vw},
    transition::Transition,
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
            ("Spring", Transition::spring(0.65, 0.3)),
            ("Soft", Transition::spring(0.9, 0.)),
            ("Bounce", Transition::spring(0.8, 0.65)),
            ("Linear", Transition::linear(0.6)),
            ("Ease", Transition::ease_in_out(0.6)),
        ],
    );
    let pulse = signal(false);
    stack()
        .style(mue_macroquad::style! {
width: 100%;
min-width: 0px;
gap: {vh(3.)};
})
        .children((
            controls(vec![a, b, c, d, e, f, g, hc, i, j, k, l, m, n, o, p]),
            center()
                .style(mue_macroquad::style! {
width: 100%;
height: {vh(52.5)};
min-height: {responsive::ui(220.)};
})
                .children(
                    stack()
                        .style(mue_macroquad::style! {
min-width: 0px;
width: {w.select(percent(65.), percent(40.))};
height: {h.select(vh(37.5), vh(21.25))};
min-height: {responsive::ui(140.)};
border-radius: {radius.select(70., 12.)};
padding: {(
                            padding.select(vw(4.), vw(1.5)),
                            padding.select(vh(4.), vh(1.5)),
                        )};
margin: {(margin.select(vw(3.), vw(0.)), margin.select(vh(3.), vh(0.)))};
opacity: {opacity.select(0.35, 1.)};
background: {color.select(Brush::Solid(CORAL), Brush::Solid(BLUE))};
left: {x.select(percent(6.), percent(0.))};
top: {y.select(percent(6.), percent(0.))};
gap: {gap.select(vh(3.5), vh(1.))};
transition: {curve};
transform: {computed(move |_| {
                            Matrix::from_scale_angle_translation(
                                vec2(1., 1.) * if scale.get() { 1.15 } else { 1. },
                                if rotate.get() { 0.12 } else { 0. },
                                vec2(0., 0.),
                            )
                        })};
})
                        .children((
                            heading("Spring")
                                .style(mue_macroquad::style! {
font-size: {responsive::font(font.select(42., 26.))};
line-height: {responsive::ui(leading.select(64., 38.))};
letter-spacing: {responsive::ui(letter.select(3., 0.))};
transition: {SPRING};
}),
                            text("Retarget any property")
                                .style(mue_macroquad::style! {
font-size: {responsive::font(18.)};
line-height: {responsive::ui(28.)};
}),
                        )),
                ),
            button("Selected curve", move || pulse.toggle()),
            center()
                .style(mue_macroquad::style! {
width: {pulse.select(percent(90.), percent(30.))};
height: {vh(8.125)};
min-height: {responsive::ui(44.)};
border-radius: {16.};
background: {CORAL};
})
                .children(
                    text("Curve")
                        .style(mue_macroquad::style! {
font-size: {responsive::font(22.)};
line-height: {responsive::ui(32.)};
}),
                ),
        ))
        .into_node()
}
