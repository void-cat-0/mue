use super::{assets, controls::*, halftone};
use mue_components::system::{ControlSize, Palette};
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    BoxShadow, BoxShadows,
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
    let (crop, crop_control) = option_switch("Crop region", false);
    let (tint, tint_control) = option_switch("Tint", false);
    let (round, round_control) = option_switch("Rounded", false);
    card(
        "Images",
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
    let black = |alpha: u8| Color::from_rgba(0, 0, 0, alpha);
    let (shadow, shadow_control) = segmented(
        "Shadow",
        vec![
            ("Elevation 4", BoxShadows::elevation(4.)),
            ("Elevation 12", BoxShadows::elevation(12.)),
            ("Elevation 24", BoxShadows::elevation(24.)),
            ("Hard offset", BoxShadow::new(8., 8., 0., black(160)).into()),
            (
                "Glow + spread",
                BoxShadow::new(0., 0., 24., BLUE).spread(4.).into(),
            ),
            (
                "Layered",
                BoxShadows::from([
                    BoxShadow::new(0., 1., 2., black(90)),
                    BoxShadow::new(0., 12., 32., black(70)).spread(-4.),
                    BoxShadow::new(-16., -16., 0., CORAL),
                ]),
            ),
        ],
    );
    let (radius, radius_control) =
        segmented("Corner radius", vec![("0", 0.), ("12", 12.), ("40", 40.)]);
    card(
        "Box shadows",
        (
            shadow_control,
            radius_control,
            center()
                .style(mue_macroquad::style! { width: 100%; height: {u(200.)}; })
                .children(
                    center()
                        .style(mue_macroquad::style! {
                            width: {u(180.)}; height: {u(100.)};
                            background: {panel_brush()};
                            border-radius: {radius.map(mue_macroquad::style::Size::Px)};
                            transition: 200ms ease-out;
                        })
                        .box_shadow(shadow)
                        .children(text("box-shadow").style(mue_macroquad::style! {
                            font-size: {f(14.)}; line-height: {u(20.)}; color: {muted()};
                        })),
                ),
        ),
    )
}

fn corner_card() -> Node {
    use mue_macroquad::style::CornerShape;
    let (radius, radius_control) = segmented("Radius", vec![("16", 16.), ("32", 32.), ("48", 48.)]);
    let shapes = [
        ("round", CornerShape::Round),
        ("squircle", CornerShape::Squircle),
        ("superellipse(3)", CornerShape::Superellipse(3.)),
        ("bevel", CornerShape::Bevel),
        ("scoop", CornerShape::Scoop),
        ("notch", CornerShape::Notch),
    ];
    card(
        "Corner shapes",
        (
            radius_control,
            row(shapes
                .into_iter()
                .map(|(label, shape)| {
                    swatch(
                        label,
                        flexbox()
                            .style(mue_macroquad::style! {
                                width: {u(110.)}; height: {u(80.)};
                                background: {BLUE};
                                border: 2px solid #ffffff;
                                outline: 2px #7fd1a8; outline-offset: 3px;
                                border-radius: {radius.map(mue_macroquad::style::Size::Px)};
                            })
                            .corner_shape(shape),
                    )
                })
                .collect::<Vec<_>>()),
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
    let (moving, moving_control) = option_switch("Animate shader", true);
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

fn stripes_card() -> Node {
    // Phira's defaults.
    const ANGLE: f32 = 38.;
    const PERIOD: f32 = 48.;
    const DUTY: f32 = 0.5;
    const SPEED: f32 = 18.;
    const INTENSITY: f32 = 0.07;
    let angle = signal(ANGLE);
    let period = signal(PERIOD);
    let duty = signal(DUTY);
    let speed = signal(SPEED);
    let intensity = signal(INTENSITY);
    let tint = signal(Color::WHITE);
    let color = computed(move |_| {
        let tint = tint.get();
        Color::new(tint.r, tint.g, tint.b, intensity.get())
    });
    let reset = move || {
        angle.set(ANGLE);
        period.set(PERIOD);
        duty.set(DUTY);
        speed.set(SPEED);
        intensity.set(INTENSITY);
        tint.set(Color::WHITE);
    };
    let control = |label: &'static str, value: Signal<f32>, min: f32, max: f32, step: f32, unit: &'static str| {
        field(
            label,
            hstack()
                .style(mue_macroquad::style! { width: 100%; gap: {u(12.)}; align-items: center; })
                .children((
                    flexbox()
                        .style(mue_macroquad::style! { flex: 1; min-width: 0px; })
                        .children(slider().range(min, max).step(step).value(value).palette(Palette::Purple)),
                    text(value.map(move |value| {
                        Rc::<str>::from(if step < 1. { format!("{value:.2}{unit}") } else { format!("{value:.0}{unit}") })
                    }))
                    .style(mue_macroquad::style! {
                        flex: none; width: {u(64.)};
                        font-size: {f(13.)}; line-height: {u(18.)};
                        font-family: monospace;
                        text-align: right;
                        color: {foreground()};
                    }),
                )),
        )
    };
    let code = computed(move |_| {
        Rc::<str>::from(format!(
            "stripes()\n    .angle({:.2})  // {:.0}°\n    .period({:.0}.)\n    .duty({:.2})\n    .speed({:.0}.)\n    .intensity({:.2})",
            angle.get().to_radians(),
            angle.get(),
            period.get(),
            duty.get(),
            speed.get(),
            intensity.get(),
        ))
    });
    card(
        "Stripes",
        (
            stack()
                .style(mue_macroquad::style! {
                    position: relative;
                    width: 100%;
                    height: {u(220.)};
                    border-radius: {u(14.)};
                    overflow: hidden;
                    align-items: center;
                    justify-content: center;
                })
                .background(linear_gradient(
                    vec2(0., 0.),
                    vec2(1., 1.),
                    [
                        ColorStop::new(0., Color::from_rgba(42, 50, 60, 255)),
                        ColorStop::new(0.55, Color::from_rgba(38, 33, 74, 255)),
                        ColorStop::new(1., Color::from_rgba(76, 34, 78, 255)),
                    ],
                ))
                .children((
                    stripes()
                        .angle(angle.map(|degrees| degrees.to_radians()))
                        .period(period)
                        .duty(duty)
                        .speed(speed)
                        .color(color)
                        .style(mue_macroquad::style! {
                            position: absolute;
                            left: 0px; top: 0px; width: 100%; height: 100%;
                        }),
                    text("PHIRA").style(mue_macroquad::style! {
                        font-size: {f(40.)}; line-height: {u(48.)};
                        font-weight: 800;
                        color: #ffffff;
                    }),
                )),
            control("Angle", angle, 0., 180., 1., "°"),
            control("Period", period, 8., 160., 1., " px"),
            control("Duty", duty, 0.05, 0.95, 0.01, ""),
            control("Speed", speed, -80., 80., 1., " px/s"),
            control("Intensity", intensity, 0., 0.4, 0.01, ""),
            field(
                "Color",
                picker(
                    vec![
                        ("White", Color::WHITE),
                        ("Sky", Color::from_rgba(120, 190, 255, 255)),
                        ("Pink", Color::from_rgba(255, 120, 200, 255)),
                        ("Black", Color::BLACK),
                    ],
                    tint,
                ),
            ),
            row((button("Restore defaults").size(ControlSize::Sm).on_activate(reset),)),
            code_block(code).style(mue_macroquad::style! {
                width: 100%; min-width: 0px;
                font-size: {f(13.)}; line-height: {u(19.)};
                padding: {(u(12.),)};
                border-radius: {u(8.)};
            }),
        ),
    )
}

pub fn page() -> Node {
    page_body(
        (
            image_card(),
            paint_card(),
            corner_card(),
            shadow_card(),
            vector_card(),
            effects_card(),
            stripes_card(),
            checklist(
                PAGE,
                &[
                    "Each object-fit mode places the photo as labeled",
                    "Borders, radii, and outlines are crisp at this DPI",
                    "Shadows are soft with no bright halo",
                    "Group opacity shows no darker overlap",
                    "Blur and grayscale apply to the whole group",
                    "The halftone shader animates smoothly",
                    "Stripes follow every slider, and speed 0 stops them",
                ],
            ),
        ),
    )
}
