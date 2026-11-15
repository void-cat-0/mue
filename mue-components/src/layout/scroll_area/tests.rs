//! These regressions deliberately prepare and hit-test real trees without rendering.
//! ScrollArea painting and wheel keyboard queries require a Macroquad window.

use super::*;
use mue_macroquad::{event::hit_test::hit_test, layout::with_taffy, node::NodeRef, style::px};

fn assert_vector(actual: Vector, expected: Vector) {
    assert!(
        (actual - expected).abs().max_element() < 0.0001,
        "expected {expected:?}, got {actual:?}"
    );
}

fn assert_rect(actual: Rect, expected: Rect) {
    assert_vector(actual.min(), expected.min());
    assert_vector(actual.size(), expected.size());
}

fn leaf(style: Style) -> (Node, ReadSignal<Rect>) {
    let mut rect = None;
    let node = Node::leaf(style, |style| rect = Some(use_layout(style).rect));
    (node, rect.unwrap())
}

fn hit_path(root: NodeRef, position: Vector) -> Vec<NodeRef> {
    hit_test(root, position).into_iter().collect()
}

// NodeRef intentionally has no Debug implementation. Keep path comparisons useful
// by reporting the first mismatching depth instead of formatting node handles.
macro_rules! assert_path_eq {
    ($actual:expr, $expected:expr $(, $context:literal)?) => {{
        let actual = &$actual;
        let expected = &$expected;
        assert!(
            actual == expected,
            "hit path mismatch: actual length {}, expected length {}, first differing depth {:?}: {}",
            actual.len(),
            expected.len(),
            actual.iter().zip(expected.iter()).position(|(a, b)| a != b),
            concat!($( $context, )? ""),
        );
    }};
}

fn content(node: NodeRef) -> NodeRef {
    let children = node.children().get_clone();
    assert_eq!(children.len(), 1, "ScrollArea owns one content wrapper");
    children[0]
}

#[test]
fn prepare_publishes_scrolled_child_clip_and_scrollbar_hits_without_render() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let offset = signal(Vector::new(0., 40.));
        let (child, child_rect) = leaf(Style::new().width(116.).height(300.).flex_none());
        let child_ref = *child;
        let scroll = scroll_area()
            .width(140.)
            .height(120.)
            .border((2.,))
            .padding((10.,))
            .offset(offset)
            .children(child)
            .into_node();
        let wrapper = content(*scroll);

        scroll.prepare_layout(Vector::new(400., 300.));

        // The viewport is the content box (12, 12, 116, 96), not the border box.
        assert_rect(child_rect.get(), Rect::new(12., -28., 116., 300.));
        assert_vector(offset.get(), Vector::new(0., 40.));
        assert_path_eq!(
            hit_path(*scroll, Vector::new(30., 25.)),
            vec![child_ref, wrapper, *scroll]
        );
        assert!(hit_path(*scroll, Vector::new(30., 5.)).is_empty());
        assert!(hit_path(*scroll, Vector::new(30., 112.)).is_empty());
        assert_path_eq!(
            hit_path(*scroll, Vector::new(122., 30.)),
            vec![*scroll],
            "the scrollbar track must intercept the child before any paint"
        );
        assert_path_eq!(
            hit_path(*scroll, Vector::new(115., 30.)),
            vec![child_ref, wrapper, *scroll]
        );

        for _ in 0..3 {
            scroll.prepare_layout(Vector::new(400., 300.));
            assert_rect(child_rect.get(), Rect::new(12., -28., 116., 300.));
            assert_vector(offset.get(), Vector::new(0., 40.));
        }
    });
}

#[test]
fn prepare_applies_external_navigation_clamps_shrink_and_resizes_immediately() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let offset = signal(Vector::new(-7., 500.));
        let width = signal(px(100.));
        let height = signal(px(100.));
        let content_height = signal(px(300.));
        let (child, child_rect) = leaf(Style::new().w_full().height(content_height).flex_none());
        let child_ref = *child;
        let scroll = scroll_area()
            .width(width)
            .height(height)
            .offset(offset)
            .children(child)
            .into_node();
        let wrapper = content(*scroll);
        let available = Vector::new(400., 400.);

        scroll.prepare_layout(available);
        assert_vector(offset.get(), Vector::new(0., 200.));
        assert_rect(child_rect.get(), Rect::new(0., -200., 100., 300.));

        offset.set(Vector::new(12., 90.));
        scroll.prepare_layout(available);
        assert_vector(offset.get(), Vector::new(0., 90.));
        assert_rect(child_rect.get(), Rect::new(0., -90., 100., 300.));

        content_height.set(px(130.));
        scroll.prepare_layout(available);
        assert_vector(offset.get(), Vector::new(0., 30.));
        assert_rect(child_rect.get(), Rect::new(0., -30., 100., 130.));
        assert_path_eq!(hit_path(*scroll, Vector::new(94., 10.)), vec![*scroll]);

        // Removing overflow removes the scrollbar's hit exclusion in the same prepare.
        content_height.set(px(80.));
        scroll.prepare_layout(available);
        assert_vector(offset.get(), Vector::ZERO);
        assert_rect(child_rect.get(), Rect::new(0., 0., 100., 80.));
        assert_path_eq!(
            hit_path(*scroll, Vector::new(94., 10.)),
            vec![child_ref, wrapper, *scroll]
        );

        height.set(px(60.));
        width.set(px(140.));
        offset.set(Vector::new(0., 500.));
        scroll.prepare_layout(available);
        assert_vector(offset.get(), Vector::new(0., 20.));
        assert_rect(child_rect.get(), Rect::new(0., -20., 140., 80.));
        assert_path_eq!(hit_path(*scroll, Vector::new(134., 10.)), vec![*scroll]);
        assert_path_eq!(
            hit_path(*scroll, Vector::new(94., 10.)),
            vec![child_ref, wrapper, *scroll],
            "resizing must move the scrollbar track"
        );
        assert!(hit_path(*scroll, Vector::new(30., 65.)).is_empty());

        height.set(px(120.));
        scroll.prepare_layout(available);
        assert_vector(offset.get(), Vector::ZERO);
        assert_rect(child_rect.get(), Rect::new(0., 0., 140., 80.));
        assert_path_eq!(
            hit_path(*scroll, Vector::new(134., 10.)),
            vec![child_ref, wrapper, *scroll]
        );
    });
}

#[test]
fn prepare_masks_horizontal_offsets_and_clamps_both_axes() {
    let owner = OwnedScope::new();
    owner.run(|| {
        for (axis, expected) in [
            (ScrollAxis::Horizontal, Vector::new(160., 0.)),
            (ScrollAxis::Both, Vector::new(160., 160.)),
        ] {
            let offset = signal(Vector::new(999., 999.));
            let (child, rect) = leaf(Style::new().width(260.).height(240.).flex_none());
            let scroll = scroll_area()
                .axis(axis)
                .width(100.)
                .height(80.)
                .offset(offset)
                .children(child)
                .into_node();

            scroll.prepare_layout(Vector::new(400., 300.));

            assert_vector(offset.get(), expected);
            assert_rect(rect.get(), Rect::new(-expected.x, -expected.y, 260., 240.));
            assert_path_eq!(hit_path(*scroll, Vector::new(30., 74.)), vec![*scroll]);
        }
    });
}

#[test]
fn nested_scroll_prepares_absolute_geometry_and_intersects_viewports() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let outer_offset = signal(Vector::new(0., 50.));
        let inner_offset = signal(Vector::new(0., 70.));
        let (child, child_rect) = leaf(Style::new().width(110.).height(300.).flex_none());
        let child_ref = *child;
        let inner = scroll_area()
            .width(120.)
            .height(100.)
            .flex_none()
            .padding((5.,))
            .offset(inner_offset)
            .children(child)
            .into_node();
        let inner_ref = *inner;
        let inner_wrapper = content(inner_ref);
        let outer = scroll_area()
            .width(200.)
            .height(180.)
            .flex_none()
            .padding((10.,))
            .offset(outer_offset)
            .children((
                flexbox().w_full().height(80.).flex_none(),
                inner,
                flexbox().w_full().height(200.).flex_none(),
            ))
            .into_node();
        let outer_ref = *outer;
        let outer_wrapper = content(outer_ref);
        let root = flexbox()
            .width(400.)
            .height(320.)
            .flex_column()
            .padding((30., 40.))
            .children(outer)
            .into_node();
        let path = vec![
            child_ref,
            inner_wrapper,
            inner_ref,
            outer_wrapper,
            outer_ref,
            *root,
        ];

        root.prepare_layout(Vector::new(400., 320.));

        assert_rect(child_rect.get(), Rect::new(45., 15., 110., 300.));
        assert_path_eq!(hit_path(*root, Vector::new(60., 100.)), path);
        assert_path_eq!(
            hit_path(*root, Vector::new(149., 100.)),
            vec![inner_ref, outer_wrapper, outer_ref, *root]
        );
        assert_path_eq!(
            hit_path(*root, Vector::new(214., 100.)),
            vec![outer_ref, *root]
        );
        assert_path_eq!(
            hit_path(*root, Vector::new(60., 82.)),
            vec![outer_wrapper, outer_ref, *root],
            "the inner clip must reject content in its top padding"
        );
        assert_path_eq!(hit_path(*root, Vector::new(60., 45.)), vec![*root]);

        // Scroll offsets affect published canvas rects, never Taffy's local locations.
        let local_locations = || {
            with_taffy(|taffy| {
                let inner = taffy.layout(inner_ref.layout_id().unwrap()).unwrap();
                let child = taffy.layout(child_ref.layout_id().unwrap()).unwrap();
                (inner.location, child.location)
            })
        };
        let local = local_locations();
        assert_eq!(local.0, taffy::Point { x: 0., y: 80. });
        assert_eq!(local.1, taffy::Point { x: 0., y: 0. });

        outer_offset.set(Vector::new(0., 90.));
        inner_offset.set(Vector::new(0., 120.));
        for _ in 0..2 {
            root.prepare_layout(Vector::new(400., 320.));
            assert_rect(child_rect.get(), Rect::new(45., -75., 110., 300.));
            assert_eq!(local_locations(), local);
            assert_path_eq!(hit_path(*root, Vector::new(60., 52.)), path);
            assert_path_eq!(
                hit_path(*root, Vector::new(60., 47.)),
                vec![*root],
                "the outer clip still excludes a point inside the inner viewport"
            );
        }
    });
}

#[test]
fn scroll_mounted_by_layout_hook_keeps_initial_offset_until_its_layout_is_computed() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let rows = signal(Rc::<[u8]>::from([]));
        let offset = signal(Vector::new(0., 50.));
        let scroll_ref = Rc::new(RefCell::new(None::<NodeRef>));
        let child_rect = Rc::new(RefCell::new(None::<ReadSignal<Rect>>));
        let observed = Rc::new(RefCell::new(Vec::new()));
        let root = flexbox()
            .width(200.)
            .height(200.)
            .on_layout(move |_| {
                if rows.get_clone().is_empty() {
                    rows.set(Rc::from([1]));
                }
            })
            .children(mue_macroquad::node::map_keyed(rows, |row| *row, {
                let scroll_ref = scroll_ref.clone();
                let child_rect = child_rect.clone();
                let observed = observed.clone();
                move |_| {
                    let (child, rect) = leaf(Style::new().width(100.).height(300.).flex_none());
                    *child_rect.borrow_mut() = Some(rect);
                    let scroll = scroll_area()
                        .width(100.)
                        .height(100.)
                        .flex_none()
                        .offset(offset)
                        .on_layout({
                            let scroll_ref = scroll_ref.clone();
                            let observed = observed.clone();
                            move |_| {
                                let scroll = (*scroll_ref.borrow()).expect("mounted ScrollArea");
                                let scroll_id = scroll.layout_id().unwrap();
                                let content_id = content(scroll).layout_id().unwrap();
                                observed.borrow_mut().push(with_taffy(|taffy| {
                                    (
                                        taffy.dirty(scroll_id).unwrap(),
                                        taffy.dirty(content_id).unwrap(),
                                        taffy.layout(scroll_id).unwrap().size,
                                        taffy.layout(content_id).unwrap().size,
                                    )
                                }));
                            }
                        })
                        .children(child)
                        .into_node();
                    *scroll_ref.borrow_mut() = Some(*scroll);
                    scroll
                }
            }))
            .into_node();

        // The first parent hook mounts a new tree after this pass computed Taffy.
        // Publishing its zero-sized stale layout would clamp the shared offset to zero.
        root.prepare_layout(Vector::new(200., 200.));

        assert_vector(offset.get(), Vector::new(0., 50.));
        assert_rect(
            child_rect.borrow().expect("mounted child rect").get(),
            Rect::new(0., -50., 100., 300.),
        );
        let observed = observed.borrow();
        assert!(!observed.is_empty(), "the mounted ScrollArea hook must run");
        for &(scroll_dirty, content_dirty, viewport, content) in observed.iter() {
            assert!(
                !scroll_dirty && !content_dirty,
                "hooks require computed layout"
            );
            assert_eq!(
                viewport,
                taffy::Size {
                    width: 100.,
                    height: 100.
                }
            );
            assert_eq!(
                content,
                taffy::Size {
                    width: 100.,
                    height: 300.
                }
            );
        }
    });
}

fn state(offset: Vector, velocity: Vector, max: Vector) -> ScrollState {
    ScrollState {
        offset: signal(offset),
        options: ScrollOptions {
            axis: ScrollAxis::Both,
            friction: 4.,
            ..ScrollOptions::default()
        },
        max,
        viewport: Rect::new(10., 20., 100., 80.),
        velocity,
        drag: None,
        last_offset: offset,
    }
}

#[test]
fn repeated_offset_correction_never_integrates_or_decays_velocity() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let initial = Vector::new(60., 100.);
        let velocity = Vector::new(120., -80.);
        let mut state = state(initial, velocity, Vector::splat(200.));
        for _ in 0..8 {
            state.correct_offset();
            assert_vector(state.offset.get(), initial);
            assert_vector(state.last_offset, initial);
            assert_vector(state.velocity, velocity);
        }
    });
}

#[test]
fn fixed_dt_inertia_matches_exponential_integral_across_prepare_corrections() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let initial = Vector::new(100., 200.);
        let velocity = Vector::new(320., -160.);
        let mut state = state(initial, velocity, Vector::splat(1000.));

        state.advance(0.02);
        let decay = (-4_f32 * 0.02).exp();
        let expected = initial + velocity * (1. - decay) / 4.;
        assert_vector(state.offset.get(), expected);
        assert_vector(state.velocity, velocity * decay);
        assert_vector(state.last_offset, expected);

        // A second prepare must not mistake the previous update for external navigation.
        for _ in 0..3 {
            state.correct_offset();
            assert_vector(state.offset.get(), expected);
            assert_vector(state.velocity, velocity * decay);
        }
        state.advance(0.03);
        let decay = (-4_f32 * 0.05).exp();
        assert_vector(state.offset.get(), initial + velocity * (1. - decay) / 4.);
        assert_vector(state.velocity, velocity * decay);
        assert_vector(state.last_offset, state.offset.get());
    });
}

#[test]
fn inertia_clamps_frame_dt_and_stops_only_the_component_at_a_hard_boundary() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let initial = Vector::new(98., 50.);
        let velocity = Vector::new(400., 200.);
        let mut state = state(initial, velocity, Vector::splat(100.));
        state.advance(-1.);
        assert_vector(state.offset.get(), initial);
        assert_vector(state.velocity, velocity);

        state.advance(1.);
        let decay = (-4_f32 * 0.05).exp();
        let expected = Vector::new(100., 50. + 200. * (1. - decay) / 4.);
        assert_vector(state.offset.get(), expected);
        assert_vector(state.velocity, Vector::new(0., 200. * decay));
        state.correct_offset();
        state.advance(0.);
        assert_vector(state.offset.get(), expected);
        assert_vector(state.velocity, Vector::new(0., 200. * decay));

        state.offset.set(Vector::new(2., 98.));
        state.correct_offset();
        state.velocity = Vector::new(-400., 400.);
        state.advance(0.05);
        assert_vector(state.offset.get(), Vector::new(0., 100.));
        assert_vector(state.velocity, Vector::ZERO);
        state.advance(0.05);
        assert_vector(state.offset.get(), Vector::new(0., 100.));
    });
}

#[test]
fn external_offset_cancels_inertia_during_prepare_or_before_the_next_update() {
    let owner = OwnedScope::new();
    owner.run(|| {
        for prepare_first in [true, false] {
            let mut state = state(
                Vector::new(100., 200.),
                Vector::new(320., -160.),
                Vector::splat(1000.),
            );
            state.advance(0.02);
            let navigation = Vector::new(40., 60.);
            state.offset.set(navigation);
            if prepare_first {
                state.correct_offset();
                assert_vector(state.velocity, Vector::ZERO);
            }
            state.advance(0.03);
            assert_vector(state.offset.get(), navigation);
            assert_vector(state.last_offset, navigation);
            assert_vector(state.velocity, Vector::ZERO);
        }
    });
}

#[test]
fn correction_sanitizes_external_offsets_and_remembers_the_clamped_value() {
    let owner = OwnedScope::new();
    owner.run(|| {
        let mut state = state(
            Vector::new(20., 30.),
            Vector::new(100., 200.),
            Vector::new(80., 100.),
        );
        state.options.axis = ScrollAxis::Vertical;
        state.offset.set(Vector::new(60., 500.));
        state.correct_offset();
        assert_vector(state.offset.get(), Vector::new(0., 100.));
        assert_vector(state.last_offset, Vector::new(0., 100.));
        assert_vector(state.velocity, Vector::ZERO);

        state.offset.set(Vector::new(f32::NAN, f32::INFINITY));
        state.correct_offset();
        assert_vector(state.offset.get(), Vector::ZERO);
        assert_vector(state.last_offset, Vector::ZERO);
        state.correct_offset();
        assert_vector(state.offset.get(), Vector::ZERO);
    });
}
