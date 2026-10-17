use super::{controls::*, responsive};
use mue_macroquad::{
    color::Color,
    factory::factory,
    node::*,
    style::{Styleable, StyleableExt, vh, vw},
    theme::{provide_theme, theme},
};
#[derive(Clone)]
struct Options {
    accent: bool,
}
pub fn page() -> Node {
    let (dark, control) = toggle("Theme");
    let tile = factory(|label: &'static str| {
        center().children(
            text(label)
                .font_size(responsive::font(22.))
                .line_height(responsive::ui(32.)),
        )
    })
    .defaults(Options { accent: false })
    .style(|s, _| {
        s.w_full()
            .min_width(0.)
            .padding((vw(2.75), vh(2.75)))
            .radius(14.)
    });
    let first = theme!(|c, s| match c {
        tile(o) if o.accent => s.background(CORAL),
        tile => s.background(BLUE),
        _ => s,
    });
    let second = theme!(|c, s| match c {
        tile(o) if o.accent => s.background(BLUE),
        tile => s.background(PANEL),
        _ => s,
    });
    let nested = theme!(|c, s| match c {
        tile => s.background(Color::from_rgba(70, 140, 100, 255)),
        _ => s,
    });
    stack()
        .w_full()
        .min_width(0.)
        .gap(vh(2.5))
        .children((
            control,
            provide_theme(dark.select(second, first), move || {
                stack().w_full().min_width(0.).gap(vh(2.)).children((
                    tile.create("Default").animation(SPRING),
                    tile.create("Accent")
                        .options(|o| o.accent = true)
                        .animation(SPRING),
                    provide_theme(nested, move || tile.create("Nested").animation(SPRING)),
                ))
            }),
        ))
        .into_node()
}
