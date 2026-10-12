use std::{
    borrow::Cow,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use crate::{
    runtime::Runtime,
    signal::{Access, ReadSignal, SignalId},
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

pub trait PropValue {}

impl<T> From<ReadSignal<T>> for Prop<T> {
    fn from(signal: ReadSignal<T>) -> Self {
        Self::Dynamic(signal)
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
impl_prop_value!(String, Cow<'static, str>, &'static str);
impl_prop_value!(PathBuf, Cow<'static, Path>, &'static Path);

impl<T: PropValue> PropValue for Option<T> {}
impl<T: PropValue> PropValue for Box<T> {}
impl<T: PropValue> PropValue for Vec<T> {}
impl<T: PropValue> PropValue for Rc<T> {}
impl<T: PropValue> PropValue for Arc<T> {}
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

#[cfg(feature = "impl-taffy")]
impl_prop_value!(
    taffy::style::Dimension,
    taffy::style::LengthPercentage,
    taffy::style::LengthPercentageAuto,
    taffy::style::AlignContent,
    taffy::style::AlignItems,
    taffy::style::AvailableSpace,
    taffy::style::BoxGenerationMode,
    taffy::style::BoxSizing,
    taffy::style::Display,
    taffy::style::FlexDirection,
    taffy::style::FlexWrap,
    taffy::style::GridAutoFlow,
    taffy::style::GridPlacement,
    taffy::style::Overflow,
    taffy::style::Position,
    taffy::style::RepetitionCount,
    taffy::style::TextAlign,
);

#[cfg(feature = "impl-taffy")]
impl<T> PropValue for taffy::Rect<T> {}

#[cfg(feature = "impl-taffy")]
impl<S, Repetition> PropValue for taffy::GenericGridTemplateComponent<S, Repetition>
where
    S: taffy::CheapCloneStr,
    Repetition: taffy::GenericRepetition<CustomIdent = S>,
{
}

#[cfg(feature = "impl-taffy")]
impl<S> PropValue for taffy::GridTemplateComponent<S> where S: taffy::CheapCloneStr {}

#[cfg(feature = "impl-cosmic-text")]
impl PropValue for cosmic_text::FamilyOwned {}

#[cfg(feature = "impl-cosmic-text")]
impl From<&str> for Prop<cosmic_text::FamilyOwned> {
    fn from(name: &str) -> Self {
        Prop::Static(cosmic_text::FamilyOwned::Name(name.into()))
    }
}

#[cfg(feature = "impl-cosmic-text")]
impl From<String> for Prop<cosmic_text::FamilyOwned> {
    fn from(name: String) -> Self {
        Prop::Static(cosmic_text::FamilyOwned::Name(name.into()))
    }
}

#[cfg(feature = "impl-cosmic-text")]
impl PropValue for cosmic_text::Weight {}

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
