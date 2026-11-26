//! Offscreen layers: group opacity, filters, and backdrop blur.
//!
//! [`offscreen`] renders its subtree into a render target and composites the
//! result once. That gives true group opacity (overlapping children do not
//! show through each other) and lets [`Filter`]s such as blur and grayscale
//! process the whole subtree. Content keeps its normal screen position, and
//! ancestor transforms and clips still apply.
//!
//! A node with `backdrop_blur` blurs whatever was drawn behind it, inside the
//! nearest offscreen ancestor (frosted glass). [`layer_host`](crate::overlay::layer::layer_host),
//! and therefore every [`App::new`](crate::App::new) root, becomes that
//! ancestor automatically while any backdrop-blurred node exists. Give the
//! root an opaque background so the backdrop has something to sample.
//!
//! Requirements and limits:
//!
//! - OpenGL only; on other backends offscreen layers draw directly and
//!   backdrop blur draws nothing.
//! - Each active layer costs a full-window render target and one extra
//!   composite per frame; blur adds two passes at reduced resolution.
//! - Semi-transparent content is drawn into a transparent target with the
//!   default blending, so alpha is reconstructed when compositing. A single
//!   layer of translucency is exact; translucent content stacked inside one
//!   offscreen layer is approximate.
//!
//! ```
//! use mue_macroquad::{paint::offscreen::{Filter, offscreen}, node::flexbox, style::Styleable};
//!
//! let _faded_group = offscreen()
//!     .opacity(0.5)
//!     .filter(Filter::Blur(4.))
//!     .children((flexbox().width(40.).height(40.), flexbox().width(40.).height(40.)));
//! ```

use std::{cell::RefCell, rc::Rc};

use macroquad::{
    color::{BLACK, WHITE},
    math::{Vec2, Vec4},
    miniquad::{UniformDesc, UniformType},
    prelude::{
        Camera2D, DrawTextureParams, FilterMode, RenderTarget, Texture2D, clear_background,
        draw_rectangle, draw_texture_ex, pop_camera_state, push_camera_state, render_target,
        set_camera,
    },
    window::{get_internal_gl, screen_dpi_scale, screen_height, screen_width},
};
use mue_core::{
    prop::{Prop, PropValue},
    signal::Access,
};

use crate::{
    node::{BuildNode, Node, NodeContext, NodeInner, flexbox::use_container},
    paint::material::UiMaterial,
    paint::render::RenderStateGuard,
    style::{Style, Styleable},
};

/// A whole-layer effect applied when an offscreen layer is composited.
#[derive(Clone, Debug)]
pub enum Filter {
    /// Gaussian-like blur with this radius in logical pixels.
    Blur(f32),
    /// Desaturate: 0 keeps colors, 1 is fully gray.
    Grayscale(f32),
    /// Multiply color: 1 keeps colors, 0 is black, above 1 brightens.
    Brightness(f32),
    /// Composite with a custom material instead of the built-in one. The
    /// fragment shader samples `Texture` at `v_uv` and receives `u_opacity`
    /// and the built-in uniforms. A texel stores `rgb = color * a` and
    /// `alpha = a * a` (see the module notes); recover `a = sqrt(alpha)`.
    Custom(Rc<UiMaterial>),
}

impl PartialEq for Filter {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Blur(a), Self::Blur(b))
            | (Self::Grayscale(a), Self::Grayscale(b))
            | (Self::Brightness(a), Self::Brightness(b)) => a == b,
            (Self::Custom(a), Self::Custom(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}
impl PropValue for Filter {}

/// Per-node offscreen configuration.
pub(crate) struct OffscreenState {
    filters: Prop<Vec<Filter>>,
    opacity: Prop<f32>,
    /// Only render offscreen while a backdrop-blurred node exists.
    backdrop_root: bool,
}

impl OffscreenState {
    pub(crate) fn backdrop_root() -> Rc<Self> {
        Rc::new(Self {
            filters: Prop::Static(Vec::new()),
            opacity: Prop::Static(1.),
            backdrop_root: true,
        })
    }
}

thread_local! {
    static POOL: RefCell<Vec<((u32, u32), RenderTarget)>> = const { RefCell::new(Vec::new()) };
    /// Render targets currently receiving an offscreen subtree, innermost last.
    static SOURCES: RefCell<Vec<RenderTarget>> = const { RefCell::new(Vec::new()) };
    static BACKDROP_USERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static MATERIALS: RefCell<Option<Materials>> = const { RefCell::new(None) };
}

fn supported() -> bool {
    unsafe { get_internal_gl() }.quad_context.info().backend == miniquad::Backend::OpenGl
}

fn window_pixels(scale: f32) -> (u32, u32) {
    let dpi = screen_dpi_scale();
    (
        ((screen_width() * dpi * scale).ceil() as u32).max(1),
        ((screen_height() * dpi * scale).ceil() as u32).max(1),
    )
}

fn acquire(size: (u32, u32)) -> RenderTarget {
    POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if let Some(index) = pool.iter().position(|(s, _)| *s == size) {
            return pool.swap_remove(index).1;
        }
        let target = render_target(size.0, size.1);
        target.texture.set_filter(FilterMode::Linear);
        target
    })
}

fn release(target: RenderTarget) {
    let size = (
        target.texture.width() as u32,
        target.texture.height() as u32,
    );
    POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        // Keep a small pool; drop targets from older window sizes.
        pool.retain(|((w, h), _)| {
            let current = window_pixels(1.);
            (*w <= current.0) && (*h <= current.1)
        });
        if pool.len() < 8 {
            pool.push((size, target));
        }
    });
}

/// Drop pooled render targets, for example after a large resize.
pub fn clear_pool() {
    POOL.with(|pool| pool.borrow_mut().clear());
}

const COMPOSITE: &str = r#"#version 100
precision mediump float;
varying lowp vec2 v_uv;
varying lowp vec4 v_color;
uniform sampler2D Texture;
uniform float u_opacity;
uniform float u_grayscale;
uniform float u_brightness;

void main() {
    vec4 c = texture2D(Texture, v_uv);
    // Content was blended onto transparent black with SrcAlpha/OneMinusSrcAlpha:
    // rgb = color * a and stored alpha = a * a. Recover a and the color.
    float a = sqrt(clamp(c.a, 0.0, 1.0));
    vec3 rgb = a > 0.0001 ? c.rgb / a : vec3(0.0);
    float gray = dot(rgb, vec3(0.2126, 0.7152, 0.0722));
    rgb = mix(rgb, vec3(gray), clamp(u_grayscale, 0.0, 1.0)) * u_brightness;
    gl_FragColor = vec4(rgb, a * u_opacity);
}
"#;

const BACKDROP: &str = r#"#version 100
precision mediump float;
varying highp vec2 v_pos;
varying lowp vec4 v_color;
uniform sampler2D Texture;
uniform vec2 u_resolution;

void main() {
    vec2 uv = vec2(v_pos.x / u_resolution.x, 1.0 - v_pos.y / u_resolution.y);
    vec4 c = texture2D(Texture, uv);
    float a = sqrt(clamp(c.a, 0.0, 1.0));
    vec3 rgb = a > 0.0001 ? c.rgb / a : vec3(0.0);
    gl_FragColor = vec4(rgb, a * v_color.a);
}
"#;

// Nine-tap separable kernel; offsets are multiplied by u_step.
const BLUR: &str = r#"#version 100
precision mediump float;
varying lowp vec2 v_uv;
uniform sampler2D Texture;
uniform vec2 u_step;

void main() {
    vec4 sum = texture2D(Texture, v_uv) * 0.2270270270;
    sum += texture2D(Texture, v_uv + u_step * 1.0) * 0.1945945946;
    sum += texture2D(Texture, v_uv - u_step * 1.0) * 0.1945945946;
    sum += texture2D(Texture, v_uv + u_step * 2.0) * 0.1216216216;
    sum += texture2D(Texture, v_uv - u_step * 2.0) * 0.1216216216;
    sum += texture2D(Texture, v_uv + u_step * 3.0) * 0.0540540541;
    sum += texture2D(Texture, v_uv - u_step * 3.0) * 0.0540540541;
    sum += texture2D(Texture, v_uv + u_step * 4.0) * 0.0162162162;
    sum += texture2D(Texture, v_uv - u_step * 4.0) * 0.0162162162;
    gl_FragColor = sum;
}
"#;

struct Materials {
    composite: UiMaterial,
    backdrop: UiMaterial,
    blur: macroquad::material::Material,
}

fn with_materials<R>(f: impl FnOnce(&Materials) -> R) -> R {
    MATERIALS.with(|materials| {
        let mut materials = materials.borrow_mut();
        let materials = materials.get_or_insert_with(|| Materials {
            composite: UiMaterial::new(
                COMPOSITE,
                vec![
                    UniformDesc::new("u_opacity", UniformType::Float1),
                    UniformDesc::new("u_grayscale", UniformType::Float1),
                    UniformDesc::new("u_brightness", UniformType::Float1),
                ],
                Vec::new(),
            )
            .expect("offscreen composite material"),
            backdrop: UiMaterial::new(BACKDROP, Vec::new(), Vec::new()).expect("backdrop material"),
            // Blur passes overwrite their target, so they use no blending.
            blur: macroquad::material::load_material(
                macroquad::miniquad::ShaderSource::Glsl {
                    vertex: crate::paint::material::UI_VERTEX,
                    fragment: BLUR,
                },
                macroquad::material::MaterialParams {
                    uniforms: vec![UniformDesc::new("u_step", UniformType::Float2)],
                    ..Default::default()
                },
            )
            .expect("blur material"),
        });
        f(materials)
    })
}

fn window_camera(target: &RenderTarget) -> Camera2D {
    Camera2D {
        render_target: Some(target.clone()),
        // Negative zoom.y agrees with mue's top-left scissor coordinates.
        ..Camera2D::from_display_rect(macroquad::math::Rect::new(
            0.,
            0.,
            screen_width(),
            screen_height(),
        ))
    }
}

fn reset_draw_state() {
    macroquad::material::gl_use_default_material();
    let gl = unsafe { get_internal_gl() }.quad_gl;
    gl.scissor(None);
    gl.viewport(None);
    gl.draw_mode(macroquad::prelude::DrawMode::Triangles);
}

/// Draw a full-window quad of `texture` in screen space, whatever the current
/// node transform is.
fn draw_window_quad(texture: &Texture2D) {
    let transform = crate::paint::current_transform();
    let gl = unsafe { get_internal_gl() }.quad_gl;
    let inverse = transform.inverse();
    let neutralize = transform != crate::math::Matrix::IDENTITY && inverse.is_finite();
    if neutralize {
        gl.push_model_matrix(crate::math::model_matrix(inverse));
    }
    draw_texture_ex(
        texture,
        0.,
        0.,
        WHITE,
        DrawTextureParams {
            dest_size: Some(Vec2::new(screen_width(), screen_height())),
            flip_y: true,
            ..Default::default()
        },
    );
    if neutralize {
        unsafe { get_internal_gl() }.quad_gl.pop_model_matrix();
    }
}

/// Render `draw` into a fresh target. The caller's camera and draw state are
/// restored afterwards; the node transform stack is kept, so content lands at
/// its normal screen position.
fn capture(draw: impl FnOnce()) -> RenderTarget {
    let target = acquire(window_pixels(1.));
    let scissor = unsafe { get_internal_gl() }.quad_gl.get_scissor();
    unsafe { get_internal_gl() }.flush();
    push_camera_state();
    let state = RenderStateGuard::enter();
    set_camera(&window_camera(&target));
    reset_draw_state();
    // Apply the all-channel default pipeline before clearing (see scene pictures).
    draw_rectangle(0., 0., 0., 0., BLACK);
    unsafe { get_internal_gl() }.flush();
    clear_background(macroquad::color::Color::new(0., 0., 0., 0.));
    unsafe { get_internal_gl() }.quad_gl.scissor(scissor);
    SOURCES.with(|sources| sources.borrow_mut().push(target.clone()));
    draw();
    SOURCES.with(|sources| sources.borrow_mut().pop());
    unsafe { get_internal_gl() }.flush();
    drop(state);
    pop_camera_state();
    target
}

/// Blur `source` (a full-window target) into a new target.
fn blur(source: &Texture2D, radius: f32) -> RenderTarget {
    let radius_px = radius.max(0.) * screen_dpi_scale();
    let scale = if radius_px > 32. {
        0.25
    } else if radius_px > 8. {
        0.5
    } else {
        1.
    };
    let size = window_pixels(scale);
    let first = acquire(size);
    let second = acquire(size);
    let state = RenderStateGuard::enter();
    push_camera_state();
    let pass = |target: &RenderTarget, input: &Texture2D, step: Vec2| {
        set_camera(&window_camera(target));
        reset_draw_state();
        with_materials(|materials| {
            materials.blur.set_uniform("u_step", step);
            macroquad::material::gl_use_material(&materials.blur);
        });
        // Full-window quad in screen space, whatever the node transform is.
        draw_window_quad(input);
        unsafe { get_internal_gl() }.flush();
        macroquad::material::gl_use_default_material();
    };
    // Four taps on each side cover the radius. Steps are in texture
    // coordinates of the full-resolution window, so both axes reach the same
    // distance whatever resolution the passes run at.
    let (full_w, full_h) = window_pixels(1.);
    let tap = (radius_px / 4.).max(1.);
    pass(&first, source, Vec2::new(tap / full_w as f32, 0.));
    pass(&second, &first.texture, Vec2::new(0., tap / full_h as f32));
    pop_camera_state();
    drop(state);
    release(first);
    second
}

fn composite(target: &RenderTarget, filters: &[Filter], opacity: f32) {
    let mut grayscale = 0.;
    let mut brightness = 1.;
    let mut blur_radius = 0.;
    let mut custom = None;
    for filter in filters {
        match filter {
            Filter::Blur(radius) => blur_radius += radius.max(0.),
            Filter::Grayscale(amount) => grayscale = f32::max(grayscale, *amount),
            Filter::Brightness(factor) => brightness *= factor.max(0.),
            Filter::Custom(material) => custom = Some(material.clone()),
        }
    }
    let blurred = (blur_radius > 0.).then(|| blur(&target.texture, blur_radius));
    let texture = blurred.as_ref().map_or(&target.texture, |b| &b.texture);
    match custom {
        Some(material) => {
            material.set_uniform("u_opacity", opacity);
            material.with(|| draw_window_quad(texture));
        }
        None => with_materials(|materials| {
            let material = &materials.composite;
            material.set_uniform("u_opacity", opacity);
            material.set_uniform("u_grayscale", grayscale);
            material.set_uniform("u_brightness", brightness);
            material.with(|| draw_window_quad(texture));
        }),
    }
    unsafe { get_internal_gl() }.flush();
    if let Some(blurred) = blurred {
        release(blurred);
    }
}

/// Render a node body through its offscreen state, or directly when inactive.
pub(crate) fn render(state: &OffscreenState, body: impl FnOnce()) {
    let active =
        supported() && (!state.backdrop_root || BACKDROP_USERS.with(std::cell::Cell::get) > 0);
    if !active {
        body();
        return;
    }
    let opacity = state.opacity.get();
    if opacity <= 0. {
        return;
    }
    let target = capture(body);
    let filters = state.filters.get_clone();
    composite(&target, &filters, opacity);
    release(target);
}

/// Register a node that uses backdrop blur for its lifetime.
pub(crate) fn backdrop_user() -> BackdropUser {
    BACKDROP_USERS.with(|users| users.set(users.get() + 1));
    BackdropUser
}

pub(crate) struct BackdropUser;

impl Drop for BackdropUser {
    fn drop(&mut self) {
        let _ = BACKDROP_USERS.try_with(|users| users.set(users.get().saturating_sub(1)));
    }
}

/// Fill `shape` (built in the node's local space) with a blurred copy of what
/// the nearest offscreen ancestor has drawn so far.
pub(crate) fn draw_backdrop(radius: f32, shape: impl FnOnce(&Texture2D)) {
    if !supported() {
        return;
    }
    let Some(source) = SOURCES.with(|sources| sources.borrow().last().cloned()) else {
        return;
    };
    unsafe { get_internal_gl() }.flush();
    let blurred = blur(&source.texture, radius);
    with_materials(|materials| {
        let material = &materials.backdrop;
        material.set_uniform("u_rect", Vec4::ZERO);
        material.with(|| shape(&blurred.texture));
    });
    unsafe { get_internal_gl() }.flush();
    release(blurred);
}

/// Builder returned by [`offscreen`].
pub struct OffscreenBuilder {
    filters: Vec<Filter>,
    dynamic_filters: Option<Prop<Vec<Filter>>>,
    style: Style,
}

/// A flex container whose subtree is rendered offscreen and composited with
/// its `opacity` and filters.
pub fn offscreen() -> OffscreenBuilder {
    OffscreenBuilder {
        filters: Vec::new(),
        dynamic_filters: None,
        style: Style::new(),
    }
}

impl OffscreenBuilder {
    /// Append a filter.
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }

    /// Replace all filters with a reactive list.
    pub fn filters(mut self, filters: impl Into<Prop<Vec<Filter>>>) -> Self {
        self.dynamic_filters = Some(filters.into());
        self
    }
}

impl Styleable for OffscreenBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for OffscreenBuilder {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            filters,
            dynamic_filters,
            mut style,
        } = self;
        // The layer applies opacity once when compositing; children draw opaque.
        let opacity = style.declared_opacity().unwrap_or(Prop::Static(1.));
        let _ = style.take_opacity();
        let filters = dynamic_filters.unwrap_or(Prop::Static(filters));
        ctx.push_style(style);
        ctx.finish(Style::new(), move |style| {
            use_container(style);
            NodeInner::with_mut(|node| {
                node.offscreen = Some(Rc::new(OffscreenState {
                    filters,
                    opacity,
                    backdrop_root: false,
                }));
            });
        })
    }
}
