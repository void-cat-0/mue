use super::{Node, NodeContext};
use crate::{layout::use_layout, paint::use_background, style::Style};

/// A flex container. The default direction is a row.
#[mue_macros::node]
pub fn flexbox(ctx: NodeContext) -> Node {
    ctx.finish(Style::new(), move |style| {
        use_container(style);
    })
}

pub fn use_container(style: &mut Style) {
    let layout = use_layout(style);
    use_background(style, layout.rect);
}
