//! Every public builder supports caller styles (component convention R-A6).

use mue_components::prelude::*;
use mue_macroquad::style::Styleable;

fn styleable<T: Styleable>(_: T) {}

#[test]
fn every_component_builder_is_styleable() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        styleable(button("A"));
        styleable(input());
        styleable(textarea());
        styleable(input_group(input()));
        styleable(icon_named("check"));
        styleable(center());
        styleable(container());
        styleable(div());
        styleable(float());
        styleable(grid());
        styleable(simple_grid());
        styleable(safe_area());
        styleable(scroll_area());
        styleable(separator());
        styleable(stack());
        styleable(hstack());
        styleable(swipe_actions(text("A")));
        styleable(wrap());
        styleable(aspect_ratio());
        styleable(back_swipe());
        styleable(overlay_host(false, flexbox()));
        styleable(pages(0_usize, Vec::<Node>::new()));
        styleable(heading("A"));
        styleable(code("A"));
        styleable(code_block("A"));
        styleable(blockquote("A"));
        styleable(em("A"));
        styleable(link("A"));
        styleable(link_overlay());
        styleable(list());
        styleable(list_item());
        styleable(markdown("A"));
        styleable(document("A"));
        styleable(path(PathData::default()));
        styleable(teleport(Layer::Modal, flexbox));
        styleable(presence(false, flexbox));
        styleable(anchored(mue_macroquad::math::Rect::default()));
        styleable(offscreen());
    });
}
