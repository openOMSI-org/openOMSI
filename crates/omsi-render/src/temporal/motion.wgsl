// Temporal upscaling: the depth prepass that also writes each pixel's motion (see `frame.rs`).
// Appended to the scene module only for this prepass (`pipelines/temporal.rs`): the scene module
// everyone else compiles stays as it is. Its depth is `vs_depth`'s to the last bit (the same
// `scene_clip`), so the main pass's depth test meets it.

struct Motion {
    // this frame's view-projection without the sub-pixel jitter
    vp: mat4x4<f32>,
    // last frame's, for last frame's model matrices (relative to last frame's render origin)
    vp_prev: mat4x4<f32>,
    // last frame's, relative to this frame's render origin: the sky, and the entries without
    // a matrix of last frame
    vp_prev_here: mat4x4<f32>,
    // the inverse of `vp` (the sky's directions)
    vp_inv: mat4x4<f32>,
    // xy: the render size in pixels, z: 1 when `prev_models` holds last frame's matrices,
    // w: how many entries it holds
    params: vec4<f32>,
    // xyz: last frame's render origin minus this one's, w: the most an object may move in a
    // frame and still be the same object (`MAX_FRAME_JUMP`)
    origin_delta: vec4<f32>,
};
// (group 2 is the vehicle reflection's in the scene module: the motion is group 3)
@group(3) @binding(0) var<uniform> motion: Motion;
// last frame's model matrices, laid out as `models`
@group(3) @binding(1) var<storage, read> prev_models: array<vec4<f32>>;

struct MotionOut {
    @builtin(position) @invariant clip: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) params: vec4<f32>,
    // (without the jitter) this frame's and last frame's clip position
    @location(5) cur: vec4<f32>,
    @location(6) prev: vec4<f32>,
};
// (the fragment side of MotionOut, without the invariant: see DepthIn)
struct MotionIn {
    @builtin(position) clip: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) params: vec4<f32>,
    @location(5) cur: vec4<f32>,
    @location(6) prev: vec4<f32>,
};

@vertex
fn vs_motion(in: VsIn) -> MotionOut {
    let e = draw_list[in.inst];
    let m = model_matrix(e);
    let local = vec4<f32>(in.pos, 1.0);
    let wp = m * local;
    var out: MotionOut;
    // (as `vs_depth`: the windy trees' sway too)
    out.clip = scene_clip(e, wp + vec4<f32>(tree_sway(in.pos, m), 0.0), (m * vec4<f32>(in.normal, 0.0)).xyz);
    let pr = inst_params[e * 2u];
    out.uv = in.uv + pr.zw;
    out.params = pr;
    out.cur = motion.vp * wp;
    if (motion.params.z > 0.5 && f32(e) < motion.params.w) {
        let k = e * 4u;
        // (x's w carries the slot's draw order level, not the matrix: `model_matrix` drops it)
        let pm = mat4x4<f32>(vec4<f32>(prev_models[k].xyz, 0.0), prev_models[k + 1u], prev_models[k + 2u], prev_models[k + 3u]);
        // (an entry handed over to another object since: its old matrix is not this one's)
        let jump = pm[3].xyz + motion.origin_delta.xyz - m[3].xyz;
        if (dot(jump, jump) <= motion.origin_delta.w * motion.origin_delta.w) {
            out.prev = motion.vp_prev * (pm * local);
        } else {
            out.prev = motion.vp_prev_here * wp;
        }
    } else {
        out.prev = motion.vp_prev_here * wp;
    }
    if (pr.y < 0.5) {
        // invisible: collapse the triangle
        out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    }
    return out;
}

// From this pixel to where the same point was last frame, in pixels (x right, y down).
fn motion_pixels(cur: vec4<f32>, prev: vec4<f32>) -> vec4<f32> {
    if (prev.w <= 1e-4 || cur.w <= 1e-4) {
        return vec4<f32>(0.0);
    }
    let d = (prev.xy / prev.w - cur.xy / cur.w) * vec2<f32>(0.5, -0.5) * motion.params.xy;
    return vec4<f32>(d, 0.0, 0.0);
}

@fragment
fn fs_motion(in: MotionIn) -> @location(0) vec4<f32> {
    return motion_pixels(in.cur, in.prev);
}

@fragment
fn fs_motion_test(in: MotionIn) -> @location(0) vec4<f32> {
    cutout_coverage(in.uv, in.params, camera.lod.x);
    return motion_pixels(in.cur, in.prev);
}

@fragment
fn fs_motion_transmap(in: MotionIn) -> @location(0) vec4<f32> {
    transmap_coverage(in.uv, in.params, camera.lod.x);
    return motion_pixels(in.cur, in.prev);
}

// The sky (whatever the prepass leaves uncovered): a point on the far plane, drawn first.
struct SkyMotionOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_motion_sky(@builtin(vertex_index) i: u32) -> SkyMotionOut {
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    var out: SkyMotionOut;
    // (reversed Z: depth 0 is the far plane)
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.ndc = vec2<f32>(x, y);
    return out;
}

@fragment
fn fs_motion_sky(in: SkyMotionOut) -> @location(0) vec4<f32> {
    let far = motion.vp_inv * vec4<f32>(in.ndc, 0.0, 1.0);
    let wp = vec4<f32>(far.xyz / far.w, 1.0);
    return motion_pixels(vec4<f32>(in.ndc, 0.0, 1.0), motion.vp_prev_here * wp);
}

// The transparency mask DLSS (its "bias current color" hint) and FSR 3 (its reactive mask) take:
// where glass, its dirt and reflections, or the rain films on it lie over the picture, they lean
// on this frame's colour rather than the history the motion behind the pane drags along (that
// history smeared the pane's reflections). Only the transparent draws, tested against the
// prepass's depth, writing the most of what lands on each pixel.
const MASK_RAIN: f32 = 0.5;
const MASK_SCREEN: f32 = 0.8;

struct MaskOut {
    @builtin(position) @invariant clip: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) params: vec4<f32>,
};
struct MaskIn {
    @builtin(position) clip: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) params: vec4<f32>,
};

@vertex
fn vs_mask(in: VsIn) -> MaskOut {
    let e = draw_list[in.inst];
    let m = model_matrix(e);
    let wp = m * vec4<f32>(in.pos, 1.0) + vec4<f32>(tree_sway(in.pos, m), 0.0);
    var out: MaskOut;
    out.clip = scene_clip(e, wp, (m * vec4<f32>(in.normal, 0.0)).xyz);
    let pr = inst_params[e * 2u];
    out.uv = in.uv + pr.zw;
    out.params = pr;
    if (pr.y < 0.5) {
        out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    }
    return out;
}

@fragment
fn fs_glass_mask(in: MaskIn) -> @location(0) vec4<f32> {
    let a = diffuse_border(textureSampleBias(t_diffuse, s_diffuse, tex_address(in.uv), camera.lod.x), in.uv).a
        * material.color.a * in.params.x;
    return vec4<f32>(clamp(a, 0.0, 1.0));
}

@fragment
fn fs_rain_mask(in: MaskIn) -> @location(0) vec4<f32> {
    return vec4<f32>(MASK_RAIN);
}

// The bus's own screens (IBIS, matrix displays, the dashboard's LCDs): their picture changes
// without any motion, so the history behind them is useless and swam over what passes them.
@fragment
fn fs_screen_mask(in: MaskIn) -> @location(0) vec4<f32> {
    return vec4<f32>(MASK_SCREEN);
}
