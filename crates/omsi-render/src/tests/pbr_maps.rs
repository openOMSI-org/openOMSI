//! The PBR maps' directions on the GPU.

use super::*;

/// A PBR normal map tilts the surface the way its channels say: red towards +u, and green
/// towards +v for a Direct3D-style map (down the texture), towards -v for an OpenGL one.
/// Checked by the light of a low sun from either side of the tilt.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_normal_map_tilts_the_surface_the_way_its_channels_say() {
    let _neutral = crate::lighting::neutral_for_tests();
    let mut renderer = super::lamp_shadows::lamp_shadows_renderer();
    let mut scene = renderer.new_scene();
    let grey = renderer.add_texture(
        &mut scene,
        &omsi_texture::Image {
            width: 4,
            height: 4,
            rgba: [150, 150, 150, 255].repeat(16),
            has_alpha: false,
        },
        false,
    );
    // the ground: u along +x (east), v along -y (the texture's top to the north)
    let mesh = renderer.add_mesh(
        &mut scene,
        &MeshData {
            wire_wind: Vec::new(),
            positions: vec![
                Vec3::new(-20.0, 20.0, 0.0),
                Vec3::new(20.0, 20.0, 0.0),
                Vec3::new(20.0, -20.0, 0.0),
                Vec3::new(-20.0, -20.0, 0.0),
            ],
            normals: vec![Vec3::Z; 4],
            uvs: vec![
                glam::Vec2::new(0.0, 0.0),
                glam::Vec2::new(1.0, 0.0),
                glam::Vec2::new(1.0, 1.0),
                glam::Vec2::new(0.0, 1.0),
            ],
            ranges: vec![(0, 6, 0)],
            indices: vec![0, 1, 2, 0, 2, 3],
            one_sided: false,
        },
    );
    let mut lit = |tn: [u8; 3], gl: bool, sun: Vec3| -> f32 {
        let tex = renderer.add_texture(
            &mut scene,
            &omsi_texture::Image {
                width: 4,
                height: 4,
                rgba: [150, 150, 150, 255].repeat(16),
                has_alpha: false,
            },
            false,
        );
        renderer.add_pbr_maps(
            &mut scene,
            tex,
            &omsi_texture::pbr::PbrImages {
                normal: Some(omsi_texture::Image {
                    width: 4,
                    height: 4,
                    rgba: [tn[0], tn[1], tn[2], 255].repeat(16),
                    has_alpha: false,
                }),
                orm: None,
                flags: [if gl { 2.0 } else { 1.0 }, 0.0, 0.0, 0.0],
            },
        );
        let m = renderer.add_material(&mut scene, Some(tex), AlphaMode::Opaque, [1.0; 4], false);
        let i = renderer.add_instance(&mut scene, mesh, DVec3::ZERO, Mat4::IDENTITY, vec![m]);
        let lighting = Lighting {
            enhanced: true,
            shadows: false,
            sun_dir: sun.normalize(),
            sun_intensity: 1.0,
            fog_density: 0.0,
            ..Default::default()
        };
        let camera = Camera {
            position: DVec3::new(0.0, 0.0, 10.0),
            yaw: 0.0,
            pitch: -89.9,
            roll: 0.0,
            fov_deg: 60.0,
            near: 0.1,
            far: 200.0,
        };
        let rgba = renderer
            .render_to_image(&mut scene, 32, 32, &camera, &lighting)
            .unwrap();
        renderer.remove_instance(&mut scene, i);
        let k = (16 * 32 + 16) * 4;
        (rgba[k] as f32 + rgba[k + 1] as f32 + rgba[k + 2] as f32) / 3.0
    };
    let _ = grey;
    // red high: the normal leans east
    let (east, west) = (
        lit([230, 128, 200], false, Vec3::new(0.9, 0.0, 0.45)),
        lit([230, 128, 200], false, Vec3::new(-0.9, 0.0, 0.45)),
    );
    assert!(
        east > west + 3.0,
        "red leans to +u (east): {east} with the sun in the east, {west} in the west"
    );
    // green high, Direct3D: the normal leans down the texture, +v = south
    let (south, north) = (
        lit([128, 230, 200], false, Vec3::new(0.0, -0.9, 0.45)),
        lit([128, 230, 200], false, Vec3::new(0.0, 0.9, 0.45)),
    );
    assert!(
        south > north + 3.0,
        "Direct3D green leans to +v (south): {south} south, {north} north"
    );
    // and an OpenGL map the other way
    let (south_gl, north_gl) = (
        lit([128, 230, 200], true, Vec3::new(0.0, -0.9, 0.45)),
        lit([128, 230, 200], true, Vec3::new(0.0, 0.9, 0.45)),
    );
    assert!(
        north_gl > south_gl + 3.0,
        "OpenGL green leans to -v (north): {north_gl} north, {south_gl} south"
    );
}
