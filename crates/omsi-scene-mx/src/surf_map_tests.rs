use super::*;
use std::io::Write;

fn bmp(red: u8) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::RgbImage::from_pixel(2, 2, image::Rgb([red, 0, 0]))
        .write_to(&mut out, image::ImageFormat::Bmp)
        .unwrap();
    out.into_inner()
}

#[test]
fn surf_map_beside_a_texture_in_a_mounted_archive() {
    let dir = std::env::temp_dir().join(format!("openomsi-surf-zip-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let zip_path = dir.join("roads.zip");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
    for (name, data) in [
        ("Splines/Roads/texture/cobbles.bmp", bmp(128)),
        ("Splines/Roads/texture/cobbles.bmp.surf", bmp(255)),
    ] {
        z.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(&data).unwrap();
    }
    z.finish().unwrap();
    let mount = omsi_cfg::vfs::mount_zip(&zip_path).unwrap();
    let tex = mount.join("Splines").join("Roads").join("texture");
    // only the archive holds it: nothing on the disk beside the zip
    assert!(!tex.join("cobbles.bmp.surf").is_file());
    let map = surf_map("cobbles.bmp", &[&tex]).expect("the .surf in the archive");
    assert!((map.lift(glam::Vec2::new(0.5, 0.5)) - 0.02).abs() < 1e-3);
    std::fs::remove_dir_all(&dir).ok();
}

/// A road's `.bmp` is found in the global `Texture` folder while its `.surf` sits in the
/// spline pack's own texture folder. OMSI still shakes the bus there; only looking beside the
/// picture left the road smooth (#3502).
#[test]
fn surf_map_is_found_in_an_earlier_texture_folder_than_its_picture() {
    let dir = std::env::temp_dir().join(format!("openomsi-surf-folders-{}", std::process::id()));
    let local = dir.join("Splines/RoadPack/texture");
    let global = dir.join("Texture");
    std::fs::create_dir_all(&local).unwrap();
    std::fs::create_dir_all(&global).unwrap();
    let name = format!("road_{}_3502.bmp", std::process::id());
    std::fs::write(global.join(&name), bmp(128)).unwrap();
    std::fs::write(local.join(format!("{name}.surf")), bmp(0)).unwrap();
    let dirs = [local.as_path(), global.as_path()];
    let map = surf_map(&name, &dirs).expect("the .surf in the spline pack");
    assert!(
        (map.lift(glam::Vec2::ZERO) + omsi_geometry::HeightMap::AMPLITUDE).abs() < 1e-3,
        "black should lower the road by the full amplitude"
    );
    std::fs::remove_dir_all(&dir).ok();
}
