mod ext;

use cosmic_text::Align;
pub use ext::StyleableExt;

use mue_core::{
    Owned,
    effect::computed,
    prop::Prop,
    signal::{Access, ReadSignal},
};
use paste::paste;
use std::mem;
use taffy::{
    BoxSizing, Dimension, Display, LengthPercentage, LengthPercentageAuto, Overflow, Point,
    Position, prelude::TaffyZero,
};

use crate::{
    event::{pointer::PointerEvent, wheel::WheelEvent},
    gesture::{PanEvent, PinchEvent, RotateEvent, SwipeEvent},
    hook::{HookFn, NodeHooks},
    math::Matrix,
    node::{Children, IntoChildren},
};

pub trait IntoRectProp<T> {
    fn into_rect_prop(self) -> taffy::Rect<Prop<T>>;
}
impl<U, T> IntoRectProp<T> for (U,)
where
    U: Into<Prop<T>>,
    T: Clone,
{
    fn into_rect_prop(self) -> taffy::Rect<Prop<T>> {
        let prop = self.0.into();
        taffy::Rect {
            left: prop.clone(),
            right: prop.clone(),
            top: prop.clone(),
            bottom: prop,
        }
    }
}
impl<U1, U2, T> IntoRectProp<T> for (U1, U2)
where
    U1: Into<Prop<T>>,
    U2: Into<Prop<T>>,
    T: Clone,
{
    fn into_rect_prop(self) -> taffy::Rect<Prop<T>> {
        let x = self.0.into();
        let y = self.1.into();
        taffy::Rect {
            left: x.clone(),
            right: x,
            top: y.clone(),
            bottom: y,
        }
    }
}
impl<U1, U2, U3, U4, T> IntoRectProp<T> for (U1, U2, U3, U4)
where
    U1: Into<Prop<T>>,
    U2: Into<Prop<T>>,
    U3: Into<Prop<T>>,
    U4: Into<Prop<T>>,
    T: Clone,
{
    fn into_rect_prop(self) -> taffy::Rect<Prop<T>> {
        taffy::Rect {
            left: self.0.into(),
            top: self.1.into(),
            bottom: self.2.into(),
            right: self.3.into(),
        }
    }
}

macro_rules! define_style {
    (@extract_default_raw ($ty:ty, $default:expr)) => {
        $default
    };

    (@extract_default_raw ($ty:ty, )) => {
        <$ty>::default()
    };

    (@extract_default ($($rest:tt)*)) => {
        Prop::Static(define_style!(@extract_default_raw ($($rest)*)))
    };

    (
        $($name:ident : $ty:ty $( = $default:expr)?;)*
        @rects {
            $($rect_name:ident : taffy::Rect<$rect_ty:ty> $( = $rect_default:expr)?;)*
        }
        @hooks {
            $($hook_name:ident : HookFn<$hook_ty:ty>;)*
        }
    ) => {
        paste! {
            #[derive(Default)]
            pub struct Style {
                $(pub(crate) $name: Option<Prop<$ty>>,)*
                $(
                    pub(crate) [<$rect_name _left>]: Option<Prop<$rect_ty>>,
                    pub(crate) [<$rect_name _right>]: Option<Prop<$rect_ty>>,
                    pub(crate) [<$rect_name _top>]: Option<Prop<$rect_ty>>,
                    pub(crate) [<$rect_name _bottom>]: Option<Prop<$rect_ty>>,
                )*
                $(pub $hook_name: HookFn<$hook_ty>,)*
                pub(crate) children: Option<Owned<Children>>,
            }

            pub trait Styleable: Sized {
                fn style_mut(&mut self) -> &mut Style;

                $(
                    fn $name(mut self, value: impl Into<Prop<$ty>>) -> Self {
                        self.style_mut().$name = Some(value.into());
                        self
                    }
                )*

                $(
                    fn [<$rect_name _left>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        self.style_mut().[<$rect_name _left>] = Some(value.into());
                        self
                    }
                    fn [<$rect_name _right>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        self.style_mut().[<$rect_name _right>] = Some(value.into());
                        self
                    }
                    fn [<$rect_name _top>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        self.style_mut().[<$rect_name _top>] = Some(value.into());
                        self
                    }
                    fn [<$rect_name _bottom>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        self.style_mut().[<$rect_name _bottom>] = Some(value.into());
                        self
                    }
                    fn [<$rect_name _x>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        let prop = value.into();
                        self.style_mut().[<$rect_name _left>] = Some(prop.clone());
                        self.style_mut().[<$rect_name _right>] = Some(prop.clone());
                        self
                    }
                    fn [<$rect_name _y>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        let prop = value.into();
                        self.style_mut().[<$rect_name _top>] = Some(prop.clone());
                        self.style_mut().[<$rect_name _bottom>] = Some(prop);
                        self
                    }
                    fn $rect_name(mut self, value: impl IntoRectProp<$rect_ty>) -> Self {
                        let rect = value.into_rect_prop();
                        self.style_mut().[<$rect_name _left>] = Some(rect.left);
                        self.style_mut().[<$rect_name _right>] = Some(rect.right);
                        self.style_mut().[<$rect_name _top>] = Some(rect.top);
                        self.style_mut().[<$rect_name _bottom>] = Some(rect.bottom);
                        self
                    }
                )*

                $(
                    fn $hook_name(mut self, hook: impl FnMut(&$hook_ty) + 'static) -> Self {
                        self.style_mut().$hook_name.append(hook);
                        self
                    }
                )*

                fn children(mut self, children: impl IntoChildren) -> Self {
                    self.style_mut().children = Some(children.into_children());
                    self
                }
            }

            impl Styleable for Style {
                fn style_mut(&mut self) -> &mut Style {
                    self
                }
            }

            impl Style {
                pub fn merge(mut self, other: Self) -> Self {
                    $(
                        self.$name = self.$name.or(other.$name);
                    )*
                    $(
                        self.$hook_name.extend(other.$hook_name);
                    )*
                    self.children = self.children.or(other.children);
                    self
                }

                pub fn provide_defaults(&mut self, mut default: Self) {
                    $(
                        if self.$name.is_none() {
                            self.$name = default.$name;
                        }
                    )*
                    $(
                        mem::swap(&mut self.$hook_name, &mut default.$hook_name);
                        self.$hook_name.extend(default.$hook_name);
                    )*
                    if self.children.is_none() {
                        self.children = default.children;
                    }
                }

                $(
                    #[allow(dead_code)]
                    pub(crate) fn [<take_ $name>](&mut self) -> Prop<$ty> {
                        self.$name.take().unwrap_or_else(|| define_style!(@extract_default ($ty, $($default)?)))
                    }
                )*

                $(
                    #[allow(dead_code)]
                    pub(crate) fn [<take_ $rect_name>](&mut self) -> Prop<taffy::Rect<$rect_ty>> {
                        if self.[<$rect_name _left>].is_none()
                            && self.[<$rect_name _right>].is_none()
                            && self.[<$rect_name _top>].is_none()
                            && self.[<$rect_name _bottom>].is_none()
                        {
                            let default = define_style!(@extract_default_raw ($rect_ty, $($rect_default)?));
                            taffy::Rect {
                                left: default.clone(),
                                right: default.clone(),
                                top: default.clone(),
                                bottom: default,
                            }
                            .into()
                        } else {
                            let left = self.[<$rect_name _left>].take().unwrap_or_else(|| define_style!(@extract_default ($rect_ty, $($rect_default)?)));
                            let right = self.[<$rect_name _right>].take().unwrap_or_else(|| define_style!(@extract_default ($rect_ty, $($rect_default)?)));
                            let top = self.[<$rect_name _top>].take().unwrap_or_else(|| define_style!(@extract_default ($rect_ty, $($rect_default)?)));
                            let bottom = self.[<$rect_name _bottom>].take().unwrap_or_else(|| define_style!(@extract_default ($rect_ty, $($rect_default)?)));
                            computed(move |_| taffy::Rect {
                                left: left.get_clone(),
                                right: right.get_clone(),
                                top: top.get_clone(),
                                bottom: bottom.get_clone(),
                            })
                            .into()
                        }
                    }
                )*

                $(
                    #[allow(dead_code)]
                    pub(crate) fn [<take_ $hook_name>](&mut self) -> HookFn<$hook_ty> {
                        mem::take(&mut self.$hook_name)
                    }
                )*
            }
        }
    };
}

define_style! {
    // Taffy
    box_sizing: BoxSizing;

    display: Display;

    width: Dimension = Dimension::auto();
    height: Dimension = Dimension::auto();

    min_width: Dimension = Dimension::auto();
    min_height: Dimension = Dimension::auto();

    max_width: Dimension = Dimension::auto();
    max_height: Dimension = Dimension::auto();

    overflow_x: Overflow = Overflow::Visible;
    overflow_y: Overflow = Overflow::Visible;

    align_items: Option<taffy::AlignItems>;
    align_self: Option<taffy::AlignSelf>;
    justify_items: Option<taffy::AlignItems>;
    justify_self: Option<taffy::AlignSelf>;
    align_content: Option<taffy::AlignContent>;
    justify_content: Option<taffy::JustifyContent>;

    position: Position;

    flex_direction: taffy::FlexDirection;
    flex_wrap: taffy::FlexWrap;

    flex_basis: Dimension = Dimension::auto();
    flex_grow: f32 = 0.0;
    flex_shrink: f32 = 1.0;

    // Paint
    transform: Matrix = Matrix::IDENTITY;
    opacity: f32 = 1.0;

    // Text
    text_align: Option<Align>;

    @rects {
        margin: taffy::Rect<LengthPercentageAuto> = LengthPercentageAuto::ZERO;
        padding: taffy::Rect<LengthPercentage> = LengthPercentage::ZERO;
        border: taffy::Rect<LengthPercentage> = LengthPercentage::ZERO;
        inset: taffy::Rect<LengthPercentageAuto> = LengthPercentageAuto::auto();
    }

    @hooks {
        on_render: HookFn<()>;
        on_pointer_event: HookFn<PointerEvent>;
        on_hover_event: HookFn<PointerEvent>;
        on_wheel_event: HookFn<WheelEvent>;

        on_tap: HookFn<()>;
        on_long_press: HookFn<()>;
        on_pan: HookFn<PanEvent>;
        on_swipe: HookFn<SwipeEvent>;
        on_pinch: HookFn<PinchEvent>;
        on_rotate: HookFn<RotateEvent>;
    }
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn build_hooks(&mut self) -> NodeHooks {
        NodeHooks {
            render: self.take_on_render(),
            pointer_event: self.take_on_pointer_event(),
            hover_event: self.take_on_hover_event(),
            wheel_event: self.take_on_wheel_event(),
        }
    }

    pub(crate) fn build_taffy(&mut self) -> ReadSignal<taffy::Style> {
        fn to_size<S>(width: Prop<S>, height: Prop<S>) -> Prop<taffy::Size<S>>
        where
            S: Clone + PartialEq + 'static,
        {
            if let Some((width, height)) = width.as_static().zip(height.as_static()) {
                Prop::Static(taffy::Size {
                    width: width.clone(),
                    height: height.clone(),
                })
            } else {
                computed(move |_| taffy::Size {
                    width: width.get_clone(),
                    height: height.get_clone(),
                })
                .into()
            }
        }

        let display = self.take_display();

        let size = to_size(self.take_width(), self.take_height());
        let min_size = to_size(self.take_min_width(), self.take_min_height());
        let max_size = to_size(self.take_max_width(), self.take_max_height());

        let overflow_x = self.take_overflow_x();
        let overflow_y = self.take_overflow_y();
        let overflow = if let Some((x, y)) = overflow_x.as_static().zip(overflow_y.as_static()) {
            Prop::Static(Point { x: *x, y: *y })
        } else {
            computed(move |_| Point {
                x: overflow_x.get_clone(),
                y: overflow_y.get_clone(),
            })
            .into()
        };

        let margin = self.take_margin();
        let padding = self.take_padding();
        let border = self.take_border();

        let align_items = self.take_align_items();
        let align_self = self.take_align_self();
        let justify_items = self.take_justify_items();
        let justify_self = self.take_justify_self();
        let align_content = self.take_align_content();
        let justify_content = self.take_justify_content();

        let position = self.take_position();
        let inset = self.take_inset();

        let flex_direction = self.take_flex_direction();
        let flex_wrap = self.take_flex_wrap();

        let flex_basis = self.take_flex_basis();
        let flex_grow = self.take_flex_grow();
        let flex_shrink = self.take_flex_shrink();

        computed(move |_| taffy::Style {
            display: display.get(),

            size: size.get(),
            min_size: min_size.get(),
            max_size: max_size.get(),

            overflow: overflow.get(),

            margin: margin.get(),
            padding: padding.get(),
            border: border.get(),

            align_items: align_items.get(),
            align_self: align_self.get(),
            justify_items: justify_items.get(),
            justify_self: justify_self.get(),
            align_content: align_content.get(),
            justify_content: justify_content.get(),

            position: position.get(),
            inset: inset.get(),

            flex_direction: flex_direction.get(),
            flex_wrap: flex_wrap.get(),

            flex_basis: flex_basis.get(),
            flex_grow: flex_grow.get(),
            flex_shrink: flex_shrink.get(),

            ..Default::default()
        })
    }
}
