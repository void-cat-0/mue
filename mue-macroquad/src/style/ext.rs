use mue_core::signal::{ReadSignal, Signal};
use paste::paste;
use taffy::{AlignContent, AlignItems, FlexDirection, Position};

use crate::{
    event::pointer::{PointerAction, PointerId},
    shader::Brush,
};

use super::{Size, Styleable};
use mue_core::prop::Prop;

macro_rules! impl_align_content {
    ($prefix:ident, $fn:ident) => {
        paste! {
            fn [<$prefix _start>](self) -> Self {
                self.$fn(AlignContent::FlexStart)
            }
            fn [<$prefix _end>](self) -> Self {
                self.$fn(AlignContent::FlexEnd)
            }
            fn [<$prefix _center>](self) -> Self {
                self.$fn(AlignContent::Center)
            }
            fn [<$prefix _between>](self) -> Self {
                self.$fn(AlignContent::SpaceBetween)
            }
            fn [<$prefix _around>](self) -> Self {
                self.$fn(AlignContent::SpaceAround)
            }
            fn [<$prefix _evenly>](self) -> Self {
                self.$fn(AlignContent::SpaceEvenly)
            }
            fn [<$prefix _stretch>](self) -> Self {
                self.$fn(AlignContent::Stretch)
            }
        }
    };
}
macro_rules! impl_align_items {
    ($prefix:ident, $fn:ident) => {
        paste! {
            fn [<$prefix _start>](self) -> Self {
                self.$fn(AlignItems::FlexStart)
            }
            fn [<$prefix _end>](self) -> Self {
                self.$fn(AlignItems::FlexEnd)
            }
            fn [<$prefix _center>](self) -> Self {
                self.$fn(AlignItems::Center)
            }
            fn [<$prefix _stretch>](self) -> Self {
                self.$fn(AlignItems::Stretch)
            }
        }
    };
}

/// Shortcuts for common layout styles and pointer interactions.
pub trait StyleableExt: Styleable {
    /// Set equal row and column spacing.
    fn gap(self, value: impl Into<Prop<Size>>) -> Self {
        let value = value.into();
        self.column_gap(value).row_gap(value)
    }

    /// Round all four corners of the background, border, and outline.
    fn radius(self, value: impl Into<Prop<Size>>) -> Self {
        let value = value.into();
        self.radius_top_left(value)
            .radius_top_right(value)
            .radius_bottom_right(value)
            .radius_bottom_left(value)
    }

    /// Round the two top corners.
    fn radius_top(self, value: impl Into<Prop<Size>>) -> Self {
        let value = value.into();
        self.radius_top_left(value).radius_top_right(value)
    }

    /// Round the two bottom corners.
    fn radius_bottom(self, value: impl Into<Prop<Size>>) -> Self {
        let value = value.into();
        self.radius_bottom_left(value).radius_bottom_right(value)
    }

    /// Round the two left corners.
    fn radius_left(self, value: impl Into<Prop<Size>>) -> Self {
        let value = value.into();
        self.radius_top_left(value).radius_bottom_left(value)
    }

    /// Round the two right corners.
    fn radius_right(self, value: impl Into<Prop<Size>>) -> Self {
        let value = value.into();
        self.radius_top_right(value).radius_bottom_right(value)
    }

    /// Set every border width and the border paint in one call.
    fn border_all(self, width: impl Into<Prop<Size>>, color: impl Into<Prop<Brush>>) -> Self {
        self.border((width,)).border_color(color)
    }

    /// Set the outline width, paint, and offset in one call.
    fn outline(
        self,
        width: impl Into<Prop<Size>>,
        color: impl Into<Prop<Brush>>,
        offset: impl Into<Prop<Size>>,
    ) -> Self {
        self.outline_width(width)
            .outline_color(color)
            .outline_offset(offset)
    }

    /// Handle a tap without an unused event argument.
    fn on_activate(self, mut action: impl FnMut() + 'static) -> Self {
        self.on_tap(move |_| action())
    }

    /// Toggle boolean state whenever this node is tapped.
    fn toggle_on_tap(self, state: Signal<bool>) -> Self {
        self.on_activate(move || state.toggle())
    }
    fn w_full(self) -> Self {
        self.width(Size::Percent(100.))
    }
    fn w_auto(self) -> Self {
        self.width(Size::Auto)
    }

    fn h_full(self) -> Self {
        self.height(Size::Percent(100.))
    }
    fn h_auto(self) -> Self {
        self.height(Size::Auto)
    }

    fn size_full(self) -> Self {
        self.width(Size::Percent(100.)).height(Size::Percent(100.))
    }

    fn relative(self) -> Self {
        self.position(Position::Relative)
    }
    fn absolute(self) -> Self {
        self.position(Position::Absolute)
    }

    fn flex_1(self) -> Self {
        self.flex_grow(1.).flex_shrink(1.)
    }
    fn flex_none(self) -> Self {
        self.flex_grow(0.).flex_shrink(0.)
    }

    fn flex_row(self) -> Self {
        self.flex_direction(FlexDirection::Row)
    }
    fn flex_column(self) -> Self {
        self.flex_direction(FlexDirection::Column)
    }

    impl_align_content!(align, align_content);
    impl_align_content!(justify, justify_content);

    impl_align_items!(items, align_items);
    impl_align_items!(justify_items, justify_items);

    fn use_pressed(self, pressed: Signal<bool>) -> Self {
        let mut active: Option<PointerId> = None;
        self.on_pointer_event(move |event| {
            if let Some(active_id) = &active {
                if *active_id != event.pointer_id() {
                    return;
                }

                return match event.action() {
                    PointerAction::Down | PointerAction::Move => {}
                    PointerAction::Up | PointerAction::Cancel => {
                        active = None;
                        pressed.set(false);
                    }
                };
            }

            if event.action() == PointerAction::Down {
                active = Some(event.pointer_id());
                pressed.set(true);
            }
        })
    }

    /// Configure styles from this node's focus state.
    ///
    /// The callback runs once now with live signals for "focused" and "focus
    /// visible" (focused by keyboard). Use it for focus rings:
    ///
    /// ```
    /// use mue_macroquad::{color::Color, node::flexbox, style::{Styleable, StyleableExt}};
    ///
    /// let _field = flexbox().focusable(true).with_focus(|style, focus| {
    ///     style.outline_width(focus.visible.map(|visible| if visible { 2. } else { 0. }))
    ///         .outline_color(Color::from_rgba(111, 168, 255, 255))
    ///         .outline_offset(2.)
    /// });
    /// ```
    fn with_focus(mut self, f: impl FnOnce(Self, crate::focus::FocusState) -> Self) -> Self {
        let owner = self.style_mut().ensure_focus_owner();
        f(self, owner.state())
    }

    /// Show an outline only while focus is visible (keyboard focus).
    fn focus_ring(self, width: f32, color: impl Into<Prop<Brush>>, offset: f32) -> Self {
        let color = color.into();
        self.with_focus(move |style, focus| {
            style
                .outline_width(
                    focus
                        .visible
                        .map(move |visible| if visible { width } else { 0. }),
                )
                .outline_color(color)
                .outline_offset(offset)
        })
    }

    /// Configure styles from this node's shared, read-only hover state.
    ///
    /// The callback runs once now. Reactive values created inside it belong to
    /// the hover owner and are retained when this style moves into a node.
    fn with_hovered(mut self, f: impl FnOnce(Self, ReadSignal<bool>) -> Self) -> Self {
        let owner = self.style_mut().ensure_hovered();
        owner.scope.run(|| f(self, *owner.signal))
    }
}

impl<T: Styleable> StyleableExt for T {}
