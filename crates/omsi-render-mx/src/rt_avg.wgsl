// Enhanced+ (rt.rs, rt_tex.rs): each texture's mean colour (sampled 8 x 8 from a small level)
// and its small copy in the reflections' atlas, for the hits
@group(0) @binding(0) var avg_tex: texture_2d<f32>;
@group(0) @binding(1) var avg_samp: sampler;
struct AvgOut {
    c: vec4<f32>,
    tile: vec4<f32>,
};
@group(0) @binding(2) var<storage, read_write> avg_out: AvgOut;
@group(0) @binding(3) var atlas: texture_storage_2d<rgba8unorm, write>;
// xy the texture's tile's corner in the atlas (texels), z 1 where it has a tile
@group(0) @binding(4) var<uniform> tile: vec4<f32>;
var<workgroup> avg_sum: array<vec4<f32>, 64>;

// (the atlas is sampled as sRGB: filtered in linear light, stored with the texture's own
// precision in the darks)
fn srgb_encode(c: vec3<f32>) -> vec3<f32> {
    let x = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    return select(1.055 * pow(x, vec3<f32>(1.0 / 2.4)) - 0.055, x * 12.92, x <= vec3<f32>(0.0031308));
}

@compute @workgroup_size(8, 8)
fn cs_tex_avg(@builtin(local_invocation_id) lid: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let levels = f32(textureNumLevels(avg_tex));
    let lod = max(levels - 4.0, 0.0);
    let uv = (vec2<f32>(lid.xy) + vec2<f32>(0.5)) / 8.0;
    avg_sum[li] = textureSampleLevel(avg_tex, avg_samp, uv, lod);
    // The tile: 30 x 30 texels of the level nearest that size, inside a texel of border that
    // repeats the opposite edge (filtering across a tile's edge wraps as the texture does).
    if (tile.z > 0.5) {
        let size = vec2<f32>(textureDimensions(avg_tex));
        let tlod = clamp(log2(max(size.x, size.y) / 30.0), 0.0, levels - 1.0);
        for (var k = li; k < 1024u; k += 64u) {
            let q = vec2<u32>(k % 32u, k / 32u);
            let c = (vec2<f32>((q + vec2<u32>(29u)) % vec2<u32>(30u)) + vec2<f32>(0.5)) / 30.0;
            let s = textureSampleLevel(avg_tex, avg_samp, c, tlod);
            textureStore(atlas, vec2<i32>(tile.xy) + vec2<i32>(q), vec4<f32>(srgb_encode(s.rgb), s.a));
        }
    }
    workgroupBarrier();
    if (li == 0u) {
        var s = vec4<f32>(0.0);
        for (var i = 0u; i < 64u; i++) {
            s += avg_sum[i];
        }
        avg_out.c = s / 64.0;
        avg_out.tile = tile;
    }
}
