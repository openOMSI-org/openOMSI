//! For each o3d: its matrix and the box its vertices span - `bounds <files>`.
fn main() {
    for p in std::env::args().skip(1) {
        let Ok(m) = omsi_o3d::load_mesh(std::path::Path::new(&p)) else {
            continue;
        };
        let (mut lo, mut hi) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
        for v in &m.vertices {
            lo = lo.min(v.position);
            hi = hi.max(v.position);
        }
        let t = m.transform.to_cols_array();
        println!("{p}\n  box {lo:.3?} .. {hi:.3?}\n  matrix {:.3?}", t);
    }
}
