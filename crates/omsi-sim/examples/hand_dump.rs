//! Where a human's right hand lies in its T-pose: the vertices that follow the hand bone,
//! binned along the arm, with their spread across (y) and up (z) - `hand_dump <file.hum>`.
use omsi_sim::human::HumanType;
fn main() {
    let p = std::env::args().nth(1).unwrap();
    let t = HumanType::load(std::path::Path::new(&p)).unwrap();
    println!(
        "wrist R {:?} finger joint {:?}",
        t.rig.wrist[1], t.joints.finger
    );
    let mut pts = Vec::new();
    for m in &t.meshes {
        for (i, inf) in m.skin.iter().enumerate() {
            let best = (0..inf.n as usize)
                .max_by(|a, b| inf.weight[*a].total_cmp(&inf.weight[*b]))
                .map(|k| inf.slot[k]);
            if best == Some(12) {
                pts.push(m.data.positions[i]);
            }
        }
    }
    println!("{} hand vertices", pts.len());
    let x0 = pts.iter().map(|p| p.x).fold(f32::MAX, f32::min);
    let x1 = pts.iter().map(|p| p.x).fold(f32::MIN, f32::max);
    let n = 12;
    for b in 0..n {
        let lo = x0 + (x1 - x0) * b as f32 / n as f32;
        let hi = x0 + (x1 - x0) * (b + 1) as f32 / n as f32;
        let s: Vec<_> = pts
            .iter()
            .filter(|p| p.x >= lo && p.x < hi + 1e-4)
            .collect();
        if s.is_empty() {
            continue;
        }
        let (y0, y1) = (
            s.iter().map(|p| p.y).fold(f32::MAX, f32::min),
            s.iter().map(|p| p.y).fold(f32::MIN, f32::max),
        );
        let (z0, z1) = (
            s.iter().map(|p| p.z).fold(f32::MAX, f32::min),
            s.iter().map(|p| p.z).fold(f32::MIN, f32::max),
        );
        println!(
            "x {:.3}..{:.3}: {:4} verts, y {:.3}..{:.3}, z {:.3}..{:.3}",
            lo,
            hi,
            s.len(),
            y0,
            y1,
            z0,
            z1
        );
    }
}
