//! Built-in directives.

use std::{cell::Cell, rc::Rc};

use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    event::{hit_test::hit_test, pointer::notify_pointer_down},
    math::Vector,
};

fn press(root: NodeRef, x: f32, y: f32) {
    let position = Vector::new(x, y);
    notify_pointer_down(position, &hit_test(root, position));
}

#[test]
fn click_outside_ignores_presses_inside() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let outside = Rc::new(Cell::new(0));
        let enabled = signal(true);
        let count = outside.clone();
        let root = flexbox()
            .width(200.)
            .height(100.)
            .children((
                flexbox()
                    .width(100.)
                    .height(100.)
                    .directive(
                        ClickOutside::new(move || count.set(count.get() + 1)).enabled(enabled),
                    )
                    .children(button("Inside")),
                button("Elsewhere"),
            ))
            .into_node();
        root.prepare_layout(Vector::new(200., 100.));
        let root = *root;
        press(root, 20., 20.);
        assert_eq!(outside.get(), 0, "a press on a descendant is inside");
        press(root, 50., 90.);
        assert_eq!(outside.get(), 0, "a press within the bounds is inside");
        press(root, 150., 50.);
        assert_eq!(outside.get(), 1);
        enabled.set(false);
        press(root, 150., 50.);
        assert_eq!(outside.get(), 1, "disabled directives ignore presses");
    });
}
