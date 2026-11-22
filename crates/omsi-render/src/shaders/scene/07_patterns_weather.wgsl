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
// snows, drawn instead of water drops (scene/08_rain_glass.wgsl `glass_snow_with`). The mask
// value is the cover: 0 clear glass, up to 1 a pane closed by snow, above 1 the bank a blade
// pushed to the end of its sweep, below 0 glass a blade has just cleared.
fn is_snow_pane() -> bool {
    return material.wipe_bounds.z != 0.0 && material.wipe_bounds.w < 0.0;
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

// By day in a downpour some of the houses' windows are lit, as people switch the light on when
// it gets that dark (`camera.wind.w` - 1: the share, `Lighting::rain_lights`, 0 with the
// setting off): the night maps of a few rooms here and there, chosen by a cell of the world
// some four metres wide and three high (the world's own figures, `world_origin`, so the same
// rooms stay lit as the render origin moves), each fading in at its own point of the rain.
// What a night map shows at `world`: the night's own strength or, where such a room is lit,
// the full light.
fn rain_lit_windows(world: vec3<f32>) -> f32 {
    let night = clamp(camera.sun_color.w, 0.0, 1.0);
    let share = clamp(camera.wind.w - 1.0, 0.0, 1.0);
    if (share <= 0.0) {
        return night;
    }
    let w = world + camera.world_origin.xyz;
    let cell = floor(vec3<f32>(w.xy / 4.0, w.z / 3.0));
    let h = rain_hash(cell.xy + vec2<f32>(cell.z * 17.3, cell.z * 5.1));
    // (some a third of the rooms at the heaviest rain)
    let lit = smoothstep(h.x, h.x + 0.08, share * 0.38);
    return max(night, lit * (0.75 + 0.25 * h.y));
}
