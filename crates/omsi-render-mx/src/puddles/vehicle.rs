//! The small planar capture of the player's vehicle for puddle reflections.

use crate::*;

use super::Pipelines;

/// The puddle vehicle pass's material draws, recorded into a pass or into a test encoder.
/// The chassis and the vehicle's batches both read the shared material table at their
/// own row: the group is bound once and each draw adds the row's dynamic offset.
pub(crate) fn encode<E: mx::RenderEncoder>(
    pass: &mut E,
    pipelines: &Pipelines,
    scene: &Scene,
    vehicle_batches: &[Batch],
    instances: u32,
    heap: &FrameHeap,
) {
    if omsi_cfg::flags::OMSI_NO_PUDDLE_VEHICLE.is_set() {
        return;
    }
    if let Some(first) = vehicle_batches.first() {
        let binding = &scene.materials[first.material as usize].binding;
        let slots = scene.material_table.immediates(binding);
        pass.set_bind_group(1, &scene.material_table.group);
        pass.set_buffer_offset(1, 2, MaterialTable::offset(binding.row.index));
        pass.set_immediates(0, bytemuck::cast_slice(&slots));
        pass.set_immediates(FrameHeap::OFFSET, bytemuck::bytes_of(heap));
        pass.set_pipeline(&pipelines.chassis);
        pass.draw(0..6, 0..instances);
    }
    encode_batches_in(
        pass,
        scene,
        heap,
        vehicle_batches,
        |_| true,
        |b| b.pipe as u16,
        |pipe| &pipelines.vehicle[pipe as usize],
    );
}
