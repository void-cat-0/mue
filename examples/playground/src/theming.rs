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
    let dark = dark_theme();
    let tile = factory(build_tile).defaults(Options { accent: false });
    let panel_factory =
        factory(|build: Rc<dyn Fn() -> Owned<Children>>| container().children(build()));
    let first = theme! {
        tile[accent] { background: { CORAL }; transform: scale(1.04); }
        tile { background: { BLUE }; transform: scale(1.02); }
        panel_factory > tile + tile { opacity: 82%; }
        panel_factory > tile ~ tile { transform: translateX(4px); }
    };
    let second = theme! {
        tile[accent] { background: { BLUE }; transform: scale(1.); }
        tile { background: { panel_brush() }; transform: scale(1.); }
    };
    let nested = theme! {
        tile { background: { Color::from_rgba(70, 140, 100, 255) }; }
    };
    let theme_code = dark.map(|is_dark| {
        if is_dark {
            r#"theme! {
    tile[accent] {
        background: { BLUE };
        transform: scale(1.);
    }
    tile {
        background: { panel_brush() };
        transform: scale(1.);
    }
}"#
        } else {
            r#"theme! {
    tile[accent] {
        background: { CORAL };
        transform: scale(1.04);
    }
    tile {
        background: { BLUE };
        transform: scale(1.02);
    }
    panel_factory > tile + tile {
        opacity: 82%;
    }
    panel_factory > tile ~ tile {
        transform: translateX(4px);
    }
}"#
        }
    });
    stack()
        .style(style! {
            width: 100%;
            min-width: 0px;
            gap: {responsive::ui(20.)};
        })
        .children((
            button(
                dark.map(|is_dark| format!("Theme: {}", if is_dark { "Dark" } else { "Light" })),
                toggle_theme,
            ),
            provide_theme(dark.select(second, first), move || {
                stack()
                    .style(style! {
                        width: 100%;
                        min-width: 0px;
                        gap: {responsive::ui(16.)};
                    })
                    .children((
                        tile.create("Default")
                            .style(style! { transition: 220ms spring; }),
                        tile.create("Accent")
                            .options(|options| options.accent = true)
                            .style(style! { transition: 220ms spring; }),
                        {
                            let build: Rc<dyn Fn() -> Owned<Children>> = Rc::new({
                                let tile = tile.clone();
                                move || {
                                    (tile.create("First sibling"), tile.create("Later sibling"))
                                        .into_children()
                                }
                            });
                            panel_factory.create(build).style(style! {
                                width: 100%;
                                gap: {responsive::ui(8.)};
                            })
                        },
                        provide_theme(nested, move || {
                            tile.create("Nested")
                                .style(style! { transition: 220ms spring; })
                        }),
                    ))
            }),
            code_block(theme_code).style(style! {
                width: 100%;
                min-width: 0px;
                text-wrap: {true};
                font-size: {responsive::font(16.)};
                line-height: {responsive::ui(24.)};
                padding: {(responsive::ui(16.),)};
                border-radius: {responsive::ui(6.)};
            }),
        ))
        .into_node()
}
