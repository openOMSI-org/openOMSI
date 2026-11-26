# PBR materials

openOMSI can draw content with physically based materials: a normal map for fine relief, and
roughness, metalness and ambient occlusion maps for how a surface reflects light. This page is
for modders who want to add these maps to buses, objects and splines.

The maps are an addition. Content without them looks as it always did, and content with them
still works in OMSI 2, which ignores the extra files. PBR maps are drawn by the **Enhanced** and
**Enhanced+** renderers on every platform (see [Graphics](GRAPHICS.md)).

## Adding maps to a texture

No config file changes. Put the maps **beside the diffuse texture**, with the same name plus a
suffix, as Substance, Blender, Materialize and similar tools export them.

1. Take the texture the model uses, for example `Texture/bus.dds`.
2. Export the maps with the same base name and a suffix from the table below, for example
   `Texture/bus_nn.png` and `Texture/bus_orm.png`.
3. Start the game with the Enhanced or Enhanced+ renderer.

| Map | Suffixes | Values |
|---|---|---|
| Normal (Direct3D, green down) | `_nn`, `_normal`, `_nrm`, `_normalmap`, `_normal_dx`, `_nn_dx` | Tangent space |
| Normal (OpenGL, green up) | `_nn_gl`, `_normal_gl`, `_nrm_gl`, `_normalgl`, `_normal_opengl` | Tangent space |
| Roughness | `_rr`, `_rough`, `_roughness`, `_rgh` | White = rough |
| Glossiness | `_gg`, `_gloss`, `_glossiness` | Inverse of roughness; used only without a roughness map |
| Metalness | `_mm`, `_metal`, `_metallic`, `_metalness`, `_mtl` | White = metal |
| Ambient occlusion | `_aa`, `_ao`, `_occlusion`, `_ambientocclusion`, `_ambient_occlusion` | White = open |
| Packed ORM | `_orm`, `_arm` | Red occlusion, green roughness, blue metalness |
| Packed MRA | `_mra` | Red metalness, green roughness, blue occlusion |
| Packed RMA | `_rma` | Red roughness, green metalness, blue occlusion |
| Height (parallax) | `_hh`, `_height`, `_heightmap`, `_disp`, `_displacement`, `_parallax` | White = high; a grey picture |

- File types: `.png`, `.tga`, `.dds`, `.bmp`, `.jpg`, `.jpeg`.
- Case does not matter. The suffix may also start with `-` instead of `_` (`bus-normal`).
- The maps may also lie in the same folder of another content root, for example a mod folder that
  adds maps to a texture of the OMSI 2 installation.
- A normal map must look like one (bluish). Otherwise it is ignored.

> **Note:** The short suffixes are **doubled letters** (`_nn`, `_rr`, `_mm`, `_aa`, `_gg`). OMSI
> mods already use single letters for other purposes (`_n` is often a night map), so a single
> letter is never taken as a PBR map.

### Size and memory

- PBR maps are kept at up to 4096 pixels per side.
- They are stored uncompressed, because a normal map does not survive DXT1 compression. Use them
  where they are seen up close.
- The separate maps are packed into one occlusion/roughness/metalness texture on the GPU.

## Factors without textures

A material can also be given its PBR values as numbers, for surfaces that are the same all
over (a polished reflector, a rubber seal, a painted rail), without making texture files. Put
the keywords after the `[matl]` they belong to in the model `.cfg`, one value per line, as the
other `[matl_*]` commands. OMSI 2 does not know these keywords and skips them; only the
**Enhanced** and **Enhanced+** renderers draw them.

```
[matl]
reflector.bmp
0

[matl_basecolor]
0.9
0.9
0.92

[matl_metallic]
1

[matl_roughness]
0.08
```

| Keyword | Lines | Values |
|---|---|---|
| `[matl_basecolor]` | red, green, blue | 0..1, sRGB (as a colour picker shows it) |
| `[matl_metallic]` | metalness | 0 (dielectric) .. 1 (metal) |
| `[matl_roughness]` | roughness | 0 (mirror) .. 1 (matt) |
| `[matl_normal_strength]` | factor | 0 (flat) .. 4; 1 the map as painted |
| `[matl_occlusion_strength]` | factor | 0 (none) .. 1 (the whole map) |

How a factor works with a map, as in glTF:

- **Base colour** multiplies the diffuse texture (and the `.o3d` material's colour). A slot
  without a texture takes it as its whole colour.
- **Metalness and roughness:** with a map the map's value is multiplied by the factor, without
  one the factor is the value. Either replaces the values derived from OMSI's files (the table
  below), so a material with a factor reflects the sky as a mapped one does.
- **Normal strength** scales the normal map's slopes; without a normal map it does nothing.
- **Occlusion strength** blends the occlusion map towards white; without a map it does nothing.
- A value out of range is taken to the nearer end. A `[matl_item]` takes the factors it does
  not set from its slot's `[matl]`, and the last keyword of a kind counts.

A material without these keywords looks exactly as before.

### glTF models

A `.gltf` or `.glb` material's `metallicFactor` and `roughnessFactor` are taken as
`[matl_metallic]` and `[matl_roughness]` when the material has no metallic-roughness texture
(that texture is not read; put the maps beside the base colour texture instead). Keywords in
the `.cfg` take precedence. The base colour factor is the `.o3d` material colour, as before.

## Emissive materials

A material can give off light of its own, with a colour and a brightness that need not be
those of its texture: a halogen filament, an LED, a dashboard lamp, a display. The scripts can
switch it, dim it and change its colour, and it can fade up and down as a bulb does. Only the
**Enhanced** and **Enhanced+** renderers draw it; OMSI 2 and the classic picture skip the
keywords. The light is drawn in HDR: it is brighter than white where it is strong, blooms by
its brightness and keeps its colour.

```
[matl]
bulb.bmp
0

[matl_emissive]
1
0.9
0.8
40
lights_main

[matl_emissive_kelvin]
3200
filament

[matl_emissive_fade]
0.12
0.25
```

| Keyword | Lines | What it does |
|---|---|---|
| `[matl_emissive]` | red, green, blue; strength; optional variable | The light's colour (sRGB 0..1) and strength. 1 is as bright as a lit display; higher values are HDR (like glTF's `KHR_materials_emissive_strength`). The variable (or a number) scales it from 0 (off) to 1 (full). |
| `[matl_emissive_nits]` | luminance | The brightness in physical units, cd/m², in place of the strength. The colour then only sets the tint. |
| `[matl_emissive_kelvin]` | temperature; optional variable or `filament` | The colour of a black body this hot (K) in place of the colour. A variable gives the temperature from the script. `filament`: the temperature follows the level, as a bulb's filament does: red-orange while it warms up or cools down, the given temperature at full power. |
| `[matl_emissive_texture]` | file | Where and in which colour it shines (rgb, times the colour). Without one the whole slot shines through its diffuse texture. |
| `[matl_emissive_fade]` | seconds up; optional seconds down | The level follows the variable with this lag (63 % of a change in that time). |

- Any of these keywords makes the slot emissive; what is left out stays at white, strength 1,
  full level, no fade.
- **Physical units.** The renderer's radiance unit is 10 000 cd/m², exposed with the scene, so
  a luminance is as bright against its surroundings as it would be: a white LED panel of a few
  hundred cd/m² glows softly by night and is hardly seen in the sun; a halogen filament
  (about 2 x 10^7 cd/m²) burns white in any light.
- **Artist's strength.** It is held against the eye's adaptation as the bus's screens and
  `[matl_glow]` are (strength 1 = `[matl_glow]` value 4).
- The texture uses the light map's slot. A slot with a `[matl_lightmap]` or a `[matl_glow]` keeps
  those; its emission is then not driven by variables and has no texture of its own.
- The variables come from the vehicle's scripts, or from a scenery object's (`.sco`) own
  script; an object without one shines at full level.
- The emission is the material's look. It does not light the surroundings: put a
  `[light_enh]`, `[interiorlight]` or `[spotlight]` beside it for that, switched by the same
  variable.

## Tips

- Export at the diffuse texture's resolution, or half of it.
- Prefer one packed `_orm` file to three separate maps: one file is read instead of three.
- Check the green channel of a normal map. If dents look like bumps, add or remove `_gl`.
- To import a model with PBR materials, see [Importing models](MAPPER_MODELS.md).

## What the maps do

- The **normal map** bends the surface normal. The tangent frame is computed per pixel, so the
  `.o3d` needs no tangents.
- **Roughness** sharpens or blurs the reflections of the environment and the sun's highlight.
- **Metalness** tints the reflection with the diffuse colour and darkens the diffuse part.
- **Occlusion** darkens the ambient light only.
- The **height map** gives the surface relief by parallax occlusion mapping: the texture is
  looked up where the view ray meets the relief, so cobbles, brick joints and tread plates
  hide what lies behind them as the view moves. The relief is 2.5 cm deep, whatever the
  texture's scale. It is stored in the alpha of the occlusion/roughness/metalness map (made
  for it when there is none).
- A `[matl_envmap]` strength still scales the reflection.
- Without a roughness map, the diffuse texture's alpha still works as OMSI's reflection mask.

## Materials without maps

Without maps, the material is derived from OMSI's own files:

| OMSI material | Roughness | Reflectance (F0) |
|---|---|---|
| Terrain | 0.92 | 3 % |
| Object with an `.o3d` specular power *p* | ((2 / (p + 2))^½)^½, clamped to 0.4-0.9 | 4 % |
| Object without specular power | 0.8 | 4 % |
| Leaves, fences (alpha-tested) | 0.7, no reflection | 4 % |
| Paint with `[matl_envmap]` | 0.06 (polished) to 0.8 (satin), from the envmap mask's share | Clear coat matching the share; metal only where its own mask says so |
| Glass (blended pane) | 0.04 | 4-90 %, from the pane's alpha and envmap factor |
| Map water | 0.06 plus ripples | 2 % |

## Weather on surfaces

| Weather | Effect |
|---|---|
| Rain | Porous (rough) surfaces darken as they soak. Upward faces turn glossy, walls much less. Roads with `[moisture]` go dark and smooth, with puddles in low spots. |
| Snow | Covers upward faces, matt (roughness 0.95). |
| Thin cloud or high veil | Part of the sun's light comes from the whole sky. Shadows fade and the light goes flat. |

## The lighting model

The Enhanced renderers light every surface, with or without maps, with the metal/roughness
model of current engines:

- **Specular:** GGX microfacets, height-correlated Smith visibility and Schlick's Fresnel. F0 is
  4 % for a dielectric and the diffuse colour for a metal. Multiple scattering is compensated,
  so a rough metal is not darker than its colour.
- **Diffuse:** Lambert, with only the energy the specular reflection leaves. A surface never
  sends out more light than it receives.
- **Ambient:** the sky and the street reach a surface through the split-sum approximation (a
  pre-blurred sky probe and an analytic DFG term). Ambient occlusion and, under a normal map, the
  horizon of the real surface darken creases.
- **Specular antialiasing:** roughness is widened where the normal changes within a pixel, so
  fine relief and polished curves do not sparkle.
- **Colour:** textures are read as sRGB. PBR maps are read as linear data.

With **Enhanced+** (ray tracing on), reflections are traced. Each noticeably reflective pixel
sends a ray along its GGX lobe, and the rays are gathered over frames and neighbouring pixels.
A rough surface gets a blurred reflection of the real street; glass, chrome or a wet road gets a
sharp one, each as strong as its Fresnel term says.
