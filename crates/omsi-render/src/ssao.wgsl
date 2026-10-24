// Screen-space ambient occlusion from the single-sampled depth prepass, and a blur.
struct SsaoParams {
    inv_proj: mat4x4<f32>,
    // x radius (m), y strength, z width, w height
    params: vec4<f32>,
    // xy: the projection's off-centre shift (m20, m21), zero for a symmetric frustum
    shift: vec4<f32>,
    // x the wide occlusion's reach (m), y its strength (0 = off), z the bounced light's
    // gain (0 = off), w the largest radiance a bounce is taken from (1 = 10 000 cd/m²)
    wide: vec4<f32>,
    // from a view-space point to the last frame's clip space, and in x whether that
    // frame's picture (`t_hist`) can be read at all (not after a cut)
    reproj: mat4x4<f32>,
    history: vec4<f32>,
};
@group(0) @binding(0) var<uniform> p: SsaoParams;
@group(0) @binding(1) var t_depth: texture_depth_2d;
// The last frame's picture at this size, as absolute radiance (1 = 10 000 cd/m²): what the
// surfaces round a point send to it (post.wgsl `fs_history`). Read where the surface was
// then (`reproj`), and only where the last frame's depth (the blurred AO's g, still in
// `t_ao` when this pass runs) agrees: a surface that was hidden then has no picture to read.
@group(0) @binding(3) var t_hist: texture_2d<f32>;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    // one triangle over the whole screen
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    var out: VsOut;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

// view-space position of a pixel (reversed Z: the depth is 0 at the far plane)
fn view_pos(px: vec2<i32>) -> vec3<f32> {
    let size = vec2<i32>(i32(p.params.z), i32(p.params.w));
    let c = clamp(px, vec2<i32>(0), size - vec2<i32>(1));
    // the AO runs at half the picture's size; the depth prepass is full size
    let d = textureLoad(t_depth, c * 2, 0);
    let ndc = vec2<f32>((f32(c.x) + 0.5) / p.params.z * 2.0 - 1.0, 1.0 - (f32(c.y) + 0.5) / p.params.w * 2.0);
    let h = p.inv_proj * vec4<f32>(ndc, d, 1.0);
    return h.xyz / h.w;
}

// view-space depth (negative, metres) of a raw depth value
fn lin(d: f32) -> f32 {
    let h = p.inv_proj * vec4<f32>(0.0, 0.0, d, 1.0);
    return h.z / h.w;
}

// A 4x4 ordered pattern rather than white noise: every 4x4 block of pixels holds the same
// sixteen sample rotations, so the 6x6 blur that follows averages them out completely.
// White noise per pixel left a grain that stood still on the screen while the world moved
// under it - a faint film of static in front of the eyes.
fn hash(v: vec2<f32>) -> f32 {
    let p = vec2<i32>(i32(v.x) & 3, i32(v.y) & 3);
    let bayer = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    return (bayer[p.y * 4 + p.x] + 0.5) / 16.0;
}

// The bounced light (irradiance, rgb) in two channels of the AO texture's four: its
// luminance as it is, and the colour's red and blue relative to it (0 .. 2, five bits
// each) in one number - the green follows from the luminance. Blurred and read back
// decoded, so the quantisation of one tap is not mixed with another's.
const BOUNCE_RANGE: f32 = 2.0;
fn bounce_pack(e: vec3<f32>) -> vec2<f32> {
    let y = dot(e, vec3<f32>(0.2126, 0.7152, 0.0722));
    if (y < 1e-6) {
        return vec2<f32>(0.0, 0.0);
    }
    let u = clamp(e.r / y / BOUNCE_RANGE, 0.0, 1.0);
    let v = clamp(e.b / y / BOUNCE_RANGE, 0.0, 1.0);
    return vec2<f32>(y, round(u * 31.0) * 32.0 + round(v * 31.0));
}

fn bounce_unpack(t: vec4<f32>) -> vec3<f32> {
    let y = t.b;
    let iu = floor(t.a / 32.0 + 0.001);
    let iv = t.a - iu * 32.0;
    let r = iu / 31.0 * BOUNCE_RANGE * y;
    let b = iv / 31.0 * BOUNCE_RANGE * y;
    let g = max((y - 0.2126 * r - 0.0722 * b) / 0.7152, 0.0);
    return vec3<f32>(r, g, b);
}

@fragment
fn fs_ssao(in: VsOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.clip.xy);
    let d0 = textureLoad(t_depth, px * 2, 0);
    if (d0 <= 0.0) {
        return vec4<f32>(1.0, 60000.0, 0.0, 0.0); // sky
    }
    let pos = view_pos(px);
    // the normal from the depth neighbourhood (the smaller step of each pair, so an edge
    // between two objects does not bend it)
    let dx1 = view_pos(px + vec2<i32>(1, 0)) - pos;
    let dx2 = pos - view_pos(px - vec2<i32>(1, 0));
    let dy1 = view_pos(px + vec2<i32>(0, 1)) - pos;
    let dy2 = pos - view_pos(px - vec2<i32>(0, 1));
    let dx = select(dx2, dx1, length(dx1) < length(dx2));
    let dy = select(dy2, dy1, length(dy1) < length(dy2));
    var n = normalize(cross(dx, dy));
    if (dot(n, -pos) < 0.0) {
        n = -n;
    }
    // a hemisphere of samples around the normal, turned by a per-pixel angle
    let a = hash(in.clip.xy) * 6.2831853;
    let rnd = vec3<f32>(cos(a), sin(a), 0.0);
    let t = normalize(rnd - n * dot(rnd, n));
    let b = cross(n, t);
    // A metre and a half of radius is right for a house corner across the street, but in
    // a bus the hand straps darkened the ceiling a metre away from them, in blotches that
    // moved as the head turned: close to the camera the radius shrinks with the distance.
    let radius = min(p.params.x, 0.3 + 0.1 * -pos.z);
    var occ = 0.0;
    let count = 12;
    for (var i = 0; i < count; i = i + 1) {
        let fi = f32(i);
        // a spiral over the hemisphere, denser near the middle
        let ang = fi * 2.399963 + a;
        let r = (fi + 0.5) / f32(count);
        let scale = mix(0.15, 1.0, r * r);
        let s = vec3<f32>(cos(ang) * r, sin(ang) * r, sqrt(max(0.0, 1.0 - r * r)));
        let sp = pos + (t * s.x + b * s.y + n * s.z) * radius * scale;
        // back to the screen
        // project: we only have inv_proj, so search the depth at the sample's screen position
        // using the perspective relation x_ndc = x / (-z * tan) - reconstruct from two points
        let q = project(sp);
        if (q.x < 0.0 || q.y < 0.0 || q.x >= p.params.z || q.y >= p.params.w) {
            continue;
        }
        let qz = lin(textureLoad(t_depth, vec2<i32>(q) * 2, 0));
        // occluded when the scene surface is in front of the sample, within range
        let dz = qz - sp.z;
        let range = smoothstep(0.0, 1.0, radius / max(abs(pos.z - qz), 0.0001));
        if (dz > 0.03) {
            occ = occ + range;
        }
    }
    var ao = 1.0 - p.params.y * occ / f32(count);
    // --- the street's scale: how much of the sky a point sees past the houses, trees and
    // hills round it (a hemisphere reaching `wide.x` metres, cosine-weighted), and the
    // light the surfaces that block it send back (their picture of the last frame)
    var wide = 1.0;
    var bounce = vec3<f32>(0.0);
    if (p.wide.y > 0.0) {
        let wr = p.wide.x;
        let wcount = 8;
        var wocc = 0.0;
        var gi = vec3<f32>(0.0);
        let a2 = a + 1.7;
        for (var i = 0; i < wcount; i = i + 1) {
            let fi = f32(i);
            let ang = fi * 2.399963 + a2;
            let r = (fi + 0.5) / f32(wcount);
            let scale = mix(0.2, 1.0, fract(fi * 0.618034 + a * 0.159155));
            let s = vec3<f32>(cos(ang) * r, sin(ang) * r, sqrt(max(0.0, 1.0 - r * r)));
            let sp = pos + (t * s.x + b * s.y + n * s.z) * wr * scale;
            let q = project(sp);
            if (q.x < 0.0 || q.y < 0.0 || q.x >= p.params.z || q.y >= p.params.w) {
                continue;
            }
            let qc = vec2<i32>(q);
            let qz = lin(textureLoad(t_depth, qc * 2, 0));
            let dz = qz - sp.z;
            // a surface counts for what it takes of the hemisphere: not further in front of
            // the sample than the sample is from the point (a wall over the point, not
            // another object's edge), and never the cab round the camera: a dashboard and its
            // pillars are not the street's houses
            let reach = max(wr * scale, 1.0);
            let range = (1.0 - smoothstep(0.25 * reach, 0.6 * reach, dz)) * smoothstep(2.0, 6.0, -qz);
            if (dz > 0.25 && range > 0.0) {
                wocc = wocc + range;
                if (p.wide.z > 0.0 && p.history.x > 0.5) {
                    // (a lamp or a lit sign among the hits would be one sample of eight worth
                    // a hundred of its neighbours' and stand in the picture as a pattern of
                    // spots: what is brighter than a sunlit white wall is compressed)
                    let lc = history_light(qc, qz);
                    gi = gi + lc / (vec3<f32>(1.0) + lc * 0.35) * range;
                }
            }
        }
        wide = 1.0 - p.wide.y * wocc / f32(wcount);
        bounce = gi * (3.14159265 / f32(wcount)) * p.wide.z;
    }
    ao = ao * wide;
    // far away a screen-space radius covers whole buildings: fade the effect out with
    // distance, where it only added shimmer
    let dist = -pos.z;
    ao = mix(ao, wide, clamp((dist - 60.0) / 140.0, 0.0, 1.0));
    // (the wide occlusion reaches farther: it fades over the next two hundred metres)
    ao = mix(ao, 1.0, clamp((dist - 200.0) / 250.0, 0.0, 1.0));
    bounce = bounce * (1.0 - clamp((dist - 200.0) / 250.0, 0.0, 1.0));
    // (clamp of a NaN is not safe on every GPU: a black blotch of shade on Windows)
    let ao_ok = select(1.0, clamp(ao, 0.0, 1.0), (bitcast<u32>(ao) & 0x7f800000u) != 0x7f800000u);
    let bounce_ok = select(vec3<f32>(0.0), bounce, all((bitcast<vec3<u32>>(bounce) & vec3<u32>(0x7f800000u)) != vec3<u32>(0x7f800000u)));
    let pk = bounce_pack(bounce_ok);
    return vec4<f32>(ao_ok, dist, pk.x, pk.y);
}

// What the surface seen at pixel `qc` (view depth `qz`) sent to the last frame's picture:
// where it was in that frame, if it was seen there.
fn history_light(qc: vec2<i32>, qz: f32) -> vec3<f32> {
    // (the surface's view position from its pixel and depth: no second depth load)
    let ndc0 = vec2<f32>((f32(qc.x) + 0.5) / p.params.z * 2.0 - 1.0, 1.0 - (f32(qc.y) + 0.5) / p.params.w * 2.0);
    let sp = vec3<f32>((ndc0.x + p.shift.x) * -qz * p.inv_proj[0][0], (ndc0.y + p.shift.y) * -qz * p.inv_proj[1][1], qz);
    let pc = p.reproj * vec4<f32>(sp, 1.0);
    if (pc.w <= 0.01) {
        return vec3<f32>(0.0);
    }
    let ndc = pc.xy / pc.w;
    let q = vec2<f32>((ndc.x + 1.0) * 0.5 * p.params.z, (1.0 - ndc.y) * 0.5 * p.params.w);
    if (q.x < 0.0 || q.y < 0.0 || q.x >= p.params.z || q.y >= p.params.w) {
        return vec3<f32>(0.0);
    }
    let pq = vec2<i32>(q);
    // (disoccluded: the surface's depth then, from the last frame's own AO, is not where
    // it should be)
    let was = textureLoad(t_ao, pq, 0).g;
    if (abs(was - pc.w) > 0.2 + 0.04 * pc.w) {
        return vec3<f32>(0.0);
    }
    // (the picture's outliers are cut where it is made: post.wgsl `fs_history`)
    let l = max(textureLoad(t_hist, pq, 0).rgb, vec3<f32>(0.0));
    return min(l, vec3<f32>(p.wide.w));
}

// pixel position of a view-space point, through the same projection the depth was made with
fn project(v: vec3<f32>) -> vec2<f32> {
    // inv_proj is the inverse of a perspective matrix: recover the projection's scale terms
    // from it (m00 = 1/(f/aspect), m11 = 1/f); an off-centre frustum (a headset eye, a
    // triple screen's side panel) adds its shift: x_ndc = (sx x + m20 z) / -z
    let sx = 1.0 / p.inv_proj[0][0];
    let sy = 1.0 / p.inv_proj[1][1];
    let ndc = vec2<f32>(v.x * sx, v.y * sy) / (-v.z) - p.shift.xy;
    return vec2<f32>((ndc.x + 1.0) * 0.5 * p.params.z, (1.0 - ndc.y) * 0.5 * p.params.w);
}

// --- blur: 6x6 window over the raw AO, keeps edges by depth
@group(0) @binding(2) var t_ao: texture_2d<f32>;

const TENT = array<f32, 36>(
    0.09901, 0.18498, 0.26915, 0.30498, 0.26915, 0.18498,
    0.18498, 0.34559, 0.50283, 0.56978, 0.50283, 0.34559,
    0.26915, 0.50283, 0.73162, 0.82903, 0.73162, 0.50283,
    0.30498, 0.56978, 0.82903, 0.93941, 0.82903, 0.56978,
    0.26915, 0.50283, 0.73162, 0.82903, 0.73162, 0.50283,
    0.18498, 0.34559, 0.50283, 0.56978, 0.50283, 0.34559,
);

@fragment
fn fs_blur(in: VsOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.clip.xy);
    let size = vec2<i32>(i32(p.params.z), i32(p.params.w));
    let z0 = lin(textureLoad(t_depth, px * 2, 0));
    // Without bounced light (every pipeline but the Enhanced one, and the Enhanced with the
    // bounce off) the blur is the plain 6x6 one: as cheap and as it always was.
    if (p.wide.z <= 0.0) {
        var sum0 = 0.0;
        var wsum0 = 0.0;
        // 6x6 window: covers every rotation of the 4x4 pattern at least once in each row and
        // column, and the depth weight keeps the occlusion from bleeding across an edge (a
        // relative tolerance: far surfaces are allowed a larger step)
        for (var y = -3; y <= 2; y = y + 1) {
            for (var x = -3; x <= 2; x = x + 1) {
                let c = clamp(px + vec2<i32>(x, y), vec2<i32>(0), size - vec2<i32>(1));
                let z = lin(textureLoad(t_depth, c * 2, 0));
                let tol0 = 0.3 + 0.02 * abs(z0);
                let w = select(0.0, 1.0, abs(z - z0) < tol0);
                sum0 = sum0 + textureLoad(t_ao, c, 0).r * w;
                wsum0 = wsum0 + w;
            }
        }
        let blurred0 = select(1.0, sum0 / wsum0, wsum0 > 0.0);
        return vec4<f32>(select(1.0, blurred0, (bitcast<u32>(blurred0) & 0x7f800000u) != 0x7f800000u), select(-z0, 60000.0, textureLoad(t_depth, px * 2, 0) <= 0.0), 0.0, 0.0);
    }
    var sum = 0.0;
    var gi_sum = vec3<f32>(0.0);
    var wsum = 0.0;
    var gsum = 0.0;
    // With the bounce the same 6x6 window for both: the occlusion's weight falls off
    // smoothly (a hard cut left the pattern standing wherever a slanting ground's steps from
    // pixel to pixel came near it); the bounced light, smooth by nature and noisier by its
    // few hits, takes a looser depth tolerance and a tent over the window, so one hit
    // leaves a soft spot, not a square.
    let tol = 0.3 + 0.04 * abs(z0);
    let gtol = 0.5 + 0.1 * abs(z0);
    for (var y = -3; y <= 2; y = y + 1) {
        for (var x = -3; x <= 2; x = x + 1) {
            let c = clamp(px + vec2<i32>(x, y), vec2<i32>(0), size - vec2<i32>(1));
            let z = lin(textureLoad(t_depth, c * 2, 0));
            let dz = abs(z - z0);
            let tap = textureLoad(t_ao, c, 0);
            let w = 1.0 - smoothstep(0.5 * tol, tol, dz);
            sum = sum + tap.r * w;
            wsum = wsum + w;
            let gw = (1.0 - smoothstep(0.5 * gtol, gtol, dz)) * TENT[(y + 3) * 6 + x + 3];
            gi_sum = gi_sum + bounce_unpack(tap) * gw;
            gsum = gsum + gw;
        }
    }
    let blurred = select(1.0, sum / wsum, wsum > 0.0);
    let gi = select(vec3<f32>(0.0), gi_sum / gsum, gsum > 0.0);
    let pk = bounce_pack(gi);
    return vec4<f32>(select(1.0, blurred, (bitcast<u32>(blurred) & 0x7f800000u) != 0x7f800000u), select(-z0, 60000.0, textureLoad(t_depth, px * 2, 0) <= 0.0), pk.x, pk.y);
}
