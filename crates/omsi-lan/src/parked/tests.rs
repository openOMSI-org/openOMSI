use super::*;
use omsi_net::{Walker, WorldInfo, FLAG_VEHICLE};

fn fp(c: &str) -> String {
    format!("v2-{}", c.repeat(32))
}

fn world() -> WorldInfo {
    WorldInfo { map: "maps/m/global.cfg".into(), date: "1989-05-30".into(), time: 9.0 * 3600.0, weather: String::new(), season: String::new() }
}

/// A player's bus `content` at `x`: each content another bus file, all in red.
fn bus(content: &str, x: f64) -> Pose {
    let file = format!("Vehicles/{0}/{0}.bus", &content[3..5]);
    Pose { bus: file, paint: "red".into(), content: content.into(), x, y: 2.0, z: 3.0, heading: 90.0, flags: FLAG_VEHICLE, length: 12.0, width: 2.5, ..Default::default() }
}

/// The same player out of its bus, standing at `x`.
fn on_foot(p: &Pose, x: f64) -> Pose {
    Pose { walker: Some(Walker { x, y: 2.0, z: 3.0, ..Default::default() }), ..p.clone() }
}

fn temp(name: &str) -> PathBuf {
    let file = std::env::temp_dir().join(format!("openomsi-parked-{name}-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&file);
    file
}

struct Room {
    host: LanSession,
    parked: Parked,
    file: PathBuf,
}

type Players<'a> = [(&'a mut LanSession, &'a Pose)];

impl Room {
    fn new(port: u16, name: &str) -> Self {
        let file = temp(name);
        let host = LanSession::host(port, "server", world(), true).unwrap();
        let parked = Parked::open_with(&file, "maps/m/global.cfg", 8, &[], |_| true);
        Room { host, parked, file }
    }

    fn join(&self, name: &str) -> LanSession {
        LanSession::join(&self.host.local_addr().unwrap().port().to_string(), name, world(), Duration::from_secs(1)).unwrap()
    }

    /// Frames of the server and the players (each showing its pose) until `done`, which is
    /// given what the players heard from the server so far.
    fn run(&mut self, players: &mut Players, done: impl Fn(&Self, &Players, &[String]) -> bool) {
        let mut heard = Vec::new();
        for _ in 0..600 {
            self.host.tick(0.06, &Pose::default());
            for (from, text) in self.host.take_commands() {
                self.parked.command(&mut self.host, from, &text);
            }
            self.parked.tick(&mut self.host, 0.06);
            for (s, pose) in players.iter_mut() {
                s.tick(0.06, pose);
                heard.extend(s.take_commands().into_iter().map(|c| c.1));
            }
            if done(self, players, &heard) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("it never came to that; heard {heard:?}, buses {:?}", self.parked.buses());
    }
}

fn sees(s: &LanSession, id: u32, x: f64) -> bool {
    s.peers().any(|p| p.pose.id == id && p.has_pose && p.has_info && (p.pose.x - x).abs() < 0.1)
}

#[test]
fn a_bus_stays_where_its_driver_left_it_and_the_next_player_takes_it_from_there() {
    let first = FIRST_PARKED_ID;
    let mut room = Room::new(28620, "takes");
    let (mut a, mut b) = (room.join("a"), room.join("b"));
    let (a_bus, b_bus) = (bus(&fp("a1"), 100.0), bus(&fp("b2"), 500.0));
    room.run(&mut [(&mut a, &a_bus), (&mut b, &b_bus)], |r, _, _| r.parked.seen.len() == 2);
    assert!(room.parked.buses().is_empty(), "a bus somebody drives is not parked");
    // A goes: its bus stands where it was, for B to see
    a.leave();
    drop(a);
    room.run(&mut [(&mut b, &b_bus)], |_, p, _| sees(p[0].0, first, 100.0));
    assert_eq!(room.parked.buses().len(), 1);
    // B asks from where its own bus stands, far from it: no
    b.command(1, &format!("park-take {first} 7"));
    room.run(&mut [(&mut b, &on_foot(&b_bus, 500.0))], |_, _, heard| heard.contains(&format!("park-deny {first} 7")));
    assert!(room.parked.buses()[0].driver.is_none());
    // B drove up to it and got out: yes - and the bus B came in stays where B left it
    let came = Pose { x: 120.0, ..b_bus.clone() };
    room.run(&mut [(&mut b, &on_foot(&came, 102.0))], |r, _, _| r.parked.seen.values().any(|p| p.x == 120.0));
    b.command(1, &format!("park-take {first} 7"));
    room.run(&mut [(&mut b, &on_foot(&came, 102.0))], |_, _, heard| heard.contains(&format!("park-grant {first} 7")));
    // the bus is kept for B and stands there still; the one B came in is B's until B sits in
    // the other - nobody else gets it meanwhile
    assert_eq!(room.parked.buses().len(), 1);
    assert!(sees(&b, first, 100.0));
    // asked again (the answer may have been lost on its way): the same yes
    b.command(1, &format!("park-take {first} 7"));
    room.run(&mut [(&mut b, &on_foot(&came, 102.0))], |_, _, heard| heard.contains(&format!("park-grant {first} 7")));
    // B's game drives A's bus from where it stood: nobody sees it parked any more, and only
    // now the bus B came in stands where B left it
    let taken = bus(&fp("a1"), 100.0);
    room.run(&mut [(&mut b, &taken)], |r, _, _| r.host.peers().any(|q| q.pose.content == fp("a1")));
    // (showing the bus is not yet having it: B's game says so, and hears that it is B's)
    assert!(!room.parked.buses()[0].driver.as_ref().unwrap().drives);
    b.command(1, &format!("park-took {first} 7"));
    room.run(&mut [(&mut b, &taken)], |r, p, heard| heard.contains(&format!("park-yours {first} 7")) && r.parked.buses()[0].driver.as_ref().is_some_and(|d| d.drives) && !sees(p[0].0, first, 100.0) && sees(p[0].0, first + 1, 120.0));
    assert_eq!(room.parked.buses().len(), 2);
    // (said again: the same yes)
    b.command(1, &format!("park-took {first} 7"));
    room.run(&mut [(&mut b, &taken)], |_, _, heard| heard.contains(&format!("park-yours {first} 7")));
    // B drives it away and goes: there it stands, also after the server started again
    let far = bus(&fp("a1"), 900.0);
    room.run(&mut [(&mut b, &far)], |r, _, _| r.parked.seen.values().any(|p| p.x == 900.0));
    b.leave();
    drop(b);
    room.run(&mut [], |r, _, _| r.parked.buses()[0].driver.is_none());
    room.parked.save();
    let again = Parked::open_with(&room.file, "maps/m/global.cfg", 8, &[], |_| true);
    assert_eq!(again.buses().iter().map(|b| (b.id, b.x)).collect::<Vec<_>>(), vec![(first, 900.0), (first + 1, 120.0)]);
    let _ = std::fs::remove_file(&room.file);
}

#[test]
fn only_buses_the_server_keeps_whole_are_parked_and_one_of_each() {
    let mut p = Parked::open_with(&temp("kept"), "m", 2, &[], |f| !f.contains("ee"));
    p.left(2, &bus(&fp("ee"), 1.0));
    assert!(p.buses().is_empty(), "a bus the server has not got");
    p.left(2, &bus(&fp("a1"), 1.0));
    p.left(3, &bus(&fp("a1"), 50.0));
    assert_eq!(p.buses().len(), 1, "the same bus twice");
    // another player's copy of that bus in that paint (a file of it differs: another
    // fingerprint) is the same bus for the players; in another paint it is another
    let copy = Pose { content: fp("f9"), ..bus(&fp("a1"), 70.0) };
    p.left(5, &Pose { bus: copy.bus.to_uppercase(), ..copy.clone() });
    assert_eq!(p.buses().len(), 1, "the same bus in the same paint twice");
    p.left(5, &Pose { paint: "blue".into(), ..copy });
    assert_eq!(p.buses().iter().map(|b| b.paint.as_str()).collect::<Vec<_>>(), vec!["red", "blue"]);
    p.buses.pop();
    p.left(3, &bus(&fp("b2"), 50.0));
    p.left(4, &bus(&fp("c3"), 90.0));
    assert_eq!(p.buses().iter().map(|b| b.id).collect::<Vec<_>>(), vec![FIRST_PARKED_ID, FIRST_PARKED_ID + 1], "no more than the server allows");
}

#[test]
fn on_another_map_the_buses_stand_at_its_entry_points() {
    let file = temp("map");
    let mut p = Parked::open_with(&file, "maps/old/global.cfg", 8, &[], |_| true);
    for (k, c) in ["a1", "b2", "c3"].iter().enumerate() {
        p.left(2 + k as u32, &bus(&fp(c), 7000.0 + k as f64));
    }
    p.save();
    // the same map: where they were; another: one at each entry point, the third beside the first
    let same = Parked::open_with(&file, "maps/old/global.cfg", 8, &[([0.0, 0.0, 0.0], 0.0)], |_| true);
    assert_eq!(same.buses()[2].x, 7002.0);
    let spots = [([10.0, 20.0, 1.0], 0.0), ([300.0, 40.0, 2.0], 90.0)];
    let other = Parked::open_with(&file, "maps/new/global.cfg", 8, &spots, |_| true);
    let at: Vec<(f64, f64, f32)> = other.buses().iter().map(|b| (b.x.round(), b.y.round(), b.heading)).collect();
    assert_eq!(at, vec![(10.0, 20.0, 0.0), (300.0, 40.0, 90.0), (15.0, 20.0, 0.0)]);
    let _ = std::fs::remove_file(&file);
}

#[test]
fn the_buses_the_server_keeps_already_stand_at_the_entry_points_the_first_time() {
    let file = temp("first");
    let spots = [([10.0, 20.0, 1.0], 0.0), ([300.0, 40.0, 2.0], 90.0)];
    let mut p = Parked::open_with(&file, "m", 3, &spots, |f| !f.contains("ee"));
    // one stands somewhere already: it stays, the others are put down after it
    p.left(2, &bus(&fp("a1"), 7000.0));
    let kept = [fp("a1"), fp("b2"), fp("ee"), fp("c3"), fp("d4")];
    p.park_kept(&kept, &spots, |f| (!f.contains("c3")).then(|| Pose { x: 0.0, ..bus(f, 0.0) }));
    let at: Vec<(String, f64, f64)> = p.buses().iter().map(|b| (b.fp[3..5].to_string(), b.x.round(), b.y.round())).collect();
    // (b2 at the second entry point, d4 beside the first: a1 keeps its place and its turn;
    // ee is not kept whole, c3 could not be read)
    assert_eq!(at, vec![("a1".to_string(), 7000.0, 2.0), ("b2".to_string(), 300.0, 40.0), ("d4".to_string(), 15.0, 20.0)]);
    // the next start finds them all standing: nothing is moved
    p.save();
    let mut again = Parked::open_with(&file, "m", 3, &spots, |_| true);
    again.park_kept(&kept, &spots, |f| Some(bus(f, 0.0)));
    assert_eq!(again.buses(), p.buses());
    let _ = std::fs::remove_file(&file);
}

#[test]
fn a_bus_asked_for_and_not_taken_is_free_again_and_the_player_keeps_its_own() {
    let first = FIRST_PARKED_ID;
    let mut room = Room::new(28621, "nottaken");
    let (mut a, mut b, mut c) = (room.join("a"), room.join("b"), room.join("c"));
    let (a_bus, b_bus, c_bus) = (bus(&fp("a1"), 100.0), bus(&fp("b2"), 120.0), bus(&fp("c3"), 110.0));
    room.run(&mut [(&mut a, &a_bus), (&mut b, &b_bus), (&mut c, &c_bus)], |r, _, _| r.parked.seen.len() == 3);
    a.leave();
    drop(a);
    room.run(&mut [(&mut b, &b_bus), (&mut c, &c_bus)], |_, p, _| sees(p[0].0, first, 100.0));
    // B asks and gets the yes, but its game never shows the bus (the answer lost, the bus not
    // loaded): C, who asks meanwhile, is told no - and nobody gets B's own bus either
    b.command(1, &format!("park-take {first} 7"));
    room.run(&mut [(&mut b, &on_foot(&b_bus, 102.0)), (&mut c, &c_bus)], |_, _, heard| heard.contains(&format!("park-grant {first} 7")));
    c.command(1, &format!("park-take {first} 7"));
    room.run(&mut [(&mut b, &on_foot(&b_bus, 102.0)), (&mut c, &on_foot(&c_bus, 101.0))], |_, _, heard| heard.contains(&format!("park-deny {first} 7")));
    assert_eq!(room.parked.buses().len(), 1, "B's own bus is still B's");
    // the time to take it runs out: free again, B drives its own as before
    room.parked.buses[0].driver.as_mut().unwrap().until = Instant::now();
    room.run(&mut [(&mut b, &b_bus), (&mut c, &c_bus)], |r, _, _| r.parked.buses()[0].driver.is_none() && r.parked.seen.len() == 2);
    assert_eq!(room.parked.buses().len(), 1);
    // a player who goes while a bus is kept for it leaves the one it came in, as any player
    c.command(1, &format!("park-take {first} 7"));
    room.run(&mut [(&mut b, &b_bus), (&mut c, &on_foot(&c_bus, 101.0))], |_, _, heard| heard.contains(&format!("park-grant {first} 7")));
    c.leave();
    drop(c);
    room.run(&mut [(&mut b, &b_bus)], |r, p, _| r.parked.buses()[0].driver.is_none() && sees(p[0].0, first + 1, 110.0));
    let _ = std::fs::remove_file(&room.file);
}

#[test]
fn a_list_that_could_not_be_written_is_written_later() {
    let dir = std::env::temp_dir().join(format!("openomsi-parked-nodir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut p = Parked::open_with(&dir.join("parked.json"), "m", 4, &[], |_| true);
    p.left(2, &bus(&fp("a1"), 1.0));
    // (its folder is not there: nothing can be written)
    p.save();
    assert!(p.unsaved.is_some(), "what changed is still to be written");
    std::fs::create_dir_all(&dir).unwrap();
    p.save();
    assert!(p.unsaved.is_none());
    assert_eq!(Parked::open_with(&dir.join("parked.json"), "m", 4, &[], |_| true).buses().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_administration_parks_a_free_bus_elsewhere_but_not_one_a_player_has() {
    let mut p = Parked::open_with(&temp("put"), "m", 4, &[], |_| true);
    p.left(2, &Pose { pitch: 4.0, ..bus(&fp("a1"), 7000.0) });
    p.left(3, &bus(&fp("b2"), 50.0));
    let first = FIRST_PARKED_ID;
    // put down on the ground there, level, the heading within a turn
    // (the floor nearest the height it stood at: asked for with that height)
    assert_eq!(p.put(first, 120.5, -30.0, 450.0, &|_, _, near| (near == 3.0).then_some(8.25)), Ok(()));
    let b = &p.buses()[0];
    assert_eq!((b.x, b.y, b.z, b.heading, b.pitch), (120.5, -30.0, 8.25, 90.0, 0.0));
    assert!(p.unsaved.is_some() && p.changed, "told to the players and written");
    // (where the world does not know the ground: at the height it stood)
    assert_eq!(p.put(first, 10.0, 10.0, 0.0, &|_, _, _| None), Ok(()));
    assert_eq!(p.buses()[0].z, 8.25);
    assert_eq!(p.put(first + 9, 0.0, 0.0, 0.0, &|_, _, _| None), Err("no such bus"));
    assert_eq!(p.put(first, f64::NAN, 0.0, 0.0, &|_, _, _| None), Err("not a place"));
    p.buses[1].driver = Some(Driver { player: 5, drives: true, until: Instant::now(), op: 7, came_in: None });
    assert_eq!(p.put(first + 1, 0.0, 0.0, 0.0, &|_, _, _| None), Err("a player has it"));
    assert_eq!(p.buses()[1].x, 50.0);
}

#[test]
fn the_bus_a_player_came_in_stays_where_it_was_left_not_where_the_bus_taken_stood() {
    let first = FIRST_PARKED_ID;
    let mut room = Room::new(28628, "leftfar");
    let (mut a, mut b) = (room.join("a"), room.join("b"));
    let (a_bus, b_bus) = (bus(&fp("a1"), 100.0), bus(&fp("b2"), 300.0));
    room.run(&mut [(&mut a, &a_bus), (&mut b, &b_bus)], |r, _, _| r.parked.seen.len() == 2);
    a.leave();
    drop(a);
    room.run(&mut [(&mut b, &b_bus)], |_, p, _| sees(p[0].0, first, 100.0));
    // B left its bus two hundred metres away and walked up to A's
    room.run(&mut [(&mut b, &on_foot(&b_bus, 102.0))], |r, _, _| r.host.peers().any(|q| q.pose.walker.is_some()));
    b.command(1, &format!("park-take {first} 7"));
    room.run(&mut [(&mut b, &on_foot(&b_bus, 102.0))], |_, _, heard| heard.contains(&format!("park-grant {first} 7")));
    // B's game sits down in it: for a moment the server knows the new place (a state) under
    // the files of the bus B came in (the last info) - that is not B's own bus driven there
    let between = Pose { x: 100.0, ..b_bus.clone() };
    let t0 = Instant::now();
    room.run(&mut [(&mut b, &between)], |_, _, _| t0.elapsed() > Duration::from_millis(600));
    assert_eq!(room.parked.buses()[0].driver.as_ref().and_then(|d| d.came_in.as_ref()).map(|c| c.x), Some(300.0));
    let taken = bus(&fp("a1"), 100.0);
    room.run(&mut [(&mut b, &taken)], |r, _, _| r.host.peers().any(|q| q.pose.content == fp("a1")));
    b.command(1, &format!("park-took {first} 7"));
    room.run(&mut [(&mut b, &taken)], |_, p, heard| heard.contains(&format!("park-yours {first} 7")) && sees(p[0].0, first + 1, 300.0));
    assert_eq!(room.parked.buses().iter().map(|b| (b.id, b.x)).collect::<Vec<_>>(), vec![(first, 100.0), (first + 1, 300.0)]);
    let _ = std::fs::remove_file(&room.file);
}

#[test]
fn a_bus_of_the_same_files_standing_near_is_not_the_bus_taken() {
    let first = FIRST_PARKED_ID;
    let mut room = Room::new(28622, "samefiles");
    let (mut a, mut b) = (room.join("a"), room.join("b"));
    // B drives the very bus A drives (the same files): two of a kind side by side
    let (a_bus, b_bus) = (bus(&fp("a1"), 100.0), bus(&fp("a1"), 110.0));
    room.run(&mut [(&mut a, &a_bus), (&mut b, &b_bus)], |r, _, _| r.parked.seen.len() == 2);
    a.leave();
    drop(a);
    room.run(&mut [(&mut b, &b_bus)], |_, p, _| sees(p[0].0, first, 100.0));
    b.command(1, &format!("park-take {first} 7"));
    room.run(&mut [(&mut b, &on_foot(&b_bus, 102.0))], |_, _, heard| heard.contains(&format!("park-grant {first} 7")));
    // B's game still shows the bus B came in, ten metres from the one kept for it: that is
    // not B driving the other - it stands there, kept for B, for everybody to see
    let t0 = Instant::now();
    room.run(&mut [(&mut b, &b_bus)], |_, _, _| t0.elapsed() > Duration::from_millis(1500));
    assert!(!room.parked.buses()[0].driver.as_ref().unwrap().drives);
    assert!(sees(&b, first, 100.0));
    // B's game says it took it (and shows it where it stood): now it is B's
    b.command(1, &format!("park-took {first} 7"));
    room.run(&mut [(&mut b, &bus(&fp("a1"), 100.0))], |r, _, heard| heard.contains(&format!("park-yours {first} 7")) && r.parked.buses()[0].driver.as_ref().is_some_and(|d| d.drives));
    let _ = std::fs::remove_file(&room.file);
}

#[test]
fn a_player_who_drives_on_after_asking_leaves_its_bus_where_it_got_to() {
    let first = FIRST_PARKED_ID;
    let mut room = Room::new(28623, "driveson");
    let (mut a, mut b) = (room.join("a"), room.join("b"));
    let (a_bus, b_bus) = (bus(&fp("a1"), 100.0), bus(&fp("b2"), 120.0));
    room.run(&mut [(&mut a, &a_bus), (&mut b, &b_bus)], |r, _, _| r.parked.seen.len() == 2);
    a.leave();
    drop(a);
    room.run(&mut [(&mut b, &b_bus)], |_, p, _| sees(p[0].0, first, 100.0));
    // B asks, the yes is given (and, say, lost): B gets back into its own bus and drives off
    b.command(1, &format!("park-take {first} 7"));
    room.run(&mut [(&mut b, &on_foot(&b_bus, 102.0))], |r, _, _| r.parked.buses()[0].driver.is_some());
    let far = bus(&fp("b2"), 900.0);
    room.run(&mut [(&mut b, &far)], |r, _, _| r.parked.buses()[0].driver.as_ref().and_then(|d| d.came_in.as_ref()).is_some_and(|c| c.x == 900.0));
    // B goes: its bus stands where it got to, not where it stood when B asked
    b.leave();
    drop(b);
    room.run(&mut [], |r, _, _| r.parked.buses().len() == 2 && r.parked.buses()[0].driver.is_none());
    assert_eq!(room.parked.buses()[1].x, 900.0);
    let _ = std::fs::remove_file(&room.file);
}

#[test]
fn a_word_of_an_earlier_asking_does_not_pass_for_the_asking_that_stands() {
    let first = FIRST_PARKED_ID;
    let mut room = Room::new(28624, "ops");
    let (mut a, mut b) = (room.join("a"), room.join("b"));
    let (a_bus, b_bus) = (bus(&fp("a1"), 100.0), bus(&fp("b2"), 120.0));
    room.run(&mut [(&mut a, &a_bus), (&mut b, &b_bus)], |r, _, _| r.parked.seen.len() == 2);
    a.leave();
    drop(a);
    room.run(&mut [(&mut b, &b_bus)], |_, p, _| sees(p[0].0, first, 100.0));
    // B asks twice (a second G: another asking of the same bus): the bus is kept under the
    // later one's number, and each answer names the asking it answers
    b.command(1, &format!("park-take {first} 11"));
    room.run(&mut [(&mut b, &on_foot(&b_bus, 102.0))], |_, _, heard| heard.contains(&format!("park-grant {first} 11")));
    b.command(1, &format!("park-take {first} 12"));
    room.run(&mut [(&mut b, &on_foot(&b_bus, 102.0))], |_, _, heard| heard.contains(&format!("park-grant {first} 12")));
    // B's game drives it and says so under the earlier number: no word back, the bus still kept
    let taken = bus(&fp("a1"), 100.0);
    room.run(&mut [(&mut b, &taken)], |r, _, _| r.host.peers().any(|q| q.pose.content == fp("a1")));
    b.command(1, &format!("park-took {first} 11"));
    let t0 = Instant::now();
    room.run(&mut [(&mut b, &taken)], |_, _, heard| {
        assert!(!heard.iter().any(|h| h.starts_with("park-yours")), "{heard:?}");
        t0.elapsed() > Duration::from_millis(800)
    });
    assert!(!room.parked.buses()[0].driver.as_ref().unwrap().drives);
    // under the number of the asking that stands: B's
    b.command(1, &format!("park-took {first} 12"));
    room.run(&mut [(&mut b, &taken)], |r, _, heard| heard.contains(&format!("park-yours {first} 12")) && r.parked.buses()[0].driver.as_ref().is_some_and(|d| d.drives));
    let _ = std::fs::remove_file(&room.file);
}

#[test]
fn the_free_buses_are_put_down_on_the_ground_where_they_stand() {
    let spots = [([10.0, 20.0, -3.9], 0.0)];
    let mut p = Parked::open_with(&temp("seat"), "m", 4, &spots, |_| true);
    // three of the store put to the one entry point, in a row at its height
    p.park_kept(&[fp("a1"), fp("b2"), fp("c3")], &spots, |f| Some(bus(f, 0.0)));
    assert_eq!(p.buses().iter().map(|b| b.z).collect::<Vec<_>>(), vec![-3.9; 3]);
    p.buses[2].driver = Some(Driver { player: 5, drives: true, until: Instant::now(), op: 1, came_in: None });
    (p.changed, p.unsaved) = (false, None);
    // the yard rises along the row: 0.25 m for each metre east (and nothing is known at x > 16)
    let yard = |x: f64, _y: f64, _near: f64| (x <= 16.0).then_some(-3.9 + (x - 10.0) * 0.25);
    p.seat(&yard);
    assert_eq!(p.buses().iter().map(|b| (b.z * 1000.0).round() / 1000.0).collect::<Vec<_>>(), vec![-3.9, -2.775, -3.9]);
    assert!(p.changed && p.unsaved.is_some(), "told to the players and written");
    // (already down: nothing to tell)
    (p.changed, p.unsaved) = (false, None);
    p.seat(&yard);
    assert!(!p.changed && p.unsaved.is_none());
}

#[test]
fn an_asking_that_comes_after_a_later_one_changes_nothing() {
    let first = FIRST_PARKED_ID;
    let mut room = Room::new(28625, "late");
    let (mut a, mut c, mut b) = (room.join("a"), room.join("c"), room.join("b"));
    let (a_bus, c_bus, b_bus) = (bus(&fp("a1"), 100.0), bus(&fp("c3"), 104.0), bus(&fp("b2"), 120.0));
    room.run(&mut [(&mut a, &a_bus), (&mut c, &c_bus), (&mut b, &b_bus)], |r, _, _| r.parked.seen.len() == 3);
    // (one after the other: the first to go gets the first number)
    a.leave();
    drop(a);
    room.run(&mut [(&mut c, &c_bus), (&mut b, &b_bus)], |_, p, _| sees(p[1].0, first, 100.0));
    c.leave();
    drop(c);
    room.run(&mut [(&mut b, &b_bus)], |_, p, _| sees(p[0].0, first, 100.0) && sees(p[0].0, first + 1, 104.0));
    // B's askings reach the server the other way round: 12 (the second bus) first, then 11
    // (the first bus), asked before it. The earlier one is left aside without a word: the
    // second bus stays kept for B under 12, the first stays free
    b.command(1, &format!("park-take {} 12", first + 1));
    room.run(&mut [(&mut b, &on_foot(&b_bus, 103.0))], |_, _, heard| heard.contains(&format!("park-grant {} 12", first + 1)));
    b.command(1, &format!("park-take {first} 11"));
    let t0 = Instant::now();
    room.run(&mut [(&mut b, &on_foot(&b_bus, 103.0))], |_, _, heard| {
        assert!(heard.is_empty(), "{heard:?}");
        t0.elapsed() > Duration::from_millis(800)
    });
    assert!(room.parked.buses()[0].driver.is_none(), "the bus of the earlier asking is free");
    assert_eq!(room.parked.buses()[1].driver.as_ref().map(|d| d.op), Some(12));
    // the same asking said again is the same yes; and its taking is confirmed under its number
    b.command(1, &format!("park-take {} 12", first + 1));
    room.run(&mut [(&mut b, &on_foot(&b_bus, 103.0))], |_, _, heard| heard.contains(&format!("park-grant {} 12", first + 1)));
    let taken = bus(&fp("c3"), 104.0);
    room.run(&mut [(&mut b, &taken)], |r, _, _| r.host.peers().any(|q| q.pose.content == fp("c3")));
    b.command(1, &format!("park-took {} 12", first + 1));
    room.run(&mut [(&mut b, &taken)], |_, _, heard| heard.contains(&format!("park-yours {} 12", first + 1)));
    let _ = std::fs::remove_file(&room.file);
}
