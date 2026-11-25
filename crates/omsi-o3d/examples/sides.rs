//! Which way the faces on each side of a vehicle look: for the triangles at |x| > limit,
//! how many face outwards (by winding, Direct3D's clockwise front) - `sides <limit> <files>`.
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let lim: f32 = a[0].parse().unwrap();
    for p in &a[1..] {
        let Ok(m) = omsi_o3d::load_mesh(std::path::Path::new(p)) else {
            continue;
        };
        let (mut out, mut inn) = (0, 0);
        for t in &m.triangles {
            let v = t.indices.map(|i| m.vertices[i as usize].position);
            let c = (v[0] + v[1] + v[2]) / 3.0;
            if c.x.abs() < lim {
                continue;
            }
            // the front face's normal is against the cross product (see winding.rs)
            let front = -(v[1] - v[0]).cross(v[2] - v[0]);
            if front.x * c.x.signum() > 0.0 {
                out += 1
            } else {
                inn += 1
            }
        }
        println!("{out:6} out {inn:6} in  {p}");
    }
}
