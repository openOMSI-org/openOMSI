use glam::{Vec2, Vec3};
use omsi_sim::VehicleType;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_SIDE: u32 = 192;
const MIN_SIDE: u32 = 16;

pub(crate) struct Face {
    pub(crate) slot: u32,
    pub(crate) lo: Vec2,
    pub(crate) hi: Vec2,
    pub(crate) lo3: Vec3,
    pub(crate) hi3: Vec3,
    area: f32,
}

struct Tri {
    v: [usize; 3],
    key: (i32, i32, i32, u32),
    area: f32,
}

fn root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

pub(crate) fn front_face(positions: &[Vec3], uvs: &[Vec2], indices: &[u32], ranges: &[(u32, u32, u32)]) -> Option<Face> {
    let mut tris = Vec::new();
    for &(first, count, slot) in ranges {
        let Some(list) = indices.get(first as usize..first.saturating_add(count) as usize) else { continue };
        for t in list.chunks_exact(3) {
            let v = [t[0], t[1], t[2]].map(|i| i as usize);
            if v.iter().any(|&i| i >= positions.len() || i >= uvs.len()) {
                continue;
            }
            let cross = (positions[v[1]] - positions[v[0]]).cross(positions[v[2]] - positions[v[0]]);
            let area = cross.length() * 0.5;
            if area <= f32::EPSILON {
                continue;
            }
            let n = cross / (area * 2.0);
            tris.push(Tri { v, key: ((n.x * 4.0).round() as i32, (n.y * 4.0).round() as i32, (n.z * 4.0).round() as i32, slot), area });
        }
    }
    let mut by_side: HashMap<(i32, i32, i32, u32), f32> = HashMap::new();
    for t in &tris {
        *by_side.entry(t.key).or_default() += t.area;
    }
    let side = by_side.into_iter().max_by(|a, b| a.1.total_cmp(&b.1))?.0;
    let mut parent: Vec<usize> = (0..positions.len()).collect();
    let mut uv_of: HashMap<(i64, i64), usize> = HashMap::new();
    for t in tris.iter().filter(|t| t.key == side) {
        for &i in &t.v {
            let k = ((uvs[i].x * 8192.0).round() as i64, (uvs[i].y * 8192.0).round() as i64);
            let first = *uv_of.entry(k).or_insert(i);
            let (a, b) = (root(&mut parent, first), root(&mut parent, i));
            parent[a] = b;
        }
        let (a, b, c) = (root(&mut parent, t.v[0]), root(&mut parent, t.v[1]), root(&mut parent, t.v[2]));
        parent[a] = c;
        parent[b] = c;
    }
    let mut islands: HashMap<usize, Face> = HashMap::new();
    for t in tris.iter().filter(|t| t.key == side) {
        let r = root(&mut parent, t.v[0]);
        let lo = uvs[t.v[0]].min(uvs[t.v[1]]).min(uvs[t.v[2]]);
        let hi = uvs[t.v[0]].max(uvs[t.v[1]]).max(uvs[t.v[2]]);
        let p = t.v.map(|i| positions[i]);
        let f = islands.entry(r).or_insert(Face { slot: side.3, lo, hi, lo3: p[0], hi3: p[0], area: 0.0 });
        f.lo = f.lo.min(lo);
        f.hi = f.hi.max(hi);
        f.lo3 = f.lo3.min(p[0]).min(p[1]).min(p[2]);
        f.hi3 = f.hi3.max(p[0]).max(p[1]).max(p[2]);
        f.area += t.area;
    }
    islands.into_values().max_by(|a, b| a.area.total_cmp(&b.area))
}

fn region(img: &omsi_texture::Image, lo: Vec2, hi: Vec2) -> Option<image::RgbaImage> {
    let shift = lo.floor();
    let (lo, hi) = (lo - shift, hi - shift);
    if hi.x > 1.001 || hi.y > 1.001 {
        return None;
    }
    let (w, h) = (img.width as f32, img.height as f32);
    let x0 = (lo.x * w).floor().max(0.0) as u32;
    let y0 = (lo.y * h).floor().max(0.0) as u32;
    let x1 = ((hi.x * w).ceil() as u32).min(img.width);
    let y1 = ((hi.y * h).ceil() as u32).min(img.height);
    if x1.saturating_sub(x0) < MIN_SIDE || y1.saturating_sub(y0) < MIN_SIDE {
        return None;
    }
    let full = image::RgbaImage::from_raw(img.width, img.height, img.rgba.clone())?;
    let mut part = image::imageops::crop_imm(&full, x0, y0, x1 - x0, y1 - y0).to_image();
    for px in part.pixels_mut() {
        px[3] = 255;
    }
    Some(part)
}

fn plain(img: &image::RgbaImage) -> bool {
    let n = (img.width() * img.height()).max(1) as f64;
    let (mut sum, mut sq) = (0f64, 0f64);
    for p in img.pixels() {
        let l = 0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64;
        sum += l;
        sq += l * l;
    }
    let mean = sum / n;
    let spread = (sq / n - mean * mean).max(0.0).sqrt();
    mean < 14.0 || spread < 7.0
}

fn png(mut part: image::RgbaImage) -> Option<Vec<u8>> {
    let (pw, ph) = part.dimensions();
    if pw.max(ph) > MAX_SIDE {
        let k = MAX_SIDE as f32 / pw.max(ph) as f32;
        part = image::imageops::resize(&part, ((pw as f32 * k) as u32).max(1), ((ph as f32 * k) as u32).max(1), image::imageops::FilterType::Triangle);
    }
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(part).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).ok()?;
    Some(out)
}

fn lit(day: &image::RgbaImage, night: &image::RgbaImage) -> Option<image::RgbaImage> {
    let night = image::imageops::resize(night, day.width(), day.height(), image::imageops::FilterType::Triangle);
    let mut out = day.clone();
    let mut glow = 0u64;
    for (o, n) in out.pixels_mut().zip(night.pixels()) {
        for c in 0..3 {
            glow += n[c] as u64;
            o[c] = o[c].saturating_add(n[c]);
        }
    }
    (glow > (day.width() * day.height()) as u64 * 3 * 8).then_some(out)
}

fn generated(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase().replace('\\', "/");
    let stem = Path::new(&n).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    stem.is_empty() || stem == "null" || stem.starts_with("reflexion")
}

struct Textures<'a> {
    dirs: Vec<&'a Path>,
    subst: HashMap<String, String>,
    decoded: HashMap<PathBuf, Option<Arc<omsi_texture::Image>>>,
}

impl Textures<'_> {
    fn get(&mut self, name: &str) -> Option<Arc<omsi_texture::Image>> {
        if generated(name) {
            return None;
        }
        let name = self.subst.get(&name.trim().to_ascii_lowercase()).cloned().unwrap_or_else(|| name.to_string());
        let path = omsi_texture::find_texture(&name, &self.dirs)?;
        self.decoded.entry(path.clone()).or_insert_with(|| omsi_texture::decode_file(&path).ok().map(Arc::new)).clone()
    }
}

fn night_name(vm: &omsi_sim::vehicle::VehicleMesh, slot: usize) -> Option<String> {
    let mine: Vec<&omsi_model::MaterialDef> = vm.overrides.iter().filter(|o| omsi_sim::vehicle::override_slot(&vm.materials, o) == Some(slot)).collect();
    let pick = |o: &omsi_model::MaterialDef| o.lightmaps.last().map(|l| l.0.clone()).or_else(|| o.nightmap.clone());
    mine.iter().filter(|o| o.item).find_map(|o| pick(o)).or_else(|| mine.iter().find_map(|o| pick(o)))
}

pub fn extract(ty: &VehicleType, root: &Path, scheme: Option<usize>) -> Vec<(String, Vec<u8>)> {
    let dirs = ty.texture_dirs(root);
    let subst = match scheme {
        Some(i) => ty.scheme_substitutions(i, omsi_texture::find_texture).0,
        None => ty.default_substitutions(root),
    };
    let mut tex = Textures {
        dirs: dirs.iter().map(PathBuf::as_path).collect(),
        subst: subst.into_iter().collect(),
        decoded: HashMap::new(),
    };
    let mut best: HashMap<String, (f32, Vec<u8>, Option<Vec<u8>>)> = HashMap::new();
    for (i, vm) in ty.meshes.iter().enumerate() {
        let Some(def) = ty.model.meshes.get(vm.def_index) else { continue };
        let Some(event) = def.mouse_event.as_deref().map(str::trim).filter(|e| !e.is_empty()) else { continue };
        let Some(data) = ty.mesh_data(i) else { continue };
        let Some(face) = front_face(&data.positions, &data.uvs, &data.indices, &data.ranges) else { continue };
        if best.get(event).is_some_and(|(area, ..)| *area >= face.area) {
            continue;
        }
        let slot = face.slot as usize;
        let Some(material) = vm.materials.get(slot) else { continue };
        let scripted = vm.overrides.iter().any(|o| omsi_sim::vehicle::override_slot(&vm.materials, o) == Some(slot) && (o.use_script_texture.is_some() || o.use_text_texture.is_some()));
        if scripted || ty.texchange(&material.texture).is_some() {
            continue;
        }
        let Some(day) = tex.get(&material.texture).and_then(|img| region(&img, face.lo, face.hi)).filter(|d| !plain(d)) else { continue };
        let night = night_name(vm, slot).and_then(|n| tex.get(&n)).and_then(|img| region(&img, face.lo, face.hi)).and_then(|n| lit(&day, &n)).and_then(png);
        if let Some(day) = png(day) {
            best.insert(event.to_string(), (face.area, day, night));
        }
    }
    let mut out = Vec::new();
    for (k, (_, day, night)) in best {
        if let Some(n) = night {
            out.push((format!("{k}.lit"), n));
        }
        out.push((k, day));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_largest_island_of_the_largest_flat_side_is_the_face() {
        let positions = vec![
            Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 0.0), Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0), Vec3::new(2.5, 0.0, 0.0), Vec3::new(2.5, 0.5, 0.0),
            Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 0.0, -0.1),
        ];
        let uvs = vec![
            Vec2::new(0.25, 0.25), Vec2::new(0.5, 0.25), Vec2::new(0.5, 0.5), Vec2::new(0.25, 0.5),
            Vec2::new(0.7, 0.7), Vec2::new(0.8, 0.7), Vec2::new(0.8, 0.8),
            Vec2::new(0.9, 0.9), Vec2::new(1.0, 0.9), Vec2::new(1.0, 1.0),
        ];
        let indices = vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 7, 8, 9];
        let f = front_face(&positions, &uvs, &indices, &[(0, 12, 0)]).unwrap();
        assert_eq!((f.lo, f.hi), (Vec2::new(0.25, 0.25), Vec2::new(0.5, 0.5)));
    }

    #[test]
    fn a_face_is_cut_out_of_its_texture() {
        let mut rgba = vec![0u8; 64 * 64 * 4];
        for y in 16..32 {
            for x in 16..32 {
                rgba[(y * 64 + x) * 4] = 255;
            }
        }
        let img = omsi_texture::Image { width: 64, height: 64, rgba, has_alpha: false };
        let part = region(&img, Vec2::new(1.25, 0.25), Vec2::new(1.5, 0.5)).unwrap();
        let back = image::load_from_memory(&png(part).unwrap()).unwrap().to_rgba8();
        assert_eq!(back.dimensions(), (16, 16));
        assert!(back.pixels().all(|p| p[0] == 255 && p[3] == 255));
        assert!(region(&img, Vec2::new(0.0, 0.0), Vec2::new(0.1, 0.1)).is_none());
        assert!(region(&img, Vec2::new(0.5, 0.5), Vec2::new(1.7, 0.9)).is_none());
    }

    #[test]
    fn black_or_flat_faces_are_left_out() {
        assert!(plain(&image::RgbaImage::from_pixel(8, 8, image::Rgba([5, 5, 5, 255]))));
        assert!(plain(&image::RgbaImage::from_pixel(8, 8, image::Rgba([128, 128, 128, 255]))));
        let mut pictogram = image::RgbaImage::from_pixel(8, 8, image::Rgba([30, 30, 30, 255]));
        for x in 2..6 {
            pictogram.put_pixel(x, 4, image::Rgba([240, 240, 240, 255]));
        }
        assert!(!plain(&pictogram));
    }

    #[test]
    fn a_night_map_lights_the_face_only_where_it_glows() {
        let day = image::RgbaImage::from_pixel(4, 4, image::Rgba([40, 40, 40, 255]));
        let dark = image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255]));
        assert!(lit(&day, &dark).is_none());
        let glow = image::RgbaImage::from_pixel(4, 4, image::Rgba([250, 0, 0, 255]));
        assert_eq!(lit(&day, &glow).unwrap().get_pixel(0, 0).0, [255, 40, 40, 255]);
    }
}
