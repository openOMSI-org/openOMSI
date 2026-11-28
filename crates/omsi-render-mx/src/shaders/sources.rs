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

/// The scene shader with `extra` after it (a module of its own that reads the scene's
/// material, as `temporal/motion.wgsl`): the material read from the heap in both parts.
pub(crate) fn scene_shader_with(gl: bool, extra: &str) -> String {
    let joined = format!(
        "{}\n{extra}",
        scene_shader_bound_forms(
            gl,
            AMD_VULKAN.load(std::sync::atomic::Ordering::Relaxed),
            MULTISAMPLED_SHADING.load(std::sync::atomic::Ordering::Relaxed),
        )
    );
    let path = array_path();
    let src = arrays_as_textures(&materials_from_heap(&joined), path);
    frame_from_heap(&if gl && path.no_storage() {
        compares_without_gather(&src)
    } else {
        src
    })
}

pub(crate) fn scene_shader_for(gl: bool, path: ArrayPath) -> String {
    let src = arrays_as_textures(&scene_shader_text(gl), path);
    frame_from_heap(&if gl && path.no_storage() {
        compares_without_gather(&src)
    } else {
        src
    })
}

fn without_tree_fade(src: &str) -> String {
    let src = stub_fn(src, "fn tree_card_fade(world: vec3<f32>) -> f32 {", "1.0");
    stub_fn(
        &src,
        "fn tree_crown_normal(local: vec3<f32>, m: mat4x4<f32>, up: vec3<f32>) -> vec3<f32> {",
        "up",
    )
}

fn stub_fn(src: &str, head: &str, value: &str) -> String {
    stub_body(src, head, &format!("return {value};"))
}

/// `head`'s function with `body` for all it does.
fn stub_body(src: &str, head: &str, body: &str) -> String {
    let start = src
        .find(head)
        .unwrap_or_else(|| panic!("scene shader: {head} not found"));
    let close = start
        + src[start..]
            .find("\n}")
            .unwrap_or_else(|| panic!("scene shader: {head} has no end"))
        + 2;
    format!("{}{head}\n    {body}\n}}{}", &src[..start], &src[close..])
}

/// The sun's shadow tent with each gather of four depth comparisons taken as four single
/// ones. OpenGL without storage buffers is GLES 3.0 or GL 3.3 (ANGLE on Direct3D 11 among
/// them), which have no textureGather: the scene pipelines failed to compile there ("no
/// matching overloaded function found") and DirectX 11 drew nothing. Each texel is read at
/// its centre, where the comparison sampler's filter takes that texel alone.
pub(crate) fn compares_without_gather(src: &str) -> String {
    let mut out = src.to_string();
    for t in ["t_shadow", "t_shadow_far"] {
        let at = |x: f32, y: f32| {
            format!(
                "textureSampleCompareLevel({t}, s_shadow, a + vec2<f32>({x:?}, {y:?}) / dims, zr)"
            )
        };
        let gather = format!("textureGatherCompare({t}, s_shadow, a, zr)");
        assert!(out.contains(&gather), "scene shader: {gather} not found");
        // (in the gather's order: (0, 1), (1, 1), (1, 0), (0, 0) of the quad around `a`)
        let four = format!(
            "vec4<f32>({}, {}, {}, {})",
            at(-0.5, 0.5),
            at(0.5, 0.5),
            at(0.5, -0.5),
            at(-0.5, -0.5)
        );
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
            if pick.is_empty() {
                format!("vec4<{ty}>")
            } else {
                ty.to_string()
            }
        )
    };
    swap(
        "var<storage, read> models: array<vec4<f32>>;",
        load("models", "f32", ""),
        "models",
        "models_at",
    );
    swap(
        "var<storage, read> inst_params: array<vec4<f32>>;",
        load("inst_params", "f32", ""),
        "inst_params",
        "inst_params_at",
    );
    swap(
        "var<storage, read> draw_list: array<u32>;",
        load("draw_list", "u32", ".x"),
        "draw_list",
        "draw_list_at",
    );
    if path == ArrayPath::LightTextures {
        let lights = format!(
            "var lights_tex: texture_2d<f32>;\nfn lights_texel(t: u32) -> vec4<f32> {{ return textureLoad(lights_tex, vec2<u32>(t % {w}u, t / {w}u), 0); }}\nfn lights_at(i: u32) -> PointLight {{ return PointLight(lights_texel(i * 4u), lights_texel(i * 4u + 1u), lights_texel(i * 4u + 2u), lights_texel(i * 4u + 3u)); }}"
        );
        swap(
            "var<storage, read> lights: array<PointLight>;",
            lights,
            "lights",
            "lights_at",
        );
        swap(
            "var<storage, read> grid: array<u32>;",
            load("grid", "u32", ".x"),
            "grid",
            "grid_at",
        );
    }
    if path == ArrayPath::NoStorage {
        swap(
            "@group(0) @binding(3) var<storage, read> lights: array<PointLight>;",
            "fn lights_at(i: u32) -> PointLight { return PointLight(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0)); }".to_string(),
            "lights",
            "lights_at",
        );
        swap(
            "@group(0) @binding(4) var<storage, read> grid: array<u32>;",
            "fn grid_at(i: u32) -> u32 { return 0xffffffffu; }".to_string(),
            "grid",
            "grid_at",
        );
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

pub(crate) fn backend_forms(
    src: String,
    gl: bool,
    amd_vulkan: bool,
    multisampled_shading: bool,
) -> String {
    let src = if !gl && amd_vulkan {
        src.replace("//VK ", "")
    } else {
        src.replace("//DX ", "")
    };
    if !gl && multisampled_shading {
        src.replace("//MS ", "")
    } else {
        src
    }
}

pub(crate) fn scene_shader_text(gl: bool) -> String {
    scene_shader_forms(
        gl,
        AMD_VULKAN.load(std::sync::atomic::Ordering::Relaxed),
        MULTISAMPLED_SHADING.load(std::sync::atomic::Ordering::Relaxed),
    )
}

pub(crate) fn scene_shader_forms(gl: bool, amd_vulkan: bool, multisampled_shading: bool) -> String {
    materials_from_heap(&scene_shader_bound_forms(
        gl,
        amd_vulkan,
        multisampled_shading,
    ))
}

/// The scene shader's material as mX reads it: no bind group per material. Its values are a
/// uniform still - the scene's material table, bound once, each draw reading its own row at
/// its own offset (`RenderPass::set_buffer_offset`, see `MaterialTable`): a uniform stays in
/// the GPU's constant memory, a storage table read per field was a fifth slower on Apple's
/// GPUs. Its textures and samplers come from the heap, their places in the draw's immediates
/// (`mx_mat`, see `MaterialBinding`).
/// (The last step of the scene shader's forms: the others match the bound names.)
pub(crate) fn materials_from_heap(src: &str) -> String {
    const BOUND: [(&str, &str); 11] = [
        ("t_diffuse", "mx_tex2d[mx_mat.diffuse]"),
        ("s_diffuse", "mx_samplers[mx_mat.sampler]"),
        ("t_trans", "mx_tex2d[mx_mat.trans]"),
        ("t_night", "mx_tex2d[mx_mat.night]"),
        ("t_light", "mx_tex2d[mx_mat.light]"),
        ("t_env", "mx_tex2d[mx_mat.env]"),
        ("t_envmask", "mx_tex2d[mx_mat.envmask]"),
        ("t_bump", "mx_tex2d[mx_mat.bump]"),
        ("t_pbr_normal", "mx_tex2d[mx_mat.pbr_normal]"),
        ("t_pbr_orm", "mx_tex2d[mx_mat.pbr_orm]"),
        ("s_tile", "mx_samplers[mx_mat.tile_sampler]"),
    ];
    // (a checkout with CRLF line ends, as on Windows, has them in the included source too)
    let mut out = src.replace('\r', "");
    // the bound declarations go
    for (k, (name, _)) in BOUND.iter().enumerate() {
        let ty = if name.starts_with("s_") {
            "sampler"
        } else {
            "texture_2d<f32>"
        };
        let binding = [0, 1, 3, 4, 5, 6, 7, 8, 9, 10, 11][k];
        let decl = format!("@group(1) @binding({binding}) var {name}: {ty};\n");
        assert!(out.contains(&decl), "scene shader: {decl} not found");
        out = out.replace(&decl, "");
    }
    // the values stay a uniform (a draw's row of the material table, by its offset - see
    // `MaterialTable`); the draw's heap places beside it
    let decl = "@group(1) @binding(2) var<uniform> material: MaterialParams;";
    assert!(out.contains(decl), "scene shader: {decl} not found");
    out = out.replace(
        decl,
        &format!(
            "{decl}
// (the material's textures and samplers by their places in the heap, see `materials_from_heap`)
// (and after them the frame's places, set once a pass: `FrameHeap`)
struct MxMaterial {{ row: u32, diffuse: u32, sampler: u32, trans: u32, night: u32, light: u32, env: u32, envmask: u32, bump: u32, pbr_normal: u32, pbr_orm: u32, tile_sampler: u32, ground: u32 }};
var<immediate> mx_mat: MxMaterial;"
        ),
    );
    // the textures and samplers: the heap's, at the draw's places
    for (name, heap) in BOUND {
        out = replace_word(&out, name, heap);
    }
    out
}

/// `src` as naga alone reads it: with mX's heap prelude, when it uses the heap (mX adds the
/// same to every module it makes). For checks that parse the scene shader without mX.
pub(crate) fn with_heap(src: &str) -> String {
    if src.contains("mx_") {
        format!("enable wgpu_binding_array; {src}\n{}", mx::PRELUDE)
    } else {
        src.to_string()
    }
}

/// One entry point of `src` in GLSL as mX's OpenGL backend writes it: the heap lowered to
/// bindings, the immediates as uniforms (naga's GLSL writer alone takes no binding arrays).
#[cfg(test)]
pub(crate) fn glsl_through_mx(
    src: &str,
    stage: naga::ShaderStage,
    entry: &str,
    constants: &[(&str, f64)],
    version: naga::back::glsl::Version,
) -> Result<String, String> {
    use mx::mx_shader::{Module, glsl};
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    static PARSED: std::sync::Mutex<Vec<(u64, std::sync::Arc<Module>)>> =
        std::sync::Mutex::new(Vec::new());
    let key = {
        let mut h = DefaultHasher::new();
        src.hash(&mut h);
        h.finish()
    };
    let found = PARSED
        .lock()
        .unwrap()
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, m)| m.clone());
    let m = match found {
        Some(m) => m,
        None => {
            let m = std::sync::Arc::new(Module::from_wgsl("test", src).map_err(|e| e.to_string())?);
            PARSED.lock().unwrap().push((key, m.clone()));
            m
        }
    };
    // (as on a device whose stage has no storage buffers: the frame's read-only views as
    // uniform blocks, `views_as_uniforms`)
    let s = glsl::stage_with(&m, stage, Some(entry), constants, true).map_err(|e| e.to_string())?;
    let layout = glsl::Layout::for_pipeline(&[&s])?;
    let options = glsl::Options {
        version,
        texture_shadow_lod: false,
        draw_parameters: false,
    };
    Ok(glsl::write(&m, &s, 0, &layout, &options)
        .map_err(|e| e.to_string())?
        .source)
}

/// `src` with every whole word `word` (not part of a longer name) replaced by `with`.
/// The scene shader's frame (the camera group's textures, samplers and buffers), its material
/// table row and the vehicle reflection's uniform read from mX's heap: no bind group but group
/// 3's (the multisampled pane depth, the temporal motion; the heap has no multisampled
/// textures). Each declaration of groups 0 to 2 goes; its texture or sampler is read from the
/// heap array of its kind, its buffer through a typed view of the heap (`@group(7)`, a
/// binding a type), all at the places the draw's immediates carry after the material's
/// (`FrameHeap`). The last step of the scene shader's forms: the others match the bound names.
pub(crate) fn frame_from_heap(src: &str) -> String {
    // (the variable, the immediates' field it is read at)
    const FIELDS: [(&str, &str); 31] = [
        ("vehicle_reflection", "ground"),
        ("t_shadow", "shadow"),
        ("t_shadow_far", "shadow_far"),
        ("t_ao", "ao"),
        ("t_gi", "gi"),
        ("t_probe", "probe"),
        ("t_sky_lut", "sky_lut"),
        ("t_lmap", "lmap_tex"),
        ("s_shadow", "s_shadow"),
        ("s_ao", "s_ao"),
        ("s_lin", "s_lin"),
        ("camera", "camera"),
        ("models", "models"),
        ("models_tex", "models"),
        ("inst_params", "inst_params"),
        ("inst_params_tex", "inst_params"),
        ("draw_list", "draw_list"),
        ("draw_list_tex", "draw_list"),
        ("lights", "lights"),
        ("lights_tex", "lights"),
        ("grid", "grid"),
        ("grid_tex", "grid"),
        ("enh", "enh"),
        ("lmap", "lmap"),
        ("snow_track_px", "snow_px"),
        ("snow_track", "snow"),
        ("t_dry", "dry"),
        ("material", "row"),
        // (the motion pipelines', temporal/motion.wgsl: they read no lights)
        ("motion", "lights"),
        ("prev_models", "grid"),
        // (the masked prefill's, `encode_masked`: it reads no ground picture)
        ("t_pane_depth", "ground"),
    ];
    let mut out = src.replace('\r', "");
    let mut views: Vec<(String, String)> = Vec::new(); // (type, view array)
    let mut decls = String::new();
    let lines: Vec<String> = out.lines().map(str::to_string).collect();
    let mut kept = Vec::with_capacity(lines.len());
    let mut swaps: Vec<(String, String)> = Vec::new();
    for line in lines {
        let t = line.trim_start();
        let group = ["@group(0)", "@group(1)", "@group(2)", "@group(3)"]
            .iter()
            .any(|g| t.starts_with(g));
        let Some(var) = t.find(" var").filter(|_| group) else {
            kept.push(line);
            continue;
        };
        let decl = t[var + 4..].trim_end().trim_end_matches(';');
        let (space, rest) = match decl.strip_prefix('<') {
            Some(r) => {
                let close = r.find('>').expect("scene shader: a var's space");
                (r[..close].trim(), r[close + 1..].trim())
            }
            None => ("", decl.trim()),
        };
        let (name, ty) = rest.split_once(':').expect("scene shader: a var's type");
        let (name, ty) = (name.trim(), ty.trim());
        let field = FIELDS
            .iter()
            .find(|f| f.0 == name)
            .unwrap_or_else(|| panic!("scene shader: {name} has no place in the frame's heap"))
            .1;
        let place = format!("mx_mat.{field}");
        let read = match (space, ty) {
            ("", "texture_2d<f32>") => format!("mx_tex2d[{place}]"),
            ("", "texture_2d<u32>") => format!("mx_tex2d_u32[{place}]"),
            ("", "texture_depth_2d") => format!("mx_texdepth[{place}]"),
            ("", "texture_depth_multisampled_2d") => format!("mx_texdepth_ms[{place}]"),
            ("", "texture_cube<f32>") => format!("mx_texcube[{place}]"),
            ("", "sampler") => format!("mx_samplers[{place}]"),
            ("", "sampler_comparison") => format!("mx_samplers_cmp[{place}]"),
            (_, _) => {
                // a buffer: a typed view of its type (a struct, as views must be)
                let k = views.iter().position(|v| v.0 == ty).unwrap_or_else(|| {
                    let k = views.len();
                    let binding = 16 + k;
                    let array = format!("mx_view{k}");
                    let wrapped = !ty.starts_with(char::is_uppercase);
                    if wrapped {
                        decls.push_str(&format!("struct MxView{k} {{ v: {ty} }};\n"));
                    }
                    decls.push_str(&format!(
                        "@group(7) @binding({binding}) var<storage, read> {array}: binding_array<{}>;\n",
                        if wrapped { format!("MxView{k}") } else { ty.to_string() }
                    ));
                    views.push((ty.to_string(), array));
                    k
                });
                let wrapped = !ty.starts_with(char::is_uppercase);
                format!("{}[{place}]{}", views[k].1, if wrapped { ".v" } else { "" })
            }
        };
        swaps.push((name.to_string(), read));
    }
    out = kept.join("\n");
    for (name, read) in &swaps {
        out = replace_ident(&out, name, read);
    }
    // the frame's places after the material's in the draw's immediates (`FrameHeap`)
    let head = "tile_sampler: u32, ground: u32 }";
    assert!(out.contains(head), "scene shader: the immediates' struct");
    out = out.replacen(
        head,
        "tile_sampler: u32, ground: u32, shadow: u32, shadow_far: u32, ao: u32, probe: u32, sky_lut: u32, lmap_tex: u32, s_shadow: u32, s_ao: u32, s_lin: u32, camera: u32, models: u32, inst_params: u32, draw_list: u32, lights: u32, grid: u32, enh: u32, lmap: u32, snow_px: u32, snow: u32, gi: u32, dry: u32 }",
        1,
    );
    format!("{decls}{out}")
}

/// `replace_word` for a variable: not where the word is a member (`enh.lights`) or declares
/// one (`lights: vec4<f32>,` in a struct).
pub(crate) fn replace_ident(src: &str, word: &str, with: &str) -> String {
    let is_name = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(k) = rest.find(word) {
        let before = rest[..k].chars().next_back();
        let member = rest[..k].trim_end().ends_with('.');
        let after_text = rest[k + word.len()..].trim_start();
        let declares = after_text.starts_with(':') && !after_text.starts_with("::");
        let after = rest[k + word.len()..].chars().next();
        out.push_str(&rest[..k]);
        if before.is_some_and(is_name) || after.is_some_and(is_name) || member || declares {
            out.push_str(word);
        } else {
            out.push_str(with);
        }
        rest = &rest[k + word.len()..];
    }
    out.push_str(rest);
    out
}

fn replace_word(src: &str, word: &str, with: &str) -> String {
    let is_name = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(k) = rest.find(word) {
        let before = rest[..k].chars().next_back();
        let after = rest[k + word.len()..].chars().next();
        out.push_str(&rest[..k]);
        if before.is_some_and(is_name) || after.is_some_and(is_name) {
            out.push_str(word);
        } else {
            out.push_str(with);
        }
        rest = &rest[k + word.len()..];
    }
    out.push_str(rest);
    out
}

fn scene_shader_bound_forms(gl: bool, amd_vulkan: bool, multisampled_shading: bool) -> String {
    let src = [
        include_str!("../colour.wgsl"),
        shaders::scene_part(),
        if road_snow_supported_on(
            gl,
            omsi_cfg::flags::OMSI_GL_TEXTURE_UNITS.is_set(),
            array_path(),
        ) {
            include_str!("../road_snow.wgsl")
        } else {
            include_str!("../road_snow_off.wgsl")
        },
        include_str!("../enhanced_common.wgsl"),
        include_str!("../puddle_common.wgsl"),
        include_str!("../beam_shadow_none.wgsl"),
        include_str!("../lamp_air.wgsl"),
        shaders::enhanced_part(),
    ]
    .join("\n");
    // Enhanced+: the enhanced pass writes the reflections' surfaces as well (`GBUF_FORMAT`)
    let src = if rt_gbuf() {
        src.replace("//RT ", "")
    } else {
        src
    };
    let src = if basic_pipelines() {
        lean_scene(src)
    } else {
        src
    };
    let src = backend_forms(src, gl, amd_vulkan, multisampled_shading);
    if !gl {
        return src;
    }
    let clamped = |t: &str, bias: &str| {
        format!(
            "textureSampleBias({t}, s_diffuse, clamp(uv, 0.5 / vec2<f32>(textureDimensions({t})), \
             vec2<f32>(1.0) - 0.5 / vec2<f32>(textureDimensions({t}))), {bias})"
        )
    };
    // No SSAO and no ray tracing on OpenGL (camera.clouds.w stays 0, see `lighter_picture`):
    // their lookups are left out of the module. With them in the scene fragment shader, the
    // D3D compiler under ANGLE (fxc) recursed without end in CProgram::CheckAssertion_Group
    // and the game closed before its first frame (#197).
    let out = without_tree_fade(&src)
        .replace(
            "textureSampleBias(t_trans, s_tile, uv, bias)",
            &clamped("t_trans", "bias"),
        )
        .replace(
            "textureSampleBias(t_night, s_tile, uv, camera.lod.x)",
            &clamped("t_night", "camera.lod.x"),
        )
        .replace("ao = ao_at(in.clip.xy, in.world);", "ao = 1.0;")
        .replace(
            "let ag = ao_gi_at(in.clip.xy, in.world, surface_rt);",
            "let ag = vec4<f32>(1.0, 0.0, 0.0, 0.0);",
        )
        .replace(
            "rt_at(in.clip.xy, in.world)",
            "vec4<f32>(1.0, 0.0, -1.0, 0.0)",
        )
        .replace(
            "gi_at(in.clip.xy, in.world)",
            "vec4<f32>(0.0, 0.0, 0.0, -1.0)",
        );
    debug_assert!(!out.contains("s_tile, uv)") && !out.contains("s_tile, uv, "));
    debug_assert!(
        !out.contains("ao_at(in.")
            && !out.contains("ao_gi_at(in.")
            && !out.contains("rt_at(in.")
            && !out.contains("gi_at(in.")
    );
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
        .replace(
            "seen_light = seen_light + share * lamp_shadow_at(li, p, n, false);",
            "seen_light = seen_light + share;",
        )
        .replace("ms = sun_shadow_soft(in.world, n, thin);", "ms = 1.0;")
        .replace(
            "let sm = sun_shadow_soft(in.world, n, thin);",
            "let sm = 1.0;",
        );
    debug_assert_eq!(out.matches("sun_shadow_soft(in.world").count(), 1);
    debug_assert!(!out.contains("* lamp_shadow_at("));
    // and no drops on the panes, only their film and blur: FXC unrolls the drops' cell
    // loops into each of the shader's glass paths, and Direct3D 11 on WARP spent over two
    // minutes on the scene pipelines (the ANGLE startup check timed out)
    stub_body(
        &out,
        "fn glass_drops(a: ptr<function, DropAcc>, b: PaneBase, p: PaneProj, w: f32, wet: f32, heavy: f32, bank: f32, size: f32, world: vec3<f32>, t: f32) {",
        "",
    )
}

/// The sky dome (both paths) and the enhanced reflection probe.
pub(crate) fn sky_shader_source() -> String {
    [
        crate::shaders::hot_wgsl(include_str!("../colour.wgsl"), "colour.wgsl"),
        crate::shaders::hot_wgsl(include_str!("../sky.wgsl"), "sky.wgsl"),
        crate::shaders::hot_wgsl(
            include_str!("../enhanced_common.wgsl"),
            "enhanced_common.wgsl",
        ),
        crate::shaders::hot_wgsl(include_str!("../sky_enhanced.wgsl"), "sky_enhanced.wgsl"),
        crate::shaders::hot_wgsl(include_str!("../clouds_ss.wgsl"), "clouds_ss.wgsl"),
    ]
    .join("\n")
}

/// The snowfall (snow.wgsl), with the lamps the enhanced picture lights it by (none
/// without storage buffers).
pub(crate) fn snow_shader_source() -> String {
    let src = precip_shader_source(include_str!("../snow.wgsl"));
    if basic_pipelines() {
        lean_snow(src)
    } else {
        src
    }
}

/// The snowfall as the basic set makes it: lit without the loop over the lamps (one more
/// thing a phone's compiler can fail on, see `lean_scene`), by the sky's light from above.
pub(crate) fn lean_snow(src: String) -> String {
    // (no line end in the pattern: a Windows checkout turns the file's into CRLF)
    const LAMPS: &str = "l = (precip_light(in.pos, to_eye, true) + up) * 0.5 * 0.85";
    debug_assert_eq!(src.matches(LAMPS).count(), 1);
    src.replace(LAMPS, "l = (up + up) * 0.5 * 0.85")
}

/// The rainfall (rain.wgsl), as the snowfall.
pub(crate) fn rain_shader_source() -> String {
    let src = precip_shader_source(include_str!("../rain.wgsl"));
    if basic_pipelines() {
        lean_rain(src)
    } else {
        src
    }
}

pub(crate) fn lean_rain(src: String) -> String {
    // (no line end in the pattern: a Windows checkout turns the file's into CRLF)
    const LAMPS: &str =
        "l = max(precip_light(in.pos, to_eye, true) - round, vec3<f32>(0.0)) * 1.5 + l;";
    debug_assert_eq!(src.matches(LAMPS).count(), 1);
    src.replace(LAMPS, "")
}

pub(crate) fn roof_as_float(src: String) -> String {
    const DECL: &str = "var roof_map: texture_depth_2d;";
    const LOAD: &str = "return textureLoad(roof_map, t, 0); }";
    debug_assert_eq!(
        (src.matches(DECL).count(), src.matches(LOAD).count()),
        (1, 1)
    );
    src.replace(DECL, "var roof_map: texture_2d<f32>;")
        .replace(LOAD, "return textureLoad(roof_map, t, 0).x; }")
}

pub(crate) fn precip_shader_source(own: &str) -> String {
    let src = [
        include_str!("../precip_common.wgsl"),
        own,
        include_str!("../enhanced_common.wgsl"),
        FOG_LAMP_LIGHTS,
        include_str!("../beam_shadow_none.wgsl"),
        include_str!("../lamp_air.wgsl"),
    ]
    .join("\n");
    let src = if gl_backend() {
        roof_as_float(src)
    } else {
        src
    };
    if array_path().no_storage() {
        let src = src
            .replace("@group(0) @binding(3) var<storage, read> lights: array<PointLight>;", "fn lights_at(i: u32) -> PointLight { return PointLight(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0)); }")
            .replace("@group(0) @binding(4) var<storage, read> grid: array<u32>;", "fn grid_at(i: u32) -> u32 { return 0xffffffffu; }");
        return indexing_as_calls(
            &indexing_as_calls(&src, "lights", "lights_at"),
            "grid",
            "grid_at",
        );
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
    /// Enhanced: where the veil's panels begin, how many, the gusts' strength, the picture's
    /// height (px).
    pub(crate) ext: [f32; 4],
}

/// The snowfall's flakes for a snowfall of strength `s` (0..1): how many each layer of
/// snow.wgsl holds (the near one at the density of a heavy fall, some 190 a cubic metre,
/// scaled down for a lighter one) and their mean diameter (m) - a light fall single
/// crystals of a millimetre, a heavy one aggregates of several.
pub(crate) fn snowfall_flakes(s: f32) -> ([u32; 3], f32) {
    let k = 0.08 + 0.92 * s.clamp(0.0, 1.0);
    (
        [
            (70000.0 * k) as u32,
            (55000.0 * k) as u32,
            (25000.0 * k) as u32,
        ],
        0.0006 + 0.0034 * s * s,
    )
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
    /// Enhanced: where the veil's panels begin, how many, the gusts' strength, the quality.
    pub(crate) ext: [f32; 4],
}

/// The enhanced rain's and snow's far veil (rain.wgsl, snow.wgsl `veil_corner`): twelve
/// panels a ring, one ring on the `rain_quality` setting's low, two on medium, three on
/// high.
pub(crate) fn veil_panels(quality: u8) -> u32 {
    12 * (quality.min(2) as u32 + 1)
}

/// How many drops each layer of rain.wgsl holds in a rain of strength `r` (0..1) at the
/// rain's quality (0 low, 1 medium, 2 high): the near one some 90 a cubic metre in a
/// downpour - of the drops big enough to see, a few hundred a cubic metre in heavy rain -
/// and a tenth of that in a drizzle.
/// The fourth figure is the splashes where they land (see rain.wgsl `splash`): some forty
/// on each square metre at a time in a downpour, close by.
pub(crate) fn rainfall_drops(r: f32, quality: u8) -> [u32; 4] {
    let k = (0.12 + 0.88 * r.clamp(0.0, 1.0)) * [0.3, 0.6, 1.0][quality.min(2) as usize];
    [
        (90000.0 * k) as u32,
        (110000.0 * k) as u32,
        (60000.0 * k) as u32,
        (25000.0 * k) as u32,
    ]
}

/// Enhanced: the lamps' light in the fog over the drawn picture (`fog_lamps.wgsl`; not
/// without storage buffers, where there are no lamps).
pub(crate) fn fog_lamps_shader_source() -> String {
    [
        crate::shaders::hot_wgsl(include_str!("../fog_lamps.wgsl"), "fog_lamps.wgsl"),
        include_str!("../enhanced_common.wgsl"),
        FOG_LAMP_LIGHTS,
        if omsi_cfg::flags::OMSI_NO_BEAM_SHADOWS.is_set() {
            include_str!("../beam_shadow_none.wgsl")
        } else {
            include_str!("../beam_shadow.wgsl")
        },
        crate::shaders::hot_wgsl(include_str!("../lamp_air.wgsl"), "lamp_air.wgsl"),
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
        crate::shaders::hot_wgsl(include_str!("../corona.wgsl"), "corona.wgsl"),
        include_str!("../enhanced_common.wgsl"),
        FOG_LAMP_LIGHTS,
        include_str!("../beam_shadow_none.wgsl"),
        include_str!("../lamp_air.wgsl"),
    ]
    .join("\n");
    // (the basic set lights rain by the sky alone: the loop over the lamps is one more
    // thing a phone's compiler can fail on, see `lean_scene`)
    let src = if basic_pipelines() {
        src.replace(
            "let l = precip_light(in.wpos.xyz, to_eye, in.wpos.w > 1.5);",
            "let l = sh_irradiance(vec3<f32>(0.0, 0.0, 1.0)) / PI;",
        )
    } else {
        src
    };
    // (without storage buffers the precipitation has no lamps to be lit by)
    if array_path().no_storage() {
        let src = src
            .replace("@group(0) @binding(3) var<storage, read> lights: array<PointLight>;", "fn lights_at(i: u32) -> PointLight { return PointLight(vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0), vec4<f32>(0.0)); }")
            .replace("@group(0) @binding(4) var<storage, read> grid: array<u32>;", "fn grid_at(i: u32) -> u32 { return 0xffffffffu; }");
        return indexing_as_calls(
            &indexing_as_calls(&src, "lights", "lights_at"),
            "grid",
            "grid_at",
        );
    }
    src
}

#[cfg(test)]
mod tree_detail_tests {
    #[test]
    fn opengl_leaves_the_tree_detail_out_with_either_line_end() {
        for src in [
            super::shaders::SCENE.to_string(),
            super::shaders::SCENE.replace('\n', "\r\n"),
        ] {
            let out = super::without_tree_fade(&src);
            assert!(
                !out.contains("cross(dpdx(world), dpdy(world))") && !out.contains("0.65 * crown")
            );
            assert!(
                out.contains("fn tree_card_fade(world: vec3<f32>) -> f32 {\n    return 1.0;\n}")
            );
            assert!(out.contains("-> vec3<f32> {\n    return up;\n}"));
            assert_eq!(out.matches("fn ").count(), src.matches("fn ").count());
        }
    }
}

#[cfg(test)]
mod materials_from_heap_tests {
    /// a Windows checkout has CRLF line ends in the included shader: the bound declarations
    /// are still found and taken out
    #[test]
    fn the_bound_materials_go_with_either_line_end() {
        let joined = super::scene_shader_bound_forms(false, false, false).replace('\r', "");
        for src in [joined.clone(), joined.replace('\n', "\r\n")] {
            let out = super::materials_from_heap(&src);
            assert!(!out.contains("var t_diffuse: texture_2d<f32>;"));
            assert!(out.contains("mx_tex2d[mx_mat.diffuse]"));
        }
    }
}
