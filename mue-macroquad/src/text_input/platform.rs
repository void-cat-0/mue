//! The connection between the focused text field and the platform's input
//! method: configuration, editing-state sync, caret rectangle, and events.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use mue_core::signal::{Access, ReadSignal, Signal};

use super::value::{TextEditingValue, TextSelection, byte_to_utf16, utf16_to_byte};
use crate::math::Rect;

/// What the field edits; selects the soft keyboard layout and input-method
/// behavior (Android `inputType`, iOS `keyboardType`, HTML `inputmode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextInputKind {
    #[default]
    Text,
    /// Masked text: no suggestions, no learning, no IME composition.
    Password,
    /// Integers.
    Number,
    /// Numbers with a decimal point (and sign when the formatter allows it).
    Decimal,
    Phone,
    Email,
    Url,
    Search,
    /// Several lines; Enter inserts a newline.
    Multiline,
}

/// The soft keyboard's action key and what Enter does in a single-line field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextInputAction {
    /// Platform default (Done for single-line, newline for multiline).
    #[default]
    Unspecified,
    Done,
    Go,
    Next,
    Previous,
    Search,
    Send,
    Newline,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextCapitalization {
    #[default]
    None,
    Characters,
    Words,
    Sentences,
}

/// Input-method configuration of the attached field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextInputConfig {
    pub kind: TextInputKind,
    pub action: TextInputAction,
    pub autocorrect: bool,
    pub capitalization: TextCapitalization,
    /// Editing is disabled; the keyboard stays hidden but selection works.
    pub read_only: bool,
}

impl Default for TextInputConfig {
    fn default() -> Self {
        Self {
            kind: TextInputKind::Text,
            action: TextInputAction::Unspecified,
            autocorrect: true,
            capitalization: TextCapitalization::None,
            read_only: false,
        }
    }
}

impl TextInputConfig {
    pub fn is_multiline(&self) -> bool {
        self.kind == TextInputKind::Multiline
    }

    pub fn is_secure(&self) -> bool {
        self.kind == TextInputKind::Password
    }

    /// The action key, resolving `Unspecified` to Done or Newline.
    pub fn resolved_action(&self) -> TextInputAction {
        match self.action {
            TextInputAction::Unspecified if self.is_multiline() => TextInputAction::Newline,
            TextInputAction::Unspecified => TextInputAction::Done,
            action => action,
        }
    }
}

/// An input-method event for the attached field.
#[derive(Clone, Debug, PartialEq)]
pub enum ImeEvent {
    /// Provisional composition text and the caret inside it (bytes). Empty
    /// text cancels the composition.
    Preedit { text: String, cursor: Option<usize> },
    /// Final text that replaces the composition (or the selection).
    Commit(String),
    /// The whole editing state from a platform that mirrors it (Android,
    /// iOS, web). Offsets are already UTF-8 bytes.
    State(TextEditingValue),
    /// The keyboard's action key, or Enter in a single-line field.
    Action(TextInputAction),
}

/// Platform operations. The default implementation forwards to miniquad;
/// tests install a recording backend.
pub(crate) trait Backend {
    fn start(&self, config: &TextInputConfig);
    fn stop(&self);
    fn set_state(&self, value: &TextEditingValue);
    /// Caret rectangle in logical window pixels.
    fn set_caret_rect(&self, rect: Rect);
}

/// Forwards to miniquad once a host window exists (an [`App`](crate::App));
/// headless trees (tests, tools) have no input method.
struct Miniquad;

thread_local! {
    static HOST: Cell<bool> = const { Cell::new(false) };
}

/// Mark the miniquad host as running; called when an `App` starts.
pub(crate) fn set_host_available() {
    HOST.with(|host| host.set(true));
}

fn host() -> bool {
    HOST.with(Cell::get)
}

impl Backend for Miniquad {
    fn start(&self, config: &TextInputConfig) {
        if host() {
            miniquad::window::start_text_input(to_miniquad(config));
        }
    }

    fn stop(&self) {
        if host() {
            miniquad::window::stop_text_input();
        }
    }

    fn set_state(&self, value: &TextEditingValue) {
        if !host() {
            return;
        }
        let text = value.text.as_str();
        let utf16 = |offset| byte_to_utf16(text, offset) as i32;
        let (composing_start, composing_end) = value
            .composing
            .as_ref()
            .map_or((-1, -1), |range| (utf16(range.start), utf16(range.end)));
        miniquad::window::set_text_input_state(&miniquad::TextInputState {
            text: value.text.clone(),
            selection_start: utf16(value.selection.base),
            selection_end: utf16(value.selection.extent),
            composing_start,
            composing_end,
        });
    }

    fn set_caret_rect(&self, rect: Rect) {
        if !host() {
            return;
        }
        let dpi = miniquad::window::dpi_scale();
        miniquad::window::set_text_input_rect(
            rect.x * dpi,
            rect.y * dpi,
            rect.w.max(1.) * dpi,
            rect.h.max(1.) * dpi,
        );
    }
}

pub(crate) fn from_miniquad_action(action: miniquad::TextInputAction) -> TextInputAction {
    use miniquad::TextInputAction as A;
    match action {
        A::Unspecified => TextInputAction::Unspecified,
        A::Done => TextInputAction::Done,
        A::Go => TextInputAction::Go,
        A::Next => TextInputAction::Next,
        A::Previous => TextInputAction::Previous,
        A::Search => TextInputAction::Search,
        A::Send => TextInputAction::Send,
        A::Newline => TextInputAction::Newline,
    }
}

fn to_miniquad(config: &TextInputConfig) -> miniquad::TextInputConfig {
    use miniquad::{TextInputAction as A, TextInputCapitalization as C, TextInputKind as K};
    miniquad::TextInputConfig {
        kind: match config.kind {
            TextInputKind::Text => K::Text,
            TextInputKind::Password => K::Password,
            TextInputKind::Number => K::Number,
            TextInputKind::Decimal => K::Decimal,
            TextInputKind::Phone => K::Phone,
            TextInputKind::Email => K::Email,
            TextInputKind::Url => K::Url,
            TextInputKind::Search => K::Search,
            TextInputKind::Multiline => K::Multiline,
        },
        action: match config.action {
            TextInputAction::Unspecified => A::Unspecified,
            TextInputAction::Done => A::Done,
            TextInputAction::Go => A::Go,
            TextInputAction::Next => A::Next,
            TextInputAction::Previous => A::Previous,
            TextInputAction::Search => A::Search,
            TextInputAction::Send => A::Send,
            TextInputAction::Newline => A::Newline,
        },
        autocorrect: config.autocorrect,
        capitalization: match config.capitalization {
            TextCapitalization::None => C::None,
            TextCapitalization::Characters => C::Characters,
            TextCapitalization::Words => C::Words,
            TextCapitalization::Sentences => C::Sentences,
        },
        read_only: config.read_only,
    }
}

type Sink = Rc<dyn Fn(ImeEvent)>;
type ActionHandler = Rc<dyn Fn(TextInputAction)>;
/// The attached field's connection id and event sink.
type Client = (u64, Sink);

pub(crate) struct Platform {
    backend: Box<dyn Backend>,
    next_id: Cell<u64>,
    /// The attached client and where its events go.
    client: RefCell<Option<Client>>,
    /// Last state pushed to or received from the platform, to avoid echoes.
    mirrored: RefCell<Option<TextEditingValue>>,
    caret: Signal<Option<Rect>>,
    keyboard: Signal<Rect>,
    actions: RefCell<Vec<ActionHandler>>,
}

thread_local! {
    static PLATFORM: RefCell<Option<Rc<Platform>>> = const { RefCell::new(None) };
}

pub(crate) fn platform() -> Rc<Platform> {
    PLATFORM.with(|cell| {
        cell.borrow_mut()
            .get_or_insert_with(|| {
                Rc::new(Platform {
                    backend: Box::new(Miniquad),
                    next_id: Cell::new(1),
                    client: RefCell::new(None),
                    mirrored: RefCell::new(None),
                    caret: mue_core::scope::Scope::null().run(|| Signal::new(None)),
                    keyboard: mue_core::scope::Scope::null().run(|| Signal::new(Rect::default())),
                    actions: RefCell::new(Vec::new()),
                })
            })
            .clone()
    })
}

#[cfg(test)]
pub(crate) fn install_backend(backend: Box<dyn Backend>) {
    PLATFORM.with(|cell| {
        *cell.borrow_mut() = Some(Rc::new(Platform {
            backend,
            next_id: Cell::new(1),
            client: RefCell::new(None),
            mirrored: RefCell::new(None),
            caret: mue_core::scope::Scope::null().run(|| Signal::new(None)),
            keyboard: mue_core::scope::Scope::null().run(|| Signal::new(Rect::default())),
            actions: RefCell::new(Vec::new()),
        }));
    });
}

/// A field's handle on the platform input method, from [`attach`].
/// Dropping it ends the session if this field is still the client.
pub struct TextInputConnection {
    id: u64,
    platform: Rc<Platform>,
}

impl Platform {
    /// Attach a field: the previous client (if any) is detached first.
    pub(crate) fn attach(
        self: &Rc<Self>,
        config: &TextInputConfig,
        value: &TextEditingValue,
        sink: impl Fn(ImeEvent) + 'static,
    ) -> TextInputConnection {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        *self.client.borrow_mut() = Some((id, Rc::new(sink)));
        self.backend.start(config);
        *self.mirrored.borrow_mut() = Some(value.clone());
        self.backend.set_state(value);
        self.caret.set(None);
        TextInputConnection {
            id,
            platform: self.clone(),
        }
    }

    fn is_attached(&self, id: u64) -> bool {
        self.client
            .borrow()
            .as_ref()
            .is_some_and(|(client, _)| *client == id)
    }
}

impl TextInputConnection {
    /// Tell the platform about a local edit or selection change.
    pub fn set_value(&self, value: &TextEditingValue) {
        if !self.platform.is_attached(self.id) {
            return;
        }
        let mut mirrored = self.platform.mirrored.borrow_mut();
        if mirrored.as_ref() == Some(value) {
            return;
        }
        *mirrored = Some(value.clone());
        drop(mirrored);
        self.platform.backend.set_state(value);
    }

    /// Report where the caret (or composition) is, in logical window pixels.
    pub fn set_caret_rect(&self, rect: Rect) {
        if !self.platform.is_attached(self.id) || self.platform.caret.get_untracked() == Some(rect)
        {
            return;
        }
        self.platform.caret.set(Some(rect));
        self.platform.backend.set_caret_rect(rect);
    }

    /// Whether this connection is still the platform's client (another
    /// field may have attached since).
    pub fn is_attached(&self) -> bool {
        self.platform.is_attached(self.id)
    }
}

impl Drop for TextInputConnection {
    fn drop(&mut self) {
        let mut client = self.platform.client.borrow_mut();
        if client.as_ref().is_some_and(|(id, _)| *id == self.id) {
            *client = None;
            drop(client);
            *self.platform.mirrored.borrow_mut() = None;
            self.platform.caret.set(None);
            self.platform.backend.stop();
        }
    }
}

/// A raw event from miniquad, before routing.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PlatformEvent {
    Preedit {
        text: String,
        cursor_utf16: Option<usize>,
    },
    Commit(Option<String>),
    State {
        text: String,
        selection: (i32, i32),
        composing: (i32, i32),
    },
    Action(TextInputAction),
    /// Soft keyboard frame in logical window pixels (empty when hidden).
    Keyboard(Rect),
}

thread_local! {
    static QUEUE: RefCell<Vec<ImeEvent>> = const { RefCell::new(Vec::new()) };
}

/// Route a platform event to the attached field.
pub(crate) fn dispatch_platform_event(event: PlatformEvent) {
    let platform = platform();
    let event = match event {
        PlatformEvent::Keyboard(rect) => {
            if platform.keyboard.get_untracked() != rect {
                platform.keyboard.set(rect);
            }
            return;
        }
        PlatformEvent::Preedit { text, cursor_utf16 } => {
            let cursor = cursor_utf16.map(|cursor| utf16_to_byte(&text, cursor));
            ImeEvent::Preedit { text, cursor }
        }
        PlatformEvent::Commit(text) => ImeEvent::Commit(text.unwrap_or_default()),
        PlatformEvent::State {
            text,
            selection,
            composing,
        } => {
            let byte = |offset: i32| utf16_to_byte(&text, offset.max(0) as usize);
            let value = TextEditingValue {
                selection: TextSelection::new(byte(selection.0), byte(selection.1)),
                composing: (composing.0 >= 0 && composing.1 > composing.0)
                    .then(|| byte(composing.0)..byte(composing.1)),
                text: text.clone(),
            }
            .normalized();
            // The platform already has this state.
            *platform.mirrored.borrow_mut() = Some(value.clone());
            ImeEvent::State(value)
        }
        PlatformEvent::Action(action) => {
            let handlers: Vec<_> = platform.actions.borrow().clone();
            for handler in handlers {
                handler(action);
            }
            ImeEvent::Action(action)
        }
    };
    let sink = platform
        .client
        .borrow()
        .as_ref()
        .map(|(_, sink)| sink.clone());
    match sink {
        Some(sink) => sink(event),
        None => QUEUE.with(|queue| queue.borrow_mut().push(event)),
    }
}

/// Input-method events that arrived while no field was attached, for hosts
/// that drive text input themselves. Clears the queue.
pub fn take_events() -> Vec<ImeEvent> {
    QUEUE.with(|queue| std::mem::take(&mut *queue.borrow_mut()))
}

/// The soft keyboard's frame in logical window pixels; empty while hidden or
/// on platforms without a soft keyboard. Tracked.
pub fn keyboard_frame() -> ReadSignal<Rect> {
    *platform().keyboard
}

/// The focused field's caret (or composition) rectangle in logical window
/// pixels, while a field is attached. Tracked. Scroll areas use it to keep
/// the caret above the soft keyboard.
pub fn caret_rect() -> ReadSignal<Option<Rect>> {
    *platform().caret
}

/// How far the soft keyboard covers the window from the bottom, in logical
/// pixels. Tracked; 0 while hidden.
pub fn keyboard_inset() -> ReadSignal<f32> {
    let frame = keyboard_frame();
    mue_core::effect::computed(move |_| {
        let frame = frame.get();
        if frame.w <= 0. || frame.h <= 0. {
            return 0.;
        }
        let height = crate::viewport::read().get().height;
        (height - frame.y).clamp(0., height)
    })
}

/// Whether a soft keyboard is showing. Tracked.
pub fn keyboard_visible() -> ReadSignal<bool> {
    let frame = keyboard_frame();
    mue_core::effect::computed(move |_| {
        let frame = frame.get();
        frame.w > 0. && frame.h > 0.
    })
}

/// Make a text field the client of the platform input method: start a
/// session with `config`, send `value`, and deliver input-method events for
/// it to `sink` until the connection is dropped or another field attaches.
///
/// The `text_edit` node does this when it gains focus; custom editors
/// can use it directly.
pub fn attach(
    config: &TextInputConfig,
    value: &TextEditingValue,
    sink: impl Fn(ImeEvent) + 'static,
) -> TextInputConnection {
    platform().attach(config, value, sink)
}

/// Run `handler` for every keyboard action key press, whichever field sent it.
pub fn on_text_input_action(handler: impl Fn(TextInputAction) + 'static) {
    platform().actions.borrow_mut().push(Rc::new(handler));
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    pub(crate) enum Call {
        Start(TextInputConfig),
        Stop,
        State(TextEditingValue),
        Caret(Rect),
    }

    #[derive(Clone, Default)]
    pub(crate) struct Recorder(pub Rc<RefCell<Vec<Call>>>);

    impl Backend for Recorder {
        fn start(&self, config: &TextInputConfig) {
            self.0.borrow_mut().push(Call::Start(*config));
        }
        fn stop(&self) {
            self.0.borrow_mut().push(Call::Stop);
        }
        fn set_state(&self, value: &TextEditingValue) {
            self.0.borrow_mut().push(Call::State(value.clone()));
        }
        fn set_caret_rect(&self, rect: Rect) {
            self.0.borrow_mut().push(Call::Caret(rect));
        }
    }

    pub(crate) fn record() -> Recorder {
        let recorder = Recorder::default();
        install_backend(Box::new(recorder.clone()));
        recorder
    }
}

#[cfg(test)]
mod tests {
    use super::{testing::*, *};

    #[test]
    fn attach_starts_and_drop_stops_only_the_current_client() {
        let calls = record();
        let platform = platform();
        let first = platform.attach(
            &TextInputConfig::default(),
            &TextEditingValue::new("a"),
            |_| {},
        );
        let second = platform.attach(
            &TextInputConfig::default(),
            &TextEditingValue::new("b"),
            |_| {},
        );
        drop(first);
        assert!(
            !calls.0.borrow().contains(&Call::Stop),
            "a replaced client must not stop input"
        );
        assert!(second.is_attached());
        drop(second);
        assert_eq!(calls.0.borrow().last(), Some(&Call::Stop));
    }

    #[test]
    fn state_from_the_platform_is_not_echoed_back() {
        let calls = record();
        let received: Rc<RefCell<Vec<ImeEvent>>> = Rc::default();
        let sink = received.clone();
        let connection = platform().attach(
            &TextInputConfig::default(),
            &TextEditingValue::default(),
            move |event| sink.borrow_mut().push(event),
        );
        dispatch_platform_event(PlatformEvent::State {
            text: "a😀".into(),
            selection: (3, 3),
            composing: (1, 3),
        });
        let ImeEvent::State(value) = received.borrow()[0].clone() else {
            panic!("expected state");
        };
        assert_eq!(value.selection, TextSelection::caret(5));
        assert_eq!(value.composing, Some(1..5));
        let before = calls.0.borrow().len();
        connection.set_value(&value);
        assert_eq!(
            calls.0.borrow().len(),
            before,
            "the platform already has this state"
        );
        connection.set_value(&value.insert("!"));
        assert_eq!(calls.0.borrow().len(), before + 1);
    }

    #[test]
    fn preedit_cursor_is_converted_to_bytes() {
        let _calls = record();
        let received: Rc<RefCell<Vec<ImeEvent>>> = Rc::default();
        let sink = received.clone();
        let _connection = platform().attach(
            &TextInputConfig::default(),
            &TextEditingValue::default(),
            move |event| sink.borrow_mut().push(event),
        );
        dispatch_platform_event(PlatformEvent::Preedit {
            text: "你好".into(),
            cursor_utf16: Some(1),
        });
        assert_eq!(
            received.borrow()[0],
            ImeEvent::Preedit {
                text: "你好".into(),
                cursor: Some(3)
            }
        );
    }
}
