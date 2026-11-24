// The crop and the eye's picture and sampler in mX's heap, in the immediates.
struct Params {
    crop: vec2<f32>,
    eye: u32,
    linear_sampler: u32,
};

var<immediate> params: Params;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vertex_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    let x = f32(i32(index & 1u) * 4 - 1);
    let y = f32(i32(index >> 1u) * 4 - 1);
    var out: VertexOutput;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fragment_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = (in.uv - vec2<f32>(0.5)) * params.crop.xy + vec2<f32>(0.5);
    return vec4<f32>(textureSample(mx_tex2d[params.eye], mx_samplers[params.linear_sampler], uv).rgb, 1.0);
}
