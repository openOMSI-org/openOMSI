fn rain_hash(p: vec2<f32>) -> vec2<f32> {
    // Integer mixing keeps neighbouring cells independent without the precision loss and
    // repeated sine evaluations of a floating-point hash.
    let cell = bitcast<vec2<u32>>(vec2<i32>(floor(p)));
    var a = cell.x * 0x9e3779b9u ^ cell.y * 0x85ebca6bu;
    a = (a ^ (a >> 16u)) * 0x7feb352du;
    a = (a ^ (a >> 15u)) * 0x846ca68bu;
    a = a ^ (a >> 16u);
    var b = a ^ 0x68bc21ebu;
    b = (b ^ (b >> 16u)) * 0x7feb352du;
    b = (b ^ (b >> 15u)) * 0x846ca68bu;
    b = b ^ (b >> 16u);
    return vec2<f32>(f32(a), f32(b)) * (1.0 / 4294967296.0);
}

// The rain on a window pane at one pixel: the water's surface and how much of the pixel it
// covers. Three kinds of water, as on a bus window in the rain:
// * fine droplets and connected water film, which soften and bend the view;
// * drops that sit on the glass in three sizes (they land, grow and dry up again, each at
//   its own pace; more and bigger ones the wetter the pane), each a little dome flattened
//   by its weight;
// * runners: drops grown heavy enough to slide down, in fits and starts, a teardrop that
//   wipes a clear track through the mist and the drops below it and leaves a string of
//   beads behind.
// `uv` is the pane's own texture coordinate (without the [texcoordtransY] scroll: that
// moved the whole picture of drops down at once, like paper off a roll), `n` its normal,
// `wet` how wet it is. Laid out in metres over the glass and running down along the
// world's vertical as it lies on the pane, whatever way the model's texture is turned.
struct RainGlass {
    // the water's surface normal (world), pointing out of the water
    n: vec3<f32>,
    // how much of the pixel a drop or a bead covers (0..1, antialiased)
    cover: f32,
    // the pane's normal on the side the rain is on (outside the bus)
    out: vec3<f32>,
    // coverage of fine droplets and irregular water film (0..1)
    mist: f32,
};

// One drop of radius `r` (metres) centred `d` (metres, across / down the glass) away:
// its cover of the pixel (`px`: the pixel's size on the glass) and the slope of its dome
// (the normal's part along the glass). A shallow cap suits water spreading on glass;
// steep caps make every rim internally reflect and look like thick glass beads.
fn rain_dome(d: vec2<f32>, r: f32, px: f32) -> vec3<f32> {
    let rr = length(d) / max(r, 1e-5);
    let aa = max(px / max(r, 1e-5), 0.02);
    // Subpixel drops retain diminishing coverage instead of disappearing at a
    // fixed cutoff; their opposing slopes average towards the flat pane.
    let seen = smoothstep(0.0, 0.9, r / max(px, 1e-6));
    let cover = (1.0 - smoothstep(1.0 - aa, 1.0 + aa, rr)) * seen;
    // A subpixel cap contains both opposing slopes. Filter its normal before
    // refraction; using the full rim slope turns fine rain into high-contrast grit.
    let cap = mix(0.18, 0.6, smoothstep(0.6, 2.0, r / max(px, 1e-6))) * seen;
    let slope = d / max(r, 1e-5) * cap;
    let s = select(slope, slope / max(length(slope), 1e-5) * cap, length(slope) > cap);
    return vec3<f32>(s, cover);
}

// Smooth noise for water-film refraction and residual wiper streaks (0..1).
fn rain_patches(q: vec2<f32>) -> vec2<f32> {
    let c = floor(q);
    let f = q - c;
    let w = f * f * (3.0 - 2.0 * f);
    let a = rain_hash(c);
    let b = rain_hash(c + vec2<f32>(1.0, 0.0));
    let d = rain_hash(c + vec2<f32>(0.0, 1.0));
    let e = rain_hash(c + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, w.x), mix(d, e, w.x), w.y);
}

// A grid of its own for each layer of drops: turned by its own angle and shifted, so that
// no two layers line up and none runs along the glass's edges.
fn rain_turn(q: vec2<f32>, a: f32) -> vec2<f32> {
    let cs = vec2<f32>(cos(a), sin(a));
    return vec2<f32>(q.x * cs.x - q.y * cs.y, q.x * cs.y + q.y * cs.x);
}

fn rain_glass(world: vec3<f32>, uv: vec2<f32>, n: vec3<f32>, water: f32, t: f32, outside_in: bool, pane: vec3<f32>) -> RainGlass {
    // Match the collector map's larger visible caps (#3019). Scale the cell spacing
    // too, so larger drops replace fine specks rather than multiplying their cover.
    let wet = clamp(water, 0.0, 1.0);
    // The drops are bigger the harder it has rained on the pane (a drizzle leaves beads, a
    // downpour full drops), and on an upright side pane, which the rain meets edge on and
    // the airstream sweeps, smaller than on the sloping windscreen (#3076).
    let drop_scale = mix(1.2, 2.0, smoothstep(0.1, 0.8, wet)) * mix(0.7, 1.0, smoothstep(0.0, 0.4, abs(n.z)));
    let heavy = smoothstep(0.55, 1.0, wet);
    let wiped_film = clamp(-water * 25.0, 0.0, 1.0);
    var g: RainGlass;
    g.n = n;
    g.out = n;
    g.cover = 0.0;
    g.mist = 0.0;
    // (the derivatives first, while every pixel of the quad still takes part)
    let dpx = dpdx(world);
    let dpy = dpdy(world);
    let dux = dpdx(uv);
    let duy = dpdy(uv);
    let water_dx = dpdx(water);
    let water_dy = dpdy(water);
    let pane_dx = dpdx(pane.xy);
    let pane_dy = dpdy(pane.xy);
    var collectors = vec4<f32>(0.0);
    var tracked = false;
    if (material.wipe_bounds.z != 0.0) {
        // the nearest texel's own depth: a filtered sample mixes the packed depth bytes of
        // neighbouring texels into a depth of neither (see `window_wetness`)
        let dims = vec2<i32>(WIPE_MASK_SIZE);
        let texel = textureLoad(t_trans, clamp(vec2<i32>(pane.xy * vec2<f32>(dims)), vec2<i32>(0), dims - 1), 0);
        let depth = dot(texel.rg, vec2<f32>(256.0, 1.0)) * (64.0 / 257.0) - 32.0;
        tracked = abs(depth - pane.z) <= 0.1 && all(pane.xy >= vec2<f32>(0.0)) && all(pane.xy <= vec2<f32>(1.0));
        if (tracked) { collectors = textureSampleLevel(t_bump, s_diffuse, pane.xy, 0.0); }
    }
    // Preserve pixel area at oblique angles; the longest axis alone erased
    // still-visible beads on passenger windows viewed along the bus.
    let px = max(sqrt(length(cross(dpx, dpy))), 1e-5);
    let footprint = max(length(dpx), length(dpy));
    let det = dux.x * duy.y - dux.y * duy.x;
    if ((wet <= 0.01 && wiped_film <= 0.01 && collectors.a <= 0.01) || abs(det) < 1e-12) {
        return g;
    }
    // world position's derivative along u and v (inverse of the screen Jacobian): metres
    // per unit of uv, and which way is down in uv
    let dpdu = (dpx * duy.y - dpy * dux.y) / det;
    let dpdv = (dpy * dux.x - dpx * duy.x) / det;
    let mu = length(dpdu);
    let mv = length(dpdv);
    if (mu < 1e-6 || mv < 1e-6) {
        return g;
    }
    // quantised so neighbouring triangles of one pane share the grid
    let su = exp2(round(log2(mu)));
    let sv = exp2(round(log2(mv)));
    let p = vec2<f32>(uv.x * su, uv.y * sv);
    // down in uv: against the height's gradient
    var down = -vec2<f32>(dpdu.z / mu, dpdv.z / mv);
    if (length(down) < 0.2) {
        down = vec2<f32>(0.0, 1.0); // a roof light: no way down, the drops just sit
    }
    // Snapped to the nearest eighth of a turn: `down` comes from the world's vertical, and
    // the body rolls and pitches on its springs all the time - a grid turned with it by a
    // degree moves drops a metre from its origin by more than a cell, so every drop on the
    // pane jumped to a neighbour's place and back from frame to frame (the drops trembled
    // and flickered while driving). A pane's texture is laid out square to it, so the eighth
    // turns keep the drops fixed to the glass and still running down it.
    let ang = round(atan2(down.y, down.x) / 0.7853982) * 0.7853982;
    down = vec2<f32>(cos(ang), sin(ang));
    let side = vec2<f32>(down.y, -down.x);
    // pane coordinates in metres: x across, y down the glass
    let q = vec2<f32>(dot(p, side), dot(p, down));
    // the same two directions in the world, and the pane's normal on the rain's side: the
    // film faces the eye unless the eye is in the bus behind it
    let tu = dpdu / su;
    let tv = dpdv / sv;
    let eye = normalize(camera.cam_pos.xyz - world);
    var out = safe_normal(n);
    out = select(-out, out, dot(out, eye) >= 0.0);
    out = select(out, -out, outside_in);
    g.out = out;
    var side_w = side.x * tu + side.y * tv;
    side_w = side_w - out * dot(side_w, out);
    side_w = side_w / max(length(side_w), 1e-6);
    var down_w = down.x * tu + down.y * tv;
    down_w = down_w - out * dot(down_w, out) - side_w * dot(down_w, side_w);
    down_w = down_w / max(length(down_w), 1e-6);

    // The wiper pushes a small ridge ahead of its lip. Its height gradient bends
    // the view through this connected water, using the same sample as the droplets.
    var best = clamp((water - 1.0) * 0.18, 0.0, 1.0);
    let gradient = vec2<f32>((water_dx * duy.y - water_dy * dux.y) / (det * su),
                            (water_dy * dux.x - water_dx * duy.x) / (det * sv));
    if (wiped_film > 0.01 && collectors.a <= 0.01) {
        // The age gradient follows the actual sweep. Fine, directional residual
        // streaks briefly distort the view, without repopulating the pane with drops.
        var along = vec2<f32>(0.0, 1.0);
        if (length(gradient) > 0.01) { along = normalize(vec2<f32>(dot(gradient, side), dot(gradient, down))); }
        let across = vec2<f32>(-along.y, along.x);
        let streak = rain_patches(vec2<f32>(dot(q, across) * 600.0, dot(q, along) * 4.0));
        let strength = wiped_film * (0.012 + 0.018 * streak.x);
        g.cover = strength;
        g.n = normalize(out + (side_w * across.x + down_w * across.y) * (streak.y - 0.5) * wiped_film * 0.003);
        return g;
    }
    var slope = clamp(-0.00005 * vec2<f32>(dot(gradient, side), dot(gradient, down)), vec2<f32>(-0.15), vec2<f32>(0.15)) * best;
    // Heavy rain and a strong relative airstream increase coverage. Three larger
    // droplet layers leave room for filtering the view without adding a render pass.
    let density = wet * wet * (1.0 + 0.65 * smoothstep(0.35, 0.9, wet)
                            + 0.45 * clamp(length(camera.wind.xyz) / 20.0, 0.0, 1.0));

    // --- the airstream: the glass moves through the air with the bus, and a drop's drag
    // grows with the speed squared - at about 40 km/h it matches the drop's weight. Along
    // the glass the air drives drops back; where it meets the glass head on (the
    // windscreen) it parts, up and out. The runners go where weight and air take them
    // together (in sixteenths of a turn, so that they do not shiver with every change of
    // speed); the drops that sit hold on.
    var flow = vec2<f32>(0.0, 1.0);
    var blow = 0.0;
    if (camera.wind.w > 0.5 && near_player_vehicle(world) > 0.5) {
        let air = -camera.wind.xyz;
        var aw = vec2<f32>(dot(air, side_w), dot(air, down_w));
        aw.y = aw.y - abs(dot(air, out)) * 0.6;
        let v2 = dot(aw, aw);
        blow = clamp(v2 / 130.0, 0.0, 3.0);
        flow = flow + aw / max(sqrt(v2), 1e-4) * blow;
    }
    let fa = round(atan2(flow.x, flow.y) / 0.3926991) * 0.3926991;
    // pane coordinates turned so the runners run down +y
    let qr = rain_turn(q, fa);

    // --- the runners: now and then a drop grown heavy breaks loose and slides down on its
    // own - each at its own moment, a few centimetres to a hand's width, nearly straight
    // with a little drift, in jerks (it sticks, then slips on), and stops again; it leaves a
    // cleared track with a few beads in it. (They were lanes of drops on wavy paths all
    // going at once: the pane looked like a wave of snakes.) One chance a cell of 5 x 140 cm;
    // a pixel looks at its own cell and the one above, whose drop may have slid into it.
    let lane_w = 0.05;
    let seg_h = 1.4;
    let lane = floor(qr.x / lane_w);
    let seg0 = floor(qr.y / seg_h);
    var track = 0.0;
    for (var k = 0; k < 2; k = k + 1) {
        if (tracked) { break; } // these panes use persistent, merging drops
        let seg = seg0 - f32(k);
        let h1 = rain_hash(vec2<f32>(lane * 1.7 + 3.0, seg * 2.3 + 11.0));
        if (h1.x >= wet * (0.7 + 0.08 * blow + 0.2 * heavy)) {
            continue;
        }
        let h2 = rain_hash(vec2<f32>(lane * 1.7 + 8.1, seg * 2.3 + 5.1));
        let sit = 0.3 + 0.8 * h2.y;
        let speed = (0.07 + 0.16 * h1.y * h1.y) * (1.0 + 0.7 * blow) * (1.0 + 0.4 * heavy);
        let run_len = 0.9 + 0.3 * h2.y;
        let period = sit + run_len / speed + 0.4;
        let cycle = t / period + h2.x;
        let tau = fract(cycle) * period;
        let h3 = rain_hash(vec2<f32>(lane * 1.7 + 1.3, seg * 2.3 + 29.7)
                           + floor(cycle) * vec2<f32>(13.0, 41.0));
        // sits and grows a moment, then goes in jerks, then lies still till it dries
        let st = max(tau - sit, 0.0);
        let pulse = 0.18 + 0.25 * h3.x;
        // Larger drops overcome pinning more easily. Random surface resistance
        // modulates their speed; consecutive arrivals do not repeat one track.
        let step_len = speed * pulse;
        // Less than one segment: checking this cell and its predecessor sees the
        // complete runner, including its head after it crosses the cell boundary.
        let travel = min((floor(st / pulse) + smoothstep(0.2, 0.9, fract(st / pulse))) * step_len, run_len);
        let vis = smoothstep(0.0, 0.2, tau) * (1.0 - smoothstep(period - 0.4, period, tau));
        let y0 = (seg + 0.02 + 0.08 * h3.x) * seg_h;
        let x0 = (lane + 0.4 + 0.2 * h3.y) * lane_w;
        let drift = (h2.y - 0.5) * 0.025;
        // (a downpour's runners are heavier: wider heads, wider cleared tracks)
        let rh = drop_scale * (0.001 + 0.0012 * h1.y * h1.y) * (1.0 + 0.9 * heavy) * mix(0.3, 1.0, smoothstep(0.0, sit, tau));
        let bend = 0.0015 * sin(travel * (30.0 + 25.0 * h3.y) + h2.x * 6.283185);
        var head_d = vec2<f32>(qr.x - (x0 + drift * travel + bend), qr.y - (y0 + travel));
        head_d.y *= select(0.65, 1.0, head_d.y > 0.0);
        let head = rain_dome(head_d, rh, px);
        if (head.z * vis > best) {
            best = head.z * vis;
            slope = rain_turn(head.xy, -fa);
        }
        // the track it cleared, and a few beads left in it
        let along = qr.y - y0;
        if (along > 0.0 && along < travel - rh) {
            let path_bend = 0.0015 * sin(along * (30.0 + 25.0 * h3.y) + h2.x * 6.283185);
            let dxp = qr.x - (x0 + drift * along + path_bend);
            let hw = rh * 0.7;
            track = max(track, (1.0 - smoothstep(hw * 0.6, hw, abs(dxp))) * vis);
            // A heavy drop draws a narrowing, connected filament behind it. The
            // tail breaks up as the head moves on; lighter runners leave only beads.
            let tail_len = min(travel, (0.08 + 0.4 * h1.y) * wet);
            let tail = smoothstep(travel - tail_len, travel, along);
            let width = rh * (0.12 + 0.35 * tail) * (0.8 + 0.2 * sin(along * 170.0 + h3.x * 6.283185));
            let filament = rain_dome(vec2<f32>(dxp, 0.0), width, px);
            let filament_cover = filament.z * tail * vis;
            if (filament_cover > best) {
                best = filament_cover;
                slope = rain_turn(filament.xy * 0.75, -fa);
            }
            let bc = floor(along / 0.008);
            let bh = rain_hash(vec2<f32>(lane * 3.1 + bc, seg * 1.9 + 41.0));
            if (bh.x < 0.35) {
                let bcen = vec2<f32>((bh.y - 0.5) * hw, (bc + 0.5) * 0.008 - along);
                let bead = rain_dome(vec2<f32>(dxp, 0.0) - bcen, 0.0004 + 0.0005 * bh.y, px);
                if (bead.z * vis > best) {
                    best = bead.z * vis;
                    slope = rain_turn(bead.xy, -fa);
                }
            }
        }
    }

    // Rainfall has the same chance across an equally wet pane. Random arrivals
    // scatter individual drops; only runoff and wiping leave cleared regions.
    track = max(track, max(collectors.b, collectors.a));
    let patches = rain_patches(q * 12.0 + 3.1);

    // --- the drops that sit: fine beads, medium drops and larger merged drops.
    for (var layer = 0; layer < 3; layer = layer + 1) {
        if (tracked && layer == 0) { continue; }
        let fl = f32(layer);
        let cellsz = drop_scale * select(select(0.018, 0.005, layer == 1), 0.0085, layer == 2);
        let turn = 0.61 + fl * 1.37;
        let g2 = rain_turn(q, turn) / cellsz + vec2<f32>(fl * 17.3, fl * 5.1);
        let c = floor(g2);
        let timing = rain_hash(c + 3.7);
        let life = 4.0 + timing.y * 7.0;
        let cycle = t / life + timing.x;
        let ph = fract(cycle);
        let arrival = c + floor(cycle) * vec2<f32>(17.0, 43.0);
        let h = rain_hash(arrival);
        let h2 = rain_hash(arrival + 3.7);
        // landed, grown, drying: a drop comes and goes; more of them the wetter the pane,
        // and the big ones only on a wet pane
        let dens = density * 0.7 * (1.0 - clamp(water - 1.0, 0.0, 1.0)) * select(0.62, 0.48 * smoothstep(0.15, 0.6, wet), layer == 0);
        let present = step(h.x, dens) * smoothstep(0.0, 0.12, ph) * (1.0 - smoothstep(0.7, 1.0, ph)) * (1.0 - track);
        if (present <= 0.0) {
            continue;
        }
        let r = (0.1 + 0.3 * h.y * h.y) * mix(0.3, 1.0, smoothstep(0.0, 0.6, ph)) * mix(0.35, 1.1, smoothstep(0.05, 0.9, wet));
        // (anywhere in the cell it still fits in)
        let room = min(r * 1.12, 0.48);
        let centre = c + vec2<f32>(room) + rain_hash(arrival + 9.1) * (1.0 - 2.0 * room);
        // flattened by its weight (below the pane's own down, not the turned grid's):
        // fuller below than above
        var d = rain_turn((g2 - centre) * cellsz, -turn);
        d.y = d.y * select(1.12, 0.9, d.y > 0.0);
        // no drop is a circle: its rim wanders where the glass held it as it spread, the
        // big ones most, and a heavy one hangs drawn out downwards
        // (a pixel beyond any rim it could have: nothing more to work out)
        let dl = length(d);
        if (dl > r * cellsz * 1.5) {
            continue;
        }
        let h3 = rain_hash(arrival + 23.9);
        // the rim's wander as waves of 2, 3 and 4 round the drop, each turned its own way
        // (sin(k a + phase) from the direction's cosine and sine, without an atan2)
        let u = d / max(dl, 1e-6);
        let c2 = u.x * u.x - u.y * u.y;
        let s2 = 2.0 * u.x * u.y;
        let c3 = u.x * (4.0 * u.x * u.x - 3.0);
        let s3 = u.y * (3.0 - 4.0 * u.y * u.y);
        let c4 = c2 * c2 - s2 * s2;
        let s4 = 2.0 * s2 * c2;
        let p2 = h3 * 2.0 - 1.0;
        let p3 = h2.yx * 2.0 - 1.0;
        let wave = (s2 * p2.x + c2 * p2.y) * h3.y + 0.55 * (s3 * p3.x + c3 * p3.y) + 0.3 * (s4 * p2.y - c4 * p3.x);
        let wob = 1.0 + (0.03 + 0.06 * h.y) * wave;
        d = d / max(wob, 0.4);
        d.y = d.y * mix(1.0, 0.88, h.y * h.y * h3.x);
        // Large drops belong to moving water; stationary spray stays fine.
        let radius = min(r * cellsz, drop_scale * 0.0015);
        let drop = rain_dome(d, radius, px);
        let cover = drop.z * present;
        if (cover > best) {
            best = cover;
            slope = drop.xy;
        }
    }

    // --- the mist: independently scattered droplets of a millimetre or less
    var grain = 0.0;
    {
        let mg = rain_turn(q, 0.33) / 0.0028;
        let mc = floor(mg);
        let mh = rain_hash(mc + 71.0);
        let mr = 0.1 + 0.16 * mh.y;
        let at = mc + vec2<f32>(mr) + rain_hash(mc + 5.3) * (1.0 - 2.0 * mr);
        grain = max(grain, step(mh.x, wet * 0.25) * (1.0 - smoothstep(mr * 0.6, mr, length(mg - at))));
    }
    // (seen from further than a millimetre a pixel: their average)
    let mist = mix(wet * 0.25 * 0.12, grain, smoothstep(0.0015, 0.0006, px));
    // Connected water bends the view between visible beads, rather than greying
    // otherwise sharp glass. Runoff clears this film along with the fine drops.
    let film = smoothstep(0.2, 1.0, wet) * 0.7 * (1.0 - track);
    // (The film only bends the view, through the slope below: it covers nothing. A thin
    // even sheet of water on glass lets nearly all the light through; drawn as a cover of
    // up to 70 % over the picture behind it, it laid a milky veil over every wet pane: a
    // third of the street's contrast left, and darker still in the classic picture.)
    g.mist = clamp(mist * (1.0 - track), 0.0, 1.0);
    // Unresolved water must soften the view, not retain large, sharp lens ripples.
    let facing = abs(dot(out, eye));
    let film_detail = (1.0 - smoothstep(0.003, 0.012, footprint)) * facing * facing;
    slope = slope + (patches - vec2<f32>(0.5)) * film * 0.03 * film_detail * (1.0 - best);

    // The CPU map stores slopes along the mesh projection; recover those two
    // directions from the screen derivatives, including a rotated side window.
    if (collectors.a > best) {
        let pd = pane_dx.x * pane_dy.y - pane_dx.y * pane_dy.x;
        if (abs(pd) > 1e-12) {
            let pu = safe_normal((dpx * pane_dy.y - dpy * pane_dx.y) / pd);
            let pv = safe_normal((dpy * pane_dx.x - dpx * pane_dy.x) / pd);
            let cap = (collectors.rg * 255.0 - vec2<f32>(128.0)) / 127.0;
            g.n = normalize(out + pu * cap.x + pv * cap.y);
            g.cover = collectors.a;
            return g;
        }
    }
    g.cover = best;
    let nz = sqrt(max(1.0 - dot(slope, slope), 0.02));
    g.n = normalize(out * nz + side_w * slope.x + down_w * slope.y);
    return g;
}

// The way the eye looks through a drop: bent at its dome and at the flat glass, like
// through a strong lens (the world shows upside down in it). Zero where the light is
// trapped by total internal reflection: the dark ring round every drop.
fn rain_through(g: RainGlass, v: vec3<f32>) -> vec3<f32> {
    if (dot(g.out, v) > 0.0) {
        // the eye on the rain's side: into the dome, out through the glass
        let t1 = refract(-v, g.n, 1.0 / 1.333);
        if (dot(t1, t1) < 1e-4) {
            return vec3<f32>(0.0);
        }
        return refract(normalize(t1), g.out, 1.333);
    }
    // the eye in the bus: through the glass into the water, out of the dome
    let t1 = refract(-v, -g.out, 1.0 / 1.333);
    if (dot(t1, t1) < 1e-4) {
        return vec3<f32>(0.0);
    }
    return refract(normalize(t1), -g.n, 1.333);
}

// What the eye sees along `through` (the way out of a drop, `rain_through`) from the drop
// at `world`: the clean current street behind the glass, looked up where that
// way meets it a few metres on - through a drop's rim the way bends far round, so the
// drop holds the whole street small and upside down, the sky at its bottom, as a real
// one does. The rain film's reflection slot holds that picture (see `Renderer::glass_slot`);
// without it (a mirror's view) or off the picture's edge,
// `fallback` (the sky's colours). `scale` takes the picture into the caller's units.
fn rain_behind(world: vec3<f32>, through: vec3<f32>, fallback: vec3<f32>, scale: f32, scatter: f32) -> vec3<f32> {
    if (camera.flags.z > -0.5 || dot(through, through) < 1e-4) {
        return fallback;
    }
    let c = camera.view_proj * vec4<f32>(world + normalize(through) * 6.0, 1.0);
    if (c.w <= 0.05) {
        return fallback;
    }
    let ndc = c.xy / c.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let inside = smoothstep(0.0, 0.06, min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y)));
    // A wet film scatters the view. Filter the existing clean scene picture locally:
    // clear glass has a sub-texel footprint, heavy water softens several pixels.
    let offset = vec2<f32>(1.0, 0.6) * scatter * 4.0 / vec2<f32>(textureDimensions(t_env));
    let a = textureSampleLevel(t_env, s_diffuse, clamp(uv - offset, vec2<f32>(0.001), vec2<f32>(0.999)), 0.0).rgb;
    let b = textureSampleLevel(t_env, s_diffuse, clamp(uv + offset, vec2<f32>(0.001), vec2<f32>(0.999)), 0.0).rgb;
    let seen = finite_or((a + b) * (0.5 * scale), fallback);
    return mix(fallback, seen, inside);
}

// A drop lit: `refl` what the dome mirrors, `thru` what is seen through it (both
// radiance), `mist_col` the light the mist scatters, `sun` the sun's strength for the
// sparkles, `d_thru` the way through (from `rain_through`). Colour and cover.
fn rain_light(g: RainGlass, v: vec3<f32>, d_thru: vec3<f32>, refl: vec3<f32>, thru: vec3<f32>, mist_col: vec3<f32>, sun: vec3<f32>) -> vec4<f32> {
    let facing = dot(g.out, v) > 0.0;
    let cosv = clamp(abs(dot(g.n, v)), 0.0, 1.0);
    // (from inside, the dome's inner face mirrors the cab: little of it)
    let f = select(0.02, 0.02 + 0.98 * pow(1.0 - cosv, 5.0), facing);
    let trapped = dot(d_thru, d_thru) < 1e-4;
    // the trapped light at the rim: what the dome mirrors, darkened
    var c = select(thru * 0.96 * (1.0 - f), refl * 0.3, trapped) + refl * f;
    // the sun: a pinpoint where the dome mirrors it, and the drop lit up whole where it
    // focuses the sun towards the eye
    let s = camera.sun_dir.xyz;
    if (facing) {
        let r = reflect(-v, g.n);
        c = c + sun * pow(max(dot(r, s), 0.0), 900.0) * f * 60.0;
    }
    if (!trapped) {
        c = c + sun * pow(max(dot(normalize(d_thru), s), 0.0), 60.0) * 1.5;
    }
    let a_drop = g.cover * 0.94;
    let a_mist = g.mist * (1.0 - a_drop);
    let a = a_drop + a_mist;
    let col = (c * a_drop + mix(mist_col, thru, 0.98) * a_mist) / max(a, 1e-4);
    return vec4<f32>(col, a);
}

// What a drop shows in the classic picture: the sky from the weather's colours, the
// horizon in the fog's, the ground darker.
fn rain_env_vanilla(d: vec3<f32>) -> vec3<f32> {
    let zenith = camera.sky_color.rgb + camera.ambient.rgb * 0.5;
    let horizon = max(camera.fog.rgb, camera.ambient.rgb);
    let ground = camera.ambient.rgb * 0.4 + camera.sun_color.rgb * camera.sun_dir.w * 0.06;
    let sky = mix(horizon, zenith, smoothstep(0.0, 0.5, d.z));
    let c = mix(ground, sky, smoothstep(-0.06, 0.04, d.z));
    return select(c, srgb_decode(c), camera.sky_color.w > 0.5);
}

