//! The vehicle type: its definition, compiled scripts, model meshes and paint schemes, loaded once
//! and shared by every instance.

use super::*;

// A vehicle pack may contain many tiny trim meshes and only a few large body/interior
// meshes. When the mesh vote is close, use capped triangle counts as a tie-breaker instead
// of letting every tiny mesh carry the same weight.
pub(super) const WINDING_EVIDENCE_CAP: usize = 4096;

pub(super) fn keep_authored_winding(
    forward_meshes: usize,
    backward_meshes: usize,
    forward_weight: usize,
    backward_weight: usize,
) -> bool {
    if forward_meshes > backward_meshes {
        return true;
    }
    if forward_meshes == 0 || backward_meshes == 0 {
        return false;
    }
    let counts_close = forward_meshes.saturating_mul(5) >= backward_meshes.saturating_mul(4);
    let forward_clearly_heavier =
        forward_weight.saturating_mul(10) >= backward_weight.saturating_mul(11);
    counts_close && forward_clearly_heavier
}

#[derive(Default, Debug)]
pub(super) struct WindingVotes {
    pub(super) forward: usize,
    pub(super) backward: usize,
    pub(super) forward_weight: usize,
    pub(super) backward_weight: usize,
}

impl WindingVotes {
    pub(super) fn record(&mut self, forward: bool, triangles: usize) {
        let weight = triangles.min(WINDING_EVIDENCE_CAP);
        if forward {
            self.forward += 1;
            self.forward_weight += weight;
        } else {
            self.backward += 1;
            self.backward_weight += weight;
        }
    }

    pub(super) fn keep_authored(&self) -> bool {
        keep_authored_winding(self.forward, self.backward, self.forward_weight, self.backward_weight)
    }
}

/// Built-in variables every road vehicle has (`program/varlist_roadvehicle.txt` + generated).
pub fn builtin_vars(root: &Path) -> Vec<String> {
    let mut v: Vec<String> =
        match omsi_cfg::CfgFile::read(root.join("program/varlist_roadvehicle.txt")) {
            Ok(f) => f
                .lines
                .iter()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect(),
            Err(_) => Vec::new(),
        };
    for a in 0..8 {
        for side in ["L", "R"] {
            for pre in [
                "Wheel_Rotation_",
                "Wheel_RotationSpeed_",
                "Axle_Steering_",
                "Axle_Suspension_",
                "Axle_Springfactor_",
                "Axle_Brakeforce_",
                "Axle_SurfaceID_",
            ] {
                v.push(format!("{pre}{a}_{side}"));
            }
        }
        v.push(format!("PAX_Entry{a}_Open"));
        v.push(format!("PAX_Entry{a}_Req"));
        v.push(format!("PAX_Exit{a}_Open"));
        v.push(format!("PAX_Exit{a}_Req"));
    }
    for i in 0..6 {
        v.push(format!("Debug_{i}"));
    }
    for i in 0..4 {
        for n in ["alpha", "beta", "gamma"] {
            v.push(format!("articulation_{i}_{n}"));
        }
    }
    // the doors past Omsi.exe's eight (#719), after all of its own variables
    for a in 8..PAX_DOORS {
        v.push(format!("PAX_Entry{a}_Open"));
        v.push(format!("PAX_Entry{a}_Req"));
        v.push(format!("PAX_Exit{a}_Open"));
        v.push(format!("PAX_Exit{a}_Req"));
    }
    // somebody standing in the doorway (openOMSI's, #720: what a door's light barrier
    // sees), after those
    for a in 0..PAX_DOORS {
        v.push(format!("PAX_Entry{a}_Busy"));
        v.push(format!("PAX_Exit{a}_Busy"));
    }
    v
}

pub fn builtin_str_vars(root: &Path) -> Vec<String> {
    match omsi_cfg::CfgFile::read(root.join("program/stringvarlist_roadvehicle.txt")) {
        Ok(f) => f
            .lines
            .iter()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        Err(_) => vec![
            "ident".into(),
            "number".into(),
            "act_route".into(),
            "act_busstop".into(),
            "SetLineTo".into(),
            "yard".into(),
            "file_schedule".into(),
        ],
    }
}

/// A bone of a `[smoothskin]` mesh: the model mesh whose animation moves it (`[setbone]
/// name id`, the id counting the meshes of the model's first level of detail - the GN92's and
/// the O530G's joint dummies `Gelenk_A`-`D` are its first four) and the weights of the o3d
/// bone of that name. `None`: an o3d bone no `[setbone]` names, which keeps its weights and
/// stays with the mesh itself (the root bone of an armature: the Agora's bellows hang half of
/// many a vertex on `Armature1_Bone`, its retarder lever on `Bone`).
#[derive(Debug, Clone, Default)]
pub struct SkinBone {
    pub def_index: Option<usize>,
    pub weights: Vec<(u32, f32)>,
}

/// One mesh of the vehicle model ready for rendering.
#[derive(Clone)]
pub struct VehicleMesh {
    pub def_index: usize,
    /// The mesh, or (for a type loaded with [`VehicleType::load_ai`]) only its material
    /// ranges: the vertices are read again from `file` when the GPU needs them.
    pub data: MeshData,
    /// The `.o3d` file the mesh came from.
    pub file: PathBuf,
    pub materials: Vec<omsi_o3d::Material>,
    pub overrides: Vec<MaterialDef>,
    pub pivot: Mat4,
    pub viewpoint: i32,
    /// `[smoothskin]` bones (empty for a rigid mesh, and for an AI type).
    pub skin: Vec<SkinBone>,
    /// A backwards mesh drawn as wound after all (its pack's exporter, see `load`).
    pub keep_winding: bool,
}

#[derive(Clone)]
pub struct VehicleType {
    pub def: Vehicle,
    pub model: Model,
    pub model_dir: PathBuf,
    pub program: Arc<Program>,
    pub meshes: Vec<VehicleMesh>,
    /// The meshes of the model's lower levels of detail (see `load_with`), drawn instead of
    /// `meshes` when the vehicle is small on the screen; `model.meshes[def_index].lod` is the
    /// level, `model.lods[level].min_size` its least size.
    pub lod_meshes: Vec<VehicleMesh>,
    /// Paint schemes / adverts from the `[CTC]` folders' `.cti` files.
    pub paint_schemes: Vec<PaintScheme>,
    /// `[texchanges]`: material textures a script variable swaps (roller blinds, trim).
    pub texchanges: Vec<omsi_model::TexChangeMaster>,
    /// The model's tyres, by the axle number of the `Wheel_Rotation_<n>_*` that turns them
    /// (an articulated bus's rear section counts on from the front): (n, centre height,
    /// centre position along the bus, radius).
    pub wheel_meshes: Vec<(usize, f32, f32, f32)>,
    /// Axle numbers whose wheels the model moves with `Axle_Suspension_<n>_*`.
    pub suspension_axles: Vec<usize>,
    /// Other vehicle packs this one takes meshes from that are not installed (see
    /// `omsi_cfg::missing_vehicle_pack`), with how many meshes are missing for it.
    pub missing_packs: Vec<(String, usize)>,
    /// Per mesh: a sphere (centre, radius) around its vertices in the mesh's own frame, so
    /// that a ray (the mouse over the cockpit) can pass most meshes by without looking at a
    /// triangle. Zero for the meshes an AI type keeps no vertices of.
    pub mesh_bounds: Vec<(Vec3, f32)>,
    /// Per mesh: the box (least, greatest corner) its vertices take in the mesh's own frame,
    /// kept for AI types too (zero for a mesh without vertices).
    pub mesh_boxes: Vec<(Vec3, Vec3)>,
}

/// One `.cti` item group: replaces the textures of `[CTCTexture]` slots and sets variables.
#[derive(Debug, Clone, Default)]
pub struct PaintScheme {
    pub name: String,
    /// Folder the item came from (textures resolve there first).
    pub dir: PathBuf,
    /// (`[CTCTexture]` name, texture file)
    pub textures: Vec<(String, String)>,
    pub set_vars: Vec<(String, f32)>,
}

/// Collect the `[item]` groups of every `.cti` file in `dir`. Consecutive items with the same
/// name form one scheme; `[setvar]` lines belong to the item before them. The folder is read
/// in every content root (a repaint installed as a mod lies in the content folder's copy of
/// the stock folder); a file of the same name in a higher-priority root replaces the other.
pub fn load_paint_schemes(dir: &Path) -> Vec<PaintScheme> {
    let mut schemes: Vec<PaintScheme> = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    // the folder in every content root (and mounted archive) that has it
    for d in omsi_cfg::mirrored_dirs(dir) {
        let Some(list) = omsi_cfg::vfs::list_dir(&d) else {
            continue;
        };
        let mut here: Vec<PathBuf> = list
            .into_iter()
            .map(|(n, _)| d.join(n))
            .filter(|p| {
                p.extension()
                    .map(|e| e.eq_ignore_ascii_case("cti"))
                    .unwrap_or(false)
            })
            .collect();
        here.sort();
        for f in here {
            if seen.insert(
                f.file_name()
                    .map(|n| n.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default(),
            ) {
                files.push(f);
            }
        }
    }
    for f in files {
        let dir = f
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| dir.to_path_buf());
        let Ok(cfg) = omsi_cfg::CfgFile::read(&f) else {
            continue;
        };
        let mut r = cfg.reader();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "item" => {
                    let name = r.str().to_string();
                    let ctc = r.str().to_string();
                    let tex = r.str().to_string();
                    match schemes
                        .iter_mut()
                        .find(|s| s.name.eq_ignore_ascii_case(&name))
                    {
                        Some(s) => s.textures.push((ctc, tex)),
                        None => schemes.push(PaintScheme {
                            name,
                            dir: dir.clone(),
                            textures: vec![(ctc, tex)],
                            set_vars: Vec::new(),
                        }),
                    }
                }
                "setvar" => {
                    let var = r.str().to_string();
                    let v = r.f32();
                    if let Some(s) = schemes.last_mut() {
                        s.set_vars.push((var, v));
                    }
                }
                _ => {}
            }
        }
    }
    schemes
}

/// The vehicle pack a mesh file belongs to (the folder under `Vehicles`, lower case), the
/// unit its exporter's winding is judged by; a file elsewhere is judged with its folder.
pub(super) fn winding_pack(p: &Path) -> String {
    // (a part borrowed as `..\..\Other\model\x.o3d`: the folders it climbs out of are not its own)
    let mut comps: Vec<String> = Vec::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                comps.pop();
            }
            std::path::Component::CurDir => {}
            c => comps.push(c.as_os_str().to_string_lossy().to_ascii_lowercase()),
        }
    }
    match comps.iter().rposition(|c| c == "vehicles") {
        Some(i) if i + 2 < comps.len() => comps[i + 1].clone(),
        _ => comps[..comps.len().saturating_sub(1)].join("/"),
    }
}

/// Where a `[mesh]` file of a model lives: next to the model file as OMSI reads it, else -
/// for add-ons laid out for another folder (Studio Polygon's `Configuration Files` sit
/// beside `model`, and packs that borrow parts name them from the vehicle folder or the
/// game folder) - the first of the vehicle folder, its `model` folder and the game folder
/// that has it. The model folder's spelling when none does, for the warning.
pub(super) fn mesh_path(root: &Path, dir: &Path, model_dir: &Path, file: &str) -> PathBuf {
    let first = omsi_cfg::resolve_path(model_dir, file);
    if omsi_cfg::vfs::exists(&first) {
        return first;
    }
    let model = omsi_cfg::resolve_path(dir, "model");
    let parent = model_dir.parent().map(Path::to_path_buf);
    for base in [Some(dir.to_path_buf()), Some(model), parent, Some(root.to_path_buf())].into_iter().flatten() {
        let p = omsi_cfg::resolve_path(&base, file);
        if omsi_cfg::vfs::exists(&p) {
            return p;
        }
        // (a path that names its own folder again: "model\Configuration Files\..." given
        // from inside `model`)
        let trimmed = file.trim_start_matches(['\\', '/']);
        if let Some(rest) = trimmed.split_once(['\\', '/']).map(|(_, r)| r) {
            let p = omsi_cfg::resolve_path(&base, rest);
            if omsi_cfg::vfs::exists(&p) {
                return p;
            }
        }
    }
    first
}

impl VehicleType {
    pub fn load(root: &Path, bus_file: &Path) -> Result<VehicleType> {
        Self::load_with(root, bus_file, true)
    }

    /// The box the whole model takes (least, greatest corner; y forward), None without
    /// vertices.
    pub fn model_box(&self) -> Option<(Vec3, Vec3)> {
        let (lo, hi) = self
            .mesh_boxes
            .iter()
            .filter(|(lo, hi)| hi.x > lo.x || hi.y > lo.y)
            .fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(a, b), (lo, hi)| (a.min(*lo), b.max(*hi)));
        (hi.y > lo.y).then_some((lo, hi))
    }

    /// Half its length as Omsi.exe keeps it (type +0xd4): half the `[boundingbox]`'s
    /// length, else half the model's.
    pub fn half_length(&self) -> Option<f32> {
        match self.def.bounding_box {
            Some(bb) if bb[1] > 0.0 => Some(bb[1] * 0.5),
            _ => self.model_box().map(|(lo, hi)| (hi.y - lo.y) * 0.5),
        }
    }

    /// A type for AI copies: its meshes are measured (the tyres) and let go - nothing but
    /// the upload to the GPU needs them, and a timetable fleet kept half a gigabyte of
    /// vertices on the CPU. [`VehicleType::mesh_data`] reads a mesh again.
    pub fn load_ai(root: &Path, bus_file: &Path) -> Result<VehicleType> {
        Self::load_with(root, bus_file, false)
    }

    pub(super) fn load_with(root: &Path, bus_file: &Path, keep_meshes: bool) -> Result<VehicleType> {
        let def =
            Vehicle::load(bus_file).with_context(|| format!("loading {}", bus_file.display()))?;
        let dir = def.dir().to_path_buf();
        let model_rel = def.model.clone().context("vehicle has no [model]")?;
        let model_path = omsi_cfg::resolve_path(&dir, &model_rel);
        let mut model = Model::load(&model_path)
            .with_context(|| format!("loading {}", model_path.display()))?;
        let model_dir = model_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        let mut input = CompileInput {
            builtin_vars: builtin_vars(root),
            builtin_str_vars: builtin_str_vars(root),
            ..Default::default()
        };
        input.varlists = def.scripts.varlists.clone();
        input.stringvarlists = def.scripts.stringvarlists.clone();
        input.constfiles = def.scripts.constfiles.clone();
        input.scripts = def.scripts.scripts.clone();
        let mut program = compile(&input);
        for e in &program.errors {
            log::warn!("{e}");
        }
        let mut meshes = Vec::new();
        let mut missing_packs: Vec<(String, usize)> = Vec::new();
        // (by the vehicle pack each mesh comes from, see `winding_pack`)
        let mut turned: Vec<(usize, String)> = Vec::new();
        let mut votes: std::collections::HashMap<String, WindingVotes> = std::collections::HashMap::new();
        if !model.lods.is_empty() {
            let start = model.lods[0].first_mesh;
            let end = model
                .lods
                .get(1)
                .map(|l| l.first_mesh)
                .unwrap_or(model.meshes.len());
            for (i, md) in model.meshes[start..end].iter().enumerate() {
                let p = mesh_path(root, &dir, &model_dir, &md.file);
                match omsi_o3d::load_mesh(&p) {
                    Ok(m) => {
                        let skin: Vec<SkinBone> = if md.smooth_skin {
                            m.bones
                                .iter()
                                .map(|b| {
                                    let id = md
                                        .bones
                                        .iter()
                                        .find(|(n, _)| {
                                            n.trim().eq_ignore_ascii_case(b.name.trim())
                                        })
                                        .map(|(_, id)| *id)
                                        .filter(|id| *id >= 0);
                                    SkinBone {
                                        def_index: id.map(|id| start + id as usize),
                                        weights: b
                                            .weights
                                            .iter()
                                            .map(|w| (w.vertex, w.weight))
                                            .collect(),
                                    }
                                })
                                .collect()
                        } else {
                            Vec::new()
                        };
                        // (a mesh none of whose bones is bound moves as a rigid one)
                        let skin = if skin.iter().any(|b| b.def_index.is_some()) { skin } else { Vec::new() };
                        let pack = winding_pack(&p);
                        match omsi_geometry::positive_det_faces_forward(&m) {
                            Some(forward) => votes.entry(pack.clone()).or_default().record(forward, m.triangles.len()),
                            None => {}
                        }
                        if omsi_geometry::turns_round(&m) {
                            turned.push((meshes.len(), pack));
                        }
                        meshes.push(VehicleMesh {
                            def_index: start + i,
                            data: mesh_from_o3d(&m),
                            file: p,
                            materials: m.materials.clone(),
                            overrides: md.materials.clone(),
                            pivot: pivot_from_mesh(&m),
                            viewpoint: md.viewpoint,
                            skin,
                            keep_winding: false,
                        })
                    }
                    Err(e) => {
                        match omsi_cfg::missing_vehicle_pack(&p) {
                            Some(pack) => match missing_packs.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(&pack)) {
                                Some(m) => m.1 += 1,
                                None => missing_packs.push((pack, 1)),
                            },
                            None => log::warn!("{}: {e}", p.display()),
                        }
                    }
                }
            }
        }
        // The lower levels of detail (`[LOD]` 2 and on): the meshes a vehicle is drawn with
        // from farther away, as Omsi.exe picks them by the vehicle's size on the screen. They
        // are small (a few thousand triangles against tens of thousands) and kept whole, and
        // they stay out of `meshes`: nothing that reads the model's real parts (the wheels,
        // the animations, the lamps, the boxes) is to take them for parts.
        let mut lod_meshes = Vec::new();
        for l in 1..model.lods.len() {
            let first = model.lods[l].first_mesh;
            for (i, md) in model.lod_meshes(l).iter().enumerate() {
                let p = mesh_path(root, &dir, &model_dir, &md.file);
                let Ok(m) = omsi_o3d::load_mesh(&p) else { continue };
                if m.triangles.is_empty() {
                    continue;
                }
                lod_meshes.push(VehicleMesh {
                    def_index: first + i,
                    data: mesh_from_o3d(&m),
                    file: p,
                    materials: m.materials.clone(),
                    overrides: md.materials.clone(),
                    pivot: pivot_from_mesh(&m),
                    viewpoint: md.viewpoint,
                    skin: Vec::new(),
                    keep_winding: false,
                });
            }
        }
        // A pack whose meshes with a positive determinant mostly face along their normals
        // keeps the winding of its backwards ones (see bb0c32b: the Citelis' buttons). Asked
        // of the whole vehicle, a part borrowed from another pack - a ticket machine, a
        // display - was turned in one bus and kept in the next, whatever its own pack's
        // exporter does: the same Atron machine inside out in some buses only (#977, #1054).
        let mut kept = 0;
        for (i, pack) in &turned {
            if votes.get(pack).is_some_and(WindingVotes::keep_authored) {
                omsi_geometry::reverse_winding(&mut meshes[*i].data);
                meshes[*i].keep_winding = true;
                kept += 1;
            }
        }
        if kept > 0 {
            log::info!("{}: {kept} meshes keep their winding (their packs' meshes with a positive determinant face along their normals: {votes:?})", bus_file.display());
        }
        for (pack, n) in &missing_packs {
            log::warn!(
                "{}: {n} meshes come from the vehicle pack 'Vehicles/{pack}', which is not installed - install it for the parts this bus borrows from it (displays, ticket machine, dashboard)",
                bus_file.display()
            );
        }
        let mut paint_schemes = Vec::new();
        for c in &model.ctc {
            let d = omsi_cfg::resolve_path(&dir, &c.path);
            paint_schemes.extend(load_paint_schemes(&d));
        }
        // the model's own items, after the `.cti` files' (their textures in the first
        // `[CTC]` folder, as a `.cti` of that folder has them)
        let item_dir = model.ctc.first().map(|c| omsi_cfg::resolve_path(&dir, &c.path)).unwrap_or_else(|| dir.clone());
        for it in &model.items {
            match paint_schemes.last_mut().filter(|s: &&mut PaintScheme| s.name.eq_ignore_ascii_case(&it.name) && s.dir == item_dir) {
                Some(s) => {
                    s.textures.push((it.ctc.clone(), it.texture.clone()));
                    s.set_vars.extend(it.set_vars.iter().cloned());
                }
                None => paint_schemes.push(PaintScheme { name: it.name.clone(), dir: item_dir.clone(), textures: vec![(it.ctc.clone(), it.texture.clone())], set_vars: it.set_vars.clone() }),
            }
        }
        let texchanges = omsi_model::load_texchanges(&dir, &model.texchanges);
        let (wheel_meshes, suspension_axles) = wheel_meshes(&model, &meshes);
        let mesh_boxes: Vec<(Vec3, Vec3)> = meshes
            .iter()
            .map(|m| {
                let lo = m.data.positions.iter().fold(Vec3::splat(f32::MAX), |a, p| a.min(*p));
                let hi = m.data.positions.iter().fold(Vec3::splat(f32::MIN), |a, p| a.max(*p));
                if lo.x <= hi.x { (lo, hi) } else { (Vec3::ZERO, Vec3::ZERO) }
            })
            .collect();
        let mesh_bounds = if keep_meshes {
            meshes.iter().map(|m| omsi_geometry::bounding_sphere(&m.data.positions)).collect()
        } else {
            vec![(Vec3::ZERO, 0.0); meshes.len()]
        };
        // A `[boundingbox]` much longer or wider than the model itself: the W906 Sprinter mod
        // declares 12 m for a 7 m van (copied from a bus), and the walkers ran into an
        // invisible wall metres ahead of its nose and behind its tail, the traffic kept its
        // distance from nothing. The box is cut down to the model (never made bigger).
        let mut def = def;
        if def.is_rail() && def.pantograph_var.is_none() {
            if let Some(var) = detect_pantograph(&model, &meshes, &program) {
                log::info!("{}: its pantograph is `{var}` (found from its model)", bus_file.display());
                def.pantograph_trigger = def.pantograph_trigger.take().or_else(|| triggers_storing(&program, &var).into_iter().next());
                def.pantograph_var = Some(var);
                def.pantograph_detected = true;
            }
        }
        if def.trolley.is_empty() {
            def.trolley = legacy_trolley(&model, &meshes, &mesh_boxes, &program, bus_file, keep_meshes);
            take_model_poles(&mut model, &meshes, &mesh_boxes, &def.trolley);
            // the raised pole's own mesh follows the pole: two animations about its hinge
            if let Some(l) = def.trolley.legacy.as_ref() {
                for (k, di) in l.mesh_defs.iter().enumerate() {
                    if let (Some(md), Some(p)) = (model.meshes.get_mut(*di), def.trolley.poles.get(k)) {
                        md.animations.extend(pole_animations(k + 1, p.base, p.work_angle));
                        // (the animations look their variables up in the program: declared here,
                        // written by the gear every step)
                        for n in [format!("Trolley_Pole{}_Elev", k + 1), format!("Trolley_Pole{}_Azim", k + 1)] {
                            program.declare_var(&n);
                        }
                    }
                }
            }
        }
        if let Some(bb) = def.bounding_box.as_mut() {
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for m in &meshes {
                for p in &m.data.positions {
                    lo = lo.min(*p);
                    hi = hi.max(*p);
                }
            }
            if hi.x > lo.x && hi.y > lo.y {
                let size = hi - lo;
                let mid = (hi + lo) * 0.5;
                if bb[1] > size.y + 1.5 {
                    log::info!("{}: [boundingbox] {:.1} m long, the model {:.1} m: the model's length taken", bus_file.display(), bb[1], size.y);
                    bb[1] = size.y + 0.1;
                    bb[4] = mid.y;
                }
                if bb[0] > size.x + 0.8 {
                    bb[0] = size.x + 0.05;
                    bb[3] = mid.x;
                }
            }
        }
        if !keep_meshes {
            // a `[smoothskin]` mesh (the bellows of an articulated bus, `[setbone]`-bound to
            // the joint's dummies) keeps its vertices and its bones even for an AI copy: it
            // is reshaped as the joint turns (`scene::own_skinned_meshes`), which needs the
            // rest pose and the bone weights this would otherwise throw away - an AI-loaded
            // GN92 or O530G kept its bellows perfectly straight through every bend because
            // there was nothing left to skin it from. There are only ever one or two such
            // meshes in a model, so keeping them costs nothing like the half gigabyte this
            // was written to save.
            for m in meshes.iter_mut().filter(|m| m.skin.is_empty()) {
                m.data = MeshData {
                    ranges: std::mem::take(&mut m.data.ranges),
                    ..Default::default()
                };
            }
        }
        Ok(VehicleType {
            def,
            model,
            model_dir,
            program: Arc::new(program),
            meshes,
            lod_meshes,
            paint_schemes,
            texchanges,
            wheel_meshes,
            suspension_axles,
            missing_packs,
            mesh_bounds,
            mesh_boxes,
        })
    }

    /// Per axle of the `.bus` (whose first axle is number `first_axle` of the whole train):
    /// the model's wheel centre height and tyre radius. The largest mesh turning with the
    /// axle is its tyre (a rim or a hub cap turns with it); one standing elsewhere along the
    /// bus belongs to another axle.
    pub fn wheel_geometry(&self, first_axle: usize) -> Vec<Option<(f32, f32)>> {
        self.def
            .axles
            .iter()
            .enumerate()
            .map(|(a, axle)| {
                self.wheel_meshes
                    .iter()
                    .filter(|(n, _, long, r)| {
                        *n == first_axle + a
                            && (long - axle.long).abs() <= 0.5
                            && *r >= axle.wheel_diameter * 0.3
                            && *r <= axle.wheel_diameter * 0.75
                    })
                    .fold(None, |best: Option<(f32, f32)>, (_, z, _, r)| {
                        if best.map(|b| *r > b.1 + 1e-3).unwrap_or(true) {
                            Some((*z, *r))
                        } else {
                            best
                        }
                    })
            })
            .collect()
    }

    /// Unloaded hub height per axle for the physics: the model's wheel centre, raised by what
    /// the `.bus` tyre is larger than the modelled one. The tyre then touches the road exactly
    /// where the physics puts the contact - the stock SD202's wheels are modelled 1.6 cm
    /// below the model's ground plane and stood that much in the asphalt. A model that says
    /// something implausible (more than 8 cm off the tyre radius) keeps the radius.
    ///
    /// A model that does not move an axle's wheels with `Axle_Suspension` (a railway car, a
    /// plane) cannot show them sitting up in their arches: that axle hangs its wheels as much
    /// lower as the load compresses the spring, and the body keeps its unloaded height.
    pub fn hub_heights(&self, first_axle: usize) -> Vec<Option<f32>> {
        let loads = crate::rigid::wheel_rest_loads(&self.def);
        self.def
            .axles
            .iter()
            .zip(self.wheel_geometry(first_axle))
            .enumerate()
            .map(|(a, (axle, g))| {
                let r = (axle.wheel_diameter / 2.0).max(0.15);
                let model = g
                    .map(|(z, r_mesh)| z + (r - r_mesh))
                    .filter(|z| (z - r).abs() <= 0.08);
                if self.suspension_axles.contains(&(first_axle + a)) {
                    return model;
                }
                let k = if axle.spring > 0.0 {
                    axle.spring * 1000.0
                } else {
                    150_000.0
                };
                let sag = (loads.get(a).copied().unwrap_or(0.0) / k).min(crate::rigid::BUMP);
                Some(model.unwrap_or(r) - sag)
            })
            .collect()
    }

    /// Texture substitutions of a paint scheme: default `[CTCTexture]` file (lower case) →
    /// replacement file, plus the folder to search first.
    pub fn scheme_substitutions(
        &self,
        scheme: usize,
        find_texture: fn(&str, &[&Path]) -> Option<PathBuf>,
    ) -> (HashMap<String, String>, Option<PathBuf>) {
        let mut map = HashMap::new();
        let Some(s) = self.paint_schemes.get(scheme) else {
            return (map, None);
        };
        for (ctc_name, file) in &s.textures {
            // The picture is the scheme folder's, named in full; one the folder has not
            // is looked for where the model's own textures are. Only what the `.cti` names
            // comes from the scheme folder: the folder is no search path for the rest of
            // the model's textures - with it first in line, a same-named file left there
            // for another scheme (the IK-103's yellow `sedista-diffuse.bmp` beside the grey
            // bars schemes) painted the details of every scheme of the folder.
            let value = match find_texture(file, &[s.dir.as_path()]) {
                Some(full) => full.to_string_lossy().into_owned(),
                None => file.clone(),
            };
            for (name, default) in &self.model.ctc_textures {
                if name.eq_ignore_ascii_case(ctc_name) {
                    map.insert(default.to_ascii_lowercase(), value.clone());
                }
            }
        }
        (map, Some(s.dir.clone()))
    }

    /// The texture swaps of the model's own look (no paint scheme chosen): none for the
    /// livery, but a `[CTCTexture]` whose own texture is a flat, colourless placeholder
    /// takes the first scheme's. The BMC Procity's destination display is lit through its
    /// CTC texture `afisaj`: the model's own `vmatrix_voll_LCD.dds` is plain grey and every
    /// repaint brings the orange of the LEDs - with the model's own textures the display
    /// shone white. A body texture is never a flat colour, so the livery stays the model's.
    /// Values are full paths.
    pub fn default_substitutions(&self, root: &Path) -> HashMap<String, String> {
        static CACHE: std::sync::OnceLock<parking_lot::Mutex<HashMap<PathBuf, HashMap<String, String>>>> = std::sync::OnceLock::new();
        let cache = CACHE.get_or_init(|| parking_lot::Mutex::new(HashMap::new()));
        if let Some(m) = cache.lock().get(&self.def.path) {
            return m.clone();
        }
        let mut map = HashMap::new();
        if let Some(first) = self.paint_schemes.first() {
            let dirs = self.texture_dirs(root);
            for (name, default) in &self.model.ctc_textures {
                let Some((_, file)) = first.textures.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) else {
                    continue;
                };
                // only a display's colour: a material that shows a script or text texture
                // through this one (`[matl_transmap] \S:n`, [useScriptTexture],
                // [useTextTexture]). Window paint and a body's transparency map are CTC
                // textures of one flat tone as well (the LiAZ's `Kuzov_trans.dds`), and the
                // first repaint's put its livery over the windows as large splashes.
                let display = self.model.meshes.iter().flat_map(|m| m.materials.iter()).any(|o| {
                    o.texture.trim().eq_ignore_ascii_case(default.trim())
                        && (o.transmap.as_deref().is_some_and(|t| t.trim().starts_with("\\S:"))
                            || o.use_script_texture.is_some()
                            || o.use_text_texture.is_some())
                });
                if !display {
                    continue;
                }
                let own = dirs.iter().map(|d| omsi_cfg::resolve_path(d, default)).find(|p| omsi_cfg::vfs::is_file(p));
                let Some(own) = own else { continue };
                if !flat_placeholder(&own) {
                    continue;
                }
                let theirs = omsi_cfg::resolve_path(&first.dir, file);
                if omsi_cfg::vfs::is_file(&theirs) {
                    log::info!("{}: CTC texture '{name}' is a flat placeholder ({default}); the first paint scheme's {file} lights it", self.def.path.display());
                    map.insert(default.to_ascii_lowercase(), theirs.to_string_lossy().into_owned());
                }
            }
        }
        cache.lock().insert(self.def.path.clone(), map.clone());
        map
    }

    /// The `[texchanges]` master for a mesh material's texture name, if there is one.
    pub fn texchange(&self, texture: &str) -> Option<&omsi_model::TexChangeMaster> {
        self.texchanges
            .iter()
            .find(|m| m.texture.eq_ignore_ascii_case(texture.trim()))
    }

    /// Bytes of the meshes the type keeps on the CPU.
    pub fn mesh_bytes(&self) -> usize {
        self.meshes.iter().map(|m| m.data.heap_bytes()).sum()
    }

    /// Mesh `i` with its vertices: as kept, or read again from its file (a type loaded with
    /// [`VehicleType::load_ai`]). None when the file cannot be read any more.
    pub fn mesh_data(&self, i: usize) -> Option<std::borrow::Cow<'_, MeshData>> {
        let m = self.meshes.get(i)?;
        if !m.data.indices.is_empty() || m.data.ranges.is_empty() {
            return Some(std::borrow::Cow::Borrowed(&m.data));
        }
        match omsi_o3d::load_mesh(&m.file) {
            Ok(o) => Some(std::borrow::Cow::Owned(omsi_geometry::mesh_from_o3d_turning(&o, !m.keep_winding))),
            Err(e) => {
                log::warn!("{}: {e}", m.file.display());
                None
            }
        }
    }

    /// Whether mesh `i` has to be read again before it can go to the GPU.
    pub fn mesh_dropped(&self, i: usize) -> bool {
        self.meshes
            .get(i)
            .map(|m| m.data.indices.is_empty() && !m.data.ranges.is_empty())
            .unwrap_or(false)
    }

    pub fn texture_dirs(&self, root: &Path) -> Vec<PathBuf> {
        vec![
            // (the folder as it is spelled on the disk: many add-ons ship `texture`, which a
            // case-sensitive file system - Linux, a phone - does not find as `Texture`)
            omsi_cfg::resolve_path(self.def.dir(), "Texture"),
            omsi_cfg::resolve_path(&self.model_dir, "Texture"),
            self.model_dir.clone(),
            omsi_cfg::resolve_path(root, "Texture"),
        ]
    }
}

/// A texture of one flat, colourless tone (a stand-in the paint schemes replace).
pub(super) fn flat_placeholder(p: &Path) -> bool {
    let Ok(img) = crate::image_decode::decode(p) else { return false };
    let n = (img.width as usize * img.height as usize).max(1);
    let step = (n / 4096).max(1);
    let (mut sum, mut sq, mut k) = ([0f64; 3], [0f64; 3], 0f64);
    for px in img.rgba.chunks(4).step_by(step) {
        for c in 0..3 {
            let v = px[c] as f64;
            sum[c] += v;
            sq[c] += v * v;
        }
        k += 1.0;
    }
    if k < 1.0 {
        return false;
    }
    let mean: Vec<f64> = sum.iter().map(|s| s / k).collect();
    let spread = (0..3).map(|c| (sq[c] / k - mean[c] * mean[c]).max(0.0).sqrt()).fold(0.0, f64::max);
    let chroma = mean.iter().cloned().fold(0.0, f64::max) - mean.iter().cloned().fold(255.0, f64::min);
    // (a few black cells for the unlit part of a matrix are allowed: the median tone matters)
    spread < 40.0 && chroma < 20.0
}

/// The tyres of a model - every mesh turned by `Wheel_Rotation_<n>_*`, with its centre and
/// its radius about the axle - and the axle numbers it moves with `Axle_Suspension_<n>_*`.
pub(super) fn wheel_meshes(
    model: &Model,
    meshes: &[VehicleMesh],
) -> (Vec<(usize, f32, f32, f32)>, Vec<usize>) {
    let number = |var: &str, prefix: &str| {
        var.to_ascii_lowercase()
            .strip_prefix(prefix)
            .and_then(|r| r.split('_').next().and_then(|a| a.parse::<usize>().ok()))
    };
    let mut wheels = Vec::new();
    let mut suspension = Vec::new();
    for m in meshes {
        for an in &model.meshes[m.def_index].animations {
            if let Some(n) = number(&an.variable, "axle_suspension_") {
                if !suspension.contains(&n) {
                    suspension.push(n);
                }
            }
            if an.kind != Some(omsi_model::AnimKind::Rot) {
                continue;
            }
            let Some(n) = number(&an.variable, "wheel_rotation_") else {
                continue;
            };
            let mut pivot = None;
            for o in &an.origins {
                match o {
                    omsi_model::AnimOrigin::Trans(t) => {
                        pivot = Some(pivot.unwrap_or(Vec3::ZERO) + Vec3::from(*t))
                    }
                    omsi_model::AnimOrigin::FromMesh => pivot = Some(m.pivot.w_axis.truncate()),
                    _ => {}
                }
            }
            let Some(p) = pivot else { continue };
            let radius = m
                .data
                .positions
                .iter()
                .map(|v| ((v.y - p.y).powi(2) + (v.z - p.z).powi(2)).sqrt())
                .fold(0.0f32, f32::max);
            wheels.push((n, p.z, p.y, radius));
        }
    }
    (wheels, suspension)
}

/// A texture name as the bytes Omsi.exe sees: the `.o3d` keeps a name in the file's own
/// bytes (read here one char a byte: a Russian "Правый борт.tga" is "Ïðàâûé áîðò.tga"), the
/// `.cfg` was read as Windows-1251 text (the Cyrillic itself). Both go back to the one byte a
/// char - so that a `[matl_alpha]` of a mod's glass finds its slot, and the windows of a
/// Russian bus are not opaque.
pub(crate) fn raw_name(name: &str) -> String {
    name.chars()
        .map(|c| match c as u32 {
            0..=0xFF => c,
            0x410..=0x44F => char::from_u32(c as u32 - 0x410 + 0xC0).unwrap_or(c),
            0x401 => '\u{A8}',
            0x451 => '\u{B8}',
            _ => c,
        })
        .collect()
}

/// Index of the material slot a `[matl]` override refers to (texture name + nth occurrence).
pub fn override_slot(materials: &[omsi_o3d::Material], o: &MaterialDef) -> Option<usize> {
    let mut nth = 0;
    let wanted = raw_name(&o.texture);
    for (slot, m) in materials.iter().enumerate() {
        if m.texture.eq_ignore_ascii_case(&o.texture) || raw_name(&m.texture).eq_ignore_ascii_case(&wanted) {
            if nth == o.index.max(0) as usize {
                return Some(slot);
            }
            nth += 1;
        }
    }
    None
}

#[cfg(test)]
mod winding_exporter_tests {
    use super::{keep_authored_winding, winding_pack, WindingVotes, WINDING_EVIDENCE_CAP};
    use std::path::Path;

    #[test]
    fn borrowed_pack_evidence_stays_separate_and_capped() {
        let body = winding_pack(Path::new("/omsi/Vehicles/Bus/model/body.o3d"));
        let borrowed = winding_pack(Path::new("/omsi/Vehicles/Bus/model/../../Display/model/display.o3d"));
        let mut votes: std::collections::HashMap<String, WindingVotes> = std::collections::HashMap::new();
        for _ in 0..4 {
            votes.entry(body.clone()).or_default().record(true, usize::MAX);
        }
        for _ in 0..5 {
            votes.entry(body.clone()).or_default().record(false, 2);
        }
        votes.entry(borrowed.clone()).or_default().record(false, 100);
        assert!(votes[&body].keep_authored());
        assert_eq!(votes[&body].forward_weight, 4 * WINDING_EVIDENCE_CAP);
        assert!(!votes[&borrowed].keep_authored());
        let local_display = winding_pack(Path::new("/omsi/Vehicles/Display/model/display.o3d"));
        assert_eq!(borrowed, local_display);
    }

    #[test]
    fn empty_or_inconclusive_votes_keep_default() {
        assert!(!WindingVotes::default().keep_authored());
        assert!(!keep_authored_winding(0, 5, 0, 10));
        assert!(!keep_authored_winding(5, 5, 10, 10));
    }

    #[test]
    fn close_mesh_count_can_use_triangle_evidence() {
        assert!(keep_authored_winding(64, 74, 91_597, 79_544));
    }

    #[test]
    fn clear_backward_majority_is_not_overridden() {
        assert!(!keep_authored_winding(40, 80, 120_000, 60_000));
    }

    #[test]
    fn forward_majority_keeps_existing_behaviour() {
        assert!(keep_authored_winding(80, 40, 1, 1));
    }
}

/// The variable that moves a rail vehicle's pantograph, read off its model: the meshes whose file
/// names say pantograph (`PANT_1.o3d`, `Pantograf`, `Stromabnehmer`) and the variable their
/// animations turn on - which is `Panto_anim_oben` in a tram made for it, and `door_0` in
/// the Tatra T6B5, a bus's scripts with a tram's body, whose pantograph is the "door" the bus
/// script already knows how to open and close. The most-used variable wins.
pub(crate) fn detect_pantograph(model: &omsi_model::Model, meshes: &[VehicleMesh], program: &Program) -> Option<String> {
    let named = |f: &str| {
        let l = f.to_ascii_lowercase();
        ["pantograph", "pantograf", "panto", "pant_", "pant.", "\\pant", "/pant", "stromabnehmer", "токоприемник", "пантограф"].iter().any(|k| l.contains(k))
    };
    let mut count: Vec<(String, usize)> = Vec::new();
    for m in meshes {
        let Some(md) = model.meshes.get(m.def_index) else { continue };
        if !named(&md.file) {
            continue;
        }
        for a in md.animations.iter().filter(|a| !a.variable.is_empty() && !a.variable.to_ascii_lowercase().starts_with("trolley_") && program.var(&a.variable).is_some()) {
            match count.iter_mut().find(|(v, _)| v.eq_ignore_ascii_case(&a.variable)) {
                Some((_, n)) => *n += 1,
                None => count.push((a.variable.clone(), 1)),
            }
        }
    }
    count.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    count.into_iter().next().map(|(v, _)| v)
}

/// The mouse-event triggers (and macros they call) of `program` that write variable `var`,
/// first the ones a cabin switch is likely to be (names with `door`, `panto` or `tuer` first).
pub(crate) fn triggers_storing(program: &Program, var: &str) -> Vec<String> {
    fn stores(program: &Program, block: omsi_script::BlockId, id: omsi_script::VarId, seen: &mut hashbrown::HashSet<omsi_script::BlockId>) -> bool {
        if !seen.insert(block) || seen.len() > 64 {
            return false;
        }
        let Some(b) = program.blocks.get(block as usize) else { return false };
        b.ops.iter().any(|op| match op {
            omsi_script::Op::Store(v) => *v == id,
            omsi_script::Op::Macro(m) => stores(program, *m, id, seen),
            _ => false,
        })
    }
    let Some(id) = program.var(var) else { return Vec::new() };
    let mut out: Vec<String> = program.triggers.iter().filter(|(_, b)| stores(program, **b, id, &mut hashbrown::HashSet::new())).map(|(n, _)| n.clone()).collect();
    if out.is_empty() {
        // a door script: the trigger sets the leaf's target and the script moves the leaf
        let leaf = var.to_ascii_lowercase();
        out = program.triggers.keys().filter(|n| crate::door_keys::trigger_leaves(program, n).iter().any(|l| *l == leaf)).cloned().collect();
    }
    out.sort();
    // (the opening trigger of a pair `x` / `x_close`, the shortest name, first)
    out.sort_by_key(|n| {
        let l = n.to_ascii_lowercase();
        (!(l.contains("panto") || l.contains("door") || l.contains("tuer") || l.contains("pant")), l.ends_with("_close") || l.ends_with("_off") || l.ends_with("_cl"), l.len())
    });
    out
}

/// A trolleybus made for OMSI 2 as it comes (no `[trolley_*]` blocks): its model switches two
/// meshes by `cp_SHTANGALEV` / `cp_SHTANGAPRAV` (0 folded on the roof, 1 raised), and no script
/// reads them. The poles' hinges and length are read off the raised meshes (the hinge is the
/// low end that lies furthest forward, the shoe the high end furthest back). The bus keeps its
/// own script's drive; whether its control is a rheostat controller or a chopper is read off
/// the description ("реостатно-контакторная", "тиристорно-импульсная", IGBT, "асинхронный").
/// The animations that turn a pole's raised mesh (authored at its working angle `work_deg`, hinged
/// at `base`) to where the pole is: its elevation about the vehicle's x axis through the hinge,
/// then its azimuth about the vertical one (`Trolley_Pole<k>_Elev`, `_Azim`, in degrees: the rig's
/// own angles, see `rotate_tip`: the boom trails to -y, up by the elevation, out by the azimuth).
fn pole_animations(k: usize, base: Vec3, work_deg: f32) -> Vec<omsi_model::Animation> {
    use omsi_model::{AnimKind, AnimOrigin, Animation};
    let hinge = AnimOrigin::Trans(base.to_array());
    vec![
        Animation { origins: vec![hinge.clone()], kind: Some(AnimKind::Rot), variable: format!("Trolley_Pole{k}_Elev"), factor: 1.0, offset: -work_deg, ..Default::default() },
        // (origin_rot_y 90 turns the animated axis, the x of its frame, up)
        Animation { origins: vec![hinge, AnimOrigin::RotY(90.0)], kind: Some(AnimKind::Rot), variable: format!("Trolley_Pole{k}_Azim"), factor: -1.0, offset: 0.0, ..Default::default() },
    ]
}

const POLE_NAMES: &[&str] = &["shtang", "штанг", "stange", "palk", "палк", "pole", "boom", "szedo", "szedő"];
const ROPE_NAMES: &[&str] = &["verevk", "веревк", "верёвк", "канат", "seil"];

fn file_named(file: &str, names: &[&str]) -> bool {
    let stem = file.rsplit(['\\', '/']).next().unwrap_or(file).to_lowercase();
    names.iter().any(|n| stem.contains(n))
}

fn take_model_poles(model: &mut omsi_model::Model, meshes: &[VehicleMesh], boxes: &[(Vec3, Vec3)], trolley: &omsi_trolley::TrolleyDef) {
    let Some(l) = trolley.legacy.as_ref().filter(|l| l.switches.is_empty()) else { return };
    let mut shown_by: Vec<String> = Vec::new();
    for di in &l.mesh_defs {
        if let Some(md) = model.meshes.get_mut(*di) {
            shown_by.extend(md.visible.take().map(|(v, _)| v.to_ascii_lowercase()));
            md.visible_any.clear();
            md.animations.clear();
        }
    }
    let never = vec![vec![omsi_model::VisibleCond { variable: "Trolley_Pole1_Elev".into(), lo: 1000.0, hi: 1001.0, range: true, not: false }]];
    for (m, (lo, hi)) in meshes.iter().zip(boxes) {
        let Some(md) = model.meshes.get_mut(m.def_index).filter(|_| !l.mesh_defs.contains(&m.def_index)) else { continue };
        let same_switch = md.visible.as_ref().is_some_and(|(v, _)| shown_by.contains(&v.to_ascii_lowercase()));
        let pole_shaped = hi.y - lo.y >= 3.0 && hi.x - lo.x <= 0.7 && lo.z >= 2.0;
        if (same_switch && (pole_shaped || file_named(&md.file, POLE_NAMES))) || file_named(&md.file, ROPE_NAMES) {
            md.visible_any = never.clone();
        }
    }
}

/// Whether nearly all of a mesh's vertices lie along the line from `base` to `tip`: a rod. A
/// light assembly or a roof rail fills the same bounding box without following its diagonal.
fn rod_along(positions: &[Vec3], base: Vec3, tip: Vec3) -> bool {
    let axis = tip - base;
    let len2 = axis.length_squared();
    if positions.is_empty() || len2 <= f32::EPSILON {
        return false;
    }
    let near = positions
        .iter()
        .filter(|p| {
            let t = ((**p - base).dot(axis) / len2).clamp(0.0, 1.0);
            (**p - (base + axis * t)).length() <= 0.4
        })
        .count();
    near as f32 >= positions.len() as f32 * 0.85
}

/// Words for a lamp or a light in the languages the add-ons come in; a mesh named so is a light.
const LIGHT_NAMES: &[&str] = &[
    "light", "lamp", "licht", "leucht", "beleucht", "lumi", "luce", "luz", "lys", "ljus", "valo", "lampa", "swiatl", "światł", "svetil", "svetl", "fenyszoro",
    "fény", "világít", "ışık", "isik", "свет", "ламп", "плафон", "фонар", "освещ", "підсвіт", "світл", "照明", "灯", "ライト", "라이트",
];

/// A mesh that is a light and never a pole: named like one, or lit by its own materials (a light
/// map or a glow).
fn is_light(md: &omsi_model::MeshDef, mesh: &VehicleMesh) -> bool {
    file_named(&md.file, LIGHT_NAMES)
        || mesh.overrides.iter().any(|m| !m.lightmaps.is_empty() || m.lightmap.is_some() || m.glow.is_some())
}

fn legacy_trolley(model: &omsi_model::Model, meshes: &[VehicleMesh], boxes: &[(Vec3, Vec3)], program: &Program, bus_file: &Path, static_poles: bool) -> omsi_trolley::TrolleyDef {
    use omsi_trolley::{LegacyDef, PoleDef, TrolleyDef};
    let mut poles: Vec<PoleDef> = Vec::new();
    let mut switches: Vec<String> = Vec::new();
    let mut mesh_defs: Vec<usize> = Vec::new();
    for switched in [true, false] {
        if !switched && !static_poles {
            break;
        }
        poles.clear();
        switches.clear();
        mesh_defs.clear();
        for (i, m) in meshes.iter().enumerate() {
            let Some(md) = model.meshes.get(m.def_index) else { continue };
            if is_light(md, m) {
                continue;
            }
            let mut unnamed = false;
            let var = if switched {
                let Some((var, value)) = md.visible.as_ref() else { continue };
                let low = var.to_ascii_lowercase();
                if !(low.contains("shtang") || low.contains("pole") || low.contains("штанг")) || *value != 1.0 || program.var(var).is_none() {
                    continue;
                }
                Some(var)
            } else if file_named(&md.file, POLE_NAMES) {
                None
            } else if md.visible_any.is_empty() && md.animations.is_empty() {
                // (a plain trolleybus names nothing: its poles are two unanimated rods - the
                // shape is checked below, since long interior lights fit the same box)
                unnamed = true;
                None
            } else {
                continue;
            };
            let Some((lo, hi)) = boxes.get(i).copied().filter(|(l, h)| h.x > l.x && h.y > l.y) else { continue };
            let side: i8 = if (lo.x + hi.x) * 0.5 < 0.0 { -1 } else { 1 };
            if poles.iter().any(|p| p.side == side) {
                continue;
            }
            let base = Vec3::new((lo.x + hi.x) * 0.5, hi.y, lo.z);
            let tip = Vec3::new((lo.x + hi.x) * 0.5, lo.y, hi.z);
            let boom = tip - base;
            if unnamed && !meshes.get(i).is_some_and(|m| rod_along(&m.data.positions, base, tip)) {
                continue;
            }
            // (a pole is long and thin: a short or a wide mesh whose visibility variable has "pole" in
            // its name - an antenna, a flag - is no trolley pole)
            if boom.length() < 3.0 || hi.x - lo.x > 1.2 {
                continue;
            }
            if var.is_none() && (hi.x - lo.x > 0.6 || lo.z < 2.0 || boom.z < 1.0 || (lo.x + hi.x).abs() > 2.0) {
                continue;
            }
            let length = boom.length().clamp(3.0, 12.0);
            poles.push(PoleDef {
                side,
                base,
                length,
                rest_angle: 2.0,
                work_angle: (boom.z / length).clamp(-1.0, 1.0).asin().to_degrees().clamp(8.0, 45.0),
                motor: true,
                retriever: 40.0,
                ..Default::default()
            });
            switches.extend(var.cloned());
            mesh_defs.push(m.def_index);
        }
        if poles.len() >= 2 {
            break;
        }
    }
    if poles.len() < 2 {
        return TrolleyDef::default();
    }
    // (the left pole first, as the cab's `LEV`/`PRAV` say)
    let mut order: Vec<usize> = (0..poles.len()).collect();
    order.sort_by_key(|k| poles[*k].side);
    let poles2 = order.iter().map(|k| poles[*k].clone()).collect();
    let switches2 = order.iter().filter_map(|k| switches.get(*k).cloned()).collect();
    let mesh_defs2 = order.iter().map(|k| mesh_defs[*k]).collect();
    let text: String = std::fs::read(bus_file).map(|b| omsi_cfg::decode_text(&b)).unwrap_or_default();
    let low = text.to_lowercase();
    let chopper = ["тиристор", "igbt", "импульс", "асинхрон", "инвертор", "транзистор", "thyristor", "chopper", "inverter"].iter().any(|k| low.contains(k));
    log::info!("{}: a plain trolleybus ({} poles from its model, {} control)", bus_file.display(), poles.len(), if chopper { "chopper" } else { "rheostat" });
    TrolleyDef { poles: poles2, legacy: Some(LegacyDef { switches: switches2, chopper, mesh_defs: mesh_defs2 }), aux_kw: if chopper { 4.0 } else { 3.0 }, ..Default::default() }
}

#[cfg(test)]
mod raw_name_tests {
    use super::raw_name;

    /// A Russian texture name as the cfg has it (Cyrillic) and as the `.o3d` is read (one char a
    /// byte) is the same name.
    #[test]
    fn a_cyrillic_name_and_its_bytes_are_one_name() {
        for (cfg, o3d) in [
            ("Правый борт new.tga", "Ïðàâûé áîðò new.tga"),
            ("Стёкла водительские левые.tga", "Ñò¸êëà âîäèòåëüñêèå ëåâûå.tga"),
        ] {
            assert_eq!(raw_name(cfg), o3d, "{cfg}");
            assert_eq!(raw_name(o3d), o3d);
        }
        assert_eq!(raw_name("Fenster_per.tga"), "Fenster_per.tga");
    }
}

#[cfg(test)]
mod pole_animation_tests {
    use super::pole_animations;
    use crate::anim::MeshAnimator;
    use glam::Vec3;
    use omsi_model::MeshDef;

    /// The animations carry a raised mesh's boom to where the rig's own angles put it:
    /// `rotate_tip` (the boom trails to -y, up by the elevation, out by the azimuth).
    #[test]
    fn a_pole_mesh_follows_the_rigs_angles() {
        let base = Vec3::new(0.4, -4.5, 3.1);
        let (work, len) = (25.0f32, 6.0f32);
        let tip_of = |th: f32, ps: f32| base + Vec3::new(ps.sin() * th.cos(), -ps.cos() * th.cos(), th.sin()) * len;
        let rest_tip = tip_of(work.to_radians(), 0.0);
        let mut def = MeshDef::default();
        def.animations = pole_animations(1, base, work);
        let names = ["trolley_pole1_elev", "trolley_pole1_azim"];
        let resolve = |n: &str| names.iter().position(|x| x.eq_ignore_ascii_case(n)).map(|i| i as omsi_script::VarId);
        for (elev, azim) in [(25.0f32, 0.0f32), (45.0, 0.0), (10.0, 0.0), (35.0, 30.0), (20.0, -40.0)] {
            let mut a = MeshAnimator::new(&def, glam::Mat4::IDENTITY, resolve);
            let m = a.update(0.016, &[elev, azim]);
            let moved = m.transform_point3(rest_tip);
            let want = tip_of(elev.to_radians(), azim.to_radians());
            assert!((moved - want).length() < 1e-3, "elev {elev} azim {azim}: {moved:?} vs {want:?}");
            // the hinge stays where it is
            assert!((m.transform_point3(base) - base).length() < 1e-4);
        }
    }
}

#[cfg(test)]
mod legacy_trolley_tests {
    use super::{legacy_trolley, take_model_poles, VehicleMesh};
    use glam::{Mat4, Vec3};
    use std::path::{Path, PathBuf};

    fn mesh(def_index: usize) -> VehicleMesh {
        VehicleMesh {
            def_index,
            data: Default::default(),
            file: PathBuf::new(),
            materials: Vec::new(),
            overrides: Vec::new(),
            pivot: Mat4::IDENTITY,
            viewpoint: 0,
            skin: Vec::new(),
            keep_winding: false,
        }
    }

    /// A rod from the box's rear-top corner to its front-bottom one, as a pole is.
    fn rod_mesh(def_index: usize, (lo, hi): (Vec3, Vec3)) -> VehicleMesh {
        let (base, tip) = (Vec3::new((lo.x + hi.x) * 0.5, hi.y, lo.z), Vec3::new((lo.x + hi.x) * 0.5, lo.y, hi.z));
        let mut m = mesh(def_index);
        m.data.positions = (0..=20).map(|k| base.lerp(tip, k as f32 / 20.0)).collect();
        m
    }

    /// A light assembly: strips along the length at the lower and the upper edge of its box.
    fn light_mesh(def_index: usize, (lo, hi): (Vec3, Vec3)) -> VehicleMesh {
        let x = (lo.x + hi.x) * 0.5;
        let mut m = mesh(def_index);
        m.data.positions = (0..=20)
            .flat_map(|k| {
                let y = lo.y + (hi.y - lo.y) * k as f32 / 20.0;
                [Vec3::new(x, y, lo.z), Vec3::new(x, y, hi.z)]
            })
            .collect();
        m
    }

    #[test]
    fn static_raised_pole_meshes_become_poles() {
        let mut model = omsi_model::Model::default();
        model.meshes = vec![Default::default(); 4];
        let boxes = [
            (Vec3::new(-1.2, -5.8, 2.8), Vec3::new(1.2, 6.0, 3.0)),
            (Vec3::new(-0.34, -6.26, 3.17), Vec3::new(-0.11, 0.27, 5.35)),
            (Vec3::new(0.11, -6.26, 3.17), Vec3::new(0.34, 0.27, 5.35)),
            (Vec3::new(0.5, 4.0, 3.0), Vec3::new(0.55, 4.3, 3.8)),
        ];
        let meshes: Vec<VehicleMesh> = (0..4).map(|i| rod_mesh(i, boxes[i])).collect();
        let program = omsi_script::Program::default();
        let bus = Path::new("/nonexistent/troll.bus");
        let def = legacy_trolley(&model, &meshes, &boxes, &program, bus, true);
        assert_eq!(def.poles.len(), 2);
        assert_eq!((def.poles[0].side, def.poles[1].side), (-1, 1));
        assert!((def.poles[0].work_angle - 18.5).abs() < 0.5, "{}", def.poles[0].work_angle);
        let legacy = def.legacy.expect("legacy");
        assert!(legacy.switches.is_empty());
        assert_eq!(legacy.mesh_defs, vec![1, 2]);
        assert!(legacy_trolley(&model, &meshes, &boxes, &program, bus, false).poles.is_empty());
        model.meshes[2].animations.push(Default::default());
        assert!(legacy_trolley(&model, &meshes, &boxes, &program, bus, true).poles.is_empty());
    }

    #[test]
    fn static_interior_meshes_do_not_identify_a_trolleybus() {
        let mut model = omsi_model::Model::default();
        model.meshes = ["interior_left.o3d", "interior_right.o3d"]
            .into_iter()
            .map(|file| omsi_model::MeshDef { file: file.into(), ..Default::default() })
            .collect();
        let boxes = [
            (Vec3::new(-0.75, -5.67, 2.49), Vec3::new(-0.43, 4.38, 4.45)),
            (Vec3::new(0.43, -5.67, 2.49), Vec3::new(0.75, 4.25, 4.45)),
        ];
        let meshes: Vec<VehicleMesh> = (0..2).map(|i| light_mesh(i, boxes[i])).collect();
        let program = omsi_script::Program::default();
        let bus = Path::new("/nonexistent/bus.bus");
        let def = legacy_trolley(&model, &meshes, &boxes, &program, bus, true);
        assert!(def.is_empty(), "interior geometry alone must not enable trolley traction");
        take_model_poles(&mut model, &meshes, &boxes, &def);
        assert!(model.meshes.iter().all(|md| md.animations.is_empty() && md.visible_any.is_empty()));
    }

    #[test]
    fn lights_are_never_poles() {
        let boxes = [
            (Vec3::new(-0.34, -6.26, 3.17), Vec3::new(-0.11, 0.27, 5.35)),
            (Vec3::new(0.11, -6.26, 3.17), Vec3::new(0.34, 0.27, 5.35)),
        ];
        let program = omsi_script::Program::default();
        let bus = Path::new("/nonexistent/b.bus");
        let run = |files: [&str; 2], lit: bool| {
            let mut model = omsi_model::Model::default();
            model.meshes = files.into_iter().map(|file| omsi_model::MeshDef { file: file.into(), ..Default::default() }).collect();
            let meshes: Vec<VehicleMesh> = (0..2)
                .map(|i| {
                    let mut m = rod_mesh(i, boxes[i]);
                    if lit {
                        m.overrides = vec![omsi_model::MaterialDef { lightmap: Some(("l.bmp".into(), "v".into())), ..Default::default() }];
                    }
                    m
                })
                .collect();
            legacy_trolley(&model, &meshes, &boxes, &program, bus, true).poles.len()
        };
        assert_eq!(run(["a.o3d", "b.o3d"], false), 2);
        assert_eq!(run(["pole_light_l.o3d", "pole_light_r.o3d"], false), 0);
        assert_eq!(run(["Лампа_л.o3d", "Лампа_п.o3d"], false), 0);
        assert_eq!(run(["Leuchte_links.o3d", "Beleuchtung_rechts.o3d"], false), 0);
        assert_eq!(run(["a.o3d", "b.o3d"], true), 0);
    }

    #[test]
    fn pole_control_variables_still_identify_unnamed_meshes() {
        let mut model = omsi_model::Model::default();
        let names = ["cp_SHTANGALEV", "cp_SHTANGAPRAV"];
        let mut program = omsi_script::Program::default();
        for name in names {
            program.declare_var(name);
            model.meshes.push(omsi_model::MeshDef { visible: Some((name.into(), 1.0)), ..Default::default() });
        }
        let meshes: Vec<VehicleMesh> = (0..2).map(mesh).collect();
        let boxes = [
            (Vec3::new(-0.34, -6.26, 3.17), Vec3::new(-0.11, 0.27, 5.35)),
            (Vec3::new(0.11, -6.26, 3.17), Vec3::new(0.34, 0.27, 5.35)),
        ];
        let def = legacy_trolley(&model, &meshes, &boxes, &program, Path::new("/nonexistent/b.bus"), false);
        assert_eq!(def.poles.len(), 2);
        assert_eq!(def.legacy.unwrap().switches, names);
    }

    #[test]
    fn named_animated_pole_meshes_are_taken_over() {
        let mut model = omsi_model::Model::default();
        let switched = |file: &str| {
            let mut md = omsi_model::MeshDef { file: file.into(), visible: Some(("knopka_massa".into(), 1.0)), ..Default::default() };
            md.visible_any = vec![vec![omsi_model::VisibleCond { variable: "knopka_massa".into(), lo: 1.0, ..Default::default() }]];
            md.animations.push(omsi_model::Animation { variable: "articulation_0_beta".into(), ..Default::default() });
            md
        };
        let mut folded = switched("GVM\\GVM_szedo_lent_akaszt2.o3d");
        folded.visible = Some(("knopka_massa".into(), 0.0));
        folded.visible_any[0][0].lo = 0.0;
        folded.animations.clear();
        model.meshes = vec![switched("GVM\\Shtanga_podn_L.o3d"), switched("GVM\\Shtanga_podn_R.o3d"), switched("GVM\\Verevka_podn_L.o3d"), switched("GVM\\Antenna.o3d"), folded];
        let meshes: Vec<VehicleMesh> = (0..5).map(mesh).collect();
        let boxes = [
            (Vec3::new(-0.34, -4.53, 3.37), Vec3::new(-0.13, 1.75, 5.36)),
            (Vec3::new(0.14, -4.50, 3.36), Vec3::new(0.36, 1.78, 5.35)),
            (Vec3::new(-0.37, -3.49, 1.42), Vec3::new(-0.23, -3.40, 4.94)),
            (Vec3::new(0.5, 4.0, 3.0), Vec3::new(0.55, 4.3, 3.8)),
            (Vec3::new(-0.68, -4.81, 3.35), Vec3::new(-0.13, 1.73, 3.69)),
        ];
        let program = omsi_script::Program::default();
        let def = legacy_trolley(&model, &meshes, &boxes, &program, Path::new("/nonexistent/b.bus"), true);
        assert_eq!(def.legacy.as_ref().map(|l| l.mesh_defs.clone()), Some(vec![0, 1]));
        take_model_poles(&mut model, &meshes, &boxes, &def);
        for k in 0..2 {
            assert!(model.meshes[k].animations.is_empty() && model.meshes[k].visible_any.is_empty());
        }
        let shown = |md: &omsi_model::MeshDef| md.visible_by(|v| Some(if v == "knopka_massa" { 1.0 } else { 18.0 })).unwrap_or(true);
        assert!(!shown(&model.meshes[2]));
        assert!(shown(&model.meshes[3]));
        assert!(!shown(&model.meshes[4]));
    }
}
