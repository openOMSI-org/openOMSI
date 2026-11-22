// Enhanced+ (rt_tex.rs): a traced mesh's triangles' texture coordinates, for the reflections'
// hits. Per triangle its three corners' coordinates less an integer part they share, as half
// floats (exact enough over a few repeats of a texture), and that integer part (two i16).
struct Job {
    first_vertex: u32,
    first_index: u32,
    tris: u32,
    at: u32,
};

// the mesh page's vertices (8 floats each: position, normal, uv) and indices
@group(0) @binding(0) var<storage, read> uv_verts: array<f32>;
@group(0) @binding(1) var<storage, read> uv_index: array<u32>;
@group(0) @binding(2) var<storage, read_write> uv_out: array<vec4<u32>>;
@group(0) @binding(3) var<uniform> job: Job;

@compute @workgroup_size(64)
fn cs_tri_uv(@builtin(global_invocation_id) gid: vec3<u32>) {
    // (rows of 65535 groups of 64)
    let t = gid.x + gid.y * 4194240u;
    if (t >= job.tris) {
        return;
    }
    var uv: array<vec2<f32>, 3>;
    for (var k = 0u; k < 3u; k++) {
        let v = (job.first_vertex + uv_index[job.first_index + t * 3u + k]) * 8u;
        uv[k] = vec2<f32>(uv_verts[v + 6u], uv_verts[v + 7u]);
    }
    let base = clamp(floor(min(min(uv[0], uv[1]), uv[2])), vec2<f32>(-32768.0), vec2<f32>(32767.0));
    let b = vec2<i32>(base);
    uv_out[job.at + t] = vec4<u32>(
        pack2x16float(uv[0] - base),
        pack2x16float(uv[1] - base),
        pack2x16float(uv[2] - base),
        (u32(b.x) & 0xffffu) | (u32(b.y) << 16u),
    );
}
