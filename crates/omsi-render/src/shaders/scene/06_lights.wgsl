// Sum of the point lights registered in the grid cell of `p`. A light is registered in
// every cell its range touches, so the point's own cell holds every light that reaches it;
// looking into the neighbouring cells as well counted a light up to four times, and how
// often depended on where the cell edges fell - lamp pools brightened and dimmed as the
// camera moved.
// `map_k`: how much of the map's lamps a surface takes (0 on a light-mapped road in the
// classic picture, whose lamps are in its light map); a vehicle's own lights (dir.x 1, see
// lib.rs `gpu_light`) always shine - the headlights lit no road at all in vanilla.
fn point_lights(p: vec3<f32>, n: vec3<f32>, map_k: f32) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    let cell = camera.light_grid.z;
    let side = u32(camera.light_grid.w);
    if (cell <= 0.0 || side == 0u) {
        return sum;
    }
    let f = (p.xy - camera.light_grid.xy) / cell;
    let x = i32(floor(f.x));
    let y = i32(floor(f.y));
    if (x >= 0 && y >= 0 && x < i32(side) && y < i32(side)) {
        let base = (u32(y) * side + u32(x)) * CELL_CAP;
        for (var j = 0u; j < CELL_CAP; j = j + 1u) {
            let li = grid[base + j];
            if (li == 0xffffffffu) {
                break;
            }
            let l = lights[li];
            let d = l.pos.xyz - p;
            let dist = length(d);
            if (dist >= l.pos.w) {
                continue;
            }
            if (!(l.dir.w < -1.5 && l.dir.z < -0.5) && own_vehicle_blocks(p, l.pos.xyz)) {
                continue;
            }
            // full intensity within 1/8 of the range, then inverse-square, cut off at the range;
            // the strength was set by eye while a street lamp was still counted about three
            // times (see above), so it is three times what it was, to keep the lamp pools
            let r0 = l.pos.w * 0.125;
            let att = min(1.0, (r0 * r0) / max(dist * dist, 0.01)) * clamp(1.0 - dist / l.pos.w, 0.0, 1.0) * 3.75;
            let ndl = max(dot(n, d / max(dist, 0.01)), 0.15);
            var k = select(map_k, 1.0, l.dir.x > 0.5 && l.dir.w < -1.5);
            if (l.dir.w >= -1.5) {
                // a spot (a vehicle's [spotlight], as Direct3D lights with it): full inside
                // the inner cone, fading to nothing at the outer one; nothing behind it
                let c = dot(-d / max(dist, 0.01), l.dir.xyz);
                k = smoothstep(l.dir.w, max(l.extra.x, l.dir.w + 1e-3), c);
            }
            sum = sum + l.color.rgb * l.color.w * att * ndl * k;
        }
    }
    return sum;
}

// [interiorlight]: the light of a vehicle's saloon lamps on a mesh that names them in its
// [illumination_interior], 1 = a lamp's full light on a seat under it. `code` is the
// instance's lamp code (lib.rs `Instance::interior_lamps`): the first of its lamp slots in
// `lights` times 64 plus how many; below 1 it is a plain brightness (a passenger standing in
// a lit bus). Each lamp is OMSI's Direct3D point light: attenuation
// 1 / (d² / core²) with `core` the lamp's [interiorlight] range, no cut-off short of 100 m,
// times N·L; the sum saturates, as Direct3D's vertex colour does.
fn interior_lamps(p: vec3<f32>, n: vec3<f32>, code: f32) -> vec3<f32> {
    if (code < 1.0) {
        return vec3<f32>(1.0, 0.96, 0.84) * clamp(code, 0.0, 1.0);
    }
    let c = u32(code + 0.5);
    // (LAMP_CODE_STRIDE: 64 - up to 63 lamps a mesh)
    let first = c >> 6u;
    let count = c & 63u;
    var sum = vec3<f32>(0.0);
    for (var i = 0u; i < count; i = i + 1u) {
        let l = lights[first + i];
        let r = l.extra.w;
        let d = l.pos.xyz - p;
        let dist2 = max(dot(d, d), 1e-4);
        if (l.color.w <= 0.0 || dist2 >= r * r) {
            continue;
        }
        let core = max(l.extra.y, 0.01);
        let att = core * core / dist2;
        let ndl = max(dot(n, d * inverseSqrt(dist2)), 0.0);
        sum = sum + l.color.rgb * l.color.w * att * ndl;
    }
    return min(sum, vec3<f32>(1.0));
}

