use super::*;
use mue_core::{
    runtime::resource_counts,
    scope::OwnedScope,
    signal::{Access, signal},
};

#[test]
fn explicit_property_access_reuses_the_source_without_allocating() {
    let before = resource_counts();
    let owner = OwnedScope::new();
    owner.run(|| {
        let source = signal(Size::Px(12.));
        let mut style = Style::new().width(source);
        let baseline = resource_counts();
        let declared = style.declared_width().unwrap();
        let taken = style.take_width();
        assert_eq!(resource_counts(), baseline);
        assert!(style.declared_width().is_none());
        source.set(Size::Px(24.));
        assert_eq!(declared.get(), Size::Px(24.));
        assert_eq!(taken.get(), Size::Px(24.));
        assert_eq!(style.take_width().get(), Size::Auto);
    });
    drop(owner);
    assert_eq!(resource_counts(), before);
}

#[test]
fn rectangle_consumption_preserves_old_values_after_later_setters() {
    let before = resource_counts();
    let owner = OwnedScope::new();
    owner.run(|| {
        let left = signal(Size::Px(2.));
        let mut style = Style::new().padding_left(left);
        let previous = style.take_padding();
        assert_eq!(previous.get().left, Size::Px(2.));
        style = style.padding_left(9.);
        assert_eq!(style.declared_padding_left().unwrap().get(), Size::Px(9.));
        assert_eq!(style.take_padding().get().left, Size::Px(9.));
        style = style.padding_y(7.);
        assert_eq!(style.take_padding().get().top, Size::Px(7.));
        style = style.padding((3.,));
        assert_eq!(style.take_padding().get().right, Size::Px(3.));
        left.set(Size::Px(5.));
        assert_eq!(previous.get().left, Size::Px(5.));
    });
    drop(owner);
    assert_eq!(resource_counts(), before);
}

#[test]
fn explicit_transition_binding_is_shared_and_uses_the_latest_curve() {
    let before = resource_counts();
    let owner = OwnedScope::new();
    owner.run(|| {
        let source = signal(Size::Px(10.));
        let curve = signal(crate::transition::Transition::Instant);
        let mut style = Style::new().width(source).transition(curve);
        crate::transition::bind_style(&mut style);
        let declared = style.declared_width().unwrap();
        let counts = resource_counts();
        let width = style.take_width();
        assert_eq!(resource_counts(), counts);
        source.set(Size::Px(20.));
        assert_eq!(width.get(), Size::Px(20.));
        assert_eq!(declared.get(), width.get());
        curve.set(crate::transition::Transition::linear(1.));
        source.set(Size::Px(40.));
        assert_eq!(width.get(), Size::Px(20.));
        crate::transition::advance(0.5);
        assert_eq!(width.get(), Size::Px(30.));
        assert_eq!(declared.get(), width.get());
        crate::transition::advance(0.5);
        assert_eq!(width.get(), Size::Px(40.));
        source.set(Size::Auto);
        assert_eq!(width.get(), Size::Auto);
    });
    drop(owner);
    assert_eq!(resource_counts(), before);
}

#[test]
fn explicit_matrix_transition_preserves_column_interpolation() {
    let before = resource_counts();
    let owner = OwnedScope::new();
    owner.run(|| {
        let source = signal(Matrix::IDENTITY);
        let mut style = Style::new()
            .transform(source)
            .transition(crate::transition::Transition::linear(1.));
        crate::transition::bind_style(&mut style);
        let matrix = style.take_matrix_transform();
        source.set(Matrix::from_translation(crate::math::Vector::new(10., 20.)));
        assert_eq!(matrix.get(), Matrix::IDENTITY);
        crate::transition::advance(0.5);
        assert_eq!(
            crate::math::project_point(matrix.get(), crate::math::Vector::ZERO),
            crate::math::Vector::new(5., 10.)
        );
        crate::transition::advance(0.5);
        assert_eq!(
            crate::math::project_point(matrix.get(), crate::math::Vector::ZERO),
            crate::math::Vector::new(10., 20.)
        );
    });
    drop(owner);
    assert_eq!(resource_counts(), before);
}

#[test]
fn composed_style_keeps_dynamic_properties_and_explicit_precedence() {
    let before = resource_counts();
    let owner = OwnedScope::new();
    owner.run(|| {
        let height = signal(Size::Px(22.));
        let fallback = Style::new().width(21.).height(height);
        let mut style = Style::new().width(99.);
        style.provide_defaults(fallback);
        let width = style.take_width();
        let actual_height = style.take_height();
        assert_eq!(width.get(), Size::Px(99.));
        assert_eq!(actual_height.get(), Size::Px(22.));
        height.set(Size::Px(35.));
        assert_eq!(width.get(), Size::Px(99.));
        assert_eq!(actual_height.get(), Size::Px(35.));
    });
    drop(owner);
    assert_eq!(resource_counts(), before);
}
