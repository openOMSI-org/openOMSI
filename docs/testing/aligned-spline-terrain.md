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

Road constraints and their fine blend samples are limited to the spline influence
region, using the existing conservative spatial index plus a halo for the road blend
and normal probes. The road mesh, its original boundaries, and the fine sampling
resolution within this region are retained. Original terrain grid diagonals still
constrain the entire tile. Empty profile lists bypass spatial-index allocation.

This addresses excessive terrain geometry during rendering, including shadow passes,
as well as loading cost. Distant road meshes no longer refine the ground. The complete
road height field and original clearance remain available for height and normal
evaluation. A regression checks that a distant road adds no vertices, preserves the
local ditch fit and retains the original clearance under that road.

Controlled synthetic measurement against `7123267`, Linux x86-64, Rust 1.99.1,
release profile with debug symbols disabled. The fixture has a 128-cell saved terrain,
one 200 m aligned ditch and fourteen 300 m roads with half-metre stations. Each variant
uses one warm-up and three measured runs; mesh construction is timed without fixture
allocation. Median times and stable mesh counts:

| Metric | Before optimization | After optimization |
| --- | ---: | ---: |
| Terrain vertices | 316,744 | 48,727 |
| Terrain triangles | 632,114 | 96,940 |
| Geometry preparation, median | 981.849 ms | 239.914 ms |

Reproduce with:

```sh
CARGO_PROFILE_RELEASE_DEBUG=0 cargo test --locked --release -p omsi-geometry --lib spline_heavy_terrain_cost -- --ignored --nocapture
```

To compare `7123267`, add the
same benchmark fixture to that checkout without changing its terrain implementation.
This is a synthetic workload, not a stock-map measurement or a measured FPS improvement.
Actual Berlin-Spandau/Map 400 CSVs and gameplay FPS still need the installed maps.

Validation after optimization: `cargo test --locked -p omsi-geometry -p omsi-scene --lib` passes 252 tests (6 ignored, including the opt-in performance fixture).
