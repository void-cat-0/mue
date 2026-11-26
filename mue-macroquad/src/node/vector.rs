//! Vector drawing nodes: [`path()`] for one path and [`svg()`] for a document.
//!
//! Both nodes fit their geometry into the content box (inside padding and
//! border) with a [`PathFit`], draw a normal background/border behind it, and
//! measure to the geometry's natural size when width or height is `auto`.
//! `currentColor` and the default path fill use the node's foreground `color`,
//! inherited like text color.

use mue_core::prelude::*;
use taffy::{AvailableSpace, Size};

use super::{BuildNode, Node, NodeContext};
use crate::{
    layout::use_layout,
    math::{Rect, Vector},
    paint::path::{FillRule, PathData, PathFit, StrokeStyle},
    paint::shader::Brush,
    paint::svg::{SvgDocument, SvgPaint},
    paint::{ShapesBuilder, use_background, use_paint},
    style::{Style, Styleable},
};

fn natural_size(natural: Vector) -> impl Fn(Size<Option<f32>>, Size<AvailableSpace>) -> Size<f32> {
    move |known, _| {
        let aspect = if natural.y > 0. {
            natural.x / natural.y
        } else {
            1.
        };
        match (known.width, known.height) {
            (Some(width), Some(height)) => Size { width, height },
            (Some(width), None) => Size {
                width,
                height: if aspect > 0. { width / aspect } else { 0. },
            },
            (None, Some(height)) => Size {
                width: height * aspect,
                height,
            },
            (None, None) => Size {
                width: natural.x,
                height: natural.y,
            },
        }
    }
}

fn foreground(style: &mut Style) -> Prop<Brush> {
    style
        .declared_color()
        .or_else(crate::inherit::inherited_foreground)
        .unwrap_or_else(|| style.take_color())
}

/// Builder returned by [`path()`].
pub struct PathBuilder {
    data: Prop<PathData>,
    fill: Option<Prop<Brush>>,
    stroke: Option<Prop<Brush>>,
    stroke_style: Prop<StrokeStyle>,
    fill_rule: Prop<FillRule>,
    view_box: Option<Prop<Rect>>,
    fit: Prop<PathFit>,
    no_fill: bool,
    stroke_current: bool,
    style: Style,
}

/// Draw one vector path, fitted into the node's content box.
///
/// Without [`PathBuilder::fill`] or [`PathBuilder::stroke`], the path is
/// filled with the foreground `color`. Setting only a stroke leaves the
/// interior empty. The view box defaults to the path bounds; set it to keep a
/// fixed coordinate space, as icons usually do.
///
/// ```
/// use mue_macroquad::{math::Rect, node::path, paint::path::{PathData, StrokeStyle}, style::Styleable};
///
/// let check = PathData::parse("M4 12l5 5L20 6").unwrap();
/// let _icon = path(check)
///     .view_box(Rect::new(0., 0., 24., 24.))
///     .stroke(mue_macroquad::paint::color::Color::WHITE)
///     .stroke_style(StrokeStyle::round(2.))
///     .width(24.)
///     .height(24.);
/// ```
pub fn path(data: impl Into<Prop<PathData>>) -> PathBuilder {
    PathBuilder {
        data: data.into(),
        fill: None,
        stroke: None,
        stroke_style: Prop::Static(StrokeStyle::default()),
        fill_rule: Prop::Static(FillRule::NonZero),
        view_box: None,
        fit: Prop::Static(PathFit::Contain),
        no_fill: false,
        stroke_current: false,
        style: Style::new(),
    }
}

impl PathBuilder {
    /// Fill with this brush instead of the foreground color.
    pub fn fill(mut self, brush: impl Into<Prop<Brush>>) -> Self {
        self.fill = Some(brush.into());
        self.no_fill = false;
        self
    }

    /// Draw the outline only.
    pub fn no_fill(mut self) -> Self {
        self.fill = None;
        self.no_fill = true;
        self
    }

    /// Stroke with this brush. Without an explicit fill, the path is not filled.
    pub fn stroke(mut self, brush: impl Into<Prop<Brush>>) -> Self {
        self.stroke = Some(brush.into());
        self.stroke_current = false;
        self
    }

    /// Stroke with the foreground color and leave the interior empty.
    pub fn stroke_current_color(mut self) -> Self {
        self.stroke = None;
        self.stroke_current = true;
        self
    }

    /// Stroke width, caps, and joins in view box units.
    pub fn stroke_style(mut self, style: impl Into<Prop<StrokeStyle>>) -> Self {
        self.stroke_style = style.into();
        self
    }

    /// Stroke width in view box units, keeping the other stroke settings.
    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_style = self
            .stroke_style
            .map(move |style| StrokeStyle { width, ..style });
        self
    }

    pub fn fill_rule(mut self, rule: impl Into<Prop<FillRule>>) -> Self {
        self.fill_rule = rule.into();
        self
    }

    /// Coordinate space mapped into the content box. Defaults to the path bounds.
    pub fn view_box(mut self, view_box: impl Into<Prop<Rect>>) -> Self {
        self.view_box = Some(view_box.into());
        self
    }

    pub fn fit(mut self, fit: impl Into<Prop<PathFit>>) -> Self {
        self.fit = fit.into();
        self
    }
}

impl Styleable for PathBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for PathBuilder {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            data,
            fill,
            stroke,
            stroke_style,
            fill_rule,
            view_box,
            fit,
            no_fill,
            stroke_current,
            style,
        } = self;
        ctx.push_style(style);
        ctx.finish(Style::new(), move |style| {
            let layout = use_layout(style);
            let (rect, border, padding) = (layout.rect, layout.border, layout.padding);
            let paint = use_paint(style);
            use_background(style, rect);
            let color = foreground(style);
            let view_box = view_box.unwrap_or_else(|| data.map(|data| data.bounds()));
            layout.set_measure_fn(move |known, available| {
                let view_box = view_box.get();
                natural_size(view_box.size())(known, available)
            });
            let fill = match (fill, no_fill || stroke.is_some() || stroke_current) {
                (Some(fill), _) => Some(fill),
                (None, true) => None,
                (None, false) => Some(color.clone()),
            };
            let stroke = if stroke_current { Some(color) } else { stroke };
            let shapes = paint.build(move |p| {
                let content = rect.get().deflate(border.get()).deflate(padding.get());
                let data = data.get_clone();
                if content.w <= 0. || content.h <= 0. || data.is_empty() {
                    return;
                }
                paint_path(
                    p,
                    &data,
                    view_box.get(),
                    content,
                    fit.get(),
                    fill.as_ref()
                        .map(|fill| (fill.get_clone(), fill_rule.get())),
                    stroke
                        .as_ref()
                        .map(|stroke| (stroke.get_clone(), stroke_style.get())),
                );
            });
            style.on_render.append(move |_| shapes.get_clone().draw());
        })
    }
}

/// Builder returned by [`svg()`].
pub struct SvgBuilder {
    document: Prop<SvgDocument>,
    fit: Prop<PathFit>,
    style: Style,
}

/// Draw a parsed [`SvgDocument`] fitted into the node's content box.
///
/// `currentColor` uses the foreground `color`, so the same icon document can
/// follow text color. See [`crate::paint::svg`] for the supported subset.
pub fn svg(document: impl Into<Prop<SvgDocument>>) -> SvgBuilder {
    SvgBuilder {
        document: document.into(),
        fit: Prop::Static(PathFit::Contain),
        style: Style::new(),
    }
}

impl SvgBuilder {
    pub fn fit(mut self, fit: impl Into<Prop<PathFit>>) -> Self {
        self.fit = fit.into();
        self
    }
}

impl Styleable for SvgBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for SvgBuilder {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            document,
            fit,
            style,
        } = self;
        ctx.push_style(style);
        ctx.finish(Style::new(), move |style| {
            let layout = use_layout(style);
            let (rect, border, padding) = (layout.rect, layout.border, layout.padding);
            let paint = use_paint(style);
            use_background(style, rect);
            let color = foreground(style);
            {
                let document = document.clone();
                layout.set_measure_fn(move |known, available| {
                    let size = document.get_clone().intrinsic_size();
                    natural_size(size)(known, available)
                });
            }
            let shapes = paint.build(move |p| {
                let content = rect.get().deflate(border.get()).deflate(padding.get());
                if content.w <= 0. || content.h <= 0. {
                    return;
                }
                let doc = document.get_clone();
                paint_svg(p, &doc, content, fit.get(), &color.get_clone());
            });
            style.on_render.append(move |_| shapes.get_clone().draw());
        })
    }
}

/// Paint `data` fitted from `view_box` into `target`. Stroke widths are in
/// view-box units and scale with the fit.
pub fn paint_path(
    p: &mut ShapesBuilder,
    data: &PathData,
    view_box: Rect,
    target: Rect,
    fit: PathFit,
    fill: Option<(Brush, FillRule)>,
    stroke: Option<(Brush, StrokeStyle)>,
) {
    if target.w <= 0. || target.h <= 0. || data.is_empty() {
        return;
    }
    let matrix = fit.matrix(view_box, target);
    let scale = matrix
        .x_axis
        .truncate()
        .length()
        .max(matrix.y_axis.truncate().length());
    let fitted = data.transformed(matrix);
    if let Some((brush, rule)) = fill {
        p.fill(&fitted, rule, brush.shader(target));
    }
    if let Some((brush, mut style)) = stroke {
        style.width *= scale;
        p.stroke(&fitted, &style, brush.shader(target));
    }
}

/// Paint an SVG document fitted into `target`; `current` is `currentColor`.
pub fn paint_svg(
    p: &mut ShapesBuilder,
    doc: &SvgDocument,
    target: Rect,
    fit: PathFit,
    current: &Brush,
) {
    if target.w <= 0. || target.h <= 0. {
        return;
    }
    let matrix = fit.matrix(doc.view_box(), target);
    let scale = matrix
        .x_axis
        .truncate()
        .length()
        .max(matrix.y_axis.truncate().length());
    let brush = |paint: SvgPaint, opacity: f32| -> Brush {
        let brush = match paint {
            SvgPaint::Color(color) => Brush::Solid(color),
            SvgPaint::CurrentColor => current.clone(),
        };
        brush.with_opacity(opacity)
    };
    for shape in doc.shapes() {
        let fitted = shape.path.transformed(matrix);
        if let Some(fill) = shape.fill {
            p.fill(
                &fitted,
                shape.fill_rule,
                brush(fill.paint, fill.opacity).shader(target),
            );
        }
        if let Some(stroke) = shape.stroke {
            let mut style = shape.stroke_style;
            style.width *= scale;
            p.stroke(
                &fitted,
                &style,
                brush(stroke.paint, stroke.opacity).shader(target),
            );
        }
    }
}
