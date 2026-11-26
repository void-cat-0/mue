//! Text inputs: single-line, password, parsed number, input groups, and a
//! multiline textarea, with IME composition, selection, clipboard, undo, and
//! Vue-style model modifiers.
//!
//! Run with `cargo run --manifest-path examples/Cargo.toml --example text-input`.
//! Set `MUE_CAPTURE=out.png` to save a screenshot and exit.

#[path = "../../foundation/src/capture.rs"]
mod capture;

use std::rc::Rc;

use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App,
    node::TextEditHandle,
    text_input::{TextInputAction, TextInputKind, TextSelection, digits_only},
};

fn label(text_value: &'static str) -> impl Component {
    text(text_value).style(style! { font-size: 13px; line-height: 18px; color: #8f9bb3; })
}

fn status(value: impl Into<Prop<Rc<str>>>) -> impl Component {
    text(value).style(style! { font-size: 14px; line-height: 20px; color: #7fd1a8; })
}

fn ui(capture: bool) -> impl Component {
    let name = signal(String::from("Ada Lovelace"));
    let submitted = signal(String::from("Press Enter in the name field"));
    let age = signal(36_u32);
    let notes = signal(String::from(
        "Multiline notes wrap and scroll.\n输入法预编辑会在光标处显示下划线。",
    ));
    let lazy_notes = signal(String::new());
    let name_handle = TextEditHandle::new();
    if capture {
        // Show focus, the caret, and a selection in screenshots.
        let handle = name_handle.clone();
        mue_macroquad::runtime::set_timeout(0.05, move || {
            handle.focus();
            handle.set_selection(TextSelection::new(4, 12));
        });
    }
    let form = stack()
        .style(style! { padding: 28px; gap: 10px; })
        .children((
            heading("Text input").style(style! { color: #ffffff; }),
            label("Name: v-model.trim, Enter submits"),
            input()
                .model(name)
                .trim()
                .placeholder("Your name")
                .handle(name_handle)
                .action(TextInputAction::Done)
                .on_submit(move |_| submitted.set(format!("Submitted: {}", name.get_clone())))
                .style(style! { width: 380px; }),
            status(submitted.map(|text| Rc::<str>::from(text.as_str()))),
            label("Password"),
            input()
                .kind(TextInputKind::Password)
                .placeholder("At least 8 characters")
                .style(style! { width: 380px; }),
            label("Age: v-model.number (red while the text does not parse)"),
            hstack()
                .style(style! { gap: 12px; align-items: center; })
                .children((
                    input()
                        .model_parsed(age)
                        .kind(TextInputKind::Number)
                        .style(style! { width: 120px; }),
                    status(age.map(|age| Rc::<str>::from(format!("age = {age}")))),
                )),
            label("Input group: addons and elements"),
            input_group(input().kind(TextInputKind::Url).placeholder("example.com"))
                .left_addon(text("https://"))
                .right_element(text("↵"))
                .style(style! { width: 380px; }),
            label("PIN: digits only, 6 max"),
            input()
                .kind(TextInputKind::Number)
                .formatter(digits_only())
                .max_length(Some(6))
                .placeholder("000000")
                .size(InputSize::Lg)
                .variant(InputVariant::Filled)
                .style(style! { width: 160px; }),
            label("Notes: textarea; the second one is v-model.lazy (written on blur)"),
            textarea()
                .model(notes)
                .rows(3)
                .style(style! { width: 380px; max-height: 110px; }),
            stack().style(style! { gap: 10px; }).children((
                textarea()
                    .model(lazy_notes)
                    .lazy()
                    .rows(2)
                    .placeholder("Type, then click elsewhere")
                    .variant(InputVariant::Flushed)
                    .style(style! { width: 380px; }),
                status(lazy_notes.map(|text| Rc::<str>::from(format!("lazy model: {text:?}")))),
            )),
            stack()
                .style(style! { gap: 10px; margin-top: 360px; })
                .children((
                    label("Below the fold: Tab here (or focus it under a soft keyboard)"),
                    input()
                        .placeholder("Revealed field")
                        .style(style! { width: 380px; }),
                )),
        ));
    // A scroll area keeps the focused caret visible (above a soft keyboard).
    scroll_area()
        .style(style! { width: 100%; height: 100%; background: #121722; })
        .children(form)
}

fn conf() -> Conf {
    Conf {
        window_title: "text input".into(),
        window_width: 520,
        window_height: 860,
        ..Default::default()
    }
}

#[macroquad::main(conf)]
async fn main() {
    let mut capture = capture::Capture::from_env();
    let mut app = App::new(ui(capture.enabled()));
    loop {
        clear_background(macroquad::color::BLACK);
        app.frame();
        if capture.after_draw() {
            break;
        }
        next_frame().await;
    }
}
