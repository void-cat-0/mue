//! Reusable text buttons for modal and menu surfaces.

use std::rc::Rc;

use mue_core::prelude::*;
use mue_macroquad::{
    color::Color,
    cursor::Cursor,
    event::{keyboard::KeyCode, pointer::PointerAction},
    node::{BuildNode, IntoChildren, Node, NodeContext, flexbox::use_container, text},
    shader::Brush,
    style::{Size, Style, Styleable, StyleableExt},
};

/// Visual weight of a button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    #[default]
    Secondary,
    Ghost,
}

fn background_color(variant: ButtonVariant, hovered: bool, pressed: bool, disabled: bool) -> Color {
    if disabled {
        return Color::from_rgba(70, 74, 86, 180);
    }
    match variant {
        ButtonVariant::Primary => {
            if pressed {
                Color::from_rgba(44, 92, 210, 255)
            } else if hovered {
                Color::from_rgba(82, 138, 255, 255)
            } else {
                Color::from_rgba(64, 120, 255, 255)
            }
        }
        ButtonVariant::Secondary => {
            if pressed {
                Color::from_rgba(34, 42, 62, 255)
            } else if hovered {
                Color::from_rgba(58, 70, 100, 255)
            } else {
                Color::from_rgba(46, 56, 82, 255)
            }
        }
        ButtonVariant::Ghost => {
            if pressed {
                Color::from_rgba(255, 255, 255, 28)
            } else if hovered {
                Color::from_rgba(255, 255, 255, 16)
            } else {
                Color::TRANSPARENT
            }
        }
    }
}

fn foreground_color(disabled: bool) -> Color {
    if disabled {
        Color::from_rgba(160, 164, 176, 255)
    } else {
        Color::WHITE
    }
}

/// A text button with secondary visual weight by default.
///
/// Use [`ButtonBuilder::primary`] or [`ButtonBuilder::variant`] to select its
/// visual weight. A disabled or loading button ignores activation. The button
/// is focusable, shows a focus ring for keyboard focus, and activates on Enter
/// or Space while focused. Its label,
/// disabled state, and loading state accept reactive properties. Builder hooks
/// and style settings remain part of the same node when consumed.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _save = button("Save")
///     .primary(true)
///     .on_activate(|| {});
/// ```
pub struct ButtonBuilder {
    label: Prop<Rc<str>>,
    variant: ButtonVariant,
    disabled: Prop<bool>,
    loading: Prop<bool>,
    action: Option<Box<dyn FnMut()>>,
    style: Style,
}

pub fn button(label: impl Into<Prop<Rc<str>>>) -> ButtonBuilder {
    ButtonBuilder {
        label: label.into(),
        variant: ButtonVariant::Secondary,
        disabled: Prop::Static(false),
        loading: Prop::Static(false),
        action: None,
        style: Style::new(),
    }
}

impl ButtonBuilder {
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Convenience for the common primary/secondary switch.
    pub fn primary(mut self, primary: bool) -> Self {
        self.variant = if primary {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Secondary
        };
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
            disabled,
            loading,
            mut action,
            style,
        } = self;

        let hovered = signal(false);
        let pressed = signal(false);

        let background = computed(move |_| {
            Brush::Solid(background_color(
                variant,
                hovered.get(),
                pressed.get(),
                disabled.try_get().unwrap_or(false) || loading.try_get().unwrap_or(false),
            ))
        });
        let foreground = computed(move |_| {
            Brush::Solid(foreground_color(
                disabled.try_get().unwrap_or(false) || loading.try_get().unwrap_or(false),
            ))
        });
        let opacity = computed(move |_| {
            if disabled.try_get().unwrap_or(false) || loading.try_get().unwrap_or(false) {
                0.55
            } else {
                1.0
            }
        });

        let mut style = style.use_pressed(pressed);
        style.provide_defaults(
            Style::new()
                .background(background)
                .opacity(opacity)
                .focusable(true)
                .cursor(Cursor::Pointer)
                .focus_ring(2., Color::from_rgba(120, 170, 255, 255), 2.),
        );
        ctx.push_style(style);

        let defaults = Style::new()
            .flex_row()
            .items_center()
            .justify_center()
            .gap(Size::Px(8.))
            .padding_x(Size::Px(20.))
            .padding_y(Size::Px(10.))
            .min_height(Size::Px(44.))
            .radius(Size::Px(10.));

        ctx.finish(defaults, move |style| {
            style
                .on_hover_event
                .append(move |event| match event.action() {
                    PointerAction::Down => hovered.set(true),
                    PointerAction::Up => hovered.set(false),
                    _ => {}
                });
            let action = std::rc::Rc::new(std::cell::RefCell::new(action.take()));
            let activate = move || {
                if disabled.try_get().unwrap_or(false) || loading.try_get().unwrap_or(false) {
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
                    && matches!(
                        event.key,
                        KeyCode::Enter | KeyCode::KpEnter | KeyCode::Space
                    )
                {
                    event.stop_propagation();
                    activate();
                }
            });

            let label = computed(move |_| {
                if loading.try_get().unwrap_or(false) {
                    Rc::from("…")
                } else {
                    label.try_get_clone().unwrap_or_default()
                }
            });
            let text = text(label).color(foreground);
            mue_macroquad::node::set_children(text.into_children());
            use_container(style);
        })
    }
}
