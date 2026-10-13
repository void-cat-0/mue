use super::{Prop, PropValue};

macro_rules! reactive_length {
    ($($ty:ty),*) => { $(
        impl From<crate::signal::ReadSignal<f32>> for Prop<$ty> {
            fn from(value: crate::signal::ReadSignal<f32>) -> Self {
                value.map(<$ty>::length).into()
            }
        }
        impl From<Prop<f32>> for Prop<$ty> {
            fn from(value: Prop<f32>) -> Self {
                value.map(<$ty>::length)
            }
        }
    )* };
}
reactive_length!(
    taffy::Dimension,
    taffy::LengthPercentage,
    taffy::LengthPercentageAuto
);

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

impl<T> PropValue for taffy::Rect<T> {}

impl From<f32> for Prop<taffy::Dimension> {
    fn from(value: f32) -> Self {
        Self::Static(taffy::Dimension::length(value))
    }
}
impl From<f32> for Prop<taffy::LengthPercentage> {
    fn from(value: f32) -> Self {
        Self::Static(taffy::LengthPercentage::length(value))
    }
}
impl From<f32> for Prop<taffy::LengthPercentageAuto> {
    fn from(value: f32) -> Self {
        Self::Static(taffy::LengthPercentageAuto::length(value))
    }
}

impl<S, Repetition> PropValue for taffy::GenericGridTemplateComponent<S, Repetition>
where
    S: taffy::CheapCloneStr,
    Repetition: taffy::GenericRepetition<CustomIdent = S>,
{
}

impl<S> PropValue for taffy::GridTemplateComponent<S> where S: taffy::CheapCloneStr {}
