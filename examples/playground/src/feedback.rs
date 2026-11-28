use super::controls::*;
use macroquad::miniquad::{HapticFeedback, window::haptic_feedback};
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    input::feedback::{haptic, set_haptic_handler, set_sound_handler, sound},
    input::keyboard::KeyCode,
    paint::color::Color,
    style::{Styleable, StyleableExt},
};
use std::rc::Rc;

const PAGE: &str = "Feedback & directives";

thread_local! {
    static HAPTICS_ON: Signal<bool> = Scope::null().run(|| signal(true));
}

/// Log every cue and forward haptics to the platform while enabled.
fn install_handlers() {
    set_haptic_handler(|style| {
        let on = HAPTICS_ON.with(|on| on.get_untracked());
        log_event(format!(
            "haptic {style:?}{}",
            if on { "" } else { " (muted)" }
        ));
        if on {
            haptic_feedback(match style {
                HapticStyle::Light => HapticFeedback::Light,
                HapticStyle::Medium => HapticFeedback::Medium,
                HapticStyle::Heavy => HapticFeedback::Heavy,
                HapticStyle::Selection => HapticFeedback::Selection,
                HapticStyle::Success => HapticFeedback::Success,
                HapticStyle::Warning => HapticFeedback::Warning,
                HapticStyle::Error => HapticFeedback::Error,
            });
        }
    });
    // The playground ships no audio; an application maps cues to its samples.
    set_sound_handler(|cue| log_event(format!("sound {cue:?}")));
}

fn haptics_card() -> Node {
    let on = HAPTICS_ON.with(|on| *on);
    let styles = [
        ("Light", HapticStyle::Light),
        ("Medium", HapticStyle::Medium),
        ("Heavy", HapticStyle::Heavy),
        ("Selection", HapticStyle::Selection),
        ("Success", HapticStyle::Success),
        ("Warning", HapticStyle::Warning),
        ("Error", HapticStyle::Error),
    ];
    card(
        "Haptics",
        "Android performHapticFeedback, iOS feedback generators, and navigator.vibrate on the web. Desktops have no haptics; the log still shows each call.",
        (
            switch()
                .checked(on)
                .label(on.map(|on| Rc::<str>::from(if on { "Haptics on" } else { "Haptics muted" }))),
            row(styles
                .into_iter()
                .map(|(label, style)| button(label).on_activate(move || haptic(style)).into_node())
                .collect::<Vec<_>>()),
        ),
    )
}

fn sounds_card() -> Node {
    let cues = [
        ("Click", SoundCue::Click),
        ("Large click", SoundCue::ClickLarge),
        ("Toggle", SoundCue::Toggle),
        ("Success", SoundCue::Success),
        ("Error", SoundCue::Error),
    ];
    card(
        "Sound cues",
        "Components ask for a cue by name; the application decides what plays. Here every cue is logged.",
        row(cues
            .into_iter()
            .map(|(label, cue)| button(label).on_activate(move || sound(cue)).into_node())
            .collect::<Vec<_>>()),
    )
}

fn directives_card() -> Node {
    let count = signal(0i32);
    let saves = signal(0u32);
    let outside = signal(0u32);
    let stepper = |label: &'static str, delta: i32| {
        button(label)
            .style(mue_macroquad::style! { min-width: {u(48.)}; })
            .directive(LongPressRepeat::new(move || {
                count.update(|n| *n = (*n + delta).clamp(-99, 99))
            }))
            .directive(Haptic::on(FeedbackTrigger::Press, HapticStyle::Selection))
    };
    card(
        "Directives",
        "Reusable behavior attached with .directive(..), like Vue's custom directives.",
        stack()
            .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: {u(14.)}; })
            .children((
                field(
                    "LongPressRepeat",
                    row((
                        stepper("−", -1),
                        mono(count.map(|n| Rc::<str>::from(format!("{n:>3}")))),
                        stepper("+", 1),
                        hint("Hold a button: it repeats and speeds up"),
                    )),
                ),
                field(
                    "Sound + Haptic",
                    row((
                        button("Tap me")
                            .directive(Sound::new(SoundCue::Click))
                            .directive(Haptic::new(HapticStyle::Light)),
                        flexbox()
                            .style(mue_macroquad::style! {
                                padding: {(u(12.), u(8.))};
                                border-radius: {u(8.)};
                                background: {raised()};
                            })
                            .directive(Haptic::on(FeedbackTrigger::LongPress, HapticStyle::Heavy))
                            .directive(Sound::on(FeedbackTrigger::LongPress, SoundCue::ClickLarge))
                            .on_long_press(|_| {})
                            .children(text("Long-press me").style(mue_macroquad::style! {
                                font-size: {f(14.)}; line-height: {u(20.)};
                                color: {foreground()};
                            })),
                    )),
                ),
                field(
                    "Hotkey",
                    flexbox()
                        .focusable(true)
                        .focus_ring(2., BLUE, 2.)
                        .directive(Hotkey::command(KeyCode::S, move || {
                            saves.update(|n| *n += 1);
                            log_event("hotkey: Ctrl/Cmd+S");
                        }))
                        .style(mue_macroquad::style! {
                            padding: {(u(12.), u(8.))};
                            border-radius: {u(8.)};
                            background: {raised()};
                        })
                        .children(
                            text(saves.map(|n| {
                                Rc::<str>::from(format!(
                                    "Click here, then press Ctrl+S (Cmd+S on macOS): saved {n}×"
                                ))
                            }))
                            .style(mue_macroquad::style! {
                                font-size: {f(14.)}; line-height: {u(20.)};
                                color: {foreground()};
                            }),
                        ),
                ),
                field(
                    "ClickOutside",
                    flexbox()
                        .directive(ClickOutside::new(move || outside.update(|n| *n += 1)))
                        .style(mue_macroquad::style! {
                            padding: {(u(12.), u(8.))};
                            border-radius: {u(8.)};
                            border-width: 2px;
                            border-color: {Color::from_rgba(127, 209, 168, 255)};
                        })
                        .children(
                            text(outside.map(|n| {
                                Rc::<str>::from(format!("Presses outside this box: {n}"))
                            }))
                            .style(mue_macroquad::style! {
                                font-size: {f(14.)}; line-height: {u(20.)};
                                color: {foreground()};
                            }),
                        ),
                ),
            )),
    )
}

pub fn page() -> Node {
    install_handlers();
    page_body(
        "Haptics, sound cues, and the built-in directives.",
        (
            haptics_card(),
            sounds_card(),
            directives_card(),
            card("Events", "", event_log()),
            checklist(
                PAGE,
                &[
                    "Phones vibrate for each haptic style (and not while muted)",
                    "Every sound cue appears in the log",
                    "Holding + or − repeats and speeds up; a single tap steps once",
                    "Ctrl/Cmd+S counts only while the box is focused",
                    "ClickOutside counts presses outside the box, not inside",
                ],
            ),
        ),
    )
}
