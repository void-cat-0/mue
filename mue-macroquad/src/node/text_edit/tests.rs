use super::*;
use crate::{
    event::keyboard::KeyMods,
    focus::{dispatch_char, dispatch_key_down, testing},
    style::Styleable,
    text_input::{
        PlatformEvent, dispatch_platform_event,
        platform::testing::{Call, record},
    },
};

fn typed(text: &str) {
    for ch in text.chars() {
        dispatch_char(CharEvent::new(ch, KeyMods::default(), false));
    }
}

fn key(key: KeyCode, mods: KeyMods) {
    dispatch_key_down(KeyEvent::new(key, mods, false));
}

/// The shortcut modifier (Ctrl, or Cmd on macOS).
fn command() -> KeyMods {
    if cfg!(target_os = "macos") {
        KeyMods {
            logo: true,
            ..Default::default()
        }
    } else {
        KeyMods {
            ctrl: true,
            ..Default::default()
        }
    }
}

/// The word-movement modifier (Ctrl, or Option on macOS).
fn word() -> KeyMods {
    if cfg!(target_os = "macos") {
        KeyMods {
            alt: true,
            ..Default::default()
        }
    } else {
        KeyMods {
            ctrl: true,
            ..Default::default()
        }
    }
}

fn field(
    build: impl FnOnce(TextEditBuilder) -> TextEditBuilder,
) -> (TextEditHandle, crate::node::Node) {
    testing::reset();
    let handle = TextEditHandle::new();
    let root = testing::mount(build(
        text_edit().handle(handle.clone()).width(200.).height(30.),
    ));
    assert!(handle.focus(), "the field is focusable");
    (handle, root)
}

fn text(handle: &TextEditHandle) -> String {
    handle.value().unwrap().text
}

#[test]
fn typing_deleting_and_undo() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let _calls = record();
        let value = signal(String::from("x"));
        let (handle, _root) = field(|field| field.value(value));
        key(KeyCode::End, KeyMods::default());
        typed("hello world");
        assert_eq!(text(&handle), "xhello world");
        assert_eq!(
            value.get_clone(),
            "xhello world",
            "the bound signal follows edits"
        );
        key(KeyCode::Backspace, word());
        assert_eq!(text(&handle), "xhello ");
        key(KeyCode::Z, command());
        assert_eq!(text(&handle), "xhello world");
        key(KeyCode::A, command());
        typed("y");
        assert_eq!(text(&handle), "y");
        value.set("external".into());
        assert_eq!(
            text(&handle),
            "external",
            "external writes replace the text"
        );
        assert_eq!(handle.value().unwrap().selection, TextSelection::caret(8));
    });
}

#[test]
fn composition_is_shown_inline_and_committed_once() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let calls = record();
        let changes = Rc::new(RefCell::new(Vec::<String>::new()));
        let seen = changes.clone();
        let (handle, _root) =
            field(|field| field.on_change(move |text| seen.borrow_mut().push(text.into())));
        assert!(
            matches!(calls.0.borrow().first(), Some(Call::Start(_))),
            "focus starts a session"
        );
        typed("a");
        dispatch_platform_event(PlatformEvent::Preedit {
            text: "ni".into(),
            cursor_utf16: Some(2),
        });
        dispatch_platform_event(PlatformEvent::Preedit {
            text: "你".into(),
            cursor_utf16: Some(1),
        });
        let value = handle.value().unwrap();
        assert_eq!(value.text, "a你");
        assert_eq!(value.composing, Some(1..4));
        dispatch_platform_event(PlatformEvent::Commit(Some("你好".into())));
        assert_eq!(text(&handle), "a你好");
        handle.undo();
        assert_eq!(text(&handle), "a", "undo removes the committed composition");
        handle.redo();
        assert_eq!(text(&handle), "a你好");
        assert_eq!(
            *changes.borrow(),
            ["a", "a你好", "a", "a你好"],
            "compositions do not report changes"
        );
        handle.blur();
        assert_eq!(
            calls.0.borrow().last(),
            Some(&Call::Stop),
            "blur ends the session"
        );
    });
}

#[test]
fn mirror_states_apply_without_echo() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let calls = record();
        let (handle, _root) = field(|field| field);
        let before = calls.0.borrow().len();
        dispatch_platform_event(PlatformEvent::State {
            text: "hi😀".into(),
            selection: (4, 4),
            composing: (-1, -1),
        });
        assert_eq!(text(&handle), "hi😀");
        assert_eq!(handle.value().unwrap().selection, TextSelection::caret(6));
        assert!(
            calls.0.borrow()[before..]
                .iter()
                .all(|call| !matches!(call, Call::State(_))),
            "a state from the platform is not sent back"
        );
        key(KeyCode::Left, KeyMods::default());
        assert!(
            matches!(calls.0.borrow().last(), Some(Call::State(_))),
            "local edits are mirrored"
        );
    });
}

#[test]
fn single_line_limits_and_password_mask() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let _calls = record();
        let (handle, _root) =
            field(|field| field.max_length(Some(5)).kind(TextInputKind::Password));
        handle.insert("ab\ncdefg");
        assert_eq!(text(&handle), "ab cd");
        let display = DisplayText::new("a😀b", Some('•'));
        assert_eq!(display.text, "•••");
        assert_eq!(display.to_display(1), 3);
        assert_eq!(display.to_real(6), 5);
        assert_eq!(display.to_real(9), 6);
    });
}

#[test]
fn submit_and_multiline_enter() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let _calls = record();
        let submitted = Rc::new(Cell::new(None));
        let seen = submitted.clone();
        let (handle, _root) = field(|field| {
            field
                .action(TextInputAction::Search)
                .on_submit(move |action| seen.set(Some(action)))
        });
        key(KeyCode::Enter, KeyMods::default());
        assert_eq!(submitted.get(), Some(TextInputAction::Search));
        assert_eq!(text(&handle), "");
        let (handle, _root) = field(|field| field.multiline(true));
        typed("a");
        key(KeyCode::Enter, KeyMods::default());
        typed("b");
        assert_eq!(text(&handle), "a\nb");
    });
}

#[test]
fn read_only_fields_select_but_do_not_edit() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let calls = record();
        let value = signal(String::from("fixed"));
        let (handle, _root) = field(|field| field.value(value).read_only(true));
        assert!(matches!(calls.0.borrow().first(), Some(Call::Start(config)) if config.read_only));
        typed("x");
        key(KeyCode::Backspace, KeyMods::default());
        assert_eq!(text(&handle), "fixed");
        key(KeyCode::A, command());
        assert_eq!(handle.value().unwrap().selection, TextSelection::new(0, 5));
    });
}

#[test]
fn caret_offsets_round_trip_through_layout_cursors() {
    let display = DisplayText::new("ab\ncd", None);
    for offset in 0..=5 {
        let cursor = Field::cursor_for(&display, offset);
        assert_eq!(
            Field::offset_for(&display, cursor),
            offset,
            "offset {offset}"
        );
    }
    assert_eq!(Field::cursor_for(&display, 4), Cursor::new(1, 1));
}

#[test]
fn vertical_motion_keeps_the_pixel_column() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let _calls = record();
        let value = signal(String::from(
            "Multiline notes wrap and scroll.
输入法预编辑会在光标处显示下划线。",
        ));
        let (handle, _root) = field(|field| {
            field
                .value(value)
                .multiline(true)
                .width(360.)
                .height(80.)
                .font_size(16.)
                .line_height(22.)
        });
        let end = value.get_clone().len();
        handle.set_selection(TextSelection::caret(end));
        key(KeyCode::Up, KeyMods::default());
        // The CJK line is wider than the Latin one: Up from its end lands at
        // the end of the first line, not partway through it.
        assert_eq!(handle.value().unwrap().selection, TextSelection::caret(32));
        key(KeyCode::Up, KeyMods::default());
        assert_eq!(handle.value().unwrap().selection, TextSelection::caret(0));
        key(KeyCode::Down, KeyMods::default());
        key(KeyCode::Down, KeyMods::default());
        assert_eq!(
            handle.value().unwrap().selection,
            TextSelection::caret(end),
            "the column survives consecutive moves"
        );
    });
}
