//! Traffic lanes from splines and objects.
use super::*;

/// A path's `[rule] trafficdensity`s: how much random traffic of any group it carries,
/// and the last value per group (the rule's fourth line: the group's place in the map's
/// `unsched_vehgroups.txt`). Without a rule for the first group the path has its medium
/// density (1); the lane carries traffic as long as any group drives on it - on
/// Berlin-Spandau 462 Falkensee paths set only the GDR cars' density.
pub(super) fn path_densities(rules: &[omsi_map::MapRule], path: usize) -> (f32, Vec<(u16, f32)>) {
    let mut per: Vec<(u16, f32)> = Vec::new();
    for r in rules.iter().filter(|r| {
        r.path_index == path as i32 && r.kind.eq_ignore_ascii_case("trafficdensity") && !r.kill
    }) {
        let g = r.extra.max(0.0) as u16;
        let v = (r.value as f32).max(0.0);
        match per.iter_mut().find(|(k, _)| *k == g) {
            Some(e) => e.1 = v,
            None => per.push((g, v)),
        }
    }
    let first = per.iter().find(|(g, _)| *g == 0).map(|e| e.1).unwrap_or(1.0);
    let density = per.iter().map(|e| e.1).fold(first, f32::max);
    (density, per)
}

/// Lanes of one map spline: every `[path]` of the spline type runs along the curve at its
/// lateral offset; `direction` 1 runs backwards, 2 both ways (two lanes).
///
/// A spline placed with `mirror` has its cross-section turned over: each path lies on the
/// other side of the centre line and runs the other way, as its carriageway does (a right
/// lane that ran forward is a left lane running backward - traffic still keeps right).
/// Ignoring the flag put every lane of Spandau's 40-odd mirrored road pieces 5 to 20 m
/// beside its road and the wrong way round; a timetable route through one had its bus turn
/// into the oncoming lanes and jump back where the next piece began.
pub(super) fn spline_lanes(
    def: &Spline,
    s: &omsi_map::MapSpline,
    curve: &SplineCurve,
    tile: (i32, i32),
) -> Vec<Lane> {
    let mut out = Vec::new();
    let side = if s.mirror { -1.0 } else { 1.0 };
    let curve = &curve.with_sli(def);
    for (pi, p) in def.paths.iter().enumerate() {
        let n = ((curve.length / 3.0).ceil() as usize).clamp(1, 300);
        let pts: Vec<DVec3> = (0..=n)
            .map(|i| {
                curve.offset_point(
                    curve.length * i as f64 / n as f64,
                    side * p.start[0] as f64,
                    p.start[2] as f64,
                )
            })
            .collect();
        let kind = LaneKind::from_code(p.kind);
        let limit = s
            .rules
            .iter()
            .filter(|r| {
                r.path_index == pi as i32
                    && r.kind.eq_ignore_ascii_case("speedlimit")
                    && r.value > 0.0
            })
            .map(|r| r.value as f32)
            .last();
        // The other rules of this path: how much traffic the mapper wants here at all.
        // Berlin-Spandau alone carries 8516 [rule] trafficdensity and 68 no_cars, and with
        // them ignored cars appeared in pedestrian streets, depot yards and back lanes the
        // original keeps empty.
        let rule_of = |name: &str| {
            s.rules
                .iter()
                .filter(|r| {
                    r.path_index == pi as i32 && r.kind.eq_ignore_ascii_case(name) && !r.kill
                })
                .map(|r| r.value as f32)
                .last()
        };
        let (density, group_density) = path_densities(&s.rules, pi);
        let no_cars = s.rules.iter().any(|r| {
            r.path_index == pi as i32 && r.kind.eq_ignore_ascii_case("no_cars") && !r.kill
        });
        // (`bus` and `trucks` are switches that open the path to those AI vehicles, see
        // `Lane::allows`; `bus` does not close it to cars)
        let rule_bus = rule_of("bus").is_some();
        let rule_trucks = rule_of("trucks").is_some();
        let no_overtaking = rule_of("overtaking_prohib").is_some();
        let priority = rule_of("priority");
        let mut push = |pts: Vec<DVec3>, reversed: bool| {
            let mut l = LaneBuilder::polyline(pts, kind, p.width);
            if let Some(v) = priority {
                l.priority = v;
            }
            if let Some(v) = limit {
                l.speed_limit_kmh = v;
            }
            l.density = density;
            l.group_density = group_density.clone();
            l.no_cars = no_cars;
            l.rule_bus = rule_bus;
            l.rule_trucks = rule_trucks;
            l.no_overtaking = no_overtaking;
            l.source = 1;
            l.key = Some(LaneKey {
                tile,
                id: s.id,
                path: pi as u16,
            });
            l.reversed = reversed;
            l.offset = side as f32 * p.start[0];
            l.name = def
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            out.push(l);
        };
        // (a mirrored spline's forward path runs backwards and the other way round)
        match (p.direction, s.mirror) {
            (2, _) => {
                push(pts.clone(), false);
                push(pts.into_iter().rev().collect(), true);
            }
            (1, false) | (0, true) => push(pts.iter().rev().copied().collect(), true),
            _ => push(pts, false),
        }
    }
    out
}

/// The crossing light a placed child names (see `ScriptedObject::light_parent`): its
/// `[varparent]` is a crossing whose program runs and its first string a light index.
pub(super) fn light_child_of(crossings: &hashbrown::HashSet<i64>, var_parent: Option<i64>, strings: &[String]) -> Option<(i64, usize)> {
    let parent = var_parent.filter(|p| crossings.contains(p))?;
    if !omsi_world::map_index::names_traffic_light(strings) {
        return None;
    }
    let index = strings.first()?.trim().parse::<usize>().ok()?;
    Some((parent, index))
}

pub(super) fn traffic_light_program_enabled(sco: &SceneryObject, has_signals: bool) -> bool {
    !sco.traffic_lights.is_empty() && (has_signals || sco.is_traffic_light)
}

/// Lanes of one placed scenery object: `[path]` arcs in the object frame (x right,
/// y forward, z up; heading clockwise, radius > 0 right turn) rotated by the object heading.
pub(super) fn object_lanes(
    sco: &SceneryObject,
    pos: DVec3,
    rot: [f64; 3],
    controller: Option<usize>,
    tile: (i32, i32),
    id: i64,
    rules: &[omsi_map::MapRule],
) -> Vec<Lane> {
    let mut out = Vec::new();
    let heading = rot[0];
    let h = heading.to_radians();
    let (sh, ch) = (h.sin(), h.cos());
    // local (x east, y north) → world for an object turned clockwise by `heading`
    let to_world = |x: f64, y: f64| DVec2::new(x * ch + y * sh, -x * sh + y * ch);
    for (pi, p) in sco.paths.iter().enumerate() {
        let v = &p.params;
        if v.len() < 11 {
            continue;
        }
        let start_local = to_world(v[0] as f64, v[1] as f64);
        let start = DVec3::new(
            pos.x + start_local.x,
            pos.y + start_local.y,
            pos.z + v[2] as f64,
        );
        let (path_heading, radius, length) = (v[3] as f64 + heading, v[4] as f64, v[5] as f64);
        if length <= 0.01 {
            continue;
        }
        let (grad_start, grad_end) = (v[6] as f64, v[7] as f64);
        let kind = LaneKind::from_code(p.kind);
        let turn = match v.get(11).map(|t| *t as i32) {
            Some(2) => 1,
            Some(3) => 2,
            _ => 0,
        };
        // The `[rule]`s the map put on this object's path. Most of a map's rules sit on the
        // junctions, not on the splines - Berlin-Spandau has 5402 trafficdensity, 693
        // speedlimit, 576 trucks and 64 no_cars on objects against 3114/810/284/4 on splines
        // - so ignoring them left cars turning into every yard and pedestrian street.
        let rule_of = |name: &str| {
            rules
                .iter()
                .filter(|r| {
                    r.path_index == pi as i32 && r.kind.eq_ignore_ascii_case(name) && !r.kill
                })
                .map(|r| r.value as f32)
                .last()
        };
        let limit = rules
            .iter()
            .filter(|r| {
                r.path_index == pi as i32
                    && r.kind.eq_ignore_ascii_case("speedlimit")
                    && r.value > 0.0
                    && !r.kill
            })
            .map(|r| r.value as f32)
            .last();
        let (density, group_density) = path_densities(rules, pi);
        let no_cars = rules.iter().any(|r| {
            r.path_index == pi as i32
                && r.kind.eq_ignore_ascii_case("no_cars")
                && !r.kill
        });
        let rule_bus = rule_of("bus").is_some();
        let rule_trucks = rule_of("trucks").is_some();
        let no_overtaking = rule_of("overtaking_prohib").is_some();
        // who goes first where this path meets another (`Network::must_yield`)
        let priority = rule_of("priority");
        let mut push = |reverse: bool| {
            let mut l = LaneBuilder::arc_with_gradients(start, path_heading, length, radius, grad_start, grad_end, kind, p.width);
            if reverse {
                let pts: Vec<DVec3> = l.points.iter().rev().copied().collect();
                l = LaneBuilder::polyline(pts, kind, p.width);
            }
            if let Some(v) = limit {
                l.speed_limit_kmh = v;
            }
            l.density = density;
            l.group_density = group_density.clone();
            l.no_cars = no_cars;
            l.rule_bus = rule_bus;
            l.rule_trucks = rule_trucks;
            l.no_overtaking = no_overtaking;
            l.turn = turn;
            if let Some(v) = priority {
                l.priority = v;
            }
            l.source = 2;
            l.key = Some(LaneKey {
                tile,
                id,
                path: pi as u16,
            });
            l.blocks = sco.path_blocks.get(pi).map(|b| b.iter().filter(|(n, _)| *n >= 0).map(|(n, _)| *n as u16).collect()).unwrap_or_default();
            l.crossing_problem = sco.path_crossing_problem.get(pi).copied().unwrap_or(false);
            l.reversed = reverse;
            l.traffic_light = controller.and_then(|c| {
                sco.path_traffic_light
                    .get(pi)
                    .copied()
                    .filter(|t| *t >= 0)
                    .map(|t| (c, t as usize))
            });
            out.push(l);
        };
        match p.direction {
            1 => push(true),
            2 => {
                push(false);
                push(true);
            }
            _ => push(false),
        }
    }
    out
}

/// Resolve the texture name for a scenery object's `[matl_freetex]` slot.
/// Tries the object's script variable first, then freetex probe, and falls back to
/// tile placement strings (by explicit numeric index or by freetex declaration order).
///
/// The object's own string variable owns the slot: when its `[stringvarnamelist]` (or a
/// script) declares `var`, its value is the file name and an empty value means the slot
/// has none - the object's own texture stays, as in Omsi.exe. Reading the placement
/// strings of another variable instead put a bus stop sign's route number (`72`, `A47X`)
/// or a stop name in a `[matl_freetex]` slot, which Omsi.exe never does; those are the
/// `[texttexture]` sizes of the sign, not files (#1756).
pub fn resolve_scenery_freetex_name<'a>(
    var: &str,
    override_: &MaterialDef,
    overrides: &[MaterialDef],
    object_script: Option<&'a omsi_sim::scenery::SceneryInstance>,
    freetex_probe: Option<&'a omsi_sim::scenery::SceneryInstance>,
    strings: &'a [String],
) -> Option<&'a str> {
    let script_name = object_script.map(|s| s.str_var(var).trim()).unwrap_or("");
    let probe_name = freetex_probe.map(|p| p.str_var(var).trim()).unwrap_or("");
    let declared = object_script.map(|s| s.program.str_var(var).is_some()).unwrap_or(false)
        || freetex_probe.map(|p| p.program.str_var(var).is_some()).unwrap_or(false);
    if declared {
        // a name a script sets in {frame} is only in the probe's state at first (see the
        // caller), so both are read; neither having one leaves the slot's own texture
        let name = if !script_name.is_empty() { script_name } else { probe_name };
        let name = name.trim_matches('"');
        return (!name.is_empty()).then_some(name);
    }
    let string_by_idx = var.parse::<usize>().ok().and_then(|idx| strings.get(idx)).map(|s| s.trim()).unwrap_or("");
    let freetex_idx = overrides.iter().filter(|o| !o.item && o.freetex.is_some()).position(|o| std::ptr::eq(o, override_)).unwrap_or(0);
    let string_by_order = strings.get(freetex_idx).map(|s| s.trim()).unwrap_or("");
    let name = if !string_by_idx.is_empty() {
        string_by_idx
    } else if !string_by_order.is_empty() {
        string_by_order
    } else {
        return None;
    };
    let name = name.trim_matches('"');
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}
