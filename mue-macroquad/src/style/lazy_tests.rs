use super::*;
use crate::node::{IntoNode, Node};
use mue_core::{
    runtime::resource_counts,
    signal::{Signal, signal},
};

fn same<T: Clone + PartialEq + 'static>(left: Prop<T>, right: Prop<T>) {
    // Resource snapshots around callers prove no second computed/tween is created.
    assert!(matches!(left, Prop::Dynamic(_)) && matches!(right, Prop::Dynamic(_)));
    assert!(left.get_clone() == right.get_clone());
}

#[test]
fn seal_allocates_no_fields_and_consumers_share_property_and_rectangle_bindings() {
    let before = resource_counts();
    let owner = OwnedScope::new();
    owner.run(|| {
        let theme = Signal::shallow(Rc::new(Style::new()));
        let baseline = resource_counts();
        let mut style = Style::themed_defaults(*theme, Style::new().width(12.));
        assert_eq!(resource_counts(), baseline);
        let width = style.declared_width().unwrap();
        let one = resource_counts();
        assert_eq!(one.signals, baseline.signals + 1);
        assert_eq!(one.effects, baseline.effects + 1);
        same(width, style.take_width());
        same(width, style.declared_width().unwrap());
        assert_eq!(resource_counts(), one);
        let left = style.take_padding_left();
        let padding = style.take_padding();
        let rect = resource_counts();
        same(padding, style.take_padding());
        same(left, style.declared_padding_left().unwrap());
        assert_eq!(resource_counts(), rect);
        theme.set(Rc::new(Style::new().width(24.).padding_left(7.)));
        assert_eq!(width.get(), Size::Px(24.));
        assert_eq!(left.get(), Size::Px(7.));
        assert_eq!(padding.get().left, Size::Px(7.));
        theme.set(Rc::new(Style::new()));
        assert_eq!(width.get(), Size::Px(12.));
        assert_eq!(padding.get().left, Size::Px(0.));
    });
    drop(owner);
    assert_eq!(resource_counts(), before);
}

#[test]
fn first_consumption_uses_seal_owner_and_ignores_temporary_caller_lifetime() {
    let before = resource_counts();
    let owner = OwnedScope::new();
    let (mut style, theme) = owner.run(|| {
        let theme = Signal::shallow(Rc::new(Style::new()));
        (Style::themed_defaults(*theme, Style::new()), theme)
    });
    let temporary = OwnedScope::new();
    let width = temporary.run(|| style.take_width());
    drop(temporary);
    theme.set(Rc::new(Style::new().width(23.)));
    assert_eq!(width.get(), Size::Px(23.));
    drop(style);
    theme.set(Rc::new(Style::new()));
    assert_eq!(width.get(), Size::Auto);
    drop(owner);
    if let Prop::Dynamic(signal) = width {
        assert!(signal.is_disposed());
    }
    assert_eq!(resource_counts(), before);
}

#[test]
fn fallback_derived_owner_survives_consumed_style_and_releases_with_projection() {
    let before = resource_counts();
    let external = OwnedScope::new();
    let input = external.run(|| signal(10.));
    let owner = OwnedScope::new();
    let (width, derived, weak) = owner.run(|| {
        let derived_owner = Rc::new(OwnedScope::new_inherited());
        let weak = Rc::downgrade(&derived_owner);
        let derived = derived_owner.run(|| computed(move |_| Size::Px(input.get())));
        let mut fallback = Style::new().width(derived);
        fallback.theme_owners.push(derived_owner);
        let theme = Signal::shallow(Rc::new(Style::new()));
        let mut style = Style::themed_defaults(*theme, fallback);
        let width = style.take_width();
        drop(style);
        (width, derived, weak)
    });
    assert!(weak.upgrade().is_some());
    input.set(19.);
    assert_eq!(width.get(), Size::Px(19.));
    drop(owner);
    assert!(derived.is_disposed());
    assert!(weak.upgrade().is_none());
    assert!(!input.is_disposed());
    drop(external);
    assert_eq!(resource_counts(), before);
}

#[test]
fn foreground_is_captured_at_seal_and_unused_fields_do_not_project() {
    let before = resource_counts();
    let owner = OwnedScope::new();
    owner.run(|| {
        let inherited = signal(Brush::Solid(Color::BLACK));
        let theme = Signal::shallow(Rc::new(Style::new()));
        let storage = Rc::new(std::cell::RefCell::new(None));
        let saved = storage.clone();
        let parent = crate::theme::provide_foreground(inherited, || {
            Node::build(|| {
                *saved.borrow_mut() = Some(Style::themed_defaults(*theme, Style::new()));
            })
        })
        .into_node();
        let mut style = storage.borrow_mut().take().unwrap();
        let width = style.take_width();
        let counts = resource_counts();
        theme.set(Rc::new(Style::new().font_italic(true)));
        assert_eq!(resource_counts(), counts);
        assert_eq!(width.get(), Size::Auto);
        let mut color = None;
        let other = crate::theme::provide_foreground(Color::WHITE, || {
            Node::build(|| {
                color = style.declared_color();
            })
        })
        .into_node();
        let color = color.unwrap();
        assert_eq!(color.get_clone(), Brush::Solid(Color::BLACK));
        inherited.set(Brush::Solid(Color::new(0.2, 0.3, 0.4, 1.)));
        assert_eq!(color.get_clone(), inherited.get_clone());
        drop(other);
        drop(style);
        drop(parent);
    });
    drop(owner);
    assert_eq!(resource_counts(), before);
}

#[test]
fn lazy_transition_is_shared_and_dynamic_curve_applies_to_later_rule_changes() {
    let before = resource_counts();
    let owner = OwnedScope::new();
    owner.run(|| {
        let theme = Signal::shallow(Rc::new(Style::new().width(10.)));
        let curve = signal(crate::transition::Transition::Instant);
        let mut style = Style::themed_defaults(*theme, Style::new());
        style.transition = Some(curve.into());
        crate::transition::bind_style(&mut style);
        let width = style.take_width();
        let counts = resource_counts();
        same(width, style.declared_width().unwrap());
        assert_eq!(resource_counts(), counts);
        theme.set(Rc::new(Style::new().width(20.)));
        assert_eq!(width.get(), Size::Px(20.));
        curve.set(crate::transition::Transition::linear(1.));
        theme.set(Rc::new(Style::new().width(40.)));
        assert_eq!(width.get(), Size::Px(20.));
        crate::transition::advance(0.5);
        assert_eq!(width.get(), Size::Px(30.));
        crate::transition::advance(0.5);
        assert_eq!(width.get(), Size::Px(40.));
        theme.set(Rc::new(Style::new()));
        assert_eq!(width.get(), Size::Auto);
    });
    drop(owner);
    assert_eq!(resource_counts(), before);
}

#[test]
fn rectangle_setters_invalidate_only_the_aggregate_for_future_consumers() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let theme = Signal::shallow(Rc::new(Style::new()));
        let mut style = Style::themed_defaults(*theme, Style::new());
        let previous = style.take_padding();
        style = style.padding_left(9.);
        assert_eq!(style.declared_padding_left().unwrap().get(), Size::Px(9.));
        assert_eq!(style.take_padding().get().left, Size::Px(9.));
        style = style.padding_y(7.);
        assert_eq!(style.take_padding().get().top, Size::Px(7.));
        style = style.padding((3.,));
        assert_eq!(style.take_padding().get().right, Size::Px(3.));
        assert_eq!(previous.get().left, Size::Px(0.));
    });
}

#[test]
fn static_instant_theme_slot_reuses_projection_without_tween_graph() {
    let before = resource_counts();
    let owner = OwnedScope::new();
    owner.run(|| {
        let theme = Signal::shallow(Rc::new(Style::new().width(10.)));
        let mut style = Style::themed_defaults(*theme, Style::new());
        style.transition = Some(crate::transition::Transition::Instant.into());
        crate::transition::bind_style(&mut style);
        let baseline = resource_counts();
        let width = style.take_width();
        let after = resource_counts();
        assert_eq!(after.signals, baseline.signals + 1);
        assert_eq!(after.effects, baseline.effects + 1);
        same(width, style.declared_width().unwrap());
        assert_eq!(resource_counts(), after);
        theme.set(Rc::new(Style::new().width(20.)));
        assert_eq!(width.get(), Size::Px(20.));
        crate::transition::advance(1.);
        assert_eq!(width.get(), Size::Px(20.));
    });
    drop(owner);
    crate::transition::advance(1.);
    assert_eq!(resource_counts(), before);
}

#[test]
fn lazy_matrix_transition_preserves_column_interpolation() {
    let before = resource_counts();
    let owner = OwnedScope::new();
    owner.run(|| {
        let theme = Signal::shallow(Rc::new(Style::new()));
        let mut style = Style::themed_defaults(*theme, Style::new());
        style.transition = Some(crate::transition::Transition::linear(1.).into());
        crate::transition::bind_style(&mut style);
        let matrix = style.take_matrix_transform();
        theme.set(Rc::new(Style::new().transform(Matrix::from_translation(
            crate::math::Vector::new(10., 20.),
        ))));
        assert_eq!(matrix.get(), Matrix::IDENTITY);
        crate::transition::advance(0.5);
        let point = crate::math::project_point(matrix.get(), crate::math::Vector::ZERO);
        assert_eq!(point, crate::math::Vector::new(5., 10.));
        crate::transition::advance(0.5);
        let point = crate::math::project_point(matrix.get(), crate::math::Vector::ZERO);
        assert_eq!(point, crate::math::Vector::new(10., 20.));
    });
    drop(owner);
    assert_eq!(resource_counts(), before);
}

#[test]
fn composed_sealed_style_keeps_dynamic_properties_and_explicit_precedence() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let theme = Signal::shallow(Rc::new(Style::new().width(21.).height(22.)));
        let sealed = Style::themed_defaults(*theme, Style::new().width(12.));
        let mut style = Style::new().width(99.).merge(sealed);
        let width = style.take_width();
        let height = style.take_height();
        assert_eq!(width.get(), Size::Px(99.));
        assert_eq!(height.get(), Size::Px(22.));
        theme.set(Rc::new(Style::new().height(35.)));
        assert_eq!(width.get(), Size::Px(99.));
        assert_eq!(height.get(), Size::Px(35.));
    });
}
