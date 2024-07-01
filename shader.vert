#version 450

layout(location = 0) in vec2 pos;
layout(location = 1) in vec2 size;
layout(location = 2) in vec2 tex_coords;
layout(location = 3) in float tex_blend;

layout(set = 0, binding = 0) uniform ViewportSizeUniform {
    vec2 size;
} viewport;

layout(location = 0) out RectData {
    vec2 pos;
    vec2 size;
    vec2 tex_coords;
    float tex_blend;
} rect;

vec2 positions[6] = vec2[](
    vec2(0.0, 0.0),
    vec2(1.0, 1.0),
    vec2(1.0, 0.0),
    vec2(0.0, 0.0),
    vec2(0.0, 1.0),
    vec2(1.0, 1.0)
);

void main() {
    vec2 in_rect = positions[gl_VertexIndex] * size;
    rect.pos = in_rect;
    rect.size = size;
    rect.tex_coords = tex_coords;
    rect.tex_blend = tex_blend;
    gl_Position = vec4((in_rect + pos) * 2.0 / viewport.size - vec2(1.0), 0.0, 1.0);
}
