use mue_core::signal::Signal;
use paste::paste;
use taffy::{AlignContent, AlignItems, Dimension, FlexDirection, Position};

use crate::event::pointer::{PointerAction, PointerId};

use super::Styleable;

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
    fn gap(self, value: impl Into<mue_core::prop::Prop<taffy::LengthPercentage>>) -> Self {
        let value = value.into();
        self.column_gap(value).row_gap(value)
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
        self.width(Dimension::percent(1.))
    }
    fn w_auto(self) -> Self {
        self.width(Dimension::auto())
    }

    fn h_full(self) -> Self {
        self.height(Dimension::percent(1.))
    }
    fn h_auto(self) -> Self {
        self.height(Dimension::auto())
    }

    fn size_full(self) -> Self {
        self.width(Dimension::percent(1.))
            .height(Dimension::percent(1.))
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

    fn use_hovered(self, hovered: Signal<bool>) -> Self {
        let mut hover_count = 0;
        self.on_hover_event(move |event| match event.action() {
            PointerAction::Down => {
                hover_count += 1;
                hovered.set(true);
            }
            PointerAction::Move => {}
            PointerAction::Up | PointerAction::Cancel => {
                hover_count -= 1;
                if hover_count == 0 {
                    hovered.set(false);
                }
            }
        })
    }
}

impl<T: Styleable> StyleableExt for T {}
