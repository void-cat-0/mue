//! Interaction feedback: haptics and UI sounds.
//!
//! Components and directives call [`haptic`] and [`sound`] at meaningful
//! moments (a tap, a toggle, a long press). Haptics go to the platform by
//! default (Android, iOS, and browsers that support vibration); sounds do
//! nothing until the application installs a handler with
//! [`set_sound_handler`], since the sounds belong to the application. Either
//! handler can be replaced, for example to respect a settings toggle.
//!
//! ```
//! use mue_macroquad::feedback::{HapticStyle, SoundCue, haptic, set_sound_handler, sound};
//!
//! set_sound_handler(|cue| {
//!     // Play the application's own sample for `cue` here.
//!     let _ = cue;
//! });
//! sound(SoundCue::Click);
//! haptic(HapticStyle::Light);
//! ```

use std::{cell::RefCell, rc::Rc};

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

/// A UI sound the application maps to its own samples.
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

/// Play a UI sound through the installed handler (silent by default).
pub fn sound(cue: SoundCue) {
    if let Some(handler) = SOUND.with(|handler| handler.borrow().clone()) {
        handler(cue);
    }
}

/// Replace how haptics are played (for example to honor a settings toggle).
pub fn set_haptic_handler(handler: impl Fn(HapticStyle) + 'static) {
    HAPTIC.with(|current| *current.borrow_mut() = Some(Rc::new(handler)));
}

/// Install the application's UI sounds.
pub fn set_sound_handler(handler: impl Fn(SoundCue) + 'static) {
    SOUND.with(|current| *current.borrow_mut() = Some(Rc::new(handler)));
}

fn platform_haptic(style: HapticStyle) {
    if !crate::text_input::platform::host_available() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn handlers_receive_cues() {
        let played = Rc::new(Cell::new(None));
        let seen = played.clone();
        set_sound_handler(move |cue| seen.set(Some(cue)));
        sound(SoundCue::Toggle);
        assert_eq!(played.get(), Some(SoundCue::Toggle));
        let felt = Rc::new(Cell::new(None));
        let seen = felt.clone();
        set_haptic_handler(move |style| seen.set(Some(style)));
        haptic(HapticStyle::Selection);
        assert_eq!(felt.get(), Some(HapticStyle::Selection));
    }
}
