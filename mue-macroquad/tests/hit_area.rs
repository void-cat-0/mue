//! Headless checks for `hit_slop` and `min_hit_size`.

use mue_core::prelude::*;
use mue_macroquad::{
    event::hit_test::{hit_rect, hit_test},
    math::{Rect, Vector},
    node::{IntoNode, flexbox},
    style::Styleable,
};

#[test]
fn hit_rect_grows_by_slop_then_to_the_minimum_size() {
    let rect = Rect::new(10., 10., 20., 20.);
    assert_eq!(hit_rect(rect, 0., 0.), rect);
    assert_eq!(hit_rect(rect, 4., 0.), Rect::new(6., 6., 28., 28.));
    assert_eq!(hit_rect(rect, 0., 44.), Rect::new(-2., -2., 44., 44.));
    assert_eq!(
        hit_rect(Rect::new(0., 0., 100., 10.), 0., 44.),
        Rect::new(0., -17., 100., 44.)
    );
}

#[test]
fn small_targets_accept_touches_around_them() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let root = flexbox()
            .width(200.)
            .height(100.)
            .padding((40.,))
            .children(
                flexbox()
                    .width(20.)
                    .height(20.)
                    .min_hit_size(44.)
                    .on_pointer_event(|_| {}),
            )
            .into_node();
        root.prepare_layout(Vector::new(200., 100.));
        let target = root.children().get_clone_untracked()[0];
        // The target sits at (40, 40)..(60, 60); its hit area is (28, 28)..(72, 72).
        assert!(hit_test(*root, Vector::new(30., 50.)).contains(&target));
        assert!(hit_test(*root, Vector::new(70., 30.)).contains(&target));
        assert!(!hit_test(*root, Vector::new(75., 50.)).contains(&target));
    });
}

#[test]
fn slop_reaches_past_the_parent_edge() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let root = flexbox()
            .width(200.)
            .height(200.)
            .children(
                flexbox().margin((50.,)).width(40.).height(20.).children(
                    flexbox()
                        .width(40.)
                        .height(20.)
                        .hit_slop(12.)
                        .on_pointer_event(|_| {}),
                ),
            )
            .into_node();
        root.prepare_layout(Vector::new(200., 200.));
        let row = root.children().get_clone_untracked()[0];
        let button = row.children().get_clone_untracked()[0];
        // 8 px above the row, inside the button's slop.
        let path = hit_test(*root, Vector::new(60., 42.));
        assert!(path.contains(&button));
        assert!(path.contains(&row));
        assert!(!hit_test(*root, Vector::new(60., 30.)).contains(&button));
    });
}
