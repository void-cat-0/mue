//! Foundation features: borders, outlines, per-corner radii, and vector paths.
//!
//! Run with `cargo run --manifest-path examples/Cargo.toml --example foundation`.

mod capture;

use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App, Shadow,
    math::{Rect, Vector, vec2},
    motion::presence::{PresenceAnimation, presence},
    node::{NodeHandle, shader_box},
    overlay::anchor::{Placement, anchored},
    overlay::layer::{Layer, teleport},
    paint::color::Color,
    paint::offscreen::{Filter, offscreen},
    paint::shader::linear_gradient,
};

const ICON: &str = r##"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
    stroke-linecap="round" stroke-linejoin="round">
  <circle cx="12" cy="12" r="9"/>
  <path d="M12 7v5l3 3"/>
</svg>"##;

fn swatch(label: &'static str, card: Style) -> impl Component {
    stack()
        .style(style! { align-items: center; gap: 8px; })
        .children((
            flexbox().style(card),
            text(label).style(style! { font-size: 14px; line-height: 18px; color: #c9d1e0; }),
        ))
}

fn paint_row() -> impl Component {
    hstack().style(style! { gap: 28px; }).children((
        swatch(
            "border",
            style! { width: 96px; height: 64px; background: #2b3446; border: 3px solid #6fa8ff; border-radius: 12px; },
        ),
        swatch(
            "per-side",
            style! {
                width: 96px; height: 64px; background: #2b3446;
                border-width: 1px 6px 1px 6px; border-color: #f2b45c;
            },
        ),
        swatch(
            "corners",
            style! { width: 96px; height: 64px; background: #8a6fd6; border-radius: 28px 4px 28px 4px; },
        ),
        swatch(
            "outline",
            style! {
                width: 96px; height: 64px; background: #2b3446; border-radius: 10px;
                outline: 2px #7fd1a8; outline-offset: 4px;
            },
        ),
        swatch(
            "gradient border",
            Style::new()
                .width(96.)
                .height(64.)
                .background(Color::from_rgba(43, 52, 70, 255))
                .radius(18.)
                .border_all(
                    4.,
                    linear_gradient(
                        vec2(0., 0.),
                        vec2(1., 1.),
                        [
                            mue_macroquad::paint::shader::ColorStop::new(0., Color::from_rgba(255, 110, 150, 255)),
                            mue_macroquad::paint::shader::ColorStop::new(1., Color::from_rgba(110, 170, 255, 255)),
                        ],
                    ),
                ),
        ),
    ))
}

fn vector_row() -> impl Component {
    let star = PathData::polygon((0..10).map(|i| {
        let angle = i as f32 * std::f32::consts::PI / 5. - std::f32::consts::FRAC_PI_2;
        let radius = if i % 2 == 0 { 1. } else { 0.45 };
        Vector::new(angle.cos(), angle.sin()) * radius
    }));
    let doc = SvgDocument::parse(ICON).expect("valid icon");
    hstack().style(style! { gap: 28px; }).children((
        path(PathData::circle(Vector::ZERO, 1.))
            .style(style! { width: 64px; height: 64px; color: #ef925a; }),
        path(star).style(style! { width: 64px; height: 64px; color: #ffd36e; }),
        path(PathData::rect(Rect::new(0., 0., 3., 2.)))
            .stroke(Color::from_rgba(111, 168, 255, 255))
            .stroke_width(0.08)
            .style(style! { width: 96px; height: 64px; }),
        path(PathData::parse("M4 12l5 5L20 6").unwrap())
            .view_box(Rect::new(0., 0., 24., 24.))
            .stroke_current_color()
            .stroke_style(StrokeStyle::round(2.5))
            .style(style! { width: 64px; height: 64px; color: #7fd1a8; }),
        svg(doc.clone()).style(style! { width: 64px; height: 64px; color: #c9d1e0; }),
        svg(doc).style(style! {
            width: 64px; height: 64px; color: #ffffff; padding: 12px;
            background: #3b6fd6; border-radius: 32px;
        }),
    ))
}

fn icon_row() -> impl Component {
    // The application registers its own icon artwork once.
    register_icon("clock", SvgDocument::parse(ICON).expect("valid icon"));
    register_icon(
        "check",
        IconSource::stroked(PathData::parse("M5 12.5l4.5 4.5L19 7.5").unwrap(), 24., 2.),
    );
    hstack()
        .style(style! { gap: 10px; font-size: 22px; color: #c9d1e0; })
        .children((
            icon_named("clock"),
            text("Icons follow font size and color"),
            icon_named("check").style(style! { color: #7fd1a8; font-size: 30px; }),
        ))
}

fn grid_row() -> impl Component {
    let tile = |label: &'static str| {
        center()
            .style(style! { height: 44px; background: #26304a; border-radius: 8px; })
            .children(
                text(label).style(style! { font-size: 14px; line-height: 18px; color: #c9d1e0; }),
            )
    };
    grid()
        .style(style! {
            width: 100%;
            grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
            grid-auto-rows: 44px;
            gap: 8px;
        })
        .children((
            tile("grid-column: 1 / -1").style(style! { grid-column: 1 / -1; background: #3b6fd6; }),
            tile("span 2").style(style! { grid-column: span 2; }),
            tile("auto"),
            tile("auto"),
            tile("auto"),
            tile("auto"),
        ))
}

fn focus_row() -> impl Component {
    let presses = signal(0_u32);
    let label =
        presses.map(|n| format!("Tab / Shift+Tab between buttons; Enter or Space presses ({n})"));
    hstack().style(style! { gap: 12px; }).children((
        button("Primary")
            .primary(true)
            .on_activate(move || presses.set(presses.get() + 1)),
        button("Secondary").on_activate(move || presses.set(presses.get() + 1)),
        flexbox()
            .focusable(true)
            .focus_ring(2., Color::from_rgba(127, 209, 168, 255), 2.)
            .cursor(Cursor::Text)
            .style(style! { padding: 10px 14px; background: #222a3b; border-radius: 8px; })
            .children(
                text(label).style(style! { font-size: 14px; line-height: 20px; color: #c9d1e0; }),
            ),
    ))
}

fn menu_item(label: &'static str) -> impl Component {
    flexbox()
        .focusable(true)
        .cursor(Cursor::Pointer)
        .with_hovered(|style, hovered| {
            style.background(hovered.map(|hovered| {
                mue_macroquad::paint::shader::Brush::Solid(if hovered {
                    Color::from_rgba(255, 255, 255, 24)
                } else {
                    Color::TRANSPARENT
                })
            }))
        })
        .focus_ring(2., Color::from_rgba(120, 170, 255, 255), -2.)
        .style(style! { padding: 8px 12px; border-radius: 6px; })
        .children(text(label).style(style! { font-size: 14px; line-height: 18px; color: #e6ebf5; }))
}

fn layers_row(menu_open: bool) -> impl Component {
    let menu = signal(menu_open);
    let dialog = signal(false);
    let trigger = NodeHandle::new();
    hstack().style(style! { gap: 12px; }).children((
        button("Menu ▾")
            .node_handle(trigger)
            .on_activate(move || menu.set(!menu.get())),
        button("Open dialog").on_activate(move || dialog.set(true)),
        // Popover: built here, drawn in the Popover layer, attached to the button.
        teleport(Layer::Popover, move || {
            presence(menu, move || {
                anchored(trigger)
                    .placement(Placement::BottomStart)
                    .offset(6.)
                    .style(style! {
                        padding: 6px; background: #2a3345; border-radius: 10px;
                        border: 1px solid #3b4862; flex-direction: column; min-width: 180px;
                    })
                    .shadow(Shadow::new(18., 10.))
                    .children((menu_item("Rename"), menu_item("Duplicate"), menu_item("Delete")))
            })
            .animation(PresenceAnimation::fade().slide(0., -6.))
        }),
        overlay_host(
            dialog,
            stack()
                .style(style! { padding: 24px; gap: 16px; background: #222a3b; border-radius: 14px; width: 320px; })
                .children((
                    heading("Dialog").level(HeadingLevel::H4).style(style! { color: #ffffff; }),
                    text("Tab stays inside. Escape or the backdrop closes it.")
                        .style(style! { font-size: 14px; line-height: 20px; color: #c9d1e0; }),
                    button("Close").primary(true).on_activate(move || dialog.set(false)),
                )),
        )
        .dismissible(true)
        .on_dismiss(move || dialog.set(false)),
    ))
}

const STRIPES: &str = r#"#version 100
precision mediump float;
varying highp vec2 v_pos;
varying lowp vec4 v_color;
uniform float u_time;
uniform vec4 u_rect;
void main() {
    // Diagonal stripes sliding over time.
    float w = (v_pos.x - u_rect.x) * 0.79 + (v_pos.y - u_rect.y) * 0.61 - u_time * 12.0;
    float band = step(mod(w, 14.0), 8.0);
    gl_FragColor = vec4(v_color.rgb, v_color.a * (0.06 + 0.10 * band));
}
"#;

fn pair() -> impl Component {
    let dot = |color: &'static str, left: f32| {
        flexbox()
            .absolute()
            .inset_left(left)
            .inset_top(8.)
            .width(48.)
            .height(48.)
            .radius(24.)
            .background(match color {
                "a" => Color::from_rgba(239, 146, 90, 255),
                _ => Color::from_rgba(111, 168, 255, 255),
            })
    };
    flexbox()
        .relative()
        .width(96.)
        .height(64.)
        .children((dot("a", 8.), dot("b", 40.)))
}

fn effects_row() -> impl Component {
    let stripes = mue_macroquad::UiMaterial::new(STRIPES, Vec::new(), Vec::new()).expect("stripes");
    let backdrop = flexbox().relative().width(220.).height(96.).children((
        flexbox().absolute().inset_left(0.).inset_top(0.).width(80.).height(80.).radius(40.)
            .background(Color::from_rgba(239, 146, 90, 255)),
        flexbox().absolute().inset_left(120.).inset_top(16.).width(90.).height(70.).radius(12.)
            .background(Color::from_rgba(111, 168, 255, 255)),
        text("GLASS").style(style! { position: absolute; left: 60px; top: 30px; font-size: 26px; line-height: 30px; color: #ffd36e; }),
        center()
            .style(style! {
                position: absolute; left: 36px; top: 20px; width: 150px; height: 56px;
                border-radius: 14px; background: rgba(255, 255, 255, 0.14);
                border: 1px solid rgba(255, 255, 255, 0.35);
            })
            .backdrop_blur(10.)
            .children(text("backdrop blur").style(style! { font-size: 14px; line-height: 18px; color: #ffffff; })),
    ));
    hstack()
        .style(style! { gap: 28px; align-items: start; })
        .children((
        swatch_node("opacity 0.5", pair().opacity(0.5)),
        swatch_node("group opacity", offscreen().opacity(0.5).children(pair())),
        swatch_node(
            "blur(4)",
            offscreen().filter(Filter::Blur(4.)).children(pair()),
        ),
        swatch_node(
            "grayscale",
            offscreen().filter(Filter::Grayscale(1.)).children(pair()),
        ),
        swatch_node(
            "shader_box",
            shader_box(stripes).style(style! {
                width: 96px; height: 64px; border-radius: 10px; background: #26304a; color: #ffffff;
            }),
        ),
        backdrop,
    ))
}

fn swatch_node(label: &'static str, content: impl IntoNode + 'static) -> impl Component {
    stack()
        .style(style! { align-items: center; gap: 8px; })
        .children((
            content.into_node(),
            text(label).style(style! { font-size: 14px; line-height: 18px; color: #c9d1e0; }),
        ))
}

fn ui(capture: bool) -> impl Component {
    stack()
        .style(style! { width: 100%; height: 100%; padding: 32px; gap: 32px; background: #161b26; })
        .children((
            heading("Foundation").style(style! { color: #ffffff; }),
            paint_row(),
            vector_row(),
            icon_row(),
            grid_row(),
            focus_row(),
            effects_row(),
            layers_row(capture),
        ))
}

fn conf() -> Conf {
    Conf {
        window_title: "foundation".into(),
        window_width: 900,
        window_height: 1000,
        ..Default::default()
    }
}

#[macroquad::main(conf)]
async fn main() {
    let mut capture = capture::Capture::from_env();
    let mut app = App::new(ui(capture.enabled()));
    let mut first = true;
    loop {
        clear_background(macroquad::color::BLACK);
        app.frame();
        if std::mem::take(&mut first) && capture.enabled() {
            // Show a keyboard focus ring in captured screenshots.
            mue_macroquad::input::focus::move_focus(true);
        }
        if capture.after_draw() {
            break;
        }
        next_frame().await;
    }
}
