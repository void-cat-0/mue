#version 100
precision mediump float;
uniform sampler2D Texture;
varying mediump vec2 uv;
void main() {
    gl_FragColor = vec4(texture2D(Texture, uv).rgb, 1.0);
}
