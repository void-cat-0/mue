//! Input formatters: rules applied to every proposed edit, whether it came
//! from typing, paste, or an input method (Flutter's `TextInputFormatter`).

use std::rc::Rc;

use super::value::{TextEditingValue, TextSelection, floor_char_boundary, grapheme_len};
use unicode_segmentation::UnicodeSegmentation;

/// Turns a proposed value into the value the field accepts.
///
/// `old` is the current value. Return `old` to reject the edit.
pub trait TextFormatter {
    fn format(&self, old: &TextEditingValue, new: TextEditingValue) -> TextEditingValue;
}

impl<F> TextFormatter for F
where
    F: Fn(&TextEditingValue, TextEditingValue) -> TextEditingValue,
{
    fn format(&self, old: &TextEditingValue, new: TextEditingValue) -> TextEditingValue {
        self(old, new)
    }
}

pub type SharedFormatter = Rc<dyn TextFormatter>;

/// Limit the text to `max` user-perceived characters.
///
/// Like native fields, the limit waits while an input method is composing,
/// so a CJK syllable is not cut in the middle, and applies when it commits.
pub struct MaxLength(pub usize);

impl TextFormatter for MaxLength {
    fn format(&self, old: &TextEditingValue, new: TextEditingValue) -> TextEditingValue {
        if new.is_composing() || grapheme_len(&new.text) <= self.0 {
            return new;
        }
        if grapheme_len(&old.text) >= self.0
            && !old.is_composing()
            && new.text.len() > old.text.len()
        {
            return old.clone();
        }
        let cut = new
            .text
            .grapheme_indices(true)
            .nth(self.0)
            .map_or(new.text.len(), |(index, _)| index);
        let mut text = new.text;
        text.truncate(cut);
        let clamp = |offset: usize| floor_char_boundary(&text, offset.min(cut));
        TextEditingValue {
            selection: TextSelection::new(clamp(new.selection.base), clamp(new.selection.extent)),
            composing: None,
            text,
        }
    }
}

/// Replace line breaks with spaces (single-line fields).
pub struct SingleLine;

impl TextFormatter for SingleLine {
    fn format(&self, _old: &TextEditingValue, mut new: TextEditingValue) -> TextEditingValue {
        if new.text.contains(['\n', '\r']) {
            // Same byte length, so offsets stay valid.
            new.text = new.text.replace(['\n', '\r'], " ");
        }
        new
    }
}

/// Keep only characters accepted by a predicate; offsets are remapped.
pub struct AllowChars<F: Fn(char) -> bool>(pub F);

impl<F: Fn(char) -> bool> TextFormatter for AllowChars<F> {
    fn format(&self, _old: &TextEditingValue, new: TextEditingValue) -> TextEditingValue {
        if new.text.chars().all(&self.0) {
            return new;
        }
        let mut text = String::with_capacity(new.text.len());
        let mut map = Vec::with_capacity(new.text.len() + 1);
        for (index, ch) in new.text.char_indices() {
            while map.len() <= index {
                map.push(text.len());
            }
            if (self.0)(ch) {
                text.push(ch);
            }
        }
        while map.len() <= new.text.len() {
            map.push(text.len());
        }
        let remap = |offset: usize| map[offset.min(new.text.len())];
        TextEditingValue {
            selection: TextSelection::new(remap(new.selection.base), remap(new.selection.extent)),
            composing: new
                .composing
                .map(|range| remap(range.start)..remap(range.end))
                .filter(|range| !range.is_empty()),
            text,
        }
    }
}

/// ASCII digits only.
pub fn digits_only() -> AllowChars<fn(char) -> bool> {
    AllowChars(|ch| ch.is_ascii_digit())
}

/// Digits, one decimal point, and a leading minus sign. Edits that would make
/// the text an invalid prefix of a decimal number are rejected.
pub struct DecimalNumber {
    pub signed: bool,
}

impl TextFormatter for DecimalNumber {
    fn format(&self, old: &TextEditingValue, new: TextEditingValue) -> TextEditingValue {
        let text = new.text.as_str();
        let body = if self.signed {
            text.strip_prefix('-').unwrap_or(text)
        } else {
            text
        };
        let valid = body.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
            && body.matches('.').count() <= 1;
        if valid || new.is_composing() {
            new
        } else {
            old.clone()
        }
    }
}

/// Apply formatters in order.
pub fn apply(
    formatters: &[SharedFormatter],
    old: &TextEditingValue,
    new: TextEditingValue,
) -> TextEditingValue {
    formatters.iter().fold(new, |value, formatter| {
        formatter.format(old, value).normalized()
    })
}
