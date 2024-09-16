struct Uniforms {
    time: f32,
    delta_time: f32,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(1) @binding(0) var texture: texture_storage_2d<rgba16float, write>;

const CLEAR_COLOR: vec4<f32> = vec4<f32>(0.0, 0.0, 0.0, 1.0);

// [0, 1]
fn random01(seed: vec2<f32>) -> f32 {
    return fract(dot(sin(seed), vec2(120.9898, 78.233)) * 2398.24531);
}

@compute @workgroup_size(8, 8, 1)
fn dissolve(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let coords = vec2<i32>(global_id.xy);
    if random01(vec2<f32>(coords) + vec2<f32>(uniforms.time, uniforms.delta_time)) > 0.97 {
        textureStore(texture, coords, CLEAR_COLOR);
    }
}
