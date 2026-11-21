//! Pixel checks for fixed SPIR-V, runtime GLSL, and texture region uploads.
use macroquad::{
    miniquad::{TextureUpdateRegion, UniformDesc, UniformType},
    prelude::*,
    retained::{self, RetainedMaterial, RetainedMesh, RetainedVertex},
    ui::root_ui,
};
use mue_macroquad::{
    App, CallbackResult, Scene, SceneRender, SceneTransition, UiMaterial, transition::Transition,
};

const RED: Color = Color::new(1., 0., 0., 1.);
const BLUE: Color = Color::new(0., 0., 1., 1.);
const GREEN: Color = Color::new(0., 1., 0., 1.);

const TINT_FRAGMENT: &str = r#"#version 100
precision mediump float;
uniform vec4 Tint;
void main() {
    gl_FragColor = Tint;
}
"#;

const SNAPSHOT_FRAGMENT: &str = r#"#version 100
precision mediump float;
uniform sampler2D _ScreenTexture;
varying lowp vec2 v_uv;
void main() {
    gl_FragColor = vec4(texture2D(_ScreenTexture, v_uv).bgr, 1.0);
}
"#;

fn check_pixel(label: &str, actual: Color, expected: Color) {
    let actual = [actual.r, actual.g, actual.b, actual.a];
    let expected = [expected.r, expected.g, expected.b, expected.a];
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(a, e)| (a - e).abs() <= 3. / 255.),
        "{label}: expected {expected:?}, got {actual:?}"
    );
}

fn check_texture_regions() {
    let texture = Texture2D::from_image(&Image::gen_image_color(8, 8, BLACK));
    let red = [255, 0, 0, 255].repeat(2);
    let green = [0, 255, 0, 255].repeat(6);
    let blue = [0, 0, 255, 255].repeat(4);
    texture.update_regions(&[]);
    texture.update_regions(&[
        TextureUpdateRegion {
            x: 0,
            y: 1,
            width: 1,
            height: 2,
            bytes: &red,
        },
        TextureUpdateRegion {
            x: 4,
            y: 5,
            width: 3,
            height: 2,
            bytes: &green,
        },
    ]);
    let image = texture.get_texture_data();
    for y in 0..8 {
        for x in 0..8 {
            let expected = if x == 0 && (1..3).contains(&y) {
                RED
            } else if (4..7).contains(&x) && (5..7).contains(&y) {
                GREEN
            } else {
                BLACK
            };
            check_pixel("disjoint texture regions", image.get_pixel(x, y), expected);
        }
    }

    // The second update must win where the two rectangles overlap.
    texture.update_regions(&[
        TextureUpdateRegion {
            x: 2,
            y: 2,
            width: 2,
            height: 1,
            bytes: &red,
        },
        TextureUpdateRegion {
            x: 3,
            y: 2,
            width: 2,
            height: 2,
            bytes: &blue,
        },
    ]);
    let image = texture.get_texture_data();
    check_pixel("first overlapping region", image.get_pixel(2, 2), RED);
    check_pixel("last region wins", image.get_pixel(3, 2), BLUE);
    check_pixel("previous update preserved", image.get_pixel(6, 6), GREEN);
}

fn check_ordered_texture_regions() {
    fn texel(x: i32, bytes: &[u8; 4]) -> TextureUpdateRegion<'_> {
        TextureUpdateRegion {
            x,
            y: 0,
            width: 1,
            height: 1,
            bytes,
        }
    }

    let source = Texture2D::from_image(&Image::gen_image_color(2, 1, RED));
    source.set_filter(FilterMode::Nearest);
    let target = render_target(8, 1);
    let camera = Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., 8., 1.))
    };
    let vertices = [[0., 0.], [2., 0.], [2., 1.], [0., 1.]].map(|position| RetainedVertex {
        position,
        texcoord: [0.25, 0.5], // Center of the first source texel.
        color: [1.; 4],
    });
    let mesh = [
        RetainedMesh::new(&vertices, &[0, 1, 2, 0, 2, 3], source.raw_miniquad_id())
            .expect("retained textured mesh"),
    ];

    set_camera(&camera);
    clear_background(BLACK);
    draw_texture(&source, 0., 0., WHITE);
    // Immediate draws must be flushed to order them before the upload.
    unsafe { get_internal_gl().flush() };
    let green = [0, 255, 0, 255];
    let mut upload_bytes = green;
    source.update_regions(&[texel(0, &upload_bytes), texel(1, &[0, 0, 255, 255])]);
    // The first upload must no longer depend on the caller's bytes.
    upload_bytes = [255, 0, 0, 255];
    draw_texture(&source, 2., 0., WHITE);

    let draw_retained = |x| {
        retained::draw_meshes_ranges(&mesh, &[], Mat4::from_translation(vec3(x, 0., 0.)), 1.);
    };
    draw_retained(4.);
    // Leave the retained pass open: the Vulkan upload must end it.
    source.update_regions(&[texel(0, &upload_bytes)]);
    draw_retained(6.);
    set_default_camera();
    let rendered = target.texture.get_texture_data();

    // The synchronous readback starts a fresh Vulkan command segment. No draw
    // or clear should be needed for another upload to be visible to a readback.
    source.update_regions(&[texel(1, &green)]);
    let updated = source.get_texture_data();

    check_pixel("immediate before upload", rendered.get_pixel(0, 0), RED);
    check_pixel("immediate after upload", rendered.get_pixel(2, 0), GREEN);
    check_pixel("second uploaded region", rendered.get_pixel(3, 0), BLUE);
    check_pixel("retained before upload", rendered.get_pixel(4, 0), GREEN);
    check_pixel("retained after upload", rendered.get_pixel(6, 0), RED);
    check_pixel("upload-only segment", updated.get_pixel(1, 0), GREEN);
}

fn check_offscreen_shaders() {
    let target = render_target(64, 64);
    let texture = target.texture.clone();
    let camera = Camera2D {
        render_target: Some(target),
        ..Camera2D::from_display_rect(Rect::new(0., 0., 64., 64.))
    };
    let vertices = [[0., 0.], [32., 0.], [32., 64.], [0., 64.]].map(|position| RetainedVertex {
        position,
        texcoord: [0., 0.],
        color: [1., 1., 1., 1.],
    });
    let mesh = [
        RetainedMesh::new(&vertices, &[0, 1, 2, 0, 2, 3], retained::white_texture())
            .expect("retained mesh"),
    ];
    let tint = RetainedMaterial::new(
        TINT_FRAGMENT,
        &[UniformDesc::new("Tint", UniformType::Float4)],
        &[],
    )
    .expect("runtime retained GLSL");
    tint.set_vec4("Tint", vec4(0., 1., 0., 1.));

    set_camera(&camera);
    clear_background(BLACK);
    retained::draw_meshes_ranges(&mesh, &[], Mat4::IDENTITY, 1.);
    retained::draw_meshes_ranges_with_material(
        &mesh,
        &[],
        Mat4::from_translation(vec3(32., 0., 0.)),
        1.,
        &tint,
    );
    set_default_camera();
    let image = texture.get_texture_data();
    check_pixel("default retained shader", image.get_pixel(16, 32), WHITE);
    check_pixel("runtime retained shader", image.get_pixel(48, 32), GREEN);

    let snapshot = UiMaterial::new(SNAPSHOT_FRAGMENT, Vec::new(), Vec::new())
        .expect("runtime snapshot material");
    set_camera(&camera);
    clear_background(RED);
    snapshot.with(|| draw_rectangle(0., 0., 64., 64., WHITE));
    set_default_camera();
    check_pixel(
        "snapshotter shader",
        texture.get_texture_data().get_pixel(32, 32),
        BLUE,
    );
}

struct SolidScene(Color);

impl Scene for SolidScene {
    fn render(&mut self, scene: &mut SceneRender<'_>) -> CallbackResult<()> {
        let bounds = scene.frame.viewport;
        draw_rectangle(bounds.x, bounds.y, bounds.w, bounds.h, self.0);
        Ok(())
    }
}

async fn run() {
    #[cfg(all(
        feature = "vulkan",
        any(
            windows,
            target_os = "android",
            all(target_os = "linux", not(target_env = "ohos"))
        )
    ))]
    assert_eq!(
        unsafe { get_internal_gl() }.quad_context.info().backend,
        miniquad::Backend::Vulkan
    );

    // This first frame creates and draws Macroquad's internal UI material.
    clear_background(BLACK);
    root_ui().canvas().rect(
        Rect::new(0., 0., screen_width(), screen_height()),
        None::<Color>,
        Some(RED),
    );
    next_frame().await;
    check_texture_regions();
    check_ordered_texture_regions();
    check_offscreen_shaders();
    let mut app = App::with_scene(SolidScene(RED));
    app.try_frame().expect("source scene");
    next_frame().await;
    app.push_scene(SolidScene(BLUE))
        .transition(SceneTransition::crossfade(Transition::linear(1.)));

    let deadline = get_time() + 10.;
    while get_time() < deadline {
        app.try_frame().expect("crossfade frame");
        if let Some(frame) = app.transition_state()
            && (0.2..0.8).contains(&frame.linear_progress)
        {
            // Android swapchains need not support transfer-source blits. Exercise
            // the real Scene transition there without requiring window readback.
            #[cfg(not(target_os = "android"))]
            {
                let image = get_screen_data();
                let center =
                    image.get_pixel(u32::from(image.width / 2), u32::from(image.height / 2));
                check_pixel(
                    "Scene crossfade",
                    center,
                    Color::new(1. - frame.progress, 0., frame.progress, 1.),
                );
            }
            next_frame().await;
            #[cfg(not(target_os = "android"))]
            {
                println!("Shader pixel checks passed");
                return;
            }
            #[cfg(target_os = "android")]
            {
                miniquad::info!("Shader pixel checks passed (offscreen); Scene crossfade rendered");
                // Keep presenting so the same run can test resize and resume.
                loop {
                    app.try_frame().expect("Scene frame after shader checks");
                    next_frame().await;
                }
            }
        }
        next_frame().await;
    }
    panic!("did not render an intermediate crossfade frame");
}

pub(crate) fn main() {
    macroquad::Window::from_config(
        Conf {
            window_title: "mue - Shader smoke".into(),
            window_width: 256,
            window_height: 192,
            window_resizable: false,
            #[cfg(all(
                feature = "vulkan",
                any(
                    windows,
                    target_os = "android",
                    all(target_os = "linux", not(target_env = "ohos"))
                )
            ))]
            platform: miniquad::conf::Platform {
                rendering_backend: miniquad::conf::RenderingBackend::Vulkan,
                ..Default::default()
            },
            ..Default::default()
        },
        run(),
    );
}
