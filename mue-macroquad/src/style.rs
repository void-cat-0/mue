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
    scope::{OwnedScope, Scope, current_scope},
    signal::{Access, ReadSignal},
};
use paste::paste;
use std::{
    cell::{Cell, OnceCell},
    collections::HashSet,
    mem,
    rc::Rc,
};
use taffy::{BoxSizing, Display, Overflow, Point, Position};

use crate::{
    event::{pointer::PointerEvent, wheel::WheelEvent},
    gesture::{PanEvent, PinchEvent, RotateEvent, SwipeEvent, TransformEvent},
    hook::{HookFn, NodeHooks},
    math::Matrix,
    node::{ChildrenPlan, HoverOwner, IntoChildren},
    theme::ClassName,
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
    (@setter $(#[$doc:meta])* transform, $ty:ty) => {
        $(#[$doc])*
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
    };
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

    (@project $state:ident, color, $ty:ty, $($default:expr)?) => {
        Style::themed_foreground_owned($state.theme, $state.fallback.clone(), $state.inherited_foreground.clone())
    };
    (@project $state:ident, $field:ident, $ty:ty, $($default:expr)?) => {{
        let theme = $state.theme;
        let fallback = $state.fallback.clone();
        computed(move |_| {
            #[cfg(test)] crate::runtime::record_projection();
            let current = theme.get_clone();
            current.$field.as_ref().or(fallback.$field.as_ref())
                .map(|value| value.get_clone())
                .unwrap_or_else(|| define_style!(@extract_default_raw ($ty, $($default)?)))
        }).into()
    }};
    (@animate $state:ident, $value:expr, $field:ident) => {
        crate::transition::bind_property!($field, $value, $state.animation_curve.get())
    };
    (@side_accessor $field:ident, $ty:ty, $($default:expr)?) => {
        paste! {
            pub fn [<declared_ $field>](&self) -> Option<Prop<$ty>> {
                self.$field.clone().or_else(|| self.themed.as_ref().map(|state| {
                    state.$field.get_or_init(|| state.scope.run(|| {
                        let value = define_style!(@project state, $field, $ty, $($default)?);
                        define_style!(@animate state, value, $field)
                    })).clone()
                }))
            }
            #[allow(dead_code)]
            pub fn [<take_ $field>](&mut self) -> Prop<$ty> {
                if self.themed.is_some() { return self.[<declared_ $field>]().unwrap(); }
                self.$field.take().unwrap_or_else(|| define_style!(@extract_default ($ty, $($default)?)))
            }
        }
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
        $($(#[$property_doc:meta])* $name:ident : $ty:ty $(= $default:expr)?;)*
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

                $(pub(crate) $name: Option<Prop<$ty>>,)*
                $(
                    pub(crate) [<$rect_name _left>]: Option<Prop<$rect_ty>>,
                    pub(crate) [<$rect_name _right>]: Option<Prop<$rect_ty>>,
                    pub(crate) [<$rect_name _top>]: Option<Prop<$rect_ty>>,
                    pub(crate) [<$rect_name _bottom>]: Option<Prop<$rect_ty>>,
                )*
                $(pub $hook_name: HookFn<$hook_ty>,)*
                pub(crate) children: Option<ChildrenPlan>,
                pub(crate) classes: Option<Prop<HashSet<ClassName>>>,
                pub(crate) hover_owners: Vec<Rc<HoverOwner>>,
                // Resolver snapshots own reactive values allocated by rule bodies.
                pub(crate) theme_owners: Vec<Rc<OwnedScope>>,
                themed: Option<Box<ThemedStyle>>,
            }

            struct ThemedStyle {
                theme: ReadSignal<Rc<Style>>,
                fallback: Rc<Style>,
                inherited_foreground: Option<Prop<Brush>>,
                scope: Scope,
                animation_curve: Cell<Option<Prop<crate::transition::Transition>>>,
                $($name: OnceCell<Prop<$ty>>,)*
                $(
                    [<$rect_name _left>]: OnceCell<Prop<$rect_ty>>,
                    [<$rect_name _right>]: OnceCell<Prop<$rect_ty>>,
                    [<$rect_name _top>]: OnceCell<Prop<$rect_ty>>,
                    [<$rect_name _bottom>]: OnceCell<Prop<$rect_ty>>,
                    $rect_name: OnceCell<Prop<taffy::Rect<$rect_ty>>>,
                )*
            }

            /// Fluent styles accepting either static properties or reactive signals.
            /// Length literals and reactive `f32` values are pixels.
            pub trait Styleable: Sized {
                fn style_mut(&mut self) -> &mut Style;

                /// Set a matrix or transform list, including 3D operations projected to the canvas plane.
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
                    let previous = std::mem::take(self.style_mut());
                    style.provide_defaults(previous);
                    *self.style_mut() = style;
                    self
                }

                $(
                    define_style!(@setter $(#[$property_doc])* $name, $ty);
                )*

                $(
                    fn [<$rect_name _left>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        if let Some(state) = &mut self.style_mut().themed { state.$rect_name.take(); }
                        self.style_mut().[<$rect_name _left>] = Some(value.into());
                        self
                    }
                    fn [<$rect_name _right>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        if let Some(state) = &mut self.style_mut().themed { state.$rect_name.take(); }
                        self.style_mut().[<$rect_name _right>] = Some(value.into());
                        self
                    }
                    fn [<$rect_name _top>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        if let Some(state) = &mut self.style_mut().themed { state.$rect_name.take(); }
                        self.style_mut().[<$rect_name _top>] = Some(value.into());
                        self
                    }
                    fn [<$rect_name _bottom>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        if let Some(state) = &mut self.style_mut().themed { state.$rect_name.take(); }
                        self.style_mut().[<$rect_name _bottom>] = Some(value.into());
                        self
                    }
                    fn [<$rect_name _x>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        if let Some(state) = &mut self.style_mut().themed { state.$rect_name.take(); }
                        let prop = value.into();
                        self.style_mut().[<$rect_name _left>] = Some(prop.clone());
                        self.style_mut().[<$rect_name _right>] = Some(prop.clone());
                        self
                    }
                    fn [<$rect_name _y>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        if let Some(state) = &mut self.style_mut().themed { state.$rect_name.take(); }
                        let prop = value.into();
                        self.style_mut().[<$rect_name _top>] = Some(prop.clone());
                        self.style_mut().[<$rect_name _bottom>] = Some(prop);
                        self
                    }
                    fn $rect_name(mut self, value: impl IntoRectProp<$rect_ty>) -> Self {
                        if let Some(state) = &mut self.style_mut().themed { state.$rect_name.take(); }
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

                /// Replace the child input, deferring child preparation until mount.
                fn children(mut self, children: impl IntoChildren) -> Self {
                    self.style_mut().children = Some(children.into_children_plan());
                    self
                }
            }

            impl Styleable for Style {
                fn style_mut(&mut self) -> &mut Style {
                    self
                }
            }

            impl Style {
                /// Store sparse theme defaults without allocating property projections.
                #[cfg(test)]
                pub(crate) fn themed_defaults(theme: ReadSignal<Rc<Style>>, default: Self) -> Self {
                    let mut result = Self::new();
                    result.provide_themed_defaults(theme, default);
                    result
                }

                /// Seal sparse defaults; create each stable projection on first consumption.
                pub(crate) fn provide_themed_defaults(&mut self, theme: ReadSignal<Rc<Style>>, mut default: Self) {
                    // Non-visual inputs keep their existing ownership and hook ordering.
                    $(
                        mem::swap(&mut self.$hook_name, &mut default.$hook_name);
                        self.$hook_name.extend(mem::take(&mut default.$hook_name));
                    )*
                    if self.children.is_none() { self.children = default.children.take(); }
                    if self.classes.is_none() { self.classes = default.classes.take(); }
                    self.append_hover_owners(mem::take(&mut default.hover_owners));
                    self.append_theme_owners(default.theme_owners.clone());
                    self.themed = Some(Box::new(ThemedStyle {
                        theme,
                        fallback: Rc::new(default),
                        // Capture inheritance at seal, not in a later caller's context.
                        inherited_foreground: crate::theme::inherited_foreground(),
                        scope: current_scope(),
                        animation_curve: Cell::new(None),
                        $($name: OnceCell::new(),)*
                        $(
                            [<$rect_name _left>]: OnceCell::new(),
                            [<$rect_name _right>]: OnceCell::new(),
                            [<$rect_name _top>]: OnceCell::new(),
                            [<$rect_name _bottom>]: OnceCell::new(),
                            $rect_name: OnceCell::new(),
                        )*
                    }));
                }
                pub(crate) fn bind_theme_transition(&self, transition: Prop<crate::transition::Transition>) {
                    if let Some(state) = &self.themed { state.animation_curve.set(Some(transition)); }
                }
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
                fn materialize_theme(&mut self) {
                    if self.themed.is_none() { return; }
                    $(if self.$name.is_none() { self.$name = self.[<declared_ $name>](); })*
                    $(
                        if self.[<$rect_name _left>].is_none() { self.[<$rect_name _left>] = self.[<declared_ $rect_name _left>](); }
                        if self.[<$rect_name _right>].is_none() { self.[<$rect_name _right>] = self.[<declared_ $rect_name _right>](); }
                        if self.[<$rect_name _top>].is_none() { self.[<$rect_name _top>] = self.[<declared_ $rect_name _top>](); }
                        if self.[<$rect_name _bottom>].is_none() { self.[<$rect_name _bottom>] = self.[<declared_ $rect_name _bottom>](); }
                    )*
                    self.themed = None;
                }
                /// Fill missing properties from `other` and append its event hooks.
                pub fn merge(mut self, mut other: Self) -> Self {
                    // Public style composition also accepts an already sealed Style.
                    self.materialize_theme();
                    other.materialize_theme();
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
                    self.classes = self.classes.or(other.classes);
                    self.append_hover_owners(other.hover_owners);
                    self.append_theme_owners(other.theme_owners);
                    self
                }

                /// Fill missing properties and prepend default event hooks.
                pub fn provide_defaults(&mut self, mut default: Self) {
                    self.materialize_theme();
                    default.materialize_theme();
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
                    if self.classes.is_none() {
                        self.classes = default.classes;
                    }
                    self.append_hover_owners(default.hover_owners);
                    self.append_theme_owners(default.theme_owners);
                }

                pub(crate) fn is_visually_empty(&self) -> bool {
                    self.themed.is_none()
                        $( && self.$name.is_none() )*
                        $( && self.[<$rect_name _left>].is_none()
                           && self.[<$rect_name _right>].is_none()
                           && self.[<$rect_name _top>].is_none()
                           && self.[<$rect_name _bottom>].is_none() )*
                }

                pub(crate) fn assert_visual_only(&self) {
                    assert!(self.children.is_none()
                        && self.classes.is_none()
                        && self.hover_owners.is_empty()
                        $( && self.$hook_name.is_empty() )*,
                        "theme declarations may only set visual properties");
                }

                $(
                    /// Return the declaration, or the node's shared lazy theme binding after seal.
                    pub fn [<declared_ $name>](&self) -> Option<Prop<$ty>> {
                        self.$name.clone().or_else(|| self.themed.as_ref().map(|state| {
                            state.$name.get_or_init(|| state.scope.run(|| {
                                let value = define_style!(@project state, $name, $ty, $($default)?);
                                define_style!(@animate state, value, $name)
                            })).clone()
                        }))
                    }
                    #[allow(dead_code)]
                    pub fn [<take_ $name>](&mut self) -> Prop<$ty> {
                        if self.themed.is_some() { return self.[<declared_ $name>]().unwrap(); }
                        self.$name.take().unwrap_or_else(|| define_style!(@extract_default ($ty, $($default)?)))
                    }
                )*

                $(
                    define_style!(@side_accessor [<$rect_name _left>], $rect_ty, $($rect_default)?);
                    define_style!(@side_accessor [<$rect_name _right>], $rect_ty, $($rect_default)?);
                    define_style!(@side_accessor [<$rect_name _top>], $rect_ty, $($rect_default)?);
                    define_style!(@side_accessor [<$rect_name _bottom>], $rect_ty, $($rect_default)?);
                    #[allow(dead_code)]
                    pub fn [<take_ $rect_name>](&mut self) -> Prop<taffy::Rect<$rect_ty>> {
                        if let Some(state) = &self.themed {
                            return state.$rect_name.get_or_init(|| state.scope.run(|| {
                                let left = self.[<declared_ $rect_name _left>]().unwrap();
                                let right = self.[<declared_ $rect_name _right>]().unwrap();
                                let top = self.[<declared_ $rect_name _top>]().unwrap();
                                let bottom = self.[<declared_ $rect_name _bottom>]().unwrap();
                                computed(move |_| taffy::Rect {
                                    left: left.get_clone(), right: right.get_clone(),
                                    top: top.get_clone(), bottom: bottom.get_clone(),
                                }).into()
                            })).clone();
                        }
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
        on_transform: HookFn<TransformEvent>;
    }
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    fn themed_foreground_owned(
        theme: ReadSignal<Rc<Style>>,
        fallback: Rc<Style>,
        inherited: Option<Prop<Brush>>,
    ) -> Prop<Brush> {
        computed(move |_| {
            #[cfg(test)]
            crate::runtime::record_projection();
            let current = theme.get_clone();
            current
                .color
                .as_ref()
                .or(fallback.color.as_ref())
                .or(inherited.as_ref())
                .map(|value| value.get_clone())
                .unwrap_or(Brush::Solid(Color::WHITE))
        })
        .into()
    }

    #[cfg(test)]
    fn themed_foreground(
        theme: ReadSignal<Rc<Style>>,
        fallback: Option<Prop<Brush>>,
        inherited: Option<Prop<Brush>>,
    ) -> Prop<Brush> {
        let fallback = fallback.or(inherited);
        computed(move |_| {
            #[cfg(test)]
            crate::runtime::record_projection();
            let current = theme.get_clone();
            current
                .color
                .as_ref()
                .or(fallback.as_ref())
                .map(|value| value.get_clone())
                .unwrap_or(Brush::Solid(Color::WHITE))
        })
        .into()
    }

    pub(crate) fn ensure_hovered(&mut self) -> Rc<HoverOwner> {
        if let Some(owner) = self.hover_owners.first() {
            return owner.clone();
        }
        let owner = HoverOwner::new();
        self.hover_owners.push(owner.clone());
        owner
    }

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

    fn append_theme_owners(&mut self, owners: Vec<Rc<OwnedScope>>) {
        for owner in owners {
            if !self
                .theme_owners
                .iter()
                .any(|existing| Rc::ptr_eq(existing, &owner))
            {
                self.theme_owners.push(owner);
            }
        }
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

#[cfg(test)]
mod tests {
    use std::{cell::Cell, panic::AssertUnwindSafe};

    use mue_core::{
        effect::watch_effect,
        scope::OwnedScope,
        signal::{Signal, signal},
    };

    use super::*;
    use crate::theme::{ThemeTarget, Themeable};

    #[derive(Default)]
    struct Target(Style);

    impl ThemeTarget for Target {
        type Options = ();
    }
    impl Styleable for Target {
        fn style_mut(&mut self) -> &mut Style {
            &mut self.0
        }
    }

    fn classes(name: &str) -> HashSet<ClassName> {
        HashSet::from([ClassName::try_from(name).unwrap()])
    }

    #[test]
    fn stable_slots_accept_new_fields_and_restore_sparse_fallbacks() {
        let scope = OwnedScope::new();
        scope.run(|| {
            let theme = Signal::shallow(Rc::new(Style::new()));
            let fallback = signal(Size::Px(12.));
            let mut style =
                Style::themed_defaults(*theme, Style::new().width(fallback).padding_left(3.));
            let width = style.take_width();
            let height = style.take_height();
            let padding = style.take_padding();
            let font_size = style.take_font_size();
            drop(style);

            assert_eq!(width.get(), Size::Px(12.));
            assert_eq!(height.get(), Size::Auto);
            assert_eq!(padding.get().left, Size::Px(3.));
            assert_eq!(font_size.get(), FontSize::Em(1.));

            theme.set(Rc::new(
                Style::new()
                    .width(24.)
                    .height(18.)
                    .padding_top(7.)
                    .font_size(20.),
            ));
            assert_eq!(width.get(), Size::Px(24.));
            assert_eq!(height.get(), Size::Px(18.));
            assert_eq!(padding.get().left, Size::Px(3.));
            assert_eq!(padding.get().top, Size::Px(7.));
            assert_eq!(font_size.get(), FontSize::Px(20.));

            fallback.set(Size::Px(16.));
            theme.set(Rc::new(Style::new()));
            assert_eq!(width.get(), Size::Px(16.));
            assert_eq!(height.get(), Size::Auto);
            assert_eq!(padding.get().top, Size::Px(0.));
            assert_eq!(font_size.get(), FontSize::Em(1.));
        });
    }

    #[test]
    fn equal_projections_do_not_notify_visual_consumers() {
        let scope = OwnedScope::new();
        scope.run(|| {
            let transform = Transform(vec![TransformOp::Scale(2., 2.)]);
            let theme = Signal::shallow(Rc::new(
                Style::new()
                    .width(10.)
                    .color(Color::BLACK)
                    .transform_list(transform.clone()),
            ));
            let mut style = Style::themed_defaults(*theme, Style::new());
            let width = style.take_width();
            let color = style.take_color();
            let transform_list = style.take_transform_list();
            let runs = Rc::new(Cell::new(0));
            let observed = runs.clone();
            watch_effect(move || {
                width.get();
                color.get_clone();
                transform_list.get_clone();
                observed.set(observed.get() + 1);
            });
            assert_eq!(runs.get(), 1);

            theme.set(Rc::new(
                Style::new()
                    .width(10.)
                    .color(Color::BLACK)
                    .transform_list(transform)
                    .height(50.),
            ));
            assert_eq!(runs.get(), 1);
            theme.set(Rc::new(
                Style::new()
                    .width(11.)
                    .color(Color::BLACK)
                    .transform_list(Transform(vec![TransformOp::Scale(2., 2.)])),
            ));
            assert_eq!(runs.get(), 2);
        });
    }

    #[test]
    fn explicit_values_keep_their_signal_and_override_theme_and_fallback() {
        let scope = OwnedScope::new();
        scope.run(|| {
            let theme = Signal::shallow(Rc::new(Style::new().width(20.)));
            let explicit = signal(Size::Px(40.));
            let mut style = Style::new().width(explicit);
            style.provide_themed_defaults(*theme, Style::new().width(10.));
            let width = style.take_width();
            theme.set(Rc::new(Style::new().width(30.)));
            assert_eq!(width.get(), Size::Px(40.));
            explicit.set(Size::Px(50.));
            assert_eq!(width.get(), Size::Px(50.));
        });
    }

    #[test]
    fn color_rule_removal_restores_reactive_inheritance_and_fallback_precedence() {
        let scope = OwnedScope::new();
        scope.run(|| {
            let theme = Signal::shallow(Rc::new(Style::new().color(Color::WHITE)));
            let inherited = signal(Brush::Solid(Color::BLACK));
            let color = Style::themed_foreground(*theme, None, Some(inherited.into()));
            assert_eq!(color.get_clone(), Brush::Solid(Color::WHITE));
            theme.set(Rc::new(Style::new()));
            assert_eq!(color.get_clone(), Brush::Solid(Color::BLACK));
            let changed = Brush::Solid(Color::new(0.2, 0.4, 0.6, 1.));
            inherited.set(changed.clone());
            assert_eq!(color.get_clone(), changed);

            let fallback =
                Style::themed_foreground(*theme, Some(Color::WHITE.into()), Some(inherited.into()));
            assert_eq!(fallback.get_clone(), Brush::Solid(Color::WHITE));
            let mut no_inheritance = Style::themed_defaults(*theme, Style::new());
            assert_eq!(
                no_inheritance.take_color().get_clone(),
                Brush::Solid(Color::WHITE)
            );
        });
    }

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
        let mut merged = outer.merge(inner);
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

    fn owned_theme_style(build: impl FnOnce() -> Style) -> (Style, std::rc::Weak<OwnedScope>) {
        let owner = Rc::new(OwnedScope::new_inherited());
        let mut style = owner.run(build);
        let weak = Rc::downgrade(&owner);
        style.theme_owners.push(owner);
        (style, weak)
    }

    #[test]
    fn combining_theme_styles_transfers_all_rule_scopes_without_duplicates() {
        let outer_scope = OwnedScope::new();
        outer_scope.run(|| {
            let input = signal(2.);
            let (outer, outer_owner) =
                owned_theme_style(|| Style::new().width(computed(move |_| Size::Px(input.get()))));
            let (mut inner, inner_owner) = owned_theme_style(|| {
                Style::new().height(computed(move |_| Size::Px(input.get() * 2.)))
            });
            inner.theme_owners.push(outer.theme_owners[0].clone());
            let mut style = outer.merge(inner);
            assert_eq!(style.theme_owners.len(), 2);

            let (mut fallback, fallback_owner) =
                owned_theme_style(|| Style::new().opacity(computed(move |_| input.get() / 4.)));
            fallback.theme_owners.push(style.theme_owners[0].clone());
            style.provide_defaults(fallback);
            assert_eq!(style.theme_owners.len(), 3);

            let (replacement, replacement_owner) = owned_theme_style(|| {
                Style::new().radius(computed(move |_| Size::Px(input.get() * 3.)))
            });
            style = style.style(replacement);
            assert_eq!(style.theme_owners.len(), 4);
            style.assert_visual_only();
            let width = style.take_width();
            let height = style.take_height();
            let opacity = style.take_opacity();
            let radius = style.take_radius();
            input.set(4.);
            assert_eq!(width.get(), Size::Px(4.));
            assert_eq!(height.get(), Size::Px(8.));
            assert_eq!(opacity.get(), 1.);
            assert_eq!(radius.get(), Size::Px(12.));

            let owners = [outer_owner, inner_owner, fallback_owner, replacement_owner];
            assert!(owners.iter().all(|owner| owner.upgrade().is_some()));
            drop(style);
            assert!(owners.iter().all(|owner| owner.upgrade().is_none()));
            for value in [width, height, radius] {
                match value {
                    Prop::Dynamic(value) => assert!(value.is_disposed()),
                    Prop::Static(_) => panic!("rule-derived length must remain reactive"),
                }
            }
            assert!(!input.is_disposed());
        });
    }

    #[test]
    fn replacing_resolver_snapshot_releases_previous_rule_scope() {
        let scope = OwnedScope::new();
        scope.run(|| {
            let input = signal(3.);
            let generation = signal(1.);
            let owners = Rc::new(std::cell::RefCell::new(Vec::new()));
            let created = owners.clone();
            let theme = mue_core::effect::computed_always(move |_| {
                let factor = generation.get();
                if factor == 0. {
                    return Rc::new(Style::new());
                }
                let (style, owner) = owned_theme_style(|| {
                    Style::new().width(computed(move |_| Size::Px(input.get() * factor)))
                });
                created.borrow_mut().push(owner);
                Rc::new(style)
            });
            let mut style = Style::themed_defaults(theme, Style::new());
            let width = style.take_width();
            let observed = Rc::new(Cell::new(Size::Auto));
            let last = observed.clone();
            watch_effect(move || last.set(width.get()));
            assert_eq!(observed.get(), Size::Px(3.));

            generation.set(2.);
            assert!(owners.borrow()[0].upgrade().is_none());
            assert!(owners.borrow()[1].upgrade().is_some());
            assert_eq!(observed.get(), Size::Px(6.));
            input.set(4.);
            assert_eq!(observed.get(), Size::Px(8.));
            assert_eq!(owners.borrow().len(), 2);

            generation.set(0.);
            assert!(owners.borrow()[1].upgrade().is_none());
            assert_eq!(observed.get(), Size::Auto);
            assert!(!input.is_disposed());
        });
    }

    #[test]
    fn classes_replace_previous_inputs_in_setters_and_style_application() {
        let scope = OwnedScope::new();
        scope.run(|| {
            let old = signal(classes("old"));
            let mut target = Target::default().classes(old).classes(classes("new"));
            old.set(classes("stale"));
            assert_eq!(
                target.0.classes.as_ref().unwrap().get_clone(),
                classes("new")
            );
            let replacement = Target::default().classes(classes("replacement")).0;
            target = target.style(replacement);
            assert_eq!(
                target.0.classes.as_ref().unwrap().get_clone(),
                classes("replacement")
            );
            target = target.style(Style::new().width(10.));
            assert_eq!(
                target.0.classes.as_ref().unwrap().get_clone(),
                classes("replacement")
            );
        });
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

    #[test]
    fn theme_styles_reject_non_visual_channels() {
        Style::new().width(10.).assert_visual_only();
        let mut classes = Style::new();
        classes.classes = Some(HashSet::new().into());
        let mut hover = Style::new();
        hover.ensure_hovered();
        for style in [
            Style::new().children(()),
            Style::new().on_tap(|_| {}),
            classes,
            hover,
        ] {
            assert!(
                std::panic::catch_unwind(AssertUnwindSafe(|| style.assert_visual_only())).is_err()
            );
        }
    }
}

/// Enumerations used by style declarations without requiring direct Taffy dependencies.
pub mod values {
    pub use cosmic_text::Align as TextAlign;
    pub use taffy::{
        AlignContent, AlignItems, BoxSizing, Display, FlexDirection, FlexWrap, Overflow, Position,
    };
}
