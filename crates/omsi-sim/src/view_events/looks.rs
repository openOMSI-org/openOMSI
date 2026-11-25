//! What an AI car looks like besides where its meshes are: the values its materials,
//! light maps and interior lamps are switched by, the pictures its scripts, pages and text
//! textures drew, the glass it cracked and the dents it took, its bellows reshaped - taken
//! into the [`FrameSnapshot`] at the view sync as owned data, so that the view draws the
//! car without reading the simulation.
//!
//! What only changes now and then goes as a change: the string variables (each car's are
//! diffed against what the view was last sent, [`LookTracker`]), the pictures (only those
//! redrawn), cracks and dents (as they happen), the bellows (when their bones moved). The
//! simulation's side of drawing a car - its HTML pages and text textures run, the script
//! textures' uploads taken, the far displays' half-second turns, the bellows' shapes - is
//! done here, in the order and under the conditions the view sync did it.

use super::*;
use crate::vehicle::VehicleDent;
use glam::{Mat4, Vec3};
use hashbrown::HashMap;

/// The looks of one AI car in view (see the module).
pub struct CarLooks {
    /// The body's type (its parts' are their `PartFrame::ty`).
    pub ty: Arc<VehicleType>,
    /// Variable names to slots of `vars`.
    pub var_index: Arc<HashMap<String, omsi_script::VarId>>,
    vars: Span,
    /// The string variables that changed since the view was last sent them: (slot, value).
    pub str_changes: Vec<(u32, String)>,
    /// Whether the far car's displays wait this frame (their half second has not come).
    pub displays_far: bool,
    /// Whether each `[interiorlight]` of the body and of each part is on, part by part.
    lamps: Vec<Span>,
    /// Text texture pictures drawn anew: (part, index, width, height, RGBA); part 0 is the body.
    pub text_uploads: Vec<(u32, usize, u32, u32, Vec<u8>)>,
    /// The body's script texture pictures to upload, within the frame's budget.
    pub script_uploads: Vec<ScriptUpload>,
    /// Panes cracked: (mesh, the impact in the mesh's frame, energy, the mesh's transform).
    pub cracks: Vec<(usize, Vec3, f32, Mat4)>,
    /// Dents taken.
    pub dents: Vec<VehicleDent>,
    /// Bellows reshaped: (part, mesh, positions, normals).
    pub skins: Vec<(u32, usize, Vec<Vec3>, Vec<Vec3>)>,
}

/// A script texture picture for the GPU.
pub struct ScriptUpload {
    pub index: usize,
    pub width: u32,
    pub height: u32,
    pub mipmaps: bool,
    pub rgba: Vec<u8>,
}

/// What the simulation remembers of what it sent the view of each car: the string
/// variables, the far displays' turn, the bellows' bones. Forgotten when a car's renders are
/// made anew or let go (`forget`).
#[derive(Default)]
pub struct LookTracker {
    cars: HashMap<u64, CarTrack>,
}

#[derive(Default)]
struct CarTrack {
    strs: Vec<String>,
    displays_far: bool,
    display_tick: u64,
    skin_keys: Vec<(u32, usize, Vec<Mat4>)>,
}

impl LookTracker {
    /// Car `id`'s renders are made anew or let go: what was sent for them no longer holds.
    pub fn forget(&mut self, id: u64) {
        self.cars.remove(&id);
    }
}

/// The view's constants the looks are taken under.
pub struct LookLimits {
    /// Beyond this (m) a car's displays go up every half second only.
    pub displays_far: f64,
    /// Beyond this (m) a car's bellows are not reshaped.
    pub skin_distance: f64,
    /// Bytes of script texture pictures uploaded in one frame at most.
    pub upload_budget: usize,
}

/// Each `[interiorlight]` of `ty`: on (its variable 0.5 or more, as OMSI enables a lamp).
fn lamps_on(ty: &VehicleType, var: impl Fn(&str) -> Option<f32>, out: &mut Vec<bool>) -> Span {
    let start = out.len() as u32;
    for il in &ty.model.interior_lights {
        out.push(
            il.variable
                .trim()
                .parse::<f32>()
                .ok()
                .or_else(|| var(&il.variable))
                .unwrap_or(0.0)
                >= 0.5,
        );
    }
    Span {
        start,
        len: out.len() as u32 - start,
    }
}

/// The meshes of `ty` drawn as bellows (`[smoothskin]` with geometry).
fn skin_meshes(ty: &VehicleType) -> impl Iterator<Item = usize> + '_ {
    (0..ty.meshes.len())
        .filter(|&i| !ty.meshes[i].skin.is_empty() && !ty.meshes[i].data.positions.is_empty())
}

impl FrameSnapshot {
    /// The variable values of `looks`.
    pub fn vars(&self, looks: &CarLooks) -> &[f32] {
        &self.vars[looks.vars.range()]
    }

    /// Whether each interior lamp of part `part` (0: the body) of `looks` is on.
    pub fn lamps(&self, looks: &CarLooks, part: usize) -> &[bool] {
        looks
            .lamps
            .get(part)
            .map(|s| &self.lamps[s.range()])
            .unwrap_or(&[])
    }

    /// The looks of `car`, when it is in view.
    pub fn looks(&self, car: &CarFrame) -> Option<&CarLooks> {
        car.looks.map(|k| &self.looks[k as usize])
    }
}

impl crate::ai_traffic::TrafficSim {
    /// The looks of every car in view into `out` (whose cars, taken by `snapshot_into`, are
    /// these cars in this order), doing the simulation's side of drawing them as the view
    /// sync did it: their pages and text textures run, the uploads taken.
    pub fn snapshot_looks(
        &mut self,
        out: &mut FrameSnapshot,
        tracker: &mut LookTracker,
        limits: &LookLimits,
    ) {
        out.looks.clear();
        out.vars.clear();
        out.lamps.clear();
        // (a far car's displays go up every half second, a slice of the cars per frame)
        let tick = (self.time as f64 * 2.0) as u64;
        let mut budget = limits.upload_budget;
        let camera = self.camera;
        for (c, frame) in self.cars.iter_mut().zip(out.cars.iter_mut()) {
            debug_assert_eq!(c.id, frame.id);
            frame.looks = None;
            if !c.vehicle.ai_visuals {
                continue;
            }
            let t = tracker.cars.entry(c.id).or_default();
            let v = &mut c.vehicle;
            if let Some(cam) = camera {
                let far = (v.position - cam).length() > limits.displays_far;
                let due = t.display_tick != tick;
                t.displays_far = far && !due;
                if far && due {
                    t.display_tick = tick;
                }
            }
            // the body's pages, its interior lamps, its text textures, its scripts' pictures
            v.update_html_textures();
            let mut lamps = vec![lamps_on(&v.ty, |n| v.var(n), &mut out.lamps)];
            let mut text_uploads = Vec::new();
            for i in v.update_text_textures() {
                let tt = &mut v.text_textures[i];
                if let Some(img) = tt.pending.take() {
                    text_uploads.push((
                        0,
                        i,
                        tt.def.width.max(1) as u32,
                        tt.def.height.max(1) as u32,
                        img,
                    ));
                }
            }
            let mut script_uploads = Vec::new();
            let scripted = v.ty.model.script_textures.len();
            for (i, st) in v.host.script_textures.iter_mut().enumerate() {
                if t.displays_far || i >= scripted {
                    continue;
                }
                if budget == 0 {
                    continue;
                }
                let Some(rgba) = st.take_upload() else {
                    continue;
                };
                budget = budget.saturating_sub(rgba.len());
                script_uploads.push(ScriptUpload {
                    index: i,
                    width: st.width,
                    height: st.height,
                    mipmaps: st.mipmaps,
                    rgba,
                });
            }
            // the glass it cracked and the dents it took
            let cracks = v
                .take_broken_glass()
                .into_iter()
                .filter_map(|g| {
                    let transform = v.mesh_transforms.get(g.mesh)?;
                    Some((
                        g.mesh,
                        transform.inverse().transform_point3(g.point),
                        g.energy,
                        v.mesh_local_transform(g.mesh),
                    ))
                })
                .collect();
            let dents = v.take_damage_dents();
            // its coupled parts: their interior lamps by its variables, their text textures
            let mut trailers = std::mem::take(&mut v.trailers);
            for (k, part) in trailers.iter_mut().enumerate() {
                lamps.push(lamps_on(&part.ty, |n| v.var(n), &mut out.lamps));
                for i in part.update_text_textures(v) {
                    let tt = &mut part.text_textures[i];
                    if let Some(img) = tt.pending.take() {
                        text_uploads.push((
                            k as u32 + 1,
                            i,
                            tt.def.width.max(1) as u32,
                            tt.def.height.max(1) as u32,
                            img,
                        ));
                    }
                }
            }
            v.trailers = trailers;
            // the bellows, near enough for their fold to show, whose bones moved
            let mut skins = Vec::new();
            let has_skin = skin_meshes(&v.ty).next().is_some()
                || v.trailers
                    .iter()
                    .any(|p| skin_meshes(&p.ty).next().is_some());
            if has_skin
                && camera.is_none_or(|cam| (v.position - cam).length() < limits.skin_distance)
            {
                let mut keys = std::mem::take(&mut t.skin_keys);
                let mut reshape = |part: u32, i: usize, key: Vec<Mat4>, skinned: &mut dyn FnMut() -> Option<(Vec<Vec3>, Vec<Vec3>)>| {
                    match keys.iter_mut().find(|(p, m, _)| *p == part && *m == i) {
                        Some((_, _, last)) if *last == key => return,
                        Some((_, _, last)) => *last = key,
                        None => keys.push((part, i, key)),
                    }
                    if let Some((pos, nrm)) = skinned() {
                        skins.push((part, i, pos, nrm));
                    }
                };
                let ty = v.ty.clone();
                for i in skin_meshes(&ty) {
                    let key = v.skin_key(i);
                    reshape(0, i, key, &mut || v.skinned(i));
                }
                let n_vars = v.state.vars.len();
                for (k, part) in v.trailers.iter_mut().enumerate() {
                    let ty = part.ty.clone();
                    for i in skin_meshes(&ty) {
                        let key = part.skin_key(i);
                        reshape(k as u32 + 1, i, key, &mut || part.skinned(i, n_vars));
                    }
                }
                t.skin_keys = keys;
            }
            // the variables as the materials read them, the strings that changed
            let vars = Span::push(&mut out.vars, &v.state.vars);
            let mut str_changes = Vec::new();
            if t.strs.len() != v.state.str_vars.len() {
                t.strs = v.state.str_vars.clone();
                str_changes.extend(
                    t.strs
                        .iter()
                        .enumerate()
                        .map(|(i, s)| (i as u32, s.clone())),
                );
            } else {
                for (i, (sent, now)) in t.strs.iter_mut().zip(&v.state.str_vars).enumerate() {
                    if sent != now {
                        sent.clone_from(now);
                        str_changes.push((i as u32, now.clone()));
                    }
                }
            }
            frame.looks = Some(out.looks.len() as u32);
            out.looks.push(CarLooks {
                ty: v.ty.clone(),
                var_index: v.var_index().clone(),
                vars,
                str_changes,
                displays_far: t.displays_far,
                lamps,
                text_uploads,
                script_uploads,
                cracks,
                dents,
                skins,
            });
        }
        // (the cars that left the road without new renders under their id)
        if tracker.cars.len() > 2 * self.cars.len() + 64 {
            let ids: hashbrown::HashSet<u64> = self.cars.iter().map(|c| c.id).collect();
            tracker.cars.retain(|id, _| ids.contains(id));
        }
    }
}
