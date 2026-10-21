use mue_components::prelude::*;
use mue_core::signal::signal;
use mue_macroquad::theme::Theme;

#[derive(Clone)]
struct ButtonOptions {
    primary: bool,
    progress: i32,
}

fn build_button(_: ()) -> FlexboxBuilder {
    flexbox()
}

fn build_canvas(_: ()) -> FlexboxBuilder {
    flexbox()
}

fn build_label(_: ()) -> FlexboxBuilder {
    flexbox()
}

#[test]
fn selector_themes_accept_options_classes_states_and_combinators() {
    let button = mue_macroquad::factory::factory(build_button).defaults(ButtonOptions {
        primary: false,
        progress: 0,
    });
    let canvas = mue_macroquad::factory::factory(build_canvas);
    let label = mue_macroquad::factory::factory(build_label);
    let theme: Theme = mue_macroquad::theme::theme! {
        canvas {
            background: { mue_macroquad::color::Color::WHITE };
        }
        label {
            color: { mue_macroquad::color::Color::BLACK };
        }
        button[primary] {
            background: #2468ac;
        }
        button[progress < 32] {
            opacity: 50%;
        }
        button:hover {
            color: white;
        }
        button.primary {
            background: blue;
        }
        button, label {
            line-height: 1.2;
        }
        button > label {
            font-size: 12px;
        }
        button + label {
            opacity: 80%;
        }
        button ~ label {
            color: #c06a40;
        }
        button label {
            line-height: 1.25;
        }
        * {
            transition: 150ms ease-in-out;
        }
    };

    let active = signal(true);
    let _ = button
        .create(())
        .options(|options| options.progress = 12)
        .class("primary", active)
        .style(style! { padding: 12px; });
    let _ = mue_macroquad::theme::provide_theme(theme, || {
        flexbox().children((button.create(()), label.create(())))
    })
    .into_node();

    let _ = (button.identity(), canvas.identity(), label.identity());
}
