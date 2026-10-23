# Aligned spline terrain validation

Compare the pre-PR base `caf95e8a` with this branch, both in release mode. Keep map
content, chrono date, weather, settings and camera identical. `OMSI_NO_SPLINE_HOLES`
must be unset: it disables the behavior being compared. Do not use the old downloadable
mod-fork executable as the baseline for this upstream PR.

## Screenshots

On Berlin-Spandau, capture matching before/after pairs at:

1. An aligned road on a slope: include the road edge and the hillside beside it.
2. A bridge approach: include the transition from saved ground to the deck, and ground
   beneath the bridge. Check that new fill does not climb to the deck.
3. A cutting: include the road, both banks, and the bottom of the cut.
4. A tile border crossed by an aligned spline: look along the border, low enough to
   reveal a crack or height/lighting seam.

Use an overview plus a close view if one image cannot show the road and bank clearly.
Record the map, tile coordinates, world/camera position and viewing direction for every
pair. Grundorf is an optional second stock-map check. Retain the Map 400 ditch pairs;
if making new ones, show the curved/sloping section, both ends and a tile border.

## Tile preparation time and terrain vertex counts

The ignored `omsi-scene::terrain_benchmark::tile_preparation_benchmark` test uses the
production `World::prepare_tiles` pipeline without a graphics adapter. It prepares one
requested tile, including staging required neighbors, placement, terrain fitting, cutting
and wanted texture decoding. Map opening is outside the timer. GPU upload, rendering and
whole-game startup are outside this measurement. The counts are the requested tile's
terrain mesh, before alpha cutting, excluding roads, objects and soil walls.

On Windows, run in PowerShell from this checkout (git, Rust and native build dependencies
must be installed):

```powershell
powershell -ExecutionPolicy Bypass -File scripts/benchmark-terrain.ps1 `
  -OmsiRoot 'D:\SteamLibrary\steamapps\common\OMSI 2' `
  -GlobalCfg 'D:\SteamLibrary\steamapps\common\OMSI 2\maps\Berlin-Spandau\global.cfg' `
  -Csv 'berlin-terrain.csv'
```

The script chooses the tile with the most alignment records in the saved map files
(before chrono patches). Use `-Tile X,Y` to select a particular tile instead. Repeat
for Map 400, changing `-GlobalCfg` and `-Csv`. Use an installed map with all dependencies;
missing assets invalidate the comparison. Run without terrain-related debug flags or
other environment overrides. Record CPU, RAM, OS, Rust version, date and exact revision.

The script makes an isolated baseline worktree at the PR base, copies only the measurement
test into it, and measures before and after with identical single-threaded Rayon settings.
Both use one untimed warm-up then seven timed runs, each with a fresh World. These are
warm filesystem-cache measurements, with cold scene caches; they are not cold disk loads.
Compilation is outside the timer. The CSV contains milliseconds, vertex and triangle
counts for every run. Report the median and min/max time, and verify counts are stable.
Send both CSVs with the screenshots. Keep any asset-load warnings from the console.

This does not measure peak memory or frame time. Do not present the synthetic tests as
stock-map measurements. The default benchmark date is 2026-10-01; override `-Date YYYYMMDD`
if needed, using the same date for both revisions.

## Fallback and CI

A fitting error restores the tile's saved terrain and the old projected spline outlines,
including neighboring source tiles. Soil walls and the alpha mask are generated from
those restored rims. Successful fitting deliberately replaces Omsi.exe's spline holes
with continuous backing to close narrow troughs and extended ends. Explicit object/tunnel
holes remain active in both paths.

Run `cargo test --locked --workspace --no-fail-fast` and `cargo build --locked --release`.
The upstream Release workflow already contains workspace regression tests and platform
builds. Runs with `action_required` need approval in the upstream repository; editing the
workflow does not provide that approval. Local results do not substitute for CI.

## Avoiding terrain over-tessellation

Road refinement is restricted to the aligned spline influence using the existing
conservative spatial index and a halo. The complete original road/spline height fields
and road clearance remain available for height and normal evaluation. Coplanar internal
source edges are omitted from the CDT; boundaries, real slope changes, duplicate-face
boundaries and non-manifold joins are retained. Straight sampling runs can be joined
only when they are collinear in 3D and have identical shoulder widths.

The potentially changed region is enclosed by a frame of saved-grid edges. Every
saved slope change and tile-border grid station is preserved. Flat grid subdivisions
outside the frame can be omitted without letting fitted triangles extend into unchanged
ground. Neighboring tiles keep every clipped sampling station on their shared border,
regardless of how their interiors refine.

The original half-metre shoulder probes, fine near-rim offsets and eighth-metre cap
probes are retained as error checks, rather than all becoming permanent vertices.
Longitudinal profile-knot cap constraints remain explicit to prevent secondary dips.
Refinement inserts the largest error per current face, then resolves remaining errors
in sequential sweeps. The error budget at a changed probe is 1 mm; a probe matching
saved ground uses a 1e-9 m budget. Heights are cached during refinement. This is a
bound at those probes, not a proof of the error everywhere between them. After sixteen
unsuccessful passes all probes are inserted, preserving the dense fallback. Normals
still use the original height field and neighbor halo.

### Controlled synthetic comparison

Linux x86-64, Rust 1.99.1, release with debug symbols disabled. A 128-cell tile has one
200 m aligned ditch and fourteen 300 m roads with half-metre stations. The flat case
uses constant saved height; the uneven case uses a 15 cm sinusoidal surface that
changes the slope at nearly every saved cell. Each variant has one warm-up and three
measured runs, excluding fixture allocation. The previous implementation is `e3ff15e6`
with the exact same benchmark fixture copied into it. Times below are medians.

| Case / metric | Previous test build | Current implementation | Saved mesh alone |
| --- | ---: | ---: | ---: |
| Flat: vertices | 48,727 | 8,820 | 16,641 |
| Flat: triangles | 96,940 | 17,126 | 32,768 |
| Flat: geometry preparation | 255.026 ms | 250.075 ms | about 0.5 ms |
| Uneven: vertices | 48,740 | 26,450 | 16,641 |
| Uneven: triangles | 96,966 | 52,386 | 32,768 |
| Uneven: geometry preparation | 269.341 ms | 229.924 ms | about 0.3 ms |

The saved-mesh column times `build_terrain_mesh`, without a ditch, spline-hole cutting
or soil walls. It is a reference geometry budget, not the full tile-load time or FPS of
an unmodified game. Terrain fitting still has a preparation cost. On uneven terrain
this fixture still needs about 60% more triangles than the saved mesh; equal cost on
all maps is not established. The first optimization had reduced the original flat
fixture from 632,114 to 96,940 triangles; this change reduces the latter by another 82%.

Reproduce with:

```sh
CARGO_PROFILE_RELEASE_DEBUG=0 cargo test --locked --release -p omsi-geometry --lib spline_heavy_terrain_cost -- --ignored --nocapture
```

Actual Berlin-Spandau/Map 400 tile CSVs and gameplay FPS require the installed maps.
The normal geometry/scene tests also cover unchanged saved terrain, matching seam
stations, duplicate coplanar faces, actual slope folds, refinement error and invariance
to extra stations in an otherwise identical planar road.

## Streaming robustness and terminal transitions

Terminal and lateral displacements use independent quintic fades. This keeps the
bank cross-section through an end cap and gives a zero terminal derivative when
it rejoins saved ground. Terrain-gradient probes are evaluated only within the
half-metre tangent-matching region. Cap sampling rays are clipped before storage. Constraint intersection candidates
use a conservative grid supercover along each segment, retaining shared
grid-boundary intersections, instead of every cell in its bounding rectangle.
Short constraints and axis-aligned ranges retain the original rectangle fast path.

Optional fitting rejects more than 600,000 profile indices, terminal/shoulder
widths exceeding one tile, more than 160,000 refinement probes or 80,000 constraint
segments, 65,536 constraint vertices or eight million broad-phase pair visits.
These deterministic limits bound the extra sampling work; they are not
a wall-clock deadline. A budget error or caught internal fitting panic follows the
same legacy-hole fallback as a triangulation error. Exceptionally complex tiles
can therefore use the original cut terrain rather than the fitted backing.

If a preparation batch panics outside fitting, its tiles are retried independently
so a damaged tile cannot permanently exclude healthy neighbors in the same batch.
Fits taking at least half a second log the tile coordinates and source counts.
The combined cut/textures timer alone cannot distinguish fitting from image decoding.
