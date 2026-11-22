//! What a driver figure reads of the vehicle it sits in: the player's bus is read in place,
//! a timetable bus through its copy in the traffic's snapshot (`DriverFrame`), so that the
//! traffic's simulation is not read while its drivers are posed.

use glam::{DVec3, Mat4};
use omsi_sim::view_events::{DriverFrame, FrameSnapshot};
use omsi_sim::{VehicleInstance, VehicleType};

/// The vehicle a [`super::DriverFigure`] sits in.
pub trait DriverCar {
    fn ty(&self) -> &VehicleType;
    fn position(&self) -> DVec3;
    /// The body's rotation (heading, pitch and roll).
    fn body_rotation(&self) -> Mat4;
    fn var(&self, name: &str) -> Option<f32>;
    /// The transform of mesh `i` in the vehicle's frame (`VehicleInstance::mesh_transforms`).
    fn mesh_transform(&self, i: usize) -> Option<Mat4>;
    /// The factor animation `anim` of mesh `i` turns by now, when it is not the model's (the
    /// player's cab wheel scaled by `cab_wheel_rotation`).
    fn anim_factor(&self, _i: usize, _anim: usize) -> Option<f32> {
        None
    }
}

impl DriverCar for VehicleInstance {
    fn ty(&self) -> &VehicleType {
        &self.ty
    }

    fn position(&self) -> DVec3 {
        self.position
    }

    fn body_rotation(&self) -> Mat4 {
        VehicleInstance::body_rotation(self)
    }

    fn var(&self, name: &str) -> Option<f32> {
        VehicleInstance::var(self, name)
    }

    fn mesh_transform(&self, i: usize) -> Option<Mat4> {
        self.mesh_transforms.get(i).copied()
    }

    fn anim_factor(&self, i: usize, anim: usize) -> Option<f32> {
        self.animators.get(i)?.anims.get(anim).map(|a| a.0.factor)
    }
}

/// A timetable bus of the traffic's snapshot, as its driver reads it.
pub struct SnapshotBus<'a> {
    pub snap: &'a FrameSnapshot,
    pub bus: &'a DriverFrame,
}

impl DriverCar for SnapshotBus<'_> {
    fn ty(&self) -> &VehicleType {
        &self.bus.ty
    }

    fn position(&self) -> DVec3 {
        self.bus.position
    }

    fn body_rotation(&self) -> Mat4 {
        self.bus.rotation
    }

    fn var(&self, name: &str) -> Option<f32> {
        let i = omsi_script::compile::with_lower(name, |k| self.bus.var_index.get(k).copied())?;
        self.snap.driver_vars(self.bus).get(i as usize).copied()
    }

    fn mesh_transform(&self, i: usize) -> Option<Mat4> {
        self.snap.driver_transforms(self.bus).get(i).copied()
    }
}
