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

// the enhanced picture: what the flake scatters of the sky's, the sun's and the lamps'
// light, at the albedo of snow
@fragment
fn fs_snow_enhanced(in: SnowOut) -> @location(0) vec4<f32> {
    let a = flake_alpha(in);
    let to_eye = normalize(camera.cam_pos.xyz - in.pos);
    // (a flake hangs under the sky: its light from above as much as from all round)
    let l = (precip_light(in.pos, to_eye, true) + sh_irradiance(vec3<f32>(0.0, 0.0, 1.0)) / PI) * 0.5 * 0.85 * enh.exposure.x;
    return vec4<f32>(min(l, vec3<f32>(60.0)) * a, a);
}
