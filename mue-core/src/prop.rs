use std::{
    borrow::Cow,
    collections::HashSet,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use crate::{
    effect::computed,
    runtime::Runtime,
    signal::{Access, ReadSignal, Signal, SignalId},
};

#[derive(Clone, Copy)]
pub struct PropTracker(Option<SignalId>);

impl Access for PropTracker {
    type Value = ();

    fn track(&self) {
        if let Some(signal_id) = &self.0 {
            Runtime::with(|rt| rt.track(*signal_id));
        }
    }

    fn get_clone_untracked(&self) {}
}

#[derive(Clone, Copy)]
/// A static value or reactive signal accepted by component and style builders.
pub enum Prop<T> {
    Static(T),
    Dynamic(ReadSignal<T>),
}

impl<T: Default> Default for Prop<T> {
    fn default() -> Self {
        Self::Static(T::default())
    }
}

impl<T: 'static> Access for Prop<T> {
    type Value = T;

    fn track(&self) {
        match self {
            Self::Static(_) => {}
            Self::Dynamic(signal) => signal.track(),
        }
    }

    fn get_clone_untracked(&self) -> T
    where
        T: Clone,
    {
        match self {
            Self::Static(value) => value.clone(),
            Self::Dynamic(signal) => signal.get_clone_untracked(),
        }
    }
}

impl<T: 'static> Prop<T> {
    pub fn as_static(&self) -> Option<&T> {
        match self {
            Self::Static(value) => Some(value),
            Self::Dynamic(_) => None,
        }
    }

    pub fn get_mut(&mut self) -> Option<&mut T> {
        match self {
            Self::Static(value) => Some(value),
            Self::Dynamic(_) => None,
        }
    }

    pub fn map<U: PartialEq + 'static>(&self, f: impl Fn(T) -> U + 'static) -> Prop<U>
    where
        T: Clone,
    {
        match self {
            Self::Static(value) => Prop::Static(f(value.clone())),
            Self::Dynamic(signal) => Prop::Dynamic(signal.map(f)),
        }
    }

    pub fn to_tracker(&self) -> PropTracker {
        PropTracker(match self {
            Self::Static(_) => None,
            Self::Dynamic(signal) => Some(signal.id),
        })
    }
}

impl Prop<bool> {
    /// Choose between two values when this property is true or false.
    ///
    /// A static property resolves immediately; a dynamic one creates a derived
    /// signal, so the result stays bound to the condition.
    ///
    /// ```
    /// use mue_core::prelude::*;
    ///
    /// let _ = Scope::null().run(|| {
    ///     let on = signal(false);
    ///     let flag: Prop<bool> = on.into();
    ///     let width = flag.select(10_f32, 4_f32);
    ///     assert_eq!(width.get(), 4.);
    ///     on.set(true);
    ///     assert_eq!(width.get(), 10.);
    /// });
    /// ```
    pub fn select<T: Clone + PartialEq + 'static>(&self, when_true: T, when_false: T) -> Prop<T> {
        match self {
            Self::Static(true) => Prop::Static(when_true),
            Self::Static(false) => Prop::Static(when_false),
            Self::Dynamic(signal) => Prop::Dynamic(signal.select(when_true, when_false)),
        }
    }

    /// Choose between two properties when this property is true or false.
    ///
    /// Unlike [`Prop::select`], both branches are properties, so a reactive branch
    /// keeps updating while it is selected. When both branches are static, the
    /// result collapses into a single mapped signal.
    ///
    /// When a value type has several `Prop` conversions (for example `f32`, which
    /// also converts to `Size` and `FontSize`), annotate the result type.
    ///
    /// ```
    /// use mue_core::prelude::*;
    ///
    /// let _ = Scope::null().run(|| {
    ///     let selected = signal(false);
    ///     let accent = signal(1_f32);
    ///     let panel = signal(2_f32);
    ///     let background: Prop<f32> = selected.pick(accent, panel);
    ///     assert_eq!(background.get(), 2.);
    ///     accent.set(9.);
    ///     assert_eq!(background.get(), 2.);
    ///     selected.set(true);
    ///     assert_eq!(background.get(), 9.);
    /// });
    /// ```
    pub fn pick<T, A, B>(&self, when_true: A, when_false: B) -> Prop<T>
    where
        T: Clone + PartialEq + 'static,
        A: Into<Prop<T>>,
        B: Into<Prop<T>>,
    {
        let when_true = when_true.into();
        let when_false = when_false.into();
        match *self {
            Self::Static(true) => when_true,
            Self::Static(false) => when_false,
            Self::Dynamic(signal) => match (when_true, when_false) {
                (Prop::Static(when_true), Prop::Static(when_false)) => {
                    Prop::Dynamic(signal.map(move |on| {
                        if on {
                            when_true.clone()
                        } else {
                            when_false.clone()
                        }
                    }))
                }
                (when_true, when_false) => Prop::Dynamic(computed(move |_| {
                    if signal.get() {
                        when_true.get_clone()
                    } else {
                        when_false.get_clone()
                    }
                })),
            },
        }
    }
}

/// Marks types that can be passed directly as static properties.
pub trait PropValue {}

impl<T> From<ReadSignal<T>> for Prop<T> {
    fn from(signal: ReadSignal<T>) -> Self {
        Self::Dynamic(signal)
    }
}

impl<T> From<Signal<T>> for Prop<T> {
    fn from(value: Signal<T>) -> Self {
        (*value).into()
    }
}

impl From<ReadSignal<&'static str>> for Prop<Rc<str>> {
    fn from(value: ReadSignal<&'static str>) -> Self {
        value.map(Rc::from).into()
    }
}
impl From<ReadSignal<String>> for Prop<Rc<str>> {
    fn from(value: ReadSignal<String>) -> Self {
        value.map(Rc::from).into()
    }
}

impl<T: PropValue> From<T> for Prop<T> {
    fn from(value: T) -> Self {
        Self::Static(value)
    }
}
impl<T: PropValue> From<T> for Prop<Option<T>> {
    fn from(value: T) -> Self {
        Self::Static(Some(value))
    }
}

macro_rules! impl_into {
    ($to:ty; $($ty:ty),*) => {
        $(
            impl From<$ty> for Prop<$to> {
                fn from(value: $ty) -> Self {
                    Prop::Static(value.into())
                }
            }
        )*
    };
}
impl_into!(String; &'_ str, Cow<'_, str>);
impl_into!(Rc<str>; &'_ str, String);
impl_into!(Arc<str>; &'_ str, String);
impl_into!(PathBuf; &'_ str, &'_ Path, Cow<'_, Path>);
impl_into!(OsString; &'_ str, &'_ OsStr, Cow<'_, OsStr>);

macro_rules! impl_prop_value {
    ($($ty:ty),* $(,)?) => {
        $(
            impl PropValue for $ty {}
        )*
    };
}
impl_prop_value!(char, bool);
impl_prop_value!(f32, f64);
impl_prop_value!(u8, u16, u32, u64, u128, usize);
impl_prop_value!(i8, i16, i32, i64, i128, isize);
impl_prop_value!(String, Cow<'static, str>, &'static str, Rc<str>, Arc<str>);
impl_prop_value!(PathBuf, Cow<'static, Path>, &'static Path);

impl<T: PropValue> PropValue for Option<T> {}
impl<T: PropValue> PropValue for Box<T> {}
impl<T: PropValue> PropValue for Vec<T> {}
impl<T: PropValue> PropValue for HashSet<T> {}
impl<T: PropValue> PropValue for Rc<T> {}
impl<T: PropValue> PropValue for Arc<T> {}
impl<T: PropValue> PropValue for Rc<[T]> {}
impl<T: PropValue> PropValue for Arc<[T]> {}
impl<T: PropValue> PropValue for [T] {}
impl<T: PropValue, const N: usize> PropValue for [T; N] {}

macro_rules! impl_prop_value_tuple {
    ($($p:ident)*) => {
        impl<$($p: PropValue),*> PropValue for ($($p,)*) {}
    };
}

impl_prop_value_tuple!();
impl_prop_value_tuple!(T1);
impl_prop_value_tuple!(T1 T2);
impl_prop_value_tuple!(T1 T2 T3);
impl_prop_value_tuple!(T1 T2 T3 T4);
impl_prop_value_tuple!(T1 T2 T3 T4 T5);
impl_prop_value_tuple!(T1 T2 T3 T4 T5 T6);
impl_prop_value_tuple!(T1 T2 T3 T4 T5 T6 T7);
impl_prop_value_tuple!(T1 T2 T3 T4 T5 T6 T7 T8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_collections_and_shared_strings_are_property_values() {
        fn into_prop<T: PropValue>(value: T) -> Prop<T> {
            value.into()
        }
        let values = HashSet::from(["compact", "selected"]);
        let prop = into_prop(values.clone());
        assert_eq!(prop.get_clone(), values);
        let values: Rc<[i32]> = Rc::from([1, 2, 3]);
        let prop = into_prop(values.clone());
        assert!(Rc::ptr_eq(prop.as_static().unwrap(), &values));
        let values: Arc<[i32]> = Arc::from([1, 2, 3]);
        let prop = into_prop(values.clone());
        assert!(Arc::ptr_eq(prop.as_static().unwrap(), &values));
        let text: Rc<str> = Rc::from("label");
        let prop = into_prop(text.clone());
        assert!(Rc::ptr_eq(prop.as_static().unwrap(), &text));
        let text: Arc<str> = Arc::from("label");
        let prop = into_prop(text.clone());
        assert!(Arc::ptr_eq(prop.as_static().unwrap(), &text));
    }
}

#[cfg(feature = "impl-cosmic-text")]
mod cosmic_text;
#[cfg(feature = "impl-taffy")]
mod taffy;

#[macro_export]
macro_rules! default_props {
    (@extract_default ($ty:ty, $default:expr)) => {
        Prop::Static($default)
    };

    (@extract_default ($ty:ty, )) => {
        Prop::Static(<$ty>::default())
    };

    (
        $vis:vis struct $name:ident {
            $($field:ident:$ty:ty $(= $default:expr)?),* $(,)?
        }
    ) => {
        #[derive(Clone, Copy)]
        $vis struct $name {
            $(pub $field: Prop<$ty>),*
        }

        impl Default for $name {
            fn default() -> Self {
                Self {
                    $($field: $crate::default_props!(@extract_default ($ty, $($default)?))),*
                }
            }
        }

        impl $name {
            pub fn new() -> Self {
                Self::default()
            }

            $(
                pub fn $field(mut self, value: impl Into<Prop<$ty>>) -> Self {
                    self.$field = value.into();
                    self
                }
            )*
        }
    };
}
