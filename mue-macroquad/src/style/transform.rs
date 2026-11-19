//! Transform functions, projected onto the node's two-dimensional canvas plane.
use super::{Size, SizeExt};
use crate::math::{Matrix, Rect, Vector};
use crate::transition::Interpolatable as _;
use mue_core::prop::PropValue;

#[derive(Clone, Debug, PartialEq)]
pub enum TransformOp {
    Translate(Size, Size),
    TranslateZ(f32),
    Translate3d(Size, Size, f32),
    Scale(f32, f32),
    ScaleZ(f32),
    Scale3d(f32, f32, f32),
    Rotate(f32),
    RotateX(f32),
    RotateY(f32),
    Skew(f32, f32),
    Perspective(f32),
    Matrix(Matrix),
    Matrix3d([f32; 16]),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Transform(pub Vec<TransformOp>);
impl PropValue for Transform {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformOrigin(pub Size, pub Size);
impl Default for TransformOrigin {
    fn default() -> Self {
        Self(Size::Percent(50.), Size::Percent(50.))
    }
}
impl PropValue for TransformOrigin {}

fn length(value: Size, base: f32) -> f32 {
    match value {
        Size::Percent(value) => value * base / 100.,
        value => value.resolve("transform"),
    }
}

impl Transform {
    pub fn resolve(&self, bounds: Rect, origin: TransformOrigin) -> Matrix {
        use glam::Mat4;
        if self.0.is_empty() {
            return Matrix::IDENTITY;
        }
        let origin =
            bounds.min() + Vector::new(length(origin.0, bounds.w), length(origin.1, bounds.h));
        let mut matrix = Mat4::from_translation(origin.extend(0.));
        for op in &self.0 {
            matrix *= match *op {
                TransformOp::Translate(x, y) => Mat4::from_translation(
                    Vector::new(length(x, bounds.w), length(y, bounds.h)).extend(0.),
                ),
                TransformOp::TranslateZ(z) => Mat4::from_translation(glam::vec3(0., 0., z)),
                TransformOp::Translate3d(x, y, z) => {
                    Mat4::from_translation(glam::vec3(length(x, bounds.w), length(y, bounds.h), z))
                }
                TransformOp::Scale(x, y) => Mat4::from_scale(glam::vec3(x, y, 1.)),
                TransformOp::ScaleZ(z) => Mat4::from_scale(glam::vec3(1., 1., z)),
                TransformOp::Scale3d(x, y, z) => Mat4::from_scale(glam::vec3(x, y, z)),
                TransformOp::Rotate(radians) | TransformOp::RotateX(radians) => {
                    if matches!(op, TransformOp::Rotate(_)) {
                        Mat4::from_rotation_z(radians)
                    } else {
                        Mat4::from_rotation_x(radians)
                    }
                }
                TransformOp::RotateY(radians) => Mat4::from_rotation_y(radians),
                TransformOp::Skew(x, y) => Mat4::from_cols_array(&[
                    1.,
                    y.tan(),
                    0.,
                    0.,
                    x.tan(),
                    1.,
                    0.,
                    0.,
                    0.,
                    0.,
                    1.,
                    0.,
                    0.,
                    0.,
                    0.,
                    1.,
                ]),
                TransformOp::Perspective(distance) => Mat4::from_cols_array(&[
                    1.,
                    0.,
                    0.,
                    0.,
                    0.,
                    1.,
                    0.,
                    0.,
                    0.,
                    0.,
                    1.,
                    -1. / distance,
                    0.,
                    0.,
                    0.,
                    1.,
                ]),
                TransformOp::Matrix(matrix) => {
                    let c = matrix.to_cols_array();
                    Mat4::from_cols_array(&[
                        c[0], c[1], 0., c[2], c[3], c[4], 0., c[5], 0., 0., 1., 0., c[6], c[7], 0.,
                        c[8],
                    ])
                }
                TransformOp::Matrix3d(matrix) => Mat4::from_cols_array(&matrix),
            };
        }
        matrix *= Mat4::from_translation((-origin).extend(0.));
        let c = matrix.to_cols_array();
        Matrix::from_cols_array(&[c[0], c[1], c[3], c[4], c[5], c[7], c[12], c[13], c[15]])
    }
}

/// A typed transform declaration accepted by the style builder.
pub enum TransformProp {
    Matrix(mue_core::prop::Prop<Matrix>),
    Functions(mue_core::prop::Prop<Transform>),
}

/// Accept transform function lists and matrices without coupling the reactive core to math types.
pub trait IntoTransformProp {
    fn into_transform_prop(self) -> TransformProp;
}
impl IntoTransformProp for Matrix {
    fn into_transform_prop(self) -> TransformProp {
        TransformProp::Matrix(mue_core::prop::Prop::Static(self))
    }
}
impl IntoTransformProp for Transform {
    fn into_transform_prop(self) -> TransformProp {
        TransformProp::Functions(mue_core::prop::Prop::Static(self))
    }
}
macro_rules! reactive_transform {
    ($ty:ty, $variant:ident) => {
        impl IntoTransformProp for mue_core::prop::Prop<$ty> {
            fn into_transform_prop(self) -> TransformProp {
                TransformProp::$variant(self)
            }
        }
        impl IntoTransformProp for mue_core::signal::ReadSignal<$ty> {
            fn into_transform_prop(self) -> TransformProp {
                TransformProp::$variant(self.into())
            }
        }
        impl IntoTransformProp for mue_core::signal::Signal<$ty> {
            fn into_transform_prop(self) -> TransformProp {
                TransformProp::$variant(self.into())
            }
        }
    };
}
reactive_transform!(Matrix, Matrix);
reactive_transform!(Transform, Functions);

impl crate::transition::Interpolatable for TransformOrigin {
    fn can_interpolate(from: &Self, to: &Self) -> bool {
        Size::can_interpolate(&from.0, &to.0) && Size::can_interpolate(&from.1, &to.1)
    }
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        Self(
            Size::interpolate(&from.0, &to.0, progress),
            Size::interpolate(&from.1, &to.1, progress),
        )
    }
}

impl TransformOp {
    fn identity(&self) -> Self {
        match *self {
            Self::Translate(x, y) => Self::Translate(zero(x), zero(y)),
            Self::TranslateZ(_) => Self::TranslateZ(0.),
            Self::Translate3d(x, y, _) => Self::Translate3d(zero(x), zero(y), 0.),
            Self::Scale(..) => Self::Scale(1., 1.),
            Self::ScaleZ(_) => Self::ScaleZ(1.),
            Self::Scale3d(..) => Self::Scale3d(1., 1., 1.),
            Self::Rotate(_) => Self::Rotate(0.),
            Self::RotateX(_) => Self::RotateX(0.),
            Self::RotateY(_) => Self::RotateY(0.),
            Self::Skew(..) => Self::Skew(0., 0.),
            Self::Perspective(value) => Self::Perspective(value),
            Self::Matrix(_) => Self::Matrix(Matrix::IDENTITY),
            Self::Matrix3d(_) => Self::Matrix3d(glam::Mat4::IDENTITY.to_cols_array()),
        }
    }
}

fn zero(size: Size) -> Size {
    match size {
        Size::Px(_) => Size::Px(0.),
        Size::Percent(_) => Size::Percent(0.),
        Size::Vw(_) => Size::Vw(0.),
        Size::Vh(_) => Size::Vh(0.),
        Size::Auto => Size::Auto,
    }
}

impl crate::transition::Interpolatable for Transform {
    fn can_interpolate(from: &Self, to: &Self) -> bool {
        if from.0.is_empty() || to.0.is_empty() {
            return true;
        }
        from.0.len() == to.0.len()
            && from
                .0
                .iter()
                .zip(&to.0)
                .all(|(from, to)| transform_op_can_interpolate(from, to))
    }
    fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        if !Self::can_interpolate(from, to) {
            return to.clone();
        }
        let from_identity;
        let to_identity;
        let from = if from.0.is_empty() {
            from_identity = Self(to.0.iter().map(TransformOp::identity).collect());
            &from_identity
        } else {
            from
        };
        let to = if to.0.is_empty() {
            to_identity = Self(from.0.iter().map(TransformOp::identity).collect());
            &to_identity
        } else {
            to
        };
        Self(
            from.0
                .iter()
                .zip(&to.0)
                .map(|(from, to)| match (from, to) {
                    (TransformOp::Translate(ax, ay), TransformOp::Translate(bx, by)) => {
                        TransformOp::Translate(
                            Size::interpolate(ax, bx, progress),
                            Size::interpolate(ay, by, progress),
                        )
                    }
                    (TransformOp::TranslateZ(a), TransformOp::TranslateZ(b)) => {
                        TransformOp::TranslateZ(f32::interpolate(a, b, progress))
                    }
                    (
                        TransformOp::Translate3d(ax, ay, az),
                        TransformOp::Translate3d(bx, by, bz),
                    ) => TransformOp::Translate3d(
                        Size::interpolate(ax, bx, progress),
                        Size::interpolate(ay, by, progress),
                        f32::interpolate(az, bz, progress),
                    ),
                    (TransformOp::Scale(ax, ay), TransformOp::Scale(bx, by)) => TransformOp::Scale(
                        f32::interpolate(ax, bx, progress),
                        f32::interpolate(ay, by, progress),
                    ),
                    (TransformOp::ScaleZ(a), TransformOp::ScaleZ(b)) => {
                        TransformOp::ScaleZ(f32::interpolate(a, b, progress))
                    }
                    (TransformOp::Scale3d(ax, ay, az), TransformOp::Scale3d(bx, by, bz)) => {
                        TransformOp::Scale3d(
                            f32::interpolate(ax, bx, progress),
                            f32::interpolate(ay, by, progress),
                            f32::interpolate(az, bz, progress),
                        )
                    }
                    (TransformOp::Rotate(a), TransformOp::Rotate(b)) => {
                        TransformOp::Rotate(f32::interpolate(a, b, progress))
                    }
                    (TransformOp::RotateX(a), TransformOp::RotateX(b)) => {
                        TransformOp::RotateX(f32::interpolate(a, b, progress))
                    }
                    (TransformOp::RotateY(a), TransformOp::RotateY(b)) => {
                        TransformOp::RotateY(f32::interpolate(a, b, progress))
                    }
                    (TransformOp::Perspective(a), TransformOp::Perspective(b)) => {
                        TransformOp::Perspective(f32::interpolate(a, b, progress))
                    }
                    (TransformOp::Skew(ax, ay), TransformOp::Skew(bx, by)) => TransformOp::Skew(
                        f32::interpolate(ax, bx, progress),
                        f32::interpolate(ay, by, progress),
                    ),
                    (TransformOp::Matrix(a), TransformOp::Matrix(b)) => {
                        TransformOp::Matrix(Matrix::from_cols_array(&<[f32; 9]>::interpolate(
                            &a.to_cols_array(),
                            &b.to_cols_array(),
                            progress,
                        )))
                    }
                    (TransformOp::Matrix3d(a), TransformOp::Matrix3d(b)) => {
                        TransformOp::Matrix3d(<[f32; 16]>::interpolate(a, b, progress))
                    }
                    _ => to.clone(),
                })
                .collect(),
        )
    }
}

fn transform_op_can_interpolate(from: &TransformOp, to: &TransformOp) -> bool {
    use TransformOp::*;
    match (from, to) {
        (Translate(ax, ay), Translate(bx, by)) => {
            Size::can_interpolate(ax, bx) && Size::can_interpolate(ay, by)
        }
        (TranslateZ(_), TranslateZ(_))
        | (Scale(..), Scale(..))
        | (ScaleZ(_), ScaleZ(_))
        | (Scale3d(..), Scale3d(..))
        | (Rotate(_), Rotate(_))
        | (RotateX(_), RotateX(_))
        | (RotateY(_), RotateY(_))
        | (Skew(..), Skew(..))
        | (Perspective(_), Perspective(_))
        | (Matrix(_), Matrix(_))
        | (Matrix3d(_), Matrix3d(_)) => true,
        (Translate3d(ax, ay, _), Translate3d(bx, by, _)) => {
            Size::can_interpolate(ax, bx) && Size::can_interpolate(ay, by)
        }
        _ => false,
    }
}
