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

// How far the puddle threshold drops with the wetness: see the puddle mask in `shade_enhanced`.
const PUDDLE_SPREAD: f32 = 0.37;

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
    let t = dp2perp * duv1.x + dp1perp * duv2.x;
    let b = dp2perp * duv1.y + dp1perp * duv2.y;
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

@fragment
fn fs_enhanced(in: FsIn) -> EnhancedOut {
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

// How much of the light crossing the player's bus's pane at `world` its condensation
// scatters (0..1): 1 - e^-depth of the windscreen's, the side windows' or the rear
// window's film (enh.condensation, optical depths from omsi-app condensation.rs), the film
// heavier low down, where the glass is coldest, and in patches; on the windscreen cleared
// in a widening fan above the defroster.
var<private> glass_fog: f32;
// The screen mask's r for a pane scattering all the light (post.wgsl `MIST_MASK`).
const MIST_MASK: f32 = 0.24;
fn pane_condensation(world: vec3<f32>) -> f32 {
    if (camera.inside_c.w < 0.5 || near_player_vehicle(world) < 0.5) {
        return 0.0;
    }
    let l = fn_box_local(world);
    let h = camera.inside_b.yzw;
    let fy = l.y / max(h.y, 0.1);
    let fz = l.z / max(h.z, 0.1);
    let fx = l.x / max(h.x, 0.1);
    // (the panes in the body's skin only - not the instruments' glass behind the windscreen
    // or a partition in the saloon)
    var f = 0.0;
    var front = false;
    if (fy > 0.96) {
        f = enh.condensation.x;
        front = true;
    } else if (fy < -0.96) {
        f = enh.condensation.z;
    } else if (abs(fx) > 0.8) {
        f = enh.condensation.y;
    }
    if (f <= 0.001) {
        return 0.0;
    }
    // (cold low down and along the frames; patches of a hand's breadth)
    let low = mix(1.2, 0.8, clamp(fz * 0.5 + 0.5, 0.0, 1.0));
    let q = select(vec2<f32>(l.y, l.z), vec2<f32>(l.x, l.z), front || fy < -0.96) * 4.0;
    let cq = floor(q);
    let fq = q - cq;
    let w = fq * fq * (3.0 - 2.0 * fq);
    let hh = fract(sin(vec4<f32>(dot(cq, vec2<f32>(12.9898, 78.233)), dot(cq + vec2<f32>(1.0, 0.0), vec2<f32>(12.9898, 78.233)), dot(cq + vec2<f32>(0.0, 1.0), vec2<f32>(12.9898, 78.233)), dot(cq + vec2<f32>(1.0, 1.0), vec2<f32>(12.9898, 78.233)))) * 43758.547);
    let patchy = mix(mix(hh.x, hh.y, w.x), mix(hh.z, hh.w, w.x), w.y);
    var amount = 1.0 - exp(-f * low * (0.75 + 0.5 * patchy));
    if (front) {
        // the defroster's warm air clears a fan from the bottom of the windscreen
        let d = length(vec2<f32>(l.x / max(h.x, 0.1), (fz + 0.2) * 1.6));
        let clear = 1.0 - smoothstep(enh.condensation.w * 1.3 - 0.25, enh.condensation.w * 1.3, d);
        amount = amount * (1.0 - clear * step(0.02, enh.condensation.w));
    }
    return amount;
}

