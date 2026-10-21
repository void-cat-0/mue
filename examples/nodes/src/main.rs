//! Layout and typography gallery. Run with `cargo run -p nodes`.
use mue_components::*;
#[path = "../../shared/responsive.rs"]
mod responsive;

use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color,
    math::Vector,
    style::{Size, Style, Styleable, StyleableExt},
};

const PAPER: Color = Color::from_rgba(22, 27, 36, 255);
const PANEL: Color = Color::from_rgba(35, 43, 56, 255);
const ACCENT: Color = Color::from_rgba(125, 211, 193, 255);

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - nodes".into(),
            window_width: 960,
            window_height: 760,
            ..Default::default()
        },
        run(),
    );
}

async fn run() {
    responsive::sync();
    let offset = signal(Vector::ZERO);
    let taps = signal(0u32);
    let expanded = signal(true);
    let items = expanded
        .select(12usize, 3usize)
        .map(|count| (0..count).collect::<std::rc::Rc<[usize]>>());
    let rows = map_keyed(
        items,
        |index| *index,
        |index| {
            text(format!("Retained item {}", index + 1)).style(mue_macroquad::style! {
            font-size: {responsive::font(18.)};
            line-height: {responsive::ui(28.)};
            })
        },
    );
    let mut app = App::new(stack().style(mue_macroquad::style! {
width: 100%; height: 100%;
font-size: {responsive::root_font()};
padding: {(Size::vw(3.),)};
background: {PAPER};
gap: {responsive::ui(18.)};
}).children((
        wrap().style(mue_macroquad::style! {
width: 100%;
flex: none;
}).children((
            heading("Layout + type").style(mue_macroquad::style! {
font-size: {responsive::font(30.)};
line-height: {responsive::ui(40.)};
min-width: 0px;
flex-basis: 60%;
flex: 1;
}),
            link("Back to top").style(mue_macroquad::style! {
min-height: {responsive::ui(44.)};
font-size: {responsive::font(16.)};
line-height: {responsive::ui(24.)};
}).on_activate(move || offset.set(Vector::ZERO)),
        )),
        scroll_area().style(mue_macroquad::style! {
flex: 1;
width: 100%;
}).offset(offset)
            .options(ScrollOptions { mouse_drag: true, ..Default::default() })
            .content_style(Style::new().style(mue_macroquad::style! {
gap: {responsive::ui(24.)};
padding: {(Size::vw(2.),)};
}))
            .children((
                container().style(mue_macroquad::style! {
max-width: {responsive::ui(760.)};
min-width: 0px;
padding: {(0.,)};
gap: {responsive::ui(18.)};
}).children((
                    text("Wheel, drag the content, or drag a scrollbar. Child links still respond to taps.")
                        .style(mue_macroquad::style! {
font-size: {responsive::font(18.)};
line-height: {responsive::ui(28.)};
}),
                    wrap().style(mue_macroquad::style! { gap: {responsive::ui(12.)}; }).children((code("flexbox").style(mue_macroquad::style! { line-height: {responsive::ui(44.)}; }), code("stack").style(mue_macroquad::style! { line-height: {responsive::ui(44.)}; }), code("wrap").style(mue_macroquad::style! { line-height: {responsive::ui(44.)}; }), code("container").style(mue_macroquad::style! { line-height: {responsive::ui(44.)}; }))),
                    aspect_ratio().ratio(3.).style(mue_macroquad::style! {
width: 100%;
flex: none;
background: {PANEL};
border-radius: {responsive::ui(8.)};
}).children((
                        center().style(mue_macroquad::style! { width: 100%; height: 100%; }).children(em("A frame with a 3:1 ratio").style(mue_macroquad::style! {
font-size: {responsive::font(20.)};
line-height: {responsive::ui(28.)};
})),
                        float().placement(FloatPlacement::TopEnd).children(
                            center().style(mue_macroquad::style! {
width: {responsive::ui(40.)};
height: {responsive::ui(40.)};
border-radius: {responsive::ui(20.)};
background: {ACCENT};
})
                                .children(text("+").style(mue_macroquad::style! {
color: {PAPER};
font-size: {responsive::font(24.)};
line-height: {responsive::ui(24.)};
})),
                        ),
                    )),
                    separator(),
                    blockquote("Small pieces, composed with the same style API.").style(mue_macroquad::style! {
font-size: {responsive::font(22.)};
line-height: {responsive::ui(32.)};
}),
                    link_overlay().style(mue_macroquad::style! {
min-height: {responsive::ui(44.)};
background: {PANEL};
padding: {(Size::vw(2.),)};
border-radius: {responsive::ui(8.)};
gap: {responsive::ui(8.)};
})
                        .on_activate(move || taps.update(|count| *count += 1))
                        .children((
                            heading("An actionable card").level(HeadingLevel::H3).style(mue_macroquad::style! {
font-size: {responsive::font(22.)};
line-height: {responsive::ui(30.)};
}),
                            text(taps.map(|count| format!("Card activated {count} times"))).style(mue_macroquad::style! {
font-size: {responsive::font(18.)};
line-height: {responsive::ui(28.)};
}),
                            link("Toggle list length").style(mue_macroquad::style! {
min-height: {responsive::ui(44.)};
font-size: {responsive::font(18.)};
line-height: {responsive::ui(28.)};
}).toggle_on_tap(expanded),
                        )),
                    list().marker(ListMarker::Ordered(1)).style(mue_macroquad::style! {
font-size: {responsive::font(18.)};
line-height: {responsive::ui(28.)};
}).children(rows),
                    scroll_area().axis(ScrollAxis::Horizontal).style(mue_macroquad::style! {
width: 100%;
height: {Size::vh(17.)};
max-height: {responsive::ui(130.)};
flex: none;
background: {PANEL};
})
                        .children(code_block("let view = stack().style(mue_macroquad::style! { gap: {16.}; }).children((heading(\"Hello\"), text(\"Compose your interface with familiar nodes.\")));" )
                            .style(mue_macroquad::style! {
font-size: {responsive::font(18.)};
line-height: {responsive::ui(28.)};
flex: none;
})),
                    document(Document::blocks([
                        Document::heading("Rich documents", HeadingLevel::H2),
                        Document::rich([Span::new("Keep styled runs together: "), Span::new("emphasis").italic(),
                            Span::new(", "), Span::new("code").monospace(), Span::new(" and normal text.")]),
                        Document::blockquote("Documents share typography conventions with the standalone nodes."),
                    ])).style(mue_macroquad::style! {
font-size: {responsive::font(18.)};
line-height: {responsive::ui(28.)};
}),
                    markdown("## Markdown still works\n\n- Existing APIs are preserved\n- Paragraphs wrap naturally\n\n**End of gallery.**")
                        .style(mue_macroquad::style! {
font-size: {responsive::font(18.)};
line-height: {responsive::ui(28.)};
}),
                )),
            )),
        text(offset.map(|value| format!("Scroll offset: {:.0}, {:.0}", value.x, value.y)))
            .style(mue_macroquad::style! {
font-size: {responsive::font(14.)};
line-height: {responsive::ui(20.)};
color: {ACCENT};
flex: none;
}),
    )));
    loop {
        responsive::sync();
        clear_background(PAPER.into());
        app.frame();
        next_frame().await;
    }
}
