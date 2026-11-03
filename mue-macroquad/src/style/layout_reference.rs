use super::*;

impl Style {
    // Original aggregate-based build_taffy from f0be4d7:mue-macroquad/src/style.rs.
    // Only the method name and a test-only final-compute counter differ.
    // The parent module enables this reference only for tests.
    pub(crate) fn build_taffy_reference(&mut self) -> ReadSignal<taffy::Style> {
        fn to_size<S>(width: Prop<S>, height: Prop<S>) -> Prop<taffy::Size<S>>
        where
            S: Clone + PartialEq + 'static,
        {
            if let Some((width, height)) = width.as_static().zip(height.as_static()) {
                Prop::Static(taffy::Size {
                    width: width.clone(),
                    height: height.clone(),
                })
            } else {
                computed(move |_| taffy::Size {
                    width: width.get_clone(),
                    height: height.get_clone(),
                })
                .into()
            }
        }

        let display = self.take_display();
        let box_sizing = self.take_box_sizing();
        let aspect_ratio = self.take_aspect_ratio();
        let gap = to_size(self.take_column_gap(), self.take_row_gap());

        let size = to_size(self.take_width(), self.take_height());
        let min_size = to_size(self.take_min_width(), self.take_min_height());
        let max_size = to_size(self.take_max_width(), self.take_max_height());

        let overflow_x = self.take_overflow_x();
        let overflow_y = self.take_overflow_y();
        let overflow = if let Some((x, y)) = overflow_x.as_static().zip(overflow_y.as_static()) {
            Prop::Static(Point { x: *x, y: *y })
        } else {
            computed(move |_| Point {
                x: overflow_x.get_clone(),
                y: overflow_y.get_clone(),
            })
            .into()
        };

        let margin = self.take_margin();
        let padding = self.take_padding();
        let border = self.take_border();

        let align_items = self.take_align_items();
        let align_self = self.take_align_self();
        let justify_items = self.take_justify_items();
        let justify_self = self.take_justify_self();
        let align_content = self.take_align_content();
        let justify_content = self.take_justify_content();

        let position = self.take_position();
        let inset = self.take_inset();

        let flex_direction = self.take_flex_direction();
        let flex_wrap = self.take_flex_wrap();

        let flex_basis = self.take_flex_basis();
        let flex_grow = self.take_flex_grow();
        let flex_shrink = self.take_flex_shrink();

        computed(move |_| {
            crate::layout::record_style_compute();
            taffy::Style {
                display: display.get(),
                box_sizing: box_sizing.get(),
                aspect_ratio: aspect_ratio.get(),
                gap: gap.get().map(|v| v.to_length_percentage("gap")),

                size: size.get().map(Size::to_dimension),
                min_size: min_size.get().map(Size::to_dimension),
                max_size: max_size.get().map(Size::to_dimension),

                overflow: overflow.get(),

                margin: margin.get().map(Size::to_length_percentage_auto),
                padding: padding.get().map(|v| v.to_length_percentage("padding")),
                border: border.get().map(|v| v.to_length_percentage("border")),

                align_items: align_items.get(),
                align_self: align_self.get(),
                justify_items: justify_items.get(),
                justify_self: justify_self.get(),
                align_content: align_content.get(),
                justify_content: justify_content.get(),

                position: position.get(),
                inset: inset.get().map(Size::to_length_percentage_auto),

                flex_direction: flex_direction.get(),
                flex_wrap: flex_wrap.get(),

                flex_basis: flex_basis.get().to_dimension(),
                flex_grow: flex_grow.get(),
                flex_shrink: flex_shrink.get(),

                ..Default::default()
            }
        })
    }
}
