//! A deliberately small SVG subset for icons and simple illustrations.
//!
//! [`SvgDocument::parse`] reads `<svg>` with `viewBox`/`width`/`height`, nested
//! `<g>`, and the basic shapes `path`, `rect`, `circle`, `ellipse`, `line`,
//! `polyline`, and `polygon`. Supported presentation attributes (also inside a
//! `style="..."` attribute) are `fill`, `stroke`, `stroke-width`,
//! `stroke-linecap`, `stroke-linejoin`, `stroke-miterlimit`, `fill-rule`,
//! `opacity`, `fill-opacity`, and `stroke-opacity`. `transform` accepts
//! `matrix`, `translate`, `scale`, `rotate`, `skewX`, and `skewY`.
//!
//! Everything else (text, gradients, `use`, masks, filters, CSS selectors) is
//! ignored. Draw a document with the [`svg`](crate::node::svg()) node.
//!
//! ```
//! use mue_macroquad::svg::{SvgDocument, SvgPaint};
//!
//! let doc = SvgDocument::parse(
//!     r#"<svg viewBox="0 0 24 24"><path d="M4 12h16" stroke="currentColor"/></svg>"#,
//! )
//! .unwrap();
//! assert_eq!(doc.shapes().len(), 1);
//! assert_eq!(doc.shapes()[0].stroke.map(|s| s.paint), Some(SvgPaint::CurrentColor));
//! ```

use std::{fmt, rc::Rc};

use mue_core::prop::PropValue;

use crate::{
    color::Color,
    math::{CornerRadii, Matrix, Rect, Vector, vec2},
    path::{FillRule, LineCap, LineJoin, PathData, StrokeStyle},
};

/// Paint for a fill or stroke.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SvgPaint {
    Color(Color),
    /// The drawing node's foreground `color`, like SVG `currentColor`.
    CurrentColor,
}

/// One fill or stroke with its own opacity, already multiplied by ancestors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgFill {
    pub paint: SvgPaint,
    pub opacity: f32,
}

/// One drawable element with ancestor transforms baked into its path.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgShape {
    pub path: PathData,
    pub fill: Option<SvgFill>,
    pub fill_rule: FillRule,
    pub stroke: Option<SvgFill>,
    /// Stroke geometry in document units, scaled by the element's transform.
    pub stroke_style: StrokeStyle,
}

/// A parsed SVG document in its view box coordinate space.
#[derive(Clone, PartialEq)]
pub struct SvgDocument {
    view_box: Rect,
    size: Option<Vector>,
    shapes: Rc<[SvgShape]>,
}

impl PropValue for SvgDocument {}

impl fmt::Debug for SvgDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SvgDocument")
            .field("view_box", &self.view_box)
            .field("size", &self.size)
            .field("shapes", &self.shapes.len())
            .finish()
    }
}

/// The document is not well-formed XML or its root is not `<svg>`.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SvgError {
    #[error("invalid XML: {0}")]
    Xml(String),
    #[error("the root element must be <svg>")]
    NotSvg,
    #[error("invalid {attribute} attribute: {message}")]
    Attribute {
        attribute: &'static str,
        message: String,
    },
}

impl SvgDocument {
    pub fn parse(source: &str) -> Result<Self, SvgError> {
        let xml = roxmltree::Document::parse(source).map_err(|e| SvgError::Xml(e.to_string()))?;
        let root = xml.root_element();
        if root.tag_name().name() != "svg" {
            return Err(SvgError::NotSvg);
        }
        let width = root.attribute("width").and_then(parse_length);
        let height = root.attribute("height").and_then(parse_length);
        let view_box = match root.attribute("viewBox") {
            Some(value) => {
                let numbers = numbers(value);
                if numbers.len() != 4 || numbers[2] < 0. || numbers[3] < 0. {
                    return Err(SvgError::Attribute {
                        attribute: "viewBox",
                        message: format!("expected four numbers, found `{value}`"),
                    });
                }
                Rect::new(numbers[0], numbers[1], numbers[2], numbers[3])
            }
            None => Rect::new(0., 0., width.unwrap_or(0.), height.unwrap_or(0.)),
        };
        let size = match (width, height) {
            (Some(w), Some(h)) => Some(vec2(w, h)),
            (Some(w), None) if view_box.w > 0. => Some(vec2(w, w * view_box.h / view_box.w)),
            (None, Some(h)) if view_box.h > 0. => Some(vec2(h * view_box.w / view_box.h, h)),
            _ => None,
        };
        let mut shapes = Vec::new();
        visit(root, &Inherited::default(), &mut shapes)?;
        Ok(Self {
            view_box,
            size,
            shapes: shapes.into(),
        })
    }

    /// Coordinate space of the shapes.
    pub fn view_box(&self) -> Rect {
        self.view_box
    }

    /// Natural size from `width`/`height`, or the view box size.
    pub fn intrinsic_size(&self) -> Vector {
        self.size.unwrap_or(self.view_box.size())
    }

    pub fn shapes(&self) -> &[SvgShape] {
        &self.shapes
    }
}

#[derive(Clone)]
struct Inherited {
    transform: Matrix,
    fill: Option<SvgPaint>,
    fill_opacity: f32,
    fill_rule: FillRule,
    stroke_paint: Option<SvgPaint>,
    stroke_opacity: f32,
    stroke: StrokeStyle,
    opacity: f32,
}

impl Default for Inherited {
    fn default() -> Self {
        Self {
            transform: Matrix::IDENTITY,
            fill: Some(SvgPaint::Color(Color::BLACK)),
            fill_opacity: 1.,
            fill_rule: FillRule::NonZero,
            stroke_paint: None,
            stroke_opacity: 1.,
            stroke: StrokeStyle::new(1.),
            opacity: 1.,
        }
    }
}

fn visit(
    node: roxmltree::Node,
    parent: &Inherited,
    shapes: &mut Vec<SvgShape>,
) -> Result<(), SvgError> {
    let mut state = parent.clone();
    if let Some(transform) = node.attribute("transform") {
        state.transform = parent.transform * parse_transform(transform)?;
    }
    let mut apply = |name: &str, value: &str| -> Result<(), SvgError> {
        let value = value.trim();
        match name {
            "fill" => {
                if let Some(paint) = parse_paint(value) {
                    state.fill = paint;
                }
            }
            "stroke" => {
                if let Some(paint) = parse_paint(value) {
                    state.stroke_paint = paint;
                }
            }
            "stroke-width" => {
                state.stroke.width =
                    parse_length(value).ok_or_else(|| bad("stroke-width", value))?
            }
            "stroke-linecap" => {
                state.stroke.cap = match value {
                    "round" => LineCap::Round,
                    "square" => LineCap::Square,
                    _ => LineCap::Butt,
                }
            }
            "stroke-linejoin" => {
                state.stroke.join = match value {
                    "round" => LineJoin::Round,
                    "bevel" => LineJoin::Bevel,
                    _ => LineJoin::Miter,
                }
            }
            "stroke-miterlimit" => {
                state.stroke.miter_limit =
                    value.parse().map_err(|_| bad("stroke-miterlimit", value))?
            }
            "fill-rule" => {
                state.fill_rule = if value == "evenodd" {
                    FillRule::EvenOdd
                } else {
                    FillRule::NonZero
                }
            }
            "opacity" => state.opacity *= parse_unit(value, "opacity")?,
            "fill-opacity" => state.fill_opacity = parse_unit(value, "fill-opacity")?,
            "stroke-opacity" => state.stroke_opacity = parse_unit(value, "stroke-opacity")?,
            _ => {}
        }
        Ok(())
    };
    for attribute in node.attributes() {
        apply(attribute.name(), attribute.value())?;
    }
    if let Some(style) = node.attribute("style") {
        for declaration in style.split(';') {
            if let Some((name, value)) = declaration.split_once(':') {
                apply(name.trim(), value)?;
            }
        }
    }
    let attr = |name: &str| node.attribute(name).and_then(parse_length).unwrap_or(0.);
    let geometry = match node.tag_name().name() {
        "svg" | "g" => {
            for child in node.children().filter(roxmltree::Node::is_element) {
                visit(child, &state, shapes)?;
            }
            return Ok(());
        }
        "path" => match node.attribute("d") {
            Some(d) => PathData::parse(d).map_err(|e| SvgError::Attribute {
                attribute: "d",
                message: e.to_string(),
            })?,
            None => return Ok(()),
        },
        "rect" => {
            let rect = Rect::new(attr("x"), attr("y"), attr("width"), attr("height"));
            let rx = node.attribute("rx").and_then(parse_length);
            let ry = node.attribute("ry").and_then(parse_length);
            let radius = rx.or(ry).unwrap_or(0.);
            PathData::rounded_rect(rect, CornerRadii::all(radius))
        }
        "circle" => PathData::circle(vec2(attr("cx"), attr("cy")), attr("r")),
        "ellipse" => PathData::ellipse(vec2(attr("cx"), attr("cy")), vec2(attr("rx"), attr("ry"))),
        "line" => PathData::line(vec2(attr("x1"), attr("y1")), vec2(attr("x2"), attr("y2"))),
        name @ ("polyline" | "polygon") => {
            let values = numbers(node.attribute("points").unwrap_or(""));
            let points = values.as_chunks::<2>().0.iter().map(|p| vec2(p[0], p[1]));
            if name == "polygon" {
                PathData::polygon(points)
            } else {
                PathData::polyline(points)
            }
        }
        _ => return Ok(()),
    };
    let scale = {
        let m = state.transform;
        let det = m.x_axis.x * m.y_axis.y - m.x_axis.y * m.y_axis.x;
        det.abs().sqrt()
    };
    let fill = state.fill.map(|paint| SvgFill {
        paint,
        opacity: state.fill_opacity * state.opacity,
    });
    let stroke = state
        .stroke_paint
        .filter(|_| state.stroke.width > 0.)
        .map(|paint| SvgFill {
            paint,
            opacity: state.stroke_opacity * state.opacity,
        });
    if fill.is_none() && stroke.is_none() {
        return Ok(());
    }
    shapes.push(SvgShape {
        path: geometry.transformed(state.transform),
        fill,
        fill_rule: state.fill_rule,
        stroke,
        stroke_style: StrokeStyle {
            width: state.stroke.width * scale,
            ..state.stroke
        },
    });
    Ok(())
}

fn bad(attribute: &'static str, value: &str) -> SvgError {
    SvgError::Attribute {
        attribute,
        message: format!("unsupported value `{value}`"),
    }
}

fn numbers(value: &str) -> Vec<f32> {
    value
        .split(|c: char| c.is_ascii_whitespace() || c == ',')
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect()
}

/// A plain number or a `px` length. Other units are not supported.
fn parse_length(value: &str) -> Option<f32> {
    let value = value.trim();
    let value = value.strip_suffix("px").unwrap_or(value);
    value.parse::<f32>().ok().filter(|v| v.is_finite())
}

fn parse_unit(value: &str, attribute: &'static str) -> Result<f32, SvgError> {
    let parsed = match value.strip_suffix('%') {
        Some(percent) => percent.trim().parse::<f32>().map(|v| v / 100.),
        None => value.parse::<f32>(),
    };
    parsed
        .map(|v| v.clamp(0., 1.))
        .map_err(|_| bad(attribute, value))
}

/// `Some(paint)` to set the paint (`Some(None)` for `none`), or `None` to keep
/// the inherited one (`inherit` and unsupported values, as SVG ignores invalid
/// presentation attributes). `url(#id)` paint servers are unsupported: their
/// fallback color is used when given, otherwise nothing is painted.
fn parse_paint(value: &str) -> Option<Option<SvgPaint>> {
    if value == "none" {
        return Some(None);
    }
    if value.eq_ignore_ascii_case("currentColor") {
        return Some(Some(SvgPaint::CurrentColor));
    }
    if let Some(rest) = value.strip_prefix("url(") {
        let fallback = rest
            .split_once(')')
            .map_or("", |(_, fallback)| fallback.trim());
        return Some(if fallback.is_empty() {
            None
        } else {
            parse_paint(fallback).flatten()
        });
    }
    parse_color(value).map(|color| Some(SvgPaint::Color(color)))
}

fn parse_color(value: &str) -> Option<Color> {
    if let Some(hex) = value.strip_prefix('#') {
        if !matches!(hex.len(), 3 | 4 | 6 | 8) || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let hex: String = if hex.len() <= 4 {
            hex.chars().flat_map(|c| [c, c]).collect()
        } else {
            hex.to_owned()
        };
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        let alpha = if hex.len() == 8 { byte(6)? } else { 255 };
        return Some(Color::from_rgba(byte(0)?, byte(2)?, byte(4)?, alpha));
    }
    if let Some(args) = value
        .strip_prefix("rgba(")
        .or_else(|| value.strip_prefix("rgb("))
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let parts: Vec<&str> = args
            .split(|c: char| c == ',' || c == '/' || c.is_ascii_whitespace())
            .filter(|p| !p.is_empty())
            .collect();
        if parts.len() < 3 {
            return None;
        }
        let channel = |part: &str| -> Option<f32> {
            match part.strip_suffix('%') {
                Some(p) => p.parse::<f32>().ok().map(|v| v / 100.),
                None => part.parse::<f32>().ok().map(|v| v / 255.),
            }
            .map(|v| v.clamp(0., 1.))
        };
        let alpha = match parts.get(3) {
            Some(part) => match part.strip_suffix('%') {
                Some(p) => p.parse::<f32>().ok()? / 100.,
                None => part.parse::<f32>().ok()?,
            },
            None => 1.,
        };
        return Some(Color::new(
            channel(parts[0])?,
            channel(parts[1])?,
            channel(parts[2])?,
            alpha.clamp(0., 1.),
        ));
    }
    let (r, g, b, a) = match value.to_ascii_lowercase().as_str() {
        "transparent" => (0, 0, 0, 0),
        "black" => (0, 0, 0, 255),
        "white" => (255, 255, 255, 255),
        "red" => (255, 0, 0, 255),
        "lime" => (0, 255, 0, 255),
        "blue" => (0, 0, 255, 255),
        "yellow" => (255, 255, 0, 255),
        "cyan" | "aqua" => (0, 255, 255, 255),
        "magenta" | "fuchsia" => (255, 0, 255, 255),
        "gray" | "grey" => (128, 128, 128, 255),
        "silver" => (192, 192, 192, 255),
        "maroon" => (128, 0, 0, 255),
        "olive" => (128, 128, 0, 255),
        "green" => (0, 128, 0, 255),
        "purple" => (128, 0, 128, 255),
        "teal" => (0, 128, 128, 255),
        "navy" => (0, 0, 128, 255),
        "orange" => (255, 165, 0, 255),
        _ => return None,
    };
    Some(Color::from_rgba(r, g, b, a))
}

fn parse_transform(value: &str) -> Result<Matrix, SvgError> {
    let mut matrix = Matrix::IDENTITY;
    let mut rest = value.trim();
    while !rest.is_empty() {
        let open = rest.find('(').ok_or_else(|| bad("transform", value))?;
        let close = rest.find(')').ok_or_else(|| bad("transform", value))?;
        if close < open {
            return Err(bad("transform", value));
        }
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let args = numbers(&rest[open + 1..close]);
        let arg = |i: usize| args.get(i).copied();
        let next = match (name, args.len()) {
            ("matrix", 6) => Matrix::from_cols_array(&[
                args[0], args[1], 0., args[2], args[3], 0., args[4], args[5], 1.,
            ]),
            ("translate", 1 | 2) => Matrix::from_translation(vec2(args[0], arg(1).unwrap_or(0.))),
            ("scale", 1 | 2) => Matrix::from_scale(vec2(args[0], arg(1).unwrap_or(args[0]))),
            ("rotate", 1) => Matrix::from_angle(args[0].to_radians()),
            ("rotate", 3) => {
                let center = vec2(args[1], args[2]);
                Matrix::from_translation(center)
                    * Matrix::from_angle(args[0].to_radians())
                    * Matrix::from_translation(-center)
            }
            ("skewX", 1) => Matrix::from_cols_array(&[
                1.,
                0.,
                0.,
                args[0].to_radians().tan(),
                1.,
                0.,
                0.,
                0.,
                1.,
            ]),
            ("skewY", 1) => Matrix::from_cols_array(&[
                1.,
                args[0].to_radians().tan(),
                0.,
                0.,
                1.,
                0.,
                0.,
                0.,
                1.,
            ]),
            _ => return Err(bad("transform", value)),
        };
        matrix *= next;
        rest = rest[close + 1..].trim_start();
    }
    Ok(matrix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_view_box_size_and_inherited_paint() {
        let doc = SvgDocument::parse(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="48" viewBox="0 0 24 12">
                <g fill="#f00" opacity="0.5">
                    <rect x="1" y="1" width="4" height="4" rx="1"/>
                    <circle cx="10" cy="6" r="2" fill="none" stroke="currentColor" stroke-width="2"/>
                </g>
                <text>ignored</text>
            </svg>"##,
        )
        .unwrap();
        assert_eq!(doc.view_box(), Rect::new(0., 0., 24., 12.));
        assert_eq!(doc.intrinsic_size(), vec2(48., 24.));
        let [rect, circle] = doc.shapes() else {
            panic!("expected two shapes, found {:?}", doc.shapes());
        };
        assert_eq!(
            rect.fill,
            Some(SvgFill {
                paint: SvgPaint::Color(Color::from_rgba(255, 0, 0, 255)),
                opacity: 0.5
            })
        );
        assert!(circle.fill.is_none());
        assert_eq!(circle.stroke.unwrap().paint, SvgPaint::CurrentColor);
        assert_eq!(circle.stroke_style.width, 2.);
    }

    #[test]
    fn bakes_transforms_and_scales_strokes() {
        let doc = SvgDocument::parse(
            r#"<svg viewBox="0 0 10 10"><g transform="translate(5 0) scale(2)">
                <line x1="0" y1="1" x2="2" y2="1" stroke="black" style="stroke-width: 1.5"/>
            </g></svg>"#,
        )
        .unwrap();
        let line = &doc.shapes()[0];
        let bounds = line.path.bounds();
        assert!(
            (bounds.x - 5.).abs() < 1e-4 && (bounds.w - 4.).abs() < 1e-4,
            "{bounds:?}"
        );
        assert!((line.stroke_style.width - 3.).abs() < 1e-4);
    }

    #[test]
    fn unsupported_paints_do_not_fail_the_document() {
        let doc = SvgDocument::parse(
            r##"<svg viewBox="0 0 10 10" fill="#00f">
                <defs><linearGradient id="g"/></defs>
                <rect width="2" height="2" fill="url(#g)"/>
                <rect width="2" height="2" fill="url(#g) red"/>
                <rect width="2" height="2" fill="inherit"/>
                <rect width="2" height="2" fill="chartreuse-ish"/>
            </svg>"##,
        )
        .unwrap();
        let fills: Vec<_> = doc
            .shapes()
            .iter()
            .map(|shape| shape.fill.map(|f| f.paint))
            .collect();
        let blue = Some(SvgPaint::Color(Color::from_rgba(0, 0, 255, 255)));
        // The gradient-only rect paints nothing and is dropped.
        assert_eq!(
            fills,
            [
                Some(SvgPaint::Color(Color::from_rgba(255, 0, 0, 255))),
                blue,
                blue
            ]
        );
    }

    #[test]
    fn rejects_non_svg_roots_and_bad_attributes() {
        assert_eq!(SvgDocument::parse("<html/>"), Err(SvgError::NotSvg));
        assert!(SvgDocument::parse(r#"<svg viewBox="0 0 1"/>"#).is_err());
        assert!(SvgDocument::parse(r#"<svg><path d="Q"/></svg>"#).is_err());
    }
}
