//! Moving diagonal stripes (百叶窗), the background effect of Phira's home
//! screen (`STRIPE_MATERIAL`).

use std::rc::Rc;

use macroquad::{
    math::Vec4,
    miniquad::{UniformDesc, UniformType},
};
use mue_core::prelude::*;
use mue_macroquad::{
    node::{BuildNode, Node, NodeContext, shader_box},
    paint::{color::Color, material::UiMaterial},
    style::{Style, Styleable},
};

const FRAGMENT: &str = r#"#version 100
precision mediump float;
varying highp vec2 v_pos;
uniform vec4 u_rect;
uniform float u_time;
uniform float u_angle;
uniform float u_period;
uniform float u_duty;
uniform float u_speed;
uniform vec4 u_color;
void main() {
    vec2 dir = vec2(cos(u_angle), sin(u_angle));
    // Distance across the stripes, in logical pixels from the box origin.
    float d = dot(v_pos - u_rect.xy, dir) - u_time * u_speed;
    float p = mod(d, u_period);
    float width = u_duty * u_period;
    // One logical pixel of antialiasing on both stripe edges.
    float band = smoothstep(0.0, 1.0, p) * (1.0 - smoothstep(width, width + 1.0, p));
    gl_FragColor = vec4(u_color.rgb, u_color.a * band);
}
"#;

/// Builder returned by [`stripes`].
pub struct StripesBuilder {
    angle: Prop<f32>,
    period: Prop<f32>,
    duty: Prop<f32>,
    speed: Prop<f32>,
    color: Prop<Color>,
    style: Style,
}

/// Diagonal stripes that drift across the box: the "blinds" background of
/// Phira's home screen. By default they run at 0.66 rad, 48 logical pixels
/// apart, half filled, move 18 pixels per second, and brighten what is
/// behind them by 7 % (white at 7 % opacity).
///
/// Place it behind content, for example as the first child of a `stack`
/// with `position: absolute` filling the parent. Every parameter accepts a
/// reactive value.
///
/// ```no_run
/// use mue_components::prelude::*;
///
/// let _background = stripes().angle(0.66).period(64.).speed(24.).style(style! {
///     width: 100%;
///     height: 100%;
///     background: #2a323c;
/// });
/// ```
pub fn stripes() -> StripesBuilder {
    StripesBuilder {
        angle: Prop::Static(0.66),
        period: Prop::Static(48.),
        duty: Prop::Static(0.5),
        speed: Prop::Static(18.),
        color: Prop::Static(Color::new(1., 1., 1., 0.07)),
        style: Style::new(),
    }
}

impl StripesBuilder {
    /// Direction across the stripes, in radians.
    pub fn angle(mut self, radians: impl Into<Prop<f32>>) -> Self {
        self.angle = radians.into();
        self
    }

    /// Distance from one stripe to the next, in logical pixels.
    pub fn period(mut self, pixels: impl Into<Prop<f32>>) -> Self {
        self.period = pixels.into();
        self
    }

    /// Filled fraction of each period (0..1).
    pub fn duty(mut self, fraction: impl Into<Prop<f32>>) -> Self {
        self.duty = fraction.into();
        self
    }

    /// Movement in logical pixels per second; 0 keeps them still.
    pub fn speed(mut self, pixels_per_second: impl Into<Prop<f32>>) -> Self {
        self.speed = pixels_per_second.into();
        self
    }

    /// Stripe color, including its alpha (the effect's intensity).
    pub fn color(mut self, color: impl Into<Prop<Color>>) -> Self {
        self.color = color.into();
        self
    }

    /// Stripe intensity: white at this opacity (0.07 is Phira's 7 % lift).
    pub fn intensity(self, alpha: f32) -> Self {
        self.color(Color::new(1., 1., 1., alpha.clamp(0., 1.)))
    }
}

impl Styleable for StripesBuilder {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

/// Macroquad deletes a pipeline as soon as its material is dropped, even
/// with draws still queued: submit them before the last reference goes.
struct FlushOnDrop(Rc<UiMaterial>);

impl Drop for FlushOnDrop {
    fn drop(&mut self) {
        unsafe { macroquad::window::get_internal_gl() }.flush();
    }
}

impl BuildNode for StripesBuilder {
    fn build(self, ctx: NodeContext) -> Node {
        let Self {
            angle,
            period,
            duty,
            speed,
            color,
            style,
        } = self;
        let material = Rc::new(
            UiMaterial::new(
                FRAGMENT,
                ["u_angle", "u_period", "u_duty", "u_speed"]
                    .into_iter()
                    .map(|name| UniformDesc::new(name, UniformType::Float1))
                    .chain([
                        UniformDesc::new("u_color", UniformType::Float4),
                        UniformDesc::new("u_rect", UniformType::Float4),
                    ])
                    .collect(),
                Vec::new(),
            )
            .expect("the stripes shader compiles"),
        );
        let guard = FlushOnDrop(material.clone());
        let upload = move || {
            let material = &guard.0;
            material.set_uniform("u_angle", angle.get_untracked());
            material.set_uniform("u_period", period.get_untracked().max(1.));
            material.set_uniform("u_duty", duty.get_untracked().clamp(0., 1.));
            material.set_uniform("u_speed", speed.get_untracked());
            let c = color.get_untracked();
            material.set_uniform("u_color", Vec4::new(c.r, c.g, c.b, c.a));
        };
        // Before the first frame's update, then every frame.
        upload();
        let mut builder = shader_box(material);
        builder.style_mut().on_update.append(move |_| upload());
        builder.style(style).build(ctx)
    }
}
