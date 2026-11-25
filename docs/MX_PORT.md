# Porting the renderer to mX

This page tracks the move of openOMSI's drawing code from wgpu to mX, openOMSI's own graphics
library. It is for contributors who work on the renderer or on mX.

## How it is set up

Every crate that draws has a copy on mX: `omsi-app-mx`, `omsi-render-mx`, `omsi-scene-mx`,
`omsi-ui-mx`, `omsi-navigator-mx`, `omsi-mapdraw-mx`, `omsi-photo-mx`, `omsi-plugin-ui-mx`,
`omsi-multiplayer-ui-mx`, `omsi-settings-ui-mx`, `omsi-mt-mx` and `omsi-xr-mx`. The copies use mX's
own API natively (no wgpu, no wgpu wrapper). The rest of the workspace (formats, simulation,
audio, network) is shared.

- **Choosing the stack.** The **mX Graphics (Experimental)** setting (`mx_graphics` in
  `settings.cfg`, off by default) picks one stack when the game starts. `OMSI_MX=on` or `off`
  overrides it for one run. With it on, `omsi_app::run` hands over to `openomsi_game_mx::run` and
  wgpu is never started. Android and iOS read the setting before they start.
- **Falling back.** If the device is lost under mX, the game starts again on another graphics API,
  and on wgpu if that fails too. If mX finds no GPU, or only a software renderer (WARP, llvmpipe),
  the game starts again on wgpu.
- **The dependency.** mX and `mx-upscale` come from `openOMSI-org/mX`, pinned by git revision in
  the workspace `Cargo.toml`. mX's patched naga (the HLSL buffer heap, Shader Model 5 for
  Direct3D 11) is repeated there as `naga-mx`, because a dependency's `[patch]` does not reach
  this workspace.
- **Changes.** A fix to a drawing crate must go into both copies.

### Graphics APIs on mX

One build keeps every graphics API of the platform; mX picks the backend at run time. The
`graphics_api` setting works as before.

| Platform | Automatic order |
| --- | --- |
| Windows | DirectX 12, Vulkan, DirectX 11, then OpenGL ES through ANGLE (Vulkan first where the driver favours it) |
| Linux, Android | Vulkan, OpenGL |
| Apple | Metal; Metal 2 Macs (Intel, AMD) use mX's bound tier |
| Dedicated server | mX's no-op device |

DirectX 11 (`graphics_api=dx11`) is mX's bound tier, for chips whose DirectX 12 and Vulkan
drivers are missing or broken. On Windows, mX's OpenGL is ANGLE's EGL.

## Status

As of 2026-11-23.

| Area | Status |
| --- | --- |
| Workspace builds without wgpu in the `-mx` crates | Done (macOS; `cargo check` for Windows and Linux) |
| Metal | Runs; tests pass; faster than wgpu |
| Metal 2 (bound tier) | Renderer tests pass on an M4 with `MX_METAL_BOUND=1`, except ray tracing |
| Direct3D 12, Vulkan, OpenGL (ANGLE) on Windows | Compiled, not run on a GPU |
| Direct3D 11 | Compiled, not run |
| Vulkan, OpenGL on Linux | Compiled, not run on a GPU |
| OpenGL tier | Vanilla picture only (16 texture units) |
| Materials and all resources in mX's heap (bindless) | Done; no bind groups left |
| Lean pipelines (Enhanced) | Done; GPU time about halved |
| Upscalers (MetalFX, DLSS, FSR 3, XeSS) and frame generation | Moved to `mx-upscale`; MetalFX tested, the Windows vendors compiled only |
| Enhanced+ (ray tracing) | Works on Metal, matches wgpu |
| OpenXR (VR) | Compiled; needs a Windows GPU to test |
| A windowed session that loads a map | Not yet checked (offscreen drives are) |

### Test results

On an Apple M4, macOS:

- **mX**: 27 conformance tests pass on Metal (with Metal's API and shader validation, with and
  without the residency set, 15 runs in a row) and on MoltenVK (with the Khronos layer and
  synchronization validation: only its two known reports). Unit tests of `mx-shader`,
  `mx-vulkan` and `mx-noop` pass.
- **Renderer**: 194 tests pass on the no-op device; 285 of 289 GPU tests pass on Metal. The other
  four fail the same way on wgpu: the two coplanar print tests (a renderer bug), `xr_diagnostics`
  (Apple's counters give a zero end stamp) and `spv_dump` (a dump tool).
- **Other crates**: `omsi-ui`, `omsi-navigator`, `omsi-scene` and `omsi-plugin-ui` pass; their
  ignored tests need content or benchmark paths.
- **The game**: Grundorf offscreen under Metal's validation shows no error. The picture matches
  the wgpu build's to 91-131 pixels, less than two wgpu runs differ (a shelter poster is random).
  A 40 s hidden-window run with MetalFX shows no error, but stayed in the menu.
- **Golden scenes** (mX against wgpu, same commit): `grundorf_night` identical,
  `grundorf_drive_traffic` within its allowance, the other seven differ by as much as two wgpu
  runs differ from each other (see [Refactor checks](REFACTOR_CHECKS.md#what-repeats-and-what-does-not)).
- **Enhanced+** (`OMSI_ENHANCED_PLUS=1`): no validation error; the frame matches wgpu's to 324
  pixels; traced shadow and occlusion alone (`OMSI_DEBUG_ENHANCED=1`, `2`) to 68 and 18 pixels.

> **Note:** GPU tests run one at a time (`--test-threads=1`). The adapter flags are process-wide,
> and tests running together change each other's.

### Speed

Release builds, Apple M4, `OMSI_BENCH`, medians of alternating runs.

| Scene | mX | wgpu |
| --- | --- | --- |
| Grundorf 1920×1080 Enhanced (2026-11-17) | 7.4 ms a frame (135 fps) | 8.2 ms (122 fps) |
| Grundorf, SD80 from outside, Enhanced+ | 22.3 ms (45 fps) | 24.7 ms (40.5 fps) |

GPU time in the MAN EN92's cab at Grundorf's first stop (`--drive 3`), 1920×1080 Enhanced, 4x
MSAA, SSAO, `OMSI_GPU_TIMERS=1`, before and after the lean pipelines and the ground pass:

| Weather | GPU wait | Main pass | Frame |
| --- | --- | --- | --- |
| Day, Sommerlich 12:00 | 29.5 → 12.6 ms | 22.7 → 6.3 ms | 31.2 → 14.3 ms (32 → 70 fps) |
| Rain, Schmuddelwetter | 26.6 → 15.3 ms | 19.8 → 8.3 ms | 27.9 → 16.9 ms (36 → 59 fps) |
| Night 22:30 | 28.1 → 13.1 ms | 20.8 → 5.9 ms | 30.8 → 15.7 ms (32 → 64 fps) |

On Metal 2 (`MX_METAL_BOUND=1`) the day case takes 12.9 ms, main pass 6.3 ms. The heap made no
difference to these times.

> **Warning:** Debug builds are no measure: `objc2` checks every Objective-C message there. Also
> check that a benchmark's log has no "drawing with 1x MSAA": a pipeline the driver refused once
> writes `~/.openomsi/gpu-fallback.cfg`, which turns every later run to 1x MSAA and basic
> pipelines. Isolate `HOME` for benchmarks.

`tools/mx_port/bench_wgpu_vs_mx.py` runs the same scenes with wgpu and mX builds in turns and
reports frame time and peak memory.

## Plan

The goal is one set of drawing crates on mX, with no wgpu. What is left, in order:

1. **Run on other GPUs.** Direct3D 12, Vulkan and ANGLE on Windows; Vulkan and OpenGL on Linux.
   Check the DLSS and FSR 3 states with enhanced barriers, and the OpenXR handover
   (`openxr/sync/tests.rs`, needs a Windows GPU).
2. **Heap on Direct3D.** Direct3D 12 and 11 read the newer heap bindings (storage textures,
   acceleration structures, multisampled depth) compiled, not run.
3. **A windowed session that loads and drives a map** on mX.
4. **OpenGL gaps.** `glSampleMaski` is not in glow, so mX's GL ignores the sample mask (logged).
   The OpenGL tier's FXAA has no screen mask, so a display behind a pane is antialiased there
   (`display_behind_a_pane_keeps_its_pixel_contrast`).
5. **Offer mX everywhere**, then retire the wgpu copies once mX has drawn the game on real cards
   on every platform.

mX's own status is in its repository (`docs/STATUS.md`).

## How the renderer uses mX

### Errors and readbacks

- **Errors**: mX returns errors where they happen (no uncaptured-error handler, no error scopes).
  `pipelines/errors.rs` keeps the old flags behind one `report` function, fed by the device polls
  (`DevicePoller`, `frame_hooks`) and by `render_inner`. Each frame runs in `gpu_attempt`: a
  resource mX cannot make ends that frame, not the game. Device loss is `mx::Error::DeviceLost`.
- **Readbacks** (exposure, lamp sight, previews, the remote cockpit picture): the submission index,
  `Device::is_done` / `wait_gpu(Some(index))`, then `Buffer::map_read_now`.
- **Limits**: `OMSI_GPU_LIMITS` goes through `mx::DeviceDescriptor::limits`.
- **Array paths**: `device_selection.rs` decides them from
  `mx::Adapter::storage_buffers_per_stage` and `texture_units`.

### The heap (bindless)

The renderer makes no bind groups. Every shader reads its resources from mX's heap.

- **Materials**: a material's textures and samplers are heap indices, its numbers a 256-byte row
  of one uniform table (`resources/materials.rs`, `MaterialTable`). A draw sets the row's offset
  and its immediates. Scene shaders read `mx_tex2d[mx_mat.diffuse]` where they read `t_diffuse`
  (`shaders/sources.rs`, `materials_from_heap`). Rows are reused four frames after their material
  goes.
- **The scene shader** (`frame_from_heap`, `FrameHeap`): the camera group's textures, samplers and
  buffers, the material row and the puddles' vehicle plane are read at heap places carried in the
  draw's immediates: 32 `u32`, 128 bytes, the most Vulkan and mX's Direct3D 12 promise.
- **Every other module** (`groups_to_heap`, `HeapGroup`): its root declarations become heap reads
  at places in its own immediates, in group and binding order (`HeapLayout`, worked out from the
  shader source). `replace_globals` replaces a module-scope name only where no parameter or local
  shadows it. The camera group is a `HeapGroup` per module (`CameraHeaps`).
- **Ray tracing, multisampled depth, motion**: storage textures (`mx_storage{k}`, bindings
  12-15), acceleration structures (`mx_tlas`, binding 11), writable buffers and multisampled depth
  views (`mx_texdepth_ms`, binding 6) are heap entries. The pane depth reads as `t_pane_depth` from
  the scene shader's `ground` place.
- **Interface and VR mirror** (`omsi-ui-mx`, `omsi-xr-mx`): a layer's numbers and its picture and
  sampler places are the draw's 128 bytes of immediates.
- **Per backend**: on Metal mX binds the heap's textures and views directly and keeps the frame's
  constants in the constant address space. OpenGL without storage buffers in a stage reads them as
  uniform blocks. On Direct3D 12 the heap's free slots hold empty views, not freed textures'.

On Metal the heap costs nothing: the main pass took 4.41-4.50 ms against 4.37-4.58 ms with bind
groups (5.09-5.17 ms through Metal's argument buffer).

### Lean pipelines

The Enhanced fragment shader was the cost: the rare branches (glass and films, water, bump and
PBR maps, LED panels, glow, debug views, wet road and puddles, snow) took registers from every
pixel.

- Plain materials draw with pipelines specialized without them (`override LEAN`, `LEAN_GLASS`,
  `WET`, `SNOW`; `frame_plan.rs` `main_key`). Blended ones keep the glass branch.
- There are four sets, by the frame's weather: dry, rain, snow, both.
- They are compiled on two background threads after the renderer is up. Until then, and where the
  driver fails on one, the full pipeline draws. (Apple's compiler service crashed on wet-and-snow
  variants together and once hung, hence the split and the background threads.)
- `OMSI_NO_LEAN=1` draws with the full pipelines, for comparison.

The ground pass now works with MSAA and SSAO: the ground picture is read through the heap
(`FrameHeap::ground`) and drawn with a depth of its own. Before, the painted terrain's layers were
lit one by one and blended (4 ms) unless MSAA was 1x and SSAO off.

### Upscalers and VR

- The vendors' code lives in mX's `mx-upscale` crate. The game's `temporal/` keeps its side
  (settings, jitter, motion vectors, the HUD) and calls `mx_upscale::Upscaler` and
  `FrameGenerator`.
- New with it: Intel XeSS, and frame generation on Windows (DLSS 2x-4x, FSR 3, XeSS), offered
  where their DLLs are beside the game (`omsi_cfg::upscaler_files`). The vendor then makes the
  window's swapchain and the surface is configured again.
- mX leaves every texture in a fixed state between commands (Direct3D 12: its home state,
  `Texture::rest_state`; Vulkan: `GENERAL`). The vendors are told that state.
- DLSS on Vulkan gets its device extensions through `mx::DeviceDescriptor::vulkan_extensions`.
- OpenXR swapchain images are mX textures (`texture_from_raw`), moved from `RENDER_TARGET` to their
  home state after `xrWaitSwapchainImage` and back before `xrReleaseSwapchainImage`
  (`openxr/sync.rs`).

### Other differences from wgpu

- mX has no dynamic offsets: `omsi-ui`'s layers and the ray tracer's jobs use one place per slot.
- `bytemuck` needs `min_const_generics` explicitly (wgpu used to turn it on).
- On OpenGL, a view dropped before its command buffer is submitted keeps its heap entry until that
  submission is done (openOMSI-org/mX#17). Blending and colour masks are set per draw buffer
  through `OES_draw_buffers_indexed` on ES 3.0 and 3.1.

### Bugs found while porting

- The port lost the `DETAILED_TREES` constant of the scene's vertex stage and depth prepass, so
  the sky showed through trees.
- `OMSI_GPU_LIMITS` was not applied.
- A frame mX refused was reported as the end of the game (now caught quietly).

## Checking other platforms from a Mac

Without the platforms' C toolchains you can still type-check (`cargo check`; nothing is linked):

- **Windows**: `cargo check -p omsi-app-mx -p omsi-xr-mx --target x86_64-pc-windows-gnu`. MinGW
  from Homebrew builds `ring`; the MSVC target lacks its C headers.
- **Linux**: the same with `--target x86_64-unknown-linux-gnu`, with stand-ins that make empty
  objects for the C parts and answer for pkg-config: `CC_x86_64_unknown_linux_gnu`,
  `CXX_x86_64_unknown_linux_gnu`, `AR_x86_64_unknown_linux_gnu`, `PKG_CONFIG` and
  `PKG_CONFIG_ALLOW_CROSS=1`.

> **Warning:** Use `CARGO_INCREMENTAL=0` and run one build at a time: the disk is small.

## The mechanical port

The first pass was done with scripts in `tools/mx_port/`:

- Cargo: the drawing crates depend on `mx` instead of `wgpu`; `wgpu::` became `mx::`
  (`wgpu::util::*` to mX's own names).
- `port_layouts.py`: removed descriptor fields mX has no concept of (`layout`, `cache`,
  `compilation_options`, `multiview_mask`, `depth_slice`, `mapped_at_creation: false`) and the 55
  bind group and pipeline layout statements. It also removed some non-GPU `layout` and `cache`
  fields in `omsi-scene`, which were put back by hand.
- `port2.py`: `ShaderSource::Wgsl(..)` to `&str`, `CommandEncoderDescriptor` to the label,
  timestamp writes, empty `set_bind_group` offsets, `.expect` on `create_*` calls that return
  `Result` in mX.
- `port3.py`: pipeline stage constants, depth state, inline leftovers.
- `port4.py`: the rest of the layout entries, buffer sizes, acceleration structures.
- `port5.py`: layout leftovers in one-line literals and calls, buffer slices, features, polls.
- `port_tests.py`: the tests' instances, adapters, error scopes and polls.
- `gpu/schema.rs` keeps the camera and material groups' bindings as openOMSI's own data, still
  used to count GL texture units (`lamps_fit`) and by tests.
