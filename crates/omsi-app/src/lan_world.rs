//! LAN play: one world for everybody. The host's game simulates the AI traffic, the
//! timetable buses, the people on the pavements and at the stops, and the traffic lights -
//! for the whole session, around every player (`Traffic::lan_centers`,
//! `Humans::lan_centers`, the tile streamer). A client simulates none of that: it draws
//! what the host sends - the cars from the traffic table (`omsi_net::traffic_table`, which
//! the traffic writes after each step: `Traffic::write_table`), the people and the lights
//! from the world frames (`omsi_net::world`) - a little in the past so that it can glide
//! between two frames, and simulates only its own bus and the people who board it.
//!
//! Who a waiting passenger belongs to is decided by the host alone: a client whose bus
//! stands at a stop with a door open asks for the people waiting there (`CLAIM`); the host
//! hands over those still waiting (they leave its world) and turns down the others, who
//! have meanwhile walked up to another bus. Nobody is ever on two buses.
//!
//! What goes over the network per client, around where its bus is: the cars of the table
//! within its range, as changes of what the client acknowledged, nearer ones more often and
//! within a byte budget (`TRAFFIC_BYTES_PER_SECOND`); people within `PERSON_RADIUS` (ten
//! times a second while they move, once a second while they stand), the light programs
//! within `LIGHT_RADIUS` (once a second), and once per person a description (`DESC`: its
//! human file). `OMSI_DEBUG_LAN` logs the counts and the bytes a second,
//! `OMSI_LAN_TRACE=<file.csv>` every car pose of the table (host) and drawn (client) for
//! comparing them.

use crate::humans::{Humans, MirrorPose};
use crate::scene::World;
use crate::traffic::Traffic;
use crate::view_sync::people::PeopleView;
use crate::view_sync::SimView;
use omsi_lan::config::LanConfig;
use glam::{DVec2, DVec3, Vec3};
use hashbrown::{HashMap, HashSet};
use omsi_net::traffic_table::{ClientTable, TrafficId};
use omsi_net::world::{
    self as nw, Activity as NetActivity, Desc, EntityRef, LightState, PersonPlace, PersonState,
    WorldFrame,
};
use omsi_net::{LanSession, Role};
use omsi_render::{Renderer, Scene};
use omsi_sim::vehicle::AiFrame;
use omsi_sim::VehicleType;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::Instant;

/// How far around a client its cars (the table's range, `ClientSync::radius_in`), people
/// and light programs are sent (m).
pub const CAR_RADIUS: f64 = 650.0;
/// The bytes a second the cars of the table take at most to each client.
pub const TRAFFIC_BYTES_PER_SECOND: f64 = 40_000.0;
pub const PERSON_RADIUS: f64 = 260.0;
pub const LIGHT_RADIUS: f64 = 450.0;
/// How far in the past a client draws the host's world (ms): two frames of moving things
/// to glide between, and room for a late datagram.
const INTERP_DELAY: f64 = 160.0;
/// A description asked for is asked again after this long (s).
const WANT_AGAIN: f32 = 0.8;
const MOVING_EVERY_S: f32 = nw::MOVING_EVERY;

type Key = (bool, u32);

/// How a car's script context goes to one client: at first often (a car is not made before
/// its context is whole), later now and then (the timetable's delay and the system values
/// move on).
#[derive(Default)]
struct ContextSent {
    page: usize,
    sent: u32,
    age: f32,
}

impl ContextSent {
    fn due(&mut self, dt: f32, bus: bool) -> bool {
        self.age += dt;
        let every = if self.sent < 4 { 0.15 } else if bus { 1.0 } else { 4.0 };
        self.age >= every
    }
}

/// What the host has sent one client.
#[derive(Default)]
struct View {
    acc: f32,
    lights_acc: f32,
    /// Per thing: the quantised pose sent last and the seconds since.
    sent: HashMap<Key, ([i64; 4], f32)>,
    /// Per thing: the hash of the description sent.
    described: HashMap<Key, u64>,
    /// Per car (its table id): the page of its script context sent last, how many pages have
    /// gone, and the seconds since.
    context_pages: HashMap<u32, ContextSent>,
    /// Things that went out of sight, named for a second more.
    gone: Vec<(Key, f32)>,
    bytes: u64,
    /// What was in the last frame (for the log: people, light programs).
    last_counts: (usize, usize),
    ids_hash: u64,
}







pub(crate) use omsi_lan::mirror::PlayClock;
use omsi_lan::mirror::{fnv, lerp_angle, local_file, net_activity, quant, sim_activity, Track};

/// A client's copy of the host's world.
#[derive(Default)]
struct Mirror {
    on: bool,
    people: HashMap<u32, Track<PersonState>>,
    descs: HashMap<Key, Desc>,
    context_receiver: omsi_net::ai_context::Receiver,
    contexts: HashMap<u32, omsi_net::ai_context::Context>,
    /// When a car's context (by table id) last came.
    context_heard: HashMap<u32, Instant>,
    /// Cars of the table not drawn yet: waiting for their context, for their type, or left out.
    waiting: (usize, usize, usize),
    /// Descriptions asked for and when.
    wanted: HashMap<Key, Instant>,
    /// Vehicle types by file: loaded, being loaded, or not to be had.
    types: HashMap<PathBuf, Option<Arc<VehicleType>>>,
    loading: HashSet<PathBuf>,
    loaded_rx: Option<mpsc::Receiver<(PathBuf, Option<Arc<VehicleType>>)>>,
    loaded_tx: Option<mpsc::Sender<(PathBuf, Option<Arc<VehicleType>>)>>,
    /// Depot files by vehicle file (the destination displays of the timetable buses).
    hofs: HashMap<PathBuf, Option<Arc<omsi_vehicle::Hof>>>,
    /// The line and destination each car shows.
    shown: HashMap<u32, (String, String)>,
    /// Cars and people drawn (created here).
    drawn_cars: HashSet<u32>,
    drawn_people: HashSet<u32>,
    /// Distance each car has rolled (its wheels).
    odometer: HashMap<u32, f32>,
    /// The host's clock minus ours (ms), from the frames that came fastest.
    offset: Option<f64>,
    play: PlayClock,
    epoch: Option<Instant>,
    bytes_at: u64,
    granted: u32,
    denied: u32,
}

impl Mirror {
    fn now_ms(&mut self) -> f64 {
        let e = *self.epoch.get_or_insert_with(Instant::now);
        e.elapsed().as_secs_f64() * 1000.0
    }
}

/// The people of a client's own on foot, as the host draws them (host).
#[derive(Default)]
struct Upstream {
    tracks: HashMap<u32, Track<PersonState>>,
    files: HashMap<u32, String>,
    /// Their ids here, by the client's.
    ids: HashMap<u32, u32>,
    offset: Option<f64>,
    play: PlayClock,
}

/// What a client sends the host of its own people.
#[derive(Default)]
struct UpView {
    acc: f32,
    sent: HashMap<u32, ([i64; 4], f32)>,
    described: HashSet<u32>,
    gone: Vec<(u32, f32)>,
}

#[derive(Default)]
pub struct LanWorld {
    views: HashMap<u32, View>,
    mirror: Mirror,
    ups: HashMap<u32, Upstream>,
    up_view: UpView,
    /// The next id a client's person gets here (far above our own people's).
    next_up_id: u32,
    epoch: Option<Instant>,
    log_t: f32,
    trace: Option<std::io::BufWriter<std::fs::File>>,
    trace_t: f32,
    trace_opened: bool,
    /// Host: the parking spaces whose cars have driven off, as the world has them now.
    departed: Vec<i64>,
    /// Host: bytes of the traffic table sent so far.
    traffic_bytes: u64,
}








/// The line a timetable bus shows and the depot terminus it is set to, `#<index>` (host;
/// `#-1` for none, so that a client still knows it is a timetable bus).


impl LanWorld {
    /// Once a frame, after `LanSession::tick`.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        lan: &mut LanSession,
        dt: f32,
        args: &LanConfig,
        world: Option<&World>,
        renderer: Option<&Renderer>,
        scene: Option<&mut Scene>,
        traffic: Option<&mut Traffic>,
        humans: Option<&mut Humans>,
        view: &mut SimView,
        me: Option<DVec3>,
    ) {
        if !self.trace_opened {
            self.trace_opened = true;
            self.trace = omsi_cfg::flags::OMSI_LAN_TRACE.var().and_then(|p| {
                std::fs::File::create(p)
                    .ok()
                    .map(std::io::BufWriter::new)
            });
        }
        match lan.role {
            Role::Host => {
                let mut humans = humans;
                self.departed = world.map(|w| w.departed_keys()).unwrap_or_default();
                self.host(lan, dt, traffic, humans.as_deref_mut(), me);
                if let (Some(w), Some(r), Some(sc), Some(h)) = (world, renderer, scene, humans) {
                    self.people_from_clients(lan, w, r, sc, h, &mut view.people);
                }
            }
            Role::Client => {
                let mut humans = humans;
                self.client(lan, dt, args, world, renderer, scene, traffic, humans.as_deref_mut(), view);
                if let (Some(h), Some(me)) = (humans, me) {
                    self.people_to_host(lan, dt, h, me);
                }
            }
        }
    }

    /// The people of our own on foot near our bus, to the host (client): those who walk up
    /// to it and those who got off, so that the others see them too.
    fn people_to_host(&mut self, lan: &mut LanSession, dt: f32, h: &Humans, me: DVec3) {
        if !self.mirror.on {
            return;
        }
        let v = &mut self.up_view;
        v.acc += dt;
        for s in v.sent.values_mut() {
            s.1 += dt;
        }
        if v.acc + 1.0e-4 < nw::MOVING_EVERY {
            return;
        }
        v.acc = (v.acc - nw::MOVING_EVERY).clamp(0.0, nw::MOVING_EVERY);
        // (their ids on the wire: those the host handed over keep the host's, the others
        // are numbered from 2^23 up)
        let wire = |id: u32| {
            if id >= 1 << 30 {
                ((id - (1 << 30)) & 0x7F_FFFF) | 0x80_0000
            } else {
                id & 0x7F_FFFF
            }
        };
        let mut frame = WorldFrame::default();
        let mut seen: HashSet<u32> = HashSet::new();
        for p in h.lan_people(me, PERSON_RADIUS) {
            if p.aboard.is_some() {
                continue;
            }
            let id = wire(p.id);
            seen.insert(id);
            if v.described.insert(id) {
                lan.send_desc_up(&Desc::Person {
                    id,
                    file: Humans::type_file(&p.ty),
                });
            }
            let q = quant(p.pos.x, p.pos.y, p.pos.z, p.heading);
            let q = [q[0], q[1], q[2], q[3] ^ ((p.activity as i64) << 40)];
            let due = match v.sent.get(&id) {
                None => true,
                Some((last, age)) => *last != q || *age >= nw::STANDING_EVERY,
            };
            if !due {
                continue;
            }
            v.sent.insert(id, (q, 0.0));
            frame.people.push(PersonState {
                id,
                activity: net_activity(p.activity),
                place: PersonPlace::Foot {
                    x: p.pos.x,
                    y: p.pos.y,
                    z: p.pos.z,
                    heading: p.heading as f32,
                    speed: p.speed as f32,
                    waiting: None,
                },
            });
        }
        // and the riders of our bus, where they sit or stand in it
        for p in h.lan_riders() {
            let Some((_, l, lh, seat)) = p.aboard else { continue };
            let id = wire(p.id);
            seen.insert(id);
            if v.described.insert(id) {
                lan.send_desc_up(&Desc::Person {
                    id,
                    file: Humans::type_file(&p.ty),
                });
            }
            let place = PersonPlace::Aboard {
                bus: nw::PLAYER_BUS | lan.my_id,
                x: l.x,
                y: l.y,
                z: l.z,
                heading: lh as f32,
                seat: seat.map(|s| s.min(254) as u8),
            };
            let st = PersonState { id, activity: net_activity(p.activity), place };
            let q = person_q(&st);
            let due = match v.sent.get(&id) {
                None => true,
                Some((last, age)) => *last != q || *age >= nw::STANDING_EVERY,
            };
            if !due {
                continue;
            }
            v.sent.insert(id, (q, 0.0));
            frame.people.push(st);
        }
        let gone: Vec<u32> = v.sent.keys().filter(|k| !seen.contains(*k)).copied().collect();
        for k in gone {
            v.sent.remove(&k);
            v.gone.push((k, 1.0));
        }
        for g in v.gone.iter_mut() {
            g.1 -= nw::MOVING_EVERY;
        }
        v.gone.retain(|g| g.1 > 0.0 && !seen.contains(&g.0));
        frame.gone = v.gone.iter().map(|(k, _)| (true, *k)).collect();
        if !frame.people.is_empty() || !frame.gone.is_empty() {
            lan.send_world_up(&frame);
        }
    }

    /// The clients' own people on foot, drawn here (host).
    fn people_from_clients(
        &mut self,
        lan: &mut LanSession,
        world: &World,
        renderer: &Renderer,
        scene: &mut Scene,
        h: &mut Humans,
        view: &mut PeopleView,
    ) {
        // (claims of our bus for somebody a client sent: not ours to give)
        let _ = h.take_claims();
        let epoch = *self.epoch.get_or_insert_with(Instant::now);
        for (id, d) in lan.take_descs_up() {
            let Desc::Person { id: pid, file } = d;
            self.ups.entry(id).or_default().files.insert(pid, file);
        }
        for (id, at, f) in lan.take_world_up() {
            let up = self.ups.entry(id).or_default();
            let arrived = at
                .checked_duration_since(epoch)
                .map(|d| d.as_secs_f64() * 1000.0)
                .unwrap_or(0.0);
            let off = f.host_ms as f64 - arrived;
            up.offset = Some(match up.offset {
                // (drifting down 2 ms a second at their 8 frames a second: 20 ms a frame
                // made it follow every frame's way over the network)
                Some(o) if (off - o).abs() < 1000.0 => off.max(o - 0.25),
                _ => off,
            });
            for p in f.people {
                up.tracks
                    .entry(p.id)
                    .or_insert_with(|| Track {
                        samples: Vec::new(),
                        heard: Instant::now(),
                    })
                    .push(f.host_ms as f64, p);
            }
            for (_, pid) in f.gone {
                up.tracks.remove(&pid);
            }
        }
        // players that left: their people go with them
        let here: HashSet<u32> = lan.peers().map(|p| p.pose.id).collect();
        let forget = std::time::Duration::from_secs_f32(nw::FORGET_AFTER);
        let now = epoch.elapsed().as_secs_f64() * 1000.0;
        for (peer, up) in self.ups.iter_mut() {
            if !here.contains(peer) {
                up.tracks.clear();
            }
            up.tracks.retain(|_, t| t.heard.elapsed() < forget);
            let gone: Vec<u32> = up
                .ids
                .keys()
                .copied()
                .filter(|k| !up.tracks.contains_key(k))
                .collect();
            for k in gone {
                if let Some(local) = up.ids.remove(&k) {
                    h.mirror_remove(local);
                }
            }
            let render_ms = up.play.step(now, now + up.offset.unwrap_or(0.0) - INTERP_DELAY, 500.0);
            for (pid, track) in &up.tracks {
                let Some((a, b, k)) = track.around(render_ms) else {
                    continue;
                };
                let pose = person_pose(a, b, k);
                if let Some(local) = up.ids.get(pid).copied() {
                    if h.has_mirror(local) {
                        h.mirror_set(local, &pose);
                        continue;
                    }
                    up.ids.remove(pid);
                }
                if pose.aboard.map(|a| !h.knows_bus(a.0)).unwrap_or(false) {
                    continue;
                }
                let Some(ty) = up.files.get(pid).and_then(|f| h.type_by_file(f)) else {
                    continue;
                };
                if self.next_up_id < 1 << 29 {
                    self.next_up_id = 1 << 29;
                }
                let local = self.next_up_id;
                self.next_up_id += 1;
                let added = h.mirror_add(world, local, ty, &pose);
                crate::view_sync::apply_people(h, view, world, renderer, scene);
                if added {
                    up.ids.insert(*pid, local);
                }
            }
        }
        self.ups.retain(|peer, up| here.contains(peer) || !up.ids.is_empty());
    }

    // -----------------------------------------------------------------------------------
    // host

    fn host(
        &mut self,
        lan: &mut LanSession,
        dt: f32,
        mut traffic: Option<&mut Traffic>,
        mut humans: Option<&mut Humans>,
        me: Option<DVec3>,
    ) {
        // where the players are: the world is kept around them too
        let players: Vec<(u32, DVec3)> = lan
            .peers()
            .filter(|p| p.has_pose && p.pose.has_vehicle())
            .map(|p| (p.pose.id, DVec3::new(p.pose.x, p.pose.y, p.pose.z)))
            .collect();
        let centers: Vec<DVec3> = players.iter().map(|p| p.1).collect();
        if let Some(t) = traffic.as_deref_mut() {
            t.lan_centers = centers.clone();
            // and what they see: no car may come or go there in front of them either
            // (the list is kept: no allocation a frame)
            t.lan_eyes.clear();
            t.lan_eyes.extend(lan.peers().filter(|p| p.has_pose && p.pose.has_vehicle()).map(|p| {
                let at = DVec3::new(p.pose.x, p.pose.y, p.pose.z);
                crate::traffic::Viewer::lan_player(at, p.pose.heading as f64)
            }));
        }
        if let Some(h) = humans.as_deref_mut() {
            h.lan_centers = centers;
        }
        // waiting people a client's bus takes
        for (id, ids) in lan.take_claims() {
            let granted = humans
                .as_deref_mut()
                .map(|h| h.hand_over(id, &ids))
                .unwrap_or_default();
            let denied: Vec<u32> = ids.iter().copied().filter(|i| !granted.contains(i)).collect();
            if !granted.is_empty() || !denied.is_empty() {
                log::info!(
                    "LAN: player {id}'s bus takes {} waiting passenger(s) {:?}{}",
                    granted.len(),
                    granted,
                    if denied.is_empty() {
                        String::new()
                    } else {
                        format!("; {denied:?} have boarded another bus")
                    }
                );
            }
            lan.answer_claim(id, &granted, &denied);
            if let Some(v) = self.views.get_mut(&id) {
                for g in &granted {
                    v.sent.remove(&(true, *g));
                }
            }
        }
        // descriptions asked for again (lost)
        let wants = lan.take_wants();
        let t_ref = traffic.as_deref();
        let h_ref = humans.as_deref();
        for (id, refs) in wants {
            for r in refs {
                if let Some(d) = describe(h_ref, r) {
                    lan.send_desc(id, &d);
                }
            }
        }
        self.views.retain(|id, _| players.iter().any(|p| p.0 == *id));
        let players_at = players.clone();
        let lan_me_pos = me;
        for (id, at) in players {
            let view = self.views.entry(id).or_default();
            view.acc += dt;
            view.lights_acc += dt;
            for s in view.sent.values_mut() {
                s.1 += dt;
            }
            if view.acc + 1.0e-4 < nw::MOVING_EVERY {
                continue;
            }
            view.acc = (view.acc - nw::MOVING_EVERY).clamp(0.0, nw::MOVING_EVERY);
            let mut g = Gather::default();
            if let Some(t) = t_ref {
                // (the cars themselves go by the table, below; what their scripts need of the
                // host's timetable goes by the car's id there)
                let mut cars = std::collections::HashSet::new();
                for c in &t.cars {
                    if (c.vehicle.position - at).truncate().length() > CAR_RADIUS { continue; }
                    let Some(tid) = t.table.id_of(c.id) else { continue };
                    cars.insert(tid.raw());
                    let sent = view.context_pages.entry(tid.raw()).or_default();
                    if !sent.due(MOVING_EVERY_S, c.is_bus()) { continue; }
                    if let Some(packet) = ai_context(&c.vehicle.host).packet(omsi_net::PROTOCOL as u8, tid.raw(), lan.stamp_ms(), sent.page) {
                        lan.send_ai_context(id, &packet);
                        sent.page = sent.page.wrapping_add(1);
                        sent.sent += 1;
                        sent.age = 0.0;
                    }
                }
                view.context_pages.retain(|k, _| cars.contains(k));
            }
            if let Some(h) = h_ref {
                gather_people(view, h, t_ref.map(|t| &t.table), at, &mut g);
            }
            let Gather { mut frame, mut seen, mut descs, mut ids_hash } = g;
            // the riders of our own bus, and the other players' people and riders: everybody
            // sees everybody's passengers
            let mut others: Vec<(PersonState, String)> = Vec::new();
            if let Some(h) = h_ref {
                for p in h.lan_riders() {
                    let Some((_, l, lh, seat)) = p.aboard else { continue };
                    if p.id > nw::MAX_ID || (lan_me_pos.map(|m| (m - at).truncate().length() > CAR_RADIUS).unwrap_or(true)) {
                        continue;
                    }
                    others.push((
                        PersonState {
                            id: p.id,
                            activity: net_activity(p.activity),
                            place: PersonPlace::Aboard {
                                bus: nw::PLAYER_BUS | lan.my_id,
                                x: l.x,
                                y: l.y,
                                z: l.z,
                                heading: lh as f32,
                                seat: seat.map(|s| s.min(254) as u8),
                            },
                        },
                        Humans::type_file(&p.ty),
                    ));
                }
            }
            for (peer, up) in &self.ups {
                if *peer == id {
                    continue;
                }
                let bus_near = players_at.iter().any(|(pp, pos)| pp == peer && (*pos - at).truncate().length() < CAR_RADIUS);
                for (pid, track) in &up.tracks {
                    let (Some(local), Some(last), Some(file)) = (up.ids.get(pid), track.samples.last(), up.files.get(pid)) else {
                        continue;
                    };
                    let near = match last.v.place {
                        PersonPlace::Foot { x, y, .. } => (DVec2::new(x, y) - at.truncate()).length() < PERSON_RADIUS,
                        PersonPlace::Aboard { .. } => bus_near,
                    };
                    if !near {
                        continue;
                    }
                    let mut st = last.v;
                    st.id = RELAYED | ((local - (1 << 29)) & 0x3F_FFFF);
                    if let PersonPlace::Foot { ref mut waiting, .. } = st.place {
                        *waiting = None;
                    }
                    others.push((st, file.clone()));
                }
            }
            for (st, file) in others {
                let key = (true, st.id);
                seen.insert(key);
                ids_hash ^= fnv(&(st.id as u64 | 1 << 40).to_le_bytes());
                if !view.described.contains_key(&key) {
                    view.described.insert(key, 0);
                    descs.push(Desc::Person { id: st.id, file });
                }
                let q = person_q(&st);
                let due = match view.sent.get(&key) {
                    None => true,
                    Some((last, age)) => *last != q || *age >= nw::STANDING_EVERY,
                };
                if !due {
                    continue;
                }
                view.sent.insert(key, (q, 0.0));
                frame.people.push(st);
            }
            if view.lights_acc >= nw::LIGHTS_EVERY {
                view.lights_acc = 0.0;
                let keys: Vec<u32> = self.departed.iter().filter_map(|k| u32::try_from(*k).ok()).collect();
                frame.parked = Some((keys.len() == self.departed.len(), keys));
                if let Some(t) = t_ref {
                    frame.lights = t
                        .light_states(at, LIGHT_RADIUS)
                        .into_iter()
                        .filter(|(o, _, _)| (0..=u32::MAX as i64).contains(o))
                        .map(|(object, time, held)| LightState { object, time, held })
                        .collect();
                }
            }
            // what went out of sight (or away) since the last frame
            let gone: Vec<Key> = view.sent.keys().filter(|k| !seen.contains(*k)).copied().collect();
            for k in gone {
                view.sent.remove(&k);
                view.gone.push((k, 1.0));
            }
            for g in view.gone.iter_mut() {
                g.1 -= nw::MOVING_EVERY;
            }
            view.gone.retain(|g| g.1 > 0.0 && !seen.contains(&g.0));
            frame.gone = view.gone.iter().map(|(k, _)| *k).collect();
            view.last_counts = (seen.iter().filter(|k| k.0).count(), frame.lights.len());
            view.ids_hash = ids_hash;
            for d in &descs {
                lan.send_desc(id, d);
            }
            if let Some(f) = self.trace.as_mut() {
                let ms = lan.stamp_ms();
                for s in t_ref.iter().flat_map(|t| t.table.iter()).filter(|s| s.row.distance_to([at.x, at.y, at.z]) <= CAR_RADIUS) {
                    let [x, y, z] = s.row.position();
                    let _ = writeln!(f, "host,{ms},{id},c,{},{x:.3},{y:.3},{z:.3},{:.2}", s.id.raw(), s.row.heading_deg());
                }
                for p in &frame.people {
                    if let PersonPlace::Foot { x, y, z, heading, .. } = p.place {
                        let _ = writeln!(f, "host,{ms},{id},p,{},{x:.3},{y:.3},{z:.3},{heading:.2}", p.id);
                    }
                }
            }
            let n = lan.send_world(id, &frame);
            view.bytes += n as u64;
        }
        // the cars: the table, to each player around where it is
        if let Some(t) = t_ref {
            let viewers: Vec<(u32, [f64; 3])> = players_at.iter().map(|(id, p)| (*id, [p.x, p.y, p.z])).collect();
            let n = lan.traffic_publish(&t.table, &viewers, dt, TRAFFIC_BYTES_PER_SECOND);
            self.traffic_bytes += n as u64;
        }
        self.log_t -= dt;
        if self.log_t <= 0.0 && omsi_cfg::flags::OMSI_DEBUG_LAN.is_set() {
            self.log_t = 4.0;
            for (peer, up) in &self.ups {
                let aboard = up.tracks.values().filter(|t| t.samples.last().map(|s| matches!(s.v.place, PersonPlace::Aboard { .. })).unwrap_or(false)).count();
                log::info!("LAN people of player {peer}: {} heard ({aboard} riding its bus), {} drawn here", up.tracks.len(), up.ids.len());
            }
            for (id, v) in &self.views {
                log::info!(
                    "LAN world → player {id}: {} people, {} light programs in range; ids {:016X}; {:.1} KB/s ({} KB in all)",
                    v.last_counts.0,
                    v.last_counts.1,
                    v.ids_hash,
                    0.0f32.max(v.bytes as f32 / 1024.0 / lan_seconds(lan)),
                    v.bytes / 1024
                );
            }
            let st = lan.traffic_stats();
            log::info!(
                "LAN traffic table: {} cars in the table, {} told to the players; {} datagrams, {:.1} KB/s ({} KB in all), rows {} whole / {} changes, {} deferred by the budget, {} answers",
                t_ref.map(|t| t.table.len()).unwrap_or(0),
                st.tracked,
                st.datagrams,
                self.traffic_bytes as f32 / 1024.0 / lan_seconds(lan),
                self.traffic_bytes / 1024,
                st.full_rows,
                st.delta_rows,
                st.deferred,
                st.acks_heard
            );
        }
        if let Some(f) = self.trace.as_mut() {
            let _ = f.flush();
        }
    }

    // -----------------------------------------------------------------------------------
    // client

    #[allow(clippy::too_many_arguments)]
    fn client(
        &mut self,
        lan: &mut LanSession,
        dt: f32,
        args: &LanConfig,
        world: Option<&World>,
        renderer: Option<&Renderer>,
        scene: Option<&mut Scene>,
        mut traffic: Option<&mut Traffic>,
        mut humans: Option<&mut Humans>,
        view: &mut SimView,
    ) {
        let (Some(world), Some(renderer), Some(scene)) = (world, renderer, scene) else {
            return;
        };
        let m = &mut self.mirror;
        if !m.switch(lan, world, renderer, scene, traffic.as_deref_mut(), humans.as_deref_mut(), view) {
            return;
        }
        let now = m.now_ms();
        m.take_frames(lan, dt, world, renderer, scene, traffic.as_deref_mut());
        let render_ms = m.play.step(now, now + m.offset.unwrap_or(0.0) - INTERP_DELAY, 500.0);
        m.ask_descriptions(lan);
        m.take_loaded_types();
        // cars: the host's table (the events are the table's own bookkeeping: the cars gone,
        // new and changed are told by comparing it with what is drawn)
        let _ = lan.traffic_client_mut().take_events();
        let table = lan.traffic_client();
        // (a car's context may come before its row: kept a few seconds without one)
        m.context_heard.retain(|id, t| t.elapsed().as_secs_f32() < 5.0 || TrafficId::from_raw(*id).is_some_and(|i| table.get(i).is_some()));
        let context_ids: Vec<u32> = m.context_heard.keys().copied().collect();
        m.context_receiver.retain(&context_ids);
        m.contexts.retain(|id, _| m.context_heard.contains_key(id));
        if let Some(t) = traffic.as_deref_mut() {
            m.add_and_remove_cars(table, t, view, args, world, renderer, scene);
            let shown = m.place_cars(table, t, dt, render_ms);
            m.show_destinations(table, t, shown, args, world);
        }
        // people
        let forget = std::time::Duration::from_secs_f32(nw::FORGET_AFTER);
        m.people.retain(|_, t| t.heard.elapsed() < forget);
        if let Some(h) = humans.as_deref_mut() {
            m.update_people(lan, h, view, world, renderer, scene, render_ms);
        }
        if let Some(f) = self.trace.as_mut() {
            self.trace_t -= dt;
            if self.trace_t <= 0.0 {
                self.trace_t = 0.1;
                if let Some(t) = traffic.as_deref() {
                    for c in t.cars.iter().filter(|c| m.drawn_cars.contains(&(c.id as u32))) {
                        let p = c.vehicle.position;
                        let _ = writeln!(f, "client,{render_ms:.0},{},c,{},{:.3},{:.3},{:.3},{:.2}", lan.my_id, c.id, p.x, p.y, p.z, c.vehicle.heading);
                    }
                }
                if let Some(h) = humans.as_deref() {
                    for (id, pos) in h.mirror_positions() {
                        let _ = writeln!(f, "client,{render_ms:.0},{},p,{id},{:.3},{:.3},{:.3},0", lan.my_id, pos.x, pos.y, pos.z);
                    }
                }
                let _ = f.flush();
            }
        }
        self.log_t -= dt;
        if self.log_t <= 0.0 && omsi_cfg::flags::OMSI_DEBUG_LAN.is_set() {
            self.log_t = 4.0;
            let m = &mut self.mirror;
            let table = lan.traffic_client();
            let ids_hash = table
                .ids()
                .map(|id| fnv(&(id.raw() as u64).to_le_bytes()))
                .chain(m.people.keys().map(|id| fnv(&(*id as u64 | 1 << 40).to_le_bytes())))
                .fold(0u64, |a, b| a ^ b);
            let rate = (lan.world_received - m.bytes_at) as f32 / 4.0 / 1024.0;
            m.bytes_at = lan.world_received;
            log::info!(
                "LAN world from the host: {} cars ({} drawn; {} waiting for a context, {} for a type, {} left out), {} people ({} drawn), ids {:016X}; {:.1} KB/s; drawn {:.0} ms behind the host; {} passengers handed to us, {} not",
                table.len(),
                m.drawn_cars.len(),
                m.waiting.0,
                m.waiting.1,
                m.waiting.2,
                m.people.len(),
                m.drawn_people.len(),
                ids_hash,
                rate,
                INTERP_DELAY,
                m.granted,
                m.denied
            );
        }
    }
}

/// A client's side of the frame (`LanWorld::client`), step by step.
impl Mirror {
    /// The host's world while we are in its session; our own again once it is over. Whether
    /// the host's world is drawn.
    #[allow(clippy::too_many_arguments)]
    fn switch(
        &mut self,
        lan: &mut LanSession,
        world: &World,
        renderer: &Renderer,
        scene: &mut Scene,
        mut traffic: Option<&mut Traffic>,
        mut humans: Option<&mut Humans>,
        view: &mut SimView,
    ) -> bool {
        let m = self;
        let want_on = lan.welcome.is_some() && lan.rejected.is_none();
        if want_on != m.on {
            m.on = want_on;
            if let Some(t) = traffic.as_deref_mut() {
                t.set_mirror(want_on);
                crate::view_sync::apply_traffic(t, &mut view.traffic, world, renderer, scene);
            }
            if let Some(h) = humans.as_deref_mut() {
                h.set_mirror(want_on);
            }
            // (the table is cleared when the session is over only: the host's first datagrams may
            // already be in it when the session turns on)
            if !want_on {
                lan.traffic_client_mut().clear();
            }
            m.people.clear();
            m.drawn_cars.clear();
            m.drawn_people.clear();
            m.shown.clear();
            m.odometer.clear();
            m.contexts.clear();
            m.context_heard.clear();
            m.context_receiver = Default::default();
            log::info!(
                "LAN: {}",
                if want_on {
                    "drawing the host's traffic and people"
                } else {
                    "the session is over: simulating our own traffic and people again"
                }
            );
        }
        m.on
    }

    /// The descriptions and world frames that came from the host.
    fn take_frames(
        &mut self,
        lan: &mut LanSession,
        dt: f32,
        world: &World,
        renderer: &Renderer,
        scene: &mut Scene,
        mut traffic: Option<&mut Traffic>,
    ) {
        let m = self;
        for d in lan.take_descs() {
            let key = match &d {
                Desc::Person { id, .. } => (true, *id),
            };
            m.wanted.remove(&key);
            m.descs.insert(key, d);
        }
        let mut contexts_new = HashSet::new();
        for packet in lan.take_ai_contexts() {
            if let Some((id, context)) = m.context_receiver.receive(&packet, omsi_net::PROTOCOL as u8) {
                m.context_heard.insert(id, Instant::now());
                if m.contexts.get(&id) != Some(&context) {
                    contexts_new.insert(id);
                    m.contexts.insert(id, context);
                }
            }
        }
        if let Some(t) = traffic.as_deref_mut() {
            for car in &mut t.cars {
                if contexts_new.contains(&(car.id as u32)) {
                    if let Some(context) = m.contexts.get(&(car.id as u32)) { apply_ai_context(&mut car.vehicle.host, context); }
                }
            }
        }
        for (at, f) in lan.take_world() {
            // the host's clock against ours: the frame that took the shortest way sets it
            let arrived = at
                .checked_duration_since(*m.epoch.get_or_insert(at))
                .map(|d| d.as_secs_f64() * 1000.0)
                .unwrap_or(0.0);
            let off = f.host_ms as f64 - arrived;
            m.offset = Some(match m.offset {
                // (and drifts down slowly, so that a clock that runs a little slower is
                // followed; a jump of more than a second is taken at once)
                Some(o) if (off - o).abs() < 1000.0 => off.max(o - dt as f64 * 2.0),
                _ => off,
            });
            let ms = f.host_ms as f64;
            for p in f.people {
                m.people
                    .entry(p.id)
                    .or_insert_with(|| Track {
                        samples: Vec::new(),
                        heard: Instant::now(),
                    })
                    .push(ms, p);
            }
            if let Some(t) = traffic.as_deref_mut() {
                for l in &f.lights {
                    // (the light programs ran on at the host while the datagram travelled)
                    let late = ((m.offset.unwrap_or(off) + arrived) - ms).max(0.0) / 1000.0;
                    t.set_light_state(l.object, l.time + if l.held { 0.0 } else { late }, l.held);
                }
            }
            if let Some((complete, keys)) = &f.parked {
                let keys: Vec<i64> = keys.iter().map(|k| *k as i64).collect();
                world.mirror_departed(renderer, scene, &keys, *complete);
            }
            for (person, id) in f.gone {
                if person {
                    m.people.remove(&id);
                }
            }
        }
    }

    /// Descriptions still missing: asked for (again).
    fn ask_descriptions(&mut self, lan: &mut LanSession) {
        let m = self;
        let mut want: Vec<EntityRef> = Vec::new();
        for (person, ids) in [(true, m.people.keys().copied().collect::<Vec<_>>())] {
            for id in ids {
                let key = (person, id);
                if m.descs.contains_key(&key) {
                    continue;
                }
                let again = m
                    .wanted
                    .get(&key)
                    .map(|t| t.elapsed().as_secs_f32() > WANT_AGAIN)
                    .unwrap_or(true);
                // (the host sends a description with the first frame: ask only when that
                // was lost)
                let first_heard = m.people.get(&id).map(|t| t.samples.len());
                if again && first_heard.unwrap_or(0) > 2 {
                    m.wanted.insert(key, Instant::now());
                    want.push(EntityRef { person, id });
                }
            }
        }
        if !want.is_empty() {
            lan.want(&want);
        }
    }

    /// Vehicle types loaded in the background.
    fn take_loaded_types(&mut self) {
        let m = self;
        if m.loaded_tx.is_none() {
            let (tx, rx) = mpsc::channel();
            m.loaded_tx = Some(tx);
            m.loaded_rx = Some(rx);
        }
        if let Some(rx) = m.loaded_rx.as_ref() {
            while let Ok((path, ty)) = rx.try_recv() {
                m.loading.remove(&path);
                if ty.is_none() {
                    log::warn!("LAN: the host's vehicle {} cannot be loaded here", path.display());
                }
                m.types.insert(path, ty);
            }
        }
    }

    /// The host's cars that are gone taken away, the new ones made (their types loaded in
    /// the background first).
    #[allow(clippy::too_many_arguments)]
    fn add_and_remove_cars(&mut self, table: &ClientTable, t: &mut Traffic, view: &mut SimView, args: &LanConfig, world: &World, renderer: &Renderer, scene: &mut Scene) {
        let m = self;
        let gone: Vec<u32> = m
            .drawn_cars
            .iter()
            .copied()
            .filter(|raw| !TrafficId::from_raw(*raw).is_some_and(|id| table.get(id).is_some()))
            .collect();
        for id in gone {
            t.remove_car(id as u64);
            crate::view_sync::apply_traffic(t, &mut view.traffic, world, renderer, scene);
            m.drawn_cars.remove(&id);
            m.shown.remove(&id);
            m.odometer.remove(&id);
        }
        let new: Vec<TrafficId> = table.ids().filter(|id| !m.drawn_cars.contains(&id.raw())).collect();
        m.waiting = (0, 0, 0);
        for tid in new {
            let id = tid.raw();
            let Some(row) = table.get(tid) else { continue };
            let Some(look) = table.look(row) else {
                m.waiting.2 += 1;
                continue;
            };
            let (file, scheme, paint) = (look.file.clone(), look.scheme, look.paint.clone());
            // Never run init with an incomplete host timetable.
            let Some(context) = m.contexts.get(&id).cloned() else {
                m.waiting.0 += 1;
                continue;
            };
            let Some(path) = local_file(&args.root, &file) else {
                if m.types.insert(PathBuf::from(&file), None).is_none() {
                    log::warn!("LAN: the host's vehicle {file} is not installed here; it is left out");
                }
                m.waiting.2 += 1;
                continue;
            };
            let ty = match m.types.get(&path) {
                Some(Some(ty)) => ty.clone(),
                Some(None) => {
                    m.waiting.2 += 1;
                    continue;
                }
                None => match t.loaded_type(&path) {
                    Some(ty) => {
                        m.types.insert(path.clone(), Some(ty.clone()));
                        ty
                    }
                    None => {
                        if m.loading.insert(path.clone()) {
                            let tx = m.loaded_tx.clone().unwrap();
                            let root = args.root.clone();
                            std::thread::spawn(move || {
                                let ty = VehicleType::load_ai(&root, &path).ok().map(Arc::new);
                                let _ = tx.send((path, ty));
                            });
                        }
                        m.waiting.1 += 1;
                        continue;
                    }
                },
            };
            let Some(first) = table.newest(tid) else {
                m.waiting.2 += 1;
                continue;
            };
            // the repaint by its name: the index counts the host's repaints, and this game
            // may have others installed (#2018); one it does not have is not guessed at
            let scheme = if paint.is_empty() {
                scheme.map(|s| s as usize)
            } else {
                ty.paint_schemes.iter().position(|p| p.name.trim().eq_ignore_ascii_case(&paint))
            };
            t.add_mirror_car(
                world,
                id as u64,
                ty.clone(),
                scheme,
                &context,
                m.hofs.entry(ty.def.path.clone()).or_insert_with(|| crate::find_hof_named(args.hof.as_deref(), world, &ty)).clone(),
                DVec3::new(first.pos[0], first.pos[1], first.pos[2]),
                first.heading,
            );
            crate::view_sync::apply_traffic(t, &mut view.traffic, world, renderer, scene);
            m.drawn_cars.insert(id);
        }
    }

    /// Where each car is now (between the host's poses at `render_ms`), and what its
    /// scripts make of it. The cars placed, with their ids.
    fn place_cars(&mut self, table: &ClientTable, t: &mut Traffic, dt: f32, render_ms: f64) -> Vec<(usize, u32)> {
        let m = self;
        // where each is now, and what its scripts make of it
        let index: HashMap<u64, usize> =
            t.cars.iter().enumerate().map(|(i, c)| (c.id, i)).collect();
        let mut work: Vec<(usize, u32, omsi_net::traffic_table::Pose)> = Vec::new();
        for tid in table.ids() {
            let Some(&i) = index.get(&(tid.raw() as u64)) else {
                continue;
            };
            let Some(pose) = table.sample(tid, render_ms) else {
                continue;
            };
            work.push((i, tid.raw(), pose));
        }
        let mut shown: Vec<(usize, u32)> = Vec::new();
        for (i, id, c) in &work {
            let car = &mut t.cars[*i];
            let before = car.vehicle.position;
            let now_p = DVec3::new(c.pos[0], c.pos[1], c.pos[2]);
            let moved = (now_p - before).truncate().length() as f32;
            let odo = m.odometer.entry(*id).or_insert(0.0);
            if moved < 30.0 {
                *odo += moved * (c.speed as f32).signum();
            }
            car.vehicle.position = now_p;
            car.vehicle.heading = c.heading;
            car.vehicle.pitch = c.pitch as f32;
            car.vehicle.bank = c.bank as f32;
            car.state.speed = c.speed as f32;
            car.state.blinker = c.blinker as i32;
            car.state.braking = c.brake;
            if let Some(b) = car.bus.as_mut() {
                b.phase = if c.at_station == 1 {
                    crate::bus_service::Phase::Boarding
                } else {
                    crate::bus_service::Phase::Running
                };
            }
            car.body.steer = c.steer as f32;
            shown.push((*i, *id));
        }
        // the scripts (lamps, indicators, doors, wheels) run in parallel, as the
        // traffic's own do
        {
            use rayon::prelude::*;
            let odometer = &m.odometer;
            // (read off the buses before the parallel block takes `t.cars` mutably:
            // which side each stands at its stop is the host's own timetable state)
            let sides: HashMap<usize, f32> =
                work.iter().map(|(i, _, _)| (*i, t.cars[*i].at_station_side())).collect();
            let frames: HashMap<usize, AiFrame> = work
                .iter()
                .map(|(i, id, c)| {
                    (
                        *i,
                        AiFrame {
                            speed: c.speed as f32,
                            odometer: odometer.get(id).copied().unwrap_or(0.0),
                            steer_deg: c.steer as f32,
                            blinker: c.blinker as i32,
                            brake: c.brake,
                            lights: c.lights,
                            at_station: c.at_station as i32,
                            at_station_side: sides.get(i).copied().unwrap_or(0.0),
                            priority_warning: false,
                            // (the host's buses' engines are not on the wire: running)
                            engine_off: false,
                        },
                    )
                })
                .collect();
            let mut run: Vec<(&mut omsi_sim::VehicleInstance, &AiFrame)> = t
                .cars
                .iter_mut()
                .enumerate()
                .filter_map(|(i, car)| frames.get(&i).map(|f| (&mut car.vehicle, f)))
                .collect();
            run.par_iter_mut().for_each(|(v, f)| v.update_ai(dt, f));
        }
        shown
    }

    /// The timetable buses' displays.
    fn show_destinations(&mut self, table: &ClientTable, t: &mut Traffic, shown: Vec<(usize, u32)>, args: &LanConfig, world: &World) {
        let m = self;
        for (i, id) in shown {
            let Some(row) = TrafficId::from_raw(id).and_then(|tid| table.get(tid)) else {
                continue;
            };
            let (line, destination) = (&row.meta.line, &row.meta.destination);
            if line.is_empty() || !destination.starts_with('#') || destination == "#-1" {
                continue;
            }
            let want = (line.clone(), destination.clone());
            if m.shown.get(&id) == Some(&want) {
                continue;
            }
            let car = &mut t.cars[i];
            let path = car.vehicle.ty.def.path.clone();
            let hof = m
                .hofs
                .entry(path)
                .or_insert_with(|| crate::find_hof_named(args.hof.as_deref(), world, &car.vehicle.ty))
                .clone();
            if let Some(k) = car.vehicle.ty.program.str_var("Linie") {
                car.vehicle.state.str_vars[k as usize] = line.clone();
            }
            show_car_destination(&mut car.vehicle, hof.as_deref(), line, destination);
            m.shown.insert(id, want);
        }

    }

    /// The host's people: its answers to our claims, those gone, those drawn, and our claims
    /// for the people waiting where our bus stands.
    #[allow(clippy::too_many_arguments)]
    fn update_people(&mut self, lan: &mut LanSession, h: &mut Humans, view: &mut SimView, world: &World, renderer: &Renderer, scene: &mut Scene, render_ms: f64) {
        let m = self;
        // the host's answers to our claims
        for (ids, granted) in lan.take_grants() {
            for id in ids {
                if granted {
                    if h.grant(id) {
                        m.granted += 1;
                        log::info!("LAN: the host hands waiting passenger {id} over to our bus");
                    } else {
                        // the host has let them go (they are ours now, and the host
                        // says no more of them): a copy kept here would stand at the
                        // stop for good, one more after every stop
                        h.mirror_remove(id);
                    }
                    m.people.remove(&id);
                    m.drawn_people.remove(&id);
                } else {
                    m.denied += 1;
                }
            }
        }
        let gone: Vec<u32> = m
            .drawn_people
            .iter()
            .copied()
            .filter(|id| !m.people.contains_key(id))
            .collect();
        for id in gone {
            h.mirror_remove(id);
            m.drawn_people.remove(&id);
        }
        for (id, track) in &m.people {
            let Some((a, b, k)) = track.around(render_ms) else {
                continue;
            };
            let pose = person_pose(a, b, k);
            if m.drawn_people.contains(id) {
                if h.has_mirror(*id) {
                    h.mirror_set(*id, &pose);
                    continue;
                }
                // (taken away here with a bus that went: drawn again once it is back)
                m.drawn_people.remove(id);
            }
            // a rider only once the bus is here to sit in
            if pose.aboard.map(|a| !h.knows_bus(a.0)).unwrap_or(false) {
                continue;
            }
            let Some(Desc::Person { file, .. }) = m.descs.get(&(true, *id)) else {
                continue;
            };
            let Some(ty) = h.type_by_file(file) else {
                continue;
            };
            let added = h.mirror_add(world, *id, ty, &pose);
            crate::view_sync::apply_people(h, &mut view.people, world, renderer, scene);
            if added {
                m.drawn_people.insert(*id);
            }
        }
        let claims = h.take_claims();
        if !claims.is_empty() {
            log::info!("LAN: our bus stands at a stop: asking the host for waiting passengers {claims:?}");
            lan.claim(&claims);
        }

    }
}

/// What one frame of the host's world for a client gathers: the frame, the things in it (in
/// sight or not sent again), the descriptions to send first, a checksum of the ids.
#[derive(Default)]
struct Gather {
    frame: WorldFrame,
    seen: HashSet<Key>,
    descs: Vec<Desc>,
    ids_hash: u64,
}

/// The host's people within `PERSON_RADIUS` of a client at `at`: those moved (or standing too
/// long unsent) go into the frame, new descriptions are sent along. A rider of a timetable bus
/// names the bus by its id in the traffic table (`table`); one whose bus has no row is left
/// out until it has.
fn gather_people(view: &mut View, h: &Humans, table: Option<&omsi_net::traffic_table::TrafficTable>, at: DVec3, g: &mut Gather) {
    for p in h.lan_people(at, PERSON_RADIUS) {
        if p.id > nw::MAX_ID {
            continue;
        }
        let key = (true, p.id);
        g.seen.insert(key);
        g.ids_hash ^= fnv(&(p.id as u64 | 1 << 40).to_le_bytes());
        if !view.described.contains_key(&key) {
            view.described.insert(key, 0);
            g.descs.push(Desc::Person {
                id: p.id,
                file: Humans::type_file(&p.ty),
            });
        }
        let (place, q) = match p.aboard {
            Some((bus, l, lh, seat)) => (
                PersonPlace::Aboard {
                    bus: match table.and_then(|t| t.id_of(bus)) {
                        Some(id) => id.raw(),
                        None => continue,
                    },
                    x: l.x,
                    y: l.y,
                    z: l.z,
                    heading: lh as f32,
                    seat: seat.map(|s| s.min(254) as u8),
                },
                quant(l.x as f64, l.y as f64, l.z as f64, lh + bus as f64 * 1000.0),
            ),
            None => (
                PersonPlace::Foot {
                    x: p.pos.x,
                    y: p.pos.y,
                    z: p.pos.z,
                    heading: p.heading as f32,
                    speed: p.speed as f32,
                    waiting: p.waiting.map(|(s, k)| (s, k.min(255) as u8)),
                },
                quant(p.pos.x, p.pos.y, p.pos.z, p.heading),
            ),
        };
        let q = [q[0], q[1], q[2], q[3] ^ ((p.activity as i64) << 40) ^ (p.waiting.is_some() as i64) << 44];
        let due = match view.sent.get(&key) {
            None => true,
            Some((last, age)) => *last != q || *age >= nw::STANDING_EVERY,
        };
        if !due {
            continue;
        }
        view.sent.insert(key, (q, 0.0));
        g.frame.people.push(PersonState {
            id: p.id,
            activity: net_activity(p.activity),
            place,
        });
    }

}

/// A person of one client passed on to the others has this bit in its id there (the host's
/// own people are numbered far below it).
const RELAYED: u32 = 0xC0_0000;

/// What decides whether a person's pose is sent again (see `quant`).
fn person_q(p: &PersonState) -> [i64; 4] {
    let q = match p.place {
        PersonPlace::Foot { x, y, z, heading, .. } => quant(x, y, z, heading as f64),
        PersonPlace::Aboard { bus, x, y, z, heading, .. } => {
            quant(x as f64, y as f64, z as f64, heading as f64 + (bus & 0xFFFF) as f64 * 1000.0)
        }
    };
    let act = match p.activity {
        NetActivity::Stand => 0,
        NetActivity::Walk => 1,
        NetActivity::Sit => 2,
        NetActivity::Run => 3,
    };
    [q[0], q[1], q[2], q[3] ^ (act << 40)]
}

fn lan_seconds(lan: &LanSession) -> f32 {
    (lan.world_ms() as f32 / 1000.0).max(1.0)
}

/// Where a person is at `k` between two of the host's poses.
fn person_pose(a: &PersonState, b: &PersonState, k: f64) -> MirrorPose {
    match (a.place, b.place) {
        (
            PersonPlace::Foot {
                x: ax,
                y: ay,
                z: az,
                heading: ah,
                ..
            },
            PersonPlace::Foot {
                x,
                y,
                z,
                heading,
                speed,
                waiting,
            },
        ) => {
            let jump = DVec2::new(x - ax, y - ay).length() > 8.0;
            let k = if jump { 1.0 } else if speed < 0.05 { k.min(1.0) } else { k };
            let pos = DVec3::new(ax + (x - ax) * k, ay + (y - ay) * k, az + (z - az) * k.min(1.0));
            let h = lerp_angle(ah as f64, heading as f64, k.min(1.0));
            let dir = DVec2::new(x - ax, y - ay);
            let vel = if speed > 0.05 && dir.length() > 1.0e-3 {
                dir.normalize() * speed as f64
            } else {
                DVec2::ZERO
            };
            MirrorPose {
                pos,
                heading: h,
                vel,
                activity: sim_activity(b.activity),
                aboard: None,
                waiting: waiting.map(|(s, k)| (s, k as usize)),
            }
        }
        (
            _,
            PersonPlace::Aboard {
                bus,
                x,
                y,
                z,
                heading,
                seat,
            },
        ) => MirrorPose {
            pos: DVec3::ZERO,
            heading: 0.0,
            vel: DVec2::ZERO,
            activity: sim_activity(b.activity),
            aboard: Some((
                if bus & nw::PLAYER_BUS != 0 {
                    crate::humans::remote_bus_id(bus & nw::MAX_ID)
                } else {
                    bus as u64
                },
                Vec3::new(x, y, z),
                heading as f64,
                seat.map(|s| s as usize),
            )),
            waiting: None,
        },
        (_, PersonPlace::Foot { .. }) => person_pose(b, b, 1.0),
    }
}

/// What a person asked for is (the cars are told with their rows: `traffic_table`).
fn describe(humans: Option<&Humans>, r: EntityRef) -> Option<Desc> {
    if !r.person {
        return None;
    }
    let p = humans?.lan_people_by_id(r.id)?;
    Some(Desc::Person {
        id: r.id,
        file: Humans::type_file(&p),
    })
}

#[cfg(test)]
mod tests;

mod display;
pub(crate) use display::car_display;
use display::show_car_destination;

mod lifecycle;
mod context;
pub(crate) use context::apply_ai_context;
use context::ai_context;
