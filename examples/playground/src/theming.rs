use super::{controls::*, responsive};
use mue_components::*;
use mue_core::{Owned, prop::Prop};
use mue_macroquad::{
    color::Color,
    factory::factory,
    style::Styleable,
    theme::{provide_theme, theme},
};
use std::rc::Rc;

#[derive(Clone)]
struct Options {
    accent: bool,
}

fn build_tile(caption: Prop<Rc<str>>) -> CenterBuilder {
    center().children(text(caption).style(style! {
        font-size: {responsive::font(22.)};
        line-height: {responsive::ui(32.)};
    }))
}

pub fn page() -> Node {
    let (dark, control) = toggle("Theme");
    let tile = factory(build_tile).defaults(Options { accent: false });
    let panel = factory(|build: Rc<dyn Fn() -> Owned<Children>>| container().children(build()));
    let first = theme! {
        tile[accent] { background: { CORAL }; }
        tile { background: { BLUE }; }
        panel > tile + tile { opacity: 82%; }
        panel > tile ~ tile { transform: translateX(4px); }
    };
    let second = theme! {
        tile[accent] { background: { BLUE }; }
        tile { background: { PANEL }; }
    };
    let nested = theme! {
        tile { background: { Color::from_rgba(70, 140, 100, 255) }; }
    };
    stack()
        .style(style! {
            width: 100%;
            min-width: 0px;
            gap: {responsive::ui(20.)};
        })
        .children((
            control,
            provide_theme(dark.select(second, first), move || {
                stack()
                    .style(style! {
                        width: 100%;
                        min-width: 0px;
                        gap: {responsive::ui(16.)};
                    })
                    .children((
                        tile.create("Default")
                            .style(style! { transition: 300ms ease-in-out; }),
                        tile.create("Accent")
                            .options(|options| options.accent = true)
                            .style(style! { transition: 300ms ease-in-out; }),
                        {
                            let build: Rc<dyn Fn() -> Owned<Children>> = Rc::new({
                                let tile = tile.clone();
                                move || {
                                    (tile.create("First sibling"), tile.create("Later sibling"))
                                        .into_children()
                                }
                            });
                            panel.create(build).style(style! {
                                width: 100%;
                                gap: {responsive::ui(8.)};
                            })
                        },
                        provide_theme(nested, move || {
                            tile.create("Nested")
                                .style(style! { transition: 300ms ease-in-out; })
                        }),
                    ))
            }),
        ))
        .into_node()
}
