// An actual view of the cabin mirrored in the pane, including geometry behind the
// driver. The capture uses the same materials and interior lamps as the main view.
struct CabinReflection {
    planes: array<vec4<f32>, 2>,
    screen: vec4<f32>, // picture width/height, this plane, number of planes
};
@group(3) @binding(1) var t_cabin_reflection: texture_2d<f32>;
@group(3) @binding(2) var s_cabin_reflection: sampler;
@group(3) @binding(3) var<uniform> cabin_reflection: CabinReflection;

fn cabin_plane_index(world: vec3<f32>, normal: vec3<f32>) -> i32 {
    var best = -1;
    var error = 0.22;
    let n = safe_normal(normal);
    for (var i = 0; i < 2; i = i + 1) {
        let p = camera.cabin_planes[i];
        let e = abs(dot(p.xyz, world) - p.w) + 4.0 * (1.0 - abs(dot(n, p.xyz)));
        if (e < error) { error = e; best = i; }
    }
    return best;
}

@fragment
fn fs_cabin_reflection(in: FsIn) -> @location(0) vec4<f32> {
    // A material range can span several panes. Each pixel belongs to its nearest
    // plane only, so split windscreens and coincident inner/outer faces do not add
    // two pictures on top of each other.
    let best = cabin_plane_index(in.world, in.normal);
    let n = safe_normal(in.normal);
    if (best != i32(cabin_reflection.screen.z)) { discard; }
    let uv = tex_address(in.uv);
    var coverage = diffuse_border(textureSample(t_diffuse, s_diffuse, uv), uv).a;
    if (material.params.z > 0.5) {
        coverage = select(1.0, sample_transmap(tex_address(in.uv - in.params.zw)).a, material.params.w > 0.5);
    }
    coverage = smoothstep(0.0, 0.05, coverage * material.color.a);
    // Envir_Brightness fades the authored daytime environment map, not the glass
    // itself. Other script alpha values still hide their pane normally.
    coverage = coverage * select(clamp(in.params.x, 0.0, 1.0), 1.0, material.glow.z > 0.5);
    if (coverage <= 0.0) { discard; }
    let v = normalize(camera.cam_pos.xyz - in.world);
    let fresnel = 0.04 + 0.96 * pow(1.0 - abs(dot(n, v)), 5.0);
    let reflected = textureSampleLevel(t_cabin_reflection, s_cabin_reflection, in.clip.xy / cabin_reflection.screen.xy, 0.0);
    // The HDR capture is already exposed. Add its reflected light before tone
    // mapping, without an opaque tint or a second copy of the street/sky.
    return vec4<f32>(reflected.rgb * fresnel * coverage * camera.cam_pos.w, 0.0);
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
