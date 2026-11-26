//! Imperative scrolling through ScrollAreaHandle.

use mue_components::prelude::*;
use mue_macroquad::math::{Rect, Vector};

#[test]
fn handles_scroll_clamp_and_reveal() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let handle = ScrollAreaHandle::new();
        let area = scroll_area()
            .handle(handle.clone())
            .width(100.)
            .height(100.)
            .children(flexbox().width(100.).height(500.))
            .into_node();
        area.prepare_layout(Vector::new(100., 100.));
        assert_eq!(handle.max_offset(), Vector::new(0., 400.));
        handle.scroll_to(Vector::new(50., 120.), false);
        assert_eq!(
            handle.offset(),
            Vector::new(0., 120.),
            "vertical areas ignore x"
        );
        handle.scroll_to_end(false);
        assert_eq!(handle.offset().y, 400.);
        handle.scroll_to(Vector::new(0., 0.), false);
        // A rect at y 250..290 (window coordinates) comes just into view.
        handle.scroll_into_view(Rect::new(0., 250., 10., 40.), false);
        assert_eq!(handle.offset().y, 190.);
    });
}
