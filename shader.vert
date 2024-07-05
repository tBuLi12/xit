#version 450

layout(location = 0) in vec2 pos;
layout(location = 1) in vec2 size;
layout(location = 2) in vec2 tex_coords;
layout(location = 3) in float tex_blend;
layout(location = 4) in vec4 bg_color;
layout(location = 5) in vec4 border_color;
layout(location = 6) in float border_width;
layout(location = 7) in float corner_radius;

layout(set = 0, binding = 0) uniform ViewportSizeUniform {
    vec2 size;
} viewport;

layout(location = 0) out RectData {
    vec2 pos;
    vec2 size;
    vec2 tex_coords;
    float tex_blend;
    vec4 bg_color;
    vec4 border_color;
    float border_width;
    float corner_radius;
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
    rect.bg_color = bg_color;
    rect.border_color = border_color;
    rect.border_width = border_width;
    rect.corner_radius = corner_radius;
    gl_Position = vec4((in_rect + pos) * 2.0 / viewport.size - vec2(1.0), 0.0, 1.0);
}
