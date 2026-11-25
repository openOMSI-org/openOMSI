//! Bounding boxes of a vehicle's meshes in the vehicle frame, as drawn at rest (all
//! animations and pivots applied): where a part really sits compared with the body.
//! usage: mesh_bbox <content folder> <bus relative to a root> [name filter]
//! (the OMSI 2 installation from $OMSI_ROOT)
use std::path::PathBuf;
fn main() {
    omsi_sim::image_decode::install(decode_picture);
    let a: Vec<String> = std::env::args().collect();
    let content = PathBuf::from(&a[1]);
    omsi_cfg::add_content_root(content.clone());
    omsi_cfg::vfs::mount_dir_zips(&content.join("Archives"));
    let orig = omsi_cfg::flags::OMSI_ROOT
        .live_var()
        .map(PathBuf::from)
        .unwrap_or(content.clone());
    omsi_cfg::add_content_root(orig.clone());
    let bus = omsi_cfg::resolve_path(&orig, &a[2]);
    let filter = a.get(3).map(|s| s.to_ascii_lowercase());
    let vt = std::sync::Arc::new(omsi_sim::VehicleType::load(&orig, &bus).unwrap());
    let mut v =
        omsi_sim::VehicleInstance::new(vt.clone(), omsi_sim::VehicleHost::new(Default::default()));
    // MESH_BBOX_SET=var=value,... sets variables first (an opened window, a door)
    if let Ok(set) = std::env::var("MESH_BBOX_SET") {
        for kv in set.split(',') {
            if let Some((k, x)) = kv.split_once('=') {
                v.set_var(k, x.parse().unwrap_or(0.0));
            }
        }
    }
    for _ in 0..60 {
        v.update(1.0 / 30.0);
    }
    for (i, m) in vt.meshes.iter().enumerate() {
        let def = &vt.model.meshes[m.def_index];
        if let Some(f) = &filter {
            if !def.file.to_ascii_lowercase().contains(f.as_str()) {
                continue;
            }
        }
        let xf = v.mesh_transforms[i];
        let (mut lo, mut hi) = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
        for p in &m.data.positions {
            let q = xf.transform_point3(*p);
            lo = lo.min(q);
            hi = hi.max(q);
        }
        println!(
            "{:3} {:40} x {:6.2}..{:6.2}  y {:6.2}..{:6.2}  z {:5.2}..{:5.2}  anims {} visible {}",
            i,
            def.file,
            lo.x,
            hi.x,
            lo.y,
            hi.y,
            lo.z,
            hi.z,
            def.animations.len(),
            v.mesh_props[i].visible
        );
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
