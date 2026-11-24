//! Who may come onto the road now. What the map, the timetable and the view would like on
//! the road (the demand, worked out by the population) is kept apart from whether a vehicle
//! may be put somewhere now (admission): a road and a way on from there, loaded ground, a
//! gap to whatever is there, nobody to see it appear.
//!
//! Requests wait in a bounded queue and are looked at in the order they came, a bounded
//! number per pass, so a pass costs the same whatever is asked and decides the same in
//! every run. A request is admitted, refused for good (`Denial`: no room in its class's
//! budget, no way, no ground) or left for a later pass (`Retry`: in sight, no gap - a gap
//! counts the vehicles admitted earlier in the same pass too, so two never land on one
//! spot). A place that keeps being refused (an entrance with a queue standing in it) is not
//! asked again for a while: it does not pile up requests.
//!
//! Each class has a budget of its own: the timetable's vehicles (`[AIMaxCountScheduled]`),
//! the random street traffic, the parked cars pulling out, aircraft, the dormant cars
//! waking up. Only the host of a LAN session (or a game on its own) decides who is on the
//! road: a client draws what the host sends (`TrafficSim::mirror`).

use super::*;
use std::collections::{BTreeMap, VecDeque};

/// The kinds of vehicles put on the road, each with a budget of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    Street,
    Parked,
    Aircraft,
    Scheduled,
    Dormant,
}

impl Class {
    pub const COUNT: usize = 5;
}

/// Refused for good.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Denial {
    /// Its class's budget is used up.
    AtCapacity,
    /// No road there, or none on from there (no vehicle of the traffic may use it).
    NoPath,
    /// The ground there is not loaded.
    NoGround,
    /// The queue is full: the request was not taken.
    QueueFull,
    /// Its place is being left alone for a while (`BACKOFF_SHORT`, `BACKOFF_LONG`).
    Backoff,
    /// Waited too long for its turn (`REQUEST_TTL`).
    Expired,
    /// A LAN client: the host decides the population.
    NotHost,
}

impl Denial {
    pub const COUNT: usize = 7;
}

/// Not now: asked again at a later pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Retry {
    /// Somebody would see it appear.
    Visible,
    /// Something stands too close (`SPAWN_GAP`), or would have to brake hard for it.
    NoGap,
}

impl Retry {
    pub const COUNT: usize = 2;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Admit,
    Deny(Denial),
    Retry(Retry),
}

/// Requests the queue holds at most, and looks at in one pass at most.
pub const QUEUE_CAP: usize = 256;
pub const EXAMINE_PER_PASS: usize = 64;
/// No vehicle is put nearer than this to another, or to one admitted in the same pass (m).
pub const SPAWN_GAP: f64 = 14.0;
/// A place refused this many times running is left alone for `BACKOFF_LONG` seconds,
/// otherwise for `BACKOFF_SHORT` after each refusal.
pub const BACKOFF_AFTER: u32 = 3;
pub const BACKOFF_SHORT: f32 = 0.5;
pub const BACKOFF_LONG: f32 = 3.0;
/// A place is the stretch of `ENTRANCE_SPAN` metres of a lane it lies in: a refusal leaves
/// that stretch alone, not the whole lane (a long country road holds many cars, and one car
/// standing on it barred it all).
pub const ENTRANCE_SPAN: f32 = 40.0;

fn entrance(lane: usize, s: f32) -> (usize, i32) {
    (lane, (s / ENTRANCE_SPAN).floor() as i32)
}

/// A request told to retry this many times is given up (`Denial::Expired`).
pub const MAX_TRIES: u8 = 3;

/// A request not admitted within this long is dropped (s): the population asks afresh.
pub const REQUEST_TTL: f32 = 10.0;
/// Nor in line with one admitted in the same pass, one ahead of the other, nearer than this
/// (m): the one behind, at its first speed of 0.7 the limit, has room enough to fall in
/// behind without braking hard (40 m, as first chosen, left Grundorf's first population a
/// third short; the cars on the road keep their own distance by `TrafficSim::spawn_gap`).
pub const SPAWN_IN_LINE: f64 = 25.0;

/// A vehicle asked to be put on the road.
#[derive(Debug, Clone)]
pub struct Request {
    /// Its place in the order the requests came.
    pub ticket: u64,
    pub class: Class,
    pub lane: usize,
    pub s: f32,
    pub pos: DVec3,
    /// Its heading there (degrees clockwise from north).
    pub heading: f64,
    /// When it was asked (traffic time, s).
    pub at: f32,
    /// A dormant car waking up: its id.
    pub dormant: Option<u64>,
    /// Times it was told to retry.
    pub tries: u8,
}

/// What is left of each class's budget in this pass (None: no limit).
#[derive(Debug, Clone, Copy, Default)]
pub struct Budgets([Option<usize>; Class::COUNT]);

impl Budgets {
    pub fn set(&mut self, class: Class, room: Option<usize>) -> &mut Self {
        self.0[class as usize] = room;
        self
    }

    pub fn room(&self, class: Class) -> Option<usize> {
        self.0[class as usize]
    }

    fn has_room(&self, class: Class) -> bool {
        self.0[class as usize].is_none_or(|n| n > 0)
    }

    fn take(&mut self, class: Class) {
        if let Some(n) = self.0[class as usize].as_mut() {
            *n = n.saturating_sub(1);
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Entrance {
    /// Refusals in a row.
    refused: u32,
    /// Not asked again before this time.
    until: f32,
}

/// The admission queue (see the module's notes) and its counts.
#[derive(Debug, Clone, Default)]
pub struct Admission {
    queue: VecDeque<Request>,
    entrances: BTreeMap<(usize, i32), Entrance>,
    next_ticket: u64,
    generation: u64,
    /// How often each answer was given (diagnostics, `OMSI_DEBUG_POPULATION`).
    pub admitted: u64,
    pub denied: [u64; Denial::COUNT],
    pub retried: [u64; Retry::COUNT],
}

impl Admission {
    /// Requests waiting.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// The network grew (`TrafficSim::lanes_generation`): the queue and what the entrances
    /// remember go, as the lanes around them may have changed.
    pub fn new_network(&mut self, generation: u64) {
        if generation != self.generation {
            self.generation = generation;
            self.queue.clear();
            self.entrances.clear();
        }
    }

    /// Requests of `class` waiting.
    pub fn waiting(&self, class: Class) -> usize {
        self.queue.iter().filter(|r| r.class == class).count()
    }

    /// The first population: the places refused a moment ago may be asked again at once
    /// (nobody watches the world load; each round tries other places beside them).
    pub fn forget_refusals(&mut self) {
        self.entrances.clear();
    }

    /// Whether the place `s` metres along lane `lane` is being left alone at `now` after its
    /// refusals.
    pub fn backing_off(&self, lane: usize, s: f32, now: f32) -> bool {
        self.entrances
            .get(&entrance(lane, s))
            .is_some_and(|e| e.until > now)
    }

    /// Ask for a vehicle of `class` at `s` metres along `lane` (at `pos`). Its ticket, or why
    /// it was not taken: the queue is full, or the place is being left alone.
    pub fn submit(
        &mut self,
        now: f32,
        class: Class,
        (lane, s): (usize, f32),
        (pos, heading): (DVec3, f64),
        dormant: Option<u64>,
    ) -> Result<u64, Denial> {
        let refused = if self.queue.len() >= QUEUE_CAP {
            Denial::QueueFull
        } else if self.backing_off(lane, s, now) {
            Denial::Backoff
        } else {
            let ticket = self.next_ticket;
            self.next_ticket += 1;
            self.queue.push_back(Request {
                ticket,
                class,
                lane,
                s,
                pos,
                heading,
                at: now,
                dormant,
                tries: 0,
            });
            return Ok(ticket);
        };
        self.denied[refused as usize] += 1;
        Err(refused)
    }

    fn refused(&mut self, lane: usize, s: f32, now: f32) {
        let e = self.entrances.entry(entrance(lane, s)).or_default();
        e.refused += 1;
        e.until = now
            + if e.refused >= BACKOFF_AFTER {
                BACKOFF_LONG
            } else {
                BACKOFF_SHORT
            };
    }

    /// One pass over the queue at `now`: up to `EXAMINE_PER_PASS` requests in the order they
    /// came, each against its class's budget, the vehicles admitted before it in this pass
    /// and `check` (the world: way, ground, view, the vehicles on the road). Returns the
    /// admitted requests in order; the refused ones leave the queue, the ones to retry keep
    /// their places. `host`: false on a LAN client, which admits nobody.
    pub fn pass(
        &mut self,
        now: f32,
        host: bool,
        budgets: &mut Budgets,
        check: impl FnMut(&Request) -> Verdict,
    ) -> Vec<Request> {
        self.pass_for(None, now, host, budgets, check)
    }

    /// `pass` over the requests of `class` alone (None: all); the others keep their places.
    pub fn pass_for(
        &mut self,
        class: Option<Class>,
        now: f32,
        host: bool,
        budgets: &mut Budgets,
        mut check: impl FnMut(&Request) -> Verdict,
    ) -> Vec<Request> {
        let mut admitted: Vec<Request> = Vec::new();
        let mut kept = VecDeque::with_capacity(self.queue.len());
        let mut examined = 0;
        while let Some(r) = self.queue.pop_front() {
            if examined >= EXAMINE_PER_PASS || class.is_some_and(|c| c != r.class) {
                kept.push_back(r);
                continue;
            }
            if self.backing_off(r.lane, r.s, now) && now - r.at < REQUEST_TTL {
                // (its place is left alone a while: its turn comes again after that)
                kept.push_back(r);
                continue;
            }
            examined += 1;
            let verdict = if !host {
                Verdict::Deny(Denial::NotHost)
            } else if now - r.at > REQUEST_TTL {
                Verdict::Deny(Denial::Expired)
            } else {
                decide(&r, budgets, &admitted, &mut check)
            };
            match verdict {
                Verdict::Admit => {
                    self.admitted += 1;
                    budgets.take(r.class);
                    self.entrances.remove(&entrance(r.lane, r.s));
                    admitted.push(r);
                }
                Verdict::Deny(d) => {
                    self.denied[d as usize] += 1;
                    if d != Denial::AtCapacity {
                        self.refused(r.lane, r.s, now);
                    }
                }
                Verdict::Retry(why) => {
                    self.retried[why as usize] += 1;
                    self.refused(r.lane, r.s, now);
                    let mut r = r;
                    r.tries += 1;
                    if r.tries < MAX_TRIES {
                        kept.push_back(r);
                    } else {
                        // (a place that would not do several times is given up: waiting in
                        // front, it took the turns of fresh places, and the queue filled
                        // with them)
                        self.denied[Denial::Expired as usize] += 1;
                    }
                }
            }
        }
        self.queue = kept;
        admitted
    }
}

/// The verdict on request `r`: its class's budget, the gap to the ones `admitted` before it
/// in the same pass, then `check`.
fn decide(
    r: &Request,
    budgets: &Budgets,
    admitted: &[Request],
    check: &mut impl FnMut(&Request) -> Verdict,
) -> Verdict {
    if !budgets.has_room(r.class) {
        return Verdict::Deny(Denial::AtCapacity);
    }
    if admitted.iter().any(|a| too_close(a, r)) {
        return Verdict::Retry(Retry::NoGap);
    }
    check(r)
}

/// Would two vehicles put at `a` and `b` stand too close: nearer than `SPAWN_GAP`, or in
/// line, one ahead of the other, nearer than `SPAWN_IN_LINE`?
fn too_close(a: &Request, b: &Request) -> bool {
    let rel = b.pos - a.pos;
    if rel.length() < SPAWN_GAP {
        return true;
    }
    // (dormant cars wake at the speed they drove at, one behind the other as they went:
    // the plain gap is enough between them)
    if a.class == Class::Dormant && b.class == Class::Dormant {
        return false;
    }
    [a.heading, b.heading].iter().any(|h| {
        let h = h.to_radians();
        let (along, across) = (
            rel.x * h.sin() + rel.y * h.cos(),
            (rel.x * h.cos() - rel.y * h.sin()).abs(),
        );
        across < 3.0 && along.abs() < SPAWN_IN_LINE
    })
}

/// Decide a batch that does not wait in the queue (the dormant cars, which wait in their
/// own registry): in the order given, against the budget of each one's class, the ones
/// admitted before it and `check`. The verdict for each.
pub fn decide_batch(
    requests: &[Request],
    budgets: &mut Budgets,
    mut check: impl FnMut(&Request) -> Verdict,
) -> Vec<Verdict> {
    let mut admitted: Vec<Request> = Vec::new();
    requests
        .iter()
        .map(|r| {
            let v = decide(r, budgets, &admitted, &mut check);
            if v == Verdict::Admit {
                budgets.take(r.class);
                admitted.push(r.clone());
            }
            v
        })
        .collect()
}

impl TrafficSim {
    /// How many more timetable vehicles may be on the road (`[AIMaxCountScheduled]`; None:
    /// no limit). A timetable vehicle is admitted on its own path (the timetable finds it a
    /// place and checks it), not through the queue of the random traffic.
    pub fn scheduled_room(&self) -> Option<usize> {
        if self.no_timetable_buses {
            return Some(0);
        }
        (self.max_scheduled > 0).then(|| {
            let on = self
                .cars
                .iter()
                .filter(|c| c.is_bus() || !c.state.route.is_empty())
                .count();
            (self.max_scheduled as usize).saturating_sub(on)
        })
    }

    /// Whether the vehicle bodies on the road leave a gap at `p` (`SPAWN_GAP`), and nobody
    /// drives up to it so close that they would have to brake hard for a vehicle put there.
    pub fn spawn_gap(&self, p: DVec3) -> bool {
        self.cars.iter().all(|c| {
            if (c.vehicle.position - p).length() < SPAWN_GAP {
                return false;
            }
            let rel = p - c.vehicle.position;
            let h = c.vehicle.heading.to_radians();
            let (along, across) = (
                rel.x * h.sin() + rel.y * h.cos(),
                (rel.x * h.cos() - rel.y * h.sin()).abs(),
            );
            let braking = (c.state.speed * c.state.speed / (2.0 * c.state.decel.max(1.0))) as f64;
            !(along > 0.0 && along < 20.0 + braking * 1.5 && across < 3.0)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f64) -> DVec3 {
        DVec3::new(x, 0.0, 0.0)
    }

    fn street(n: usize) -> Budgets {
        let mut b = Budgets::default();
        b.set(Class::Street, Some(n));
        b
    }

    /// Far more asked than the queue holds: it stays bounded, a pass looks at a bounded
    /// number, and no two of the admitted ones stand on one spot.
    #[test]
    fn an_overload_is_bounded_and_admits_no_two_on_one_spot() {
        let mut a = Admission::default();
        let mut refused = 0;
        for k in 0..1000 {
            // (pairs of requests 3 m apart, at 40 lanes)
            if a.submit(
                0.0,
                Class::Street,
                (k % 40, 0.0),
                (at((k / 2) as f64 * 20.0 + (k % 2) as f64 * 3.0), 0.0),
                None,
            )
            .is_err()
            {
                refused += 1;
            }
        }
        assert_eq!(a.len(), QUEUE_CAP);
        assert_eq!(refused, 1000 - QUEUE_CAP);
        assert_eq!(a.denied[Denial::QueueFull as usize], refused as u64);
        let mut looked = 0;
        let admitted = a.pass(0.0, true, &mut street(1000), |_| {
            looked += 1;
            Verdict::Admit
        });
        assert!(looked <= EXAMINE_PER_PASS);
        assert!(!admitted.is_empty());
        for (i, x) in admitted.iter().enumerate() {
            for y in &admitted[i + 1..] {
                assert!(
                    (x.pos - y.pos).length() >= SPAWN_GAP,
                    "{:?} and {:?}",
                    x.pos,
                    y.pos
                );
            }
        }
        // in the order they came
        assert!(admitted.windows(2).all(|w| w[0].ticket < w[1].ticket));
        assert!(a.retried[Retry::NoGap as usize] > 0);
    }

    /// An entrance whose requests keep being refused (a queue stands in it) is left alone
    /// for a while: it does not pile up requests.
    #[test]
    fn a_busy_entrance_is_backpressured() {
        let mut a = Admission::default();
        let mut now = 0.0;
        let mut taken = 0;
        while now < 2.0 {
            if a.submit(now, Class::Street, (5, 0.0), (at(0.0), 0.0), None)
                .is_ok()
            {
                taken += 1;
            }
            a.pass(now, true, &mut street(10), |_| Verdict::Retry(Retry::NoGap));
            now += 0.1;
        }
        assert!(a.len() <= BACKOFF_AFTER as usize, "{} waiting", a.len());
        assert!(taken <= BACKOFF_AFTER as usize + 1, "{taken} taken");
        assert!(a.backing_off(5, 0.0, now));
        assert!(a.denied[Denial::Backoff as usize] > 0);
        // another place is not held up by it
        assert!(
            a.submit(now, Class::Street, (6, 0.0), (at(500.0), 0.0), None)
                .is_ok()
        );
        let admitted = a.pass(now, true, &mut street(10), |r| {
            if r.lane == 6 {
                Verdict::Admit
            } else {
                Verdict::Retry(Retry::NoGap)
            }
        });
        assert_eq!(admitted.len(), 1);
        // and an admission there clears its record
        assert!(!a.backing_off(6, 0.0, now));
    }

    /// Each class has a budget of its own, refusals say why, and a LAN client admits nobody.
    #[test]
    fn budgets_are_kept_per_class_and_only_the_host_admits() {
        let mut a = Admission::default();
        for k in 0..4 {
            a.submit(
                0.0,
                Class::Street,
                (k, 0.0),
                (at(k as f64 * 100.0), 0.0),
                None,
            )
            .unwrap();
        }
        a.submit(0.0, Class::Aircraft, (9, 0.0), (at(1000.0), 0.0), None)
            .unwrap();
        let mut b = Budgets::default();
        b.set(Class::Street, Some(2)).set(Class::Aircraft, Some(1));
        let admitted = a.pass(0.0, true, &mut b, |_| Verdict::Admit);
        assert_eq!(
            admitted.iter().map(|r| r.class).collect::<Vec<_>>(),
            vec![Class::Street, Class::Street, Class::Aircraft]
        );
        assert_eq!(a.denied[Denial::AtCapacity as usize], 2);
        assert_eq!(b.room(Class::Street), Some(0));
        a.submit(0.0, Class::Street, (20, 0.0), (at(0.0), 0.0), None)
            .unwrap();
        a.submit(0.0, Class::Street, (21, 0.0), (at(300.0), 0.0), None)
            .unwrap();
        assert!(
            a.pass(0.0, true, &mut street(5), |r| if r.lane == 20 {
                Verdict::Deny(Denial::NoGround)
            } else {
                Verdict::Deny(Denial::NoPath)
            })
            .is_empty()
        );
        assert_eq!(a.denied[Denial::NoGround as usize], 1);
        assert_eq!(a.denied[Denial::NoPath as usize], 1);
        a.submit(5.0, Class::Street, (30, 0.0), (at(0.0), 0.0), None)
            .unwrap();
        assert!(
            a.pass(5.0, false, &mut street(5), |_| Verdict::Admit)
                .is_empty()
        );
        assert_eq!(a.denied[Denial::NotHost as usize], 1);
        assert!(a.is_empty());
    }

    /// A request kept waiting by the view expires; a new network empties the queue.
    #[test]
    fn requests_lapse_and_a_new_network_starts_afresh() {
        let mut a = Admission::default();
        a.submit(0.0, Class::Street, (1, 0.0), (at(0.0), 0.0), None)
            .unwrap();
        let mut now = 0.0;
        while now <= REQUEST_TTL + BACKOFF_LONG + 1.0 {
            a.pass(now, true, &mut street(5), |_| {
                Verdict::Retry(Retry::Visible)
            });
            now += 0.5;
        }
        assert!(a.is_empty());
        assert_eq!(a.denied[Denial::Expired as usize], 1);
        a.submit(now, Class::Street, (2, 0.0), (at(0.0), 0.0), None)
            .unwrap();
        a.new_network(1);
        assert!(a.is_empty());
        assert!(!a.backing_off(1, 0.0, now));
    }

    /// A batch (dormant cars waking) is decided in its order, against its budget and the
    /// ones admitted before.
    #[test]
    fn a_batch_wakes_in_order_without_overlaps() {
        let r = |k: u64, x: f64| Request {
            ticket: k,
            class: Class::Dormant,
            lane: 0,
            s: 0.0,
            pos: at(x),
            heading: 0.0,
            at: 0.0,
            dormant: Some(k),
            tries: 0,
        };
        let reqs = [r(1, 0.0), r(2, 5.0), r(3, 50.0), r(4, 100.0)];
        let mut b = Budgets::default();
        b.set(Class::Dormant, Some(2));
        let v = decide_batch(&reqs, &mut b, |_| Verdict::Admit);
        assert_eq!(
            v,
            vec![
                Verdict::Admit,
                Verdict::Retry(Retry::NoGap),
                Verdict::Admit,
                Verdict::Deny(Denial::AtCapacity)
            ]
        );
    }
    /// Two put in line on one road in the same pass keep more than a gap apart: the one
    /// behind would brake hard for the other at once. Side by side on two roads they may.
    #[test]
    fn two_admitted_in_line_keep_their_distance() {
        let r = |k: u64, x: f64, y: f64| Request {
            ticket: k,
            class: Class::Street,
            lane: k as usize,
            s: 0.0,
            pos: DVec3::new(x, y, 0.0),
            heading: 0.0,
            at: 0.0,
            dormant: None,
            tries: 0,
        };
        let reqs = [
            r(1, 0.0, 0.0),
            r(2, 0.0, 20.0),
            r(3, 10.0, 45.0),
            r(4, 0.0, 70.0),
        ];
        let v = decide_batch(&reqs, &mut street(10), |_| Verdict::Admit);
        assert_eq!(
            v,
            vec![
                Verdict::Admit,
                Verdict::Retry(Retry::NoGap),
                Verdict::Admit,
                Verdict::Admit
            ]
        );
    }
}
