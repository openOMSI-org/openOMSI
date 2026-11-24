//! A module's root bind groups read from mX's heap instead: no bind group. Each declaration
//! of groups 0 to 6 goes; its texture or sampler is read from the heap array of its kind,
//! its buffer through a typed view of the heap (`@group(7)`, a binding a type), all at places
//! the draw's immediates carry (`mx_g`, one `u32` a declaration, in the order of their
//! group and binding: `HeapLayout`). What the heap cannot hold stays a root binding:
//! multisampled and storage textures, acceleration structures, writable buffers.
//! On the Rust side a `HeapGroup` holds the places of a bind group's resources (made from the
//! same entries) and writes them into a pass's immediates for the layout of the module the
//! pass draws with.

use std::sync::Arc;

/// The layout of the full-screen picture passes (upscale.wgsl, fsr1.wgsl, overlay.wgsl,
/// xr_ui.wgsl): a uniform, a texture and a sampler at bindings 0 to 2 of group 0 - one
/// `HeapGroup` serves them all.
pub(crate) fn picture_layout() -> &'static HeapLayout {
    static L: std::sync::OnceLock<HeapLayout> = std::sync::OnceLock::new();
    L.get_or_init(|| HeapLayout {
        slots: vec![(0, 0), (0, 1), (0, 2)],
    })
}

/// `groups_to_heap` for a picture pass's module (see `picture_layout`).
pub(crate) fn picture_wgsl(src: &str) -> String {
    assert_eq!(
        &layout_of(src),
        picture_layout(),
        "a picture pass's bindings"
    );
    groups_to_heap(src)
}

/// A module's layout made once from its source.
pub(crate) struct HeapModule {
    pub(crate) source: fn() -> String,
    pub(crate) layout: std::sync::OnceLock<HeapLayout>,
}

impl HeapModule {
    pub(crate) const fn new(source: fn() -> String) -> HeapModule {
        HeapModule {
            source,
            layout: std::sync::OnceLock::new(),
        }
    }

    /// Its source as the device compiles it: the root groups read from the heap.
    pub(crate) fn wgsl(&self) -> String {
        groups_to_heap(&(self.source)())
    }

    pub(crate) fn layout(&self) -> &HeapLayout {
        self.layout.get_or_init(|| layout_of(&(self.source)()))
    }
}

/// A module's places in the immediates: the (group, binding) of each `u32`, sorted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HeapLayout {
    pub(crate) slots: Vec<(u32, u32)>,
}

/// One root declaration of a module: (group, binding, space, name, type).
struct Decl {
    group: u32,
    binding: u32,
    space: String,
    name: String,
    ty: String,
}

fn parse_decl(line: &str) -> Option<Decl> {
    let t = line.trim_start();
    let rest = t.strip_prefix("@group(")?;
    let (group, rest) = rest.split_once(')')?;
    let group: u32 = group.trim().parse().ok()?;
    if group > 6 {
        return None;
    }
    let rest = rest.trim_start().strip_prefix("@binding(")?;
    let (binding, rest) = rest.split_once(')')?;
    let binding: u32 = binding.trim().parse().ok()?;
    let decl = rest.trim_start().strip_prefix("var")?;
    let decl = decl.trim_end().trim_end_matches(';').trim_end();
    let (space, rest) = match decl.strip_prefix('<') {
        Some(r) => {
            let close = r.find('>')?;
            (r[..close].trim().to_string(), r[close + 1..].trim())
        }
        None => (String::new(), decl.trim()),
    };
    let (name, ty) = rest.split_once(':')?;
    Some(Decl {
        group,
        binding,
        space,
        name: name.trim().to_string(),
        ty: ty.trim().to_string(),
    })
}

/// The heap array a texture or sampler type is read from; `None`: a buffer, or what stays
/// a root binding.
fn heap_array(ty: &str) -> Option<&'static str> {
    Some(match ty {
        "texture_2d<f32>" => "mx_tex2d",
        "texture_2d<u32>" => "mx_tex2d_u32",
        "texture_2d_array<f32>" => "mx_tex2d_array",
        "texture_cube<f32>" => "mx_texcube",
        "texture_3d<f32>" => "mx_tex3d",
        "texture_depth_2d" => "mx_texdepth",
        "sampler" => "mx_samplers",
        "sampler_comparison" => "mx_samplers_cmp",
        _ => return None,
    })
}

/// Whether a declaration moves to the heap.
fn in_heap(d: &Decl) -> bool {
    if d.space.is_empty() {
        heap_array(&d.ty).is_some()
    } else {
        d.space == "uniform" || d.space == "storage" || d.space == "storage, read"
    }
}

/// `src`'s layout (what `groups_to_heap` makes of it).
pub(crate) fn layout_of(src: &str) -> HeapLayout {
    let mut slots: Vec<(u32, u32)> = src
        .lines()
        .filter_map(parse_decl)
        .filter(in_heap)
        .map(|d| (d.group, d.binding))
        .collect();
    slots.sort_unstable();
    slots.dedup();
    HeapLayout { slots }
}

/// `src` with its root groups read from the heap (see the module's comment).
pub(crate) fn groups_to_heap(src: &str) -> String {
    let src = src.replace('\r', "");
    let layout = layout_of(&src);
    if layout.slots.is_empty() {
        return src;
    }
    assert!(
        !src.contains("var<immediate>"),
        "a module with immediates of its own keeps its bind groups"
    );
    let mut views: Vec<String> = Vec::new();
    let mut decls = String::new();
    let mut swaps: Vec<(String, String)> = Vec::new();
    let mut kept = Vec::new();
    for line in src.lines() {
        let Some(d) = parse_decl(line).filter(in_heap) else {
            kept.push(line);
            continue;
        };
        let place = format!("mx_g.g{}_{}", d.group, d.binding);
        let read = match heap_array(&d.ty).filter(|_| d.space.is_empty()) {
            Some(array) => format!("{array}[{place}]"),
            None => {
                // (a typed view of the buffer's type: a struct, as views must be)
                let wrapped = !d.ty.starts_with(char::is_uppercase);
                let k = views.iter().position(|v| *v == d.ty).unwrap_or_else(|| {
                    let k = views.len();
                    if wrapped {
                        decls.push_str(&format!("struct MxView{k} {{ v: {} }};\n", d.ty));
                    }
                    decls.push_str(&format!(
                        "@group(7) @binding({}) var<storage, read> mx_view{k}: binding_array<{}>;\n",
                        16 + k,
                        if wrapped {
                            format!("MxView{k}")
                        } else {
                            d.ty.clone()
                        }
                    ));
                    views.push(d.ty.clone());
                    k
                });
                format!("mx_view{k}[{place}]{}", if wrapped { ".v" } else { "" })
            }
        };
        swaps.push((d.name, read));
    }
    let out = replace_globals(&kept.join("\n"), &swaps);
    let fields: Vec<String> = layout
        .slots
        .iter()
        .map(|(g, b)| format!("g{g}_{b}: u32"))
        .collect();
    format!(
        "struct MxGroups {{ {} }};\nvar<immediate> mx_g: MxGroups;\n{decls}{out}",
        fields.join(", ")
    )
}

/// A bind group's resources by their places in the heap, for one module's layout: what a
/// pass sets instead of the bind group (`HeapGroup::set`).
#[derive(Clone)]
pub(crate) struct HeapGroup {
    /// (offset in the immediates, the places) of each run of the layout this group fills
    runs: Vec<(u32, Vec<u32>)>,
    /// what the places name, kept alive while they are drawn with
    _keep: Arc<Vec<Keep>>,
}

#[allow(dead_code)]
enum Keep {
    Texture(mx::TextureView),
    Sampler(mx::Sampler),
    Buffer(mx::BufferView),
}

impl HeapGroup {
    /// `new` with a bind group's descriptor (what the pass set before).
    pub(crate) fn of(
        layout: &HeapLayout,
        group: u32,
        desc: &mx::BindGroupDescriptor<'_>,
    ) -> HeapGroup {
        HeapGroup::new(layout, group, desc.entries)
    }

    /// The places of `entries` (as a bind group's) for group `group` of `layout`. A buffer
    /// gets a read-only view of its range.
    pub(crate) fn new(
        layout: &HeapLayout,
        group: u32,
        entries: &[mx::BindGroupEntry<'_>],
    ) -> HeapGroup {
        let mut keep = Vec::new();
        let mut place = |r: &mx::BindingResource<'_>| -> u32 {
            match r {
                mx::BindingResource::TextureView(v) => {
                    keep.push(Keep::Texture((*v).clone()));
                    v.index()
                }
                mx::BindingResource::Sampler(s) => {
                    keep.push(Keep::Sampler((*s).clone()));
                    s.index()
                }
                mx::BindingResource::Buffer(b) => {
                    assert!(
                        b.buffer.usage().contains(mx::BufferUsages::STORAGE),
                        "a buffer read through the heap needs STORAGE usage"
                    );
                    let view = b
                        .buffer
                        .create_view(&mx::BufferViewDescriptor {
                            offset: b.offset,
                            size: b.size,
                            ..Default::default()
                        })
                        .expect("mX: a buffer view");
                    let index = view.index();
                    keep.push(Keep::Buffer(view));
                    index
                }
                _ => panic!("a heap group holds textures, samplers and buffers"),
            }
        };
        let mut runs: Vec<(u32, Vec<u32>)> = Vec::new();
        for (k, &(g, b)) in layout.slots.iter().enumerate() {
            if g != group {
                continue;
            }
            let Some(e) = entries.iter().find(|e| e.binding == b) else {
                continue;
            };
            let index = place(&e.resource);
            let at = k as u32 * 4;
            match runs.last_mut() {
                Some((start, places)) if *start + places.len() as u32 * 4 == at => {
                    places.push(index)
                }
                _ => runs.push((at, vec![index])),
            }
        }
        HeapGroup {
            runs,
            _keep: Arc::new(keep),
        }
    }

    /// Write the places into `pass`'s immediates (in place of `set_bind_group`).
    pub(crate) fn set<E: mx::RenderEncoder>(&self, pass: &mut E) {
        for (at, places) in &self.runs {
            pass.set_immediates(*at, bytemuck::cast_slice(places));
        }
    }

    /// `set` for a compute pass.
    pub(crate) fn set_compute(&self, pass: &mut mx::ComputePass<'_>) {
        for (at, places) in &self.runs {
            pass.set_immediates(*at, bytemuck::cast_slice(places));
        }
    }
}

/// `src` with each module-scope variable of `swaps` read as its replacement: every use of the
/// name that the variable is - not a member (`enh.lights`), not a struct member's or a
/// parameter's declaration (`lights: vec4<f32>`), not where a parameter or a `let`, `var` or
/// `const` of the same name shadows it - in comments too, which is harmless.
pub(crate) fn replace_globals(src: &str, swaps: &[(String, String)]) -> String {
    let find = |name: &str| swaps.iter().find(|s| s.0 == name).map(|s| s.1.as_str());
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len() * 2);
    // the names shadowed in each open block; the parameters of the function about to open one
    let mut scopes: Vec<Vec<String>> = Vec::new();
    let mut params: Vec<String> = Vec::new();
    let mut in_params = false;
    let mut paren = 0i32;
    let (mut after_fn, mut declaring) = (false, false);
    let mut prev_sig = ' ';
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '/' && bytes.get(i + 1) == Some(&b'/') {
            let end = src[i..].find('\n').map_or(src.len(), |e| i + e);
            out.push_str(&src[i..end]);
            i = end;
            continue;
        }
        if c.is_ascii_digit() {
            // (a number with its suffix: `1u`, `0x80`, `2.5e-3f`)
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric()
                    || bytes[i] == b'.'
                    || bytes[i] == b'_'
                    || ((bytes[i] == b'-' || bytes[i] == b'+')
                        && matches!(bytes[i - 1], b'e' | b'E')
                        && !src[start..i].starts_with("0x")))
            {
                i += 1;
            }
            out.push_str(&src[start..i]);
            prev_sig = '0';
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &src[start..i];
            let rest = src[i..].trim_start();
            let declares = rest.starts_with(':') && !rest.starts_with("::");
            if after_fn {
                after_fn = false;
                out.push_str(word);
            } else if declaring {
                declaring = false;
                if let Some(s) = scopes.last_mut() {
                    s.push(word.to_string());
                }
                out.push_str(word);
            } else if in_params && paren == 1 && declares {
                params.push(word.to_string());
                out.push_str(word);
            } else {
                match word {
                    "fn" => after_fn = true,
                    // (`var<...> name` at module scope is a global: only local ones shadow)
                    "let" | "const" | "var" if !scopes.is_empty() => {
                        declaring = !rest.starts_with('<');
                        if rest.starts_with('<') {
                            // (the name after the address space)
                            declaring = true;
                        }
                    }
                    _ => {}
                }
                let shadowed = scopes.iter().any(|s| s.iter().any(|n| n == word));
                match find(word) {
                    Some(read)
                        if !shadowed
                            && !declares
                            && prev_sig != '.'
                            && !(in_params && paren == 1) =>
                    {
                        out.push_str(read)
                    }
                    _ => out.push_str(word),
                }
            }
            prev_sig = 'a';
            continue;
        }
        match c {
            '(' => {
                paren += 1;
                if prev_sig == 'a' && params.is_empty() && !in_params && fn_pending(&out) {
                    in_params = true;
                }
            }
            ')' => {
                paren -= 1;
                if in_params && paren == 0 {
                    in_params = false;
                }
            }
            '{' => scopes.push(std::mem::take(&mut params)),
            '}' => {
                scopes.pop();
            }
            '<' if declaring => {
                // (`var<function> x`: skip to the name)
                let close = src[i..].find('>').map_or(src.len(), |e| i + e + 1);
                out.push_str(&src[i..close]);
                i = close;
                prev_sig = '>';
                continue;
            }
            _ => {}
        }
        if !c.is_whitespace() {
            prev_sig = c;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Whether `out` ends in `fn name` (a function's parameters open next).
fn fn_pending(out: &str) -> bool {
    let t = out.trim_end();
    let name_start = t
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map_or(0, |k| k + 1);
    t[..name_start].trim_end().ends_with("fn")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globals_are_read_from_the_heap_where_they_are_not_shadowed() {
        let src = "struct P { a: vec4<f32>, p: f32 }
@group(0) @binding(0) var<uniform> p: P;
@group(0) @binding(1) var t: texture_2d<f32>;
@group(0) @binding(2) var s: sampler;
@group(0) @binding(3) var<uniform> u: vec4<f32>;
fn h(i: u32) -> vec4<f32> { return u * f32((i << 1u) & 2u) * 1e-3; }
fn f(p: vec2<f32>) -> vec4<f32> { return vec4<f32>(p, 0.0, 0.0); }
fn g(x: vec2<f32>) -> vec4<f32> { let q = p.a * p.p; { let p = 2.0; return q * p; } }
@fragment fn fs(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    var<function> s2 = textureSample(t, s, uv);
    return s2 + p.a + f(uv) + g(uv);
}";
        let out = groups_to_heap(src);
        assert!(
            out.contains("fn f(p: vec2<f32>) -> vec4<f32> { return vec4<f32>(p, 0.0, 0.0); }"),
            "{out}"
        );
        assert!(
            out.contains("let q = mx_view0[mx_g.g0_0].a * mx_view0[mx_g.g0_0].p;"),
            "{out}"
        );
        assert!(out.contains("{ let p = 2.0; return q * p; }"), "{out}");
        assert!(
            out.contains("textureSample(mx_tex2d[mx_g.g0_1], mx_samplers[mx_g.g0_2], uv)"),
            "{out}"
        );
        assert!(out.contains("struct P { a: vec4<f32>, p: f32 }"), "{out}");
        assert!(
            out.contains("return mx_view1[mx_g.g0_3].v * f32((i << 1u) & 2u) * 1e-3;"),
            "{out}"
        );
        assert!(out.contains("return s2 + mx_view0[mx_g.g0_0].a"), "{out}");
        assert_eq!(layout_of(src).slots, vec![(0, 0), (0, 1), (0, 2), (0, 3)]);
        naga::front::wgsl::parse_str(&crate::shaders::with_heap(&out)).expect("valid WGSL");
    }

    /// Every module the renderer reads from the heap, as the device gets it, parses and
    /// validates: a name `groups_to_heap` replaced wrongly fails here, not on a GPU.
    #[test]
    fn every_module_in_the_heap_validates() {
        let sources: Vec<(&str, String)> = vec![
            ("post", crate::pipelines::post::POST.wgsl()),
            ("ssao", crate::pipelines::ssao::SSAO.wgsl()),
            ("mip", crate::pipelines::mip::MIP.wgsl()),
            ("glass mips", crate::glass_mips::GLASS_MIP.wgsl()),
            ("srgb", crate::srgb_encode::SRGB.wgsl()),
            ("lamp sight", crate::lighting::LAMP_SIGHT.wgsl()),
            ("puddles", crate::puddles::PUDDLES.wgsl()),
            (
                "sky",
                groups_to_heap(&crate::pipelines::sky::shader_source(true)),
            ),
            (
                "sky plain",
                groups_to_heap(&crate::pipelines::sky::shader_source(false)),
            ),
            (
                "corona",
                groups_to_heap(&crate::shaders::corona_shader_source()),
            ),
            (
                "snow",
                groups_to_heap(&crate::shaders::snow_shader_source()),
            ),
            (
                "rain",
                groups_to_heap(&crate::shaders::rain_shader_source()),
            ),
            (
                "fog lamps",
                groups_to_heap(&crate::shaders::fog_lamps_shader_source()),
            ),
            (
                "volumetric fog",
                groups_to_heap(&crate::vol_fog::shader_source()),
            ),
            ("upscale", picture_wgsl(include_str!("../upscale.wgsl"))),
            ("overlay", picture_wgsl(include_str!("../overlay.wgsl"))),
            ("xr ui", picture_wgsl(include_str!("../xr_ui.wgsl"))),
            (
                "fsr1",
                picture_wgsl(&crate::shaders::fsr1_source_for(
                    mx::TextureFormat::Rgba8UnormSrgb,
                    false,
                )),
            ),
        ];
        for (name, src) in sources {
            let full = crate::shaders::with_heap(&src);
            let module = naga::front::wgsl::parse_str(&full)
                .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&full)));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert!(
                !src.lines()
                    .any(|l| l.trim_start().starts_with("@group(0)")
                        && !l.contains("texture_storage")),
                "{name}: a root group is left"
            );
        }
    }
}
