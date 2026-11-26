//! Mobile patterns: pull to refresh, load more, swipe actions, back
//! navigation (Android back button, Escape, edge swipe), keyboard avoidance,
//! touch-friendly hit areas, and the interaction directives (click outside,
//! press-and-hold repeat, haptics). Mouse drags stand in for touches on
//! desktop.
//!
//! Run with `cargo run --manifest-path examples/Cargo.toml --example mobile`.
//! Set `MUE_CAPTURE=out.png` to save a screenshot and exit.

#[path = "../../foundation/src/capture.rs"]
mod capture;

use std::rc::Rc;

use macroquad::prelude::{Conf, clear_background, next_frame};
use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App, back::use_back_handler, presence::PresenceAnimation, runtime::set_timeout,
    transition_group::transition_group,
};

#[derive(Clone, PartialEq)]
struct Message {
    id: u32,
    from: Rc<str>,
}

fn message_row(message: Message, messages: Signal<Rc<[Message]>>) -> impl Component {
    let id = message.id;
    let remove = move || {
        let next: Rc<[Message]> = messages
            .get_clone()
            .iter()
            .filter(|message| message.id != id)
            .cloned()
            .collect();
        messages.set(next);
    };
    let delete = button("Delete")
        .style(style! { border-radius: 0px; background: #c0392b; min-width: 88px; })
        .on_activate(remove.clone());
    let archive = button("Archive")
        .style(style! { border-radius: 0px; background: #2d6cdf; min-width: 88px; });
    swipe_actions(
        stack()
            .style(style! { padding: 12px 16px; gap: 2px; background: #1b2131; border-bottom-width: 1px; border-color: #262e42; })
            .children((
                text(message.from.clone()).style(style! { font-size: 15px; line-height: 20px; color: #e6ebf5; }),
                text(Rc::<str>::from(format!("Message #{id}: swipe left or right")))
                    .style(style! { font-size: 13px; line-height: 18px; color: #8f9bb3; }),
            )),
    )
    .leading(archive)
    .trailing(delete)
    .on_full_swipe_trailing(remove)
    .mouse(true)
}

fn ui(capture: bool) -> impl Component {
    let names = [
        "Ada", "Grace", "Linus", "Margaret", "Alan", "Barbara", "Edsger", "Frances",
    ];
    let next_id = signal(0_u32);
    let make = move |count: u32| -> Vec<Message> {
        (0..count)
            .map(|_| {
                let id = next_id.get_untracked();
                next_id.set(id + 1);
                Message {
                    id,
                    from: Rc::from(names[id as usize % names.len()]),
                }
            })
            .collect()
    };
    let messages: Signal<Rc<[Message]>> = signal(Rc::from(make(12)));
    let refreshing = signal(false);
    let loading = signal(false);
    let status = signal(String::from(
        "Pull down to refresh, scroll to the end to load more",
    ));
    let detail_open = signal(false);
    let menu_open = signal(false);
    let text_size = signal(15_i32);

    // Back closes the detail panel first; with nothing to close, back does
    // nothing on desktop and closes the app on Android.
    use_back_handler(detail_open, move || detail_open.set(false));
    if capture {
        set_timeout(0.05, move || refreshing.set(true));
    }

    let list = scroll_area()
        .options(ScrollOptions {
            mouse_drag: true,
            ..Default::default()
        })
        .on_refresh(refreshing, move || {
            status.set("Refreshing…".into());
            set_timeout(1.2, move || {
                let mut fresh = make(3);
                fresh.extend(messages.get_clone().iter().cloned());
                messages.set(Rc::from(fresh));
                refreshing.set(false);
                status.set("Refreshed: 3 new messages on top".into());
            });
        })
        .on_reach_end(120., move || {
            if loading.get_untracked() {
                return;
            }
            loading.set(true);
            status.set("Loading more…".into());
            set_timeout(0.6, move || {
                let mut all: Vec<Message> = messages.get_clone().to_vec();
                all.extend(make(8));
                messages.set(Rc::from(all));
                loading.set(false);
                status.set(format!("{} messages", messages.get_clone().len()));
            });
        })
        .style(style! { flex-grow: 1; })
        .children(
            // Deleted rows fade out and the rest slide up; new ones fade in.
            transition_group(
                messages,
                |message: &Message| message.id,
                move |message: &Message| message_row(message.clone(), messages),
            )
            .animation(PresenceAnimation::fade().slide(0., -12.)),
        );

    back_swipe()
        .mouse(true)
        .style(style! { width: 100%; height: 100%; background: #121722; })
        .children((
            hstack()
                .style(style! { padding: 16px; gap: 12px; align-items: center; background: #171d2b; font-size: 15px; line-height: 20px; })
                .children((
                    heading("Inbox").level(HeadingLevel::H4).style(style! { color: #ffffff; flex-grow: 1; }),
                    button("Sort").on_activate(move || menu_open.set(true)),
                    button("Details").on_activate(move || detail_open.set(true)),
                )),
            // A menu that closes on any press outside it.
            show_if(
                menu_open,
                stack()
                    .directive(ClickOutside::new(move || menu_open.set(false)).enabled(menu_open))
                    .style(style! { position: absolute; top: 60px; right: 16px; padding: 6px; gap: 4px; font-size: 15px; line-height: 20px; background: #2a3349; border-radius: 10px; z-index: 10; })
                    .children((
                        button("Newest first").on_activate(move || menu_open.set(false)),
                        button("Oldest first").on_activate(move || menu_open.set(false)),
                    )),
            ),
            text(status.map(|text| Rc::<str>::from(text.as_str())))
                .style(style! { padding: 8px 16px; font-size: 13px; line-height: 18px; color: #7fd1a8; }),
            list,
            overlay_host(
                detail_open,
                stack()
                    .style(style! { padding: 24px; gap: 12px; background: #222a3b; border-radius: 14px; width: 300px; })
                    .children((
                        heading("Details").level(HeadingLevel::H4).style(style! { color: #ffffff; }),
                        text("Back (Escape, the Android back button, or a swipe from the left edge) closes this panel.")
                            .style(style! { font-size: 14px; line-height: 20px; color: #c9d1e0; }),
                        input().placeholder("Reply…"),
                        // Hold a button to keep stepping; each step ticks a haptic.
                        hstack()
                            .style(style! { gap: 8px; align-items: center; })
                            .children((
                                text("Text size").style(style! { font-size: 14px; color: #c9d1e0; flex-grow: 1; }),
                                button("−")
                                    .directive(LongPressRepeat::new(move || text_size.update(|size| *size = (*size - 1).max(10))))
                                    .directive(Haptic::on(FeedbackTrigger::Press, HapticStyle::Selection)),
                                text(text_size.map(|size| Rc::<str>::from(size.to_string())))
                                    .style(style! { font-size: 14px; color: #ffffff; min-width: 28px; }),
                                button("+")
                                    .directive(LongPressRepeat::new(move || text_size.update(|size| *size = (*size + 1).min(32))))
                                    .directive(Haptic::on(FeedbackTrigger::Press, HapticStyle::Selection)),
                            )),
                        button("Close").primary(true).on_activate(move || detail_open.set(false)),
                    )),
            )
            .dismissible(true)
            .on_dismiss(move || detail_open.set(false)),
        ))
}

fn conf() -> Conf {
    Conf {
        window_title: "mobile".into(),
        window_width: 420,
        window_height: 720,
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
