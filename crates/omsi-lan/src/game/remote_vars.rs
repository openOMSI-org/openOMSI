use super::{RemoteVehicle, VarId};

pub(super) fn pin_view_values(rv: &RemoteVehicle, pinned: &mut Vec<(VarId, f32)>) {
    // Pin model animation inputs at the pose's playback time. The raw variable
    // updates must not overwrite interpolated doors and moving parts again.
    pinned.extend(rv.synced.iter().map(|(id, v)| {
        (
            *id as VarId,
            rv.animations.value(*id, rv.draw_time).unwrap_or(*v),
        )
    }));
    pinned.extend(rv.vehicle.ty.program.var("CameraView").map(|id| (id, 3.0)));
}
