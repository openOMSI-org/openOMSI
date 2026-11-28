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
    let c = glam::Vec2::new(
        (cam_rel.x / texel).round() * texel,
        (cam_rel.y / texel).round() * texel,
    );
    let view = Mat4::look_at_rh(
        Vec3::new(c.x, c.y, cam_rel.z + 150.0),
        Vec3::new(c.x, c.y, cam_rel.z),
        Vec3::Y,
    );
    Mat4::orthographic_rh(
        -PRECIP_RANGE,
        PRECIP_RANGE,
        -PRECIP_RANGE,
        PRECIP_RANGE,
        0.0,
        300.0,
    ) * view
}

/// Enhanced's dry map: what stands over the roads round the camera - bridges, the roofs of
/// depots, shelters and petrol stations, tunnels - seen from straight above, without the
/// vehicles and the cut-out leaves (the road under a bus that drives on is wet, and rain
/// comes through the trees). A road under it stays dry in the rain: no film of water, no
/// puddles, no rings (`roof_dry` in enhanced/03_surface.wgsl). Further out than the
/// precipitation's map, at a coarser texel: its edge is soft anyway, and a dry patch under a
/// bridge should not appear only once the camera is within 40 m of it.
pub(crate) const DRY_RANGE: f32 = 128.0;
pub(crate) const DRY_TEXELS: u32 = 1024;
/// The dry map is redrawn once the camera is this far (m, level) from its middle, or it is
/// `DRY_REDRAW_SECS` old: what it holds stands still, only the tiles the map loads add to
/// it.
const DRY_SHIFT: f32 = 32.0;
const DRY_REDRAW_SECS: f32 = 2.0;
/// How far the dry map's box reaches above and below its middle (m).
pub(crate) const DRY_DEPTH: f32 = 150.0;

/// The dry map's matrix round `mid` (render frame): as `precip_matrix`, `DRY_RANGE` a side.
pub(crate) fn dry_matrix(mid: Vec3) -> Mat4 {
    let view = Mat4::look_at_rh(
        Vec3::new(mid.x, mid.y, mid.z + DRY_DEPTH),
        Vec3::new(mid.x, mid.y, mid.z),
        Vec3::Y,
    );
    Mat4::orthographic_rh(
        -DRY_RANGE,
        DRY_RANGE,
        -DRY_RANGE,
        DRY_RANGE,
        0.0,
        DRY_DEPTH * 2.0,
    ) * view
}

/// Where the dry map was last drawn: its middle (world, absolute) and when.
#[derive(Clone, Copy)]
pub(crate) struct DryDrawn {
    pub(crate) mid: glam::DVec3,
    pub(crate) at: std::time::Instant,
}

/// The map's texture, the camera the casters are drawn with (the frame's own with the close
/// cascade's matrix swapped for the map's, so that the close cascade's pipelines draw it),
/// and the matrix it was last drawn with (`None` while nothing falls). The same for the dry
/// map (`DRY_RANGE`).
pub(crate) struct PrecipMap {
    pub(crate) view: mx::TextureView,
    pub(crate) camera_buf: mx::Buffer,
    pub(crate) matrix: std::cell::Cell<Option<Mat4>>,
    pub(crate) dry_view: mx::TextureView,
    pub(crate) dry_camera_buf: mx::Buffer,
    pub(crate) dry: std::cell::Cell<Option<DryDrawn>>,
}

impl PrecipMap {
    pub(crate) fn new(device: &mx::Device) -> PrecipMap {
        let depth_map = |label, texels| {
            device
                .create_texture(&mx::TextureDescriptor {
                    label: Some(label),
                    size: mx::Extent3d {
                        width: texels,
                        height: texels,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: mx::TextureDimension::D2,
                    format: DEPTH_FORMAT,
                    usage: mx::TextureUsages::RENDER_ATTACHMENT
                        | mx::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .expect("mX: a texture")
                .create_view(&Default::default())
                .expect("mX: a texture view")
        };
        let camera = |label| {
            device
                .create_buffer(&mx::BufferDescriptor {
                    label: Some(label),
                    size: std::mem::size_of::<CameraUniform>() as u64,
                    usage: mx::BufferUsages::UNIFORM
                        | mx::BufferUsages::STORAGE
                        | mx::BufferUsages::COPY_DST,
                })
                .expect("mX: a buffer")
        };
        PrecipMap {
            view: depth_map("precipitation roof map", PRECIP_TEXELS),
            camera_buf: camera("precipitation roof camera"),
            matrix: std::cell::Cell::new(None),
            dry_view: depth_map("dry map", DRY_TEXELS),
            dry_camera_buf: camera("dry map camera"),
            dry: std::cell::Cell::new(None),
        }
    }

    /// The enhanced uniform's dry map slot (`Enhanced::glass_ext`, see `roof_dry`): xy the
    /// map's middle, z its top (render frame, `ro` the render origin), w 1 when there is a
    /// map to go by.
    pub(crate) fn dry_slot(&self, ro: glam::DVec3) -> [f32; 4] {
        match self.dry.get() {
            Some(d) => {
                let m = (d.mid - ro).as_vec3();
                [m.x, m.y, m.z + DRY_DEPTH, 1.0]
            }
            None => [0.0; 4],
        }
    }

    /// Whether the dry map is to be drawn again round the camera at `cam` (world): none yet,
    /// the camera `DRY_SHIFT` from its middle, or it is old.
    pub(crate) fn dry_due(&self, cam: glam::DVec3) -> bool {
        self.dry.get().is_none_or(|d| {
            let off = cam - d.mid;
            off.truncate().length() > DRY_SHIFT as f64
                || off.z.abs() > DRY_DEPTH as f64 * 0.5
                || d.at.elapsed().as_secs_f32() > DRY_REDRAW_SECS
        })
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
        self.queue
            .write_buffer(&self.shadows.precip.camera_buf, 0, bytemuck::bytes_of(&c))
            .expect("mX: a queue write");
    }

    /// The same for the dry map (`m` its matrix, `mid` its middle), which is drawn this frame:
    /// from here on the enhanced uniform reads it there (`PrecipMap::dry_slot`).
    pub(crate) fn write_dry_camera(&self, cu: &CameraUniform, m: Mat4, mid: glam::DVec3) {
        self.shadows.precip.dry.set(Some(DryDrawn {
            mid,
            at: std::time::Instant::now(),
        }));
        let mut c = *cu;
        c.light_view_proj_close = m.to_cols_array_2d();
        c.sun_dir = [0.0; 4];
        self.queue
            .write_buffer(
                &self.shadows.precip.dry_camera_buf,
                0,
                bytemuck::bytes_of(&c),
            )
            .expect("mX: a queue write");
    }

    /// Draw the dry map (when the frame redraws it: see `ShadowPlan::dry`).
    pub(crate) fn encode_dry_map(
        &self,
        encoder: &mut mx::CommandEncoder,
        scene: &Scene,
        sh: &ShadowPlan,
        shadow_batches: &[Vec<Batch>; SHADOW_LISTS],
        timers: &mut PassTimers,
    ) {
        if sh.dry.is_none() {
            return;
        };
        let mut pass = encoder.begin_render_pass(&mx::RenderPassDescriptor {
            label: Some("dry map"),
            color_attachments: &[],
            depth_stencil_attachment: Some(mx::RenderPassDepthStencilAttachment {
                view: &self.shadows.precip.dry_view,
                depth_ops: Some(mx::Operations {
                    load: mx::LoadOp::Clear(1.0),
                    store: mx::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: pass_timer(timers.set.as_ref(), &mut timers.timed, "dry map"),
            occlusion_query_set: None,
        });
        // (the dry map's camera in the heap: `Scene::dry_heap`)
        encode_batches_in(
            &mut pass,
            scene,
            &scene.dry_heap,
            &shadow_batches[DRY_LIST],
            |_| true,
            |b| b.pipe as u16,
            |pipe| &self.shadows.shadow_pipelines[4 + pipe as usize],
        );
        drop(pass);
    }

    /// Draw the map (when the frame has one: see `ShadowPlan::precip`).
    pub(crate) fn encode_precip_map(
        &self,
        encoder: &mut mx::CommandEncoder,
        scene: &Scene,
        sh: &ShadowPlan,
        shadow_batches: &[Vec<Batch>; SHADOW_LISTS],
        timers: &mut PassTimers,
    ) {
        let Some(m) = sh.precip else {
            self.shadows.precip.matrix.set(None);
            return;
        };
        let mut pass = encoder.begin_render_pass(&mx::RenderPassDescriptor {
            label: Some("precipitation roof"),
            color_attachments: &[],
            depth_stencil_attachment: Some(mx::RenderPassDepthStencilAttachment {
                view: &self.shadows.precip.view,
                depth_ops: Some(mx::Operations {
                    load: mx::LoadOp::Clear(1.0),
                    store: mx::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: pass_timer(
                timers.set.as_ref(),
                &mut timers.timed,
                "precipitation roof",
            ),
            occlusion_query_set: None,
        });
        // (the roof map's camera in the heap: `Scene::precip_heap`)
        encode_batches_in(
            &mut pass,
            scene,
            &scene.precip_heap,
            &shadow_batches[PRECIP_LIST],
            |_| true,
            |b| b.pipe as u16,
            |pipe| &self.shadows.shadow_pipelines[4 + pipe as usize],
        );
        drop(pass);
        self.shadows.precip.matrix.set(Some(m));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The map looks straight down: what is higher lies nearer (a smaller depth), and a
    /// still point keeps its texel while the camera moves by fractions of one.
    /// The dry map looks straight down from `DRY_DEPTH` over its middle, as `roof_dry`
    /// reads it: a point's depth is its drop from the top over twice that.
    #[test]
    fn the_dry_map_is_read_as_the_shader_reads_it() {
        let mid = Vec3::new(30.0, -12.0, 4.0);
        let m = dry_matrix(mid);
        for p in [Vec3::new(31.0, -10.0, 9.0), Vec3::new(-60.0, 80.0, -3.0)] {
            let q = m.project_point3(p);
            let top = mid.z + DRY_DEPTH;
            assert!(
                (q.z - (top - p.z) / (2.0 * DRY_DEPTH)).abs() < 1e-5,
                "{q:?}"
            );
            assert!((q.x - (p.x - mid.x) / DRY_RANGE).abs() < 1e-5, "{q:?}");
            assert!((q.y - (p.y - mid.y) / DRY_RANGE).abs() < 1e-5, "{q:?}");
        }
    }

    #[test]
    fn the_roof_map_looks_down_and_moves_by_whole_texels() {
        let m = precip_matrix(Vec3::new(3.0, 4.0, 1.5));
        let roof = m.project_point3(Vec3::new(5.0, 6.0, 4.0));
        let floor = m.project_point3(Vec3::new(5.0, 6.0, 1.0));
        assert!(
            roof.z < floor.z && roof.z > 0.0 && floor.z < 1.0,
            "{roof:?} {floor:?}"
        );
        assert!((roof.x - floor.x).abs() < 1e-6 && (roof.y - floor.y).abs() < 1e-6);
        let p = Vec3::new(10.3, -7.7, 0.0);
        let frac = |cam: Vec3| {
            let q = precip_matrix(cam).project_point3(p);
            let t = (glam::Vec2::new(q.x, q.y) * 0.5 + 0.5) * PRECIP_TEXELS as f32;
            t - t.round()
        };
        let f0 = frac(Vec3::ZERO);
        for k in 1..30 {
            assert!(
                (frac(Vec3::new(0.011 * k as f32, -0.017 * k as f32, 0.0)) - f0)
                    .abs()
                    .max_element()
                    < 0.02
            );
        }
    }
}
