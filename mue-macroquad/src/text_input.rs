//! Text editing model: values with selection and composition, undo history,
//! and input formatters.
//!
//! The design follows Flutter's text input protocol: a text field owns a
//! [`TextEditingValue`] (text, selection, composing range) and every edit,
//! whether typed, pasted, or proposed by an input method, becomes a new value
//! that formatters may adjust before the field accepts it.

mod formatter;
mod history;
mod value;

pub use formatter::{
    AllowChars, DecimalNumber, MaxLength, SharedFormatter, SingleLine, TextFormatter, apply,
    digits_only,
};
pub use history::{EditKind, UndoHistory};
pub use value::{
    TextEditingValue, TextSelection, TextUnit, byte_to_utf16, grapheme_len, next_boundary,
    paragraph_at, previous_boundary, utf16_to_byte, word_at,
};
