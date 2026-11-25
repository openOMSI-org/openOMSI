// The water normal shared by shading and scene reflections. The integer hash and the
// 1000 m period match shader.wgsl, keeping the rings fixed through floating-origin moves.
fn puddle_hash(c: vec2<f32>) -> f32 {
    let cells = 8000.0;
    let w = c - cells * floor(c / cells);
    var v = vec2<u32>(w) * 1664525u + vec2<u32>(1013904223u);
    v.x = v.x + v.y * 1664525u;
    v.y = v.y + v.x * 1664525u;
    v = v ^ (v >> vec2<u32>(16u));
    v.x = v.x + v.y * 1664525u;
    v.y = v.y + v.x * 1664525u;
    v = v ^ (v >> vec2<u32>(16u));
    return f32(v.x >> 8u) / 16777215.0;
}

// The rings the rain makes on standing water (Enhanced): xy the water's slope (a horizontal
// offset to its normal), z how much it is stirred (0..1, for its roughness as well).
//
// A falling drop sends out a short train of capillary waves: two or three rings close
// together, running out at a few decimetres a second and dying away within a hand's
// breadth. Three lattices of cells of 18 to 27 cm, turned against each other so that no grid
// shows, each take a drop now and then - the harder it rains the more often, and in more
// of them - every drop at a new place in its cell. A ring is a wave packet on its circle
// (a cosine under a Gaussian), its slope worked out exactly, and it stays inside its cell
// (no ring is cut off at a cell's edge).
fn puddle_ripple(pattern_xy: vec2<f32>, time: f32, rain: f32, coverage: f32) -> vec3<f32> {
    if (rain <= 0.001 || coverage <= 0.0) {
        return vec3<f32>(0.0);
    }
    var slope = vec2<f32>(0.0);
    var stir = 0.0;
    let rate = 0.7 + 1.6 * rain;
    let amp = min(1.0, 0.45 + rain) * coverage;
    for (var i = 0; i < 3; i = i + 1) {
        let fi = f32(i);
        let size = 0.18 + 0.045 * fi;
        let cs = vec2<f32>(cos(fi * 1.93), sin(fi * 1.93));
        let turned = vec2<f32>(pattern_xy.x * cs.x - pattern_xy.y * cs.y, pattern_xy.x * cs.y + pattern_xy.y * cs.x);
        let q = turned / size + vec2<f32>(fi * 17.0, fi * 5.0);
        let cell = floor(q);
        let seed = puddle_hash(cell + vec2<f32>(fi * 311.0, 0.0));
        let tt = time * rate * (0.8 + 0.4 * seed) + seed * 7.0;
        let n = floor(tt);
        let age = tt - n;
        let h = puddle_hash(cell + vec2<f32>(n - 97.0 * floor(n / 97.0) + 401.0, fi * 13.0 + 7.0));
        let hit = step(fract(h * 31.7), 0.3 + 0.65 * rain);
        let centre = vec2<f32>(0.3) + 0.4 * vec2<f32>(h, fract(h * 13.7));
        let local = fract(q) - centre;
        let r = length(local);
        let radius = 0.05 + 0.24 * sqrt(age);
        let d = r - radius;
        let w = 0.045;
        let k = 6.2831853 / 0.07;
        let env = exp(-(d * d) / (w * w));
        let a = (1.0 - age) * (1.0 - age) * hit;
        // the height is a * env * cos(k d): its slope along the radius
        let dh = a * env * (-2.0 * d / (w * w) * cos(k * d) - k * sin(k * d));
        slope = slope - local / max(r, 1e-4) * dh / k;
        stir = stir + a * env;
    }
    return vec3<f32>(slope * 0.03 * amp, clamp(stir * amp, 0.0, 1.0));
}

// The rings as a picture whose pixel covers `fp` metres of the water sees them: their
// slopes are a few centimetres long, and a pixel (or a puddle ray standing for several)
// wider than half of that lands on a ring's crest one frame and its trough the next - the
// water's picture trembled and broke into blocks. Faded out there, the rings leave their
// stir (the roughness) and the reflection keeps still.
fn puddle_ripple_seen(pattern_xy: vec2<f32>, time: f32, rain: f32, coverage: f32, fp: f32) -> vec3<f32> {
    let fade = 1.0 - smoothstep(0.012, 0.035, fp);
    let r = puddle_ripple(pattern_xy, time, rain, coverage);
    return vec3<f32>(r.xy * fade, r.z);
}

// The classic rings (Vanilla+'s puddle reflections): one lattice of 12.5 cm cells.
fn puddle_ripple_classic(pattern_xy: vec2<f32>, time: f32, rain: f32, coverage: f32) -> vec3<f32> {
    let cell = floor(pattern_xy * 8.0);
    let seed = puddle_hash(cell);
    let phase = fract(time * (0.8 + seed * 0.9) + seed * 13.0);
    let local = fract(pattern_xy * 8.0) - vec2<f32>(0.5);
    let ring = abs(length(local) - phase * 0.4);
    let hit = step(1.0 - clamp(0.25 + 0.6 * rain, 0.0, 0.9), fract(seed * 31.7));
    let strength = (1.0 - smoothstep(0.0, 0.06, ring)) * (1.0 - phase) * rain * coverage * hit;
    // Shallow rain rings disturb the image gently; steep normals break neighbouring
    // reflection rays apart and make otherwise still puddles look like rough waves.
    let bump = normalize(local + vec2<f32>(1e-5, 0.0)) * strength * 0.018;
    return vec3<f32>(bump, strength);
}
