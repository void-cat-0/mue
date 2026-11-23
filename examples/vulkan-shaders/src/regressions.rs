//! Regression checks for render-pass state, shader inputs, and resource lifetimes.
use macroquad::{
    miniquad::{self as mq, RenderingBackend},
    prelude::*,
    retained::{self, RetainedMesh, RetainedVertex},
};

use super::{BLUE, GREEN, RED, check_pixel};

pub(super) async fn check() {
    check_retained_passes();
    check_named_vertex_attributes();
    check_texture_transfer_lengths();
    check_buffer_contents_survive_readback().await;
    check_render_target_attachments();
}

fn camera(target: &RenderTarget) -> Camera2D {
    Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(
            0.,
            0.,
            target.texture.width(),
            target.texture.height(),
        ))
    }
}

fn quad(x: f32, texture: &Texture2D) -> RetainedMesh {
    let corners = [[x, 0.], [x + 8., 0.], [x + 8., 8.], [x, 8.]];
    let vertices = corners.map(|position| RetainedVertex {
        position,
        texcoord: [0.5, 0.5],
        color: [1.; 4],
    });
    RetainedMesh::new(&vertices, &[0, 1, 2, 0, 2, 3], texture.raw_miniquad_id()).unwrap()
}

fn check_retained_passes() {
    let target = render_target(16, 8);
    let target_camera = camera(&target);
    let red = Texture2D::from_rgba8(1, 1, &[255, 0, 0, 255]);
    let blue = Texture2D::from_rgba8(1, 1, &[0, 0, 255, 255]);
    let meshes = [quad(0., &red), quad(8., &blue)];
    let draw = |meshes| retained::draw_meshes_ranges(meshes, &[], Mat4::IDENTITY, 1.);

    set_camera(&target_camera);
    clear_background(BLACK);
    draw(&meshes);
    set_default_camera();
    let pixels = target.texture.get_texture_data();
    check_pixel("first retained mesh", pixels.get_pixel(4, 4), RED);
    check_pixel(
        "second retained mesh shares uniforms",
        pixels.get_pixel(12, 4),
        BLUE,
    );

    set_camera(&target_camera);
    clear_background(BLACK);
    draw(&meshes[..1]);
    // Vulkan's synchronous upload ends rendering without a Macroquad flush.
    let _texture = Texture2D::from_rgba8(1, 1, &[255; 4]);
    draw(&meshes[1..]);
    set_default_camera();
    let pixels = target.texture.get_texture_data();
    check_pixel(
        "before incidental texture creation",
        pixels.get_pixel(4, 4),
        RED,
    );
    check_pixel(
        "pass reopened after texture creation",
        pixels.get_pixel(12, 4),
        BLUE,
    );

    set_camera(&target_camera);
    clear_background(GREEN);
    draw(&meshes[..1]);
    // The new texture closes Vulkan's rendering instance, not the retained target.
    let _texture = Texture2D::from_rgba8(1, 1, &[255; 4]);
    set_default_camera();
    let ctx = unsafe { get_internal_gl() }.quad_context;
    ctx.clear(Some((0., 0., 1., 1.)), None, None);
    ctx.end_render_pass();
    check_pixel(
        "ending retained rendering releases the offscreen target",
        target.texture.get_texture_data().get_pixel(12, 4),
        GREEN,
    );

    // Binding a freshly rendered image requires a layout transition and a new
    // Vulkan rendering instance, but must not discard already applied uniforms.
    let source = render_target(4, 4);
    let sample = quad(0., &source.texture);
    set_camera(&camera(&source));
    clear_background(GREEN);
    set_camera(&target_camera);
    clear_background(BLACK);
    draw(std::slice::from_ref(&sample));
    set_default_camera();
    check_pixel(
        "retained sample from rendered texture",
        target.texture.get_texture_data().get_pixel(4, 4),
        GREEN,
    );
}

fn check_named_vertex_attributes() {
    // Different declaration order and no texcoord: color0 still reads packed
    // color bytes, not the intervening UV attribute in the vertex buffer.
    let material = load_material(
        ShaderSource::Glsl {
            vertex: "#version 100\n\
                attribute vec4 color0;\n\
                attribute vec3 position;\n\
                uniform mat4 Projection;\n\
                uniform mat4 Model;\n\
                varying lowp vec4 v_color;\n\
                void main() {\n\
                    gl_Position = Projection * Model * vec4(position, 1.0);\n\
                    v_color = color0 / 255.0;\n\
                }",
            fragment: "#version 100\n\
                precision mediump float;\n\
                varying lowp vec4 v_color;\n\
                void main() { gl_FragColor = v_color; }",
        },
        MaterialParams::default(),
    )
    .unwrap();
    let target = render_target(8, 8);
    set_camera(&camera(&target));
    clear_background(BLACK);
    gl_use_material(&material);
    draw_rectangle(0., 0., 8., 8., RED);
    gl_use_default_material();
    set_default_camera();
    check_pixel(
        "runtime GLSL vertex input bound by name",
        target.texture.get_texture_data().get_pixel(4, 4),
        RED,
    );
}

fn check_texture_transfer_lengths() {
    let ctx = unsafe { get_internal_gl() }.quad_context;
    // A larger caller buffer must not change the texel format or the amount copied.
    let texture = ctx.new_texture_from_data_and_format(
        &[255, 0, 0, 123],
        mq::TextureParams {
            width: 1,
            height: 1,
            format: mq::TextureFormat::RGB8,
            ..Default::default()
        },
    );
    let mut bytes = [123; 4];
    ctx.texture_read_pixels(texture, &mut bytes);
    assert_eq!(bytes, [255, 0, 0, 123]);
    ctx.texture_resize(texture, 1, 2, Some(&[0, 255, 0, 0, 0, 255, 123]));
    let mut bytes = [123; 7];
    ctx.texture_read_pixels(texture, &mut bytes);
    assert_eq!(bytes, [0, 255, 0, 0, 0, 255, 123]);
    ctx.delete_texture(texture);
}

async fn check_buffer_contents_survive_readback() {
    let target = render_target(16, 8);
    let pass = target.render_pass.raw_miniquad_id();
    let (shader, pipeline, a, b, indices) = {
        let ctx = unsafe { get_internal_gl() }.quad_context;
        let shader = ctx
            .new_shader(
                mq::ShaderSource::Glsl {
                    vertex: "#version 100\n\
                        attribute vec2 position;\n\
                        uniform vec2 Offset;\n\
                        void main() { gl_Position = vec4(position + Offset, 0.0, 1.0); }",
                    fragment: "#version 100\n\
                        precision mediump float;\n\
                        uniform vec4 Tint;\n\
                        void main() { gl_FragColor = Tint; }",
                },
                mq::ShaderMeta {
                    images: vec![],
                    uniforms: mq::UniformBlockLayout {
                        uniforms: vec![
                            mq::UniformDesc::new("Offset", mq::UniformType::Float2),
                            mq::UniformDesc::new("Tint", mq::UniformType::Float4),
                        ],
                    },
                },
            )
            .unwrap();
        let pipeline = ctx.new_pipeline(
            &[mq::BufferLayout::default()],
            &[mq::VertexAttribute::new(
                "position",
                mq::VertexFormat::Float2,
            )],
            shader,
            mq::PipelineParams::default(),
        );
        let a = ctx.new_buffer(
            mq::BufferType::VertexBuffer,
            mq::BufferUsage::Dynamic,
            mq::BufferSource::slice(&positions(0.)),
        );
        let b = ctx.new_buffer(
            mq::BufferType::VertexBuffer,
            mq::BufferUsage::Dynamic,
            mq::BufferSource::empty::<[f32; 2]>(4),
        );
        let indices = ctx.new_buffer(
            mq::BufferType::IndexBuffer,
            mq::BufferUsage::Immutable,
            mq::BufferSource::slice(&[0u16, 1, 2, 0, 2, 3]),
        );
        (shader, pipeline, a, b, indices)
    };
    let draw = |ctx: &mut dyn RenderingBackend, init_uniforms: bool| {
        ctx.begin_pass(Some(pass), mq::PassAction::clear_color(0., 0., 0., 1.));
        ctx.apply_pipeline(&pipeline);
        ctx.apply_bindings_from_slice(&[a], indices, &[]);
        if init_uniforms {
            ctx.apply_uniforms(mq::UniformsSource::table(&[0f32, 0., 1., 0., 0., 1.]));
        }
        ctx.draw(0, 6, 1);
        ctx.end_render_pass();
    };
    let check_left = |label| {
        let pixels = target.texture.get_texture_data();
        check_pixel(label, pixels.get_pixel(4, 4), RED);
        check_pixel(label, pixels.get_pixel(12, 4), BLACK);
    };
    {
        let ctx = unsafe { get_internal_gl() }.quad_context;
        // Partial first update: the initial suffix is still needed by the quad.
        ctx.buffer_update(a, mq::BufferSource::slice(&positions(0.)[..2]));
        draw(ctx, true);
    }
    check_left("updated vertex prefix preserves the initial suffix");
    {
        let ctx = unsafe { get_internal_gl() }.quad_context;
        // Make B take the first slice in the next segment. A must not point
        // at that slice, and the shader still has the uniforms uploaded above.
        ctx.buffer_update(b, mq::BufferSource::slice(&positions(1.)));
        ctx.begin_pass(Some(pass), mq::PassAction::Nothing);
        ctx.apply_pipeline(&pipeline);
        ctx.apply_bindings_from_slice(&[b], indices, &[]);
        ctx.draw(0, 6, 1);
        ctx.end_render_pass();
        draw(ctx, false);
    }
    check_left("A's buffer and uniforms survive unrelated update after readback");
    next_frame().await;
    {
        let ctx = unsafe { get_internal_gl() }.quad_context;
        draw(ctx, false);
    }
    check_left("buffer and uniforms survive the next frame slot");
    let ctx = unsafe { get_internal_gl() }.quad_context;
    ctx.delete_buffer(a);
    ctx.delete_buffer(b);
    ctx.delete_buffer(indices);
    ctx.delete_pipeline(pipeline);
    ctx.delete_shader(shader);
}

fn positions(shift: f32) -> [[f32; 2]; 4] {
    [
        [-1. + shift, -1.],
        [shift, -1.],
        [shift, 1.],
        [-1. + shift, 1.],
    ]
}

fn check_render_target_attachments() {
    use macroquad::texture::{RenderTargetParams, render_target_ex};

    // A 2x request exercises fallback on drivers with only 1x and 4x.
    // Request 4x separately so those drivers also execute a real resolve.
    for requested in [2, 4] {
        let target = render_target_ex(
            8,
            8,
            RenderTargetParams {
                sample_count: requested,
                depth: true,
            },
        );
        let ctx = unsafe { get_internal_gl() }.quad_context;
        let color = ctx.texture_params(target.render_pass.color_texture.raw_miniquad_id());
        let depth = ctx.texture_params(
            target
                .render_pass
                .depth_texture
                .as_ref()
                .unwrap()
                .raw_miniquad_id(),
        );
        // Legacy GL may round a requested count up during allocation; compare
        // actual attachments, not the pre-allocation count, on that backend.
        assert_eq!(color.sample_count, depth.sample_count);
        assert_eq!(
            target.render_pass.resolve_texture.is_some(),
            color.sample_count > 1
        );
        if let Some(resolve) = &target.render_pass.resolve_texture {
            assert_eq!(
                ctx.texture_params(resolve.raw_miniquad_id()).sample_count,
                1
            );
        }
        assert!(ctx.texture_supports_color_attachment(target.texture.raw_miniquad_id()));

        #[cfg(all(
            feature = "vulkan",
            any(
                windows,
                target_os = "android",
                all(target_os = "linux", not(target_env = "ohos"))
            )
        ))]
        if ctx.info().backend == mq::Backend::Vulkan {
            let samples = ctx.render_target_sample_count(
                mq::TextureFormat::RGBA8,
                Some(mq::TextureFormat::Depth),
                requested,
            );
            assert_eq!(color.sample_count, samples);
            if requested == 4 && samples < 4 {
                eprintln!("4x offscreen MSAA unsupported: testing {samples}x instead");
            }
        }

        set_camera(&camera(&target));
        clear_background(GREEN);
        draw_rectangle(0., 0., 4., 8., RED);
        set_default_camera();
        let image = target.texture.get_texture_data();
        check_pixel("offscreen resolve: drawn half", image.get_pixel(2, 4), RED);
        check_pixel(
            "offscreen resolve: clear half",
            image.get_pixel(6, 4),
            GREEN,
        );
    }

    #[cfg(all(
        feature = "vulkan",
        any(
            windows,
            target_os = "android",
            all(target_os = "linux", not(target_env = "ohos"))
        )
    ))]
    {
        let uploaded = Texture2D::from_rgba8(1, 1, &[255; 4]);
        let ctx = unsafe { get_internal_gl() }.quad_context;
        if ctx.info().backend == mq::Backend::Vulkan {
            assert!(!ctx.texture_supports_color_attachment(uploaded.raw_miniquad_id()));
        }
    }
}
