// Enhanced+: what the ray-traced passes share (rt.rs): the parameters, the scene's
// acceleration structure with its geometry records, the ray queries.
enable wgpu_ray_query;

struct RtParams {
    inv_view_proj: mat4x4<f32>,
    view_proj: mat4x4<f32>,
    prev_view_proj: mat4x4<f32>,
    // xyz the camera (render-origin relative), w the frame number
    eye: vec4<f32>,
    // xyz towards the sun, w the tangent of the sun disc's radius as traced (0: no sun)
    sun: vec4<f32>,
    // width, height, 1 / width, 1 / height
    size: vec4<f32>,
    // xy the projection's depth coefficients, z the traced range (m), w the AO radius (m)
    proj: vec4<f32>,
    // x history usable, y the history's least weight, z debug view, w frames accumulated at most
    temporal: vec4<f32>,
    // the player's vehicle now: xyz its origin, w its heading (rad); and a frame ago (in this
    // frame's render-origin coordinates); its box half extents (w: 1 = there is one) and centre
    vehicle_now: vec4<f32>,
    vehicle_prev: vec4<f32>,
    vehicle_box: vec4<f32>,
    vehicle_centre: vec4<f32>,
    // the sky light for the reflections' hits: xyz the mean sky irradiance / pi (pre-exposed),
    // w the probe's scale
    sky: vec4<f32>,
    // xyz the sun's radiance at the hits (pre-exposed), w the probe's mip count
    sun_light: vec4<f32>,
};

struct Record {
    tex: u32,
    trans: u32,
    // bit 0: alpha-tested (stochastic), bit 1: unlit (shows its own colour), bit 2: the
    // coverage is the transmap's alpha, bits 3-4 how its textures are addressed (rt_tex.rs
    // `address_mode`), bit 5: opaque all the same (a transmap whose alpha is not taken),
    // bit 6: a see-through pane
    flags: u32,
    // its first triangle's texture coordinates (rt_tex.rs; 0xffffffff: none yet)
    uvs: u32,
    color: vec4<f32>,
    // the diffuse texture's coordinates: (uv + yz) * x (a texcoordtrans offset, the ground's
    // repeat)
    uvt: vec4<f32>,
};

// One texture's mean colour and its tile in the reflections' atlas (xy its corner, z 1 when
// it is there), 256 bytes apart (written with a dynamic offset, see rt.rs).
struct TexAvg {
    c: vec4<f32>,
    tile: vec4<f32>,
    pad: array<vec4<f32>, 14>,
};

@group(0) @binding(0) var<uniform> p: RtParams;
@group(0) @binding(1) var acc: acceleration_structure;
@group(0) @binding(2) var t_depth: texture_depth_2d;
@group(0) @binding(3) var<storage, read> records: array<Record>;
@group(0) @binding(4) var<storage, read> tex_avg: array<TexAvg>;

// instance masks: solid shadow casters, solid meshes seen (by the occlusion, the
// reflections), cut-out ones seen, panes
const MASK_SHADOW: u32 = 0x01u;
const MASK_SEEN: u32 = 0x06u;
// see-through panes (their shadow: part of the sun gets through)
const MASK_GLASS: u32 = 0x08u;
// ... and the same panes as the reflections' rays meet them
const MASK_GLASS_SEEN: u32 = 0x10u;
// Ray flags. (Not called RAY_FLAG_*: Direct3D's HLSL has those names built in, and naga
// writes the constants out under their own names - DXC refused the redefinition, the ray
// tracing's pipelines were never made on Windows, and every frame was thrown away.)
const RT_FORCE_OPAQUE: u32 = 0x01u;
const RT_FIRST_HIT: u32 = 0x04u;
const PI: f32 = 3.14159265;

fn world_pos(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let h = p.inv_view_proj * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    return h.xyz / h.w;
}
fn linear_depth(depth: f32) -> f32 {
    return p.proj.y / max(depth + p.proj.x, 1e-7);
}

// The surface's normal from the depth around it: of the two neighbours on each axis the one
// nearer in depth, so that an edge does not tilt it; turned towards the camera.
fn depth_normal(px: vec2<i32>, world: vec3<f32>, depth: f32) -> vec3<f32> {
    let size = vec2<i32>(textureDimensions(t_depth));
    var axes: array<vec3<f32>, 2>;
    for (var i = 0; i < 2; i++) {
        let axis = select(vec2<i32>(1, 0), vec2<i32>(0, 1), i == 1);
        let plus = clamp(px + axis, vec2<i32>(0), size - vec2<i32>(1));
        let minus = clamp(px - axis, vec2<i32>(0), size - vec2<i32>(1));
        let dp = textureLoad(t_depth, plus, 0);
        let dm = textureLoad(t_depth, minus, 0);
        let forward = dp > 0.0 && (dm <= 0.0 || abs(dp - depth) < abs(dm - depth));
        let q = select(minus, plus, forward);
        let d = select(dm, dp, forward);
        axes[i] = (world_pos((vec2<f32>(q) + vec2<f32>(0.5)) * p.size.zw, d) - world) * select(-1.0, 1.0, forward);
    }
    var n = cross(axes[0], axes[1]);
    if (dot(n, n) < 1e-14) {
        n = p.eye.xyz - world;
    }
    n = normalize(n);
    return select(-n, n, dot(n, p.eye.xyz - world) >= 0.0);
}

// Interleaved gradient noise, moved on every frame (a different sample per frame and pixel
// that spreads evenly over a few frames and pixels).
fn noise(px: vec2<f32>, k: f32) -> f32 {
    let f = p.eye.w + k * 17.0;
    let q = px + vec2<f32>(47.0, 17.0) * (f % 64.0) * 0.695;
    return fract(52.9829189 * fract(dot(q, vec2<f32>(0.06711056, 0.00583715))));
}

fn hash3(v: vec3<u32>) -> f32 {
    var x = v.x * 1664525u + v.y * 22695477u + v.z * 1013904223u;
    x = (x ^ (x >> 16u)) * 0x7feb352du;
    x = (x ^ (x >> 15u)) * 0x846ca68bu;
    x = x ^ (x >> 16u);
    return f32(x >> 8u) / 16777216.0;
}

fn basis(n: vec3<f32>) -> mat3x3<f32> {
    let s = select(-1.0, 1.0, n.z >= 0.0);
    let a = -1.0 / (s + n.z);
    let b = n.x * n.y * a;
    let t = vec3<f32>(1.0 + s * n.x * n.x * a, s * b, -s * n.x);
    let bt = vec3<f32>(b, s + n.y * n.y * a, -n.y);
    return mat3x3<f32>(t, bt, n);
}

// How much of a hit geometry's texels stop a ray: 1 for a solid one, the mean alpha of its
// cut-out texture (or transmap) for an alpha-tested one.
fn coverage(r: Record) -> f32 {
    if ((r.flags & 1u) == 0u || p.temporal.z == 3.0 || p.temporal.z == 5.0) {
        return 1.0;
    }
    if (p.temporal.z == 4.0) {
        return 0.0;
    }
    var a = 1.0;
    if ((r.flags & 4u) != 0u && r.trans != 0xffffffffu) {
        a = tex_avg[r.trans].c.a;
    } else if (r.tex != 0xffffffffu) {
        a = tex_avg[r.tex].c.a;
    }
    // (not the material's alpha: the alpha test compares the texture's own)
    return clamp(a, 0.0, 1.0);
}

struct Hit {
    t: f32,
    record: u32,
    front: bool,
    tries: u32,
};

// One ray query from `start` to `tmax`: its committed intersection. A function of its own,
// called for every ray: naga's Vulkan and Direct3D 12 code keeps a query's initialization
// tracker per function, not per loop iteration, and a query started again in a loop after
// a miss was never traced again there (each later try came back empty).
fn ray_hit(flags: u32, mask: u32, start: f32, tmax: f32, o: vec3<f32>, d: vec3<f32>) -> RayIntersection {
    var rq: ray_query;
    rayQueryInitialize(&rq, acc, RayDesc(flags, mask, start, tmax, o, d));
    rayQueryProceed(&rq);
    return rayQueryGetCommittedIntersection(&rq);
}

// The first hit along a ray within `tmax` (t < 0: none). An alpha-tested hit stops the ray by
// its coverage's chance; else the ray goes on past it (Metal's ray queries take no any-hit
// test of their own: the query is started again from there).
fn trace(o: vec3<f32>, d: vec3<f32>, tmax: f32, mask: u32, seed: vec3<u32>, solid_only: bool) -> Hit {
    var start = 0.0;
    var out: Hit;
    out.t = -1.0;
    for (var i = 0u; i < 6u; i++) {
        out.tries = i + 1u;
        let h = ray_hit(RT_FORCE_OPAQUE, mask, start, tmax, o, d);
        if (h.kind == RAY_QUERY_INTERSECTION_NONE) {
            return out;
        }
        let ri = h.instance_custom_data + h.geometry_index;
        let r = records[ri];
        if ((r.flags & 1u) == 0u || (!solid_only && hash3(seed + vec3<u32>(i * 7919u, ri, 0u)) < coverage(r))) {
            out.t = h.t;
            out.record = ri;
            out.front = h.front_face;
            return out;
        }
        start = h.t + 0.002;
    }
    return out;
}

