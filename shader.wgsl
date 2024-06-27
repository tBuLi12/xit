@group(0)
@binding(0)
var<uniform> size: vec2<f32>;

var<private> positions: array<vec2<f32>, 6> = array(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(0.0, 0.0),
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 1.0),
);

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) size: vec2<f32>,
    @location(1) rect_pos: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) in: u32, @location(0) rect: vec4<f32>) -> VertexOutput {
    let pos = positions[in];
    var out: VertexOutput;
    out.position = vec4<f32>((pos.x * rect.z + rect.x) / size.x - 1., (pos.y * rect.w + rect.y) / size.y - 1., 0.0, 1.0);
    out.rect_pos = vec2<f32>(pos.x * rect.z, pos.y * rect.w);
    out.size = rect.zw;
    return out;
}

fn rect_sdf(pos: vec2<f32>, size: vec2<f32>, corner_radius: f32) -> f32 {
    let center = size / 2.0;
    let towards_corner = abs(pos - center);
    let shrunk_corner = center - corner_radius;
    let pixel_to_corner = max(vec2<f32>(0.0, 0.0), towards_corner - shrunk_corner); 
    let distance = length(pixel_to_corner) - corner_radius;
    return distance;
}

@fragment
fn fs_main(@location(1) pos: vec2<f32>, @location(0) size: vec2<f32>) -> @location(0) vec4<f32> {
    let distance = rect_sdf(pos, size, 30.0);
    let coeff = clamp(distance, -1.0, 1.0) / 2.0 + 0.5;
    let clear = vec4<f32>(0.0, 1.0, 0.0, 0.0);
    let red = vec4<f32>(1.0, 0.0, 0.0, 1.0);
    return mix(red, clear, coeff);
}
