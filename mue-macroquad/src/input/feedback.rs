//! Interaction feedback: haptics and UI sounds.
//!
//! Components and directives call [`haptic`] and [`sound`] at meaningful
//! moments (a tap, a toggle, a long press). Haptics go to the platform by
//! default (Android, iOS, and browsers that support vibration). With the
//! `sound` feature, sounds play built-in samples through
//! [sasa](https://github.com/Mivik/sasa) on desktop, Android, and iOS;
//! `set_sound_sample` replaces a cue's sample with the application's own
//! file and `set_sound_volume` scales them all. Without the feature (and on
//! the web) sounds are silent unless the application installs a handler.
//! Either handler can be replaced with [`set_haptic_handler`] and
//! [`set_sound_handler`], for example to respect a settings toggle;
//! `play_sound` still plays the built-in sample from inside a handler.
//!
//! ```
//! use mue_macroquad::input::feedback::{HapticStyle, SoundCue, haptic, set_sound_handler, sound};
//!
//! set_sound_handler(|cue| {
//!     // Play the application's own sample for `cue` here.
//!     let _ = cue;
//! });
//! sound(SoundCue::Click);
//! haptic(HapticStyle::Light);
//! ```

use std::{cell::RefCell, rc::Rc};

#[cfg(all(feature = "sound", not(target_arch = "wasm32")))]
pub use super::sound::{play_sound, set_sound_sample, set_sound_volume};

/// A haptic pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HapticStyle {
    Light,
    Medium,
    Heavy,
    /// Moving through discrete values (pickers, sliders, switches).
    Selection,
    Success,
    Warning,
    Error,
}

/// A UI sound: a built-in sample with the `sound` feature, or the
/// application's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SoundCue {
    Click,
    /// A larger confirmation click (primary buttons, dialogs).
    ClickLarge,
    Toggle,
    Success,
    Error,
}

type Handler<T> = Rc<dyn Fn(T)>;

thread_local! {
    static HAPTIC: RefCell<Option<Handler<HapticStyle>>> = const { RefCell::new(None) };
    static SOUND: RefCell<Option<Handler<SoundCue>>> = const { RefCell::new(None) };
}

/// Play a haptic pattern through the installed handler (the platform by default).
pub fn haptic(style: HapticStyle) {
    match HAPTIC.with(|handler| handler.borrow().clone()) {
        Some(handler) => handler(style),
        None => platform_haptic(style),
    }
}

/// Play a UI sound through the installed handler, or the built-in sample
/// with the `sound` feature (silent otherwise).
pub fn sound(cue: SoundCue) {
    match SOUND.with(|handler| handler.borrow().clone()) {
        Some(handler) => handler(cue),
        None => platform_sound(cue),
    }
}

#[cfg(all(feature = "sound", not(target_arch = "wasm32")))]
fn platform_sound(cue: SoundCue) {
    if crate::input::text_input::platform::host_available() {
        play_sound(cue);
    }
}

#[cfg(not(all(feature = "sound", not(target_arch = "wasm32"))))]
fn platform_sound(_cue: SoundCue) {}

/// Replace how haptics are played (for example to honor a settings toggle).
pub fn set_haptic_handler(handler: impl Fn(HapticStyle) + 'static) {
    HAPTIC.with(|current| *current.borrow_mut() = Some(Rc::new(handler)));
}

/// Install the application's UI sounds.
pub fn set_sound_handler(handler: impl Fn(SoundCue) + 'static) {
    SOUND.with(|current| *current.borrow_mut() = Some(Rc::new(handler)));
}

fn platform_haptic(style: HapticStyle) {
    if !crate::input::text_input::platform::host_available() {
        return;
    }
    use miniquad::HapticFeedback as H;
    miniquad::window::haptic_feedback(match style {
        HapticStyle::Light => H::Light,
        HapticStyle::Medium => H::Medium,
        HapticStyle::Heavy => H::Heavy,
        HapticStyle::Selection => H::Selection,
        HapticStyle::Success => H::Success,
        HapticStyle::Warning => H::Warning,
        HapticStyle::Error => H::Error,
    });
}
