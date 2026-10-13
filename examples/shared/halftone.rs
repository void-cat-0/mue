//! A custom GPU material can live inside an ordinary Mue node.
use macroquad::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{layout::use_layout, node::Node, style::Style};

// Macroquad queues draws but deletes a material's pipeline as soon as it is dropped.
// Submit any pending draws before a departing page releases its material.
struct PageMaterial(Material);

impl Drop for PageMaterial {
    fn drop(&mut self) {
        unsafe { get_internal_gl() }.flush();
    }
}

const VERTEX: &str = r#"#version 100
attribute vec3 position;
uniform mat4 Model;
uniform mat4 Projection;
varying vec2 point;
void main() { point = position.xy; gl_Position = Projection * Model * vec4(position, 1.0); }
"#;
const FRAGMENT: &str = r#"#version 100
precision mediump float;
varying vec2 point;
uniform float time;
uniform float density;
uniform vec2 origin;
void main() {
    vec2 p = point - origin;
    float wave = sin(p.y * 0.022 + time) * 14.0;
    float stripe = sin((p.x + p.y * 0.45 + wave) * density - time * 2.0);
    float ink = smoothstep(-0.15, 0.15, stripe);
    vec3 a = vec3(0.12, 0.16, 0.29);
    vec3 b = vec3(0.98, 0.66, 0.42);
    gl_FragColor = vec4(mix(a, b, ink), 1.0);
}
"#;

pub fn halftone(style: Style, density: ReadSignal<f32>, moving: ReadSignal<bool>) -> Node {
    let material = PageMaterial(
        load_material(
            ShaderSource::Glsl {
                vertex: VERTEX,
                fragment: FRAGMENT,
            },
            MaterialParams {
                uniforms: vec![
                    UniformDesc::new("time", UniformType::Float1),
                    UniformDesc::new("density", UniformType::Float1),
                    UniformDesc::new("origin", UniformType::Float2),
                ],
                ..Default::default()
            },
        )
        .expect("halftone material"),
    );
    Node::build_with_style(style, move |style| {
        let rect = use_layout(style).rect;
        let mut time = 0.;
        style.on_render.append(move |_| {
            if moving.get() {
                time += get_frame_time();
            }
            let r = rect.get();
            let material = &material;
            material.0.set_uniform("time", time);
            material.0.set_uniform("density", density.get());
            material.0.set_uniform("origin", (r.x, r.y));
            gl_use_material(&material.0);
            draw_rectangle(r.x, r.y, r.w, r.h, WHITE);
            gl_use_default_material();
        });
    })
}
