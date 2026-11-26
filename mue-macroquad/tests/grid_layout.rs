//! Headless layout checks for CSS grid containers.

use mue_core::prelude::*;
use mue_macroquad::{
    math::{Rect, Vector},
    node::{IntoNode, NodeRef, flexbox},
    style::{GridLine, GridTemplate, GridTrack, Repeat, Styleable, values::Display},
};

fn child_rects(root: NodeRef) -> Vec<Rect> {
    root.children()
        .get_clone_untracked()
        .iter()
        .map(|child| child.rect().expect("child has layout").get_untracked())
        .collect()
}

#[test]
fn fixed_and_fractional_columns_share_the_width() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let root = flexbox()
            .display(Display::Grid)
            .grid_template_columns(GridTemplate::from([GridTrack::px(100.), GridTrack::fr(1.)]))
            .width(300.)
            .column_gap(10.)
            .children((
                flexbox().height(20.),
                flexbox().height(20.),
                flexbox().height(20.).grid_column(GridLine::new(1, 3)),
            ))
            .into_node();
        root.prepare_layout(Vector::new(300., 300.));
        let rects = child_rects(*root);
        assert_eq!(rects[0], Rect::new(0., 0., 100., 20.));
        assert_eq!(rects[1], Rect::new(110., 0., 190., 20.));
        assert_eq!(rects[2], Rect::new(0., 20., 300., 20.));
    });
}

#[test]
fn auto_fill_creates_as_many_columns_as_fit() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let template = GridTemplate::new().repeat(
            Repeat::AutoFill,
            [GridTrack::minmax(GridTrack::px(100.), GridTrack::fr(1.))],
        );
        let root = flexbox()
            .display(Display::Grid)
            .grid_template_columns(template)
            .width(350.)
            .children((0..4).map(|_| flexbox().height(10.)).collect::<Vec<_>>())
            .into_node();
        root.prepare_layout(Vector::new(350., 300.));
        let rects = child_rects(*root);
        // Three 100 px minimum columns fit in 350 px and share the remainder.
        let width = 350. / 3.;
        assert!((rects[0].w - width).abs() < 1e-3, "{rects:?}");
        assert!((rects[2].x - 2. * width).abs() < 1e-3, "{rects:?}");
        assert_eq!(rects[3].y, 10.);
    });
}
