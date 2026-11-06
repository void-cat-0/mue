use super::*;
use mue_core::{
    effect::watch_effect,
    runtime::{batch, resource_counts},
    signal::signal,
};
use taffy::{Dimension, LengthPercentage, LengthPercentageAuto};

fn notifications(style: ReadSignal<taffy::Style>) -> Rc<Cell<usize>> {
    let count = Rc::new(Cell::new(0));
    watch_effect({
        let count = count.clone();
        move || {
            style.get_clone();
            count.set(count.get() + 1);
        }
    });
    count
}

#[test]
fn batched_inputs_notify_final_consumer_once_with_all_fields_current() {
    let before = resource_counts();
    let owner = OwnedScope::new();
    owner.run(|| {
        let width = signal(Size::Auto);
        let height = signal(Size::Auto);
        let min = signal(Size::Auto);
        let max = signal(Size::Auto);
        let column = signal(px(0.));
        let row = signal(px(0.));
        let x = signal(Overflow::Visible);
        let y = signal(Overflow::Visible);
        let left = signal(px(0.));
        let right = signal(px(0.));
        let top = signal(px(0.));
        let bottom = signal(px(0.));
        let mut raw = Style::new()
            .width(width)
            .height(height)
            .min_width(min)
            .min_height(min)
            .max_width(max)
            .max_height(max)
            .column_gap(column)
            .row_gap(row)
            .overflow_x(x)
            .overflow_y(y)
            .margin((left, top, bottom, right))
            .padding((left, top, bottom, right))
            .border((left, top, bottom, right))
            .inset((left, top, bottom, right));
        let style = raw.build_taffy();
        let count = notifications(style);
        assert_eq!(count.get(), 1);
        for step in [1., 2.] {
            batch(|| {
                width.set(px(100. * step));
                height.set(percent(50. * step));
                min.set(px(10. * step));
                max.set(percent(90. * step));
                column.set(px(3. * step));
                row.set(percent(4. * step));
                x.set(Overflow::Hidden);
                y.set(Overflow::Scroll);
                left.set(px(step));
                right.set(px(2. * step));
                top.set(percent(3. * step));
                bottom.set(percent(4. * step));
            });
            assert_eq!(count.get(), step as usize + 1);
            let current = style.get_clone();
            assert_eq!(
                current.size,
                taffy::Size {
                    width: Dimension::length(100. * step),
                    height: Dimension::percent(50. * step / 100.),
                }
            );
            assert_eq!(
                current.min_size,
                taffy::Size {
                    width: Dimension::length(10. * step),
                    height: Dimension::length(10. * step),
                }
            );
            assert_eq!(
                current.max_size,
                taffy::Size {
                    width: Dimension::percent(90. * step / 100.),
                    height: Dimension::percent(90. * step / 100.),
                }
            );
            assert_eq!(
                current.gap,
                taffy::Size {
                    width: LengthPercentage::length(3. * step),
                    height: LengthPercentage::percent(4. * step / 100.),
                }
            );
            assert_eq!(
                current.overflow,
                Point {
                    x: Overflow::Hidden,
                    y: Overflow::Scroll
                }
            );
            let rect = taffy::Rect {
                left: LengthPercentage::length(step),
                right: LengthPercentage::length(2. * step),
                top: LengthPercentage::percent(3. * step / 100.),
                bottom: LengthPercentage::percent(4. * step / 100.),
            };
            assert_eq!(current.padding, rect);
            assert_eq!(current.border, rect);
            let auto_rect = taffy::Rect {
                left: LengthPercentageAuto::length(step),
                right: LengthPercentageAuto::length(2. * step),
                top: LengthPercentageAuto::percent(3. * step / 100.),
                bottom: LengthPercentageAuto::percent(4. * step / 100.),
            };
            assert_eq!(current.margin, auto_rect);
            assert_eq!(current.inset, auto_rect);
        }
    });
    drop(owner);
    assert_eq!(resource_counts(), before);
}

#[test]
fn equal_lowered_style_suppresses_notifications_across_distinct_source_units() {
    // Initialize the permanent viewport signal before taking the cleanup snapshot.
    let viewport = crate::viewport::read().get_untracked();
    crate::viewport::set(crate::viewport::Viewport {
        width: 200.,
        height: 100.,
    });
    let before = resource_counts();
    let owner = OwnedScope::new();
    owner.run(|| {
        let width = signal(px(100.));
        let height = signal(px(50.));
        let side = signal(px(10.));
        let mut raw = Style::new()
            .width(width)
            .height(height)
            .column_gap(side)
            .row_gap(side)
            .padding((side,))
            .border((side,))
            .margin((side,))
            .inset((side,));
        let style = raw.build_taffy();
        let count = notifications(style);
        let initial = style.get_clone();
        batch(|| {
            width.set(vw(50.));
            height.set(vh(50.));
            side.set(vw(5.));
        });
        assert_eq!(style.get_clone(), initial);
        assert_eq!(count.get(), 1);
        batch(|| {
            width.set(px(999.));
            width.set(px(100.));
            side.set(vh(10.));
        });
        assert_eq!(style.get_clone(), initial);
        assert_eq!(count.get(), 1);
        width.set(percent(50.));
        assert_eq!(style.get_clone().size.width, Dimension::percent(0.5));
        assert_eq!(count.get(), 2);
        batch(|| {
            width.set(Size::Auto);
            height.set(Size::Auto);
        });
        assert_eq!(
            style.get_clone().size,
            taffy::Size {
                width: Dimension::auto(),
                height: Dimension::auto(),
            }
        );
        assert_eq!(count.get(), 3);
    });
    drop(owner);
    assert_eq!(resource_counts(), before);
    crate::viewport::set(viewport);
}

#[test]
fn projected_tokens_publish_one_complete_layout_per_replacement() {
    #[derive(Clone, Copy, PartialEq)]
    struct Tokens {
        width: f32,
        height: f32,
        padding: f32,
    }

    let before = resource_counts();
    let owner = OwnedScope::new();
    owner.run(|| {
        let initial = Tokens {
            width: 10.,
            height: 20.,
            padding: 2.,
        };
        let source = signal(initial);
        let mut raw = Style::new()
            .width(source.map(|t| t.width))
            .height(source.map(|t| t.height))
            .padding((source.map(|t| t.padding),));
        crate::transition::bind_style(&mut raw);
        let style = raw.build_taffy();
        let count = notifications(style);
        let initial_layout = style.get_clone();
        let computes = crate::layout::style_computes();
        source.set(Tokens {
            width: 30.,
            height: 40.,
            padding: 7.,
        });
        assert_eq!(count.get(), 2);
        assert_eq!(crate::layout::style_computes() - computes, 1);
        let current = style.get_clone();
        assert_eq!(current.size.width, Dimension::length(30.));
        assert_eq!(current.size.height, Dimension::length(40.));
        assert_eq!(
            current.padding,
            taffy::Rect {
                left: LengthPercentage::length(7.),
                right: LengthPercentage::length(7.),
                top: LengthPercentage::length(7.),
                bottom: LengthPercentage::length(7.),
            }
        );
        source.set(initial);
        assert_eq!(style.get_clone(), initial_layout);
        assert_eq!(count.get(), 3);
        assert_eq!(crate::layout::style_computes() - computes, 2);
    });
    drop(owner);
    assert_eq!(resource_counts(), before);
}

#[test]
fn transition_outputs_coalesce_batched_layout_properties() {
    let before = resource_counts();
    let owner = OwnedScope::new();
    owner.run(|| {
        let width = signal(px(10.));
        let height = signal(px(20.));
        let gap = signal(px(2.));
        let padding = signal(px(3.));
        let mut raw = Style::new()
            .width(width)
            .height(height)
            .column_gap(gap)
            .row_gap(gap)
            .padding((padding,))
            .transition(crate::transition::Transition::linear(1.));
        crate::transition::bind_style(&mut raw);
        let style = raw.build_taffy();
        let count = notifications(style);
        assert_eq!(count.get(), 1);

        batch(|| {
            width.set(px(30.));
            height.set(px(40.));
            gap.set(px(5.));
            padding.set(px(7.));
        });

        assert_eq!(count.get(), 1);
        crate::transition::advance(0.5);
        assert_eq!(count.get(), 2);
        let current = style.get_clone();
        assert_eq!(current.size.width, Dimension::length(20.));
        assert_eq!(current.size.height, Dimension::length(30.));
        assert_eq!(current.gap.width, LengthPercentage::length(3.5));
        assert_eq!(current.gap.height, LengthPercentage::length(3.5));
        assert_eq!(current.padding.left, LengthPercentage::length(5.));
        crate::transition::advance(0.5);
        assert_eq!(count.get(), 3);
    });
    drop(owner);
    assert_eq!(resource_counts(), before);
}

#[test]
fn four_side_transitions_publish_consistent_values_once_per_advance() {
    let before = resource_counts();
    let owner = OwnedScope::new();
    owner.run(|| {
        let left = signal(px(1.));
        let top = signal(px(2.));
        let bottom = signal(px(3.));
        let right = signal(px(4.));
        let mut raw = Style::new()
            .padding((left, top, bottom, right))
            .transition(crate::transition::Transition::linear(1.));
        crate::transition::bind_style(&mut raw);
        let style = raw.build_taffy();
        let count = Rc::new(Cell::new(0));
        let expected = Rc::new(Cell::new(1.));
        watch_effect({
            let count = count.clone();
            let expected = expected.clone();
            move || {
                let unit = expected.get();
                assert_eq!(
                    style.get_clone().padding,
                    taffy::Rect {
                        left: LengthPercentage::length(unit),
                        top: LengthPercentage::length(2. * unit),
                        bottom: LengthPercentage::length(3. * unit),
                        right: LengthPercentage::length(4. * unit),
                    }
                );
                count.set(count.get() + 1);
            }
        });
        batch(|| {
            left.set(px(11.));
            top.set(px(22.));
            bottom.set(px(33.));
            right.set(px(44.));
        });
        assert_eq!(count.get(), 1);
        expected.set(6.);
        crate::transition::advance(0.5);
        assert_eq!(count.get(), 2);
        expected.set(11.);
        crate::transition::advance(0.5);
        assert_eq!(count.get(), 3);
        crate::transition::advance(0.5);
        assert_eq!(count.get(), 3);
    });
    drop(owner);
    assert_eq!(resource_counts(), before);
}

#[test]
fn raw_dynamic_build_adds_only_final_computed_and_releases_consumer_resources() {
    let before = resource_counts();
    let source_owner = OwnedScope::new();
    let (size, side, overflow) =
        source_owner.run(|| (signal(px(10.)), signal(px(2.)), signal(Overflow::Visible)));
    let sources = resource_counts();
    let owner = OwnedScope::new();
    let (style, count) = owner.run(|| {
        let mut raw = Style::new()
            .width(size)
            .height(size)
            .min_width(size)
            .min_height(size)
            .max_width(size)
            .max_height(size)
            .column_gap(side)
            .row_gap(side)
            .overflow_x(overflow)
            .overflow_y(overflow)
            .margin((side,))
            .padding((side,))
            .border((side,))
            .inset((side,));
        let baseline = resource_counts();
        let style = raw.build_taffy();
        let built = resource_counts();
        assert!(
            built.signals <= baseline.signals + 1,
            "{baseline:?} -> {built:?}"
        );
        assert!(
            built.effects <= baseline.effects + 1,
            "{baseline:?} -> {built:?}"
        );
        let count = notifications(style);
        (style, count)
    });
    batch(|| {
        size.set(px(20.));
        side.set(px(4.));
        overflow.set(Overflow::Hidden);
    });
    assert_eq!(count.get(), 2);
    assert_eq!(style.get_clone().size.width, Dimension::length(20.));
    drop(owner);
    assert!(style.is_disposed());
    assert!(!size.is_disposed());
    assert_eq!(resource_counts(), sources);
    batch(|| {
        size.set(px(30.));
        side.set(px(6.));
        overflow.set(Overflow::Scroll);
    });
    assert_eq!(count.get(), 2);
    drop(source_owner);
    assert_eq!(resource_counts(), before);
}
