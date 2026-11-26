//! Icons from SVG documents, vector paths, or textures, sized by font size and
//! tinted by the foreground color.
//!
//! An icon is a square of `1em` (the node's font size) unless `width` or
//! `height` is set, and paints with the inherited foreground `color`, so it
//! matches neighboring text. mue ships no icon artwork: register your own or an
//! icon library's SVGs by name with [`register_icon`] (`currentColor` strokes
//! and fills follow the foreground color) and draw them with [`icon_named`].
//!
//! ```
//! use mue_components::prelude::*;
//!
//! let check = SvgDocument::parse(
//!     r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
//!         <path d="M5 12.5l4.5 4.5L19 7.5"/></svg>"#,
//! )
//! .unwrap();
//! register_icon("check", check);
//! let _row = hstack().children((
//!     icon_named("check").style(style! { color: #7fd1a8; }),
//!     text("Saved"),
//! ));
//! ```

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use mue_core::prelude::*;
use mue_macroquad::{
    layout::use_layout,
    math::Rect,
    node::{
        BuildNode, Node, NodeContext, use_font_size,
        vector::{paint_path, paint_svg},
    },
    paint::{use_background, use_paint},
    path::{FillRule, PathData, PathFit, StrokeStyle},
    shader::{Brush, SharedTexture},
    style::{Size, Style, Styleable},
    svg::SvgDocument,
};

/// What an icon draws.
#[derive(Clone)]
pub enum IconSource {
    /// An SVG document; `currentColor` follows the foreground color.
    Svg(SvgDocument),
    /// A path in a view box, filled or stroked with the foreground color.
    Path {
        data: PathData,
        view_box: Rect,
        /// `None` fills the path; `Some` strokes it with this style.
        stroke: Option<StrokeStyle>,
    },
    /// A texture region (the whole texture when `None`), tinted by the
    /// foreground color. Use white artwork for a fully tintable icon.
    Texture {
        texture: SharedTexture,
        region: Option<Rect>,
    },
}

impl From<SvgDocument> for IconSource {
    fn from(doc: SvgDocument) -> Self {
        Self::Svg(doc)
    }
}

impl From<SharedTexture> for IconSource {
    fn from(texture: SharedTexture) -> Self {
        Self::Texture {
            texture,
            region: None,
        }
    }
}

impl IconSource {
    /// A path filled in a `0 0 size size` view box.
    pub fn filled(data: PathData, size: f32) -> Self {
        Self::Path {
            data,
            view_box: Rect::new(0., 0., size, size),
            stroke: None,
        }
    }

    /// A path stroked with round caps in a `0 0 size size` view box.
    pub fn stroked(data: PathData, size: f32, width: f32) -> Self {
        Self::Path {
            data,
            view_box: Rect::new(0., 0., size, size),
            stroke: Some(StrokeStyle::round(width)),
        }
    }
}

thread_local! {
    static REGISTRY: RefCell<Option<HashMap<Rc<str>, IconSource>>> = const { RefCell::new(None) };
}

fn with_registry<R>(f: impl FnOnce(&mut HashMap<Rc<str>, IconSource>) -> R) -> R {
    REGISTRY.with(|registry| f(registry.borrow_mut().get_or_insert_with(HashMap::new)))
}

/// Register or replace a named icon for [`icon_named`] on this thread.
pub fn register_icon(name: impl Into<Rc<str>>, source: impl Into<IconSource>) {
    let (name, source) = (name.into(), source.into());
    with_registry(|registry| {
        registry.insert(name, source);
    });
}

/// Look up a registered icon.
pub fn icon_source(name: &str) -> Option<IconSource> {
    with_registry(|registry| registry.get(name).cloned())
}

/// Builder returned by [`icon`] and [`icon_named`].
pub struct IconBuilder {
    source: Option<IconSource>,
    fit: PathFit,
    style: Style,
}

/// Draw `source` as an icon.
pub fn icon(source: impl Into<IconSource>) -> IconBuilder {
    IconBuilder {
        source: Some(source.into()),
        fit: PathFit::Contain,
        style: Style::new(),
    }
}

/// Draw a registered icon. An unknown name draws nothing (and logs once in
/// debug builds).
pub fn icon_named(name: &str) -> IconBuilder {
    let source = icon_source(name);
    #[cfg(debug_assertions)]
    if source.is_none() {
        eprintln!("mue-components: unknown icon `{name}`");
    }
    IconBuilder {
        source,
        fit: PathFit::Contain,
        style: Style::new(),
    }
}

impl IconBuilder {
    pub fn fit(mut self, fit: PathFit) -> Self {
        self.fit = fit;
        self
    }

    /// Set width and height together.
    pub fn size(self, size: impl Into<Prop<Size>>) -> Self {
        let size = size.into();
        self.width(size).height(size)
    }
}

impl Styleable for IconBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for IconBuilder {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self { source, fit, style } = self;
        ctx.push_style(style);
        ctx.finish(Style::new().flex_shrink(0.), move |style| {
            // 1em square unless sized explicitly.
            let em = use_font_size().map(Size::Px);
            if style.declared_width().is_none() {
                let _ = (&mut *style).width(em);
            }
            if style.declared_height().is_none() {
                let _ = (&mut *style).height(em);
            }
            let layout = use_layout(style);
            let (rect, border, padding) = (layout.rect, layout.border, layout.padding);
            let paint = use_paint(style);
            use_background(style, rect);
            let color = style
                .declared_color()
                .or_else(mue_macroquad::inherit::inherited_foreground)
                .unwrap_or_else(|| style.take_color());
            let Some(source) = source else {
                return;
            };
            let shapes = paint.build(move |p| {
                let content = rect.get().deflate(border.get()).deflate(padding.get());
                if content.w <= 0. || content.h <= 0. {
                    return;
                }
                let current: Brush = color.get_clone();
                match &source {
                    IconSource::Svg(doc) => paint_svg(p, doc, content, fit, &current),
                    IconSource::Path {
                        data,
                        view_box,
                        stroke,
                    } => paint_path(
                        p,
                        data,
                        *view_box,
                        content,
                        fit,
                        stroke
                            .is_none()
                            .then(|| (current.clone(), FillRule::NonZero)),
                        stroke.map(|stroke| (current.clone(), stroke)),
                    ),
                    IconSource::Texture { texture, region } => {
                        let (w, h) = (texture.width(), texture.height());
                        let region = region.unwrap_or(Rect::new(0., 0., w, h));
                        let view_box = Rect::new(0., 0., region.w, region.h);
                        let m = fit.matrix(view_box, content);
                        let min = mue_macroquad::math::project_point(m, view_box.min());
                        let max = mue_macroquad::math::project_point(m, view_box.max());
                        let uv = Rect::new(region.x / w, region.y / h, region.w / w, region.h / h);
                        p.draw_texture(
                            Rect::new(min.x, min.y, max.x - min.x, max.y - min.y),
                            texture.clone(),
                            uv,
                            current.shader(content),
                        );
                    }
                }
            });
            style.on_render.append(move |_| shapes.get_clone().draw());
        })
    }
}
