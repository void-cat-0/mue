mod ext;
pub mod units;

pub use units::{IntoLength, IntoLengthValue, Percent, Px, Vh, Vw, percent, px, vh, vw};

use crate::{color::Color, shader::Brush};
use cosmic_text::Align;
use cosmic_text::{FamilyOwned, Weight};
pub use ext::StyleableExt;

use mue_core::{
    Owned,
    effect::{computed, computed_always},
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
    U: IntoLength<T>,
    T: Clone,
{
    fn into_rect_prop(self) -> taffy::Rect<Prop<T>> {
        let prop = self.0.into_length();
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
    U1: IntoLength<T>,
    U2: IntoLength<T>,
    T: Clone,
{
    fn into_rect_prop(self) -> taffy::Rect<Prop<T>> {
        let x = self.0.into_length();
        let y = self.1.into_length();
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
    U1: IntoLength<T>,
    U2: IntoLength<T>,
    U3: IntoLength<T>,
    U4: IntoLength<T>,
    T: Clone,
{
    fn into_rect_prop(self) -> taffy::Rect<Prop<T>> {
        taffy::Rect {
            left: self.0.into_length(),
            top: self.1.into_length(),
            bottom: self.2.into_length(),
            right: self.3.into_length(),
        }
    }
}

macro_rules! define_style {
    (@setter [length] [$(#[$doc:meta])*] $name:ident, $ty:ty) => {
        $(#[$doc])*
        fn $name(mut self, value: impl IntoLengthValue) -> Self {
            self.style_mut().$name = Some(value.into_length_value());
            self
        }
    };
    (@setter [layout] [$(#[$doc:meta])*] $name:ident, $ty:ty) => {
        $(#[$doc])*
        fn $name(mut self, value: impl IntoLength<$ty>) -> Self {
            self.style_mut().$name = Some(value.into_length());
            self
        }
    };
    (@setter [] [$(#[$doc:meta])*] $name:ident, $ty:ty) => {
        $(#[$doc])*
        fn $name(mut self, value: impl Into<Prop<$ty>>) -> Self {
            self.style_mut().$name = Some(value.into());
            self
        }
    };
    (@themed_property $result:ident, $theme:ident, $defaults:ident, $field:ident, $ty:ty, $($default:expr)?) => {
        let fallback = $defaults.$field.take();
        $result.$field = Some(computed_always(move |_| {
            let current = $theme.get_clone();
            current.$field.as_ref().or(fallback.as_ref())
                .map(|value| value.get_clone())
                .unwrap_or_else(|| define_style!(@extract_default_raw ($ty, $($default)?)))
        }).into());
    };
    // Rect properties already have separate fields: fill only the missing sides.
    (@rect_defaults $target:ident, $source:ident, $rect:ident) => {
        paste! {
            define_style!(@defaults $target, $source, [<$rect _left>], [<$rect _right>], [<$rect _top>], [<$rect _bottom>]);
        }
    };
    (@defaults $target:ident, $source:ident, $($field:ident),*) => {
        $(if $target.$field.is_none() { $target.$field = $source.$field; })*
    };
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
        $($(#[$property_doc:meta])* $name:ident : $ty:ty $(= $default:expr)? $(=> $kind:ident)?;)*
        @rects {
            $($rect_name:ident : taffy::Rect<$rect_ty:ty> $( = $rect_default:expr)?;)*
        }
        @hooks {
            $($hook_name:ident : HookFn<$hook_ty:ty>;)*
        }
    ) => {
        paste! {
            #[derive(Default)]
            /// Optional layout, paint, font, and event properties for a node.
            pub struct Style {
                pub(crate) animation: Option<crate::animation::Animation>,
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

            /// Fluent styles accepting either static properties or reactive signals.
            /// Length literals and reactive `f32` values are pixels.
            pub trait Styleable: Sized {
                fn style_mut(&mut self) -> &mut Style;

                /// Animate changes to this node's interpolatable visual properties.
                fn animation(mut self, animation: crate::animation::Animation) -> Self {
                    self.style_mut().animation = Some(animation);
                    self
                }

                $(
                    define_style!(@setter [$($kind)?] [$(#[$property_doc])*] $name, $ty);
                )*

                $(
                    fn [<$rect_name _left>](mut self, value: impl IntoLength<$rect_ty>) -> Self {
                        self.style_mut().[<$rect_name _left>] = Some(value.into_length());
                        self
                    }
                    fn [<$rect_name _right>](mut self, value: impl IntoLength<$rect_ty>) -> Self {
                        self.style_mut().[<$rect_name _right>] = Some(value.into_length());
                        self
                    }
                    fn [<$rect_name _top>](mut self, value: impl IntoLength<$rect_ty>) -> Self {
                        self.style_mut().[<$rect_name _top>] = Some(value.into_length());
                        self
                    }
                    fn [<$rect_name _bottom>](mut self, value: impl IntoLength<$rect_ty>) -> Self {
                        self.style_mut().[<$rect_name _bottom>] = Some(value.into_length());
                        self
                    }
                    fn [<$rect_name _x>](mut self, value: impl IntoLength<$rect_ty>) -> Self {
                        let prop = value.into_length();
                        self.style_mut().[<$rect_name _left>] = Some(prop.clone());
                        self.style_mut().[<$rect_name _right>] = Some(prop.clone());
                        self
                    }
                    fn [<$rect_name _y>](mut self, value: impl IntoLength<$rect_ty>) -> Self {
                        let prop = value.into_length();
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

                /// Replace the node's children. Factory content is passed to `create`.
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
                pub(crate) fn themed_defaults(theme: ReadSignal<std::rc::Rc<Style>>, mut default: Self) -> Self {
                    let mut result = Self::new();
                    $(
                        define_style!(@themed_property result, theme, default, $name, $ty, $($default)?);
                    )*
                    $(
                        define_style!(@themed_property result, theme, default, [<$rect_name _left>], $rect_ty, $($rect_default)?);
                        define_style!(@themed_property result, theme, default, [<$rect_name _right>], $rect_ty, $($rect_default)?);
                        define_style!(@themed_property result, theme, default, [<$rect_name _top>], $rect_ty, $($rect_default)?);
                        define_style!(@themed_property result, theme, default, [<$rect_name _bottom>], $rect_ty, $($rect_default)?);
                    )*
                    result.provide_defaults(default);
                    result
                }
                /// Fill missing properties from `other` and append its event hooks.
                pub fn merge(mut self, other: Self) -> Self {
                    self.animation = self.animation.or(other.animation);
                    $(
                        self.$name = self.$name.or(other.$name);
                    )*
                    $(
                        define_style!(@rect_defaults self, other, $rect_name);
                    )*
                    $(
                        self.$hook_name.extend(other.$hook_name);
                    )*
                    self.children = self.children.or(other.children);
                    self
                }

                /// Fill missing properties and prepend default event hooks.
                pub fn provide_defaults(&mut self, mut default: Self) {
                    self.animation = self.animation.or(default.animation);
                    $(
                        if self.$name.is_none() {
                            self.$name = default.$name;
                        }
                    )*
                    $(
                        define_style!(@rect_defaults self, default, $rect_name);
                    )*
                    $(
                        mem::swap(&mut self.$hook_name, &mut default.$hook_name);
                        self.$hook_name.extend(default.$hook_name);
                    )*
                    if self.children.is_none() {
                        self.children = default.children;
                    }
                }

                pub(crate) fn assert_visual_only(&self) {
                    assert!(self.children.is_none() $( && self.$hook_name.is_empty() )*,
                        "theme and factory style callbacks may only set visual properties");
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

    /// Set the preferred width; numbers are pixels and the default is auto.
    width: Dimension = Dimension::auto() => layout;
    /// Set the preferred height; numbers are pixels and the default is auto.
    height: Dimension = Dimension::auto() => layout;

    /// Set a lower bound on width.
    min_width: Dimension = Dimension::auto() => layout;
    /// Set a lower bound on height.
    min_height: Dimension = Dimension::auto() => layout;

    /// Set an upper bound on width.
    max_width: Dimension = Dimension::auto() => layout;
    /// Set an upper bound on height.
    max_height: Dimension = Dimension::auto() => layout;

    overflow_x: Overflow = Overflow::Visible;
    overflow_y: Overflow = Overflow::Visible;

    /// Align children along the flex cross axis.
    align_items: Option<taffy::AlignItems>;
    align_self: Option<taffy::AlignSelf>;
    justify_items: Option<taffy::AlignItems>;
    justify_self: Option<taffy::AlignSelf>;
    align_content: Option<taffy::AlignContent>;
    /// Distribute children along the flex main axis.
    justify_content: Option<taffy::JustifyContent>;

    /// Choose normal relative positioning or absolute positioning with insets.
    position: Position;

    /// Choose the main axis and ordering for flex children.
    flex_direction: taffy::FlexDirection;
    flex_wrap: taffy::FlexWrap;

    /// Horizontal space between columns and vertical space between rows.
    column_gap: LengthPercentage = LengthPercentage::ZERO => layout;
    row_gap: LengthPercentage = LengthPercentage::ZERO => layout;
    /// Preferred width divided by height, when one dimension is automatic.
    aspect_ratio: Option<f32>;

    flex_basis: Dimension = Dimension::auto() => layout;
    /// Set how much of the parent's remaining main-axis space this node takes.
    flex_grow: f32 = 0.0;
    /// Set how much this node shrinks when flex children exceed available space.
    flex_shrink: f32 = 1.0;

    // Paint
      /// Transform this node and its descendants without changing layout size.
    transform: Matrix = Matrix::IDENTITY;
    /// Multiply painted alpha; 0 is transparent and 1 is opaque.
    opacity: f32 = 1.0;
    /// Paint the background with a solid color, gradient, or texture brush.
    background: Brush = Brush::default();
    /// Set the foreground brush for text and image tint.
    color: Brush = Brush::Solid(Color::WHITE);
    /// Round background corners by this radius in pixels.
    radius: f32 = 0.0 => length;

    // Text
    /// Align shaped lines within the text node's content width.
    text_align: Option<Align>;
    /// Set glyph size in pixels, including document spans without overrides.
    font_size: f32 = 32.0 => length;
    /// Set baseline spacing in pixels.
    line_height: f32 = 44.0 => length;
    /// Choose a registered font family or a generic family.
    font_family: FamilyOwned = FamilyOwned::SansSerif;
    /// Choose a font weight; the matching face must be registered or installed.
    font_weight: Weight = Weight::NORMAL;
    /// Use italic font faces where available.
    font_italic: bool = false;
    /// Wrap long lines at words or glyphs. Disable for preformatted code.
    text_wrap: bool = true;
    /// Underline each shaped line with the foreground brush.
    text_underline: bool = false;
    text_strikethrough: bool = false;
    text_wrap_mode: crate::node::TextWrap;
    text_overflow: crate::node::TextOverflow;
    /// Maximum visual lines. None displays the whole document.
    text_max_lines: Option<usize>;
    text_vertical_align: crate::node::TextVerticalAlign;
    /// Extra space between newline-delimited paragraphs, in logical pixels.
    paragraph_spacing: f32 = 0. => length;
    letter_spacing: f32 = 0. => length;
    tab_width: u16 = 4;

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
            after_children: HookFn::default(),
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
        let box_sizing = self.take_box_sizing();
        let aspect_ratio = self.take_aspect_ratio();
        let gap = to_size(self.take_column_gap(), self.take_row_gap());

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
            box_sizing: box_sizing.get(),
            aspect_ratio: aspect_ratio.get(),
            gap: gap.get(),

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

#[cfg(test)]
mod tests {
    use super::{Style, Styleable, percent, px, vh, vw};

    #[test]
    fn styleable_accepts_explicit_units_for_layout_and_scalar_lengths() {
        let _style = Style::new()
            .width(px(120.0))
            .height(percent(50.0))
            .min_width(vw(10.0))
            .max_height(vh(25.0))
            .column_gap(px(8.0))
            .row_gap(percent(2.0))
            .padding((px(1.0), vw(2.0)))
            .margin((vh(1.0), px(2.0), percent(3.0), vw(4.0)))
            .inset_x(px(5.0))
            .font_size(px(18.0))
            .line_height(vh(4.0))
            .letter_spacing(px(0.5))
            .paragraph_spacing(vw(1.0));
    }
}
