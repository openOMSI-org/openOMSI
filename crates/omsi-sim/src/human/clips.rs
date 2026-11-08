use super::*;
use glam::Mat3;
use omsi_o3d::fbx::Clip;

const PELVIS: usize = 0;
const SPINE: usize = 1;
const UPPER_B: [usize; 2] = [2, 3];
const FORE_B: [usize; 2] = [4, 5];
const HAND_B: [usize; 2] = [6, 7];
const NAMES: [&str; 8] = ["pelvis", "spine2", "l upperarm", "r upperarm", "l forearm", "r forearm", "l hand", "r hand"];

fn key(name: &str) -> String {
    let n = name.trim().to_ascii_lowercase().replace("bip001", "bip01");
    n.strip_prefix("bip01").map(|r| r.trim().to_string()).unwrap_or(n)
}

fn twist(q: Quat, axis: Vec3) -> Quat {
    let v = Vec3::new(q.x, q.y, q.z);
    let p = axis * v.dot(axis);
    let t = Quat::from_xyzw(p.x, p.y, p.z, q.w);
    if t.length_squared() < 1e-12 { Quat::IDENTITY } else { t.normalize() }
}

fn index(names: &[String], want: &str) -> Option<usize> {
    names.iter().position(|n| key(n) == want)
}

#[derive(Debug, Clone)]
pub struct Biped {
    bind: [Quat; 8],
    pelvis: Quat,
    arm_t: [Quat; 2],
}

impl Biped {
    pub fn from_joints(joints: &[omsi_o3d::Joint], arm_t: [Quat; 2]) -> Option<Biped> {
        let names: Vec<String> = joints.iter().map(|j| j.name.clone()).collect();
        let mut bind = [Quat::IDENTITY; 8];
        for (k, n) in NAMES.iter().enumerate() {
            bind[k] = joints[index(&names, n)?].rotation;
        }
        Some(Biped { pelvis: twist(bind[PELVIS], Vec3::Y), bind, arm_t })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Walk,
    Stand,
    Sit,
}

#[derive(Debug, Clone)]
pub struct Motion {
    pub kind: Kind,
    pub female: bool,
    fps: f32,
    frames: Vec<[Quat; 8]>,
}

impl Motion {
    pub fn from_clip(clip: &Clip, kind: Kind, female: bool) -> Option<Motion> {
        let ix: Vec<usize> = NAMES.iter().map(|n| index(&clip.names, n)).collect::<Option<_>>()?;
        let rel = |f: &[(Vec3, Quat)]| -> [Quat; 8] {
            let p = twist(f[ix[PELVIS]].1, Vec3::Y).inverse();
            std::array::from_fn(|k| p * f[ix[k]].1)
        };
        let (start, len) = match kind {
            Kind::Walk => walk_cycle(clip)?,
            _ => (0, clip.frames.len()),
        };
        if len < 2 {
            return None;
        }
        let n = clip.frames.len();
        let frames: Vec<[Quat; 8]> = (start..start + len).map(|i| rel(&clip.frames[i % n])).collect();
        if frames.iter().flatten().any(|q| !q.is_finite()) {
            return None;
        }
        Some(Motion { kind, female, fps: clip.fps.max(1.0), frames })
    }

    pub fn seconds(&self) -> f32 {
        self.frames.len() as f32 / self.fps
    }

    fn at(&self, x: f32) -> [Quat; 8] {
        let n = self.frames.len();
        let x = x.rem_euclid(n as f32);
        let (i, f) = (x.floor() as usize % n, x.fract());
        let (a, b) = (&self.frames[i], &self.frames[(i + 1) % n]);
        std::array::from_fn(|k| a[k].slerp(b[k], f))
    }

    fn cycle(&self, phase: f32) -> [Quat; 8] {
        self.at(phase * self.frames.len() as f32)
    }

    fn ping_pong(&self, seconds: f32) -> [Quat; 8] {
        let n = (self.frames.len() - 1) as f32;
        let x = (seconds * self.fps).rem_euclid(2.0 * n);
        self.at(if x > n { 2.0 * n - x } else { x })
    }
}

fn walk_cycle(clip: &Clip) -> Option<(usize, usize)> {
    let (p, l) = (index(&clip.names, "pelvis")?, index(&clip.names, "l foot")?);
    let rel: Vec<Vec2> = clip.frames.iter().map(|f| {
        let d = f[l].0 - f[p].0;
        Vec2::new(d.x, d.z)
    }).collect();
    let mean = rel.iter().copied().sum::<Vec2>() / rel.len() as f32;
    let (mut xx, mut xy, mut yy) = (0.0f32, 0.0f32, 0.0f32);
    for r in &rel {
        let d = *r - mean;
        xx += d.x * d.x;
        xy += d.x * d.y;
        yy += d.y * d.y;
    }
    let angle = 0.5 * (2.0 * xy).atan2(xx - yy);
    let axis = Vec2::new(angle.cos(), angle.sin());
    let s: Vec<f32> = rel.iter().map(|r| (*r - mean).dot(axis)).collect();
    let n = s.len();
    let w = 4usize.min(n / 4);
    let extreme = |i: usize, sign: f32| (1..=w).all(|k| sign * s[(i + k) % n] < sign * s[i] && sign * s[(i + n - k) % n] <= sign * s[i]);
    let maxima: Vec<usize> = (0..n).filter(|&i| extreme(i, 1.0)).collect();
    let minima: Vec<usize> = (0..n).filter(|&i| extreme(i, -1.0)).collect();
    let gap = |from: &[usize], to: &[usize]| -> Option<f32> {
        let d: Vec<usize> = from.iter().filter_map(|a| to.iter().map(|b| (b + n - a) % n).min()).collect();
        (!d.is_empty()).then(|| d.iter().sum::<usize>() as f32 / d.len() as f32)
    };
    let front = if gap(&maxima, &minima)? > gap(&minima, &maxima)? { maxima } else { minima };
    let a = *front.first()?;
    let len = front.get(1).map_or(n, |b| b - a);
    Some((a, len))
}

pub struct Mix<'a> {
    pub walk: &'a Motion,
    pub stand: &'a Motion,
    pub sit: &'a Motion,
}

impl Pose {
    pub fn clip_blend(&self) -> (f32, f32, f32, f32) {
        (self.phase, if self.gait { self.walk } else { 0.0 }, self.sit, self.reach.max(self.grip).max(self.hold))
    }
}

fn model(q: Quat) -> Quat {
    let c = Mat3::from_cols(Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, -1.0, 0.0));
    Quat::from_mat3(&(c * Mat3::from_quat(q) * c.transpose())).normalize()
}

fn rot_of(a: &Affine3A) -> Quat {
    Quat::from_mat3a(&a.matrix3).normalize()
}

pub fn apply(ty: &HumanType, posed: &mut Posed, mix: &Mix, blend: (f32, f32, f32, f32), seconds: f32) {
    let Some(b) = ty.biped.as_deref() else { return };
    let (phase, walk, sit, busy) = blend;
    let keep = busy.clamp(0.0, 1.0);
    if keep >= 1.0 || !posed.ok {
        return;
    }
    let (walk, sit) = (walk.clamp(0.0, 1.0), sit.clamp(0.0, 1.0));
    let mut rel = if walk >= 1.0 { mix.walk.cycle(phase) } else { mix.stand.ping_pong(seconds) };
    if walk > 0.0 && walk < 1.0 {
        let w = mix.walk.cycle(phase);
        rel = std::array::from_fn(|k| rel[k].slerp(w[k], walk));
    }
    if sit > 0.0 {
        let s = mix.sit.ping_pong(seconds);
        rel = std::array::from_fn(|k| rel[k].slerp(s[k], sit));
    }
    let saved = (posed.bones, posed.elbow, posed.wrist, posed.neck);
    let delta = |k: usize| model(b.pelvis * rel[k] * b.bind[k].inverse());
    let rig = &ty.rig;
    let pelvis_m = posed.bones[HIP];
    let qp = rot_of(&pelvis_m);
    let yaw = twist(qp, Vec3::Z);
    let old_main = posed.bones[MAIN];
    let trunk = (yaw * delta(SPINE)).slerp(rot_of(&old_main), keep);
    let main = pelvis_m * about(rig.waist, qp.inverse() * trunk);
    posed.bones[HEAD] = main * old_main.inverse() * posed.bones[HEAD];
    posed.bones[MAIN] = main;
    for s in 0..2 {
        let blend_rot = |bone: usize, slot: usize| (yaw * delta(bone) * b.arm_t[s].inverse()).slerp(rot_of(&posed.bones[slot]), keep);
        let (ru, rf, rh) = (blend_rot(UPPER_B[s], UPPER[s]), blend_rot(FORE_B[s], FORE[s]), blend_rot(HAND_B[s], HAND[s]));
        let sh = main.transform_point3(rig.shoulder[s]);
        let el = sh + ru * (rig.elbow[s] - rig.shoulder[s]);
        let wr = el + rf * (rig.wrist[s] - rig.elbow[s]);
        posed.bones[UPPER[s]] = joint_xf(rig.shoulder[s], sh, Mat3A::from_quat(ru));
        posed.bones[FORE[s]] = joint_xf(rig.elbow[s], el, Mat3A::from_quat(rf));
        posed.bones[HAND[s]] = joint_xf(rig.wrist[s], wr, Mat3A::from_quat(rh));
        posed.elbow[s] = el;
        posed.wrist[s] = wr;
    }
    posed.neck = posed.bones[HEAD].transform_point3(rig.head_pivot);
    if !posed.bones.iter().all(|b| b.is_finite()) {
        (posed.bones, posed.elbow, posed.wrist, posed.neck) = saved;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk_clip(fps: f32, period: usize, n: usize) -> Clip {
        let names = ["Bip01 Pelvis", "Bip01 L Foot"].iter().map(|s| s.to_string()).collect();
        let frames = (0..n)
            .map(|i| {
                let u = (i % period) as f32 / period as f32;
                let s = if u < 0.6 { 0.3 - u / 0.6 * 0.6 } else { -0.3 + (u - 0.6) / 0.4 * 0.6 };
                vec![(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY), (Vec3::new(0.1, 0.1, s), Quat::IDENTITY)]
            })
            .collect();
        Clip { fps, names, frames }
    }

    #[test]
    fn a_walk_cycle_starts_at_the_heel_strike() {
        let (start, len) = walk_cycle(&walk_clip(30.0, 32, 96)).unwrap();
        assert_eq!(len, 32);
        assert_eq!(start % 32, 0);
        let (start, len) = walk_cycle(&walk_clip(30.0, 32, 32)).unwrap();
        assert_eq!((start, len), (0, 32));
    }

    fn motion(frames: Vec<[Quat; 8]>) -> Motion {
        Motion { kind: Kind::Stand, female: false, fps: 10.0, frames }
    }

    #[test]
    fn idles_play_back_and_forth_and_cycles_wrap() {
        let m = motion((0..5).map(|i| [Quat::from_rotation_z(i as f32 * 0.1); 8]).collect());
        let angle = |q: [Quat; 8]| q[0].to_axis_angle().1;
        assert!((angle(m.ping_pong(0.4)) - 0.4).abs() < 1e-4);
        assert!((angle(m.ping_pong(0.6)) - 0.2).abs() < 1e-4, "back after the last frame");
        assert!((angle(m.ping_pong(0.8)) - 0.0).abs() < 1e-4);
        assert!((angle(m.cycle(1.0)) - angle(m.cycle(0.0))).abs() < 1e-5);
        assert!(angle(m.cycle(-0.1)).is_finite());
    }

    #[test]
    fn a_clip_with_a_broken_key_is_left_out() {
        let names: Vec<String> = NAMES.iter().map(|n| format!("Bip01 {n}")).collect();
        let mut f = vec![(Vec3::ZERO, Quat::IDENTITY); 8];
        f[3].1 = Quat::from_xyzw(f32::NAN, 0.0, 0.0, 1.0);
        let clip = Clip { fps: 30.0, names, frames: vec![f.clone(), f] };
        assert!(Motion::from_clip(&clip, Kind::Stand, true).is_none());
    }

    #[test]
    fn only_the_turn_about_the_vertical_is_kept() {
        let q = Quat::from_rotation_z(0.7) * Quat::from_rotation_x(0.4);
        let t = twist(q, Vec3::Z);
        assert!((t.to_axis_angle().1 - 0.7).abs() < 0.05);
        assert!(t.to_axis_angle().0.z.abs() > 0.99);
    }

    #[test]
    fn biped_names_match_with_or_without_prefix() {
        assert_eq!(key("Bip01 L UpperArm"), "l upperarm");
        assert_eq!(key("Bip001 Pelvis"), "pelvis");
    }

    #[test]
    #[ignore]
    fn rocketbox_clips_pose_an_avatar() {
        let (Ok(av), Ok(dir)) = (std::env::var("OMSI_TEST_AVATAR"), std::env::var("OMSI_TEST_CLIPS")) else { return };
        let ty = HumanType::load_avatar(Path::new(&av), &avatar::AvatarInfo { id: "t".into(), female: true, child: false }).unwrap();
        assert!(ty.biped.is_some());
        let load = |n: &str, k| Motion::from_clip(&omsi_o3d::fbx::load_clip(&Path::new(&dir).join(n)).unwrap(), k, true).unwrap();
        let (walk, stand, sit) = (load("f_walk_neutral_01.fbx", Kind::Walk), load("f_idle_neutral_01.fbx", Kind::Stand), load("f_sit_chair_idle_neutral_01.fbx", Kind::Sit));
        eprintln!("walk cycle {:.2} s, stand {:.1} s, sit {:.1} s", walk.seconds(), stand.seconds(), sit.seconds());
        assert!(walk.seconds() > 0.8 && walk.seconds() < 1.6);
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            let n = p.file_name().unwrap().to_string_lossy().to_string();
            let kind = if n.contains("_walk_") { Kind::Walk } else if n.contains("_sit_") { Kind::Sit } else { Kind::Stand };
            let m = Motion::from_clip(&omsi_o3d::fbx::load_clip(&p).unwrap(), kind, n.starts_with("f_"));
            let m = m.unwrap_or_else(|| panic!("{n}"));
            eprintln!("{n}: {:.2} s", m.seconds());
            assert!(kind != Kind::Walk || (m.seconds() > 0.7 && m.seconds() < 2.5), "{n}");
        }
        let mix = Mix { walk: &walk, stand: &stand, sit: &sit };
        let mut pose = Pose::new(3);
        let mut pos = DVec3::ZERO;
        for i in 0..120 {
            let speed = if i < 60 { 0.0 } else { 1.3 };
            pos.y += speed * 0.033;
            let input = PoseInput { activity: if speed > 0.0 { Activity::Walk } else { Activity::Stand }, origin: pos, velocity: DVec2::new(0.0, speed), ..Default::default() };
            pose.advance(&ty.rig, &input, 0.033);
            let mut posed = pose.bones(&ty.rig);
            apply(&ty, &mut posed, &mix, pose.clip_blend(), i as f32 * 0.033);
            let r = &ty.rig;
            for s in 0..2 {
                let (sh, el, wr) = (posed.bones[MAIN].transform_point3(r.shoulder[s]), posed.elbow[s], posed.wrist[s]);
                if i % 20 == 0 {
                    eprintln!("{i} side {s} shoulder {sh:.2} elbow {el:.2} wrist {wr:.2} blend {:?}", pose.clip_blend());
                }
                assert!(wr.z < sh.z - 0.25, "{i}: the hands hang: {wr} under {sh}");
                assert!((wr.x - sh.x).abs() < 0.3, "{i}: the arms by the sides: {wr} {sh}");
            }
            let up = posed.bones[MAIN].matrix3 * Vec3A::Z;
            assert!(up.z > 0.9, "{i}: the trunk upright: {up}");
        }
    }
}
