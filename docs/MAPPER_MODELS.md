# Your own models

This tutorial brings your own 3D models into a map and works on them inside openMapper: importing,
editing the mesh, materials and textures, PBR maps, lamps and exporting. It is for map makers who
have done [Your first map](MAPPER_FIRST_MAP.md) and have a model, or want to make a simple one.

## What you will build

- a bus shelter imported from a glTF, FBX or OBJ file, placed by a road,
- its roof edited in openMapper's mesh editor,
- a material with a texture, and PBR maps made for it,
- a street lamp that glows at night,
- the shelter exported again for another program.

## What you need

- A map to place things on.
- A model file: `.gltf` / `.glb`, `.fbx`, `.obj`, `.x` or `.o3d` (from Blender, 3ds Max, Maya or an
  asset store). Or none: step 2 makes a shape in openMapper.

## 1. Import a model

1. Choose **File → Import assets…** (or the import button in the toolbar), or drop the file on the
   window.
2. Pick your model.

openMapper makes a new scenery object of it in `Sceneryobjects\openMapper\Imported` of the content
folder:

- every node's transform is kept, each node a part of the object,
- its materials' colours and textures come along, embedded ones too, turned into a format OMSI reads,
- normal and roughness / metalness / occlusion maps become openOMSI's `_nn` and `_orm` maps,
- a `.sco` is written for it, so OMSI 2 and openOMSI read it like any object.

The **Objects** tool opens with the new object chosen, and the status bar says "Click on the map to
place it". Click beside a road to place it; `R` turns it.

### What else you can import

| What you drop | What happens |
| --- | --- |
| A mod (a folder or `.zip` laid out like OMSI 2) | Merged into the content folder |
| A folder of `.sco` files | A new folder under `Sceneryobjects`, its objects ready to place |
| A folder of `.sli` files | A new folder under `Splines` (new road types) |
| A folder or `.zip` of models | Each model becomes an object |
| A picture (`.png`, `.jpg`, `.webp`, `.bmp`, `.tga`, `.tif`) | An upright sign showing it, or a standee cut to its outline if it has see-through parts |

See [Mods](MODS.md) for the content folder.

## 2. Or make a shape

1. Press `3` (**Objects**) and open **Shapes and lamps**.
2. Click a shape: **Cube**, **Plane**, **Cylinder**, **Stairs**, **Arch** and more.
3. Click on the map to put it there.

Until you edit its mesh, its sizes and segments can be changed in the properties.

For a flat area (a pavement, a plaza): press `P` with roads or splines selected to fill the outline
they make, or `Shift+P` to click the corners on the map. Click the first corner or press `Enter` to
close it.

## 3. Edit the mesh

1. Press `1` and select the object.
2. Press `Tab` (or click **Edit the mesh (Tab)** in its properties).
   - For an object that is not openMapper's own, a copy of its type is made first, with its
     meshes, materials and lamps. The original stays as it is.
3. Choose what to pick: `1` vertices, `2` edges, `3` faces.
4. Click to pick. `Shift` + click adds. Drag on nothing for a box. `Alt` + click (or a second click)
   picks a loop.
5. Move, turn and scale what is picked with the handles (`V`, `C`, `X`).

Select the roof's top faces and try:

| Key | Operation |
| --- | --- |
| `E` | **Extrude**: pull the faces out (a thicker roof) |
| `I` | **Inset**: a smaller face inside each face |
| `Ctrl+B` | **Bevel**: round the edges |
| `Ctrl+R` | **Loop cut**: point at an edge, press, and cut a ring |

The size follows the mouse until you click; the wheel sets segments or cuts. **Adjust the last
operation** in the panel changes it afterwards.

More operations are in the panel: subdivide, fill (`F`), bridge loops, connect (`J`), merge (`M`),
dissolve, delete (`X`), duplicate (`Shift+D`), split (`Y`), separate into an object (`P`), spin,
mirror, smooth, triangulate (`Ctrl+T`), triangles to quads (`Alt+J`), flip normals (`Alt+N`),
normals outward (`Shift+N`), smooth or flat shading, seams and sharp edges.

- `A` selects all, `Alt+A` none, `Ctrl+I` inverts, `Ctrl+L` selects linked.
- `Alt+Z` turns on X-ray, to see and pick through the mesh.
- `Ctrl+Z` undoes inside the mesh editor.
- `Tab` leaves the edit mode.

### Parts, modifiers and booleans

In the object's properties:

- **Parts** lists its meshes: show or hide, rename, duplicate, delete, or **Add a shape as a part**.
  **Separate parts into objects** makes each part an object where it stood.
- **Modifiers** work like Blender's stack: **Add a modifier** (array, mirror, solidify, bevel,
  subdivision surface, decimate, boolean, follow the ground and more), then **Apply** to make it
  part of the mesh.
- With two or more objects selected, **Join into one object (Ctrl+J)** makes them one, and the
  boolean buttons give their union, difference, intersection or a slice.

openMapper keeps its own objects as `<name>.openmapper.json` beside the `.sco` and `.x` files it
writes from them.

## 4. Give it materials

Open **Materials** in the object's properties.

1. Pick a material, or click **Add a material**.
2. Set **Colour** (it multiplies the texture).
3. Under **Texture**, click **Choose…** and pick any picture. OMSI formats are kept, others become
   PNG.
4. Set **Transparency**: **Opaque**, **Cut out** (leaves, fences) or **Blended** (glass).
5. Set **Glows** (its own light) and **Shine**.
6. Optional: a **Normal map**, an **ORM map**, a **Night texture** and a **Light map**.

To give a material to part of the mesh: in the edit mode, pick faces and click **Assign to selected
faces**.

**Import materials…** reads them from an `.mtl`, a glTF / GLB / FBX file, or the pictures of one PBR
material (base colour, normal, roughness, metal, AO, opacity, emission). **Save as…** writes a texture
as PNG, JPEG, BMP, TGA, TIFF, WebP or DDS.

### UV maps

In the edit mode, **Unwrap** offers **By seams (conformal)**, **Smart project**, **Cube projection
(metres)**, **Cylinder projection**, **Sphere projection**, **Project from the view** and
**Lightmap pack**. Mark seams on edges first for an unwrap by seams. The **UV editor** window shows
the texture under the faces, to move, turn, scale and pack them.

### Bake

**Bake** paints the object's textures and colours into one picture on a new UV map, optionally with
**Ambient occlusion** and the **Lamps' light at night (light map)**. The whole object then has one
material, which draws faster in the game.

## 5. Make PBR maps

PBR maps make a surface look rough, shiny or metallic in openOMSI's newer renderers.

1. Select the object and click **Create PBR** in its properties.
2. Choose one of its textures, or **Own picture…**.
3. Tune the maps with the live preview: **Normals (_nn)**, **Bumps**, **Roughness**, **Shadow in
   hollows**, **Metal** and their options. **Back to the defaults** resets them.
4. Click **Apply**. The maps are written as openOMSI reads them (`_nn`, `_orm`).

Later, **Configure PBR** tunes them again, or **Delete the PBR material** takes them away. For a
picture that is not on an object, use **Tools → PBR maps for a picture…**. A road type has **Create
PBR** in its properties too. See [PBR](PBR.md) for how the game uses these maps.

## 6. Add a lamp

On your own object, open **Lamps** in its properties:

- **Add a glow**: a lamp's bulb or a lit sign, seen at night. Set **Colour**, size, **Seen from one
  side** or all round, and **On**: **Always**, **At night** or by a script **variable**.
- **Add light on the ground**: the pool of light a street lamp throws at night.

For a complete lamp, open **Shapes and lamps** in the Objects tool and click **Street lamp** (a pole
with a lamp that glows and lights the ground) or **Signal head** (a traffic light driven by a
crossing's light program; put it within 40 m of a crossing with lights).

## 7. Export

1. Select the object (or several things, roads too).
2. Click **Export…** in its properties, or right-click it in the view and choose **Export…**.
3. Pick a format and a place.

Formats: OBJ + MTL, glTF, GLB, FBX (binary, as Blender writes it), Collada, USD, STL, PLY, 3MF, OFF,
DirectX `.x` and OMSI `.o3d`. Textures are copied beside the file (or packed in), in metres with Z up.
A single object comes out as its model is made; several things come out as they stand together.

A road or spline can become an object too: **Copy as an object** or **Replace by an object** in its
properties, then edit it as a mesh.

## What you learned

- **Import assets…** turns models, mods and pictures into objects in the content folder.
- `Tab` edits any object's mesh, Blender-style.
- **Materials**, **Unwrap** and **Bake** handle textures; **Create PBR** makes PBR maps.
- **Lamps** and **Shapes and lamps** add light.
- **Export…** writes models for other programs.

## Next

- [openMapper tools](MAPPER_TOOLS.md): every mesh operation, modifier and material setting.
- [openMapper keys](MAPPER_KEYS.md): the mesh editor's keys.
- [Modding](MODDING.md) for scenery objects in OMSI's own formats.
