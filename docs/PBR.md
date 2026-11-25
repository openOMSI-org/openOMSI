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
