//! Foundation features: borders, outlines, per-corner radii, and vector paths.
//!
//! Run with `cargo run --manifest-path examples/Cargo.toml --example foundation`.

mod capture;

use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color,
    math::{Rect, Vector, vec2},
    shader::linear_gradient,
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
                            mue_macroquad::shader::ColorStop::new(0., Color::from_rgba(255, 110, 150, 255)),
                            mue_macroquad::shader::ColorStop::new(1., Color::from_rgba(110, 170, 255, 255)),
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

fn ui() -> impl Component {
    stack()
        .style(style! { width: 100%; height: 100%; padding: 32px; gap: 32px; background: #161b26; })
        .children((
            heading("Foundation").style(style! { color: #ffffff; }),
            paint_row(),
            vector_row(),
            grid_row(),
            focus_row(),
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
    let mut app = App::new(ui());
    let mut first = true;
    loop {
        clear_background(macroquad::color::BLACK);
        app.frame();
        if std::mem::take(&mut first) && capture.enabled() {
            // Show a keyboard focus ring in captured screenshots.
            mue_macroquad::focus::move_focus(true);
        }
        if capture.after_draw() {
            break;
        }
        next_frame().await;
    }
}
