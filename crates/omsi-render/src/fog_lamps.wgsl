// Enhanced graphics: the light the air scatters towards the camera that the main pass
// leaves out - the lamps' in the weather's fog, rain or falling snow (`lamp_airlight` in
// lamp_air.wgsl) and the sun's shafts between the shadows (`sun_shafts`) - added onto the
// drawn picture before its glow and its metering. Once per pixel over the depth prepass: worked out in
// the main pass it was paid again for every layer drawn over a pixel (painted ground,
// decals), several milliseconds a frame in a foggy night.
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

struct FogLampParams {
    // clip space (reversed Z) back to the render-origin relative world
    inv_view_proj: mat4x4<f32>,
    // xy the picture's size in pixels, z 1: the sun's glare is added (2: it is all that is
    // added, the half-size result holds only the sun's row), w the share of the weather's
    // extinction that is mist and fog (not rain)
    size: vec4<f32>,
};
@group(1) @binding(0) var<uniform> fp: FogLampParams;
@group(1) @binding(1) var t_depth: texture_depth_2d;
// the half-size result (rgb pre-exposed light, a the distance it was worked out to)
@group(1) @binding(2) var t_fog: texture_2d<f32>;

struct FogVsOut {
    @builtin(position) clip: vec4<f32>,
};

@vertex
fn vs_fog_lamps(@builtin(vertex_index) i: u32) -> FogVsOut {
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    var out: FogVsOut;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    return out;
}

// The camera, the direction through a full-size pixel position and the distance to what
// the prepass holds there (3 km for the sky).
fn pixel_ray(at: vec2<f32>) -> vec4<f32> {
    let dims = vec2<i32>(textureDimensions(t_depth));
    let px = clamp(vec2<i32>(at), vec2<i32>(0), dims - vec2<i32>(1));
    let d = textureLoad(t_depth, px, 0);
    let ndc = vec2<f32>(at.x / fp.size.x * 2.0 - 1.0, 1.0 - at.y / fp.size.y * 2.0);
    let c = camera.cam_pos.xyz;
    let mid = fp.inv_view_proj * vec4<f32>(ndc, 0.5, 1.0);
    let dir = normalize(mid.xyz / mid.w - c);
    var len = 3000.0;
    if (d > 0.0) {
        let h = fp.inv_view_proj * vec4<f32>(ndc, d, 1.0);
        len = distance(h.xyz / h.w, c);
    }
    return vec4<f32>(dir, len);
}

// The sun's shadow maps (shader.wgsl's bindings and constants).
@group(0) @binding(5) var t_shadow: texture_depth_2d;
@group(0) @binding(6) var s_shadow: sampler_comparison;
@group(0) @binding(7) var t_shadow_far: texture_depth_2d;
const SHAFT_DEPTH_RANGE: f32 = 2199.0;
const SHAFT_STEPS: u32 = 16u;
// how far the shafts are followed in the dense steps (the near cascade's reach, a little less)
const SHAFT_NEAR: f32 = 180.0;
// and, past it, in a few sparse ones to the far map's reach (the beams of a misty avenue, or
// a low sun between the blocks, reach farther than the first hundred metres)
const SHAFT_FAR_STEPS: u32 = 4u;
const SHAFT_REACH: f32 = 420.0;
// how far the sky's gaps are followed (the haze beyond that is the table's, lit as a whole) and in
// how many steps growing by a fixed ratio
const SKY_SHAFT_REACH: f32 = 7000.0;
const SKY_SHAFT_STEPS: u32 = 8u;

// Whether the sun reaches a point in the air (1) or a house or a crown stands between (0).
fn sun_reaches(x: vec3<f32>) -> f32 {
    let lp = camera.light_view_proj * vec4<f32>(x, 1.0);
    let uv = vec2<f32>(lp.x * 0.5 + 0.5, 0.5 - lp.y * 0.5);
    if (max(abs(uv.x - 0.5), abs(uv.y - 0.5)) < 0.48 && lp.z >= 0.0 && lp.z <= 1.0) {
        return textureSampleCompareLevel(t_shadow, s_shadow, vec2<f32>(uv.x * 0.5, uv.y), lp.z - 0.15 / SHAFT_DEPTH_RANGE);
    }
    let fp2 = camera.light_view_proj_far * vec4<f32>(x, 1.0);
    let fuv = vec2<f32>(fp2.x * 0.5 + 0.5, 0.5 - fp2.y * 0.5);
    if (fuv.x < 0.0 || fuv.x > 1.0 || fuv.y < 0.0 || fuv.y > 1.0 || fp2.z < 0.0 || fp2.z > 1.0) {
        return 1.0;
    }
    // (the far map takes the top of its texture, see shader.wgsl `shadow_pcf_far`)
    return textureSampleCompareLevel(t_shadow_far, s_shadow, vec2<f32>(fuv.x, fuv.y * (1.0 / (camera.shadow.y * camera.lamp_atlas.z))), fp2.z - 0.6 / SHAFT_DEPTH_RANGE);
}

// Shafts of sunlight - crepuscular rays: the air between the camera and what it sees
// scatters the sun towards the eye wherever the sun reaches it, and not in the shadow of a
// house or a tree. The picture's air (`air` in enhanced_common.wgsl) holds that light as
// if the sun reached the air everywhere; along the first stretch of the ray, which the
// shadow maps cover, what lies in shadow is taken out again here (a negative radiance):
// lit and dark shafts through a hazy evening, a misty morning wood, a dusty street. The
// air scatters the way `air` has it - the weather's fog with g 0.55, the clear air's haze
// and dust forward with g 0.7 (its Rayleigh part is small near the ground).
fn sun_shafts(c: vec3<f32>, d: vec3<f32>, len: f32, jitter: f32) -> vec3<f32> {
    let s = camera.sun_dir.xyz;
    let e_sun = enh.sun.rgb;
    // (the clouds' shadow field: the shafts through a broken sky, followed to kilometres)
    let sky_gaps = enh.cloud_f1.w > 0.5 && s.z >= 0.06;
    let shadows = camera.shadow.x > 0.5;
    if ((!shadows && !sky_gaps) || s.z <= 0.0 || max(e_sun.r, e_sun.g) < 1e-4) {
        return vec3<f32>(0.0);
    }
    let end = min(len, SHAFT_NEAR);
    let dt = end / f32(SHAFT_STEPS);
    let far_len = max(min(len, SHAFT_REACH) - SHAFT_NEAR, 0.0);
    let dt_far = far_len / f32(SHAFT_FAR_STEPS);
    let cos_t = dot(s, d);
    let p_fog = hg_phase(cos_t, 0.55) * 0.9;
    let p_air = hg_phase(cos_t, 0.7);
    let h0 = c.z - enh.fog.z;
    // the clouds' part reaches on, out to the haze's own horizon: from the sparse steps'
    // end, in steps growing by a fixed ratio
    let sky_end = select(SHAFT_REACH, min(len, SKY_SHAFT_REACH), sky_gaps);
    // (the most the shadows could take out along the way, on the screen: where that is
    // too little to see - most of the picture, looking away from the sun - nothing is done)
    let most = (enh.fog.x * p_fog + enh.fog.w * p_air) * max(e_sun.r, e_sun.g) * max(end, select(0.0, sky_end, sky_gaps)) * enh.exposure.x;
    if (most < 0.004) {
        return vec3<f32>(0.0);
    }
    var shade = vec3<f32>(0.0);
    for (var i = 0u; i < SHAFT_STEPS + SHAFT_FAR_STEPS; i = i + 1u) {
        let far = i >= SHAFT_STEPS;
        if (far && far_len <= 0.0) {
            break;
        }
        let t = select((f32(i) + jitter) * dt, SHAFT_NEAR + (f32(i - SHAFT_STEPS) + jitter) * dt_far, far);
        let w = select(dt, dt_far, far);
        let x = c + d * t;
        var lit = 1.0;
        if (shadows) {
            lit = sun_reaches(x);
        }
        if (sky_gaps) {
            lit = lit * cloud_shadow_ratio(x);
        }
        // (a point the cloud lets more sun through than the camera's own place gets adds light)
        if (lit >= 0.999 && lit <= 1.001) {
            continue;
        }
        let h = max(x.z - enh.fog.z, 0.0);
        let s_fog = enh.fog.x * exp(-enh.fog.y * h);
        let s_air = enh.fog.w * exp(-h / 1500.0);
        let tau = layer_depth(enh.fog.x, enh.fog.y, h0, h, t) + layer_depth(enh.fog.w, 1.0 / 1500.0, h0, h, t);
        shade = shade + vec3<f32>((1.0 - lit) * (s_fog * p_fog + s_air * p_air) * exp(-tau) * w);
    }
    // the part beyond the shadow maps: only the clouds' shadow is known there
    if (sky_gaps && sky_end > SHAFT_REACH) {
        let r = pow(sky_end / SHAFT_REACH, 1.0 / f32(SKY_SHAFT_STEPS));
        var t0 = SHAFT_REACH;
        for (var i = 0u; i < SKY_SHAFT_STEPS; i = i + 1u) {
            let t1 = t0 * r;
            let t = t0 + (t1 - t0) * jitter;
            let x = c + d * t;
            let lit = cloud_shadow_ratio(x);
            if (lit < 0.999 || lit > 1.001) {
                let h = max(x.z - enh.fog.z, 0.0);
                let s_fog = enh.fog.x * exp(-enh.fog.y * h);
                let s_air = enh.fog.w * exp(-h / 1500.0);
                let tau = layer_depth(enh.fog.x, enh.fog.y, h0, h, t) + layer_depth(enh.fog.w, 1.0 / 1500.0, h0, h, t);
                shade = shade + vec3<f32>((1.0 - lit) * (s_fog * p_fog + s_air * p_air) * exp(-tau) * (t1 - t0));
            }
            t0 = t1;
        }
    }
    return -shade * e_sun;
}

// at half size: one ray per four pixels
@fragment
fn fs_fog_lamps(in: FogVsOut) -> @location(0) vec4<f32> {
    // (the texture's last row: how much of the sun is seen, worked out once for the frame
    // in its first texel)
    // (t_fog is not this pass's target: the row is the one under the half picture)
    if (i32(in.clip.y) >= i32(ceil(fp.size.y * 0.5))) {
        var seen = 0.0;
        if (i32(in.clip.x) == 0 && fp.size.z > 0.5 && camera.sun_dir.z > -0.02) {
            seen = sun_seen(camera.cam_pos.xyz, camera.sun_dir.xyz);
        }
        return vec4<f32>(seen, 0.0, 0.0, 0.0);
    }
    let r = pixel_ray(floor(in.clip.xy) * 2.0 + vec2<f32>(0.5));
    let q = vec2<i32>(in.clip.xy) & vec2<i32>(1);
    let ign = (f32(q.x * 2 + (q.x ^ q.y)) + 0.5) * 0.25;
    // (the lamps' halo and cones only from the mist's and fog's droplets: fp.size.w)
    let l = (lamp_airlight(camera.cam_pos.xyz, r.xyz, 0.0, r.w, ign, fp.size.w) + sun_shafts(camera.cam_pos.xyz, r.xyz, r.w, ign)) * enh.exposure.x;
    return vec4<f32>(clamp(l, vec3<f32>(-4000.0), vec3<f32>(4000.0)), r.w);
}

// How much of the sun's disc is seen (0..1): houses, trees and the bus in front of it
// are in the depth prepass (the clouds and the fog are in `enh.sun` already).
fn sun_seen(c: vec3<f32>, s: vec3<f32>) -> f32 {
    let r = enh.sun.w;
    let up0 = select(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(1.0, 0.0, 0.0), abs(s.z) > 0.9);
    let a = normalize(cross(s, up0));
    let b = cross(s, a);
    var seen = 0.0;
    var n = 0.0;
    for (var k = 0; k < 13; k = k + 1) {
        var o = vec2<f32>(0.0);
        if (k > 0) {
            let ang = f32(k) * 2.3999632;
            o = vec2<f32>(cos(ang), sin(ang)) * sqrt(f32(k) / 12.0) * 0.9;
        }
        let dir = normalize(s + (a * o.x + b * o.y) * r);
        let clip = camera.view_proj * vec4<f32>(c + dir * 1000.0, 1.0);
        let ndc = clip.xy / max(clip.w, 1e-6);
        n = n + 1.0;
        if (clip.w <= 0.0 || abs(ndc.x) > 1.0 || abs(ndc.y) > 1.0) {
            // Beyond the picture's edge the prepass knows nothing: whether the eye stands in
            // the shadow of anything the sun's shadow maps hold (without shadows: seen).
            // What stood at the edge was taken before - a house or a pole reaching the edge
            // as the camera turned put out the whole veil at once, and sky put it back, a
            // sheet of light over the picture switched off and on.
            if (camera.shadow.x > 0.5) {
                seen = seen + sun_reaches(c + dir * 0.3);
            } else {
                seen = seen + 1.0;
            }
            continue;
        }
        let px = vec2<f32>((ndc.x * 0.5 + 0.5) * fp.size.x, (0.5 - ndc.y * 0.5) * fp.size.y);
        let dims = vec2<i32>(textureDimensions(t_depth));
        let q = clamp(vec2<i32>(px), vec2<i32>(0), dims - vec2<i32>(1));
        seen = seen + select(0.0, 1.0, textureLoad(t_depth, q, 0) <= 0.0);
    }
    return seen / n;
}

// Glare: the light a lens scatters round a bright source - the sun seen directly washes out
// what is right beside it. The picture is a camera's, and the player compares it with
// photographs: a good lens' stray light falls off as the near term of the CIE's glare
// function, 10/theta^3 per unit of the source's illuminance (theta in degrees, CIE
// 146:2002), a glow of a few degrees. The eye's own share - the wide veil of 5/theta^2 and
// more that the vitreous and the eye's wall add, its ciliary streaks and its lenticular
// halo - was drawn as well and made a white blot ten degrees round the sun of a clear day,
// "the sun is too large" (#2400): a photograph shows none of it.
fn sun_glare(dir: vec3<f32>) -> vec3<f32> {
    let s = camera.sun_dir.xyz;
    if (max(enh.sun.r, enh.sun.g) < 1e-5 || s.z < -0.02) {
        return vec3<f32>(0.0);
    }
    let cos_t = clamp(dot(dir, s), -1.0, 1.0);
    let theta = max(degrees(acos(cos_t)), 0.25);
    if (theta > 100.0) {
        return vec3<f32>(0.0);
    }
    let seen = textureLoad(t_fog, vec2<i32>(0, i32(textureDimensions(t_fog).y) - 1), 0).r;
    if (seen <= 0.0) {
        return vec3<f32>(0.0);
    }
    return enh.sun.rgb * seen * (10.0 / (theta * theta * theta));
}

// at full size, added onto the picture: the four nearest half-size rays, each as far as
// it was worked out to a distance like this pixel's (the light in front of a near pole
// is not the light in front of the houses behind it)
@fragment
fn fs_fog_composite(in: FogVsOut) -> @location(0) vec4<f32> {
    let ray = pixel_ray(in.clip.xy);
    var air_light = vec3<f32>(0.0);
    if (fp.size.z < 1.5) {
        air_light = gathered_air_light(in.clip.xy, ray.w);
    }
    // the eye's glare from the sun, on top of everything (fp.size.z: on)
    var glare = vec3<f32>(0.0);
    if (fp.size.z > 0.5) {
        glare = sun_glare(ray.xyz) * enh.exposure.x;
    }
    return vec4<f32>(air_light + min(glare, vec3<f32>(4000.0)), 0.0);
}

fn gathered_air_light(at: vec2<f32>, len: f32) -> vec3<f32> {
    let hp = at * 0.5 - vec2<f32>(0.5);
    let base = vec2<i32>(floor(hp));
    let f = hp - floor(hp);
    // (the last row is the sun's, see `fs_fog_lamps`)
    let hd = vec2<i32>(textureDimensions(t_fog)) - vec2<i32>(1, 2);
    var sum = vec3<f32>(0.0);
    var wsum = 0.0;
    // (a twig or a wire none of the four rays met: the one nearest in distance)
    var best = vec3<f32>(0.0);
    var best_d = 1e9;
    // (a tent over the 4 x 4 nearest: the edge of a headlight's beam in the haze, worked
    // out at half size, showed its 2 x 2 blocks under a plain bilinear one)
    for (var k = 0; k < 16; k = k + 1) {
        let o = vec2<i32>(k & 3, k >> 2) - vec2<i32>(1);
        let s = textureLoad(t_fog, clamp(base + o, vec2<i32>(0), hd), 0);
        let dx = abs(f32(o.x) - f.x);
        let dy = abs(f32(o.y) - f.y);
        let bil = max(1.0 - dx * 0.5, 0.0) * max(1.0 - dy * 0.5, 0.0);
        let dd = abs(s.a - len);
        // (a Gaussian of the distance difference, not an exponential: a road's rows differ by
        // a few per cent of its length from one half-size row to the next, and an exponential
        // weighted every row differently - the offsets of the staggered steps, which cancel
        // over the tent exactly when its weights are even, then came out as a stripe along
        // every other row of a bright beam. A step in depth, a pole against what is behind it,
        // still keeps the taps of the far side out.)
        let tol = (dd / (0.05 * len + 0.5)) * 1.2;
        let w = (bil + 1e-3) * exp(-tol * tol);
        sum = sum + s.rgb * w;
        wsum = wsum + w;
        if (dd < best_d) {
            best_d = dd;
            best = s.rgb * min(len / max(s.a, 0.1), 1.0);
        }
    }
    if (wsum < 1e-4) {
        return best;
    }
    return sum / max(wsum, 1e-6);
}

