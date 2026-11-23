# Porting the renderer to mX

The renderer and the crates that draw (`omsi-render`, `omsi-app`, `omsi-ui`, `omsi-navigator`,
`omsi-scene`, `omsi-plugin-ui`, `omsi-xr`) move from wgpu's API to mX's own, natively: no wgpu,
no `wgpu-mx`. One build keeps every graphics API of the platform (mX picks the backend at run
time), so the graphics API setting stays as it is: Direct3D 12, Vulkan and OpenGL (ANGLE) on
Windows, Vulkan and OpenGL on Linux and Android, Metal on Apple platforms.

Branch `mx-native` (local; mX as a path dependency, `../mX/crates/mx`, until mX's revision is
pinned).

## State (2026-11-17)

The workspace builds without wgpu: `cargo check --workspace` on macOS, and `omsi-app` with
`omsi-xr` for `x86_64-pc-windows-gnu` and `x86_64-unknown-linux-gnu` (see "Checking other
platforms"). The tests are ported and run on this Mac's GPU (see "Tested").

What changed besides the names:

- **Graphics API setting** (`omsi-app/src/startup.rs`): mX is the game's only graphics library.
  `backend_order` is as before; each interface is `mx::Instance::with_backends`, ANGLE is
  `omsi_render::angle::instance()` (mX's OpenGL on Windows *is* ANGLE's EGL, so OpenGL there
  means ANGLE). The old `mx_graphics` switch stays in the settings file and is not read. The
  dedicated server opens mX's no-op device (`Backends::NOOP`).
- **Errors**: mX returns errors where they happen (no uncaptured-error handler, no error
  scopes). `pipelines/errors.rs` keeps the old flags behind one `report` function, fed by the
  device polls (`DevicePoller`, `frame_hooks`) and by `render_inner`, which runs each frame in
  `gpu_attempt`: a resource mX could not make ends that frame, not the game (wgpu handed out an
  invalid object instead). Device loss is `mx::Error::DeviceLost` (new in mX).
- **Readbacks** (exposure, lamp sight, previews, the remote cockpit picture): the submission
  index, `Device::is_done` / `wait_gpu(Some(index))`, then `Buffer::map_read_now`.
- **Dynamic offsets** (mX has none): one bind group per slot (`omsi-ui`'s layers, the ray
  tracer's texture means and coordinate jobs).
- **Upscalers** (2026-11-18): the vendors' code moved to mX's `mx-upscale` crate; `temporal/`
  keeps the game's side (settings words, jitter, motion vectors, the HUD) and calls
  `mx_upscale::Upscaler` and `FrameGenerator`. New with it: Intel XeSS upscaling, and frame
  generation on Windows (DLSS 2x-4x, FSR 3, XeSS), offered where their DLLs are beside the game
  (`omsi_cfg::upscaler_files`); the vendor then makes the window's swapchain and the surface is
  configured again. MetalFX's GPU tests (13, frame generation among them) pass through it; the
  Windows vendors are compiled, not run.
- **Upscalers (before) and VR**: MetalFX through `as_metal()` (`texture_from_raw` for the drawable,
  `Surface::with_metal` for the layer); DLSS and FSR 3 through `as_dx12()` / `as_vulkan()`.
  mX leaves every texture in a fixed state between its commands (Direct3D 12: its home state,
  `Texture::rest_state`; Vulkan: `GENERAL`), so the vendors are told that state and the empty
  passes that put the inputs in wgpu's last-use states are gone. DLSS on Vulkan gets its
  device extensions through `mx::DeviceDescriptor::vulkan_extensions`; mX turns on buffer
  device addresses wherever the device has them. OpenXR: swapchain images are mX textures
  (`texture_from_raw`), moved from `RENDER_TARGET` to their home state after
  `xrWaitSwapchainImage` and back before `xrReleaseSwapchainImage` (`openxr/sync.rs`).
- **naga**: the workspace uses naga 30 patched with mX's `vendor/naga` (the HLSL buffer heap);
  a path dependency's `[patch]` does not reach this workspace, so it is repeated in
  `Cargo.toml`. bytemuck needs `min_const_generics` explicitly now (wgpu turned it on).

## Tested (2026-11-17, Apple M4, macOS)

- **mX**: 27 conformance tests pass on Metal (under Metal's API and shader validation, with and
  without the residency set, 15 runs in a row) and on MoltenVK (under the Khronos layer with
  synchronization validation: only its two known reports). Unit tests of mx-shader, mx-vulkan,
  mx-noop pass.
- **Renderer** (`omsi-render`): 194 tests pass on the no-op device; 86 of 90 GPU tests pass on
  Metal. The other 4 fail the same way on a wgpu build of the same commit: the two coplanar
  print tests (218 / 11 pixels on both, a bug of the renderer), `xr_diagnostics` (Apple's
  stage-boundary counters give a zero end stamp, also on wgpu) and `spv_dump` (a dump tool).
  GPU tests run one at a time (`--test-threads=1`): the adapter flags are process-wide and
  tests running together change each other's.
- **Other crates**: `omsi-ui`, `omsi-navigator`, `omsi-scene`, `omsi-plugin-ui` pass; their
  ignored tests need content or benchmark paths.
- **The game**: Grundorf offscreen under Metal's validation, no error; the picture matches the
  wgpu build's (91-131 pixels apart, less than two wgpu runs differ from each other: a bus
  shelter's poster is chosen at random). A 40 s windowed run (`OMSI_HIDDEN_WINDOW`) under
  Metal's validation, MetalFX on, no error (it stayed in the menu: the map was not driven).
- **Speed** (release builds, Grundorf 1920x1080 Enhanced, `OMSI_BENCH=600`, medians of 5
  alternating runs): mX 0.50 ms recording, 6.2 ms GPU wait, 7.4 ms a frame (about 135 fps);
  wgpu (the installed 0.3.54 release, six days older) 0.55 ms, 7.4 ms, 8.2 ms (about 122 fps).
  Debug builds are no measure: objc2 checks every Objective-C message's signature there.
- **Golden scenes** (`scripts/golden.sh`, dev builds of the same commit, mX against wgpu):
  `grundorf_night` identical; `grundorf_drive_traffic` within its allowance (0.001 % of
  pixels; the SD202 started by its switches, driven 12 s with AI traffic); the other seven
  differ by as much as two wgpu runs differ from each other (day 12.8 %, rain 23.7 % of the
  pixels on wgpu against wgpu: animated particles - `REFACTOR_CHECKS.md` still says those
  scenes repeat byte for byte, which no longer holds on wgpu either).
- **Enhanced+** (ray tracing, `OMSI_ENHANCED_PLUS=1`): the device is opened with ray queries;
  under Metal's validation, no error; the frame matches wgpu's (324 pixels apart), and the
  traced shadow and occlusion alone (`OMSI_DEBUG_ENHANCED=1`, `2`) match to 68 and 18
  pixels. Release, 1920x1080, Grundorf with an SD80 from outside, medians of 3 runs each:
  mX 1.16 ms recording, 20.3 ms GPU wait, 22.3 ms a frame (about 45 fps); wgpu 0.3.54 1.45 ms,
  22.5 ms, 24.7 ms (about 40.5 fps).
- **Windows and Linux**: everything, tests included, compiles (`cargo check`, see below); not
  run.

Found and fixed while testing: the port had lost the `DETAILED_TREES` constant of the scene's
vertex stage and of the depth prepass (`port_layouts.py` took one-line `compilation_options`
out with their constants): the prepass and the colour pass shaped trees differently and the
sky showed through bushes and trees. `OMSI_GPU_LIMITS` was not applied (now through
`mx::DeviceDescriptor::limits`). A frame mX refused was reported by the game's panic hook as the
end of the game (now caught quietly). In mX: see its `docs/STATUS.md`.

## Materials in the heap (2026-11-17)

A material is no longer a bind group. Its textures and samplers are heap indices, its numbers a
256-byte row of one uniform table (`resources/materials.rs`, `MaterialTable`); a draw sets the
row's offset (`set_buffer_offset(1, 2, ..)`) and 48 bytes of immediates (the row and the eleven
indices). The scene shaders read `mx_tex2d[mx_mat.diffuse]` where they read `t_diffuse`
(`shaders/sources.rs`, `materials_from_heap`); rows are reused four pictures after their
material goes. On Metal mX binds those textures directly (its `docs/ARCHITECTURE.md`), and the
main pass runs as fast as with bind groups: Grundorf 1920x1080 Enhanced, release, M4, three runs
each, 4.41-4.50 ms (bind groups 4.37-4.58 ms; through Metal's argument buffer 5.09-5.17 ms). The
pictures match the bind-group build's to run noise (510 pixels of 2 million); 194 + 86 tests
pass as before, under Metal's validation.

## GPU work (2026-11-23)

The cab of the MAN EN92 at Grundorf's first stop (`--drive 3`), 1920x1080 Enhanced, the
default settings (4x MSAA, SSAO), release, M4, `OMSI_BENCH=300` and `OMSI_GPU_TIMERS=1`, cost
28 ms of GPU a frame on mX and 25 ms on wgpu (the speed figures above are another view): the
main pass alone 19 to 22 ms. Neither multisampling (1x: the same) nor the number of draws
(240) was the cause; the enhanced fragment shader was. Cut to the texture alone it took 1.3 ms.

- **Lean pipelines** (`override LEAN`, `LEAN_GLASS`, `DRY` in the scene shader,
  `frame_plan.rs` `main_key`): the shader's branches are uniform per draw, but the code of the
  rare ones (glass and its films, water, bump and PBR maps, LED panels, glow, debug views; on a
  dry frame the wet road, its puddles and the snow) took registers from every pixel. Plain
  materials are drawn with pipelines specialized without them, blended ones with the glass
  branch kept; the weather's branches come in a wet and a dry set. 21.9 -> 10.3 ms of main
  pass on a sunny day. A lean pipeline the driver fails on is drawn with the full one.
- **The ground pass with MSAA and AO**: the painted terrain's layers were lit each in full
  and blended (4 ms here) unless MSAA was 1x and SSAO off, because the ground picture took the
  camera group's AO slot. It is read through the heap now (`FrameHeap::ground`, immediates
  after the material's places) and drawn with a depth of the terrain's own, so a
  multisampled pass finds the ground at the edges too. 10.3 -> 6.5 ms.

| Grundorf cab, GPU wait a frame (median) | before | after |
|---|---|---|
| day, Sommerlich | 28.2 ms (main 21.9) | 10.5-12.9 ms (main 5.4-6.5) |
| rain, Schmuddelwetter | 25.6 ms (main 19.4) | 12.3 ms (main 7.2) |
| night 22:30 | 26.1 ms (main 19.8) | 12.9 ms (main 6.0) |

`OMSI_NO_LEAN=1` draws with the full pipelines, for comparison.

## Older GPUs (2026-11-18)

- **Direct3D 11** is a graphics API choice on Windows (`graphics_api=dx11`, "DirectX 11" in the
  launcher), and comes after DirectX 12 and Vulkan in the automatic order, before ANGLE: mX's
  bound tier, for chips whose DirectX 12 and Vulkan drivers are missing or broken. Compiled,
  not run.
- **Metal 2** Macs (Intel, AMD) start on mX's bound tier with no setting; the renderer's tests
  pass on an M4 run so (`MX_METAL_BOUND=1`), but those needing ray tracing.

## Left

- **GPU checks off this Mac**: Direct3D 12, Vulkan and OpenGL (ANGLE) on Windows; Vulkan and
  OpenGL on Linux; DLSS / FSR 3 states with enhanced barriers; the OpenXR handover
  (`openxr/sync/tests.rs`, rewritten for it, compiles; needs a Windows GPU).
- **A windowed session that loads a map** on mX (the windowed run above stayed in the menu;
  driving and Enhanced+ are checked offscreen).
- **Heap**: the other root bind groups (camera, lights, shadows) stay as they are (mX keeps
  groups 0..=6); per-draw arrays move to the heap pass by pass, with a pixel test each.
- `glSampleMaski` is not in glow: mX's GL ignores the sample mask (logged).

## Checking other platforms

From a Mac, without the platforms' C toolchains (`cargo check` only: nothing is linked):

- Windows: `cargo check -p omsi-app -p omsi-xr --target x86_64-pc-windows-gnu` (mingw-w64 from
  Homebrew builds `ring`; the MSVC target lacks its C headers).
- Linux: the same with `--target x86_64-unknown-linux-gnu`, with stand-ins that make empty
  objects for the C parts and answer for pkg-config: `CC_x86_64_unknown_linux_gnu`,
  `CXX_x86_64_unknown_linux_gnu`, `AR_x86_64_unknown_linux_gnu`, `PKG_CONFIG` and
  `PKG_CONFIG_ALLOW_CROSS=1`.
- `CARGO_INCREMENTAL=0`, one build at a time: the disk is small.

## Done earlier (mechanical, `tools/mx_port/`)

- Cargo: the drawing crates depend on `mx` instead of `wgpu`.
- `wgpu::` → `mx::` everywhere in those crates (`wgpu::util::*` to mX's own names).
- `port_layouts.py`: descriptor fields mX has no concept of (`layout`, `cache`,
  `compilation_options`, `multiview_mask`, `depth_slice`, `mapped_at_creation: false`) and the
  55 `create_bind_group_layout` / `create_pipeline_layout` statements; layout fields and
  parameters. (It also took `layout: &TileLayout` parameters and a `cache:` field that are not
  the GPU's out of `omsi-scene`: put back by hand.)
- `port2.py`: `ShaderSource::Wgsl(..)` to `&str`; `CommandEncoderDescriptor` to the label;
  `*PassTimestampWrites`; `set_bind_group(.., &[])`; `.expect` on the `create_*` calls that
  return `Result` in mX.
- `gpu/schema.rs`: the camera and material groups' bindings as openOMSI's own data, still used
  to count GL texture units (`lamps_fit`) and by tests. `device_selection.rs` decides the array
  paths from `mx::Adapter::storage_buffers_per_stage` and `texture_units`.
