//! Parts coupled behind a vehicle: articulated sections, trailers and rail cars.

use super::*;

impl VehicleInstance {
    /// Couple a vehicle behind this one (`[couple_back]`).
    pub fn attach_trailer(&mut self, ty: Arc<VehicleType>) {
        self.attach_trailer_ex(ty, false);
    }

    /// Uncouple the last part of the train (it is gone from this vehicle).
    pub fn detach_last_trailer(&mut self) -> Option<TrailerPart> {
        self.trailers.pop()
    }

    /// Couple a vehicle behind the last part of the train; `reversed` cars run backwards
    /// (their `[coupling_back]` becomes the front coupling).
    pub fn attach_trailer_ex(&mut self, ty: Arc<VehicleType>, reversed: bool) {
        let first_axle =
            self.physics.wheels.len() + self.trailers.iter().map(|t| t.axle_count).sum::<usize>();
        let (lead_ty, lead_reversed) = match self.trailers.last() {
            Some(t) => (t.ty.clone(), t.reversed),
            None => (self.ty.clone(), false),
        };
        let adopted = self.adopt_trailer_trolley(&ty, self.trailers.len());
        let mut t = TrailerPart::new_ex(
            ty,
            &lead_ty,
            lead_reversed,
            reversed,
            self.ty.program.as_ref(),
            first_axle,
            self.trailers.len(),
        );
        if adopted {
            let program = self.ty.program.clone();
            t.animators = part_animators(&t.ty, &|n| program.var(n).or_else(|| self.var_index.get(&n.to_ascii_lowercase()).copied()));
        }
        self.trailers.push(t);
    }
}

/// A vehicle coupled behind another (articulated bus rear section, trailer, train car).
/// Its meshes animate from the leading vehicle's variables - or, for a train car whose
/// scripts read its place in the train, from its own [`CarRun`](super::CarRun) - and it is
/// towed at the coupling point with trailer kinematics.
/// Sign of `articulation_<n>_alpha` relative to (heading in front − own heading), clockwise
/// degrees: settled with the GN92's joint dummies (see the vehicle tests).
pub(super) const ARTICULATION_SIGN: f64 = 1.0;

pub struct TrailerPart {
    pub ty: Arc<VehicleType>,
    /// The part's own `[smoke]` particle systems (the pusher's rear engine).
    pub particles: ParticleSet,
    /// Its lights' brightness as they come on and go out (see `VehicleInstance::light_fade`).
    pub light_fade: Vec<f32>,
    pub(super) animators: Vec<MeshAnimator>,
    pub mesh_transforms: Vec<Mat4>,
    pub mesh_props: Vec<MeshProps>,
    pub position: DVec3,
    pub heading: f64,
    /// World position of the rear pivot (the trailer axle) from the last frame.
    pub(super) pivot: Option<DVec3>,
    /// Coupling → rear axle distance.
    pub(super) length: f32,
    pub(super) coupling_front: Vec3,
    pub(super) coupling_back: Vec3,
    pub(super) axle_long: f32,
    pub(super) axle_count: usize,
    /// The curvature (1/m, positive to the right) the part's turning line runs on: Omsi.exe's
    /// inverse turning radius of a coupled section, worked out of the part in front's and
    /// the joint angle (see [`coupled_curvature`]), which steers each of its axles (see
    /// `axle_steer`).
    pub(super) kappa: f32,
    /// The part in front's curvature (1/m) and where its turning line lies along it (m, in
    /// its frame), set by the vehicle before each step (`VehicleInstance::turning`).
    pub(super) lead_turn: (f32, f32),
    /// First axle index of this part in the train's `Wheel_*` variables.
    pub(super) first_axle: usize,
    pub(super) wheel_radius: f32,
    /// `articulation_<n>_alpha` (the angle about the vertical axis, which the jackknife
    /// protection watches and the joint's plates and bellows turn with) and
    /// `articulation_<n>_beta` (about the transverse axis) of this part's joint.
    pub(super) v_alpha: Option<omsi_script::VarId>,
    pub(super) v_beta: Option<omsi_script::VarId>,
    pub(super) odometer: f32,
    /// Placed by another game (a LAN player's rear section, `set_remote_pose`): it stays
    /// where that game has it, and is not walked on behind its coupling here.
    pub(super) remote: bool,
    pub reversed: bool,
    /// Per axle of this part: tyre radius less the unloaded hub height, the static load per
    /// wheel and the spring rate - it sits down on its springs like the part it follows.
    pub(super) rest: Vec<(f32, f32, f32)>,
    /// The share of its weight the part hangs on its coupling with (N; see `joint_load`).
    pub(super) joint_load: f32,
    /// `Axle_Brakeforce_<axle>_L/R` of this part's axles, in the leading vehicle's scripts.
    pub(super) v_brakes: Vec<Option<omsi_script::VarId>>,
    /// How far the part's origin stands above the ground under its axles (m).
    pub(super) ground_lift: f32,
    /// The part's pitch (degrees, nose up) and lean (degrees, as the vehicle's `bank`) as
    /// it travels (not turned round for a reversed part), and the eased height of its axle.
    pub(crate) pitch: f32,
    pub(crate) bank: f32,
    pub(super) axle_z: Option<f64>,
    /// How fast the axle's height moves (m/s), for the road part's springs (see `follow`).
    pub(super) axle_vz: f64,
    /// The track under its turning axle, when it runs on rails (`VehicleInstance::retrail`):
    /// it stands at the track's height, not on whatever the ground probe finds there.
    pub(super) track: Option<DVec3>,
    /// Mesh property sources, resolved against the leading vehicle's variables.
    pub(super) props_plan: PropsPlan,
    /// The meshes' transforms in the modelled pose, for `[smoothskin]` (made when needed).
    pub(super) skin_rest: Vec<Mat4>,
    /// What `[smoothskin]` needs of each skinned mesh, worked out once (see [`SkinPlan`]).
    pub(super) skin_plans: SkinPlans,
    /// The part's own `[texttexture]`s, written from the leading vehicle's string
    /// variables (`[scriptshare]`: the rear section of an articulated bus shows the
    /// number and the destination the front's scripts set).
    pub text_textures: Vec<crate::texttex::TextTextureState>,
    /// The leading vehicle's per-axle variables this part writes every frame (see
    /// [`AxleVarIds`]).
    pub(super) axle_ids: AxleVarIds,
    /// Coupled to something ahead and behind, in the train's order (`set_couplings`).
    pub(super) coupled_at: [bool; 2],
    /// Its program or model reads its place in the train, so it gets a [`CarRun`] on its
    /// first frame.
    ///
    /// [`CarRun`]: super::CarRun
    pub(super) runs_itself: bool,
    pub run: Option<Box<super::CarRun>>,
}

/// The leading vehicle's variables a coupled part reads and writes every frame for each of
/// its axles (index 0: its first): looked up once, not by a `format!`ted name per wheel per
/// frame. The program's (`Wheel_Rotation*`, `Axle_Suspension_*`) are what `program.var`
/// answers, `Axle_Springfactor_*` what the vehicle's `var` answers - so they are made again
/// for another program or when the vehicle gains a variable.
#[derive(Default)]
pub(super) struct AxleVarIds {
    /// The program (its address) and the number of the vehicle's variables they were made for.
    made_for: Option<(usize, usize)>,
    /// `Wheel_Rotation_<n>_L/R` and `Wheel_RotationSpeed_<n>_L/R`.
    pub(super) rotation: Vec<[Option<omsi_script::VarId>; 2]>,
    pub(super) rotation_speed: Vec<[Option<omsi_script::VarId>; 2]>,
    /// `Axle_Suspension_<n>_L/R`.
    pub(super) suspension: Vec<[Option<omsi_script::VarId>; 2]>,
    /// `Axle_Steering_<n>_L/R`.
    pub(super) steering: Vec<[Option<omsi_script::VarId>; 2]>,
    /// `Axle_Springfactor_<n>_L`.
    pub(super) spring_factor: Vec<Option<omsi_script::VarId>>,
}

impl AxleVarIds {
    /// Made (again) for `main` unless they are for its program and variables already;
    /// `axles` of the part from train axle `first`.
    fn refresh(&mut self, main: &VehicleInstance, first: usize, axles: usize) {
        let key = (Arc::as_ptr(&main.ty.program) as usize, main.var_index.len());
        if self.made_for == Some(key) {
            return;
        }
        let p = &main.ty.program;
        let sides = |f: &dyn Fn(&str) -> Option<omsi_script::VarId>| ["L", "R"].map(|side| f(side));
        let n = first..first + axles;
        self.rotation = n.clone().map(|k| sides(&|side| p.var(&format!("Wheel_Rotation_{k}_{side}")))).collect();
        self.rotation_speed = n.clone().map(|k| sides(&|side| p.var(&format!("Wheel_RotationSpeed_{k}_{side}")))).collect();
        self.suspension = n.clone().map(|k| sides(&|side| p.var(&format!("Axle_Suspension_{k}_{side}")))).collect();
        self.steering = n.clone().map(|k| sides(&|side| p.var(&format!("Axle_Steering_{k}_{side}")))).collect();
        self.spring_factor = n.map(|k| main.var_slot(&format!("Axle_Springfactor_{k}_L")).map(|i| i as omsi_script::VarId)).collect();
        self.made_for = Some(key);
    }
}

/// The curvature (1/m, positive to the right) a coupled part's turning line runs on, as
/// Omsi.exe 0x7e0f44-0x7e0fc0 works out the section's inverse turning radius: the joint
/// moves at atan(`lead_arm` x `k_lead`) to the part in front's heading (`lead_arm`: how far
/// the joint lies ahead of that part's turning line), so at that plus the joint angle
/// `alpha` (rad, the part in front turned to the right of this one) to this part's, and
/// the tangent of that over `arm` (how far the joint lies ahead of this part's turning line)
/// is the curvature that has it move so. A joint on the turning line turns nothing.
pub(super) fn coupled_curvature(k_lead: f32, lead_arm: f32, alpha: f32, arm: f32) -> f32 {
    if arm.abs() < 0.1 || !(k_lead.is_finite() && alpha.is_finite()) {
        return 0.0;
    }
    let at_joint = ((lead_arm * k_lead).atan() + alpha).clamp(-1.4, 1.4);
    (at_joint.tan() / arm).clamp(-1.0, 1.0)
}

/// The weight (N) a road part on a single axle hangs on the coupling in front of it, the
/// share of its centre of gravity ahead of the axle; the axle carries the rest. Omsi.exe
/// joins the sections of an articulated bus with a ball joint between bodies of their own,
/// so the GN92's rear section (7.2 t, its axle 4.6 m behind the joint and 0.4 m behind its
/// centre of gravity) lays 0.6 t on the front section's rear axle. Held up by its axle alone
/// it pressed its springs a fifth deeper than the front section's and its end sagged (#1835).
/// None for a part with more axles, which stands on them.
pub(super) fn joint_load(def: &omsi_vehicle::Vehicle, coupling_y: f32) -> f32 {
    let [axle] = def.axles.as_slice() else { return 0.0 };
    if def.is_rail() || coupling_y <= axle.long + 0.5 {
        return 0.0;
    }
    let weight = if def.mass < 100.0 { def.mass * 1000.0 } else { def.mass }.max(0.0) * 9.81;
    let cog_y = def.cog.map(|c| c[1]).unwrap_or(0.0);
    (weight * (cog_y - axle.long) / (coupling_y - axle.long)).clamp(0.0, weight * 0.5)
}

/// The mesh animators of a part, their variables found by `var`.
fn part_animators(ty: &VehicleType, var: &dyn Fn(&str) -> Option<omsi_script::VarId>) -> Vec<MeshAnimator> {
    let mut animators: Vec<MeshAnimator> = ty
        .meshes
        .iter()
        .map(|m| MeshAnimator::new(&ty.model.meshes[m.def_index], m.pivot, var))
        .collect();
    crate::anim::link_parents(
        &mut animators,
        &ty.meshes
            .iter()
            .map(|m| &ty.model.meshes[m.def_index])
            .collect::<Vec<_>>(),
    );
    animators
}

impl TrailerPart {
    /// Pitch (degrees, nose up), eased axle height and the track point it stands on (for
    /// the `OMSI_DEBUG_TRAILERS` trace).
    pub fn debug_pose(&self) -> (f32, Option<f64>, Option<DVec3>) {
        (self.pitch, self.axle_z, self.track)
    }

    /// How far the part's origin stands above the plane its wheels touch (m): the road is
    /// at z = -this in its own frame (where its shadow blob lies, and the tyre spray starts).
    pub fn ground_lift(&self) -> f32 {
        self.ground_lift
    }

    pub fn new(
        ty: Arc<VehicleType>,
        main: &VehicleType,
        program: &Program,
        first_axle: usize,
    ) -> TrailerPart {
        Self::new_ex(ty, main, false, false, program, first_axle, 0)
    }

    /// `joint`: which joint of the train this part hangs on (0 behind the leading vehicle).
    pub fn new_ex(
        ty: Arc<VehicleType>,
        main: &VehicleType,
        main_reversed: bool,
        reversed: bool,
        program: &Program,
        first_axle: usize,
        joint: usize,
    ) -> TrailerPart {
        let animators = part_animators(&ty, &|n| program.var(n));
        let (coupling_back, coupling_front) = coupling_points(main, main_reversed, &ty, reversed);
        // the line the part turns about: its own `[rot_pnt_long]` where a road part names
        // one (Omsi.exe runs every section as a body of its own on the same wheel physics,
        // each axle steered towards the turning centre on that line), else the axle
        // farthest from the coupled end. A rear section whose axle steers (the Van Hool
        // AG300's, set ahead of its axle) followed it as if it were a fixed one (#322);
        // the stock GN92's line is its axle, a semitrailer's the middle of its axle group,
        // and rail cars name none.
        let turning_line = (ty.def.rot_pnt_long != 0.0 && !ty.def.axles.is_empty()).then_some(ty.def.rot_pnt_long);
        let axle_long = if let Some(r) = turning_line {
            r
        } else if let Some(b) = ty.def.boogies {
            // a rail car's rear bogie: `[boogies]` is how far each bogie stands from the
            // origin (see `rail_ends`), at the model's back - its front where it is turned
            // round. (Half as far out, and kept from going past the coupling of the car in
            // front instead of its own, the bogie of a car turned round lay ahead of its
            // front coupling.)
            if body_reversed(&ty.def, reversed) {
                b.abs()
            } else {
                -b.abs()
            }
        } else if ty.def.is_rail() {
            // Rail vehicle without [boogies] (e.g. hanging bridge section): pivot is at
            // its own rear coupling (`coupling_back` is the one of the part in front)
            declared_coupling(&ty.def, reversed).y
        } else if body_reversed(&ty.def, reversed) {
            let a = ty.def.axles.iter().map(|a| a.long).fold(f32::MIN, f32::max);
            if a == f32::MIN {
                0.5
            } else {
                a
            }
        } else {
            let a = ty.def.axles.iter().map(|a| a.long).fold(f32::MAX, f32::min);
            if a == f32::MAX {
                -0.5
            } else {
                a
            }
        };
        let length = if ty.def.is_rail() {
            let len = (coupling_front.y - axle_long).abs();
            if len < 0.1 {
                (coupling_front.y - declared_coupling(&ty.def, reversed).y).abs().max(0.5)
            } else {
                len
            }
        } else {
            (coupling_front.y - axle_long).abs()
        };
        let wheel_radius = ty
            .def
            .axles
            .first()
            .map(|a| (a.wheel_diameter / 2.0).max(0.1))
            .unwrap_or(0.5);
        let n = ty.meshes.len();
        let mut loads = crate::rigid::wheel_rest_loads(&ty.def);
        let joint_load = if joint == 0 && !reversed { joint_load(&ty.def, coupling_front.y) } else { 0.0 };
        if let Some(l) = loads.first_mut().filter(|_| joint_load > 0.0) {
            *l -= joint_load / 2.0;
        }
        let rest = ty
            .def
            .axles
            .iter()
            .zip(ty.hub_heights(first_axle))
            .enumerate()
            .map(|(a, (axle, z))| {
                let r = (axle.wheel_diameter / 2.0).max(0.15);
                let k = if axle.spring > 0.0 {
                    axle.spring * 1000.0
                } else {
                    150_000.0
                };
                (r - z.unwrap_or(r), loads.get(a).copied().unwrap_or(0.0), k)
            })
            .collect();
        let v_brakes = (0..ty.def.axles.len())
            .flat_map(|a| {
                ["L", "R"]
                    .map(|side| program.var(&format!("Axle_Brakeforce_{}_{side}", first_axle + a)))
            })
            .collect();
        TrailerPart {
            particles: ParticleSet::new(ty.model.particle_systems(), first_axle as u64 * 7919 + 17),
            light_fade: Vec::new(),
            rest,
            joint_load,
            v_brakes,
            ground_lift: 0.0,
            pitch: 0.0,
            bank: 0.0,
            axle_z: None,
            axle_vz: 0.0,
            track: None,
            v_alpha: program.var(&format!("articulation_{joint}_alpha")),
            v_beta: program.var(&format!("articulation_{joint}_beta")),
            animators,
            mesh_transforms: vec![Mat4::IDENTITY; n],
            mesh_props: vec![MeshProps::default(); n],
            props_plan: PropsPlan::default(),
            position: DVec3::ZERO,
            heading: 0.0,
            pivot: None,
            length,
            coupling_front,
            coupling_back,
            axle_long,
            axle_count: ty.def.axles.len().max(1),
            kappa: 0.0,
            lead_turn: (0.0, 0.0),
            first_axle,
            wheel_radius,
            odometer: 0.0,
            remote: false,
            reversed,
            text_textures: Vec::new(),
            axle_ids: AxleVarIds::default(),
            skin_rest: Vec::new(),
            skin_plans: SkinPlans::default(),
            coupled_at: [true, false],
            runs_itself: super::car_run::uses_place_in_train(&ty),
            run: None,
            ty,
        }
    }

    /// `train_frontcoupling`, `train_backcoupling`, `train_me_reverse` of this part: a
    /// part turned round has the train's rear at its own front.
    pub fn place_in_train(&self) -> [f32; 3] {
        let [ahead, behind] = self.coupled_at;
        let own_front = if self.reversed { behind } else { ahead };
        let own_back = if self.reversed { ahead } else { behind };
        [own_front, own_back, self.reversed].map(|b| if b { 1.0 } else { 0.0 })
    }

    /// Variable `name` of the part's own [`CarRun`](super::CarRun); None when it has none
    /// (it shows the leading vehicle's).
    pub fn run_var(&self, name: &str) -> Option<f32> {
        self.run.as_deref()?.get(name)
    }

    /// The `[smoothskin]` meshes of this part.
    pub fn skinned_meshes(&self) -> Vec<usize> {
        (0..self.ty.meshes.len())
            .filter(|&i| !self.ty.meshes[i].skin.is_empty())
            .collect()
    }

    /// See [`VehicleInstance::skin_key`].
    pub fn skin_key(&self, i: usize) -> Vec<Mat4> {
        skin_key(&self.ty, i, &self.mesh_transforms)
    }

    /// Skinned mesh `i` as its bones hold it now; `n_vars`: the leading vehicle's variable
    /// count (this part animates from its variables).
    pub fn skinned(&mut self, i: usize, n_vars: usize) -> Option<(Vec<Vec3>, Vec<Vec3>)> {
        let n_vars = self.run.as_ref().map_or(n_vars, |r| r.state.vars.len());
        if self.skin_rest.len() != self.animators.len() {
            self.skin_rest = rest_transforms(&self.animators, n_vars);
        }
        let plan = self.skin_plans.get(&self.ty, i)?;
        skin_vertices(&self.ty, i, &self.mesh_transforms, &self.skin_rest, plan)
    }

    /// Prepare the part's text textures with fonts from `lib`.
    pub fn init_text_textures(
        &mut self,
        lib: &mut crate::texttex::FontLibrary,
        decode: &dyn Fn(&Path) -> Option<(u32, u32, Vec<u8>)>,
    ) {
        self.text_textures = self
            .ty
            .model
            .text_textures
            .iter()
            .map(|t| crate::texttex::TextTextureState::new(t.clone(), lib.get(&t.font, decode)))
            .collect();
    }

    /// Re-render the text textures whose variable (of the leading vehicle) changed;
    /// returns the indices with a pending image.
    pub fn update_text_textures(&mut self, main: &VehicleInstance) -> Vec<usize> {
        let mut changed = Vec::new();
        for (i, t) in self.text_textures.iter_mut().enumerate() {
            if t.update(main.text_texture_str(&t.def.variable)) {
                changed.push(i);
            }
        }
        changed
    }

    /// A trailer follows its tractor's heading; it pitches between the coupling and the
    /// ground under its axle, and leans as the vehicle does.
    pub fn body_rotation(&self) -> Mat4 {
        body_rotation(self.heading, self.pitch, self.bank, body_reversed(&self.ty.def, self.reversed))
    }

    pub fn body_heading(&self) -> f64 {
        body_heading(&self.ty.def, self.heading, self.reversed)
    }

    /// World transform of the part's body (f32, for sound positions).
    pub fn world_transform(&self) -> Mat4 {
        Mat4::from_translation(self.position.as_vec3()) * self.body_rotation()
    }

    /// One of the part's own `.bus` cameras fixed to its body: (eye, yaw, pitch, roll), as
    /// `VehicleInstance::camera_world_full` for the front part (`dist` and the body's pitch
    /// and bank included).
    pub fn camera_world_full(&self, cam: &omsi_vehicle::Camera) -> (DVec3, f32, f32, f32) {
        camera_in_body(self.position, self.body_rotation(), cam)
    }

    /// Transform for mesh `i` relative to the part's position; a shadow blob lies on the
    /// ground under its axles (see `VehicleInstance::mesh_local_transform`).
    pub fn mesh_local_transform(&self, i: usize) -> Mat4 {
        let xf = self.mesh_transforms.get(i).copied().unwrap_or(Mat4::IDENTITY);
        if is_shadow_mesh(&self.ty, i) {
            let lift = if self.ty.def.is_rail() { 0.0 } else { -self.ground_lift };
            return self.body_rotation()
                * onto_plane([lift, 0.0, 0.0])
                * xf;
        }
        self.body_rotation() * xf
    }

    /// Where this part hangs on the one in front: the coupling point in the frame of the
    /// part in front, and its own coupling point in its own frame (the two meet in the
    /// world). With the parts straight behind each other, a point `p` of this part lies at
    /// `p + back - front` in the frame of the part in front.
    pub fn couplings(&self) -> (Vec3, Vec3) {
        (self.coupling_back, self.coupling_front)
    }

    /// Put this part where another source says it is (a LAN player's rear section): the
    /// pivot is set so that the next follow step keeps this heading.
    pub fn set_pose(&mut self, position: DVec3, heading: f64) {
        self.heading = heading;
        let rot = self.body_rotation();
        let c = position + rot.transform_point3(self.coupling_front).as_dvec3();
        let h = heading.to_radians();
        self.pivot = Some(c - DVec3::new(h.sin(), h.cos(), 0.0) * self.length as f64);
        self.position = position;
        // (its wheels straight again: the bend it was in is no longer known)
        self.kappa = 0.0;
    }

    /// `set_pose` for a part another game drives (LAN), which sends where the part is and
    /// its heading but not how it leans: tilted so that its front coupling meets the leading
    /// part's at `coupling` (`coupling_point`). Drawn level, an articulated bus's rear
    /// section climbing a slope sank into the road in the other players' games (#1702).
    pub fn set_remote_pose(&mut self, position: DVec3, heading: f64, coupling: DVec3) {
        let ahead = self.coupling_front.y.abs().max(0.5) as f64;
        let rise = coupling.z - (position.z + self.coupling_front.z as f64);
        self.pitch = (rise.atan2(ahead).to_degrees() as f32).clamp(-20.0, 20.0);
        self.set_pose(position, heading);
        self.remote = true;
    }

    /// Forget where this part was: the next step puts it straight behind the leading part
    /// (after the vehicle was moved somewhere else).
    pub fn realign(&mut self) {
        self.pivot = None;
        self.axle_z = None;
        self.track = None;
        self.kappa = 0.0;
    }

    /// Where this part couples to the part in front of it, in the world, with the leading
    /// part as it stands now (`lead`: position, rotation of the part in front; the vehicle
    /// itself for the first trailer).
    pub fn coupling_point(&self, lead_pos: DVec3, lead_rot: Mat4) -> DVec3 {
        lead_pos + lead_rot.transform_point3(self.coupling_back).as_dvec3()
    }

    /// The distance from the coupling to the axle the part turns about.
    pub fn pivot_length(&self) -> f32 {
        self.length
    }

    /// Put the part's turning axle at `pivot` (a point on the road behind the coupling):
    /// a bus placed on a bend stands with its rear section following the bend rather than
    /// straight behind it.
    pub fn place_pivot(&mut self, pivot: DVec3) {
        self.pivot = Some(pivot);
    }

    /// Put the part's turning axle on a track at `p` (a rail vehicle's coupled car or
    /// section): it also takes the track's height there.
    pub fn place_on_track(&mut self, p: DVec3) {
        self.pivot = Some(p);
        self.track = Some(p);
    }

    /// Its springs: an AI copy sits `[ai_deltaheight]` low like its tractor; a driven one
    /// as far down as the scripts' `Axle_Springfactor` lets the load press it. Writes each
    /// axle's `Axle_Suspension_*` and returns how far the part's origin stands above the
    /// ground under its axles (m).
    fn rest_springs(&self, main: &mut VehicleInstance) -> f64 {
        let ai = main.ev(Ev::Ai).map(|v| v > 0.5).unwrap_or(false);
        let shows = !self.ty.suspension_axles.is_empty();
        let mut sag = Vec::with_capacity(self.rest.len());
        for (a, (offset, load, k)) in self.rest.iter().enumerate() {
            let axle = self.first_axle + a;
            let compression = if ai {
                if shows {
                    -self.ty.def.ai_delta_height
                } else {
                    0.0
                }
            } else if self.ty.suspension_axles.contains(&axle) {
                let factor = self.axle_ids.spring_factor[a]
                    .map(|i| main.state.vars[i as usize])
                    .unwrap_or(1.0)
                    .max(0.05);
                (load / (k * factor)).min(crate::rigid::BUMP)
            } else {
                0.0
            };
            for id in self.axle_ids.suspension[a].into_iter().flatten() {
                main.state.vars[id as usize] = -compression;
            }
            sag.push(offset - compression);
        }
        sag.iter().sum::<f32>() as f64 / sag.len().max(1) as f64
    }

    /// Where the part runs on the track, in the frame of its travel (m forward of its
    /// origin): its two bogies (`[boogies]` is how far each stands from the origin - the
    /// A3's axles at ±2.835 and ±4.735 m are its 3.785), else its first and last axle, else
    /// its front coupling and the point it was towed by behind it.
    pub(super) fn rail_ends(&self) -> (f32, f32) {
        if let Some(b) = self.ty.def.boogies.map(f32::abs).filter(|b| *b > 0.05) {
            return (b, -b);
        }
        let turn = if body_reversed(&self.ty.def, self.reversed) { -1.0 } else { 1.0 };
        let longs = self.ty.def.axles.iter().map(|a| a.long * turn);
        let (lo, hi) = longs.fold((f32::MAX, f32::MIN), |(lo, hi), y| (lo.min(y), hi.max(y)));
        if hi - lo > 0.5 {
            return (hi, lo);
        }
        // a bogie frame that names no coupling of its own (see `declared_coupling`) stands on the
        // track by its own middle
        if is_bogie_frame(&self.ty.def) {
            // (its own wheelbase: the model's length, not the distance between its couplings)
            let half = self.ty.model_box().map(|(lo, hi)| (hi.y - lo.y) * 0.5).unwrap_or(0.9).clamp(0.5, 3.0);
            return (half, -half);
        }
        let front = self.coupling_front.y * turn;
        (front, front - self.length.max(0.5))
    }

    /// Stand the part on the track (`VehicleInstance::retrail`): `front` and `back` are the
    /// track's points under its running ends (`rail_ends`, `y_front` and `y_back` metres
    /// forward of its origin). It takes the track's heading and grade between them, at
    /// the track's height, as the vehicle it is coupled to does (`AiBody::ride`).
    pub(super) fn place_on_rails(
        &mut self,
        main: &mut VehicleInstance,
        dt: f32,
        lead: Option<(DVec3, Mat4, f64)>,
        (front, y_front): (DVec3, f32),
        (back, y_back): (DVec3, f32),
    ) {
        self.axle_ids.refresh(main, self.first_axle, self.axle_count.max(self.rest.len()));
        let d = front - back;
        let run = d.truncate().length();
        if run > 1e-3 {
            self.heading = d.x.atan2(d.y).to_degrees().rem_euclid(360.0);
            self.pitch = ((d.z / run).atan().to_degrees() as f32).clamp(-30.0, 30.0);
        }
        let span = (y_front - y_back).max(0.1) as f64;
        self.position = back + d * ((0.0 - y_back) as f64 / span);
        self.bank = main.bank;
        // (its wheels roll as far as its rear end moved)
        if let Some(p) = self.pivot {
            let ds = (back - p).truncate().length() as f32;
            self.odometer += ds * (main.physics.velocity_kmh().signum().max(0.0) * 2.0 - 1.0).max(-1.0);
        }
        self.pivot = Some(back);
        self.track = Some(back);
        self.axle_z = Some(back.z);
        self.axle_vz = 0.0;
        self.ground_lift = self.rest_springs(main) as f32;
        self.update_visuals_only(main, dt, lead);
    }

    pub(super) fn update(&mut self, main: &mut VehicleInstance, dt: f32, lead: Option<(DVec3, Mat4, f64)>) {
        self.axle_ids.refresh(main, self.first_axle, self.axle_count.max(self.rest.len()));
        if self.remote {
            // Another game's rear section stands where that game says, leaning onto the
            // coupling (`set_remote_pose`). Walked on behind the coupling here as well, it
            // took the ground under it in this game and lost the lean, and the gap between
            // its pose and our copy of the front moved its axle every frame: its wheels
            // turned on a bus that stood still (#1177). Its wheels roll with the bus.
            self.odometer += main.physics.speed * dt;
            self.update_visuals_only(main, dt, lead);
            return;
        }
        // coupling point in the world: on the leading part (the vehicle or the previous trailer)
        let (lead_pos, lead_rot, lead_heading) =
            lead.unwrap_or((main.position, main.body_rotation(), main.heading));
        let c = lead_pos + lead_rot.transform_point3(self.coupling_back).as_dvec3();
        // (a pivot at NaN never came back: every step after took its way from it, and the
        // rear section stayed at NaN for good while the front drove on - it starts again)
        if self.pivot.is_some_and(|p| !p.is_finite()) || self.axle_z.is_some_and(|z| !z.is_finite()) || !self.axle_vz.is_finite() {
            self.pivot = None;
            self.axle_z = None;
            self.axle_vz = 0.0;
        }
        // the height its axle had (the new one eases from it; nothing after a move)
        let prev_z = self.pivot.and(self.axle_z);
        let pivot = match self.pivot {
            Some(p) => p,
            None => {
                // start straight behind the leading vehicle
                let h = main.heading.to_radians();
                c - DVec3::new(h.sin(), h.cos(), 0.0) * self.length as f64
            }
        };
        let mut dir = (c - pivot).truncate();
        if dir.length() < 1e-3 {
            let h = main.heading.to_radians();
            dir = glam::DVec2::new(h.sin(), h.cos());
        }
        let mut dir = dir.normalize();
        let mut heading = dir.x.atan2(dir.y).to_degrees();
        // `[coupling_front_character]`: a bus joint (type != 0) stops hard at its max
        // alpha (Omsi.exe 0x7e0848); the rear section's axle is dragged sideways there
        // instead of jackknifing through the part in front.
        if let Some([amax, _, _, kind]) = self.ty.def.coupling_front_character {
            if kind != 0.0 && amax > 0.0 {
                let a = amax as f64;
                let rel = ((lead_heading - heading + 540.0) % 360.0) - 180.0;
                if rel.abs() > a {
                    heading = lead_heading - rel.clamp(-a, a);
                    let h = heading.to_radians();
                    dir = glam::DVec2::new(h.sin(), h.cos());
                }
            }
        }
        self.heading = heading;
        // the bend the part's turning line runs on, from the part in front's and the joint
        // angle as they stand (no difference between frames, which a creeping bus or the
        // joint at its stop made jump)
        if !self.ty.def.is_rail() && !self.reversed {
            let (k_lead, line_lead) = self.lead_turn;
            let alpha = (((lead_heading - heading + 540.0).rem_euclid(360.0) - 180.0) as f32).to_radians();
            self.kappa = coupled_curvature(k_lead, self.coupling_back.y - line_lead, alpha, self.coupling_front.y - self.axle_long);
        }
        let new_pivot = c - DVec3::new(dir.x, dir.y, 0.0) * self.length as f64;
        let ds = (new_pivot - pivot).truncate().length() as f32;
        self.odometer += ds * (main.physics.velocity_kmh().signum().max(0.0) * 2.0 - 1.0).max(-1.0);
        self.pivot = Some(new_pivot);
        let ai = main.ev(Ev::Ai).map(|v| v > 0.5).unwrap_or(false);
        let shows = !self.ty.suspension_axles.is_empty();
        let lift = self.rest_springs(main);
        self.ground_lift = lift as f32;
        // On rails: the track's height where it was put on it (self.track is set by
        // retrail; we don't reject it with a distance threshold on curves).
        let on_track = self.track.map(|t| t.z);
        // the height of the part's origin over its axle (where the ground has none: level
        // with the coupling, as before)
        let level = c.z - self.coupling_front.z as f64;
        // the ground under its axle: what the wheels stand on where the world says, else the
        // plain height sampler
        let ground_z = match (on_track, &main.contact, &main.ground) {
            (Some(_), _, _) => None,
            (None, Some(g), _) => {
                // Looked for from above the coupling's level as well as from the part's own
                // height: from its own height alone, a rear section that had once dropped
                // under a viaduct's deck (a frame's step at the ramp, a gap at a joint) only
                // ever found the ground beneath and hung there under the bridge while the
                // front section drove on above (#135).
                let top = self.position.z.max(level) + 1.5;
                g.probe(new_pivot.x, new_pivot.y, top).below
            }
            (None, None, Some(g)) => g(new_pivot.x, new_pivot.y),
            _ => None,
        };
        // A height far from where the coupling holds the part is another level's: the AI's
        // ground lookup knows only x and y and gives the highest road there, which under a
        // bridge is the deck (or, on the deck, a road that runs on beneath it) - the trailer
        // of a lorry and the rear of an articulated bus stood up on the bridge or down under
        // it (#140). Level with the coupling instead. With the world's faces the part may
        // stand lower than the coupling on a grade, but never metres under it: that is the
        // road under a bridge seen through a gap in the deck (#135).
        let ground_z = ground_z.filter(|z| if main.contact.is_some() { z + lift - level > -3.0 } else { (z + lift - level).abs() < 1.5 });
        let axle_z = if let Some(z) = on_track {
            // Rail vehicles (vehtype == 3) bypass terrain raycasts and suspension height relaxation
            // entirely: vertical coordinate z is locked rigidly to the rail track spline geometry.
            z
        } else {
            match ground_z.map(|z| z + lift) {
                Some(z) if main.contact.is_some() && dt > 0.0 => {
                    // On the road the part stands on its springs as the part in front does in
                    // Omsi.exe (each section is a body of its own on the same wheel springs): a
                    // bump under its axle is a jolt that swings out, not a height eased into over
                    // a sixth of a second, which smoothed every bump away under the rear of an
                    // articulated bus. (Sprung at about 1.6 Hz, a little damped, as a bus body.)
                    let from = prev_z.unwrap_or(z);
                    if (z - from).abs() > 0.5 {
                        self.axle_vz = 0.0;
                        z
                    } else {
                        let (w, zeta) = (2.0 * std::f64::consts::PI * 1.6, 0.35);
                        let h = (dt as f64).min(0.05);
                        let acc = w * w * (z - from) - 2.0 * zeta * w * self.axle_vz;
                        self.axle_vz = (self.axle_vz + acc * h).clamp(-3.0, 3.0);
                        from + self.axle_vz * h
                    }
                }
                Some(z) => {
                    let from = prev_z.unwrap_or(z);
                    let dz = z - from;
                    if dz.abs() > 0.5 {
                        z
                    } else {
                        from + dz * (dt as f64 * 6.0).min(1.0)
                    }
                }
                None => level,
            }
        };
        self.axle_z = Some(axle_z);
        // The part hangs at the coupling in front and stands on its axle behind: its pitch is
        // the slope between them.
        let dz = c.z - (axle_z + self.coupling_front.z as f64);
        let run = if let Some(track_p) = self.track {
            (c.truncate() - track_p.truncate()).length().max(0.5)
        } else {
            self.length.max(0.5) as f64
        };
        let mut pitch = (dz.atan2(run).to_degrees() as f32).clamp(-30.0, 30.0);
        if let Some([_, bmax, _, kind]) = self.ty.def.coupling_front_character {
            if kind != 0.0 && bmax > 0.0 {
                let lead_pitch = {
                    let f = lead_rot.transform_vector3(Vec3::Y);
                    let h = lead_heading.to_radians();
                    let along = f.x as f64 * h.sin() + f.y as f64 * h.cos();
                    (f.z as f64).atan2(along.abs().max(1e-6) * along.signum()).to_degrees() as f32
                };
                let lead_pitch = if lead_pitch.abs() > 90.0 { lead_pitch - 180.0 * lead_pitch.signum() } else { lead_pitch };
                let rel = lead_pitch - pitch;
                if rel.abs() > bmax {
                    pitch = lead_pitch - rel.clamp(-bmax, bmax);
                }
            }
        }
        self.pitch = pitch;
        self.bank = main.bank;
        // the trailer origin: coupling_front sits at c
        let rot = self.body_rotation();
        self.position = c - rot.transform_point3(self.coupling_front).as_dvec3();
        // The wheels stand on the road under them, wherever the body above swings: the
        // travel of each wheel is the gap between its hub on the body and the ground under
        // it (Omsi.exe runs the section as a body on its own springs, each wheel's travel
        // its own). Held at the static sag, the rear axle of an articulated bus was a rigid
        // one - its wheels bounced and leant with the body over every bump and in every
        // bend (#901).
        if !ai && on_track.is_none() && dt > 0.0 && shows {
            self.spring_wheels(main, rot);
        }
        if omsi_cfg::flags::OMSI_DEBUG_TRAILER.is_set() {
            log::info!(
                "trailer: rest {:?} lift {lift:.3} ground {ground_z:?} z {:.3}",
                self.rest,
                self.position.z
            );
            for (i, m) in self.ty.meshes.iter().enumerate() {
                let file = &self.ty.model.meshes[m.def_index].file;
                if file.to_ascii_lowercase().contains("rad") {
                    log::info!(
                        "  {file}: pivot w {:?} transform w {:?}",
                        m.pivot.w_axis.truncate(),
                        self.mesh_transforms[i].w_axis.truncate()
                    );
                }
            }
        }
        self.update_visuals_only(main, dt, lead);
    }

    pub(super) fn update_visuals_only(&mut self, main: &mut VehicleInstance, dt: f32, lead: Option<(DVec3, Mat4, f64)>) {
        let (_lead_pos, lead_rot, lead_heading) =
            lead.unwrap_or((main.position, main.body_rotation(), main.heading));
        let lead_pitch = {
            let f = lead_rot.transform_vector3(Vec3::Y);
            let h = lead_heading.to_radians();
            let along = f.x as f64 * h.sin() + f.y as f64 * h.cos();
            (f.z as f64).atan2(along.abs().max(1e-6) * along.signum()).to_degrees()
        };
        let lead_pitch = if lead_pitch.abs() > 90.0 { lead_pitch - 180.0 * lead_pitch.signum() } else { lead_pitch };
        let alpha = ((lead_heading - self.heading + 540.0) % 360.0) - 180.0;
        let beta = lead_pitch - self.pitch as f64;
        if let Some(id) = self.v_alpha {
            main.state.vars[id as usize] = (alpha * ARTICULATION_SIGN) as f32;
        }
        if let Some(id) = self.v_beta {
            main.state.vars[id as usize] = beta as f32;
        }
        let rpm =
            main.physics.velocity_kmh() / 3.6 / (2.0 * std::f32::consts::PI * self.wheel_radius)
                * 60.0;
        let rot = (self.odometer / self.wheel_radius).rem_euclid(std::f32::consts::TAU);
        for a in 0..self.axle_count {
            let steer = self.axle_steer(a);
            for side in 0..2 {
                if let (Some(Some(id)), Some(s)) = (self.axle_ids.steering.get(a).map(|s| s[side]), steer) {
                    main.state.vars[id as usize] = s;
                }
                if let Some(id) = self.axle_ids.rotation[a][side] {
                    main.state.vars[id as usize] = rot;
                }
                if let Some(id) = self.axle_ids.rotation_speed[a][side] {
                    main.state.vars[id as usize] = rpm;
                }
            }
        }
        // a car that reads its place in the train runs its own scripts first, and its
        // meshes show its values (see `car_run`)
        let place = self.place_in_train();
        let (ahead, behind) = (&self.ty.def.control_cable_front, &self.ty.def.control_cable_back);
        let plugs = if self.reversed { [behind.as_slice(), ahead.as_slice()] } else { [ahead.as_slice(), behind.as_slice()] };
        let probe = main.height_probe_at(self.position, self.body_rotation());
        match self.run.as_deref_mut() {
            Some(run) => run.frame(main, place, plugs, probe),
            None if self.runs_itself => {
                let run = super::CarRun::start(&self.ty, main, place);
                let slot_of = |n: &str| omsi_script::compile::with_lower(n, |k| run.names.get(k).copied());
                self.animators = part_animators(&self.ty, &slot_of);
                self.skin_rest = Vec::new();
                self.props_plan = PropsPlan::default();
                self.run = Some(Box::new(run));
            }
            None => {}
        }
        let (vars, index) = self.run.as_deref().map_or((&main.state.vars, &main.var_index), |r| (&r.state.vars, &r.names));
        for (transform, animator) in self.mesh_transforms.iter_mut().zip(self.animators.iter_mut()) {
            *transform = animator.update(dt, vars);
        }
        crate::anim::apply_parents(&self.animators, &mut self.mesh_transforms);
        self.props_plan.refresh(&self.ty, index);
        self.props_plan.apply(vars, &mut self.mesh_props);
    }
}

impl TrailerPart {
    /// Road axle `a`'s steering angle (rad, positive to the right), what Omsi.exe hands the
    /// scripts as the part's `Axle_Steering_<n>_L/R`: atan((axle long - turning line) x
    /// curvature), every axle pointed at the centre of the bend on the part's
    /// `[rot_pnt_long]` line (0x7e303d, as the front section's at 0x7df3ac). An axle on that
    /// line (the GN92's) stays straight; the Van Hool AG300's, behind it, turns against the
    /// bend. (No coupled part's axle had the variable set at all, #322.) Rail cars and parts
    /// running backwards steer none.
    pub(super) fn axle_steer(&self, a: usize) -> Option<f32> {
        if self.ty.def.is_rail() || self.reversed {
            return None;
        }
        let axle = self.ty.def.axles.get(a)?;
        Some(((axle.long - self.axle_long) * self.kappa).atan().clamp(-1.05, 1.05))
    }

    /// `Axle_Suspension_*` of the part's sprung axles from the ground under each wheel, the
    /// body standing at `self.position` turned by `rot` (see `update`).
    pub(super) fn spring_wheels(&self, main: &mut VehicleInstance, rot: Mat4) {
        let probe = |x: f64, y: f64, top: f64| -> Option<f64> {
            match (&main.contact, &main.ground) {
                (Some(g), _) => g.probe(x, y, top).below,
                (None, Some(g)) => g(x, y),
                _ => None,
            }
        };
        let mut travel: Vec<(usize, [Option<f32>; 2])> = Vec::new();
        for (a, (offset, _, _)) in self.rest.iter().enumerate() {
            let axle = self.first_axle + a;
            if !self.ty.suspension_axles.contains(&axle) {
                continue;
            }
            let Some(def) = self.ty.def.axles.get(a) else { continue };
            let r = (def.wheel_diameter / 2.0).max(0.15);
            let hub = r - offset;
            let outer = (def.max_width / 2.0).max(0.3);
            let across = if def.min_width > 0.0 && def.min_width < def.max_width { (def.max_width + def.min_width) / 4.0 } else { outer * 0.85 };
            let mut sides = [None, None];
            for (si, x) in [-across, across].into_iter().enumerate() {
                let p = self.position + rot.transform_point3(Vec3::new(x, def.long, hub)).as_dvec3();
                let Some(g) = probe(p.x, p.y, p.z + 1.0) else { continue };
                // how far the wheel is pushed up into its arch (never below where it hangs
                // unloaded, never past the bump stop)
                sides[si] = Some(((g + r as f64 - p.z) as f32).clamp(0.0, crate::rigid::BUMP));
            }
            travel.push((a, sides));
        }
        for (a, sides) in travel {
            for (id, c) in self.axle_ids.suspension[a].into_iter().zip(sides) {
                if let (Some(c), Some(id)) = (c, id) {
                    main.state.vars[id as usize] = -c;
                }
            }
        }
    }
}
