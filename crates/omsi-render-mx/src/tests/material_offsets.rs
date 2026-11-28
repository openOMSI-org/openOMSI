//! The material table's dynamic offsets after it grows, on the paths #4379's Vulkan driver
//! would take: an ordinary draw, the puddle vehicle pass, and a kept main-pass bundle.

use super::*;

#[derive(Debug, PartialEq)]
enum Event {
    Pipeline,
    BindGroup(u32),
    BufferOffset(u32, u32, u64),
    Immediates(u32, usize),
    VertexBuffer,
    IndexBuffer,
    Draw,
    DrawIndexed,
    DrawIndirect,
    DrawIndexedIndirect,
}

/// A render encoder that only records the state a material draw sets. mX encodes ordinary
/// passes, bundles and the puddle vehicle pass through the same `RenderEncoder` methods, so
/// this checks the Vulkan-bound calls without needing an RTX 3060 on the test machine.
#[derive(Default)]
struct Recorder {
    events: Vec<Event>,
}

impl Recorder {
    fn offsets(&self) -> Vec<u64> {
        self.events
            .iter()
            .filter_map(|e| match e {
                Event::BufferOffset(1, 2, offset) => Some(*offset),
                _ => None,
            })
            .collect()
    }
}

impl mx::RenderEncoder for Recorder {
    fn set_pipeline(&mut self, _pipeline: &mx::RenderPipeline) {
        self.events.push(Event::Pipeline);
    }
    fn set_bind_group(&mut self, group: u32, _group_ref: &mx::BindGroup) {
        self.events.push(Event::BindGroup(group));
    }
    fn set_buffer_offset(&mut self, group: u32, binding: u32, offset: u64) {
        self.events
            .push(Event::BufferOffset(group, binding, offset));
    }
    fn set_immediates(&mut self, offset: u32, data: &[u8]) {
        self.events.push(Event::Immediates(offset, data.len()));
    }
    fn set_vertex_buffer(&mut self, _slot: u32, _buffer: &mx::Buffer, _offset: u64) {
        self.events.push(Event::VertexBuffer);
    }
    fn set_index_buffer(&mut self, _buffer: &mx::Buffer, _offset: u64, _format: mx::IndexFormat) {
        self.events.push(Event::IndexBuffer);
    }
    fn draw(&mut self, _vertices: std::ops::Range<u32>, _instances: std::ops::Range<u32>) {
        self.events.push(Event::Draw);
    }
    fn draw_indexed(
        &mut self,
        _indices: std::ops::Range<u32>,
        _base_vertex: i32,
        _instances: std::ops::Range<u32>,
    ) {
        self.events.push(Event::DrawIndexed);
    }
    fn draw_indirect(&mut self, _args: &mx::Buffer, _offset: u64) {
        self.events.push(Event::DrawIndirect);
    }
    fn draw_indexed_indirect(&mut self, _args: &mx::Buffer, _offset: u64) {
        self.events.push(Event::DrawIndexedIndirect);
    }
}

fn renderer() -> Renderer {
    pollster::block_on(Renderer::new_with(
        &mx::Instance::with_backends(mx::Backends::NOOP),
        None,
        Some(mx::TextureFormat::Rgba8UnormSrgb),
        RenderOptions {
            msaa: 1,
            ssao: false,
            shadow_size: 256,
            fxaa: false,
            render_scale: 1.0,
            ..Default::default()
        },
    ))
    .expect("test renderer")
}

fn mesh(r: &Renderer, scene: &mut Scene) -> MeshId {
    let h = 2.0;
    r.add_mesh(
        scene,
        &MeshData {
            positions: vec![
                Vec3::new(-h, 0.0, -h),
                Vec3::new(h, 0.0, -h),
                Vec3::new(h, 0.0, h),
                Vec3::new(-h, 0.0, h),
            ],
            normals: vec![Vec3::Y; 4],
            uvs: vec![glam::Vec2::ZERO; 4],
            ranges: vec![(0, 6, 0)],
            indices: vec![0, 1, 2, 0, 2, 3],
            one_sided: false,
        },
    )
}

/// Start with a one-row table so the second material grows it and replaces its bind group,
/// as precaching many AI vehicle types did in #4379. No adapter limit is needed: the calls
/// that pick a row are the same on the Vulkan backend that ran out of buffer slots.
fn grown_scene(r: &Renderer) -> (Scene, MeshId, MaterialId, MaterialId) {
    let mut scene = r.new_scene();
    scene.material_table = MaterialTable::new(&r.device, 1);
    let mesh = mesh(r, &mut scene);
    let first = r.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [1.0, 0.0, 0.0, 1.0],
        true,
    );
    let second = r.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 1.0, 0.0, 1.0],
        true,
    );
    assert!(scene.material_table.buffer.size() > MATERIAL_ROW_BYTES);
    assert_eq!(scene.materials[first].binding.row.index, 0);
    assert!(scene.materials[second].binding.row.index > 0);
    (scene, mesh, first, second)
}

fn batch(material: MaterialId, mesh: MeshId) -> Batch {
    Batch {
        pipe: 0,
        mesh: mesh as u32,
        first: 0,
        count: 6,
        material: material as u32,
        instances: 0..1,
        lean: Lean::No,
        vehicle: false,
    }
}

#[test]
fn a_draw_after_growth_binds_each_material_at_its_dynamic_row_offset() {
    let r = renderer();
    let (scene, mesh, first, second) = grown_scene(&r);
    let rows = [
        scene.materials[first].binding.row.index,
        scene.materials[second].binding.row.index,
    ];
    let mut recorder = Recorder::default();
    encode_batches_in(
        &mut recorder,
        &scene,
        &scene.frame_heap,
        &[batch(first, mesh), batch(second, mesh)],
        |_| true,
        |b| b.pipe as u16,
        |_| &r.overlay_pipeline_1x,
    );
    assert!(recorder.events.contains(&Event::BindGroup(1)));
    assert_eq!(
        recorder.offsets(),
        rows.iter()
            .map(|&row| scene.material_table.offset(row))
            .collect::<Vec<_>>(),
        "each draw after growth must select its row at the table's dynamic offset"
    );
    assert_eq!(r.device.bindless_buffers_in_use(), 0);
}

#[test]
fn the_puddle_vehicle_pass_binds_a_nonzero_row_after_growth() {
    let r = renderer();
    let (scene, mesh, first, second) = grown_scene(&r);
    let pipelines = r.puddles.as_ref().expect("puddle pipelines");
    let row = scene.materials[second].binding.row.index;
    let mut recorder = Recorder::default();
    crate::puddles::vehicle::encode(
        &mut recorder,
        pipelines,
        &scene,
        &[batch(second, mesh), batch(first, mesh)],
        1,
        &FrameHeap::default(),
    );
    assert_eq!(
        recorder
            .events
            .iter()
            .filter(|e| **e == Event::BindGroup(1))
            .count(),
        2,
        "the chassis and the vehicle batches each bind the current table"
    );
    assert_eq!(
        recorder.offsets(),
        vec![
            scene.material_table.offset(row),
            scene.material_table.offset(row),
            scene
                .material_table
                .offset(scene.materials[first].binding.row.index),
        ],
        "the chassis and the first vehicle batch must draw at the grown row"
    );
    assert!(row > 0);
    assert_eq!(r.device.bindless_buffers_in_use(), 0);
}

#[test]
fn a_kept_bundle_after_growth_is_reused_with_the_new_table() {
    let mut r = renderer();
    let mut scene = r.new_scene();
    scene.material_table = MaterialTable::new(&r.device, 1);
    let id = mesh(&r, &mut scene);
    let first = r.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [1.0, 0.0, 0.0, 1.0],
        true,
    );
    r.add_instance(&mut scene, id, DVec3::ZERO, Mat4::IDENTITY, vec![first]);
    let camera = Camera {
        position: DVec3::new(0.0, -8.0, 4.0),
        yaw: 0.0,
        pitch: -20.0,
        roll: 0.0,
        fov_deg: 90.0,
        near: 0.1,
        far: 200.0,
    };
    let lighting = Lighting::default();
    let frame = |r: &mut Renderer, s: &mut Scene| {
        r.render_to_image(s, 32, 32, &camera, &lighting).unwrap();
    };
    frame(&mut r, &mut scene);
    let parts = r.history.bundle_stats.get().0;
    assert!(parts > 0, "the quad must be drawn in a main-pass bundle");
    // The second picture records once more to remember what the bundle binds; only then
    // can a later picture keep it (as in the main-pass bundle regression).
    frame(&mut r, &mut scene);
    frame(&mut r, &mut scene);
    assert_eq!(
        r.history.bundle_stats.get(),
        (0, parts),
        "the bundle with the original table is kept"
    );

    // Add a material the draw list does not use: only the table and its bind group change.
    let grown = r.add_material(
        &mut scene,
        None,
        AlphaMode::Opaque,
        [0.0, 1.0, 0.0, 1.0],
        true,
    );
    let row = scene.materials[grown].binding.row.index;
    assert!(row > 0);
    let epoch = scene.camera_epoch;
    frame(&mut r, &mut scene);
    assert!(scene.camera_epoch > epoch);
    assert_eq!(
        r.history.bundle_stats.get(),
        (parts, 0),
        "a bundle recorded with the old table must not be reused after growth"
    );
    // As after the first picture, the new group's resources are remembered on the next
    // picture before a following one can keep the bundle.
    frame(&mut r, &mut scene);
    frame(&mut r, &mut scene);
    assert_eq!(
        r.history.bundle_stats.get(),
        (0, parts),
        "the bundle recorded after growth is kept"
    );

    // Now draw with the new row and keep that bundle for another picture.
    r.set_material(&mut scene, 0, 0, grown);
    frame(&mut r, &mut scene);
    frame(&mut r, &mut scene);
    frame(&mut r, &mut scene);
    assert_eq!(
        r.history.bundle_stats.get(),
        (0, parts),
        "a bundle recorded after growth, with a nonzero row, is kept"
    );
    assert_eq!(r.device.bindless_buffers_in_use(), 0);
}

#[test]
fn material_table_row_stride_respects_uniform_alignment() {
    assert_eq!(MaterialTable::row_stride_for(64), MATERIAL_ROW_BYTES);
    assert_eq!(MaterialTable::row_stride_for(256), MATERIAL_ROW_BYTES);
    assert_eq!(MaterialTable::row_stride_for(512), 512);
    assert_eq!(MaterialTable::row_stride_for(1024), 1024);

    let r = renderer();
    let table = MaterialTable::new(&r.device, 2);
    let min_align = r.device.limits().min_uniform_buffer_offset_alignment as u64;
    assert!(table.stride >= min_align.max(MATERIAL_ROW_BYTES));
    assert_eq!(table.stride % min_align.max(1), 0);
    assert_eq!(table.offset(3), 3 * table.stride);
}
