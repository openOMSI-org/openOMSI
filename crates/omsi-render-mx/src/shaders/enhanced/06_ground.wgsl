
// The painted ground's albedo (`Renderer::encode_ground`): the terrain's base and its paint
// layers laid over each other into one picture of what the surface is made of - texture,
// detail and tint, blended by each layer's coverage - which the main pass then lights once
// (`from_ground` in `shade_enhanced`) instead of lighting every layer in full and blending
// the lit colours. The base writes it, the layers blend into it (the pipeline's blend: the
// coverage over what is there; the alpha channel is left as the base wrote it).
@fragment
fn fs_ground(in: FsIn) -> @location(0) vec4<f32> {
    let duv = in.uv * material.extra.z;
    let buv = tex_address(in.uv - in.params.zw);
    let mode = material.params.x;
    // (a brush mask that covers nothing here: the layer's texture need not be read)
    if (TERRAIN_PAINT && mode > 1.5 && material.params.z > 0.5 && material.params.w > 0.5) {
        let cover = smoothstep(0.32, 0.68, sample_transmap(buv).a);
        if (cover * material.color.a * in.params.x == 0.0) {
            discard;
        }
    }
    var tex = diffuse_border(textureSampleBias(t_diffuse, s_diffuse, duv, camera.lod.x), duv);
    if (material.extra.y > 0.0) {
        let det = textureSampleBias(t_light, s_diffuse, in.uv * material.extra.y, camera.lod.x);
        tex = vec4<f32>(clamp(tex.rgb * det.rgb, vec3<f32>(0.0), vec3<f32>(1.0)), tex.a);
    }
    var cover = tex.a;
    if (material.params.z > 0.5) {
        cover = select(1.0, sample_transmap(buv).a, material.params.w > 0.5);
        if (mode > 1.5) {
            cover = smoothstep(0.32, 0.68, cover);
        }
    }
    var alpha = cover * material.color.a;
    if (mode < 0.5) {
        alpha = 1.0;
    }
    alpha = alpha * clamp(in.params.x, 0.0, 1.0);
    if (TERRAIN_PAINT && alpha == 0.0) {
        discard;
    }
    return vec4<f32>(tex.rgb * material.color.rgb, alpha);
}
