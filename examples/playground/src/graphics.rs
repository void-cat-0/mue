use super::{controls::*, halftone, responsive};
use macroquad::texture::Texture2D;
use mue_components::*;
use mue_macroquad::{
    math::{Rect, vec2},
    shader::{Brush, ColorStop, LinearGradient, RadialGradient, SharedTexture},
    style::{Style, Styleable, percent, vh},
};

pub fn page() -> Node {
    let photo: SharedTexture =
        Texture2D::from_file_with_format(include_bytes!("../../color/assets/photo.png"), None)
            .into();
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
            ("Solid", Brush::Solid(BLUE)),
            (
                "Linear",
                LinearGradient::new(
                    vec2(0., 0.),
                    vec2(1., 1.),
                    vec![ColorStop::new(0., BLUE), ColorStop::new(1., CORAL)],
                )
                .into(),
            ),
            (
                "Radial",
                RadialGradient::new(
                    vec2(0.5, 0.5),
                    0.7,
                    vec![ColorStop::new(0., CORAL), ColorStop::new(1., BLUE)],
                )
                .into(),
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
                    Brush::Solid(CORAL),
                    Brush::Solid(mue_macroquad::color::Color::WHITE),
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
            halftone::halftone(
                Style::new().style(mue_macroquad::style! {
                height: {vh(22.5)};
                width: 100%;
                flex: none;
                }),
                density,
                pause.map(|v| !v),
            ),
            // Shapes use the same paint API; a custom node can draw cached geometry.
            shape(mue_core::prop::Prop::Static(
                mue_macroquad::paint::Shapes::build(|p| {
                    p.fill_circle(vec2(50., 50.), 40., BLUE);
                    p.stroke_rect(Rect::new(110., 15., 120., 70.), CORAL);
                }),
            ))
            .style(mue_macroquad::style! { height: 100px; }),
        ))
        .into_node()
}
