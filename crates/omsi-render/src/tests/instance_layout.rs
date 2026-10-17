use super::*;
use std::mem::{align_of, offset_of, size_of};

#[test]
fn what_the_culling_reads_of_an_instance_fits_in_two_cache_lines() {
    assert_eq!(align_of::<Instance>(), 64);
    let ends = [
        offset_of!(Instance, mesh) + size_of::<MeshId>(),
        offset_of!(Instance, origin) + size_of::<DVec3>(),
        offset_of!(Instance, bounds) + size_of::<InstanceBounds>(),
        offset_of!(Instance, lod) + size_of::<(f32, f32)>(),
        offset_of!(Instance, object_radius) + 4,
        offset_of!(Instance, detail) + 4,
        offset_of!(Instance, any_distance) + 1,
        offset_of!(Instance, visible) + 1,
        offset_of!(Instance, mirror_only) + 1,
        offset_of!(Instance, blob) + 1,
        offset_of!(Instance, surface) + 1,
        offset_of!(Instance, casts_shadow) + 1,
        offset_of!(Instance, omsi_caster) + 1,
        offset_of!(Instance, near_only) + size_of::<Option<[f64; 4]>>(),
    ];
    assert!(ends.iter().all(|&e| e <= 128), "{ends:?}");
}
