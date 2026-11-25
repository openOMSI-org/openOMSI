# TH_Wald CPU rendering bottleneck (#284)

Developer notes on the slowdown on the TH_Wald map: what caused it, what was changed, the measurements, and how to repeat them.

**Summary**

- The sustained slowdown at spawn `1792.7,1561.9,187,6.0` is dominated by draw preparation and command recording on the CPU, not tile streaming or GPU shading.
- Cause: thousands of short curb, grass and pavement spline segments, each uploaded as its own mesh, so instancing could not combine them. `[terrainmapping]` sections also draw with the tile's ground layers, and Enhanced rendering processes the geometry again in its depth passes.
- Fix: batch spline segments spatially, pool terrain-mapped faces, cache bounds, keep persistent encoding workers. No shader changes, no lower graphics settings.
- Result before main integration (RTX 5070, Enhanced): **17.8 → 43.2 FPS** median at the outside view. After integration with current main: **36.0 FPS** (a sanity measurement, not an A/B).
- Stable 60 FPS at these Enhanced settings has not been reached in this outside scene: it needs frame time below 16.7 ms, measured 23.1 ms.

## The cause

The asset audit listed the batches the unbatched main view recorded per frame:

| Asset | Batches |
|---|---:|
| `Randstein06_mG.sli` | 5,349 |
| `Gras_1m.sli` | 3,418 |
| `Randstein06.sli` | 2,066 |
| `Gras_3m.sli` | 1,868 |

The first audit grouped the terrain-mapped parts under procedural geometry. Both halves now carry their source asset name.

## Changes

| Change | What it does |
|---|---|
| Spline batching | Joins short spline segments of the same type in 48 m spatial cells on the loading worker, before GPU upload. Shadow flags, winding and material-slot order stay separate. Long segments keep their own bounds. Positions, normals and UVs are copied exactly; the original staging geometry still defines collision and terrain deformation. Tile unloading owns and frees the merged meshes as before. Adjacent profiles with identical material slots in a merged mesh are combined. |
| Material-compatible splines | Different spline types share a cell when all used slots have identical texture filenames, lookup directories and alpha modes. Unused terrain slots do not prevent merging. UV generation has already finished; no material is approximated. Blended segments keep their own meshes and placement origins for far-to-near sorting. The audit labels a shared opaque/cutout mesh with its representative type. |
| Ground batching | Terrain-mapped spline faces are split before batching. In each 48 m cell, faces from different spline types that use the same tile ground materials are pooled. Vertical cells and winding stay separate. Tile UVs are calculated before merging; the same base and paint layers are drawn. |
| Hashed transparency lookup | Replaces two linear scans per blended mesh with a hash lookup for the object's nearest mesh distance. Keeps signed-zero equality and full f64 coordinates. |
| Bounds cache | Transformed instance spheres and scale are cached once when instance or mesh data changes, and reused in the main view, shadows and mirrors. Mesh deformation, replacement and recycling invalidate them. Floating origins remain per view. |
| Persistent encoding workers | Up to three dedicated workers for render bundles and encoder finishing, with the main thread taking part. The pool is sized by available CPU parallelism, independent of simulation workers. Batch replay order is kept. |
| Profiling | Asset audits every ten seconds under `OMSI_PROFILE`, per-encoder finish and join timings, and elapsed-time-corrected one-second FPS intervals. |

All of this uses ordinary meshes and draws on every rendering backend. Spatial batching slightly increases conservative culling, so GPU-limited hardware still needs benchmarking.

## Measurements before main integration (2026-09-30)

**Setup**

| Item | Value |
|---|---|
| System | Windows, DirectX 12, RTX 5070, 1920x1080, V-sync on |
| Graphics | Enhanced, 4x MSAA, SSAO, 2048 shadows, 1024 mirrors, object distance 734 m |
| Scene | Same stationary spawn, date and seed; 20 traffic target; scheduled buses and passengers |
| Bus | MB C2 articulated (as in the supplied audit) |
| Runs | Sequential, 110 s each |
| FPS | Median of the last 30 one-second intervals after loading |
| Phase and batch figures | Renderer averages printed at shutdown (not the warm-up-excluded FPS window) |

Each comparison below uses one executable and toggles only the named change with an environment variable. Run-to-run traffic and background activity still affect FPS.

### 1. Spline batching

Disabled with `OMSI_NO_SPLINE_BATCHING=1`. Both cases include the hashed transparency lookup.

| Metric | Disabled | Enabled |
|---|---:|---:|
| Stationary FPS median | 17.8 | 36.7 |
| Main batches/frame | 31,221 | 6,135 |
| Prepass batches/frame | 30,902 | 5,817 |
| Main triangles/frame | 1.789 million | 1.821 million |
| Draw preparation | 6.34 ms | 1.43 ms |
| Bundle recording | 5.12 ms | 1.50 ms |
| Encoder finishing wall time | 13.77 ms | 3.49 ms |
| Main-view culling | 3.19 ms | 1.77 ms |
| Mirror culling | 3.14 ms | 1.92 ms |

- An earlier untouched release executable also reproduced the slowdown. The first regressing commit between 0.1.144 and 0.1.221 has not been established. Other hardware is not measured.
- Screenshots at 80 s: mean absolute RGB difference 0.103 (0–255 scale); 0.254 % of pixels differed by more than 8 in any channel. Live sessions with moving traffic and HUD differences: a visual sanity check, not a pixel-equivalence test.
- Artifacts: `target/performance-284/final-before.*`, `final-after.*` (not committed).

### 2. Ground batching

`ground-before` disables only the cross-type ground batching (first spline change stays on); `ground-after` enables both.

| Metric | First spline change | Also pool ground faces |
|---|---:|---:|
| Stationary FPS median | 36.8 | 40.9 |
| Main batches/frame | 6,136 | 4,849 |
| Prepass batches/frame | 5,818 | 4,530 |
| Main triangles/frame | 1.821 million | 1.829 million |
| Main render CPU | 10.1 ms | 9.1 ms |
| Mirrors CPU | 6.0 ms | 5.2 ms |
| Encoder finishing wall time | 3.34 ms | 2.91 ms |
| Main GPU pass | 2.59 ms | 2.68 ms |

- +11.3 % FPS, 21 % fewer main batches.
- Screenshots: mean absolute RGB difference 0.041; 0.076 % of pixels differed by more than 8 in a channel. Both images inspected; still a sanity check. No O550 follow-up tests were run.
- Artifacts: `target/performance-284/ground-before.*`, `ground-after.*`.

### 3. Bounds cache and material-compatible splines

Toggled together; both earlier spline changes stay on. Before: `OMSI_NO_BOUNDS_CACHE=1` and `OMSI_NO_MATERIAL_SPLINE_BATCHING=1`.

| Metric | Disabled | Enabled |
|---|---:|---:|
| Stationary FPS median | 39.8 | 42.3 |
| Main-view culling | 1.71 ms | 1.41 ms |
| Mirror culling | 1.79 ms | 1.06 ms |
| Shadow draw preparation | 1.46 ms | 0.97 ms |
| Main batches/frame | 4,848 | 4,765 |

- +6.4 %, about 1.5 ms less frame time. The stage figures are shutdown averages.
- The GPU integration test exercises bounds after transforms, skinning, render origin changes, freeing, recycling and mesh replacement.
- Screenshots: mean RGB difference 0.125/255; 0.304 % of pixels above an 8-level change.
- Artifacts: `target/performance-284/bounds-material-before.*`, `bounds-material-after.*`.

### 4. Persistent encoding workers

`pool-before` / `pool-after` toggle only `OMSI_NO_RENDER_POOL=1`. Both include all batching and bounds changes.

| Metric | New threads per picture | Persistent workers |
|---|---:|---:|
| Stationary FPS median | 36.9 | 43.2 |
| Main bundle recording | 1.35 ms | 1.10 ms |
| Mirror bundle recording | 1.07 ms | 0.59 ms |
| Main render CPU | 9.2 ms | 8.5 ms |
| Mirrors CPU | 5.3 ms | 4.4 ms |
| Encoder finishing wall time | 3.00 ms | 2.76 ms |

- The lower encoding cost is measured directly. The full FPS difference should not be assumed for every machine or scene.
- Pre-integration outside-view median: 43.2 FPS, against 36.7 FPS after the first spline change.
- Artifacts: `target/performance-284/pool-before.*`, `pool-after.*`.

A final driver's-seat sanity run (`c2-driver-final`) measured 40.1 FPS median with the same Enhanced settings. The scene and mirrors were inspected. Different camera, so not an outside-view A/B result.

## Integration with current main

Upstream `main` at `97b2519` was integrated, then the newly published `200336e` (0.1.328), before opening the PR.

The integration keeps main's ordered world passes, alpha-test/blend distinction and metric surface lift:

- Opaque/cutout spline cells may merge. Declared blended segments keep their own placement origins and meshes for far-to-near composition. The grouping test checks both their separation and their exact placement origins.
- Ground cells use the same `Spline` phase and 8 cm lift as unbatched terrain-mapped splines.

The tables above describe the pre-integration executable. They are not a claim that current main reproduces the same baseline. After integration, 18 geometry unit tests, 21 renderer unit tests (including shader validation), 13 app scene tests and both GPU integration tests passed.

| Build | FPS median (range, last 30 s) | Main batches | Prepass batches | Main render CPU | Mirrors CPU | Encoder finishing | Artifacts |
|---|---|---:|---:|---:|---:|---:|---|
| `9f3afc7` (main `97b2519`) | 35.5 (30.7–36.5) | 5,744 | 3,027 | 10.1 ms | 5.8 ms | 3.44 ms | `main-integrated-final.*` |
| `be8e7f0` (upstream `200336e`, 0.1.328) | 36.0 (31.7–38.8) | 5,744 | 3,027 | 9.5 ms | 5.7 ms | 3.01 ms | `main-328-final.*` |

- Both screenshots were inspected. For `be8e7f0`, all 54 executed unit/integration tests and the release build passed.
- These are different upstream implementations, and the merge keeps the new transparent-spline behaviour. So the pre-integration 43.2 FPS is not the integrated build's FPS or a current-main A/B.
- No current-main run with the optimisations disabled was made: these are sanity measurements.

## Repeat the measurements

1. Build: `cargo build -p omsi-app --release --offline` (omit `--offline` if dependencies are not cached).
2. On Windows, from the repository, run the profiling script once per case:
   ```powershell
   ./scripts/profile-th-wald.ps1 -Root 'D:\OMSI 2' -Name before -NoSplineBatching
   ./scripts/profile-th-wald.ps1 -Root 'D:\OMSI 2' -Name after
   ./scripts/profile-th-wald.ps1 -Root 'D:\OMSI 2' -Name first-fix -NoGroundSplineBatching
   ./scripts/profile-th-wald.ps1 -Root 'D:\OMSI 2' -Name no-pool -NoRenderPool
   ./scripts/profile-th-wald.ps1 -Root 'D:\OMSI 2' -Name driver -View driver
   ```
3. Compare the stationary medians and the shutdown averages.

Script options:

| Option | Default | What it does |
|---|---|---|
| `-Root` | (required) | The OMSI 2 folder |
| `-Name` | `th-wald` | Name of the run's artifacts |
| `-Bus` | `Vehicles/MB_C2_EN_BVG/MB_C2_E6_Gn_BVG_main.bus` | Another installed vehicle |
| `-Exe` | `target/release/openomsi.exe` | A saved executable to test |
| `-View` | `outside` | `outside` or `driver` |
| `-Backend` | `auto` | `auto`, `dx12`, `vulkan` or `gl` |
| `-NoSplineBatching`, `-NoGroundSplineBatching`, `-NoMaterialSplineBatching`, `-NoBoundsCache`, `-NoRenderPool` | off | Disable one change |

The script restores its temporary environment variables. Existing graphics settings apply, so match them across runs.

Do not:

- compare the whole-session average FPS (it includes loading);
- overlap benchmarks;
- compile during the stationary measurement window.

A detailed audit can add some CPU overhead.

**Validation:** geometry and renderer unit tests (including shader validation), the spline grouping tests in the app, bounds lifecycle and presurface GPU integration tests, the release build, and live map runs. The existing automatic-scale test assumed the Mac pixel budget on Windows; its expectations now account for both budgets.
