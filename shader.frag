#version 450

layout (set = 0, binding = 1) uniform sampler2D tex;

layout(location = 0, index = 0) out vec4 outColor;
layout(location = 0, index = 1) out vec4 outBlend;

layout(location = 0) in RectData {
    vec2 pos;
    vec2 size;
    vec2 tex_coords;
    float tex_blend;
    vec4 bg_color;
    vec4 border_color;
    float border_width;
    float corner_radius;
} rect;

float rect_sdf(vec2 pos, vec2 size, float corner_radius) {  
    vec2 center = size / 2.0;
    vec2 towards_corner = abs(pos - center);
    vec2 shrunk_corner = center - corner_radius;
    vec2 to_shrunk_corner = towards_corner - shrunk_corner;
    vec2 pixel_to_corner = max(vec2(0.0), to_shrunk_corner); 
    float dist = length(pixel_to_corner) - corner_radius;
    float dist_to_edge = max(to_shrunk_corner.x, to_shrunk_corner.y) - corner_radius;
    return pixel_to_corner.x > 0.0 && pixel_to_corner.y > 0.0 ? dist : dist_to_edge;
}

const float gamma = 1.0;

void main() {
    float corner_radius = rect.corner_radius;
    float border_width = rect.border_width;

    float dist = rect_sdf(rect.pos, rect.size, corner_radius);
    float coeff = clamp(dist, -0.5, 0.5) + 0.5;
    float border_coeff = clamp(dist + border_width, -0.5, 0.5) + 0.5; 
    float adjusted_coeff = max(0.0, border_coeff - coeff);
    vec4 clear = vec4(0.0, 0.0, 0.0, 0.0); 
    vec4 red = vec4(1.0, 0.0, 0.0, 1.0);
    vec4 green = vec4(0.0, 1.0, 0.0, 1.0);
    vec4 blue = vec4(0.0, 0.0, 1.0, 1.0);
    
    float inner = 1.0 - adjusted_coeff - coeff;
    vec4 rect_color = clear * coeff + rect.bg_color * inner + rect.border_color * adjusted_coeff;
    // vec4 rect_color = green * border_coeff + red * (1.0 - border_coeff);
    // vec4 rect_color = red;
    vec4 channel_alphas = textureLod(tex, (rect.tex_coords + rect.pos), 0).bgra;
    // vec3 border_linear = pow(rect.border_color.rgb, vec3(1. / gamma));
    // vec3 bg_linear = pow(rect.bg_color.rgb, vec3(1. / gamma));
    // float text_r = mix(bg_linear.r, border_linear.r, channel_alphas.r);
    // float text_g = mix(bg_linear.g, border_linear.g, channel_alphas.g);
    // float text_b = mix(bg_linear.b, border_linear.b, channel_alphas.b);
    // vec4 tex_color = vec4(pow(vec3(text_r, text_g, text_b), vec3(gamma)), 1.0);
    
    // vec4 tex_color = mix(clear, rect.bg_color, tex_alpha);
    // vec4 tex_color = mix(green, blue, tex_alpha);

    outColor = mix(rect_color, rect.border_color, rect.tex_blend);
    outBlend = mix(rect_color.aaaa, channel_alphas, rect.tex_blend); 
    
    // outColor = clear * coeff + red * inner + green * adjusted_coeff;
    // outColor = vec4(rect.tex_coords / vec2(204.0, 204.0), 0.0, 1.0);
}
