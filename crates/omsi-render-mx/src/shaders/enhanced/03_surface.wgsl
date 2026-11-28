// The relief a rough surface has under its microfacets - asphalt's grit and chippings, a
// paving stone's dressing, a cobble's dome (slopes of a quarter and more, at millimetres to
// centimetres) - shadows and masks itself (Smith's function for that slope spread,
// height-correlated) towards a light and an eye low over it. GGX's own roughness is the
// grain under that; on its own it let a low sun's grazing glint off every rough road and
// wall shine white towards the camera, where a real road keeps it dull. The spread grows
// with the rougher surface, and with the patch the pixel covers: farther off, the road's
// waves and ruts are under the pixel too. A smooth surface is left as it is.
fn relief_masking(nl: f32, nv: f32, rough: f32, footprint: f32) -> f32 {
    let s2 = 0.0625 * smoothstep(0.45, 0.85, rough) + 0.02 * smoothstep(0.01, 0.12, footprint) * smoothstep(0.45, 0.7, rough);
    if (s2 <= 0.0) {
        return 1.0;
    }
    // (Smith's Lambda for a slope variance s2: (sqrt(1 + s2 tan^2) - 1) / 2)
    let ml = clamp(nl, 1e-3, 1.0);
    let mv = clamp(nv, 1e-3, 1.0);
    let ll = 0.5 * (sqrt(1.0 + s2 * (1.0 - ml * ml) / (ml * ml)) - 1.0);
    let lv = 0.5 * (sqrt(1.0 + s2 * (1.0 - mv * mv) / (mv * mv)) - 1.0);
    return 1.0 / (1.0 + ll + lv);
}

// The light inside a cab relative to the average light outside: a bus's big windows let in
// most of the sky, and the dashboard lies right under the windscreen.
const CAB_AMBIENT: f32 = 1.15;
// With the traced global illumination (`rt_gi`): the even cabin light's share inside the
// cab (0.6 without it); the rest what the rays bring in through the windows and off the
// seats, the floor and the walls - at 0.6 the even light hid it.
const GI_CAB_SHARE: f32 = 0.15;

// How far the puddle threshold drops with the wetness: see the puddle mask in `shade_enhanced`.
const PUDDLE_SPREAD: f32 = 0.37;

// The water in the lanes' wheel tracks (`lane_ruts`: 0..1, how worn the road is there),
// on a road `wet` (0..1): x the cover of the water standing in the ruts, y how far the
// tyres have dried the wheel paths. A road soaked through holds a broken line of water in
// each rut (the traffic has worn them a few millimetres deep), which the tyres run through
// and throw up; as it dries, the wheels' paths dry first, lighter strips down each lane
// before the rest of the road. The same as `rut_water` in omsi-scene's puddles.rs (the
// spray), which keeps the numbers in step.
fn rut_water(world: vec3<f32>, ruts: f32, wet: f32) -> vec2<f32> {
    let breaks = vnoise_f(world_pattern_xy(world), 0.9, vec2<f32>(5.3, -2.2));
    let level = ruts * smoothstep(0.75, 0.95, wet) * (0.7 + 0.6 * breaks);
    return vec2<f32>(smoothstep(0.45, 0.55, level), ruts * (1.0 - smoothstep(0.6, 0.85, wet)));
}

// The dry map (passes/precip_map.rs, `DRY_RANGE` either side of its middle, seen from
// straight above): what stands over the ground round the camera, the vehicles and the cut-out
// leaves left out. `enh.glass_ext`: xy its middle, z its top (`DRY_DEPTH` over the middle),
// w 1 when there is one.
@group(0) @binding(23) var t_dry: texture_depth_2d;
const DRY_RANGE: f32 = 128.0;
const DRY_TEXELS: f32 = 1024.0;
const DRY_DEPTH: f32 = 150.0;

// How far the rain is kept off the ground at `world` (0 open sky .. 1 under a roof): a
// bridge, the roof of a depot, a shelter or a petrol station, a tunnel. The road under a
// bridge stood wet, with puddles and the rain's rings on them, because the ground was always
// taken for out in the weather (`weather_outside_n`). A place is covered where something
// stands at least 1.8 m over it; the edge of the dry patch is the map's filtered texels, a
// quarter of a metre, and a lamp post, a sign or a wire over the road keeps nearly nothing
// dry (the map leaves out what is small, see `ShadowPlan::dry`). (At 0.8 m the grass beside a
// pavement on an embankment went dry in a strip: the pavement's edge stands that high over
// the ground falling away below it.) Faded out towards the map's edge,
// where the wet road takes over again (from 109 m off).
fn roof_dry(world: vec3<f32>) -> f32 {
    let m = enh.glass_ext;
    let d = (world.xy - m.xy) / DRY_RANGE;
    let c = vec2<f32>(d.x * 0.5 + 0.5, 0.5 - d.y * 0.5);
    // (the map's depth of a roof 1.8 m over the point: what is drawn nearer is higher)
    let roof = (m.z - world.z - 1.8) / (2.0 * DRY_DEPTH);
    // (one look, through the comparison sampler as the sun's maps are read - 1 where nothing
    // stands that high, filtered over the four texels round the place, a quarter of a metre
    // each: more looks than one, or a plain load of the depth, brought Apple's M4 shader
    // compiler service down in the lean pipelines)
    let open = textureSampleCompareLevel(t_dry, s_shadow, c, roof);
    let edge = max(abs(d.x), abs(d.y));
    return (1.0 - open) * select(0.0, 1.0, m.w > 0.5) * (1.0 - smoothstep(0.82, 0.96, edge));
}

/// The mip level a pixel's footprint asks for, in levels of the texture whose size is
/// `texels` (the usual `log2` of the larger derivative, held at 0 and up). An LED panel is
/// sampled with this, held at `enh.led.y` (`Lighting::led_mips`): 0 point-samples it, which
/// is the sharpest and shimmers worst - a regular dot grid is the worst case for a point
/// sample - and every level the chain is allowed to take is a 2x2 average that a point
/// sample of the level below does not have. (The derivatives have to be taken in uniform
/// control flow: the panels are per draw, so a call inside the material's branch is not.)
fn led_lod(uv: vec2<f32>, texels: vec2<f32>) -> f32 {
    let dx = dpdx(uv) * texels;
    let dy = dpdy(uv) * texels;
    return max(0.5 * log2(max(dot(dx, dx), dot(dy, dy))), 0.0);
}

// A tangent-space normal `tn` (a PBR normal map's) turned into the world about the
// surface normal `n`, with the tangent frame taken from how the position and the uv change
// across the pixel (no tangents in OMSI's meshes). `tn.y` down the texture, as Direct3D's
// normal maps have it.
fn perturb_normal(n: vec3<f32>, p: vec3<f32>, uv: vec2<f32>, tn: vec3<f32>) -> vec3<f32> {
    let dp1 = dpdx(p);
    let dp2 = dpdy(p);
    let duv1 = dpdx(uv);
    let duv2 = dpdy(uv);
    let dp2perp = cross(dp2, n);
    let dp1perp = cross(n, dp1);
    // (the frame's handedness from the screen's: these are the uv's gradients times the
    // determinant of the screen-to-surface map, whose sign is the screen's y axis - down in
    // Direct3D, Metal and Vulkan. Normalised without it, both tangents came out reversed and
    // every normal map lit its bumps from the wrong side)
    let det = dot(dp1, dp2perp);
    let sgn = select(-1.0, 1.0, det >= 0.0);
    let t = (dp2perp * duv1.x + dp1perp * duv2.x) * sgn;
    let b = (dp2perp * duv1.y + dp1perp * duv2.y) * sgn;
    let m = max(dot(t, t), dot(b, b));
    if (m < 1e-20) {
        return n;
    }
    let k = inverseSqrt(m);
    return safe_normal(t * k * tn.x + b * k * tn.y + n * max(tn.z, 0.05));
}

// The enhanced pass's two targets: the picture, and the screen mask (r: 1 on the bus's own
// screens, carried by the coverage of what is drawn over them; g: 1 on an LED panel's own
// dots, 0.25 on other screens, see MASK_FORMAT; b is wet puddles' reflected-light weight).
struct EnhancedOut {
    @location(0) color: vec4<f32>,
    @location(1) mask: vec4<f32>,
    //RT @location(2) gbuf: vec4<f32>,
    //RT @location(3) aux: vec4<f32>,
};

// Enhanced+ (the window's picture, camera.clouds.w 2): the surface the traced reflections
// start from - its normal and how much of the reflection lands on the screen (w), its
// distance from the eye and its roughness, all times that weight (see GBUF_FORMAT). The
// probe's reflection is left out of the colour there: the traced one is added in its place.
var<private> rt_gbuf: vec4<f32>;
var<private> rt_aux: vec4<f32>;

fn enhanced_surface(in: FsIn) -> EnhancedOut {
    rt_gbuf = vec4<f32>(0.0);
    rt_aux = vec4<f32>(0.0);
    var puddle_weight = vec2<f32>(0.0);
    let c = shade_enhanced(in, &puddle_weight, false, camera.cam_pos.xyz);
    let screen = material.flags.x > 0.5;
    // an LED panel's dots stay in the glow's source (`post.wgsl`), the other screens'
    // letters stay out of it
    let led = select(0.0, 1.0, material.emissive.w < -1.5);
    var out: EnhancedOut;
    out.color = c;
    // Non-LED screens carry g = r / 4. The ratio survives a clear pane's coverage
    // blending, so a faded screen marker cannot be mistaken for condensation.
    // Water keeps its occluded sky weight in g and reflection weight in b.
    // A vehicle's shadow is light blocked from the road, not a new dry surface. Its
    // colour still blends normally, but it must preserve the road's reflection mask.
    // A transparent screen layer must leave the masks underneath intact. Otherwise
    // unused LED colour layers make the whole panel's background bloom as lit dots.
    let coverage = select(c.a, 0.0, in.params2.w > 1.5);
    // (r under 0.5: how much light the player's bus's misted glass scatters there, for the
    // tone mapping's blur - post.wgsl `misted`. The mask is blended by the coverage, so the
    // share is divided by it to land as MIST_MASK x the share over what lies behind;
    // a misted pane's coverage is at least half its share, see `shade_enhanced`)
    let mist = select(0.0, glass_fog * MIST_MASK / max(coverage, 1e-3), glass_fog > 0.001);
    let screen_code = select(0.25, 1.0, led > 0.5);
    out.mask = vec4<f32>(select(min(mist, 0.49), 1.0, screen), select(max(led, puddle_weight.y * 0.49), screen_code, screen), puddle_weight.x, coverage);
    //RT out.gbuf = rt_gbuf;
    //RT out.aux = rt_aux;
    return out;
}

@fragment
fn fs_enhanced(in: FsIn) -> EnhancedOut {
    return enhanced_surface(in);
}

// A headset uses mapped shadows, not screen-space ray reflections. Its colour
// pass needs only colour and the screen mask, even on an Enhanced+ renderer.
struct EnhancedColorOut {
    @location(0) color: vec4<f32>,
    @location(1) mask: vec4<f32>,
};

@fragment
fn fs_enhanced_color(in: FsIn) -> EnhancedColorOut {
    let surface = enhanced_surface(in);
    return EnhancedColorOut(surface.color, surface.mask);
}

// A self-lit picture (a display, a light-mapped surface lit fully) at its own colour after
// the tone curve (post.wgsl `natural_tone`): it bleaches what is brighter than its knee -
// a display's yellow-green text came out olive - and gives the middle tones the contrast
// of `enh.debug.w` about mid grey. So the colour is kept under the knee and the contrast
// undone in advance (with the small black offset the text has always been drawn with).
// A display's own brightness for the light round it: as a bus's screens dim at night, to
// 15 % of the day's, and never lifted over their own colour by the dark-adapted metering.
fn display_dim(lift: f32) -> f32 {
    let night = clamp(camera.sun_color.w, 0.0, 1.0);
    return mix(lift, min(lift, 1.0) * 0.15, night);
}

fn display_level(t: vec3<f32>) -> vec3<f32> {
    let peak = max(t.r, max(t.g, t.b));
    let tk = t * min(1.0, 0.64 / max(peak, 1e-3));
    let c = max(enh.debug.w, 1.0);
    let x = 0.18 * pow(tk / 0.18 + vec3<f32>(1e-7), vec3<f32>(1.0 / c));
    return x + 0.04 * smoothstep(vec3<f32>(0.0), vec3<f32>(0.08), x);
}

// [matl_glow] (see MaterialExtra::glow): the material is its own light. `buv` is where its
// mask is read (the light map's own coordinate, the slot it rides in) and `col` the colour
// the light is drawn in - the material's own, so a destination panel keeps the colour its
// display draws. The mask is a greyscale picture of how much shines where (white full, black
// none). The strength is the mod's own (the .cfg value x0.25, on the scale of the LED
// panels' `Led glow`) and is held against the metering as an LED panel's dots are; the
// glare round it comes from how bright it is, as every light's does (post.wgsl).
fn matl_glow_light(buv: vec2<f32>, col: vec3<f32>) -> vec3<f32> {
    let gm = textureSampleBias(t_light, s_diffuse, buv, camera.lod.x);
    return col * dot(gm.rgb, vec3<f32>(0.299, 0.587, 0.114)) * material.glow.x * max(enh.exposure.z * 2.0, 0.8);
}

// The colour (linear rgb, luminance 1) of a black body at `kelvin`: its chromaticity on the
// Planckian locus (Kim et al.'s cubic fit, 1667..25000 K, held at the ends), as
// `omsi_model::blackbody_rgb` computes it.
fn blackbody(kelvin: f32) -> vec3<f32> {
    let t = clamp(kelvin, 1667.0, 25000.0);
    let t2 = t * t;
    let t3 = t2 * t;
    var x = -3.0258469e9 / t3 + 2.1070379e6 / t2 + 0.2226347e3 / t + 0.240390;
    if (t <= 4000.0) {
        x = -0.2661239e9 / t3 - 0.2343589e6 / t2 + 0.8776956e3 / t + 0.179910;
    }
    let x2 = x * x;
    let x3 = x2 * x;
    var y = 3.0817580 * x3 - 5.8733867 * x2 + 3.75112997 * x - 0.37001483;
    if (t <= 2222.0) {
        y = -1.1063814 * x3 - 1.3481102 * x2 + 2.18555832 * x - 0.20219683;
    } else if (t <= 4000.0) {
        y = -0.9549476 * x3 - 1.37418593 * x2 + 2.09137015 * x - 0.16748867;
    }
    let xyz = vec3<f32>(x / y, 1.0, (1.0 - x - y) / y);
    let rgb = max(vec3<f32>(
        dot(xyz, vec3<f32>(3.2406, -1.5372, -0.4986)),
        dot(xyz, vec3<f32>(-0.9689, 1.8758, 0.0415)),
        dot(xyz, vec3<f32>(0.0557, -0.2040, 1.0570)),
    ), vec3<f32>(0.0));
    return rgb / max(dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722)), 1e-6);
}

// [matl_emissive] (openOMSI, `EmissionLight`): the material's own light, in HDR - it blooms
// by its brightness (post.wgsl) and keeps its colour however bright. `buv` is where its
// texture is read (the light map's coordinate, the slot it rides in), `base` the diffuse
// texture's colour it shines through without one, `slot` the instance's slot value (the
// level, or `-(K + level)` with a temperature: `omsi_model::slot_value`) and `pre` the
// frame's exposure. The artist's strength is on the scale of the screens and `[matl_glow]`
// (held against the metering as they are); a luminance in cd/m² is exposed as the scene is.
fn emission_light(buv: vec2<f32>, base: vec3<f32>, slot: f32, pre: f32) -> vec3<f32> {
    let bits = u32(material.emission.w + 0.5);
    var col = material.emission.rgb;
    var level = 1.0;
    if ((bits & 8u) != 0u) {
        if ((bits & 16u) != 0u && slot < 0.0) {
            col = blackbody(floor(-slot));
            level = min(fract(-slot) / 0.999, 1.0);
        } else {
            level = clamp(slot, 0.0, 1.0);
        }
    }
    var map = base;
    if ((bits & 2u) != 0u) {
        map = textureSampleBias(t_light, s_diffuse, buv, camera.lod.x).rgb;
    }
    let scale = select(max(enh.exposure.z * 2.0, 0.8), pre, (bits & 4u) != 0u) * material.glow.z;
    return col * map * level * scale;
}

// The road dirt on a vehicle's outside (openOMSI #1386, vehicle_dirt.rs): rgb its colour,
// a how much of the surface it covers (0..1), for a point `world` of a vehicle's part with
// the normal `n` (`pane`: a window, dirty on both faces). The vehicle is the one of the
// frame's dirty ones whose box holds the point. Only its outside takes dirt: surfaces near
// the box's sides or ends that face out of it, and the roof's dust - the saloon's walls face
// in, its seats and floor lie far inside. Where it lands: the spray off each wheel behind and
// above it, thinning with height; the road film all round low down and higher up the back,
// which the wake behind the vehicle coats; dust all over, the roof too. Broken up in
// splashes, run down in streaks where it is wet.
// (and how wet the dirt that `road_dirt` found is, 0..1)
var<private> dirt_wet: f32;
fn road_dirt(world: vec3<f32>, n: vec3<f32>, pane: bool) -> vec4<f32> {
    let count = min(i32(enh.dirt_n.x + 0.5), 64);
    for (var i = 0; i < count; i = i + 1) {
        let b = i * 8;
        let o = enh.dirt[b];
        let r = enh.dirt[b + 1];
        let f = enh.dirt[b + 2];
        let lo = enh.dirt[b + 3].xyz;
        let hi = enh.dirt[b + 4].xyz;
        let u = normalize(cross(r.xyz, f.xyz));
        let d = world - o.xyz;
        let p = vec3<f32>(dot(d, r.xyz), dot(d, f.xyz), dot(d, u));
        if (any(p < lo - vec3<f32>(0.15)) || any(p > hi + vec3<f32>(0.15))) {
            continue;
        }
        let ln = vec3<f32>(dot(n, r.xyz), dot(n, f.xyz), dot(n, u));
        let mid = 0.5 * (lo + hi);
        // facing out of the side or the end it is near (a pane: either face)
        var side_out = select(-ln.x, ln.x, p.x > mid.x);
        var end_out = select(-ln.y, ln.y, p.y > mid.y);
        if (pane) {
            side_out = abs(ln.x);
            end_out = abs(ln.y);
        }
        let to_side = min(p.x - lo.x, hi.x - p.x);
        let to_end = min(p.y - lo.y, hi.y - p.y);
        // (soft: an inset door or a rounded edge facing a little away takes nearly as much,
        // without a seam where one mesh meets the next)
        let on_side = smoothstep(0.65, 0.2, to_side) * smoothstep(-0.3, 0.2, side_out);
        let on_end = smoothstep(0.7, 0.25, to_end) * smoothstep(-0.3, 0.2, end_out);
        let on_roof = smoothstep(0.35, 0.1, hi.z - p.z) * smoothstep(0.6, 0.9, ln.z);
        let outside = max(on_side, on_end);
        // (not this one's outside: the next box may hold the point too - an articulated
        // bus's sections overlap at the joint, two vehicles parked nose to tail)
        if (outside + on_roof <= 0.0) {
            continue;
        }
        let am = enh.dirt[b + 6];
        let kind = enh.dirt[b + 7];
        let h = max(p.z - lo.z, 0.0);
        let radius = r.w;
        // the spray off the wheels: round the arch and behind it, low down
        var spray = 0.0;
        let axles = i32(o.w + 0.5);
        for (var k = 0; k < axles; k = k + 1) {
            let ay = enh.dirt[b + 5][k];
            let behind = ay - p.y;
            // (round the arch the tyre throws it up and back: little ahead of the wheel)
            let arch = smoothstep(radius * 2.0, radius * 1.05, length(vec2<f32>(behind, h - radius))) * smoothstep(-radius * 0.6, radius * 0.4, behind) * 0.7;
            let trail = smoothstep(-0.3, 0.5, behind) * exp(-max(behind, 0.0) / 2.2) * exp(-h / (radius * 2.4));
            spray = max(spray, max(arch, trail));
        }
        // the back: what the last wheels throw and the wake lay on it, reaching high
        let back = on_end * select(0.0, 1.0, p.y < mid.y);
        let film = exp(-h / 0.9) * outside + back * exp(-h / 1.8) * 0.75;
        spray = spray * on_side + back * exp(-h / 1.0) * 0.45;
        let dust = outside * (0.55 + 0.45 * exp(-h / 1.6)) + on_roof * 0.8;
        var c = clamp(am.x * spray * 1.2 + am.y * film + am.z * dust * 0.6, 0.0, 1.0);
        // splashes and, where it is wet, streaks running down
        let q = vec2<f32>(p.y + p.x * 0.7, p.z);
        let splash = vnoise_f(q, 6.0, vec2<f32>(3.7, 1.3)) * 0.45 + vnoise_f(q, 19.0, vec2<f32>(9.1, 4.4)) * 0.3 + vnoise_f(q, 57.0, vec2<f32>(2.2, 7.9)) * 0.25;
        let streak = vnoise_f(vec2<f32>(q.x * 9.0, q.y * 0.6), 1.0, vec2<f32>(5.3, 2.9));
        let grain = mix(splash, streak, am.w * 0.6);
        // a thin film where any lies, and crisp-edged splashes over it where it is thick
        let spots = smoothstep(grain * 0.85 - 0.03, grain * 0.85 + 0.03, c) * (0.5 + 0.35 * c);
        c = max(c * 0.5, spots);
        // ordinary grime grey-brown, salt and slush pale grey, mud dark brown; darker wet
        var col = vec3<f32>(0.20, 0.17, 0.13);
        col = mix(col, vec3<f32>(0.42, 0.42, 0.40), kind.x);
        col = mix(col, vec3<f32>(0.14, 0.10, 0.06), kind.y);
        col = col * mix(1.0, 0.6, am.w);
        dirt_wet = am.w;
        return vec4<f32>(col, c);
    }
    return vec4<f32>(0.0);
}

// How much of the light crossing the player's bus's pane its condensation scatters (0..1,
// 07_glass.wgsl `glass_condensation`), for the screen mask.
var<private> glass_fog: f32;
// The screen mask's r for a pane scattering all the light (post.wgsl `MIST_MASK`).
const MIST_MASK: f32 = 0.24;
// Heading, pitch and roll: with the heading alone the film slid along the glass on a hill (#3958).
fn pane_local(world: vec3<f32>) -> vec3<f32> {
    let d = world - camera.inside_a.xyz;
    let sh = camera.inside_a.w;
    let ch = camera.inside_b.x;
    let v = vec3<f32>(d.x * ch - d.y * sh, d.x * sh + d.y * ch, d.z);
    let t = enh.pane_tilt;
    let p = vec3<f32>(v.x, v.y * t.y + v.z * t.x, v.z * t.y - v.y * t.x);
    let b = vec3<f32>(p.x * t.w - p.z * t.z, p.y, p.z * t.w + p.x * t.z);
    return b - camera.inside_c.xyz;
}


// Parallax occlusion mapping (a PBR set's height map, `foo_height`: in the ORM map's alpha,
// white high): the uv the view ray meets the relief at, marched down into it from the
// surface. The tangent frame and the texture's scale in the world come from the pixel's
// derivatives (OMSI's meshes have no tangents); the relief is `POM_DEPTH` metres deep, so a
// cobble or a brick joint is as deep on a coarse texture as on a fine one.
const POM_DEPTH: f32 = 0.025;
fn parallax_uv(uv: vec2<f32>, p: vec3<f32>, n: vec3<f32>, v: vec3<f32>) -> vec2<f32> {
    let dp1 = dpdx(p);
    let dp2 = dpdy(p);
    let duv1 = dpdx(uv);
    let duv2 = dpdy(uv);
    let dp2perp = cross(dp2, n);
    let dp1perp = cross(n, dp1);
    let det = dot(dp1, dp2perp);
    if (abs(det) < 1e-12) {
        return uv;
    }
    // the uv's gradient in the world (how far u and v move per metre along the surface)
    let gu = (dp2perp * duv1.x + dp1perp * duv2.x) / det;
    let gv = (dp2perp * duv1.y + dp1perp * duv2.y) / det;
    let vn = dot(v, n);
    if (vn <= 0.05) {
        return uv;
    }
    // (per metre down into the relief, the ray moves this far over the uv; grazing views
    // are held so that the march does not smear the texture along the surface)
    let slide = -(v - n * vn) / max(vn, 0.25);
    let per_m = vec2<f32>(dot(slide, gu), dot(slide, gv));
    let steps = mix(24.0, 8.0, vn);
    let dz = POM_DEPTH / steps;
    var depth = 0.0;
    var q = uv;
    var prev_gap = 0.0;
    var prev_q = uv;
    for (var i = 0; i < 24; i = i + 1) {
        if (f32(i) >= steps) {
            break;
        }
        let h = textureSampleGrad(t_pbr_orm, s_diffuse, q, duv1, duv2).a;
        let gap = (1.0 - h) * POM_DEPTH - depth;
        if (gap <= 0.0) {
            // between the last step above the relief and this one under it
            let t = prev_gap / max(prev_gap - gap, 1e-6);
            return mix(prev_q, q, t);
        }
        prev_gap = gap;
        prev_q = q;
        depth = depth + dz;
        q = uv + per_m * depth;
    }
    return q;
}

// The PBR set's normal map flag without the height map's mark (see omsi_texture::pbr).
fn pbr_normal_flag() -> f32 {
    return select(material.pbr.x, material.pbr.x - 4.0, material.pbr.x > 3.5);
}
