// Enhanced+ (experimental, `rt_gi`): the traced global illumination's denoiser (rt.rs `gi`),
// part of the reflections' denoiser module (rt_denoise.wgsl: its bindings, `vehicle_prev`,
// `oct_decode`). Between the rays (rt_gi.wgsl `cs_gi`, one per GI_DIV x GI_DIV pixels) and
// the main pass:
// - `cs_gi_temporal`: each ray's light added to its surface's history - found where that
//   surface was a frame ago (on the player's vehicle, where the vehicle was) at the same depth
//   and facing the same way - over `p.temporal.w` frames at most;
// - `cs_gi_atrous`: four passes of an a-trous wavelet (3 x 3 taps) 1, 2, 4 and 8 rays apart
//   over the rays on the same surface (depth, normal);
// - `cs_gi_upsample`: up to the full picture, per pixel from the rays round it on its own
//   surface; a = -1 where none is (a pole thinner than the rays' grid: the main pass lights
//   it as without the traced light).
// The history's meta (t_out_m): x the view depth, y the frames in it, zw the normal.

// (one ray per GI_DIV x GI_DIV pixels, from the block's pixel GI_OFF in: rt_gi.wgsl)
const GI_DIV: i32 = 4;
const GI_OFF: i32 = 2;

fn gi_cell_px(q: vec2<i32>) -> vec2<i32> {
    return min(q * GI_DIV + vec2<i32>(GI_OFF), vec2<i32>(p.size.xy) - vec2<i32>(1));
}

@compute @workgroup_size(8, 8)
fn cs_gi_temporal(@builtin(global_invocation_id) gid: vec3<u32>) {
    let q = vec2<i32>(gid.xy);
    let size = vec2<i32>(textureDimensions(t_cur));
    if (any(q >= size)) {
        return;
    }
    let m = textureLoad(t_meta, q, 0);
    let cur = textureLoad(t_cur, q, 0);
    if (m.x <= 0.0) {
        textureStore(t_out, q, cur);
        textureStore(t_out_m, q, vec4<f32>(0.0));
        return;
    }
    let n = oct_decode(m.zw);
    let px = gi_cell_px(q);
    let depth = textureLoad(t_depth, px, 0);
    let pt = world_pos((vec2<f32>(px) + vec2<f32>(0.5)) * p.size.zw, depth);
    let pc = p.prev_view_proj * vec4<f32>(vehicle_prev(pt), 1.0);
    var hist = vec4<f32>(0.0);
    var count = 0.0;
    if (p.temporal.x > 0.5 && pc.w > 0.05) {
        let puv = vec2<f32>(pc.x / pc.w * 0.5 + 0.5, 0.5 - pc.y / pc.w * 0.5);
        // (the rays' grid of the last frame: cell k stands for pixel k * GI_DIV + GI_OFF)
        let f = (puv * p.size.xy - vec2<f32>(f32(GI_OFF) + 0.5)) / f32(GI_DIV);
        let b = vec2<i32>(floor(f));
        let fr = f - floor(f);
        let ptol = 0.03 * pc.w + 0.1;
        var wsum = 0.0;
        var csum = 0.0;
        for (var j = 0; j <= 1; j++) {
            for (var i = 0; i <= 1; i++) {
                let s = b + vec2<i32>(i, j);
                if (all(s >= vec2<i32>(0)) && all(s < size)) {
                    let pm = textureLoad(t_prev_m, s, 0);
                    let w = select(1.0 - fr.x, fr.x, i == 1) * select(1.0 - fr.y, fr.y, j == 1);
                    if (pm.x > 0.0 && abs(pm.x - pc.w) < ptol && dot(oct_decode(pm.zw), n) > 0.8 && w > 1e-3) {
                        hist += textureLoad(t_prev, s, 0) * w;
                        csum += pm.y * w;
                        wsum += w;
                    }
                }
            }
        }
        if (wsum > 0.05) {
            hist /= wsum;
            count = csum / wsum;
        }
    }
    let total = min(count + 1.0, p.temporal.w);
    let c = mix(hist, cur, 1.0 / total);
    textureStore(t_out, q, c);
    textureStore(t_out_m, q, vec4<f32>(m.x, total, m.zw));
}

@compute @workgroup_size(8, 8)
fn cs_gi_atrous(@builtin(global_invocation_id) gid: vec3<u32>) {
    let q = vec2<i32>(gid.xy);
    let size = vec2<i32>(textureDimensions(t_cur));
    if (any(q >= size)) {
        return;
    }
    let m = textureLoad(t_meta, q, 0);
    let c = textureLoad(t_cur, q, 0);
    if (m.x <= 0.0) {
        textureStore(t_out, q, c);
        return;
    }
    let n = oct_decode(m.zw);
    let step = max(i32(pass_step.x), 1);
    // (a fresh history - a surface just come into view - is blurred further: the few rays it
    // has are noise)
    let young = 1.0 + 2.0 * (1.0 - smoothstep(2.0, 10.0, m.y));
    let tol = (0.01 * f32(step) * young + 0.01) * m.x + 0.05;
    // (3 x 3 taps of the B-spline's 1/4, 1/2, 1/4: as wide as 5 x 5 over four passes, for a
    // third of the reads)
    var sum = c * 0.25;
    var wsum = 0.25;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            if (i == 0 && j == 0) {
                continue;
            }
            let s = q + vec2<i32>(i, j) * step;
            if (any(s < vec2<i32>(0)) || any(s >= size)) {
                continue;
            }
            let ms = textureLoad(t_meta, s, 0);
            if (ms.x <= 0.0) {
                continue;
            }
            let w_depth = exp(-abs(ms.x - m.x) / tol);
            let w_normal = pow(max(dot(oct_decode(ms.zw), n), 0.0), 16.0);
            let w = select(0.5, 0.25, i != 0) * select(0.5, 0.25, j != 0) * w_depth * w_normal;
            sum += textureLoad(t_cur, s, 0) * w;
            wsum += w;
        }
    }
    textureStore(t_out, q, sum / wsum);
}

@compute @workgroup_size(8, 8)
fn cs_gi_upsample(@builtin(global_invocation_id) gid: vec3<u32>) {
    let px = vec2<i32>(gid.xy);
    if (f32(px.x) >= p.size.x || f32(px.y) >= p.size.y) {
        return;
    }
    let depth = textureLoad(t_depth, px, 0);
    if (depth <= 0.0) {
        textureStore(t_out, px, vec4<f32>(0.0, 0.0, 0.0, -1.0));
        return;
    }
    let z = linear_depth(depth);
    let size = vec2<i32>(textureDimensions(t_cur));
    let f = (vec2<f32>(px) - vec2<f32>(f32(GI_OFF))) / f32(GI_DIV);
    let b = vec2<i32>(floor(f));
    let fr = f - floor(f);
    let tol = 0.02 * z + 0.05;
    var sum = vec4<f32>(0.0);
    var wsum = 0.0;
    var best = vec4<f32>(0.0, 0.0, 0.0, -1.0);
    var best_d = tol * 3.0;
    // (the four rays round the pixel, bilinear, those at its depth only; none: the nearest
    // in depth of them, if near enough - an edge of a thin pole)
    for (var j = 0; j <= 1; j++) {
        for (var i = 0; i <= 1; i++) {
            let s = clamp(b + vec2<i32>(i, j), vec2<i32>(0), size - vec2<i32>(1));
            let ms = textureLoad(t_meta, s, 0);
            if (ms.x <= 0.0) {
                continue;
            }
            let dz = abs(ms.x - z);
            let c = textureLoad(t_cur, s, 0);
            if (dz < best_d) {
                best_d = dz;
                best = c;
            }
            let w = (select(1.0 - fr.x, fr.x, i == 1) * select(1.0 - fr.y, fr.y, j == 1) + 1e-3) * (1.0 - smoothstep(0.0, tol, dz));
            sum += c * w;
            wsum += w;
        }
    }
    var out = best;
    if (wsum > 0.02) {
        out = sum / wsum;
    }
    textureStore(t_out, px, out);
}
