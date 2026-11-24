//! How far a human type's hands reach along the forearm from the wrist, and where its
//! [links] put the finger joint: the knuckle estimate of `curl_hands` against the mesh.
//! usage: hand_extent <file.hum>
fn main() {
    let p = std::env::args().nth(1).expect("hum file");
    let ty = omsi_sim::human::HumanType::load(std::path::Path::new(&p)).expect("load");
    let rig = &ty.rig;
    let hand_len = (ty.joints.finger - ty.joints.hand).length();
    println!(
        "links hand {:?} finger {:?} -> hand_len {hand_len:.3}",
        ty.joints.hand, ty.joints.finger
    );
    for side in 0..2 {
        let slot = omsi_sim::human::hand_slot(side) as u8;
        let w = rig.wrist[side];
        let u = (rig.wrist[side] - rig.elbow[side]).normalize();
        let mut s: Vec<f32> = Vec::new();
        for m in &ty.meshes {
            for (i, inf) in m.skin.iter().enumerate() {
                if (0..inf.n as usize).any(|k| inf.slot[k] == slot && inf.weight[k] > 0.5) {
                    s.push((m.data.positions[i] - w).dot(u));
                }
            }
        }
        s.sort_by(|a, b| a.total_cmp(b));
        let q = |f: f32| s[((s.len() - 1) as f32 * f) as usize];
        println!(
            "side {side}: {} verts, along the hand from the wrist: min {:.3} 25% {:.3} 50% {:.3} 75% {:.3} max {:.3}",
            s.len(),
            q(0.0),
            q(0.25),
            q(0.5),
            q(0.75),
            q(1.0)
        );
    }
}
