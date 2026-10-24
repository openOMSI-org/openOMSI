// What the snowfall (snow.wgsl) and the rainfall (rain.wgsl) share: the camera, a hash of
// a particle's number, and the player's vehicle the weather is kept out of.
struct Camera {
    view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    // (kept in lockstep with `CameraUniform` in lib.rs, as sky.wgsl's)
    world_origin: vec4<f32>,
    sun_dir: vec4<f32>,
    ambient: vec4<f32>,
    fog: vec4<f32>,
    sun_color: vec4<f32>,
    sky_color: vec4<f32>,
    light_grid: vec4<f32>,
    sky: vec4<f32>,
    cam_right: vec4<f32>,
    cam_up: vec4<f32>,
    clouds: vec4<f32>,
    light_view_proj: mat4x4<f32>,
    light_view_proj_far: mat4x4<f32>,
    shadow: vec4<f32>,
    post: vec4<f32>,
    inside_a: vec4<f32>,
    inside_b: vec4<f32>,
    inside_c: vec4<f32>,
    flags: vec4<f32>,
    light_view_proj_close: mat4x4<f32>,
    wind: vec4<f32>,
    lamp_view_proj: array<mat4x4<f32>, 56>,
    lamp_atlas: vec4<f32>,
    tree_wind: vec4<f32>,
    inside2_a: vec4<f32>,
    inside2_b: vec4<f32>,
    inside2_c: vec4<f32>,
    cookies: array<vec4<f32>, 8>,
    cabin_lamps: array<vec4<f32>, 4>,
    // x: the textures' mip bias (FSR 1: negative, see `CameraUniform::lod`)
    lod: vec4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

fn hash4(n: u32) -> vec4<f32> {
    var x = vec4<u32>(n, n * 747796405u + 2891336453u, n ^ 0x9e3779b9u, n * 1664525u + 1013904223u);
    x = x * 1664525u + 1013904223u;
    x.x = x.x + x.y * x.w;
    x.y = x.y + x.z * x.x;
    x.z = x.z + x.x * x.y;
    x.w = x.w + x.y * x.z;
    x = x ^ (x >> vec4<u32>(16u));
    x.x = x.x + x.y * x.w;
    x.y = x.y + x.z * x.x;
    x.z = x.z + x.x * x.y;
    return vec4<f32>(x & vec4<u32>(0xffffffu)) / 16777216.0;
}

// The precipitation's roof map (passes/precip_map.rs): what stands over the ground round
// the camera, its depth seen from straight above.
@group(1) @binding(1) var roof_map: texture_depth_2d;
fn roof_depth(t: vec2<i32>) -> f32 { return textureLoad(roof_map, t, 0); }

// Whether something solid stands over `p` (the map's matrix `m`; `on` 0 without a map): a
// bus's or a car's roof, a shelter, a bridge, a house - no drop or flake falls there.
fn under_roof(p: vec3<f32>, m: mat4x4<f32>, on: f32) -> bool {
    let c = m * vec4<f32>(p, 1.0);
    let uv = vec2<f32>(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
    let side = f32(textureDimensions(roof_map).x);
    let t = vec2<i32>(clamp(uv, vec2<f32>(0.0), vec2<f32>(0.9999)) * side);
    let top = roof_depth(t);
    // (5 cm under it: the map's depth reaches 300 m)
    return on > 0.5 && all(abs(c.xy) < vec2<f32>(1.0)) && c.z > top + 0.00017;
}

// 1 inside the player's own vehicle (its box, a little wider): no snow falls in the cab
fn in_own_vehicle(p: vec3<f32>) -> bool {
    // (the front and, of an articulated bus, the rear section: it snowed in the back, #1967;
    // without branches - two boxes tested with early returns and short-circuits brought
    // Apple's M4 shader compiler service down, XPC_ERROR_CONNECTION_INTERRUPTED, and the
    // game fell back to no snowfall)
    return any(vec2<bool>(in_box(p, camera.inside_a, camera.inside_b, camera.inside_c), in_box(p, camera.inside2_a, camera.inside2_b, camera.inside2_c)));
}

fn in_box(p: vec3<f32>, a: vec4<f32>, b: vec4<f32>, c: vec4<f32>) -> bool {
    let d = p - a.xyz;
    let sh = a.w;
    let ch = b.x;
    let q = vec3<f32>(d.x * ch - d.y * sh - c.x, d.x * sh + d.y * ch - c.y, d.z - c.z);
    let h = b.yzw + vec3<f32>(0.3);
    return select(false, all(abs(q) < h), c.w >= 0.5);
}
