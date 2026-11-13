use glam::Vec2;
use omsi_sim::{VehicleInstance, VehicleType};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use omsi_sim::texttex::TextTextureState;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

const SIGN: &[&str] = &["girouette", "ziel", "matrix", "dest", "annax", "krueger", "kruger", "lawo", "mobitec", "aesys", "buse", "hanover"];
const MAYBE: &[&str] = &["anzeige", "front", "display", "led", "av", "avant", "vorne"];
const NOT_FRONT: &[&str] = &["innen", "inside", "interior", "seite", "side", "lat", "heck", "rear", "back", "hinten", "ar", "arr", "arriere", "ibis", "drucker", "printer", "cover", "leer", "kasten", "tacho", "cockpit", "dash", "dashboard", "conduent", "ecran", "siv", "ineo", "eg3", "kpp", "ticket", "int", "intern", "matrixint"];

fn rank(file: &str) -> Option<i32> {
    let file = file.to_ascii_lowercase();
    let words: Vec<&str> = file.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    if words.iter().any(|w| NOT_FRONT.iter().any(|n| w == n || (n.len() >= 5 && w.contains(n)))) {
        return None;
    }
    let sign = words.iter().any(|w| SIGN.iter().any(|n| w.contains(n))) as i32;
    let maybe = words.iter().any(|w| MAYBE.contains(w)) as i32;
    Some(sign * 2 + maybe)
}

enum Source {
    Mask { script: usize, colour: String, freetex: Option<String> },
    Script(usize),
    Text(usize),
}

struct Part {
    source: Source,
    mesh: usize,
    slot: usize,
    lo: Vec2,
    hi: Vec2,
    x: f32,
    y: f32,
    xs: (f32, f32),
    zs: (f32, f32),
}

struct Group {
    rank: i32,
    y: f32,
    x: f32,
    layers: Vec<Part>,
}

pub struct Sign {
    parts: Vec<Group>,
    jobs: Sender<Vec<Shot>>,
    done: Receiver<Option<Option<Vec<u8>>>>,
    busy: bool,
}

enum Pixels {
    Script(u32, u32, Vec<u8>),
    Text(Box<TextTextureState>, String),
    Mask(u32, u32, Vec<u8>, String),
}

type Shot = (Vec<(Pixels, Vec2, Vec2)>, (f32, f32), (f32, f32));

type Face = (Vec2, Vec2, f32, f32, (f32, f32), (f32, f32));
type Placed = (image::RgbaImage, (f32, f32), (f32, f32));

fn slot_face(data: &omsi_geometry::MeshData, slot: u32) -> Option<Face> {
    let p = |i: u32| data.positions.get(i as usize).copied().unwrap_or_default();
    let (mut indices, mut ahead) = (Vec::new(), Vec::new());
    for &(first, count, _) in data.ranges.iter().filter(|r| r.2 == slot) {
        let Some(tris) = data.indices.get(first as usize..first.saturating_add(count) as usize) else { continue };
        for t in tris.chunks_exact(3) {
            let n = (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0])).normalize_or_zero();
            indices.extend_from_slice(t);
            if n.y.abs() > 0.7 {
                ahead.extend_from_slice(t);
            }
        }
    }
    let chosen = if ahead.is_empty() { indices } else { ahead };
    let centre = |t: &[u32]| (p(t[0]).y + p(t[1]).y + p(t[2]).y) / 3.0;
    let front = chosen.chunks_exact(3).map(centre).fold(f32::MIN, f32::max);
    let chosen: Vec<u32> = chosen.chunks_exact(3).filter(|t| centre(t) > front - 0.3).flatten().copied().collect();
    let ranges = [(0u32, chosen.len() as u32, slot)];
    let face = crate::skins::front_face(&data.positions, &data.uvs, &chosen, &ranges)?;
    let (x, y) = ((face.lo3.x + face.hi3.x) / 2.0, (face.lo3.y + face.hi3.y) / 2.0);
    Some((face.lo, face.hi, x, y, (face.lo3.x, face.hi3.x), (face.lo3.z, face.hi3.z)))
}

fn script_index(s: &str) -> Option<usize> {
    s.trim().strip_prefix("\\S:").or_else(|| s.trim().strip_prefix("\\s:")).and_then(|n| n.trim().parse().ok())
}

fn sample(w: u32, h: u32, data: &[u8], u: f32, v: f32) -> [u8; 4] {
    let x = ((u.rem_euclid(1.0) * w as f32) as u32).min(w.saturating_sub(1));
    let y = ((v.rem_euclid(1.0) * h as f32) as u32).min(h.saturating_sub(1));
    let i = ((y * w + x) * 4) as usize;
    data.get(i..i + 4).map(|p| [p[0], p[1], p[2], p[3]]).unwrap_or([0; 4])
}

fn size_of(lo: Vec2, hi: Vec2, w: u32, h: u32) -> (u32, u32) {
    let pw = (((hi.x - lo.x) * w as f32).round() as u32).clamp(1, 1024);
    let ph = (((hi.y - lo.y) * h as f32).round() as u32).clamp(1, 512);
    (pw, ph)
}

fn compose(mut parts: Vec<Placed>) -> Option<image::RgbaImage> {
    if parts.len() == 1 {
        return parts.pop().map(|p| p.0);
    }
    let (x0, x1) = parts.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.1 .0), a.1.max(p.1 .1)));
    let (z0, z1) = parts.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.2 .0), a.1.max(p.2 .1)));
    let ppm = parts.iter().map(|p| p.0.width() as f32 / (p.1 .1 - p.1 .0).max(0.01)).fold(0.0, f32::max);
    let ppm = ppm.min(1024.0 / (x1 - x0).max(0.01)).min(512.0 / (z1 - z0).max(0.01));
    let (w, h) = ((((x1 - x0) * ppm).round() as u32).clamp(1, 1024), (((z1 - z0) * ppm).round() as u32).clamp(1, 512));
    let mut out = image::RgbaImage::from_pixel(w, h, image::Rgba([0, 0, 0, 255]));
    for (img, xs, zs) in &parts {
        let (pw, ph) = ((((xs.1 - xs.0) * ppm).round() as u32).max(1), (((zs.1 - zs.0) * ppm).round() as u32).max(1));
        let img = image::imageops::resize(img, pw, ph, image::imageops::FilterType::Triangle);
        let (ox, oy) = (((x1 - xs.1) * ppm).round() as u32, ((z1 - zs.1) * ppm).round() as u32);
        for (x, y, p) in img.enumerate_pixels() {
            if let Some(o) = out.get_pixel_mut_checked(ox + x, oy + y) {
                for c in 0..3 {
                    o[c] = o[c].max(p[c]);
                }
            }
        }
    }
    Some(out)
}

impl Sign {
    pub fn describe(&self) -> Vec<String> {
        self.parts
            .iter()
            .flat_map(|g| g.layers.iter().map(move |p| (g.rank, p)))
            .map(|(rank, p)| {
                let what = match &p.source {
                    Source::Mask { script, colour, freetex } => format!("mask S:{script} colour {colour} {freetex:?}"),
                    Source::Script(n) => format!("script {n}"),
                    Source::Text(n) => format!("text {n}"),
                };
                format!("rank {rank} {what} mesh {} uv {:?}-{:?} x {:.2} y {:.2}", p.mesh, p.lo, p.hi, p.x, p.y)
            })
            .collect()
    }

    pub fn find(ty: &VehicleType, root: &Path) -> Option<Sign> {
        let mut parts: Vec<(i32, Part)> = Vec::new();
        for (i, vm) in ty.meshes.iter().enumerate() {
            let Some(def) = ty.model.meshes.get(vm.def_index) else { continue };
            if def.viewpoint == 2 {
                continue;
            }
            let Some(named) = rank(&def.file) else { continue };
            let Some(data) = ty.mesh_data(i) else { continue };
            for o in &vm.overrides {
                let Some(slot) = omsi_sim::vehicle::override_slot(&vm.materials, o) else { continue };
                let source = if let Some(n) = o.transmap.as_deref().and_then(script_index) {
                    Source::Mask { script: n, colour: o.texture.clone(), freetex: o.freetex.as_ref().map(|f| f.1.clone()) }
                } else if let Some(n) = o.use_script_texture.filter(|n| *n >= 0) {
                    Source::Script(n as usize)
                } else if let Some(n) = o.use_text_texture.filter(|n| *n >= 0) {
                    Source::Text(n as usize)
                } else {
                    continue;
                };
                let Some((lo, hi, x, y, xs, zs)) = slot_face(&data, slot as u32) else { continue };
                parts.push((named, Part { source, mesh: i, slot, lo, hi, x, y, xs, zs }));
            }
        }
        let mut groups: Vec<Group> = Vec::new();
        for (rank, p) in parts {
            let same = |q: &Part| q.mesh == p.mesh && (q.lo - p.lo).abs().max_element() < 0.01 && (q.hi - p.hi).abs().max_element() < 0.01;
            match groups.iter_mut().find(|g| g.layers.first().is_some_and(same)) {
                Some(g) => g.layers.push(p),
                None => groups.push(Group { rank, y: p.y, x: p.x, layers: vec![p] }),
            }
        }
        groups.sort_by(|a, b| b.x.total_cmp(&a.x));
        let parts = groups;
        let mut dirs = ty.texture_dirs(root);
        dirs.push(ty.def.dir().to_path_buf());
        if parts.is_empty() {
            return None;
        }
        let (jobs, inbox) = std::sync::mpsc::channel::<Vec<Shot>>();
        let (outbox, done) = std::sync::mpsc::channel();
        let mut painter = Painter { dirs, colours: HashMap::new(), last: 1 };
        std::thread::Builder::new()
            .name("remote cockpit sign".into())
            .spawn(move || {
                while let Ok(shots) = inbox.recv() {
                    if outbox.send(painter.paint(shots)).is_err() {
                        break;
                    }
                }
            })
            .ok()?;
        Some(Sign { parts, jobs, done, busy: false })
    }

    fn shown(&self, v: &VehicleInstance) -> Vec<usize> {
        let visible = |g: &Group| g.layers.first().is_none_or(|p| v.mesh_props.get(p.mesh).is_none_or(|m| m.visible));
        let Some(best) = self.parts.iter().filter(|g| visible(g)).map(|g| g.rank).max() else { return Vec::new() };
        let front = self.parts.iter().filter(|g| visible(g) && g.rank == best).map(|g| g.y).fold(f32::MIN, f32::max);
        (0..self.parts.len()).filter(|&i| visible(&self.parts[i]) && self.parts[i].rank == best && self.parts[i].y > front - 0.3).collect()
    }

    fn snapshot(&self, i: usize, v: &VehicleInstance) -> Shot {
        let g = &self.parts[i];
        let mut layers = Vec::new();
        for p in &g.layers {
            if v.mesh_props.get(p.mesh).and_then(|m| m.slot_alpha.get(p.slot)).is_some_and(|a| *a < 0.05) {
                continue;
            }
            let pixels = match &p.source {
                Source::Script(n) => v.host.script_textures.get(*n).map(|st| Pixels::Script(st.width, st.height, st.rgba.clone())),
                Source::Text(n) => v.text_textures.get(*n).map(|tt| Pixels::Text(Box::new(tt.clone()), tt.last_text.clone().unwrap_or_default())),
                Source::Mask { script, colour, freetex } => {
                    let name = freetex.as_ref().map(|var| v.str_var(var)).filter(|s| !s.trim().is_empty()).unwrap_or_else(|| colour.clone());
                    v.host.script_textures.get(*script).map(|st| Pixels::Mask(st.width, st.height, st.rgba.clone(), name))
                }
            };
            if let Some(px) = pixels {
                let (lo, hi) = (p.lo - p.lo.floor(), p.hi - p.lo.floor());
                layers.push((px, lo, hi));
            }
        }
        (layers, g.layers[0].xs, g.layers[0].zs)
    }

    pub fn render(&mut self, v: &VehicleInstance) -> Option<Option<Vec<u8>>> {
        let mut out = None;
        while let Ok(r) = self.done.try_recv() {
            self.busy = false;
            out = r.or(out);
        }
        if !self.busy {
            let shots: Vec<Shot> = self.shown(v).into_iter().map(|i| self.snapshot(i, v)).collect();
            self.busy = self.jobs.send(shots).is_ok();
        }
        out
    }
}

struct Painter {
    dirs: Vec<PathBuf>,
    colours: HashMap<String, Option<Arc<omsi_texture::Image>>>,
    last: u64,
}

impl Painter {
    fn colour(&mut self, name: &str) -> Option<Arc<omsi_texture::Image>> {
        let key = name.trim().to_ascii_lowercase();
        if let Some(c) = self.colours.get(&key) {
            return c.clone();
        }
        let dirs: Vec<&Path> = self.dirs.iter().map(PathBuf::as_path).collect();
        let img = omsi_texture::find_texture(name.trim(), &dirs).and_then(|p| omsi_texture::decode_file(&p).ok()).map(Arc::new);
        self.colours.insert(key, img.clone());
        img
    }

    fn layer(&mut self, px: &Pixels, lo: Vec2, hi: Vec2) -> image::RgbaImage {
        let at = |x: u32, y: u32, w: u32, h: u32| (lo.x + (x as f32 + 0.5) / w as f32 * (hi.x - lo.x), lo.y + (y as f32 + 0.5) / h as f32 * (hi.y - lo.y));
        let scale = |c: u8, a: u32| (c as u32 * a / 255) as u8;
        match px {
            Pixels::Script(tw, th, rgba) => {
                let (w, h) = size_of(lo, hi, *tw, *th);
                image::RgbaImage::from_fn(w, h, |x, y| {
                    let (u, v) = at(x, y, w, h);
                    let p = sample(*tw, *th, rgba, u, v);
                    image::Rgba([p[0], p[1], p[2], 255])
                })
            }
            Pixels::Text(tt, text) => {
                let rgba = tt.image(text);
                let (tw, th) = (tt.def.width.max(1) as u32, tt.def.height.max(1) as u32);
                let (w, h) = size_of(lo, hi, tw, th);
                image::RgbaImage::from_fn(w, h, |x, y| {
                    let (u, v) = at(x, y, w, h);
                    let p = sample(tw, th, &rgba, u, v);
                    let a = p[3] as u32;
                    image::Rgba([scale(p[0], a), scale(p[1], a), scale(p[2], a), 255])
                })
            }
            Pixels::Mask(tw, th, rgba, name) => {
                let tint = self.colour(name);
                let (w, h) = size_of(lo, hi, *tw, *th);
                image::RgbaImage::from_fn(w, h, |x, y| {
                    let (u, v) = at(x, y, w, h);
                    let a = sample(*tw, *th, rgba, u, v)[3] as u32;
                    let c = tint.as_ref().map_or([255, 176, 0, 255], |t| sample(t.width, t.height, &t.rgba, u, v));
                    image::Rgba([scale(c[0], a), scale(c[1], a), scale(c[2], a), 255])
                })
            }
        }
    }

    fn group(&mut self, layers: &[(Pixels, Vec2, Vec2)]) -> Option<image::RgbaImage> {
        let mut out: Option<image::RgbaImage> = None;
        for (px, lo, hi) in layers {
            let layer = self.layer(px, *lo, *hi);
            match out.as_mut() {
                Some(o) if o.dimensions() == layer.dimensions() => {
                    for (a, b) in o.pixels_mut().zip(layer.pixels()) {
                        for c in 0..3 {
                            a[c] = a[c].saturating_add(b[c]);
                        }
                    }
                }
                Some(_) => {}
                None => out = Some(layer),
            }
        }
        out
    }

    fn paint(&mut self, shots: Vec<Shot>) -> Option<Option<Vec<u8>>> {
        let parts: Vec<Placed> = shots.iter().filter_map(|(layers, xs, zs)| Some((self.group(layers)?, *xs, *zs))).collect();
        let lit = parts.iter().flat_map(|p| p.0.pixels()).filter(|p| p[0].max(p[1]).max(p[2]) > 40).count();
        let total: usize = parts.iter().map(|p| p.0.width() as usize * p.0.height() as usize).sum();
        if lit * 500 < total.max(1) {
            let changed = self.last != 0;
            self.last = 0;
            return changed.then_some(None);
        }
        let hash = parts.iter().flat_map(|p| p.0.as_raw().chunks(8)).fold(1469598103934665603u64, |h, c| (h ^ c.iter().fold(0u64, |a, b| a << 8 | *b as u64)).wrapping_mul(1099511628211)).max(1);
        if hash == self.last {
            return None;
        }
        let out = compose(parts)?;
        self.last = hash;
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(out).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
        Some(Some(png))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mask_slot_names_its_script_texture() {
        assert_eq!(script_index("\\S:1"), Some(1));
        assert_eq!(script_index(" \\S:12 "), Some(12));
        assert_eq!(script_index("matrix_alpha.bmp"), None);
    }

    #[test]
    fn a_french_front_sign_is_found_and_side_ones_are_not() {
        assert_eq!(rank("Spayk\\Girouette\\girouette_av.o3d"), Some(3));
        assert_eq!(rank("Spayk\\Girouette\\girouette_lat.o3d"), None);
        assert_eq!(rank("Spayk\\Girouette\\12m\\girouette_ar.o3d"), None);
        assert_eq!(rank("GPM\\Conduent\\displayST.o3d"), None);
        assert_eq!(rank("matrix_front.o3d"), Some(3));
        assert_eq!(rank("plate.o3d"), Some(0));
    }

    #[test]
    fn samples_wrap_and_stay_inside() {
        let data = [1, 2, 3, 4, 5, 6, 7, 8];
        assert_eq!(sample(2, 1, &data, 0.75, 0.0), [5, 6, 7, 8]);
        assert_eq!(sample(2, 1, &data, 1.25, 0.0), [1, 2, 3, 4]);
        assert_eq!(sample(2, 1, &data, 0.999, 0.999), [5, 6, 7, 8]);
    }
}
