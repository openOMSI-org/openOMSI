//! Materials: their uniform, maps and bind groups, the many ways the scene adds them, and
//! the bind group a material's textures and parameters make.

use crate::*;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct MaterialUniform {
    pub(crate) color: [f32; 4],
    pub(crate) params: [f32; 4],
    pub(crate) extra: [f32; 4],
    pub(crate) params2: [f32; 4],
    /// rgb: emissive colour; w: an explicitly identified transparent glass layer
    pub(crate) emissive: [f32; 4],
    pub(crate) specular: [f32; 4],
    /// x: `[matl_bumpmap]` factor, y: has a bump map, z/w: noZwrite/noZcheck
    pub(crate) bump: [f32; 4],
    /// The PBR maps beside the diffuse texture (`Scene::pbr_maps`): x has a normal map,
    /// y an occlusion, z a roughness, w a metalness channel.
    pub(crate) pbr: [f32; 4],
    /// x: a screen (`MaterialExtra::screen`); y: 1 `[matl_texadress_border]`, 2
    /// `[matl_texadress_mirroronce]`; z the border colour's rgb packed as r * 65536 + g * 256 + b (bytes), w its alpha.
    pub(crate) flags: [f32; 4],
    /// rgb: the D3D material's ambient colour, which takes the ambient light (C); w: 1 for
    /// a texture that is a season's snow picture (no snow laid over it), 2 the map's water,
    /// -1 a slot without a texture
    pub(crate) ambient: [f32; 4],
    /// `MaterialExtra::sway`: x 1 for foliage the wind moves, y its pivot's and z its top's
    /// height (mesh units), w how much it gives to the wind
    pub(crate) sway: [f32; 4],
    /// Window mask: mesh X/Z origin and inverse size; zero disables it.
    pub(crate) wipe_bounds: [f32; 4],
    /// x: `[matl_glow]` strength (`MaterialExtra::glow`); 0 = the keyword is not there.
    /// y: LED glow scale; z: a pane faded by Envir_Brightness; w free.
    pub(crate) glow: [f32; 4],
}

/// The maps of a PBR set found beside a diffuse texture (`foo_n.png` and the rest, see
/// `omsi_texture::pbr`): a tangent-space normal map, and occlusion / roughness / metalness
/// packed into the red, green and blue of one texture.
#[derive(Debug, Clone, Copy)]
pub struct PbrMaps {
    pub normal: Option<TextureId>,
    pub orm: Option<TextureId>,
    /// x normal, y occlusion, z roughness, w metalness (1 = present)
    pub flags: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlphaMode {
    Opaque,
    Test,
    Blend,
}

/// How a material's textures read outside [0, 1]: Omsi.exe sets the material's
/// `[matl_texadress_*]` mode as ADDRESSU/ADDRESSV of all eight sampler stages (0x7fff70).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TexAddressing {
    /// Repeating, Direct3D's default.
    #[default]
    Wrap,
    /// `[matl_texadress_mirror]`: every other repeat mirrored.
    Mirror,
    /// `[matl_texadress_clamp]`, and `[matl_texadress_border]` (whose colour the shader
    /// puts outside, `MaterialExtra::border`).
    Clamp,
    /// `[matl_texadress_mirroronce]`: mirrored once about 0, then clamped (the shader takes
    /// the coordinates' absolute value under the clamping sampler).
    MirrorOnce,
}

/// What a material's bind group is made of: its textures (slot and generation), the
/// sampler and the uniform values.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct BindKey {
    pub(crate) textures: [(usize, u64); 7],
    pub(crate) address: TexAddressing,
    // 13 vec4: the MaterialUniform's own fields (see the count in
    // `shaders_validate_and_match_the_uniforms`)
    pub(crate) uniform: [u32; 52],
}

pub struct Material {
    pub texture: Option<TextureId>,
    pub alpha: AlphaMode,
    pub color: [f32; 4],
    pub unlit: bool,
    /// `[matl_noZwrite]`: a blended surface (glass, rain film, dirt) that must not write
    /// depth, or everything blended behind it is thrown away - which is what punched holes
    /// into the world seen through a window or a mirror.
    pub no_z_write: bool,
    /// See [`MaterialExtra::writes_depth`].
    pub writes_depth: bool,
    /// `[matl_noZcheck]`: a decal drawn over the surface it lies on - blended, without
    /// depth write, with the surfaces' depth bias (see the blended draw items).
    pub no_z_check: bool,
    /// `[matl_Zbias]`: a positive bias pulls a decal in front of the coplanar surface
    /// under it (drawn with the depth bias of the road surfaces).
    pub z_bias: i32,
    pub nightmap: Option<TextureId>,
    pub lightmap: Option<TextureId>,
    pub envmap: Option<(TextureId, f32)>,
    /// `[matl_envmap_mask]`: the reflection mask is this texture's alpha instead of the
    /// diffuse texture's.
    pub env_mask: Option<TextureId>,
    /// `[matl_bumpmap]` height map and factor.
    pub bump: Option<(TextureId, f32)>,
    pub emissive: [f32; 3],
    /// `[matl_transmap]` (texture, its alpha channel is used).
    pub transmap: Option<(TextureId, bool)>,
    /// Its textures' addressing (`[matl_texadress_*]`).
    pub(crate) address: TexAddressing,
    /// Keep the exact material parameters so a CTC texture swap can change only the diffuse
    /// map without losing map lighting, moisture, screen, or other renderer flags.
    pub(crate) uniform: MaterialUniform,
    pub(crate) buf: wgpu::Buffer,
    pub(crate) bind_group: wgpu::BindGroup,
    /// Materials that draw alike (same textures, sampler and values) share this number.
    pub(crate) look: u32,
}

impl Material {
    /// Whether the material samples texture `id`.
    pub fn uses_texture(&self, id: TextureId) -> bool {
        self.texture == Some(id)
            || self.nightmap == Some(id)
            || self.lightmap == Some(id)
            || self.envmap.map(|e| e.0) == Some(id)
            || self.transmap.map(|t| t.0) == Some(id)
            || self.env_mask == Some(id)
            || self.bump.map(|b| b.0) == Some(id)
    }

    /// `[matl_transmap]` was given, its file there or not (the shader's
    /// `has_transmap_declared`).
    pub fn transmap_declared(&self) -> bool {
        (self.uniform.params2[3] + 0.5) as u32 & 2 != 0
    }
}

/// The material manager's settings beyond the maps of `add_material_all`: depth handling,
/// the reflection mask and the o3d material's specular term.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MaterialExtra {
    /// `[matl_envmap_mask]`
    pub env_mask: Option<TextureId>,
    /// `[matl_noZwrite]`
    pub no_z_write: bool,
    /// A slot `no_z_write` marks as a see-through layer (a pane, a dirt film, a sticker
    /// on a window - for the glass shading and the shadow map) that the model.cfg does not
    /// give `[matl_noZwrite]`: Omsi.exe draws it with its depth written (0x7fd6c4 sets
    /// ZWRITEENABLE from that flag alone), and so is it drawn here. Stacked panes of a
    /// door or a window then hide each other in model order as in the original, instead
    /// of all being blended over each other whichever is in front (#211).
    pub writes_depth: bool,
    /// `[matl_noZcheck]`
    pub no_z_check: bool,
    /// `[matl_Zbias]`
    pub z_bias: i32,
    /// The D3D material's ambient colour, its share of the ambient light (C); None: the
    /// diffuse colour's.
    pub ambient: Option<[f32; 3]>,
    /// Specular colour (rgb) and power (w) of the D3D material; black = no highlight.
    pub specular: [f32; 4],
    /// `[matl_bumpmap]`: a height map (in its alpha, `Image::bump_height_map`) whose slope
    /// shifts the `[matl_envmap]` lookup, times the factor.
    pub bump: Option<(TextureId, f32)>,
    /// A named transparent window layer. This is separate from envmap/transmap because
    /// stock and add-on buses often use a plain alpha-blended window texture.
    pub glass: bool,
    /// `Envir_Brightness` fades the environment layer, not the physical pane.
    pub daylight_glass: bool,
    /// The night map is switched by something other than the time of day - a `[matl_item]`'s
    /// variable, a vehicle mesh's `[visible]`: it glows by day as well (warning lamps,
    /// dashboard displays), not only at night.
    pub night_switched: bool,
    /// A blended lamp with a completely clear diffuse placeholder takes its coverage
    /// from the active night texture. The unpowered material keeps its own alpha.
    pub night_alpha: bool,
    /// A display's text (`[useTextTexture]`): in the enhanced picture it glows a little
    /// by itself, as a lit matrix does, instead of taking only the light that reaches it
    /// under the bus's front overhang, where it was hardly readable by day.
    pub display: bool,
    /// A screen the bus draws itself - a `[useTextTexture]` or `[useScriptTexture]` slot:
    /// the IBIS, the matrix displays, the dashboard's LCDs. The enhanced picture's glow
    /// and FXAA leave it alone (see `MASK_FORMAT`): FXAA took half the contrast out of
    /// their letters and they read as blurred.
    /// Its mask remains identifiable through a clear pane so it cannot be blurred as mist.
    pub screen: bool,
    /// An LED matrix - a display whose lit dots are the `\S:n` script texture's
    /// (`[matl_transmap]`), the Krueger and K++ destination panels: the dots are the
    /// panel's own light, so the enhanced picture lets them burn in HDR and blooms them
    /// (the glow's source keeps them, where the other screens are left out of it -
    /// something no Direct3D 9 without shaders of its own could do). `MASK_FORMAT`'s g.
    /// (Only a panel whose `[matl_lightmap]` is white all over: a flipdot carries the same
    /// mask, but its light map is a picture of the lamps over it, and it does not glow.)
    pub led: bool,
    /// An LED panel drawn by a page (`[useHtmlTexture]`): a destination sign, made to be read
    /// from the street, so it keeps its brightness at night even where it sits in the cab
    /// (behind a coach's windscreen) - the cab's dimming at night is for the dashboard's LCDs.
    pub led_sign: bool,
    /// The film of water on a window (`[alphascale] Rain_Window_…`): drawn as drops that sit,
    /// gather and run down the glass instead of the texture sliding down as a whole.
    pub rain_film: bool,
    /// The map's water (`texture/water.tga`): Enhanced draws it as water - a smooth surface
    /// mirroring the sky more the flatter it is seen, rippled by small waves.
    pub water: bool,
    /// `[nomaplighting]`: the map's lamps (`[maplight]`) do not light it - a street lamp
    /// is not lit by its own light.
    pub no_map_lights: bool,
    /// A `[tree]`'s leaf cards: the vanilla picture leaves the map's lamps off them, as
    /// OMSI 2 shows a tree standing right under a street lamp dark; Vanilla+ and Enhanced
    /// still light them.
    pub tree: bool,
    /// 1 when the texture's `.cfg` sidecar carries `[moisture]`/`[puddles]`: the road of a
    /// junction or crossing object gets wet and collects puddles like a spline's.
    pub moisture: f32,
    /// `[matl_transmap]` was given, whether or not its file is there: Omsi.exe raises the
    /// material's transmap flag before it reads the name (0x7fbbf4), and with it the
    /// `[matl_envmap]` reflection goes by the texture's alpha instead of the factor.
    pub transmap_declared: bool,
    /// `[matl_texadress_border]`: its colour (RGBA, 0..1). Where the (scrolled) texture
    /// coordinates leave [0, 1] the diffuse texture reads this colour instead of its edge,
    /// as Direct3D's border addressing does: a roller blind's band that has scrolled away
    /// vanishes in a transparent border.
    pub border: Option<[f32; 4]>,
    /// An opaque, sphere-mapped part of a vehicle that is not its body (a handrail, a
    /// bumper, a wheel trim): the enhanced picture may make it metal by its `[matl_envmap]`
    /// factor alone, as the vanilla one shows the sphere map on it - chrome read as a
    /// faint clear coat there. A body needs a mask of its own for that (a Golf's bonnet).
    pub metal_ok: bool,
    /// A body - a person's skin, clothes and hair - whose cut-out alpha is no foliage: the
    /// enhanced picture lights it by its normals, not as leaves lit from every side and
    /// glowing with the sun through them (people shone in the sun).
    pub solid: bool,
    /// A blended slot that also keeps Omsi.exe's alpha test: `[matl_alpha] 1` sets
    /// ALPHATESTENABLE with ALPHAREF 0x80 and leaves ALPHABLENDENABLE as the draw before
    /// left it (0x7ffd6c), so a livery's tinted window glass (alpha 129 to 254) is blended
    /// and its clear glass (128 and less) cut away. Drawn solid, the tinted panes were black
    /// (#2609, #2624); blended without the cut, the clear glass was a grey film.
    pub alpha_cut: bool,
    /// Windy trees: foliage the wind moves - the height (mesh units) of the pivot where the
    /// crown leaves the trunk (nothing below it moves), of the crown's top, and how much the
    /// tree gives to the wind (1 a broadleaf). `None`: not foliage.
    pub sway: Option<[f32; 3]>,
    /// `[matl_glow] <texture> <value>` (an openOMSI extension): the material is its own
    /// light. The named picture - a greyscale mask of how much shines where - is bound in
    /// the light map's slot (the shaders sample it as `t_light`) and the light is the
    /// material's own colour; this is its strength in the shader's terms (the .cfg value
    /// x0.25, the LED panels' `Lighting::led_glow` scale; 0 = the keyword is not there).
    /// Only the enhanced picture draws it, as an emission like an LED panel's dots; its
    /// halo comes from its brightness as any light's does. `params2.x` is left off, so the
    /// classic picture draws the material as if the keyword were not there.
    pub glow: f32,
    /// `[led_glow_effect]` (an openOMSI extension): the share (0..1) of the `Led glow`
    /// setting an LED panel's dots burn with; `None` the whole of it. With the keyword
    /// the modeller has said whether the slot is an LED panel at all (`led` follows it),
    /// so this is only read where `led` is set.
    pub led_glow: Option<f32>,
}

/// The textures a material's bind group samples.
#[derive(Clone, Copy)]
pub(crate) struct MaterialMaps {
    pub(crate) texture: Option<TextureId>,
    pub(crate) transmap: Option<(TextureId, bool)>,
    pub(crate) nightmap: Option<TextureId>,
    pub(crate) lightmap: Option<TextureId>,
    pub(crate) envmap: Option<(TextureId, f32)>,
    pub(crate) env_mask: Option<TextureId>,
    pub(crate) bump: Option<(TextureId, f32)>,
    pub(crate) pbr: Option<PbrMaps>,
}

impl Renderer {
    /// `[matl_noZwrite]` for a material that is already in the scene.
    pub fn set_no_z_write(&self, scene: &mut Scene, id: MaterialId, on: bool) {
        if let Some(m) = scene.materials.get_mut(id) {
            m.no_z_write = on;
        }
    }

    pub fn add_material(
        &self,
        scene: &mut Scene,
        texture: Option<TextureId>,
        alpha: AlphaMode,
        color: [f32; 4],
        unlit: bool,
    ) -> MaterialId {
        self.add_material_ex(scene, texture, alpha, color, unlit, None)
    }

    /// Material with an optional transparency map `(texture, use_alpha_channel)`.
    pub fn add_material_ex(
        &self,
        scene: &mut Scene,
        texture: Option<TextureId>,
        alpha: AlphaMode,
        color: [f32; 4],
        unlit: bool,
        transmap: Option<(TextureId, bool)>,
    ) -> MaterialId {
        self.add_material_full(
            scene, texture, alpha, color, unlit, transmap, None, None, None, None, [0.0; 3],
        )
    }

    /// Material with transparency map and `[matl_nightmap]` self-illumination texture.
    pub fn add_material_night(
        &self,
        scene: &mut Scene,
        texture: Option<TextureId>,
        alpha: AlphaMode,
        color: [f32; 4],
        unlit: bool,
        transmap: Option<(TextureId, bool)>,
        nightmap: Option<TextureId>,
    ) -> MaterialId {
        self.add_material_full(
            scene, texture, alpha, color, unlit, transmap, None, nightmap, None, None, [0.0; 3],
        )
    }

    /// Material with a `[matl_lightmap]` texture whose strength comes from the per-slot
    /// instance value (illuminated displays).
    pub fn add_material_lit(
        &self,
        scene: &mut Scene,
        texture: Option<TextureId>,
        alpha: AlphaMode,
        color: [f32; 4],
        unlit: bool,
        transmap: Option<(TextureId, bool)>,
        nightmap: Option<TextureId>,
        lightmap: Option<TextureId>,
    ) -> MaterialId {
        self.add_material_full(
            scene, texture, alpha, color, unlit, transmap, None, nightmap, lightmap, None, [0.0; 3],
        )
    }

    /// Full vehicle material: also a `[matl_envmap]` sphere map with its strength factor
    /// (masked by the diffuse alpha channel like the original).
    pub fn add_material_env(
        &self,
        scene: &mut Scene,
        texture: Option<TextureId>,
        alpha: AlphaMode,
        color: [f32; 4],
        unlit: bool,
        transmap: Option<(TextureId, bool)>,
        nightmap: Option<TextureId>,
        lightmap: Option<TextureId>,
        envmap: Option<(TextureId, f32)>,
    ) -> MaterialId {
        self.add_material_full(
            scene, texture, alpha, color, unlit, transmap, None, nightmap, lightmap, envmap,
            [0.0; 3],
        )
    }

    /// Like `add_material_env` with an emissive colour (`[matl_allcolor]`).
    pub fn add_material_all(
        &self,
        scene: &mut Scene,
        texture: Option<TextureId>,
        alpha: AlphaMode,
        color: [f32; 4],
        unlit: bool,
        transmap: Option<(TextureId, bool)>,
        nightmap: Option<TextureId>,
        lightmap: Option<TextureId>,
        envmap: Option<(TextureId, f32)>,
        emissive: [f32; 3],
    ) -> MaterialId {
        self.add_material_full(
            scene, texture, alpha, color, unlit, transmap, None, nightmap, lightmap, envmap,
            emissive,
        )
    }

    /// Like `add_material_all` with the rest of the material manager's settings
    /// (reflection mask, depth handling, specular term).
    #[allow(clippy::too_many_arguments)]
    pub fn add_material_extra(
        &self,
        scene: &mut Scene,
        texture: Option<TextureId>,
        alpha: AlphaMode,
        color: [f32; 4],
        unlit: bool,
        transmap: Option<(TextureId, bool)>,
        nightmap: Option<TextureId>,
        lightmap: Option<TextureId>,
        envmap: Option<(TextureId, f32)>,
        emissive: [f32; 3],
        extra: MaterialExtra,
    ) -> MaterialId {
        self.add_material_inner(
            scene, texture, alpha, color, unlit, transmap, None, nightmap, lightmap, envmap,
            emissive, extra.moisture, extra,
        )
    }

    /// Swap the material of one slot of an instance (material variants).
    pub fn set_material(
        &self,
        scene: &mut Scene,
        instance: usize,
        slot: usize,
        material: MaterialId,
    ) {
        if let Some(m) = scene.instances[instance].materials.get_mut(slot) {
            *m = material;
        }
    }

    /// Make a copy of a material with a different diffuse texture. Used by scenery CTC and
    /// `[texchanges]` selectors: the slot's alpha, lighting, reflection, and depth settings
    /// stay as they were, while the replacement texture may bring its own PBR maps.
    pub fn add_material_retextured(
        &self,
        scene: &mut Scene,
        base: MaterialId,
        texture: Option<TextureId>,
    ) -> Option<MaterialId> {
        self.copy_material(scene, base, texture, None)
    }

    /// A vehicle's precipitation layer with wetness and collector-drop maps.
    /// Reuses the transmap binding without changing the layer's authored texture alpha.
    pub fn add_window_wetness_material(
        &self,
        scene: &mut Scene,
        base: MaterialId,
        mask: TextureId,
        drops: TextureId,
        bounds: [f32; 4],
        snow: bool,
    ) -> Option<MaterialId> {
        let texture = scene.materials.get(base)?.texture;
        // The shader reads the inverse pane height only as a magnitude (`abs(wipe_bounds.zw)`),
        // so its sign is free: negative marks the variant that shows settled snow, which
        // the shader draws itself from the mask (`snow_on_pane`) instead of water drops.
        let mut bounds = bounds;
        bounds[3] = if snow { -bounds[3].abs() } else { bounds[3].abs() };
        self.copy_material(scene, base, texture, Some((mask, drops, bounds)))
    }

    pub(crate) fn copy_material(
        &self,
        scene: &mut Scene,
        base: MaterialId,
        texture: Option<TextureId>,
        wetness: Option<(TextureId, TextureId, [f32; 4])>,
    ) -> Option<MaterialId> {
        let (
            alpha,
            color,
            unlit,
            no_z_write,
            writes_depth,
            no_z_check,
            z_bias,
            nightmap,
            lightmap,
            envmap,
            env_mask,
            mut bump,
            emissive,
            mut transmap,
            address,
            mut uniform,
        ) = {
            let src = scene.materials.get(base)?;
            (
                src.alpha,
                src.color,
                src.unlit,
                src.no_z_write,
                src.writes_depth,
                src.no_z_check,
                src.z_bias,
                src.nightmap,
                src.lightmap,
                src.envmap,
                src.env_mask,
                src.bump,
                src.emissive,
                src.transmap,
                src.address,
                src.uniform,
            )
        };
        if let Some((mask, drops, bounds)) = wetness {
            transmap = Some((mask, true));
            bump = Some((drops, 0.0));
            uniform.wipe_bounds = bounds;
            uniform.params[2] = 0.0;
            // the snow variant is a pane of glass, not the film of water (`emissive.w` 2)
            if bounds[3] < 0.0 && uniform.emissive[3] > 1.5 {
                uniform.emissive[3] = 1.0;
            }
        }
        uniform.pbr = texture
            .and_then(|id| scene.pbr_maps.get(&id))
            .map(|maps| maps.flags)
            .unwrap_or([0.0; 4]);
        if uniform.ambient[3] < 1.5 {
            uniform.ambient[3] = if texture.is_none() { -1.0 } else { snow_texture_flag(scene, texture) };
        }
        let slot = |t: Option<TextureId>| {
            t.and_then(|t| scene.textures.get(t).map(|g| (t, g.generation)))
                .unwrap_or((usize::MAX, 0))
        };
        let key = BindKey {
            textures: [
                slot(texture),
                slot(transmap.map(|t| t.0)),
                slot(nightmap),
                slot(lightmap),
                slot(envmap.map(|e| e.0)),
                slot(env_mask),
                slot(bump.map(|b| b.0)),
            ],
            address,
            uniform: bytemuck::cast(uniform),
        };
        let look = scene.look(&key);
        let (bind_group, buf) = match scene.bind_groups.get(&key) {
            Some((bg, b)) => (bg.clone(), b.clone()),
            None => {
                let buf = buffer_init(
                    &self.device,
                    &self.queue,
                    None,
                    bytemuck::bytes_of(&uniform),
                    wgpu::BufferUsages::UNIFORM,
                );
                let bind_group = self.material_bind_group(
                    &scene.textures,
                    MaterialMaps {
                        texture,
                        transmap,
                        nightmap,
                        lightmap,
                        envmap,
                        env_mask,
                        bump,
                        pbr: texture.and_then(|id| scene.pbr_maps.get(&id)).copied(),
                    },
                    address,
                    &buf,
                );
                scene
                    .bind_groups
                    .insert(key, (bind_group.clone(), buf.clone()));
                (bind_group, buf)
            }
        };
        scene.materials.push(Material {
            texture,
            alpha,
            color,
            unlit,
            no_z_write,
            writes_depth,
            no_z_check,
            z_bias,
            nightmap,
            lightmap,
            envmap,
            env_mask,
            bump,
            emissive,
            transmap,
            address,
            uniform,
            buf,
            bind_group,
            look,
        });
        Some(scene.materials.len() - 1)
    }

    /// Terrain material: uv is tile space, the ground texture repeats `repeats` times per
    /// tile, its detail texture `detail` times, and the optional mask (alpha 0 = cut) is
    /// sampled in tile space.
    /// `nightmap`: the tile's `.map.LM.bmp` (street lamp light pools), added at night.
    /// `moisture`: 1 when this layer's `<texture>.cfg` sidecar carries `[moisture]` or
    /// `[puddles]` (the map's base ground layer wets in the rain just like a painted one).
    #[allow(clippy::too_many_arguments)]
    pub fn add_terrain_material(
        &self,
        scene: &mut Scene,
        texture: Option<TextureId>,
        mask: Option<TextureId>,
        detail: Option<(TextureId, f32)>,
        repeats: f32,
        nightmap: Option<TextureId>,
        moisture: f32,
    ) -> MaterialId {
        self.add_material_wet(
            scene,
            texture,
            if mask.is_some() {
                AlphaMode::Test
            } else {
                AlphaMode::Opaque
            },
            [1.0; 4],
            false,
            mask.map(|m| (m, true)),
            Some((detail.map(|d| d.1).unwrap_or(0.0), repeats)),
            nightmap,
            detail.map(|d| d.0),
            None,
            [0.0; 3],
            moisture,
        )
    }

    /// A painted ground layer: the tile mesh drawn again with one of the map's other
    /// `[groundtex]` textures, blended in wherever the layer's painting mask (the alpha
    /// DDS the editor's brush writes to `texture/map/tile_x_y.map.<n>.dds`) says so.
    #[allow(clippy::too_many_arguments)]
    pub fn add_terrain_layer_material(
        &self,
        scene: &mut Scene,
        texture: Option<TextureId>,
        mask: TextureId,
        detail: Option<(TextureId, f32)>,
        repeats: f32,
        nightmap: Option<TextureId>,
        moisture: f32,
    ) -> MaterialId {
        // The brush mask includes fully transparent road cutouts. Those fragments must
        // never write biased terrain depth over the splines drawn in the next phase:
        // they contribute no colour, but would reject the road and expose the sky.
        // The C++ handler likewise draws painted terrain with depth writes disabled.
        self.add_material_inner(
            scene,
            texture,
            AlphaMode::Blend,
            [1.0; 4],
            false,
            Some((mask, true)),
            Some((detail.map(|d| d.1).unwrap_or(0.0), repeats)),
            nightmap,
            detail.map(|d| d.0),
            None,
            [0.0; 3],
            moisture,
            MaterialExtra {
                no_z_write: true,
                ..MaterialExtra::default()
            },
        )
    }

    pub(crate) fn add_material_full(
        &self,
        scene: &mut Scene,
        texture: Option<TextureId>,
        alpha: AlphaMode,
        color: [f32; 4],
        unlit: bool,
        transmap: Option<(TextureId, bool)>,
        terrain: Option<(f32, f32)>,
        nightmap: Option<TextureId>,
        lightmap: Option<TextureId>,
        envmap: Option<(TextureId, f32)>,
        emissive: [f32; 3],
    ) -> MaterialId {
        self.add_material_wet(
            scene, texture, alpha, color, unlit, transmap, terrain, nightmap, lightmap, envmap,
            emissive, 0.0,
        )
    }

    /// `moisture`: 1 when the `<texture>.cfg` sidecar marks this surface as one that
    /// darkens and starts to mirror the sky while the rain is on it.
    #[allow(clippy::too_many_arguments)]
    pub fn add_material_wet(
        &self,
        scene: &mut Scene,
        texture: Option<TextureId>,
        alpha: AlphaMode,
        color: [f32; 4],
        unlit: bool,
        transmap: Option<(TextureId, bool)>,
        terrain: Option<(f32, f32)>,
        nightmap: Option<TextureId>,
        lightmap: Option<TextureId>,
        envmap: Option<(TextureId, f32)>,
        emissive: [f32; 3],
        moisture: f32,
    ) -> MaterialId {
        self.add_material_inner(
            scene,
            texture,
            alpha,
            color,
            unlit,
            transmap,
            terrain,
            nightmap,
            lightmap,
            envmap,
            emissive,
            moisture,
            MaterialExtra::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add_material_inner(
        &self,
        scene: &mut Scene,
        texture: Option<TextureId>,
        alpha: AlphaMode,
        color: [f32; 4],
        unlit: bool,
        transmap: Option<(TextureId, bool)>,
        terrain: Option<(f32, f32)>,
        nightmap: Option<TextureId>,
        lightmap: Option<TextureId>,
        envmap: Option<(TextureId, f32)>,
        emissive: [f32; 3],
        moisture: f32,
        extra: MaterialExtra,
    ) -> MaterialId {
        // (the reflection maps switched off: as a material without one)
        let envmap = envmap.filter(|_| self.options.reflections);
        // the mask and the bump map only ever change the reflection
        let env_mask = extra.env_mask.filter(|_| envmap.is_some());
        let bump = extra.bump.filter(|_| envmap.is_some());
        // a rain film's reflection slot holds the picture behind the glass: its drops show
        // the street through themselves, bent and upside down, as real drops do
        let envmap = if extra.rain_film { Some((self.glass_slot(scene), 0.0)) } else { envmap };
        let address = self.address_next.replace(TexAddressing::Wrap);
        let lm_mapped = self.light_map_next.replace(false);
        let mode = match alpha {
            AlphaMode::Opaque => 0.0,
            AlphaMode::Test => 1.0,
            AlphaMode::Blend if extra.alpha_cut => 3.0,
            AlphaMode::Blend => 2.0,
        };
        // a mirror's glass shows a picture this renderer drew (`add_render_texture`): the
        // enhanced shader must not brighten it as it does a display (see enhanced.wgsl)
        let mirror = unlit
            && texture
                .and_then(|t| scene.textures.get(t))
                .is_some_and(|t| t.texture.usage().contains(wgpu::TextureUsages::RENDER_ATTACHMENT));
        let uniform = MaterialUniform {
            wipe_bounds: [0.0; 4],
            color,
            params: [
                mode,
                // 1 unlit (0.9 a mirror's own picture); 0.25 lit by everything but the map's
                // lamps; 0.15 a tree, not lit by the map's lamps in the vanilla picture
                if mirror { 0.9 } else if unlit { 1.0 } else if lm_mapped { 0.35 } else if extra.no_map_lights { 0.25 } else if extra.tree { 0.15 } else { 0.0 },
                // a [matl_transmap] whose file is missing still takes the alpha stage: Omsi.exe
                // sets a NULL texture there, which D3D9 samples as alpha 1, so the slot is as
                // opaque as its transmap says - not as see-through as the diffuse texture's
                // alpha, a reflection mask on car bodies (traffic half transparent)
                if transmap.is_some() || extra.transmap_declared { 1.0 } else { 0.0 },
                if transmap.map(|t| t.1).unwrap_or(false) {
                    1.0
                } else {
                    0.0
                },
            ],
            extra: [
                if terrain.is_some() { 1.0 } else { 0.0 },
                terrain.map(|t| t.0).unwrap_or(1.0),
                terrain.map(|t| t.1).unwrap_or(1.0),
                match (nightmap.is_some(), extra.night_switched) {
                    (false, _) => 0.0,
                    (true, false) => 1.0,
                    (true, true) => 2.0,
                },
            ],
            params2: [
                // ([matl_glow]'s mask rides in the light map's slot, but the material is
                // not a light-mapped one: `params2.x` stays off so the classic picture is as
                // it was - the enhanced one draws the glow, see `MaterialExtra::glow`)
                if lightmap.is_some() && extra.glow <= 0.0 { 1.0 } else { 0.0 },
                envmap.map(|e| e.1).unwrap_or(0.0),
                moisture,
                // bit 1: a [matl_envmap_mask]; bit 2: a [matl_transmap]; bit 4: a vehicle's
                // part that may be metal; bit 8: a body, never foliage; bit 16: the active
                // night texture supplies a clear vehicle material's coverage
                (if env_mask.is_some() { 1.0 } else { 0.0 })
                    + if extra.transmap_declared || transmap.is_some() { 2.0 } else { 0.0 }
                    + if extra.metal_ok { 4.0 } else { 0.0 }
                    + if extra.solid { 8.0 } else { 0.0 }
                    + if extra.night_alpha && alpha == AlphaMode::Blend && nightmap.is_some()
                        && !extra.transmap_declared && transmap.is_none() { 16.0 } else { 0.0 },
            ],
            emissive: [emissive[0], emissive[1], emissive[2], if extra.rain_film { 2.0 } else if extra.glass { 1.0 } else if extra.led && extra.led_sign { -3.0 } else if extra.led { -2.0 } else if extra.display { -1.0 } else { 0.0 }],
            specular: extra.specular,
            bump: [
                bump.map(|b| b.1).unwrap_or(0.0),
                if bump.is_some() { 1.0 } else { 0.0 },
                if extra.no_z_write { 1.0 } else { 0.0 },
                if extra.no_z_check { 1.0 } else { 0.0 },
            ],
            pbr: texture.and_then(|t| scene.pbr_maps.get(&t)).map(|m| m.flags).unwrap_or([0.0; 4]),
            flags: {
                let b = extra.border.unwrap_or([0.0; 4]).map(|c| (c.clamp(0.0, 1.0) * 255.0).round());
                [
                    if extra.screen { 1.0 } else { 0.0 },
                    if extra.border.is_some() {
                        1.0
                    } else if address == TexAddressing::MirrorOnce {
                        2.0
                    } else {
                        0.0
                    },
                    b[0] * 65536.0 + b[1] * 256.0 + b[2],
                    b[3] / 255.0,
                ]
            },
            ambient: {
                let a = extra.ambient.unwrap_or([color[0], color[1], color[2]]);
                // (w -1: no texture - the Enhanced shading lights it by its diffuse colour)
                [a[0], a[1], a[2], if extra.water { 2.0 } else if texture.is_none() { -1.0 } else { snow_texture_flag(scene, texture) }]
            },
            sway: extra.sway.map_or([0.0; 4], |s| [1.0, s[0], s[1], s[2]]),
            // (no mask bound - its file missing - no glow: the slot would shine all over)
            // (y: an LED panel's share of its glow, see `MaterialExtra::led_glow`)
            glow: [if lightmap.is_some() { extra.glow.max(0.0) } else { 0.0 }, extra.led_glow.map_or(1.0, |g| g.clamp(0.0, 1.0)), if extra.glass && extra.daylight_glass { 1.0 } else { 0.0 }, 0.0],
        };
        let slot = |t: Option<TextureId>| {
            t.and_then(|t| scene.textures.get(t).map(|g| (t, g.generation)))
                .unwrap_or((usize::MAX, 0))
        };
        let key = BindKey {
            textures: [
                slot(texture),
                slot(transmap.map(|t| t.0)),
                slot(nightmap),
                slot(lightmap),
                slot(envmap.map(|e| e.0)),
                slot(env_mask),
                slot(bump.map(|b| b.0)),
            ],
            address,
            uniform: bytemuck::cast(uniform),
        };
        let look = scene.look(&key);
        let (bind_group, buf) = match scene.bind_groups.get(&key) {
            Some((bg, b)) => (bg.clone(), b.clone()),
            None => {
                let buf = buffer_init(&self.device, &self.queue, None, bytemuck::bytes_of(&uniform), wgpu::BufferUsages::UNIFORM);
                let bind_group = self.material_bind_group(
                    &scene.textures,
                    MaterialMaps {
                        texture,
                        transmap,
                        nightmap,
                        lightmap,
                        envmap,
                        env_mask,
                        bump,
                        pbr: texture.and_then(|t| scene.pbr_maps.get(&t)).copied(),
                    },
                    address,
                    &buf,
                );
                scene
                    .bind_groups
                    .insert(key, (bind_group.clone(), buf.clone()));
                (bind_group, buf)
            }
        };
        scene.materials.push(Material {
            texture,
            alpha,
            color,
            unlit,
            no_z_write: extra.no_z_write,
            writes_depth: extra.writes_depth && extra.no_z_write,
            no_z_check: extra.no_z_check,
            z_bias: extra.z_bias,
            nightmap,
            lightmap,
            envmap,
            env_mask,
            bump,
            emissive,
            transmap,
            address,
            uniform,
            buf,
            bind_group,
            look,
        });
        scene.materials.len() - 1
    }

    /// The bind group of a material: its textures (or the plain white/black ones), its
    /// sampler and its uniform buffer.
    pub(crate) fn material_bind_group(
        &self,
        textures: &[GpuTexture],
        maps: MaterialMaps,
        address: TexAddressing,
        buf: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        let view = |t: Option<TextureId>, or: &'_ GpuTexture| -> wgpu::TextureView {
            t.and_then(|t| textures.get(t))
                .map(|t| t.view.clone())
                .unwrap_or_else(|| or.view.clone())
        };
        let env_view = view(maps.envmap.map(|e| e.0), &self.black_texture);
        let night_view = view(maps.nightmap, &self.black_texture);
        let light_view = view(maps.lightmap, &self.black_texture);
        let diffuse_view = view(maps.texture, &self.white_texture);
        let trans_view = view(maps.transmap.map(|t| t.0), &self.white_texture);
        // a mask or bump map without its own texture reads white (full mask, flat)
        let mask_view = view(maps.env_mask, &self.white_texture);
        let bump_view = view(maps.bump.map(|b| b.0), &self.white_texture);
        // (without a PBR set: a flat normal, and full occlusion/roughness/metal channels the
        // shader leaves unread by its flags)
        let normal_view = view(maps.pbr.and_then(|p| p.normal), &self.flat_normal_texture);
        let orm_view = view(maps.pbr.and_then(|p| p.orm), &self.white_texture);
        let sampler = match address {
            TexAddressing::Wrap => &self.sampler,
            TexAddressing::Mirror => &self.mirror_sampler,
            TexAddressing::Clamp | TexAddressing::MirrorOnce => &self.clamp_sampler,
        };
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.material_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&diffuse_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&trans_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&night_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&light_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&env_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&mask_view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(&bump_view),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(&normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: wgpu::BindingResource::TextureView(&orm_view),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: wgpu::BindingResource::Sampler(&self.clamp_sampler),
                },
            ],
        })
    }
}
