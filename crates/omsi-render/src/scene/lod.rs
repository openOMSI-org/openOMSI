//! An object's size on the screen, which chooses its `[LOD]` level and drops it when too
//! small - the same for the picture, its shadows and the ray-traced scene, so that they
//! all draw the same level.

/// The object's diameter over its distance to the camera as a share of the vertical field
/// of view, as OMSI 2.2.032 measures it (0x5fde54..0x5fde89, the distance from
/// `D3DXVec3Length` at 0x5fdde4; docs/FORMATS.md "Object visibility"). It stays finite with
/// the camera inside the object's sphere: OMSI makes no exception there, and backdrops
/// count on it - a panorama a kilometre from its origin has a sphere one drives into long
/// before the picture, and only this size lets it give way to its transparent last level.
pub(crate) fn object_screen_size(radius: f32, distance: f32, fov_radians: f32) -> f32 {
    2.0 * radius / (distance.max(0.01) * fov_radians.max(1e-3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_backdrop_inside_origin_sphere_keeps_finite_lod() {
        // A visible plane offset 1100 m from the origin, LOD 5 followed by an
        // invisible LOD 0. The enclosing sphere reaches the camera at 1000 m.
        let size = object_screen_size(1810.0, 1000.0, 1.2);
        assert!((size - 3.0166667).abs() < 1e-5);
        assert!(size < 5.0);
    }

    #[test]
    fn ordinary_objects_keep_distance_order_and_close_detail() {
        let near = object_screen_size(5.0, 3.0, 1.0);
        let middle = object_screen_size(5.0, 50.0, 1.0);
        let far = object_screen_size(5.0, 200.0, 1.0);
        assert!(near > 0.1 && middle > 0.1 && far < 0.1);
        assert!((middle - 0.2).abs() < 1e-6);
        assert!(object_screen_size(5.0, 0.0, 1.0).is_finite());
    }

    #[test]
    #[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
    fn scenery_lod_uses_projection_with_offset_meshes_and_ordinary_objects() {
        use crate::*;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let mut renderer = pollster::block_on(Renderer::new_with(
            &instance,
            None,
            Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            RenderOptions {
                msaa: 1,
                ssao: false,
                shadow_size: 256,
                fxaa: false,
                render_scale: 1.0,
                ..Default::default()
            },
        ))
        .expect("test renderer");
        let camera = Camera {
            position: DVec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            fov_deg: 60.0,
            near: 0.1,
            far: 4000.0,
        };
        let lighting = Lighting {
            shadows: false,
            fog_density: 0.0,
            ..Default::default()
        };
        // radius, placement depth, mesh offset, LOD threshold, expected top level.
        // The first two used to force the visible panorama instead of the far LOD.
        for (radius, origin_y, offset_y, threshold, top) in [
            (1810.0, 1000.0, -990.0, 5.0, false),
            (1810.0, -1000.0, 1010.0, 5.0, false),
            (1810.0, 100.0, -90.0, 5.0, true),
            (5.0, 3.0, 0.0, 0.1, true),
            (5.0, 50.0, 0.0, 0.1, true),
            (5.0, 200.0, 0.0, 0.1, false),
        ] {
            let mut scene = renderer.new_scene();
            let half = (origin_y + offset_y) * 0.4;
            let mesh = MeshData {
                positions: vec![
                    Vec3::new(-half, offset_y, -half),
                    Vec3::new(half, offset_y, -half),
                    Vec3::new(half, offset_y, half),
                    Vec3::new(-half, offset_y, half),
                ],
                normals: vec![-Vec3::Y; 4],
                uvs: vec![glam::Vec2::ZERO; 4],
                indices: vec![0, 1, 2, 0, 2, 3],
                ranges: vec![(0, 6, 0)],
                ..Default::default()
            };
            let mesh = renderer.add_mesh(&mut scene, &mesh);
            for (min, max, color) in [
                (threshold, f32::MAX, [1.0, 0.0, 0.0, 1.0]),
                (0.0, threshold, [0.0, 0.0, 1.0, 1.0]),
            ] {
                let mat = renderer.add_material(&mut scene, None, AlphaMode::Opaque, color, true);
                let id = renderer.add_instance(
                    &mut scene,
                    mesh,
                    DVec3::new(0.0, origin_y as f64, 0.0),
                    Mat4::IDENTITY,
                    vec![mat],
                );
                renderer.set_object_culling(&mut scene, id, radius, 1.0, false);
                renderer.set_lod_range(&mut scene, id, min, max);
            }
            let rgba = renderer
                .render_to_image(&mut scene, 64, 64, &camera, &lighting)
                .unwrap();
            let pixel = &rgba[(32 * 64 + 32) * 4..][..3];
            assert!(
                if top {
                    pixel[0] > pixel[2].saturating_add(60)
                } else {
                    pixel[2] > pixel[0].saturating_add(60)
                },
                "origin={origin_y} offset={offset_y} radius={radius} expected top={top}: {pixel:?}"
            );
        }
    }
}
