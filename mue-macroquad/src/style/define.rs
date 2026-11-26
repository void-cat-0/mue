//! The `define_style!` table macro that generates `Style`, `Styleable`, and accessors.

macro_rules! define_style {
    (@setter $(#[$doc:meta])* shadow_cache, $ty:ty) => {
        /// Explicitly enable/disable caching, retaining bool and reactive-bool compatibility.
        /// Use shadow_cache_policy for Auto; leaving the property unset selects Auto.
        fn shadow_cache(mut self, enabled: impl Into<Prop<bool>>) -> Self {
            self.style_mut().put_shadow_cache(enabled.into().map(ShadowCache::from));
            self
        }
        $(#[$doc])*
        fn shadow_cache_policy(mut self, policy: impl Into<Prop<ShadowCache>>) -> Self {
            self.style_mut().put_shadow_cache(policy.into());
            self
        }
    };
    (@setter $(#[$doc:meta])* transform_list, $ty:ty) => {
        $(#[$doc])*
        fn transform_list(mut self, value: impl Into<Prop<Transform>>) -> Self {
            self.style_mut().put_transform_list(value.into());
            self.style_mut().put_matrix_transform(Prop::Static(Matrix::IDENTITY));
            self
        }
    };
    (@setter $(#[$doc:meta])* line_height, $ty:ty) => {
        $(#[$doc])*
        fn line_height(mut self, value: impl Into<Prop<Size>>) -> Self {
            self.style_mut().put_line_height(value.into());
            self.style_mut().put_line_height_multiplier(Prop::Static(None));
            self
        }
    };
    (@setter $(#[$doc:meta])* $name:ident, $ty:ty) => {
        $(#[$doc])*
        fn $name(mut self, value: impl Into<Prop<$ty>>) -> Self {
            paste! { self.style_mut().[<put_ $name>](value.into()); }
            self
        }
    };
    (@accessor $field:ident, $ty:ty, $($default:expr)?) => {
        paste! {
            /// Return the explicitly declared property, if any.
            pub fn [<declared_ $field>](&self) -> Option<Prop<$ty>> {
                self.props.get::<$ty>(StyleKey::$field as u16)
            }
            #[allow(dead_code)]
            pub fn [<take_ $field>](&mut self) -> Prop<$ty> {
                self.props
                    .take::<$ty>(StyleKey::$field as u16)
                    .unwrap_or_else(|| define_style!(@extract_default ($ty, $($default)?)))
            }
            #[allow(dead_code)]
            pub(crate) fn [<put_ $field>](&mut self, value: Prop<$ty>) {
                self.props.set(StyleKey::$field as u16, value);
            }
        }
    };
    (@extract_default_raw ($ty:ty, $default:expr)) => { $default };
    (@extract_default_raw ($ty:ty, )) => { <$ty>::default() };
    (@extract_default ($($rest:tt)*)) => { Prop::Static(define_style!(@extract_default_raw ($($rest)*))) };
    (
        $($(#[$property_doc:meta])* $name:ident : $ty:ty $(= $default:expr)?;)*
        @rects { $($rect_name:ident : taffy::Rect<$rect_ty:ty> $( = $rect_default:expr)?;)* }
        @hooks { $($(#[$hook_doc:meta])* $hook_name:ident : CallbackList<$hook_ty:ty>;)* }
    ) => {
        paste! {
            /// Keys of the sparse property store, one per declared property.
            #[allow(non_camel_case_types, dead_code)]
            #[derive(Clone, Copy)]
            #[repr(u16)]
            enum StyleKey {
                $($name,)*
                $([<$rect_name _left>], [<$rect_name _right>], [<$rect_name _top>], [<$rect_name _bottom>],)*
            }

            /// Optional layout, paint, font, child, and event properties for a node.
            ///
            /// Only declared properties are stored, so a `Style` stays small no
            /// matter how many properties exist; builders carry one by value.
            #[derive(Default)]
            pub struct Style {
                props: crate::style::store::PropStore,
                $($(#[$hook_doc])* pub $hook_name: CallbackList<$hook_ty>,)*
                pub(crate) children: Option<ChildrenPlan>,
                pub(crate) hover_owners: Vec<Rc<HoverOwner>>,
                pub(crate) focus_owners: Vec<Rc<crate::input::focus::FocusOwner>>,
                pub(crate) node_handles: Vec<crate::node::NodeHandle>,
            }

            /// Fluent styles accepting either static properties or reactive signals.
            /// Length literals and reactive f32 values are pixels.
            pub trait Styleable: Sized {
                fn style_mut(&mut self) -> &mut Style;

                /// Set a matrix or transform list, including projected 3D operations.
                fn transform(mut self, value: impl IntoTransformProp) -> Self {
                    match value.into_transform_prop() {
                        TransformProp::Matrix(value) => {
                            self.style_mut().put_matrix_transform(value);
                            self.style_mut().put_transform_list(Prop::Static(Transform::default()));
                        }
                        TransformProp::Functions(value) => {
                            self.style_mut().put_transform_list(value);
                            self.style_mut().put_matrix_transform(Prop::Static(Matrix::IDENTITY));
                        }
                    }
                    self
                }

                /// Apply a style, replacing properties explicitly set by it.
                fn style(mut self, mut style: Style) -> Self {
                    style.provide_defaults_mut(self.style_mut());
                    let target = self.style_mut();
                    target.clear();
                    mem::swap(target, &mut style);
                    self
                }
                $(define_style!(@setter $(#[$property_doc])* $name, $ty);)*
                $(
                    fn [<$rect_name _left>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        self.style_mut().[<put_ $rect_name _left>](value.into());
                        self
                    }
                    fn [<$rect_name _right>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        self.style_mut().[<put_ $rect_name _right>](value.into());
                        self
                    }
                    fn [<$rect_name _top>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        self.style_mut().[<put_ $rect_name _top>](value.into());
                        self
                    }
                    fn [<$rect_name _bottom>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        self.style_mut().[<put_ $rect_name _bottom>](value.into());
                        self
                    }
                    fn [<$rect_name _x>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        let value = value.into();
                        self.style_mut().[<put_ $rect_name _left>](value.clone());
                        self.style_mut().[<put_ $rect_name _right>](value);
                        self
                    }
                    fn [<$rect_name _y>](mut self, value: impl Into<Prop<$rect_ty>>) -> Self {
                        let value = value.into();
                        self.style_mut().[<put_ $rect_name _top>](value.clone());
                        self.style_mut().[<put_ $rect_name _bottom>](value);
                        self
                    }
                    fn $rect_name(mut self, value: impl IntoRectProp<$rect_ty>) -> Self {
                        let rect = value.into_rect_prop();
                        self.style_mut().[<put_ $rect_name _left>](rect.left);
                        self.style_mut().[<put_ $rect_name _right>](rect.right);
                        self.style_mut().[<put_ $rect_name _top>](rect.top);
                        self.style_mut().[<put_ $rect_name _bottom>](rect.bottom);
                        self
                    }
                )*
                $(
                    $(#[$hook_doc])*
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
            impl Styleable for &mut Style { fn style_mut(&mut self) -> &mut Style { &mut **self } }

            impl Style {
                pub(crate) fn bind_explicit_transitions(&mut self, transition: Prop<crate::motion::transition::Transition>) {
                    $(if let Some(value) = self.props.take::<$ty>(StyleKey::$name as u16) {
                        self.[<put_ $name>](crate::motion::transition::bind_property!($name, value, Some(transition)));
                    })*
                    $(
                        if let Some(value) = self.props.take::<$rect_ty>(StyleKey::[<$rect_name _left>] as u16) {
                            self.[<put_ $rect_name _left>](crate::motion::transition::bind_property!([<$rect_name _left>], value, Some(transition)));
                        }
                        if let Some(value) = self.props.take::<$rect_ty>(StyleKey::[<$rect_name _right>] as u16) {
                            self.[<put_ $rect_name _right>](crate::motion::transition::bind_property!([<$rect_name _right>], value, Some(transition)));
                        }
                        if let Some(value) = self.props.take::<$rect_ty>(StyleKey::[<$rect_name _top>] as u16) {
                            self.[<put_ $rect_name _top>](crate::motion::transition::bind_property!([<$rect_name _top>], value, Some(transition)));
                        }
                        if let Some(value) = self.props.take::<$rect_ty>(StyleKey::[<$rect_name _bottom>] as u16) {
                            self.[<put_ $rect_name _bottom>](crate::motion::transition::bind_property!([<$rect_name _bottom>], value, Some(transition)));
                        }
                    )*
                }
                /// Fill missing properties and prepend the default hooks.
                pub fn provide_defaults(&mut self, mut default: Self) {
                    self.provide_defaults_mut(&mut default);
                }
                /// Merge a borrowed input without moving its complete storage through the call chain.
                pub(crate) fn provide_defaults_mut(&mut self, default: &mut Self) {
                    self.props.merge_missing(&mut default.props);
                    $(
                        mem::swap(&mut self.$hook_name, &mut default.$hook_name);
                        self.$hook_name.extend(mem::take(&mut default.$hook_name));
                    )*
                    if self.children.is_none() { self.children = default.children.take(); }
                    self.append_hover_owners(mem::take(&mut default.hover_owners));
                    for owner in mem::take(&mut default.focus_owners) { if !self.focus_owners.iter().any(|o| Rc::ptr_eq(o, &owner)) { self.focus_owners.push(owner); } }
                    self.node_handles.append(&mut default.node_handles);
                    // Release rejected inputs only after the complete merge, in the same
                    // field order as dropping an owned Style. In particular, a rejected
                    // child plan must be gone before installing the resulting style.
                    default.clear();
                }
                fn clear(&mut self) {
                    self.props.clear();
                    $(let _ = mem::take(&mut self.$hook_name);)*
                    drop(self.children.take());
                    drop(mem::take(&mut self.hover_owners));
                    drop(mem::take(&mut self.focus_owners));
                    self.node_handles.clear();
                }
                $(define_style!(@accessor $name, $ty, $($default)?);)*
                $(
                    define_style!(@accessor [<$rect_name _left>], $rect_ty, $($rect_default)?);
                    define_style!(@accessor [<$rect_name _right>], $rect_ty, $($rect_default)?);
                    define_style!(@accessor [<$rect_name _top>], $rect_ty, $($rect_default)?);
                    define_style!(@accessor [<$rect_name _bottom>], $rect_ty, $($rect_default)?);
                    #[allow(dead_code)]
                    pub fn [<take_ $rect_name>](&mut self) -> Prop<taffy::Rect<$rect_ty>> {
                        if !self.props.contains(StyleKey::[<$rect_name _left>] as u16)
                            && !self.props.contains(StyleKey::[<$rect_name _right>] as u16)
                            && !self.props.contains(StyleKey::[<$rect_name _top>] as u16)
                            && !self.props.contains(StyleKey::[<$rect_name _bottom>] as u16)
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

pub(crate) use define_style;
