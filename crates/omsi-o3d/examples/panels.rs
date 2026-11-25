//! The flat parts of a mesh at one side: clusters of triangles at x beyond a limit, as
//! y/z boxes - `panels <x limit, negative for the left> <file>`.
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let lim: f32 = a[0].parse().unwrap();
    let m = omsi_o3d::load_mesh(std::path::Path::new(&a[1])).unwrap();
    let mut boxes: Vec<[f32; 4]> = Vec::new(); // zmin zmax ymin ymax
    for t in &m.triangles {
        let v = t.indices.map(|i| m.vertices[i as usize].position);
        if !v
            .iter()
            .all(|p| if lim > 0.0 { p.x > lim } else { p.x < lim })
        {
            continue;
        }
        let b = [
            v.iter().map(|p| p.z).fold(f32::MAX, f32::min),
            v.iter().map(|p| p.z).fold(f32::MIN, f32::max),
            v.iter().map(|p| p.y).fold(f32::MAX, f32::min),
            v.iter().map(|p| p.y).fold(f32::MIN, f32::max),
        ];
        if let Some(o) = boxes.iter_mut().find(|o| {
            b[0] <= o[1] + 0.01 && b[1] >= o[0] - 0.01 && b[2] <= o[3] + 0.01 && b[3] >= o[2] - 0.01
        }) {
            o[0] = o[0].min(b[0]);
            o[1] = o[1].max(b[1]);
            o[2] = o[2].min(b[2]);
            o[3] = o[3].max(b[3]);
        } else {
            boxes.push(b);
        }
    }
    boxes.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
    for b in boxes {
        println!("z {:7.3}..{:7.3}  y {:6.3}..{:6.3}", b[0], b[1], b[2], b[3]);
    }
}
