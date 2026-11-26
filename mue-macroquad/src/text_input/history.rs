//! Undo and redo for one text field.

use super::value::TextEditingValue;

/// What kind of edit produced a value. Consecutive edits of the same kind
/// within [`UndoHistory::coalesce_seconds`] merge into one undo step, so undo
/// removes a typed word rather than a single letter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Typing,
    Deleting,
    /// Paste, cut, drop, or a programmatic change: always its own step.
    Other,
}

/// Linear undo stack of whole values (selection included). Composition
/// states are never recorded: only the committed result is.
#[derive(Clone, Debug)]
pub struct UndoHistory {
    undo: Vec<TextEditingValue>,
    redo: Vec<TextEditingValue>,
    last_kind: Option<EditKind>,
    last_time: f64,
    pub coalesce_seconds: f64,
    pub limit: usize,
}

impl Default for UndoHistory {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            last_kind: None,
            last_time: f64::NEG_INFINITY,
            coalesce_seconds: 1.0,
            limit: 200,
        }
    }
}

impl UndoHistory {
    /// Record that `before` changed into a new value by an edit of `kind` at `time`.
    pub fn record(&mut self, before: &TextEditingValue, kind: EditKind, time: f64) {
        if before.is_composing() {
            return;
        }
        let merge = kind != EditKind::Other
            && self.last_kind == Some(kind)
            && time - self.last_time <= self.coalesce_seconds
            && !self.undo.is_empty();
        if !merge {
            self.undo.push(before.clone());
            if self.undo.len() > self.limit {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_kind = Some(kind);
        self.last_time = time;
    }

    /// Break coalescing, for example after the caret moves.
    pub fn seal(&mut self) {
        self.last_kind = None;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self, current: &TextEditingValue) -> Option<TextEditingValue> {
        let previous = self.undo.pop()?;
        self.redo.push(current.finish_composing());
        self.seal();
        Some(previous)
    }

    pub fn redo(&mut self, current: &TextEditingValue) -> Option<TextEditingValue> {
        let next = self.redo.pop()?;
        self.undo.push(current.finish_composing());
        self.seal();
        Some(next)
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.seal();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_coalesces_and_other_edits_do_not() {
        let mut history = UndoHistory::default();
        let mut value = TextEditingValue::default();
        for (i, ch) in "abc".chars().enumerate() {
            history.record(&value, EditKind::Typing, i as f64 * 0.1);
            value = value.insert(&ch.to_string());
        }
        history.record(&value, EditKind::Other, 0.4);
        value = value.insert("PASTE");
        let value = history.undo(&value).unwrap();
        assert_eq!(value.text, "abc");
        let value = history.undo(&value).unwrap();
        assert_eq!(value.text, "");
        assert!(!history.can_undo());
        let value = history.redo(&value).unwrap();
        assert_eq!(value.text, "abc");
    }

    #[test]
    fn a_pause_or_a_seal_starts_a_new_step() {
        let mut history = UndoHistory::default();
        let mut value = TextEditingValue::default();
        history.record(&value, EditKind::Typing, 0.);
        value = value.insert("a");
        history.record(&value, EditKind::Typing, 5.);
        value = value.insert("b");
        history.seal();
        history.record(&value, EditKind::Typing, 5.1);
        value = value.insert("c");
        assert_eq!(history.undo(&value).unwrap().text, "ab");
    }

    #[test]
    fn a_new_edit_clears_redo() {
        let mut history = UndoHistory::default();
        let value = TextEditingValue::default();
        history.record(&value, EditKind::Other, 0.);
        let typed = value.insert("x");
        let undone = history.undo(&typed).unwrap();
        history.record(&undone, EditKind::Other, 1.);
        assert!(!history.can_redo());
    }
}
