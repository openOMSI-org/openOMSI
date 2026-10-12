//! A vehicle's passenger cabin as the people use it: doors, places, the path network of
//! every section joined into one, and the buses of this frame (`BusNow`).

use super::*;

/// An `[entry]` or `[exit]` of a cabin, in the bus frame.
#[derive(Debug, Clone)]
pub struct Door {
    /// The door's path point (the threshold) and its index.
    pub inside: Vec3,
    pub point: Option<usize>,
    /// Where somebody stands just outside, at ground level.
    pub outside: Vec3,
    /// +1 on the right side of the bus, -1 on the left.
    pub side: f32,
    /// Direction along the bus (+1 forwards) in which the queue at this door runs.
    pub queue_dir: f32,
    /// A passenger who still has to buy a ticket may board here (no `{noticketsale}`).
    pub sells: bool,
    /// `{withbutton}`: a door the passenger opens with the request button, worth walking to
    /// while it is still shut.
    pub button: bool,
    /// Where people getting off wait for the door to open: the path point next to it.
    pub wait: Vec3,
    /// The sections it belongs to that people walk between (see `Cabin::groups`).
    pub group: usize,
}

impl Door {
    /// Somebody standing at `p` (bus frame) is in the doorway (`PAX_Entry<n>_Busy`,
    /// `PAX_Exit<n>_Busy`, #720): within a body's width of the way through the door, from
    /// its threshold point across the bus side to the step outside - where a door's light
    /// barrier sees them. Not the queue waiting outside a shut door, nor the deck above it.
    /// It ends 0.4 m out from the bus side, clear of the line 0.5 m out that the people
    /// walking along a bus keep to (`clamp_x`), so they do not set it as they pass.
    pub fn in_doorway(&self, p: Vec3) -> bool {
        const BODY: f32 = 0.3;
        if p.z > self.inside.z + 1.0 {
            return false;
        }
        let a = self.inside.truncate();
        let b = glam::Vec2::new(self.outside.x - self.side * (DOOR_OUT - 0.1), self.inside.y);
        let ab = b - a;
        let t = if ab.length_squared() > 1e-6 { ((p.truncate() - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
        (a + ab * t - p.truncate()).length() < BODY
    }
}

#[derive(Debug, Clone)]
pub struct Seat {
    /// The `[passpos]` point: a seated passenger's hip, a standing one's feet.
    pub pos: Vec3,
    /// The floor in front of it, where the feet go (and where a seated passenger stands
    /// before sitting down and after getting up).
    pub floor: Vec3,
    pub rot: f32,
    pub seated: bool,
    /// The `[passpos]`'s seat height (+0x20; 0: a standing place).
    pub height: f32,
    /// Its number for the scripts (`GetHumanCountOnSeat`): Omsi.exe's place in the file
    /// among the `[passpos]` and `[drivpos]`, the sections behind counted on after those
    /// in front (0x7d39a4 asks the next one for a number past its own places).
    pub omsi_seat: usize,
    /// openOMSI's variables of the place (#721): the one that switches it on and off, and the
    /// one its occupancy is written into.
    pub switch_var: Option<String>,
    pub taken_var: Option<String>,
    /// The sections it lies in that people walk between (see `Cabin::groups`).
    pub group: usize,
}

/// What passengers need to know about one vehicle type's cabin.
pub struct Cabin {
    pub data: PassengerCabin,
    pub graph: PathGraph,
    pub links: Vec<(i32, i32, bool)>,
    /// Each link's footstep sounds: its section's `[stepsoundpack]` named by the link's
    /// `[next_stepsound]` (index into `step_packs`), none where the paths.cfg gives none -
    /// Omsi.exe hears no steps there - and on the joint between two sections.
    pub link_pack: Vec<Option<usize>>,
    pub step_packs: Vec<Arc<[String]>>,
    pub entries: Vec<Door>,
    pub exits: Vec<Door>,
    /// Where a passenger stands at the cash desk, its path point, and the heading (bus
    /// frame) they face: between the desk top, where the money goes, and the driver.
    pub desk: Option<(Vec3, Option<usize>, f64)>,
    pub seats: Vec<Seat>,
    /// The sections (one for a rigid bus), front first; everything above is in the
    /// unfolded frame of the front section.
    pub parts: Vec<CabinPart>,
    /// How many groups of sections people walk between (one but where a trailer hangs on
    /// that nobody walks into from the bus, #718), and each path point's group: a passenger
    /// gets in, rides and gets out within one.
    pub groups: usize,
    pub point_group: Vec<usize>,
    /// Each link's room height (`[next_roomheight]`; 2 m before any).
    pub link_room: Vec<f32>,
    /// The routing tables of the path network (sub_72410c).
    pub routes: Vec<Vec<RouteLink>>,
    /// The validators and the cash desk: (path point, device). Omsi.exe keeps one of each,
    /// the last of the file (cabin +0x14/+0x18, +0x28/+0x2c); every `[stamper]` of every
    /// section is kept here, and a passenger stamps at the one nearest the door they came
    /// in by (`Cabin::nearest_stamper`, #722) - with one, that one.
    pub stampers: Vec<(Option<usize>, Vec3)>,
    pub sale: Option<(Option<usize>, Vec3)>,
    /// Where the money goes (+0x38) and where the change is taken from (+0x58), with the
    /// money point's spread.
    pub money_point: Option<Vec3>,
    pub money_var: Option<(Vec3, [f32; 2], Option<String>)>,
    pub change_point: Option<Vec3>,
}

/// The people on each seat by the scripts' numbers (`Seat::omsi_seat`, the `[drivpos]`
/// counted with the `[passpos]`), from the places (indices into `seats`) taken by people
/// sitting there. (Counted by the `[passpos]` alone, every seat of a cabin with the
/// driver's place first was one off: a tip-up seat folded down under the next one.)
pub fn seat_numbers(seats: &[Seat], sitting: impl Iterator<Item = usize>) -> Vec<u32> {
    let n = seats.iter().map(|s| s.omsi_seat + 1).max().unwrap_or(0);
    let mut out = vec![0u32; n];
    for k in sitting {
        if let Some(c) = seats.get(k).and_then(|s| out.get_mut(s.omsi_seat)) {
            *c += 1;
        }
    }
    out
}

/// The places of a bus its scripts have switched off: a `[passpos]` naming a variable of
/// its own that is 0 now (#721), by index into the cabin's.
/// (A place in a section of its own without an entry or without an exit is off too: nobody
/// could get there, or out of it again, #718.)
pub fn places_off(v: &VehicleInstance, cabin: &Cabin) -> Vec<bool> {
    let unreached = |g: usize| cabin.groups > 1 && (!cabin.entries.iter().any(|e| e.group == g) || !cabin.exits.iter().any(|e| e.group == g));
    cabin
        .seats
        .iter()
        .map(|s| s.switch_var.as_deref().and_then(|n| v.var(n)).is_some_and(|x| x == 0.0) || unreached(s.group))
        .collect()
}

/// The occupancy variables of the places of bus `bn` that name one (#721), and whether
/// somebody is on each: a rider at that place (`sitting`: the places of its riders there).
pub fn places_taken(bn: &BusNow, sitting: &[(BusId, usize)]) -> Vec<(String, bool)> {
    bn.cabin
        .seats
        .iter()
        .enumerate()
        .filter_map(|(k, s)| s.taken_var.as_ref().map(|n| (n.clone(), sitting.contains(&(bn.id, k)))))
        .collect()
}

/// Which doorways of bus `bn` somebody stands in (entries, exits; see `Door::in_doorway`):
/// of the people `at` (inside a bus and where in its frame, or where in the world), its
/// riders and those outside close enough to it.
pub fn doorways_taken(bn: &BusNow, at: &[(Option<BusId>, DVec3)]) -> (Vec<bool>, Vec<bool>) {
    let mut entries = vec![false; bn.cabin.entries.len()];
    let mut exits = vec![false; bn.cabin.exits.len()];
    // (beyond the bus's own length round its origin nobody outside can be in a doorway)
    let reach = bn.half.length() + bn.centre.length() + bn.trailers.iter().map(|t| t.offset.length() as f64).fold(0.0, f64::max) + 2.0;
    for (inside, pos) in at {
        let local = match inside {
            Some(b) if *b == bn.id => pos.as_vec3(),
            Some(_) => continue,
            None if (*pos - bn.pos).truncate().length() < reach => bn.to_local(*pos),
            None => continue,
        };
        for (k, d) in bn.cabin.entries.iter().enumerate() {
            entries[k] |= d.in_doorway(local);
        }
        for (k, d) in bn.cabin.exits.iter().enumerate() {
            exits[k] |= d.in_doorway(local);
        }
    }
    (entries, exits)
}

/// A section of an articulated bus in its cabin's unfolded frame.
#[derive(Debug, Clone, Copy)]
pub struct CabinPart {
    /// Where the section's own origin lies.
    pub offset: Vec3,
    /// The unfolded y of the joint in front of it (the front section: none, +inf).
    pub joint_y: f32,
}

/// One vehicle of a coupled train as a cabin is put together from it: its definition, its
/// origin in the front vehicle's unfolded frame, and the unfolded y of its front joint.
pub type TrainPart<'a> = (&'a omsi_vehicle::Vehicle, Vec3, f32);

/// The sections of `v` passengers can walk through, front first: the vehicle and every
/// coupled part straight behind it (a part coupled the wrong way round and all behind it
/// are left out).
pub fn train_parts(v: &VehicleInstance) -> Vec<TrainPart<'_>> {
    let mut out: Vec<TrainPart<'_>> = vec![(&v.ty.def, Vec3::ZERO, f32::INFINITY)];
    let mut offset = Vec3::ZERO;
    for t in &v.trailers {
        if t.reversed {
            break;
        }
        let (back, front) = t.couplings();
        let joint_y = offset.y + back.y;
        offset += back - front;
        out.push((&t.ty.def, offset, joint_y));
    }
    out
}

impl Cabin {
    /// The cabin of a train of vehicles (see [`train_parts`]): the front one's, with the
    /// sections behind joined on as far as they have a cabin and a path network.
    pub fn load_train(parts: &[TrainPart<'_>]) -> Option<Cabin> {
        let (lead, _, _) = parts.first()?;
        let load_cabin = |def: &omsi_vehicle::Vehicle| -> Option<PassengerCabin> {
            let rel = def.passenger_cabin.as_ref()?;
            PassengerCabin::load(&omsi_cfg::resolve_path(def.dir(), rel))
                .map_err(|e| log::warn!("{e}"))
                .ok()
        };
        let load_paths = |def: &omsi_vehicle::Vehicle| {
            def.paths.as_ref().and_then(|rel| {
                omsi_vehicle::VehiclePaths::load(&omsi_cfg::resolve_path(def.dir(), rel))
                    .map_err(|e| log::warn!("{e}"))
                    .ok()
            })
        };
        let data = load_cabin(lead)?;
        let mut points: Vec<Vec3> = Vec::new();
        let mut links: Vec<(i32, i32, bool)> = Vec::new();
        let mut link_pack: Vec<Option<usize>> = Vec::new();
        let mut link_room: Vec<f32> = Vec::new();
        let mut step_packs: Vec<Arc<[String]>> = Vec::new();
        // (merged path point or -1, sells tickets, {withbutton}, half width of the section,
        // group)
        let mut entry_points: Vec<(i32, bool, bool, f32, usize)> = Vec::new();
        let mut exit_points: Vec<(i32, f32, usize)> = Vec::new();
        let mut places: Vec<(omsi_vehicle::cabin::PassPos, Vec3, usize, usize)> = Vec::new();
        let mut group = 0usize;
        let mut point_group: Vec<usize> = Vec::new();
        // (the script seat numbers of the sections in front)
        let mut seat_base = 0usize;
        let mut cabin_parts: Vec<CabinPart> = Vec::new();
        let mut stampers: Vec<(Option<usize>, Vec3)> = Vec::new();
        // the point of the section in front that leads on to the next one, and that one as
        // its cabin gives it (`[linkToPrevVeh]`)
        let mut rear_link: Option<usize> = None;
        let mut rear_declared: Option<usize> = None;
        for (k, (def, offset, joint_y)) in parts.iter().enumerate() {
            let cab = if k == 0 {
                Some(data.clone())
            } else {
                load_cabin(def)
            };
            let Some(cab) = cab else { break };
            let (own, own_links, own_steps, own_packs, own_rooms): (Vec<Vec3>, Vec<(i32, i32, bool)>, Vec<i32>, Vec<Vec<String>>, Vec<f32>) = match load_paths(def) {
                Some(p) => (
                    p.points
                        .iter()
                        .map(|q| Vec3::from(q.pos) + *offset)
                        .collect(),
                    p.links,
                    p.link_step_sound,
                    p.step_sound_packs,
                    p.link_room_height,
                ),
                None => (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            };
            let base = points.len();
            let valid = |i: i32| (i >= 0 && (i as usize) < own.len()).then_some(base + i as usize);
            let end = |front: bool| {
                (0..own.len())
                    .filter(|i| own[*i].x.abs() < 0.6)
                    .max_by(|a, b| {
                        if front {
                            own[*a].y.total_cmp(&own[*b].y)
                        } else {
                            own[*b].y.total_cmp(&own[*a].y)
                        }
                    })
                    .map(|i| base + i)
            };
            if k > 0 {
                // through the joint: from the front section's [linkToPrevVeh] point to this
                // one's [linkToNextVeh] point (the frontmost aisle point when it has none). A
                // trailer's hitch (`[coupling_front_character]` type 0, a lorry's coupling) is
                // no bellows: walked through only where both cabins give those points, as
                // Omsi.exe joins coupled cabins (0x7d172c) - joined at the aisle's ends, the
                // passengers of a bus walked through its back wall into the trailer behind
                let hitch = def.coupling_front_character.is_some_and(|c| c[3] == 0.0);
                let front = cab.link_to_next_veh.and_then(valid).or_else(|| if hitch { None } else { end(true) });
                match (if hitch { rear_declared } else { rear_link }, front) {
                    (Some(a), Some(b)) => {
                        links.push((a as i32, b as i32, false));
                        link_pack.push(None);
                        link_room.push(2.0);
                    }
                    // no way through: a section of its own, which people get into and out
                    // of by its own doors and ride in (#718; it stayed empty)
                    _ if !own.is_empty() => group += 1,
                    // nowhere to walk in it
                    _ => break,
                }
            }
            points.extend(own.iter().copied());
            point_group.extend(std::iter::repeat_n(group, own.len()));
            links.extend(
                own_links
                    .iter()
                    .map(|(a, b, o)| (a + base as i32, b + base as i32, *o)),
            );
            let pack_base = step_packs.len();
            link_pack.extend((0..own_links.len()).map(|i| {
                let n = own_steps.get(i).copied().unwrap_or(-1);
                (n >= 0 && (n as usize) < own_packs.len()).then(|| pack_base + n as usize)
            }));
            step_packs.extend(own_packs.into_iter().map(Arc::from));
            link_room.extend((0..own_links.len()).map(|i| own_rooms.get(i).copied().unwrap_or(2.0)));
            rear_declared = cab.link_to_prev_veh.and_then(valid);
            rear_link = rear_declared.or_else(|| end(false));
            let half = def
                .bounding_box
                .map(|b| b[0] * 0.5)
                .unwrap_or_else(|| own.iter().map(|p| p.x.abs()).fold(1.2, f32::max));
            let shift = |i: i32| valid(i).map(|m| m as i32).unwrap_or(-1);
            entry_points.extend(
                cab.entries
                    .iter()
                    .map(|e| (shift(e.path_point), !e.no_ticket_sale, e.with_button, half, group)),
            );
            exit_points.extend(cab.exits.iter().map(|e| (shift(*e), half, group)));
            places.extend(cab.pass_positions.iter().map(|p| (p.clone(), *offset, seat_base + p.file_index, group)));
            stampers.extend(cab.stampers.iter().map(|st| (valid(st.path_point), Vec3::from(st.pos) + *offset)));
            seat_base += cab.pass_positions.len() + cab.driver_positions.len();
            cabin_parts.push(CabinPart {
                offset: *offset,
                joint_y: *joint_y,
            });
        }
        let graph = PathGraph::new(points.clone(), &links);
        // (the side of the road the stops are on: where a door's own point does not tell)
        let kerb = if LEFT_HAND.load(std::sync::atomic::Ordering::Relaxed) { -1.0f32 } else { 1.0 };
        let door = |pp: i32, sells: bool, button: bool, half_width: f32, group: usize| -> Door {
            let point = (pp >= 0 && (pp as usize) < points.len()).then_some(pp as usize);
            let inside = point
                .map(|i| points[i])
                .unwrap_or(Vec3::new(kerb * (half_width - 0.1), 4.0, 0.4));
            // A door's side is the side of its entry point; one in the middle of the aisle
            // (or none) is taken to open to the kerb - on the left where the traffic keeps
            // left. (Always the right: a UK bus whose entry point lies on the aisle had the
            // people come to its door from the road side, round the bus.)
            let side = if inside.x.abs() < 0.6 { kerb } else if inside.x >= 0.0 { 1.0 } else { -1.0 };
            let outside = Vec3::new(side * (half_width + DOOR_OUT), inside.y, 0.0);
            // the aisle point next to the door: its neighbour nearest the middle
            let wait_point = point
                .and_then(|i| {
                    graph
                        .neighbours(i)
                        .into_iter()
                        .min_by(|a, b| points[*a].x.abs().total_cmp(&points[*b].x.abs()))
                })
                .filter(|&w| (points[w].x - inside.x).abs() > 0.3);
            // (no aisle point linked beside the door - the W906's door steps lead straight on
            // along it: the nearest path point off the door's line, else a step inwards)
            let wait = wait_point.map(|w| points[w]).unwrap_or_else(|| {
                points
                    .iter()
                    .filter(|p| (p.x - inside.x).abs() > 0.3 && (p.truncate() - inside.truncate()).length() < 1.2 && (p.z - inside.z).abs() < 0.6)
                    .min_by(|a, b| (a.truncate() - inside.truncate()).length().total_cmp(&(b.truncate() - inside.truncate()).length()))
                    .copied()
                    .unwrap_or(Vec3::new(inside.x - side * 0.7, inside.y, inside.z))
            });
            Door {
                inside,
                point,
                outside,
                side,
                queue_dir: -1.0,
                sells,
                button,
                wait,
                group,
            }
        };
        let mut entries: Vec<Door> = entry_points
            .iter()
            .map(|(pp, sells, button, half, g)| door(*pp, *sells, *button, *half, *g))
            .collect();
        let exits: Vec<Door> = exit_points
            .iter()
            .map(|(pp, half, g)| door(*pp, false, false, *half, *g))
            .collect();
        // two leaves of one door: the queue of the front leaf runs forwards, the other's back,
        // so that the two lines do not stand in each other
        for i in 0..entries.len() {
            let partner = (0..entries.len()).find(|&j| {
                j != i
                    && entries[j].side == entries[i].side
                    && (entries[j].inside.y - entries[i].inside.y).abs() < 1.4
            });
            entries[i].queue_dir = match partner {
                Some(j) if entries[j].inside.y < entries[i].inside.y => 1.0,
                _ => -1.0,
            };
        }
        let desk = data.ticket_sales.first().map(|ts| {
            let top = Vec3::from(ts.pos);
            let by_point = usize::try_from(ts.path_point)
                .ok()
                .and_then(|i| points.get(i).map(|p| (i, *p)))
                .filter(|(_, p)| (top.truncate() - p.truncate()).length() < 3.0);
            let (stand, pi) = match by_point {
                Some((i, p)) => (p, Some(i)),
                None => {
                    // no usable path point: the nearest one on the entry floor, else the floor by the desk
                    let floor = entries.first().map(|e| e.inside.z).unwrap_or(0.4);
                    let near = points
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| (p.z - floor).abs() < 0.6)
                        .min_by(|a, b| {
                            (a.1.truncate() - top.truncate())
                                .length()
                                .total_cmp(&(b.1.truncate() - top.truncate()).length())
                        });
                    match near {
                        Some((i, p)) => (*p, Some(i)),
                        None => (Vec3::new(top.x + 0.4, top.y, floor), None),
                    }
                }
            };
            let target = match data.driver_positions.first() {
                Some(d) => (top + Vec3::from(d.pos)) * 0.5,
                None => top,
            };
            let d = target - stand;
            let face = if d.truncate().length() > 0.05 {
                (d.x as f64).atan2(d.y as f64).to_degrees()
            } else {
                -90.0
            };
            (stand, pi, face)
        });
        let seats = places
            .iter()
            .map(|(p, offset, omsi_seat, group)| {
                let pos = Vec3::from(p.pos) + *offset;
                let seated = p.height > 0.01;
                let floor = if seated {
                    let r = p.rot.to_radians();
                    Vec3::new(
                        pos.x + r.sin() * SEAT_FRONT,
                        pos.y + r.cos() * SEAT_FRONT,
                        pos.z - p.height,
                    )
                } else {
                    pos
                };
                Seat {
                    pos,
                    floor,
                    rot: p.rot,
                    seated,
                    height: p.height,
                    omsi_seat: *omsi_seat,
                    switch_var: p.switch_var.clone(),
                    taken_var: p.taken_var.clone(),
                    group: *group,
                }
            })
            .collect();
        let routes = build_routes(graph.points.len(), &links);
        let point_of = |i: i32| usize::try_from(i).ok().filter(|i| *i < graph.points.len());
        let sale = data.ticket_sales.last().map(|st| (point_of(st.path_point), Vec3::from(st.pos)));
        let money_point = data.money_points.last().map(|m| Vec3::from(m.pos));
        let money_var = data.money_points.last().map(|m| (Vec3::from(m.pos), m.var, m.parent.clone()));
        let change_point = data.change_points.last().map(|m| Vec3::from(m.pos));
        Some(Cabin {
            data,
            graph,
            links,
            link_pack,
            step_packs,
            entries,
            exits,
            desk,
            seats,
            parts: cabin_parts,
            groups: group + 1,
            point_group,
            link_room,
            routes,
            stampers,
            sale,
            money_point,
            money_var,
            change_point,
        })
    }

    /// Every point of the path network (Omsi.exe's list +0xc of the paths).
    pub fn all_points(&self) -> Vec<Option<usize>> {
        (0..self.graph.points.len()).map(Some).collect()
    }











}

/// A bus as the passengers see it this frame.
#[derive(Clone)]
pub struct BusNow {
    pub id: BusId,
    pub next_stop: Option<RequestStop>,
    pub cabin: Arc<Cabin>,
    pub pos: DVec3,
    pub rot: Mat4,
    pub heading: f64,
    /// m/s, forwards.
    pub speed: f64,
    pub entry_open: Vec<bool>,
    pub exit_open: Vec<bool>,
    /// The doors a walker may use (another player's bus: its doors as they are, while
    /// `entry_open` stays shut for the passengers here); None: as `entry_open`/`exit_open`.
    pub walk_open: Option<(Vec<bool>, Vec<bool>)>,
    pub interior: f32,
    /// The saloon's air and the light outside, for what boarding passengers say.
    pub air: CabinAir,
    /// Half extents across / along and the centre of its bounding box (bus frame).
    pub half: DVec2,
    pub centre: DVec2,
    /// Acceleration of the floor (bus frame: x to the right, y forwards; m/s²).
    pub accel: DVec2,
    /// The sections behind the front one (the cabin's parts after the first).
    pub trailers: Vec<PartFrame>,
    /// The terminus it shows, by name (Omsi.exe's bus +0x7bc). None: "$allexit$" - the
    /// scripts' `target_index_int` names a hof terminus added with `[addterminus_allexit]`
    /// ("Nicht einsteigen", a works trip) - or none; no timetable target has it.
    pub terminus: Option<String>,
    /// Whom it takes on at the stops.
    pub takes: Takes,
    /// The places its scripts have switched off (see `places_off`; empty: none).
    pub places_off: Vec<bool>,
    /// A timetable bus boarding at a stop the passengers' nearby stops do not have (its
    /// tile is not among theirs): the stop its timetable serves, so that its riders still
    /// get off there.
    pub served: Option<i64>,
}

/// What passengers feel stepping into a bus (OMSI reads the same fields: the vehicle's
/// `Cabinair_Temp` and `Cabinair_relHum`, the weather's temperature and the daylight).
#[derive(Debug, Clone, Copy, Default)]
pub struct CabinAir {
    /// °C, when the bus keeps its cabin air (every bus does: its script or the engine).
    pub temp: Option<f32>,
    /// Relative humidity, a fraction.
    pub rel_hum: f32,
    /// The temperature outside (°C).
    pub outside: f32,
    /// `Envir_Brightness`: the daylight, 0 dark .. 1.
    pub brightness: f32,
}

impl CabinAir {
    pub fn of(v: &VehicleInstance) -> CabinAir {
        CabinAir {
            temp: v.var("Cabinair_Temp").filter(|t| t.is_finite()),
            rel_hum: v.var("Cabinair_relHum").filter(|h| h.is_finite()).unwrap_or(0.0),
            outside: v.host.temperature,
            brightness: v.var("Envir_Brightness").unwrap_or(1.0),
        }
    }
}

/// Where a rear section of a bus is this frame, with its place in the cabin.
#[derive(Debug, Clone, Copy)]
pub struct PartFrame {
    pub pos: DVec3,
    pub rot: Mat4,
    pub heading: f64,
    pub offset: Vec3,
    pub joint_y: f32,
    /// Half extents across / along and the centre of its bounding box (own frame).
    pub half: DVec2,
    pub centre: DVec2,
}

/// The rear sections of `v` that are parts of `cabin`, as they stand now.
pub fn part_frames(v: &VehicleInstance, cabin: &Cabin) -> Vec<PartFrame> {
    cabin
        .parts
        .iter()
        .skip(1)
        .zip(&v.trailers)
        .map(|(cp, t)| {
            let bb =
                t.ty.def
                    .bounding_box
                    .unwrap_or([2.5, 7.0, 3.0, 0.0, 0.0, 1.5]);
            PartFrame {
                pos: t.position,
                rot: t.body_rotation(),
                heading: t.heading,
                offset: cp.offset,
                joint_y: cp.joint_y,
                half: DVec2::new(bb[0] as f64 * 0.5, bb[1] as f64 * 0.5),
                centre: DVec2::new(bb[3] as f64, bb[4] as f64),
            }
        })
        .collect()
}

/// How far into the frame of the section behind joint `t` a cabin point `y` lies: 0 in
/// front of the joint's blend, 1 behind it.
pub fn behind(t: &PartFrame, y: f32) -> f32 {
    ((JOINT_BLEND - (y - t.joint_y)) / (2.0 * JOINT_BLEND)).clamp(0.0, 1.0)
}

/// Where a point of a cabin (unfolded frame) is in the world: the front section carries
/// what lies ahead of the first joint, a rear section what lies behind its joint, and near
/// a joint the two are blended, so that somebody walking through the bellows moves on
/// smoothly however far the bus is bent.
pub fn train_point(pos: DVec3, rot: &Mat4, trailers: &[PartFrame], local: Vec3) -> DVec3 {
    let mut here = pos + rot.transform_point3(local).as_dvec3();
    for t in trailers {
        let w = behind(t, local.y);
        if w <= 0.0 {
            break;
        }
        let there = t.pos + t.rot.transform_point3(local - t.offset).as_dvec3();
        here = here.lerp(there, w as f64);
        if w < 1.0 {
            break;
        }
    }
    here
}

fn train_tilt(rot: &Mat4, heading: f64, trailers: &[PartFrame], local: Vec3) -> Mat4 {
    let (mut rot, mut heading) = (*rot, heading);
    for t in trailers {
        if behind(t, local.y) < 0.5 {
            break;
        }
        rot = t.rot;
        heading = t.heading;
    }
    rot * Mat4::from_rotation_z(heading.to_radians() as f32)
}

#[derive(Debug, Clone)]
pub struct DrawnBus {
    pub pos: DVec3,
    pub rot: Mat4,
    pub heading: f64,
    pub trailers: Vec<PartFrame>,
}

impl DrawnBus {
    pub fn of(v: &VehicleInstance, cabin: &Cabin) -> DrawnBus {
        DrawnBus { pos: v.position, rot: v.body_rotation(), heading: v.heading, trailers: part_frames(v, cabin) }
    }

    pub fn world(&self, local: Vec3) -> DVec3 {
        train_point(self.pos, &self.rot, &self.trailers, local)
    }

    pub fn heading_at(&self, local: Vec3) -> f64 {
        train_heading(self.heading, &self.trailers, local)
    }

    pub fn tilt_at(&self, local: Vec3) -> Mat4 {
        train_tilt(&self.rot, self.heading, &self.trailers, local)
    }
}

/// The heading of the floor at a point of a cabin (see [`train_point`]).
pub fn train_heading(heading: f64, trailers: &[PartFrame], local: Vec3) -> f64 {
    let mut here = heading;
    for t in trailers {
        let w = behind(t, local.y);
        if w <= 0.0 {
            break;
        }
        here += crowd::angle_diff(here, t.heading) * w as f64;
        if w < 1.0 {
            break;
        }
    }
    here
}

impl BusNow {
    pub fn world(&self, local: Vec3) -> DVec3 {
        train_point(self.pos, &self.rot, &self.trailers, local)
    }
    /// A world point in the cabin's frame (the inverse of `world`): the front section's,
    /// or a rear section's for a point behind its joint.
    pub fn to_local(&self, w: DVec3) -> Vec3 {
        let mut l = self.rot.inverse().transform_point3((w - self.pos).as_vec3());
        for t in &self.trailers {
            if l.y > t.joint_y {
                break;
            }
            l = t.rot.inverse().transform_point3((w - t.pos).as_vec3()) + t.offset;
        }
        l
    }
    /// The tilt (pitch and bank, in the world's axes, no heading) of the section a point of
    /// the cabin is in.
    pub fn tilt_at(&self, local: Vec3) -> Mat4 {
        train_tilt(&self.rot, self.heading, &self.trailers, local)
    }

    pub fn follow(&self, drawn: &DrawnBus, local: Vec3, position: DVec3, heading: f64, tilt: Mat4) -> (DVec3, f64, Mat4) {
        (
            position + (drawn.world(local) - self.world(local)),
            heading + crowd::angle_diff(self.heading_at(local), drawn.heading_at(local)),
            drawn.tilt_at(local) * self.tilt_at(local).inverse() * tilt,
        )
    }
    /// The heading of the section a point of the cabin is in.
    pub fn heading_at(&self, local: Vec3) -> f64 {
        train_heading(self.heading, &self.trailers, local)
    }
    pub fn fwd(&self) -> DVec2 {
        let h = self.heading.to_radians();
        DVec2::new(h.sin(), h.cos())
    }
    /// The bodies people on the ground walk round: the bus and its rear sections.
    pub fn blocks(&self) -> Vec<Block> {
        let block = |pos: DVec3, heading: f64, half: DVec2, centre: DVec2| {
            let h = heading.to_radians();
            let (fwd, right) = (DVec2::new(h.sin(), h.cos()), DVec2::new(h.cos(), -h.sin()));
            Block {
                center: pos.truncate() + right * centre.x + fwd * centre.y,
                half,
                heading: h,
                vel: fwd * self.speed,
            }
        };
        let mut out = vec![block(self.pos, self.heading, self.half, self.centre)];
        out.extend(
            self.trailers
                .iter()
                .map(|t| block(t.pos, t.heading, t.half, t.centre)),
        );
        out
    }
}
