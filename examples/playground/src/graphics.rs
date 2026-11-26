use super::{assets, controls::*, halftone};
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    Shadow,
    math::{Rect, Vector, vec2},
    paint::color::Color,
    paint::offscreen::{Filter, offscreen},
    paint::shader::{ColorStop, linear_gradient, radial_gradient, solid},
    style::{Styleable, StyleableExt},
};
use std::rc::Rc;

const PAGE: &str = "Graphics";

const CLOCK: &str = r##"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
    stroke-linecap="round" stroke-linejoin="round">
  <circle cx="12" cy="12" r="9"/>
  <path d="M12 7v5l3 3"/>
</svg>"##;

/// A demo with its caption underneath.
fn swatch(label: &'static str, content: impl IntoNode + 'static) -> Node {
    stack()
        .style(mue_macroquad::style! {
            flex: none;
            align-items: center;
            gap: {u(6.)};
            padding: {(u(8.),)};
        })
        .children((
            content.into_node(),
            text(label).style(mue_macroquad::style! {
                font-size: {f(12.)}; line-height: {u(16.)};
                color: {muted()};
            }),
        ))
        .into_node()
}

fn image_card() -> Node {
    let photo = assets::background_texture(include_bytes!("../../color/assets/photo.png"));
    let (fit, fit_control) = segmented(
        "Object fit",
        vec![
            ("Cover", ObjectFit::Cover),
            ("Contain", ObjectFit::Contain),
            ("Fill", ObjectFit::Fill),
            ("Scale down", ObjectFit::ScaleDown),
            ("None", ObjectFit::None),
        ],
    );
    let (position, position_control) = segmented(
        "Position",
        vec![
            ("Center", vec2(0.5, 0.5)),
            ("Top left", vec2(0., 0.)),
            ("Bottom right", vec2(1., 1.)),
        ],
    );
    let (crop, crop_control) = switch("Crop region", false);
    let (tint, tint_control) = switch("Tint", false);
    let (round, round_control) = switch("Rounded", false);
    card(
        "Images",
        "A photo decoded on a worker thread, then fitted into a fixed box. The box outline is the node's bounds.",
        (
            fit_control,
            position_control,
            row((crop_control, tint_control, round_control)),
            image(photo)
                .style(mue_macroquad::style! {
                    width: 100%;
                    height: {u(220.)};
                    border-width: 1px;
                    border-color: {border()};
                    border-radius: {round.select(24., 0.)};
                    background: {raised()};
                })
                .object_fit(fit)
                .object_position(position)
                .region(crop.map(|on| on.then_some(Rect::new(100., 100., 400., 300.))))
                .style(mue_macroquad::style! {
                    color: {tint.select(solid(CORAL), solid(Color::WHITE))};
                }),
        ),
    )
}

fn paint_card() -> Node {
    let gradient = linear_gradient(
        vec2(0., 0.),
        vec2(1., 1.),
        [
            ColorStop::new(0., Color::from_rgba(255, 110, 150, 255)),
            ColorStop::new(1., BLUE),
        ],
    );
    let box_style = || {
        mue_macroquad::style! {
            width: {u(96.)}; height: {u(64.)};
            background: {raised()};
        }
    };
    card(
        "Backgrounds, borders, and outlines",
        "Gradients, per-side borders, per-corner radii, outlines with an offset, and gradient borders.",
        row((
            swatch("solid", flexbox().style(box_style()).style(mue_macroquad::style! { background: {BLUE}; border-radius: 10px; })),
            swatch(
                "linear",
                flexbox().style(box_style()).style(mue_macroquad::style! { border-radius: 10px; }).background(gradient.clone()),
            ),
            swatch(
                "radial",
                flexbox()
                    .style(box_style())
                    .style(mue_macroquad::style! { border-radius: 32px; })
                    .background(radial_gradient(
                        (0.5, 0.5),
                        0.7,
                        [ColorStop::new(0., CORAL), ColorStop::new(1., Color::from_rgba(40, 50, 90, 255))],
                    )),
            ),
            swatch("border", flexbox().style(box_style()).style(mue_macroquad::style! { border: 3px solid #6fa8ff; border-radius: 12px; })),
            swatch(
                "per side",
                flexbox().style(box_style()).style(mue_macroquad::style! { border-width: 1px 6px 1px 6px; border-color: #f2b45c; }),
            ),
            swatch(
                "corners",
                flexbox().style(box_style()).style(mue_macroquad::style! { background: #8a6fd6; border-radius: 28px 4px 28px 4px; }),
            ),
            swatch(
                "outline",
                flexbox().style(box_style()).style(mue_macroquad::style! { border-radius: 10px; outline: 2px #7fd1a8; outline-offset: 4px; }),
            ),
            swatch(
                "gradient border",
                flexbox().style(box_style()).radius(18.).border_all(4., gradient),
            ),
        )),
    )
}

fn shadow_card() -> Node {
    let (elevation, elevation_control) = segmented(
        "Elevation",
        vec![("2", 2.), ("8", 8.), ("16", 16.), ("32", 32.)],
    );
    let (radius, radius_control) =
        segmented("Corner radius", vec![("0", 0.), ("12", 12.), ("40", 40.)]);
    let shadow = computed(move |_| Shadow::new(elevation.get(), radius.get()));
    card(
        "Shadows",
        "Soft shadows follow the rounded shape and animate with it.",
        (
            elevation_control,
            radius_control,
            center()
                .style(mue_macroquad::style! { width: 100%; height: {u(180.)}; })
                .children(
                    center()
                        .style(mue_macroquad::style! {
                            width: {u(180.)}; height: {u(100.)};
                            background: {panel_brush()};
                            border-radius: {radius.map(mue_macroquad::style::Size::Px)};
                            transition: 200ms ease-out;
                        })
                        .shadow(shadow)
                        .children(text(elevation.map(|e| Rc::<str>::from(format!("elevation {e:.0}")))).style(
                            mue_macroquad::style! { font-size: {f(14.)}; line-height: {u(20.)}; color: {muted()}; },
                        )),
                ),
        ),
    )
}

fn vector_card() -> Node {
    register_icon("clock", SvgDocument::parse(CLOCK).expect("valid icon"));
    register_icon(
        "check",
        IconSource::stroked(PathData::parse("M5 12.5l4.5 4.5L19 7.5").unwrap(), 24., 2.),
    );
    let star = PathData::polygon((0..10).map(|i| {
        let angle = i as f32 * std::f32::consts::PI / 5. - std::f32::consts::FRAC_PI_2;
        let radius = if i % 2 == 0 { 1. } else { 0.45 };
        Vector::new(angle.cos(), angle.sin()) * radius
    }));
    let doc = SvgDocument::parse(CLOCK).expect("valid icon");
    let size = || mue_macroquad::style! { width: {u(56.)}; height: {u(56.)}; };
    card(
        "Vector paths, SVG, and icons",
        "Paths scale to their box without blurring; icons follow the font size and text color.",
        (
            row((
                swatch(
                    "circle",
                    path(PathData::circle(Vector::ZERO, 1.))
                        .style(size())
                        .style(mue_macroquad::style! { color: #ef925a; }),
                ),
                swatch(
                    "star",
                    path(star)
                        .style(size())
                        .style(mue_macroquad::style! { color: #ffd36e; }),
                ),
                swatch(
                    "stroke",
                    path(PathData::rect(Rect::new(0., 0., 3., 2.)))
                        .stroke(BLUE)
                        .stroke_width(0.08)
                        .style(mue_macroquad::style! { width: {u(84.)}; height: {u(56.)}; }),
                ),
                swatch(
                    "path data",
                    path(PathData::parse("M4 12l5 5L20 6").unwrap())
                        .view_box(Rect::new(0., 0., 24., 24.))
                        .stroke_current_color()
                        .stroke_style(StrokeStyle::round(2.5))
                        .style(size())
                        .style(mue_macroquad::style! { color: #7fd1a8; }),
                ),
                swatch(
                    "svg",
                    svg(doc.clone())
                        .style(size())
                        .style(mue_macroquad::style! { color: {foreground()}; }),
                ),
                swatch(
                    "svg on a badge",
                    svg(doc).style(size()).style(mue_macroquad::style! {
                        color: #ffffff; padding: {(u(12.),)};
                        background: #3b6fd6; border-radius: 9999px;
                    }),
                ),
            )),
            row((
                icon_named("clock")
                    .style(mue_macroquad::style! { font-size: {f(16.)}; color: {foreground()}; }),
                icon_named("clock")
                    .style(mue_macroquad::style! { font-size: {f(24.)}; color: {foreground()}; }),
                icon_named("check")
                    .style(mue_macroquad::style! { font-size: {f(32.)}; color: #7fd1a8; }),
                hint("Icons at 16, 24, and 32"),
            )),
        ),
    )
}

fn pair() -> FlexboxBuilder {
    let dot = |color: Color, left: f32| {
        flexbox()
            .absolute()
            .inset_left(left)
            .inset_top(8.)
            .width(48.)
            .height(48.)
            .radius(24.)
            .background(color)
    };
    flexbox()
        .relative()
        .width(96.)
        .height(64.)
        .children((dot(CORAL, 8.), dot(BLUE, 40.)))
}

fn effects_card() -> Node {
    let (density, density_control) =
        segmented("Halftone density", vec![("Fine", 0.3), ("Coarse", 0.08)]);
    let (moving, moving_control) = switch("Animate shader", true);
    let shader_view = halftone::halftone(density, moving).style(mue_macroquad::style! {
        width: 100%;
        height: {u(120.)};
        flex: none;
    });
    let glass = flexbox().relative().width(220.).height(96.).children((
        flexbox().absolute().inset_left(0.).inset_top(0.).width(80.).height(80.).radius(40.).background(CORAL),
        flexbox().absolute().inset_left(120.).inset_top(16.).width(90.).height(70.).radius(12.).background(BLUE),
        text("GLASS").style(mue_macroquad::style! { position: absolute; left: 60px; top: 30px; font-size: 26px; line-height: 30px; color: #ffd36e; }),
        center()
            .style(mue_macroquad::style! {
                position: absolute; left: 36px; top: 20px; width: 150px; height: 56px;
                border-radius: 14px; background: rgba(255, 255, 255, 0.14);
                border: 1px solid rgba(255, 255, 255, 0.35);
            })
            .backdrop_blur(10.)
            .children(text("backdrop blur").style(mue_macroquad::style! { font-size: 14px; line-height: 18px; color: #ffffff; })),
    ));
    card(
        "Layer effects and shaders",
        "Group opacity draws the pair as one layer (no darker overlap), filters apply to the whole group, and backdrop blur frosts what is behind.",
        (
            row((
                swatch("opacity 0.5", pair().opacity(0.5)),
                swatch("group opacity", offscreen().opacity(0.5).children(pair())),
                swatch(
                    "blur(4)",
                    offscreen().filter(Filter::Blur(4.)).children(pair()),
                ),
                swatch(
                    "grayscale",
                    offscreen().filter(Filter::Grayscale(1.)).children(pair()),
                ),
                swatch("backdrop blur", glass),
            )),
            density_control,
            moving_control,
            shader_view,
        ),
    )
}

pub fn page() -> Node {
    page_body(
        "Images, paint, shadows, vectors, layer effects, and custom shaders.",
        (
            image_card(),
            paint_card(),
            shadow_card(),
            vector_card(),
            effects_card(),
            checklist(
                PAGE,
                &[
                    "Each object-fit mode places the photo as labeled",
                    "Borders, radii, and outlines are crisp at this DPI",
                    "Shadows are soft with no bright halo",
                    "Group opacity shows no darker overlap; blur and grayscale apply",
                    "The halftone shader animates smoothly",
                ],
            ),
        ),
    )
}
