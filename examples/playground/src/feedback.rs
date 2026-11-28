use super::controls::*;
use macroquad::miniquad::{HapticFeedback, window::haptic_feedback};
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    input::feedback::{haptic, set_haptic_handler, set_sound_handler, sound},
    input::keyboard::KeyCode,
    style::{Styleable, StyleableExt},
};
use std::rc::Rc;

const PAGE: &str = "Feedback & directives";

/// The save shortcut as the platform writes it.
const SAVE_KEYS: &str = if cfg!(any(target_os = "macos", target_os = "ios")) { "Cmd+S" } else { "Ctrl+S" };

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
    // Play the built-in sample and show the cue in the log.
    set_sound_handler(|cue| {
        // The built-in samples need the sound feature, which the web lacks.
        #[cfg(not(target_arch = "wasm32"))]
        mue_macroquad::input::feedback::play_sound(cue);
        log_event(format!("sound {cue:?}"));
    });
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
        (
            switch()
                .checked(on)
                .label("Vibrate"),
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
    // The web plays no built-in samples, so it has no volume either.
    #[cfg(target_arch = "wasm32")]
    let volume: Vec<Node> = Vec::new();
    #[cfg(not(target_arch = "wasm32"))]
    let volume: Vec<Node> = vec![field(
        "Volume",
        slider()
            .value(signal(100_f32))
            .on_change(|level: &f32| mue_macroquad::input::feedback::set_sound_volume(*level / 100.)),
    )];
    card(
        "Sound cues",
        (
            row(cues
                .into_iter()
                .map(|(label, cue)| button(label).on_activate(move || sound(cue)).into_node())
                .collect::<Vec<_>>()),
            volume,
        ),
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
        stack()
            .style(mue_macroquad::style! { width: 100%; min-width: 0px; gap: {u(14.)}; })
            .children((
                field(
                    "LongPressRepeat",
                    row((
                        stepper("−", -1),
                        mono(count.map(|n| Rc::<str>::from(format!("{n:>3}")))),
                        stepper("+", 1),
                        hint("Hold to repeat"),
                    )),
                ),
                field(
                    "Sound, Haptic",
                    row((
                        button("Play click")
                            .directive(Sound::new(SoundCue::Click))
                            .directive(Haptic::new(HapticStyle::Light)),
                        flexbox()
                            .style(mue_macroquad::style! {
                                padding: {(u(12.), u(8.))};
                                border-radius: {mue_components::system::radius::MD};
                                background: {raised()};
                            })
                            .directive(Haptic::on(FeedbackTrigger::LongPress, HapticStyle::Heavy))
                            .directive(Sound::on(FeedbackTrigger::LongPress, SoundCue::ClickLarge))
                            .on_long_press(|_| {})
                            .children(text("Long-press").style(mue_macroquad::style! {
                                font-size: {f(14.)}; line-height: {u(20.)};
                                color: {foreground()};
                            })),
                    )),
                ),
                field(
                    "Hotkey",
                    flexbox()
                        .focusable(true)
                        .focus_ring(2., mue_components::system::Palette::Blue.shade(400), 2.)
                        .directive(Hotkey::command(KeyCode::S, move || {
                            saves.update(|n| *n += 1);
                            log_event(format!("hotkey {SAVE_KEYS}"));
                        }))
                        .style(mue_macroquad::style! {
                            padding: {(u(12.), u(8.))};
                            border-radius: {mue_components::system::radius::MD};
                            background: {raised()};
                        })
                        .children(
                            text(saves.map(|n| {
                                Rc::<str>::from(format!("Click here, then press {SAVE_KEYS} ({n} saved)"))
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
                            border-radius: {mue_components::system::radius::MD};
                            border-width: 1px;
                            border-color: {border()};
                        })
                        .children(
                            text(outside.map(|n| {
                                Rc::<str>::from(format!("{n} presses outside this box"))
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
        (
            haptics_card(),
            sounds_card(),
            directives_card(),
            card("Events", event_log()),
            checklist(
                PAGE,
                &[
                    "Phones vibrate for each style, and not with Vibrate off",
                    "Every sound cue plays and appears in the log",
                    "Volume scales the cues, and 0 silences them",
                    "Holding + or − repeats faster, a tap steps once",
                    "The save keys count only while the box is focused",
                    "ClickOutside counts only presses outside the box",
                ],
            ),
        ),
    )
}
