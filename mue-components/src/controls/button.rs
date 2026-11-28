//! Buttons: [`button`], [`icon_button`], and [`button_group`], modeled on
//! Chakra UI's `Button`.
//!
//! A button has a [`ButtonVariant`] (solid, subtle, outline, ghost, link), a
//! [`Palette`] (Chakra's `colorScheme`), and a [`ControlSize`]. Sizes are in
//! `rem`, so buttons follow the root font size. Colors follow the color mode.
//!
//! ```
//! use mue_components::prelude::*;
//! use mue_components::system::{ControlSize, Palette};
//!
//! let _save = button("Save").palette(Palette::Teal).on_activate(|| {});
//! let _cancel = button("Cancel").variant(ButtonVariant::Ghost);
//! let _more = button("More").variant(ButtonVariant::Outline).size(ControlSize::Sm);
//! ```

use std::{cell::RefCell, rc::Rc};

use mue_core::prelude::*;
use mue_macroquad::{
    input::cursor::Cursor,
    input::{keyboard::KeyCode, pointer::PointerAction},
    motion::transition::Transition,
    node::{
        BuildNode, IntoChildren, IntoNode, Node, NodeContext, flexbox, flexbox::use_container,
        text,
    },
    paint::color::Color,
    paint::shader::Brush,
    style::{Size, Style, Styleable, StyleableExt, Transform, TransformOp, values::Display},
};

use crate::{
    controls::spinner,
    system::{self, ControlSize, Palette, white_alpha},
};

/// Visual style of a button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// Filled with the palette color.
    #[default]
    Solid,
    /// A light tint of the palette color.
    Subtle,
    /// A border and colored text.
    Outline,
    /// Colored text; a tint appears on hover.
    Ghost,
    /// Text only, like a link.
    Link,
}

#[derive(Clone, Copy, PartialEq)]
struct Colors {
    background: Color,
    foreground: Color,
    border: Color,
}

/// Chakra's button recipe for one variant, palette, mode, and state.
fn colors(variant: ButtonVariant, palette: Palette, dark: bool, hover: bool, active: bool) -> Colors {
    let state = usize::from(hover) + usize::from(active);
    let gray = palette == Palette::Gray;
    let clear = Color::TRANSPARENT;
    let tint = |shade: u16, alphas: [f32; 3]| palette.alpha(shade, alphas[state.min(2)]);
    match variant {
        ButtonVariant::Solid => {
            let (background, foreground) = match (gray, dark) {
                (true, false) => (
                    Palette::Gray.shade([100, 200, 300][state]),
                    Palette::Gray.shade(800),
                ),
                (true, true) => (white_alpha([0.2, 0.3, 0.4][state]), white_alpha(0.92)),
                (false, false) => {
                    let light_text = matches!(palette, Palette::Yellow | Palette::Cyan);
                    let base = if light_text { [400, 500, 600] } else { [500, 600, 700] };
                    let text = if light_text { Palette::Gray.shade(800) } else { Color::WHITE };
                    (palette.shade(base[state]), text)
                }
                (false, true) => (
                    palette.shade([200, 300, 400][state]),
                    Palette::Gray.shade(800),
                ),
            };
            Colors { background, foreground, border: clear }
        }
        ButtonVariant::Subtle => {
            let (background, foreground) = match (gray, dark) {
                (true, false) => (
                    Palette::Gray.shade([100, 200, 300][state]),
                    Palette::Gray.shade(800),
                ),
                (true, true) => (white_alpha([0.1, 0.16, 0.24][state]), white_alpha(0.92)),
                (false, false) => (palette.shade([100, 200, 300][state]), palette.shade(800)),
                (false, true) => (tint(200, [0.16, 0.24, 0.32]), palette.shade(200)),
            };
            Colors { background, foreground, border: clear }
        }
        ButtonVariant::Outline | ButtonVariant::Ghost => {
            let (background, foreground) = match (gray, dark) {
                (true, false) => (
                    [clear, Palette::Gray.shade(100), Palette::Gray.shade(200)][state],
                    Palette::Gray.shade(800),
                ),
                (true, true) => ([clear, white_alpha(0.2), white_alpha(0.3)][state], white_alpha(0.92)),
                (false, false) => (
                    [clear, palette.shade(50), palette.shade(100)][state],
                    palette.shade(600),
                ),
                (false, true) => (tint(200, [0., 0.12, 0.24]), palette.shade(200)),
            };
            let border = if variant == ButtonVariant::Ghost {
                clear
            } else if gray {
                if dark { white_alpha(0.24) } else { Palette::Gray.shade(200) }
            } else {
                foreground
            };
            Colors { background, foreground, border }
        }
        ButtonVariant::Link => {
            let foreground = if dark {
                palette.shade([200, 300, 400][state])
            } else {
                palette.shade([500, 600, 700][state])
            };
            let foreground = if gray {
                if dark { white_alpha(0.92) } else { Palette::Gray.shade(800) }
            } else {
                foreground
            };
            Colors { background: clear, foreground, border: clear }
        }
    }
}

/// Builder returned by [`button`] and [`icon_button`].
pub struct ButtonBuilder {
    label: Option<Prop<Rc<str>>>,
    variant: ButtonVariant,
    palette: Palette,
    size: ControlSize,
    disabled: Prop<bool>,
    loading: Prop<bool>,
    loading_text: Option<Prop<Rc<str>>>,
    left_icon: Option<Node>,
    right_icon: Option<Node>,
    icon: Option<Node>,
    full_width: bool,
    action: Option<Box<dyn FnMut()>>,
    style: Style,
}

/// A button with a text label: solid gray by default.
///
/// It is focusable, shows a focus ring for keyboard focus, and activates on
/// a click, a tap, or Enter or Space while focused. A disabled or loading
/// button ignores activation; a loading one shows a [`spinner`] (and
/// [`loading_text`](ButtonBuilder::loading_text) when set). Pressing scales it
/// down slightly. Its hit area is at least 44 × 44 logical pixels even when
/// it is drawn smaller. Label, disabled, and loading accept reactive values.
pub fn button(label: impl Into<Prop<Rc<str>>>) -> ButtonBuilder {
    ButtonBuilder {
        label: Some(label.into()),
        variant: ButtonVariant::Solid,
        palette: Palette::Gray,
        size: ControlSize::Md,
        disabled: Prop::Static(false),
        loading: Prop::Static(false),
        loading_text: None,
        left_icon: None,
        right_icon: None,
        icon: None,
        full_width: false,
        action: None,
        style: Style::new(),
    }
}

/// A square button that shows only an icon (Chakra's `IconButton`); ghost
/// by default.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _close = icon_button(text("✕")).on_activate(|| {});
/// ```
pub fn icon_button(icon: impl IntoNode) -> ButtonBuilder {
    ButtonBuilder {
        label: None,
        icon: Some(icon.into_node()),
        variant: ButtonVariant::Ghost,
        ..button("")
    }
}

impl ButtonBuilder {
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// The accent color (Chakra's `colorScheme`).
    pub fn palette(mut self, palette: Palette) -> Self {
        self.palette = palette;
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// Shorthand for the main action of a view: a solid blue button.
    pub fn primary(mut self, primary: bool) -> Self {
        if primary {
            self.variant = ButtonVariant::Solid;
            self.palette = Palette::Blue;
        } else {
            self.palette = Palette::Gray;
        }
        self
    }

    pub fn disabled(mut self, disabled: impl Into<Prop<bool>>) -> Self {
        self.disabled = disabled.into();
        self
    }

    pub fn loading(mut self, loading: impl Into<Prop<bool>>) -> Self {
        self.loading = loading.into();
        self
    }

    /// Text shown next to the spinner while loading (the label is hidden).
    pub fn loading_text(mut self, label: impl Into<Prop<Rc<str>>>) -> Self {
        self.loading_text = Some(label.into());
        self
    }

    /// An icon before the label.
    pub fn left_icon(mut self, icon: impl IntoNode) -> Self {
        self.left_icon = Some(icon.into_node());
        self
    }

    /// An icon after the label.
    pub fn right_icon(mut self, icon: impl IntoNode) -> Self {
        self.right_icon = Some(icon.into_node());
        self
    }

    /// Stretch to the width of the container.
    pub fn full_width(mut self, full: bool) -> Self {
        self.full_width = full;
        self
    }

    pub fn on_activate(mut self, action: impl FnMut() + 'static) -> Self {
        self.action = Some(Box::new(action));
        self
    }
}

impl Styleable for ButtonBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for ButtonBuilder {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            label,
            variant,
            palette,
            size,
            disabled,
            loading,
            loading_text,
            left_icon,
            right_icon,
            icon,
            full_width,
            mut action,
            style,
        } = self;
        let hovered = signal(false);
        let pressed = signal(false);
        let dark = system::is_dark();
        let inactive = move || disabled.try_get().unwrap_or(false) || loading.try_get().unwrap_or(false);
        let state = computed(move |_| {
            let inactive = inactive();
            colors(
                variant,
                palette,
                dark.get(),
                hovered.get() && !inactive,
                pressed.get() && !inactive,
            )
        });
        let background = state.map(|colors| Brush::Solid(colors.background));
        let foreground = state.map(|colors| Brush::Solid(colors.foreground));
        let border = state.map(|colors| Brush::Solid(colors.border));
        let opacity = disabled.map(|disabled| if disabled { 0.4 } else { 1. });
        let scale = computed(move |_| {
            let s = if pressed.get() && !inactive() { 0.96 } else { 1. };
            Transform(vec![TransformOp::Scale(s, s)])
        });
        let link = variant == ButtonVariant::Link;
        let square = icon.is_some();

        let mut style = style.use_pressed(pressed);
        style.provide_defaults(
            Style::new()
                .background(background)
                .border_color(border)
                .color(foreground)
                .opacity(opacity)
                .transform(scale)
                .transition(Transition::ease_out(0.12))
                .focusable(true)
                .cursor(disabled.map(|disabled| {
                    if disabled { Cursor::NotAllowed } else { Cursor::Pointer }
                }))
                .min_hit_size(44.)
                .focus_ring(2., system::focus_ring(), 2.),
        );
        ctx.push_style(style);

        let mut defaults = Style::new()
            .flex_row()
            .items_center()
            .justify_center()
            .flex_shrink(0.)
            .gap(size.gap())
            .font_size(size.font_size())
            .line_height(size.line_height())
            .font_weight(cosmic_text::Weight::SEMIBOLD)
            .radius(size.radius());
        if !link {
            defaults = defaults
                .height(size.height())
                .min_width(size.height())
                .padding_x(if square { Size::Px(0.) } else { size.button_padding() });
        }
        if variant == ButtonVariant::Outline {
            defaults = defaults.border((Size::Px(1.),));
        }
        if full_width {
            defaults = defaults.width(Size::Percent(100.));
        }

        ctx.finish(defaults, move |style| {
            style
                .on_hover_event
                .append(move |event| match event.action() {
                    PointerAction::Down => hovered.set(true),
                    PointerAction::Up => hovered.set(false),
                    _ => {}
                });
            let action = Rc::new(RefCell::new(action.take()));
            let activate = move || {
                if inactive() {
                    return;
                }
                if let Some(action) = action.borrow_mut().as_mut() {
                    action();
                }
            };
            style.on_tap.append({
                let activate = activate.clone();
                move |_| activate()
            });
            // Keyboard activation while focused.
            style.on_key_down.append(move |event| {
                if !event.repeat
                    && matches!(event.key, KeyCode::Enter | KeyCode::KpEnter | KeyCode::Space)
                {
                    event.stop_propagation();
                    activate();
                }
            });

            let is_loading = computed(move |_| loading.try_get().unwrap_or(false));
            let idle = is_loading.map(|loading| !loading);
            let mut children: Vec<Node> = Vec::new();
            children.push(
                spinner()
                    .size(match size {
                        ControlSize::Xs | ControlSize::Sm => ControlSize::Xs,
                        _ => ControlSize::Sm,
                    })
                    .color(foreground)
                    .display(visible(is_loading))
                    .into_node(),
            );
            let has_loading_text = loading_text.is_some();
            if let Some(loading_text) = loading_text {
                children.push(text(loading_text).color(foreground).display(visible(is_loading)).into_node());
            }
            let content: Vec<Node> = left_icon
                .into_iter()
                .chain(icon)
                .chain(label.map(|label| text(label).color(foreground).into_node()))
                .chain(right_icon)
                .collect();
            if !content.is_empty() {
                let content = flexbox()
                    .flex_row()
                    .items_center()
                    .gap(size.gap())
                    .children(content);
                children.push(if has_loading_text {
                    // The loading text replaces the label.
                    content.display(visible(idle)).into_node()
                } else {
                    // Keep the button's size while loading; hide the content.
                    content.opacity(idle.map(|idle| if idle { 1. } else { 0. })).into_node()
                });
            }
            mue_macroquad::node::set_children(children.into_children());
            use_container(style);
        })
    }
}

fn visible(shown: ReadSignal<bool>) -> Prop<Display> {
    shown
        .map(|shown| if shown { Display::Flex } else { Display::None })
        .into()
}

/// Builder returned by [`button_group`].
pub struct ButtonGroupBuilder {
    buttons: Vec<ButtonBuilder>,
    attached: bool,
    variant: Option<ButtonVariant>,
    palette: Option<Palette>,
    size: Option<ControlSize>,
    style: Style,
}

/// A row of buttons (Chakra's `ButtonGroup`). Variant, palette, and size
/// set on the group apply to every button; `attached(true)` joins them into
/// one segmented control.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _pager = button_group(vec![button("Prev"), button("Next")])
///     .variant(ButtonVariant::Outline)
///     .attached(true);
/// ```
pub fn button_group(buttons: Vec<ButtonBuilder>) -> ButtonGroupBuilder {
    ButtonGroupBuilder {
        buttons,
        attached: false,
        variant: None,
        palette: None,
        size: None,
        style: Style::new(),
    }
}

impl ButtonGroupBuilder {
    pub fn attached(mut self, attached: bool) -> Self {
        self.attached = attached;
        self
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = Some(variant);
        self
    }

    pub fn palette(mut self, palette: Palette) -> Self {
        self.palette = Some(palette);
        self
    }

    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = Some(size);
        self
    }
}

impl Styleable for ButtonGroupBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for ButtonGroupBuilder {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            buttons,
            attached,
            variant,
            palette,
            size,
            style,
        } = self;
        let count = buttons.len();
        let children: Vec<Node> = buttons
            .into_iter()
            .enumerate()
            .map(|(index, mut button)| {
                if let Some(variant) = variant {
                    button.variant = variant;
                }
                if let Some(palette) = palette {
                    button.palette = palette;
                }
                if let Some(size) = size {
                    button.size = size;
                }
                if attached {
                    let mut join = Style::new();
                    if index > 0 {
                        join = join.radius_left(Size::Px(0.));
                        if button.variant == ButtonVariant::Outline {
                            join = join.margin_left(Size::Px(-1.));
                        }
                    }
                    if index + 1 < count {
                        join = join.radius_right(Size::Px(0.));
                    }
                    button.style.provide_defaults(join);
                }
                button.into_node()
            })
            .collect();
        ctx.push_style(style);
        let gap = if attached { Size::Px(0.) } else { Size::Rem(0.5) };
        ctx.finish(Style::new().flex_row().items_center().gap(gap), move |style| {
            mue_macroquad::node::set_children(children.into_children());
            use_container(style);
        })
    }
}
