//! Application theme tokens: named values registered once and read as live properties.
//!
//! Light and dark palettes share the same token keys. Switching a palette writes
//! new values into the existing signals, so every style binding updates in place.
use mue_components::*;
#[path = "../../shared/responsive.rs"]
mod responsive;

use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_core::{FontSize, Size, prelude::*};
use mue_macroquad::{
    App,
    color::Color,
    inherit::provide_foreground,
    shader::{Brush, solid},
    style::{Styleable, StyleableExt, vh, vw},
};
use std::rc::Rc;

const PAPER: Color = Color::from_rgba(236, 242, 247, 255);
const NIGHT: Color = Color::from_rgba(22, 30, 44, 255);
const INK: Color = Color::from_rgba(30, 40, 55, 255);
const BLUE: Color = Color::from_rgba(39, 80, 135, 255);
const MUTED: Color = Color::from_rgba(118, 130, 149, 255);
const GREEN: Color = Color::from_rgba(46, 110, 76, 255);
const LIGHT_NEUTRAL: Color = Color::from_rgba(175, 185, 198, 255);

/// One theme: the same keys, different values.
struct Palette {
    brushes: TokenSet<Brush>,
    fonts: TokenSet<FontSize>,
    sizes: TokenSet<Size>,
}

impl Palette {
    /// Typography and spacing are shared; only the brushes change per mode.
    fn new(brushes: TokenSet<Brush>) -> Self {
        Self {
            brushes,
            fonts: token_set! {
                title: FontSize::Px(32.),
                body: FontSize::Px(22.),
            },
            sizes: token_set! {
                radius: Size::Px(12.),
                control_height: Size::Px(44.),
                control_padding_y: Size::Px(12.),
                control_padding_x: Size::Px(20.),
            },
        }
    }

    fn register(&self, theme: &Theme) {
        theme.register(&self.brushes);
        theme.register(&self.fonts);
        theme.register(&self.sizes);
    }

    fn apply(&self, theme: &Theme) {
        theme.apply(&self.brushes);
        theme.apply(&self.fonts);
        theme.apply(&self.sizes);
    }
}

fn light_palette() -> Palette {
    Palette::new(token_set! {
        canvas: solid(PAPER),
        text: solid(INK),
        neutral: solid(LIGHT_NEUTRAL),
        primary: solid(BLUE),
        selected: solid(GREEN),
        button_text: solid(Color::WHITE),
    })
}

fn dark_palette() -> Palette {
    Palette::new(token_set! {
        canvas: solid(NIGHT),
        text: solid(Color::WHITE),
        neutral: solid(MUTED),
        primary: solid(GREEN),
        selected: solid(BLUE),
        button_text: solid(Color::WHITE),
    })
}

#[component]
fn label(caption: Rc<str>) -> impl Component {
    text(caption).style(style! {
        font-size: token!(body);
        line-height: 1.45;
        transition: 280ms ease-in-out;
    })
}

#[component]
fn button(
    caption: Rc<str>,
    #[default] primary: bool,
    #[default] progress: i32,
    #[default] selected: bool,
) -> impl Component {
    // Resolve token handles while the theme scope is current; the computed
    // closure below only reads the captured handles when it reruns.
    let primary_fill: Prop<Brush> = token!(primary);
    let selected_fill: Prop<Brush> = token!(selected);
    let neutral_fill: Prop<Brush> = token!(neutral);
    flexbox()
        .style(style! {
            align-items: center;
            justify-content: center;
            min-height: token!(control_height);
            padding: token!(control_padding_y) token!(control_padding_x);
            border-radius: token!(radius);
            background: {computed(move |_| {
                if primary.get() {
                    if selected.get() {
                        selected_fill.get_clone()
                    } else {
                        primary_fill.get_clone()
                    }
                } else {
                    neutral_fill.get_clone()
                }
            })};
            transition: 280ms ease-in-out;
        })
        .with_hovered(move |view, hovered| {
            view.opacity(computed(move |_| {
                if hovered.get() {
                    0.82
                } else if progress.get() < 32 {
                    0.96
                } else {
                    1.
                }
            }))
        })
        .children(label(caption).style(style! {
            font-size: {responsive::font(18.)};
            line-height: {responsive::ui(26.)};
            color: token!(button_text);
            min-width: 0px;
        }))
}

#[component]
fn canvas() -> impl Component {
    scroll_area()
        .style(style! {
            width: 100%; height: 100%;
            background: token!(canvas);
            font-size: {responsive::root_font()};
            transition: 280ms ease-in-out;
        })
        .content_style(style! {
            padding: {(vw(6.), vh(6.))};
            flex-direction: column;
        })
}

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - theme tokens".into(),
            window_width: 640,
            window_height: 480,
            ..Default::default()
        },
        run(),
    );
}

async fn run() {
    responsive::sync();
    let is_dark = signal(false);
    let count = signal(0);
    let selected = signal(true);

    let light = light_palette();
    let dark = dark_palette();
    let theme = Rc::new(Theme::new());
    light.register(&theme);
    // The root has no theme scope yet, so read this handle explicitly.
    let foreground = theme.get::<Brush>("text");
    let switch_theme = Rc::clone(&theme);

    let mut app = App::new(provide_foreground(foreground, move || {
        provide_theme(theme, move || {
            canvas().children((
                label("Daily practice").style(style! {
                    font-size: token!(title);
                    line-height: 1.4;
                }),
                label(count.map(|n| format!("{n} sessions complete")))
                    .style(style! { margin-top: 5vh; margin-bottom: 6.5vh; }),
                wrap().style(style! { gap: {vw(2.)}; }).children((
                    button("Complete session")
                        .primary(true)
                        .progress(count)
                        .selected(selected)
                        .on_activate(move || count.update(|n| *n += 1)),
                    button("Reset").on_activate(move || count.set(0)),
                )),
                button(is_dark.select("Light mode", "Dark mode"))
                    .style(style! { margin-top: 6.5vh; })
                    .on_activate(move || {
                        is_dark.toggle();
                        selected.toggle();
                        if is_dark.get() {
                            dark.apply(&switch_theme);
                        } else {
                            light.apply(&switch_theme);
                        }
                    }),
            ))
        })
    }));

    loop {
        responsive::sync();
        clear_background(NIGHT.into());
        app.frame();
        next_frame().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A palette switch writes into the handles that were already handed out.
    #[test]
    fn switching_palettes_updates_every_handle() {
        Scope::null().run(|| {
            let theme = Theme::new();
            let light = light_palette();
            let dark = dark_palette();
            light.register(&theme);

            let canvas = theme.get::<Brush>("canvas");
            let primary = theme.get::<Brush>("primary");
            let selected = theme.get::<Brush>("selected");
            let title = theme.get::<FontSize>("title");
            let radius = theme.get::<Size>("radius");

            assert_eq!(canvas.get_clone(), solid(PAPER));
            assert_eq!(primary.get_clone(), solid(BLUE));
            assert_eq!(selected.get_clone(), solid(GREEN));
            assert_eq!(title.get(), FontSize::Px(32.));
            assert_eq!(radius.get(), Size::Px(12.));

            dark.apply(&theme);

            assert_eq!(canvas.get_clone(), solid(NIGHT));
            assert_eq!(primary.get_clone(), solid(GREEN));
            assert_eq!(selected.get_clone(), solid(BLUE));
            // Values shared by both palettes keep their handles and values.
            assert_eq!(title.get(), FontSize::Px(32.));
            assert_eq!(radius.get(), Size::Px(12.));
        });
    }

    /// Handles resolved at build time keep updating after the provider scope ends.
    #[test]
    fn computed_token_bindings_outlive_the_provider_scope() {
        use std::cell::RefCell;

        let theme = Rc::new(Theme::new());
        let light = light_palette();
        let dark = dark_palette();
        light.register(&theme);

        let captured = Rc::new(RefCell::new(None));
        let capture = Rc::clone(&captured);
        let node = provide_theme(Rc::clone(&theme), move || {
            let canvas: Prop<Brush> = token!(canvas);
            let style = style! {
                background: {computed(move |_| canvas.get_clone())};
            };
            *capture.borrow_mut() = style.declared_background();
            flexbox().style(style)
        })
        .into_node();

        let background = (*captured.borrow()).clone().expect("background binding");
        assert_eq!(background.get_clone(), solid(PAPER));

        // The provider scope is no longer current; switching only writes tokens.
        dark.apply(&theme);
        assert_eq!(background.get_clone(), solid(NIGHT));

        drop(node);
    }

    /// Style declarations resolve tokens through the installed theme.
    #[test]
    fn styles_read_the_installed_theme() {
        let theme = Rc::new(Theme::new());
        light_palette().register(&theme);
        let node = provide_theme(theme, || {
            let style = style! {
                font-size: token!(body);
                background: token!(canvas);
                border-radius: token!(radius);
            };
            assert_eq!(style.declared_font_size().unwrap().get(), FontSize::Px(22.));
            assert_eq!(
                style.declared_background().unwrap().get_clone(),
                solid(PAPER)
            );
            assert_eq!(style.declared_radius().unwrap().get(), Size::Px(12.));
            flexbox().style(style)
        })
        .into_node();
        drop(node);
    }
}
