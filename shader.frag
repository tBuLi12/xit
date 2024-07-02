#version 450

layout (set = 0, binding = 1) uniform sampler2D tex;

layout(location = 0) out vec4 outColor;

layout(location = 0) in RectData {
    vec2 pos;
    vec2 size;
    vec2 tex_coords;
    float tex_blend;
} rect;

float rect_sdf(vec2 pos, vec2 size, float corner_radius) {  
    vec2 center = size / 2.0;
    vec2 towards_corner = abs(pos - center);
    vec2 shrunk_corner = center - corner_radius;
    vec2 pixel_to_corner = max(vec2(0.0), towards_corner - shrunk_corner); 
    float dist = length(pixel_to_corner) - corner_radius;
    return dist;
}

void main() {
    // float corner_radius = 30.0;
    // float border_width = 4.0;
    float corner_radius = 10.0;
    float border_width = 4.0;

    float dist = rect_sdf(rect.pos, rect.size, corner_radius);
    float coeff = clamp(dist, -0.5, 0.5) + 0.5;
    float border_coeff = clamp(dist + border_width, -0.5, 0.5) + 0.5; 
    float adjusted_coeff = max(0.0, border_coeff - coeff);
    vec4 clear = vec4(0.0, 0.0, 0.0, 0.0); 
    vec4 red = vec4(1.0, 0.0, 0.0, 1.0);
    vec4 green = vec4(0.0, 1.0, 0.0, 1.0);
    vec4 blue = vec4(0.0, 0.0, 1.0, 1.0);
    
    float inner = 1.0 - adjusted_coeff - coeff;
    vec4 rect_color = clear * coeff + red * inner + green * adjusted_coeff;
    // vec4 rect_color = clear * border_coeff + red * (1.0 - border_coeff);
    // vec4 rect_color = red;
    
    float tex_alpha = textureLod(tex, (rect.tex_coords + rect.pos), 0).r;
    vec4 tex_color = mix(clear, blue, tex_alpha);

    outColor = mix(rect_color, tex_color, rect.tex_blend);
    
    // outColor = clear * coeff + red * inner + green * adjusted_coeff;
    // outColor = vec4(rect.tex_coords / vec2(204.0, 204.0), 0.0, 1.0);
}
