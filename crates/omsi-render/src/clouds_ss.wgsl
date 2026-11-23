// Enhanced graphics: the window's clouds, drawn per pixel the way games draw volumetric
// clouds (Horizon Zero Dawn, Frostbite, Unreal):
// * `fs_enhanced_cloud_trace` marches them (sky_enhanced.wgsl `cloud_march`) for a pixel
//   in four each way, its ray through another quarter of the pixel each frame and its
//   steps started at another point (interleaved gradient noise moved on by the golden
//   ratio), and keeps how far the clouds are seen; where the scene's geometry covers the pixel and every
//   pixel round it (the depth prepass) it marches nothing;
// * `fs_enhanced_cloud_resolve` takes that into a picture at half the window's size: each
//   pixel finds where its clouds were in the last frame's picture (their depth and the last
//   frame's view, the camera's and the clouds' own movement), takes the history from there
//   clamped to what the traced pixels round it allow (so nothing ghosts when a cloud or
//   the view changes), and blends in the new trace - most where a traced ray landed on it
//   this frame. Turning the view, driving and the clouds' drift move nothing but what moves;
// * the dome (`enhanced_sky`) looks the result up only where the sky shows: the geometry is
//   drawn first, so the clouds never cover it.

// The scene's depth (the prepass, reversed: 0 where nothing was drawn); a texel when there
// is none.
@group(2) @binding(0) var t_scene_depth: texture_depth_2d;
// The trace (`fs_enhanced_cloud_trace`): its light and transmittance, and the clouds' depth
// (m; < 0 where the geometry covers it); the accumulated picture of the frame before.
@group(0) @binding(23) var t_trace: texture_2d<f32>;
@group(0) @binding(24) var t_trace_depth: texture_2d<f32>;
@group(0) @binding(25) var t_cloud_hist: texture_2d<f32>;

// This pass's frame number: the frame's, with the round of a picture drawn on its own
// (`probe.p.x`) added.
fn cloud_frame() -> f32 {
    return cview.frame.x + probe.p.x;
}

// Where in its pixel the trace's ray goes this frame (in traced pixels): the four quarters
// in turn, each the centre of one of the resolved picture's pixels.
fn trace_jitter(frame: f32) -> vec2<f32> {
    let k = u32(frame) % 4u;
    return vec2<f32>(select(-0.25, 0.25, k == 1u || k == 2u), select(-0.25, 0.25, k == 1u || k == 3u));
}

// The view's direction through the point uv (0..1, y down) of the picture.
fn cloud_view_dir(uv: vec2<f32>) -> vec3<f32> {
    let p = cview.inv_vp * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.5, 1.0);
    return normalize(p.xyz / p.w);
}

// Whether the scene's geometry covers the traced pixel at `px` and a traced pixel round it
// (what the resolve and the dome's filter read), every other pixel of it looked at.
fn cloud_covered(px: vec2<f32>) -> bool {
    let dims = vec2<i32>(textureDimensions(t_scene_depth));
    if (dims.x < 4) {
        return false;
    }
    let scale = vec2<f32>(dims) / cview.size.xy;
    let c = vec2<i32>((floor(px) + vec2<f32>(0.5)) * scale);
    let n = i32(ceil(max(scale.x, scale.y) * 1.5));
    for (var y = -n; y <= n; y = y + 2) {
        for (var x = -n; x <= n; x = x + 2) {
            let q = clamp(c + vec2<i32>(x, y), vec2<i32>(0), dims - vec2<i32>(1));
            if (textureLoad(t_scene_depth, q, 0) <= 0.0) {
                return false;
            }
        }
    }
    return true;
}

struct TraceOut {
    @location(0) col: vec4<f32>,
    @location(1) depth: f32,
};

@fragment
fn fs_enhanced_cloud_trace(in: FsIn) -> TraceOut {
    if (cloud_covered(in.clip.xy)) {
        return TraceOut(vec4<f32>(0.0, 0.0, 0.0, 1.0), -1.0);
    }
    let frame = cloud_frame();
    let uv = (in.clip.xy + trace_jitter(frame)) / cview.size.xy;
    let d = cloud_view_dir(uv);
    let ign = fract(52.9829189 * fract(dot(floor(in.clip.xy), vec2<f32>(0.06711056, 0.00583715))));
    let jitter = fract(ign + frame * 0.61803399);
    let bg = sky_table(d);
    let c = cloud_march(d, bg, i32(cview.frame.w), jitter, cview.params.z, true);
    return TraceOut(vec4<f32>(c.light / max(enh.ground.w, 1e-8), c.trans), c.dist);
}

@fragment
fn fs_enhanced_cloud_resolve(in: FsIn) -> @location(0) vec4<f32> {
    let frame = cloud_frame();
    let ts = cview.size.xy;
    let tsi = vec2<i32>(ts);
    let uv = in.clip.xy / cview.size.zw;
    // where this pixel lies among the traced ones (their centres at i + 0.5 this frame)
    let x = uv * ts - trace_jitter(frame);
    let ic = clamp(vec2<i32>(floor(x)), vec2<i32>(0), tsi - vec2<i32>(1));
    let fresh = all(abs(fract(x) - vec2<f32>(0.5)) < vec2<f32>(0.15));
    var cur = textureSampleLevel(t_trace, s_lin, x / ts, 0.0);
    if (fresh) {
        cur = textureLoad(t_trace, ic, 0);
    }
    // the traced pixels round it: their mean and spread (the geometry's left out)
    var m1 = vec4<f32>(0.0);
    var m2 = vec4<f32>(0.0);
    var n = 0.0;
    for (var dy = -1; dy <= 1; dy = dy + 1) {
        for (var dx = -1; dx <= 1; dx = dx + 1) {
            let q = clamp(ic + vec2<i32>(dx, dy), vec2<i32>(0), tsi - vec2<i32>(1));
            if (textureLoad(t_trace_depth, q, 0).r >= 0.0) {
                let s = textureLoad(t_trace, q, 0);
                m1 = m1 + s;
                m2 = m2 + s * s;
                n = n + 1.0;
            }
        }
    }
    if (n < 0.5) {
        return cur;
    }
    let mean = m1 / n;
    let spread = sqrt(max(m2 / n - mean * mean, vec4<f32>(0.0)));
    // where the clouds this pixel sees were in the last frame's picture
    let depth = textureLoad(t_trace_depth, ic, 0).r;
    let p = cloud_view_dir(uv) * select(1e6, depth, depth > 0.0) + cview.moved.xyz;
    let pc = cview.prev_vp * vec4<f32>(p, 1.0);
    let manual = probe.p.y >= 0.0;
    var w = select(cview.moved.w, probe.p.y, manual);
    if (pc.w <= 1e-6) {
        w = 0.0;
    }
    let puv = vec2<f32>(pc.x / pc.w * 0.5 + 0.5, 0.5 - pc.y / pc.w * 0.5);
    if (any(puv < vec2<f32>(0.0)) || any(puv > vec2<f32>(1.0))) {
        w = 0.0;
    }
    if (w <= 0.0) {
        return cur;
    }
    var hist = textureSampleLevel(t_cloud_hist, s_lin, puv, 0.0);
    hist = vec4<f32>(hist.rgb * cview.params.y, hist.a);
    if (!manual) {
        // (the history only within what the neighbourhood allows; a fresh ray replaces
        // more of it than the upsampled trace between them)
        let g = 1.25;
        hist = clamp(hist, mean - g * spread - vec4<f32>(1e-4), mean + g * spread + vec4<f32>(1e-4));
        w = w * select(0.985, 0.9, fresh);
    }
    return mix(cur, hist, w);
}
