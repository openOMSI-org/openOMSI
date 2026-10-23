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
