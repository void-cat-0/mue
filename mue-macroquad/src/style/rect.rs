//! Edge shorthands accepted by `padding`, `margin`, `border`, and `inset` setters.

use mue_core::prop::Prop;

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
