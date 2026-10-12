use macroquad::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    node::*,
    runtime::set_timeout,
    style::{Styleable, StyleableExt},
};

fn main() {
    macroquad::Window::from_config(
        macroquad::window::Conf {
            window_title: "mue – events".to_string(),
            window_width: 800,
            window_height: 600,
            ..Default::default()
        },
        async { the_main().await },
    );
}

#[mue_macros::component]
fn view() -> impl Component {
    let pressed = signal(false);
    let hovered = signal(false);
    let count = signal(0);
    let long_pressed = signal(false);
    let mut long_pressed_handle = None;

    text(computed(move |_| {
        format!(
            "Clicked {} times\nPressed: {}{}{}",
            count.get(),
            pressed.get(),
            if long_pressed.get() {
                "\nLong pressed"
            } else {
                ""
            },
            if hovered.get() { "\nHovered" } else { "" }
        )
        .into()
    }))
    .use_pressed(pressed)
    .use_hovered(hovered)
    .on_tap(move |_| count.update(|c| *c += 1))
    .on_long_press(move |_| {
        long_pressed.set(true);
        if let Some(old) = long_pressed_handle.replace(set_timeout(1., move || {
            long_pressed.set(false);
        })) {
            old.cancel();
        }
    })
    .on_wheel_event(|event| {
        println!("{event:?}");
    })
}

async fn the_main() {
    let root = view();

    let mut app = App::new(root);

    loop {
        clear_background(BLACK);
        app.frame();
        next_frame().await;
    }
}
