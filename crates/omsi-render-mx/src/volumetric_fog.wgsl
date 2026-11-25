// Enhanced graphics: the volumetric fog (the settings' `volumetric_fog`). The main pass lays
// an analytic air over every surface (`air` in enhanced_common.wgsl): a fog as even as a
// sheet, lit as if the sun reached all of it. Here the air between the camera and what the
// depth prepass holds is marched through instead, at half size: the weather's fog thicker
// and thinner in banks that drift with the wind, the sun's light only where the shadow maps
// and the clouds' shadow field let it in - shafts through the crowns and between the houses,
// beams under a broken sky - scattered strongly forward round the sun, as fog droplets do,
// and the sky's light from all round (`enh.fog_color`, from the sky's harmonics). Each frame
// starts its steps elsewhere and is blended into the frames before, followed to where they
// lie now (a jittered march and its history: no banding, little noise).
//
// Over the full picture the air the main pass laid on is then exchanged for the marched one,
// along the stretch that was marched: what the picture holds behind that air (its light less
// the analytic air's, over what that let through) times what the marched air lets through,
// plus the light the marched air scatters. Nothing is fogged twice, a pane or a blended layer
// in front keeps the fog the main pass gave it, and past the march's reach (and with the
// option off) the analytic fog stands as it always did. The sun's shafts that fog_lamps.wgsl
// cut into the analytic air (`sun_shafts`) are this march's then; the lamps' light in the air
// stays there (`lamp_airlight`, added over this).
//
// The module is fog_lamps.wgsl's (its camera, shadow look-ups and enhanced_common.wgsl) with
// this added; its own bindings are group 2.

struct VolFogParams {
    // clip space (reversed Z) back to the render-origin relative world
    inv_view_proj: mat4x4<f32>,
    // the last frame's view and projection, taken to this frame's render origin
    prev_view_proj: mat4x4<f32>,
    // xy the picture's size in pixels, zw the half-size result's
    size: vec4<f32>,
    // x the steps of a ray, y where in its step this frame's samples lie (0..1), z 1 when the
    // last frame's result can be followed, w the exposure now over the last frame's
    march: vec4<f32>,
    // xyz the last frame's camera (this frame's render origin), w which pixel of each 2 x 2
    // block is marched this frame (0..3)
    prev_cam: vec4<f32>,
    // xyz the small noise's offset (m: the world origin and the drift, within its period)
    detail: vec4<f32>,
};
@group(2) @binding(0) var<uniform> vf: VolFogParams;
@group(2) @binding(1) var vf_depth: texture_depth_2d;
// the half-size result: rgb the light the marched air scatters towards the camera
// (pre-exposed), a what it lets through; the march reads the last frame's, the composite
// this frame's
@group(2) @binding(2) var vf_air: texture_2d<f32>;
// the distance it was marched to (negative: the sky's)
@group(2) @binding(3) var vf_dist: texture_2d<f32>;
// the fog's density noise (vol_fog.rs `density_noise`), tiling
@group(2) @binding(4) var vf_noise: texture_3d<f32>;
@group(2) @binding(5) var vf_repeat: sampler;

// The span of the noise texture in the world (m): the banks across and up, the small eddies
// across and up. The offsets the Rust side passes are taken within these periods.
const VF_BANKS: vec3<f32> = vec3<f32>(260.0, 260.0, 70.0);
const VF_EDDIES: vec3<f32> = vec3<f32>(52.0, 52.0, 26.0);
// how fast the ground haze thins upwards (m, its scale height)
const HAZE_HEIGHT: f32 = 40.0;

struct VfVsOut {
    @builtin(position) clip: vec4<f32>,
};

@vertex
fn vs_vol_fog(@builtin(vertex_index) i: u32) -> VfVsOut {
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    var out: VfVsOut;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    return out;
}

// The direction through a full-size pixel position and the distance to what the prepass
// holds there (-1: the sky).
fn vf_pixel_ray(at: vec2<f32>) -> vec4<f32> {
    let dims = vec2<i32>(textureDimensions(vf_depth));
    let px = clamp(vec2<i32>(at), vec2<i32>(0), dims - vec2<i32>(1));
    let z = textureLoad(vf_depth, px, 0);
    let ndc = vec2<f32>(at.x / vf.size.x * 2.0 - 1.0, 1.0 - at.y / vf.size.y * 2.0);
    let c = camera.cam_pos.xyz;
    let mid = vf.inv_view_proj * vec4<f32>(ndc, 0.5, 1.0);
    let dir = normalize(mid.xyz / mid.w - c);
    var len = -1.0;
    if (z > 0.0) {
        let h = vf.inv_view_proj * vec4<f32>(ndc, z, 1.0);
        len = distance(h.xyz / h.w, c);
    }
    return vec4<f32>(dir, len);
}

// A point in the player's vehicle box frame (the scene shader's `fn_box_local`).
fn vf_box_local(world: vec3<f32>) -> vec3<f32> {
    let d = world - camera.inside_a.xyz;
    let sh = camera.inside_a.w;
    let ch = camera.inside_b.x;
    return vec3<f32>(d.x * ch - d.y * sh - camera.inside_c.x, d.x * sh + d.y * ch - camera.inside_c.y, d.z - camera.inside_c.z);
}

// Where the ray's first `len` metres run through the player's vehicle (from, to; equal: not
// at all): there is no fog in the cab, and the main pass leaves that stretch out of its air
// (the scene shader's `fog_distance`, the same box and the same test).
fn vf_cab(c: vec3<f32>, d: vec3<f32>, len: f32) -> vec2<f32> {
    if (camera.inside_c.w < 0.5 || len <= 0.0) {
        return vec2<f32>(0.0);
    }
    let o = vf_box_local(c);
    let e = vf_box_local(c + d * len) - o;
    let h = camera.inside_b.yzw;
    var t0 = 0.0;
    var t1 = 1.0;
    for (var k = 0; k < 3; k = k + 1) {
        if (abs(e[k]) < 1e-6) {
            if (abs(o[k]) > h[k]) {
                return vec2<f32>(0.0);
            }
        } else {
            let a = (-h[k] - o[k]) / e[k];
            let b = (h[k] - o[k]) / e[k];
            t0 = max(t0, min(a, b));
            t1 = min(t1, max(a, b));
        }
    }
    if (t1 <= t0) {
        return vec2<f32>(0.0);
    }
    return vec2<f32>(t0, t1) * len;
}

// How much thicker or thinner than the weather's mean the fog is at x: banks a couple of
// hundred metres across, flatter than wide, with smaller eddies in them, both drifting with
// the wind (`enh.vol_fog2`, `vf.detail`). Its mean is 1: past the march the analytic fog
// goes on with the same amount.
fn vf_density(x: vec3<f32>) -> f32 {
    let amp = enh.vol_fog.y;
    if (amp <= 0.0) {
        return 1.0;
    }
    let banks = textureSampleLevel(vf_noise, vf_repeat, (x + enh.vol_fog2.xyz) / VF_BANKS, 0.0).r;
    let eddies = textureSampleLevel(vf_noise, vf_repeat, (x + vf.detail.xyz) / VF_EDDIES + vec3<f32>(0.37, 0.11, 0.73), 0.0).r;
    let n = banks * 0.72 + eddies * 0.28;
    return max(1.0 + amp * (n - 0.5) * 3.2, 0.0);
}

// The fog droplets' phase function: most of their light goes on within a few tens of degrees
// of the way it came (a bright glow round the sun in the fog, a dim fog with the sun behind),
// a little is sent back (two Henyey-Greenstein lobes).
fn vf_phase(c: f32) -> f32 {
    return mix(hg_phase(c, 0.7), hg_phase(c, -0.2), 0.15);
}

// The air along d from c over [0, reach] (the cab's stretch `cab` left out): rgb the light
// it scatters towards the camera (radiance), a what it lets through. `clear` 0 for the sky,
// whose table holds the clear air already (its shadowed part is taken out all the same, as
// `sun_shafts` did).
fn vf_march(c: vec3<f32>, d: vec3<f32>, reach: f32, clear: f32, cab: vec2<f32>, jitter: f32) -> vec4<f32> {
    // the light that casts the shadow maps: the sun, or by night the moon (enh.moon_light.w)
    let moon = enh.moon_light.w > 0.5;
    let l_dir = select(camera.sun_dir.xyz, enh.moon.xyz, moon);
    let e = select(enh.sun.rgb, enh.moon_light.rgb, moon);
    let lit = max(e.r, max(e.g, e.b)) > 1e-7 && l_dir.z > -0.02;
    let shadows = lit && camera.shadow.x > 0.5;
    let gaps = lit && !moon && enh.cloud_f1.w > 0.5 && l_dir.z >= 0.06;
    let cos_t = dot(d, l_dir);
    let p_fog = vf_phase(cos_t) * 0.9;
    let p_air = hg_phase(cos_t, 0.7);
    // (the clear air takes the sky's colour at the horizon in this direction, as `air_of`)
    let horizon = sky_table(normalize(vec3<f32>(d.x, d.y, max(d.z, 0.0) * 0.3 + 0.03)));
    let ambient = enh.fog_color.rgb;
    let steps = max(u32(vf.march.x), 1u);
    var through = 1.0;
    var light = vec3<f32>(0.0);
    var t0 = 0.0;
    for (var i = 0u; i < steps; i = i + 1u) {
        // steps growing with the distance (squared): the near air, where the shafts and the
        // banks are seen largest, sampled densest
        let u = f32(i + 1u) / f32(steps);
        let t1 = reach * u * u;
        let t = mix(t0, t1, jitter);
        let w = (t1 - t0) - max(min(t1, cab.y) - max(t0, cab.x), 0.0);
        t0 = t1;
        if (w <= 0.0) {
            continue;
        }
        let x = c + d * t;
        let h = max(x.z - enh.fog.z, 0.0);
        let dn = vf_density(x);
        // the weather's fog and the low haze over the ground (`vol_fog.w`, vol_fog.rs
        // `ground_haze`), both droplets: thicker and thinner in the same banks
        let s_fog = enh.fog.x * exp(-enh.fog.y * h) * dn + enh.vol_fog.w * exp(-h / HAZE_HEIGHT) * mix(1.0, dn, 0.7);
        let s_air = enh.fog.w * exp(-h / 1500.0);
        let s_ext = s_fog + s_air * clear;
        var vis = 1.0;
        if (shadows) {
            vis = sun_reaches(x);
        }
        if (gaps) {
            vis = vis * cloud_shadow_ratio(x);
        }
        // what a metre of this air sends towards the camera: the fog the sky's light and
        // the sun where it reaches; the clear air its horizon's colour, less the sun it
        // holds there where the sun does not reach
        let src = s_fog * (ambient + e * (p_fog * vis)) + s_air * (horizon * clear - e * (p_air * (1.0 - vis)));
        // (the step's light integrated over its own extinction: energy kept however long the
        // step - Hillaire, "Physically based and unified volumetric rendering", 2015)
        let step_t = exp(-s_ext * w);
        let along = select(w, (1.0 - step_t) / max(s_ext, 1e-12), s_ext * w > 1e-5);
        light = light + src * (through * along);
        through = through * step_t;
        if (through < 0.002) {
            break;
        }
    }
    return vec4<f32>(light, through);
}

// The analytic air the main pass laid over [0, reach] of this ray (`air` for a surface, the
// sky's own `air_of` without the clear air for the sky), and the stretch's length outside
// the cab.
fn vf_analytic(c: vec3<f32>, d: vec3<f32>, reach: f32, sky: bool, cab: vec2<f32>) -> vec4<f32> {
    let h0 = c.z - enh.fog.z;
    if (sky) {
        return air_of(d, reach, h0, h0 + reach * max(d.z, 0.0), 0.0);
    }
    let p = c + d * reach;
    let outside = max(reach - max(min(reach, cab.y) - cab.x, 0.0), 0.0);
    return air(d, outside, h0, p.z - enh.fog.z);
}

struct VfMarchOut {
    @location(0) air: vec4<f32>,
    @location(1) dist: f32,
};

// at half size: one ray per 2 x 2 block, through another of its pixels each frame
@fragment
fn fs_vol_march(in: VfVsOut) -> VfMarchOut {
    let k = u32(vf.prev_cam.w);
    let at = floor(in.clip.xy) * 2.0 + vec2<f32>(f32(k & 1u), f32(k >> 1u)) + vec2<f32>(0.5);
    let r = vf_pixel_ray(at);
    let c = camera.cam_pos.xyz;
    let sky = r.w < 0.0;
    let full = enh.vol_fog.z;
    let reach = select(min(r.w, full), full, sky);
    let cab = select(vf_cab(c, r.xyz, reach), vec2<f32>(0.0), sky);
    let ign = fract(52.9829189 * fract(dot(in.clip.xy, vec2<f32>(0.06711056, 0.00583715))));
    let jitter = fract(ign + vf.march.y);
    var cur = vf_march(c, r.xyz, reach, select(1.0, 0.0, sky), cab, jitter);
    cur = vec4<f32>(min(cur.rgb * enh.exposure.x, vec3<f32>(60000.0)), cur.a);
    var out: VfMarchOut;
    out.dist = select(reach, -reach, sky);
    out.air = cur;
    if (vf.march.z < 0.5) {
        return out;
    }
    // the last frames' result where this point lay then: the four half-size texels round it,
    // those that were marched to the same distance (a pole that moved, a bus driving past:
    // what was behind is not this)
    let p = c + r.xyz * reach;
    let prev = vf.prev_view_proj * vec4<f32>(p, 1.0);
    if (prev.w <= 1e-4) {
        return out;
    }
    let uv = vec2<f32>(prev.x / prev.w * 0.5 + 0.5, 0.5 - prev.y / prev.w * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return out;
    }
    let expect = select(min(distance(p, vf.prev_cam.xyz), full), -full, sky);
    let tol = 0.06 * abs(expect) + 0.6;
    let hp = uv * vf.size.zw - vec2<f32>(0.5);
    let base = vec2<i32>(floor(hp));
    let f = hp - floor(hp);
    let hd = vec2<i32>(vf.size.zw) - vec2<i32>(1);
    var sum = vec4<f32>(0.0);
    var wsum = 0.0;
    for (var j = 0; j < 4; j = j + 1) {
        let o = vec2<i32>(j & 1, j >> 1);
        let q = clamp(base + o, vec2<i32>(0), hd);
        let dd = abs(textureLoad(vf_dist, q, 0).r - expect);
        if (dd > tol) {
            continue;
        }
        let w = select(1.0 - f.x, f.x, o.x == 1) * select(1.0 - f.y, f.y, o.y == 1);
        sum = sum + textureLoad(vf_air, q, 0) * w;
        wsum = wsum + w;
    }
    if (wsum < 0.25) {
        return out;
    }
    let hist = sum / wsum;
    // (the exposure moved since: the history is taken to this frame's)
    let held = vec4<f32>(hist.rgb * vf.march.w, hist.a);
    out.air = mix(held, cur, 0.12);
    return out;
}

// The marched air at a full-size pixel: the four nearest half-size results, each as far as
// it was marched to a distance like this pixel's (the fog in front of a near pole is not the
// fog in front of the houses behind it, nor the sky's).
fn vf_gather(at: vec2<f32>, dist: f32) -> vec4<f32> {
    let hp = at * 0.5 - vec2<f32>(0.5);
    let base = vec2<i32>(floor(hp));
    let f = hp - floor(hp);
    let hd = vec2<i32>(vf.size.zw) - vec2<i32>(1);
    var sum = vec4<f32>(0.0);
    var wsum = 0.0;
    var best = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    var best_d = 1e9;
    for (var j = 0; j < 4; j = j + 1) {
        let o = vec2<i32>(j & 1, j >> 1);
        let q = clamp(base + o, vec2<i32>(0), hd);
        let s = textureLoad(vf_air, q, 0);
        let dd = abs(textureLoad(vf_dist, q, 0).r - dist);
        let bil = select(1.0 - f.x, f.x, o.x == 1) * select(1.0 - f.y, f.y, o.y == 1);
        let tol = dd / (0.05 * abs(dist) + 0.5);
        let w = (bil + 1e-3) * exp(-tol * tol);
        sum = sum + s * w;
        wsum = wsum + w;
        if (dd < best_d) {
            best_d = dd;
            best = s;
        }
    }
    if (wsum < 1e-3) {
        return best;
    }
    return sum / wsum;
}

// at full size, over the picture (blended: the source plus the picture times the source's
// alpha): the analytic air exchanged for the marched one
@fragment
fn fs_vol_composite(in: VfVsOut) -> @location(0) vec4<f32> {
    let r = vf_pixel_ray(in.clip.xy);
    let c = camera.cam_pos.xyz;
    let sky = r.w < 0.0;
    let full = enh.vol_fog.z;
    let reach = select(min(r.w, full), full, sky);
    let cab = select(vf_cab(c, r.xyz, reach), vec2<f32>(0.0), sky);
    let a = vf_analytic(c, r.xyz, reach, sky, cab);
    let v = vf_gather(in.clip.xy, select(reach, -reach, sky));
    // (where the analytic air let next to nothing through, what the picture holds there is
    // that air itself: the ratio is kept within reason)
    let ratio = clamp(v.a / max(a.a, 1e-4), 0.0, 4.0);
    return vec4<f32>(v.rgb - a.rgb * (enh.exposure.x * ratio), ratio);
}
