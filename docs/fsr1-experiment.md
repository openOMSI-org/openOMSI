# FSR 1 test build (opt-in)

This branch adds AMD FidelityFX Super Resolution **1.x spatial upscaling** using EASU + RCAS, using shaders derived from AMD's MIT-licensed reference. No native driver DLLs are required.

## Enable and compare

FSR1 is **disabled by default**. Open **Settings > Graphics > Upscaling** in the launcher (or **Esc > Options > Graphics** inside the game) and choose **AMD FSR 1 (experimental)**. Choose an **FSR quality** preset (Ultra Quality 77%, Quality 67%, Balanced 59%, Performance 50%, Automatic, or Custom) and **FSR sharpness** (0-100%). Settings are persisted in `settings.cfg` and included in graphics profiles.

**In the game the change shows at once**: FSR 1's passes are compiled the first time it is chosen (nothing for whoever never does). If the graphics driver refuses them, the game says so once and keeps the standard upscaler for the rest of the run. The FSR quality row shows the size the 3D picture is drawn at and the window's (`1286×723 … 1920×1080`).

- **Automatic** leaves the size to the frame-rate governor (render scale *Auto*): the picture is drawn smaller while the card cannot keep the frame rate (85, 70, 55 %) and FSR 1 scales it up; at full size nothing is upscaled.
- For a quick 1080p test, select **Quality**; the game will use a 0.67 internal scale.
- A **Render scale** below the preset's still caps it: the optimiser's steps down and the
  lighter graphics after a lost graphics device (75 %) lower the render scale, and the
  picture is drawn at the smaller of the two (`fsr1_presets_keep_the_render_scale_caps`).
- In VR, `OMSI_OPENXR_SCALE` takes the place of the eye resolution once: with FSR 1 the
  headset image stays full-sized and the eyes are drawn at that share of it.
- Ultra Quality: 0.77; Balanced: ~0.59; Performance: 0.50 (the renderer clamps fixed scales to 0.5).
- FSR is used for the window, for each panel of a triple screen, and in VR (below). The unchanged Catmull-Rom upscaler is used elsewhere (mirrors, offscreen pictures) and whenever FSR is off or nothing is scaled.
- **VR:** without FSR the headset's image itself is made at the eye resolution (`vr_scale`, 65 % by default) and the runtime scales it. With FSR on when VR starts, the image is full-sized and each eye is drawn at the eye resolution and scaled up to it by FSR 1 - sharper at the same drawing cost, plus the two FSR passes per eye and a larger image for the runtime to show. Turned off during the VR session, the eyes stay at that size and the standard upscaler takes over. **Not tried on a headset yet**: the renderer's part is tested offscreen (`a_vr_eye_with_fsr1_is_drawn_smaller_and_scaled_up`).
- **Mirrors** keep their own size: they are small (256 by default) and FSR would add two passes per mirror picture for little; their GPU time shows as `mirrors: mirror` in the measurements below, which decides whether that is worth trying.
- RCAS sharpening is controlled by the FSR sharpening slider (defaults to **80%**). Its whole range maps onto 0-0.8 of AMD's RCAS strength (`fsr1_rcas_strength`): measured below, AMD's strongest (1.0) was overdone - grain on the gauges, dark outlines - and 0.8 was as sharp as the standard upscaler and clean, so that is the slider's 100%; 30% of the old scale was too soft. *A `settings.cfg` saved with the old scale keeps its number, now 80% as strong: 0.3 there was too soft already, so raise it.* At 0% RCAS would give its input back, so it is skipped: EASU draws the window's picture itself, one full-screen pass and its full-size intermediate fewer (`fsr1_without_sharpening_skips_rcas`). Only `FSR1 EASU` then shows in the pass times.
- A picture FSR does not draw (a mirror, vanilla FXAA, an offscreen view) keeps the standard upscaler's own sharpening; the slider is RCAS's only.
- The `OMSI_FSR1=1` environment override still works for developer testing; with it the existing **Render scale** setting controls the internal resolution.

## Measuring it (send numbers, not impressions)

Compare at the same place, standing still, with MSAA, backend and window size held fixed, and the frame limit off (`max fps` 0 or high: a limit under the card's rate hides every difference, and below 50 it also stops the governor).

**Frame rate, in one run.** Start with `OMSI_PROFILE=1`: once a second the log (`game.log`) says `profile interval: 30.3 fps over 1.01 s`. Switch the upscaler or the FSR quality in Esc > Options > Graphics (no restart needed) and wait a minute between switches; the averages of each stretch compare. To compare FSR 1 with the standard upscaler *at the same internal resolution*, set the standard one's **Render scale** to the FSR quality's (67 % for Quality).

**GPU time per pass, one run per setting.** The passes' times (`OMSI_GPU_TIMERS=1`) are written once, when the game ends by itself after `--exit-after <seconds>`: `profile gpu pass FSR1 EASU: … ms`, `FSR1 RCAS`, or `upscale` for the standard upscaler, and `profile: window 1161x653, scene drawn at 778x437`. Start `openomsi.exe` with the command line the log's `command line:` names plus `--exit-after 120` (PowerShell: `$env:OMSI_PROFILE='1'; $env:OMSI_GPU_TIMERS='1'; .\openomsi.exe <that command line> --exit-after 120`), once with each setting. With `OMSI_PROFILE_JSON=<file>` the same summary goes to a file, and `scripts/compare-performance.py a.json b.json` compares two.

## Results so far (one machine)

AMD Radeon R7 240 class card (GCN 1, Vulkan, 4 GB), 20-thread CPU, Windows. Window 1161x653,
MSAA 2x, SSAO on, frame limit off. Ponte Rasa, standing in the cab, same start for every run,
150 s each (`--exit-after 150`), frame rate averaged over the ~85 s after loading;
`OMSI_GPU_TIMERS` for the pass times. The game waits on the graphics card (`profile acquire`
16.8-18.6 ms of a ~27 ms frame), so GPU time saved shows as frame rate. Between two runs of
the same setting the average moved by up to 0.8 fps: differences under ~1 fps are a tie.

| setting | scene | fps | GPU (timed passes) | upscale / EASU / RCAS (ms) |
|---|---|---|---|---|
| standard 67 % | 778x438 | 36.8-37.6 | 4.5-5.6 ms | 1.62-1.66 / - / - |
| standard 59 % | 685x385 | 40.4-41.0 | 5.0-5.2 ms | 1.62-1.66 / - / - |
| FSR 67 % (Quality), sharpening 30 % | 778x438 | 34.5-34.8 | 9.0 ms | - / 2.75 / 0.93 |
| FSR 67 %, sharpening 0 (RCAS skipped) | 778x438 | 35.2 | 7.6 ms | - / 2.85 / - |
| FSR 59 % (Balanced), sharpening 0 | 685x385 | 38.8-38.9 | 7.3-7.5 ms | - / 2.81 / - |
| FSR 59 %, sharpening 30 / 80 / 100 % | 685x385 | 37.3-37.8 | 8.7-9.0 ms | - / 2.70 / 0.93 |
| FSR 50 % (Performance), sharpening 0 | 581x327 | 42.5-42.6 | 7.0 ms | - / 2.80 / - |
| FSR 50 %, sharpening 30 / 80 / 100 % | 581x327 | 40.4-41.2 | 8.2-8.5 ms | - / 2.70 / 0.93 |

(The "timed passes" figure leaves out the first main-pass part on frames where it is not timed;
compare it between rows, not with the frame time.)

- **Cost.** EASU alone (2.7-2.85 ms) costs more than the whole standard upscaler (1.62-1.66 ms, its
  sharpening included); RCAS adds ~0.93 ms whatever its strength. At the same internal size FSR
  is 2-2.5 fps slower than the standard upscaler on this card.
- **As an optimisation.** FSR at 50 % with useful sharpening (40.4-41.2 fps) ties the standard
  upscaler at 59 % (40.4-41.0 fps). On this card it buys no frame rate over simply lowering the
  standard render scale, which at 59 % looks nearly the same as at 67 %.
- **Picture** (screenshots from the same spot via `OMSI_INPUT="t=140 shot <file.png>"`, cropped
  and enlarged; brightness identical across all runs within 1/255):
  - standard: sharper, readable gauge numbers and bus lettering, but stair-steps on diagonals
    (the viaduct's underside) and dark/light halos around foliage from its strong CAS (0.66 at
    67 %, 0.8 at 59 %);
  - FSR: clean diagonals, no stair-steps or CAS halos; with 0-30 % sharpening clearly softer
    (gauge numbers and lettering blur); at 80 % close to the standard's sharpness and clean; at
    100 % overdone - grain on the gauges, dark outlines around the bus and trees.
- **Sharpening scale.** RCAS's strength grows steeply near the top (it divides by `1 + 4 * lobe`):
  on the strongest edges the centre is weighted ~1.3x at 30 %, ~2.5x at 80 %, ~4x at 100 %.

### Since these measurements (to be measured again)

- **Flat areas:** where the four texels round an output pixel differ by less than 1/1024 in every
  channel (sky, road, walls), EASU's result - clamped to their range - can only be within that of
  their bilinear mix, so that is drawn and the other eight taps (24 square roots) and the edge
  analysis are skipped. On the test scene the picture is identical to the byte with and without it.
- **RCAS's input:** EASU writes the intermediate in perceptual values, in `Rgb10a2Unorm` (32 bits a
  pixel, as before), and RCAS reads it back without its 15 square roots a pixel. Against the old
  sRGB intermediate, 99.8 % of the test picture's bytes are the same and the rest one step apart
  (in eight bits the values were rounded twice, a step off on a third of the pixels).
- The slider's new scale and its 80 % default (above).

- **Textures at the window's detail (mip bias):** drawn smaller, the scene took its textures'
  mips for the smaller picture - letters, gauges and signs blurred before FSR saw them. With
  FSR 1 scaling the picture up, the scene's textures are now read with a mip bias of log2 of
  the scale (-0.58 at 67 %, -1 at 50 %), AMD's advice: as detailed as the window's size wants,
  never more. On a checkered wall square on to the camera at 50 % the checks' contrast is
  147 986 with the bias and 14 064 without (the mips' grey), against 708 212 drawn at full size
  (`fsr1_textures_keep_their_detail_with_the_mip_bias`; its texels as wide as high: on a wall
  striped one way only, Metal's anisotropic filtering took the top mip with the bias or
  without it, and the test saw no difference). The shadow and rain maps, drawn at their own
  size, keep their textures' mips. `OMSI_NO_FSR_MIP_BIAS=1` leaves the bias out, to compare
  in the game. The standard upscaler is unchanged.
- **`OMSI_FSR_RAW_INPUT=1`** (to measure, off by default; not on OpenGL): EASU reads the scene
  through a view that is not sRGB, its values as stored - no square roots for its twelve taps -
  and the way out takes the exact sRGB curve once a pixel. Not tried on a GPU yet (the test
  adapter here is OpenGL): if `FSR1 EASU`'s time falls and the picture is the same, it becomes
  the default.
- **No anti-aliasing before FSR in Vanilla:** Vanilla's FXAA runs only at full size, so at a
  render scale FSR is handed an aliased picture and enlarges its stair-steps. Until a pass for
  it exists, use MSAA 2x with FSR (the measurements above did); Enhanced runs its FXAA on the
  smaller picture already.

### Open questions

- The default: 80 % on the new scale - to be confirmed by a test at 60 / 80 / 100 %.
- A non-sRGB view of the scene for EASU's twelve taps (no square roots at all) needs the scaled
  scene made with that view allowed, which OpenGL cannot do: left for a measurement that says the
  remaining square roots matter.
- Measurements on a newer card: FSR's cost is fixed per output pixel, the scene's grows with
  resolution; the trade may differ there.

## Technical design

- Source picture is tone mapped, then **EASU** writes into a cached full-sized target (`Rgb10a2Unorm`, perceptual values), or straight into the window when RCAS is skipped.
- Both passes work on a perceptual picture, as FSR 1 is made for: on an sRGB window format the texels read back linear, so they are taken to gamma 2.0 on the way in and back to linear on the way out (as AMD's sample does); on a format that is not sRGB the stored values are used as they are. Without it, EASU's edge test and RCAS's limits ran on linear light (more halos in the darks, uneven sharpening).
- **RCAS** reads the cached target and writes the final window image.
- HUD/overlay quads are drawn after RCAS at native resolution.
- Uses WGSL textureLoad (no textureGather requirement), no changes to the scene shaders, wgpu fork, multiplayer or game asset loading.
- The temporary image is cached by window resolution rather than allocated per frame; all FSR shader compilation is skipped unless explicitly enabled at launch.
- Existing AA/render scale settings remain in charge; for best image quality test with MSAA and avoid noisy inputs.

## Remaining validation

This is an experimental implementation and should **not** be merged until:
1. `cargo fmt --check`, `cargo test -p omsi-render --lib` and an application build pass.
2. WGSL pipeline construction is exercised on DX12, Vulkan, ANGLE and OpenGL, including an older GPU.
3. Visual A/B comparisons cover routes, distant text, wires, foliage, bus destination displays and nighttime lighting.
4. GPU timings and memory usage are compared at the **same internal resolution** against the old scaler, with MSAA and backend held fixed.
5. Verify resize, minimized/restored windows, and the runtime fallback when a driver rejects FSR (covered on the software adapter by `fsr1_turned_on_while_running_upscales_the_picture`).

No performance gain is claimed until tests are measured. FSR is spatial only; there is **no temporal upscaling or frame generation** here.
