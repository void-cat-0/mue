//! The editable text node: [`text_edit()`].
//!
//! A focused `text_edit` is the client of the platform text input connection
//! ([`crate::input::text_input`]). It keeps its own [`TextEditingValue`], applies
//! keyboard, pointer, clipboard, and input-method edits to it, runs input
//! formatters, records undo history, and draws the text with its selection,
//! composition underline, and caret. Layout and hit testing reuse
//! cosmic-text's `Buffer` (line layout, `hit`, vertical and line motions).
//!
//! ```
//! use mue_core::prelude::*;
//! use mue_macroquad::{node::text_edit, style::Styleable, input::text_input::TextInputKind};
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let name = signal(String::new());
//! let _field = text_edit()
//!     .value(name)
//!     .placeholder("Your name")
//!     .max_length(Some(32))
//!     .on_submit(move |_| println!("submitted {}", name.get_clone()))
//!     .width(240.);
//! let _password = text_edit().kind(TextInputKind::Password);
//! # });
//! ```

use std::{
    cell::{Cell, RefCell},
    ops::Range,
    rc::{Rc, Weak},
};

use cosmic_text::{Attrs, Buffer, Cursor, Metrics, Motion, Shaping, Wrap};
use mue_core::{
    effect::{computed, watch_effect},
    prop::Prop,
    signal::{Access, Signal, signal},
};
use taffy::{AvailableSpace, Size};
use unicode_segmentation::UnicodeSegmentation;

use super::{BuildNode, Node, NodeContext, NodeHandle, text::FontState};
use crate::{
    app::runtime::Runtime,
    input::text_input::{
        self, EditKind, ImeEvent, MaxLength, SharedFormatter, SingleLine, TextCapitalization,
        TextEditingValue, TextInputAction, TextInputConfig, TextInputKind, TextSelection, TextUnit,
        UndoHistory,
    },
    input::{
        keyboard::{CharEvent, KeyCode, KeyEvent},
        pointer::{ButtonState, PointerAction, PointerEvent, PointerType},
    },
    layout::use_layout,
    math::{Rect, Vector, vec2},
    paint::color::Color,
    paint::path::{FillRule, PathData},
    paint::shader::Brush,
    paint::{current_transform, use_background, use_paint},
    style::{Style, Styleable, StyleableExt},
};

type TextHandler = Rc<dyn Fn(&str)>;
type ActionHandler = Rc<dyn Fn(TextInputAction)>;

/// Seconds between caret blink phases.
const BLINK: f64 = 0.53;
/// Seconds within which presses count as double and triple clicks.
const MULTI_CLICK: f32 = 0.45;
/// Touch movement after which a press is a scroll, not a tap.
const TAP_SLOP: f32 = 10.;
/// Hold time that selects a word on touch.
const LONG_PRESS: f32 = 0.5;

/// Imperative access to a text field (Vue's `defineExpose`): bind with
/// [`TextEditBuilder::handle`], then focus, select, or edit from code.
#[derive(Clone, Default)]
pub struct TextEditHandle(Rc<RefCell<Weak<Field>>>);

impl TextEditHandle {
    pub fn new() -> Self {
        Self::default()
    }

    fn field(&self) -> Option<Rc<Field>> {
        self.0.borrow().upgrade()
    }

    /// Focus the field. Returns false when it is not mounted or disabled.
    pub fn focus(&self) -> bool {
        self.field()
            .and_then(|field| field.node.get_untracked())
            .is_some_and(|node| node.focus())
    }

    pub fn blur(&self) {
        if let Some(node) = self.field().and_then(|field| field.node.get_untracked()) {
            node.blur();
        }
    }

    /// The current editing value.
    pub fn value(&self) -> Option<TextEditingValue> {
        self.field().map(|field| field.state.borrow().value.clone())
    }

    pub fn select_all(&self) {
        if let Some(field) = self.field() {
            let next = field.state.borrow().value.select_all();
            field.set_selection(next.selection);
        }
    }

    /// Set the selection (byte offsets, clamped to char boundaries).
    pub fn set_selection(&self, selection: TextSelection) {
        if let Some(field) = self.field() {
            field.set_selection(selection);
        }
    }

    /// Insert text over the selection, as typing would (formatters apply).
    pub fn insert(&self, text: &str) {
        if let Some(field) = self.field() {
            let next = field.state.borrow().value.insert(text);
            field.edit(next, EditKind::Other);
        }
    }

    /// Replace the whole text (undoable).
    pub fn set_text(&self, text: &str) {
        if let Some(field) = self.field() {
            field.edit(TextEditingValue::new(text), EditKind::Other);
        }
    }

    pub fn undo(&self) {
        if let Some(field) = self.field() {
            field.undo(false);
        }
    }

    pub fn redo(&self) {
        if let Some(field) = self.field() {
            field.undo(true);
        }
    }
}

/// Builder returned by [`text_edit()`].
pub struct TextEditBuilder {
    value: Option<Signal<String>>,
    editing: Option<Signal<TextEditingValue>>,
    placeholder: Prop<std::rc::Rc<str>>,
    config: TextInputConfig,
    max_length: Option<usize>,
    formatters: Vec<SharedFormatter>,
    disabled: Prop<bool>,
    read_only: Prop<bool>,
    select_all_on_focus: bool,
    min_lines: usize,
    mask: char,
    caret_color: Option<Prop<Brush>>,
    selection_color: Prop<Brush>,
    placeholder_color: Option<Prop<Brush>>,
    on_change: Vec<TextHandler>,
    on_submit: Vec<ActionHandler>,
    on_blur_commit: Vec<TextHandler>,
    handle: Option<TextEditHandle>,
    style: Style,
}

/// An editable text field. Single-line by default; see
/// [`TextEditBuilder::multiline`] and [`TextEditBuilder::kind`].
///
/// The field is focusable and shows a text cursor. Font properties and the
/// foreground `color` work as on `text` nodes; padding and borders surround
/// the editing area, which scrolls to keep the caret visible.
pub fn text_edit() -> TextEditBuilder {
    TextEditBuilder {
        value: None,
        editing: None,
        placeholder: Prop::Static(Rc::from("")),
        config: TextInputConfig::default(),
        max_length: None,
        formatters: Vec::new(),
        disabled: Prop::Static(false),
        read_only: Prop::Static(false),
        select_all_on_focus: false,
        min_lines: 1,
        mask: '•',
        caret_color: None,
        selection_color: Prop::Static(Brush::Solid(Color::from_rgba(80, 140, 255, 110))),
        placeholder_color: None,
        on_change: Vec::new(),
        on_submit: Vec::new(),
        on_blur_commit: Vec::new(),
        handle: None,
        style: Style::new(),
    }
}

impl TextEditBuilder {
    /// Two-way bind the text. External writes replace the text and move the
    /// caret to the end; edits write the signal.
    pub fn value(mut self, value: Signal<String>) -> Self {
        self.value = Some(value);
        self
    }

    /// Two-way bind the whole editing value (text, selection, composition).
    pub fn editing_value(mut self, value: Signal<TextEditingValue>) -> Self {
        self.editing = Some(value);
        self
    }

    pub fn placeholder(mut self, text: impl Into<Prop<Rc<str>>>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// The keyboard and input-method kind. `Password` masks the text and
    /// disables composition; `Multiline` is the same as [`Self::multiline`].
    pub fn kind(mut self, kind: TextInputKind) -> Self {
        self.config.kind = kind;
        self
    }

    /// Several lines: Enter inserts a newline and text wraps.
    pub fn multiline(mut self, multiline: bool) -> Self {
        if multiline {
            self.config.kind = TextInputKind::Multiline;
        } else if self.config.kind == TextInputKind::Multiline {
            self.config.kind = TextInputKind::Text;
        }
        self
    }

    /// The soft keyboard's action key (and what Enter reports in a single-line field).
    pub fn action(mut self, action: TextInputAction) -> Self {
        self.config.action = action;
        self
    }

    pub fn autocorrect(mut self, autocorrect: bool) -> Self {
        self.config.autocorrect = autocorrect;
        self
    }

    pub fn capitalization(mut self, capitalization: TextCapitalization) -> Self {
        self.config.capitalization = capitalization;
        self
    }

    /// Limit the text to this many user-perceived characters.
    pub fn max_length(mut self, max: Option<usize>) -> Self {
        self.max_length = max;
        self
    }

    /// Run an input formatter on every proposed edit, after earlier ones.
    pub fn formatter(mut self, formatter: impl text_input::TextFormatter + 'static) -> Self {
        self.formatters.push(Rc::new(formatter));
        self
    }

    /// Not focusable and not editable.
    pub fn disabled(mut self, disabled: impl Into<Prop<bool>>) -> Self {
        self.disabled = disabled.into();
        self
    }

    /// Focusable and selectable, but not editable; no soft keyboard.
    pub fn read_only(mut self, read_only: impl Into<Prop<bool>>) -> Self {
        self.read_only = read_only.into();
        self
    }

    pub fn select_all_on_focus(mut self, select: bool) -> Self {
        self.select_all_on_focus = select;
        self
    }

    /// Minimum visible lines of a multiline field when its height is auto.
    pub fn min_lines(mut self, lines: usize) -> Self {
        self.min_lines = lines.max(1);
        self
    }

    /// The character shown for each character of a password.
    pub fn mask(mut self, mask: char) -> Self {
        self.mask = mask;
        self
    }

    /// Caret color; defaults to the foreground color.
    pub fn caret_color(mut self, brush: impl Into<Prop<Brush>>) -> Self {
        self.caret_color = Some(brush.into());
        self
    }

    pub fn selection_color(mut self, brush: impl Into<Prop<Brush>>) -> Self {
        self.selection_color = brush.into();
        self
    }

    /// Placeholder color; defaults to the foreground color at 45% opacity.
    pub fn placeholder_color(mut self, brush: impl Into<Prop<Brush>>) -> Self {
        self.placeholder_color = Some(brush.into());
        self
    }

    /// Called with the new text after each committed edit (not during an
    /// input-method composition).
    pub fn on_change(mut self, handler: impl Fn(&str) + 'static) -> Self {
        self.on_change.push(Rc::new(handler));
        self
    }

    /// Enter in a single-line field, or the soft keyboard's action key.
    pub fn on_submit(mut self, handler: impl Fn(TextInputAction) + 'static) -> Self {
        self.on_submit.push(Rc::new(handler));
        self
    }

    /// Called with the text when the field loses focus (for `lazy` models).
    pub fn on_commit(mut self, handler: impl Fn(&str) + 'static) -> Self {
        self.on_blur_commit.push(Rc::new(handler));
        self
    }

    pub fn handle(mut self, handle: TextEditHandle) -> Self {
        self.handle = Some(handle);
        self
    }
}

impl Styleable for TextEditBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

/// How a press extends the selection while dragging.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Granularity {
    Grapheme,
    Word,
    Paragraph,
}

struct Press {
    pointer: crate::input::pointer::PointerId,
    touch: bool,
    /// The range selected by the press itself (a word for a double click).
    anchor: Range<usize>,
    granularity: Granularity,
    moved: bool,
    start: Vector,
    time: f32,
    long_pressed: bool,
}

struct State {
    value: TextEditingValue,
    history: UndoHistory,
    connection: Option<text_input::TextInputConnection>,
    focused: bool,
    press: Option<Press>,
    clicks: u32,
    last_click: (f32, Vector),
    /// Horizontal position kept across vertical moves (cosmic-text units).
    preferred_x: Option<f32>,
    scroll: Vector,
    blink_epoch: f64,
    /// Display text the buffer was last shaped from.
    shaped: Option<(String, String)>,
    /// The value before the current composition started; undo returns here.
    pre_compose: Option<TextEditingValue>,
    /// A touch selected text (long press, double tap): show the platform's
    /// text toolbar over the selection until the next touch or edit.
    menu: TextMenu,
}

#[derive(Clone, Copy, Default, PartialEq)]
enum TextMenu {
    #[default]
    Hidden,
    Requested,
    /// Shown over this rectangle (logical window pixels).
    Shown(Rect),
}

/// Shared field state reachable from hooks, IME sinks, and handles.
struct Field {
    state: RefCell<State>,
    buffer: RefCell<Buffer>,
    config: TextInputConfig,
    multiline: bool,
    secure: bool,
    mask: char,
    formatters: Vec<SharedFormatter>,
    read_only: Prop<bool>,
    disabled: Prop<bool>,
    revision: Signal<u64>,
    node: NodeHandle,
    value_signal: Option<Signal<String>>,
    editing_signal: Option<Signal<TextEditingValue>>,
    on_change: Vec<TextHandler>,
    on_submit: Vec<ActionHandler>,
    on_blur_commit: Vec<TextHandler>,
    /// Set while writing bound signals, so their watchers ignore the echo.
    writing: Cell<bool>,
    content: Cell<Rect>,
}

impl Field {
    fn editable(&self) -> bool {
        !self.read_only.get_untracked() && !self.disabled.get_untracked()
    }

    /// Display text (masked for passwords) and a map back to real offsets.
    fn display(&self, value: &TextEditingValue) -> DisplayText {
        DisplayText::new(&value.text, self.secure.then_some(self.mask))
    }

    /// Apply a proposed value from a local edit or an input method.
    fn edit(self: &Rc<Self>, proposed: TextEditingValue, kind: EditKind) {
        if !self.editable() {
            return;
        }
        let old = self.state.borrow().value.clone();
        let next = text_input::apply(&self.formatters, &old, proposed.normalized());
        if next == old {
            // A rejected input-method edit must not stay in the mirror.
            self.sync_platform();
            return;
        }
        let time = miniquad::date::now();
        {
            let mut state = self.state.borrow_mut();
            if next.text != old.text {
                // A committed composition undoes to the text before it.
                let before = if old.is_composing() {
                    state
                        .pre_compose
                        .take()
                        .unwrap_or_else(|| old.finish_composing())
                } else {
                    old.clone()
                };
                state.history.record(&before, kind, time);
            }
            if !next.is_composing() {
                state.pre_compose = None;
            }
            state.value = next.clone();
            state.preferred_x = None;
            state.blink_epoch = time;
        }
        self.publish(&old);
    }

    /// Change only the selection (no history, no formatters).
    fn set_selection(self: &Rc<Self>, selection: TextSelection) {
        let old = self.state.borrow().value.clone();
        let next = TextEditingValue {
            selection,
            composing: None,
            text: old.text.clone(),
        }
        .normalized();
        if next == old {
            return;
        }
        {
            let mut state = self.state.borrow_mut();
            state.value = next;
            state.preferred_x = None;
            state.history.seal();
            state.blink_epoch = miniquad::date::now();
        }
        self.publish(&old);
    }

    /// Replace the value from outside (bound signal): no history entry.
    fn replace(self: &Rc<Self>, next: TextEditingValue) {
        let old = self.state.borrow().value.clone();
        if next == old {
            return;
        }
        {
            let mut state = self.state.borrow_mut();
            state.value = next.normalized();
            state.preferred_x = None;
        }
        self.publish_with(&old, false);
    }

    /// Redraw, sync the platform mirror, write bound signals, and notify.
    fn publish(self: &Rc<Self>, old: &TextEditingValue) {
        self.publish_with(old, true);
    }

    /// `notify`: run `on_change` (edits made by the field, not external writes).
    fn publish_with(self: &Rc<Self>, old: &TextEditingValue, notify: bool) {
        let value = self.state.borrow().value.clone();
        self.revision
            .update(|revision| *revision = revision.wrapping_add(1));
        self.sync_platform();
        self.writing.set(true);
        if let Some(signal) = self.editing_signal
            && signal.get_clone_untracked() != value
        {
            signal.set(value.clone());
        }
        let text_changed = value.text != old.text;
        if text_changed {
            self.hide_text_menu();
        }
        if text_changed
            && !value.is_composing()
            && let Some(signal) = self.value_signal
            && signal.get_clone_untracked() != value.text
        {
            signal.set(value.text.clone());
        }
        self.writing.set(false);
        if notify && text_changed && !value.is_composing() {
            for handler in &self.on_change {
                handler(&value.text);
            }
        }
    }

    fn request_text_menu(&self) {
        self.state.borrow_mut().menu = TextMenu::Requested;
        self.revision
            .update(|revision| *revision = revision.wrapping_add(1));
    }

    fn hide_text_menu(&self) {
        let mut state = self.state.borrow_mut();
        if std::mem::take(&mut state.menu) != TextMenu::Hidden
            && let Some(connection) = &state.connection
        {
            connection.hide_text_menu();
        }
    }

    fn sync_platform(&self) {
        let state = self.state.borrow();
        if let Some(connection) = &state.connection {
            connection.set_value(&state.value);
        }
    }

    fn undo(self: &Rc<Self>, redo: bool) {
        if !self.editable() {
            return;
        }
        let old = self.state.borrow().value.clone();
        let restored = {
            let mut state = self.state.borrow_mut();
            let current = state.value.clone();
            if redo {
                state.history.redo(&current)
            } else {
                state.history.undo(&current)
            }
        };
        if let Some(restored) = restored {
            self.state.borrow_mut().value = restored;
            self.publish(&old);
        }
    }

    fn submit(&self, action: TextInputAction) {
        let action = match action {
            TextInputAction::Unspecified => self.config.resolved_action(),
            action => action,
        };
        for handler in &self.on_submit {
            handler(action);
        }
    }

    /// Show a provisional composition, remembering the value before it.
    fn set_composing(self: &Rc<Self>, next: TextEditingValue) {
        let old = self.state.borrow().value.clone();
        {
            let mut state = self.state.borrow_mut();
            if !old.is_composing() && next.is_composing() {
                state.pre_compose = Some(old.clone());
            } else if !next.is_composing()
                && state.pre_compose.is_some()
                && next.text == old.finish_composing().text
            {
                state.pre_compose = None;
            }
            state.value = next;
            state.blink_epoch = miniquad::date::now();
        }
        self.publish(&old);
    }

    fn ime(self: &Rc<Self>, event: ImeEvent) {
        match event {
            ImeEvent::Preedit { text, cursor } => {
                if !self.editable() {
                    return;
                }
                // An empty preedit only cancels a composition. Without one it
                // must not replace the selection (Ctrl+A then a stray cancel
                // would clear the field).
                if text.is_empty() && !self.state.borrow().value.is_composing() {
                    return;
                }
                let next = self.state.borrow().value.compose(&text, cursor);
                // Compositions bypass history and length limits until committed.
                self.set_composing(next);
            }
            ImeEvent::Commit(text) => {
                // A committed phrase is its own undo step.
                let composing = self.state.borrow().value.is_composing();
                let next = self.state.borrow().value.insert(&text);
                self.edit(
                    next,
                    if composing {
                        EditKind::Other
                    } else {
                        EditKind::Typing
                    },
                );
            }
            ImeEvent::State(value) => {
                let old_len = self.state.borrow().value.text.len();
                let was_composing = self.state.borrow().value.is_composing();
                let kind = if was_composing {
                    EditKind::Other
                } else if value.text.len() < old_len {
                    EditKind::Deleting
                } else {
                    EditKind::Typing
                };
                if value.is_composing() {
                    if self.editable() {
                        self.set_composing(value);
                    }
                } else {
                    self.edit(value, kind);
                }
            }
            ImeEvent::Action(action) => self.submit(action),
        }
    }

    // ---- layout helpers -------------------------------------------------

    /// Offset into the display text → cosmic-text cursor.
    fn cursor_for(display: &DisplayText, offset: usize) -> Cursor {
        let offset = display.to_display(offset);
        let mut line = 0;
        let mut start = 0;
        for (index, ch) in display.text.char_indices() {
            if index >= offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                start = index + 1;
            }
        }
        Cursor::new(line, offset - start)
    }

    /// cosmic-text cursor → offset into the real text.
    fn offset_for(display: &DisplayText, cursor: Cursor) -> usize {
        let mut line_start = 0;
        let mut line = 0;
        if cursor.line > 0 {
            for (index, ch) in display.text.char_indices() {
                if ch == '\n' {
                    line += 1;
                    if line == cursor.line {
                        line_start = index + 1;
                        break;
                    }
                }
            }
            if line < cursor.line {
                return display.to_real(display.text.len());
            }
        }
        let line_end = display.text[line_start..]
            .find('\n')
            .map_or(display.text.len(), |end| line_start + end);
        let offset = (line_start + cursor.index).min(line_end);
        display.to_real(crate::input::text_input::floor(&display.text, offset))
    }

    /// The caret rectangle of `offset` relative to the text origin.
    fn caret_rect(buffer: &Buffer, display: &DisplayText, offset: usize, line_height: f32) -> Rect {
        let cursor = Self::cursor_for(display, offset);
        let mut fallback = None;
        for run in buffer.layout_runs() {
            if run.line_i != cursor.line {
                continue;
            }
            if fallback.is_none() {
                fallback = Some(Rect::new(0., run.line_top, 1., run.line_height));
            }
            let first = run.glyphs.first().map_or(0, |glyph| glyph.start);
            let last = run.glyphs.last().map_or(0, |glyph| glyph.end);
            if run.glyphs.is_empty() || (cursor.index >= first && cursor.index <= last) {
                let x = if run.glyphs.is_empty() {
                    0.
                } else if cursor.index >= last {
                    run.glyphs.last().map_or(0., |glyph| glyph.x + glyph.w)
                } else {
                    run.glyphs
                        .iter()
                        .find(|glyph| glyph.start >= cursor.index || glyph.end > cursor.index)
                        .map_or(0., |glyph| {
                            if glyph.start >= cursor.index {
                                glyph.x
                            } else {
                                // Inside a ligature or cluster: interpolate.
                                let span = (glyph.end - glyph.start).max(1) as f32;
                                glyph.x + glyph.w * (cursor.index - glyph.start) as f32 / span
                            }
                        })
                };
                // Prefer the run where the caret starts a line over the end of
                // the previous wrapped run.
                if cursor.index == last && last != first {
                    fallback = Some(Rect::new(x, run.line_top, 1., run.line_height));
                    continue;
                }
                return Rect::new(x, run.line_top, 1., run.line_height);
            }
        }
        fallback.unwrap_or(Rect::new(0., 0., 1., line_height))
    }
}

/// The text as displayed: a password shows one mask character per grapheme.
struct DisplayText {
    text: String,
    /// For masked text: real byte offset of each display grapheme boundary.
    map: Option<(Vec<usize>, usize)>,
    mask_len: usize,
}

impl DisplayText {
    fn new(text: &str, mask: Option<char>) -> Self {
        match mask {
            None => Self {
                text: text.to_string(),
                map: None,
                mask_len: 0,
            },
            Some(mask) => {
                let boundaries: Vec<usize> = text
                    .grapheme_indices(true)
                    .map(|(index, _)| index)
                    .collect();
                let mut display = String::new();
                for _ in &boundaries {
                    display.push(mask);
                }
                Self {
                    text: display,
                    map: Some((boundaries, text.len())),
                    mask_len: mask.len_utf8(),
                }
            }
        }
    }

    fn to_display(&self, real: usize) -> usize {
        match &self.map {
            None => real.min(self.text.len()),
            Some((boundaries, _)) => {
                let index = boundaries.partition_point(|&start| start < real);
                index * self.mask_len
            }
        }
    }

    fn to_real(&self, display: usize) -> usize {
        match &self.map {
            None => display,
            Some((boundaries, len)) => {
                let index = display / self.mask_len.max(1);
                boundaries.get(index).copied().unwrap_or(*len)
            }
        }
    }
}

impl BuildNode for TextEditBuilder {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            value,
            editing,
            placeholder,
            mut config,
            max_length,
            mut formatters,
            disabled,
            read_only,
            select_all_on_focus,
            min_lines,
            mask,
            caret_color,
            selection_color,
            placeholder_color,
            on_change,
            on_submit,
            on_blur_commit,
            handle,
            mut style,
        } = self;
        let multiline = config.is_multiline();
        if !multiline {
            formatters.insert(0, Rc::new(SingleLine));
        }
        if let Some(max) = max_length {
            formatters.push(Rc::new(MaxLength(max)));
        }
        config.read_only = read_only.get_untracked();
        let node = NodeHandle::new();
        style = style.node_handle(node);
        ctx.push_style(style);
        let defaults = Style::new()
            .focusable(disabled.map(|disabled| !disabled))
            .cursor(crate::input::cursor::Cursor::Text)
            .min_hit_size(44.);
        ctx.finish(defaults, move |style| {
            let initial = editing
                .map(|signal| signal.get_clone_untracked())
                .or_else(|| value.map(|value| TextEditingValue::new(value.get_clone_untracked())))
                .unwrap_or_default();
            let layout = use_layout(style);
            let (rect, border, padding) = (layout.rect, layout.border, layout.padding);
            let paint = use_paint(style);
            use_background(style, rect);
            let declared_font_size = style.declared_font_size();
            let font_size = crate::node::use_font_size();
            crate::node::NodeInner::with_mut(|node| {
                if node.font_size.is_none() {
                    node.font_size = declared_font_size;
                }
            });
            let line_height =
                crate::style::units::resolve_length(style.take_line_height(), "line_height");
            let line_height_multiplier = style.take_line_height_multiplier();
            let font_family = style.take_font_family();
            let font_weight = style.take_font_weight();
            let font_italic = style.take_font_italic();
            let letter_spacing =
                crate::style::units::resolve_length(style.take_letter_spacing(), "letter_spacing");
            let color = style
                .declared_color()
                .or_else(crate::inherit::inherited_foreground)
                .unwrap_or_else(|| style.take_color());
            let metrics = computed(move |_| {
                let size = font_size.get().max(1.);
                Metrics::new(
                    size,
                    line_height_multiplier
                        .get()
                        .map_or_else(|| line_height.get().max(size * 1.2), |factor| size * factor)
                        .max(1.),
                )
            });

            let field = Rc::new(Field {
                state: RefCell::new(State {
                    value: initial,
                    history: UndoHistory::default(),
                    connection: None,
                    focused: false,
                    press: None,
                    clicks: 0,
                    last_click: (f32::NEG_INFINITY, Vector::ZERO),
                    preferred_x: None,
                    scroll: Vector::ZERO,
                    blink_epoch: 0.,
                    shaped: None,
                    pre_compose: None,
                    menu: TextMenu::Hidden,
                }),
                buffer: RefCell::new(Runtime::with_fonts_mut(|fonts| {
                    Buffer::new(fonts.font_system(), metrics.get_untracked())
                })),
                config,
                multiline,
                secure: config.is_secure(),
                mask,
                formatters,
                read_only,
                disabled,
                revision: signal(0),
                node,
                value_signal: value,
                editing_signal: editing,
                on_change,
                on_submit,
                on_blur_commit,
                writing: Cell::new(false),
                content: Cell::new(Rect::default()),
            });
            if let Some(handle) = &handle {
                *handle.0.borrow_mut() = Rc::downgrade(&field);
            }

            // External writes to the bound signals.
            if let Some(value) = value {
                let field = field.clone();
                watch_effect(move || {
                    let text = value.get_clone();
                    if field.writing.get() {
                        return;
                    }
                    let current = field.state.borrow().value.text.clone();
                    if text != current {
                        field.replace(TextEditingValue::new(text));
                    }
                });
            }
            if let Some(editing) = editing {
                let field = field.clone();
                watch_effect(move || {
                    let next = editing.get_clone();
                    if !field.writing.get() {
                        field.replace(next);
                    }
                });
            }

            let placeholder_family = font_family.clone();
            // Reshape when the displayed text or any shaping attribute changes.
            let shape = {
                let field = field.clone();
                move |fonts: &mut FontState, width: Option<f32>| {
                    let mut buffer = field.buffer.borrow_mut();
                    let value = field.state.borrow().value.clone();
                    let display = field.display(&value);
                    let family = font_family.get_clone_untracked();
                    let weight = font_weight.get_untracked();
                    let italic = font_italic.get_untracked();
                    let spacing = letter_spacing.get_untracked();
                    let metrics = metrics.get_untracked();
                    let key = (
                        display.text.clone(),
                        format!(
                            "{family:?}/{}/{italic}/{}/{}/{}",
                            weight.0,
                            spacing.to_bits(),
                            metrics.font_size.to_bits(),
                            metrics.line_height.to_bits()
                        ),
                    );
                    let fs = fonts.font_system();
                    if field.state.borrow().shaped.as_ref() != Some(&key) {
                        let attrs = Attrs::new()
                            .family(family.as_family())
                            .weight(weight)
                            // cosmic-text takes letter spacing in em.
                            .letter_spacing(spacing / metrics.font_size.max(1.))
                            .style(if italic {
                                cosmic_text::Style::Italic
                            } else {
                                cosmic_text::Style::Normal
                            });
                        buffer.set_wrap(
                            fs,
                            if field.multiline {
                                Wrap::WordOrGlyph
                            } else {
                                Wrap::None
                            },
                        );
                        buffer.set_text(fs, &display.text, &attrs, Shaping::Advanced, None);
                        field.state.borrow_mut().shaped = Some(key);
                    }
                    buffer.set_metrics_and_size(
                        fs,
                        metrics,
                        if field.multiline { width } else { None },
                        None,
                    );
                    buffer.shape_until_scroll(fs, false);
                }
            };

            layout.set_measure_fn({
                let field = field.clone();
                let shape = shape.clone();
                move |known: Size<Option<f32>>, available: Size<AvailableSpace>| -> Size<f32> {
                    if let Size {
                        width: Some(width),
                        height: Some(height),
                    } = known
                    {
                        return Size { width, height };
                    }
                    let width = known.width.or(match available.width {
                        AvailableSpace::Definite(width) => Some(width),
                        AvailableSpace::MaxContent => None,
                        AvailableSpace::MinContent => Some(0.),
                    });
                    Runtime::with_fonts_mut(|fonts| shape(fonts, width));
                    let buffer = field.buffer.borrow();
                    let line = metrics.get_untracked().line_height;
                    let mut text_width: f32 = 0.;
                    let mut height: f32 = 0.;
                    for run in buffer.layout_runs() {
                        text_width = text_width.max(run.line_w.ceil());
                        height = run.line_top + run.line_height;
                    }
                    let height = height.max(
                        line * if field.multiline {
                            min_lines as f32
                        } else {
                            1.
                        },
                    );
                    Size {
                        width: known.width.unwrap_or(if field.multiline {
                            width.unwrap_or(text_width)
                        } else {
                            // Single-line fields do not grow with their text.
                            (font_size.get_untracked() * 10.).max(text_width.min(240.))
                        }),
                        height: known.height.unwrap_or(height.ceil()),
                    }
                }
            });
            {
                let revision = field.revision;
                watch_effect(move || {
                    let _ = revision.get();
                    layout.mark_dirty();
                });
            }

            // Focus: attach to and detach from the platform input method.
            let focus = crate::input::focus::use_focus();
            {
                let field = field.clone();
                watch_effect(move || {
                    let focused = focus.focused.get();
                    mue_core::effect::untracked(|| {
                        focus_changed(&field, focused, select_all_on_focus)
                    });
                });
            }

            // Keys.
            style.on_key_down.append({
                let field = field.clone();
                move |event: &KeyEvent| key_down(&field, event)
            });
            style.on_char.append({
                let field = field.clone();
                move |event: &CharEvent| {
                    let ch = event.character;
                    let mods = event.modifiers;
                    // Shortcuts produce control characters or arrive with
                    // Ctrl/Cmd; AltGr (Ctrl+Alt on Windows) still types.
                    if ch.is_control() || (mods.ctrl && !mods.alt) || mods.logo {
                        return;
                    }
                    if !field.state.borrow().focused {
                        return;
                    }
                    let next = field.state.borrow().value.insert(&ch.to_string());
                    field.edit(next, EditKind::Typing);
                    event.stop_propagation();
                }
            });

            // Pointer.
            style.on_pointer_event.append({
                let field = field.clone();
                move |event: &PointerEvent| pointer(&field, event)
            });

            // Paint.
            let caret_color = caret_color.unwrap_or_else(|| color.clone());
            let placeholder_color = placeholder_color.unwrap_or_else(|| {
                let color = color.clone();
                computed(move |_| color.get_clone().with_opacity(0.45)).into()
            });
            let placeholder_buffer = Rc::new(RefCell::new(Runtime::with_fonts_mut(|fonts| {
                Buffer::new(fonts.font_system(), metrics.get_untracked())
            })));
            let blink_tick = signal(0u64);
            style.on_update.append({
                let field = field.clone();
                move |_| {
                    // Keep frames coming while the caret blinks.
                    if field.state.borrow().focused {
                        let phase = ((miniquad::date::now() - field.state.borrow().blink_epoch)
                            / BLINK) as u64;
                        if blink_tick.get_untracked() != phase {
                            blink_tick.set(phase);
                        }
                        long_press_tick(&field);
                    }
                }
            });
            style.on_render.append({
                let field = field.clone();
                move |_| {
                    let _ = blink_tick.get_untracked();
                    let outer = rect.get();
                    let content = outer.deflate(border.get()).deflate(padding.get());
                    field.content.set(content);
                    if content.w <= 0. || content.h <= 0. {
                        return;
                    }
                    Runtime::with_fonts_mut(|fonts| {
                        shape(fonts, Some(content.w));
                        let line = metrics.get_untracked().line_height;
                        let value = field.state.borrow().value.clone();
                        let display = field.display(&value);
                        let buffer = field.buffer.borrow();
                        let caret =
                            Field::caret_rect(&buffer, &display, value.selection.extent, line);
                        // Scroll to keep the caret inside the content box.
                        let mut scroll = field.state.borrow().scroll;
                        let focused = field.state.borrow().focused;
                        if field.multiline {
                            let mut text_h: f32 = line;
                            for run in buffer.layout_runs() {
                                text_h = run.line_top + run.line_height;
                            }
                            scroll.x = 0.;
                            if focused {
                                if caret.y - scroll.y < 0. {
                                    scroll.y = caret.y;
                                }
                                if caret.bottom() - scroll.y > content.h {
                                    scroll.y = caret.bottom() - content.h;
                                }
                            }
                            scroll.y = scroll.y.clamp(0., (text_h - content.h).max(0.));
                        } else {
                            let mut text_w: f32 = 0.;
                            for run in buffer.layout_runs() {
                                text_w = text_w.max(run.line_w);
                            }
                            scroll.y = 0.;
                            let caret_w = 2.;
                            if focused {
                                if caret.x - scroll.x < 0. {
                                    scroll.x = caret.x;
                                }
                                if caret.x + caret_w - scroll.x > content.w {
                                    scroll.x = caret.x + caret_w - content.w;
                                }
                            }
                            scroll.x = scroll.x.clamp(0., (text_w + caret_w - content.w).max(0.));
                        }
                        field.state.borrow_mut().scroll = scroll;
                        // Vertically center a single line in a taller box.
                        let first_h = buffer
                            .layout_runs()
                            .next()
                            .map_or(line, |run| run.line_height);
                        let offset_y = if field.multiline {
                            0.
                        } else {
                            ((content.h - first_h) / 2.).max(0.)
                        };
                        let origin = vec2(content.x - scroll.x, content.y - scroll.y + offset_y);

                        // Report the caret (or composition start) for
                        // candidate windows and soft keyboards.
                        if focused && let Some(connection) = &field.state.borrow().connection {
                            let anchor = value
                                .composing
                                .as_ref()
                                .map_or(value.selection.extent, |range| range.start);
                            let rect = Field::caret_rect(&buffer, &display, anchor, line);
                            let local = Rect::new(origin.x + rect.x, origin.y + rect.y, 2., rect.h);
                            connection.set_caret_rect(crate::paint::transform_rect(
                                current_transform(),
                                local,
                            ));
                        }

                        // The text toolbar sits over the selection's bounds.
                        let menu = field.state.borrow().menu;
                        if focused && menu != TextMenu::Hidden {
                            let start = value.selection.start();
                            let end = value.selection.end();
                            let a = Field::caret_rect(&buffer, &display, start, line);
                            let b = Field::caret_rect(&buffer, &display, end, line);
                            let (left, right) = if a.y == b.y {
                                (a.x.min(b.x), a.x.max(b.x))
                            } else {
                                (0., content.w + scroll.x)
                            };
                            let local = Rect::new(
                                origin.x + left,
                                origin.y + a.y.min(b.y),
                                (right - left).max(1.),
                                (b.y + b.h).max(a.y + a.h) - a.y.min(b.y),
                            );
                            let rect = crate::paint::transform_rect(current_transform(), local);
                            if menu != TextMenu::Shown(rect) {
                                let mut state = field.state.borrow_mut();
                                state.menu = TextMenu::Shown(rect);
                                if let Some(connection) = &state.connection {
                                    connection.show_text_menu(rect);
                                }
                            }
                        }

                        let selection_brush = selection_color.get_clone();
                        let text_brush = color.get_clone();
                        let caret_brush = caret_color.get_clone();
                        let empty = value.text.is_empty();
                        let placeholder_text = placeholder.get_clone();
                        if empty && !placeholder_text.is_empty() {
                            let mut placeholder_buffer = placeholder_buffer.borrow_mut();
                            let fs = fonts.font_system();
                            let family = placeholder_family.get_clone_untracked();
                            let attrs = Attrs::new().family(family.as_family());
                            placeholder_buffer.set_metrics_and_size(
                                fs,
                                metrics.get_untracked(),
                                if field.multiline {
                                    Some(content.w)
                                } else {
                                    None
                                },
                                None,
                            );
                            placeholder_buffer.set_text(
                                fs,
                                &placeholder_text,
                                &attrs,
                                Shaping::Advanced,
                                None,
                            );
                            placeholder_buffer.shape_until_scroll(fs, false);
                        }
                        let blink_on = (((miniquad::date::now() - field.state.borrow().blink_epoch)
                            / BLINK) as u64)
                            .is_multiple_of(2);
                        // Clip only text that can leave the box: a scrolled
                        // field, or text (plus the caret) wider or taller than
                        // the content. Short fields then batch with the frame.
                        let overflows = scroll != Vector::ZERO || {
                            let (width, height) = buffer.layout_runs().fold((0_f32, 0_f32), |(w, h), run| {
                                (w.max(run.line_w), h.max(run.line_top + run.line_height))
                            });
                            let width = width.max(if empty { placeholder_buffer.borrow().layout_runs().map(|run| run.line_w).fold(0., f32::max) } else { 0. });
                            origin.x < content.x - 0.5
                                || origin.y < content.y - 0.5
                                || origin.x + width + 2. > content.right() + 0.5
                                || origin.y + height > content.bottom() + 0.5
                        };
                        crate::paint::with_clip_if(overflows, content, || {
                            paint.draw(|p| {
                                // Selection.
                                if focused && !value.selection.is_collapsed() {
                                    let start =
                                        Field::cursor_for(&display, value.selection.start());
                                    let end = Field::cursor_for(&display, value.selection.end());
                                    for run in buffer.layout_runs() {
                                        if run.line_i < start.line || run.line_i > end.line {
                                            continue;
                                        }
                                        let line_start = if run.line_i == start.line {
                                            start
                                        } else {
                                            Cursor::new(run.line_i, 0)
                                        };
                                        let line_end = if run.line_i == end.line {
                                            end
                                        } else {
                                            Cursor::new(run.line_i, usize::MAX)
                                        };
                                        if let Some((x, w)) = run.highlight(line_start, line_end) {
                                            // A selected line break shows as a sliver.
                                            let w = if run.line_i < end.line {
                                                w + font_size.get_untracked() * 0.3
                                            } else {
                                                w
                                            };
                                            if w > 0. {
                                                p.fill(
                                                    &PathData::rect(Rect::new(
                                                        origin.x + x,
                                                        origin.y + run.line_top,
                                                        w,
                                                        run.line_height,
                                                    )),
                                                    FillRule::NonZero,
                                                    selection_brush.shader(content),
                                                );
                                            }
                                        }
                                    }
                                }
                                // Text or placeholder.
                                if empty {
                                    let placeholder_brush = placeholder_color.get_clone();
                                    let placeholder_buffer = placeholder_buffer.borrow();
                                    for run in placeholder_buffer.layout_runs() {
                                        let pen =
                                            vec2(origin.x.round(), (origin.y + run.line_y).round());
                                        super::text::draw_run_glyphs(
                                            p,
                                            fonts,
                                            &run,
                                            pen,
                                            &placeholder_brush,
                                            content,
                                        );
                                    }
                                } else {
                                    for run in buffer.layout_runs() {
                                        let pen =
                                            vec2(origin.x.round(), (origin.y + run.line_y).round());
                                        super::text::draw_run_glyphs(
                                            p,
                                            fonts,
                                            &run,
                                            pen,
                                            &text_brush,
                                            content,
                                        );
                                    }
                                }
                                // Composition underline.
                                if let Some(range) = &value.composing {
                                    let start = Field::cursor_for(&display, range.start);
                                    let end = Field::cursor_for(&display, range.end);
                                    for run in buffer.layout_runs() {
                                        if run.line_i < start.line || run.line_i > end.line {
                                            continue;
                                        }
                                        if let Some((x, w)) = run.highlight(start, end) {
                                            let thickness =
                                                (font_size.get_untracked() / 14.).max(1.);
                                            p.fill(
                                                &PathData::rect(Rect::new(
                                                    origin.x + x,
                                                    (origin.y + run.line_y).round() + 2.,
                                                    w,
                                                    thickness,
                                                )),
                                                FillRule::NonZero,
                                                text_brush.shader(content),
                                            );
                                        }
                                    }
                                }
                                // Caret.
                                if focused && blink_on && field.editable() {
                                    let width = (font_size.get_untracked() / 12.).max(1.5);
                                    p.fill(
                                        &PathData::rect(Rect::new(
                                            (origin.x + caret.x).round(),
                                            origin.y + caret.y,
                                            width,
                                            caret.h,
                                        )),
                                        FillRule::NonZero,
                                        caret_brush.shader(content),
                                    );
                                }
                                super::text::end_glyph_pass(fonts);
                            });
                        });
                    });
                }
            });
        })
    }
}

fn focus_changed(field: &Rc<Field>, focused: bool, select_all: bool) {
    let was = field.state.borrow().focused;
    if was == focused {
        return;
    }
    if focused {
        let value = field.state.borrow().value.clone();
        let sink = {
            let weak = Rc::downgrade(field);
            move |event: ImeEvent| {
                if let Some(field) = weak.upgrade() {
                    field.ime(event);
                }
            }
        };
        let mut config = field.config;
        config.read_only = !field.editable();
        let connection = text_input::attach(&config, &value, sink);
        {
            let mut state = field.state.borrow_mut();
            state.focused = true;
            state.connection = Some(connection);
            state.blink_epoch = miniquad::date::now();
        }
        if select_all {
            let selection = value.select_all().selection;
            field.set_selection(selection);
        } else {
            field
                .revision
                .update(|revision| *revision = revision.wrapping_add(1));
        }
    } else {
        // Keep composed text, as native fields do when focus leaves.
        let old = field.state.borrow().value.clone();
        {
            let mut state = field.state.borrow_mut();
            state.focused = false;
            state.press = None;
            state.menu = TextMenu::Hidden;
            state.connection = None;
            state.value = state.value.finish_composing();
            state.history.seal();
        }
        field.publish(&old);
        let text = field.state.borrow().value.text.clone();
        for handler in &field.on_blur_commit {
            handler(&text);
        }
        field
            .revision
            .update(|revision| *revision = revision.wrapping_add(1));
    }
}

fn key_down(field: &Rc<Field>, event: &KeyEvent) {
    if !field.state.borrow().focused {
        return;
    }
    let mods = event.modifiers;
    let command = event.command();
    let shift = mods.shift;
    // Word movement: Ctrl on Windows/Linux, Option on macOS.
    let word = if cfg!(target_os = "macos") {
        mods.alt
    } else {
        mods.ctrl
    };
    let value = field.state.borrow().value.clone();
    let editable = field.editable();
    let mut handled = true;
    match event.key {
        KeyCode::Left | KeyCode::Right => {
            let forward = event.key == KeyCode::Right;
            let next = if cfg!(target_os = "macos") && mods.logo {
                visual_line_edge(field, &value, forward, shift)
            } else {
                value.moved(
                    forward,
                    if word {
                        TextUnit::Word
                    } else {
                        TextUnit::Grapheme
                    },
                    shift,
                )
            };
            field.set_selection(next.selection);
        }
        KeyCode::Up | KeyCode::Down
            if field.multiline && !(cfg!(target_os = "macos") && mods.logo) =>
        {
            vertical(
                field,
                &value,
                if event.key == KeyCode::Up { -1 } else { 1 },
                shift,
            );
        }
        KeyCode::Up | KeyCode::Down => {
            // Single line (or Cmd+Up/Down): to the start or end of the text.
            let forward = event.key == KeyCode::Down;
            let next = value.moved(forward, TextUnit::Document, shift);
            field.set_selection(next.selection);
        }
        KeyCode::Home | KeyCode::End => {
            let forward = event.key == KeyCode::End;
            let next = if command || !field.multiline {
                value.moved(forward, TextUnit::Document, shift)
            } else {
                visual_line_edge(field, &value, forward, shift)
            };
            field.set_selection(next.selection);
        }
        KeyCode::PageUp | KeyCode::PageDown if field.multiline => {
            // A page is the visible height of the field.
            let line = field.buffer.borrow().metrics().line_height.max(1.);
            let page = ((field.content.get().h / line).floor() as i32).max(1);
            vertical(
                field,
                &value,
                if event.key == KeyCode::PageUp {
                    -page
                } else {
                    page
                },
                shift,
            );
        }
        KeyCode::Backspace if editable => {
            let next = if cfg!(target_os = "macos") && mods.logo {
                let start = text_input::previous_boundary(
                    &value.text,
                    value.selection.extent,
                    TextUnit::Paragraph,
                );
                if value.selection.is_collapsed() {
                    value.replaced(start..value.selection.extent, "")
                } else {
                    value.delete_backward(TextUnit::Grapheme)
                }
            } else {
                value.delete_backward(if word {
                    TextUnit::Word
                } else {
                    TextUnit::Grapheme
                })
            };
            field.edit(next, EditKind::Deleting);
        }
        KeyCode::Delete if editable => {
            let next = value.delete_forward(if word {
                TextUnit::Word
            } else {
                TextUnit::Grapheme
            });
            field.edit(next, EditKind::Deleting);
        }
        KeyCode::Enter | KeyCode::KpEnter => {
            if field.multiline && editable && !command {
                let next = value.insert("\n");
                field.edit(next, EditKind::Typing);
            } else {
                field.submit(field.config.action);
            }
        }
        KeyCode::A if command => {
            field.set_selection(value.select_all().selection);
        }
        KeyCode::C if command => {
            if !value.selection.is_collapsed() && !field.secure {
                miniquad::window::clipboard_set(value.selected_text());
            }
        }
        KeyCode::X if command => {
            if !value.selection.is_collapsed() && !field.secure && editable {
                miniquad::window::clipboard_set(value.selected_text());
                field.edit(value.replaced(value.selection.range(), ""), EditKind::Other);
            }
        }
        KeyCode::V if command && editable => {
            if let Some(text) = miniquad::window::clipboard_get() {
                let text = text.replace("\r\n", "\n").replace('\r', "\n");
                field.edit(value.insert(&text), EditKind::Other);
            }
        }
        KeyCode::Z if command && editable => field.undo(shift),
        KeyCode::Y if command && editable && !cfg!(target_os = "macos") => field.undo(true),
        // Typing keys produce char events; let them through.
        _ => handled = false,
    }
    if handled {
        event.stop_propagation();
    }
}

/// Move to the start or end of the visual line holding the caret.
fn visual_line_edge(
    field: &Field,
    value: &TextEditingValue,
    forward: bool,
    extend: bool,
) -> TextEditingValue {
    let display = field.display(value);
    let cursor = Field::cursor_for(&display, value.selection.extent);
    let mut buffer = field.buffer.borrow_mut();
    let moved = Runtime::with_fonts_mut(|fonts| {
        buffer.cursor_motion(
            fonts.font_system(),
            cursor,
            None,
            if forward { Motion::End } else { Motion::Home },
        )
    });
    let offset = moved.map_or(value.selection.extent, |(cursor, _)| {
        Field::offset_for(&display, cursor)
    });
    value.with_extent(offset, extend)
}

/// Up, Down, PageUp, PageDown through the layout, keeping the column.
fn vertical(field: &Rc<Field>, value: &TextEditingValue, lines: i32, extend: bool) {
    let display = field.display(value);
    let line_height = field.buffer.borrow().metrics().line_height;
    let target = {
        let buffer = field.buffer.borrow();
        let caret = Field::caret_rect(&buffer, &display, value.selection.extent, line_height);
        let x = field.state.borrow().preferred_x.unwrap_or(caret.x);
        // Visual lines in order; find the caret's and step from it.
        let tops: Vec<(f32, f32)> = buffer
            .layout_runs()
            .map(|run| (run.line_top, run.line_height))
            .collect();
        let current = tops
            .iter()
            .position(|(top, height)| caret.y + caret.h / 2. < top + height)
            .unwrap_or(tops.len().saturating_sub(1)) as i32;
        let index = current + lines;
        if index < 0 {
            // Up from the first line goes to the start, as native fields do.
            (0, x)
        } else if index as usize >= tops.len() {
            (value.text.len(), x)
        } else {
            let (top, height) = tops[index as usize];
            let offset = buffer
                .hit(x.max(0.), top + height / 2.)
                .map_or(value.selection.extent, |cursor| {
                    Field::offset_for(&display, cursor)
                });
            (offset, x)
        }
    };
    let (offset, x) = target;
    field.set_selection(value.with_extent(offset, extend).selection);
    // Keep the column across consecutive vertical moves.
    field.state.borrow_mut().preferred_x = Some(x);
}

/// The text offset under a window point.
fn hit(field: &Field, position: Vector) -> usize {
    let state = field.state.borrow();
    let value = state.value.clone();
    let scroll = state.scroll;
    drop(state);
    let content = field.content.get();
    let display = field.display(&value);
    let buffer = field.buffer.borrow();
    let line = buffer.metrics().line_height;
    let first_h = buffer
        .layout_runs()
        .next()
        .map_or(line, |run| run.line_height);
    let offset_y = if field.multiline {
        0.
    } else {
        ((content.h - first_h) / 2.).max(0.)
    };
    let x = position.x - content.x + scroll.x;
    let y = (position.y - content.y + scroll.y - offset_y).max(0.);
    match buffer.hit(x, y) {
        Some(cursor) => Field::offset_for(&display, cursor),
        None => {
            if y <= 0. {
                0
            } else {
                value.text.len()
            }
        }
    }
}

fn select_granular(
    text: &str,
    offset: usize,
    granularity: Granularity,
    multiline: bool,
) -> Range<usize> {
    match granularity {
        Granularity::Grapheme => offset..offset,
        Granularity::Word => text_input::word_at(text, offset),
        Granularity::Paragraph if multiline => text_input::paragraph_at(text, offset),
        Granularity::Paragraph => 0..text.len(),
    }
}

fn pointer(field: &Rc<Field>, event: &PointerEvent) {
    let touch = event.pointer_type() == PointerType::Touch;
    match event.action() {
        PointerAction::Down => {
            if !touch && !event.button_state().contains(ButtonState::PRIMARY) {
                return;
            }
            let position = event.position();
            let time = event.time();
            let (clicks, shift) = {
                let mut state = field.state.borrow_mut();
                let (last_time, last_pos) = state.last_click;
                state.clicks =
                    if time - last_time < MULTI_CLICK && (position - last_pos).length() < 8. {
                        state.clicks % 3 + 1
                    } else {
                        1
                    };
                state.last_click = (time, position);
                (
                    state.clicks,
                    macroquad::input::is_key_down(KeyCode::LeftShift)
                        || macroquad::input::is_key_down(KeyCode::RightShift),
                )
            };
            let offset = hit(field, position);
            let granularity = match clicks {
                2 => Granularity::Word,
                3 => Granularity::Paragraph,
                _ => Granularity::Grapheme,
            };
            let text = field.state.borrow().value.text.clone();
            let anchor = select_granular(&text, offset, granularity, field.multiline);
            field.hide_text_menu();
            field.state.borrow_mut().press = Some(Press {
                pointer: event.pointer_id(),
                touch,
                anchor: anchor.clone(),
                granularity,
                moved: false,
                start: position,
                time,
                long_pressed: false,
            });
            if touch {
                // Touches act on release, so a drag can scroll an ancestor.
                return;
            }
            let selection = if shift && granularity == Granularity::Grapheme {
                TextSelection::new(field.state.borrow().value.selection.base, offset)
            } else {
                TextSelection::new(anchor.start, anchor.end)
            };
            field.set_selection(selection);
        }
        PointerAction::Move => {
            let Some((anchor, granularity, touch, start)) =
                field.state.borrow().press.as_ref().and_then(|press| {
                    (press.pointer == event.pointer_id()).then(|| {
                        (
                            press.anchor.clone(),
                            press.granularity,
                            press.touch,
                            press.start,
                        )
                    })
                })
            else {
                return;
            };
            if touch {
                let long = field
                    .state
                    .borrow()
                    .press
                    .as_ref()
                    .is_some_and(|press| press.long_pressed);
                if (event.position() - start).length() > TAP_SLOP
                    && let Some(press) = field.state.borrow_mut().press.as_mut()
                {
                    press.moved = true;
                }
                // After a long press, dragging extends the word selection.
                if !long {
                    return;
                }
            }
            let offset = hit(field, event.position());
            let text = field.state.borrow().value.text.clone();
            let here = select_granular(&text, offset, granularity, field.multiline);
            let selection = if here.start < anchor.start {
                TextSelection::new(anchor.end, here.start)
            } else {
                TextSelection::new(anchor.start, here.end.max(anchor.end))
            };
            field.set_selection(selection);
        }
        PointerAction::Up => {
            let press = field.state.borrow_mut().press.take();
            if let Some(press) = press
                && press.touch
                && !press.moved
                && !press.long_pressed
            {
                let offset = hit(field, event.position());
                let text = field.state.borrow().value.text.clone();
                let range = select_granular(&text, offset, press.granularity, field.multiline);
                field.set_selection(TextSelection::new(range.start, range.end));
                if press.granularity != Granularity::Grapheme {
                    field.request_text_menu();
                }
            }
        }
        PointerAction::Cancel => {
            field.state.borrow_mut().press = None;
        }
    }
}

/// Select the word under a held, unmoved touch.
fn long_press_tick(field: &Rc<Field>) {
    let target = {
        let mut state = field.state.borrow_mut();
        let now = crate::app::runtime::get_time();
        match state.press.as_mut() {
            Some(press)
                if press.touch
                    && !press.moved
                    && !press.long_pressed
                    && now - press.time >= LONG_PRESS =>
            {
                press.long_pressed = true;
                press.granularity = Granularity::Word;
                Some(press.start)
            }
            _ => None,
        }
    };
    if let Some(position) = target {
        let offset = hit(field, position);
        let text = field.state.borrow().value.text.clone();
        let range = text_input::word_at(&text, offset);
        if let Some(press) = field.state.borrow_mut().press.as_mut() {
            press.anchor = range.clone();
        }
        field.set_selection(TextSelection::new(range.start, range.end));
        field.request_text_menu();
    }
}
