// Enhanced+ (experimental, the settings' `rt_gi`): ray-traced global illumination - the light
// the surfaces round a point send back to it, one diffuse bounce (rt.rs `gi`). Part of the
// reflections' module: what a ray meets is lit by the same functions (rt_reflect.wgsl
// `hit_texel`, `hit_normal`, `sky_light`).
//
// After the depth prepass, before the main pass, one ray per GI_DIV x GI_DIV pixels leaves
// the surface in a cosine-weighted direction of its hemisphere (a new one every frame) and
// brings back, per ray:
// - a mesh within `p.gi.x` metres that last frame's picture showed: the light the picture had
//   there (`gi_seen`) - everything the main pass lit it with, the lamps with their shadows,
//   the lit windows and signs, and its own bounced light, so that the light bounces on from
//   frame to frame;
// - a mesh the picture did not show: its colour (its texel there times its material's) lit by
//   the sun where a shadow ray finds it, by the sky about its own normal (less inside the
//   player's vehicle, where most of the sky is behind its roof and walls), by the street lamps
//   of its grid cell, and its own light if it is unlit (a lamp's glass, a lit sign);
// - nothing: the sky. The main pass lights the point with its own sky light at the pixel's
//   own normal (less noisy than rays into the probe), scaled by how much of that light is not
//   blocked: alpha, the share of the sky's spherical harmonics the missed rays see (each ray
//   weighted by the sky's irradiance its way over the open hemisphere's, `sh_conv`). The sky
//   light holds the ground's light too: weighted so, a wall over an open square keeps its sky
//   and loses only the harmonics' ground, which the rays that met the real ground bring.
// A pane on the way (a bus's window) lets part of either through. Cut-out leaves stop a ray
// by their texel's alpha there.
//
// The rays' output: rgb the bounced irradiance (not pre-exposed, the main pass's `e_amb`
// units), a the share of the sky light that reaches it (see above); the meta: x the view depth (clip w)
// of the surface the rays left (0: none), y unused, zw its normal on the octahedron. The
// denoiser (rt_gi_denoise.wgsl) gathers them over the frames and the pixels round each.
// Without a history (a picture on its own, `p.temporal.x` 0) a pixel traces `p.gi.y` rays.

// (a quarter of the picture each way: the bounced light changes slowly over a surface, and
// the upsampling by depth keeps its edges; GI_OFF puts each ray in its block's middle)
const GI_DIV: i32 = 4;
const GI_OFF: i32 = 2;

// The street lamps' list (the main pass's `lights`, `grid`; `p.light_grid` says where the
// grid lies): a lamp in the point's cell lights a hit as it lights a surface there, without
// its shadow map.
struct GiLight {
    pos: vec4<f32>,
    color: vec4<f32>,
    dir: vec4<f32>,
    extra: vec4<f32>,
};
@group(0) @binding(17) var<storage, read> gi_lights: array<GiLight>;
@group(0) @binding(18) var<storage, read> gi_grid: array<u32>;

// The street lamps' irradiance at `pt` on a surface facing `n` (02_lamps.wgsl `lamp_light`'s
// falloff, cone and housing, no shadows, no headlamps: their beams are drawn on the road).
fn gi_lamps(pt: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    let cell = p.light_grid.z;
    let side = u32(p.light_grid.w);
    if (cell <= 0.0 || side == 0u) {
        return sum;
    }
    let f = (pt.xy - p.light_grid.xy) / cell;
    let x = i32(floor(f.x));
    let y = i32(floor(f.y));
    if (x < 0 || y < 0 || x >= i32(side) || y >= i32(side)) {
        return sum;
    }
    let base = (u32(y) * side + u32(x)) * 32u;
    for (var j = 0u; j < 32u; j++) {
        let li = gi_grid[base + j];
        if (li == 0xffffffffu) {
            break;
        }
        let l = gi_lights[li];
        if (l.extra.z != 0.0) {
            continue;
        }
        let d = l.pos.xyz - pt;
        let dist2 = dot(d, d);
        let range = l.extra.w;
        if (dist2 >= range * range) {
            continue;
        }
        let dist = sqrt(dist2);
        let ld = d / max(dist, 1e-3);
        let nl = dot(n, ld);
        if (nl <= 0.0) {
            continue;
        }
        var core = l.extra.y;
        if (core <= 0.0) {
            core = range * 0.125;
        }
        let q = dist2 / (range * range);
        let window = (1.0 - q * q) * (1.0 - q * q);
        var e = core * core / sqrt(dist2 * dist2 + core * core * core * core) * window;
        if (l.dir.w > -1.5) {
            e = e * smoothstep(l.dir.w, max(l.extra.x, l.dir.w + 1e-3), dot(-ld, l.dir.xyz));
        } else if (l.dir.z < -0.5) {
            e = e * (0.05 + 0.95 * smoothstep(-0.1, 0.3, ld.z));
        }
        sum += l.color.rgb * l.color.w * enh.lights.y * e * nl;
    }
    return sum;
}

// Inside the player's vehicle's box (the hits on its seats and walls see the sky only
// through its windows).
fn gi_in_vehicle(pt: vec3<f32>) -> bool {
    if (p.vehicle_box.w < 0.5) {
        return false;
    }
    let d = pt - p.vehicle_now.xyz;
    let cn = cos(p.vehicle_now.w);
    let sn = sin(p.vehicle_now.w);
    let local = vec3<f32>(d.x * cn + d.y * sn, -d.x * sn + d.y * cn, d.z) - p.vehicle_centre.xyz;
    return all(abs(local) <= p.vehicle_box.xyz);
}

// Last frame's picture at the rays' size (`cs_gi_capture`, the reflections' module's `t_hist`
// binding): rgb its light (not pre-exposed), a the view depth it was drawn at (0: the sky).
// `p.gi.w` 3 when it is there.

// A point on the player's vehicle where it was a frame ago (rt_denoise.wgsl `vehicle_prev`).
fn gi_vehicle_prev(pt: vec3<f32>) -> vec3<f32> {
    if (p.vehicle_box.w < 0.5) {
        return pt;
    }
    let d = pt - p.vehicle_now.xyz;
    let cn = cos(p.vehicle_now.w);
    let sn = sin(p.vehicle_now.w);
    let local = vec3<f32>(d.x * cn + d.y * sn, -d.x * sn + d.y * cn, d.z) - p.vehicle_centre.xyz;
    if (any(abs(local) > p.vehicle_box.xyz + vec3<f32>(0.3))) {
        return pt;
    }
    let l = local + p.vehicle_centre.xyz;
    let cp = cos(p.vehicle_prev.w);
    let sp = sin(p.vehicle_prev.w);
    return p.vehicle_prev.xyz + vec3<f32>(l.x * cp - l.y * sp, l.x * sp + l.y * cp, l.z);
}

// The light last frame's picture had at a hit (w 1), or none (w 0: off it, or the picture
// showed something nearer there).
fn gi_seen(pt: vec3<f32>) -> vec4<f32> {
    if (p.gi.w < 2.5) {
        return vec4<f32>(0.0);
    }
    let c = p.prev_view_proj * vec4<f32>(gi_vehicle_prev(pt), 1.0);
    if (c.w <= 0.05) {
        return vec4<f32>(0.0);
    }
    let uv = vec2<f32>(c.x / c.w * 0.5 + 0.5, 0.5 - c.y / c.w * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv >= vec2<f32>(1.0))) {
        return vec4<f32>(0.0);
    }
    let size = vec2<i32>(textureDimensions(t_hist));
    let s = textureLoad(t_hist, min(vec2<i32>(uv * vec2<f32>(size)), size - vec2<i32>(1)), 0);
    if (s.a <= 0.0 || abs(s.a - c.w) > 0.03 * c.w + 0.1) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(finite(s.rgb), 1.0);
}

// The light a hit sends back along the ray (not pre-exposed): its colour lit by the sun, the
// sky and the lamps, or its own light if it is unlit.
fn gi_hit_light(pt: vec3<f32>, d: vec3<f32>, r: Record, texel: vec4<f32>, nh: vec3<f32>) -> vec3<f32> {
    var albedo = r.color.rgb;
    if (texel.a >= 0.0) {
        albedo = albedo * texel.rgb;
    } else if (r.tex != 0xffffffffu) {
        albedo = albedo * tex_avg[r.tex].c.rgb;
    }
    albedo = clamp(albedo, vec3<f32>(0.0), vec3<f32>(1.0));
    if ((r.flags & 2u) != 0u) {
        // (drawn at `exposure.y` pre-exposed: its radiance)
        return albedo * enh.exposure.y / max(enh.exposure.x, 1e-6);
    }
    var nf = -d;
    if (dot(nh, nh) > 0.5) {
        nf = select(nh, -nh, dot(nh, d) > 0.0);
    }
    var e = vec3<f32>(0.0);
    let nl = dot(nf, p.sun.xyz);
    if (p.sun.w > 0.0 && nl > 0.0) {
        if (ray_hit(RT_FORCE_OPAQUE | RT_FIRST_HIT, MASK_SHADOW, 0.0, 1500.0, pt + nf * 0.03 - d * 0.02, p.sun.xyz).kind == RAY_QUERY_INTERSECTION_NONE) {
            let beam = select(1.0, clamp(enh.pbr_ext.x, 0.0, 1.0), enh.pbr_ext.y > 0.5);
            e = enh.sun.rgb * beam * nl;
        }
    }
    let sky_k = select(1.0, 0.3, gi_in_vehicle(pt));
    e = e + sky_light(nf) * sky_k + gi_lamps(pt + nf * 0.05, nf);
    return albedo / PI * e;
}

// One ray from `o` along `d`: x y z the light it brings back, w how much of the sky it saw
// (both already through a pane on its way). Kept lean: at most two queries (a pane, then
// what is behind it) and the hit lit once after them. (A loop that started the query again
// past every cut-out texel and lit the hit inside it took the trace from 1 to 6 ms of an
// M4's 1080p frame - the shader's size, not its rays, kept the GPU from filling up.)
// A cut-out mesh (leaves, a fence) stops the ray by its texel's coverage there, else the
// ray is taken as having gone through to the sky.
fn gi_ray(o: vec3<f32>, d: vec3<f32>, seed: vec3<u32>) -> vec4<f32> {
    let range = p.gi.x;
    var h = ray_hit(RT_FORCE_OPAQUE, MASK_SEEN | MASK_GLASS_SEEN, 0.0, range, o, d);
    if (h.kind == RAY_QUERY_INTERSECTION_NONE) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    var through = 1.0;
    var ri = h.instance_custom_data + h.geometry_index;
    if ((records[ri].flags & 64u) != 0u) {
        // a pane: most of the light through it, and what lies behind
        through = 0.7;
        h = ray_hit(RT_FORCE_OPAQUE, MASK_SEEN, h.t + 0.01, range, o, d);
        if (h.kind == RAY_QUERY_INTERSECTION_NONE) {
            return vec4<f32>(0.0, 0.0, 0.0, through);
        }
        ri = h.instance_custom_data + h.geometry_index;
    }
    let pt = o + d * h.t;
    // (a hit near the end of the range is partly the sky: no edge where it stops)
    let far = smoothstep(0.5 * range, range, h.t);
    let seen = gi_seen(pt);
    if (seen.w > 0.5) {
        return vec4<f32>(seen.rgb * through * (1.0 - far), through * far);
    }
    let r = records[ri];
    let texel = hit_texel(r, h.primitive_index, h.barycentrics);
    if ((r.flags & 1u) != 0u) {
        let a = select(coverage(r), smoothstep(ALPHA_REF - 0.2, ALPHA_REF + 0.2, texel.a), texel.a >= 0.0);
        if (hash3(seed + vec3<u32>(ri, 3u, 0u)) >= a) {
            return vec4<f32>(0.0, 0.0, 0.0, through);
        }
    }
    let nh = hit_normal(r, h.primitive_index, h.barycentrics, h.world_to_object);
    let l = gi_hit_light(pt, d, r, texel, nh);
    return vec4<f32>(finite(l) * through * (1.0 - far), through * far);
}

// The sky's irradiance (its harmonics, `sh_irradiance`) averaged over a cosine-weighted
// hemisphere about n: each band once more convolved by the cosine (2/3, 1/4). Over an open
// hemisphere the rays' `sh_irradiance(d)` average to this.
fn sh_conv(n: vec3<f32>) -> vec3<f32> {
    let c = enh.sh;
    let e = c[0].rgb * 0.282095
        + (c[1].rgb * (0.488603 * n.y) + c[2].rgb * (0.488603 * n.z) + c[3].rgb * (0.488603 * n.x)) * (2.0 / 3.0)
        + (c[4].rgb * (1.092548 * n.x * n.y) + c[5].rgb * (1.092548 * n.y * n.z)
        + c[6].rgb * (0.315392 * (3.0 * n.z * n.z - 1.0)) + c[7].rgb * (1.092548 * n.x * n.z)
        + c[8].rgb * (0.546274 * (n.x * n.x - n.y * n.y))) * 0.25;
    return max(e, vec3<f32>(0.0));
}

@compute @workgroup_size(8, 8)
fn cs_gi(@builtin(global_invocation_id) gid: vec3<u32>) {
    let hp = vec2<i32>(gid.xy);
    let hsize = vec2<i32>(textureDimensions(t_out));
    if (any(hp >= hsize)) {
        return;
    }
    let size = vec2<i32>(p.size.xy);
    let px = min(hp * GI_DIV + vec2<i32>(GI_OFF), size - vec2<i32>(1));
    let depth = textureLoad(t_depth, px, 0);
    if (depth <= 0.0) {
        textureStore(t_out, hp, vec4<f32>(0.0, 0.0, 0.0, 1.0));
        textureStore(t_meta_out, hp, vec4<f32>(0.0));
        return;
    }
    let uv = (vec2<f32>(px) + vec2<f32>(0.5)) * p.size.zw;
    let world = world_pos(uv, depth);
    let dist = length(world - p.eye.xyz);
    let clip_w = (p.view_proj * vec4<f32>(world, 1.0)).w;
    let n = depth_normal(px, world, depth);
    // (towards the end of its reach the traced light hands over to the main pass's own)
    let fade = 1.0 - smoothstep(0.75 * p.gi.z, p.gi.z, dist);
    textureStore(t_meta_out, hp, vec4<f32>(clip_w, 0.0, oct_encode(n)));
    if (fade <= 0.0) {
        textureStore(t_out, hp, vec4<f32>(0.0, 0.0, 0.0, 1.0));
        return;
    }
    let o = world + n * (0.03 + dist * 0.002);
    let b = basis(n);
    let rays = select(1u, u32(max(p.gi.y, 1.0)), p.temporal.x < 0.5);
    let u0 = frame_noise(hp);
    // (no single ray brighter than the open ground in the sun: a speck of a sunlit pane or a
    // lamp's glass seen by one ray in a hundred stays a speck after the denoiser)
    // (and by night than a white wall at a lamp's core: the lamps are the light then)
    let cap = max(max(dot(sky_light(vec3<f32>(0.0, 0.0, 1.0)) + enh.sun.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)), enh.lights.y), 1e-3) / PI;
    let lw = vec3<f32>(0.2126, 0.7152, 0.0722);
    let open = max(dot(sh_conv(n), lw), 1e-6);
    var sum = vec4<f32>(0.0);
    for (var k = 0u; k < rays; k++) {
        // (the R2 sequence on from this frame's pair: the rays of a still picture spread)
        let u = fract(u0 + f32(k) * vec2<f32>(0.7548777, 0.5698403));
        let rr = sqrt(u.x);
        let phi = 2.0 * PI * u.y;
        let d = b * vec3<f32>(rr * cos(phi), rr * sin(phi), sqrt(max(1.0 - u.x, 0.0)));
        var s = gi_ray(o, d, vec3<u32>(vec2<u32>(hp), u32(p.eye.w) * 16u + k));
        let lum = dot(s.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        // (the sky this ray saw, weighted by the sky's light its way)
        let sky = s.a * dot(sh_irradiance(d), lw) / open;
        s = vec4<f32>(s.rgb * min(1.0, cap / max(lum, 1e-6)), sky);
        sum += s;
    }
    sum = sum / f32(rays);
    // (cosine-weighted: the irradiance is pi times the rays' mean radiance)
    let e = sum.rgb * PI;
    textureStore(t_out, hp, vec4<f32>(e * fade, mix(1.0, clamp(sum.a, 0.0, 2.0), fade)));
}

// After the main pass: the picture at the rays' size for the next frame's rays (`gi_seen`) -
// its light over the 2 x 2 pixels in each block's middle (no longer pre-exposed) and the view
// depth there.
@compute @workgroup_size(8, 8)
fn cs_gi_capture(@builtin(global_invocation_id) gid: vec3<u32>) {
    let hp = vec2<i32>(gid.xy);
    let hsize = vec2<i32>(textureDimensions(t_out));
    if (any(hp >= hsize)) {
        return;
    }
    let size = vec2<i32>(p.size.xy);
    let px = min(hp * GI_DIV + vec2<i32>(GI_OFF), size - vec2<i32>(1));
    let depth = textureLoad(t_depth, px, 0);
    if (depth <= 0.0) {
        textureStore(t_out, hp, vec4<f32>(0.0));
        return;
    }
    let uv = vec2<f32>(px) * p.size.zw;
    let c = textureSampleLevel(t_scene, s_lin, uv, 0.0).rgb / max(enh.exposure.x, 1e-6);
    let world = world_pos((vec2<f32>(px) + vec2<f32>(0.5)) * p.size.zw, depth);
    textureStore(t_out, hp, vec4<f32>(finite(c), (p.view_proj * vec4<f32>(world, 1.0)).w));
}
