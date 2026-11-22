// Current-frame puddle reflections. Rays are capped at quality.y screen-space steps and
// quality.x m (48 and 100 m high, 24 and 40 m low);
// only wet pixels run them. Misses leave the main shader's sky reflection untouched.
struct PuddleVehicleBox { a: vec4<f32>, b: vec4<f32>, c: vec4<f32> };
struct PuddleParams {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    eye_time: vec4<f32>,
    // xy: floating origin modulo 1000 m, z: rain without snow, w: probe mip count
    origin_rain: vec4<f32>,
    // rgb: the main shader's horizon surround, w: probe scale (both pre-exposed)
    sky: vec4<f32>,
    // xy: projection z coefficients, z: diagnostic view (0 normally), w: hit thickness
    projection_trace: vec4<f32>,
    vehicle_plane: vec4<f32>,
    // xyz: capture count and scene size; w: -1 Vanilla+, 0 Vanilla, 1 Enhanced
    vehicle_info: vec4<f32>,
    vehicle_parts: array<PuddleVehicleBox, 4>,
    // x: the farthest wet pixel traced (m), y: the most steps a ray takes, z: the picture
    // pixels one ray stands for, each way
    quality: vec4<f32>,
};
@group(0) @binding(0) var<uniform> p: PuddleParams;
@group(0) @binding(1) var t_scene: texture_2d<f32>;
@group(0) @binding(2) var t_mask: texture_2d<f32>;
@group(0) @binding(3) var t_depth: texture_depth_2d;
@group(0) @binding(4) var t_trace: texture_2d<f32>;
@group(0) @binding(5) var s_linear: sampler;
@group(0) @binding(6) var t_probe: texture_cube<f32>;
// Opaque scene depth plus reflective windows; receiver reconstruction uses t_depth.
@group(0) @binding(7) var t_hit_depth: texture_depth_2d;
@group(0) @binding(8) var t_vehicle: texture_2d<f32>;

struct PuddleVertex {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> PuddleVertex {
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    return PuddleVertex(vec4<f32>(x, y, 0.0, 1.0), vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5));
}

fn scene_pixel(uv: vec2<f32>) -> vec2<i32> {
    let size = vec2<i32>(textureDimensions(t_scene));
    return clamp(vec2<i32>(uv * vec2<f32>(size)), vec2<i32>(0), size - vec2<i32>(1));
}
fn world_pos(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let h = p.inv_view_proj * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    return h.xyz / h.w;
}
fn linear_depth(depth: f32) -> f32 {
    return p.projection_trace.y / max(depth + p.projection_trace.x, 1e-7);
}
fn clip_uv(c: vec4<f32>) -> vec2<f32> {
    return vec2<f32>(c.x / c.w * 0.5 + 0.5, 0.5 - c.y / c.w * 0.5);
}
fn finite_colour(c: vec3<f32>) -> vec3<f32> {
    let exponent = bitcast<vec3<u32>>(c) & vec3<u32>(0x7f800000u);
    return select(vec3<f32>(0.0), clamp(c, vec3<f32>(0.0), vec3<f32>(65000.0)), all(exponent != vec3<u32>(0x7f800000u)));
}
fn reflection_colour(c: vec3<f32>) -> vec3<f32> {
    return select(c, srgb_encode(c), abs(p.vehicle_info.w) < 0.5);
}
fn scene_colour(c: vec3<f32>) -> vec3<f32> {
    return select(c, srgb_decode(c), abs(p.vehicle_info.w) < 0.5);
}
fn trace_miss(depth: f32) -> vec4<f32> {
    return vec4<f32>(select(vec3<f32>(0.0), vec3<f32>(1.0, 0.0, 0.0), p.projection_trace.z == 2.0), depth);
}
fn fallback_light(direction: vec3<f32>, px: vec2<i32>) -> vec3<f32> {
    if (p.vehicle_info.w < 0.5) { return p.sky.rgb; }
    let lod = 0.03 * (p.origin_rain.w - 1.0);
    let sky = finite_colour(textureSampleLevel(t_probe, s_linear, direction.xzy, lod).rgb * p.sky.w);
    let open = smoothstep(-0.05, 0.35, direction.z);
    let fallback = mix(p.sky.rgb, sky, mix(open, 1.0, 0.25));
    let mask = textureLoad(t_mask, px, 0);
    // Sky ambient occlusion removes sky light; a local scene hit already contains its
    // own lighting and must not be suppressed by the receiver's ambient occlusion.
    return fallback * clamp(mask.g / max(mask.b * 0.49, 1e-6), 0.0, 1.0);
}
fn player_surface(world: vec3<f32>) -> bool {
    for (var i = 0u; i < u32(p.vehicle_info.x); i++) {
        let box = p.vehicle_parts[i];
        let d = world - box.a.xyz;
        let local = vec3<f32>(d.x * box.b.x - d.y * box.a.w,
            d.x * box.a.w + d.y * box.b.x, d.z) - box.c.xyz;
        if (all(abs(local) < box.b.yzw + vec3<f32>(0.35))) { return true; }
    }
    return false;
}

fn local_vehicle_plane(world: vec3<f32>) -> bool {
    return p.vehicle_info.x > 0.5 && abs(dot(p.vehicle_plane.xyz, world) - p.vehicle_plane.w) < 0.04;
}

// Reconstruct the receiver's geometric normal for roads on other grades. Use the
// nearer neighbor on each axis so a foreground bus or kerb does not tilt the water.
fn receiver_normal(px: vec2<i32>, uv: vec2<f32>, world: vec3<f32>, depth: f32) -> vec3<f32> {
    let size = vec2<i32>(textureDimensions(t_depth));
    var axes: array<vec3<f32>, 2>;
    for (var i = 0; i < 2; i++) {
        let axis = select(vec2<i32>(1, 0), vec2<i32>(0, 1), i == 1);
        let plus = clamp(px + axis, vec2<i32>(0), size - vec2<i32>(1));
        let minus = clamp(px - axis, vec2<i32>(0), size - vec2<i32>(1));
        let dp = textureLoad(t_depth, plus, 0);
        let dm = textureLoad(t_depth, minus, 0);
        let forward = dp > 0.0 && (dm <= 0.0 || abs(dp - depth) < abs(dm - depth));
        let q = select(minus, plus, forward);
        let d = select(dm, dp, forward);
        axes[i] = (world_pos((vec2<f32>(q) + vec2<f32>(0.5)) / vec2<f32>(size), d) - world)
            * select(-1.0, 1.0, forward);
    }
    let n = cross(axes[0], axes[1]);
    if (dot(n, n) < 1e-12) { return vec3<f32>(0.0, 0.0, 1.0); }
    return normalize(n * select(-1.0, 1.0, n.z > 0.0));
}

@fragment
fn fs_trace(in: PuddleVertex) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(t_scene));
    let centre = vec2<i32>(in.uv * vec2<f32>(size));
    // Pick a wet sample among the pixels this ray stands for (2x2 at half size, 4x4 on Low or
    // past the ray budget), so a small pool off its centre still gets a ray: at most 3x3
    // looks, spread over them (2x2, the same pixels, at half size) - the looks per ray do not
    // grow with the picture. The full-size resolve reapplies the exact mask.
    var px = clamp(centre, vec2<i32>(0), size - vec2<i32>(1));
    var weight = 0.0;
    let span = clamp(i32(p.quality.z), 1, 8);
    let half = span / 2;
    let taps = min(span, 3);
    for (var y = 0; y < taps; y++) {
        for (var x = 0; x < taps; x++) {
            let at = (vec2<i32>(2 * x + 1, 2 * y + 1) * span) / (2 * taps) - vec2<i32>(half);
            let q = clamp(centre + at, vec2<i32>(0), size - vec2<i32>(1));
            let w = textureLoad(t_mask, q, 0).b;
            if (w > weight) { weight = w; px = q; }
        }
    }
    if (weight < 0.004) { return vec4<f32>(0.0); }
    let depth = textureLoad(t_depth, px, 0);
    if (depth <= 0.0) { return vec4<f32>(0.0); }
    let uv = (vec2<f32>(px) + vec2<f32>(0.5)) / vec2<f32>(size);
    let world = world_pos(uv, depth);
    let distance = length(world - p.eye_time.xyz);
    if (distance > p.quality.x) { return vec4<f32>(0.0); }
    let ripple = puddle_ripple(world.xy + p.origin_rain.xy, p.eye_time.w, p.origin_rain.z, 1.0);
    var surface_normal = p.vehicle_plane.xyz;
    if (!local_vehicle_plane(world)) { surface_normal = receiver_normal(px, uv, world, depth); }
    let normal = normalize(surface_normal + vec3<f32>(ripple.xy, 0.0));
    let direction = reflect(normalize(world - p.eye_time.xyz), normal);
    if (local_vehicle_plane(world)) {
        let vehicle = textureSampleLevel(t_vehicle, s_linear, uv + ripple.xy * 0.006, 0.0);
        if (vehicle.a > 0.01) {
            if (p.projection_trace.z == 2.0) { return vec4<f32>(0.0, 1.0, 0.0, depth); }
            if (p.projection_trace.z == 3.0) { return vec4<f32>(finite_colour(vehicle.rgb), depth); }
            return vec4<f32>((reflection_colour(finite_colour(vehicle.rgb / max(vehicle.a, 1e-5))) - fallback_light(direction, px)) * vehicle.a, depth);
        }
    }
    let start = world + normal * 0.025 + direction * 0.08;
    let c0 = p.view_proj * vec4<f32>(start, 1.0);
    let cd = p.view_proj * vec4<f32>(direction, 0.0);
    var ray_length = 80.0;
    // A steep downward camera reflects towards the near plane. Clip before projecting;
    // projecting an endpoint behind the eye would turn the ray inside out.
    if (cd.w < 0.0) { ray_length = min(ray_length, max(0.0, (c0.w - 0.1) / -cd.w)); }
    if (ray_length < 0.1 || c0.w <= 0.0) { return trace_miss(depth); }
    let c1 = c0 + cd * ray_length;
    let uv0 = clip_uv(c0);
    let uv1 = clip_uv(c1);
    let duv = uv1 - uv0;
    var end = 1.0;
    // Trim to the screen rather than spending the rest of the budget outside it.
    for (var axis = 0; axis < 2; axis++) {
        if (abs(duv[axis]) > 1e-6) {
            let edge = select(0.002, 0.998, duv[axis] > 0.0);
            end = min(end, max(0.0, (edge - uv0[axis]) / duv[axis]));
        }
    }
    let pixels = length(duv * end * vec2<f32>(size));
    if (pixels < 2.0) { return trace_miss(depth); }
    let steps = i32(clamp(ceil(pixels / 8.0), 8.0, p.quality.y));
    let z0 = c0.z / c0.w;
    let z1 = c1.z / c1.w;
    var previous = 0.0;
    var previous_delta = -1.0;
    for (var i = 1; i <= steps; i++) {
        let t = end * f32(i) / f32(steps);
        let at = uv0 + duv * t;
        let seen = textureLoad(t_hit_depth, scene_pixel(at), 0);
        let delta = linear_depth(mix(z0, z1, t)) - linear_depth(seen);
        if (seen > 0.0 && delta >= 0.0) {
            var lo = previous;
            var hi = t;
            // Refine a crossing; checking thickness afterwards rejects silhouettes with
            // empty space behind them instead of stretching the object's edge into water.
            for (var j = 0; j < 5 && previous_delta < 0.0; j++) {
                let mid = (lo + hi) * 0.5;
                let md = textureLoad(t_hit_depth, scene_pixel(uv0 + duv * mid), 0);
                if (md > 0.0 && linear_depth(mix(z0, z1, mid)) >= linear_depth(md)) { hi = mid; }
                else { lo = mid; }
            }
            let hit_uv = uv0 + duv * hi;
            let hit_depth = textureLoad(t_hit_depth, scene_pixel(hit_uv), 0);
            let hit_linear = linear_depth(hit_depth);
            let gap = linear_depth(mix(z0, z1, hi)) - hit_linear;
            let hit_world = world_pos(hit_uv, hit_depth);
            // The player vehicle has one consistent planar projection. A screen-space
            // hit outside that silhouette would create a second, offset bumper/window.
            if (player_surface(hit_world) && local_vehicle_plane(world)) { return trace_miss(depth); }
            if (hit_depth > 0.0 && gap >= 0.0 && gap < p.projection_trace.w + hit_linear * 0.015
                && dot(hit_world - world, normal) > 0.05) {
                let border = min(min(hit_uv.x, hit_uv.y), min(1.0 - hit_uv.x, 1.0 - hit_uv.y));
                let confidence = smoothstep(0.0, 0.04, border) * (1.0 - smoothstep(p.quality.x * 0.35, p.quality.x, distance));
                let colour = finite_colour(textureSampleLevel(t_scene, s_linear, hit_uv, 0.0).rgb);
                if (p.projection_trace.z == 2.0) { return vec4<f32>(0.0, 1.0, 0.0, depth); }
                if (p.projection_trace.z == 3.0) { return vec4<f32>(colour, depth); }
                // Replace the reflected sky light rather than adding a second reflection.
                return vec4<f32>((reflection_colour(colour) - fallback_light(direction, px)) * confidence, depth);
            }
        }
        previous = t;
        previous_delta = select(-1.0, delta, seen > 0.0);
    }
    return trace_miss(depth);
}

// Separable five-tap binomial filter at the bounded trace resolution. Filter the
// signed lighting change, including misses, to soften silhouettes and isolated gaps.
// Receiver depth prevents mixing water on opposite sides of a kerb or foreground mesh.
fn filter_reflection(px: vec2<i32>, axis: vec2<i32>) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(t_trace));
    let centre = textureLoad(t_trace, px, 0);
    if (centre.a <= 0.0) { return vec4<f32>(0.0); }
    let kernel = array<f32, 5>(1.0, 4.0, 6.0, 4.0, 1.0);
    var sum = vec3<f32>(0.0);
    var total = 0.0;
    // (Enhanced blurs wider: a puddle on asphalt is no mirror, its picture is soft)
    let step = select(1, 3, p.vehicle_info.w > 0.5);
    for (var i = -2; i <= 2; i++) {
        let q = clamp(px + axis * i * step, vec2<i32>(0), size - vec2<i32>(1));
        let r = textureLoad(t_trace, q, 0);
        let difference = abs(r.a - centre.a) / max(centre.a, 1e-6);
        let w = kernel[u32(i + 2)] * (1.0 - smoothstep(0.015, 0.06, difference))
            * select(0.0, 1.0, r.a > 0.0);
        sum += r.rgb * w;
        total += w;
    }
    return vec4<f32>(sum / max(total, 1e-5), centre.a);
}
@fragment
fn fs_blur_x(in: PuddleVertex) -> @location(0) vec4<f32> {
    return filter_reflection(vec2<i32>(in.clip.xy), vec2<i32>(1, 0));
}
@fragment
fn fs_blur_y(in: PuddleVertex) -> @location(0) vec4<f32> {
    return filter_reflection(vec2<i32>(in.clip.xy), vec2<i32>(0, 1));
}

@fragment
fn fs_resolve(in: PuddleVertex) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.clip.xy);
    let base = textureLoad(t_scene, px, 0);
    let weight = textureLoad(t_mask, px, 0).b;
    if (p.projection_trace.z == 1.0) { return vec4<f32>(vec3<f32>(weight * 4.0), 1.0); }
    if (weight < 0.004) { return base; }
    let depth = textureLoad(t_depth, px, 0);
    let size = vec2<i32>(textureDimensions(t_trace));
    if (p.projection_trace.z == 4.0) {
        let world = world_pos(in.uv, depth);
        return vec4<f32>(vec3<f32>(0.5 + (dot(p.vehicle_plane.xyz, world) - p.vehicle_plane.w) * 2.0), 1.0);
    }
    let q = in.uv * vec2<f32>(size) - vec2<f32>(0.5);
    let origin = vec2<i32>(floor(q));
    let fraction = fract(q);
    var sum = vec3<f32>(0.0);
    var total = 0.0;
    for (var y = 0; y < 2; y++) {
        for (var x = 0; x < 2; x++) {
            let c = clamp(origin + vec2<i32>(x, y), vec2<i32>(0), size - vec2<i32>(1));
            let r = textureLoad(t_trace, c, 0);
            let f = select(vec2<f32>(1.0) - fraction, fraction, vec2<bool>(x == 1, y == 1));
            // Bilateral upsampling keeps a half-size ray on its receiver. The full-size
            // material mask keeps reflections off kerbs, bus bodies and dry patches.
            let difference = abs(r.a - depth) / max(depth, 1e-6);
            let w = f.x * f.y * (1.0 - smoothstep(0.01, 0.04, difference)) * select(0.0, 1.0, r.a > 0.0);
            sum += r.rgb * w;
            total += w;
        }
    }
    if (p.projection_trace.z > 1.0) { return vec4<f32>(max(sum / max(total, 1e-5), vec3<f32>(0.0)), 1.0); }
    return vec4<f32>(scene_colour(max(reflection_colour(base.rgb) + sum / max(total, 1e-5) * weight, vec3<f32>(0.0))), base.a);
}
