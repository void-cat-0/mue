//! Declarative styles applied to ordinary Rust component builders.
use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    math::{Rect, Vector},
    motion::transition::Transition,
    paint::Shapes,
    paint::color::Color,
    paint::shader::SharedTexture,
};

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue · styles + components".into(),
            window_width: 900,
            window_height: 700,
            ..Default::default()
        },
        run(),
    );
}

async fn run() {
    let active = signal(false);
    let offset = signal(Vector::ZERO);
    let texture: SharedTexture = macroquad::texture::Texture2D::from_file_with_format(
        include_bytes!("../../../deer.png"),
        None,
    )
    .into();
    let geometry = Shapes::build(|paint| {
        paint.fill(
            &PathData::rounded_rect(Rect::new(8., 10., 80., 60.), 12.),
            FillRule::NonZero,
            Color::from_rgba(116, 91, 196, 255),
        );
    });
    let idle = style! { transform: rotate(-6deg) scale(1); }
        .take_transform_list()
        .get_clone();
    let turned = style! { transform: rotate(12deg) scale(1.1); }
        .take_transform_list()
        .get_clone();
    let card = flexbox()
        .style(style! {
        width: 220px;
                    min-height: 120px;
                    padding: 20px;
                    background: #e6f0ff;
                    color: #244261;
                    border-radius: 16px;
                    align-items: center;
                    justify-content: center;
                    transform-origin: center;
        transform: {active.select(turned, idle)};
        transition: {Transition::ease_in_out(0.3)};
        })
        .on_activate(move || active.toggle())
        .children(text("Click to transform").style(style! {
            font-size: 18px;
            text-align: center;
            color: #244261;
        }));

    let mut app = App::new(
        scroll_area()
            .offset(offset)
            .style(style! {
                width: 100%;
                height: 100%;
                background-color: #f4f6fa;
                font-size: 18px;
                line-height: 1.5;
            })
            .children(
                stack().style(style! {
                    width: 100%;
                    max-width: 880px;
                    margin: 0 auto;
                    padding: 32px 28px;
                    gap: 24px;
                }).children((
                    heading("Styles. Rust components.").style(style! {
                        font-size: 30px;
                        line-height: 1.3;
                        color: #17243c;
                    }),
                    text("style! accepts familiar declarations and {Rust expressions}.")
                        .style(style! { color: #51617a; }),
                    flexbox().style(style! {
                        flex-wrap: wrap;
                        align-items: center;
                        gap: 24px 40px;
                        padding: 20px 8px;
                    }).children((
                        card,
                        path(PathData::circle(Vector::ZERO, 1.)).style(style! { width: 76px; height: 76px; color: #ef925a; }),
                        shape(geometry).style(style! { width: 96px; height: 80px; }),
                        image(texture).object_fit(ObjectFit::Cover).style(style! {
                            width: 110px; height: 100px;
                        }),
                    )),
                    document(Document::markdown(
                        "## Everything above is a component\n\nThe scroll area, image, circle, shape, and this document live in **mue-components**.\n\nThe renderer handles layout, painting, fonts, and events.",
                    )).style(style! {
                        color: #33445c;
                        padding: 24px;
                        background: white;
                        border-radius: 12px;
                    }),
                    text((0..16).map(|i| format!("Row {} · Scroll to inspect content and clipping.\n", i + 1)).collect::<String>())
                        .style(style! { color: #51617a; line-height: 2; }),
                )),
            ),
    );
    loop {
        clear_background(Color::WHITE.into());
        app.frame();
        next_frame().await;
    }
}
