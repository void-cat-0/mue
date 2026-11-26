//! Text inputs: [`input`], [`textarea`], and [`input_group`].
//!
//! The controls wrap the renderer's `text_edit` node with field styling, a
//! focus border, an invalid state, and Vue-style model modifiers:
//!
//! | Vue | mue |
//! | --- | --- |
//! | `v-model="name"` | `.model(name)` (`Signal<String>`) |
//! | `v-model.lazy` | `.lazy()`: write the model on blur or Enter, not on each edit |
//! | `v-model.trim` | `.trim()`: write the text without surrounding whitespace |
//! | `v-model.number` | `.model_parsed(age)` for any `T: FromStr + Display` |
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_core::prelude::*;
//!
//! # let owner = mue_core::scope::OwnedScope::new();
//! # owner.run(|| {
//! let name = signal(String::new());
//! let age = signal(30_u32);
//! let _name = input().model(name).trim().placeholder("Name");
//! let _age = input().model_parsed(age).placeholder("Age");
//! let _bio = textarea().model(name).lazy().rows(4);
//! # });
//! ```

use std::{rc::Rc, str::FromStr};

use mue_core::prelude::*;
use mue_macroquad::{
    input::text_input::{TextCapitalization, TextFormatter, TextInputAction, TextInputKind},
    node::{
        BuildNode, IntoChildren, IntoNode, Node, NodeContext, TextEditBuilder, TextEditHandle,
        flexbox::use_container, text_edit,
    },
    paint::color::Color,
    paint::shader::Brush,
    style::{Size, Style, Styleable, StyleableExt},
};

/// Field size: padding, font size, and minimum height.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputSize {
    Sm,
    #[default]
    Md,
    Lg,
}

/// Field appearance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputVariant {
    /// A bordered box.
    #[default]
    Outline,
    /// A filled box without a visible border until focused.
    Filled,
    /// Only a bottom border.
    Flushed,
}

type TextHandler = Rc<dyn Fn(&str)>;
/// Connects the field's draft text to a caller's model: `(draft, change handlers, invalid)`.
type Sync = Box<dyn FnOnce(Signal<String>, &mut Vec<TextHandler>, Signal<bool>)>;

/// Builder returned by [`input`] and [`textarea`].
pub struct InputBuilder {
    field: TextEditBuilder,
    /// Connects the field's text to the caller's model, if any.
    model: Option<Sync>,
    lazy: bool,
    trim: bool,
    size: InputSize,
    variant: InputVariant,
    invalid: Prop<bool>,
    disabled: Prop<bool>,
    change: Vec<TextHandler>,
    multiline: bool,
    rows: usize,
    style: Style,
}

/// A single-line text field.
///
/// Uncontrolled by default (it keeps its own text); bind a model with
/// [`InputBuilder::model`] or [`InputBuilder::model_parsed`]. The field shows
/// a blue border while focused and a red one while [`InputBuilder::invalid`];
/// a parsed model that cannot parse the text also marks the field invalid.
pub fn input() -> InputBuilder {
    InputBuilder {
        field: text_edit(),
        model: None,
        lazy: false,
        trim: false,
        size: InputSize::Md,
        variant: InputVariant::Outline,
        invalid: Prop::Static(false),
        disabled: Prop::Static(false),
        change: Vec::new(),
        multiline: false,
        rows: 3,
        style: Style::new(),
    }
}

/// A multiline text field (Enter inserts a newline). Three rows tall by
/// default; see [`InputBuilder::rows`].
pub fn textarea() -> InputBuilder {
    let mut builder = input();
    builder.multiline = true;
    builder.field = builder.field.multiline(true);
    builder
}

impl InputBuilder {
    /// Two-way bind the text (Vue's `v-model`).
    pub fn model(mut self, model: Signal<String>) -> Self {
        self.model = Some(Box::new(move |draft, change, _invalid| {
            // Model → field: external writes replace the draft unless they
            // only differ by what `trim` removed.
            watch_effect(move || {
                let value = model.get_clone();
                let current = draft.get_clone_untracked();
                if value != current && value != current.trim() {
                    draft.set(value);
                }
            });
            change.push(Rc::new(move |text: &str| {
                if model.get_clone_untracked() != text {
                    model.set(text.to_string());
                }
            }));
        }));
        self
    }

    /// Two-way bind a parsed value (Vue's `v-model.number`, for any type).
    /// Text that does not parse leaves the model unchanged and marks the field
    /// invalid until it parses again; an empty field is not invalid.
    pub fn model_parsed<T>(mut self, model: Signal<T>) -> Self
    where
        T: FromStr + std::fmt::Display + PartialEq + Clone + 'static,
    {
        self.model = Some(Box::new(move |draft, change, invalid| {
            watch_effect(move || {
                let value = model.get_clone();
                let parsed = draft.get_clone_untracked().trim().parse::<T>().ok();
                if parsed.as_ref() != Some(&value) {
                    draft.set(value.to_string());
                    invalid.set(false);
                }
            });
            change.push(Rc::new(move |text: &str| {
                let text = text.trim();
                match text.parse::<T>() {
                    Ok(value) => {
                        invalid.set(false);
                        if model.get_clone_untracked() != value {
                            model.set(value);
                        }
                    }
                    Err(_) => invalid.set(!text.is_empty()),
                }
            }));
        }));
        self
    }

    /// Write the model when the field loses focus or Enter is pressed
    /// (Vue's `.lazy`), instead of after each edit.
    pub fn lazy(mut self) -> Self {
        self.lazy = true;
        self
    }

    /// Write the model without leading and trailing whitespace (Vue's `.trim`).
    pub fn trim(mut self) -> Self {
        self.trim = true;
        self
    }

    pub fn placeholder(mut self, text: impl Into<Prop<Rc<str>>>) -> Self {
        self.field = self.field.placeholder(text);
        self
    }

    pub fn kind(mut self, kind: TextInputKind) -> Self {
        self.field = self.field.kind(kind);
        self
    }

    pub fn action(mut self, action: TextInputAction) -> Self {
        self.field = self.field.action(action);
        self
    }

    pub fn autocorrect(mut self, autocorrect: bool) -> Self {
        self.field = self.field.autocorrect(autocorrect);
        self
    }

    pub fn capitalization(mut self, capitalization: TextCapitalization) -> Self {
        self.field = self.field.capitalization(capitalization);
        self
    }

    pub fn max_length(mut self, max: Option<usize>) -> Self {
        self.field = self.field.max_length(max);
        self
    }

    pub fn formatter(mut self, formatter: impl TextFormatter + 'static) -> Self {
        self.field = self.field.formatter(formatter);
        self
    }

    pub fn read_only(mut self, read_only: impl Into<Prop<bool>>) -> Self {
        self.field = self.field.read_only(read_only);
        self
    }

    pub fn disabled(mut self, disabled: impl Into<Prop<bool>>) -> Self {
        self.disabled = disabled.into();
        self
    }

    /// Show the error border (and set the invalid state for assistive styles).
    pub fn invalid(mut self, invalid: impl Into<Prop<bool>>) -> Self {
        self.invalid = invalid.into();
        self
    }

    pub fn size(mut self, size: InputSize) -> Self {
        self.size = size;
        self
    }

    pub fn variant(mut self, variant: InputVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn select_all_on_focus(mut self, select: bool) -> Self {
        self.field = self.field.select_all_on_focus(select);
        self
    }

    /// Visible rows of a [`textarea`] while its height is auto.
    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = rows.max(1);
        self
    }

    /// Called with the text after each committed edit.
    pub fn on_change(mut self, handler: impl Fn(&str) + 'static) -> Self {
        self.change.push(Rc::new(handler));
        self
    }

    /// Enter (single line) or the soft keyboard's action key.
    pub fn on_submit(mut self, handler: impl Fn(TextInputAction) + 'static) -> Self {
        self.field = self.field.on_submit(handler);
        self
    }

    /// Imperative access: focus, select, insert, undo.
    pub fn handle(mut self, handle: TextEditHandle) -> Self {
        self.field = self.field.handle(handle);
        self
    }
}

impl Styleable for InputBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

fn metrics(size: InputSize) -> (f32, f32, f32, f32) {
    // padding x, padding y, font size, line height
    match size {
        InputSize::Sm => (10., 5., 14., 20.),
        InputSize::Md => (12., 9., 16., 22.),
        InputSize::Lg => (16., 12., 18., 24.),
    }
}

impl BuildNode for InputBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            field,
            model,
            lazy,
            trim,
            size,
            variant,
            invalid,
            disabled,
            change,
            multiline,
            rows,
            style,
        } = self;
        let draft = signal(String::new());
        let parse_invalid = signal(false);
        let mut handlers: Vec<TextHandler> = Vec::new();
        if let Some(model) = model {
            model(draft, &mut handlers, parse_invalid);
        }
        handlers.extend(change);
        let handlers = Rc::new(handlers);
        let notify = {
            let handlers = handlers.clone();
            move |text: &str| {
                let text = if trim { text.trim() } else { text };
                for handler in handlers.iter() {
                    handler(text);
                }
            }
        };
        let mut field = field.value(draft).disabled(disabled);
        if multiline {
            field = field.min_lines(rows);
        }
        field = if lazy {
            let on_submit = notify.clone();
            field
                .on_commit(notify)
                .on_submit(move |_| on_submit(&draft.get_clone_untracked()))
        } else {
            field.on_change(notify)
        };
        let invalid = computed(move |_| invalid.get() || parse_invalid.get());
        let (pad_x, pad_y, font_size, line_height) = metrics(size);
        let border_color = move |focused: bool| -> Brush {
            let color = if invalid.get() {
                Color::from_rgba(240, 96, 96, 255)
            } else if focused {
                Color::from_rgba(90, 150, 255, 255)
            } else {
                match variant {
                    InputVariant::Filled => Color::TRANSPARENT,
                    _ => Color::from_rgba(70, 82, 108, 255),
                }
            };
            Brush::Solid(color)
        };
        let background = match variant {
            InputVariant::Outline => Color::from_rgba(24, 30, 44, 255),
            InputVariant::Filled => Color::from_rgba(38, 46, 66, 255),
            InputVariant::Flushed => Color::TRANSPARENT,
        };
        let opacity = disabled.map(|disabled| if disabled { 0.5 } else { 1. });
        let mut defaults = Style::new()
            .padding_x(Size::Px(pad_x))
            .padding_y(Size::Px(pad_y))
            .font_size(font_size)
            .line_height(Size::Px(line_height))
            .color(Color::from_rgba(230, 235, 245, 255))
            .background(background)
            .opacity(opacity)
            .min_width(Size::Px(0.));
        defaults = if variant == InputVariant::Flushed {
            defaults.border_bottom(Size::Px(1.)).padding_x(Size::Px(0.))
        } else {
            defaults.border((Size::Px(1.),)).radius(Size::Px(8.))
        };
        defaults = defaults.with_focus(|style, focus| {
            style.border_color(computed(move |_| border_color(focus.focused.get())))
        });
        // Caller styles win over the defaults.
        field.style(defaults).style(style).build(ctx)
    }
}

/// Builder returned by [`input_group`].
pub struct InputGroupBuilder {
    input: InputBuilder,
    left_addon: Option<Node>,
    right_addon: Option<Node>,
    left_element: Option<Node>,
    right_element: Option<Node>,
    style: Style,
}

/// An input with attached addons (outside the field: units, protocols) or
/// elements (inside it: icons, clear or reveal buttons), like Chakra's
/// `InputGroup`.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _url = input_group(input().placeholder("example.com")).left_addon(text("https://"));
/// let _search = input_group(input().placeholder("Search")).right_element(text("⌘K"));
/// ```
pub fn input_group(input: InputBuilder) -> InputGroupBuilder {
    InputGroupBuilder {
        input,
        left_addon: None,
        right_addon: None,
        left_element: None,
        right_element: None,
        style: Style::new(),
    }
}

impl InputGroupBuilder {
    /// Content attached before the field, in a box joined to its border.
    pub fn left_addon(mut self, content: impl IntoNode) -> Self {
        self.left_addon = Some(content.into_node());
        self
    }

    pub fn right_addon(mut self, content: impl IntoNode) -> Self {
        self.right_addon = Some(content.into_node());
        self
    }

    /// Content inside the field's left edge; the field pads its text past it.
    pub fn left_element(mut self, content: impl IntoNode) -> Self {
        self.left_element = Some(content.into_node());
        self
    }

    pub fn right_element(mut self, content: impl IntoNode) -> Self {
        self.right_element = Some(content.into_node());
        self
    }
}

impl Styleable for InputGroupBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for InputGroupBuilder {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            mut input,
            left_addon,
            right_addon,
            left_element,
            right_element,
            style,
        } = self;
        let (pad_x, _, font_size, line_height) = metrics(input.size);
        let addon = |content: Node, left: bool| {
            let mut style = Style::new()
                .flex_row()
                .items_center()
                .padding_x(Size::Px(pad_x))
                .font_size(font_size)
                .line_height(Size::Px(line_height))
                .background(Color::from_rgba(38, 46, 66, 255))
                .border((Size::Px(1.),))
                .border_color(Color::from_rgba(70, 82, 108, 255))
                .color(Color::from_rgba(170, 180, 200, 255));
            style = if left {
                style.radius_left(Size::Px(8.)).border_right(Size::Px(0.))
            } else {
                style.radius_right(Size::Px(8.)).border_left(Size::Px(0.))
            };
            mue_macroquad::node::flexbox()
                .style(style)
                .children(content.into_children())
                .into_node()
        };
        let element = |content: Node, left: bool| {
            let mut style = Style::new()
                .absolute()
                .inset_top(Size::Px(0.))
                .inset_bottom(Size::Px(0.))
                .flex_row()
                .items_center()
                .justify_center()
                .min_width(Size::Px(36.))
                .font_size(font_size)
                .line_height(Size::Px(line_height))
                .color(Color::from_rgba(150, 160, 180, 255))
                .z_index(1);
            style = if left {
                style.inset_left(Size::Px(0.))
            } else {
                style.inset_right(Size::Px(0.))
            };
            mue_macroquad::node::flexbox()
                .style(style)
                .children(content.into_children())
                .into_node()
        };
        // Join the field's corners to its addons and leave room for elements.
        let mut field_style = Style::new().flex_grow(1.);
        if left_addon.is_some() {
            field_style = field_style.radius_left(Size::Px(0.));
        }
        if right_addon.is_some() {
            field_style = field_style.radius_right(Size::Px(0.));
        }
        if left_element.is_some() {
            field_style = field_style.padding_left(Size::Px(36.));
        }
        if right_element.is_some() {
            field_style = field_style.padding_right(Size::Px(36.));
        }
        input.style.provide_defaults(field_style);
        let mut children: Vec<Node> = Vec::new();
        children.extend(left_addon.map(|node| addon(node, true)));
        let mut field_box: Vec<Node> = vec![input.into_node()];
        field_box.extend(left_element.map(|node| element(node, true)));
        field_box.extend(right_element.map(|node| element(node, false)));
        children.push(
            mue_macroquad::node::flexbox()
                .style(
                    Style::new()
                        .relative()
                        .flex_row()
                        .flex_grow(1.)
                        .min_width(Size::Px(0.)),
                )
                .children(field_box)
                .into_node(),
        );
        children.extend(right_addon.map(|node| addon(node, false)));
        ctx.push_style(style);
        ctx.finish(Style::new().flex_row().items_stretch(), move |style| {
            mue_macroquad::node::set_children(children.into_children());
            use_container(style);
        })
    }
}
