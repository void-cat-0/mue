mod ext;
#[cfg(test)]
#[path = "style/layout_reference.rs"]
mod layout_reference;
#[cfg(test)]
#[path = "style/layout_tests.rs"]
mod layout_tests;
#[cfg(test)]
#[path = "style/lazy_tests.rs"]
mod lazy_tests;
mod transform;
pub use transform::{IntoTransformProp, Transform, TransformOp, TransformOrigin, TransformProp};
pub mod units;

pub use units::{FontSize, FontSizeExt, Size, SizeExt, percent, px, vh, vw};

use crate::{color::Color, shader::Brush};
use cosmic_text::Align;
use cosmic_text::{FamilyOwned, Weight};
pub use ext::StyleableExt;

use mue_core::{
    effect::computed,
    prop::Prop,
    signal::{Access, ReadSignal},
};
use paste::paste;
use std::{mem, rc::Rc};

#[cfg(test)]
use mue_core::scope::OwnedScope;
#[cfg(test)]
use std::cell::Cell;
use taffy::{BoxSizing, Display, Overflow, Point, Position};

use crate::{
    event::{pointer::PointerEvent, wheel::WheelEvent},
    gesture::{PanEvent, PinchEvent, RotateEvent, SwipeEvent, TransformEvent},
    hook::{CallbackList, NodeHooks},
    math::Matrix,
    node::{ChildrenPlan, HoverOwner, IntoChildren},
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
    (@setter $(#[$doc:meta])* transform_list, $ty:ty) => {
        $(#[$doc])*
        fn transform_list(mut self, value: impl Into<Prop<Transform>>) -> Self {
            self.style_mut().transform_list = Some(value.into());
            self.style_mut().matrix_transform = Some(Prop::Static(Matrix::IDENTITY));
            self
        }
    };
    (@setter $(#[$doc:meta])* line_height, $ty:ty) => {
        $(#[$doc])*
        fn line_height(mut self, value: impl Into<Prop<Size>>) -> Self {
            self.style_mut().line_height = Some(value.into());
            self.style_mut().line_height_multiplier = Some(Prop::Static(None));
            self
        }
    };
    (@setter $(#[$doc:meta])* $name:ident, $ty:ty) => {
        $(#[$doc])*
        fn $name(mut self, value: impl Into<Prop<$ty>>) -> Self {
            self.style_mut().$name = Some(value.into());
            self
        }
    };
    (@accessor $field:ident, $ty:ty, $($default:expr)?) => {
        paste! {
            /// Return the explicitly declared property, if any.
            pub fn [<declared_ $field>](&self) -> Option<Prop<$ty>> {
                self.$field.clone()
            }
            #[allow(dead_code)]
            pub fn [<take_ $field>](&mut self) -> Prop<$ty> {
                self.$field.take().unwrap_or_else(|| define_style!(@extract_default ($ty, $($default)?)))
            }
        }
    };
    (@rect_defaults $target:ident, $source:ident, $rect:ident) => {
        paste! {
            define_style!(@defaults $target, $source, [<$rect _left>], [<$rect _right>], [<$rect _top>], [<$rect _bottom>]);
        }
    };
    (@defaults $target:ident, $source:ident, $($field:ident),*) => {
        $(if $target.$field.is_none() { $target.$field = $source.$field.take(); })*
    };
    (@extract_default_raw ($ty:ty, $default:expr)) => { $default };
    (@extract_default_raw ($ty:ty, )) => { <$ty>::default() };
    (@extract_default ($($rest:tt)*)) => { Prop::Static(define_style!(@extract_default_raw ($($rest)*))) };
    (
        $($(#[$property_doc:meta])* $name:ident : $ty:ty $(= $default:expr)?;)*
        @rects { $($rect_name:ident : taffy::Rect<$rect_ty:ty> $( = $rect_default:expr)?;)* }
        @hooks { $($hook_name:ident : CallbackList<$hook_ty:ty>;)* }
    ) => {
        paste! {
            /// Optional layout, paint, font, child, and event properties for a node.
            #[derive(Default)]
            pub struct Style {
                $(pub(crate) $name: Option<Prop<$ty>>,)*
                $(
                    pub(crate) [<$rect_name _left>]: Option<Prop<$rect_ty>>,
                    pub(crate) [<$rect_name _right>]: Option<Prop<$rect_ty>>,
                    pub(crate) [<$rect_name _top>]: Option<Prop<$rect_ty>>,
                    pub(crate) [<$rect_name _bottom>]: Option<Prop<$rect_ty>>,
                )*
                $(pub $hook_name: CallbackList<$hook_ty>,)*
                pub(crate) children: Option<ChildrenPlan>,
                pub(crate) hover_owners: Vec<Rc<HoverOwner>>,
            }

            /// Fluent styles accepting either static properties or reactive signals.
            /// Length literals and reactive f32 values are pixels.
            pub trait Styleable: Sized {
                fn style_mut(&mut self) -> &mut Style;

                /// Set a matrix or transform list, including projected 3D operations.
                fn transform(mut self, value: impl IntoTransformProp) -> Self {
                    match value.into_transform_prop() {
                        TransformProp::Matrix(value) => {
                            self.style_mut().matrix_transform = Some(value);
                            self.style_mut().transform_list = Some(Prop::Static(Transform::default()));
                        }
                        TransformProp::Functions(value) => {
                            self.style_mut().transform_list = Some(value);
                            self.style_mut().matrix_transform = Some(Prop::Static(Matrix::IDENTITY));
                        }
                    }
                    self
                }

                /// Apply a style, replacing properties explicitly set by it.
                fn style(mut self, mut style: Style) -> Self {
                    style.provide_defaults(mem::take(self.style_mut()));
                    *self.style_mut() = style;
                    self
                }
                $(define_style!(@setter $(#[$property_doc])* $name, $ty);)*
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
                        let value = value.into();
                        self.style_mut().[<$rect_name _left>] = Some(value.clone());
                        self.style_mut().[<$rect_name _right>] = Some(value);
                        self
                    }
                    fn [<$rect_name _y>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        let value = value.into();
                        self.style_mut().[<$rect_name _top>] = Some(value.clone());
                        self.style_mut().[<$rect_name _bottom>] = Some(value);
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
                /// Replace deferred child input, mounted later with the node.
                fn children(mut self, children: impl IntoChildren) -> Self {
                    self.style_mut().children = Some(children.into_children_plan());
                    self
                }
            }
            impl Styleable for Style { fn style_mut(&mut self) -> &mut Style { self } }

            impl Style {
                pub(crate) fn bind_explicit_transitions(&mut self, transition: Prop<crate::transition::Transition>) {
                    $(if let Some(value) = self.$name.take() {
                        self.$name = Some(crate::transition::bind_property!($name, value, Some(transition)));
                    })*
                    $(
                        if let Some(value) = self.[<$rect_name _left>].take() {
                            self.[<$rect_name _left>] = Some(crate::transition::bind_property!([<$rect_name _left>], value, Some(transition)));
                        }
                        if let Some(value) = self.[<$rect_name _right>].take() {
                            self.[<$rect_name _right>] = Some(crate::transition::bind_property!([<$rect_name _right>], value, Some(transition)));
                        }
                        if let Some(value) = self.[<$rect_name _top>].take() {
                            self.[<$rect_name _top>] = Some(crate::transition::bind_property!([<$rect_name _top>], value, Some(transition)));
                        }
                        if let Some(value) = self.[<$rect_name _bottom>].take() {
                            self.[<$rect_name _bottom>] = Some(crate::transition::bind_property!([<$rect_name _bottom>], value, Some(transition)));
                        }
                    )*
                }
                /// Fill missing properties and prepend the default hooks.
                pub fn provide_defaults(&mut self, mut default: Self) {
                    define_style!(@defaults self, default, $($name),*);
                    $(define_style!(@rect_defaults self, default, $rect_name);)*
                    $(
                        mem::swap(&mut self.$hook_name, &mut default.$hook_name);
                        self.$hook_name.extend(mem::take(&mut default.$hook_name));
                    )*
                    if self.children.is_none() { self.children = default.children.take(); }
                    self.append_hover_owners(mem::take(&mut default.hover_owners));
                }
                $(define_style!(@accessor $name, $ty, $($default)?);)*
                $(
                    define_style!(@accessor [<$rect_name _left>], $rect_ty, $($rect_default)?);
                    define_style!(@accessor [<$rect_name _right>], $rect_ty, $($rect_default)?);
                    define_style!(@accessor [<$rect_name _top>], $rect_ty, $($rect_default)?);
                    define_style!(@accessor [<$rect_name _bottom>], $rect_ty, $($rect_default)?);
                    #[allow(dead_code)]
                    pub fn [<take_ $rect_name>](&mut self) -> Prop<taffy::Rect<$rect_ty>> {
                        if self.[<$rect_name _left>].is_none()
                            && self.[<$rect_name _right>].is_none()
                            && self.[<$rect_name _top>].is_none()
                            && self.[<$rect_name _bottom>].is_none()
                        {
                            let default = define_style!(@extract_default_raw ($rect_ty, $($rect_default)?));
                            Prop::Static(taffy::Rect { left: default.clone(), right: default.clone(), top: default.clone(), bottom: default })
                        } else {
                            let left = self.[<take_ $rect_name _left>]();
                            let right = self.[<take_ $rect_name _right>]();
                            let top = self.[<take_ $rect_name _top>]();
                            let bottom = self.[<take_ $rect_name _bottom>]();
                            computed(move |_| taffy::Rect { left: left.get_clone(), right: right.get_clone(), top: top.get_clone(), bottom: bottom.get_clone() }).into()
                        }
                    }
                )*
                $(
                    #[allow(dead_code)]
                    pub(crate) fn [<take_ $hook_name>](&mut self) -> CallbackList<$hook_ty> { mem::take(&mut self.$hook_name) }
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
    width: Size = Size::Auto;
    /// Set the preferred height; numbers are pixels and the default is auto.
    height: Size = Size::Auto;

    /// Set a lower bound on width.
    min_width: Size = Size::Auto;
    /// Set a lower bound on height.
    min_height: Size = Size::Auto;

    /// Set an upper bound on width.
    max_width: Size = Size::Auto;
    /// Set an upper bound on height.
    max_height: Size = Size::Auto;

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
    column_gap: Size = Size::Px(0.);
    row_gap: Size = Size::Px(0.);
    /// Preferred width divided by height, when one dimension is automatic.
    aspect_ratio: Option<f32>;

    flex_basis: Size = Size::Auto;
    /// Set how much of the parent's remaining main-axis space this node takes.
    flex_grow: f32 = 0.0;
    /// Set how much this node shrinks when flex children exceed available space.
    flex_shrink: f32 = 1.0;

    transition: crate::transition::Transition = crate::transition::Transition::Instant;

    // Paint
      /// Transform this node and its descendants without changing layout size.
    matrix_transform: Matrix = Matrix::IDENTITY;
    /// Transform functions, applied in order around the border-box origin.
    transform_list: Transform;
    transform_origin: TransformOrigin;
    /// Multiply painted alpha; 0 is transparent and 1 is opaque.
    opacity: f32 = 1.0;
    /// Paint the background with a solid color, gradient, or texture brush.
    background: Brush = Brush::default();
    /// Set the foreground brush for text and image tint.
    color: Brush = Brush::Solid(Color::WHITE);
    /// Round background corners by this radius in pixels.
    radius: Size = Size::Px(0.);

    // Text
    /// Align shaped lines within the text node's content width.
    text_align: Option<Align>;
    /// Set glyph size; percent/em use the parent computed size, rem uses the root.
    /// Unspecified sizes inherit; the root defaults to 32 logical pixels.
    font_size: FontSize = FontSize::Em(1.);
    /// Set baseline spacing in pixels.
    line_height: Size = Size::Px(44.);
    line_height_multiplier: Option<f32>;
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
    text_wrap_mode: crate::node::text::TextWrap;
    text_overflow: crate::node::text::TextOverflow;
    /// Maximum visual lines. None displays the whole document.
    text_max_lines: Option<usize>;
    text_vertical_align: crate::node::text::TextVerticalAlign;
    /// Extra space between newline-delimited paragraphs, in logical pixels.
    paragraph_spacing: Size = Size::Px(0.);
    letter_spacing: Size = Size::Px(0.);
    tab_width: u16 = 4;

    @rects {
        margin: taffy::Rect<Size> = Size::Px(0.);
        padding: taffy::Rect<Size> = Size::Px(0.);
        border: taffy::Rect<Size> = Size::Px(0.);
        inset: taffy::Rect<Size> = Size::Auto;
    }

    @hooks {
        on_render: CallbackList<()>;
        on_pointer_event: CallbackList<PointerEvent>;
        on_hover_event: CallbackList<PointerEvent>;
        on_wheel_event: CallbackList<WheelEvent>;

        on_tap: CallbackList<()>;
        on_long_press: CallbackList<()>;
        on_pan: CallbackList<PanEvent>;
        on_swipe: CallbackList<SwipeEvent>;
        on_pinch: CallbackList<PinchEvent>;
        on_rotate: CallbackList<RotateEvent>;
        on_transform: CallbackList<TransformEvent>;
    }
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    /// Peek at the deferred input without mounting children.
    #[allow(dead_code)]
    pub(crate) fn has_children_plan(&self) -> bool {
        self.children.is_some()
    }

    /// Take the deferred input for mounting inside the node's scope.
    pub(crate) fn take_children_plan(&mut self) -> Option<ChildrenPlan> {
        self.children.take()
    }

    pub(crate) fn ensure_hovered(&mut self) -> Rc<HoverOwner> {
        if let Some(owner) = self.hover_owners.first() {
            return owner.clone();
        }
        let owner = HoverOwner::new();
        self.hover_owners.push(owner.clone());
        owner
    }

    pub(crate) fn build_hooks(&mut self) -> NodeHooks {
        NodeHooks {
            render: self.take_on_render(),
            after_children: CallbackList::default(),
            pointer_event: self.take_on_pointer_event(),
            hover_event: self.take_on_hover_event(),
            wheel_event: self.take_on_wheel_event(),
        }
    }

    pub(crate) fn build_taffy(&mut self) -> ReadSignal<taffy::Style> {
        let display = self.take_display();
        let box_sizing = self.take_box_sizing();
        let aspect_ratio = self.take_aspect_ratio();
        let column_gap = self.take_column_gap();
        let row_gap = self.take_row_gap();

        let width = self.take_width();
        let height = self.take_height();
        let min_width = self.take_min_width();
        let min_height = self.take_min_height();
        let max_width = self.take_max_width();
        let max_height = self.take_max_height();

        let overflow_x = self.take_overflow_x();
        let overflow_y = self.take_overflow_y();

        let margin_left = self.take_margin_left();
        let margin_right = self.take_margin_right();
        let margin_top = self.take_margin_top();
        let margin_bottom = self.take_margin_bottom();
        let padding_left = self.take_padding_left();
        let padding_right = self.take_padding_right();
        let padding_top = self.take_padding_top();
        let padding_bottom = self.take_padding_bottom();
        let border_left = self.take_border_left();
        let border_right = self.take_border_right();
        let border_top = self.take_border_top();
        let border_bottom = self.take_border_bottom();

        let align_items = self.take_align_items();
        let align_self = self.take_align_self();
        let justify_items = self.take_justify_items();
        let justify_self = self.take_justify_self();
        let align_content = self.take_align_content();
        let justify_content = self.take_justify_content();

        let position = self.take_position();
        let inset_left = self.take_inset_left();
        let inset_right = self.take_inset_right();
        let inset_top = self.take_inset_top();
        let inset_bottom = self.take_inset_bottom();

        let flex_direction = self.take_flex_direction();
        let flex_wrap = self.take_flex_wrap();

        let flex_basis = self.take_flex_basis();
        let flex_grow = self.take_flex_grow();
        let flex_shrink = self.take_flex_shrink();

        computed(move |_| {
            #[cfg(test)]
            crate::layout::record_style_compute();
            taffy::Style {
                display: display.get(),
                box_sizing: box_sizing.get(),
                aspect_ratio: aspect_ratio.get(),
                gap: taffy::Size {
                    width: column_gap.get().to_length_percentage("gap"),
                    height: row_gap.get().to_length_percentage("gap"),
                },

                size: taffy::Size {
                    width: width.get().to_dimension(),
                    height: height.get().to_dimension(),
                },
                min_size: taffy::Size {
                    width: min_width.get().to_dimension(),
                    height: min_height.get().to_dimension(),
                },
                max_size: taffy::Size {
                    width: max_width.get().to_dimension(),
                    height: max_height.get().to_dimension(),
                },

                overflow: Point {
                    x: overflow_x.get(),
                    y: overflow_y.get(),
                },

                margin: taffy::Rect {
                    left: margin_left.get().to_length_percentage_auto(),
                    right: margin_right.get().to_length_percentage_auto(),
                    top: margin_top.get().to_length_percentage_auto(),
                    bottom: margin_bottom.get().to_length_percentage_auto(),
                },
                padding: taffy::Rect {
                    left: padding_left.get().to_length_percentage("padding"),
                    right: padding_right.get().to_length_percentage("padding"),
                    top: padding_top.get().to_length_percentage("padding"),
                    bottom: padding_bottom.get().to_length_percentage("padding"),
                },
                border: taffy::Rect {
                    left: border_left.get().to_length_percentage("border"),
                    right: border_right.get().to_length_percentage("border"),
                    top: border_top.get().to_length_percentage("border"),
                    bottom: border_bottom.get().to_length_percentage("border"),
                },

                align_items: align_items.get(),
                align_self: align_self.get(),
                justify_items: justify_items.get(),
                justify_self: justify_self.get(),
                align_content: align_content.get(),
                justify_content: justify_content.get(),

                position: position.get(),
                inset: taffy::Rect {
                    left: inset_left.get().to_length_percentage_auto(),
                    right: inset_right.get().to_length_percentage_auto(),
                    top: inset_top.get().to_length_percentage_auto(),
                    bottom: inset_bottom.get().to_length_percentage_auto(),
                },

                flex_direction: flex_direction.get(),
                flex_wrap: flex_wrap.get(),

                flex_basis: flex_basis.get().to_dimension(),
                flex_grow: flex_grow.get(),
                flex_shrink: flex_shrink.get(),

                ..Default::default()
            }
        })
    }
}

impl Style {
    fn append_hover_owners(&mut self, owners: Vec<Rc<HoverOwner>>) {
        for owner in owners {
            if !self
                .hover_owners
                .iter()
                .any(|existing| Rc::ptr_eq(existing, &owner))
            {
                self.hover_owners.push(owner);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use mue_core::signal::signal;

    use super::*;
    #[test]
    fn hover_configuration_reuses_owner_and_transfers_derived_lifetimes() {
        let external_scope = OwnedScope::new();
        let external = external_scope.run(|| signal(1.));
        let configured_scope = OwnedScope::new();
        let (style, hovered) = configured_scope.run(|| {
            let mut observed = None;
            let style = Style::new().with_hovered(|style, hovered| {
                observed = Some(hovered);
                style.opacity(computed(
                    move |_| if hovered.get() { external.get() } else { 0.5 },
                ))
            });
            (style, observed.unwrap())
        });
        drop(configured_scope);
        let mut style = style.style(Style::new().height(10.));
        let owner = style.ensure_hovered();
        assert_eq!(style.hover_owners.len(), 1);
        assert!(Rc::ptr_eq(&owner, &style.ensure_hovered()));
        let opacity = style.take_opacity();
        assert_eq!(opacity.get(), 0.5);
        owner.signal.set(true);
        assert!(hovered.get());
        assert_eq!(opacity.get(), 1.);
        external.set(0.75);
        assert_eq!(opacity.get(), 0.75);
        drop(owner);
        drop(style);
        assert!(hovered.is_disposed());
        match opacity {
            Prop::Dynamic(value) => assert!(value.is_disposed()),
            Prop::Static(_) => panic!("hover-derived property must be reactive"),
        }
        assert!(!external.is_disposed());
    }

    #[test]
    fn combining_styles_retains_all_hover_owners_without_duplicates() {
        let mut outer = Style::new();
        let first = outer.ensure_hovered();
        let mut inner = Style::new();
        let second = inner.ensure_hovered();
        inner.hover_owners.push(first.clone());
        let mut merged = outer;
        merged.provide_defaults(inner);
        assert_eq!(merged.hover_owners.len(), 2);
        assert!(Rc::ptr_eq(&first, &merged.ensure_hovered()));
        let mut defaults = Style::new();
        let third = defaults.ensure_hovered();
        defaults.hover_owners.push(second.clone());
        merged.provide_defaults(defaults);
        assert_eq!(merged.hover_owners.len(), 3);
        let signals = [*first.signal, *second.signal, *third.signal];
        drop((first, second, third));
        assert!(signals.iter().all(|value| !value.is_disposed()));
        drop(merged);
        assert!(signals.iter().all(ReadSignal::is_disposed));
    }

    #[test]
    fn child_setter_defers_build_and_replaces_old_plan() {
        let builds = Rc::new(Cell::new(0));
        let build = builds.clone();
        let mut style = Style::new().children(ChildrenPlan::new(move || {
            build.set(build.get() + 1);
            ().into_children()
        }));
        assert_eq!(builds.get(), 0);
        style = style.children(());
        drop(style);
        assert_eq!(builds.get(), 0);
    }
}

/// Enumerations used by style declarations without requiring direct Taffy dependencies.
pub mod values {
    pub use cosmic_text::Align as TextAlign;
    pub use taffy::{
        AlignContent, AlignItems, BoxSizing, Display, FlexDirection, FlexWrap, Overflow, Position,
    };
}
