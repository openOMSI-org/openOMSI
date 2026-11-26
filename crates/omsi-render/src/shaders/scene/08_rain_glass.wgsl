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

// --- The rain on a pane as all the pictures draw it (the enhanced film, enhanced/07_glass.wgsl
// `glass_film`, and the classic one, scene/09_vanilla.wgsl `glass_film_classic`): the drops laid
// out in the vehicle's own frame (`FsIn.local`), so they stay on the glass however the body
// rolls and pitches and on a door that swings open. The modellers' texture coordinates of a
// film are no measure of the glass: on the windscreen of the NL202 they run half as fast
// again one way as the other and at sixty degrees to each other, from triangle to triangle
// anew, and every drop drawn in them came out a slanting streak (the classic pictures' old
// `rain_glass` did, and was replaced by this). So a pane is laid flat by the axes of the
// vehicle (`pane_proj`).

// The glass at a pixel: the position and the flat normal in the vehicle's frame, the
// normal on the rain's side in the world, the pixel's footprint on the glass (m, its longer
// side and the side of a square of its area), and the screen derivatives of both positions.
struct PaneBase {
    l: vec3<f32>,
    nl: vec3<f32>,
    out: vec3<f32>,
    px: f32,
    px_area: f32,
    dlx: vec3<f32>,
    dly: vec3<f32>,
    dwx: vec3<f32>,
    dwy: vec3<f32>,
    ok: bool,
};

fn pane_base(world: vec3<f32>, local: vec3<f32>, n: vec3<f32>, outside_in: bool) -> PaneBase {
    return pane_base_d(world, local, n, outside_in, dpdx(world), dpdy(world), dpdx(local), dpdy(local));
}

// `pane_base` with the positions' screen derivatives taken by the caller (`dwx`, `dwy` of the
// world position, `dlx`, `dly` of the vehicle's own), where the pixel may be in a branch of
// its own: there a derivative has no meaning, and taken there it brought Apple's M4 shader
// compiler service down (`body_water`).
fn pane_base_d(world: vec3<f32>, local: vec3<f32>, n: vec3<f32>, outside_in: bool, dwx: vec3<f32>, dwy: vec3<f32>, dlx: vec3<f32>, dly: vec3<f32>) -> PaneBase {
    var b: PaneBase;
    b.l = local;
    b.dlx = dlx;
    b.dly = dly;
    b.dwx = dwx;
    b.dwy = dwy;
    // the film faces the eye unless the eye is in the bus behind it (the triangle's own
    // normal: a film mesh's vertex normals are not to be relied on, many have none, and
    // the drops' size followed their noise)
    let eye = normalize(camera.cam_pos.xyz - world);
    let gn = cross(b.dwx, b.dwy);
    var out = select(safe_normal(n), gn / max(length(gn), 1e-20), dot(gn, gn) > 1e-24);
    out = select(-out, out, dot(out, eye) >= 0.0);
    out = select(out, -out, outside_in);
    b.out = out;
    b.px = max(max(length(b.dwx), length(b.dwy)), 1e-5);
    b.px_area = max(sqrt(length(cross(b.dwx, b.dwy))), 1e-5);
    let c = cross(b.dlx, b.dly);
    let cl = length(c);
    b.ok = cl > 1e-12;
    b.nl = select(vec3<f32>(0.0, 1.0, 0.0), c / max(cl, 1e-20), b.ok);
    return b;
}

// A direction on the glass in the vehicle's frame, in the world (through the two positions'
// screen derivatives).
fn pane_to_world(b: PaneBase, t: vec3<f32>) -> vec3<f32> {
    let g = vec3<f32>(dot(b.dlx, b.dlx), dot(b.dlx, b.dly), dot(b.dly, b.dly));
    let det = g.x * g.z - g.y * g.y;
    if (abs(det) < 1e-24) {
        return vec3<f32>(0.0);
    }
    let r = vec2<f32>(dot(b.dlx, t), dot(b.dly, t));
    let a = (g.z * r.x - g.y * r.y) / det;
    let c = (g.x * r.y - g.y * r.x) / det;
    return a * b.dwx + c * b.dwy;
}

// The pane laid flat along two of the vehicle's axes (`axis`: 0 across and up, the
// windscreen and the rear window; 1 along and up, the side windows; 2 the plan, a roof
// light): where the pixel is (m: x across, y down), how an offset there is taken back onto
// the glass (`m`: metres along `side_w` and `down_w`, the glass's own across and down in
// the world), and the projection's screen derivatives (for the wiper's ridge).
struct PaneProj {
    q: vec2<f32>,
    m: mat2x2<f32>,
    side_w: vec3<f32>,
    down_w: vec3<f32>,
    dqx: vec2<f32>,
    dqy: vec2<f32>,
    steep: bool,
};

fn pane_proj(b: PaneBase, axis: i32) -> PaneProj {
    var p: PaneProj;
    var ea = vec3<f32>(1.0, 0.0, 0.0);
    var eb = vec3<f32>(0.0, 0.0, -1.0);
    var ec = vec3<f32>(0.0, 1.0, 0.0);
    if (axis == 1) {
        ea = vec3<f32>(0.0, 1.0, 0.0);
        ec = vec3<f32>(1.0, 0.0, 0.0);
    } else if (axis == 2) {
        eb = vec3<f32>(0.0, 1.0, 0.0);
        ec = vec3<f32>(0.0, 0.0, 1.0);
    }
    let nl = b.nl;
    let nc = dot(nl, ec);
    let ncs = select(nc, select(-1e-3, 1e-3, nc >= 0.0), abs(nc) < 1e-3);
    p.q = vec2<f32>(dot(b.l, ea), dot(b.l, eb));
    p.dqx = vec2<f32>(dot(b.dlx, ea), dot(b.dlx, eb));
    p.dqy = vec2<f32>(dot(b.dly, ea), dot(b.dly, eb));
    // the glass under a step of the projection's two axes
    let t1 = ea - ec * (dot(nl, ea) / ncs);
    let t2 = eb - ec * (dot(nl, eb) / ncs);
    // the glass's own down (the steepest way on it) and across, in the vehicle's frame
    p.steep = axis != 2;
    var dl = vec3<f32>(0.0, 0.0, -1.0) + nl * nl.z;
    dl = select(normalize(t2), dl / max(length(dl), 1e-6), dot(dl, dl) > 0.02 && p.steep);
    var sl = t1 - dl * dot(t1, dl);
    sl = sl / max(length(sl), 1e-6);
    p.m = mat2x2<f32>(vec2<f32>(dot(t1, sl), dot(t1, dl)), vec2<f32>(dot(t2, sl), dot(t2, dl)));
    // the same two ways in the world, square to the normal on the rain's side
    var sw = pane_to_world(b, sl);
    sw = sw - b.out * dot(sw, b.out);
    var dw = pane_to_world(b, dl);
    dw = dw - b.out * dot(dw, b.out);
    p.side_w = sw / max(length(sw), 1e-6);
    dw = dw - p.side_w * dot(dw, p.side_w);
    p.down_w = dw / max(length(dw), 1e-6);
    return p;
}

// The water on a pane at a pixel: the surface's normal (world, out of the water), how much
// of the pixel a drop or a bead covers and how far out on it the pixel lies (0 its middle,
// 1 its rim), the drop's radius in pixels, the pane's normal on the rain's side, the fine
// mist of droplets under a millimetre, and how much of the view through the pane the
// connected water scatters into a soft blur (`veil`).
struct GlassWater {
    n: vec3<f32>,
    cover: f32,
    out: vec3<f32>,
    mist: f32,
    veil: f32,
    rim: f32,
    size_px: f32,
};

// One drop of radius `r` (m) at `d` (m on the glass, across / down): the slope of its dome
// (the normal's part along the glass), its cover of the pixel (`px`: the pixel's size on
// the glass) and how far out on it the pixel lies. A shallow cap, as water spreading on
// glass makes; a drop smaller than the pixel keeps its share of it, its slopes flattened
// to their average.
fn glass_dome(d: vec2<f32>, r: f32, px: f32) -> vec4<f32> {
    let rr = length(d) / max(r, 1e-6);
    let aa = clamp(px / max(r, 1e-6), 0.02, 1.0);
    let cover = 1.0 - smoothstep(1.0 - aa, 1.0 + aa, rr);
    let seen = smoothstep(0.0, 0.8, r / px);
    let cap = mix(0.15, 0.55, smoothstep(0.7, 2.5, r / px));
    let s = d / max(r, 1e-6) * cap * (0.6 + 0.4 * rr * rr);
    return vec4<f32>(s, cover * seen, min(rr, 1.0));
}

// What covers the pixel, gathered over the layers: the cover, the slope, the rim and the
// drop's size in pixels of the drop on top.
struct DropAcc {
    cover: f32,
    slope: vec2<f32>,
    rim: f32,
    size_px: f32,
    track: f32,
    // the cover of the drops too small for the pixel to see one by one, as their average
    // (taken into the mist, so that they read as an even haze, not as random specks)
    avg: f32,
};

fn drop_take(a: ptr<function, DropAcc>, d: vec4<f32>, r: f32, px: f32) {
    if (d.z > (*a).cover) {
        (*a).cover = d.z;
        (*a).slope = d.xy;
        (*a).rim = d.w;
        (*a).size_px = r / px;
    }
}

// One layer of the drops that sit on the glass: a jittered grid of cells `cell` metres
// wide (of the projection), each holding a drop now and then (`occ` the share of cells
// that hold one). A drop lands, spreads to its size within a moment and sits; after its
// own while it is gone again, taken by a runner or merged into a neighbour, and the next
// lands somewhere else in the cell - all fading, nothing popping.
fn glass_bead(a: ptr<function, DropAcc>, p: PaneProj, layer: f32, cell: f32, occ: f32, r0: f32, size: f32, t: f32, px: f32) {
    // (a layer whose drops are smaller than the pixel, or cells no wider than it - a mirror's
    // small picture, the glass seen from afar - is taken at its average cover: sampled one
    // random cell a pixel, it drew white noise)
    // Filter by the typical bead diameter. Using its minimum radius suppressed
    // distinguishable drops on oblique panes and at reduced render scales.
    let diameter = 2.0 * (4.0 / 3.0) * r0 * size;
    let fine = max(1.0 - smoothstep(0.7, 1.5, diameter / px), 1.0 - smoothstep(1.0, 2.5, cell / px));
    if (fine > 0.0) {
        let rr = 1.6 * r0 * size / cell;
        (*a).avg = (*a).avg + fine * occ * min(3.14159 * rr * rr, 1.0);
        if (fine > 0.99) {
            return;
        }
    }
    let turn = 0.61 + layer * 1.37;
    let g2 = rain_turn(p.q, turn) / cell + vec2<f32>(layer * 17.3, layer * 5.1);
    let c = floor(g2);
    let timing = rain_hash(c + vec2<f32>(3.7, 9.2 + layer));
    let life = (9.0 + layer * 4.0) * (1.0 + timing.y);
    let cycle = t / life + timing.x;
    let ph = fract(cycle);
    let arrival = c + floor(cycle) * vec2<f32>(17.0, 43.0);
    let h = rain_hash(arrival);
    // (a share of cells rising with the wetness: a drop fades in over a small rise of it)
    let shown = clamp((occ - h.x) * 20.0, 0.0, 1.0) * smoothstep(0.0, 0.03, ph) * (1.0 - smoothstep(0.9, 1.0, ph)) * (1.0 - fine);
    if (shown <= 0.0) {
        return;
    }
    // (`r0` the layer's smallest drop, the largest twice that: rain on glass sits as drops of
    // half a millimetre to four across - drawn up to a share of the cell they made drops of a
    // centimetre and more, few and far between)
    let radius = r0 * (1.0 + h.y * h.y) * mix(0.6, 1.0, smoothstep(0.0, 0.2, ph)) * size;
    // (anywhere in the cell the largest still fits in - on the glass it is never smaller
    // than in the projection - and the same place whatever its size: the size follows the
    // wetness, which changes across the glass)
    let room = min(r0 * 3.0 / cell, 0.45);
    let centre = c + vec2<f32>(room) + rain_hash(arrival + 9.1) * (1.0 - 2.0 * room);
    let dq = rain_turn((g2 - centre) * cell, -turn);
    if (length(dq) > radius * 1.3) {
        return;
    }
    var d = p.m * dq;
    // No drop is a circle: its rim wanders where the glass held it as it spread, and its
    // weight draws it a little fuller below than above.
    let h3 = rain_hash(arrival + 23.9);
    let dl = max(length(d), 1e-7);
    let u = d / dl;
    let c2 = u.x * u.x - u.y * u.y;
    let s2 = 2.0 * u.x * u.y;
    let c3 = u.x * (4.0 * u.x * u.x - 3.0);
    let s3 = u.y * (3.0 - 4.0 * u.y * u.y);
    let p2 = h3 * 2.0 - 1.0;
    let wave = (s2 * p2.x + c2 * p2.y) * 0.7 + 0.45 * (s3 * p2.y - c3 * p2.x);
    d = d / (1.0 + (0.03 + 0.05 * h.y) * wave);
    d.y = d.y * select(1.06, 0.94, d.y > 0.0);
    let dome = glass_dome(d, radius, px);
    drop_take(a, vec4<f32>(dome.xy, dome.z * shown, dome.w), radius, px);
}

// A runner's sideways wander as it goes down (m): a smooth random path of its own.
fn glass_meander(s: f32, seed: vec2<f32>, amp: f32) -> f32 {
    let w = rain_patches(vec2<f32>(s * 9.0 + seed.x, seed.y));
    return amp * ((w.x - 0.5) * 1.5 + (w.y - 0.5) * 0.5);
}

// The runners in a lattice of lanes (`qr`: the projection turned so they run down +y, `mr`
// taking its offsets onto the glass): now and then a drop grown heavy breaks loose and
// slides a hand's breadth or two down, in fits and starts, wandering a little from side to
// side, and stops again. It sweeps a narrow clear track through the small drops, which
// fill in again as the rain falls on it; behind it lies a short wet trail that drains
// within a second, and a few beads it left. One chance a cell of 4.5 cm x 1.2 m; a pixel
// looks at its own cell and the one above, whose drop may have slid into it.
fn glass_runners(a: ptr<function, DropAcc>, qr: vec2<f32>, mr: mat2x2<f32>, seed: f32, wet: f32, heavy: f32, blow: f32, size: f32, t: f32, px: f32) {
    // (lanes of 4.5 cm and cells 1.2 m tall, few of them running at once: with lanes of 2 cm
    // and cells of 0.8 m nearly every lane ran in a downpour, and their tracks cut the glass
    // into wide clear stripes, #4153)
    let lane_w = 0.045;
    let seg_h = 1.2;
    let lane = floor(qr.x / lane_w);
    let seg0 = floor(qr.y / seg_h);
    for (var k = 0; k < 2; k = k + 1) {
        let seg = seg0 - f32(k);
        let key = vec2<f32>(lane * 1.7 + seed * 101.0, seg * 2.3 + seed * 37.0);
        let h1 = rain_hash(key + vec2<f32>(3.0, 11.0));
        if (h1.x >= wet * (0.15 + 0.15 * heavy + 0.1 * blow)) {
            continue;
        }
        let h2 = rain_hash(key + vec2<f32>(8.1, 5.1));
        // (it gathers a while, runs, and its trail fills in with new rain)
        // (a drop grown heavy on upright glass runs off at some ten to twenty centimetres a
        // second, most of the way down the pane)
        let sit = 0.6 + 2.4 * h2.y;
        let v = (0.08 + 0.16 * h1.y * h1.y) * (1.0 + 0.8 * blow) * (1.0 + 0.4 * heavy);
        let run_len = 0.12 + 0.4 * h2.x;
        let run_t = run_len / v;
        let refill = mix(3.0, 1.5, wet);
        let period = sit + run_t + refill;
        let cycle = t / period + h2.x;
        let tau = fract(cycle) * period;
        let h3 = rain_hash(key + vec2<f32>(1.3, 29.7) + floor(cycle) * vec2<f32>(13.0, 41.0));
        // stick and slip: v s - v P / 2 pi sin(2 pi s / P) halts once a pulse P
        let pulse = 0.3 + 0.4 * h3.x;
        let st = clamp(tau - sit, 0.0, run_t * 1.05);
        let travel = min(v * st - v * pulse * 0.1591549 * sin(6.283185 * st / pulse), run_len);
        let vis = smoothstep(0.0, 0.6, tau) * (1.0 - smoothstep(period - 1.0, period, tau));
        let y0 = (seg + 0.02 + 0.32 * h3.x) * seg_h;
        let x0 = (lane + 0.3 + 0.4 * h3.y) * lane_w;
        let wseed = h3 * 57.0;
        let amp = 0.002 + 0.002 * h2.y;
        if (abs(qr.x - x0) > lane_w * 0.5 + amp * 1.5 || qr.y < y0 - 0.01 || qr.y > y0 + run_len + 0.01) {
            continue;
        }
        // (a downpour's runners are heavier; a drop gathers before it goes)
        let rh = size * (0.0012 + 0.0008 * h1.y) * (1.0 + 0.3 * heavy) * mix(0.6, 1.0, smoothstep(0.0, sit, tau));
        var head_d = mr * vec2<f32>(qr.x - (x0 + glass_meander(travel, wseed, amp)), qr.y - (y0 + travel));
        // (drawn out a little behind, full in front)
        head_d.y = head_d.y * select(0.8, 1.0, head_d.y > 0.0);
        let head = glass_dome(head_d, rh, px);
        // (once stopped it sits at the end of its run and is taken into the new rain)
        let head_vis = vis * (1.0 - smoothstep(sit + run_t, sit + run_t + refill * 0.8, tau));
        drop_take(a, vec4<f32>(head.xy, head.z * head_vis, head.w), rh, px);
        let along = qr.y - y0;
        if (along > 0.0 && along < travel - rh * 0.5) {
            let dxq = qr.x - (x0 + glass_meander(along, wseed, amp));
            let dx = (mr * vec2<f32>(dxq, 0.0)).x;
            // how long ago the head passed here
            let age = max(tau - sit - along / v, 0.0);
            let hw = rh * 0.9;
            let cleared = (1.0 - smoothstep(hw * 0.6, hw * 1.1, abs(dx))) * (1.0 - smoothstep(refill * 0.4, refill, age)) * vis;
            (*a).track = max((*a).track, cleared);
            // the thin wet trail it draws, draining within a second: a faint lens, no strip
            let tw = rh * 0.45 * exp(-age * 2.0);
            let trail = glass_dome(vec2<f32>(dx, 0.0), max(tw, 1e-5), px);
            let trail_cover = trail.z * 0.35 * (1.0 - smoothstep(0.2, 1.0, age)) * vis;
            drop_take(a, vec4<f32>(trail.xy * 0.4, trail_cover, trail.w), tw, px);
            // ... and a few beads, left behind where the trail broke
            let bc = floor(along / 0.009);
            let bh = rain_hash(vec2<f32>(lane * 3.1 + bc, seg * 1.9 + 41.0 + seed * 7.0));
            if (bh.x < 0.3) {
                let bcen = vec2<f32>((bh.y - 0.5) * hw * 0.8, ((bc + 0.5) * 0.009 - along) * (mr * vec2<f32>(0.0, 1.0)).y);
                let br = 0.0004 + 0.0005 * bh.y;
                let bead = glass_dome(vec2<f32>(dx, 0.0) - bcen, br, px);
                let bead_cover = bead.z * smoothstep(0.4, 1.0, age) * (1.0 - smoothstep(refill * 0.5, refill, age)) * vis;
                drop_take(a, vec4<f32>(bead.xy, bead_cover, bead.w), br, px);
            }
        }
    }
}

// The drops on one projection of the pane (`w` its share, see `glass_water`).
fn glass_drops(a: ptr<function, DropAcc>, b: PaneBase, p: PaneProj, w: f32, wet: f32, heavy: f32, bank: f32, size: f32, world: vec3<f32>, t: f32) {
    // --- the airstream: a drop's drag grows with the speed squared, and at some 40 km/h it
    // matches its weight. Along the glass the air drives the water back; where it meets the
    // glass head on (the windscreen) it parts, up and out. The runners go where weight and
    // air take them together; the drops that sit hold on.
    var flow = vec2<f32>(0.0, 1.0);
    var blow = 0.0;
    if (camera.wind.w > 0.5 && near_player_vehicle(world) > 0.5) {
        let air = -camera.wind.xyz;
        var aw = vec2<f32>(dot(air, p.side_w), dot(air, p.down_w));
        aw.y = aw.y - abs(dot(air, b.out)) * 0.6;
        let v2 = dot(aw, aw);
        blow = clamp(v2 / 130.0, 0.0, 3.0);
        flow = flow + aw / max(sqrt(v2), 1e-4) * blow;
    }
    if (p.steep) {
        // The runners' lanes are laid along the way the water runs, in sixteenths of a turn
        // (turned freely, a degree moved every lane a metre off by its width). Between two
        // such ways the two lattices cross-fade, so a change of speed fades the runners
        // over instead of jumping them. (`flow` is on the glass; the lanes in the
        // projection, whose down is the glass's down.)
        let fstep = 0.3926991;
        let fa = atan2(flow.x, flow.y) / fstep;
        let f0 = floor(fa);
        let fw = smoothstep(0.35, 0.65, fa - f0);
        for (var k = 0; k < 2; k = k + 1) {
            let wk = select(1.0 - fw, fw, k == 1) * w;
            if (wk < 0.01) {
                continue;
            }
            let ang = (f0 + f32(k)) * fstep;
            let cs = vec2<f32>(cos(ang), sin(ang));
            let mr = p.m * mat2x2<f32>(vec2<f32>(cs.x, -cs.y), vec2<f32>(cs.y, cs.x));
            glass_runners(a, rain_turn(p.q, ang), mr, f0 + f32(k), wet * wk, heavy, blow, size, t, b.px_area);
        }
    }
    // --- the drops that sit: four sizes, a great many small ones and fewer large ones (radius
    // 1 to 2, 1.6 to 3.2, 2.4 to 4.8 and 3.4 to 6.8 mm: smaller ones, true to a drizzle's,
    // were under a pixel from the driver's seat and drew the windscreen as an even grain),
    // all fewer in the runners' fresh
    // tracks and under the bank
    let room = (1.0 - (*a).track) * (1.0 - bank) * w;
    for (var layer = 0; layer < 4; layer = layer + 1) {
        let fl = f32(layer);
        var cell = 0.0068;
        var r0 = 0.001;
        var occ = 0.85 * smoothstep(0.02, 0.25, wet);
        if (layer == 1) {
            cell = 0.0108;
            r0 = 0.0016;
            occ = 0.75 * smoothstep(0.05, 0.4, wet);
        } else if (layer == 2) {
            cell = 0.016;
            r0 = 0.0024;
            occ = 0.6 * smoothstep(0.15, 0.6, wet);
        } else if (layer == 3) {
            cell = 0.023;
            r0 = 0.0034;
            occ = 0.4 * smoothstep(0.35, 0.9, wet);
        }
        glass_bead(a, p, fl, cell, occ * room, r0, size, t, b.px_area);
    }
}

// The rain on a pane at a pixel (see the head of this file). `water` is the pane's
// wetness (0..1; below 0 a blade has just wiped it, above 1 the bank a blade pushed),
// `t` the animation clock, `outside_in` true for the eye in the bus.
fn glass_water(world: vec3<f32>, local: vec3<f32>, n: vec3<f32>, water: f32, t: f32, outside_in: bool) -> GlassWater {
    let b = pane_base(world, local, n, outside_in);
    let wdx = dpdx(water);
    let wdy = dpdy(water);
    var g: GlassWater;
    g.n = b.out;
    g.out = b.out;
    g.cover = 0.0;
    g.mist = 0.0;
    g.veil = 0.0;
    g.rim = 0.0;
    g.size_px = 0.0;
    let wet = clamp(water, 0.0, 1.0);
    let wiped = clamp(-water * 25.0, 0.0, 1.0);
    let bank = clamp((water - 1.0) * 0.18, 0.0, 1.0);
    if (!b.ok || (wet <= 0.01 && wiped <= 0.01 && bank <= 0.0)) {
        return g;
    }
    // Which way the pane is laid flat: by the larger of its normal's parts across and along
    // the vehicle, the two cross-faded over a band round 45 degrees (a windscreen's curved
    // corner), each drop of either set coming or going whole; a pane facing up is a roof's.
    let an = abs(b.nl);
    let roof = an.z > max(an.x, an.y) * 2.0;
    let w_front = select(smoothstep(-0.12, 0.12, an.y - an.x), 1.0, roof);
    let main_axis = select(select(1, 0, w_front >= 0.5), 2, roof);
    let p = pane_proj(b, main_axis);
    // the wetness's gradient on the glass (from its screen derivatives)
    let jd = p.dqx.x * p.dqy.y - p.dqx.y * p.dqy.x;
    var grad = vec2<f32>(0.0);
    if (abs(jd) > 1e-14) {
        grad = vec2<f32>(wdx * p.dqy.y - wdy * p.dqx.y, wdy * p.dqx.x - wdx * p.dqy.x) / jd;
    }
    if (wiped > 0.01) {
        // Just wiped: fine streaks along the blade's sweep (the age gradient follows it)
        // bend the view for a moment, without bringing the drops back.
        var along = vec2<f32>(0.0, 1.0);
        if (length(grad) > 0.01) {
            along = normalize(grad);
        }
        let across = vec2<f32>(-along.y, along.x);
        let streak = rain_patches(vec2<f32>(dot(p.q, across) * 600.0, dot(p.q, along) * 4.0));
        g.cover = wiped * (0.012 + 0.018 * streak.x);
        g.veil = wiped * 0.06;
        g.size_px = 4.0;
        g.n = normalize(b.out + (p.side_w * across.x + p.down_w * across.y) * (streak.y - 0.5) * wiped * 0.003);
        return g;
    }
    let heavy = smoothstep(0.55, 1.0, wet);
    // (a drizzle leaves beads, a downpour full drops; on an upright side pane, which the rain
    // meets edge on and the airstream sweeps, they stay smaller than on a raked windscreen)
    let size = mix(0.85, 1.1, smoothstep(0.1, 0.8, wet)) * mix(0.8, 1.0, smoothstep(0.0, 0.4, abs(b.out.z)));
    var acc: DropAcc;
    acc.cover = 0.0;
    acc.slope = vec2<f32>(0.0);
    acc.rim = 0.0;
    acc.size_px = 0.0;
    acc.track = 0.0;
    acc.avg = 0.0;
    // the bank a blade pushed: its ridge bends the view
    if (bank > 0.0) {
        acc.cover = bank;
        acc.slope = clamp(-0.00005 * grad, vec2<f32>(-0.15), vec2<f32>(0.15)) * bank;
        acc.size_px = 8.0;
    }
    let w_main = select(select(1.0 - w_front, w_front, main_axis == 0), 1.0, roof);
    glass_drops(&acc, b, p, w_main, wet, heavy, bank, size, world, t);
    var side_w = p.side_w;
    var down_w = p.down_w;
    if (w_main < 0.99) {
        let p2 = pane_proj(b, 1 - main_axis);
        glass_drops(&acc, b, p2, 1.0 - w_main, wet, heavy, bank, size, world, t);
    }
    // --- the mist: droplets of a millimetre and less, too many to draw one by one far off
    var grain = 0.0;
    {
        let mg = rain_turn(p.q, 0.33) / 0.0024;
        let mc = floor(mg);
        let mh = rain_hash(mc + 71.0);
        let mr = 0.1 + 0.16 * mh.y;
        let at = mc + vec2<f32>(mr) + rain_hash(mc + 5.3) * (1.0 - 2.0 * mr);
        grain = step(mh.x, wet * 0.3) * (1.0 - smoothstep(mr * 0.6, mr, length(mg - at)));
    }
    // (seen from further than a millimetre a pixel: their average)
    let mist = mix(wet * 0.3 * 0.12, grain, smoothstep(0.0015, 0.0006, b.px));
    g.mist = clamp((mist + 0.6 * acc.avg) * (1.0 - acc.track), 0.0, 1.0);
    // The connected water between the drops scatters some of the view into a soft haze
    // (the veil, drawn from the picture's blurred levels), the same on every pane as wet.
    // A runner's fresh track is a clear sheet of water, sharper than the rest.
    let film = smoothstep(0.03, 0.4, wet) * (1.0 - 0.3 * acc.track);
    // (by the air's temperature, `camera.lod.w`: on cold glass the water spreads into a film
    // and a fine mist of droplets that scatter the view - near freezing the sleet's slush on
    // top - while in a warm summer rain it beads up and runs off, the glass between the drops
    // nearly clear)
    let cold = 1.0 - smoothstep(2.0, 24.0, camera.lod.w);
    // (light: a strong veil hid the drops behind a uniform fog)
    g.veil = film * mix(0.12, 0.25, heavy) * mix(0.7, 1.2, cold);
    g.cover = acc.cover;
    g.rim = acc.rim;
    g.size_px = acc.size_px;
    let slope = acc.slope;
    let nz = sqrt(max(1.0 - dot(slope, slope), 0.02));
    g.n = normalize(b.out * nz + side_w * slope.x + down_w * slope.y);
    return g;
}

// The picture behind the glass (`glass_mips.rs`, pre-exposed radiance) at `uv`, blurred to
// level `lod`: five reads round the point a texel of that level apart, so the small levels'
// texels do not show as blocks when they are spread over many pixels. (Through `s_ao`, a
// plain trilinear sampler: the material's own `s_diffuse` is anisotropic, and read at an
// explicit level some Windows drivers took its small levels unfiltered - the street behind
// a wet windscreen in squares of four and eight pixels, #4258.)
fn glass_blur(uv: vec2<f32>, lod: f32) -> vec3<f32> {
    let dims = vec2<f32>(textureDimensions(t_env));
    let o = exp2(lod) * 0.75 / dims;
    let c = clamp(uv, vec2<f32>(0.001), vec2<f32>(0.999));
    var s = textureSampleLevel(t_env, s_ao, c, lod).rgb * 2.0;
    s = s + textureSampleLevel(t_env, s_ao, clamp(c + vec2<f32>(o.x, o.y * 0.5), vec2<f32>(0.001), vec2<f32>(0.999)), lod).rgb;
    s = s + textureSampleLevel(t_env, s_ao, clamp(c + vec2<f32>(-o.x * 0.5, o.y), vec2<f32>(0.001), vec2<f32>(0.999)), lod).rgb;
    s = s + textureSampleLevel(t_env, s_ao, clamp(c + vec2<f32>(-o.x, -o.y * 0.5), vec2<f32>(0.001), vec2<f32>(0.999)), lod).rgb;
    s = s + textureSampleLevel(t_env, s_ao, clamp(c + vec2<f32>(o.x * 0.5, -o.y), vec2<f32>(0.001), vec2<f32>(0.999)), lod).rgb;
    return s / 6.0;
}

// The snow a pane holds while it snows (see enhanced/07_glass.wgsl `glass_snow`):
// `defrost` how far the windscreen's defroster has cleared it (0..0.85), `fz` the height on
// the glass in the bus's frame (-1 its bottom, 1 its top), for the drift along the bottom.
fn glass_snow_with(in: FsIn, cover: f32, defrost: f32, fz: f32) -> vec4<f32> {
    if (cover <= 0.0) {
        return vec4<f32>(1.0, 1.0, 1.0, 0.0);
    }
    // (laid out on the glass as the drops are, `pane_proj`: the film's own coordinates
    // drew the crystals and the drifts out long on the windscreen)
    let b = pane_base(in.world, in.local, in.normal, false);
    let an = abs(b.nl);
    let roof = an.z > max(an.x, an.y) * 2.0;
    let axis = select(select(1, 0, an.y >= an.x), 2, roof);
    let m = pane_proj(b, axis).q;
    let t = camera.post.y;
    // The defroster warms the glass that faces ahead on the panes the wipers sweep. By the
    // pane's facing and not by its film's slot: a windscreen is several slots, a side one
    // among them at its wrapped corner, and the slot's flag melted the snow on one and kept it
    // on the next, a straight seam across the glass.
    let ahead = select(smoothstep(-0.12, 0.12, an.y - an.x), 0.0, roof) * select(0.0, 1.0, material.wipe_bounds.z != 0.0);
    let heat = mix(0.15, 0.45 + 0.55 * clamp(defrost / 0.85, 0.0, 1.0), ahead);
    // The flakes as they land: soft clumps of a few millimetres (sharp six-armed stars a
    // centimetre across were each a star drawn on the street behind), each staying a while
    // and melting into a little water - soon on the warm windscreen, slowly on a side window.
    // Where a flake is smaller than a pixel it is drawn as the share of the glass it covers
    // (a speckle of single pixels flickered as the view moved); where its cell is, as the
    // share all of them cover.
    let px = b.px;
    var flakes = 0.0;
    var melt_water = 0.0;
    for (var layer = 0; layer < 2; layer = layer + 1) {
        let fl = f32(layer);
        let cell = select(0.007, 0.012, layer == 1);
        let density = clamp(cover * select(1.15, 0.85, layer == 1), 0.0, 1.0);
        let p = m / cell + fl * 37.0;
        let c = floor(p);
        let tm = rain_hash(c + vec2<f32>(5.0, 1.0 + fl));
        let life = mix(40.0, 6.0, heat) * (0.6 + 0.8 * tm.y);
        let cycle = t / life + tm.x;
        let ph = fract(cycle);
        let arrival = c + floor(cycle) * vec2<f32>(19.0, 47.0);
        let h = rain_hash(arrival);
        let h2 = rain_hash(arrival + vec2<f32>(91.0, 13.0));
        // (a flake shows once the cover passes its own threshold: they come one by one)
        let shown = clamp((density - h2.y) * 12.0, 0.0, 1.0) * smoothstep(0.0, 0.02, ph);
        // the second half of its while it melts: it shrinks and goes clear
        let melt = smoothstep(0.45, 0.95, ph);
        let d = (p - (c + 0.3 + 0.4 * h)) * cell;
        let r = cell * (0.11 + 0.13 * h2.x) * (1.0 - 0.5 * melt);
        // a clump is round only roughly: three soft lobes, turned its own way
        let lobe = 0.88 + 0.12 * cos(3.0 * atan2(d.y, d.x) + h.x * 6.283185);
        let aa = max(px * 0.7, r * 0.2);
        let edge = 1.0 - smoothstep(r * lobe - aa, r * lobe + aa, length(d));
        let detail = smoothstep(0.5, 1.5, r / px);
        let own = mix(shown * 3.14159 * r * r / (cell * cell), shown * edge, detail);
        // (the cells' average: a flake shows for some 70 % of its while, solid for some half)
        let mean = density * 0.55 * 3.14159 * 0.0324;
        let lit = mix(mean, own, smoothstep(1.0, 3.0, cell / px));
        flakes = max(flakes, lit * (0.7 + 0.3 * h.x) * (1.0 - melt));
        melt_water = max(melt_water, mix(mean * 0.5, shown * edge * detail, smoothstep(1.0, 3.0, cell / px)) * melt * (1.0 - smoothstep(0.92, 1.0, ph)));
    }
    // The settled layer closes evenly as the cover grows, its edge broken up at the scale of
    // a hand (a noise of a few centimetres drew a blotchy film over the whole pane), deepest
    // along the windscreen's bottom edge, where the blades and the airstream do not reach.
    let lumps = snow_noise(m / 0.11) * 0.75 + snow_noise(m / 0.035 + 7.0) * 0.25;
    var drift = 0.0;
    // (by the height in the bus's own frame, not by the wiper mask's `wipe_uv`: a
    // windscreen's film is several slots - each wiper's field and the glass round them -
    // each with its own mask and its own 0 .. 1, and a bank laid along their edges drew
    // seams down the middle of the windscreen and round the wipers' arcs)
    if (camera.inside_c.w > 0.5 && ahead > 0.0) {
        let low = 1.0 - (fz * 0.5 + 0.5);
        drift = ahead * smoothstep(0.5, 0.75, low + 0.12 * (lumps - 0.5)) * smoothstep(0.1, 0.6, cover);
    }
    let c = cover * (0.8 + 0.4 * lumps) * (1.0 - 0.3 * heat) + drift * 0.9;
    let sheet = smoothstep(0.65, 1.25, c) * 0.72 + smoothstep(1.05, 1.7, c) * 0.22;
    let white = mix(vec3<f32>(0.9, 0.92, 0.96), vec3<f32>(0.97, 0.98, 1.0), smoothstep(1.0, 1.8, c));
    let solid = max(flakes * 0.9, sheet);
    // (the melt water: a faint clear bead, the street a shade darker through it)
    let wet = melt_water * 0.16 * (1.0 - solid);
    let alpha = clamp(solid + wet, 0.0, 0.97);
    let col = (white * solid + vec3<f32>(0.5, 0.55, 0.6) * wet) / max(solid + wet, 1e-4);
    return vec4<f32>(col, alpha);
}

// The rain on a pane that has no film of its own to carry it (`glass_film`): the windows of
// the traffic's cars and buses, seen from the street. OMSI gives only some buses a rain film,
// and only the player's own wets it, so every other window stayed dry and clear in a downpour.
// The same drops and runners (`glass_water`), as wet as the roads (`camera.shadow.w`); none
// on the player's own bus, whose films carry its rain.
fn pane_rain(world: vec3<f32>, local: vec3<f32>, n: vec3<f32>) -> GlassWater {
    let wet = clamp(camera.shadow.w * 1.3, 0.0, 1.0);
    let own = near_player_vehicle(world) > 0.5;
    let far = distance(world, camera.cam_pos.xyz) > 80.0;
    let w = select(wet, 0.0, own || far);
    return glass_water(world, local, n, w, camera.post.y, false);
}

// The pane's colour `rgb` and cover `a` under that rain (`vn` towards the eye): the drops,
// small lenses darker than the glass round them with the sky mirrored on their domes
// (`refl`, what the dome mirrors), and between them the wet film, a little milky
// (`haze`, the daylight it scatters) and less see-through.
fn pane_rain_compose(g: GlassWater, rgb: vec3<f32>, a: f32, vn: vec3<f32>, refl: vec3<f32>, haze: vec3<f32>) -> vec4<f32> {
    let cosv = clamp(abs(dot(g.n, vn)), 0.0, 1.0);
    let f = 0.02 + 0.98 * pow(1.0 - cosv, 5.0);
    let drop = mix(rgb * 0.7, refl, clamp(f * 3.0, 0.15, 0.8));
    var c = mix(rgb, drop, g.cover * 0.85);
    let film = clamp(g.veil * 0.35 + g.mist * 0.2, 0.0, 0.5);
    c = mix(c, haze, film);
    let a2 = clamp(a + (1.0 - a) * (g.cover * 0.5 + film), a, 1.0);
    return vec4<f32>(c, a2);
}


// The rain on a vehicle (Enhanced, `shade_enhanced`): the drops a waxed paint beads the
// water into - the glass's own (`glass_bead`), laid out the same way on the body
// (`pane_proj`), with fewer layers than a pane's: the paint's film is smooth already, the
// drops only break its mirror into points of light. (No runners: with `glass_runners` as
// well the lean pipelines' code brought Apple's M4 shader compiler service down.) On an outside mirror the
// same drops bend the picture it shows (`mirror_bend`). `wet` 0..1; `dw` and `dl` the
// world's and the vehicle's positions' screen derivatives in x and y, taken where every
// pixel of the draw takes them (in a branch of the pixel's own they brought Apple's M4
// shader compiler service down, as did a second copy of this code in the same pipeline).
struct VehicleDrops {
    // the water's normal (world) and the surface's, on the eye's side
    n: vec3<f32>,
    out: vec3<f32>,
    // how much of the pixel a drop covers, and how far out on it the pixel lies
    cover: f32,
    rim: f32,
};

fn vehicle_drops(world: vec3<f32>, local: vec3<f32>, n: vec3<f32>, wet: f32, t: f32, dw: mat2x3<f32>, dl: mat2x3<f32>) -> VehicleDrops {
    let b = pane_base_d(world, local, n, false, dw[0], dw[1], dl[0], dl[1]);
    var out = VehicleDrops(b.out, b.out, 0.0, 0.0);
    if (!b.ok || wet <= 0.01) {
        return out;
    }
    let an = abs(b.nl);
    let roof = an.z > max(an.x, an.y) * 2.0;
    let axis = select(select(1, 0, an.y > an.x), 2, roof);
    let p = pane_proj(b, axis);
    var acc: DropAcc;
    acc.cover = 0.0;
    acc.slope = vec2<f32>(0.0);
    acc.rim = 0.0;
    acc.size_px = 0.0;
    acc.track = 0.0;
    acc.avg = 0.0;
    // (the paint holds bigger beads than glass, fewer of them: a polished body's water
    // contracts into round drops)
    let room = 1.0;
    glass_bead(&acc, p, 1.0, 0.0125, 0.55 * smoothstep(0.03, 0.35, wet) * room, 0.0018, 1.0, t, b.px_area);
    glass_bead(&acc, p, 3.0, 0.026, 0.4 * smoothstep(0.25, 0.85, wet) * room, 0.0036, 1.0, t, b.px_area);
    let slope = acc.slope;
    let nz = sqrt(max(1.0 - dot(slope, slope), 0.02));
    out.n = normalize(b.out * nz + p.side_w * slope.x + p.down_w * slope.y);
    out.cover = clamp(acc.cover + acc.avg * 0.5, 0.0, 1.0);
    out.rim = acc.rim;
    return out;
}

// Where an outside mirror's picture is read under its drops (`vehicle_drops`): a drop's
// dome tilts the view through it, some 3 cm across the glass for a slope of one against the
// mirror's 20 cm or so - the picture inside it is the street turned over and drawn small.
// `duv` the picture's coordinates' screen derivatives in x and y, `dw` the world position's.
fn mirror_bend(uv: vec2<f32>, duv: mat2x2<f32>, dw: mat2x3<f32>, d: VehicleDrops) -> vec2<f32> {
    let det = duv[0].x * duv[1].y - duv[0].y * duv[1].x;
    if (d.cover <= 0.0 || abs(det) < 1e-14) {
        return uv;
    }
    let dw_du = (dw[0] * duv[1].y - dw[1] * duv[0].y) / det;
    let dw_dv = (dw[1] * duv[0].x - dw[0] * duv[1].x) / det;
    let tilt = (d.n - d.out) * 0.03;
    let shift = vec2<f32>(dot(tilt, dw_du) / max(dot(dw_du, dw_du), 1e-12), dot(tilt, dw_dv) / max(dot(dw_dv, dw_dv), 1e-12));
    return uv - shift * d.cover;
}
