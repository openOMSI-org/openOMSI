struct CabinReflection {
    planes: array<vec4<f32>, 2>,
    screen: vec4<f32>, // picture width/height, this plane, number of planes
};
@group(3) @binding(1) var t_cabin_reflection: texture_2d<f32>;
@group(3) @binding(2) var s_cabin_reflection: sampler;
@group(3) @binding(3) var<uniform> cabin_reflection: CabinReflection;

fn cabin_plane_error(world: vec3<f32>, normal: vec3<f32>, plane: vec4<f32>) -> f32 {
    return abs(dot(plane.xyz, world) - plane.w) + 4.0 * (1.0 - abs(dot(normal, plane.xyz)));
}

fn cabin_plane_matches(world: vec3<f32>, normal: vec3<f32>) -> bool {
    return cabin_plane_error(world, normal, camera.cabin_planes[0]) < 0.22
        || cabin_plane_error(world, normal, camera.cabin_planes[1]) < 0.22;
}

fn cabin_plane_index(world: vec3<f32>, normal: vec3<f32>) -> i32 {
    let a = cabin_plane_error(world, normal, camera.cabin_planes[0]);
    let b = cabin_plane_error(world, normal, camera.cabin_planes[1]);
    if (min(a, b) >= 0.22) { return -1; }
    return select(0, 1, b < a);
}

@fragment
fn fs_cabin_reflection(in: FsIn) -> @location(0) vec4<f32> {
    // Select one plane to avoid overlapping reflected images.
    let n = safe_normal(in.normal);
    let best = cabin_plane_index(in.world, n);
    if (best < 0) { discard; }
    let uv = tex_address(in.uv);
    var coverage = diffuse_border(textureSample(t_diffuse, s_diffuse, uv), uv).a;
    if (material.params.z > 0.5) {
        coverage = select(1.0, sample_transmap(tex_address(in.uv - in.params.zw)).a, material.params.w > 0.5);
    }
    coverage = smoothstep(0.0, 0.05, coverage * material.color.a);
    // Envir_Brightness hides the daytime environment map, not the glass.
    coverage = coverage * select(clamp(in.params.x, 0.0, 1.0), 1.0, material.glow.z > 0.5);
    if (coverage <= 0.0) { discard; }
    let v = normalize(camera.cam_pos.xyz - in.world);
    let fresnel = 0.04 + 0.96 * pow(1.0 - abs(dot(n, v)), 5.0);
    let size = vec2<f32>(textureDimensions(t_cabin_reflection));
    let height = size.y / cabin_reflection.screen.w;
    let local = clamp(in.clip.xy / cabin_reflection.screen.xy, vec2<f32>(0.5) / vec2<f32>(size.x, height), vec2<f32>(1.0) - vec2<f32>(0.5) / vec2<f32>(size.x, height));
    let atlas_uv = vec2<f32>(local.x, (local.y + f32(best)) / cabin_reflection.screen.w);
    let reflected = textureSampleLevel(t_cabin_reflection, s_cabin_reflection, atlas_uv, 0.0);
    // The capture already includes exposure.
    return vec4<f32>(reflected.rgb * 1.15 * fresnel * coverage * camera.cam_pos.w, 0.0);
}

@vertex
fn vs_cabin_resolve(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(p[i], 0.0, 1.0);
}

@fragment
fn fs_cabin_resolve(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(t_cabin_reflection, vec2<i32>(p.xy), 0);
}
