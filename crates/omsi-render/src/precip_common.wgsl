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
    // (the classic picture's headlight, `CameraUniform::spot`: unread here)
    spot: array<vec4<f32>, 4>,
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

// Enhanced: the sun's far shadow map (the scene's camera group), which keeps every caster -
// a drop or a flake in the shade of a house or under the trees does not catch the sun.
@group(0) @binding(6) var s_shadow: sampler_comparison;
@group(0) @binding(7) var t_shadow_far: texture_depth_2d;

// Enhanced: the procedural flake atlas (precip.rs `FlakeAtlas`): eight cells of snow
// crystals and flakes, R how much of a texel they cover, G how bright it is.
@group(1) @binding(2) var t_flakes: texture_2d<f32>;
@group(1) @binding(3) var s_flakes: sampler;

// How far `p` is over whatever stands uppermost there by the roof map (m): below 0 under a
// roof, and far over everything where there is no map.
fn roof_clearance(p: vec3<f32>, m: mat4x4<f32>, on: f32) -> f32 {
    let c = m * vec4<f32>(p, 1.0);
    let uv = vec2<f32>(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
    let side = f32(textureDimensions(roof_map).x);
    let top = roof_depth(vec2<i32>(clamp(uv, vec2<f32>(0.0), vec2<f32>(0.9999)) * side));
    // (the map looks straight down: its depth goes against the height, m[2].z < 0)
    let h = (c.z - top) / m[2].z;
    return select(1e4, h, on > 0.5 && all(abs(c.xy) < vec2<f32>(1.0)) && top < 0.9999);
}

// The wind's gusts: it swells and slackens over some seconds, each swell running downwind
// over the ground as a wave some 70 m long, so that neighbouring drops and flakes are blown
// together and a gust is seen to sweep through the rain. x: the time's worth of the mean
// wind the gusts have added to a particle's way by `t` (the swell's integral, s), y: the
// swell now (a share of the mean wind).
fn precip_gust(x: vec2<f32>, t: f32, wind: vec2<f32>, strength: f32) -> vec2<f32> {
    let speed = length(wind);
    let along = dot(x, wind / max(speed, 1e-3)) * (6.2831853 / 70.0);
    let across = dot(x, vec2<f32>(-wind.y, wind.x) / max(speed, 1e-3)) * (6.2831853 / 160.0);
    let w1 = 0.42;
    let w2 = 1.17;
    let a1 = 0.38 * strength;
    let a2 = 0.17 * strength;
    let p1 = w1 * t - along + 0.6 * sin(across);
    let p2 = w2 * t - 2.3 * along + 1.7 + across;
    return vec2<f32>(-a1 * cos(p1) / w1 - a2 * cos(p2) / w2, a1 * sin(p1) + a2 * sin(p2));
}

// How much of the sun reaches `p` (Enhanced): one compare in the far cascade, lit beyond it
// and without shadow maps.
fn precip_sun(p: vec3<f32>) -> f32 {
    if (camera.shadow.x < 0.5 || enh.sun.r + enh.sun.g < 1e-4) {
        return 1.0;
    }
    let lp = camera.light_view_proj_far * vec4<f32>(p, 1.0);
    let uv = vec2<f32>(lp.x * 0.5 + 0.5, 0.5 - lp.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || lp.z < 0.0 || lp.z > 1.0) {
        return 1.0;
    }
    return textureSampleCompareLevel(t_shadow_far, s_shadow, uv, lp.z - 0.0004);
}

// A hash of a cell of the plane (0..1 each), its integer coordinates taken modulo 4096 so
// that the world's large figures stay exact.
fn cell_hash(c: vec2<f32>) -> vec4<f32> {
    let w = c - 4096.0 * floor(c / 4096.0);
    let i = vec2<u32>(w);
    return hash4(i.x * 4099u + i.y * 7919u + 17u);
}

// The panels of a far veil of rain or snow (Enhanced): rings of upright panels round the
// camera, twelve a ring, each a strip of four vertices. Beyond the particles' layers the
// drops or flakes are too many and too small to draw one by one, and a heavy fall is seen
// there as what it is, a grey sheet streaked or speckled by them, sweeping by with the
// wind. `c`: the corner (0..1 each way); returns the corner's place (render frame) in xyz
// and its ring's radius in w.
fn veil_corner(panel: u32, c: vec2<f32>) -> vec4<f32> {
    var radii = array<f32, 3>(30.0, 48.0, 78.0);
    let ring = min(panel / 12u, 2u);
    let r = radii[ring];
    // (each panel a little wider than its twelfth, so that the chords close the ring; the
    // rings turned against each other, so their joins do not line up)
    let a = (f32(panel % 12u) + c.x * 1.04 - 0.02 + 0.37 * f32(ring)) * (6.2831853 / 12.0);
    let z = camera.cam_pos.z + mix(-6.0, 26.0 + 0.4 * r, c.y);
    return vec4<f32>(camera.cam_pos.xy + vec2<f32>(cos(a), sin(a)) * r, z, r);
}
