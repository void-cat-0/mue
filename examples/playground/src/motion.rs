use super::{controls::*, responsive};
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    shader::Brush,
    style::{Styleable, Transform, TransformOp, percent, vh, vw},
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
    let (x, k) = choice(
        "X",
        vec![
            ("0%", percent(0.)),
            ("6%", percent(6.)),
            ("-6%", percent(-6.)),
        ],
    );
    let (y, l) = choice(
        "Y",
        vec![
            ("0%", percent(0.)),
            ("6%", percent(6.)),
            ("-6%", percent(-6.)),
        ],
    );
    let (rotate_z, m) = choice(
        "Rotate Z",
        vec![("0°", 0.), ("7°", 0.12), ("-7°", -0.12), ("18°", 0.31)],
    );
    let (scale_x, n) = choice(
        "Scale X",
        vec![("100%", 1.), ("115%", 1.15), ("85%", 0.85), ("130%", 1.3)],
    );
    let (scale_y, sy) = choice(
        "Scale Y",
        vec![("100%", 1.), ("115%", 1.15), ("85%", 0.85), ("130%", 1.3)],
    );
    let (rotate_x, rx) = choice("Rotate X", vec![("0°", 0.), ("12°", 0.21), ("-12°", -0.21)]);
    let (rotate_y, ry) = choice("Rotate Y", vec![("0°", 0.), ("18°", 0.31), ("-18°", -0.31)]);
    let (translate_z, tz) = choice(
        "Translate Z",
        vec![("0px", 0.), ("16px", 16.), ("-12px", -12.)],
    );
    let (scale_z, sz) = choice("Scale Z", vec![("1", 1.), ("0.8", 0.8), ("1.2", 1.2)]);
    let (perspective, ps) = choice(
        "Perspective",
        vec![("900px", 900.), ("520px", 520.), ("1400px", 1400.)],
    );
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
    stack()
        .style(mue_macroquad::style! {
width: 100%;
min-width: 0px;
gap: {vh(3.)};
})
        .children((
            controls(vec![a, b, c, d, e, f, g, hc, i, j, k, l, m, n, sy, rx, ry, tz, sz, ps, o, p]),
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
left: {x};
top: {y};
gap: {gap.select(vh(3.5), vh(1.))};
transition: {curve};
transform: {computed(move |_| {
                            Transform(vec![
                                TransformOp::Perspective(perspective.get()),
                                TransformOp::RotateX(rotate_x.get()),
                                TransformOp::RotateY(rotate_y.get()),
                                TransformOp::TranslateZ(translate_z.get()),
                                TransformOp::Scale3d(scale_x.get(), scale_y.get(), scale_z.get()),
                                TransformOp::Rotate(rotate_z.get()),
                            ])
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
        ))
        .into_node()
}
