struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @builtin(instance_index) inst: u32,
};
struct VsOut {
    // invariant: the depth prepass and the main pass must compute the same depth to the
    // last bit, the main pass tests against the depth the prepass left
    @builtin(position) @invariant clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) params: vec4<f32>,
    @location(4) params2: vec4<f32>,
    // the D3D material's highlight, lit at the vertex as Omsi.exe's fixed function lights
    // it: from the sun (light A) and from the light above (light B)
    @location(5) spec_sun: vec3<f32>,
    @location(6) spec_sky: vec3<f32>,
    // where a [matl_envmap] reads its sphere map, made at the vertex (see `sphere_map_uv`)
    @location(7) env_uv: vec2<f32>,
    @location(8) wipe_uv: vec3<f32>,
    // the classic picture's headlight, lit at the vertex as Omsi.exe's fixed function lights
    // it (see `classic_spot`): its diffuse and its ambient light
    @location(9) spot_diffuse: vec3<f32>,
    @location(10) spot_ambient: vec3<f32>,
};
// What the fragment shaders take: VsOut without the invariant on the position. The
// invariant belongs to the vertex output; on a fragment input naga's GLSL writer turns it
// into `invariant gl_FragCoord`, which desktop GL drivers and GLES reject.
struct FsIn {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) params: vec4<f32>,
    @location(4) params2: vec4<f32>,
    @location(5) spec_sun: vec3<f32>,
    @location(6) spec_sky: vec3<f32>,
    @location(7) env_uv: vec2<f32>,
    @location(8) wipe_uv: vec3<f32>,
    @location(9) spot_diffuse: vec3<f32>,
    @location(10) spot_ambient: vec3<f32>,
};

// Direct3D's D3DTSS_TCI_SPHEREMAP, by which Omsi.exe's environment stage reads the sphere
// map of a [matl_envmap] (0x7ff670): at the vertex, from the reflection R in camera space
// (x right, y up, z ahead), u = Rx/m + 0.5 and v = Ry/m + 0.5 with m = 2|R - (0, 0, 1)| -
// as Windows draws it (wine's d3d9 test_generated_texcoords, measured there). What a pane
// facing the camera mirrors is the map's middle, at half the scale of the u = Rx/2 + 0.5
// taken before (which showed the whole street of the picture across a bus's flank); the
// map's rim is the reflection running on away from the camera, at a grazing angle, where
// the picture swings round between two vertices - the seam the original shows there.
// `eye` is the camera's position, or its mirror image under the road for the player
// vehicle seen in a puddle; the camera's own axes lay the map out either way.
// (The headset: laid out by the bus's heading, level, not by each eye's view.)
fn sphere_map_uv(world: vec3<f32>, n: vec3<f32>, eye: vec3<f32>) -> vec2<f32> {
    let r = reflect(normalize(world - eye), n);
    let vr_env = camera.cam_up.w > 0.5;
    let right = select(camera.cam_right.xyz, vec3<f32>(cos(camera.cam_right.w), -sin(camera.cam_right.w), 0.0), vr_env);
    let up = select(camera.cam_up.xyz, vec3<f32>(0.0, 0.0, 1.0), vr_env);
    let rc = vec3<f32>(dot(r, right), dot(r, up), dot(r, cross(up, right)));
    let m = 2.0 * length(rc - vec3<f32>(0.0, 0.0, 1.0));
    return rc.xy / max(m, 1e-6) + 0.5;
}

// Direct3D's specular term at a vertex (Omsi.exe switches it on in FormActivate, 0x8254e0):
// Omsi.exe's sun (light 0, 0x7089f0: directional, specular = light A) and the light from
// straight above (light 1: specular = light B), each (N.H)^power with the local viewer's
// half vector, only where the light falls on the face, times the material's specular colour
// and clamped at 1 - then interpolated across the face. Computed per pixel instead, every
// small flat part (a gear selector, a switch, a dashboard screen) caught a sharp spot of the
// sun in its middle that the original, lighting only at the corners, never shows.
fn vertex_specular(wp: vec3<f32>, n: vec3<f32>) -> array<vec3<f32>, 2> {
    var out = array<vec3<f32>, 2>(vec3<f32>(0.0), vec3<f32>(0.0));
    if (material.specular.w <= 0.0 || material.params.y >= 0.5) {
        return out;
    }
    let v = normalize(camera.cam_pos.xyz - wp);
    let p = material.specular.w;
    let l = camera.sun_dir.xyz;
    if (dot(n, l) > 0.0) {
        out[0] = camera.sun_color.rgb * camera.sun_dir.w * pow(max(dot(n, normalize(v + l)), 0.0), p);
    }
    let up = vec3<f32>(0.0, 0.0, 1.0);
    if (n.z > 0.0) {
        out[1] = camera.sky_color.rgb * pow(max(dot(n, normalize(v + up)), 0.0), p);
    }
    let total = out[0] + out[1];
    let k = min(vec3<f32>(1.0), total) / max(total, vec3<f32>(1e-6));
    out[0] = min(vec3<f32>(1.0), out[0] * k * material.specular.rgb);
    out[1] = min(vec3<f32>(1.0), out[1] * k * material.specular.rgb);
    return out;
}

// Windy trees: how far the wind moves a vertex of a tree's foliage (world, m). `local` is
// the vertex in the mesh, `m` its model matrix, whose origin is the tree's foot.
//
// The tree is bent, not shifted: nothing under its pivot (`material.sway.y`, where the crown
// leaves the trunk) moves, and above it the crown bends over as a cantilever does, by the
// square of the height above the pivot, sinking a little so that its branches keep their
// length. The crown moves as one heavy body - a tree is a lightly damped oscillator, and
// what it does is smooth: no part of the picture moves on its own (each corner of a card
// wobbling in its own phase made the crown jelly).
//
// What bends it is the drag of the air: it grows with the wind speed to the power 1.5 rather
// than 2, as a crown streamlines itself (Vogel's reconfiguration exponent), from a few
// millimetres at 1 m/s to some 3.5 % of the tree's height at 20 m/s, a storm. The gusts are
// a frozen pattern of faster and slower air carried along at the mean wind (Taylor): three
// smooth waves 54 .. 149 m long, a turbulence intensity of 0.3 as over a town, so that
// neighbouring trees bow together as a gust passes and the ones further on follow - and,
// being smooth, the gusts never stop the tree dead as the corners of a value noise did.
// Around that lean the tree swings at its own natural frequency (0.2 .. 1 Hz, lower the
// taller it is), its amplitude and phase wandering slowly as a resonance driven by
// turbulence does, and a little across the wind; the crown's top lags its middle a little.
// With no wind nothing moves.
fn tree_sway(local: vec3<f32>, m: mat4x4<f32>) -> vec3<f32> {
    let sw = material.sway;
    let wind = camera.tree_wind.xy;
    let speed = length(wind);
    if (sw.x < 0.5 || speed < 0.05) {
        return vec3<f32>(0.0);
    }
    let h = clamp((local.z - sw.y) / max(sw.z - sw.y, 1e-4), 0.0, 1.0);
    if (h <= 0.0) {
        return vec3<f32>(0.0);
    }
    let scale = length(m[2].xyz);
    let crown = max((sw.z - sw.y) * scale, 0.1);
    let height = max(sw.z * scale, crown);
    let foot = (m * vec4<f32>(0.0, 0.0, 0.0, 1.0)).xyz;
    let foot_map = world_pattern_xy(foot);
    let t = camera.post.y;
    // each tree its own phase
    let seed = hash_cell(floor(foot_map * 2.0), 2000.0);
    let phase = seed * 6.2831853;
    // the gusts at the tree: whole waves per PATTERN_PERIOD (the map position and the
    // drift arrive modulo it), 149, 93 and 54 m long - a gust lasts some seconds; their sum has a standard deviation of
    // 0.51 (0.6, 0.35, 0.2 over sqrt 2), scaled to the turbulence intensity 0.3
    let gp = (foot_map - camera.tree_wind.zw) * (6.2831853 / PATTERN_PERIOD);
    let waves = 0.6 * sin(dot(gp, vec2<f32>(6.0, 3.0)) + 1.3)
        + 0.35 * sin(dot(gp, vec2<f32>(-4.0, 10.0)) + 4.1)
        + 0.2 * sin(dot(gp, vec2<f32>(17.0, -7.0)) + 2.2);
    let gust = max(1.0 + 0.59 * waves, 0.0);
    let along = wind / speed;
    let across = vec2<f32>(-along.y, along.x);
    // the lean under the gusting wind, and the tree's swing about it
    let give = 0.035 * height * sw.w;
    let lean = give * pow(speed * gust / 20.0, 1.5);
    // (in strong wind the swing is of the order of the mean lean itself)
    let swing_size = 0.7 * give * pow(speed / 20.0, 1.5);
    let f0 = clamp(1.6 / sqrt(height), 0.2, 1.0);
    let w0 = 6.2831853 * f0 * t + phase + 0.8 * sin(0.17 * t + 9.0 * seed);
    let envelope = 0.65 + 0.35 * sin(0.23 * t + 5.0 * seed);
    let swing = swing_size * envelope;
    // (the top lags the middle by a fraction of a cycle: the crown bends, not only leans)
    let lag = 0.35 * h;
    let move_xy = along * (lean + swing * sin(w0 - lag)) + across * (0.35 * swing * sin(0.93 * w0 + 1.7 - lag));
    var d = vec3<f32>(move_xy * h * h, 0.0);
    // the crown's big boughs: lobes about the crown's size swaying at about twice the tree's
    // frequency, each with its own slowly wandering phase, harder in a gust - neighbouring
    // parts of the picture move nearly together, so the crown stirs rather than wobbles
    let lm = (m * vec4<f32>(local, 0.0)).xyz;
    let bp = dot(lm, vec3<f32>(0.45, 0.35, 0.3) * (4.0 / crown)) + phase;
    let fb = clamp(2.2 * f0, 0.8, 2.0);
    let wb = 6.2831853 * fb * t + bp + 0.6 * sin(0.31 * t + bp);
    let bough = 0.25 * give * pow(speed * gust / 20.0, 1.5) * h;
    d = d + vec3<f32>(along * sin(wb) + across * 0.5 * sin(1.17 * wb + 2.0), 0.25 * sin(0.9 * wb + 1.0)) * bough;
    // the bent crown keeps its length: it sinks as it leans
    let above = max((local.z - sw.y) * scale, 0.3);
    d.z = d.z - 0.5 * dot(d.xy, d.xy) / above;
    return d;
}

// Where a scene vertex lands on the screen: the main pass's and the depth passes' vertex
// shaders (`vs_main`, `vs_depth`) both take it from here, so that their depths agree to the
// last bit (the main pass tests against the depth the prepass left).
fn scene_clip(e: u32, wp: vec4<f32>, nw: vec3<f32>) -> vec4<f32> {
    // Road surfaces (splines, the objects lying on them, a shadow blob) are pulled towards
    // the eye along the line of sight - the picture does not move, only the depth - so that
    // they win over the flush ground beside them. Code 0.9 keeps the surface shading without
    // the pull; painted ground (0.75) stays put.
    let surf = inst_params[e * 2u + 1u].w;
    var cp = wp.xyz;
    if (surf > 0.9) {
        let to = wp.xyz - camera.cam_pos.xyz;
        let d = length(to);
        // The same share of the way to the eye at every vertex (0.3 %, a few millimetres of
        // height seen from the cab): the pulled road is still the plane it was, so a spline
        // laid a millimetre over another - the rails or markings over a road - stays over it
        // however the two are cut into triangles. A fixed 2 cm more along each vertex's own
        // line of sight bowed the road's long triangles over the short ones of what lies on
        // it, by millimetres near the eye, and the rails went under the road there (#1196).
        // (surface objects, 1.25, a little more than the splines under them)
        // (a vehicle's shadow blob, 2: drawn without a depth test in the original, right
        // over the road it lies on - with the road's own pull alone the two fought for
        // every pixel and the road mostly won; a few centimetres more bring it through the
        // road and still leave it behind the body and the wheels standing on it)
        // The share stops growing at 5 cm. The pull is a depth, not a place - the picture
        // does not move - but the pole of a traffic light that a mod's map tagged as a
        // surface is pulled with the road under it, and a share still growing at distance
        // (a third of a metre at 90 m) carried the pole through the signal head it stands
        // beside. Capped, the pole stays behind what it stands by, and the depth a road
        // needs over flush ground is a few millimetres at most (a floor raised 3 cm above
        // the road is still seen - `surface_bias_keeps_raised_floors_visible_at_a_distance`).
        // (a road's painted lines, 1.4, a little more again: a crossing object laid over
        // the road covered the lines it overlaps, which in Omsi.exe lie a millimetre over
        // the road and win the depth test against it - #1694)
        let decal = surf > 1.1 && surf < 1.35;
        let paint = surf > 1.35 && surf < 1.5;
        let share = select(select(select(0.003, 0.004, decal), 0.0045, paint), 0.005, surf > 1.5);
        let fixed = select(select(select(0.0, 0.01, decal), 0.015, paint), 0.05, surf > 1.5);
        // The cap on the share is for upright faces, the poles. A flat surface keeps its
        // share all the way: pulled by the same part of the way to the eye at every vertex
        // the road is still one plane, however long its triangles, and a marking on it keeps
        // its place. Capped at 5 cm the pull of a triangle from the eye to the horizon
        // stopped growing with the distance and turned the road's plane into a bowl: at
        // 40 m from a low eye the road came in front of the lines on it, and they were
        // gone, and back as the bus came on (#3085). (Half a metre of it, at 170 m and
        // beyond: millimetres of height from a driver's eye.)
        let flat = abs(safe_normal(nw).z) > 0.5;
        let pull = min(min(share * d, select(0.05, 0.5, flat)) + fixed, d * 0.3);
        cp = wp.xyz - to / max(d, 1e-3) * pull;
    }
    // Omsi.exe draws a model mesh by mesh and subset by subset with LESSEQUAL into a 24-bit
    // depth buffer: of two of its surfaces in one plane - a dashboard and the instruments
    // printed on it - the later one is seen. Here their depths differed by rounding alone
    // and every pixel was a coin toss. A slot lying on an earlier surface of its object
    // (`draw_order_level`, 1 to 3; everything else 0) is brought nearer by two steps, each
    // the same over a whole plane however it is cut into triangles (a step worked out per
    // vertex from its own distance bends the surface): the reversed depth times 1 + 1e-5 a
    // level, above the vertices' rounding; and 2e-5 a level over the plane's distance from
    // the eye, against the tilt the rasterizer's snapping of each corner to a fraction of a
    // pixel gives a triangle's depth, the more the flatter it is seen. Seen from above the
    // plane that lifts it by 3e-5 a level times the distance: 0.9 mm at 10 m at level 3.
    // (The plane's distance is at least 5 % of the vertex's own: the edge of a curved mesh,
    // whose tangent plane runs through the eye, is not pulled metres forward.)
    let clip = camera.view_proj * vec4<f32>(cp, 1.0);
    let level = models[e * 4u].w;
    if (level <= 0.0) {
        return clip;
    }
    // (the depth of the eye's own point: n f / (f - n), about the near plane)
    let at_eye = (camera.view_proj * vec4<f32>(camera.cam_pos.xyz, 1.0)).z;
    let plane = max(abs(dot(safe_normal(nw), cp - camera.cam_pos.xyz)), 0.05 * length(cp - camera.cam_pos.xyz));
    let slant = level * 2e-5 * at_eye * clip.w / plane;
    return vec4<f32>(clip.x, clip.y, clip.z * (1.0 + level * 1e-5) + slant, clip.w);
}

@vertex
fn vs_main(in: VsIn) -> VsOut {
    let e = draw_list[in.inst];
    let m = model_matrix(e);
    let wp = m * vec4<f32>(in.pos, 1.0) + vec4<f32>(tree_sway(in.pos, m), 0.0);
    var out: VsOut;
    out.clip = scene_clip(e, wp, (m * vec4<f32>(in.normal, 0.0)).xyz);
    out.world = wp.xyz;
    out.normal = safe_normal((m * vec4<f32>(in.normal, 0.0)).xyz);
    let sp = vertex_specular(wp.xyz, out.normal);
    out.spec_sun = sp[0];
    out.spec_sky = sp[1];
    let spot = classic_spot(wp.xyz, out.normal);
    out.spot_diffuse = spot[0];
    out.spot_ambient = spot[1];
    out.env_uv = sphere_map_uv(wp.xyz, out.normal, camera.cam_pos.xyz);
    let pr = inst_params[e * 2u];
    out.uv = in.uv + pr.zw;
    out.params = pr;
    out.params2 = inst_params[e * 2u + 1u];
    let side_pane = material.wipe_bounds.z < 0.0;
    let pane = select(in.pos.xz, in.pos.yz, side_pane);
    out.wipe_uv = vec3<f32>((pane - material.wipe_bounds.xy) * abs(material.wipe_bounds.zw), select(in.pos.y, in.pos.x, side_pane));
    if (pr.y < 0.5) {
        // invisible: collapse the triangle
        out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    }
    return out;
}

// What the depth-only passes (the depth prepass, the multisampled prefill) take from a
// vertex: its place and what a cut-out or a transmap needs to decide its coverage. The
// highlights, the sphere map and the normal of `vs_main` are not worked out for them.
struct DepthOut {
    @builtin(position) @invariant clip: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) params: vec4<f32>,
    // the instance's surface code (`inst_params` w, see `scene_clip`)
    @location(4) surf: f32,
};
struct DepthIn {
    @builtin(position) clip: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) params: vec4<f32>,
    @location(4) surf: f32,
};

@vertex
fn vs_depth(in: VsIn) -> DepthOut {
    let e = draw_list[in.inst];
    let m = model_matrix(e);
    let wp = m * vec4<f32>(in.pos, 1.0) + vec4<f32>(tree_sway(in.pos, m), 0.0);
    var out: DepthOut;
    out.clip = scene_clip(e, wp, (m * vec4<f32>(in.normal, 0.0)).xyz);
    let pr = inst_params[e * 2u];
    out.uv = in.uv + pr.zw;
    out.params = pr;
    out.surf = inst_params[e * 2u + 1u].w;
    if (pr.y < 0.5) {
        out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    }
    return out;
}

struct VehicleBox { a: vec4<f32>, b: vec4<f32>, c: vec4<f32> };
struct VehicleReflection { plane: vec4<f32>, parts: array<VehicleBox, 4> };
@group(2) @binding(0) var<uniform> vehicle_reflection: VehicleReflection;

@vertex
fn vs_puddle_vehicle(in: VsIn) -> VsOut {
    let e = draw_list[in.inst];
    let m = model_matrix(e);
    var out: VsOut;
    out.world = (m * vec4<f32>(in.pos, 1.0)).xyz;
    out.normal = safe_normal((m * vec4<f32>(in.normal, 0.0)).xyz);
    let pr = inst_params[e * 2u];
    out.uv = in.uv + pr.zw;
    out.params = pr;
    out.params2 = inst_params[e * 2u + 1u];
    out.spec_sun = vec3<f32>(0.0);
    out.spec_sky = vec3<f32>(0.0);
    out.spot_diffuse = vec3<f32>(0.0);
    out.spot_ambient = vec3<f32>(0.0);
    let plane = vehicle_reflection.plane;
    // the sphere map from the reflected eye fs_puddle_vehicle shades the vehicle from
    out.env_uv = sphere_map_uv(out.world, out.normal, camera.cam_pos.xyz - 2.0 * plane.xyz * (dot(plane.xyz, camera.cam_pos.xyz) - plane.w));
    let reflected = out.world - 2.0 * plane.xyz * (dot(plane.xyz, out.world) - plane.w);
    out.clip = camera.view_proj * vec4<f32>(reflected, 1.0);
    if (out.params.y < 0.5) { out.clip = vec4<f32>(0.0, 0.0, 2.0, 1.0); }
    return out;
}

// Close an open legacy underbody with one depth-tested face in the same reflected
// view. Wheels and authored panels occlude it normally; no screen-space bands.
@vertex
fn vs_puddle_chassis(@builtin(vertex_index) i: u32, @builtin(instance_index) part: u32) -> @builtin(position) vec4<f32> {
    let corners = array<vec2<f32>, 6>(vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));
    let box = vehicle_reflection.parts[part];
    let local = corners[i] * max(box.b.yz - vec2<f32>(0.12, 0.18), vec2<f32>(0.0)) + box.c.xy;
    let sh = box.a.w;
    let ch = box.b.x;
    let xy = box.a.xy + vec2<f32>(local.x * ch + local.y * sh, -local.x * sh + local.y * ch);
    let plane = vehicle_reflection.plane;
    let road_z = (plane.w - dot(plane.xy, xy)) / plane.z;
    let z = max(road_z + 0.18, box.a.z + box.c.z - box.b.w + 0.04);
    let world = vec3<f32>(xy, z);
    let reflected = world - 2.0 * plane.xyz * (dot(plane.xyz, world) - plane.w);
    return camera.view_proj * vec4<f32>(reflected, 1.0);
}

