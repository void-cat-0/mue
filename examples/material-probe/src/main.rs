//! A windowed example of immediate materials on mue geometry.
//!
//! Shows custom clipping with transformed geometry, independent material
//! uniforms, shadows, rounded and sector clips, and default-material drawing.
//!
//! Run with `cargo run --manifest-path examples/Cargo.toml --example material-probe --offline` and inspect the window.
//! Press Escape or close the window to quit.

use macroquad::prelude::*;
use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App, BoxShadows, Sector, UiMaterial,
    math::{Matrix, Rect as MueRect, vec2},
    node::{BuildNode, Node, NodeContext, flexbox, flexbox::use_container, use_rect},
    paint::color::Color as MueColor,
    paint::shader::SharedTexture,
    paint::{Shapes, use_paint},
    style::{Size, Style, Styleable, StyleableExt, Transform, TransformOp},
};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;
const BG: MueColor = MueColor::new(24. / 255., 26. / 255., 34. / 255., 1.);
const PANEL: MueColor = MueColor::new(46. / 255., 52. / 255., 74. / 255., 1.);
const GREEN: MueColor = MueColor::new(40. / 255., 200. / 255., 80. / 255., 1.);
const YELLOW: MueColor = MueColor::new(1., 200. / 255., 40. / 255., 1.);
const PURPLE: MueColor = MueColor::new(120. / 255., 80. / 255., 1., 1.);
const PINK: MueColor = MueColor::new(1., 80. / 255., 200. / 255., 1.);

const CLIP_FRAGMENT: &str = r#"#version 100
precision highp float;

varying highp vec2 v_pos;
varying lowp vec2 v_uv;
varying lowp vec4 v_color;

uniform highp vec4 rect;
uniform highp float radius;
uniform sampler2D Texture;

void main() {
    vec2 lower = rect.xy;
    vec2 upper = rect.zw;
    vec2 half_size = (upper - lower) * 0.5;
    vec2 center = lower + half_size;
    vec2 q = abs(v_pos - center) - half_size + vec2(radius);
    float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
    float mask = 1.0 - smoothstep(-0.5, 0.5, d);
    vec4 tex = texture2D(Texture, v_uv);
    gl_FragColor = tex * v_color;
    gl_FragColor.a *= mask;
}
"#;

fn translate(x: f32, y: f32) -> Matrix {
    Matrix::from_cols_array(&[1., 0., 0., 0., 1., 0., x, y, 1.])
}

fn scale_translate(sx: f32, sy: f32, tx: f32, ty: f32) -> Matrix {
    Matrix::from_cols_array(&[sx, 0., 0., 0., sy, 0., tx, ty, 1.])
}

fn checker_texture() -> SharedTexture {
    const SIZE: usize = 64;
    let mut pixels = vec![0u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let cell = (x / 8 + y / 8) % 2 == 0;
            let color = if cell {
                [255, 80, 80, 255]
            } else {
                [80, 200, 255, 255]
            };
            let i = (y * SIZE + x) * 4;
            pixels[i..i + 4].copy_from_slice(&color);
        }
    }
    Texture2D::from_rgba8(SIZE as u16, SIZE as u16, &pixels).into()
}

struct ClipQuad {
    texture: SharedTexture,
    material: UiMaterial,
    layout: MueRect,
    uniform: MueRect,
    radius: f32,
    transform: Matrix,
    style: Style,
}

impl ClipQuad {
    fn new(
        texture: SharedTexture,
        material: UiMaterial,
        layout: MueRect,
        uniform: MueRect,
        radius: f32,
        transform: Matrix,
    ) -> Self {
        Self {
            texture,
            material,
            layout,
            uniform,
            radius,
            transform,
            style: Style::new(),
        }
    }
}

impl Styleable for ClipQuad {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

impl BuildNode for ClipQuad {
    fn build(self, mut ctx: NodeContext) -> Node {
        let Self {
            texture,
            material,
            layout,
            uniform,
            radius,
            transform,
            style,
        } = self;

        let style = style
            .absolute()
            .inset_left(Size::Px(layout.x))
            .inset_top(Size::Px(layout.y))
            .width(Size::Px(layout.w))
            .height(Size::Px(layout.h))
            .transform(transform);
        ctx.push_style(style);

        ctx.finish(Style::new(), move |style| {
            use_container(style);
            let rect = use_rect();
            let paint = use_paint(style);
            style.on_render.append(move |_| {
                let r = rect.get();
                material.with(|| {
                    material.set_uniform(
                        "rect",
                        vec4(uniform.x, uniform.y, uniform.right(), uniform.bottom()),
                    );
                    material.set_uniform("radius", radius);
                    paint.draw(|p| {
                        p.draw_texture(
                            r,
                            texture.clone(),
                            MueRect::new(0., 0., 1., 1.),
                            MueColor::WHITE,
                        );
                    });
                });
            });
        })
    }
}

fn ui() -> impl Component {
    let texture = checker_texture();
    let retained_shapes = Shapes::build(|p| {
        for y in 0..30 {
            for x in 0..40 {
                p.fill(
                    &PathData::rect(MueRect::new(x as f32 * 2., y as f32 * 2., 2., 2.)),
                    FillRule::NonZero,
                    YELLOW,
                );
            }
        }
    });

    let clip_material = UiMaterial::new(
        CLIP_FRAGMENT,
        vec![
            UniformDesc::new("rect", UniformType::Float4),
            UniformDesc::new("radius", UniformType::Float1),
        ],
        Vec::new(),
    )
    .expect("clip material");
    flexbox()
        .style(
            Style::new()
                .width(Size::Px(WIDTH as f32))
                .height(Size::Px(HEIGHT as f32))
                .background(BG),
        )
        .children((
            ClipQuad::new(
                texture.clone(),
                clip_material.clone(),
                MueRect::new(40., 40., 200., 120.),
                MueRect::new(60., 50., 200., 120.),
                24.,
                translate(20., 10.),
            ),
            ClipQuad::new(
                texture.clone(),
                clip_material.clone(),
                MueRect::new(40., 200., 200., 120.),
                MueRect::new(40., 200., 200., 120.),
                12.,
                Matrix::IDENTITY,
            ),
            ClipQuad::new(
                texture,
                clip_material,
                MueRect::new(0., 0., 160., 100.),
                MueRect::new(300., 200., 200., 125.),
                25.,
                scale_translate(1.25, 1.25, 300., 200.),
            ),
            flexbox()
                .style(
                    Style::new()
                        .absolute()
                        .inset_left(Size::Px(300.))
                        .inset_top(Size::Px(40.))
                        .width(Size::Px(200.))
                        .height(Size::Px(120.))
                        .background(PANEL)
                        .radius(Size::Px(16.)),
                )
                .box_shadow(BoxShadows::elevation(8.)),
            flexbox()
                .style(
                    Style::new()
                        .absolute()
                        .inset_left(Size::Px(520.))
                        .inset_top(Size::Px(40.))
                        .width(Size::Px(80.))
                        .height(Size::Px(60.)),
                )
                .clip_radius(Size::Px(20.))
                .children(
                    shape(retained_shapes).style(
                        Style::new()
                            .absolute()
                            .inset_left(Size::Px(0.))
                            .inset_top(Size::Px(0.))
                            .width(Size::Px(80.))
                            .height(Size::Px(60.)),
                    ),
                ),
            flexbox()
                .style(
                    Style::new()
                        .absolute()
                        .inset_left(Size::Px(520.))
                        .inset_top(Size::Px(120.))
                        .width(Size::Px(100.))
                        .height(Size::Px(100.)),
                )
                .clip_sector(Sector::new(
                    vec2(50., 50.),
                    -std::f32::consts::FRAC_PI_2,
                    -std::f32::consts::FRAC_PI_4,
                ))
                .children(
                    flexbox().style(
                        Style::new()
                            .absolute()
                            .inset_left(Size::Px(0.))
                            .inset_top(Size::Px(0.))
                            .width(Size::Px(100.))
                            .height(Size::Px(100.))
                            .background(PURPLE),
                    ),
                ),
            flexbox()
                .style(
                    Style::new()
                        .absolute()
                        .inset_left(Size::Px(520.))
                        .inset_top(Size::Px(240.))
                        .width(Size::Px(80.))
                        .height(Size::Px(80.)),
                )
                .clip_radius(Size::Px(20.))
                .transform(Transform(vec![TransformOp::Rotate(
                    std::f32::consts::FRAC_PI_4,
                )]))
                .children(
                    flexbox().style(
                        Style::new()
                            .absolute()
                            .inset_left(Size::Px(0.))
                            .inset_top(Size::Px(0.))
                            .width(Size::Px(80.))
                            .height(Size::Px(80.))
                            .background(PINK),
                    ),
                ),
            flexbox().style(
                Style::new()
                    .absolute()
                    .inset_left(Size::Px(40.))
                    .inset_top(Size::Px(40.))
                    .width(Size::Px(20.))
                    .height(Size::Px(20.))
                    .background(GREEN),
            ),
        ))
}

#[macroquad::main("material demo")]
async fn main() {
    request_new_screen_size(WIDTH as f32, HEIGHT as f32);
    let mut app = App::new(ui());

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }

        clear_background(BG.into());
        app.frame();
        next_frame().await;
    }
}
