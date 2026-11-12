use omsi_sim::{VehicleHost, VehicleInstance, VehicleType};
use std::sync::Arc;

fn leaf(var: &str) -> Option<usize> {
    var.trim().to_ascii_lowercase().strip_prefix("door_")?.parse().ok()
}

fn median(mut v: Vec<f32>) -> Option<f32> {
    v.sort_by(f32::total_cmp);
    v.get(v.len() / 2).copied()
}

pub fn doorways(ty: &VehicleType) -> Vec<Vec<usize>> {
    let mut hinges: Vec<(usize, Vec<f32>, Vec<f32>)> = Vec::new();
    for (i, vm) in ty.meshes.iter().enumerate() {
        let Some(def) = ty.model.meshes.get(vm.def_index) else { continue };
        let Some((n, anim)) = def.animations.iter().find_map(|a| Some((leaf(&a.variable)?, a))) else { continue };
        let at = match hinges.iter().position(|e| e.0 == n) {
            Some(k) => k,
            None => {
                hinges.push((n, Vec::new(), Vec::new()));
                hinges.len() - 1
            }
        };
        let hinge = anim.origins.iter().find_map(|o| match o {
            omsi_model::AnimOrigin::Trans(t) => Some(t[1]),
            _ => None,
        });
        match hinge {
            Some(y) => hinges[at].1.push(y),
            None => {
                if let Some(d) = ty.mesh_data(i).filter(|d| !d.positions.is_empty()) {
                    hinges[at].2.push(d.positions.iter().map(|p| vm.pivot.transform_point3(*p).y).sum::<f32>() / d.positions.len() as f32);
                }
            }
        }
    }
    let mut leaves: Vec<(usize, f32)> = hinges.into_iter().filter_map(|(n, h, c)| Some((n, if h.is_empty() { median(c)? } else { median(h)? }))).collect();
    leaves.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut ways: Vec<Vec<(usize, f32)>> = Vec::new();
    for l in leaves {
        match ways.last_mut() {
            Some(w) if w.last().is_some_and(|p| (p.1 - l.1).abs() < 1.6) => w.push(l),
            _ => ways.push(vec![l]),
        }
    }
    ways.into_iter().map(|w| w.into_iter().map(|(n, _)| n).collect()).collect()
}

fn opened(ty: &Arc<VehicleType>, first: &[&str], press: Option<&str>, last: usize) -> Vec<usize> {
    let mut v = VehicleInstance::new(ty.clone(), VehicleHost::new(omsi_sim::SimClock::default()));
    let mut start = omsi_sim::startup::StartUp::new(&v, &[]);
    for _ in 0..1500 {
        start.tick(&mut v, &[], 0.02);
        v.update(0.02);
    }
    for name in first {
        v.trigger(name);
        for _ in 0..50 {
            v.update(0.02);
        }
    }
    if let Some(name) = press {
        v.trigger(name);
    }
    let mut out = Vec::new();
    for frame in 0..200 {
        if let (10, Some(name)) = (frame, press) {
            let off = format!("{name}_off");
            if ty.program.trigger(&off).is_some() {
                v.trigger(&off);
            }
        }
        v.update(0.02);
        for leaf in 0..=last {
            if !out.contains(&leaf) && v.var(&format!("door_{leaf}")).is_some_and(|x| x > 0.05) {
                out.push(leaf);
            }
        }
    }
    out.sort_unstable();
    out
}

fn one_doorway(a: &[usize], b: &[usize], ways: &[Vec<usize>]) -> bool {
    let at: Vec<Option<usize>> = a.iter().chain(b).map(|l| ways.iter().position(|w| w.contains(l))).collect();
    if at.iter().all(Option::is_some) {
        return at.windows(2).all(|p| p[0] == p[1]);
    }
    matches!((a, b), ([x], [y]) if *y == x + 1 && x % 2 == 0)
}

pub fn tried_doors(ty: Arc<VehicleType>, ways: &[Vec<usize>]) -> Vec<(Vec<String>, Vec<usize>)> {
    let mut keys: Vec<(u32, String)> = ty.program.triggers.keys().filter_map(|n| Some((n.strip_prefix("bus_doorfront")?.parse().ok()?, n.clone()))).collect();
    keys.sort();
    let mut names: Vec<String> = keys.into_iter().map(|(_, n)| n).collect();
    if ty.program.trigger("bus_dooraft").is_some() {
        names.push("bus_dooraft".into());
    }
    let stock = |t: &String| t.split('|').all(|n| n.starts_with("bus_doorfront") || n == "bus_dooraft");
    if names.is_empty() || !omsi_sim::door_keys::door_keys(&ty).iter().flatten().all(stock) {
        return Vec::new();
    }
    let last = ways.iter().flatten().copied().max().unwrap_or(0).max(11);
    let still = opened(&ty, &[], None, last);
    let released: Vec<&str> = if ty.program.trigger("bus_dooraft").is_some() { vec!["bus_dooraft"] } else { Vec::new() };
    let still_released = opened(&ty, &released, None, last);
    let mut buttons: Vec<(Vec<String>, Vec<usize>)> = Vec::new();
    for name in names {
        let mut leaves: Vec<usize> = opened(&ty, &[], Some(&name), last).into_iter().filter(|l| !still.contains(l)).collect();
        if leaves.is_empty() && !released.is_empty() && name != "bus_dooraft" {
            leaves = opened(&ty, &released, Some(&name), last).into_iter().filter(|l| !still_released.contains(l)).collect();
        }
        if leaves.is_empty() {
            if name.starts_with("bus_doorfront") {
                return Vec::new();
            }
            continue;
        }
        if name == "bus_dooraft" && leaves.iter().all(|l| buttons.iter().any(|b| b.1.contains(l))) {
            continue;
        }
        match buttons.last_mut() {
            Some((t, l)) if t.len() == 1 && !l.iter().any(|x| leaves.contains(x)) && one_doorway(l, &leaves, ways) => {
                t.push(name);
                l.extend(leaves);
                l.sort_unstable();
            }
            _ => buttons.push((vec![name], leaves)),
        }
    }
    let left: Vec<usize> = ways.iter().flatten().filter(|l| !buttons.iter().any(|b| b.1.contains(l))).copied().collect();
    if !buttons.is_empty() && !left.is_empty() && !released.is_empty() && !buttons.iter().any(|b| b.0.iter().any(|t| t == "bus_dooraft")) {
        buttons.push((vec!["bus_dooraft".into()], left));
    }
    let order: Vec<usize> = ways.iter().flatten().copied().collect();
    buttons.sort_by_key(|b| b.1.iter().map(|l| order.iter().position(|o| o == l).unwrap_or(order.len() + l)).min());
    buttons.truncate(6);
    buttons
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_leaf_is_a_numbered_door_variable() {
        assert_eq!(super::leaf("door_3"), Some(3));
        assert_eq!(super::leaf("Door_0"), Some(0));
        assert_eq!(super::leaf("door_2L"), None);
        assert_eq!(super::leaf("doorlight"), None);
    }

    #[test]
    fn two_door_keys_are_one_door_only_when_they_move_its_two_leaves() {
        assert!(super::one_doorway(&[0], &[1], &[]));
        assert!(!super::one_doorway(&[1], &[2], &[]));
        assert!(!super::one_doorway(&[0], &[1], &[vec![0], vec![1]]));
        assert!(super::one_doorway(&[0], &[1], &[vec![0, 1], vec![2, 3]]));
        assert!(!super::one_doorway(&[0, 1], &[2, 3], &[vec![0, 1], vec![2, 3]]));
    }
}
