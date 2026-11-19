//! Reactive application tokens: one named value per property, switched in place.
//!
//! Every style property reads a token handle once. Switching a theme writes new
//! values into the same signals, so existing bindings update without rebuilding
//! the view and without a hand-written `computed` per property.
use mue::{
    App,
    components::{Theme, TokenSet, provide_theme, scroll_area, token_set},
    core::{FontSize, Size},
    macroquad::{
        self,
        prelude::{Conf, clear_background, next_frame},
    },
    prelude::*,
    renderer::{
        color::Color,
        shader::{Brush, solid},
    },
};
use std::rc::Rc;

/// One theme: the same property names, different values.
struct Palette {
    brushes: TokenSet<Brush>,
    fonts: TokenSet<FontSize>,
    sizes: TokenSet<Size>,
    numbers: TokenSet<f32>,
}

impl Palette {
    fn apply(&self, theme: &Theme) {
        theme.apply(&self.brushes);
        theme.apply(&self.fonts);
        theme.apply(&self.sizes);
        theme.apply(&self.numbers);
    }
}

fn light() -> Palette {
    Palette {
        brushes: token_set! {
            canvas: solid(Color::from_rgba(236, 242, 247, 255)),
            panel: solid(Color::WHITE),
            text: solid(Color::from_rgba(30, 40, 55, 255)),
            muted: solid(Color::from_rgba(118, 130, 149, 255)),
            accent: solid(Color::from_rgba(39, 80, 135, 255)),
        },
        fonts: token_set! {
            title: FontSize::Px(28.),
            body: FontSize::Px(17.),
            button: FontSize::Px(16.),
        },
        sizes: token_set! {
            radius: Size::Px(14.),
            gap: Size::Px(14.),
        },
        numbers: token_set! { "body.opacity": 1_f32 },
    }
}

fn dark() -> Palette {
    Palette {
        brushes: token_set! {
            canvas: solid(Color::from_rgba(22, 30, 44, 255)),
            panel: solid(Color::from_rgba(33, 42, 57, 255)),
            text: solid(Color::from_rgba(240, 244, 250, 255)),
            muted: solid(Color::from_rgba(154, 166, 184, 255)),
            accent: solid(Color::from_rgba(46, 110, 76, 255)),
        },
        fonts: token_set! {
            title: FontSize::Px(28.),
            body: FontSize::Px(17.),
            button: FontSize::Px(16.),
        },
        sizes: token_set! {
            radius: Size::Px(14.),
            gap: Size::Px(14.),
        },
        numbers: token_set! { "body.opacity": 0.86_f32 },
    }
}

#[component]
fn card(title: Rc<str>, body: Rc<str>, #[default] selected: bool) -> impl Component {
    flexbox()
        .style(style! {
            flex-direction: column;
            gap: token!(gap);
            padding: 20px;
            border-radius: token!(radius);
            // pick() is only for component state; the theme axis lives in the tokens.
            background: {selected.pick(token!(accent), token!(panel))};
            transition: 260ms ease-in-out;
        })
        .children((
            text(title).style(style! {
                font-size: token!(title);
                color: {selected.pick(solid(Color::WHITE), token!(text))};
            }),
            text(body).style(style! {
                font-size: token!(body);
                color: {selected.pick(solid(Color::WHITE), token!(muted))};
            }),
        ))
}

#[component]
fn action_button(caption: Rc<str>) -> impl Component {
    flexbox()
        .style(style! {
            align-items: center;
            justify-content: center;
            padding: 12px 20px;
            border-radius: token!(radius);
            background: token!(accent);
            transition: 260ms ease-in-out;
        })
        .children(text(caption).style(style! {
            font-size: token!(button);
            color: {Color::WHITE};
        }))
}

fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - reactive tokens".into(),
            window_width: 720,
            window_height: 620,
            ..Default::default()
        },
        run(),
    );
}

async fn run() {
    let theme = Rc::new(Theme::new());
    let light = light();
    let dark = dark();
    theme.register(&light.brushes);
    theme.register(&light.fonts);
    theme.register(&light.sizes);
    theme.register(&light.numbers);

    let is_dark = signal(false);
    let toggle_theme = Rc::clone(&theme);

    let mut app = App::new(provide_theme(theme, move || {
        scroll_area()
                .style(style! {
                    width: 100%; height: 100%;
                    background: token!(canvas);
                    transition: 260ms ease-in-out;
                })
                .content_style(style! {
                    padding: 32px;
                    flex-direction: column;
                    gap: 18px;
                })
                .children((
                    text("Reactive tokens").style(style! {
                        font-size: token!(title);
                        color: token!(text);
                    }),
                    text("Every property reads one named value. Switching a theme writes new values into the same signals.")
                        .style(style! {
                            font-size: token!(body);
                            color: token!(muted);
                            opacity: token!("body.opacity");
                        }),
                    card("Panel", "Reads panel, accent, text and radius tokens."),
                    card("Selected", "This card uses pick() for its selected state.").selected(true),
                    action_button("Toggle theme").on_activate(move || {
                        is_dark.toggle();
                        if is_dark.get() {
                            dark.apply(&toggle_theme);
                        } else {
                            light.apply(&toggle_theme);
                        }
                    }),
                ))
    }));

    loop {
        clear_background(Color::from_rgba(22, 30, 44, 255).into());
        app.frame();
        next_frame().await;
    }
}
