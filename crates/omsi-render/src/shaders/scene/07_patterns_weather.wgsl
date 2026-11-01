// Value noise in three octaves: the "fractal" detail (the `detail_textures` setting) laid
// over the ground and the road surfaces close by, so that a blurred texture keeps some grain.
//
// The lattice cell is hashed as an integer. The old `fract(sin(dot(q, …)) * 43758.5)`
// hash took the cell's map coordinate - two and a half million at Spandau's 892 km - into
// a sine: in 32-bit floats its argument is a multiple of 64 there, and what came out was
// not noise but a ramp that repeated along straight lines, so every road wore thin
// diagonal streaks at an exact spacing. Cells are wrapped at `PATTERN_PERIOD` (the pattern
// coordinate is taken modulo that on the CPU, see `world_pattern_xy`), which keeps the
// hash's input small and the pattern seamless across the wrap.
const PATTERN_PERIOD: f32 = 1000.0;

fn hash_cell(c: vec2<f32>, cells: f32) -> f32 {
    // the cell index modulo the lattice's cells per period, as an exact integer
    let w = c - cells * floor(c / cells);
    var v = vec2<u32>(w) * 1664525u + vec2<u32>(1013904223u);
    // (a PCG-style mix: two rounds of multiply, cross-add and xor-shift)
    v.x = v.x + v.y * 1664525u;
    v.y = v.y + v.x * 1664525u;
    v = v ^ (v >> vec2<u32>(16u));
    v.x = v.x + v.y * 1664525u;
    v.y = v.y + v.x * 1664525u;
    v = v ^ (v >> vec2<u32>(16u));
    return f32(v.x >> 8u) / 16777215.0;
}

// `freq`: lattice cells per metre of `p` (a whole number of cells per `PATTERN_PERIOD`).
fn vnoise_f(p: vec2<f32>, freq: f32, offset: vec2<f32>) -> f32 {
    let q = p * freq + offset;
    let cells = round(PATTERN_PERIOD * freq);
    let i = floor(q);
    let f = q - i;
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash_cell(i, cells);
    let b = hash_cell(i + vec2<f32>(1.0, 0.0), cells);
    let c = hash_cell(i + vec2<f32>(0.0, 1.0), cells);
    let d = hash_cell(i + vec2<f32>(1.0, 1.0), cells);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}


fn detail_noise(p: vec2<f32>, near: f32) -> f32 {
    // Anti-alias: point-sampling a lattice this fine aliases wherever one pixel's
    // footprint spans more than about one cell of it - looking down a street at a
    // grazing angle, or just driving away, constantly changes that footprint, and the
    // alias pattern with it, which reads as the "fractal" texture re-rendering itself as
    // the view angle changes. `fwidth` gives the footprint in the same units as `p`; an
    // octave fades to its own mean (0.5, the hash is uniform 0..1) once a pixel can no
    // longer resolve it, which is what an infinitely dense filter of it would give anyway.
    let w = max(fwidth(p).x, fwidth(p).y);
    let seen = select(0.0, 1.0, near > 0.0);
    let fade0 = (1.0 - smoothstep(0.3, 1.2, w * 2.9)) * seen;
    let fade1 = (1.0 - smoothstep(0.3, 1.2, w * 0.73)) * seen;
    let fade2 = (1.0 - smoothstep(0.3, 1.2, w * 0.19)) * seen;
    var n0 = 0.5;
    var n1 = 0.5;
    var n2 = 0.5;
    if (fade0 > 0.0) {
        n0 = mix(0.5, vnoise_f(p, 2.9, vec2<f32>(0.0)), fade0);
    }
    if (fade1 > 0.0) {
        n1 = mix(0.5, vnoise_f(p, 0.73, vec2<f32>(0.0)), fade1);
    }
    if (fade2 > 0.0) {
        n2 = mix(0.5, vnoise_f(p, 0.19, vec2<f32>(0.0)), fade2);
    }
    return n0 * 0.5 + n1 * 0.3 + n2 * 0.2;
}

fn world_pattern_xy(p: vec3<f32>) -> vec2<f32> {
    // Geometry is relative to the floating render origin for f32 precision. Procedural
    // patterns are not: reconstruct their stable map-space coordinate so crossing the
    // renderer's 100 m origin cell cannot re-seed the asphalt/ground pattern. The origin
    // arrives modulo PATTERN_PERIOD (lib.rs), so this stays a small, exact number: the map
    // coordinate itself (892 km at Spandau) has a 6 cm step in 32 bits, a fifth of the
    // finest noise cell.
    return p.xy + camera.world_origin.xy;
}

// Whether every component is a finite number, tested on the bits: Metal's fast maths takes
// `x != x` and comparisons with a NaN as it likes (a NaN colour happened to come out as
// something on the Mac), Direct3D 12 keeps the NaN and draws it black.
fn all_finite(v: vec3<f32>) -> bool {
    let e = bitcast<vec3<u32>>(v) & vec3<u32>(0x7f800000u);
    return all(e != vec3<u32>(0x7f800000u));
}

fn finite_or(v: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {
    return select(fallback, v, all_finite(v));
}

// The side of a tracked pane's wetness mask (`window_wipers::SIZE`). A constant, not the
// texture's own size: with `textureDimensions(t_trans)` in these functions Apple's Metal
// compiler crashed on the scene shader (the pipelines failed and the renderer fell back to
// 1x MSAA without snowfall and lamp shadows).
const WIPE_MASK_SIZE: i32 = 128;

fn window_wetness(in: FsIn) -> f32 {
    if (material.wipe_bounds.z != 0.0) {
        // The mask packs the pane depth into R/G (high and low byte) and the wetness into
        // B (the pane's unwiped wetness) and A (the local wetness). A filtered sample mixes
        // the depth bytes of neighbouring texels into a depth that belongs to neither, so
        // the pane test flickered between A and B from texel to texel: a staircase along
        // every texel row where the raked glass changes depth. Read the four texels
        // unfiltered, test each by its own depth, and blend the results bilinearly.
        let dims = vec2<i32>(WIPE_MASK_SIZE);
        let inside = all(in.wipe_uv.xy >= vec2<f32>(0.0)) && all(in.wipe_uv.xy <= vec2<f32>(1.0));
        let p = in.wipe_uv.xy * vec2<f32>(dims) - 0.5;
        let base = vec2<i32>(floor(p));
        let f = p - floor(p);
        var wet = 0.0;
        for (var k = 0; k < 4; k++) {
            let o = vec2<i32>(k & 1, k >> 1);
            let w = select(1.0 - f.x, f.x, o.x == 1) * select(1.0 - f.y, f.y, o.y == 1);
            let texel = textureLoad(t_trans, clamp(base + o, vec2<i32>(0), dims - 1), 0);
            let depth = dot(texel.rg, vec2<f32>(256.0, 1.0)) * (64.0 / 257.0) - 32.0;
            let local = (texel.a * 255.0 - 8.0) * (2.0 / 247.0);
            wet += w * select(texel.b, local, inside && abs(depth - in.wipe_uv.z) <= 0.1);
        }
        return wet;
    }
    return in.params.x;
}

// The snow variant of a tracked pane (`wipe_bounds.w` < 0): the snow the mask holds while it
// snows, drawn here instead of water drops. The mask value is the cover: 0 clear glass, up
// to 1 a pane closed by snow, above 1 the bank a blade pushed to the end of its sweep, below
// 0 glass a blade has just cleared. Crystals settle first, sparse and of two sizes; as the
// cover grows they close into a milky layer, thickest in the banks.
fn is_snow_pane() -> bool {
    return material.wipe_bounds.z != 0.0 && material.wipe_bounds.w < 0.0;
}

fn snow_on_pane(in: FsIn, cover: f32) -> vec4<f32> {
    if (cover <= 0.0) {
        return vec4<f32>(1.0, 1.0, 1.0, 0.0);
    }
    // the position on the pane in metres, so that crystals keep their size on every pane:
    // the film mesh's own coordinates, laid out in metres on each pane's glass when the mesh
    // is uploaded (omsi-scene `pane_uv`). Not the mask's projection, which is one plane for
    // all the slot's panes - a rear window in the slot of the side windows lay edge-on in it,
    // and its snow came out as streaks.
    let m = in.uv - in.params.zw;
    var flakes = 0.0;
    for (var layer = 0; layer < 2; layer++) {
        let cell = select(0.011, 0.02, layer == 1);
        let p = m / cell + f32(layer) * 37.0;
        let h = rain_hash(p);
        let h2 = rain_hash(p + vec2<f32>(91.0, 13.0));
        let centre = floor(p) + 0.25 + 0.5 * h;
        let r = 0.16 + 0.2 * h2.x;
        // a crystal shows once the cover passes its own threshold: they appear one by one
        let shown = step(h2.y, cover * select(1.15, 0.85, layer == 1));
        let edge = 1.0 - smoothstep(r * 0.55, r, length(p - centre));
        flakes = max(flakes, shown * edge * (0.6 + 0.4 * h.x));
    }
    // settled snow is never even: a smooth noise of a few centimetres and a finer one break
    // up where the layer closes and give a blade's bank its ragged edge and lumpy body
    let lumps = snow_noise(m / 0.045) * 0.65 + snow_noise(m / 0.012 + 7.0) * 0.35;
    let c = cover * (0.7 + 0.6 * lumps);
    let sheet = smoothstep(0.5, 1.2, c) * 0.75 + smoothstep(1.0, 1.7, c) * 0.22;
    let white = mix(vec3<f32>(0.9, 0.92, 0.96), vec3<f32>(0.97, 0.98, 1.0), smoothstep(1.0, 1.8, c));
    return vec4<f32>(white, clamp(max(flakes * 0.92, sheet), 0.0, 0.97));
}

// Smooth value noise (0..1) from the cell hash: the corners' values blended with a
// smoothstep, so it has no seams at the cell edges.
fn snow_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = p - i;
    let u = f * f * (3.0 - 2.0 * f);
    let a = rain_hash(i).x;
    let b = rain_hash(i + vec2<f32>(1.0, 0.0)).x;
    let c = rain_hash(i + vec2<f32>(0.0, 1.0)).x;
    let d = rain_hash(i + vec2<f32>(1.0, 1.0)).x;
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

