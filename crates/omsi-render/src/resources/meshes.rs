//! Meshes on the GPU: their vertex and index buffers, the shared pages they are placed in,
//! uploads prepared off the main thread, and changed vertices written at the next frame.

use crate::*;

pub struct GpuMesh {
    pub(crate) vertex_buf: wgpu::Buffer,
    pub(crate) index_buf: wgpu::Buffer,
    /// Where the mesh lies in its page's buffers (pages hold many meshes); `page` is its
    /// page in the scene's list, `u32::MAX` for a page of its own.
    pub(crate) page: u32,
    pub(crate) base_vertex: i32,
    pub(crate) first_index: u32,
    pub(crate) vertex_offset: u64,
    pub(crate) vertex_bytes: u64,
    pub(crate) index_bytes: u64,
    /// Which mesh this is (a page's range handed to another mesh is another): what the ray
    /// tracer's structures are kept by, with the mesh's place.
    pub(crate) generation: u64,
    /// Counts the vertices' rewrites (`update_mesh`, a skinned human's poses): a street
    /// lamp's shadow tile kept from an earlier frame is drawn anew when it changes.
    pub(crate) pose: u32,
    pub ranges: Vec<(u32, u32, u32)>,
    pub bounds_center: Vec3,
    pub bounds_radius: f32,
    /// Back faces are culled (a content mesh, see `MeshData::one_sided`).
    pub one_sided: bool,
    /// Source asset for the optional draw-cost audit.
    pub source: Option<String>,
    /// Its flat surfaces by material slot, for the draw order's depth step of a surface
    /// lying on another of its object (`scene::overlay`).
    pub(crate) flats: Vec<crate::scene::overlay::Flat>,
}

/// A buffer holding `contents` - what `create_buffer_init` makes, but written through the
/// queue instead of mapped at its creation. When the device refuses the memory (an
/// integrated chip that shares a small heap: "Out of Memory"), the buffer is invalid, and
/// mapping it ended the game - "Error in Buffer::get_mapped_range: Validation Error"
/// (#107, #109) - where writing to it is an error that is logged and the game goes on from.
pub(crate) fn buffer_init(device: &wgpu::Device, queue: &wgpu::Queue, label: Option<&str>, contents: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
    let align = wgpu::COPY_BUFFER_ALIGNMENT as usize;
    let size = contents.len().next_multiple_of(align).max(align);
    let buf = device.create_buffer(&wgpu::BufferDescriptor { label, size: size as u64, usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    if !contents.is_empty() {
        if contents.len() == size {
            queue.write_buffer(&buf, 0, contents);
        } else {
            let mut padded = contents.to_vec();
            padded.resize(size, 0);
            queue.write_buffer(&buf, 0, &padded);
        }
    }
    buf
}

/// Meshes are made for the ray tracer's acceleration structures as well (Enhanced+ on a
/// device with ray queries: their buffers are its geometry input).
pub(crate) static RT_BUFFERS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Is `device` wgpu's no-op device (a dedicated server, which draws nothing)? Meshes take
/// their place in the pages there, but their bytes are not written: that device keeps every
/// buffer in memory, a second copy of every mesh of the map that nothing ever reads (about
/// 430 MB of a dedicated server on Brasil Viagem Xtreme).
pub(crate) fn no_device(device: &wgpu::Device) -> bool {
    device.adapter_info().backend == wgpu::Backend::Noop
}

/// Vertex and index buffers holding several meshes; the space a freed mesh leaves is
/// handed to the next that fits.
pub(crate) struct MeshPage {
    pub(crate) vertex: wgpu::Buffer,
    pub(crate) index: wgpu::Buffer,
    /// The pages' room (their buffers' sizes, but on no device: those are a token there).
    pub(crate) vertex_cap: u64,
    pub(crate) index_cap: u64,
    /// On a real device: the meshes' bytes are written (see [`no_device`]).
    pub(crate) real: bool,
    pub(crate) vertex_top: u64,
    pub(crate) index_top: u64,
    pub(crate) vertex_free: Vec<(u64, u64)>,
    pub(crate) index_free: Vec<(u64, u64)>,
}

pub(crate) const MESH_PAGE_VERTEX_BYTES: u64 = 32 << 20;
pub(crate) const MESH_PAGE_INDEX_BYTES: u64 = 16 << 20;

/// First fit of `len` bytes in `free` (offset, length) or at `top` under `cap`.
pub(crate) fn page_take(free: &mut Vec<(u64, u64)>, top: &mut u64, cap: u64, len: u64) -> Option<u64> {
    if len == 0 {
        return Some(0);
    }
    if let Some(k) = free.iter().position(|&(_, l)| l >= len) {
        let (o, l) = free[k];
        if l == len {
            free.remove(k);
        } else {
            free[k] = (o + len, l - len);
        }
        return Some(o);
    }
    (*top + len <= cap).then(|| {
        *top += len;
        *top - len
    })
}

/// Give `len` bytes at `at` back, joined to the free space beside them.
pub(crate) fn page_give(free: &mut Vec<(u64, u64)>, top: &mut u64, at: u64, len: u64) {
    if len == 0 {
        return;
    }
    let k = free.partition_point(|&(o, _)| o < at);
    free.insert(k, (at, len));
    if k + 1 < free.len() && free[k].0 + free[k].1 == free[k + 1].0 {
        free[k].1 += free[k + 1].1;
        free.remove(k + 1);
    }
    if k > 0 && free[k - 1].0 + free[k - 1].1 == free[k].0 {
        free[k - 1].1 += free[k].1;
        free.remove(k);
    }
    if let Some(&(o, l)) = free.last() {
        if o + l == *top {
            *top = o;
            free.pop();
        }
    }
}

pub(crate) static MESH_OUT_OF_MEMORY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn try_create_buffer(device: &wgpu::Device, desc: &wgpu::BufferDescriptor) -> Option<wgpu::Buffer> {
    let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let buffer = device.create_buffer(desc);
    let invalid = pollster::block_on(validation.pop());
    let out_of_memory = pollster::block_on(memory.pop());
    if out_of_memory.is_some() {
        MESH_OUT_OF_MEMORY.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let Some(e) = out_of_memory.or(invalid) else {
        return Some(buffer);
    };
    static FAILED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = FAILED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if n < 5 || n.is_multiple_of(1000) {
        log::warn!("mesh buffer of {:.1} MB could not be made, its meshes go into smaller ones or are left out (#{}): {}", desc.size as f64 / 1e6, n + 1, crate::gpu_error_text(&e));
    }
    None
}

#[cfg(test)]
thread_local! {
    pub(crate) static REFUSE_MESH_PAGES_OVER: std::cell::Cell<u64> = const { std::cell::Cell::new(u64::MAX) };
}

fn page_refused(real: bool, bytes: u64) -> bool {
    let _ = (real, bytes);
    #[cfg(test)]
    if bytes > REFUSE_MESH_PAGES_OVER.with(std::cell::Cell::get) {
        return true;
    }
    #[cfg(feature = "test-hooks")]
    if real && omsi_cfg::flags::OMSI_FAKE_GPU_ERROR.var() == Some("mesh") {
        return true;
    }
    false
}

impl MeshPage {
    pub(crate) fn new(device: &wgpu::Device, vertex_bytes: u64, index_bytes: u64) -> Option<MeshPage> {
        let blas = if RT_BUFFERS.load(std::sync::atomic::Ordering::Relaxed) { wgpu::BufferUsages::BLAS_INPUT | wgpu::BufferUsages::STORAGE } else { wgpu::BufferUsages::empty() };
        let real = !no_device(device);
        let size = |bytes: u64| if page_refused(real, bytes) { device.limits().max_buffer_size + wgpu::COPY_BUFFER_ALIGNMENT } else if real { bytes.max(32) } else { 32 };
        let buffer = |bytes: u64, usage| try_create_buffer(device, &wgpu::BufferDescriptor { label: Some("mesh page"), size: size(bytes), usage: usage | wgpu::BufferUsages::COPY_DST | blas, mapped_at_creation: false });
        Some(MeshPage {
            vertex: buffer(vertex_bytes, wgpu::BufferUsages::VERTEX)?,
            index: buffer(index_bytes, wgpu::BufferUsages::INDEX)?,
            vertex_cap: vertex_bytes.max(32),
            index_cap: index_bytes.max(32),
            real,
            vertex_top: 0,
            index_top: 0,
            vertex_free: Vec::new(),
            index_free: Vec::new(),
        })
    }

    /// Room for a mesh of these sizes: its vertex and index offsets.
    pub(crate) fn take(&mut self, vertex_bytes: u64, index_bytes: u64) -> Option<(u64, u64)> {
        let v = page_take(&mut self.vertex_free, &mut self.vertex_top, self.vertex_cap, vertex_bytes)?;
        match page_take(&mut self.index_free, &mut self.index_top, self.index_cap, index_bytes) {
            Some(i) => Some((v, i)),
            None => {
                page_give(&mut self.vertex_free, &mut self.vertex_top, v, vertex_bytes);
                None
            }
        }
    }

    pub(crate) fn give(&mut self, m: &GpuMesh) {
        page_give(&mut self.vertex_free, &mut self.vertex_top, m.vertex_offset, m.vertex_bytes);
        page_give(&mut self.index_free, &mut self.index_top, m.first_index as u64 * 4, m.index_bytes);
    }

    /// Write the mesh at these offsets.
    pub(crate) fn place(&self, queue: &wgpu::Queue, data: &MeshData, page: u32, at: (u64, u64)) -> GpuMesh {
        let verts = mesh_vertices(data);
        let (vb, ib): (&[u8], &[u8]) = (bytemuck::cast_slice(&verts), bytemuck::cast_slice(&data.indices));
        if !vb.is_empty() && self.real {
            queue.write_buffer(&self.vertex, at.0, vb);
        }
        if !ib.is_empty() && self.real {
            queue.write_buffer(&self.index, at.1, ib);
        }
        let (center, radius) = mesh_bounds(data);
        GpuMesh {
            vertex_buf: self.vertex.clone(),
            index_buf: self.index.clone(),
            page,
            base_vertex: (at.0 / std::mem::size_of::<Vertex>() as u64) as i32,
            first_index: (at.1 / 4) as u32,
            vertex_offset: at.0,
            vertex_bytes: vb.len() as u64,
            index_bytes: ib.len() as u64,
            generation: next_gen(),
            pose: 0,
            ranges: data.ranges.clone(),
            bounds_center: center,
            bounds_radius: radius,
            one_sided: data.one_sided,
            source: None,
            flats: crate::scene::overlay::mesh_flats(data),
        }
    }
}

impl GpuMesh {
    pub(crate) fn left_out(vertex_buf: wgpu::Buffer, index_buf: wgpu::Buffer) -> GpuMesh {
        GpuMesh {
            vertex_buf,
            index_buf,
            page: u32::MAX,
            base_vertex: 0,
            first_index: 0,
            vertex_offset: 0,
            vertex_bytes: 0,
            index_bytes: 0,
            generation: next_gen(),
            pose: 0,
            ranges: Vec::new(),
            bounds_center: Vec3::ZERO,
            bounds_radius: 0.0,
            one_sided: false,
            source: None,
            flats: Vec::new(),
        }
    }
}

pub(crate) fn mesh_vertices(data: &MeshData) -> Vec<Vertex> {
    data.positions
        .iter()
        .zip(&data.normals)
        .zip(&data.uvs)
        .map(|((p, n), uv)| Vertex {
            pos: p.to_array(),
            normal: n.to_array(),
            uv: uv.to_array(),
        })
        .collect()
}

pub(crate) fn mesh_page_bytes(data: &MeshData) -> (u64, u64) {
    let v = data.positions.len().min(data.normals.len()).min(data.uvs.len());
    ((v * std::mem::size_of::<Vertex>()) as u64, (data.indices.len() * 4) as u64)
}

pub(crate) fn mesh_bounds(data: &MeshData) -> (Vec3, f32) {
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in &data.positions {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    if data.positions.is_empty() {
        lo = Vec3::ZERO;
        hi = Vec3::ZERO;
    }
    let center = (lo + hi) * 0.5;
    (center, (hi - center).length())
}

/// The meshes in one page of their own, sized to them (each in a page of its own where the
/// adapter cannot draw with a base vertex: `paged` false).
pub(crate) fn make_meshes(device: &wgpu::Device, queue: &wgpu::Queue, data: &[&MeshData], paged: bool) -> Vec<GpuMesh> {
    if data.is_empty() {
        return Vec::new();
    }
    if !paged {
        return data.iter().map(|d| make_meshes(device, queue, &[d], true).pop().expect("one mesh")).collect();
    }
    let sizes: Vec<(u64, u64)> = data.iter().map(|d| mesh_page_bytes(d)).collect();
    let Some(page) = MeshPage::new(device, sizes.iter().map(|s| s.0).sum(), sizes.iter().map(|s| s.1).sum()) else {
        if data.len() > 1 {
            return data.iter().map(|d| make_meshes(device, queue, &[d], true).pop().expect("one mesh")).collect();
        }
        let stand_in = device.create_buffer(&wgpu::BufferDescriptor { label: Some("left out mesh"), size: 32, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::INDEX, mapped_at_creation: false });
        return vec![GpuMesh::left_out(stand_in.clone(), stand_in)];
    };
    let mut at = (0, 0);
    data.iter().zip(&sizes).map(|(d, s)| {
        let m = page.place(queue, d, u32::MAX, at);
        at = (at.0 + s.0, at.1 + s.1);
        m
    }).collect()
}

/// A mesh on the GPU, made on a worker thread; [`Renderer::add_prepared_mesh`] puts it into
/// a scene.
pub struct PreparedMesh(GpuMesh);

/// Make a mesh's GPU buffers on any thread (the device takes calls from all of them).
pub fn prepare_mesh(device: &wgpu::Device, queue: &wgpu::Queue, data: &MeshData) -> PreparedMesh {
    prepare_meshes(device, queue, &[data], false).pop().expect("one mesh")
}

/// [`prepare_mesh`] for several meshes at once, which share one page of buffers.
/// `paged`: [`Renderer::mesh_pages`] of the renderer the meshes are for.
pub fn prepare_meshes(device: &wgpu::Device, queue: &wgpu::Queue, data: &[&MeshData], paged: bool) -> Vec<PreparedMesh> {
    let _turn = gl_worker_turn();
    make_meshes(device, queue, data, paged).into_iter().map(PreparedMesh).collect()
}

impl Renderer {
    /// Draw instance `instance` with mesh `mesh` from now on (a vehicle's skinned mesh gets
    /// a copy of its own).
    pub fn set_instance_mesh(&self, scene: &mut Scene, instance: usize, mesh: MeshId) {
        if scene.instances[instance].mesh != mesh {
            scene.instances[instance].mesh = mesh;
            scene.bounds_users_stale = true;
            Self::mark_changed(scene, instance);
        }
    }

    /// Overwrite the vertices of a mesh (skinned humans); the vertex count must not grow.
    pub fn update_mesh(
        &self,
        scene: &mut Scene,
        id: MeshId,
        positions: &[Vec3],
        normals: &[Vec3],
        uvs: &[glam::Vec2],
    ) {
        let verts: Vec<Vertex> = positions
            .iter()
            .zip(normals)
            .zip(uvs)
            .map(|((p, n), uv)| Vertex {
                pos: p.to_array(),
                normal: n.to_array(),
                uv: uv.to_array(),
            })
            .collect();
        let bytes: &[u8] = bytemuck::cast_slice(&verts);
        let m = &mut scene.meshes[id];
        if (m.vertex_bytes as usize) < bytes.len() {
            return;
        }
        m.pose = m.pose.wrapping_add(1);
        #[cfg(feature = "temporal-diagnostics")]
        if self.temporal_requested() {
            scene.temporal.record_pose(id, m.generation, m.pose, positions);
        }
        {
            // (a newer pose of the same mesh replaces one still waiting)
            let mut pending = self.pending_meshes.borrow_mut();
            match pending.iter_mut().find(|(mid, _)| *mid == id) {
                Some(e) => e.1 = bytes.to_vec(),
                None => pending.push((id, bytes.to_vec())),
            }
        }
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for p in positions {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        if !positions.is_empty() {
            m.bounds_center = (lo + hi) * 0.5;
            m.bounds_radius = (hi - m.bounds_center).length();
            Self::mesh_bounds_changed(scene, id);
        }
    }

    /// Copy the vertex data `update_mesh` collected into the meshes. Written through the
    /// queue, mesh by mesh (the writes land before the frame's commands run): one staging
    /// buffer for the whole frame outgrew the device's buffer limit on big maps, its
    /// creation failed validation and mapping it panicked (Windows, Vulkan).
    pub(crate) fn flush_pending_meshes(&self, scene: &Scene, _encoder: &mut wgpu::CommandEncoder) {
        let pending = std::mem::take(&mut *self.pending_meshes.borrow_mut());
        if pending.is_empty() || no_device(&self.device) {
            return;
        }
        if let Some(rt) = self.rt.as_ref() {
            rt.meshes_changed(pending.iter().map(|(id, _)| *id));
        }
        for (id, b) in &pending {
            let Some(m) = scene.meshes.get(*id) else { continue };
            let len = (b.len() as u64) / 4 * 4;
            if len == 0 || m.vertex_bytes < len {
                continue;
            }
            self.queue.write_buffer(&m.vertex_buf, m.vertex_offset, &b[..len as usize]);
        }
    }

    pub fn add_mesh(&self, scene: &mut Scene, data: &MeshData) -> MeshId {
        let (vb, ib) = mesh_page_bytes(data);
        let mesh = if vb > MESH_PAGE_VERTEX_BYTES / 4 || ib > MESH_PAGE_INDEX_BYTES / 4 || !self.mesh_pages {
            make_meshes(&self.device, &self.queue, &[data], true).pop().expect("one mesh")
        } else {
            let found = scene.mesh_pages.iter_mut().enumerate().find_map(|(k, p)| p.take(vb, ib).map(|at| (k, at)));
            let placed = found.or_else(|| {
                let mut page = MeshPage::new(&self.device, MESH_PAGE_VERTEX_BYTES, MESH_PAGE_INDEX_BYTES)?;
                let at = page.take(vb, ib).expect("a new page holds a mesh a quarter its size");
                scene.mesh_pages.push(page);
                Some((scene.mesh_pages.len() - 1, at))
            });
            match placed {
                Some((k, at)) => scene.mesh_pages[k].place(&self.queue, data, k as u32, at),
                None => make_meshes(&self.device, &self.queue, &[data], true).pop().expect("one mesh"),
            }
        };
        scene.meshes.push(mesh);
        scene.meshes.len() - 1
    }

    /// Whether this renderer's adapter draws meshes from shared pages (it can draw with a
    /// base vertex); [`prepare_meshes`] is told it.
    pub fn mesh_pages(&self) -> bool {
        self.mesh_pages
    }

    /// Put a mesh made on another thread ([`prepare_mesh`]) into the scene.
    pub fn add_prepared_mesh(&self, scene: &mut Scene, mesh: PreparedMesh) -> MeshId {
        scene.meshes.push(mesh.0);
        scene.meshes.len() - 1
    }
}
