// Enhanced graphics: the main pass's own lighting model. Energy-conserving Lambert diffuse
// and GGX specular with Fresnel, lit by the sun (soft shadows whose penumbra grows with the
// distance to the blocker), the sky and the ground (spherical harmonics, occluded by the
// SSAO), a reflection probe of the sky for the reflections, point and spot lights falling
// off with the square of the distance, and the air in between. OMSI's material data
// decides the surface: [matl_envmap] factor x mask is how much it reflects and how smooth
// it is, the o3d specular power how rough a surface without one is, [matl_bumpmap] bends
// the normal. Everything is drawn pre-exposed into the high-range target.

// Only painted terrain may skip empty brush-mask pixels.
override TERRAIN_PAINT: bool = false;

@group(0) @binding(12) var t_probe: texture_cube<f32>;

fn d_ggx(nh: f32, a: f32) -> f32 {
    let a2 = a * a;
    let d = nh * nh * (a2 - 1.0) + 1.0;
    return a2 / (PI * d * d);
}

// The microfacets of a film of water over a road: the slopes of a water surface are
// Gaussian (Cox and Munk 1954), the Beckmann distribution. Its tail falls off as
// exp(-tan^2 / a^2), where GGX's falls as a^2 / tan^4: under GGX a headlamp - a hundred
// thousand candela towards the road - kept a highlight brighter than the lit road some ten
// degrees off its mirror direction, and its reflection ran as one white line from the lamp
// down to the camera.
fn d_beckmann(nh: f32, a: f32) -> f32 {
    let c2 = max(nh * nh, 1e-4);
    let a2 = a * a;
    return exp(-(1.0 - c2) / (c2 * a2)) / (PI * a2 * c2 * c2);
}

// Height-correlated Smith visibility (G / (4 n.l n.v)).
fn v_smith(nv: f32, nl: f32, a: f32) -> f32 {
    let a2 = a * a;
    let gv = nl * sqrt(nv * nv * (1.0 - a2) + a2);
    let gl = nv * sqrt(nl * nl * (1.0 - a2) + a2);
    return 0.5 / max(gv + gl, 1e-5);
}

// A diffuse surface's albedo under a film of water (n = 1.33; Lekner and Dorf 1988): the
// film lets in 1 - 0.066 of diffuse light, and of what the surface scatters back up only
// 1 - 0.472 leaves through the film's top, the rest returns to the surface.
fn wet_albedo(a: vec3<f32>) -> vec3<f32> {
    return 0.934 * a * 0.528 / (vec3<f32>(1.0) - 0.472 * a);
}

fn f_schlick(f0: vec3<f32>, c: f32) -> vec3<f32> {
    let f = pow(1.0 - clamp(c, 0.0, 1.0), 5.0);
    return f0 + (vec3<f32>(1.0) - f0) * f;
}

// The split-sum environment term (Karis' analytic fit).
fn env_brdf(f0: vec3<f32>, rough: f32, nv: f32) -> vec3<f32> {
    let c0 = vec4<f32>(-1.0, -0.0275, -0.572, 0.022);
    let c1 = vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let r = c0 * rough + c1;
    let a004 = min(r.x * r.x, pow(2.0, -9.28 * nv)) * r.x + r.y;
    let ab = vec2<f32>(-1.04, 1.04) * a004 + r.zw;
    return f0 * ab.x + vec3<f32>(ab.y);
}

// Ambient occlusion that lets bright surfaces bounce some light back into their own
// corners (Jimenez' multi-bounce fit), so white walls do not get grey creases.
fn ao_bounce(ao: f32, albedo: vec3<f32>) -> vec3<f32> {
    let a = 2.0404 * albedo - vec3<f32>(0.3324);
    let b = -4.7951 * albedo + vec3<f32>(0.6417);
    let c = 2.7552 * albedo + vec3<f32>(0.6903);
    return max(vec3<f32>(ao), ((a * ao + b) * ao + c) * ao);
}

// World direction to the probe's cube coordinates (and back: the swap is its own inverse).
fn to_cube(d: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(d.x, d.z, d.y);
}

// What a raindrop on a pane shows (`rain_light`): the sky probe that way, blurred a little
// by the drop's small lens, and low down the street's light in place of the probe's
// horizon (see the reflections in `shade_enhanced`).
fn rain_env_enhanced(d: vec3<f32>, lod: f32) -> vec3<f32> {
    let e = textureSampleLevel(t_probe, s_lin, to_cube(d), lod).rgb * enh.fog_color.w;
    let surround = enh.fog_color.rgb * (0.25 / 0.9);
    return mix(surround, e, smoothstep(-0.05, 0.35, d.z));
}

// A fixed, deterministic PCF kernel (shader.wgsl's `shadow_tent`). The old PCSS blocker
// search was unstable for alpha-tested foliage: a few leaves entering or leaving its
// 12-sample search changed the penumbra radius, producing checkerboard patches and
// camera-driven strips. The filter is fixed in shadow-map space, so the only thing that
// can change is the actual caster. `thin`: foliage, whose normals OMSI points up for even
// lighting - no receiver plane can be taken from them, a leaf compares at its own depth.
fn sun_shadow_soft(world_in: vec3<f32>, n: vec3<f32>, thin: bool) -> f32 {
    if (camera.shadow.x < 0.5) {
        return 1.0;
    }
    // (half a metre towards the sun for a leaf, as in `sun_shadow`)
    let world = world_in + select(vec3<f32>(0.0), camera.sun_dir.xyz * 0.5, thin);
    let ndl = clamp(dot(n, camera.sun_dir.xyz), 0.0, 1.0);
    // (a veil of high cloud spreads the sun into an aureole a few degrees wide: the light
    // comes from a larger source, and the shadow's edge widens with it)
    // (the tent's four gathers step apart for it: see `shader.wgsl`'s `shadow_tent_near`)
    let widen = 1.0 + 3.0 * enh.cloud_sun[1].w;
    let close = shadow_close(world, n, ndl, thin);
    if (close.y >= 0.999) {
        return close.x;
    }
    let near_lp = camera.light_view_proj * vec4<f32>(world + shadow_push_near(n, ndl), 1.0);
    let near_uv = vec2<f32>(near_lp.x * 0.5 + 0.5, 0.5 - near_lp.y * 0.5);
    let near_edge = max(abs(near_uv.x - 0.5), abs(near_uv.y - 0.5));
    let near_valid = near_edge < 0.48 && near_lp.z >= 0.0 && near_lp.z <= 1.0;
    var near_slope = shadow_receiver_slope(camera.light_view_proj, n);
    var far_slope = shadow_receiver_slope(camera.light_view_proj_far, n);
    if (thin) {
        near_slope = vec2<f32>(0.0);
        far_slope = vec2<f32>(0.0);
    }
    // Blend cascades by radial distance, not by the near map's projected square.  The
    // projected-edge test made the blend factor change when the camera moved sideways or
    // turned, even for a stationary receiver; that reads as a full shadow shimmer at the
    // cascade boundary.  The light boxes still validate coverage above, while this gives
    // the overlap a camera-centred, continuous transition.
    let radial = distance(world, camera.cam_pos.xyz);
    let near_weight = select(
        0.0,
        1.0 - smoothstep(camera.shadow.z * 0.62, camera.shadow.z * 0.92, radial),
        near_valid,
    );
    // Each cascade is filtered only where it counts: sampling both everywhere (32 compares
    // a pixel) halved the enhanced picture's frame rate against shadows off.
    var near_value = 1.0;
    if (near_weight > 0.001) {
        near_value = shadow_pcf_near(near_uv, near_lp.z, near_slope, widen);
    }
    near_value = mix(near_value, close.x, close.y);
    if (near_weight >= 0.999) {
        return near_value;
    }
    let flp = camera.light_view_proj_far * vec4<f32>(world + shadow_push_far(n, ndl), 1.0);
    let fuv = vec2<f32>(flp.x * 0.5 + 0.5, 0.5 - flp.y * 0.5);
    let far_valid = fuv.x >= 0.0 && fuv.x <= 1.0 && fuv.y >= 0.0 && fuv.y <= 1.0 && flp.z >= 0.0 && flp.z <= 1.0;
    var far_safe = 1.0;
    if (far_valid) {
        let far_value = shadow_pcf_far(fuv, flp.z, far_slope, widen);
        let far_edge = max(abs(fuv.x - 0.5), abs(fuv.y - 0.5));
        far_safe = mix(1.0, far_value, clamp((0.5 - far_edge) * 12.0, 0.0, 1.0));
    }
    return mix(far_safe, near_value, near_weight);
}

// Enhanced+: the solid casters' shadow where no traced one is there for a surface. The close
// and near maps hold only the cut-out casters then (`shadow_plan`), and such a surface - a
// pane, the edge of a step or a handrail thinner than a pixel of the depth prepass - lay in
// the sun under a roof: bright specks and stair-stepped lines along the edges, flickering
// as the camera moved and the traced texel was found or not. The far map keeps them all.
fn sun_shadow_far(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if (camera.shadow.x < 0.5) {
        return 1.0;
    }
    let ndl = clamp(dot(n, camera.sun_dir.xyz), 0.0, 1.0);
    let flp = camera.light_view_proj_far * vec4<f32>(world + shadow_push_far(n, ndl), 1.0);
    let fuv = vec2<f32>(flp.x * 0.5 + 0.5, 0.5 - flp.y * 0.5);
    if (fuv.x < 0.0 || fuv.x > 1.0 || fuv.y < 0.0 || fuv.y > 1.0 || flp.z < 0.0 || flp.z > 1.0) {
        return 1.0;
    }
    return shadow_pcf_far(fuv, flp.z, shadow_receiver_slope(camera.light_view_proj_far, n), 1.0);
}

// The sun's shadow at a point by one compare in the finest cascade that holds it (the close
// one, else the near one; lit beyond): for glass (see fs_enhanced).
fn sun_shadow_hard(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if (camera.shadow.x < 0.5) {
        return 1.0;
    }
    let ndl = clamp(dot(n, camera.sun_dir.xyz), 0.0, 1.0);
    let range = camera.flags.w;
    if (range > 0.0 && distance(world, camera.cam_pos.xyz) < range * 0.75) {
        let lp = camera.light_view_proj_close * vec4<f32>(world + shadow_push_close(n, ndl), 1.0);
        let uv = vec2<f32>(lp.x * 0.5 + 0.5, 0.5 - lp.y * 0.5);
        if (max(abs(uv.x - 0.5), abs(uv.y - 0.5)) < 0.48 && lp.z >= 0.0 && lp.z <= 1.0) {
            let scale = select(1.0, camera.post.w, camera.post.w > 0.0);
            let a = vec2<f32>(uv.x * 0.5 * scale + 0.5, uv.y * scale);
            return textureSampleCompareLevel(t_shadow, s_shadow, a, lp.z - SHADOW_BIAS_CLOSE / SHADOW_DEPTH_RANGE);
        }
    }
    let lp = camera.light_view_proj * vec4<f32>(world + shadow_push_near(n, ndl), 1.0);
    let uv = vec2<f32>(lp.x * 0.5 + 0.5, 0.5 - lp.y * 0.5);
    if (max(abs(uv.x - 0.5), abs(uv.y - 0.5)) < 0.48 && lp.z >= 0.0 && lp.z <= 1.0) {
        return textureSampleCompareLevel(t_shadow, s_shadow, vec2<f32>(uv.x * 0.5, uv.y), lp.z - SHADOW_BIAS_NEAR / SHADOW_DEPTH_RANGE);
    }
    return 1.0;
}

