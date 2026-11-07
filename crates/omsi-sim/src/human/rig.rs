//! The skeleton of a human type: its joints, its meshes with their bone weights, and the rig
//! measured from them when the type is loaded.

use super::*;

/// Joint positions in the model frame (x right, y forward, z up) of the right side.
#[derive(Debug, Clone, Copy)]
pub struct Joints {
    pub hip: Vec3,
    pub knee: Vec3,
    pub waist: Vec3,
    pub shoulder: Vec3,
    pub elbow: Vec3,
    pub neck: Vec3,
    pub hand: Vec3,
    pub finger: Vec3,
}

impl Joints {
    pub fn from_links(l: &[f32]) -> Joints {
        let g = |i: usize| l.get(i).copied().filter(|v| v.is_finite()).unwrap_or(0.0);
        Joints {
            hip: Vec3::new(g(0), g(1), g(2)),
            knee: Vec3::new(g(3), g(4), g(5)),
            waist: Vec3::new(0.0, g(6), g(7)),
            shoulder: Vec3::new(g(8), g(9), g(10)),
            elbow: Vec3::new(g(11), g(12), g(13)),
            neck: Vec3::new(0.0, g(14), g(15)),
            hand: Vec3::new(g(16), g(17), g(18)),
            finger: Vec3::new(g(19), g(20), g(21)),
        }
    }
}

/// One vertex's bone influences: up to four (slot, weight) pairs, normalised.
#[derive(Debug, Clone, Copy, Default)]
pub struct Influence {
    pub n: u8,
    pub slot: [u8; 4],
    pub weight: [f32; 4],
}

/// A skinned mesh of a human with its bone → engine id table.
pub struct HumanMesh {
    pub data: MeshData,
    pub materials: Vec<omsi_o3d::Material>,
    /// Per bone of the file: engine id and (vertex, weight) list, as stored.
    pub bones: Vec<(i32, Vec<(u32, f32)>)>,
    /// Per vertex: the influences the skinning uses. The files list a vertex once per
    /// face corner (up to 24 times, always with the same weight), so the weights are taken
    /// once per bone and normalised; vertices near the sole also follow the foot.
    pub skin: Vec<Influence>,
    /// Per material: the model's `[matl_alpha]` for it (0 opaque, 1 alpha test, 2 blend) -
    /// the hair of the stock women and of man02 is an alpha-tested texture.
    pub alpha: Vec<i32>,
    /// Blend shapes in the model frame (an imported avatar's face; OMSI's people have none).
    pub morphs: Vec<omsi_o3d::Morph>,
}

/// The skeleton of one human type, measured from its `[links]` and its mesh. Index 0 of the
/// pairs is the left side, 1 the right.
#[derive(Debug, Clone)]
pub struct Rig {
    pub hip: [Vec3; 2],
    pub knee: [Vec3; 2],
    pub ankle: [Vec3; 2],
    pub shoulder: [Vec3; 2],
    pub elbow: [Vec3; 2],
    pub wrist: [Vec3; 2],
    pub waist: Vec3,
    pub neck: Vec3,
    /// What the head turns about: at the neck's height, under the middle of the head. The
    /// `[links]` neck point lies at the back of the neck (13 cm behind the middle of the
    /// head of aXYZ man02), and the head turned about it swung off the collar - a broken
    /// neck whenever the passenger looked to the side.
    pub head_pivot: Vec3,
    /// Between the hip joints.
    pub pelvis: Vec3,
    pub thigh: f32,
    pub shin: f32,
    pub upper_arm: f32,
    pub forearm: f32,
    /// Lowest point of the soles.
    pub sole: f32,
    /// Ankle joint above the sole.
    pub ankle_h: f32,
    /// Heel, ball and toe tip along the foot from the ankle (m, heel negative), and the
    /// height of the toe joint above the sole.
    pub heel: f32,
    pub ball: f32,
    pub toe: f32,
    pub ball_h: f32,
    pub head_top: f32,
    /// Hip joint above the seat surface when sitting (`[seatheight]` names the rest).
    pub seat_lift: f32,
    /// Size relative to a 1.75 m adult.
    pub scale: f32,
    /// From `[walk_param]`: the step length at full stride (m) - half the file's first
    /// line, the stride ({schrittweite}, 1.4 by default) -, the arm swing and the hip sway.
    pub walk_step: f32,
    pub arm_swing: f32,
    pub hip_sway: f32,
}

impl Rig {
    pub(super) fn measure(def: &Human, j: &Joints, meshes: &[HumanMesh]) -> Rig {
        let scale = if def.height > 0.5 {
            (def.height / 1.75).clamp(0.6, 1.3)
        } else {
            (j.neck.z / 1.55).clamp(0.6, 1.3)
        };
        let mirror = |v: Vec3| Vec3::new(-v.x, v.y, v.z);
        // the right shin's vertices (the foot is part of it)
        let mut shin: Vec<Vec3> = Vec::new();
        let mut top = f32::MIN;
        let (mut head_sum, mut head_n) = (Vec3::ZERO, 0u32);
        for m in meshes {
            for p in &m.data.positions {
                top = top.max(p.z);
            }
            for (i, inf) in m.skin.iter().enumerate() {
                if (0..inf.n as usize).any(|k| inf.slot[k] as usize == HEAD && inf.weight[k] > 0.5) {
                    head_sum += m.data.positions[i];
                    head_n += 1;
                }
            }
            for (i, inf) in m.skin.iter().enumerate() {
                if (0..inf.n as usize)
                    .any(|k| inf.slot[k] as usize == SHIN[1] && inf.weight[k] > 0.5)
                {
                    shin.push(m.data.positions[i]);
                }
            }
        }
        let knee = j.knee;
        let sole = shin.iter().map(|p| p.z).fold(f32::MAX, f32::min);
        let sole = if sole.is_finite() && sole < knee.z - 0.1 {
            sole
        } else {
            0.0
        };
        let ankle_h = (0.075 * scale).clamp(0.05, 0.1);
        let foot: Vec<&Vec3> = shin.iter().filter(|p| p.z < sole + 0.035 * scale).collect();
        let (mut heel_y, mut toe_y) = (knee.y - 0.07 * scale, knee.y + 0.19 * scale);
        if foot.len() >= 4 {
            let lo = foot.iter().map(|p| p.y).fold(f32::MAX, f32::min);
            let hi = foot.iter().map(|p| p.y).fold(f32::MIN, f32::max);
            if hi - lo > 0.12 * scale && hi - lo < 0.45 {
                heel_y = lo;
                toe_y = hi;
            }
        }
        // the ankle: the middle of the leg just above the foot
        let ring: Vec<&Vec3> = shin
            .iter()
            .filter(|p| (p.z - (sole + ankle_h + 0.02)).abs() < 0.025)
            .collect();
        let (mut ax, mut ay) = (knee.x, heel_y + 0.28 * (toe_y - heel_y));
        if ring.len() >= 4 {
            let n = ring.len() as f32;
            ax = ring.iter().map(|p| p.x).sum::<f32>() / n;
            ay = ring.iter().map(|p| p.y).sum::<f32>() / n;
        }
        // (a foot shorter than 13 cm - a child, a small model - has the ankle as far back as
        // it goes: the bounds crossed and the game stopped on the clamp, #138)
        let ay = ay.clamp(heel_y + 0.03, (toe_y - 0.1).max(heel_y + 0.03));
        let ax = if (ax - knee.x).abs() < 0.1 {
            ax
        } else {
            knee.x
        };
        let ankle = Vec3::new(ax, ay, sole + ankle_h);
        let foot_len = toe_y - heel_y;
        let hip = j.hip;
        let shoulder = j.shoulder;
        let elbow = if (j.elbow - j.shoulder).length() > 0.1 {
            j.elbow
        } else {
            j.shoulder + Vec3::new(0.26 * scale, 0.0, 0.0)
        };
        let wrist = if (j.hand - elbow).length() > 0.1 {
            j.hand
        } else {
            elbow + (elbow - shoulder).normalize() * 0.25 * scale
        };
        let head_top = if top.is_finite() && top > j.neck.z {
            top
        } else {
            j.neck.z + 0.2 * scale
        };
        let seat_lift = if def.seat_height > 0.2 {
            (hip.z - def.seat_height).clamp(0.05, 0.2)
        } else {
            0.1 * scale
        };
        let wp = def.walk_param;
        Rig {
            hip: [mirror(hip), hip],
            knee: [mirror(knee), knee],
            ankle: [mirror(ankle), ankle],
            shoulder: [mirror(shoulder), shoulder],
            elbow: [mirror(elbow), elbow],
            wrist: [mirror(wrist), wrist],
            waist: j.waist,
            neck: j.neck,
            head_pivot: if head_n > 20 {
                // the middle of the neck itself at the linked height: the vertices of the
                // mesh there (the neck's cross-section, under the head, not the collar)
                let c = head_sum / head_n as f32;
                let (mut sum, mut n) = (Vec3::ZERO, 0u32);
                for m in meshes {
                    for p in &m.data.positions {
                        if (p.z - j.neck.z).abs() < 0.03 && Vec2::new(p.x - c.x, p.y - c.y).length() < 0.09 {
                            sum += *p;
                            n += 1;
                        }
                    }
                }
                let at = if n >= 8 { sum / n as f32 } else { Vec3::new(c.x, j.neck.y + (c.y - j.neck.y) * 0.75, j.neck.z) };
                // (never further than 12 cm from the linked point)
                Vec3::new(j.neck.x, j.neck.y + (at.y - j.neck.y).clamp(-0.12, 0.12), j.neck.z)
            } else {
                j.neck
            },
            pelvis: Vec3::new(0.0, hip.y, hip.z),
            thigh: (knee - hip).length().max(0.2),
            shin: (ankle - knee).length().max(0.2),
            upper_arm: (elbow - shoulder).length(),
            forearm: (wrist - elbow).length(),
            sole,
            ankle_h,
            heel: heel_y - ay,
            ball: heel_y + 0.72 * foot_len - ay,
            toe: toe_y - ay,
            ball_h: (0.022 * scale).clamp(0.015, 0.03),
            head_top,
            seat_lift,
            scale,
            // (line 1 is the stride, hum+0x2d8: Omsi.exe's walk phase 0x626ae8 runs 2.0 per
            // two strides and sets a foot down at 0.2, 0.7, 1.2 and 1.7, one step per half a
            // stride; line 2, {upper_arm_beta}, is an angle of the arm)
            walk_step: 0.5 * if wp[0] > 0.3 { wp[0] } else { 1.4 },
            arm_swing: if wp[2] > 0.0 {
                wp[2].clamp(0.2, 1.6)
            } else {
                1.0
            },
            hip_sway: if wp[3] > 0.0 {
                wp[3].clamp(0.4, 2.2)
            } else {
                1.0
            },
        }
    }

    /// The toe joint of a foot in the rest pose.
    pub(super) fn ball_joint(&self, side: usize) -> Vec3 {
        self.ankle[side] + Vec3::new(0.0, self.ball, self.ball_h - self.ankle_h)
    }

    /// Hip joint height above the sole with the legs straight.
    pub fn leg(&self) -> f32 {
        self.hip[1].z - self.sole
    }

    /// Where a standing foot rests (the floor point under its ankle), in the model frame.
    pub(super) fn rest_foot(&self, side: usize) -> Vec3 {
        Vec3::new(self.ankle[side].x, self.ankle[side].y, self.sole)
    }

    /// How far in front of a seat's `[passpos]` (the hip) a person stands before sitting
    /// down, which is also where the feet stay while seated.
    pub fn seat_front(&self) -> f32 {
        (self.thigh * 0.85).clamp(0.25, 0.4)
    }

    /// Steps per second at `speed`, as Omsi.exe times the walk (0x626ae8): the stride is
    /// the full one from 1.2 m/s on and shortens with the speed below that, so the steps
    /// keep one pace when walking slowly; never with a step longer than the legs allow.
    pub fn cadence(&self, speed: f32) -> f32 {
        let v = speed.max(0.05);
        let f = v / (self.walk_step * (v / 1.2).min(1.0));
        f.max(v / (0.8 * self.leg())).min(3.4)
    }
}

pub struct HumanType {
    pub def: Human,
    /// The skeleton as Omsi.exe animates it (see [`crate::human_omsi`]).
    pub omsi: crate::human_omsi::OmsiRig,
    pub model: Model,
    pub model_dir: PathBuf,
    pub meshes: Vec<HumanMesh>,
    pub joints: Joints,
    pub rig: Rig,
    /// Clothing variants: the `[item]`s of the `.cti` files in the model's `[CTC]` folder
    /// (the stock people have two or three each), which replace the `[CTCTexture]`
    /// default. Variant 0 is the default texture, variant n the scheme n-1.
    pub variants: Vec<crate::vehicle::PaintScheme>,
    /// Folders the textures may lie in besides the usual ones: the `[CTC]` folders and
    /// the sub-folders of the human's own folder (see [`HumanType::texture_dirs`]).
    pub(super) extra_dirs: Vec<PathBuf>,
    /// An imported avatar's face: which blend shape of `meshes[0]` makes which expression.
    pub face: Option<super::face::FaceRig>,
    pub biped: Option<Box<super::clips::Biped>>,
}

impl HumanType {
    pub fn load(path: &Path) -> Result<HumanType> {
        let def = Human::load(path).with_context(|| format!("loading {}", path.display()))?;
        let dir = path.parent().unwrap_or(Path::new(""));
        let model_path = omsi_cfg::resolve_path(dir, &def.model);
        let model = Model::load(&model_path)
            .with_context(|| format!("loading {}", model_path.display()))?;
        let model_dir = model_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        let mut meshes = Vec::new();
        if !model.lods.is_empty() {
            for md in model.lod_meshes(0) {
                let p = omsi_cfg::resolve_path(&model_dir, &md.file);
                let m =
                    omsi_o3d::load_mesh(&p).with_context(|| format!("loading {}", p.display()))?;
                let bones: Vec<(i32, Vec<(u32, f32)>)> = m
                    .bones
                    .iter()
                    .map(|b| {
                        let id = md
                            .bones
                            .iter()
                            .find(|(n, _)| n.eq_ignore_ascii_case(&b.name))
                            .map(|(_, id)| *id)
                            .or_else(|| bone_id_by_name(&b.name))
                            .unwrap_or(0);
                        (id, b.weights.iter().map(|w| (w.vertex, w.weight)).collect())
                    })
                    .collect();
                let data = mesh_from_o3d(&m);
                let skin = influences(&data, &bones);
                // `[matl] <texture> <n>` names the n-th material with that texture
                let mut seen: Vec<String> = Vec::new();
                let alpha = m
                    .materials
                    .iter()
                    .map(|mat| {
                        let name = mat.texture.trim().to_ascii_lowercase();
                        let n = seen.iter().filter(|t| **t == name).count() as i32;
                        seen.push(name);
                        md.materials
                            .iter()
                            .find(|d| d.texture.trim().eq_ignore_ascii_case(mat.texture.trim()) && d.index == n)
                            .map(|d| d.alpha)
                            .unwrap_or(0)
                    })
                    .collect();
                meshes.push(HumanMesh {
                    data,
                    materials: m.materials.clone(),
                    bones,
                    skin,
                    alpha,
                    morphs: Vec::new(),
                });
            }
        }
        let mut joints = Joints::from_links(&def.links);
        fit_leg_joints(&mut joints, &meshes, path);
        let rig = Rig::measure(&def, &joints, &meshes);
        for m in meshes.iter_mut() {
            split_feet(m, &rig);
        }
        // The `[CTC]` folder is relative to the .hum file's folder (`Texture\man02` is
        // Humans/Other/texture/man02). An add-on that put that folder straight into its own
        // folder instead (GSPNS: Humans/GSPNS/man02, with the default texture in it too) is
        // found by the folder's last name; OMSI would show that person untextured.
        let mut variants = Vec::new();
        let mut extra_dirs: Vec<PathBuf> = Vec::new();
        for c in &model.ctc {
            let mut d = omsi_cfg::resolve_path(dir, &c.path);
            if !omsi_cfg::vfs::is_dir(&d) {
                let last = c.path.trim().trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or("");
                if !last.is_empty() {
                    let alt = omsi_cfg::resolve_path(dir, last);
                    if omsi_cfg::vfs::is_dir(&alt) {
                        d = alt;
                    }
                }
            }
            variants.extend(crate::vehicle::load_paint_schemes(&d));
            extra_dirs.push(d);
        }
        // the sub-folders of the human's folder and of its texture folder, last
        for base in [dir.to_path_buf(), omsi_cfg::resolve_path(dir, "texture")] {
            for (n, is_dir) in omsi_cfg::vfs::list_dir(&base).unwrap_or_default() {
                if is_dir && !n.to_string_lossy().eq_ignore_ascii_case("model") {
                    let d = base.join(n);
                    if !extra_dirs.contains(&d) {
                        extra_dirs.push(d);
                    }
                }
            }
        }
        Ok(HumanType {
            omsi: crate::human_omsi::OmsiRig::new(&def),
            joints,
            rig,
            def,
            model,
            model_dir,
            meshes,
            variants,
            extra_dirs,
            face: None,
            biped: None,
        })
    }

    pub fn texture_dirs(&self, root: &Path) -> Vec<PathBuf> {
        let human_dir = self.model_dir.parent().unwrap_or(&self.model_dir);
        let hum_dir = self.def.path.parent().unwrap_or(human_dir);
        let mut dirs = vec![
            omsi_cfg::resolve_path(&self.model_dir, "texture"),
            self.model_dir.clone(),
            omsi_cfg::resolve_path(human_dir, "texture"),
            human_dir.to_path_buf(),
            omsi_cfg::resolve_path(hum_dir, "texture"),
            hum_dir.to_path_buf(),
            omsi_cfg::resolve_path(&self.model_dir, "..\\texture"),
            omsi_cfg::resolve_path(root, "Texture"),
        ];
        dirs.dedup();
        dirs.extend(self.extra_dirs.iter().cloned());
        dirs
    }

    /// The texture a material shows in clothing variant `variant` (0 = the default):
    /// the file name and the folder to look in first.
    pub fn variant_texture<'a>(&'a self, texture: &'a str, variant: usize) -> (&'a str, Option<&'a Path>) {
        let Some(s) = variant.checked_sub(1).and_then(|k| self.variants.get(k)) else {
            return (texture, None);
        };
        for (ctc_name, file) in &s.textures {
            for (name, default) in &self.model.ctc_textures {
                if name.eq_ignore_ascii_case(ctc_name)
                    && default.trim().eq_ignore_ascii_case(texture.trim())
                {
                    return (file.as_str(), Some(s.dir.as_path()));
                }
            }
        }
        (texture, None)
    }
}

/// A leg joint of `[links]` that lies outside the leg it belongs to is the author's slip:
/// the GSPNS man04 and man041 give the right hip at x = 0.6 m and the knee at 0.83 m, where
/// the mesh's thigh is at 0.08. OMSI's rotations about those points hardly show it (the
/// legs swing about the x axis, along which the error lies), but a leg rig measured from
/// them reaches its foot 0.8 m out to the side and crosses the legs. Such a coordinate is
/// taken from the mesh instead: the middle of the limb's vertices at the joint's height.
pub(super) fn fit_leg_joints(j: &mut Joints, meshes: &[HumanMesh], path: &Path) {
    let verts = |slots: &[usize]| -> Vec<Vec3> {
        let mut out = Vec::new();
        for m in meshes {
            for (i, inf) in m.skin.iter().enumerate() {
                if (0..inf.n as usize).any(|k| slots.contains(&(inf.slot[k] as usize)) && inf.weight[k] > 0.5) {
                    out.push(m.data.positions[i]);
                }
            }
        }
        out
    };
    let thigh = verts(&[THIGH[1]]);
    let knee_region = verts(&[THIGH[1], SHIN[1]]);
    let fit = |joint: &mut Vec3, limb: &[Vec3], what: &str| {
        if limb.len() < 8 {
            return;
        }
        let near: Vec<&Vec3> = limb.iter().filter(|p| (p.z - joint.z).abs() < 0.06).collect();
        let near: Vec<&Vec3> = if near.len() >= 4 { near } else { limb.iter().collect() };
        let n = near.len() as f32;
        let (lo, hi) = limb.iter().fold((Vec3::MAX, Vec3::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        let mut fixed = false;
        if joint.x < lo.x - 0.05 || joint.x > hi.x + 0.05 {
            joint.x = near.iter().map(|p| p.x).sum::<f32>() / n;
            fixed = true;
        }
        if joint.y < lo.y - 0.05 || joint.y > hi.y + 0.05 {
            joint.y = near.iter().map(|p| p.y).sum::<f32>() / n;
            fixed = true;
        }
        if fixed {
            log::info!(
                "{}: the {what} of [links] lies outside the leg; using ({:.2}, {:.2}, {:.2}) from the mesh",
                path.display(),
                joint.x,
                joint.y,
                joint.z
            );
        }
    };
    fit(&mut j.hip, &thigh, "hip");
    fit(&mut j.knee, &knee_region, "knee");
}

/// Per-vertex influences from the file's bone lists: each bone counted once per vertex,
/// the strongest four kept, normalised. A vertex no bone claims follows the torso.
pub(super) fn influences(data: &MeshData, bones: &[(i32, Vec<(u32, f32)>)]) -> Vec<Influence> {
    let n = data.positions.len();
    let mut per: Vec<Vec<(u8, f32)>> = vec![Vec::new(); n];
    for (id, weights) in bones {
        let Some(slot) = slot_of(*id) else { continue };
        for (v, w) in weights {
            let Some(list) = per.get_mut(*v as usize) else {
                continue;
            };
            if !w.is_finite() || *w <= 0.0 {
                continue;
            }
            match list.iter_mut().find(|e| e.0 as usize == slot) {
                Some(e) => e.1 = e.1.max(*w),
                None => list.push((slot as u8, *w)),
            }
        }
    }
    per.into_iter()
        .map(|mut list| {
            list.sort_by(|a, b| b.1.total_cmp(&a.1));
            list.truncate(4);
            let total: f32 = list.iter().map(|e| e.1).sum();
            if total < 1e-4 {
                return Influence {
                    n: 1,
                    slot: [MAIN as u8, 0, 0, 0],
                    weight: [1.0, 0.0, 0.0, 0.0],
                };
            }
            let mut inf = Influence {
                n: list.len() as u8,
                ..Default::default()
            };
            for (k, e) in list.iter().enumerate() {
                inf.slot[k] = e.0;
                inf.weight[k] = e.1 / total;
            }
            inf
        })
        .collect()
}

/// Move `share` of a vertex's weight on slot `from` to slot `to`.
fn move_weight(inf: &mut Influence, from: usize, to: usize, share: f32) {
    let Some(k) = (0..inf.n as usize).find(|&k| inf.slot[k] as usize == from) else {
        return;
    };
    let moved = inf.weight[k] * share;
    if moved < 1e-3 {
        return;
    }
    if let Some(e) = (0..inf.n as usize).find(|&e| inf.slot[e] as usize == to) {
        inf.weight[k] -= moved;
        inf.weight[e] += moved;
    } else if (inf.n as usize) < 4 {
        inf.weight[k] -= moved;
        let e = inf.n as usize;
        inf.slot[e] = to as u8;
        inf.weight[e] = moved;
        inf.n += 1;
    } else if share > 0.5 {
        // no room: all of it
        inf.slot[k] = to as u8;
    }
}

/// Give the feet and the toes bones of their own: shin weight below the ankle moves to the
/// foot and foot weight beyond the ball to the toes, with short blends around the joints.
/// A skirt is weighted to the thighs in the files and would fly up with a swinging leg like
/// a lap: its cloth (thigh vertices well outside the leg) follows the hips in part.
pub(super) fn split_feet(m: &mut HumanMesh, rig: &Rig) {
    let top = rig.sole + rig.ankle_h + 0.025;
    let bottom = rig.sole + rig.ankle_h - 0.03;
    for (i, inf) in m.skin.iter_mut().enumerate() {
        let v = m.data.positions[i];
        if v.z >= top {
            continue;
        }
        let to_foot = smoothstep(top, bottom, v.z);
        for side in 0..2 {
            move_weight(inf, SHIN[side], FOOT[side], to_foot);
            let along = v.y - rig.ankle[side].y;
            let to_toe = smoothstep(rig.ball - 0.012, rig.ball + 0.02, along);
            if to_toe > 0.0 {
                move_weight(inf, FOOT[side], TOE[side], to_toe);
            }
        }
    }
}
