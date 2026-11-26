//! The editing state shared by text fields and platform input methods.
//!
//! Offsets are UTF-8 byte offsets into [`TextEditingValue::text`] and always
//! lie on `char` boundaries. Platform bridges convert to UTF-16 at the edge
//! ([`utf16_to_byte`], [`byte_to_utf16`]).

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// A caret (`base == extent`) or a selection from `base` (anchor) to `extent`
/// (the moving end). Either end may come first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextSelection {
    pub base: usize,
    pub extent: usize,
}

impl TextSelection {
    pub const fn caret(offset: usize) -> Self {
        Self {
            base: offset,
            extent: offset,
        }
    }

    pub const fn new(base: usize, extent: usize) -> Self {
        Self { base, extent }
    }

    pub fn is_collapsed(&self) -> bool {
        self.base == self.extent
    }

    pub fn start(&self) -> usize {
        self.base.min(self.extent)
    }

    pub fn end(&self) -> usize {
        self.base.max(self.extent)
    }

    pub fn range(&self) -> Range<usize> {
        self.start()..self.end()
    }
}

/// Text, selection, and the range an input method is still composing
/// (preedit). Mirrors Flutter's `TextEditingValue` and Android's `Editable`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextEditingValue {
    pub text: String,
    pub selection: TextSelection,
    /// Text inside this range is provisional: the input method may replace it.
    pub composing: Option<Range<usize>>,
}

impl mue_core::prop::PropValue for TextEditingValue {}

impl TextEditingValue {
    /// Text with the caret at the end.
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let end = text.len();
        Self {
            text,
            selection: TextSelection::caret(end),
            composing: None,
        }
    }

    pub fn selected_text(&self) -> &str {
        &self.text[self.selection.range()]
    }

    pub fn is_composing(&self) -> bool {
        self.composing
            .as_ref()
            .is_some_and(|range| !range.is_empty())
    }

    /// Clamp offsets into the text and onto `char` boundaries.
    pub fn normalized(mut self) -> Self {
        let fix = |text: &str, offset: usize| floor_char_boundary(text, offset.min(text.len()));
        self.selection = TextSelection::new(
            fix(&self.text, self.selection.base),
            fix(&self.text, self.selection.extent),
        );
        self.composing = self.composing.and_then(|range| {
            let start = fix(&self.text, range.start);
            let end = fix(&self.text, range.end.max(range.start));
            (start < end).then_some(start..end)
        });
        self
    }

    /// Replace `range` with `replacement` and put the caret after it.
    pub fn replaced(&self, range: Range<usize>, replacement: &str) -> Self {
        let mut text = String::with_capacity(self.text.len() + replacement.len());
        text.push_str(&self.text[..range.start]);
        text.push_str(replacement);
        text.push_str(&self.text[range.end..]);
        Self {
            text,
            selection: TextSelection::caret(range.start + replacement.len()),
            composing: None,
        }
    }

    /// Insert `text` over the selection (or the composing range, which it ends).
    pub fn insert(&self, text: &str) -> Self {
        let range = self
            .composing
            .clone()
            .unwrap_or_else(|| self.selection.range());
        self.replaced(range, text)
    }

    /// Show `preedit` as provisional text over the composing range or
    /// selection, with the caret `cursor` bytes into it.
    pub fn compose(&self, preedit: &str, cursor: Option<usize>) -> Self {
        let range = self
            .composing
            .clone()
            .unwrap_or_else(|| self.selection.range());
        let mut next = self.replaced(range.clone(), preedit);
        if preedit.is_empty() {
            return next;
        }
        let cursor =
            floor_char_boundary(preedit, cursor.unwrap_or(preedit.len()).min(preedit.len()));
        next.selection = TextSelection::caret(range.start + cursor);
        next.composing = Some(range.start..range.start + preedit.len());
        next
    }

    /// Keep the composed text and end composition.
    pub fn finish_composing(&self) -> Self {
        Self {
            composing: None,
            ..self.clone()
        }
    }

    /// Delete the selection, or the grapheme before the caret.
    pub fn delete_backward(&self, unit: TextUnit) -> Self {
        if !self.selection.is_collapsed() {
            return self.replaced(self.selection.range(), "");
        }
        let end = self.selection.extent;
        let start = previous_boundary(&self.text, end, unit);
        self.replaced(start..end, "")
    }

    /// Delete the selection, or the grapheme after the caret.
    pub fn delete_forward(&self, unit: TextUnit) -> Self {
        if !self.selection.is_collapsed() {
            return self.replaced(self.selection.range(), "");
        }
        let start = self.selection.extent;
        let end = next_boundary(&self.text, start, unit);
        self.replaced(start..end, "")
    }

    /// Move the caret horizontally in logical order. A collapsing move on a
    /// selection lands on the selection edge, like native text fields.
    pub fn moved(&self, forward: bool, unit: TextUnit, extend: bool) -> Self {
        let selection = self.selection;
        let extent = if !extend && !selection.is_collapsed() && unit == TextUnit::Grapheme {
            if forward {
                selection.end()
            } else {
                selection.start()
            }
        } else if forward {
            next_boundary(&self.text, selection.extent, unit)
        } else {
            previous_boundary(&self.text, selection.extent, unit)
        };
        self.with_extent(extent, extend)
    }

    /// Move the selection's extent to `offset`, keeping the base when extending.
    pub fn with_extent(&self, offset: usize, extend: bool) -> Self {
        let offset = floor_char_boundary(&self.text, offset.min(self.text.len()));
        Self {
            selection: if extend {
                TextSelection::new(self.selection.base, offset)
            } else {
                TextSelection::caret(offset)
            },
            composing: None,
            text: self.text.clone(),
        }
    }

    pub fn select_all(&self) -> Self {
        Self {
            selection: TextSelection::new(0, self.text.len()),
            composing: None,
            text: self.text.clone(),
        }
    }
}

/// Caret movement and deletion granularity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextUnit {
    /// One user-perceived character (extended grapheme cluster).
    Grapheme,
    /// To the previous word start or the next word end, skipping spaces and
    /// punctuation, as Ctrl/Option+arrow do.
    Word,
    /// To the start or end of the paragraph (text between newlines).
    Paragraph,
    /// To the start or end of the whole text.
    Document,
}

pub(crate) fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// The boundary of `unit` before `offset`.
pub fn previous_boundary(text: &str, offset: usize, unit: TextUnit) -> usize {
    let offset = floor_char_boundary(text, offset);
    match unit {
        TextUnit::Grapheme => text[..offset]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(index, _)| index),
        TextUnit::Word => {
            let mut start = 0;
            for (index, word) in text[..offset].split_word_bound_indices().rev() {
                if is_word(word) {
                    start = index;
                    break;
                }
            }
            start
        }
        TextUnit::Paragraph => {
            let before = &text[..offset];
            // At a paragraph start, move to the previous paragraph's start.
            let search = before.strip_suffix('\n').unwrap_or(before);
            if search.len() < before.len() && offset > 0 {
                return search.rfind('\n').map_or(0, |index| index + 1);
            }
            before.rfind('\n').map_or(0, |index| index + 1)
        }
        TextUnit::Document => 0,
    }
}

/// The boundary of `unit` after `offset`.
pub fn next_boundary(text: &str, offset: usize, unit: TextUnit) -> usize {
    let offset = floor_char_boundary(text, offset);
    match unit {
        TextUnit::Grapheme => text[offset..]
            .graphemes(true)
            .next()
            .map_or(text.len(), |grapheme| offset + grapheme.len()),
        TextUnit::Word => {
            for (index, word) in text[offset..].split_word_bound_indices() {
                if is_word(word) {
                    return offset + index + word.len();
                }
            }
            text.len()
        }
        TextUnit::Paragraph => text[offset..]
            .find('\n')
            .map_or(text.len(), |index| offset + index),
        TextUnit::Document => text.len(),
    }
}

fn is_word(segment: &str) -> bool {
    segment.chars().any(char::is_alphanumeric)
}

/// The word (or run of spaces/punctuation) around `offset`, for double-click.
pub fn word_at(text: &str, offset: usize) -> Range<usize> {
    let offset = floor_char_boundary(text, offset);
    let mut found = None;
    for (index, segment) in text.split_word_bound_indices() {
        let range = index..index + segment.len();
        // Prefer a word on either side of the caret, as native fields do.
        if range.contains(&offset) || (range.end == offset && is_word(segment)) {
            if is_word(segment) || found.is_none() {
                found = Some(range.clone());
            }
            if range.contains(&offset) {
                break;
            }
        }
    }
    found.unwrap_or(offset..offset)
}

/// The paragraph around `offset`, for triple-click (without its newline).
pub fn paragraph_at(text: &str, offset: usize) -> Range<usize> {
    let offset = floor_char_boundary(text, offset);
    let start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let end = text[offset..]
        .find('\n')
        .map_or(text.len(), |index| offset + index);
    start..end
}

/// Convert a UTF-16 offset from a platform API to a byte offset.
pub fn utf16_to_byte(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (index, ch) in text.char_indices() {
        if units >= utf16 {
            return index;
        }
        units += ch.len_utf16();
    }
    text.len()
}

/// Convert a byte offset to a UTF-16 offset for a platform API.
pub fn byte_to_utf16(text: &str, byte: usize) -> usize {
    text[..floor_char_boundary(text, byte)]
        .chars()
        .map(char::len_utf16)
        .sum()
}

/// Number of user-perceived characters, as `max_length` counts them.
pub fn grapheme_len(text: &str) -> usize {
    text.graphemes(true).count()
}
