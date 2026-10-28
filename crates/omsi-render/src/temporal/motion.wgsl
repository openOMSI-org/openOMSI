// Engine convention: previous minus current, in render pixels, Y down. No jitter.
// This contract is deliberately independent of an unavailable NVIDIA NR header.
struct Temporal {
    previous_vp: mat4x4<f32>,
    size: vec4<f32>,
    time: vec4<f32>,
    tree_wind: vec4<f32>,
    origin: vec4<f32>,
};
@group(2) @binding(2) var<uniform> temporal: Temporal;
@group(2) @binding(3) var<storage, read> previous_models: array<vec4<f32>>;
@group(2) @binding(4) var<storage, read> previous_positions: array<vec4<f32>>;

struct MotionVertex {
    @builtin(position) @invariant clip: vec4<f32>,
    @location(0) current: vec4<f32>,
    @location(1) previous: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) params: vec4<f32>,
    @location(4) @interpolate(flat) valid: f32,
};
struct MotionFragment {
    @builtin(position) clip: vec4<f32>,
    @location(0) current: vec4<f32>,
    @location(1) previous: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) params: vec4<f32>,
    @location(4) @interpolate(flat) valid: f32,
};
struct MotionOutput {
    @location(0) motion: vec2<f32>,
    @location(1) valid: f32,
};

@vertex
fn vs_motion(in: VsIn, @builtin(vertex_index) vertex: u32) -> MotionVertex {
    let d = depth_for_motion(in);
    let e = draw_list[in.inst];
    let k = e * 4u;
    // The affine zeros of columns 1/2 carry pose offset (-1 for rigid meshes) and
    // the page's base vertex. Restore the zeros before transforming positions.
    let pm = mat4x4<f32>(vec4<f32>(previous_models[k].xyz, 0.0), vec4<f32>(previous_models[k + 1u].xyz, 0.0), vec4<f32>(previous_models[k + 2u].xyz, 0.0), previous_models[k + 3u]);
    var old_local = in.pos;
    let pose_offset = previous_models[k + 1u].w;
    if (pose_offset >= 0.0) {
        let local_index = vertex - u32(previous_models[k + 2u].w);
        old_local = previous_positions[u32(pose_offset) + local_index].xyz;
    }
    let m = model_matrix(e);
    let world = m * vec4<f32>(in.pos, 1.0) + vec4<f32>(tree_sway(in.pos, m), 0.0);
    let old_world = pm * vec4<f32>(old_local, 1.0) + vec4<f32>(previous_tree_sway(old_local, pm), 0.0);
    var out: MotionVertex;
    out.clip = d.clip;
    // Geometric projection, without the depth-only pulls used to order road decals.
    out.current = camera.view_proj * world;
    out.previous = temporal.previous_vp * old_world;
    out.uv = d.uv;
    out.params = d.params;
    out.valid = previous_models[k].w * temporal.size.z;
    return out;
}

fn projected_motion(current: vec4<f32>, previous: vec4<f32>, valid: f32) -> MotionOutput {
    var out: MotionOutput;
    out.motion = vec2<f32>(0.0);
    out.valid = 0.0;
    if (valid < 0.5 || current.w <= 1e-5 || previous.w <= 1e-5
        || !all_finite(current.xyz) || !all_finite(previous.xyz)
        || !all_finite(vec3<f32>(current.w, previous.w, 0.0))) {
        return out;
    }
    let v = (previous.xy / previous.w - current.xy / current.w) * temporal.size.xy * vec2<f32>(0.5, -0.5);
    // Half floats have a finite range. Invalid projection history must not create NaNs.
    if (all_finite(vec3<f32>(v, 0.0)) && all(abs(v) < vec2<f32>(32000.0))) {
        out.motion = v;
        out.valid = 1.0;
    }
    return out;
}

@fragment
fn fs_motion_opaque(in: MotionFragment) -> MotionOutput {
    return projected_motion(in.current, in.previous, in.valid);
}

@fragment
fn fs_motion_cutout(in: MotionFragment) -> MotionOutput {
    cutout_coverage(in.uv, in.params, camera.lod.x);
    return projected_motion(in.current, in.previous, in.valid);
}

@fragment
fn fs_motion_transmap(in: MotionFragment) -> MotionOutput {
    // A translucent pixel mixes depths and has no single certified velocity. Mark its
    // contribution invalid instead of leaving the background's valid motion there.
    var alpha = diffuse_border(textureSampleBias(t_diffuse, s_diffuse, tex_address(in.uv), camera.lod.x), in.uv).a;
    if (material.params.z > 0.5) {
        let tm = sample_transmap_bias(tex_address(in.uv - in.params.zw), camera.lod.x);
        alpha = select(1.0, tm.a, material.params.w > 0.5);
    }
    alpha = alpha * in.params.x * material.color.a;
    if (alpha < 0.002) { discard; }
    if (alpha < 0.99) { return projected_motion(in.current, in.previous, 0.0); }
    return projected_motion(in.current, in.previous, in.valid);
}
