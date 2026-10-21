use super::text::text;
use mue_core::{prelude::*, prop::PropValue};
use mue_macroquad::node::flexbox::flexbox;
use mue_macroquad::node::flexbox::render_container;
use mue_macroquad::{
    node::{NodeRef, map_keyed},
    style::{Style, Styleable, StyleableExt},
};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListMarker {
    #[default]
    Bullet,
    Ordered(u64),
    None,
}
impl PropValue for ListMarker {}

impl ListMarker {
    pub(crate) fn label(self, index: usize) -> String {
        match self {
            Self::Bullet => "•".into(),
            Self::Ordered(start) => format!("{}.", start.saturating_add(index as u64)),
            Self::None => String::new(),
        }
    }
}

/// A vertical list of arbitrary child nodes, with bullets or ordered markers.
/// Marker typography comes from this node's font/color styles; content styles stay local.
#[mue_macros::node]
pub fn list(style: &mut Style, #[default] marker: ListMarker) {
    style.provide_defaults(Style::new().flex_column().gap(8.));
    let source = mue_macroquad::node::take_children();
    let entries = computed(move |_| {
        source
            .get_clone()
            .iter()
            .copied()
            .enumerate()
            .collect::<Rc<[(usize, NodeRef)]>>()
    });
    let line_height = style.take_line_height();
    let font_family = style.take_font_family();
    let font_weight = style.take_font_weight();
    let color = style
        .declared_color()
        .or_else(mue_macroquad::theme::inherited_foreground)
        .unwrap_or_else(|| style.take_color());
    let rows = map_keyed(
        entries,
        |entry| *entry,
        move |&(index, child)| {
            let children = computed(move |_| Rc::from([child])).owned();
            flexbox()
                .gap(marker.map(|m| if m == ListMarker::None { 0. } else { 10. }))
                .items_start()
                .children((
                    text(marker.map(move |marker| Rc::<str>::from(marker.label(index))))
                        .line_height(line_height)
                        .font_family(font_family.clone())
                        .font_weight(font_weight)
                        .color(color.clone())
                        .flex_none(),
                    flexbox()
                        .flex_column()
                        .min_width(0.)
                        .flex_1()
                        .children(children),
                ))
                .into_node()
        },
    );
    mue_macroquad::node::set_children(rows);
    render_container(style);
}

/// A column for one list item's content, including nested lists.
pub fn list_item() -> crate::StackBuilder {
    crate::stack()
}
