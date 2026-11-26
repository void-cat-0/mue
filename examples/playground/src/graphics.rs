use super::{assets, controls::*, halftone, responsive};
use mue_components::*;
use mue_macroquad::{
    math::{Rect, vec2},
    shader::{ColorStop, linear_gradient, radial_gradient, solid},
    style::{Styleable, percent, vh},
};

pub fn page() -> Node {
    let photo = assets::background_texture(include_bytes!("../../color/assets/photo.png"));
    let (fit, a) = choice(
        "Fit",
        vec![
            ("Fill", ObjectFit::Fill),
            ("Cover", ObjectFit::Cover),
            ("Contain", ObjectFit::Contain),
            ("ScaleDown", ObjectFit::ScaleDown),
            ("None", ObjectFit::None),
        ],
    );
    let (position, b) = choice(
        "Position",
        vec![
            ("Center", vec2(0.5, 0.5)),
            ("Start", vec2(0., 0.)),
            ("End", vec2(1., 1.)),
        ],
    );
    let (crop, c) = toggle("Crop");
    let (tint, d) = toggle("Tint");
    let (wide, e) = toggle("Width");
    let (tall, f) = toggle("Height");
    let (radius, g) = toggle("Radius");
    let (density, h) = toggle("Density");
    let (pause, i) = toggle("Pause");
    let (brush, j) = choice(
        "Brush",
        vec![
            ("Solid", solid(BLUE)),
            (
                "Linear",
                linear_gradient(
                    (0., 0.),
                    (1., 1.),
                    [ColorStop::new(0., BLUE), ColorStop::new(1., CORAL)],
                ),
            ),
            (
                "Radial",
                radial_gradient(
                    (0.5, 0.5),
                    0.7,
                    [ColorStop::new(0., CORAL), ColorStop::new(1., BLUE)],
                ),
            ),
        ],
    );
    let density = density.select(0.3, 0.08);
    stack()
        .style(mue_macroquad::style! {
        width: 100%;
        min-width: 0px;
        gap: {vh(2.5)};
        })
        .children((
            controls(vec![a, b, c, d, e, f, g, h, i, j]),
            image(photo)
                .style(mue_macroquad::style! {
                width: {wide.select(percent(90.), percent(50.))};
                height: {tall.select(vh(43.75), vh(27.5))};
                transition: {SPRING};
                })
                .object_fit(fit)
                .object_position(position)
                .region(crop.map(|on| on.then_some(Rect::new(100., 100., 400., 300.))))
                .style(mue_macroquad::style! { color: {tint.select(
                    solid(CORAL),
                    solid(mue_macroquad::color::Color::WHITE),
                )}; }),
            center()
                .style(mue_macroquad::style! {
                width: 100%;
                height: {vh(12.5)};
                min-height: {responsive::ui(54.)};
                border-radius: {radius.select(50., 8.)};
                background: {brush};
                transition: {SPRING};
                })
                .children(heading("Brush").style(mue_macroquad::style! {
                font-size: {responsive::font(28.)};
                line-height: {responsive::ui(38.)};
                })),
            halftone::halftone(density, pause.map(|v| !v)).style(mue_macroquad::style! {
                height: {vh(22.5)};
                width: 100%;
                flex: none;
            }),
            // Vector paths fit into their node; SVG path data and documents are supported.
            hstack().children((
                path(PathData::circle(vec2(50., 50.), 40.))
                    .fill(BLUE)
                    .style(mue_macroquad::style! { width: 100px; height: 100px; }),
                path(PathData::rect(Rect::new(0., 0., 120., 70.)))
                    .stroke(CORAL)
                    .stroke_width(2.)
                    .style(mue_macroquad::style! { width: 120px; height: 70px; }),
                path(PathData::parse("M4 12l5 5L20 6").unwrap())
                    .view_box(Rect::new(0., 0., 24., 24.))
                    .stroke_current_color()
                    .stroke_style(mue_macroquad::path::StrokeStyle::round(2.5))
                    .style(mue_macroquad::style! { width: 72px; height: 72px; color: #7fd1a8; }),
            )),
        ))
        .into_node()
}
