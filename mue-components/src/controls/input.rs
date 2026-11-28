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
    input::pointer::PointerAction,
    motion::transition::Transition,
    paint::color::Color,
    paint::shader::Brush,
    style::{Size, Style, Styleable, StyleableExt},
};

use crate::system::{self, ControlSize, Palette, white_alpha};

/// Field size: height, padding, and font size, in `rem` (Chakra's sizes).
pub type InputSize = ControlSize;

/// Field appearance (Chakra's input variants).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputVariant {
    /// A bordered box.
    #[default]
    Outline,
    /// A filled box; the border appears on focus.
    Filled,
    /// Only a bottom border, no padding at the sides.
    Flushed,
    /// No box at all: just the text.
    Unstyled,
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
/// a blue border (and ring) while focused and a red one while
/// [`InputBuilder::invalid`]; sizes are in `rem` and colors follow the color
/// mode;
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

/// Padding x, font size, line height, and height of a field.
fn metrics(size: ControlSize) -> (Size, Size, Size) {
    (size.field_padding(), size.line_height(), size.height())
}

#[derive(Clone, Copy, PartialEq)]
struct FieldColors {
    background: Color,
    border: Color,
}

/// Chakra's input recipe for one variant, mode, and state.
fn field_colors(variant: InputVariant, dark: bool, hover: bool, focus: bool, invalid: bool) -> FieldColors {
    let gray = |shade| Palette::Gray.shade(shade);
    let focus_color = if dark { Palette::Blue.shade(300) } else { Palette::Blue.shade(500) };
    let error = if dark { Palette::Red.shade(300) } else { Palette::Red.shade(500) };
    let accent = if invalid { Some(error) } else if focus { Some(focus_color) } else { None };
    match variant {
        InputVariant::Outline => FieldColors {
            background: Color::TRANSPARENT,
            border: accent.unwrap_or(match (dark, hover) {
                (false, false) => gray(200),
                (false, true) => gray(300),
                (true, false) => white_alpha(0.24),
                (true, true) => white_alpha(0.4),
            }),
        },
        InputVariant::Filled => FieldColors {
            background: if focus {
                Color::TRANSPARENT
            } else {
                match (dark, hover) {
                    (false, false) => gray(100),
                    (false, true) => gray(200),
                    (true, false) => white_alpha(0.06),
                    (true, true) => white_alpha(0.1),
                }
            },
            border: accent.unwrap_or(Color::TRANSPARENT),
        },
        InputVariant::Flushed => FieldColors {
            background: Color::TRANSPARENT,
            border: accent.unwrap_or(if dark { white_alpha(0.24) } else { gray(200) }),
        },
        InputVariant::Unstyled => FieldColors {
            background: Color::TRANSPARENT,
            border: Color::TRANSPARENT,
        },
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
        let (pad_x, line_height, height) = metrics(size);
        let dark = system::is_dark();
        let hovered = signal(false);
        let opacity = disabled.map(|disabled| if disabled { 0.4 } else { 1. });
        let mut defaults = Style::new()
            .padding_x(pad_x)
            .font_size(size.font_size())
            .line_height(line_height)
            .color(system::fg())
            .opacity(opacity)
            .transition(Transition::ease_out(0.15))
            .min_width(Size::Px(0.))
            .on_hover_event(move |event| match event.action() {
                PointerAction::Down => hovered.set(true),
                PointerAction::Up => hovered.set(false),
                _ => {}
            });
        if multiline {
            // Chakra's textarea: 0.5rem above and below the text.
            defaults = defaults.padding_y(Size::Rem(0.5)).min_height(height);
        } else {
            defaults = defaults.min_height(height).padding_y(Size::Px(0.));
        }
        defaults = match variant {
            InputVariant::Outline => defaults.border((Size::Px(1.),)).radius(size.radius()),
            InputVariant::Filled => defaults.border((Size::Px(2.),)).radius(size.radius()),
            InputVariant::Flushed => defaults
                .border_bottom(Size::Px(1.))
                .radius(Size::Px(0.))
                .padding_x(Size::Px(0.)),
            InputVariant::Unstyled => defaults.padding_x(Size::Px(0.)).min_height(Size::Px(0.)),
        };
        defaults = defaults.with_focus(|style, focus| {
            let colors = computed(move |_| {
                field_colors(variant, dark.get(), hovered.get(), focus.focused.get(), invalid.get())
            });
            style
                .background(colors.map(|colors| Brush::Solid(colors.background)))
                .border_color(colors.map(|colors| Brush::Solid(colors.border)))
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
        let size = input.size;
        let (pad_x, line_height, height) = metrics(size);
        let font_size = size.font_size();
        let radius = size.radius();
        let addon = |content: Node, left: bool| {
            let mut style = Style::new()
                .flex_row()
                .items_center()
                .padding_x(pad_x)
                .font_size(font_size)
                .line_height(line_height)
                .background(system::bg_subtle())
                .border((Size::Px(1.),))
                .border_color(system::border())
                .color(system::fg_muted());
            style = if left {
                style.radius_left(radius).border_right(Size::Px(0.))
            } else {
                style.radius_right(radius).border_left(Size::Px(0.))
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
                .min_width(height)
                .font_size(font_size)
                .line_height(line_height)
                .color(system::fg_muted())
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
            field_style = field_style.padding_left(height);
        }
        if right_element.is_some() {
            field_style = field_style.padding_right(height);
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
