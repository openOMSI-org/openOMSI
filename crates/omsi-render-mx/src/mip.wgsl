// Mipmap generation: each texel of the smaller level is the average of exactly the area of
// the bigger level it covers. For even sizes that is a 2x2 block; for odd ones (15 texels ->
// 7) it is a bit more than two texels a side, the outer ones with partial weights - a plain
// 2x2 block there shifts the picture and drops its last row, and display digits grew bolder
// each level and lost their bottom row.
//
// `fs_main` alpha-weights RGB so bright RGB stored in transparent texels does not bleed into
// tree/fence silhouettes when the texture switches to a smaller mip. Where all alpha is 0
// the plain average is kept: on opaque materials the alpha channel is often only the envmap
// mask, and 0 there (lamps, wheels) must not turn the colour black.
//
// `fs_text` averages colour and alpha each on its own, as OMSI does for text and script
// textures (the cab displays, `[CTC]` pictures): alpha-weighted, the white of a digit kept
// full brightness out to the edge of its fading alpha, and digits grew bold and ran together
// from a few metres off.
@group(0) @binding(0) var t_src: texture_2d<f32>;
@group(0) @binding(1) var s_src: sampler;
struct VsOut { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    var out: VsOut;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

// How much of destination texel `i` source texel `j` covers along one axis (source size
// `s`, destination size `d`). In units of 1/d of a source texel the destination texel runs
// from i*s to (i+1)*s and source texel j from j*d to (j+1)*d: whole numbers, so the shares
// are exact and add up to 1.
fn share(i: u32, j: u32, s: u32, d: u32) -> f32 {
    let start = max(i * s, j * d);
    let end = min((i + 1u) * s, (j + 1u) * d);
    if (end <= start) {
        return 0.0;
    }
    return f32(end - start) / f32(s);
}

// The texels of the bigger level under one texel of the smaller, summed by their shares:
// colour times alpha, colour alone, and alpha. They are at most four a side (s <= 2d + 1).
struct Mix { by_alpha: vec3<f32>, colour: vec3<f32>, cover: f32 };

fn mix_under(pos: vec2<f32>) -> Mix {
    let s = textureDimensions(t_src);
    let d = max(s / 2u, vec2<u32>(1u));
    let at = min(vec2<u32>(pos), d - vec2<u32>(1u));
    let first = at * s / d;
    let last = min(((at + vec2<u32>(1u)) * s - vec2<u32>(1u)) / d, s - vec2<u32>(1u));
    var m = Mix(vec3<f32>(0.0), vec3<f32>(0.0), 0.0);
    for (var y = first.y; y <= last.y; y = y + 1u) {
        let wy = share(at.y, y, s.y, d.y);
        for (var x = first.x; x <= last.x; x = x + 1u) {
            let w = wy * share(at.x, x, s.x, d.x);
            let q = textureLoad(t_src, vec2<i32>(i32(x), i32(y)), 0);
            m.by_alpha = m.by_alpha + q.rgb * (q.a * w);
            m.colour = m.colour + q.rgb * w;
            m.cover = m.cover + q.a * w;
        }
    }
    return m;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let m = mix_under(in.clip.xy);
    if (m.cover > 2.5e-6) {
        return vec4<f32>(m.by_alpha / m.cover, m.cover);
    }
    return vec4<f32>(m.colour, m.cover);
}

@fragment
fn fs_text(in: VsOut) -> @location(0) vec4<f32> {
    let m = mix_under(in.clip.xy);
    return vec4<f32>(m.colour, m.cover);
}
