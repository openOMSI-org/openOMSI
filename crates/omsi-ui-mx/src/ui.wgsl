// omsi-ui: shapes, text and icons in pixels, ribbons and markers in a 3D world.
struct U {
    view_proj: mat4x4<f32>,
    viewport: vec4<f32>,   // x, y, w, h (target pixels) the world is drawn into
    target_size: vec4<f32>, // w, h of the target, 0, 0
    clip: vec4<f32>,       // x0, y0, x1, y1: everything outside fades out
    params: vec4<f32>,     // clip corner radius, opacity, metres per pixel per unit depth, 0
};
@group(0) @binding(0) var<uniform> u: U;
@group(1) @binding(0) var t_img: texture_2d<f32>;
@group(1) @binding(1) var s_img: sampler;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) ext: vec2<f32>,
    @location(2) width: vec2<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) color: vec4<f32>,
    @location(5) mode: vec2<f32>,
};
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) tex: f32,
};

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@vertex
fn vs_main(v: VIn) -> VOut {
    var o: VOut;
    if (v.mode.x < 0.5) {
        // pixels of the target, whatever the viewport
        let p = v.pos.xy + v.ext * v.width.x;
        let q = (p - u.viewport.xy) / u.viewport.zw;
        o.clip = vec4<f32>(q.x * 2.0 - 1.0, 1.0 - q.y * 2.0, 0.0, 1.0);
    } else {
        let c = u.view_proj * vec4<f32>(v.pos, 1.0);
        // the larger of the width in metres and in pixels at this depth
        let mpp = max(c.w, 0.01) * u.params.z;
        let w = max(v.width.x, v.width.y * mpp);
        // solid shapes stand `target_size.z` times as tall; a shadow's corner is moved by its
        // offset (in `uv`) as far
        let k = u.target_size.z;
        var base = v.pos;
        if (v.mode.x > 1.5 && v.mode.x < 2.5) {
            base = vec3<f32>(v.pos.xy, v.pos.z * k);
        } else if (v.mode.x > 2.5) {
            base = vec3<f32>(v.pos.xy + v.uv * k, v.pos.z);
        }
        let wp = base + vec3<f32>(v.ext * w, 0.0);
        o.clip = u.view_proj * vec4<f32>(wp, 1.0);
        // mode 1: flat on the map (no depth); 2: a solid shape at its own depth; 3: a shadow
        // on the ground (the stencil keeps two shadows from darkening it twice)
        if (v.mode.x < 1.5 || v.mode.x > 2.5) {
            o.clip.z = 0.5 * o.clip.w;
        }
    }
    o.uv = v.uv;
    o.color = vec4<f32>(to_linear(v.color.rgb), v.color.a);
    o.tex = v.mode.y;
    return o;
}

fn rounded_mask(p: vec2<f32>) -> f32 {
    let r = u.params.x;
    let lo = u.clip.xy;
    let hi = u.clip.zw;
    let c = (lo + hi) * 0.5;
    let half = (hi - lo) * 0.5;
    let q = abs(p - c) - half + vec2<f32>(r);
    let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    return clamp(0.5 - d, 0.0, 1.0);
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    // (sampled for every fragment: a sample in a branch has no derivatives)
    let t = textureSample(t_img, s_img, i.uv);
    var c = i.color;
    if (i.tex > 0.5) {
        c = vec4<f32>(c.rgb * t.rgb, c.a * t.a);
    }
    let a = c.a * rounded_mask(i.clip.xy) * u.params.y;
    return vec4<f32>(c.rgb * a, a);
}
