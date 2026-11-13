//! Probe immediate materials on mue geometry.
//!
//! Verifies:
//! - UiMaterial's fixed vertex shader sees global positions through mue's Model matrix;
//! - UiMaterial clips, Style::shadow, Style::clip_radius, and Style::clip_sector apply to mue's `Shapes` draws;
//! - two clips with different uniforms do not bleed;
//! - restoring the default material lets later draws through.
//!
//! Run with `cargo run -p material-probe --offline`; exits 0 when all checks pass.
//! Writes `/tmp/material-probe.png` for visual inspection.

use macroquad::prelude::*;
use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{
    App, Sector, Shadow, UiMaterial,
    color::Color as MueColor,
    math::{Matrix, Rect as MueRect, vec2},
    node::{BuildNode, Node, NodeContext, flexbox, flexbox::use_container, use_rect},
    paint::{Shapes, use_paint},
    shader::SharedTexture,
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
                p.fill_rect(MueRect::new(x as f32 * 2., y as f32 * 2., 2., 2.), YELLOW);
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
                .shadow(Shadow::new(10., 16.)),
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

fn pixel(image: &macroquad::texture::Image, x: f32, y: f32) -> Color {
    // Render-target readback is bottom-up; sample in top-left target coordinates.
    let y = image.height as u32 - 1 - y as u32;
    image.get_pixel(x as u32, y)
}

fn is_texture(color: Color) -> bool {
    color.r > 0.2 && color.b > 0.2
}

fn is_background(color: Color) -> bool {
    (color.r - BG.r).abs() < 0.03 && (color.g - BG.g).abs() < 0.03 && (color.b - BG.b).abs() < 0.03
}

fn is_green(color: Color) -> bool {
    color.g > 0.5 && color.g > color.r * 2. && color.g > color.b * 2.
}

fn is_yellow(color: Color) -> bool {
    color.r > 0.8 && color.g > 0.6 && color.b < 0.3
}

fn is_purple(color: Color) -> bool {
    color.b > 0.8 && color.r > 0.3 && color.g < 0.5
}

fn is_pink(color: Color) -> bool {
    color.r > 0.8 && color.b > 0.5 && color.g < 0.5
}

fn sum(color: Color) -> f32 {
    color.r + color.g + color.b
}

#[macroquad::main("material probe")]
async fn main() {
    request_new_screen_size(WIDTH as f32, HEIGHT as f32);

    let target = render_target(WIDTH, HEIGHT);
    target.texture.set_filter(FilterMode::Linear);
    let mut camera = Camera2D::from_display_rect(Rect::new(0., 0., WIDTH as f32, HEIGHT as f32));
    camera.render_target = Some(target.clone());

    let mut app = App::new(ui());
    if macroquad::retained::is_supported() {
        assert!(
            mue_macroquad::clip::retained_material_available(),
            "retained clip material"
        );
    }

    // Warm up layout and material pipelines.
    set_camera(&camera);
    clear_background(BG.into());
    app.frame();
    next_frame().await;

    // Measured frame.
    set_camera(&camera);
    clear_background(BG.into());
    app.frame();
    next_frame().await;
    set_default_camera();

    let image = target.texture.get_texture_data();
    image.export_png("/tmp/material-probe.png");

    let center_a = pixel(&image, 160., 110.);
    let corner_a = pixel(&image, 258., 52.);
    let corner_a2 = pixel(&image, 62., 168.);
    // Inside radius 24 but outside radius 12: catches a leaked clip uniform.
    let radius_a = pixel(&image, 230., 80.);
    // Inside the translated global rect but outside the untranslated local rect.
    let global_pos = pixel(&image, 250., 110.);
    let center_b = pixel(&image, 140., 260.);
    let corner_b = pixel(&image, 239., 201.);
    let corner_b2 = pixel(&image, 239., 319.);
    // Outside radius 12 but inside radius 24: the opposite leak check.
    let radius_b = pixel(&image, 236., 204.);
    let center_c = pixel(&image, 400., 262.);
    let corner_c = pixel(&image, 497., 203.);
    let shadow = pixel(&image, 298., 100.);
    let baseline = pixel(&image, 600., 300.);
    let panel = pixel(&image, 400., 100.);
    let restore = pixel(&image, 50., 50.);
    let clip_style_center = pixel(&image, 560., 70.);
    let clip_style_corner = pixel(&image, 522., 42.);
    let clip_style_edge = pixel(&image, 560., 42.);
    let sector_inside = pixel(&image, 582., 142.);
    let sector_outside = pixel(&image, 598., 160.);
    let rotated_center = pixel(&image, 560., 280.);
    let rotated_corner = pixel(&image, 590., 250.);

    let checks = [
        ("clip A center is texture", is_texture(center_a)),
        (
            "clip A top-right corner is background",
            is_background(corner_a),
        ),
        (
            "clip A bottom-left corner is background",
            is_background(corner_a2),
        ),
        ("clip A keeps radius 24", is_texture(radius_a)),
        ("clip A uses global v_pos", is_texture(global_pos)),
        ("clip B center is texture", is_texture(center_b)),
        (
            "clip B top-right corner is background",
            is_background(corner_b),
        ),
        (
            "clip B bottom-right corner is background",
            is_background(corner_b2),
        ),
        ("clip B keeps radius 12", is_texture(radius_b)),
        ("scaled clip center is texture", is_texture(center_c)),
        ("scaled clip corner is background", is_background(corner_c)),
        (
            "shadow darkens the background",
            sum(shadow) < sum(baseline) - 0.02,
        ),
        (
            "panel is drawn over the shadow",
            panel.b > 0.2 && panel.r < 0.3,
        ),
        ("default material is restored", is_green(restore)),
        (
            "Style::clip_radius center is child",
            is_yellow(clip_style_center),
        ),
        (
            "Style::clip_radius corner is background",
            is_background(clip_style_corner),
        ),
        (
            "Style::clip_radius edge is child",
            is_yellow(clip_style_edge),
        ),
        (
            "Style::clip_sector inside is child",
            is_purple(sector_inside),
        ),
        (
            "Style::clip_sector outside is background",
            is_background(sector_outside),
        ),
        ("rotated clip center is child", is_pink(rotated_center)),
        (
            "rotated clip corner is background",
            is_background(rotated_corner),
        ),
    ];

    println!("material probe: target {WIDTH}x{HEIGHT}");
    println!("  clip A center       {}", color_label(center_a));
    println!("  clip A corner       {}", color_label(corner_a));
    println!("  clip A corner 2     {}", color_label(corner_a2));
    println!("  clip A radius probe {}", color_label(radius_a));
    println!("  clip A global probe {}", color_label(global_pos));
    println!("  clip B center       {}", color_label(center_b));
    println!("  clip B corner       {}", color_label(corner_b));
    println!("  clip B corner 2     {}", color_label(corner_b2));
    println!("  clip B radius probe {}", color_label(radius_b));
    println!("  clip C center       {}", color_label(center_c));
    println!("  clip C corner       {}", color_label(corner_c));
    println!(
        "  shadow / baseline   {} / {}",
        color_label(shadow),
        color_label(baseline)
    );
    println!("  panel               {}", color_label(panel));
    println!("  restore             {}", color_label(restore));
    println!("  clip style center   {}", color_label(clip_style_center));
    println!("  clip style corner   {}", color_label(clip_style_corner));
    println!("  clip style edge     {}", color_label(clip_style_edge));
    println!("  sector inside       {}", color_label(sector_inside));
    println!("  sector outside      {}", color_label(sector_outside));
    println!("  rotated center      {}", color_label(rotated_center));
    println!("  rotated corner      {}", color_label(rotated_corner));

    let mut pass = true;
    for (name, ok) in checks {
        println!("  [{}] {name}", if ok { "PASS" } else { "FAIL" });
        pass &= ok;
    }
    println!("  image: /tmp/material-probe.png");
    std::process::exit(if pass { 0 } else { 1 });
}

fn color_label(color: Color) -> String {
    format!(
        "({:.0},{:.0},{:.0},{:.2})",
        color.r * 255.,
        color.g * 255.,
        color.b * 255.,
        color.a
    )
}
