# The X converter

DirectX `.x` meshes to OMSI's `.o3d`, as OMSI's own "X to O3D Converter 1.0" does it - the
same eight switches under the same names, at the same defaults, the same output - but for
many files at once, with a log for each, from the command line (`openomsi-xconv`). Text and binary `.x` files alike (32- or 64-bit floats);
the compressed forms are refused with a word to save them uncompressed.

The crate is `crates/omsi-xconv`: `xfile` reads the file as its frames, meshes and
materials; `convert` turns them into the meshes to write by the settings; `o3d` writes a
plain file - version 1 while every count fits in 16 bits, version 3 (32-bit counts and
indices) past that, as the original tool wrote them.

## The switches

| Switch | Default | What it does |
|---|---|---|
| X-File: z up | on | the file's Z is up (Blender); Y and Z are swapped to OMSI's Y up, and the triangles turned round with the swap. Off for GMax and 3ds Max exports that are Y up already |
| Separate Files | off | one `.o3d` a mesh, named `<file>_<mesh>` (the frame's name without a mesh name) |
| Show Details of X | off | the log tells each mesh: its frame, counts, the matrix applied |
| Remove Specular | on | the materials' specular colour zeroed (the power stays) |
| Opt. Materials | on | the materials no face uses dropped, the rest in the file's order |
| Opt. Vertices | on | corners that are the same in every number folded into one vertex |
| All Normals Up | off | every normal straight up |
| Use Submesh Local Pos. as Global Pos. | off | the frame matrices left out |

And three of ours, off unless asked: *Skin weights as bones* (`SkinWeights` written as the
file's bones - the original dropped them), *Frame matrix as the pivot* (the mesh's frame
matrix kept as the `origin_from_mesh` pivot), *Flat normals where the file has none* (the
original wrote zeros).

## How the original does it, as far as the stock files tell

The rules were taken from the objects of the installation that ship both the `.x` and the
`.o3d` made of it (345 pairs on this machine: the IK-201 and IK-103 buses, the Novi Sad and
gibanica objects, D016, Ljubljana, Ruede) and checked by `tests/stock.rs`
(`cargo test -p omsi-xconv --test stock -- --ignored --nocapture`, `OMSI_ROOT` or the
Steam folder; `XCONV_ONLY=<name>` for one pair with every try reported):

* the vertices of the `.o3d` are the *corners* of the faces in the order the faces list
  them, a corner being its vertex's position and texture coordinate with the normal
  `MeshNormals` gives that corner - so a vertex two faces light differently is two vertices,
  and the file's own vertex order is not kept;
* the meshes of a file come in the order of their frames' *names* (the original keeps the
  frames in a sorted list), not the file's;
* positions go through the frame matrices, the mesh's own first then its parents'
  (`v · child · parent`, D3D's row vectors); normals through the same matrices as vectors
  and made unit length again;
* a face with more than three corners is a fan from its first;
* Z up swaps Y and Z after the matrices and turns every triangle round; a mirroring frame
  matrix does not turn anything.

324 of the 345 pairs come out the same (vertices to a rounding, triangles and materials
exact). The rest: the Ruede pack's files were made with another tool (its normals keep the
frame's scale, its ball has texture coordinates its `.x` does not), some IK-201 and Novi
Sad `.x` files were re-exported after their `.o3d` (the pairs no longer belong together),
and the D016 lamps' `.o3d` files carry no materials at all (a fault of theirs, not kept).

## The command line

```text
openomsi-xconv [switches] file.x [more.x ...] [folder ...]
  --no-zup           X-File: z up off (the file is already Y up)
  --separate         Separate Files: one .o3d a mesh
  --details          Show Details of X
  --keep-specular    Remove Specular off
  --no-opt-materials Opt. Materials off
  --no-opt-vertices  Opt. Vertices off
  --normals-up       All Normals Up
  --local            Use Submesh Local Pos. as Global Pos.
  --bones            skin weights written as bones
  --pivot            the frame matrix kept as the pivot (origin_from_mesh)
  --out DIR          write into DIR instead of beside each file
```

A folder converts every `.x` in it (not below). The exit code is the number of files that
failed.
