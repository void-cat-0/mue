use mue_components::prelude::*;
use mue_core::{FontSize, Size, prelude::*, scope::OwnedScope};
use mue_macroquad::{color::Color, shader::Brush};
use std::rc::Rc;

#[test]
fn token_set_macro_builds_mixed_key_sets() {
    let spacing: TokenSet<f32> = token_set! {
        gap: 8.,
        "body.opacity": 1.,
    };

    assert_eq!(spacing.get("gap"), Some(&8.));
    assert_eq!(spacing.get("body.opacity"), Some(&1.));
}

#[test]
fn token_set_macro_feeds_the_theme_store() {
    Scope::null().run(|| {
        let theme = Theme::new();
        theme.register(&token_set! {
            panel: 1_f32,
            "tile.accent": 2_f32,
        });

        let panel = theme.get::<f32>("panel");
        assert_eq!(panel.get(), 1.);

        theme.apply(&token_set! { panel: 3_f32 });
        assert_eq!(panel.get(), 3.);
        assert_eq!(theme.get::<f32>("tile.accent").get(), 2.);
    });
}

#[test]
fn token_macro_accepts_all_four_forms() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let theme = Rc::new(Theme::new());
        theme.register(&token_set! { title: FontSize::Px(28.) });
        Theme::install(&theme);

        let bare: Prop<FontSize> = token!(title);
        let quoted: Prop<FontSize> = token!("title");
        let explicit: Prop<FontSize> = token!(theme, title);
        let explicit_quoted: Prop<FontSize> = token!(theme, "title");

        assert_eq!(bare.get(), FontSize::Px(28.));
        assert_eq!(quoted.get(), FontSize::Px(28.));
        assert_eq!(explicit.get(), FontSize::Px(28.));
        assert_eq!(explicit_quoted.get(), FontSize::Px(28.));
    });
}

#[test]
fn style_tokens_read_the_installed_theme() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let theme = Rc::new(Theme::new());
        theme.register(&token_set! { title: FontSize::Px(28.) });
        theme.register(&token_set! { gap: Size::Px(14.) });
        theme.register(&token_set! { panel: Brush::Solid(Color::BLACK) });
        Theme::install(&theme);

        let style = style! {
            font-size: token!(title);
            gap: token!(gap);
            background: token!(panel);
        };

        assert_eq!(style.declared_font_size().unwrap().get(), FontSize::Px(28.));
        assert_eq!(style.declared_row_gap().unwrap().get(), Size::Px(14.));
        assert_eq!(style.declared_column_gap().unwrap().get(), Size::Px(14.));
        assert_eq!(
            style.declared_background().unwrap().get_clone(),
            Brush::Solid(Color::BLACK)
        );
    });
}

#[test]
fn style_tokens_rewrite_inside_expressions() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let theme = Rc::new(Theme::new());
        theme.register(&token_set! {
            accent: Brush::Solid(Color::WHITE),
            panel: Brush::Solid(Color::BLACK),
        });
        Theme::install(&theme);

        let selected = signal(false);
        let style = style! {
            background: { selected.pick(token!(accent), token!(panel)) };
        };

        assert_eq!(
            style.declared_background().unwrap().get_clone(),
            Brush::Solid(Color::BLACK)
        );
        selected.set(true);
        assert_eq!(
            style.declared_background().unwrap().get_clone(),
            Brush::Solid(Color::WHITE)
        );
    });
}

#[test]
fn style_tokens_accept_an_explicit_theme() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let theme = Rc::new(Theme::new());
        theme.register(&token_set! { title: FontSize::Px(20.) });

        let style = style! { font-size: token!(theme, title); };

        assert_eq!(style.declared_font_size().unwrap().get(), FontSize::Px(20.));
    });
}
