// The rainfall, every mode: a field of drops that lives on the GPU alone, as the snowfall
// does (snow.wgsl) - each drop's place is worked out here from its number, the time, the
// wind and the camera, so that a downpour is a quarter of a million drops for one draw.
// The field is the world's, repeating round the camera in three layers, dense close by,
// thinner further out; beyond them the rain's extinction takes the view (the fog).
//
// A drop is seen as the streak it draws in the eye's time (`ep.eye.w`): its way against
// the eye in that time, short and upright standing, long and slanting at a moving bus's
// windscreen. It is as wide as the drop is - a raindrop of two millimetres is less than a
// pixel wide a few metres off - so a streak is drawn a pixel wide at least, as transparent
// as the drop covers less of it: a downpour is a veil of many fine, faint lines, not a few
// thick ones. The drops are drawn as rain shows them, weighted by the light they catch
// (their cross-section) from Marshall and Palmer's sizes (a gamma of order 3 over the
// exponential), each falling at its own speed (Atlas, Srivastava and Sekhon 1973).

struct RainParams {
    // xyz the wind the drops drift with (m/s), w the time (s)
    wind: vec4<f32>,
    // xyz the eye's velocity (m/s, world), w the time the eye sees a drop as one streak (s)
    eye: vec4<f32>,
    // x how hard it rains (0..1), y the median volume diameter of its drops (m), zw how
    // many drops the first two layers hold (the rest are the third's)
    fall: vec4<f32>,
    // xy the picture's size (px), z how opaque a drop's streak is, w how many of the
    // instances are drops (the rest are their splashes)
    view: vec4<f32>,
    // the precipitation's roof map: its matrix, and x 1 when there is one
    roof: mat4x4<f32>,
    roof_on: vec4<f32>,
};
@group(1) @binding(0) var<uniform> rp: RainParams;

struct RainOut {
    @builtin(position) clip: vec4<f32>,
    // across the streak and along it from its tail (px)
    @location(0) uv: vec2<f32>,
    // x the streak's half width, y its length (px), z its cover (alpha)
    @location(1) @interpolate(flat) shape: vec3<f32>,
    @location(2) pos: vec3<f32>,
};

// The layers round the camera: half width, bottom and top (m).
const RAIN_LAYERS = array<vec3<f32>, 3>(vec3<f32>(5.0, -2.5, 6.0), vec3<f32>(14.0, -3.0, 12.0), vec3<f32>(32.0, -3.0, 22.0));

// Where a drop lands: a crown of water a centimetre or two high that is gone in a tenth of
// a second - on whatever is uppermost there, the ground, a kerb, a bus's roof, as the
// precipitation's roof map has it (none without the map). Each splash takes a new place
// round the camera every time it comes round again; the most of them close by.
fn splash(vid: u32, s: u32) -> RainOut {
    // (a strip of four vertices)
    let c = vec2<f32>(f32(i32(vid & 1u) * 2 - 1), f32(i32(vid >> 1u) * 2 - 1));
    var out: RainOut;
    out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    out.uv = vec2<f32>(0.0);
    out.shape = vec3<f32>(0.0);
    out.pos = vec3<f32>(0.0);
    let life = 0.1;
    let h0 = hash4(s * 3u + 11u);
    let cycle = rp.wind.w / life + h0.w;
    let age = fract(cycle);
    let h = hash4(s * 7919u + u32(floor(cycle)) * 2654435761u);
    // its place in the world: a field 28 m square laid over the world (in the world's own
    // figures), the copy of it round the camera - a splash stays where it landed however
    // the camera moves or turns, and only those the camera leaves behind come round again
    // ahead of it
    let size = 28.0;
    let world_cam = camera.cam_pos.xy + camera.world_origin.xy;
    let lo = world_cam - vec2<f32>(0.5 * size);
    let wxy = lo + (((h.xy * size - lo) % size) + size) % size;
    let xy = wxy - camera.world_origin.xy;
    // (fewer further off, faded in and out by the distance alone)
    let fade_far = 1.0 - smoothstep(6.0, 14.0, length(xy - camera.cam_pos.xy));
    let m = rp.roof;
    let mc = m * vec4<f32>(xy, 0.0, 1.0);
    let uv = vec2<f32>(mc.x * 0.5 + 0.5, 0.5 - mc.y * 0.5);
    let side = f32(textureDimensions(roof_map).x);
    let top = roof_depth(vec2<i32>(clamp(uv, vec2<f32>(0.0), vec2<f32>(0.9999)) * side));
    // (the map's depth back to a height: it looks straight down)
    let z = (top - mc.z) / m[2].z;
    let p = vec3<f32>(xy, z + 0.008 + 0.012 * age);
    let ch = camera.view_proj * vec4<f32>(p, 1.0);
    let cu = camera.view_proj * vec4<f32>(p + camera.cam_up.xyz, 1.0);
    if (rp.roof_on.x < 0.5 || top >= 0.9999 || ch.w < 0.3 || in_own_vehicle(p)) {
        return out;
    }
    let half_px = 0.5 * rp.view.xy;
    let px_per_m = length(cu.xy / cu.w * half_px - ch.xy / ch.w * half_px);
    // the crown widens as it rises and falls back, and fades
    let w = (0.006 + 0.02 * age) * px_per_m;
    let hw = max(0.5 * w, 0.5);
    let pad = hw + 1.0;
    let cover = min(w, 1.0) * (1.0 - age) * (1.0 - age) * 0.8 * rp.view.z * fade_far;
    out.clip = vec4<f32>(ch.xy + c * pad / half_px * ch.w, ch.zw);
    out.uv = c * pad;
    // (a round speck: no length, see `streak_alpha`)
    out.shape = vec3<f32>(hw, -1.0, cover);
    out.pos = p;
    return out;
}

@vertex
fn vs_rain(@builtin(vertex_index) vid: u32, @builtin(instance_index) k: u32) -> RainOut {
    if (k >= u32(rp.view.w)) {
        return splash(vid, k - u32(rp.view.w));
    }
    // across (-1, 1) and along (0 the tail, 1 the head)
    let c = vec2<f32>(f32(i32(vid & 1u) * 2 - 1), f32(vid >> 1u));
    var out: RainOut;
    out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    out.uv = vec2<f32>(0.0);
    out.shape = vec3<f32>(0.0);
    out.pos = vec3<f32>(0.0);
    let n0 = u32(rp.fall.z);
    let n1 = u32(rp.fall.w);
    let layer = select(select(2u, 1u, k < n0 + n1), 0u, k < n0);
    let lh = RAIN_LAYERS[layer];
    let size = vec3<f32>(2.0 * lh.x, 2.0 * lh.x, lh.z - lh.y);
    let h1 = hash4(k * 2u + 3u);
    let h2 = hash4(k * 2u + 6007u);
    // the drop: its diameter (m), drawn by its cross-section from the rain's sizes, and
    // the speed it falls at
    let lambda = 3.67 / max(rp.fall.y, 1e-4);
    let d = clamp(-log(max(h2.x * h2.y * h2.z, 1e-12)) / lambda, 0.0003, 0.006);
    let fall = max(9.65 - 10.3 * exp(-600.0 * d), 0.5);
    // its place in the world: where it started, carried by the wind, falling; then the
    // copy of the field nearest the camera (in the world's own figures, so that it stays
    // put while the render origin moves)
    let t = rp.wind.w;
    let world_cam = camera.cam_pos.xyz + vec3<f32>(camera.world_origin.xy, 0.0);
    let vel = vec3<f32>(rp.wind.xy, -fall);
    let lo = world_cam + vec3<f32>(-lh.x, -lh.x, lh.y);
    // (the field repeats every `size`: the place wrapped into one copy of it)
    let wrapped = (h1.xyz * size + vel * t - lo) / size;
    let q = lo + (wrapped - floor(wrapped)) * size;
    let p = q - vec3<f32>(camera.world_origin.xy, 0.0);
    // a denser layer thins out with the distance from the eye, over the whole of its reach,
    // into the thinner one round it: faded only at its edges, it stood round the camera as a
    // box of dense rain that went wherever the camera went
    let rel = q - lo;
    let e = min(rel, size - rel) / (lh.x * 0.3);
    let edge = clamp(min(min(e.x, e.y), e.z), 0.0, 1.0);
    let reach = 1.0 - smoothstep(0.25 * lh.x, lh.x, length((q - world_cam).xy));
    let fade = select(1.0, edge * reach, layer < 2u);
    // a drop right at the eye is a blur too wide and faint to see, not a streak
    let near = smoothstep(0.6, 1.6, length(p - camera.cam_pos.xyz));
    // the streak: from where the drop was against the eye a streak's time ago to where it is
    let ch = camera.view_proj * vec4<f32>(p, 1.0);
    // (the field reaches all round the camera, most of it out of the picture: a drop well
    // beyond its edge - a streak is a few per cent of the picture long - is let go before
    // the rest of its work, and so is one too faint to see)
    if (fade * near < 0.01 || ch.w < 0.1 || abs(ch.x) > 1.5 * ch.w || abs(ch.y) > 1.5 * ch.w) {
        return out;
    }
    let tail = p - (vel - rp.eye.xyz) * rp.eye.w;
    let ct = camera.view_proj * vec4<f32>(tail, 1.0);
    if (ct.w < 0.1 || in_own_vehicle(p) || under_roof(p, rp.roof, rp.roof_on.x)) {
        return out;
    }
    let cu = camera.view_proj * vec4<f32>(p + camera.cam_up.xyz, 1.0);
    let half_px = 0.5 * rp.view.xy;
    let sh = ch.xy / ch.w * half_px;
    let st = ct.xy / ct.w * half_px;
    let along = sh - st;
    let len = length(along);
    let dir = select(vec2<f32>(0.0, 1.0), along / max(len, 1e-4), len > 1e-3);
    let across = vec2<f32>(-dir.y, dir.x);
    // as wide as the drop (out of focus close by: its blur as wide as the pupil, some
    // 2.5 mm), a pixel at least - and as much fainter as it is narrower than that
    let px_per_m = length(cu.xy / cu.w * half_px - sh);
    let blur = sqrt(d * d + 0.0025 * 0.0025);
    let w = blur * px_per_m;
    // and spread along its way: the drop lies on each pixel of its streak for the share of
    // the eye's time it takes to cross it - a long streak is a faint one (the drop's lens
    // gathers the sky's light, brighter than what is behind it: some six times its width)
    let spread = clamp(6.0 * max(w, 1.0) / max(len, 1.0), 0.2, 1.0);
    let cover = min(w, 1.0) * (d / blur) * spread * fade * near * rp.view.z * clamp(sqrt(d / 0.002), 0.5, 1.4);
    // (a hair under a pixel at the least: a picture drawn smaller and scaled up widened the
    // full-pixel lines into the thick white bars of the old sprites)
    let hw = max(0.5 * w, 0.4);
    let pad = hw + 1.0;
    // the quad round the streak (the head's and the tail's clip positions mixed, so that it
    // keeps the depth along it) widened in the picture's pixels
    let clip = mix(ct, ch, c.y);
    let off = across * c.x * pad + dir * (c.y * 2.0 - 1.0) * pad;
    out.clip = vec4<f32>(clip.xy + off / half_px * clip.w, clip.zw);
    out.uv = vec2<f32>(c.x * pad, c.y * (len + 2.0 * pad) - pad);
    out.shape = vec3<f32>(hw, len, cover);
    out.pos = p;
    return out;
}

fn streak_alpha(in: RainOut) -> f32 {
    let hw = in.shape.x;
    let len = in.shape.y;
    // a pixel's edge across and at both ends, and a little fainter towards the tail
    let a_across = clamp(hw + 0.5 - abs(in.uv.x), 0.0, 1.0);
    let a_along = clamp(0.5 + min(in.uv.y, len - in.uv.y), 0.0, 1.0);
    let tail = mix(0.55, 1.0, clamp(in.uv.y / max(len, 1.0), 0.0, 1.0));
    // (a splash's round speck)
    let speck = clamp(hw + 0.5 - length(in.uv), 0.0, 1.0);
    return clamp(in.shape.z * select(a_across * a_along * tail, speck, len < 0.0), 0.0, 1.0);
}

// the classic picture: water lit as the scene is (envir.cfg's ambient and sun light)
@fragment
fn fs_rain(in: RainOut) -> @location(0) vec4<f32> {
    let a = streak_alpha(in);
    let light = min(camera.ambient.rgb + camera.sky_color.rgb * 0.5 + camera.sun_color.rgb * 0.3, vec3<f32>(1.0)) * vec3<f32>(0.68, 0.7, 0.75);
    return vec4<f32>(light * a, a);
}

// the enhanced picture: a drop shows what lies round it, turned over - the sky's and the
// ground's light - and flashes in the lamps' (`precip_light`, forward scattering)
@fragment
fn fs_rain_enhanced(in: RainOut) -> @location(0) vec4<f32> {
    let a = streak_alpha(in);
    let to_eye = normalize(camera.cam_pos.xyz - in.pos);
    // (a drop is a small lens of what lies round it, the sky above as the street below: a
    // little brighter than the street it is seen against, never a white line)
    let l = (precip_light(in.pos, to_eye, false) * 0.3 + sh_irradiance(vec3<f32>(0.0, 0.0, 1.0)) / PI * 0.5) * enh.exposure.x;
    return vec4<f32>(min(l, vec3<f32>(60.0)) * a, a);
}
