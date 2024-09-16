struct Uniforms {
    time: f32,
    delta_time: f32,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

fn rotate(v: vec2<f32>, angle: f32) -> vec2<f32> {
    let cosine = cos(angle);
    let sine = sin(angle);

    return vec2<f32>(
        v.x * cosine - v.y * sine,
        v.x * sine + v.y * cosine
    );
}

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(in_vertex_index) - 1) * 0.5;
    let y = f32(i32(in_vertex_index & 1u) * 2 - 1) * 0.5;
    let rotated = rotate(vec2<f32>(x, y), uniforms.time * 0.2);
    return vec4<f32>(rotated, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.0, 0.0, 1.0);
}
