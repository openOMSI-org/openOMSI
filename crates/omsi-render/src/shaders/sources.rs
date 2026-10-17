//! The WGSL modules as each device gets them: the parts joined, and rewritten where a
//! backend cannot take them as they are (OpenGL, no storage buffers, phone compilers).

use crate::*;

/// The scene shader: the vanilla path and the enhanced fragment shader in one module.
///
/// On OpenGL a texture has one sampler (GLSL's combined sampler2D), so there the tile
/// masks are read through `s_diffuse` at a UV clamped half a texel inside the tile, which
/// is what `s_tile`'s clamp to edge gives; reading `t_trans`/`t_night` through both
/// samplers fails the whole module ("Conflicting samplers").
pub(crate) fn scene_shader_source(gl: bool) -> String {
    scene_shader_for(gl, array_path())
}

pub(crate) fn scene_shader_for(gl: bool, path: ArrayPath) -> String {
    let src = arrays_as_textures(&scene_shader_text(gl), path);
    if gl && path.no_storage() { compares_without_gather(&src) } else { src }
}

/// The sun's shadow tent with each gather of four depth comparisons taken as four single
/// ones. OpenGL without storage buffers is GLES 3.0 or GL 3.3 (ANGLE on Direct3D 11 among
/// them), which have no textureGather: the scene pipelines failed to compile there ("no
/// matching overloaded function found") and DirectX 11 drew nothing. Each texel is read at
/// its centre, where the comparison sampler's filter takes that texel alone.
pub(crate) fn compares_without_gather(src: &str) -> String {
    let mut out = src.to_string();
    for t in ["t_shadow", "t_shadow_far"] {
        let at = |x: f32, y: f32| format!("textureSampleCompareLevel({t}, s_shadow, a + vec2<f32>({x:?}, {y:?}) / dims, zr)");
        let gather = format!("textureGatherCompare({t}, s_shadow, a, zr)");
        assert!(out.contains(&gather), "scene shader: {gather} not found");
        // (in the gather's order: (0, 1), (1, 1), (1, 0), (0, 0) of the quad around `a`)
        let four = format!("vec4<f32>({}, {}, {}, {})", at(-0.5, 0.5), at(0.5, 0.5), at(0.5, -0.5), at(-0.5, -0.5));
        out = out.replace(&gather, &four);
    }
    out
}

/// The scene module with its arrays read as `path` has them (see `ArrayPath`): each
/// storage array the device cannot read becomes a texture and its `name[i]` a function
/// that loads texel `i`; without storage at all the point lights are textures or none.
pub(crate) fn arrays_as_textures(src: &str, path: ArrayPath) -> String {
    if path == ArrayPath::Storage {
        return src.to_string();
    }
    let w = ARRAY_TEX_WIDTH;
    let mut out = src.to_string();
    let mut swap = |decl: &str, with: String, name: &str, call: &str| {
        assert!(out.contains(decl), "scene shader: {decl} not found");
        out = out.replace(decl, &with);
        out = indexing_as_calls(&out, name, call);
    };
    let load = |name: &str, ty: &str, pick: &str| {
        format!(
            "var {name}_tex: texture_2d<{ty}>;\nfn {name}_at(i: u32) -> {} {{ return textureLoad({name}_tex, vec2<u32>(i % {w}u, i / {w}u), 0){pick}; }}",
            if pick.is_empty() { format!("vec4<{ty}>") } else { ty.to_string() }
        )
    };
    swap("var<storage, read> models: array<vec4<f32>>;", load("models", "f32", ""), "models", "models_at");
    swap("var<storage, read> inst_params: array<vec4<f32>>;", load("inst_params", "f32", ""), "inst_params", "inst_params_at");
    swap("var<storage, read> draw_list: array<u32>;", load("draw_list", "u32", ".x"), "draw_list", "draw_list_at");
    if path == ArrayPath::LightTextures {
        let lights = format!(
            "var lights_tex: texture_2d<f32>;\nfn lights_texel(t: u32) -> vec4<f32> {{ return textureLoad(lights_tex, vec2<u32>(t % {w}u, t / {w}u), 0); }}\nfn lights_at(i: u32) -> PointLight {{ return PointLight(lights_texel(i * 4u), lights_texel(i * 4u + 1u), lights_texel(i * 4u + 2u), lights_texel(i * 4u + 3u)); }}"
        );
        swap("var<storage, read> lights: array<PointLight>;", lights, "lights", "lights_at");
        swap("var<storage, read> grid: array<u32>;", load("grid", "u32", ".x"), "grid", "grid_at");
    }
    if path == ArrayPath::NoStorage {
        swap(
            "@group(0) @binding(3) var<storage, read> lights: array<PointLight>;",
            "fn lights_at(i: u32) -> PointLight { return PointLight(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0)); }".to_string(),
            "lights",
            "lights_at",
        );
        swap("@group(0) @binding(4) var<storage, read> grid: array<u32>;", "fn grid_at(i: u32) -> u32 { return 0xffffffffu; }".to_string(), "grid", "grid_at");
    }
    out
}

/// `name[expr]` (the whole word `name`) turned into `call(expr)`.
pub(crate) fn indexing_as_calls(src: &str, name: &str, call: &str) -> String {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let pat = format!("{name}[");
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(at) = rest.find(&pat) {
        let before = rest[..at].chars().next_back();
        out.push_str(&rest[..at]);
        if before.is_some_and(word) {
            out.push_str(&pat);
            rest = &rest[at + pat.len()..];
            continue;
        }
        // the matching bracket
        let inner = &rest[at + pat.len()..];
        let mut depth = 1;
        let end = inner
            .char_indices()
            .find(|&(_, c)| {
                depth += match c {
                    '[' => 1,
                    ']' => -1,
                    _ => 0,
                };
                depth == 0
            })
            .map(|(i, _)| i)
            .expect("unbalanced brackets in the scene shader");
        out.push_str(call);
        out.push('(');
        out.push_str(&indexing_as_calls(&inner[..end], name, call));
        out.push(')');
        rest = &inner[end + 1..];
    }
    out.push_str(rest);
    out
}

pub(crate) fn scene_shader_text(gl: bool) -> String {
    let src = [
        include_str!("../colour.wgsl"),
        shaders::SCENE,
        include_str!("../enhanced_common.wgsl"),
        include_str!("../puddle_common.wgsl"),
        include_str!("../lamp_air.wgsl"),
        shaders::ENHANCED,
    ]
    .join("\n");
    // Enhanced+: the enhanced pass writes the reflections' surfaces as well (`GBUF_FORMAT`)
    let src = if rt_gbuf() { src.replace("//RT ", "") } else { src };
    let src = if basic_pipelines() { lean_scene(src) } else { src };
    if !gl {
        return src;
    }
    let clamped = |t: &str| {
        format!(
            "textureSample({t}, s_diffuse, clamp(uv, 0.5 / vec2<f32>(textureDimensions({t})), \
             vec2<f32>(1.0) - 0.5 / vec2<f32>(textureDimensions({t}))))"
        )
    };
    // No SSAO and no ray tracing on OpenGL (camera.clouds.w stays 0, see `lighter_picture`):
    // their lookups are left out of the module. With them in the scene fragment shader, the
    // D3D compiler under ANGLE (fxc) recursed without end in CProgram::CheckAssertion_Group
    // and the game closed before its first frame (#197).
    let out = src
        .replace("textureSample(t_trans, s_tile, uv)", &clamped("t_trans"))
        .replace("textureSample(t_night, s_tile, uv)", &clamped("t_night"))
        .replace("ao = ao_at(in.clip.xy, in.world);", "ao = 1.0;")
        .replace("rt_at(in.clip.xy, in.world)", "vec4<f32>(1.0, 0.0, -1.0, 0.0)");
    debug_assert!(!out.contains("s_tile, uv)"));
    debug_assert!(!out.contains("ao_at(in.") && !out.contains("rt_at(in."));
    out
}

/// The scene shader without the parts 0.2.0 added to every pixel's lighting that a phone's
/// shader compiler gives up on (`basic_pipelines`): the street lamps' shadow maps read in
/// the loop over the lamps, and the sun's soft shadow taken a second and a third time for
/// the moon and a debug view. Adreno 740/830 (Galaxy S23-S25) failed the scene pipelines
/// with these inlined into the enhanced fragment shader, and every frame stopped on an
/// invalid 'omsi' pipeline (#1633, #1663, #1708).
pub(crate) fn lean_scene(src: String) -> String {
    let out = src
        .replace("irr = irr * lamp_shadow_at(li, p, n, thin);", "")
        .replace("ms = sun_shadow_soft(in.world, n, thin);", "ms = 1.0;")
        .replace("let sm = sun_shadow_soft(in.world, n, thin);", "let sm = 1.0;");
    debug_assert_eq!(out.matches("sun_shadow_soft(in.world").count(), 1);
    debug_assert!(!out.contains("* lamp_shadow_at("));
    out
}

/// The sky dome (both paths) and the enhanced reflection probe.
pub(crate) fn sky_shader_source() -> String {
    [
        include_str!("../colour.wgsl"),
        include_str!("../sky.wgsl"),
        include_str!("../enhanced_common.wgsl"),
        include_str!("../sky_enhanced.wgsl"),
    ]
    .join("\n")
}

/// The snowfall (snow.wgsl), with the lamps the enhanced picture lights it by (none
/// without storage buffers).
pub(crate) fn snow_shader_source() -> String {
    precip_shader_source(include_str!("../snow.wgsl"))
}

/// The rainfall (rain.wgsl), as the snowfall.
pub(crate) fn rain_shader_source() -> String {
    let src = precip_shader_source(include_str!("../rain.wgsl"));
    if basic_pipelines() { lean_rain(src) } else { src }
}

pub(crate) fn lean_rain(src: String) -> String {
    const LAMPS: &str = "precip_light(in.pos, to_eye, false) * 0.3 + ";
    debug_assert_eq!(src.matches(LAMPS).count(), 1);
    src.replace(LAMPS, "")
}

pub(crate) fn precip_shader_source(own: &str) -> String {
    let src = [
        include_str!("../precip_common.wgsl"),
        own,
        include_str!("../enhanced_common.wgsl"),
        FOG_LAMP_LIGHTS,
        include_str!("../lamp_air.wgsl"),
    ]
    .join("\n");
    if array_path().no_storage() {
        let src = src
            .replace("@group(0) @binding(3) var<storage, read> lights: array<PointLight>;", "fn lights_at(i: u32) -> PointLight { return PointLight(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0)); }")
            .replace("@group(0) @binding(4) var<storage, read> grid: array<u32>;", "fn grid_at(i: u32) -> u32 { return 0xffffffffu; }");
        return indexing_as_calls(&indexing_as_calls(&src, "lights", "lights_at"), "grid", "grid_at");
    }
    src
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct SnowUniform {
    pub(crate) wind: [f32; 4],
    pub(crate) fall: [f32; 4],
    /// The roof map's matrix, and x 1 when there is one (`passes::PrecipMap::uniform`).
    pub(crate) roof: [[f32; 4]; 4],
    pub(crate) roof_on: [f32; 4],
}

/// The snowfall's flakes for a snowfall of strength `s` (0..1): how many each layer of
/// snow.wgsl holds (the near one at the density of a heavy fall, some 190 a cubic metre,
/// scaled down for a lighter one) and their mean diameter (m) - a light fall single
/// crystals of a millimetre, a heavy one aggregates of several.
pub(crate) fn snowfall_flakes(s: f32) -> ([u32; 3], f32) {
    let k = 0.08 + 0.92 * s.clamp(0.0, 1.0);
    ([(70000.0 * k) as u32, (55000.0 * k) as u32, (25000.0 * k) as u32], 0.0006 + 0.0034 * s * s)
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct RainUniform {
    pub(crate) wind: [f32; 4],
    pub(crate) eye: [f32; 4],
    pub(crate) fall: [f32; 4],
    pub(crate) view: [f32; 4],
    pub(crate) roof: [[f32; 4]; 4],
    pub(crate) roof_on: [f32; 4],
}

/// How many drops each layer of rain.wgsl holds in a rain of strength `r` (0..1) at the
/// rain's quality (0 low, 1 medium, 2 high): the near one some 90 a cubic metre in a
/// downpour - of the drops big enough to see, a few hundred a cubic metre in heavy rain -
/// and a tenth of that in a drizzle.
/// The fourth figure is the splashes where they land (see rain.wgsl `splash`): some forty
/// on each square metre at a time in a downpour, close by.
pub(crate) fn rainfall_drops(r: f32, quality: u8) -> [u32; 4] {
    let k = (0.12 + 0.88 * r.clamp(0.0, 1.0)) * [0.3, 0.6, 1.0][quality.min(2) as usize];
    [(90000.0 * k) as u32, (110000.0 * k) as u32, (60000.0 * k) as u32, (25000.0 * k) as u32]
}

/// Enhanced: the lamps' light in the fog over the drawn picture (`fog_lamps.wgsl`; not
/// without storage buffers, where there are no lamps).
pub(crate) fn fog_lamps_shader_source() -> String {
    [
        include_str!("../fog_lamps.wgsl"),
        include_str!("../enhanced_common.wgsl"),
        FOG_LAMP_LIGHTS,
        include_str!("../lamp_air.wgsl"),
    ]
    .join("\n")
}

/// The scene's point lights and their grid as the fog pass reads them (`lamp_air.wgsl`):
/// the scene shader's declarations.
pub(crate) const FOG_LAMP_LIGHTS: &str = "struct PointLight {
    pos: vec4<f32>,
    color: vec4<f32>,
    dir: vec4<f32>,
    extra: vec4<f32>,
};
@group(0) @binding(3) var<storage, read> lights: array<PointLight>;
@group(0) @binding(4) var<storage, read> grid: array<u32>;
const CELL_CAP: u32 = 32u;
";

/// The light coronas (both paths).
pub(crate) fn corona_shader_source() -> String {
    let src = [
        include_str!("../corona.wgsl"),
        include_str!("../enhanced_common.wgsl"),
        FOG_LAMP_LIGHTS,
        include_str!("../lamp_air.wgsl"),
    ]
    .join("\n");
    // (the basic set lights rain by the sky alone: the loop over the lamps is one more
    // thing a phone's compiler can fail on, see `lean_scene`)
    let src = if basic_pipelines() {
        src.replace("let l = precip_light(in.wpos.xyz, to_eye, in.wpos.w > 1.5);", "let l = sh_irradiance(vec3<f32>(0.0, 0.0, 1.0)) / PI;")
    } else {
        src
    };
    // (without storage buffers the precipitation has no lamps to be lit by)
    if array_path().no_storage() {
        let src = src
            .replace("@group(0) @binding(3) var<storage, read> lights: array<PointLight>;", "fn lights_at(i: u32) -> PointLight { return PointLight(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0)); }")
            .replace("@group(0) @binding(4) var<storage, read> grid: array<u32>;", "fn grid_at(i: u32) -> u32 { return 0xffffffffu; }");
        return indexing_as_calls(&indexing_as_calls(&src, "lights", "lights_at"), "grid", "grid_at");
    }
    src
}
