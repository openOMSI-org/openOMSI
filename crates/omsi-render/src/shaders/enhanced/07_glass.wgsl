
// The bus's windows in the enhanced picture: the rain on their outer face, the snow that
// settles there and the condensation on their inner face. (The drops and the snow are the classic pictures' too: scene/08_rain_glass.wgsl.)

// The pane at a pixel, in the vehicle's own frame (the film's vertices as its mesh has them,
// `FsIn.local`): the drops are laid out there, so they stay on the glass however the body
// rolls and pitches and on a door that swings open. The modellers' texture coordinates of
// a film are no measure of the glass: on the windscreen of the NL202 they run half as fast
// again one way as the other and at sixty degrees to each other, from triangle to triangle
// anew, and every drop drawn in them came out a slanting streak. So a pane is laid flat by
// the axes of the vehicle (`pane_proj`): the windscreen and the rear window onto its
// across and up, the side windows onto its along and up, a roof light onto its plan. A
// raked or curved pane is foreshortened in that projection; a drop is drawn round all the
// same, its offsets taken back onto the glass (`m`) before its shape is worked out. The
// drops' grid alone stays in the projection: on a pane at an angle to it a cell covers
// more glass, and the drops are a little fewer there.

// (The drops on a pane, the runners, the blur's picture and the pane's own frame -
// `glass_water`, `glass_blur` - are the classic pictures' too: scene/08_rain_glass.wgsl.)

// The rain film on a pane (the enhanced pass's `emissive.w` 2 layer), drawn over the
// clean picture of the frame: the drops, each a small lens that shows the street behind
// it upside down, a shade darker at its rim where the light is bent away, with the sky's
// glint on it, and between them the soft blur of the wet glass.
fn glass_film(in: FsIn, water: f32) -> vec4<f32> {
    let v = camera.cam_pos.xyz - in.world;
    let dist = length(v);
    let vn = v / max(dist, 1e-4);
    let in_cab = inside_vehicle(camera.cam_pos.xyz) * near_player_vehicle(in.world) > 0.5;
    let g = glass_water(in.world, in.local, in.normal, water, camera.post.y, in_cab);
    if (g.cover <= 0.001 && g.mist <= 0.001 && g.veil <= 0.001) {
        return vec4<f32>(0.0);
    }
    let pre = enh.exposure.x;
    let has_picture = camera.flags.z < -0.5;
    let dims = vec2<f32>(textureDimensions(t_env));
    let uv0 = in.clip.xy / dims;
    let mist_col = sh_irradiance(g.out) / PI * 0.9 * pre;
    // the view through the wet glass between the drops: softened by the film of water
    // (a running film scatters the light by a fraction of a degree: some 0.7 % of the
    // picture's height in a downpour, a third of that in a drizzle)
    let veil_lod = log2(max(dims.y * mix(0.002, 0.006, clamp(g.veil / 0.3, 0.0, 1.0)), 1.0));
    var behind = mist_col;
    if (has_picture) {
        behind = finite_or(glass_blur(uv0, veil_lod), mist_col);
    }
    // --- a drop: a small lens. Through it the street shows small and upside down (each
    // point of the drop looks out along its own bent way, `tilt` across the screen), the
    // more bent the further out on it; at its rim the light is bent so far that little of
    // it reaches the eye, a darker ring.
    var drop_col = behind;
    var a_drop = 0.0;
    if (g.cover > 0.001) {
        let tilt = g.n - g.out * dot(g.n, g.out);
        let right = camera.cam_right.xyz;
        let up = camera.cam_up.xyz;
        // (a drop is a wide lens: from its rim it looks out some 30 degrees aside, the sky
        // in the bottom of a drop against a wall)
        let o = vec2<f32>(dot(tilt, right), -dot(tilt, up)) * 0.3;
        var seen = behind;
        if (has_picture) {
            var uv = clamp(uv0 - o, vec2<f32>(0.001), vec2<f32>(0.999));
            // (only what lies beyond the glass: where the bent way lands on the cab's own
            // dashboard or pillar, in front of the pane, the drop shows what is straight
            // behind it instead)
            if (camera.clouds.w > 0.5 && camera.clouds.w < 1.5) {
                let asz = vec2<f32>(textureDimensions(t_ao));
                let z = textureLoad(t_ao, min(vec2<i32>(uv * asz), vec2<i32>(asz) - vec2<i32>(1)), 0).g;
                let zg = (camera.view_proj * vec4<f32>(in.world, 1.0)).w;
                let beyond = select(smoothstep(zg + 0.05, zg + 0.4, z), 1.0, z <= 0.0);
                uv = mix(uv0, uv, beyond);
            }
            seen = finite_or(glass_blur(uv, 0.8 + 1.5 * g.mist), behind);
        }
        let rim = smoothstep(0.55, 1.0, g.rim);
        let facing = dot(g.out, vn) > 0.0;
        let cosv = clamp(abs(dot(g.n, vn)), 0.0, 1.0);
        let f = select(0.02, 0.02 + 0.98 * pow(1.0 - cosv, 5.0), facing);
        let mirrored = rain_env_enhanced(reflect(-vn, g.n), 1.0) * pre;
        var c = seen * (1.0 - 0.45 * rim) * (1.0 - f) + mirrored * f;
        // the sun: a pinpoint where the dome mirrors it
        if (facing) {
            let r = reflect(-vn, g.n);
            c = c + enh.sun.rgb / PI * pre * pow(max(dot(r, camera.sun_dir.xyz), 0.0), 900.0) * f * 60.0;
        }
        // A drop of a pixel or less is no lens one can see: its many ways average to the
        // view behind it, a little dimmer for its rim - a faint grain, not a dark speck.
        let detail = smoothstep(0.5, 1.6, g.size_px);
        drop_col = mix(behind * 0.93, c, detail);
        a_drop = g.cover * mix(select(0.15, 0.5, has_picture), 0.95, detail);
    }
    // Drops a few pixels across are a pane's sparkle close up; seen from the street they
    // fade out over the first fifteen metres. The haze stays further.
    let aer = air(-vn, fog_distance(in.world), camera.cam_pos.z - enh.fog.z, in.world.z - enh.fog.z);
    a_drop = a_drop * (1.0 - smoothstep(5.0, 15.0, dist));
    // the fine mist of droplets: a faint grey scatter of the light about
    let a_mist = g.mist * 0.5;
    var a_veil = 0.0;
    if (g.veil > 0.001 && has_picture) {
        a_veil = g.veil * (1.0 - smoothstep(25.0, 60.0, dist));
    }
    let a_under = a_veil + a_mist * (1.0 - a_veil);
    let under = (behind * a_veil + mix(mist_col, behind, 0.9) * a_mist * (1.0 - a_veil)) / max(a_under, 1e-4);
    var a = a_drop + a_under * (1.0 - a_drop);
    if (a <= 1e-4) {
        return vec4<f32>(0.0);
    }
    let col = (drop_col * aer.a * a_drop + under * a_under * (1.0 - a_drop)) / a;
    // From inside, the rain is seen through the condensation on the glass's inner face
    // (drawn with the pane before this film): a misted pane shows the drops behind it as
    // soft shadows, not sharp lenses.
    if (in_cab) {
        a = a * (1.0 - 0.5 * glass_condensation(in.world, g.out));
    }
    return vec4<f32>(col, a);
}

// The snow a wiped pane holds while it snows (`is_snow_pane`; the mask's cover: 0 clear
// glass, up to 1 a pane closed by snow, above 1 the bank a blade pushed to the end of its
// sweep, below 0 glass a blade has just cleared). Crystals land one by one and stick, hold
// a while and melt into a little water - soon on the windscreen, whose glass the warm air
// of the defroster heats, slowly on a side window. As the cover grows they close into a
// milky layer, and it lies deepest along the windscreen's bottom edge and in its lower
// corners, where the blades and the airstream do not reach.
fn glass_snow(in: FsIn, cover: f32) -> vec4<f32> {
    // (the windscreen's defroster from the cabin's air, the height on the glass in the bus's
    // own frame with the body's tilt: see `glass_snow_with`, which the classic pictures
    // share)
    let l = pane_local(in.world);
    let fz = clamp(l.z / max(camera.inside_b.w, 0.1), -1.0, 1.0);
    return glass_snow_with(in, cover, enh.condensation.w, fz);
}

// A direction in the frame of the bus the camera is in (`pane_local` without the shift).
fn pane_dir(d: vec3<f32>) -> vec3<f32> {
    let sh = camera.inside_a.w;
    let ch = camera.inside_b.x;
    let v = vec3<f32>(d.x * ch - d.y * sh, d.x * sh + d.y * ch, d.z);
    let t = enh.pane_tilt;
    let p = vec3<f32>(v.x, v.y * t.y + v.z * t.x, v.z * t.y - v.y * t.x);
    return vec3<f32>(p.x * t.w - p.z * t.z, p.y, p.z * t.w + p.x * t.z);
}

// How close the edge of the pane at `world` (normal `n`) is: 1 against it, falling to 0
// some 25 cm in; -1 where it cannot be told. A pane's edge is where its frame begins - the
// rubber, the pillar, the trim, the dashboard under the windscreen: opaque and in the depth
// prepass, lying on the glass's own plane, where the glass is not. So points on the pane's
// plane round the pixel, in eight ways out to 26 cm, are looked up in the prepass's depth
// (the ambient occlusion's texture keeps it): where an opaque surface lies there on the
// plane - within a centimetre or two of it, not a wiper's arm a hand's breadth outside - the
// pane has ended. The nearest such end in each way, averaged over the ways: in a corner
// two ways find it close, along an edge one, in the middle of a pane none.
fn glass_frame_near(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if (camera.clouds.w < 0.5 || camera.clouds.w > 1.5) {
        return -1.0;
    }
    let size = vec2<f32>(textureDimensions(t_ao));
    let nn = safe_normal(n);
    let up0 = vec3<f32>(0.0, 0.0, 1.0) - nn * nn.z;
    let a = select(vec3<f32>(1.0, 0.0, 0.0), normalize(up0), dot(up0, up0) > 0.04);
    let b = cross(nn, a);
    var sum = 0.0;
    for (var i = 0; i < 8; i = i + 1) {
        let ang = f32(i) * 0.7853982 + 0.2;
        let dir = a * cos(ang) + b * sin(ang);
        var near = 0.0;
        for (var k = 0; k < 6; k = k + 1) {
            let dist = 0.03 + 0.046 * f32(k);
            let c = camera.view_proj * vec4<f32>(world + dir * dist, 1.0);
            if (c.w <= 0.05) {
                break;
            }
            let uv = vec2<f32>(c.x / c.w * 0.5 + 0.5, 0.5 - c.y / c.w * 0.5);
            if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
                break;
            }
            let z = textureLoad(t_ao, min(vec2<i32>(uv * size), vec2<i32>(size) - vec2<i32>(1)), 0).g;
            if (z > 0.0 && abs(z - c.w) < 0.015 + 0.006 * c.w) {
                near = 1.0 - (dist - 0.03) / 0.26;
                break;
            }
        }
        sum = sum + near;
    }
    return clamp(sum / 3.0, 0.0, 1.0);
}

// How much of the light crossing the player's bus's pane at `world` (normal `n`) its
// condensation scatters (0..1). One cabin air mists all the glass much alike: each
// window's film (enh.condensation: the optical depths of the windscreen's, the side
// windows' and the rear window's, omsi-weather condensation.rs) is taken two thirds from
// their mean. The film starts where the glass is coldest and the air stillest, in a pane's
// corners and along its frame (and low down), and grows inward as it thickens, its front
// wandering with the glass's own patchiness, until it closes over the pane. A heavy film
// gathers into drops that trickle down and leave clear tracks; on the windscreen the
// defroster's warm air clears it from the bottom up.
fn glass_condensation(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if (camera.inside_c.w < 0.5) {
        return 0.0;
    }
    let l = pane_local(world);
    let h = camera.inside_b.yzw;
    if (any(abs(l) >= h + vec3<f32>(0.25))) {
        return 0.0;
    }
    let nl = safe_normal(pane_dir(n));
    let gap = h - abs(l);
    // (the panes in the body's skin only, by the way they face: not the instruments' glass
    // behind the windscreen or a partition in the saloon)
    var own = -1.0;
    var front = false;
    if (abs(nl.y) > 0.55 && gap.y < 0.45) {
        front = l.y > 0.0;
        own = select(enh.condensation.z, enh.condensation.x, front);
    } else if (abs(nl.x) > 0.5 && gap.x < 0.35) {
        own = enh.condensation.y;
    }
    if (own < 0.0) {
        return 0.0;
    }
    let depth = mix((enh.condensation.x + enh.condensation.y + enh.condensation.z) / 3.0, own, 0.35);
    if (depth <= 0.001) {
        return 0.0;
    }
    // the pane's own frame in metres: across, and up the glass
    let up0 = vec3<f32>(0.0, 0.0, 1.0) - nl * nl.z;
    let up = select(vec3<f32>(1.0, 0.0, 0.0), normalize(up0), dot(up0, up0) > 0.04);
    let s = vec2<f32>(dot(l, cross(up, nl)), dot(l, up));
    let fz = clamp(l.z / max(h.z, 0.1), -1.0, 1.0);
    let low = 1.0 - clamp(fz * 0.5 + 0.5, 0.0, 1.0);
    let edge_seen = glass_frame_near(world, n);
    let edge = select(edge_seen, 0.15 + 0.3 * low, edge_seen < 0.0);
    let n1 = snow_noise(s * 3.0 + vec2<f32>(11.0, 3.0));
    let n2 = snow_noise(s * 9.0 + vec2<f32>(3.0, 17.0));
    let patchy = n1 * 0.7 + n2 * 0.3;
    // (the front: into the corners and along the frame at first, the glass's patchiness
    // breaking its line, over the whole pane at a depth of two; a soft front, the film
    // thinning out over a few centimetres)
    let grow = 1.0 - exp(-depth * 1.3);
    let potential = 0.45 * edge + 0.15 * low + 0.45 * patchy;
    let front_line = 1.0 - grow * 1.3;
    let covered = smoothstep(front_line - 0.18, front_line + 0.18, potential);
    var amount = covered * (1.0 - exp(-depth * (0.9 + 0.5 * patchy)));
    // the fine grain of the droplets themselves
    amount = amount * (0.92 + 0.08 * snow_noise(s * 140.0));
    if (depth > 1.4) {
        // trickles: drops grown heavy run down a few centimetres a minute, each clearing a
        // narrow track below where it started
        let lane = floor(s.x / 0.03);
        let h1 = rain_hash(vec2<f32>(lane, 71.0));
        if (h1.x < clamp((depth - 1.4) * 0.35, 0.0, 0.6)) {
            let h2 = rain_hash(vec2<f32>(lane, 13.0));
            let x0 = (lane + 0.3 + 0.4 * h2.x) * 0.03 + 0.003 * sin(s.y * 40.0 + h2.y * 6.283185);
            let period = 90.0 + 120.0 * h1.y;
            let tau = fract(camera.post.y / period + h2.y) * period;
            let start = h.z * (0.75 - 0.9 * h2.x);
            let end = start - min(tau * 0.004, 0.6);
            let fade = smoothstep(0.0, 5.0, tau) * (1.0 - smoothstep(period - 10.0, period, tau));
            let dx = abs(s.x - x0);
            let in_track = step(end, s.y) * step(s.y, start) * (1.0 - smoothstep(0.0012, 0.0025, dx));
            let head = 1.0 - smoothstep(0.0018, 0.0028, length(vec2<f32>(dx, s.y - end)));
            amount = amount * (1.0 - 0.8 * max(in_track, head) * fade);
        }
    }
    if (front && enh.condensation.w > 0.02) {
        // the defroster's warm air clears the windscreen from its bottom edge up, first in
        // the middle over its outlets
        let u = clamp((fz + 0.35) / 1.2, 0.0, 1.0);
        let fx = l.x / max(h.x, 0.1);
        let reach = enh.condensation.w / 0.85 * 1.15 - 0.2 * fx * fx + 0.06 * (n1 - 0.5);
        amount = amount * smoothstep(reach - 0.12, reach, u);
    }
    return clamp(amount, 0.0, 1.0);
}

// The reflection a pane adds over what shows through it (pre-exposed `reflection`): the
// street and the sky it mirrors, seen from outside or on another vehicle; none on the inner
// face of the player's own bus's glass. (A box-projected cube of the cabin was mirrored
// there; it lagged behind the camera, shook with the head and showed a blurred picture, and
// was taken out on the user's word, 2026-11-22.)
fn glass_reflection(world: vec3<f32>, n: vec3<f32>, v: vec3<f32>, reflection: vec3<f32>) -> vec3<f32> {
    let inside = inside_vehicle(camera.cam_pos.xyz) * near_player_vehicle(world);
    return reflection * (1.0 - step(0.5, inside));
}
