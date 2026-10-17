// What a surface reflects and scatters: diffuse albedo, specular colour at normal
// incidence, perceptual roughness.
struct Surface {
    albedo: vec3<f32>,
    f0: vec3<f32>,
    rough: f32,
};

// How much of street lamp `li` reaches the point (1 for a lamp without a shadow map): its
// map is one of the tiles under the far shadow map (lib.rs `LampShadow`), looking down from
// the lamp's head; a few taps of it soften the edge, as the lamp's glowing bowl does.
// (the player's headlamps - marked so, see `light_marks` - share the tile marked -2: one
// map from between them along the road, lib.rs `passes::headlamp_shadow`; it is read once
// for the pair, `own_shadow`)
var<private> own_shadow: f32;
fn lamp_shadow_at(li: u32, p: vec3<f32>, n: vec3<f32>, thin: bool) -> f32 {
    var k = 4u;
    let own = light_marks(lights[li]).x > 0.5;
    for (var j = 0u; j < 4u; j = j + 1u) {
        if ((camera.lamp_shadow[j] >= 0.0 && u32(camera.lamp_shadow[j]) == li) || (own && camera.lamp_shadow[j] < -1.5)) {
            k = j;
        }
    }
    if (k > 3u) {
        return 1.0;
    }
    if (own && own_shadow >= 0.0) {
        return own_shadow;
    }
    let d = distance(p, lights[li].pos.xyz);
    // (off the surface by a texel or so of the map there; a leaf card towards the lamp)
    let off = select(n * (0.03 + 0.012 * d), normalize(lights[li].pos.xyz - p) * 0.3, thin);
    let q = camera.lamp_view_proj[k] * vec4<f32>(p + off, 1.0);
    if (q.w <= 0.0) {
        return 1.0;
    }
    let ndc = q.xyz / q.w;
    if (abs(ndc.x) >= 1.0 || abs(ndc.y) >= 1.0 || ndc.z >= 1.0 || ndc.z <= 0.0) {
        return 1.0;
    }
    let tuv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let dims = vec2<f32>(textureDimensions(t_shadow_far));
    let tile = dims.x * 0.25;
    // (the tile's texels in the atlas's terms, kept a texel off its edges)
    let tx = clamp(tuv * tile, vec2<f32>(1.5), vec2<f32>(tile - 1.5));
    let base = vec2<f32>(f32(k) * tile, dims.x) + tx;
    var sum = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let a = (base + vec2<f32>(f32(x), f32(y)) * 1.25) / dims;
            sum = sum + textureSampleCompareLevel(t_shadow_far, s_shadow, a, ndc.z - 0.00002);
        }
    }
    if (own) {
        own_shadow = sum / 9.0;
    }
    return sum / 9.0;
}

// A light cookie layer's texels a side (lib.rs `cookies::COOKIE_SIZE`).
const COOKIE_SIZE: u32 = 32u;

// The enhanced path's marks on a light (lib.rs `prepare_lights`, carried in `pos.w` below
// zero): x 1 for one of the player's headlamps, y 1 + the layer of its beam's picture
// (0: none).
fn light_marks(l: PointLight) -> vec2<f32> {
    let m = max(-l.pos.w, 0.0);
    let own = select(0.0, 1.0, m > 15.5);
    return vec2<f32>(own, m - own * 16.0);
}

// A light cookie's texel (x, y) of the layer whose records start at `base`.
fn cookie_texel(base: u32, x: u32, y: u32) -> vec3<f32> {
    let i = min(y, COOKIE_SIZE - 1u) * COOKIE_SIZE + min(x, COOKIE_SIZE - 1u);
    let rec = lights[base + i / 4u];
    let k = i % 4u;
    return select(select(rec.pos.xyz, rec.color.xyz, k == 1u), select(rec.dir.xyz, rec.extra.xyz, k == 3u), k >= 2u);
}

// What a lamp with a picture of its beam (`[spotlight_cookie]`) sends towards `t` (unit,
// from the lamp): the picture's light there, as the beam would show on a wall square to
// the lamp - the picture's right towards the lamp's right as seen from behind it, its top
// up - and nothing outside the field it covers (`camera.cookies`: x, y its half angles'
// tangents, z where its layer starts in the light buffer, four linear texels a record),
// filtered between its four nearest texels.
fn cookie_light(t: vec3<f32>, l: PointLight) -> vec3<f32> {
    let f = l.dir.xyz;
    let ahead = dot(t, f);
    let rc = cross(f, vec3<f32>(0.0, 0.0, 1.0));
    let r = select(vec3<f32>(1.0, 0.0, 0.0), normalize(rc), dot(rc, rc) > 1e-8);
    let layer = camera.cookies[min(u32(light_marks(l).y - 0.5), 7u)];
    let q = vec2<f32>(dot(t, r), dot(t, cross(r, f))) / (max(ahead, 1e-3) * max(layer.xy, vec2<f32>(1e-3)));
    let st = clamp(vec2<f32>(q.x * 0.5 + 0.5, 0.5 - q.y * 0.5) * f32(COOKIE_SIZE) - vec2<f32>(0.5), vec2<f32>(0.0), vec2<f32>(f32(COOKIE_SIZE) - 1.0));
    let c0 = vec2<u32>(st);
    let w = st - floor(st);
    let base = u32(layer.z);
    let top = mix(cookie_texel(base, c0.x, c0.y), cookie_texel(base, c0.x + 1u, c0.y), w.x);
    let bottom = mix(cookie_texel(base, c0.x, c0.y + 1u), cookie_texel(base, c0.x + 1u, c0.y + 1u), w.x);
    return select(vec3<f32>(0.0), mix(top, bottom, w.y), ahead > 1e-3 && all(abs(q) < vec2<f32>(1.0)));
}

// The radiance of the street round the point as the lamps light it (`lamp_light` leaves it
// here): what a reflection pointing at the street and the walls beside it sees at night.
var<private> lamp_ground: vec3<f32>;

// The point and spot lights of the pixel's grid cell: diffuse and specular.
// `thin`: foliage, lit from whichever side the lamp is on (see the sun below).
// `film`: a wet road's film of water - x how much of the surface it is (0: none), y how
// rough it is, z the share of it that mirrors - with which the lamps are reflected as the
// sky is (`refl_rough`, `wet_share` in fs_enhanced), not by the dry surface's own lobe.
fn lamp_light(p: vec3<f32>, n: vec3<f32>, v: vec3<f32>, sf: Surface, thin: bool, shadows: bool, film: vec3<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    lamp_ground = vec3<f32>(0.0);
    own_shadow = -1.0;
    // the lamps' light on the ground round the point (a horizontal surface's, unshadowed:
    // the ground the point looks down at is wider than its own shadow)
    var ground_e = vec3<f32>(0.0);

    let cell = camera.light_grid.z;
    let side = u32(camera.light_grid.w);
    if (cell <= 0.0 || side == 0u) {
        return sum;
    }
    let f = (p.xy - camera.light_grid.xy) / cell;
    let x = i32(floor(f.x));
    let y = i32(floor(f.y));
    if (x < 0 || y < 0 || x >= i32(side) || y >= i32(side)) {
        return sum;
    }
    let a = max(sf.rough * sf.rough, 0.02);
    let a_film = max(film.y * film.y, 0.02);
    let nv = max(dot(n, v), 1e-4);
    let base = (u32(y) * side + u32(x)) * CELL_CAP;
    for (var j = 0u; j < CELL_CAP; j = j + 1u) {
        let li = grid[base + j];
        if (li == 0xffffffffu) {
            break;
        }
        let l = lights[li];
        let d = l.pos.xyz - p;
        let dist2 = dot(d, d);
        // (pos.w is 0 on a spot light, which the vanilla shader is to pass by)
        let range = l.extra.w;
        if (dist2 >= range * range) {
            continue;
        }
        let dist = sqrt(dist2);
        let ld = d / max(dist, 1e-3);
        // inverse-square beyond the core with a soft knee at it, not flat within it; windowed to zero at
        // the range so the grid cut-off does not show
        var core = l.extra.y;
        if (core <= 0.0) {
            core = range * 0.125;
        }
        let q = dist2 / (range * range);
        let window = (1.0 - q * q) * (1.0 - q * q);
        var e = core * core / sqrt(dist2 * dist2 + core * core * core * core) * window;
        // (a lamp's picture of its beam: its colour that way, see `cookie_light`)
        var tint = vec3<f32>(1.0);
        if (l.extra.z != 0.0) {
            e = headlamp(-ld, l.dir.xyz, l.extra.z > 0.0) / max(dist2, 0.3) * window;
        } else if (l.dir.w > -1.5) {
            let cd = dot(-ld, l.dir.xyz);
            if (light_marks(l).y > 0.5) {
                tint = cookie_light(-ld, l);
                e = e * step(1e-4, max(tint.r, max(tint.g, tint.b)));
            } else {
                e = e * smoothstep(l.dir.w, l.extra.x, cd);
            }
        } else if (l.dir.z < -0.5) {
            // a lamp in a housing (a street lamp's head, a platform's light): its reflector
            // sends the light down and out, its housing keeps it from the sky - a few per
            // cent above its horizon, what the bowl and the pole scatter (the upward light
            // ratio of road lighting, EN 13201 / CIE 115). Shining evenly every way, every
            // lamp lit the crowns of the trees and the upper floors over it as brightly
            // as the street.
            e = e * (0.05 + 0.95 * smoothstep(-0.1, 0.3, ld.z));
        }
        if (e <= 0.0 || (!(l.extra.z == 0.0 && l.dir.w < -1.5 && l.dir.z < -0.5) && own_vehicle_blocks(p, l.pos.xyz))) {
            continue;
        }
        var irr = l.color.rgb * tint * l.color.w * enh.lights.y * e;
        let nl = dot(n, ld);
        ground_e = ground_e + irr * max(ld.z, 0.0);
        // Ground bounce is unshadowed. A lamp behind an ordinary surface contributes
        // no direct light, so it must not sample its shadow map before being rejected.
        if (!thin && nl <= 0.0) {
            continue;
        }
        // the lamps' own shadow maps (the few lighting the view most, see `lamp_shadow_at`)
        if (shadows) {
            irr = irr * lamp_shadow_at(li, p, n, thin);
        }
        if (thin) {
            // a headlamp skims the grass: it lights the tips, not a crown's every side
            let wrap = select(0.45 + 0.25 * nl, 0.15 + 0.6 * max(nl, 0.0), l.extra.z != 0.0);
            sum = sum + irr * wrap * sf.albedo / PI;
            continue;
        }
        let h = normalize(ld + v);
        let nh = max(dot(n, h), 0.0);
        let fr = f_schlick(sf.f0, dot(v, h));
        var spec = d_ggx(nh, a) * v_smith(nv, nl, a) * fr;
        if (film.x > 0.0) {
            spec = mix(spec, d_beckmann(nh, a_film) * v_smith(nv, nl, a_film) * fr * film.z, film.x);
        }
        sum = sum + irr * nl * (sf.albedo / PI + spec);
    }
    // What the lit ground throws back up: a diffuse reflector of the street's albedo (asphalt
    // and paving about 0.18, under water darker, fresh snow 0.75), seen by a surface over the
    // lower half of its view, (1 - n.z) / 2 of it - a wall half, the underside of a shelter's
    // roof all, the ground itself nothing. Without it the lamps lit only what faces them, and
    // a facade or a bus's flank beside a lit street stood black.
    let snow = clamp(enh.weather.y, 0.0, 1.0);
    let rho = mix(0.18, 0.75, snow) * mix(1.0, 0.6, clamp(enh.weather.x, 0.0, 1.0) * (1.0 - snow));
    let seen = select((1.0 - clamp(n.z, -1.0, 1.0)) * 0.5, 0.5, thin);
    sum = sum + ground_e * rho * seen * sf.albedo / PI;
    lamp_ground = ground_e * rho / PI;
    return sum;
}

