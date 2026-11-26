//! Lower declared layout properties to one reactive Taffy style.

use mue_core::{
    effect::computed,
    signal::{Access, ReadSignal},
};
use taffy::Point;

use super::{SizeExt as _, Style};

impl Style {
    pub(crate) fn build_taffy(&mut self) -> ReadSignal<taffy::Style> {
        let display = self.take_display();
        let box_sizing = self.take_box_sizing();
        let aspect_ratio = self.take_aspect_ratio();
        let column_gap = self.take_column_gap();
        let row_gap = self.take_row_gap();

        let width = self.take_width();
        let height = self.take_height();
        let min_width = self.take_min_width();
        let min_height = self.take_min_height();
        let max_width = self.take_max_width();
        let max_height = self.take_max_height();

        let overflow_x = self.take_overflow_x();
        let overflow_y = self.take_overflow_y();

        let margin_left = self.take_margin_left();
        let margin_right = self.take_margin_right();
        let margin_top = self.take_margin_top();
        let margin_bottom = self.take_margin_bottom();
        let padding_left = self.take_padding_left();
        let padding_right = self.take_padding_right();
        let padding_top = self.take_padding_top();
        let padding_bottom = self.take_padding_bottom();
        let border_left = self.take_border_left();
        let border_right = self.take_border_right();
        let border_top = self.take_border_top();
        let border_bottom = self.take_border_bottom();

        let align_items = self.take_align_items();
        let align_self = self.take_align_self();
        let justify_items = self.take_justify_items();
        let justify_self = self.take_justify_self();
        let align_content = self.take_align_content();
        let justify_content = self.take_justify_content();

        let position = self.take_position();
        let inset_left = self.take_inset_left();
        let inset_right = self.take_inset_right();
        let inset_top = self.take_inset_top();
        let inset_bottom = self.take_inset_bottom();

        let flex_direction = self.take_flex_direction();
        let flex_wrap = self.take_flex_wrap();

        let grid_template_columns = self.take_grid_template_columns();
        let grid_template_rows = self.take_grid_template_rows();
        let grid_auto_columns = self.take_grid_auto_columns();
        let grid_auto_rows = self.take_grid_auto_rows();
        let grid_auto_flow = self.take_grid_auto_flow();
        let grid_column = self.take_grid_column();
        let grid_row = self.take_grid_row();

        let flex_basis = self.take_flex_basis();
        let flex_grow = self.take_flex_grow();
        let flex_shrink = self.take_flex_shrink();

        computed(move |_| taffy::Style {
            display: display.get(),
            box_sizing: box_sizing.get(),
            aspect_ratio: aspect_ratio.get(),
            gap: taffy::Size {
                width: column_gap.get().to_length_percentage("gap"),
                height: row_gap.get().to_length_percentage("gap"),
            },

            size: taffy::Size {
                width: width.get().to_dimension(),
                height: height.get().to_dimension(),
            },
            min_size: taffy::Size {
                width: min_width.get().to_dimension(),
                height: min_height.get().to_dimension(),
            },
            max_size: taffy::Size {
                width: max_width.get().to_dimension(),
                height: max_height.get().to_dimension(),
            },

            overflow: Point {
                x: overflow_x.get(),
                y: overflow_y.get(),
            },

            margin: taffy::Rect {
                left: margin_left.get().to_length_percentage_auto(),
                right: margin_right.get().to_length_percentage_auto(),
                top: margin_top.get().to_length_percentage_auto(),
                bottom: margin_bottom.get().to_length_percentage_auto(),
            },
            padding: taffy::Rect {
                left: padding_left.get().to_length_percentage("padding"),
                right: padding_right.get().to_length_percentage("padding"),
                top: padding_top.get().to_length_percentage("padding"),
                bottom: padding_bottom.get().to_length_percentage("padding"),
            },
            border: taffy::Rect {
                left: border_left.get().to_length_percentage("border"),
                right: border_right.get().to_length_percentage("border"),
                top: border_top.get().to_length_percentage("border"),
                bottom: border_bottom.get().to_length_percentage("border"),
            },

            align_items: align_items.get(),
            align_self: align_self.get(),
            justify_items: justify_items.get(),
            justify_self: justify_self.get(),
            align_content: align_content.get(),
            justify_content: justify_content.get(),

            position: position.get(),
            inset: taffy::Rect {
                left: inset_left.get().to_length_percentage_auto(),
                right: inset_right.get().to_length_percentage_auto(),
                top: inset_top.get().to_length_percentage_auto(),
                bottom: inset_bottom.get().to_length_percentage_auto(),
            },

            flex_direction: flex_direction.get(),
            flex_wrap: flex_wrap.get(),

            flex_basis: flex_basis.get().to_dimension(),
            flex_grow: flex_grow.get(),
            flex_shrink: flex_shrink.get(),

            grid_template_columns: grid_template_columns.get_clone().to_taffy(),
            grid_template_rows: grid_template_rows.get_clone().to_taffy(),
            grid_auto_columns: grid_auto_columns
                .get_clone()
                .0
                .iter()
                .map(|t| t.to_taffy())
                .collect(),
            grid_auto_rows: grid_auto_rows
                .get_clone()
                .0
                .iter()
                .map(|t| t.to_taffy())
                .collect(),
            grid_auto_flow: grid_auto_flow.get(),
            grid_column: grid_column.get().to_taffy(),
            grid_row: grid_row.get().to_taffy(),

            ..Default::default()
        })
    }
}
