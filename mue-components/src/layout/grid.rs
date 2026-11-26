use mue_core::prop::Prop;
use mue_macroquad::node::{Component, flexbox};
use mue_macroquad::style::{GridTemplate, GridTrack, Repeat, Styleable, values::Display};

/// A CSS grid container. Describe tracks with `grid-template-columns` /
/// `grid-template-rows` and place children with `grid-column` / `grid-row`.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _layout = grid()
///     .style(style! {
///         grid-template-columns: 240px 1fr;
///         grid-template-rows: auto 1fr;
///         gap: 12px;
///     })
///     .children((
///         text("Header").style(style! { grid-column: 1 / 3; }),
///         text("Sidebar"),
///         text("Content"),
///     ));
/// ```
#[mue_macros::component]
pub fn grid() -> impl Component {
    flexbox().display(Display::Grid)
}

/// A grid of equal columns, the common card-grid layout.
///
/// With `min_child_width`, as many columns as fit are created (each at least
/// that wide, sharing leftover space); otherwise `columns` equal columns are
/// used. The default is one column with an 8-pixel gap.
///
/// ```
/// use mue_components::prelude::*;
///
/// let _cards = simple_grid()
///     .min_child_width(Some(160.))
///     .children((text("A"), text("B"), text("C")));
/// let _fixed = simple_grid().columns(3_u16);
/// ```
#[mue_macros::component]
pub fn simple_grid(
    #[default(1)] columns: u16,
    #[default] min_child_width: Option<f32>,
) -> impl Component {
    let template: Prop<GridTemplate> = mue_core::effect::computed(move |_| {
        match mue_core::signal::Access::get(&min_child_width) {
            Some(width) => GridTemplate::new().repeat(
                Repeat::AutoFill,
                [GridTrack::minmax(GridTrack::px(width), GridTrack::fr(1.))],
            ),
            None => GridTemplate::equal(mue_core::signal::Access::get(&columns).max(1)),
        }
    })
    .into();
    flexbox()
        .display(Display::Grid)
        .grid_template_columns(template)
        .column_gap(8.)
        .row_gap(8.)
}
