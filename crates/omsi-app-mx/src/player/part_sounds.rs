//! The sounds of the own train's coupled parts, each at its place.

use omsi_audio::{AudioEngine, SoundSet};

/// What the parts' own scripts fired (`VehicleInstance::take_part_fired`).
pub(super) type PartFired = (Vec<Option<Vec<String>>>, Vec<Vec<(String, String)>>);

/// Each coupled part's sounds at its place: a part that runs its own scripts (the NF6D's rear
/// cab car) by its own variables and what they fired, the others by the leading vehicle's
/// (`fired`, `at_fire`).
pub(super) fn play(
    a: &AudioEngine,
    ss: &mut SoundSet,
    v: &omsi_sim::VehicleInstance,
    own: &PartFired,
    fired: &[String],
    at_fire: &dyn Fn(&str, &str) -> Option<f32>,
) {
    for (i, part) in &mut ss.parts {
        let Some(t) = v.trailers.get(*i) else {
            part.stop_all(a);
            continue;
        };
        let pxf = t.world_transform();
        let var = |n: &str| t.run_var(n).or_else(|| v.var(n));
        match own.0.get(*i).and_then(|f| f.as_ref()) {
            Some(mine) => part.update_fired(a, &var, &pxf, mine, &|_, _| None),
            None => part.update_fired(a, &|n| v.var(n), &pxf, fired, at_fire),
        }
        for (trigger, file) in own.1.get(*i).into_iter().flatten() {
            part.play_file_trigger(a, trigger, file, &var, &pxf);
        }
    }
}
