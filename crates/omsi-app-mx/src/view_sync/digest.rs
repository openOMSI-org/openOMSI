//! `OMSI_SCENE_DIGEST=1`: hashes of what the renderer holds of the scene, logged before each
//! picture an offscreen run takes, part by part - so that a part that moves with the wall
//! clock (textures finishing their upload on worker threads, tiles streamed in) does not
//! hide whether the others are the same:
//!
//! * `counts`: how many instances, meshes, textures, materials and interior lamps;
//! * `places`: every instance's mesh, transform and origin, in their order;
//! * `materials`: every instance's materials;
//! * `params`: every instance's per-slot alpha, light map, night map and texture offsets,
//!   visibility and interior light, and the interior lamps;
//! * `sim`: the instances the view sync owns (the AI cars' bodies and coupled parts, the
//!   people's meshes) - places and params - in the cars' and the people's order.
//!
//! Two runs whose renderers saw the same calls in the same order log the same digests: a
//! change that is meant to leave the renderer's calls as they were (the view sync's events,
//! for one) is checked with it against the commit before it.
//!
//! What moves with the wall clock rather than with the game (an instance placed or faded by
//! the time a frame took) makes `places` or `params` differ between two runs of one build.
//! To find it, each digest also writes every instance's own hashes to a file (its path is
//! logged): one line per instance, `index mesh place materials params owner` (owner: the AI
//! car or the person whose instance it is, `-` for the rest); the lines that differ between
//! two runs name the instances to look at.

use super::SimView;
use omsi_render::Scene;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn bits(v: &[f32], h: &mut DefaultHasher) {
    for x in v {
        x.to_bits().hash(h);
    }
}

fn place(scene: &Scene, i: usize, h: &mut DefaultHasher) {
    let Some(i) = scene.instances.get(i) else { return };
    i.mesh.hash(h);
    bits(&i.transform.to_cols_array(), h);
    (i.origin.x.to_bits(), i.origin.y.to_bits(), i.origin.z.to_bits()).hash(h);
}

fn params(scene: &Scene, i: usize, h: &mut DefaultHasher) {
    let Some(i) = scene.instances.get(i) else { return };
    bits(&i.slot_alpha, h);
    bits(&i.slot_light, h);
    bits(&i.slot_night, h);
    for uv in &i.slot_uv {
        bits(uv, h);
    }
    (i.visible, i.interior.to_bits(), i.interior_lamps).hash(h);
}

/// The digests of `scene` (see the module): (counts, places, materials, params, sim).
pub(crate) fn scene_digest(scene: &Scene, view: Option<&SimView>) -> [u64; 5] {
    let mut counts = DefaultHasher::new();
    (scene.instances.len(), scene.meshes.len(), scene.textures.len(), scene.materials.len(), scene.interior_lights.len()).hash(&mut counts);
    let (mut places, mut materials, mut pars) = (DefaultHasher::new(), DefaultHasher::new(), DefaultHasher::new());
    for k in 0..scene.instances.len() {
        place(scene, k, &mut places);
        scene.instances[k].materials.hash(&mut materials);
        params(scene, k, &mut pars);
    }
    for l in &scene.interior_lights {
        (l.position.x.to_bits(), l.position.y.to_bits(), l.position.z.to_bits(), l.intensity.to_bits()).hash(&mut pars);
    }
    let mut sim = DefaultHasher::new();
    if let Some(v) = view {
        for (id, inst) in v.traffic.instances().into_iter().chain(v.people.instances()) {
            (id, inst).hash(&mut sim);
            place(scene, inst, &mut sim);
            params(scene, inst, &mut sim);
        }
    }
    [counts.finish(), places.finish(), materials.finish(), pars.finish(), sim.finish()]
}

/// Every instance's own hashes, one line each (see the module).
fn instance_lines(scene: &Scene, view: Option<&SimView>) -> String {
    use std::fmt::Write;
    let mut owner: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    if let Some(v) = view {
        owner.extend(v.traffic.instances().into_iter().map(|(id, i)| (i, format!("car {id}"))));
        owner.extend(v.people.instances().into_iter().map(|(id, i)| (i, format!("person {id}"))));
    }
    let mut out = String::new();
    for (k, i) in scene.instances.iter().enumerate() {
        let (mut p, mut m, mut a) = (DefaultHasher::new(), DefaultHasher::new(), DefaultHasher::new());
        place(scene, k, &mut p);
        i.materials.hash(&mut m);
        params(scene, k, &mut a);
        let who = owner.get(&k).map_or("-", |s| s.as_str());
        let _ = writeln!(out, "{k} {} {:016x} {:016x} {:016x} {who}", i.mesh, p.finish(), m.finish(), a.finish());
    }
    out
}

/// Log the digests of `scene` (and of the instances of `view`) under `what` when
/// `OMSI_SCENE_DIGEST` asks for them, and write each instance's (see the module).
pub(crate) fn log_scene_digest(scene: &Scene, view: Option<&SimView>, what: &str) {
    if omsi_cfg::flags::OMSI_SCENE_DIGEST.is_set() {
        let [c, p, m, a, s] = scene_digest(scene, view);
        log::info!("scene digest {what}: counts {c:016x} places {p:016x} materials {m:016x} params {a:016x} sim {s:016x} ({} instances)", scene.instances.len());
        static TAKEN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = TAKEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("openomsi-scene-digest-{}-{what}-{n}.txt", std::process::id()));
        match std::fs::write(&path, instance_lines(scene, view)) {
            Ok(()) => log::info!("scene digest {what}: each instance's hashes in {}", path.display()),
            Err(e) => log::warn!("scene digest {what}: {}: {e}", path.display()),
        }
    }
}
