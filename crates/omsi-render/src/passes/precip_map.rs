//! The rain's and the snow's roof: a map of what stands over the ground round the camera,
//! seen from straight above (the shadow casters' depth, as a sun at the zenith would draw
//! it). A drop or a flake under anything solid - a bus's roof, a car's, a shelter, a bridge,
//! the eaves of a house - is not drawn (rain.wgsl, snow.wgsl `under_roof`): before it, the
//! weather was kept out of the player's own bus alone, and only by its box, and from the
//! outside view it rained and snowed in every saloon, under every bridge and shelter.

use super::*;

/// How far the map reaches either side of the camera (m), and its texels a side: 8 cm.
pub(crate) const PRECIP_RANGE: f32 = 40.0;
pub(crate) const PRECIP_TEXELS: u32 = 1024;

/// The map's matrix round `cam_rel`: an orthographic box looking straight down from 150 m
/// above the camera to 150 m below it, its centre moved in whole texels (a still roof stays
/// on its texels while the camera moves).
pub(crate) fn precip_matrix(cam_rel: Vec3) -> Mat4 {
    let texel = PRECIP_RANGE * 2.0 / PRECIP_TEXELS as f32;
    let c = glam::Vec2::new((cam_rel.x / texel).round() * texel, (cam_rel.y / texel).round() * texel);
    let view = Mat4::look_at_rh(Vec3::new(c.x, c.y, cam_rel.z + 150.0), Vec3::new(c.x, c.y, cam_rel.z), Vec3::Y);
    Mat4::orthographic_rh(-PRECIP_RANGE, PRECIP_RANGE, -PRECIP_RANGE, PRECIP_RANGE, 0.0, 300.0) * view
}

/// The map's texture, the camera the casters are drawn with (the frame's own with the close
/// cascade's matrix swapped for the map's, so that the close cascade's pipelines draw it),
/// and the matrix it was last drawn with (`None` while nothing falls).
pub(crate) struct PrecipMap {
    pub(crate) depth_view: wgpu::TextureView,
    pub(crate) color_view: wgpu::TextureView,
    #[cfg(test)]
    color_texture: wgpu::Texture,
    pub(crate) camera_buf: wgpu::Buffer,
    pub(crate) matrix: std::cell::Cell<Option<Mat4>>,
}

impl PrecipMap {
    pub(crate) fn new(device: &wgpu::Device) -> PrecipMap {
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("precipitation roof depth"),
            size: wgpu::Extent3d { width: PRECIP_TEXELS, height: PRECIP_TEXELS, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("precipitation roof color"),
            size: wgpu::Extent3d { width: PRECIP_TEXELS, height: PRECIP_TEXELS, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("precipitation roof camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        PrecipMap {
            depth_view: depth.create_view(&Default::default()),
            color_view: color.create_view(&Default::default()),
            #[cfg(test)]
            color_texture: color,
            camera_buf,
            matrix: std::cell::Cell::new(None),
        }
    }

    /// What the rain's and the snow's shaders take of it: the matrix (identity) and whether
    /// there is a map to go by (0 none).
    pub(crate) fn uniform(&self) -> ([[f32; 4]; 4], f32) {
        match self.matrix.get() {
            Some(m) => (m.to_cols_array_2d(), 1.0),
            None => (Mat4::IDENTITY.to_cols_array_2d(), 0.0),
        }
    }
}

impl Renderer {
    /// The camera the map's casters are drawn with: the frame's, the close cascade's matrix
    /// the map's, and no sun to push a raised spline's shadow away from.
    pub(crate) fn write_precip_camera(&self, cu: &CameraUniform, m: Mat4) {
        let mut c = *cu;
        c.light_view_proj_close = m.to_cols_array_2d();
        c.sun_dir = [0.0; 4];
        self.queue.write_buffer(&self.shadows.precip.camera_buf, 0, bytemuck::bytes_of(&c));
    }

    /// Draw the map (when the frame has one: see `ShadowPlan::precip`).
    pub(crate) fn encode_precip_map(&self, encoder: &mut wgpu::CommandEncoder, scene: &Scene, sh: &ShadowPlan, shadow_batches: &[Vec<Batch>; SHADOW_LISTS], timers: &mut PassTimers) {
        let (Some(m), Some(bg)) = (sh.precip, scene.precip_bind_group.as_ref()) else {
            self.shadows.precip.matrix.set(None);
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("precipitation roof"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.shadows.precip.color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.shadows.precip.depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: pass_timer(timers.set.as_ref(), &mut timers.timed, "precipitation roof"),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, bg, &[]);
        encode_batches(&mut pass, scene, &shadow_batches[PRECIP_LIST], |pipe| &self.shadows.precip_pipelines[pipe as usize]);
        drop(pass);
        self.shadows.precip.matrix.set(Some(m));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The map looks straight down: what is higher lies nearer (a smaller depth), and a
    /// still point keeps its texel while the camera moves by fractions of one.
    #[test]
    fn the_roof_map_looks_down_and_moves_by_whole_texels() {
        let m = precip_matrix(Vec3::new(3.0, 4.0, 1.5));
        let roof = m.project_point3(Vec3::new(5.0, 6.0, 4.0));
        let floor = m.project_point3(Vec3::new(5.0, 6.0, 1.0));
        assert!(roof.z < floor.z && roof.z > 0.0 && floor.z < 1.0, "{roof:?} {floor:?}");
        assert!((roof.x - floor.x).abs() < 1e-6 && (roof.y - floor.y).abs() < 1e-6);
        let p = Vec3::new(10.3, -7.7, 0.0);
        let frac = |cam: Vec3| {
            let q = precip_matrix(cam).project_point3(p);
            let t = (glam::Vec2::new(q.x, q.y) * 0.5 + 0.5) * PRECIP_TEXELS as f32;
            t - t.round()
        };
        let f0 = frac(Vec3::ZERO);
        for k in 1..30 {
            assert!((frac(Vec3::new(0.011 * k as f32, -0.017 * k as f32, 0.0)) - f0).abs().max_element() < 0.02);
        }
    }
}

#[cfg(test)]
#[path = "precip_roof_gpu.rs"]
mod gpu_tests;
