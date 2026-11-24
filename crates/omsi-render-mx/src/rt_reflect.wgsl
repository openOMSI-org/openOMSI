// Enhanced+: ray-traced reflections (rt.rs), after the main pass.
//
// The enhanced pass left, per pixel, the surface a reflection starts from (its normal, its
// distance from the eye, its roughness) and how much of the reflection lands on the screen
// (GBUF_FORMAT: the split sum's Fresnel and lobe integral, so that a rough dielectric shows a
// few per cent of it and glass, metal and a wet road much), and drew the picture without the
// probe's reflection there. A ray per pixel (or per REFL_DIV x REFL_DIV, from the most
// reflective of them) goes out along a direction of the surface's GGX lobe - the mirror
// direction for a smooth one - and what it meets is
// - on the screen and seen there: the picture's own colour at that place (lit, fogged,
//   reflections and all, exactly as drawn);
// - off the screen or hidden: the hit mesh's texel there (its texture's small copy in the
//   atlas, rt_tex.rs; the texture's mean colour until that is made) times its material's
//   colour, lit by the sun (a shadow ray from there) and the sky about its own normal;
// - nothing: the sky probe.
// The denoiser (rt_denoise.wgsl) gathers the rays over the frames and the pixels round each,
// and the composite adds the result, times the weight, to the picture.

// The rays' grid: one per REFL_DIV x REFL_DIV pixels (1: every pixel).
override REFL_DIV: i32 = 1;

@group(0) @binding(5) var t_gbuf: texture_2d<f32>;
@group(0) @binding(6) var t_aux: texture_2d<f32>;
@group(0) @binding(7) var t_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(8) var s_lin: sampler;
@group(0) @binding(9) var t_scene: texture_2d<f32>;
@group(0) @binding(10) var t_probe: texture_cube<f32>;
@group(0) @binding(11) var<uniform> enh: Enhanced;
@group(0) @binding(12) var t_hist: texture_2d<f32>;
// per traced triangle its texture coordinates (rt_uv.wgsl), and the textures' tiles
@group(0) @binding(13) var<storage, read> tri_uv: array<vec4<u32>>;
@group(0) @binding(14) var t_atlas: texture_2d<f32>;

// the alpha test's threshold (the main pass's `ALPHA_REF`)
const ALPHA_REF: f32 = 128.5 / 255.0;

// Texture `t`'s texel at `uv`, from its tile in the atlas, addressed by `mode` (0 repeat,
// 1 mirror, 2 clamp, 3 mirror once); w < 0 while the texture has no tile.
fn tile_texel(t: u32, uv: vec2<f32>, mode: u32, cut: bool) -> vec4<f32> {
    let tile = tex_avg[t].tile;
    if (tile.z < 0.5) {
        return vec4<f32>(-1.0);
    }
    var w = fract(uv);
    if (mode != 0u) {
        // (kept off the border, which repeats the other edge: these do not)
        w = select(select(abs(uv), uv, mode == 2u), 1.0 - abs(fract(uv * 0.5) * 2.0 - 1.0), mode == 1u);
        w = clamp(w, vec2<f32>(1.0 / 60.0), vec2<f32>(59.0 / 60.0));
    }
    if (!cut) {
        let at = (tile.xy + vec2<f32>(1.0) + w * 30.0) / vec2<f32>(textureDimensions(t_atlas));
        return textureSampleLevel(t_atlas, s_lin, at, 0.0);
    }
    // A cut-out texture's colour filtered by its coverage: the texels the alpha test throws
    // away are white or black in most leaves' textures, and filtered in as they were they
    // fringed every reflected crown with white specks.
    let f = tile.xy + vec2<f32>(0.5) + w * 30.0;
    let b = vec2<i32>(floor(f));
    let fr = f - floor(f);
    var rgb = vec3<f32>(0.0);
    var a = 0.0;
    for (var j = 0; j <= 1; j++) {
        for (var i = 0; i <= 1; i++) {
            let c = textureLoad(t_atlas, b + vec2<i32>(i, j), 0);
            let k = select(1.0 - fr.x, fr.x, i == 1) * select(1.0 - fr.y, fr.y, j == 1);
            rgb += c.rgb * c.a * k;
            a += c.a * k;
        }
    }
    let mean = tex_avg[t].c.rgb;
    return vec4<f32>(select(mean, rgb / max(a, 1e-4), a > 1e-3), a);
}

// What a hit shows of its textures: rgb the diffuse texture's colour there (white without
// one), a the coverage the alpha test takes; a < 0 while that is not known yet (no
// coordinates or no tile: the mean colour then).
fn hit_texel(r: Record, prim: u32, bary: vec2<f32>) -> vec4<f32> {
    if (r.uvs == 0xffffffffu) {
        return vec4<f32>(-1.0);
    }
    let q = tri_uv[(r.uvs + prim) * 2u];
    let base = vec2<f32>(f32(i32(q.w << 16u) >> 16u), f32(i32(q.w) >> 16u));
    let uv = base + unpack2x16float(q.x) * (1.0 - bary.x - bary.y) + unpack2x16float(q.y) * bary.x + unpack2x16float(q.z) * bary.y;
    let mode = (r.flags >> 3u) & 3u;
    var c = vec4<f32>(1.0);
    if (r.tex != 0xffffffffu) {
        c = tile_texel(r.tex, (uv + r.uvt.yz) * r.uvt.x, mode, (r.flags & 5u) == 1u);
        if (c.a < 0.0) {
            return c;
        }
    }
    if ((r.flags & 4u) != 0u && r.trans != 0xffffffffu) {
        // (the transmap without the offset and the ground's repeat, as the main pass reads it)
        let m = tile_texel(r.trans, uv, mode, false);
        if (m.a < 0.0) {
            return m;
        }
        c.a = m.a;
    }
    if ((r.flags & 32u) != 0u) {
        c.a = 1.0;
    }
    return c;
}

fn sh_irradiance(n: vec3<f32>) -> vec3<f32> {
    let c = enh.sh;
    let e = c[0].rgb * 0.282095
        + c[1].rgb * (0.488603 * n.y) + c[2].rgb * (0.488603 * n.z) + c[3].rgb * (0.488603 * n.x)
        + c[4].rgb * (1.092548 * n.x * n.y) + c[5].rgb * (1.092548 * n.y * n.z)
        + c[6].rgb * (0.315392 * (3.0 * n.z * n.z - 1.0)) + c[7].rgb * (1.092548 * n.x * n.z)
        + c[8].rgb * (0.546274 * (n.x * n.x - n.y * n.y));
    return max(e, vec3<f32>(0.0));
}

// The sky probe towards d at the blur of roughness `rough` (pre-exposed).
fn sky_probe(d: vec3<f32>, rough: f32) -> vec3<f32> {
    let lod = rough * (enh.lights.x - 1.0);
    return textureSampleLevel(t_probe, s_lin, d.xzy, lod).rgb * enh.fog_color.w;
}

fn finite(c: vec3<f32>) -> vec3<f32> {
    let e = bitcast<vec3<u32>>(c) & vec3<u32>(0x7f800000u);
    return select(vec3<f32>(0.0), clamp(c, vec3<f32>(0.0), vec3<f32>(6.0e4)), all(e != vec3<u32>(0x7f800000u)));
}

// Heitz's visible-normal sampling of the GGX lobe (in Dupuy and Benyoub's spherical-cap
// form): a microfacet normal round n as the eye at `wo` (towards the eye) sees them, for
// roughness `rough` (alpha = rough^2) and two uniform numbers. Rays reflected about it follow
// the lobe the main pass's split sum integrates, so many frames of them average to the
// same blur the sky probe gives a surface that rough, with the street in it.
fn ggx_vndf(n: vec3<f32>, wo: vec3<f32>, rough: f32, u: vec2<f32>) -> vec3<f32> {
    let a = max(rough * rough, 1e-4);
    let b = basis(n);
    let wl = vec3<f32>(dot(wo, b[0]), dot(wo, b[1]), max(dot(wo, n), 1e-4));
    let ws = normalize(vec3<f32>(wl.xy * a, wl.z));
    let phi = 2.0 * PI * u.x;
    let z = (1.0 - u.y) * (1.0 + ws.z) - ws.z;
    let st = sqrt(clamp(1.0 - z * z, 0.0, 1.0));
    let hs = vec3<f32>(st * cos(phi), st * sin(phi), z) + ws;
    let hl = normalize(vec3<f32>(hs.xy * a, max(hs.z, 1e-6)));
    return normalize(b * hl);
}

// This frame's two numbers for a pixel: a hash per pixel moved along the R2 sequence every
// frame, so that the rays of a pixel cover its lobe evenly over the frames the history keeps.
fn frame_noise(px: vec2<i32>) -> vec2<f32> {
    let h = vec2<f32>(hash3(vec3<u32>(vec2<u32>(px), 11u)), hash3(vec3<u32>(vec2<u32>(px), 23u)));
    let f = f32(u32(p.eye.w) % 1024u);
    return fract(h + f * vec2<f32>(0.7548777, 0.5698403));
}

// A unit vector folded onto the octahedron (rt_uv.wgsl) and back.
fn oct_encode(n: vec3<f32>) -> vec2<f32> {
    let q = n.xy / (abs(n.x) + abs(n.y) + abs(n.z));
    if (n.z >= 0.0) {
        return q;
    }
    return (vec2<f32>(1.0) - abs(q.yx)) * select(vec2<f32>(-1.0), vec2<f32>(1.0), q >= vec2<f32>(0.0));
}
fn oct_decode(e: vec2<f32>) -> vec3<f32> {
    var n = vec3<f32>(e, 1.0 - abs(e.x) - abs(e.y));
    let t = max(-n.z, 0.0);
    n = vec3<f32>(n.xy + select(vec2<f32>(t), vec2<f32>(-t), n.xy >= vec2<f32>(0.0)), n.z);
    return normalize(n);
}

// The surface's normal where a ray met mesh record `r`'s triangle `prim` (its corners' normals
// mixed by the barycentrics, turned into the world by the instance's transform; length 0
// while the mesh's normals are not there yet).
fn hit_normal(r: Record, prim: u32, bary: vec2<f32>, w2o: mat4x3<f32>) -> vec3<f32> {
    if (r.uvs == 0xffffffffu) {
        return vec3<f32>(0.0);
    }
    let q = tri_uv[(r.uvs + prim) * 2u + 1u];
    let nl = oct_decode(unpack2x16snorm(q.x)) * (1.0 - bary.x - bary.y)
        + oct_decode(unpack2x16snorm(q.y)) * bary.x + oct_decode(unpack2x16snorm(q.z)) * bary.y;
    // (normals go by the inverse transpose: the world-to-object matrix's rows)
    let nw = vec3<f32>(dot(w2o[0], nl), dot(w2o[1], nl), dot(w2o[2], nl));
    if (dot(nw, nw) < 1e-12) {
        return vec3<f32>(0.0);
    }
    return normalize(nw);
}

// Where a point a ray along `d` meets is on the screen: x 1 there and seen (the picture's
// colour is its own), 2 there but the picture shows something further (a gap: the ray went
// through a cut-out texel), 3 there behind something nearer - or the picture shows the other
// face of it - 0 off the screen; yz its place.
fn on_screen(pt: vec3<f32>, d: vec3<f32>) -> vec3<f32> {
    let c = p.view_proj * vec4<f32>(pt, 1.0);
    if (c.w > 0.05) {
        let uv = vec2<f32>(c.x / c.w * 0.5 + 0.5, 0.5 - c.y / c.w * 0.5);
        if (all(uv > vec2<f32>(0.002)) && all(uv < vec2<f32>(0.998))) {
            let q = vec2<i32>(uv * p.size.xy);
            let dq = textureLoad(t_depth, q, 0);
            let seen = select(1e9, linear_depth(dq), dq > 0.0);
            let tol = 0.02 * c.w + 0.08;
            if (abs(seen - c.w) < tol) {
                // (the picture shows the face turned to the camera: a ray that met the
                // other one - a car's reflection that went in through the bus's window and
                // met the back of a seat, or the bus's side from outside next to the seat
                // the camera sees from inside - is not lit as the picture there is; taken
                // as it was, every car beside the bus mirrored its red seats)
                let qc = (vec2<f32>(q) + vec2<f32>(0.5)) * p.size.zw;
                let ns = depth_normal(q, world_pos(qc, dq), dq);
                return vec3<f32>(select(3.0, 1.0, dot(ns, d) < 0.0), uv);
            }
            return vec3<f32>(select(3.0, 2.0, seen > c.w + tol), uv);
        }
    }
    return vec3<f32>(0.0);
}

// The light a reflected ray brings back from where it meets the scene (`sc` its place on
// the screen): the picture's own colour there, else the mesh lit as the main pass lights it.
fn hit_light(o: vec3<f32>, d: vec3<f32>, h: Hit, sc: vec3<f32>, texel: vec4<f32>, nh: vec3<f32>) -> vec3<f32> {
    let pt = o + d * h.t;
    // (a cut-out mesh - leaves - seen through a gap shows what lies behind it there: the
    // picture's colour all the same, the same for the rays beside it; deciding leaf or gap
    // one ray at a time sprinkled the glass with light and dark specks, #1907)
    let cut = (records[h.record].flags & 1u) != 0u;
    if (sc.x == 1.0 || (cut && sc.x == 2.0)) {
        let seen = finite(textureSampleLevel(t_scene, s_lin, sc.yz, 0.0).rgb);
        // (towards the screen's edge the picture's colour hands over to the mesh's own,
        // which is what the rays beyond the edge get: a hard seam ran across the windows
        // where the reflected street left the screen)
        let edge = min(min(sc.y, 1.0 - sc.y), min(sc.z, 1.0 - sc.z));
        let k = smoothstep(0.0, 0.08, edge);
        if (k >= 0.999) {
            return seen;
        }
        return mix(mesh_light(pt, d, h, texel, nh), seen, k);
    }
    return mesh_light(pt, d, h, texel, nh);
}

// The sky's light on a surface facing n (as the main pass's `sh_irradiance`) with what the
// clouds scatter of the sun (its `sun_scattered`, the beam's share in `enh.pbr_ext.x`).
fn sky_light(n: vec3<f32>) -> vec3<f32> {
    var e = sh_irradiance(n);
    if (enh.pbr_ext.y > 0.5 && enh.pbr_ext.x < 0.999) {
        let s = p.sun.xyz;
        let lobe = (1.0 + dot(n, s)) / (1.0 + max(s.z, 0.0));
        e = e + enh.sun.rgb * (1.0 - clamp(enh.pbr_ext.x, 0.0, 1.0)) * max(s.z, 0.0) * mix(0.5 + 0.5 * n.z, lobe, 0.4);
    }
    return e;
}

// What a ray finds off the screen or hidden there: the hit mesh's colour (its texel there,
// `hit_texel`, or its texture's mean colour, times its material's) lit as the main pass
// lights a matt surface - the sun's beam by its angle to the surface's own normal (`nh`,
// from the mesh's normals; facing the ray where they are not known yet) where a shadow ray
// finds the sun, the sky and the scattered sun over the hemisphere - and the air along the way.
fn mesh_light(pt: vec3<f32>, d: vec3<f32>, h: Hit, texel: vec4<f32>, nh: vec3<f32>) -> vec3<f32> {
    let o = pt - d * h.t;
    let r = records[h.record];
    var albedo = r.color.rgb;
    if (texel.a >= 0.0) {
        albedo = albedo * texel.rgb;
    } else if (r.tex != 0xffffffffu) {
        albedo = albedo * tex_avg[r.tex].c.rgb;
    }
    if ((r.flags & 2u) != 0u) {
        return albedo * enh.exposure.y;
    }
    // (two-sided: the face the ray met)
    var nf = -d;
    if (dot(nh, nh) > 0.5) {
        nf = select(nh, -nh, dot(nh, d) > 0.0);
    }
    var sun = vec3<f32>(0.0);
    let nl = dot(nf, p.sun.xyz);
    if (p.sun.w > 0.0 && nl > 0.0) {
        if (ray_hit(RT_FORCE_OPAQUE | RT_FIRST_HIT, MASK_SHADOW, 0.0, 1500.0, pt + nf * 0.03 - d * 0.02, p.sun.xyz).kind == RAY_QUERY_INTERSECTION_NONE) {
            let beam = select(1.0, clamp(enh.pbr_ext.x, 0.0, 1.0), enh.pbr_ext.y > 0.5);
            sun = enh.sun.rgb * beam * nl;
        }
    }
    var l = albedo / PI * (sun + sky_light(nf)) * enh.exposure.x;
    // the air along the way: from the eye to the surface and on to the hit
    let dist = length(o - p.eye.xyz) + h.t;
    let ext = exp(-(enh.fog.x + enh.fog.w) * dist);
    l = l * ext + enh.fog_color.rgb * enh.exposure.x * (1.0 - ext);
    return finite(l);
}

// The rays' second output: per ray the surface it left (x its distance from the eye, 0 where
// no ray went; y its roughness; zw its normal folded onto the octahedron), for the denoiser
// (rt_denoise.wgsl) and the composite.
@group(0) @binding(15) var t_meta_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(16) var t_meta: texture_2d<f32>;

@compute @workgroup_size(8, 8)
fn cs_reflect(@builtin(global_invocation_id) gid: vec3<u32>) {
    let hp = vec2<i32>(gid.xy);
    let hsize = vec2<i32>(textureDimensions(t_out));
    if (any(hp >= hsize)) {
        return;
    }
    let size = vec2<i32>(p.size.xy);
    // the most reflective of the pixels the ray stands for
    var px = min(hp * REFL_DIV, size - vec2<i32>(1));
    var g = vec4<f32>(0.0);
    for (var j = 0; j < REFL_DIV; j++) {
        for (var i = 0; i < REFL_DIV; i++) {
            let q = min(hp * REFL_DIV + vec2<i32>(i, j), size - vec2<i32>(1));
            let s = textureLoad(t_gbuf, q, 0);
            if (length(s.xyz) > length(g.xyz)) {
                g = s;
                px = q;
            }
        }
    }
    // (a reflection that hardly shows - a pane seen from the cab, a matt surface at a steep
    // angle - is as good from the sky probe: the composite takes that where no ray went)
    if (length(g.xyz) < 0.012 || g.w < 1e-4) {
        textureStore(t_out, hp, vec4<f32>(0.0));
        textureStore(t_meta_out, hp, vec4<f32>(0.0));
        return;
    }
    let aux = textureLoad(t_aux, px, 0);
    // (premultiplied by the coverage, gbuf.w: a pane's is 1, a surface's its weight)
    let n = normalize(g.xyz);
    let dist = aux.r / g.w;
    let rough = clamp(aux.g / g.w, 0.0, 1.0);
    let uv = (vec2<f32>(px) + vec2<f32>(0.5)) * p.size.zw;
    let view = normalize(world_pos(uv, 0.001) - p.eye.xyz);
    let pt = p.eye.xyz + view * dist;
    textureStore(t_meta_out, hp, vec4<f32>(dist, rough, oct_encode(n)));
    var d = reflect(view, n);
    // A rough surface (a satin panel, dry plaster) is no mirror: its rays follow the GGX lobe
    // of its roughness, a new one every frame, which the denoiser averages over the frames and
    // the pixels round it. What is smoother - a pane, a polished coat, a wet road, a puddle -
    // takes the one ray along the mirror direction, blurred by its lobe afterwards
    // (`GLOSSY_MIRROR`).
    if (rough > GLOSSY_MIRROR) {
        let hn = ggx_vndf(n, -view, rough, frame_noise(hp));
        let dd = reflect(view, hn);
        if (dot(dd, n) > 0.01) {
            d = dd;
        }
    }
    // (a reflection into the surface would show what is under it: bent up along it)
    let below = dot(d, n);
    if (below < 0.02) {
        d = normalize(d + n * (0.02 - below));
    }
    // (a ray that leaves the scene takes the sky probe at a little of the lobe's blur: the
    // rays themselves spread it, the rest keeps the sky's stars and edges from crawling)
    let sky_rough = select(rough, rough * 0.5, rough > GLOSSY_MIRROR);
    var l: vec3<f32>;
    // how far the ray went (the reprojection's mirror image is that far behind the surface);
    // far for the sky
    var reach = 1.0e4;
    // (OMSI_DEBUG_RT=11: what each ray found - blue the sky, green the picture, yellow a
    // gap in a cut-out on the screen, red a mesh off the screen or hidden)
    var dbg = vec3<f32>(0.0, 0.0, 1.0);
    if (dist > p.proj.z) {
        l = sky_probe(d, sky_rough);
    } else {
        let o = pt + n * (0.02 + dist * 0.0015);
        // (a cut-out mesh - leaves, a fence - is taken as solid, and the picture says where
        // its texels are: where it shows a gap there, the ray goes on)
        var start = 0.0;
        l = sky_probe(d, sky_rough);
        // A pane first: what it mirrors itself (glass at the slant a reflection meets it
        // takes about a tenth) and the inside behind it, seen through its tint. The inside
        // is lit only through the panes as well: where the picture does not show it, the
        // mean colour lit as if outdoors is dimmed by the tint once more.
        var through = 1.0;
        var pane = vec3<f32>(0.0);
        let gh = ray_hit(RT_FORCE_OPAQUE, MASK_SEEN | MASK_GLASS_SEEN, 0.0, 400.0, o, d);
        if (gh.kind != RAY_QUERY_INTERSECTION_NONE && (records[gh.instance_custom_data + gh.geometry_index].flags & 64u) != 0u) {
            pane = sky_probe(d, sky_rough) * 0.1;
            through = 0.45;
            start = gh.t + 0.01;
            reach = gh.t;
        }
        // What the ray meets, front to back: a solid mesh ends it; a cut-out one (leaves, a
        // fence) where it is not on the screen stops as much of it as its texels' filtered
        // coverage there, and the rest goes on. (Taken all or nothing at the alpha test, one
        // ray a pixel, every crown's edge in a window was a sprinkle of specks that changed
        // with each frame's ray.)
        var acc = vec3<f32>(0.0);
        var left = 1.0;
        for (var k = 0; k < 4; k++) {
            let ch = ray_hit(RT_FORCE_OPAQUE, MASK_SEEN, start, 400.0, o, d);
            if (ch.kind == RAY_QUERY_INTERSECTION_NONE) {
                break;
            }
            var h: Hit;
            h.t = ch.t;
            h.record = ch.instance_custom_data + ch.geometry_index;
            let rec = records[h.record];
            let cut = (rec.flags & 1u) != 0u;
            let sc = on_screen(o + d * h.t, d);
            let texel = hit_texel(rec, ch.primitive_index, ch.barycentrics);
            let hidden = sc.x != 1.0 && sc.x != 2.0;
            var a = 1.0;
            if (cut && hidden) {
                // (its texels not known yet: as much as they cover on average)
                a = select(mix(coverage(rec), 1.0, 0.5), smoothstep(ALPHA_REF - 0.3, ALPHA_REF + 0.2, texel.a), texel.a >= 0.0);
            }
            if (a > 0.01) {
                let nh = hit_normal(rec, ch.primitive_index, ch.barycentrics, ch.world_to_object);
                var lh = hit_light(o, d, h, sc, texel, nh);
                if (through < 1.0 && sc.x != 1.0) {
                    lh = lh * through;
                }
                acc += lh * a * left;
                if (left > 0.5) {
                    reach = min(reach, ch.t);
                    dbg = select(select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(1.0, 1.0, 0.0), sc.x == 2.0), vec3<f32>(0.0, 1.0, 0.0), sc.x == 1.0);
                }
                left *= 1.0 - a;
            }
            if (left < 0.02) {
                break;
            }
            start = ch.t + 0.002;
        }
        l = acc + l * left;
        l = pane + l * through * select(1.0, 0.9, through < 1.0);
    }
    // (no single ray brighter than the sky's brightest: the sun's own glint on the surface
    // is the main pass's, and a ray that found the sun's disc was a speck of light)
    let lum = dot(l, vec3<f32>(0.2126, 0.7152, 0.0722));
    let cap = 2.5 * enh.exposure.x * max(dot(sh_irradiance(vec3<f32>(0.0, 0.0, 1.0)), vec3<f32>(0.2126, 0.7152, 0.0722)), 0.05);
    l = l * min(1.0, cap / max(lum, 1e-5));
    if (p.temporal.z == 8.0) {
        l = vec3<f32>(f32(dist > p.proj.z), rough, 0.0);
    }
    if (p.temporal.z == 11.0) {
        l = dbg;
    }
    textureStore(t_out, hp, vec4<f32>(l, reach));
}

// --- the composite: the denoised reflection (rt_denoise.wgsl) times its weight, added to the
// picture (blended additively). Where the rays are fewer than the pixels, from those round
// the pixel on its own surface.
struct VsOut {
    @builtin(position) clip: vec4<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> VsOut {
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    return VsOut(vec4<f32>(x, y, 0.0, 1.0));
}

@fragment
fn fs_composite(in: VsOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.clip.xy);
    let g = textureLoad(t_gbuf, px, 0);
    // (the reflection's weight is the normal's length: see `cs_reflect`)
    let weight = length(g.xyz);
    if (weight < 0.002 || g.w < 1e-4) {
        discard;
    }
    let aux = textureLoad(t_aux, px, 0);
    let dist = aux.r / g.w;
    let rough = clamp(aux.g / g.w, 0.0, 1.0);
    let hsize = vec2<i32>(textureDimensions(t_hist));
    let hq = (vec2<f32>(px) + vec2<f32>(0.5)) / f32(REFL_DIV) - vec2<f32>(0.5);
    let base = vec2<i32>(floor(hq));
    let fr = hq - floor(hq);
    let tol = (0.02 + rough * 0.06) * dist + 0.05;
    var sum = vec3<f32>(0.0);
    var wsum = 0.0;
    // (the four rays round the pixel, bilinear, those on its own surface only)
    for (var j = 0; j <= 1; j++) {
        for (var i = 0; i <= 1; i++) {
            let q = clamp(base + vec2<i32>(i, j), vec2<i32>(0), hsize - vec2<i32>(1));
            let m = textureLoad(t_meta, q, 0);
            let k = select(1.0 - fr.x, fr.x, i == 1) * select(1.0 - fr.y, fr.y, j == 1) + 1e-3;
            let w = k * select(0.0, 1.0, m.x > 0.0 && abs(m.x - dist) < tol);
            sum += textureLoad(t_hist, q, 0).rgb * w;
            wsum += w;
        }
    }
    var l: vec3<f32>;
    if (wsum < 1e-4) {
        // (no ray at this distance nearby: the sky probe the way the surface's lobe looks)
        let uv = (vec2<f32>(px) + vec2<f32>(0.5)) * p.size.zw;
        let view = normalize(world_pos(uv, 0.001) - p.eye.xyz);
        l = sky_probe(reflect(view, normalize(g.xyz)), rough);
    } else {
        l = sum / wsum;
    }
    if (p.temporal.z == 11.0) {
        return vec4<f32>(l * 3.0, 0.0);
    }
    if (p.temporal.z >= 8.0) {
        return vec4<f32>(select(l * 0.3, vec3<f32>(0.0), p.temporal.z == 10.0) + vec3<f32>(0.0, 0.0, weight * 10.0), 0.0);
    }
    return vec4<f32>(l * weight, 0.0);
}
