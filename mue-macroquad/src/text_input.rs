//! Text editing: the editing model, undo, formatters, and the bridge to
//! platform input methods (IME, soft keyboards).
//!
//! The design follows Flutter's text input protocol. A focused field is the
//! *client* of a platform *connection*:
//!
//! - the field owns a [`TextEditingValue`] (text, selection, composing range);
//! - on desktop the platform sends composition (preedit) and commit events,
//!   which the field applies to its value;
//! - on Android, iOS, and the web the platform keeps a mirror of the value
//!   (`Editable`, a hidden text view, a hidden `<textarea>`), sends the whole
//!   new state after each input-method edit, and receives the field's value
//!   back after local edits;
//! - the field reports its caret rectangle so candidate windows and soft
//!   keyboards can be positioned, and the platform reports the keyboard's
//!   frame so the UI can scroll the field into view.
//!
//! The renderer's editable text node, `node::text_edit`, is built on this
//! module.

mod formatter;
mod history;
pub(crate) mod platform;
mod value;

pub use formatter::{
    AllowChars, DecimalNumber, MaxLength, SharedFormatter, SingleLine, TextFormatter, apply,
    digits_only,
};
pub use history::{EditKind, UndoHistory};
pub use platform::{
    ImeEvent, TextCapitalization, TextInputAction, TextInputConfig, TextInputConnection,
    TextInputKind, attach, keyboard_frame, keyboard_inset, keyboard_visible, on_text_input_action,
    take_events,
};
pub(crate) use platform::{
    PlatformEvent, dispatch_platform_event, from_miniquad_action, set_host_available,
};
pub(crate) use value::floor_char_boundary as floor;
pub use value::{
    TextEditingValue, TextSelection, TextUnit, byte_to_utf16, grapheme_len, next_boundary,
    paragraph_at, previous_boundary, utf16_to_byte, word_at,
};
