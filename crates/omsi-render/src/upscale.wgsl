// Render scale: the 3D picture, drawn smaller than the window, scaled up to it. A
// Catmull-Rom filter (nine bilinear taps) keeps edges and texture detail far better than a
// plain bilinear stretch; its overshoot is clamped to the neighbouring texels so that
// edges do not ring, and a contrast-adaptive sharpening pass (after AMD's CAS) brings back
// what the smaller picture lost, most where the picture is flat and least on hard edges.
struct Params {
    // xy: size of the source picture, z: sharpening 0..1, w: 1 = the picture at the window's
    // own size smoothed by FXAA instead (the plain graphics without multisampling); 2 = a
    // temporal upscaler's result at the window's size, copied as it is but brought back to
    // linear light (it hands back the sRGB-encoded numbers it read, which the window's sRGB
    // target would encode once more)
    src: vec4<f32>,
};
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var t_src: texture_2d<f32>;
@group(0) @binding(2) var s_src: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    var out: VsOut;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

fn tap(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(t_src, s_src, uv, 0.0).rgb;
}

fn catmull_rom(uv: vec2<f32>, size: vec2<f32>) -> vec3<f32> {
    let pos = uv * size;
    let t1 = floor(pos - 0.5) + 0.5;
    let f = pos - t1;
    let w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    let w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    let w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    let w3 = f * f * (-0.5 + 0.5 * f);
    // the two middle taps become one bilinear tap between them
    let w12 = w1 + w2;
    let p0 = (t1 - 1.0) / size;
    let p3 = (t1 + 2.0) / size;
    let p12 = (t1 + w2 / w12) / size;
    var c = tap(vec2<f32>(p0.x, p0.y)) * w0.x * w0.y;
    c = c + tap(vec2<f32>(p12.x, p0.y)) * w12.x * w0.y;
    c = c + tap(vec2<f32>(p3.x, p0.y)) * w3.x * w0.y;
    c = c + tap(vec2<f32>(p0.x, p12.y)) * w0.x * w12.y;
    c = c + tap(vec2<f32>(p12.x, p12.y)) * w12.x * w12.y;
    c = c + tap(vec2<f32>(p3.x, p12.y)) * w3.x * w12.y;
    c = c + tap(vec2<f32>(p0.x, p3.y)) * w0.x * w3.y;
    c = c + tap(vec2<f32>(p12.x, p3.y)) * w12.x * w3.y;
    c = c + tap(vec2<f32>(p3.x, p3.y)) * w3.x * w3.y;
    return c;
}

// FXAA as the Enhanced post pass does it (after Lottes' FXAA 3.11), on a picture with
// linear values: the luma is taken of their square roots, near enough to the gamma curve.
fn luma(c: vec3<f32>) -> f32 {
    return dot(sqrt(max(c, vec3<f32>(0.0))), vec3<f32>(0.299, 0.587, 0.114));
}

fn lum_at(uv: vec2<f32>) -> f32 {
    return luma(tap(uv));
}

fn fxaa(uv: vec2<f32>, texel: vec2<f32>) -> vec3<f32> {
    let rgbm = tap(uv);
    let m = luma(rgbm);
    let n = lum_at(uv + vec2<f32>(0.0, -texel.y));
    let s = lum_at(uv + vec2<f32>(0.0, texel.y));
    let e = lum_at(uv + vec2<f32>(texel.x, 0.0));
    let w = lum_at(uv + vec2<f32>(-texel.x, 0.0));
    let hi = max(m, max(max(n, s), max(e, w)));
    let lo = min(m, min(min(n, s), min(e, w)));
    let range = hi - lo;
    if (range < max(0.0312, hi * 0.125)) {
        return rgbm;
    }
    let nw = lum_at(uv + vec2<f32>(-texel.x, -texel.y));
    let ne = lum_at(uv + vec2<f32>(texel.x, -texel.y));
    let sw = lum_at(uv + vec2<f32>(-texel.x, texel.y));
    let se = lum_at(uv + vec2<f32>(texel.x, texel.y));
    let avg = (2.0 * (n + s + e + w) + nw + ne + sw + se) / 12.0;
    let sub = smoothstep(0.0, 1.0, clamp(abs(avg - m) / range, 0.0, 1.0));
    let blend_sub = sub * sub * 0.75;
    let horz = abs(nw + sw - 2.0 * w) + abs(n + s - 2.0 * m) * 2.0 + abs(ne + se - 2.0 * e);
    let vert = abs(nw + ne - 2.0 * n) + abs(w + e - 2.0 * m) * 2.0 + abs(sw + se - 2.0 * s);
    let is_h = horz >= vert;
    var stp = select(texel.x, texel.y, is_h);
    let l1 = select(w, n, is_h);
    let l2 = select(e, s, is_h);
    let g1 = abs(l1 - m);
    let g2 = abs(l2 - m);
    let neg = g1 >= g2;
    let grad = max(g1, g2) * 0.25;
    let edge_l = 0.5 * (m + select(l2, l1, neg));
    if (neg) {
        stp = -stp;
    }
    var cuv = uv;
    if (is_h) {
        cuv.y = cuv.y + stp * 0.5;
    } else {
        cuv.x = cuv.x + stp * 0.5;
    }
    let dir = select(vec2<f32>(0.0, texel.y), vec2<f32>(texel.x, 0.0), is_h);
    let steps = array<f32, 12>(1.0, 1.0, 1.0, 1.0, 1.0, 1.5, 2.0, 2.0, 2.0, 2.0, 4.0, 8.0);
    var p_uv = cuv + dir;
    var n_uv = cuv - dir;
    var p_end = lum_at(p_uv) - edge_l;
    var n_end = lum_at(n_uv) - edge_l;
    var p_done = abs(p_end) >= grad;
    var n_done = abs(n_end) >= grad;
    for (var i = 1; i < 12; i = i + 1) {
        if (p_done && n_done) {
            break;
        }
        if (!p_done) {
            p_uv = p_uv + dir * steps[i];
            p_end = lum_at(p_uv) - edge_l;
            p_done = abs(p_end) >= grad;
        }
        if (!n_done) {
            n_uv = n_uv - dir * steps[i];
            n_end = lum_at(n_uv) - edge_l;
            n_done = abs(n_end) >= grad;
        }
    }
    var dp: f32;
    var dn: f32;
    if (is_h) {
        dp = p_uv.x - uv.x;
        dn = uv.x - n_uv.x;
    } else {
        dp = p_uv.y - uv.y;
        dn = uv.y - n_uv.y;
    }
    let near_neg = dn <= dp;
    let end_l = select(p_end, n_end, near_neg);
    let span = dp + dn;
    var blend_edge = 0.0;
    if ((m - edge_l < 0.0) != (end_l < 0.0)) {
        blend_edge = 0.5 - min(dp, dn) / span;
    }
    let blend = max(blend_edge, blend_sub);
    var fuv = uv;
    if (is_h) {
        fuv.y = fuv.y + blend * stp;
    } else {
        fuv.x = fuv.x + blend * stp;
    }
    return tap(fuv);
}

@fragment
fn fs_copy(in: VsOut) -> @location(0) vec4<f32> {
    // Present classic lighting without metering, grading, or a second AA/upscale pass.
    return textureLoad(t_src, vec2<i32>(in.clip.xy), 0);
}

@fragment
fn fs_panel(in: VsOut) -> @location(0) vec4<f32> {
    // Triple screen: one panel's picture 1:1 at its place in the window (src.z: its left
    // edge in window pixels).
    return textureLoad(t_src, vec2<i32>(in.clip.xy) - vec2<i32>(i32(p.src.z), 0), 0);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let size = p.src.xy;
    if (p.src.w > 1.5) {
        var c = max(tap(in.uv), vec3<f32>(0.0));
        // (a light contrast-adaptive sharpening, on the sRGB-encoded numbers as AMD's CAS wants)
        if (p.src.z > 0.0) {
            let px = 1.0 / size;
            let n = tap(in.uv - vec2<f32>(0.0, px.y));
            let s = tap(in.uv + vec2<f32>(0.0, px.y));
            let w = tap(in.uv - vec2<f32>(px.x, 0.0));
            let e = tap(in.uv + vec2<f32>(px.x, 0.0));
            let mn = min(c, min(min(n, s), min(w, e)));
            let mx = max(c, max(max(n, s), max(w, e)));
            let amp = sqrt(clamp(min(mn, vec3<f32>(1.0) - mx) / max(mx, vec3<f32>(1e-4)), vec3<f32>(0.0), vec3<f32>(1.0)));
            let wgt = amp * (-1.0 / mix(8.0, 5.0, p.src.z));
            c = clamp((c + (n + s + w + e) * wgt) / (vec3<f32>(1.0) + 4.0 * wgt), vec3<f32>(0.0), vec3<f32>(1.0));
        }
        let lin = select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
        return vec4<f32>(lin, 1.0);
    }
    if (p.src.w > 0.5) {
        return vec4<f32>(fxaa(in.uv, 1.0 / size), 1.0);
    }
    let px = 1.0 / size;
    // the four texels around the point bound the filtered value (no ringing)
    let base = (floor(in.uv * size - 0.5) + 0.5) * px;
    let a = tap(base);
    let b = tap(base + vec2<f32>(px.x, 0.0));
    let c = tap(base + vec2<f32>(0.0, px.y));
    let d = tap(base + px);
    let lo = min(min(a, b), min(c, d));
    let hi = max(max(a, b), max(c, d));
    var col = clamp(catmull_rom(in.uv, size), lo, hi);
    // contrast-adaptive sharpening on the source's texel grid
    if (p.src.z > 0.0) {
        let n = tap(in.uv - vec2<f32>(0.0, px.y));
        let s = tap(in.uv + vec2<f32>(0.0, px.y));
        let w = tap(in.uv - vec2<f32>(px.x, 0.0));
        let e = tap(in.uv + vec2<f32>(px.x, 0.0));
        let mn = min(col, min(min(n, s), min(w, e)));
        let mx = max(col, max(max(n, s), max(w, e)));
        let amp = sqrt(clamp(min(mn, vec3<f32>(1.0) - mx) / max(mx, vec3<f32>(1e-4)), vec3<f32>(0.0), vec3<f32>(1.0)));
        let peak = -1.0 / mix(8.0, 5.0, p.src.z);
        let wgt = amp * peak;
        col = clamp((col + (n + s + w + e) * wgt) / (vec3<f32>(1.0) + 4.0 * wgt), vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return vec4<f32>(col, 1.0);
}
