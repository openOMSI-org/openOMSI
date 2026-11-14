use super::*;

type Resources = (wgpu::Buffer, wgpu::Buffer, i32, u32, wgpu::BindGroup);

pub(super) struct Recorded {
    batches: Vec<Batch>,
    resources: Vec<Resources>,
    camera: wgpu::BindGroup,
    plane: wgpu::BindGroup,
    picture: Option<wgpu::BindGroup>,
    bundle: wgpu::RenderBundle,
}

fn resources(scene: &Scene, batch: &Batch) -> Resources {
    let mesh = &scene.meshes[batch.mesh as usize];
    (mesh.vertex_buf.clone(), mesh.index_buf.clone(), mesh.base_vertex, mesh.first_index, scene.materials[batch.material as usize].bind_group.clone())
}

pub(super) fn get<'a>(cache: &'a mut Option<Recorded>, device: &wgpu::Device, scene: &'a Scene, batches: &[Batch], plane: &wgpu::BindGroup, picture: Option<&wgpu::BindGroup>, pipeline: impl Fn(u8) -> &'a wgpu::RenderPipeline) -> &'a wgpu::RenderBundle {
    let camera = scene.camera_bind_group.as_ref().unwrap();
    let keep = cache.as_ref().is_some_and(|c| c.camera == *camera && c.plane == *plane && c.picture.as_ref() == picture && c.batches == batches
        && batches.iter().zip(&c.resources).all(|(b, old)| resources(scene, b) == *old));
    if !keep {
        let mut encoder = device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
            label: Some("cabin reflection draws"), color_formats: &[Some(HDR_FORMAT)],
            depth_stencil: Some(wgpu::RenderBundleDepthStencil { format: DEPTH_FORMAT, depth_read_only: false, stencil_read_only: true }),
            sample_count: 1, multiview: None,
        });
        encoder.set_bind_group(0, camera, &[]);
        encoder.set_bind_group(2, plane, &[]);
        if let Some(picture) = picture { encoder.set_bind_group(3, picture, &[]); }
        encode_batches(&mut encoder, scene, batches, pipeline);
        *cache = Some(Recorded {
            batches: batches.to_vec(), resources: batches.iter().map(|b| resources(scene, b)).collect(),
            camera: camera.clone(), plane: plane.clone(), picture: picture.cloned(),
            bundle: encoder.finish(&wgpu::RenderBundleDescriptor { label: Some("cabin reflection draws") }),
        });
    }
    &cache.as_ref().unwrap().bundle
}
