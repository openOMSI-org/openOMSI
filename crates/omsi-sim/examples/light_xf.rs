//! The transform each light-carrying mesh of a vehicle gets (for checking where its
//! [light_enh] coronas land). usage: light_xf <content folder> <bus>
fn main() {
    omsi_sim::image_decode::install(decode_picture);
    let a: Vec<String> = std::env::args().collect();
    let root = std::path::PathBuf::from(omsi_cfg::flags::OMSI_ROOT.live_var().unwrap());
    omsi_cfg::add_content_root(std::path::PathBuf::from(&a[1]));
    omsi_cfg::add_content_root(root.clone());
    let path = omsi_cfg::resolve_path(std::path::Path::new(&a[1]), &a[2]);
    let vt = std::sync::Arc::new(omsi_sim::VehicleType::load(&root, &path).unwrap());
    let mut v =
        omsi_sim::VehicleInstance::new(vt.clone(), omsi_sim::VehicleHost::new(Default::default()));
    for _ in 0..30 {
        v.update(1.0 / 30.0);
    }
    let inv = v.body_rotation().inverse();
    for (i, md) in vt.model.meshes.iter().enumerate() {
        if md.light_enh.is_empty() && md.light_enh_2.is_empty() {
            continue;
        }
        match vt.meshes.iter().position(|m| m.def_index == i) {
            Some(k) => {
                let xf = inv * v.mesh_local_transform(k);
                let t = xf.w_axis.truncate();
                let l = md
                    .light_enh_2
                    .first()
                    .map(|l| l.pos)
                    .or(md.light_enh.first().map(|l| l.pos))
                    .unwrap();
                let p = xf.transform_point3(glam::Vec3::from(l));
                println!(
                    "{:40} translation ({:.2},{:.2},{:.2})  light {:?} -> ({:.2},{:.2},{:.2})",
                    md.file, t.x, t.y, t.z, l, p.x, p.y, p.z
                );
            }
            None => println!("{:40} not loaded", md.file),
        }
    }
}

/// The game's picture decoder for the simulation (as omsi-app registers it at start).
fn decode_picture(path: &std::path::Path) -> Result<omsi_sim::image_decode::DecodedImage, String> {
    omsi_texture::decode_file(path)
        .map(|i| omsi_sim::image_decode::DecodedImage {
            width: i.width,
            height: i.height,
            rgba: i.rgba,
            has_alpha: i.has_alpha,
        })
        .map_err(|e| e.to_string())
}
