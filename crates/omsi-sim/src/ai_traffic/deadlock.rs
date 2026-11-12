//! Circles of cars that wait on each other, found on the waits-for graph of every step and
//! sorted by what holds them.
//!
//! Every car notes whom it waits for at its junction and why (`AiCar::holds`), and whom it
//! follows (`AiCar::waits_on`). The circles of that graph (its strongly connected parts of
//! two cars or more) are of four kinds:
//! - a red light holds one of them: congestion as the law wants it, it clears on green;
//! - they only hold each other's claims (decisions to go that nobody has acted on, no body
//!   in anybody's way): the claims are stale - the one that has waited longest keeps its
//!   claim and tries again, the others let theirs go;
//! - they all stand and only give way to each other by the rules (four cars at a junction
//!   where everybody has somebody on the right): after `YIELD_CYCLE_WAIT` the one that has
//!   waited longest is let go by the rules for `YIELD_PASS` (it still waits for bodies,
//!   people, lights and a full exit), as drivers sort it out by hand signals;
//! - they all stand, nobody's body is in anybody's way, and the right of way or a claim
//!   holds some of them while the others follow them or wait for room on their exit: after
//!   `YIELD_CYCLE_WAIT` the claims are let go but the longest waiter's, and he is let go
//!   by the rules (`Circle::Tangled`);
//! - otherwise the road is physically full: nobody is forced through anybody.
//!
//! Nothing here moves a car through a body or a red light; the timer escapes it replaces
//! (a pass that disregarded every rule of the junction, the squeeze into a full exit after
//! 45 s, the claim of a long waiter) did.

use super::*;

/// Why a car waits for another at a junction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    /// Its body is in this car's way: in a meeting place, moving through it.
    Body,
    /// It has claimed its way through and gets to the meeting place first.
    Claim,
    /// It has the right of way.
    Rule,
    /// It waits at its line, and this car (with the right of way) lets it in.
    Courtesy,
    /// It is the last of the queue on this car's exit, or ahead on the way with no room
    /// behind it beyond the junction.
    Exit,
    /// It is the car ahead.
    Lead,
}

/// Seconds every car of a circle that only gives way to each other has stood before one of
/// them is let go.
pub const YIELD_CYCLE_WAIT: f32 = 8.0;
/// Seconds that one is let go by the rules.
pub const YIELD_PASS: f32 = 12.0;

/// What a circle of waiting cars is (see the module).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Circle {
    Legal,
    /// One of them lets another in that waits, in the end, for it.
    Courtesy,
    StaleClaims,
    Yielding,
    /// The right of way or claims hold some of them, the others follow them or wait for
    /// room on an exit; no body is in anybody's way.
    Tangled,
    Full,
}

/// The strongly connected parts of two or more nodes of the graph `edges` (Tarjan's
/// algorithm, without recursion), each in the order of its nodes.
pub fn circles(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = edges.len();
    let mut index = vec![usize::MAX; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut next = 0usize;
    let mut out = Vec::new();
    for root in 0..n {
        if index[root] != usize::MAX {
            continue;
        }
        // (node, how many of its edges have been followed)
        let mut work: Vec<(usize, usize)> = vec![(root, 0)];
        while let Some(&(v, k)) = work.last() {
            if k == 0 && index[v] == usize::MAX {
                index[v] = next;
                low[v] = next;
                next += 1;
                stack.push(v);
                on_stack[v] = true;
            }
            if let Some(&w) = edges[v].get(k) {
                if let Some(top) = work.last_mut() {
                    top.1 += 1;
                }
                if index[w] == usize::MAX {
                    work.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
                continue;
            }
            work.pop();
            if let Some(&(u, _)) = work.last() {
                low[u] = low[u].min(low[v]);
            }
            if low[v] == index[v] {
                let mut part = Vec::new();
                while let Some(w) = stack.pop() {
                    on_stack[w] = false;
                    part.push(w);
                    if w == v {
                        break;
                    }
                }
                if part.len() > 1 {
                    part.sort_unstable();
                    out.push(part);
                }
            }
        }
    }
    out
}

/// The kind of a circle from what holds its members: whether a red light holds one of
/// them, whether all of them stand, and the kinds of the holds between them.
pub fn classify(red: bool, all_standing: bool, holds: &[Hold]) -> Circle {
    if red {
        Circle::Legal
    } else if holds.contains(&Hold::Courtesy) {
        Circle::Courtesy
    } else if !holds.is_empty() && holds.iter().all(|h| *h == Hold::Claim) {
        Circle::StaleClaims
    } else if all_standing && !holds.is_empty() && holds.iter().all(|h| *h == Hold::Rule) {
        Circle::Yielding
    } else if all_standing && !holds.contains(&Hold::Body) && holds.iter().any(|h| matches!(h, Hold::Rule | Hold::Claim)) {
        // Since 0.2.28 such a circle counted as a full road and was never sorted out (the
        // timer escape before it let one of any circle go): a side road's car giving way to
        // a main road's car that queues behind cars waiting, in the end, for the side road's
        // queue, or a claim held against a car that follows another one held by that claim,
        // stood for good - at a big crossing object every approach, at any light (#2373,
        // #2562), and round a roundabout (#2257). What holds them is only the rules.
        Circle::Tangled
    } else {
        Circle::Full
    }
}

/// Of the members (index, seconds waited, id), the one that has waited longest (a tie of
/// 0.05 s goes to the lower id).
pub fn longest_waiter(members: &[(usize, f32, u64)]) -> Option<usize> {
    members
        .iter()
        .min_by(|a, b| {
            if (a.1 - b.1).abs() <= 0.05 {
                a.2.cmp(&b.2)
            } else {
                b.1.total_cmp(&a.1)
            }
        })
        .map(|m| m.0)
}

impl TrafficSim {
    /// The waits-for graph of the last step, its circles, and what is done about each (see
    /// the module). Read from what every car noted in its plan, so the order the cars plan
    /// in decides nothing.
    pub fn resolve_waits(&mut self) {
        if !self.cars.iter().any(|car| !car.holds.is_empty()) {
            return;
        }
        let n = self.cars.len();
        let index = |id: u64| self.index_of.get(&id).copied().filter(|&k| k < n);
        // (edges with their kinds; a car that follows another stands for it)
        let mut kinds: Vec<Vec<(usize, Hold)>> = vec![Vec::new(); n];
        for (k, c) in self.cars.iter().enumerate() {
            for &(id, h) in &c.holds {
                if let Some(j) = index(id) {
                    kinds[k].push((j, h));
                }
            }
            if matches!(c.why.0, "lead" | "keep_back") {
                if let Some(j) = c.waits_on.and_then(index) {
                    kinds[k].push((j, Hold::Lead));
                }
            }
        }
        // (cheap: most steps nobody waits at a junction)
        if kinds.iter().all(|k| k.is_empty()) {
            return;
        }
        let edges: Vec<Vec<usize>> = kinds.iter().map(|k| k.iter().map(|e| e.0).collect()).collect();
        for part in circles(&edges) {
            let inside: Vec<Hold> = part
                .iter()
                .flat_map(|&k| kinds[k].iter().filter(|e| part.binary_search(&e.0).is_ok()).map(|e| e.1))
                .collect();
            let red = part.iter().any(|&k| self.cars[k].light_hold && self.cars[k].why.0 == "light");
            let standing = part.iter().all(|&k| self.cars[k].state.speed < 0.3);
            let kind = classify(red, standing, &inside);
            let members: Vec<(usize, f32, u64)> = part.iter().map(|&k| (k, self.cars[k].state.yield_time, self.cars[k].id)).collect();
            let debug = omsi_cfg::flags::OMSI_DEBUG_STUCK.is_set() || omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set();
            match kind {
                Circle::StaleClaims => {
                    let keep = longest_waiter(&members);
                    for &k in &part {
                        if Some(k) != keep {
                            self.cars[k].reserved.clear();
                            self.cars[k].exit_claim = None;
                        }
                    }
                    if debug {
                        log::info!("t={:.1}: cars {:?} hold each other's claims: car {:?} keeps its own", self.time, members.iter().map(|m| m.2).collect::<Vec<_>>(), keep.map(|k| self.cars[k].id));
                    }
                }
                Circle::Yielding => {
                    if part.iter().any(|&k| self.cars[k].yield_pass > self.time) {
                        continue; // one of them is going already
                    }
                    if part.iter().any(|&k| self.cars[k].state.yield_time < YIELD_CYCLE_WAIT) {
                        continue;
                    }
                    if let Some(go) = longest_waiter(&members) {
                        self.cars[go].yield_pass = self.time + YIELD_PASS;
                        if debug {
                            log::info!("t={:.1}: cars {:?} only give way to each other: car {} goes", self.time, members.iter().map(|m| m.2).collect::<Vec<_>>(), self.cars[go].id);
                        }
                    }
                }
                Circle::Tangled => {
                    if part.iter().any(|&k| self.cars[k].yield_pass > self.time) {
                        continue; // one of them is going already
                    }
                    // (the followers' wait at a junction is not counted: how long they stood)
                    if part.iter().any(|&k| self.cars[k].stopped < YIELD_CYCLE_WAIT) {
                        continue;
                    }
                    // of those the rules or a claim hold, the one that has waited longest (not
                    // one a full exit holds as well: let go by the rules, it would still stand)
                    let held: Vec<(usize, f32, u64)> = part
                        .iter()
                        .filter(|&&k| !self.cars[k].exit_wait && kinds[k].iter().any(|e| matches!(e.1, Hold::Rule | Hold::Claim) && part.binary_search(&e.0).is_ok()))
                        .map(|&k| (k, self.cars[k].state.yield_time, self.cars[k].id))
                        .collect();
                    let Some(go) = longest_waiter(&held) else { continue };
                    for &k in &part {
                        if k != go {
                            self.cars[k].reserved.clear();
                            self.cars[k].exit_claim = None;
                        }
                    }
                    self.cars[go].yield_pass = self.time + YIELD_PASS;
                    if debug {
                        log::info!("t={:.1}: cars {:?} hold each other by the rules and their claims: car {} goes", self.time, members.iter().map(|m| m.2).collect::<Vec<_>>(), self.cars[go].id);
                    }
                }
                Circle::Courtesy => {
                    // Letting somebody in is a favour, never owed: a driver who lets in a
                    // car that waits, round the circle, for him takes his right of way back
                    // (the queue behind him held the room on the exit the other car waited
                    // for, Grundorf at 17:00). Bodies, lights and a full exit still hold him.
                    for &k in &part {
                        if kinds[k].iter().any(|e| e.1 == Hold::Courtesy && part.binary_search(&e.0).is_ok()) {
                            self.cars[k].yield_pass = self.time + YIELD_PASS;
                            if debug {
                                log::info!("t={:.1}: car {} lets in a car that waits for it in turn ({:?}): it goes", self.time, self.cars[k].id, members.iter().map(|m| m.2).collect::<Vec<_>>());
                            }
                        }
                    }
                }
                Circle::Legal | Circle::Full => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circles_are_the_strongly_connected_parts() {
        // 0 -> 1 -> 2 -> 0, 3 -> 0 (waits on the circle, is not in it), 4 -> 5 -> 4
        let edges = vec![vec![1], vec![2], vec![0], vec![0], vec![5], vec![4], vec![]];
        let mut c = circles(&edges);
        c.sort();
        assert_eq!(c, vec![vec![0, 1, 2], vec![4, 5]]);
        // a chain is no circle
        assert!(circles(&[vec![1], vec![2], vec![]]).is_empty());
        // two circles joined by one edge stay two
        let mut c = circles(&[vec![1], vec![0, 2], vec![3], vec![2]]);
        c.sort();
        assert_eq!(c, vec![vec![0, 1], vec![2, 3]]);
    }

    #[test]
    fn circles_are_told_apart_by_what_holds_them() {
        use Hold::*;
        assert_eq!(classify(true, true, &[Rule, Rule]), Circle::Legal);
        assert_eq!(classify(false, true, &[Claim, Claim, Claim]), Circle::StaleClaims);
        assert_eq!(classify(false, true, &[Rule, Rule, Rule, Rule]), Circle::Yielding);
        // one of them still rolls: not (yet) a circle of cars standing for each other
        assert_eq!(classify(false, false, &[Rule, Rule]), Circle::Full);
        // a body in the way is physically full, never forced
        assert_eq!(classify(false, true, &[Rule, Body]), Circle::Full);
        // a claim or the right of way held against cars that follow, no body in the way
        assert_eq!(classify(false, true, &[Claim, Lead]), Circle::Tangled);
        assert_eq!(classify(false, true, &[Rule, Lead, Exit]), Circle::Tangled);
        assert_eq!(classify(false, false, &[Rule, Lead]), Circle::Full);
        // nothing but following and full exits: the road is full
        assert_eq!(classify(false, true, &[Lead, Exit, Lead]), Circle::Full);
        // a queue waiting behind a driver who lets in a car that waits for the queue
        assert_eq!(classify(false, true, &[Lead, Lead, Courtesy, Exit]), Circle::Courtesy);
        assert_eq!(classify(true, true, &[Lead, Courtesy, Exit]), Circle::Legal);
    }

    #[test]
    fn the_longest_waiter_is_chosen_a_tie_to_the_lower_id() {
        assert_eq!(longest_waiter(&[(0, 9.0, 5), (1, 12.0, 7), (2, 3.0, 1)]), Some(1));
        assert_eq!(longest_waiter(&[(0, 12.0, 5), (1, 12.03, 7)]), Some(0));
    }
}
