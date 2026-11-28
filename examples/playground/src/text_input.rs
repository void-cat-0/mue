use super::controls::*;
use mue_components::*;
use mue_core::prelude::*;
use mue_macroquad::{
    input::text_input::{
        DecimalNumber, TextEditingValue, TextInputAction, TextInputKind, TextSelection,
        byte_to_utf16, caret_rect, digits_only,
    },
    node::{TextEditHandle, text_edit},
    style::Styleable,
};
use std::rc::Rc;

const PAGE: &str = "Text input";

fn show<T: Clone + 'static>(
    value: ReadSignal<T>,
    format: impl Fn(&T) -> String + 'static,
) -> Prop<Rc<str>> {
    value
        .map(move |value| Rc::<str>::from(format(&value)))
        .into()
}

fn labeled(label: &str, field: impl IntoNode + 'static, readout: Option<Prop<Rc<str>>>) -> Node {
    stack()
        .style(mue_macroquad::style! {
            flex: 1;
            min-width: {u(220.)};
            gap: {u(4.)};
        })
        .children({
            let mut children = vec![
                text(Rc::<str>::from(label))
                    .style(mue_macroquad::style! {
                        font-size: {f(12.)}; line-height: {u(16.)};
                        color: {muted()};
                    })
                    .into_node(),
                field.into_node(),
            ];
            children.extend(readout.map(mono));
            children
        })
        .into_node()
}

fn fields_card() -> Node {
    let name = signal(String::new());
    let password = signal(String::new());
    let age = signal(30u32);
    let pin = signal(String::new());
    let price = signal(String::new());
    let email = signal(String::new());
    let submitted = |kind: &'static str| {
        move |action: TextInputAction| log_event(format!("{kind}: submit ({action:?})"))
    };
    card(
        "Field types",
        "Each field asks the soft keyboard for a matching layout and action key. On desktop, focus a field and type; on phones, check the keyboard that opens.",
        row((
            labeled(
                "Text (action: Next)",
                input()
                    .model(name)
                    .placeholder("Your name")
                    .action(TextInputAction::Next)
                    .on_submit(submitted("text")),
                Some(show(*name, |text| format!("{text:?}"))),
            ),
            labeled(
                "Password",
                input()
                    .model(password)
                    .kind(TextInputKind::Password)
                    .placeholder("Secret"),
                Some(show(*password, |text| {
                    format!("{} characters", text.chars().count())
                })),
            ),
            labeled(
                "Number (model_parsed u32)",
                input().model_parsed(age).kind(TextInputKind::Number),
                Some(show(*age, |age| format!("age = {age}"))),
            ),
            labeled(
                "Digits only, max 6",
                input()
                    .model(pin)
                    .formatter(digits_only())
                    .max_length(Some(6))
                    .kind(TextInputKind::Number)
                    .placeholder("123456"),
                Some(show(*pin, |pin| format!("{pin:?}"))),
            ),
            labeled(
                "Decimal",
                input()
                    .model(price)
                    .formatter(DecimalNumber { signed: true })
                    .kind(TextInputKind::Decimal)
                    .placeholder("-12.50"),
                None,
            ),
            labeled(
                "Email (action: Send)",
                input()
                    .model(email)
                    .kind(TextInputKind::Email)
                    .action(TextInputAction::Send)
                    .placeholder("you@example.com")
                    .on_submit(submitted("email")),
                None,
            ),
            labeled(
                "Search (action: Search)",
                input()
                    .kind(TextInputKind::Search)
                    .action(TextInputAction::Search)
                    .placeholder("Search…")
                    .on_submit(submitted("search")),
                None,
            ),
            labeled(
                "Phone",
                input()
                    .kind(TextInputKind::Phone)
                    .placeholder("+1 555 0100"),
                None,
            ),
            labeled(
                "URL (action: Go)",
                input()
                    .kind(TextInputKind::Url)
                    .action(TextInputAction::Go)
                    .placeholder("https://")
                    .on_submit(submitted("url")),
                None,
            ),
        )),
    )
}

fn states_card() -> Node {
    let (variant, variant_control) = segmented(
        "Variant",
        vec![
            ("Outline", InputVariant::Outline),
            ("Filled", InputVariant::Filled),
            ("Flushed", InputVariant::Flushed),
        ],
    );
    let (invalid, invalid_control) = option_switch("Invalid", false);
    let (disabled, disabled_control) = option_switch("Disabled", false);
    let (read_only, read_only_control) = option_switch("Read only", false);
    let fields = keyed(variant, move |variant| {
        let variant = *variant;
        stack()
            .style(mue_macroquad::style! { width: 100%; gap: {u(8.)}; })
            .children((
                input()
                    .variant(variant)
                    .size(InputSize::Sm)
                    .placeholder("Small")
                    .invalid(invalid)
                    .disabled(disabled)
                    .read_only(read_only),
                input()
                    .variant(variant)
                    .size(InputSize::Md)
                    .placeholder("Medium")
                    .invalid(invalid)
                    .disabled(disabled)
                    .read_only(read_only),
                input()
                    .variant(variant)
                    .size(InputSize::Lg)
                    .placeholder("Large")
                    .invalid(invalid)
                    .disabled(disabled)
                    .read_only(read_only),
            ))
            .into_node()
    });
    card(
        "Sizes, variants, and states",
        "A disabled field ignores input; a read-only field can still be selected and copied.",
        (
            variant_control,
            row((invalid_control, disabled_control, read_only_control)),
            fields,
        ),
    )
}

fn modifiers_card() -> Node {
    let live = signal(String::new());
    let lazy = signal(String::new());
    let trimmed = signal(String::new());
    card(
        "Model modifiers",
        "Vue's v-model modifiers. .lazy writes on blur or Enter; .trim drops surrounding spaces from the model.",
        row((
            labeled(
                ".model(text)",
                input().model(live).placeholder("Updates on every edit"),
                Some(show(*live, |t| format!("{t:?}"))),
            ),
            labeled(
                ".model(text).lazy()",
                input()
                    .model(lazy)
                    .lazy()
                    .placeholder("Updates on blur or Enter"),
                Some(show(*lazy, |t| format!("{t:?}"))),
            ),
            labeled(
                ".model(text).trim()",
                input()
                    .model(trimmed)
                    .trim()
                    .placeholder("  spaces are trimmed  "),
                Some(show(*trimmed, |t| format!("{t:?}"))),
            ),
        )),
    )
}

fn textarea_card() -> Node {
    let body = signal(String::from(
        "Multiline text.\nEnter inserts a new line; Up and Down keep the column.\n多行文本与中文输入。",
    ));
    card(
        "Textarea",
        "Grows with its content from four rows. Try selection across lines, Page Up/Down, and the soft keyboard's return key.",
        (
            textarea().model(body).rows(4),
            mono(show(*body, |text| {
                format!(
                    "{} characters · {} lines",
                    text.chars().count(),
                    text.lines().count().max(1)
                )
            })),
        ),
    )
}

fn groups_card() -> Node {
    let query = signal(String::new());
    card(
        "Input groups",
        "Addons attach to the border; elements sit inside it.",
        stack()
            .style(mue_macroquad::style! { width: 100%; gap: {u(8.)}; })
            .children((
                input_group(input().placeholder("example.com"))
                    .left_addon(text("https://"))
                    .right_addon(text(".org")),
                input_group(input().model(query).placeholder("Search"))
                    .right_element(button("Clear").on_activate(move || query.set(String::new()))),
            )),
    )
}

fn inspector_card() -> Node {
    let editing = signal(TextEditingValue::new("你好 😀 world"));
    let handle = TextEditHandle::new();
    let field_handle = handle.clone();
    let caret = caret_rect();
    let keyboard = use_keyboard();
    let state = editing.map(|value| {
        let selection = format!(
            "selection {}..{} bytes · {}..{} UTF-16",
            value.selection.start(),
            value.selection.end(),
            byte_to_utf16(&value.text, value.selection.start()),
            byte_to_utf16(&value.text, value.selection.end()),
        );
        let composing = match &value.composing {
            Some(range) if !range.is_empty() => format!(
                "composing {:?} = {:?}",
                range,
                value.text.get(range.clone()).unwrap_or("")
            ),
            _ => String::from("not composing"),
        };
        Rc::<str>::from(format!("text {:?}\n{selection}\n{composing}", value.text))
    });
    let platform = computed(move |_| {
        let caret = caret.get().map_or(String::from("caret: none"), |r| {
            format!(
                "caret rect: {:.0}, {:.0}  {:.0} × {:.0}",
                r.x, r.y, r.w, r.h
            )
        });
        let keyboard = if keyboard.visible.get() {
            format!("keyboard: visible, covers {:.0} px", keyboard.inset.get())
        } else {
            String::from("keyboard: hidden")
        };
        Rc::<str>::from(format!("{caret}\n{keyboard}"))
    });
    let h = handle.clone();
    let select_all = move || h.select_all();
    let h = handle.clone();
    let insert = move || {
        h.focus();
        h.insert("😀");
    };
    let h = handle.clone();
    let caret_home = move || {
        h.focus();
        h.set_selection(TextSelection::caret(0));
    };
    let h = handle.clone();
    let undo = move || h.undo();
    let h = handle.clone();
    let redo = move || h.redo();
    let h = handle.clone();
    let reset = move || h.set_text("你好 😀 world");
    let h = handle.clone();
    let focus = move || {
        h.focus();
    };
    let h = handle;
    let blur = move || h.blur();
    card(
        "IME inspector",
        "The raw editing state. While an input method composes (Pinyin, Kana, Hangul, dictation), the composing range shows the provisional text; the candidate window should open at the caret rect.",
        (
            text_edit()
                .editing_value(editing)
                .handle(field_handle)
                .placeholder("Compose here")
                .style(mue_macroquad::style! {
                    width: 100%;
                    padding: {(u(12.), u(10.))};
                    border-radius: {u(8.)};
                    background: {raised()};
                    font-size: {f(18.)}; line-height: {u(26.)};
                    color: {foreground()};
                }),
            mono(state),
            mono(platform),
            row((
                button("Focus").on_activate(focus),
                button("Blur").on_activate(blur),
                button("Select all").on_activate(select_all),
                button("Caret to start").on_activate(caret_home),
                button("Insert 😀").on_activate(insert),
                button("Undo").on_activate(undo),
                button("Redo").on_activate(redo),
                button("Reset").on_activate(reset),
            )),
        ),
    )
}

pub fn page() -> Node {
    page_body(
        "Text fields on top of the platform text input bridge: IME composition on desktop, soft keyboards and their mirrors on Android, iOS, and the web.",
        (
            fields_card(),
            states_card(),
            modifiers_card(),
            textarea_card(),
            groups_card(),
            inspector_card(),
            card(
                "Events",
                "Submits and soft-keyboard actions from the fields above.",
                event_log(),
            ),
            checklist(
                PAGE,
                &[
                    "Typing, selection, copy, cut, paste, undo, and redo work",
                    "IME composition is drawn inline and the candidate window sits at the caret",
                    "Committing an IME word and pressing Undo removes the whole word",
                    "Emoji and CJK text insert and delete as whole characters",
                    "Phones open the matching keyboard (number, email, phone, URL)",
                    "The keyboard's action key submits (see the event log)",
                    "A focused field below the fold scrolls above the soft keyboard",
                    "The password field never shows suggestions or the plain text",
                ],
            ),
        ),
    )
}
