# Diagnosing missing shadows with temporal upscaling

Related reports: #3809, #4040, #4221, #4271 and #4345.

The existing `OMSI_DEBUG_SHADOW_FAR=1` flag now also records the main window's shadow state and encoded cascade batch counts on both wgpu and mX. Enable only for short diagnostic captures; per-frame logging has a cost. Normal play is unchanged.

## Controlled A/B

Use the same map, bus, camera, time, weather, preset, render API and shadow settings. Start each run from a fresh process. Disable frame generation and the automatic optimiser for this test. Record the exact build SHA, GPU/driver, backend, game log and a short video.

1. Native rendering, 1x MSAA, 100% render scale.
2. DLAA (native), then DLSS Quality.
3. FSR 3 Native, then FSR 3 Quality.
4. Repeat a failing case with `OMSI_SHADOW_NEAR_EVERY_FRAME=1` and separately with `OMSI_SHADOW_FAR_EVERY_FRAME=1`.
5. If Enhanced+ is affected, repeat with `OMSI_NO_RT_FRAME=1`; also test Enhanced without ray tracing.
6. Repeat with DX12/Vulkan and wgpu/mX where supported. With `OMSI_PROFILE=1`, compare VRAM and texture-budget logs in the affected run.

In PowerShell, before starting the game from that process:

```powershell
$env:OMSI_DEBUG_SHADOW_FAR = "1"
$env:OMSI_PROFILE = "1"
```

## Interpreting the output

- `shadow state`: whether the main view requests and schedules sun/moon shadows, whether temporal upscaling and ray tracing are active, effective shadow size, cascade update decisions, sun direction/intensity and render size.
- `shadow batches`: the number of batches submitted to the shadow-map encoder for each cascade. A zero count for a cached near/far cascade is expected when no redraw is scheduled; a zero count is not proof that an object has stopped casting shadows. In ray-traced mode some opaque casters are intentionally omitted from raster maps.
- If settings remain enabled and the close cascade still has batches while shadows disappear, compare the scene before temporal reconstruction with the final image; inspect depth, motion, and ray-traced lighting next.
- If maps shrink or a rebuild occurs, correlate with OOM and renderer fallback messages; do not infer a renderer failure from FPS alone.

This instrumentation is diagnostic, not a confirmed visual fix. GPU validation on the affected Windows configuration is still required.

## Long-session memory comparison

Run the same 30–60 minute round trip on a 4 GB card, keeping camera range, texture budget, traffic and buses fixed. Log `OMSI_PROFILE=1` and record dedicated/shared VRAM, live textures/meshes and allocator allocated/reserved bytes (where reported). Repeat a return to an unloaded area and vehicle despawn/spawn. Compare the full-resolution nearby textures and peak memory, not just FPS. If pressure persists while stationary with no expired tiles, investigate retained references and allocator fragmentation separately. The streaming change only prioritizes eviction of already-expired tiles; it does not force eviction of live assets or bypass hard limits.

### CPU memory and decode backlog

The profile log now includes queued texture upgrades, outstanding jobs (including results waiting for upload), completed results and their decoded payload bytes. The admission limit is 32 outstanding upgrades per world, not a byte cap on all texture memory. Compare the process private/committed memory with working set and shared GPU memory; a retained allocator page is not by itself a live allocation leak. Test a stationary view after loading, repeated travel over the same route, repeated bus switches, and map exit/re-entry. Check whether queue counts and decoded bytes return to a baseline after streaming settles. The native Windows heap release helper is currently a no-op; no forced working-set trimming is added.

### Automatic shadow shutoff

A report of "The graphics card cannot keep up: shadows off for this drive" identifies the frame governor in both app crates, not an upscaler-specific shader failure. The governor now preserves both shadows and SSAO, leaves temporal upscaler resolution to the upscaler, and gives at most two four-second advisories per session after six consecutive five-second low-FPS GPU/presentation-bound windows. Nonqualifying samples reset the streak. Frame-generation and OOM recovery remain separate. Verify with automatic render scale, a sustained GPU-bound workload below 30 drawn FPS, and DLSS/FSR/native runs. Confirm the shadows setting remains enabled, then repeat with it explicitly disabled. Actual GPU allocation failure recovery remains separate. A possible trade-off is lower FPS on weak GPUs.

### mX UI memory parity

Port the wgpu high-water buffer reclamation from #4264 to mX. After 180 uploads using less than one quarter of a buffer of at least 1 MiB, shrink the GPU vertex buffer and CPU staging vector independently. Keep the old GPU buffer if the smaller allocation fails. This addresses retained UI capacity, not a proven leak in the underlying mX driver library.
