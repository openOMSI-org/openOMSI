// Shadow map passes: depth from the sun, alpha-tested materials cut out by their texture.
// A surface that casts - a spline standing clear of the ground, a bridge deck - casts from
// half a metre further away from the sun: what lies right under it (the embankment object
// under a railway, the ground a ramp touches down on) is no darker for it, while the ground
// metres below a deck gets its shadow.
fn shadow_caster_pos(e: u32, wp: vec4<f32>) -> vec4<f32> {
    if (abs(inst_params[e * 2u + 1u].w - 1.0) < 0.01) {
        return vec4<f32>(wp.xyz - camera.sun_dir.xyz * 0.5, wp.w);
    }
    return wp;
}

@vertex
fn vs_shadow(in: VsIn) -> VsOut {
    let e = draw_list[in.inst];
    let m = model_matrix(e);
    let wp = shadow_caster_pos(e, m * vec4<f32>(in.pos, 1.0) + vec4<f32>(tree_sway(in.pos, m), 0.0));
    var out: VsOut;
    out.clip = camera.light_view_proj * wp;
    out.world = wp.xyz;
    out.normal = in.normal;
    out.spec_sun = vec3<f32>(0.0);
    out.spec_sky = vec3<f32>(0.0);
    let pr = inst_params[e * 2u];
    out.uv = in.uv + pr.zw;
    out.params = pr;
    out.params2 = inst_params[e * 2u + 1u];
    if (pr.y < 0.5) {
        out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    }
    return out;
}

@vertex
fn vs_shadow_close(in: VsIn) -> VsOut {
    let e = draw_list[in.inst];
    let m = model_matrix(e);
    let wp = shadow_caster_pos(e, m * vec4<f32>(in.pos, 1.0) + vec4<f32>(tree_sway(in.pos, m), 0.0));
    var out: VsOut;
    out.clip = camera.light_view_proj_close * wp;
    out.world = wp.xyz;
    out.normal = in.normal;
    out.spec_sun = vec3<f32>(0.0);
    out.spec_sky = vec3<f32>(0.0);
    let pr = inst_params[e * 2u];
    out.uv = in.uv + pr.zw;
    out.params = pr;
    out.params2 = inst_params[e * 2u + 1u];
    if (pr.y < 0.5) {
        out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    }
    return out;
}

@vertex
fn vs_shadow_far(in: VsIn) -> VsOut {
    let e = draw_list[in.inst];
    let m = model_matrix(e);
    let wp = shadow_caster_pos(e, m * vec4<f32>(in.pos, 1.0) + vec4<f32>(tree_sway(in.pos, m), 0.0));
    var out: VsOut;
    out.clip = camera.light_view_proj_far * wp;
    out.world = wp.xyz;
    out.normal = in.normal;
    out.spec_sun = vec3<f32>(0.0);
    out.spec_sky = vec3<f32>(0.0);
    let pr = inst_params[e * 2u];
    out.uv = in.uv + pr.zw;
    out.params = pr;
    out.params2 = inst_params[e * 2u + 1u];
    if (pr.y < 0.5) {
        out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    }
    return out;
}

// Enhanced: a lamp's shadow map (one of the tiles under the far map). The draw list names
// the tile above the entry's own bits (`LAMP_ENTRY_BITS` in lamp_slots.rs), and the tile's
// matrix looks down from a street lamp's head or along the road from a vehicle's headlamps.
const LAMP_ENTRY_BITS: u32 = 26u;

@vertex
fn vs_shadow_lamp(in: VsIn) -> VsOut {
    let raw = draw_list[in.inst];
    let e = raw & ((1u << LAMP_ENTRY_BITS) - 1u);
    let k = raw >> LAMP_ENTRY_BITS;
    let m = model_matrix(e);
    let wp = m * vec4<f32>(in.pos, 1.0) + vec4<f32>(tree_sway(in.pos, m), 0.0);
    var out: VsOut;
    out.clip = camera.lamp_view_proj[k] * wp;
    out.world = wp.xyz;
    out.normal = in.normal;
    out.spec_sun = vec3<f32>(0.0);
    out.spec_sky = vec3<f32>(0.0);
    let pr = inst_params[e * 2u];
    out.uv = in.uv + pr.zw;
    out.params = pr;
    out.params2 = inst_params[e * 2u + 1u];
    if (pr.y < 0.5) {
        out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    }
    return out;
}

@fragment
fn fs_shadow(in: FsIn) {
}

// Reflection rays need the window surface as well as the opaque interior behind it.
// This writes a private hit-depth texture after colour compositing; ordinary depth and
// AO still leave glass transparent. Reject absent/fully faded layers and texture holes.
@fragment
fn fs_puddle_glass_depth(in: FsIn) {
    // From the cabin we see the street through our own pane. Treating that pane as
    // an SSR hit terminates every ray at the window instead of the outside vehicle.
    if (inside_vehicle(camera.cam_pos.xyz) * near_player_vehicle(in.world) > 0.5) {
        discard;
    }
    var a = diffuse_border(textureSampleBias(t_diffuse, s_diffuse, tex_address(in.uv), camera.lod.x), in.uv).a;
    if (material.params.z > 0.5) {
        let tm = sample_transmap(tex_address(in.uv - in.params.zw));
        a = select(1.0, tm.a, material.params.w > 0.5);
    }
    if (a * material.color.a * in.params.x < 0.002) {
        discard;
    }
}

// (the shadow and rain maps, drawn at their own size: their textures' mips as they are)
@fragment
fn fs_shadow_test(in: FsIn) {
    cutout_coverage(in.uv, in.params, 0.0);
}

// the depth-only passes' (`vs_depth`) fragment shaders: plain, cut out, a transmap's
// opaque part
@fragment
fn fs_depth(in: DepthIn) {
}

// (the picture's depth: its textures read with the colour pass's mip bias, `camera.lod` -
// a cut-out's edge the prepass took from a blurrier mip than the colour pass wrote depth
// where the colour then discarded, and the sky showed through)
@fragment
fn fs_depth_test(in: DepthIn) {
    cutout_coverage(in.uv, in.params, camera.lod.x);
}

// (a transmap's opaque part, see `transmap_coverage`)
@fragment
fn fs_depth_transmap(in: DepthIn) {
    transmap_coverage(in.uv, in.params, camera.lod.x);
}

// The multisampled prefill's cut-outs (foliage): their depth only where the colour pass
// is sure to cover every sample - the alpha-to-coverage of an alpha of 1 (`shade_enhanced`:
// the texture's alpha past the cutoff's smoothed edge, a plain material at full opacity,
// no pane, LED panel, transmap, ground or shadow blob). The edges, which alpha-to-coverage
// covers in part, are left to the colour pass: a sample laid here that the colour then
// left uncovered would hide what lies behind it (the sky showed through the houses).
@fragment
fn fs_depth_cover(in: DepthIn) {
    let mode = material.params.x;
    if (mode < 0.5 || mode > 1.5 || material.params.z > 0.5 || material.extra.x > 0.5
        || material.wipe_bounds.z != 0.0 || abs(material.emissive.w) > 1.5
        || material.color.a < 1.0 || in.params.x < 1.0 || in.surf > 1.5) {
        discard;
    }
    let duv = tex_address(in.uv);
    let a = diffuse_border(textureSampleBias(t_diffuse, s_diffuse, duv, camera.lod.x), duv).a;
    let aa = max(fwidth(a) * 0.5, 1.0 / 255.0);
    if (a < ALPHA_REF + aa + 4.0 / 255.0) {
        discard;
    }
}

// A cut-out's texel: kept where its alpha reaches the cutoff (`bias`: the mip bias its
// pass reads the textures with, the colour pass's for the picture's depth).
fn cutout_coverage(uv: vec2<f32>, params: vec4<f32>, bias: f32) {
    var duv = tex_address(uv);
    if (material.extra.x > 0.5) {
        duv = uv * material.extra.z;
    }
    // Use the light-pass footprint for cutout coverage. Forcing mip 0 here aliases dense
    // foliage and alpha layers into a checkerboard when the light projection moves by a
    // texel; the mip-aware sample keeps the caster coverage coherent with its resolution.
    // For a material promoted to the shadow pass from Blend, diffuse alpha is commonly a
    // paint/gloss value rather than coverage (vehicle bodies use values such as 0.03). Only
    // alpha-test materials use diffuse alpha as a cutout; blended bodies are solid unless a
    // real transmap supplies coverage.
    var a = select(diffuse_border(textureSampleBias(t_diffuse, s_diffuse, duv, bias), duv).a, 1.0, material.params.x > 1.5 && material.params.z < 0.5);
    if (material.params.z > 0.5) {
        // (the transmap stays where it is: [texcoordtransX/Y] only moves the diffuse stage)
        let tm = sample_transmap_bias(tex_address(uv - params.zw), bias);
        a = select(1.0, tm.a, material.params.w > 0.5);
    }
    if (a < ALPHA_REF) {
        discard;
    }
}

// Roads with feathered alpha borders stay blended while their overlaps compose.
// Once the surface phases are complete, only fully covered diffuse pixels occlude
// later scenery. The transparent borders must not become invisible depth walls.
@fragment
fn fs_surface_depth(in: FsIn) -> @location(0) vec4<f32> {
    let a = diffuse_border(textureSampleBias(t_diffuse, s_diffuse, tex_address(in.uv), camera.lod.x), in.uv).a
        * material.color.a * in.params.x;
    if (a < 0.999) {
        discard;
    }
    return vec4<f32>(0.0);
}

// A vehicle body and its windows are often one mesh/material. OMSI represents that
// material as alpha-blended because the transmap contains the window opacity, but the
// alpha-tested body pixels still have to occlude vehicles behind them. The normal depth prepass
// deliberately skips all blended materials, so those pixels would otherwise have no depth
// until the colour pass (where blend order can expose the bus through the car). Keep only
// the effectively opaque part of a transmap in a separate depth-only pass; window pixels
// remain out of the prepass and are composited normally.
fn transmap_coverage(uv: vec2<f32>, params: vec4<f32>, bias: f32) {
    if (material.params.z < 0.5) {
        discard;
    }
    let tm = sample_transmap_bias(tex_address(uv - params.zw), bias);
    let a = select(1.0, tm.a, material.params.w > 0.5) * params.x;
    // Only what the colour pass will cover completely may hide what lies behind it: a
    // texel that is merely more opaque than not (the dimmer and anti-aliased dots of a
    // display's text layer, whose transmap is its script texture) wrote depth here, the
    // display's backplate behind it was then rejected, and the half-transparent text was
    // blended over the sky - holes in the display.
    if (a < 0.99) {
        discard;
    }
}

// 1 = lit by the sun, 0 = in shadow. Three cascades: the close one (the bus and what is
// next to it), the near one around the camera and the far one covering the rest of the
// visible street.

