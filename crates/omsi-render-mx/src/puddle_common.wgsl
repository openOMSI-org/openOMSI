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

// xy: horizontal normal offset; z: the ring's strength, used for roughness as well.
fn puddle_ripple(pattern_xy: vec2<f32>, time: f32, rain: f32, coverage: f32) -> vec3<f32> {
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
