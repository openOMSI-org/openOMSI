// A textured UI quad projected into each headset eye from one place in space.
struct Quad {
    top_left: vec4<f32>,
    top_right: vec4<f32>,
    bottom_right: vec4<f32>,
    bottom_left: vec4<f32>,
    opts: vec4<f32>,
};

@group(0) @binding(0) var<uniform> q: Quad;
@group(0) @binding(1) var t_img: texture_2d<f32>;
@group(0) @binding(2) var s_img: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
    let indices = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
    let corners = array<vec4<f32>, 4>(q.top_left, q.top_right, q.bottom_right, q.bottom_left);
    let coords = array<vec2<f32>, 4>(vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
    let index = indices[vid];
    var out: VsOut;
    out.clip = corners[index];
    out.uv = coords[index];
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let sample = textureSample(t_img, s_img, in.uv);
    if (q.opts.x > 0.5) {
        return sample;
    }
    return vec4<f32>(sample.rgb * sample.a, sample.a);
}
