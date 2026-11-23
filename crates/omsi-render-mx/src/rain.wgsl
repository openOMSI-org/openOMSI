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
    // Enhanced: x the instance the veil's panels begin at (the drops' and the splashes' end),
    // y how many panels there are, z how strong the wind's gusts are (0..1), w the rain's
    // quality (0 low .. 2 high)
    ext: vec4<f32>,
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


// ---- The enhanced rainfall
//
// The same field of drops, drawn as rain photographs: each drop the streak it leaves in the
// eye's time, its length and slant its way against the eye (the bus's own motion, the wind
// and its gusts), a soft line as wide as the drop with rounded ends, beaded along a long
// streak where the oscillating drop flickers. A drop is a small ball lens: it carries the
// light of what lies round it turned over - the sky in its lower half, the street in its
// upper - so that by day it is a silvery line, faint against the bright sky and clear
// against a dark street, glows when the sun shines through it from behind, colours the
// rainbow's ring round the antisolar point, and flashes in the lamps and headlights at
// night (`precip_light`). Where the drops land, crowns of water rise and throw droplets;
// beyond the drops' layers the rain is a veil of streaks (`veil_corner`).

struct RainOutE {
    @builtin(position) clip: vec4<f32>,
    // a drop: across its streak and along it from the tail (px); a splash: across it and up
    // it (-1..1, 0..1 and over); the veil: the panel's corner (0..1)
    @location(0) uv: vec2<f32>,
    // a drop: x the streak's half width, y its length (px); a splash: x one pixel in its
    // own measure, y its age (0..1); z the cover, w which it is (0 a drop, 1 a splash, 2 the
    // veil)
    @location(1) @interpolate(flat) shape: vec4<f32>,
    @location(2) pos: vec3<f32>,
    // x a number of the particle's own (0..1), y the veil's radius (m), z its ring
    @location(3) @interpolate(flat) seed: vec4<f32>,
};

fn rain_none() -> RainOutE {
    var out: RainOutE;
    out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    out.uv = vec2<f32>(0.0);
    out.shape = vec4<f32>(0.0);
    out.pos = vec3<f32>(0.0);
    out.seed = vec4<f32>(0.0);
    return out;
}

@vertex
fn vs_rain_enhanced(@builtin(vertex_index) vid: u32, @builtin(instance_index) k: u32) -> RainOutE {
    if (k >= u32(rp.ext.x)) {
        return rain_veil(vid, k - u32(rp.ext.x));
    }
    if (k >= u32(rp.view.w)) {
        return rain_crown(vid, k - u32(rp.view.w));
    }
    return rain_drop(vid, k);
}

fn rain_drop(vid: u32, k: u32) -> RainOutE {
    // across (-1, 1) and along (0 the tail, 1 the head)
    let c = vec2<f32>(f32(i32(vid & 1u) * 2 - 1), f32(vid >> 1u));
    var out = rain_none();
    let n0 = u32(rp.fall.z);
    let n1 = u32(rp.fall.w);
    let layer = select(select(2u, 1u, k < n0 + n1), 0u, k < n0);
    let lh = RAIN_LAYERS[layer];
    let size = vec3<f32>(2.0 * lh.x, 2.0 * lh.x, lh.z - lh.y);
    let h1 = hash4(k * 2u + 3u);
    let h2 = hash4(k * 2u + 6007u);
    // the drop's diameter and speed, as `vs_rain` draws them
    let lambda = 3.67 / max(rp.fall.y, 1e-4);
    let d = clamp(-log(max(h2.x * h2.y * h2.z, 1e-12)) / lambda, 0.0003, 0.006);
    let fall = max(9.65 - 10.3 * exp(-600.0 * d), 0.5);
    let t = rp.wind.w;
    let world_cam = camera.cam_pos.xyz + vec3<f32>(camera.world_origin.xy, 0.0);
    let lo = world_cam + vec3<f32>(-lh.x, -lh.x, lh.y);
    let wrapped = (h1.xyz * size + vec3<f32>(rp.wind.xy, -fall) * t - lo) / size;
    let q0 = lo + (wrapped - floor(wrapped)) * size;
    // the gusts carry it on from its place in the field, and slant it as they blow
    let g = precip_gust(q0.xy, t, rp.wind.xy, rp.ext.z);
    let q = q0 + vec3<f32>(rp.wind.xy * g.x, 0.0);
    let vel = vec3<f32>(rp.wind.xy * (1.0 + g.y), -fall);
    let p = q - vec3<f32>(camera.world_origin.xy, 0.0);
    // every layer fades out at its edges (where its drops come round again, the gusts'
    // way at the far side of the field differs from the near side's), the denser ones
    // over their whole reach into the thinner round them
    let rel = q0 - lo;
    let e = min(rel, size - rel) / (lh.x * 0.3);
    let edge = clamp(min(min(e.x, e.y), e.z), 0.0, 1.0);
    let reach = 1.0 - smoothstep(0.25 * lh.x, lh.x, length((q - world_cam).xy));
    let fade = edge * select(1.0, reach, layer < 2u);
    let near = smoothstep(0.5, 1.4, length(p - camera.cam_pos.xyz));
    let ch = camera.view_proj * vec4<f32>(p, 1.0);
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
    let dir = select(vec2<f32>(0.0, 1.0), along / max(len, 1e-3), len > 1e-3);
    let across = vec2<f32>(-dir.y, dir.x);
    // as wide as the drop, a millimetre's blur at the least (crisp close by: the eye is
    // focused on the street, a few metres off, and a drop there is sharp), and spread over
    // its way as `vs_rain` spreads it
    let px_per_m = length(cu.xy / cu.w * half_px - sh);
    let blur = sqrt(d * d + 1e-6);
    let w = blur * px_per_m;
    let spread = clamp(6.0 * max(w, 1.0) / max(len, 1.0), 0.2, 1.0);
    let cover = min(w, 1.0) * (d / blur) * spread * fade * near * rp.view.z * clamp(sqrt(d / 0.002), 0.5, 1.4);
    let hw = max(0.5 * w, 0.4);
    let pad = hw + 1.5;
    let clip = mix(ct, ch, c.y);
    let off = across * c.x * pad + dir * (c.y * 2.0 - 1.0) * pad;
    out.clip = vec4<f32>(clip.xy + off / half_px * clip.w, clip.zw);
    out.uv = vec2<f32>(c.x * pad, c.y * (len + 2.0 * pad) - pad);
    out.shape = vec4<f32>(hw, len, cover, 0.0);
    out.pos = p;
    out.seed = vec4<f32>(h2.w, 0.0, 0.0, 0.0);
    return out;
}

// Where a drop lands (`splash`'s places): a crown of water rises round the impact for a
// few hundredths of a second, its rim breaks into points, and the points fling droplets
// out on short arcs that fall back - on the street, a kerb, a bus's roof. Upright, it is
// drawn facing the eye.
fn rain_crown(vid: u32, s: u32) -> RainOutE {
    // across (-1, 1) and up (0, 1)
    let c = vec2<f32>(f32(i32(vid & 1u) * 2 - 1), f32(vid >> 1u));
    var out = rain_none();
    let life = 0.24;
    let h0 = hash4(s * 3u + 11u);
    let cycle = rp.wind.w / life + h0.w;
    let age = fract(cycle);
    let h = hash4(s * 7919u + u32(floor(cycle)) * 2654435761u);
    let size = 28.0;
    let world_cam = camera.cam_pos.xy + camera.world_origin.xy;
    let lo = world_cam - vec2<f32>(0.5 * size);
    let wxy = lo + (((h.xy * size - lo) % size) + size) % size;
    let xy = wxy - camera.world_origin.xy;
    let fade_far = 1.0 - smoothstep(7.0, 16.0, length(xy - camera.cam_pos.xy));
    let m = rp.roof;
    let mc = m * vec4<f32>(xy, 0.0, 1.0);
    let uv = vec2<f32>(mc.x * 0.5 + 0.5, 0.5 - mc.y * 0.5);
    let side = f32(textureDimensions(roof_map).x);
    let top = roof_depth(vec2<i32>(clamp(uv, vec2<f32>(0.0), vec2<f32>(0.9999)) * side));
    let p = vec3<f32>(xy, (top - mc.z) / m[2].z + 0.002);
    let ch = camera.view_proj * vec4<f32>(p, 1.0);
    if (rp.roof_on.x < 0.5 || top >= 0.9999 || fade_far <= 0.0 || ch.w < 0.3 || in_own_vehicle(p)) {
        return out;
    }
    let cu = camera.view_proj * vec4<f32>(p + camera.cam_up.xyz, 1.0);
    let half_px = 0.5 * rp.view.xy;
    let px_per_m = length(cu.xy / cu.w * half_px - ch.xy / ch.w * half_px);
    // a few centimetres across, the bigger the drops the bigger, and at least a few pixels
    // (the crown drawn smaller inside it, its parts no thinner than a pixel)
    let scale = clamp(rp.fall.y / 0.0018, 0.6, 1.6) * (0.7 + 0.6 * h.z);
    let half_w = 0.045 * scale;
    let grow = max(1.0, 2.0 / max(half_w * px_per_m, 1e-3));
    let to_cam = camera.cam_pos.xyz - p;
    let flat_side = vec3<f32>(-to_cam.y, to_cam.x, 0.0);
    let ax = select(vec3<f32>(1.0, 0.0, 0.0), normalize(flat_side), length(flat_side) > 1e-4);
    let corner = p + ax * (c.x * half_w * grow) + vec3<f32>(0.0, 0.0, (c.y * 1.2 * grow - 0.05) * half_w);
    out.clip = camera.view_proj * vec4<f32>(corner, 1.0);
    out.uv = vec2<f32>(c.x * grow, (c.y * 1.2 * grow - 0.05));
    out.shape = vec4<f32>(1.0 / max(half_w * px_per_m, 1e-3), age, 0.9 * rp.view.z * fade_far, 1.0);
    out.pos = p;
    out.seed = vec4<f32>(h.w, 0.0, 0.0, 0.0);
    return out;
}

fn rain_veil(vid: u32, panel: u32) -> RainOutE {
    let c = vec2<f32>(f32(vid & 1u), f32(vid >> 1u));
    var out = rain_none();
    let v = veil_corner(panel, c);
    let ring = f32(panel / 12u);
    // (none in a light rain, whose drops are not seen that far)
    if (rp.fall.x < 0.15) {
        return out;
    }
    out.clip = camera.view_proj * vec4<f32>(v.xyz, 1.0);
    out.uv = c;
    // (the rain's streaks are seen beyond the drops only in a fair rain, and as a veil in a
    // heavy one; the further rings stand for more rain behind them)
    out.shape = vec4<f32>(0.0, 0.0, pow(rp.fall.x, 1.5) * 0.085 * (0.8 + 0.2 * ring), 2.0);
    out.pos = v.xyz;
    out.seed = vec4<f32>(0.0, v.w, ring, 0.0);
    return out;
}

fn drop_alpha(in: RainOutE) -> f32 {
    let hw = in.shape.x;
    let len = in.shape.y;
    // across: a soft line, as a lens draws a thin one - at least a pixel's blur
    let s = max(hw * 0.85, 0.6);
    let across = exp(-0.5 * in.uv.x * in.uv.x / (s * s));
    // along: the drop's way, its ends rounded as wide as the drop
    let e = hw + 0.7;
    let along = smoothstep(-e, e, in.uv.y) * smoothstep(-e, e, len - in.uv.y);
    // a little brighter at the head (where the drop is now), and a long streak beaded: a
    // falling drop oscillates a few hundred times a second, its glint flickering on its way
    let f = clamp(in.uv.y / max(len, 1.0), 0.0, 1.0);
    let beads = 1.0 + 0.22 * sin(in.uv.y * (0.35 + 0.3 * in.seed.x) + in.seed.x * 40.0) * smoothstep(12.0, 40.0, len);
    return clamp(in.shape.z * across * along * mix(0.75, 1.0, f) * beads, 0.0, 1.0);
}

fn crown_alpha(in: RainOutE) -> f32 {
    let px = in.shape.x;
    let age = in.shape.y;
    let seed = in.seed.x;
    let uv = in.uv;
    // the crown: a thin sheet rising round the impact, a cylinder seen from the side - most
    // of it at its outline, where the eye looks along the sheet - its rim pulled into points
    let rise = smoothstep(0.0, 0.18, age) * (1.0 - smoothstep(0.3, 0.6, age));
    let r = 0.35 + 0.5 * sqrt(age);
    let x = uv.x / r;
    let points = 7.0 + floor(seed * 5.0);
    let ang = asin(clamp(x, -1.0, 1.0));
    let tips = pow(abs(cos(ang * points * 0.5 + seed * 6.2831853)), 6.0);
    let top = rise * (0.5 + 0.35 * tips) * (0.8 + 0.4 * fract(seed * 7.3));
    let outline = 0.3 + 0.7 * smoothstep(0.5, 1.0, abs(x));
    let side = 1.0 - smoothstep(1.0 - px / r, 1.0 + px / r, abs(x));
    let below = 1.0 - smoothstep(top - px, top + px, uv.y);
    // (a sheet thinner than a pixel is as faint as it is thin)
    let crown = side * below * outline * step(0.0, uv.y) * 0.75 * min(1.0, 0.04 / px);
    // the droplets the points fling off, on arcs out and up and back down
    var drops = 0.0;
    let fly = max(age - 0.14, 0.0);
    let rad = max(0.07, px);
    for (var i = 0u; i < 5u; i = i + 1u) {
        let hi = hash4(u32(seed * 16777216.0) + i * 977u);
        let vx = (hi.x * 2.0 - 1.0) * 2.4;
        let vy = 2.2 + 2.2 * hi.y;
        let q = vec2<f32>(sign(vx) * r * 0.9 + vx * fly, 0.5 * rise + vy * fly - 14.0 * fly * fly);
        let a = (1.0 - smoothstep(rad - px, rad + px, length(uv - q))) * step(0.0, q.y) * step(0.05, fly);
        drops = max(drops, a * (0.07 / rad) * (0.07 / rad) * step(hi.z, 0.8));
    }
    return clamp(in.shape.z * max(crown, drops) * (1.0 - age * age), 0.0, 1.0);
}

fn veil_rain_alpha(in: RainOutE) -> f32 {
    let r = in.seed.y;
    let ring = in.seed.z;
    let d = in.pos - camera.cam_pos.xyz;
    let theta = atan2(d.y, d.x);
    let t = rp.wind.w;
    let tang = vec2<f32>(-sin(theta), cos(theta));
    let fall = 7.0;
    // (the wind slants the streaks: so many metres sideways for each metre of fall)
    let slant = dot(rp.wind.xy, tang) / fall;
    // Columns of streaks, a fixed angle apart (a few pixels) and a whole number of them to
    // the ring, so that its seam does not show; each column's drops fall at their own
    // speed, a streak a stretch of it.
    var streaks = 0.0;
    for (var j = 0u; j < 2u; j = j + 1u) {
        let fj = f32(j);
        let cols = round((1400.0 + 216.0 * ring) * (1.0 + 0.37 * fj));
        let cw = 6.2831853 * r / cols;
        let x = theta / 6.2831853 * cols - slant * in.pos.z / cw + 0.5 * fj;
        let ci = floor(x);
        let h = cell_hash(vec2<f32>(ci, 97.0 * fj + 13.0 * ring));
        let fx = fract(x) - 0.5 - (h.x - 0.5) * 0.4;
        let speed = fall * (0.85 + 0.3 * h.y);
        let period = r * (0.07 + 0.05 * h.z);
        let v = fract((in.pos.z + speed * t) / period + h.w);
        let len = 0.25 + 0.35 * h.x;
        let along = smoothstep(0.0, 0.1, v) * (1.0 - smoothstep(len - 0.1, len, v));
        let across = 1.0 - smoothstep(0.06, 0.16, abs(fx));
        streaks = max(streaks, along * across * (0.4 + 0.6 * h.y));
    }
    // the gusts sweep through it as a little denser sheets (at the full swing of 0.3 to 1.9 they
    // stood in the sky as broad pale bands, sliding across it)
    let g = precip_gust(in.pos.xy + camera.world_origin.xy, t, rp.wind.xy, rp.ext.z);
    let sweep = clamp(1.0 + 0.6 * g.y, 0.7, 1.3);
    // (thinned out towards the ground, where the panel meets it in a line, and above the
    // horizon: the veil is the far rain seen edgewise along the ground - looked up at, its
    // panels and their tops stood round the eye as a ring of grey sheets in the sky; what the
    // rain takes from the view up there is its extinction, the fog's)
    let up = in.pos.z - camera.cam_pos.z;
    let height = smoothstep(-3.0, 3.0, up) * (1.0 - smoothstep(0.08, 0.3, up / r));
    // (the streaks themselves: a flat grey under them read as a sheet of frosted glass)
    return clamp(in.shape.z * (0.12 + 0.88 * streaks) * sweep * height, 0.0, 1.0);
}

// The light a drop sends to the eye besides the lamps': the sky's and the street's through
// it, and the sun's - refracted forwards (a shower lit from behind shines) and, at 42 and
// 51 degrees round the antisolar point, split into the rainbow's colours.
fn rain_surround(to_eye: vec3<f32>, sun: f32) -> vec3<f32> {
    let up = sh_irradiance(vec3<f32>(0.0, 0.0, 1.0)) / PI;
    let down = sh_irradiance(vec3<f32>(0.0, 0.0, -1.0)) / PI;
    // (a ball lens gathers the light of half the sky into the eye: a drop's streak is a
    // little brighter than the overcast behind it - a silvery line - and much brighter
    // than the street, the trees and the walls)
    let l = mix(down, up, 0.8) * 1.35;
    let c = dot(-camera.sun_dir.xyz, to_eye);
    let forward = 0.85 * hg_phase(c, 0.82) + 0.15 / (4.0 * PI);
    let a = degrees(acos(clamp(-c, -1.0, 1.0)));
    let b1 = (a - vec3<f32>(42.3, 41.5, 40.7)) / 0.75;
    let b2 = (a - vec3<f32>(50.3, 51.2, 52.4)) / 1.0;
    let bow = exp(-b1 * b1) * 0.12 + exp(-b2 * b2) * 0.045;
    return l + enh.sun.rgb * sun * (forward + bow);
}

@fragment
fn fs_rain_enhanced(in: RainOutE) -> @location(0) vec4<f32> {
    let kind = in.shape.w;
    var a = 0.0;
    if (kind < 0.5) {
        a = drop_alpha(in);
    } else if (kind < 1.5) {
        a = crown_alpha(in);
    } else {
        a = veil_rain_alpha(in);
    }
    if (a < 0.002) {
        discard;
    }
    let to_eye = normalize(camera.cam_pos.xyz - in.pos);
    var l = rain_surround(to_eye, precip_sun(in.pos));
    // (the lamps for the drops and the crowns alone: the veil's panels cover much of the
    // picture, and the lamps' light in the air is the fog's). A drop sends a lamp's light
    // every way - the glint of its surface, a small mirror ball, besides what it refracts on
    // forwards - so the streaks shine in a street lamp's light however the eye looks at
    // them, as rain is seen at night (`precip_light`'s flake lobe, less the light round it,
    // which `rain_surround` gives already)
    if (kind < 1.5) {
        let round = enh.fog_color.rgb / 0.9 + enh.sun.rgb * (0.25 / PI);
        l = max(precip_light(in.pos, to_eye, true) - round, vec3<f32>(0.0)) * 1.5 + l;
    }
    return vec4<f32>(min(l * enh.exposure.x, vec3<f32>(60.0)) * a, a);
}
