// Depth bias of the sun shadow, in metres along the sun's ray. The maps' depth runs over
// the light box's 2199 m, and the comparison used to take off 0.0015 (near) and 0.004 (far)
// of that: 3.3 m and 8.8 m. Nothing closer to the ground than that along the ray cast a
// shadow - not a car (1.4 m), not a bus (3 m; only its roof's shadow at a low sun,
// which lay detached and offset from the bus) - and the grass and kerbs had none either.
// The receiver plane below takes care of what the big constant stood in for.
const SHADOW_DEPTH_RANGE: f32 = 2199.0;
const SHADOW_BIAS_NEAR: f32 = 0.06;
const SHADOW_BIAS_FAR: f32 = 0.3;
const SHADOW_BIAS_CLOSE: f32 = 0.025;

// How the receiver's depth in the shadow map changes across the map (per unit of uv), from
// its normal: the PCF taps around the pixel compare against the surface's own depth at the
// tap instead of the centre's, so a surface at a slant to the sun does not shadow itself
// within the filter radius (acne), which a big constant bias used to hide. `lvp` is the
// cascade's light matrix (orthographic, so the map from the world is affine).
fn shadow_receiver_slope(lvp: mat4x4<f32>, n: vec3<f32>) -> vec2<f32> {
    let a = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), abs(n.x) > 0.9);
    let t1 = normalize(cross(n, a));
    let t2 = cross(n, t1);
    let p1 = (lvp * vec4<f32>(t1, 0.0)).xyz;
    let p2 = (lvp * vec4<f32>(t2, 0.0)).xyz;
    // (uv = 0.5 x + 0.5, 0.5 - 0.5 y)
    let d1 = vec3<f32>(p1.x * 0.5, -p1.y * 0.5, p1.z);
    let d2 = vec3<f32>(p2.x * 0.5, -p2.y * 0.5, p2.z);
    let det = d1.x * d2.y - d2.x * d1.y;
    if (abs(det) < 1e-12) {
        return vec2<f32>(0.0);
    }
    let g = vec2<f32>(d1.z * d2.y - d2.z * d1.y, d1.x * d2.z - d2.x * d1.z) / det;
    // a surface seen edge-on by the sun: keep the slope finite (it is in shadow of itself
    // there anyway, by its normal)
    return clamp(g, vec2<f32>(-2.0), vec2<f32>(2.0));
}

// The filter's half width in texels. A tent of 2 texels covers 4 x 4 texels, two gathers a
// side - four in all, fewer texture reads than the Poisson kernel's five (sixteen on an
// edge) - and spreads an edge about as the old kernel did in the near and close cascades
// (0.82 of a texel's standard deviation against 0.9 and 1.0; a fifth sharper in the far one).
const SHADOW_TENT: f32 = 2.0;
// Percentage-closer filtering with a tent of `SHADOW_TENT` texels, every texel of the map
// under it compared and weighted by the tent at its centre: four gathers of 2 x 2 texels,
// the whole 4 x 4 footprint read. The sparse Poisson kernel this replaces (16 bilinear taps
// 1.6 to 2.2 texels apart, and only its 5 outermost-and-middle taps when those agreed)
// sampled the map coarser than its texels: a lit gap between leaves, a few texels wide, came
// out as a copy of the kernel - an X of five magnified texels - and a feature lying between
// the taps vanished and reappeared as the receiver moved by a texel, which made the shadows
// shimmer. Read in full, the result is a continuous function of the receiver's position.
// `widen` (Enhanced: a veil of high cloud enlarging the sun) spreads the four gathers that
// many texels apart, a tent as much wider, without reading more.
// `uv`: the receiver in the cascade's own 0..1; `size`: the cascade's texels a side;
// `origin`: where its first texel lies in the texture; `slope`: its receiver plane (per uv).
// The shadow maps' size, texels a side, from the uniforms (`camera.shadow.y` is a texel): the
// near map an atlas of two cascades side by side, the far one taller by the lamps' tiles under
// it (`camera.lamp_atlas.z` is its texture's height). (Asked of the textures with
// textureDimensions in every inlined use of the sun's shadow, they brought Apple's M4 shader
// compiler down on every scene pipeline.)
fn shadow_map_size() -> f32 {
    return 1.0 / max(camera.shadow.y, 1e-6);
}

fn shadow_tent_place(uv: vec2<f32>, size: f32, widen: f32) -> vec4<f32> {
    let k = max(1.0, floor(widen + 0.5));
    // (texel i of the cascade has its centre at p = i; the first texel the tent reaches)
    let p = uv * size - vec2<f32>(0.5);
    let lo = floor(p - vec2<f32>(SHADOW_TENT * k)) + vec2<f32>(1.0);
    return vec4<f32>(p, lo);
}

// The tent's weights of a gather's four texels starting at `t0` (gather order: (0, 1),
// (1, 1), (1, 0), (0, 0) of the quad).
fn shadow_tent_weights(t0: vec2<f32>, p: vec2<f32>, r: f32) -> vec4<f32> {
    let w0 = max(vec2<f32>(0.0), vec2<f32>(1.0) - abs(t0 - p) / r);
    let w1 = max(vec2<f32>(0.0), vec2<f32>(1.0) - abs(t0 + vec2<f32>(1.0) - p) / r);
    return vec4<f32>(w0.x * w1.y, w1.x * w1.y, w1.x * w0.y, w0.x * w0.y);
}

fn shadow_tent_near(uv: vec2<f32>, size: f32, origin: vec2<f32>, z: f32, slope: vec2<f32>, widen: f32, bias: f32) -> f32 {
    let dims = vec2<f32>(2.0, 1.0) * shadow_map_size();
    let pl = shadow_tent_place(uv, size, widen);
    let k = max(1.0, floor(widen + 0.5));
    let r = SHADOW_TENT * k;
    var sum = 0.0;
    var wsum = 0.0;
    for (var q = 0; q < 4; q = q + 1) {
        let t0 = pl.zw + vec2<f32>(f32(q & 1), f32(q >> 1u)) * 2.0 * k;
        let a = (origin + t0 + vec2<f32>(1.0)) / dims;
        let zr = z + dot(slope, (t0 + vec2<f32>(0.5) - pl.xy) / size) - bias;
        let w = shadow_tent_weights(t0, pl.xy, r);
        sum = sum + dot(textureGatherCompare(t_shadow, s_shadow, a, zr), w);
        wsum = wsum + w.x + w.y + w.z + w.w;
    }
    return sum / max(wsum, 1e-6);
}

fn shadow_tent_far(uv: vec2<f32>, size: f32, z: f32, slope: vec2<f32>, widen: f32, bias: f32) -> f32 {
    let dims = vec2<f32>(shadow_map_size(), camera.lamp_atlas.z);
    let pl = shadow_tent_place(uv, size, widen);
    let k = max(1.0, floor(widen + 0.5));
    let r = SHADOW_TENT * k;
    var sum = 0.0;
    var wsum = 0.0;
    for (var q = 0; q < 4; q = q + 1) {
        let t0 = pl.zw + vec2<f32>(f32(q & 1), f32(q >> 1u)) * 2.0 * k;
        let a = (t0 + vec2<f32>(1.0)) / dims;
        let zr = z + dot(slope, (t0 + vec2<f32>(0.5) - pl.xy) / size) - bias;
        let w = shadow_tent_weights(t0, pl.xy, r);
        sum = sum + dot(textureGatherCompare(t_shadow_far, s_shadow, a, zr), w);
        wsum = wsum + w.x + w.y + w.z + w.w;
    }
    return sum / max(wsum, 1e-6);
}

// The near map is an atlas two cascades wide: the near cascade in its left half, the close
// one (`camera.light_view_proj_close`) from the top left of its right half, drawn no bigger
// than 2048 texels (`SHADOW_CLOSE_MAX` in lib.rs). `widen`: how much wider than the
// cascade's own the filter is (Enhanced: a veil of high cloud enlarging the sun).
fn shadow_pcf_near(uv: vec2<f32>, z: f32, slope: vec2<f32>, widen: f32) -> f32 {
    let size = shadow_map_size();
    return shadow_tent_near(uv, size, vec2<f32>(0.0), z, slope, widen, SHADOW_BIAS_NEAR / SHADOW_DEPTH_RANGE);
}

fn shadow_pcf_close(uv: vec2<f32>, z: f32, slope: vec2<f32>) -> f32 {
    // (camera.post.w: the close map's size over the near map's)
    let half = shadow_map_size();
    let size = half * select(1.0, camera.post.w, camera.post.w > 0.0);
    return shadow_tent_near(uv, size, vec2<f32>(half, 0.0), z, slope, 1.0, SHADOW_BIAS_CLOSE / SHADOW_DEPTH_RANGE);
}

fn shadow_push_close(n: vec3<f32>, ndl: f32) -> vec3<f32> {
    return n * (0.015 + 0.03 * (1.0 - ndl));
}

// The close cascade's value and weight at a point (weight 0 outside it): it fades into the
// near cascade between 60 and 90 % of its range from the camera.
fn shadow_close(world: vec3<f32>, n: vec3<f32>, ndl: f32, thin: bool) -> vec2<f32> {
    let range = camera.flags.w;
    if (range <= 0.0) {
        return vec2<f32>(1.0, 0.0);
    }
    let radial = distance(world, camera.cam_pos.xyz);
    let w = 1.0 - smoothstep(range * 0.6, range * 0.9, radial);
    if (w <= 0.001) {
        return vec2<f32>(1.0, 0.0);
    }
    let lp = camera.light_view_proj_close * vec4<f32>(world + shadow_push_close(n, ndl), 1.0);
    let uv = vec2<f32>(lp.x * 0.5 + 0.5, 0.5 - lp.y * 0.5);
    let edge = max(abs(uv.x - 0.5), abs(uv.y - 0.5));
    if (edge >= 0.48 || lp.z < 0.0 || lp.z > 1.0) {
        return vec2<f32>(1.0, 0.0);
    }
    let slope = select(shadow_receiver_slope(camera.light_view_proj_close, n), vec2<f32>(0.0), thin);
    return vec2<f32>(shadow_pcf_close(uv, lp.z, slope), w);
}

// The far map takes the top of its texture (its texels are the texture's first rows); the
// lamps' tiles lie under it (`LampAtlas` in lights.rs).
fn shadow_pcf_far(uv: vec2<f32>, z: f32, slope: vec2<f32>, widen: f32) -> f32 {
    let size = shadow_map_size();
    return shadow_tent_far(uv, size, z, slope, widen, SHADOW_BIAS_FAR / SHADOW_DEPTH_RANGE);
}

// The shadow lookup point: pushed off the surface along its normal by about a texel of the
// cascade (0.14 m near, 0.68 m far), more where the surface turns away from the sun.
fn shadow_push_near(n: vec3<f32>, ndl: f32) -> vec3<f32> {
    return n * (0.06 + 0.12 * (1.0 - ndl));
}

fn shadow_push_far(n: vec3<f32>, ndl: f32) -> vec3<f32> {
    return n * (0.3 + 0.6 * (1.0 - ndl));
}

// `thin`: an alpha-tested card (leaves, fences), whose normal says nothing about its plane
// (OMSI points a tree's normals up): no receiver plane, the lookup moves half a metre
// towards the sun instead, which keeps a crown from speckling itself.
fn sun_shadow(world_in: vec3<f32>, n: vec3<f32>, thin: bool) -> f32 {
    if (camera.shadow.x < 0.5) {
        return 1.0;
    }
    let world = world_in + select(vec3<f32>(0.0), camera.sun_dir.xyz * 0.5, thin);
    let ndl = clamp(dot(n, camera.sun_dir.xyz), 0.0, 1.0);
    let close = shadow_close(world, n, ndl, thin);
    if (close.y >= 0.999) {
        return close.x;
    }
    let near_lp = camera.light_view_proj * vec4<f32>(world + shadow_push_near(n, ndl), 1.0);
    let near_uv = vec2<f32>(near_lp.x * 0.5 + 0.5, 0.5 - near_lp.y * 0.5);
    let inside = max(abs(near_uv.x - 0.5), abs(near_uv.y - 0.5));
    if (inside < 0.48 && near_lp.z >= 0.0 && near_lp.z <= 1.0) {
        let nv = shadow_pcf_near(near_uv, near_lp.z, select(shadow_receiver_slope(camera.light_view_proj, n), vec2<f32>(0.0), thin), 1.0);
        return mix(nv, close.x, close.y);
    }
    let lp = camera.light_view_proj_far * vec4<f32>(world + shadow_push_far(n, ndl), 1.0);
    let uv = vec2<f32>(lp.x * 0.5 + 0.5, 0.5 - lp.y * 0.5);
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || lp.z < 0.0 || lp.z > 1.0) {
        return 1.0;
    }
    let sum = shadow_pcf_far(uv, lp.z, select(shadow_receiver_slope(camera.light_view_proj_far, n), vec2<f32>(0.0), thin), 1.0);
    let edge = clamp((0.5 - max(abs(uv.x - 0.5), abs(uv.y - 0.5))) * 12.0, 0.0, 1.0);
    return mix(1.0, sum, edge);
}

