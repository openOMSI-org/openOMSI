// The snowfall, every mode: a field of flakes that lives on the GPU alone - each flake's
// place is worked out here from its number, the time, the wind and the camera, so that a
// heavy fall is a hundred and fifty thousand flakes for the cost of one draw (nothing is
// simulated or sent per flake). The field is the world's, repeating every few metres round
// the camera in three layers - dense close by, where every flake is a few pixels wide,
// thinner further out, where it is a speck; beyond them the snowfall's extinction takes the
// view (Enhanced). Flakes are a few millimetres across (an exponential spread, Gunn and
// Marshall 1958; a light fall single crystals of a millimetre), fall at about a metre a
// second (aggregates at 0.8 D^0.16 m/s, Locatelli and Hobbs 1974), sway as they go and
// drift with the wind; opaque and lit by what lights them - the classic picture's
// envir.cfg light, or the enhanced picture's sky, sun and lamps (`precip_light`).

struct SnowParams {
    // xyz the wind the flakes drift with (m/s), w the time (s)
    wind: vec4<f32>,
    // x how hard it snows (0..1), y the flakes' mean diameter (m), zw how many flakes the
    // first two layers hold (the rest are the third's)
    fall: vec4<f32>,
    // the precipitation's roof map: its matrix, and x 1 when there is one
    roof: mat4x4<f32>,
    roof_on: vec4<f32>,
    // Enhanced: x the instance the veil's panels begin at (the flakes' end), y how many
    // panels there are, z how strong the wind's gusts are (0..1), w the picture's height (px)
    ext: vec4<f32>,
};
@group(1) @binding(0) var<uniform> sp: SnowParams;

struct SnowOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // the flake's cover (alpha), and its place (for its light)
    @location(1) cover: f32,
    @location(2) pos: vec3<f32>,
};

// The layers round the camera: half width, bottom and top (m).
const LAYER_H = array<vec3<f32>, 3>(vec3<f32>(4.0, -2.5, 5.5), vec3<f32>(10.0, -3.0, 9.0), vec3<f32>(24.0, -3.0, 16.0));

@vertex
fn vs_snow(@builtin(vertex_index) vid: u32, @builtin(instance_index) k: u32) -> SnowOut {
    let corners = array<vec2<f32>, 6>(vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));
    let c = corners[vid % 6u];
    var out: SnowOut;
    out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    let n0 = u32(sp.fall.z);
    let n1 = u32(sp.fall.w);
    var layer = 2u;
    if (k < n0) {
        layer = 0u;
    } else if (k < n0 + n1) {
        layer = 1u;
    }
    let lh = LAYER_H[layer];
    let size = vec3<f32>(2.0 * lh.x, 2.0 * lh.x, lh.z - lh.y);
    let h1 = hash4(k * 2u + 1u);
    let h2 = hash4(k * 2u + 7919u);
    // the flake: its diameter, fall speed and sway
    let r = sp.fall.x;
    let d = min(0.0005 - sp.fall.y * log(max(1.0 - h1.w, 1e-6)), 0.015);
    let fall = max(0.8 * pow(d * 1000.0, 0.16) * (0.75 + 0.25 * r) * (0.85 + 0.3 * h2.x), 0.3);
    let t = sp.wind.w;
    let pace = 1.5 + 2.5 * h2.y;
    let sway = 0.05 + 0.15 * h2.z;
    let ph = h2.w * 6.2831853 + pace * t;
    // its place in the world: where it started, carried by the wind, swaying round, falling;
    // then the copy of the field nearest the camera (the field's figures in the world's
    // own, modulo a kilometre, so that it stays put while the render origin moves)
    let world_cam = camera.cam_pos.xyz + vec3<f32>(camera.world_origin.xy, 0.0);
    let start = h1.xyz * size;
    let drift = sp.wind.xyz * (0.6 * t) + vec3<f32>(cos(ph), sin(ph), 0.0) * sway - vec3<f32>(0.0, 0.0, fall * t);
    let lo = world_cam + vec3<f32>(-lh.x, -lh.x, lh.y);
    let q = lo + (((start + drift - lo) % size) + size) % size;
    let p = q - vec3<f32>(camera.world_origin.xy, 0.0);
    if (in_own_vehicle(p) || under_roof(p, sp.roof, sp.roof_on.x)) {
        return out;
    }
    // a denser layer fades out towards its edges, into the thinner one round it
    let rel = q - lo;
    let soft = lh.x * 0.3;
    var fade = 1.0;
    if (layer < 2u) {
        let e = min(rel, size - rel) / soft;
        fade = clamp(min(min(e.x, e.y), e.z), 0.0, 1.0);
    }
    let to_cam = camera.cam_pos.xyz - p;
    let dist = length(to_cam);
    let vdir = to_cam / max(dist, 0.001);
    // a flake close by is out of focus (its blur as wide as the pupil, some 2.5 mm), one far
    // off at least a pixel and a half wide; as bright as it is seen - a small point's
    // brightness goes with the square root of its light (Stevens' power law)
    let rad = d * 0.5;
    let rs = max(sqrt(rad * rad + 0.0025 * 0.0025), dist * 0.0016);
    out.cover = fade * sqrt(rad / rs);
    if (out.cover < 0.004) {
        return out;
    }
    var ax = cross(vec3<f32>(0.0, 0.0, 1.0), vdir);
    ax = select(vec3<f32>(1.0, 0.0, 0.0), normalize(ax), length(ax) > 1e-4);
    let ay = cross(vdir, ax);
    out.clip = camera.view_proj * vec4<f32>(p + (ax * c.x + ay * c.y) * rs, 1.0);
    out.uv = c;
    out.pos = p;
    return out;
}

fn flake_alpha(in: SnowOut) -> f32 {
    let r = length(in.uv);
    return clamp(in.cover * (1.0 - smoothstep(0.55, 1.0, r)), 0.0, 1.0);
}

// the classic picture: white, lit as the scene is (envir.cfg's ambient and sun light)
@fragment
fn fs_snow(in: SnowOut) -> @location(0) vec4<f32> {
    let a = flake_alpha(in);
    let light = min(camera.ambient.rgb + camera.sun_color.rgb * 0.5, vec3<f32>(1.0));
    return vec4<f32>(light * 0.9 * a, a);
}


// ---- The enhanced snowfall
//
// The same field of flakes, each drawn as what it is (precip.rs `FlakeAtlas`): in a light
// fall single crystals - stellar and fernlike dendrites, sectored and plain hexagonal
// plates - in a heavier one aggregates of many, a centimetre across. A flake turns as it
// falls and a plate tips over and back (seen edge-on it thins to a line), flutters on two
// periods of its own besides its slow sway, and is blown on by the wind's gusts with its
// neighbours. Close to the eye it is out of focus, a soft disc; far off a speck. A plate
// turned to the sun flashes for a moment. Lit as the classic snowfall is by the sky and the
// lamps, the sun's share shaded by the far shadow map and thrown a little forwards; beyond
// the flakes' layers a heavy fall is a speckled veil (`veil_corner`).

struct SnowOutE {
    @builtin(position) clip: vec4<f32>,
    // across the quad (-1..1 each way); the veil: the panel's corner (0..1)
    @location(0) uv: vec2<f32>,
    // x the cover, y the atlas cell (-1 the veil), z its mip level, w how far out of focus
    // (0 sharp .. 1 a soft disc)
    @location(1) @interpolate(flat) shape: vec4<f32>,
    @location(2) pos: vec3<f32>,
    // the flake's turn (cos, sin), how far it faces the eye (1 face on), how much it glints
    @location(3) @interpolate(flat) turn: vec4<f32>,
    // x the quad's radius over the flake's, y the veil's radius (m), z its ring
    @location(4) @interpolate(flat) extra: vec4<f32>,
};

fn snow_none() -> SnowOutE {
    var out: SnowOutE;
    out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    out.uv = vec2<f32>(0.0);
    out.shape = vec4<f32>(0.0);
    out.pos = vec3<f32>(0.0);
    out.turn = vec4<f32>(1.0, 0.0, 1.0, 0.0);
    out.extra = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    return out;
}

@vertex
fn vs_snow_enhanced(@builtin(vertex_index) vid: u32, @builtin(instance_index) k: u32) -> SnowOutE {
    let corners = array<vec2<f32>, 6>(vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));
    let c = corners[vid % 6u];
    if (k >= u32(sp.ext.x)) {
        return snow_veil(c * 0.5 + vec2<f32>(0.5), k - u32(sp.ext.x));
    }
    var out = snow_none();
    let n0 = u32(sp.fall.z);
    let n1 = u32(sp.fall.w);
    let layer = select(select(2u, 1u, k < n0 + n1), 0u, k < n0);
    let lh = LAYER_H[layer];
    let size = vec3<f32>(2.0 * lh.x, 2.0 * lh.x, lh.z - lh.y);
    let h1 = hash4(k * 2u + 1u);
    let h2 = hash4(k * 2u + 7919u);
    let h3 = hash4(k * 5u + 15485863u);
    // the flake: its diameter and fall speed as `vs_snow` has them - but a heavy fall's
    // aggregates bigger: half a centimetre across on the mean, the big wet flakes of a
    // thick fall up to two (the classic sizes, two millimetres, are those of a moderate
    // fall's single crystals, and a heavy fall showed as a fine dust)
    let r = sp.fall.x;
    let mean = max(sp.fall.y, 0.0012 + 0.0062 * smoothstep(0.2, 0.9, r));
    let d = min(0.0005 - mean * log(max(1.0 - h1.w, 1e-6)), 0.02);
    let fall = max(0.8 * pow(d * 1000.0, 0.16) * (0.75 + 0.25 * r) * (0.85 + 0.3 * h2.x), 0.3);
    let t = sp.wind.w;
    let world_cam = camera.cam_pos.xyz + vec3<f32>(camera.world_origin.xy, 0.0);
    let lo = world_cam + vec3<f32>(-lh.x, -lh.x, lh.y);
    let base = sp.wind.xyz * (0.6 * t) - vec3<f32>(0.0, 0.0, fall * t);
    let q0 = lo + (((h1.xyz * size + base - lo) % size) + size) % size;
    // its sway, a slow circle, and its flutter, quicker and smaller, up and down as well;
    // and the gusts
    let ph = h2.w * 6.2831853 + (1.5 + 2.5 * h2.y) * t;
    let ph2 = h3.x * 6.2831853 + (4.0 + 3.0 * h3.y) * t;
    let sway = vec3<f32>(cos(ph), sin(ph), 0.0) * (0.05 + 0.15 * h2.z);
    let flutter = vec3<f32>(cos(ph2), sin(1.3 * ph2), 0.5 * sin(0.7 * ph2)) * (0.015 + 0.03 * h3.z);
    let g = precip_gust(q0.xy, t, sp.wind.xy, sp.ext.z);
    let q = q0 + sway + flutter + vec3<f32>(sp.wind.xy * (0.6 * g.x), 0.0);
    let p = q - vec3<f32>(camera.world_origin.xy, 0.0);
    if (in_own_vehicle(p)) {
        return out;
    }
    // under a roof none; one about to land on the ground or a roof fades into it (the
    // surface would cut it in half)
    let clear = roof_clearance(p, sp.roof, sp.roof_on.x);
    if (clear < -0.05) {
        return out;
    }
    let landing = smoothstep(-0.03, 0.04, clear);
    // every layer fades out at its edges, where its flakes come round again
    let rel = q0 - lo;
    let e = min(rel, size - rel) / (lh.x * 0.3);
    let fade = clamp(min(min(e.x, e.y), e.z), 0.0, 1.0) * landing;
    let to_cam = camera.cam_pos.xyz - p;
    let dist = length(to_cam);
    let vdir = to_cam / max(dist, 0.001);
    let ch = camera.view_proj * vec4<f32>(p, 1.0);
    if (fade < 0.004 || ch.w < 0.05 || abs(ch.x) > 1.3 * ch.w || abs(ch.y) > 1.3 * ch.w) {
        return out;
    }
    let cu = camera.view_proj * vec4<f32>(p + camera.cam_up.xyz, 1.0);
    let px_per_m = max(abs(cu.y / cu.w - ch.y / ch.w) * 0.5 * sp.ext.w, 1e-3);
    // In focus a few metres off (the street the eye is on): nearer, a flake's blur grows to
    // a few millimetres (as a camera's lens draws it: a big flake right in front of the eye
    // is a soft blot, not a sharp star); it is a pixel and a half wide at the least, and as
    // bright as `vs_snow` has it.
    let rad = d * 0.5;
    let blur = 0.004 * clamp(1.0 - dist / 5.0, 0.0, 1.0);
    let rs = max(sqrt(rad * rad + blur * blur), 1.5 / px_per_m);
    let cover = fade * sqrt(rad / rs);
    if (cover < 0.004) {
        return out;
    }
    let sharp = rad / rs;
    // single crystals in a light fall, the bigger flakes aggregates of them
    let aggregate = h3.w < smoothstep(0.0012, 0.004, d);
    let cell = select(floor(h3.z * 3.99), 4.0 + floor(fract(h3.z * 7.0) * 3.99), aggregate);
    // turning, and tipping over and back (an aggregate hardly shows it)
    let spin = (h3.y - 0.5) * 5.0;
    let ang = h3.x * 6.2831853 + spin * t;
    let tip = cos((0.8 + 2.2 * h2.x) * t + h1.x * 6.2831853);
    let face = select(0.25 + 0.75 * abs(tip), 0.8 + 0.2 * abs(tip), aggregate);
    let glint = pow(0.5 + 0.5 * sin((2.0 + 3.0 * h2.x) * t + h2.w * 20.0), 60.0) * select(1.0, 0.25, aggregate);
    // the atlas's level: the flake's 58 texels over its pixels
    let level = clamp(log2(58.0 / max(2.0 * rad * px_per_m, 1e-3)), 0.0, 4.0);
    var ax = cross(vec3<f32>(0.0, 0.0, 1.0), vdir);
    ax = select(vec3<f32>(1.0, 0.0, 0.0), normalize(ax), length(ax) > 1e-4);
    let ay = cross(vdir, ax);
    out.clip = camera.view_proj * vec4<f32>(p + (ax * c.x + ay * c.y) * rs, 1.0);
    out.uv = c;
    out.shape = vec4<f32>(cover, cell, level, 1.0 - smoothstep(0.35, 0.9, sharp));
    out.pos = p;
    out.turn = vec4<f32>(cos(ang), sin(ang), face, glint);
    out.extra = vec4<f32>(1.0 / max(sharp, 1e-3), 0.0, 0.0, 0.0);
    return out;
}

fn snow_veil(c: vec2<f32>, panel: u32) -> SnowOutE {
    var out = snow_none();
    let v = veil_corner(panel, c);
    let ring = f32(panel / 12u);
    if (sp.fall.x < 0.08) {
        return out;
    }
    out.clip = camera.view_proj * vec4<f32>(v.xyz, 1.0);
    out.uv = c;
    out.shape = vec4<f32>(pow(sp.fall.x, 1.2) * 0.11 * (0.8 + 0.2 * ring), -1.0, 0.0, 0.0);
    out.pos = v.xyz;
    out.extra = vec4<f32>(1.0, v.w, ring, 0.0);
    return out;
}

fn flake_alpha_e(in: SnowOutE) -> vec2<f32> {
    // the flake's own figure, turned and tipped, the crystal's rim at 1
    let u = vec2<f32>(in.turn.x * in.uv.x - in.turn.y * in.uv.y, in.turn.y * in.uv.x + in.turn.x * in.uv.y);
    let f = vec2<f32>(u.x, u.y / max(in.turn.z, 0.05)) * in.extra.x;
    let cell = vec2<f32>(in.shape.y % 4.0, floor(in.shape.y / 4.0));
    let cuv = clamp(vec2<f32>(0.5) + f * 0.45, vec2<f32>(0.0), vec2<f32>(1.0));
    let tex = textureSampleLevel(t_flakes, s_flakes, (cell + cuv) * vec2<f32>(0.25, 0.5), in.shape.z);
    let crisp = select(0.0, tex.r, all(abs(f) < vec2<f32>(1.1)));
    // out of focus: a soft disc, a touch brighter at its rim, as a lens draws a point
    let rr = length(in.uv);
    let disc = (1.0 - smoothstep(0.55, 1.0, rr)) * (0.85 + 0.15 * smoothstep(0.3, 0.8, rr));
    let a = mix(crisp, disc, in.shape.w) * in.shape.x;
    return vec2<f32>(clamp(a, 0.0, 1.0), mix(tex.g, 1.0, in.shape.w));
}

fn veil_snow_alpha(in: SnowOutE) -> f32 {
    let r = in.extra.y;
    let ring = in.extra.z;
    let d = in.pos - camera.cam_pos.xyz;
    let theta = atan2(d.y, d.x);
    let t = sp.wind.w;
    let tang = vec2<f32>(-sin(theta), cos(theta));
    // The flakes beyond, each a speck a pixel or two across, falling a metre a second and
    // drifting with the wind: two lattices of specks, a fixed angle apart, each cell with a
    // flake of its own somewhere in it - more of them showing the harder it snows.
    var specks = 0.0;
    for (var j = 0u; j < 2u; j = j + 1u) {
        let fj = f32(j);
        let cols = round((900.0 + 150.0 * ring) * (1.0 + 0.41 * fj));
        let cw = 6.2831853 * r / cols;
        let drift = dot(sp.wind.xy, tang) * 0.6 * t;
        let x = vec2<f32>(theta / 6.2831853 * cols - drift / cw, (in.pos.z + (0.9 + 0.3 * fj) * t) / cw) + vec2<f32>(0.5 * fj);
        let ci = floor(x);
        let h = cell_hash(ci + vec2<f32>(311.0 * ring, 57.0 * fj));
        let centre = vec2<f32>(0.2) + 0.6 * h.xy + 0.08 * vec2<f32>(sin(t * 1.7 + h.z * 30.0), 0.0);
        let s = (1.0 - smoothstep(0.08, 0.22, length(fract(x) - centre))) * step(h.w, 0.35 + 0.5 * sp.fall.x);
        specks = max(specks, s * (0.5 + 0.5 * h.z));
    }
    let g = precip_gust(in.pos.xy + camera.world_origin.xy, t, sp.wind.xy, sp.ext.z);
    // (as the rain's veil, rain.wgsl `veil_rain_alpha`: gentler gusts, kept to the horizon, and
    // little flat grey under the specks - looked up at, the panels stood in the sky as a ring
    // of pale sheets)
    let sweep = clamp(1.0 + 0.6 * g.y, 0.7, 1.3);
    let up = in.pos.z - camera.cam_pos.z;
    let height = smoothstep(-3.0, 3.0, up) * (1.0 - smoothstep(0.08, 0.3, up / r));
    return clamp(in.shape.x * (0.2 + 0.8 * specks) * sweep * height, 0.0, 1.0);
}

// the enhanced picture: what the flake scatters of the sky's, the sun's and the lamps'
// light, at the albedo of snow
@fragment
fn fs_snow_enhanced(in: SnowOutE) -> @location(0) vec4<f32> {
    let veil = in.shape.y < -0.5;
    var a = 0.0;
    var bright = 1.0;
    if (veil) {
        a = veil_snow_alpha(in);
    } else {
        let fa = flake_alpha_e(in);
        a = fa.x;
        bright = fa.y;
    }
    if (a < 0.002) {
        discard;
    }
    let to_eye = normalize(camera.cam_pos.xyz - in.pos);
    let sun = precip_sun(in.pos);
    // the sun's share: `precip_light` takes it as even and unshaded; here it is shaded, and
    // thrown a little forwards (a fall lit from behind is brighter), and a tipping plate
    // flashes as it faces it
    let c = dot(-camera.sun_dir.xyz, to_eye);
    let lobe = 0.7 * hg_phase(c, 0.35) * 4.0 * PI + 0.3;
    let sun_part = enh.sun.rgb * (0.25 / PI * (sun * lobe - 1.0) + sun * in.turn.w * 0.6);
    let up = sh_irradiance(vec3<f32>(0.0, 0.0, 1.0)) / PI;
    var l = vec3<f32>(0.0);
    if (veil) {
        l = (enh.fog_color.rgb / 0.9 + enh.sun.rgb * 0.25 / PI + up) * 0.5 * 0.85 + sun_part * 0.425;
    } else {
        l = (precip_light(in.pos, to_eye, true) + up) * 0.5 * 0.85 + sun_part * 0.425;
    }
    l = max(l, vec3<f32>(0.0)) * mix(0.75, 1.05, bright);
    return vec4<f32>(min(l * enh.exposure.x, vec3<f32>(60.0)) * a, a);
}
