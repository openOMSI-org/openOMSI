struct Camera {
    view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    world_origin: vec4<f32>, // world-space origin removed from render coordinates
    sun_dir: vec4<f32>,      // xyz direction to the sun, w intensity
    ambient: vec4<f32>,      // rgb ambient (envir light C)
    fog: vec4<f32>,          // rgb, density
    sun_color: vec4<f32>,    // rgb (envir light A), w night factor 0..1 (nightmap strength)
    sky_color: vec4<f32>,    // rgb secondary light from above (envir light B), w 1 = vanilla (as OMSI 2), 0 = Vanilla+
    light_grid: vec4<f32>,   // x,y grid origin (render-origin relative), z cell size, w cells per side
    sky: vec4<f32>,
    cam_right: vec4<f32>,
    cam_up: vec4<f32>,
    clouds: vec4<f32>,
    light_view_proj: mat4x4<f32>,
    light_view_proj_far: mat4x4<f32>,
    shadow: vec4<f32>,       // x enabled, y texel size, z near cascade half range
    post: vec4<f32>,         // x enhanced, y time, zw sun ndc
    inside_a: vec4<f32>,     // player vehicle box: origin xyz, sin(heading)
    inside_b: vec4<f32>,     // cos(heading), half extents xyz
    inside_c: vec4<f32>,     // box centre offset xyz, w = 1 when there is a box, 2 with the eye in it
    flags: vec4<f32>,        // x detail texturing, y enhanced graphics, z never set (see fs_main's end), w close cascade half range
    light_view_proj_close: mat4x4<f32>,
    wind: vec4<f32>,         // the player's vehicle's velocity (m/s, world): the airstream on its glass
    // Enhanced: the lamps' shadow maps (the tiles under the far map; zero where nothing is
    // drawn) and where the tiles lie: x a tile's side, y tiles to a row, z the far map's
    // texture height, w the number of tiles (`LampAtlas::uniform`)
    lamp_view_proj: array<mat4x4<f32>, 56>,
    lamp_atlas: vec4<f32>,
    // Windy trees: xy the weather's wind (m/s, world; 0 with the setting off), zw how far
    // the air has carried the gusts since the start (m, modulo PATTERN_PERIOD)
    tree_wind: vec4<f32>,
    // an articulated bus's rear section (as inside_*): read by the snowfall only - a test of
    // it here, inlined into every use of inside_vehicle, brought Apple's M4 shader compiler
    // down (see 0.2.21's Metal note)
    inside2_a: vec4<f32>,
    inside2_b: vec4<f32>,
    inside2_c: vec4<f32>,
    // Enhanced: the field each light cookie layer covers (xy: the tangents of its half
    // angles across and up; z: its first record in the light buffer), see enhanced.wgsl
    // `cookie_light`
    cookies: array<vec4<f32>, 8>,
    // Enhanced: the lights of the player's saloon that have a shadow map (their places in the
    // light buffer, 4 a vector, -1 for none): see `cabin_visibility`
    cabin_lamps: array<vec4<f32>, 4>,
    // x: the textures' mip bias (FSR 1: negative, see `CameraUniform::lod`)
    lod: vec4<f32>,
};

// 1 when the point lies inside the player's vehicle (its [boundingbox], shrunk a little so
// that the outer skin, the glass and the roof stay outside): weather stays out of the cab.
// The side/front/back shrink was a bare 0.12 m, which left an interior fixture close to the
// wall (a window sill, the handbrake by the windscreen) just outside the box, wearing the
// up-facing wet/snow coat meant for the body panel beside it (see the `outside` weight
// below). A previous fix WIDENED this margin to 0.6 m, reasoning by analogy with the falling-
// particle exclusion box in `rain.rs` - but that box works the other way round (its margin
// is added, growing the excluded region outward, away from the body) from this one (whose
// margin is subtracted, shrinking the "inside" region inward, toward the body): widening it
// here shrank the cabin's own "inside" zone and threw the *whole* cabin outside it, coating
// the entire interior in snow instead of just the ledge. Narrowed to 0.03 m instead - just
// enough that the true outer skin and glass stay outside, without giving up real interior
// floor space near the wall.
// A normal of no length (a mesh saved with zero normals) is upright instead of NaN:
// normalize(0) is NaN on Vulkan and Metal and the pixel went black.
fn safe_normal(v: vec3<f32>) -> vec3<f32> {
    let d = dot(v, v);
    if (d > 1e-20) {
        return v * inverseSqrt(d);
    }
    return vec3<f32>(0.0, 0.0, 1.0);
}

fn inside_vehicle(world: vec3<f32>) -> f32 {
    if (camera.inside_c.w < 0.5) {
        return 0.0;
    }
    let d = world - camera.inside_a.xyz;
    let sh = camera.inside_a.w;
    let ch = camera.inside_b.x;
    let x = d.x * ch - d.y * sh - camera.inside_c.x;
    let y = d.x * sh + d.y * ch - camera.inside_c.y;
    let z = d.z - camera.inside_c.z;
    let h = camera.inside_b.yzw;
    let in_x = abs(x) < h.x - 0.03;
    let in_y = abs(y) < h.y - 0.03;
    let in_z = z > -h.z - 0.6 && z < h.z - 0.03;
    return select(0.0, 1.0, in_x && in_y && in_z);
}

// 1 when the point belongs to the player's vehicle: its `[boundingbox]` grown a little,
// so that the skin and the glass the cab test (`inside_vehicle`) leaves out are in.
fn near_player_vehicle(world: vec3<f32>) -> f32 {
    if (camera.inside_c.w < 0.5) {
        return 0.0;
    }
    let d = world - camera.inside_a.xyz;
    let sh = camera.inside_a.w;
    let ch = camera.inside_b.x;
    let x = d.x * ch - d.y * sh - camera.inside_c.x;
    let y = d.x * sh + d.y * ch - camera.inside_c.y;
    let z = d.z - camera.inside_c.z;
    let h = camera.inside_b.yzw + vec3<f32>(0.25);
    return select(0.0, 1.0, abs(x) < h.x && abs(y) < h.y && abs(z) < h.z);
}

// How much of the way from the camera to `world` lies outside the player's vehicle: there is
// no fog in the cab. From the driver's seat the whole saloon (seats 2 m away, the doors 5 m
// away) took the fog of that distance and in thick ground fog the interior went milky, the
// door panes nearly opaque - outside the bus there was nothing of the kind. The box is the
// one `inside_vehicle` uses; the part of the ray inside it is taken off the fogged distance.
fn fog_distance(world: vec3<f32>) -> f32 {
    let full = distance(world, camera.cam_pos.xyz);
    if (camera.inside_c.w < 0.5) {
        return full;
    }
    let sh = camera.inside_a.w;
    let ch = camera.inside_b.x;
    let to_local = fn_box_local(camera.cam_pos.xyz);
    let p1 = fn_box_local(world);
    let d = p1 - to_local;
    let h = camera.inside_b.yzw;
    // slab test of the segment against the box (x, y the plan, z height)
    var t0 = 0.0;
    var t1 = 1.0;
    for (var k = 0; k < 3; k = k + 1) {
        let o = to_local[k];
        let dk = d[k];
        let hk = h[k];
        if (abs(dk) < 1e-6) {
            if (abs(o) > hk) {
                return full;
            }
        } else {
            let a = (-hk - o) / dk;
            let b = (hk - o) / dk;
            t0 = max(t0, min(a, b));
            t1 = min(t1, max(a, b));
        }
    }
    if (t1 <= t0) {
        return full;
    }
    return max(full * (1.0 - (t1 - t0)), 0.0);
}

// A point in the player's vehicle box frame (centred, x across, y along, z up).
fn fn_box_local(world: vec3<f32>) -> vec3<f32> {
    let d = world - camera.inside_a.xyz;
    let sh = camera.inside_a.w;
    let ch = camera.inside_b.x;
    return vec3<f32>(d.x * ch - d.y * sh - camera.inside_c.x, d.x * sh + d.y * ch - camera.inside_c.y, d.z - camera.inside_c.z);
}

fn own_vehicle_blocks(p: vec3<f32>, l: vec3<f32>) -> bool {
    if (camera.inside_c.w < 1.5) {
        return false;
    }
    let a = fn_box_local(l);
    let d = fn_box_local(p) - a;
    let h = camera.inside_b.yzw - vec3<f32>(0.15);
    if (all(abs(a) < h)) {
        return false;
    }
    var t0 = 0.0;
    var t1 = 1.0;
    for (var k = 0; k < 3; k = k + 1) {
        let o = a[k];
        let dk = d[k];
        let hk = h[k];
        if (abs(dk) < 1e-6) {
            if (abs(o) >= hk) {
                return false;
            }
        } else {
            let lo = (-hk - o) / dk;
            let hi = (hk - o) / dk;
            t0 = max(t0, min(lo, hi));
            t1 = min(t1, max(lo, hi));
        }
    }
    return t1 > t0;
}

// 1 where the weather reaches the point. The cab test above reaches 0.6 m below the
// vehicle's box so that the floor stays dry, and that takes in the road under the bus as
// well: the snow on it vanished in a bus-shaped patch of bare asphalt (which read as the
// bus's shadow wiping the snow off). The ground - terrain, roads, painted ground, flagged
// by `terrain` or the instance's surface flag - is never inside a vehicle.
fn weather_outside(world: vec3<f32>, terrain: bool, surface: f32) -> f32 {
    if (terrain || surface > 0.5) {
        return 1.0;
    }
    return 1.0 - inside_vehicle(world);
}

// As `weather_outside`, knowing the surface's normal: the bus's own outer skin is outside
// even where it lies inside the box. A side wall leans in towards the roof (and bulges over
// the wheel arches), so the box's side plane cuts through the panel: above the cut the paint
// took the cab's light and stayed dry, below it the sky's - the wall looked half glossy,
// half matt, along a sharp line. A face within 0.35 m of a side, the front, the back or the
// roof of the box and turned out through it is skin, not cabin.
fn weather_outside_n(world: vec3<f32>, n: vec3<f32>, terrain: bool, surface: f32) -> f32 {
    if (terrain || surface > 0.5) {
        return 1.0;
    }
    // a vehicle's part (lib.rs `Instance::roof`): under its roof it is dry, whichever
    // vehicle it is - the one the camera is in, another player's, a timetable bus
    // (what faces up only: taken for every face below the roof, the whole outer skin of
    // every car and bus below the roof's edge was cab - lit by the dim cab light in the
    // enhanced picture, dry in the rain - with a hard seam round the body 0.3 m under the
    // roof where the sky's light began, #805)
    if (surface < -500.0) {
        let roof = -surface - 5000.0;
        if (world.z < roof - 0.3 && n.z > 0.5) {
            return 0.0;
        }
    }
    if (inside_vehicle(world) < 0.5) {
        return 1.0;
    }
    let d = world - camera.inside_a.xyz;
    let sh = camera.inside_a.w;
    let ch = camera.inside_b.x;
    let x = d.x * ch - d.y * sh - camera.inside_c.x;
    let y = d.x * sh + d.y * ch - camera.inside_c.y;
    let z = d.z - camera.inside_c.z;
    let nx = n.x * ch - n.y * sh;
    let ny = n.x * sh + n.y * ch;
    let h = camera.inside_b.yzw;
    let skin_x = h.x - abs(x) < 0.35 && nx * sign(x) > 0.5;
    let skin_y = h.y - abs(y) < 0.35 && ny * sign(y) > 0.5;
    let skin_z = h.z - z < 0.35 && n.z > 0.5;
    return select(0.0, 1.0, skin_x || skin_y || skin_z);
}
