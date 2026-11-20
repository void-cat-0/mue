#version 100
precision mediump float;
uniform sampler2D Texture;
uniform sampler2D From;
uniform float Progress;
varying mediump vec2 uv;
void main() {
    vec3 from = texture2D(From, uv).rgb;
    vec3 to = texture2D(Texture, uv).rgb;
    gl_FragColor = vec4(mix(from, to, Progress), 1.0);
}
