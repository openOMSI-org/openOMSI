//! Vehicle material and texture sync, and the D3D material mapping.
use super::*;

/// Give the `[smoothskin]` meshes of a vehicle instance copies of their own: their vertices
/// are rewritten as the joint turns, which a mesh shared between every instance of the type
/// (the AI pool, and the player's own set before this ran) cannot be. Called for the
/// player's vehicle and for every AI copy alike (`render.set` says which).
pub(super) fn own_skinned_meshes(
    renderer: &Renderer,
    scene: &mut Scene,
    vt: &omsi_sim::VehicleType,
    render: &mut VehicleRender,
) {
    for (i, vm) in vt.meshes.iter().enumerate() {
        if vm.skin.is_empty() || vm.data.positions.is_empty() {
            continue;
        }
        let Some(&inst) = render.instances.get(i) else {
            continue;
        };
        // (a mesh OMSI_ONLY_MESH / OMSI_HIDE_MESH left out stays out)
        if scene
            .meshes
            .get(scene.instances[inst].mesh)
            .map(|m| m.ranges.is_empty())
            .unwrap_or(true)
        {
            continue;
        }
        let id = renderer.add_mesh(scene, &vm.data);
        renderer.set_instance_mesh(scene, inst, id);
        render.skinned.push((i, id, Vec::new()));
    }
    if !render.skinned.is_empty() {
        let names = render
            .skinned
            .iter()
            .map(|(i, _, _)| vt.model.meshes[vt.meshes[*i].def_index].file.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let name = vt
            .def
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        // the player's own vehicle says so once; an AI copy (the timetable's articulated
        // buses spawn dozens of these) only on demand
        match render.set {
            None => log::info!("{name}: {} skinned meshes ({names})", render.skinned.len()),
            Some(_) => log::debug!(
                "{name} (AI): {} skinned meshes ({names})",
                render.skinned.len()
            ),
        }
    }
}

/// Distance (m) up to which the `[htmltexture]` pages of scenery objects are kept running.
pub const HTML_OBJECT_NEAR: f64 = 60.0;

/// Distance (m) beyond which what a vehicle's scripts redraw is uploaded only every half
/// second (the picture itself stays: see `Traffic::sync`).
pub const DISPLAYS_FAR: f64 = 50.0;

/// A texture name that stands for "no texture": exporters write `null.bmp` into slots
/// that have none (the SD202's IBIS key click spots). The slot shows its material colour;
/// nothing is looked up, and nothing is reported missing.
pub fn is_null_texture(name: &str) -> bool {
    let n = name.trim();
    n.is_empty()
        || Path::new(&n.replace('\\', "/"))
            .file_stem()
            .is_some_and(|s| s.eq_ignore_ascii_case("null"))
}

pub(super) fn scenery_texture_key(name: &str) -> String {
    name.trim().replace('\\', "/").to_ascii_lowercase()
}

pub(super) fn scenery_texture_selection(
    ot: &ObjectType,
    var: &dyn Fn(&str) -> Option<f32>,
) -> Vec<usize> {
    ot.dynamic_textures
        .iter()
        .map(|group| {
            let Some(value) = var(&group.variable) else {
                return usize::MAX;
            };
            if !value.is_finite() || value < 0.0 {
                return usize::MAX;
            }
            let index = value.trunc() as usize;
            if index < group.choices.len() {
                index
            } else {
                usize::MAX
            }
        })
        .collect()
}

/// The Direct3D material of a slot as OMSI sets it: a `[matl_allcolor]` (diffuse rgba,
/// ambient rgb, specular rgb, emissive rgb, power) replaces the o3d file's material. Returns (diffuse colour, emissive colour, specular colour and power).
/// The diffuse colour modulates the texture; its alpha only counts where there is no
/// texture (with one, the texture's alpha is used alone, as D3D's default stage does).
/// The emissive colour lights the texture by itself (the NL202's interior display, the
/// lamps of a traffic light); the specular term is the sun's highlight.
pub(super) fn d3d_material(
    m: &omsi_o3d::Material,
    allcolor: Option<[f32; 14]>,
    textured: bool,
) -> ([f32; 4], [f32; 3], [f32; 4], [f32; 3]) {
    // (the ambient colour: Omsi.exe gives every o3d slot a white one, 0x7c62f8, and a
    // textured .x slot too, 0x7c6d2d; a [matl_allcolor] sets its own)
    let (diffuse, emissive, specular, power, ambient) = match allcolor {
        Some(v) => (
            [v[0], v[1], v[2], v[3]],
            [v[10], v[11], v[12]],
            [v[7], v[8], v[9]],
            v[13],
            [v[4], v[5], v[6]],
        ),
        None => (m.diffuse, m.emissive, m.specular, m.specular_power, [1.0; 3]),
    };
    let clamp01 = |x: f32| {
        if x.is_finite() {
            x.clamp(0.0, 1.0)
        } else {
            0.0
        }
    };
    let color = [
        clamp01(diffuse[0]),
        clamp01(diffuse[1]),
        clamp01(diffuse[2]),
        if textured { 1.0 } else { clamp01(diffuse[3]) },
    ];
    let emissive = emissive.map(clamp01);
    let specular = specular.map(clamp01);
    // D3D ignores the specular colour without a power to raise the highlight to
    let power = if power.is_finite() && power >= 1.0 && specular.iter().any(|c| *c > 0.004) {
        power.min(256.0)
    } else {
        0.0
    };
    (
        color,
        emissive,
        [specular[0], specular[1], specular[2], power],
        ambient.map(clamp01),
    )
}

/// The material manager's depth and reflection settings of a slot's `[matl]` commands
/// (`bump`: the loaded `[matl_bumpmap]` height map and its factor).
pub(super) fn material_extra(
    ov: &[&MaterialDef],
    env_mask: Option<TextureId>,
    bump: Option<(TextureId, f32)>,
    specular: [f32; 4],
) -> MaterialExtra {
    MaterialExtra {
        env_mask,
        no_z_write: ov.iter().any(|o| o.no_z_write),
        writes_depth: false,
        // `[matl_noZcheck]` leaves Omsi.exe's depth test on: its draw of the slot (0x7fd6c4)
        // never reads the flag, which only adds a colourless stencil pass marking the panes
        // for the raindrops (0x7c32c4 -> 0x7fc58c, ZENABLE 1, blend ZERO/ONE). Taken as "no
        // depth test", the Sprinter's inner window glass (flagged so) was drawn over the
        // body skin round every opening. OMSI_NOZCHECK_BIAS=1: the old reading.
        no_z_check: ov.iter().any(|o| o.no_z_check) && omsi_cfg::flags::OMSI_NOZCHECK_BIAS.is_set(),
        z_bias: ov.iter().map(|o| o.z_bias).find(|b| *b != 0).unwrap_or(0),
        ambient: None,
        specular,
        bump: bump.filter(|b| b.1.is_finite() && b.1 != 0.0),
        glass: false,
        night_switched: false,
        night_alpha: false,
        rain_film: false,
        water: false,
        display: false,
        screen: false,
        // (a scenery object's slot is an LED panel only by the modeller's word)
        led: led_glow_effect(ov).is_some_and(|g| g > 0.0),
        led_sign: false,
        no_map_lights: false,
        tree: false,
        sway: None,
        moisture: 0.0,
        transmap_declared: ov.iter().any(|o| o.transmap.is_some()),
        // (the last addressing command of the slot decides; the colour is given in bytes)
        border: ov
            .iter()
            .rev()
            .find(|o| o.tex_address != omsi_model::TexAddress::Wrap)
            .filter(|o| o.tex_address == omsi_model::TexAddress::Border)
            .map(|o| o.border_color.map(|c| (c / 255.0).clamp(0.0, 1.0))),
        metal_ok: false,
        solid: false,
        alpha_cut: false,
        glow: glow_strength(ov),
        led_glow: led_glow_effect(ov),
    }
}

/// `[led_glow_effect] <0..1>` of a slot (an openOMSI extension, see
/// `MaterialExtra::led_glow`): the last of its commands counts, as with the others.
pub(super) fn led_glow_effect(ov: &[&MaterialDef]) -> Option<f32> {
    ov.iter().rev().find_map(|o| o.led_glow)
}

/// The LED panel guess (`MaterialExtra::led`) as the modeller's `[led_glow_effect]` has it,
/// where the slot has one: a value over 0 makes the slot an LED panel glowing with that
/// share, 0 makes it none.
pub(super) fn apply_led_glow_effect(extra: &mut MaterialExtra, ov: &[&MaterialDef]) {
    if let Some(g) = led_glow_effect(ov) {
        extra.led = g > 0.0;
        extra.led_glow = Some(g);
    }
}

/// `[matl_glow] <texture> <value>` (an openOMSI extension, see `MaterialExtra::glow`): the
/// mask a slot's commands bind in the light map's slot. A `[matl_lightmap]` of the slot keeps
/// that slot, and the glow is left out; the last `[matl_glow]` of the slot counts, as the
/// other `[matl_*]` commands do.
pub(super) fn glow_mask<'a>(ov: &[&'a MaterialDef]) -> Option<&'a str> {
    if ov.iter().any(|o| o.lightmap.is_some()) {
        return None;
    }
    ov.iter().rev().find_map(|o| o.glow.as_ref()).map(|g| g.0.as_str())
}

/// The strength of `glow_mask`'s glow in the shader's terms: the .cfg value x0.25, the
/// `Led glow` setting's own levels (6 its default); 0 without one, and for a negative value
/// (a light that would be taken away).
fn glow_strength(ov: &[&MaterialDef]) -> f32 {
    if glow_mask(ov).is_none() {
        return 0.0;
    }
    ov.iter().rev().find_map(|o| o.glow.as_ref()).map_or(0.0, |g| (g.1 * 0.25).max(0.0))
}

/// Whether a `[matl_item]`'s commands give it a light map's slot of its own (a light map or
/// a glow); without one it keeps its base material's.
pub(super) fn own_light_slot(ov: &[&MaterialDef]) -> bool {
    ov.iter().any(|o| o.lightmap.is_some() || o.glow.is_some())
}

/// The addressing of a slot's textures: its last `[matl_texadress_*]` command decides (the
/// border mode is clamped, its colour comes with `material_extra`).
pub(super) fn tex_addressing<'a>(ov: impl DoubleEndedIterator<Item = &'a MaterialDef>) -> omsi_render::TexAddressing {
    use omsi_model::TexAddress as A;
    use omsi_render::TexAddressing as R;
    match ov.rev().map(|o| o.tex_address).find(|a| *a != A::Wrap) {
        None | Some(A::Wrap) => R::Wrap,
        Some(A::Mirror) => R::Mirror,
        Some(A::Clamp | A::Border) => R::Clamp,
        Some(A::MirrorOnce) => R::MirrorOnce,
    }
}

/// The key a `[matl_bumpmap]` height map of `path` is kept under (the same file may be a
/// colour texture as well).
pub(super) fn bump_key(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}#bump", path.display()))
}

/// Whether a texture file is a season's snow picture: it lies in a `WinterSnow` folder
/// (`Texture\WinterSnow\gras.bmp`, any case), where the snow weather finds the map's
/// snowy textures, or in a `WinterSnowfall` one, its snowy roads.
pub(super) fn is_snow_picture(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str().to_str().is_some_and(|s| s.eq_ignore_ascii_case("WinterSnow") || s.eq_ignore_ascii_case("WinterSnowfall")))
}

/// A PBR set beside the diffuse texture `path` (`foo_n.png` and the rest, see
/// `omsi_texture::pbr`), put up and tied to texture `id` for the materials made with it.
pub fn attach_pbr(renderer: &Renderer, scene: &mut Scene, path: &Path, id: TextureId) {
    // (and a season's snow picture is known as one: it gets no snow laid over it, #879)
    if is_snow_picture(path) {
        scene.snow_textures.insert(id);
    }
    if omsi_cfg::flags::OMSI_NO_PBR.is_set() {
        return;
    }
    let files = omsi_texture::pbr::find(path);
    if files.is_empty() {
        return;
    }
    if let Some(set) = omsi_texture::pbr::load_set(&files) {
        log::info!("PBR maps for {}: normal {:?}, occlusion/roughness/metal {:?}", path.display(), set.normal.as_ref().map(|i| (i.width, i.height)), set.flags);
        renderer.add_pbr_maps(scene, id, &set);
    }
}

/// The texture data for a key of the texture maps: a file, or a file's bump height map.
pub(super) fn load_texture_key(key: &Path, compress: bool) -> Option<TextureData> {
    let k = key.to_string_lossy();
    match k.strip_suffix("#bump") {
        Some(file) => omsi_texture::decode_file(Path::new(file))
            .map_err(|e| log::warn!("{e}"))
            .ok()
            .map(|img| omsi_texture::gpu::prepare_bump(&img, compress)),
        None => {
            if compress {
                omsi_texture::gpu::load_gpu(key).ok().map(|t| t.0)
            } else {
                omsi_texture::gpu::load_gpu_fast(key).ok().map(|t| t.0)
            }
        }
    }
}

/// Decide the alpha mode of an o3d material from the model.cfg `[matl]` overrides.
pub fn material_alpha(
    materials: &[omsi_o3d::Material],
    slot: usize,
    overrides: &[MaterialDef],
) -> AlphaMode {
    // the plain [matl] overrides of this slot decide; without one: opaque. A
    // `[matl_change]` record only opens the variants (`[matl_item]`) and says nothing of the
    // slot's own look: a `[matl]` of the same slot after it does. (The LED matrices of
    // churaPixel/Krüger++ open a change first and give the slot `[matl_alpha] 2` and the
    // script texture as its mask in a `[matl]` after it: taken as opaque from the change,
    // the mask cut nothing and the whole panel was lit.)
    // Several plain [matl] of one slot are one material in OMSI: each selects it again and
    // the commands after it modify it, so the last `[matl_alpha]` among them counts. (Taken
    // from the first block alone, an alpha-tested texture whose `[matl_alpha]` sits in a
    // second [matl] was drawn opaque, its transparent parts as solid areas.) omsi-model
    // already joins blocks spelt the same; this covers those that reach the slot otherwise
    // (an index of -1 selects the first one, as 0 does).
    let mine: Vec<&MaterialDef> = overrides.iter().filter(|o| !o.item && omsi_sim::vehicle::override_slot(materials, o) == Some(slot)).collect();
    let plain = || mine.iter().filter(|o| o.change.is_none());
    plain()
        .rev()
        .find(|o| o.alpha_set)
        .or_else(|| plain().next())
        .or(mine.first())
        .map(|o| alpha_mode(o.alpha))
        .unwrap_or(AlphaMode::Opaque)
}

pub(super) fn alpha_mode(a: i32) -> AlphaMode {
    match a {
        0 => AlphaMode::Opaque,
        1 => AlphaMode::Test,
        _ => AlphaMode::Blend,
    }
}

/// Whether a vehicle's `[useTextTexture]` slot is a display (`MaterialExtra::display`): one
/// that has a light of its own, a light map or a night map (a destination matrix, a
/// counter lit with the dashboard), not lettering on the body.
pub(super) fn text_is_display(lightmap: bool, night: bool) -> bool {
    lightmap || night
}

/// How a `[texttexture]` shows on its slot: alpha tested where the slot's `[matl_alpha]` is 1
/// (the stock route helpers, `routearrows_busstop.sco`: blended, the empty part of the text
/// wrote depth and cut away whatever was drawn behind it later - a bus beside the stop lost
/// half its roof), blended otherwise.
pub(super) fn text_alpha(materials: &[omsi_o3d::Material], slot: usize, overrides: &[MaterialDef]) -> AlphaMode {
    match material_alpha(materials, slot, overrides) {
        AlphaMode::Test => AlphaMode::Test,
        _ => AlphaMode::Blend,
    }
}

/// Placement is part of the picture: otherwise a centred sign can lend its cached
/// texture to a left-aligned one showing the same words.
/// A text texture's material, addressed as its slot is: `[matl_texadress_border]` (and
/// clamp, mirror) on the slot carry over to the text drawn into it. Made with the plain
/// wrap, Road-Hog123's bus stop flags - a second mesh whose UVs run past the text texture
/// into its border - showed the stop name repeated across the whole flag (#1645).
/// The slot's own emissive colour goes with it (`slot_mat`, its o3d material, or its
/// `[matl_allcolor]`): Omsi.exe only swaps the slot's texture and keeps its material, so a
/// sign whose slot glows - a route arrow's caption - glows with its text, and one that does
/// not stays dark at night. Made without it, the arrows' street and stop names went black
/// after dusk on arrows that shone.
pub(super) fn text_material(renderer: &Renderer, scene: &mut Scene, tex: TextureId, alpha: AlphaMode, slot_ov: &[&MaterialDef], slot_mat: Option<&omsi_o3d::Material>) -> MaterialId {
    let extra = material_extra(slot_ov, None, None, [0.0; 4]);
    let plain = MaterialExtra::default();
    let border_only = MaterialExtra { border: extra.border, ..plain.clone() };
    renderer.address_next.set(tex_addressing(slot_ov.iter().copied()));
    let emissive = text_emissive(slot_ov, slot_mat);
    renderer.add_material_extra(scene, Some(tex), alpha, [1.0; 4], false, None, None, None, None, emissive, border_only)
}

/// The emissive colour of a text texture's slot (see `text_material`).
pub(super) fn text_emissive(slot_ov: &[&MaterialDef], slot_mat: Option<&omsi_o3d::Material>) -> [f32; 3] {
    slot_mat.map(|m| d3d_material(m, slot_ov.iter().find_map(|o| o.allcolor), true).1).unwrap_or([0.0; 3])
}

/// The key of a text texture material: the text's own key, how its slot addresses it and
/// the slot's glow.
pub(super) fn text_material_key(base: String, slot_ov: &[&MaterialDef], slot_mat: Option<&omsi_o3d::Material>) -> String {
    let border = material_extra(slot_ov, None, None, [0.0; 4]).border;
    format!("{base}|{:?}|{border:?}|{:?}", tex_addressing(slot_ov.iter().copied()), text_emissive(slot_ov, slot_mat))
}

pub(super) fn scenery_text_key(tt: &omsi_model::TextTexture, text: &str, alpha: AlphaMode) -> String {
    format!(
        "{}|{}|{}x{}|{}|{:?}|{:?}|{}|{}",
        tt.font.to_ascii_lowercase(),
        text,
        tt.width.max(1),
        tt.height.max(1),
        tt.full_color,
        tt.color,
        alpha,
        tt.orientation,
        tt.grid,
    )
}

pub(super) fn scenery_text_image(
    tt: &omsi_model::TextTexture,
    atlas: Option<Arc<omsi_content::font::FontAtlas>>,
    text: &str,
) -> Image {
    // Static and scripted text textures use the placement from their definition.
    let state = omsi_sim::texttex::TextTextureState::new(tt.clone(), atlas);
    Image {
        width: tt.width.max(1) as u32,
        height: tt.height.max(1) as u32,
        rgba: state.image(text),
        has_alpha: true,
    }
}

/// The text of one of the game's own helper objects (the route arrows' street and stop
/// names) that its `.oft` font cannot draw: the stock arrows ask for the font "test"
/// (`Fonts/test1.oft`), which has the Latin letters and German umlauts only, so a street
/// or a stop named in Cyrillic (or Greek, Chinese ...) came out as an empty arrow - at
/// most a stray `Ä` where a code page variant of `Д` happened to be in the font. Such a
/// text is drawn with the interface font (Inter, then the system's fonts for the
/// scripts it lacks) in the texture's colour, the height of the `.oft` font's letters,
/// centred, and narrowed to the texture's width. None when the font draws every letter:
/// that text keeps OMSI's own look.
pub(super) fn helper_text_image(tt: &omsi_model::TextTexture, atlas: Option<&omsi_content::font::FontAtlas>, text: &str) -> Option<Image> {
    let drawable = |c: char| c == '@' || c.is_whitespace() || atlas.is_some_and(|a| a.font.glyph(c).is_some());
    if text.trim().is_empty() || text.chars().all(drawable) {
        return None;
    }
    // (global: the interface fonts are the same for every map; built once, on first use)
    static FONTS: std::sync::OnceLock<omsi_ui::Fonts> = std::sync::OnceLock::new();
    let fonts = FONTS.get_or_init(omsi_ui::Fonts::new);
    let (w, h) = (tt.width.max(1) as u32, tt.height.max(1) as u32);
    // `@` starts a new line, as in OMSI's own text textures (#1553, #1482): drawn as a
    // letter, a Polish stop's "622@Sosnowiec@Urząd@Miasta" stood in one line with its
    // at signs, and a Chinese one was squeezed into a sliver. (A text starting with `@`
    // keeps its empty first line.)
    let text = text.trim_end();
    let lines: Vec<&str> = text.split('@').map(str::trim).collect();
    let n = lines.len().max(1) as f32;
    // (the .oft's line height holds its capitals and the gap below them; Inter's capitals
    // are 0.7 of its size, so nearly the line height gives letters of the same height)
    let line = atlas.map(|a| a.font.height.max(8) as f32).unwrap_or(h as f32 * 0.2);
    // the lines share the texture's height when they would not fit at the font's own
    let mut pitch = line.min(h as f32 / n);
    // A line too long for the texture is made smaller first, letters and all, down to two
    // thirds of the height, and only what is still too wide is narrowed: narrowed alone, a
    // long Polish street name came out in thin sticks nobody could read (#1696, #1365).
    let widest = lines.iter().filter(|l| !l.is_empty()).map(|l| fonts.render(l, pitch * 0.95, omsi_ui::Weight::Medium).w).max().unwrap_or(0);
    if widest > w {
        pitch *= (w as f32 / widest as f32).max(2.0 / 3.0);
    }
    let px = pitch * 0.95;
    let top = (h as f32 - pitch * n) * 0.5;
    let rgb = if tt.full_color { [255u8; 3] } else { [tt.color[0] as u8, tt.color[1] as u8, tt.color[2] as u8] };
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for (k, l) in lines.iter().enumerate() {
        if l.is_empty() {
            continue;
        }
        let bmp = fonts.render(l, px, omsi_ui::Weight::Medium);
        if bmp.w == 0 || bmp.h == 0 {
            continue;
        }
        // too long for the texture: narrowed to fit (columns sampled), the height kept
        let scale = (w as f32 / bmp.w as f32).min(1.0);
        let out_w = ((bmp.w as f32 * scale).floor() as u32).max(1);
        let x0 = (w - out_w.min(w)) / 2;
        let y0 = (top + pitch * k as f32 + (pitch - bmp.h as f32) * 0.5).round() as i32;
        for y in 0..bmp.h as i32 {
            let dy = y0 + y;
            if dy < 0 || dy >= h as i32 {
                continue;
            }
            for x in 0..out_w.min(w) {
                let sx = ((x as f32 + 0.5) / scale) as u32;
                let a = bmp.alpha[(y as u32 * bmp.w + sx.min(bmp.w - 1)) as usize];
                if a == 0 {
                    continue;
                }
                let i = ((dy as u32 * w + x0 + x) * 4) as usize;
                rgba[i..i + 3].copy_from_slice(&rgb);
                rgba[i + 3] = rgba[i + 3].max(a);
            }
        }
    }
    Some(Image { width: w, height: h, rgba, has_alpha: true })
}

/// Identify a solid vehicle body material that should participate in the depth buffer.
/// Some bus packs put either `[matl_alpha] 2` or `[matl_noZcheck]` on a complete body mesh.
/// The decision must not depend on one creator's language or on a particular bus name:
/// use the model metadata and the material's actual mesh volume, while keeping thin glass
/// and explicit overlay/transparency materials on their authored paths.
/// Words that name a pane of glass in a mesh or texture file, in the languages OMSI's
/// add-ons are made in. The body-depth repair must not turn one of these opaque when the
/// model.cfg declares it blended: a Czech bus's `okna.o3d` (windows) on the shared
/// `body.png` was drawn as a black wall, where OMSI shows the tinted glass.
pub(super) const GLASS_WORDS: [&str; 24] = [
    "window", "fenster", "glas", "scheibe", "windshield", "windscreen", // en, de ("glas" is also German: `Leuchtmelderglas.tga`)
    "okn", "sklo", // cs, sk (okna, okno, sklo)
    "szyb", "okien", // pl
    "ablak", // hu
    "steklo", // ru (transliterated)
    "vitre", "fenetre", // fr
    "vetro", "finestr", // it
    "raam", "ruit", // nl
    "ventan", "cristal", // es
    "vidro", "janel", "parabrisa", "para-brisa", // pt (vidro, vidros, janela, janelas)
];

pub(super) fn is_vehicle_body_material(
    mesh_file: &str,
    texture: &str,
    has_texture: bool,
    has_transmap: bool,
    no_z_write: bool,
    body_hint: bool,
) -> bool {
    if !has_texture || has_transmap || no_z_write || !body_hint {
        return false;
    }
    let name = format!("{} {}", mesh_file, texture).to_ascii_lowercase();
    let overlay = ["regen", "dirt", "dreck", "wiper", "matrix", "display", "shadow"];
    if GLASS_WORDS.iter().chain(overlay.iter()).any(|part| name.contains(part)) {
        return false;
    }
    true
}

/// The texture whose alpha is a blended slot's coverage: its `[matl_transmap]` file where
/// it has one (the diffuse alpha is then the reflection mask), else the diffuse texture. A
/// script texture as the map (`\S:n`) is not known at load time: the diffuse texture then.
pub(super) fn coverage_texture<'a>(transmap: Option<&'a str>, diffuse: &'a str) -> &'a str {
    match transmap.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) if !t.starts_with("\\S:") => t,
        _ => diffuse,
    }
}

/// Side of the square a texture's alpha is kept at for [`slot_is_see_through`].
pub(super) const ALPHA_MASK: usize = 256;

/// A texture's alpha channel, thinned out to [`ALPHA_MASK`] squared (a body texture is
/// 4096 squared, and every blended slot of a bus asks). None for a file that cannot be
/// read or has no alpha.
pub(super) fn alpha_mask(path: &Path) -> Option<Arc<Vec<u8>>> {
    // (global, not a field of `World`: a memo of what the file holds, the same whatever map
    // is open, so a new `World` reads nothing twice)
    static MASKS: std::sync::OnceLock<Mutex<HashMap<PathBuf, Option<Arc<Vec<u8>>>>>> = std::sync::OnceLock::new();
    let masks = MASKS.get_or_init(Default::default);
    if let Some(m) = masks.lock().get(path) {
        return m.clone();
    }
    let mask = omsi_texture::decode_file(path).ok().filter(|img| img.has_alpha && img.width > 0 && img.height > 0).map(|img| {
        let (w, h) = (img.width as usize, img.height as usize);
        let mut out = vec![255u8; ALPHA_MASK * ALPHA_MASK];
        for y in 0..ALPHA_MASK {
            for x in 0..ALPHA_MASK {
                let (sx, sy) = ((x * w / ALPHA_MASK).min(w - 1), (y * h / ALPHA_MASK).min(h - 1));
                out[y * ALPHA_MASK + x] = img.rgba[(sy * w + sx) * 4 + 3];
            }
        }
        Arc::new(out)
    });
    masks.lock().insert(path.to_path_buf(), mask.clone());
    mask
}

/// Whether the triangles of material `slot` lie on a see-through part of their texture
/// (`mask`, see [`alpha_mask`]): two in three of them with an alpha under 0.9 at their
/// middle. A pane does - the SOR NB12's glass is 62 of 255 on its body texture; a door or
/// a body panel blended by `[matl_alpha] 2` has its paint at 255 and does not.
pub(super) fn slot_is_see_through(mesh: &MeshData, slot: usize, mask: &[u8]) -> bool {
    let (mut clear, mut all) = (0usize, 0usize);
    for &(first, count, material) in &mesh.ranges {
        if material as usize != slot {
            continue;
        }
        let start = first as usize;
        let end = start.saturating_add(count as usize).min(mesh.indices.len());
        for tri in mesh.indices.get(start..end).unwrap_or_default().chunks_exact(3) {
            let Some(uv) = tri.iter().map(|&i| mesh.uvs.get(i as usize).copied()).sum::<Option<glam::Vec2>>() else { continue };
            let uv = uv / 3.0;
            let at = |t: f32| ((t.rem_euclid(1.0) * ALPHA_MASK as f32) as usize).min(ALPHA_MASK - 1);
            all += 1;
            if mask[at(uv.y) * ALPHA_MASK + at(uv.x)] < 230 {
                clear += 1;
            }
        }
    }
    // (two thirds: a layer over the windows that painters draw on - the glass's own unwrap
    // in the paint scheme, its adverts and tint - is covered where the picture is, and is
    // a pane all the same)
    all > 0 && clear * 3 >= all * 2
}

/// Whether a texture (`mask`, see [`alpha_mask`]) is clear all over: no texel above 8 of
/// 255. A slot blended by `[matl_alpha] 2` with such a texture is drawn for its depth alone
/// - an "invisible cover" bus makers put in front of a roller blind or a window to hide it
/// (a 16x16 clear `.tga`, #1113, #576): Omsi.exe writes its depth in model order, and what
/// the model lists after it behind it is gone.
pub(super) fn texture_is_clear(mask: &[u8]) -> bool {
    !mask.is_empty() && mask.iter().all(|&a| a <= 8)
}

/// Whether an alpha-tested texture (`mask`, see [`alpha_mask`]) is a tint: a quarter of it
/// partly clear above Omsi.exe's cut (129 to 254) and nearly nothing that the cut throws
/// away (1 to 128), like the darkened glass a livery paints on a window layer (GPM C2's
/// `741_vitres.tga`, black at 140). Omsi.exe blends what passes its alpha test; cut out
/// and drawn solid, the windows went black.
pub(super) fn texture_is_tint(mask: &[u8]) -> bool {
    let tinted = mask.iter().filter(|&&a| a > 128 && a < 255).count();
    let cut = mask.iter().filter(|&&a| a > 0 && a <= 128).count();
    !mask.is_empty() && tinted * 4 >= mask.len() && cut * 100 <= mask.len()
}

/// Whether the triangles of an alpha-tested material `slot` lie on a tint of their texture
/// (`mask`, see [`alpha_mask`]): a tenth of them (and two at least) at an alpha that passes
/// Omsi.exe's cut but is clearly see-through (129 to 240) at their middle. A livery's window
/// layer often shares its texture with the body and the clear glass (alpha 128 and less,
/// the NefAZ 5299 2024), so [`texture_is_tint`] over the whole picture misses it - its panes
/// were drawn solid black (#2609, #2624). Cut-out pictograms and seat covers are solid
/// (255) where their faces are.
pub(super) fn slot_is_tint(mesh: &MeshData, slot: usize, mask: &[u8]) -> bool {
    let (mut tint, mut all) = (0usize, 0usize);
    for &(first, count, material) in &mesh.ranges {
        if material as usize != slot {
            continue;
        }
        let start = first as usize;
        let end = start.saturating_add(count as usize).min(mesh.indices.len());
        for tri in mesh.indices.get(start..end).unwrap_or_default().chunks_exact(3) {
            let Some(uv) = tri.iter().map(|&i| mesh.uvs.get(i as usize).copied()).sum::<Option<glam::Vec2>>() else { continue };
            let uv = uv / 3.0;
            let at = |t: f32| ((t.rem_euclid(1.0) * ALPHA_MASK as f32) as usize).min(ALPHA_MASK - 1);
            all += 1;
            if (129..=240).contains(&mask[at(uv.y) * ALPHA_MASK + at(uv.x)]) {
                tint += 1;
            }
        }
    }
    tint >= 2 && tint * 10 >= all
}

/// Return whether the triangles of one material occupy a volumetric part of the vehicle.
/// Windows and rain films are normally very thin sheets; this lets unnamed/modded body
/// meshes be repaired without maintaining a language-specific list of mesh names.
pub(super) fn material_has_vehicle_volume(mesh: &MeshData, slot: usize) -> bool {
    let mut lo = glam::Vec3::splat(f32::MAX);
    let mut hi = glam::Vec3::splat(f32::MIN);
    let mut points = 0usize;
    for &(first, count, material) in &mesh.ranges {
        if material as usize != slot {
            continue;
        }
        let start = first as usize;
        let end = start.saturating_add(count as usize).min(mesh.indices.len());
        for &index in mesh.indices.get(start..end).unwrap_or_default() {
            if let Some(p) = mesh.positions.get(index as usize) {
                lo = lo.min(*p);
                hi = hi.max(*p);
                points += 1;
            }
        }
    }
    if points < 8 || !lo.x.is_finite() || !hi.x.is_finite() {
        return false;
    }
    let extent = hi - lo;
    let mut sides = [extent.x.abs(), extent.y.abs(), extent.z.abs()];
    sides.sort_by(f32::total_cmp);
    // A solid shell has meaningful thickness compared with both of its other dimensions.
    // A windshield or side pane may be wide and tall, but remains a sheet in its thin axis.
    sides[2] > 3.0
        && sides[1] > 1.0
        && sides[0] > 0.5
        && sides[0] / sides[1] > 0.25
        && sides[0] / sides[2] > 0.02
}

/// Whether the triangles of material `slot` lie on the faces of another slot of the same
/// mesh: a layer modelled as a copy of the surface under it with a material of its own (a
/// baked ambient-occlusion or shading film over the floor, `[matl_alpha] 2`), which OMSI
/// blends over the surface as declared.
pub(super) fn slot_overlays_another(mesh: &MeshData, slot: usize) -> bool {
    let key = |p: &glam::Vec3| ((p.x * 1000.0).round() as i32, (p.y * 1000.0).round() as i32, (p.z * 1000.0).round() as i32);
    let mut own = std::collections::HashSet::new();
    let mut others = std::collections::HashSet::new();
    for &(first, count, material) in &mesh.ranges {
        let start = first as usize;
        let end = start.saturating_add(count as usize).min(mesh.indices.len());
        let set = if material as usize == slot { &mut own } else { &mut others };
        for &index in mesh.indices.get(start..end).unwrap_or_default() {
            if let Some(p) = mesh.positions.get(index as usize) {
                set.insert(key(p));
            }
        }
    }
    own.len() >= 3 && own.iter().filter(|k| others.contains(*k)).count() * 10 >= own.len() * 9
}
