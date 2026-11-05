//! Physical, monoscopic three-screen rig. Dimensions are millimetres; angles are
//! measured inward from a flat row of monitors. All views share the driver's eye.
use crate::{Camera, DVec3, Mat4, Vec3, Vec4};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TripleScreen {
    pub enabled: bool,
    pub width_mm: f32,
    pub distance_mm: f32,
    /// Combined hidden width at each join (both adjacent bezels).
    pub bezel_mm: f32,
    pub left_angle_deg: f32,
    pub right_angle_deg: f32,
    /// Eye above the centre of the panels.
    pub eye_height_mm: f32,
    /// Optional centre panel vertical FOV. Zero uses measured eye distance.
    pub fov_deg: f32,
}

impl Default for TripleScreen {
    fn default() -> Self {
        Self {
            enabled: false,
            width_mm: 600.0,
            distance_mm: 650.0,
            bezel_mm: 0.0,
            left_angle_deg: 45.0,
            right_angle_deg: 45.0,
            eye_height_mm: 0.0,
            fov_deg: 0.0,
        }
    }
}

/// One panel's width in pixels. All three are drawn at this same size, so they share the
/// renderer's size-keyed targets (depth, AO, rain glass) instead of rebuilding them per panel.
pub fn panel_width(width: u32) -> u32 {
    width.div_ceil(3)
}

pub struct ScreenView {
    pub camera: Camera,
    pub projection: Mat4,
    /// x, y, width, height in physical window pixels.
    pub viewport: [u32; 4],
}

impl TripleScreen {
    /// Zoom moves the common eye instead of changing one panel's projection.
    pub fn zoomed(&self, width: u32, height: u32, zoom: f32) -> Self {
        let mut rig = *self;
        if zoom.is_finite() && (zoom - 1.0).abs() > 1e-6 {
            let panel_height = self.width_mm * height as f32 / panel_width(width).max(1) as f32;
            let base = if self.fov_deg > 0.0 {
                self.fov_deg
            } else {
                (2.0 * (panel_height * 0.5 / self.distance_mm).atan()).to_degrees()
            };
            rig.fov_deg = (base * zoom).clamp(8.0, 120.0);
        }
        rig
    }

    /// The three panels, all of the same pixel size (`panel_width`): the right one is
    /// cropped at the window's edge when the width is not divisible by three.
    pub fn views(&self, camera: &Camera, width: u32, height: u32) -> [ScreenView; 3] {
        let pw = panel_width(width);
        // One common physical height: rounding a viewport by a pixel must not
        // move the horizon at the joins.
        let panel_height = self.width_mm * height as f32 / pw.max(1) as f32;
        // FOV zoom moves the common virtual eye, keeping all panel joins aligned.
        let eye_distance = if self.fov_deg > 0.0 {
            panel_height * 0.5 / (self.fov_deg.to_radians() * 0.5).tan()
        } else {
            self.distance_mm
        };
        std::array::from_fn(|i| {
            let angle = match i {
                0 => -self.left_angle_deg,
                2 => self.right_angle_deg,
                _ => 0.0,
            }
            .to_radians();
            let (s, c) = angle.sin_cos();
            let hinge = if i == 1 {
                0.0
            } else {
                self.width_mm * 0.5 + self.bezel_mm
            };
            let distance = eye_distance * c + hinge * s.abs();
            let offset = if i == 1 {
                0.0
            } else {
                angle.signum() * (hinge * c - eye_distance * s.abs() + self.width_mm * 0.5)
            };
            let half = self.width_mm * 0.5;
            let l = (offset - half) / distance;
            let r = (offset + half) / distance;
            let b = (-panel_height * 0.5 - self.eye_height_mm) / distance;
            let t = (panel_height * 0.5 - self.eye_height_mm) / distance;
            // Right-handed, 0..1 reversed depth. Unlike a symmetric FOV, the
            // shift accounts for the eye not being on the side panel's normal.
            let n = camera.near;
            let f = camera.depth_far();
            let projection = Mat4::from_cols(
                Vec4::new(2.0 / (r - l), 0.0, 0.0, 0.0),
                Vec4::new(0.0, 2.0 / (t - b), 0.0, 0.0),
                Vec4::new((r + l) / (r - l), (t + b) / (t - b), n / (f - n), -1.0),
                Vec4::new(0.0, 0.0, f * n / (f - n), 0.0),
            );
            let mut panel = *camera;
            let forward = camera.forward() * c + camera.right() * s;
            let up = camera.right().cross(camera.forward()).normalize();
            panel.yaw = forward.x.atan2(forward.y).to_degrees();
            panel.pitch = forward.z.clamp(-1.0, 1.0).asin().to_degrees();
            let right = Vec3::new(forward.y, -forward.x, 0.0).normalize_or(Vec3::X);
            panel.roll = up
                .dot(right)
                .atan2(up.dot(right.cross(forward)))
                .to_degrees();
            panel.fov_deg = (2.0 * ((t - b) * 0.5).atan()).to_degrees();
            ScreenView {
                camera: panel,
                projection,
                viewport: [i as u32 * pw, 0, pw, height],
            }
        })
    }

    /// Tangents of the widest horizontal and vertical half-angles the three panels show,
    /// measured from the centre camera's axis (capped at 85°): the frustum the whole rig
    /// covers, for "nothing appears or vanishes in sight".
    pub fn view_extent(&self, camera: &Camera, width: u32, height: u32) -> (f32, f32) {
        let cap = 85f32.to_radians().tan();
        let (f, r, u) = (camera.forward(), camera.right(), camera.up());
        let mut extent = (0.0f32, 0.0f32);
        for v in self.views(camera, width, height) {
            let inv = (v.projection * Mat4::look_to_rh(Vec3::ZERO, v.camera.forward(), v.camera.up())).inverse();
            for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let d = inv.project_point3(Vec3::new(x, y, 0.0)).normalize_or_zero();
                let z = d.dot(f);
                let (tx, ty) = if z > 1e-3 {
                    ((d.dot(r) / z).abs().min(cap), (d.dot(u) / z).abs().min(cap))
                } else {
                    (cap, cap)
                };
                extent = (extent.0.max(tx), extent.1.max(ty));
            }
        }
        extent
    }

    pub fn cursor_ray(
        &self,
        camera: &Camera,
        cursor: (f32, f32),
        size: (u32, u32),
    ) -> (DVec3, Vec3, f32) {
        let views = self.views(camera, size.0, size.1);
        let view = views
            .iter()
            .find(|v| cursor.0 < (v.viewport[0] + v.viewport[2]) as f32)
            .unwrap_or(&views[2]);
        let [x, _, w, h] = view.viewport;
        let ndc = Vec3::new(
            (cursor.0 - x as f32) / w.max(1) as f32 * 2.0 - 1.0,
            1.0 - cursor.1 / h.max(1) as f32 * 2.0,
            0.0,
        );
        let vp =
            view.projection * Mat4::look_to_rh(Vec3::ZERO, view.camera.forward(), view.camera.up());
        let direction = vp.inverse().project_point3(ndc).normalize();
        let spread = 2.0 / view.projection.y_axis.y / h.max(1) as f32;
        (camera.position, direction, spread)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn camera() -> Camera {
        Camera {
            position: DVec3::new(1e6, 2e6, 3.0),
            yaw: 17.0,
            pitch: 12.0,
            roll: 7.0,
            fov_deg: 60.0,
            near: 0.1,
            far: 5000.0,
        }
    }
    #[test]
    fn temporary_zoom_keeps_calibration_and_supports_close_views() {
        let rig = TripleScreen::default();
        assert_eq!(rig.zoomed(5760, 1080, 1.0), rig);
        let zoom = rig.zoomed(5760, 1080, 0.1);
        assert_eq!(zoom.fov_deg, 8.0);
        assert_eq!(rig.fov_deg, 0.0);
        assert!((zoom.views(&camera(), 5760, 1080)[1].camera.fov_deg - 8.0).abs() < 1e-4);
    }

    #[test]
    fn fov_override_changes_all_views_without_breaking_joins() {
        let cam = camera();
        let base = TripleScreen::default();
        let wide = TripleScreen {
            fov_deg: 80.0,
            ..base
        };
        let views = wide.views(&cam, 5760, 1080);
        assert!((views[1].camera.fov_deg - 80.0).abs() < 1e-4);
        let (_, before, _) = base.cursor_ray(&cam, (1920.0, 540.0), (5760, 1080));
        let (_, after, _) = wide.cursor_ray(&cam, (1920.0, 540.0), (5760, 1080));
        assert!(before.distance(after) > 0.1);
        let ray = |i: usize, x| {
            let v = &views[i];
            let vp = v.projection * Mat4::look_to_rh(Vec3::ZERO, v.camera.forward(), v.camera.up());
            vp.inverse()
                .project_point3(Vec3::new(x, 0.0, 0.0))
                .normalize()
        };
        assert!(ray(0, 1.0).distance(ray(1, -1.0)) < 1e-5);
        assert!(ray(1, 1.0).distance(ray(2, -1.0)) < 1e-5);
    }
    #[test]
    fn joins_are_continuous_with_pitch_and_roll() {
        for angle in [0.0, 30.0, 45.0, 70.0, 90.0] {
            let rig = TripleScreen {
                left_angle_deg: angle,
                right_angle_deg: angle,
                ..Default::default()
            };
            let cam = camera();
            let views = rig.views(&cam, 5760, 1080);
            let ray = |i: usize, x: f32, y: f32| {
                let v = &views[i];
                let vp =
                    v.projection * Mat4::look_to_rh(Vec3::ZERO, v.camera.forward(), v.camera.up());
                vp.inverse()
                    .project_point3(Vec3::new(x, y, 0.0))
                    .normalize()
            };
            for y in [-1.0, 0.0, 1.0] {
                assert!(
                    ray(0, 1.0, y).distance(ray(1, -1.0, y)) < 1e-5,
                    "left {angle}"
                );
                assert!(
                    ray(1, 1.0, y).distance(ray(2, -1.0, y)) < 1e-5,
                    "right {angle}"
                );
            }
        }
    }
    #[test]
    fn covers_every_pixel_and_reverses_depth() {
        let cam = camera();
        let rig = TripleScreen::default();
        let views = rig.views(&cam, 5761, 1080);
        // equal panels side by side, the last one cropped by the window's edge
        assert!(views.iter().all(|v| v.viewport[2] == 1921));
        assert_eq!(views[0].viewport[0], 0);
        for i in 0..2 {
            assert_eq!(views[i + 1].viewport[0], views[i].viewport[0] + views[i].viewport[2]);
        }
        let end = views[2].viewport[0] + views[2].viewport[2];
        assert!(end >= 5761 && end < 5761 + 3);
        for v in &views {
            assert!(
                (v.projection
                    .project_point3(Vec3::new(0.0, 0.0, -cam.near))
                    .z
                    - 1.0)
                    .abs()
                    < 1e-5
            );
            assert!(
                v.projection
                    .project_point3(Vec3::new(0.0, 0.0, -cam.depth_far()))
                    .z
                    .abs()
                    < 1e-5
            );
            assert_eq!(v.camera.position, cam.position);
        }
    }
    #[test]
    fn view_extent_covers_the_side_panels() {
        let cam = Camera { roll: 0.0, pitch: 0.0, ..camera() };
        let rig = TripleScreen::default();
        let (tx, ty) = rig.view_extent(&cam, 5760, 1080);
        // the centre panel alone: 300 mm either side at 650 mm
        assert!(tx > 300.0 / 650.0 * 2.0, "{tx}");
        assert!(ty >= rig.views(&cam, 5760, 1080)[1].projection.y_axis.y.recip() - 1e-4);
        // a flat row: the outer edges at 900 mm either side
        let flat = TripleScreen { left_angle_deg: 0.0, right_angle_deg: 0.0, ..rig };
        assert!((flat.view_extent(&cam, 5760, 1080).0 - 900.0 / 650.0).abs() < 1e-3);
    }

    #[test]
    fn bezel_hides_a_gap_and_eye_height_shifts_horizon() {
        let cam = camera();
        let rig = TripleScreen {
            bezel_mm: 20.0,
            eye_height_mm: 50.0,
            ..Default::default()
        };
        let (_, left, _) = rig.cursor_ray(&cam, (1919.99, 540.0), (5760, 1080));
        let (_, centre, _) = rig.cursor_ray(&cam, (1920.0, 540.0), (5760, 1080));
        assert!(left.distance(centre) > 0.01);
        let v = rig.views(&cam, 5760, 1080);
        assert!(v[1].projection.z_axis.y < 0.0);
    }

    #[test]
    #[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
    fn gpu_renders_each_panel_and_composites_hud_with_msaa() {
        use crate::{AlphaMode, Lighting, MaterialExtra, MeshData, RenderOptions, Renderer};
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let cam = Camera {
            position: DVec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            ..camera()
        };
        let rig = TripleScreen::default();
        for (msaa, render_scale) in [(1, 1.0), (4, 1.0), (4, 0.65)] {
            let mut renderer = pollster::block_on(Renderer::new_with(
                &instance,
                None,
                Some(wgpu::TextureFormat::Rgba8UnormSrgb),
                RenderOptions {
                    msaa,
                    ssao: true,
                    shadow_size: 1024,
                    fxaa: true,
                    render_scale,
                    ..Default::default()
                },
            ))
            .unwrap();
            let mut scene = renderer.new_scene();
            let colours = [
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0, 1.0],
            ];
            for i in 0..3 {
                // Put a coloured marker exactly on the pixel ray of that panel.
                let (_, ray, _) = rig.cursor_ray(&cam, (i as f32 * 64.0 + 32.0, 32.0), (192, 64));
                let right = Vec3::new(ray.y, -ray.x, 0.0).normalize();
                let up = right.cross(ray);
                let centre = ray * 10.0;
                let quad = MeshData {
                    positions: vec![
                        centre - right - up,
                        centre + right - up,
                        centre + right + up,
                        centre - right + up,
                    ],
                    normals: vec![-ray; 4],
                    uvs: vec![glam::Vec2::ZERO; 4],
                    ranges: vec![(0, 6, 0)],
                    indices: vec![0, 1, 2, 0, 2, 3],
                    one_sided: false,
                };
                let mesh = renderer.add_mesh(&mut scene, &quad);
                let material =
                    renderer.add_material(&mut scene, None, AlphaMode::Opaque, colours[i], true);
                renderer.add_instance(
                    &mut scene,
                    mesh,
                    DVec3::ZERO,
                    Mat4::IDENTITY,
                    vec![material],
                );
                let film = renderer.add_material_extra(
                    &mut scene,
                    None,
                    AlphaMode::Blend,
                    [1.0, 1.0, 1.0, 0.08],
                    false,
                    None,
                    None,
                    None,
                    None,
                    [0.0; 3],
                    MaterialExtra {
                        rain_film: true,
                        ..Default::default()
                    },
                );
                renderer.add_instance(
                    &mut scene,
                    mesh,
                    DVec3::ZERO,
                    Mat4::from_scale(Vec3::splat(0.5)),
                    vec![film],
                );
            }
            let white = renderer.add_texture(
                &mut scene,
                &omsi_texture::Image {
                    width: 1,
                    height: 1,
                    rgba: vec![255; 4],
                    has_alpha: false,
                },
                false,
            );
            scene.overlays.push((white, [0.0, 0.0, 192.0, 4.0]));
            for enhanced in [false, true] {
                let lighting = Lighting {
                    enhanced,
                    shadows: true,
                    fog_density: 0.0,
                    rain: 0.5,
                    wetness: 0.5,
                    ..Default::default()
                };
                let rgba = renderer
                    .render_triple_to_image(&mut scene, 192, 64, &cam, &lighting, &rig)
                    .unwrap();
                for i in 0..3 {
                    let at = ((32 * 192 + i * 64 + 32) * 4) as usize;
                    let pixel = &rgba[at..at + 3];
                    assert!(
                        pixel[i] > 80 && pixel[i] > pixel[(i + 1) % 3] * 2,
                        "panel {i}, msaa {msaa}, enhanced {enhanced}: {pixel:?}"
                    );
                    let at = ((2 * 192 + i * 64 + 32) * 4) as usize;
                    assert!(
                        rgba[at..at + 3].iter().all(|c| *c > 240),
                        "HUD must cover every panel at full resolution"
                    );
                }
                assert_eq!(scene.overlays.len(), 1);
                assert!(renderer.env_heading.get().is_none());
                assert!(renderer.history.cull_drawn.borrow().is_empty());
                assert_ne!(
                    renderer.history.triple_culling[0].drawn,
                    renderer.history.triple_culling[1].drawn
                );
                assert_ne!(
                    renderer.history.triple_culling[2].drawn,
                    renderer.history.triple_culling[1].drawn
                );
                // Rain must sample this panel's current scene, including when
                // the three panels reuse the same scratch texture dimensions.
                let behind = renderer.targets.glass_picture.as_ref().unwrap().texture();
                let data = renderer
                    .read_texture(behind, wgpu::TextureAspect::All)
                    .unwrap();
                let at = ((behind.height() / 2 * behind.width() + behind.width() / 2)
                    * if enhanced { 8 } else { 4 }) as usize;
                if enhanced {
                    let channel =
                        |i: usize| u16::from_le_bytes([data[at + i * 2], data[at + i * 2 + 1]]);
                    assert!(channel(2) > channel(0) && channel(2) > channel(1));
                } else {
                    assert!(data[at + 2] > data[at] && data[at + 2] > data[at + 1]);
                }
                scene.overlays[0].1 = [64.0, 0.0, 128.0, 4.0];
                let centred = renderer
                    .render_triple_to_image(&mut scene, 193, 64, &cam, &lighting, &rig)
                    .unwrap();
                for i in 0..3 {
                    let at = ((2 * 193 + i * 64 + 32) * 4) as usize;
                    assert_eq!(centred[at..at + 3].iter().all(|c| *c > 240), i == 1);
                }
                scene.overlays.clear();
                let clear = renderer
                    .render_triple_to_image(&mut scene, 192, 64, &cam, &lighting, &rig)
                    .unwrap();
                let at = ((2 * 192 + 96) * 4) as usize;
                assert!(!clear[at..at + 3].iter().all(|c| *c > 240));
                scene.overlays.push((white, [0.0, 0.0, 192.0, 4.0]));
            }
        }
    }
}
