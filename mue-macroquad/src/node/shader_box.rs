//! A node filled by a custom fragment shader.

use std::rc::Rc;

use mue_core::prelude::*;

use super::{BuildNode, Node, NodeContext};
use crate::{
    color::Color,
    layout::use_layout,
    material::UiMaterial,
    paint::{use_background, use_paint},
    style::{Style, Styleable},
};

/// Builder returned by [`shader_box`].
pub struct ShaderBoxBuilder {
    material: Rc<UiMaterial>,
    style: Style,
}

/// Fill the node's border box (with its corner radii) using `material`.
///
/// The material's built-in uniforms are set for each draw: `u_rect` is the
/// node's layout rect (the same space as `v_pos` when the node is not
/// transformed), plus `u_time`, `u_resolution`, and `u_dpi`; see
/// [`crate::material`]. Vertex color is the node's foreground `color` with
/// its opacity. Set other uniforms on the material from an `on_update` hook
/// or a reactive effect. The normal background, border, and outline still
/// draw; the shader fill draws above the background.
///
/// ```no_run
/// use mue_macroquad::{material::UiMaterial, node::shader_box, style::Styleable};
///
/// let stripes = UiMaterial::new(
///     r#"#version 100
///     precision mediump float;
///     varying highp vec2 v_pos;
///     uniform float u_time;
///     void main() {
///         float band = step(0.5, fract((v_pos.x + v_pos.y) / 24.0 - u_time * 0.5));
///         gl_FragColor = vec4(vec3(1.0), 0.08 * band);
///     }"#,
///     Vec::new(),
///     Vec::new(),
/// )
/// .unwrap();
/// let _background = shader_box(stripes).width(320.).height(200.);
/// ```
pub fn shader_box(material: impl Into<Rc<UiMaterial>>) -> ShaderBoxBuilder {
    ShaderBoxBuilder {
        material: material.into(),
        style: Style::new(),
    }
}

impl Styleable for ShaderBoxBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for ShaderBoxBuilder {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self { material, style } = self;
        ctx.push_style(style);
        ctx.finish(Style::new(), move |style| {
            let rect = use_layout(style).rect;
            let radii = crate::paint::declared_corner_radii(style);
            let color = style
                .declared_color()
                .unwrap_or(Prop::Static(Color::WHITE.into()));
            let paint = use_paint(style);
            use_background(style, rect);
            style.on_render.append(move |_| {
                let rect = rect.get();
                if rect.w <= 0. || rect.h <= 0. {
                    return;
                }
                let radii = radii.get().fit(rect);
                let tint = match color.get_clone() {
                    crate::shader::Brush::Solid(color) => color,
                    _ => Color::WHITE,
                };
                material.with_rect(rect, || {
                    paint.draw(|p| {
                        if radii.is_zero() {
                            p.fill_rect(rect, tint);
                        } else {
                            p.fill_rounded_rect(rect, radii, tint);
                        }
                    });
                });
            });
        })
    }
}
