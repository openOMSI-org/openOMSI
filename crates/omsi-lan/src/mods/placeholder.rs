//! What another player's bus is drawn as while it is not here as that bus - being made sure
//! of, downloading, or not to be had: a plain grey box of its size, never another bus (and
//! never this player's own, which said a wrong thing about who drove what). It is a vehicle
//! like any other - written once for each size as a `.bus`, a model and a box mesh in the
//! data folder, and loaded as the game loads every bus - so the traffic, the people and the
//! collisions take its real size, which its player's INFO says (`length`, `width`,
//! `box_offset`, `height`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The size of a bus that says none (an older game): a city bus.
const LENGTH: f32 = 12.0;
const WIDTH: f32 = 2.5;
const HEIGHT: f32 = 3.0;

static MADE: Mutex<Option<HashMap<String, Arc<omsi_sim::VehicleType>>>> = Mutex::new(None);

/// The box for a bus of that size (m; `offset`: how far its centre lies ahead of its origin).
pub(crate) fn of_size(root: &Path, length: f32, width: f32, height: f32, offset: f32) -> Option<Arc<omsi_sim::VehicleType>> {
    let sane = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() && v >= lo && v <= hi { v } else { d };
    let (l, w, h) = (sane(length, 2.0, 60.0, LENGTH), sane(width, 1.0, 8.0, WIDTH), sane(height, 1.0, 10.0, HEIGHT));
    let o = sane(offset, -40.0, 40.0, 0.0);
    // (to a decimetre: one box for every bus of about that size)
    let key = format!("{:.1}x{:.1}x{:.1}{:+.1}", l, w, h, o);
    if let Some(t) = MADE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(&key)) {
        return Some(t.clone());
    }
    let dir = super::sandbox_base()?.with_file_name("placeholders").join(&key);
    let bus = match write(&dir, l, w, h, o) {
        Ok(b) => b,
        Err(e) => {
            log::warn!("placeholder {key}: {e}");
            return None;
        }
    };
    let ty = match omsi_sim::VehicleType::load(root, &bus) {
        Ok(t) => Arc::new(t),
        Err(e) => {
            log::warn!("placeholder {key}: {e}");
            return None;
        }
    };
    MADE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(key, ty.clone());
    Some(ty)
}

/// The placeholder's files in `dir`: its `.bus` (returned), `model/model.cfg`, the box mesh
/// and its grey texture.
fn write(dir: &Path, l: f32, w: f32, h: f32, o: f32) -> std::io::Result<PathBuf> {
    let model = dir.join("model");
    std::fs::create_dir_all(&model)?;
    let bus = dir.join("placeholder.bus");
    std::fs::write(
        &bus,
        format!(
            "[model]\nmodel\\model.cfg\n\n[boundingbox]\n{w:.2}\n{l:.2}\n{h:.2}\n0\n{o:.2}\n{:.2}\n",
            h / 2.0
        ),
    )?;
    std::fs::write(model.join("model.cfg"), "[mesh]\nbox.x\n\n[matl]\nbox.bmp\n0\n")?;
    std::fs::write(model.join("box.x"), box_x(l, w, h, o))?;
    std::fs::write(model.join("box.bmp"), grey_bmp())?;
    Ok(bus)
}

/// The box as a DirectX text mesh (x right, y up, z ahead): its six sides, each both ways
/// round - seen from outside and from inside alike, whichever way the renderer culls.
fn box_x(l: f32, w: f32, h: f32, o: f32) -> String {
    let (x0, x1, y0, y1, z0, z1) = (-w / 2.0, w / 2.0, 0.0, h, o - l / 2.0, o + l / 2.0);
    let v = [[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]];
    let faces = [[0, 1, 2, 3], [5, 4, 7, 6], [4, 0, 3, 7], [1, 5, 6, 2], [3, 2, 6, 7], [4, 5, 1, 0]];
    let mut tris: Vec<[usize; 3]> = Vec::new();
    for f in faces {
        for t in [[f[0], f[1], f[2]], [f[0], f[2], f[3]]] {
            tris.push(t);
            tris.push([t[0], t[2], t[1]]);
        }
    }
    let verts: Vec<String> = v.iter().map(|p| format!(" {:.3};{:.3};{:.3};", p[0], p[1], p[2])).collect();
    let tri: Vec<String> = tris.iter().map(|t| format!(" 3;{},{},{};", t[0], t[1], t[2])).collect();
    let uvs: Vec<String> = v.iter().map(|_| " 0.5;0.5;".to_string()).collect();
    format!(
        "xof 0303txt 0032\nMesh placeholder {{\n {};\n{};\n {};\n{};\n MeshMaterialList {{\n  1;\n  {};\n{};\n  Material {{ 1;1;1;1;; 0; 0;0;0;; 0;0;0;; TextureFilename {{ \"box.bmp\"; }} }}\n }}\n MeshTextureCoords {{\n  {};\n{};\n }}\n}}\n",
        v.len(),
        verts.join(",\n"),
        tris.len(),
        tri.join(",\n"),
        tris.len(),
        tris.iter().map(|_| "  0").collect::<Vec<_>>().join(",\n"),
        v.len(),
        uvs.join(",\n"),
    )
}

/// An 8×8 grey bitmap (24-bit BMP): a colour that says "not here yet", no livery.
fn grey_bmp() -> Vec<u8> {
    let (w, h) = (8u32, 8u32);
    let row = (w * 3).div_ceil(4) * 4;
    let size = 54 + row * h;
    let mut b = Vec::with_capacity(size as usize);
    b.extend_from_slice(b"BM");
    b.extend_from_slice(&size.to_le_bytes());
    b.extend_from_slice(&[0, 0, 0, 0]);
    b.extend_from_slice(&54u32.to_le_bytes());
    b.extend_from_slice(&40u32.to_le_bytes());
    b.extend_from_slice(&w.to_le_bytes());
    b.extend_from_slice(&h.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&24u16.to_le_bytes());
    b.extend_from_slice(&[0; 24]);
    for _ in 0..h {
        for _ in 0..w {
            // (blue, green, red)
            b.extend_from_slice(&[132, 124, 118]);
        }
        b.resize(b.len() + (row - w * 3) as usize, 0);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_box_mesh_is_a_box_of_that_size() {
        let mesh = omsi_o3d::xfile::parse_x(box_x(12.0, 2.5, 3.2, 1.0).as_bytes()).unwrap();
        assert_eq!(mesh.vertices.len(), 8);
        assert_eq!(mesh.triangles.len(), 24);
        let xs: Vec<f32> = mesh.vertices.iter().map(|v| v.position.x).collect();
        assert!(xs.iter().all(|x| (x.abs() - 1.25).abs() < 1e-3));
        let img = image::load_from_memory(&grey_bmp()).unwrap().to_rgb8();
        assert_eq!((img.width(), img.height()), (8, 8));
        assert_eq!(img.get_pixel(3, 3).0, [118, 124, 132]);
    }

    /// Loaded as every bus is (`OMSI_ROOT`, for the game's variable lists): a bus with no
    /// script, its box of the size asked.
    #[test]
    fn it_loads_as_a_vehicle_of_its_size() {
        let Some(root) = std::env::var_os("OMSI_ROOT").map(PathBuf::from) else { return };
        let ty = of_size(&root, 18.0, 2.55, 3.1, -2.0).unwrap();
        assert_eq!(ty.def.bounding_box.map(|b| [b[0], b[1], b[2], b[4]]), Some([2.55, 18.0, 3.1, -2.0]));
        assert_eq!(ty.meshes.len(), 1);
        // the same size again: the one made already
        assert!(Arc::ptr_eq(&ty, &of_size(&root, 18.02, 2.55, 3.1, -2.0).unwrap()));
    }
}

