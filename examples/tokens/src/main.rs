//! Reactive application tokens: one named value per property, switched in place.
//!
//! Every style property reads a token handle once. Switching a theme writes new
//! values into the same signals, so existing bindings update without rebuilding
//! the view and without a hand-written `computed` per property.
use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_components::*;
use mue_core::{FontSize, Size, prelude::*};
use mue_macroquad::{
    App,
    color::Color,
    shader::{Brush, solid},
    style::{Styleable, StyleableExt},
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A global switch touches only handles that were already handed out.
    #[test]
    fn switching_a_palette_updates_every_handle() {
        Scope::null().run(|| {
            let theme = Theme::new();
            let light = light();
            let dark = dark();
            theme.register(&light.brushes);
            theme.register(&light.fonts);
            theme.register(&light.sizes);
            theme.register(&light.numbers);

            let panel = theme.get::<Brush>("panel");
            let gap = theme.get::<Size>("gap");
            let opacity = theme.get::<f32>("body.opacity");

            assert_eq!(panel.get_clone(), solid(Color::WHITE));
            assert_eq!(gap.get(), Size::Px(14.));
            assert_eq!(opacity.get(), 1.);

            dark.apply(&theme);

            assert_eq!(panel.get_clone(), solid(Color::from_rgba(33, 42, 57, 255)));
            assert_eq!(opacity.get(), 0.86);
            // Unchanged tokens keep the same handle and value.
            assert_eq!(gap.get(), Size::Px(14.));
        });
    }

    /// A variant picks between tokens, so it follows the theme without extra work.
    #[test]
    fn a_variant_keeps_following_the_theme() {
        Scope::null().run(|| {
            let theme = Theme::new();
            let light = light();
            let dark = dark();
            theme.register(&light.brushes);

            let selected = signal(false);
            let background =
                selected.pick(theme.get::<Brush>("accent"), theme.get::<Brush>("panel"));

            assert_eq!(background.get_clone(), solid(Color::WHITE));

            dark.apply(&theme);
            assert_eq!(
                background.get_clone(),
                solid(Color::from_rgba(33, 42, 57, 255))
            );

            selected.set(true);
            assert_eq!(
                background.get_clone(),
                solid(Color::from_rgba(46, 110, 76, 255))
            );
        });
    }
}

/// A component resolves the theme through ordinary scope context.
#[component]
fn theme_probe() -> impl Component {
    let theme = Theme::current().expect("theme installed");
    let _ = theme.get::<Brush>("panel");
    flexbox()
}

#[cfg(test)]
mod context_tests {
    use super::*;
    use mue_macroquad::node::IntoNode;

    #[test]
    fn components_see_the_installed_theme() {
        let theme = Rc::new(Theme::new());
        theme.register(&light().brushes);
        let _node = provide_theme(theme, || theme_probe()).into_node();
    }
}
