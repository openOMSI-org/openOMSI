@group(0) @binding(0) var<uniform> camera: Camera;
// Whether this pipeline draws alpha-tested materials.
// Opaque pipelines set the discard overrides false: a fragment
// function that can discard turns the GPU's early depth test off - on Apple's GPUs the
// hidden surface removal as well - for everything it draws, and the heavy shading then
// ran for every covered layer of the city, not once per pixel.
override ALPHA_TEST: bool = true;
// Multisampled cutout pipelines can turn filtered alpha directly into sample coverage.
override ALPHA_TO_COVERAGE: bool = false;
@group(0) @binding(5) var t_shadow: texture_depth_2d;
@group(0) @binding(6) var s_shadow: sampler_comparison;
@group(0) @binding(7) var t_shadow_far: texture_depth_2d;
@group(0) @binding(8) var t_ao: texture_2d<f32>;
@group(0) @binding(9) var s_ao: sampler;

// Enhanced+: the traced lighting at a pixel (rt.wgsl, full size; camera.clouds.w 2): x the
// ambient occlusion, z the sun's visibility (< 0 where it was not traced), w 1 where the
// texel lies at this surface's depth (the pixel's own or a neighbour's; 0: the surface is
// not in the depth prepass - a pane, a blended layer - and the shadow map stands in).
fn rt_at(frag: vec2<f32>, world: vec3<f32>) -> vec4<f32> {
    if (camera.clouds.w < 1.5) {
        return vec4<f32>(1.0, 0.0, -1.0, 0.0);
    }
    let size = vec2<i32>(textureDimensions(t_ao));
    let z = (camera.view_proj * vec4<f32>(world, 1.0)).w;
    let tol = 0.03 + 0.01 * z;
    let px = vec2<i32>(frag);
    var best = vec4<f32>(1.0, 0.0, -1.0, 0.0);
    var best_d = tol;
    for (var k = 0; k < 5; k = k + 1) {
        var o = vec2<i32>(0, 0);
        switch k {
            case 1: { o = vec2<i32>(1, 0); }
            case 2: { o = vec2<i32>(-1, 0); }
            case 3: { o = vec2<i32>(0, 1); }
            case 4: { o = vec2<i32>(0, -1); }
            default: {}
        }
        let s = textureLoad(t_ao, clamp(px + o, vec2<i32>(0), size - vec2<i32>(1)), 0);
        let d = abs(s.g - z);
        if (s.g > 0.0 && d < best_d) {
            best_d = d;
            best = vec4<f32>(s.r, s.g, s.b, 1.0);
        }
        if (k == 0 && d < tol * 0.3) {
            break;
        }
    }
    return best;
}

// The ambient occlusion at a pixel of the full picture. It is worked out at half size, and
// a plain bilinear lookup blended the occlusion of the ground behind an edge with that of
// the object in front: every wheel, pole and kerb stood in a pale outline on the shaded
// ground under the bus. Of the four half-size texels around the pixel only those at the
// pixel's own depth count (a depth-aware upsample); none of them: the closest in depth.
// The light the surfaces round a point send back to it (irradiance, rgb), packed by
// ssao.wgsl `bounce_pack` in the AO texture's b and a.
fn bounce_unpack(t: vec4<f32>) -> vec3<f32> {
    let y = t.b;
    let iu = floor(t.a / 32.0 + 0.001);
    let iv = t.a - iu * 32.0;
    let r = iu / 31.0 * 2.0 * y;
    let b = iv / 31.0 * 2.0 * y;
    let g = max((y - 0.2126 * r - 0.0722 * b) / 0.7152, 0.0);
    return vec3<f32>(r, g, b);
}

// The occlusion (x) and the bounced light (yzw) at a pixel, upsampled by depth as `ao_at`.
fn ao_gi_at(frag: vec2<f32>, world: vec3<f32>) -> vec4<f32> {
    if (camera.clouds.w > 1.5) {
        let t = rt_at(frag, world);
        return vec4<f32>(select(t.x, 1.0, t.w < 0.5), 0.0, 0.0, 0.0);
    }
    let size = vec2<i32>(textureDimensions(t_ao));
    let z = (camera.view_proj * vec4<f32>(world, 1.0)).w;
    let f = frag * 0.5 - vec2<f32>(0.5);
    let base = vec2<i32>(floor(f));
    let fr = f - floor(f);
    let tol = 0.04 + 0.015 * z;
    var sum = 0.0;
    var wsum = 0.0;
    var best = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    var best_d = 1e9;
    for (var j = 0; j < 2; j = j + 1) {
        for (var i = 0; i < 2; i = i + 1) {
            let c = clamp(base + vec2<i32>(i, j), vec2<i32>(0), size - vec2<i32>(1));
            let s = textureLoad(t_ao, c, 0);
            let bw = select(1.0 - fr.x, fr.x, i == 1) * select(1.0 - fr.y, fr.y, j == 1);
            let dz = abs(s.g - z);
            let w = bw * (1.0 - smoothstep(0.0, tol, dz)) + 1e-4 * bw;
            sum = sum + s.r * w;
            wsum = wsum + w;
            if (dz < best_d) {
                best_d = dz;
                best = s;
            }
        }
    }
    // no AO sample at this surface's depth: the surface is not in the depth prepass (a
    // door or panel drawn in front of what the AO saw) - the AO behind it must not show
    if (best_d > tol * 1.5) {
        return vec4<f32>(1.0, 0.0, 0.0, 0.0);
    }
    // (the bounced light is smooth already - the blur pass averaged it - so the tap at this
    // surface's depth is enough)
    return vec4<f32>(select(best.r, sum / wsum, wsum > 0.02), bounce_unpack(best));
}

fn ao_at(frag: vec2<f32>, world: vec3<f32>) -> f32 {
    if (camera.clouds.w > 1.5) {
        let t = rt_at(frag, world);
        return select(t.x, 1.0, t.w < 0.5);
    }
    let size = vec2<i32>(textureDimensions(t_ao));
    let z = (camera.view_proj * vec4<f32>(world, 1.0)).w;
    let f = frag * 0.5 - vec2<f32>(0.5);
    let base = vec2<i32>(floor(f));
    let fr = f - floor(f);
    let tol = 0.04 + 0.015 * z;
    var sum = 0.0;
    var wsum = 0.0;
    var best = 1.0;
    var best_d = 1e9;
    for (var j = 0; j < 2; j = j + 1) {
        for (var i = 0; i < 2; i = i + 1) {
            let c = clamp(base + vec2<i32>(i, j), vec2<i32>(0), size - vec2<i32>(1));
            let s = textureLoad(t_ao, c, 0);
            let bw = select(1.0 - fr.x, fr.x, i == 1) * select(1.0 - fr.y, fr.y, j == 1);
            let dz = abs(s.g - z);
            let w = bw * (1.0 - smoothstep(0.0, tol, dz)) + 1e-4 * bw;
            sum = sum + s.r * w;
            wsum = wsum + w;
            if (dz < best_d) {
                best_d = dz;
                best = s.r;
            }
        }
    }
    // no AO sample at this surface's depth: the surface is not in the depth prepass (a
    // door or panel drawn in front of what the AO saw) - the AO behind it must not show
    if (best_d > tol * 1.5) {
        return 1.0;
    }
    return select(best, sum / wsum, wsum > 0.02);
}
// the tile light maps (`.map.LM.bmp`) of the 5x5 tiles around the camera, north up, and where
// they lie: x, y the south-west corner (render-origin relative), z the side, w 1 when set
@group(0) @binding(18) var t_lmap: texture_2d<f32>;
@group(0) @binding(19) var<uniform> lmap: vec4<f32>;

// The tile light map's light at a world point (black outside the loaded square).
fn light_map_at(p: vec3<f32>) -> vec3<f32> {
    if (lmap.w < 0.5) {
        return vec3<f32>(0.0);
    }
    let uv = vec2<f32>((p.x - lmap.x) / lmap.z, 1.0 - (p.y - lmap.y) / lmap.z);
    let c = textureSampleLevel(t_lmap, s_ao, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb;
    let inside = uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0;
    return select(vec3<f32>(0.0), c, inside);
}

// A material lit at night by the tile light map (`[LightMapMapping]`, the splines): the
// unlit flag carries 0.35.
fn light_map_mapped(m: vec4<f32>) -> bool {
    return m.y > 0.3 && m.y < 0.45;
}
// (the model matrices as their columns, four vec4 each: some phone GPUs (Mali, Adreno) read an
// array of matrices from a storage buffer wrongly, and every mesh came out flat and far away)
@group(0) @binding(1) var<storage, read> models: array<vec4<f32>>;
// (the first column's w, 0 in an affine matrix, carries the draw's order level: see
// `draw_order_level` and `scene_clip`)
fn model_matrix(e: u32) -> mat4x4<f32> {
    let k = e * 4u;
    return mat4x4<f32>(vec4<f32>(models[k].xyz, 0.0), models[k + 1u], models[k + 2u], models[k + 3u]);
}
// x: alpha multiplier, y: visible (0/1), zw: uv offset
@group(0) @binding(2) var<storage, read> inst_params: array<vec4<f32>>;
// the frame's draw list: the per-draw entry of each drawn instance. Draws of the same mesh
// and material are batched, so the instance index points into this list, not at an entry.
@group(0) @binding(10) var<storage, read> draw_list: array<u32>;
struct PointLight {
    pos: vec4<f32>,   // xyz relative to the render origin, w radius (0: an enhanced-only light)
    color: vec4<f32>, // rgb, w intensity
    dir: vec4<f32>,   // spot direction, w cosine of the outer cone (< -1.5: a point light)
    extra: vec4<f32>, // enhanced path: cosine of the inner cone, core radius, beam gain, radius
};
@group(0) @binding(3) var<storage, read> lights: array<PointLight>;
// per cell CELL_CAP light indices, 0xffffffff = empty
@group(0) @binding(4) var<storage, read> grid: array<u32>;
const CELL_CAP: u32 = 32u;
// The cutout of `[matl_alpha] 1`: Omsi.exe sets ALPHAREF 0x80 with ALPHAFUNC GREATER, so a
// texel of alpha 128 is thrown away. Painters' window layers (a bus's glass unwrapped on its
// own texture to draw on) mark the clear glass with exactly 128: cut below one half, those
// texels stayed and every window was the layer's black.
const ALPHA_REF: f32 = 128.5 / 255.0;

@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
@group(1) @binding(1) var s_diffuse: sampler;
struct MaterialParams {
    color: vec4<f32>,
    // x: alpha mode (0 opaque, 1 test, 2 blend), y: unlit flag, z: has transmap, w: transmap uses alpha
    params: vec4<f32>,
    // x: terrain material (uv in tile space), y: detail texture repeats per tile (0 = none),
    // z: ground texture repeats per tile, w: has nightmap
    extra: vec4<f32>,
    // x: has lightmap, y: envmap factor, z: moisture, w: has [matl_envmap_mask]
    params2: vec4<f32>,
    // emissive colour (the o3d material's or a [matl_allcolor]'s)
    emissive: vec4<f32>,
    // specular colour and power of the D3D material (power in w; 0 = no highlight)
    specular: vec4<f32>,
    // x: [matl_bumpmap] factor, y: has a bump map, z/w: noZwrite/noZcheck
    bump: vec4<f32>,
    // the PBR set beside the diffuse texture: x normal map, y occlusion, z roughness,
    // w metalness (1 = the map has it; see t_pbr_normal / t_pbr_orm)
    pbr: vec4<f32>,
    // x: one of the bus's own screens (the enhanced glow and FXAA leave it alone)
    flags: vec4<f32>,
    // rgb: the D3D material's ambient colour, which takes the ambient light (C)
    ambient: vec4<f32>,
    // Windy trees: x 1 for foliage that sways, y the height (mesh units) of its pivot - where
    // the crown leaves the trunk, nothing below it moves - z the crown's top, w how much the
    // tree gives to the wind (1 a broadleaf, less a conifer)
    sway: vec4<f32>,
    // Window wetness mask in mesh X/Z: origin, inverse size; zero disables it.
    wipe_bounds: vec4<f32>,
    // x: [matl_glow] strength (0 = the keyword is not there; the material is its own light -
    // its `t_light` is a greyscale mask of how much shines where, its colour is its own, see
    // the enhanced pass), y an LED panel's share of the `Led glow` ([led_glow_effect],
    // 1 without it), z/w free
    glow: vec4<f32>,
};
@group(1) @binding(2) var<uniform> material: MaterialParams;
@group(1) @binding(3) var t_trans: texture_2d<f32>;
@group(1) @binding(4) var t_night: texture_2d<f32>;
@group(1) @binding(5) var t_light: texture_2d<f32>;
@group(1) @binding(6) var t_env: texture_2d<f32>;
@group(1) @binding(7) var t_envmask: texture_2d<f32>;
@group(1) @binding(8) var t_bump: texture_2d<f32>;
@group(1) @binding(9) var t_pbr_normal: texture_2d<f32>;
@group(1) @binding(10) var t_pbr_orm: texture_2d<f32>;
@group(1) @binding(11) var s_tile: sampler;

// Paint/cut masks and night light maps cover one tile. Wrapping at its edge blends in
// the opposite edge of the SAME tile, opening grass seams even when adjacent masks
// agree. The diffuse/detail textures still repeat through s_diffuse.
fn sample_transmap(uv: vec2<f32>) -> vec4<f32> {
    return sample_transmap_bias(uv, camera.lod.x);
}

// (`bias`: FSR 1's mip bias for the picture, `camera.lod`; 0 for the shadow and rain maps,
// drawn at their own size)
fn sample_transmap_bias(uv: vec2<f32>, bias: f32) -> vec4<f32> {
    if (material.extra.x > 0.5 || material.wipe_bounds.z != 0.0) {
        return textureSampleBias(t_trans, s_tile, uv, bias);
    }
    return textureSampleBias(t_trans, s_diffuse, uv, bias);
}

fn sample_nightmap(uv: vec2<f32>) -> vec4<f32> {
    if (material.extra.x > 0.5) {
        return textureSampleBias(t_night, s_tile, uv, camera.lod.x);
    }
    return textureSampleBias(t_night, s_diffuse, uv, camera.lod.x);
}

// Some vehicle lamps use a clear diffuse placeholder and put the entire lit
// picture, including its silhouette, in the switched night texture. A transmap
// still owns coverage when the model explicitly supplies one.
fn night_coverage(alpha: f32, night_alpha: f32) -> f32 {
    return select(alpha, night_alpha,
        (u32(material.params2.w + 0.5) & 16u) != 0u && material.params.z < 0.5);
}

// The reflection mask of a [matl_envmap] material: the alpha of its [matl_envmap_mask]
// texture when it has one, else the diffuse texture's alpha - which reads 1 for a texture
// without an alpha channel (a 24-bit bitmap, DXT1, a JPEG), so its factor alone decides.
fn has_env_mask() -> bool {
    return (u32(material.params2.w + 0.5) & 1u) != 0u;
}

// [matl_transmap] was given (its file there or not: see MaterialExtra::transmap_declared)
fn has_transmap_declared() -> bool {
    return (u32(material.params2.w + 0.5) & 2u) != 0u;
}

// [matl_texadress_border]: where the diffuse texture's coordinates leave [0, 1] (a roller
// blind's band scrolled away by [texcoordtransX/Y]) Direct3D reads the border colour, not
// the texture's edge (the sampler only clamps). Its rgb comes packed in flags.z.
fn diffuse_border(tex: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let p = u32(material.flags.z + 0.5);
    let border = vec4<f32>(
        f32((p >> 16u) & 0xffu) / 255.0,
        f32((p >> 8u) & 0xffu) / 255.0,
        f32(p & 0xffu) / 255.0,
        material.flags.w,
    );
    let outside = any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0));
    return select(tex, border, material.flags.y > 0.5 && material.flags.y < 1.5 && outside);
}

// [matl_texadress_mirroronce]: Direct3D mirrors the coordinates once about 0 and clamps
// them beyond; the clamping sampler does the rest.
fn tex_address(uv: vec2<f32>) -> vec2<f32> {
    return select(uv, abs(uv), material.flags.y > 1.5);
}

fn reflection_mask(uv: vec2<f32>, diffuse_a: f32) -> f32 {
    let mask = textureSampleBias(t_envmask, s_diffuse, uv, camera.lod.x).a;
    return select(diffuse_a, mask, has_env_mask());
}

// The D3DCOLOR Omsi.exe packs for its environment stage (0x7ff8ed): each channel the
// [matl_envmap] factor x 255 x its light (at most 1), truncated, and OR-ed together shifted
// into place without saturation - a factor over 1 spills into the neighbouring channel
// exactly as there (a factor of 10 at full light reads almost white).
fn omsi_texture_factor(factor: f32, light: vec3<f32>) -> vec3<f32> {
    let m = min(max(light, vec3<f32>(0.0)), vec3<f32>(1.0)) * max(factor, 0.0) * 255.0;
    let r = u32(m.r);
    let g = u32(m.g);
    let b = u32(m.b);
    let packed = (0xffu << 24u) | (r << 16u) | (g << 8u) | b;
    return vec3<f32>(f32((packed >> 16u) & 0xffu), f32((packed >> 8u) & 0xffu), f32(packed & 0xffu)) / 255.0;
}

// [matl_bumpmap]: Direct3D's bump-mapped environment stage moves the sphere-map lookup by
// a du/dv map times the factor. Every bump map in the stock and mod content is a grey
// height map, so the du/dv map has to be its slope; it is taken here as the forward
// differences of the height, a byte of height difference counting as a signed-byte step
// (1/127, hence the 2). The stock bodies' noise (factor 0.05-0.1) makes their reflections a
// little wavy, and the panel lines of the O530 Facelift's map (factor 1) break its
// reflection at every seam.
fn bump_offset(uv: vec2<f32>) -> vec2<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(t_bump));
    let h = textureSampleBias(t_bump, s_diffuse, uv, camera.lod.x).a;
    let hx = textureSampleBias(t_bump, s_diffuse, uv + vec2<f32>(texel.x, 0.0), camera.lod.x).a;
    let hy = textureSampleBias(t_bump, s_diffuse, uv + vec2<f32>(0.0, texel.y), camera.lod.x).a;
    return clamp(vec2<f32>(h - hx, h - hy) * 2.0, vec2<f32>(-1.0), vec2<f32>(1.0)) * material.bump.x;
}

