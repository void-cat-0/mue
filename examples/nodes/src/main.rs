//! Layout and typography gallery. Run with `cargo run -p nodes`.
#[path = "../../shared/responsive.rs"]
mod responsive;

use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    color::Color,
    math::Vector,
    node::*,
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
            text(format!("Retained item {}", index + 1))
                .font_size(responsive::font(18.))
                .line_height(responsive::ui(28.))
        },
    );
    let mut app = App::new(stack().size_full().font_size(responsive::root_font()).padding((Size::vw(3.),)).background(PAPER).gap(responsive::ui(18.)).children((
        wrap().w_full().flex_none().children((
            heading("Layout + type").font_size(responsive::font(30.)).line_height(responsive::ui(40.))
                .min_width(0.).flex_basis(Size::percent(60.)).flex_1(),
            link("Back to top").min_height(responsive::ui(44.)).font_size(responsive::font(16.)).line_height(responsive::ui(24.)).on_activate(move || offset.set(Vector::ZERO)),
        )),
        scroll_area().flex_1().w_full().offset(offset)
            .options(ScrollOptions { mouse_drag: true, ..Default::default() })
            .content_style(Style::new().gap(responsive::ui(24.)).padding((Size::vw(2.),)))
            .children((
                container().max_width(responsive::ui(760.)).min_width(0.).padding((0.,)).gap(responsive::ui(18.)).children((
                    text("Wheel, drag the content, or drag a scrollbar. Child links still respond to taps.")
                        .font_size(responsive::font(18.)).line_height(responsive::ui(28.)),
                    wrap().gap(responsive::ui(12.)).children((code("flexbox").line_height(responsive::ui(44.)), code("stack").line_height(responsive::ui(44.)), code("wrap").line_height(responsive::ui(44.)), code("container").line_height(responsive::ui(44.)))),
                    aspect_ratio().ratio(3.).w_full().flex_none().background(PANEL).radius(responsive::ui(8.)).children((
                        center().size_full().children(em("A frame with a 3:1 ratio").font_size(responsive::font(20.)).line_height(responsive::ui(28.))),
                        float().placement(FloatPlacement::TopEnd).children(
                            center().width(responsive::ui(40.)).height(responsive::ui(40.)).radius(responsive::ui(20.)).background(ACCENT)
                                .children(text("+").color(PAPER).font_size(responsive::font(24.)).line_height(responsive::ui(24.))),
                        ),
                    )),
                    separator(),
                    blockquote("Small pieces, composed with the same style API.").font_size(responsive::font(22.)).line_height(responsive::ui(32.)),
                    link_overlay().min_height(responsive::ui(44.)).background(PANEL).padding((Size::vw(2.),)).radius(responsive::ui(8.)).gap(responsive::ui(8.))
                        .on_activate(move || taps.update(|count| *count += 1))
                        .children((
                            heading("An actionable card").level(HeadingLevel::H3).font_size(responsive::font(22.)).line_height(responsive::ui(30.)),
                            text(taps.map(|count| format!("Card activated {count} times"))).font_size(responsive::font(18.)).line_height(responsive::ui(28.)),
                            link("Toggle list length").min_height(responsive::ui(44.)).font_size(responsive::font(18.)).line_height(responsive::ui(28.)).toggle_on_tap(expanded),
                        )),
                    list().marker(ListMarker::Ordered(1)).font_size(responsive::font(18.)).line_height(responsive::ui(28.)).children(rows),
                    scroll_area().axis(ScrollAxis::Horizontal).w_full()
                        .height(Size::vh(17.)).max_height(responsive::ui(130.)).flex_none().background(PANEL)
                        .children(code_block("let view = stack().gap(16.).children((heading(\"Hello\"), text(\"Compose your interface with familiar nodes.\")));" )
                            .font_size(responsive::font(18.)).line_height(responsive::ui(28.)).flex_none()),
                    document(Document::blocks([
                        Document::heading("Rich documents", HeadingLevel::H2),
                        Document::rich([Span::new("Keep styled runs together: "), Span::new("emphasis").italic(),
                            Span::new(", "), Span::new("code").monospace(), Span::new(" and normal text.")]),
                        Document::blockquote("Documents share typography conventions with the standalone nodes."),
                    ])).font_size(responsive::font(18.)).line_height(responsive::ui(28.)),
                    markdown("## Markdown still works\n\n- Existing APIs are preserved\n- Paragraphs wrap naturally\n\n**End of gallery.**")
                        .font_size(responsive::font(18.)).line_height(responsive::ui(28.)),
                )),
            )),
        text(offset.map(|value| format!("Scroll offset: {:.0}, {:.0}", value.x, value.y)))
            .font_size(responsive::font(14.)).line_height(responsive::ui(20.)).color(ACCENT).flex_none(),
    )));
    loop {
        responsive::sync();
        clear_background(PAPER.into());
        app.frame();
        next_frame().await;
    }
}
