//! A bus's door keys, read from its model and scripts: which triggers open which doorway.

/// A door leaf's animation variable: `door_0`, `door_2L`, `Door_3` (and the stock scripts'
/// `doorTarget_0` names the same leaf).
fn door_leaf_of(var: &str) -> Option<String> {
    let v = var.to_ascii_lowercase();
    let rest = v.strip_prefix("doortarget_").or_else(|| v.strip_prefix("door_"))?;
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    let tail = &rest[digits..];
    (digits > 0 && tail.len() <= 1 && tail.chars().all(|c| c.is_ascii_alphabetic())).then(|| format!("door_{rest}"))
}

/// The door leaves a trigger moves: the leaf variables (or their targets) it stores, itself
/// or in its macros; failing that, the ones it reads.
pub fn trigger_leaves(program: &omsi_script::Program, name: &str) -> Vec<String> {
    fn walk(program: &omsi_script::Program, block: omsi_script::BlockId, seen: &mut hashbrown::HashSet<omsi_script::BlockId>, stored: &mut Vec<String>, read: &mut Vec<String>) {
        if !seen.insert(block) || seen.len() > 64 {
            return;
        }
        let Some(b) = program.blocks.get(block as usize) else { return };
        for op in &b.ops {
            match op {
                omsi_script::Op::Store(id) | omsi_script::Op::Load(id) => {
                    let Some(leaf) = program.var_names.get(*id as usize).and_then(|n| door_leaf_of(n)) else { continue };
                    let list = if matches!(op, omsi_script::Op::Store(_)) { &mut *stored } else { &mut *read };
                    if !list.contains(&leaf) {
                        list.push(leaf);
                    }
                }
                omsi_script::Op::Macro(m) => walk(program, *m, seen, stored, read),
                _ => {}
            }
        }
    }
    let Some(b) = program.trigger(name) else { return Vec::new() };
    let (mut stored, mut read) = (Vec::new(), Vec::new());
    walk(program, b, &mut hashbrown::HashSet::new(), &mut stored, &mut read);
    if stored.is_empty() { read } else { stored }
}

/// The doors of a bus front to back, found from the model: the meshes each door leaf's
/// variable animates, and leaves standing within 1.6 m of each other along the bus are one
/// doorway. Each doorway gets the triggers that move its leaves: the toggles
/// (`bus_doorfront<n>`, `bus_dooraft`, a mod's own), or a mod's open and close pair
/// (`bus_door_0` / `bus_door_0_close`, written `open|close`: the one that fits the leaf's
/// state is fired). None when the model's doors cannot be told apart this way.
fn doorways(ty: &crate::VehicleType) -> Option<Vec<Vec<String>>> {
    // where each leaf is along the bus (the centre of the meshes it moves)
    let mut at: Vec<(String, f32, u32)> = Vec::new();
    for vm in &ty.meshes {
        let Some(def) = ty.model.meshes.get(vm.def_index) else { continue };
        // (a pantograph some mod animates with a door variable is no door)
        let pantograph = ty.def.pantograph_var.as_deref();
        let Some(leaf) = def.animations.iter().filter(|a| pantograph.is_none_or(|p| !p.eq_ignore_ascii_case(&a.variable))).find_map(|a| door_leaf_of(&a.variable)) else { continue };
        if vm.data.positions.is_empty() {
            continue;
        }
        let y = vm.data.positions.iter().map(|p| vm.pivot.transform_point3(*p).y).sum::<f32>() / vm.data.positions.len() as f32;
        match at.iter_mut().find(|e| e.0 == leaf) {
            Some(e) => {
                e.1 += y;
                e.2 += 1;
            }
            None => at.push((leaf, y, 1)),
        }
    }
    if at.is_empty() {
        return None;
    }
    let mut leaves: Vec<(String, f32)> = at.into_iter().map(|(l, y, n)| (l, y / n as f32)).collect();
    leaves.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut ways: Vec<Vec<(String, f32)>> = Vec::new();
    for l in leaves {
        match ways.last_mut() {
            Some(w) if (w.last().unwrap().1 - l.1).abs() < 1.6 => w.push(l),
            _ => ways.push(vec![l]),
        }
    }
    let program = &ty.program;
    // which doorways a trigger reaches (its leaves' doorways; a script's branches all count)
    let way_of = |leaf: &String| ways.iter().position(|w| w.iter().any(|(l, _)| l == leaf));
    let reach = |name: &str| -> Vec<usize> {
        // (a target for several leaves of one doorway, the Urbanway's `doorTarget_23`, reaches it)
        let mut v: Vec<usize> = trigger_leaves(program, name)
            .iter()
            .flat_map(|leaf| match way_of(leaf) {
                Some(w) => vec![w],
                None => leaf
                    .strip_prefix("door_")
                    .filter(|d| d.len() > 1 && d.bytes().all(|c| c.is_ascii_digit()))
                    .and_then(|d| d.chars().map(|c| way_of(&format!("door_{c}"))).collect::<Option<Vec<usize>>>())
                    .filter(|w| w.iter().all(|x| *x == w[0]))
                    .map(|w| vec![w[0]])
                    .unwrap_or_default(),
            })
            .collect();
        v.sort();
        v.dedup();
        v
    };
    // the stock keys by name (`bus_doorfront<n>`, two leaves of a doorway paired), a pair
    // split when its two triggers share no doorway: the LiAZ's `bus_doorfront0` and `1` are
    // its middle and rear doors, and Shift+1 opened both
    let mut groups: Vec<(Vec<String>, usize)> = Vec::new();
    for g in door_trigger_groups(program) {
        if g == ["bus_dooraft"] {
            continue;
        }
        let parts: Vec<Vec<String>> = match (g.first(), g.get(1)) {
            (Some(a), Some(b)) if !reach(a).is_empty() && !reach(b).is_empty() && !reach(a).iter().any(|w| reach(b).contains(w)) => vec![vec![a.clone()], vec![b.clone()]],
            // (leaves on a coupled part, the Urbanway 18's `doorTarget_45` and `_67`: by their numbers)
            (Some(a), Some(b)) if (reach(a).is_empty() || reach(b).is_empty()) && numbers_differ(&trigger_door_numbers(program, a, true), &trigger_door_numbers(program, b, true)) => vec![vec![a.clone()], vec![b.clone()]],
            _ => vec![g.clone()],
        };
        for part in parts {
            let front = part.iter().flat_map(|n| reach(n)).min().unwrap_or(usize::MAX);
            groups.push((part, front));
        }
    }
    // doorways no stock key reaches: a mod's own door triggers, all of whose leaves lie in
    // that doorway (a toggle, or an open and close pair written `open|close`)
    let reached: Vec<usize> = groups.iter().flat_map(|(g, _)| g.iter().flat_map(|n| reach(n))).collect();
    let mut names: Vec<&String> = program.triggers.keys().filter(|n| {
        let n = n.to_ascii_lowercase();
        (n.starts_with("bus_door") || n.starts_with("bus_tuer")) && !n.ends_with("_off") && !n.ends_with("_close") && !n.starts_with("bus_doorfront") && n != "bus_dooraft"
    }).collect();
    names.sort();
    for w in 0..ways.len() {
        if reached.contains(&w) {
            continue;
        }
        let mut group: Vec<String> = Vec::new();
        for n in &names {
            let r = reach(n);
            if r != [w] {
                continue;
            }
            let close = format!("{n}_close");
            group.push(if program.trigger(&close).is_some() { format!("{n}|{close}") } else { (*n).clone() });
        }
        if !group.is_empty() {
            groups.push((group, w));
        }
    }
    groups.sort_by_key(|(_, front)| *front);
    let mut out: Vec<Vec<String>> = groups.into_iter().map(|(g, _)| g).collect();
    // a release switch (the SD202's `bus_dooraft`) comes last
    if program.trigger("bus_dooraft").is_some() {
        out.push(vec!["bus_dooraft".to_string()]);
    }
    (!out.is_empty()).then_some(out)
}

/// The door keys of a vehicle type: its doorways from the model ([`doorways`]), else the
/// trigger names ([`door_trigger_groups`]).
pub fn door_keys(ty: &crate::VehicleType) -> Vec<Vec<String>> {
    doorways(ty).unwrap_or_else(|| split_numbered_doors(&ty.program, door_trigger_groups(&ty.program)))
}

/// The door numbers (`Tuer_1`, `Tür_2`, with `targets` also `doorTarget_45`) a trigger stores into, itself or in its macros.
fn trigger_door_numbers(program: &omsi_script::Program, name: &str, targets: bool) -> Vec<u32> {
    fn walk(program: &omsi_script::Program, block: omsi_script::BlockId, seen: &mut hashbrown::HashSet<omsi_script::BlockId>, out: &mut Vec<u32>, targets: bool) {
        if !seen.insert(block) || seen.len() > 64 {
            return;
        }
        let Some(b) = program.blocks.get(block as usize) else { return };
        for op in &b.ops {
            match op {
                omsi_script::Op::Store(id) => {
                    let Some(n) = program.var_names.get(*id as usize).map(|n| n.to_lowercase()) else { continue };
                    for word in ["tuer_", "tür_", "doortarget_"].iter().take(if targets { 3 } else { 2 }) {
                        if let Some(at) = n.find(word) {
                            let digits: String = n[at + word.len()..].chars().take_while(|c| c.is_ascii_digit()).collect();
                            if let Ok(d) = digits.parse::<u32>() {
                                if !out.contains(&d) {
                                    out.push(d);
                                }
                            }
                        }
                    }
                }
                omsi_script::Op::Macro(m) => walk(program, *m, seen, out, targets),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    if let Some(b) = program.trigger(name) {
        walk(program, b, &mut hashbrown::HashSet::new(), &mut out, targets);
    }
    out
}

/// Both lists name doors, and none of the same.
fn numbers_differ(a: &[u32], b: &[u32]) -> bool {
    !a.is_empty() && !b.is_empty() && !a.iter().any(|n| b.contains(n))
}

/// A `bus_doorfront<n>` pair split when its triggers work differently numbered doors (the C2's first and second).
fn split_numbered_doors(program: &omsi_script::Program, groups: Vec<Vec<String>>) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    for g in groups {
        if let [a, b] = g.as_slice() {
            if numbers_differ(&trigger_door_numbers(program, a, false), &trigger_door_numbers(program, b, false)) {
                out.push(vec![a.clone()]);
                out.push(vec![b.clone()]);
                continue;
            }
        }
        out.push(g);
    }
    out
}

/// Every door of a bus, `Shift+1` first: the stock scripts name a door `bus_doorfront<n>`
/// (the SD200/SD202/EN92 have two, both leaves of the one front door) and the aft one
/// `bus_dooraft` (which is also the stop brake release); a low-floor mod with three or four
/// independent doors, like the O530 Facelift, just has more `bus_doorfront<n>` triggers.
/// The keyboard names the passenger door, not each of its leaves.  Most stock and
/// add-on buses expose the two leaves as consecutive `bus_doorfront<n>` triggers.
/// A few older scripts use the second trigger as a *close* command instead; do not
/// combine that one with the opener (A3/BR275 are examples).
/// Sorted by `n` with `bus_dooraft` last. Consecutive leaves are grouped into one physical
/// doorway unless the script clearly names the second trigger as a close command.
fn door_trigger_groups(program: &omsi_script::Program) -> Vec<Vec<String>> {
    let mut fronts: Vec<(u32, String)> = program
        .triggers
        .keys()
        .filter_map(|n| {
            n.strip_prefix("bus_doorfront")
                .and_then(|rest| rest.parse::<u32>().ok())
                .map(|i| (i, n.clone()))
        })
        .collect();
    fronts.sort_by_key(|(i, _)| *i);
    let mut out = Vec::new();
    let mut i = 0;
    while i < fronts.len() {
        let mut group = vec![fronts[i].1.clone()];
        if let Some((next, name)) = fronts.get(i + 1) {
            if *next == fronts[i].0 + 1 && !door_trigger_closes(program, name) {
                group.push(name.clone());
                i += 1;
            }
        }
        out.push(group);
        i += 1;
    }
    if program.trigger("bus_dooraft").is_some() {
        out.push(vec!["bus_dooraft".to_string()]);
    }
    out
}

/// A door trigger that is a close command of its own (the A3's and BR275's second door key:
/// `1 (S.L.CCW_Tuerschliessen)`): a short block that itself stores into a variable named for
/// closing. Looked for in the macros as well, any mention of such a variable took a bus's
/// second door leaf for one - mod door scripts read `door_close_time` and the like while
/// opening - and Shift+1 moved one leaf, Shift+2 the other.
fn door_trigger_closes(program: &omsi_script::Program, name: &str) -> bool {
    let Some(b) = program.trigger(name).and_then(|b| program.blocks.get(b as usize)) else {
        return false;
    };
    if b.ops.len() > 6 || b.ops.iter().any(|op| matches!(op, omsi_script::Op::Macro(_))) {
        return false;
    }
    b.ops.iter().any(|op| match op {
        omsi_script::Op::Store(id) => {
            let n = program.var_names.get(*id as usize).map(|n| n.to_ascii_lowercase()).unwrap_or_default();
            ["close", "closing", "schliess", "schließ"].iter().any(|x| n.contains(x))
        }
        _ => false,
    })
}

/// The variable a door trigger toggles to say where the leaf is going (`doorTarget_0` of
/// the stock door scripts: the trigger's macros store it), if it has one.
pub fn door_trigger_target(program: &omsi_script::Program, name: &str) -> Option<omsi_script::VarId> {
    fn find(program: &omsi_script::Program, block: omsi_script::BlockId, seen: &mut hashbrown::HashSet<omsi_script::BlockId>) -> Option<omsi_script::VarId> {
        if !seen.insert(block) {
            return None;
        }
        let b = program.blocks.get(block as usize)?;
        for op in &b.ops {
            match op {
                omsi_script::Op::Store(id) => {
                    let n = program.var_names.get(*id as usize).map(|n| n.to_ascii_lowercase()).unwrap_or_default();
                    if n.contains("target") || n.contains("soll") {
                        return Some(*id);
                    }
                }
                omsi_script::Op::Macro(m) => {
                    if let Some(v) = find(program, *m, seen) {
                        return Some(v);
                    }
                }
                _ => {}
            }
        }
        None
    }
    program.trigger(name).and_then(|b| find(program, b, &mut hashbrown::HashSet::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(src: &str, vars: &str) -> omsi_script::Program {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("omsi_door_test_{}_{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("t.osc"), src).unwrap();
        std::fs::write(dir.join("v.txt"), vars).unwrap();
        omsi_script::compile(&omsi_script::CompileInput { varlists: vec![dir.join("v.txt")], scripts: vec![dir.join("t.osc")], ..Default::default() })
    }

    #[test]
    fn the_c2s_first_and_second_door_keys_are_not_one_door() {
        // GPM C2: `bus_doorfront0` works door 1, `bus_doorfront1` door 2
        let p = program(
            "{trigger:bus_doorfront0}\n1 (S.L.C2_CP_BT_Tuer_1)\n{end}\n{trigger:bus_doorfront1}\n1 (S.L.C2_CP_BT_Tuer_2)\n{end}\n{trigger:bus_dooraft}\n{end}\n",
            "C2_CP_BT_Tuer_1\nC2_CP_BT_Tuer_2\n",
        );
        assert_eq!(split_numbered_doors(&p, door_trigger_groups(&p)), vec![vec!["bus_doorfront0".to_string()], vec!["bus_doorfront1".to_string()], vec!["bus_dooraft".to_string()]]);
    }

    #[test]
    fn the_two_leaves_of_one_door_stay_one_door_key() {
        // the stock SD202: one door's two leaves, `doorTarget_0` and `_1`
        let p = program("{trigger:bus_doorfront0}\n1 (S.L.doorTarget_0)\n{end}\n{trigger:bus_doorfront1}\n1 (S.L.doorTarget_1)\n{end}\n", "doorTarget_0\ndoorTarget_1\n");
        assert_eq!(split_numbered_doors(&p, door_trigger_groups(&p)), vec![vec!["bus_doorfront0".to_string(), "bus_doorfront1".to_string()]]);
        // (the targets count only where the leaves cannot be placed, the Urbanway 18's rear section)
        let p = program("{trigger:bus_doorfront2}\n1 (S.L.doorTarget_45)\n{end}\n{trigger:bus_doorfront3}\n1 (S.L.doorTarget_67)\n{end}\n", "doorTarget_45\ndoorTarget_67\n");
        assert!(numbers_differ(&trigger_door_numbers(&p, "bus_doorfront2", true), &trigger_door_numbers(&p, "bus_doorfront3", true)));
        assert!(trigger_door_numbers(&p, "bus_doorfront2", false).is_empty());
    }
}
