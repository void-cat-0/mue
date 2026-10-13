use super::{controls::*, halftone};
use macroquad::texture::Texture2D;
use mue_macroquad::{
    animation::animated_with,
    math::{Rect, vec2},
    node::*,
    shader::{Brush, ColorStop, LinearGradient, RadialGradient, SharedTexture},
    style::{Style, Styleable, StyleableExt},
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
    let density = animated_with(density.select(0.3, 0.08), SPRING).value;
    stack()
        .gap(20.)
        .children((
            controls(vec![a, b, c, d, e, f, g, h, i, j]),
            image(photo)
                .width(wide.select(700., 380.))
                .height(tall.select(350., 220.))
                .animation(SPRING)
                .object_fit(fit)
                .object_position(position)
                .region(crop.map(|on| on.then_some(Rect::new(100., 100., 400., 300.))))
                .color(tint.select(
                    Brush::Solid(CORAL),
                    Brush::Solid(mue_macroquad::color::Color::WHITE),
                )),
            center()
                .height(100.)
                .radius(radius.select(50., 8.))
                .background(brush)
                .animation(SPRING)
                .children(heading("Brush").font_size(28.)),
            halftone::halftone(
                Style::new().height(180.).w_full().flex_none(),
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
            .height(100.),
        ))
        .into_node()
}
